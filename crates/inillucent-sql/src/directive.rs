//! Statements the session carries out itself rather than compiling.
//!
//! Invariant: a directive is a decision, already resolved, with nothing left
//! to look up. Binding a `DROP TABLE` resolves the name and refuses a missing
//! one here; what reaches the session is "free this root page and remove this
//! `sqlite_schema` row", not a name it has to resolve again.
//!
//! Transaction control and DDL are here rather than in the bytecode for a
//! reason the TDD's own DDL protocol describes: their steps are catalog
//! publication, cookie invalidation and lock transitions, none of which the
//! machine's register-and-cursor model expresses. They still run inside the
//! same transaction machinery as DML - the statement savepoint, the journal
//! and the commit are identical - which is what the protocol actually
//! requires. The row-touching part of DDL is ordinary storage work and goes
//! through the same pager as everything else.

use crate::ast::{self, ObjectKind, TransactionBehaviour};
use crate::bind::{no_such_table, refused, schema_refused, unsupported, Binder, BoundExpr};
use crate::catalog_view::CatalogView;
use crate::catalog_view::TableKind;
use crate::diagnostic::ParseError;
use crate::lexer::Span;
use inillucent_value::Collation;

/// Returns the direct children of an expression node.
///
/// The arena has no walker of its own, and the only caller that needs one is
/// the generated-column check, so it lives beside it rather than becoming a
/// method every other reader would have to ignore.
pub(crate) fn expression_children(ast: &crate::ast::Ast, expr: ast::ExprId) -> Vec<ast::ExprId> {
    let mut out = Vec::new();
    let Some(node) = ast.expr(expr) else {
        return out;
    };
    match node {
        ast::Expr::Unary { operand, .. } => out.push(*operand),
        ast::Expr::Binary { left, right, .. } => {
            out.push(*left);
            out.push(*right);
        }
        ast::Expr::Collate { operand, .. } | ast::Expr::Cast { operand, .. } => out.push(*operand),
        ast::Expr::IsNull { operand, .. } => out.push(*operand),
        ast::Expr::Raise {
            message: Some(message),
            ..
        } => out.push(*message),
        ast::Expr::Is { left, right, .. } => {
            out.push(*left);
            out.push(*right);
        }
        ast::Expr::Between {
            operand, low, high, ..
        } => {
            out.push(*operand);
            out.push(*low);
            out.push(*high);
        }
        ast::Expr::In { operand, rhs, .. } => {
            out.push(*operand);
            if let ast::InRhs::List(items) = rhs {
                out.extend(items.iter().copied());
            }
        }
        ast::Expr::Case {
            operand,
            branches,
            otherwise,
        } => {
            if let Some(operand) = operand {
                out.push(*operand);
            }
            for (when, then) in branches {
                out.push(*when);
                out.push(*then);
            }
            if let Some(otherwise) = otherwise {
                out.push(*otherwise);
            }
        }
        ast::Expr::Pattern {
            operand,
            pattern,
            escape,
            ..
        } => {
            out.push(*operand);
            out.push(*pattern);
            if let Some(escape) = escape {
                out.push(*escape);
            }
        }
        ast::Expr::Function {
            arguments: Some(arguments),
            ..
        } => out.extend(arguments.iter().copied()),
        _ => {}
    }
    out
}

/// Returns whether a column is declared `UNIQUE` in its own definition.
///
/// SQLite sets its "unique" flag on a column only for `UNIQUE` written in the
/// column's definition. A column named by a table level `UNIQUE (a, b)` or by
/// `CREATE UNIQUE INDEX` does not get it, and `DROP COLUMN` treats the two
/// differently: only the first is refused up front.
///
/// @param create_sql - the table's stored `CREATE TABLE` text
/// @param position - the column's declared position
fn declared_unique(create_sql: &[u8], position: usize) -> bool {
    let limits = inillucent_base::limits::Limits::default();
    let Ok(parsed) = crate::parser::parse_next_statement(create_sql, 0, &limits) else {
        return false;
    };
    let ast::Statement::CreateTable {
        body: ast::CreateTableBody::Columns { columns, .. },
        ..
    } = &parsed.statement
    else {
        return false;
    };
    columns.get(position).is_some_and(|column| {
        column
            .constraints
            .iter()
            .any(|(_, constraint)| matches!(constraint, ast::ColumnConstraint::Unique(_)))
    })
}

/// Refuses `NULLS FIRST` and `NULLS LAST` on an index key.
///
/// SQLite's grammar accepts them there, because it reads an index key as an ORDER BY term, and
/// then refuses them with a message that points at nothing.
///
/// @param columns - the key columns as written
fn refuse_nulls_order(columns: &[ast::IndexedColumn]) -> Result<(), ParseError> {
    for column in columns {
        let word = match column.nulls {
            Some(ast::NullOrder::First) => "FIRST",
            Some(ast::NullOrder::Last) => "LAST",
            None => continue,
        };
        return Err(refused(
            format!("unsupported use of NULLS {word}"),
            Span::default(),
        ));
    }
    Ok(())
}

/// Returns the failure `RENAME COLUMN` and `DROP COLUMN` give for a column that
/// is not there, which SQLite words with the name in double quotes.
///
/// @param name - the column as the statement wrote it
fn no_such_quoted_column(name: &[u8]) -> ParseError {
    refused(
        format!("no such column: \"{}\"", String::from_utf8_lossy(name)),
        Span::default(),
    )
}

/// Returns the failure an `ALTER TABLE` gives for a name that is not a table.
///
/// SQLite words it differently for each kind of statement when the name is a
/// view.
///
/// @param action - what the statement does
/// @param target - the object that was named
fn not_a_table_message(
    action: &ast::AlterAction,
    target: &crate::catalog_view::TableInfo,
) -> String {
    let name = String::from_utf8_lossy(&target.name).into_owned();
    if target.kind != TableKind::View {
        return format!("cannot alter {name}: not a table");
    }
    match action {
        ast::AlterAction::RenameTo(_) => format!("view {name} may not be altered"),
        ast::AlterAction::RenameColumn { .. } => {
            format!("cannot rename columns of view \"{name}\"")
        }
        ast::AlterAction::AddColumn(_) => "Cannot add a column to a view".to_string(),
        ast::AlterAction::DropColumn(_) => format!("cannot drop column from view \"{name}\""),
        ast::AlterAction::SetNotNull { .. }
        | ast::AlterAction::DropNotNull(_)
        | ast::AlterAction::AddCheck { .. }
        | ast::AlterAction::DropConstraint(_) => {
            format!("cannot edit constraints of view \"{name}\"")
        }
    }
}

/// Returns the failure `REINDEX` gives for a name that is nothing it knows.
///
/// SQLite's message does not name the object, and it points at nothing, so the
/// failure carries no position either. It used to be an `Unexpected` token failure,
/// which printed `near "unable to identify ...": syntax error`.
///
/// @param name - the name that matched no table, index or collation
/// @param span - where the name was written, which the failure does not report
fn no_such_collation_sequence(name: &[u8], span: Span) -> ParseError {
    let _ = (name, span);
    ParseError::new(
        crate::diagnostic::ParseErrorKind::Refused(
            "unable to identify the object to be reindexed".to_string(),
        ),
        Span::default(),
    )
}

/// How an explicit `BEGIN` acquires its rights.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeginKind {
    /// Take nothing until the first read or write needs it.
    Deferred,
    /// Take the writer's reservation now.
    Immediate,
    /// Take the write lock now, excluding readers too.
    Exclusive,
}

impl BeginKind {
    /// Returns the kind a `BEGIN` clause names, defaulting to DEFERRED.
    pub fn of(behaviour: Option<TransactionBehaviour>) -> BeginKind {
        match behaviour {
            None | Some(TransactionBehaviour::Deferred) => BeginKind::Deferred,
            Some(TransactionBehaviour::Immediate) => BeginKind::Immediate,
            Some(TransactionBehaviour::Exclusive) => BeginKind::Exclusive,
        }
    }
}

/// What an added column would do to rows that already exist.
///
/// SQLite refuses `PRIMARY KEY` and `UNIQUE` while it is still compiling,
/// because no table can take them however empty it is. The other three it
/// defers: a `NOT NULL` column with no default, a non-constant default and a
/// `STORED` generated column are refused *only when there is a row to break*,
/// and are accepted on an empty table. That is not a quirk worth smoothing
/// over - it is the difference between a migration that runs on a fresh
/// database and one that runs on a populated one - so the binder records what
/// it saw and the executor, which knows the row count, decides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddedColumnRisk {
    /// `REFERENCES` with a `DEFAULT` that is not `NULL`.
    ///
    /// Only a refusal while `PRAGMA foreign_keys` is on, which the binder does
    /// not know, so [`AddedColumnRisk::refusal`] takes it as an argument.
    pub references_with_default: bool,
    /// `NOT NULL` with nothing to fill the existing rows with.
    pub null_without_default: bool,
    /// A `DEFAULT` the existing rows cannot all be given one answer from.
    pub non_constant_default: bool,
    /// `GENERATED ALWAYS AS (...) STORED`, which needs a value in every record.
    pub generated_stored: bool,
}

impl AddedColumnRisk {
    /// Returns the refusal a table with rows in it owes, in SQLite's wording.
    ///
    /// The capitalisation is the reference's own and is inconsistent between
    /// the four; it is reproduced rather than tidied, because a caller
    /// matching on the message is matching on what SQLite prints. The order is
    /// the order SQLite tests in, which decides the message when a column
    /// breaks more than one rule.
    ///
    /// @param foreign_keys - whether `PRAGMA foreign_keys` is on
    pub fn refusal(&self, foreign_keys: bool) -> Option<&'static str> {
        if foreign_keys && self.references_with_default {
            return Some("Cannot add a REFERENCES column with non-NULL default value");
        }
        if self.null_without_default {
            return Some("Cannot add a NOT NULL column with default value NULL");
        }
        if self.non_constant_default {
            return Some("Cannot add a column with non-constant default");
        }
        if self.generated_stored {
            return Some("cannot add a STORED column");
        }
        None
    }
}

/// What an `ALTER TABLE` does, with every name already resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlterKind {
    /// `RENAME TO`.
    RenameTable {
        /// The new name, as written.
        to: Vec<u8>,
    },
    /// `RENAME COLUMN a TO b`.
    RenameColumn {
        /// The column's current name, as stored.
        from: Vec<u8>,
        /// Its new name, as written.
        to: Vec<u8>,
        /// Whether the new name was written quoted, which makes every
        /// occurrence in the schema quoted.
        to_quoted: bool,
    },
    /// `ADD COLUMN`.
    AddColumn {
        /// Where the definition starts in the statement's own source.
        ///
        /// The offsets rather than the text, for the same reason `CREATE TABLE`
        /// carries an offset: the executor has the statement's source and
        /// slicing it there keeps the *written* definition - its spacing, its
        /// case and its comments - rather than something re-rendered from the
        /// parse.
        start: u32,
        /// Where it ends.
        end: u32,
        /// What it would do to rows that already exist.
        risk: AddedColumnRisk,
    },
    /// An `ADD COLUMN` that SQLite refuses only after it has changed the schema.
    AddColumnFailsAfter {
        /// The full message, for example `error in table t after add column: ...`.
        message: String,
    },
    /// `DROP COLUMN`.
    DropColumn {
        /// The column's name, as stored.
        name: Vec<u8>,
        /// Its declared position, which is the record slot to remove.
        position: u16,
    },
    /// `ALTER COLUMN ... SET NOT NULL`.
    SetNotNull {
        /// The column's name, as stored.
        name: Vec<u8>,
        /// Its declared position.
        position: u16,
        /// Where `NOT NULL` starts in the statement's own source.
        start: u32,
        /// Where the clause ends.
        end: u32,
    },
    /// `ALTER COLUMN ... DROP NOT NULL`.
    DropNotNull {
        /// The column's name, as stored.
        name: Vec<u8>,
        /// Its declared position.
        position: u16,
    },
    /// `ADD [CONSTRAINT name] CHECK (...)`.
    AddCheck {
        /// The constraint's name, when it has one.
        name: Option<Vec<u8>>,
        /// Where the constraint starts in the statement's own source.
        start: u32,
        /// Where it ends.
        end: u32,
        /// Where the predicate starts in the statement's own source.
        expr_start: u32,
        /// Where the predicate ends.
        expr_end: u32,
    },
    /// `DROP CONSTRAINT name`.
    DropConstraint {
        /// The constraint's name, as written.
        name: Vec<u8>,
    },
}

/// One key column of an index being created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexKeyColumn {
    /// The table column, when the key is a bare column.
    ///
    /// `None` for a key that is an expression. It was a bare `u16` while
    /// `CREATE INDEX ix ON t(lower(a))` was refused in the binder; the field is
    /// an `Option` now so that a reader which needs a column - a module-backed
    /// index, say - has to say what it does when there is not one, rather than
    /// reading a position that was invented to fill the slot.
    pub column: Option<u16>,
    /// The key expression, as written, when the key is one.
    pub expr_sql: Option<Vec<u8>>,
    /// The folded collation name.
    pub collation: Vec<u8>,
    /// Whether the key is stored descending.
    pub descending: bool,
}

