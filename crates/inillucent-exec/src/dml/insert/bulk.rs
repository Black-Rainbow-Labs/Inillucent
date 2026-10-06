//! The rows an `INSERT ... SELECT` into an empty table builds its tree from.
//!
//! Invariant: **a row image built here is the row image the ordinary insert
//! would have written, and a statement this refuses is refused with the error
//! the ordinary insert reports for the same row.** The engine takes these rows
//! and builds the table's tree from them in one pass, instead of writing them
//! one at a time; nothing about what a row holds is decided here that
//! `insert_at` does not decide the same way, because both go through
//! [`compile_insert`], `InsertPlan::build_row` and the same declaration checks.
//!
//! ## Why the checks run in input order
//!
//! The ordinary insert writes row 1 before it looks at row 2, so the error a
//! failing statement reports is the first row, in input order, that breaks
//! anything: a `NOT NULL`, a `CHECK`, a `STRICT` type, or a rowid an earlier
//! row already took. Sorting first and checking the sorted rows would report a
//! different row's constraint for a statement that breaks two. So every check
//! runs here, in the order the rows arrived, and the rowid collision is found
//! against the rows already kept rather than after the sort.
//!
//! ## What the table must be
//!
//! [`bulk_shape`] decides from the statement alone, and [`bulk_target_ready`]
//! asks the file. Between them they admit an ordinary rowid table that holds no
//! row, with no trigger, no foreign key, no `RETURNING`, no upsert, no
//! `AUTOINCREMENT`, no index a module owns, and no conflict clause that keeps
//! rows or ends the transaction. Every one of those either writes something
//! while the rows go in or depends on rows already being in the tree, and the
//! bulk build has neither.

use std::collections::HashSet;

use inillucent_base::{DbError, DbResult, ExtendedCode};
use inillucent_sql::ast::ConflictAction;
use inillucent_sql::bind::BoundExpr;
use inillucent_sql::catalog_view::{IndexInfo, TableInfo, TableKind};
use inillucent_sql::dml::{rowid_message, BoundInsert, BoundInsertSource};
use inillucent_tree::datum::OwnedDatum;

use super::{compile_insert, declarations_are_met, CompiledInsert};
use crate::declared::IndexExprs;
use crate::dml::index::{index_entry, maintained};
use crate::dml::target::holds_no_row;
use crate::dml::*;
use crate::insert_plan::{RowSource, RowidKeys};
use crate::physical::Params;

/// The rows a bulk build writes, already checked and in key order.
#[derive(Debug)]
pub struct BulkRows {
    /// The table rows, sorted by rowid, in tree column order.
    pub rows: Vec<Row>,
    /// The entries of each index whose key or predicate is an expression, by
    /// the index's position in `TableInfo::indexes`, unsorted.
    ///
    /// An index on plain columns is not here: the engine reads its entries
    /// straight out of `rows`, which costs no copy. Only an expression key or
    /// a partial predicate needs the evaluator, and that lives in this crate.
    pub computed: Vec<(usize, Vec<Row>)>,
    /// The rowid of the last row kept, in input order, which is what
    /// `last_insert_rowid()` reports after the statement.
    pub last_rowid: Option<i64>,
}

/// Why the rows could not all be built, and how far the statement got.
#[derive(Debug)]
pub struct BulkFailure {
    /// The error the ordinary insert reports for the same statement.
    pub error: DbError,
    /// The rowid of the last row kept before the failure.
    ///
    /// The ordinary insert has written those rows by the time it fails, and
    /// SQLite's `last_insert_rowid()` is the last rowid attempted, so a failed
    /// statement still moves it to that row.
    pub last_rowid: Option<i64>,
}

/// Reports whether a statement's shape allows a bulk build, from the statement
/// alone.
///
/// See the module comment for the list and why each item is on it. The file is
/// not asked here: [`bulk_target_ready`] does that, once the caller has a
/// write target.
///
/// **A conflict clause is allowed only when it raises and undoes the
/// statement.** `ABORT`, the default, does both. `IGNORE` and `REPLACE` on a
/// column's `NOT NULL` are allowed too, because they decide a row's fate from
/// that row alone, which `declarations_are_met` already does here. `FAIL`
/// keeps the rows written before the failure and `ROLLBACK` ends the
/// transaction, and a bulk build has written no row to keep and must not end
/// anything, so both refuse. So does any clause on the rowid other than
/// `ABORT`, because `IGNORE` and `REPLACE` resolve a collision with a row that
/// is already in the tree.
///
/// @param statement - the bound insert
pub fn bulk_shape(statement: &BoundInsert) -> bool {
    bulk_shape_with(statement, false)
}

