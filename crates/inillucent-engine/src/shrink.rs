//! The step that gives pages back at the end of a commit.
//!
//! Invariant: **the page count falls only by whole pages taken from the end of the
//! file, and a page at the end is freed before the count passes it.** A page at the
//! end that is free is simply given up. One that is in use is moved to the lowest
//! free page first, with every field that named it rewritten under the commit's own
//! records, so a crash at any point either keeps the file as it was or replays the
//! whole step. The file itself is cut afterwards, by the checkpoint that writes the
//! lower count, so at no moment is it shorter than the record on disk says.
//!
//! It runs for a file in `auto_vacuum` mode 1 (`FULL`) at every commit that has a
//! free page below the end, and for a file in mode 2 (`INCREMENTAL`) for as many
//! pages as `PRAGMA incremental_vacuum` asked for in the transaction. A file in mode
//! 0 never runs it.
//!
//! A page it cannot move stops it, and the file keeps the pages above that page
//! until the page is gone. The reasons are in `inillucent_tree::relocate::Pinned`.

use std::collections::HashMap;

use inillucent_base::error::refusal;
use inillucent_base::DbResult;
use inillucent_pool::Database;
use inillucent_pool::PageId;
use inillucent_tree::relocate::{relocate_page, Pinned, Relocated, TreeLookup};
use inillucent_tree::write::TreeLog;
use inillucent_tree::PagedTree;
use inillucent_wal::record::Body;

use crate::engine::write::WalLog;
use crate::*;

/// What one run of the step did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Shrunk {
    /// How many pages the page count fell by.
    pub(crate) freed: u64,
    /// The page the step stopped at and why, when it stopped at one it could not move.
    pub(crate) pinned: Option<(PageId, Pinned)>,
}

/// The trees of one file, found by the identifier a page carries.
pub(crate) struct SchemaTrees<'a> {
    /// Every tree the connection holds, by handle.
    pub(crate) trees: &'a HashMap<u32, PagedTree>,
    /// The handle of each tree of this file, by the file's own identifier.
    pub(crate) handles: HashMap<u64, u32>,
    /// A tree built in this statement that no handle names yet.
    pub(crate) extra: Option<&'a PagedTree>,
}

impl TreeLookup for SchemaTrees<'_> {
    /// Returns the tree of this file with the identifier.
    ///
    /// @param tree_id - the identifier a page carries in its header
    fn tree(&self, tree_id: u64) -> Option<&PagedTree> {
        if let Some(held) = self.extra.filter(|held| held.tree_id() == tree_id) {
            return Some(held);
        }
        self.handles
            .get(&tree_id)
            .and_then(|handle| self.trees.get(handle))
    }
}

/// Gives pages back from the end of a file, up to a limit.
///
/// Appends a `PageCount` record when the page count fell. The caller commits.
///
/// @param database - the file
/// @param log - where the records go
/// @param trees - finds the tree a page belongs to
/// @param limit - the most pages to give back
pub(crate) fn shrink(
    database: &mut Database,
    log: &mut dyn TreeLog,
    trees: &dyn TreeLookup,
    limit: u64,
) -> DbResult<Shrunk> {
    let mut freed = 0u64;
    let mut pinned = None;
    while freed < limit {
        let count = database.pool().page_count();
        let Some(end) = count.checked_sub(1).filter(|end| *end >= 2).map(PageId) else {
            break;
        };
        if database.page_is_reserved(end) || !database.page_is_allocated(end) {
            database.lower_page_count(end.0);
            freed = freed.saturating_add(1);
            continue;
        }
        let Some(to) = database.lowest_free_below(end.0) else {
            break;
        };
        match relocate_page(database, log, trees, end, to)? {
            Relocated::Moved => {
                database.lower_page_count(end.0);
                freed = freed.saturating_add(1);
            }
            Relocated::Pinned(why) => {
                pinned = Some((end, why));
                break;
            }
        }
    }
    if freed > 0 {
        log.log(Body::PageCount {
            pages: database.pool().page_count(),
        })?;
        database.discard_beyond_page_count()?;
    }
    Ok(Shrunk { freed, pinned })
}

