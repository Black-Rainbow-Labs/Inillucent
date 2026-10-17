//! The FTS5 query language, and what it matches.
//!
//! Invariant: a query is parsed once and evaluated against doclists, never
//! against the rows. A phrase is a run of terms at consecutive positions in one
//! column, a `NEAR` is a set of terms within a distance of each other, and both
//! are decided from the positions the index already holds - which is why
//! positions are stored at all. Re-reading the documents to check a phrase
//! would make a phrase search a table scan.
//!
//! The grammar, which is the pinned release's:
//!
//! ```text
//! expr    := orlist
//! orlist  := andlist ( OR andlist )*
//! andlist := notpart ( AND? notpart )*        -- juxtaposition is AND
//! notpart := primary ( NOT primary )*
//! primary := '(' expr ')' | colspec ':' primary | phrase | NEAR '(' phrase+ , n ')'
//! colspec := '-'? ( word | '{' word+ '}' )    -- a '-' excludes those columns
//! phrase  := '^'? ( '"' term+ '"' | term ) '*'?
//! term    := word '*'?                        -- a trailing star is a prefix
//! ```
//!
//! **Four of those forms were refused and one was parsed and dropped
//! (task-1979, R4 and R7).** `{title body}:cat`, `{title}:cat` and `-title:cat`
//! were syntax errors here and are answered by SQLite; `"a b"*`, a phrase whose
//! last term is a prefix, was refused the same way. `^cat` parsed, and the
//! anchor was then thrown away: `^alpha` returned every row holding `alpha`
//! anywhere - 206 rows where SQLite answered 82.

use std::collections::BTreeMap;

use inillucent_base::DbResult;

use super::tokenize::Tokenizer;
use super::DocEntry;
use crate::shadow::ShadowTables;
use crate::vtab::{failure, Context};

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
/// An empty list admits every column, which is what a phrase with no filter in
/// front of it means. `negated` is the `-` form: `-title:cat` searches every
/// column *except* `title`, so the same list answers both questions and there
/// is one place that decides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnFilter {
    /// The columns the filter names.
    pub named: Vec<usize>,
    /// Whether the named columns are the ones to skip.
    pub negated: bool,
}

