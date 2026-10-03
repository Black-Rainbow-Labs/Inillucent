# Matching SQLite for the statements applications actually send

## Terms used in this document

| Term | Meaning |
|---|---|
| case | one named script in `compat/corpus/usage/*.sql`, starting at a line `-- case: <area>/<name>` |
| the corpus | every case in `compat/corpus/usage/` |
| the pinned shell | `.sqlite-ref/3.53.4/shell/sqlite3.exe`, the SQLite 3.53.4 command line built from the published amalgamation |
| match | the pinned shell and `inillucent-shell` print the same bytes for a case, after the two pointer lines under an error message are set aside |
| known list | `compat/corpus/usage/known.toml`: the cases that do not match yet, each marked `defect` or `deviation` |
| defect group | differences that share one cause and that one person can fix together |
| held out cases | cases a fixing agent does not read, used to check that a fix is general and was not written for one case |

## 1. Why

The ticket before this one (multi process access, crashes and file damage) found thirteen defects by
running the database the way programs run it. That work covered the file and the processes. This
ticket covers the SQL: the statements an application, a driver or an ORM sends, in the order it sends
them.

The existing suites are strong for single statements. `compat/compat-report.md` records 263 of 276
capabilities as passing, the statement matrix holds about 18,800 lines of `.slt`, and
`docs/feature-comparison.md` records 402 of 416 probed cases matching SQLite. What they do not test
is a sequence: a rename followed by a catalog read, a table rebuild followed by a foreign key check,
a `sqlite_master` row joined to `pragma_index_list`, an upsert against a partial unique index. The
inventory for this ticket (kept with its evidence in `_agent_output/task-2174/inventory.md`) found no
test that replays the SQL any ORM sends, no end to end test of the twelve step table rebuild, and thin
coverage of `ALTER TABLE RENAME` rewriting views, triggers and foreign keys.

Every one of those gaps turned out to hold defects. The first 548 cases of this ticket found 141 that
do not match SQLite 3.53.4. Section 4 lists them.

## 2. How the cases were found

Three sources, each run against the pinned shell so the expected output is SQLite's own answer and
not anybody's recollection of it:

1. **The statements ORMs and drivers send.** A research pass read how Django, SQLAlchemy and
   Alembic, Rails, Prisma, Drizzle, EF Core, Knex, TypeORM, Sequelize, rusqlite, sqlx, diesel,
   better-sqlite3, Python's `sqlite3` module and the Go drivers use SQLite: the PRAGMAs they run when
   they connect, the catalog queries they run to read a schema, the table rebuild they run to change
   a column, and the error messages they parse. Django, SQLAlchemy and Rails match on the message
   text `UNIQUE constraint failed: <table>.<column>`, and Prisma maps extended result codes, so a
   message or a code that differs is a defect an application sees. 398 cases came from this pass.
   The source of each is recorded in `_agent_output/task-2174/research-sqlite-and-orms.md`.
2. **SQLite's documented quirks.** `quirks.html`, `datatype3.html`, `nulls.html`, `lang_upsert`,
   `lang_returning`, `lang_altertable`, `foreignkeys.html`, `windowfunctions.html`, `json1.html` and
   `pragma.html`, plus the bug fix sections of the release notes from 3.35 to 3.53. About 150 cases
   were written by hand from these, covering affinity, numbers, text and collation, DML, schema,
   constraints, ALTER, queries, JSON, dates and transactions.
3. **The bugs other rewrites of SQLite have had.** Turso (formerly Limbo) is a rewrite of SQLite in
   Rust, and its issue tracker is a list of the mistakes a rewrite makes. Section 2.1 records what
   that pass found.

The method follows the evaluation and hill climbing loop Anthropic describes for agents (sample real
inputs, grade them with a programmatic grader, split into a training set and a held out set, change
one thing per round, keep a change only when both sets improve). Here the grader is exact: the
pinned shell's output. The `/claude-api hillclimb` skill itself optimises a prompt or an agent, and
there is no prompt here, so the loop is run by hand with the corpus as the grader. Section 5 says how.

### 2.1 Turso issues

