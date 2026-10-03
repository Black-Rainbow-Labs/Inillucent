//! Finding where a column's name is written in stored schema text.
//!
//! Invariant: **a column reference is rewritten only when the name resolves to
//! the column being renamed.** A view or a trigger body can name a dozen
//! columns called `b` that belong to other tables, to subqueries and to common
//! table expressions, and `ALTER TABLE ... RENAME COLUMN` has to change the
//! ones that mean the renamed column and none of the others. The resolution here
//! follows the same scoping the binder uses: the innermost `SELECT` that has a
//! matching source wins, `new` and `old` mean the trigger's own table, and a
//! source this module cannot see into (a subquery, a common table expression)
//! stops an unqualified search instead of being guessed at.
//!
//! A name is replaced by token position, so whatever the author wrote around it
//! (comments, spacing, quoting of other names) comes back exactly as it was.

use inillucent_base::limits::Limits;
use inillucent_base::{error, DbResult};
use inillucent_sql::ast::{
    Ast, Expr, FromSource, FromTermId, SelectCoreId, SelectId, Statement as Parsed,
};
use inillucent_sql::lexer::{Lexer, Punctuator, Span, Token, TokenKind};
use inillucent_sql::parser::parse_next_statement;

use crate::rename::{bare_word, unquoted};
use crate::rename_table::lex_all;

/// A table the resolver may meet in a `FROM` clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownTable {
    /// The table's folded name.
    pub folded: Vec<u8>,
    /// The folded names of its columns, or `None` when they are not known (a
    /// view whose columns are only worked out by binding it).
    pub columns: Option<Vec<Vec<u8>>>,
}

/// The column being renamed.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    /// The folded name of the table that owns the column.
    pub table: &'a [u8],
    /// The folded name of the column.
    pub column: &'a [u8],
}

/// One `FROM` term (or the target of a write) that columns can be found in.
struct Term {
    exposed: Vec<u8>,
    table: Option<Vec<u8>>,
    columns: Option<Vec<Vec<u8>>>,
    scope: usize,
}

/// Resolves column references in one parsed statement.
struct Resolver<'a> {
    ast: &'a Ast,
    sql: &'a [u8],
    scopes: Vec<Span>,
    terms: Vec<Term>,
    trigger_table: Option<&'a [u8]>,
    target: Target<'a>,
}

/// Returns the spans of the target column's name in a view's text.
///
/// @param sql - the stored `CREATE VIEW` text
/// @param target - the column being renamed
/// @param known - the tables a `FROM` clause can name
pub fn view_edits(sql: &[u8], target: Target<'_>, known: &[KnownTable]) -> DbResult<Vec<Span>> {
    let parsed = parse_stored(sql, "CREATE VIEW")?;
    let resolver = Resolver::new(&parsed.ast, sql, Vec::new(), None, target);
    let mut resolver = resolver;
    resolver.add_from_terms(known, &[]);
    Ok(resolver.column_edits())
}

/// Returns the spans of the target column's name in a trigger's text.
///
/// @param sql - the stored `CREATE TRIGGER` text
/// @param trigger_table - the folded name of the table the trigger is on
/// @param target - the column being renamed
/// @param known - the tables a `FROM` clause can name
pub fn trigger_edits(
    sql: &[u8],
    trigger_table: &[u8],
    target: Target<'_>,
    known: &[KnownTable],
) -> DbResult<Vec<Span>> {
    let parsed = parse_stored(sql, "CREATE TRIGGER")?;
    let Parsed::CreateTrigger { body, .. } = &parsed.statement else {
        return Err(error::corrupt(
            "the schema SQL for this trigger is not a CREATE TRIGGER",
        ));
    };
    let tokens = lex_all(sql)?;
    let mut ranges = body_ranges(sql, &tokens);
    if ranges.len() != body.len() {
        ranges.clear();
    }
    let mut resolver = Resolver::new(
        &parsed.ast,
        sql,
        ranges.clone(),
        Some(trigger_table),
        target,
    );
    resolver.add_from_terms(known, body);
    let mut edits = resolver.column_edits();
    if trigger_table == target.table {
        edits.extend(update_of_edits(sql, &tokens, target.column));
    }
    for (statement, range) in body.iter().zip(ranges.iter()) {
        edits.extend(write_target_edits(&resolver, &tokens, statement, *range));
    }
    edits.sort_by_key(|span| span.start);
    edits.dedup();
    Ok(edits)
}

