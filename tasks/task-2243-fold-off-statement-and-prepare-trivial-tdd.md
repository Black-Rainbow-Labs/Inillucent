# task-2243: taking the fold off a user's statement, and what `en:prepare.trivial` should measure

A design document. Nothing in this document is implemented. It answers the two questions the eighth
performance hill climb (task-2226, `tasks/task-2226-performance-hillclimb-8.md` sections 7 and 8)
left for a design ticket, and it ends with a ranked recommendation for each and the decision each
needs from Jason.

Every number in this document is either measured in this ticket (sections 1.3, 1.4 and 10.1; the
evidence is under `_agent_output/task-2243/` in the main checkout), measured by task-2226, or marked
as arithmetic from those measurements.

## Part 1. `hc:edge.delete.range` and the fold

## 1. The problem

### 1.1 The number

`hc:edge.delete.range` is `DELETE FROM copied WHERE id BETWEEN 20000 AND 59999`
(`crates/inillucent-compat/src/perf/hillclimb.rs:565`), run once per round of the 48 workload
hillclimb plan through the `Connection`. At the end of task-2226 it read 34.3 ms against SQLite's
11.1 ms, 0.33x. This ticket's three round run read 35.4 ms against 10.8 ms, 0.31x. The delete with
no fold on it, measured in this ticket by running `ai.copy`, `edge.delete.range` and
`edge.scan.after.delete` alone so the log never reaches the bar, is 11.4 ms against 10.8 ms, 0.94x.

The rest is a fold of the pool to the file, which the statement takes on its way out because the log
has grown past `RECLAIM_BYTES`, four mebibytes (`crates/inillucent-engine/src/checkpoint.rs:160`).
The plan writes about 9.3 MB of log a round in a fixed order, so exactly two reclaiming folds land
every round: one on `ai.append` at 4.2 MB of log, one on `edge.delete.range` at 4.5 MB.

### 1.2 What a fold is, in the default `journal_mode = delete`

The release path is `release_if_idle` (`crates/inillucent-engine/src/engine/locks.rs:898`), which
folds when `fold_owed_at_release` (`locks.rs:1336`) holds: the connection holds RESERVED or more,
and either a rollback journal is hot or `since_checkpoint() >= RECLAIM_BYTES`. It calls
`checkpoint_of(false)` (`locks.rs:1027`), which is `checkpoint_within` (`checkpoint.rs:407`). In
order:

| step | what runs | where |
|---|---|---|
| 1 | raise the file lock to EXCLUSIVE, waiting up to `FOLD_PATIENCE_MILLIS` (50 ms) for readers | `checkpoint.rs:427`, `:185` |
| 2 | refresh the catalog statistics, on a reclaiming fold | `checkpoint.rs:456` |
| 3 | sync the log | `hand_over_the_log`, `checkpoint.rs:463`, `:608` |
| 4 | roll the log to a new segment, on a reclaiming fold | `checkpoint.rs:477` |
| 5 | log the changed free map pages, then sync the log again | `checkpoint.rs:524`; `crates/inillucent-txn/src/engine.rs:355` |
| 6 | `Pool::checkpoint`: seal any journal the statement already has; choose the meta route; in the `OneSync` route journal the meta and shadow pages | `crates/inillucent-pool/src/pool/fold.rs:183` |
| 7 | `Pool::flush` pass 1: for every dirty frame, read the page's old image from the file and save it to the journal; then seal the journal with a sync | `fold.rs:124`; `pool/journal_gate.rs:207`; `journal.rs:495` |
| 8 | `Pool::flush` pass 2: write every dirty frame to the file | `fold.rs:142`; `pool.rs:1144` |
| 9 | write both meta slots, then sync the data file | `pool/meta_write.rs:117` |
| 10 | finish the journal: sync it if anything was written after the seal, delete it, sync the directory | `journal.rs:519`, `:577` |
| 11 | truncate the file if the page count fell | `checkpoint.rs:535` |
| 12 | on a reclaiming fold: append a `Checkpoint` record and sync the log, then delete the segments below the recovery point | `checkpoint.rs:537`, `:218`; `crates/inillucent-wal/src/writer.rs:1037` |

The pre images in step 7 are read from the file because the pool frame holds the new content by the
time the fold runs.

The gate runs inillucent at a 32 KiB page with 4,096 frames (`fullgate.rs:269`), so every pre image
read, every journal record and every page write in the fold is 32 KiB. SQLite's arm runs at a 4 KiB
page in `journal_mode = delete`, `synchronous = full` (`compat/oracle/sqlite_bench.c:370`,
`crates/inillucent-compat/src/perf.rs:817`). SQLite pays its journal and page writes inside every
write statement, two or three syncs each. inillucent pays one log sync per statement and defers the
pages to the fold.

### 1.3 The fold's phases, measured

Task-2226 inferred the fold's split from a CPU profile, which cannot see time spent waiting in a
sync. This ticket built the release gate from the worktree with temporary timing around every phase
of `checkpoint_within`, `Pool::checkpoint` and `Pool::flush`, printed to stderr with the workload's
name when `INILLUCENT_FOLD_TRACE` was set, ran the `hc` plan for three rounds, and reverted the
timing. The run, the trace and the write up are `fold-run.txt`, `fold-trace.txt` and
`fold-split.md` under `_agent_output/task-2243/` in the main checkout. Medians of the three rounds,
in milliseconds:

| phase | `ai.append` fold | `edge.delete.range` fold | sync? |
|---|---:|---:|---|
| raise to EXCLUSIVE | 0.00 | 0.00 | |
| refresh the catalog statistics | 1.15 | 1.22 | |
| log sync 1 | 0.00 | 0.00 | yes, already durable |
| roll the segment | 0.34 | 0.29 | |
| log the free map pages | 0.02 | 0.01 | |
| log sync 2 | 1.29 | 1.24 | yes |
| pass 1, read and save the pre images | 0.07 | 3.69 | |
| of which the file reads | 0.03 | 1.43 | |
| of which `Journal::save` | 0.08 | 1.99 | |
| seal the journal | 2.34 | 6.04 | yes |
| pass 2, write the pages | 1.08 | 2.34 | |
| write the meta slots and sync the data file | 3.44 | 6.30 | yes |
| finish the journal: delete it, sync the directory | 0.17 | 0.59 | yes |
| reclaim: the `Checkpoint` record and its sync | 1.09 | 1.14 | yes |
| reclaim: delete the old segments | 0.58 | 0.65 | |
| **fold** | **11.63** | **23.62** | |

