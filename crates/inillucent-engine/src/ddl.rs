//! DDL on the catalog tree: creating, dropping and altering the schema.
//!
//! Invariant: **a schema change is one transaction's worth of ordinary writes.**
//! The catalog row goes into the catalog tree through `PagedTree::insert`, the
//! pages a new tree needs come off the same free map a split allocates from, and
//! every one of them is described in the log before it happens. So a
//! `CREATE INDEX` that crashes half-way is undone by the same recovery that
//! undoes an `INSERT`, and there is no second write path to keep in step.
//!
//! ## Why the stored text is sliced rather than printed
//!
//! `sqlite_schema.sql` holds the statement from its object name onward with the
//! keywords prefixed - so `create table IF NOT EXISTS "T" ( a )` is stored as
//! `CREATE TABLE "T" ( a )`, keeping the author's spacing, case and quoting and
//! dropping the `IF NOT EXISTS`. That is what SQLite stores, byte for byte, and
//! `inillucent_catalog::ddl::canonical_sql` is the function that already did it
//! for the old engine. Printing the parse back would produce text that
//! round-trips today and stops round-tripping at the first syntax the renderer
//! forgets - and the acceptance for this phase is *digest-equal to SQLite in its
//! effect on `sqlite_schema`*, which is a byte comparison.
//!
//! ## Two identifiers, and why they are not the same number
//!
//! A catalog row's `rootpage` is the page the tree is rooted at **in this
//! file**. The `trees` and `layouts` maps are keyed by something else: an
//! identifier, which for an imported table is the fixture's SQLite root page and
//! for a created one is a number counted up from `super::FIRST_CREATED_ROOT`.
//! They are different because the physical root is not known until the tree has
//! been built, and the identifier has to be chosen before it - a tree is stamped
//! with its identifier on every page it packs.
//!
//! ## What invalidates a plan
//!
//! Every statement that changes the catalog ends in [`ImportedDatabase::
//! refresh_catalog`], which rebuilds the binder's view, empties the statement
//! cache and bumps the generation. Emptying the cache is the invalidation; the
//! generation is what a test can read to prove it happened.

use inillucent_base::error::{confinement_refusal, refusal, statement_refusal};
use inillucent_base::DbResult;
use inillucent_catalog::paged::{ObjectKind, SchemaEntry};
use inillucent_exec::dml::Changes;
use inillucent_exec::physical::Params;
use inillucent_sql::bind::BoundStatement;
use inillucent_sql::directive::Directive;
use inillucent_tree::datum::OwnedDatum;
use inillucent_tree::paged::KeyEncoding;
use inillucent_value::collation::Collation;

// **The five modules this file is made of (task-1962, A1 step 1).** It was
// 2,753 lines in one `impl` block: the catalog rows a statement writes, the
// trees it builds, and the four statements. Every method moved whole and
// nothing changed shape.
mod alter;
mod alter_checks;
mod alter_rewrite;
pub(crate) use alter::Already;
mod catalog;
mod classify;
pub(crate) use classify::query_only_refusal;
use classify::{directive_writes, read_only_allows, schema_change, schema_of};
mod dropped_parents;
mod helpers;
pub(crate) use dropped_parents::DroppedParent;
mod index;
mod module_rename;
pub(crate) mod pads;
mod reindex;
mod table;
mod tree;

use super::{index_shape, ImportedDatabase, Outcome};

/// `(schema, transaction, whether a rollback has anything to undo, log,
/// no-steal handle)` - what [`ImportedDatabase::catalog_write`] hands back.
pub(crate) type CatalogWrite = (
    usize,
    u64,
    bool,
    std::rc::Rc<inillucent_wal::Wal>,
    std::sync::Arc<std::sync::atomic::AtomicU64>,
);

