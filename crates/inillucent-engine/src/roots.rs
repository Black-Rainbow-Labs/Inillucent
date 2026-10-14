//! Keeping the roots of a file that gives pages back at the start of it.
//!
//! Invariant: **in a file whose mode gives pages back and whose meta record names a
//! largest root, every root page is at or below that page.** A root is named by a
//! catalog row, which the step that moves pages does not rewrite, so a root is never
//! moved to empty the end of the file. SQLite keeps the same rule and the same
//! number. A new tree takes the first page above the largest root, moving whatever
//! is there out of the way, and a drop moves the highest root into the page the
//! dropped root leaves.
//!
//! A file whose largest root is zero has never had its roots placed, which is every
//! file in mode 0 and a file whose mode was set by an older build. None of this
//! applies to it, and the step stops at a root it meets there.

use std::collections::BTreeSet;

use inillucent_base::error::refusal;
use inillucent_base::DbResult;
use inillucent_pool::PageId;
use inillucent_tree::relocate::{relocate_page, relocate_root, Relocated};
use inillucent_tree::PagedTree;

use crate::engine::write::WalLog;
use crate::shrink::SchemaTrees;
use crate::*;

impl ImportedDatabase {
    /// Reports whether a file keeps its roots at the start.
    ///
    /// @param at - the schema, as the binder numbers them
    fn keeps_roots_low(&self, at: usize) -> bool {
        self.schema_file(at)
            .is_some_and(|file| file.auto_vacuum() != 0 && file.largest_root() != 0)
    }

    /// Returns the page of every root in a file, the catalog's included.
    ///
    /// @param at - the schema, as the binder numbers them
    fn root_pages_of(&self, at: usize) -> BTreeSet<u64> {
        let mut roots: BTreeSet<u64> = self
            .entries_of(at)
            .iter()
            .filter(|held| !held.entry.root.is_none())
            .map(|held| held.entry.root.0)
            .collect();
        if let Some(file) = self.schema_file(at) {
            roots.insert(file.catalog_root().0);
        }
        roots
    }

    /// Puts the root of a tree just built at the first page above the largest root.
    ///
    /// A page already there is moved to a free page first. A page that cannot be
    /// moved, such as an extent page written with no owner key, is passed over and
    /// the root takes the next page. The file's largest root becomes the page chosen.
    ///
    /// @param at - the schema the tree belongs to
    /// @param txn - the transaction the records belong to
    /// @param tree - the tree, whose root is moved
    pub(crate) fn place_root(
        &mut self,
        at: usize,
        txn: u64,
        mut tree: PagedTree,
    ) -> DbResult<PagedTree> {
        if !self.keeps_roots_low(at) {
            return Ok(tree);
        }
        let mut slot = self.schema_file(at).map_or(0, |file| file.largest_root()) + 1;
        loop {
            let file = self.schema_file(at).ok_or_else(not_attached)?;
            if file.page_is_reserved(PageId(slot)) {
                slot += 1;
                continue;
            }
            let free = slot >= file.pool().page_count() || !file.page_is_allocated(PageId(slot));
            if slot == tree.root().0 || free || self.clear_slot(at, txn, &tree, PageId(slot))? {
                break;
            }
            slot += 1;
        }
        self.move_tree_root(at, txn, &mut tree, PageId(slot))?;
        self.set_largest_root_of(at, slot)?;
        Ok(tree)
    }

