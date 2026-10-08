# task-2209: a seventh performance hill climb, on common operations

## 1. What was asked

Make inillucent 50% faster on common operations: select, join, group by, insert and
`inillucent_search`, by a hill climb, asking Fable for advice where needed.

Midway, Jason pointed at the build eval and hillclimb workflows of the bundled `claude-api` skill. The
rest of the climb follows that guide: a fixed eval with a train and test split, a recorded baseline,
one change per round with its rationale, a noise floor measured before trusting a round, and a flow
directory with a report. A `hillclimb` skill in claude-settings now routes future hill climbs there.

## 2. The eval

| Part | What it runs | Cases |
|---|---|---:|
| the gate, both plans | `inillucent-fullgate --api connection --rounds 10`, the contract plan and the hillclimb plan, SQLite 3.53.4 timed in the same rounds, every answer's digest compared with SQLite's | 82 |
| the search probe | `inillucent-searchprobe` (new in this ticket): 5,000 rows of 384 numbers in an `inillucent_search` table; insert in one transaction, 100 keyword, vector and hybrid searches, 100 cycles of an insert then a search, the first search after an open | 6 |

The score of a case is its speedup against release 2.3.4. For a gate workload that is one round's
ratio (SQLite's time over inillucent's) divided by the baseline's median ratio, so a machine that
slows down slows both engines in the same round. For a search case it is the baseline's median time
over this run's. A round's score is the geometric mean over cases of each case's median. Train and
test are the gate's fixed split (FNV-1a of the name is 0 mod 3): 56 train cases and 32 test cases.

Every build was measured in one quiet window, the builds interleaved pass by pass, three passes, so
each gate workload has 60 rounds and each search case 3 runs. Two quiet windows were used; the round
6 build was measured in both and read 1.401, 1.622 and 1.477 in the first and 1.401, 1.621 and 1.478
in the second (train, test, all). The flow directory with every row, trace and change note is
`_agent_output/task-2209/hillclimb/common-ops/` in the main checkout (gitignored), with
`narrative.md`, `percase.tsv`, `percase2.tsv` and `report.html`.

**The noise floor.** Rounds 1 and 2 change no code the gate runs. Their gate geometric means read
1.003 and 0.999. A mean of per round ratios was tried first and moved those same builds by up to 3%,
because a single SQLite round sometimes takes 20 to 35 times its usual time when its own checkpoint
lands in it; medians are used for that reason.

## 3. Result

| round | change | test | train | gate only | search only | all 88 |
|---|---|---:|---:|---:|---:|---:|
| 0 | release 2.3.4 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |
| 1 | search cache kept across writes and commits | 1.127 | 1.004 | 1.003 | 1.898 | 1.047 |
| 2 | an exact search table builds a two link graph | 1.168 | 1.003 | 0.999 | 2.400 | 1.060 |
| 3 | a read after a write keeps the read lease | 1.362 | 1.166 | 1.177 | 2.345 | 1.234 |
| 4 | a search segment is read from memory without copies | 1.371 | 1.170 | 1.176 | 2.533 | 1.239 |
| 5 | no lease thread wake a statement; the lock level without a mutex | 1.387 | 1.188 | 1.193 | 2.556 | 1.257 |
| 6 | a batch oplock on the log segment (Windows) | 1.621 | 1.401 | 1.418 | 2.582 | 1.478 |
| 7 | a one key `GROUP BY` finds small integer keys by value | 1.664 | 1.407 | 1.434 | 2.643 | 1.495 |
| 8 | a keyword or vector search alone runs in place | 1.682 | 1.432 | 1.433 | 3.355 | 1.519 |
| 9 | the exact vector scan in tasks of 2,048 vectors | 1.699 | 1.436 | 1.433 | 3.617 | 1.527 |
| 10 | aggregates read their arguments and keys a batch at a time | 1.793 | 1.507 | 1.507 | 3.818 | 1.605 |
| 11 | a multi term top N rejects by its first term; `LIKE '%x%'` | 1.780 | 1.516 | 1.510 | 3.754 | 1.607 |
| 12 | an append keeps the largest key hint | 1.788 | 1.520 | 1.515 | 3.781 | 1.612 |
| 13 | a new row's non unique index entries take the new key path | 1.778 | 1.523 | 1.515 | 3.759 | 1.611 |

Rounds 1 to 5 are from the first window and rounds 6 to 13 from the second; round 6 was in both.

**The goal is met: the gate's 82 workloads are 51.5% faster than 2.3.4 (train 50.2%, test 53.8%),
the search cases 276% faster, and all 88 cases 61% faster.** The test split rose with the train
split in every round that moved the gate.

What moved most, against 2.3.4: a search after an insert 158x, `large.read` 8.0x, `ai.group.total`
7.8x, `ai.copy.point` 7.4x, `edge.empty` 7.1x, `ai.point.newest` and `ai.point.rowid` 5.6x,
`ai.count` 4.1x, `edge.wide.row` 3.8x, keyword search 3.1x, `point.miss` 2.7x, `point.rowid` 2.2x,
`join.selective` 2.0x, `churn.scan` 1.9x.

