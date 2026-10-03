//! Editing the constraints in a stored `CREATE TABLE` text.
//!
//! Invariant: **an edit changes the bytes of one constraint and nothing else.**
//! The text is what a person wrote, comments and spacing included, and SQLite's
//! `ALTER TABLE ... SET NOT NULL`, `DROP NOT NULL`, `ADD CHECK` and `DROP
//! CONSTRAINT` give it back changed only there, with a separating space kept or
//! dropped in the same cases SQLite does. The edits are made on tokens, so a
//! comment that holds the word `NOT NULL` is never mistaken for a constraint.

use inillucent_base::DbResult;
use inillucent_sql::lexer::{Punctuator, Span, Token, TokenKind};

use crate::rename::{bare_word, unquoted};
use crate::rename_table::lex_all;

/// The kinds of column constraint the edits tell apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    NotNull,
    Check,
    Default,
    Collate,
    Other,
}

/// One column constraint, as token positions.
#[derive(Clone, Copy, Debug)]
struct Constraint {
    /// The first token, which is `CONSTRAINT` when the constraint is named.
    first: usize,
    /// The last token.
    last: usize,
    kind: Kind,
    /// The token that spells the name, when it has one.
    name: Option<usize>,
}

/// One comma separated element of the column list.
#[derive(Clone, Copy, Debug)]
struct Element {
    first: usize,
    last: usize,
    /// Whether it is a table constraint rather than a column.
    table_level: bool,
}

/// What `DROP CONSTRAINT` found.
#[derive(Debug, PartialEq, Eq)]
pub enum Dropped {
    /// The new text of the table.
    Text(Vec<u8>),
    /// No constraint has that name.
    Missing,
    /// The constraint exists and SQLite does not let it be dropped.
    Refused,
}

/// The tokens of a stored statement and what is needed to read them.
struct Text<'a> {
    sql: &'a [u8],
    tokens: Vec<Token>,
}

/// Returns the folded names of every named constraint in a `CREATE TABLE`.
///
/// @param sql - the stored text
pub fn constraint_names(sql: &[u8]) -> DbResult<Vec<Vec<u8>>> {
    let text = Text::new(sql)?;
    let mut names = Vec::new();
    for element in text.elements() {
        match element.table_level {
            true => {
                if text.word(element.first).as_deref() == Some(b"constraint") {
                    names.extend(text.folded_name(element.first.saturating_add(1)));
                }
            }
            false => {
                for constraint in text.column_constraints(element) {
                    names.extend(constraint.name.and_then(|at| text.folded_name(at)));
                }
            }
        }
    }
    Ok(names)
}

/// Returns the text with `NOT NULL` added to a column, or `None` when the column has it.
///
/// The clause goes in just before the comma or parenthesis that ends the
/// column's definition, so whatever spacing and comments the definition ended
/// with stay before it.
///
/// @param sql - the stored text
/// @param position - the column's declared position
/// @param clause - `NOT NULL` and any `ON CONFLICT`, as the statement wrote it
pub fn set_not_null(sql: &[u8], position: usize, clause: &[u8]) -> DbResult<Option<Vec<u8>>> {
    let text = Text::new(sql)?;
    let Some(element) = text.column(position) else {
        return Ok(None);
    };
    if text
        .column_constraints(element)
        .iter()
        .any(|constraint| constraint.kind == Kind::NotNull)
    {
        return Ok(None);
    }
    let at = text.start_after(element.last);
    let mut out = Vec::with_capacity(sql.len().saturating_add(clause.len()).saturating_add(1));
    out.extend_from_slice(sql.get(..at).unwrap_or(&[]));
    out.push(b' ');
    out.extend_from_slice(clause);
    out.extend_from_slice(sql.get(at..).unwrap_or(&[]));
    Ok(Some(out))
}

/// Returns the text with a column's first `NOT NULL` removed, or `None` when it has none.
///
/// @param sql - the stored text
/// @param position - the column's declared position
pub fn drop_not_null(sql: &[u8], position: usize) -> DbResult<Option<Vec<u8>>> {
    let text = Text::new(sql)?;
    let Some(element) = text.column(position) else {
        return Ok(None);
    };
    let found = text
        .column_constraints(element)
        .into_iter()
        .find(|constraint| constraint.kind == Kind::NotNull);
    Ok(found.map(|constraint| text.cut_constraint(constraint)))
}