/// A statement the session carries out.
#[derive(Clone, Debug, PartialEq)]
pub enum Directive {
    /// `BEGIN`.
    Begin(BeginKind),
    /// `COMMIT` or `END`.
    Commit,
    /// `ROLLBACK`, or `ROLLBACK TO savepoint`.
    Rollback {
        /// The savepoint to roll back to, when one was named.
        savepoint: Option<Vec<u8>>,
    },
    /// `SAVEPOINT name`.
    Savepoint(Vec<u8>),
    /// `RELEASE name`.
    Release(Vec<u8>),
    /// `CREATE TABLE`.
    CreateTable {
        /// Whether `IF NOT EXISTS` was written.
        if_not_exists: bool,
        /// Which attached database.
        database: usize,
        /// The table name as written.
        name: Vec<u8>,
        /// The byte the name starts at in the statement's source.
        name_offset: u32,
        /// Whether the table already exists.
        exists: bool,
        /// A failure the statement reports when it runs rather than when it is prepared.
        ///
        /// **A generated column loop is found by running a query.** SQLite finishes
        /// `CREATE TABLE` by running `SELECT * FROM` the new table, and that query is
        /// where `generated column loop on "c"` comes from, so the shell prints it as
        /// `Error near line N`, not `Parse error`. Nothing is created when it is set.
        refusal: Option<String>,
    },
    /// `CREATE TABLE ... AS SELECT`.
    ///
    /// A `CREATE` whose column list comes from a plan, which is why it is a
    /// directive of its own rather than a flag on the one above: everything
    /// about the table - its column names, and the declared types it inherits
    /// from the query's origin columns - is decided by binding the query, and
    /// the `CREATE` text that is stored is *synthesised* rather than being a
    /// slice of what was typed.
    CreateTableAsSelect {
        /// Whether `IF NOT EXISTS` was written.
        if_not_exists: bool,
        /// Which attached database.
        database: usize,
        /// The table name as written.
        name: Vec<u8>,
        /// Whether the table already exists.
        exists: bool,
        /// The `CREATE TABLE name(...)` text to store, built from the query.
        create_sql: Vec<u8>,
        /// The `SELECT` that fills it, as the source text it was written as.
        ///
        /// The text rather than the bound query, because the rows are inserted
        /// by an ordinary `INSERT INTO name <select>` compiled against the
        /// schema *after* the table exists - which is one implementation of
        /// what an insert means rather than a second one written here.
        select_sql: Vec<u8>,
    },
    /// `CREATE VIRTUAL TABLE`.
    CreateVirtualTable {
        /// Whether `IF NOT EXISTS` was written.
        if_not_exists: bool,
        /// Which attached database.
        database: usize,
        /// The table name as written.
        name: Vec<u8>,
        /// The module name as written.
        module: Vec<u8>,
        /// The arguments inside the parentheses, as written.
        arguments: Vec<Vec<u8>>,
        /// The byte the name starts at in the statement's source.
        name_offset: u32,
        /// Whether the table already exists.
        exists: bool,
    },
    /// `ALTER TABLE`.
    Alter {
        /// Which attached database.
        database: usize,
        /// The table being altered, by its stored name.
        table: Vec<u8>,
        /// What to do to it.
        action: AlterKind,
    },
    /// `REINDEX`, over one index, one table's indexes, or everything.
    Reindex {
        /// Which attached database.
        database: usize,
        /// The indexes to rebuild, by name.
        indexes: Vec<Vec<u8>>,
    },
    /// `VACUUM`, which rebuilds the database into a fresh file.
    Vacuum {
        /// Which attached database.
        database: usize,
        /// The file `VACUUM INTO` writes the rebuilt copy to.
        ///
        /// A string literal, as SQLite's grammar has it. `INTO` leaves the
        /// database it was run on completely alone, which is the difference
        /// between the two forms and the reason the path is carried rather
        /// than resolved here.
        into: Option<Vec<u8>>,
        /// The text of an `INTO` expression that is not a string literal, such
        /// as `(SELECT n FROM p)` or `'a' || 'b'`, which the engine evaluates
        /// when the statement runs. `into` is `None` when this is set.
        into_sql: Option<String>,
    },
    /// `ATTACH`, which adds a database file to this connection.
    Attach {
        /// The file to open, as the literal it was written as.
        file: Vec<u8>,
        /// The name it will be known by.
        schema: Vec<u8>,
        /// The `KEY` clause's text: the file's encryption key, or empty for a
        /// plaintext file. `None` when there was no `KEY` clause, which means
        /// the connection's own key.
        key: Option<Vec<u8>>,
    },
    /// `DETACH`, which removes one.
    Detach {
        /// The name it was attached under.
        schema: Vec<u8>,
    },
    /// `ANALYZE`, over one object or the whole schema.
    Analyze {
        /// Which attached database.
        database: usize,
        /// The one table or index to measure, or nothing for all of them.
        table: Option<Vec<u8>>,
        /// Whether the statement was a bare `ANALYZE`, which measures every
        /// database but `temp` rather than `database` alone.
        every_schema: bool,
    },
    /// `CREATE VIEW`.
    CreateView {
        /// Whether `IF NOT EXISTS` was written.
        if_not_exists: bool,
        /// Which attached database.
        database: usize,
        /// The view name as written.
        name: Vec<u8>,
        /// The byte the name starts at in the statement's source.
        name_offset: u32,
        /// Whether the view already exists.
        exists: bool,
    },
    /// `CREATE TRIGGER`.
    CreateTrigger {
        /// Which attached database.
        database: usize,
        /// The trigger name as written.
        name: Vec<u8>,
        /// The byte the name starts at in the statement's source.
        name_offset: u32,
        /// The table or view the trigger is attached to.
        table: Vec<u8>,
        /// Whether the trigger already exists.
        exists: bool,
    },
    /// `CREATE INDEX`.
    CreateIndex {
        /// Whether `UNIQUE` was written.
        unique: bool,
        /// Whether `IF NOT EXISTS` was written.
        if_not_exists: bool,
        /// Which attached database.
        database: usize,
        /// The index name as written.
        name: Vec<u8>,
        /// The byte the name starts at in the statement's source.
        name_offset: u32,
        /// The table it indexes.
        table: Vec<u8>,
        /// The root page of that table.
        table_root: u32,
        /// The module named by `USING`, folded, when one was.
        using: Option<Vec<u8>>,
        /// The key columns.
        columns: Vec<IndexKeyColumn>,
        /// The storage parameters `WITH ( ... )` named, checked against the
        /// module that will read them.
        settings: Vec<(Vec<u8>, Vec<u8>)>,
        /// Whether the index already exists.
        exists: bool,
    },
    /// `DROP TABLE` or `DROP INDEX`.
    Drop {
        /// Which kind of object.
        kind: ObjectKind,
        /// Whether `IF EXISTS` was written.
        if_exists: bool,
        /// Which attached database.
        database: usize,
        /// The object name.
        name: Vec<u8>,
        /// The root page to free, or zero when the object has none.
        root: u32,
        /// The root pages of the indexes a `DROP TABLE` takes with it.
        index_roots: Vec<u32>,
        /// Whether the object exists.
        exists: bool,
    },
    /// `PRAGMA`.
    Pragma {
        /// The schema the pragma was qualified with, when one was written.
        ///
        /// `PRAGMA aux.table_info(t)` asks about the attached database rather
        /// than about `main`, and a pragma that dropped the qualifier would
        /// answer confidently about the wrong file.
        database: Option<usize>,
        /// The pragma name, folded.
        name: Vec<u8>,
        /// The argument, when one was written.
        argument: Option<PragmaArgument>,
    },
}

/// What a `PRAGMA` was given.
#[derive(Clone, Debug, PartialEq)]
pub enum PragmaArgument {
    /// A bare word, such as `PRAGMA journal_mode = WAL`.
    Name(Vec<u8>),
    /// An expression, such as `PRAGMA user_version = 4`.
    Value(BoundExpr),
}

/// Whether a `CREATE INDEX` declared `UNIQUE`.
///
/// **An enum rather than a `bool` beside another `bool` (task-1962, A9).**
/// `bind_create_index` took `unique` and `if_not_exists` adjacent and
/// positional; swapping them compiles and declares a unique index where the
/// statement asked for `IF NOT EXISTS`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Uniqueness {
    /// `CREATE UNIQUE INDEX`: two rows may not share a key.
    Unique,
    /// `CREATE INDEX`: a key may repeat.
    Duplicates,
}

/// Whether a `CREATE` declared `IF NOT EXISTS`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IfNotExists {
    /// The statement is a no-op when the object is already there.
    Skip,
    /// The statement fails when the object is already there.
    Refuse,
}

/// Everything a `CREATE INDEX` statement names.
///
/// The grammar's own fields, gathered rather than passed as nine positional
/// arguments of which two were adjacent booleans.
pub struct CreateIndexSpec<'a> {
    /// Whether the index refuses a repeated key.
    pub unique: Uniqueness,
    /// What to do when the index is already there.
    pub if_not_exists: IfNotExists,
    /// The schema the index is created in, when one was written.
    pub database: Option<ast::NameId>,
    /// The index's name.
    pub name: ast::NameId,
    /// The table it is over.
    pub table: ast::NameId,
    /// The module named by `USING`, for the extension index forms.
    pub using: Option<ast::NameId>,
    /// The indexed columns, in key order.
    pub columns: &'a [ast::IndexedColumn],
    /// The `WITH` settings, as written.
    pub settings: &'a [Vec<u8>],
    /// The `WHERE` of a partial index, as an expression of the statement.
    pub filter: Option<ast::ExprId>,
}

/// The fields of a `CREATE TRIGGER`, passed as one argument.
///
/// Ten parameters is past the point where their order is checkable by reading,
/// and every one of them is a field of the statement rather than something
/// computed here.
pub(crate) struct CreateTriggerParts<'p> {
    /// Whether `TEMP` was written.
    pub temporary: bool,
    /// Whether `IF NOT EXISTS` was written.
    pub if_not_exists: bool,
    /// The schema qualifier.
    pub database: Option<ast::NameId>,
    /// The trigger name.
    pub name: ast::NameId,
    /// When it fires.
    pub time: Option<ast::TriggerTime>,
    /// The table it is attached to.
    pub table: ast::NameId,
    /// The schema qualifier on the table.
    pub table_database: Option<ast::NameId>,
    /// Whether `FOR EACH ROW` was written.
    pub for_each_row: bool,
    /// The `WHEN` guard.
    pub when: Option<ast::ExprId>,
    /// The body statements.
    pub body: &'p [ast::Statement],
}

impl<'a> Binder<'a> {
    /// Binds a statement the session carries out itself.
    pub fn bind_directive(&mut self, statement: &ast::Statement) -> Result<Directive, ParseError> {
        match statement {
            ast::Statement::Begin { behaviour } => Ok(Directive::Begin(BeginKind::of(*behaviour))),
            ast::Statement::Commit => Ok(Directive::Commit),
            ast::Statement::Rollback { savepoint } => Ok(Directive::Rollback {
                savepoint: savepoint.map(|id| self.ast.text(id).to_vec()),
            }),
            ast::Statement::Savepoint(name) => {
                Ok(Directive::Savepoint(self.ast.text(*name).to_vec()))
            }
            ast::Statement::Release(name) => Ok(Directive::Release(self.ast.text(*name).to_vec())),
            ast::Statement::CreateTable {
                temporary,
                if_not_exists,
                database,
                name,
                body,
            } => self.bind_create_table(*temporary, *if_not_exists, *database, *name, body),
            ast::Statement::CreateVirtualTable {
                if_not_exists,
                database,
                name,
                module,
                arguments,
            } => {
                self.bind_create_virtual_table(*if_not_exists, *database, *name, *module, arguments)
            }
            ast::Statement::CreateIndex {
                unique,
                if_not_exists,
                database,
                name,
                table,
                using,
                columns,
                settings,
                filter,
            } => self.bind_create_index(&CreateIndexSpec {
                unique: if *unique {
                    Uniqueness::Unique
                } else {
                    Uniqueness::Duplicates
                },
                if_not_exists: if *if_not_exists {
                    IfNotExists::Skip
                } else {
                    IfNotExists::Refuse
                },
                database: *database,
                name: *name,
                table: *table,
                using: *using,
                columns,
                settings,
                filter: *filter,
            }),
            ast::Statement::Analyze { database, name } => self.bind_analyze(*database, *name),
            ast::Statement::AlterTable {
                database,
                table,
                action,
            } => self.bind_alter(*database, *table, action),
            ast::Statement::Reindex { database, name } => self.bind_reindex(*database, *name),
            ast::Statement::Vacuum { database, into } => self.bind_vacuum(*database, *into),
            ast::Statement::Attach { file, schema, key } => self.bind_attach(*file, *schema, *key),
            ast::Statement::Detach { schema } => self.bind_detach(*schema),
            ast::Statement::CreateView {
                temporary,
                if_not_exists,
                database,
                name,
                columns,
                select,
            } => self.bind_create_view(
                *temporary,
                *if_not_exists,
                *database,
                *name,
                columns,
                *select,
            ),
            ast::Statement::CreateTrigger {
                temporary,
                if_not_exists,
                database,
                name,
                time,
                event: _,
                table,
                table_database,
                for_each_row,
                when,
                body,
            } => self.bind_create_trigger(CreateTriggerParts {
                temporary: *temporary,
                if_not_exists: *if_not_exists,
                database: *database,
                name: *name,
                time: *time,
                table: *table,
                table_database: *table_database,
                for_each_row: *for_each_row,
                when: *when,
                body,
            }),
            ast::Statement::Drop {
                kind,
                if_exists,
                database,
                name,
            } => self.bind_drop(*kind, *if_exists, *database, *name),
            ast::Statement::Pragma {
                database,
                name,
                value,
            } => self.bind_pragma(*database, *name, value),
            _ => Err(unsupported(
                "this statement is not implemented yet",
                Span::default(),
            )),
        }
    }

