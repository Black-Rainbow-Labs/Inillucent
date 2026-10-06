# task-2191 guidance: how to be faster than SQLite in every usage workload

Written for task-2191 by task-2192 on 5 October 2026. It is guidance, not a change. Every claim
about SQLite comes from reading `.sqlite-ref/3.53.4/src/sqlite3.c` and `shell.c`; every claim
about inillucent comes from reading the task-2191 worktree as it stood at 08:00 on 5 October, mid
edit, so line numbers can move. The four reading reports with the full citations are under
`_agent_output/task-2192/` in the main checkout:

| Report | What it covers |
|---|---|
| `sqlite-insert-open.md` | `OP_NewRowid`, `OP_Insert`, `sqlite3BtreeInsert`, `insertCellFast`, `balance_quick`, `balance_nonroot`, `pager_write`, `sqlite3_open_v2`, `process_input`, `.import`, lookaside |
| `inillucent-insert-log.md` | the leaf format, `put_absent` step by step, `make_room`, splits, the log record, the undo record, `highest_rowid`, index puts |
| `inillucent-open-log-ownership.md` | every system call in `open`, the segment layout, who owns the log, what a deferred open would move |
| `inillucent-shell-prepare-addons.md` | `drive`, the literal lift, the plan cache, the Python and Node bindings, the `inillucent` process before `main` |

Numbers quoted as "ours / SQLite" are task-2191's round 2 numbers against the pinned 3.53.4 shell,
measured on a machine that was not quiet. Nothing in this document was measured by task-2192.

## 1. The short version

The remaining gaps are five, and they have three causes.

| Gap | Ours / SQLite | Cause |
|---|---|---|
| 100,000 single row `INSERT`s in one transaction, through the shell | 285 ms / 148 ms | per row work in the tree and the executor (section 3), the prepare and shell overhead per statement (section 5) |
| Python `execute_many`, 10,000 rows, two TEXT indexes | 57 ms / 24 ms | the same tree work, three times per row, plus mid leaf compaction in 32 KiB leaves (section 4) |
| one `INSERT` of 20,000 rows, `.import --csv` of 50,000 rows | 87 / 34 ms, 146 / 55 ms | the same insert path, plus a parse of 20,000 rows (section 5.3) |
| open and close from Python | 300 us / 77 us | about twenty system calls where SQLite makes two (section 6) |
| the `inillucent` process | 0.6 to 2.6 ms slower a call | the open, plus work before and after the query that a one row query does not need (section 7) |

Ranked by expected gain per unit of risk. The gain column is my arithmetic from the profile
percentages and the code, not a measurement, and section 3.4 shows the arithmetic.

| Rank | Change | Where the gain is | Expected gain | Risk |
|---|---|---|---|---|
| 1 | One fetch and one parse per row instead of five or six fetches and five parses; scratch buffers reused across rows instead of eight to ten allocations a row | every insert workload | 8 to 12% of the shell case, similar for `execute_many` | low |
| 2 | A `may_abort` flag computed at compile time, and no undo record when it is false; an arena for the undo records when it is true | every insert workload | 4 to 6% shell; more for `execute_many`, which writes three undo records a row | low to medium |
| 3 | The literal lift's hit path without the rewritten `String`, the `sql.to_string()`, the token `Vec` regrowth and the second clone of every value; and the lift extended to multi row `VALUES` | shell scripts, the 20,000 row `INSERT` | 6 to 9% shell; the 20,000 row case roughly halves | low |
| 4 | The shell keeps its buffer's capacity and scans each statement once | shell scripts | 3 to 5% | low |
| 5 | A delta policy with a guaranteed gap after compaction, and an append split that tries nothing before it splits | every insert workload, most for indexes | `make_room` from 10% to about 3% in the shell case and from 14% to about 4% in the index case | medium |
| 6 | One binary search per index put instead of two, when the key cannot already exist | `execute_many` with indexes | 5 to 7% of the index case | medium |
| 7 | One lock per statement around the log appends instead of three per record | every insert workload | 3 to 4% | low to medium |
| 8 | The open cut in section 6: one handle at open, everything else at the first statement | open and close, the command line | 300 us to about 100 us | medium |
| 9 | The command line list in section 7, measured first with the trace points task-2191 already added | the command line | up to 1 ms a process | low |
| 10 | A delta directory that does not move on every insert | appends and index puts | 1 to 2% | format change, do later |

Ranks 1 to 7 together bring the shell case from 285 ms to somewhere between 170 and 190 ms by the
arithmetic in section 3.4. That does not reach 148 ms. Reaching it needs the rest of the profile,
the 38% that the round 2 comment did not attribute, to be read and worked the same way. Section
3.5 says what I think is in it.

The two direct questions:

- **Compaction and delta policy, and the two records per row** (section 4 and section 8). A per
  row logical log record is the right cost and is why inillucent wins every autocommit workload by
  247% to 531%. A per row undo record is not: SQLite keeps none for a statement that cannot abort,
  and neither should we. The delta design is sound; its policy constants are what make a 32 KiB
  leaf rewrite every live row every fifty inserts, and those constants are the change.
- **Deferring the open** (section 6). Defer everything but the database file handle, the format
  check and the meta read. The log segment has no owner of its own: the writer is whoever holds
  RESERVED on the database file, and the per statement tail check is a `file_size` call. Opening
  the segment write handle at the first append, under RESERVED, is safer than opening it at open,
  not less safe. The errors that move to the first statement are the corrupt log, a page stamped
  above the log's end and an unreadable catalog, and SQLite moves the same class of error to the
  first prepare.

## 2. What SQLite does that we do not

This is the comparison the ticket asked for, one row per mechanism. Line numbers are in
`sqlite3.c` unless marked `shell.c`. Paths on our side are under `crates/`.

