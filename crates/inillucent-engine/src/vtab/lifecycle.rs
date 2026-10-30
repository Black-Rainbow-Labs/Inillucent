//! Telling a module what the transaction did.
//!
//! Invariant: **every module hears about a boundary, in the order the engine
//! crossed it.** A `BEGIN`, a savepoint, a release, a rollback and a commit are
//! each announced; a module that heard about a commit it had not been told to
//! prepare for would be writing against a transaction it thinks is still open.
//!
//! `schema_changed` and `committed_elsewhere` are the two hooks a module needs
//! to cache anything at all: without them a cached manifest is a manifest that
//! goes stale the first time another connection writes.

use inillucent_base::error::refusal;
use inillucent_base::DbResult;
use inillucent_catalog::paged::ObjectKind;
use inillucent_ext::vtab::Context;
use inillucent_sql::vtab::{Change, ModuleArguments, ShadowRoot};
use inillucent_value::Value;

use crate::{Outcome, WalLog};

use super::*;

impl crate::ImportedDatabase {
    /// Connects every virtual table the catalog declares.
    ///
    /// **A reopened database has to reach its modules.** `CREATE VIRTUAL TABLE`
    /// connects one and holds it in `virtual_tables`, which lives in memory; a
    /// later open starts with that map empty, so every virtual table answered
    /// "no such table" until this ran. The module trait already anticipated it -
    /// `connect(arguments, creating)` documents `creating` as "true only for
    /// the `CREATE VIRTUAL TABLE` that first makes it. A module that has to
    /// write an initial row into a shadow table does it then; every later open
    /// is a connect and writes nothing." Nobody was calling it with `false`.
    ///
    /// The arguments are read back out of the stored `CREATE VIRTUAL TABLE`
    /// through the ordinary parser rather than remembered separately, because a
    /// second remembered copy of a declaration is a second thing that can
    /// disagree with the file.
    pub(crate) fn reconnect_modules(&mut self) -> DbResult<()> {
        // Every declaration is connected again, so an error recorded by an
        // earlier pass is an error about a catalog that may have changed.
        self.session_state.unconnected.clear();
        self.connect_declared(ConnectWhich::Every)
    }

    /// Connects every declared virtual table this connection holds no connection for.
    ///
    /// **A rolled back `DROP` puts the catalog row back but not the
    /// connection.** `BEGIN; DROP TABLE f; ROLLBACK` left an FTS5 table that
    /// every statement refused with "no module is connected", and a table with
    /// a vector index lost the index from this connection: rows written after
    /// the rollback never reached it, so after a reopen the index answered
    /// without them. A table that is still connected is left alone, because
    /// its module may hold the open transaction's state.
    pub(crate) fn connect_missing(&mut self) -> DbResult<()> {
        self.connect_declared(ConnectWhich::NotConnected)
    }

    /// Drops the connection of every virtual table whose declaration in the catalog differs from the one it was connected with.
    ///
    /// **A rolled back rename leaves the connection of the new definition.**
    /// `ALTER TABLE p RENAME TO q` rewrites the `source=p` argument of a vector
    /// index's store and connects the store again with `source=q`. The rollback
    /// puts the old catalog row back and not the connection, so the index went on
    /// reading the table's new name, and rows inserted into `p` never reached it.
    /// The next [`ImportedDatabase::connect_missing`] connects what this removed.
    pub(crate) fn disconnect_changed_modules(&mut self) {
        let mut declared: std::collections::HashMap<Vec<u8>, Vec<Vec<u8>>> =
            std::collections::HashMap::new();
        for recorded in &self.schema.entries {
            if recorded.entry.kind != ObjectKind::Table || !names_virtual(&recorded.entry.sql) {
                continue;
            }
            let Ok(parsed) = inillucent_sql::parser::parse_next_statement(
                &recorded.entry.sql,
                0,
                &self.pragmas.limits().borrow(),
            ) else {
                continue;
            };
            let inillucent_sql::ast::Statement::CreateVirtualTable {
                name, arguments, ..
            } = &parsed.statement
            else {
                continue;
            };
            declared.insert(
                parsed.ast.text(*name).to_ascii_lowercase(),
                arguments.clone(),
            );
        }
        self.session_state.virtual_tables.retain(|name, connected| {
            declared
                .get(name)
                .is_none_or(|held| *held == connected.arguments.arguments)
        });
    }

    /// Connects the virtual tables that could not be connected before, and
    /// reports whether any of them now is.
    ///
    /// **A module an application registers arrives after the open.**
    /// `Database::register_module` adds it once the open has finished, and the
    /// open had already tried every table, so a table made with that module
    /// was never connected again after a reopen. Only the tables in
    /// `unconnected` are tried, because connecting a table that is already
    /// connected would replace a module that may be holding a transaction.
    pub(crate) fn connect_unconnected(&mut self) -> DbResult<bool> {
        if self.session_state.unconnected.is_empty() {
            return Ok(false);
        }
        let before = self.session_state.virtual_tables.len();
        self.connect_declared(ConnectWhich::Unconnected)?;
        Ok(self.session_state.virtual_tables.len() > before)
    }

    /// Returns every schema this connection holds, the one searched last first.
    ///
    /// SQLite looks a name up in `temp`, then `main`, then the attached databases. The connected
    /// tables are kept by name, so a later insert replaces an earlier one, and walking the
    /// schemas from the lowest priority up leaves the table a statement would find.
    pub(crate) fn schemas_in_lookup_order(&self) -> Vec<usize> {
        let mut schemas: Vec<usize> = (0..self.session_state.attached.len())
            .map(|held| crate::FIRST_ATTACHED.saturating_add(held))
            .collect();
        schemas.push(crate::MAIN);
        if self.session_state.schema_at(crate::TEMP).is_some() {
            schemas.push(crate::TEMP);
        }
        schemas
    }

