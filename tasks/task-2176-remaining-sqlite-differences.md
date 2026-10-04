# The SQLite differences left after the usage corpus work

task-2174 brought `compat/corpus/usage` to 1,729 of 1,746 cases matching the pinned sqlite3
3.53.4. Fourteen of the other seventeen are deliberate. This ticket covered the remaining three,
which were all `EXPLAIN QUERY PLAN` text, and seven smaller differences found while that work was
done. Every item has a case in `compat/corpus/usage/remaining-differences.sql`, and the three
`defect` rows are gone from `known.toml`. The corpus now holds 14 rows, all `deviation`.

## 1. The plan tree, the flattener, and when a derived table is run again (items 1 and 7)

**Plan tree.** `crates/inillucent-sql/src/plan/tree.rs` renders a plan as SQLite's tree: each line
has a depth, and `query_plan_rows` in `crates/inillucent-engine/src/engine/explain.rs` turns the
depths into parent ids for the shell to draw. The nodes are SQLite's:

| Node | When |
|---|---|
| `CO-ROUTINE name` | a derived table SQLite's `fromClauseTermCanBeCoroutine` accepts |
| `MATERIALIZE name` | any other derived table, a `MATERIALIZED` CTE, a CTE used twice |
| `SETUP`, `RECURSIVE STEP` | the arms of a recursive CTE |
| `COMPOUND QUERY`, `LEFT-MOST SUBQUERY`, `UNION ALL` | a `UNION ALL` with no `ORDER BY` |
| `MERGE (op)`, `LEFT`, `RIGHT` | every other compound; each side planned with the merge ordering |
| `SCALAR SUBQUERY n`, `LIST SUBQUERY n`, `CORRELATED ...` | expression subqueries, after the loops |
| `CO-ROUTINE (subquery-n)` | a statement with a window function |

`n` is SQLite's select number. A parser core is added after everything inside it, so
`BoundSelect::serial` is the core's position plus one. `DerivedNote` on `BoundSource` records
whether a derived table is a view, a CTE (with its `MATERIALIZED` hint and use count) or written
with no alias.

**Flattener.** `crates/inillucent-sql/src/plan/flatten.rs` runs at the start of planning. It
follows `flattenSubquery`'s numbered restrictions, and first turns a `LEFT JOIN` whose right side
the `WHERE` cannot leave null into a join (`sqlite3ExprImpliesNonNullRow`). Three restrictions are
narrower than SQLite's, because this engine has no `TK_IF_NULL_ROW`: the right side of a `LEFT
JOIN` is flattened only when every result column is a column of its one table; a derived table
whose result columns hold a correlated subquery is kept; a compound derived table is kept, and
`tree.rs` describes a `UNION ALL` one the way SQLite flattens it. The view an `INSTEAD OF` trigger
writes through is never flattened (`DerivedNote::pinned`).

**Item 7.** SQLite runs a co-routine again each time the query holding it runs, and fills any other
derived table once. `unshare_coroutines` takes the binder's run once mark off a derived table SQLite
runs as a co-routine, unless SQLite reads it through an automatic index, which is also built once.
The push down skips a `MATERIALIZED` CTE and a CTE used twice, as SQLite's does.

## 2. Small differences

| Item | Cause | Fix |
|---|---|---|
| 2, maths last digit | the pinned Windows shell links the static C runtime, whose `sinh`, `cosh`, `tanh`, `asin` and `acos` are AMD libm; `ucrtbase.dll` differs | `crates/inillucent-scalar/src/mathfn/amd.rs`, AMD's reference code ported, Windows only. 640,000 sampled arguments match; `sinh` switches to the exponential at 36.1236018, measured |
| 3, window `sum` after an overflow | SQLite adds entering rows and removes leaving rows on one accumulator | `Accumulator::pull` follows `sumInverse`; `sliding_sums` in `window.rs` steps it |
| 4, `reverse_unordered_selects` | SQLite reverses every loop | the joined rows are reversed when every term is a table or an index; subqueries get the outer levers |
| 5, dates | SQLite's `DateTime` fields and error flag decide the edge cases | `crates/inillucent-scalar/src/datetime/state.rs` ports `DateTime` and `parseModifier` |
| 6, module constraints | SQLite's `generate_series` consumes and converts its constraints and promises `omit` | the series module follows `seriesBestIndex` and `seriesFilter`; an `IN` list on a module column is tested by the scan only when the module did not promise `omit`; a constraint that reads another table is used only when the plan costs less |
| 8, `AND`/`OR` constant operand | already matched | case added |
| 9, `STRICT` and `NOT NULL` order | SQLite type checks ordinary and virtual columns while computing generated columns | `generated_types_are_met` runs before `NOT NULL` |
| 10, malformed numeric literal | already matched | case added |

## 3. A wrong answer the flattener exposed

The deep join matrix failed 23 cases once derived tables were flattened, all of the form
`EXISTS (SELECT 1 FROM (SELECT s.k AS c1 ... FROM s RIGHT JOIN r ...) p WHERE p.c1 = u.k)`. The
cause was older than this ticket: `s RIGHT JOIN r ON r.a = s.a WHERE s.k = 1` over an empty `s`
returned every row of `r` in 2.1.1. The `WHERE` term became a rowid search on `s`, which the
`RIGHT JOIN` then null extended, and the consumed term was never tested again. Flattening moved the
correlated `p.c1 = u.k` into exactly that position.

Two changes fix it. `choose_path_without_where` in `plan/outer_paths.rs` offers a term before a
`RIGHT` or `FULL` join only the inner joins' `ON` terms. `simplify_left_joins` in `plan/flatten.rs`
now also follows SQLite for `RIGHT` and `FULL` joins: a term before one that the `WHERE` keeps non
null turns a later `RIGHT` join into an inner join and a `FULL` join into a `LEFT` one, which is how
SQLite still gets `SEARCH s USING INTEGER PRIMARY KEY`. It runs again after flattening. The case is
`joins/where-on-the-null-extended-side-of-a-right-or-full-join`.

The binder ANDs a table valued function's arguments into the `WHERE` as equalities on hidden
columns. SQLite keeps them out of its `WHERE`, so both changes skip them (`is_table_argument`):
without that, `json_each('[7]') s RIGHT JOIN r ON 0` became an inner join, and the arguments were
left as filters that removed every null extended row.

The vtab matrix found a second older bug the same way. FTS5's `best_index` took a rowid equality as
soon as it saw one and returned, so `WHERE rowid = 1 AND f MATCH 'x'` never gave the module the
match, and the match tested outside the module failed every row. Flattening puts the outer
`p.c1 = u.k` first, which is the order that failed. The module now chooses a match before a rowid
lookup (`crates/inillucent-ext/src/vtab/fts5/mod.rs`); the case is
`fts5/match-written-after-a-rowid-equality`. `WHERE f = 'text'`, which SQLite reads as a match, is
task-2111 Bug 28.

## 4. What still differs

Recorded in the bug tickets, not fixed here: rows before a mid statement error are lost with the
rest of their batch (task-2110 Bug 28); a table valued function joined to a table is the outer
loop; `CROSS JOIN generate_series` with a text constraint; `pragma_table_info WHERE arg IN (...)`;
the missing start error raised when the scan runs; and plans where the two planners choose
different loops (task-2111 Bugs 22 to 26); a `WHERE` equality on a table valued function's hidden
column read as an argument (Bug 27); FTS5's `f = 'text'` (Bug 28).
