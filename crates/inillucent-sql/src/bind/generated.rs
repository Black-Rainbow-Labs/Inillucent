//! Reading a `VIRTUAL` generated column: its value, and the rows that carry no
//! such column.
//!
//! Invariant: **a virtual generated column is bound by one function, whatever
//! reads it.** A `SELECT`, the key of an index over the column, a write that
//! checks `NOT NULL` or a `STRICT` type, and `OLD`, `NEW` and `excluded` all get
//! the same [`BoundExpr::Generated`] tree. The planner finds an index by
//! comparing two bound trees, so two ways of building one would stop it finding
//! the index.
//!
//! Split out of `bind.rs`, which is recorded at a ceiling in `policy.rs`.

use super::*;

impl Binder<'_> {
    /// Binds the value of a `VIRTUAL` generated column read from a FROM term.
    ///
    /// The result is a [`BoundExpr::Generated`]: the column's expression, bound
    /// with only that term visible, together with the column's affinity and
    /// collation and an expression that tells the executor whether the term has
    /// a row. The same function binds the key of an index over the column, so
    /// the planner finds the index by comparing two trees that were built alike.
    ///
    /// @param source - which FROM term
    /// @param column - which column of it, by declared position
    pub(crate) fn generated_value(
        &mut self,
        source: usize,
        column: u16,
    ) -> Result<BoundExpr, ParseError> {
        let Some(term) = self.sources.get(source) else {
            return Err(unsupported("unknown source", Span::default()));
        };
        let Some(info) = term.table.column(column) else {
            return Err(unsupported("unknown column", Span::default()));
        };
        let affinity = info.affinity;
        let collation = self
            .collation_named(&info.collation)
            .unwrap_or(Collation::Binary);
        let Some(sql) = info.generated_sql.clone() else {
            return Err(unsupported(
                "a generated column with no expression",
                Span::default(),
            ));
        };
        let present = self.row_present_expr(source);
        self.generating = self.generating.saturating_add(1);
        if self.generating > MAX_GENERATED_DEPTH {
            self.generating = self.generating.saturating_sub(1);
            return Err(ParseError::new(
                ParseErrorKind::Unsupported("a generated column refers to itself"),
                Span::default(),
            ));
        }
        let operand = self.bind_schema_expr_for(source, &sql);
        self.generating = self.generating.saturating_sub(1);
        Ok(BoundExpr::Generated {
            source,
            column,
            operand: Box::new(operand?),
            present: Box::new(present),
            affinity,
            collation,
        })
    }

    /// Returns an expression that is NULL exactly when a FROM term has no row.
    ///
    /// An outer join that finds no match presents every column of the term as
    /// NULL, and the rowid of a table that has one is among them. A table
    /// without a rowid is asked through the first column of its primary key,
    /// which is never NULL in a row that exists.
    ///
    /// @param source - which FROM term
    fn row_present_expr(&self, source: usize) -> BoundExpr {
        let Some(term) = self.sources.get(source) else {
            return BoundExpr::Rowid { source };
        };
        let table = &term.table;
        let key = table
            .columns
            .iter()
            .position(|held| held.primary_key_position == Some(1));
        match (table.without_rowid, key) {
            (true, Some(position)) => {
                let info = table.column(position as u16);
                BoundExpr::Column {
                    source,
                    column: position as u16,
                    slot: table.record_slot(position as u16).unwrap_or(position) as u16,
                    affinity: info.map_or(Affinity::Blob, |held| held.affinity),
                    collation: Collation::Binary,
                }
            }
            _ => BoundExpr::Rowid { source },
        }
    }

    /// Binds a `VIRTUAL` generated column of a trigger's `OLD` or `NEW`, or of
    /// an upsert's `excluded`.
    ///
    /// Those rows are images in registers and carry no virtual column, so a
    /// read of one gave NULL. The column's expression is bound against a
    /// temporary FROM term for the table, and every reference to that term is
    /// then pointed at the row image. Firing substitutes the image's values for
    /// those references, so the expression is computed from the row the trigger
    /// was fired for.
    ///
    /// @param image - `OLD_SOURCE`, `NEW_SOURCE` or `EXCLUDED_SOURCE`
    /// @param table - the table the image is a row of
    /// @param column - which column, by declared position
    pub(super) fn generated_of_row_image(
        &mut self,
        image: usize,
        table: &crate::catalog_view::TableInfo,
        column: u16,
    ) -> Result<BoundExpr, ParseError> {
        let id = self.sources.len();
        self.sources.push(BoundSource {
            index_hint: crate::bind::IndexChoice::Any,
            id,
            rows: SourceRows::Table,
            table: std::rc::Rc::new(table.clone()),
            alias: table.name.clone(),
            join: ast::JoinKind::Inner,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
        });
        let bound = self.generated_value(id, column);
        self.sources.pop();
        let mut bound = bound?;
        crate::rewrite::rewrite_expr(&mut bound, &mut |expr: &mut BoundExpr| match expr {
            BoundExpr::Column { source, .. }
            | BoundExpr::Rowid { source }
            | BoundExpr::Generated { source, .. }
                if *source == id =>
            {
                *source = image;
            }
            _ => {}
        });
        Ok(bound)
    }

    /// Binds a schema expression against one FROM term's scope.
    ///
    /// A generated column's expression names other columns of its own table, so
    /// it is bound with exactly that term visible and nothing else - a name it
    /// cannot resolve there is an error rather than something it picks up from
    /// the query that happened to read it.
    pub(super) fn bind_schema_expr_for(
        &mut self,
        source: usize,
        sql: &[u8],
    ) -> Result<BoundExpr, ParseError> {
        let saved = core::mem::replace(&mut self.scopes, vec![vec![source]]);
        // **A column a `USING` or `NATURAL` join suppresses is still a column of
        // the table the expression belongs to.** The expression is bound with
        // this term alone, and a name the join suppressed resolves only through
        // the term it was joined with, which is not in scope, so `a` in `g AS
        // (a * 2)` was "no such column" under `JOIN t USING (a)`.
        let suppressed = self
            .sources
            .get_mut(source)
            .map(|held| core::mem::take(&mut held.suppressed))
            .unwrap_or_default();
        let bound = self.bind_schema_expr(sql);
        if let Some(held) = self.sources.get_mut(source) {
            held.suppressed = suppressed;
        }
        self.scopes = saved;
        bound
    }
}
