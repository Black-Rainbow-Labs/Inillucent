# Performance history

This page keeps every earlier measurement of inillucent against SQLite: the last graded run of the
engine gate, each hill climb through the Connection, each round of the comparison of how programs
call it, and the investigations behind them. [Performance](performance.md) has the current figures.
A ratio on this page is SQLite's time divided by inillucent's; a ratio of 2.00x is 100% faster and
one of 0.50x is 100% slower.

## The graded gate run of 26 September 2026

The last engine gate run that passed the quiet check. `main` at `bc46bc9`, release 1.0.32.


| | |
|---|---|
| **What was compared** | inillucent against SQLite 3.53.4, built from the official source and run as a separate program over the same data, with the same SQL |
| **Settings on both engines** | `synchronous = FULL`. A 128 MiB page cache: 4,096 frames of 32 KiB for inillucent, `PRAGMA cache_size = -131072` for SQLite. No plan cache on either |
| **Data** | the medium fixture, 100,000 rows |
| **Machine** | Windows 11 on x64, Intel Core Ultra 9 285 (8 performance cores, 16 efficiency cores). Both engines ran on the same eight performance cores. No other work was running |
| **Method** | `inillucent-fullgate`, 34 workloads in ten weighted families, 30 paired rounds a run, four runs in a row, the median of the two middle runs |
| **Correctness check** | every workload's answer is hashed and compared with SQLite's before its time counts. All 34 workloads agreed on every round of all four runs |
| **Quiet check** | each run compares SQLite's own speed with this machine's recorded idle reference. The four runs read 1.06%, 1.00%, 1.43% and 1.16% slower than the reference, under the 3% limit, so all four were graded |
| **Run** | 26 September 2026, `main` at `bc46bc9`, which is release 1.0.32. Every number on this page comes from this run unless a section gives another date |

| Measure | SQLite 3.53.4 | inillucent | Result |
|---|---|---|---|
| **Elapsed time**, weighted over the ten families | the reference | 5.19x | **419% faster** |
| **Elapsed time**, 95% lower bound | | 5.05x | **405% faster**. The contract asks for 200% |
| **Processor time**, one round of the whole plan | 1,043 ms | 344 ms | **67% less processor time**, a ratio of 0.33. The contract asks for 0.40, so this bar is met |
| **Peak resident memory**, one round of the whole plan | 37.21 MiB | 41.48 MiB | **11.5% more**. The one loss |
| **Database file**, the same imported fixture | 16,830,464 B | 17,432,576 B | **3.6% larger** (1.036x) |

The four runs read 5.15x, 5.20x, 5.32x and 5.17x, with 95% lower bounds of 4.73x, 5.10x, 5.16x and
5.00x. Processor time met its bar on all four runs, at ratios of 0.32, 0.31, 0.35 and 0.33.