A research pass read the issue trackers of Turso (formerly Limbo) and of frankensqlite, another
rewrite of SQLite in Rust, and turned 839 reports into self contained scripts. Each was run through
the pinned shell, so the expected output is SQLite 3.53.4's own. Four scripts whose output SQLite
itself does not repeat between runs (they print `random()`, page counts or `now`) were removed,
and one more was removed later for the same reason. The other 834 are the held out files
`compat/corpus/usage/holdout-*.sql`, grouped by area: joins 99, upsert and conflict clauses 78,
triggers 61, generated columns 56, affinity and comparison 54, and the rest across 17 smaller areas.

On their first run, 567 of 835 matched.

## 3. The harness

### 3.1 The corpus

`compat/corpus/usage/*.sql` holds the cases. A file is a list of cases; a case starts at
`-- case: <area>/<name>` and runs to the next marker. Every case runs on a new, empty database in a
directory of its own, so a case can `ATTACH` or `VACUUM INTO` a relative file name without meeting
another case's file.

Each case is fed to both shells after `.mode quote` and `.headers on`. Quote mode prints a value as
the literal that would produce it, so `1`, `1.0`, `'1'` and `x'31'` are four different strings, and
a storage class difference is a byte difference. Headers make the column names part of the bytes.
Error messages are printed by both shells as `Parse error near line N: <message>` or
`Error near line N: <message>`, so the message text is compared too.

What the comparison does not see: the extended result code (the engine suites under
`crates/inillucent-compat/tests/differential/` compare those through the oracle driver), and the two
lines a shell prints under an error to point at the offending token. Those two lines are removed
from both outputs before comparing. Where a shell points depends on the byte offset the engine
reports for each kind of error, which is its own piece of parity. Comparing it in this suite made
cases with every right row and the right message fail on the pointer, and hid the differences the
suite exists to find. Section 4 lists the pointer differences as their own group.

### 3.2 The suite

`crates/inillucent-compat/tests/differential/usage_corpus.rs`, target
`inillucent-compat::differential::usage_corpus`, tier `differential`, `requires = ["shell"]`.

- `every_usage_case_matches_sqlite_or_is_listed` runs the corpus on eight threads. It fails when a
  case differs and is not in the known list, and when a case in the known list matches. So the list
  can only shrink by fixing something, and can only grow by somebody writing down why. It prints how
  many cases match in each area.
- `the_corpus_and_its_known_list_are_well_formed` checks names are unique, each case has SQL, each
  row of the known list names a case that exists, and each row's kind is `defect` or `deviation`.
  It also fails if the corpus drops below 500 cases, so a file deleted by mistake does not leave a
  suite that passes because it runs less.
- Two small tests check the file splitting and the removal of pointer lines.

With `INILLUCENT_USAGE_REPORT=<path>` the suite writes every difference as JSON, which is what a
fixing agent reads to see the effect of a change on the whole corpus.

The whole corpus runs in about five seconds in a debug build.

### 3.3 The known list

```toml
[[known]]
case = "dml/upsert-partial-index-target"
kind = "defect"
reason = "ON CONFLICT with a partial unique index target is refused as unsupported"
```

A `deviation` is a difference the project keeps on purpose. Each needs a reason a reader can check,
and `docs/sql.md` must say the same thing. A `defect` is something to fix. The goal of this ticket
is a known list with no `defect` rows.

## 4. What the corpus found

The first full run, after the shell defects below were fixed, matched 420 of 563 cases. The 143 that
differ were read statement by statement and grouped by cause. The full triage, with a minimal
reproduction and both outputs for every group, is `tasks/task-2174-usage-parity-triage.md`. A case
can belong to more than one group.

Four shell defects were found and fixed while the harness was built, because each one made every
later comparison noisy:

- with `.headers on`, a statement that returned no rows (`CREATE TABLE`, `INSERT`, an empty
  `SELECT`) printed an empty line, where `sqlite3` prints nothing;
- `.mode insert` with headers on did not name the columns (`INSERT INTO tab(a,b) VALUES(...)`);
- `.mode box` turned headers on for good, so `list` and `insert` modes printed headers afterwards.
  In `sqlite3` an explicit `.headers` choice survives every `.mode`, and without one each mode uses
  its own default;
