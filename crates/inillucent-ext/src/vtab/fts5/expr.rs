//! The FTS5 query language: what a parsed query is made of, and the FTS3 rewrite.
//!
//! Invariant: a query is parsed once, by [`super::grammar`], into a tree whose leaves are phrases
//! with the columns they may be found in, and it is evaluated against doclists by
//! [`super::matcher`], never against the rows. A phrase is a run of terms at consecutive
//! positions in one column, a `NEAR` is a set of phrases within a distance of each other, and
//! both are decided from the positions the index already holds - which is why positions are
//! stored at all. Re-reading the documents to check a phrase would make a phrase search a table
//! scan.
//!
//! The grammar is the pinned release's own, as `fts5parse.y` writes it. Its precedence from the
//! loosest to the tightest is `OR`, `AND`, `NOT`, and then two phrases side by side, so
//! `a NOT b c` is `a NOT (b c)`. A group in parentheses is a whole expression and cannot be
//! followed by a phrase, so `(a) b` is a syntax error. [`super::grammar`] says how each form is
//! read and which message each refusal gives.

use inillucent_base::DbResult;

use super::tokenize::Tokenizer;
use crate::vtab::failure;

/// One term of a phrase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    /// The token, already folded by the tokenizer.
    pub token: Vec<u8>,
    /// Whether a trailing `*` made it a prefix.
    pub prefix: bool,
}

/// Which columns a phrase may be found in.
///
/// `named` is the list of columns, and `negated` says the list is the columns to skip. A phrase
/// with no filter in front of it skips none, which is the default. A filter the parser has
/// resolved is always the positive form, the columns a phrase may be found in, because two
/// filters one inside the other have to be intersected and that is simple on two lists of
/// columns that are allowed. An empty positive list admits no column at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnFilter {
    /// The columns the filter names.
    pub named: Vec<usize>,
    /// Whether the named columns are the ones to skip.
    pub negated: bool,
}

impl Default for ColumnFilter {
    /// Returns the filter of a phrase that has none: every column is admitted.
    fn default() -> ColumnFilter {
        ColumnFilter {
            named: Vec::new(),
            negated: true,
        }
    }
}

impl ColumnFilter {
    /// Returns a filter that admits only the named columns.
    ///
    /// @param named - the columns, which may be empty
    pub fn only(named: Vec<usize>) -> ColumnFilter {
        ColumnFilter {
            named,
            negated: false,
        }
    }

    /// Returns whether a phrase under this filter may match in a column.
    ///
    /// @param column - the column a hit was found in
    pub fn admits(&self, column: usize) -> bool {
        self.named.contains(&column) != self.negated
    }

    /// Returns whether the filter names the columns to search rather than admitting all.
    pub fn is_restricted(&self) -> bool {
        !self.negated
    }
}

/// A run of terms that must appear at consecutive positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phrase {
    /// The terms, in order.
    pub terms: Vec<Term>,
    /// The columns the phrase is restricted to.
    pub columns: ColumnFilter,
    /// Whether a leading `^` anchored it to the start of a column.
    pub anchored: bool,
}

/// One node of a parsed query.
///
/// Phrases are numbered in the order they were written, and a leaf records the number of its
/// first phrase so that what a phrase matched can be found without walking the tree in step with
/// a counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// A plain phrase, or `NEAR(a b, n)`: every phrase within `distance` tokens of the others.
    Near {
        /// The number of the first phrase.
        first: usize,
        /// The phrases; one for a plain phrase.
        phrases: Vec<Phrase>,
        /// How many tokens apart they may be.
        distance: u32,
    },
    /// A node that matches no row but still owns phrases, which is what a string with no tokens
    /// in it becomes, and a phrase whose column filters left it no column to be found in.
    Nothing {
        /// The number of the first phrase.
        first: usize,
        /// The phrases, which are numbered like any other.
        phrases: Vec<Phrase>,
    },
    /// Every one of the children.
    And(Vec<Match>),
    /// Any of the children.
    Or(Vec<Match>),
    /// The left side and not the right.
    Not(Box<Match>, Box<Match>),
}

