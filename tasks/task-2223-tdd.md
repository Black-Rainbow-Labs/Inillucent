# Automatic indexes for joins and correlated subqueries

Design document. Written on 2026-10-09 against commit `e7f7bdc7` of `main`. Nothing in this document
is implemented. The session that finishes task-2217's open items builds from it.

The ticket asked for a design covering joins and correlated subqueries. Reading the code (section 3)
and measuring it (section 4) gave a narrower and more specific answer than the ticket's description:

- The automatic index for a **join** already exists. An unindexed inner side is read once into a
  hash table keyed on the join equality, under `PRAGMA automatic_index`, and `EXPLAIN QUERY PLAN`
  prints SQLite's lines for it. A join of two 10,000 row tables on one equality runs in 7 ms against
  SQLite's 5 ms. What is missing is three shapes the key extraction declines: a join condition that
  holds anything beside equalities, and an equality whose side is an expression and not a plain
  column. Those run 360 to 833 times slower than SQLite, and two of them print a plan line claiming
  an index the executor does not build.
- The automatic index for a **correlated subquery** does not exist. `EXISTS (SELECT 1 FROM b WHERE
  b.k = a.k)` over an unindexed `b.k` runs its block once per outer row and scans `b` every time:
  1.2 s against SQLite's 4.5 ms at 10,000 by 10,000 rows, 273 times slower. SQLite gets its speed by
  rewriting that `EXISTS` into a join loop with an automatic index. For a correlated block it does
  not rewrite (`NOT EXISTS`, a scalar `count(*)`), SQLite builds no automatic index and inillucent is
  already faster.
- Three **plan text** defects: the two joins above, the subquery inside `sum((SELECT ...))` prints
  no node, and a correlated `LIST SUBQUERY` prints a `CREATE BLOOM FILTER` line SQLite does not.

So the design has three parts, each with its own tests, and they can be built and merged in any
order. Part A, the correlated block's built table, is the one the ticket is about and the largest.
Part B, the join key extraction, is small and fixes the largest measured ratios. Part C is plan text.

## 1. Terms used in this document

| Term | Meaning here |
|---|---|
| block | one subquery, as the binder produced it: a `BoundSelect` with its own `FROM`, `WHERE`, projection and aggregates |
| correlated block | a block that reads a column of a `FROM` term it does not own, such as `a.k` inside `(SELECT ... FROM b WHERE b.k = a.k)` |
| feed | the pair the executor records for each such read: the joined row column that supplies the value, and the parameter number the reference was rewritten to. `Correlation::feeds` in `crates/inillucent-exec/src/correlate.rs` |
| the chain | the block's compiled operator pipeline, built at its first outer row and run again for each one after. `Correlation::chain`, type `crate::compiled::Compiled` |
| the gate | the `WHERE` conjuncts that read no subquery, tested on a row before any block is answered for it. `Correlated::gate` |
| stage | one `FROM` term's physical read: a tree, an access kind and a column width. `PreparedStage` in `crates/inillucent-exec/src/physical/stages.rs` |
| residual | the `WHERE` conjuncts the planner left for a stage to test after its access path has run. `plan.residuals[position]` |
| key equality | a conjunct of the form `column = other` or `other = column`, where the column belongs to the inner table and `other` reads only the outer side; in a block, `other` is a feed parameter |
| build filter | the conjuncts that read only the inner table and constants, applied once while the table is read |
| the built table | the hash table Part A adds: the block's table read once, keyed on the key equalities, held for one execution of the statement |
| lever | one switch in `inillucent_sql::plan::Levers`. `Levers::AUTOMATIC_INDEX` is what `PRAGMA automatic_index` sets and clears |
| the pinned SQLite | the `sqlite3` 3.53.4 build under `.sqlite-ref/`, which every differential test compares against |
| the measurements file | `_agent_output/task-2223-autoindex/measurements.md` in the main checkout, with `eqp.txt` beside it holding every plan verbatim. Not tracked by git; section 4 is the copy that lasts |

## 2. Goals

1. A correlated block whose only way to reach its rows is a table scan with an equality against
   the outer row reads that table once per statement execution and probes a hash table per outer
   row. The answer of every statement is unchanged.
2. A join whose condition holds a key equality beside other terms, or whose key equality has an
   expression on one side, is run through the hash join the plan text already claims for it.
3. `EXPLAIN QUERY PLAN` prints what the pinned SQLite prints for each of these shapes where the two
   engines do the same thing, and prints what inillucent does where they differ, with the difference
   listed in `docs/sql.md` and `compat/corpus/usage/known.toml`.
4. `PRAGMA automatic_index = OFF` turns every one of these off, and the plan text follows it.
5. Memory a built table takes is counted against the request budget the way the hash join's build
   side already is, and a budget the build would exceed makes the block fall back to the per row
   scan; the statement does not fail.
6. The 10,000 by 10,000 row correlated `EXISTS` and the three slow joins finish within about three
   times the pinned SQLite's time.

Not goals: a planner cost model that decides between a built table and a scan (section 5.2 says
why none is needed); decorrelating `EXISTS` into a join term of the outer query (section 10);
spilling a built table to disk (section 7); changing the join order the planner chooses (section
10).

## 3. What the code does today

### 3.1 The join: already an automatic index, with a narrow key rule

`crates/inillucent-exec/src/autoindex.rs` is the module `PRAGMA automatic_index` controls. Its
`equi_keys(condition, offset, width)` splits a join condition into conjuncts with `flatten_and` and
accepts the whole condition only when **every** conjunct is a plain or affinity carrying `Equal`
whose two sides each read only one side of the join. `side_of` decides which side an expression
reads through `collect_columns`, which walks only `Expr::Column` and `Expr::Literal` and returns
`None` for anything else, and `rebased` moves an inner column's number down by `offset` for the
same two leaf shapes only. The comment on `rebased` says "the whole value of this optimisation is in
the plain `t.a = u.b` case anyway". Section 4 shows it is not: `a.k = b.k + 1` is 497 times slower
than SQLite because of this rule, and `a.k = b.k AND a.id < b.id` 360 times.

Two places in `crates/inillucent-exec/src/physical/joins.rs` use it:

- `build_nested`, at the comment "And a third shape: an inner term with nothing to seek on" (about
  line 361). When the inner term has no seek key and the lever is on and `has_equi_key` says the
  residual offers one, the term is built through `build_materialised_join`.
- `build_materialised_join`, at the comment "The automatic index" (about line 790). The inner side
  has already been read into `rows` by `materialise_stage`. With the lever on, no correlation in the
  condition and a key from `equi_keys`, the rows go into `crate::join::HashJoin` through
  `build_materialised` and every outer row probes it. The residual stays in the `Filter` above the
  join. Otherwise a `NestedLoopJoin` walks the vector per outer row, which is what the lever off
  selects.

