# Bug hunt 9: the places the first eight hunts did not reach

## Terms used in this document

| Term | Meaning |
|---|---|
| case | a short SQL script run against a fresh database through both the pinned `sqlite3` and `inillucent-shell`, whose outputs are compared byte for byte |
| probe | `tools/bug-hunt/probe.mjs`, which runs case files and reports every difference, crash, hang and slow case |
| hunter | a sub agent given one area of real use and asked to find defects in it, not to fix them |
| metamorphic check | a query rewritten into a form that must return the same rows, such as a `WHERE p` query against the union of `WHERE p`, `WHERE NOT p` and `WHERE p IS NULL`. A difference is a defect without needing SQLite to say which answer is right |
| red, green | the test run before the fix, which must fail, and after it, which must pass |

## 1. Why

The ticket asks for as many defects as possible to be found before the public release, for tests
that cover real use from end to end, and for edge cases with wrong answers or slow paths. It asks
for the issue trackers of Turso, FrankenSQLite and SQLite to be read for cases a second
implementation of the dialect may share.

Hunts 1 to 8 compared hand written SQL against SQLite, mined SQLite's own tests, ran the command
line round trips, and ran several processes on one file with kills. Each SQL case they wrote was
picked by a person or an agent. Nothing generated queries at random, nothing drove the language
packages or the MCP server the way an application does, and nothing pushed a single statement to
the size limits.

## 2. Areas

Eight hunters run at once against one release build. Each works in its own folder under
`_agent_output/task-2245/agents/<area>/` and prefixes its database names with the area, because
sub agents share a scratch folder (see `.claude/repo-plan.md`).

| Hunter | Area | What it does |
|---|---|---|
| random | random queries | a generator of random schemas, rows and queries, run through both shells, with three metamorphic checks: ternary logic partitioning, the same predicate with and without an index (`+col` and `NOT INDEXED`), and `count(*)` against `sum(p)` over the same rows |
| bindings | language packages | the Python `Database` class, the Node addon, the C library from a small C program, and the Rust driver: value types at their edges, statements left half read, handles closed in the wrong order, errors and their statuses, threads, reopen |
| mcp | the MCP server and `--output json` | every MCP tool with odd arguments, `--readonly` and `--root` against paths that try to leave the root, large and typed results, malformed requests, and the JSON every CLI command prints |
| limits | size and resource limits | values of hundreds of megabytes, thousands of columns, deep nesting, long `IN` lists, many tables, many attached files, sorts larger than memory, long sessions that may leak, read only and unusual paths |
| planner | speed of everyday queries | application shaped queries at 100,000 to 1,000,000 rows, warm and after a reopen, against SQLite; anything more than three times slower is reported with its plan |
| upstream | other engines' trackers | Turso (and Limbo before it), FrankenSQLite, the SQLite forum and timeline from 2025 to now, written as cases and run through the probe |
| search | the retrieval engine | vector and keyword search under updates, deletes, reopen, `VACUUM`, rollbacks and several processes; recall after churn; `embed()`; `inillucent_search` with filters |
| shell | `inillucent-shell` against `sqlite3` | every dot command and command line flag a script uses, piped input, exit codes, `.import`, `.output`, `.once`, `.read`, `.backup`, `.restore`, `.clone`, `.parameter` |

## 3. Method

1. Each hunter writes cases in its area, runs them, and reports a minimal repro for every
   difference, crash, hang and slow case, with what SQLite does.
2. I confirm each report, drop the ones that are deliberate deviations listed in
   `compat/corpus/usage/known.toml` or `docs/sql.md`, and group the rest.
3. Each defect gets a test that fails before the fix. A SQL difference becomes a case in
   `compat/corpus/usage/bug-hunt-9-<area>.sql`, graded by `differential::usage_corpus`. Anything
   SQLite does not have, or a timing, becomes a Rust test in the tier the testing standard names.
4. The fix, then the test green, then `inillucent-testrun --changed --strict`.

The lead also probed by hand: an index matrix (`lead/genidx.mjs`, every index shape across three
table kinds and five write patterns, 4,000 rows so every index has several leaves), large values
through updates, `VACUUM` and reopen (`lead/genbig.mjs`), and seeded write workloads over multi leaf
trees with savepoint rollbacks and reopens, compared row for row with SQLite (`lead/genwrite.mjs`).
The scripts are in `_agent_output/task-2245/lead/`.

