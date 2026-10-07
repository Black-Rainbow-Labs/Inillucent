-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/expressions/printf-integer-edge
-- printf %u reads the 64 bits as unsigned
SELECT printf('%d', 9223372036854775807), printf('%d', -9223372036854775808), printf('%u', -1), printf('%x', -1), printf('%lld', 12), printf('%,d', 1234567), printf('%!.20g', 0.1);

-- case: bug-hunt/expressions/printf_star_width_and_misc
-- printf with an empty format is NULL
SELECT printf('%*d|', 6, 42), printf('%-*d|', 6, 42), printf('%.*f', 2, 3.14159), printf('%*.*f|', 8, 2, 3.14159), printf('%*d|', -6, 42), printf('%.*s', 2, 'abcdef'), printf('%.*s', -1, 'abcdef'), printf('hello'), printf('%d%%', 50), printf(''), printf(NULL), printf('%d %d', 1), printf('%s', ''), printf('%0*d', 5, 7);
SELECT format('%d', 5), format('%s-%s', 'a', 'b'), printf('%,d', 1234567), printf('%,.2f', 1234567.891), printf('%!d', 5), printf('%!s', 'x');

-- case: bug-hunt/expressions/unary_minus_literal_edge
-- -0x8000000000000000 is refused as SQLite refuses it
SELECT -9223372036854775808, typeof(-9223372036854775808), -9223372036854775809, typeof(-9223372036854775809);
SELECT 0x7FFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0x8000000000000000, -0x8000000000000000, 0x0, 0Xff;
SELECT 9223372036854775807, 9223372036854775808, typeof(9223372036854775808), 1e3, typeof(1e3), .5, 5., 1_000;

-- case: bug-hunt/expressions/is_true_collate_real
-- x IS TRUE COLLATE NOCASE is still a truth test
SELECT 0.5 IS TRUE COLLATE NOCASE;
SELECT 0.5 IS TRUE COLLATE RTRIM;
SELECT 0.5 IS TRUE COLLATE BINARY;

-- case: bug-hunt/expressions/comparison-precedence
-- < <= > >= bind tighter than = IS IN LIKE BETWEEN
SELECT 2 = 1 < 3, 0 < 1 = 1, 1 <> 2 > 1, 'a' LIKE 'a' < 1, 3 IN (1) < 1, NULL IS 1 < 2, 5 > 4 > 3, 1 == 2 >= 2, 1 IS NOT 2 <= 1, 1 BETWEEN 0 AND 4<=3, 1 BETWEEN 0 AND 4 < 3, 1 < 2 BETWEEN 1 AND 1;

-- case: bug-hunt/expressions/between-upper-bound-is-a-relational-expression
-- x BETWEEN a AND b <= c takes b <= c as the upper bound, also in UPDATE
CREATE TABLE t1(x INT, y INT);
INSERT INTO t1(x) VALUES(1),(2),(3),(4),(5);
SELECT x, (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t1)) FROM t1;
UPDATE t1 SET x=x+100 WHERE (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t1));
SELECT x FROM t1 ORDER BY x;
CREATE TABLE t2(x INT, y INT);
INSERT INTO t2(x) VALUES(1),(2),(3),(4),(5);
UPDATE t2 SET x=x+100 WHERE x<3 OR (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t2));
SELECT x FROM t2 ORDER BY x;
CREATE TABLE t3(x INT, y INT);
INSERT INTO t3(x) VALUES(1),(2),(3),(4),(5);
UPDATE t3 SET x=x+100, y=1 WHERE x<3 OR (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t3));
SELECT x FROM t3 ORDER BY x;

-- case: bug-hunt/expressions/group_by_constant
-- an aggregate named by a GROUP BY ordinal is refused
CREATE TABLE t(a, b);
SELECT count(*), sum(a), total(a), max(a), min(a), avg(a), group_concat(a) FROM t;
SELECT count(*) FROM t GROUP BY 1;
SELECT count(*) FROM t GROUP BY a;
INSERT INTO t VALUES(1,2),(3,4);
SELECT count(*) FROM t GROUP BY 1;
SELECT 5 FROM t GROUP BY 5;

-- case: bug-hunt/expressions/group-by-alias-of-an-aggregate
-- an aggregate named by a GROUP BY alias is refused
CREATE TABLE t(a);
SELECT count(*) AS c FROM t GROUP BY c;
SELECT a AS c FROM t GROUP BY c;
SELECT count(*) FROM t GROUP BY a+1;

-- case: bug-hunt/expressions/ignore-check-constraints
-- PRAGMA ignore_check_constraints turns CHECK off
CREATE TABLE t(a CHECK(a > 0), b);
INSERT INTO t VALUES(-1, 1);
PRAGMA ignore_check_constraints = 1;
INSERT INTO t VALUES(-2, 2);
UPDATE t SET a = -3 WHERE b = 2;
PRAGMA ignore_check_constraints = 0;
INSERT INTO t VALUES(-4, 4);
SELECT * FROM t;

-- case: bug-hunt/expressions/pragma-schema-version-set-is-silent
-- PRAGMA schema_version = N answers no row
CREATE TABLE t(a);
PRAGMA schema_version = 50;
PRAGMA user_version = 3;
PRAGMA user_version;