/// Returns the spans of the target column's name in the text of an object that
/// belongs to the target's own table: its `CREATE TABLE` or one of its indexes.
///
/// A `REFERENCES other (...)` list names the other table's columns and is left
/// alone; a `REFERENCES this (...)` list is a self reference and is rewritten.
///
/// @param sql - the stored text
/// @param target - the column being renamed
pub fn own_text_edits(sql: &[u8], target: Target<'_>) -> DbResult<Vec<Span>> {
    let tokens = lex_all(sql)?;
    let mut edits = Vec::new();
    let mut at = 0usize;
    while at < tokens.len() {
        if word_at(sql, &tokens, at).as_deref() == Some(b"references") {
            at = reference_list(sql, &tokens, at, target, &mut edits);
            continue;
        }
        if is_column_position(sql, &tokens, at, target) {
            if let Some(token) = tokens.get(at) {
                edits.push(token.span);
            }
        }
        at = at.saturating_add(1);
    }
    Ok(edits)
}

/// Returns the spans of the target column's name in the `REFERENCES` clauses
/// of a table other than the target's own.
///
/// @param sql - the stored `CREATE TABLE` text of the child
/// @param target - the column being renamed
pub fn reference_edits(sql: &[u8], target: Target<'_>) -> DbResult<Vec<Span>> {
    let tokens = lex_all(sql)?;
    let mut edits = Vec::new();
    let mut at = 0usize;
    while at < tokens.len() {
        if word_at(sql, &tokens, at).as_deref() == Some(b"references") {
            at = reference_list(sql, &tokens, at, target, &mut edits);
            continue;
        }
        at = at.saturating_add(1);
    }
    Ok(edits)
}

/// Parses stored text, reporting a failure as an unreadable schema.
///
/// @param sql - the stored text
/// @param what - the statement kind, for the message
fn parse_stored(sql: &[u8], what: &str) -> DbResult<inillucent_sql::parser::ParsedStatement> {
    parse_next_statement(sql, 0, &Limits::default())
        .map_err(|reason| crate::load::unparseable_schema(what, reason))
}

/// Returns the lowercase bare word at a token position.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param at - the position
fn word_at(sql: &[u8], tokens: &[Token], at: usize) -> Option<Vec<u8>> {
    tokens.get(at).and_then(|token| bare_word(sql, *token))
}

/// Returns the folded name a name token spells, when it is a name.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param at - the position
fn name_at(sql: &[u8], tokens: &[Token], at: usize) -> Option<Vec<u8>> {
    let token = tokens.get(at)?;
    match token.kind {
        TokenKind::Identifier { keyword, quote } => {
            let bare = quote == inillucent_sql::lexer::QuoteForm::Bare;
            if bare && keyword.is_some_and(|word| !word.may_be_name()) {
                return None;
            }
            Some(unquoted(token.span.slice(sql)).to_ascii_lowercase())
        }
        _ => None,
    }
}

/// Returns whether a token position is a punctuator of the given kind.
///
/// @param tokens - the tokens
/// @param at - the position, which may be out of range
/// @param punctuator - what it should be
fn is_punct(tokens: &[Token], at: Option<usize>, punctuator: Punctuator) -> bool {
    at.and_then(|index| tokens.get(index))
        .is_some_and(|token| token.is(punctuator))
}

