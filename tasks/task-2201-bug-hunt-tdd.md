# Bug hunt: finding the defects users would find first

## Terms used in this document

| Term | Meaning |
|---|---|
| case | a short SQL script, run alone against a fresh database, whose output is compared byte for byte |
| probe | `tools/bug-hunt/probe.mjs`, which runs case files through the pinned `sqlite3` and `inillucent-shell` and reports every difference, crash, hang and slow case |
| round | one pass of: gather cases, run them, fix what differs, write regression cases |
| research corpus | the cases gathered from other engines' bug reports, kept in `compat/corpus/research/` |
| regression case | a case written for a defect this ticket fixed, kept in `compat/corpus/usage/bug-hunt-*.sql` |

## 1. Why

The ticket asks for defects, edge cases, slow paths and odd behaviour that users have not hit yet,
found by reading the current tests and by mining the bug reports of other engines that implement
SQLite's dialect. It asks for a loop: find, fix, research more, until a round finds nothing new of
substance. Performance must not get worse.

## 2. What the test suite already covers

The suite is large. `differential::usage_corpus` runs 1,761 application shaped scripts against the
pinned SQLite 3.53.4. The engine and differential tiers add the SQL logic test files, TLP and PQS
differential testing, a statement matrix of every statement form in every pair of contexts, and
about thirty durability suites with real process kills. A first run of this ticket's 65 hand written
probes of limits and numeric edge cases found one difference (`printf('%u', -1)`).

So the cheap classes are covered. The defects that remain are in combinations that no generator
produces and nobody wrote down: a planner rule meeting a collation, a join kind meeting a table
function, two levels of correlation, a JSON subtype crossing a derived table. Those are exactly the
classes other engines' bug trackers are full of, which is why the research is the core of the
method.

## 3. Method

### 3.1 The probe

`tools/bug-hunt/probe.mjs` takes case files in the usage corpus format (`-- case: <name>` starts a
case; text after a `|` on that line is a note, usually the source URL). Each case runs in a fresh
directory against a fresh database file, under `.mode quote` and `.headers on`, through both shells,
with the error pointer lines removed, exactly as `differential::usage_corpus` does. So a case that
passes the probe passes the suite. It reports:

- a difference in output;
- a crash: a Rust panic, a stack overflow (`0xC00000FD`), an access violation, or a signal;
- a hang: no exit within 60 seconds;
- a slow case: more than 1 second, and more than ten times SQLite's time.

Cases run several at a time. The report goes to `_agent_output/bug-hunt/report.json`.

### 3.2 Where the cases come from

| Source | Cases | How gathered |
|---|---|---|
| hand written probes of limits, numbers, the planner, collations, joins | about 100 | from knowledge of SQLite's documented edge cases |
| Turso (formerly limbo) GitHub issues | 306 | about 800 issues read through the GitHub API by label: compatibility, correctness, fuzzing, panic |
| SQLite release notes and SQLancer bug lists | 319 | release notes 3.45, Rigger's SQLancer tickets |
| SQLite forum posts and SQLancer lists, second pass | 409 | web search of the forum by bug class, forum posts read one by one |
| performance pitfalls | 62 | queries SQLite answers with an index or a special path, with data generated inside the case |

### 3.3 The loop

Each round runs every case, groups the differences by cause, and fixes the causes in order of how
likely a user is to hit them: wrong rows first, then crashes, then refusals of valid SQL, then error
message parity. A cause that needs a deep change is diagnosed first, then fixed with the smallest
change that is correct. Each fix gets a regression case. A round ends when every difference left is
either fixed, a deliberate difference documented in `docs/sql.md`, or recorded in section 6 with a
reason.

## 4. Defects found and fixed

Each row names the case that found it. "Release" means the shipped 2.2.0 build answers the same
wrong way, so a user can hit it today.

### 4.1 Wrong rows

