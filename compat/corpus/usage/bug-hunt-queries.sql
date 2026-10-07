-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/queries/nested-correlated-subquery-reads-two-levels
-- a subquery two deep that reads its parent and the outermost query
CREATE TABLE v0 ( v1 INTEGER);
INSERT INTO v0 VALUES ( 10 ),  ( 7 );
CREATE TABLE v4 ( v5 INTEGER PRIMARY KEY );
INSERT INTO v4 VALUES ( 10 );
SELECT v1, (SELECT v5 FROM v4 WHERE EXISTS ( SELECT v5 WHERE v5 = v1 )) FROM v0;
SELECT v1, (SELECT v5 FROM v4 WHERE EXISTS ( SELECT 1 WHERE v5 = v1 )) FROM v0;
SELECT v1, (SELECT v5 FROM v4 WHERE (SELECT v5 = v1)) FROM v0;
SELECT v1, (SELECT v5 FROM v4 WHERE v5 = v1) FROM v0;
SELECT v1, (SELECT count(*) FROM v4 WHERE EXISTS ( SELECT 1 WHERE v5 = v1 )) FROM v0;
SELECT v1 FROM v0 WHERE EXISTS (SELECT 1 FROM v4 WHERE EXISTS (SELECT 1 WHERE v5 = v1));
CREATE TABLE w(x);
INSERT INTO w VALUES(10),(7);
SELECT v1, (SELECT x FROM w WHERE EXISTS ( SELECT 1 WHERE x = v1 )) FROM v0;

-- case: bug-hunt/queries/exists_scalar_subquery_total
-- nested EXISTS inside a correlated scalar subquery
CREATE TABLE v0 ( v1 INTEGER);
INSERT INTO v0 VALUES ( 10 ),  ( 7 );
CREATE TABLE v4 ( v5 INTEGER PRIMARY KEY );
INSERT INTO v4 VALUES ( 10 );
SELECT cast ( (SELECT v5 FROM v4 WHERE EXISTS ( SELECT v5 WHERE v5 = v1 )) as BOOL) != 0 FROM v0 ;
SELECT TOTAL(cast ( (SELECT v5 FROM v4 WHERE EXISTS ( SELECT v5 WHERE v5 = v1 )) as BOOL) != 0) FROM v0 ;

-- case: bug-hunt/queries/update_where_subquery_min_of_updated_column
-- UPDATE whose WHERE holds BETWEEN over a comparison with a subquery
CREATE TABLE t1(x INT, y INT);
INSERT INTO t1(x) VALUES(1),(2),(3),(4),(5);
UPDATE t1 SET x=x+100, y=x<=(SELECT min(x) FROM t1) WHERE x<3 OR (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t1));
SELECT x FROM t1 WHERE x<100 ORDER BY x;

-- case: bug-hunt/queries/or_short_circuit_returns_operand
-- a derived table aliased subQuery beside an unnamed one
CREATE TABLE t0(c0);
INSERT INTO t0 VALUES (1);
SELECT * FROM (SELECT 0 AS col_0) LEFT JOIN (SELECT 1 AS col_1, (0 OR 2) AS col_2 FROM t0) as subQuery ON (subQuery.col_1 % subQuery.col_2);
SELECT 0 OR 2;

-- case: bug-hunt/queries/union-all-arms-keep-derived-table-order
-- UNION ALL over ordered derived tables keeps each arm in its order
CREATE TABLE t1(id INTEGER PRIMARY KEY, a INT);
INSERT INTO t1(a) VALUES (5),(3),(9),(1);
SELECT a FROM (SELECT a FROM t1 ORDER BY a DESC);
SELECT a FROM (SELECT a FROM t1 ORDER BY a DESC) UNION ALL SELECT 100;
SELECT * FROM (SELECT a FROM t1 WHERE a<5 ORDER BY a DESC) UNION ALL SELECT * FROM (SELECT a FROM t1 WHERE a>=5 ORDER BY a ASC);
SELECT * FROM (SELECT a FROM t1 WHERE a<5 ORDER BY a DESC) UNION ALL SELECT * FROM (SELECT a FROM t1 WHERE a>=5 ORDER BY a ASC) LIMIT 3;

-- case: bug-hunt/queries/union_all_of_ordered_subqueries_limit_offset
-- the same, with LIMIT and OFFSET
CREATE TABLE t1(id INTEGER PRIMARY KEY, a INT);
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<100) INSERT INTO t1(a) SELECT 100-x FROM c;
SELECT * FROM (SELECT * FROM t1 WHERE a<50 ORDER BY a DESC) UNION ALL SELECT * FROM (SELECT * FROM t1 WHERE a>=50 ORDER BY a ASC) LIMIT 5 OFFSET 47;

-- case: bug-hunt/queries/limit_offset_union_all_view_two_order_by_arms
-- the same, through a view
CREATE TABLE employees (id INTEGER PRIMARY KEY, salary INTEGER);
INSERT INTO employees VALUES (11, 70), (12, 78), (21, 84), (22, 90), (23, 104), (24, 104), (25, 120), (31, 96), (32, 96), (33, 100);
CREATE VIEW v AS SELECT * FROM (SELECT * FROM employees WHERE salary < 100 ORDER BY salary DESC) UNION ALL SELECT * FROM (SELECT * FROM employees WHERE salary >= 100 ORDER BY salary ASC);
SELECT * FROM v LIMIT 5 OFFSET 0;

