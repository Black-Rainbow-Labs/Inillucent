# Bug hunt 10: writes at random, internal errors, old files, the documentation, and threads

## Terms used in this document

| Term | Meaning |
|---|---|
| case | a short SQL script run against a fresh database through both the pinned `sqlite3` and `inillucent-shell`, whose outputs are compared byte for byte |
| probe | `tools/bug-hunt/probe.mjs`, which runs case files and reports every difference, crash, hang and slow case |
| hunter | a sub agent given one area of real use and asked to find defects in it, not to fix them |
| state comparison | after a random write workload, every table is read in rowid or key order through both engines and the two dumps are compared, together with `changes()` and `total_changes()` after each statement |
| internal error | a message that names the engine's own structures, such as "no layout imported", "does not carry column", "page is not a blob extent", or any panic, abort or `SQL logic error` that SQLite does not give for the same statement |
| red, green | the test run before the fix, which must fail, and after it, which must pass |

## 1. Why

The ticket asks for as many defects as possible to be found before the public release, for tests
that cover real use from end to end, and for edge cases with wrong answers or slow paths. It asks
for the issue trackers of Turso, FrankenSQLite and SQLite to be read for cases a second
implementation of the dialect may share.

Hunt 9 added a random query generator, and it found more than any hand written area: a `WITHOUT
ROWID` table whose `DELETE` did nothing, a planner that compared record slots with declared columns,
and seven more. That generator only wrote reads. Its writes were the `INSERT` statements that filled
the tables. The lead's seeded write workloads in hunt 9 found the `DESC` index corruption, which
had been in the engine since a hill climb and which no earlier hunt saw, because nothing wrote to a
multi leaf index in a random order and then checked it.

So the largest gap is random writes: `UPDATE`, `DELETE`, upsert, `REPLACE`, triggers, foreign key
actions and savepoints, checked by reading every table back. The other gaps are areas no hunt has
been given at all:

- no hunt opened a file that an older release wrote and then changed it;
- no hunt ran the examples in the documentation, the skills and the package readmes as written, and
  a reader copies those first;
- no hunt drove one file from many threads in one process, through the Rust driver, Python and Node;
- write speed and very large files were measured only through the gates, which run fixed workloads;
- `ALTER TABLE` with views, triggers and indexes that name the altered column was covered only by
  the shapes ORMs emit;
- the random query generator compared results, so a statement that both engines refused was never
  looked at, and an internal error that is refused rather than wrong was never counted.

## 2. Areas

Eight hunters run at once against one release build. Each works in its own folder under
`_agent_output/task-2264/agents/<area>/` and prefixes its database names with the area, because
sub agents share a scratch folder (see `.claude/repo-plan.md`). Each reads the known bugs in
task-2110 and task-2111 first and does not report them again.

| Hunter | Area | What it does |
|---|---|---|
| writes | random write workloads | a generator of random schemas (rowid, `WITHOUT ROWID`, `STRICT`, every index shape including `DESC`, partial and expression indexes, `UNIQUE`, foreign keys with every action, triggers) and random write statements (`INSERT`, `INSERT OR *`, upsert, `REPLACE`, `UPDATE`, `UPDATE ... FROM`, `DELETE`, `RETURNING`, savepoints, rollbacks, reopens), compared with SQLite by state comparison and `integrity_check` after every block |
| internal | internal errors and crashes | a grammar based generator and a mutator of the existing corpus that looks only for panics, aborts, hangs, internal errors and messages SQLite does not give, in statements both engines may refuse |
| upstream | other engines' trackers | Turso and Limbo issues not already written as cases in hunt 9, FrankenSQLite, libSQL, the SQLite forum and timeline for 2025 and 2026, written as cases and run through the probe |
| files | old files and migration | databases written by every published release in `tests/interop/`, opened by this build, written to, checked and reopened; real SQLite databases (Chinook, Northwind, a large public dataset) through `inillucent migrate` and back through `.dump`; files in WAL mode with an open log |
| docs | the documentation as written | every SQL and shell example in `README.md`, `docs/`, `agent-skills/`, `drivers/README.md`, `examples/` and the package readmes, run exactly as written, the output compared with what the page says |
| threads | one file, many threads | the Rust driver, Python and Node with 2 to 32 threads and as many connections on one file: readers during writes, `busy_timeout`, checkpoints with readers open, search indexes written from several threads, handles closed while another thread steps |
| speed | write speed and large files | bulk loads with several indexes, many small commits, delete and reinsert churn, a database larger than the buffer pool, file size against SQLite after each workload, reopen time, `VACUUM`, and memory, each against SQLite |
| schema | schema changes | `ALTER TABLE RENAME`, `RENAME COLUMN`, `ADD COLUMN`, `DROP COLUMN` with views, triggers, indexes, generated columns, `CHECK` and foreign keys that name what changes; the twelve step table rebuild SQLite documents; `legacy_alter_table`; reopen after each |