/// Returns whether the name token at a position is the target column in a
/// place where a column of the target's own table is written.
///
/// A name that follows another plain name is a type name (`a INTEGER`,
/// `x DOUBLE PRECISION`) and not a column. A qualified name counts only when
/// the qualifier is the target's table.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param at - the position
/// @param target - the column being renamed
fn is_column_position(sql: &[u8], tokens: &[Token], at: usize, target: Target<'_>) -> bool {
    if name_at(sql, tokens, at).as_deref() != Some(target.column) {
        return false;
    }
    let before = at.checked_sub(1);
    if is_punct(tokens, before, Punctuator::Dot) {
        let qualifier = at
            .checked_sub(2)
            .and_then(|index| name_at(sql, tokens, index));
        return qualifier.as_deref() == Some(target.table);
    }
    if let Some(previous) = before.and_then(|index| tokens.get(index)) {
        if let TokenKind::Identifier { keyword: None, .. } = previous.kind {
            return false;
        }
        let word = bare_word(sql, *previous).unwrap_or_default();
        if matches!(
            word.as_slice(),
            b"table"
                | b"index"
                | b"view"
                | b"trigger"
                | b"as"
                | b"exists"
                | b"constraint"
                | b"collate"
        ) {
            return false;
        }
    }
    before.is_some()
}

/// Handles one `REFERENCES parent (columns)` clause and returns where to resume.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param at - the position of the `REFERENCES` word
/// @param target - the column being renamed
/// @param edits - receives the spans to rewrite
fn reference_list(
    sql: &[u8],
    tokens: &[Token],
    at: usize,
    target: Target<'_>,
    edits: &mut Vec<Span>,
) -> usize {
    let mut parent = at.saturating_add(1);
    let mut name = name_at(sql, tokens, parent);
    if is_punct(tokens, parent.checked_add(1), Punctuator::Dot) {
        parent = parent.saturating_add(2);
        name = name_at(sql, tokens, parent);
    }
    let open = parent.saturating_add(1);
    if !is_punct(tokens, Some(open), Punctuator::LeftParen) {
        return open;
    }
    let mut index = open.saturating_add(1);
    while let Some(token) = tokens.get(index) {
        if token.is(Punctuator::RightParen) {
            break;
        }
        let ours = name.as_deref() == Some(target.table);
        if ours && name_at(sql, tokens, index).as_deref() == Some(target.column) {
            edits.push(token.span);
        }
        index = index.saturating_add(1);
    }
    index.saturating_add(1)
}

/// Returns the spans of the columns in a trigger's `UPDATE OF a, b` list.
///
/// @param sql - the stored text
/// @param tokens - its tokens
/// @param column - the folded column name
fn update_of_edits(sql: &[u8], tokens: &[Token], column: &[u8]) -> Vec<Span> {
    let mut edits = Vec::new();
    let Some(of) = (0..tokens.len().saturating_sub(1)).find(|index| {
        word_at(sql, tokens, *index).as_deref() == Some(b"update")
            && word_at(sql, tokens, index.saturating_add(1)).as_deref() == Some(b"of")
    }) else {
        return edits;
    };
    let mut index = of.saturating_add(2);
    while let Some(token) = tokens.get(index) {
        if word_at(sql, tokens, index).as_deref() == Some(b"on") {
            break;
        }
        if name_at(sql, tokens, index).as_deref() == Some(column) {
            edits.push(token.span);
        }
        index = index.saturating_add(1);
    }
    edits
}

/// Returns the span of each body statement of a trigger.
///
/// The statements sit between `BEGIN` and the final `END` and end at a
/// semicolon outside any parenthesis.
///
/// @param sql - the stored text
/// @param tokens - its tokens
fn body_ranges(sql: &[u8], tokens: &[Token]) -> Vec<Span> {
    let Some(begin) =
        (0..tokens.len()).find(|index| word_at(sql, tokens, *index).as_deref() == Some(b"begin"))
    else {
        return Vec::new();
    };
    let end = tokens.len().saturating_sub(1);
    let mut ranges = Vec::new();
    let mut open: Option<Span> = None;
    let mut depth = 0usize;
    for token in tokens.get(begin.saturating_add(1)..end).unwrap_or(&[]) {
        match token.kind {
            TokenKind::Punctuator(Punctuator::LeftParen) => depth = depth.saturating_add(1),
            TokenKind::Punctuator(Punctuator::RightParen) => depth = depth.saturating_sub(1),
            TokenKind::Punctuator(Punctuator::Semicolon) if depth == 0 => {
                if let Some(range) = open.take() {
                    ranges.push(range);
                }
                continue;
            }
            _ => {}
        }
        open = Some(match open {
            Some(range) => range.to(token.span),
            None => token.span,
        });
    }
    if let Some(range) = open {
        ranges.push(range);
    }
    ranges
}

