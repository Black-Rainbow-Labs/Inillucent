//! How a checkpoint writes the meta record into its two slots, and which syncs
//! it takes.
//!
//! Invariant: **at every instant the disk holds one complete meta record that
//! describes pages the disk holds, or a rollback journal that puts back the
//! record and every page it describes.** Moved out of `fold.rs`, which carries
//! a line ceiling in `crates/inillucent-compat/tests/tooling/policy.rs`, when
//! the record took three routes instead of two (task-2191).

use super::*;

impl Pool {
    /// Writes a file being created: its pages, then both meta slots, then one
    /// sync. See `Database::finish_new_file`.
    ///
    /// @param meta - the record to write, with its generation already bumped
    pub(crate) fn finish_new_file(&self, meta: &mut Meta) -> DbResult<()> {
        self.hold_for_writing()?;
        self.flush()?;
        meta.high_water_lsn = meta.high_water_lsn.max(self.high_water_lsn.get());
        let mut image = vec![0u8; self.page_size];
        meta.encode(&mut image)?;
        self.write_both_slots(&image, true)?;
        self.sync_data_file()?;
        Counters::add(&self.counters.folds, 1);
        Counters::add(&self.counters.writes, 2);
        Ok(())
    }

    /// Answers which of the three ways a checkpoint writes its meta record
    /// applies.
    pub(super) fn meta_route(&self) -> MetaRoute {
        if self.fold_protected_by_log.get() {
            return MetaRoute::Logged;
        }
        let durable_journal = self
            .journal
            .borrow()
            .as_ref()
            .is_some_and(|journal| journal.mode().is_durable());
        match durable_journal {
            true => MetaRoute::OneSync,
            false => MetaRoute::PagesFirst,
        }
    }

