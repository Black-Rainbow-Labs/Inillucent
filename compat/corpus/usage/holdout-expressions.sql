-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/expressions/42-order-by-a-real-literal-is-a-constant-expression-t
SELECT 1 ORDER BY 0.5;
SELECT 1 ORDER BY 1.0;
SELECT 1 ORDER BY 1;
SELECT 1 ORDER BY 2;
-- case: holdout/expressions/148-generate-series-with-negative-bounds
SELECT * FROM generate_series(-5, -1);
SELECT * FROM generate_series(-1, 0);
SELECT * FROM generate_series(-1, -1);
SELECT * FROM generate_series(5, 1);
SELECT * FROM generate_series(1, 10, 4);
SELECT * FROM generate_series(10, 1, -4);
SELECT count(*) FROM generate_series(1, 5, 0);
-- case: holdout/expressions/178-match-on-a-regular-table-column
CREATE TABLE t1(a);
INSERT INTO t1 VALUES(1),(2),(3);
SELECT * FROM t1 WHERE a MATCH 'pattern';
SELECT 1 MATCH 2;
-- case: holdout/expressions/344-many-rows-sorted-on-several-keys-with-limit
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 3000) SELECT i % 7 AS a, i % 11 AS b, i FROM n ORDER BY a, b DESC, i LIMIT 5 OFFSET 100;
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 3000) SELECT count(*), sum(a), sum(b) FROM (SELECT i % 7 AS a, i % 11 AS b FROM n ORDER BY a, b DESC, i);
-- case: holdout/expressions/366-a-runtime-error-inside-one-statement-does-not-stop
SELECT abs(-9223372036854775808); SELECT 'after';
SELECT 1/0, 'x'; SELECT 'after2';
-- case: holdout/expressions/425-order-by-an-expression-over-a-name-that-is-both-an
CREATE TABLE t(a INT, b INT);
CREATE TABLE d(k INT PRIMARY KEY, v INT);
INSERT INTO t VALUES(1,30),(2,20),(3,10);
SELECT b AS a FROM t ORDER BY abs(a);
INSERT OR REPLACE INTO d SELECT 1, b AS a FROM t ORDER BY abs(a);
SELECT k||'='||v FROM d;
SELECT b AS a FROM t ORDER BY a;
SELECT b AS a FROM t ORDER BY a+0;
SELECT b AS a FROM t ORDER BY (a);
SELECT b AS a FROM t ORDER BY a COLLATE BINARY;
-- case: holdout/expressions/466-strict-table-columns-reject-nan-and-inf-in-non-rea
CREATE TABLE t(i REAL, tt REAL, b REAL) STRICT;
INSERT INTO t VALUES(1.25, 1.25, 1.25);
PRAGMA integrity_check;
CREATE TABLE s(i INT, tt TEXT, b BLOB, a ANY) STRICT;
INSERT INTO s VALUES(1, 'a', x'01', 1e999);
INSERT INTO s VALUES(1.5, 'a', x'01', 1);
INSERT INTO s VALUES('1', 5, 'x', 1);
SELECT typeof(i), typeof(tt), typeof(b), quote(a) FROM s;
-- case: holdout/expressions/467-and-or-in-the-select-list-skip-the-operand-sqlite-
SELECT 1 OR json_extract('bare','$.a');
SELECT 0 AND json_extract('bare','$.a');
SELECT CASE WHEN (SELECT abs(-9223372036854775807 - 1)) AND 0 THEN 'y' ELSE 'n' END;
SELECT 0 AND abs(-9223372036854775807 - 1);
SELECT 1 OR abs(-9223372036854775807 - 1);
SELECT NULL AND abs(-9223372036854775807 - 1);
SELECT coalesce(1, abs(-9223372036854775807 - 1));
SELECT CASE WHEN 1 THEN 1 ELSE abs(-9223372036854775807 - 1) END;
SELECT iif(1, 1, abs(-9223372036854775807 - 1));
-- case: holdout/expressions/490-filter-on-a-scalar-function
CREATE TABLE t(a INT);
SELECT abs(a) FILTER (WHERE a > 0) FROM t;
SELECT count(a) FILTER (WHERE a > 0) FROM t;
SELECT max(a, 1) FILTER (WHERE a > 0) FROM t;
SELECT min(a) FILTER (WHERE a > 0) FROM t;
-- case: holdout/expressions/560-a-line-comment-ends-at-the-end-of-the-line
--whatCREATE TABLE users (
    id INT PRIMARY KEY,
    first_name VARCHAR(50),
    age INT
);
SELECT 'after comment';
/* block */ SELECT 'b' /* x */ ;
-- c
SELECT 'c' -- trailing
;
-- case: holdout/expressions/570-limit-and-default-with-an-identifier
select * from sqlite_schema limit asdf;
select * from sqlite_schema limit `asdf`;
create table t(a default asdf);
insert into t default values;
select a, typeof(a) from t;
create table u(a default "asdf");
insert into u default values;
select a from u;
-- case: holdout/expressions/585-unterminated-block-comment-at-the-end-of-the-input
select 1; /* never closed
select 2;
select 'a' /* ;
-- case: holdout/expressions/621-select-0-and-out-of-range-parameter-numbers
SELECT ?0;
SELECT ?1;
SELECT ?32767;
SELECT ?32768;
SELECT ?99999999999;
-- case: holdout/expressions/623-calling-scalar-functions-with-the-wrong-number-of-
SELECT replace();
SELECT replace('a');
SELECT replace('a', 'b');
SELECT replace('a', 'b', 'c', 'd');
SELECT substr('a');
SELECT length();
SELECT length('a', 'b');
SELECT nosuchfunction(1);
SELECT abs(1, 2);
SELECT round();
SELECT round(1, 2, 3);
SELECT coalesce(1);
SELECT ifnull(1);
SELECT nullif(1);
SELECT iif(1, 2);
SELECT typeof();
SELECT min();
SELECT count(1, 2);
-- case: holdout/expressions/725-a-real-with-a-whole-number-value-stays-real-when-i
CREATE TABLE t(n INTEGER, r REAL);
INSERT INTO t VALUES(2,2.5),(4,1.25);
CREATE TABLE u AS SELECT n*r AS x FROM t;
SELECT typeof(x), quote(x), x, printf('%s', x), x || '' FROM u;
SELECT quote(5.0), quote(5.0e0), quote(1e20), quote(1e-5), quote(-0.0), quote(1.0e15), quote(123456789012345678.0), quote(0.1+0.2), quote(1.0e100), quote(1.5e300), quote(2e-300);
-- case: holdout/expressions/788-iif-with-two-arguments-and-with-more-than-three
SELECT iif(1, 'a');
SELECT iif(0, 'a');
SELECT iif(1, 'a', 'b'), iif(0, 'a', 'b'), iif(NULL, 'a', 'b');
SELECT iif(0, 'a', 0, 'b', 'c'), iif(0, 'a', 1, 'b', 'c'), iif(0, 'a', 0, 'b'), iif(0, 'a', 0, 'b', 0, 'c');
SELECT iif();
SELECT iif(1);
-- case: holdout/expressions/833-check-constraint-of-a-create-table-that-begins-wit
/* leading comment
   spanning lines */
-- another comment
CREATE TABLE issues (
  id INTEGER PRIMARY KEY, -- the id
  status TEXT NOT NULL,
  closed_at TEXT,
  CONSTRAINT status_closed CHECK ((status = 'closed' AND closed_at IS NOT NULL) OR (status <> 'closed' AND closed_at IS NULL))
);
INSERT INTO issues VALUES (1, 'closed', NULL);
INSERT INTO issues VALUES (2, 'open', 'x');
INSERT INTO issues VALUES (3, 'closed', 'x');
SELECT id FROM issues;
SELECT sql FROM sqlite_master WHERE name = 'issues';