| Mechanism | SQLite | inillucent |
|---|---|---|
| Finding the next rowid | `OP_NewRowid` (102195) calls `sqlite3BtreeLast`, which returns at once when the cursor is flagged `BTCF_AtLast` (78999), then parses one cell (`getCellInfo`, 78116). A descent happens only after a split, because `sqlite3BtreeInsert` invalidates the cursor after `balance()` (82915) | `highest_rowid` is a cached hint validated by one fetch and four header reads (`inillucent-tree/src/paged.rs:944-956`), then `put_absent` fetches the same page again, and `note_appended_key` fetches it a third time (`write.rs:790-800`, `paged.rs:985-1000`). Three fetches of one page per row |
| Keeping a position between rows | the cursor stays on the leaf and the slot. `sqlite3BtreeTableMoveto` skips the seek when the cached key is below the new one and `BTCF_AtLast` is set (79046 to 79056) | `leaf_hints` remembers up to eight leaf windows per tree (`paged.rs:684`, `726`), not a slot. Every row runs `locate_slot` again in both halves of the leaf (`leaf/read.rs:442-457`) and parses the leaf about five times (`write.rs:909-960`, `1177-1195`, `1066-1119`) |
| Putting a cell into a leaf with room | `insertCellFast` (80653): one `memcpy` of the cell and a `memmove` of `2*(nCell-i)` bytes of slot pointers, which is 0 bytes for an append. No existing cell body moves. `defragmentPage` runs only when the gap is too small (`allocateSpace`, 75107) | `apply_delta` (`inillucent-tree/src/leaf/mutate.rs:501-558`): one copy of the row, and a `memmove` of the whole delta directory, `2*delta_count` bytes, wherever the key lands, because the directory grows towards lower addresses |
| A full leaf under appends | `balance_quick` (82410 to 82415): a new right sibling receives the one overflow cell and nothing else moves | `choose_fit` (`write.rs:1819-1895`) tries a splice (every live row copied), then a repack (every value materialised and re encoded), then a split that copies the page, materialises every value again, runs a measuring pack and encodes two full pages (`write.rs:2319-2460`). About five passes over a 32 KiB leaf. Before the split, a leaf near `TIGHT_FILL` compacts again every few dozen inserts |
| A full leaf under random inserts | `balance_nonroot` (81548): up to three siblings, cells moved by `editPage` only when few move (81098), pages come out about 75% full | the same `choose_fit`; `SPLIT_FILL` 0.50 (`write.rs:262`) gives two half pages, and `TIGHT_FILL` 0.95 (`write.rs:292`) means the next compaction comes after about 1.6 KiB of delta rows |
| Undo for a failed statement | none for a statement that cannot abort. `sqlite3MayAbort` sets `mayAbort` at compile time, and `OP_Transaction` opens a statement journal only when `usesStmtJournal` is set and the statement runs inside a transaction (`sqlite3BtreeBeginStmt`). An I/O error rolls back the whole transaction (`sqlite3VdbeHalt`, `isSpecialError`) | `with_write_view` always passes `undo = Some(..)` (`inillucent-engine/src/engine/compiled.rs:1320`). One `Before` with a heap allocated key per row and per index entry (`engine/write.rs:429-453`), held until commit inside a transaction |
| What the log holds per row | nothing. The rollback journal holds one page image per page per transaction, decided by the `pInJournal` bitvec (`pager_write`, 65770); WAL holds dirty pages at commit (`pagerWalFrames`, 66193) | one `InsertRow` record: a 32 byte header, tree, page, length, the row, padding to 8 and a crc32 (`inillucent-wal/src/record.rs:482-510`, `640-642`), appended under three mutex acquisitions (`writer.rs:546-577`). This is the better design for autocommit and the costlier one per row in a large transaction |
| Index key comparison | `vdbeRecordCompareString` (92940): a `memcmp` of the first field before any full compare, chosen by `sqlite3VdbeFindCompare` (93010) | `compare_key_with` (`leaf/compare.rs:53-75`): a zero copy `Datum` from the column slot, no allocation, a byte compare for TEXT. Comparable per compare. The difference is the count: two binary searches per put, about 20 to 25 compares, half of them decoding tagged values from scattered delta rows (`leaf/delta.rs:426-445`) |
| Prepare of a script statement | a full parse and code generation per statement, made cheap by lookaside (`1200,40`, 24072; a pop from a free list, 32120), a `Parse` on the stack (148599) and a parser stack on the stack (179915). No statement cache | a cache hit after the lift (`inillucent-engine/src/plans.rs:287-318`), which still lexes every token into a `Vec`, builds a rewritten `String`, hashes it with SipHash, allocates a `Vec<u8>` per text and integer literal, clones the original text in `Connection::prepare` (`engine/connect.rs:1238`) and clones every value again at step (`engine/statements.rs:559-560`) |
| The shell's line loop | `process_input` (`shell.c` 36097): `quickscan` for a trailing `;`, then `sqlite3_complete`; `shell_exec` prepares, steps and finalizes with no cache | `drive` (`inillucent-cli/src/shell.rs:1176-1228`): one `String` per line, the completeness check only on lines with `;` or `*/`, then `std::mem::take(&mut pending)`, which drops the buffer's capacity so it is regrown for every statement (`:1216`), and two scans per statement (`:1238-1247`, `:1289`) |
| Open | `sqlite3OsOpen` of the database file (64629), a 100 byte header read (75911), a 48,000 byte lookaside allocation (191058). No lock, no journal, no WAL, no schema. The schema loads at the first prepare that names a table (126961, 148105). A close that wrote nothing syncs nothing (63857) | about twenty system calls: section 6 |

## 3. Single row inserts into the table tree

### 3.1 What one row costs today

`inillucent-insert-log.md` section 2 lists the fifteen steps of `put_absent` for a row appended at
the right edge of a rowid tree with the leaf hint set and room in the delta gap. The parts that
are not in SQLite's path at all:

- **Five or six page table lookups and five leaf parses of the same page.** `hinted_largest_key`
  fetches the rightmost leaf (step 1). `leaf_for_hinted` fetches it again (step 6).
  `locate_and_read_previous` fetches and parses it (step 7). `room_for_write` calls `pool.modify`
  for a question that only reads, parses the leaf in `LeafMut::new`, parses it again in
  `delta_shape`, and loops over every column in `columns_end` (step 8, `write.rs:1177-1195`,
  `mutate.rs:1168-1185`). `apply_row` calls `modify` again and parses again (step 12).
  `note_appended_key` fetches a third time (step 15). SQLite touches the page once, through a
  cursor that already points at it.
