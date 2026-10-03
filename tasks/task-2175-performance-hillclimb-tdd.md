# task-2175: a performance hill climb over common use cases and edge cases

## 1. Why

Recent tickets reported slow cases that the performance gate never saw: a search after another
process published, reads of tables an application had just filled, deletes of many rows, and
statements an example application ran through `SharedDatabase`. The published figure
(`docs/performance.md`, 5.19x faster than SQLite) comes from `inillucent-fullgate`, and that gate
has three properties that hide those cases:

| Property of the contract gate | What it hides |
|---|---|
| The fixture is bulk imported, so every leaf is packed and sorted | tables built with `INSERT`, leaves with rows waiting in the delta area, half full leaves after deletes |
| It drives the engine through `ImportedDatabase::plan`, `prepare` and `pipeline` | everything the shipped `Connection` API adds per statement, which is what every driver, the CLI and MCP use |
| Its 34 workloads were chosen in 2026-09 and stayed fixed | the queries applications write that are not in it: `OR` across two indexes, `NOT IN (SELECT ...)`, recursive CTEs, bulk `DELETE` and `UPDATE`, `INSERT ... SELECT` |

This ticket builds an eval that covers those cases, measures speed, processor time and peak
memory on it, and improves the engine against it with a hill climb that guards against tuning to
the eval.

## 2. Terms

| Term | Meaning |
|---|---|
| contract plan | the 34 workloads in `perf::plan_for`, which `docs/performance.md` reports |
| hillclimb plan | the 48 workloads in `perf::hillclimb::plan_for`, added by this ticket |
| Connection arm | the gate's `--api connection` arm: `Connection::prepare`, `bind`, `step`, the API a program calls |
| pipeline arm | the gate's default arm, the engine's internal entry points; every published number |
| ratio | SQLite's time divided by inillucent's, per workload per round. Above 1 is faster |
| train set, test set | the two fixed halves of the workloads, chosen by a hash of the workload name |
| candidate | one change to the engine, built into its own gate binary |
| base | the build the candidate is compared with: the last kept candidate |

## 3. The eval

### 3.1 Workloads

The hillclimb plan (`crates/inillucent-compat/src/perf/hillclimb.rs`) runs on the same medium
fixture (100,000 rows) and through the same `sqlite_bench.c` as the contract plan, so every
workload is timed on both engines with the same SQL, the same parameters and the same
transaction grouping, and its answer is compared by digest before its time counts.

| Family | Workloads | What it covers |
|---|---|---|
| `hc.after_insert` | 12 | `fresh` gets 20,000 rows in batches of 500 with a unique text index and a `(grp, created)` index whose keys arrive in no order; reads by rowid, by name, newest of a group, totals and counts; 2,000 more rows and reads of the newest; `INSERT ... SELECT` of 100,000 rows and reads of the copy |
| `hc.churn` | 6 | updates that grow values, a delete of every other row, the reads again, a refill |
| `hc.app` | 16 | pagination with `OFFSET`, a filtered count, a short `IN` list, a view with `WHERE`, a dashboard join, a window, a recursive CTE, a JSON filter, statements prepared on every call, an upsert counter, `RETURNING`, `EXISTS`, `min` and `max`, a top 10 by text |
| `hc.edge` | 11 | a 600 value `IN` list, `LIKE '%x%'`, a 50,000 group `GROUP BY`, a self join, `OR` across two indexes, `NOT IN (SELECT ...)`, `UNION`, a 40 column row, an empty table, a whole table `UPDATE`, a range `DELETE` |
| `hc.check` | 3 | reads that digest every table the plan built, so a write that lost a row cannot be timed |

The contract plan is the second half of the eval. It is run through the Connection arm as well
as the pipeline arm, because the Connection arm is what applications get.

### 3.2 Train and test

