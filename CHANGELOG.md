# Changelog

Every released version, what it was for, and what it is known not to do. The
dates are the dates the release was cut.

The version is the workspace's, which every package carries: the command line,
the shell, the MCP server, the migration tool, the C ABI library, and the Go,
npm, PyPI and Composer wrappers are all one number. `tools/doc-facts/check.mjs`
fails the build when any copy of it disagrees.

## Unreleased

**Faster than SQLite in 19 of 25 workloads called the way most programs call it.** Measured on 25
workloads (one `inillucent` process per command, a SQL script piped into `inillucent-shell`, the
Python driver and the npm package) against the same use of SQLite 3.53.4, the build after 2.1.5's
changes was 51% slower and this one is 46% faster. `docs/performance.md` has every workload. The
changes:

- The C library is a Node addon, and the npm package runs `query`, `exec` and `batch` in the Node
  process. `query()` keeps a file open for the next read of the same file and closes it one second
  after the last.
- The Python driver gets results as Python objects and sends `execute_many`'s rows as Python
  objects, through four new provisional C calls, `inillucent_py_init_params`,
  `inillucent_py_params`, `inillucent_stmt_execute_params` and `inillucent_params_free`. ABI 1.3.0.
- A database is opened from the handle that created it. On Windows the second open waited for the
  virus scanner, about 6 ms of every program that created a database.
- An `INSERT` of many rows of literals, a prepared `INSERT` run many times, a run of single row
  inserts in the shell and `.import` insert their rows as one statement, with an exact fallback to
  one row at a time when a row fails.
- An open reads the file's first pages once, opens the log segment once and reads the schema at
  the first statement. A new file is written with one sync, and a fold under a rollback journal
  syncs the data file once.
- An index entry for a new row searches only the rows added since its leaf was packed, and the leaf
  hint covers the leaf's whole key range.
- The allocators keep blocks under 8 bytes and from 4 KiB to 64 KiB, and the Windows programs load
  `bcrypt.dll` on first use.
- The Windows release is built with profile guided optimisation. `packaging/release-all.ps1` builds
  it instrumented, runs `packaging/pgo/train.mjs` against it and builds it again with the counts.
  Against the same commit built without a profile, Python inserts of 10,000 rows were 20% faster
  and 200 grouped aggregates 28% faster. `-NoProfile` builds it the old way.
- A page read into a buffer pool frame used for the first time is read into the frame's reserved
  memory, instead of into 32 KiB of zeros written first.
- An open asks the file system once whether a rollback journal is there. When the newer by name
  query answered that it was not, the older query was asked the same question again.
- An open reads the meta record without taking the shared lock, and reads it again under the lock
  only when the copy it read does not check out. The first statement takes the lock and checks the
  record as it did. On Windows the file's identity is read when something asks for it, not at every
  open. A Python open and close went from 40 us to 29 us.

**A third performance hill climb through the `Connection`.** Of the workloads the hillclimb and
contract plans run, 12 are still slower than SQLite 3.53.4, where 16 were on 2.1.3. Against 2.1.3,
the hillclimb plan's train set is 16% faster and its test set 18% faster, the contract plan's train
set 9% faster and its test set 5% faster, and a round of each plan uses 16% and 17% less processor
time. The changes:

- `length()` counts eight ASCII bytes at a time and finds the terminating NUL in the same pass.
  Copying and scanning a text column that calls it is about three times as fast.
- An autocommit statement asks Windows for the log's length with `GetFileSizeEx`. A point select
  outside a transaction went from 6.79 µs to 5.10 µs.
- A merge of two leaves packs both straight from their pages, the delta area is searched from the
  previous key's place, and a bulk `DELETE` reads a deleted row from the copy of its leaf. Deleting
  half of a table is 26% faster.
- A prepared `INSERT` keeps its plan, its declarations and its `VALUES` expressions between
  executions. Refilling a table row by row is 21% faster.
- A bulk `DELETE` or `UPDATE` keeps its keys as slices of one buffer.
- A literal `json_extract` path is parsed when the statement is compiled.

`docs/performance.md` lists the 12 workloads still slower than SQLite and why.

**Two ways a process killed while writing could damage a database that other processes had open.**
Both were found by running eight copies of the process storm with a 64 page pool at the same time,
where about one storm in sixteen failed. Readers reported `page 156 checksum 00000000`, a shared
extent slot the page did not hold, or a page that was not a blob extent, and one storm left a file
that failed `integrity_check` with `page 126 checksum 00000000`.

- A writer holding the file exclusively writes some pages into it before its fold finishes, with
  their old images in `<database>-journal`. When that writer was killed, other processes went on
  reading those pages, because the journal is put back only by a process that can take the RESERVED
  lock and a live writer may hold it. When a process did put the journal back, the file went back
  under the others, and neither the meta record nor the log had moved, so none of them read the file
  again. One of them could then fold without a page the journal had taken back, and move the
  checkpoint past the log records that would have rebuilt it. Putting a journal back now writes the
  meta record with its generation moved on, so every connection rebuilds its cache from the log on
  its next statement.
- An eviction under the exclusive lock wrote a page holding an open transaction's changes into the
  database file. If that writer was killed, other processes could read those rows before the
  journal was put back. Such a page now goes to the connection's own spill file, which no other
  process can read, and reaches the database file at the fold after its transaction commits.

Afterwards 446 of 448 storms passed. The two that failed read a page from before the journal was put
back in a statement that came after it, and the cause of that is not found yet.

**A second performance hill climb through the `Connection`.** On the 48 workloads of
`inillucent-fullgate --plan hillclimb` this release is 69% faster than SQLite 3.53.4, where 2.1.2 was
34% faster in the same day's run, with 22% less peak memory and 21% less processor time. On the
contract plan through the `Connection` its train and test sets are 16% faster than 2.1.2. Of the 82
workloads the two plans run, 17 are still slower than SQLite, where 29 were. The changes:

- A correlated `EXISTS` whose `WHERE` is an equality on an index is answered by probing the index
  for each outer row, and a correlated `IN` over two `NOT NULL` columns is lowered to one `EXISTS`.
  `x IN (SELECT y FROM t WHERE y = x)` went from 900% slower than SQLite to 10% slower.
- A correlated block's plan and chain are kept with the cached statement, and a statement holding
  one is kept as a compiled chain like any other.
- A scan reads only the columns the statement reads, including through a correlated block and on
  the inner side of an index nested loop join.
- A reader keeps a packed copy of a leaf that took writes once it has read it twice, until the page
  changes.
- A `GROUP BY` term in the select list is read from the group's key. It was also kept as a column
  of one of the group's rows, which copied every row.
- A hash `GROUP BY` keeps its keys in one table, and keeps a plain `count(*)` as one number a group.
  With the change above, a `GROUP BY` over 50,000 groups is 241% faster.
  A grouping by the outer term's walk streams through inner and left joins.
- A bulk `DELETE` logs one record per leaf it changes and reads a deleted row only when something
  needs it. A bulk `UPDATE` whose new value outgrows its slot repacks the leaf once.
- A recursive CTE step that reads only its queue is evaluated directly, without building a batch
  for a pass of one row.
- `json_extract` over distinct documents builds only the part of a document its path reads, and a
  JSON call over literals is answered once.
- `LIKE '%text%'` searches for the first byte before comparing.
- CRC-32 over a long piece runs four streams side by side and joins them, the same answer: 12.2 us
  to 3.9 us for a 32 KiB page.
- A window computes its values over its rows and moves them, where it copied every row five times.
- Compiling a statement no longer copies each table's schema into the plan.

`docs/performance.md` lists the 17 workloads still slower than SQLite and why.

**A statement outside a transaction makes one system call fewer.** Under `locking_mode = normal`
each such statement checks whether another process changed the database. The check read the meta
record from both meta pages and now reads it from the second page only. A checkpoint writes the
same record to both pages and now always writes the second page first, under a rollback journal as
it already did under the write ahead log, so a checkpoint that changed either page changed the
second one. On Windows the lock and the check are six system calls a statement where they were
seven. Through the `Connection`, a point select outside a transaction went from 4,866 ns to
3,967 ns, 23% faster, and the hillclimb plan's train and test sets were 7.5% and 8.6% faster.

**A `DELETE` or `UPDATE` of many rows changes each leaf once.** When no trigger, foreign key action,
`RETURNING` or correlated subquery can watch the statement, its rows are taken a leaf at a time: the
leaf is read once, each row still gets its own log record and undo entry, and the leaf is changed
once. An `UPDATE` does this for rows where one column outside the key changes and the new value fits
where the old one lies, and writes every other row as before. Through the `Connection`, deleting
40,000 rows by a rowid range is 35% faster (181% slower than SQLite 3.53.4 before, 107% after),
deleting half of a table with two indexes is 13% faster, and updating every row of a table is 13%
faster. Recovery and rollback read the same records as before. Under `OR FAIL`, `OR IGNORE` or
`OR REPLACE` an `UPDATE` keeps the order the query found its rows in, so it keeps the same rows as
before. Removing rows from a leaf's delta area now zeroes the bytes they leave, so a page recovery
rebuilds is the same as the page the session wrote.

**`INSERT ... SELECT` into an empty table builds the table in one pass.** When the target holds no
row and the query returns at least 1,024 rows, every row is built and checked first, then the table's
tree and each of its indexes are packed the way `CREATE INDEX` packs an index, with no page images
in the log. A copy of 100,000 rows went from 191 ms to 56 ms through the `Connection`, from 352%
slower than SQLite 3.53.4 to 35% slower, and with a plain and a unique index from about 715 ms to about 110 ms,
faster than SQLite's 132 ms. One round of the hillclimb plan used 5.3% less peak memory and 11% less
processor time. A failing statement reports the same error and the same `last_insert_rowid()` as
before, a rollback or a crash leaves the table empty, and a duplicate in a unique index hands the
statement back to the row by row insert. `CREATE TABLE ... AS SELECT` gets the same build.
`docs/relational-architecture.md` lists the shapes that still insert row by row.

**The three issues reported against 2.0.7, investigated.**

- **Damage after SIGTERM with a reader open** (`a reference names slot 5 of a shared page that
  holds 5`) is not reproduced on this release. A new storm case in the shape of the report rewrites
  values of 4 to 20 KB in autocommit statements, lets the readers write, and ends with every
  process killed at the same moment. It passes 12 of 12 rounds here and fails from its first round
  on 2.0.7, where processes are refused at open and one round met a replay error. Leaving out the
  log record of a shared page's image makes the case fail with the report's message word for word,
  which is the class of defect the multi process fixes after 2.0.7 removed.
- **A commit that finishes a segment merge no longer reads the merged segment and its inputs back.**
  It reread the accumulator it had just built and every input, only to learn which row ids each
  held. With the report's script the slowest commits went from 7.5, 7.4 and 6.2 s to 6.3, 6.2 and
  4.8 s on the development machine.
- **The 2,000 row publish commit is the fold the 4 MiB log bar triggers.** It writes the 2,384
  leaves the random rowids touched, and the data file sync is most of it. 2.0.4 did the same fold
  one publish earlier. Nothing is changed; smaller batches spread it, as the report found.

**A leaf split logs what it did instead of three whole pages.** The new `SplitLeaf` log record
carries the rows that moved, the key the parent gained and how many rows the leaf kept, and recovery
rebuilds each of the three pages from that page's own state, so a crash after any one of them
reached the file still recovers. A split record went from 98,384 bytes at a 32 KiB page to 5,148 on
average for 2,000 inserts into a table with two indexes, and that workload's log from 1,553.9 KiB to
734.4 KiB. Over the hillclimb plan through the `Connection`, 20,000 inserts into a table with two
indexes are 11.7% faster, appended inserts 14.9% faster, and growing updates of existing rows 52.9%
faster, with the held out workloads 2.9% faster overall and processor time 3.8% lower. A root split,
a split whose parent is full and a split that moves a value stored outside the page still log the
pages whole. A build of 2.1.2 or earlier refuses a log holding this record, which only arises when a
connection stopped without closing.

**Two wrong answers fixed, found by a new performance plan.**

- **`WHERE g = ? ORDER BY c DESC LIMIT n` over an index on `(g, c)` returned rows from the middle
  of the group**, or fewer than `n` rows, when the group's entries crossed a leaf boundary: the
  backward walk started at the leaf where the group begins. It now starts past the group's last
  entry and applies the upper bound on every leaf.
- **`LIMIT n OFFSET <expression over a parameter>` kept the first execution's offset** when the
  statement was prepared once and bound again, so every page after the first repeated it.

**One more wrong answer fixed, and the last SQLite differences in the usage corpus that were
defects.**

- **A `WHERE` term on the side a `RIGHT` or `FULL` join null extends** became a search on that
  table, and the term was not tested again after the join: `s RIGHT JOIN r ON r.a = s.a WHERE
  s.k = 1` over an empty `s` returned every row of `r`. The term is tested after the join now, and
  a join the `WHERE` keeps from null extending is planned as an inner join, as SQLite plans it.
- **An FTS5 `MATCH` written after a rowid equality** (`WHERE rowid = 1 AND f MATCH 'x'`) returned
  no rows, because the module took the rowid lookup and the match was then tested outside it. The
  module now takes the match wherever it is written, and the rowid is tested against the matches.
- **`EXPLAIN QUERY PLAN` prints SQLite's tree**: `CO-ROUTINE`, `MATERIALIZE`, `COMPOUND QUERY`,
  `MERGE`, `LIST SUBQUERY` and the other nodes, with parent ids. Derived tables, views and CTEs are
  flattened into the query where SQLite flattens them, and a `LEFT JOIN` the `WHERE` makes an inner
  join is planned as one.
- **A derived table in a correlated subquery is run again for each outer row** where SQLite runs it
  as a co-routine, so an `UPDATE` that reads the table it changes sees what SQLite sees.
- **`sinh`, `cosh`, `tanh`, `asin` and `acos` on Windows** return the last digit SQLite's Windows
  shell returns.
- **A sliding window `sum`, `total` or `avg`** stays infinite after an overflow, as in SQLite.
- **`PRAGMA reverse_unordered_selects`** reverses joined rows and the rows of a subquery.
- **Date and time functions** follow SQLite for raw numbers, `'subsec'`, out of range days and the
  `'localtime'` and `'utc'` edge cases.
- **`generate_series`** converts its own constraint values (`stop IN ('3')`) and tests an `IN` list
  on its columns as SQLite does.
- **A `STRICT` table with a generated column** reports a type error before a `NOT NULL` error, in
  SQLite's order.

**Faster for the statements an application sends.** Measured through the `Connection` API against
SQLite 3.53.4 with `inillucent-fullgate --plan hillclimb --api connection`, a plan of 48 workloads
over tables built with `INSERT`, the same tables after updates and deletes, common application
queries and edge cases (see `docs/performance.md`). Released in 2.1.2. Against 2.1.1, over that
plan:

- the 28 workloads the changes were tuned on are 53% faster: 18% slower than SQLite before, 30%
  faster after;