/// Returns the spans of target column names written as the targets of a write.
///
/// `UPDATE t SET b = 1`, `INSERT INTO t (b) ...` and the `ON CONFLICT` clauses
/// of an upsert name columns of the table being written without an expression
/// around them, so the resolver never sees them.
///
/// @param resolver - the resolver for the trigger
/// @param tokens - the trigger's tokens
/// @param statement - one body statement
/// @param range - where it is written
fn write_target_edits(
    resolver: &Resolver<'_>,
    tokens: &[Token],
    statement: &Parsed,
    range: Span,
) -> Vec<Span> {
    let table = match statement {
        Parsed::Update(update) => resolver.target_of_term(update.target),
        Parsed::Insert(insert) => Some(resolver.ast.folded(insert.table).to_vec()),
        _ => None,
    };
    if table.as_deref() != Some(resolver.target.table) {
        return Vec::new();
    }
    let inside: Vec<usize> = (0..tokens.len())
        .filter(|index| {
            tokens
                .get(*index)
                .is_some_and(|token| token.span.start >= range.start && token.span.end <= range.end)
        })
        .collect();
    let mut edits = Vec::new();
    let sql = resolver.sql;
    let column = resolver.target.column;
    for index in inside {
        let word = word_at(sql, tokens, index).unwrap_or_default();
        match word.as_slice() {
            b"set" => {
                assignment_targets(sql, tokens, index, column, &mut edits);
            }
            b"into" if matches!(statement, Parsed::Insert(_)) => {
                insert_columns(sql, tokens, index, column, &mut edits);
            }
            b"conflict" => {
                ident_list_after(sql, tokens, index, column, &mut edits);
            }
            _ => {}
        }
    }
    edits
}

/// Collects the assignment targets after a `SET` word.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param set - the position of the `SET` word
/// @param column - the folded column name
/// @param edits - receives the spans
fn assignment_targets(
    sql: &[u8],
    tokens: &[Token],
    set: usize,
    column: &[u8],
    edits: &mut Vec<Span>,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut expect_target = true;
    let mut index = set.saturating_add(1);
    while let Some(token) = tokens.get(index) {
        match token.kind {
            TokenKind::Punctuator(Punctuator::LeftParen) => depth = depth.saturating_add(1),
            TokenKind::Punctuator(Punctuator::RightParen) => depth = depth.saturating_sub(1),
            TokenKind::Punctuator(Punctuator::Semicolon) if depth == 0 => break,
            TokenKind::Punctuator(Punctuator::Comma) if depth == 0 => expect_target = true,
            _ => {}
        }
        let word = word_at(sql, tokens, index).unwrap_or_default();
        if depth == 0
            && matches!(
                word.as_slice(),
                b"where" | b"from" | b"returning" | b"order" | b"limit"
            )
        {
            break;
        }
        let in_target_group = depth == 1 && expect_target_group(tokens, index);
        if (expect_target && depth == 0 || in_target_group)
            && name_at(sql, tokens, index).as_deref() == Some(column)
            && !is_punct(tokens, index.checked_add(1), Punctuator::Dot)
        {
            edits.push(token.span);
        }
        if depth == 0 && name_at(sql, tokens, index).is_some() {
            expect_target = false;
        }
        index = index.saturating_add(1);
    }
    Some(index)
}

/// Returns whether a position is inside the `(a, b) = ...` form of an assignment.
///
/// @param tokens - the tokens
/// @param index - the position of a name inside parentheses
fn expect_target_group(tokens: &[Token], index: usize) -> bool {
    let mut at = index;
    while let Some(previous) = at.checked_sub(1) {
        match tokens.get(previous).map(|token| token.kind) {
            Some(TokenKind::Punctuator(Punctuator::LeftParen)) => {
                let before = previous.checked_sub(1).and_then(|p| tokens.get(p));
                return before.is_some_and(|token| {
                    token.is(Punctuator::Comma)
                        || matches!(
                            token.kind,
                            TokenKind::Identifier {
                                keyword: Some(_),
                                ..
                            }
                        )
                });
            }
            Some(TokenKind::Punctuator(Punctuator::Comma)) => at = previous,
            Some(TokenKind::Identifier { .. }) => at = previous,
            _ => return false,
        }
    }
    false
}

