//! What a write does to the FTS5 index.
//!
//! Invariant: **a row's postings are written under the same lock its content
//! row is.** `add` and `remove` stage into the pending buffer and the buffer is
//! flushed as one, so a reader never sees a term whose document is not there
//! yet, or a document whose terms are gone.

use std::collections::BTreeMap;

use inillucent_base::DbResult;
use inillucent_value::Value;

use super::super::{failure, Context, VirtualTable};

use super::doclist::{decode_doclist, encode_doclist, write_varint, DocEntry};
use super::index::*;
use super::*;

/// Returns whether a command is `word=value`, ignoring case.
///
/// @param command - the text written to the table's own column
/// @param word - the command name before the equals sign
fn is_command_with_value(command: &str, word: &str) -> bool {
    command
        .split_once('=')
        .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(word))
}

/// Returns a command argument as a whole number above zero, or nothing.
///
/// @param value - the argument of the command
fn positive_integer(value: &Value<'static>) -> Option<i64> {
    let number = match value {
        Value::Integer(number) => Some(*number),
        Value::Text(text) => String::from_utf8_lossy(&text.utf8_bytes())
            .trim()
            .parse::<i64>()
            .ok(),
        _ => None,
    };
    number.filter(|number| *number > 0)
}

impl Fts5Table {
    /// Returns the error for a write that names a rowid the index already holds.
    ///
    /// FTS3, FTS4 and FTS5 all report the message SQLite does, `constraint
    /// failed`.
    pub(super) fn rowid_taken(&self) -> inillucent_base::DbError {
        constraint("constraint failed")
    }