The six search cases at round 13, against 2.3.4: one insert then one search 158x, keyword search
3.1x, the first search after an open 1.7x, hybrid search 1.6x, 5,000 inserts in a transaction 1.5x,
vector search 1.5x. The search probe runs each case three times, so a single search case moves by
up to 10% between rounds that do not touch it.

**What did not move or got slower.** Six cases read 0.96 to 0.97 against 2.3.4. Five of them move
both ways from round to round, inside the per workload band. `write.insert.batch` drops at round 6
and stays at about 0.95: its log writes now go through overlapped I/O on the shared segment, which
costs a few percent on a write heavy workload. The writes that are bound by a sync of the disk
(`write.insert.autocommit`, `txn.autocommit`) are unchanged, as expected.

## 4. The changes

### 4.1 Search

1. **The cached index survives writes and commits** (`crates/inillucent-search/src/module.rs`,
   `merge.rs`). Every write and every commit called `cache.forget()`, so the search after each insert
   loaded and folded every segment again: 51 ms a cycle on 5,000 rows. The cache is keyed by the live
   segments and the delta entries it applied, and the next search applies only the entries added
   since. A flush and a finished merge re-key it (`Cache::after_flush`, `Cache::after_merge`).
   Rollback and rollback to a savepoint still drop it. One insert and one search: 51 ms to 0.44 ms.
2. **An exact table builds a two link graph** (`merge.rs`, `configuration`). `mode = 'exact'`, the
   default, never walks the graph, and the full graph build was 58% of the processor time of 5,000
   inserts. A lexical table already had m = 2 and ef_construction = 2 for the same reason. A
   declaration that names m or ef_construction still gets them. 5,000 inserts: 466 ms to 316 ms.
3. **A segment is read from memory without copying each section** (`inillucent-core/src/persist.rs`,
   `read_index_bytes`; `store.rs` reserves the whole segment). First search after an open: 34 ms to
   about 21 ms.
4. **One search leg runs in place** (`inillucent-core/src/index.rs`). `rayon::join` from an
   application thread sends both closures to the pool and blocks, even when one is empty: 42% of a
   keyword search. The BM25 maps are sized from the postings (`bm25.rs`). 100 keyword searches:
   7.3 to 9.3 ms to 2.3 ms.
5. **The exact vector scan takes tasks of at least 2,048 vectors** (`flat.rs`). Stealing and waking
   were 60% of a vector search's samples. 100 vector searches: 23 to 25 ms to 19 to 23 ms; the rest is
   reading 7.7 MB of vectors a search.

### 4.2 Statements outside a transaction

6. **A read after a write keeps the read lease** (`inillucent-engine/src/engine/locks.rs`,
   `state.rs`). `release_if_idle` judged whether the statement wrote by the pool's dirty page count,
   which stays high after every commit until the next fold, so every read after any write let the
   lock go and the next read took it again and read the meta record. The outermost statement now
   records the log end and the dirty count as it enters, and the lease decision asks whether this
   statement moved either, raised the lock past SHARED or left a hot journal. A point read by rowid
   after 20,000 inserts: 4.6 us to 1.4 us.
7. **The statement path stops waking the lease thread and stops taking a mutex for the lock level**
   (`inillucent-pool/src/lease.rs`, `inillucent-vfs/src/os/windows.rs`). The lease thread polls every
   `IDLE` while leases keep being armed and waits on its condition variable only once none was armed
   since its last look, so a statement loop never wakes it. The Windows lock level is copied into an
   atomic at the end of every transition. Both were Fable's suggestions in task-2210.
8. **A batch oplock on the log segment** (`inillucent-vfs/src/os/windows_log.rs`). Fable's
   recommendation in task-2210, `_agent_output/task-2209/fable/advice.md`. After item 6, half of an
   autocommit point read was `GetFileSizeEx` on the open log segment, the check for another
   process's commit, and no cheaper kernel call answers it (0.8 to 1.6 us each, measured from Python).
   Every open of a log segment in a process now shares one file object opened for overlapped I/O, and
   a watcher thread holds a batch oplock on it. While it holds, no other handle to the segment exists
   on the machine, so the segment's length is kept in memory from this process's own writes and
   truncations. Another process's open breaks the oplock and waits until the watcher acknowledges, and
   the watcher clears its trust in the kept length before it acknowledges, so a commit by the other
   process is always seen. Reads and writes on the shared object use `ReadFile` and `WriteFile` with an
   event per thread, because the standard library waits on the file handle, which goes wrong when two
   threads share one object. `INILLUCENT_LOG_OPLOCK=0` turns it off. Linux and macOS are unchanged.
   hcprobe `select.point` in autocommit: 1.74 to 1.91 us to 0.46 to 0.55 us.

### 4.3 Aggregates, sorts and patterns