| Defect | Found by | Cause | Fix |
|---|---|---|---|
| On a `WITHOUT ROWID` table whose primary key has `COLLATE NOCASE` or `RTRIM`, `UPDATE` and `DELETE` changed the first row or no row, and `INSERT OR IGNORE`, `ON CONFLICT DO NOTHING` and `DO UPDATE` overwrote the existing row | SQLancer case in the release notes set | the write path took the row's key from the layout's `key_columns`, which is left empty for a key that is not sorted in binary order; `identity` holds the key | every write path reads `identity`; a key equal under its collation is the row itself (`dml/conflict.rs`, `dml/index.rs`, `dml/update.rs`, `dml/target.rs`, `dml/insert.rs`) |
| A partial index held the rows whose predicate is NULL, so `WHERE b = 'x' AND a > 5` returned rows with `a` NULL | hand probe | `IndexExprs::holds` used the `CHECK` rule, where NULL passes | NULL is not true (`exec/declared.rs`). A file written before needs `REINDEX` |
| `2 = 1 < 3` was 1 and `1 BETWEEN 0 AND x <= 3` took `x` as the bound, also in `UPDATE ... WHERE` | forum case | `<`, `<=`, `>`, `>=` shared a precedence level with `=`, `IS`, `IN`, `LIKE` and `BETWEEN` | a level of their own, one tighter, as SQLite's grammar declares (`sql/precedence.rs`) |
| A subquery two levels deep that reads its parent and the outermost query answered the first outer row's value for every row | forum case | both levels numbered their correlation parameters from the same base, so two columns shared one slot | a block's numbering starts above any parameter already in it (`exec/correlate.rs`) |
| `FROM j, json_each(j.doc) e JOIN t ON t.id = e.value` returned no rows (release) | performance case | the join order chooser could put a table function before the term its argument reads | an argument is a prerequisite (`sql/plan.rs`, `argument_prerequisites`) |
| A table function on the right of `RIGHT` or `FULL JOIN` returned nothing or failed with "first argument missing" | forum case | the planner offers an outer term only its `ON`, and the arguments were in the `WHERE` | arguments go to the term's `ON` for every outer join, and a `RIGHT` or `FULL` term is offered its constant arguments (`sql/bind/scratch.rs`, `sql/plan/outer_paths.rs`) |
| `SELECT * FROM t('query')` and `WHERE t = 'query'` on an FTS5 table returned no rows (release) | forum case | an equality on the table's own column was tested as a value instead of being a match | it is a match, as in FTS5 (`ext/vtab/fts5/mod.rs`) |
| The trigger pattern SQLite documents for an external content FTS5 table failed with "SQL logic error" on every `UPDATE` and `DELETE` of the content table and left stale postings (release) | forum case | the `delete` command was not implemented | implemented from the text the command supplies (`ext/vtab/fts5/merge.rs`) |
| `contentless_delete=1` was accepted and ignored, so `count(*)` counted deleted rows | forum case | an option the index cannot honour | refused with exit code 3, with a capability row |
| FTS5 `rank` ignored `INSERT INTO t(t, rank) VALUES('rank', 'bm25(...)')` | forum case | the setting was stored and never read | `rank` scores with the configured weights (`ext/vtab/fts5/bm25.rs`) |
| `count(*) OVER (ORDER BY a RANGE BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING)` was 0 on the first number when NULLs sort first | release notes case | a NULL row was skipped when a `RANGE` bound was resolved | NULL sorts beyond every value at its end, as SQLite compares it (`scalar/window.rs`) |
| `RANGE BETWEEN 0.5 PRECEDING AND 0.5 FOLLOWING` was read as an offset of 0 | hand probe | the offset was read as an integer | a `RANGE` offset is a number |
| A derived table's `ORDER BY` was lost when it was an arm of a `UNION ALL` | forum case | it was flattened into an arm that carries no `ORDER BY` | not flattened (`sql/plan/flatten.rs`) |
| `json_group_array(obj)` over `(SELECT json_object(...) AS obj ...)` built objects where SQLite builds quoted strings, and `WHERE NOT json_quote(c)` over a view kept no row | forum cases | flattening and pushing down put a JSON call where the column was, which carried the JSON mark | the substituted call stands behind a unary plus (`flatten.rs`, `pushdown.rs`) |
| `json_group_array(value)` over `json_each` turned every nested object into a quoted string | forum case | a `json_each` value carried no JSON mark | an internal call marks a container's value (`JsonFunc::WalkValue`) |
| A subquery named `subQuery` beside an unnamed derived table: every reference failed as ambiguous | forum case | the unnamed table's stand in name compared equal to it | asked of the flag, not the name (`sql/bind/twin_terms.rs`) |
| `INSERT OR REPLACE` of a `NOT NULL` column's default stored it without the column's affinity | Turso issue | the default was put in after affinity was applied | affinity applied to the default |
| `x IS TRUE COLLATE NOCASE` was a comparison with 1 | forum case | the truth test looked for the literal only | it looks through `COLLATE` |
| `PRAGMA ignore_check_constraints = 1` changed nothing | hand probe | the pragma was stored and never read | the binder binds no `CHECK` under it |
| A unique index on an expression with `COLLATE NOCASE` accepted 'arth' beside 'ARTH', and a seek through it found only the exact case (release) | SQLite TCL `indexexpr1-810` | an expression key column was built with no collation, so its tree compared in BINARY order | the key column takes the collation the index names (`engine/rowshape.rs`). A file written before needs `REINDEX` |
| `INSERT OR REPLACE` into a table whose foreign key cascades to itself deleted the new row as well and reported nothing | SQLite TCL `fkey1-5.2` | the new row's parent was checked before the replace ran the old row's cascade | the check runs after the replace deletes (`exec/trigger.rs`, `fire_key_checks`) |
| A `REPLACE` constraint replaced a row that a constraint with the default `ABORT` also refused | SQLite TCL `conflict-15.20` | the first conflict found decided | a `REPLACE` first asks whether any constraint that does not replace also collides (`dml/conflict.rs`, `all_conflicts`) |
| `SELECT ... FROM t WHERE x = 4`, `UPDATE` and `DELETE` on a table whose `VIRTUAL` generated column comes before `x` decoded the neighbouring column | SQLite TCL `gencol1-5.100` | the column mask was indexed by record slot where it needed the declared position | the slot is mapped back to its column (`physical/stages.rs`, `declared_of_slot`) |
| `last_insert_rowid()` inside a trigger body was the value from before the statement | SQLite TCL `lastinsert-2.2` | the value was fixed when the statement started | each insert in a body sets it, and the value is restored when the body ends, as SQLite does |
| An `AUTOINCREMENT` table whose triggers insert into it got two `sqlite_sequence` rows, the second with a smaller value | SQLite TCL `autoinc-3928.2` | each nested insert wrote a sequence row as if none existed | the row is read again and the larger value kept (`exec/sequence.rs`) |
| `x LIKE 'abc%%' ESCAPE '%'` treated the first `%` as a wildcard | SQLite TCL `like-17.0` | the wildcard test ran before the escape test | a character named by `ESCAPE` is not a wildcard (`scalar/pattern.rs`) |
| A window function in the first arm of a compound derived table dropped every other arm, and under `UNION` replaced their values | SQLite TCL `window1-45.2`, `window1-62.4` | the derived table was run as a windowed query before it was checked for a compound | the compound is checked first (`physical/joins.rs`) |
| `(a, b) = (SELECT 5,6 UNION SELECT 3,4 ORDER BY 1)` compared with the wrong row | SQLite TCL `rowvalue-22.100` | each column of a row value was read from the first arm cut to one column, so the `ORDER BY` sorted by the wrong column | a compound is read through a derived table that holds all of it (`sql/bind/rowvalue.rs`) |
| `SELECT DISTINCT c0 FROM t0 INDEXED BY i0 ORDER BY c0 ASC NULLS LAST` put NULL first | SQLite TCL `nulls1-12.9` | a skip scan was taken as already sorted for any ascending order | not when NULLs are wanted last (`physical/chain.rs`) |
| `t2 RIGHT JOIN t3 ON ... LEFT JOIN t1 ON c=3 WHERE t1.a<>0` returned a row SQLite does not | SQLite TCL `index6-19.2` | `c=3` was used for a seek on a partial index of a term the `RIGHT JOIN` later extends with NULLs, and was never tested on those rows | the `ON` terms of joins written after a `RIGHT` or `FULL` join are held back like the `WHERE` (`sql/plan/outer_paths.rs`) |
| `DROP TABLE t` also deleted a trigger named `t` on another table | SQLite TCL `schema4-1.4` | the schema rows were matched by name alone | a trigger row is matched by the table it belongs to (`engine/ddl/alter.rs`) |
| `b GENERATED ALWAYS AS (...)` gave the column NUMERIC affinity, so `556.0` was stored as `556` | SQLite TCL `upsert5-4.3.3` | `GENERATED` was read as the type name | the type stops at a word that begins a constraint (`sql/parser/ddl.rs`) |
| `x IN ((SELECT ...))` tested only the first row | SQLite TCL `in-22.2` | the parentheses made a one element list holding a scalar subquery | a lone subquery in any number of parentheses is the subquery form (`sql/parser/expr.rs`) |
| A nested `WITH` in a common table expression's body captured an outer name | SQLite TCL `with2-1.6`, `with3-2.0` | the body was bound with every `WITH` level visible | the levels above the one that defined the expression are hidden (`sql/bind/cte.rs`) |
| `JOIN (t2) AS x USING (b)` did not merge `b` | SQLite TCL `tkt3911.2` | a parenthesised single table was bound as a subquery | it is the table (`sql/parser/select.rs`) |
| `RTRIM` sorted `' '` after `char(20)` | SQLite TCL `collate1-8.0` | the shared prefix was compared before the spaces were dropped | trim both, then compare (`inillucent-value/src/collation.rs`). An `RTRIM` index holding control characters needs `REINDEX` |
| A filter was pushed into the arms of an `INTERSECT` over `NOCASE` columns | SQLite TCL `selectA-9.2` | | not pushed into `UNION`, `INTERSECT` or `EXCEPT` unless every column is `BINARY`, SQLite's rule (`sql/plan/pushdown.rs`) |
| `ORDER BY` a column that `WHERE` sets equal to an outer value sorted by the column's own value | SQLite TCL `orderby5-5.3` | | such a term is constant and not sorted on (`sql/plan/order_constant.rs`) |
| An aggregate whose argument is a subquery reading the outer query, in a query with window functions, was refused | SQLite TCL `window1-63.3` | | the aggregate belongs to the outer query (`sql/bind/outer_aggregate.rs`) |
| Smaller: `substr` with a huge negative start, `->>` labels with JSON5 escapes, `printf` with an unknown conversion and `%p`, a row value subquery's collation, the collation of a `FULL JOIN` `USING` column, column names for `AS ''` | SQLite TCL `substr-6.1`, `json502-3.2`, `printf-20.*`, `rowvalue-23.110`, `joinI-8.13`, `view-22.1` | | each follows SQLite's rule |
| A filter on a compound derived table compared each arm under that arm's own collation | SQLite TCL `collate5-5.1` | the derived column's collation was taken from the first arm and not applied to the pushed filter | the derived column's collation is the compound's, and the pushed filter carries it (`sql/plan/pushdown.rs`) |
| `CREATE TABLE AS` over a compound wrote the first arm's type | SQLite TCL `affinity3-220` | as above | the affinity every arm agrees on, or none |
| `b IN s` over a common table expression applied no affinity | SQLite TCL `view-31.2` | a column with no affinity was read as an untyped declared column | the rule `=` uses is shared (`sql/bind/comparison_rules.rs`) |
| The merged `USING` column of a `FULL` or `RIGHT JOIN` had no affinity, so `x = '2.5'` over a `REAL` column found nothing | SQLite TCL `joinI-8.3`, `joinH-15.1` | it is a `coalesce`, which has none | an internal `coalesce` that takes its first argument's affinity (`ScalarFunc::UsingCoalesce`) |