| count | `ai.append` fold | `edge.delete.range` fold |
|---|---:|---:|
| log since the last checkpoint | 4.2 MB | 4.5 MB |
| dirty frames selected and written | 69 | 201 |
| pre images journaled | 7 (229 KB) | 178 (5.8 MB) |
| dirty pages skipped in pass 1: beyond the file's length | 128 | 14 |
| dirty pages skipped in pass 1: the old image is all zeros | 0 | 36 |
| `file_size` queries | 1 per pass | 1 per pass |

Three things follow.

- **Five syncs are 15.2 ms of the 23.6.** The journal seal (6.0 ms, a 5.8 MB journal) and the data
  file sync (6.3 ms, 201 pages of 32 KiB) are 52% of the fold on their own. The log sync, the
  journal delete and the checkpoint record add 3 ms.
- **The CPU and I/O call work is 8.4 ms**: pass 1 at 3.7, pass 2 at 2.3, the statistics at 1.2, the
  segment deletes at 0.65, the roll at 0.3. Task-2242's reading of the profile put pass 1 at about
  half the fold; wall clock, it is 16%.
- **A fold has a floor of about 9 ms whatever it writes.** The `ai.append` fold journaled seven
  pages and wrote 69, and still cost 11.6 ms: the two syncs that scale with bytes were 2.3 and
  3.4 ms, and the statistics, the log sync, the checkpoint record and the finish were 3.9 ms that do
  not scale at all. This floor is what section 8.3 prices the gate change by.

The first log sync cost nothing in both folds. The statement's own commit had synced the log, so
the sync found nothing dirty, which section 1.4 measures at 22 us. The 176 times `journal.wants`
answered no in the `edge.delete.range` fold are pass 2 asking again for pages pass 1 had already
saved (`pool.rs:1248`); a set lookup each, with no measurable cost.

### 1.4 The disk, measured

To cost the cuts in section 7 without building them, a standalone probe measured the I/O shapes the
fold uses, on the C: NVMe drive the gate runs on, with the file in the OS cache. Medians of five:

| shape | time |
|---|---|
| 400 scattered 4 KiB reads, one call each | 1.15 ms |
| 400 contiguous 4 KiB reads, one call each | 0.48 ms |
| 400 contiguous 4 KiB reads, one 1.6 MiB call | 0.16 ms |
| 400 scattered 4 KiB writes, then one sync | 1.38 ms, then 12.5 ms |
| 400 contiguous 4 KiB writes, one call each, then one sync | 0.83 ms, then 3.0 ms |
| one 1.6 MiB write, then one sync | 0.42 ms, then 3.1 ms |
| 400 appends of 4 KiB to a journal, then one sync | 1.14 ms, then 3.1 ms |
| one 512 byte append, then one sync | 1.1 ms |
| a sync with nothing dirty | 22 us |
| allocate and touch a zero filled 1 MiB buffer | 0.14 ms |

Two of these matter more than the rest. The sync after scattered page writes costs four times the
sync after the same bytes written contiguously, so the data file sync in step 9 is priced by how many
separate places the dirty pages sit in, and no change to how the pages are written changes it. And a
sync of a file that already reached the disk costs 22 us, so a redundant sync is close to free and a
necessary one costs 1 to 6 ms.

## 2. Terms

| Term | Meaning in this document |
|---|---|
| fold | writing the pool's dirty pages into the `.rdb` file, with the journal and syncs that make a crash in the middle repairable. `checkpoint_within` is the whole of it. |
| reclaiming fold | a fold that also rolls the log, writes a `Checkpoint` record and deletes the old segments. A fold at release reclaims only when the log has passed `RECLAIM_BYTES`; an asked fold always reclaims. |
| pre image | the bytes a page held in the file before the fold writes it. Restoring every pre image returns the file to the state of the last fold. |
| after image | the bytes a page holds after the change. `journal_mode = wal` appends one per dirty page to the log before the fold and keeps no journal. |
| journal | `<database>-journal`: a header and a list of pre images, `crates/inillucent-pool/src/journal.rs`. |
| hot journal | a journal that a later open must restore before reading the file. Today: a journal with a valid header beside a file nobody holds EXCLUSIVE on. |
| the bar | `RECLAIM_BYTES`, four mebibytes of log since the last checkpoint. |
| meta generation | a counter in the meta record, raised once per fold and once per journal restore. Another process reads it to learn the file changed under it. |
| saturated | a client that issues its next statement the moment the previous one returns, with no idle time between them. The gate is saturated. |

## 3. Four facts that bound every design

1. **A fold writes pages in place under EXCLUSIVE, so before the first page write the fold needs a
   durable copy of either the old page or the new one.** In `delete` mode the log holds logical
   records (`DeleteRows`, `InsertRows`, `UpdateInPlace`, `SplitLeaf` carry rows, not pages,
   `crates/inillucent-wal/src/record.rs`), so a page torn by an interrupted fold cannot be repaired
   from the log. That is what the pre images are for, and why the journal is sealed with a sync
   before pass 2. `wal` mode appends after images instead and pays a log sync for them.
2. **The engine is single threaded.** `Pool` holds `RefCell`, `Cell` and `Rc` (`pool.rs:447`) and
   is not `Send`; `connect::Database` holds `RefCell<Slot>` (`connect.rs:106`); the engine crates
   have no `unsafe impl Send`. The only threads in the engine's tree are the SHARED lease releaser
   (`lease.rs:345`) and the Windows log oplock watcher (`windows_log.rs:361`), and neither touches a
   pool.
3. **Two processes coordinate through SQLite style byte range locks and the meta generation.** A
   writer holds RESERVED; a fold holds EXCLUSIVE; each `enter` compares the shadow meta slot and the
   log tail with what it last saw and, if either moved, replays the log from the file's checkpoint
   (`locks.rs:660`, `:715`, `:816`). Nothing else is shared. Two handles on one file in one process
   are kept apart the same way: on Windows byte locks are per handle (`vfs/src/os/windows.rs:6`),
   and on POSIX the vfs keeps a per process registry keyed by device and inode that arbitrates
   between handles before asking the kernel (`vfs/src/os/unix.rs:9`, `:464`, `:516`).