/// Returns the text with the first constraint of a name removed.
///
/// A `CHECK` or `NOT NULL` is removed with the name. A `DEFAULT` or `COLLATE`
/// stays and only loses the name in front of it, and the keys and references
/// (`PRIMARY KEY`, `UNIQUE`, `FOREIGN KEY`, `REFERENCES`) cannot be dropped.
///
/// @param sql - the stored text
/// @param folded - the folded name
pub fn drop_constraint(sql: &[u8], folded: &[u8]) -> DbResult<Dropped> {
    let text = Text::new(sql)?;
    let elements = text.elements();
    for (at, element) in elements.iter().enumerate() {
        if element.table_level {
            let named = text.word(element.first).as_deref() == Some(b"constraint")
                && text.folded_name(element.first.saturating_add(1)).as_deref() == Some(folded);
            if !named {
                continue;
            }
            let kind_at = element.first.saturating_add(2);
            if text.word(kind_at).as_deref() != Some(b"check") {
                return Ok(Dropped::Refused);
            }
            return Ok(Dropped::Text(text.cut_table_element(&elements, at)));
        }
        for constraint in text.column_constraints(*element) {
            let named = constraint.name.and_then(|name| text.folded_name(name));
            if named.as_deref() != Some(folded) {
                continue;
            }
            return Ok(match constraint.kind {
                Kind::NotNull | Kind::Check => Dropped::Text(text.cut_constraint(constraint)),
                Kind::Default | Kind::Collate => Dropped::Text(text.cut_label(constraint)),
                Kind::Other => Dropped::Refused,
            });
        }
    }
    Ok(Dropped::Missing)
}