- the 20 held out workloads are 71% faster: 49% slower than SQLite before, 15% faster after;
- one round's peak memory is 18% lower, 100.96 to 82.82 MiB;
- one round's processor time is 55% lower, 3,906 to 1,750 ms, which is now 7% less than SQLite's;
- the contract plan through the `Connection` is 22% faster: 94% faster than SQLite before, 137%
  faster after.

The changes:

- **An insert that lets the engine choose the rowid** read every live row of the last leaf to find
  the largest one. It reads two values now: 19.0 to 2.6 microseconds for a single row insert, and 20,000 inserts
  into a table with two indexes are 246% faster.
- **An `OR` whose every arm can use the rowid or an index** is one search per arm, read by rowid once
  each, where it scanned the table: 100 queries of `WHERE key = ?1 OR id = ?1` over 100,000 rows
  take 0.88 ms where they took 588 ms. SQLite takes 1.38 ms. `EXPLAIN QUERY PLAN` prints `MULTI-INDEX OR`.
- **`IN` and `NOT IN` over eight or more values**, written out or returned by a subquery, use a
  binary search: two queries of `NOT IN (SELECT ...)` over 25,000 rows take 4.44 ms where they
  took 958 ms. SQLite takes 5.01 ms.
- **A query that reads no table** (`SELECT 1`, `SELECT json_extract(?1, '$.a')`) takes no file
  lock under `locking_mode = normal`, as in SQLite. Through a `Connection`, 4,000 `SELECT 1`
  statements take 0.82 ms where they took 23.3 ms, and SQLite takes 1.62 ms.
- **A recursive CTE** compiles its step once instead of once per pass: 211% faster on a
  10,000 pass counter.
- **A `GROUP BY` with many groups** keeps its accumulators in one vector and each accumulator is
  144 bytes: 89% faster on 50,000 groups, and a lower peak memory.
- **`LIKE '%text%'`, `'text%'`, `'%text'` and plain text** compare in place without the general
  matcher: 160% faster on a scan of 100,000 rows.
- **Bulk `DELETE` and `UPDATE`** look first in the leaf the last row used, and a scan of a table
  that took writes fills its columns without a vector per row: deleting half a table is 56%
  faster, an `UPDATE` of every row 69% faster, and a counter upsert 145% faster.
- **`INSERT ... SELECT` no longer copies the query's rows**, which lowered that statement's rise in
  peak memory from 65 to 46 MiB on 100,000 rows and made it 23% faster.
- **`json_extract` over a column of distinct documents** stops keeping each document after a few
  misses in a row, 9% faster.

**A usage corpus of 1,719 application scripts, and the defects it found, fixed.** The new suite
`differential::usage_corpus` runs the scripts in `compat/corpus/usage` through `inillucent-shell` and
through the pinned `sqlite3` 3.53.4 shell and compares the output byte for byte, including column
names, storage classes and error text. The scripts are the statements applications send: the pragmas
a driver runs on connect, the catalog queries an ORM runs to read a schema, a migration's table
rebuild, ordinary create, read, update and delete code, and the error messages those tools parse.
`compat/corpus/usage/known.toml` lists the cases that still differ, each as a `defect` or a
`deviation`, and the suite fails on a case that differs without an entry and on an entry whose case
now matches. `INILLUCENT_USAGE_REPORT=<file>` writes every difference to a JSON file.

The shell:

- **With `.headers on`, a statement that returns no rows prints nothing.** It printed an empty line.
- **`.mode insert` with headers on names the columns**, as in `INSERT INTO tab(a,b) VALUES(...)`, and
  quotes a name only where SQLite's shell does.
- **`.headers` is a choice that survives `.mode`.** Without one, the `column`, `table`, `box`,
  `markdown` and `html` modes show headers and the other modes do not. With headers off, the drawn
  modes size columns by their values and draw no header.
- **`inillucent export` of an empty result still writes the header line.**
- **`.mode quote` prints text that holds control characters as `unistr('...')`.**
- **Input runs in chunks the way `sqlite3_complete` splits it.** A parse error abandons the rest of
  that input line, and the excerpt and caret lines under an error match SQLite's.
- **`.parameter set` evaluates its value as an expression**, so `NULL` is `NULL`, and
  `.parameter set ?2 20` binds `?2`.

Schema changes:

- **`ALTER TABLE ... RENAME TO` rewrites `REFERENCES` in other tables, table qualifiers in views,
  trigger bodies, and temporary triggers and views.** `RENAME COLUMN` rewrites trigger bodies
  (`new.col`), `UPDATE OF` lists, upsert targets, partial and expression indexes and child
  `REFERENCES t(col)`. Both check every dependent view and trigger first and fail with SQLite's
  `error in view v: ...` messages. A name that starts with `sqlite_` is refused.
- **`PRAGMA legacy_alter_table` can be set and changes `RENAME` as in SQLite.**
- **`ALTER TABLE ADD COLUMN` refuses a default that is not constant, a `CURRENT_TIMESTAMP` default,
  and a `REFERENCES` column with a default that is not `NULL` when the table has rows.** The new
  column is written before the table constraints in the stored SQL.
- **`ALTER TABLE DROP COLUMN` follows SQLite's rules and messages**, and cuts the stored SQL as
  SQLite does.
- **The SQLite 3.53 forms `ALTER COLUMN c SET NOT NULL`, `ALTER COLUMN c DROP NOT NULL`,
  `ADD CONSTRAINT n CHECK (...)` and `DROP CONSTRAINT` run.**
- **`CREATE VIEW` stores the body without resolving it.** An error in the body, including a column
  list of the wrong width, appears when the view is read.
- **`CREATE TABLE` refuses what SQLite refuses at create time**: an unknown collation, a
  `DEFAULT (nosuch)`, a duplicate `WITH` name, a qualified table name in a trigger body, and two
  primary keys. A persistent view cannot name another database.
- **`DROP TABLE` and `DROP INDEX` remove the table's `sqlite_stat1` rows**, and SQLite's own tables
  and constraint indexes cannot be dropped.
- **`PRIMARY KEY(a AUTOINCREMENT)` at table level and quoted type names parse.**

Writes:

- **`UPDATE` and `DELETE` on a view run its `INSTEAD OF UPDATE` and `INSTEAD OF DELETE` triggers**,
  including `UPDATE ... FROM`. `INSERT ... ON CONFLICT` on a view fails with `cannot UPSERT a view`.
  Triggers on views are still there after the file is closed and opened again.
- **`ON CONFLICT` may name a partial unique index or an expression index.** The `WHERE` must match
  the index predicate. The capability rows `on_conflict_partial_index` and
  `on_conflict_expression_index` changed from `no` to `yes`.
- **Upsert `DO UPDATE` runs the `CHECK`, `NOT NULL` and `STRICT` checks and fires the table's
  `UPDATE` triggers.**
- **`REPLACE` and `UPDATE OR REPLACE` run the foreign key actions of every row they remove.** Delete
  triggers fire for those rows only with `recursive_triggers` on.
- **A `BEFORE UPDATE` trigger's change to the same row is kept**, and constraints are checked after
  the `BEFORE` triggers. The rowid of a new row is chosen after the `BEFORE INSERT` triggers, and a
  `BEFORE INSERT` trigger sees `-1` for an unassigned rowid.
- **A trigger body inherits the conflict action of the statement that fired it.**
- **`UPDATE t SET id = id + 1` visits rows in rowid order** and fails with the `UNIQUE` error where
  SQLite does.
- **A column assigned twice in `UPDATE` takes the last value**, and a column listed twice in `INSERT`
  follows SQLite's rule.
- **A foreign key compares with the parent column's collation**, and a row may reference itself.
- **`INSERT ... AS alias` works for upsert**, `last_insert_rowid()` works inside `RETURNING`, and
  the `sqlite_sequence` rows for `AUTOINCREMENT` follow SQLite.

Generated columns:

- **Virtual and stored generated columns follow SQLite**: their result names, the declared affinity
  and collation, `STRICT` checks, `NOT NULL`, `UNIQUE` and `ON CONFLICT`, foreign keys as parent or
  child, real values in `OLD`, `NEW` and `excluded`, and `NULL` for the missing row of an outer join.

Queries:

- **`ORDER BY` is applied when an `IN` list is answered from an index**, and `LIMIT -1 OFFSET n`
  keeps the offset in every query shape.
- **`x IS TRUE` is true for any number that is not zero.**
- **A scalar subquery keeps its column affinity.** A compound subquery column has an affinity only
  when its arms agree, and `WHERE` terms over a compound subquery are pushed into its arms as SQLite
  does.
- **Recursive CTEs accept `ORDER BY`, which orders the queue, and `LIMIT` and `OFFSET`.**
- **A result column alias may be used in `WHERE`.**
- **`(a, b) IN (SELECT ...)` runs.** The capability row `row_value_in_subquery` changed from `no` to
  `yes`, and the example of an unbuilt construct in the docs is now `ATTACH ? AS other`.
- **`SELECT *` on a rowid table walks the table in rowid order** unless a narrower covering index
  exists.
- **View and `CREATE TABLE ... AS SELECT` column names follow SQLite**: an expression such as
  `a + b` is named by its text, and duplicate names get `:1` and `:2` suffixes.
- **A constant false `WHERE` is tested once before any scan**, so `generate_series(1) WHERE 0`
  returns at once. `generate_series` with a `NULL` argument returns no rows, and a negative step
  works.
- **`count()` with no argument is valid.** `sum`, `total` and `avg` of overflowing reals give `Inf`,
  and bare columns next to `min` and `max` follow SQLite's rule.
- **`IN` lists take affinity and collation from the left operand.** `COLLATE` on an `ORDER BY`
  ordinal or alias works, and a compound takes its collation from the leftmost arm that has one.
- **`printf` supports `*` for width and precision**, and an unknown conversion returns `NULL`.
- **Infinity prints as `9.0e+999` in `quote()` and the JSON constructors**, and numeric literals
  accept `_` separators.
- **`PRAGMA reverse_unordered_selects` reverses the outer scan.**

Pragmas and the catalog:

- **A `pragma_*` table valued function can take its argument from an earlier `FROM` term**, as in
  `FROM sqlite_master m, pragma_index_list(m.name)`. This is how EF Core and Rails read a schema.
  `pragma_foreign_key_check` exists, and `PRAGMA foreign_key_check` reports rows whose parent table
  does not exist.
- **`PRAGMA foreign_keys` inside a transaction does nothing**, and `BEGIN IMMEDIATE` fails under
  `query_only`.
- **`mmap_size`, `wal_autocheckpoint`, `journal_size_limit`, `temp_store = FILE`,
  `synchronous = EXTRA`, `threads`, the heap limits, `cache_spill` and the flag pragmas are stored
  and read back as SQLite reports them.** They do not change how inillucent runs: it maps no pages,
  has one thread, and folds the log at checkpoints.
- **`index_list` reports the origin `u`, `pk` or `c`.** `foreign_key_list` numbers its rows and names
  the implicit parent column as SQLite does, `table_list` and `database_list` report schema names,
  `function_list` reports the type `w`, and standard type names are upper case.

Messages and functions:

- **Error messages and error classes match SQLite's text across the corpus.** That includes
  `datatype mismatch`, which printed a Rust debug value, `misuse of aggregate function count()`,
  `table t has 2 columns but 3 values were supplied`, `UNIQUE constraint failed: index 'i'` for an
  expression index, and `no such table: main.x` where SQLite names the schema.
- **Date and time functions follow SQLite's `date.c`**: negative years, the `+YYYY-MM-DD HH:MM:SS`
  modifier, fractional months and `subsec`.
- **JSON functions handle malformed paths and wrong argument counts as `json.c` does.**

Joins and subqueries:

- **`WHERE` terms are pushed into the arms of a compound subquery**, as SQLite does.
- **`RIGHT` and `FULL` joins after an inner join keep unmatched rows**, and parenthesised joins work.
- **Subqueries work in a join's `ON`, in `RETURNING` and in the `SET` and `WHERE` of an upsert.**
- **`INSERT ... VALUES ... UNION ALL VALUES` inserts every row**, and `LEFT JOIN json_each(NULL)`
  keeps the left row.
- **Rows produced before a run time error are returned before the error.**
- **`AND`, `OR`, `coalesce`, `ifnull` and `iif` skip the operands SQLite skips**, and an `EXISTS`
  select list is never evaluated.
- **A CTE used twice is evaluated once when it calls a volatile function**, and a subquery that calls
  `random()` is evaluated for every row.
- **A subquery with several columns works as a row value**, and fails with
  `sub-select returns N columns - expected 1` where one value is needed.
- **A view in `main` no longer reads a temporary table that shadows its base table.**
- **`INDEXED BY` a partial index works where SQLite accepts it.**
- **`RETURNING` is evaluated right after each row is written, before `AFTER` triggers.**
- **An aggregate whose arguments read only the outer query belongs to the outer query when it is used
  inside a subquery**, as in `SELECT (SELECT max(t.a)) FROM t`.

Errors and ordering:

- **`ORDER BY` or `LIMIT` before a compound operator is an error with SQLite's wording**, and a
  circular CTE fails with `circular reference: name`.
- **`GROUP BY` takes the direction of an `ORDER BY` with the same number of terms**, which sets the
  order of tied groups.
- **`CREATE TABLE ... AS` stores the query's values without applying the new column's affinity.**
- **`MATCH` on a plain column fails for each row**, an unknown function's error keeps its name as
  written, and `NULLS FIRST` and `NULLS LAST` on an index key are refused as in SQLite.
- **A `WITHOUT ROWID` table with a `DESC` primary key term answers range predicates and `ORDER BY`
  correctly.** They returned no rows before.
- **`PRAGMA page_size = n` prints nothing**, and `PRAGMA cache_size` reads a value outside 32 bits
  as 0.
- **`EXPLAIN QUERY PLAN` prints attached schema names, automatic index lines and `LEFT-JOIN` markers**
  as SQLite does. It still prints a flat list, with no `MATERIALIZE`, `CO-ROUTINE`, `MERGE` or
  `COMPOUND QUERY` blocks.

JSON and dates:

- **JSON5 input accepts Unicode whitespace**, and an unpaired surrogate escape is read as SQLite
  reads it.
- **A blob that is not JSONB is read as JSON text**, and `json_each` and `json_tree` give SQLite's
  `id` values at a path.
- **A date function accepts a blob as the format or a modifier and `subsec` as the time value**, and
  `timediff` works in both directions.

Differences from SQLite that stay, each listed in `known.toml` as a `deviation`: `DELETE ... LIMIT`
and `UPDATE ... LIMIT` are accepted, because the pinned `sqlite3` is built without
`SQLITE_ENABLE_UPDATE_DELETE_LIMIT` and many distribution builds enable it. `page_size` defaults to
32768 and `cache_size` to -131072, and `PRAGMA page_size` before the first write, and
`PRAGMA aux.page_size`, are ignored, because the page size is part of inillucent's own file format.
`page_count`, `freelist_count` and `max_page_count` count the larger pages, `busy_timeout` defaults to
5000, `sqlite_schema.rootpage` numbers differ, and a `WITHOUT ROWID` table with a `DESC` primary key
returns rows in ascending key order when a query has no `ORDER BY`.