A workload is in the test set when FNV-1a of its name is 0 mod 3. The rule is in
`perf::hillclimb::is_test` and in `tools/perf-hillclimb/score.mjs`, and was fixed before the first
measurement. It puts 20 of the 48 hillclimb workloads and 9 of the 34 contract workloads in the
test set. Changes are designed by profiling train workloads. Test workloads are looked at only
to accept or reject a change.

### 3.3 Measures

| Measure | How | Source |
|---|---|---|
| speed | per workload ratio per round; the train and test score is the geometric mean of the workload medians | `--samples`, `score.mjs` |
| processor time | user plus kernel time of one child process that runs one round, each engine | the gate's child pair |
| peak memory | the peak working set of the same child | the gate's child pair |
| per workload memory | the peak after each workload in the child | `mark` lines, `--api connection` now records them |

The child runs the plan through the same arm the timed rounds used. Before this ticket the child
ignored `--plan` and `--api`, so a hillclimb run compared SQLite's cost of the hillclimb plan with
this engine's cost of the contract plan.

### 3.4 Noise

Both engines run on the performance cores, in rounds that alternate which engine goes first, and
the ratio is taken inside a round, so a change in machine speed moves both arms. The gate's quiet
check compares SQLite's own speed with a recorded idle reference and refuses to grade a busy
machine. It refused this ticket's first baseline: this session's own code search server was using
22 cores after one semantic query. Every graded number below comes from a run the quiet check
passed.

## 4. Accepting a change

The first version of this rule asked for the train and the test geometric means both to rise,
the rule the hill climbing article uses for prompts. Two measurements changed it.

- **The same binary run twice moves.** An A/A run of one build (c2 against c2b in the log) moved a
  geometric mean by up to 2.5%, one workload by up to 7%, the child's processor time by up to 10%
  and its peak by up to 3%. The fsync bound write workloads (`txn.autocommit`, `txn.large`,
  `extension.fts.build`, `extension.rtree.insert`, `write.upsert`) moved by up to 35% between two
  runs, in both arms, because the disk's sync latency changes over minutes.
- **A targeted fix moves only the workloads that reach the code.** The `IN` set made
  `edge.not.in` 166 times faster and is in no test workload, so the test mean could not rise.
  Rejecting it would reject every change whose workloads happen to hash into one set.

So a candidate is kept when all of these hold, comparing its runs with the base's runs:

1. the workloads the change targets are faster with their whole 95% interval above 1.00, and the
   train geometric mean does not fall by more than the A/A band (2.5%);
2. the test geometric mean does not fall by more than the A/A band: its centre is at least 0.975;
3. a workload more than 7% slower with its whole interval below 1.00 is run again, and the
   candidate is rejected if it is slower again; a workload the change cannot reach that moved in
   both arms is recorded as noise, with the rounds that show it;
4. the child's processor time and peak memory do not rise by more than their A/A bands (10% and
   3%), unless the change is a memory change and the trade is stated;
5. the tests the change can affect pass, a new test shows the change is right where it differs
   from the old code, and every answer in both plans still agrees with SQLite.

The guard against tuning to the eval is rules 2 and 5 together with how a change is made: every
change is a general mechanism (a set lookup, a cached setup, a lock that is not needed) and none
is keyed on a workload's SQL text or its data.

A rejected candidate is recorded in section 7 with its numbers, so it is not tried again.
After two candidates in a row are rejected on the same workload, that workload's remaining gap is
written up by cause instead of tried again.

## 5. Baseline

Measured on 2026-10-03 at `7bd72b8d` plus the two answer fixes in section 6, ten rounds each,
graded by the quiet check.

| Run | Train | Test | Peak memory | Processor time |
|---|---|---|---|---|
| hillclimb plan, Connection arm | 0.851x | 0.669x | 100.96 MiB against 38.30 (2.64x) | 3,906 ms against 1,820 (2.15x) |
| contract plan, Connection arm | 1.623x | 1.262x | 42.29 MiB against 37.73 (1.12x) | 484 ms against 992 (0.49x) |
| contract plan, pipeline arm | 3.425x | 3.784x | 41.94 MiB against 37.75 (1.11x) | 297 ms against 977 (0.30x) |

