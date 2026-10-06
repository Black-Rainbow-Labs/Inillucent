//! Dirty pages evicted while this connection may not write the database file.
//!
//! Invariant: **a page is resident, spilled or neither, and never two of them at
//! once.** A fetch reads a spilled page back from here before it would read the
//! file, so a stale copy in the file is never seen while a newer one is spilled.
//!
//! ## Why a spill file exists
//!
//! A write transaction holds RESERVED, which readers in other processes share, so
//! they keep reading the last committed state while it runs. The database file is
//! what they read. Before this file existed, a transaction whose dirty pages
//! outgrew the buffer pool evicted them **into the database file**, saving their
//! old images in `<database>-journal`, and writing the database file needs the
//! EXCLUSIVE lock. The writer kept that lock until it committed, so every reader
//! in another process waited for the whole rest of the transaction and then failed
//! with busy. Measured on 2.0.4: an `UPDATE` of 20,000 rows of 3 KB values in one
//! transaction blocked a reader that had a three second busy timeout, every time.
//!
//! A connection that does not hold EXCLUSIVE now writes an evicted dirty page here
//! instead. This file belongs to this connection alone: it is opened through the
//! database's own file system as a transient file, which is deleted when it is
//! closed and when the process dies, and which an encrypting file system encrypts
//! with a key of its own. Nothing in it has to survive a crash, because every
//! change a spilled page holds is described by a record in the log, and a page of
//! a transaction that never committed is exactly what a crash is meant to lose. So
//! a spill needs no lock, no log sync before it and no journal.
//!
//! The pages reach the database file the way every other dirty page does: a fold,
//! under EXCLUSIVE, writes them with the rest (`Pool::flush`).
//!
//! The same path serves a reader. A connection holding SHARED that replays a log
//! larger than its pool used to raise its lock to EXCLUSIVE in order to evict, and
//! a read only connection kept such pages resident past its budget. Both now spill.

use super::*;

/// Where one spilled page is, and what the pool knew about it when it left.
#[derive(Clone, Copy, Debug)]
pub(super) struct Slot {
    /// The byte offset of the page image in the spill file.
    offset: u64,
    /// The LSN of the change that first dirtied the page since the file last
    /// held it, carried so a page read back counts from the same point.
    rec_lsn: u64,
    /// The LSN stamped in the page's own header, which is what decides whether
    /// the page holds a change no transaction has committed.
    lsn: u64,
}

impl Slot {
    /// Returns the LSN the page was dirty from when it was spilled.
    pub(super) fn rec_lsn(&self) -> u64 {
        self.rec_lsn
    }
}

/// The spill file and the pages in it.
pub(super) struct Spill {
    /// The transient file, opened on first use.
    file: Box<dyn VfsFile>,
    /// Which page is where.
    slots: HashMap<PageId, Slot, PageHashing>,
    /// Offsets whose page has been read back, for the next page to reuse.
    free: Vec<u64>,
    /// Where the next new offset is.
    end: u64,
}

/// What opens a spill file. The engine registers it, because the pool holds a
/// file handle and not the file system the handle came from.
pub type SpillOpener = std::rc::Rc<dyn Fn() -> DbResult<Box<dyn VfsFile>>>;

impl Pool {
    /// Registers what the pool calls to open its spill file.
    ///
    /// A pool with nothing registered writes an evicted dirty page into the
    /// database file, raising its lock to EXCLUSIVE first, as it always did.
    ///
    /// @param open - opens a new transient file on the database's file system
    pub fn on_spill(&self, open: SpillOpener) {
        *self.spill_opener.borrow_mut() = Some(open);
    }

    /// Returns how many pages are in the spill file.
    pub fn spilled_pages(&self) -> usize {
        self.spill
            .borrow()
            .as_ref()
            .map_or(0, |spill| spill.slots.len())
    }

    /// Reports whether an eviction should spill rather than write the file.
    ///
    /// Only a connection that holds the file but not exclusively spills. One
    /// holding EXCLUSIVE may write the file and does, with its journal, as before,
    /// except for a page an open transaction has changed: see
    /// [`Pool::spills_an_open_transaction`]. One holding no lock at all is a file
    /// nobody else can reach, which is a file being created or one opened without
    /// the locking protocol.
    pub(super) fn should_spill(&self) -> bool {
        if self.spill_opener.borrow().is_none() {
            return false;
        }
        matches!(
            self.file.lock_level(),
            FileLock::Shared | FileLock::Reserved | FileLock::Pending
        )
    }