/// A parsed query, with the phrases it names in the order they were written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// The expression.
    pub root: Match,
    /// Every phrase in the query, which is what `bm25` scores.
    pub phrases: Vec<Phrase>,
}

impl Query {
    /// Returns where each `NEAR` group sits in the list of phrases.
    ///
    /// Each answer is the number of the group's first phrase, how many phrases the group has,
    /// and the distance it allows. A plain phrase is not a group.
    pub fn near_groups(&self) -> Vec<(usize, usize, u32)> {
        let mut groups = Vec::new();
        collect_near(&self.root, &mut groups);
        groups
    }

    /// Parses one query string.
    ///
    /// @param text - the `MATCH` operand
    /// @param tokenizer - the table's tokenizer
    /// @param columns - the table's column names
    pub fn parse(text: &[u8], tokenizer: &Tokenizer, columns: &[Vec<u8>]) -> DbResult<Query> {
        Query::parse_in(text, tokenizer, columns, None)
    }

    /// Parses one query string that was written against one column, as `col MATCH 'x'` is.
    ///
    /// The column filter is applied to the whole tree after it is parsed, so a filter inside the
    /// query is intersected with it, as SQLite does.
    ///
    /// @param text - the `MATCH` operand
    /// @param tokenizer - the table's tokenizer
    /// @param columns - the table's column names
    /// @param column - the column the operand was written against, when it was
    pub fn parse_in(
        text: &[u8],
        tokenizer: &Tokenizer,
        columns: &[Vec<u8>],
        column: Option<usize>,
    ) -> DbResult<Query> {
        super::grammar::parse_query(text, tokenizer, columns, column)
    }

    /// Joins two queries with `AND`, the second one's phrases numbered after the first's.
    ///
    /// @param other - the query to add
    pub fn and(mut self, mut other: Query) -> Query {
        renumber(&mut other.root, self.phrases.len());
        let mut children = Vec::new();
        for root in [self.root, other.root] {
            match root {
                Match::And(inner) => children.extend(inner),
                single => children.push(single),
            }
        }
        self.phrases.extend(other.phrases);
        Query {
            root: Match::And(children),
            phrases: self.phrases,
        }
    }
}

/// Moves the phrase numbers of a tree up, which is what joining it after another query's phrases does.
///
/// @param node - the tree
/// @param by - how many phrases come before it
fn renumber(node: &mut Match, by: usize) {
    match node {
        Match::Near { first, .. } | Match::Nothing { first, .. } => *first += by,
        Match::And(children) | Match::Or(children) => {
            for child in children {
                renumber(child, by);
            }
        }
        Match::Not(left, right) => {
            renumber(left, by);
            renumber(right, by);
        }
    }
}

/// Finds the `NEAR` groups of an expression.
///
/// @param node - the expression
/// @param into - where each group's first phrase, size and distance go
fn collect_near(node: &Match, into: &mut Vec<(usize, usize, u32)>) {
    match node {
        Match::Near {
            first,
            phrases,
            distance,
        } if phrases.len() > 1 => into.push((*first, phrases.len(), *distance)),
        Match::Near { .. } | Match::Nothing { .. } => {}
        Match::And(children) | Match::Or(children) => {
            for child in children {
                collect_near(child, into);
            }
        }
        Match::Not(left, right) => {
            collect_near(left, into);
            collect_near(right, into);
        }
    }
}

/// Returns a syntax error whose own words reach the caller.
///
/// **`failure` attaches `detail`, and nothing above this reads it (task-1979,
/// R14).** Every refusal this parser built - "no such column: nope", "a group
/// is not closed", "NEAR is not closed" - reached an application as
/// `SQL logic error`, the generic text for the primary code, so a query with a
/// mistake in it said nothing about where the mistake was. The message is what
/// a caller is shown, so the same sentence goes in both.
///
/// @param said - what is wrong with the query
pub(super) fn syntax(said: impl Into<String>) -> inillucent_base::DbError {
    let said = said.into();
    failure(said.clone()).with_message(said)
}

/// The byte that opens a `NEAR/n` marker in the text [`fts3_words`] produces.
///
/// A control byte, because no query a person types contains one and the marker
/// has to survive until [`fts3_group_near`] reads it back.
const NEAR_OPEN: u8 = 0x01;
/// The byte that closes a `NEAR/n` marker.
const NEAR_CLOSE: u8 = 0x02;
/// How many tokens FTS3 allows between two terms joined by a bare `NEAR`.
const FTS3_DEFAULT_NEAR: u32 = 10;