## 4. Defects found and fixed

"Test" names the test that fails before the fix and passes after it. Corpus cases are in
`compat/corpus/usage/bug-hunt-9-<area>.sql` and are graded against SQLite 3.53.4.

### 4.1 Wrong answers, lost data, and a file that would not open

| Defect | Found by | Cause | Fix | Test |
|---|---|---|---|---|
| Every `DESC` index was corrupted by writes once it had more than one leaf: `integrity_check` reported keys out of order along the leaf chain, and an indexed `WHERE c = 3` counted 85 rows of 428. Every `DESC` shape failed (single, composite, `NOCASE DESC`, expression, partial, `UNIQUE`), on rowid, `WITHOUT ROWID` and implicit rowid tables; every ascending shape passed | lead, index matrix: 103 of 195 cases | a regression from the round 3 usage hill climb (bisected to `3ccc0b2e`): `put` started encoding its key with `encode_key_small`, whose fast branch encoded each column itself and left out the directions, so a write descended with an ascending key past separators stored inverted. Seeks through the same function were routed the same way | `encode_key_small` encodes through `KeyEncoding::encode_into` with the directions; `is_above`, which chooses an append split, compares in the stored order | corpus `engine/desc-*`, 5 cases; the matrix, 195 of 195 after |
| `GROUP BY` over a covering index with a `sum`, `avg`, `total` or `group_concat` of a `REAL` returned one group per run of equal keys: 1,250 groups where SQLite returns 5 | planner | the stage builder swaps a covering index scan for a table scan when an aggregate is sensitive to the order its values arrive in (a float sum), but the planner had already chosen a streaming `GROUP BY` that relies on the index order | the swap applies only to an aggregate over the whole table | corpus `engine/group-by-over-a-covering-index-*` |
| `sum()` of an `INTEGER` column returned 0 instead of `integer overflow` on the second read of the same table (1, 2^63-1, -2^63, 0), so a long running MCP session and a fresh command line disagreed | MCP | the dense integer path added the run in `i128` and looked only at the total; SQLite reports an overflow of the running total | a checked add per value, in row order, and the rest of the run through the per row path | unit `the_dense_path_agrees_with_the_per_row_path`, corpus `engine/sum-overflows-*` |
| A join's `WHERE` or `ON` term that is a bare `REAL` or numeric `TEXT` value dropped its rows: `FROM y JOIN x ON x.c1 = y.c1 WHERE x.c2` kept only the row where `c2` was an integer | random | `keeps()` kept a pair only when the condition was a non zero integer | the condition is true by SQLite's rule, `expr::truth` | corpus `engine/a-join-condition-is-true-*` |
| A database could not be opened again: `replaying the log: record kind 18 ... page is not a blob extent`. It needs a long key deleted, its freed pages given to another table, and the process ending with a transaction open, which skips the checkpoint | random | the replay of the long key's insert read the key from its extent pages, but those pages now held a newer write, so their own records had been skipped by the page LSN rule | an insert whose key extent starts on a page stamped after the record is skipped: the row it named was removed later in the same log | `durability::reused_key_extent` |
| A row of many values each under the spill threshold (17 text columns of 2,000 bytes) was refused as larger than a page | limits | only a value over `page_size / 8` went out of line | `wide_row_threshold`: a row wider than half a page moves its longest non key values out of line until half a page is left; the write path and the builder share the rule | unit `a_wide_row_moves_*`, corpus `engine/a-row-of-many-values-*` |
| `PRAGMA recursive_triggers = ON`: an `AFTER INSERT` trigger that updated its own row failed with `too many levels of trigger recursion` | limits | the body's write was handed the trigger back whatever its event | handed back only for the trigger's own event, and for `UPDATE OF` only when a named column is assigned | corpus `engine/recursive-triggers-*` |
| Recursive CTE: an unbounded recursion under an outer `WHERE` and `LIMIT` hung; a literal anchor compared with the wrong affinity; invalid shapes were accepted | upstream (Turso) | | the recursion stops when the query above is full; SQLite's affinity and refusals | corpus `sql/recursive-cte-*` |
| A bare column beside a `FILTER`ed `min` or `max` took the wrong row; unqualified `rowid` over two tables was not ambiguous; `lag`/`lead` with a NULL or fractional offset; `round(0.49999999999999994)`; `unhex(x, NULL)`; `printf()` with no arguments; `foreign_key_check` with a parent that has no unique key; `integrity_check(nosuch)`; an attached `:memory:` database's journal and locking modes; `SELECT 'a' COLLATE bogus`; `main.generate_series`; column order of a parenthesised join with `USING`; `rtree_i32` rounding; rtree and FTS5 messages | upstream (Turso) | | each matches SQLite 3.53.4 | corpus `sql/*`, 19 cases |
| FTS5: `f MATCH 'a' AND f MATCH 'b'` returned no rows; lower case `and`, `or`, `not` were operators; a query with no tokens was an error; quoted column filters failed; `CREATE` accepted declarations SQLite refuses; unicode61 folded U+0130 and U+0131 differently; hidden column and auxiliary function argument checks | search | | each matches SQLite; the unicode61 fold uses SQLite's own tables | corpus `search/*`, 12 cases |
| Vector index: a huge `LIMIT` aborted the process; `m = 1` aborted the build; `REPLACE` and upsert failed on a table with an HNSW index; `DROP TABLE` left the index's six tables behind; `--measure dot` returned the farthest rows first; `k = 0` returned a row; a bad query vector said `SQL logic error`; a dump of search tables did not load back | search | | each fixed | the engine search suites, named in the search fixer's commit |
| Python `Connection.execute` ran only the first of several statements and dropped the rest silently; `run()` and `query()` decoded the program's output as cp1252; a refused `close()` leaked the database; a NaN parameter was stored as a REAL NaN where SQLite stores NULL; a dict of parameters bound its keys; Node returned different values for the same query on its three routes, and hung on a lone surrogate | bindings | | each fixed; NaN binds NULL in the driver, so every binding agrees | `packages/python/tests/test_driver_defects.py`, `roundtrip.test.mjs`, `a_nan_parameter_is_stored_as_null` |
| MCP: a `db` argument re-pointed the session for every later call and rolled back an open transaction; a `db` naming a missing file created it, even under `--readonly`; `setup_embeddings` wrote outside `--root`; `params-file: "-"` read the protocol stream and hung; `inillucent_query` ran writes; `ping` waited for a running call; Windows device names and alternate data streams were accepted as file names under `--root`; stale `changes` on a `SELECT` | MCP | | each fixed | `e2e::mcp_per_call` and the MCP suites |
| `inillucent-shell`: `.backup aux` copied main; invalid UTF-8 input silently stopped the script with exit 0; blob output was rewritten to U+FFFD; `.open --readonly` and `file:` URIs created files named after the option; `.exit N` exited 0; several SQL arguments were joined into one; a recursive `.read` overflowed the stack; a byte order mark was a syntax error; `.once` covered more than one command; missing modes and `.mode` options; `.separator` row separators; `.width` wrapping | shell | | each matches `sqlite3` | corpus `shell/*`, 27 cases, and `e2e::shell_command_line` |

