# Bug hunt 8: what real applications run into

## Terms used in this document

| Term | Meaning |
|---|---|
| case | a short SQL script run against a fresh database through both the pinned `sqlite3` and `inillucent-shell`, whose outputs are compared byte for byte |
| probe | `tools/bug-hunt/probe.mjs`, which runs case files and reports every difference, crash, hang and slow case |
| reopen | a line `-- reopen` inside a case: the shell process ends and the rest of the case runs in a new process on the same file |
| hunter | a sub agent given one area of real use and asked to find defects in it, not to fix them |
| fixer | a sub agent given a list of defects in one area, working in its own git worktree, asked to write a failing test and then fix each |
| red, green | the test run before the fix, which must fail, and after it, which must pass |

## 1. Why

The ticket asks for tests that cover what real applications do, for the edge cases with unexpected
behaviour and the slow paths found that way, and for each defect to be shown red and then fixed.
Partway through, it was widened: processes killed while they write, several clients on one file,
and SQLite bugs fixed on GitHub that a second implementation of the dialect may share.

The previous bug hunt (`tasks/task-2201-bug-hunt-tdd.md`) mined SQLite's TCL tests and other engines'
bug trackers, and compared SQL answers. Its corpus runs every case in one shell process, so nothing
in it ever closed a database and read it back, ran two processes, or used the command line the way a
script does. That is where this hunt looked.

## 2. Method

### 2.1 The harness changes

- **`-- reopen` in the probe and in the usage corpus.** `tools/bug-hunt/probe.mjs` and
  `crates/inillucent-compat/tests/support/shell_corpus.rs` split a case at each `-- reopen` line and
  run each part in its own shell process on the same file. A value that only lived in memory, such
  as a `PRAGMA user_version` a `ROLLBACK` should have undone, now has to survive a close to pass.
- **Two process and many process drivers**, written as scripts for the hunt
  (`_agent_output/task-2217/lead/twoproc.mjs`, `samestart.mjs`, `idleopen.mjs`), turned into Rust
  tests once a defect was found: `durability::process_schema_change` and `durability::process_create`.

### 2.2 The hunts

Eight hunters ran at once, each in its own folder with its own database names, against a release
build. Each wrote at least 150 cases in its area and reported minimised repros.

| Hunter | Area | Cases | Defects reported |
|---|---|---|---|
| ORM | schemas and migrations the way Django, Rails, Prisma, Room and eight others write them, across reopens | about 380 | 9 |
| data | messy text, numbers and dates through functions and comparisons | about 30,000, most generated | 10 and a cosmetic list |
| speed | application shaped queries at 20,000 to 300,000 rows, warm and after a reopen | 416 | 7 |
| CLI | the `inillucent` program's import, export, dump, backup, restore and migrate round trips | about 700 command runs | 15 |
| two processes | interleaved statements, kills during writes, many short processes | 18 scenarios, 1,100 killed writers | 5 |
| crash | 3 to 8 processes writing, reading, checkpointing, vacuuming and backing up one file, killed at random, then every acknowledged commit checked | about 1,900 runs, and 620 against SQLite with no loss | 7 |
| search | FTS5 and vector search with writes, rollbacks and reopens | about 250 | 14 |
| upstream | SQLite's own fixes from 3.46 to 3.53, the forum, and other engines' trackers | about 850 | 35 |

My own probes added the close and reopen lifecycle cases, the driver's sessions and handles, and the
two process schema cases.

### 2.3 The fixes

Each defect got a test that fails on the code before the fix. A SQL difference became a case in a new
usage corpus file, `compat/corpus/usage/bug-hunt-8-<area>.sql`, so `differential::usage_corpus` grades
it against SQLite on every change. Anything SQLite does not have, or any timing, became a Rust test.
I fixed the engine, locking and storage defects myself. Five fixers took the rest in their own
worktrees, one area each, and their branches were merged into `task-2217`: FTS5; FTS3, FTS4 and the
tokenizers; vector search; the command line and `.import`; small SQL differences; and slow paths.

## 3. Defects found and fixed

"Test" names the red then green test. Corpus cases are in `compat/corpus/usage/bug-hunt-8-*.sql`.

### 3.1 Wrong answers and lost data