- **Eight to ten allocations per row.** `key: Vec<Datum>` and `encoded_key` (`write.rs:1291-1292`,
  through `encode_key`, not the inline `encode_key_small`), `encoded_row` and `runs`
  (`write.rs:853-854`, where `runs` is `vec![None; row.len()]` even when nothing can spill), the
  undo key `Vec<OwnedDatum>` with its cloned text, and in the executor the evaluated values `Vec`,
  the built image `Vec<OwnedDatum>` and `borrowed: Vec<Datum>` (`inillucent-exec/src/dml/insert.rs:944`).
  SQLite's `OP_MakeRecord` writes into a register that is reused for every row.
- **Two generation bumps and two dirty marks per row**, one of them from `room_for_write`
  using `modify` for a read (`pool.rs:1485-1505`, `pool/merged.rs:97-102`).
- **Two copies of a 250 byte `WriteStats` struct** through a `Cell` (`write.rs:1245-1256`).
- **One `Before` record** with a heap allocated key (section 8).
- **Three mutex acquisitions per log record** (`writer.rs:547-549`, `855-863`).

### 3.2 Rank 1: a cursor that lasts for the statement

The change is to make `write_row` do what SQLite's cursor does: find the leaf and the slot once,
then do everything with that one parse.

1. `locate_and_read_previous` returns the parsed `LeafRef`, the slot and the page. `room_for_write`
   takes that parse and computes `room_for_row_and_tombstone` from it, with `pool.fetch` rather
   than `pool.modify`, so the read asks no generation bump and no dirty mark. `apply_row` then
   calls `modify` once and reuses the offsets. One fetch and one `modify`, one parse.
2. `put_absent`'s largest key check and `note_appended_key` go. `write_row` already knows the key
   sorts past every key in the leaf when `locate_slot` returns `Err(delta_count)` and
   `Err(row_count)`, and `apply_row` has the page and the LSN it stamped (`write.rs:1059`,
   `1116`). Return those to `put_absent` and update `largest_hint` from them. That is zero extra
   fetches instead of three.
3. Keep a per tree scratch struct on the `PagedTree` handle, next to `leaf_hints`, holding `key`,
   `encoded_key`, `encoded_row` and `runs`, cleared and reused per row. Use `encode_key_small`
   for the one column integer key. In the executor, keep the built image and `borrowed` on the
   `InsertCache` the same way. Allocation count per row goes from eight to ten to about zero on
   the steady state.
4. `note_one_write` updates the counters in place behind a `RefCell`, or keeps `WriteStats` as
   separate `Cell<u64>` fields, so nothing copies 250 bytes twice a row.
5. After a split at the right edge, do not clear every hint and descend. The split knows the new
   right page; set the hint to it (`write.rs:2455`, `paged.rs:1020-1030`).

Risk is low: nothing about the format, the log or recovery changes. The tree's write campaign
(`inillucent-tree/tests/write_campaign.rs`) and the crash campaigns are the tests.

### 3.3 Rank 7: one lock per statement around the log

`Wal::append` takes the `inner` lock, the `io` lock inside `roll_if_full`, and `inner` again, for
every record. The shell is one thread; the C library may be called from several. Add an appender
guard that `WalLog` opens once per statement, holds the `inner` lock for the statement's records,
and checks the roll condition once with a byte budget instead of per record. The crc32 per record
stays; it is about 80 ns on an 88 byte record with slice by 8, which is 3% of the row, and section
8 says why I would not move it.

### 3.4 The arithmetic for the shell case

The round 2 profile of 100,000 single row inserts in one transaction: `insert_rows` 44%
(`put_absent` 36%: `make_room` 10%, `apply_row` 8.6%, `room_for_row_and_tombstone` 3.6%, undo
4.2%, the remaining 9.6% in locate, encode and fetches), prepare 17% (the lift 8%), shell scanning
9%, and 30% not named.

| Change | Share today | Share after | Saved |
|---|---|---|---|
| rank 1, fetches and parses and allocations | about 12% across `put_absent`'s remainder, `room_for_row`, `insert_rows` minus `put_absent` | 4% | 8% |
| rank 2, no undo for a statement that cannot abort | 4.2% | 0 | 4% |
| rank 3, the lift hit path | 17% | 9% | 8% |
| rank 4, the shell loop | 9% | 5% | 4% |
| rank 5, the append split | 10% | 3% | 7% |
| rank 7, the log appends | 7% | 3.5% | 3.5% |
| total | | | about 35% |

285 ms less 35% is 185 ms. SQLite is 148 ms, or 1.48 us per statement including a full parse and
code generation. Ours after these is 1.85 us. The rest is in the 30% the profile did not name and
in the floor of the design: a logical record per row with a checksum, a 32 KiB page whose delta
directory moves per insert, and a compaction pass per page instead of a cell copy. Section 3.5 and
section 10 say what to do about each.

### 3.5 What the unnamed 30% probably holds

Read from the code, to be confirmed by a full profile tree posted on the ticket:

- `ImportedDatabase::write` per statement: `next_txn`, `statement_mark` (three `RefCell` borrows,
  `engine/batch.rs:691-697`), `with_write_view` building a `WalLog` (an `Rc` clone and an `Arc`
  clone) and a `WriteView`, then `Outcome { rows: changes.returned.clone(), names: Rc::new(..) }`
  (`compiled.rs:1284-1288`). Per statement, so 100,000 times.
- `Shell::run` per statement: `Instant::now()`, `driver::arm` with an `Arc` clone, `collect_bound`
  ending in `columns().to_vec()`, `is_query_plan`, `is_bytecode_explain`, `rendering_layout`
  (`shell.rs:817-922`, `998-1084`). None of these is needed for a statement that returns no rows.
- `check_keys_at_statement_end`, `follow_modules`, `remember_rowid`, `record_changes` per
  statement (`compiled.rs:1265-1288`).
- Reading standard input a line at a time through `stdin().lines()`, one `String` per line
  (`main.rs:421`).

## 4. Index puts and the delta policy

### 4.1 What a mid leaf index put costs today

For `execute_many` of 10,000 rows into a table with indexes on `tag` and `created`,
`write_index_entry` is 36% of the run and the table put 14%. Per index entry
(`inillucent-insert-log.md` section 6):

- `locate_slot` runs two binary searches (`leaf/read.rs:442-457`): about eleven compares over
  the sorted region of a 1,782 row leaf, then eight to eleven compares over the delta directory,
  where every compare decodes a tagged value from a delta row that sits wherever it arrived in a
  32 KiB page. Twenty to twenty five compares where one search would make eleven.