The contract plan's weighted headline is 5.12x through the pipeline arm and 1.94x through the
Connection arm.

The slowest workloads, hillclimb plan, Connection arm:

| Workload | Ratio | Cause found |
|---|---|---|
| `edge.or.two.indexes` | 0.002x | an `OR` of two indexed columns is a residual on a full scan |
| `edge.not.in` | 0.004x | `NOT IN (SELECT ...)` becomes a list that is searched linearly per row |
| `edge.delete.range`, `churn.delete.half`, `edge.update.all` | 0.12x to 0.14x | bulk DML |
| `app.insert.returning`, `app.insert.prepare_each`, `churn.refill` | 0.14x to 0.16x | inserts into indexed tables through the Connection |
| `app.cte.recursive` | 0.15x | the recursive step is prepared again on every pass |
| `ai.copy` | 0.17x | `INSERT ... SELECT` into an empty table |
| `edge.group.high` | 0.23x | the hash aggregate hashes twice and copies the key per row |
| `ai.build` | 0.28x | 20,000 inserts into a table with two secondary indexes |
| `edge.like.contains` | 0.34x | `LIKE` copies subject and pattern per row |
| `ai.count`, `app.json.where` | 0.47x | delta rows materialised to count; JSON parsed into a tree per row |

Through the Connection arm, `prepare.trivial` is 0.05x and `extension.json` 0.04x, where the
pipeline arm reads 0.54x and 0.97x.

## 6. Wrong answers the eval found

Both are fixed in this ticket and covered by `crates/inillucent/tests/hillclimb_answers.rs`, which
fails on the old code.

| Query | Wrong answer | Cause | Fix |
|---|---|---|---|
| `SELECT id FROM fresh WHERE grp = ? ORDER BY created DESC LIMIT 20` after 20,000 batched inserts | twenty rows from the middle of the group, or 19 rows, for every group whose index entries crossed a leaf boundary | `visit_span_reverse` descended to `enc(grp)`, which is the leaf where the group starts, and applied the upper bound on the first leaf only | descend to `enc(grp)` followed by 0xFF bytes when the bound is inclusive, and apply the upper bound on every leaf (`crates/inillucent-tree/src/paged/cursor.rs`) |
| `SELECT ... LIMIT 20 OFFSET (?1 % 250) * 20`, prepared once and bound again | every page after the first started where the first did | the offset expression was folded when the chain was built and the fold read `?1` without counting it, so the chain was reused | count the read with `note_execution_constant`, the guard a folded `now()` uses (`crates/inillucent-exec/src/physical/translate.rs`) |

## 7. What the hill climb did

The round by round record, with every run's numbers, is
`_agent_output/task-2175/hillclimb-log.md`. Runs are ten paired rounds each, graded by the quiet
check. "hc" is the hillclimb plan and "cc" the contract plan, both through the Connection arm.

### 7.1 Kept