### 4.2 Speed and memory

| Case | Before | After | SQLite |
|---|---|---|---|
| `ORDER BY created_at DESC LIMIT 20` over 200,000 rows and an index on `created_at` that does not cover the query | full scan and sort, 19 to 116 ms | walks the index, under 1 ms | 0.1 ms |
| `count(*) WHERE json_extract(payload, '$.type') = 'buy'` with an index on that expression | table scan, 129 ms | covering index seek, under 1 ms | 11 ms |
| `LEFT JOIN ... WHERE p.id IS NULL`, 100,000 by 200,000 rows | 630 to 800 ms | 75 to 155 ms | 26 to 34 ms |
| `count(DISTINCT post_id)` over 500,000 rows | 80 to 112 ms | 15 to 21 ms | 27 to 31 ms |
| `INSERT OR REPLACE` onto 100,000 existing keys | 750 to 780 ms | 100 to 130 ms | 17 to 24 ms |
| `json_each` over 300,000 documents | 750 to 830 ms | 580 to 610 ms | 88 ms |
| `VACUUM` of two 500,000 row tables | 2.87 s | 1.95 s | 1.57 s |
| FTS5 bulk load of 120,000 rows in one transaction | 282 s | 6.5 s | about 1 s |
| `printf('%.1000000d', 1)` | 8.9 s | milliseconds | 0.04 s |
| `substr` and `trim` of a 100 MB text, peak memory | 2.6 GB | 0.68 GB | 0.39 GB |
| `ORDER BY` of 3,000,000 rows, peak memory | 650 MB | 400 MB | about 10 MB |

