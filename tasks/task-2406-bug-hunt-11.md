# Bug hunt 11: applications, old files, JSON, windows, dates, full text search, other projects' bugs and pragmas

## Terms used in this document

| Term | Meaning |
|---|---|
| case | a short SQL script run against a fresh database through both the pinned `sqlite3` and `inillucent-shell`, whose outputs are compared byte for byte |
| probe | `tools/bug-hunt/probe.mjs`, which runs case files and reports every difference, crash, hang and slow case |
| hunter | a sub agent given one area of real use and asked to find defects in it, not to fix them |
| fixer | a sub agent given a list of defects from the hunters' reports, working in its own git worktree, which fixes each with a test that fails before the fix |
| lane | one fixer's list of defects, its worktree and its build folder |
| red, green | the test run before the fix, which must fail, and after it, which must pass |

## 1. Why

The ticket asks for another bug hunt that tests real use, finds defects and fixes them, and for the
bugs other projects have found to be read for ones that affect inillucent.

Hunts 8 to 10 covered messy data, speed, two processes, crashes, search, random queries, bindings at
their edges, MCP and JSON output, limits, planner speed, the shell, random writes, internal errors,
old files with `migrate`, the documentation examples, threads, write speed, large files and schema
changes. This hunt goes to areas no hunt was given, and deeper into two that were:

- whole applications written through the Python and Node packages and the Rust driver, with the
  SQL that ORMs emit for SQLite;
- the stored schema text that older releases and real SQLite write, after task-2110 Bug 58 showed
  that a table whose stored `CREATE TABLE` the current parser refused vanished from the schema
  without a message;
- JSON and JSONB functions, window functions and aggregates, and dates, times, numbers and text
  conversions, each against SQLite with generators large enough to find rare differences;
- FTS5 and R*Tree, which earlier hunts used as search features and did not compare with SQLite
  statement by statement;
- the issue trackers of Turso, libSQL, FrankenSQLite, rqlite, sql.js, `wa-sqlite` and rusqlite, the
  SQLite forum, and SQLite's check ins since the start of August 2026;
- pragmas, the lifecycle commands (`VACUUM`, `ANALYZE`, `REINDEX`, `ATTACH`) and what the C API
  exposes.

## 2. Areas

Eight hunters ran at once against one release build of 2.8.1. Each worked in its own folder under
`_agent_output/task-2406/agents/<area>/` and wrote a report with a minimal repro for every
difference. `_agent_output/task-2406/AREAS.md` has the full brief for each area.

| Hunter | Area | What it ran |
|---|---|---|
| apps | real applications through the language packages | about 7,800 statements through the Python package, 3,000 binds, the Node package's three routes, 9,300 generated queries in the shapes SQLAlchemy, Django, Prisma, Drizzle, Knex, Sequelize, TypeORM, ActiveRecord, Ecto, GORM, Diesel, SQLx and peewee emit, compared with the pinned shell and Python's `sqlite3` |
| olddata | stored schema text and files from older releases | every published release binary from 0.1.1 to 2.8.1 wrote files for 100 scenarios (about 4,600 runs); all 46 interop fixtures were read, written, altered, vacuumed, dumped and reopened; files made by real SQLite with unusual stored DDL were opened and migrated |
| json | JSON and JSONB functions | about 310,000 differential statements and 5,300 probe cases |
| window | window functions and aggregates | about 30,000 generated cases and 450 hand written ones, with timings |
| datetime | `log(B, X)` with a blob or a text that is not a clean number; date modifiers with two signs refused; `subsec` rounds milliseconds; `printf` flags `+` with space, `#` with zero padding, `%p`, and precision on `%q`, `%Q` and `%w` with NULL; `strftime('%J')` of tiny values; `LIKE ... ESCAPE ''` refused with SQLite's message; `utc` and `localtime` on Windows before 2007 and after 2037 use the current rules for the year asked about; `unistr_quote` of a NUL; the shell prints `char(1114111)` as UTF-8 |
| fts | FTS5 and R*Tree | about 9,600 generated FTS5 scenarios (240,000 queries, with a shrinker), 1,000 hand statements, tokenizer sweeps over every code point, 1,350 R*Tree seeds, scale runs |
| upstream | other projects' bugs | about 1,980 cases from real bug reports: 874 Turso issues from 2024-09 to 2026-10-30, 127 from libSQL, FrankenSQLite, rqlite, sql.js, `wa-sqlite` and rusqlite, 207 SQLite check ins since 2026-08-01 and 607 SQLite forum threads, plus 963 test blocks from those check ins run against SQLite's own expected values |
| pragma | `query_only` stops writes to FTS5, FTS4 and R*Tree tables, `VACUUM`, `VACUUM INTO`, `ANALYZE`, `REINDEX`, `user_version` and the rest; `DROP COLUMN` is refused when a double quoted or non ASCII reference to the column remains; `data_version` counts other connections' commits; `schema_version` follows another connection's schema change and is kept per database; a temp trigger on a temp table; `export --format tabs`; `integrity_check` reports rows that break a `CHECK`; column types of `CREATE TABLE ... AS SELECT` over `STRICT` and over pragma functions; `pragma_table_info` with `OR` on its argument; `optimize(0x10002)` creates `sqlite_stat1`; `cache_size` and `synchronous` per database; the `UNIQUE` index of a `WITHOUT ROWID` table; `defer_foreign_keys`; `count_changes`, `full_column_names` and `short_column_names`; `index_xinfo` collation names as written; `table_list(sqlite_master)`; `.backup` of an attached memory database and `.lint fkey-indexes` |