4. **A journal beside an unlocked file is hot, and the engine keeps that true by never releasing
   the lock with a journal outstanding.** `fold_owed_at_release` forces a fold whenever a journal
   is hot. The comment at `locks.rs:1310` records why: before task-2166, a statement whose pages
   outgrew the pool left its journal beside the file, the next process restored it over committed
   pages, and 505 of 4,800 acknowledged updates were lost with `integrity-check` answering `ok`.
   Any design in which pre images outlive a statement has to replace this rule with one that is as
   safe.

## 4. Why no engine change gets the gate's number to 1.00x

The gate is saturated: `round_through_a_connection` (`fullgate.rs:1375`) issues the next workload as
soon as the previous one returns. A fold that is deferred, split, or run on another thread does not
stop being work; in a saturated client it lands on whatever statement next needs the lock, and the
`hc` plan's next statement is `edge.scan.after.delete`. The only way the fold stops landing on a
statement is idle time at least as long as the fold between statements, which the gate does not
have and a real application often does.

What the engine can do is make the fold cheaper. Section 7 costs the cuts, and section 6 the largest
structural change short of a thread. Together, by arithmetic, they take about 10 ms off the 23.6:
every cut in section 7 (about 6 ms) and design B's removal of pass 1 (about 4 ms). The five syncs,
15.2 ms, remain under all of them, because each is priced by the disk and each is needed by fact 1.
The best engine only outcome for this statement is therefore about 11.4 plus 13.5, about 25 ms
against SQLite's 10.8, about 0.43x. The hill climb's 0.33x becomes about 0.4x and stops there.

So part 1 has two separate deliverables. Sections 5 and 6 design the product change, which removes
the spike from a user's statement in an application with idle time. Section 8 is the gate change,
which is the only thing that makes `hc:edge.delete.range` read 1.00x, and it does so by measuring
the fold where SQLite's equivalent work is measured.

## 5. Design A: a fold on a background thread, as a second connection

### 5.1 The idea in one paragraph

The engine already has a protocol for a fold run by somebody else: another process. A second
process takes EXCLUSIVE, replays the log into its own pool, folds, and the first process notices
through the meta generation and resynchronises. Design A runs that second party as a thread in the
same process, owning its own `Pool` and its own `ImportedDatabase`, so nothing in the engine is
shared across threads and fact 2 is untouched. The user's connection stops folding at release when
the bar is passed; it signals the folder and lets the file go. The folder takes the lock, folds, and
releases. The user's next `enter` adopts the fold.

Nothing stops a second `Database::open` on the same path in one process today: the "one file is one
pool" rule (`connect.rs:1352`) is enforced only by the borrow a `Connection` takes on its
`Database`, and the lock layer treats a second handle as another process (fact 3). Design A uses
exactly that.

### 5.2 The protocol

1. **Trigger.** In `release_if_idle`, when `a_fold_is_due()` holds and a folder is attached, the
   connection does not call `checkpoint_of`. It records the log tail, signals the folder's condition
   variable, and releases the lock as it does for a statement that owed nothing. A hot journal is not
   deferred: a statement that spilled still folds inline, as today, because fact 4 applies to its
   journal.
2. **The folder.** A thread started with the connection (or on the first request) that owns an
   `ImportedDatabase` opened on the same path with a small pool: the dirty set a fold writes is
   bounded by the log since the checkpoint, so a pool of 1,024 frames at the file's page size
   (32 MiB at 32 KiB pages) holds a four mebibyte replay with room; a replay that outgrows it spills
   through the existing eviction path, which is correct and merely slower. On a signal it calls
   `enter(true)` (RESERVED, then EXCLUSIVE), which replays the log from the file's checkpoint into
   its pool, then `checkpoint_of(true)` and `leave()`. A `Busy` from `enter` (the user came back
   first) is not an error: the folder sleeps until the next signal.
3. **The user's next statement.** Its `enter` finds the meta generation moved. Today that is
   `resync_from_file` (`locks.rs:816`), which calls `adopt_from_file` and `abandon_all`
   (`pool/eviction.rs:405`): every frame is evicted, clean or dirty, and the log is replayed into an
   empty pool. The comments at `eviction.rs` and `file.rs:1664` record why nothing less was safe
   when the file might be ahead of the cache. Section 5.4 gives the shortcut for the one case design
   A creates, where the file and the cache agree.
4. **Backstop.** If the user's connection reaches `4 * RECLAIM_BYTES` of log since the checkpoint
   with the folder not having folded (the client never went idle, or the folder is behind), the
   connection folds inline as today. The bar stays four mebibytes for the signal and sixteen for the
   backstop, so a saturated client sees at most one fold per sixteen mebibytes of log instead of one
   per four, and an application with any idle time sees none on its own statements. Today the log
   already grows without bound while readers keep a fold off (`FOLD_PATIENCE_MILLIS` skips it and
   nothing caps the growth), so the backstop is a new bound, not a loosened one.
5. **Waiting without a busy error.** When the user's statement needs the lock while the folder holds
   EXCLUSIVE, the user must wait, and must not fail with `Busy`. The vfs cannot tell the folder's
   EXCLUSIVE from another process's, so the connection checks its own folder's `folding` flag first
   and waits on the folder's condition variable, then takes the lock. The busy timeout (5,000 ms by
   default, `file.rs:1896`) is not consumed by a wait for one's own folder.
6. **Close.** `Drop for ImportedDatabase` signals the folder to stop, joins it, then runs
   `fold_on_close` as today. A folder mid fold finishes first; a fold is never abandoned half way.

### 5.3 Two handles on one file in one process

Fact 3 says the lock layer already handles it on both platforms. What design A adds is a test that
proves the folder's EXCLUSIVE is not dropped when the user's handle releases SHARED, on Linux and
macOS, where the POSIX registry's deferred close list (`unix.rs:437`) is what makes that true. It is
a prerequisite test, not a prerequisite change.

### 5.4 Keeping the user's pool after the folder's fold

After the folder's fold the user's pool holds the same bytes the folder just wrote, because the
folder replayed the user's own log records. Throwing the pool away, which is what `abandon_all` does,
would make every statement after a fold pay for page reads: in the gate that moves cost onto the
workloads that follow (the pool is 128 MiB and the fixture is read through it), and in an application
it turns one spike into a slow tail.

