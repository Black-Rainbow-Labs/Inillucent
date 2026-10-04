# task-2183: a second performance hill climb against SQLite

## 1. What was asked

task-2175 built the hill climb eval and improved the engine against it, but release 2.1.2 still lost
to SQLite on 29 of the 82 workloads the two plans measure through the `Connection` API. This ticket
ran the same method again: profile a losing workload, change the engine, measure the change against
the last kept build, keep it when the acceptance rule of
[task-2175 section 4](task-2175-performance-hillclimb-tdd.md) says so.

## 2. Result

Measured on 4 October 2026 with `inillucent-fullgate medium.db --api connection --rounds 10`, two
runs of each build, interleaved, scored with `tools/perf-hillclimb/score.mjs`. The base is the build
task-2175 ended at (release 2.1.2). Ratios are SQLite's time divided by inillucent's, so above 1 is
faster than SQLite.

| Measure | 2.1.2 | this branch | Change |
|---|---:|---:|---:|
| hillclimb plan, 28 train workloads | 1.44x | **1.84x** | 28% faster |
| hillclimb plan, 20 test workloads | 1.20x | **1.50x** | 25% faster |
| hillclimb plan, all 48 workloads | 1.34x | **1.69x** | 26% faster |
| contract plan through the Connection, 25 train workloads | 1.88x | **2.19x** | 16% faster |
| contract plan through the Connection, 9 test workloads | 1.67x | **1.94x** | 16% faster |
| hillclimb plan, peak resident memory of one round | 77.5 MiB | **60.6 MiB** | 22% less |
| hillclimb plan, processor time of one round | 1,586 ms | **1,250 ms** | 21% less |
| workloads slower than SQLite, both plans | 29 | **17** | |

The test set was never used to design a change, only to accept one, and it moved with the train set.

### 2.1 Workloads that changed most

| Workload | 2.1.2 | this branch |
|---|---:|---:|
| `correlated.in` | 0.10x | 0.91x |
| `correlated.exists` | 0.56x | 1.62x |
| `ai.count` | 0.51x | 1.72x |
| `edge.group.high` | 0.45x | 1.52x |
| `app.cte.recursive` | 0.47x | 1.12x |
| `edge.update.all` | 0.21x | 0.64x |
| `edge.delete.range` | 0.48x | 0.96x |
| `check.fresh` | 0.87x | 1.85x |
| `ai.group.newest` | 0.64x | 1.23x |
| `churn.scan` | 0.79x | 1.36x |
| `extension.json` | 0.67x | 1.45x |
| `app.json.where` | 0.52x | 0.84x |
| `edge.like.contains` | 0.86x | 1.26x |
| `app.window.rank` | 0.86x | 1.23x |
| `edge.in.long` | 0.68x | 0.98x |

## 3. The changes, in the order they were kept

Each line names what was measured for the change against the build before it, in the paired run
that accepted it. The commit messages hold the same.

1. **A scan reads only the columns the statement reads** (`PreparedStage::needed`, a column mask).
   An unread column is a `Const(NULL)` vector, and a leaf with rows in its delta area decodes only
   up to the last column read. `edge.in.long` 1.38x, `churn.scan` 1.27x, `ai.count` 1.21x.
2. **A reader keeps a packed copy of a written leaf it reads twice** (`pool/merged.rs`). The copy is
   keyed by page and frame generation, so a change to the page retires it. `ai.count` 3.0x,
   `check.fresh` 2.0x, `ai.group.newest` 1.9x.
3. **A split reads its rows out of a copy of the page** instead of owned values. `ai.build` 1.07x,
   `app.insert.returning` 1.20x.
4. **A bulk delete reads a deleted row only if something needs it**, and the undo image is read
   from a copy of the page. `edge.delete.range` 1.67x.
5. **A bulk update whose new value outgrows its slot repacks the leaf** with every change of its
   run, logged as one `CompactLeaf` carrying the page. `edge.update.all` 2.0x.
6. **A recursive step that reads only the queue is evaluated directly**, without a pipeline per pass.
   `app.cte.recursive` 1.86x, `ai.build` 1.61x.
7. **`json_extract` over distinct documents builds only the part its path reads.**
   `app.json.where` 1.54x.
8. **A hash `GROUP BY` keeps its keys in one table** (`ops/group_table.rs`) and sorts by an
   eight byte key prefix. `edge.group.high` 1.54x.
9. **A bulk delete logs one `DeleteRows` record per leaf run.** `edge.delete.range` 4.0x.
10. **Correlated blocks keep their chain** across outer rows. `correlated.in` 1.50x.
11. **An index nested loop emits NULL for inner columns nobody reads; `LIKE '%x%'` searches for the
    first byte; a JSON call over literals is answered once.** `edge.like.contains` 1.45x,
    `extension.json` 1.57x, `edge.self.join` 1.19x.