/// Collects the column list of `INSERT INTO t (a, b)`.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param into - the position of the `INTO` word
/// @param column - the folded column name
/// @param edits - receives the spans
fn insert_columns(
    sql: &[u8],
    tokens: &[Token],
    into: usize,
    column: &[u8],
    edits: &mut Vec<Span>,
) -> Option<usize> {
    let mut index = into.saturating_add(1);
    if is_punct(tokens, index.checked_add(1), Punctuator::Dot) {
        index = index.saturating_add(2);
    }
    index = index.saturating_add(1);
    if word_at(sql, tokens, index).as_deref() == Some(b"as") {
        index = index.saturating_add(2);
    }
    if !is_punct(tokens, Some(index), Punctuator::LeftParen) {
        return None;
    }
    ident_list_after(sql, tokens, index.saturating_sub(1), column, edits)
}

/// Collects the names in the parenthesised list that follows a position.
///
/// @param sql - the text
/// @param tokens - its tokens
/// @param before - the position just before the opening parenthesis
/// @param column - the folded column name
/// @param edits - receives the spans
fn ident_list_after(
    sql: &[u8],
    tokens: &[Token],
    before: usize,
    column: &[u8],
    edits: &mut Vec<Span>,
) -> Option<usize> {
    let open = before.saturating_add(1);
    if !is_punct(tokens, Some(open), Punctuator::LeftParen) {
        return None;
    }
    let mut index = open.saturating_add(1);
    while let Some(token) = tokens.get(index) {
        if token.is(Punctuator::RightParen) {
            break;
        }
        let after_separator = is_punct(tokens, index.checked_sub(1), Punctuator::LeftParen)
            || is_punct(tokens, index.checked_sub(1), Punctuator::Comma);
        if after_separator && name_at(sql, tokens, index).as_deref() == Some(column) {
            edits.push(token.span);
        }
        index = index.saturating_add(1);
    }
    Some(index)
}