/// Rewrites an FTS3 or FTS4 query into the FTS5 spelling.
///
/// **FTS3 and FTS4 read a different query language from FTS5 (bug hunt 10).**
/// SQLite builds them without `SQLITE_ENABLE_FTS3_PARENTHESIS`, so the language
/// has three operators and no more:
///
/// - a leading `-` on a term is `AND NOT`, so `MATCH 'x -y'` is `x` and not `y`;
/// - `OR`, in capitals, is `OR`;
/// - `NEAR` and `NEAR/n`, in capitals, join two terms or phrases that are at most
///   `n` tokens apart. A bare `NEAR` is `NEAR/10`.
///
/// `AND` and `NOT` are ordinary words there, so `MATCH 'x NOT y'` finds rows
/// holding all three words `x`, `NOT` and `y`. They are quoted on the way to the
/// FTS5 parser, which would otherwise read them as operators. `near/3` and `or`
/// in lower case are ordinary words as well.
///
/// A `-` that is not directly followed by a term is only a separator, so
/// `MATCH '- a'` is the query `a`. A query of nothing but `-term`s, a quote that
/// is never closed, and a `NEAR` with no term on one side are SQLite's
/// `malformed MATCH expression: [...]`. A query with no terms at all matches
/// nothing, and `None` says so. [`fts3_group_near`] says how the operators bind.
///
/// @param text - the query the `MATCH` operand was given
pub fn fts3_pattern(text: &[u8]) -> DbResult<Option<Vec<u8>>> {
    let malformed = || {
        syntax(format!(
            "malformed MATCH expression: [{}]",
            String::from_utf8_lossy(text)
        ))
    };
    let Some(words) = fts3_words(text, &malformed)? else {
        return Ok(None);
    };
    fts3_group_near(&words, &malformed)
}

/// Replaces the bytes of a rewritten FTS3 query that the FTS5 parser would refuse with spaces.
///
/// **FTS3 and FTS4 split words on punctuation, and the FTS5 lexer refuses it.** `x near/3 y` is the
/// three words `x`, `near` and `3` to FTS3 and its tokenizer, and `/` is
/// `fts5: syntax error near "/"` to FTS5's lexer. A byte outside quotes that is not white space, an
/// operator, a quote or part of a word is turned into a space, which leaves the same words.
///
/// @param text - what [`fts3_pattern`] answered
pub fn fts5_text_of_fts3(text: &[u8]) -> Vec<u8> {
    let mut quoted = false;
    text.iter()
        .map(|byte| {
            if *byte == b'"' {
                quoted = !quoted;
                return *byte;
            }
            let word = *byte >= 0x80 || byte.is_ascii_alphanumeric() || matches!(byte, b'_' | 0x1A);
            let punctuation = matches!(
                byte,
                b' ' | b'\t'
                    | b'\n'
                    | b'\r'
                    | b'('
                    | b')'
                    | b'{'
                    | b'}'
                    | b':'
                    | b','
                    | b'+'
                    | b'*'
                    | b'-'
                    | b'^'
            );
            if quoted || word || punctuation {
                *byte
            } else {
                b' '
            }
        })
        .collect()
}