/// [`bulk_shape`], for a statement whose rows were handed in.
///
/// A `VALUES` row of parameters run over rows of literals (task-2191) writes
/// rows that are already values, as a `SELECT`'s are, so it may take the same
/// path.
///
/// @param statement - the bound insert
/// @param rows_handed_in - whether the rows were handed in rather than its own
pub fn bulk_shape_with(statement: &BoundInsert, rows_handed_in: bool) -> bool {
    let table = &statement.table;
    let plain_clause =
        |action: Option<ConflictAction>| matches!(action, None | Some(ConflictAction::Abort));
    let column_clauses_allowed = table.columns.iter().all(|column| {
        !matches!(
            column.not_null_conflict,
            Some(ConflictAction::Fail | ConflictAction::Rollback)
        ) && plain_clause(column.primary_key_conflict)
    });
    let source_is_rows = match &statement.source {
        BoundInsertSource::Select(_) => true,
        BoundInsertSource::Values(_) => rows_handed_in,
    };
    source_is_rows
        && table.kind == TableKind::Table
        && !table.without_rowid
        && !table.autoincrement
        && statement.triggers.is_empty()
        && statement.upsert.is_empty()
        && statement.returning.is_empty()
        && plain_clause(statement.on_conflict)
        && plain_clause(rowid_conflict(table))
        && column_clauses_allowed
        && table.foreign_keys.is_empty()
        && table.foreign_key_triggers.is_empty()
}

/// Reports whether a prepared single row `INSERT` can be run once over many
/// rows of parameters, as one statement, in place of once per row.
///
/// **One statement where a driver would run thousands** (task-2191). Python's
/// `executemany` of 10,000 rows ran the prepared insert 10,000 times, and each
/// run paid for a statement: the lock check, the counters, the undo mark and
/// the write view. Run once over all the rows, it pays once, and an empty table
/// takes the bulk build.
///
/// The caller runs the rows one at a time again whenever the statement fails,
/// and it fails having written nothing, so the one statement has to be
/// indistinguishable from the rows run one after another whenever it
/// succeeds. That is what each condition is for:
///
/// - the `VALUES` row is `?1`, `?2` and so on, exactly the statement's
///   parameters in order, so a row handed in is the row the values would
///   have built;
/// - no trigger, `RETURNING`, upsert or foreign key, each of which acts per
///   statement or reads the rows that came before;
/// - no conflict clause but `ABORT`, on the statement, a column or an index.
///   `IGNORE` and `REPLACE` change what one row writes, `FAIL` keeps a failed
///   statement's earlier rows, and `ROLLBACK` ends the transaction the caller
///   needs to try again in.
///
/// @param statement - the bound insert
pub fn runs_rows_at_once(statement: &BoundInsert) -> bool {
    let table = &statement.table;
    let plain_clause =
        |action: Option<ConflictAction>| matches!(action, None | Some(ConflictAction::Abort));
    let parameters_in_order = match &statement.source {
        BoundInsertSource::Values(rows) => match rows.as_slice() {
            [row] => {
                row.len() == statement.arity
                    && row.iter().enumerate().all(|(nth, value)| {
                        matches!(value, BoundExpr::Parameter(index)
                            if usize::try_from(*index).ok() == Some(nth.saturating_add(1)))
                    })
            }
            _ => false,
        },
        BoundInsertSource::Select(_) => false,
    };
    parameters_in_order
        && table.kind == TableKind::Table
        && statement.triggers.is_empty()
        && statement.replace_triggers.is_empty()
        && statement.upsert.is_empty()
        && statement.returning.is_empty()
        && plain_clause(statement.on_conflict)
        && plain_clause(rowid_conflict(table))
        && table.columns.iter().all(|column| {
            plain_clause(column.not_null_conflict) && plain_clause(column.primary_key_conflict)
        })
        && table
            .indexes
            .iter()
            .all(|index| plain_clause(index.conflict))
        && table.foreign_keys.is_empty()
        && table.foreign_key_triggers.is_empty()
}

/// Reports whether the table has an index a duplicate can fail.
///
/// The engine keeps the statement's source rows while it builds such a table,
/// so that a duplicate it finds can hand the statement back to the ordinary
/// insert: which row SQLite names when two constraints fail depends on the
/// order the rows went in, and only the ordinary insert writes them in that
/// order.
///
/// @param table - the table being written
pub fn has_unique_index(table: &TableInfo) -> bool {
    maintained(table).any(|(_, index)| index.unique)
}