The causes, in the order of the table: an index that does not cover the query was never offered as
a path for an `ORDER BY` with a small `LIMIT`, and would have been priced as a row fetch for every
row (`plan/limit_walk.rs` offers the walk and prices it by the rows the `LIMIT` reads); an index on
an expression did not count as covering because the column read inside the answered term counted
as read; probes into a written leaf rebuilt its packed copy for most probes; `DISTINCT` hashed
encoded keys where integers suffice; `REPLACE` made six tree operations per row; `json_each`
allocated per call; `VACUUM` copied a row at a time; FTS5 flushed its buffer before every multi row
statement; `printf` inserted one zero at a time; `substr` and `trim` held a slice per character;
the sort's run threshold counted only the values of a row, about a sixth of what it occupies.

### 4.3 Found by the random query generator, fixed after the first round

| Defect | Fix | Test |
|---|---|---|
| A `WITHOUT ROWID` table whose `INTEGER` primary key holds a text, real or blob key: inserts worked, `DELETE` and `UPDATE` did nothing | the tree keeps its 8 byte integer keys and adds a tail for the other classes; a file made by the previous build reads and updates correctly | corpus `random/wor-integer-key-*`, 2 cases |
| A `WITHOUT ROWID` table with its primary key not on the first column, plus a secondary index: `DELETE` or `UPDATE` failed with "the tree read for FROM term 0 does not carry column 1", and so did a plain `SELECT` | the planner compared record slots with declared columns when it chose a covering index | corpus `random/wor-primary-key-not-first-*` |
| A `UNIQUE` on the same columns as the primary key of a `WITHOUT ROWID` table: `count(*)` failed with "no layout imported for root page 0" and an upsert raised `UNIQUE` | the two merge into one index, as in SQLite | corpus `random/wor-unique-duplicating-the-primary-key` |
| A compound select used as a subquery kept INTEGER where SQLite applies REAL from another arm | SQLite's `sqlite3SubqueryColumnTypes` rule | corpus `random/compound-subquery-column-real-affinity` |
| `UPDATE ... FROM` on a table with a generated column: "ambiguous column name"; `REINDEX` with a generated column and an expression index: "no layout imported"; a STORED generated column ignored its affinity in an upsert; a virtual generated column whose expression fails was accepted on write | each fixed | corpus `random/*`, 4 cases |
| `UPDATE OR FAIL`, `OR IGNORE`, `OR REPLACE` and the same `INSERT` forms on a STRICT table undid earlier rows on a datatype error inside a transaction | the earlier rows are kept, as in SQLite | corpus `random/strict-datatype-failure-*` |
| `sum() OVER ()` with no argument was accepted; `GLOB ... ESCAPE` was accepted; `unhex` read past a zero byte; a `WITHOUT ROWID` primary key was checked after a `UNIQUE` declared before it | SQLite's messages and order | corpus `random/*`, 4 cases |

### 4.4 Planner choices and resource growth, fixed after the first round

| Case | Before | After | SQLite |
|---|---|---|---|
| Keyset pagination `WHERE (created_at, id) < (?, ?) ORDER BY created_at DESC, id DESC LIMIT 20` | 2 to 107 ms | under 1 ms | under 1 ms |
| A join ordered by `p.created_at DESC LIMIT 20` | 224 ms | under 1 ms | 0.1 ms |
| A correlated subquery in the result under `ORDER BY p.id LIMIT 10` | 428 ms | under 1 ms | 9 ms for `LIMIT 1000` |
| `row_number() OVER (PARTITION BY user_id ...)` under `WHERE user_id < 50` | 150 ms | 1 ms | 0.2 ms |
| A six table join filtered on one table's unindexed column | about 800 ms | about 100 ms | 159 ms |
| 200,000 `CREATE TEMP TABLE` and `DROP` | 45 s, 266 MB | 8.0 s, 9 MB | 4.1 s, 6 MB |
| 1,000,000 `UPDATE`s of 1,000 rows in one transaction, memory | 239 MB | 15 MB | |
| `INSERT ... SELECT` of 5,000,000 rows into a non empty table, memory | 2,195 MB | 988 MB | about 10 MB |