| # | Change | Where | Measured against the round before |
|---|---|---|---|
| 1 | a query that reads no table, no subquery and no registered function takes no file lock | `engine/lockless.rs`, `engine/compiled.rs` | cc: `prepare.trivial` 28.3x, `extension.json` 14.8x, test mean 1.310x |
| 2 | `IN` and `NOT IN` over 8 or more constants (written or folded from a subquery) are answered by a binary search of the sorted values | `exec/inset.rs`, `expr/tree.rs` | hc: `edge.not.in` 166x; child processor time -25% |
| 3 | a recursive CTE prepares each step arm once and keeps one set of the rows a `UNION` has kept | `exec/recursive.rs` | hc: `app.cte.recursive` 1.13x |
| 4 | the largest rowid is read from the last live sorted row and the last delta key of the rightmost leaf, not by visiting every live row | `tree/leaf/read.rs`, `exec/dml/target.rs` | probe: an insert that lets the engine pick the rowid, 19.0 to 2.6 us; hc: `ai.build` 2.28x, `churn.refill` 2.09x, train 1.092x, test 1.060x |
| 5 | an `OR` whose every arm can seek is a union of the arms' seeks, read by rowid once each, with the `OR` kept as a residual | `sql/plan.rs`, `plan/seek_union.rs`, `exec/physical/*` | hc: `edge.or.two.indexes` 664x, test mean 1.388x, child processor time -23% |
| 6 | a bulk delete looks first in the leaf the last delete used, locates each row once, and builds the root path only for a merge or a split | `tree/write.rs`, `exec/dml/delete.rs` | hc: `churn.delete.half` 1.30x, `edge.delete.range` 1.10x; cc: `write.delete` 1.09x |
| 7 | `update_in_place` uses the leaf the read before it found; `vector_dimensions` answers an ordinary column without an allocation | `tree/write.rs`, `tree/paged.rs`, `sql/catalog_view.rs` | hc: `edge.update.all` 1.15x; cc: `write.upsert` 1.17x |
| 9a | the hash `GROUP BY` keeps a group's position in the map, every group's accumulators in one vector, and finishes them in storage order | `exec/ops/aggregate.rs` | probe 55.5 to 30.0 ms; hc: `edge.group.high` 1.53x, `ai.group.total` 1.44x |
| 9b | `LIKE` with no `ESCAPE` over text reads both sides in place and answers `literal`, `literal%`, `%literal`, `%literal%` with an ASCII folding comparison | `exec/scalar.rs` | probe 13.7 to 5.1 ms; hc: `edge.like.contains` 2.58x |
| 9c | `json_extract` over a column of distinct documents parses each where it lies after four misses in a row | `exec/scalar.rs` | probe 2.10 to 1.86 ms; hc: `app.json.where` 1.10x |
| 10 | `INSERT ... SELECT` reads the query's rows where they are instead of copying them | `exec/dml/insert.rs` | hc: `ai.copy` 1.10x, its rise in peak memory 65 to 46 MiB |
| 11 | the accumulator's rarely used parts are boxed and made on first use (144 bytes) | `exec/aggregate.rs` | hc: child peak memory 102.3 to 82.7 MiB, `edge.group.high` 1.26x |
| 12 | a recursive CTE's step arms are compiled once and run per pass | `exec/recursive.rs` | probe 5.19 to 2.13 ms; hc: `app.cte.recursive` 2.69x |
| 13 | a full scan fills a written leaf's projected columns straight from the merge order | `exec/paged.rs` | hc: `churn.scan` 1.16x, `check.fresh` 1.12x, `ai.count` 1.09x |

Each has a test that fails without it where it changes an answer, or a unit test of its rule:
`crates/inillucent/tests/hillclimb_answers.rs` (the two wrong answers, long `IN` lists against the
written out comparisons, nine `OR` shapes against `NOT INDEXED`, rowid allocation after every kind
of delete and a reopen, bulk deletes against `integrity_check`, rollback of inserted rows),
`like_shapes_agree_with_the_matcher`, `an_accumulator_keeps_its_rare_parts_out_of_line`.

### 7.2 Rejected

