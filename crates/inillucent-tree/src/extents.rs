//! Reading, writing and freeing the out of line values of a tree.
//!
//! Invariant: **a value out of line is reached through its reference alone, one
//! page at a time by following each page's own link to the next.** A chain is
//! contiguous when the file had a run long enough and is not once a page of it has
//! moved, so no reader or freer assumes the pages of a value are consecutive.
//!
//! A file that writes owned extent pages (see `inillucent_pool::extent::owned`)
//! records on each page who refers to it. The owner key a writer is given is the
//! key of the row that holds the reference, as its tree encodes it; a writer
//! with no key to give stores none, and the page it writes can be read but is
//! never moved. A format 2 file writes the older layout until it is raised to
//! format 3.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::{self, ExtentRef};
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{Database, PageId, Pool};
use inillucent_wal::record::Body;

use crate::write::TreeLog;

/// Reads one out-of-line value.
///
/// The reference's length is checked against the lengths the pages themselves
/// declare rather than trusted: the two disagreeing is a corruption, and reading
/// the longer of the two would walk off the end of the chain. A reference that
/// stands for a value already freed reads as nothing.
///
/// @param pool - the buffer pool the file is open through
/// @param reference - what the leaf holds
pub fn read_extent(pool: &Pool, reference: ExtentRef) -> DbResult<Vec<u8>> {
    if reference.is_dead() {
        return Ok(Vec::new());
    }
    // A value packed into a shared page is one slot of one page, so it is one
    // fetch and one copy - there is no chain to follow and no length to
    // reconcile beyond the one the slot itself declares.
    if let Some(slot) = reference.slot {
        let guard = pool.fetch(reference.first)?;
        let held = extent::shared::read(guard.bytes(), slot)?;
        if held.len() as u64 != reference.length {
            return Err(corrupt(format!(
                "a shared extent slot holds {} bytes where its reference says {}",
                held.len(),
                reference.length
            )));
        }
        return Ok(held.to_vec());
    }
    read_chain(pool, reference)
}

/// Reads a value stored as a chain of pages.
///
/// @param pool - the buffer pool the file is open through
/// @param reference - what the leaf holds
fn read_chain(pool: &Pool, reference: ExtentRef) -> DbResult<Vec<u8>> {
    let mut out = Vec::with_capacity(reference.length.min(1 << 20) as usize);
    let mut page = reference.first;
    let mut pages = 0u64;
    // A chain that loops would otherwise read for ever. The bound is the most
    // pages the declared length could possibly need in either layout, plus one for
    // the zero length case.
    let bound = extent::pages_needed(reference.length, pool.page_size())
        .max(extent::owned::chain_bound(
            reference.length,
            pool.page_size(),
        ))
        .saturating_add(1);
    while !page.is_none() {
        let guard = pool.fetch(page)?;
        // **The page number, because the message alone does not identify one.**
        // A page whose kind is no longer `BlobExtent` is a page something else
        // has taken over, and naming the page number is what tied a failing
        // read to a `CREATE TABLE`'s root page landing on a page a recovery bug
        // had left mismarked as free while it was still live.
        let (body, next) = extent::read_page(guard.bytes()).map_err(|error| {
            corrupt(format!(
                "{} at page {}, which is page {} of an extent starting at {} and {} bytes long",
                error.detail().unwrap_or("an extent page is not readable"),
                page.0,
                pages.saturating_add(1),
                reference.first.0,
                reference.length,
            ))
        })?;
        out.extend_from_slice(body);
        page = next;
        pages = pages.saturating_add(1);
        if out.len() as u64 > reference.length {
            return Err(corrupt(
                "an extent holds more bytes than its reference says",
            ));
        }
        if pages > bound {
            return Err(corrupt("an extent chain is longer than its length allows"));
        }
    }
    if out.len() as u64 != reference.length {
        return Err(corrupt(format!(
            "an extent holds {} bytes where its reference says {}",
            out.len(),
            reference.length
        )));
    }
    Ok(out)
}

