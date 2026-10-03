//! Working out what an `ALTER TABLE` writes to the schema, and refusing the
//! ones SQLite refuses.
//!
//! Invariant: **an `ALTER TABLE` first works out the new text of every schema
//! row it touches, checks that text, and only then writes any of it.** The
//! catalog is derived from the stored text, so a row written and found wanting
//! afterwards would have to be undone by the statement's rollback, and a row the
//! catalog cannot load would leave the connection without a schema to roll back
//! into.

use inillucent_base::error::statement_refusal;
use inillucent_base::DbResult;
use inillucent_catalog::paged::{ObjectKind, SchemaEntry};
use inillucent_catalog::{alter_text, rename};
use inillucent_sql::catalog_view::{CatalogView, TableKind};
use inillucent_sql::directive::{AddedColumnRisk, AlterKind};

use super::alter_checks::{column_names, index_text_problem, table_text_problem};
/// What every row of the schema is rewritten against.
struct Alteration<'a> {
    /// The statement text, for `ADD COLUMN`'s definition.
    source: &'a [u8],
    /// The table's name as written.
    table: &'a [u8],
    /// The table's folded name.
    folded: &'a [u8],
    /// What the statement does.
    action: &'a AlterKind,
    /// The tables a view or trigger body can read.
    known: Vec<rename::KnownTable>,
    /// `PRAGMA legacy_alter_table`.
    legacy: bool,
    /// `PRAGMA foreign_keys`.
    foreign_keys: bool,
}

impl crate::ImportedDatabase {
    /// Returns the new schema rows an `ALTER TABLE` writes, with their rowids.
    ///
    /// @param at - the schema the table is in
    /// @param source - the statement text
    /// @param table - the table's name as written
    /// @param folded - the table's folded name
    /// @param action - what the statement does
    pub(super) fn altered_entries(
        &self,
        at: usize,
        source: &[u8],
        table: &[u8],
        folded: &[u8],
        action: &AlterKind,
    ) -> DbResult<Vec<(i64, SchemaEntry)>> {
        let context = Alteration {
            source,
            table,
            folded,
            action,
            known: self.known_tables(at),
            legacy: self.pragmas.legacy_alter_table(),
            foreign_keys: self.pragmas.foreign_keys(),
        };
        let mut updates = Vec::new();
        for held in self.entries_of(at) {
            if let Some(moved) = altered_entry(&held.entry, &context)? {
                updates.push((held.rowid, moved));
            }
        }
        Ok(updates)
    }

    /// Returns the new rows of this session's temporary triggers and views that a
    /// rename in another schema changes.
    ///
    /// A temporary trigger or view can name a table of `main` or of an attached
    /// database, and SQLite rewrites it when that table or one of its columns is
    /// renamed. A temporary object is left alone when the temporary schema has a
    /// table of the same name, because the name then means that table.
    ///
    /// @param at - the schema the altered table is in
    /// @param source - the statement text
    /// @param table - the table's name as written
    /// @param folded - the table's folded name
    /// @param action - what the statement does
    pub(super) fn temp_dependents_altered(
        &self,
        at: usize,
        source: &[u8],
        table: &[u8],
        folded: &[u8],
        action: &AlterKind,
    ) -> DbResult<Vec<(i64, SchemaEntry)>> {
        let renames = matches!(
            action,
            AlterKind::RenameTable { .. } | AlterKind::RenameColumn { .. }
        );
        let shadowed = self
            .schema
            .tables
            .iter()
            .any(|held| held.database == crate::TEMP && held.folded == folded);
        if !renames || at == crate::TEMP || shadowed {
            return Ok(Vec::new());
        }
        let context = Alteration {
            source,
            table,
            folded,
            action,
            known: self.known_tables(at),
            legacy: self.pragmas.legacy_alter_table(),
            foreign_keys: self.pragmas.foreign_keys(),
        };
        let mut updates = Vec::new();
        for held in self.entries_of(crate::TEMP) {
            if !matches!(held.entry.kind, ObjectKind::Trigger | ObjectKind::View) {
                continue;
            }
            if let Some(moved) = altered_entry(&held.entry, &context)? {
                updates.push((held.rowid, moved));
            }
        }
        Ok(updates)
    }