impl ImportedDatabase {
    /// Refuses a statement that would change a file on a connection that opened it read only.
    ///
    /// @param directive - the bound statement
    fn refuse_a_read_only_directive(&self, directive: &Directive) -> DbResult<()> {
        match (self.storage.read_only || self.pragmas.files_read_only())
            && !read_only_allows(directive)
        {
            true => Err(query_only_refusal()),
            false => Ok(()),
        }
    }

    /// Runs one statement the session carries out itself.
    ///
    /// @param sql - the statement text, which is also the source the stored
    ///   `CREATE` text is sliced out of
    pub fn execute_ddl(&mut self, sql: &str) -> DbResult<Outcome> {
        self.execute_ddl_with(sql, &Params::default())
    }

    /// Runs one statement the session carries out itself, with the values its
    /// parameters were bound to.
    ///
    /// Almost every directive reads no parameter. `ATTACH` and `VACUUM INTO`
    /// take a file name that may be one, so the values travel as far as the
    /// directive that evaluates that expression.
    ///
    /// @param sql - the statement text, which is also the source the stored
    ///   `CREATE` text is sliced out of
    /// @param params - the values the statement's parameters are bound to
    pub(crate) fn execute_ddl_with(&mut self, sql: &str, params: &Params) -> DbResult<Outcome> {
        let BoundStatement::Directive(directive) = self.bind(sql)? else {
            return Err(refusal(format!("{sql} is not a directive")));
        };
        // **`PRAGMA query_only` is enforced on the bound directive**, after the
        // binder has said whether an `IF [NOT] EXISTS` statement will do
        // anything. A no op under `query_only` is allowed, as in SQLite.
        if self.pragmas.query_only() && directive_writes(&directive) {
            return Err(query_only_refusal());
        }
        self.refuse_a_read_only_directive(&directive)?;
        // **Which file the statement is about, from the statement's own
        // words.** `CREATE TABLE aux.t` and `CREATE TEMP TABLE t` each bind to a
        // schema, and the primitives underneath - `allocate_root`, `record`,
        // `build_tree`, `seal` - have to write into it. Put back afterwards, so
        // nothing outside one statement ever observes it as anything but `main`.
        let previous = self.schema.ddl_schema;
        let at = schema_of(&directive);
        // **The temporary database is made by the first statement that needs
        // one.** The binder has already resolved `temp` to schema one, because
        // the name is always in the catalog; this is where the file behind it
        // comes into being, so a connection that never writes a temporary object
        // never makes one.
        if at == super::TEMP {
            self.ensure_temp()?;
        }
        self.schema.ddl_schema = at;
        // **The schema cookie moves once per schema change, and only here.**
        // Not in `refresh_catalog`, which also runs on open and on `ATTACH`:
        // a cookie that rose every time the catalog was re-derived would climb
        // on a database nobody had changed, which is the one thing an
        // application watching it must be able to rule out. Read back by
        // `PRAGMA schema_version`; a directive that failed does not move it.
        let changes_schema = schema_change(&directive);
        // **Statement atomicity for DDL, which only DML had (task-1932, H3).**
        // A directive is several writes - `alter_table` rewrites every catalog
        // row that names the table, rebuilds the connection's schema, then
        // rebuilds the tree - and nothing put the earlier ones back when a
        // later one failed. There was no rollback wrapper here, unlike
        // `write`'s `abandon`, and `record`/`rewrite`/`forget` recorded no
        // before-image at all outside an explicit transaction, so there was
        // nothing to put back with. `next_txn` had not moved either, because
        // `seal` was never reached, so the half-written catalog rows were
        // committed by whatever the next successful statement committed:
        // `ALTER TABLE t ADD COLUMN b INTEGER DEFAULT (no_such_function())`
        // errored and left `PRAGMA table_info(t)` listing a column the tree had
        // no slot for, on disk, across a reopen.
        //
        // The mark and the floor are exactly `write`'s, at exactly its cost -
        // one integer read off a `Vec`'s length - and the reload is what puts
        // the connection's derived schema back in step with the catalog tree
        // the undo has just restored.
        let autocommit = self.writing.batch().is_none();
        let mark = self.statement_mark();
        let txn = self.current_txn();
        let logged_from = self.log_of(at).map(|wal| wal.next_lsn());
        let outcome = self.run_directive(*directive, sql, params);
        self.schema.ddl_schema = previous;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let undone = self.undo_to_floor(mark, true, txn);
                if autocommit {
                    self.writing.undo().borrow_mut().clear();
                    self.writing.pending_frees().borrow_mut().clear();
                    self.writing.built().borrow_mut().clear();
                    if let (true, Some(from)) = (undone.is_ok(), logged_from) {
                        self.commit_failed_directive(at, from)?;
                    }
                }
                // **The undo's own failure is the one worth reporting.** A
                // "no such function" describing a database that is now in a
                // state nobody intended is worse than saying so, which is the
                // argument `abandon` already makes for DML.
                return Err(undone.err().unwrap_or(error));
            }
        };
        if autocommit {
            // **Nothing else can abandon what a committed directive wrote.**
            // `seal` has already gone through `commit_across` by here, so the
            // before-images stop being useful - and leaving them would put a
            // committed `CREATE TABLE` inside the reach of the next explicit
            // `ROLLBACK`, which undoes to floor zero.
            self.writing.undo().borrow_mut().clear();
        }
        if changes_schema {
            self.bump_schema_cookie_of(at, autocommit);
        }
        Ok(outcome)
    }

    /// Moves the schema cookie of the database a schema change was made in.
    ///
    /// **Each database has its own cookie**, as in SQLite: a table created in an attached
    /// database or in `temp` moves that database's `PRAGMA schema_version` and leaves `main`'s
    /// alone. Inside a transaction the old value is kept, so a `ROLLBACK` (or a `ROLLBACK TO`
    /// before this statement) puts it back.
    ///
    /// @param at - the database the statement changed
    /// @param autocommit - whether no transaction is open
    fn bump_schema_cookie_of(&mut self, at: usize, autocommit: bool) {
        let before = match self.file_of(Some(at)) {
            Ok(file) => file.schema_cookie(),
            Err(_) => return,
        };
        if !autocommit {
            self.writing
                .meta_changes()
                .borrow_mut()
                .push(crate::engine::state::MetaChange {
                    at: (at != super::MAIN).then_some(at),
                    field: crate::engine::state::MetaField::SchemaCookie,
                    before,
                });
        }
        if let Ok(file) = self.file_of_mut(Some(at)) {
            file.bump_schema_cookie();
        }
    }

    /// Runs a `BEGIN`.
    ///
    /// **`BEGIN IMMEDIATE` and `BEGIN EXCLUSIVE` take the write lock, and a
    /// connection under `PRAGMA query_only` may not.** SQLite fails them with
    /// `attempt to write a readonly database` and starts no transaction, so the
    /// `COMMIT` after one says `cannot commit - no transaction is active`.
    /// `BEGIN DEFERRED` takes nothing until a statement needs it, and succeeds.
    ///
    /// @param kind - which lock the `BEGIN` asks for
    fn begin_statement(&mut self, kind: inillucent_sql::directive::BeginKind) -> DbResult<Outcome> {
        let wants_the_write_lock = kind != inillucent_sql::directive::BeginKind::Deferred;
        if self.writing.batch().is_none() && self.pragmas.query_only() && wants_the_write_lock {
            return Err(inillucent_base::error::DbError::primary(
                inillucent_base::error::PrimaryCode::ReadOnly,
            )
            .with_message("attempt to write a readonly database")
            .with_detail("attempt to write a readonly database"));
        }
        self.transaction_statement("begin")
    }

    /// Runs `BEGIN`, `COMMIT` or `ROLLBACK` with no savepoint named.
    ///
    /// **The three transaction statements refuse what SQLite refuses.**
    /// `begin_batch`, `commit_batch` and `rollback` are deliberately tolerant -
    /// they are called at boundaries by code that does not know whether a
    /// transaction is open - and the *statements* are not: SQLite reports all
    /// three of these, and before the write path had a statement boundary, this
    /// engine reported none of them. It is how a caller finds out that an `OR
    /// ROLLBACK` ended the transaction underneath it: the `COMMIT` that follows
    /// has nothing left to commit and has to say so. Each refusal is
    /// `SQLITE_ERROR` (1), which is what the pinned reference answers; `refusal`
    /// would answer `SQLITE_MISUSE` (21), and the statement matrix's transaction
    /// cases found exactly that.
    ///
    /// @param which - `begin`, `commit` or `rollback`
    fn transaction_statement(&mut self, which: &str) -> DbResult<Outcome> {
        let open = self.writing.batch().is_some();
        match (which, open) {
            ("begin", true) => {
                return Err(statement_refusal(
                    "cannot start a transaction within a transaction",
                ))
            }
            ("begin", false) => self.begin_batch(),
            ("commit", false) => {
                return Err(statement_refusal(
                    "cannot commit - no transaction is active",
                ))
            }
            ("commit", true) => self.commit_batch()?,
            (_, false) => {
                return Err(statement_refusal(
                    "cannot rollback - no transaction is active",
                ))
            }
            (_, true) => self.rollback()?,
        }
        Ok(Outcome::empty())
    }

    /// Runs one bound directive against the schema `execute_ddl` selected.
    ///
    /// @param directive - the bound statement
    /// @param sql - the statement text, which the DDL cases slice the stored
    ///   `CREATE` text out of
    /// @param params - the values the statement's parameters are bound to
    fn run_directive(
        &mut self,
        directive: Directive,
        sql: &str,
        params: &Params,
    ) -> DbResult<Outcome> {
        let source = sql.as_bytes();
        match directive {
            // **`IF NOT EXISTS` on a table that exists never reads the
            // definition.** SQLite returns from `sqlite3StartTable` as soon as it
            // finds the table, and the checks this refusal stands for (`PRIMARY
            // KEY missing on table`, `missing datatype for`) run in
            // `sqlite3EndTable`, which is not reached. The refusal was raised
            // first here, so a repeated `CREATE TABLE IF NOT EXISTS` with a
            // definition SQLite would not accept failed where SQLite does
            // nothing (bug hunt 10).
            Directive::CreateTable {
                refusal: Some(_),
                if_not_exists: true,
                exists: true,
                ..
            } => Ok(Outcome::empty()),
            Directive::CreateTable {
                refusal: Some(said),
                ..
            } => Err(inillucent_base::error::statement_refusal(said)),
            Directive::CreateTable {
                if_not_exists,
                name,
                name_offset,
                exists,
                ..
            } => self.create_table(source, name_offset, &name, exists, if_not_exists),
            Directive::CreateTableAsSelect {
                if_not_exists,
                name,
                exists,
                create_sql,
                select_sql,
                database,
            } => self.create_table_as_select(
                database,
                &name,
                exists,
                if_not_exists,
                create_sql,
                &select_sql,
            ),
            // **`USING inillucent_hnsw` is sugar for a store plus a promise.**
            // The store is an ordinary `inillucent_search` virtual table over
            // the same HNSW `inillucent-core` builds for the retrieval engine,
            // so there is one implementation of an approximate vector index
            // rather than two. The promise is `follow_vector_indexes`: the
            // write path reports what it stored and removed for a table an
            // index a module owns is built over, and the engine applies both
            // to the module inside the same transaction.
            Directive::CreateIndex {
                using: Some(module),
                name,
                table,
                columns,
                settings,
                exists,
                if_not_exists,
                ..
            } => self.create_vector_index(
                &module,
                &name,
                &table,
                &columns,
                &settings,
                exists,
                if_not_exists,
            ),
            Directive::CreateIndex {
                unique,
                if_not_exists,
                name,
                name_offset,
                table,
                exists,
                ..
            } => self.create_index(
                source,
                name_offset,
                &name,
                &table,
                unique,
                exists,
                if_not_exists,
            ),
            Directive::CreateView {
                if_not_exists,
                name,
                name_offset,
                exists,
                ..
            } => self.create_bodiless(
                "CREATE VIEW",
                ObjectKind::View,
                source,
                name_offset,
                &name,
                &name,
                Already::of(exists, if_not_exists),
            ),
            // **Stored and fired.** It used to be refused,
            // and the refusal was right at the time: this engine could store a
            // trigger and list it in `sqlite_schema` but could not run one, and
            // a database whose triggers never fire is one whose invariants are
            // not being maintained by anything - which the application finds
            // out from its data rather than from an error.
            //
            // `inillucent-exec`'s firing point is what makes it honest, and it
            // is the same mechanism foreign keys are enforced by: the binder
            // turns a `REFERENCES` clause into `CREATE TRIGGER` text, so a
            // written trigger and a key take exactly one path.
            Directive::CreateTrigger {
                name,
                name_offset,
                table,
                exists,
                ..
            } => self.create_bodiless(
                "CREATE TRIGGER",
                ObjectKind::Trigger,
                source,
                name_offset,
                &name,
                &table,
                // The directive carries no `if_not_exists` because it does not
                // need one: `bind_create_trigger` has already refused a
                // duplicate that did not say so, and `exists` reaching here at
                // all therefore means the statement did.
                Already::of(exists, true),
            ),
            Directive::CreateVirtualTable {
                if_not_exists,
                name,
                name_offset,
                module,
                arguments,
                exists,
                ..
            } => self.create_virtual_table(
                source,
                name_offset,
                &name,
                &module,
                &arguments,
                exists,
                if_not_exists,
            ),
            Directive::Drop {
                kind,
                if_exists,
                name,
                exists,
                ..
            } => self.drop_object(kind, &name, exists, if_exists),
            Directive::Alter { table, action, .. } => self.alter_table(source, &table, &action),
            analyze @ Directive::Analyze { .. } => self.run_analyze(analyze),
            Directive::Reindex { indexes, .. } => self.reindex(&indexes),
            Directive::Begin(kind) => self.begin_statement(kind),
            Directive::Commit => self.transaction_statement("commit"),
            Directive::Pragma {
                ref name,
                ref argument,
                database,
                ..
            } => self.pragma(name, argument.as_ref(), database),
            Directive::Rollback { savepoint } => match savepoint {
                Some(name) => {
                    self.rollback_to(&name)?;
                    Ok(Outcome::empty())
                }
                None => self.transaction_statement("rollback"),
            },
            // A `SAVEPOINT` outside a transaction opens one, which is what
            // SQLite does: it is the only way to name a point inside a
            // statement that would otherwise be its own transaction. Recorded
            // as `implicit_transaction` so `release` knows this transaction is
            // the savepoint stack's own, and not one an explicit `BEGIN`
            // opened around it - see that field's own doc comment.
            Directive::Savepoint(name) => {
                if self.writing.batch().is_none() {
                    self.begin_batch();
                    self.writing.set_implicit_transaction(true);
                }
                self.savepoint(&name)?;
                Ok(Outcome::empty())
            }
            Directive::Release(name) => {
                self.release(&name)?;
                // **The last savepoint of a transaction the savepoint stack
                // itself opened releases like a `COMMIT`.** A `SAVEPOINT`
                // inside an explicit `BEGIN` also empties `marks` when
                // released, and that transaction stays open for the `COMMIT`
                // that follows - `implicit_transaction` is what tells the two
                // apart.
                if self.writing.marks().borrow().is_empty() && self.writing.implicit_transaction() {
                    self.commit_batch()?;
                }
                Ok(Outcome::empty())
            }
            // **A second database, opened beside the one this connection was
            // opened on.** Everything above the file - the binder's schema
            // numbering, the planner's tree handles, the write path's choice of
            // log - was already written for more than one; what was missing was
            // a second file to point them at.
            Directive::Attach {
                file,
                file_sql,
                schema,
                key,
            } => {
                let file = match file_sql {
                    Some(expression) => self.file_name_of(&expression, params)?,
                    None => file,
                };
                self.attach(&file, &schema, key.as_deref())?;
                Ok(Outcome::empty())
            }
            Directive::Detach { schema } => {
                self.detach(&schema)?;
                Ok(Outcome::empty())
            }
            // **`VACUUM` folds the log into the file; `VACUUM INTO` writes a
            // verified copy.** Both were refused by name, and `VACUUM INTO` is
            // how a backup is taken - the one form an application cannot do
            // without, because there is no other statement that produces a
            // second file.
            //
            // What `VACUUM` means here is a checkpoint. SQLite's rebuilds the
            // file to reclaim free pages and to defragment; this engine's pages
            // are reclaimed by the free map as they are released, so the part
            // that is left is making everything durable in the file - which is
            // exactly what a checkpoint does. It is not a lie about the space:
            // `PRAGMA freelist_count` says what is free either way.
            // Neither form may run inside an explicit transaction, which is
            // SQLite's rule and is not a formality here either: a checkpoint
            // folds committed frames into the file, and an open transaction's
            // are not committed. The message is SQLite's, and so is the code:
            // `SQLITE_ERROR` (1), not `refusal`'s `SQLITE_MISUSE` (21) -
            // `dml_differential.rs`'s `vacuum_matches_sqlite` grades it.
            Directive::Vacuum {
                database,
                into,
                into_sql,
            } => self.run_vacuum(database, into, into_sql, params),
        }
    }

    /// Runs a `VACUUM` or `VACUUM INTO` on the schema it names.
    ///
    /// **`VACUUM <schema>` on `temp` or an attached database** used to take the
    /// arm for `main`, which ignored `database`, so the statement rebuilt `main`
    /// and then put `main`'s catalog in place of the named schema's (bug hunt
    /// 10). `main` is rebuilt in place, any other schema is folded into its
    /// file, and `INTO` copies the schema that was named.
    ///
    /// @param database - the schema, as the binder numbers them
    /// @param into - the file name when it was a literal
    /// @param into_sql - the file name expression when it was not a literal
    /// @param params - the values the statement's parameters are bound to
    fn run_vacuum(
        &mut self,
        database: usize,
        into: Option<Vec<u8>>,
        into_sql: Option<String>,
        params: &Params,
    ) -> DbResult<Outcome> {
        if self.writing.batch().is_some() {
            return Err(statement_refusal("cannot VACUUM from within a transaction"));
        }
        let into = match (into, into_sql) {
            (None, Some(expression)) => Some(self.vacuum_target(&expression, params)?),
            (into, _) => into,
        };
        let Some(path) = into else {
            match database {
                super::MAIN => self.vacuum_in_place()?,
                other => self.vacuum_schema_in_place(other)?,
            }
            return Ok(Outcome::empty());
        };
        let path = String::from_utf8_lossy(&path).into_owned();
        if path.is_empty() || path == ":memory:" {
            self.rebuild_into_memory(database)?;
            return Ok(Outcome::empty());
        }
        // The second statement that names a file of its own, and so the second
        // one a confined process refuses by name rather than leaving to the
        // VFS's result code. See `attach`.
        let path = match inillucent_vfs::confine::process_root() {
            None => path,
            Some(root) => match root.admit(&path) {
                Ok(inside) => inside.to_string_lossy().into_owned(),
                Err(refused) => return Err(confinement_refusal(refused.message())),
            },
        };
        if std::path::Path::new(&path).exists() {
            // SQLite's own rule, and the one that makes the statement safe to
            // put in a backup script: it never overwrites.
            return Err(refusal("output file already exists"));
        }
        // **A rebuild, not a copy.** `VACUUM INTO` is documented as writing a
        // *compacted* database, and a byte copy reproduces the free pages and
        // the half-empty leaves it was asked to remove - which is what it used
        // to do here.
        self.rebuild_schema_copy_into(database, std::path::Path::new(&path))?;
        Ok(Outcome::empty())
    }
}