/// Writes one value out of line with no owner key, logging every page it takes.
///
/// For a value whose reference is not in a leaf row that a key finds: a separator
/// stored out of line in an interior page. On a file that writes owned pages the
/// page is owned with a key length of zero, and the step that moves pages leaves
/// it where it is.
///
/// @param database - the file the pages are allocated and installed in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param value - the bytes to store
pub fn write_extent(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    value: &[u8],
) -> DbResult<ExtentRef> {
    write_extent_owned(database, log, tree_id, &[], value)
}

/// Writes one value out of line, recording the key of the row that owns it.
///
/// The allocator asks the free map for a **contiguous run** big enough for the
/// whole value, so the common case is one seek and one sequential read rather
/// than SQLite's chain of dependent four kilobyte reads. A value that fits inside
/// one page shares one with other values, so a value one byte over the spill
/// threshold does not cost a whole page.
///
/// @param database - the file the pages are allocated and installed in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param owner_key - the key of the row that holds the reference, or empty for none
/// @param value - the bytes to store
pub fn write_extent_owned(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    owner_key: &[u8],
    value: &[u8],
) -> DbResult<ExtentRef> {
    let page_size = database.page_size();
    if !database.writes_owned_extents() {
        return write_older_extent(database, log, tree_id, value);
    }
    let key_len = extent::owned::stored_key_len(page_size, owner_key);
    let key = owner_key.get(..key_len).unwrap_or(&[]);
    if value.len() <= extent::shared::owned_capacity(page_size, key_len) {
        return write_owned_packed(database, log, tree_id, key, value);
    }
    let pages = extent::owned::pages_needed(value.len() as u64, page_size, key_len);
    let first = database.allocate(pages)?;
    let images = extent::owned::encode_run(value, first, page_size, tree_id, key)?;
    for (id, mut image) in images {
        log_allocated_page(&mut Some(log), database, id, &mut image)?;
    }
    Ok(ExtentRef::run(first, value.len() as u64))
}

/// Writes one value in the layout files wrote before extent pages were owned.
///
/// @param database - the file the pages are allocated and installed in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param value - the bytes to store
fn write_older_extent(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    value: &[u8],
) -> DbResult<ExtentRef> {
    let page_size = database.page_size();
    let pages = extent::pages_needed(value.len() as u64, page_size).max(1);
    // **It has to fit a shared page, which holds less than a page.** The count
    // above is pages of *payload*; a shared page spends another fifty six bytes on
    // its header and the directory entry, so a value between the two reached
    // `shared::place` on a page made fresh for it and was refused with `a shared
    // extent page has no room for this value`. Measured on the default page size:
    // a bound TEXT of 32,000 bytes stored and one of 32,700 did not.
    if pages <= 1 && value.len() <= extent::shared::capacity(page_size) {
        return write_older_packed(database, log, tree_id, value);
    }
    let first = database.allocate(pages)?;
    let images = extent::encode_run(value, first, page_size, tree_id)?;
    for (id, mut image) in images {
        log_allocated_page(&mut Some(log), database, id, &mut image)?;
    }
    Ok(ExtentRef::run(first, value.len() as u64))
}

/// Puts one small value into a shared page in the older layout.
///
/// The page the last value went on is tried first, a hint held on the `Database`
/// in memory, and a fresh page is allocated when it is full or there is none.
///
/// @param database - the file the page is allocated and installed in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param value - the bytes to store
fn write_older_packed(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    value: &[u8],
) -> DbResult<ExtentRef> {
    if let Some(mut image) = shared_page_with_room(database, false, value.len())? {
        let page = database.shared_extent().unwrap_or(PageId::NONE);
        let slot = extent::shared::place(&mut image, value)?;
        log_shared_page(log, database, page, &mut image, false)?;
        return Ok(ExtentRef::packed(page, slot, value.len() as u64));
    }
    let page = database.allocate(1)?;
    let mut image = vec![0u8; database.page_size()];
    extent::shared::initialise(&mut image, tree_id)?;
    let slot = extent::shared::place(&mut image, value)?;
    log_shared_page(log, database, page, &mut image, true)?;
    database.set_shared_extent(Some(page));
    Ok(ExtentRef::packed(page, slot, value.len() as u64))
}