impl<'a> Resolver<'a> {
    /// Builds a resolver with the scopes of one parsed statement.
    ///
    /// @param ast - the statement's arena
    /// @param sql - the stored text the arena was parsed from
    /// @param statements - the spans of a trigger's body statements
    /// @param trigger_table - the trigger's table, when it is a trigger
    /// @param target - the column being renamed
    fn new(
        ast: &'a Ast,
        sql: &'a [u8],
        statements: Vec<Span>,
        trigger_table: Option<&'a [u8]>,
        target: Target<'a>,
    ) -> Resolver<'a> {
        let mut scopes: Vec<Span> = (0..ast.core_count())
            .filter_map(|index| ast.core(SelectCoreId(index as u32)).map(|core| core.span))
            .collect();
        scopes.extend(statements);
        Resolver {
            ast,
            sql,
            scopes,
            terms: Vec::new(),
            trigger_table,
            target,
        }
    }

    /// Returns the index of the smallest scope that contains a span.
    ///
    /// @param span - the span to place
    fn scope_of(&self, span: Span) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.start <= span.start && span.end <= scope.end)
            .min_by_key(|(_, scope)| scope.len())
            .map(|(index, _)| index)
    }

    /// Returns the names of every common table expression in the statement.
    fn cte_names(&self) -> Vec<Vec<u8>> {
        let mut names = Vec::new();
        for index in 0..self.ast.select_count() {
            if let Some(select) = self.ast.select(SelectId(index as u32)) {
                for cte in &select.with.ctes {
                    names.push(self.ast.folded(cte.name).to_vec());
                }
            }
        }
        names
    }

    /// Returns the folded base table a `FROM` term names, when it names one.
    ///
    /// @param term - the term
    fn target_of_term(&self, term: FromTermId) -> Option<Vec<u8>> {
        match &self.ast.from_term(term)?.source {
            FromSource::Table { name, .. } => Some(self.ast.folded(*name).to_vec()),
            _ => None,
        }
    }

    /// Records every `FROM` term, and the target of every `INSERT`, as a term.
    ///
    /// @param known - the tables a `FROM` clause can name
    /// @param body - a trigger's body statements, which may insert
    fn add_from_terms(&mut self, known: &[KnownTable], body: &[Parsed]) {
        let ctes = self.cte_names();
        let lookup = |folded: &[u8]| -> Option<Vec<Vec<u8>>> {
            known
                .iter()
                .find(|table| table.folded == folded)
                .and_then(|table| table.columns.clone())
        };
        let mut terms = Vec::new();
        for index in 0..self.ast.from_term_count() {
            let Some(term) = self.ast.from_term(FromTermId(index as u32)) else {
                continue;
            };
            let scope = self.scope_of(term.span).unwrap_or(usize::MAX);
            let alias = term.alias.map(|name| self.ast.folded(name).to_vec());
            match &term.source {
                FromSource::Table {
                    name, arguments, ..
                } => {
                    let folded = self.ast.folded(*name).to_vec();
                    let opaque = arguments.is_some() || ctes.contains(&folded);
                    terms.push(Term {
                        exposed: alias.unwrap_or_else(|| folded.clone()),
                        columns: if opaque { None } else { lookup(&folded) },
                        table: (!opaque).then_some(folded),
                        scope,
                    });
                }
                FromSource::Subquery(_) => terms.push(Term {
                    exposed: alias.unwrap_or_default(),
                    table: None,
                    columns: None,
                    scope,
                }),
                FromSource::Join(_) => {}
            }
        }
        let statements_start = self.ast.core_count();
        let ranges = self.scopes.len().saturating_sub(statements_start);
        if ranges == body.len() {
            for (offset, statement) in body.iter().enumerate() {
                let Parsed::Insert(insert) = statement else {
                    continue;
                };
                let folded = self.ast.folded(insert.table).to_vec();
                let alias = insert.alias.map(|name| self.ast.folded(name).to_vec());
                let scope = statements_start.saturating_add(offset);
                // `excluded` in an upsert is the row the insert proposed, which
                // has the target table's columns.
                terms.push(Term {
                    exposed: b"excluded".to_vec(),
                    columns: lookup(&folded),
                    table: Some(folded.clone()),
                    scope,
                });
                terms.push(Term {
                    exposed: alias.unwrap_or_else(|| folded.clone()),
                    columns: lookup(&folded),
                    table: Some(folded),
                    scope,
                });
            }
        }
        self.terms = terms;
    }

    /// Returns the scopes that contain a span, innermost first.
    ///
    /// An expression of an `ORDER BY` or a `LIMIT` is written after its
    /// `SELECT` arm and outside the arm's span, so a span that no arm contains
    /// is placed in the first arm of the smallest `SELECT` around it. That is
    /// also where a compound `SELECT` resolves its `ORDER BY`: against the
    /// result columns of the leftmost arm.
    ///
    /// @param span - where the reference is written
    fn chain(&self, span: Span) -> Vec<usize> {
        let mut anchor = span;
        let contains = |outer: Span| outer.start <= span.start && span.end <= outer.end;
        if self.scope_of(span).is_none_or(|index| {
            self.scopes
                .get(index)
                .is_some_and(|scope| !contains(*scope))
        }) {
            let select = (0..self.ast.select_count())
                .filter_map(|index| self.ast.select(SelectId(index as u32)))
                .filter(|select| contains(select.span))
                .min_by_key(|select| select.span.len());
            if let Some(core) = select.and_then(|select| self.ast.core(select.first)) {
                anchor = core.span;
            }
        }
        let mut chain: Vec<usize> = self
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.start <= anchor.start && anchor.end <= scope.end)
            .map(|(index, _)| index)
            .collect();
        chain.sort_by_key(|index| self.scopes.get(*index).map_or(0, |scope| scope.len()));
        chain
    }

    /// Returns whether a reference means the column being renamed.
    ///
    /// @param qualifier - the folded table or alias written before the column
    /// @param column - the folded column name
    /// @param span - where the reference is written
    fn resolves_to_target(&self, qualifier: Option<&[u8]>, column: &[u8], span: Span) -> bool {
        if column != self.target.column {
            return false;
        }
        if let (Some(trigger_table), Some(q)) = (self.trigger_table, qualifier) {
            if q == b"new" || q == b"old" {
                return trigger_table == self.target.table;
            }
        }
        for scope in self.chain(span) {
            let here: Vec<&Term> = self
                .terms
                .iter()
                .filter(|term| term.scope == scope)
                .collect();
            if let Some(q) = qualifier {
                if let Some(term) = here.iter().find(|term| term.exposed == q) {
                    return term.table.as_deref() == Some(self.target.table);
                }
                continue;
            }
            let with: Vec<&&Term> = here
                .iter()
                .filter(|term| {
                    term.columns
                        .as_ref()
                        .is_some_and(|columns| columns.iter().any(|name| name == column))
                })
                .collect();
            if !with.is_empty() {
                return with.len() == 1
                    && with.first().and_then(|term| term.table.as_deref())
                        == Some(self.target.table);
            }
            if here.iter().any(|term| term.columns.is_none()) {
                return false;
            }
        }
        false
    }

    /// Returns the span of every column reference that means the target.
    fn column_edits(&self) -> Vec<Span> {
        let mut edits = Vec::new();
        for index in 0..self.ast.expr_count() {
            let id = inillucent_sql::ast::ExprId(index as u32);
            let Some(Expr::Column { table, column, .. }) = self.ast.expr(id) else {
                continue;
            };
            let span = self.ast.expr_span(id);
            let qualifier = table.map(|name| self.ast.folded(name).to_vec());
            let folded = self.ast.folded(*column);
            if self.resolves_to_target(qualifier.as_deref(), folded, span) {
                if let Some(name) = last_name_in(self.sql, span) {
                    edits.push(name);
                }
            }
        }
        edits
    }
}