impl ColumnFilter {
    /// Returns whether a phrase under this filter may match in a column.
    ///
    /// @param column - the column a hit was found in
    pub fn admits(&self, column: usize) -> bool {
        if self.named.is_empty() {
            return true;
        }
        self.named.contains(&column) != self.negated
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// A phrase.
    Phrase(Phrase),
    /// `NEAR(a b, n)`: every phrase within `distance` tokens of the others.
    Near {
        /// The phrases.
        phrases: Vec<Phrase>,
        /// How many tokens apart they may be.
        distance: u32,
    },
    /// Both sides.
    And(Box<Match>, Box<Match>),
    /// Either side.
    Or(Box<Match>, Box<Match>),
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
    /// Parses one query string.
    pub fn parse(text: &[u8], tokenizer: &Tokenizer, columns: &[Vec<u8>]) -> DbResult<Query> {
        let tokens = lex(text)?;
        let mut parser = Parser {
            tokens,
            at: 0,
            tokenizer,
            columns,
        };
        let root = parser.parse_or(&ColumnFilter::default())?;
        if parser.at < parser.tokens.len() {
            return Err(syntax("fts5: syntax error near the end of the query"));
        }
        let mut phrases = Vec::new();
        collect_phrases(&root, &mut phrases);
        Ok(Query { root, phrases })
    }
}

/// Refuses an FTS5 query that holds a character no FTS5 token starts with.
///
/// **SQLite's `fts5ExprGetToken` stops at the first byte that is not white space, an operator, a
/// quote or part of a word.** A word is ASCII letters and digits, `_`, the byte `0x1A` and every byte
/// at or above `0x80`. So `x NEAR/3 w`, which is FTS3's spelling of a `NEAR`, is
/// `fts5: syntax error near "/"` there, where the lexer here dropped the `/` into a word, the
/// tokenizer split the word, and the query matched nothing without an error. The FTS3 and FTS4
/// front end does not come here: [`fts3_pattern`] rewrites their queries first.
///
/// @param text - the `MATCH` operand of an FTS5 table
pub fn refuse_stray_characters(text: &[u8]) -> DbResult<()> {
    let mut quoted = false;
    for byte in text.iter().copied() {
        if byte == b'"' {
            // A doubled quote inside a string closes it and opens it again,
            // which leaves the same state as reading it as one character.
            quoted = !quoted;
            continue;
        }
        let allowed = quoted
            || byte.is_ascii_alphanumeric()
            || byte >= 0x80
            || matches!(
                byte,
                b'_' | 0x1A
                    | b' '
                    | b'\t'
                    | b'\n'
                    | b'\r'
                    | 0x0B
                    | 0x0C
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
        if !allowed {
            return Err(syntax(format!(
                "fts5: syntax error near \"{}\"",
                char::from(byte)
            )));
        }
    }
    Ok(())
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

impl Query {
    /// Returns where each `NEAR` group sits in the list of phrases.
    ///
    /// Each answer is the index of the group's first phrase, how many phrases
    /// the group has, and the distance it allows. The phrases of a group are
    /// next to each other in [`Query::phrases`], because they are listed in
    /// written order.
    pub fn near_groups(&self) -> Vec<(usize, usize, u32)> {
        let mut groups = Vec::new();
        let mut counter = 0usize;
        collect_near(&self.root, &mut counter, &mut groups);
        groups
    }
}

/// Finds the `NEAR` groups of an expression, numbering phrases as it goes.
///
/// @param node - the expression
/// @param counter - the index of the next phrase, advanced past this node
/// @param into - where each group's first phrase, size and distance go
fn collect_near(node: &Match, counter: &mut usize, into: &mut Vec<(usize, usize, u32)>) {
    match node {
        Match::Phrase(_) => *counter = counter.saturating_add(1),
        Match::Near { phrases, distance } => {
            into.push((*counter, phrases.len(), *distance));
            *counter = counter.saturating_add(phrases.len());
        }
        Match::And(left, right) | Match::Or(left, right) | Match::Not(left, right) => {
            collect_near(left, counter, into);
            collect_near(right, counter, into);
        }
    }
}

/// Adds every phrase of an expression to a list, in written order.
fn collect_phrases(node: &Match, into: &mut Vec<Phrase>) {
    match node {
        Match::Phrase(phrase) => into.push(phrase.clone()),
        Match::Near { phrases, .. } => into.extend(phrases.iter().cloned()),
        Match::And(left, right) | Match::Or(left, right) | Match::Not(left, right) => {
            collect_phrases(left, into);
            collect_phrases(right, into);
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
fn syntax(said: impl Into<String>) -> inillucent_base::DbError {
    let said = said.into();
    failure(said.clone()).with_message(said)
}

/// One lexical token of a query.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Lexeme {
    /// A bare word.
    Word(Vec<u8>),
    /// A quoted string, which is a phrase however it tokenises.
    Quoted(Vec<u8>),
    /// `(`
    Open,
    /// `)`
    Close,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `*`
    Star,
    /// `+`, which joins two phrases into one.
    Plus,
    /// `^`, which anchors a phrase to the start of a column.
    Caret,
    /// `{`, which opens a list of column names.
    OpenBrace,
    /// `}`, which closes one.
    CloseBrace,
    /// `-`, which negates the column filter after it.
    Minus,
}

/// Splits a query into its lexemes.
///
/// **A string that is never closed is a syntax error, and a `-` is its own
/// lexeme (task-1979, R17).** Both were swallowed: an unterminated `"cat`
/// produced the phrase `cat`, and `it\'s` produced `it` AND `s`, where SQLite
/// refuses each of them; `co-operate` lexed as one word and looked for a token
/// no tokenizer would ever produce. Three strings SQLite calls errors were
/// answered with rows.
///
/// @param text - the query the `MATCH` operand was given
fn lex(text: &[u8]) -> DbResult<Vec<Lexeme>> {
    let text = String::from_utf8_lossy(text);
    let mut out = Vec::new();
    let mut characters = text.chars().peekable();
    let mut word = String::new();
    while let Some(character) = characters.next() {
        let punctuation = match character {
            '(' => Some(Lexeme::Open),
            ')' => Some(Lexeme::Close),
            ',' => Some(Lexeme::Comma),
            ':' => Some(Lexeme::Colon),
            '*' => Some(Lexeme::Star),
            '+' => Some(Lexeme::Plus),
            '^' => Some(Lexeme::Caret),
            '{' => Some(Lexeme::OpenBrace),
            '}' => Some(Lexeme::CloseBrace),
            '-' => Some(Lexeme::Minus),
            _ => None,
        };
        if let Some(punctuation) = punctuation {
            if !word.is_empty() {
                out.push(Lexeme::Word(core::mem::take(&mut word).into_bytes()));
            }
            out.push(punctuation);
            continue;
        }
        if character == '"' || character == '\'' {
            if !word.is_empty() {
                out.push(Lexeme::Word(core::mem::take(&mut word).into_bytes()));
            }
            let mut quoted = String::new();
            let mut closed = false;
            while let Some(inner) = characters.next() {
                // **A doubled quote inside a string stands for one quote**,
                // which is how a query writes a phrase that holds a quote
                // character. Without this the first two characters read as an
                // empty string and the rest read as two more strings, so a
                // query SQLite answers with rows was a syntax error here.
                if inner == character {
                    if characters.peek() == Some(&character) {
                        characters.next();
                        quoted.push(inner);
                        continue;
                    }
                    closed = true;
                    break;
                }
                quoted.push(inner);
            }
            if !closed {
                return Err(syntax(format!(
                    "fts5: syntax error - the string opened by {character} is not closed"
                )));
            }
            out.push(Lexeme::Quoted(quoted.into_bytes()));
            continue;
        }
        if character.is_whitespace() {
            if !word.is_empty() {
                out.push(Lexeme::Word(core::mem::take(&mut word).into_bytes()));
            }
            continue;
        }
        word.push(character);
    }
    if !word.is_empty() {
        out.push(Lexeme::Word(word.into_bytes()));
    }
    Ok(out)
}

/// The query parser's position.
struct Parser<'a> {
    tokens: Vec<Lexeme>,
    at: usize,
    tokenizer: &'a Tokenizer,
    columns: &'a [Vec<u8>],
}

impl Parser<'_> {
    /// Returns the lexeme at the cursor.
    fn peek(&self) -> Option<&Lexeme> {
        self.tokens.get(self.at)
    }

    /// Returns whether the cursor is on a keyword.
    ///
    /// **Upper case only.** FTS5 reads `AND`, `OR`, `NOT` and `NEAR` as operators when they are
    /// written in capitals and as ordinary words otherwise, so `this and that` searches for
    /// three words and `and*` is the prefix `and`. They were matched without regard to case, and
    /// a query that held the word `not` in English was a syntax error.
    fn at_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek(), Some(Lexeme::Word(word)) if word.as_slice() == keyword.as_bytes())
    }

    /// Parses an `OR` chain.
    fn parse_or(&mut self, column: &ColumnFilter) -> DbResult<Match> {
        let mut left = self.parse_and(column)?;
        while self.at_keyword("OR") {
            self.at = self.at.saturating_add(1);
            let right = self.parse_and(column)?;
            left = Match::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses an `AND` chain, where two things side by side are an `AND`.
    fn parse_and(&mut self, column: &ColumnFilter) -> DbResult<Match> {
        let mut left = self.parse_not(column)?;
        loop {
            let mut written = false;
            if self.at_keyword("AND") {
                written = true;
                self.at = self.at.saturating_add(1);
            } else if self.at_keyword("OR")
                || self.at_keyword("NOT")
                || matches!(
                    self.peek(),
                    None | Some(Lexeme::Close) | Some(Lexeme::Comma)
                )
            {
                return Ok(left);
            }
            let right = self.parse_not(column)?;
            // Only two strings side by side lose an empty one. With `AND` written, an empty
            // string is a node that matches nothing, so `hello AND 😀` finds no rows.
            left = if written {
                Match::And(Box::new(left), Box::new(right))
            } else {
                join_implicit(left, right)
            };
        }
    }

    /// Parses a `NOT` chain.
    fn parse_not(&mut self, column: &ColumnFilter) -> DbResult<Match> {
        let mut left = self.parse_primary(column)?;
        while self.at_keyword("NOT") {
            self.at = self.at.saturating_add(1);
            let right = self.parse_primary(column)?;
            left = Match::Not(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Parses one primary: a group, a column filter, a `NEAR`, or a phrase.
    fn parse_primary(&mut self, column: &ColumnFilter) -> DbResult<Match> {
        if matches!(self.peek(), Some(Lexeme::Open)) {
            self.at = self.at.saturating_add(1);
            let inner = self.parse_or(column)?;
            if !matches!(self.peek(), Some(Lexeme::Close)) {
                return Err(syntax("fts5: a group is not closed"));
            }
            self.at = self.at.saturating_add(1);
            return Ok(inner);
        }
        if self.at_keyword("NEAR") && matches!(self.tokens.get(self.at + 1), Some(Lexeme::Open)) {
            return self.parse_near(column);
        }
        if let Some(filter) = self.parse_column_filter()? {
            return self.parse_primary(&filter);
        }
        let phrase = self.parse_phrase(column)?;
        Ok(Match::Phrase(phrase))
    }

    /// Parses a column filter, when the cursor is on one.
    ///
    /// The four forms SQLite takes: `c:`, `{c d}:`, `-c:` and `-{c d}:`. A `-`
    /// makes the list the columns to skip. `Ok(None)` means the cursor was not
    /// on a filter at all, which is the ordinary case and is why this is
    /// separated from [`Parser::parse_primary`] rather than written into it.
    fn parse_column_filter(&mut self) -> DbResult<Option<ColumnFilter>> {
        let started = self.at;
        let negated = matches!(self.peek(), Some(Lexeme::Minus));
        if negated {
            self.at = self.at.saturating_add(1);
        }
        let named = match self.peek().cloned() {
            Some(Lexeme::OpenBrace) => {
                self.at = self.at.saturating_add(1);
                let mut named = Vec::new();
                // A column name may be quoted, which is how one holding a character such as `-`
                // is written: `{"b-c"} : beta`.
                while let Some(Lexeme::Word(name) | Lexeme::Quoted(name)) = self.peek().cloned() {
                    self.at = self.at.saturating_add(1);
                    named.push(self.column_index(&name)?);
                }
                if !matches!(self.peek(), Some(Lexeme::CloseBrace)) {
                    return Err(syntax("fts5: a column list is not closed"));
                }
                self.at = self.at.saturating_add(1);
                named
            }
            // After a `-` the name is a column whether or not a colon
            // follows, which is how SQLite answers `co-operate` with
            // "no such column: operate" rather than with rows.
            Some(Lexeme::Word(name) | Lexeme::Quoted(name))
                if negated || matches!(self.tokens.get(self.at + 1), Some(Lexeme::Colon)) =>
            {
                self.at = self.at.saturating_add(1);
                vec![self.column_index(&name)?]
            }
            // A `-` that is not in front of a column filter is a stray, and
            // `parse_phrase` refuses it where every other stray is refused.
            _ => {
                self.at = started;
                return Ok(None);
            }
        };
        if !matches!(self.peek(), Some(Lexeme::Colon)) {
            return Err(syntax(
                "fts5: a column filter has to be followed by a colon and a phrase",
            ));
        }
        self.at = self.at.saturating_add(1);
        Ok(Some(ColumnFilter { named, negated }))
    }

    /// Returns which column a name is, refusing one the table does not have.
    ///
    /// @param name - the column name as written
    fn column_index(&self, name: &[u8]) -> DbResult<usize> {
        self.column_named(name)
            .ok_or_else(|| syntax(format!("no such column: {}", String::from_utf8_lossy(name))))
    }

    /// Parses `NEAR(phrase phrase, distance)`.
    fn parse_near(&mut self, column: &ColumnFilter) -> DbResult<Match> {
        self.at = self.at.saturating_add(2);
        let mut phrases = Vec::new();
        while !matches!(
            self.peek(),
            None | Some(Lexeme::Close) | Some(Lexeme::Comma)
        ) {
            phrases.push(self.parse_phrase(column)?);
        }
        let mut distance = 10u32;
        if matches!(self.peek(), Some(Lexeme::Comma)) {
            self.at = self.at.saturating_add(1);
            if let Some(Lexeme::Word(word)) = self.peek().cloned() {
                distance = String::from_utf8_lossy(&word).trim().parse().unwrap_or(10);
                self.at = self.at.saturating_add(1);
            }
        }
        if !matches!(self.peek(), Some(Lexeme::Close)) {
            return Err(syntax("fts5: NEAR is not closed"));
        }
        self.at = self.at.saturating_add(1);
        if phrases.is_empty() {
            return Err(syntax("fts5: NEAR needs at least one phrase"));
        }
        phrases.retain(|phrase| !phrase.terms.is_empty());
        if phrases.is_empty() {
            return Ok(Match::Phrase(Phrase {
                terms: Vec::new(),
                columns: column.clone(),
                anchored: false,
            }));
        }
        Ok(Match::Near { phrases, distance })
    }

    /// Parses one phrase: a quoted string, or a word with an optional `*`.
    fn parse_phrase(&mut self, column: &ColumnFilter) -> DbResult<Phrase> {
        // A leading `^` anchors the phrase to the start of the column, which
        // is kept on the phrase and applied in `phrase_hits`.
        let anchored = matches!(self.peek(), Some(Lexeme::Caret));
        if anchored {
            self.at = self.at.saturating_add(1);
        }
        let mut terms = Vec::new();
        // Whether any string or word was read. A string the tokenizer reduces to nothing is a
        // phrase with no terms and is left out of the query; no string at all is a syntax error.
        let mut read_a_string = false;
        loop {
            match self.peek().cloned() {
                Some(Lexeme::Quoted(text)) => {
                    read_a_string = true;
                    self.at = self.at.saturating_add(1);
                    // **A quoted phrase may end in a prefix (task-1979, R7).**
                    // `"a b"*` is SQLite's phrase prefix and was refused here,
                    // because only the bare word branch below looked for a
                    // trailing star.
                    let starred = matches!(self.peek(), Some(Lexeme::Star));
                    if starred {
                        self.at = self.at.saturating_add(1);
                    }
                    let tokens = self.tokenizer.tokens(&text);
                    let last = tokens.len().saturating_sub(1);
                    for (position, token) in tokens.into_iter().enumerate() {
                        terms.push(Term {
                            token,
                            prefix: starred && position == last,
                        });
                    }
                }
                Some(Lexeme::Word(word))
                    if word.as_slice() != b"AND"
                        && word.as_slice() != b"OR"
                        && word.as_slice() != b"NOT" =>
                {
                    read_a_string = true;
                    self.at = self.at.saturating_add(1);
                    let tokens = self.tokenizer.tokens(&word);
                    let last = tokens.len().saturating_sub(1);
                    for (position, token) in tokens.into_iter().enumerate() {
                        terms.push(Term {
                            token,
                            prefix: position == last && matches!(self.peek(), Some(Lexeme::Star)),
                        });
                    }
                    if matches!(self.peek(), Some(Lexeme::Star)) {
                        self.at = self.at.saturating_add(1);
                    }
                }
                _ => break,
            }
            // A `+` joins the next word into the same phrase.
            if matches!(self.peek(), Some(Lexeme::Plus)) {
                self.at = self.at.saturating_add(1);
                continue;
            }
            break;
        }
        if terms.is_empty() && !read_a_string {
            return Err(syntax("fts5: syntax error - a phrase has no terms"));
        }
        Ok(Phrase {
            terms,
            columns: column.clone(),
            anchored,
        })
    }

    /// Returns which column a name is.
    fn column_named(&self, name: &[u8]) -> Option<usize> {
        self.columns
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
    }
}

/// Reports whether a node is a phrase that has no terms, which is what a string the tokenizer
/// reduces to nothing becomes.
///
/// @param node - the parsed node
fn is_empty_phrase(node: &Match) -> bool {
    matches!(node, Match::Phrase(phrase) if phrase.terms.is_empty())
}

/// Joins two strings written side by side, leaving out one that has no tokens.
///
/// A string the tokenizer reduces to nothing is a node that matches no row. SQLite removes it
/// when it sits next to another string with no operator between them, so `hello 😀` is `hello`
/// and `😀 hello` is `hello`. With `AND`, `OR` or `NOT` written the node stays: `hello OR 😀`
/// is `hello` because nothing is added, `hello AND 😀` finds nothing, and `😀 NOT hello` finds
/// nothing. A query that is only empty strings matches no rows.
///
/// @param left - the node before
/// @param right - the node after
fn join_implicit(left: Match, right: Match) -> Match {
    if is_empty_phrase(&right) {
        return left;
    }
    if is_empty_phrase(&left) {
        return right;
    }
    Match::And(Box::new(left), Box::new(right))
}

/// Where one phrase was found: a row, and the positions per column.
pub type Hits = BTreeMap<i64, Vec<(usize, Vec<u32>)>>;

/// Runs one query and returns the rows it matched, with the phrase hits.
pub fn evaluate(
    query: &Query,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    columns: usize,
) -> DbResult<(Vec<i64>, Vec<Hits>)> {
    // The segment list is read once for the query, so every term is looked up in the same set.
    let structure = super::segment::read_structure(context, shadows)?;
    let mut hits = Vec::with_capacity(query.phrases.len());
    for phrase in &query.phrases {
        hits.push(phrase_hits(
            phrase, context, shadows, buffer, &structure, columns,
        )?);
    }
    let mut counter = 0usize;
    let rows = walk(&query.root, &hits, &mut counter);
    // The hits belong to the *query*, not to a row: they are one map per
    // phrase, over every row that phrase appears in. Handing back a copy per
    // matched row - which is what this used to do - meant a common term cost a
    // clone of the whole index for every row it found.
    Ok((rows, hits))
}

/// Runs one query and returns only the rows it matched.
///
/// **Positions are decoded when something needs them, and most queries do
/// not.** A doclist entry carries a position list per column, and building the
/// [`Hits`] map allocates one vector per row per column and inserts every row
/// into a `BTreeMap` - work that `SELECT count(*) FROM t WHERE t MATCH 'x'`
/// throws away. Over five hundred documents that was seventy per cent of the
/// query.
///
/// Answers `None` for the queries whose *answer* depends on positions, and the
/// caller runs [`evaluate`] for those instead: a `NEAR`, and a phrase of more
/// than one term, where adjacency is the question. Scoring needs them too, so a
/// ranked plan does not come here.
///
/// @param query - the parsed query
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param columns - how many columns the table declares
pub fn evaluate_rows(
    query: &Query,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    columns: usize,
) -> DbResult<Option<Vec<i64>>> {
    // An anchored phrase is decided by *where* its term sits, so it belongs on
    // the path that decodes positions however few terms it has (task-1979, R4).
    if query
        .phrases
        .iter()
        .any(|phrase| phrase.terms.len() != 1 || phrase.anchored)
    {
        return Ok(None);
    }
    let structure = super::segment::read_structure(context, shadows)?;
    let mut per_phrase = Vec::with_capacity(query.phrases.len());
    for phrase in &query.phrases {
        per_phrase.push(phrase_rows(
            phrase, context, shadows, buffer, &structure, columns,
        )?);
    }
    let mut counter = 0usize;
    Ok(walk_rows(&query.root, &per_phrase, &mut counter))
}

/// Returns the rows a single-term phrase appears in, ascending.
///
/// The same predicate [`phrase_hits`] applies at offset zero - a column the
/// phrase asked for, inside the table, with at least one position - decided
/// without collecting the positions that prove it.
///
/// @param phrase - the phrase, which has exactly one term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
/// @param columns - how many columns the table declares
fn phrase_rows(
    phrase: &Phrase,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
    columns: usize,
) -> DbResult<Vec<i64>> {
    let Some(term) = phrase.terms.first() else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for bytes in term_doclists(term, context, shadows, buffer, structure)? {
        super::doclist_rows(&bytes, &phrase.columns, columns, &mut rows);
    }
    rows.sort_unstable();
    rows.dedup();
    Ok(rows)
}

/// Returns the rows one expression matches, or `None` if it needs positions.
///
/// The counter advances down both sides of every branch whatever the answer,
/// so a `None` from one side still leaves the phrase numbering right for the
/// other - which is what makes the walk safe to abandon half way.
///
/// @param node - the expression
/// @param rows - the rows each phrase matched, in the order they were parsed
/// @param counter - which phrase the walk has reached
fn walk_rows(node: &Match, rows: &[Vec<i64>], counter: &mut usize) -> Option<Vec<i64>> {
    match node {
        Match::Phrase(_) => {
            let index = *counter;
            *counter = counter.saturating_add(1);
            Some(rows.get(index).cloned().unwrap_or_default())
        }
        // `NEAR` is how far apart the phrases are, which is a question about
        // positions and cannot be answered from rowids.
        Match::Near { phrases, .. } => {
            *counter = counter.saturating_add(phrases.len());
            None
        }
        Match::And(left, right) => {
            let left = walk_rows(left, rows, counter);
            let right = walk_rows(right, rows, counter);
            match (left, right) {
                (Some(left), Some(right)) => Some(intersect(&left, &right)),
                _ => None,
            }
        }
        Match::Or(left, right) => {
            let left = walk_rows(left, rows, counter);
            let right = walk_rows(right, rows, counter);
            match (left, right) {
                (Some(left), Some(right)) => Some(union(&left, &right)),
                _ => None,
            }
        }
        Match::Not(left, right) => {
            let left = walk_rows(left, rows, counter);
            let right = walk_rows(right, rows, counter);
            match (left, right) {
                (Some(left), Some(right)) => Some(
                    left.into_iter()
                        .filter(|row| !right.contains(row))
                        .collect(),
                ),
                _ => None,
            }
        }
    }
}

/// Returns the rows one expression matches.
fn walk(node: &Match, hits: &[Hits], counter: &mut usize) -> Vec<i64> {
    match node {
        Match::Phrase(_) => {
            let index = *counter;
            *counter = counter.saturating_add(1);
            hits.get(index)
                .map(|found| found.keys().copied().collect())
                .unwrap_or_default()
        }
        Match::Near { phrases, distance } => {
            let first = *counter;
            *counter = counter.saturating_add(phrases.len());
            near_rows(hits, first, phrases, *distance)
        }
        Match::And(left, right) => {
            let left = walk(left, hits, counter);
            let right = walk(right, hits, counter);
            intersect(&left, &right)
        }
        Match::Or(left, right) => {
            let left = walk(left, hits, counter);
            let right = walk(right, hits, counter);
            union(&left, &right)
        }
        Match::Not(left, right) => {
            let left = walk(left, hits, counter);
            let right = walk(right, hits, counter);
            left.into_iter()
                .filter(|row| !right.contains(row))
                .collect()
        }
    }
}

/// Returns the rows where every phrase of a `NEAR` is close enough to the rest.
fn near_rows(hits: &[Hits], first: usize, phrases: &[Phrase], distance: u32) -> Vec<i64> {
    let count = phrases.len();
    let Some(head) = hits.get(first) else {
        return Vec::new();
    };
    let mut rows: Vec<i64> = head.keys().copied().collect();
    for offset in 1..count {
        let Some(next) = hits.get(first + offset) else {
            return Vec::new();
        };
        rows.retain(|row| next.contains_key(row));
    }
    rows.retain(|row| {
        // Every pair has to be within the distance in the *same* column, which
        // is what makes `NEAR` mean nearness rather than co-occurrence.
        for column in 0..64usize {
            let mut all = true;
            let mut spans: Vec<Vec<u32>> = Vec::with_capacity(count);
            for offset in 0..count {
                let Some(found) = hits.get(first + offset).and_then(|found| found.get(row)) else {
                    all = false;
                    break;
                };
                let Some((_, positions)) = found.iter().find(|(index, _)| *index == column) else {
                    all = false;
                    break;
                };
                spans.push(positions.clone());
            }
            if !all {
                continue;
            }
            let lengths: Vec<u32> = phrases
                .iter()
                .map(|phrase| phrase.terms.len().max(1) as u32)
                .collect();
            if within(&spans, &lengths, distance) {
                return true;
            }
        }
        false
    });
    rows
}

/// Returns whether one position can be picked from each phrase such that no
/// more than `distance` tokens lie between them.
///
/// `NEAR` counts the tokens *between* the phrases, not the gap between their
/// starting positions: `a X Y b` has two tokens between `a` and `b`, so it
/// matches `NEAR(a b, 2)` and not `NEAR(a b, 1)`. With the phrase lengths
/// summed, the test is that the window covering one instance of every phrase is
/// no longer than the phrases themselves plus the distance - which is the rule
/// as SQLite writes it, and generalises to phrases longer than a word.
fn within(spans: &[Vec<u32>], lengths: &[u32], distance: u32) -> bool {
    if spans.is_empty() {
        return false;
    }
    let total: u32 = lengths.iter().take(spans.len()).sum();
    // Every position of every phrase, in order, tagged with the phrase it came
    // from: the smallest window covering all of them is a sliding window over
    // this one sequence, which is exact where picking the nearest to an anchor
    // is only a guess.
    let mut merged: Vec<(u32, usize)> = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        for position in span {
            merged.push((*position, index));
        }
    }
    merged.sort_unstable();
    // **Every index here is bounds-checked**, which is the crate's rule and is
    // not a formality on this function: `merged` is built from positions a
    // stored index supplied, and a sliding window whose two ends are indexed
    // directly is exactly the shape that reads past the end when one of them
    // has been damaged.
    let mut seen = vec![0usize; spans.len()];
    let mut covered = 0usize;
    let mut low = 0usize;
    for high in 0..merged.len() {
        let Some((_, which)) = merged.get(high).copied() else {
            break;
        };
        let Some(count) = seen.get_mut(which) else {
            continue;
        };
        if *count == 0 {
            covered += 1;
        }
        *count += 1;
        while covered == spans.len() {
            let (Some((start, _)), Some((end, last))) =
                (merged.get(low).copied(), merged.get(high).copied())
            else {
                break;
            };
            let window = end
                .saturating_add(lengths.get(last).copied().unwrap_or(1))
                .saturating_sub(start);
            if window <= total.saturating_add(distance) {
                return true;
            }
            let Some((_, dropped)) = merged.get(low).copied() else {
                break;
            };
            let Some(count) = seen.get_mut(dropped) else {
                break;
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                covered -= 1;
            }
            low += 1;
        }
    }
    false
}

/// Returns the rows in both lists.
fn intersect(left: &[i64], right: &[i64]) -> Vec<i64> {
    left.iter()
        .filter(|row| right.contains(row))
        .copied()
        .collect()
}

/// Returns the rows in either list, once each and in order.
fn union(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut out = left.to_vec();
    for row in right {
        if !out.contains(row) {
            out.push(*row);
        }
    }
    out.sort_unstable();
    out
}

/// Returns where one phrase appears, by row and column.
pub(crate) fn phrase_hits(
    phrase: &Phrase,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
    columns: usize,
) -> DbResult<Hits> {
    let mut found: Option<Hits> = None;
    for (offset, term) in phrase.terms.iter().enumerate() {
        let entries = term_entries(term, context, shadows, buffer, structure)?;
        let mut current: Hits = BTreeMap::new();
        for entry in entries {
            for (column, positions) in &entry.columns {
                if !phrase.columns.admits(*column) {
                    continue;
                }
                if *column >= columns {
                    continue;
                }
                // A phrase's later terms have to sit one place further on, so
                // every position is shifted back by its place in the phrase and
                // the intersection is then over the phrase's *start*.
                let shifted: Vec<u32> = positions
                    .iter()
                    .filter(|position| **position >= offset as u32)
                    .map(|position| position - offset as u32)
                    .collect();
                if shifted.is_empty() {
                    continue;
                }
                current
                    .entry(entry.rowid)
                    .or_default()
                    .push((*column, shifted));
            }
        }
        found = Some(match found {
            None => current,
            Some(previous) => meet(&previous, &current),
        });
        if found.as_ref().is_some_and(BTreeMap::is_empty) {
            return Ok(BTreeMap::new());
        }
    }
    let mut found = found.unwrap_or_default();
    if phrase.anchored {
        anchor(&mut found);
    }
    Ok(found)
}

/// Keeps only the hits that start at the first token of their column.
///
/// **What `^` means, and what it did not do (task-1979, R4).** The parser
/// recognised the caret, stepped past it and recorded nothing, so `^alpha`
/// matched `alpha` anywhere in the column - 206 rows where SQLite answered 82.
/// The positions in `found` have already been shifted back by each term's place
/// in the phrase, so the phrase's own start is position zero and the anchor is
/// simply that position surviving.
///
/// @param found - the phrase's hits, narrowed in place
fn anchor(found: &mut Hits) {
    found.retain(|_, columns| {
        columns.retain(|(_, positions)| positions.first() == Some(&0));
        !columns.is_empty()
    });
    for columns in found.values_mut() {
        for (_, positions) in columns.iter_mut() {
            positions.retain(|position| *position == 0);
        }
    }
}

/// Returns the positions two terms share, per row and column.
fn meet(left: &Hits, right: &Hits) -> Hits {
    let mut out = BTreeMap::new();
    for (rowid, columns) in left {
        let Some(other) = right.get(rowid) else {
            continue;
        };
        let mut kept = Vec::new();
        for (column, positions) in columns {
            let Some((_, others)) = other.iter().find(|(index, _)| index == column) else {
                continue;
            };
            let shared: Vec<u32> = positions
                .iter()
                .filter(|position| others.contains(position))
                .copied()
                .collect();
            if !shared.is_empty() {
                kept.push((*column, shared));
            }
        }
        if !kept.is_empty() {
            out.insert(*rowid, kept);
        }
    }
    out
}

/// Returns the doclists one term matches, prefix included.
///
/// One doclist for an ordinary term, with the entries of every segment and of the running
/// transaction laid over one another; one for each term that starts with the prefix for
/// `word*`.
///
/// @param term - the term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
fn term_doclists(
    term: &Term,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
) -> DbResult<Vec<Vec<u8>>> {
    if !term.prefix {
        let bytes =
            super::segment::read_term_bytes(context, shadows, buffer, structure, &term.token)?;
        return Ok(bytes.into_iter().collect());
    }
    // **The staged postings are read from the buffer, not flushed.** A query may not write, so
    // a prefix query that follows a write in the same transaction cannot flush what the write
    // staged. They are laid over what is stored instead, term by term. A prefix reads every
    // term the dictionary holds that starts with it, and the scan starts at the prefix, so it
    // reads only those.
    let terms =
        super::segment::scan_terms(context, shadows, Some(buffer), structure, Some(&term.token))?;
    Ok(terms
        .into_values()
        .map(|entries| super::doclist::encode_doclist(&entries))
        .collect())
}

/// Returns the entries of the rows one term matches, prefix included, in rowid order.
///
/// @param term - the term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
fn term_entries(
    term: &Term,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
) -> DbResult<Vec<DocEntry>> {
    // **One term that is not a prefix is already the answer.** Its entries are in rowid order
    // and name each row once, so there is nothing to merge, and running a merge anyway searched
    // a growing vector once per entry. That is quadratic in the documents a term appears in: a
    // term in five hundred of them cost a hundred and twenty-five thousand comparisons per
    // query, and `evaluate` was seventy per cent of `SELECT count(*) ... MATCH`.
    if !term.prefix {
        return super::segment::read_term_entries(context, shadows, buffer, structure, &term.token);
    }
    let found =
        super::segment::scan_terms(context, shadows, Some(buffer), structure, Some(&term.token))?;
    // A prefix reads several terms, and they can name the same row: merged by rowid
    // rather than searched for it.
    let mut entries: Vec<DocEntry> = found.into_values().flatten().collect();
    entries.sort_by_key(|entry| entry.rowid);
    let mut merged: Vec<DocEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        match merged.last_mut().filter(|last| last.rowid == entry.rowid) {
            Some(found) => {
                for (column, positions) in entry.columns {
                    match found.columns.iter_mut().find(|(index, _)| *index == column) {
                        Some((_, existing)) => {
                            existing.extend(positions);
                            existing.sort_unstable();
                            existing.dedup();
                        }
                        None => found.columns.push((column, positions)),
                    }
                }
            }
            None => merged.push(entry),
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a query with the default tokenizer and two columns.
    fn parse(text: &str) -> Query {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        Query::parse(
            text.as_bytes(),
            &tokenizer,
            &[b"title".to_vec(), b"body".to_vec()],
        )
        .expect("parses")
    }

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

    /// Two words side by side are an `AND`.
    #[test]
    fn juxtaposition_is_and() {
        let query = parse("cat dog");
        assert!(matches!(query.root, Match::And(_, _)));
        assert_eq!(query.phrases.len(), 2);
    }

    /// A quoted string is one phrase however many words it holds.
    #[test]
    fn a_quoted_string_is_one_phrase() {
        let query = parse("\"quick brown fox\"");
        assert_eq!(query.phrases.len(), 1);
        assert_eq!(query.phrases[0].terms.len(), 3);
        assert_eq!(query.phrases[0].terms[1].token, b"brown");
    }

    /// A trailing star makes the last term a prefix.
    #[test]
    fn a_trailing_star_is_a_prefix() {
        let query = parse("cat*");
        assert!(query.phrases[0].terms[0].prefix);
        assert!(!parse("cat").phrases[0].terms[0].prefix);
    }

    /// A column filter restricts what follows it.
    #[test]
    fn a_column_filter_restricts_what_follows() {
        let query = parse("title : cat");
        assert_eq!(query.phrases[0].columns.named, vec![0]);
        let both = parse("body:dog cat");
        assert_eq!(both.phrases[0].columns.named, vec![1]);
        assert!(both.phrases[1].columns.named.is_empty());
    }

    /// A brace list names several columns, and a `-` excludes them.
    #[test]
    fn a_brace_list_names_several_columns() {
        let several = parse("{title body}:cat");
        assert_eq!(several.phrases[0].columns.named, vec![0, 1]);
        assert!(!several.phrases[0].columns.negated);
        let one = parse("{body}:cat");
        assert_eq!(one.phrases[0].columns.named, vec![1]);
        let without = parse("-title:cat");
        assert_eq!(without.phrases[0].columns.named, vec![0]);
        assert!(without.phrases[0].columns.negated);
        assert!(!without.phrases[0].columns.admits(0));
        assert!(without.phrases[0].columns.admits(1));
    }

    /// A caret is kept on the phrase rather than stepped over.
    #[test]
    fn a_caret_anchors_the_phrase() {
        assert!(parse("^cat").phrases[0].anchored);
        assert!(!parse("cat").phrases[0].anchored);
        assert!(parse("title:^cat").phrases[0].anchored);
    }

    /// A quoted phrase can end in a prefix.
    #[test]
    fn a_quoted_phrase_can_end_in_a_prefix() {
        let query = parse("\"quick brown\"*");
        assert_eq!(query.phrases[0].terms.len(), 2);
        assert!(!query.phrases[0].terms[0].prefix);
        assert!(query.phrases[0].terms[1].prefix);
    }

    /// The three strings SQLite calls syntax errors are refused here too.
    #[test]
    fn the_strings_sqlite_refuses_are_refused() {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        let columns = [b"title".to_vec(), b"body".to_vec()];
        for text in [b"co-operate".as_slice(), b"it's".as_slice(), b"\"cat"] {
            assert!(
                Query::parse(text, &tokenizer, &columns).is_err(),
                "{} should be a syntax error",
                String::from_utf8_lossy(text)
            );
        }
    }

    /// A syntax error says what is wrong, rather than "SQL logic error".
    #[test]
    fn a_syntax_error_carries_its_own_message() {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        let failed = Query::parse(b"nope:cat", &tokenizer, &[b"title".to_vec()])
            .expect_err("an unknown column");
        assert!(failed.message().contains("no such column"), "{failed:?}");
    }

    /// A column nobody declared is a query error.
    #[test]
    fn an_unknown_column_is_refused() {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        assert!(Query::parse(b"nope:cat", &tokenizer, &[b"title".to_vec()]).is_err());
    }

    /// The three operators parse, and `NOT` binds tighter than `OR`.
    #[test]
    fn the_operators_parse() {
        assert!(matches!(parse("a OR b").root, Match::Or(_, _)));
        assert!(matches!(parse("a AND b").root, Match::And(_, _)));
        assert!(matches!(parse("a NOT b").root, Match::Not(_, _)));
        assert!(matches!(parse("a OR b NOT c").root, Match::Or(_, _)));
    }

    /// `NEAR` takes its phrases and its distance.
    #[test]
    fn near_takes_a_distance() {
        let query = parse("NEAR(cat dog, 3)");
        let Match::Near { phrases, distance } = query.root else {
            panic!("a NEAR");
        };
        assert_eq!(phrases.len(), 2);
        assert_eq!(distance, 3);
        let default = parse("NEAR(cat dog)");
        assert!(matches!(default.root, Match::Near { distance: 10, .. }));
    }

    /// A group changes what binds to what.
    #[test]
    fn a_group_changes_the_binding() {
        assert!(matches!(parse("(a OR b) AND c").root, Match::And(_, _)));
    }

    /// Nearness counts the tokens *between* two single-word phrases.
    ///
    /// `a X b` has one token between the words, so it is within one and not
    /// within zero: the boundary is the thing worth pinning, because measuring
    /// the gap between the starting positions instead would be off by one and
    /// every `NEAR` in the suite would be one token too strict.
    #[test]
    fn nearness_counts_the_tokens_between() {
        let ones = [1u32, 1];
        assert!(within(&[vec![0], vec![2]], &ones, 3));
        assert!(!within(&[vec![0], vec![9]], &ones, 3));
        assert!(within(&[vec![0, 8], vec![9]], &ones, 2));
        assert!(within(&[vec![0], vec![2]], &ones, 1));
        assert!(!within(&[vec![0], vec![2]], &ones, 0));
        assert!(within(&[vec![0], vec![1]], &ones, 0));
    }

    /// A longer phrase spends its own length, not the distance.
    #[test]
    fn a_phrase_spends_its_own_length() {
        // A two-word phrase starting at 0 ends at 1, so a word at 2 is
        // adjacent to it and within zero.
        assert!(within(&[vec![0], vec![2]], &[2, 1], 0));
        assert!(!within(&[vec![0], vec![3]], &[2, 1], 0));
    }

    /// Three phrases are covered by one window rather than pairwise.
    #[test]
    fn three_phrases_share_one_window() {
        let ones = [1u32, 1, 1];
        assert!(within(&[vec![0], vec![1], vec![2]], &ones, 0));
        assert!(!within(&[vec![0], vec![1], vec![9]], &ones, 0));
        // The far instance of the first phrase is the one that makes it fit,
        // which an anchor on its first position would have missed.
        assert!(within(&[vec![0, 8], vec![9], vec![10]], &ones, 0));
    }
}
