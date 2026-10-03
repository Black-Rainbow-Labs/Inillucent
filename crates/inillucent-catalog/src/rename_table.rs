//! Finding where a table's name is written in stored schema text.
//!
//! Invariant: **a site is reported only where the token names the table.** A
//! column that happens to share the table's name, a word inside a string
//! literal and an alias are all left alone, because `ALTER TABLE ... RENAME TO`
//! has to change the text in exactly the places SQLite changes it and nowhere
//! else.
//!
//! The scan is a small state machine over the lexer's tokens. A keyword such as
//! `FROM`, `JOIN`, `INTO`, `UPDATE`, `REFERENCES` or the `ON` of a `CREATE INDEX`
//! says that the next name token is a table, and the state remembers what kind
//! of place that is, because `ALTER TABLE ... RENAME TO` treats three kinds of
//! place differently when `PRAGMA legacy_alter_table` is on.

use inillucent_base::{error, DbResult};
use inillucent_sql::lexer::{Lexer, Punctuator, Span, Token, TokenKind};

use crate::rename::{bare_word, unquoted};

/// What kind of place a table name was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    /// The name the statement is about: `CREATE TABLE name`, or the `ON name`
    /// of a `CREATE INDEX` or `CREATE TRIGGER`.
    Head,
    /// The parent of a `REFERENCES name (...)` clause.
    Reference,
    /// A table read or written by a view or by a trigger's body.
    Body,
}

/// One place a table's name is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// Where the name token is.
    pub(crate) span: Span,
    /// What kind of place it is.
    pub(crate) site: Site,
}

/// Which statement the text is, decided from its first words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Statement {
    /// `CREATE INDEX`, whose first `ON` names the table.
    Index,
    /// `CREATE TRIGGER`, whose first `ON` names the table.
    Trigger,
    /// Anything else.
    Other,
}

/// The state of one scan.
struct Scan<'a> {
    sql: &'a [u8],
    tokens: &'a [Token],
    folded: &'a [u8],
    statement: Statement,
    /// What the next name token would be, when a keyword said a table is next.
    pending: Option<Site>,
    /// Per parenthesis depth: whether a comma continues a `FROM` list.
    in_from: Vec<bool>,
    head_on_done: bool,
    /// Whether the table after the latest `INTO` is the table being renamed.
    last_insert_is_target: bool,
    /// Whether some alias is spelled like the table, which makes a qualifier
    /// `name.column` ambiguous.
    aliased: bool,
    found: Vec<Found>,
    /// Sites that are `name.column` qualifiers, kept apart so an alias can
    /// cancel them.
    qualifiers: Vec<Found>,
}

/// Lexes the whole text into tokens, without the end marker.
///
/// @param sql - the stored text
pub(crate) fn lex_all(sql: &[u8]) -> DbResult<Vec<Token>> {
    let mut lexer = Lexer::at(sql, 0);
    let mut tokens = Vec::new();
    loop {
        let token = lexer
            .next_token()
            .map_err(|reason| error::corrupt(format!("schema SQL does not lex: {reason:?}")))?;
        if token.kind == TokenKind::EndOfInput {
            return Ok(tokens);
        }
        tokens.push(token);
    }
}

/// Returns every place a table's name is written in one stored statement.
///
/// @param sql - the stored `CREATE` text
/// @param folded - the table's folded name
pub(crate) fn table_sites(sql: &[u8], folded: &[u8]) -> DbResult<Vec<Found>> {
    let tokens = lex_all(sql)?;
    let statement = statement_of(sql, &tokens);
    let mut scan = Scan {
        sql,
        tokens: &tokens,
        folded,
        statement,
        pending: None,
        in_from: vec![false],
        head_on_done: false,
        last_insert_is_target: false,
        aliased: false,
        found: Vec::new(),
        qualifiers: Vec::new(),
    };
    for at in 0..tokens.len() {
        scan.step(at);
    }
    let mut all = scan.found;
    if !scan.aliased {
        all.extend(scan.qualifiers);
    }
    all.sort_by_key(|found| found.span.start);
    Ok(all)
}

/// Decides which kind of statement the text is.
///
/// @param sql - the stored text
/// @param tokens - its tokens
fn statement_of(sql: &[u8], tokens: &[Token]) -> Statement {
    for token in tokens.iter().take(6) {
        match bare_word(sql, *token).as_deref() {
            Some(b"index") => return Statement::Index,
            Some(b"trigger") => return Statement::Trigger,
            Some(b"table") | Some(b"view") => return Statement::Other,
            _ => {}
        }
    }
    Statement::Other
}