/// Returns the span of the last name token inside a span.
///
/// @param sql - the text
/// @param span - the span, which is a column reference
fn last_name_in(sql: &[u8], span: Span) -> Option<Span> {
    let mut lexer = Lexer::at(sql, span.start as usize);
    let mut last = None;
    while let Ok(token) = lexer.next_token() {
        if token.kind == TokenKind::EndOfInput || token.span.end > span.end {
            break;
        }
        if matches!(token.kind, TokenKind::Identifier { .. }) {
            last = Some(token.span);
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the tables the tests use: `t(a, b, c)` and `u(b, z)`.
    fn known() -> Vec<KnownTable> {
        let columns =
            |names: &[&str]| Some(names.iter().map(|name| name.as_bytes().to_vec()).collect());
        vec![
            KnownTable {
                folded: b"t".to_vec(),
                columns: columns(&["a", "b", "c"]),
            },
            KnownTable {
                folded: b"u".to_vec(),
                columns: columns(&["b", "z"]),
            },
        ]
    }

    /// Returns the text each edit covers.
    fn texts(sql: &str, spans: Vec<Span>) -> Vec<String> {
        spans
            .iter()
            .map(|span| String::from_utf8_lossy(span.slice(sql.as_bytes())).into_owned())
            .collect()
    }

    /// A join keeps the other table's column and renames this table's.
    #[test]
    fn a_join_renames_only_the_target_tables_column() {
        let sql = "CREATE VIEW v AS SELECT t.b, z FROM t, u WHERE t.b = u.b AND z > 0";
        let target = Target {
            table: b"t",
            column: b"b",
        };
        let edits = view_edits(sql.as_bytes(), target, &known()).expect("it resolves");
        assert_eq!(edits.len(), 2);
    }

    /// A subquery's own `b` is resolved inside it.
    #[test]
    fn a_correlated_subquery_keeps_its_own_column() {
        let sql = "CREATE VIEW v AS SELECT (SELECT max(b) FROM u WHERE u.b = t.b) FROM t";
        let target = Target {
            table: b"t",
            column: b"b",
        };
        let edits = view_edits(sql.as_bytes(), target, &known()).expect("it resolves");
        assert_eq!(texts(sql, edits), vec!["b".to_string()]);
    }
}
