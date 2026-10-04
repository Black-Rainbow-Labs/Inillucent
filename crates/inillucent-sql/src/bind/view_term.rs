//! A FROM term that names a view.
//!
//! Invariant: **a view is bound as a nested query over its stored body, in the
//! arena and the text the catalog snapshot holds, and its columns are named by
//! what they resolve to, not by how they were typed.** The body is not parsed
//! again for each reference.

use super::{function, qualify_missing_table, Binder, ParseError, ParseErrorKind, TableInfo};
use crate::ast::{self, JoinKind};
use crate::lexer::Span;

impl<'a> Binder<'a> {
    /// Binds a FROM term that names a view, as a nested query over its body.
    ///
    /// @param table - the view's catalog entry
    /// @param alias - the alias written after the name
    /// @param join - the join that attaches the term
    /// @param span - where the term was written
    pub(super) fn bind_view_term(
        &mut self,
        table: &'a TableInfo,
        alias: Option<ast::NameId>,
        join: JoinKind,
        span: Span,
    ) -> Result<(), ParseError> {
        let view_alias = match alias {
            Some(alias) => self.ast.text(alias).to_vec(),
            None => table.name.clone(),
        };
        let database_index = table.database;
        let view_name = table.name.clone();
        let Some(body) = table.view.as_ref() else {
            return Err(ParseError::new(
                ParseErrorKind::Unsupported("the view's definition could not be parsed"),
                span,
            ));
        };
        self.record_dependency(database_index);
        // The view's own arena outlives the binder because it belongs to
        // the catalog snapshot the binder holds, which is what lets the
        // body be bound in place rather than re-parsed here.
        let columns = body.columns.clone();
        let saved = self.ast;
        // A view's body is a string in the schema, so everything it names
        // is named from a schema - including anything a further view or a
        // generated column it reads goes on to name. The site is saved and
        // restored rather than set, because a view inside a view is still
        // inside the outer one.
        let saved_site = self.call_site;
        // **The view's own text, because its spans index into that.** A
        // result column with no alias is named after the text it was
        // written as, and cutting that text out of the statement that
        // reads the view gave an empty name: `CREATE VIEW v AS SELECT a + b
        // FROM t` had a column called "" where SQLite has `a + b`.
        let saved_source = self.source;
        self.ast = &body.ast;
        self.source = &table.create_sql;
        self.call_site = function::CallSite::Schema;
        // A view in `main` reads the tables of `main`, even where a temp table
        // shadows one by name; a temp view looks in the usual order.
        let view_database_name = self.catalog.database_name(database_index).to_vec();
        let saved_view_database = self.view_database.take();
        if !view_database_name.eq_ignore_ascii_case(b"temp") {
            self.view_database = Some(view_database_name.to_ascii_lowercase());
        }
        // A view body cannot see a CTE of the statement that reads it.
        let saved_ctes = core::mem::take(&mut self.ctes);
        let bound = self.bind_select(body.select);
        self.ctes = saved_ctes;
        self.view_database = saved_view_database;
        self.call_site = saved_site;
        self.source = saved_source;
        self.ast = saved;
        let view_database = self.catalog.database_name(database_index).to_vec();
        let mut bound = bound.map_err(|error| qualify_missing_table(error, &view_database))?;
        super::finish_view_columns(&mut bound, &columns, &view_name, span)?;
        let id = self.sources.len();
        self.push_subquery_source(bound, view_alias, columns, join, span)?;
        if let Some(source) = self.sources.get_mut(id) {
            source.derived.view = true;
            source.derived.name = view_name;
        }
        Ok(())
    }
}
