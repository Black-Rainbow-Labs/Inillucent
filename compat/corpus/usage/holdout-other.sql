-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/other/232-sqlite-stat1-contents-after-analyze
CREATE TABLE t1(id INTEGER PRIMARY KEY, a TEXT);
INSERT INTO t1 VALUES(1, 'x');
CREATE INDEX i1 ON t1(a);
ANALYZE t1;
SELECT tbl, idx, stat FROM sqlite_stat1 ORDER BY tbl, idx;
CREATE TABLE t2(a TEXT, b INTEGER);
INSERT INTO t2 VALUES('x', 1);
ANALYZE t2;
CREATE TABLE t3(id INTEGER PRIMARY KEY, a TEXT, b INTEGER);
INSERT INTO t3 VALUES(1, 'x', 10);
INSERT INTO t3 VALUES(2, 'x', 20);
CREATE INDEX i3 ON t3(a, b);
ANALYZE t3;
SELECT tbl, idx, stat FROM sqlite_stat1 ORDER BY tbl, idx;
-- case: holdout/other/784-create-table-as-select-keeps-the-storage-class-of-
CREATE TABLE s(a);
INSERT INTO s VALUES ('1');
INSERT INTO s VALUES (2);
INSERT INTO s VALUES (3.5);
INSERT INTO s VALUES (X'04');
INSERT INTO s VALUES (NULL);
CREATE TABLE t AS SELECT a FROM s;
SELECT typeof(a), quote(a) FROM t ORDER BY rowid;
SELECT sql FROM sqlite_master WHERE name = 't';
CREATE TABLE u AS SELECT 1 AS i, 'x' AS t, 1.5 AS r, X'01' AS b, NULL AS n, 1+1 AS e, CAST(1 AS TEXT) AS c, abs(-1) AS f;
SELECT sql FROM sqlite_master WHERE name = 'u';
