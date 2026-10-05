# task-2185: a third performance hill climb against SQLite

## 1. What was asked

Release 2.1.3 was still slower than SQLite on 17 workloads through the `Connection` API. This
ticket ran the method of [task-2175 section 4](task-2175-performance-hillclimb-tdd.md) again,
starting from the list in [task-2183 section 5](task-2183-performance-hillclimb-2.md): profile a
losing workload, change the engine, measure the change against the last kept build, keep it when the
acceptance rule says so.

## 2. Result

Measured on 4 and 5 October 2026 with `inillucent-fullgate medium.db --api connection --rounds 10`,
three runs of each build in turn on each plan, scored with `tools/perf-hillclimb/score.mjs`. The base
is release 2.1.3. Release 2.1.4, cut while this ticket ran, changed version numbers and the hostile
suite's peak memory reading and no engine code, so the comparison holds for it too. Ratios are
SQLite's time divided by inillucent's, so above 1 is faster than SQLite.

| Measure | 2.1.3 | this branch | Change |
|---|---:|---:|---:|
| hillclimb plan, 28 train workloads | 1.865x | **2.160x** | 15.8% faster |
| hillclimb plan, 20 test workloads | 1.495x | **1.769x** | 18.3% faster |
| contract plan through the Connection, 25 train workloads | 2.207x | **2.395x** | 8.5% faster |
| contract plan through the Connection, 9 test workloads | 1.969x | **2.075x** | 5.4% faster |
| hillclimb plan, processor time of one round | 1,375 ms | **1,156 ms** | 15.9% less |
| contract plan, processor time of one round | 453 ms | **375 ms** | 17.2% less |
| hillclimb plan, peak resident memory of one round | 59.16 MiB | **56.91 MiB** | 3.8% less |
| contract plan, peak resident memory of one round | 42.59 MiB | **43.87 MiB** | 3.0% more |
| workloads slower than SQLite, both plans | 16 on this day | **12** | |

The ticket counted 17 from task-2183's runs. On this day's run of 2.1.3 `check.counters` read 1.04x,
so the base count is 16. Four workloads crossed: `edge.delete.range` (0.90x to 1.04x),
`range.lookaside` (0.98x to 1.12x), `edge.in.long` (0.99x to 1.04x) and `txn.autocommit` (0.98x to
1.02x). None fell below.

The contract plan's peak memory rose 3.0%, which is the edge of the A/A band. It is the kept `INSERT`
setup of section 3.5: each prepared insert in the plan now holds its compiled plan, declarations and
`VALUES` expressions between executions, and the processor time of the same round fell 17%.

The hillclimb plan's two worst changes are `correlated.exists` 0.896 (interval 0.818 to 1.023) and
`scan.sort` 0.950 (0.902 to 1.020). Both intervals cross 1.00, and both read faster than the base
in the c9 and c11 runs (1.056 and 1.038; 1.020 and 1.029). They are recorded as noise.

### 2.1 Workloads that changed most

| Workload | 2.1.3 | this branch | Change |
|---|---:|---:|---:|
| `ai.copy.scan` | 1.11x | 3.50x | 3.16x |
| `check.copied` | 1.15x | 3.30x | 2.88x |
| `large.read` | 1.96x | 2.68x | 1.36x |
| `ai.point.rowid` | 1.83x | 2.49x | 1.37x |
| `point.miss` | 1.99x | 2.63x | 1.32x |
| `point.rowid` | 2.05x | 2.61x | 1.28x |
| `churn.delete.half` | 0.33x | 0.42x | 1.26x |
| `churn.refill` | 0.74x | 0.89x | 1.21x |
| `range.lookaside` | 0.98x | 1.12x | 1.15x |
| `edge.delete.range` | 0.90x | 1.04x | 1.15x |
| `write.upsert` | 3.45x | 3.92x | 1.14x |
| `app.insert.prepare_each` | 0.81x | 0.90x | 1.11x |
| `app.json.where` | 0.80x | 0.86x | 1.07x |
| `edge.update.all` | 0.61x | 0.66x | 1.07x |

## 3. The changes, in the order they were kept

Each line names what was measured for the change against the build before it. The commit messages
hold the same. Candidate numbers refer to the binaries under `_agent_output/task-2185/bin`.

1. **The bulk delete (c1 to c3).** A merge packs both leaves straight from their pages in one pass
   (`write/merge.rs`), the owned route stays for leaves with out of line values. The delta area is
   searched by galloping from the previous key's position (`LeafRef::delta_search_from`, a
   `RunCursor` for both regions). A deleted row is shown to the caller as values borrowing from the
   copy of the leaf the undo images already use, and the caller copies only an index entry's values
   (`plain_index_entry`). The `DeleteRows` keys go into one buffer, the index keys are encoded into
   one buffer for sorting, and the undo buffer is reserved once (`TreeLog::expect_undo`).
   `churn.delete.half` 1.21x to 1.25x in three runs against 2.1.3.
