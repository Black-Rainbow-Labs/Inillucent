//! `CREATE`, `DROP` and `ALTER`.
//!
//! Invariant: a column constraint and a table constraint are different nodes
//! even where they mean the same thing, and both keep the order they were
//! written in. The catalog reproduces a table's declaration from this tree when
//! it writes `sqlite_schema`, and a reordered constraint list is a schema that
//! no longer round-trips.

use inillucent_base::limits::Limit;

use super::Parser;
use crate::ast::{
    AlterAction, ColumnConstraint, ColumnDef, CreateTableBody, Expr, ForeignKeyAction,
    ForeignKeyClause, IndexedColumn, Literal, NameId, ObjectKind, ReferentialAction, SortOrder,
    Statement, TableConstraint, TriggerEvent, TriggerTime,
};
use crate::diagnostic::{ParseError, ParseErrorKind};
use crate::keyword::Keyword;
use crate::lexer::{self, Punctuator, QuoteForm, Span, TokenKind};

/// Returns the declared type of a column as `PRAGMA table_info` reports it.
///
/// **SQLite stores the type with its quoting taken off, and names the six
/// standard types in capitals.** `d "My Type"` has the type `My Type`, and
/// `a Int`, `a "Integer"` and `a [text]` have `INT`, `INTEGER` and `TEXT`. The
/// type text is dequoted the way `sqlite3Dequote` does it, which stops at the
/// first closing quote, and a result equal to `INT`, `INTEGER`, `REAL`, `TEXT`,
/// `BLOB` or `ANY` in any case is replaced by the capitals. Anything longer,
/// such as `integer(5)` or `int unsigned`, is kept as written. The text kept in
/// `sqlite_schema` is the statement as it was written and is not touched.
///
/// A quoted `"INTEGER"` on a `PRIMARY KEY` column therefore makes a rowid alias,
/// as it does in SQLite.
///
/// @param written - the type's tokens, as the statement spelled them
fn declared_type_text(written: &[u8]) -> Vec<u8> {
    let dequoted = dequote_type(written);
    for standard in [&b"INT"[..], b"INTEGER", b"REAL", b"TEXT", b"BLOB", b"ANY"] {
        if dequoted.eq_ignore_ascii_case(standard) {
            return standard.to_vec();
        }
    }
    dequoted
}

/// Takes the quotes off a type name that starts with one.
///
/// @param written - the type's tokens, as the statement spelled them
fn dequote_type(written: &[u8]) -> Vec<u8> {
    let Some((&first, rest)) = written.split_first() else {
        return Vec::new();
    };
    let closing = match first {
        b'"' | b'\'' | b'`' => first,
        b'[' => b']',
        _ => return written.to_vec(),
    };
    let mut out = Vec::with_capacity(rest.len());
    let mut at = 0usize;
    while let Some(&byte) = rest.get(at) {
        if byte == closing {
            // A doubled quote is one quote of the name; `]` has no doubled form.
            if closing != b']' && rest.get(at.saturating_add(1)) == Some(&closing) {
                out.push(closing);
                at = at.saturating_add(2);
                continue;
            }
            break;
        }
        out.push(byte);
        at = at.saturating_add(1);
    }
    out
}