    /// Binds a `CREATE TABLE`.
    fn bind_create_virtual_table(
        &mut self,
        if_not_exists: bool,
        database: Option<ast::NameId>,
        name: ast::NameId,
        module: ast::NameId,
        arguments: &[Vec<u8>],
    ) -> Result<Directive, ParseError> {
        let index = self.resolve_database(database)?;
        let written = self.ast.text(name).to_vec();
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let folded = self.ast.folded(name).to_vec();
        let database_name = self.catalog.database_name(index).to_vec();
        let exists = self
            .catalog
            .find_table(Some(database_name.as_slice()), &folded)
            .is_some();
        if exists && !if_not_exists {
            return Err(self.already_exists(&database_name, &folded, name));
        }
        if !exists {
            self.refuse_index_namesake(&database_name, &folded, &written)?;
        }
        Ok(Directive::CreateVirtualTable {
            if_not_exists,
            database: index,
            name: written,
            module: self.ast.text(module).to_vec(),
            arguments: arguments.to_vec(),
            name_offset: self
                .ast
                .name(name)
                .map(|entry| entry.span.start)
                .unwrap_or_default(),
            exists,
        })
    }

    /// Binds `CREATE TABLE`, refusing what the file format cannot hold.
    fn bind_create_table(
        &mut self,
        temporary: bool,
        if_not_exists: bool,
        database: Option<ast::NameId>,
        name: ast::NameId,
        body: &ast::CreateTableBody,
    ) -> Result<Directive, ParseError> {
        let temp = self.temporary_database(temporary, database, false)?;
        // **Two bodies, and the second one is built.** This used to be written
        // as two `let ... else` bindings, the inner one answering
        // `unsupported("CREATE TABLE ... AS SELECT")` - an arm no statement
        // could reach, because `CreateTableBody` has exactly these two
        // variants, so a feature that works was described by a refusal
        // (task-1979, section 8.3). A match over both says the same thing with
        // nothing left over.
        let (columns, constraints, without_rowid, strict) = match body {
            ast::CreateTableBody::AsSelect(select) => {
                return self.bind_create_table_as_select(
                    temp,
                    if_not_exists,
                    database,
                    name,
                    *select,
                )
            }
            ast::CreateTableBody::Columns {
                columns,
                constraints,
                without_rowid,
                strict,
            } => (columns, constraints, without_rowid, strict),
        };
        // First, because SQLite meets `PRIMARY KEY(... AUTOINCREMENT)` before it
        // looks at what the key names.
        self.check_table_autoincrement(columns, constraints, *without_rowid)?;
        for (_, constraint) in constraints {
            match constraint {
                ast::TableConstraint::PrimaryKey { columns, .. }
                | ast::TableConstraint::Unique { columns, .. } => refuse_nulls_order(columns)?,
                _ => {}
            }
        }
        self.check_table_shape(self.ast.text(name), columns, constraints)?;
        self.check_table_declarations(self.ast.text(name), columns, constraints)?;
        if *without_rowid && !self.declares_primary_key(columns, constraints) {
            return Err(schema_refused(
                format!(
                    "PRIMARY KEY missing on table {}",
                    String::from_utf8_lossy(self.ast.text(name))
                ),
                Span::default(),
            ));
        }
        self.check_autoincrement(columns, *without_rowid)?;
        self.check_column_collations(columns)?;
        if *strict {
            self.check_strict(columns, name)?;
        }
        let refusal = self.check_generated(columns, constraints)?;
        self.refuse_all_generated(columns)?;
        if columns.is_empty() {
            return Err(refused(
                "a table must have at least one column",
                Span::default(),
            ));
        }
        let index = match temp {
            Some(index) => index,
            None => self.resolve_database(database)?,
        };
        let written = self.ast.text(name).to_vec();
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let folded = self.ast.folded(name).to_vec();
        let database_name = self.catalog.database_name(index).to_vec();
        let exists = self
            .catalog
            .find_table(Some(database_name.as_slice()), &folded)
            .is_some();
        if exists && !if_not_exists {
            return Err(self.already_exists(&database_name, &folded, name));
        }
        if !exists {
            self.refuse_index_namesake(&database_name, &folded, &written)?;
        }
        self.record_write_dependency(index);
        Ok(Directive::CreateTable {
            if_not_exists,
            database: index,
            name: written,
            name_offset: self.name_offset(name),
            exists,
            refusal,
        })
    }

    /// Binds `CREATE TABLE ... AS SELECT`.
    ///
    /// **The column list comes from a plan**, which is the whole of why this is
    /// a shape of its own. SQLite takes the table's columns from the query's
    /// result columns: the name each one reports, and the declared type it
    /// carries when it is a plain reference to a column that has one. So
    /// `CREATE TABLE u AS SELECT a*2 AS d, b, c FROM t` on `t(a INTEGER, b TEXT,
    /// c REAL)` stores `CREATE TABLE u(d,b TEXT,c REAL)` - `d` is an expression
    /// and inherits nothing, and the other two inherit their origin's type.
    ///
    /// The rows are inserted afterwards by an ordinary `INSERT INTO name
    /// <select>`, compiled against the schema once the table is in it. That is
    /// one implementation of what an insert means rather than a second one
    /// written into the DDL path, and it is what makes the affinity Part B4
    /// applies reach these rows too.
    ///
    /// @param temp - the temporary database's index, when `TEMP` was written
    /// @param if_not_exists - whether `IF NOT EXISTS` was written
    /// @param database - the schema qualifier, when one was written
    /// @param name - the table's name
    /// @param select - the query the table is built from
    fn bind_create_table_as_select(
        &mut self,
        temp: Option<usize>,
        if_not_exists: bool,
        database: Option<ast::NameId>,
        name: ast::NameId,
        select: ast::SelectId,
    ) -> Result<Directive, ParseError> {
        let index = match temp {
            Some(index) => index,
            None => self.resolve_database(database)?,
        };
        let written = self.ast.text(name).to_vec();
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let folded = self.ast.folded(name).to_vec();
        let database_name = self.catalog.database_name(index).to_vec();
        let exists = self
            .catalog
            .find_table(Some(database_name.as_slice()), &folded)
            .is_some();
        if exists && !if_not_exists {
            return Err(self.already_exists(&database_name, &folded, name));
        }
        if !exists {
            self.refuse_index_namesake(&database_name, &folded, &written)?;
        }
        let span = self
            .ast
            .select(select)
            .map(|held| held.span)
            .ok_or_else(|| refused("the query could not be read", Span::default()))?;
        let select_sql = self
            .source
            .get(span.start as usize..span.end as usize)
            .ok_or_else(|| refused("the query could not be read", span))?
            .to_vec();
        // Bound rather than merely parsed, because binding is what resolves the
        // result columns' names and origins - and because a query that does not
        // bind has to be refused here rather than after the table exists.
        let bound = self.bind_select(select)?;
        if bound.columns.is_empty() {
            return Err(refused(
                "a table must have at least one column",
                Span::default(),
            ));
        }
        // **The declaration a `CREATE TABLE ... AS SELECT` stores is the
        // *affinity*, not the source column's declared type.** SQLite writes
        // `a INT` for a source column declared `INTEGER` and `b TEXT` for one
        // declared `VARCHAR(3)`, because what survives a query is the affinity
        // and nothing else - the width, the precision and the spelling are
        // properties of the source table that the copy does not have. Storing
        // `VARCHAR(3)` here claimed a constraint the new table does not
        // enforce, and made the two schemas differ for every CTAS.
        //
        // The line break is SQLite's own rule too, so the stored text matches
        // byte for byte: the name lengths are added up first, and a wide
        // declaration is written one column per line.
        // Two columns that share a name are told apart the way a derived table
        // tells them apart: `SELECT a, a` makes a table of `a` and `a:1`.
        let written_names: Vec<Vec<u8>> = bound
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let names = crate::bind::unique_column_names(&written_names);
        let mut width = identifier_width(&written);
        for name in &names {
            width = width
                .saturating_add(identifier_width(name))
                .saturating_add(5);
        }
        let (open, between, close): (&[u8], &[u8], &[u8]) = if width < 50 {
            (b"", b",", b")")
        } else {
            (b"\n  ", b",\n  ", b"\n)")
        };
        let mut create_sql = Vec::new();
        create_sql.extend_from_slice(b"CREATE TABLE ");
        // Quoted like a column name: a table name with a quote or a space in it
        // must be written as a quoted identifier or the stored text cannot be read.
        create_sql.extend_from_slice(&quoted_name(&written));
        create_sql.push(b'(');
        for (position, (column, name)) in bound.columns.iter().zip(&names).enumerate() {
            create_sql.extend_from_slice(if position > 0 { between } else { open });
            create_sql.extend_from_slice(&quoted_name(name));
            create_sql
                .extend_from_slice(affinity_type(&column.declared_type, column.expr.affinity()));
        }
        create_sql.extend_from_slice(close);
        self.record_write_dependency(index);
        Ok(Directive::CreateTableAsSelect {
            if_not_exists,
            database: index,
            name: written,
            exists,
            create_sql,
            select_sql,
        })
    }

    /// Returns whether a `CREATE TABLE` declares a primary key anywhere.
    fn declares_primary_key(
        &self,
        columns: &[ast::ColumnDef],
        constraints: &[(Option<ast::NameId>, ast::TableConstraint)],
    ) -> bool {
        let on_column = columns.iter().any(|column| {
            column.constraints.iter().any(|(_, constraint)| {
                matches!(constraint, ast::ColumnConstraint::PrimaryKey { .. })
            })
        });
        on_column
            || constraints.iter().any(|(_, constraint)| {
                matches!(constraint, ast::TableConstraint::PrimaryKey { .. })
            })
    }

