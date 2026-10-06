# task-2191 guidance, round 3: the last eight usage workloads slower than SQLite

Written for task-2191 by task-2196 on 6 October 2026. It is guidance, not a change. Every claim
about inillucent comes from reading the task-2191 worktree at commit `4140925c` (the round 2 merge);
every claim about SQLite comes from reading `.sqlite-ref/3.53.4/src/sqlite3.c`. Four reading reports
with the full citations are under `_agent_output/task-2196/` in the main checkout, and one measurement
was taken, the Defender probe in section 5.

| Report | What it covers |
|---|---|
| `insert-path.md` | `inillucent_stmt_execute_params` down to `write_row`, `make_room`, the log record, the undo record, recovery of an `InsertRow`, how `ROLLBACK` works, the `DeleteRows` record |
| `syncs-and-open.md` | the commit path, the close fold and every sync in it, the crash campaigns, `inillucent --db f query` from `main` to the first row, the binary and its DLLs |
| `cold-read-and-open-handles.md` | the page miss path, the checksum, the plan of the 100 row query, every file open of a Python connection, the per statement tail check |
| `sqlite-comparison.md` | SQLite's sync count, index inserts, statement journal, page cache, open, change detection |
| `defender-probe/results.md` | the one measurement: a by name size query against a first handle open |

Numbers quoted as "ours / SQLite" are task-2191's round 2 numbers from 5 October. Where this document
gives an expected gain, it is arithmetic from the code and the round 2 profile, and it says so. Nothing
in sections 2 to 6 was timed by task-2196.

## 1. The short version

### 1.1 Two premises in the ticket do not hold against the source

1. **SQLite makes three syncs for `cli.insert`, not two.** `syncJournal` (sqlite3.c 64039 to 64051)
   syncs the journal body, writes the record count into the journal header, and syncs the journal
   again, because the Windows VFS reports neither `SQLITE_IOCAP_SAFE_APPEND` nor
   `SQLITE_IOCAP_SEQUENTIAL`. Then `sqlite3PagerSync` (66088) syncs the database. Deleting the
   journal costs no sync on Windows. So SQLite is at three. We are at **four**: the commit's log sync,
   the fold's journal seal, the fold's data file sync, and a second journal sync in `Journal::finish`
   (`inillucent-pool/src/journal.rs:452-455`) that follows no write. The trace that counted three
   missed one or found it cheap. Section 3 says what to do.
2. **`py.open.close.100` never touches the log segment at HEAD.** The open stops at `Slot::Pending`
   (`inillucent-engine/src/connect.rs:276`), nothing calls `ready()`, and `inillucent_close` returns
   from `fold_if_owed` while the slot is pending. Per open the files touched are two by name stats
   and the data file handle. The segment `<db>-wal.0000000001` is not stat'ed, opened or read. The
   virus scanner cannot be waiting on it. What the 100 opens pay instead is the open's own fixed work,
   and section 5 lists it. The ticket's sentence is true of a connection that runs a statement, and
   section 5.3 covers that case because it is the one that matters for every command line workload.

### 1.2 Ranked by expected gain per unit of risk

| Rank | Change | Workloads | Expected gain | Risk | Decision needed |
|---|---|---|---|---|---|
| 1 | Skip the `Journal::finish` sync when nothing was written since the seal (section 3.1) | `cli.insert` | about 1.2 ms of a 2.3 ms gap | none to durability; one line and the fold campaigns | no |
| 2 | Under appends, no splice and no repack: let the delta area take the whole gap, then split with the left page packed once (section 2.2) | `py.insert.10k.txn`, every append workload | 3 to 5 ms of a 6.7 ms gap | low; policy inside `choose_fit`, the page format is unchanged | no |
| 3 | A statement cursor per tree: the leaf pinned across rows, scratch buffers reused, text borrowed (section 2.4) | every insert workload | 3 to 5 ms | low to medium; `write_row` and the pool pin API | no |
| 4 | Open: `register_module` must not force `finish_open`; one `refresh_catalog` per process; `Path::exists` replaced; no lock, one stat and a lazy `Pool::new` for an open that runs nothing (sections 5.2, 6.2) | all 8 command line workloads, `py.open.close.100` | 0.3 to 0.8 ms per process; 10 to 20 us per Python open | low | no |
| 5 | CRC-32 by carryless multiply, same polynomial, same values (section 2.6, 4.2) | every workload that reads pages or writes log records | about 0.2 ms on `cli.range.json`, 1.5 to 2 ms on `py.insert.10k.txn` | low; a correctness test against the table implementation | no |
| 6 | A guaranteed gap after a mid leaf compaction, measured by counting compactions rather than by timing (section 2.3) | `py.insert.10k.txn`, random index inserts | 2 to 3 ms | low to medium; one constant and the fit test; a read regression check on `inillucent-fullgate` | no |
| 7 | Frame buffers that are not zero filled before `ReadFile`; adjacent leaves read in one call (section 4.3) | `cli.range.json`, cold scans | 0.2 to 0.4 ms | low to medium; an uninitialised buffer type in the pool | no |
| 8 | One `InsertRows` record per statement and leaf run instead of one record per row and per index entry (section 2.5) | `py.insert.10k.txn`, `shell.insert.10k.txn`, the multi row insert, `.import` | 2.5 to 3.5 ms at commit | medium; a new log record kind, an LSN reserved before the batch, the no steal watermark armed at reservation | **yes, D1** |
| 9 | One undo record per statement for a run of inserts that cannot abort, expanded at rollback (section 2.7) | `py.insert.10k.txn` | 1 to 2 ms | medium; a new undo variant, the rollback campaigns | no |
| 10 | The command line's `exec` holds the lock from the statement through the close fold and defers the commit's log sync into it: four syncs become two (section 3.2) | `cli.insert` | about 1.2 ms more | medium to high; the write ahead gate, the lock span, the durability campaigns | **yes, D2** |
| 11 | A `log_end_at_checkpoint` field in the meta record and a by name tail check, so a first statement on a folded log never opens the segment (section 5.3) | every command line workload, any reader after a writer | about 3 ms once per file after a writer's close, 30 to 40 us per connection otherwise | medium; a meta field, and NTFS must not report a stale by name size for a file another process is appending to | **yes, D3**, after the test in 5.3 |
| 12 | PGO and function ordering for the release binary, after a page fault count per phase says where the 1.1 ms goes (section 6.3) | all 8 command line workloads | 0.3 to 0.7 ms per process, unmeasured | low; a build step | no |