    /// Reports whether an eviction under EXCLUSIVE should spill a page because
    /// it holds a change no transaction has committed.
    ///
    /// **Such a page in the data file can be read by another process.** Writing
    /// it there was a steal, made safe against a crash by its old image in the
    /// journal. It was not safe against the other processes. When the writer is
    /// killed, its locks go with it, and the journal is put back only by a
    /// process that can take RESERVED. A live writer holding RESERVED stops that,
    /// and every process that opens or reads in the meantime reads the file as
    /// the dead writer left it, rows of a transaction that will never commit
    /// included. The spill file is this connection's own and dies with it, so a
    /// page in it is never seen by anybody else. The fold that follows a commit
    /// writes it, as it writes any spilled page.
    ///
    /// A committed page still goes to the file. Another process reading it there
    /// reads what the log already says, and `journal::announce_the_restore` makes
    /// every connection read again when a journal puts the old image back.
    ///
    /// @param frame - the frame being evicted
    /// @param page - the page it holds
    pub(super) fn spills_an_open_transaction(&self, frame: u32, page: PageId) -> DbResult<bool> {
        if self.spill_opener.borrow().is_none() || self.file.lock_level() != FileLock::Exclusive {
            return Ok(false);
        }
        self.holds_uncommitted(frame, page)
    }

    /// Writes one dirty frame to the spill file and marks the frame clean.
    ///
    /// The image is translated exactly as a writeback translates it, because the
    /// frame's swizzled swips name frames that will hold other pages by the time
    /// this one is read back.
    ///
    /// @param frame - the frame being evicted
    /// @param page - the page it holds
    pub(super) fn spill_frame(&self, frame: u32, page: PageId) -> DbResult<()> {
        let rec_lsn = self
            .state
            .borrow()
            .frames
            .get(frame as usize)
            .map_or(u64::MAX, |meta| meta.rec_lsn);
        let mut image = self
            .scratch
            .try_borrow_mut()
            .map_err(|_| misuse("the pool's writeback scratch is already in use"))?;
        {
            let bytes = self
                .buffers
                .get(frame as usize)
                .ok_or_else(|| misuse("frame index out of range"))?
                .try_borrow()
                .map_err(|_| misuse("a frame chosen for spilling was mutably borrowed"))?;
            if image.len() != bytes.len() {
                image.resize(bytes.len(), 0);
            }
            image.copy_from_slice(&bytes);
        }
        let image = &mut *image;
        let translated = self.translate_swips(image)?;
        let lsn = page::read_u64(image, page::header::LSN)?;
        page::checksum_page(image)?;
        self.put_in_spill(
            page,
            image,
            Slot {
                offset: 0,
                rec_lsn,
                lsn,
            },
        )?;
        let mut state = self.state.borrow_mut();
        state.amend(frame, |meta| {
            meta.dirty = false;
            meta.rec_lsn = u64::MAX;
        });
        drop(state);
        Counters::add(&self.counters.translated, translated as u64);
        Ok(())
    }

    /// Stores one page image in the spill file, opening the file on first use.
    ///
    /// @param page - the page
    /// @param image - its bytes, translated and checksummed
    /// @param slot - its bookkeeping; the offset is chosen here
    fn put_in_spill(&self, page: PageId, image: &[u8], slot: Slot) -> DbResult<()> {
        let mut held = self.spill.borrow_mut();
        if held.is_none() {
            let opener = self
                .spill_opener
                .borrow()
                .as_ref()
                .map(std::rc::Rc::clone)
                .ok_or_else(|| misuse("a page was spilled with no spill file registered"))?;
            *held = Some(Spill {
                file: opener()?,
                slots: HashMap::default(),
                free: Vec::new(),
                end: 0,
            });
        }
        let spill = held
            .as_mut()
            .ok_or_else(|| misuse("the spill file did not open"))?;
        let offset = match spill.slots.get(&page) {
            Some(existing) => existing.offset,
            None => match spill.free.pop() {
                Some(offset) => offset,
                None => {
                    let offset = spill.end;
                    spill.end = spill.end.saturating_add(self.page_size as u64);
                    offset
                }
            },
        };
        spill
            .file
            .write_all_at(offset, image)
            .map_err(|error| error.into_db_error())?;
        spill.slots.insert(page, Slot { offset, ..slot });
        Ok(())
    }

    /// Reads a spilled page into a claimed frame, if the page is spilled.
    ///
    /// Returns the slot it came from, so the caller can mark the frame dirty from
    /// the same point the page was dirty from when it left. The slot is released
    /// only once the bytes are in the frame and their checksum holds, so a read
    /// that fails leaves the page where it was.
    ///
    /// @param frame - the claimed frame
    /// @param page - the page wanted
    pub(super) fn fill_from_spill(&self, frame: u32, page: PageId) -> DbResult<Option<Slot>> {
        let mut held = self.spill.borrow_mut();
        let Some(spill) = held.as_mut() else {
            return Ok(None);
        };
        let Some(slot) = spill.slots.get(&page).copied() else {
            return Ok(None);
        };
        {
            let mut bytes = self.frame_for_a_read(frame)?;
            spill
                .file
                .read_exact_into(slot.offset, &mut bytes, self.page_size)
                .map_err(|error| error.into_db_error())?;
            page::verify_checksum(&bytes, page)?;
        }
        spill.slots.remove(&page);
        spill.free.push(slot.offset);
        Ok(Some(slot))
    }