impl ImportedDatabase {
    /// Runs the step for every file of a commit that is in a mode that gives pages
    /// back, and returns the files whose page count fell.
    ///
    /// @param txn - the transaction the records belong to
    /// @param participants - the files the transaction wrote
    pub(crate) fn shrink_participants(&mut self, txn: u64, participants: u16) -> DbResult<u16> {
        let mut shrunk = 0u16;
        for at in schemas_in(participants) {
            let limit = self.pages_to_give_back(at);
            if limit > 0 && self.shrink_file(at, txn, limit)?.freed > 0 {
                shrunk |= schema_bit(at);
            }
        }
        self.writing.incremental_steps().borrow_mut().clear();
        Ok(shrunk)
    }

    /// Returns how many pages this commit may give back from one file.
    ///
    /// Everything in mode 1, whatever `PRAGMA incremental_vacuum` asked for in
    /// mode 2, and nothing in mode 0, in a file with no free page below its end,
    /// or in a file with no file on disk.
    ///
    /// @param at - the schema, as the binder numbers them
    fn pages_to_give_back(&self, at: usize) -> u64 {
        let Some(file) = self.schema_file(at).filter(|_| self.path_of(at).is_some()) else {
            return 0;
        };
        let limit = match file.auto_vacuum() {
            1 => u64::MAX,
            2 => self
                .writing
                .incremental_steps()
                .borrow()
                .get(&at)
                .copied()
                .unwrap_or(0),
            _ => 0,
        };
        match file.has_free_below(file.pool().page_count()) {
            true => limit,
            false => 0,
        }
    }

    /// Gives pages back from one file.
    ///
    /// @param at - the schema, as the binder numbers them
    /// @param txn - the transaction the records belong to
    /// @param limit - the most pages to give back
    fn shrink_file(&mut self, at: usize, txn: u64, limit: u64) -> DbResult<Shrunk> {
        let wal = self
            .log_of(at)
            .ok_or_else(|| refusal("a commit names a database that is not attached"))?;
        let mut log = WalLog {
            wal,
            txn,
            schema: at,
            wrote: false,
            undo: None,
            uncommitted: self.uncommitted_handle_of(at),
        };
        let handles = self.handles_of(at);
        let session = self.session_state.session.get();
        let database = file_of(
            &mut self.storage.database,
            &mut self.session_state.attached,
            &mut self.session_state.temps,
            session,
            at,
        )?;
        let lookup = SchemaTrees {
            trees: &self.schema.trees,
            handles,
            extra: None,
        };
        shrink(database, &mut log, &lookup, limit)
    }

    /// Writes the lower page count to the file, when no reader is in the way.
    ///
    /// **A checkpoint cuts the file**, and the cut is what makes the file short when
    /// the statement returns. It waits for no reader: a file a reader holds shrinks
    /// at the next checkpoint or the next open, and `PRAGMA page_count` already
    /// reports the lower count. A commit never reports `busy` for a cut, because the
    /// commit is already durable.
    ///
    /// @param shrunk - the files whose page count fell
    pub(crate) fn checkpoint_after_shrink(&mut self, shrunk: u16) -> DbResult<()> {
        if shrunk == 0 {
            return Ok(());
        }
        let all_free = schemas_in(shrunk).all(|at| {
            self.schema_file(at)
                .is_none_or(|file| file.try_hold_for_writing(0))
        });
        if all_free {
            // The checkpoint writes many pages at once, so they are made durable
            // before the meta record that describes them can be.
            for at in schemas_in(shrunk) {
                if let Some(file) = self.schema_file(at) {
                    file.sync_pages_before_the_next_meta();
                }
            }
        }
        match all_free {
            true => self.checkpoint(),
            false => Ok(()),
        }
    }

    /// Frees what the transaction gave up and runs the step, before the commit.
    ///
    /// Returns the participants, which now include every file a free touched, and
    /// the files the step shrank.
    ///
    /// @param txn - the transaction
    /// @param participants - the files the transaction wrote
    pub(crate) fn settle_space(&mut self, txn: u64, participants: u16) -> DbResult<(u16, u16)> {
        self.flush_pending_frees(txn)?;
        let participants = participants | self.writing.replace_touched(0);
        self.compact_roots(txn, participants)?;
        let shrunk = self.shrink_participants(txn, participants)?;
        Ok((participants, shrunk))
    }
}

impl ImportedDatabase {
    /// Returns the handle of each tree of one file, by the file's own identifier.
    ///
    /// @param at - the schema, as the binder numbers them
    pub(crate) fn handles_of(&self, at: usize) -> HashMap<u64, u32> {
        self.schema
            .trees
            .iter()
            .filter(|(handle, _)| self.session_state.schema_of(**handle) == at)
            .map(|(handle, tree)| (tree.tree_id(), *handle))
            .collect()
    }
}