    /// Moves whatever page occupies `slot` to a free page, when it can be moved.
    ///
    /// @param at - the schema
    /// @param txn - the transaction the records belong to
    /// @param extra - a tree built in this statement and not yet registered
    /// @param slot - the page to clear
    fn clear_slot(
        &mut self,
        at: usize,
        txn: u64,
        extra: &PagedTree,
        slot: PageId,
    ) -> DbResult<bool> {
        let handles = self.handles_of(at);
        let mut log = self.log_for(at, txn)?;
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
            extra: Some(extra),
        };
        let to = database.allocate(1)?;
        match relocate_page(database, &mut log, &lookup, slot, to)? {
            Relocated::Moved => Ok(true),
            Relocated::Pinned(_) => {
                database.release(to, 1)?;
                Ok(false)
            }
        }
    }

    /// Moves a tree's root page to `to`, leaving the catalog row to the caller.
    ///
    /// @param at - the schema
    /// @param txn - the transaction the records belong to
    /// @param tree - the tree
    /// @param to - the page to move the root to
    fn move_tree_root(
        &mut self,
        at: usize,
        txn: u64,
        tree: &mut PagedTree,
        to: PageId,
    ) -> DbResult<()> {
        if tree.root() == to {
            return Ok(());
        }
        let mut log = self.log_for(at, txn)?;
        let session = self.session_state.session.get();
        let database = file_of(
            &mut self.storage.database,
            &mut self.session_state.attached,
            &mut self.session_state.temps,
            session,
            at,
        )?;
        relocate_root(database, &mut log, tree, to)
    }

    /// Records the largest root in a file's meta record.
    ///
    /// @param at - the schema
    /// @param root - the page number
    fn set_largest_root_of(&mut self, at: usize, root: u64) -> DbResult<()> {
        let session = self.session_state.session.get();
        file_of(
            &mut self.storage.database,
            &mut self.session_state.attached,
            &mut self.session_state.temps,
            session,
            at,
        )?
        .set_largest_root(root);
        Ok(())
    }

    /// Returns a log for records of one transaction in one file.
    ///
    /// @param at - the schema
    /// @param txn - the transaction
    fn log_for(&self, at: usize, txn: u64) -> DbResult<WalLog<'static>> {
        let wal = self.log_of(at).ok_or_else(not_attached)?;
        Ok(WalLog {
            wal,
            txn,
            schema: at,
            wrote: false,
            undo: None,
            uncommitted: self.uncommitted_handle_of(at),
        })
    }

    /// Fills the pages the roots of dropped trees left, from the top, at a commit.
    ///
    /// **At the commit, after the dropped pages are freed**, because a drop that is
    /// rolled back must find its root where it was. The highest root moves into the
    /// lowest free page above the first root, its catalog row is rewritten under
    /// the same transaction, and the largest root falls to the highest root left.
    ///
    /// @param txn - the committing transaction
    /// @param participants - the files the transaction wrote
    pub(crate) fn compact_roots(&mut self, txn: u64, participants: u16) -> DbResult<()> {
        let released = self.writing.take_roots_released();
        for at in schemas_in(participants & released) {
            if self.keeps_roots_low(at) {
                self.compact_roots_of(at, txn)?;
            }
        }
        // A `VACUUM` of a file that does not give pages back by itself packed
        // every tree into a new one, so the new roots sit at the end of the file
        // and the step that cuts it would stop at the first of them.
        for at in schemas_in(participants) {
            if self.is_being_vacuumed(at) && !self.keeps_roots_low(at) {
                self.compact_roots_of(at, txn)?;
            }
        }
        Ok(())
    }

    /// Reports whether this commit carries a `VACUUM` of one file in a mode that
    /// does not give pages back by itself.
    ///
    /// `VACUUM` of such a file is the only statement that queues every page for
    /// the step, which is how it is told apart from `PRAGMA incremental_vacuum`.
    ///
    /// @param at - the schema, as the binder numbers them
    fn is_being_vacuumed(&self, at: usize) -> bool {
        self.schema_file(at)
            .is_some_and(|file| file.auto_vacuum() == 0)
            && self.writing.incremental_steps().borrow().get(&at) == Some(&u64::MAX)
    }

    /// Compacts the roots of one file.
    ///
    /// @param at - the schema
    /// @param txn - the committing transaction
    fn compact_roots_of(&mut self, at: usize, txn: u64) -> DbResult<()> {
        loop {
            let roots = self.root_pages_of(at);
            let (Some(first), Some(top)) = (roots.first().copied(), roots.last().copied()) else {
                return Ok(());
            };
            self.set_largest_root_of(at, top)?;
            let file = self.schema_file(at).ok_or_else(not_attached)?;
            let hole = file.lowest_free_below(top).filter(|hole| hole.0 > first);
            let Some(hole) = hole else {
                return Ok(());
            };
            if !self.move_root_into(at, txn, top, hole)? {
                return Ok(());
            }
        }
    }

    /// Moves the root on page `top` to `hole` and rewrites its catalog row.
    ///
    /// Answers `false` when no tree is registered at that root, which stops the
    /// compaction rather than looping.
    ///
    /// @param at - the schema
    /// @param txn - the committing transaction
    /// @param top - the root page to move
    /// @param hole - the free page to move it to
    fn move_root_into(&mut self, at: usize, txn: u64, top: u64, hole: PageId) -> DbResult<bool> {
        let found = self
            .entries_of(at)
            .iter()
            .find(|held| held.entry.root.0 == top)
            .map(|held| (held.rowid, held.root, held.entry.clone()));
        let Some((rowid, handle, mut entry)) = found else {
            return Ok(false);
        };
        let Some(mut tree) = self.schema.trees.remove(&handle) else {
            return Ok(false);
        };
        let moved = self.move_tree_root(at, txn, &mut tree, hole);
        self.schema.trees.insert(handle, tree);
        moved?;
        entry.root = hole;
        self.rewrite_in(at, txn, rowid, entry)?;
        Ok(true)
    }
}

/// The refusal for a commit that names a file the connection no longer holds.
fn not_attached() -> inillucent_base::DbError {
    refusal("a commit names a database that is not attached")
}