- `apply_delta` moves the whole delta directory, `2*delta_count` bytes.
- `make_room` runs when the gap is full. After a compaction the image is at most `TIGHT_FILL`
  (0.95) of the page, so the gap is about 1.6 KiB, which holds about 55 index rows of 25 tagged
  bytes. Then every live row of the leaf is rewritten again. That is roughly 1,500 rows rewritten
  per 55 inserts, or 27 rows of copying per insert, amortised. SQLite's equivalent is a 38 byte
  `memmove` of slot pointers in a 4 KiB page and a split every 38 rows that `editPage` does by
  moving only the cells that move.
- Eight to ten allocations and several copies of the TEXT key: `entry_from`'s cloned
  `OwnedDatum::Text`, `borrowed`, `key`, `encoded_key`, `encoded_row`, `runs`, and the undo
  `Before` with its own clone of the text (`index.rs:85-150`, `write.rs:1291-1292`, `853-854`,
  `engine/write.rs:440-443`).

### 4.2 Rank 5: a delta policy with a guaranteed gap

The delta design amortises the rewrite of a leaf over the rows that arrive between compactions.
Its cost per insert is `leaf_rows / rows_between_compactions`. Today the second number is set by
`TIGHT_FILL`, and 0.95 makes it about 55 for an index leaf. The change is a policy constant, not
a format change: a guaranteed gap `G` after every compaction, and a split when the packed image
would leave less than `G` free.

- With `G` at 12.5% of the page, 4 KiB, an index leaf compacts every 140 rows instead of every
  55, and the amortised rewrite per insert drops from about 27 rows to about 10. With `G` at 20%
  it is about 6.
- Space: leaves hold at most `1 - G` packed. At 12.5% that is 6% more index pages than today;
  SQLite's random insert leaves sit at about 75% full, so this is still denser than SQLite.
- The directory `memmove` and the delta search grow with the delta count: 140 entries is a 280
  byte move and eight compares. Both are small next to the rewrite saved.
- The append case is separate, and section 4.3 covers it.

Measure it on the `execute_many` workload with `G` at 1.6 KiB (today), 4 KiB and 6.5 KiB, and on
`inillucent-fullgate`'s index workloads so a read regression from more leaves shows. The
constant lives in `write.rs:292` and `choose_fit`'s fit test (`write.rs:1970-2017`).

A cheaper experiment first, with no code change: build the fixture at page size 8,192, which is a
legal size (`inillucent-pool/src/page.rs:80`), and run the same workload. A 32 KiB leaf is 8
times the rows of an 8 KiB one and every compaction is 8 times the work. If the index case
improves a lot at 8 KiB, that is the measurement that justifies the gap constant, and it also
says how much of the gap comes from the leaf size and how much from the policy.

### 4.3 Rank 5, the append half: a split that moves nothing extra

At the right edge, `is_appending` is already known (`write.rs:1903-1957`), and the sequence per
page today is several splices with geometrically shrinking intervals, a repack every fourth
splice (`SPLICE_LIMIT`, `leaf.rs:188`), then a split that first tries and fails a splice and a
compaction, copies the page (`leaf.bytes().to_vec()`, `write.rs:2015`), materialises every value
again, runs a measuring pack and encodes two pages.

SQLite's `balance_quick` is: the left page stays as it is, the new right page gets the overflow.
Ours cannot leave the left page as it is, because the delta rows must be packed into the sorted
region once. It can do that once:

1. Keep a packed size estimate for the leaf: the sorted region's bytes plus `delta_count` times
   the mean packed row width, both readable from the parse. When `is_appending` and the estimate
   is above `APPEND_FILL`, go to `Fit::Split` directly. No splice attempt, no compaction attempt,
   no page copy.
2. The split encodes the left image once from the live rows and gives the right page only the
   rows that did not fit. That is the existing `split_carrying` without the failed attempts before
   it and without `Fit::Split`'s page copy.
3. Between splits, under appends, let the delta area take the whole gap and splice once, which
   already costs one pass. The splice is the compaction that is cheap; the repack is the one that
   materialises. Under pure appends the values always fit the existing slot widths unless a wider
   integer arrives, so `SPLICE_LIMIT` is forcing a repack that gains nothing. Lift the limit when
   `is_appending`.
4. Set the hint to the new right leaf (section 3.2, item 5).

Passes over the leaf per page go from about eight (three splices, one repack, the failed splice
and compaction, the copy, two encodes) to about four, and the next insert does not descend.

### 4.4 Rank 6: one binary search per index put

`locate_slot` searches both halves because a key can be tombstoned in the sorted region and
live in the delta area. For a secondary index put from `place_row_absent`, the key is
`(indexed columns, rowid)` and the rowid is new, so the key cannot be live anywhere in the leaf.
It can be a tombstone in the sorted region only when the rowid was reused after a delete, and
then the delta row wins on read ("a key that appears twice keeps the newer row first",
`leaf/delta.rs:12-24`) and compaction removes the older one. So the sorted region search exists
only to find the slot for the tombstone check, and for this caller it can be skipped: find the
directory position in the delta half and insert. Verify the two tie rules named here before
relying on them, with a test that deletes the highest rowid, inserts again with the same
indexed value, and reads the index back.

When the key can exist (`put` with `replace`, a unique index, an update), search the sorted
region first and consult the delta directory only when the key falls inside the delta's key range,
which the first and last directory entries give in two compares.

## 5. The statement path for scripts

### 5.1 Rank 3: the lift's hit path

The lift is the right idea and it already turns a parse into a cache hit. What a hit still does
per statement (`inillucent-shell-prepare-addons.md` part A):

- lexes every token into `rows: Vec<Token>`, regrown from empty each time (`lift.rs:72-79`);
- builds a `Vec<u8>` per text literal and a temporary one per integer (`lift.rs:187-220`);
- builds the rewritten `String` with a `to_string()` per value (`lift.rs:223-236`);
- hashes the rewritten text with SipHash and probes two maps (`plans.rs:386-390`);
- maps `Vec<LiftedValue>` to a second `Vec<OwnedDatum>` (`plans.rs:302-316`);
- `Connection::prepare` clones the original text, allocates `Params`, an `Rc<Vec>` for names and
  a rows `Vec` (`engine/connect.rs:1231-1249`);