/// Reports whether an index's entries need the evaluator.
///
/// The same test `CREATE INDEX` makes when it chooses between scanning the
/// table and running a query, so an index is built from computed entries here
/// exactly when it would be built by a query there.
///
/// @param index - the index
fn index_is_computed(index: &IndexInfo) -> bool {
    index.partial_sql.is_some() || index.columns.iter().any(|key| key.expr_sql.is_some())
}

/// Reports whether the file allows a bulk build: the table holds no row, its
/// tree is keyed by the rowid alone, and no module index follows it.
///
/// @param statement - the bound insert
/// @param target - the file and its trees
pub fn bulk_target_ready(statement: &BoundInsert, target: &mut dyn WriteTarget) -> DbResult<bool> {
    let table = &statement.table;
    if target.captures(table.root) {
        return Ok(false);
    }
    let layout = layout_of(target, table)?;
    if layout.rowid != Some(0) || layout.key_columns.len() != 1 {
        return Ok(false);
    }
    holds_no_row(target, table)
}

/// Builds and checks every row an `INSERT ... SELECT` writes, without writing
/// any.
///
/// Each source row is built by the insert plan, converted by the column
/// affinities, checked against `NOT NULL`, `STRICT` and `CHECK`, and checked
/// against the rowids of the rows kept before it, in that order and in input
/// order. A row a clause says to skip is skipped and gives its rowid back, as
/// `give_back` does for the ordinary insert.
///
/// **The source rows are taken by value when the caller can give them up.** An
/// `INSERT ... SELECT` holds every row the query produced, and holding the
/// built images beside them would double what the statement keeps live. A
/// caller that passes an owning iterator frees each source row as soon as its
/// image exists. Moving the values into the image instead of cloning them was
/// tried and measured no faster on a 100,000 row copy, so the image is built by
/// the same `InsertPlan::build_row` the ordinary insert calls.
///
/// @param statement - the bound insert, which `bulk_shape` accepted
/// @param target - the file and its trees, which `bulk_target_ready` accepted
/// @param params - the bound parameters
/// @param supplied - the rows the `SELECT` produced, in order
pub fn bulk_rows<'r, S: Into<RowSource<'r>>>(
    statement: &BoundInsert,
    target: &mut dyn WriteTarget,
    params: &Params,
    supplied: impl IntoIterator<Item = S>,
) -> Result<BulkRows, BulkFailure> {
    let fail = |error: DbError, last_rowid: Option<i64>| BulkFailure { error, last_rowid };
    let compiled = compile_insert(statement, target, params).map_err(|error| fail(error, None))?;
    let supplied = supplied.into_iter();
    let mut built = Building {
        rows: Vec::with_capacity(supplied.size_hint().0),
        computed: computed_indexes(&statement.table),
        kept: KeptKeys::default(),
        next_rowid: None,
    };
    for source in supplied {
        let last = built.kept.last();
        built
            .add(statement, &compiled, source.into())
            .map_err(|error| fail(error, last))?;
    }
    let last_rowid = built.kept.last();
    let mut rows = built.rows;
    if !built.kept.increasing() {
        rows.sort_unstable_by_key(|row| match row.first() {
            Some(OwnedDatum::Int(rowid)) => *rowid,
            _ => i64::MIN,
        });
    }
    Ok(BulkRows {
        rows,
        computed: built.computed,
        last_rowid,
    })
}

/// Returns an empty entry list for each index whose entries need the
/// evaluator.
///
/// @param table - the table being written
fn computed_indexes(table: &TableInfo) -> Vec<(usize, Vec<Row>)> {
    maintained(table)
        .filter(|(_, index)| index_is_computed(index))
        .map(|(position, _)| (position, Vec::new()))
        .collect()
}

/// The rows built so far, and what the next one is checked against.
struct Building {
    /// The images kept, in input order.
    rows: Vec<Row>,
    /// The computed indexes' entries, in input order.
    computed: Vec<(usize, Vec<Row>)>,
    /// The rowids of the kept images.
    kept: KeptKeys,
    /// The largest rowid handed out so far, as `InsertPlan::build_row` keeps
    /// it.
    next_rowid: Option<i64>,
}