impl ImportedDatabase {
    /// Evaluates the file name expression of `VACUUM INTO`.
    ///
    /// SQLite accepts any expression there, such as `(SELECT n FROM p)` or
    /// `'a' || 'b'`, and refuses a value that is not text.
    ///
    /// @param expression - the expression as it was written
    /// @param params - the values the statement's parameters are bound to
    fn vacuum_target(&mut self, expression: &str, params: &Params) -> DbResult<Vec<u8>> {
        match self.file_name_value(&format!("SELECT ({expression})"), params)? {
            Some(inillucent_tree::datum::OwnedDatum::Text(bytes)) => Ok(bytes),
            _ => Err(statement_refusal("non-text filename")),
        }
    }

    /// Evaluates the file name expression of an `ATTACH`.
    ///
    /// SQLite accepts any expression and reads its value as text, so an
    /// integer names a file called `5` and a NULL names the empty file name,
    /// which is a private temporary database. `CAST(... AS TEXT)` does the
    /// same conversion here, and leaves NULL as NULL for the empty name.
    ///
    /// @param expression - the expression as it was written
    /// @param params - the values the statement's parameters are bound to
    fn file_name_of(&mut self, expression: &str, params: &Params) -> DbResult<Vec<u8>> {
        match self.file_name_value(&format!("SELECT CAST(({expression}) AS TEXT)"), params)? {
            Some(inillucent_tree::datum::OwnedDatum::Text(bytes)) => Ok(bytes),
            _ => Ok(Vec::new()),
        }
    }