- step clones `params` and clones every datum again into `refill` (`statements.rs:559-560`).

The changes, all local to `lift.rs`, `plans.rs` and `prepare`:

1. Keep the token `Vec` and the lifted values as scratch on the connection, cleared per call.
2. Hash the template while lexing, as a 64 bit hash over the token kinds and the non literal
   token bytes, and key the cache by that hash with the template compared on hit. No rewritten
   `String` on a hit; build it only on a miss, when the parser needs text.
3. Bind the lifted values straight into the statement's parameter slots by move. No `Params`
   clone and no second clone at step.
4. `Connection::prepare` stores the text only when something will read it (`sql()` on the
   statement, the trace, an error message); otherwise keep an `Rc<str>` from the cache entry.

### 5.2 Rank 4: the shell loop

- `std::mem::take(&mut pending)` drops the buffer's capacity (`shell.rs:1216`). Keep two buffers
  and swap, or hand `run_chunk` a slice and `clear()` afterwards.
- `ends_in_complete_statement` scans the chunk and `run_chunk` scans it again (`shell.rs:1238-1247`,
  `1289`). Return the end offset from the first scan.
- `Shell::run` runs `Instant::now()`, `arm`, `is_query_plan`, `is_bytecode_explain` and
  `rendering_layout` for a statement that returns no rows (`shell.rs:826-889`). Branch on
  `columns().is_empty()` first.
- Read standard input through a locked `BufReader` with `read_line` into one reused `String`
  in place of `stdin().lines()` (`main.rs:421`).

SQLite's shell does none of this work per statement and still pays a full parse. After rank 3 and
rank 4, ours should pay less per statement than SQLite, because the parse is the expensive part
and we skip it.

### 5.3 The 20,000 row `INSERT` and `.import`

One `INSERT` of 20,000 rows is 87 ms against 34 ms because the parser builds an AST per
literal and the planner sees 20,000 rows. SQLite parses them too, through a stack parser with
lookaside, and compiles the `VALUES` list as a coroutine that yields one row at a time, so its
memory stays flat. The lift already splits the token stream into rows (`lift.rs:111-200`). Extend
it: when every row is plain literals and the rows have one width, compile the one row template
once and run it once per row inside the statement, under the statement's own undo so ABORT still
undoes every row. `MOST_VALUES` (64) becomes a limit per row instead of per statement. The
planner never sees the 20,000 rows and the per row cost becomes the insert path of section 3.

`.import --csv` (146 ms against 55 ms for 50,000 rows) already goes straight to the insert path
and the CSV parser lends fields. The gap is the engine's per row cost, so it moves with ranks 1,
2, 5 and 7. SQLite's `.import` binds every field with `SQLITE_TRANSIENT`, which copies, and runs
one prepared statement per row (`shell.c` 31350, 31384), so its per row cost is its insert path
plus a copy per field. There is nothing to copy from it beyond what the lending parser does.

## 6. Open and close

### 6.1 What open does today, against SQLite

SQLite's `sqlite3_open_v2`: `CreateFileW` of the database file, a 100 byte read at offset 0, a
48,000 byte lookaside allocation. No lock, no journal, no WAL, no schema. Everything else waits
for the first statement, which takes SHARED, checks for a hot journal with a stat and a 16 byte
read, and loads the schema at the first prepare that names a table. A close that wrote nothing
syncs and writes nothing.

Ours, for an existing database with a small log (`inillucent-open-log-ownership.md` section 1):

| Step | System calls |
|---|---|
| two existence checks (`drivers/inillucent-driver/src/lib.rs:435`, `engine/connect.rs:420`) | 2 stats |
| the hot journal check (`inillucent-pool/src/journal.rs:773`) | 1 stat |
| `doubtful_transactions` reads the marker file with `read_to_string` (`engine/multi.rs:74`) | 1 failed `CreateFileW` |
| handle 1: the database file, read and write, create (`inillucent-pool/src/file.rs:506`) | 1 stat, 1 `CreateFileW`, 1 `GetFileInformationByHandle` |
| SHARED: lock PENDING, lock the shared range, unlock PENDING (`file.rs:524`) | 3 lock calls |
| the format reads (`file.rs:536-557`) | 3 small reads |
| the two meta pages (`file.rs:575-577`) | 2 reads of 32 KiB |
| `Pool::new` (`file.rs:584`) | a 32 KiB allocation |
| catalog read 1 (`recovery.rs:281`) | page reads through the pool |
| the log chain: stat, handle 2 read only, `file_size`, 64 byte header, the body from the checkpoint, stat of the next sequence (`inillucent-wal/src/recover.rs:353-437`) | 2 stats, 1 `CreateFileW`, 1 `GetFileInformationByHandle`, 1 `GetFileSizeEx`, 2 reads |
| `load_free_map` (`recovery.rs:390`) | page reads |
| handle 3: `Wal::open`, read and write, create (`inillucent-wal/src/writer.rs:1283`) | 1 stat, 1 `CreateFileW`, 1 `GetFileInformationByHandle`, 1 header read |
| catalog read 2 (`recovery.rs:916`), `load_schema` as catalog read 3 (`engine/open.rs:522`), `rebuild_tables`, `refresh_catalog` | page reads, `CREATE` text parsed |
| `finish_the_open`: `file_size` (`file.rs:601`) | 1 `GetFileSizeEx` |
| `end_access`: unlock (`file.rs:1228`) | 1 unlock |

About twenty system calls and three handles against SQLite's two calls and one handle. Three of
the opens are write capable, and task-2191 measured 30 to 40 us for the open and close of each
write capable handle. One likely cause is the real time scanner: Windows Defender scans a file
when the last handle opened with write access closes, even when nothing was written. That is
cheap to test: exclude the benchmark directory from real time protection, run `py.open.close.100`
again, and see whether the 30 to 40 us per write handle goes. Whatever the cause, not opening a
write handle the connection does not use removes it.

### 6.2 The cut

Neither "defer the whole engine open" nor "only open the segment write handle lazily" is the right
line. The right line is SQLite's: at open, open the one file whose absence or shape is an error
the caller must see before any statement; everything else at the first statement.

**At open:**

1. The not found stat, as today.
2. Handle 1, the database file. Open it read and write when the connection is write capable, as
   today, because SQLite does the same and the handle costs nothing until it is used.