/// Returns the length of the FTS3 keyword that starts at a position, and what it becomes.
///
/// Only a word that begins at `at` and ends at a space, a parenthesis, a quote or
/// the end of the query is a keyword. `AND` and `NOT` become quoted words. `NEAR`
/// and `NEAR/n` become the marker [`fts3_group_near`] reads. `NEAR/` with no
/// digits after it is the word `NEAR` followed by whatever the slash starts.
///
/// @param text - the query
/// @param at - where a word begins
fn fts3_keyword(text: &[u8], at: usize) -> Option<(usize, Vec<u8>)> {
    let rest = text.get(at..)?;
    let ends = |length: usize| {
        rest.get(length)
            .is_none_or(|next| next.is_ascii_whitespace() || matches!(*next, b'(' | b')' | b'"'))
    };
    if rest.starts_with(b"AND") && ends(3) {
        return Some((3, b"\"AND\"".to_vec()));
    }
    if rest.starts_with(b"NOT") && ends(3) {
        return Some((3, b"\"NOT\"".to_vec()));
    }
    if !rest.starts_with(b"NEAR") {
        return None;
    }
    if ends(4) {
        return Some((4, fts3_near_marker(FTS3_DEFAULT_NEAR)));
    }
    if rest.get(4) != Some(&b'/') {
        return None;
    }
    let digits = rest
        .get(5..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let end = 5usize.saturating_add(digits);
    if digits == 0 || !ends(end) {
        return None;
    }
    let distance = rest
        .get(5..end)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|number| number.parse::<u32>().ok())
        .unwrap_or(FTS3_DEFAULT_NEAR);
    Some((end, fts3_near_marker(distance)))
}

/// Returns the marker for a `NEAR/n` operator.
///
/// @param distance - how many tokens may sit between the two operands
fn fts3_near_marker(distance: u32) -> Vec<u8> {
    let mut marker = vec![NEAR_OPEN];
    marker.extend_from_slice(distance.to_string().as_bytes());
    marker.push(NEAR_CLOSE);
    marker
}

/// Rewrites the words of an FTS3 query: `-` becomes `NOT`, `AND` and `NOT` are quoted, `NEAR` is marked.
///
/// @param text - the query
/// @param malformed - builds the error for a query SQLite refuses
fn fts3_words(
    text: &[u8],
    malformed: &dyn Fn() -> inillucent_base::DbError,
) -> DbResult<Option<Vec<u8>>> {
    let mut out = Vec::with_capacity(text.len().saturating_add(8));
    let mut quoted = false;
    let mut at = 0usize;
    while let Some(byte) = text.get(at).copied() {
        let next_at = at.saturating_add(1);
        if byte == b'"' {
            quoted = !quoted;
            out.push(byte);
            at = next_at;
            continue;
        }
        if quoted {
            out.push(byte);
            at = next_at;
            continue;
        }
        // Only a word that begins here can be an operator; one inside a word
        // stays as it was written.
        let begins_word = out
            .last()
            .is_none_or(|before| before.is_ascii_whitespace() || *before == b'(');
        if begins_word {
            if let Some((length, replacement)) = fts3_keyword(text, at) {
                out.extend_from_slice(&replacement);
                at = at.saturating_add(length);
                continue;
            }
        }
        // SQLite reads the next token wherever the last one ended and negates it
        // when the byte before it is a `-`, so `x-y` is `x` and not `y` just as
        // `x -y` is. A `-` phrase with nothing before it is allowed when another
        // phrase follows; `fts3_group_near` refuses a query of nothing else.
        let starts_term = text
            .get(next_at)
            .is_some_and(|next| next.is_ascii_alphanumeric() || *next >= 0x80);
        if byte == b'-' && (begins_word || starts_term) {
            out.extend_from_slice(if starts_term { b" NOT " } else { b" " });
            at = next_at;
            continue;
        }
        out.push(byte);
        at = next_at;
    }
    if quoted {
        return Err(malformed());
    }
    if out.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    Ok(Some(out))
}

/// Splits the rewritten query into items: quoted strings, words, parentheses and `NEAR` markers.
///
/// @param text - the output of [`fts3_words`]
fn fts3_items(text: &[u8]) -> Vec<&[u8]> {
    let mut items: Vec<&[u8]> = Vec::new();
    let mut at = 0usize;
    while let Some(byte) = text.get(at).copied() {
        if byte.is_ascii_whitespace() {
            at = at.saturating_add(1);
            continue;
        }
        let start = at;
        match byte {
            b'(' | b')' => at = at.saturating_add(1),
            NEAR_OPEN => {
                while text.get(at).is_some_and(|held| *held != NEAR_CLOSE) {
                    at = at.saturating_add(1);
                }
                at = at.saturating_add(1);
            }
            b'"' => {
                at = at.saturating_add(1);
                while text.get(at).is_some_and(|held| *held != b'"') {
                    at = at.saturating_add(1);
                }
                at = at.saturating_add(1);
                // A star after the closing quote makes the last word a prefix.
                while text.get(at).is_some_and(|held| *held == b'*') {
                    at = at.saturating_add(1);
                }
            }
            _ => {
                while text.get(at).is_some_and(|held| {
                    !held.is_ascii_whitespace() && !matches!(*held, b'(' | b')' | b'"' | NEAR_OPEN)
                }) {
                    at = at.saturating_add(1);
                }
            }
        }
        items.push(text.get(start..at).unwrap_or_default());
    }
    items
}