    /// Connects the virtual tables the catalog declares.
    ///
    /// @param which - every table, only those in `unconnected`, or only those with no connection
    fn connect_declared(&mut self, which: ConnectWhich) -> DbResult<()> {
        let declarations: Vec<(usize, Vec<u8>)> = self
            .schemas_in_lookup_order()
            .into_iter()
            .flat_map(|at| {
                self.entries_of(at)
                    .iter()
                    .map(move |recorded| (at, recorded))
            })
            .filter(|(_, recorded)| recorded.entry.kind == ObjectKind::Table)
            // **Only a statement that can declare a virtual table is parsed**
            // (task-2191). Every table's `CREATE` was parsed at every open to
            // find the virtual ones. A `CREATE VIRTUAL TABLE` holds the word
            // `virtual` wherever comments put it, so a text without it cannot
            // be one, and a text that has it for another reason is parsed and
            // skipped below as before.
            .filter(|(_, recorded)| names_virtual(&recorded.entry.sql))
            .map(|(at, recorded)| (at, recorded.entry.sql.clone()))
            .collect();
        for (at, sql) in declarations {
            let parsed = match inillucent_sql::parser::parse_next_statement(
                &sql,
                0,
                &self.pragmas.limits().borrow(),
            ) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };
            let inillucent_sql::ast::Statement::CreateVirtualTable {
                name,
                module,
                arguments,
                ..
            } = &parsed.statement
            else {
                continue;
            };
            let name = parsed.ast.text(*name).to_vec();
            let folded = name.to_ascii_lowercase();
            let skip = match which {
                ConnectWhich::Every => false,
                ConnectWhich::Unconnected => {
                    self.session_state.unconnected.remove(&folded).is_none()
                }
                ConnectWhich::NotConnected => {
                    self.session_state.virtual_tables.contains_key(&folded)
                }
            };
            if skip {
                continue;
            }
            let module = parsed.ast.text(*module).to_vec();
            let arguments: Vec<Vec<u8>> = arguments.clone();
            let Some(found) = self.session_state.registry.module(&module) else {
                // A file naming a module this build does not have is a file
                // this build cannot answer for. It is skipped rather than
                // refused so the rest of the database still opens. A statement
                // that uses the table gets "no such module", which is what
                // SQLite answers, through `refuse_unconnected`.
                self.session_state.unconnected.insert(
                    name.to_ascii_lowercase(),
                    inillucent_base::error::statement_refusal(format!(
                        "no such module: {}",
                        String::from_utf8_lossy(&module)
                    )),
                );
                continue;
            };
            let mut connect = ModuleArguments {
                database: at,
                schema: self.schema_name_of(at),
                table: name.clone(),
                module: module.clone(),
                arguments,
                shadows: Vec::new(),
            };
            for shadow in found.shadow_tables(&connect)? {
                // Looked up in the catalog rows rather than through
                // `table_root`, which answers over the tables the *planner* can
                // see. A shadow table is a real tree either way, and the row is
                // the authority for what it is registered under.
                let owned = shadow.owner.clone().unwrap_or_else(|| name.clone());
                // An owner may name the schema its table is in, which is not always this one.
                let (owner_schema, owner_table) = inillucent_sql::vtab::split_owner(&owned);
                let owner_at = owner_schema
                    .and_then(|schema| self.schema_named(schema))
                    .unwrap_or(at);
                let shadow_name =
                    shadow_table_name(owner_table, &shadow.suffix).to_ascii_lowercase();
                let Some(recorded) = self
                    .entries_of(owner_at)
                    .iter()
                    .find(|recorded| recorded.entry.name.to_ascii_lowercase() == shadow_name)
                else {
                    // A shadow another table owns, read and not made, may be a
                    // table that does not exist: `fts5vocab(nosuch, row)` was
                    // created that way and reports it when it is read (bug hunt
                    // 10). The module is connected without the root.
                    if shadow.owner.is_some() && found.reads_tables_that_may_be_missing() {
                        continue;
                    }
                    // **Refused rather than skipped.** A module connected
                    // without one of its shadow tables is a module that will
                    // answer wrongly rather than fail - it may even create a
                    // second copy of the table it could not find - so a shadow
                    // the catalog does not name stops the open and says which
                    // one. This is how a catalog that had lost rows was found:
                    // the connect went ahead without them.
                    return Err(refusal(format!(
                        "the catalog does not name {}, which {} needs",
                        String::from_utf8_lossy(&shadow_name),
                        String::from_utf8_lossy(&name)
                    )));
                };
                connect.shadows.push(ShadowRoot {
                    suffix: shadow.suffix.clone(),
                    root: recorded.root,
                });
            }
            // **A module that refuses to connect refuses its own table, and
            // the rest of the file still opens.** The open used to stop here,
            // so a file holding one search table with an option an older build
            // does not know could not be read at all, even its plain tables.
            // SQLite opens such a file and reports the module error to the
            // statement that uses the table. The error is kept for
            // `refuse_unconnected` to report, so that statement hears the
            // module's own reason instead of "no such table".
            //
            // A missing shadow table above still refuses the whole open,
            // because that is a catalog that has lost rows.
            let table = match found.connect(&connect, false) {
                Ok(table) => table,
                Err(error) => {
                    self.session_state
                        .unconnected
                        .insert(name.to_ascii_lowercase(), error);
                    continue;
                }
            };
            self.session_state.virtual_tables.insert(
                name.to_ascii_lowercase(),
                Connected {
                    table,
                    arguments: connect,
                },
            );
        }
        Ok(())
    }
    /// Applies one change to a virtual table.
    ///
    /// @param name - the table's name
    /// @param change - what to do
    pub(crate) fn change_module(&mut self, name: &[u8], change: &Change) -> DbResult<Option<i64>> {
        let entered = super::stages::clock();
        // **The first write to any module opens the transaction on all of them
        // (task-1932, M2).** Before the flag existed there was no moment at
        // which a module could start buffering, because the engine's only
        // `begin` was at `CREATE VIRTUAL TABLE`. Told before the table is taken
        // out of the map, so the module being written hears it too.
        if !self.session_state.modules_begun.get() {
            self.session_state.modules_begun.set(true);
            self.begin_modules()?;
        }
        let key = name.to_ascii_lowercase();
        let mut connected = self
            .session_state
            .virtual_tables
            .remove(&key)
            .ok_or_else(|| refusal(format!("no such table: {}", String::from_utf8_lossy(name))))?;
        // **A view has no tree, so a change that reads the owner's rows is given them.**
        // An insert that names its rowid reads nothing of the owner. A delete, an update
        // and a command such as `rebuild` do, and they are the changes that ask here.
        let reads_owner =
            !matches!(change, Change::Insert { rowid, .. } if rowid.as_integer().is_some());
        let view = match reads_owner {
            true => self.owner_view_content(&connected),
            false => Ok(None),
        };
        let view = match view {
            Ok(view) => view,
            Err(error) => {
                self.session_state.virtual_tables.insert(key, connected);
                return Err(error);
            }
        };
        let outcome = {
            let txn = self.current_txn();
            let at = connected.arguments.database;
            let session = self.session_state.session.get();
            let wal = self
                .log_of(at)
                .ok_or_else(|| refusal("a statement names a database that is not attached"))?;
            let mut log = WalLog {
                wal,
                txn,
                schema: at,
                wrote: false,
                // **The before-images a rollback needs.** Every ordinary write
                // passes `Some(self.writing.undo())`; this path passed `None`, so a
                // virtual table's writes went into the pool with nothing
                // recorded that could put them back. `ROLLBACK` then undid
                // every ordinary table and left the module's shadow trees as
                // the abandoned transaction had made them - so the connection
                // read two rows where the file held one, and a reopen was the
                // only thing that corrected it. The file itself was never
                // wrong: no commit record was written, so recovery ignored
                // the pages. Only the live connection was.
                undo: Some(self.writing.undo()),
                uncommitted: self.uncommitted_handle_of(at),
            };
            let store = WriteStore {
                database: crate::file_of(
                    &mut self.storage.database,
                    &mut self.session_state.attached,
                    &mut self.session_state.temps,
                    session,
                    at,
                )?,
                trees: &mut self.schema.trees,
                log: &mut log,
                view: view.as_ref(),
            };
            let mut nowhere = inillucent_ext::vtab::WithStore { store };
            let mut context = Context {
                host: &mut nowhere,
                database: 0,
                limits: &self.pragmas.limits().borrow(),
                catalog: Some(&self.schema.catalog),
            };
            let called = super::stages::clock();
            let outcome = connected.table.update(&mut context, change);
            (outcome, super::stages::elapsed(called))
        };
        // The module's own time, and the whole call's, so the difference is the
        // plumbing this function builds per row - the map removal, the `WalLog`,
        // the `WriteStore`, the `Context`. Measured at 0.16 microseconds a row
        // of `extension.fts.build`'s 20.3 (task-2025).
        // Put it back whatever happened: a module that failed a write is still
        // the connected table, and dropping it would make the next statement
        // say the table does not exist.
        self.session_state.virtual_tables.insert(key, connected);
        let (outcome, inside) = outcome;
        let around = super::stages::elapsed(entered);
        super::stages::record(|stages| {
            stages.change = stages.change.saturating_add(around);
            stages.update = stages.update.saturating_add(inside);
        });
        outcome
    }
    /// Applies an `INSERT` into a virtual table by handing the row to the module.
    ///
    /// The engine evaluates the row and the module decides what to do with it,
    /// which is what makes a module a module rather than a table with a funny
    /// name. Only a `VALUES` source is taken: an `INSERT ... SELECT` into a
    /// virtual table is a pipeline feeding a module row by row, and it is
    /// refused by name rather than answered half way.
    ///
    /// @param statement - the bound insert
    /// @param params - the values bound to `?1`, `?2`, ...
    /// Applies an `UPDATE` to a virtual table, one row at a time.
    ///
    /// **A module owns its storage, so an update is a replacement.** The row is
    /// read back through an ordinary query - which is the module's own cursor,
    /// so there is no second reader of its format - the assignments are applied
    /// over it, and the whole row is handed back as one `Change::Update`. That
    /// is what `xUpdate` takes and what an append-only index does with an edit:
    /// tombstone the old version, append the new one.
    ///
    /// The hidden columns are left NULL. They are the module's query interface -
    /// `k`, `vector`, `rank` on a search table - and are not values a row holds.
    ///
    /// @param statement - the bound update
    /// @param keys - the rowid of each row the `WHERE` selected
    /// @param params - the bound parameters
    pub(crate) fn update_module(
        &mut self,
        statement: &inillucent_sql::dml::BoundUpdate,
        keys: &[Vec<inillucent_tree::datum::OwnedDatum>],
        params: &inillucent_exec::physical::Params,
    ) -> DbResult<usize> {
        let table = statement.table.clone();
        let width = table.columns.len();
        // The columns a row actually holds, by declared position, and the query
        // that reads them.
        let visible: Vec<usize> = (0..width)
            .filter(|at| {
                table
                    .column(*at as u16)
                    .is_some_and(|column| !column.hidden)
            })
            .collect();
        let projection = visible
            .iter()
            .filter_map(|at| table.column(*at as u16))
            .map(|column| {
                format!(
                    "\"{}\"",
                    String::from_utf8_lossy(&column.name).replace('"', "\"\"")
                )
            })
            .collect::<Vec<String>>()
            .join(", ");
        let quoted = String::from_utf8_lossy(&table.name).replace('"', "\"\"");
        let mut changed = 0usize;
        for key in keys {
            let Some(&inillucent_tree::datum::OwnedDatum::Int(rowid)) = key.first() else {
                continue;
            };
            let held = self.execute_any(
                &format!("SELECT {projection} FROM \"{quoted}\" WHERE rowid = {rowid}"),
                &inillucent_exec::physical::Params::new(),
            )?;
            let mut new_rowid = rowid;
            let mut unconverted: Option<Value<'static>> = None;
            let mut values = vec![Value::Null; width];
            if let Some(row) = held.rows.first() {
                for (position, at) in visible.iter().enumerate() {
                    if let (Some(slot), Some(value)) = (values.get_mut(*at), row.get(position)) {
                        *slot = Value::from(&value.borrow()).into_owned()?;
                    }
                }
            }
            for (at, assignment) in statement.assignments.iter().enumerate() {
                // **The new value comes with the key**, evaluated against the
                // row by the query that found it: the key row is the rowid and
                // then one value per assignment. `SET body = body || '!'` and
                // a subquery correlated to the row both read the row being
                // replaced, which a value folded here as a constant could not,
                // and those were refused. A key row with no values is from a
                // caller that asked only for rowids, and its assignments are
                // folded as constants as before.
                let value = match key.get(at.saturating_add(1)) {
                    Some(value) => value.clone(),
                    None => inillucent_exec::physical::literal_value_in(
                        &assignment.value,
                        params,
                        Some(&*self),
                    )
                    .map_err(|_| {
                        refusal(
                            "an UPDATE of a virtual table assigns a constant; \
                             an expression over the row being replaced is not supported",
                        )
                    })?,
                };
                // **`SET rowid = 10` moves the row.** The assignment carries
                // `column: 0` and says nothing else about which column it is, so
                // it used to be written into the first declared column while
                // the row kept its rowid. It is the new rowid of the change.
                if assignment.rowid {
                    // **A value that is not an integer reaches the module as written.** FTS5
                    // refuses it with `datatype mismatch`, and an R*Tree takes the rowid from its
                    // id column and does not read it, so refusing it here for every module was
                    // wrong for the second.
                    match module_rowid(value.clone()) {
                        Ok(rowid) => new_rowid = rowid,
                        Err(_) => {
                            unconverted = Some(Value::from(&value.borrow()).into_owned()?);
                        }
                    }
                    continue;
                }
                if let Some(slot) = values.get_mut(usize::from(assignment.column)) {
                    *slot = Value::from(&value.borrow()).into_owned()?;
                }
            }
            self.change_module(
                &table.name,
                &Change::Update {
                    old_rowid: Value::Integer(rowid),
                    new_rowid: unconverted.unwrap_or(Value::Integer(new_rowid)),
                    values,
                },
            )?;
            changed = changed.saturating_add(1);
        }
        Ok(changed)
    }
    /// Hands each row of an insert to the module that owns the table.
    ///
    /// **An `INSERT ... SELECT` is answered by running the query first and
    /// handing its rows over one at a time.** It was refused as unsupported,
    /// and it is the natural way to fill an FTS5 or `inillucent_search` table
    /// from an ordinary one: `INSERT INTO ft(body) SELECT body FROM src` is the
    /// FTS5 backfill idiom. The query's rows are all read before the first one
    /// is written, so a query that reads the table being filled sees it as it
    /// was when the statement started, which is what SQLite does.
    ///
    /// @param statement - the bound insert
    /// @param params - the values bound for this execution
    /// @param selected - the rows a `SELECT` source produced, `None` for `VALUES`
    pub(crate) fn insert_into_module(
        &mut self,
        statement: &inillucent_sql::dml::BoundInsert,
        params: &inillucent_exec::physical::Params,
        selected: Option<&[Vec<inillucent_tree::datum::OwnedDatum>]>,
    ) -> DbResult<Outcome> {
        let mut changed = 0usize;
        let mut images: Vec<Vec<inillucent_tree::datum::OwnedDatum>> = Vec::new();
        // **A statement that fails leaves none of its rows.** `OR FAIL` keeps
        // what it wrote and `OR ROLLBACK` ends the transaction, so only the
        // other algorithms put the statement back.
        let atomic = !matches!(
            statement.on_conflict,
            Some(
                inillucent_sql::ast::ConflictAction::Fail
                    | inillucent_sql::ast::ConflictAction::Rollback
            )
        );
        let started = match atomic {
            true => self.begin_module_statement(module_insert_rows(statement, selected))?,
            false => None,
        };
        let applied =
            self.insert_rows_into_module(statement, params, selected, &mut changed, &mut images);
        let abandoned = started.is_some() && applied.is_err();
        let applied = self.end_module_statement(started, applied);
        if let Err(error) = applied {
            let kept = if abandoned { 0 } else { changed };
            self.record_changes(kept as i64, kept as i64);
            // **`OR ROLLBACK` ends the transaction when the module refuses a
            // row as a constraint**, as it does for an ordinary table: SQLite
            // sets the statement's error action from the conflict clause, so
            // after the failure there is no transaction open. The rollback
            // tells the modules as well, so their buffered rows go too.
            if module_rolls_back(statement, &error) {
                self.counters.last_changes.set(0);
                self.rollback()?;
            }
            return Err(error);
        }
        // Outside a transaction the statement is its own, so the module flushes
        // and the log commits here; inside one, `commit_batch` does both.
        if self.writing.batch().is_none() {
            self.sync_modules()?;
            self.seal_module_write()?;
        }
        // **A module's insert changed rows exactly as much as an ordinary
        // one.** `changes()`/`total_changes()` read `last_changes`/
        // `changed_ever`, and nothing on this path used to touch either -
        // `Outcome::changes` was set correctly and nobody after this call ever
        // read it, since the SQL-level `changes()` and `total_changes()`
        // built-ins read the connection's own counters instead. A module has
        // no triggers of its own, so every row this loop counted is both this
        // statement's own change and the whole of what it changed.
        self.record_changes(changed as i64, changed as i64);
        let returned = self.module_returning(statement, params, &images)?;
        Ok(Outcome {
            rows: returned.clone(),
            names: std::rc::Rc::new(
                statement
                    .returning
                    .iter()
                    .map(|column| String::from_utf8_lossy(&column.name).into_owned())
                    .collect(),
            ),
            changes: inillucent_exec::dml::Changes {
                rows: changed,
                returned,
                ..Default::default()
            },
        })
    }
    /// Evaluates an insert's `RETURNING` list over the rows the module took.
    ///
    /// **A module's insert returns rows like any other.** SQLite evaluates
    /// `RETURNING` over the values it handed the module, and this path used
    /// to answer no columns and no rows. The images are the declared columns
    /// followed by the rowid, which is the shape `module_layout` describes.
    ///
    /// @param statement - the bound insert
    /// @param params - the values bound for this execution, with the
    ///   `RETURNING` subqueries already folded
    /// @param images - one row image per inserted row
    fn module_returning(
        &self,
        statement: &inillucent_sql::dml::BoundInsert,
        params: &inillucent_exec::physical::Params,
        images: &[Vec<inillucent_tree::datum::OwnedDatum>],
    ) -> DbResult<Vec<Vec<inillucent_tree::datum::OwnedDatum>>> {
        if statement.returning.is_empty() {
            return Ok(Vec::new());
        }
        let layout = inillucent_exec::dml::module_layout(&statement.table);
        let space = inillucent_exec::dml::RowSpace::new(&[statement.target_source], &layout);
        let mut compiled = Vec::with_capacity(statement.returning.len());
        for column in &statement.returning {
            compiled.push(space.compile(&column.expr, params, self)?);
        }
        let mut returned = Vec::with_capacity(images.len());
        for image in images {
            let mut row = Vec::with_capacity(compiled.len());
            for eval in &compiled {
                row.push(space.evaluate(eval.as_ref(), &[image.as_slice()])?);
            }
            returned.push(row);
        }
        Ok(returned)
    }
    /// Hands each row of an insert to the module, counting as it goes.
    ///
    /// The loop [`ImportedDatabase::insert_into_module`] and a trigger body's
    /// deferred `INSERT` share. It neither flushes the module nor records the
    /// count, because a deferred insert runs inside a statement that does both
    /// once at its end. The count is written through `changed` so a caller
    /// still has it when a row fails.
    ///
    /// @param statement - the bound insert
    /// @param params - the values bound for this execution
    /// @param selected - the rows a `SELECT` source produced, `None` for `VALUES`
    /// @param changed - how many rows were handed over, advanced per row
    /// @param images - where each inserted row's image goes, the columns and
    ///   then the rowid, when the statement has a `RETURNING` list
    pub(crate) fn insert_rows_into_module(
        &mut self,
        statement: &inillucent_sql::dml::BoundInsert,
        params: &inillucent_exec::physical::Params,
        selected: Option<&[Vec<inillucent_tree::datum::OwnedDatum>]>,
        changed: &mut usize,
        images: &mut Vec<Vec<inillucent_tree::datum::OwnedDatum>>,
    ) -> DbResult<()> {
        let count = match (&statement.source, selected) {
            (inillucent_sql::dml::BoundInsertSource::Values(values), _) => values.len(),
            (inillucent_sql::dml::BoundInsertSource::Select(_), Some(rows)) => rows.len(),
            // The compiled statement always runs its query before it gets
            // here, so this is a caller that skipped it rather than a
            // statement the engine has not built.
            (inillucent_sql::dml::BoundInsertSource::Select(_), None) => {
                return Err(refusal(
                    "an INSERT ... SELECT into a virtual table reached the module without its rows",
                ));
            }
        };
        let width = statement.table.columns.len();
        for index in 0..count {
            // **Timed per row, because 4.2 ms of `extension.fts.build`'s 8.07
            // was believed to be here and is not (task-2025).** What this arm
            // costs - the owned copy of every text value, the column map, the
            // rowid - was invisible between `execute_statement` and
            // `Fts5Table::add`, so it could be assumed. Measured, the whole arm
            // including `change_module`'s plumbing is 0.45 microseconds a
            // document against the module's own 12.03, and the 4.2 ms was the
            // transaction's commit: FTS5 flushes its doclists at `xSync`, which
            // is outside the `Fts5Table::add` that `BuildStages::whole` times.
            let row_started = super::stages::clock();
            // The statement's own column list decides where each value lands:
            // `INSERT INTO documents(title, body)` supplies two of however many
            // the module declared, and the rest are NULL.
            let supplied = self.module_row(statement, index, params, selected)?;
            let mut cells = vec![Value::Null; width];
            for (position, column) in statement.columns.iter().enumerate() {
                let inillucent_sql::dml::ColumnSource::Row(at) = column else {
                    continue;
                };
                if let (Some(slot), Some(value)) = (cells.get_mut(position), supplied.get(*at)) {
                    *slot = value.clone();
                }
            }
            let rowid = self.rowid_of_module_insert(statement, &supplied, params)?;
            // **Tried gating this on `change_module`'s `Option<i64>` return -
            // `Some` for an ordinary content row, `None` for a command, on the
            // theory that a command never counts as a change.** That broke
            // three passing cases: SQLite's own `changes()` reports 1 for a
            // *recognised* command (`'pgsz'`, `'rebuild'`,
            // `'integrity-check'`) and only 0 for one SQLite itself refuses -
            // `crates\inillucent-compat\tests\differential\fts5.rs`'s
            // `an_unknown_command_is_refused`. Both engines answer `ok:
            // false` there, so the count is right and the count is what has
            // to answer 0: this loop errors out through the `?` below before
            // `changed` moves, past the `record_changes` call after it, and
            // `changes()` then read whatever the *previous* statement had
            // left - measured at 1, from the schema's last successful
            // single-row insert, where the reference answers 0 for a
            // statement that changed nothing. Recording here, on the way out,
            // is what makes a refused command's `changes()` its own instead
            // of an earlier statement's leftover.
            let built = super::stages::elapsed(row_started);
            let change = Change::Insert {
                rowid,
                values: cells,
            };
            let applied = match self.change_module(&statement.table.name, &change) {
                // **`OR REPLACE` removes the row whose rowid is taken and
                // tries again**, which is what SQLite asks FTS5 to do for it.
                // Only a constraint failure on a rowid the statement named
                // can be answered that way.
                Err(error) if module_replaces(statement, &error) => {
                    self.replace_in_module(&statement.table.name, &change, error)
                }
                applied => applied,
            };
            let whole = super::stages::elapsed(row_started);
            super::stages::record(|stages| {
                stages.rows = stages.rows.saturating_add(1);
                stages.values = stages.values.saturating_add(built);
                stages.whole = stages.whole.saturating_add(whole);
            });
            // **The `RETURNING` image is kept before the outcome is known.**
            // SQLite evaluates `RETURNING` for a virtual table from the values
            // it hands the module, so a row `OR IGNORE` skips is still
            // returned, and a rowid the module chooses reads as -1: the
            // statement never learns it.
            if let (false, Change::Insert { rowid, values }) =
                (statement.returning.is_empty(), &change)
            {
                let mut image: Vec<inillucent_tree::datum::OwnedDatum> = values
                    .iter()
                    .map(inillucent_tree::datum::OwnedDatum::from)
                    .collect();
                image.push(match rowid {
                    Value::Null => inillucent_tree::datum::OwnedDatum::Int(-1),
                    supplied => inillucent_tree::datum::OwnedDatum::from(supplied),
                });
                images.push(image);
            }
            match applied {
                Ok(_) => {}
                // **`OR IGNORE` skips a row the module refused as a
                // constraint.** SQLite's `OP_VUpdate` turns a module's
                // constraint failure into success under `IGNORE` and counts no
                // change for it, so `INSERT OR IGNORE INTO f(rowid, ...)
                // SELECT ...` skips the rowid that is taken and inserts the
                // rest. Any other failure still fails the statement.
                Err(error) if module_ignores(statement, &error) => continue,
                Err(error) => return Err(error),
            }
            *changed = changed.saturating_add(1);
        }
        Ok(())
    }
    /// Returns the rowid one row of an insert names, or NULL when the module is to choose.
    ///
    /// **A supplied rowid is the caller's, not the module's to choose.** This passed `Null`
    /// unconditionally, so `INSERT INTO t(rowid, ...) VALUES (?1, ...)` was accepted and the
    /// rowid silently discarded - the module allocated its own, and every row came back under a
    /// number the caller had not written. It surfaced as a migrated search index whose every
    /// ranking was correct and whose every identifier was one too high, because the source
    /// numbered its chunks from zero and the module numbered them from one.
    ///
    /// @param statement - the bound insert
    /// @param supplied - the values the row supplies, in statement order
    /// @param params - the values bound for this execution
    fn rowid_of_module_insert(
        &self,
        statement: &inillucent_sql::dml::BoundInsert,
        supplied: &[Value<'static>],
        params: &inillucent_exec::physical::Params,
    ) -> DbResult<Value<'static>> {
        Ok(match (statement.named_rowid, &statement.rowid) {
            // `INSERT INTO t(rowid, ...)`, which the binder records apart from the columns
            // because a rowid is not one: nothing writes it into the record. It is the only place
            // the value appears - no `ColumnSource` refers to it - so reading `statement.rowid`
            // alone found nothing and the value was dropped on the floor.
            //
            // **A named rowid takes integer affinity, as it does for an ordinary table.** `'2'`
            // and `3.0` are rowids 2 and 3, and `'abc'`, `1.5` and a blob are refused with
            // `datatype mismatch`. They were handed to the module as they were written, and an
            // FTS5 table kept a row under a text rowid.
            (Some(at), _) => match supplied.get(at) {
                None | Some(Value::Null) => Value::Null,
                Some(named) => Value::Integer(module_rowid(
                    inillucent_tree::datum::OwnedDatum::from(named),
                )?),
            },
            (None, Some(inillucent_sql::dml::ColumnSource::Row(at))) => {
                supplied.get(*at).cloned().unwrap_or(Value::Null)
            }
            (None, Some(inillucent_sql::dml::ColumnSource::Expr(expr))) => Value::from(
                &inillucent_exec::physical::literal_value_in(expr, params, Some(self))?.borrow(),
            )
            .into_owned()?,
            // Nothing named one, so the module allocates - which is what `Null` asks it for.
            (None, Some(inillucent_sql::dml::ColumnSource::Generated(_)) | None) => Value::Null,
        })
    }

    /// Deletes the row a replacing insert collided with, then inserts again.
    ///
    /// @param table - the virtual table's name
    /// @param change - the insert the module refused
    fn replace_in_module(
        &mut self,
        table: &[u8],
        change: &Change,
        refused: inillucent_base::error::DbError,
    ) -> DbResult<Option<i64>> {
        let Change::Insert { rowid, values } = change else {
            return Err(refused);
        };
        // The rowid the statement named, or the one the table's own first column names (an
        // R*Tree's `id`). With neither there is no row to remove and the refusal stands.
        let key = match rowid {
            Value::Null => self
                .session_state
                .virtual_tables
                .get(&table.to_ascii_lowercase())
                .and_then(|connected| connected.table.rowid_in_columns(values))
                .map(Value::Integer),
            named => Some(named.clone()),
        };
        let Some(key) = key else {
            return Err(refused);
        };
        self.change_module(table, &Change::Delete(key))?;
        self.change_module(table, change)
    }
    /// Returns the values one row of an insert supplies, in statement order.
    ///
    /// **A `VALUES` row is folded with the catalog in reach.** It was folded
    /// without one, so `INSERT INTO chunk_search(rowid, text, vector) VALUES
    /// (1, 'x', embed('search_document: x'))` was refused as "a call to the
    /// registered function embed from here" while the same row went into an
    /// ordinary table. `docs/embeddings.md` says `embed(TEXT)` can go in a
    /// `VALUES` row, and for a module's table it could not.
    ///
    /// @param statement - the bound insert
    /// @param index - which row, counting from zero
    /// @param params - the values bound for this execution
    /// @param selected - the rows a `SELECT` source produced, `None` for `VALUES`
    fn module_row(
        &self,
        statement: &inillucent_sql::dml::BoundInsert,
        index: usize,
        params: &inillucent_exec::physical::Params,
        selected: Option<&[Vec<inillucent_tree::datum::OwnedDatum>]>,
    ) -> DbResult<Vec<Value<'static>>> {
        let mut supplied: Vec<Value<'static>> = Vec::new();
        match (&statement.source, selected) {
            (inillucent_sql::dml::BoundInsertSource::Values(values), _) => {
                for expr in values.get(index).map(Vec::as_slice).unwrap_or_default() {
                    let value =
                        inillucent_exec::physical::literal_value_in(expr, params, Some(self))?;
                    supplied.push(Value::from(&value.borrow()).into_owned()?);
                }
            }
            (inillucent_sql::dml::BoundInsertSource::Select(_), rows) => {
                let row = rows.and_then(|rows| rows.get(index));
                for value in row.map(Vec::as_slice).unwrap_or_default() {
                    supplied.push(Value::from(&value.borrow()).into_owned()?);
                }
            }
        }
        Ok(supplied)
    }

    /// Applies an `UPDATE` of a module's table as one statement.
    ///
    /// A statement that changes several rows and fails on a later one leaves
    /// none of them changed, which is what it does for an ordinary table. See
    /// [`ImportedDatabase::begin_module_statement`].
    ///
    /// @param statement - the bound update
    /// @param keys - the rowid of each row the `WHERE` selected
    /// @param params - the bound parameters
    pub(crate) fn update_module_statement(
        &mut self,
        statement: &inillucent_sql::dml::BoundUpdate,
        keys: &[Vec<inillucent_tree::datum::OwnedDatum>],
        params: &inillucent_exec::physical::Params,
    ) -> DbResult<usize> {
        let started = self.begin_module_statement(keys.len())?;
        let applied = self.update_module(statement, keys, params);
        self.end_module_statement(started, applied)
    }

    /// Marks the point a statement that writes several rows is put back to.
    ///
    /// **A module has no statement journal of its own, so the engine takes one
    /// for it.** A multi row `INSERT` or `UPDATE` hands the module its rows one
    /// at a time, and one that failed on the second row kept the first: the
    /// shadow tables had changed and the module's buffer had staged the row, and
    /// nothing put either back. The statement is bracketed the way a savepoint
    /// is. Every module flushes first, so what it holds in memory before the
    /// statement is under the undo log and what is left in its buffer belongs to
    /// the statement, and each is told a savepoint opened, so that
    /// [`ImportedDatabase::end_module_statement`] can tell them to roll back to
    /// it.
    ///
    /// A statement of one row is not bracketed: it either writes its row or
    /// fails before writing anything, and a flush for every statement of a bulk
    /// load made of single row inserts would undo the buffering the modules
    /// exist for.
    ///
    /// @param rows - how many rows the statement hands to the module
    pub(crate) fn begin_module_statement(
        &mut self,
        rows: usize,
    ) -> DbResult<Option<ModuleStatement>> {
        if rows < 2 || self.session_state.virtual_tables.is_empty() {
            return Ok(None);
        }
        if !self.session_state.modules_begun.get() {
            self.session_state.modules_begun.set(true);
            self.begin_modules()?;
        }
        self.sync_modules_before_savepoint()?;
        let level = i32::try_from(self.writing.marks().borrow().len()).unwrap_or(i32::MAX);
        self.savepoint_modules(level)?;
        Ok(Some(ModuleStatement {
            mark: self.statement_mark(),
            level,
            txn: self.current_txn(),
            autocommit: self.writing.batch().is_none(),
        }))
    }

    /// Ends a statement [`ImportedDatabase::begin_module_statement`] bracketed.
    ///
    /// A statement that worked releases the point. One that failed is put back:
    /// the modules discard what they staged since the point, the shadow trees
    /// are restored from the undo log, and outside a transaction the restore is
    /// committed together with the writes it undid, so a replay of the log
    /// arrives at the same pages. The error returned is the statement's own
    /// unless putting it back failed.
    ///
    /// @param started - what `begin_module_statement` answered
    /// @param outcome - what the statement's writes answered
    pub(crate) fn end_module_statement<T>(
        &mut self,
        started: Option<ModuleStatement>,
        outcome: DbResult<T>,
    ) -> DbResult<T> {
        let Some(started) = started else {
            return outcome;
        };
        let error = match outcome {
            Ok(value) => {
                self.release_modules(started.level)?;
                return Ok(value);
            }
            Err(error) => error,
        };
        let told = self.rollback_modules(Some(started.level));
        let undone = self.undo_to_floor(started.mark, false, started.txn);
        let sealed = match (started.autocommit, &undone) {
            (true, Ok(())) => {
                self.writing.undo().borrow_mut().clear();
                self.seal()
            }
            _ => Ok(()),
        };
        Err(undone
            .err()
            .or(told.err())
            .or(sealed.err())
            .unwrap_or(error))
    }

    /// Commits a module write that was its own transaction.
    ///
    /// **The before-images of the statement end here.** A virtual table's write passes through
    /// this commit and nothing else clears them, so the next `INSERT OR ROLLBACK` that failed
    /// undid every earlier autocommit write to the module's shadow tables as well, and the table
    /// read as empty.
    pub(crate) fn seal_module_write(&mut self) -> DbResult<()> {
        if self.writing.batch().is_none() {
            self.writing.undo().borrow_mut().clear();
        }
        self.seal()
    }

    /// Flushes every connected module before the engine commits.
    ///
    /// **Once per transaction, not once per row.** FTS5 holds a segment in
    /// memory and writes it out on `sync`, which is the whole reason the method
    /// exists - a sync after every insert turns a bulk load into one segment
    /// flush per document, and `extension.fts.build` measured 436 microseconds
    /// per row against SQLite's 5.5. SQLite syncs its modules at the end of the
    /// statement's transaction and so does this.
    pub(crate) fn sync_modules(&mut self) -> DbResult<()> {
        self.sync_modules_matching(false)
    }

    /// Flushes the modules that cannot put their own buffer back, before a savepoint opens.
    ///
    /// A module that can (see `VirtualTable::undoes_its_own_buffer`) is left holding its buffer.
    /// The flush used to be unconditional, and a bulk load made of multi row statements paid for
    /// the whole of a full text index once per statement.
    pub(crate) fn sync_modules_before_savepoint(&mut self) -> DbResult<()> {
        self.sync_modules_matching(true)
    }

    /// Flushes the connected modules, all of them or only those that cannot undo their buffer.
    ///
    /// @param skip_self_undoing - whether a module that undoes its own buffer is left alone
    fn sync_modules_matching(&mut self, skip_self_undoing: bool) -> DbResult<()> {
        let names: Vec<Vec<u8>> = self.session_state.virtual_tables.keys().cloned().collect();
        for name in names {
            let Some(mut connected) = self.session_state.virtual_tables.remove(&name) else {
                continue;
            };
            if skip_self_undoing && connected.table.undoes_its_own_buffer() {
                self.session_state.virtual_tables.insert(name, connected);
                continue;
            }
            let outcome = {
                let txn = self.current_txn();
                let at = connected.arguments.database;
                let session = self.session_state.session.get();
                let wal = self
                    .log_of(at)
                    .ok_or_else(|| refusal("a statement names a database that is not attached"))?;
                let mut log = WalLog {
                    wal,
                    txn,
                    schema: at,
                    wrote: false,
                    // A flush is part of the transaction that asked for it -
                    // at a commit the buffer is cleared immediately after, and
                    // at a savepoint these writes are exactly what a later
                    // `ROLLBACK TO` an earlier point has to be able to undo.
                    undo: Some(self.writing.undo()),
                    uncommitted: self.uncommitted_handle_of(at),
                };
                let store = WriteStore {
                    database: crate::file_of(
                        &mut self.storage.database,
                        &mut self.session_state.attached,
                        &mut self.session_state.temps,
                        session,
                        at,
                    )?,
                    trees: &mut self.schema.trees,
                    log: &mut log,
                    view: None,
                };
                let mut nowhere = inillucent_ext::vtab::WithStore { store };
                let mut context = Context {
                    host: &mut nowhere,
                    database: 0,
                    limits: &self.pragmas.limits().borrow(),
                    catalog: Some(&self.schema.catalog),
                };
                connected
                    .table
                    .sync(&mut context)
                    .and_then(|()| connected.table.commit(&mut context))
            };
            self.session_state.virtual_tables.insert(name, connected);
            outcome?;
        }
        Ok(())
    }
    /// Tells every connected module that the transaction was abandoned.
    ///
    /// **The half of the contract the new engine never held up.** The module
    /// trait has had `rollback` since the old engine, and
    /// `inillucent-session/src/vtab.rs` dispatches `Moment::Rollback` to it -
    /// but this engine only ever called `begin`, `sync` and `commit`. A module
    /// that buffers therefore never heard that its buffer was void.
    ///
    /// The file was always right: shadow tables are ordinary trees, so the undo
    /// log put them back. What was wrong was the *connection*, which went on
    /// reading the module's staging area - so one query answered differently
    /// before and after a reopen with nothing written in between, in whichever
    /// direction the abandoned transaction had written. Measured against the
    /// pinned SQLite 3.53.4 before the fix: an abandoned insert left an `fts5`
    /// table reading 2 where the file held 1, and an abandoned delete left it
    /// reading 0 where the file held 2.
    ///
    /// A module that fails to abandon its buffer is not allowed to stop the
    /// rollback - the transaction is going away either way, and a rollback that
    /// could itself fail would leave the connection in a state with no name. The
    /// first failure is remembered and returned once every module has been told.
    ///
    /// @param to_savepoint - the savepoint level, or nothing for the whole
    ///     transaction
    pub(crate) fn rollback_modules(&mut self, to_savepoint: Option<i32>) -> DbResult<()> {
        match to_savepoint {
            Some(level) => self.tell_modules(Moment::RollbackTo(level)),
            None => self.tell_modules(Moment::Rollback),
        }
    }
    /// Tells every connected module that a write transaction has started.
    ///
    /// **Called once per transaction that reaches a module, and before this it
    /// was called once per `CREATE VIRTUAL TABLE` (task-1932, M2).** The only
    /// `begin` in the engine was at creation, so a module that wanted to buffer
    /// a transaction's writes had no moment at which to start one - FTS5's own
    /// `begin` gates on `self.creating` and does nothing afterwards, which is
    /// what a module writes when the hook only ever fires at creation.
    ///
    /// Every connected module is told rather than only the one being written,
    /// which is the same set `sync_modules` flushes at the commit. A module
    /// that begins and is never written syncs nothing.
    pub(crate) fn begin_modules(&mut self) -> DbResult<()> {
        self.tell_modules(Moment::Begin)
    }
    /// Tells every connected module that a savepoint was opened.
    ///
    /// @param level - how many savepoints were already open
    pub(crate) fn savepoint_modules(&mut self, level: i32) -> DbResult<()> {
        self.tell_modules(Moment::Savepoint(level))
    }
    /// Tells every connected module that savepoints above a level were
    /// released.
    ///
    /// @param level - the level being released down to
    pub(crate) fn release_modules(&mut self, level: i32) -> DbResult<()> {
        self.tell_modules(Moment::Release(level))
    }
    /// Tells every connected module that the schema changed under it.
    ///
    /// Infallible, because it is called from `refresh_catalog`, which is called
    /// from paths that have already committed to what they did. A module that
    /// wanted to refuse a schema change would have had to refuse the statement
    /// that made it.
    pub(crate) fn schema_changed_modules(&mut self) {
        let names: Vec<Vec<u8>> = self.session_state.virtual_tables.keys().cloned().collect();
        for name in names {
            if let Some(connected) = self.session_state.virtual_tables.get_mut(&name) {
                connected.table.schema_changed();
            }
        }
    }
    /// Tells every connected module that another process committed.
    ///
    /// Infallible for the same reason: the reload has already happened, and a
    /// module's opinion about it cannot put the pages back.
    pub(crate) fn committed_elsewhere_modules(&mut self) {
        let names: Vec<Vec<u8>> = self.session_state.virtual_tables.keys().cloned().collect();
        for name in names {
            if let Some(connected) = self.session_state.virtual_tables.get_mut(&name) {
                connected.table.committed_elsewhere();
            }
        }
    }
    /// Tells every connected module about one moment.
    ///
    /// **One loop, because the set is always the same set.** A moment told to
    /// some modules and not others is how `begin` came to fire at creation and
    /// nowhere else: there was no loop, only a call beside the thing that
    /// happened.
    ///
    /// A module that fails is not allowed to stop the others being told - a
    /// transaction that is ending is ending either way - so the first failure
    /// is remembered and returned once every module has heard.
    ///
    /// @param moment - what happened
    fn tell_modules(&mut self, moment: Moment) -> DbResult<()> {
        let names: Vec<Vec<u8>> = self.session_state.virtual_tables.keys().cloned().collect();
        let mut first_failure: Option<inillucent_base::DbError> = None;
        for name in names {
            let Some(mut connected) = self.session_state.virtual_tables.remove(&name) else {
                continue;
            };
            let outcome = self.tell_one_module(&mut connected, moment);
            self.session_state.virtual_tables.insert(name, connected);
            if let Err(why) = outcome {
                if first_failure.is_none() {
                    first_failure = Some(why);
                }
            }
        }
        match first_failure {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }
    /// Tells one module the transaction was abandoned.
    ///
    /// Split out so `rollback_modules` can hold the table out of the map across
    /// the call - a module may read its own shadow trees while it discards, and
    /// the map is borrowed for the walk.
    ///
    /// @param connected - the module and the arguments it was connected with
    /// @param to_savepoint - the savepoint level, or nothing for the whole
    ///     transaction
    fn tell_one_module(&mut self, connected: &mut Connected, moment: Moment) -> DbResult<()> {
        let txn = self.current_txn();
        let at = connected.arguments.database;
        let session = self.session_state.session.get();
        let Some(wal) = self.log_of(at) else {
            return Ok(());
        };
        let mut log = WalLog {
            wal,
            txn,
            schema: at,
            wrote: false,
            undo: None,
            uncommitted: self.uncommitted_handle_of(at),
        };
        let store = WriteStore {
            database: crate::file_of(
                &mut self.storage.database,
                &mut self.session_state.attached,
                &mut self.session_state.temps,
                session,
                at,
            )?,
            trees: &mut self.schema.trees,
            log: &mut log,
            view: None,
        };
        let mut nowhere = inillucent_ext::vtab::WithStore { store };
        let mut context = Context {
            host: &mut nowhere,
            database: 0,
            limits: &self.pragmas.limits().borrow(),
            catalog: Some(&self.schema.catalog),
        };
        match moment {
            Moment::Begin => connected.table.begin(&mut context),
            Moment::Rollback => connected.table.rollback(&mut context),
            Moment::RollbackTo(level) => connected.table.rollback_to(&mut context, level),
            Moment::Savepoint(level) => connected.table.savepoint(&mut context, level),
            Moment::Release(level) => connected.table.release(&mut context, level),
        }
    }
}