3. The format reads and the meta read, under SHARED, as today. This is what makes "not a
   database", "newer format", "wrong key", "torn meta" and "permission denied" errors of open.
   Then unlock, as today.
4. If the hot journal stat finds a journal, run the full eager open, because the journal must
   replay before the meta record can be trusted (`open.rs:490-495`).

**At the first statement**, inside the lock that statement already takes, and that already
reads the meta record again and checks the log tail because open sets `trusted = false`
(`open.rs:690-710`, `engine/locks.rs:83-250`):

5. The marker check, as a stat. Today it is a `read_to_string` of a file that is normally absent.
6. Recovery: the log chain read with its handle, the replay, the free map.
7. The catalog and the schema, once, not three times. `read_checkpointed_catalog` before redo
   exists to detect a catalog page the log must repair; after the cut it runs only on the error
   path, as its own comment at `recovery.rs:268-270` already intends.
8. The segment handle: see 6.3.

**What moves to the first statement:** a corrupt log, a page stamped above the log's end, an
unreadable catalog, and a stale `wal_sequence`. SQLite moves the same class, a corrupt schema and
a corrupt WAL, to the first prepare and the first read transaction, and no user of SQLite expects
otherwise. The connection's `recovery()` report (`drivers/inillucent-driver/src/lib.rs:478`)
answers after the first statement or runs the deferred part itself when asked first.

**Tests that assert the error at open and will need a statement sent:**
`crates/inillucent-compat/tests/engine/analyze_reopen.rs:445`
(`a_page_stamped_above_the_logs_end_refuses_the_open`),
`crates/inillucent-compat/tests/durability/file_damage.rs:163`, the open time cases in
`crates/inillucent-compat/tests/durability/fault_sweep.rs` near line 126, and the encryption
cases in `crates/inillucent-compat/tests/engine/encryption_at_rest.rs:187` onward, which should
stay at open because the key check is part of step 3. `crates/inillucent-pool/src/file.rs:2038`
stays at open as well. The C ABI's status table (`drivers/inillucent-driver-capi/tests/abi.rs:378-383`)
does not change: the statuses stay, the moment moves.

### 6.3 Log ownership, and why the lazy write handle is the safe direction

The worry in the round 2 comment was multi process ownership of the log. The code answers it
(`inillucent-open-log-ownership.md` section 2):

- A segment file carries no lock, no owner field and no lease. `crates/inillucent-wal` holds
  only in process mutexes (`writer.rs:229-236`). Any process can open a segment read and write.
- The writer is whoever holds RESERVED on the database file through SQLite's byte ranges
  (`inillucent-vfs/src/os/ranges.rs:14-23`, `windows.rs:543-714`). Appends happen under RESERVED.
  A fold needs EXCLUSIVE. Readers hold SHARED and keep reading under `locking_mode = normal`.
- The guard against two processes appending at one offset is not on the segment. Before each
  statement that takes the lock, `the_meta_moved()` and then `the_log_moved()` run
  (`engine/locks.rs:452`, `507-514`); the second is `file_size` on the connection's own segment
  handle compared with `written_end` (`writer.rs:951-963`), and a move triggers
  `resync_from_file`, which replays and builds a new `Wal` (`locks.rs:623-632`).
- `Wal::open` writes nothing in the normal case. It writes a header and syncs only when the
  segment is missing, short, foreign or ahead of the resume position (`writer.rs:1347-1362`),
  and today it does that under SHARED, so two processes opening the same damaged segment can both
  truncate it.

So the write handle provides no ownership. Ownership is the database lock, and the segment handle
is only the thing the append goes through and the thing the tail check measures. That gives the
design:

1. Open the segment once at the first statement, as the deferred recovery's handle, read and
   write when the connection is write capable. `read_chain` already has the last segment open
   with its header decoded (`recover.rs:353-437`); hand that handle to `Wal` through `Recovered`
   instead of opening a third one. One segment handle per connection instead of two.
2. `tail_of_open_segment` keeps working on that handle, so the per statement check is unchanged.
   Do not replace it with a path based size check: the earlier version of that cost 3.2 ms a
   statement (`writer.rs:1188-1196`).
3. A read only connection opens the segment read only. It never appends and its `the_log_moved`
   works on a read handle as well as a write handle.
4. The header write for a missing or foreign segment moves under RESERVED, at the first append,
   which removes the double truncate.

The sequence stale check already protects a write handle opened late: a new segment cannot
appear without the meta record moving, and the meta check runs first (`writer.rs:945-950`).

### 6.4 What is left in the open after the cut

One stat, one `CreateFileW`, three lock calls, three small reads, two 32 KiB reads, one unlock, the
`Pool::new` allocation. That is about nine calls against SQLite's two, and the remaining
difference is the lock and the meta pages, which are what let open refuse a torn or foreign file.
Two further cuts, if the number still matters:

- Read the two meta pages with one 64 KiB read, or read the primary and read the shadow only
  when the primary fails its check.
- `Pool::new` at 60 us is a 32 KiB scratch allocation and a frame table. SQLite's 48 KB lookaside
  is a single `malloc`. If the 60 us is page zeroing, allocate the scratch on first use.

## 7. The command line process

Process start is level (15.4 ms against 15.3 ms after the static C runtime). The 0.6 to 2.6 ms
left is after `main`. What runs on every start that a one row query does not need
(`inillucent-shell-prepare-addons.md` part C), in the order I would measure them with the
`__cli` and `__ctr` trace points task-2191 already added:

1. The open, 300 us today, about 100 us after section 6.
2. `strays_beside` does a `read_dir` of the database directory and reads any stray segment it
   finds (`command/mod.rs:513`, `643`). A `FindFirstFileW` over a directory with many entries
   costs more than the open. Run it only for the commands that report on strays, or only when the
   meta record's `wal_sequence` says a segment was left behind.
3. `on_a_sized_stack` spawns a 64 MiB stack thread and joins it for every command
   (`bin/inillucent.rs:78-82`). The reserve is cheap; the thread creation and the join are about
   100 us. Run `query` and `exec` on the main thread and spawn only for the commands that recurse
   (the shell, `dump`, `migrate`).
4. `Context::open_for` builds the whole `Shell` struct, registers the `fsdir` and `zipfile`
   virtual table modules and sets two connection options (`shell.rs:322-371`, `396-452`). A query
   needs a connection.