/// Puts one small value and its owner into a shared page in the owned layout.
///
/// @param database - the file the page is allocated and installed in
/// @param log - where the records go
/// @param tree_id - the tree the value belongs to
/// @param key - the owner key, already short enough to store
/// @param value - the bytes to store
fn write_owned_packed(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree_id: u64,
    key: &[u8],
    value: &[u8],
) -> DbResult<ExtentRef> {
    let wanted = extent::shared::owned_slot_len(key.len(), value.len());
    if let Some(mut image) = shared_page_with_room(database, true, wanted)? {
        let page = database.shared_extent().unwrap_or(PageId::NONE);
        let slot = extent::shared::place_owned(&mut image, tree_id, key, value)?;
        log_shared_page(log, database, page, &mut image, false)?;
        return Ok(ExtentRef::packed(page, slot, value.len() as u64));
    }
    let page = database.allocate(1)?;
    let mut image = vec![0u8; database.page_size()];
    extent::shared::initialise_owned(&mut image, tree_id)?;
    let slot = extent::shared::place_owned(&mut image, tree_id, key, value)?;
    log_shared_page(log, database, page, &mut image, true)?;
    database.set_shared_extent(Some(page));
    Ok(ExtentRef::packed(page, slot, value.len() as u64))
}

/// Returns the image of the shared page last written, when it can take more.
///
/// The page has to be in the layout the caller writes and have room for `wanted`
/// bytes of slot.
///
/// @param database - the file holding the hint
/// @param owned - whether the caller writes the owned layout
/// @param wanted - how many bytes the slot needs
fn shared_page_with_room(
    database: &Database,
    owned: bool,
    wanted: usize,
) -> DbResult<Option<Vec<u8>>> {
    let Some(page) = database.shared_extent() else {
        return Ok(None);
    };
    let guard = database.pool().fetch(page)?;
    let bytes = guard.bytes();
    let usable = extent::shared::is_shared(bytes)
        && extent::shared::is_owned(bytes) == owned
        && extent::shared::has_room(bytes, wanted)?;
    Ok(usable.then(|| bytes.to_vec()))
}

/// Logs and installs a shared extent page's whole image.
///
/// **The image, not the change.** A slot placed or cleared is a few bytes, but
/// the record has to be replayable against a page recovery may be holding at any
/// earlier state, and a whole image is the one form that always is - the same
/// argument `WritePage` already rests on. A shared page is written once per
/// value that lands on it, and a value that lands on it is at least four
/// kilobytes, so the log is at most a page per four kilobytes of payload.
///
/// @param log - where the records go
/// @param database - the file
/// @param page - the page's number
/// @param image - the page bytes, stamped with the record's LSN on return
/// @param fresh - whether the page was just allocated
pub(crate) fn log_shared_page(
    log: &mut dyn TreeLog,
    database: &mut Database,
    page: PageId,
    image: &mut [u8],
    fresh: bool,
) -> DbResult<()> {
    if fresh {
        log.log(Body::AllocPage { page: page.0 })?;
    }
    let lsn = log.log(Body::WritePage {
        page: page.0,
        image,
    })?;
    page::write_u64(image, page::header::LSN, lsn)?;
    database.install(page, image)
}

/// Describes one freshly allocated page in the log, then installs it.
///
/// Two records rather than one: the allocation and the contents are separate
/// facts and recovery needs both. `AllocPage` is what stops a later allocation
/// handing the same page out twice after a crash; `WritePage` is what puts the
/// bytes back. The image is stamped with the write's LSN before it is installed,
/// so the page-LSN rule holds for it exactly as it does for a page a split wrote.
///
/// With no log the image is installed unstamped, which is the byte for byte
/// behaviour the unlogged builder has always had.
///
/// **This is the out-of-line value run's path, and it is not the bulk builder's.**
/// A bulk build can write its pages past the log because it syncs the data file
/// before the statement that names its root commits; an overflow run written in
/// the middle of an ordinary statement has no such sync to hide behind, so its
/// bytes are in the log like any other page's.
///
/// @param log - where the records go, when there is one
/// @param database - the file the page is installed in
/// @param id - the page, already allocated
/// @param image - the page bytes, stamped in place with the LSN
pub(crate) fn log_allocated_page(
    log: &mut Option<&mut dyn TreeLog>,
    database: &mut Database,
    id: PageId,
    image: &mut [u8],
) -> DbResult<()> {
    if let Some(log) = log.as_mut() {
        log.log(Body::AllocPage { page: id.0 })?;
        let lsn = log.log(Body::WritePage { page: id.0, image })?;
        page::write_u64(image, page::header::LSN, lsn)?;
    }
    database.install(id, image)
}