### 4.2 Crashes and hangs

| Defect | Cause | Fix |
|---|---|---|
| With `recursive_triggers` on, a delete trigger that reinserts the row `INSERT OR REPLACE` deleted overflowed the stack and killed the process | nothing bounded a recursion that happens at run time | the executor refuses one level past the `trigger_depth` limit (`exec/trigger.rs`) |
| `WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT count(*) FROM r)` never finished | an aggregate step makes a row from no rows, so the queue never empties | refused as SQLite refuses it, with a `GROUP BY`, a window function and a second reference (`sql/bind/cte.rs`) |
| A recursive step with more columns than the seed never finished | the width of a step was not checked | refused with SQLite's message (`sql/bind/cte.rs`) |

### 4.3 Valid SQL refused

| Construct | Now |
|---|---|
| a table function followed by a derived table in `FROM` | answered: the function's argument stayed pending and the derived table took it into its own `WHERE` (`sql/bind/scratch.rs`) |
| `json_group_array`, `json_group_object`, `median`, `percentile` and `group_concat` with a per row separator, over a window | answered |
| a view that reads itself | refused as "view v is circularly defined", exit code 1, where it was exit code 3 |
| a column constraint that is only a deferral clause: `a NOT NULL DEFERRABLE INITIALLY DEFERRED` | accepted; it changes the `REFERENCES` before it in the same column |
| table constraints with no comma between them, and a `CONSTRAINT name` with nothing after it | accepted, and `ALTER TABLE ... DROP CONSTRAINT` handles stacked names as SQLite does |
| a string literal as a key column: `PRIMARY KEY('a')`, `CREATE INDEX i ON t('c')` | read as a column name |
| a `USING` column of a parenthesised join, named by an outer `USING` or by its bare name: `t1 JOIN (t2 LEFT JOIN t3 USING(a)) USING(a)` | answered: the merged column counts once, and `t3.a` still names the hidden copy (`sql/bind/nested_names.rs`). 283 of SQLite's join tests were refused with "ambiguous column name". Once the form was accepted, a parenthesised `RIGHT` or `FULL` join could be flattened into the enclosing join and lose rows, so such a join is no longer flattened (`sql/plan/flatten.rs`) |
| join words in any order: `OUTER LEFT NATURAL JOIN`, `LEFT RIGHT JOIN` | read as flags, as `sqlite3JoinType` reads them; `INNER LEFT` and `OUTER` alone are refused with SQLite's "unknown join type" |