| Defect | Found by | Cause | Fix | Test |
|---|---|---|---|---|
| A `UNIQUE` index accepted a duplicate of the first key of every leaf after the first (6529, 13057, 19585 in 20,000 integers) through every conflict path | ORM | the uniqueness probe uses the key without its rowid; that prefix sorts before every entry with it, so the descent landed on the leaf before and found nothing | `PagedTree::probe` walks right while the run can continue, the rule `visit_equal` already used | `engine::unique_leaf_boundary` |
| After another process's `ALTER TABLE`, a process that kept the file open returned the old columns, failed with an internal error after a `DROP COLUMN`, and lost an `INSERT` after a `DROP` and `CREATE` of the table | lead, two shells | the schema generation was compared before the file lock, and the lock is what rereads the catalog | the lock moving the generation returns `SQLITE_SCHEMA`, and the statement is compiled again and rerun | `durability::process_schema_change` |
| A table, view or column another process created was never found by a process already open | lead | the statement failed to compile, so it never took the lock that rereads the catalog | a failed compile checks the schema under a shared lock and compiles again, as `sqlite3Prepare` does | `durability::process_schema_change` |
| Two processes opening a missing path together: one failed with "cannot find the file specified", and a commit could be lost | lead | creating deleted the path and truncated the file under the lock, even when another process had just written it | `Database::create_if_empty`: creation deletes nothing and gives way to a database another process wrote, and a creator that cannot get RESERVED lets SHARED go before it retries | `durability::process_create` |
| Eight shells opening a missing path at once: about one shell in a hundred failed with "cannot find the file specified" | lead | the first open failed because the file was not there yet, the check that followed found it because another shell had just created it, and the first open's error was reported | the open is tried once more when the file has appeared | `durability::process_create`: 3 of 8 runs failed before, 0 of 22 after |
| An empty file was "malformed" | CLI | | it opens as an empty database, as in SQLite | `durability::process_create` |
| `n IN (1, '1')` over an index returned the row twice; an `UPDATE` applied twice; same under `NOCASE` | data | literal repeats were folded at plan time, but `1` and `'1'` only meet after affinity | the seek union drops a branch whose converted bounds equal another's | corpus `engine/in-list-*` |
| `INSERT INTO t(rowid, ...)` ignored the value on an `INTEGER PRIMARY KEY` table; a text rowid was accepted | upstream | the key column looked only for its own name in the target list | a rowid name sets the key too, the last named wins | corpus `engine/insert-names-*` |
| `PRAGMA user_version` and `application_id` survived `ROLLBACK` and `ROLLBACK TO` | ORM | the meta page is not in the log; the pragma checkpointed at once | inside a transaction the old value is recorded, put back by a rollback and checkpointed at commit | corpus `lifecycle/reopen-user-version-*` |
| `VACUUM` reset the `AUTOINCREMENT` sequence and dropped WAL mode | ORM, upstream | `sqlite_sequence` was skipped with the other `sqlite_` tables; the WAL flag was not carried | both are carried | corpus `lifecycle/vacuum-*` |
| A table deleted from the front never gave a page back; ten rounds took the file from 1 MB to 11 MB for 100 rows | upstream | the merge of an emptied leaf was refused because its sibling was full, and then packed at the fill a compaction leaves room for | an emptied leaf merges whatever its sibling holds, at the tight fill; the freed page is installed as the empty leaf its record names | `engine::space_reuse` |
| A key's `CASCADE` ran after the parent's `AFTER` triggers; a `DELETE`'s `RETURNING` was computed before the cascade | ORM | key triggers were appended after the user's | row changing key actions run first, then `RETURNING`, then the user's triggers | corpus `engine/fk-action-order-*` |
| `backup` onto its own database destroyed it | CLI | `std::fs::copy` of a file onto itself truncates it | refused; a backup is written beside the destination, checked, the replaced database's log removed, and renamed | `e2e::cli_round_trips` |
| FTS5: integer `bm25` weights ignored; prefix query after a write in the transaction failed; `SET rowid` wrote the first column; a failed multi row insert kept rows; `rebuild` added to the totals; `snippet` window and column; `rank MATCH`; integrity check of external content; a huge token left an unindexed row | search | see the FTS5 fixer's commit | each fixed; tokens are cut at 1,000 bytes | corpus `fts5/*`, `engine::fts5_bulk_edits` |
| FTS3 and FTS4: `docid` writes ignored; `content=` read nothing before `rebuild`; default `snippet` column; `notindexed` searchable; `merge=` refused | ORM | | each fixed | corpus `fts4/*` |
| The `ascii` tokenizer split at non ASCII bytes; the `porter` stemmer stemmed differently from SQLite | search | the stemmer was a different Porter variant | a port of SQLite's `fts5_tokenize.c` stemmer, matching on 30,000 generated words | corpus `fts4/*` |
| A nearest neighbour query on an empty index failed; a failed multi row insert into `inillucent_search` kept rows; `inillucent_search` read FTS5 operators as words | search | | no rows; atomic; the operators are refused as unsupported and the docs say so | `engine::vector_edges` |
| `GLOB` classes with non ASCII; integral text above 2^51 stored as REAL; a key to a TEXT parent; `IS` affinity; STRICT `ADD COLUMN` types; `DETACH` in a transaction; `IS NULL` precedence; upsert clause order; `HAVING` overflow; `UPDATE OR REPLACE` row order; `log`; `strftime`; `unistr`; window frames; JSON5; `printf`; parser leniency; nesting depth | data, upstream, ORM | | each matches SQLite 3.53.4 | corpus `sql/*`, 74 cases |
| An automatic rowid after a named rowid 0, or after deleting the highest row, inside a transaction, overwrote a row or skipped a value | FTS5 fixer | | fixed | corpus `fts5/*rowid*` |
| `VACUUM` in one process lost rows another process inserted beside it: 265 of 502 acknowledged inserts, with no error and a clean `integrity_check` | crash | `VACUUM` writes a new file and renames it over the database. It closed the old connection, which let go of the write lock, before the rename, and a writer that took the lock in that moment committed to a file the rename then put out of reach. Every process that kept the old file open also went on using it | the rename comes before the old connection closes, so the lock is held across it, and `VACUUM` takes EXCLUSIVE and removes the old log before the rename. Every connection that takes a lock, and every open once it holds SHARED, compares the number the file system gives its path with the one it read at open, and reopens when they differ (`Vfs::file_number_by_name`, one `GetFileInformationByName` call). A backup over a database another process has open holds that database's RESERVED lock across its rename | `durability::process_maintenance::vacuum_in_another_process_loses_no_acknowledged_insert`, red 143 of 361 rows; `rebuild::vacuum_crash::a_crash_after_the_early_segment_removal_recovers_the_original`; the crash hunter's `VACUUM` scenarios, see section 5 |
| After a process was killed during a write, the processes opening the file together could lose acknowledged commits, and failed with "replaying InsertRow ... found no room in leaf" in the meantime | crash | the open that won the lock put back the journal the killed process left. Every other open asked for RESERVED once, was refused, took that as "somebody else will deal with it", and read pages the journal had not been put back over | an open waits, holding nothing, while a journal that still has pages to put back is beside the file and another process holds the lock | `inillucent-pool` `journal::tests::a_replay_waits_for_another_handle_beside_a_hot_journal`; six writers on a crash snapshot failed 15 rounds of 20 before, 0 of 15 after |
| `SELECT DISTINCT r` over an index filled by many processes failed with "database disk image is malformed", or returned 30 of 40 values | crash | the skip scan bounded itself by the catalog's stored row count, an estimate that lags rows in leaves' delta areas, and used a position in a leaf's packed rows as an index into its merged rows | the bound is the file's size; a seek that lands on a leaf with writes starts at its first row and skips the run of the last value | corpus `engine/distinct-over-index-filled-by-many-processes` |
| A process that ran one statement and then a deferred `BEGIN`, such as a process per request setting `busy_timeout`, waited out the busy timeout beside another doing the same: 16 to 31 of 60 transactions finished, the median in 5.4 s | crash | a statement that only reads keeps SHARED for a moment, and the transaction began inside that moment, so it looked like one that had read first | a deferred `BEGIN` lets go of a kept SHARED lock | `durability::process_deferred`, red: one transaction waited 30 s |
| `backup`, `VACUUM INTO` and `PRAGMA wal_checkpoint` beside another process's read transaction waited the busy timeout and failed; a process opening beside another's transaction waited 5 to 10 s before its first read; `PRAGMA user_version` asked for the write lock | two processes | each began with a checkpoint or a write the open made on its own account, which needs the file to itself | the copies take the log with the file under SHARED; `wal_checkpoint` answers its busy row; the open's own writes are tried for 100 ms and left to a later statement; pragmas that only read take SHARED | `durability::process_maintenance` |
| With a checkpointer beside four writers and processes killed at random, acknowledged commits went missing in about one run in 15 | crash | a connection asked whether another process had committed by the length of the log segment it had open. A roll leaves that length as it was, so the connection checkpointed from pages without the commits made in the next segment and moved the recovery point past them | the question also asks whether the next segment exists (`Wal::next_segment_exists`) | `inillucent-wal` `log::a_roll_by_another_writer_shows_as_the_next_segment`; the crash hunter's checkpoint scenario, see section 5 |
| The crash suites failed with "database schema has changed" after the schema retry above was added | lead | after a crash every statement found the file moved and reloaded the catalog, and every reload moved the generation, so all fifty retries failed | a reload that reads the same catalog rows keeps the generation | `durability::free_map_checkpoint_crash` and four other crash suites |
| An indexed or key value over about 32 KB was refused, and keys around 17 KB could fail to split | schema survey | a key column was never stored out of line, so a key over about 32 KB had no page to live on, and a split priced the left half and never the right, so two keys of about 17 KB on the right half overflowed the encoder | a key over `page_size / 8` is stored on pages of its own with a 512 byte prefix in the leaf; a long separator is shortened, and stored out of line when it cannot be; the split prices both halves; a file moves to format 3 the first time it stores one; the longest key is 1,048,576 bytes | `differential::new_engine_long_keys`, `engine::long_keys_at_every_page_size`, `durability::long_key_crash` |
| `PRAGMA auto_vacuum` FULL and INCREMENTAL changed nothing, `PRAGMA incremental_vacuum` rebuilt the whole file, and `PRAGMA max_page_count` was not enforced | features | the free map only grew the file; no step moved a page to a lower number; a delete left the out of line values of its rows allocated until the row was reused | a shrink step moves the last page into the lowest free page and cuts the file at a checkpoint, roots are kept at the start of the file, `incremental_vacuum(N)` runs N steps, a delete frees the out of line values of the rows it deletes, and `max_page_count` refuses the statement that needs a page past it | `engine::auto_vacuum`, `durability::auto_vacuum_crash`, `inillucent-tree::relocate`, corpus `usage/auto-vacuum` |
| At 4,096 byte pages, a table trimmed from the front kept about 6 pages a round: the last emptied leaf under each parent, and the parent | upstream | an emptied leaf could only merge with a sibling under the same parent, so a parent left with one child kept it, and a parent was never freed | a parent left with one child hands it to the neighbouring parent and is freed, up to the level below the root (`crates/inillucent-tree/src/write/dissolve.rs`, `tasks/task-2225-tdd.md`) | `engine::space_reuse` at 4,096 byte pages, flat over 40 rounds; `inillucent-tree` `write_campaign`; `durability::new_engine_recovery_shapes::a_trim_that_dissolved_a_parent_replays` |
| A file made at a page size other than the default, then opened through the driver, kept the requested size in `Storage::page_size` | lead | the open took the size from the connection settings | the open takes the size from the file | `engine::page_sizes` |
| FTS3 and FTS4 `snippet` placed its window differently from SQLite for long text | search | the FTS5 window choice was used | a port of SQLite's fragment selection, which joins up to four fragments (`fts3_snippet.rs`) | corpus `fts4/snippet_*`, 10 cases |
| `DETACH` of an earlier database after a later one was written in the same transaction was refused | data | every record of the transaction names its database by position, and the positions move | the detach renumbers the transaction's records and every later database's schema | corpus `engine/detach-of-an-earlier-database-*`, 3 cases |
| With 16 processes each running `BEGIN IMMEDIATE` for 15 s, 2 transactions waited out the 5 s busy timeout, where SQLite's 16 did not | two processes | a wait for the write lock slept 2, 4 and then 8 ms, so a long waiter tried less often than a client that had just let go | after 15 tries a waiter sleeps 2 or 3 ms, alternating (`inillucent_pool::busy`). The longest wait went from 3.6 to 4.9 s to 0.67 to 0.85 s, and commits from about 1,200 to about 2,400 | `inillucent-pool` `a_long_wait_for_a_lock_keeps_trying_every_few_milliseconds` |
| A new database created over the log files of a deleted one | lead | not reproduced: the new database overwrites the segment numbers it reaches | a test holds it | `durability::process_create` |

