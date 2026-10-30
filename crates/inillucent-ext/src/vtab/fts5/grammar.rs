//! The FTS5 query parser, written to read what the pinned release's parser reads.
//!
//! Invariant: **a query is accepted or refused exactly as SQLite's `fts5parse.y` accepts or
//! refuses it, with the same message.** The grammar is small and its quirks are all visible in
//! what people type: a word followed by `(` is the start of a `NEAR` whatever the word is, so
//! `one (three)` is `fts5: syntax error near "one"` and is found only after the `)`; a group in
//! parentheses is an expression and cannot be followed by a phrase; `NOT` binds tighter than
//! `AND`, and two phrases side by side bind tighter than `NOT`.
//!
//! The parser is a shift and reduce machine over an explicit stack, because three of the results
//! depend on the stack and not on the grammar alone. The message names the token the machine
//! could not shift. A semantic error such as `no such column` is raised when the reduction that
//! checks it runs, which can be before a later syntax error and then wins. And the stack holds at
//! most 99 entries, after which SQLite stops with `fts5: parser stack overflow`, which a query
//! nested a hundred groups deep reaches. Reductions that SQLite performs as part of shifting a
//! token (`star_opt ::= STAR`, `colset ::= MINUS STRING` and the closing tokens) are done in
//! [`Machine::shift`], so the stack is the same height at the same moments.
//!
//! The rules are `fts5parse.y`'s, numbered there from 0:
//!
//! ```text
//! colset     ::= MINUS LCP colsetlist RCP | LCP colsetlist RCP | STRING | MINUS STRING
//! colsetlist ::= colsetlist STRING | STRING
//! expr       ::= expr AND expr | expr OR expr | expr NOT expr
//!              | colset COLON LP expr RP | LP expr RP | exprlist
//! exprlist   ::= cnearset | exprlist cnearset
//! cnearset   ::= nearset | colset COLON nearset
//! nearset    ::= phrase | CARET phrase | STRING LP nearphrases neardist_opt RP
//! nearphrases ::= phrase | nearphrases phrase
//! neardist_opt ::= | COMMA STRING
//! phrase     ::= phrase PLUS STRING star_opt | STRING star_opt
//! star_opt   ::= STAR |
//! ```

mod reduce;
#[cfg(test)]
mod tests;

use inillucent_base::DbResult;

use super::expr::{syntax, ColumnFilter, Match, Phrase, Query, Term};
use super::tokenize::Tokenizer;

/// The most entries SQLite's parser stack holds, past which it refuses the query.
const STACK_LIMIT: usize = 99;
/// The deepest an expression tree may be.
const MAX_DEPTH: u32 = 256;
/// How far apart `NEAR` phrases may be when the query does not say.
const DEFAULT_NEAR: u32 = 10;

/// The kind of one token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Eof,
    Lp,
    Rp,
    Lcp,
    Rcp,
    Colon,
    Comma,
    Plus,
    Star,
    Minus,
    Caret,
    Str,
    And,
    Or,
    Not,
}

/// One token, with the bytes it was read from.
#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    text: Vec<u8>,
}

/// Reports whether a byte may be part of a bareword, as `sqlite3Fts5IsBareword` does.
///
/// @param byte - one byte of the query
fn is_bareword(byte: u8) -> bool {
    byte >= 0x80 || byte.is_ascii_alphanumeric() || byte == b'_' || byte == 0x1A
}