2. **`length()` (c4).** UTF-8 characters are counted eight ASCII bytes at a time, and the NUL that
   ends SQLite's count is found in the same pass. It counted one byte at a time after a separate
   search for the NUL. `ai.copy.scan` 3.20x, `check.copied` 2.87x, `churn.scan` 1.14x,
   `check.fresh` 1.13x. A randomized test checks the count against the byte rule.
3. **Compile (c5).** A statement's names are searched one by one below 16 names and filed by hash
   past it (`LINEAR_NAMES`), so the parse stays linear for a statement with many names. The
   authorizer is lent the database, table and column names instead of copies. An `UPDATE` compares
   an index entry's slots to decide whether the entry moves, when no index computes anything.
   Compiling a one table aggregate, median of ten alternations: 6.21 µs to 5.74 µs.
4. **`json_extract` with a literal path (c8).** The path is parsed when the call is compiled, so a
   row no longer copies the literal and compares it with the cached path. `app.json.where` 1.057
   (1.045 to 1.063).
5. **A kept `INSERT` setup (c9).** A prepared insert keeps its plan, its table's declarations and
   its compiled `VALUES` expressions between executions (`dml/insert/cached.rs`), under the three
   tests the kept `UPDATE` setup already used: the layout is the same object, the settings match,
   and nothing but a parameter was read while it was built. A `VALUES` list holding a subquery is
   compiled each time. `churn.refill` 1.139, `ai.build` 1.123, `app.insert.returning` 1.117,
   `app.insert.prepare_each` 1.113, `app.upsert.counter` 1.075. The test runs one insert on a
   connection that keeps its setup and one that compiles every time, across `ALTER TABLE ADD
   COLUMN`, `CREATE UNIQUE INDEX` and a value reading `last_insert_rowid()`, and fails when the kept
   setup is not pointed at the new values.
6. **A file's length on Windows (c11).** The VFS answered `file_size` through the standard
   library's `metadata`, which asks Windows for every attribute of the file. A connection under
   `locking_mode = normal` asks the open log segment's length on every statement, to learn whether
   another process committed. It is `GetFileSizeEx` now. An autocommit point select went from 6.79
   µs to 5.10 µs (median of eight). Against c9: hillclimb plan train 1.098, test 1.089; contract
   plan train 1.084, test 1.073; the point reads 1.25x to 1.30x.
7. **Key slices (c13, c14).** `delete_sorted` and `update_sorted` take slices of one buffer of key
   values. A vector per key was 40,000 allocations for `edge.delete.range`, and dropping them was 8%
   of the statement. `edge.delete.range` 1.121, `edge.update.all` 1.045.

## 4. Tried and not kept

| Change | Measured | Why it was reverted |
|---|---|---|
| Batched journal pre-images for a fold: one length call, one read per run of pages, one write and one header | `churn.delete.half` 0.984 (0.934 to 1.012) | the fold's time is its syncs; the system calls it removed were processor time the wall clock did not show |
| Moving text out of the owned `SELECT` rows into the bulk path's images | `ai.copy` 1.004 | the heap allocations of `ai.copy` are in the collecting sink; the image copies come from the gate's pooled allocator |
| `UPDATE` keys found with the run cursor; `decide` stops copying the assigned column and the decided value | `edge.update.all` 0.972 (0.940 to 1.012) | the same reason: small allocations are cheap under the pooled allocator |
| `x % k` and `x / k` by a multiply with a reciprocal worked out once (Granlund and Montgomery, tested against `checked_div` and `checked_rem`) | selective correlated execution 10.2 µs and 10.3 µs; `edge.group.high` 0.999 | the per row cost is the calls through the evaluator tree, not the division |

### 4.1 What task-2183's third threshold experiment measured

task-2183 found that merging a leaf only below a third full made `churn.delete.half` 0.84x and
`churn.refill` 0.32x, and read it as inserts into tombstoned leaves being slow. A profile of each
statement says otherwise:

| Build | delete, samples | of which the fold | refill, samples | of which the fold |
|---|---:|---:|---:|---:|
| merge below half (2.1.3) | 2,124 | 440 | 1,153 | 245 |
| merge below a third | 1,331 | 0 | 1,571 | 446 |