5. `refuse_a_script` runs `statement_length`, a full `parse_next_statement`, and then a
   `prepare`; `collect_bound` prepares again as a cache hit (`shell.rs:1111-1119`,
   `connect.rs:998-1005`). One prepare with a tail check is enough.
6. Each `--params` value runs its own `collect("SELECT <literal>")`, a prepare and a run per
   parameter (`verbs.rs:195-205`). Parse the JSON array into values directly, as `--params` on
   `exec` already carries JSON.
7. `command::find` allocates a `String` per command entry per argument, about 68 allocations
   for `--db x.rdb query` (`command/mod.rs:849-858`). Microseconds, but free to fix: compare
   with the dash mapped on the fly.
8. `INILLUCENT_TRACE_OPEN` is read with `env::var_os` at every trace point. Read it once.
9. `SetConsoleCtrlHandler` at every start. Install it only for commands that can run long.

Items 1 to 4 are where the time is; 5 to 9 are small. The `sqlite3` shell does the open, one
prepare, the steps and the print, and nothing else.

## 8. The storage format and the log: what caps single row inserts

The ticket's third question, with a verdict per item.

| Item | What it costs per row | Verdict |
|---|---|---|
| The logical `InsertRow` record: 32 byte header, 16 bytes of tree and page, 4 byte length, the row, padding to 8, a crc32 | about 88 bytes written and a crc over them for a 30 byte row; in bytes it is less than SQLite's rollback journal (two 4 KiB writes per 38 rows, about 215 bytes a row) and about the same as WAL (108 bytes a row) | keep it. This is why a single row autocommit costs one 88 byte append and one fsync here, and a 4 KiB page frame plus an fsync in SQLite's WAL, and why `shell.insert.200.autocommit`, `py.insert.100.autocommit` and `py.update.1k` win by 247% to 531% |
| The crc32 per record | about 80 ns, 3% of a row | keep it for now. A checksum per transaction would be sound, because a transaction's records are useless without its commit record, but it is a log format change that touches recovery and the crash campaigns for 3% |
| Three mutex acquisitions per append | about 60 ns uncontended, more with the atomic and the roll check | rank 7 |
| The undo `Before` per row and per index entry | one allocation and one clone of the key per entry; 300,000 entries held for the 100,000 row transaction | rank 2: compute `may_abort` at compile time, as SQLite's `sqlite3MayAbort` does, and record nothing when it is false. A single row `INSERT` with no triggers, no foreign keys and no unique secondary index cannot fail after its first write for a reason the statement should survive; an I/O error rolls back the whole transaction, as it does in SQLite. A multi row statement can fail on its third row and must keep the undo, but the undo can be an arena of encoded key bytes, which `write_row` already computes, in place of a `Vec<OwnedDatum>` per entry |
| The delta directory that grows towards lower addresses, so every insert moves every entry | `2*delta_count` bytes; 2 KiB per insert near the end of a 1,100 row append run, about 40 ns | rank 10. Reserve the directory's capacity after each compaction so an append writes its entry in place, or grow the directory upwards. Both are page format changes; do it when another format change is being made, not for 1 to 2% |
| 32 KiB leaves with a 0.95 fill after compaction | a rewrite of every live row every 55 index inserts, about 27 rows of copying per insert | rank 5: the gap constant. The leaf size itself is a trade: an 8 KiB leaf makes every compaction 8 times cheaper and the tree one level deeper for large tables. Measure before deciding, as section 4.2 says |
| Page LSN, dirty mark, generation bump per `modify` | small, but doubled by `room_for_write` using `modify` for a read | rank 1 |
| Page checksum | computed at writeback and spill only (`pool/page.rs:17-35`) | nothing to change |
| Copy on write, per row timestamps, `max_cts` | none on the insert path | nothing to change |

The one sentence answer: the format does not cap single row inserts; the policy constants and the
per row bookkeeping around the format do. No format change is needed to reach the numbers in
section 3.4, and the one that would help (the directory) is small.

## 9. The in process Python extension and the Node addon

The question was whether a Python extension and a Node N-API addon written in Rust without new
crates, resolving the CPython stable ABI and the `napi_*` symbols from the host process at run
time, are sound, and what to watch for. What exists in the worktree
(`inillucent-shell-prepare-addons.md` part B):

- **Python is not an extension module.** It is `ctypes.CDLL` over the C library, and the C library
  builds Python objects in `inillucent_rows_py` from eleven addresses Python hands it once:
  nine stable ABI functions (`PyList_New`, `PyList_SetItem`, `PyLong_FromLongLong`,
  `PyFloat_FromDouble`, `PyUnicode_FromStringAndSize`, `PyBytes_FromStringAndSize`, `Py_IncRef`,
  `Py_DecRef`, `PyErr_SetString`) taken from `ctypes.pythonapi`, plus `id(None)` and
  `id(RuntimeError)` (`drivers/bindings/python/inillucent.py:412-441`,
  `drivers/inillucent-driver-capi/src/capi/python.rs:34-123`). That one call goes through
  `ctypes.PyDLL`, so it holds the GIL; every other call goes through `CDLL` and releases it.
- **Node is an addon in the C library itself.** `napi_register_module_v1` is exported from the
  shared library, the npm package loads it with `process.dlopen`, and the 24 `napi_*` functions
  are resolved at run time with `GetProcAddress(GetModuleHandleW(null), name)` on Windows and
  `dlsym(RTLD_DEFAULT, name)` elsewhere (`capi/node.rs:84-162`). A missing symbol registers
  nothing and the package falls back to a process.

### 9.1 Verdict

Both are sound, and both are the approach I would have proposed.

**Node.** A `.node` file built the usual way is a shared library whose import table names
`node.exe`; resolving the same symbols with `GetProcAddress` on the host executable is the same
binding done later. Electron exports them from `electron.exe`, Bun and Deno export them from
their executables, and a statically linked Node exports them with `-rdynamic`, so `dlsym` with
`RTLD_DEFAULT` finds them on Linux and macOS. The N-API functions are a C ABI with a versioned
table and a compatibility promise, which is the whole reason N-API exists. Loading the C library
itself as the addon is fine: `process.dlopen` accepts any path, and `napi_register_module_v1` is
the entry Node looks for.