    /// Lists the tables a view or a trigger body can read in one schema.
    ///
    /// A view's columns are only known by binding it, so a view is listed
    /// without them and the resolver treats it as a source it cannot see into.
    ///
    /// @param at - the schema
    fn known_tables(&self, at: usize) -> Vec<rename::KnownTable> {
        self.schema
            .tables
            .iter()
            .filter(|table| table.database == at)
            .map(|table| rename::KnownTable {
                folded: table.folded.clone(),
                columns: (table.kind == TableKind::Table).then(|| column_names(table)),
            })
            .collect()
    }

    /// Moves a renamed table's `sqlite_sequence` row to the new name.
    ///
    /// The high-water mark of an `AUTOINCREMENT` table is kept under the table's
    /// name, so a rename that left the row alone would start the renamed table
    /// again from one and leave a row for a table that no longer exists.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name before the rename
    /// @param action - what the statement does
    pub(super) fn rename_sequence(
        &mut self,
        at: usize,
        folded: &[u8],
        action: &AlterKind,
    ) -> DbResult<()> {
        let AlterKind::RenameTable { to } = action else {
            return Ok(());
        };
        let Some(old) = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == folded && table.autoincrement)
            .map(|table| table.name.clone())
        else {
            return Ok(());
        };
        let quote = |name: &[u8]| String::from_utf8_lossy(name).replace('\'', "''");
        let sql = format!(
            "UPDATE sqlite_sequence SET name = '{}' WHERE name = '{}'",
            quote(to),
            quote(&old)
        );
        self.query_internally(&sql)?;
        Ok(())
    }

    /// Returns the stored `CREATE TABLE` text of a table.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    fn stored_table_sql(&self, at: usize, folded: &[u8]) -> Vec<u8> {
        self.entries_of(at)
            .iter()
            .find(|held| {
                held.entry.kind == ObjectKind::Table
                    && held.entry.name.to_ascii_lowercase() == folded
            })
            .map(|held| held.entry.sql.clone())
            .unwrap_or_default()
    }

    /// Refuses `ADD CONSTRAINT` with a name in use, and `DROP CONSTRAINT` of a
    /// name that is not there or that SQLite does not let go.
    ///
    /// Names are compared without regard to case, and a `DEFAULT` or a `COLLATE`
    /// that was given a name counts as a constraint with that name.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    /// @param action - what the statement does
    pub(super) fn check_constraint_names(
        &self,
        at: usize,
        folded: &[u8],
        action: &AlterKind,
    ) -> DbResult<()> {
        match action {
            AlterKind::AddCheck {
                name: Some(name), ..
            } => {
                let taken = alter_text::constraint_names(&self.stored_table_sql(at, folded))?;
                match taken.contains(&name.to_ascii_lowercase()) {
                    true => Err(statement_refusal(format!(
                        "constraint {} already exists",
                        String::from_utf8_lossy(name)
                    ))),
                    false => Ok(()),
                }
            }
            AlterKind::DropConstraint { name } => {
                let sql = self.stored_table_sql(at, folded);
                match alter_text::drop_constraint(&sql, &name.to_ascii_lowercase())? {
                    alter_text::Dropped::Text(_) => Ok(()),
                    alter_text::Dropped::Missing => Err(statement_refusal(format!(
                        "no such constraint: {}",
                        String::from_utf8_lossy(name)
                    ))),
                    alter_text::Dropped::Refused => Err(statement_refusal(format!(
                        "constraint may not be dropped: {}",
                        String::from_utf8_lossy(name)
                    ))),
                }
            }
            _ => Ok(()),
        }
    }

    /// Refuses a `SET NOT NULL` or an `ADD CHECK` that the rows already in the
    /// table do not satisfy.
    ///
    /// SQLite's word for both is `constraint failed`. A `CHECK` that evaluates
    /// to NULL passes, as it does on an insert.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    /// @param source - the statement text
    /// @param action - what the statement does
    /// @param changes_text - whether the statement changes the table's text at all
    pub(super) fn check_constraint_rows(
        &mut self,
        at: usize,
        folded: &[u8],
        source: &[u8],
        action: &AlterKind,
        changes_text: bool,
    ) -> DbResult<()> {
        let violating = match action {
            AlterKind::SetNotNull { name, .. } if changes_text => {
                format!(
                    "\"{}\" IS NULL",
                    String::from_utf8_lossy(name).replace('"', "\"\"")
                )
            }
            AlterKind::AddCheck {
                expr_start,
                expr_end,
                ..
            } => format!(
                "NOT ({})",
                String::from_utf8_lossy(&statement_text(source, *expr_start, *expr_end))
            ),
            _ => return Ok(()),
        };
        let Some(info) = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == folded)
            .cloned()
        else {
            return Ok(());
        };
        let database = String::from_utf8_lossy(CatalogView::database_name(
            &self.schema.catalog,
            info.database,
        ))
        .replace('"', "\"\"");
        let name = String::from_utf8_lossy(&info.name).replace('"', "\"\"");
        let sql = format!("SELECT 1 FROM \"{database}\".\"{name}\" WHERE {violating} LIMIT 1");
        match self.query_internally(&sql)?.is_empty() {
            true => Ok(()),
            false => Err(constraint_error("constraint failed")),
        }
    }

    /// Checks the schema as it stands, for the actions SQLite checks first.
    ///
    /// @param at - the schema the table is in
    /// @param action - what the statement does
    pub(super) fn check_schema_before(&self, at: usize, action: &AlterKind) -> DbResult<()> {
        match action {
            AlterKind::RenameTable { .. } if !self.pragmas.legacy_alter_table() => {
                self.schema_still_binds(at, "")
            }
            AlterKind::RenameColumn { .. } | AlterKind::DropColumn { .. } => {
                self.schema_still_binds(at, "")
            }
            _ => Ok(()),
        }
    }

    /// Checks the new text of the table and of its indexes before it is written.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    /// @param updates - the rows about to be written
    /// @param action - what the statement does
    pub(super) fn check_new_texts(
        &self,
        at: usize,
        folded: &[u8],
        updates: &[(i64, SchemaEntry)],
        action: &AlterKind,
    ) -> DbResult<()> {
        let when = match action {
            AlterKind::RenameColumn { .. } => " after rename",
            AlterKind::DropColumn { .. } => " after drop column",
            _ => return Ok(()),
        };
        let Some(columns) = new_column_names(updates, folded) else {
            return Ok(());
        };
        // Every row in the order it was created, each in its new form when the
        // statement rewrites it: an index on a dropped column is not rewritten
        // and is the row that has to be found.
        for held in self.entries_of(at) {
            let entry = updates
                .iter()
                .find(|(rowid, _)| *rowid == held.rowid)
                .map_or(&held.entry, |(_, moved)| moved);
            let problem = match entry.kind {
                ObjectKind::Table if entry.name.to_ascii_lowercase() == folded => {
                    table_text_problem(&entry.sql)
                }
                ObjectKind::Index if entry.table.to_ascii_lowercase() == folded => {
                    index_text_problem(&entry.sql, &columns)
                }
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

    /// Checks the schema after the change, for the actions SQLite checks twice.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    /// @param action - what the statement does
    pub(super) fn check_schema_after(
        &mut self,
        at: usize,
        folded: &[u8],
        action: &AlterKind,
    ) -> DbResult<()> {
        match action {
            AlterKind::RenameColumn { .. } => self.schema_still_binds(at, " after rename"),
            AlterKind::DropColumn { .. } => self.schema_still_binds(at, " after drop column"),
            AlterKind::AddColumn { .. } => self.added_column_keeps_the_rows_valid(at, folded),
            _ => Ok(()),
        }
    }

    /// Refuses an `ADD COLUMN` that SQLite refuses because rows already exist.
    ///
    /// All of these are `SQLITE_ERROR` (1), and all of them are only an error
    /// when the table has a row: an empty table takes any of them. The order is
    /// the order SQLite tests them in, which decides the message when a column
    /// breaks more than one.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    /// @param risk - what the binder found in the new column's definition
    pub(super) fn refuse_an_unaddable_column(
        &mut self,
        at: usize,
        folded: &[u8],
        risk: &AddedColumnRisk,
    ) -> DbResult<()> {
        let Some(message) = risk.refusal(self.pragmas.foreign_keys()) else {
            return Ok(());
        };
        match self.table_has_a_row(at, folded)? {
            true => Err(statement_refusal(message)),
            false => Ok(()),
        }
    }

    /// Checks the rows a table already holds against its constraints after a
    /// column is added.
    ///
    /// SQLite runs `quick_check` for this and reports `CHECK constraint failed`
    /// or `NOT NULL constraint failed`. A new column's default is the only value
    /// that can break a constraint, so the table's `CHECK` expressions are
    /// evaluated over the rows and a generated column is tested for NULL.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    fn added_column_keeps_the_rows_valid(&mut self, at: usize, folded: &[u8]) -> DbResult<()> {
        let Some(info) = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == folded)
            .cloned()
        else {
            return Ok(());
        };
        let generated_not_null = info
            .columns
            .last()
            .is_some_and(|column| column.generated && column.not_null);
        if (info.checks.is_empty() && !generated_not_null) || !self.table_has_a_row(at, folded)? {
            return Ok(());
        }
        let name = String::from_utf8_lossy(&info.name).replace('"', "\"\"");
        let database = String::from_utf8_lossy(CatalogView::database_name(
            &self.schema.catalog,
            info.database,
        ))
        .replace('"', "\"\"");
        for check in &info.checks {
            let expression = String::from_utf8_lossy(&check.expr_sql).into_owned();
            let sql =
                format!("SELECT 1 FROM \"{database}\".\"{name}\" WHERE NOT ({expression}) LIMIT 1");
            if !self.query_internally(&sql)?.is_empty() {
                return Err(constraint_error("CHECK constraint failed"));
            }
        }
        if let (true, Some(column)) = (generated_not_null, info.columns.last()) {
            let column = String::from_utf8_lossy(&column.name).replace('"', "\"\"");
            let sql = format!(
                "SELECT 1 FROM \"{database}\".\"{name}\" WHERE \"{column}\" IS NULL LIMIT 1"
            );
            if !self.query_internally(&sql)?.is_empty() {
                return Err(constraint_error("NOT NULL constraint failed"));
            }
        }
        Ok(())
    }
}