**Slower than SQLite.** Thirty workloads count toward the headline. Twenty five are faster than
SQLite. Five are slower: `prepare.trivial` (85% slower), `join.range` (4%), `txn.autocommit` (4%),
`extension.json` (3%) and `extension.fts.build` (3%). Four more workloads, the correlated
subqueries, do not count toward the headline. Two of them are faster than SQLite and two are slower.
[The workloads that are slower](#the-workloads-that-are-slower) has each one.

### By family

`weight` is the family's share of the headline. `bar` is the ratio the contract asks of the family.
The lower bound is the per round statistic every gate uses, and the column gives the lowest and
highest of the four runs. [How a family's interval is computed](performance.md#how-a-familys-interval-is-computed)
explains it.

| family | weight | what it measures | result | 95% lower bound, four runs | bar |
|---|---|---|---|---|---|
| `read.point` | 16% | one row by rowid, by integer key, and through a secondary index | **3,061% faster** (31.61x) | 30.10x to 31.71x | 2.00x, met |
| `large.values` | 4% | text and blobs across the size where a value stops fitting in a leaf | **1,132% faster** (12.32x) | 12.02x to 12.13x | 1.50x, met |
| `read.analytical` | 10% | scans, aggregates, `GROUP BY`, `DISTINCT`, sorts | **1,013% faster** (11.13x) | 10.87x to 11.11x | 5.00x, met |
| `read.range` | 12% | selective ranges, forward and reverse, covering and not | **435% faster** (5.35x) | 5.11x to 5.33x | 3.00x, met |
| `read.join` | 8% | two table and four table joins | **369% faster** (4.69x) | 4.59x to 4.64x | 3.00x, met |
| `write` | 20% | insert, update, delete, upsert, with and without indexes | **257% faster** (3.57x) | 2.43x to 3.32x | 1.50x, met |
| `transaction` | 10% | autocommit, small batches, large batches, savepoints | **138% faster** (2.38x) | 1.75x to 2.21x | 1.00x, met |
| `extension` | 8% | JSON, FTS5, R-Tree | **79% faster** (1.79x) | 1.73x to 1.75x | 1.50x, met |
| `open.prepare` | 8% | parse, bind, step one row, reset | **65% faster** (1.65x) | 1.62x to 1.65x | 5.00x, missed |
| `schema` | 4% | `CREATE INDEX` and its backfill | **27% faster** (1.27x) | 1.17x to 1.28x | 3.00x, missed |

**The release condition is that no family is below the 1.00x floor.** No family went under it on any
of the four runs. `schema` is the closest, at a lower bound of 1.17x. `schema` is one workload,
`schema.index`, run once a round, so its interval is the widest on the page.

**`write`** moved the most between runs: 3.03x, 3.80x, 3.83x and 3.34x. Its workloads write and
`fsync` the log, so they depend on the disk as much as on the engine.
[What this page does not measure](performance.md#what-this-page-does-not-measure) has what the disk does to a
sequence of runs.

**`extension`** reads `extension.rtree.query` 5.10x, `extension.rtree.insert` 2.35x,
`extension.fts.query` 1.46x, `extension.json` 0.97x (3% slower) and `extension.fts.build` 0.97x (3%
slower, 10.4 µs a document against 10.2).

**`read.join`** reads `join.selective` 22.90x and `join.range` 0.96x (4% slower).

**`transaction`** reads 2.38x. A commit is one append to the log and one `fsync` of the log. The log
is copied into the file when it has grown past 4 MiB, when a caller asks, or when the connection
closes. On the gate's own counters, `txn.autocommit`'s hundred statements make 100 log writes, 100
log syncs, no data file syncs and no checkpoints. `txn.autocommit` reads 0.96x (4% slower), 1.15 ms a
statement against SQLite's 1.11 ms. Both engines do one `fsync` a commit, and on this disk the
`fsync` is most of the millisecond. `txn.batched` reads 3.55x and `txn.large` 3.83x, 2.63 ms a round
against SQLite's 10.08 ms.

#### What moved since 2026-09-23

The graded run of 23 September 2026 was taken at `main` at `6f84ce6` with `7f93661` applied, with
the same method and the same machine.

| family | 2026-09-23 | 2026-09-26 | change |
|---|---:|---:|---|
| `write` | 3.04x | **3.57x** | 17% faster |
| `read.join` | 4.21x | **4.69x** | 11% faster |
| `read.range` | 5.02x | **5.35x** | 7% faster |
| `read.point` | 29.85x | **31.61x** | 6% faster |
| `read.analytical` | 10.71x | 11.13x | 4% faster |
| `extension` | 1.73x | 1.79x | 3% faster |
| `transaction` | 2.37x | 2.38x | no change |
| `large.values` | 12.35x | 12.32x | no change |
| `open.prepare` | 1.69x | 1.65x | 2% slower |
| `schema` | 1.31x | 1.27x | 3% slower, inside its own noise |
| **weighted** | **4.97x** | **5.19x** | **4% faster** |
| **processor time, inillucent over SQLite** | 0.500 | **0.33** | bar of 0.40 met |
| **peak memory** | 40.76 MiB | 41.48 MiB | 0.72 MiB more |

- **Processor time.** The four `read.correlated` workloads took 182 ms of each round on 23 September.
  They now take 1.3 ms. Each execution used to grow a list of parameters to 100,001 entries (3.2 MB)
  and free it again. That list is gone. The processor figure is one round of the whole plan, so it
  includes these workloads, and it went from 555 ms to 344 ms.
- **`read.join` and `read.range`.** `join.range` went from 0.84x to 0.96x and `range.lookaside` from
  0.96x to 1.11x. Both make 200 rowid probes. Since `52c4b5f` a probe reads a leaf column once and no
  longer copies a probe key whose affinity changes nothing.
  [Measured again at `52c4b5f`](#measured-again-at-52c4b5f-on-2026-09-24-and-not-graded) has the
  ungraded passes that first showed it.
- **`write`.** `write.insert.batch` went from 1.47x to 1.58x, `write.update.indexed` from 3.37x to
  3.58x and `write.delete` from 4.16x to 4.40x. The family's four runs spread from 3.03x to 3.83x, so
  part of the change is the disk.
- **Memory.** One of the four runs read 43.37 MiB and the other three 40.99 to 41.92 MiB. `schema.index`
  sets the peak in each. [Memory](#memory) has the detail.

### The workloads that are slower

Thirty workloads count toward the headline. Twenty five are faster than SQLite. These five are not:

| workload | family | ratio | how much slower | time per operation | why |
|---|---|---|---|---|---|
| `prepare.trivial` | `open.prepare` | 0.54x | **85% slower** | 742 ns against 405 | `SELECT 1` is compiled on every call. `inillucent-prepareprofile` counted 13 allocations on the path the gate times on 23 September. Nine of the thirteen are part of the compiled statement: the bound result columns, the column name, the `Box<BoundSelect>`, the projection expression tree and the output names |
| `join.range` | `read.join` | 0.96x | **4% slower** | 50.2 µs against 48.3 | an index range and a probe per entry. SQLite spreads one statement's overhead over two hundred rows |
| `txn.autocommit` | `transaction` | 0.96x | **4% slower** | 1.15 ms a statement against 1.11 | one `fsync` a commit on each engine. On this disk an `fsync` is most of the millisecond |
| `extension.json` | `extension` | 0.97x | **3% slower** | 290 ns a call against 284 | the extraction, plus one uncontended mutex and two comparisons a call. The parse of a repeated document and path is already cached |
| `extension.fts.build` | `extension` | 0.97x | **3% slower** | 10.4 µs a document against 10.2 | FTS5's build is four ordinary row writes a document, so it follows the write path |

`range.lookaside` was on this list on 23 September at 0.96x. It now reads 1.11x, 11% faster than
SQLite.

**Four more workloads are not weighted by the contract.** They are the `read.correlated` family: a
subquery that names a column of the outer query and is answered once per outer row. They are graded
against the join that asks the same question, so they have no family bar and no place in the
headline. They are in the plan, so they are in the processor and memory figures.

| workload | inillucent | SQLite | result |
|---|---|---|---|
| `correlated.exists`, `EXISTS` over 400 outer rows | 0.38 ms | 0.27 ms | **41% slower** |
| `correlated.in`, `IN (SELECT ...)` over the same | 0.89 ms | 0.099 ms | **807% slower** |
| `correlated.exists.selective`, a filter keeps 4 outer rows | 21.8 µs | 22.3 µs | **2% faster** |
| `correlated.scalar.selective`, a scalar subquery, 4 outer rows | 20.2 µs | 21.3 µs | **6% faster** |

On 23 September the first two read 59.69 ms and 118.19 ms. A correlated `IN` over 400 outer rows is
still 807% slower than SQLite, so write a correlated `IN` against a large table as a join.

The fastest workloads in the same run: `scan.aggregate` 54.96x, `point.miss` 52.35x, `large.read`
44.24x, `point.rowid` 34.58x, `scan.group` 27.51x, `join.selective` 22.90x, `point.index` 17.93x,
`range.reverse` 16.23x and `range.covering` 8.54x.

### Memory

**Peak resident memory is 41.48 MiB against SQLite's 37.21 MiB, 11.5% more.** The contract asks for
5% less, so this bar is missed. The four runs read 43.37, 41.04, 40.99 and 41.92 MiB. SQLite read
37.20 or 37.21 MiB on every run.

Each engine's figure comes from one child process that opens a finished file the parent built and
runs one round of the plan. The processor figure is measured the same way. Neither figure is a
difference taken inside a running program.

The peak after each workload, from the second run:

| workload | peak MiB | rise MiB | page pool MiB | everything else MiB |
|---|---:|---:|---:|---:|
| the file opened and the pool warmed | 25.49 | 25.49 | 16.56 | 8.93 |
| every read workload, the correlated ones included | 25.56 | at most 0.02 | 16.56 | 9.00 |
| `write.insert.batch` | 28.76 | 3.20 | 16.84 | 11.92 |
| `write.update.indexed` and the transaction workloads | 30.38 | 1.62 in all | 16.84 | 11.57 |
| `schema.index` | **41.04** | **10.66** | 22.88 | 11.51 |

The correlated workloads no longer raise the peak. On 23 September they raised it by 6.19 MiB, from
the list of parameters described under [What moved](#what-moved-since-2026-09-23). The write
workloads now raise it instead, by 4.82 MiB in all. `schema.index` sets the peak.

Where the 4.3 MiB difference is:

| | inillucent | SQLite | what it is |
|---|---|---|---|
| the cached database | 16.56 MiB | about 16 MiB | the `.rdb` is 1.036x the size of the `.db`. Under 0.6 MiB of the difference is here |
| the process floor | 8.93 MiB | about 4.2 MiB | 3.62 MiB is what any Rust program costs on this machine before the engine starts, measured on 23 September. A 130 KB program whose `main` reads its own working set and returns peaks at 3.62 MiB over five runs, 0.66 MiB of it private. The rest is the engine's code, its statics and opening the file |
| `schema.index`'s rise | 10.66 MiB | about 15.9 MiB, measured on 23 September | the pages of the new index plus the sort's arena. inillucent's rise is smaller than SQLite's |

Most of the difference is the operating system's cost of a process. The engine's own allocator is
not part of it. The same 130 KB program built with `inillucent-alloc` as its global allocator
measures 3.62 MiB, with 0.66 MiB private. `inillucent-alloc` is a free list per size class. Each list
starts empty and hands a block back to the system allocator when the class is full, so it reserves
nothing up front.

The one remaining place to save memory is the index build's sort arena. An earlier design measured
spilling the sorted run to a temporary file at about 8 ms on a 27 ms statement. That would put the
`schema` family under the 1.00x floor, so it is not done.

[Memory, earlier measurements](#memory-earlier-measurements) has how the figure went from 102% more
to 9.5% more by 23 September.

### Other table sizes

The headline uses the medium fixture, 100,000 rows. The small and large fixtures were measured
straight after the medium runs on 26 September 2026, pinned the same way, two runs each. This
machine has no recorded idle reference at those two sizes, so those runs are graded without the
quiet check.

| | 5,000 rows | 100,000 rows | 600,000 rows |
|---|---|---|---|
| weighted | 3.99x, **299% faster** | 5.19x, **419% faster** | 5.60x, **460% faster** |
| 95% lower bound | 3.88x | 5.05x | 5.40x |
| `write` | 1.23x, **23% faster**, lower bound 0.98x on one run, under the floor | 3.57x | 6.92x, **592% faster** |
| `schema` | 1.32x | 1.27x | 1.23x, lower bound 0.79x on one run, under the floor |
| `extension` | 1.85x, lower bound 1.82x | 1.79x, lower bound 1.73x | 2.05x, lower bound 1.99x |
| processor, inillucent against SQLite | 234 ms against 805, **71% less** | 344 against 1,043, **67% less** | 570 against 891, **36% less** |
| peak memory, inillucent against SQLite | 14.40 MiB against 9.35, **54% more** | 41.48 against 37.21, **11.5% more** | 188.50 against 181.82, **3.7% more** |

`write` improves as the table grows, because a bigger table spreads a statement's setup cost over
more of a page. At 5,000 rows three of the five `write` workloads are slower than SQLite:
`write.insert.batch` 0.54x, `write.update.indexed` 0.57x and `write.delete` 0.57x. At 600,000 rows
`txn.autocommit` reads 0.65x, 54% slower. At 5,000 rows the memory figure is mostly the process
floor described under [Memory](#memory).

On 23 September inillucent used 30% more processor time than SQLite at 600,000 rows. It now uses 36%
less.

### Linux

**The same binary measured 53% faster than SQLite on Linux when Windows measured 279% faster.** The
Linux figure has not been measured again since. The Windows headline is now 419%. Compare the two
old figures with each other. Neither one is comparable with the headline.

The cause is SQLite's side, and the allocator on inillucent's side. With a free list per size class
in place of the system allocator, a `SELECT 1` compile goes from 46.95 ms to 38.97 ms on Windows (17%
faster) and from 39.91 ms to 38.20 ms on Linux (4% faster). The two platforms then run at the same
speed. On the Windows compile, the C runtime's heap was 59% of the time. SQLite does work with the
operating system on every statement, and Windows charges more for that work than Linux does. SQLite's
time is the denominator of every ratio, so the ratio changes between platforms while inillucent's own
time does not.

Neither the allocator change, which took the Windows medium gate from 3.24x to 3.86x, nor any later
change has been measured on Linux.

## Through the Connection, on common and edge case workloads

The headline above calls the engine's `pipeline` functions directly. An application calls
`Connection::prepare` and `Statement::step`, and pays for the plan cache lookup, a `String` per
result column, and the file lock a statement takes outside a transaction. `--api connection` runs
the gate through those calls. `--plan hillclimb` replaces the contract plan with 48 workloads an
application meets and the contract plan does not: reading right after a batch of inserts, deleting
and refilling half a table, inserting with `RETURNING`, a recursive CTE, an `OR` of two indexed
columns, `NOT IN` over a subquery, `LIKE '%text%'`, a `GROUP BY` with 50,000 groups, and an
`INSERT ... SELECT` of 100,000 rows.

```sh
inillucent-fullgate medium.db --plan hillclimb --api connection --rounds 10 --samples hc.samples
inillucent-fullgate medium.db --api connection --rounds 10 --samples cc.samples
node tools/perf-hillclimb/score.mjs --base before.samples --cand after.samples
```

A third of the workloads, chosen by a hash of the name, are a test set. A change was kept only when
it made the other two thirds faster and did not make the test third slower, so a change that helps
only the workload it was written for shows up as a test set that did not move.

Measured on 3 October 2026, ten rounds each, on the machine in the summary, with release 2.1.1
and the two answer fixes in the changelog before the changes, and release 2.1.2 after them. A speed row is the geometric mean of each set's
ratios against SQLite.

| Through the Connection | 2.1.1 | 2.1.2 | Change in inillucent |
|---|---|---|---|
| hillclimb plan, 28 train workloads | 18% slower than SQLite | **30% faster** than SQLite | 53% faster |
| hillclimb plan, 20 test workloads | 49% slower than SQLite | **15% faster** than SQLite | 71% faster |
| hillclimb plan, all 48 workloads | 30% slower than SQLite | **24% faster** than SQLite | 60% faster |
| hillclimb plan, peak resident memory of one round | 100.96 MiB, 164% more than SQLite | 82.82 MiB, 116% more than SQLite | 18% less memory |
| hillclimb plan, processor time of one round | 3,906 ms, 115% more than SQLite | 1,750 ms, 7% less than SQLite | 55% less processor time |
| contract plan, weighted over the ten families | 94% faster than SQLite | **137% faster** than SQLite | 22% faster |

The contract plan through the pipeline, the headline at the top of this page, read 412% faster
before and 410% faster after, which is the same within a run's noise.

The four largest changes are clearest as times. Each is the median time for the workload's
repetitions in one round.

| Workload | inillucent 2.1.1 | inillucent 2.1.2 | SQLite | What changed |
|---|---|---|---|---|
| `edge.or.two.indexes`, 100 queries | 588 ms | 0.88 ms | 1.38 ms | an `OR` whose every arm can use an index reads each arm's index and then each row once, shown as `MULTI-INDEX OR` |
| `edge.not.in`, 2 queries | 958 ms | 4.44 ms | 5.01 ms | `IN` and `NOT IN` over 8 or more constants, written out or from a subquery, are answered by a binary search |
| `extension.json`, contract plan, 4,000 statements | 24.4 ms | 1.70 ms | 1.12 ms | a statement that reads no table and calls no registered function takes no file lock |
| `prepare.trivial`, contract plan, 4,000 statements | 23.3 ms | 0.82 ms | 1.62 ms | the same change, for `SELECT 1` |

The other workloads that moved, as the change in inillucent's own speed:

| Workload | Against SQLite, 2.1.1 | Against SQLite, 2.1.2 | Change in inillucent | What changed |
|---|---|---|---|---|
| `app.insert.prepare_each` | 424% slower | 37% slower | 282% faster | the next rowid is read from the last row of the rightmost leaf instead of from every live row in it |
| `ai.build` | 188% slower | 20% faster | 246% faster | the same |
| `app.cte.recursive` | 571% slower | 116% slower | 211% faster | a recursive CTE prepares its step once and runs it on each pass |
| `edge.like.contains` | 205% slower | 17% slower | 160% faster | `LIKE` with no `ESCAPE` compares text in place |
| `app.upsert.counter` | 7% slower | 129% faster | 145% faster | an `UPDATE` uses the leaf the read before it found |
| `edge.group.high` | 320% slower | 122% slower | 89% faster | a hash `GROUP BY` keeps each group's position in the map and its accumulators in one vector |
| `app.insert.returning` | 435% slower | 189% slower | 85% faster | the rowid change |
| `edge.update.all` | 510% slower | 261% slower | 69% faster | the `UPDATE` change |
| `churn.delete.half` | 502% slower | 288% slower | 56% faster | a bulk delete looks first in the leaf the last delete used |
| `ai.group.total` | 73% faster | 169% faster | 55% faster | the `GROUP BY` change |
| `edge.delete.range` | 625% slower | 446% slower | 32% faster | the bulk delete change |
| `ai.copy` | 452% slower | 350% slower | 23% faster | `INSERT ... SELECT` reads the query's rows where they are instead of copying them |
| `churn.scan` | 48% slower | 25% slower | 18% faster | a scan of a table that took writes fills its columns without a vector per row |
| `check.fresh` | 33% slower | 17% slower | 14% faster | the same |
| `ai.count` | 117% slower | 97% slower | 10% faster | the same |
| `app.json.where` | 111% slower | 93% slower | 9% faster | `json_extract` over distinct documents stops keeping each one |

Five workloads were still more than 100% slower than SQLite after 2.1.2: `edge.delete.range`
(446% slower), `ai.copy` (350%), `churn.delete.half` (288%), `edge.update.all` (261%) and
`app.insert.returning` (189%). Each writes many rows, and each logs a page image for every leaf it
changes where SQLite logs less.

### A copy into an empty table

`ai.copy` is `INSERT INTO copied SELECT ... FROM main_table`, 100,000 rows into an empty table. It
now builds the table's tree in one pass, the way `CREATE INDEX` builds an index, and logs no page
images ([how it works](relational-architecture.md#a-copy-into-an-empty-table)). Measured on 3
October 2026, ten rounds, two runs of each build, with the two other tickets on this machine paused:

| Through the Connection | Before | After | Change in inillucent |
|---|---|---|---|
| `ai.copy` | 191.0 ms, 352% slower than SQLite | 55.8 ms, 35% slower than SQLite | 242% faster |
| hillclimb plan, 28 train workloads | 30% faster than SQLite | **40% faster** than SQLite | 8% faster |
| hillclimb plan, 20 test workloads | 13% faster than SQLite | **15% faster** than SQLite | 1% faster |
| peak resident memory of one round | 79.75 MiB | 75.53 MiB | 5% less memory |
| processor time of one round | 1,813 ms | 1,609 ms | 11% less processor time |

The same copy into a table with a plain index and a unique index took 715 ms to 1,213 ms before and
109 ms to 132 ms after, against 132 ms to 146 ms in SQLite, timed with each engine's shell.

Four other workloads moved, and none of them runs the bulk build. `edge.delete.range` (443% slower
than SQLite before, 181% slower after) and `app.insert.returning` (199% slower before, 5% slower
after) are faster. `churn.update.grow` (266% faster than SQLite before, 151% faster after) and
`edge.update.all` (265% slower before, 435% slower after) are slower. The cause is the log
housekeeping that runs once the log has grown by 4 MiB. The old copy wrote enough log to run it
during `ai.copy`; the new copy writes almost none, so the next two runs of it land on other
workloads. Each pair moves by about 24 ms in opposite directions, and the plan's time without
`ai.copy`, summed over the medians of every other workload, is 1,845.4 ms and 1,837.6 ms before and
1,845.5 ms and 1,837.8 ms after. `edge.delete.range` also gains from deleting out of packed leaves.

The three workloads bound by `fsync` (`txn.autocommit`, `txn.batched` and
`write.insert.autocommit`) read slower in the 2.1.2 run than in the 2.1.1 run, and the write
workloads of the contract plan read faster. SQLite's own time for the three rose 3.3 to 5.6 times
between the two runs, so the disk was slower. Run again back to back on the same disk, the two
builds read the same: `txn.autocommit` 6% and 3% faster than SQLite, `write.insert.autocommit`
212% and 215% faster. Neither the losses nor the gains of those workloads are counted above.

### A `DELETE` or `UPDATE` of many rows

A `DELETE` or `UPDATE` that nothing can watch now changes each leaf once for all of its rows there,
with the same log records as before. The second hill climb below changed the records a bulk
`DELETE` writes
([how it works](relational-architecture.md#a-delete-or-update-of-many-rows)). Measured on
3 October 2026 through the Connection, hillclimb plan, ten rounds, two runs of each build in
turn, with three other tickets working on this machine:

| Through the Connection | Before | After | Change in inillucent |
|---|---|---|---|
| `edge.delete.range`, 40,000 rows | 32.5 ms, 181% slower than SQLite | 23.8 ms, 107% slower than SQLite | 35% faster |
| `churn.delete.half`, three trees | 72.5 ms, 289% slower than SQLite | 64.2 ms, 246% slower than SQLite | 13% faster |
| `edge.update.all`, every row of `side_table` | 75.3 ms, 435% slower than SQLite | 68.8 ms, 374% slower than SQLite | 13% faster |
| hillclimb plan, 28 train workloads | 40% faster than SQLite | **43% faster** than SQLite | 2% faster |
| hillclimb plan, 20 test workloads | 15% faster than SQLite | **16% faster** than SQLite | 1% faster |
| peak resident memory of one round | 75.46 MiB | 75.93 MiB | 0.6% more memory |
| processor time of one round | 1,633 ms | 1,562 ms | 4% less processor time |

Each row still writes one log record and one undo entry, which is most of what is left of the
difference from SQLite. An `UPDATE` gains less than a `DELETE` because a row whose new value does
not fit where the old one lies is still written row by row. In a profile of
`UPDATE copied SET label = label || '!'` over 100,000 rows, that path was a quarter of the time spent writing rows. `edge.group.high` read 6% slower in this comparison
and 0.2% faster in an earlier one of the delete change alone; it is a `GROUP BY` that writes
nothing, and it is recorded here as noise.

### A second hill climb

Release 2.1.2 was still slower than SQLite on 29 of the 82 workloads the two plans run through the
Connection. A second hill climb, by the same rules, took that to 17. Measured on 4 October 2026,
ten rounds, two runs of each build in turn, release 2.1.2 against this release:

| Through the Connection | 2.1.2 | This release | Change in inillucent |
|---|---|---|---|
| hillclimb plan, 28 train workloads | 44% faster than SQLite | **84% faster** than SQLite | 28% faster |
| hillclimb plan, 20 test workloads | 20% faster than SQLite | **50% faster** than SQLite | 25% faster |
| hillclimb plan, all 48 workloads | 34% faster than SQLite | **69% faster** than SQLite | 26% faster |
| contract plan, 25 train workloads | 88% faster than SQLite | **119% faster** than SQLite | 16% faster |
| contract plan, 9 test workloads | 67% faster than SQLite | **94% faster** than SQLite | 16% faster |
| hillclimb plan, peak resident memory of one round | 77.5 MiB, 102% more than SQLite | 60.6 MiB, 58% more than SQLite | 22% less memory |
| hillclimb plan, processor time of one round | 1,586 ms, 16% less than SQLite | 1,250 ms, 34% less than SQLite | 21% less processor time |

The 2.1.2 column reads faster here than in the table above because it is a different day's run of
the same build; both builds in this table were run on the same day, in turn.

| Workload | Against SQLite, 2.1.2 | Against SQLite, now | Change in inillucent | What changed |
|---|---|---|---|---|
| `correlated.in` | 900% slower | 10% slower | 810% faster | a correlated `IN` over two `NOT NULL` columns is one `EXISTS`, and an `EXISTS` whose `WHERE` is an index equality on the outer row is answered by probing the index |
| `edge.group.high` | 124% slower | 52% faster | 241% faster | a `GROUP BY` term in the select list is read from the group's key instead of kept from a row, and the groups' keys are kept in one table |
| `ai.count` | 98% slower | 72% faster | 240% faster | a reader keeps a packed copy of a leaf that took writes once it has read it twice |
| `edge.update.all` | 367% slower | 56% slower | 199% faster | a leaf whose rows outgrew their slots is repacked once with all its changes |
| `correlated.exists` | 78% slower | 62% faster | 188% faster | the index probe above |
| `app.cte.recursive` | 115% slower | 12% faster | 141% faster | a recursive step that reads only its queue is evaluated directly, and a one row pass builds nothing |
| `extension.json` | 49% slower | 45% faster | 116% faster | a JSON call over literals is answered once |
| `check.fresh` | 15% slower | 85% faster | 114% faster | the packed copy, and a scan reads only the columns the statement reads |
| `edge.delete.range` | 108% slower | 4% slower | 100% faster | a bulk delete logs one record per leaf it changes |
| `ai.group.newest` | 56% slower | 23% faster | 92% faster | the packed copy |
| `churn.scan` | 27% slower | 36% faster | 73% faster | the column change |
| `app.json.where` | 93% slower | 19% slower | 62% faster | `json_extract` builds only the part of a document its path reads |
| `edge.like.contains` | 17% slower | 26% faster | 47% faster | `LIKE '%text%'` searches for the first byte before comparing |
| `edge.in.long` | 47% slower | 2% slower | 44% faster | the column change |
| `app.window.rank` | 17% slower | 23% faster | 43% faster | a window computes its values over its rows and moves them, where it copied every row five times |

Of the 17 workloads still slower, five are more than 25% slower. `churn.delete.half` (192% slower)
merges half empty leaves; leaving them unmerged made the delete 0.84 of SQLite's speed and the
refill that follows it 0.32. `correlated.scalar.selective` and `correlated.exists.selective` (83%
and 71% slower) answer four outer rows, and the gate times one execution with its compile: the
round's warming reads every table but no index, so the four probes read cold index pages. At 50
executions a sample both are faster than SQLite. `edge.update.all` (56% slower) is 4% faster than
SQLite when it runs alone; in the plan, SQLite's cache is warm from the inserts before it.
`check.counters` (35% slower) is one execution of a small aggregate, most of it compile.
`tasks/task-2183-performance-hillclimb-2.md` lists every change, every measurement and the changes
that were tried and not kept.

### A third hill climb

A third hill climb by the same rules took the workloads slower than SQLite from 16 to 12. Measured
on 5 October 2026 through the Connection, ten rounds, three runs of each build in turn on each
plan, release 2.1.3 against the build that follows it, which was released as 2.1.5. Release 2.1.4
changed version numbers and a test harness and no engine code, so the 2.1.3 column is also 2.1.4:

| Through the Connection | 2.1.3 | After the third hill climb | Change in inillucent |
|---|---|---|---|
| hillclimb plan, 28 train workloads | 87% faster than SQLite | **116% faster** than SQLite | 16% faster |
| hillclimb plan, 20 test workloads | 50% faster than SQLite | **77% faster** than SQLite | 18% faster |
| contract plan, 25 train workloads | 121% faster than SQLite | **140% faster** than SQLite | 9% faster |
| contract plan, 9 test workloads | 97% faster than SQLite | **108% faster** than SQLite | 5% faster |
| hillclimb plan, processor time of one round | 1,375 ms | 1,156 ms | 16% less processor time |
| contract plan, processor time of one round | 453 ms | 375 ms | 17% less processor time |
| hillclimb plan, peak resident memory of one round | 59.16 MiB | 56.91 MiB | 4% less memory |
| contract plan, peak resident memory of one round | 42.59 MiB | 43.87 MiB | 3% more memory |

The contract plan's memory rose because a prepared `INSERT` now keeps its compiled plan and
`VALUES` expressions between executions.

| Workload | Against SQLite, 2.1.3 | Against SQLite, now | Change in inillucent | What changed |
|---|---|---|---|---|
| `ai.copy.scan` | 11% faster | 250% faster | 216% faster | `length()` counts eight ASCII bytes at a time and finds the terminating NUL in the same pass |
| `check.copied` | 15% faster | 230% faster | 188% faster | the same |
| `point.rowid` | 105% faster | 161% faster | 28% faster | an autocommit statement asks the log's length with `GetFileSizeEx`, which asks Windows for the size alone. A point select in autocommit went from 6.79 µs to 5.10 µs |
| `churn.delete.half` | 200% slower | 139% slower | 26% faster | a merge packs both leaves straight from their pages, the delta area is searched from the previous key's place, and a deleted row is read from the copy of its leaf |
| `churn.refill` | 36% slower | 12% slower | 21% faster | a prepared `INSERT` keeps its plan, declarations and `VALUES` expressions between executions |
| `edge.delete.range` | 11% slower | 4% faster | 15% faster | a bulk delete's keys are slices of one buffer |
| `app.insert.prepare_each` | 23% slower | 11% slower | 11% faster | the kept `INSERT` setup |
| `app.json.where` | 26% slower | 17% slower | 7% faster | a literal `json_extract` path is parsed when the statement is compiled |
| `edge.update.all` | 63% slower | 52% slower | 7% faster | a bulk update's keys are slices of one buffer |

Of the 12 still slower, `churn.delete.half` (139% slower) spends a quarter of its time in a
checkpoint, which runs because the log passes 4 MiB during the statement. Most of a merge is
logging three whole pages.
`correlated.scalar.selective` and `correlated.exists.selective` (74% and 64% slower) are one
execution with its compile, about 38 µs against 22. `edge.update.all` (52% slower) grows every row
of leaves that a bulk build packed full, so each leaf is packed again and split.
`tasks/task-2185-performance-hillclimb-3.md` lists every change, every measurement and the changes
that were tried and not kept.

## Through the command line, the shell, Python and Node

Every number above times statements inside one process. Most people call inillucent another way: one
`inillucent` process per command, a SQL script piped into `inillucent-shell`, the Python driver, or
the npm package. A fourth hill climb measured those against the same use of SQLite and fixed what it
found. Measured on 5 October 2026, ten rounds, release 2.1.5 against the build after the fourth hill
climb, with no other work running. A second run of six rounds gave the same results to within a few
percent.

**Release 2.2.0 carries all three rounds.** Against SQLite, 2.1.5 was 216% slower over the 25
workloads and 2.2.0 is 46% faster, with 19 of the 25 faster. The sections below give each round.

| How it was called | SQLite measured as | 2.1.5 against SQLite | Now against SQLite | Change in inillucent |
|---|---|---|---|---|
| the command line, one process a command, 8 workloads | the `sqlite3` 3.53.4 shell, one process a command | 20% slower | **10% slower** | 9% faster |
| a SQL script into the shell, 6 workloads | the same scripts into `sqlite3` | 218% slower | **44% slower** | 121% faster |
| the Python driver, 9 workloads | Python's `sqlite3` module | 173% slower | **2% slower** | 168% faster |
| the npm package, 2 workloads | Node's `node:sqlite` | 29,270% slower | **2,223% slower** | 1,164% faster |
| all 25 | | 216% slower | **46% slower** | 118% faster |

Each group is the geometric mean of its workloads. Every engine ran with its own default settings,
because the defaults are what this kind of use gets. Both engines write the same rows; the answers
were not hashed and compared here the way the gate compares them.

| Workload | 2.1.5 | Now | SQLite | Change in inillucent |
|---|---:|---:|---:|---|
| `inillucent --db f query` of one row as JSON | 19.95 ms | 18.53 ms | 16.73 ms | 8% faster |
| `inillucent --db f exec` of a one row `INSERT` | 30.15 ms | 22.50 ms | 19.29 ms | 34% faster |
| `inillucent --version` | 16.82 ms | 16.32 ms | 15.92 ms | 3% faster |
| one `INSERT` of 20,000 rows, a row to a line, into the shell | 5,348.91 ms | 81.81 ms | 34.51 ms | 6,438% faster |
| 10,000 `INSERT` statements in a transaction into the shell | 98.88 ms | 77.69 ms | 33.52 ms | 27% faster |
| `.import --csv` of 50,000 rows | 163.62 ms | 153.54 ms | 55.97 ms | 7% faster |
| Python, read 10,000 rows | 92.64 ms | 13.55 ms | 7.78 ms | 584% faster |
| Python, 200 range queries of 200 rows | 159.48 ms | 38.28 ms | 35.77 ms | 317% faster |
| Python, 5,000 lookups by key | 96.11 ms | 74.93 ms | 85.17 ms | 28% faster, now faster than SQLite |
| Python, 10,000 inserts in a transaction | 142.41 ms | 67.09 ms | 25.67 ms | 112% faster |
| Python, 100 opens and closes | 1,672.61 ms | 100.14 ms | 7.73 ms | 1,570% faster |
| Node, a lookup by key through a session | 10.35 ms | 0.076 ms | 0.014 ms | 13,599% faster |

What changed:

- **Closing a database through the C library no longer checkpoints a file it only read.** That was
  9.3 ms of the 12 ms an open and close took from Python.
- **A close does the log's housekeeping only once the open log segment holds a mebibyte.** It still
  folds every time, so a closed file is complete without its log. Rolling a segment, writing a
  checkpoint record and deleting old segments at every close made a one row `exec` spend 12 ms
  closing.
- **The shell checks for the end of a statement only on a line holding `;` or `*/`.** Checking after
  every line made a statement spread over many lines quadratic.
- **The Python driver reads a result and binds a list of parameters in one call each**, through
  three new C library calls that carry them as JSON, and keeps its prepared statements. It took two
  foreign calls a cell before. `Connection.execute_many` is new.
- **The npm package has `open()`**, a session over one `inillucent-mcp` process, so a call no longer
  starts a program.
- **Windows builds link the C runtime statically and load the TLS client's three DLLs on first
  use**, which is what `sqlite3.exe` loads at start.
- Smaller changes to the write path's binding, the CSV parser, the JSON escaper and an open.

What is still slower, and why:

- **Single row inserts** (the shell scripts, `.import`, Python inserts): 2.3 to 2.8 times SQLite's
  time. The engine's own insert of a row into a tree with an index is the cost now, and it is the
  same path the gate's `churn.refill` and `app.insert.prepare_each` measure.
- **Python opens**: 1.0 ms against 0.08 ms. An open reads the meta record, opens the log and loads
  the schema; SQLite reads nothing until the first statement.
- **Node**: a session is a pipe to another process, about 0.08 ms a call against 0.014 ms for
  `node:sqlite` in process. Matching it needs a native binding over the C library.
- **The command line**: 1 to 2 ms a process, half of it the open.

`tasks/task-2191-performance-hillclimb-4.md` has every change, every measurement and the changes
that were tried and not kept.

### A second round of the same comparison

A second round measured the same 25 workloads again and fixed what it found. Measured on 5 October
2026, ten rounds, the build at the end of the first round against the build at the end of the
second, with no other work running.

| How it was called | After the first round, against SQLite | Now against SQLite |
|---|---|---|
| the command line, one process a command, 8 workloads | 11% slower | **1% slower** |
| a SQL script into the shell, 6 workloads | 42% slower | **35% faster** |
| the Python driver, 9 workloads | 9% slower | **77% faster** |
| the npm package, 2 workloads | 2,603% slower | **102% faster** |
| all 25 | 51% slower | **39% faster** |

The first round's own measurement of the same build, made earlier the same day, put the four
groups at 10%, 44%, 2% and 2,223% slower.

| Workload | After the first round | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---|
| `inillucent --version` | 16.37 ms | 16.56 ms | 15.85 ms | 4% slower |
| `query` of one row as JSON | 19.47 ms | 18.40 ms | 18.12 ms | 2% slower |
| `query` of one row as text | 20.18 ms | 17.34 ms | 18.38 ms | 6% faster |
| `query` of 100 rows by an index, sorted, as JSON | 21.49 ms | 19.88 ms | 18.22 ms | 9% slower |
| `query` of `count(*)` and `max` over 10,000 rows | 19.74 ms | 18.33 ms | 18.20 ms | 1% slower |
| `exec` of a one row `INSERT` | 27.40 ms | 24.39 ms | 22.09 ms | 10% slower |
| `tables` | 19.23 ms | 17.61 ms | 17.21 ms | 2% slower |
| `dump` | 36.97 ms | 31.49 ms | 34.83 ms | 11% faster |
| 1,000 lookups by key in a script | 40.19 ms | 35.49 ms | 40.93 ms | 15% faster |
| 200 single row inserts, each its own transaction | 290 ms | 272 ms | 912 ms | 235% faster |
| 10,000 `INSERT` statements in a transaction | 96.10 ms | 39.51 ms | 42.27 ms | 7% faster |
| one `INSERT` of 20,000 rows | 103 ms | 41.42 ms | 43.88 ms | 6% faster |
| `.import --csv` of 50,000 rows | 170 ms | 62.06 ms | 67.90 ms | 9% faster |
| a 10,000 row table and two indexes, from a script | 137 ms | 55.07 ms | 69.87 ms | 27% faster |
| Python, a 10,000 row table and two indexes | 110 ms | 44.66 ms | 45.58 ms | 2% faster |
| Python, 5,000 lookups by key | 75.15 ms | 54.84 ms | 86.16 ms | 57% faster |
| Python, read 10,000 rows | 13.43 ms | 5.82 ms | 8.00 ms | 38% faster |
| Python, 200 range queries of 200 rows | 36.74 ms | 22.61 ms | 36.80 ms | 63% faster |
| Python, 200 `GROUP BY` queries | 127 ms | 119 ms | 520 ms | 338% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 68.56 ms | 36.45 ms | 29.80 ms | 22% slower |
| Python, 100 single row inserts | 126 ms | 121 ms | 436 ms | 261% faster |
| Python, 1,000 single row updates | 1,344 ms | 1,250 ms | 4,882 ms | 290% faster |
| Python, 100 opens and closes | 99.17 ms | 7.58 ms | 7.25 ms | 5% slower |
| Node, a lookup by key with `query()` | 9.16 ms | 0.046 ms | 0.093 ms | 103% faster |
| Node, a lookup by key through a session | 0.108 ms | 0.007 ms | 0.014 ms | 101% faster |

What changed:

- **The C library is a Node addon, and the npm package runs `query`, `exec` and `batch` in the
  Node process.** A call started an `inillucent` program before. `query()` keeps a file open for
  the next read of the same file and closes it one second after the last one, so a lookup by key
  costs what the statement costs.
- **The Python driver gets results as Python objects and sends `execute_many`'s rows as Python
  objects.** The C library builds and reads them with CPython's own functions, whose addresses the
  driver hands over, so no JSON is written or parsed on the way.
- **A database is opened from the handle that created it.** Creating a file closed it and opened
  it again, and on Windows the second open waited for the virus scanner, which reads a file on its
  first open after a handle that wrote it closes. Every script and program that created a database
  paid about 6 ms for it.
- **An `INSERT` of many rows of literals, a prepared `INSERT` run many times, a run of single row
  inserts in the shell and `.import` each insert their rows as one statement**, with an exact
  fallback to one row at a time when any row fails.
- **An index entry for a new row searches only the rows added since the leaf was last packed, and
  the leaf hint covers the leaf's whole key range.** Of the 30,000 entries the Python insert makes,
  29,982 now find their leaf without a descent from the root, against 18,759 before.
- **An open reads the file's first pages once, opens the log segment once and reads the schema at
  the first statement**, which is when SQLite reads its schema too.
- **A fold under a rollback journal syncs the data file once, and a new file is written with one
  sync.**
- **The allocators keep blocks under 8 bytes and from 4 KiB to 64 KiB**, which went to the system
  heap one allocation and one free at a time.

What is still slower, and why:

- **Python inserts into a table with indexes**: 36.5 ms against 29.8 ms for 10,000 rows. The time
  is the engine's own work for each row and each index entry: making room in a full 32 KiB leaf, a
  log record for every row and every entry, and an undo record for each. No single part of it is
  more than a fifth.
- **A one row `exec`**: 24.4 ms against 22.1 ms. The commit syncs the log, and the fold when the
  program closes syncs the rollback journal and the data file: three syncs where SQLite makes two.
- **A query reading rows scattered over a table**: 19.9 ms against 18.2 ms. A cold read of 200 rows
  by an index reads about 65 pages of 32 KiB, where SQLite reads about 180 pages of 4 KiB, which is
  less than a third of the bytes.
- **The other command line workloads** are within 1% to 4% of SQLite, which is about the
  difference between two runs of the same build. Most of a call is starting the process, 15 ms for
  either program.

### A third round of the same comparison

A third round measured the same 25 workloads again. Measured on 6 October 2026, fifteen rounds,
the build at the end of the second round against the Windows release build of commit 90fa96c4,
with no other work running. Release 2.2.0 is that code: the commits after it change a generated
page and the version. The third round's build is the one a release ships: profile guided,
with the embedding feature, as `packaging/release-all.ps1` makes it.

| How it was called | After the second round, against SQLite | Now against SQLite |
|---|---|---|
| the command line, one process a command, 8 workloads | 1% slower | **within 1%** |
| a SQL script into the shell, 6 workloads | 34% faster | **39% faster** |
| the Python driver, 9 workloads | 80% faster | **105% faster** |
| the npm package, 2 workloads | 108% faster | **69% faster** |
| all 25 | 40% faster | **46% faster** |

19 of the 25 workloads are faster than SQLite, and all nine Python workloads are. Five of the six
that are not are command line reads within 1% to 3% of SQLite, and they move by about that much
between runs: a ten round run of the build one commit earlier had three of them faster. The npm
group's change is the scheduling of the Node process, not the code: two of the four samples of
`query()` ran at about 0.065 ms in each build that measured them, and three direct runs of this build
measured 0.071, 0.037 and 0.038 ms.

| Workload | After the second round | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---|
| `inillucent --version` | 15.30 ms | 15.11 ms | 15.48 ms | 2% faster |
| `query` of one row as JSON | 16.96 ms | 17.00 ms | 16.82 ms | 1% slower |
| `query` of one row as text | 17.29 ms | 17.08 ms | 16.64 ms | 3% slower |
| `query` of 100 rows by an index, sorted, as JSON | 19.24 ms | 19.18 ms | 18.60 ms | 3% slower |
| `query` of `count(*)` and `max` over 10,000 rows | 17.79 ms | 17.71 ms | 17.38 ms | 2% slower |
| `exec` of a one row `INSERT` | 23.36 ms | 23.63 ms | 22.25 ms | 6% slower |
| `tables` | 17.41 ms | 17.33 ms | 17.06 ms | 2% slower |
| `dump` | 30.93 ms | 29.38 ms | 33.24 ms | 13% faster |
| 1,000 lookups by key in a script | 32.35 ms | 30.91 ms | 37.95 ms | 23% faster |
| 200 single row inserts, each its own transaction | 268 ms | 267 ms | 871 ms | 226% faster |
| 10,000 `INSERT` statements in a transaction | 38.42 ms | 37.54 ms | 41.60 ms | 11% faster |
| one `INSERT` of 20,000 rows | 42.06 ms | 39.76 ms | 42.59 ms | 7% faster |
| `.import --csv` of 50,000 rows | 59.50 ms | 55.96 ms | 66.15 ms | 18% faster |
| a 10,000 row table and two indexes, from a script | 53.92 ms | 53.57 ms | 68.42 ms | 28% faster |
| Python, a 10,000 row table and two indexes | 46.17 ms | 36.87 ms | 48.87 ms | 33% faster |
| Python, 5,000 lookups by key | 55.85 ms | 52.25 ms | 84.91 ms | 63% faster |
| Python, read 10,000 rows | 6.14 ms | 5.75 ms | 7.74 ms | 35% faster |
| Python, 200 range queries of 200 rows | 22.20 ms | 20.24 ms | 35.20 ms | 74% faster |
| Python, 200 `GROUP BY` queries | 119 ms | 91 ms | 512 ms | 463% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 36.26 ms | 25.85 ms | 26.80 ms | 4% faster |
| Python, 100 single row inserts | 122 ms | 123 ms | 426 ms | 246% faster |
| Python, 1,000 single row updates | 1,226 ms | 1,238 ms | 6,985 ms | 464% faster |
| Python, 100 opens and closes | 7.45 ms | 6.76 ms | 7.37 ms | 9% faster |
| Node, a lookup by key with `query()` | 0.043 ms | 0.066 ms | 0.091 ms | 38% faster |
| Node, a lookup by key through a session | 0.007 ms | 0.007 ms | 0.014 ms | 107% faster |

What changed:

- **The Windows release is built with profile guided optimisation.** `packaging/release-all.ps1`
  builds the programs with counters, runs `packaging/pgo/train.mjs` against them, and builds them
  again with the counts. Against the same commit built without a profile, Python inserts of 10,000
  rows were 20% faster and 200 grouped aggregates 28% faster.
- **An append into a full leaf splits at once**, and packs the left page once.
- **A compaction leaves an eighth of the page free when the write adds a key**, so an insert into
  the middle of an index rewrites its leaf about half as often.
- **Python's `execute_many` moves its rows into the engine** instead of copying each row and each
  text.
- **A page read into a buffer pool frame used for the first time is read into the frame's
  reserved memory**, instead of into 32 KiB of zeros written first.
- **An open asks the file system fewer questions and takes no lock.** It tries the open before
  asking whether the file is there, takes "not found" for the rollback journal from the first query
  that answers it, and reads the meta record without the shared lock, which the first statement
  takes. On Windows the file's identity is read only when something asks for it.
- **The shell's `fsdir` and `zipfile` wait for the open**, so a command line verb that runs nothing
  does not run recovery or read the schema, and the catalog is refreshed once.

What is still slower, and why:

- **A cold query that reads pages from the command line**: one row as JSON or as text, 100 rows by
  an index, `count(*)`, `tables`. A sampled loop of the 100 row query found 38% of its time in the
  kernel's copy of 65 pages of 32 KiB into fresh memory and 20% in the CRC each page is checked
  with. SQLite reads about 180 pages of 4 KiB and checks none.
- **A one row `exec`**: the change is written twice, to the log at the commit and to the data file
  by the fold when the program closes. Both engines make three syncs: the log, the rollback
  journal and the data file here, and the journal twice and the data file in SQLite.

### A fourth round of the same comparison

A fourth round measured the same 25 workloads again. Measured on 6 October 2026, fifteen rounds,
release 2.2.0 against the Windows release build of commit de455577, both profile guided as
`packaging/release-all.ps1` makes them. Other programs used about 10% of the processor during the
run, so every number in this round is a little slower than in the third round, for all three
programs.

| How it was called | 2.2.0 against SQLite | Now against SQLite |
|---|---|---|
| the command line, one process a command, 8 workloads | 4% slower | **within 1%** |
| a SQL script into the shell, 6 workloads | 39% faster | **40% faster** |
| the Python driver, 9 workloads | 104% faster | **109% faster** |
| the npm package, 2 workloads | 138% faster | **171% faster** |
| all 25 | 48% faster | **53% faster** |

20 of the 25 workloads are faster than SQLite. 2.2.0 had 18 in the same run. The five that are not
are command line programs, each between 2% and 5% slower, and each runs one statement in a new
process.

| Workload | 2.2.0 | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---|
| `inillucent --version` | 17.78 ms | 16.03 ms | 15.46 ms | 4% slower |
| `query` of one row as JSON | 19.29 ms | 18.91 ms | 19.09 ms | 1% faster |
| `query` of one row as text | 18.76 ms | 19.01 ms | 18.09 ms | 5% slower |
| `query` of 100 rows by an index, sorted, as JSON | 20.52 ms | 20.66 ms | 19.83 ms | 4% slower |
| `query` of `count(*)` and `max` over 10,000 rows | 19.09 ms | 17.94 ms | 17.97 ms | within 1% |
| `exec` of a one row `INSERT` | 25.65 ms | 23.66 ms | 23.24 ms | 2% slower |
| `tables` | 19.33 ms | 19.85 ms | 19.25 ms | 3% slower |
| `dump` | 32.05 ms | 30.57 ms | 35.67 ms | 17% faster |
| 1,000 lookups by key in a script | 34.09 ms | 35.33 ms | 41.92 ms | 19% faster |
| 200 single row inserts, each its own transaction | 268 ms | 263 ms | 881 ms | 235% faster |
| 10,000 `INSERT` statements in a transaction | 38.21 ms | 37.83 ms | 43.16 ms | 14% faster |
| one `INSERT` of 20,000 rows | 43.77 ms | 43.09 ms | 44.09 ms | 2% faster |
| `.import --csv` of 50,000 rows | 58.54 ms | 57.76 ms | 68.22 ms | 18% faster |
| a 10,000 row table and two indexes, from a script | 52.63 ms | 51.71 ms | 71.63 ms | 39% faster |
| Python, a 10,000 row table and two indexes | 40.17 ms | 39.10 ms | 46.32 ms | 18% faster |
| Python, 5,000 lookups by key | 53.89 ms | 57.59 ms | 94.95 ms | 65% faster |
| Python, read 10,000 rows | 6.49 ms | 6.15 ms | 8.48 ms | 38% faster |
| Python, 200 range queries of 200 rows | 22.68 ms | 23.13 ms | 38.54 ms | 67% faster |
| Python, 200 `GROUP BY` queries | 94.93 ms | 94.59 ms | 581 ms | 514% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 28.20 ms | 26.69 ms | 29.89 ms | 12% faster |
| Python, 100 single row inserts | 121 ms | 123 ms | 438 ms | 257% faster |
| Python, 1,000 single row updates | 1,236 ms | 1,212 ms | 6,694 ms | 452% faster |
| Python, 100 opens and closes | 7.91 ms | 6.88 ms | 8.46 ms | 23% faster |
| Node, a lookup by key with `query()` | 0.048 ms | 0.037 ms | 0.117 ms | 215% faster |
| Node, a lookup by key through a session | 0.007 ms | 0.007 ms | 0.017 ms | 133% faster |

What changed:

- **The CRC that checks each page and each log record runs on the processor's CRC instructions.**
  On x86-64 it folds 64 bytes at a time with carryless multiplication, and on aarch64 it uses the
  CRC32 instructions. It is the same polynomial, so every file written before still reads. A
  32 KiB page took 3.93 us to check and now takes 0.64 us. The code checks that the processor has
  the instructions before it uses them, and falls back to the table otherwise.
- **The command line's `exec` makes its commit durable with the fold.** A one row `exec` wrote the
  change to the log and synced it, then folded it into the data file and synced again when the
  program closed. The program now holds the commit until the fold, and the fold's sync makes it
  durable. A one row `exec` makes two syncs instead of three. A crash campaign cuts the power at
  every write of such a commit and checks that the row is either there or absent.
- **A first statement reads no log after a clean fold.** The meta record now stores where the log
  ended when it was last folded. When the log file still has that length, the open replays nothing
  and does not open the log until a write needs it. The meta record grew from 120 to 128 bytes.
- **Inserts share a log record.** Rows inserted in one transaction go into one log record of up to
  64 KiB instead of one record each. Recovery decides for each page whether its rows are already
  applied before it applies any of them.

What is still slower, and why:

- **`inillucent --version`** opens no database, so its time is starting the process. The program
  is 10 MB and SQLite's shell is 4 MB.
- **A cold query from the command line**: one row as text, 100 rows by an index, `tables`. Each
  reads 32 KiB pages into fresh memory that the kernel has to zero and copy.
- **A one row `exec`** makes two syncs now, one fewer than SQLite, and is 2% slower. The rest of
  its time is starting the program and opening the file.

### A fifth round of the same comparison

A fifth round measured the same 25 workloads against release 2.2.0, the build at the end of the
fourth round and SQLite, all in the same runs. Measured on 6 October 2026, on the Windows release
build of this round, profile guided as `packaging/release-all.ps1` makes it. Another program used
between half a core and a whole core during these runs, so the absolute times moved from run to
run, SQLite's as much as ours. Every workload ran at least 15 rounds. Twelve workloads that moved in
a way the changes could not explain were run again, and the table gives each workload's last run:
nine in a second run of 15 rounds, and the command line `query` of one row as JSON, the 100 row
query, `exec` and every Python and npm workload in a third run of 30 rounds.

| How it was called | 2.2.0 against SQLite | Round 4 against SQLite | Now against SQLite |
|---|---|---|---|
| the command line, one process a command, 8 workloads | 2% faster | 2% faster | **7% faster** |
| a SQL script into the shell, 6 workloads | 48% faster | 48% faster | **50% faster** |
| the Python driver, 9 workloads | 114% faster | 133% faster | **129% faster** |
| the npm package, 2 workloads | 171% faster | 209% faster | **250% faster** |
| all 25 | 57% faster | 64% faster | **68% faster** |

All 25 workloads are faster than SQLite. In the same runs 2.2.0 was faster in 22 and the round 4
build in 21. The closest are the command line's 100 row query, under 1% faster, and its one row
query as JSON, 1% faster. A short command line call is about 16 ms of starting a process on this
machine, for both programs, so those two move by about their whole margin between runs.

| Workload | 2.2.0 | Round 4 | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---:|---|
| `inillucent --version` | 16.53 ms | 16.06 ms | 16.11 ms | 16.99 ms | 5% faster |
| `query` of one row as JSON | 19.02 ms | 19.18 ms | 18.80 ms | 19.07 ms | 1% faster |
| `query` of one row as text | 21.42 ms | 21.53 ms | 20.73 ms | 21.82 ms | 5% faster |
| `query` of 100 rows by an index, sorted, as JSON | 20.71 ms | 20.62 ms | 20.36 ms | 20.43 ms | within 1%, faster |
| `query` of `count(*)` and `max` over 10,000 rows | 18.06 ms | 17.91 ms | 17.39 ms | 18.61 ms | 7% faster |
| `exec` of a one row `INSERT` | 25.76 ms | 23.79 ms | 23.15 ms | 23.75 ms | 3% faster |
| `tables` | 20.92 ms | 21.12 ms | 18.69 ms | 19.88 ms | 6% faster |
| `dump` | 34.18 ms | 37.21 ms | 33.20 ms | 42.18 ms | 27% faster |
| 1,000 lookups by key in a script | 40.48 ms | 39.80 ms | 39.22 ms | 53.23 ms | 36% faster |
| 200 single row inserts, each its own transaction | 288 ms | 292 ms | 288 ms | 1,041 ms | 261% faster |
| 10,000 `INSERT` statements in a transaction | 43.98 ms | 44.78 ms | 43.94 ms | 51.75 ms | 18% faster |
| one `INSERT` of 20,000 rows | 41.08 ms | 40.99 ms | 41.20 ms | 44.99 ms | 9% faster |
| `.import --csv` of 50,000 rows | 62.93 ms | 60.32 ms | 60.75 ms | 76.19 ms | 25% faster |
| a 10,000 row table and two indexes, from a script | 52.52 ms | 53.55 ms | 51.65 ms | 73.77 ms | 43% faster |
| Python, a 10,000 row table and two indexes | 45.32 ms | 44.46 ms | 45.30 ms | 52.23 ms | 15% faster |
| Python, 5,000 lookups by key | 78.39 ms | 76.30 ms | 77.04 ms | 140 ms | 82% faster |
| Python, read 10,000 rows | 8.97 ms | 6.53 ms | 7.27 ms | 10.40 ms | 43% faster |
| Python, 200 range queries of 200 rows | 28.63 ms | 25.15 ms | 25.02 ms | 50.62 ms | 102% faster |
| Python, 200 `GROUP BY` queries | 96.19 ms | 94.78 ms | 108 ms | 750 ms | 597% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 30.18 ms | 28.87 ms | 29.13 ms | 37.85 ms | 30% faster |
| Python, 100 single row inserts | 130 ms | 130 ms | 138 ms | 539 ms | 289% faster |
| Python, 1,000 single row updates | 1,408 ms | 1,387 ms | 1,327 ms | 6,300 ms | 375% faster |
| Python, 100 opens and closes | 10.29 ms | 8.47 ms | 7.40 ms | 12.58 ms | 70% faster |
| Node, a lookup by key with `query()` | 0.063 ms | 0.062 ms | 0.045 ms | 0.193 ms | 326% faster |
| Node, a lookup by key through a session | 0.011 ms | 0.009 ms | 0.009 ms | 0.027 ms | 188% faster |

What changed:

- **A short command spends less of its time in page faults.** The command line and the shell carve
  every block of 4 KiB to 64 KiB, which includes every buffer pool frame, from 512 KiB chunks
  aligned to a page, where each frame used to be a separate Windows heap block. A frame now covers
  eight pages of 4 KiB instead of nine. Blocks of every smaller size are carved from one shared
  chunk instead of one chunk per size, so a query that allocates in a hundred sizes touches pages in
  proportion to the bytes it allocates. The 100 row query took 1,298 page faults more than a
  process that does nothing, and takes 1,160.
- **The rollback journal is written in one call.** Its records wait in memory until the journal is
  sealed, and the header and every record then go out in one write. A one row `exec` journals five
  pages and made eleven writes into a file each of them extended. No page reaches the database
  before the seal's sync, so a record held in memory until then protects the same thing. Twelve
  crash campaigns now have fewer cut points, because each of those writes was one.
- **A checkpoint writes the 128 byte meta record into each slot, not the slot's 32 KiB page.** The
  rest of a meta page is zero padding that every earlier write of it left.
- **The system's random numbers come from `ProcessPrng`**, which the standard library already
  loads for its hash seeds. The rollback journal takes a random value for each transaction, and
  taking it from `BCryptGenRandom` loaded `bcrypt.dll` into every `exec`. That load cost about
  0.7 ms.

What this round found and did not change:

- **The default page size.** Every remaining difference on a cold command line read is the bytes
  read: the 100 row query reads 65 pages of 32 KiB, 2.1 MB, where SQLite reads about 180 pages of
  4 KiB, 0.7 MB. Each page costs a first touch fault on eight 4 KiB pages of new memory. A smaller
  default page would change every workload and the file format, so it was left alone.
- **A link order file.** Placing the functions a short command runs together in the program took
  60 page faults off a one row query, which touches about 600 pages of the program's code. Most of those
  functions are large after inlining, and the profile guided build already moves the functions its
  training never ran into a section of their own.

### A sixth round of the same comparison

A sixth round set out to make every workload three times as fast as SQLite. Measured on 6 October
2026, fifteen rounds, release 2.2.0, the round 5 build of `main` and this round's build against
SQLite in the same runs, all three profile guided as `packaging/release-all.ps1` makes them, with
no other work running.

**No program can be three times as fast on the 14 workloads that start a process.**
Starting an empty program from Node on this machine takes 13 to 17 ms, and SQLite's command line
answers a one row query in 17 to 21 ms, so a command three times as fast would have to finish
before an empty program does. The benchmark now times an empty program in every round, and the
shell's figures below are also given above that floor, which is the part either engine controls.

**About 11 ms of that floor is the benchmark's own.** It starts every program with Node's
`windowsHide`, which gives each child a console of its own, hidden. A program started from a
terminal or a script shares its parent's console instead: measured that way, 40 starts each, an
empty program takes 5.7 ms, SQLite's one row query 7.95 ms and inillucent's 7.55 ms. The 14 process
workloads were run again that way, 15 rounds of all three builds. The command line group came out
3% slower than SQLite for 2.2.0, 5% faster for `main` and 4% faster now, which is the spread of one
run to the next. The shell group is 49% faster for `main` and 56% faster now, and 68% faster above
the empty program. Three times as fast would still need a one row query to finish in 2.5 ms, under
the 5.7 ms an empty program takes.

| How it was called | 2.2.0 against SQLite | `main` against SQLite | Now against SQLite |
|---|---|---|---|
| the command line, one process a command, 8 workloads | 1% faster | 4% faster | **3% faster** |
| a SQL script into the shell, 6 workloads | 38% faster | 41% faster | **46% faster** |
| the same, time above an empty program | 52% faster | 55% faster | **69% faster** |
| the Python driver, 9 workloads | 107% faster | 106% faster | **136% faster** |
| the npm package, 2 workloads | 114% faster | 126% faster | **207% faster** |
| all 25 | 50% faster | 52% faster | **64% faster** |

Six of the 25 are three times as fast as SQLite or more, where `main` had four: Python point
queries, the Node session, Python grouped aggregates, Python single row updates and inserts, and
the shell's 200 single row inserts.

| Workload | 2.2.0 | `main` | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---:|---|
| an empty program, the floor | 17.41 ms | 16.81 ms | 16.91 ms | 16.65 ms | |
| `inillucent --version` | 16.86 ms | 16.57 ms | 17.20 ms | 17.00 ms | within 2% |
| `query` of one row as JSON | 19.40 ms | 18.16 ms | 19.23 ms | 19.19 ms | within 1% |
| `query` of one row as text | 18.71 ms | 18.56 ms | 18.58 ms | 18.40 ms | within 1% |
| `query` of 100 rows by an index, sorted, as JSON | 20.48 ms | 19.22 ms | 19.50 ms | 19.33 ms | within 1% |
| `query` of `count(*)` and `max` over 10,000 rows | 19.23 ms | 18.52 ms | 18.93 ms | 18.82 ms | within 1% |
| `exec` of a one row `INSERT` | 24.59 ms | 23.65 ms | 22.79 ms | 24.26 ms | 6% faster |
| `tables` | 18.52 ms | 18.60 ms | 18.67 ms | 19.09 ms | 2% faster |
| `dump` | 31.98 ms | 31.07 ms | 30.74 ms | 36.22 ms | 18% faster |
| 1,000 lookups by key in a script | 34.09 ms | 33.09 ms | 26.99 ms | 42.89 ms | 59% faster, 160% above the floor |
| 200 single row inserts, each its own transaction | 272 ms | 272 ms | 270 ms | 868 ms | 222% faster |
| 10,000 `INSERT` statements in a transaction | 41.48 ms | 38.98 ms | 39.67 ms | 42.52 ms | 7% faster |
| one `INSERT` of 20,000 rows | 41.82 ms | 41.33 ms | 40.68 ms | 44.26 ms | 9% faster |
| `.import --csv` of 50,000 rows | 57.10 ms | 56.60 ms | 57.66 ms | 69.19 ms | 20% faster |
| a 10,000 row table and two indexes, from a script | 54.85 ms | 53.36 ms | 52.25 ms | 70.90 ms | 36% faster |
| Python, a 10,000 row table and two indexes | 36.43 ms | 39.06 ms | 37.35 ms | 46.66 ms | 25% faster |
| Python, 5,000 lookups by key | 52.44 ms | 53.65 ms | 21.50 ms | 96.16 ms | 347% faster |
| Python, read 10,000 rows | 5.90 ms | 5.90 ms | 5.88 ms | 7.88 ms | 34% faster |
| Python, 200 range queries of 200 rows | 20.86 ms | 21.24 ms | 18.86 ms | 41.72 ms | 121% faster |
| Python, 200 `GROUP BY` queries | 95.51 ms | 94.12 ms | 94.29 ms | 522 ms | 453% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 26.98 ms | 24.55 ms | 24.30 ms | 30.34 ms | 25% faster |
| Python, 100 single row inserts | 129 ms | 125 ms | 124 ms | 456 ms | 268% faster |
| Python, 1,000 single row updates | 1,265 ms | 1,241 ms | 1,243 ms | 6,284 ms | 405% faster |
| Python, 100 opens and closes | 6.85 ms | 7.70 ms | 6.82 ms | 7.17 ms | 5% faster |
| Node, a lookup by key with `query()` | 0.043 ms | 0.037 ms | 0.034 ms | 0.096 ms | 181% faster |
| Node, a lookup by key through a session | 0.007 ms | 0.007 ms | 0.004 ms | 0.014 ms | 237% faster |

What changed:

- **A Python execution is one foreign call.** `inillucent_py_stmt_execute` reads the parameters out
  of their Python objects, runs the statement with the interpreter lock let go, and returns the
  result built as Python objects, where an execution made four `ctypes` calls and a `json.dumps`.
  C ABI 1.4.0. A point query from Python went from 17.25 us to 6.25 us a call.
- **A statement that only read keeps its shared lock for the next one.** Every statement outside a
  transaction took SHARED, read the meta record to learn whether another process had folded, asked
  the log its length, and let SHARED go: four kernel calls, 6.4 us of a point query whose lookup is
  0.9 us. A statement that only read now keeps SHARED for up to 1 ms with nothing running and
  250 us in all, and the statement after it asks only the log's length, because nothing can fold
  while SHARED is held. A thread lets an idle lease go, and a connection in the same process that
  needs the file takes the lease away at once. The Python point query went to 3.3 us a call.
- **The shell writes its output in 64 KiB blocks when it is not writing to a terminal.** It wrote
  every row to the pipe on its own. It still writes what it holds before anything goes to standard
  error and before it waits for more input. 60,000 point queries took 340 ms and take 170 ms;
  SQLite's shell takes 1,559 ms.
- **An `ORDER BY` encodes its sort keys into one buffer**, and a descent through an interior page
  guesses the child in 64 bits, where it used a 128 bit division.

What is still under three times SQLite, and why:

- **The 14 workloads that start a process**, for the reason above.
- **Inserting rows.** 10,000 rows from Python into a table with no index take 14.2 ms against
  SQLite's 12.2 ms, so the gap is in the write of a row, not in its indexes. In a profile of the
  same insert into the table with two indexes, the tree's write of a row is 61% of the time, a
  third of that is rewriting leaves whose delta area has filled, and the library's allocator is
  another 10%.
- **Reading many rows into Python.** The engine copies every cell of a result into its own
  allocation before the C library builds the Python value, and about 30% of a 10,000 row scan is
  the Windows heap serving and freeing those copies. Building the Python values from the driver's
  rows or from the engine's own made no difference, so the copy has to go, which means handing rows
  to the caller as the plan produces them.
- **A Python open and close.** It is one file open, a read of both 32 KiB meta pages, a check by
  name for a rollback journal and a close: 68 us against SQLite's 72 us in the table above. A bare
  open and close of the same file from Python costs 11.5 us, so three times SQLite's speed would
  leave about 12 us for everything else.

`tasks/task-2197-performance-hillclimb-5.md` has every change, the measurements behind each, and
the changes that were tried and not kept.

### `read.join`'s bar under the per round statistic

**The bar stays at 3.00x.** The family's lower bound on five builds back to `57e87b0`, pinned, two
passes each except `main`:

| build | full plan, per round | full plan, pooled | read gate plan, per round | read gate plan, pooled | `join.selective` and `join.range`, full plan |
|---|---|---|---|---|---|
| `57e87b0`, the first build graded | no full gate | no full gate | 4.69x, 4.52x | 3.23x, 3.18x | no full gate |
| `b0ba286` | **3.23x, 3.18x** | 2.42x, 2.38x | 4.42x, 4.35x | 3.11x, 3.07x | 11.15x and 0.97x, 11.03x and 0.95x |
| `a8f45b1` | 4.17x, 4.07x | 2.78x, 2.76x | 4.39x, 4.37x | 2.91x, 2.96x | 20.89x and 0.85x, 20.32x and 0.85x |
| `f9e2374` | 4.33x, 4.21x | 2.92x, 2.82x | 4.56x, 4.20x | 3.04x, 2.97x | 21.27x and 0.90x, 20.62x and 0.87x |
| `main` at `16c01a4` | 4.27x, 4.08x, 4.18x, 4.19x | 2.90x, 2.75x, 2.78x, 2.81x | 4.19x, 4.06x | 2.86x, 2.81x | 20.36x to 21.68x and 0.86x to 0.88x |

Every build since the family was first graded meets 3.00x on both plans under the per round
statistic. Under the pooled statistic no build ever met it on the full plan. The reasons to keep
3.00x:

1. **3.00x is the number the contract meant, on the statistic it meant.** It is the "low estimate"
   column of `tasks/task-1816-rearchitecture-tdd.md`, where a family's ratio is the geometric mean
   over its workloads. The per round statistic grades exactly that. Nine of the ten family bars come
   from that column. The tenth is `read.analytical`, at 5.00x against a low estimate of 8x.
2. **Moving the bar now would choose it from the measurement.** `main` reads 4.08x at its lowest, and
   a bar of 4.00x would be that reading rounded down. `compat/perf/contract.toml` says why a
   threshold that moves toward the measurement is not a threshold.
3. **Builds that existed met 3.00x by a small margin.** `b0ba286` met it on the full plan at 3.18x
   and 3.23x, with `join.selective` at 11x. With today's `join.range` of 0.86x, the family drops under
   3.00x if `join.selective` falls from about 21x to about 10.5x, or if `join.range` falls to about
   0.43x. Either is a loss of half a workload's speed.
4. **The bar does not catch the 26% of its time `join.range` lost since `b0ba286`.** That loss moved
   the family 8% on the read gate's plan (4.71x at `57e87b0` to 4.32x on `main`, the mean of two
   passes each).

`join.range` is still slower than SQLite, at 0.84x to 0.88x. The design document expected the hash
join to fix its 0.015x. It went from 0.015x to 0.86x.

## Earlier and supporting measurements

The sections below are measurements behind the figures above, and measurements of single changes.
Each gives its own date and method. None of them replaces the headline.

### Measured again at `52c4b5f` on 2026-09-24, and not graded

Four full gate passes of `main` at `52c4b5f`, pinned to the performance cores, 30 rounds each. **The
gate refused to grade all four.** SQLite's side ran 6.71% to 7.42% slower than the machine's recorded
idle reference, against a limit of 3%. Firefox and WebView used about 1.2 cores throughout. A busy
machine slows SQLite's side more than inillucent's, so every ratio from these passes is too high.
They read 5.31x to 5.47x weighted. That figure is not a headline and is not compared with any
graded figure.

Against the graded 23 September run, SQLite's own times in these passes are 2% to 5% slower on most
workloads and inillucent's are 2% to 8% faster. Some workloads moved far more than a 7% bias:

| workload | graded 2026-09-23 | at `52c4b5f`, not graded | SQLite at `52c4b5f` |
|---|---|---|---|
| `correlated.exists`, 400 outer rows | 59.69 ms | **0.40 ms** | 0.30 ms |
| `correlated.in`, 400 outer rows | 118.19 ms | **0.92 ms** | 0.10 ms |
| `correlated.exists.selective` | 2.11 ms | **19.5 µs** | 23.7 µs |
| `correlated.scalar.selective` | 2.11 ms | **17.9 µs** | 22.3 µs |
| `join.range`, a probe | 57.1 µs | **53.7 µs** | 51.2 µs |
| `range.lookaside`, a probe | 57.4 µs | **52.4 µs** | 57.8 µs |
| processor time, one round of the plan | 555 ms | **367 ms** | 1,102 ms |
| peak resident set, one round of the plan | 40.76 MiB | 40.88 MiB | 37.22 MiB |

- **The correlated subqueries are 99% cheaper.** Each execution used to grow a slot array to 100,001
  entries (3.2 MB), and `correlated.exists` made and freed 58 of them each time it ran: 45,414 page
  faults an execution, and 91,390 for `correlated.in`. Those faults are now 0. The two selective
  forms are faster than SQLite. `correlated.exists` is 35% slower and `correlated.in` 809% slower,
  where they were 21,332% and 118,020% slower.
- **Processor time follows.** The four correlated workloads took 182 ms of each round on 23 September
  and take under 2 ms now. One round of the plan is 367 ms against SQLite's 1,102 ms, a ratio of
  0.335, under the contract's 0.400 bar. The pass was not graded. The graded run of 26 September
  2026 met the bar at 0.33.
- **`join.range` is 6% cheaper a probe**, because a leaf column is read once and a probe key whose
  affinity changes nothing is no longer copied. It reads 0.95x, inside the busy machine's bias.
- **`range.lookaside` reads 1.10x**, faster than SQLite, but also inside the bias. The graded run of
  26 September 2026 read it at 1.11x.
- **Memory did not move.** A busy machine does not bias the memory figure.

The workloads that depend on the disk moved between the four passes by more than any engine change.
`txn.autocommit` read 1.88x, 1.91x, 0.99x and 1.00x, and `extension.fts.build` 0.63x, 0.69x, 0.99x
and 0.99x, mostly because SQLite's side changed. Nothing is concluded from them.

### Runs of one family alone, 2026-09-15 and 2026-09-20

A family measured on its own is a different measurement from the same family inside the whole plan.
The plan's other workloads decide what is in the page pool when the family runs. The contract grades
the whole plan. The roadmap quotes these runs.

| run | `extension` | 95% lower bounds | `extension.fts.build` | `extension.fts.query` |
|---|---|---|---|---|
| `extension` alone, 2026-09-15, four runs | 1.58x, 1.60x, 1.59x, 1.67x | 1.40x, 1.39x, 1.39x, 1.45x | 0.56x to 0.58x | 1.70x to 1.85x. 1.77x run on its own, against 1.43x with the reverted segment format |
| whole plan, 2026-09-20, four runs | 1.57x | 1.17x, 1.33x, 1.38x, 1.45x | 0.69x | |
| whole plan, 2026-09-23, four runs | 1.73x | 1.56x, 1.48x, 1.54x, 1.57x | 0.95x | 1.44x |

| run | `read.join` | 95% lower bounds | `join.selective` | `join.range` |
|---|---|---|---|---|
| `read.join` alone (`--families read.join`), 2026-09-15, four runs | 6.46x, 6.26x, 6.14x, 5.60x | 4.11x, 4.00x, 4.02x, 3.67x | 35.49x | 1.17x |
| `read.join` alone, before the chain reuse change | | 2.97x, 3.00x, 3.00x, 2.99x | | |
| whole plan, 2026-09-23, four runs | 4.21x | 2.73x, 2.83x, 2.78x, 2.90x (pooled) | 20.94x | 0.84x |

### Why `read.join` misses its 3.00x bar, and when it last met it

This section explains the pooled bound's misses. Under the per round statistic the family meets its
bar ([`read.join`'s bar under the per round
statistic](#readjoins-bar-under-the-per-round-statistic)).

**Method.** 60 passes on 2026-09-23, `inillucent-readgate` and `inillucent-fullgate`, medium
fixture, 30 rounds, every pass pinned from outside to `0xC03C03`, the mask read back from the SQLite
child as `0xC03C03` all 60 times. Builds from before the gates pinned themselves cannot pin
themselves, so HEAD was pinned the same way. Passes 1 to 3 ran at 6% to 9% machine load from another
test. The rest ran on an idle machine.

**The 3.00x bar was never measured.** It is the "low estimate" column of the performance contract in
`tasks/task-1816-rearchitecture-tdd.md`, written when `read.join` read 0.154x.
`inillucent-readgate` first graded it at `57e87b0`, which read the family at 4.87x with a lower bound
of 3.32x.

**The pooled bound mostly measures the distance between the two workloads.** On HEAD each workload
is measured to within 2%: `join.selective` 20.30x to 22.14x and `join.range` 0.85x to 0.89x. The
pooled family interval beside them is 2.76x to 6.67x. For the pooled bound to reach 3.00x,
`0.3735 * ln(join.selective) + 0.6265 * ln(join.range)` has to reach `ln 3`. On two pinned passes
of HEAD the per round statistic read 4.15x [4.11x, 4.21x] and 4.17x [4.16x, 4.25x].

**`join.range` lost 26% of inillucent's time, in four steps.** On the read gate's plan, pinned,
inillucent in milliseconds, forward sweep / reverse sweep, with SQLite at 23.9 to 24.6 ms on every
pass:

| build | `join.range` | step |
|---|---|---|
| `57e87b0` | 22.21, 23.00, 22.39, 22.00 | the build the bar was first met on |
| `b0ba286` | 22.37 / 22.62 | |
| `ea03335` | 23.44 / 23.36 | **+1.0 ms** in the 15 commits before it |
| `3322436`, `1334b80`, `59ccf91` | 23.27 to 23.72 | |
| `71d014a` | 24.12 / 24.15 | **+0.8 ms** in the 30 commits before it |
| `566c688`, the narrow integer slot change | 24.23 / 24.01 | |
| `a8f45b1`, five follow on changes | 27.03 / 26.77 | **+2.8 ms in this one commit** |
| `f9e2374` | 25.86 / 26.05 | 1.0 ms back |
| `b3ad244` to `a07036b` | 26.07 to 26.53 | |
| `dcc65f2` | 27.37 / 27.52 | **+1.0 ms** in the 25 commits before it |
| `36939dd`, `5353eb4` | 27.03 to 27.59 | |
| `d389021` | 27.96 / 27.88 | **+0.9 ms** in the 25 first parent commits before it (41 with the merged branches) |
| HEAD | 27.88 / 28.00 | |

The two sweeps agree at every build to within 0.4 ms. The full gate's plan shows the same drift:
25.61 and 25.41 ms at `b0ba286`, 27.55 and 27.38 at `f9e2374`, and 28.08 to 28.43 on HEAD.

On the read gate's plan the pooled bound last met 3.00x at `57e87b0` and `b0ba286`, where
`join.range` read 1.04x to 1.10x and the pooled bound 3.03x to 3.28x. On the full plan no build met it
pinned. `b0ba286` read 2.20x and 2.38x, because `join.selective` was 11x then.

#### The +2.8 ms at `a8f45b1`

`a8f45b1` did five things. Three have a switch: the constant `FRAME_OF_REFERENCE`, the `(u16, u16)`
heap pair `heap_slot_width`, and the release profile's `panic = "abort"` with `strip = true`
(`CARGO_PROFILE_RELEASE_PANIC` and `CARGO_PROFILE_RELEASE_STRIP` override them). 39 passes of
`inillucent-readgate` in two quiet windows on 2026-09-23 and 24, medium fixture, 30 rounds, pinned
to `0xC03C03` from outside, the SQLite child's mask read back each time. SQLite read 23.7 to 24.6 ms
on every pass after the machine settled. inillucent's `join.range` in milliseconds, in the order run:

| build | window 1 | window 2 |
|---|---|---|
| `a8f45b1` | 27.35, 26.52, 26.67 | 27.04, 26.66, 26.63 |
| `a8f45b1`, frame of reference off | 27.27, 26.59, 26.63 | |
| `a8f45b1`, `(u16, u16)` heap pair off | 26.93, 26.69, 26.71 | |
| `a8f45b1`, `panic = "unwind"`, `strip = false` | 25.64, 25.58, 25.45 | 25.58, 25.80, 26.51 |
| `a8f45b1`, `panic = "unwind"`, `strip` kept | | 26.36, 25.58, 25.56 |
| `a8f45b1`, all three off | | 25.32, 25.53, 25.34 |
| `566c688` | 24.28, 23.97, 24.07 | 24.34, 24.24, 24.15 |
| `566c688`, `panic = "abort"`, `strip = true` | | 25.34, 25.41, 25.25 |
| HEAD (`40ca955`) | | 28.07, 28.18, 28.27 |
| HEAD, `panic = "unwind"` | | 27.91, 28.73, 28.70 |

- **The two file format changes cost nothing.** Frame of reference off moved `join.range` by -0.07 to
  +0.08 ms and the narrow heap pair off by -0.17 to +0.42 ms, against a 0.5 ms threshold. They took
  the imported file from 573 pages to 532, and they are kept.
- **`panic = "abort"` cost about 1.1 ms at `a8f45b1` and costs nothing at HEAD.** Added to `566c688`
  alone it costs 1.00, 1.17 and 1.10 ms. `strip` has no measurable effect. At HEAD, `panic =
  "unwind"` reads +0.16, -0.55 and -0.43 ms against `panic = "abort"`, under the 0.4 ms threshold, so
  `panic = "abort"` stays for the smaller binary.
- **The read code in follow on changes 4 and 5 cost about 1.2 ms.** `a8f45b1` with all three switched
  off writes the same 573 pages as `566c688` and is still 0.98, 1.29 and 1.19 ms slower. The code
  added a `base == 0` test in every integer read, a width match in every heap slot read, a `Vector`
  eight bytes wider, and a fourth lookup of the column directory entry on every `LeafRef::column`
  call.

**The column read fix.** `LeafRef::column` runs on every probe, and it located and bounds checked the
same directory entry four times. It now reads the entry once. HEAD (`40ca955`) against HEAD with that
change, alternated, pinned, every workload agreeing with SQLite on all eight passes:

| | HEAD | HEAD, entry read once |
|---|---|---|
| `join.range`, ms | 28.43, 28.35, 28.53, 28.14 | 26.18, 26.16, 25.98, 26.11 |
| `range.lookaside`, ms (passes 204 and 206) | 28.33, 0.94x | 24.52, 1.11x |
| `point.index` (the same passes) | 16.14x | 18.53x |
| `join.selective` (the same passes) | 20.52x | 23.11x |
| point probe, warm (the same passes) | 344.5 ns | 306.9 ns |
| `read.join` family (the same passes) | 4.19x [4.13x, 4.25x] | 4.67x [4.61x, 4.73x] |

`join.range` is 2.04 to 2.55 ms faster in every adjacent pair, against a 0.5 ms threshold. It reads
0.92x after the fix. One `join.selective` and one `join.range` make 512 `column` calls at every build
from `566c688` to HEAD, so the number of calls did not change. The cost of each call did.

#### Where the rest of `join.range`'s time went after `57e87b0`

The whole history was measured again with the column read fix applied at every build. **Method:**
`inillucent-readgate`, medium fixture, 30 rounds, one pass per build per sweep, pinned from outside to
`0xC03C03` with the SQLite child's mask read back on every pass. 83 builds in two windows on
2026-09-24: 108 passes from 03:12 to 03:54Z and 101 from 06:06 to 06:44Z, each build once forward and
once reversed. A pass counts only if SQLite read 26.5 ms or less in it. That rule refused 28 of 108
passes in window 1 and 18 of 101 in window 2. The machine read about 3% slower than on 2026-09-23, so
these figures compare with each other and not with the tables above.

| from | to | step | where |
|---|---|---|---|
| `57e87b0` 22.73 | `b0ba286` 22.71 | 0 | |
| `b0ba286` 22.71 | `ea03335` 23.76 | **+1.05** | spread over `34e026e` and `9d3d84d` |
| `59ccf91` 23.88 | `71d014a` 24.44 | **+0.56** | between `c401bb2` and `71d014a` |
| `566c688` 24.06 | `a8f45b1` 26.21 | **+2.15** | `a8f45b1` itself (window 2, reverse sweep) |
| `a8f45b1` | `a07036b` 26.58 | about 0 | |
| `a07036b` 26.58 | `dcc65f2` 26.26 | -0.32 | |
| `5353eb4` 26.43 | `d389021` 27.30 | **+0.87** | `6f84ce6` |
| `d389021` 27.30 | HEAD 27.17 | -0.13 | |

HEAD (`01f37bb`) read 27.17 ms against 22.73 ms for `57e87b0`, both with the fix.

- **`dcc65f2`**: without the fix `a07036b` read 26.95 ms and `dcc65f2` 28.37 (+1.42). With it, 26.58
  and 26.26. The fix saves 0.37 ms a round at `a07036b` and 2.11 ms at `dcc65f2`: at least 1.4 ns a
  call before and at least 8 ns after, over 500 runs of at most 512 calls. The call count is 512 per
  pair at `566c688`, `59ccf91`, `71d014a`, `a07036b`, `dcc65f2`, `5353eb4`, `d389021` and HEAD. The
  same code became more expensive to call, which fits a change in how `column` is compiled and
  inlined. The fix removes it.
- **`6f84ce6`** is the change where an index seek converts its key the way SQLite does. It fixed four
  wrong answers. Of the 21 builds between `5353eb4` and `d389021`, the one step over 0.5 ms in both
  sweeps is `6833582` to `6f84ce6`: 26.38 to 27.07 ms reversed, and 27.37 to 28.81 forward, or 1.013
  to 1.072 as a ratio to SQLite's time. That is about 0.7 to 1.4 ms a round, 7 to 14 ns a probe over
  100,500 probes.
- **`a8f45b1`** is still about 2.2 ms with the fix at both ends: 24.06 to 26.21 ms reversed in window
  2, and 0.940 to 1.043 as a ratio to SQLite over both sweeps. The `566c688..a07036b` walk, a build
  every 19 commits, found no other step over 0.5 ms. Its builds read from about 0.8 ms below
  `a8f45b1` (`03fdb42`, `4e8a78f`) to 0.9 ms above it (`a07036b` reversed).
- **`b0ba286` to `ea03335`** is +1.05 ms with the fix and +1.29 without. Over four passes each,
  `356ef19` reads 22.99 to 23.14, `34e026e` 23.00 to 23.61, `9d3d84d` 23.64 to 24.16 and `d5ea139`
  23.61 to 24.34. About 0.7 ms is spread over `34e026e` (a shadow read copies the row once) and
  `9d3d84d` (a wide value is spilled).
- **`59ccf91` to `71d014a`** is +0.56 ms with the fix. Window 2 put `c401bb2` to `71d014a` at +0.35
  and +0.54. `71d014a`'s descending direction check costs nothing: 24.96 and 24.95 ms without it
  against 24.93 and 24.60 with it. The three commits between (`6e19c0b`, `81855a7`, `0f24df5`) do not
  compile, so the step cannot be placed more finely. `81855a7` and `0f24df5` change
  `inillucent-pool`'s `pool.rs` for the rollback journal and file locking, which is on every page
  fetch.

Of HEAD's 4.4 ms over `57e87b0`: about 2.2 ms is `a8f45b1`, about 0.9 ms is `6f84ce6`, about 1.05 ms
is `34e026e` and `9d3d84d`, and about 0.5 ms is `c401bb2..71d014a`.

#### Two of those steps, measured at `52c4b5f`

`6f84ce6` passes a nested loop's probe key through `ApplyAffinity` once per probe. `ApplyAffinity`
copied the key into an owned `Value` and back even when nothing changed. It now returns an unchanged
value as it is. Four builds of the read gate, medium fixture, 30 rounds, pinned to `0xC03C03`, run in
the order v0 v1 v2 v3 v3 v2 v1 v0 v0 v1 v2 v3. **The machine was busy**: Firefox and WebView held about
2.2 cores, and SQLite's speed index read 9.3% to 11.2%, so the gate graded none of the passes. The
builds are compared with each other only.

| build | `join.range`, ms a round | mean |
|---|---|---|
| v0: HEAD's probe | 26.94, 26.88, 27.16 | 26.99 |
| v1: `ApplyAffinity` returns an unchanged value | 26.69, 26.89, 26.71 | 26.76 |
| v2: v1 with the frame of reference and the four byte heap pair both off | 26.16, 25.92, 26.15 | 26.08 |
| v3: v2 without the `base == 0` test and the width match | 26.52, 26.43, 26.87 | 26.60 |

- The affinity change saves about 0.23 ms a round, 2.3 ns a probe. The passes of v0 and v1 do not
  overlap.
- The two read branches cost nothing measurable: v3 is no faster than v2.
- Turning the two file formats off saved about 0.7 ms here, where the full history walk found
  nothing. That would trade file size for speed, and it is not done. The quiet repeat is in the next
  section, and it found 0.4 ms.

#### The two formats and the older steps, measured again at `fc92827`

Four builds of the read gate at `fc92827`, medium fixture, 30 rounds, pinned to `0xC03C03` from
outside, the SQLite child's mask read back as `0xC03C03` on every pass. The passes ran in four quiet
windows on 2026-09-25, with the order reversed on every cycle. The gate graded every pass counted
here as quiet: SQLite's speed index read between -21.8% and -23.7%. The first five passes of the
third window ran while the machine settled after another run stopped, read -14.8% to -21.1%, and are
left out. Every pass agreed with SQLite.

| build | `join.range`, ms a round, windows 1 and 2 | windows 3 and 4 | mean |
|---|---|---|---|
| `fc92827` | 26.02, 25.62, 26.19, 25.65, 25.63, 25.91, 26.00, 26.37, 25.83, 25.76 | 25.96, 25.81, 25.83, 25.65, 25.77, 25.70 | 25.90, 25.79 |
| the frame of reference and the four byte heap pair both off | 25.65, 25.54, 25.28, 25.17, 25.53, 25.15, 25.60, 25.81, 25.50, 25.61 | | 25.48 |
| `LeafRef::has_extents` always false | 25.30, 25.28, 24.98, 25.03, 24.93, 25.05, 25.09, 25.37, 25.13, 25.09 | 25.37, 25.15, 25.20, 24.89, 24.93, 24.83 | 25.13, 25.06 |
| out of line values attached only to a leaf that has some | | 24.87, 24.56, 24.95, 24.86, 24.77, 24.59, 24.54 | 24.73 |

- **The two file formats cost about 0.4 ms a round on a quiet machine**, 1.6% of `join.range`,
  where the busy machine above said 0.7 ms. The slowest pass with the formats off (25.81) is slower
  than the fastest pass at `fc92827` (25.62), so the two ranges touch. The formats make the imported
  file 7% smaller, 532 pages against 573. They stay on. Turning them off changes the file format,
  and that is a decision for a person, not for a measurement of one workload.
- **The out of line value plumbing cost about 1.1 ms a round, and it is removed.** It is the likely
  cause of the `b0ba286` to `ea03335` step, which was the same size. `9d3d84d` made every walk call
  `read_extents` and `with_extents` on every leaf it opened. For a
  leaf with no out of line values that built an empty `Extents`, attached it and dropped it. None of
  the medium fixture's leaves has one: a build that counted found the flag set 0 times in about 2.1
  million checks over a three round run of the whole gate. `join.range` opens a leaf for every probe
  of `side_table`. `PagedTree::with_leaf_extents` now tests the flag first and
  attaches nothing to a leaf without the flag. It is 1.06 ms faster than `fc92827` over the same
  windows, and no pass of the two builds overlaps. It is also faster than the build with the flag
  test removed, so the flag test itself costs nothing measurable.
- **`34e026e`'s change is no longer on this path.** The copy it removed is in
  `crates/inillucent-engine/src/vtab/shadow.rs`, which only a virtual table module reading its own
  shadow tree runs. `join.range` reads two ordinary tables.
- **Nothing `81855a7` and `0f24df5` added runs in the read gate's timed loop.** The rollback journal
  is written only when a page is written back or checkpointed. The file lock is taken and released in
  `ImportedDatabase::enter` and `leave`, around each statement run through a connection, and the read
  gate calls the prepared `Statement::run` directly, so it takes no lock. Whatever the
  `c401bb2..71d014a` step was, it is not those commits' code running at HEAD. The step cannot be
  placed more finely, because the commits between do not compile.

### Where `write.insert.batch`'s time and log volume go

Measured 2026-09-20 with `inillucent-writelogattrib` on the medium fixture at a 32 KiB page: 2,000
inserts into `main_table`, which has two secondary indexes, in one transaction. The log is read back
from disk with the decoder recovery uses, so the byte counts are exact. The same run with the two
indexes dropped gives what they cost.

| | with both indexes | without either | the two indexes |
|---|---:|---:|---:|
| wall | 50.43 ms | 15.47 ms | **34.96 ms, 69%** |
| applying the changes to pages | 46.37 ms | 11.77 ms | 34.60 ms |
| log written | 1,563.9 KiB | 1,163.4 KiB | 400.5 KiB |
| leaf compactions | 181 | 58 | 123 |
| splits | 9 | 8 | 1 |

| record kind | records | bytes | share of the log |
|---|---:|---:|---:|
| `Structural` (a split) | 9 | 864.7 KiB | **55%** |
| `InsertRow` | 6,000 | 687.5 KiB | 44% |
| `CompactLeaf` | 181 | 11.3 KiB | 0.7% |
| `AllocPage`, `Commit` | 10 | 0.4 KiB | 0.03% |

The indexes are where the time is. The split records were where the log bytes were. A split record
held the left page, the right page and the parent in full: 98,384 bytes at a 32 KiB page.

A split is now logged as a `SplitLeaf` record: the rows that moved, the key the parent gained, and
how many rows the leaf kept. `docs/relational-architecture.md` says how recovery rebuilds the three
pages from it. The same workload, measured on 2026-10-04 with both builds on the same fixture:

| | before | after |
|---|---:|---:|
| log written, with both indexes | 1,553.9 KiB | 734.4 KiB, **53% less** |
| bytes for the 9 splits | 864.7 KiB, 98,384 a split | 45.2 KiB, 5,148 a split |
| log written, without either index | 1,160.4 KiB | 400.9 KiB, **65% less** |
| bytes for the 8 splits | 768.6 KiB | 9.0 KiB, 1,157 a split |

The splits without the indexes are rows arriving in key order, so the leaf keeps 95% of its rows and
the record carries the other 5%. The wall time of this one transaction did not move: 24.50 ms
against 24.25 ms with the indexes, because the log is written and synced once at the commit.

Where many transactions each write their log, the smaller record is measurable. Three alternating
pairs of `inillucent-fullgate --plan hillclimb --api connection --rounds 10` on 2026-10-04, with no
other work loading the processor, scored with `tools/perf-hillclimb/score.mjs`:

| workload | change |
|---|---:|
| `ai.build`, 20,000 inserts committed every 500 into a table with two indexes | 11.7% faster |
| `ai.append` | 14.9% faster |
| `app.insert.prepare_each` | 11.8% faster |
| `churn.update.grow` | 52.9% faster |
| the 28 tuned workloads, geometric mean | 1.4% faster |
| the 20 held out workloads, geometric mean | 2.9% faster |
| processor time of one round | 3.8% lower |

`WriteStats::room_nanos` times `make_room`, which compacts or splits a leaf:

| | with both indexes | without either | the two indexes |
|---|---:|---:|---:|
| wall | 44.17 ms | 15.68 ms | 28.49 ms |
| **making room** | **23.97 ms** | 5.10 ms | **18.87 ms** |

Making room was 54% of the transaction then. It later fell to 36%, 10.57 ms of 29.60. Its four
passes, medians of five runs, same fixture:

| | with both indexes | without either |
|---|---:|---:|
| making room | 10.57 ms | 4.12 ms |
| building the image | 7.77 | 1.59 |
| the merge, which rows are live | 1.98 | 0.50 |
| reading every live row | 1.90 | 0.33 |
| the sizing pass | 1.16 | 0.12 |
| the encode | 2.22 | 0.35 |

The merge works per delta row, so it does not grow with the page size. The other three passes roughly
double between an 8 KiB page and a 32 KiB page, because a 32 KiB leaf holds four times as many rows.
The gate runs at 32 KiB.

The sizing pass went from 4.00 ms to 1.16 ms, and the transaction from 33.91 ms to 29.60 ms, when
`fit_all_widths` replaced a row by row check with one pass. A compaction only needs to know whether
all live rows fit one page. The price of a run of rows never falls as rows are added, so a leaf that
fits whole had every prefix fit, and the two checks cannot disagree.

At the gate, four runs alternating that build with a control:

| | control | with the one pass sizing |
|---|---|---|
| `write.insert.batch`, inillucent's time | 38.34 ms, 36.95 ms | **33.14 ms, 32.93 ms** |
| the same workload's ratio | 0.46x, 0.58x | **0.56x, 0.63x** |
| the `write` family | 1.54x, 1.74x | **1.67x, 1.80x** |

The machine was busy for those runs. SQLite's time drifted from 18.49 ms to 21.33 ms while
inillucent's varied by 3.8% in the control and 0.6% with the change. On inillucent's own time the
change is 12.2% faster, 37.65 ms to 33.04 ms as medians, which matches `inillucent-writelogattrib`.

`LeafRef::locate` walks each leaf's unsorted delta area on every insert. Counted directly at an 8 KiB
page: 8,329 calls, 119,645 entries walked, 5.1 ms, 14.4 entries a call, against 66.8 ms of apply time.
That is under 8%.

### The cost of each secondary index

`main_table` has two secondary indexes, so `write.insert.batch` measures one index count.
`inillucent-writeprofile --sweep` inserts 5,000 rows in one transaction into a 100,000 row table with
0, 2, 5 and 10 indexes, in one process, with the write path's counters.
`inillucent-perfhistory --only insert.indexes` inserts 20,000 rows into a 20,000 row table with the
same index counts, beside SQLite. The indexes are built after the rows are loaded on both engines.

One quiet window, the fastest of five interleaved rounds, microseconds a row:

| indexes | before | delta area sized by free space | and the compaction splice |
|---:|---:|---:|---:|
| 0 | 7.93 | 5.04 | 5.14 |
| 2 | 18.28 | 10.47 | 9.12 |
| 5 | 33.45 | 17.16 | 15.50 |
| 10 | 73.06 | 39.97 | 37.20 |
| cost per index at 2 | 5.17 | 2.71 | 1.99 |
| leaf compactions at 10 indexes | 1,624 | 423 | 423, 391 of them spliced |
| time making room at 10 indexes | 194.03 ms | 66.18 ms | 49.49 ms |

The ratio against SQLite, net of process startup, from `tests/performance-history.tsv`:

| indexes | before | delta area sized by free space | and the splice |
|---:|---:|---:|---:|
| 0 | 0.09x | 0.14x | 0.14x |
| 2 | 0.08x | 0.18x | 0.19x |
| 5 | 0.08x | 0.25x | 0.19x |
| 10 | 0.43x | 1.20x | 1.28x |

Measured again on 2026-09-23 with both engines on the performance cores: 5.09, 9.15, 15.33 and 36.37
µs a row at 0, 2, 5 and 10 indexes, with the same 423 compactions and 391 splices at 10 indexes. The
ratios for that run read 0.15x, 0.18x, 0.22x and 1.13x.

The delta area change is most of the gain. The delta area used to compact every 32 rows, and an index
leaf holds thousands of entries. With a directory in key order, a lookup in the delta area is a
binary search, the area can use all the free space, and compactions fall by 3.7x. The splice adds 7%
to 15% at two indexes and more: most compactions keep the page's column widths and write only the
new rows. SQLite's own time jumps at 10 indexes on this table, from 64 ms at 5 to 525 ms.

On the gate: `inillucent-fullgate`, medium fixture, 32 KiB page, the base commit against the change,
30 rounds each, alternated twice in one quiet window:

| workload | base | base | after | after |
|---|---:|---:|---:|---:|
| `write.insert.batch` | 0.88x | 0.81x | 1.52x | 1.52x |
| `write.update.indexed` | 2.13x | 2.10x | 3.77x | 3.52x |
| `write.delete` | 3.75x | 3.47x | 4.69x | 4.61x |
| `extension.fts.build` | 0.70x | 0.73x | 0.98x | 0.99x |
| `extension.rtree.insert` | 2.02x | 1.99x | 2.37x | 2.40x |
| `txn.large` | 4.06x | 4.15x | 3.84x | 3.59x |
| `join.range` | 0.90x | 0.87x | 0.87x | 0.84x |
| the `write` family | 2.43x | 2.33x | 3.28x | 3.24x |
| the weighted headline | 4.96x | 4.84x | 5.10x | 5.16x |

The `extension` family's lower bound went from 1.49x to 1.59x. `read.join`'s pooled lower bound went
from 3.14x and 3.04x to 2.92x and 2.98x. These passes were not pinned. The next section measures
`txn.large` and `join.range` again, pinned.

### The two workloads the format 2 leaf was suspected of slowing

`inillucent-fullgate`, medium fixture, 32 KiB page, 30 rounds, builds alternated, pinned to
`0xC03C03` with the SQLite child's mask read back as `0xC03C03` on every pass. 14:50 to 15:18:51Z on
2026-09-23. The rule, written before the passes: a slowdown is real only if the mean is at least 3%
slower **and** every pass is slower than every pass of the build before. The fix counts only if it is
at least 2% faster on the mean **and** every pass is faster. The fix makes a lookup past a leaf's
last delta key cost one comparison. inillucent's time in milliseconds:

| build | `txn.large`, each pass | mean | `join.range`, each pass | mean |
|---|---|---:|---|---:|
| before the delta area change (`420e68a`) | 2.838, 2.803 | 2.821 | 27.55, 27.37 | 27.46 |
| the delta area change (`abf042c`) | 2.845, 2.837, 2.899, 2.827 | 2.852 | 27.46, 27.43, 27.53, 27.59 | 27.50 |
| the fix | 2.726, 2.672, 2.726 | 2.708 | 27.46, 27.25, 27.27 | 27.33 |

- **`txn.large` did not slow down.** The delta area change is 1.1% slower on the mean, and its fastest
  pass (2.827) is faster than the slowest pass before it (2.838).
- **`join.range` did not slow down**, 0.1% on the mean. SQLite read 23.8 to 24.1 ms on every pinned
  pass, and the `read.join` pooled lower bound read 2.80x to 2.86x on all three builds.
- **The fix is 5.0% faster than the delta area change on `txn.large`**, and its slowest pass is faster
  than the delta area change's fastest.

| workload | before the delta area change | the delta area change | the fix |
|---|---:|---:|---:|
| `write.insert.batch` | 24.47 | 14.02 | 13.90 |
| `write.update.indexed` | 33.91 | 20.35 | 20.40 |
| `write.delete` | 20.49 | 16.08 | 16.28 |

Every pass of each build is inside 15.8 to 16.4 ms for `write.delete`.

Why the fix works: three in four of `txn.large`'s updates look up a rowid past the end of
`side_table` and land in its last leaf. `write.insert.autocommit` appended 100 rows to that leaf
earlier in the round, and those rows stay in the delta area until the free space fills.
`LeafRef::delta_search` now compares the last directory entry first. A key above it is past the
whole delta area and is answered after that one comparison.

27 unpinned passes (11:35 to 12:13:31Z and 12:35 to 13:06Z) gave the same answers at efficiency core
speed: `txn.large` 3.12 and 3.20 ms before the delta area change, 3.20 and 3.19 with it, and 3.04
with the fix. `join.range` read 33.0 to 33.6 ms on every build, while SQLite drifted from 24.8 to
31.6 ms and moved the `read.join` lower bound between 2.57x and 3.16x.

### What a statement costs before it reaches a tree

With the tree write removed from the update path, `txn.large` measured 1.54 µs against 1.53 µs.
Its gap was in statement setup:

| `UPDATE side_table SET note = ?2 WHERE id = ?1` | ns each | heap allocations |
|---|---|---|
| before | 2,219 | 33.7 |
| a layout shared instead of copied three times a statement | 1,896 | 21.3 |
| the row space, assignments and declarations built once per compiled statement | 1,318 | 14.3 |
| the undo image taken from the caller | **1,235** | **13.3** |
| the same statement, where no row matches | **615** | **7.0** |

A parameter is read when the expression is evaluated. An `Expr::Parameter` reads a cell the
statement refreshes on each execution, so a compiled statement can run again with new values.

### The workload that was measuring nothing

`txn.batched` and `txn.large` ran the same statement over the same rowids with the same text. By the
time `txn.large` ran, every row it touched already held the bytes it was about to write. It measured
2,000 updates that changed nothing.

That hid a defect. `only_change` returned `None` both when no column differed and when several did,
so an update that changed nothing took the most expensive path: a tombstone, a delta insert, and a
compaction every 32 writes. The same `UPDATE` run twice over the same rows:

| pass | ns each | allocations | inserted | in place |
|---|---|---|---|---|
| values differ | 1,723 | 13.3 | 0.00 | 0.25 |
| values identical, before | 4,067 | 56.7 | 0.25 | 0.00 |
| values identical, after | **867** | **10.8** | 0.00 | 0.00 |

An update that changes nothing is no longer written. `changes()` still counts the row, both trigger
times still fire, `RETURNING` still returns, and an index entry that did not move is skipped as
before.

The workload was corrected too. `txn.batched` and `txn.large` now reset `side_table.note` first,
outside the timed region on both engines. `txn.large` reads **0.09x** on the old workload with the old
engine, **0.59x** on the old workload with the current engine, and **3.63x** on the corrected
workload. SQLite's own time goes from 834 µs to 9.65 ms when it has to perform the 2,000 updates.

A heap slot used to accept only a value of exactly the same length, and `txn.large` replaces an 8
byte `note 1234` with a 42 byte `row 1234 lorem ipsum ...`. A longer value is now written at the
bottom of the leaf's heap after the tombstone bitmap and the delta area move down by its length, and
the slot points to it. A shorter value is written in place and the slot's length is lowered. The next
compaction reclaims the unused bytes. The log record did not change.

### Where a retrieval index's resident bytes go

Measured 2026-09-15 with `inillucent-indexresidency` on the 600,589 chunk corpus at 768 dimensions,
1,705,097 terms, with the vectors left in the file (the default). Each part is read in the order an
open reads it, and the resident set is sampled between parts. Two runs agreed to a tenth of a
mebibyte.

| part | on disk MiB | resident MiB | share of resident |
|---|---:|---:|---:|
| `lexical.bin`, the BM25 postings | 614.8 | **890.3** | **53%** |
| `store.bin`, the chunks and their dictionaries | 564.8 | 620.3 | 37% |
| `graph.bin`, the HNSW adjacency | 87.7 | 154.2 | 9% |
| `vectors.bin` | 1,759.5 | 0.0 | none |
| total | 3,026.9 | **1,664.9** | |

The postings are the largest part and the graph is the smallest.
[The roadmap](roadmap.md#2-memory-held-by-a-retrieval-index) uses these figures.

### Memory, earlier measurements

The peak memory figure was 102% more than SQLite's, then 43% more, then 14% more, and now 9.5% more.

- **102% to 43%**: a 512 KiB limit on the redo buffer, an index build that holds one copy of the tree
  instead of three, collecting a version log that nothing collected, and a byte limit on the
  allocator's free list.
- **43% to 14%**: a smaller file.
- **14% to 9.5%**: the index build writes pages to the file directly instead of through the page pool
  (design 2 of [the performance design](../tasks/task-2000-inillucent-performance-tdd.md)).

| | before design 2 | after |
|---|---:|---:|
| the child's peak | 42.45 MiB | **40.76 MiB** |
| `schema.index` raises the peak by | 12.50 MiB | **10.53 MiB** |
| the pool at that point holds | 24.66 MiB | **22.91 MiB** |

The peak after each workload, MiB, on the plan of that time:

| workload | peak, before | pool | everything else | peak, after | pool | everything else |
|---|---|---|---|---|---|---|
| the file opened and the pool warmed | 31.50 | 22.59 | 8.90 | **26.74** | **17.84** | 8.90 |
| every read workload | 31.54 | 22.59 | 8.95 | 26.79 | 17.84 | 8.94 |
| `write.insert.batch` | 34.73 | 22.94 | 11.55 | 30.92 | 18.19 | 12.20 |
| `schema.index` | **51.35** | 29.84 | 12.23 | **46.61** | 24.66 | 12.78 |

The "everything else" column does not move. The 4.74 MiB came out of the page pool, because the file
got smaller.

On 2026-09-20, four runs, before the correlated workloads joined the plan:

| workload | peak MiB | this workload added |
|---|---|---|
| the file opened and the pool warmed | 24.95 | 24.95 |
| every read workload, all eleven | 25.00 | 0.05 in total |
| `write.insert.batch` | 28.20 | 3.20 |
| the other four write workloads | 29.21 | 1.01 in total |
| `txn.batched` | 30.20 | 0.98 |
| **`schema.index`** | **40.75** | **10.53** |
| every remaining workload | 40.75 | nothing |

The plan held 30.20 MiB until it built an index. The index build added 10.53 MiB, and 12.5 MiB before
design 2. On 2026-09-23 the plan holds 31.46 MiB before the index build, because of the correlated
workloads, and the build adds 9.33 MiB to reach the same 40.8 MiB. One statement and its sort arena
decide the peak.