impl<'a> Scan<'a> {
    /// Returns the token at a position, when there is one.
    fn at(&self, index: usize) -> Option<Token> {
        self.tokens.get(index).copied()
    }

    /// Returns whether the token at a position is the given punctuator.
    ///
    /// @param index - the position, which may be out of range
    /// @param punctuator - what it should be
    fn is_punct(&self, index: Option<usize>, punctuator: Punctuator) -> bool {
        index
            .and_then(|at| self.at(at))
            .is_some_and(|token| token.is(punctuator))
    }

    /// Returns the lowercase bare word at a position.
    ///
    /// @param index - the position, which may be out of range
    fn word_at(&self, index: Option<usize>) -> Option<Vec<u8>> {
        index
            .and_then(|at| self.at(at))
            .and_then(|token| bare_word(self.sql, token))
    }

    /// Handles one token.
    ///
    /// @param index - the token's position
    fn step(&mut self, index: usize) {
        let Some(token) = self.at(index) else {
            return;
        };
        match token.kind {
            TokenKind::Punctuator(Punctuator::LeftParen) => {
                let keep = self.pending == Some(Site::Body);
                self.in_from.push(keep);
                if !keep {
                    self.pending = None;
                }
            }
            TokenKind::Punctuator(Punctuator::RightParen) => {
                if self.in_from.len() > 1 {
                    self.in_from.pop();
                }
                self.pending = None;
            }
            TokenKind::Punctuator(Punctuator::Comma) => {
                let continues = self.in_from.last().copied().unwrap_or(false);
                self.pending = continues.then_some(Site::Body);
            }
            TokenKind::Punctuator(Punctuator::Semicolon) => {
                self.in_from = vec![false];
                self.pending = None;
            }
            TokenKind::Punctuator(Punctuator::Dot) => {}
            TokenKind::Identifier { keyword, quote } => {
                let hard = keyword.is_some_and(|word| !word.may_be_name())
                    && quote == inillucent_sql::lexer::QuoteForm::Bare;
                match hard {
                    true => self.keyword(index),
                    false => self.name(index),
                }
            }
            _ => self.pending = None,
        }
    }

    /// Handles a keyword, which decides what the next name token is.
    ///
    /// @param index - the keyword's position
    fn keyword(&mut self, index: usize) {
        let word = self.word_at(Some(index)).unwrap_or_default();
        let before = index.checked_sub(1);
        match word.as_slice() {
            b"from" => {
                if let Some(last) = self.in_from.last_mut() {
                    *last = true;
                }
                self.pending = Some(Site::Body);
            }
            b"join" | b"into" | b"update" => self.pending = Some(Site::Body),
            b"references" => self.pending = Some(Site::Reference),
            b"on" => self.pending = self.on_site(),
            b"table" if index < 6 => self.pending = Some(Site::Head),
            b"exists" if index < 10 && self.word_at(before).as_deref() == Some(b"not") => {
                self.pending = Some(Site::Head)
            }
            b"or" if self.word_at(before).as_deref() == Some(b"update") => {}
            b"where" | b"group" | b"having" | b"order" | b"limit" | b"union" | b"intersect"
            | b"except" | b"select" | b"values" | b"window" | b"returning" | b"set" | b"begin" => {
                if let Some(last) = self.in_from.last_mut() {
                    *last = false;
                }
                self.pending = None;
            }
            _ => self.pending = None,
        }
    }

    /// Decides what the name after `ON` is.
    ///
    /// Only the first `ON` of a `CREATE INDEX` or `CREATE TRIGGER` names the
    /// table. Every other `ON` introduces a join condition.
    fn on_site(&mut self) -> Option<Site> {
        let header = matches!(self.statement, Statement::Index | Statement::Trigger);
        if header && !self.head_on_done && self.in_from.len() == 1 {
            self.head_on_done = true;
            return Some(Site::Head);
        }
        None
    }