    /// Forgets a spilled page, because something newer replaces it.
    ///
    /// Asked wherever a page enters the pool or the file other than by a fetch:
    /// an install puts a whole new image in a frame, and a bulk build writes one
    /// straight into the file. A spilled copy left behind would be read back by
    /// the next fetch in place of either.
    ///
    /// @param page - the page
    pub(super) fn forget_spilled(&self, page: PageId) {
        let mut held = self.spill.borrow_mut();
        let Some(spill) = held.as_mut() else {
            return;
        };
        if let Some(slot) = spill.slots.remove(&page) {
            spill.free.push(slot.offset);
        }
    }

    /// Drops every spilled page.
    ///
    /// What a connection does when it throws its cache away: every change a
    /// spilled page holds is in the log, which the caller replays next. The file
    /// is kept open for the next spill and its space reused.
    pub(super) fn forget_every_spilled_page(&self) {
        let mut held = self.spill.borrow_mut();
        if let Some(spill) = held.as_mut() {
            spill.slots.clear();
            spill.free.clear();
            spill.end = 0;
        }
    }

    /// Returns the spilled pages a fold may write, in page order, with their
    /// slots.
    ///
    /// A page holding a change no transaction has committed is left out, by the
    /// same test [`Pool::holds_uncommitted`] makes of a resident frame.
    pub(super) fn spilled_for_fold(&self) -> Vec<(PageId, Slot)> {
        let uncommitted = self.uncommitted_lsn.load(Ordering::SeqCst);
        let held = self.spill.borrow();
        let Some(spill) = held.as_ref() else {
            return Vec::new();
        };
        let mut pages: Vec<(PageId, Slot)> = spill
            .slots
            .iter()
            .filter(|(_, slot)| uncommitted == u64::MAX || slot.lsn < uncommitted)
            .map(|(page, slot)| (*page, *slot))
            .collect();
        pages.sort_unstable_by_key(|(page, _)| *page);
        pages
    }

    /// Returns the lowest `rec_lsn` among spilled pages a fold will hold back.
    ///
    /// The spilled half of [`Pool::oldest_dirty_lsn`].
    pub(super) fn oldest_spilled_uncommitted_lsn(&self) -> u64 {
        let uncommitted = self.uncommitted_lsn.load(Ordering::SeqCst);
        if uncommitted == u64::MAX {
            return u64::MAX;
        }
        let held = self.spill.borrow();
        let Some(spill) = held.as_ref() else {
            return u64::MAX;
        };
        spill
            .slots
            .values()
            .filter(|slot| slot.lsn >= uncommitted)
            .map(|slot| slot.rec_lsn)
            .min()
            .unwrap_or(u64::MAX)
    }

    /// Reads one spilled page's image into `image`.
    ///
    /// @param slot - where it is
    /// @param image - a buffer one page long
    pub(super) fn read_spilled(&self, slot: &Slot, image: &mut [u8]) -> DbResult<()> {
        let held = self.spill.borrow();
        let spill = held
            .as_ref()
            .ok_or_else(|| misuse("a spilled page was read with no spill file open"))?;
        spill
            .file
            .read_exact_at(slot.offset, image)
            .map_err(|error| error.into_db_error())
    }

    /// Writes one spilled page into the database file, and forgets it.
    ///
    /// The fold's path for a page that is not resident: the same write ahead
    /// check, the same lock, the same pre image in the journal and the same high
    /// water as [`Pool::writeback`] applies to a frame.
    ///
    /// @param page - the page
    /// @param slot - where it is in the spill file
    pub(super) fn write_spilled(&self, page: PageId, slot: &Slot) -> DbResult<bool> {
        self.refuse_an_image_ahead_of_the_log(slot.lsn, page)?;
        if !self.may_write_back()? {
            return Ok(false);
        }
        let mut image = self
            .scratch
            .try_borrow_mut()
            .map_err(|_| misuse("the pool's writeback scratch is already in use"))?;
        if image.len() != self.page_size {
            image.resize(self.page_size, 0);
        }
        self.read_spilled(slot, &mut image)?;
        self.high_water_lsn
            .set(self.high_water_lsn.get().max(slot.lsn));
        let ask_the_journal = !self.fold_protected_by_log.get();
        if ask_the_journal && self.journal_page(page)? {
            self.seal_journal()?;
        }
        self.file
            .write_all_at(page.0.saturating_mul(self.page_size as u64), &image)
            .map_err(|error| error.into_db_error())?;
        drop(image);
        self.forget_spilled(page);
        Counters::add(&self.counters.writes, 1);
        Ok(true)
    }
}