    /// Returns the row number an FTS3 or FTS4 write names with `docid`.
    ///
    /// `docid` is the hidden column in the slot `rank` has in FTS5, and it is
    /// another name for the rowid: `INSERT INTO t(docid, a)` stores the row
    /// under that number and `UPDATE t SET docid = n` moves it. Both were
    /// ignored, so every join on `docid` after an insert by `docid` (which is
    /// what Android Room's sync triggers do) found nothing. An FTS5 table has no
    /// such column, so it answers nothing there.
    ///
    /// @param values - the declared columns of the row being written
    pub(super) fn named_docid(&self, values: &[Value<'static>]) -> Option<i64> {
        if self.dialect != Dialect::Three {
            return None;
        }
        values
            .get(self.rank_column() as usize)
            .and_then(Value::as_integer)
    }

    /// Runs one of the module's special commands.
    ///
    /// `integrity-check` and `rebuild` do what they say. The merge family -
    /// `optimize`, `merge`, `automerge`, `crisismerge`, `usermerge`, `pgsz` -
    /// is accepted and does nothing, because this index keeps one doclist per
    /// term rather than a stack of segments to merge: there is no work for them
    /// to ask for. They are accepted rather than refused so that an application
    /// written against SQLite runs unchanged, and they are listed here rather
    /// than ignored silently so the next reader knows the difference is
    /// deliberate.
    pub(crate) fn command(
        &mut self,
        context: &mut Context<'_>,
        command: &[u8],
        argument: Option<Value<'static>>,
    ) -> DbResult<()> {
        let name = String::from_utf8_lossy(command).into_owned();
        let argument = argument.filter(|value| !matches!(value, Value::Null));
        match name.as_str() {
            "integrity-check" => {
                if let Some(problem) = self.integrity(context)? {
                    return Err(failure(problem));
                }
                // SQLite compares the index with the content for a table that
                // owns its rows, and for an external content table only when
                // the command's `rank` argument is not zero. A contentless
                // table has nothing to compare with.
                let asked = argument.as_ref().and_then(Value::as_integer).unwrap_or(0) != 0;
                let compares = !self.contentless && (self.external.is_none() || asked);
                if compares && !self.content_matches_index(context)? {
                    return Err(failure(format!(
                        "fts5: checksum mismatch for table \"{}\"",
                        String::from_utf8_lossy(&self.name)
                    )));
                }
                Ok(())
            }
            "rebuild" => self.rebuild(context),
            // There is nothing to merge or flush: one doclist per term is the
            // whole index, so the segment machinery these ask about does not
            // exist here. They are accepted rather than refused so that an
            // application written against SQLite runs unchanged.
            "optimize" | "flush" => Ok(()),
            // **FTS4 writes these two with the value inside the command.**
            // `merge=2,4` and `automerge=8` only tune how segments are merged,
            // and this index has none, so `merge` is accepted and `automerge` is
            // kept as a setting the way FTS5's is.
            _ if self.dialect == Dialect::Three && is_command_with_value(&name, "merge") => Ok(()),
            _ if self.dialect == Dialect::Three && is_command_with_value(&name, "automerge") => {
                let value = name
                    .split_once('=')
                    .map(|(_, value)| value.trim())
                    .unwrap_or("");
                let value = value
                    .parse::<i64>()
                    .map(Value::Integer)
                    .unwrap_or(Value::Null);
                self.shadows.write_keyed(
                    context,
                    b"config",
                    1,
                    &[Value::owned_text(b"automerge")?, value],
                )
            }
            "merge" => match argument {
                Some(_) => Ok(()),
                None => Err(unrecognized()),
            },
            // The settings *are* kept, because `%_config` is a table an
            // application reads. Nothing here acts on them - see above - but a
            // value that was written and cannot be read back would be a
            // difference a reader can see.
            "automerge" | "crisismerge" | "deletemerge" | "pgsz" | "rank" | "secure-delete"
            | "usermerge" | "hashsize" => {
                let Some(value) = argument else {
                    return Err(unrecognized());
                };
                // `hashsize` is how many bytes of changes SQLite holds in memory
                // before it writes them out. This index has its own buffer, so
                // the number changes nothing here, but SQLite refuses a value
                // that is not a positive integer and so does this.
                // A number given as text is stored as the number, which is
                // what SQLite's check leaves in `%_config`.
                let value = match (name.as_str(), positive_integer(&value)) {
                    ("hashsize", None) => return Err(unrecognized()),
                    ("hashsize", Some(number)) => Value::Integer(number),
                    _ => value,
                };
                self.shadows.write_keyed(
                    context,
                    b"config",
                    1,
                    &[Value::owned_text(name.as_bytes())?, value],
                )
            }
            // **Answered rather than always refused (task-1979, R5).** The
            // message named the two kinds of table the command is for and the
            // code never checked whether this was one of them, so the command
            // was refused on exactly the tables SQLite accepts it on.
            "delete-all" if self.stores_no_rows() => self.delete_all(context),
            "delete-all" => Err(failure(
                "'delete-all' may only be used with a contentless or external content fts5 table",
            )),
            _ => Err(unrecognized()),
        }
    }

    /// Throws the index away and builds it again from the content.
    ///
    /// The content table is the truth: it holds the rows exactly as they were
    /// inserted, and everything else - the doclists, the term dictionary, the
    /// sizes, the totals - is derived from it. That is what makes a rebuild
    /// possible at all, and what makes it the repair for an index that has
    /// drifted.
    fn rebuild(&mut self, context: &mut Context<'_>) -> DbResult<()> {
        // **A contentless table has nothing to rebuild from**, which is
        // SQLite's own refusal word for word: the document text was never
        // stored, so there is no truth to derive the index from again.
        if self.contentless {
            return Err(failure(
                "'rebuild' may not be used with a contentless fts5 table",
            ));
        }
        // Every row is about to go, staged ones included: a doclist left in
        // the buffer would be written back after the wipe. This is also the
        // one place every remaining `%_data` term row - the ones an older
        // build left and nothing has touched since - is cleared in one pass,
        // because everything `rebuild` writes from here on goes through
        // `%_idx` directly.
        if let Ok(mut held) = self.pending.lock() {
            held.doclists.clear();
            held.bytes = 0;
        }
        let rows = self.content_rows(context)?;
        self.wipe_index(context)?;
        for (rowid, values) in rows {
            self.add(context, rowid, &values)?;
        }
        Ok(())
    }

    /// Reads every row of the content, as the declared columns in order.
    ///
    /// For an external content table these are the owner's rows, found by the
    /// column names the full-text table declares.
    ///
    /// @param context - the running statement
    fn content_rows(
        &mut self,
        context: &mut Context<'_>,
    ) -> DbResult<Vec<(i64, Vec<Value<'static>>)>> {
        let offsets = self.offsets(context);
        let mut rows: Vec<(i64, Vec<Value<'static>>)> = Vec::new();
        let suffix = self.content.clone();
        self.shadows.scan(context, &suffix, |rowid, values| {
            rows.push((
                rowid,
                offsets
                    .iter()
                    .map(|at| values.get(*at).cloned().unwrap_or(Value::Null))
                    .collect(),
            ));
            Ok(true)
        })?;
        Ok(rows)
    }

    /// Reports whether the index holds exactly the postings the content gives.
    ///
    /// This is the check `integrity-check` makes against the content, which
    /// SQLite does for a table that owns its rows and, for an external content
    /// table, when the command's `rank` argument is not zero. Every row of the
    /// content is tokenised the way `add` does and the postings are compared
    /// with what the term dictionary holds, so a changed row, a row the index
    /// never saw and a row the index still has after the content lost it are
    /// all mismatches.
    ///
    /// @param context - the running statement
    fn content_matches_index(&mut self, context: &mut Context<'_>) -> DbResult<bool> {
        flush_doclists(context, &self.shadows, &self.pending)?;
        let mut expected: BTreeMap<Vec<u8>, BTreeMap<i64, Vec<(usize, Vec<u32>)>>> =
            BTreeMap::new();
        for (rowid, values) in self.content_rows(context)? {
            for (term, columns) in self.postings_of(&values) {
                expected.entry(term).or_default().insert(rowid, columns);
            }
        }
        let mut stored: Vec<(Vec<u8>, Vec<Value<'static>>)> = Vec::new();
        self.shadows.scan_keyed(context, b"idx", 2, |values| {
            if let Some(term) = values.get(1).and_then(Value::as_blob) {
                stored.push((term.raw().to_vec(), values.to_vec()));
            }
            Ok(true)
        })?;
        let mut actual: BTreeMap<Vec<u8>, BTreeMap<i64, Vec<(usize, Vec<u32>)>>> = BTreeMap::new();
        for (term, row) in stored {
            let Some(bytes) = resolve_doclist(context, &self.shadows, &row)? else {
                return Ok(false);
            };
            let entries = decode_doclist(&bytes)
                .into_iter()
                .map(|entry| (entry.rowid, entry.columns))
                .collect();
            actual.insert(term, entries);
        }
        Ok(expected == actual)
    }

    /// Returns where each term occurs in one row, as `add` would index it.
    ///
    /// @param values - the row's declared columns in order
    fn postings_of(&self, values: &[Value<'static>]) -> BTreeMap<Vec<u8>, Vec<(usize, Vec<u32>)>> {
        let mut postings: Vec<(Vec<u8>, usize, u32)> = Vec::new();
        for (index, column) in self.options.columns.iter().enumerate() {
            if column.unindexed {
                continue;
            }
            let Some(text) = values.get(index).and_then(text_of) else {
                continue;
            };
            for (position, token) in self.tokenizer.tokens(&text).into_iter().enumerate() {
                postings.push((token, index, position as u32));
            }
        }
        postings.sort_unstable();
        let mut terms: BTreeMap<Vec<u8>, Vec<(usize, Vec<u32>)>> = BTreeMap::new();
        for (term, column, position) in postings {
            let columns = terms.entry(term).or_default();
            match columns.last_mut() {
                Some((held, positions)) if *held == column => positions.push(position),
                _ => columns.push((column, vec![position])),
            }
        }
        terms
    }

    /// Empties an index that stores no document text of its own.
    ///
    /// **`delete-all`, which was refused on every table (task-1979, R5).** It
    /// is the only way to empty a contentless table, because a row cannot be
    /// deleted one at a time without the text that produced its terms. An
    /// external content table takes it for the same reason and then rebuilds
    /// from the owner.
    ///
    /// @param context - the running statement
    pub(crate) fn delete_all(&mut self, context: &mut Context<'_>) -> DbResult<()> {
        if let Ok(mut held) = self.pending.lock() {
            held.doclists.clear();
            held.bytes = 0;
        }
        self.wipe_index(context)
    }

    /// Removes every doclist, every dictionary row, every size and the totals.
    ///
    /// The body `rebuild` and `delete_all` share; the difference between the
    /// two is only what is put back afterwards.
    ///
    /// @param context - the running statement
    fn wipe_index(&mut self, context: &mut Context<'_>) -> DbResult<()> {
        let width = self.options.columns.len();
        let mut doclists = Vec::new();
        self.shadows.scan(context, b"data", |rowid, _| {
            // The totals row and the layout record are the index's own; every
            // other `%_data` row is a doclist an older build left behind, and
            // those are what a wipe is for. The layout record survives because
            // the index it describes is about to be written again by this
            // build, in this build's layout - see `super::layout`.
            if rowid != TOTALS && !layout::is_layout_row(rowid) {
                doclists.push(rowid);
            }
            Ok(true)
        })?;
        for rowid in doclists {
            self.shadows.delete_row(context, b"data", rowid)?;
        }
        // The staged dictionary rows go out first, so the scan below sees
        // every term there is - a delete-all that missed one would leave it in
        // `%_idx` naming a `%_data` row that has been removed.
        flush_doclists(context, &self.shadows, &self.pending)?;
        let mut terms = Vec::new();
        self.shadows.scan_keyed(context, b"idx", 2, |values| {
            if let Some(term) = values.get(1).and_then(Value::as_blob) {
                terms.push(term.raw().to_vec());
            }
            Ok(true)
        })?;
        for term in terms {
            self.shadows.delete_keyed(
                context,
                b"idx",
                &[Value::Integer(SEGMENT), Value::owned_blob(&term)?],
            )?;
        }
        let mut sizes = Vec::new();
        self.shadows.scan(context, b"docsize", |rowid, _| {
            sizes.push(rowid);
            Ok(true)
        })?;
        for rowid in sizes {
            self.shadows.delete_row(context, b"docsize", rowid)?;
        }
        // **Stamped here as well as at `CREATE`, because this is the other
        // place the whole index is written from scratch.** An index an older
        // build wrote has no layout record; `rebuild` reads every row out of
        // `%_content` and writes the dictionary again from nothing, so after
        // it the claim the record makes is true of every row - which is
        // exactly the claim an ordinary insert cannot make. It is also how an
        // application gets the record onto a file that predates it.
        layout::stamp(context, &self.shadows)?;
        // **The buffered totals are reset along with the stored ones.** The
        // buffer keeps the totals it last read for the life of the connection,
        // and `add` counts on top of that copy. Resetting only the stored row
        // let each `rebuild` add the table's rows and tokens to the totals it
        // had already counted, so `bm25` scored differently after every rebuild.
        stage_totals(&self.pending, Totals::empty(width));
        put_totals(context, &self.shadows, &Totals::empty(width))
    }

    /// Adds one row to the content and to the index.
    pub(crate) fn add(
        &mut self,
        context: &mut Context<'_>,
        rowid: i64,
        values: &[Value<'static>],
    ) -> DbResult<()> {
        let whole_started = std::time::Instant::now();
        let width = self.options.columns.len();
        let started = std::time::Instant::now();
        // **Nothing is stored when the rows are somebody else's.** An
        // external content table's rows are written through the owner, and a
        // copy written here would be a second, diverging one.
        if !self.stores_no_rows() {
            let mut content = vec![Value::Null];
            for index in 0..width {
                content.push(values.get(index).cloned().unwrap_or(Value::Null));
            }
            self.shadows
                .write_row(context, b"content", rowid, &content)?;
        }
        let content_ns = started.elapsed().as_nanos();

        let started = std::time::Instant::now();
        let mut sizes = vec![0i64; width];
        let mut postings: Vec<(Vec<u8>, usize, u32)> = Vec::new();
        for (index, column) in self.options.columns.iter().enumerate() {
            if column.unindexed {
                continue;
            }
            let Some(text) = values.get(index).and_then(text_of) else {
                continue;
            };
            for (position, token) in self.tokenizer.tokens(&text).into_iter().enumerate() {
                if let Some(size) = sizes.get_mut(index) {
                    *size = size.saturating_add(1);
                }
                postings.push((token, index, position as u32));
            }
        }
        let tokenize_ns = started.elapsed().as_nanos();

        let started = std::time::Instant::now();
        let mut encoded = Vec::new();
        for size in &sizes {
            write_varint(&mut encoded, *size as u64);
        }
        self.shadows.write_row(
            context,
            b"docsize",
            rowid,
            &[Value::Null, Value::owned_blob(&encoded)?],
        )?;
        let docsize_ns = started.elapsed().as_nanos();

        let started = std::time::Instant::now();
        let mut terms_ns = 0u128;
        postings.sort_unstable();
        let mut at = 0usize;
        while at < postings.len() {
            // **The run of postings for one term, as a slice rather than a
            // copy.** They are sorted by term, then column, then position, so
            // one term's occurrences are contiguous and already in the order a
            // doclist entry writes them - which is what lets the append below
            // read them in place. Building a `DocEntry` here allocated a
            // `Vec<(usize, Vec<u32>)>` per term per document, and a term that
            // occurs once is two allocations to describe one number.
            let start = at;
            let Some((term, _, _)) = postings.get(at) else {
                break;
            };
            while matches!(postings.get(at), Some((candidate, _, _)) if candidate == term) {
                at = at.saturating_add(1);
            }
            let merged = std::time::Instant::now();
            let run = postings.get(start..at).unwrap_or_default();
            self.merge_postings(context, rowid, run)?;
            terms_ns = terms_ns.saturating_add(merged.elapsed().as_nanos());
        }
        let group_ns = started.elapsed().as_nanos().saturating_sub(terms_ns);

        let started = std::time::Instant::now();
        let mut totals = buffered_totals(context, &self.shadows, &self.pending, width);
        totals.rows = totals.rows.saturating_add(1);
        totals.tokens.resize(width, 0);
        for (index, size) in sizes.iter().enumerate() {
            if let Some(total) = totals.tokens.get_mut(index) {
                *total = total.saturating_add(*size);
            }
        }
        stage_totals(&self.pending, totals);
        let totals_ns = started.elapsed().as_nanos();

        record_stage(|stages| {
            stages.rows = stages.rows.saturating_add(1);
            stages.whole = stages
                .whole
                .saturating_add(whole_started.elapsed().as_nanos());
            stages.content = stages.content.saturating_add(content_ns);
            stages.tokenize = stages.tokenize.saturating_add(tokenize_ns);
            stages.docsize = stages.docsize.saturating_add(docsize_ns);
            stages.group = stages.group.saturating_add(group_ns);
            stages.terms = stages.terms.saturating_add(terms_ns);
            stages.totals = stages.totals.saturating_add(totals_ns);
        });
        Ok(())
    }

    /// Adds one document's occurrences of one term to that term's doclist.
    ///
    /// **The bulk-load path takes one lock and allocates nothing.** It used to
    /// take three - `term_row`, `append_staged` and `buffer_is_full` each
    /// locked the buffer - and build a `Vec<(usize, Vec<u32>)>` to describe the
    /// occurrences before writing them out as varints. Neither is much on its
    /// own and both are per term per document: a five-hundred-document build
    /// over a ten-term vocabulary is fifteen thousand lock/unlock pairs and ten
    /// thousand allocations, and `extension.fts.build` spent 3.5 of its 8.7 ms
    /// in here.
    ///
    /// Everything the fast path needs is in the buffer, so it is one critical
    /// section: find the term's page, append the entry to the staged doclist,
    /// and read back whether the buffer is now full. The flush - which needs
    /// the host and cannot be done under the lock - happens after it.
    ///
    /// @param context - the host
    /// @param rowid - the document being indexed
    /// @param run - this term's postings, sorted by column then position
    fn merge_postings(
        &self,
        context: &mut Context<'_>,
        rowid: i64,
        run: &[(Vec<u8>, usize, u32)],
    ) -> DbResult<()> {
        let Some((term, _, _)) = run.first() else {
            return Ok(());
        };
        match append_in_one_lock(&self.pending, term, rowid, run) {
            Appended::Done { full } => {
                if full {
                    flush_doclists(context, &self.shadows, &self.pending)?;
                }
                return Ok(());
            }
            Appended::No => {}
        }
        let term = term.clone();
        let entry = DocEntry {
            rowid,
            columns: columns_of(run),
        };
        let started = std::time::Instant::now();
        let answer = self.merge_term(context, &term, entry);
        let spent = started.elapsed().as_nanos();
        record_stage(|stages| {
            stages.new_terms = stages.new_terms.saturating_add(spent);
            stages.new_term_count = stages.new_term_count.saturating_add(1);
        });
        answer
    }

    /// Adds one entry to a term's doclist, keeping it in rowid order.
    ///
    /// **`term_row` staging what it finds is what makes the fast path apply
    /// twice.** `append_staged` is tried before `term_row` because a term
    /// this transaction has already staged answers there directly; it is
    /// tried again straight after, because `term_row` - when the term
    /// exists - has just staged what it read, and the ordinary case for a
    /// bulk load is a document whose rowid sorts after everything already in
    /// the list, which is exactly what the retried fast path handles without
    /// a decode. What is left after both tries is a term with no row yet, or
    /// one whose new entry does not sort last - and both go through the
    /// general decode-and-reinsert path below.
    fn merge_term(&self, context: &mut Context<'_>, term: &[u8], entry: DocEntry) -> DbResult<()> {
        if append_staged(&self.pending, term, &entry) {
            if buffer_is_full(&self.pending) {
                flush_doclists(context, &self.shadows, &self.pending)?;
            }
            return Ok(());
        }
        let Some(held) = term_row(context, &self.shadows, &self.pending, term, true)? else {
            return Ok(());
        };
        if !held.fresh && append_staged(&self.pending, term, &entry) {
            if buffer_is_full(&self.pending) {
                flush_doclists(context, &self.shadows, &self.pending)?;
            }
            return Ok(());
        }
        // A term whose dictionary row was created a moment ago has no
        // doclist to merge into; one that failed the retry above has an
        // entry that does not sort last. Decoding the whole doclist to
        // insert in the middle and then re-encoding it costs time and
        // allocation proportional to how many documents already contain the
        // term, so a bulk index build was quadratic: measured at 100, 200,
        // 400, 800 and 1,600 documents sharing a vocabulary, the cost of one
        // insert rose 1.00x, 1.36x, 2.03x, 3.32x, 6.24x, and the total went
        // from 19 ms to 1,925 ms for sixteen times the documents. That is the
        // cost of an out-of-order document, not of the ordinary bulk load the
        // fast path above already leaves this branch for.
        let existing: Option<Vec<u8>> = if held.fresh {
            None
        } else {
            read_doclist(context, &self.shadows, &self.pending, term)?
        };
        let mut entries = match existing.as_deref() {
            Some(bytes) => decode_doclist(bytes),
            None => Vec::new(),
        };
        match entries.binary_search_by_key(&entry.rowid, |existing| existing.rowid) {
            Ok(position) => {
                if let Some(slot) = entries.get_mut(position) {
                    *slot = entry;
                }
            }
            Err(position) => entries.insert(position, entry),
        }
        let encoded = encode_doclist(&entries);
        stage_doclist(&self.pending, term, encoded);
        if buffer_is_full(&self.pending) {
            flush_doclists(context, &self.shadows, &self.pending)?;
        }
        Ok(())
    }

    /// Removes one row from the content and from every doclist it is in.
    pub(crate) fn remove(&mut self, context: &mut Context<'_>, rowid: i64) -> DbResult<()> {
        // **A contentless table cannot be deleted from**, because un-posting a
        // row needs the text that produced its terms and that text was never
        // stored. SQLite refuses with this sentence; `delete-all` is what a
        // contentless table has instead.
        if self.contentless {
            return Err(failure(format!(
                "cannot DELETE from contentless fts5 table: {}",
                String::from_utf8_lossy(&self.name)
            )));
        }
        let offsets = self.offsets(context);
        let suffix = self.content.clone();
        let Some(content) = self.shadows.read_row(context, &suffix, rowid)? else {
            return Ok(());
        };
        let texts: Vec<Option<Vec<u8>>> = (0..self.options.columns.len())
            .map(|index| {
                offsets
                    .get(index)
                    .and_then(|at| content.get(*at))
                    .and_then(text_of)
            })
            .collect();
        self.remove_postings(context, rowid, &texts)
    }

    /// FTS5's `delete` command: removes one row whose text the caller supplies.
    ///
    /// **The way a contentless or an external content table is kept in step.**
    /// SQLite's documented triggers for an external content table run
    /// `INSERT INTO f(f, rowid, a, b) VALUES('delete', old.rowid, old.a, old.b)`,
    /// because by the time an `AFTER DELETE` or `AFTER UPDATE` trigger runs, the
    /// content row holds the new text or nothing, and the postings can only be
    /// found from the old one. The command was not recognised here, so every
    /// `UPDATE` and `DELETE` of such a content table failed with "SQL logic
    /// error" and the index kept postings for text that was gone.
    ///
    /// @param context - the running statement
    /// @param rowid - the row the text was indexed under
    /// @param values - the row's declared columns, in declared order
    pub(crate) fn remove_given(
        &mut self,
        context: &mut Context<'_>,
        rowid: i64,
        values: &[Value<'static>],
    ) -> DbResult<()> {
        if !self.stores_no_rows() {
            return Err(failure(
                "'delete' may only be used with a contentless or external content fts5 table",
            ));
        }
        let texts: Vec<Option<Vec<u8>>> = (0..self.options.columns.len())
            .map(|index| values.get(index).and_then(text_of))
            .collect();
        self.remove_postings(context, rowid, &texts)
    }

    /// Takes one row out of every doclist its text is in, and out of the sizes
    /// and the totals.
    ///
    /// @param context - the running statement
    /// @param rowid - the row
    /// @param texts - the text of each declared column, `None` for NULL
    fn remove_postings(
        &mut self,
        context: &mut Context<'_>,
        rowid: i64,
        texts: &[Option<Vec<u8>>],
    ) -> DbResult<()> {
        let width = self.options.columns.len();
        let mut terms: Vec<Vec<u8>> = Vec::new();
        for (index, column) in self.options.columns.iter().enumerate() {
            if column.unindexed {
                continue;
            }
            let Some(text) = texts.get(index).cloned().flatten() else {
                continue;
            };
            terms.extend(self.tokenizer.tokens(&text));
        }
        terms.sort();
        terms.dedup();
        for term in &terms {
            let removal = remove_row_from_term(context, &self.shadows, &self.pending, term, rowid)?;
            let TermRemoval::Emptied { legacy_page } = removal else {
                continue;
            };
            // The row was the last one in the doclist. The buffer has let the
            // term go, so what is left is the stored row, and a delete that
            // left it there would find the term again.
            self.shadows.delete_keyed(
                context,
                b"idx",
                &[Value::Integer(SEGMENT), Value::owned_blob(term)?],
            )?;
            if let Some(page) = legacy_page {
                self.shadows.delete_row(context, b"data", page)?;
            }
        }

        let sizes = match self.shadows.read_row(context, b"docsize", rowid)? {
            Some(row) => row
                .get(1)
                .and_then(Value::as_blob)
                .map(|blob| decode_sizes(blob.raw(), width))
                .unwrap_or_else(|| vec![0; width]),
            None => vec![0; width],
        };
        self.shadows.delete_row(context, b"docsize", rowid)?;
        if !self.stores_no_rows() {
            self.shadows.delete_row(context, b"content", rowid)?;
        }
        forget_content_rowid(&self.pending, rowid);
        let mut totals = buffered_totals(context, &self.shadows, &self.pending, width);
        totals.rows = (totals.rows - 1).max(0);
        totals.tokens.resize(width, 0);
        for (index, size) in sizes.iter().enumerate() {
            if let Some(total) = totals.tokens.get_mut(index) {
                *total = (*total - size).max(0);
            }
        }
        stage_totals(&self.pending, totals);
        Ok(())
    }
}
