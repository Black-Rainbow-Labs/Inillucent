//! Separators too long to keep whole in an interior page.
//!
//! Invariant: **an interior page that holds a separator out of line holds its
//! reference and its first bytes, and every comparison that the first bytes do
//! not settle reads the whole separator through the pool.** A separator is the
//! shortest prefix of a leaf's first key that sorts above the previous leaf's
//! last key, so it is long only when two neighbouring keys share a prefix longer
//! than the limit. That is what an index over a column with duplicates of a long
//! value produces: the key is the value and then the rowid.
//!
//! The slot form is the one a leaf keeps for a key stored out of line, over the
//! encoded bytes: the sixteen byte reference, a `u16` prefix length and the
//! prefix. The pages of the separator are an ordinary extent of the tree.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::interior::{
    separator_inline_limit, separator_prefix_bytes, InteriorRef, SlotBytes, OUT_OF_LINE_HEAD_BYTES,
};
use inillucent_pool::{Database, Pool, Swip};

use inillucent_value::collation::Collation;

use crate::datum::Datum;
use crate::format::write_key_extent;
use crate::keyenc::{shorten_separator, KeyEncoding};
use crate::paged::read_extent;
use crate::split_log::StoredSlot;
use crate::write::TreeLog;

/// A separator as a split hands it to the parent page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Separator {
    /// The separator's bytes, whole.
    pub(crate) full: Vec<u8>,
    /// What the parent page stores for it when it is stored out of line: the
    /// reference, the prefix length and the prefix.
    pub(crate) slot: Option<Vec<u8>>,
}

impl Separator {
    /// Returns a separator that the page holds whole.
    ///
    /// @param full - the separator's bytes
    pub(crate) fn inline(full: Vec<u8>) -> Separator {
        Separator { full, slot: None }
    }

    /// Returns what the parent page stores for this separator.
    pub(crate) fn page_slot(&self) -> SlotBytes<'_> {
        match &self.slot {
            Some(record) => (true, record.as_slice()),
            None => (false, self.full.as_slice()),
        }
    }

    /// Reports whether the parent page stores this separator out of line.
    pub(crate) fn is_out_of_line(&self) -> bool {
        self.slot.is_some()
    }
}

/// Returns the slot an interior page stores for a separator written out of line.
///
/// @param reference - where the whole separator was written
/// @param full - the separator's bytes
/// @param page_size - the database's page size in bytes
pub(crate) fn slot_record(reference: ExtentRef, full: &[u8], page_size: usize) -> Vec<u8> {
    let kept = separator_prefix_bytes(page_size).min(full.len());
    let mut record = Vec::with_capacity(OUT_OF_LINE_HEAD_BYTES.saturating_add(kept));
    record.extend_from_slice(&reference.encode());
    record.extend_from_slice(&(kept as u16).to_le_bytes());
    record.extend_from_slice(full.get(..kept).unwrap_or(full));
    record
}

/// Returns how many bytes the parent page spends on a separator of this length.
///
/// The separator itself when it is short enough to keep whole, and otherwise
/// the reference, the prefix length and the prefix.
///
/// @param length - the separator's length in bytes
/// @param page_size - the database's page size in bytes
pub(crate) fn slot_bytes_for(length: usize, page_size: usize) -> usize {
    if length <= separator_inline_limit(page_size) {
        return length;
    }
    OUT_OF_LINE_HEAD_BYTES.saturating_add(separator_prefix_bytes(page_size))
}

/// Makes a separator ready for a parent page, writing it out of line when it is
/// longer than the page keeps whole.
///
/// @param database - the file the separator's pages are allocated in
/// @param log - where the records for those pages go
/// @param tree_id - the tree the separator belongs to
/// @param full - the separator's bytes
pub(crate) fn prepare_separator(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    full: Vec<u8>,
) -> DbResult<Separator> {
    let page_size = database.page_size();
    if full.len() <= separator_inline_limit(page_size) {
        return Ok(Separator::inline(full));
    }
    let reference = write_key_extent(database, log, tree_id, &[], &full)?;
    let slot = Some(slot_record(reference, &full, page_size));
    Ok(Separator { full, slot })
}

/// Returns the whole bytes of one separator of an interior page.
///
/// A separator held whole is copied; one stored out of line is read through the
/// pool.
///
/// @param pool - the buffer pool
/// @param interior - the interior page
/// @param slot - the separator's index
pub(crate) fn separator_full(
    pool: &Pool,
    interior: &InteriorRef<'_>,
    slot: usize,
) -> DbResult<Vec<u8>> {
    match interior.separator_slot(slot)? {
        (false, bytes) => Ok(bytes.to_vec()),
        (true, bytes) => read_extent(pool, ExtentRef::decode(bytes)?),
    }
}

/// Returns the child of an interior page whose range holds a key, whether or
/// not the page keeps a separator out of line.
///
/// @param pool - the buffer pool, which a separator stored out of line is read through
/// @param interior - the interior page
/// @param key - the encoded key being looked for
pub(crate) fn child_in(
    pool: &Pool,
    interior: &InteriorRef<'_>,
    key: &[u8],
) -> DbResult<(usize, Swip, usize)> {
    if !interior.has_extents() {
        return interior.child_for(key);
    }
    interior.child_for_resolving(key, &mut |reference| read_extent(pool, reference))
}