-- case: bug-hunt/queries/filter-on-grouping-column-of-aggregate-view
-- a WHERE on a grouping column of an aggregate derived table
CREATE TABLE t(id INTEGER PRIMARY KEY, g, h TEXT COLLATE NOCASE, v INT);
CREATE INDEX tg ON t(g);
INSERT INTO t VALUES(1,1,'a',10),(2,1,'A',20),(3,2,'b',30),(4,2,'B',40),(5,3,NULL,50),(6,NULL,'c',60),(7,'1','a',70);
SELECT g, s FROM (SELECT g, sum(v) s FROM t GROUP BY g) WHERE g = 1;
SELECT g, s FROM (SELECT g, sum(v) s FROM t GROUP BY g) WHERE g > 1 ORDER BY g;
SELECT g, s FROM (SELECT g, sum(v) s FROM t GROUP BY g HAVING sum(v) > 40) WHERE g >= 1 ORDER BY g;
SELECT h, n FROM (SELECT h, count(*) n FROM t GROUP BY h) WHERE h = 'A';
SELECT h, n FROM (SELECT h COLLATE BINARY AS h, count(*) n FROM t GROUP BY h COLLATE BINARY) WHERE h = 'A';
SELECT x, s FROM (SELECT g+1 AS x, sum(v) s FROM t GROUP BY g+1) WHERE x = 2;
SELECT x, s FROM (SELECT g AS x, sum(v) s FROM t GROUP BY g) WHERE s > 50 ORDER BY x;
SELECT x, s FROM (SELECT g AS x, sum(v) s FROM t GROUP BY g) WHERE x IS NULL;
SELECT x, n FROM (SELECT g AS x, count(*) OVER () n FROM t GROUP BY g) WHERE x = 1;
SELECT x FROM (SELECT DISTINCT g AS x FROM t GROUP BY g, h) WHERE x = 2;
SELECT cnt FROM (SELECT count(*) cnt FROM t) WHERE cnt > 0;
WITH q(i) AS (VALUES(1),(2),(3)) SELECT i, (SELECT s FROM (SELECT g, sum(v) s FROM t GROUP BY g) WHERE g = q.i) FROM q;

-- case: bug-hunt/queries/recursive-cte-step-restrictions
-- aggregate, GROUP BY and window in a recursive step are refused rather than run forever
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT count(*) FROM r) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT max(n)+1 FROM r WHERE n < 5) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r GROUP BY n HAVING n < 3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT DISTINCT n+1 FROM r WHERE n<3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT row_number() OVER () FROM r WHERE n < 3) SELECT * FROM r LIMIT 5;
WITH RECURSIVE r(n) AS (SELECT n FROM r UNION ALL SELECT 1) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r LEFT JOIN (SELECT 1 AS x) ON 1 WHERE n<3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM (SELECT 1) LEFT JOIN r ON 1 WHERE n<3) SELECT * FROM r;

-- case: bug-hunt/queries/circular-view
-- a view that reads itself is refused by name
CREATE VIEW v AS SELECT 1 AS a;
DROP VIEW v;
CREATE VIEW v AS SELECT * FROM v;
SELECT * FROM v;
CREATE VIEW a1 AS SELECT * FROM b1;
CREATE VIEW b1 AS SELECT * FROM a1;
SELECT * FROM a1;

-- case: bug-hunt/queries/recursive_trigger_replace_duplicate_unique
-- recursive triggers stop at the depth limit instead of overflowing the stack
PRAGMA recursive_triggers = true;
CREATE TABLE t0(c0 UNIQUE);
CREATE TRIGGER tr0 AFTER DELETE ON t0 BEGIN INSERT INTO t0 VALUES(0); END;
INSERT OR REPLACE INTO t0(c0) VALUES(0), (0);
REINDEX;
SELECT * FROM t0;

-- case: bug-hunt/queries/recursive_cte_width_mismatch
-- a recursive step with a different number of columns is refused instead of running forever
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1, 2 FROM r WHERE n<3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3) SELECT * FROM r;

-- case: bug-hunt/queries/skip_scan_asc_nulls_last
-- a skip scan yields NULLs first, so ASC NULLS LAST still sorts
CREATE TABLE t0(c0 TEXT);
CREATE INDEX i0 ON t0(c0);
INSERT INTO t0(c0) VALUES (NULL),(NULL),(NULL),(NULL),(NULL),(NULL),
                          (NULL),(NULL),(NULL),(NULL),(NULL),(NULL);
INSERT INTO t0(c0) VALUES('A');
SELECT DISTINCT c0 FROM t0 INDEXED BY i0 ORDER BY c0 ASC NULLS LAST;