Ranks 1 to 7 need no decision and none of them changes a file format or what is durable when. By
the arithmetic in section 2.8, ranks 2, 3, 5 and 6 together take `py.insert.10k.txn` from 36.5 ms to
between 27 and 30 ms against SQLite's 29.8. Rank 8 is what gets it clearly under. Rank 1 alone takes
`cli.insert` from four syncs to three, which is SQLite's count, and rank 4 is what gets the eight
command line workloads under, because every one of them pays the open.

### 1.3 The three decisions only Jason can make

- **D1, the log record format.** A batched `InsertRows` record is a new record kind. A binary from
  before the change refuses a log that contains one. Because a clean close leaves every record below
  the checkpoint, an older binary only meets the new kind after a crash of a newer one. Section 2.5.
- **D2, what `inillucent --db f exec` acknowledges.** Today the row is durable when `exec` returns
  from the statement, before the close fold, and another process can read it in between. Two syncs
  are reachable only if the acknowledgement moves to the end of the close fold and the lock is held
  across. The library's `commit` is not affected. Section 3.2. The larger version of the same
  decision, not recommended, is in section 3.3: whether a cleanly closed file in `journal_mode =
  delete` must be self contained without its segment.
- **D3, a meta record field for the log's end at the last checkpoint.** It is what lets a reader know
  there is nothing to replay without opening the segment. It is a format addition to the data file,
  and it depends on a test of NTFS by name sizes that nobody has run. Section 5.3.

## 2. Gap 1: Python `execute_many`, 10,000 rows, two indexes. 36.5 ms / 29.8 ms

### 2.1 Where the time goes, from the code

The statement runs as one `INSERT` over 10,000 rows (`run_rows_at_once`, `insert_rows`,
`insert_cached`, loop at `inillucent-exec/src/dml/insert.rs:158`), into a table that already holds
10,000 rows, so `write_row` is the path. Per row the engine does three tree puts, and the three share
nothing but the eight leaf hint windows per tree and the table's largest rowid hint. The costs that are
not in SQLite's path, with the arithmetic that sizes them:

| Cost | How much, per the code | Over the workload |
|---|---|---|
| Compactions in the table tree. After a splice the gap is at most 5% of the page (`TIGHT_FILL` 0.95, `write.rs:292`), about 1.6 KiB, which holds about 9 rows of this fixture's 165 byte rows. Every ninth append splices: one pass over every live row and column plus two 32 KiB copies (`leaf/splice.rs:195`, `compact_into`'s `install`). Every fourth compaction is a repack (`SPLICE_LIMIT` 3, `leaf.rs:188`): materialise, size, encode. A split is about six passes and a page copy | about 1,100 splices, 370 repacks, 55 splits | 4 to 6 ms |
| Compactions in the `tag` index. An entry is 17 tagged bytes plus 4, so the same 1.6 KiB gap takes about 78 entries, then every live row of a leaf of about 1,500 entries is rewritten | about 250 compactions | 2 to 3 ms |
| Pool lookups: 5 per table row, 3 per index entry, 11 per row, each a `HashMap` lookup under `state.borrow_mut()` plus `borrow_frame`; `modify` adds three more `state` borrows | 110,000 lookups | 2 to 4 ms |
| Allocations: `key`, `encode_key`'s `Vec<u8>`, `encoded_row`, `runs` on every `write_row`; the text of a cell copied five times between the C layer and the leaf (`String`, `Value::to_engine`, `build_row`, `entry_from`, the undo key) | 12 to 15 allocations and about 750 bytes of text copies per row | 3 to 4 ms |
| The log: 30,000 records of 32 byte header, 8 byte tree, 8 byte page, 4 byte length, body, padding to 8, a crc32 per record. About 248 bytes a row, 2.48 MB, more than half of it framing. One mutex per record, one crc per record, one write per 512 KiB, one sync at commit | 30,000 crcs of about 80 bytes, 2.48 MB written and synced | the ticket's 5 ms at commit; the crcs about 2.4 ms more |
| The undo: a `Before` per row and per index entry, pushed into a `Vec` that is never reserved, freed at `COMMIT` | 30,000 pushes | 1 to 2 ms |
| `Instant::now()` in `make_room_timed`, `timed_live_order`, `timed_splice`, `timed_compact_image`, each copying `WriteStats` through a `Cell`, with no harness switch (`write.rs:1776-1860`, `2090-2150`) | 4 to 6 clock reads per compaction | about 0.3 ms |

These add to more than the 6.7 ms gap, which is the point: no single change closes it, and four of
them together do. Section 2.8 gives the order and the arithmetic.

### 2.2 Rank 2: under appends, no splice and no repack

The round 2 experiment with a guaranteed gap at 8%, 15% and 25% measured 41 to 45 ms on a machine that
was not quiet, against a quiet 36.5 ms baseline. A 4 ms spread hides a 3 ms effect, so that experiment
says nothing either way, and section 2.8 says how to run it so that it does.