/// Returns the failure SQLite gives when rows already in a table break a
/// constraint an `ALTER TABLE` adds.
///
/// It carries the constraint result code (19), which is what the statement
/// reports, and not the generic error code the neighbouring refusals carry.
///
/// @param said - the message
fn constraint_error(said: &str) -> inillucent_base::error::DbError {
    inillucent_base::error::DbError::primary(inillucent_base::error::PrimaryCode::Constraint)
        .with_message(said.to_string())
        .with_detail(said.to_string())
}

/// Returns the part of the statement text between two offsets, trimmed.
///
/// A constraint is stored exactly as the statement wrote it, spacing and
/// comments included, so it is a slice of the source rather than something
/// rebuilt from the parse.
///
/// @param source - the statement text
/// @param start - where the constraint starts
/// @param end - where it ends
fn statement_text(source: &[u8], start: u32, end: u32) -> Vec<u8> {
    let span = inillucent_sql::lexer::Span::new(start as usize, end as usize);
    alter_text::trimmed_span(source, span)
}

/// Returns the folded column names the altered table will have, from its new text.
///
/// @param updates - the rows about to be written
/// @param folded - the table's folded name
fn new_column_names(updates: &[(i64, SchemaEntry)], folded: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (_, entry) = updates.iter().find(|(_, entry)| {
        entry.kind == ObjectKind::Table && entry.name.to_ascii_lowercase() == folded
    })?;
    let parsed = inillucent_sql::parser::parse_next_statement(
        &entry.sql,
        0,
        &inillucent_base::limits::Limits::default(),
    )
    .ok()?;
    let inillucent_sql::ast::Statement::CreateTable {
        body: inillucent_sql::ast::CreateTableBody::Columns { columns, .. },
        ..
    } = &parsed.statement
    else {
        return None;
    };
    Some(
        columns
            .iter()
            .map(|column| parsed.ast.folded(column.name).to_vec())
            .collect(),
    )
}