/// Returns the distance a `NEAR` marker item carries, or nothing for any other item.
///
/// @param item - one item from [`fts3_items`]
fn fts3_near_distance(item: &[u8]) -> Option<u32> {
    let inner = item
        .strip_prefix(&[NEAR_OPEN])?
        .strip_suffix(&[NEAR_CLOSE])?;
    std::str::from_utf8(inner).ok()?.parse::<u32>().ok()
}

/// What one node of an FTS3 query is, as SQLite's `fts3ExprParse` builds the tree.
#[derive(Clone, Copy)]
enum Fts3Kind<'q> {
    /// A word or a quoted string, as it was written.
    Phrase(&'q [u8]),
    /// `NEAR/n`, with its distance.
    Near(u32),
    /// `OR`.
    Or,
    /// Two phrases side by side.
    And,
}

/// One node of the tree, with the links `insertBinaryOperator` moves.
struct Fts3Node<'q> {
    /// What the node is.
    kind: Fts3Kind<'q>,
    /// The operator above it, if any.
    parent: Option<usize>,
    /// An operator's left operand.
    left: Option<usize>,
    /// An operator's right operand.
    right: Option<usize>,
}

/// How tightly an FTS3 operator binds: lower binds tighter.
///
/// SQLite's `opPrecedence` without `SQLITE_ENABLE_FTS3_PARENTHESIS`: `NEAR`, then `OR`, then the
/// implicit `AND`. So `x y OR z` is `x AND (y OR z)`, where FTS5 reads it as `(x AND y) OR z`.
///
/// @param kind - the operator
fn fts3_precedence(kind: Fts3Kind<'_>) -> u8 {
    match kind {
        Fts3Kind::Near(_) => 1,
        Fts3Kind::Or => 2,
        Fts3Kind::And | Fts3Kind::Phrase(_) => 3,
    }
}

/// The tree of an FTS3 query while it is built, the way `fts3ExprParse` builds it.
struct Fts3Tree<'q> {
    /// Every node, in the order it was read.
    nodes: Vec<Fts3Node<'q>>,
    /// The node at the top.
    root: Option<usize>,
    /// The node read last, where the next operator is inserted.
    previous: Option<usize>,
    /// Whether the next item has to be a phrase.
    needs_phrase: bool,
    /// The phrases written with a leading `-`, which are taken out of the tree.
    negated: Vec<&'q [u8]>,
}

impl<'q> Fts3Tree<'q> {
    /// Adds a node and returns its number.
    ///
    /// @param kind - what it is
    fn push(&mut self, kind: Fts3Kind<'q>) -> usize {
        self.nodes.push(Fts3Node {
            kind,
            parent: None,
            left: None,
            right: None,
        });
        self.nodes.len().saturating_sub(1)
    }

    /// Returns the node's operator above it, if any.
    ///
    /// @param at - the node
    fn parent_of(&self, at: usize) -> Option<usize> {
        self.nodes.get(at).and_then(|node| node.parent)
    }

    /// Puts a binary operator into the tree above the node read last, as `insertBinaryOperator` does.
    ///
    /// It climbs from the node read last while the operator above binds at least as tightly as
    /// the new one, so operators of one precedence group to the left.
    ///
    /// @param operator - the new operator's node
    /// @param from - the node read last
    fn insert_binary(&mut self, operator: usize, from: usize) {
        let new = self
            .nodes
            .get(operator)
            .map_or(u8::MAX, |node| fts3_precedence(node.kind));
        let mut split = from;
        while let Some(above) = self.parent_of(split) {
            let binds = self
                .nodes
                .get(above)
                .map_or(u8::MAX, |node| fts3_precedence(node.kind));
            if binds > new {
                break;
            }
            split = above;
        }
        match self.parent_of(split) {
            Some(above) => {
                if let Some(node) = self.nodes.get_mut(above) {
                    node.right = Some(operator);
                }
                if let Some(node) = self.nodes.get_mut(operator) {
                    node.parent = Some(above);
                }
            }
            None => self.root = Some(operator),
        }
        if let Some(node) = self.nodes.get_mut(operator) {
            node.left = Some(split);
        }
        if let Some(node) = self.nodes.get_mut(split) {
            node.parent = Some(operator);
        }
    }