The reports number their defects D1 upwards per area: apps 18, olddata 12, json 22, window 12,
datetime 13, fts 31, upstream 35 and pragma 20, 163 in all. Several items hold more than one
defect of the same kind.

## 3. Method

1. Each hunter wrote cases in its area, ran them through the probe and reported each defect with a
   minimal repro and what SQLite does.
2. The lead read each report, dropped what is a deliberate deviation in
   `compat/corpus/usage/known.toml` or `docs/sql.md`, and grouped the rest into lanes by the code
   they touch (`_agent_output/task-2406/LANES.md`), so that two fixers rarely edit one file.
3. A fixer per lane, at most three at a time, each in `D:/agent-worktrees/task-2406-fix-<lane>`
   with its own build folder, under the brief `_agent_output/task-2406/FIXER-BRIEF.md`. A SQL
   difference became a case in `compat/corpus/usage/bug-hunt-11-<lane>.sql`, graded by
   `differential::usage_corpus`. Anything SQLite does not have, or a timing, became a Rust test in
   the tier the testing standard names.
4. The lead rebased each finished lane onto `task-2406`, recomputed the test counts where two lanes
   changed them, merged it, and ran the tooling suite and the usage corpus on the merged branch
   before the next merge.

The lead fixed Bug 58 itself, and the `CREATE INDEX` defect in an attached database.

## 4. Defects found and fixed

### 4.1 Old files whose stored text the parser refuses (lead)

Releases before 2.1.0 wrote `ALTER TABLE ... ADD COLUMN` by appending the column at the end of the
stored `CREATE TABLE`, after any table constraint. SQLite's grammar puts columns before
constraints, and 2.1.0 onwards refuses that text. The loader dropped any table it could not parse
and went on, so the table and its rows vanished with no message.

| Defect | Fix | Test |
|---|---|---|
| A table that 0.1.1 to 2.0.9 altered after a table constraint was missing from the schema | `alter_text::columns_before_constraints` moves the columns in front of the constraints when the stored text does not parse. The loader, `.dump` and the catalog read the reordered text, and the next reclaiming checkpoint writes it back | `release_format::a_table_that_2_0_2_altered_after_its_table_constraints_reads_and_writes`, over the fixture `tests/interop/alter-after-constraint/` written by 2.0.2 |
| Any table, index or trigger whose stored text failed to parse was skipped without a message | the open fails and names the object: `cannot read table t in the schema: ...` | the same suite |
| `CREATE INDEX ix ON t(...)` with `t` in an attached database made the index in `main` | the index goes to the table's database, and a temp table's index to `temp` | `bug-hunt-11-lead.sql` |

### 4.2 Fixed by the fixers

Each row names the lane and its corpus file, `compat/corpus/usage/bug-hunt-11-<lane>.sql`.

