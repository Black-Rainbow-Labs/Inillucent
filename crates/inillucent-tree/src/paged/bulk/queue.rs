//! Built leaves waiting to be written, so a run of consecutive pages goes to the file in one call.
//!
//! Invariant: **the bytes that reach the file are the bytes one write per page would have
//! written, and every page is written before the build returns.** Each page is logged as it is
//! queued, stamped and checksummed when the queue is written, and a page a live rollback journal
//! could put back is not queued at all.

use super::super::*;

/// How many built leaves are written with one call (task-2226).
const WRITE_RUN: usize = 64;

/// Built leaves waiting to be written, as one run of consecutive pages.
#[derive(Default)]
pub(super) struct PendingRun {
    /// The first page of the run.
    first: PageId,
    /// The pages' bytes, one after another.
    bytes: Vec<u8>,
    /// Each page's `AllocPage` LSN, or zero for an unlogged build.
    stamps: Vec<u64>,
}

/// Logs a built leaf's allocation and queues its image, writing the queue when it is full or
/// when the page does not continue it.
///
/// A page a live rollback journal could put back goes the way [`write_built_page`] sends it,
/// after the queue, so the queue holds only pages that are written straight to the file.
///
/// @param log - where the allocation record goes, when there is one
/// @param database - the file the page is written into
/// @param pending - the queue
/// @param id - the page, already allocated
/// @param image - the page bytes
pub(super) fn queue_built_page(
    log: &mut Option<&mut dyn crate::write::TreeLog>,
    database: &mut Database,
    pending: &mut PendingRun,
    id: PageId,
    image: &mut [u8],
) -> DbResult<()> {
    if log.is_some() && database.pool().journal_holds(id) {
        flush_built_pages(database, pending)?;
        return log_allocated_page(log, database, id, image);
    }
    let next = PageId(pending.first.0.saturating_add(pending.stamps.len() as u64));
    if !pending.stamps.is_empty() && id != next {
        flush_built_pages(database, pending)?;
    }
    let mut stamp = 0u64;
    if let Some(log) = log.as_mut() {
        stamp = log.log(inillucent_wal::record::Body::AllocPage { page: id.0 })?;
    }
    if pending.stamps.is_empty() {
        pending.first = id;
    }
    pending.bytes.extend_from_slice(image);
    pending.stamps.push(stamp);
    if pending.stamps.len() >= WRITE_RUN {
        flush_built_pages(database, pending)?;
    }
    Ok(())
}

/// Writes the queued leaves with one call and empties the queue.
///
/// @param database - the file the pages are written into
/// @param pending - the queue
pub(super) fn flush_built_pages(database: &mut Database, pending: &mut PendingRun) -> DbResult<()> {
    if pending.stamps.is_empty() {
        return Ok(());
    }
    database
        .pool()
        .write_built_pages(pending.first, &mut pending.bytes, &pending.stamps)?;
    pending.bytes.clear();
    pending.stamps.clear();
    Ok(())
}