### 3.2 The command line

| Defect | Fix | Test |
|---|---|---|
| `import` kept a UTF-8 byte order mark in the first header; `--params-file` refused one | dropped | corpus `cli/import-*`, `e2e::cli_round_trips` |
| `import` aborted on a short or long row, a duplicate header and a rejected row, and numbered rows from 0 | as `sqlite3`: pad, truncate, rename `a_01`, keep going, number lines from 1 | corpus `cli/*` |
| `batch` refused a script with its own `BEGIN` and `COMMIT`, such as `.dump` output | such a script runs as written and is rolled back on failure | `e2e::cli_round_trips` |
| `export <table> --format insert` wrote `INSERT INTO tab` | names the table | `e2e::cli_round_trips` |
| `CREATE TABLE IF NOT EXISTS sqlite_sequence` under `writable_schema` was refused, so reloading a dump with `AUTOINCREMENT` printed an error | the reserved name check is skipped while `writable_schema` is on, as `sqlite3CheckObjectName` does | corpus `engine/dump-reload-*` |
| statuses named the wrong cause (`syntax` for a directory, `corrupt` for a missing folder, `io` for a busy backup); usage errors were not JSON under `--output json` | the status names the cause; every failure is JSON | `e2e::cli_round_trips` |
| tabs format, markdown, HTML and JSON output differences; `.dump --nosys`, `.schema --indent`; `.parameter set` of a blob | as `sqlite3` | corpus `cli/*` |
| 110 messages and help texts held a run of spaces where a line was joined without its `\` | rejoined | `tooling::string_literals` |

### 3.3 Speed

Measured with the release build and the pinned `sqlite3` through each shell, as the time of the
statements less the time of their setup, the median of three runs
(`_agent_output/task-2217/lead/speed.mjs`, and the crash hunter's `vactime.mjs` for `VACUUM`).
"Before" is what the hunter that found the case measured.

| Case | Before | After | SQLite |
|---|---|---|---|
| `VACUUM` of 20,000 rows with two indexes | 72 s | under 0.5 s | 0.05 s |
| `VACUUM` of 10,000 rows with a quarter deleted | 30.6 s | 78 ms | 43 ms |
| `VACUUM` of 50,000 rows with a quarter deleted | 168 s | 201 ms | 159 ms |
| FTS5 `UPDATE` of 8,000 rows | 7.7 s | 1.8 s | 0.04 s |
| 40,000 upserts onto 1,000 keys | 952 ms | 75 ms | 11 ms |
| A cascading delete of 100,000 children scattered over 1,000 parents | 8,569 merge attempts for 50,000 deletes | 164 ms | 53 ms |
| 1,600 `CREATE TABLE` in one transaction | 6.8 s | 120 ms | 80 ms |
| A join with a `WHERE` term on one table, 2,000 rows each | the term tested once per pair of rows | 6 ms | 2 ms |
| A correlated `EXISTS` over an unindexed column, 10,000 rows in each table | 1.219 s | 5 ms | 5 ms |
| A correlated `NOT EXISTS` over an unindexed column, 10,000 rows in each table | 1.226 s | 5 ms | 940 ms |
| A correlated `count(*)` in a result column over an unindexed column, 10,000 rows in each table | 2.185 s | 7 ms | 1.73 s |
| A join with a condition on the inner table beside the key, 10,000 rows in each table | 2.836 s | 6 ms | 2.9 ms |
| A join keyed on `a.k = b.k + 1`, 10,000 rows in each table | 2.207 s | 5 ms | 3.2 ms |
| A join keyed on `a.k = b.k` with `a.id < b.id`, 10,000 rows in each table | 2.001 s | 7 ms | 5.4 ms |
| 20,000 `INSERT` statements on one line, each its own commit | each statement copied the rest of the line | 24.8 s | 144 s |

The last case is mostly commits: every statement syncs, and SQLite's rollback journal syncs more
often. What it shows is that the line is no longer copied once per statement.

The fixes behind the rows: an upsert or replace onto a key in the same leaf no longer copies the
leaf's delta area; a leaf that failed to merge does not try again until it has lost an eighth of its
rows; `CREATE TABLE` syncs once per commit and no longer parses every table already in the schema;
a `WHERE` term on one table filters before the join; the shell parses a line without copying its
rest; a correlated subquery whose table has no index for its key reads the table once into a hash
table, and a join keeps the rest of its condition beside the key it hashes on and tests it for each
pair (`tasks/task-2223-tdd.md` has the design). `engine::write_speed` and
`engine::automatic_index_speed` hold each of them under a limit about ten times what the fixed code
takes and several times less than the old code took.

## 4. Found and not fixed

| Item | Why |
|---|---|
| The ordinary checkpoint syncs its pages and its meta record together, so a power loss inside that sync can leave a torn catalog page the log no longer covers | found by the auto_vacuum crash test; the checkpoint after a shrink syncs twice, the ordinary one does not, because the cost of the second sync has to be measured first. Filed as a high priority follow up |
| An owner key longer than `page_size / 4`, and a separator stored out of line, keep their extent pages pinned, so `auto_vacuum` cannot move those pages | a moved extent page has to find its owner, and these owners are not written into the page |
| SQLite rolls back the whole transaction when a one row statement needs a page past `max_page_count`; this engine rolls back only the statement | the statement rollback is what SQLite does for a multi row statement, and it loses nothing the caller did not ask to lose |

## 5. Results

**The test suite.** `inillucent-testrun --changed origin/main --strict` selects all 403 targets,
because the change touches files no rule narrows. The final run passed all 4,867 tests, after the fixes for the items section 4 used to list, and
`cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean. The run before it
failed one: `durability::process_create`, which failed 3 of 8 runs at the commit this session
started from as well, and was fixed (section 3.1): 0 of 22 after.
The suites that need something this machine does not have were declared absent: MySQL, PostgreSQL,
Go, the Nikaya checkout beside the repository, and the network. `inillucent-remote::lib` was then
run with `INILLUCENT_NETWORK_TESTS` set and passed.