- `box`, `table`, `markdown` and `column` with headers off still drew the header and sized each
  column by its name.

### 4.1 Wrong answers, and refused statements applications send (high)

| Id | Defect | Cases |
|---|---|---|
| H1 | `ORDER BY` is not applied when an `IN` list is answered from an index on a TEXT column | 1 |
| H2 | `LIMIT -1 OFFSET n` over a compound subquery ignores the OFFSET | 1 |
| H3 | `x IS TRUE` and `x IS FALSE` treat only the integer 1 as true | 1 |
| H4 | a scalar subquery value loses its column affinity in a comparison, and a compound SELECT column keeps an affinity it should not have | 2 |
| H5 | a `pragma_*` table valued function joined to `sqlite_master` with `m.name` as its argument fails, and `pragma_foreign_key_check` does not exist. EF Core and Rails read a schema this way | 4 |
| H6 | `ALTER TABLE RENAME TO` does not rewrite `REFERENCES` in child tables or table prefixes in views | 3 |
| H7 | `ALTER TABLE RENAME COLUMN` does not rewrite trigger bodies or `REFERENCES parent(column)` | 2 |
| H8 | `UPDATE` and `DELETE` on a view with `INSTEAD OF` triggers are refused | 1 |
| H9 | a recursive CTE with `LIMIT` or `ORDER BY` is refused | 1 |

### 4.2 Differences an ORM or a driver sees (medium)

| Id | Defect | Cases |
|---|---|---|
| M1 | `ON CONFLICT` naming a partial unique index or an expression index is refused. The partial index form is the usual soft delete pattern | 3 |
| M2 | a result column alias cannot be used in `WHERE` | 1 |
| M3 | `(a, b) IN (SELECT ...)` is refused | 1 |
| M4 | `count()` is refused, and some aggregate and window misuse reaches an internal error | 3 |
| M5 | `UPDATE t SET id = id + 1` visits rows in an order that lets it succeed where SQLite fails with a UNIQUE error | 1 |
| M6 | `PRAGMA foreign_keys` inside a transaction takes effect, and `BEGIN IMMEDIATE` succeeds under `query_only` | 3 |
| M7 | PRAGMAs SQLite accepts are refused or not stored: `mmap_size`, `temp_store = FILE`, `wal_autocheckpoint`, `journal_size_limit`, `legacy_alter_table`, `synchronous = EXTRA`, and `user_version` past 32 bits | 7 |
| M8 | `ALTER TABLE ADD COLUMN` accepts a non constant default, and a `REFERENCES` column with a non NULL default | 3 |
| M9 | `ALTER TABLE DROP COLUMN` refuses drops SQLite allows, allows one it refuses, leaves a double space in the stored SQL, and words its errors differently | 9 |
| M10 | `ALTER TABLE RENAME` does not check dependent views and triggers, and accepts a name starting with `sqlite_` | 5 |
| M11 | the 3.53 forms `ALTER COLUMN ... SET NOT NULL`, `DROP NOT NULL` and `ADD CONSTRAINT ... CHECK` are missing | 2 |
| M12 | statements SQLite refuses are accepted, for example `CREATE TABLE t(a COLLATE nosuch)`, `DEFAULT (nosuch)`, a duplicate `WITH` name, a qualified table name inside a trigger | 8 |
| M13 | statements SQLite accepts are refused or give a wrong value: `SELECT 'a' 'b'`, `VACUUM INTO`, `BEGIN TRANSACTION name`, a view column list of the wrong width, `printf('%.*c', n, 'x')` | 6 |
| M14 | a view's expression column is named `''`, and duplicate column names in a subquery are not given the `a:1` suffix | 3 |
| M15 | PRAGMA output differs: `index_list` reports origin `pk` for a `UNIQUE` constraint, `foreign_key_list` order and implicit parent column, `table_list` lists a temp table under `main`, and others | 9 |
| M16 | error messages ORMs match on differ, including a Rust debug value in `a rowid must be an integer, not Text([120])` where SQLite says `datatype mismatch` | 10 |
| M17 | dates with a negative year, and the `+YYYY-MM-DD HH:MM:SS` modifier, return NULL | 2 |
| M18 | `json_extract` with a malformed path fails where SQLite returns NULL; `json_object` with an odd number of arguments fails when it is prepared rather than when it runs | 2 |