The shortcut: on `enter`, when the meta generation moved by exactly one and the log tail is where
this connection last left it (no other writer appended), every dirty frame in this pool is clean
with respect to the new file. The connection marks all frames clean (`dirty = false`,
`rec_lsn = u64::MAX`, as `writeback` does at `pool.rs:1259`), re reads the meta page and adopts the
new generation and checkpoint LSN, re reads the free map pages the fold restamped
(`engine.rs:355`), and keeps every frame. Any other combination (the generation moved by more than
one, or the log tail moved) takes today's `resync_from_file`.

The argument that this is safe: the folder's pool was built by replaying the same records this
connection applied, through the same code, so the page images agree byte for byte except for the
LSN stamp, which `writeback` restamps from the page's own LSN in both cases. A test in
`crates/inillucent-compat/tests/durability/` folds through a folder, then compares every frame of
the user's pool with the file's page.

### 5.5 What design A buys and costs

| | saturated client (the gate) | client with idle time between statements |
|---|---|---|
| the fold on the user's statement | gone from `edge.delete.range`; the wait lands on `edge.scan.after.delete` | gone |
| round total | unchanged, plus the folder's replay (CPU on another core) | unchanged |
| memory | a second pool, 32 MiB at the gate's settings | the same |
| the user's pool | kept, by 5.4 | kept |

Design A does not move `hc:edge.delete.range` toward 1.00x and does not change the round total. It
removes the fold from every statement of an application whose statements have gaps of 25 ms or more,
which is most interactive and most server applications. The backstop in 5.2 step 4 means a saturated
loop sees a quarter as many folds and each one larger, which is a change in that client's profile
that the performance page must say.

### 5.6 Risk and tests

Risk: medium. The engine's single threaded types are not shared; the new code is the folder thread's
life cycle, the wait in 5.2 step 5, and the pool shortcut in 5.4. The shortcut is the one piece that
can corrupt a file if wrong, and it has a byte for byte test.

Tests, each a new file in `crates/inillucent-compat/tests/durability/` with a row in
`tests/selection.toml`:

- a folder folds after the bar, the user's next statement adopts it, every frame equals the file;
- a user statement that arrives mid fold waits and succeeds, and never sees `Busy`;
- the backstop folds inline at sixteen mebibytes when the folder is held off;
- a crash injected (through `inillucent-sim`) in the folder's fold leaves a hot journal that the
  next open restores, and every acknowledged row is present;
- two handles in one process on Linux: the folder's EXCLUSIVE is not dropped by the user's release;
- a second process that opens the file mid fold waits, as it does today.

## 6. Design B: a pre image when a page is first dirtied, and a journal that outlives the lock

### 6.1 The idea in one paragraph

SQLite writes a page's old image to the rollback journal at the moment the page is first modified in
a transaction, while the old bytes are still in the cache, and writes the journal through to disk
once at commit. The fold's pass 1 (step 7 in section 1.2) does the same work later and worse: it
re reads every old image from the file because the frame has already changed. Design B saves the
pre image when the frame goes from clean to dirty, into the same journal buffer the fold uses, so
pass 1 disappears from the fold and the journal file fills gradually across the statements that
dirty pages. Because the engine lets the lock go between statements and only folds every four
mebibytes, that journal outlives the lock, and fact 4 has to be replaced.

### 6.2 Where the pre image is taken

The pool's path that turns a clean frame dirty (`install`, and `hold_for_writing` on a frame about to
be modified; `pool.rs`) calls `Journal::save` with the frame's current bytes before the modification,
when all of these hold: the page is below the file's page count, the frame was loaded from the file
or written by a fold (not installed by replay from a record, since then the file's bytes differ and
the pre image must be read from the file as today), and the page is not free in the free map the
file holds. The `saved` set already makes a second save of the same page a no op. A frame that was
folded (clean) and dirtied again saves again, because the journal was finished at the fold.

The pre image read of step 7 is then needed only for the replay installed frames, and pass 1 becomes
a check of the `saved` set plus the seal.

### 6.3 The journal that outlives the lock

Today's rule "hot means beside an unlocked file" cannot hold once an idle connection keeps pre images
on disk. The replacement has four parts.

**Header, version 3.** The 40 byte header (`journal.rs:130`) gains the meta generation the pre images
were taken against and a state byte: `open` (being appended to), `folding` (a fold has started
writing pages). Both are written under RESERVED. `RDBJRNL2` becomes `RDBJRNL3`; a version 2 header is
read as `folding` with an unknown generation, which is today's behaviour.

**Liveness through a lock on the journal file.** Every process that holds pre images in the journal
holds a SHARED byte range lock on one byte of the journal file for as long as it does. The operating
system releases it when the process dies. A process that wants to know whether any owner is alive
tries an EXCLUSIVE lock on that byte and releases it at once: success means no owner is alive.