/// Returns the reference of every separator an interior page stores out of line.
///
/// @param interior - the interior page
pub(crate) fn separator_references(interior: &InteriorRef<'_>) -> DbResult<Vec<ExtentRef>> {
    let mut found = Vec::new();
    if !interior.has_extents() {
        return Ok(found);
    }
    for slot in 0..interior.count() {
        if let Some(reference) = interior.out_of_line_reference(slot)? {
            found.push(reference);
        }
    }
    Ok(found)
}

/// Checks the separators of an interior page against each other, reading the
/// whole of any that is stored out of line.
///
/// The page checks the order over the bytes it holds. This is the check over the
/// whole separators, and it is the one that settles two neighbours whose leading
/// bytes are the same.
///
/// @param pool - the buffer pool
/// @param interior - the interior page
pub(crate) fn check_separator_order(pool: &Pool, interior: &InteriorRef<'_>) -> DbResult<()> {
    if !interior.has_extents() {
        return Ok(());
    }
    let mut before: Option<Vec<u8>> = None;
    for slot in 0..interior.count() {
        let whole = separator_full(pool, interior, slot)?;
        if before.as_ref().is_some_and(|held| *held >= whole) {
            return Err(corrupt(format!(
                "interior slot {slot} does not follow the one before it"
            )));
        }
        before = Some(whole);
    }
    Ok(())
}

/// Returns the separator that opens one planned leaf of a bulk build.
///
/// The leaf's first key, encoded. When that is longer than an interior page keeps
/// whole it is the shortest prefix of that key which sorts above the previous
/// leaf's last key, exactly as a split shortens it.
///
/// @param head - scratch space for one key tuple
/// @param rows - the rows, already in key order
/// @param at - the position of the leaf's first row
/// @param key_columns - how many leading columns are the key
/// @param encoding - how a key tuple becomes comparable bytes
/// @param collations - one per key column
/// @param directions - one per key column, true for descending
/// @param page_size - the file's page size, which decides how long a separator may be
pub(crate) fn leaf_separator<'d>(
    head: &mut Vec<Datum<'d>>,
    rows: &dyn crate::leaf::Rows<'d>,
    at: usize,
    key_columns: usize,
    encoding: KeyEncoding,
    collations: &[Collation],
    directions: &[bool],
    page_size: usize,
) -> Vec<u8> {
    let mut encode = |row: usize, out: &mut Vec<u8>| {
        head.clear();
        for column in 0..key_columns {
            head.push(rows.value(row, column));
        }
        encoding.encode_into(head, collations, directions, out);
    };
    let mut separator = Vec::new();
    encode(at, &mut separator);
    if at > 0 && separator.len() > separator_inline_limit(page_size) {
        let mut previous = Vec::new();
        encode(at.saturating_sub(1), &mut previous);
        separator = shorten_separator(&previous, &separator).to_vec();
    }
    separator
}

/// Checks that a child's keys lie between the separators on either side of it.
///
/// Child `i` holds every key `k` with `K[i-1] <= k < K[i]`. Separators stored out
/// of line are read whole.
///
/// @param pool - the buffer pool
/// @param interior - the parent page
/// @param child - which child
/// @param lowest - the lowest live encoded key under the child, if it holds one
/// @param highest - the highest live encoded key under the child, if it holds one
pub(crate) fn check_child_range(
    pool: &Pool,
    interior: &InteriorRef<'_>,
    child: usize,
    lowest: Option<&Vec<u8>>,
    highest: Option<&Vec<u8>>,
) -> DbResult<()> {
    if let (Some(lowest), Some(below)) = (lowest, child.checked_sub(1)) {
        if lowest.as_slice() < separator_full(pool, interior, below)?.as_slice() {
            return Err(corrupt(format!(
                "a key below separator {below} is in the child above it"
            )));
        }
    }
    if let Some(highest) = highest {
        if child < interior.count()
            && highest.as_slice() >= separator_full(pool, interior, child)?.as_slice()
        {
            return Err(corrupt(format!(
                "a key at or above separator {child} is in the child below it"
            )));
        }
    }
    Ok(())
}

/// Writes the separators one interior page takes, those too long to keep whole
/// out of line, and returns what the page stores for each.
///
/// @param database - the file being built into
/// @param log - where the records go, when there are any
/// @param tree_id - the tree these pages belong to
/// @param keys - the separators, whole
pub(crate) fn store_separators(
    database: &mut Database,
    log: &mut Option<&mut dyn crate::write::TreeLog>,
    tree_id: u64,
    keys: &[Vec<u8>],
) -> DbResult<Vec<StoredSlot>> {
    let mut nowhere = crate::write::NoLog::default();
    let mut stored = Vec::with_capacity(keys.len());
    for key in keys {
        let sink: &mut dyn crate::write::TreeLog = match log.as_deref_mut() {
            Some(log) => log,
            None => &mut nowhere,
        };
        let separator = prepare_separator(database, sink, tree_id, key.clone())?;
        stored.push(match separator.slot {
            Some(record) => (true, record),
            None => (false, separator.full),
        });
    }
    Ok(stored)
}