For the two trees that take appends in this workload, the table and the `created` index, the gap
constant is the wrong tool anyway. SQLite's `balance_quick` (82410) handles an append into a full
leaf by giving the new right sibling the one overflow cell and leaving the left page as it is. Our
leaf cannot be left as it is, because the delta rows have to be packed into the sorted region once.
It can be packed once:

1. In `choose_fit` (`write.rs:1871`), when `is_appending` is true, do not try a splice and do not
   try a repack. Return `Fit::Split` only when the delta area cannot take the row, which with the
   whole free space available to the delta area is when the leaf is genuinely full. Between splits,
   the delta area grows to about 185 rows for this fixture. The delta directory search stays a
   binary search over the directory (`locate_new_key_slot`, `leaf/read.rs:472`), and the directory
   move in `apply_delta` is `2 * delta_count` bytes, under 400 bytes at the end of a run.
2. The split packs the live rows once at `APPEND_FILL` into the left image and puts the rows that did
   not fit into the right page. That is `split_carrying` (`write.rs:2371`) without the failed splice,
   the failed repack, and the `leaf.bytes().to_vec()` page copy that `Fit::Split` carries today.
   Measure and encode are the two passes that remain.
3. Set the leaf hint to the new right page instead of clearing all eight windows (`note_leaves(1)`,
   `paged.rs:882`), so the next row does not descend. Round 2 section 3.2 item 5 asked for this and
   `insert-path.md` section 7 says it is not done.
4. Reads of an append leaf between splits see a delta area of up to 185 rows. A read merges the sorted
   region and the delta directory (`leaf/delta.rs`), which it already does for the 9 to 78 rows the
   delta area holds today. Check `inillucent-fullgate`'s point read workloads for a regression; the
   arithmetic says a binary search over 185 entries is three more compares than over 20.

Passes per leaf under appends go from roughly 20 splices plus 5 repacks plus a 6 pass split, about 45
passes and 50 page copies, to one measure and one encode. For the table tree in this workload that is
the 4 to 6 ms in section 2.1 down to under 1 ms. The `created` index gets the same treatment. The page
format does not change, and no record format changes: a split still logs `SplitLeaf` or
`Structural`, and the `CompactLeaf` records of the splices simply stop being written.

### 2.3 Rank 6: the guaranteed gap, for the leaves that are not appends

The `tag` index takes 200 entries per tag into 50 runs, each run advancing by rowid, so every leaf
sees inserts at several positions and the delta design is the right one. Its constant is wrong. The
cost per insert is `rows_in_leaf / entries_between_compactions`, and the second number is what 5% of
the page leaves, about 78. A gap `G` after every compaction, with the split taken when the packed
image would leave less than `G`:

- `G` at 12.5% of the page, 4 KiB: about 240 entries between compactions, the rewrite per insert
  from about 19 rows to about 6. `G` at 20%: about 380 entries, about 4 rows per insert.
- Space: leaves pack to at most `1 - G`. SQLite's random insert leaves sit near 75%, so 12.5% is still
  denser than SQLite.
- Where: `TIGHT_FILL` and `COMPACT_FILL` in `write.rs:253-292`, the `fits` test in `pack_or_split`
  (`write.rs:2022`), and `splice_image`'s size refusal (`splice.rs:62-85`).

Before timing anything, add three counters to `WriteStats`: splices, repacks, splits. They are
deterministic, they do not need a quiet machine, and a run that reports 250 compactions at 5% and 80
at 12.5% has answered the question that the round 2 timing could not. Then time it quietly, once.

### 2.4 Rank 3: a statement cursor per tree

Round 2's rank 1 is partly done: the room check reuses the locate's parse. The lookups, the
allocations, the stats copy and the hint reset are untouched. The change is a `WriteCursor` that lives
for the statement, one per tree the statement writes:

1. **The pinned leaf.** The cursor holds the pool pin (`borrow_frame`, `pool.rs:1259`) on the leaf
   it last wrote and its parsed `LeafRef` header. For the next row, compare the key against the
   leaf's fence keys, which `leaf_for_hinted` already does by byte compare, and if it falls inside,
   skip `hinted_largest_key`, `leaf_for_hinted`, the locate's fetch and `note_appended_key`: the
   cursor already knows the page, the parse and the last slot. `modify` still runs for the write,
   and its three `state` borrows can become one by passing the frame the cursor holds. 11 lookups
   per row become about 3, one `modify` per tree.
2. **The largest rowid.** The table cursor remembers the rowid it last wrote while it still holds the
   rightmost leaf. `hinted_largest_key`'s four header reads validate what a cursor that never let go
   of the page already knows. SQLite's `BTCF_AtLast` (78999) is the same idea.
3. **Scratch buffers.** `key`, `encoded_key`, `encoded_row`, `runs` and the undo key become fields
   of the cursor, cleared and refilled per row. `encode_key_small` (`paged/descent.rs:25`) exists
   and the read path uses it; `write_row` still calls the allocating `encode_key`.
4. **Text borrowed, not copied.** `inillucent_py_params` reads each Python `str` into the C layer's
   `String`. From there to the leaf the bytes are copied four more times. The row the statement
   builds can hold `Datum::Text(&[u8])` into the parameter buffer for the life of the row, and the
   leaf encoder copies once. `build_row`'s `RowSource::Lent` clone and `entry_from`'s owned text are
   the two to remove first; `insert-path.md` section 1 names each copy.
5. **Invalidation.** A split, a compaction, a spill, or any structural record on the tree drops the
   pin and the cursor descends once. The hint windows stay as the fallback.

Expected gain: 3 to 5 ms across the lookups and the allocations in section 2.1. Risk is low to
medium: the pool's pin API has to allow a pin held across `modify` calls on the same frame, and the
cursor must be dropped before anything that can evict.