`keyable` (about line 557) feeds those two the `ON` for an outer join, or the stage's residual for an
inner one, with any conjunct that holds a subquery removed. A non outer join's `ON` is split into the
planner's term list before paths are chosen, so `b.p = a.x` arrives as the inner stage's residual
and `source_term.on` is empty.

`crates/inillucent-exec/src/join/hash.rs` holds `HashTable`: `HashMap<Vec<u8>, Vec<u32>>` from the
memcmp encoded key to row positions, the rows themselves, and a matched flag per row for `RIGHT` and
`FULL`. A `NULL` in the key is not inserted, because SQL equality is never true for `NULL`. The build
side is charged to the request budget per batch with `inillucent_base::budget::materialise`. The
probe (`Sink::push`, about line 250) encodes the probe keys, reads the bucket and emits one joined
row per bucket entry; a `LEFT` probe row with an empty bucket is null extended there. `JoinKind`
already has `Semi` and `Anti` and `HashJoin` and `NestedLoopJoin` both implement them, but nothing in
`physical/` builds a join of either kind today.

The plan text is `automatic_index_lines` in `crates/inillucent-sql/src/plan/describe.rs`. It prints
`BLOOM FILTER ON b (k=?)` and `SEARCH b USING AUTOMATIC COVERING INDEX (k=?)`, with ` LEFT-JOIN`
after the search for a left join, for a table or a derived table that is not the first loop, when
the lever is on and the term's `ON` or residual holds **at least one** equality whose one side is a
plain column of this term (`key_column`) and whose other side reads only earlier loops or an
enclosing query (`reads_only`). Other conjuncts are skipped. That is why the Q9 and Q11 plans in
section 4 print an index the executor declines: the describer takes the equalities it finds, the
extractor refuses the condition if anything else is present. And it is why Q10 prints no index on
either side: `b.k + 1` is not a `key_column`.

### 3.2 The correlated block: planned once, scanned per row

`crates/inillucent-exec/src/correlate.rs`. The module comment states the invariant: a correlated
block is planned once and run per row against bound parameters, through the ordinary planner and
the ordinary pipeline. `prepare_blocks` (about line 764) rewrites each outer column reference into a
parameter numbered from `ENGINE_PARAMETER_BASE`, records the feed, plans the block with
`plan_select_with(block, levers)` and prepares it once with `prepare_any`.

Per outer row, `Correlated::push` (about line 218) tests the gate, writes the feeds into one copy of
the parameters, and calls `Correlation::answer_from`, which calls `Correlation::run` (about line
571). `run` tries three things in order:

1. `probe_directly`: an `EXISTS` block that reduced to one equality prefix seek of a real index is
   answered by `holds_key` on that tree. This is `DirectProbe`, found by the first row's chain run
   when `compiled.answers_existence(plan)` holds and the single stage is `AccessKind::Span` over an
   `IndexSeek` whose equalities are all feed parameters. It cannot help an unindexed column: there
   is no tree to seek.
2. `exists_by_chain` and `run_chain`: the kept chain, built by `try_compile_limited` at the first row
   and reused while `compiled.built_under(bound)` holds. Its source is whatever `source_for_run` in
   `crates/inillucent-exec/src/physical/chain.rs` (about line 2010) builds for the first stage. For
   `SELECT 1 FROM b WHERE b.k = ?N` with no index on `k` that is a `FullScan` of `b`, and the
   residual `b.k = ?N` is a `Filter` above it. Every outer row scans `b`.
3. `run_any_prepared_limited`, when the chain cannot be kept.

### 3.3 Where the rows of a stage come from, and in what shape

`materialise_stage` in `physical/joins.rs` (about line 955) reads a table stage with
`FullScan::new(tree, Projection::all(stage.width)).with_needed(stage.needed)` into a
`CollectInto`. The rows have `stage.width` columns, and columns the statement does not read are
`NULL`. `build_materialised_join` hands exactly these rows to `HashJoin::build_materialised` and to
`NestedLoopJoin`, both `with_widths(stage.offset, stage.width)`, and the operators above read the
columns by position. `Source::Rows(rows)` in `physical/stages.rs` (about line 395) drives a chain
from such a buffer with `emit_rows`. These three facts are what section 5.3 rests on: rows read by
`materialise_stage` are the shape the chain above the source expects.

### 3.4 The plan text for a block

`crates/inillucent-sql/src/plan/tree.rs`, `subquery_nodes_into` (about line 420). For each
subquery used as a value it prints `CORRELATED SCALAR SUBQUERY n` or `LIST SUBQUERY n`, plans the
block again with `plan_select_with((**block).clone(), levers)` and prints the block's own tree one
level deeper. In that plan the outer reference is still a `Column` of a source that is not one of
the block's own, which is how `automatic_index_lines` can tell "reads an enclosing query": a source
not in `local` passes `reads_only`. What stops the line printing today is the test
`eligible = (position > 0 || derived) && ...`: the block's table is its first loop.

`subqueries_in` (about line 482) collects the subqueries of an expression through
`BoundExpr::children()`. Q8's `sum((SELECT count(*) FROM b WHERE b.k = a.k))` prints no node at all,
so either `children()` does not descend into an aggregate call's arguments or the aggregate is held
somewhere `expression_subqueries` does not look. The implementer finds which; Part C names the test.

`subquery_nodes_into` prints `CREATE BLOOM FILTER` under every `IN` subquery in the `WHERE`. SQLite
prints it only under an uncorrelated one (Q13 yes, Q6 no), because a correlated `IN` is re run per
row and there is nothing to build a filter over once.

### 3.5 The pragma and the lever

`PRAGMA automatic_index` sets and clears `Levers::AUTOMATIC_INDEX` on the connection, and the plan
cache keys on the levers (`inillucent-compat::engine::plan_cache`). `correlations_of` passes
`plan.levers` to `prepare_blocks`, so a block's plan already carries the statement's levers. Nothing
new is needed for the switch itself.

## 4. Measurements

### 4.1 Procedure

Release build of `inillucent-shell` from the worktree (inillucent 2.4.2), the pinned `sqlite3` from
`.sqlite-ref/3.53.4/shell/sqlite3.exe`. Both tables unindexed on `k`:

```sql
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 10000, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 10000);
INSERT INTO b SELECT value, (value * 7) % 10000, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 10000);
```

Each statement is timed through the shell with `.timer on`, one process per run, three runs, the
median of the real time. The result value is recorded from both engines and must agree.
`EXPLAIN QUERY PLAN` is captured from both engines with the pragma at its default and again after
`PRAGMA automatic_index = OFF`. inillucent's timer prints three decimals, so its values under 10 ms
are coarse. Two `cargo.exe` processes from other work were running on the machine.