impl<'a> Text<'a> {
    /// Lexes a stored statement.
    ///
    /// @param sql - the stored text
    fn new(sql: &'a [u8]) -> DbResult<Text<'a>> {
        Ok(Text {
            sql,
            tokens: lex_all(sql)?,
        })
    }

    /// Returns the lowercase bare word at a token position.
    ///
    /// @param at - the position
    fn word(&self, at: usize) -> Option<Vec<u8>> {
        self.tokens
            .get(at)
            .and_then(|token| bare_word(self.sql, *token))
    }

    /// Returns whether the token at a position is a punctuator.
    ///
    /// @param at - the position
    /// @param punctuator - what it should be
    fn is(&self, at: usize, punctuator: Punctuator) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.is(punctuator))
    }

    /// Returns the folded, unquoted name a token spells.
    ///
    /// @param at - the position
    fn folded_name(&self, at: usize) -> Option<Vec<u8>> {
        let token = self.tokens.get(at)?;
        match token.kind {
            TokenKind::Identifier { .. } | TokenKind::String => {
                let raw = token.span.slice(self.sql);
                let unquoted = match token.kind {
                    TokenKind::String => raw
                        .get(1..raw.len().saturating_sub(1))
                        .unwrap_or(&[])
                        .to_vec(),
                    _ => unquoted(raw),
                };
                Some(unquoted.to_ascii_lowercase())
            }
            _ => None,
        }
    }

    /// Returns the byte offset where a token starts, or the end of the text.
    ///
    /// @param at - the position
    fn start_of(&self, at: usize) -> usize {
        self.tokens
            .get(at)
            .map_or(self.sql.len(), |token| token.span.start as usize)
    }

    /// Returns the byte offset where the token after a position starts.
    ///
    /// @param at - the position
    fn start_after(&self, at: usize) -> usize {
        self.start_of(at.saturating_add(1))
    }

    /// Returns the byte offset where the token before a position ends.
    ///
    /// @param at - the position
    fn end_before(&self, at: usize) -> usize {
        at.checked_sub(1)
            .and_then(|previous| self.tokens.get(previous))
            .map_or(0, |token| token.span.end as usize)
    }

    /// Splits the column list into its comma separated elements.
    fn elements(&self) -> Vec<Element> {
        let Some(open) = (0..self.tokens.len()).find(|at| self.is(*at, Punctuator::LeftParen))
        else {
            return Vec::new();
        };
        let mut elements = Vec::new();
        let mut depth = 0usize;
        let mut first = open.saturating_add(1);
        for at in open.saturating_add(1)..self.tokens.len() {
            let ends = match self.tokens.get(at).map(|token| token.kind) {
                Some(TokenKind::Punctuator(Punctuator::LeftParen)) => {
                    depth = depth.saturating_add(1);
                    false
                }
                Some(TokenKind::Punctuator(Punctuator::RightParen)) => {
                    if depth == 0 {
                        self.push_element(&mut elements, first, at);
                        return elements;
                    }
                    depth = depth.saturating_sub(1);
                    false
                }
                Some(TokenKind::Punctuator(Punctuator::Comma)) => depth == 0,
                _ => false,
            };
            if ends {
                self.push_element(&mut elements, first, at);
                first = at.saturating_add(1);
            }
        }
        elements
    }

    /// Records the element that fills the tokens from `first` up to `end`.
    ///
    /// @param elements - receives the element
    /// @param first - the position of its first token
    /// @param end - the position of the comma or parenthesis after it
    fn push_element(&self, elements: &mut Vec<Element>, first: usize, end: usize) {
        if end <= first {
            return;
        }
        let lead = match self.word(first).as_deref() {
            Some(b"constraint") => self.word(first.saturating_add(2)),
            other => other.map(<[u8]>::to_vec),
        };
        let table_level = matches!(
            lead.as_deref(),
            Some(b"primary") | Some(b"unique") | Some(b"check") | Some(b"foreign")
        );
        elements.push(Element {
            first,
            last: end.saturating_sub(1),
            table_level,
        });
    }

    /// Returns the element of the column at a declared position.
    ///
    /// @param position - the position
    fn column(&self, position: usize) -> Option<Element> {
        self.elements()
            .into_iter()
            .filter(|element| !element.table_level)
            .nth(position)
    }

    /// Lists the constraints written in one column's definition.
    ///
    /// @param element - the column's element
    fn column_constraints(&self, element: Element) -> Vec<Constraint> {
        let mut found = Vec::new();
        let mut at = element.first.saturating_add(1);
        let mut depth = 0usize;
        while at <= element.last {
            if self.is(at, Punctuator::LeftParen) {
                depth = depth.saturating_add(1);
            } else if self.is(at, Punctuator::RightParen) {
                depth = depth.saturating_sub(1);
            } else if depth == 0 {
                if let Some(constraint) = self.constraint_at(at, element.last) {
                    at = constraint.last;
                    found.push(constraint);
                }
            }
            at = at.saturating_add(1);
        }
        found
    }

    /// Reads the constraint that starts at a position, when one does.
    ///
    /// @param at - the position
    /// @param limit - the last position of the element
    fn constraint_at(&self, at: usize, limit: usize) -> Option<Constraint> {
        let word = self.word(at)?;
        let (body, name) = match word.as_slice() {
            b"constraint" => (at.saturating_add(2), Some(at.saturating_add(1))),
            _ => (at, None),
        };
        let kind_word = self.word(body)?;
        let (kind, last) = match kind_word.as_slice() {
            b"not" if self.word(body.saturating_add(1)).as_deref() == Some(b"null") => (
                Kind::NotNull,
                self.after_conflict(body.saturating_add(1), limit),
            ),
            b"null" => (Kind::Other, body),
            b"unique" => (Kind::Other, self.after_conflict(body, limit)),
            b"primary" => (Kind::Other, self.primary_key_end(body, limit)),
            b"check" => (Kind::Check, self.group_end(body.saturating_add(1))),
            b"default" => (Kind::Default, self.default_end(body, limit)),
            b"collate" => (Kind::Collate, body.saturating_add(1)),
            b"references" => (Kind::Other, self.references_end(body, limit)),
            b"generated" | b"as" => (Kind::Other, self.generated_end(body, limit)),
            _ => return None,
        };
        Some(Constraint {
            first: at,
            last: last.min(limit),
            kind,
            name,
        })
    }

    /// Returns the position of the parenthesis that closes the group at `open`.
    ///
    /// @param open - the position of the opening parenthesis
    fn group_end(&self, open: usize) -> usize {
        let mut depth = 0usize;
        for at in open..self.tokens.len() {
            if self.is(at, Punctuator::LeftParen) {
                depth = depth.saturating_add(1);
            } else if self.is(at, Punctuator::RightParen) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at;
                }
            }
        }
        open
    }

    /// Returns the last position of a constraint that may end in `ON CONFLICT x`.
    ///
    /// @param last - the position of the constraint's last token so far
    /// @param limit - the last position of the element
    fn after_conflict(&self, last: usize, limit: usize) -> usize {
        let next = last.saturating_add(1);
        let on = self.word(next).as_deref() == Some(b"on");
        let conflict = self.word(next.saturating_add(1)).as_deref() == Some(b"conflict");
        match on && conflict && next.saturating_add(2) <= limit {
            true => next.saturating_add(2),
            false => last,
        }
    }

    /// Returns the last position of `PRIMARY KEY [ASC|DESC] [ON CONFLICT x] [AUTOINCREMENT]`.
    ///
    /// @param primary - the position of `PRIMARY`
    /// @param limit - the last position of the element
    fn primary_key_end(&self, primary: usize, limit: usize) -> usize {
        let mut last = primary.saturating_add(1);
        if matches!(
            self.word(last.saturating_add(1)).as_deref(),
            Some(b"asc") | Some(b"desc")
        ) {
            last = last.saturating_add(1);
        }
        last = self.after_conflict(last, limit);
        if self.word(last.saturating_add(1)).as_deref() == Some(b"autoincrement") {
            last = last.saturating_add(1);
        }
        last
    }

    /// Returns the last position of a `DEFAULT` and its value.
    ///
    /// @param default - the position of `DEFAULT`
    /// @param limit - the last position of the element
    fn default_end(&self, default: usize, limit: usize) -> usize {
        let mut at = default.saturating_add(1);
        if self.is(at, Punctuator::LeftParen) {
            return self.group_end(at);
        }
        if self.is(at, Punctuator::Plus) || self.is(at, Punctuator::Minus) {
            at = at.saturating_add(1);
        }
        at.min(limit)
    }

    /// Returns the last position of a `REFERENCES` clause.
    ///
    /// @param references - the position of `REFERENCES`
    /// @param limit - the last position of the element
    fn references_end(&self, references: usize, limit: usize) -> usize {
        let mut last = references.saturating_add(1);
        if self.is(last.saturating_add(1), Punctuator::LeftParen) {
            last = self.group_end(last.saturating_add(1));
        }
        loop {
            let next = last.saturating_add(1);
            if next > limit {
                return last;
            }
            let word = self.word(next).unwrap_or_default();
            let following = self.word(next.saturating_add(1)).unwrap_or_default();
            let consumed = match (word.as_slice(), following.as_slice()) {
                (b"on", b"delete") | (b"on", b"update") => self.action_end(next.saturating_add(2)),
                (b"match", _) => next.saturating_add(1),
                (b"not", b"deferrable") => self.initially_end(next.saturating_add(1)),
                (b"deferrable", _) => self.initially_end(next),
                _ => return last,
            };
            last = consumed;
        }
    }

    /// Returns the last position of a referential action.
    ///
    /// @param at - the position of the action's first word
    fn action_end(&self, at: usize) -> usize {
        match self.word(at).as_deref() {
            Some(b"set") | Some(b"no") => at.saturating_add(1),
            _ => at,
        }
    }

    /// Returns the last position of `DEFERRABLE [INITIALLY DEFERRED|IMMEDIATE]`.
    ///
    /// @param deferrable - the position of `DEFERRABLE`
    fn initially_end(&self, deferrable: usize) -> usize {
        match self.word(deferrable.saturating_add(1)).as_deref() {
            Some(b"initially") => deferrable.saturating_add(2),
            _ => deferrable,
        }
    }

    /// Returns the last position of `[GENERATED ALWAYS] AS (expr) [STORED|VIRTUAL]`.
    ///
    /// @param start - the position of `GENERATED` or `AS`
    /// @param limit - the last position of the element
    fn generated_end(&self, start: usize, limit: usize) -> usize {
        let open = (start..=limit).find(|at| self.is(*at, Punctuator::LeftParen));
        let Some(open) = open else {
            return start;
        };
        let close = self.group_end(open);
        match self.word(close.saturating_add(1)).as_deref() {
            Some(b"stored") | Some(b"virtual") => close.saturating_add(1),
            _ => close,
        }
    }

    /// Removes a column constraint the way SQLite does.
    ///
    /// Everything from the end of the token before it to the start of the token
    /// after it goes, which takes the comments on either side, and one space is
    /// put back unless a comma or a parenthesis follows.
    ///
    /// @param constraint - the constraint
    fn cut_constraint(&self, constraint: Constraint) -> Vec<u8> {
        let from = self.end_before(constraint.first);
        let after = constraint.last.saturating_add(1);
        let to = self.start_of(after);
        let closes = self.is(after, Punctuator::Comma) || self.is(after, Punctuator::RightParen);
        let mut out = Vec::with_capacity(self.sql.len());
        out.extend_from_slice(self.sql.get(..from).unwrap_or(&[]));
        if !closes {
            out.push(b' ');
        }
        out.extend_from_slice(self.sql.get(to..).unwrap_or(&[]));
        out
    }

    /// Removes `CONSTRAINT name` from in front of a constraint that stays.
    ///
    /// @param constraint - the named constraint
    fn cut_label(&self, constraint: Constraint) -> Vec<u8> {
        let from = self.start_of(constraint.first);
        let to = self.start_of(constraint.first.saturating_add(2));
        let mut out = Vec::with_capacity(self.sql.len());
        out.extend_from_slice(self.sql.get(..from).unwrap_or(&[]));
        out.extend_from_slice(self.sql.get(to..).unwrap_or(&[]));
        out
    }

    /// Removes a table constraint, taking the comma that separates it.
    ///
    /// One that is followed by another goes from its first byte up to the first
    /// byte of the next. The last one goes with the comma before it, up to the
    /// parenthesis that closes the list.
    ///
    /// @param elements - every element of the column list
    /// @param at - the index of the element to remove
    fn cut_table_element(&self, elements: &[Element], at: usize) -> Vec<u8> {
        let Some(element) = elements.get(at) else {
            return self.sql.to_vec();
        };
        let (from, to) = match elements.get(at.saturating_add(1)) {
            Some(next) => (self.start_of(element.first), self.start_of(next.first)),
            None => (
                self.end_before(element.first.saturating_sub(1)),
                self.start_after(element.last),
            ),
        };
        let mut out = Vec::with_capacity(self.sql.len());
        out.extend_from_slice(self.sql.get(..from).unwrap_or(&[]));
        out.extend_from_slice(self.sql.get(to..).unwrap_or(&[]));
        out
    }
}

