# task-2226: an eighth performance hill climb, until every workload beats SQLite

## 1. What was asked

The earlier hill climbs left inillucent slower than SQLite on a few workloads. This ticket climbs
until inillucent beats SQLite 3.53.4 in every category, researching where needed and asking Fable for
advice on the hard items.

"Every category" is read as every workload the gate times, in all three measurements the published
performance page uses:

| Plan | What it runs | Workloads |
|---|---|---:|
| `en` | the engine plan through the engine API (`inillucent-fullgate`, the default) | 34 |
| `cc` | the same plan through the `Connection` API (`--api connection`) | 34 |
| `hc` | the 48 workload plan through the `Connection` API (`--plan hillclimb --api connection`) | 48 |

A workload beats SQLite when the median over the run's rounds of SQLite's time over inillucent's time
is 1.00x or more. The goal is all 116 cases at 1.00x or more.

## 2. The eval

The eval is the one task-2209 built, with one change: the three plans each become their own set of
cases, named `en:`, `cc:` and `hc:` followed by the workload, because the same workload behaves
differently through the two APIs.

- One pass is `survey.sh <build> <run> 10`: each plan once, 10 paired rounds, SQLite timed in the same
  rounds and every answer's digest compared with SQLite's.
- `tools/to-flow.mjs` writes each pass as a variant of the flow directory
  `_agent_output/task-2226/hillclimb/beat-sqlite/` in the main checkout (gitignored). Each row has three
  grades: `beats` (1 when the round's ratio is at least 1.00x), `speedup` (the round's ratio over the
  baseline's median ratio for that case) and `vs_sqlite` (the round's ratio).
- Train and test are the gate's fixed split: a workload is held out when FNV-1a of its name is 0 mod 3.
- The headline is the number of the 116 cases whose median `vs_sqlite` is at least 1.00x. The
  geometric mean of `speedup` is reported beside it, to show that nothing got slower.