/// Reads the next token of a query, as `fts5ExprGetToken` does.
///
/// Only a space, a tab, a line feed and a carriage return are white space. Any other byte that
/// is not an operator, a quote or part of a word is `fts5: syntax error near` that byte. A quoted
/// string that is never closed is `unterminated string`.
///
/// @param text - the query, up to its first NUL byte
/// @param at - where to start, advanced past the token
fn next_token(text: &[u8], at: &mut usize) -> Result<Token, String> {
    while matches!(text.get(*at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *at += 1;
    }
    let start = *at;
    let Some(byte) = text.get(start).copied() else {
        return Ok(Token {
            kind: Kind::Eof,
            text: Vec::new(),
        });
    };
    let single = |kind: Kind, at: &mut usize| {
        *at = start + 1;
        Ok(Token {
            kind,
            text: vec![byte],
        })
    };
    match byte {
        b'(' => single(Kind::Lp, at),
        b')' => single(Kind::Rp, at),
        b'{' => single(Kind::Lcp, at),
        b'}' => single(Kind::Rcp, at),
        b':' => single(Kind::Colon, at),
        b',' => single(Kind::Comma, at),
        b'+' => single(Kind::Plus, at),
        b'*' => single(Kind::Star, at),
        b'-' => single(Kind::Minus, at),
        b'^' => single(Kind::Caret, at),
        b'"' => quoted_token(text, at),
        _ => bare_token(text, at),
    }
}

/// Reads a string in double quotes, where a doubled quote stands for one.
///
/// @param text - the query
/// @param at - the position of the opening quote, advanced past the closing one
fn quoted_token(text: &[u8], at: &mut usize) -> Result<Token, String> {
    let start = *at;
    let mut end = start + 1;
    loop {
        match text.get(end) {
            None => return Err("unterminated string".to_string()),
            Some(b'"') if text.get(end + 1) == Some(&b'"') => end += 2,
            Some(b'"') => break,
            Some(_) => end += 1,
        }
    }
    *at = end + 1;
    Ok(Token {
        kind: Kind::Str,
        text: text.get(start..=end).unwrap_or_default().to_vec(),
    })
}

/// Reads a bareword, or the keyword it spells.
///
/// @param text - the query
/// @param at - the position of the first byte, advanced past the word
fn bare_token(text: &[u8], at: &mut usize) -> Result<Token, String> {
    let start = *at;
    let first = text.get(start).copied().unwrap_or(0);
    if !is_bareword(first) {
        let near = String::from_utf8_lossy(&[first]).into_owned();
        return Err(format!("fts5: syntax error near \"{near}\""));
    }
    let mut end = start + 1;
    while text.get(end).is_some_and(|byte| is_bareword(*byte)) {
        end += 1;
    }
    *at = end;
    let word = text.get(start..end).unwrap_or_default();
    let kind = match word {
        b"OR" => Kind::Or,
        b"NOT" => Kind::Not,
        b"AND" => Kind::And,
        _ => Kind::Str,
    };
    Ok(Token {
        kind,
        text: word.to_vec(),
    })
}

/// A node being built, with the height the depth limit is checked against.
///
/// `kids` holds the height of each child of an `AND` or an `OR`, because a child of the same
/// kind is merged into its parent and the depth is then measured over the merged children.
#[derive(Clone, Debug)]
struct Node {
    tree: Match,
    height: u32,
    kids: Vec<u32>,
}

/// The phrases of a `NEAR` group, and how far apart they may be.
#[derive(Clone, Debug)]
struct NearSet {
    phrases: Vec<Phrase>,
    distance: u32,
}

/// One entry of the parser's stack: a token that has not been reduced, or a value.
#[derive(Clone, Debug)]
enum Entry {
    Tok(Token),
    Colset(Vec<usize>),
    ColsetList(Vec<usize>),
    Expr(Node),
    ExprList(Node),
    CNearset(Node),
    Nearset(NearSet),
    NearPhrases(NearSet),
    Phrase(Phrase),
    StarOpt(bool),
    NearDist(Option<Token>),
}

/// What the machine does with the next token.
#[derive(Clone, Copy, Debug)]
enum Action {
    Shift,
    Reduce(Rule),
    Error,
    Accept,
}

/// The grammar rules the machine reduces by.
#[derive(Clone, Copy, Debug)]
enum Rule {
    ColsetStr,
    ColsetMinusStr,
    ColsetBraces,
    ColsetMinusBraces,
    ColsetListFirst,
    ColsetListMore,
    StarEmpty,
    PhraseNew,
    PhraseAppend,
    NearDistComma,
    NearDistEmpty,
    NearphrasesFirst,
    NearphrasesMore,
    NearsetPhrase,
    NearsetCaret,
    NearsetFunction,
    CNearset,
    CNearsetColset,
    ExprListFirst,
    ExprListMore,
    ExprFromList,
    ExprBinary,
    ExprParen,
    ExprColsetParen,
}

/// Where a STRING token sits, which decides what it can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// Where an expression may start: a word here may be a column filter or a function name.
    ExprStart,
    /// After `colset COLON`: a word here may be a function name.
    AfterColon,
    /// Inside `NEAR(` or after `^` or `+`: only a phrase can follow.
    PhraseOnly,
    /// Inside the braces of a column list.
    InBraces,
    /// After a `-`.
    AfterMinus,
    /// After the `,` of a `NEAR` distance.
    AfterComma,
}