### 4.3 Shell output and rare forms (low)

| Id | Defect | Cases |
|---|---|---|
| L1 | quote mode prints control characters raw where `sqlite3` prints `unistr('...')` | 5 |
| L2 | the pointer lines under an error differ (set aside by the corpus, section 3.1) | 9 |
| L3 | after a parse error `sqlite3` abandons the rest of the input line | 4 |
| L4 | a runtime error discards rows already printed | 1 |
| L5 | `new.id` in a `BEFORE INSERT` trigger is the assigned rowid, where SQLite gives -1 | 1 |
| L6 | `EXPLAIN QUERY PLAN` text differs | 3 |
| L7 | other error messages are worded differently | 29 |
| L8 | `PRAGMA wal_checkpoint(TRUNCATE)` reports a non zero log size | 1 |

Three more were found after the triage: numbered parameters set with `.parameter set ?2 20` do not
bind, `.parameter list` prints a NULL parameter as the text `'NULL'`, and the out of range message
for `?0` differs.

### 4.4 Not defects

| Id | Difference | Decision |
|---|---|---|
| N1 | `DELETE ... LIMIT` and `UPDATE ... LIMIT` are accepted; the pinned shell is built without `SQLITE_ENABLE_UPDATE_DELETE_LIMIT` | deviation. Many distribution builds of SQLite enable it, and refusing it would break scripts written for them |
| N2 | `page_size` defaults to 32768 and `cache_size` to -131072; `PRAGMA page_size = 8192` before the first write is ignored | deviation. The page size is part of inillucent's own file format |
| N3, N4, N5 | the database file path, a random byte, and the order of `PRAGMA table_list` with no `ORDER BY` | the cases were rewritten to print only what SQLite defines |

## 5. The hill climbing loop

One defect group per round:

1. Run the suite with `INILLUCENT_USAGE_REPORT` set and record how many cases match.
2. Read only the differences that belong to the group, from the training files. Do not open the held
   out files (section 5.1).
3. Find the cause in the engine, the binder, the planner or the shell, and fix the cause. A change
   that special cases one statement's text, one table name or one value is not a fix.
4. Run the suite again. Keep the change only when no case that matched before differs now, on the
   training files and on the held out files both.
5. Remove the known list rows of the cases that now match, run the suite once more, and run
   `inillucent-testrun --changed` for everything else the change could affect.

When a group stops improving, the agent writes down why each remaining case still differs and stops
changing code for that group; the reasons go into the known list.

### 5.1 Held out cases

`compat/corpus/usage/holdout-*.sql` holds cases written from sources the fixing agents do not read.
The suite runs them like any other file. A fixing agent does not open them. When a round fixes a
training case, the held out cases with the same cause should start matching too; when they do not,
the fix was written for the case and not for the cause.

## 6. Work packages

Each package is one Sonnet agent in a git worktree of its own, branched from `task-2174` after the
harness commit. Each follows the loop in section 5, removes the known list rows of the cases it
fixes, and adds cases for anything it finds on the way. The packages are split by the code they
edit, so two agents rarely touch one file.

| Package | Groups | Where the code is |
|---|---|---|
| 1. ALTER TABLE | H6, H7, M8, M9, M10, M11 | the ALTER paths of `inillucent-engine` and the catalog rewrite |
| 2. Query results | H1, H2, H3, H4, H9, M2, M3, M5, M14, and `printf('%.*c')` | binder, planner, executor, scalar functions |
| 3. Catalog and PRAGMA | H5, M6, M7, M15, L6, L8 | PRAGMA handling and the `pragma_*` table valued functions |
| 4. Writes, views and triggers | H8, M1, M4, M12, M13 except `printf`, L4, L5, numbered parameters | DML, triggers, upsert, statement checks, the driver's parameter names |
| 5. Messages, dates, JSON and shell output | M16, M17, M18, L1, L2, L3, L7, `.parameter list` | error text, the date and JSON functions, the shell |