The refill's inserts cost the same on both builds (891 and 879 samples). A fold runs inside whichever
statement takes the log past `RECLAIM_BYTES`, 4 MiB. With fewer merge images the delete stopped
crossing it, and the refill crossed it and paid for writing back every page the delete had dirtied.
The third threshold is cheaper in total, by about 11%, and moves a fixed cost between two timed
statements. A trace of every fold in a round (`TRACE_FOLD`, not kept) puts them in `ai.build`,
`churn.delete.half` and `check.counters` on 2.1.3 and on this branch alike.

## 5. The workloads still slower than SQLite

| Workload | Ratio | What the profile says |
|---|---:|---|
| `churn.delete.half` | 0.42x | a quarter of the statement is the fold above. Of the rest, merges are a fifth, and most of a merge is logging its three page images, one of them an empty page. Closing the gap needs a smaller merge record, which is a log format change |
| `correlated.scalar.selective`, `correlated.exists.selective` | 0.58x, 0.61x | about 38 µs for one execution with its compile, against 22 µs. Measured apart: compile 9.5 µs, the statement's own lock and staleness checks about 4.7 µs since c11 (section 6), the warm execution 10.3 µs, and the first execution's chain build. A tenth of the execution is evaluating `a.id % 100 = 0` through the evaluator tree for 400 rows |
| `edge.update.all` | 0.66x | every leaf of `side_table` is a bulk built leaf at 90% fill, so a row that grows by a byte repacks and then splits the leaf. A sixth is copying each row out for the caller and a sixth is evaluating `note \|\| '!'` |
| `ai.copy` | 0.81x | 100,000 rows read from leaves no earlier statement read (18% reading and checking pages), collected as owned rows (24%), then packed and written (29%) |
| `app.json.where` | 0.86x | the pruned parse still reads every member of each document to check it; what remains per row is the label, the kept member and the number's text |
| `app.dashboard`, `join.range` | 0.86x, 0.93x | index nested loop probes; the descent and the leaf search are already interpolated and swizzled, and the probe is bound by cache misses |
| `churn.refill`, `app.insert.prepare_each` | 0.89x, 0.90x | single row inserts: each index entry is a descent, a delta area insert and sometimes a compaction (56%), the unique check is a second search of the same leaf (14%), and the log append is 10% |
| `correlated.in`, `extension.fts.build` | 0.95x, 0.97x | within two readings of each other in runs of the same build |

## 6. What a statement costs before it reaches a tree

The c11 change came from a measurement worth keeping. An autocommit statement under
`locking_mode = normal` takes the file lock, checks whether the meta record moved (two reads, the
primary and the shadow), checks whether the log moved (the open segment's length), and releases the
lock. A point select costs 0.37 µs inside a transaction and 5.10 µs in autocommit, and of the
autocommit time the read of the two meta records is 37%, the lock 31%, the length 12% and the
release 9%. Both meta records are read because a checkpoint writes the shadow before the primary,
and a writer that died between the two leaves the newer record in the shadow.

## 7. Tools

- `_agent_output/task-2185/qpair.sh` pairs two gate builds on named workloads and scores them; the
  `--workloads` runs need the workloads a workload depends on (`edge.delete.range` needs `ai.copy`).
- `_agent_output/task-2185/prof/seg.mjs` profiles the samples between one frame and the next
  occurrence of another, in time order, which is how one statement of a round is separated from the
  others. `children.mjs` and `callers.mjs` print a frame's callees and callers.
- The gate binary links `inillucent_alloc::Pooled`, so a change that only removes small allocations
  shows little on the gate. Two of the four reverted changes in section 4 were that.

## 8. Tests

`inillucent-testrun --changed main --strict` with the machine's declared absences, twice:

| Run | Targets | Tests | Failed |
|---|---:|---:|---|
| before the policy and lint fixes | 372 | 4,574 | `tooling::policy` (this branch: `dml.rs` one line past its ceiling, two functions over 150 lines), `retrieval::bulk_embed` and `inillucent-bench` (an ONNX model failed to load with "bad allocation" while 24 targets ran; both pass when run alone) |
| after them | 372 | 4,574 | `durability::process_storm`, the small pool arm, with readers reporting `a reference names slot 4 of a shared page that holds 4` |

The storm failure is the intermittent one task-2183 measured on 2.1.2 and on the commit before its own
branch (Bug 30 in task-2110). It passed in the first run, and four runs of that arm alone on this
branch all passed. Every other target passed in both runs. Four suites were not evidenced by
declaration (OpenSSL, MySQL, PostgreSQL, Go), and two ran without their prerequisite
(`inillucent-remote::lib` needs `INILLUCENT_NETWORK_TESTS`, `workload_freshness` needs the Nikaya
checkout). `cargo clippy --all-targets --all-features -D warnings` is clean on every changed crate.