| Name | Statement |
|---|---|
| Q1 | `SELECT count(*) FROM a JOIN b ON a.k = b.k` |
| Q2 | `SELECT count(*) FROM a, b WHERE a.k = b.k` |
| Q3 | `SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k` |
| Q4 | `SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k WHERE b.id IS NULL` |
| Q5 | `SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k)` |
| Q6 | `SELECT count(*) FROM a WHERE a.k IN (SELECT k FROM b WHERE b.id > a.id)` |
| Q7 | `SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k)` |
| Q8 | `SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a` |
| Q9 | `SELECT count(*) FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0` |
| Q10 | `SELECT count(*) FROM a JOIN b ON a.k = b.k + 1` |
| Q11 | `SELECT count(*) FROM a JOIN b ON a.k = b.k AND a.id < b.id` |
| Q12 | `SELECT count(*) FROM a x JOIN a y ON x.k = y.k JOIN b z ON z.k = y.k` |
| Q13 | `SELECT count(*) FROM a WHERE k IN (SELECT k FROM b)` |
| Q14 | `SELECT count(*) FROM a JOIN (SELECT k, count(*) n FROM b GROUP BY k) s ON s.k = a.k` |

The same procedure is run after the change. The after column is what the engine tests in section 8
fix a limit on, at about ten times the fixed code's time, which is the house rule
`crates/inillucent-compat/tests/engine/write_speed.rs` states in its module comment.

### 4.2 Before any change, 10,000 by 10,000 rows, default settings

Taken on 2026-10-09. Every answer matched.

| Query | inillucent | SQLite | inillucent / SQLite | Answer |
|---|---|---|---|---|
| Q1 | 0.007 s | 0.0053 s | 1.3x | 10000 |
| Q2 | 0.007 s | 0.0044 s | 1.6x | 10000 |
| Q3 | 0.008 s | 0.0048 s | 1.7x | 10000 |
| Q4 | 0.007 s | 0.0050 s | 1.4x | 0 |
| **Q5** | **1.219 s** | **0.0045 s** | **273x** | 10000 |
| Q6 | 1.245 s | 10.46 s | 0.12x | 4999 |
| Q7 | 1.226 s | 1.717 s | 0.71x | 0 |
| Q8 | 2.185 s | 3.498 s | 0.62x | 10000 |
| **Q9** | **2.836 s** | **0.0034 s** | **833x** | 5000 |
| **Q10** | **2.207 s** | **0.0044 s** | **497x** | 9999 |
| **Q11** | **2.001 s** | **0.0056 s** | **360x** | 4999 |
| Q12 | 0.016 s | 0.0082 s | 2.0x | 10000 |
| Q13 | 0.004 s | 0.0048 s | 0.8x | 10000 |
| Q14 | 0.010 s | 0.0056 s | 1.8x | 10000 |

With `PRAGMA automatic_index = OFF` on both engines: Q1 1.365 s against 1.729 s, Q5 1.153 s against
0.918 s, Q8 1.984 s against 1.937 s. With the switch off the engines are within 1.3x of each other,
so every large ratio above is SQLite building an index that inillucent does not build.

At 2,000 by 2,000 rows: Q5 0.070 s against 0.0007 s with the switch on (98x) and 0.071 s against
0.057 s with it off; Q8 0.075 s against 0.070 s on, 0.085 s against 0.074 s off. The Q5 gap grows
with the row count the way a quadratic plan against a linear one does.

### 4.3 The plans, verbatim

Where the two engines print the same thing the line is given once.

| Query | inillucent | SQLite |
|---|---|---|
| Q1, Q2 | `SCAN a` / `BLOOM FILTER ON b (k=?)` / `SEARCH b USING AUTOMATIC COVERING INDEX (k=?)` | same |
| Q3, Q4 | as Q1 with ` LEFT-JOIN` after the search | same |
| Q5 | `SCAN a` / `CORRELATED SCALAR SUBQUERY 1` / `  SCAN b` | `SCAN a` / `BLOOM FILTER ON b (k=?)` / `SEARCH b EXISTS USING AUTOMATIC PARTIAL COVERING INDEX (k=?)` |
| Q5, switch off | unchanged | `SCAN a` / `SCAN b EXISTS` |
| Q6 | `SCAN a` / `CORRELATED LIST SUBQUERY 1` / `  SEARCH b USING INTEGER PRIMARY KEY (rowid>?)` / `  CREATE BLOOM FILTER` | the same without the `CREATE BLOOM FILTER` line |
| Q7 | `SCAN a` / `CORRELATED SCALAR SUBQUERY 1` / `  SCAN b` | same |
| Q8 | `SCAN a` and nothing else | `SCAN a` / `CORRELATED SCALAR SUBQUERY 1` / `  SCAN b` |
| Q9 | `SCAN a` / `BLOOM FILTER ON b (k=?)` / `SEARCH b USING AUTOMATIC COVERING INDEX (k=?)` | `SCAN b` / `BLOOM FILTER ON a (k=?)` / `SEARCH a USING AUTOMATIC COVERING INDEX (k=?)` |
| Q10 | `SCAN a` / `SCAN b` | `SCAN b` / `BLOOM FILTER ON a (k=?)` / `SEARCH a USING AUTOMATIC COVERING INDEX (k=?)` |
| Q11 | `SCAN a` / `BLOOM FILTER ON b (k=?)` / `SEARCH b USING AUTOMATIC COVERING INDEX (k=?)` | same |
| Q11, switch off | `SCAN a` / `SCAN b` | `SCAN a` / `SEARCH b USING INTEGER PRIMARY KEY (rowid>?)` |
| Q12 | `SCAN x` / `BLOOM FILTER ON y (k=?)` / `SEARCH y USING AUTOMATIC COVERING INDEX (k=?)` / `BLOOM FILTER ON z (k=?)` / `SEARCH z USING AUTOMATIC COVERING INDEX (k=?)` | same |
| Q13 | `SCAN a` / `LIST SUBQUERY 1` / `  SCAN b` / `  CREATE BLOOM FILTER` | same |
| Q14 | `CO-ROUTINE s` / `  SCAN b` / `  USE TEMP B-TREE FOR GROUP BY` / `SCAN a` / `BLOOM FILTER ON s (k=?)` / `SEARCH s USING AUTOMATIC COVERING INDEX (k=?)` | same |

Four facts from this table drive the design:

1. **SQLite builds an automatic index inside a correlated subquery node for none of these.** Q7
   and Q8 print `SCAN b` under the node and take 1.7 s and 3.5 s. Its 273x on Q5 comes from
   rewriting the `EXISTS` into a loop of the outer query, printed with the word `EXISTS` on the loop
   line, and building the index for that loop. The rewrite is not applied to `NOT EXISTS`.
2. **The Q9 and Q11 plan text is wrong today.** The line says the index is used; the run time says
   it is not. Part B makes the executor do what the line says.
3. **SQLite orders Q9 and Q10 the other way round**, scanning `b` and indexing `a`. inillucent keeps
   `a` outside. After Part B inillucent's Q9 text stays as it is and SQLite's differs in order. For
   Q10 inillucent keys on the expression `b.k + 1`; section 6.3 says what it prints.