    /// Reads one item, a phrase or an operator, into the tree.
    ///
    /// @param kind - the item
    /// @param malformed - builds the error for a query SQLite refuses
    fn read(
        &mut self,
        kind: Fts3Kind<'q>,
        malformed: &dyn Fn() -> inillucent_base::DbError,
    ) -> DbResult<()> {
        let phrase = matches!(kind, Fts3Kind::Phrase(_));
        if !phrase && self.needs_phrase {
            return Err(malformed());
        }
        if phrase && !self.needs_phrase {
            let and = self.push(Fts3Kind::And);
            let Some(from) = self.previous else {
                return Err(malformed());
            };
            self.insert_binary(and, from);
            self.previous = Some(and);
        }
        let previous_is_phrase = self.previous.is_none_or(|at| {
            self.nodes
                .get(at)
                .is_some_and(|node| matches!(node.kind, Fts3Kind::Phrase(_)))
        });
        if matches!(kind, Fts3Kind::Near(_)) && !previous_is_phrase {
            return Err(malformed());
        }
        let at = self.push(kind);
        match (phrase, self.root, self.previous) {
            (true, Some(_), Some(above)) => {
                if let Some(node) = self.nodes.get_mut(above) {
                    node.right = Some(at);
                }
                if let Some(node) = self.nodes.get_mut(at) {
                    node.parent = Some(above);
                }
            }
            (true, _, _) => self.root = Some(at),
            (false, _, Some(from)) => self.insert_binary(at, from),
            (false, _, None) => return Err(malformed()),
        }
        self.needs_phrase = !phrase;
        self.previous = Some(at);
        Ok(())
    }

    /// Writes a node and everything below it in the FTS5 spelling, with every operator bracketed.
    ///
    /// @param at - the node
    /// @param out - where the text goes
    fn render(&self, at: usize, out: &mut Vec<u8>) {
        let Some(node) = self.nodes.get(at) else {
            return;
        };
        let (word, left, right) = match node.kind {
            Fts3Kind::Phrase(text) => {
                out.extend_from_slice(text);
                return;
            }
            Fts3Kind::Near(_) => {
                let mut members = Vec::new();
                let widest = self.near_members(at, &mut members);
                out.extend_from_slice(b"NEAR(");
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        out.push(b' ');
                    }
                    out.extend_from_slice(member);
                }
                out.extend_from_slice(format!(", {widest})").as_bytes());
                return;
            }
            Fts3Kind::Or => (&b" OR "[..], node.left, node.right),
            Fts3Kind::And => (&b" AND "[..], node.left, node.right),
        };
        out.push(b'(');
        if let Some(left) = left {
            self.render(left, out);
        }
        out.extend_from_slice(word);
        if let Some(right) = right {
            self.render(right, out);
        }
        out.push(b')');
    }

    /// Collects the phrases of a chain of `NEAR` operators, and returns the widest distance in it.
    ///
    /// @param at - the top `NEAR` of the chain, or one of its phrases
    /// @param members - where the phrases go, in written order
    fn near_members(&self, at: usize, members: &mut Vec<&'q [u8]>) -> u32 {
        let Some(node) = self.nodes.get(at) else {
            return 0;
        };
        match node.kind {
            Fts3Kind::Phrase(text) => {
                members.push(text);
                0
            }
            Fts3Kind::Near(distance) => {
                let left = node.left.map_or(0, |left| self.near_members(left, members));
                let right = node
                    .right
                    .map_or(0, |right| self.near_members(right, members));
                distance.max(left).max(right)
            }
            Fts3Kind::Or | Fts3Kind::And => 0,
        }
    }
}