-- case: bug-hunt/queries/right_join_then_left_join_partial_index
-- an ON term of a join written after a RIGHT JOIN is tested after the null extension
CREATE TABLE t1(a INT, b INT);
INSERT INTO t1(a) VALUES(2);
CREATE TABLE t2(c INT);
CREATE INDEX i0 ON t2(c) WHERE c=3;
CREATE TABLE t3(d INT);
INSERT INTO t3 VALUES(1);
SELECT * FROM t2 RIGHT JOIN t3 ON d<>0 LEFT JOIN t1 ON c=3 WHERE t1.a<>0;

-- case: bug-hunt/queries/join_keyword_orders
-- join words may come in any order, as SQLite reads them
CREATE TABLE t1(a, b);
CREATE TABLE t2(a, c);
INSERT INTO t1 VALUES(1, 'x'), (2, 'y');
INSERT INTO t2 VALUES(1, 'p'), (3, 'q');
SELECT * FROM t1 OUTER LEFT NATURAL JOIN t2 ORDER BY 1;
SELECT * FROM t1 NATURAL OUTER LEFT JOIN t2 ORDER BY 1;
SELECT * FROM t1 LEFT RIGHT JOIN t2 USING(a) ORDER BY 1;
SELECT * FROM t1 INNER LEFT JOIN t2 USING(a);
SELECT * FROM t1 OUTER JOIN t2 USING(a);

-- case: bug-hunt/queries/nested_join_using_merged_column
-- a parenthesised join exposes its USING column once, to an outer USING and to a bare name
CREATE TABLE t1(a INT, b INT, c INT);
CREATE TABLE t2(a INT, b INT, d INT);
CREATE TABLE t3(a INT, b INT, e INT);
CREATE TABLE t4(a INT, b INT, f INT);
CREATE TABLE t5(a INT, b INT, g INT);
INSERT INTO t1 VALUES(11,21,31),(12,22,32),(15,25,35),(17,27,37);
INSERT INTO t2 VALUES(12,22,32),(13,23,33),(15,25,35),(18,28,38),(NULL,NULL,36);
INSERT INTO t4 VALUES(11,21,31),(13,23,33),(15,25,35),(19,29,39);
INSERT INTO t3 SELECT * FROM t1 UNION SELECT * FROM t2 UNION SELECT * FROM t4;
INSERT INTO t5 SELECT * FROM t3 WHERE a>=15;
SELECT a, c, d, e, f, g FROM t1
  INNER JOIN (t2 LEFT JOIN t3 USING(a)) USING(a)
  LEFT JOIN (t4 LEFT JOIN t5 USING(a)) USING(a)
  WHERE a<=18 ORDER BY 1 NULLS FIRST;
SELECT a, c, d, e, f, g FROM t1
  RIGHT JOIN (t2 LEFT JOIN t3 USING(a)) USING(a)
  FULL JOIN (t4 LEFT JOIN t5 USING(a)) USING(a)
  WHERE a<=18 ORDER BY 1 NULLS FIRST;
SELECT t2.a AS x2, t3.a AS x3, t4.a AS x4 FROM t1
  LEFT JOIN (t2 LEFT JOIN t3 USING(a)) USING(a)
  LEFT JOIN (t4 FULL JOIN t5 USING(a)) USING(a)
  ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST;

-- case: bug-hunt/queries/parenthesised_right_and_full_join_inside_an_outer_join
-- a parenthesised RIGHT or FULL join is not flattened into the outer join, which lost its ON
CREATE TABLE t1(a INT, c INT);
CREATE TABLE t4(a INT, f INT);
CREATE TABLE t5(a INT, g INT);
INSERT INTO t1 VALUES(11,31),(12,32),(15,35),(17,37);
INSERT INTO t4 VALUES(11,31),(13,33),(15,35),(19,39);
INSERT INTO t5 VALUES(15,35),(17,37),(18,38);
SELECT t1.a AS x1, t4.a AS x4, t5.a AS x5 FROM t1 LEFT JOIN (t4 RIGHT JOIN t5 ON t4.a = t5.a) ON t1.a = t5.a ORDER BY 1, 2, 3;
SELECT t1.a AS x1, t4.a AS x4, t5.a AS x5 FROM t1 LEFT JOIN (t4 FULL JOIN t5 ON t4.a = t5.a) ON t1.a = coalesce(t4.a, t5.a) ORDER BY 1, 2, 3;
SELECT t1.a AS x1, t4.a AS x4, t5.a AS x5 FROM t1 JOIN (t4 FULL JOIN t5 ON t4.a = t5.a) ON t1.a = t4.a ORDER BY 1, 2, 3;
SELECT t1.a AS x1, t4.a AS x4, t5.a AS x5 FROM t1 LEFT JOIN (t4 RIGHT JOIN t5 ON t4.a = t5.a) ON t1.a = t4.a ORDER BY 1, 2, 3;
SELECT t1.a, x.a4, x.a5 FROM t1 LEFT JOIN (SELECT t4.a AS a4, t5.a AS a5 FROM t4 RIGHT JOIN t5 ON t4.a = t5.a) AS x ON x.a5 = t1.a ORDER BY 1, 2, 3;