/// Returns the bytes of a span, trimmed of surrounding whitespace.
///
/// @param source - the text
/// @param span - the span
pub fn trimmed_span(source: &[u8], span: Span) -> Vec<u8> {
    let bytes = span.slice(source);
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |at| at.saturating_add(1));
    bytes.get(start..end).unwrap_or(&[]).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `NOT NULL` that ends a column goes in before the comma.
    #[test]
    fn not_null_goes_in_before_the_comma() {
        let sql = b"CREATE TABLE t(a  DEFAULT 5  , b)";
        let out = set_not_null(sql, 0, b"NOT NULL")
            .expect("it edits")
            .expect("it adds");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "CREATE TABLE t(a  DEFAULT 5   NOT NULL, b)"
        );
    }

    /// A column that already has the constraint is left as it is.
    #[test]
    fn a_column_with_not_null_is_not_changed() {
        let sql = b"CREATE TABLE t(a INT NOT NULL, b)";
        assert_eq!(set_not_null(sql, 0, b"NOT NULL").expect("it edits"), None);
    }

    /// Dropping a constraint takes the space and the comments around it.
    #[test]
    fn dropping_not_null_takes_the_space_around_it() {
        let sql = b"CREATE TABLE t(a INT NOT NULL UNIQUE, b INT /*x*/ CONSTRAINT n NOT NULL /*y*/ DEFAULT 1, c NOT NULL)";
        let first = drop_not_null(sql, 0).expect("it edits").expect("it drops");
        assert!(String::from_utf8_lossy(&first).starts_with("CREATE TABLE t(a INT UNIQUE, b"));
        let second = drop_not_null(sql, 1).expect("it edits").expect("it drops");
        assert!(String::from_utf8_lossy(&second).contains("b INT DEFAULT 1, c"));
        let third = drop_not_null(sql, 2).expect("it edits").expect("it drops");
        assert!(String::from_utf8_lossy(&third).ends_with(", c)"));
    }

    /// A named `CHECK` goes, a named `DEFAULT` only loses its name, and a key
    /// cannot be dropped.
    #[test]
    fn a_named_constraint_is_dropped_by_kind() {
        let sql = b"CREATE TABLE t(a INT CONSTRAINT k PRIMARY KEY, b CONSTRAINT d DEFAULT 5, CONSTRAINT c1 CHECK(a>0), CONSTRAINT c2 CHECK(a<9))";
        assert_eq!(
            drop_constraint(sql, b"k").expect("it reads"),
            Dropped::Refused
        );
        assert_eq!(
            drop_constraint(sql, b"zz").expect("it reads"),
            Dropped::Missing
        );
        let Dropped::Text(label) = drop_constraint(sql, b"d").expect("it reads") else {
            panic!("a default is relabelled");
        };
        assert!(String::from_utf8_lossy(&label).contains("b DEFAULT 5,"));
        let Dropped::Text(first) = drop_constraint(sql, b"c1").expect("it reads") else {
            panic!("a check is dropped");
        };
        assert!(String::from_utf8_lossy(&first).ends_with("5, CONSTRAINT c2 CHECK(a<9))"));
        let Dropped::Text(last) = drop_constraint(sql, b"c2").expect("it reads") else {
            panic!("a check is dropped");
        };
        assert!(String::from_utf8_lossy(&last).ends_with("CHECK(a>0))"));
    }
}
