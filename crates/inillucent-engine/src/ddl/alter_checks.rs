//! The checks `ALTER TABLE` makes of the schema around the table it changes.
//!
//! Invariant: **an `ALTER TABLE` that would leave a view, a trigger, an index or
//! a table whose names no longer resolve is refused whole.** SQLite re-reads
//! every object in the schema before and after a rename or a dropped column and
//! reports the first one that does not resolve as `error in <type> <name>
//! [after <what>]: <reason>`. The statement's own rollback undoes whatever had
//! been written by the time a check refuses.

use inillucent_base::error::statement_refusal;
use inillucent_base::DbResult;
use inillucent_catalog::paged::ObjectKind;
use inillucent_sql::ast::{CreateTableBody, Expr, ExprId, Statement as Parsed, TableConstraint};
use inillucent_sql::catalog_view::{CatalogView, TableInfo};
use inillucent_sql::parser::parse_next_statement;

/// The messages SQLite reports when it re-reads a view or a trigger.
///
/// A name that does not resolve is reported; a function that does not exist is
/// not, because SQLite looks functions up when it generates code and this check
/// only resolves names.
const RESOLUTION_FAILURES: [&str; 3] = ["no such column", "no such table", "ambiguous column name"];

impl crate::ImportedDatabase {
    /// Refuses the statement when a view or a trigger in a schema no longer binds.
    ///
    /// Objects are checked in the order they were created, and the first one
    /// that fails is the one reported, which is what SQLite does.
    ///
    /// @param at - the schema the table is in
    /// @param when - the words SQLite puts after the object's name, such as
    ///   ` after rename`, or an empty string for the check made before the change
    pub(crate) fn schema_still_binds(&self, at: usize, when: &str) -> DbResult<()> {
        for held in self.entries_of(at) {
            let entry = &held.entry;
            let problem = match entry.kind {
                ObjectKind::View => self.view_problem(at, &entry.name),
                ObjectKind::Trigger => self.trigger_problem(at, entry),
                _ => None,
            };
            if let Some(message) = problem {
                return Err(statement_refusal(format!(
                    "error in {} {}{when}: {message}",
                    String::from_utf8_lossy(entry.kind.as_text()),
                    String::from_utf8_lossy(&entry.name),
                )));
            }
        }
        Ok(())
    }

    /// Returns why a view no longer binds, when it does not.
    ///
    /// @param at - the schema the view is in
    /// @param name - the view's name
    fn view_problem(&self, at: usize, name: &[u8]) -> Option<String> {
        let folded = name.to_ascii_lowercase();
        let table = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == folded)?;
        let message = match self.bind_view(table)? {
            Ok(_) => return None,
            Err(error) => error.message().to_string(),
        };
        self.resolution_failure(at, message)
    }

    /// Returns why a trigger no longer binds, when it does not.
    ///
    /// @param at - the schema the trigger is in
    /// @param entry - the trigger's schema row
    fn trigger_problem(
        &self,
        at: usize,
        entry: &inillucent_catalog::paged::SchemaEntry,
    ) -> Option<String> {
        let table_folded = entry.table.to_ascii_lowercase();
        let name_folded = entry.name.to_ascii_lowercase();
        let table = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == table_folded)?;
        let trigger = table
            .triggers
            .iter()
            .find(|trigger| trigger.folded == name_folded)?;
        // A view has no columns of its own in the catalog: `new` and `old` in an
        // `INSTEAD OF` trigger mean the columns its query produces.
        let mut subject = table.clone();
        if subject.kind == inillucent_sql::catalog_view::TableKind::View {
            subject.columns = self.view_columns(table);
        }
        let table = &subject;
        let authorizer = inillucent_sql::bind::AllowAll;
        let externals = self.external_functions();
        let mut binder =
            inillucent_sql::bind::Binder::new(&self.schema.catalog, &trigger.ast, &authorizer)
                .with_functions(&externals)
                .with_collations(&self.session_state.collations)
                .with_trusted_schema(self.session_state.registry.policy().trusted_schema)
                .in_schema();
        let message = match binder.check_trigger(trigger, table) {
            Ok(()) => return None,
            Err(error) => error.message().to_string(),
        };
        self.resolution_failure(at, message)
    }

    /// Keeps a message only when it says a name did not resolve, and qualifies
    /// a missing table with the schema it was looked up in.
    ///
    /// SQLite re-reads a view with the schema named, so its message says
    /// `no such table: main.book` where a plain statement says `no such table:
    /// book`.
    ///
    /// @param at - the schema the object is in
    /// @param message - what the binder said
    fn resolution_failure(&self, at: usize, message: String) -> Option<String> {
        if !RESOLUTION_FAILURES
            .iter()
            .any(|prefix| message.starts_with(prefix))
        {
            return None;
        }
        let Some(missing) = message.strip_prefix("no such table: ") else {
            return Some(message);
        };
        if missing.contains('.') {
            return Some(message);
        }
        let database = CatalogView::database_name(&self.schema.catalog, at);
        Some(format!(
            "no such table: {}.{missing}",
            String::from_utf8_lossy(database)
        ))
    }
}