The machine had no other agent on it for the whole ticket. The gates' own quiet check graded the
engine plan passes (SQLite's speed index within 3% of its idle reference).

## 3. The baseline: a regression first

`main` at `44e07d43` (release 2.5.2) beat SQLite on 74 of the 116 cases. Through the `Connection`,
every point read was about 0.19x, where the last published run had it at 21x.

The profile of `point.rowid` showed why. 71% of each statement was three lookups by file name, which
the bug hunt of 9 October added (task-2217):

- `ImportedDatabase::file_was_replaced`, twice: was the database file replaced by `VACUUM` or a
  backup in another process, asked before and after the lock.
- `Wal::next_segment_exists`: did another process roll the log to a new segment.

Each is one `GetFileInformationByName` call, which its comment priced at 3 us. On this machine, with
its file system filter drivers, each took about 20 us.

## 4. Rounds 1 and 2

### 4.1 Round 1: the lookups by name are skipped while the log oplock holds

`crates/inillucent-vfs/src/os/windows_log.rs`, `contract.rs`, `crates/inillucent-wal/src/writer.rs`,
`crates/inillucent-engine/src/engine/locks.rs`.

On Windows a process already holds a batch oplock on its open log segment (task-2209). While it
holds one, no other handle to the segment exists on the machine, and any other open breaks it before
that open proceeds. The watcher now counts the breaks, and `LogShare::quiet_token` returns a token
that changes on every break. A statement that has asked both questions by name and found nothing
remembers the token, and skips both lookups while the token is unchanged.

That is sound because neither event can happen without another process opening the segment:

- `VACUUM` and a backup remove the replaced database's log segments before the rename
  (`crate::rebuild::remove_log_segments`), and a delete opens the file.
- A roll that `next_segment_exists` is there to find follows records the other process appended to
  this segment. A roll with nothing to append is a checkpoint's, which moves the meta record that
  every statement still reads, and which then deletes this segment.

Where there is no oplock (Linux, macOS, `INILLUCENT_LOG_OPLOCK=0`, a segment not opened yet), the
token is `None` and every check runs as before.

Tests (`crates/inillucent-compat/tests/durability/log_oplock.rs`):

- `the_oplock_takes_the_lookups_by_name_off_a_read`: 100 point reads under the oplock make no lookup
  by name; with another handle open on the segment they make at least 100.
- `a_vacuum_by_another_process_is_found_under_the_oplock`: the shipped command line runs `VACUUM`
  while this process reads under the oplock; a row inserted afterwards is in the file the path names.
  With the break count disabled the reopen counts 50 rows instead of 51, so the test fails.

Result: 74 to 104 of 116 cases at or above SQLite.

### 4.2 Round 2: a range delete unlinks the leaves it empties

`crates/inillucent-tree/src/write/unlink.rs` (new), `write/sorted.rs`, `write.rs`, `mutate.rs`,
`crates/inillucent-exec/src/dml/delete.rs`, `crates/inillucent-engine/src/engine/write.rs`,
`engine/batch.rs`.

`hc:edge.delete.range`, `DELETE FROM copied WHERE id BETWEEN 20000 AND 59999`, was 0.23x. It was
1.04x on 5 October. Since task-2217 made a delete free the pages it empties, every emptied leaf was
merged with its right sibling: the sibling's rows were packed into the emptied page and the merge was
logged as three whole page images. In a range delete the right sibling is the next leaf to be
emptied, so the statement copied about 165 full leaves and logged about 16 MB.

Four changes, measured one after another on the delete alone (5 rounds, median):

| change | inillucent | SQLite | ratio |
|---|---:|---:|---:|
| before | 43.1 ms | 11.5 ms | 0.28x |
| an emptied leaf is unlinked: its left neighbour's right link is rewritten with a `Relink` record, the parent is logged once, the page is freed | 16.4 ms | 11.2 ms | 0.67x |
| the leaves a sorted delete empties under one parent are unlinked together, with one write of the parent | 13.6 ms | 11.8 ms | 0.86x |
| one undo record per leaf for a sorted delete run, not one per row | 13.2 ms | 12.1 ms | 0.89x |
| the tombstones of a run are set with one parse of the leaf header | 11.3 to 11.8 ms | 11.2 to 11.7 ms | 0.99x to 1.00x |

The log format does not change: `Relink`, `WritePage` and `FreePage` records already exist and replay
by the page LSN rule.

### 4.3 Round 2: the `NOT NULL` check borrows the row

`crates/inillucent-exec/src/dml/insert.rs`. `declarations_are_met` cloned every value of the row to
ask whether it was NULL, a string copy per text value: 6% of `hc:ai.copy`.

## 5. The Fable consultation

Filed as its own ticket on `claude-fable-5-1[1m]` (task-2242) with the profiles of the cases still
behind after round 2, asking for a ranked list of changes per case with the risks to correctness
named. The answer is `_agent_output/task-2226/fable/advice.md` in the main checkout. Its items, and
what became of each:

| Fable's item | Done here? |
|---|---|
| `app.dashboard`: a compact integer hash join chosen at run time | No. A merge of the probes was built instead (6.1), which keeps the index plan and the row order |
| `app.dashboard`, `join.range`: compare interior separators as integers | Yes, by their leading eight bytes (6.8) |
| `edge.update.all`: fewer copies of each row, no wasted page copy | Yes (6.5, 6.6) |
| `ai.copy`: keep the leaf layouts, reuse the source row, coalesce the leaf writes, then a columnar sink | Yes (6.2, 6.7) |
| the correlated selective cases: keep SHARED under the oplock token | No. The cause was in the gate (6.4) |
| `prepare.trivial`: fewer allocations, a literal `SELECT` fast path | No (section 8) |
| `txn.autocommit`: measure the log handle's write path first | Measured: the commit is one disk flush, as SQLite's is (section 8) |
| `edge.delete.range`: the fold cannot leave this statement at acceptable risk | Agreed (section 8) |

## 6. Rounds 3 to 7

### 6.1 Round 3: an index nested loop merges the rest of a large outer side

`crates/inillucent-exec/src/join/merged.rs` (new), `join/loops.rs`, `compiled.rs`,
`physical/joins.rs`.

`hc:app.dashboard` joins 12,500 `main_table` rows to `side_table` through `side_owner`, one descent
per outer row, with keys in random order. An inner join on one key into a tree whose first key column
ascends now probes its first outer rows as before, and once it has seen more outer rows than a tenth
of the inner index it buffers the rest, sorts their keys, walks the inner index once over the range
they cover, and puts the pairs back in the order the probes would have given them. Only integer keys
are buffered; any other key flushes the buffer and is probed. The rows come out in probe order, so a
`sum` of reals above the join adds in the same order as before.

`ProbedOuterJoin` (a `LEFT JOIN` whose `ON` holds more than the key) pushes one outer row into an
inner join and reads the matches before the next row, without calling `finish`. The merge held those
matches back, and every such outer row came back null extended. The first full test run found it
through `differential_part8`, `usage_corpus` and the `join` and `ddl_index` matrices. Those joins are
now built without the merge, and `a_left_join_with_more_than_the_key_matches_sqlite` in
`tests/differential/merge_join.rs` covers the shape at a size where the merge would start.

In round 7 the buffer's two vectors are kept, emptied, for the next buffer on the thread: a compiled
statement builds its joins again on every execution, so each execution grew them from nothing.

Result: `hc:app.dashboard` 0.78x to 0.96x in round 3, and 1.11x after round 7.

### 6.2 Round 3: the bulk build keeps its layouts and writes leaves in runs

`crates/inillucent-tree/src/paged/bulk.rs`, `paged/bulk/leafplan.rs`, `paged/bulk/queue.rs` (new),
`crates/inillucent-pool/src/pool/fold.rs`, `pool/journal_gate.rs`, `journal.rs`,
`crates/inillucent-exec/src/insert_plan.rs`.

The leaf planner computed each leaf's column widths and threw them away, and the encoder computed
them again. Built leaves were written one `write_all_at` each; they are now queued and written 64
pages to a call. The fold reads the file's length once per pass instead of once per page, and the
journal appends a pre image to its buffer instead of growing it with zeros and copying. An insert
whose source row already is the image (each value lands where it was) keeps the row instead of
building a second one.

### 6.3 Round 4: a filter judges a batch of integer comparisons in one loop

`crates/inillucent-exec/src/expr.rs`, `expr/tree.rs`, `ops/row.rs`, `correlate.rs`.

`Eval::truths_over` answers, for a whole batch, whether a predicate is definitely true. A comparison
of two integer expressions (a column, a constant, arithmetic, a division or modulo by a constant), an
affinity comparison whose affinity leaves an integer alone, and an `AND` of those answer it; every
other node says no and the caller asks `value` a row at a time, which gives the same answers. A NULL
on either side makes the batch take the row path. `Filter` and the correlated block's gate use it.
`a.id % 100 = 0` was two thirds of `correlated.exists.selective`'s execution.

`tests/e2e/mcp_cancel.rs` needed a slower statement: its three way cross join with
`a.n + b.n + c.n >= 0` over 200 rows finished in 0.05 s, below the half second the test needs to see a
cancellation land early. It now uses 600 rows.

### 6.4 Round 4: the `Connection` arm compiles a `prepare: once` statement before its clock

`crates/inillucent-compat/src/bin/fullgate.rs`.

`time_one_through_a_connection`'s doc comment says `prepare` is outside the clock for a workload the
plan marks `prepare: once`, which is where the pipeline arm and `sqlite_bench.c` put it: SQLite calls
`sqlite3_prepare_v2` and then reads its clock. The code read the clock first. For a workload that runs
its statement once, such as `correlated.scalar.selective`, the `Connection` arm was timed as a compile
plus one execution against SQLite's one execution. With `--repeat 2000` this engine ran the two
selective statements 2.4 and 2.8 times faster than SQLite; the gap was the compile. The prepare now
happens before the clock, as documented.

This is a change to the measurement, not to the engine. It moves the published `Connection` numbers
of every `prepare: once` workload whose `repeat` is small, and `docs/performance.md` says so.

Result: `cc:correlated.exists.selective` 0.84x to 1.31x, `cc:correlated.scalar.selective` 0.78x to
1.20x.

### 6.5 Round 5: the update run asks its questions once

`crates/inillucent-tree/src/write/sorted.rs`, `mutate.rs`.

Once a run's leaf copy exists, `update_slot` is tried on it directly. `would_update_slot` asked the
same questions first and parsed the leaf four more times a row; `update_slot` refuses before it
changes a byte, and each of its refusals is one `would_update_slot` makes, so the page comes out the
same. Carving room in the heap parses the leaf once instead of three times.

### 6.6 Round 5: the update run's undo image is one copy of the leaf

`crates/inillucent-tree/src/write/sorted.rs`.

Each updated row's undo image was its own vector, kept until the transaction ended: 25,000 of them
for `UPDATE side_table SET note = note || '!'`. The run now records one copy of the leaf and where
each row lies in it, the record the delete run already uses (`FromPage::Run`). A rollback reads each
row back with `row_from_page` and puts it, the same `put` a row image gets. A leaf with an out of line
value keeps a row image per row, because the copy alone cannot give the value back. The run also reads
each column's place in the leaf once, not once per row.

`UPDATE side_table SET note = note || '!'` measured alone, 12 rounds, medians:

| change | inillucent |
|---|---:|
| before | 18.4 ms |
| 6.5 | 17.6 ms |
| 6.6, the undo image | 15.3 ms |
| 6.6, the columns read once | 13.5 ms |

SQLite takes 14.0 ms for it in the full plan, where its page cache is warm.

### 6.7 Round 6: an `INSERT ... SELECT` the bulk build takes collects its rows packed

`crates/inillucent-exec/src/packed.rs` (new), `ops/collect.rs`, `compiled.rs`,
`dml/insert/bulk.rs`, `insert_plan.rs`, `crates/inillucent-engine/src/engine/bulk.rs`,
`engine/compiled.rs`, `plans.rs`.

`PackedRows` holds rows as 16 byte cells in chunks of 16,384, and every text and blob in chunks of
256 KiB of bytes. When an `INSERT ... SELECT`'s shape and its empty target admit the bulk build, the
compiled source collects into it instead of a vector a row. The bulk build lends each packed row to
the same `build_row_from` as before, keeps the images packed too, and the leaf builder reads them
through its `Rows` trait. A packed set the bulk build declines (fewer than 1,024 rows, a duplicate in
a unique index) is unpacked for the ordinary insert, which then runs as before.

The first version used one growing vector for the cells and one for the bytes: `ai.copy` went from
46.4 ms to 42.6 ms. Growing them was then a third of the statement, each doubling copying everything
into fresh memory, so the chunks: 37.6 ms, against SQLite's 42.6 ms.

### 6.8 Round 6: interior separators are compared by their leading eight bytes first

`crates/inillucent-pool/src/interior.rs`.

`child_for` already read each separator's first eight bytes, padded with zeros and read big endian,
to place its interpolated guesses. When that number differs from the probe's it decides the order by
itself, so the byte compare now runs only on a tie. `an_interpolated_descent_agrees_with_a_linear_scan`
gained separators that are prefixes of each other ending in zero bytes, which pad to the same number
and must reach the byte compare. About 1% on `join.range` and `app.dashboard`.

## 7. Results

The last pass, round 7, against the baseline (release 2.5.2), 10 paired rounds of each plan, medians:

| plan | cases at or above SQLite, baseline | round 7 | geometric mean against SQLite, baseline | round 7 |
|---|---:|---:|---:|---:|
| `en`, the engine API | 31 of 34 | 31 of 34 | 4.57x | 4.72x |
| `cc`, the engine plan through the `Connection` | 17 of 34 | 32 of 34 | 0.90x | 4.02x |
| `hc`, the 48 workloads through the `Connection` | 26 of 48 | 47 of 48 | 1.02x | 3.83x |
| all | 74 of 116 | 110 of 116 | 1.53x | 4.13x |

Every answer agreed with SQLite's in every round. The cases that were behind at the start or are
behind now:

| case | baseline | round 7 | inillucent | SQLite |
|---|---:|---:|---:|---:|
| `hc:app.dashboard` | 0.78x | **1.11x** | 70.5 ms | 78.3 ms |
| `hc:ai.copy` | 0.87x | **1.13x** | 37.6 ms | 42.6 ms |
| `hc:edge.update.all` | 0.70x | **1.01x** | 13.7 ms | 14.0 ms |
| `hc:edge.in.long` | 0.70x | **1.05x** | 6.9 ms | 7.3 ms |
| `hc:app.json.where` | 0.97x | **1.05x** | 37.8 ms | 39.9 ms |
| `cc:correlated.exists.selective` | 0.21x | **1.31x** | 17 us | 22 us |
| `cc:correlated.scalar.selective` | 0.18x | **1.20x** | 18 us | 21 us |
| `cc:correlated.in` | 0.52x | **1.17x** | 85 us | 99 us |
| `hc:edge.delete.range` | 0.23x | 0.33x | 34.3 ms | 11.1 ms |
| `en:prepare.trivial` | 0.56x | 0.56x | 2.91 ms | 1.62 ms |
| `cc:join.range` | 0.42x | 0.95x | 25.3 ms | 23.9 ms |
| `en:join.range` | 0.99x | 0.99x | 24.4 ms | 24.2 ms |
| `en:txn.autocommit` | 0.85x | 0.97x | 114.8 ms | 109.6 ms |
| `cc:txn.autocommit` | 0.87x | 0.99x | 114.7 ms | 113.2 ms |

Most of the baseline's `cc` and `hc` cases at 0.2x were the regression of section 3. Round by round:

| round | change | cases at or above SQLite |
|---|---|---:|
| 0 | release 2.5.2 | 74 |
| 1 | lookups by file name skipped under the log oplock | 104 |
| 2 | a range delete unlinks the leaves it empties | 104 |
| 3 | the merge join, the bulk build's layouts and write runs | 105 |
| 4 | batch comparisons; the `Connection` arm's clock | 107 |
| 5 | the update run | 108 |
| 6 | packed rows; the interior compare | 109 |
| 7 | the merge buffer's vectors kept | 110 |

## 8. What is still behind, and why

**`hc:edge.delete.range`, 0.33x.** Measured alone the delete is level with SQLite (section 4.2). In
the full plan the log passes the 4 MB fold threshold during this statement every round, and the fold
(a pre image of every dirty page to the journal, the pages written back, the meta record, four syncs)
is about 23 ms of its 34 ms. SQLite's rollback journal pays the same kind of work inside each write
statement. Fable's advice and my own reading agree that the engine cannot take the fold off this
statement at acceptable risk: an incremental fold costs a journal cycle of three syncs per increment
and pushes `txn.autocommit` below SQLite, and a fold on a background thread needs the pool, the write
lock and the compiled statements shared across threads in an engine built single threaded. Two
decisions remain, and both are Jason's:

- a design ticket for a fold that does not land on a user's statement: a background fold, or
  journaling each page's pre image when it is first dirtied, as SQLite does;
- or a gate change: run the checkpoint at the end of each round on both arms, outside every
  statement's clock, and report it as its own line, so the fold's cost is still counted.

**`en:prepare.trivial`, 0.56x.** `SELECT 1` compiled from text on every call through the engine API,
which no program calls: through the `Connection`, which keeps compiled plans by text, the same
workload is 1.66x. The 2.9 us a call is spread over parsing, binding, planning and building the
operator chain, with no single large cost. The ways to close it are a cache of plans in the engine
API, which would stop the workload measuring a compile, or a fast path for literal `SELECT`s, which
helps nothing a program does. Neither is done.

**`txn.autocommit`, 0.97x and 0.99x.** A commit is one append to the log and one `FlushFileBuffers`.
On this disk a flush of a 512 byte write takes 1.2 ms whether the file grows or not (measured with a
separate probe), and SQLite's commit costs the same 1.1 ms. Going faster means syncing less, which
this ticket does not do. Earlier runs read this case between 0.98x and 1.07x.

**`join.range`, 0.95x and 0.99x.** 201 outer rows a run, each an index probe into `side_owner` and a
rowid lookup into `side_table` per match. Both engines do the same probes; the descent was tuned by
earlier climbs and section 6.8 took about 1%. Separate runs of this round read it between 0.95x and
1.05x.

## 9. Record

Each round's builds, gate outputs and samples are under `_agent_output/task-2226/` in the main
checkout: `bin/`, `runs/`, and the flow directory `hillclimb/beat-sqlite/` with `_state.json`,
`narrative.md` and `report.html`. Profiles were taken with the Visual Studio standard collector and
are not kept: they were several gigabytes each.