impl Parser<'_> {
    /// Dispatches the five `CREATE` forms.
    pub(super) fn parse_create(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(Keyword::CREATE)?;
        let temporary = self.eat_keyword(Keyword::TEMP)? || self.eat_keyword(Keyword::TEMPORARY)?;
        if self.eat_keyword(Keyword::TABLE)? {
            return self.parse_create_table(temporary);
        }
        if self.at_keyword(Keyword::UNIQUE)? || self.at_keyword(Keyword::INDEX)? {
            let unique = self.eat_keyword(Keyword::UNIQUE)?;
            self.expect_keyword(Keyword::INDEX)?;
            return self.parse_create_index(unique);
        }
        if self.eat_keyword(Keyword::VIEW)? {
            return self.parse_create_view(temporary);
        }
        if self.eat_keyword(Keyword::TRIGGER)? {
            return self.parse_create_trigger(temporary);
        }
        if self.eat_keyword(Keyword::VIRTUAL)? {
            self.expect_keyword(Keyword::TABLE)?;
            return self.parse_create_virtual_table();
        }
        Err(self.unexpected(&["TABLE", "INDEX", "VIEW", "TRIGGER", "VIRTUAL"])?)
    }

    /// Parses `IF NOT EXISTS`, if present.
    fn parse_if_not_exists(&mut self) -> Result<bool, ParseError> {
        if !self.at_keyword(Keyword::IF)? {
            return Ok(false);
        }
        self.bump()?;
        self.expect_keyword(Keyword::NOT)?;
        self.expect_keyword(Keyword::EXISTS)?;
        Ok(true)
    }

    /// Parses the body of `CREATE TABLE`.
    fn parse_create_table(&mut self, temporary: bool) -> Result<Statement, ParseError> {
        let if_not_exists = self.parse_if_not_exists()?;
        let (database, name) = self.parse_qualified_name()?;
        if self.eat_keyword(Keyword::AS)? {
            let select = self.parse_select()?;
            return Ok(Statement::CreateTable {
                temporary,
                if_not_exists,
                database,
                name,
                body: CreateTableBody::AsSelect(select),
            });
        }
        self.expect(Punctuator::LeftParen)?;
        let mut columns = Vec::new();
        let mut constraints = Vec::new();
        loop {
            if self.at_table_constraint()? {
                self.parse_table_constraint_run(&mut constraints)?;
            } else {
                columns.push(self.parse_column_def()?);
                // **Charged here, the way a result set is charged in
                // `parse_result_columns`** (task-2066 section 4.2, item 21).
                // `Limit::Column` was enforced on what a `SELECT` returns and
                // on nothing a table declares, so a 2,100-column
                // `CREATE TABLE` succeeded here and was refused by SQLite
                // with "too many columns"; at about five thousand it failed
                // with "the mini-columns do not fit in one page", which is
                // the right outcome for the wrong reason and says nothing a
                // caller can act on.
                if columns.len() as i64 > self.limits.get(Limit::Column) {
                    // SQLite names the table: `too many columns on w`.
                    return Err(ParseError::new(
                        ParseErrorKind::Refused(format!(
                            "too many columns on {}",
                            String::from_utf8_lossy(self.ast.text(name))
                        )),
                        Span::at(self.cursor()),
                    ));
                }
            }
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        self.expect(Punctuator::RightParen)?;
        let mut without_rowid = false;
        let mut strict = false;
        loop {
            if self.eat_keyword(Keyword::WITHOUT)? {
                let word = self.parse_name()?;
                if !self.ast.folded(word).eq_ignore_ascii_case(b"rowid") {
                    return Err(ParseError::new(
                        ParseErrorKind::Unexpected {
                            found: String::from_utf8_lossy(self.ast.text(word)).into_owned(),
                            expected: vec!["ROWID"],
                        },
                        self.ast.name(word).map(|n| n.span).unwrap_or_default(),
                    ));
                }
                without_rowid = true;
            } else if self.at_name()?
                && self
                    .peek()?
                    .text(self.source())
                    .eq_ignore_ascii_case(b"strict")
            {
                self.bump()?;
                strict = true;
            } else {
                break;
            }
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        Ok(Statement::CreateTable {
            temporary,
            if_not_exists,
            database,
            name,
            body: CreateTableBody::Columns {
                columns,
                constraints,
                without_rowid,
                strict,
            },
        })
    }

    /// Returns whether the next tokens begin a table constraint rather than a
    /// column definition.
    fn at_table_constraint(&mut self) -> Result<bool, ParseError> {
        if self.at_keyword(Keyword::CONSTRAINT)? {
            return Ok(true);
        }
        Ok(matches!(
            self.peek()?.keyword(),
            Some(Keyword::PRIMARY)
                | Some(Keyword::UNIQUE)
                | Some(Keyword::CHECK)
                | Some(Keyword::FOREIGN)
        ))
    }

    /// Parses the table constraints between two commas.
    ///
    /// SQLite's grammar lets table constraints follow each other without a
    /// comma (`PRIMARY KEY(a) UNIQUE(b)`), and lets `CONSTRAINT name` stand
    /// alone, so `CONSTRAINT one CONSTRAINT two CHECK (x)` is accepted. The last
    /// name written before a constraint is the one it takes, and a name with no
    /// constraint after it names nothing.
    /// @param constraints - where each parsed constraint and its name is appended
    fn parse_table_constraint_run(
        &mut self,
        constraints: &mut Vec<(Option<NameId>, TableConstraint)>,
    ) -> Result<(), ParseError> {
        let mut pending = None;
        loop {
            if self.at_keyword(Keyword::CONSTRAINT)? {
                pending = self.parse_constraint_name()?;
            } else if self.at_table_constraint()? {
                constraints.push((pending.take(), self.parse_table_constraint()?));
            } else {
                return Ok(());
            }
        }
    }

    /// Parses an optional `CONSTRAINT name` prefix.
    fn parse_constraint_name(&mut self) -> Result<Option<NameId>, ParseError> {
        if !self.eat_keyword(Keyword::CONSTRAINT)? {
            return Ok(None);
        }
        Ok(Some(self.parse_name()?))
    }

    /// Parses one column definition and every constraint attached to it.
    pub(super) fn parse_column_def(&mut self) -> Result<ColumnDef, ParseError> {
        let start = self.cursor();
        let name = self.parse_name()?;
        // The *name* takes the wide class and the *type* takes the narrow one,
        // so `left TEXT` is a column called `left` and `a left` is a syntax
        // error - which is what the pinned release does. Asking the wide
        // question here would read the `left` of `a left` as a type and accept
        // a statement SQLite refuses.
        // `GENERATED` is a word a type name may contain, but right after the
        // column name it begins `GENERATED ALWAYS AS (...)`. Read as a type it
        // gave the column the NUMERIC affinity of an unknown type name, which
        // turned a generated 556.0 into 556.
        let declared_type =
            if Parser::token_is_type_word(self.peek()?) && !self.at_constraint_keyword()? {
                let id = self.parse_type_name()?;
                Some(declared_type_text(self.ast.text(id)))
            } else {
                None
            };
        let mut constraints = Vec::new();
        loop {
            if self.at_deferral()? {
                // SQLite's `ccons ::= defer_subclause` is a constraint of its own. It
                // changes the foreign key written before it in this column and is
                // ignored when there is none.
                let (deferrable, initially_deferred) = self.parse_deferral()?;
                let key = constraints.iter_mut().rev().find_map(|(_, c)| match c {
                    ColumnConstraint::References(clause) => Some(clause),
                    _ => None,
                });
                if let Some(clause) = key {
                    clause.deferrable = deferrable;
                    clause.initially_deferred = initially_deferred;
                }
                continue;
            }
            let named = self.parse_constraint_name()?;
            // **A name with no constraint after it is legal.** SQLite's
            // grammar has `ccons ::= CONSTRAINT nm` as a constraint of its own,
            // which only names whatever constraint comes next, so
            // `CREATE TABLE t (a CONSTRAINT c)` is accepted and
            // `CREATE TABLE t (a CONSTRAINT c foo)` fails at `foo`, which the
            // caller reports when it finds neither a comma nor a parenthesis.
            let Some(constraint) = self.parse_column_constraint()? else {
                if named.is_some() {
                    continue;
                }
                break;
            };
            constraints.push((named, constraint));
        }
        let end = self.cursor();
        Ok(ColumnDef {
            name,
            declared_type,
            constraints,
            span: Span::new(start, end),
            deferred_failure: None,
        })
    }

    /// Parses one column constraint, or `None` when there is not one next.
    fn parse_column_constraint(&mut self) -> Result<Option<ColumnConstraint>, ParseError> {
        let Some(keyword) = self.peek()?.keyword() else {
            return Ok(None);
        };
        let constraint = match keyword {
            Keyword::PRIMARY => {
                self.bump()?;
                self.expect_keyword(Keyword::KEY)?;
                let order = self.parse_sort_order()?;
                let on_conflict = self.parse_on_conflict()?;
                let autoincrement = self.eat_keyword(Keyword::AUTOINCREMENT)?;
                ColumnConstraint::PrimaryKey {
                    order,
                    on_conflict,
                    autoincrement,
                }
            }
            Keyword::NOT => {
                if !self.at_keyword_ahead(1, Keyword::NULL)? {
                    return Ok(None);
                }
                self.bump()?;
                self.bump()?;
                ColumnConstraint::NotNull(self.parse_on_conflict()?)
            }
            Keyword::NULL => {
                self.bump()?;
                ColumnConstraint::Null
            }
            Keyword::UNIQUE => {
                self.bump()?;
                ColumnConstraint::Unique(self.parse_on_conflict()?)
            }
            Keyword::CHECK => {
                self.bump()?;
                self.expect(Punctuator::LeftParen)?;
                // The reference points at the *expression*, not at the keyword,
                // and the shell draws its caret from the offset - so the span
                // taken here is the first token inside the parenthesis.
                let at = self.peek()?.span;
                let before = self.selects;
                let expr = self.parse_expr()?;
                self.expect(Punctuator::RightParen)?;
                self.no_subquery_in_check(before, at)?;
                ColumnConstraint::Check(expr)
            }
            Keyword::DEFAULT => {
                self.bump()?;
                ColumnConstraint::Default(self.parse_default_value()?)
            }
            Keyword::COLLATE => {
                self.bump()?;
                ColumnConstraint::Collate(self.parse_name()?)
            }
            Keyword::REFERENCES => {
                self.bump()?;
                ColumnConstraint::References(self.parse_foreign_key_clause()?)
            }
            Keyword::GENERATED | Keyword::AS => {
                if keyword == Keyword::GENERATED {
                    self.bump()?;
                    self.expect_keyword(Keyword::ALWAYS)?;
                }
                self.expect_keyword(Keyword::AS)?;
                self.expect(Punctuator::LeftParen)?;
                let expr = self.parse_expr()?;
                self.expect(Punctuator::RightParen)?;
                let mut bad_storage = false;
                let stored = if self.at_name()? {
                    let word = self.peek()?;
                    let text = word.text(self.source());
                    if text.eq_ignore_ascii_case(b"stored") {
                        self.bump()?;
                        true
                    } else if text.eq_ignore_ascii_case(b"virtual") {
                        self.bump()?;
                        false
                    } else if self.at_keyword(Keyword::GENERATED)? {
                        // SQLite reads the word after the closing parenthesis as
                        // the storage name, and `GENERATED` is a word it accepts
                        // there, so a second `GENERATED ALWAYS AS` fails at `ALWAYS`.
                        self.bump()?;
                        false
                    } else {
                        // Any other plain word is read as the storage name too and refused
                        // afterwards with `error in generated column`.
                        self.bump()?;
                        bad_storage = true;
                        false
                    }
                } else {
                    false
                };
                ColumnConstraint::Generated {
                    expr,
                    stored,
                    bad_storage,
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(constraint))
    }

    /// Parses a `DEFAULT` value, which is a literal, a signed number, or a
    /// parenthesised expression.
    fn parse_default_value(&mut self) -> Result<crate::ast::ExprId, ParseError> {
        if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            let expr = self.parse_expr()?;
            self.expect(Punctuator::RightParen)?;
            return Ok(expr);
        }
        // Unparenthesised, SQLite's grammar takes a *literal* and nothing else:
        // a signed number, a string, a blob, NULL, TRUE, FALSE, or one of the
        // CURRENT_ keywords. Reading a whole expression here swallowed the
        // column's own `COLLATE` clause - `DEFAULT 'x' COLLATE NOCASE` became
        // one default and no collation - which changed both what
        // `PRAGMA table_info` reports and how the column compares.
        self.parse_literal_default()
    }

    /// Parses the literal an unparenthesised `DEFAULT` takes.
    ///
    /// One prefix form and no more: a number, a string, a blob, `NULL`, one of
    /// the `CURRENT_` keywords, or a signed number. It stops before any infix
    /// operator, which is the whole point - the next word after the default is
    /// the column's next constraint, not more of the default.
    fn parse_literal_default(&mut self) -> Result<crate::ast::ExprId, ParseError> {
        // **A word followed by `(` is a function call, and the grammar does not
        // allow one here.** SQLite reads `DEFAULT abs` as the word and then finds
        // a `(` where a constraint or a comma should be: `DEFAULT abs(1)` is
        // `near "(": syntax error`. A function needs the parentheses around the
        // whole expression, as in `DEFAULT (abs(1))`.
        let first = self.peek()?;
        if matches!(first.kind, TokenKind::Identifier { .. })
            && self.peek_at(1)?.is(Punctuator::LeftParen)
            && !matches!(first.keyword(), Some(Keyword::NULL))
        {
            self.bump()?;
            return Err(self.unexpected(&[",", ")"])?);
        }
        // **A bare word is a string.** SQLite's `ccons ::= DEFAULT scantok id`
        // makes `DEFAULT hello` the text `hello`, and a double quoted word the
        // same, because there is no column in scope for it to name. `TRUE` and
        // `FALSE` are the two words that keep their meaning.
        if Parser::token_is_plain_name(first) {
            let word = lexer::identifier_text(self.source, first).into_owned();
            if !word.eq_ignore_ascii_case(b"true") && !word.eq_ignore_ascii_case(b"false") {
                self.bump()?;
                return Ok(self
                    .ast
                    .add_expr(Expr::Literal(Literal::String(word)), first.span));
            }
        }
        self.parse_prefix()
    }

    /// Parses an optional `ASC` or `DESC`.
    fn parse_sort_order(&mut self) -> Result<SortOrder, ParseError> {
        if self.eat_keyword(Keyword::ASC)? {
            return Ok(SortOrder::Ascending);
        }
        if self.eat_keyword(Keyword::DESC)? {
            return Ok(SortOrder::Descending);
        }
        Ok(SortOrder::Ascending)
    }

    /// Parses an optional `ON CONFLICT action`.
    fn parse_on_conflict(&mut self) -> Result<Option<crate::ast::ConflictAction>, ParseError> {
        if !self.at_keyword(Keyword::ON)? || !self.at_keyword_ahead(1, Keyword::CONFLICT)? {
            return Ok(None);
        }
        self.bump()?;
        self.bump()?;
        Ok(Some(self.parse_conflict_action()?))
    }

    /// Parses a table constraint.
    fn parse_table_constraint(&mut self) -> Result<TableConstraint, ParseError> {
        if self.eat_keyword(Keyword::PRIMARY)? {
            self.expect_keyword(Keyword::KEY)?;
            // SQLite writes `AUTOINCREMENT` inside the parentheses, after the
            // last key column: `PRIMARY KEY(a AUTOINCREMENT)`. After the closing
            // parenthesis it is a syntax error, and the binder refuses it unless
            // the key is a single INTEGER column.
            let (columns, autoincrement) = self.parse_primary_key_columns()?;
            if self.at_keyword(Keyword::AUTOINCREMENT)? {
                return Err(self.unexpected(&[")", ",", "ON"])?);
            }
            let on_conflict = self.parse_on_conflict()?;
            return Ok(TableConstraint::PrimaryKey {
                columns,
                on_conflict,
                autoincrement,
            });
        }
        if self.eat_keyword(Keyword::UNIQUE)? {
            let columns = self.parse_indexed_column_list()?;
            let on_conflict = self.parse_on_conflict()?;
            return Ok(TableConstraint::Unique {
                columns,
                on_conflict,
            });
        }
        if self.at_keyword(Keyword::CHECK)? {
            self.bump()?;
            self.expect(Punctuator::LeftParen)?;
            let at = self.peek()?.span;
            let before = self.selects;
            let expr = self.parse_expr()?;
            self.expect(Punctuator::RightParen)?;
            self.no_subquery_in_check(before, at)?;
            return Ok(TableConstraint::Check {
                expr,
                on_conflict: self.parse_on_conflict()?,
            });
        }
        self.expect_keyword(Keyword::FOREIGN)?;
        self.expect_keyword(Keyword::KEY)?;
        self.expect(Punctuator::LeftParen)?;
        let mut columns = Vec::new();
        loop {
            columns.push(self.parse_name()?);
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        self.expect(Punctuator::RightParen)?;
        self.expect_keyword(Keyword::REFERENCES)?;
        let clause = self.parse_foreign_key_clause()?;
        Ok(TableConstraint::ForeignKey { columns, clause })
    }

    /// Parses the parenthesised key of a table level `PRIMARY KEY`, with its
    /// optional `AUTOINCREMENT` after the last column.
    fn parse_primary_key_columns(&mut self) -> Result<(Vec<IndexedColumn>, bool), ParseError> {
        self.expect(Punctuator::LeftParen)?;
        let mut columns = Vec::new();
        loop {
            columns.push(self.parse_indexed_column()?);
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        let autoincrement = self.eat_keyword(Keyword::AUTOINCREMENT)?;
        self.expect(Punctuator::RightParen)?;
        Ok((columns, autoincrement))
    }

    /// Parses `( indexed-column, ... )`.
    fn parse_indexed_column_list(&mut self) -> Result<Vec<IndexedColumn>, ParseError> {
        self.expect(Punctuator::LeftParen)?;
        let mut columns = Vec::new();
        loop {
            columns.push(self.parse_indexed_column()?);
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        self.expect(Punctuator::RightParen)?;
        Ok(columns)
    }

    /// Reads a string literal written as an index key as the column it names.
    ///
    /// SQLite turns a string there into an identifier, so `PRIMARY KEY('a')` and
    /// `CREATE INDEX i ON t('c')` index the columns `a` and `c`. A backtick
    /// name is used because it never falls back to a string literal.
    /// @param expr - the parsed key expression
    fn string_as_column_name(&mut self, expr: crate::ast::ExprId) -> crate::ast::ExprId {
        // The expression parser reads `'b' COLLATE nocase` as one collated
        // expression, so the string is looked for under the collation too.
        if let Some(Expr::Collate { operand, collation }) = self.ast.expr(expr) {
            let (operand, collation) = (*operand, *collation);
            let converted = self.string_as_column_name(operand);
            if converted == operand {
                return expr;
            }
            let span = self.ast.expr_span(expr);
            return self.ast.add_expr(
                Expr::Collate {
                    operand: converted,
                    collation,
                },
                span,
            );
        }
        let span = self.ast.expr_span(expr);
        let text = match self.ast.expr(expr) {
            Some(Expr::Literal(Literal::String(text))) => text.clone(),
            _ => return expr,
        };
        let column = self.ast.intern(text, QuoteForm::Backtick, span);
        self.ast.add_expr(
            Expr::Column {
                database: None,
                table: None,
                column,
            },
            span,
        )
    }

    /// Parses one indexed column: an expression with an optional collation and
    /// direction.
    pub(super) fn parse_indexed_column(&mut self) -> Result<IndexedColumn, ParseError> {
        let expr = self.parse_expr()?;
        let expr = self.string_as_column_name(expr);
        let collation = if self.eat_keyword(Keyword::COLLATE)? {
            Some(self.parse_name()?)
        } else {
            None
        };
        let order = self.parse_sort_order()?;
        // SQLite's grammar reads an index key like an ORDER BY term, so `NULLS FIRST` parses and is
        // refused when the index is built.
        let nulls = if self.eat_keyword(Keyword::NULLS)? {
            if self.eat_keyword(Keyword::FIRST)? {
                Some(crate::ast::NullOrder::First)
            } else {
                self.expect_keyword(Keyword::LAST)?;
                Some(crate::ast::NullOrder::Last)
            }
        } else {
            None
        };
        Ok(IndexedColumn {
            expr,
            collation,
            order,
            nulls,
        })
    }

    /// Parses the tail of a `REFERENCES` clause.
    fn parse_foreign_key_clause(&mut self) -> Result<ForeignKeyClause, ParseError> {
        let table = self.parse_name()?;
        let mut columns = Vec::new();
        if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            loop {
                columns.push(self.parse_name()?);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
            self.expect(Punctuator::RightParen)?;
        }
        let mut actions = Vec::new();
        loop {
            if self.at_keyword(Keyword::ON)? {
                self.bump()?;
                let on_delete = if self.eat_keyword(Keyword::DELETE)? {
                    true
                } else {
                    self.expect_keyword(Keyword::UPDATE)?;
                    false
                };
                let action = self.parse_referential_action()?;
                actions.push(if on_delete {
                    ForeignKeyAction::OnDelete(action)
                } else {
                    ForeignKeyAction::OnUpdate(action)
                });
                continue;
            }
            if self.at_keyword(Keyword::MATCH)? {
                self.bump()?;
                actions.push(ForeignKeyAction::Match(self.parse_name()?));
                continue;
            }
            break;
        }
        let (deferrable, initially_deferred) = self.parse_deferral()?;
        Ok(ForeignKeyClause {
            table,
            columns,
            actions,
            deferrable,
            initially_deferred,
        })
    }

    /// Returns whether the next tokens are `DEFERRABLE` or `NOT DEFERRABLE`.
    fn at_deferral(&mut self) -> Result<bool, ParseError> {
        if self.at_keyword(Keyword::DEFERRABLE)? {
            return Ok(true);
        }
        Ok(self.at_keyword(Keyword::NOT)? && self.at_keyword_ahead(1, Keyword::DEFERRABLE)?)
    }

    /// Parses an optional `[NOT] DEFERRABLE [INITIALLY DEFERRED | IMMEDIATE]` clause.
    ///
    /// Returns whether the key is deferrable, when the clause was written, and
    /// whether it starts deferred.
    fn parse_deferral(&mut self) -> Result<(Option<bool>, bool), ParseError> {
        let mut deferrable = None;
        let mut initially_deferred = false;
        if self.at_keyword(Keyword::NOT)? && self.at_keyword_ahead(1, Keyword::DEFERRABLE)? {
            self.bump()?;
            self.bump()?;
            deferrable = Some(false);
        } else if self.eat_keyword(Keyword::DEFERRABLE)? {
            deferrable = Some(true);
        }
        if deferrable.is_some() && self.eat_keyword(Keyword::INITIALLY)? {
            if self.eat_keyword(Keyword::DEFERRED)? {
                initially_deferred = true;
            } else {
                self.expect_keyword(Keyword::IMMEDIATE)?;
            }
        }
        Ok((deferrable, initially_deferred))
    }

    /// Parses `SET NULL`, `SET DEFAULT`, `CASCADE`, `RESTRICT` or `NO ACTION`.
    fn parse_referential_action(&mut self) -> Result<ReferentialAction, ParseError> {
        if self.eat_keyword(Keyword::SET)? {
            if self.eat_keyword(Keyword::NULL)? {
                return Ok(ReferentialAction::SetNull);
            }
            self.expect_keyword(Keyword::DEFAULT)?;
            return Ok(ReferentialAction::SetDefault);
        }
        if self.eat_keyword(Keyword::CASCADE)? {
            return Ok(ReferentialAction::Cascade);
        }
        if self.eat_keyword(Keyword::RESTRICT)? {
            return Ok(ReferentialAction::Restrict);
        }
        self.expect_keyword(Keyword::NO)?;
        self.expect_keyword(Keyword::ACTION)?;
        Ok(ReferentialAction::NoAction)
    }

    /// Parses the body of `CREATE [UNIQUE] INDEX`.
    fn parse_create_index(&mut self, unique: bool) -> Result<Statement, ParseError> {
        let if_not_exists = self.parse_if_not_exists()?;
        let (database, name) = self.parse_qualified_name()?;
        self.expect_keyword(Keyword::ON)?;
        let table = self.parse_name()?;
        // `USING <module>` between the table and the columns, which is where
        // PostgreSQL puts it and therefore where anybody writing `USING hnsw`
        // will look for it. SQLite's grammar has nothing here, so accepting it
        // adds a form rather than changing one.
        let using = if self.eat_keyword(Keyword::USING)? {
            Some(self.parse_name()?)
        } else {
            None
        };
        let columns = self.parse_indexed_column_list()?;
        // `WITH ( name = value, ... )` after the columns, which is where
        // PostgreSQL and pgvector put an index's storage parameters - and it
        // follows `USING` for the reason `USING` is here at all: somebody who
        // has written `WITH (m = 16, ef_construction = 64)` against pgvector
        // writes it here, in that order.
        //
        // Collected as raw source slices rather than parsed as SQL, exactly as
        // a module's arguments are: a storage parameter is a setting a
        // structure reads, not an expression the engine evaluates, and the
        // structure is the thing that knows which names it has.
        let settings = if self.eat_keyword(Keyword::WITH)? {
            self.expect(Punctuator::LeftParen)?;
            let held = self.parse_module_arguments()?;
            self.expect(Punctuator::RightParen)?;
            held
        } else {
            Vec::new()
        };
        let filter = if self.eat_keyword(Keyword::WHERE)? {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Statement::CreateIndex {
            unique,
            if_not_exists,
            database,
            name,
            table,
            using,
            columns,
            settings,
            filter,
        })
    }

    /// Parses the body of `CREATE VIEW`.
    fn parse_create_view(&mut self, temporary: bool) -> Result<Statement, ParseError> {
        let if_not_exists = self.parse_if_not_exists()?;
        let (database, name) = self.parse_qualified_name()?;
        let mut columns = Vec::new();
        if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            loop {
                columns.push(self.parse_name()?);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
            self.expect(Punctuator::RightParen)?;
        }
        self.expect_keyword(Keyword::AS)?;
        let select = self.parse_select()?;
        Ok(Statement::CreateView {
            temporary,
            if_not_exists,
            database,
            name,
            columns,
            select,
        })
    }

    /// Parses the body of `CREATE TRIGGER`, including its statement list.
    fn parse_create_trigger(&mut self, temporary: bool) -> Result<Statement, ParseError> {
        let if_not_exists = self.parse_if_not_exists()?;
        let (database, name) = self.parse_qualified_name()?;
        let time = if self.eat_keyword(Keyword::BEFORE)? {
            Some(TriggerTime::Before)
        } else if self.eat_keyword(Keyword::AFTER)? {
            Some(TriggerTime::After)
        } else if self.eat_keyword(Keyword::INSTEAD)? {
            self.expect_keyword(Keyword::OF)?;
            Some(TriggerTime::InsteadOf)
        } else {
            None
        };
        let event = if self.eat_keyword(Keyword::DELETE)? {
            TriggerEvent::Delete
        } else if self.eat_keyword(Keyword::INSERT)? {
            TriggerEvent::Insert
        } else {
            self.expect_keyword(Keyword::UPDATE)?;
            let mut columns = Vec::new();
            if self.eat_keyword(Keyword::OF)? {
                loop {
                    columns.push(self.parse_name()?);
                    if !self.eat(Punctuator::Comma)? {
                        break;
                    }
                }
            }
            TriggerEvent::Update(columns)
        };
        self.expect_keyword(Keyword::ON)?;
        // `ON main.t` is SQLite's grammar too: a temporary trigger may name a
        // table in any database, and a trigger in a named database may repeat
        // that database's name.
        let (table_database, table) = self.parse_qualified_name()?;
        let for_each_row = if self.eat_keyword(Keyword::FOR)? {
            self.expect_keyword(Keyword::EACH)?;
            self.expect_keyword(Keyword::ROW)?;
            true
        } else {
            false
        };
        let when = if self.eat_keyword(Keyword::WHEN)? {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_keyword(Keyword::BEGIN)?;
        let mut body = Vec::new();
        loop {
            if self.at_keyword(Keyword::END)? {
                // A trigger body must hold at least one statement; SQLite
                // refuses `BEGIN END` rather than creating a trigger that does
                // nothing.
                if body.is_empty() {
                    return Err(self.unexpected(&["UPDATE", "INSERT", "DELETE", "SELECT"])?);
                }
                self.bump()?;
                break;
            }
            let statement = self.parse_trigger_body_statement()?;
            body.push(statement);
            self.expect(Punctuator::Semicolon)?;
        }
        Ok(Statement::CreateTrigger {
            temporary,
            if_not_exists,
            database,
            name,
            time,
            event,
            table,
            table_database,
            for_each_row,
            when,
            body,
        })
    }

    /// Parses one statement of a trigger body.
    ///
    /// The body accepts only four statement kinds, and saying so here gives a
    /// better diagnostic than letting the general dispatcher accept a `CREATE`
    /// and having the catalog refuse it much later.
    fn parse_trigger_body_statement(&mut self) -> Result<Statement, ParseError> {
        let statement = match self.peek()?.keyword() {
            Some(Keyword::UPDATE) => self.parse_update(),
            Some(Keyword::INSERT) | Some(Keyword::REPLACE) => self.parse_insert(),
            Some(Keyword::DELETE) => self.parse_delete(),
            Some(Keyword::SELECT) | Some(Keyword::VALUES) | Some(Keyword::WITH) => {
                self.parse_select_statement()
            }
            _ => Err(self.unexpected(&["UPDATE", "INSERT", "DELETE", "SELECT"])?),
        }?;
        // A trigger body has no caller to return rows to, so SQLite refuses
        // `RETURNING` in one - and refuses it while parsing, which is why the
        // check is here rather than in the binder.
        let returning = match &statement {
            Statement::Insert(insert) => !insert.returning.is_empty(),
            Statement::Update(update) => !update.returning.is_empty(),
            Statement::Delete(delete) => !delete.returning.is_empty(),
            _ => false,
        };
        if returning {
            // SQLite points at nothing for this refusal.
            return Err(ParseError::new(
                ParseErrorKind::Refused("cannot use RETURNING in a trigger".to_string()),
                Span::default(),
            ));
        }
        self.refuse_trigger_index_hint(&statement)?;
        Ok(statement)
    }

    /// Refuses `INDEXED BY` and `NOT INDEXED` on the target of an `UPDATE` or
    /// `DELETE` in a trigger body, in SQLite's words.
    ///
    /// Both used to be accepted and stored, and `INDEXED BY` was not even
    /// checked for an index that exists, because a trigger body is only bound
    /// when the trigger fires: `CREATE TRIGGER t AFTER INSERT ON s BEGIN DELETE
    /// FROM h INDEXED BY nope; END` created a trigger here, and the pinned
    /// 3.53.4 shell refuses it with the message below. A `SELECT` in a body
    /// may still carry either clause, as it may there.
    /// @param statement - one statement of the trigger body
    fn refuse_trigger_index_hint(&self, statement: &Statement) -> Result<(), ParseError> {
        let target = match statement {
            Statement::Update(update) => update.target,
            Statement::Delete(delete) => delete.target,
            _ => return Ok(()),
        };
        let Some(term) = self.ast.from_term(target) else {
            return Ok(());
        };
        let clause = match term.source {
            crate::ast::FromSource::Table {
                indexed_by: crate::ast::IndexHint::IndexedBy(_),
                ..
            } => "INDEXED BY",
            crate::ast::FromSource::Table {
                indexed_by: crate::ast::IndexHint::NotIndexed,
                ..
            } => "NOT INDEXED",
            _ => return Ok(()),
        };
        Err(ParseError::new(
            ParseErrorKind::Refused(format!(
                "the {clause} clause is not allowed on UPDATE or DELETE statements within triggers"
            )),
            Span::default(),
        ))
    }

    /// Parses `CREATE VIRTUAL TABLE`, whose module arguments are opaque text.
    fn parse_create_virtual_table(&mut self) -> Result<Statement, ParseError> {
        let if_not_exists = self.parse_if_not_exists()?;
        let (database, name) = self.parse_qualified_name()?;
        self.expect_keyword(Keyword::USING)?;
        let module = self.parse_name()?;
        let mut arguments = Vec::new();
        if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            if !self.at(Punctuator::RightParen)? {
                arguments = self.parse_module_arguments()?;
            }
            self.expect(Punctuator::RightParen)?;
        }
        Ok(Statement::CreateVirtualTable {
            if_not_exists,
            database,
            name,
            module,
            arguments,
        })
    }

    /// Collects module arguments as raw source slices, split on top-level
    /// commas. A module argument is not SQL, so it is not parsed as SQL.
    fn parse_module_arguments(&mut self) -> Result<Vec<Vec<u8>>, ParseError> {
        let mut arguments = Vec::new();
        let mut start = self.cursor();
        let mut depth = 0usize;
        let mut end = start;
        loop {
            let token = self.peek()?;
            match token.kind {
                TokenKind::EndOfInput => return Err(self.unexpected(&[")"])?),
                TokenKind::Punctuator(Punctuator::LeftParen) => {
                    depth = depth.saturating_add(1);
                }
                TokenKind::Punctuator(Punctuator::RightParen) if depth == 0 => {
                    arguments.push(
                        self.source()
                            .get(start..end)
                            .unwrap_or(&[])
                            .trim_ascii()
                            .to_vec(),
                    );
                    return Ok(arguments);
                }
                TokenKind::Punctuator(Punctuator::RightParen) => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Punctuator(Punctuator::Comma) if depth == 0 => {
                    arguments.push(
                        self.source()
                            .get(start..end)
                            .unwrap_or(&[])
                            .trim_ascii()
                            .to_vec(),
                    );
                    self.bump()?;
                    start = self.cursor();
                    end = start;
                    continue;
                }
                _ => {}
            }
            end = token.span.end as usize;
            self.bump()?;
        }
    }

    /// Parses `DROP TABLE|INDEX|VIEW|TRIGGER [IF EXISTS] name`.
    pub(super) fn parse_drop(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(Keyword::DROP)?;
        let kind = if self.eat_keyword(Keyword::TABLE)? {
            ObjectKind::Table
        } else if self.eat_keyword(Keyword::INDEX)? {
            ObjectKind::Index
        } else if self.eat_keyword(Keyword::VIEW)? {
            ObjectKind::View
        } else if self.eat_keyword(Keyword::TRIGGER)? {
            ObjectKind::Trigger
        } else {
            return Err(self.unexpected(&["TABLE", "INDEX", "VIEW", "TRIGGER"])?);
        };
        let if_exists = if self.at_keyword(Keyword::IF)? {
            self.bump()?;
            self.expect_keyword(Keyword::EXISTS)?;
            true
        } else {
            false
        };
        let (database, name) = self.parse_qualified_name()?;
        Ok(Statement::Drop {
            kind,
            if_exists,
            database,
            name,
        })
    }

    /// Parses `ALTER TABLE name <action>`.
    pub(super) fn parse_alter(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(Keyword::ALTER)?;
        self.expect_keyword(Keyword::TABLE)?;
        let (database, table) = self.parse_qualified_name()?;
        let action = if self.eat_keyword(Keyword::RENAME)? {
            if self.eat_keyword(Keyword::TO)? {
                AlterAction::RenameTo(self.parse_name()?)
            } else {
                self.eat_keyword(Keyword::COLUMN)?;
                let from = self.parse_name()?;
                self.expect_keyword(Keyword::TO)?;
                AlterAction::RenameColumn {
                    from,
                    to: self.parse_name()?,
                }
            }
        } else if self.eat_keyword(Keyword::ADD)? {
            if self.at_keyword(Keyword::CONSTRAINT)? || self.at_keyword(Keyword::CHECK)? {
                self.parse_alter_add_check()?
            } else {
                self.eat_keyword(Keyword::COLUMN)?;
                self.parse_alter_add_column()?
            }
        } else if self.eat_keyword(Keyword::DROP)? {
            if self.eat_keyword(Keyword::CONSTRAINT)? {
                AlterAction::DropConstraint(self.parse_name()?)
            } else {
                self.eat_keyword(Keyword::COLUMN)?;
                AlterAction::DropColumn(self.parse_name()?)
            }
        } else if self.eat_keyword(Keyword::ALTER)? {
            self.parse_alter_column()?
        } else {
            return Err(self.unexpected(&["RENAME", "ADD", "DROP", "ALTER"])?);
        };
        Ok(Statement::AlterTable {
            database,
            table,
            action,
        })
    }

    /// Parses the column definition after `ADD [COLUMN]`.
    ///
    /// A subquery in the column's `CHECK` is not a compile error here, because
    /// SQLite finds it only when it reads the schema back after the change.
    fn parse_alter_add_column(&mut self) -> Result<AlterAction, ParseError> {
        self.deferring_check_subquery = true;
        self.deferred_check_subquery = false;
        let parsed = self.parse_column_def();
        self.deferring_check_subquery = false;
        let mut definition = parsed?;
        if std::mem::take(&mut self.deferred_check_subquery) {
            definition.deferred_failure = Some("subqueries prohibited in CHECK constraints");
        }
        Ok(AlterAction::AddColumn(definition))
    }

    /// Parses `[CONSTRAINT name] CHECK (expr) [ON CONFLICT action]` after `ADD`.
    ///
    /// The span is kept because SQLite copies the constraint into the table's
    /// stored text exactly as it was written, spacing and all.
    fn parse_alter_add_check(&mut self) -> Result<AlterAction, ParseError> {
        let start = self.cursor();
        let name = self.parse_constraint_name()?;
        self.expect_keyword(Keyword::CHECK)?;
        self.expect(Punctuator::LeftParen)?;
        let at = self.peek()?.span;
        let before = self.selects;
        let expr = self.parse_expr()?;
        self.expect(Punctuator::RightParen)?;
        self.no_subquery_in_check(before, at)?;
        self.parse_on_conflict()?;
        Ok(AlterAction::AddCheck {
            name,
            expr,
            start: start as u32,
            end: self.cursor() as u32,
        })
    }

    /// Parses `[COLUMN] name SET NOT NULL [ON CONFLICT action]` or
    /// `[COLUMN] name DROP NOT NULL` after `ALTER`.
    fn parse_alter_column(&mut self) -> Result<AlterAction, ParseError> {
        self.eat_keyword(Keyword::COLUMN)?;
        let column = self.parse_name()?;
        if self.eat_keyword(Keyword::DROP)? {
            self.expect_keyword(Keyword::NOT)?;
            self.expect_keyword(Keyword::NULL)?;
            return Ok(AlterAction::DropNotNull(column));
        }
        self.expect_keyword(Keyword::SET)?;
        let start = self.cursor();
        self.expect_keyword(Keyword::NOT)?;
        self.expect_keyword(Keyword::NULL)?;
        self.parse_on_conflict()?;
        Ok(AlterAction::SetNotNull {
            column,
            start: start as u32,
            end: self.cursor() as u32,
        })
    }
}