    /// Handles a token that is a name.
    ///
    /// @param index - the name's position
    fn name(&mut self, index: usize) {
        let Some(token) = self.at(index) else {
            return;
        };
        let spelled = unquoted(token.span.slice(self.sql)).to_ascii_lowercase();
        let matches = spelled == self.folded;
        let before = index.checked_sub(1);
        let followed_by_dot = self.is_punct(index.checked_add(1), Punctuator::Dot);
        if let Some(site) = self.pending {
            if followed_by_dot {
                // A schema qualifier: the table is the next name.
                return;
            }
            if matches {
                self.found.push(Found {
                    span: token.span,
                    site,
                });
            }
            if self.word_at(before).as_deref() == Some(b"into") {
                self.last_insert_is_target = matches;
            }
            self.pending = self.conflict_word_keeps_pending(index);
            return;
        }
        // **`excluded` in an upsert is rewritten like the table it stands for.**
        // SQLite resolves `excluded.col` to the insert's target table, so a
        // rename of that table writes its new name where `excluded` was.
        if spelled == b"excluded" && followed_by_dot && self.last_insert_is_target {
            self.qualifiers.push(Found {
                span: token.span,
                site: Site::Body,
            });
            return;
        }
        let after_dot = self.is_punct(before, Punctuator::Dot);
        if matches && followed_by_dot && (!after_dot || self.is_database_then_table(index)) {
            self.qualifiers.push(Found {
                span: token.span,
                site: Site::Body,
            });
            return;
        }
        if matches && self.word_at(before).as_deref() == Some(b"as") {
            self.aliased = true;
        }
    }

    /// Returns whether the name at a position is the middle of `db.table.column`.
    ///
    /// @param index - the name's position
    fn is_database_then_table(&self, index: usize) -> bool {
        let two_back = index.checked_sub(2);
        self.is_punct(index.checked_sub(1), Punctuator::Dot)
            && matches!(
                two_back.and_then(|at| self.at(at)).map(|token| token.kind),
                Some(TokenKind::Identifier { .. })
            )
    }

    /// Keeps a pending table position alive across `UPDATE OR REPLACE`.
    ///
    /// `REPLACE`, `IGNORE`, `ABORT`, `FAIL` and `ROLLBACK` are words that may
    /// also be names, so the scan would otherwise take the conflict word for
    /// the table and lose the real one.
    ///
    /// @param index - the position of the name just examined
    fn conflict_word_keeps_pending(&self, index: usize) -> Option<Site> {
        let word = self.word_at(Some(index))?;
        let conflict = matches!(
            word.as_slice(),
            b"replace" | b"ignore" | b"abort" | b"fail" | b"rollback"
        );
        let after_or = self.word_at(index.checked_sub(1)).as_deref() == Some(b"or");
        (conflict && after_or).then_some(Site::Body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the text of every site found, in order.
    fn sites(sql: &str, folded: &str) -> Vec<(String, Site)> {
        table_sites(sql.as_bytes(), folded.as_bytes())
            .expect("it scans")
            .iter()
            .map(|found| {
                (
                    String::from_utf8_lossy(found.span.slice(sql.as_bytes())).into_owned(),
                    found.site,
                )
            })
            .collect()
    }

    /// A view's FROM list, join, qualifier and subquery are all sites.
    #[test]
    fn a_view_names_its_table_in_every_place_it_reads_it() {
        let sql = "CREATE VIEW pv AS SELECT p.name, c.id FROM p JOIN c ON c.pid = p.id WHERE EXISTS (SELECT 1 FROM p AS z)";
        let found = sites(sql, "p");
        assert_eq!(found.len(), 4);
        assert!(found.iter().all(|(_, site)| *site == Site::Body));
    }

    /// A column that shares the table's name is not a site.
    #[test]
    fn a_column_called_like_the_table_is_left_alone() {
        let sql = "CREATE VIEW v AS SELECT a, p FROM c, p WHERE p > 1";
        let found = sites(sql, "p");
        assert_eq!(found.len(), 1);
    }

    /// The `ON` of a join condition is not the table of an index.
    #[test]
    fn only_the_first_on_of_an_index_names_the_table() {
        assert_eq!(
            sites("CREATE INDEX i ON p(a)", "p"),
            vec![("p".to_string(), Site::Head)]
        );
        assert!(sites("CREATE VIEW v AS SELECT 1 FROM c JOIN d ON p = 1", "p").is_empty());
    }

    /// A reference clause is its own kind of site.
    #[test]
    fn a_reference_is_its_own_kind_of_site() {
        let found = sites(
            "CREATE TABLE c(pid REFERENCES p(id), x, FOREIGN KEY(x) REFERENCES p(id))",
            "p",
        );
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|(_, site)| *site == Site::Reference));
    }
}