impl Building {
    /// Builds one image, checks it, and keeps it unless a clause skips it.
    ///
    /// The steps are the ordinary insert's, in its order: build, convert, `NOT
    /// NULL`, `STRICT`, `CHECK`, the rowid, and then the index entries, which
    /// the ordinary insert computes after the table's own key has been
    /// claimed.
    ///
    /// @param statement - the bound insert
    /// @param compiled - the plan and the declarations
    /// @param source - one row the `SELECT` produced, lent or handed over
    fn add(
        &mut self,
        statement: &BoundInsert,
        compiled: &CompiledInsert,
        source: RowSource<'_>,
    ) -> DbResult<()> {
        let table = &statement.table;
        let CompiledInsert {
            layout,
            space,
            plan,
            declarations,
        } = compiled;
        let before = self.next_rowid;
        let mut keys = BulkKeys { kept: &self.kept };
        let mut image =
            plan.build_row_from(source, space, &mut self.next_rowid, &mut keys, None)?;
        plan.convert(declarations, space, &mut image)?;
        let declared = resolution_of(statement.on_conflict);
        let met = declarations_are_met(table, layout, declarations, space, &mut image, declared)?;
        if met {
            declarations.types_are_met(table, space, &image)?;
        }
        if !met || !declarations.checks_are_met(space, &image, declared == Resolution::Skip)? {
            // See `give_back`: a skipped row hands its rowid back.
            self.next_rowid = before;
            return Ok(());
        }
        let Some(OwnedDatum::Int(rowid)) = image.first() else {
            return Err(inillucent_base::error::misuse(
                "a row built for a bulk insert has no rowid",
            ));
        };
        let rowid = *rowid;
        if self.kept.contains(rowid) {
            let (code, message) = rowid_message(table);
            return Err(DbError::new(ExtendedCode(code))
                .with_message(message)
                .or_unwind(unwind_of(statement.on_conflict.or(rowid_conflict(table)))));
        }
        let indexes = IndexExprs::new(declarations, space);
        for (position, entries) in &mut self.computed {
            let Some(index) = table.indexes.get(*position) else {
                continue;
            };
            if indexes.holds(*position, &image)? {
                entries.push(index_entry(*position, index, layout, &image, indexes)?);
            }
        }
        self.kept.push(rowid);
        self.rows.push(image);
        Ok(())
    }
}

/// The rowids kept so far, answering membership without a set while they
/// arrive in increasing order.
///
/// An `INSERT ... SELECT` from a rowid table, and one that lets every rowid be
/// allocated, both produce increasing rowids, so the common statement checks
/// each new rowid against the largest one and nothing else. The first rowid
/// that is not larger builds the set from what was kept, and every check after
/// that uses it.
#[derive(Default)]
struct KeptKeys {
    /// Every rowid kept, in input order.
    order: Vec<i64>,
    /// The largest rowid kept.
    largest: Option<i64>,
    /// Every rowid kept, once `order` stopped increasing.
    set: Option<HashSet<i64>>,
}

impl KeptKeys {
    /// Reports whether a rowid was already kept.
    ///
    /// @param rowid - the candidate
    fn contains(&self, rowid: i64) -> bool {
        if let Some(set) = &self.set {
            return set.contains(&rowid);
        }
        match self.largest {
            None => false,
            Some(largest) if rowid > largest => false,
            Some(_) => self.order.binary_search(&rowid).is_ok(),
        }
    }

    /// Records a rowid that has been kept.
    ///
    /// @param rowid - the rowid, which `contains` has just said is new
    fn push(&mut self, rowid: i64) {
        if self.set.is_none() && self.largest.is_some_and(|largest| rowid <= largest) {
            self.set = Some(self.order.iter().copied().collect());
        }
        if let Some(set) = &mut self.set {
            set.insert(rowid);
        }
        self.order.push(rowid);
        self.largest = Some(self.largest.map_or(rowid, |largest| largest.max(rowid)));
    }

    /// Reports whether every rowid so far arrived larger than the one before.
    fn increasing(&self) -> bool {
        self.set.is_none()
    }

    /// Returns the rowid kept last, in input order.
    fn last(&self) -> Option<i64> {
        self.order.last().copied()
    }
}

/// What a rowid allocation asks about the table, answered from the rows kept
/// so far.
///
/// The table held no row when the statement began, so the rows kept by this
/// statement are every row it holds - which is exactly what `TableKeys` would
/// read out of the tree if the rows had been written one at a time.
struct BulkKeys<'k> {
    /// The rowids kept so far.
    kept: &'k KeptKeys,
}

impl RowidKeys for BulkKeys<'_> {
    fn highest(&mut self) -> DbResult<i64> {
        Ok(self.kept.largest.unwrap_or(0))
    }

    fn holds(&mut self, rowid: i64) -> DbResult<bool> {
        Ok(self.kept.contains(rowid))
    }
}