/// Parses the items of an FTS3 query the way SQLite does, and writes it in the FTS5 spelling.
///
/// **This follows `fts3ExprParse` in SQLite's `fts3_expr.c` with
/// `SQLITE_ENABLE_FTS3_PARENTHESIS` off.** Three of its rules differ from FTS5, and an answer
/// that followed the FTS5 reading differed from SQLite:
///
/// - `OR` binds tighter than two phrases side by side, so `x OR y z` is `(x OR y) AND z`.
/// - A phrase written with a leading `-` is taken out of the tree where it stands, and every such
///   phrase is applied with `NOT` to the whole query at the end. The next operator joins the
///   phrase before the `-` phrase, so `x -y NEAR/3 w` is `(x NEAR/3 w) NOT y` and not
///   `x NOT (y NEAR/3 w)`. A query of nothing but `-` phrases is malformed, and `-x y` is
///   `y NOT x`.
/// - Parentheses group nothing. They only separate words.
///
/// A `NEAR` chain such as `a NEAR/5 b NEAR/5 c` becomes one FTS5 group `NEAR(a b c, k)`, which
/// keeps every term within a window of that many tokens. FTS3 compares each pair that is next to
/// each other in the chain. Both answer the same on the chains with one distance throughout. With
/// several distances the group takes the largest. An operator with no phrase on one side, and a
/// `NEAR` after something other than a phrase, are SQLite's malformed query.
///
/// @param text - the output of [`fts3_words`]
/// @param malformed - builds the error for a query SQLite refuses
fn fts3_group_near(
    text: &[u8],
    malformed: &dyn Fn() -> inillucent_base::DbError,
) -> DbResult<Option<Vec<u8>>> {
    let mut tree = Fts3Tree {
        nodes: Vec::new(),
        root: None,
        previous: None,
        needs_phrase: true,
        negated: Vec::new(),
    };
    let mut negate_next = false;
    for item in fts3_items(text) {
        if matches!(item, b"(" | b")") {
            continue;
        }
        if item == b"NOT" {
            negate_next = true;
            continue;
        }
        if std::mem::take(&mut negate_next) {
            // A word after a `-` is a phrase whatever it spells: `-OR` is the word `or`.
            // It is one token, so a column name before a colon is not a filter there:
            // `-b:y` negates the word `b`, and `y` is a phrase of its own.
            let (negated, rest) = match item.iter().position(|byte| *byte == b':') {
                Some(colon) if item.first() != Some(&b'"') => (
                    item.get(..colon).unwrap_or_default(),
                    item.get(colon.saturating_add(1)..),
                ),
                _ => (item, None),
            };
            tree.negated
                .push(match (negated, fts3_near_distance(negated)) {
                    (b"OR", _) => b"\"OR\"",
                    (_, Some(_)) => b"\"NEAR\"",
                    _ => negated,
                });
            if let Some(rest) = rest.filter(|rest| !rest.is_empty()) {
                tree.read(Fts3Kind::Phrase(rest), malformed)?;
            }
            continue;
        }
        let kind = match (item, fts3_near_distance(item)) {
            (_, Some(distance)) => Fts3Kind::Near(distance),
            (b"OR", None) => Fts3Kind::Or,
            _ => Fts3Kind::Phrase(item),
        };
        tree.read(kind, malformed)?;
    }
    if tree.root.is_some() && tree.needs_phrase {
        return Err(malformed());
    }
    let Some(root) = tree.root else {
        return match tree.negated.is_empty() {
            true => Ok(None),
            false => Err(malformed()),
        };
    };
    let mut out: Vec<u8> = Vec::with_capacity(text.len().saturating_add(16));
    tree.render(root, &mut out);
    for phrase in &tree.negated {
        let mut wrapped = Vec::with_capacity(out.len().saturating_add(phrase.len() + 8));
        wrapped.push(b'(');
        wrapped.extend_from_slice(&out);
        wrapped.extend_from_slice(b" NOT ");
        wrapped.extend_from_slice(phrase);
        wrapped.push(b')');
        out = wrapped;
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns what an FTS3 or FTS4 query becomes, as text.
    ///
    /// @param text - the query
    fn fts3(text: &str) -> Option<String> {
        fts3_pattern(text.as_bytes())
            .expect("a well formed query")
            .map(|bytes| String::from_utf8(bytes).expect("text"))
    }

    /// `AND` and `NOT` are words in FTS3 and FTS4, and the binary operator is a leading `-`.
    #[test]
    fn fts3_reads_and_and_not_as_words() {
        assert_eq!(fts3("x NOT y").as_deref(), Some("((x AND \"NOT\") AND y)"));
        assert_eq!(fts3("x AND y").as_deref(), Some("((x AND \"AND\") AND y)"));
        assert_eq!(fts3("x not y").as_deref(), Some("((x AND not) AND y)"));
        assert_eq!(fts3("x -y").as_deref(), Some("(x NOT y)"));
        assert_eq!(fts3("x OR y").as_deref(), Some("(x OR y)"));
    }

    /// `NEAR/n` becomes an FTS5 group with the same distance, and a bare `NEAR` is ten.
    #[test]
    fn fts3_near_becomes_a_near_group() {
        assert_eq!(fts3("x NEAR/3 y").as_deref(), Some("NEAR(x y, 3)"));
        assert_eq!(fts3("x NEAR y").as_deref(), Some("NEAR(x y, 10)"));
        assert_eq!(
            fts3("z OR x NEAR/0 y").as_deref(),
            Some("(z OR NEAR(x y, 0))")
        );
        assert_eq!(
            fts3("\"x w\" NEAR/2 y*").as_deref(),
            Some("NEAR(\"x w\" y*, 2)")
        );
        assert_eq!(
            fts3("x NEAR/5 y NEAR/5 q").as_deref(),
            Some("NEAR(x y q, 5)")
        );
        assert_eq!(
            fts3("x near/3 y").as_deref(),
            Some("((x AND near/3) AND y)")
        );
    }

    /// A `NEAR` with no term on one side is the error SQLite gives.
    #[test]
    fn fts3_near_needs_a_term_on_both_sides() {
        for text in [
            "NEAR",
            "NEAR/3 y",
            "x NEAR/3",
            "x NEAR OR y",
            "x NEAR NEAR y",
        ] {
            let error = fts3_pattern(text.as_bytes()).expect_err(text);
            assert!(
                error.message().contains("malformed MATCH expression"),
                "{text}: {}",
                error.message()
            );
        }
    }

    /// A `-` before a quote is only a separator in FTS3 and FTS4.
    #[test]
    fn fts3_minus_before_a_quote_is_not_an_operator() {
        assert_eq!(fts3("x -\"w q\"").as_deref(), Some("(x AND \"w q\")"));
    }

    /// `OR` binds tighter than two phrases side by side, as SQLite's FTS3 parser has it.
    #[test]
    fn fts3_or_binds_tighter_than_and() {
        assert_eq!(fts3("x y OR z").as_deref(), Some("(x AND (y OR z))"));
        assert_eq!(fts3("x OR y z").as_deref(), Some("((x OR y) AND z)"));
        assert_eq!(
            fts3("x OR y NEAR/2 z").as_deref(),
            Some("(x OR NEAR(y z, 2))")
        );
    }

    /// A `-` phrase leaves the tree where it stands and is applied with `NOT` to the whole query.
    #[test]
    fn fts3_minus_applies_to_the_whole_query() {
        assert_eq!(
            fts3("x -y NEAR/3 w").as_deref(),
            Some("(NEAR(x w, 3) NOT y)")
        );
        assert_eq!(fts3("x -y OR z").as_deref(), Some("((x OR z) NOT y)"));
        assert_eq!(fts3("-x y").as_deref(), Some("(y NOT x)"));
        assert_eq!(fts3("x -y -z").as_deref(), Some("((x NOT y) NOT z)"));
        assert_eq!(fts3("(x OR y) z").as_deref(), Some("((x OR y) AND z)"));
        for text in ["-x", "-x -y", "x OR -y", "x NEAR/3 -y"] {
            let error = fts3_pattern(text.as_bytes()).expect_err(text);
            assert!(
                error.message().contains("malformed MATCH expression"),
                "{text}: {}",
                error.message()
            );
        }
    }
}
