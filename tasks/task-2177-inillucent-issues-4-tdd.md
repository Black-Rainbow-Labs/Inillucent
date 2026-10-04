# The three issues reported against inillucent 2.0.7

## Terms used in this document

| Term | Meaning |
|---|---|
| shared extent page | a page that holds several values that are too large for a leaf and smaller than one page, each in a numbered slot |
| fold | writing the pages the log describes into the database file, under the EXCLUSIVE lock |
| the bar | `RECLAIM_BYTES`, 4 MiB: a statement that leaves more log than this behind folds before it lets the file go |
| merge | joining several segments of an `inillucent_search` table into one at the next level |
| storm | a run of several real processes on one file, killed at moments a seed chooses, graded by a process that wrote nothing (`inillucent-chaos`, `crates/inillucent-compat/src/storm.rs`) |
| the report | the issue report against 2.0.7 in the ticket: issue 1 damage after SIGTERM, issue 2 slow merges, issue 3 a slow publish commit |

Every number below was measured on the Windows development machine (24 threads, NVMe), with the
report's own scripts ported to the Windows C library. They live with the evidence in
`_agent_output/task-2177-issues/` in the main checkout. The machine was shared with three other
tickets during most measurements, so single timings vary by up to two times; comparisons were run
back to back.

## 1. Issue 1: damage after SIGTERM with a reader open

### 1.1 What the error means

`a reference names slot 5 of a shared page that holds 5` comes from `extent::shared::read` in
`crates/inillucent-pool/src/extent.rs`. A leaf holds a reference to slot 5 of a shared extent page,
and the page's directory has only slots 0 to 4. Every change to a shared page is logged as a whole
page image (`paged::log_shared_page`), so a replay that applies every committed record cannot
produce this: the last image of the page always has every slot a committed leaf names. The page in
the file is older than the leaf, which means a replay did not apply a committed image of that page.

This was confirmed by mutation. With `log_shared_page` changed to skip the image record for a page
that already existed, the new storm case below fails with
`a reference names slot 1 of a shared page that holds 1`, the report's message.

### 1.2 What changed after 2.0.7

The release that followed 2.0.7 fixed eleven defects found by the first multi process storms
(task-2173, `tasks/task-2173-real-world-failure-testing-tdd.md` section 6). Three of them lose or
misapply committed log records in exactly the situation the report describes, a writer and a
reader on one file:

- an open kept its lock after it finished, so the first statement after it trusted the open's
  replay and appended its commit over another process's committed records. Every later recovery
  stopped at those bytes and left indexes out of step with their tables;
- a transaction could reuse a number another process had used, which made a killed process's
  unfinished records look committed;
- a checkpoint called between statements folded this process's older pages over another process's
  commit.

The report's reader writes a row now and then, and its writer's first statements after it opens
run an `embed()` call of about a second before the first `UPDATE`. That leaves a long window right
after the writer's open in which the reader commits, which is the window the first defect needs.

### 1.3 Reproduction attempts

A Python reproduction of the report's two processes over the C library
(`_agent_output/task-2177-issues/storm/`): 40,000 chunks, 2.6 GB start state, a third of the
contents between 4.2 and 20 KB so they sit on shared extent pages, a trigger to an outbox, a
publish transaction into an `inillucent_search` table with facets, a search service that searches
every 300 ms and writes a row every fifth search, both killed with `TerminateProcess` at the same
moment after 5 to 120 s:

| Library | Shape | Rounds | Damaged |
|---|---|---|---|
| 2.0.7 | as above | 40 | 0 |
| 2.0.7 | job pauses 800 ms between read and write, search writes after every search | 14 | 0 |
| current main | as above | 20 | 0 |

The report says the same: its standalone Python reproduction, 27 rounds, did not damage the file
either. The Node application's timing is what this harness does not have.

### 1.4 The storm case added

`inillucent-chaos` gains `--rewrite` (`chaos::Settings::rewrite`) and the storm gains
`kill_all_at_end`:

- before each transaction a worker rewrites one row of `doc` in an autocommit upsert, with a value
  of 4.2 to 20 KB drawn from its seed (`chaos::doc_body`), so shared extent page slots are placed
  and cleared all the time by `UPDATE`;
- `doc` has an index on `version`, and every reader check reads every `doc` value, compares it
  with its digest and compares the table's count with the index's;
- a reader writes a row of `reader_log` after every check, as the report's search service did;
- with `kill_all_at_end` the run ends with every process killed at the same moment, so the first
  replay of the log is the grading process's own open;
- the grader checks each worker's `doc` slots against the seed: a slot holds the version of the
  last recorded transaction that wrote it, or the version of the transaction after the last, whose
  autocommit rewrite committed before the kill.

Two cases in `durability::process_storm`: `a_storm_in_the_shape_of_the_2_0_7_damage_report_loses_nothing`
(no kills until the end) and `..._with_kills_loses_nothing`. Both pass on main. The mutation in
section 1.1 makes the second fail.

### 1.5 Running the case against 2.0.7

The harness files were copied into a checkout of the 2.0.7 commit (`8e3366d0`) and built there
against that engine, so the same workload and checks run on the release the report used. Twelve
rounds of the report shape on each, alternating rounds with and without kills during the run, every
round ending with all processes killed at once:

| Engine | Rounds | Passed | What failed |
|---|---|---|---|
| 2.0.7 | 12 | 0 | in every round, processes refused at open with `busy` (`this connection had to write pages it replayed from the log, and another connection holds the file for writing`); in one round a worker also met `corrupt: ... page 3 carries lsn 921176, which is at or above the log's end 426360` |
| current main | 12 | 12 | nothing: 194 to 1,180 acknowledged transactions a round, every one in the file byte for byte |

The refused open is the second defect task-2173 fixed, and it ends the processes early, so most
2.0.7 rounds never reach the point where damage would show. The replay error in the one round that
got further is the family section 1.2 describes: a page stamped by log records the replay no longer
finds. Neither appears on main.

### 1.6 Conclusion for issue 1

The report's damage is not reproduced on current main, by the Python reproduction (20 rounds) or by
the storm in its shape (12 rounds plus the two durability cases), and the same storm fails on 2.0.7
from its first round. The defects fixed after 2.0.7 that lose or misapply committed log records
beside a second process are the explanation that fits the error. No engine change is made for
issue 1 in this ticket; the storm cases stay as the regression test, and the nightly tier runs a
three minute version (`nightly::storm_nightly::a_long_storm_in_the_shape_of_the_2_0_7_damage_report_loses_nothing`).

What would still help: the reporter's damaged files (`workload-both-round-7/`) and a rerun of their
`run.sh` on 2.1.x. A file damaged by 2.0.7 stays damaged when a later release opens it, as the
report found for files damaged by 2.0.4.

## 2. Issue 2: commits that merge segments

### 2.1 Where the time goes

Traced with the report's script (`repro2_segments.py`), 2,000 rows of 768 numbers per commit:

| Step of a merging commit | Time |
|---|---|
| flush of the commit's own 2,000 rows into a level zero segment | 0.25 to 0.44 s |
| one fold of 2,000 chunks into the accumulator (HNSW inserts) | 0.45 to 0.8 s |
| one fold of 8,000 chunks at level one | 2.9 to 3.8 s |
| writing the merge's delta link | 0.1 to 0.7 s |
| `finish_merge` reloading the accumulator and every input | 0.11 s at level zero, 1.38 s at level one |

The graph insert dominates. On one thread an insert into a graph of a few thousand 768 number
vectors costs about 2.6 ms, and with 24 threads the same 2,000 inserts take 0.47 s, so the
parallel insert scales about elevenfold. Most of an insert is the neighbour pruning heuristic,
which compares each candidate with the neighbours already kept.

### 2.2 Change