| Lane | Defects fixed |
|---|---|
| datetime | Windows time zone rules for `localtime` and `utc`, from the current rules for the year asked about; `strftime` and `datetime` modifiers, `julianday` and `unixepoch` at their edges; `printf` and `format` widths and precision; text to number conversion at the edges of 64 bit integers and reals; the shell printing U+10FFFF |
| pragma | `query_only` refuses writes through virtual tables, `VACUUM` and the other paths that missed it; `DROP COLUMN` with quoted and non ASCII names in the references; `data_version`; `schema_version` per database; `table_list`, `index_xinfo` and the other listing pragmas on attached and temp databases; messages |
| json | nested `json_each` over a column 14.5 s to 0.2 s; `->>` reuses the parse of its document; subtypes through nested calls; keys that need quoting in `json_object` and the path output of `json_tree`; text that is not UTF-8 |
| window | sliding window aggregates are linear in the frame; `percentile` argument checks; `RANGE` with real offsets; `group_concat` over blobs |
| migrate | rows shorter than the table read their defaults; a `WITHOUT ROWID` index stores its full key; triggers keep their order; the dump keeps comments; an external content FTS5 table is rebuilt from its owner; a zero byte file; the cause of task-2111 Bug 33 in `AttachedSchemas::take`; R*Tree tables are carried |
| apps engine | deferred foreign keys compare against the state at `BEGIN` (`engine/key_baselines.rs`); foreign key actions use the parent's affinity, as SQLite trunk does; `json_each` with a constant document and a path from an earlier table; the other engine items from the apps and upstream reports |
| files | a collated table constraint in a file an older release wrote is found stale and rebuilt by `REINDEX`, including in an attached file (`engine/stale_index.rs`); `VACUUM` keeps non ASCII names and `sqlite_stat1` and removes its temporary file when it fails; a `contentless_delete` FTS5 table reads; view column types and trailing comments match SQLite |
| bindings | a deeply nested statement is refused for want of stack instead of overflowing it (`inillucent-base/src/stack.rs`); the Node addon refuses several statements in one call and accepts a script with its own `BEGIN`; a missing parameter value is refused with `Incorrect number of bindings supplied`; the raw `BLOB` form and its status agree on both routes; a closed Python connection ends its statements |
| FTS5 queries | the query parser is a port of SQLite's grammar: `NOT` takes the phrases after it, nested column filters intersect, the nesting limits and messages are SQLite's; `bm25`, `highlight` and `snippet` count a phrase only where its branch matched, and `NEAR` uses SQLite's sweep; `bm25` over 20,000 rows 40.4 s to 0.1 s (debug build); the length term in C's order; `rowid` and `rank` constraints beside `MATCH`; special queries; comments around module arguments |
| FTS5 tables and R*Tree | `INSERT OR ROLLBACK` no longer empties the table; random rowids past the largest; external content columns read by name; FTS5, R*Tree and geopoly in temp and attached databases; a dump restore keeps the index; `rtree_i32` stores 32 bit integers, and a table 2.8.1 wrote with float cells reads and is converted by its first write (`tests/interop/rtree-i32-float/`); `UPDATE` of an R*Tree id; integer affinity for rowids; `categories`, `tokenchars`, `separators` and the unicode61 classes from SQLite's tables, compared for every code point; `fts5vocab` schema argument; the trigram tokenizer with `LIKE` and `GLOB` narrowing; `content_rowid` naming an ordinary column |
| planner | expression indexes serve ranges, `BETWEEN`, `IN`, `ORDER BY ... LIMIT` and covering reads (200,000 rows, `ORDER BY json_extract(p,'$.k') LIMIT 10` 36 to 65 ms to under 1 ms); `CREATE INDEX`, `CREATE VIEW` and `CREATE TRIGGER` derive only the table they change (800 tables 6.0 s to 0.57 s); a `LIMIT` reaches a recursive CTE through a view; the spill reader reads rows larger than its window; a correlated block answers repeated outer values once (1.85 s to 4 ms); constant `WHERE` terms stop at the first false one; 200 FROM terms and 64 tables in a join are refused with SQLite's messages; wide rows of 450 to 790 columns share leaves |

### 4.3 Merging

Twelve lanes were merged in turn. Each rebase conflicted on the per tier counts in
`tests/inillucent-testing-tdd.md` and the totals in `docs/repository.md`, which were recomputed.
The FTS5 tables lane and the FTS5 queries lane both rewrote `fts5/query.rs` and both added
`fts5/content.rs`; the tables lane rebased its trigram narrowing onto the query lane's constraint
reader. After the merges the module and function length ratchets in `policy.rs` failed four times,
and code was moved to new modules (`bind/small.rs`, `ddl/helpers.rs`, `fts5/content.rs`) instead ofraising a recorded size.

### 4.4 Lanes after the first round

Three more lanes ran after the first twelve were merged, for what the first round left open.