/// Returns why a `CREATE TABLE` text no longer holds together, when it does not.
///
/// This is the check SQLite makes of a table after a column is renamed or
/// dropped: a duplicate column name, an expression that names a column the
/// table does not have, and a foreign key whose own columns are not columns.
///
/// @param sql - the table's new `CREATE` text
pub(crate) fn table_text_problem(sql: &[u8]) -> Option<String> {
    let parsed = parse_next_statement(sql, 0, &inillucent_base::limits::Limits::default()).ok()?;
    let Parsed::CreateTable {
        body:
            CreateTableBody::Columns {
                columns,
                constraints,
                ..
            },
        ..
    } = &parsed.statement
    else {
        return None;
    };
    let names: Vec<Vec<u8>> = columns
        .iter()
        .map(|column| parsed.ast.folded(column.name).to_vec())
        .collect();
    for (at, name) in names.iter().enumerate() {
        if names
            .get(..at)
            .is_some_and(|earlier| earlier.contains(name))
        {
            let written = columns.get(at).map(|column| parsed.ast.text(column.name));
            return Some(format!(
                "duplicate column name: {}",
                String::from_utf8_lossy(written.unwrap_or_default())
            ));
        }
    }
    // SQLite finishes the table before it resolves the expressions in it, so a table left with
    // only generated columns is refused for that and not for the column its expression names.
    let only_generated = !columns.is_empty()
        && columns.iter().all(|column| {
            column.constraints.iter().any(|(_, constraint)| {
                matches!(
                    constraint,
                    inillucent_sql::ast::ColumnConstraint::Generated { .. }
                )
            })
        });
    if only_generated {
        return Some("must have at least one non-generated column".to_string());
    }
    if let Some(missing) = unresolved_column(&parsed.ast, &names) {
        return Some(missing);
    }
    for (_, constraint) in constraints {
        let TableConstraint::ForeignKey { columns, .. } = constraint else {
            continue;
        };
        for column in columns {
            if !names.contains(&parsed.ast.folded(*column).to_vec()) {
                return Some(format!(
                    "unknown column \"{}\" in foreign key definition",
                    String::from_utf8_lossy(parsed.ast.text(*column))
                ));
            }
        }
    }
    None
}

/// Returns why a `CREATE INDEX` text names a column its table does not have.
///
/// @param sql - the index's `CREATE` text
/// @param columns - the folded names of the table's columns after the change
pub(crate) fn index_text_problem(sql: &[u8], columns: &[Vec<u8>]) -> Option<String> {
    let parsed = parse_next_statement(sql, 0, &inillucent_base::limits::Limits::default()).ok()?;
    unresolved_column(&parsed.ast, columns)
}

/// Returns the first column reference in a parse that no column answers to.
///
/// @param ast - the parsed statement's arena
/// @param columns - the folded names of the columns that exist
fn unresolved_column(ast: &inillucent_sql::ast::Ast, columns: &[Vec<u8>]) -> Option<String> {
    for index in 0..ast.expr_count() {
        let id = ExprId(index as u32);
        let Some(Expr::Column { table, column, .. }) = ast.expr(id) else {
            continue;
        };
        let folded = ast.folded(*column);
        // A double quoted word that names no column is a string literal, which
        // is how a `DEFAULT "text"` is written.
        let quoted = ast
            .name(*column)
            .is_some_and(|name| name.quote == inillucent_sql::lexer::QuoteForm::Double);
        if quoted && table.is_none() {
            continue;
        }
        let rowid = matches!(folded, b"rowid" | b"oid" | b"_rowid_");
        if rowid || columns.iter().any(|name| name.as_slice() == folded) {
            continue;
        }
        let written = match table {
            Some(qualifier) => format!(
                "{}.{}",
                String::from_utf8_lossy(ast.text(*qualifier)),
                String::from_utf8_lossy(ast.text(*column))
            ),
            None => String::from_utf8_lossy(ast.text(*column)).into_owned(),
        };
        return Some(format!("no such column: {written}"));
    }
    None
}

/// Returns the folded names of a table's columns.
///
/// @param table - the table
pub(crate) fn column_names(table: &TableInfo) -> Vec<Vec<u8>> {
    table
        .columns
        .iter()
        .map(|column| column.folded.clone())
        .collect()
}