**`finish_merge` no longer reads the accumulator back.** `continue_merge` returns the accumulator it
just folded (`FinishedMerge`), and the tombstone list is computed from the ids each input held
(`merge::dead_after_merge`, `merge::live_ids_of`). An input folded in the same commit is not read
again; an input an earlier commit folded is loaded once, for its ids.

Measured during a quiet window with the report's script, 60 commits, the second run of each kept:

| Build | Slowest three commits | Total of all 60 commits |
|---|---|---|
| before | 7.52, 7.43, 6.17 s | 109.3 s |
| after | 6.32, 6.15, 4.81 s | 104.5 s |

The slowest commits are the ones that finish a level one merge, and each lost about 1.2 s.

### 2.3 A change that was tried and taken out

`Hnsw::select_neighbours` copies each candidate's vector (`copy_of`, 3 KiB at 768 numbers) before
comparing it with the neighbours already kept. Reading it in place through `VectorSet::with` looked
cheaper and measured slower: 3.55 s against 3.17 s for 2,000 single thread inserts, twice each. The
copy stays.

### 2.4 What is not changed

The merge budget (`DEFAULT_MERGE_BUDGET`, 8,192 chunks a commit) and the segment fan in stay as
they are. Lowering the budget would shorten the longest merging commit and add merge work to more
commits; that is a trade an application can already make with `merge_budget = N`, and the report's
workaround (`compact = 1000000` and a scheduled `compact`) still applies.

## 3. Issue 3: the 2,000 row publish commit

### 3.1 Where the time goes

The commit is where the fold runs. A publish of 2,000 random rowids of 120,000 dirties about one
leaf in five of `s_content`'s 11,400: traced, 2,384 dirty pages, all leaves of that one tree. The
transaction's log passes the bar, so the statement that ends it folds:

| Part of the fold | Time |
|---|---|
| reading 2,384 pre-images and writing them to the rollback journal | 92 ms |
| syncing the journal | 34 ms |
| writing 2,384 pages into the file | 57 ms |
| syncing the data file | 444 ms |

The 100 and 500 row publishes stay under the bar and their pages are folded by a later statement.
2.0.4 measured on this machine shows the same work in a different place: its 500 row publish
commit took 0.65 s and its 2,000 row commit 0.37 s, because its slow `DELETE` had already pushed a
fold into the earlier commit. The fold did not get larger in 2.0.7; it moved.

On macOS the journal syncs are `F_FULLFSYNC`, which flushes the drive's cache including the data
pages, which is why the report measured 2.02 s for the same commit.

### 3.2 A change that was tried and taken out

Saving the pre-images in batches (one length query, the pages read in order into a buffer, one
write per 64 records and one header write) replaced about 9,500 calls into the file system with
about 2,500 for this fold. Measured back to back three times each, the commit took 0.91 to 0.96 s
before and 0.94 to 1.01 s after. The calls were not where the time was, so the change was reverted.

### 3.3 What is not changed

The data file sync is the device writing 75 MiB at random offsets, and it is most of the commit.
Making a 2,000 row publish cost what its delete and insert cost would mean not folding at that
commit, and then a later statement pays the same fold while holding EXCLUSIVE. The report's
workaround of smaller batches spreads the fold over more commits.

## 4. Tests

| Test | What it checks |
|---|---|
| `durability::process_storm::a_storm_in_the_shape_of_the_2_0_7_damage_report_loses_nothing` | section 1.4, no kills before the end |
| `durability::process_storm::a_storm_in_the_shape_of_the_2_0_7_damage_report_with_kills_loses_nothing` | the same with kills during the run |
| `nightly::storm_nightly::a_long_storm_in_the_shape_of_the_2_0_7_damage_report_loses_nothing` | section 1.4 for three minutes with eight workers and four readers |
| existing `engine::segment_merge_bound`, `segment_delta_chain`, `segmented_generations`, `search*`, `durability::search_crash` | the merge change keeps every row and tombstone |