/// Returns the new form of one schema row, or `None` when the action leaves it alone.
///
/// @param entry - the row
/// @param context - what is being done
fn altered_entry(entry: &SchemaEntry, context: &Alteration<'_>) -> DbResult<Option<SchemaEntry>> {
    let owns = entry.table.to_ascii_lowercase() == context.folded;
    let itself =
        entry.name.to_ascii_lowercase() == context.folded && entry.kind == ObjectKind::Table;
    if entry.sql.is_empty() {
        return Ok(renamed_automatic_index(entry, context, owns));
    }
    let sql = match context.action {
        AlterKind::RenameTable { to } => {
            let which = rename::TableRewrite {
                references: !context.legacy || context.foreign_keys,
                bodies: !context.legacy,
            };
            rename::rewrite_table(&entry.sql, context.table, to, which)?
        }
        AlterKind::RenameColumn {
            from,
            to,
            to_quoted,
        } => {
            let from = from.to_ascii_lowercase();
            let ren = rename::ColumnRename {
                table: context.folded,
                from: &from,
                to,
                to_quoted: *to_quoted,
                known: &context.known,
            };
            let owner = entry.table.to_ascii_lowercase();
            rename::rewrite_column(&entry.sql, entry.kind, &owner, itself, &ren)?
        }
        AlterKind::AddColumn { start, end, .. } if itself => {
            let definition = context
                .source
                .get(*start as usize..*end as usize)
                .unwrap_or_default();
            rename::add_column_definition(&entry.sql, definition)?
        }
        AlterKind::DropColumn { position, .. } if itself => {
            rename::drop_column(&entry.sql, usize::from(*position))?
        }
        AlterKind::SetNotNull {
            position,
            start,
            end,
            ..
        } if itself => {
            let clause = statement_text(context.source, *start, *end);
            match alter_text::set_not_null(&entry.sql, usize::from(*position), &clause)? {
                Some(text) => text,
                None => return Ok(None),
            }
        }
        AlterKind::DropNotNull { position, .. } if itself => {
            match alter_text::drop_not_null(&entry.sql, usize::from(*position))? {
                Some(text) => text,
                None => return Ok(None),
            }
        }
        AlterKind::AddCheck { start, end, .. } if itself => {
            let constraint = statement_text(context.source, *start, *end);
            rename::add_column(&entry.sql, &constraint)?
        }
        AlterKind::DropConstraint { name } if itself => {
            match alter_text::drop_constraint(&entry.sql, &name.to_ascii_lowercase())? {
                alter_text::Dropped::Text(text) => text,
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    };
    if sql == entry.sql && !(owns && matches!(context.action, AlterKind::RenameTable { .. })) {
        return Ok(None);
    }
    let mut moved = entry.clone();
    moved.sql = rename::reparsed(sql)?;
    if let AlterKind::RenameTable { to } = context.action {
        if itself {
            moved.name = to.clone();
            moved.table = to.clone();
        } else if owns {
            moved.table = to.clone();
        }
    }
    Ok(Some(moved))
}

/// Returns the renamed row of an automatic index, which has no statement of its own.
///
/// Its `tbl_name` and its generated name still follow a rename.
///
/// @param entry - the row
/// @param context - what is being done
/// @param owns - whether the row belongs to the altered table
fn renamed_automatic_index(
    entry: &SchemaEntry,
    context: &Alteration<'_>,
    owns: bool,
) -> Option<SchemaEntry> {
    let AlterKind::RenameTable { to } = context.action else {
        return None;
    };
    if !owns {
        return None;
    }
    let mut moved = entry.clone();
    moved.table = to.clone();
    moved.name = super::renamed_automatic(&entry.name, context.table, to);
    Some(moved)
}