/// Returns whether a token can start a `cnearset`, which is a phrase with an optional filter.
///
/// @param kind - the token
fn starts_cnearset(kind: Kind) -> bool {
    matches!(kind, Kind::Str | Kind::Caret | Kind::Lcp | Kind::Minus)
}

/// Returns how tightly a binary operator binds.
///
/// @param kind - the operator token
fn precedence(kind: Kind) -> u8 {
    match kind {
        Kind::Or => 1,
        Kind::And => 2,
        _ => 3,
    }
}

/// Returns whether a token is `AND`, `OR` or `NOT`.
///
/// @param kind - the token
fn is_operator(kind: Kind) -> bool {
    matches!(kind, Kind::And | Kind::Or | Kind::Not)
}

/// Removes the quotes from a string token and collapses each doubled quote, as `Dequote` does.
///
/// @param text - the token's bytes
fn dequote(text: &[u8]) -> Vec<u8> {
    if text.first() != Some(&b'"') {
        return text.to_vec();
    }
    let inner = text
        .get(1..text.len().saturating_sub(1))
        .unwrap_or_default();
    let mut out = Vec::with_capacity(inner.len());
    let mut at = 0usize;
    while let Some(byte) = inner.get(at) {
        out.push(*byte);
        at += if *byte == b'"' { 2 } else { 1 };
    }
    out
}

/// The parser: its stack, the table it parses for, and the first error it met.
struct Machine<'a> {
    stack: Vec<Entry>,
    tokenizer: &'a Tokenizer,
    columns: &'a [Vec<u8>],
    error: Option<String>,
    root: Option<Match>,
}

/// Parses one query into a tree.
///
/// @param text - the `MATCH` operand
/// @param tokenizer - the table's tokenizer
/// @param columns - the table's column names
/// @param column - the column the operand was written against, when it was
pub(super) fn parse_query(
    text: &[u8],
    tokenizer: &Tokenizer,
    columns: &[Vec<u8>],
    column: Option<usize>,
) -> DbResult<Query> {
    let end = text
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(text.len());
    let text = text.get(..end).unwrap_or_default();
    let mut machine = Machine {
        stack: Vec::new(),
        tokenizer,
        columns,
        error: None,
        root: None,
    };
    let mut at = 0usize;
    loop {
        let token = next_token(text, &mut at).map_err(syntax)?;
        let last = token.kind == Kind::Eof;
        machine.feed(token);
        if let Some(said) = machine.error.take() {
            return Err(syntax(said));
        }
        if last {
            break;
        }
    }
    let Some(mut root) = machine.root.take() else {
        return Err(syntax("fts5: syntax error near \"\""));
    };
    if let Some(column) = column {
        reduce::apply_colset(&mut root, &[column]);
    }
    let mut phrases = Vec::new();
    number_phrases(&mut root, &mut phrases);
    Ok(Query { root, phrases })
}