**Python.** Handing the addresses in from `ctypes.pythonapi` is more robust than resolving them
in Rust, because the library never has to find the interpreter's DLL, and every function used is
in the limited API, so one build serves every CPython from 3.9 on. The `restype = py_object`
conversion in ctypes takes the returned pointer as a new reference and does not increment it
(`Modules/_ctypes/callproc.c`, `GetResult`, the special case for the `O` getter), so returning the
new reference from `PyList_New` is right and returning a borrowed one would be a double decref.
Keep returning new references, and add a test that `sys.getrefcount` of a returned list is what
a fresh list gives.

### 9.2 What to watch for, Node

1. **Every `napi_status` is checked.** Most are discarded in `result_of` and `js_value`. After a
   pending JavaScript exception, every later call returns `napi_pending_exception` and a null
   handle, and the code passes those on. Return at the first non `napi_ok` status.
2. **`panic = "abort"` in the release profile** (`Cargo.toml:55-58`) makes `catch_unwind` in
   `guarded` do nothing, so a Rust panic ends the host process, and the `js_*` callbacks are not
   wrapped in `guarded` at all. The governed crates deny `unwrap`, `expect`, `panic!` and slice
   indexing on data paths, which is the real defence. Build the C library under a profile that
   inherits `release` with `panic = "unwind"`, so `catch_unwind` means something in a library that
   lives inside somebody else's process; `packaging/release-all.ps1` would name that profile for
   the library only.
3. **`worker_threads`.** Each worker loads the same library and calls `napi_register_module_v1`
   again. `LIVE_HANDLES` is process wide (`drivers/inillucent-driver-capi/src/lib.rs:255-264`), so
   a handle number opened on the main thread is accepted from a worker, and the engine's
   connection is single threaded. Record the thread that opened each handle and refuse a call from
   another with a status, or keep the handle table per environment through
   `napi_set_instance_data`.
4. **Cleanup at environment exit.** There are no finalizers and no cleanup hook, so a connection
   left open when Node exits is never closed, and the next open pays the recovery.
   `napi_add_env_cleanup_hook` closing that environment's handles is the fix.
5. **Handle count.** One `query` holds one `napi_value` per cell until it returns. For a
   100,000 row by 10 column result that is a million handles in one scope. Open a
   `napi_handle_scope` per row or per thousand rows, escaping the row array.
6. **The host without exported symbols.** Node embedded as `libnode.dll` or `libnode.so` inside
   another program exports nothing from the executable. When `GetModuleHandleW(null)` misses, try
   the module that exports `napi_module_register` by walking the loaded modules; on Unix,
   `dlsym(RTLD_DEFAULT)` already searches every loaded object. The process fallback covers the
   rest, and the N-API version in use (BigInt needs version 6, Node 10.7 and later) is below the
   `>=18` engines floor.
7. **Strings.** `napi_create_string_utf8` makes V8 validate and possibly widen. For a value that
   is ASCII, `napi_create_string_latin1` gives V8 a one byte string without the scan. Measure on
   `py.scan.all`'s Node twin before adopting it.

### 9.3 What to watch for, Python

1. **`id()` is not the stable ABI.** `id(None)` is the object's address only in CPython. Take the
   two objects the way the limited API does: `ctypes.c_void_p.in_dll(ctypes.pythonapi,
   "_Py_NoneStruct")` for `None` and `in_dll(..., "PyExc_RuntimeError")`, which is a `PyObject**`,
   for the exception. On 3.13 and later `Py_GetConstant` exists as well. Skip the fast path when
   `sys.implementation.name != "cpython"`; PyPy's `ctypes.pythonapi` is not the same thing.
2. **The GIL and threads.** `CDLL` releases the GIL, so two Python threads can be inside the
   engine on one connection at once, and the engine's connection is single threaded. Either a
   `threading.Lock` per connection in the binding or `check_same_thread`, as the standard
   library's `sqlite3` does. `inillucent_rows_py` must only ever be called through `PyDLL`, and
   must never call the engine or block, which it does not.
3. **Free threaded CPython (3.13t, 3.14t).** The nine functions remain valid, `Py_IncRef` and
   `Py_DecRef` are functions so the reference count layout does not matter, and a ctypes call
   from a Python thread is from an attached thread, so building objects is allowed. The limited
   API is not available for free threaded builds, but this code links nothing, so that does not
   apply. Test one free threaded interpreter in the wheel's matrix.
4. **Extension module speed without an extension module.** A `.pyd` with a `PyInit_` entry is
   not needed to get past ctypes's half microsecond per call. `PyCFunction_NewEx` is in the
   limited API, and `PyMethodDef` has a stable layout, so the library can build a builtin
   function object around a Rust function with `METH_FASTCALL` and hand it back once; Python
   then calls into Rust at builtin speed with no ctypes marshalling. `METH_FASTCALL` joined the
   limited API in 3.10, so it needs the wheel's floor raised from 3.9 to 3.10, or `METH_VARARGS`
   below 3.10. The five calls a point lookup makes today (prepare lookup, bind, step, rows, reset)
   are about 2.5 us of ctypes, against 15 us a lookup measured, so this is a 15% gain on
   `py.point.select.5k` and nothing on the batched workloads.
5. **Sub interpreters.** The addresses are per process and the same in every interpreter, so the
   `OnceLock<PyApi>` is right. Objects built in one interpreter must not be handed to another,
   which the binding never does.

## 10. What I would not do

- Change the page size default, the leaf format or the log record format for the numbers in
  this document. Section 8 says why each is a small gain that touches many tests.
- Replace the logical log record with page images in a large transaction. It would move the
  per row cost to commit, as SQLite's WAL does, and lose every autocommit win.
- Defer the format and meta checks past open. "Not a database", "wrong key" and "newer format"
  are open errors in SQLite too, and every caller expects them there.
- Keep working the shell case from the profile that names 70% of it. Post the whole tree first.

## 11. The order to do it in

1. Sections 6 and 7 first, on the trace points: they are measured in isolation and their gains
   do not depend on the tree work.
2. Rank 1 and rank 2, together, because both touch `write_row` and `record_undo`.
3. Rank 3 and rank 4, together, in the shell and the lift.
4. The 8 KiB page experiment, then rank 5 with the gap constant chosen from it.
5. Rank 6 and rank 7.
6. Profile the shell case again and post the tree before deciding what, if anything, is left.