/// Gives one out-of-line value's pages back to the free map.
///
/// A dead reference, which stands for a value already freed, gives back nothing.
///
/// @param database - the file
/// @param log - where the records go
/// @param reference - what the leaf held
pub fn free_extent(
    database: &mut Database,
    log: &mut dyn TreeLog,
    reference: ExtentRef,
) -> DbResult<()> {
    if reference.is_dead() {
        return Ok(());
    }
    // **A packed value gives back a slot, and the page only when it is the
    // last.** The bytes stay where they are: compacting the page would move a
    // value whose reference is in some other leaf.
    if let Some(slot) = reference.slot {
        return free_packed(database, log, reference.first, slot);
    }
    for page in chain_pages(database.pool(), reference)? {
        log.log(Body::FreePage { page: page.0 })?;
        database.release(page, 1)?;
    }
    Ok(())
}

/// Clears one slot of a shared page, and frees the page when it was the last.
///
/// @param database - the file
/// @param log - where the records go
/// @param page - the shared page
/// @param slot - the slot to clear
fn free_packed(
    database: &mut Database,
    log: &mut dyn TreeLog,
    page: PageId,
    slot: u16,
) -> DbResult<()> {
    let mut image = {
        let guard = database.pool().fetch(page)?;
        guard.bytes().to_vec()
    };
    let remaining = extent::shared::clear(&mut image, slot)?;
    if remaining > 0 {
        return log_shared_page(log, database, page, &mut image, false);
    }
    // Nothing on it is live, so it goes back to the free map - and the hint has
    // to let go of it first, or the next small value would be placed on a page
    // that is no longer this tree's.
    if database.shared_extent() == Some(page) {
        database.set_shared_extent(None);
    }
    log.log(Body::FreePage { page: page.0 })?;
    database.release(page, 1)
}

/// Returns the pages a value stored as a chain occupies, in chain order.
///
/// A chain in the owned layout is followed by each page's own link. A value in
/// the older layout, or whose first page is no longer an extent page, is the
/// run its length needs from its first page, which is what the older writer made.
///
/// @param pool - the buffer pool the file is open through
/// @param reference - a reference that is not packed
pub fn chain_pages(pool: &Pool, reference: ExtentRef) -> DbResult<Vec<PageId>> {
    let owned = {
        let guard = pool.fetch(reference.first)?;
        page::kind_of(guard.bytes())? == PageKind::BlobExtent
            && extent::owned::is_owned(guard.bytes())
    };
    if !owned {
        let pages = extent::pages_needed(reference.length, pool.page_size()).max(1);
        return Ok((0..pages)
            .map(|offset| PageId(reference.first.0.saturating_add(offset)))
            .collect());
    }
    let bound = extent::owned::chain_bound(reference.length, pool.page_size());
    let mut found = Vec::new();
    let mut page = reference.first;
    while !page.is_none() {
        let next = {
            let guard = pool.fetch(page)?;
            extent::owned::links(guard.bytes())?.next
        };
        found.push(page);
        if found.len() as u64 > bound {
            return Err(corrupt("an extent chain is longer than its length allows"));
        }
        page = next;
    }
    Ok(found)
}

impl crate::paged::PagedTree {
    /// Returns the owner key a value spilled from this row records: the row's key
    /// as this tree encodes it for a descent.
    ///
    /// @param row - the row's values, key columns first
    pub(crate) fn owner_key_of(&self, row: &[crate::datum::Datum<'_>]) -> Vec<u8> {
        self.encode_key(row.get(..self.key_columns()).unwrap_or(row))
    }
}
