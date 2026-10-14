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

## 4. Defects found and fixed

Filled in as the work is done.

## 5. Found and not fixed

Filled in as the work is done.

## 6. Results

Filled in as the work is done.
