//! Moving a file to format 3 the first time it stores a key out of line.
//!
//! Invariant: **the data file never holds a key extent page, or a leaf or
//! interior page that names one, while its meta record says format 2 and its log
//! holds no format record.** The record is written before the first page of the
//! first key or separator stored out of line, and it belongs to no transaction,
//! so recovery replays it whether or not the transaction that wrote it
//! committed. A release before this one refuses a log holding it by kind, which is
//! how it refuses every record added since 0.1.8.
//!
//! A file with no such value stays at format 2 and is read by every release from
//! 0.1.8 on. Nothing lowers the number once it has moved.

use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::meta::FORMAT_VERSION;
use inillucent_pool::Database;

use crate::paged::write_extent_owned;
use crate::write::TreeLog;

/// Makes the file say format 3, logging the move when this is the first time.
///
/// Does nothing when the file already says 3.
///
/// @param database - the file
/// @param log - where the format record goes
pub(crate) fn require_key_extents(database: &mut Database, log: &mut dyn TreeLog) -> DbResult<()> {
    if database.raise_format(FORMAT_VERSION) {
        log.log_format(FORMAT_VERSION)?;
    }
    Ok(())
}

/// Writes a key or a separator out of line, moving the file to format 3 first.
///
/// @param database - the file the pages are allocated in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param owner - the key of the row that holds the reference, or empty for none
///   (a separator stored out of line is held by an interior page, which no key finds)
/// @param value - the bytes to store
pub(crate) fn write_key_extent(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    owner: &[u8],
    value: &[u8],
) -> DbResult<ExtentRef> {
    require_key_extents(database, log)?;
    write_extent_owned(database, log, tree_id, owner, value)
}