**Appends are serialised by RESERVED.** Only the writer holding RESERVED on the database appends to
the journal, so two processes never append at once. A process that takes RESERVED and finds a journal
with state `open` and the file's current generation joins it: it reads the record count from the
header, truncates the journal to the counted length (a dead owner's torn last record), and appends
after it, rewriting the header count as it goes. Its `saved` set is seeded with the page ids already
in the journal, read once sequentially from the record prefixes (a six megabyte journal reads in
about a millisecond from the cache, section 1.4).

**Finishing without deleting.** The folder sets the state to `folding`, seals, writes pages, syncs the
data file, writes the meta record, and then finishes the journal by zeroing its header, as
`journal_mode = persist` does today (`journal.rs:519`). It does not delete the file: a live foreign
owner still holds a handle, and a zeroed header is what every reader already treats as "nothing
here". The
file is deleted at close by the last owner, which is the one whose EXCLUSIVE probe on the owner byte
succeeds. A live owner whose pre images were folded by somebody else finds out at its next `enter`
through the meta generation, drops its `saved` set, and starts a fresh header the next time it saves.

### 6.4 Hot or not: the rules a reader applies while holding EXCLUSIVE on the database

| header | owner alive? | meaning | action |
|---|---|---|---|
| zeroed or unreadable | any | finished, or torn before the first seal | delete if no owner is alive; otherwise ignore |
| `folding` | any | a fold started writing pages and did not finish, because a fold holds EXCLUSIVE and this reader has it | restore the pre images, raise the generation, replay the log, as today |
| `open`, generation equals the file's | yes | an idle process's pre images; no fold has started | ignore and proceed; the pages in the file are the pre images |
| `open`, generation equals the file's | no | a dead idle owner | restore (a no op for pages never written early, correct for pages the eviction path wrote early), or finish |
| `open`, generation below the file's | any | pre images of a file that has since been folded by somebody who could not zero this header | finish; never restore |

The last row is the one that protects against the corruption in section 3 fact 4: a stale journal is
recognised by its generation and never restored. The `folding` row is the one that keeps today's
crash guarantee: a fold writes `folding` and seals before the first page write, so a torn fold always
has a sealed `folding` journal, and the reader that can take EXCLUSIVE knows the folder is dead.

### 6.5 What design B buys and costs

Removed from the fold: pass 1, which is 3.7 ms of the 23.6 on `edge.delete.range` (section 1.3), and
the per page `vec![0u8; page_size]` at `journal_gate.rs:243`. Kept: every sync in section 1.3, pass
2, the meta write, the finish and the reclaim. The journal seal stays at 6 ms because the same 5.8 MB
has to reach the disk before the first page write, whenever it was written to the file.

Added to every write statement: one copy of each page it dirties for the first time since the last
fold into the pending buffer, and a buffered append to the journal file each time the buffer passes a
mebibyte. At a 32 KiB page that is a 32 KiB copy, under 3 us, per newly dirtied page. An autocommit
insert that dirties one leaf pays about 3 us on a statement that costs about 115 us in the gate,
under 3%, and `txn.autocommit` at 0.97x cannot afford it without the cuts in section 7.

In the gate: `edge.delete.range` loses about 4 ms and reads about 0.35x. Design B on its own is not
worth its risk for the gate or for the product. It is the prerequisite for one thing that is:
writing pages back before the fold, incrementally during later statements, becomes safe for any page
whose pre image is already in a sealed journal, and that is how the fold would stop being one large
write and one large sync. Task-2242's advice costed that increment at three syncs; with design B's
journal it is one seal sync per increment, 1 to 2 ms by section 1.4, which is still a tax on the
write statement that pays it, and `txn.autocommit` sits at 0.97x. The product cannot afford
incremental write back at any sync count on the user's statement. Only a background thread can pay
it, which is design A.

### 6.6 Risk and tests

Risk: high. A new journal format, a new lock on a second file in the vfs, a protocol with five
states in section 6.4 across processes, and a change to the pool's hottest path. The tests are the
five rows of section 6.4 as process level tests in `crates/inillucent-compat/tests/durability/`
(each row made true by a controlled crash or a held process, then an open from a second process),
plus the existing `process_journal_handoff`, `process_crash` and `fold_protocol` suites, which must
keep passing. This is a ticket of its own and it should not be the next one.

## 7. The low risk cuts to the fold, costed

These change no format and no protocol. Each line gives the saving on the `edge.delete.range` fold,
by arithmetic from sections 1.3 and 1.4.

| cut | where | what it saves on this fold | risk |
|---|---|---|---|
| 7.1 skip pages free in the new free map in pass 1 and pass 2 | `fold.rs:125`, `journal_gate.rs:207` | the emptied leaves: about 50 of the 201 pages at 32 KiB (40,000 rows deleted, about 800 rows a leaf). About a quarter of pass 1 and pass 2 (1.5 ms) and of the journal bytes (1.6 MB, about 1.5 ms of the seal). The data sync changes little: a bulk built table's leaves are contiguous, the cheap kind of sync. About 3 ms. | medium, see below |
| 7.2 read pre images of contiguous dirty pages in one call, write contiguous dirty pages in one call | `journal_gate.rs:243`, `pool.rs:1248` | about 1 us per call saved on 178 reads and 201 writes, 0.4 ms; the syncs are unchanged (section 1.4) | low |
| 7.3 one reusable scratch for the pre image read instead of `vec![0u8; page_size]` per page | `journal_gate.rs:243` | 178 zero filled 32 KiB allocations, about 0.8 ms (0.14 ms per MiB) | low |
| 7.4 refresh the catalog statistics only when the catalog changed since the last fold, or every fourth reclaiming fold | `checkpoint.rs:456` | 1.2 ms on every reclaiming fold | low; the statistics are advisory |
| 7.5 do not sync the `Checkpoint` record; delete the old segments at the next fold instead | `writer.rs:1037`, `checkpoint.rs:218` | 1.1 ms | medium, see below |
| 7.6 skip the first log sync when the log is already durable | `checkpoint.rs:463` | nothing: measured at 0.00 ms, it already finds nothing dirty | none |

**7.1 needs a change in redo first.** `free_emptied_leaf` (`crates/inillucent-tree/src/write/unlink.rs:343`)
logs `FreePage`, then installs an empty leaf image as a dirty frame so that a reader holding the page
as a nearby leaf cannot find old rows in a page the free map owns. The fold writes that empty image.
If 7.1 skips it, the file keeps the old full leaf under a page the free map says is free. That is
safe only if a later allocation of the page never lets the old rows show: `Applier` handles
`AllocPage` by noting a map change and installs no image (`crates/inillucent-txn/src/redo.rs:1340`,
`:1214`), so replay of an `InsertRow` into the reallocated page would apply to the stale leaf by the
page LSN rule. 7.1 therefore requires redo to install a fresh, correctly stamped empty image on
`AllocPage` (the stamp question is the one `write_built_page`'s comment at `fold.rs:350` records
having bitten once), and a crash test that frees a leaf, folds, crashes after reallocating it and
inserting, and checks the rows. With that change 7.1 is the largest cut available.

**7.5 is not safe as the code stands.** Recovery starts from the meta record's `checkpoint_lsn`, not
from the `Checkpoint` record (`checkpoint.rs:207`), so the record itself is not what the sync
protects. What it protects is the new segment: `open_segment` writes the segment header without a
sync (`writer.rs:1817`, "every record appended here is made durable by a sync of this same file"),
and `retire_segments_below` deletes the old segments right after. Without the sync a crash in that
window leaves a log whose newest segment's header is not on disk and whose older segments are gone.
The safe form of 7.5 keeps the header's durability by deleting the old segments at the next fold,
after a later commit or fold has synced the new segment. Then the record needs no sync of its own.
It is a small change with a crash test, and it saves 1.1 ms per reclaiming fold.

**`roll_segment` is not a cut.** The comment at `checkpoint.rs:150` prices creating a segment at
2.4 ms; measured here it is 0.3 ms. The comment is from a different machine or an earlier code path
and should be corrected when the file is next touched.

**Sum.** About 6.5 ms of the 23.6 by arithmetic (7.1 at 3, 7.4 at 1.2, 7.5 at 1.1, 7.3 at 0.8,
7.2 at 0.4), leaving about 17 ms on the fold and about 28.5 ms on `edge.delete.range` against
SQLite's 10.8, about 0.38x. The five syncs are 15 ms of what is left, and no cut in this table
touches them.

## 8. The gate change

### 8.1 What the gate measures today, and why the per statement ratio is the wrong unit for durability work

SQLite in `delete` mode does its page writes, its journal and two or three syncs inside every write
statement. inillucent does one log sync inside every write statement and the page writes, journal
and the remaining syncs at the fold, inside whichever statement crosses the bar. So the per statement
ratio favours inillucent on every write statement that does not fold (`edge.update.all` at 1.01x has
not paid for its pages) and punishes the two that do (`ai.append` carries 11.6 ms of fold,
`edge.delete.range` 23.6). Neither direction is a fair reading of one statement. The round total is
fair: over a round both engines have written every page and paid every sync.

### 8.2 The change

1. **Checkpoint between workloads, outside every clock, on both arms.** After each workload's `post`
   (`round_through_a_connection`, `fullgate.rs:1375`; `sqlite_bench.c:313`), the gate runs
   `PRAGMA wal_checkpoint` on both arms with its own clock. On inillucent that is `checkpoint_of(true)`
   (`pragma/integrity.rs:233`), a reclaiming fold whatever the log size. On SQLite in `delete` mode
   it is a no op that returns `busy|-1|-1`, measured at a few microseconds. A checkpoint at round end
   alone is not enough: the round writes 9.3 MB of log and crosses the bar twice, so a fold would
   still land on a statement mid round.
2. **Report the sum as its own line, `hc:checkpoint`.** Both arms, like any workload. Because the
   SQLite arm reads near zero, the ratio is far below 1.00x (or dropped when exactly zero,
   `perf.rs:333`, `to-flow.mjs`). It is excluded from the "cases at or above SQLite" count and
   included in the round total, and the report says both in its header, so the cost is visible and
   counted and does not pretend to be a statement.
3. **Report the round total for the `hc` plan as the headline pair.** inillucent against SQLite,
   every workload and the checkpoint line summed, with its own ratio. That is the number section 8.1
   argues is fair.
4. **Say it on the performance page.** `docs/performance.md` states that inillucent defers page
   writes to a checkpoint and SQLite in rollback mode does not, that the per statement lines compare
   statement work and the `hc:checkpoint` line and the round total carry the durability work, and
   gives the 48 checkpoints a round as a count.

With this, `hc:edge.delete.range` measures the delete, 11.4 ms against 10.8, 0.94x by this ticket's
control run, and the fold's cost is in `hc:checkpoint` and the round total. The delete itself is
then a 6% gap for a later climb, not a design question.

### 8.3 Costs of the change

- 48 asked folds a round on inillucent instead of two. Section 1.3 measured the floor of a fold at
  about 9 ms (the `ai.append` fold wrote 69 pages and cost 11.6 ms), so the `hc:checkpoint` line will
  read about 48 times 9 to 12 ms, 450 to 600 ms a round, against a round total of a few seconds. The
  cuts in section 7 (7.4 and 7.5 alone are 2.3 ms of every fold) and an asked fold that does not
  reclaim when the log is small (`checkpoint_of(false)` exposed to the gate) both reduce it.
- The pool is folded after every workload, so the next workload starts with a clean pool. That is
  the state SQLite's arm is in after every statement, so it makes the arms more alike, not less.
- Every `hc` ratio moves a little, because the fold no longer lands on `ai.append` or
  `edge.delete.range`. A fresh baseline is needed, which the hill climb does anyway.

### 8.4 A variant: SQLite's arm in `wal` mode

Running the SQLite arm with `journal_mode = wal` and `wal_autocheckpoint = 1000` (four mebibytes at
4 KiB pages, the same bar) would make SQLite defer its page writes to a checkpoint that lands on a
statement, as inillucent does, and no checkpoint line would be needed. It is not recommended: it
changes every SQLite number in the gate, because WAL reads and commits have a different cost
profile, it compares two checkpoints that land on different statements because the two logs
grow at different rates, and the performance page's claim is against SQLite's default mode. It is
listed because it is the only other way to make the arms alike, and Jason may prefer it.

## 9. Ranked recommendation for part 1, and the decision

1. **The gate change, section 8.** It is the only change that makes `hc:edge.delete.range` read
   near 1.00x, it is a few hundred lines in `fullgate.rs`, `sqlite_bench.c`, `to-flow.mjs` and
   `docs/performance.md`, and its argument (section 8.1) is one the performance page can state in
   two sentences. It needs Jason's decision because it changes what the headline count counts: 116 cases
   become 116 statements plus a checkpoint line that is reported, summed, and not counted.
2. **The low risk cuts, section 7, in the order 7.4, 7.3, 7.2, then 7.5 with its deferred segment
   deletion, then 7.1 with its redo change.** They make every fold cheaper for every application and
   shrink the `hc:checkpoint` line. 7.4, 7.3 and 7.2 are one small ticket on `opus[1m]`; 7.5 and 7.1
   each carry a crash test and are their own tickets.
3. **Design A, section 5, as the product change.** It removes the fold from a user's statement in
   any application with idle time, shares nothing across threads, and reuses the cross process
   protocol. Its one delicate piece is the pool shortcut (5.4), with a byte for byte test. It does
   not change the gate's number and the ticket that builds it should say so up front.
4. **Design B, section 6, not now.** Its gain is 4 ms of a 23.6 ms fold, its risk is a new cross
   process protocol, and the thing it enables (incremental write back) still costs a sync on a
   user's statement. Keep the design for a ticket that wants the fold spike smaller for its own
   sake, after design A exists to pay for the increments.

**The decision Jason has to make.** Whether the gate should charge the fold at the point SQLite is
charged for the same work (section 8: the checkpoint line, and the round total as the headline) or
keep charging it to the statement that crosses the bar and accept about 0.4x on
`hc:edge.delete.range` as the best any engine change in sections 5 to 7 can reach. If section 8 is
taken, the second decision is whether the checkpoint line is excluded from the count (recommended)
or kept in it as a permanently failing case.

## Part 2. `en:prepare.trivial`

## 10. What the workload measures

### 10.1 The numbers, per iteration

The gate's `prepare.trivial` is `SELECT 1` with `prepare_each: true`, repeated 4,000 times
(`perf.rs:880`). The reported 2.91 ms against 1.62 ms is the total for 4,000 iterations: 728 ns
against 405 ns each, 0.56x.

Through the engine API (`time_read`, `fullgate.rs:2445`) each iteration is `database.plan(sql)`
(parse, bind, plan), `database.prepare(&plan)`, a boxed `DigestRows` sink, `database.pipeline(..)`
(build the operator chain) and `pipeline.run()`. No cache is involved; the comment at
`fullgate.rs:716` says so on purpose. SQLite's arm (`sqlite_bench.c:313`) does `sqlite3_prepare_v2`,
`sqlite3_step` to done, a digest and `sqlite3_finalize` each iteration. Both arms compile from text
every time, run, and digest. The pair measures the same thing.

Through the `Connection` (`cc:prepare.trivial`) the same text is a cache hit after the first
iteration (`Compiled::statements`, `state.rs:1165`; `compiled_with_parameters`, `plans.rs:381`),
and the iteration is a hash lookup, a `Statement` construction, a step and a digest: 245 ns, 1.66x.

The stage split and the allocation counts, from `inillucent-prepareprofile` on this ticket's build
(`_agent_output/task-2243/prepareprofile.txt`; cumulative nanoseconds, so each stage is the
difference from the one before; the profile uses the system allocator where the gate uses a pooled
one, so its total is higher than the gate's 728):

| stage | cumulative ns | the stage alone | allocations |
|---|---:|---:|---:|
| parse | 354 | 354 | 8 when parsing alone, 3 inside a compile with the recycled scratch |
| bind | 605 | 251 | 3 |
| plan | 576 | 0 (noise) | 0 |
| physical (`prepare`) | 591 | 15 | 0 |
| pipeline (`build_chain`, the sink, `run`) | 892 | 301 | 7 |
| whole gate path, quietest batch of 100 | 861 | | 13 |

Parse is 40% of the iteration and 354 ns on its own, which is 87% of the time SQLite takes for the
whole statement. For eight characters and three tokens that is fixed cost, not parsing: the
construction of the `Parser`, its lookahead buffer, and whatever `Ast::clear` and the scratch reset
do per call. The compile ticket should profile that first, because every statement any program
compiles pays it.

By reading the path, the gate's iteration makes 13 allocations today, not the 21 the comment at
`state.rs:1201` records (that figure predates the task-2026 reductions; `docs/closed-items.md:231`
has the 13, and the profile confirms it):

| # | where | what | fixed or per column |
|---|---|---|---|
| 1 | `inillucent-sql/src/parser/mod.rs:183` | the parser's lookahead token buffer | fixed |
| 2 | `parser/select.rs:329` | the result column list | per column |
| 3 | `parser/expr.rs:61` | the integer literal's bytes, copied from the text | per literal |
| 4 | `bind.rs:2663` | the bound result columns | per column |
| 5 | `bind.rs:3007` | the default column name `"1"` | per column |
| 6 | `bind.rs:1526` | `Box<BoundSelect>` | fixed |
| 7 | `inillucent-exec/src/physical/chain.rs:664` | the projected expressions | per column |
| 8 | `chain.rs:1089` | the compiled evaluators | per column |
| 9 | `expr/tree.rs:604` | `Box<Literal>` | per expression node |
| 10 | `chain.rs:1096` | `Box<Project>` | fixed |
| 11 | `chain.rs:1404` | the output names list | per column |
| 12 | `chain.rs:1407` | each output name, cloned | per column |
| 13 | `fullgate.rs:2460` | `Box<DigestRows>`, the harness's sink | fixed, harness |

At 20 to 30 ns an allocation that is 260 to 390 ns of the 728.

### 10.2 Should the engine plan keep measuring an uncached compile? Yes.

- **It is what the SQLite arm measures.** `sqlite3_prepare_v2` compiles from text every iteration.
  A plan cache on the engine arm would compare a hash lookup against a compile, and that comparison
  already exists: it is `cc:prepare.trivial`, 1.66x.
- **Programs compile from text more than the "no program calls this" framing allows.** The
  `Connection`'s cache is cleared when it reaches 1,000 entries and on every schema change and
  function registration (`plans.rs:49`, `:381`), so a program with more than a thousand distinct
  statement texts, an agent issuing varied SQL over MCP, a migration, or anything that interpolates
  values into its text compiles continually. `en:prepare.trivial` is the cost of a cache miss for the
  simplest possible statement, which is the floor under every compile.
- **The statement is the right trivial statement.** `SELECT 1` is the liveness probe every
  connection pool sends, so the floor is measured on the text that is sent most often.

What should change is the label. The gate's header already says the pipeline arm does not use the
plan cache; the report line and `docs/performance.md` should say "compile from text, no cache, both
arms" beside `prepare.trivial` so the 0.56x is read as a compile, and the `cc` line as a hit.

### 10.3 Which cuts get it to 1.00x

To read 1.00x the iteration has to lose 323 ns of 728. Two sources are measured: the 13 allocations,
and the parser's 354 ns fixed cost.

**The allocation cuts**, 13 to 5:

| cut | allocations removed | how |
|---|---|---|
| A | 1 (#1) | keep the parser's lookahead buffer in the recycled `Ast` scratch (`Compiled::scratch_ast`, `state.rs:1181`), which already survives between compiles |
| B | 1 (#3) | lex an integer literal to its value, or keep a byte range into the SQL text, instead of copying its bytes into a `Vec<u8>` |
| C | 1 (#5) | the default column name for a literal is the literal's text; borrow it from the SQL text or use an inline small string |
| D | 2 (#11, #12) | hand the output names from the bound statement to the chain by move or `Rc` instead of cloning each |
| E | 2 (#7, #8) | inline storage for up to four projected expressions and evaluators (a small vector type), falling back to the heap above that |
| F | 1 (#13) | the gate reuses one `DigestRows` across iterations, as the SQLite arm keeps its digest on the stack; harness code inside the clock |

Left: #2 and #4 (per column lists owned by the AST and the bound statement; both could take inline
storage too, which makes the target 3), #6 `Box<BoundSelect>`, #9 `Box<Literal>`, #10
`Box<Project>`. The three boxes are the operator tree's own nodes and are not worth changing.

Arithmetic: 8 allocations at 25 ns is 200 ns, 728 to about 530 ns, about 0.76x.

**The parser's fixed cost.** 354 ns to parse `SELECT 1` with three of its eight allocations
already recycled is 280 ns of work that is not allocation. Candidates by reading, to be confirmed
by a profile of `inillucent-prepareprofile --sql "SELECT 1"` under the Visual Studio collector:
`Parser` construction per call (its fields at `parser/mod.rs:129` include the lookahead buffer and
the parameter bitset), `Ast::clear` walking a recycled arena (`ast.rs:1452`), and `bump` removing
from the front of the lookahead vector (`parser/mod.rs:249`, a shift on every token). If half of the
280 ns is fixed setup that can be done once per `Compiled` instead of once per parse, the iteration
falls to about 390 ns, about 1.04x, with the allocation cuts. This is the cut with value to every
compile, and it is where the ticket should start. Its number is a target, not a measurement, until
the profile is taken.

**The literal select fast path.** After the parse, when the statement is a single `SELECT` of
literals with no `FROM`, `WHERE`, `GROUP BY`, `ORDER BY`, `LIMIT` or set operation, build the
constant row plan and the operator chain directly, skipping the binder and planner. By the profile
that removes bind (251 ns) and part of the pipeline build: 530 ns falls to about 300 ns, about 1.35x.
Column names, declared types and affinities must match the general path, and `SELECT 1 WHERE 0`
must not take it. Risk: low to medium, and the test is `differential/semantics.rs` on
`SELECT 1, 'a', NULL, 1.5, x'00'`, `SELECT 1 AS n`, `SELECT 1 WHERE 0`, `SELECT 1 UNION SELECT 2`.

What the fast path is worth outside the gate: little. Through the `Connection` `SELECT 1` is a cache
hit. Through the engine API it is the gate. It helps a program only on a cache miss of a literal
select, which is a liveness probe on a connection whose cache was just cleared.

## 11. Ranked recommendation for part 2, and the decision

1. **Keep measuring the uncached compile, and label it.** Section 10.2. No code change beyond the
   report line and the performance page.
2. **Take the allocation cuts A to F and the parser's fixed cost, section 10.3.** They help every
   compile of every statement, and they are measured by
   `inillucent-prepareprofile` and bounded by the budget test (`crates/inillucent/tests/budget.rs:230`,
   `TRIVIAL_COMPILE_ALLOCATIONS`), which comes down to the new count. Expected result: 0.76x from
   the allocations alone, about 1.0x if half the parser's fixed cost is setup that moves out of the
   per call path, and a few percent on every `prepare_each` workload. Risk low. One ticket on
   `opus[1m]`, which profiles the parser first and reports the split before cutting.
3. **The literal select fast path only if the parser profile does not get there and Jason wants
   116 of 116.** It is low risk with the differential tests named, and it helps no program much.

**The decision Jason has to make.** Whether `en:prepare.trivial` short of 1.00x after the general
compile cuts is acceptable, with the line labelled as an uncached compile, or whether the count
matters enough to add a fast path whose value is the count. The TDD recommends the general cuts
first and the fast path only on their result.

## 12. Files each recommendation touches

| recommendation | files |
|---|---|
| gate change (section 8) | `crates/inillucent-compat/src/bin/fullgate.rs` (`round_through_a_connection`, `round_on`, `report_results`), `compat/oracle/sqlite_bench.c` (`run_workload`), `_agent_output/task-2226/tools/to-flow.mjs` or its successor, `tools/perf-hillclimb/score.mjs`, `docs/performance.md`, the gate's own tests in `tests/selection.toml` if they assert the line format |
| cuts (section 7) | `crates/inillucent-pool/src/pool/fold.rs`, `pool/journal_gate.rs`, `pool.rs`, `crates/inillucent-engine/src/checkpoint.rs` (7.4, the `roll_segment` comment), `crates/inillucent-wal/src/writer.rs` (7.5), `crates/inillucent-txn/src/redo.rs` (7.1), `crates/inillucent-compat/tests/durability/fold_protocol.rs` and a new crash test each for 7.1 and 7.5 |
| design A (section 5) | `crates/inillucent-engine/src/engine/locks.rs` (`release_if_idle`, `enter_within`), a new `crates/inillucent-engine/src/engine/folder.rs`, `crates/inillucent-engine/src/lib.rs` (`Drop`), `connect.rs` (the one pool comment), `crates/inillucent-pool/src/pool.rs` (adopt a fold), new tests under `crates/inillucent-compat/tests/durability/`, `docs/relational-architecture.md`, `docs/performance.md` |
| design B (section 6) | `crates/inillucent-pool/src/journal.rs` (header v3, state, joining), `pool.rs` (save at first dirty), `pool/journal_gate.rs`, `crates/inillucent-engine/src/engine/open.rs` and `locks.rs` (the hot rules), `crates/inillucent-vfs` (a lock on the journal file), tests as in 6.6, `docs/relational-architecture.md` |
| compile cuts (section 10.3) | `crates/inillucent-sql/src/parser/mod.rs`, `parser/expr.rs`, `ast.rs`, `bind.rs`, `crates/inillucent-exec/src/physical/chain.rs`, `crates/inillucent-engine/src/engine/state.rs` (the comment), `crates/inillucent/tests/budget.rs`, `crates/inillucent-compat/src/bin/fullgate.rs` (F and the label), `docs/performance.md` |

## 13. Evidence

- `_agent_output/task-2243/fold-split.md`, `fold-tables.md`, `fold-trace.txt`, `fold-run.txt`: the
  instrumented three round `hc` run, main checkout. The instrumentation was reverted and is not in
  the repository.
- `_agent_output/task-2243/alone-run.txt`, `alone-trace.txt`: the five round control with
  `--workloads ai.copy,edge.delete.range,edge.scan.after.delete`, where the log stays at 570 KB and
  no fold runs.
- `_agent_output/task-2243/prepareprofile.txt`: the stage split and allocation counts for
  `SELECT 1` and the point read.
- `_agent_output/task-2226/fable/advice.md` section 8 and `tasks/task-2226-performance-hillclimb-8.md`
  sections 7 and 8: the numbers this ticket started from.
- The disk probe (section 1.4) ran from this session's scratch directory and its source was not
  kept in the repository; the table is the record.