    /// Writes the meta record into both slots, with the syncs its route needs.
    ///
    /// **The meta pages are journaled too, and they were the last pages that
    /// were not.** A rollback journal has to hold a pre-image of every page
    /// the checkpoint overwrites, and these two are pages the checkpoint
    /// overwrites. Leaving them out left a crash here able to produce a file
    /// whose data pages the journal put back to before the checkpoint and
    /// whose meta record says the checkpoint finished: the recorded
    /// `checkpoint_lsn` then tells redo that everything up to it is already
    /// in the file, so the records that would have re-applied the pages the
    /// journal just undid are skipped, and the database comes back as
    /// neither its old self nor its new one. It came back with no tables at
    /// all, because the catalog's own page is one of the pages the journal
    /// put back.
    ///
    /// The shadow page does not cover this **under a journal**. Both slots take
    /// the same image, so the second one is not an older copy to fall back on -
    /// it is a second chance for the *new* record to survive, and `Meta::choose`
    /// believing either of them is the failure. What makes the checkpoint
    /// undoable there is the previous record being on the disk in the journal,
    /// which is the same thing that makes every other page undoable.
    ///
    /// Without a journal the same two slots are what protects the record, and the
    /// sync between them is what makes them two states rather than one - which is
    /// the next paragraph.
    ///
    /// **The shadow slot goes first and takes the pages' own sync with it; the
    /// primary follows and takes the second.** Two syncs of the data file, and
    /// one complete meta record on the disk at every instant.
    ///
    /// **Why the order is what it is** (task-2000, design 1a). Without a
    /// rollback journal, nothing else can put the old meta record back, so the
    /// two slots may not be written into one unsynced batch: a crash there can
    /// tear both, and then neither decodes and the file does not open at all.
    /// Splitting the sync between them closes it:
    ///
    /// - a crash before the first sync leaves the primary holding the previous
    ///   record, whole, whatever the shadow's bytes look like. Recovery starts
    ///   from that record's own `checkpoint_lsn` and the after images the fold
    ///   appended before it wrote anything repair every page it had reached.
    /// - a crash between the syncs leaves the shadow holding this record,
    ///   whole, and `Meta::choose` takes it because its generation is higher.
    ///   The first sync covered the pages **and** the shadow together, so a
    ///   shadow that is durable cannot describe a page that is not.
    /// - a crash after the second leaves both, identical.
    ///
    /// **Writing to one slot a fold, alternating by generation, was tried first
    /// and is wrong with two processes.** It looks like the arrangement the two
    /// slots were shaped for, and it saves a write: the new record goes to the
    /// slot not holding the current one, so a torn write always leaves the other
    /// intact. What it misses is that each connection bumps *its own* copy of
    /// the generation. Two processes sharing a file can therefore write records
    /// of the same generation to the same slot, or leave one process's newer
    /// record in the slot `Meta::choose` does not pick - and then that process's
    /// fold is invisible: `checkpoint_lsn`, `page_count` and `free_map` all stay
    /// at the other process's older record, and `the_meta_moved` answers no
    /// because `choose` returns what the asking connection already had. Measured
    /// on two processes each inserting sixty rows into one `ATTACH`ed file: a
    /// hundred and twenty acknowledged, **one** in the file, `ANALYZE`
    /// afterwards reporting the table empty, and every process exit zero. Both
    /// slots taking the same image is what makes a meta record a fact about the
    /// file rather than about the connection that wrote it.
    ///
    /// Under a rollback journal the pre-images are what makes the write
    /// undoable, so both slots are journaled before they are written together.
    /// A journal on the disk takes them with the pages' pre-images, before the
    /// first page moves, which is [`MetaRoute::OneSync`].
    ///
    /// @param route - which of the three ways applies
    /// @param image - the encoded record, one page long
    pub(super) fn write_meta_record(&self, route: MetaRoute, image: &[u8]) -> DbResult<()> {
        match route {
            // The redo log protects the fold, so the split sync above is what
            // keeps one record whole. Three writes and two syncs of the data
            // file: the pages, the shadow slot, one sync; the primary slot, one
            // sync.
            MetaRoute::Logged => {
                self.write_slot_and_sync(SHADOW_PAGE, image)?;
                self.write_slot_and_sync(META_PAGE, image)
            }
            // **Every pre-image was saved and sealed before the first page
            // moved, the meta slots' included, so the pages and the meta record
            // take one sync between them (task-2191).** A crash anywhere before
            // the journal is disposed of puts back every page this fold wrote
            // and both slots, which is the database as it was before the fold,
            // and redo replays the log from the record those slots held. A crash
            // after the disposal finds the sync behind it. This used to sync the
            // pages, then journal and seal the two slots and sync again: two more
            // syncs a fold, for an order the journal already made irrelevant to
            // a crash. The fold that closes a database after a 10,000 row table
            // build and two indexes took 7.8 ms with them and 4.7 ms without,
            // the median of five runs of each. SQLite's rollback journal commits
            // the same way.
            //
            // **The shadow is written first (task-2181).** The journal
            // makes the order irrelevant to a crash of the machine, because it
            // puts both slots back. It is not irrelevant to another process:
            // a writer killed between the two writes leaves one slot changed in
            // the page cache, which every other process sees at once, and
            // `Pool::read_shadow_record` reads only the shadow on every
            // statement. With the primary first, that reader saw nothing and
            // went on without looking for the journal the dead writer left.
            MetaRoute::OneSync => {
                self.write_both_slots(image, false)?;
                self.sync_data_file()
            }
            // **Pages first, with a sync of their own, when nothing on the disk
            // can put the old record back**: no rollback journal at all, or one
            // that keeps its pre-images in memory. Merging the two syncs there
            // was tried and is wrong. With them merged, `inillucent-txn`'s
            // durability campaign crashed at write 25 of a checkpoint and the
            // database would not reopen, and `fault_shapes` recovered an empty
            // table from a misdirected write at cut 56 after 400 rows were
            // committed. A journal kept in memory still takes the slots'
            // pre-images, because a rollback in this process reads them.
            MetaRoute::PagesFirst => {
                self.sync_data_file()?;
                self.journal_page(META_PAGE)?;
                self.journal_page(SHADOW_PAGE)?;
                self.seal_journal()?;
                self.write_both_slots(image, false)?;
                self.sync_data_file()
            }
        }
    }

    /// Writes the meta record into the shadow slot and then the primary.
    ///
    /// **A checkpoint writes the record's first [`META_RECORD_BYTES`] bytes,
    /// not the page (task-2191).** `Meta::encode` writes zeros over the rest of
    /// the page, and every slot write before this one did too, so the bytes
    /// after the record on the disk are already the bytes the image holds there
    /// and the checksum over the whole page still matches. Writing 32 KiB to
    /// change 128 bytes cost a one row `exec` about 0.16 ms, two writes and the
    /// sync that carried them. A file being created gets whole pages: the
    /// slots are the first thing in it, and a file holding only the record of
    /// the shadow slot would be too short to read back two whole pages. An
    /// encrypted file reads, changes and writes the unit, which is correct and
    /// costs more than the whole page would.
    ///
    /// @param image - the encoded record, one page long
    /// @param whole - whether to write the whole page, for a file being created
    fn write_both_slots(&self, image: &[u8], whole: bool) -> DbResult<()> {
        let written = match whole {
            true => image,
            false => image.get(..crate::meta::META_RECORD_BYTES).unwrap_or(image),
        };
        for slot in [SHADOW_PAGE, META_PAGE] {
            self.file
                .write_all_at(slot.0.saturating_mul(self.page_size as u64), written)
                .map_err(|error| error.into_db_error())?;
        }
        Ok(())
    }
}

/// How a checkpoint writes its meta record; see `Pool::write_meta_record`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MetaRoute {
    /// The redo log holds an after image of every page the fold writes.
    Logged,
    /// A rollback journal on the disk holds the pre-images of the pages and of
    /// both meta slots.
    OneSync,
    /// Nothing on the disk can put the old record back.
    PagesFirst,
}