/// The point a statement that writes several rows to a module is put back to.
pub(crate) struct ModuleStatement {
    /// Where the statement's writes begin in the transaction's records.
    mark: crate::engine::state::StatementMark,
    /// The savepoint level the modules were told.
    level: i32,
    /// The transaction the statement writes under.
    txn: u64,
    /// Whether the statement is its own transaction.
    autocommit: bool,
}

/// Returns how many rows an insert into a module hands over.
///
/// @param statement - the bound insert
/// @param selected - the rows a `SELECT` source produced, `None` for `VALUES`
fn module_insert_rows(
    statement: &inillucent_sql::dml::BoundInsert,
    selected: Option<&[Vec<inillucent_tree::datum::OwnedDatum>]>,
) -> usize {
    match (&statement.source, selected) {
        (inillucent_sql::dml::BoundInsertSource::Values(values), _) => values.len(),
        (inillucent_sql::dml::BoundInsertSource::Select(_), Some(rows)) => rows.len(),
        (inillucent_sql::dml::BoundInsertSource::Select(_), None) => 0,
    }
}

/// Returns the rowid an `UPDATE ... SET rowid = value` moves a row to.
///
/// The value takes integer affinity first, so `'5'` and `5.0` both move the row
/// to 5, and what is still not an integer is refused with `datatype mismatch`,
/// which is what SQLite answers and what an update of an ordinary table does.
///
/// @param value - the value the statement assigned to the rowid
fn module_rowid(value: inillucent_tree::datum::OwnedDatum) -> DbResult<i64> {
    match inillucent_exec::declared::to_key_affinity(value) {
        inillucent_tree::datum::OwnedDatum::Int(rowid) => Ok(rowid),
        _ => Err(inillucent_base::DbError::new(inillucent_base::ExtendedCode(
            inillucent_sql::dml::codes::MISMATCH,
        ))
        .with_message("datatype mismatch")),
    }
}