## 3. Method

1. Each hunter writes cases in its area, runs them, and reports a minimal repro for every
   difference, crash, hang and slow case, with what SQLite does, in
   `_agent_output/task-2264/agents/<area>/report.md`.
2. I confirm each report, drop the ones that are deliberate deviations listed in
   `compat/corpus/usage/known.toml` or `docs/sql.md` or already filed in task-2110 and task-2111,
   and group the rest.
3. Each defect gets a test that fails before the fix. A SQL difference becomes a case in
   `compat/corpus/usage/bug-hunt-10-<area>.sql`, graded by `differential::usage_corpus`. Anything
   SQLite does not have, or a timing, becomes a Rust test in the tier the testing standard names.
4. The fix, then the test green, then `inillucent-testrun --changed --strict`.

Fixes were made by the lead and by sub agents ("fixers"), each in its own git worktree with the
brief in `_agent_output/task-2264/FIXER-BRIEF.md`. The lead reviewed each fixer's commit, picked it
onto this branch, and graded every corpus file again on the merged code.

The lead also probed by hand: `ATTACH`, temp schemas and `VACUUM INTO` (`lead/attach-*.sql`),
rollback of every schema change (`lead/rollback-*.sql`, `lead/dml-then-ddl-rollback.sql`), cyclic
foreign keys (`lead/cyclic-settle.sql`), encryption (`lead/enc.mjs`), and two random generators of
transactions that mix DML and DDL and end in `ROLLBACK`, `ROLLBACK TO` or `COMMIT`
(`lead/gentx.mjs`, `lead/gentx2.mjs`), graded against SQLite after a reopen. Of 300 seeds of the
first generator, 111 differed on the build this ticket started from.

## 4. Defects found and fixed

### 4.1 Lost data and wrong answers in transactions and attached databases (lead)