### 2.5 Rank 8: one `InsertRows` record per statement and leaf run. Decision D1

The log is the right design for autocommit and the costly one here. 30,000 records at 248 bytes a
row is 2.48 MB, more than half framing, and the per record crc is the single largest hidden cost:
30,000 crcs of about 80 bytes at the table implementation's rate is about 2.4 ms, on top of the 5 ms
the commit takes to write and sync the bytes.

`DeleteRows` (task-2183) is the model: after the 32 byte header, `u64 tree`, `u64 page`, `u32 L`,
then a key list, padded to 8, applied to one page gated by that page's LSN (`recover.rs:737-754`,
`redo.rs:120`). An `InsertRows` record batches rows for the statement:

- **Layout.** Header, then entries of `varint tree`, `varint page`, `varint len`, row bytes, with a
  `u32 count` first, padded to 8, one crc over the record. Entries may name different trees and
  pages, so the Python workload's three puts per row go into one batch. Framing per entry falls from
  52 bytes to about 5.
- **LSN.** A page is stamped with the LSN `log.log` returns, and an LSN is a byte position in the
  log, so a batch has to reserve its position when it opens: the current `written_end`. Every entry's
  page is stamped with that position. Nothing else may be appended until the batch flushes, which the
  cut rules guarantee. Recovery's rule is unchanged: apply an entry when its page's LSN is below the
  record's, and a page stamped with the record's own LSN is skipped as already folded.
- **Cut rules.** Flush the batch before any other record is appended: `compact_into`'s
  `CompactLeaf`, `split_carrying`'s `AllocPage` and `SplitLeaf` or `Structural`, an `Abort`, an undo
  write, a spill, and at statement end. Also at a size cap of 64 KiB, so a batch never holds more
  than the writer's spill unit. With section 2.2 in place the table tree logs nothing between splits,
  so this workload's 30,000 records become a few hundred.
- **The no steal watermark** is armed by the first record (`insert-path.md` section 9). It has to be
  armed at the reservation, so that a page stamped with a reserved LSN cannot reach the data file
  before the record is in the log. `has_an_unclaimed_tail` and the "page stamped above the log's
  end" refusal are the two checks that would otherwise fire on a crash between stamp and flush;
  with the watermark armed, no such page is ever written.
- **Recovery.** One arm in `Applier::redo` near `redo.rs:1117`, applying entries in order, each gated
  by its own page. `PageList` holds at most three pages (`MAX_PAGES` 3, `recover.rs:62`), so the
  arm checks each entry's page itself rather than returning a page list.
- **Compatibility.** The record kind is new. A binary from before the change refuses a log with an
  unknown kind. A clean close leaves every record below the checkpoint, where no open reads, so the
  older binary meets the kind only when the newer one crashed. That is D1: whether a crash of a newer
  binary may leave a file an older binary cannot open until the newer one has run recovery on it.

Expected gain: the commit's write and sync from about 5 ms to about 2 ms, and the 30,000 short crcs
replaced by a few hundred long ones. Rank 5's faster crc shrinks the same cost from the other side; do
rank 5 first, because it is free of any decision, then measure whether rank 8 is still needed for this
workload. It stays the right change for `shell.insert.10k.txn` and `.import`, which write the same
records.

### 2.6 Rank 5: the checksum, same polynomial, ten times the rate

`inillucent-base/src/checksum.rs` computes CRC-32/ISO-HDLC, polynomial `0xEDB88320`, with slice by
eight tables and four streams for buffers of 4 KiB or more, at the file's own quoted 3.93 us per 32 KiB
page. The doc comment at `page.rs:121` calls it crc32c; it is not. The comment at
`checksum.rs:176-200` rules out the SSE4.2 `crc32` instruction because that instruction computes the
Castagnoli polynomial and would change the on disk format. That is correct and it is not the only
hardware route. A carryless multiply implementation (`PCLMULQDQ`, with the folding constants for
`0xEDB88320`) computes the ISO-HDLC polynomial, the values on disk do not change, and the rate is
about ten times the table rate. The `crc32fast` crate is this implementation with a runtime feature
check and a table fallback; a hand written one is about 150 lines.

It pays everywhere a page is read or a record is written: about 0.22 ms of `cli.range.json`'s 65
pages (section 4), about 1.5 to 2 ms of the Python insert's 30,000 record crcs plus the commit's
write, and every fold's writeback. Test it against the existing implementation on random buffers of
every length from 0 to 70 KiB, and keep the table path for a CPU without the feature.

### 2.7 Rank 9: the undo, and why removing it is a bigger change than round 2 said

Round 2's rank 2 was a compile time `may_abort` and no undo when it is false, as SQLite's
`sqlite3MayAbort` does. Reading the rollback code changes that advice. `rollback`
(`engine/batch.rs:536`) runs `undo_to_floor`, which pops every `Before` newest first and applies it
as a compensating `tree.put` or `tree.delete` (`batch.rs:131-160`). The pool keeps no clean copy of a
page, the log has no `Begin` record, and `Body::Abort` is never written. So the `Before` records are
not only the statement abort path; they are the only way a user `ROLLBACK` works. SQLite can skip
them because its rollback is the journal's page images. Dropping ours for a statement that cannot
abort would leave `BEGIN; INSERT ...; ROLLBACK` with nothing to roll back with.

Rolling back by discarding dirty frames is possible in principle: the no steal watermark already
keeps uncommitted pages out of the data file, so every uncommitted change is in frames that could be
dropped and read again. It also needs the spill file entries dropped, the free map, the catalog and the
`PagedTree` cached state restored, and a per statement savepoint for `abandon`. That is a rollback
redesign, not an insert change, and 1 to 2 ms on this workload does not justify it.