-- case: bug-hunt/expressions/like_escape_is_a_wildcard
-- an ESCAPE character that is also a wildcard is only the escape
CREATE TABLE t1(id INTEGER PRIMARY KEY, x TEXT);
INSERT INTO t1 VALUES (1,'abcde'),(2,'abc_'),(3,'abc__'),(4,'abc%'),(5,'abc%%');
SELECT id FROM t1 WHERE x LIKE 'abc%%' ESCAPE '%';
SELECT id FROM t1 WHERE x LIKE 'abc__' ESCAPE '_';
SELECT id FROM t1 WHERE x LIKE 'abc%_' ESCAPE '%';

-- case: bug-hunt/expressions/compound_derived_column_collation
-- a filter pushed into the arms of a compound compares with the derived column's collation
CREATE TABLE t1(a, b COLLATE nocase);
CREATE TABLE t2(c, d);
INSERT INTO t2 VALUES(1, 'bbb');
SELECT * FROM (SELECT a, b FROM t1 UNION ALL SELECT c, d FROM t2) WHERE b='BbB';
SELECT * FROM (SELECT c, d FROM t2 UNION ALL SELECT a, b FROM t1) WHERE d='BbB';

-- case: bug-hunt/expressions/create_table_as_over_a_compound_view
-- CREATE TABLE AS writes the affinity every arm of a compound agrees on
CREATE TABLE map_integer (id INT, name);
INSERT INTO map_integer VALUES(1,'a');
CREATE TABLE map_text (id TEXT, name);
INSERT INTO map_text VALUES('4','e');
CREATE TABLE data (id TEXT, name);
INSERT INTO data VALUES(1,'abc'),('4','xyz');
CREATE VIEW idmap AS SELECT * FROM map_integer UNION SELECT * FROM map_text;
CREATE TABLE mzed AS SELECT * FROM idmap;
SELECT sql FROM sqlite_schema WHERE name = 'mzed';
SELECT * FROM data JOIN mzed USING(id) ORDER BY 1;

-- case: bug-hunt/expressions/in_over_a_cte_column
-- IN over a common table expression's column applies the left column's affinity
CREATE TABLE t3(a INTEGER, b TEXT);
INSERT INTO t3 VALUES(123, 123);
WITH s AS (VALUES(123), (456)) SELECT * FROM t3 WHERE b IN s;
WITH s(v) AS (SELECT 123) SELECT * FROM t3 WHERE b IN (SELECT v FROM s);

-- case: bug-hunt/expressions/using_column_of_full_and_right_joins
-- the merged USING column of a FULL or RIGHT JOIN keeps its affinity
CREATE TABLE t1(x REAL);
CREATE TABLE t2(x REAL);
INSERT INTO t1 VALUES(2.5);
INSERT INTO t2 VALUES(2.5);
SELECT x FROM t1 FULL JOIN t2 USING(x) WHERE x='2.5';
SELECT x FROM t1 RIGHT JOIN t2 USING(x) WHERE x='2.5';
SELECT coalesce(t1.x, t2.x) FROM t1 FULL JOIN t2 USING(x) WHERE coalesce(t1.x, t2.x)='2.5';

-- case: bug-hunt/expressions/row_value_subquery_collate
-- a row value equality in WHERE compares each column under the scalar rule
CREATE TABLE t33(a, b, c);
INSERT INTO t33 VALUES ('abc', 1, 'i'), ('ABC', 1, 'ii');
SELECT c FROM t33 WHERE (a, b) = (SELECT 'abc' COLLATE nocase, 1);
CREATE INDEX t33ab ON t33(a, b);
SELECT c FROM t33 WHERE (a, b) = (SELECT 'abc' COLLATE nocase, 1);
SELECT c FROM t33 WHERE a = (SELECT 'abc' COLLATE nocase);
CREATE TABLE t(b COLLATE nocase);
INSERT INTO t VALUES('ABC');
SELECT (a, 1) = (SELECT b, 1 FROM t) FROM t33;

-- case: bug-hunt/expressions/limit_and_aliased_aggregate_messages
-- LIMIT sees no columns and an aggregate cannot read an aggregate alias, with SQLite's messages
CREATE TABLE test1(f1, f2);
INSERT INTO test1 VALUES(1, 2), (3, 4);
SELECT min(f1) AS m FROM test1 GROUP BY f1 HAVING max(m+5)<10;
SELECT min(f1) AS m FROM test1 GROUP BY f1 HAVING m<10;
SELECT f1 AS m FROM test1 GROUP BY f1 HAVING max(m+5)<10;
SELECT * FROM test1 LIMIT f1;
SELECT * FROM test1 LIMIT (SELECT count(*) FROM test1);
SELECT * FROM test1 LIMIT 1 OFFSET f2;
SELECT (SELECT f2 FROM test1 LIMIT t.f1) FROM test1 t;
CREATE TABLE t2(b);
INSERT INTO t2 VALUES(5), (7);
SELECT (SELECT max(b) LIMIT (SELECT total((SELECT f1 FROM test1 LIMIT 1)))) FROM t2;
SELECT (SELECT max(b) LIMIT 1 OFFSET (SELECT count(*) FROM test1)) FROM t2;