### 4.4 Messages and small parity

`printf('%u', -1)`; `printf('')` is NULL; `json_extract` with a NULL among several paths;
`json_valid` and form feed; JSON nesting of exactly 1000; DELETE left raw in JSON strings and in
the shell; a tab printed raw by the shell; `-0x8000000000000000` refused; an aggregate named by a
`GROUP BY` ordinal or alias refused with SQLite's message; invalid frame offsets and `nth_value`
positions refused with SQLite's messages; `PRAGMA schema_version = N` answers no row.

## 5. Performance

62 cases written to be slow in a naive engine, each generating its own data, run on a release
build. A case was slow when it took more than a second and more than three times SQLite's time.

| Case | Before | After | SQLite | Cause and fix |
|---|---|---|---|---|
| running `sum(v) OVER (ORDER BY id)`, 100,000 rows | over 60 s | linear | 0.14 s | every row's frame was folded afresh. A frame that starts at `UNBOUNDED PRECEDING` now feeds one accumulator, and a sliding sum adds and removes from frame bounds that only move forward (`exec/window.rs`) |
| `RANGE` frame with ties, 100,000 rows | over 60 s | linear | 0.15 s | the same, and `RANGE` bounds are a binary search |
| sliding frame, 100,000 rows | 4.3 s | under 1 s | 0.39 s | the same |
| `dense_rank()` and `GROUPS` frames | quadratic | linear | | peer groups are indexed once per partition |
| `json_each` of a 20,000 element array | 2.0 s | 0.011 s | | the scan read the hidden `json` column, the whole document, into every row (`sql/bind/column_use.rs`) |
| `j, json_each(j.doc)` with a 110 KB document | 2.14 s at 20,000 elements, growing as the square | 0.020 s | 0.02 s | the lateral join copied the outer row, document included, into every row the function returned. The outer values are now shared constants across the expansion (`exec/lateral.rs`) |
| `json_each(...) e, j` and `e, (SELECT small FROM j)` | 0.12 s at 20,000 | 0.016 s | | the cross join read the inner row's large values from their overflow pages once per outer row, including columns nothing reads. It now reads only the columns the query needs, and reads overflow only when one of them is stored there (`join/loops.rs`) |
| a filter on the grouping column of an aggregate view | the whole table grouped | an index search | | the filter is pushed into the view when it reads only grouping columns (`sql/plan/pushdown.rs`) |