    /// Checks the rules a generated column has to obey.
    ///
    /// A generated column may not carry a `DEFAULT` - it has no value of its
    /// own to fall back to - may not be part of a rowid table's `PRIMARY KEY`,
    /// and may not refer to a column that does not exist or to itself. The
    /// cycle check is the one that matters: without it a `CREATE TABLE` that
    /// describes one is accepted and every later insert recurses.
    fn check_generated(
        &self,
        columns: &[ast::ColumnDef],
        constraints: &[(Option<ast::NameId>, ast::TableConstraint)],
    ) -> Result<Option<String>, ParseError> {
        let names: Vec<Vec<u8>> = columns
            .iter()
            .map(|column| self.ast.folded(column.name).to_vec())
            .collect();
        let mut generated: Vec<(usize, Vec<usize>)> = Vec::new();
        for (position, column) in columns.iter().enumerate() {
            let Some(expr) = self.check_generated_clauses(column)? else {
                continue;
            };
            let mut reads = Vec::new();
            self.expression_names(expr, &mut reads);
            let mut resolved = Vec::new();
            for name in &reads {
                let Some(found) = names.iter().position(|candidate| candidate == name) else {
                    return Err(crate::bind::no_such_column(name, Span::default()));
                };
                resolved.push(found);
            }
            generated.push((position, resolved));
        }
        self.check_key_has_no_generated(columns, constraints)?;
        // A cycle is anything that never becomes computable: repeat the "every
        // dependency is settled" pass until it stops making progress, and if
        // anything is left it depends on itself, directly or through others.
        let mut settled: Vec<usize> = (0..columns.len())
            .filter(|position| !generated.iter().any(|(owner, _)| owner == position))
            .collect();
        let mut pending = generated;
        loop {
            let before = pending.len();
            let mut still = Vec::new();
            for (position, reads) in pending {
                if reads.iter().all(|read| settled.contains(read)) {
                    settled.push(position);
                } else {
                    still.push((position, reads));
                }
            }
            pending = still;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        // SQLite computes the generated columns in passes and, when a pass makes no
        // progress, names the last column in declaration order that is still waiting.
        // That is the column `pending.last()` holds, because `pending` keeps declaration
        // order. Measured against the pinned shell for loops of two and three columns.
        if let Some((position, _)) = pending.last() {
            let written = columns
                .get(*position)
                .map(|column| String::from_utf8_lossy(self.ast.text(column.name)).into_owned())
                .unwrap_or_default();
            return Ok(Some(format!("generated column loop on \"{written}\"")));
        }
        Ok(None)
    }

    /// Checks the order of the `DEFAULT`, `AS` and `PRIMARY KEY` clauses of one column.
    ///
    /// SQLite meets the clauses in the order they are written. `AS` after a
    /// `DEFAULT` or after another `AS` is `error in generated column "c"`,
    /// `DEFAULT` after `AS` is `cannot use DEFAULT on a generated column`, and a
    /// `PRIMARY KEY` on a generated column is refused whichever comes first.
    /// Returns the generated expression, when the column has one.
    ///
    /// @param column - the column definition
    pub(crate) fn check_generated_clauses(
        &self,
        column: &ast::ColumnDef,
    ) -> Result<Option<ast::ExprId>, ParseError> {
        let mut expr = None;
        let mut has_default = false;
        let mut in_primary_key = false;
        for (_, constraint) in &column.constraints {
            match constraint {
                ast::ColumnConstraint::Generated {
                    expr: body,
                    bad_storage,
                    ..
                } => {
                    if has_default || expr.is_some() || *bad_storage {
                        return Err(refused(
                            format!(
                                "error in generated column \"{}\"",
                                String::from_utf8_lossy(self.ast.text(column.name))
                            ),
                            Span::default(),
                        ));
                    }
                    expr = Some(*body);
                }
                ast::ColumnConstraint::Default(_) => {
                    if expr.is_some() {
                        return Err(refused(
                            "cannot use DEFAULT on a generated column",
                            Span::default(),
                        ));
                    }
                    has_default = true;
                }
                ast::ColumnConstraint::PrimaryKey { .. } => in_primary_key = true,
                _ => {}
            }
        }
        if expr.is_some() && in_primary_key {
            return Err(refused(
                "generated columns cannot be part of the PRIMARY KEY",
                Span::default(),
            ));
        }
        Ok(expr)
    }

    /// Refuses a table level `PRIMARY KEY` that names a generated column.
    ///
    /// A `UNIQUE` constraint may name one.
    ///
    /// @param columns - the table's columns
    /// @param constraints - the table's constraints
    fn check_key_has_no_generated(
        &self,
        columns: &[ast::ColumnDef],
        constraints: &[(Option<ast::NameId>, ast::TableConstraint)],
    ) -> Result<(), ParseError> {
        for (_, constraint) in constraints {
            let ast::TableConstraint::PrimaryKey { columns: keys, .. } = constraint else {
                continue;
            };
            for key in keys {
                let Some(ast::Expr::Column { column: named, .. }) = self.ast.expr(key.expr) else {
                    continue;
                };
                let folded = self.ast.folded(*named);
                let generated = columns.iter().any(|column| {
                    self.ast.folded(column.name) == folded
                        && column.constraints.iter().any(|(_, constraint)| {
                            matches!(constraint, ast::ColumnConstraint::Generated { .. })
                        })
                });
                if generated {
                    return Err(refused(
                        "generated columns cannot be part of the PRIMARY KEY",
                        Span::default(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Collects the folded column names an expression mentions.
    fn expression_names(&self, expr: ast::ExprId, into: &mut Vec<Vec<u8>>) {
        let Some(node) = self.ast.expr(expr) else {
            return;
        };
        if let ast::Expr::Column { column, .. } = node {
            let name = self.ast.folded(*column).to_vec();
            if !into.contains(&name) {
                into.push(name);
            }
        }
        for child in expression_children(self.ast, expr) {
            self.expression_names(child, into);
        }
    }

    /// Refuses a column whose `COLLATE` names a collation sequence that does not exist.
    ///
    /// SQLite looks the name up when the table is created, so
    /// `CREATE TABLE t(a COLLATE nosuch)` fails with `no such collation sequence:
    /// nosuch` and creates nothing. It used to be accepted and fail later, on the
    /// first statement that compared the column.
    ///
    /// @param columns - the column definitions
    fn check_column_collations(&self, columns: &[ast::ColumnDef]) -> Result<(), ParseError> {
        for column in columns {
            for (_, constraint) in &column.constraints {
                let ast::ColumnConstraint::Collate(name) = constraint else {
                    continue;
                };
                let written = self.ast.text(*name);
                if self.collation_named(written).is_none() {
                    return Err(crate::bind::no_such_collation(written, Span::default()));
                }
            }
        }
        Ok(())
    }

    /// Checks the rules a `STRICT` table adds to its column list.
    ///
    /// Every column must name one of six types, and the check is on the
    /// declared text rather than on the affinity it maps to: `VARCHAR(10)` has
    /// TEXT affinity and is still refused, because STRICT is about what was
    /// written and not about what it means.
    ///
    /// SQLite names the column with its table, `missing datatype for t.a` and
    /// `unknown datatype for t.a: "DATETIME"`, with both as written.
    ///
    /// @param columns - the column definitions
    /// @param table - the table's name as written
    fn check_strict(
        &self,
        columns: &[ast::ColumnDef],
        table: ast::NameId,
    ) -> Result<(), ParseError> {
        let table = String::from_utf8_lossy(self.ast.text(table)).into_owned();
        for column in columns {
            let Some(declared) = column.declared_type.as_ref() else {
                return Err(refused(
                    format!(
                        "missing datatype for {table}.{}",
                        String::from_utf8_lossy(self.ast.text(column.name))
                    ),
                    Span::default(),
                ));
            };
            let folded = declared.to_ascii_uppercase();
            let allowed = matches!(
                folded.as_slice(),
                b"INT" | b"INTEGER" | b"REAL" | b"TEXT" | b"BLOB" | b"ANY"
            );
            if !allowed {
                return Err(refused(
                    format!(
                        "unknown datatype for {table}.{}: \"{}\"",
                        String::from_utf8_lossy(self.ast.text(column.name)),
                        String::from_utf8_lossy(declared)
                    ),
                    Span::default(),
                ));
            }
        }
        Ok(())
    }

    /// Binds an `ANALYZE`.
    ///
    /// A bare `ANALYZE` measures everything; one with a name measures that
    /// object. SQLite accepts a database name, an index name or a table name in
    /// the same position and works out which it is, and so does this: the name
    /// is resolved against the tables, then the indexes, and only then refused.
    fn bind_analyze(
        &mut self,
        database: Option<ast::NameId>,
        name: Option<ast::NameId>,
    ) -> Result<Directive, ParseError> {
        let Some(name) = name else {
            // A bare `ANALYZE` is every database but `temp`, which is SQLite's
            // `sqlite3Analyze`.
            let temp = self.catalog.database_index(b"temp");
            for index in 0..self.catalog.database_count() {
                if Some(index) != temp {
                    self.record_write_dependency(index);
                }
            }
            return Ok(Directive::Analyze {
                database: 0,
                table: None,
                every_schema: true,
            });
        };
        let folded = self.ast.folded(name).to_vec();
        // **An unqualified name may be a database's.** `ANALYZE aux` measures
        // every table in `aux`; resolving the name as a table in `main` first
        // made it "no such table: aux".
        if database.is_none() {
            if let Some(index) = self.catalog.database_index(&folded) {
                self.record_write_dependency(index);
                return Ok(Directive::Analyze {
                    database: index,
                    table: None,
                    every_schema: false,
                });
            }
        }
        // An unqualified table or index is searched for in every database, in
        // the usual order; a qualified one only in its own. An index is looked
        // for first, as SQLite does, and is passed on by its own name, because
        // `ANALYZE ix` measures that index alone.
        let schema_name = match database {
            Some(_) => {
                let index = self.resolve_database(database)?;
                Some(self.catalog.database_name(index).to_vec())
            }
            None => None,
        };
        let found = self
            .catalog
            .find_index(schema_name.as_deref(), &folded)
            .map(|(table, index)| (table.database, index.name.clone()))
            .or_else(|| {
                self.catalog
                    .find_table(schema_name.as_deref(), &folded)
                    .map(|table| (table.database, table.name.clone()))
            });
        let Some((index, table)) = found else {
            return Err(no_such_table(self.ast.text(name), Span::default()));
        };
        self.record_write_dependency(index);
        Ok(Directive::Analyze {
            database: index,
            table: Some(table),
            every_schema: false,
        })
    }

    /// Binds an `ALTER TABLE`.
    ///
    /// Every refusal SQLite makes is made here, where the catalog is available,
    /// rather than half-way through rewriting the schema: a rename that is
    /// going to fail must fail before anything has been written.
    fn bind_alter(
        &mut self,
        database: Option<ast::NameId>,
        table: ast::NameId,
        action: &ast::AlterAction,
    ) -> Result<Directive, ParseError> {
        // **An unqualified `ALTER TABLE` searches `temp` before `main`
        // (task-2061).** This resolved every unqualified name through
        // `resolve_database(None)`, which answers `main` and nothing else, and
        // then looked the table up in `main` alone - so
        // `CREATE TEMP TABLE t (a, b); ALTER TABLE t ADD COLUMN c` was
        // `no such table: t` when nothing called `t` was in `main`, and altered
        // `main.t` when something was. SQLite searches `temp` first for an
        // unqualified name in `ALTER TABLE` exactly as it does in a `SELECT`,
        // and `find_table(None, ...)` is already that search - the same one
        // every query goes through - so the schema comes back from the table
        // that was found rather than being decided before the search.
        let written = match database {
            // A qualifier still has to name a database that exists, and it
            // still restricts the search to that one.
            //
            // **A database that does not exist reads as a table that does not
            // exist.** SQLite answers `no such table: nosuch.t` for
            // `ALTER TABLE nosuch.t ...`, and never says "unknown database".
            Some(qualifier) => match self.resolve_database(database) {
                Ok(found) => Some(self.catalog.database_name(found).to_vec()),
                Err(_) => {
                    let written = [self.ast.text(qualifier), b".", self.ast.text(table)].concat();
                    return Err(no_such_table(&written, Span::default()));
                }
            },
            None => None,
        };
        let folded = self.ast.folded(table).to_vec();
        let Some(target) = self
            .catalog
            .find_table(written.as_deref(), &folded)
            .cloned()
        else {
            return Err(no_such_table(self.ast.text(table), Span::default()));
        };
        let index = target.database;
        let database_name = self.catalog.database_name(index).to_vec();
        if target.kind != crate::catalog_view::TableKind::Table {
            return Err(refused(
                not_a_table_message(action, &target),
                Span::default(),
            ));
        }
        if target.folded.starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "table {} may not be altered",
                    String::from_utf8_lossy(&target.name)
                ),
                Span::default(),
            ));
        }
        self.record_write_dependency(index);
        let kind = match action {
            ast::AlterAction::RenameTo(name) => self.bind_rename_to(*name, &database_name)?,
            ast::AlterAction::RenameColumn { from, to } => {
                let from_folded = self.ast.folded(*from).to_vec();
                let Some(position) = target.column_position(&from_folded) else {
                    return Err(no_such_quoted_column(self.ast.text(*from)));
                };
                // A new name that another column already has is refused after
                // the rewrite, as `error in table t after rename: duplicate
                // column name: b`, because that is where SQLite finds it.
                let stored = target
                    .column(position)
                    .map(|column| column.name.clone())
                    .unwrap_or_default();
                let to_quoted = self
                    .ast
                    .name(*to)
                    .is_some_and(|name| name.quote != crate::lexer::QuoteForm::Bare);
                AlterKind::RenameColumn {
                    from: stored,
                    to: self.ast.text(*to).to_vec(),
                    to_quoted,
                }
            }
            ast::AlterAction::AddColumn(definition) => {
                let risk = self.check_added_column(&target, definition)?;
                match definition.deferred_failure {
                    Some(reason) => AlterKind::AddColumnFailsAfter {
                        message: format!(
                            "error in table {} after add column: {reason}",
                            String::from_utf8_lossy(&target.name)
                        ),
                    },
                    None => AlterKind::AddColumn {
                        start: definition.span.start,
                        end: definition.span.end,
                        risk,
                    },
                }
            }
            ast::AlterAction::DropColumn(name) => {
                let folded = self.ast.folded(*name).to_vec();
                let Some(position) = target.column_position(&folded) else {
                    return Err(no_such_quoted_column(self.ast.text(*name)));
                };
                self.check_dropped_column(&target, position, self.ast.text(*name))?;
                let stored = target
                    .column(position)
                    .map(|column| column.name.clone())
                    .unwrap_or_default();
                AlterKind::DropColumn {
                    name: stored,
                    position,
                }
            }
            other => self.bind_constraint_alter(&target, other)?,
        };
        Ok(Directive::Alter {
            database: index,
            table: target.name.clone(),
            action: kind,
        })
    }

    /// Binds `RENAME TO`, refusing the names SQLite refuses.
    ///
    /// A name that starts with `sqlite_` is reserved, and that check comes
    /// before the one for a name already in use by a table or an index.
    ///
    /// @param name - the new name
    /// @param database_name - the schema the table is in
    fn bind_rename_to(
        &self,
        name: ast::NameId,
        database_name: &[u8],
    ) -> Result<AlterKind, ParseError> {
        let to = self.ast.text(name).to_vec();
        let to_folded = self.ast.folded(name).to_vec();
        let written = String::from_utf8_lossy(&to).into_owned();
        if to_folded.starts_with(b"sqlite_") {
            return Err(refused(
                format!("object name reserved for internal use: {written}"),
                Span::default(),
            ));
        }
        let taken = self
            .catalog
            .find_table(Some(database_name), &to_folded)
            .is_some()
            || self
                .catalog
                .find_index(Some(database_name), &to_folded)
                .is_some();
        if taken {
            return Err(refused(
                format!("there is already another table or index with this name: {written}"),
                Span::default(),
            ));
        }
        Ok(AlterKind::RenameTable { to })
    }

    /// Binds the four constraint forms of `ALTER TABLE`.
    ///
    /// @param target - the table
    /// @param action - `SET NOT NULL`, `DROP NOT NULL`, `ADD CHECK` or `DROP CONSTRAINT`
    fn bind_constraint_alter(
        &self,
        target: &crate::catalog_view::TableInfo,
        action: &ast::AlterAction,
    ) -> Result<AlterKind, ParseError> {
        Ok(match action {
            ast::AlterAction::SetNotNull { column, start, end } => {
                let (name, position) = self.constrained_column(target, *column)?;
                AlterKind::SetNotNull {
                    name,
                    position,
                    start: *start,
                    end: *end,
                }
            }
            ast::AlterAction::DropNotNull(column) => {
                let (name, position) = self.constrained_column(target, *column)?;
                AlterKind::DropNotNull { name, position }
            }
            ast::AlterAction::AddCheck {
                name,
                expr,
                start,
                end,
            } => {
                self.check_names_resolve(target, *expr)?;
                let span = self.ast.expr_span(*expr);
                AlterKind::AddCheck {
                    name: name.map(|id| self.ast.text(id).to_vec()),
                    start: *start,
                    end: *end,
                    expr_start: span.start,
                    expr_end: span.end,
                }
            }
            ast::AlterAction::DropConstraint(name) => AlterKind::DropConstraint {
                name: self.ast.text(*name).to_vec(),
            },
            _ => return Err(unsupported("that ALTER TABLE form", Span::default())),
        })
    }
    /// Checks what `ADD COLUMN` may not add.
    ///
    /// Every one of these is refused because the existing rows have no value
    /// for the new column and cannot be given one: a `PRIMARY KEY` or `UNIQUE`
    /// column would need an index built over values that are all the same
    /// default, and a `NOT NULL` column with no default would make every
    /// existing row violate its own table.
    fn check_added_column(
        &self,
        table: &crate::catalog_view::TableInfo,
        definition: &ast::ColumnDef,
    ) -> Result<AddedColumnRisk, ParseError> {
        let folded = self.ast.folded(definition.name).to_vec();
        if table.column_position(&folded).is_some() {
            return Err(refused(
                format!(
                    "duplicate column name: {}",
                    String::from_utf8_lossy(self.ast.text(definition.name))
                ),
                Span::default(),
            ));
        }
        // SQLite meets the collation and the default while it reads the column
        // definition, so these come before every rule below.
        self.check_column_constraints(definition)?;
        self.check_generated_clauses(definition)?;
        let mut not_null = false;
        let mut has_default = false;
        let mut constant = true;
        let mut generated = false;
        let mut generated_stored = false;
        let mut references = false;
        for (_, constraint) in &definition.constraints {
            match constraint {
                ast::ColumnConstraint::PrimaryKey { .. } => {
                    return Err(schema_refused(
                        "Cannot add a PRIMARY KEY column",
                        Span::default(),
                    ))
                }
                ast::ColumnConstraint::Unique(_) => {
                    return Err(schema_refused(
                        "Cannot add a UNIQUE column",
                        Span::default(),
                    ))
                }
                ast::ColumnConstraint::NotNull(_) => not_null = true,
                ast::ColumnConstraint::Default(expr) => {
                    // A literal `DEFAULT NULL` is no default at all to SQLite:
                    // `NOT NULL DEFAULT NULL` is refused like `NOT NULL`, and a
                    // `REFERENCES` column with it is accepted.
                    has_default = !self.is_null_literal(*expr);
                    if !self.constant_default(*expr) {
                        constant = false;
                    }
                }
                ast::ColumnConstraint::References(_) => references = true,
                ast::ColumnConstraint::Generated { stored, .. } => {
                    generated = true;
                    generated_stored = *stored;
                }
                _ => {}
            }
        }
        // None of the three default rules applies to a generated column, which
        // has no default.
        Ok(AddedColumnRisk {
            references_with_default: !generated && references && has_default,
            null_without_default: !generated && not_null && !has_default,
            non_constant_default: !generated && !constant && has_default,
            generated_stored,
        })
    }

    /// Returns whether a `DEFAULT` is a constant an existing row can be given.
    ///
    /// SQLite can evaluate a literal, with a sign, while it compiles the
    /// statement. It cannot evaluate `CURRENT_TIMESTAMP` and its two relatives,
    /// a function, or an expression such as `1 + 1`, and refuses those.
    fn constant_default(&self, expr: ast::ExprId) -> bool {
        match self.ast.expr(expr) {
            Some(ast::Expr::Literal(
                ast::Literal::CurrentDate
                | ast::Literal::CurrentTime
                | ast::Literal::CurrentTimestamp,
            )) => false,
            Some(ast::Expr::Literal(_)) => true,
            // A bare word is a string in a default: `DEFAULT hello`, `DEFAULT "q"`.
            Some(ast::Expr::Column {
                database: None,
                table: None,
                ..
            }) => true,
            Some(ast::Expr::Unary {
                op: ast::UnaryOp::Negate | ast::UnaryOp::Identity,
                operand,
            })
            | Some(ast::Expr::Cast { operand, .. }) => self.constant_default(*operand),
            _ => false,
        }
    }

    /// Resolves the column an `ALTER COLUMN` names.
    ///
    /// The message has no quotes around the name, unlike `DROP COLUMN`'s.
    ///
    /// @param table - the table
    /// @param column - the column as written
    fn constrained_column(
        &self,
        table: &crate::catalog_view::TableInfo,
        column: ast::NameId,
    ) -> Result<(Vec<u8>, u16), ParseError> {
        let folded = self.ast.folded(column).to_vec();
        let Some(position) = table.column_position(&folded) else {
            return Err(crate::bind::no_such_column(
                self.ast.text(column),
                Span::default(),
            ));
        };
        let stored = table
            .column(position)
            .map(|found| found.name.clone())
            .unwrap_or_default();
        Ok((stored, position))
    }

    /// Refuses a `CHECK` added by `ALTER TABLE` that names a column the table
    /// does not have.
    ///
    /// @param table - the table
    /// @param expr - the predicate
    fn check_names_resolve(
        &self,
        table: &crate::catalog_view::TableInfo,
        expr: ast::ExprId,
    ) -> Result<(), ParseError> {
        let mut pending = vec![expr];
        while let Some(id) = pending.pop() {
            pending.extend(expression_children(self.ast, id));
            let Some(ast::Expr::Column {
                table: qualifier,
                column,
                ..
            }) = self.ast.expr(id)
            else {
                continue;
            };
            let folded = self.ast.folded(*column);
            let own = qualifier.is_none_or(|name| self.ast.folded(name) == table.folded.as_slice());
            let rowid = matches!(folded, b"rowid" | b"oid" | b"_rowid_");
            if own && (rowid || table.column_position(folded).is_some()) {
                continue;
            }
            let written = match qualifier {
                Some(name) => [self.ast.text(*name), b".", self.ast.text(*column)].concat(),
                None => self.ast.text(*column).to_vec(),
            };
            return Err(crate::bind::no_such_column(
                &written,
                self.ast.expr_span(id),
            ));
        }
        Ok(())
    }

    /// Returns whether an expression is the literal `NULL`.
    fn is_null_literal(&self, expr: ast::ExprId) -> bool {
        matches!(
            self.ast.expr(expr),
            Some(ast::Expr::Literal(ast::Literal::Null))
        )
    }

    /// Checks what `DROP COLUMN` may not drop.
    ///
    /// SQLite refuses three things before it changes anything: a column that is
    /// part of the primary key, a column declared `UNIQUE` in its own
    /// definition, and the only column of a table. Everything else that would
    /// break (an index, a `CHECK`, a generated column, a view, a trigger) is
    /// found after the change is made, by `ALTER TABLE` re-reading the schema.
    ///
    /// **Only a column level `UNIQUE` is refused here.** A column named by a
    /// table level `UNIQUE (a, b)`, or by `CREATE UNIQUE INDEX`, is dropped as
    /// far as this check goes, and the index or the table is what fails.
    ///
    /// @param table - the table
    /// @param position - the declared position of the column
    /// @param written - the column's name as the statement wrote it, which is
    ///   the spelling SQLite puts in its message
    fn check_dropped_column(
        &self,
        table: &crate::catalog_view::TableInfo,
        position: u16,
        written: &[u8],
    ) -> Result<(), ParseError> {
        let named = String::from_utf8_lossy(written).into_owned();
        let in_primary_key = table.rowid_alias == Some(position)
            || table
                .column(position)
                .is_some_and(|column| column.primary_key_position.is_some());
        if in_primary_key {
            return Err(refused(
                format!("cannot drop PRIMARY KEY column: \"{named}\""),
                Span::default(),
            ));
        }
        if declared_unique(&table.create_sql, usize::from(position)) {
            return Err(refused(
                format!("cannot drop UNIQUE column: \"{named}\""),
                Span::default(),
            ));
        }
        if table.columns.len() <= 1 {
            return Err(refused(
                format!("cannot drop column \"{named}\": no other columns exist"),
                Span::default(),
            ));
        }
        Ok(())
    }

    /// Binds a `REINDEX`.
    ///
    /// The name is a collation, a table or an index, and SQLite works out which
    /// from what it finds - so the resolution order is the same here. A bare
    /// `REINDEX` rebuilds everything, which is the form that matters: it is what
    /// a person runs after a collation's definition has changed underneath an
    /// index that was built with the old one.
    fn bind_reindex(
        &mut self,
        database: Option<ast::NameId>,
        name: Option<ast::NameId>,
    ) -> Result<Directive, ParseError> {
        let index = self.resolve_database(database)?;
        self.record_write_dependency(index);
        // **An unqualified name means every database**, which is SQLite's
        // `sqlite3Reindex`: a bare `REINDEX` and a collation rebuild the
        // indexes of every database, and a table or index name is looked for
        // in all of them. Reading only `main` made `REINDEX ix` on a temporary
        // table's index "unable to identify the object to be reindexed".
        let schema_name = database.map(|_| self.catalog.database_name(index).to_vec());
        let qualified = database.is_some();
        let every_index = |catalog: &dyn CatalogView| -> Vec<Vec<u8>> {
            let tables = if qualified {
                catalog.tables_of(index)
            } else {
                catalog.every_table()
            };
            tables
                .into_iter()
                .flat_map(|table| table.indexes.iter())
                .map(|entry| entry.name.clone())
                .filter(|name| !name.is_empty())
                .collect()
        };
        let Some(name) = name else {
            return Ok(Directive::Reindex {
                database: index,
                indexes: every_index(self.catalog),
            });
        };
        let folded = self.ast.folded(name).to_vec();
        if let Some(table) = self.catalog.find_table(schema_name.as_deref(), &folded) {
            return Ok(Directive::Reindex {
                database: index,
                indexes: table
                    .indexes
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect(),
            });
        }
        if let Some((_, entry)) = self.catalog.find_index(schema_name.as_deref(), &folded) {
            return Ok(Directive::Reindex {
                database: index,
                indexes: vec![entry.name.clone()],
            });
        }
        // A collation name rebuilds every index ordered by it. An unknown name
        // is an error, and SQLite reports it against the collation because that
        // is the last thing it tried.
        if Collation::from_name(core::str::from_utf8(&folded).unwrap_or("")).is_some() {
            let wanted = folded.clone();
            let tables = if qualified {
                self.catalog.tables_of(index)
            } else {
                self.catalog.every_table()
            };
            let indexes = tables
                .into_iter()
                .flat_map(|table| table.indexes.iter())
                .filter(|entry| {
                    entry
                        .columns
                        .iter()
                        .any(|key| key.collation.eq_ignore_ascii_case(&wanted))
                })
                .map(|entry| entry.name.clone())
                .collect();
            return Ok(Directive::Reindex {
                database: index,
                indexes,
            });
        }
        Err(no_such_collation_sequence(
            self.ast.text(name),
            Span::default(),
        ))
    }

    /// Binds a `VACUUM`.
    fn bind_vacuum(
        &mut self,
        database: Option<ast::NameId>,
        into: Option<ast::ExprId>,
    ) -> Result<Directive, ParseError> {
        let literal = into.and_then(|expr| match self.ast.expr(expr) {
            Some(ast::Expr::Literal(ast::Literal::String(text))) => Some(text.clone()),
            _ => None,
        });
        // Anything but a string literal is an expression SQLite evaluates when
        // the statement runs, so its text travels with the directive.
        let into_sql = match (into, &literal) {
            (Some(expr), None) => {
                let span = self.ast.expr_span(expr);
                let written = self
                    .source
                    .get(span.start as usize..span.end as usize)
                    .ok_or_else(|| refused("the file name could not be read", span))?;
                Some(String::from_utf8_lossy(written).into_owned())
            }
            _ => None,
        };
        let index = self.resolve_database(database)?;
        self.record_write_dependency(index);
        Ok(Directive::Vacuum {
            database: index,
            into: literal,
            into_sql,
        })
    }

    /// Binds an `ATTACH`.
    ///
    /// Every operand is a literal, the `KEY` included. SQLite evaluates them, and every other
    /// value they could produce is a file name computed at run time - a
    /// statement that decides which database to open from arithmetic is not a
    /// shape worth supporting before it is asked for, and it is one an
    /// authorizer could not check.
    pub(crate) fn bind_attach(
        &mut self,
        file: ast::ExprId,
        schema: ast::ExprId,
        key: Option<ast::ExprId>,
    ) -> Result<Directive, ParseError> {
        // SQLCipher's documentation writes a raw key as `KEY "x'...'"`, which
        // the grammar reads as a double quoted name, so a name is read as its
        // text the way `literal_or_name` reads a schema name.
        let key = match key.map(|expr| self.ast.expr(expr)) {
            None => None,
            Some(Some(ast::Expr::Literal(ast::Literal::String(text)))) => Some(text.clone()),
            Some(Some(ast::Expr::Column {
                table: None,
                column,
                ..
            })) => Some(self.ast.text(*column).to_vec()),
            Some(_) => {
                return Err(unsupported(
                    "an ATTACH KEY that is not a string literal",
                    Span::default(),
                ))
            }
        };
        Ok(Directive::Attach {
            file: self.literal_path(file, "ATTACH with a file name that is not a literal")?,
            schema: self.literal_or_name(schema)?,
            key,
        })
    }

    /// Binds a `DETACH`.
    pub(crate) fn bind_detach(&mut self, schema: ast::ExprId) -> Result<Directive, ParseError> {
        Ok(Directive::Detach {
            schema: self.literal_or_name(schema)?,
        })
    }

    /// Reads a name written either as a word or as a string.
    ///
    /// `ATTACH 'file.db' AS aux` and `ATTACH 'file.db' AS 'aux'` name the same
    /// schema. The grammar parses that position as an expression, so a bare
    /// word arrives as a reference to a column that does not exist - and what
    /// the statement meant is the word.
    fn literal_or_name(&mut self, expr: ast::ExprId) -> Result<Vec<u8>, ParseError> {
        match self.ast.expr(expr) {
            Some(ast::Expr::Literal(ast::Literal::String(text))) => Ok(text.clone()),
            Some(ast::Expr::Column {
                table: None,
                column,
                ..
            }) => Ok(self.ast.text(*column).to_vec()),
            _ => Err(unsupported(
                "a schema name that is not a word or a string",
                Span::default(),
            )),
        }
    }

    /// Reads the file name a `VACUUM INTO` or an `ATTACH` was given.
    ///
    /// A literal only. SQLite evaluates the expression, but every other value
    /// it could produce is a file name computed at run time, and a statement
    /// that decides which file to open or where to write a copy of the
    /// database from arithmetic is not a shape worth supporting before it is
    /// asked for.
    ///
    /// @param expr - the file name operand
    /// @param refused - the construct the refusal names when it is not one
    fn literal_path(
        &mut self,
        expr: ast::ExprId,
        refused: &'static str,
    ) -> Result<Vec<u8>, ParseError> {
        match self.ast.expr(expr) {
            Some(ast::Expr::Literal(ast::Literal::String(text))) => Ok(text.clone()),
            _ => Err(unsupported(refused, Span::default())),
        }
    }

    /// Binds a `CREATE VIEW`.
    ///
    /// The body is not resolved here: SQLite checks only the syntax of a view
    /// when it is created, and finds a missing table or column when the view is
    /// read.
    fn bind_create_view(
        &mut self,
        temporary: bool,
        if_not_exists: bool,
        database: Option<ast::NameId>,
        name: ast::NameId,
        _columns: &[ast::NameId],
        select: ast::SelectId,
    ) -> Result<Directive, ParseError> {
        let temp = self.temporary_database(temporary, database, false)?;
        let index = match temp {
            Some(index) => index,
            None => self.resolve_database(database)?,
        };
        let written = self.ast.text(name).to_vec();
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let folded = self.ast.folded(name).to_vec();
        let database_name = self.catalog.database_name(index).to_vec();
        let exists = self
            .catalog
            .find_table(Some(database_name.as_slice()), &folded)
            .is_some();
        if exists && !if_not_exists {
            return Err(self.already_exists(&database_name, &folded, name));
        }
        if !exists {
            self.refuse_index_namesake(&database_name, &folded, &written)?;
            // **The body is not resolved.** SQLite stores a view whose query
            // names a table or a column that does not exist, or that reads the
            // view itself, and reports it when the view is read; so does a
            // column list of the wrong width. Only a parameter is refused here.
            self.refuse_view_parameters()?;
            if temp.is_none() && !database_name.eq_ignore_ascii_case(b"temp") {
                self.refuse_view_in_another_database(&written, &database_name)?;
            }
        }
        let _ = select;
        self.record_write_dependency(index);
        Ok(Directive::CreateView {
            if_not_exists,
            database: index,
            name: written,
            name_offset: self.name_offset(name),
            exists,
        })
    }

    /// Refuses a view that is not temporary and names a table of another database.
    ///
    /// SQLite checks the names the view's query is written with: one qualified
    /// with a database other than the view's own is `view v cannot reference
    /// objects in database aux`. A temporary view may name any database.
    ///
    /// @param view - the view's name as written
    /// @param home - the database the view is created in
    fn refuse_view_in_another_database(&self, view: &[u8], home: &[u8]) -> Result<(), ParseError> {
        for index in 0..self.ast.from_term_count() {
            let Some(term) = self.ast.from_term(ast::FromTermId(index as u32)) else {
                continue;
            };
            let ast::FromSource::Table {
                database: Some(qualifier),
                ..
            } = &term.source
            else {
                continue;
            };
            if self.ast.text(*qualifier).eq_ignore_ascii_case(home) {
                continue;
            }
            return Err(refused(
                format!(
                    "view {} cannot reference objects in database {}",
                    String::from_utf8_lossy(view),
                    String::from_utf8_lossy(self.ast.text(*qualifier))
                ),
                Span::default(),
            ));
        }
        Ok(())
    }

    /// Refuses the two places `AUTOINCREMENT` may not be written.
    ///
    /// It counts the rowid the table has handed out, so it needs a rowid to
    /// count: only an `INTEGER PRIMARY KEY` column, and never on a table that
    /// has no rowid at all. Both messages are the reference's own, because an
    /// application that reads them is reading SQLite's.
    fn check_autoincrement(
        &mut self,
        columns: &[ast::ColumnDef],
        without_rowid: bool,
    ) -> Result<(), ParseError> {
        for column in columns {
            let declared = column.declared_type.clone().unwrap_or_default();
            for (_, constraint) in &column.constraints {
                let ast::ColumnConstraint::PrimaryKey {
                    autoincrement: true,
                    order,
                    ..
                } = constraint
                else {
                    continue;
                };
                if without_rowid {
                    return Err(refused(
                        "AUTOINCREMENT not allowed on WITHOUT ROWID tables",
                        Span::default(),
                    ));
                }
                // A `DESC` key is not the rowid alias, so it is not allowed
                // either.
                if !declared.eq_ignore_ascii_case(b"integer")
                    || *order == ast::SortOrder::Descending
                {
                    return Err(refused(
                        "AUTOINCREMENT is only allowed on an INTEGER PRIMARY KEY",
                        Span::default(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Refuses `AUTOINCREMENT` written inside a table level `PRIMARY KEY` unless
    /// the key is one ascending INTEGER column of a rowid table.
    ///
    /// @param columns - the table's columns
    /// @param constraints - the table's constraints
    /// @param without_rowid - whether `WITHOUT ROWID` was written
    fn check_table_autoincrement(
        &self,
        columns: &[ast::ColumnDef],
        constraints: &[(Option<ast::NameId>, ast::TableConstraint)],
        without_rowid: bool,
    ) -> Result<(), ParseError> {
        for (_, constraint) in constraints {
            let ast::TableConstraint::PrimaryKey {
                columns: keys,
                autoincrement: true,
                ..
            } = constraint
            else {
                continue;
            };
            if without_rowid {
                return Err(refused(
                    "AUTOINCREMENT not allowed on WITHOUT ROWID tables",
                    Span::default(),
                ));
            }
            // SQLite looks through a `COLLATE` and ignores the term's `DESC`:
            // `PRIMARY KEY(a DESC AUTOINCREMENT)` is still the rowid alias.
            let single = match keys.as_slice() {
                [key] => {
                    let named = match self.ast.expr(key.expr) {
                        Some(ast::Expr::Collate { operand, .. }) => self.ast.expr(*operand),
                        other => other,
                    };
                    match named {
                        Some(ast::Expr::Column {
                            table: None,
                            column,
                            ..
                        }) => Some(self.ast.folded(*column)),
                        _ => None,
                    }
                }
                _ => None,
            };
            let integer = single.is_some_and(|folded| {
                columns.iter().any(|column| {
                    self.ast.folded(column.name) == folded
                        && column
                            .declared_type
                            .as_deref()
                            .is_some_and(|declared| declared.eq_ignore_ascii_case(b"integer"))
                })
            });
            if !integer {
                return Err(refused(
                    "AUTOINCREMENT is only allowed on an INTEGER PRIMARY KEY",
                    Span::default(),
                ));
            }
        }
        Ok(())
    }

    /// Binds a `CREATE TRIGGER`.
    ///
    /// The body is bound here, against the table the trigger is attached to, so
    /// a trigger that reads a column that does not exist is refused when it is
    /// written rather than the first time somebody writes the table. SQLite
    /// makes the same promise, and the alternative is a schema that loads and
    /// then fails on an unrelated INSERT.
    fn bind_create_trigger(
        &mut self,
        parts: CreateTriggerParts<'_>,
    ) -> Result<Directive, ParseError> {
        let temp = self.temporary_database(parts.temporary, parts.database, true)?;
        // `for_each_row` records whether the words were written, not whether
        // the trigger is one: SQLite has only row triggers, an omitted clause
        // means FOR EACH ROW, and FOR EACH STATEMENT is a syntax error in the
        // parser. There is nothing to refuse here.
        let _ = parts.for_each_row;
        let index = match temp {
            Some(index) => index,
            None => self.resolve_database(parts.database)?,
        };
        let written = self.ast.text(parts.name).to_vec();
        let folded = self.ast.folded(parts.name).to_vec();
        let database_name = self.catalog.database_name(index).to_vec();
        let table_folded = self.ast.folded(parts.table).to_vec();
        // A trigger created in a named database fires for a table in that
        // database. A temporary one fires for whatever the name finds, which
        // is the whole point of `CREATE TEMP TRIGGER ... ON t`: the trigger is
        // the connection's and the table is everybody's. `ON main.t` names the
        // database: a temporary trigger may name any, and any other trigger
        // only its own, which is SQLite's `sqlite3FixSrcList`.
        let named = match parts.table_database {
            Some(id) => {
                let at = self.resolve_database(Some(id))?;
                if temp.is_none() && at != index {
                    return Err(refused(
                        format!(
                            "trigger {} cannot reference objects in database {}",
                            String::from_utf8_lossy(&written),
                            String::from_utf8_lossy(self.ast.text(id))
                        ),
                        Span::default(),
                    ));
                }
                Some(self.catalog.database_name(at).to_vec())
            }
            None => None,
        };
        let scope = match &named {
            Some(name) => Some(name.as_slice()),
            None => temp.map_or(Some(database_name.as_slice()), |_| None),
        };
        let Some(target) = self.catalog.find_table(scope, &table_folded).cloned() else {
            // SQLite names the schema the table was looked for in, except for a
            // temporary trigger, which looks in all of them.
            let missing = match scope {
                Some(schema) => [schema, b".", self.ast.text(parts.table)].concat(),
                None => self.ast.text(parts.table).to_vec(),
            };
            return Err(crate::bind::no_such_table(&missing, Span::default()));
        };
        // After the table is found, as SQLite does: `CREATE TRIGGER sqlite_x ... ON missing` is
        // a missing table and not a reserved name.
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let exists = self
            .catalog
            .find_trigger(Some(database_name.as_slice()), &folded)
            .is_some();
        if exists && !parts.if_not_exists {
            return Err(refused(
                format!(
                    "trigger {} already exists",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let instead_of = parts.time == Some(ast::TriggerTime::InsteadOf);
        match target.kind {
            TableKind::View if !instead_of => {
                return Err(refused(
                    format!(
                        "cannot create {} trigger on view: {}",
                        if parts.time == Some(ast::TriggerTime::After) {
                            "AFTER"
                        } else {
                            "BEFORE"
                        },
                        String::from_utf8_lossy(&target.name)
                    ),
                    Span::default(),
                ));
            }
            TableKind::Table if instead_of => {
                return Err(refused(
                    format!(
                        "cannot create INSTEAD OF trigger on table: {}",
                        String::from_utf8_lossy(&target.name)
                    ),
                    Span::default(),
                ));
            }
            TableKind::Virtual | TableKind::Subquery => {
                return Err(unsupported("a trigger on that object", Span::default()));
            }
            _ => {}
        }
        // `UPDATE OF a, b` is deliberately *not* checked against the table's
        // columns. The pinned build accepts `UPDATE OF nosuchcolumn` and simply
        // never fires the trigger, and refusing it here would make inillucent's
        // language smaller than the reference's - a schema SQLite wrote that
        // inillucent could not load.
        // The body is deliberately *not* bound here. SQLite stores a trigger
        // whose body names a column that does not exist and reports it on the
        // first write that fires it - measured against the pinned build, which
        // accepts both `UPDATE OF nosuchcolumn` and a body reading a column the
        // table has not got. Refusing either here would leave inillucent unable to
        // load a schema SQLite had written.
        if temp.is_none() {
            self.refuse_qualified_trigger_targets(parts.body)?;
        }
        let _ = (parts.time, parts.when, parts.body);
        self.record_write_dependency(index);
        Ok(Directive::CreateTrigger {
            database: index,
            name: written,
            name_offset: self.name_offset(parts.name),
            table: target.name.clone(),
            exists,
        })
    }

    /// Finds the table a `CREATE INDEX` is on, and the schema the index goes in.
    ///
    /// **An unqualified index goes where its table is.** SQLite looks the
    /// table up in the usual order, `temp` first, and creates the index in
    /// the schema it found the table in. Taking an unqualified index to mean
    /// `main` made `CREATE TEMP TABLE t(a); CREATE INDEX i ON t(a)` report
    /// "no such table: t". A table that is not there is reported with the
    /// schema it was looked for in, which is `main` when none was written.
    ///
    /// @param database - the schema the statement wrote, when it wrote one
    /// @param table - the table's name
    fn index_target(
        &self,
        database: Option<ast::NameId>,
        table: ast::NameId,
    ) -> Result<(usize, Vec<u8>, crate::catalog_view::TableInfo), ParseError> {
        let table_folded = self.ast.folded(table).to_vec();
        let index = match database {
            Some(_) => self.resolve_database(database)?,
            None => match self.catalog.find_table(None, &table_folded) {
                Some(found) => found.database,
                None => return Err(self.index_without_table(0, &table_folded, table)),
            },
        };
        let database_name = self.catalog.database_name(index).to_vec();
        let Some(target) = self
            .catalog
            .find_table(Some(database_name.as_slice()), &table_folded)
            .cloned()
        else {
            return Err(self.index_without_table(index, &table_folded, table));
        };
        Ok((index, database_name, target))
    }
    /// Binds a `CREATE INDEX`.
    ///
    /// @param spec - what the statement named
    fn bind_create_index(&mut self, spec: &CreateIndexSpec<'_>) -> Result<Directive, ParseError> {
        let CreateIndexSpec {
            database,
            name,
            table,
            using,
            columns,
            settings,
            ..
        } = *spec;
        refuse_nulls_order(columns)?;
        let unique = spec.unique == Uniqueness::Unique;
        let if_not_exists = spec.if_not_exists == IfNotExists::Skip;
        // **A `WHERE` is carried in the statement text, not in this
        // directive.** The engine re-parses the canonical SQL it stores -
        // `index_from_create_sql` already puts the predicate on
        // `IndexInfo::partial_sql` - so a field here would be a second copy to
        // keep in step. A predicate that names a column the table has not got
        // is refused when the index is built, by the query that fills it.
        // Only one module can back an index, and naming another is refused here
        // rather than accepted and ignored - an index that silently was not the
        // structure it asked for is the shape of wrong answer this ticket keeps
        // finding.
        let using = match using {
            None => None,
            Some(named) => {
                let folded = self.ast.folded(named).to_vec();
                // Two structures, and both are real: `inillucent_hnsw` is the
                // graph the retrieval engine builds, and `ivfflat` is the
                // inverted file pgvector's other index type is - k-means
                // centroids and a list per centroid, probed `probes` deep.
                // Anything else is refused rather than accepted and ignored:
                // an index that silently was not the structure it asked for is
                // the shape of wrong answer this ticket keeps finding.
                if folded != b"inillucent_hnsw" && folded != b"ivfflat" {
                    return Err(unsupported(
                        "an index USING a module other than inillucent_hnsw or ivfflat",
                        Span::default(),
                    ));
                }
                Some(folded)
            }
        };
        let parsed_settings = index_settings(&using, settings)?;
        let (index, database_name, target) = self.index_target(database, table)?;
        self.refuse_unindexable(&target)?;
        let written = self.ast.text(name).to_vec();
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!(
                    "object name reserved for internal use: {}",
                    String::from_utf8_lossy(&written)
                ),
                Span::default(),
            ));
        }
        let folded = self.ast.folded(name).to_vec();
        let exists = self.check_new_index_name(&database_name, &folded, &written, if_not_exists)?;
        self.check_index_declarations(&target, columns, spec.filter)?;
        let keys = self.index_key_columns(&target, columns)?;
        self.record_write_dependency(index);
        Ok(Directive::CreateIndex {
            unique,
            if_not_exists,
            database: index,
            name: written,
            name_offset: self.name_offset(name),
            table: target.name.clone(),
            table_root: target.root,
            using,
            columns: keys,
            settings: parsed_settings,
            exists,
        })
    }

    /// Describes each key of a `CREATE INDEX` for the engine.
    ///
    /// @param target - the table the index is over
    /// @param columns - the indexed columns, in key order
    fn index_key_columns(
        &self,
        target: &crate::catalog_view::TableInfo,
        columns: &[ast::IndexedColumn],
    ) -> Result<Vec<IndexKeyColumn>, ParseError> {
        let mut keys = Vec::with_capacity(columns.len());
        for column in columns {
            // `CREATE INDEX x ON t(b COLLATE NOCASE DESC)` parses the collation
            // into the *expression*, because that is where the grammar puts a
            // `COLLATE` that follows a value. It is still an index on a bare
            // column, and treating it as one is the difference between
            // supporting the everyday form and refusing it as an expression.
            let (expr, written_collation) = match self.ast.expr(column.expr) {
                Some(ast::Expr::Collate { operand, collation }) => {
                    (self.ast.expr(*operand), Some(*collation))
                }
                other => (other, column.collation),
            };
            // A key that is not a bare column is an expression, and is carried
            // as the source text the engine re-parses. Its collation is BINARY
            // unless the statement named one: there is no column to inherit
            // from.
            let named = match expr {
                Some(ast::Expr::Column {
                    table: None,
                    column: name,
                    ..
                }) => Some(*name),
                _ => None,
            };
            let Some(name) = named else {
                let collation = match written_collation {
                    Some(collation) => self.ast.folded(collation).to_vec(),
                    None => b"binary".to_vec(),
                };
                keys.push(IndexKeyColumn {
                    column: None,
                    expr_sql: Some(self.ast.expr_span(column.expr).slice(self.source).to_vec()),
                    collation,
                    descending: column.order == ast::SortOrder::Descending,
                });
                continue;
            };
            let folded = self.ast.folded(name).to_vec();
            let Some(position) = target.column_position(&folded) else {
                return Err(crate::bind::no_such_column(
                    self.ast.text(name),
                    Span::default(),
                ));
            };
            let collation = match written_collation {
                Some(collation) => self.ast.folded(collation).to_vec(),
                None => target
                    .column(position)
                    .map(|column| column.collation.clone())
                    .unwrap_or_else(|| b"binary".to_vec()),
            };
            keys.push(IndexKeyColumn {
                column: Some(position),
                expr_sql: None,
                collation,
                descending: column.order == ast::SortOrder::Descending,
            });
        }
        Ok(keys)
    }

    /// Refuses `DROP TABLE` and `DROP VIEW` on the schema table, in SQLite's words.
    ///
    /// SQLite answers `table sqlite_master may not be dropped` for either statement,
    /// with or without `IF EXISTS`, and spells the temporary database's copy
    /// `sqlite_temp_master`. All four spellings of the two names are the schema table.
    ///
    /// @param folded - the name the statement dropped, folded
    /// @param database - the database it resolved to
    fn refuse_dropping_the_schema_table(
        &self,
        folded: &[u8],
        database: &[u8],
    ) -> Result<(), ParseError> {
        let main = matches!(folded, b"sqlite_master" | b"sqlite_schema");
        let temp = matches!(folded, b"sqlite_temp_master" | b"sqlite_temp_schema");
        if !main && !temp {
            return Ok(());
        }
        let in_temp = temp || database.eq_ignore_ascii_case(b"temp");
        let said = match in_temp {
            true => "table sqlite_temp_master may not be dropped",
            false => "table sqlite_master may not be dropped",
        };
        Err(refused(said, Span::default()))
    }

    /// Works out which database a `DROP` is about, and how a missing table is named.
    ///
    /// **An unqualified name is looked for in every database, `temp` first,** which is
    /// SQLite's `sqlite3LocateTable` order. Taking it to mean `main` made `DROP TABLE s`
    /// "no such table" for a temporary `s`, and dropped `main.s` where SQLite drops the
    /// temporary `s` that shadows it.
    ///
    /// SQLite names a missing table with the schema the statement wrote, and calls an
    /// unknown schema in front of a table a missing table: `DROP TABLE nosuch.t` is `no
    /// such table: nosuch.t`.
    ///
    /// @param kind - what the statement drops
    /// @param database - the schema as written, when there is one
    /// @param written - the object's name as written
    /// @param folded - the object's name folded
    fn resolve_drop_database(
        &self,
        kind: ObjectKind,
        database: Option<ast::NameId>,
        written: &[u8],
        folded: &[u8],
    ) -> Result<(Vec<u8>, usize), ParseError> {
        let qualified = match database {
            Some(schema) => [self.ast.text(schema), b".".as_slice(), written].concat(),
            None => written.to_vec(),
        };
        let index = match database {
            Some(_) => match self.resolve_database(database) {
                Err(_) if kind == ObjectKind::Table => {
                    return Err(no_such_table(&qualified, Span::default()))
                }
                resolved => resolved?,
            },
            None => self.unqualified_home(kind, folded).unwrap_or(0),
        };
        Ok((qualified, index))
    }

    /// Binds `DROP TABLE`, which frees the table's tree and the trees of its indexes.
    ///
    /// @param if_exists - whether `IF EXISTS` was written
    /// @param index - the database the table is in
    /// @param database_name - that database's name
    /// @param folded - the table's name folded
    /// @param written - the table's name as written
    /// @param qualified - the name as a failure should print it
    fn bind_drop_table(
        &self,
        if_exists: bool,
        index: usize,
        database_name: &[u8],
        folded: &[u8],
        written: Vec<u8>,
        qualified: &[u8],
    ) -> Result<Directive, ParseError> {
        let kind = ObjectKind::Table;
        let found = self
            .catalog
            .find_table(Some(database_name), folded)
            .cloned();
        let Some(table) = found else {
            if if_exists {
                return Ok(Directive::Drop {
                    kind,
                    if_exists,
                    database: index,
                    name: written,
                    root: 0,
                    index_roots: Vec::new(),
                    exists: false,
                });
            }
            return Err(no_such_table(qualified, Span::default()));
        };
        self.refuse_dropping_own_table(&table, &written)?;
        // A WITHOUT ROWID table's primary key *is* the table's own b-tree, so its entry
        // names the same root. Freeing it twice frees a page that is already on the free
        // list, which reads back as a malformed database.
        let index_roots = table
            .indexes
            .iter()
            .map(|held| held.root)
            .filter(|root| *root != 0 && *root != table.root)
            .collect();
        Ok(Directive::Drop {
            kind,
            if_exists,
            database: index,
            name: written,
            root: table.root,
            index_roots,
            exists: true,
        })
    }

    /// Binds a `DROP TABLE` or `DROP INDEX`.
    fn bind_drop(
        &mut self,
        kind: ObjectKind,
        if_exists: bool,
        database: Option<ast::NameId>,
        name: ast::NameId,
    ) -> Result<Directive, ParseError> {
        let written = self.ast.text(name).to_vec();
        let folded = self.ast.folded(name).to_vec();
        let (qualified, index) = self.resolve_drop_database(kind, database, &written, &folded)?;
        let database_name = self.catalog.database_name(index).to_vec();
        self.record_write_dependency(index);
        if kind != ObjectKind::Trigger && kind != ObjectKind::Index {
            self.refuse_dropping_the_schema_table(&folded, database_name.as_slice())?;
        }
        if kind == ObjectKind::Trigger {
            // A trigger owns no B-tree either, so dropping one is its schema row
            // and nothing else.
            let exists = self
                .catalog
                .find_trigger(Some(database_name.as_slice()), &folded)
                .is_some();
            if !exists && !if_exists {
                return Err(refused(
                    format!("no such trigger: {}", String::from_utf8_lossy(&written)),
                    Span::default(),
                ));
            }
            return Ok(Directive::Drop {
                kind,
                if_exists,
                database: index,
                name: written,
                root: 0,
                index_roots: Vec::new(),
                exists,
            });
        }
        if kind == ObjectKind::View {
            // A view owns no B-tree, so dropping one is the schema row and
            // nothing else - and it must refuse a table, because `DROP VIEW t`
            // on a table is an error rather than a drop.
            let found = self
                .catalog
                .find_table(Some(database_name.as_slice()), &folded)
                .cloned();
            let exists = found
                .as_ref()
                .is_some_and(|table| table.kind == crate::catalog_view::TableKind::View);
            if found.is_some() && !exists {
                return Err(refused(
                    format!(
                        "use DROP TABLE to delete table {}",
                        String::from_utf8_lossy(&written)
                    ),
                    Span::default(),
                ));
            }
            if !exists && !if_exists {
                return Err(refused(
                    format!("no such view: {}", String::from_utf8_lossy(&written)),
                    Span::default(),
                ));
            }
            return Ok(Directive::Drop {
                kind,
                if_exists,
                database: index,
                name: written,
                root: 0,
                index_roots: Vec::new(),
                exists,
            });
        }
        if kind == ObjectKind::Table {
            return self.bind_drop_table(
                if_exists,
                index,
                &database_name,
                &folded,
                written,
                &qualified,
            );
        }
        self.refuse_dropping_constraint_index(&database_name, &folded)?;
        let found = self.find_index_root(index, &folded);
        let Some(root) = found else {
            if if_exists {
                return Ok(Directive::Drop {
                    kind,
                    if_exists,
                    database: index,
                    name: written,
                    root: 0,
                    index_roots: Vec::new(),
                    exists: false,
                });
            }
            return Err(refused(
                format!("no such index: {}", String::from_utf8_lossy(&written)),
                Span::default(),
            ));
        };
        // The index a `UNIQUE` or `PRIMARY KEY` constraint made is part of the table,
        // and SQLite refuses to drop it by name.
        if folded.starts_with(b"sqlite_autoindex_") {
            return Err(refused(
                "index associated with UNIQUE or PRIMARY KEY constraint cannot be dropped",
                Span::default(),
            ));
        }
        Ok(Directive::Drop {
            kind,
            if_exists,
            database: index,
            name: written,
            root,
            index_roots: Vec::new(),
            exists: true,
        })
    }

    /// Returns the database an unqualified object name resolves to.
    ///
    /// `None` when no database holds an object of that kind by that name, so
    /// the caller reports it against `main` as before.
    ///
    /// @param kind - what sort of object the statement names
    /// @param folded - the object's folded name
    fn unqualified_home(&self, kind: ObjectKind, folded: &[u8]) -> Option<usize> {
        match kind {
            ObjectKind::Trigger => self
                .catalog
                .find_trigger(None, folded)
                .map(|(table, _)| table.database),
            ObjectKind::Index => self
                .catalog
                .find_index(None, folded)
                .map(|(table, _)| table.database),
            _ => self
                .catalog
                .find_table(None, folded)
                .map(|table| table.database),
        }
    }

    /// Binds a `PRAGMA`.
    fn bind_pragma(
        &mut self,
        database: Option<ast::NameId>,
        name: ast::NameId,
        value: &ast::PragmaValue,
    ) -> Result<Directive, ParseError> {
        let argument = match value {
            ast::PragmaValue::None => None,
            ast::PragmaValue::Name(name) => {
                Some(PragmaArgument::Name(self.ast.text(*name).to_vec()))
            }
            ast::PragmaValue::Value(expr) => Some(PragmaArgument::Value(self.bind_expr(*expr)?)),
        };
        let database = match database {
            Some(id) => Some(self.resolve_database(Some(id))?),
            None => None,
        };
        Ok(Directive::Pragma {
            database,
            name: self.ast.folded(name).to_vec(),
            argument,
        })
    }

    /// Returns the temporary database's number when `TEMP` was written.
    ///
    /// A temporary table's or view's name may be qualified only by `temp`:
    /// `CREATE TEMP TABLE main.t` says two different things about where the
    /// table goes, and SQLite refuses it rather than picking one, while
    /// `CREATE TEMP TABLE temp.t` says the same thing twice and SQLite accepts
    /// it. A temporary trigger takes no qualifier at all, which is SQLite's
    /// rule in `sqlite3BeginTrigger`.
    ///
    /// @param temporary - whether `TEMP` was written
    /// @param database - the qualifier, when one was written
    /// @param trigger - whether the object is a trigger
    fn temporary_database(
        &self,
        temporary: bool,
        database: Option<ast::NameId>,
        trigger: bool,
    ) -> Result<Option<usize>, ParseError> {
        if !temporary {
            return Ok(None);
        }
        if let Some(id) = database {
            if trigger {
                return Err(refused(
                    "temporary trigger may not have qualified name",
                    Span::default(),
                ));
            }
            if self.ast.folded(id) != b"temp" {
                return Err(refused(
                    "temporary table name must be unqualified",
                    Span::default(),
                ));
            }
        }
        self.catalog
            .database_index(b"temp")
            .map(Some)
            .ok_or_else(|| refused("no temporary database", Span::default()))
    }

    /// Resolves a schema qualifier to an attached database index.
    fn resolve_database(&self, database: Option<ast::NameId>) -> Result<usize, ParseError> {
        let Some(id) = database else {
            return Ok(0);
        };
        let folded = self.ast.folded(id);
        self.catalog.database_index(folded).ok_or_else(|| {
            refused(
                format!(
                    "unknown database {}",
                    String::from_utf8_lossy(self.ast.text(id))
                ),
                // SQLite points at the schema name.
                self.ast.name(id).map_or(Span::default(), |name| name.span),
            )
        })
    }

    /// Returns the byte an identifier starts at in the statement's source.
    ///
    /// The canonical `sqlite_schema` text is the statement from its object
    /// name onward, which is how `IF NOT EXISTS` and the schema qualifier come
    /// to be missing from what SQLite stores. Slicing the source is the only
    /// way to reproduce that exactly; rendering the tree back would normalise
    /// whitespace and quoting the user chose.
    fn name_offset(&self, name: ast::NameId) -> u32 {
        self.ast.name(name).map_or(0, |name| name.span.start)
    }

    /// Returns an index's root page, searching every table of a database.
    fn find_index_root(&self, database: usize, folded: &[u8]) -> Option<u32> {
        let name = self.catalog.database_name(database).to_vec();
        self.catalog
            .find_index(Some(name.as_slice()), folded)
            .map(|(_, index)| index.root)
    }
}

/// Returns a column name as it can be written back into a `CREATE` statement.
///
/// A name a query invented - `SELECT 1` reports the column as `1` - is not an
/// identifier, so it is quoted the way SQLite quotes it: `CREATE TABLE w("1")`.
///
/// @param name - the column's name as the query reports it
fn quoted_name(name: &[u8]) -> Vec<u8> {
    // SQLite quotes a name that is a keyword as well as one that is not a plain
    // word, so the stored text of `CREATE TABLE "select" AS ...` can be read back.
    let plain = !name.is_empty()
        && !name.first().is_some_and(u8::is_ascii_digit)
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        && crate::keyword::lookup(name).is_none();
    if plain {
        return name.to_vec();
    }
    let mut out = Vec::with_capacity(name.len().saturating_add(2));
    out.push(b'"');
    for byte in name {
        if *byte == b'"' {
            out.push(b'"');
        }
        out.push(*byte);
    }
    out.push(b'"');
    out
}

/// Returns the type name a `CREATE TABLE ... AS SELECT` writes for a column.
///
/// The affinity's own name, with the leading space, exactly as SQLite writes
/// it: BLOB affinity - which is what a column with no declared type has -
/// writes nothing at all, so the copy of an untyped column is untyped.
///
/// A column that is not a bare column of a table has no declared type, and
/// takes the affinity of its expression instead: `CAST(1 AS TEXT)` is a `TEXT`
/// column, and `1 + 1` has no affinity and so no type.
///
/// @param declared - the source column's declared type, as written
/// @param expression - the affinity of the expression the column is computed by
fn affinity_type(
    declared: &[u8],
    expression: Option<inillucent_value::affinity::Affinity>,
) -> &'static [u8] {
    let affinity = match (declared.is_empty(), expression) {
        (true, Some(held)) => held,
        _ => inillucent_value::affinity::for_column(declared),
    };
    match affinity {
        inillucent_value::affinity::Affinity::Blob => b"",
        inillucent_value::affinity::Affinity::Text => b" TEXT",
        inillucent_value::affinity::Affinity::Integer => b" INT",
        inillucent_value::affinity::Affinity::Real => b" REAL",
        inillucent_value::affinity::Affinity::Numeric
        | inillucent_value::affinity::Affinity::FlexNum => b" NUM",
    }
}

/// Returns the width SQLite counts an identifier as when it decides whether to
/// write a `CREATE TABLE ... AS SELECT`'s columns one per line.
///
/// Its own `identLength`: the name plus the two quotes it might need, plus one
/// for each quote inside it that would have to be doubled. The rule that reads
/// it is "under fifty, one line", and reproducing both is what makes the stored
/// declaration byte-identical rather than merely equivalent.
///
/// @param name - the identifier
fn identifier_width(name: &[u8]) -> usize {
    name.len()
        .saturating_add(2)
        .saturating_add(name.iter().filter(|byte| **byte == b'"').count())
}

/// The storage parameters `CREATE INDEX ... WITH ( ... )` accepts.
///
/// One entry per name the vector index understands, with the store option it
/// becomes. **A name that is not here is refused rather than ignored**, which is
/// the same rule `USING` follows a few lines above and for the same reason: an
/// index that quietly was not built the way it was asked to be is a wrong answer
/// nobody can see.
const INDEX_SETTINGS: [(&str, &str); 10] = [
    // The graph's own three, spelled as pgvector spells them.
    ("m", "m"),
    ("ef_construction", "ef_construction"),
    ("ef_search", "ef_search"),
    // Whether a query walks the graph (`approximate`, the default for an
    // `inillucent_hnsw` index) or compares every vector (`exact`). The store
    // validates the value, so `mode = 'fast'` is refused by name.
    ("mode", "mode"),
    // The distance the index is built for. pgvector puts this in an operator
    // class - `USING hnsw (v vector_l2_ops)` - and names it here as well.
    ("metric", "metric"),
    ("distance", "metric"),
    // How many threads the build uses, and how far behind the table the index
    // may fall before it is rebuilt.
    ("threads", "threads"),
    ("compact", "compact"),
    // The two an `ivfflat` has: how many centroids it clusters into, and how
    // many of those lists a query reads.
    ("lists", "lists"),
    ("probes", "probes"),
];

/// Checks `WITH ( ... )` against the structure that will read it.
///
/// Returns the settings as folded `(name, value)` pairs, in the order written.
/// A plain `CREATE INDEX` may not carry any: a b-tree has no parameters, and
/// accepting them would mean accepting a setting nothing reads.
///
/// @param using - the module the index named, when it named one
/// @param settings - the raw `name = value` slices
fn index_settings(
    using: &Option<Vec<u8>>,
    settings: &[Vec<u8>],
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ParseError> {
    if settings.is_empty() {
        return Ok(Vec::new());
    }
    if using.is_none() {
        return Err(unsupported(
            "WITH ( ... ) on an index that is not USING a module",
            Span::default(),
        ));
    }
    let mut held = Vec::with_capacity(settings.len());
    for setting in settings {
        let text = String::from_utf8_lossy(setting).to_string();
        let Some((name, value)) = text.split_once('=') else {
            return Err(refused(
                format!("index setting {} is not name = value", text.trim()),
                Span::default(),
            ));
        };
        let folded = name.trim().to_ascii_lowercase();
        let Some((_, option)) = INDEX_SETTINGS
            .iter()
            .find(|(known, _)| *known == folded.as_str())
        else {
            return Err(refused(
                format!("no such index setting: {folded}"),
                Span::default(),
            ));
        };
        let value = value
            .trim()
            .trim_matches(|held| held == '\'' || held == '"');
        held.push((option.as_bytes().to_vec(), value.as_bytes().to_vec()));
    }
    Ok(held)
}