| Lane | Defect | Cause and fix | Test |
|---|---|---|---|
| matrix | after the merges, `matrix` and `matrix_deep` failed on `DELETE` and `UPDATE ... ORDER BY ... LIMIT ... RETURNING`, with `changes()` 2 against 0 | the apps engine lane made the parser accept `RETURNING` after `LIMIT`. The matrix rewrites a limited statement into a `rowid IN (...)` form for the pinned SQLite, which is built without `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`, and refused to rewrite one with `RETURNING`, so SQLite did nothing. The rewrite in `statement_matrix/limited.rs` now carries `RETURNING` | `matrix`, `matrix_deep`, `limited::returning_follows_the_rewritten_statement` |
| matrix | a nested aggregate (upstream D8) failed with the internal error `does not carry` where SQLite answers 1 | the aggregate belongs to the outermost query and sits in an unread column of a `UNION ALL` subquery. SQLite replaces such a column with `NULL` and drops the call. The binder now does the same before it settles aggregates (`bind/unused_columns.rs`), and still refuses with `misuse of aggregate` where SQLite does | `bug-hunt-11-matrix.sql`, 22 cases |
| exprindex | `min()`, `max()` and `GROUP BY` over an indexed expression read the whole table | `min()` and `max()` are planned as `ORDER BY ... LIMIT 1` over the same index (`plan/extreme.rs`), and `GROUP BY` walks the index in order (`plan/group_walk.rs`), with the expression's value read from the index entry. 200,000 rows: `min` 42.4 ms to 0.31 ms (SQLite 0.06 ms), `GROUP BY` 43.9 ms to 4.8 ms (SQLite 5.3 ms) | `bug-hunt-11-exprindex.sql`, 15 cases; `engine::expression_index_shapes` |
| widerows | a row of 800 or more integer columns took a 32 KiB leaf of its own | a leaf with several delta rows and no sorted rows is an ordinary leaf, so the builder now puts as many wide rows in one delta area as fit, and an append that splits a full leaf of a wide table keeps its rows on the left. No format change. 1,000 rows of 900 columns: 31.4 MiB to 10.6 MiB (SQLite 3.9 MiB) | `inillucent-tree::wide_rows`, `bug-hunt-11-widerows.sql`, the 63 durability suites |

## 5. Found and not changed

| Item | Why it is left |
|---|---|
| A table and an index each take at least one 32 KiB page, so 3,000 tables with one index each take 197 MB against SQLite's 25 MB | the page size is part of the file format. `docs/sql.md` now states the cost |
| `PRAGMA table_list` lists tables in a different order | SQLite's order is the iteration order of its internal hash table, which is not defined |
| A table that 2.8.1 or earlier wrote as `rtree_i32` holds values that release had already rounded to a 32 bit float, such as 16777217 stored as 16777216 | the file no longer holds the number that was written |
| An FTS5 query with three levels of nested `AND`, `OR`, `NOT` and `NEAR` gives a different `bm25` or highlight in about 2 to 4% of random queries | matching it needs a closer copy of SQLite's iterator machinery. A few of those queries make SQLite itself report `database disk image is malformed` |

## 6. Results

**The strict run.** `inillucent-testrun --changed origin/main --strict` on the merged branch selected
every target, 462 targets and 5,369 tests, because the change touches paths no rule in the map
covers. None failed and none was undetermined. It took 1,697 s of wall time and 22,166 s of
processor time. Six suites ran without their prerequisite and evidenced nothing: the TLS transport
tests (no openssl), the network tests of `inillucent-remote`, live MySQL and live PostgreSQL, the
Nikaya workload freshness check (no Nikaya checkout beside this worktree), and the Go runner of the
bindings conformance suite. These are this machine's declared absences. The suites that failed in
the nightly run of `main` at 7333d3ca (`usage_corpus`, `process_storm`, `automatic_index_speed`,
`documentation`, `gates_fail_closed`) all pass on this branch.

**Clippy** with `--workspace --all-targets --all-features -D warnings` is clean, and
`cargo fmt --all -- --check` reports nothing. `node tools/doc-style/check.mjs` reports no problems.
`node tools/doc-facts/check.mjs` agrees with the engine after the feature probe and the register
audit were run on a release build.

**The usage corpus.** Fourteen new files hold 403 cases, all graded against SQLite 3.53.4 by
`differential::usage_corpus`, and every one passes: pragma 65, window 59, ftsquery 51, ftstables
39, appsengine 37, datetime 33, files 27, json 27, matrix 22, planner 19, exprindex 15, lead 4,
migrate 3 and widerows 2. Deviations that follow SQLite's development version or a documented
difference are in `compat/corpus/usage/known.toml`.

**New interop fixtures:** `tests/interop/alter-after-constraint/` (written by 2.0.2) and
`tests/interop/rtree-i32-float/` (written by 2.8.1), each read and written by a test in
`e2e::release_format` that fails on the release before the fix.