What does pay, and needs no redesign:

1. **Reserve the undo `Vec`** for the rows the statement knows it will write: three entries a row for
   this table. Trivial.
2. **One undo record per run.** For a statement whose rows cannot abort, which is an `INSERT` with
   no trigger, no foreign key, no `RETURNING`, no upsert, no unique secondary index and a rowid the
   engine assigns, the rows it writes are a contiguous rowid range and every index entry is derivable
   from the row. A new undo variant `InsertedRange { table, first_rowid, last_rowid }` replaces the
   30,000 `Before`s with one. `undo_to_floor` expands it at rollback: read each row in the range,
   delete its index entries, delete the row. A statement that writes 10,000 rows and is then rolled
   back does the same 30,000 compensating writes it does today, through the same code, from one
   record. `commit_the_undo` (`batch.rs:548`) logs the compensating writes, as it does now.
3. **The abort path.** With a unique secondary index the statement can abort on a later row. SQLite
   checks every unique index before the first write of the row (`OP_NoConflict`, reused by
   `OP_IdxInsert` through `OPFLAG_USESEEKRESULT`). Doing the same here, a locate on each unique index
   before the table put, makes the per row abort impossible after the first write, so the range undo
   covers unique indexes too. Without it, keep the per entry `Before` for statements with a unique
   secondary index.

### 2.8 The arithmetic and the order

| Step | Takes `py.insert.10k.txn` from | To, by arithmetic |
|---|---|---|
| counters in `WriteStats` for splices, repacks, splits; the `Instant::now()` calls behind a switch | 36.5 ms | 36.2 ms, and the numbers section 2.3 needs |
| rank 2, appends split without compaction | 36.2 | 31 to 32 |
| rank 6, the gap for the `tag` index, chosen from the counters | 31 to 32 | 29 to 30 |
| rank 5, the checksum | 29 to 30 | 27.5 to 28.5 |
| rank 3, the statement cursor and the text copies | 27.5 to 28.5 | 24 to 25 |
| rank 8, `InsertRows`, if D1 is yes | 24 to 25 | 21 to 23 |
| rank 9, the range undo | 21 to 23 | 20 to 22 |

SQLite is 29.8. The first four steps need no decision and reach it by this arithmetic, with the
caveat that the arithmetic is from the code and not from a profile. Profile after rank 2, because
rank 2 changes the shape of the profile more than anything else here.

## 3. Gap 2: `exec` of a one row `INSERT`. 24.4 ms / 22.1 ms

### 3.1 Rank 1: four syncs, one of them for nothing

The close fold in `delete` mode runs, in order: the commit's log sync (`Wal::commit`,
`writer.rs:664`, then `drive`'s `sync_all` at `writer.rs:1148`); `seal_journal`'s sync after the pre
images of every dirty page and both meta slots are saved (`fold.rs:149`, `journal.rs:410`); the data
file sync after the pages and both meta slots are written (`meta_write.rs:151`); then
`Journal::finish` syncs the journal again (`journal.rs:452-455`) and unlinks it. Between the seal and
`finish` nothing is written to the journal: every pre image was saved before the seal, and the page
writes find them saved. The `unsealed` flag that `save` sets and `seal` clears is never read by
`finish`. The fix is `finish` syncing only when `unsealed` is true, which in `truncate` and `persist`
modes it always is, because `finish` writes there before it syncs. In `delete` mode the sync goes.

A `FlushFileBuffers` with nothing dirty still issues the flush command to the device, so it costs
what the other three cost, about 1.2 ms on this box. That is more than half of the 2.3 ms gap, it
changes nothing about what is durable when, and the fold campaigns (`fold_protocol`, the DELETE
fault and power loss campaigns in `crash_reports.rs`) are the proof that it did not.

Confirm the count first with a trace of `FlushFileBuffers` on both processes, ours and
`sqlite3.exe`, for one `cli.insert`. Source reading says four and three. If the trace says three and
three, the `finish` sync is being elided somewhere this reading did not find, and the rest of this
section still stands.

### 3.2 Rank 10: two syncs. Decision D2

With rank 1 done, `cli.insert` makes three syncs, as SQLite does, and the remaining gap is the open
(section 6). Getting under SQLite on syncs means two, and the only route that keeps "acknowledged
means durable" is to make the acknowledgement later:

1. The command line's `exec` driver opens the connection with a flag, `defer_commit_sync_into_fold`,
   that the library never sets.
2. Under the flag, the statement's commit appends its records and does not sync. The lock is **not**
   released after the statement: today `leave` releases to `None` (`locks.rs:822`) and
   `fold_on_close` takes EXCLUSIVE again (`locks.rs:904`). Under the flag the connection goes from
   RESERVED straight to EXCLUSIVE and folds. No other process can read the log in between, so no
   process ever builds on records that are written but not durable.
3. The fold writes the pre images and seals the journal (sync one), writes the pages and both meta
   slots and syncs the data file (sync two), unlinks the journal. The write ahead gate at
   `journal_gate.rs:152-178`, which refuses to write a page whose records are not yet durable, is
   relaxed for a fold that has sealed a journal, because the journal's pre images are what make the
   data write recoverable, not the log.
4. `exec` prints its result and exits 0 after the fold. A crash before the data sync leaves a sealed
   journal: recovery restores the pre images, the data file is at its pre statement state, and the
   log's unsynced tail is a transaction with no durable commit, which recovery discards. Nothing was
   acknowledged, so nothing was lost that was promised.
5. The campaigns: add a cut point campaign for this path, every cut from the first record append to
   the unlink, with the assertion that exit 0 implies the row is in the file and a crash before exit
   leaves the file self contained at one of the two states.