The planner changes are in `plan/implied.rs` (a row value comparison gives its leading column a
range), `plan/join_cost.rs` (the join order cost counts the `ORDER BY`, the `LIMIT` and unindexed
filters, with SQLite's selectivity numbers) and a window pushdown of terms on the partition columns.
The temp table growth was a log nobody folded for a temp or in memory database and a handle map
that never lost an entry; the update memory was one undo record per update, now dropped when an
earlier statement already holds the row. Tests: `engine::planner_choices` and
`engine::scratch_memory_flat`.

## 5. Found and not fixed

The items left are recorded in the known bugs tickets so they are triaged with the others: the
medium ones as bugs 45 to 49 of task-2110, the low ones as bugs 36 to 46 of task-2111.

| Item | Why it is left |
|---|---|
| A statement holds its whole result in memory, and so does the shell: printing 3,000,000 rows peaks at 892 MB | the engine materialises a result on its first step; streaming needs a new driver API |
| Window functions, `DISTINCT` and compound selects hold every row, with no spill | only `Sort` spills; each operator needs its own spill |
| `INSERT ... SELECT` collects every source row before it writes | streaming the source changes the statement's atomicity and constraint checks |
| Two connections of one `Database` share one transaction | the documented design of `Database::session`; now documented for Python and in `drivers/README.md` |
| An FTS5 load that commits every statement rewrites the common words' lists | needs a segment layout that appends |
| `json_each` setup per outer row, `ATTACH ':memory:'` cost, a compound derived table counted by materialising, rowid probes in a self join, a vector index in an attached database, vector functions returning infinity, a plain `UPDATE` on a STRICT table after a datatype error, the MCP backup overwriting a file that is not a database, looser limits at the edges, the unicode61 letter classes, `.mode` limits | smaller; each is written up with its numbers in task-2111 |
| Short circuit evaluation: `a = 1 OR abs(-9223372036854775808)` returns 1 where SQLite raises `integer overflow` | deliberate; `docs/sql.md` lists it |
| Python's `run()` and `query()` cannot send SQL longer than the command line allows | the CLI has no way to read SQL from a file |

## 6. Results

**The strict run.** `inillucent-testrun --changed origin/main --strict` selected every target,
because the change touches root pages no rule narrows: 411 targets, 4,982 tests. It found two
regressions this ticket had made, both fixed and then rerun:

- `matrix_deep::compound`: the REAL conversion of a compound derived column (section 4.3) also
  converted the rows of a multi row `VALUES` arm, which SQLite writes without it. Such a column now
  keeps a numeric affinity and converts nothing; corpus `random/compound-subquery-multi-row-values-*`.
- `durability::process_journal_handoff`: the shell fixer made `inillucent-shell -readonly` open its
  connection read only in the engine, and a long lived read only shell then answered from its first
  statement's state. The shell opens for writing and refuses writes itself again, as it did before.
  The engine's read only connection, held open through the driver, does see each later commit of
  another process; `durability::read_only_connection` holds that.

The other suites the run did not grade are the machine's declared absences: MySQL, PostgreSQL, Go and
openssl. `inillucent-remote`'s library suite passes with `INILLUCENT_NETWORK_TESTS` set (85 tests).
The Nikaya workload freshness check needs the Nikaya checkout beside the main checkout, so it was not
evidenced from the worktree.

**Clippy.** `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean.

**The usage corpus.** `differential::usage_corpus` grades the five new files against SQLite 3.53.4:
`bug-hunt-9-engine.sql` (13 cases), `bug-hunt-9-sql.sql` (19), `bug-hunt-9-search.sql` (12),
`bug-hunt-9-shell.sql` (27) and `bug-hunt-9-random.sql` (15), 86 cases, and all pass. The cases
for the defects in section 4 were run against the build this ticket started from and differed
there.

**What the hunters ran.** About 275,000 random query blocks over 600 small schemas and 105 multi
leaf schemas; 298 cases from Turso's tracker; about 1,040 shell comparisons; about 14,300 MCP calls;
520 limit comparisons; 92 application queries at 100,000 to 1,000,000 rows, each six times; and about
2,500 retrieval statements with recall measured against exact scans.