/// Reports whether an insert skips a row its module refused.
///
/// Only a constraint failure under `OR IGNORE` is skipped, which is the rule
/// SQLite applies to a module that declares constraint support, as FTS5 does.
///
/// @param statement - the bound insert
/// @param error - what the module answered for the row
fn module_ignores(
    statement: &inillucent_sql::dml::BoundInsert,
    error: &inillucent_base::error::DbError,
) -> bool {
    statement.on_conflict == Some(inillucent_sql::ast::ConflictAction::Ignore)
        && error.code() == inillucent_base::error::PrimaryCode::Constraint
}

/// Reports whether an insert's failure in a module rolls the transaction back.
///
/// @param statement - the bound insert
/// @param error - what the insert failed with
fn module_rolls_back(
    statement: &inillucent_sql::dml::BoundInsert,
    error: &inillucent_base::error::DbError,
) -> bool {
    statement.on_conflict == Some(inillucent_sql::ast::ConflictAction::Rollback)
        && error.code() == inillucent_base::error::PrimaryCode::Constraint
}

/// Reports whether an insert may replace the row a module refused it for.
///
/// Only under `OR REPLACE` and only for a constraint failure. Whether the insert names the row
/// to remove is decided by [`ImportedDatabase::replace_in_module`], which needs the table.
///
/// @param statement - the bound insert
/// @param error - what the module answered for the row
fn module_replaces(
    statement: &inillucent_sql::dml::BoundInsert,
    error: &inillucent_base::error::DbError,
) -> bool {
    statement.on_conflict == Some(inillucent_sql::ast::ConflictAction::Replace)
        && error.code() == inillucent_base::error::PrimaryCode::Constraint
}

/// Reports whether a statement's text holds the word `virtual`, in any case,
/// which every `CREATE VIRTUAL TABLE` does.
///
/// @param sql - the statement the catalog recorded
fn names_virtual(sql: &[u8]) -> bool {
    sql.windows(b"virtual".len())
        .any(|window| window.eq_ignore_ascii_case(b"virtual"))
}

/// Which declared virtual tables `connect_declared` connects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectWhich {
    /// Every one, as an open does.
    Every,
    /// Only those an earlier attempt could not connect, as a module registered after the open needs.
    Unconnected,
    /// Only those this connection holds no connection for, as a rollback that restored a `DROP` needs.
    NotConnected,
}