This is a durability protocol change for one caller, and it is D2. My recommendation is to do rank
1 and section 6 first and measure. If `cli.insert` is then within noise of SQLite, do not do this; if
it is still slower, this is the change, and it is confined to the `exec` verb.

### 3.3 The larger version of D2, not recommended

The fold at close exists because `fold_on_close` folds whenever `since_fold() > 0`
(`locks.rs:841`), which makes a cleanly closed `.rdb` self contained: `fold_protocol::a_closed_file_is_self_contained`
tests exactly that, and it is the promise `journal_mode = delete` makes in SQLite, one file after
close. Giving that up, leaving the records in the segment for the next open to replay, would make
`cli.insert` one sync, and would mean a copied `.rdb` without its `-wal.N` file silently loses its
last transactions. SQLite's WAL mode has the same property and SQLite documents it as the reason
WAL is not the default. I would not make it the default here either. If it is ever wanted, it is a
pragma, off by default.

## 4. Gap 3: a cold query of 100 rows by an index, sorted, as JSON. 19.9 ms / 18.2 ms

### 4.1 What the 65 pages are

`SELECT id, title, created FROM note WHERE tag = 't7' ORDER BY created LIMIT 100`. `t7` matches 200
of 10,000 rows, every fiftieth, and the plan is a span on `note_tag` then an `IndexNestedLoopJoin`
probing the table by rowid (`stages.rs:988`, `:1103`, `loops.rs:110`), then a sort of 200 rows. A
32 KiB leaf holds about 190 of these rows, so every fiftieth row touches every leaf: the query reads
the whole table, about 59 leaves, plus the index and interior pages, 65 pages and 2.1 MB. SQLite's
4 KiB leaf holds about 24 rows, every fiftieth row touches about every other leaf, and it reads about
180 pages, 0.7 MB. The ticket says the default page size does not change, so the byte count does not
change, and the lever is what a byte costs. Three costs per page, from the code:

| Per 32 KiB page miss | What happens | Arithmetic for 65 pages |
|---|---|---|
| a fresh frame buffer | `Vec::try_reserve_exact(32768)` then `resize(32768, 0)` (`eviction.rs:123`): a heap block from the system heap, since the class list is empty in a new process, zero filled by the CPU, eight first touch page faults, then overwritten by the read | 2 MB of memset and about 520 page faults, 0.3 to 0.5 ms |
| the read | one synchronous `ReadFile` with an `OVERLAPPED` offset (`windows.rs:46`), no readahead, no scatter, no flags on the handle | the measured 673 us, about 10 us a call, a copy out of the file cache |
| the checksum | 32,760 bytes through the four stream table crc at 3.93 us | 255 us |

The total is about 1.3 ms of the 1.66 ms gap. The rest is the open, section 6.

### 4.2 Rank 5 here: the checksum

Section 2.6. About 0.22 ms of the 0.26.

### 4.3 Rank 7: frames that are not zeroed, and runs of leaves read together

1. **No zero fill.** The buffer is fully overwritten by `read_exact_at` on every miss, and a short
   read is already an error. Give the frame a buffer type that is reserved but not initialised, and
   let `fill_frame` set the length after the read. The faults stay, the 2 MB memset goes.
2. **Runs of adjacent pages.** The 200 probes arrive in ascending rowid order because the index
   entries for one tag sort by their trailing rowid, and the table's leaves were written by the
   fixture's bulk build, so consecutive leaves are consecutive pages. Nothing uses either fact. A
   pool call `fetch_run(first_page, count)` that reads `count` pages in one `ReadFile` into `count`
   consecutive frame buffers turns 59 calls into a handful. The join knows it is probing an ascending
   run; the tree knows the leaf's right sibling page id (`page.rs:131`, offset 24). The simplest
   trigger is in the probe: when the leaf a probe reaches has a right sibling that is the next page
   number and the next key is above the leaf's fence, prefetch the sibling. Saves most of the per call
   kernel cost, about 0.2 to 0.3 ms by the measured 10 us a call, and keeps the checksum per page.
3. **Not a mapping.** A read only mapping of the data file would remove the copy and the zero fill
   and leave only soft faults. It is also a pool redesign: frames that point into a mapping, copy on
   write for `modify`, remapping on growth, and a file another process is writing. Not for 1.66 ms.
   If the page size question is ever reopened, this is the alternative to weigh against it.

## 5. Gap 4: Python, 100 opens and closes. 7.6 ms / 7.2 ms

### 5.1 What an open that runs nothing does today

Both engines pay the virus scanner once: the first `CreateFile` of the data file after the
writer's handle closed. The probe in `_agent_output/task-2196/defender-probe/results.md` measured
that open at about 3.0 ms on this box, the second open at 35 to 46 us, and a by name size query
through `GetFileInformationByName` at 25 to 30 us with no scan triggered and no wait. So of the
7.6 ms, 3 ms is one scan and 4.6 ms is 99 opens at about 46 us; SQLite's are about 43 us. The 3 us
difference per open is the open's own fixed work against SQLite's `CreateFile`, a 100 byte read and
a `CloseHandle`:

| Ours, per open (`connect.rs:421`, `file.rs:499-600`) | SQLite |
|---|---|
| `is_a_file(path)` and `there_is_a_database_at(path)`: two by name stats of the same path | none |
| `access(<db>-journal)`: a third stat | none at open; `hasHotJournal` runs at the first read transaction |
| `CreateFileW`, `GetFileInformationByHandle` | `CreateFileW` |
| `LockFileEx` SHARED, later `UnlockFileEx` | no lock at open |
| a 16 byte read, then one 65,536 byte read of both meta pages, one page checksummed | a 100 byte read |
| `Pool::new`: a 32 KiB zeroed scratch buffer, chunk tables | a 48,000 byte lookaside allocation, untouched |
| `OsVfs::new`, `Registry::with_builtins` (12 modules) plus the search module | nothing |