9. **A one key `GROUP BY` finds a small integer key's group by value** (`inillucent-exec/src/ops/
   aggregate.rs`). Keys 0 to 65,535 index a table of group positions. A value seen for the first
   time takes the encoded path, which finds or makes the group, and is then remembered, so a real
   equal to an integer lands in that integer's group. The first key that is not such an integer
   turns the table off for the statement.
10. **Aggregates evaluate their arguments and keys a batch at a time** (Fable's round 2 advice in
    task-2212, `_agent_output/task-2209/fable2/advice.md`). `Eval::ints_over` and `reals_over` let
    column references, integer literals, integer arithmetic, division by a constant and `length()`
    produce a whole batch's values in one loop (`batch.rs`, `expr.rs`, `scalar.rs`).
    `SimpleAggregate` and the one key `HashAggregate` fold them with `Accumulator::push_int` and
    `push_real`, which mirror `push` for counts and sums and hand every other kind to it. A batch
    any part of which declines takes the per row path. Against the round before: `ai.group.total`
    3.63x, `churn.scan` 2.21x, `check.fresh` 1.61x, `ai.copy.scan` 1.48x, `check.copied` 1.31x.
11. **A multi term top N rejects by its first term's typed reader** (`ops/order.rs`).
    `app.order.text.limit` 1.63x.
12. **`LIKE '%literal%'` checks for a NUL only before the end of a match**, and finds the literal's
    first byte a word at a time (`exec/scalar.rs`). A text cut at its first NUL is a prefix of the
    whole, so a whole text without the literal cannot hold it cut either.

### 4.4 Inserts

13. **An append keeps the largest key hint** (`inillucent-tree/src/write.rs`, `paged/shape.rs`).
    The write forgot the remembered largest key for the leaf it changed, and the code meant to restore
    it read the forgotten hint back and kept nothing, so every rowid allocation descended the tree:
    4.8% of an insert into a table with two indexes, now 0.6%. `note_appended_key` takes the leaf
    seen before the append and checks it is still this tree's rightmost leaf.
14. **A new row's non unique index entries take the new key path** (`exec/dml/insert.rs`). Every
    caller reaches `place_row` with no previous row only after the conflict checks proved the row's
    key free, and such an entry ends in that key. Unique indexes keep `put`.

## 5. Tried and not kept, and not tried

- **SHARED on the RESERVED byte during a read lease**, to stop writers appending instead of asking
  the log its length. Fable rejected it: a writer of any version would then wait for a busy reader's
  lease, against the documented rule that readers do not hold writers up.
- **A shared memory commit counter.** No way to make it safe beside an older build, which would not
  move the counter (Fable, task-2210).
- **A cost based choice of the existing hash join** for `app.dashboard`. Its build costs more than the
  12,500 index probes it would replace (Fable, task-2212). A compact integer hash join chosen at run
  time is the change that would move it, and was not needed for the goal.
- **Sorting rowids before fetching the rows of a non covering index scan.** The fetch is the descent
  and the leaf search, not memory locality, and the sort changes the output order (Fable).
- **Applying a transaction's index entries in key order at commit.** The only write change that could
  reach 25% on the insert family, but every reader of the index inside the transaction, the unique
  check, statement abort, savepoints and `REPLACE` would have to see the pending entries. A ticket of
  its own.
- **Skipping the rowid probe for a key the statement just allocated**, and recording a leaf hint in
  point probes: about 4% and 2% of an insert each; not done.

## 6. Tests

- `crates/inillucent-compat/tests/engine/search.rs`: a kept index answers like a fresh load after
  every write across flushes and merges; a rolled back write and a savepoint rollback are forgotten;
  an exact table answers the true nearest rows against a brute force ordering.
- `crates/inillucent-core/src/persist.rs`: `read_index_bytes` reads what `read_index` reads, and
  refuses every truncation.
- `crates/inillucent/tests/budget.rs`: 200 point reads after one autocommit write read the meta record
  at most 20 times (2 measured; one a read before).
- `crates/inillucent-compat/tests/durability/log_oplock.rs`: a commit by another process, the shipped
  command line, is seen by the very next read 40 times, with pauses that let the oplock be taken back;
  the oplock takes the length query off a read, gives it back while another handle is open, and takes
  it again. The second test fails with `INILLUCENT_LOG_OPLOCK=0`. A third test opens a database at a
  path whose files were deleted while an older database still held them, and checks the new commits
  reach the segment on disk. The full test run found this case: `walperf` leaks a database, deletes
  its files and opens the path again, and the shared segment it was handed still pointed at the
  deleted file, so the reopen read `no such table: t`. `open_log_share` now refuses a share whose
  file is delete pending or has no links. The test fails without that check.
- `crates/inillucent-compat/tests/differential/group_by.rs`: one key groups over integers inside and
  outside the direct range, reals equal to integers, NULLs and text, against SQLite.
- `crates/inillucent-compat/tests/differential/rowid_hint.rs`: allocated rowids after writes of every
  kind, in a transaction and in autocommit, against SQLite.
- `crates/inillucent-exec/src/ops.rs`: the grouped batch loop against a plain fold over a typed batch,
  a selected batch, materialised values and a key past the direct range.
- `inillucent-searchprobe` is the new binary that times the search cases.