| Defect | Cause | Fix | Test |
|---|---|---|---|
| With foreign keys on, `BEGIN; DROP TABLE p; ROLLBACK` brought `p` back with no rows | `DROP TABLE` first deletes every row. Those undo records named the tree the drop released, and `undo_to_floor` skipped records whose tree was not attached | after a run of catalog undo records, the trees are matched to the restored catalog before the next data record (`match_trees_to_the_restored_catalog`) | corpus `engine/fk-drop-*` |
| `BEGIN; INSERT ...; ALTER TABLE t DROP COLUMN c; ROLLBACK` (or `ADD COLUMN`) kept the insert and lost rows the transaction deleted; `integrity_check` reported a page never used | the rebuild registers a new tree under the same handle, so older undo records went into the new tree | the same resync | corpus `engine/insert-then-*-rollback`, `engine/many-rows-then-drop-column-rollback`, 12 random transaction cases |
| A foreign key in an attached or temp table looked up its parent in `main`: every write to the child failed with `no such table: main.p`, and a `main` table of the same name was checked instead | keys were planned for all databases as one schema under `main` | `foreign_key::plan_databases` plans each database on its own tables; the commit check and the cyclic sweep match the parent in the child's database | corpus `engine/attached-foreign-key-*`, `engine/temp-foreign-key` |
| An unqualified table name in a trigger in an attached database read and wrote `main` | trigger bodies were bound with the usual search order | a trigger that is not temporary binds in its own database, as a view does (`TriggerInfo::temporary`) | corpus `engine/attached-trigger-body-*` |
| `AUTOINCREMENT` counters of different databases were kept in one `sqlite_sequence`, and `main.sqlite_sequence` was never made | the binder and the DDL found `sqlite_sequence` without naming a database | each names the table's database | corpus `engine/sequence-*` |
| `DROP TABLE` of an `AUTOINCREMENT` table in an attached database broke its `sqlite_sequence`: `read 0 of 32768 bytes` | the row was deleted through `main`'s file | the table's own file | corpus `engine/attached-drop-autoincrement-table` |
| `BEGIN; DROP TABLE t; ROLLBACK` lost `t`'s `sqlite_sequence` row, so the table reused ids | that delete recorded no undo | it records a before image | corpus `engine/rollback-*-keeps-sequence` |
| After a rolled back `DROP` of an FTS5 table every statement on it failed with "no module is connected"; after a rolled back `DROP` of a table with an `inillucent_hnsw` index, rows written later never reached the index | the rollback restored catalog rows, not the connection's module map | `connect_missing` during the rollback, before the tables are rebuilt | `engine::rollback_restores_modules` |
| With a cyclic foreign key with an action anywhere in the schema, a failed statement on a table with a trigger kept its rows (the shipped `todo-mvc` example's own test failed) | the statement's key check ran a query that settled cyclic keys with autocommit writes, which committed the statement before the check failed | settle only for the outermost statement | corpus `engine/cyclic-key-*` |
| `DROP TRIGGER`, `DROP VIEW` and `ALTER TABLE RENAME` of an attached file inside `BEGIN` were lost when the file was opened again | deleting or rewriting a catalog row did not add the database to the transaction's written set | `forget` and `rewrite` add it, as `record` does | corpus `engine/attached-*-in-transaction-survives-reopen` |
| `DROP TRIGGER` of a temporary trigger on a `main` table reported success and the trigger kept firing | the trigger was looked for in its table's database | a temporary trigger is in `temp` | corpus `engine/drop-temp-trigger-*` |
| After another connection's `VACUUM`, a connection inside `BEGIN` could not run any statement, and its `ROLLBACK` was refused too | a replaced file was refused inside every transaction | a transaction that holds no lock reopens the file and goes on | `durability::two_connections_end_a_transaction` |
| `ROLLBACK`, and `PRAGMA busy_timeout`, failed `busy` while another connection held the file to write it; the transaction stayed open and the statements after it were lost at close | they asked for the read lock | `ROLLBACK`, a `COMMIT` that wrote nothing and connection pragmas take no lock | `durability::two_connections_end_a_transaction` |
| Smaller: `ALTER TABLE m.t` on a missing table, and `DROP TRIGGER temp.tr`, did not name the database; the shell printed why an encrypted file did not open twice | | | corpus, `e2e::shell_command_line` |

### 4.2 Fixed by the fixers

Each row names the fixer's area and its corpus file, `compat/corpus/usage/bug-hunt-10-<area>.sql`.

| Area | Defects fixed |
|---|---|
| alter | `RENAME TO` rewrote a CTE of the same name; `RENAME COLUMN` rewrote an `ORDER BY` result alias, so a view returned rows in the wrong order; `ADD COLUMN c AS (c)` overflowed the stack (now `generated column loop on "c"`); `ADD COLUMN` generated expressions were not validated; `RENAME COLUMN` of a `USING` column, upsert targets in triggers, temp views and triggers over `main.t` when a temp table shadows it, temp views over main views; `DROP COLUMN` ignored temp views and triggers; `legacy_alter_table` with a partial index; trigger on `sqlite_schema`; messages; a trailing comment kept in stored SQL; `ADD CONSTRAINT CHECK` with NULL |
| files | `.dump` panicked on a table name with a multibyte character; a SQLite dump with an `AUTOINCREMENT` table or an FTS5 table loads (SQLite's dump header rule, and the FTS5 shadow rows become the table's content); rows of up to about 1,920 integer columns in `migrate`, `ADD COLUMN`, `DROP COLUMN` and `UPDATE`; `migrate` of FTS5 `UNINDEXED`; `VACUUM` of a 0.1.1 file with FTS5 |
| drop | FTS5, `inillucent_search`, vector index and rtree shadow tables cannot be dropped, altered or given triggers on a defensive connection; `DROP VIEW` drops its triggers and `DROP TABLE` the temp triggers on it; a deferred foreign key is checked at `COMMIT` after `DROP TABLE` of the parent; `RENAME` of a table with a vector index carries the index; `ALTER TABLE ... RENAME` of FTS5 and `inillucent_search` tables; `schema_version` restored by `ROLLBACK`; `table_info` of table valued functions |
| sql | recursive CTE comparisons use the column's affinity and its `UNION` the column's collation; a bare column beside `max()` under `GROUP BY` comes from the max row; an uncorrelated subquery's error is raised only if its value is used; compound `ORDER BY` terms matching an arm's expression; `NULL ON CONFLICT`; partial index implication for `INDEXED BY` and plans; empty quoted type names; `json_error_position`; `changes()` with `RAISE(FAIL)`; `CREATE TEMP INDEX`; a `LIMIT` subquery keeps the rows it would return alone |
| memory | a write transaction kept one undo record per changed row in memory; past 16 MiB the oldest go to a temporary file (`engine/undo_spill.rs`), read only by a rollback. `UPDATE` of every row of a 564 MB table: 825 MB peak to 219 MB; 1,000,000 single row inserts with 2 indexes: 536 MB to 151 MB; `VACUUM` of the same file 839 MB to 289 MB, reading tables a page at a time (`engine::transaction_memory`) |
| vacuum | `VACUUM aux` wrote `main`'s statistics into `aux`'s catalog and `VACUUM aux INTO` copied `main`; JSON function errors kept their message on the command line and over MCP; FTS5 `merge` with no rank; `integrity_check` inside a transaction counts pages waiting for the commit; a datatype mismatch under `OR FAIL`, `IGNORE`, `REPLACE` and `DO NOTHING` keeps earlier rows inside a transaction; a duplicate `UNIQUE` and primary key make one index on a rowid table; a foreign key to a missing parent table; `--readonly` refuses `.shell` and `.system`; four refusals SQLite does not make |
| size | appended rows pack a leaf again before splitting it, so a table loaded with 500 row `VALUES` statements takes 11 pages where it took 25 (`engine::append_load_density`); an older copy of the file beside the newer file's log opens and sets the log aside (`durability::restored_copy_under_newer_log`) |
| writes | a duplicate key insert leaked the pages of a long value; `INSERT OR FAIL` kept a row breaking a foreign key; `OR REPLACE` left a stored generated column NULL; `excluded.col` had the column's affinity; a WITHOUT ROWID `UNIQUE` on a `DESC` key column ("no layout imported for root page 0"); key errors reported before `NOT NULL`, `UNIQUE` and `CHECK`, and `OR IGNORE` raising them; `UPDATE` and `DELETE` with no `WHERE` visit rows in rowid order; change counts with `RAISE(FAIL)`; `RETURNING` rows before a failure; trigger `new.col` affinity; `STRICT` checked before `BEFORE` triggers; a WITHOUT ROWID table with a trigger allocated a rowid |
| query | an `IN` subquery correlated two levels out failed (540 of 3,432 generated queries); a compound arm resolved a column in the first arm's `FROM`; a recursive CTE on the right of a `LEFT JOIN` looped forever; string results over the length limit; `IN` over a table valued function was quadratic (6.2 s to 0.09 s); correlated subqueries in an `ON` before a `FULL` or `RIGHT` join and in a derived table inside a subquery; aggregates with outer references in `FILTER`; JSON5 `inf` and `nan`; `unhex`, `unistr`, `json_patch`, `json_array_insert`; `RAISE` in `RETURNING`; expression depth messages |
| cli | `query` reports `syntax` for a statement that does not parse; `PRAGMA <schema>.encryption`; zero argument `strftime()`; shell `.header`, `.mode --quote`, `argv[N]:` errors, `.open --readonly`, `.trace`, `.archive`, `.connection`; the Node addon route reports `feature`; three refusal messages; `k` over 1,000,000 refused; 15 page corrections |

### 4.3 Merging

Three files had two fixers' fixes of the same defect, made two ways, and were merged by hand:
`catalog/load.rs` (duplicate key indexes: one rule, sort direction ignored for every table),
`engine/pages.rs` (`integrity_check` inside a transaction: one way of counting pages waiting for the
commit) and `exec/dml/insert.rs` (counting a row whose `AFTER INSERT` trigger fails). `rebuild.rs`
had the memory fixer's paged copy and the vacuum fixer's named database, and keeps both.

## 5. Found and not fixed

These are recorded in the known bugs tickets: bugs 51 to 57 of task-2110 (medium) and bugs 47 to
52 of task-2111 (low).

| Item | Why it is left |
|---|---|
| `CREATE INDEX` holds every entry in memory before it sorts them (2,000,000 rows, 211 MB), and the shell's `.import` reads and parses the whole file first (1,000,000 rows, 536 MB) | each needs a sort or a parser that streams; the first is an external merge sort feeding the bulk builder |
| `ADD COLUMN` rewrites every row (800,000 rows: 783 ms, SQLite 19 ms) | a leaf stores one column count for its rows, and reading a row with fewer columns than the table declares fails; changing that touches the write, redo and relocation code of the tree |
| A row of about 1,940 or more integer columns does not fit a 32 KiB page | needs overflow pages for a row or a smaller integer tag, which is a file format change |
| A plain `VACUUM aux` folds the log and does not shrink the file; `VACUUM aux INTO` does | replacing an attached file under another process that has it attached would lose that process's commits, because only `main` checks for a replaced file |
| `PRAGMA integrity_check` does not compare an `inillucent_hnsw` index with its table | it said `ok` with a row missing from the index; the check needs the search crate to count an index's vectors |
| A cascade 290 levels deep completes where SQLite stops with "too many levels of trigger recursion" and keeps every row | the cyclic key settle repeats until nothing changes; no row is wrong, the limit differs |
| `ATTACH` with a file name that is not a literal (`ATTACH ?1`) still exits 3 | it works in the engine, but it is the documented example of `unsupported` in four packages' conformance suites, the capability table and six pages, which move together |
| A chain of about 1,000 CTEs fails with `too many levels of nested SELECT` | a CTE body is bound each time it is referenced; it needs binding once |
| Random write workloads still differ in 42 of 300 seeds after the fixes: `total_changes()` off by one with trigger rows, `INSERT OR ROLLBACK` counts, `LIKE` on a blob in a `NOCASE` column, `STRICT` checked before `UNIQUE` in `INSERT ... SELECT`, a WITHOUT ROWID `DESC` key scanned ascending (`known.toml` N4) | smaller; each class is written up in task-2111 |
| The append packing takes 13% to 16% more processor time on a 500 row `VALUES` load, inside the run to run spread of wall time | the threshold is a choice between density and time; a half costs 3% to 10% and packs less |
| A write transaction still collects every key an `UPDATE` or `DELETE` will change before it runs (about 100 bytes a row) | known bug 45 |
| Smaller: the Node addon and the process route of the npm package; `.scanstats` prints nothing; nesting limits are 999 where SQLite's differ by construct; two differences in the order of errors when a statement has two | low |

## 6. Results

**The strict run.** `inillucent-testrun --changed origin/main --strict` selected 422 targets and
5,099 tests. Four failed, and each was settled:

- `engine::append_load_density`: `VACUUM` of 90,000 appended rows took 28 pages against 23 for a
  bulk build, because the memory fix copies a table a page of rows at a time and so packs as an
  append does. Reading the whole table to bulk build it is what the memory fix removed, so the
  test allows a quarter over the bulk build, with that reason beside the assertion.
- `tooling::crash_reports`: seven crash campaigns covered two fewer cut points. A bisection found
  the change that makes connection pragmas take no lock: the pragmas `open()` runs no longer take a
  lock and read the header, so each campaign lost one `Lock` and one `Read` before its transaction.
  With those two left out, the traced call sequences before and after are the same, so every write,
  sync, truncate and journal delete is still armed. The floors moved by two with the reason beside
  them, and the regenerated schedules show no damage and no lost commit.
- `durability::environment` and `durability::process_deferred`: timing. The first kills a worker
  after a fixed 1.5 s and the second bounds 48 transactions at 20 s; under the run's load each
  missed, and each passed alone (`environment` four times; `process_deferred` measured 20 to 23 s
  with this branch and 28 to 37 s without it, because the change that lets a connection pragma
  take no lock also shortens its workers' turns).

The suites not evidenced are this machine's declared absences (MySQL, PostgreSQL, Go), plus
`inillucent-remote::lib`, which needs `INILLUCENT_NETWORK_TESTS`, and the Nikaya workload freshness
check, which needs the Nikaya checkout beside the main checkout.

**Clippy** with `--all-features -D warnings` is clean. **`node tools/doc-facts/check.mjs`** agrees
with the engine after the feature probe and the register audit were run on the release build, and
**`node tools/doc-style/check.mjs`** reports no problems.

**The usage corpus.** Ten new files hold 285 cases, all graded against SQLite 3.53.4 by
`differential::usage_corpus`, and every one passes: `bug-hunt-10-engine.sql` (63),
`bug-hunt-10-alter.sql` (53), `bug-hunt-10-writes.sql` (41), `bug-hunt-10-vacuum.sql` (37),
`bug-hunt-10-query.sql` (27), `bug-hunt-10-drop.sql` (21), `bug-hunt-10-sql.sql` (18),
`bug-hunt-10-files.sql` (14), `bug-hunt-10-shell.sql` (10) and `bug-hunt-10-cli.sql` (1). Of the 63
lead cases, 55 differ on the build this ticket started from; the others are controls.

**The random transaction generators** (`lead/gentx.mjs`, `lead/gentx2.mjs`) ran 300, 1,000, 1,500
and 800 seeds on successive merged builds. The last 800 seeds after every fix differed in one case,
which found the savepoint defect fixed last.

**New Rust suites:** `engine::rollback_restores_modules`, `engine::transaction_memory`,
`engine::append_load_density`, `engine::vector_rename`, `durability::two_connections_end_a_transaction`,
`durability::restored_copy_under_newer_log` and `e2e::sqlite_dump_load`, plus new tests in
`e2e::shell_command_line`, `e2e::cli_commands`, `e2e::mcp_per_call`, `engine::encryption_at_rest`,
`engine::new_engine_vtab_stream`, `migrate_sqlite` and `release_format`.

**What the hunters ran.** About 3,000 random write workloads and 120 killed writers; about 30,000
generated or mutated statements for internal errors, 388 command lines and 95 MCP scenarios; 379
upstream cases (about 4,700 statements) and 900 more single statements; 43 release fixtures and 3
public databases migrated and compared; about 400 documentation examples and the 416 case feature
probe; thread and process stress through Rust, Python and Node; about 560 timed runs on files up to
3.5 GB; and 1,910 schema change cases.