**Eleven defects found by new tests that run several real processes against one file, and fixed.**
Four workers and two readers open one file, commit seeded transactions, and are killed at random
moments; a checker rebuilds every acknowledged transaction from its seed and compares the file byte
for byte. On 2.0.7 its first minute found the first four defects below with no kill at all. Each
defect below has a test that fails without its fix.

- **A checkpoint or a backup called on the database handle undid another process's commit.**
  `Database::checkpoint` and `backup_to` folded this process's pages as they stood, without first
  catching up with the log, so rows another process had deleted came back and an extent page could
  be overwritten with a leaf. They now take the lock the way a statement does.
- **Opening a file while another process held a write transaction failed with `busy` at once**,
  whatever busy timeout was set. The open tried to fold the pages it replayed, and the writer's
  lock refused it. The open now leaves that fold to the writer.
- **Opening with a small page cache beside a writer failed with `every frame in the buffer pool is
  pinned`.** The open's replay had no spill file to put pages in. It has one now.
- **`BEGIN` took the write lock, so a read transaction in one process failed with `busy` while any
  other process had a transaction open.** `BEGIN` and `BEGIN DEFERRED` now take no lock, as in
  SQLite, and the first statement inside the transaction takes the lock it needs. `BEGIN IMMEDIATE`
  and `BEGIN EXCLUSIVE` still take the write lock at once. A transaction that reads first and then
  writes is refused with `busy` when another process committed in between, as in SQLite.
- **Making `BEGIN` deferred had been tried before and lost acknowledged inserts.** The cause was the
  transaction manager raising the lock on its own after the statement had checked for other
  processes' commits, so a transaction wrote from stale pages into a log segment the next recovery
  started after. It no longer raises the lock. Two writer processes running 60 inserts each lost up
  to 12 of 120; they lose none in 8 runs now.
- **An open kept its lock after it finished**, so the first statement of the first transaction
  trusted the open's replay, and a commit could be written over another process's committed
  records in the log. Every later recovery stopped at those bytes, losing acknowledged transactions
  and leaving indexes that disagreed with their tables. An open now releases the lock when it is
  done, and writes the file itself only when the log has not moved since its replay.
- **A transaction could reuse a number another process had used**, which made the records of a
  killed process's unfinished transaction look committed: readers saw part of a transaction that
  never committed. A transaction that has written nothing is renumbered when its first statement
  catches up with the log.
- **A full disk made a connection refuse every statement until it was closed.** The log stays
  refused for the statement that met the failure, and the next statement rebuilds it from the
  files, so the same connection writes again once space is free.
- **A file the process may not write could not be opened, even to read.** It now opens read only:
  reads work and a write fails with the status `readonly`. A connection opened read only also
  refuses a write itself now, where it used to report one row changed and keep it only in memory.
- **`inillucent batch` and `inillucent analyze` reported `busy` as `syntax`**, and the driver
  reported out of memory as `syntax`. They report `busy` and `too_big`.
- A commit of a transaction that wrote nothing no longer appends a commit record to the shared log
  without the write lock.

New suites: `durability::process_storm` (the storms above, with a 64 page pool, the search table
and an encrypted file), `durability::process_interleavings` (one test per defect above, each the
smallest sequence that shows it), `durability::file_damage` (a copy taken during a write, a removed
or foreign log segment, and damage at every page), `durability::environment` (a full disk, a read
only file, one file under several spellings of its path, and unusual paths), and
`nightly::storm_nightly`. The program they drive is `inillucent-chaos`.

**Six problems a user of 2.0.4 reported, fixed, and five more found while fixing them.** Each was
reproduced first, with the user's own scripts where there were scripts, and every number below was
measured with the same script before and after.

- **A file could be damaged after a process was stopped, when another process had it open.** A
  `ROLLBACK`, or a statement undone after it failed, puts rows back through the B+trees, so the page
  splits the transaction made stay in place, and the next transaction writes into those pages. No
  commit record followed the undo, so a replay of the log skipped the transaction and its undo and
  did not know those pages existed. The replay that goes wrong is the next open after a crash, or
  another process catching up on the log; both happen when a writer is stopped while a reader keeps
  it from folding. The next open failed with `read 0 of 32768 bytes at` the file's size, or applied
  rows to an older copy of a page (`row 7766 missing from index`), and a fold then wrote that into
  the file. Reproduced in one process with no signal at all: commit, roll back, commit, exit without
  closing, and 2.0.4 cannot open the file. A finished rollback now ends with a commit record, so the
  replay applies the transaction and its undo together. Killing a background job and a search
  service at random moments, in the shape of the user's workload, left a file that would not open in
  5 of 19 rounds on 2.0.4 and in none of 12 rounds now.
- **A connection whose replay of the log failed part way could fold what it had replayed.** It now
  drops every page the failed replay produced, so the file never takes a half applied log.
- **A reader waited for a write transaction larger than about 50 MiB, and then failed with `busy`.**
  A transaction whose changed pages outgrew the writer's page cache wrote them into the database
  file early, which needs the EXCLUSIVE lock, and kept that lock until it committed. Such a page now
  goes to a spill file the writer owns: a temporary file deleted when it is closed or when the
  process ends, encrypted with a key of its own when the database is encrypted. The database file is
  written only by a fold. With the user's script, a reader beside transactions of 10,000 to 60,000
  rows of 3 KB values reads in 0.04 to 0.23 s where it failed after its three second timeout from
  20,000 rows on.
- **Deleting rows from a search table by rowid was slow.** `rowid IN (...)` on a virtual table read
  every row and tested it against the whole list. The list is now offered to the module as `=` and
  the module runs once per value. Deleting 2,000 rowids of 120,000 took 4.94 s and takes 0.12 s.