### 5.2 Rank 4 here: the open that runs nothing

1. One stat, not three. The two stats of the data file ask what the `CreateFileW` answers. The
   journal stat stays only because the meta pages are read at open and a hot journal has to be
   replayed before anything is read; if item 3 below moves the meta read, this stat moves with it to
   the first statement, where `begin_read` already takes the lock.
2. No lock at open. The meta record read at open decides only the format, version and key
   refusals. The first statement reads the shadow record again under SHARED anyway
   (`disk_record_is_as_last_read`), so a torn read at open is caught there. A torn read that fails
   the checksum at open can be retried once without a lock.
3. Read what the refusal needs. The 16 byte header carries the format and version. If the key
   check needs the meta record, read the two 120 byte record heads at their fixed offsets to pick the
   newer, then the one 32 KiB slot that the checksum covers, instead of 64 KiB. If the record had its
   own checksum, the slot read would go too; that is a small format addition and too small a gain to make on
   its own.
4. `Pool::new` lazily: no scratch buffer and no chunk table until the first fetch. The pool exists
   for an open that may never run a statement.
5. `Registry::with_builtins` once per process, shared by `Rc`.

Expected: 10 to 20 us of 46 per open, which is the gap and more. It also helps every command line
workload, because the command line pays the same open.

### 5.3 Rank 11: the first statement, and the segment. Decision D3

Every connection that runs a statement opens the segment at its first statement, in `read_chain`
(`recover.rs:391-490`): a stat of the segment path, a `CreateFileW` read and write with create
allowed, `GetFileSizeEx`, the 64 byte header, and a read of the body from the checkpoint to the end
of the file, which for a cleanly closed file is nothing. A writable connection keeps the handle for
`the_log_moved`, one `GetFileSizeEx` per statement (`locks.rs:507`, `writer.rs:1012`). A read only
connection drops it after the replay and uses a scratch log. The segment is opened because the open
is the check: the meta record says where the log starts (`checkpoint_lsn`, `wal_sequence`) and
nothing says where it ends, so the only way to learn that there is nothing to replay is to open the
file and find no records after the checkpoint.

The cost is the virus scanner, once per file after a writer's close, about 3 ms on this box, plus
the write capable open and close, 30 to 40 us, on every connection. Every command line workload
after a fixture copy pays the scan once on the data file and once on the segment; SQLite pays it once.

The design that removes it, in three parts, each conditional on the one before:

1. **The test.** NTFS may report a stale size by name for a file another process holds open and is
   appending to. Two processes: one appends to a segment without closing and without syncing, the
   other queries `GetFileInformationByName` every millisecond and records when the size moves. If the
   by name size lags the handle size by more than the appender's own write call, this design stops
   here and the handle stays. If it tracks, go on. Nobody has run this; the probe in this round did
   not, because it tested a closed file.
2. **A `log_end_at_checkpoint` field in the meta record**: the segment's byte length at the moment
   the fold wrote the checkpoint. The fold writes the meta record anyway, so it costs no write. This
   is D3, a format addition to the data file.
3. **The by name check.** At the first statement: by name size of the segment named by
   `wal_sequence`. If it equals `log_end_at_checkpoint`, there is nothing to replay and the segment is
   not opened. A read only connection never opens it. A writable connection opens it at its first
   append, under RESERVED, which round 2 section 6.3 already argued is the safer place for the header
   write. `the_log_moved` uses the by name size until the connection holds the handle, then the
   handle. The earlier check by path that cost 3.2 ms (task-1999) opened each segment and read its
   header; this one opens nothing, which is why it is 25 us and not 3 ms.

If the test in part 1 fails, a smaller version still removes the scan for the common case: a writer's
close that reclaims the log past 1 MiB rolls to a new segment and deletes the old ones
(`retire_segments_below`, `writer.rs:838`). A file whose writer closed with the segment folded could
also delete the segment when the segment holds nothing above the checkpoint, so that the next
connection's `access` finds no file and opens nothing. The next writer creates it again, about 100 us.
That is a policy change inside `journal_mode = delete` and no format change, and it trades a scan on
every reader for a create on every writer, which for the benchmark's read heavy command line is the
right trade and for an insert heavy process is not. Measure both before choosing.

## 6. Gap 5: process start and the cold open from the command line, 1 to 4%

### 6.1 Where the 1.4 ms goes, from the code

`syncs-and-open.md` section B6 lists every step from `main` to the first row. The ones that are not
in SQLite's path, or are in it once where ours run several times:

| Step | What it does | SQLite |
|---|---|---|
| `Context::open_for` calls `Path::exists` (`command/mod.rs:478`), which is `std::fs::metadata`, which on Windows opens the file | one `CreateFileW` and `CloseHandle` before the real open, and the first open after a writer's close is the one the scanner waits on | `sqlite3_open_v2` opens once |
| `Shell::open_one_reporting` calls `register_module` twice (`shell.rs:358-364`), each of which calls `ready()`, which runs `finish_open` | round 2's deferred open does not help the command line binary at all; every verb pays recovery, the schema and the catalog inside `Shell::open` | the schema loads at the first prepare (126961) |
| `refresh_catalog` runs three times per process: once in `finish_open`, once per `register_module` (`inillucent-engine/src/lib.rs:1148-1163`) | each clones every `TableInfo`, replans every foreign key, reads `sqlite_stat1` again when present | once |
| the catalog decoded twice (`read_checkpointed_catalog`, then `load_schema`'s `read_catalog_rows`), every `CREATE TABLE` parsed three times, every `CREATE INDEX` twice (`rowshape.rs:1043-1189`, `alter.rs:494`, `paged.rs:485`), every `SchemaEntry` cloned in `rebuild_tables` | for every table, whichever the query names | every `CREATE` parsed once by `sqlite3InitCallback` |
| `BUILTIN_EPONYMOUS` built on first use (`ddl/catalog.rs:882`): every built in module connected with no arguments, a `TableInfo` per module and per `PRAGMA` function | the author's own estimate is half a `refresh_catalog` | the shell registers about 12 extensions in `open_db`, cheaply |
| the first `HashMap` with `RandomState` loads `bcryptprimitives.dll` for its seed | one DLL load per process, delayed but not avoided; `--version` avoids it and every query pays it | SQLite has no hash map that seeds from the OS |
| 7.17 MB of `.text` in a 7.8 MB binary, fat LTO, `panic = abort`, `opt-level` 3; the startup path is scattered through it | code page faults; `EmptyWorkingSet` measured 1.1 ms of the 1.4 as faults | `sqlite3.exe` is about a quarter of the size |

### 6.2 Rank 4 here: the open that the command line pays

1. `register_module` records the module and lets `finish_open` connect it. Then the deferred open
   round 2 built reaches the command line: `--version`, `tables` and any verb that fails before its
   first statement never run recovery or the schema, and `refresh_catalog` runs once.
2. `Path::exists` becomes `path_state`, which the engine already uses for the same question
   (`connect.rs:189-213`).
3. Parse each `CREATE` once. `load_schema` and `rebuild_tables` both parse every statement from
   `sql: Vec<u8>`; the parsed `TableInfo` can be built once and shared by `Rc` through
   `rebuild_tables` and `refresh_catalog`'s `StaticCatalog`, which clones every `TableInfo` today.
   For this fixture it is one table and two indexes, so the absolute saving is tens of microseconds;
   for a real schema it scales with the schema.
4. A fixed seed hasher for the engine's own maps. The maps built at open are keyed by page ids,
   names and the engine's own identifiers, and a seed from `QueryPerformanceCounter` and the process
   id is enough for them. `bcryptprimitives.dll` then loads only for a query that hashes user data,
   if that map keeps `RandomState`, and the better answer is to seed that one from the same source
   too; a hash flood against an embedded database's own `GROUP BY` is the caller flooding itself.

### 6.3 Rank 12: the page faults

1.1 ms of faults is between 1,000 and 2,000 pages touched at start, code and data. Before changing
the build, measure which: `GetProcessMemoryInfo` gives `PageFaultCount`, and a delta at each of the
trace points task-2191 already has (process start, `open_for`, `finish_open`, first statement,
output) says where they are. Then:

- If most are code, PGO (`-Cprofile-generate` on the benchmark run, `-Cprofile-use` for the release)
  clusters the hot path and lets the linker order the cold code away from it. The round 1 experiment
  with `opt-level = "s"` (`inillucent-task-2191-optS` in the target directory) shrank the binary at
  the cost of the hot loops; PGO shrinks the touched pages without that cost.
- If most are data, the candidates are the 32 KiB frame buffers (section 4.3 item 1), `Carved`'s
  64 KiB chunks, and the catalog clones in section 6.1, in that order.

## 7. The decisions, stated plainly

| Decision | What changes | What it buys | My recommendation |
|---|---|---|---|
| **D1**: a new log record kind, `InsertRows`, that an older binary cannot replay | the log format version; a crash of a newer binary leaves a log an older binary refuses until the newer one recovers it | about 3 ms on the Python indexed insert, about the same on `shell.insert.10k.txn` and `.import` | yes, after ranks 2, 3, 5, 6 are measured; the log is a transient file and this is the cost of the per row logical record design that wins every autocommit workload |
| **D2**: the command line's `exec` acknowledges after the close fold, holding the lock across, and the commit does not sync on its own | what "the statement returned" means for one caller; the write ahead gate for a journaled fold; a new campaign | about 1.2 ms on `cli.insert` | only if `cli.insert` is still slower than SQLite after rank 1 and section 6; and never the section 3.3 version |
| **D3**: a `log_end_at_checkpoint` field in the meta record | the data file format, one field | one virus scan per file after a writer's close, and a write capable open per connection | yes, if the NTFS by name size test in section 5.3 passes; otherwise the segment delete at a folded close, measured against the writer's create |

Everything else in this document is policy, caching, or framing inside the existing formats.

## 8. The order to do it in

1. Rank 1, the `finish` sync, with a `FlushFileBuffers` trace of both processes before and after.
2. The counters in `WriteStats` and the `Instant::now()` switch, then rank 2, then a profile of
   `py.insert.10k.txn`.
3. Rank 5, the checksum, because every later measurement should include it.
4. Rank 6 from the counters, rank 3, rank 4 and rank 7.
5. Measure all eight. Post the table. Then D1, D2 and D3 are decisions with numbers next to them.
6. Rank 8 and rank 9 if D1 is yes; rank 11 if the NTFS test passes and D3 is yes; rank 10 only if
   `cli.insert` is still slower; rank 12 from the fault counts.

## 9. What I would not do

- Change the page size default, or the leaf format. Section 4 shows the byte count is inherent to
  32 KiB leaves on a scattered read and that the per byte cost is where the gap is.
- Remove the undo records by making rollback discard frames. Section 2.7: it is a rollback redesign
  for 1 to 2 ms, and the range undo gets most of it inside the current design.
- Replace the logical log records with page images for a large transaction. Our pages are 32 KiB and
  about half full after a split, so the page images for this workload would be about 2.6 MB, the same
  as the records are now, and every autocommit workload would lose its win.
- Give up the self contained close in `journal_mode = delete` for one sync. Section 3.3.
- Time the gap constant again on a machine that is not quiet. Count compactions instead.