The fixes take work out of a path and add none to the paths the gates measure: the window changes
replace per row frames with bounds, the join order rule is checked only when a run holds a table
function argument, and the column rule skips conjuncts only for a virtual table term.

### 5.1 The gate against 2.3.1

`inillucent-fullgate` on the medium fixture, 10 rounds, page size 32768 and 4096 frames, run four
times alternating between a build of 2.3.1 (`36bddf3b`) and this branch, on the performance cores.
The machine had other work on it, so the gate graded nothing and only the ratios are compared.

| | 2.3.1 | this branch |
|---|---|---|
| weighted geometric mean against SQLite, run 1 | 5.80x | 5.91x |
| weighted geometric mean against SQLite, run 2 | 5.82x | 5.87x |
| unweighted geometric mean, run 1 | 5.07x | 5.14x |
| unweighted geometric mean, run 2 | 5.08x | 5.01x |
| this branch's time over 2.3.1's, geometric mean of 34 workloads | | 1.0013 |

Single workloads moved by up to 4% each way, inside the spread between the two runs of 2.3.1
(up to 7%). `write.upsert` and `write.delete` were 4% faster; `large.read` and `prepare.point`
were the slowest at 3.8% and 2.1%.

`prepare.point` was examined further, because it was slower in every run. It prepares
`SELECT label FROM main_table WHERE id = ?1` 4,000 times a round. Timed at every commit of the
branch, four interleaved runs of 30 rounds each, median in microseconds per round:

| build | 2.3.1 | round 3 fixes | LIMIT messages | nested USING | statement and value fixes | LIMIT revised | head |
|---|---|---|---|---|---|---|---|
| `prepare.point` | 9,936 | 10,053 | 10,074 | 10,267 | 9,961 | 10,139 | 10,134 |

The time does not grow with the work added: the build with every fix up to the statement and value
fixes is at 2.3.1's time, and the next build, which only changed how `LIMIT` is checked, is 1.8%
slower again. The release profile links with fat LTO and one codegen unit, so any change moves
where the whole binary's code lands, and this workload is short enough to feel it. Two changes that
do remove work from every prepare were kept anyway: `PRAGMA ignore_check_constraints` is set on the
binder without one more move of the whole binder, and the column mask of a scan maps record slots
to declared positions only for a table where the two can differ.

## 6. Found and not fixed

Each item is either listed in a corpus `known.toml` with the same reason, or written here because
it came from the TCL conversion, which is not a suite. The order is how likely a user is to meet
it.

### 6.1 Wrong or different answers

| Item | Why it is not fixed |
|---|---|
| `INSERT OR FAIL` of two rows where the second breaks a `UNIQUE` constraint and the first an immediate foreign key: SQLite reports the foreign key and keeps nothing | SQLite counts immediate foreign key violations per statement and checks them at the end; this engine checks each row as it is written. Changing it moves every foreign key check |
| With `recursive_triggers` on, `INSERT OR REPLACE` whose delete trigger inserts the replaced key again recurses to the trigger depth limit; SQLite reports the `UNIQUE` constraint | SQLite turns off the `REPLACE` for the inner insert. The error is now an error and not a stack overflow |
| An FTS5 table with `content=` a view returns no rows from a full scan | the scan reads content through the shadow table path, which needs a tree |
| `UNIQUE(x,x)` beside `UNIQUE(x,x) ON CONFLICT REPLACE`: the message names `t1.x, t1.x` where SQLite names `t1.x` | SQLite merges the two identical indexes into one when the table is created. Merging changes the `sqlite_autoindex` numbering |
| A `RIGHT JOIN` whose left side is empty, with `(a,b) IN (SELECT ...)` over an index: SQLite returns duplicate rows, and the reverse order `(b,a)` returns none | both are SQLite defects in its pass over unmatched rows, and this engine's answer is the correct one (`join-34.2`) |
| `SELECT (a, b) = (SELECT 'abc' COLLATE nocase, 1) FROM t` compares under `BINARY`; SQLite uses `NOCASE` in a result column (`rowvalueA-6.8`) | SQLite uses `BINARY` for the same comparison in a top level `WHERE` term, because it splits it into scalar comparisons there (`rowvalue-6.2`). Following both needs the binder to know whether a comparison is a top level `WHERE` term. A fix for the result column alone made the `WHERE` form return rows SQLite does not |
| `blobcol LIKE 'ab%'` answered through an index skips `BLOB` values; a scan finds them (`like3`) | SQLite seeks the index twice, once with text bounds and once with blob bounds. One wider range would read every text key above the prefix and slow every ordinary `LIKE` |
| `PRAGMA count_changes` and `PRAGMA full_column_names` are stored and have no effect | both are deprecated in SQLite; `full_column_names` would need a connection setting carried into plans cached by their SQL text |
| A `zeroblob(n)` value is built as `n` zero bytes; SQLite keeps it as a count until it is written. A row of three `zeroblob(1e9)` values now fails with "string or blob too big" as in SQLite, but after 5.4 s; an 800 MB row is written in 5.7 s against 1.7 s | a lazy blob value is a new kind of value through the whole executor |
| `printf` with `%!` and some widths of `%c`, the JSON5 forms `jsonb` refuses, `like3` cases on `BLOB` values | small; see the TCL report |
| SQLite answers differently from the correct answer, and this engine gives the correct one: `x IN (SELECT col ...)` over a `DESC` index on `col` misses the NULL in the subquery (`in-13.12`); a `GROUP BY` column read after a comparison sees the converted value (`in7-5.*`, `select1-24.1`); four join cases in `in7-6.1`, `in7-7.6`, `joinH-17.2`, `joinI-9.4` | copying a defect of the reference is not a fix |