- **`id IN (...)` with more than 512 values scanned the table**, on any table. The union of seeks
  that answers a list was built for at most 512 values, and a longer list tested every row against
  all of them: 2,000 ids on a table of 120,000 rows took 3.85 s and take 0.066 s. A list of up to
  100,000 values is now sought by key. This is most of the time the user's `INSERT INTO ...
  SELECT ... WHERE c.id IN (...)` took, which went from 4.29 s to 0.114 s on the same shape.
- **A reader's first search after another process wrote the search table was slow.** The reader
  applied each changed row to its cached index one at a time, one graph insert at a time on one
  core. The changes now go in as one batch on every core: 3.26 s became 0.52 s after 2,000 changed
  rows, and 0.94 s became 0.15 s after 500.
- **With default options, some commits to a search table took 30 to 40 seconds.** Three causes.
  Loading one segment read every row of `%_gen`, each a blob of up to 512 KiB, about 1 GB on a table
  of 120,000 rows of 768 numbers; a segment is now read by rowid from a map the table keeps in
  `%_state`. Folding a segment into a merge inserted one document at a time on one core; it is one
  batch on every core now. And one commit could fold twice its `merge_budget`; a second fold now has
  to fit what is left. With the user's script the commits that merge took 13.7, 17.7 and 21.3 s on
  this machine and take 2.1, 2.8 and 3.3 s. The first search in a new process over a table of
  120,000 rows held in many segments took 420.6 s and takes 24.2 s.
- **`drop-old-generations` made a search table unreadable.** A merge that takes more than one
  commit leaves a segment stored as a chain of links, and the command kept only the last link: every
  later query failed with `segment N is unreadable`. It keeps every link of every live segment now.
  `rebuild` repairs a table an earlier version damaged this way.
- **On Windows, a file larger than 1 GiB that two processes had open failed reads with `disk I/O
  error`.** The locking protocol locks bytes at 1 GiB, as SQLite does, and Windows stops every other
  handle from reading or writing locked bytes. SQLite never puts data in that page. inillucent did;
  the free map now never hands that page out. A file an older release wrote keeps what it already
  stores there.
- **`inillucent setup-embeddings model --from <folder>`**, and the same for `reranker`, installs
  from a folder already on the machine, for a network that cannot reach Hugging Face. Each file is
  found by its installed name or by its path in the repository, such as `onnx/model.onnx`, and is
  checked against the size and SHA-256 this release pins before anything is copied. One file missing
  or different and nothing is installed.

The new durability suite `large_transactions` runs a reader beside a transaction larger than the
page cache, and kills a writer after a rollback and a commit. Both fail on 2.0.4.

**The GitHub tests ran again, and a release now waits for them.** From 2026-09-24 the tests
workflow was not valid YAML, so GitHub ran no job on either repository, and 1.0.30 to 2.0.4 were
released without a CI result. The workflow parses again, and what the week without CI let through is
fixed:

- **`x IN (SELECT ...)` against a REAL column matched as SQLite does.** SQLite tests membership in
  an index that stores an integer as a real, so `9223372036854775806 IN (SELECT a FROM t)` is true
  when the REAL column `a` holds 9223372036854775807. inillucent compared the integer exactly and
  answered false. The correlated form had the same difference.
- **A compound `IN` took its rules from the wrong arm.** `'7' IN (SELECT r FROM t UNION ALL SELECT
  'x')` used the REAL affinity of the first arm. SQLite uses the last arm, and so does inillucent
  now, for the collation as well.
- **`PRAGMA data_store_directory` on Linux and macOS answers no column**, as SQLite does there. The
  pragma exists only in SQLite's Windows build.
- **`LIKE`, `GLOB` and `quote()` end text at its first zero byte**, as SQLite does, which reads
  them as C strings. `('1' || x'00ff') LIKE '_'` is 1 now.
- **A function takes its collation from the first argument that has one.** `min(CASE ... END, b)`
  with `b` declared `COLLATE NOCASE` compared with BINARY before.
- **`a GLOB '1*'` over an index on a column with no declared type finds an integer 1.** A pattern
  becomes an index range only over a column with TEXT affinity, as in SQLite.
- **`LIMIT 0` evaluates nothing.** A WHERE clause that would fail on a row, such as `abs()` of the
  smallest integer, failed the statement before.
- **A connection that was already open removes an empty journal a killed writer left.** On Linux
  the writer can die after it creates `<database>-journal` and before it writes the header, and the
  empty file stayed until the next process opened the database.
- **A release is not finished until GitHub's tests run of it passes.** `ship.ps1`'s new last route,
  `ci`, starts the tests workflow on the public mirror once the release is there and waits for it.
  `packaging/ci-status.ps1` runs that check on its own.
- **CI runs only on the public mirror, at night.** Both workflows run at 03:00 UTC and by hand, in
  Black-Rainbow-Labs/Inillucent only, and split the suite across jobs with the new
  `inillucent-testrun --shard <k>/<n>`: four on Windows and three on Linux for the tests, four for
  the nightly.

**Nine problems a user of 2.0.3 on macOS reported, fixed.**

- **`$1` and `$2` bind by their number.** `SET a = $2 WHERE id = $1` used to bind the first value to
  `$2`, because `$N` was numbered by the order it appeared, which is SQLite's rule for a name. It
  matched no row and reported no error. `$N` now binds the Nth value, as `?N` does and as PostgreSQL
  does. The driver conformance suite has a case for it, so every language binding checks it.
- **A facet filter with `=` dropped rows after many small commits.** Merging a search table's
  segments rebuilt each row without its facet values, so `src = 'slack'` missed the merged rows
  while `src IN ('slack')` found them. Merges now keep the values, and a table written by an earlier
  release has the missing values read back from its rows when a segment loads, without a `compact`.
- **A process that used `embed()` crashed as it exited.** On macOS it aborted with exit code 134
  under the `idle` and `resident` profiles; on Linux it crashed under `on-demand`. Both came from
  ONNX Runtime being torn down in the wrong order. A loaded model is now dropped before ONNX
  Runtime's own exit code runs.
- **`embed()` failed from the command line on macOS.** The programs are signed with the
  `com.apple.security.cs.disable-library-validation` entitlement, so they can load ONNX Runtime,
  and the release refuses a program without it. A runtime that will not load is now an error that
  names the file, not a panic.
- **`inillucent setup-embeddings` aborted on macOS before downloading anything.** It loaded the
  system's private OpenSSL, which macOS refuses. Downloads on macOS now go through `/usr/bin/curl`,
  and the digest of every file is still checked before it is installed.
- **An open can wait as long as the caller says.** `INILLUCENT_BUSY_TIMEOUT` in milliseconds,
  `OpenOptions::busy_timeout` in Rust, `inillucent_open_with_timeout` in C (ABI 1.2.0) and
  `busy_timeout_ms` in Python set how long an open waits for a file another process is writing.
  The open used to fail after five seconds with nothing to set.
- **A reader no longer waits for a write transaction in another process.** A write holds the
  RESERVED lock, which readers share, and a reader reads the last committed state. The writer takes
  the EXCLUSIVE lock only to write the database file. A reader waited up to 15.6 seconds before.
  `PRAGMA busy_timeout` and the other settings of a connection no longer take the write lock.
- **inillucent.com/install.md named 1.0.29.** The release now rewrites it, and refuses to finish
  when the live page names another version.
- **The client libraries' tests asserted version 1.0.** They check the format now, and the
  TypeScript client, published to npm as `inillucent-client`, finds the library where the engine's
  installers put it.

**Three fixes for one file shared by two processes.** A 3.5 GB database was damaged twice in one
day while one process wrote large batches and another had the file open. One copy had four pages
of zeros near its end. The other had a table whose leaf chain reached three leaves its interior
levels did not.

- **A second process could undo a writer's committed pages.** A statement whose dirty pages outgrow
  the buffer pool writes some of them to the file early, and saves their old images in
  `<database>-journal` first. The journal stayed on the disk after the statement committed and the
  lock was released. A process that opened the file in that window took it for a journal left by a
  crash and wrote the old images back. The writer did not notice and kept building on the old
  pages. A test with a 64 frame pool lost 505 of 4,800 committed updates, and in another run the
  file could no longer be opened. A connection now folds before it releases the lock whenever its
  journal holds old images.
- **A reader could not catch up on a log larger than its pool.** Every statement failed with
  `writing it would put the data file ahead of the log` until the writer folded. The replay now
  writes the pages it has to evict. Before it writes the file it raises its lock to exclusive, so a
  connection holding the shared lock never writes the file, the journal or the log.
- **A connection that was already open ignored a journal left by a process that died.** It replayed
  the log over the file as it stood, while an open puts the old pages back first. It now puts them
  back first too.

The new durability suite `process_journal_handoff` runs three shells on one file and fails on 2.0.2
in all three ways. A file damaged by an earlier release is not repaired by upgrading.

**Two fixes found by keeping a mailbox's search table current.**

- **A facet compared with an integer outside a search matched nothing.** A `FACET` column stores
  its value as text. Inside a search `email_id = 3` matched, but in a plain `SELECT`, `UPDATE` or
  `DELETE` the engine compared the stored `'3'` with the integer 3 and matched no row, so
  `DELETE FROM email_search WHERE email_id = 3` reported `0 rows changed` and left the chunks
  searchable. A facet column now has text affinity, and the engine applies a comparison's affinity
  when it tests a constraint a virtual table did not apply itself, as SQLite does. A virtual table
  column with no declared type compares as before.
- **A write that changed no rows left its transaction in the log for good.** `DELETE FROM t WHERE
  x = 5` with no such row committed a transaction that the connection did not fold into the file
  when it closed, so every later open printed `replayed the log: 1 committed transactions`. A
  connection now folds on close whenever it wrote to the log.

The docs now show how a program keeps an `inillucent_search` table current: embed each document in
the statement that stores it, delete and insert a changed document's chunks, and use `inillucent
embed` for the first import. See [Keep the search table current](docs/rag-explained.md#keep-the-search-table-current).

**Retrieval for a language model, out of the box.** A retrieval study on a mailbox of 67,369 emails
found which techniques put the right text in front of a language model. This release turns the
winning setup into features. See [Retrieval for RAG, explained from the start](docs/rag-explained.md).

- **A reranker.** `inillucent setup-embeddings reranker` installs `gte-reranker-modernbert-base`, a
  cross encoder of about 600 MB, from a pinned Hugging Face commit with a pinned SHA-256 for every
  file. `rerank(question, passage)` returns a score from 0 to 1. A search table reranks its own
  results when a query names `question`, and `rerank_depth` says how many rows the reranker scores
  (default 60). `inillucent search --rerank` does the same from the command line. `setup-embeddings
  all` does not install the reranker.
- **A `fusion` option on `inillucent_search`.** `fusion = 'rrf'` combines the keyword list and the
  vector list with reciprocal rank fusion. `'adaptive'` is the default and is unchanged, and
  `'weighted'` is what `vector_weight` alone has always meant. An RRF score is at most 2/61.
- **`inillucent embed`.** Fills a vector column for every row whose vector is `NULL`, on the processor
  or on a graphics card, in transactions of `--commit-every` rows, so a stopped run continues where it
  ended. It reports the rows embedded, the rows skipped and the rows cut at the token limit.
- **`chunk_text(text, size, overlap, heading)`.** A table function that cuts a document into
  windows, ending each at a paragraph break, a sentence end or a space, with the heading written at
  the start of every chunk.
- **`INILLUCENT_EMBED_THREADS` and `INILLUCENT_EMBED_DEVICE`**, and `setup-embeddings --threads` and
  `--device`, for `embed()`, `rerank()` and reranked searches. The defaults are unchanged. A device of
  `cuda` that will not start is an error that names `inillucent setup-embeddings runtime --gpu`.
- **`embed_tokens(text)`** counts the tokens the embedding model sees before its limit is applied.
- The test runner passes a row's `inillucent-cli/...` features to the build of the programs.

Two things an older release does differently. A reader from before this change ignores the
`fusion` and `rerank_depth` keys of a table, so a table declared with `fusion = 'rrf'` searches with
the adaptive weight there. It refuses a query that names `question` with `no such column`, because
the hidden column `question` is new. The hidden column `rank` of an `inillucent_search` table moved
from the fifth position to the sixth, behind `question`.

**Encryption at rest.** A database opened with a key is encrypted with XChaCha20-Poly1305: the
database file, its log segments, its rollback journal and its sort and spill files. A byte changed
without the key is found on the next read. The key is a passphrase, stretched with
PBKDF2-HMAC-SHA256 at 600,000 iterations, or a raw 32 byte key written `x'<64 hex digits>'`. The
command line, the shell and the MCP server take it from `--key-file` or `INILLUCENT_KEY`, never as a
word on the command line. The Rust driver has `OpenOptions::key`, the C library has
`inillucent_open_with_key`, and the npm, Go, PHP and Python packages have a `key` option. New
commands `inillucent encrypt`, `inillucent decrypt` and `inillucent rekey` write an encrypted copy,
write a plaintext copy and change the key. `PRAGMA encryption`, `PRAGMA rekey` and `ATTACH ... KEY`
work, with SQLCipher's rules. A wrong key, a missing key and a key for a plaintext file all fail
with the status `corrupt`. An encrypted file is 1.6% larger at the default page size, and opening
one with a passphrase costs about 200 ms. The cryptography is written in this repository and checked
against the published test vectors. The C ABI version is 1.1.0. See
[Encryption at rest](docs/encryption.md).

**A connection with a full buffer pool keeps working after another process writes the file.**
When another process writes, a connection throws its cache away and replays the log before its
next statement. Every frame it let go was left off the free list, so a connection whose pool had
filled up had nothing to read into afterwards, and every statement failed with `every frame in the
buffer pool is pinned` until the process restarted. A server holding one connection over a file
larger than its pool is always in that state. An insert into a table whose rowid is generated,
after the table's newest rows were deleted, took rowid 1 when the rightmost leaf held no live row
and failed with `UNIQUE constraint failed: <table>.rowid`; it now takes the largest rowid left plus
one. `integrity-check` lists every page the free map calls allocated and no tree reaches, one
`Page N: never used` line each up to a hundred, where it stopped at the first. It now names the table or index a
tree check failed on and the leaf pages involved, and checks the trees in the same order every
time.

**The statement matrix's first run found 67 ways inillucent answered differently from SQLite
3.53.4, and 66 of them are fixed.** Fixing them and running the matrix's random layer found seven
more, and six of those are fixed. The matrix runs every case against the pinned SQLite and
compares rows, result codes and counters. What changed, by what a caller sees:

- **Result codes.** A refused `BEGIN`, `COMMIT` or `ROLLBACK`, an `ATTACH` past ten, an `ADD COLUMN
  NOT NULL` with no default, an `ESCAPE` that is not one character and a write to `sqlite_schema`
  answer code 1, where they answered 21. A unique index built over duplicates answers 19 and 2067. A
  `LIMIT` of 2.7 or NULL answers 20, `datatype mismatch`. `ON DELETE RESTRICT` reports 1811, and a
  `UNIQUE` index over a `WITHOUT ROWID` key reports 2067.
- **Counters.** `changes()`, `total_changes()` and `last_insert_rowid()` belong to each
  connection, so another connection's statement no longer moves them. `CREATE TABLE ... AS SELECT`
  moves none of them. A row that `OR IGNORE`, a `CHECK` or `RAISE(IGNORE)` skips gives its rowid
  back, so the rows are stored as 1, 2, 3 where they were 1, 3, 6. An immediate foreign key is
  checked when the statement ends, so one `INSERT` can write a child before its parent.
- **SQL that was refused as not built.** `LIMIT` and `OFFSET` take any expression that reads no
  row. `UPDATE ... SET (a, b) = (...)` and `= (SELECT ...)`, and row values with `IS`, `BETWEEN`
  and a `CASE` operand, all run. A full text `MATCH` under an `OR`, and a `MATCH` written on one
  FTS5 column, run. An `IN` list on a joined table, a correlated subquery over a nested subquery,
  an `UPDATE` of a virtual table computed from the row, and a correlated `EXISTS` in a join's `ON`
  clause all run. `CREATE TABLE ... AS WITH ...`,
  `CREATE INDEX` on a temporary table, `CREATE TEMP TABLE temp.t` and `ANALYZE` of an attached
  schema run. Every pragma that returns a value has its `pragma_<name>` table function.
- **Wrong answers.** A `RIGHT` or `FULL JOIN` put the right table's values in the left table's
  columns; it no longer does. `UNION`, `EXCEPT` and `INTERSECT` return their rows in key order,
  and when two rows are equal but not identical, such as `0` and `0.0`, they keep the row SQLite
  keeps. `sum()` over text holding integers is an integer, the remainder of a
  real is a real, a `STRICT` table's `ANY` column keeps text as text, and negative zero, `unicode()`,
  `substr()`, `replace()`, a unary minus on text and a division by text answer what SQLite answers.
  An `OR` conflict clause no longer applies to a `STRICT` column's type check. A `DROP COLUMN` a
  view uses is refused, `PRAGMA auto_vacuum` survives a reopen, and VACUUM keeps a
  table with a stored generated column.
- **The Rust driver can cancel a statement from another thread.** `Database::cancel_handle()`
  returns a `CancelHandle` that is `Send` and `Sync`, and `inillucent_cancel` in the C library now
  sets the flag and touches nothing else.

Two are left. A table whose columns do not fit one page, such as 2,000 columns at a page of 4,096
bytes, is refused with exit code 3 and a message that names the column count and says a larger
page size holds it. And when the query of an `INSERT ... SELECT` fails part way, SQLite has already
written and undone the earlier rows, so `last_insert_rowid()` names the last of them, while
inillucent evaluates the query before it writes any row and leaves `last_insert_rowid()` where it
was. The table ends the same in both.

**A new example, `examples/coffee-shop`: a coffee shop's orders, stock and double entry books, as a
REST API in Rust.** It takes orders with sizes and modifiers, prices them with promotion codes,
loyalty points and sales tax, takes split cash and card payments, and keeps the stock and the books
in one database built on inillucent 1.0.30 from crates.io. Triggers post every sale and refund to
the journal and use up the stock, and a view proves that every journal entry balances. Its README
shows the SQL for each use case with the answer it returned on the seed data, from the ticket
number to the trial balance, and lists eleven ways 1.0.30 answers differently from SQLite, with what
the example does in each case.

**A new example, `examples/todo-mvc`: a todo service with a REST API, in Rust.** It keeps people,
lists, todos with subtasks to any depth, tags and comments in one database built on inillucent
1.0.30 from crates.io, and its README explains the SQL behind each route: foreign keys that cascade,
triggers that write a history, a view, recursive CTEs, window functions, filtered aggregates,
`UPSERT`, JSON functions and FTS5 search. Its README also lists six ways 1.0.30 answers differently
from SQLite, and what the example does in each case.

**`RAISE()` takes any expression as its message.** `SELECT RAISE(ABORT, 'too big: ' || NEW.n)` and
`RAISE(FAIL, printf('huge: %d', NEW.n))` in a trigger body report the computed text, as SQLite
does. Release 1.0.30 refused both `CREATE TRIGGER` statements with a syntax error at the `||`.
NULL is an empty message and a number is its text.

**A `json_group_array` or `json_group_object` result nests inside another JSON function.**
`json_object('day', day, 'items', json_group_array(item))` answered
`{"day":"2026-09-20","items":"[\"Latte\",\"Croissant\"]"}`, the array quoted as a string, where
SQLite answers `{"day":"2026-09-20","items":["Latte","Croissant"]}`. The same holds for
`json_array(...)` around one, and for a scalar subquery whose one column is a JSON value.

**A table valued function over another one's column, and a join on an expression of one.**
`FROM json_each(doc) s, json_each(s.value) r` and a `json_tree` or `generate_series` over an outer
`json_each` failed with "the tree read for FROM term 1 does not carry column 8", and `JOIN
ingredient i ON i.id = c.value ->> '$.id'` failed with "a seek key or range bound reads a column".
Both now answer as SQLite does. The same planner fix applies to a registered function's call and to
an FTS5 auxiliary function in a join condition.

**A `WHERE` on a view or derived table filters inside it.** `SELECT * FROM order_summary WHERE id
= 57` built every row of the view, correlated subqueries included, before it applied the `WHERE`,
so its cost grew with the table: 2.50 ms against 0.048 ms for the same query written without the
view, on the coffee shop example's 270 orders. A condition that reads only the view's columns is
now copied into the view's own query, which can then search by key. In a debug build, 30 runs of
that lookup over 2,000 orders went from 4,581 ms to 88 ms, the same as the query written out. The
copy is not made where it could change the answer: the right side of a `LEFT JOIN`, a view with
`DISTINCT`, `LIMIT`, `GROUP BY`, a window function or a compound, and a condition holding a
subquery or `random()`.

**Two window function queries that were refused now run.** A correlated scalar subquery in the
select list beside a window function, such as a count of each order's lines next to `row_number()
OVER (ORDER BY paid_at)`, was refused with "a correlated subquery used as a value". A windowed
query ordered by an expression, such as `ORDER BY low DESC` over `on_hand <= reorder_level AS low`
or `ORDER BY lower(name)`, was refused with "a windowed query ordered by a computed expression".

**`ORDER BY` a name that is both an alias and a table column sorts by the alias.**
`SELECT item, sum(quantity) AS quantity FROM order_line GROUP BY item ORDER BY quantity DESC`
sorted by `order_line.quantity`, read from whichever row stood for each group, so a list of best
sellers came out in the wrong order with nothing to say so. In a `SELECT`'s `ORDER BY`, a bare name
that matches an alias written in the result list now names that result column first, as in SQLite.
A name inside an expression, such as `quantity + 0`, still reads the table column in both engines.

**`UPDATE ... FROM` changes a target row once when several rows of the join match it.** It was
changed once per match, `changes()` counted every match, and `RETURNING` listed the row once per
match. SQLite changes it once, with the values of one match.

**`inillucent describe` lists generated columns.** It read `PRAGMA table_info`, which leaves them
out as SQLite's does, so a table with 18 columns was described as having 16. It now lists every
column, and a new `kind` column says `generated stored`, `generated virtual`, or `hidden` for a
hidden column of a virtual table. The MCP tool `inillucent_describe` answers the same.

**`inillucent-shell` no longer cuts a trigger at the `END` of a `CASE`.** A body written as
`SELECT CASE WHEN NEW.n < 0 THEN RAISE(ABORT, 'negative') END; END;` was sent to the parser after
the first `END;` and refused as incomplete. The shell now ends a trigger only at an `END` that
follows a semicolon, which is SQLite's rule.

**An index on a `VIRTUAL` generated column.** `CREATE INDEX payment_day ON payment (business_day)`
over `business_day TEXT GENERATED ALWAYS AS (date(at)) VIRTUAL` was refused with "an index on a
column the tree does not carry". The index is now kept as an index on the column's expression, so a
query on the column seeks it, every write keeps it current, and a `UNIQUE` one refuses a duplicate
computed value.

**`julianday()` gives SQLite's digits.** `julianday('2026-09-25T17:30:00Z')` answered
`2461309.229166667` where SQLite answers `2461309.2291666665`. The value is now computed in whole
milliseconds, as SQLite keeps it, so a difference of two Julian days multiplied by 24 gives the
same number of hours in both engines.

**The Rust rag example uses what 1.0.30 added.** It depends on `inillucent` alone with
`features = ["embed"]`, fills `chunk_search` from `chunk` with one `INSERT ... SELECT` per document,
embeds each question inside the search SQL, declares `vector_weight = 0.5`, and compacts the search
table after a sync that changed it. The command line example fills its FTS5 table with
`INSERT ... SELECT` in place of a second CSV file.

**`ship.ps1` prepares the release checkout the way the nightly prepares its own.** It copies the
pinned SQLite build, the gate fixtures and `tests/prerequisites.local.toml` from the main checkout,
and sets `INILLUCENT_NETWORK_TESTS`. Without them the 1.0.30 release was refused twice while every
test passed.

**What an unloaded model leaves on a graphics card is measured and documented.** A model dropped
from a CUDA session leaves about 500 MiB on the card until the process exits. That is the CUDA
context. ONNX Runtime releases its memory arena and the weights when the session is dropped, and it
has no call that releases the context. `inillucent-bench embed-residency --memory` reads the card
after each load, embed and drop, and [Embeddings](docs/embeddings.md#what-stays-on-a-graphics-card-after-the-model-is-dropped)
prints the run.

**Two changes for a Rust caller of `inillucent-core`.** `Branches::runs_vector()` and
`Branches::runs_lexical()` are public, so a caller of `Index::search_branches` can report the same
path `hybrid_search_grouped` does. `ModelManifest::read` and `ModelManifest::write` return
`anyhow::Result` in place of `Result<_, String>`, so `.with_context` and `?` work on them. A caller
that matched on the `String` error needs to change.

## 1.0.30 — 2026-09-25

**`embed(TEXT)` works in an `inillucent_search` or FTS5 table's `VALUES` row, and as a search's
query vector.** Release 1.0.29 refused `INSERT INTO chunk_search (..., vector) VALUES (...,
embed('...'))` and `WHERE vector = embed('search_query: ' || ?1)` with the status `unsupported`,
while the same call worked on an ordinary table and in `ORDER BY`.

**`INSERT ... SELECT` into a virtual table runs.** `INSERT INTO docs(body) SELECT body FROM t` fills
an FTS5 or `inillucent_search` table from a query. The query is read in full before the first row is
written. The capability `insert_select_into_virtual_table` is now `yes`, and 17 capabilities are
`no`.

**`inillucent batch` reports the engine's status.** A failure inside a batch was always `syntax`
with exit code 1. A statement the engine has not built is now `unsupported` with exit code 3, as
under `exec`.

**The `inillucent` and `inillucent-driver` crates have an `embed` feature.** It forwards to
`inillucent-engine`, so a Rust application no longer names the engine crate to get `embed(TEXT)`.

**`inillucent_search` takes a `vector_weight` option.** It fixes the vector list's weight in a search
with both parts, in place of the weight chosen for each query. The default is unchanged. On the
`examples/rag-agent` corpus, a fixed 0.5 raised the mean reciprocal rank from 0.681 to 0.789 and made
`confidence` separate answerable questions from unrelated ones. `docs/vector-search.md` also says
when `confidence` is not reliable, and what the first search in a process costs.

**A change is tested by what it touched, and the rest runs on a merge or at night.** Every tier in
`tests/selection.toml` now has a cadence. `inillucent-testrun --changed` runs a `durability` or
`perf` target only when a crate that changed is in the target's `covers`, and never runs the
`nightly` tier. CI runs every tier but `nightly` on each push, and a pull request runs what its
change can break at that cadence. A new workflow, `nightly.yml`, runs every tier on Linux once a day.
The runner builds only the packages and targets it selected, instead of the whole workspace. A
change to the parser alone now gets a strict verdict in 159 seconds, 165 targets. A one line change
to the command line program gets one in 116 seconds, and links 24 executables where the build step
used to link 283.

**`gates_fail_closed` no longer builds a second workspace.** Its nested runners read the executables
the outer run located from an artifact list, `--artifacts`, and start no cargo. The target took 36
minutes of a 37 minute run, alone at the end. Through the runner it now takes about 30 seconds, beside
the other targets.

**`inillucent-compat`'s 149 integration test files are seven binaries, one per tier.** Each file is a
module, `tests/<tier>/<name>.rs`, and its target is `inillucent-compat::<tier>::<name>`. The runner
still runs each suite in a process of its own. The test names in every suite were compared before and
after the move: 149 suites and 1,189 tests, identical. With debug and test builds keeping line tables
only, a cold test build of the workspace takes 50 seconds instead of 102, an edit to
`inillucent-base` rebuilds in 30 seconds instead of 78, and the target directory after a full test
build is 10.2 GB instead of 41.1 GB.

**A machine can declare the prerequisites it will never have.** The gitignored
`tests/prerequisites.local.toml` lists them. A strict run reports the suites they excuse under their
own heading and does not fail for them.

**A release relies on the nightly.** `packaging/nightly.ps1`, registered as a scheduled task by
`packaging/register-nightly.ps1`, runs every tier, builds all five release targets in parallel, runs
the gates and the scorecard on that build, replaces a rolling `nightly` pre release, commits the
timings, and files a ticket when anything is red. `ship.ps1` reads its verdict for the commit being
released instead of running a suite of its own, and says so in the release notes. The five release
builds now run at once: 257 seconds cold, against 646 seconds one after another. `--strict` passes
on the development machine for the first time, with four suites reported as not evidenced there by
declaration.

**A gate that misses only a known bar does not make the night red.** `inillucent-fullgate` exits 1
on any missed bar, and three bars have been missed on every graded run: `open.prepare`, `schema` and
the peak resident set. `compat/perf/known-misses.txt` lists them, and the nightly is red only when a
gate misses a bar that is not on the list, disagrees with SQLite, puts a family under the floor, or
does not finish. A gate that was not graded because the machine was busy is reported and is not red.
The first nightly also failed to publish, because `gh` was never logged in on the machine; it now
uses the token `git push` already uses, as `ship.ps1` does. And `latest.json` now records a commit
hash: the first night recorded git's `HEAD is now at` message, which no release commit could match.

**A new `CREATE INDEX ... USING inillucent_hnsw` index walks its HNSW graph.** An index used to be
created in exact mode, which compares the query with every stored vector on every query. There was
no setting to change that, so the graph the index built was never used. A new index is now
approximate unless it says `WITH (mode = 'exact')`, which matches pgvector's `USING hnsw`. `mode` is
an accepted index setting. `mode` on a `USING ivfflat` index is refused, because that module would
ignore it. An index created by 1.0.29 or earlier recorded exact mode in its own configuration and
keeps it, so the rows it returns do not change on upgrade. Drop it and create it again to get the new
default. A table declared `USING inillucent_search` directly is still exact unless it says otherwise.

Measured on 200,000 random vectors of 256 numbers, a query through a new index is 361% faster than
through an exact one (0.858 ms against 3.952 ms at the median), and its recall of the top 10 is
0.074. Random vectors are the worst case for a graph. On the 185,078 chunk corpus the same graph has
recall 0.8775 at the default `ef_search` of 64. `docs/vector-search.md` has both measurements and
says how to check recall on your own vectors. That page used to tell readers to write
`WHERE mode = 'approximate'` in a query, which fails with `no such column: mode`.

## 1.0.29 — 2026-09-24

**`DELETE` and `UPDATE` take `ORDER BY`, `LIMIT` and `OFFSET`.** They were refused with
`near "LIMIT": syntax error`, in the words of the pinned SQLite build, which is compiled without
`SQLITE_ENABLE_UPDATE_DELETE_LIMIT`. Apple's SQLite and many application builds have the option, and
`DELETE FROM t WHERE ... LIMIT 1000` in a loop is how a large table is trimmed without one large
transaction. The statement changes exactly the rows a `SELECT` with the same `WHERE`, `ORDER BY`,
`LIMIT` and `OFFSET` would return, which is how SQLite defines it. An `ORDER BY` with no `LIMIT` is
refused with SQLite's message, `ORDER BY without LIMIT on DELETE`. The two cases in the feature probe
that test this clause now answer where the pinned build refuses.

**A bare `REINDEX` runs on a database holding a `WITHOUT ROWID` table.** It failed with
`the index has no catalog row`, and so did `REINDEX` naming the table. A `WITHOUT ROWID` table is
its own primary key tree, so there is no separate index to rebuild. Its secondary indexes are still
rebuilt.

**A scalar subquery can be a value in an insert or update of a virtual table, and in a trigger
body.** `INSERT INTO docs(title, body) VALUES ((SELECT title FROM shelf LIMIT 1), 'x')` into an FTS5
table was refused as "a correlated subquery used as a value", although the same insert into an
ordinary table ran. A statement inside a trigger body was refused the same way, and so was an insert
through a view's `INSTEAD OF` trigger. A trigger body's `WHEN EXISTS (...)` guard and its
`WHERE ... IN (SELECT ...)` also failed when the statement that fired the trigger held a subquery of
its own.

**`inillucent run` reports the same status and exit code as `exec`.** A statement the engine has not
built was reported as `syntax` with exit code 1 when it was part of a `run` script, and as
`unsupported` with exit code 3 under `exec`. Exit code 3 now means the same thing under both.

**`inillucent capabilities` has four more rows**: `update_delete_limit`,
`subquery_value_in_a_virtual_table`, `subquery_in_a_trigger_body`, and `load_extension`, which is
`no` because there is no C extension interface to load a library into.

## 0.1.9 — 2026-09-24

**The macOS `.pkg` opens in Installer.app again.** The 0.1.8 package crashed the macOS Installer
with `abort()` in `_ReadFreeList` as soon as it was opened, because the `Bom` inside it had no free
list after its block table. The `Bom` writer now writes the free list and counts blocks in the
header the way Apple's tools do. It also writes the paths tree as a `Bom` from Apple's tools has
it: one leaf directly under the tree, with entries sorted by parent and name. `install.sh` and
Homebrew were not affected.

## 0.1.8 — 2026-09-24

**The file format is 2, and an index costs a write about 40% of what it did.** A leaf's delta area -
the rows written to it since it was last packed - now opens with a directory kept in key order, so a
lookup in it is a binary search and the area is as large as the page's free space rather than 32
rows. A compaction that finds the new rows fit the page's existing column widths splices them in
rather than packing every row again. On the index count sweep, 5,000 inserts into a 100,000 row
table, the cost of each secondary index went from 5.2 µs a row to 2.0, and at ten indexes the
workload went from 0.43x SQLite to 1.28x. A page's checksum also covers its LSN now, which it did
not.

**A lookup past the last key of a leaf that has been written to costs one comparison in its delta
area**, where it cost a binary search of it. Appending at the end of a table does this twice a row,
and so does a lookup of a rowid past the end, so rows appended since the leaf was packed no longer
slow either down; the gate's `txn.large` went from 2.852 ms to 2.708 pinned to the performance cores,
5.0% faster, and a range
probe into such a leaf now reads only the rows inside its bounds.

**This build reads every earlier file**, written by any release from 0.1.1 on, and a file becomes
format 2 at the first checkpoint after this build writes to it. **No earlier release reads a format
2 file**: 0.1.5 and later refuse it by name as `unsupported`, and 0.1.1 to 0.1.3 report it as
malformed. `docs/relational-architecture.md` section 5a has the details.

**A search table can filter inside the search: `FACET` columns.** A column of an `inillucent_search`
table declared `live FACET` is stored and can be constrained in a search - `WHERE docs MATCH ?1 AND
k = 10 AND live = '1'` - and the constraint is compiled into the filter the scan runs under rather
than applied to what the scan answered. Several narrow rather than widen. A facet's value is not
indexed as text, so it changes no ranking of the prose beside it, and it is an ordinary column
otherwise: it comes back from a `SELECT`, and on a query that is not a search the engine evaluates
it itself.

The difference between filtering inside the search and filtering after it is not small, and it is
why the feature exists. The keyword ranking rescores the best `k * 6` hits by where the query's
terms sit inside them, the rescore only ever lowers a score, and a hit below that window keeps its
full score and competes against rescored ones - so which hits are in the window depends on which
rows the scan admitted. Measured on a 400 row corpus, a constraint applied afterwards shared one hit
of the top ten with the same constraint applied inside. Filtering afterwards also returns fewer rows
than the `LIMIT` asked for.

**A table declaring a facet is stored in format 2** and an older build refuses to open it, by name,
naming the release to install. A table declaring none is stored in format 1 exactly as before, so
nothing already written becomes unreadable.

**`inillucent-migrate` uses it, and its `filter.deleted` check was wrong until it did.** The legacy
engine excludes a tombstoned document's chunks inside the posting scan. The copy had no way to
exclude anything before it ranked, so the migration claimed that joining to `document` and dropping
the deleted rows reproduced the legacy default - which it does not, for the reason above. A migrated
database now carries the flag on the search table's own `live` facet, and the check asks both sides
the same question at the same depth instead of comparing an answer ranked over the live chunks against one ranked over every chunk and
filtered afterwards. A second check goes in beside it, `filter.unreachable`, which asks whether
any chunk of a tombstoned document comes back at all.

**`PRAGMA integrity_check` and `PRAGMA quick_check` account for every page of the file.** They used
to read one tree at a time and each index against its table, and neither of those can see a page two
tables both own: each tree is a well formed tree and neither is an index of the other, so a file
where `SELECT count(*) FROM p` answers with `q`'s rows passed both and was reported `ok`. Two new
answers: `page N is used by table p and also by table q`, and `page N is used by table t but the
free map says it is free`, which is the state the first one grows out of at the next allocation.

The two pragmas stop being one pass under two names. `quick_check` reads every tree and accounts for
every page; `integrity_check` does that and then reads each index against its table. The line is at
the index pass because that is the expensive one, which was counted rather than assumed: over a
table of sixty out-of-line values the whole page walk cost 4 page fetches on top of 133, because it
takes an out-of-line value's pages from the reference in the leaf rather than by reading the value.
The pinned SQLite 3.53.4 draws it in the same place.

**The two page leaks this found are fixed.** A rolled-back `CREATE TABLE` or
`CREATE INDEX` kept its tree's root page, and `DROP TABLE` kept every page the table's out-of-line
values sat on. Both were dead space rather than lost data. Abandoning a transaction now gives back
every tree it built, a commit that drops a table frees the pages its out-of-line values used, and a
`REINDEX` gives back the pages of the index it replaced. A leak is now a state no statement
produces, and `PRAGMA integrity_check` reports one as `Page N: never used`, as SQLite does.
`docs/relational-architecture.md` has the details.

**A commit is one append to the log and one sync of it.** It used to be a
checkpoint: the log folded into the file, and a rollback journal holding the
pre-image of every page the fold was about to overwrite, which is six to eight
`fsync` class calls a statement. The fold is deferred now - until the log passes
four mebibytes, until a caller asks for a checkpoint, or until the connection
closes - and it is made safe without a rollback journal by appending the after
image of every page it is about to write to the log first. Measured on the
gate's own counters, an autocommit `UPDATE`'s hundred statements make **100 log
writes, 100 log syncs, no data file syncs and no folds**, where the same
workload used to make 202 syncs and write 3,252 KiB of log for 50 KiB of rows.

A bulk index build writes each page into the file directly rather than through
a buffer pool frame that then has to be evicted. `count(*)` is one addition a
batch rather than one accumulator call a row. The retrieval index builds on
every core, the two legs of a hybrid search run in parallel, and the distance
kernel dispatches once to an explicit AVX2 and FMA version.

Measured against SQLite 3.53.4, four consecutive 30-round runs either side of
the work on one machine, minutes apart:

| | before | after |
|---|---:|---:|
| weighted over the ten families | 3.55x | **4.53x** |
| 95% lower bound | 3.42x | **4.21x** |
| processor time, ratio to SQLite's | 0.635 | **0.400** |
| peak resident set, ratio to SQLite's | 1.140 | **1.100** |
| an autocommit `UPDATE` of one row | 0.13x | **0.94x** |
| an autocommit `INSERT` | 0.47x | **3.04x** |
| `count(*)` over 100,000 rows | 11.41x | **52.16x** |
| `GROUP BY` over the same | 7.89x | **27.51x** |
| `CREATE INDEX` over 100,000 rows | 0.66x | **1.37x** |
| the retrieval index build, 185,078 chunks | 129.7 s | **16.8 s** |
| vector search p50 | 0.934 ms | **0.8462 ms** |

The retrieval score card's ranking verdicts are unchanged - 15 better, 1
equivalent, 1 inconclusive, 0 worse, every correctness gate passing - which is
the condition the parallel build had to meet, because a parallel build's graph
is not the serial one.

**A statement run outside a transaction no longer reads the meta record twice on its way in.**
Under `locking_mode = normal` a statement takes the file lock, asks whether another process has
folded since this connection last held it, and gives the lock back. Both halves of that question
read both meta slots in full: a buffer one page long allocated and zeroed for each slot, a page read
into each, and a crc32 pass over each before a field could be read. At the 32 KiB default page size
that is four 32 KiB allocations, four 32 KiB reads and four crc32 passes over 32 KiB, per
statement, to compare a record that occupies 116 bytes. The record's own bytes are compared
instead, and the two callers share the one answer they both make under the same SHARED lock, which
a writer cannot hold at the same time. Bytes that differ still go to the full read and its
checksum, which is what decides.

A release from SHARED also unlocks one byte range rather than three. RESERVED is taken only on the
way to RESERVED and `take_shared` gives PENDING back before it returns, so two of the six lock and
unlock calls a statement made were unlocking a range the handle did not hold.

Measured through `Connection` on the medium fixture, `SELECT 1` cost **132,884 nanoseconds outside
a transaction and 1,126 inside one**, and costs **10,095 against 727** now. The two readings were
taken minutes apart on a box with two other agents working, so the ratio going from 118 to 13.9 is
the claim and the nanoseconds are not.

**No published figure moved, which is why this survived.** The scorecard and the performance
contract are measured with `inillucent-fullgate`, which drives the engine's own `plan`, `prepare`
and `pipeline` calls and never opens a connection. Nothing that grades this engine paid the cost or
would have noticed it moving. `inillucent-prepareperf` prints both columns, and
`crates/inillucent/tests/budget.rs` now counts what a statement outside a transaction reads so the
cost cannot come back unnoticed.

**An index a build cannot read is refused by name rather than answered with no rows.** The FTS5
index layout changed in 0.1.2, and 0.1.1 reads a file a later build wrote almost perfectly: the
tables, the `WITHOUT ROWID` entries, a blob stored over a page, the row that exists only in the log,
`SELECT count(*) FROM note_fts` as 5 and `SELECT rowid, title FROM note_fts` as all five rows. The
one thing it gets wrong is `WHERE note_fts MATCH 'segment'`, which comes back as no rows at all -
because it read the new doclist blob as a page number, found no such page, and a term with no
doclist is a term in no documents. That is the worst answer a compatibility break can give: an empty
result set is a legitimate answer to a search, so an application has nothing to tell it apart from
"there are no matching documents".

0.1.1 is published and its answer can never be fixed. What changes is the next one. An FTS5 index
written from this release on carries a layout record - one `%_data` row holding a magic, the layout
number and the release that wrote it - and a reader that meets a layout it has not got refuses with
the status `unsupported`, naming both, on `MATCH`, on any write, and on `fts5vocab`. The record is
stamped at `CREATE VIRTUAL TABLE` and by `rebuild` and `delete-all`, the two places the whole index
is written from scratch, and deliberately not by an ordinary insert: a file 0.1.2 through 0.1.7 wrote
has no record and may hold rows in both layouts at once, so a record stamped on the next write would
claim something the file cannot support. A missing record means "some layout up to and including
this build's" and is read exactly as it was, so no existing database changes behaviour.

An `inillucent_search` table's `%_config` gains a `writer` row beside the format number it already
carried, and its refusal now carries `unsupported` and names that release. The format was also
checked only in `begin`, which is the start of a write transaction, so an ordinary `SELECT` against a
table a later build wrote was answered out of a store whose format had never been checked; the read
path asks now. The database file's own format version already answered this way and is what the two
were made to match.

Two quieter instances of the same defect went with it. A `%_idx` row naming a `%_data` row that is
not there, or holding a third column that is neither a doclist nor a page number, resolved to "no
doclist", and every reader but `integrity-check` read that as "this term is in no documents" - so a
search over an index full of documents answered nothing, with no error anywhere. `term_row` was
worse: it staged the unreadable doclist as an empty one and the flush wrote it back, so a write to
the table destroyed the postings it could not read. Both refuse now.

`docs/relational-architecture.md` §5a states the promise for all three layouts in one table.
`crates/inillucent-compat/tests/format_refusal.rs` manufactures a record from a build that does not
exist and checks each refusal, including the command line's exit code.

**Every published release can search a graph this release wrote, and nothing was asking.**
`tests/interop/verify.sql` asks an `inillucent_search` table for its rows and its content, which is a
read of `%_content`; it never asked it to search, so no term query, no ranked query and no
nearest-neighbour query had been run by an older binary against a graph a newer build wrote - the
half of the file SQLite has no equivalent of, and the half a format change is most likely to move.
`tests/interop/retrieval.sql` asks it now, of an `approximate` table over twelve vectors compacted
into a stored generation, and 0.1.1 through 0.1.7 answer every question identically to this build.

**Measured against SQLite 3.53.4 on 2026-09-23**, four consecutive
30-round runs at 100,000 rows in a quiet window, with both engines pinned to the
performance cores:

| | 2026-09-20 | 2026-09-23 |
|---|---:|---:|
| weighted over the ten families | 4.53x | **4.97x** |
| 95% lower bound | 4.21x | **4.62x** |
| the `write` family | 2.12x | **3.04x** |
| 2,000 inserts in one transaction | 0.72x | **1.47x** |
| processor time, ratio to SQLite's | 0.400 | **0.500** |
| peak resident set | 40.76 MiB | **40.76 MiB** |

**The benchmark was measuring the two engines on different cores.** The machine
has 8 performance cores and 16 efficiency cores, and unpinned, Windows ran the
gate process, which is this engine's arm, on the efficiency cores and the SQLite
child on the performance cores. The same run unpinned reads 4.40x. The published
figure pins both; `docs/performance.md` has the evidence, and the gates now pin
themselves.

**The processor ratio got worse** because the plan gained four correlated
subquery workloads, which this engine answers once per outer row and SQLite as a
join: 182 ms of each round against under half a millisecond.

**Known not to do.** Six of the thirty weighted workloads are still slower than
SQLite: compiling `SELECT 1` on every call, a join over an index range, building
an FTS5 index, a range scan of the join's shape, `json_extract`, and an
autocommit `UPDATE` of one row. In that run a correlated subquery was 9,395% to
118,020% slower than SQLite, depending on the shape; the correlated fix under
"Speed without a change in answers" below takes a correlated `EXISTS` over 400
outer rows from 59.69 ms to 0.40 ms, measured on passes the gate did not grade
because the machine was busy. The resident set is 9.5% more than
SQLite's against a bar asking for 5% less, and the allocator is measured out of
the difference: a trivial binary's floor is 3.62 MiB with it and 3.62 MiB
without. At 600,000 rows this engine spends 30% more processor than SQLite on the
whole plan while finishing it 434% faster.

**Eleven correctness fixes the audit before release found, and nine smaller
differences from SQLite beside them.**

- A database file shorter than its own header is refused before a page is read
  out of it, saying how many pages the header claims and how many the file
  holds. It used to open and answer queries.
- A zeroed sector in the middle of the redo log stops recovery there. It used to
  end that segment's scan and carry on into the next one, replaying records
  whose predecessors had never been read.
- Creating a file forces the directory entry that names it on POSIX, and
  `PRAGMA synchronous = FULL` is `F_FULLFSYNC` on macOS. A log segment could be
  fsynced, acknowledged, and lost whole to a power loss.
- `randomblob(n)` answers `n` bytes up to the connection's value bound and
  refuses above it by name. It was silently clamped to 1,000,000, so
  `length(randomblob(100000000))` answered `1000000`.
- `CREATE TABLE` and `ALTER TABLE ... ADD COLUMN` are charged against
  `SQLITE_LIMIT_COLUMN`. A 2,100 column table used to be accepted.
- A recursive CTE has no pass limit. The guard at a million passes refused a
  series generator past a million rows, which is an ordinary idiom; what stops a
  recursion that settles neither way is the request budget, charged per row.
- A reader waits for a busy file for as long as its own `PRAGMA busy_timeout`
  says. It waited five seconds whatever the pragma was set to.
- A CSV that is not UTF-8 is refused with the byte that does not decode and its
  offset, instead of `cannot open "<path>"`.
- `PRAGMA aux.user_version` is about the attached database it names. Every
  pragma that is about a file answered about `main`.
- `--readonly` admits the `run` verb and the shell refuses each write inside it,
  so a read only agent can reach the dot commands.
- `run` exits 1 on a statement the shell refused, and `inillucent_run` over MCP
  answers `"isError": true`. Both used to report success.
- A blank line no longer ends an MCP session.
- The `CHECK` named in a refusal is the one that failed. Two unnamed `CHECK`s on
  one table always reported the first.
- `ESCAPE ''` is refused as SQLite refuses it, and an escape character of more
  than one byte is the whole character rather than its first byte.
- `date('2024-1-1')` and `date(' 2024-01-01')` are NULL, as they are in SQLite;
  the field widths are exact and only trailing whitespace is skipped.
- `printf('%.20f', 1.0/3)` answers sixteen significant digits and fills the rest
  with zeros, which is what SQLite prints; `!` raises it to twenty.
- An empty CSV field that is not NULL is written as two quotes, so an exported
  blob is no longer indistinguishable from a NULL.
- A path a confined process may not reach is reported as `invalid_state` rather
  than as a syntax error.

### The release blockers

The review before release named fifteen defects that had to be fixed before the
next release. Fourteen are fixed, each with a test that fails without the fix.

- **`inillucent integrity-check` answered `"ok": true` and exit 0 on a corrupt
  database.** It listed the pragma's rows without reading them. It now returns
  the status `corrupt` for any answer that is not exactly `ok`, including no
  rows at all.
- **`--params-file` read any file on the machine**, past `--root` and
  `--readonly`, and `query` and `exec` both take it over MCP. The path is
  confined now, `-` is refused on a confined server, and the file is capped at
  one mebibyte.
- **One request could stop the MCP server.** A line of 120,000 `[` overflowed
  the JSON parser's stack. Nesting is bounded at 1,000 levels, the server
  answers such a request with `-32700 nested more than 1000 deep`, and it
  answers the next request normally.
- **A free map chain that loops made `Database::open` run forever**, from every
  command including `integrity-check`. The open now refuses in under a second
  and names the chain.
- **`migrate --kind sqlite` did none of the checks it documents.** A database
  whose only content was an FTS5 table migrated to an empty file and reported
  success. The verb now runs the same verified migration as `inillucent-migrate`,
  and it carries `application_id` and `user_version` across.
- **`PRAGMA cache_size = -1000000000` grew the process to 3.3 GB.** The cache
  has a hard maximum now, `INILLUCENT_LIMIT_CACHE_SIZE`, and a request above it
  is refused in 74 ms.
- **`substr`, `trim`, `ltrim` and `rtrim` replaced bytes that are not valid
  UTF-8** with U+FFFD, and were quadratic in the length. They borrow the bytes
  now and follow SQLite's rule for a character: `substr(x'fffe80',1,2)` is
  `x'fffe80'`, as in SQLite.
- **Recovery dropped log records and counted them as applied.** A dropped record
  is now counted, reported by the driver and printed by the command line.
- **The HNSW and BM25 readers sized allocations from a count in the file**, so a
  damaged file could stop the process. The first fuzzing campaign found one of
  these, reachable from an ordinary `SELECT`.
- **Three C ABI entry points read a caller's pointer before checking it**:
  `inillucent_txn_execute`, `inillucent_txn_commit` and
  `inillucent_clear_bindings`. A freed handle is now answered from a table of
  live handles and is never read.
- **An outer `ORDER BY ... LIMIT` over a recursive CTE applied the limit
  before sorting**, so it returned the first rows generated rather than the
  first rows in order.
- **A vector query failed as soon as its HNSW index existed.** With an index,
  the literal `'[1,0,0]'` was read as seven raw bytes and refused against a
  three dimension index. One vector parser now serves both plans.
- **`dump` left out every row of a table whose name or a column name is a
  reserved word**, at exit 0, and dropped a column named `""`.
- **A blob came out of `--output json` as the text `x'00ff'`**, typed `text`, so
  bytes could be written and not read back by the Node, Go and PHP wrappers or
  the Python subprocess path. A blob is now `{"blob":"<hex>"}` in both
  directions, the column type says `blob`, and all four wrappers decode it.

The fifteenth, how an integer above 2^53 is carried in JSON, is not done. The
wrappers disagree today: given 9007199254740993, Node answers 9007199254740992,
and Python and PHP answer 9007199254740993. Changing the format would change
what the correct callers receive, so it has its own ticket.

### Correctness fixes after the review

Each of these was found by comparing against the pinned SQLite 3.53.4 or by a
test that reopens the file, and each has a test that fails without the fix.

**Rows that were lost or changed.**

- `ALTER TABLE t DROP COLUMN b` on a table `(a, b, c, d)` left `c` holding
  `b`'s values and `d` holding `c`'s, and it survived a reopen. Dropping the last
  column was correct, which is why nothing caught it.
- A rolled-back `DROP TABLE` followed by `CREATE TABLE` lost every row of the
  dropped table, durably, and `PRAGMA integrity_check` said `ok`. A dropped
  tree's pages are now freed when the transaction commits, not while the
  statement runs.
- A rolled-back `ALTER TABLE` left the connection reading and writing a tree
  the catalog no longer named, for the rest of the connection's life.
- An index built on pages that a `DROP` or an `ALTER TABLE ADD COLUMN` had just
  freed could come back damaged after a reopen, because redo replayed what those
  pages held before.
- `ALTER TABLE ADD COLUMN` and `DROP COLUMN` on an attached database failed with
  an I/O error, and altered the table in `main` when one of the same name was
  there. Every `ALTER` on a `TEMP` table was refused. All three work now.
- Below the default 32 KiB page size, an ordinary `INSERT` could answer
  `SQLITE_CORRUPT` or lose rows: two hundred FTS5 documents at a 4,096 byte
  page refused on row 42, and the third `CREATE TABLE` in a 512 byte database
  refused. Every page size the engine accepts now writes and reads back.
- Opening a database whose header was one checkpoint behind cut live pages off
  the end of the file, and every open after that failed. The trim now happens
  only after the open has read the schema, so an open that refuses leaves the
  file as it found it.

**Answers that differed from SQLite.**

- `SELECT count(*) AS n FROM t HAVING n > 0` was a syntax error. A `HAVING`
  without a `GROUP BY` filters the one group an aggregate makes, and the shapes
  SQLite refuses are refused in its words.
- `SELECT 1 UNION ALL SELECT count(*) FROM t` was refused, and with a
  `GROUP BY` on the later arm it returned one blank row per group. An aggregate
  or window function in any arm of a compound select now answers.
- Each `BETWEEN` bound is compared with its own operand's affinity and
  collation, and a `COLLATE` inside an operand, as in `'B' = 'b' || '' COLLATE
  NOCASE`, reaches the comparison around it.
- A `COLLATE` inside an aggregate or window call, as in `max(s COLLATE NOCASE)`,
  and each `CASE WHEN` decide their own comparison.
- An index seek converts its key with the comparison's affinity rather than the
  column's declared type, so a join into an untyped column or one of another
  affinity finds the rows SQLite finds, and a `NULL` key matches nothing.
- A seek key that is a `CAST` or a built in call, as in `id = CAST('8' AS
  INTEGER)` or `k = abs(-4)`, is evaluated rather than refused.

**Command line and migration.**

- `export --out <file>`, `.once` and `.output` wrote an empty file, answered
  `"ok": true`, and returned the rows in the response instead, including inside
  a script run by `run` over MCP. The rows go to the file now.
- Migrating a SQLite database with a `VIRTUAL` generated column failed its
  digest check and deleted a correct result. The digest compares the columns
  the source stores, a new `columns.<table>` check compares the declared list,
  and a digest failure now names the rows that differ.

**Speed without a change in answers.**

- A whole table `DELETE` below the default page size was quadratic: 8,000 rows
  took 1,301 ms and take 17.5 now. A `DELETE` visits the table and each index
  in that tree's own order, which makes the 32 KiB delete about 10 times
  cheaper a row.
- A correlated subquery is evaluated only for rows the query's cheaper
  conditions keep, and it no longer builds and frees a 3.2 MB slot
  array on every execution. `correlated.exists` made 45,414 page
  faults an execution and makes none, and the full gate plan now takes about
  4,200 page faults a round against SQLite's 11,638. Over 400 outer rows a
  correlated `EXISTS` went from 59.69 ms to 0.40 ms and a correlated `IN` from
  118.19 ms to 0.92, measured on passes the gate did not grade because the
  machine was busy.
- `LeafRef::column` reads its directory entry once, and a probe key whose
  affinity changes nothing is no longer copied: `join.range` went from 28.4 ms
  a round to 26.1.
- A statement that uses `%`, `/`, `||` or a bitwise operator is kept and run
  again from its compiled form. It used to be compiled again on every run,
  because those operators read the connection's settings through the same path
  as `changes()` and `random()`. A kept statement now records the settings it
  was built under and is rebuilt only when they change.

### The test and performance review

**A retrieval index holds a fifth less of itself in memory.** The BM25 postings
were a `HashMap<String, Vec<Posting>>` beside a second `Vec<String>` of the same
terms in sorted order, so each of 1.7 million terms was charged for three times:
a string in the map and a string in the list, a `Vec` header and its own heap
block however few postings it had, and a hash map slot with its stored hash.
They are flat arrays now - one byte array for the terms, one array for the
postings, a start per term, and a binary search instead of a hash - with an
overflow map that takes appends and is folded back in once it holds an eighth of
the postings. And the chunk text is left in `store.bin` and read a range at a
time, the way `vectors.bin` already was. Measured on a 185,078 chunk index, each
figure taken twice: the postings went from 306.2 MiB resident to 209.4 for
211.1 MiB on disk, and the whole index from 541.3 MiB to 444.3. **Neither
changes the file format**, so an index written by any build opens under this one
and the other way round.

**`NOT INDEXED` reaches the planner.** The clause parsed, an `INDEXED BY` was
checked for a name that exists, and then the hint was dropped: nothing carried it
past the binder, so both were accepted and ignored. `SELECT count(*) FROM h NOT
INDEXED WHERE a = 3 AND b = 100` on a 600 row table with two indexes planned as
`SCAN h` in SQLite 3.53.4 and as `SEARCH h USING INDEX h_b (b=?)` here. It
mattered beyond the plan: `inillucent integrity-check` reads every table
`SELECT * FROM "t" NOT INDEXED` to build its digest, on the argument that the
clause is what makes the digest a fact about the rows - and a table whose index
disagreed with it was being digested through the index.

**`INDEXED BY` forces the index it names.** It used to be checked for
a name that exists and then ignored. The planner now offers the named index and
nothing else, walks it whole when nothing seeks it, and refuses a statement the
index cannot answer with `no query solution`, as SQLite does. `UPDATE` and
`DELETE` obey it on their target, and `INDEXED BY` or `NOT INDEXED` on an
`UPDATE` or `DELETE` inside a trigger body is refused in SQLite's words. Two
planner defects went with it: a partial or expression index was never usable on
a `FROM` term after the first, and `SELECT count(*) FROM s CROSS JOIN h WHERE
h.b > 595` was refused with exit code 3.

**`printf` and every printed double use SQLite's own digits.** The
sixteen digit cap reached `%f` alone at first, so `printf('%.20g',
3.14159265358979)` was `3.1415926535897900074` here and `3.14159265358979` there.
Capping `%e` and `%g` took the disagreements from 222 of 675 generated statements
to 7, all in the last digit of a double in the exponent tail. Those 7 are closed
by printing through a transcription of SQLite's `sqlite3FpDecode`, whose last
digit is not always the correctly rounded one: `1.1304293785495057e251` printed
`...058e+251` here and prints `...057e+251` now, as SQLite does. The `!` and `,`
flags follow SQLite too: `printf('%!.25f', 0.1)` is `0.1000000000000000056` and
`printf('%,.10g', 1234567.0)` is `1,234,567`. The test population grew from 224
values to 1,024 and every statement must now match the pinned shell exactly.

**`.show` reports what was set.** `explain`, `stats` and `output` were written
into its format strings as `auto`, `off` and `stdout`, so `.explain on` then
`.show` answered `auto`, and a shell whose rows were going into a file said they
were going to the terminal.

**A sort that does not fit in memory spills to a run file and merges it back**,
where it used to hold every surviving row and be killed by the operating system.
The sort also encodes each row's key once and compares byte strings, instead of
dispatching on the value's type for every one of the `n log n` comparisons.

**`NOCASE` stops at a NUL, as SQLite's does.** SQLite's `NOCASE`
ends the comparison at a NUL both values hold at the same position and then
compares the byte lengths. This engine used to read every byte. The difference is
reachable only through `CAST(x'..' AS TEXT)` or a bound parameter, because no SQL
string literal can carry a NUL. The comparison, the index key and the `CREATE
INDEX` sort all follow SQLite's rule now. The same change found that a walk of a
`BINARY` index was taken as the order for `ORDER BY`, `GROUP BY` and `DISTINCT`
under `NOCASE`: with rows `b A a B c`, `GROUP BY x COLLATE NOCASE` answered five
groups where SQLite answers three. It answers three now.

**And the suite grew where it could not fail.** Sixteen fuzz targets that had
never been run are run by `tools/run-fuzz.ps1`, with a row per target in
`tests/fuzz-history.tsv`; the first campaign found an allocation sized from an
unvalidated `u32` in the HNSW reader, reachable from an ordinary `SELECT`. The
nightly tier's ledger is checked for freshness, so a scheduled task that stops
running is a failure rather than a silence. `docs/repository.md`'s coverage table
has a floor per crate and a test that reads it. And `semantics.rs`'s 235 probed
constructs are eight parallel groups rather than one serial test, with a guard
that every category belongs to a group - which found seven `fts5` cases being
graded by nothing.

## 0.1.7 — 2026-09-19

**`packaging/install.sh` had been unrunnable for three releases, and the reason it
was the one script nobody noticed is that it was the one script git left alone.**
`.gitattributes` converts every `.sh` in the repository to LF, and git skips a
file that already holds a carriage return - `install.sh` holds a literal one
inside a `tr -d` argument, so the conversion passed over it and it shipped with
CRLF. `sh` on Debian and Ubuntu is dash, which reads the carriage return as part
of the command and dies on the first line with `set: Illegal option -`. Anybody
who ran the `curl | sh` line off inillucent.com got that. The site route now
refuses to publish a shell script that does not parse or that holds a carriage
return, so this cannot ship again without the release stopping.

**Composer read the wrong commit, because a registry pins a version's commit the
first time it sees the tag and never moves it.** Packagist is a route of
`ship.ps1` now rather than something a person remembers, and it runs after the
mirror and after the GitHub release for that reason. The same rule is why
inillucent's Go module at v0.1.5 and its Composer package at v0.1.6 name the
previous release's source and cannot be corrected.

The mirror route pushes. It had never pushed anything, and `gh release create`
against a tag that does not exist makes one at the repository's current HEAD, so
a registry reading the mirror cached whatever was there. `-Only site,pypi` works
when the script is launched with `-File`. The straggler scan stops reporting a
dependency's version as a straggler.

`AGENTS.md` carries what three releases taught, at the top where it is read
before the first command rather than after the first mistake.

## 0.1.6 — 2026-09-19

**The first real release run found four places the plan and the run disagreed,
and this version is what they cost.** `ship.ps1 -WhatIf` printed a plan that
every route would follow, and then two routes could never have run at all: one
named a parameter the function does not take, and one was unreachable. A
`-WhatIf` that prints a route the run cannot take is worse than no plan, because
it is read as evidence.

**The mirror is pushed before the GitHub release is created against its tag.**
proxy.golang.org and Packagist both pin a version's commit the first time they
see the tag, and `gh release create` against a tag the mirror does not have makes
one at that repository's HEAD. Ordering the two routes is the whole fix and it
cannot be retried, which is why it is written down in `AGENTS.md` as well as
fixed here.

The PyPI route builds a wheel for every platform rather than only the one the
release was cut on - 0.1.5 published a single wheel, so `pip install inillucent`
worked on Windows and found nothing anywhere else. The Windows build imports the
MSVC environment itself: `onig_sys` compiles oniguruma with `cl.exe`, and a shell
that is not a Developer PowerShell has no `INCLUDE`, so the build stopped on
`stddef.h`. The crates.io route needs no prompt. The version phase commits every
file it wrote, rather than leaving some of them for the next run to trip on. The
Go wrapper's pinned release is a version carrier, so it moves with the other
seven. The release deploys inillucent.com rather than printing an instruction to
deploy it.

## 0.1.5 — 2026-09-19

**One command releases inillucent, and says what reached where.**
`packaging/ship.ps1` publishes to twelve destinations - five build targets, the
Linux packages, the signature over `SHA256SUMS`, the tag, the GitHub release, the
public mirror, inillucent.com, and the crates.io, npm, PyPI, Go module and
Homebrew routes - in five phases, and verifies each by asking the destination
what it serves rather than by reading an exit code. A route with no credential
is a skip carrying the sentence that fixes it, because a script that refuses
without all twelve is one nobody runs. Every credential is sealed under
`%LOCALAPPDATA%\inillucent\signing` and unsealed to a RAM disk for the run.

**The macOS release is built on the Windows box, with no Mac involved.** zig
cross-links the Mach-O and `rcodesign` replaces `lipo`, `codesign`, `productsign`,
`notarytool` and `stapler`; Apple's notary is an HTTPS API. The `.pkg` Apple
refused had three things wrong with it, all of them recorded in
`tasks/task-1995-macos-releases-without-a-mac-tdd.md`.

**Two lost writes, both where a file is handed back.** A rollback journal put
back after the pages it describes had already been superseded, and a checkpoint
writing a file it no longer held the lock on. The crash record moved eight cut
points from the new state to the old one, which is the evidence that the fix
changed what the engine does under power loss rather than only what it reports.

**Five things a statement stopped doing on its way out of the file**,
and a tombstoned document counts as one document rather than one per chunk.
An extent reference says what its value reads back as.

The part eight review closed thirteen defects across `inillucent-cli`, the bench
crate and the schema function authorizer, and the suite's own honesty work landed
with it: the PHP round trip ran for the first time, a Python wrapper that
resolved and would not start was fixed, the expected absences come from
`tests/selection.toml` rather than from a list of four, and a run that had every
prerequisite says so. Coverage was measured rather than estimated.

**Known not to do.** The Go module at this version names the previous release's
source, permanently: proxy.golang.org saw the tag before the mirror was pushed
and a registry does not re-read a version it has cached. PyPI carries one wheel
rather than four, so `pip install inillucent` finds nothing on macOS or Linux at
this version; 0.1.6 is the first with all four.

## 0.1.4 — 2026-09-17

**A statement no longer pays for the log's housekeeping on its way out, which
is worth a factor of two to four on every autocommit write.** `locking_mode =
normal` became the default in this version, and under it a connection
checkpoints and releases the file after every statement that wrote. A
checkpoint also makes the catalog's statistics honest, rolls a log segment,
writes a checkpoint record and deletes the segments it has made redundant -
work that belongs to a checkpoint somebody asked for, and that was running once
a statement. Nothing released carries the cost: 0.1.3 shipped with
`locking_mode = exclusive` as the default, under which a connection checkpoints
at close.

Measured on the medium gate against pinned SQLite 3.53.4, the same fixture and
the same disk: an autocommit insert 27.4 ms before and 8.2 ms after, an
autocommit update 22.6 ms and 8.6 ms, `schema.index` 61.5 ms and 54.2 ms. Two
thousand autocommit inserts through `inillucent-shell` took 73.4 s and now take
28.6 s, and the per-statement checkpoint is flat where it used to climb from
20 ms to 38 ms as the run went on.

What is left of an autocommit statement, timed: about 4 ms is the fold - five or
six pages written, three `fsync`s and a rollback journal created and deleted -
and about 4 ms is the statement's own execution and commit sync.

**What this costs, so it is not a surprise.** Between reclamations the log
keeps the segments that would have been deleted, up to four mebibytes - the
same bar SQLite draws at `SQLITE_DEFAULT_WAL_AUTOCHECKPOINT`, which is 1,000
pages of its 4 KiB default. **Nothing in this engine checkpoints when a
connection closes**, so that is also what a process leaves on disk when it
exits without asking for one; it was previously near zero only because the
per-statement checkpoint reclaimed every statement. Measured: 4,000 autocommit
statements against a 320 KB database leave 3.3 MB of log in one segment. A
process killed without closing leaves that same amount for the next open to
replay, where it used to leave at most one statement's worth, so the reopen
after a crash reads more and reports it. `PRAGMA wal_checkpoint`, the
`checkpoint` verb and `Database::checkpoint` all reclaim on demand.

**`Wal::retire_segments_below` was quadratic and unbounded.** It walked every
sequence number from 1, opening a file per sequence to read its header, so
every call re-asked about every segment an earlier call had already deleted.
Two thousand autocommit inserts opened 1.5 million segment headers, 1.49
million of them for a file that is not there. The sequence number is read back
from the meta record, so the cost survived a close: the same database reopened
with 2,030 segments behind it spent 19.5 ms a statement, more than half the
checkpoint, deleting nothing. It now starts at the lowest sequence that might
still be there, and deletes exactly the same files.

**`Wal::sequence_containing` answers for the segment being appended to without
reading its header off disk**, which is the answer almost every call gets.

**The staleness check a statement makes on its way in no longer opens a file.**
Before every statement a connection asks whether another process has written,
and the log half of that question cost a path lookup and a file open: it went
through `tail_on_disk`, which takes a path and walks forward from a sequence,
calling `access` at each one and opening the file to read its header. Its own
doc comment prices the check at "one `file_size` per lock acquisition", and
`Wal::tail_of_open_segment` is what makes that true - the log already holds
that segment open. Measured at 3.2 ms of an 8.8 ms autocommit statement, where
the meta-record half of the same check cost 0.03 ms. Only the open segment has
to be asked: a checkpoint is the only thing that rolls a segment, and it moves
the meta record's generation before it releases the file lock, so the
generation is seen first.

Statistics are written by a checkpoint somebody asked for - a close, `PRAGMA
wal_checkpoint`, `VACUUM`, a backup, an integrity check, a journal-mode switch -
rather than by every statement. Between those, a tree's recorded row and leaf
counts are the shape as of the last one. They were already an estimate rather
than an invariant: `PagedTree::attach` derives the leftmost leaf from the file
instead of trusting the recorded copy, and `PagedTree::check` compares the
sibling chain against the interior levels rather than against the recorded leaf
count.

**`embed(TEXT)` is `direct_only`, which is a behaviour change to a shipped
function.** A schema may no longer name it: a `CHECK` constraint, an index
expression, a generated column, a `DEFAULT`, a view or a trigger that calls
`embed` is refused with "may only be used from top-level SQL". A statement may
call it exactly as before.

It was registered with `FunctionFlags { deterministic: true, ..Default::default() }`,
and the `Default` derive is every flag false - so the flag said a schema may
name it while the function's own doc comment said "It stays `direct_only`: a
function that loads a 275 MB model has no business being called out of a `CHECK`
constraint or an index expression". Nothing published promised the old
behaviour: `PRAGMA function_list` does not report the bit, and no document said
a schema could call it. A `CREATE INDEX i ON t (embed(body))` would load the
model once per row of the table, inside the statement that creates the index.

`UserFunction::external` is the constructor a registrant should use for this;
`FunctionFlags::default()` exists for `builtin()`'s sake and is not what
anything registered from outside wants.

**And the binder consults it**, which it did not before this.
`Registry::authorize_function` had no caller anywhere in the workspace, so
`direct_only`, `innocuous` and `PRAGMA trusted_schema` were a policy with a
passing unit test and no effect on the engine: a `CHECK`, an index expression,
a generated column, a `DEFAULT`, a partial-index predicate, a view and a trigger
could each name any registered function whatever its flags said.

The rule itself moved down to `inillucent_sql::function::schema_refusal`, below
the binder that enforces it, and `Registry::authorize_function` calls that same
function - so an application asking the registry directly and a statement the
binder compiles cannot answer differently. `FunctionFlags` and `CallSite` moved
with it and are re-exported from `inillucent_ext::registry`, so every path an
application already writes resolves to the same type it did.

The binder carries a call site that is set at seven places: a `DEFAULT`, a
`CHECK`, a generated column's expression, an index expression, a partial-index
predicate, a view's body and a trigger's body. Two of those paths - the nested
binders in `Binder::bind_alone` and `dml.rs::bind_schema_expr` - also dropped
the connection's registered functions and collations on the way, so a schema
expression naming a registered function did not resolve at all; they inherit
them now.

**`CREATE INDEX` on an expression is refused when the index is created**, not on
the next write of the table. Such an index is filled by a `SELECT` the engine
builds out of the index's own expression, and a `SELECT` is a statement - so
that one query was the place a schema expression reached the machine with a
statement's permissions, and `CREATE INDEX i ON t (embed(body))` loaded the model
once per row before anything was refused.

**`PRAGMA trusted_schema` reports and sets the connection's own policy.** It
used to answer a constant 0 from the fixed-answer table while the connection's
policy said the opposite, which was harmless only for as long as nothing read
either one. The library's default is on, which is SQLite's; turning it off
refuses every registered function a schema names unless the registration said
`innocuous`.

**And `inillucent-shell` turns it off at startup**, which is what the reference's
shell does and why `.dbconfig` on the reference prints `trusted_schema off` on a
connection whose library default was on. A shell is a program that opens files it
did not write, which is the case the flag exists for. `.dbconfig trusted_schema`
reads and writes that setting now instead of printing a constant beside it.
`semantics.rs`'s `shell.dbconfig` case grades the whole listing against the
pinned SQLite and is what caught the difference.

**`PRAGMA defensive` refuses a write to a module's shadow table**, which is the
same defect in the same file: `Registry::authorize_shadow_write` was the whole of
that promise, it read a `Policy::defensive` nothing ever set, and nothing called
it. The shell turns defensive on for every connection it opens, so what
`.dbconfig defensive on` actually refused was `PRAGMA journal_mode = OFF` and
nothing else. Which names are shadow tables is derived from the roots each module
was connected with rather than from the spelling, so `docs_backup` is still an
ordinary table beside `docs_data`.

`Registry::authorize_extension` still has no caller and that is not the same
defect: nothing in this engine loads a shared library. `load_extension(path)`
refuses every path and so does the shell's `.load`, and those two refusals are
what `crates/inillucent-compat/tests/schema_function_policy.rs` checks, because
they are the guarantee a caller has. There is no `authorize_module` at all.

### The census: sixty-one places that could report success having checked nothing

The rest of this release is this review's answer to one question - how
many places in this repository can print a green result without having checked
anything - and the answer was 61, against a page that named 5.

- **One skip helper.** `inillucent_base::testing::skipping` prints the one
  marker and panics under `INILLUCENT_STRICT`, and it is below every crate, so
  the three production crates that could not reach the test harness no longer
  print their own sentence. Twenty-four raw prints across nine files are gone,
  including a local `announce_skip` in `tests/differential.rs` that shadowed the
  library one and let nine of that file's ten tests pass on a machine with no
  SQLite oracle.
- **The map and the suites have to agree.** `tests/selection.toml` gained a
  prerequisite on seventeen rows and lost one from six that could not skip, and
  `selection.rs` now fails in both directions: a suite that can skip without a
  declared prerequisite, and a declared prerequisite whose suite cannot skip.
- **An instrument can answer about an older tree, which looks exactly like an
  answer about this one.** `tools/doc-facts/check.mjs` measured the published
  test count with `target/release/inillucent-testrun.exe`, because its binary
  lookup prefers a release build - while both validate scripts build the runner
  into `target/debug` and run it from there. The release copy on the machine
  that cut this was four days old and reported 3,010 tests where the current one
  reports 3,016. The check now takes the newer of the two and refuses one older
  than any source it was built from, naming the file and the rebuild command.
- **A contract file is only as good as the lines its parser reads.**
  `tests/selection.toml` was carrying a bare array and a repeated key, left by an
  edit that removed half a row. `toml_lite` drops the first and keeps the last of
  the second, so the file parsed, 188 rows came back and every check over it
  passed. `every_line_of_the_map_is_one_the_parser_reads` compares the file to
  itself rather than through the parser, because a line the parser drops is a
  line no other check looks at.
- **The checks outside cargo fail when they cannot check.**
  `tools/doc-facts/check.mjs` treats an instrument that cannot answer as a
  failure rather than a skip - ten of its sixteen facts were skipping on any
  fresh clone - refuses a feature-probe result recorded at another commit, and
  runs as a stage of both validate scripts and of `packaging/release.sh`.

### End to end

The layer a user touches was the layer nothing exercised. Eighteen of the thirty
command line verbs had never been passed to a spawned binary, no test had seen
exit code 3 from outside a process, no MCP tool had been called by name over
real pipes, thirty of the seventy-one dot commands appeared in no test file, and
no durability test had ever killed a real writer.

- `cli_commands.rs`: one subprocess test per verb, asserting a named field of
  parsed `--output json` or a specific exit code, and one that drives a built
  binary to exit code 3.
- `mcp_wire.rs`: one handshake, twenty-eight `tools/call` requests, one process.
- `dot_commands.rs`: every dispatched name through a real shell, and the
  63-of-65 claim held to the pinned `sqlite3`'s own list in both directions.
- `process_crash.rs`: the operating system ends a real writer twenty times and
  the file is reopened from the parent.
- `crates/inillucent-migrate/tests/cli.rs`: the migration tool as a process,
  which it had never been.
- Round trips for the npm and PHP wrappers, which had only ever read their own
  source as text, and the Python conformance runner, which `drivers/README.md`
  calls the proof that a second language can implement the driver and which was
  run by nothing.

**Two defects those tests found**, both in verbs nothing had spawned:
`inillucent --db app.rdb shell` ignored `--db` and opened `:memory:`, and
`inillucent restore <file>` accepted a backup that is not there, exited 0 and
created an empty database.

### Known not to do

- A `CHECK`, a `DEFAULT`, a generated column or an index expression that names a
  function a schema may not name is refused when the statement that reads it is
  bound, not when the schema object is created. SQLite refuses the `CREATE`
  itself. `CREATE INDEX` is the exception and is refused at creation, because
  that is the one form this engine binds while it builds it.
- The Go wrapper's engine tests did not run on the machine that cut this: Go
  is not installed there, and `winget install --id GoLang.Go -e` downloaded
  1.27.0, verified its hash and ended with `Installer failed with exit code:
  1603`, which is the MSI declining to install without elevation. The `wrappers`
  validate stage names the absent toolchain and the install URL rather than
  passing over it silently. The other three wrappers ran: the npm suite 8 of 8,
  both PHP suites, and the Python conformance runner's 18 cases and 69 steps.
- The retrieval baseline names 19 files under `crates/inillucent-bench` that
  moved without an amendment, so `the_retrieval_baseline_is_unchanged` fails and
  with it the `contracts`, `tests` and `doc-facts` stages of both validate
  scripts. Every other stage of `tools/validate.ps1` passes. The amendment
  belongs to whoever changed those files.

## 0.1.3 — 2026-09-15, published 2026-09-19

**Published.** https://github.com/Black-Rainbow-Labs/Inillucent/releases/tag/v0.1.3
carries eleven assets and inillucent.com serves nine downloads, all naming 0.1.3.
This is the first inillucent release with macOS binaries: `inillucent-0.1.3.pkg`
is signed with a Developer ID and notarised by Apple, and
`inillucent-0.1.3-universal-apple-darwin.tar.gz` holds the same universal
binaries. It is also the first with signed `.deb` and `.rpm` packages, for x86-64
and aarch64, and the first whose `SHA256SUMS` carries a signature anyone can
check: `SHA256SUMS.minisig`, against `packaging/inillucent.pub`.

It was tagged on 2026-09-15 and left unpublished for four days. The GitHub
release stayed a **draft**, which is worse than nothing having happened: `gh
release view` finds a draft, so the release step uploaded every asset into it and
reported success while the release stayed invisible and untagged.
`Publish-GitHubRelease` publishes a draft it uploaded into now, and
`packaging/ship.ps1`'s preflight asks GitHub who it is rather than checking that
`gh` is installed — `gh` had never been logged in on the release machine.

This entry is written after the fact, because the release that cut the tag did
not write one and a hole between 0.1.2 and 0.1.4 is the kind of thing a reader
assumes is a mistake in their checkout.

What is in it is the public Rust surface reduced to one - the facade
is a re-export of the driver rather than a second API over the same engine -
`Connection::begin`, the engine's `lib.rs` from 7,307 lines to 1,279 and
`physical.rs` from 5,708 to 297, the parameter lists that were really types,
`ImportedDatabase`'s 63 fields in six groups behind their own cells, six
roadmap items, and coverage measured at 77.2% of regions.

Seven version pins moved together, because nothing downstream can tell which is
the real one, and `tools/doc-facts/check.mjs` fails the build when any copy
disagrees.

## 0.1.2 — 2026-09-13

**`embed(TEXT)` answers in a published binary.** Every archive up to 0.1.1 was
built without `--features inillucent-cli/embed`, so
`inillucent setup-embeddings all` downloaded 620 MB of ONNX Runtime and weights
and the program that downloaded them then answered `no such function: embed`.
The feature is in the release scripts now; the published Windows and Linux
archives both answer `SELECT length(embed('hello'))` with `3072`, the Linux one
after `setup-embeddings all` on a machine that had never run it.

Cutting it found three defects in the release gate, none of them reachable by
building the workspace:

- Five release checks spoke an MCP handshake the server no longer accepts. It
  enforces the lifecycle now — `initialize` needs `protocolVersion`,
  `capabilities` and `clientInfo`, and every other method answers `-32002` until
  `notifications/initialized` arrives — and `packaging/release.ps1`'s smoke test
  refused to build the archive at all.
- Two of those read the answer through `grep -q`, which stops at its first match
  and closes the pipe; the server's next write then failed and `set -o pipefail`
  reported a pipeline that had answered correctly as failed.
- No release had ever carried an ARM Linux archive, because `rust-toolchain.toml`
  named only the two x86-64 targets.

A fourth was not about the gate: a clean checkout on Windows turned every shell
script into CRLF, because `core.autocrlf` is true and the repository carried no
`.gitattributes`. `*.sh` is pinned to LF.

## 0.1.1 — 2026-09-11

**0.1.0 is withdrawn rather than patched.** Its archives carried `README.md`,
`docs/getting-started.md` and the quickstart skill from before the Go command
was renamed, so all three told a reader to run
`go install .../packages/go/cmd/inillucent@latest` — and `@latest` resolves to a
module where that directory no longer exists. The archive the site handed out
contained an install command that failed. Replacing those archives in place
would have left two different archives both called 0.1.0, so the version was
withdrawn instead; its archives answer 404 and its GitHub release is marked
*withdrawn — use 0.1.1*.

Also in this release, each found by running a command the release ships rather
than the same command from the repository:

- `SHA256SUMS` was written with CRLF, so Linux `awk` kept the carriage return
  and `curl -fsSL .../install.sh | sh` on Ubuntu said Linux had no build and
  then listed the Linux archive on the next line.
- Two one-liners pointed at `raw.githubusercontent.com`, which answered 404.
- `install.sh` used `set -o pipefail` and `${BASH_SOURCE[0]}`, both bash-only,
  against a documented command that pipes into `sh` — which on Debian and Ubuntu
  is dash. The script is POSIX now.
- `release.ps1` could produce an empty `SHA256SUMS` and exit 0, when an inherited
  `PSModulePath` shadowed `Microsoft.PowerShell.Utility` and `Get-FileHash`
  resolved to nothing. It checks for the cmdlets it needs before doing anything.

## 0.1.0 — 2026-09-10, withdrawn

The first release: the command line, the `sqlite3`-shaped shell, the MCP server
and the migration tool, with Windows and Linux archives on inillucent.com and
the Go module published as a tag.

Withdrawn the next day for the reason above. Its archives are removed from the
site and its GitHub release keeps its assets attached, because deleting them
would remove the record of what was published.