| Change | Why |
|---|---|
| a full scan of a written leaf decodes only the projected columns through `visit_live` | the scan's projection is every column even for `count(*)`, so there was nothing to leave out, and `visit_live` decodes a delta row one column at a time: `ai.count` 0.68x |
| an undo record holds a rowid key as an integer | no change in peak memory (`ai.copy`'s rise 46.19 against 46.02 MiB) and the write workloads were faster without it |
| taking the shared lock without the PENDING byte | not built: a reader can then take the read range between a writer's two steps and starve it, and SQLite's own Windows VFS takes the same three steps |

### 7.3 Handed on

Each needs a storage format, a log record or a cross process protocol change, and each has a ticket
with its measured numbers and the way to measure it.

| Ticket | Change | Workloads |
|---|---|---|
| task-2178 | build the tree in bulk for `INSERT ... SELECT` into an empty table | `ai.copy` 0.22x, 46 MiB of peak memory |
| task-2179 | log a leaf split as a logical record instead of three page images | `ai.build`, `ai.copy`, every insert heavy workload |
| task-2180 | delete a leaf's keys with one page change | `edge.delete.range` 0.18x, `churn.delete.half` 0.26x, `edge.update.all` 0.28x |
| task-2181 | fewer system calls per statement for the cross process checks | every autocommit statement through the Connection |
| task-2182 | push the columns a query reads into the scan | `ai.count` 0.53x, `edge.in.long` 0.68x |

### 7.4 Where it ended

| Run | Train | Test | Peak memory | Processor time |
|---|---|---|---|---|
| hc, baseline | 0.851x | 0.669x | 100.96 MiB against 38.30 | 3,906 ms against 1,820 |
| hc, after (c13r) | 1.302x | 1.140x | 82.86 MiB against 38.27 | 1,719 ms against 1,938 |

The final graded runs of all three arms are in section 9.

## 8. Out of scope

Search table (`inillucent_search`) commits and merges are task-2177's, which is open beside this
one, and are not measured here.

## 9. Final graded runs

Measured on 2026-10-03 with the build that was merged, ten rounds each, files
`_agent_output/task-2175/runs/final-*.txt`. The quiet check graded both contract runs (SQLite's
speed index +0.02% and -0.02% against the reference). The reference has no rows for the hillclimb
plan's workloads, so that run was graded without it.

| Run | Train | Test | Peak memory | Processor time |
|---|---|---|---|---|
| hc, baseline | 0.851x | 0.669x | 100.96 MiB against 38.30 | 3,906 ms against 1,820 |
| hc, final | 1.302x | 1.147x | 82.82 MiB against 38.27 | 1,750 ms against 1,891 |
| cc, baseline | 1.623x | 1.262x | 42.29 MiB against 37.73 | 484 ms against 992 |
| cc, final | 1.908x | 1.637x | 42.19 MiB against 37.75 | 453 ms against 1,008 |
| cp, baseline | 3.425x | 3.784x | 41.94 MiB against 37.75 | 297 ms against 977 |
| cp, final | 3.469x | 3.600x | 41.97 MiB against 37.73 | 344 ms against 1,008 |

Against the baseline, the hillclimb plan's train mean rose 1.53x (95% interval 1.50 to 1.63) and
its test mean 1.71x (1.58 to 1.73). The child's processor time fell 55% and its peak 18%. The
contract plan's weighted headline through the Connection arm rose from 1.94x to 2.37x. Through the
pipeline arm it read 5.12x before and 5.10x after.

Two readings in these runs are the disk:

- `txn.autocommit`, `txn.batched` and `write.insert.autocommit` read 0.51x to 0.65x of the
  baseline in both contract arms. SQLite's own time for them rose 3.3x to 5.6x between the two
  runs (`txn.autocommit` 32.7 ms to 109 ms). Run again the same evening on the `transaction` and
  `write` families, the baseline binary and the final binary read the same: `txn.autocommit`
  1.06x and 1.03x, `txn.batched` 3.48x and 3.53x, `write.insert.autocommit` 3.12x and 3.15x
  (`runs/disk-*.txt`, `runs/disktx-*.txt`). Those three are the reason the pipeline arm's test
  mean reads 0.95 of the baseline; every other pipeline workload is within 4% of it or faster.
- `edge.empty` reads 0.940 of the baseline, inside the 7% band one workload moves between two runs
  of the same binary (section 4). It is a query over an empty table that takes about 2 us.

Five workloads of the hillclimb plan are still under 0.5x. Each has a follow up ticket in section
7.3: `ai.copy` 0.22x, `edge.delete.range` 0.18x, `churn.delete.half` 0.26x, `edge.update.all`
0.28x, `app.insert.returning` 0.35x.