/// Numbers the phrases of a finished tree in the order they were written.
///
/// @param node - the tree, whose leaves are given their first phrase's number
/// @param phrases - every phrase met so far, which the leaves' phrases are added to
fn number_phrases(node: &mut Match, phrases: &mut Vec<Phrase>) {
    match node {
        Match::Near {
            first,
            phrases: own,
            ..
        }
        | Match::Nothing {
            first,
            phrases: own,
        } => {
            *first = phrases.len();
            phrases.extend(own.iter().cloned());
        }
        Match::And(children) | Match::Or(children) => {
            for child in children {
                number_phrases(child, phrases);
            }
        }
        Match::Not(left, right) => {
            number_phrases(left, phrases);
            number_phrases(right, phrases);
        }
    }
}

impl Machine<'_> {
    /// Returns the entry `depth` places below the top, where 0 is the top.
    fn entry(&self, depth: usize) -> Option<&Entry> {
        self.stack
            .len()
            .checked_sub(depth + 1)
            .and_then(|at| self.stack.get(at))
    }

    /// Returns the kind of the token entry `depth` places below the top, if it is one.
    fn kind_at(&self, depth: usize) -> Option<Kind> {
        match self.entry(depth) {
            Some(Entry::Tok(token)) => Some(token.kind),
            _ => None,
        }
    }

    /// Records the first error and ignores every later one, as SQLite's `ParseError` does.
    ///
    /// @param said - the message
    fn fail(&mut self, said: String) {
        if self.error.is_none() {
            self.error = Some(said);
        }
    }

    /// Pushes an entry, refusing the query when the stack would hold more than SQLite's does.
    ///
    /// @param entry - the entry
    fn push(&mut self, entry: Entry) {
        if self.stack.len() >= STACK_LIMIT {
            self.fail("fts5: parser stack overflow".to_string());
            return;
        }
        self.stack.push(entry);
    }

    /// Gives the machine one token, reducing as far as the token allows and then shifting it.
    ///
    /// @param token - the next token of the query
    fn feed(&mut self, token: Token) {
        loop {
            if self.error.is_some() {
                return;
            }
            match self.action(token.kind) {
                Action::Shift => return self.shift(token),
                Action::Reduce(rule) => self.reduce(rule),
                Action::Accept => return self.accept(),
                Action::Error => {
                    let near = String::from_utf8_lossy(&token.text).into_owned();
                    return self.fail(format!("fts5: syntax error near \"{near}\""));
                }
            }
        }
    }

    /// Finishes the parse: the one expression on the stack is the query.
    fn accept(&mut self) {
        if let Some(Entry::Expr(node)) = self.stack.pop() {
            self.root = Some(node.tree);
        }
    }

    /// Works out where a STRING sits, given how many entries are on the stack beneath it.
    ///
    /// @param beneath - how many places below the top of the stack the entry under the STRING is
    fn place(&self, beneath: usize) -> Place {
        match self.entry(beneath) {
            Some(Entry::Tok(token)) => match token.kind {
                Kind::Colon => Place::AfterColon,
                Kind::Caret | Kind::Plus => Place::PhraseOnly,
                Kind::Lcp => Place::InBraces,
                Kind::Minus => Place::AfterMinus,
                Kind::Comma => Place::AfterComma,
                Kind::Lp if self.kind_at(beneath + 1) == Some(Kind::Str) => Place::PhraseOnly,
                _ => Place::ExprStart,
            },
            Some(Entry::ColsetList(_)) => Place::InBraces,
            Some(Entry::NearPhrases(_)) => Place::PhraseOnly,
            _ => Place::ExprStart,
        }
    }

    /// Chooses what to do with the next token given what is on the stack.
    ///
    /// @param look - the kind of the next token
    fn action(&self, look: Kind) -> Action {
        let Some(top) = self.stack.last() else {
            return Self::accept_if(starts_cnearset(look) || look == Kind::Lp);
        };
        match top {
            Entry::Tok(token) => self.after_token(token.kind, look),
            Entry::Expr(_) => self.after_expr(look),
            Entry::Phrase(_) => self.after_phrase(look),
            Entry::StarOpt(_) => Action::Reduce(if self.kind_at(2) == Some(Kind::Plus) {
                Rule::PhraseAppend
            } else {
                Rule::PhraseNew
            }),
            Entry::Nearset(_) => Action::Reduce(if self.kind_at(1) == Some(Kind::Colon) {
                Rule::CNearsetColset
            } else {
                Rule::CNearset
            }),
            Entry::CNearset(_) => {
                Action::Reduce(if matches!(self.entry(1), Some(Entry::ExprList(_))) {
                    Rule::ExprListMore
                } else {
                    Rule::ExprListFirst
                })
            }
            Entry::ExprList(_) if starts_cnearset(look) => Action::Shift,
            Entry::ExprList(_) => Action::Reduce(Rule::ExprFromList),
            Entry::Colset(_) => Self::accept_if(look == Kind::Colon),
            Entry::ColsetList(_) => Self::accept_if(matches!(look, Kind::Str | Kind::Rcp)),
            Entry::NearPhrases(_) => match look {
                Kind::Str | Kind::Comma => Action::Shift,
                Kind::Rp => Action::Reduce(Rule::NearDistEmpty),
                _ => Action::Error,
            },
            Entry::NearDist(_) => Self::accept_if(look == Kind::Rp),
        }
    }

    /// Returns a shift when a token is acceptable and an error when it is not.
    ///
    /// @param acceptable - whether the token may be shifted here
    fn accept_if(acceptable: bool) -> Action {
        if acceptable {
            Action::Shift
        } else {
            Action::Error
        }
    }

    /// Chooses the action when a token that has not been reduced is on top of the stack.
    ///
    /// @param top - the kind of the token on top
    /// @param look - the kind of the next token
    fn after_token(&self, top: Kind, look: Kind) -> Action {
        match top {
            Kind::Str => self.after_string(look),
            Kind::Lp if self.kind_at(1) == Some(Kind::Str) => Self::accept_if(look == Kind::Str),
            Kind::Lp | Kind::And | Kind::Or | Kind::Not => {
                Self::accept_if(starts_cnearset(look) || look == Kind::Lp)
            }
            Kind::Colon => {
                // `colset COLON LP expr RP` is an expression, so it cannot continue a run of phrases.
                let continues_run = matches!(self.entry(2), Some(Entry::ExprList(_)));
                Self::accept_if(
                    matches!(look, Kind::Str | Kind::Caret) || (look == Kind::Lp && !continues_run),
                )
            }
            Kind::Minus => Self::accept_if(matches!(look, Kind::Str | Kind::Lcp)),
            Kind::Caret | Kind::Lcp | Kind::Comma | Kind::Plus => {
                Self::accept_if(look == Kind::Str)
            }
            _ => Action::Error,
        }
    }

    /// Chooses the action when a STRING that has not been reduced is on top.
    ///
    /// A word is a column filter when a colon follows it where a filter may start, the name of a
    /// `NEAR` when a parenthesis follows it where one may start, and a phrase otherwise.
    ///
    /// @param look - the kind of the next token
    fn after_string(&self, look: Kind) -> Action {
        let place = self.place(1);
        let filter_ok = place == Place::ExprStart;
        let function_ok = matches!(place, Place::ExprStart | Place::AfterColon);
        match look {
            Kind::Colon if filter_ok => Action::Reduce(Rule::ColsetStr),
            Kind::Lp if function_ok => Action::Shift,
            Kind::Star => Action::Shift,
            _ => Action::Reduce(Rule::StarEmpty),
        }
    }

    /// Chooses the action when an expression is on top of the stack.
    ///
    /// @param look - the kind of the next token
    fn after_expr(&self, look: Kind) -> Action {
        match self.kind_at(1) {
            None => match look {
                Kind::Eof => Action::Accept,
                _ => Self::accept_if(is_operator(look)),
            },
            Some(Kind::Lp) => Self::accept_if(look == Kind::Rp || is_operator(look)),
            Some(operator) if is_operator(look) && precedence(operator) < precedence(look) => {
                Action::Shift
            }
            Some(_) => Action::Reduce(Rule::ExprBinary),
        }
    }

    /// Chooses the action when a phrase is on top of the stack.
    ///
    /// @param look - the kind of the next token
    fn after_phrase(&self, look: Kind) -> Action {
        if look == Kind::Plus {
            return Action::Shift;
        }
        match (self.entry(1), self.kind_at(1)) {
            (_, Some(Kind::Caret)) => Action::Reduce(Rule::NearsetCaret),
            (Some(Entry::NearPhrases(_)), _) => Action::Reduce(Rule::NearphrasesMore),
            (_, Some(Kind::Lp)) if self.kind_at(2) == Some(Kind::Str) => {
                Action::Reduce(Rule::NearphrasesFirst)
            }
            _ => Action::Reduce(Rule::NearsetPhrase),
        }
    }

    /// Shifts a token, and performs the reduction SQLite does as part of shifting it.
    ///
    /// @param token - the token
    fn shift(&mut self, token: Token) {
        let kind = token.kind;
        let place = (kind == Kind::Str).then(|| self.place(0));
        self.push(Entry::Tok(token));
        if self.error.is_some() {
            return;
        }
        match (kind, place) {
            (Kind::Rp, _) => self.reduce_close(),
            (Kind::Rcp, _) => self.reduce_braces(),
            (Kind::Star, _) => {
                self.stack.pop();
                self.stack.push(Entry::StarOpt(true));
            }
            (Kind::Str, Some(Place::InBraces)) => {
                let first = !matches!(self.entry(1), Some(Entry::ColsetList(_)));
                self.reduce(if first {
                    Rule::ColsetListFirst
                } else {
                    Rule::ColsetListMore
                });
            }
            (Kind::Str, Some(Place::AfterMinus)) => self.reduce(Rule::ColsetMinusStr),
            (Kind::Str, Some(Place::AfterComma)) => self.reduce(Rule::NearDistComma),
            _ => {}
        }
    }

    /// Reduces the rule a `)` just shifted completes.
    fn reduce_close(&mut self) {
        if matches!(self.entry(1), Some(Entry::NearDist(_))) {
            return self.reduce(Rule::NearsetFunction);
        }
        let colset_first =
            matches!(self.entry(4), Some(Entry::Colset(_))) && self.kind_at(3) == Some(Kind::Colon);
        self.reduce(if colset_first {
            Rule::ExprColsetParen
        } else {
            Rule::ExprParen
        });
    }

    /// Reduces the column list a `}` just closed.
    fn reduce_braces(&mut self) {
        let minus = self.kind_at(3) == Some(Kind::Minus);
        self.reduce(if minus {
            Rule::ColsetMinusBraces
        } else {
            Rule::ColsetBraces
        });
    }

    /// Pops `count` entries off the stack, returning them in the order they were pushed.
    ///
    /// @param count - how many entries
    fn pop(&mut self, count: usize) -> Vec<Entry> {
        let at = self.stack.len().saturating_sub(count);
        self.stack.split_off(at)
    }

    /// Performs one reduction.
    ///
    /// @param rule - the rule
    fn reduce(&mut self, rule: Rule) {
        match rule {
            Rule::StarEmpty | Rule::NearDistEmpty => self.reduce_empty(rule),
            Rule::ColsetStr
            | Rule::ColsetMinusStr
            | Rule::ColsetBraces
            | Rule::ColsetMinusBraces
            | Rule::ColsetListFirst
            | Rule::ColsetListMore => self.reduce_colset(rule),
            Rule::PhraseNew
            | Rule::PhraseAppend
            | Rule::NearDistComma
            | Rule::NearphrasesFirst
            | Rule::NearphrasesMore
            | Rule::NearsetPhrase
            | Rule::NearsetCaret
            | Rule::NearsetFunction => self.reduce_phrase(rule),
            _ => self.reduce_expression(rule),
        }
    }

    /// Performs a reduction by a rule with nothing on its right hand side.
    ///
    /// @param rule - the rule
    fn reduce_empty(&mut self, rule: Rule) {
        let entry = match rule {
            Rule::StarEmpty => Entry::StarOpt(false),
            _ => Entry::NearDist(None),
        };
        self.push(entry);
    }
}