4. **Q8 prints no subquery node**, and Q6 prints a bloom filter line SQLite does not. Part C.

### 4.4 Which `EXISTS` shapes SQLite rewrites

Recorded on 2026-10-09 from the pinned shell on 500 row tables after `ANALYZE`, in
`_agent_output/task-2223-autoindex/shapes/plans.md`, 28 statements in both settings. The rule the
shell follows, read off those plans:

**SQLite rewrites an `EXISTS` into a loop of the outer query**, printed as `BLOOM FILTER ON b (...)`
and `SEARCH b EXISTS USING AUTOMATIC PARTIAL COVERING INDEX (...)`, and as `SCAN b EXISTS` with the
switch off, when all of these hold:

- the `EXISTS` is not negated and is a conjunct of the top level `WHERE` (S1, S6, S15 with two of
  them, S11 under a `GROUP BY`, S12 where the block reads a `LEFT JOIN` term, S19 over the outer
  query's own table, S28 with the equality written the other way round);
- the block's `FROM` is one table (S10 with two tables keeps the node);
- the block has no aggregate and no `LIMIT` (S9 and S8 keep the node);
- any other conjunct of the block's `WHERE` is allowed: `b.active = 1` (S2), a second key (S3),
  `b.id > a.id` (S4), `b.pad LIKE 'b%'` (S27). A conjunct that reads only the block's table enters
  the printed key when it is an equality (`(k=? AND active=?)` for S2) and is tested in the loop
  when it is not (S4 prints `(k=?)`; with the switch off S4 prints `SEARCH b EXISTS USING INTEGER
  PRIMARY KEY (rowid>?)`).

**SQLite keeps a `CORRELATED SCALAR SUBQUERY` node and scans inside it** for `NOT EXISTS` (S5), an
`EXISTS` under `OR` (S7), in a `LEFT JOIN`'s `ON` (S13), in the result columns (S14), every scalar
block (S16, S17, S20 in an `UPDATE`), and keeps a `CORRELATED LIST SUBQUERY` node for a correlated
`IN` (S18). With a real index `bk` on `b(k)`, the loop form prints `SEARCH b EXISTS USING COVERING
INDEX bk (k=?)` and the node form prints `SEARCH b USING COVERING INDEX bk (k=?)` under the node.

The word `PARTIAL` appears on every `EXISTS` loop line, with or without a filter term, and on a
join's automatic index line when the inner table has a conjunct of its own: S21, `b CROSS JOIN a ON
a.k = b.k WHERE a.id % 2 = 0`, prints `SEARCH a USING AUTOMATIC PARTIAL COVERING INDEX (k=?)`, and
S25, `LEFT JOIN b ON a.k = b.k AND b.active = 1`, prints `(active=? AND k=?)` with `PARTIAL`. Q1,
with nothing but the equality, prints no `PARTIAL`. The key order rule in `automatic_index_lines`
holds for constant equalities too: S2 writes `b.k` and `b.active` on the left and prints `k=? AND
active=?`; S25 writes `a.k = b.k`, which is flipped, and `b.active = 1`, and prints `active=? AND
k=?`.

An expression side gets no automatic index from SQLite in either order: S22 `b CROSS JOIN a ON a.k
= b.k + 1` indexes `a` on the plain `k`, and S23 `a CROSS JOIN b ON a.k = b.k + 1` prints `SCAN a` /
`SCAN b` with the switch on or off.

**A trigger's writes are seen row by row.** Part 2 of the same file: with `b` holding three rows
per `k`, a trigger `AFTER UPDATE ON a` inserting `new.k` into `b`, and `UPDATE a SET pad = (SELECT
count(*) FROM b WHERE b.k = a.k)` over 30 rows of `a` with `k = id % 10`, SQLite sets `pad` to 3 for
ids 1 to 10, 4 for 11 to 20 and 5 for 21 to 30, with the switch on or off. inillucent answers the
same today because it scans per row. A built table read before the first row would answer 3 for
every row. Section 5.2 condition 4 is written for this.

## 5. Part A: the built table for a correlated block

### 5.1 One sentence

When a correlated block's only stage is a full scan of a table and its residual holds an equality
between a column of that table and a feed parameter, the first outer row that reaches the block
reads the table once into a hash table keyed on those columns, and every outer row after it drives
the block's chain from the matching bucket instead of from the scan.

This is more than SQLite does. SQLite builds the index only for the `EXISTS` shapes it can rewrite
into a join; inside a subquery node it scans. The built table answers every block shape through the
one mechanism `correlate.rs` already has, so `NOT EXISTS`, a scalar `count(*)` in a result column
and an `UPDATE ... SET x = (SELECT ...)` all get it. Section 10 says why this and not SQLite's
rewrite.

### 5.2 When it is chosen

The decision is made once per statement, in `prepare_blocks`, from the block's plan and its
prepared stages. It is a shape test and not a cost estimate, for the same reason the join's is:
building the table costs one scan of the inner table plus one hash insert per row, and the first
outer row was going to pay that scan anyway. A statement with one outer row pays the inserts for
nothing; one with two pays them back. SQLite builds its automatic index before the first outer row
on the same reasoning.

Every condition below must hold. A block that fails any of them is prepared exactly as today.

1. `plan.levers.has(Levers::AUTOMATIC_INDEX)`.
2. `prepared.stages` has exactly one stage, its `kind` is `AccessKind::Full`, its term's `path` is
   `AccessPath::TableScan`, the term's table `kind` is `TableKind::Table`, and the stage has no
   module `functions`. A block with a join inside it runs its joins through the ordinary builder,
   which uses the hash join; a block over a view, a virtual table or a derived table is out of scope.
3. The stage's residual, `plan.residuals[0]`, split with `inillucent_sql::plan::conjunction`, holds
   at least one key equality: `BoundExpr::Compare { op: Equal, left, right, .. }` where one side is
   `BoundExpr::Column { source, column, .. }` with `source` the term's id and the other side is
   `BoundExpr::Parameter(n)` with `n` one of the block's feeds. The collation must be `BINARY`, for
   the reason `equi_keys` gives: the key is bytes and `NOCASE` equality is not byte equality. The
   affinity the comparison carries is kept with the key and applied to both sides, as `equi_keys`
   does. A `Rowid` on the column side never arrives here; the planner has made that a `RowidSeek`.
4. The block is not `once`. When the statement writes a table (`target_root` is `Some`), two more
   things must hold: `!reads_table(&block, root)`, the test `once` already uses, which keeps a block
   over the table being changed on the per row scan; and the write can run nothing that writes
   another table. That means no trigger on the target table and no foreign key with an `ON DELETE`
   or `ON UPDATE` action that names the target as its parent. Section 4.4 measured why: SQLite's
   scalar block sees a trigger's inserts row by row, and a table built before the first row would
   not. The three call sites that prepare blocks for a write, `correlations_in_every` in
   `dml/answers.rs` and `insert_plan.rs` and `correlations_in` in `dml/update.rs`, already know the
   target; they pass one more boolean, computed from the catalog's trigger and foreign key records
   for the target, and `prepare_blocks` declines when it is set. A `SELECT` fires nothing, so the
   rule costs it nothing. Finer rules, such as reading each trigger body for the tables it writes,
   are a follow on if a measured statement needs them.

The other residual conjuncts fall into two groups. The **build filter** is every conjunct that
reads only the block's own term and holds no subquery and no feed parameter: `b.active = 1`,
`b.pad <> ''`, `b.id > ?1` where `?1` is the statement's own parameter. It is applied once while the
table is read, which is what SQLite's `PARTIAL` in `AUTOMATIC PARTIAL COVERING INDEX` means. The
**rest**, such as `b.id > ?feed` from Q6, is tested per row by the chain's own `Filter`.

**The whole residual stays in the chain.** This is the rule the join already follows and the reason
is the same: the key enforces the key equalities, but removing them from the filter would mean
proving the key captured the whole predicate, and the key extraction deliberately declines to look
inside anything it does not recognise. A second test of an equality the bucket already satisfied
costs one comparison per surviving row and cannot change an answer.

The decision is recorded on `Correlation`:

```rust
/// How the block reads its table through a built table, when it does.
automatic: Option<AutomaticKey>,
/// The built table, read at the first outer row of an execution and dropped
/// at the end of it.
built: std::cell::RefCell<Option<BuiltTable>>,
```

`AutomaticKey` holds, in residual order, each key equality's table column index, the feed's joined
row column, and the affinity; the build filter as one `BoundExpr` conjunction or `None`; and a flag
`existence_only`, true when the block `answers_existence` and the residual holds nothing but key
equalities and build filter conjuncts.

### 5.3 How it is built

At the first outer row that passes the gate, inside `Correlation::run` and before the existing
`probe_directly` and chain steps, when `automatic` is `Some` and `built` is `None`:

1. Read the table with `materialise_stage(&self.plan, catalog, bound, stage, None)`. This is the
   call `build_materialised_join` makes, so the rows have the stage's width and the columns the
   block reads, with the rest `NULL`.
2. When there is a build filter, compile it once over the stage's types the way
   `build_materialised_join` compiles a condition over the inner row (`translate_scan` with a
   `Space` of the one stage, then `compile`), and drop the rows it rejects.
3. For each row, encode the key columns through the memcmp key codec after applying the key's
   affinity, exactly as `encode_row_key` in `join/hash.rs` does, and insert into a `HashTable`. A
   `NULL` in any key column is not inserted. Charge `inillucent_base::budget::materialise` with the
   rows' bytes, as `HashJoin::build` does per batch.
4. Store the `HashTable` in `built`.

Move `HashTable`, `encode_row_key` and the insert from `join/hash.rs` into `autoindex.rs` as
`pub(crate)` so the join and the block share one structure. That keeps everything `PRAGMA
automatic_index` controls in the module named for it.

If step 1 or step 3 returns the budget's `Bytes` error, drop whatever was built, set `automatic` to
`None` for the rest of the statement, and continue to the chain as today. Section 7 says why that
and not a failure.

### 5.4 How it is probed

Per outer row, when `built` is `Some`:

1. Evaluate each key's feed through `feed(column)`, convert with `constant::seek_value(value,
   affinity)` as `probe_directly` does, and encode the key. A `NULL` in any key value means an empty
   bucket, because the equality is never true. Do not short circuit the block to `false` or `NULL`
   from here: an aggregate over zero rows is `0` for `count(*)` and `NULL` for `max(x)`, and only
   running the chain over an empty source answers both correctly.
2. When `existence_only` is set, the answer is whether the bucket is non empty, negated when `NOT`
   was written. No chain run. This is `DirectProbe` for a built table, and it is what makes Q5 and
   Q7 one hash lookup per outer row.
3. Otherwise look the key up, copy the bucket's rows into a `Vec<Vec<OwnedDatum>>`, and run the
   block's chain from them. This needs one new entry on `Compiled`:

   ```rust
   /// Runs the chain over rows the caller supplies in place of its first
   /// stage's source. The rows must have that stage's width.
   pub fn run_over_rows(&mut self, plan: &PhysicalPlan, catalog: &dyn TreeCatalog, params: &Params, rows: Vec<Vec<OwnedDatum>>) -> DbResult<()>
   ```

   and the matching `any_row_over_rows` for an `EXISTS` block that `answers_existence` but has a
   residual the key did not capture. Both build `Source::Rows(rows)` where `run` and `any_row` call
   `source_for_run`, and otherwise do what those two do. `source_for_run`'s two early returns,
   `LIMIT 0` and a constant filter that rejects, are kept. Copying the bucket per probe is fine for
   the first version: a bucket is the rows of one key value. If the copy shows in a profile,
   `Source::Rows` can take a borrowed slice of positions later.

The chain's `built_under(bound)` check and the parameter writes in `run` stay as they are: the chain
is still run with the feeds bound, so a residual conjunct that reads a feed parameter sees the right
value.

### 5.5 Lifetime

The built table belongs to one execution of the statement. `Correlation` is kept across executions
through `KeptBlocks`, and `Correlated::reset` already clears the per execution `bound`; it also
clears every block's `built`. `Correlated::finish` clears them too, so a prepared statement that is
not reset before it is dropped does not hold the table. A statement executed twice builds twice and
sees any write between the two executions, which is the rule the chain's `built_under` already
gives.

`IndexNestedLoopJoin` answers blocks through `Correlation::answer` for joined rows; the built table
is reached through the same `run`, so that path gets it with no further work, and the same `reset`
clears it.

### 5.6 The lowered correlated `IN`

`crates/inillucent-sql/src/correlated_in.rs` rewrites `x IN (SELECT y FROM s WHERE p)` into three
`EXISTS` tests over the same block: any row, a row equal to the operand, a `NULL` row. The second
is the one with a key equality and it gets the built table with no further work. The first and
third have no key equality and stay as scans. Sharing one built table between the three, keyed on
the table root and the build filter, would make all three a lookup; it is a follow on. The second
block is the one that runs on every row, the first is answered `once` when the block has no
correlation of its own, and the third runs only when the second found nothing.

### 5.7 Plan text for a block

Two forms, chosen by the rule section 4.4 read off the pinned shell, so that the common case grades
equal in the corpus and every other case prints what runs.

**The loop form.** When the block gets the built table and SQLite would rewrite it, print it as
SQLite does: no subquery node; after the enclosing query's loops, `BLOOM FILTER ON b (<keys>)` and
`SEARCH b EXISTS USING AUTOMATIC PARTIAL COVERING INDEX (<keys>)`, and with the lever off `SCAN b
EXISTS`. The condition is the one in section 4.4: a positive `EXISTS` that is a conjunct of the top
level `WHERE`, over one table, with no aggregate and no `LIMIT`. `<keys>` follows the existing
order rule in `automatic_index_lines`, with constant equalities included, so S2 prints `(k=? AND
active=?)`. This is `tree.rs` work: `subquery_nodes_into` skips such a block and `tree_of` appends
the two lines after the loops, which is where SQLite puts them (S12 shows them after the `LEFT-JOIN`
search of `c`). The engine does not rewrite anything; the words describe a per row hash probe that
decides existence, which is what a semi join loop is. The existing `BLOOM FILTER` line is already
printed for a hash join that has no bloom filter, on the same reasoning.

**The node form.** Every other block that gets the built table keeps its `CORRELATED SCALAR
SUBQUERY n` node and prints `SEARCH b USING AUTOMATIC COVERING INDEX (<keys>)` under it, with
`PARTIAL` when a build filter exists, and `SCAN b` with the lever off. SQLite prints `SCAN b` there
and takes the slow path, so this is a difference, in inillucent's favour, listed in `docs/sql.md`'s
plan differences row and in `known.toml` for the corpus cases that show it (Q7, Q8, S16, S17, S20).
The implementation is the one line in `automatic_index_lines` that section 3.4 names: a term at
position 0 is eligible when it is a table and the equality's other side reads a source outside
`local`.

A block that does not get the built table prints as today.

## 6. Part B: the join key extraction

### 6.1 What changes in `equi_keys`

`equi_keys` returns the key equalities it finds and the conjuncts it did not take, instead of
declining the whole condition when one conjunct is not a key:

```rust
pub struct EquiKeys {
    pub probe: Vec<Expr>,
    pub build: Vec<Expr>,
    /// The conjuncts that are not key equalities, joined again with `And`,
    /// over the joined row. `None` when the key took every conjunct.
    pub rest: Option<Expr>,
}
```

A conjunct becomes a key when it is an `Equal` whose sides each read only one side of the join;
every other conjunct goes into `rest`. `None` is still returned when no conjunct is a key, so a
condition of nothing but ranges builds the nested loop it always did.

`collect_columns` walks every `Expr` node that can hold a column; today it walks `Column` and
`Literal` alone.
`Expr` has a children walk already for other passes; use it, so `b.k + 1`, `lower(b.name)`, `CAST`,
and `CASE` all report the columns they read. Only a subquery still returns `None`, and `keyable`
has already removed those. `rebased` becomes a general rewrite of every `Column(at)` to
`Column(at - offset)` through the same walk. The comment that said the plain case was the whole
value of the optimisation is replaced by the Q10 number.

### 6.2 What changes in `HashJoin`

`HashJoin` gains `with_condition(rest: Box<dyn Eval>)`, compiled over the joined row's types the way
`build_materialised_join` compiles the nested loop's condition. In `Sink::push`, for each bucket
entry, the joined row is formed and the condition evaluated; a pair that fails is not a match. For
`Inner` the pair is dropped. For `Left` a probe row whose every bucket entry failed is null extended,
exactly as one with an empty bucket is. For `Right` and `Full` the `matched` flag is set only for an
entry that passed. For `Semi` and `Anti`, a match is a bucket entry that passed.

This is what makes `rest` safe for an outer join: the key decides which pairs are candidates, the
condition decides which candidates are partners, and "no partner" is what null extends. Without the
condition on the join, a `LEFT JOIN ... ON a.k = b.k AND b.active = 1` keyed on `k` alone would drop
the null extended rows for keys whose partners are all inactive. The nested loop had this right
because it tested the whole `ON` per pair; the hash join must too.

For an inner join `rest` is also still tested by the residual `Filter` above, which is the rule in
section 5.2 and costs one comparison per surviving row.

### 6.3 What changes in the two builders and the describer

`build_materialised_join` and `has_equi_key` call the new `equi_keys` and pass `rest` to
`with_condition`. `keyable` is unchanged.

`automatic_index_lines` needs one change for Q10: `key_column` accepts an expression side when the
expression reads only this term's columns, and renders it as `<expr>`, which is how SQLite prints
an expression column of a real index (`SEARCH t USING INDEX i (<expr>=?)`). SQLite would not build an
automatic index on an expression; it reorders the join so the plain column is inside. inillucent's
line for Q10 is therefore `SEARCH b USING AUTOMATIC COVERING INDEX (<expr>=?)` against SQLite's
`SEARCH a USING AUTOMATIC COVERING INDEX (k=?)` after `SCAN b`. That is a plan difference to list
in `docs/sql.md` and `known.toml`, under the existing sentence that this engine may join in another
order. The alternative, teaching the planner to prefer the order with a plain column key, is in
section 10.

Where the inner side has a conjunct of its own, SQLite prints `AUTOMATIC PARTIAL COVERING INDEX`
and, when that conjunct is an equality against a constant, puts its column into the key list:
section 4.4, S21 and S25. The describer prints `PARTIAL` when the term's `ON` or residual holds a
conjunct that reads only this term, and renders a constant equality of this term's column as one
more `col=?` in the key list under the existing order rule. The executor already applies such a
term before the join since task-2217, so the word describes what runs; the extra key column is text
only, since the hash key does not need a constant.

## 7. Memory

A built table holds the inner table's rows in the shape `materialise_stage` produces, plus one
encoded key and one `u32` per row. That is the memory the hash join's build side already takes for
the same table, and it is counted the same way: `inillucent_base::budget::materialise`, which does
nothing when no budget is armed and refuses when the request's byte limit is passed. The MCP server
and the command line arm a budget; the embedded driver does not unless the application does.

**A refused build falls back, it does not fail.** Before this change a correlated block ran from a
streaming scan and materialised nothing, so a statement under the served budget that ran slowly and
succeeded would start failing with `bytes` if the build's refusal propagated. The build catches that
one error (`budget::exceeded_kind(&error) == Some(Exceeded::Bytes)`), drops the partial table,
clears `automatic`, and runs the chain as before. Every other error propagates. The join's build
side keeps its present behaviour, which is to propagate, because it materialised the inner side
before this change too.

No other ceiling. The join path has none beyond the budget, and the case where the inner table is
large is the case where ten thousand scans of it cost the most. A spill through
`TreeCatalog::spill` would need an on disk form of the hash table and a partitioned probe, and
nothing measured here needs it. `PRAGMA cache_size` is unaffected: the built table is heap memory
outside the pool, as the hash join's table is. `docs/pragmas.md` says `automatic_index` takes one
unnamed column; that stays true.

## 8. Tests

### 8.1 Tests that must keep passing

Run with `target/debug/inillucent-testrun --changed` from the worktree. The closure of
`inillucent-exec` and `inillucent-sql` selects these among others. Each one checks something this
change could break:

| Target | What it protects |
|---|---|
| `inillucent-compat::engine::new_engine_subquery` | every correlated answer: `a_correlated_subquery_is_answered_per_row`, `a_cheap_conjunct_rejects_a_row_before_its_correlated_block_is_answered`, `a_filtered_correlated_query_answers_what_sqlite_answers`, `a_folded_subquery_is_read_again_after_a_write`, the three valued `IN` rules |
| `inillucent-compat::engine::autoindex_reopen` | the join's automatic index answers after a tidy close |
| `inillucent-compat::engine::plan_cache` | the plan cache keys on the levers, so toggling the pragma re plans |
| `inillucent-compat::engine::write_speed` | `a_term_on_one_table_of_a_join_filters_before_the_join`, which is the `x.c = y.c + 1` shape at 2,000 rows, and the other speed limits |
| `inillucent-compat::differential::planner` | every asserted plan line, including the join's automatic index lines and the `INDEXED BY` cases |
| `inillucent-compat::differential::pragma` | `PRAGMA automatic_index` reads and writes |
| `inillucent-compat::differential::usage_corpus` | the 116 `EXPLAIN QUERY PLAN` statements and every correlated subquery and join in `holdout-subqueries.sql`, `bug-hunt-8-perf.sql` and the ORM scripts |
| `inillucent-compat::differential::semantics` | the plan cases at lines 652 and 1831, and every scripted join and correlated shape |
| `inillucent-compat::differential::ordering` | plan text for sorts, which shares `loop_lines` |
| `inillucent-exec` unit tests | `join.rs` holds the `Left`, `Semi` and `Anti` hash join cases at line 160; `autoindex.rs` and `physical/joins.rs` have their own |
| `inillucent-compat::tooling::policy` and `harness` | formatting, `deny(missing_docs)`, the layering contract: `inillucent-exec` depends on nothing new |
| `inillucent-compat::tooling::selection` | the new test file has a row |
| `inillucent-compat::tooling::documentation` | the docs pages in section 9 pass the style and facts checks |

### 8.2 Tests that prove the change

Each is red on `main` and green after. Where the file is new it needs a `mod` line in the tier's
`main.rs` and a `[[target]]` row in `tests/selection.toml` in the form of the `autoindex_reopen`
row: `package = "inillucent-compat"`, `kind = "test"`, `name = "engine"`, `module = "<file>"`,
`tier = "engine"`, `covers = ["inillucent-exec", "inillucent-sql"]`.

**Part A**

1. **Speed.** New file `crates/inillucent-compat/tests/engine/automatic_index_speed.rs`, in the
   pattern of `write_speed.rs`. Build `a` and `b` as in section 4.1 at 10,000 rows each. Time Q5 and
   Q8 together and assert the answers (10000 and 10000) and a limit of about ten times the after
   measurement. Red today: 1.2 s and 2.2 s. Also run Q5 with `PRAGMA automatic_index = OFF` at
   2,000 by 2,000 rows and assert only the answer (571), so the fallback path runs every time.
2. **Answers under the built table.** In `new_engine_subquery.rs`, one test that runs each of these
   with the pragma on and off and asserts the two answers agree and equal literal values: a scalar
   `count(*)` block where some outer keys match nothing (`0` for those rows); a scalar `max(x)` block
   where some match nothing (`NULL`); an `EXISTS` with `a.k` `NULL` on some rows (false); a block
   with a build filter `b.active = 1`; a block with a residual that reads a second feed,
   `b.id > a.id`, so the key captures part of the predicate and the filter the rest; a composite key
   `b.k = a.k AND b.g = a.g`; a key with an affinity, `INT` against a `TEXT` column storing `'7'`; a
   `NOCASE` column, which must decline and still answer; a block over the table an `UPDATE` is
   changing, which must decline. The answers already match today, so this test is red only through
   the plan assertions in item 3 that it shares a fixture with; its point is that the fast path
   cannot land without each shape running against it.
3. **Plan text.** In `differential/planner.rs`, `assert_plans` cases for Q5, Q7, Q8 and S2 with the
   lines section 5.7 gives each, and the same four after `PRAGMA automatic_index = OFF`. Red today
   on all four.
4. **Budget fallback.** In the new speed file: arm a budget with
   `inillucent_base::budget::arm(Limits::unbounded().with_bytes(Some(small)), cancel)` around Q8
   at a size whose table exceeds `small`, and assert the statement succeeds with the right answer.
   It is the test that keeps goal 5 true.
5. **Corpus.** New file `compat/corpus/usage/automatic-index.sql` with the section 4.1 schema at a
   few hundred rows, Q1 to Q14 as answers, and `EXPLAIN QUERY PLAN` for Q1, Q3, Q5, Q7, Q8, Q9, Q10
   and Q11, each `EXPLAIN` in its own case so that a statement whose text is a documented deviation
   can be listed alone in `known.toml`. Red today on the Q5, Q8 and Q10 plans.
6. **A write during the statement.** In `new_engine_subquery.rs`: `a` and `b` as in section 4.4's
   trigger case, a trigger `AFTER UPDATE ON a` that inserts `new.k` into `b`, then `UPDATE a SET pad
   = (SELECT count(*) FROM b WHERE b.k = a.k)`, asserting `pad` is 3 for ids 1 to 10, 4 for 11 to
   20 and 5 for 21 to 30, which is what the pinned shell answered. It passes today and it is the
   test that fails the moment condition 4 of section 5.2 is dropped or loosened. A second arm with
   the trigger removed asserts 3 for every row and, through the planner's plan text, that the built
   table was used.

**Part B**

7. **Speed.** In the same speed file, time Q9, Q10 and Q11 at 10,000 rows, assert the answers (5000,
   9999, 4999) and a limit of about ten times the after measurement. Red today: 2.0 to 2.8 s.
8. **Outer joins with a mixed condition.** In `new_engine_subquery.rs` or a new
   `automatic_index_answers.rs`: `LEFT JOIN b ON a.k = b.k AND b.active = 1` where some keys have
   only inactive partners (those rows null extend); `LEFT JOIN ... AND a.id < b.id WHERE b.id IS
   NULL`; `RIGHT JOIN` and `FULL JOIN` with the same conditions, asserting unmatched build rows; each
   with the pragma on and off and the answers equal. Red today only if the hash join is reached
   without `with_condition`, which is exactly the mistake these catch.
9. **Expression keys.** `a.k = b.k + 1`, `a.k = abs(b.k)`, `lower(a.s) = lower(b.s)`, each with the
   pragma on and off and equal answers, plus the plan line from section 6.3.
10. **Unit tests in `autoindex.rs`.** `equi_keys` on `k = k AND id < id` returns one key and a
    `rest`; on `k = k + 1` returns a key whose build side is the rebased arithmetic; on `id < id`
    alone returns `None`; on a `NOCASE` equality returns `None`.

**Part C**

11. **Plan text.** In `differential/planner.rs`: Q8 prints the `CORRELATED SCALAR SUBQUERY 1` node
    (today nothing); Q6 prints no `CREATE BLOOM FILTER` line under a correlated `LIST SUBQUERY` and
    Q13 still prints one under an uncorrelated one. Both lines copied from section 4.3.

### 8.3 Measured after

Repeat section 4.1 after the change and add the after column to section 4.2 in this document, with
the date and commit. The release notes line comes from that table.

## 9. Files the change touches

| File | Part | Change |
|---|---|---|
| `crates/inillucent-exec/src/correlate.rs` | A | `AutomaticKey`, the decision in `prepare_blocks`, `built` on `Correlation`, the build and probe in `run`, clearing in `Correlated::reset` and `finish`; the module comment gains a section on the built table |
| `crates/inillucent-exec/src/autoindex.rs` | A, B | `HashTable`, `encode_row_key` and the insert moved here as `pub(crate)`; `block_keys` splitting a block's residual into key equalities, build filter and rest; `equi_keys` returning `rest`; general `collect_columns` and `rebased`; the module comment's "why a hash table" section gains the block case and loses the sentence about the plain case |
| `crates/inillucent-exec/src/join/hash.rs` | A, B | uses the shared `HashTable`; `with_condition` and the per pair test in `push` |
| `crates/inillucent-exec/src/physical/joins.rs` | B | `build_materialised_join` and `has_equi_key` pass `rest` through |
| `crates/inillucent-exec/src/compiled.rs` | A | `run_over_rows` and `any_row_over_rows` |
| `crates/inillucent-exec/src/dml/answers.rs`, `crates/inillucent-exec/src/dml/update.rs`, `crates/inillucent-exec/src/insert_plan.rs` | A | pass whether the target has triggers or referencing foreign key actions to `correlations_in_every` and `correlations_in` |
| `crates/inillucent-sql/src/plan/describe.rs` | A, B | position 0 eligibility for a block's table read by an enclosing query's column; the `EXISTS` loop form of section 6; `<expr>` keys; `PARTIAL` |
| `crates/inillucent-sql/src/plan/tree.rs` | C | the subquery inside an aggregate argument; no `CREATE BLOOM FILTER` under a correlated `LIST SUBQUERY` |
| `crates/inillucent-compat/tests/engine/automatic_index_speed.rs` | A, B | new: items 1, 4, 7 |
| `crates/inillucent-compat/tests/engine/main.rs` | | `mod automatic_index_speed;` |
| `crates/inillucent-compat/tests/engine/new_engine_subquery.rs` | A, B | items 2, 6, 8, 9 |
| `crates/inillucent-compat/tests/differential/planner.rs` | A, B, C | items 3, 9, 11 |
| `compat/corpus/usage/automatic-index.sql`, `compat/corpus/usage/known.toml` | A, B | item 5 |
| `tests/selection.toml`, `tests/timings.toml` | | the new target's row; timings are written by `--record` |
| `docs/sql.md` | A, B, C | the "Plans" row: the `EXISTS` loop form and the automatic index line under a correlated subquery node; the plan differences row: the join order for Q9 and Q10 and the `<expr>` key; the sentence "does not turn `EXISTS` into a join" becomes a sentence about printing the same loop form without the rewrite |
| `docs/pragmas.md` | A | the `automatic_index` row says it covers a correlated subquery's table as well as a join's inner side |
| `docs/performance.md` | A | the correlated workloads paragraph, if the after numbers move the published figures |
| `docs/relational-architecture.md` | A, B | the executor's correlated subquery section: planned once, run per row, and read once when the block's table has no index for the key; the hash join section: a condition beside the key |
| `CHANGELOG.md` | A, B, C | Unreleased lines with the before and after numbers from section 4 |
| `agent-skills/inillucent-troubleshoot/SKILL.md` | A | the slow query entry, if it tells a reader to add an index for exactly this shape; then `node tools/sync-skills.mjs` |
| `tasks/task-2217-bug-hunt-8.md` | | section 4: the row "No automatic index for a join or correlated subquery" says what this change did and names this document |
| `src/data/documentation.ts` in `sites/inillucent` of `black-rainbow-labs-sites` | | the chapter that mirrors the `docs/sql.md` plans row |

Nothing in `inillucent-engine`, `inillucent-catalog` or `inillucent-tree` changes. No new
dependency, no layering row.

## 10. Considered and not chosen

**Rewriting `EXISTS` into a semi join of the outer query, as SQLite does.** `JoinKind::Semi` and
`Anti` exist in `join.rs` and both join operators implement them, so the executor half is there.
The planner half is not: the block's table would have to become a `FROM` term of the outer query
with a join kind the binder and the planner do not have, its correlation would have to become an
`ON`, and every place that counts loops, numbers subqueries or describes a plan would see one more
term. It answers `EXISTS` and not a scalar block in a result column, which is Q8 and every `SET x =
(SELECT ...)`, so the per row mechanism would stay beside it. The built table reaches the same speed
for Q5 through the mechanism that already runs every block, and prints SQLite's loop form for the
shapes SQLite rewrites. If the planner ever gains a semi join for its own reasons, the built table's
`existence_only` path is what it replaces.

**A transient b-tree in the pool, as SQLite builds.** SQLite builds a tree because its one join
operator is a nested loop that needs something to seek in. This engine has a hash table two join
paths already use, keyed on the byte encoding the trees use. A tree would cost a pool allocation,
page formatting and a log exemption for pages that must never be written, for a lookup that is
slower than a hash probe. `autoindex.rs`'s module comment makes the same case for the join.

**Reordering the join so the plain column side is inside, for `a.k = b.k + 1`.** It would make the
Q10 plan text equal to SQLite's. It is a change to how the planner orders inner joins, which has its
own tests and its own cost reasoning in `planner.rs`, and the hash join on the expression gives the
same speed. Listed as a plan text difference instead.

**Waiting for the second outer row before deciding.** It would save the hash inserts for a
statement with one outer row, at the price of one scan of the inner table thrown away when the
second row arrives, and a plan text that says the index exists before the engine has decided to
build it. SQLite builds before the first row. So does this.

**Keeping a built table across executions of a prepared statement.** The table's rows can change
between executions and nothing versions them. The chain is rebuilt under the same rule, and
`a_folded_subquery_is_read_again_after_a_write` is the test that says a block sees writes.

**A byte ceiling of its own, such as the pool's size.** The join path has none, the request budget
is the one place memory is counted, and a second limit here would be a second place to get wrong.
When a ceiling is wanted it is the budget's `with_bytes`.

**Printing `SCAN b` under the node for Q7 and Q8 to match SQLite while running the built table.**
It would make the corpus grade equal and the plan text false. The describer's contract is that the
line says what runs, and `docs/sql.md`'s plan differences row already describes the tree as "the
loops this planner chose". A true line that differs is listed; a false line that matches is a bug
report waiting to be filed.