    /// Runs the `SELECT` that computes a file name and returns its one value.
    ///
    /// **It runs on a copy of the bound values.** When the outer statement is streamed, the
    /// caller's row consumer rides on its `Params`, and a `SELECT` run with those `Params` hands
    /// its row to that consumer: the file name was printed as a result row and the directive saw
    /// no row, so `ATTACH` opened a private temporary database in place of the named file and
    /// `VACUUM INTO` failed. A copy made from the values alone carries no consumer.
    ///
    /// @param select - the `SELECT` that computes the file name
    /// @param params - the values the outer statement's parameters are bound to
    fn file_name_value(
        &mut self,
        select: &str,
        params: &Params,
    ) -> DbResult<Option<inillucent_tree::datum::OwnedDatum>> {
        let values = Params::from_values(params.values());
        let rows = self.execute_any(select, &values)?.rows;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next()))
    }
}

pub(crate) use helpers::{
    pragma_column, quoted, refuse_duplicates, renamed_automatic, unique_violation,
};

impl Outcome {
    /// Returns the answer a statement that produced no rows gives.
    pub fn empty() -> Outcome {
        Outcome {
            rows: Vec::new(),
            names: std::rc::Rc::new(Vec::new()),
            changes: Changes::default(),
        }
    }
}

/// Where one column of a rebuilt tree gets its values.
///
/// Three cases and not two: a column carried across, a column the declaration
/// has just gained with a `DEFAULT`, and one it has gained without.
#[derive(Clone, Debug)]
pub(crate) enum Fill {
    /// The old tree column that held the same declared column.
    From(usize),
    /// The `DEFAULT` a new column declared.
    Constant(OwnedDatum),
    /// Nothing: a new column with no default, which is NULL.
    Absent,
}