### 6.2 Valid SQL refused

| Construct | Message |
|---|---|
| a correlated scalar subquery with no `FROM` used as a value, as in `WHERE t1.a = (SELECT x)` or `sum(x + (SELECT y))` | "the physical pass does not handle a correlated subquery used as a value" |
| an `ON` clause inside a parenthesised `RIGHT JOIN` that reads a table outside the parentheses | "the tree read for FROM term 0 does not carry column 0" |
| an aggregate inside the `ORDER BY` of a subquery in a result column | "misuse of aggregate" |
| `SELECT 'a' COLLATE nosuch` where the collation is never used to compare | "no such collation sequence" at bind time; SQLite refuses only where it compares |
| the FTS5 `trigram` tokenizer and `contentless_delete=1` | exit code 3, with a capability row |
| `ALTER TABLE ... ADD CONSTRAINT` forms other than `CHECK`, `temp.sqlite_sequence`, `main.temp_table` in a trigger | exit code 3 or "no such table" |

### 6.3 Slower than SQLite

| Case | Here | SQLite | Why |
|---|---|---|---|
| a correlated lookup into a materialized aggregate derived table | quadratic | an automatic index | `EXPLAIN` shows an automatic index that the executor does not build for a derived table |
| `LIKE` with a leading literal | about 2 times SQLite | | the pattern is matched per row without the prefix range SQLite derives |

## 7. Results

### 7.1 What each round found

| Round | Cases | Source | Differences at the start | Outcome |
|---|---|---|---|---|
| 1 and 2 | about 1,200 | hand probes, Turso issues, SQLite release notes and forum, performance pitfalls | 4.1 to 4.4 and section 5 | fixed, or listed in `compat/corpus/research/known.toml` |
| 3 | 10,657 | SQLite's own TCL test suite, converted to probe cases | 880 real, after 39 that differ only in row order | 455 left; the rest fixed |
| 4 | 490 | generated nested joins, `tools/bug-hunt/nested-joins.mjs` | none on this branch; 18 on 2.3.1 | kept as a nightly corpus |

The 455 differences left in round 3 are: 186 statements this engine refuses, 87 it accepts and
SQLite refuses, 141 that differ only in an error message, and 41 with different values. Most of the
41 are SQLite defects listed in section 6.1. Another 251 cases differ only in the random suffix
SQLite gives a duplicated column name in a header, such as `a:2621292967`.

### 7.2 Against the release

Every regression case in `compat/corpus/usage/bug-hunt-*.sql` was run against a build of 2.3.1.
73 of the 77 that existed at the time printed something different from SQLite there, and two of
them crashed or hung. So nearly every defect in section 4 is one a user of the current release can
meet.

### 7.3 Where it stops

The loop stopped when a round's new cases found no wrong answer that a user is likely to meet: the
nested join generator, written after a join bug, found nothing on this branch. What is left is in
section 6, each with its reason: SQLite defects that are not copied, refusals of rare forms, and
three changes too large for a bug fix (a lazy `zeroblob`, statement level foreign key counting, and
a `WHERE` term context for row value collation).

### 7.4 The suites

- `differential::usage_corpus`: 1,856 cases, 15 listed differences, unchanged from before.
- `nightly::research_corpus`: 1,696 cases, 22 listed differences.
- the library tests of every crate this changed, and `inillucent-testrun --changed`.