**The usage corpus.** `differential::usage_corpus` grades every case in
`compat/corpus/usage/bug-hunt-8-*.sql` against SQLite 3.53.4, including the cases that reopen the
database part way through. All pass.

**The crash hunter's harness, rerun on the final release build.** Each run starts 3 to 8 processes on
one file, kills them at random, and checks every acknowledged commit, every group of rows written
together, every version after an update, `integrity_check`, and the FTS5 and vector indexes.

| Scenario | Before | After |
|---|---|---|
| `VACUUM` beside three writers, processes killed at random | 20 of 30 lost commits | 0 of 30 |
| `VACUUM` beside three writers, nothing killed | 5 of 5 lost commits | 0 of 30 |
| `VACUUM` in WAL mode | 17 of 30 | 0 of 20 |
| A checkpointer beside four writers, processes killed at random | 4 of 60 | 0 of 300 |
| Six writers to a table and an FTS5 index, every process killed at once | 7 of 40 | 0 of 40 |
| Backups during writes | passing | 0 of 20 |
| Everything at once: writers, checkpoints, `VACUUM`, backups, FTS5 | not run | 0 of 20 |
| Four processes per request, each `PRAGMA busy_timeout` then `BEGIN; INSERT; COMMIT` | 16 to 31 of 60 finished, median 5.4 s | 48 of 48, 1.3 s for all |

SQLite ran 620 of the same iterations with no loss.

**Two processes taking turns.** The hunter's busy timeout matrix, 8, 12 and 16 clients for 15 s each,
ran to the end on the build from partway through this session. No client hung, every integrity
check passed, and inillucent finished more operations than SQLite in most cells, with a lower 99th
percentile wait. Deferred transactions that read and then write were refused with `busy` in both
engines, as SQLite's `SQLITE_BUSY_SNAPSHOT` is. One difference is open: with 16 clients each running
`BEGIN IMMEDIATE`, two of inillucent's transactions waited out the 5 s busy timeout, where SQLite's
did not.

**How the losses were found.** The `VACUUM` and checkpoint losses did not show in any single run of a
test. They were found by a temporary trace each process appended to, lined up with the
acknowledgements the harness recorded, and in two cases a backtrace at the moment a log segment was
truncated. The probes are not in the commit; `_agent_output/task-2217/agents/crash/run.mjs` keeps
the `TRACE`, `SNAP`, `NOKILL` and `NOVACKILL` switches that drove them.