12. **A correlated `EXISTS` whose path consumes its whole `WHERE` is answered by an index probe**;
    the correlated `IN` lowering drops its NULL arms when both sides are declared `NOT NULL`; a
    correlated block's outer reads are listed instead of opaque; the window pass stops copying its
    rows; a `GROUP BY` on the outer term streams through joins. `correlated.in` 6.3x,
    `correlated.exists` 2.3x, `app.window.rank` 1.47x, `app.dashboard` 1.13x.
13. **Correlated blocks and their chains are kept with the cached statement; CRC-32 runs four
    streams side by side** (12.19 us to 3.93 us for a 32 KiB page, the same answer).
    `ai.copy` 1.12x, `prepare.point` 1.08x.
14. **A `GROUP BY` term is not a bare column.** `SELECT key % 50000, count(*) ... GROUP BY 1` fed a
    copy of every row to an accumulator for `key`, which nothing read. Integer `%` and `/` by a
    constant take one instruction. `edge.group.high` 2.0x, peak memory 12% less.
15. **A one row recursion pass builds nothing; two integers add and compare without conversion.**
    `app.cte.recursive` 1.33x.
16. **A planned FROM term shares its `TableInfo`; a buffered token is returned without a call.**
    Compiling a short aggregate went from 9.8 us to 6.7 us.

## 4. Tried and not kept

| Change | Measured | Why it was reverted |
|---|---|---|
| A remembered leaf for a correlated `EXISTS` probe | 90 us to 100 us | the index is two levels deep, so checking the leaf cost what the descent did |
| An index nested loop that reads its inner side into a hash table after enough probes | `app.dashboard` 0.83x to 0.45x | a probe costs about 184 ns, and reading a row into the table costs about the same |
| A finishing step for the table hasher | `ai.group.total` 0.91, `edge.group.high` 0.93 | the old hash clustered keys inserted in order, and the caches liked that. A test proved the clustering: 20,000 encoded integers fell into 1,024 of 65,536 low bit buckets |
| Merging a leaf only below a third full | `churn.delete.half` 0.36x to 0.84x, `churn.refill` 0.77x to 0.32x | the inserts that follow land in half tombstoned leaves; the two cancel in the score |
| An interpolation search that gallops from one guess | no change | the probe is bound by cache misses, not by the search |
| A plain byte scan in the JSON string parser | no change | |

## 5. The workloads still slower than SQLite

| Workload | Ratio | What the profile says |
|---|---:|---|
| `churn.delete.half` | 0.34x | a quarter of the delete merges half empty leaves; not merging them moves the cost to `churn.refill` (section 4). Needs inserts into tombstoned leaves to be as cheap as into clean ones |
| `correlated.scalar.selective`, `correlated.exists.selective` | 0.55x, 0.58x | the gate times one execution with its prepare, and its warming reads tables but not indexes, so the four probes read cold 32 KiB index pages and check their CRC. At 50 executions per sample both win, 1.12x and 1.23x |
| `edge.update.all` | 0.64x | 1.04x when run alone; in the plan SQLite's cache is warm from the inserts before it and its time falls from 22 ms to 14 ms while ours stays near 22 ms. The cost is repacking every leaf, a split when a bulk built leaf has no room for the growth, and an undo image per row |
| `check.counters` | 0.74x | one execution of a small aggregate, so it is mostly compile time |
| `churn.refill` | 0.77x | single row inserts in one transaction into the fresh table's indexes |
| `app.dashboard` | 0.84x | 12,500 index probes into `side_owner`, most finding nothing; each is a descent and a leaf search bound by cache misses |
| `app.insert.prepare_each` | 0.84x | single row inserts; a fifth is `make_room` on the indexes' leaves |
| `app.json.where` | 0.84x | a third is parsing the document; the rest is the allocations of the answer |
| `ai.copy` | 0.84x | `INSERT ... SELECT` of 100,000 rows |
| `join.range` | 0.89x | the same probe cost as `app.dashboard` |
| `correlated.in` | 0.91x | 1.18x on the correlated family alone; in the full plan it moves with the cache |
| the rest | 0.95x to 0.98x | `extension.fts.build`, `txn.autocommit`, `edge.delete.range`, `range.lookaside`, `edge.in.long`; within two readings of the same build of each other |

## 6. Tools added

- `inillucent-fullgate --workloads a,b` runs only the named workloads, so one workload can be timed
  and profiled on the gate's own database. A run that names workloads grades nothing.
- `inillucent-hcprobe --prepare-each --no-plan-cache` compiles the statement for every execution,
  which is what a workload the gate runs once pays.