When all five are merged, the known list holds only `deviation` rows, each package's own new cases
match, and `inillucent-testrun --changed origin/main` passes.

## 7. What was done, and how it was verified

### 7.1 Rounds

| Round | Packages | Corpus after the round |
|---|---|---|
| start | the harness and four shell fixes | 420 of 563 match |
| 1 | ALTER TABLE; query results; catalog and PRAGMA; writes, views and triggers; messages, dates, JSON and shell | 1,343 of 1,538 |
| 2 | joins and subqueries; generated columns; keys, triggers and conflicts; collation, aggregates and numbers; DDL and schema; triggers on views lost at reopen | 1,680 of 1,730 |
| 3 | outer aggregates in subqueries and the merge regressions; everything else left | 1,729 of 1,746 |

Each package was one Sonnet agent in its own worktree. Packages that changed the same function
were merged by keeping the version of the package that owned the defect group, and porting the
other package's additions into it. Every merge was measured against the whole corpus before it was
committed; a case that matched before a merge and differed after it was either fixed in the merge
or listed and given to the next round.

### 7.2 The held out check

The 835 held out cases were not read by any round 1 agent. After round 1 merged, 61 of the 268 held
out cases that differed at the start matched, and 1 that had matched differed. That one was a real
regression: a round 1 change gave a compound subquery's column no affinity when its arms disagree,
which made a `DELETE ... WHERE x IN (SELECT x FROM (... UNION ALL ...) WHERE x > 7)` delete rows
SQLite keeps. SQLite pushes the outer `WHERE` into each arm, so each arm compares with its own
affinity; round 2 implemented that push down. After round 1 the held out files joined the training
set.

### 7.3 Defects found by the regression runs, not by the corpus

The full `inillucent-testrun --changed origin/main` run after each round found three things the
corpus could not, because each needs a second session or a measurement:

- `INSTEAD OF` triggers on a view were dropped when the catalog was loaded from the file, so a view
  that accepted writes in the session that created it refused them after a reopen. The catalog load
  skipped every trigger whose table has no tree. A test that closes and reopens now covers it.
- Two extra allocations per compile of any statement using `?N`, and later one more for `SELECT 1`.
  Both were removed; the bounds were not raised.
- Functions and modules that grew past their recorded sizes in merges were split.

### 7.4 What still differs

`compat/corpus/usage/known.toml` holds 17 rows. 14 are `deviation`: the 32768 byte page and the
cache default, which change `page_size`, `page_count`, `freelist_count`, `max_page_count` and
`cache_size` readings; `busy_timeout` defaulting to 5000; `sqlite_schema.rootpage` numbers; the
ascending storage order of a WITHOUT ROWID table with a DESC primary key, seen only by a query with
no `ORDER BY`; and `DELETE` and `UPDATE` with `LIMIT`. `docs/sql.md` lists each. 3 are `defect`
and differ only in `EXPLAIN QUERY PLAN` text: inillucent prints a flat plan and has no subquery
flattener, where SQLite prints a tree with `MATERIALIZE`, `CO-ROUTINE` and `COMPOUND QUERY` nodes.
The rows those queries return match.

Smaller differences the agents found and did not fix, with no corpus case yet, are listed in
task-2176: the last digit of `sinh`, `cosh`, `tanh`, `asin` and `acos` for a few percent of
arguments; a window `sum` after an overflow; `reverse_unordered_selects` on inner join terms; a few
date combinations with out of range numbers and `localtime` across a daylight saving change; and the
`stop IN ('3')` case of a table valued function's hidden column.

### 7.5 Verification

- `differential::usage_corpus`: 1,746 cases, 1,729 identical to the pinned shell, the other 17
  listed. The suite fails on any new difference and on any listed case that matches.
- `inillucent-testrun --changed origin/main --strict` over the merged tree: every test passed. The
  suites reported as not evidenced are the ones the development machine cannot run (no network, no
  PostgreSQL, MySQL, Go or openssl), as on `main`.
- The documentation tooling suites, `doc-style` and `doc-facts` pass after the documentation pages
  were updated.
