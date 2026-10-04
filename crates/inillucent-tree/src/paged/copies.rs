//! Packed copies of written leaves, handed to readers in place of the page.
//!
//! Invariant: **a copy holds exactly the live rows its page holds, in the
//! same order, and is used only at the page generation it was made from.**
//! The pool drops a copy the moment its page's bytes change; see
//! `inillucent_pool`'s `pool/merged.rs`. A walk that has to see the page
//! itself, the integrity check, refuses copies.

use inillucent_base::DbResult;
use inillucent_pool::Pool;

use crate::leaf::LeafRef;

use super::*;

/// Whether a walk may hand a reader a packed copy of a written leaf in place of its page.
///
/// Every walk a query makes allows it; see [`PagedTree::with_leaf_extents`]. The
/// integrity check refuses it, because what it checks is the page: a copy is
/// always well formed, so a check that read one would pass a damaged delta area.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Copies {
    /// A packed copy may stand in for a written leaf.
    Allowed,
    /// The page itself is read.
    Refused,
}

impl Copies {
    /// Returns the page a copy may be looked up for, or `None` when copies are refused.
    ///
    /// @param page - the leaf being visited
    pub(super) fn of(self, page: PageId) -> Option<PageId> {
        match self {
            Copies::Allowed => Some(page),
            Copies::Refused => None,
        }
    }
}

impl PagedTree {
    /// Returns a packed copy of a written leaf for a reader, when one is kept or worth making.
    ///
    /// **The second read of a written leaf at one generation packs it
    /// (task-2183).** The copy holds the leaf's live rows exactly as a
    /// compaction would write them, so a reader handed it sees the rows the
    /// merge would give, through the path a leaf nothing has written to takes.
    /// It is never written anywhere: the pool keeps it beside the page and
    /// drops it the moment the page's bytes change. A leaf whose rows would not
    /// pack into one page, and one shaped by a different column list than the
    /// tree's, are read as they are.
    ///
    /// @param pool - the buffer pool holding the page
    /// @param at - the leaf's page
    /// @param leaf - the leaf as the page holds it
    pub(super) fn merged_copy(
        &self,
        pool: &Pool,
        at: PageId,
        leaf: &LeafRef<'_>,
    ) -> DbResult<Option<std::rc::Rc<[u8]>>> {
        let Some(generation) = pool.generation_of(at) else {
            return Ok(None);
        };
        if let Some(image) = pool.merged_copy(at, generation) {
            return Ok(Some(image));
        }
        if !pool.merged_sighting(at, generation)
            || leaf.column_count() != self.columns().len()
            || leaf.key_columns() != self.key_columns()
        {
            return Ok(None);
        }
        let source = leaf.live_source()?;
        let builder = LeafBuilder::new(
            self.page_size(),
            self.tree_id(),
            self.columns().to_vec(),
            self.key_columns(),
        )?;
        let Some(mut image) = crate::write::compact_image(&builder, &source)? else {
            return Ok(None);
        };
        page::set_right(&mut image, leaf.right_sibling())?;
        crate::page::write_u64(
            &mut image,
            crate::leaf::leaf_header::MAX_CTS,
            leaf.max_cts(),
        )?;
        let image: std::rc::Rc<[u8]> = std::rc::Rc::from(image);
        pool.keep_merged_copy(at, generation, std::rc::Rc::clone(&image));
        Ok(Some(image))
    }
}
