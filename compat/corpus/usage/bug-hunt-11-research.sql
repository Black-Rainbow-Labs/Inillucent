-- case: bug-hunt-11/research/expr-in-index-table-scan
CREATE TABLE t0(c0 REAL, c1 TEXT);
CREATE INDEX i0 ON t0(+c0, c0);
INSERT INTO t0(c0) VALUES(0);
SELECT CAST(+ t0.c0 AS BLOB) LIKE 0 FROM t0;
SELECT CAST(+t0.c0 AS BLOB), typeof(+t0.c0) FROM t0;
-- case: bug-hunt-11/research/expr-in-index-not-indexed
CREATE TABLE t0(c0 REAL, c1 TEXT);
CREATE INDEX i0 ON t0(+c0, c0);
INSERT INTO t0(c0) VALUES(0),(2.5),(NULL);
SELECT +c0, typeof(+c0), typeof(c0) FROM t0 NOT INDEXED ORDER BY rowid;
SELECT +c0, typeof(+c0), typeof(c0) FROM t0 INDEXED BY i0;
-- case: bug-hunt-11/research/expr-in-index-covering-scan
CREATE TABLE t(a INTEGER, b INTEGER, c TEXT);
CREATE INDEX i ON t(a + 1, a);
INSERT INTO t VALUES(1,2,'x'),(5,6,'y'),(NULL,7,'z');
SELECT a + 1, a FROM t ORDER BY rowid;
SELECT typeof(a + 1) FROM t ORDER BY rowid;
SELECT a + 1, typeof(a) FROM t;
SELECT count(*), sum(a + 1) FROM t;
-- case: bug-hunt-11/research/expr-in-index-where
CREATE TABLE t(a INTEGER, b INTEGER, c TEXT);
CREATE INDEX i ON t(a * 2, a);
INSERT INTO t VALUES(1,2,'x'),(5,6,'y'),(NULL,7,'z'),(3,8,'w');
SELECT a * 2, a FROM t WHERE a * 2 > 4 ORDER BY 1;
SELECT typeof(a * 2) FROM t WHERE a * 2 IS NOT NULL ORDER BY a * 2;
SELECT a FROM t WHERE a * 2 = 10;
SELECT count(*) FROM t WHERE a * 2 BETWEEN 2 AND 6;
-- case: bug-hunt-11/research/expr-in-index-order-by
CREATE TABLE t(a INTEGER, b INTEGER, c TEXT);
CREATE INDEX i ON t(-a, a);
INSERT INTO t VALUES(1,2,'x'),(5,6,'y'),(NULL,7,'z'),(3,8,'w');
SELECT -a, a FROM t ORDER BY -a;
SELECT typeof(-a), typeof(a) FROM t ORDER BY -a DESC;
SELECT a FROM t ORDER BY -a LIMIT 2;
SELECT -a FROM t ORDER BY -a LIMIT 2;
-- case: bug-hunt-11/research/expr-in-index-unary-plus-text
CREATE TABLE t(x TEXT, y);
CREATE INDEX i ON t(+x, x);
INSERT INTO t VALUES('1',1),('b',2),(NULL,3);
SELECT +x, typeof(+x) FROM t;
SELECT length(+x) FROM t;
SELECT +x || 'z' FROM t ORDER BY rowid;
-- case: bug-hunt-11/research/expr-in-index-group-by
CREATE TABLE t(a INTEGER, b INTEGER);
CREATE INDEX i ON t(a + 0, a);
INSERT INTO t VALUES(1,1),(1,2),(2,3),(NULL,4);
SELECT a + 0, typeof(a + 0), count(*) FROM t GROUP BY a + 0;
SELECT typeof(a + 0), count(*) FROM t GROUP BY a;
-- case: bug-hunt-11/research/generated-unique-reindex-constant
CREATE TABLE t0(c0, c1 TEXT GENERATED ALWAYS AS (1) UNIQUE);
INSERT INTO t0(c0) VALUES (1);
REINDEX;
INSERT INTO t0(c0) VALUES (0);
REINDEX;
SELECT * FROM t0;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/reindex-virtual-generated-unique
CREATE TABLE t(a, g AS (a + 1) VIRTUAL UNIQUE);
INSERT INTO t(a) VALUES (1),(2),(3);
REINDEX;
INSERT INTO t(a) VALUES (1);
INSERT INTO t(a) VALUES (10);
REINDEX t;
INSERT INTO t(a) VALUES (10);
SELECT a, g FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/reindex-stored-generated-unique
CREATE TABLE t(a, g AS (a * 2) STORED UNIQUE);
INSERT INTO t(a) VALUES (1),(2),(3);
REINDEX;
INSERT INTO t(a) VALUES (1);
INSERT INTO t(a) VALUES (10);
REINDEX;
INSERT INTO t(a) VALUES (10);
SELECT a, g FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/reindex-index-on-virtual-generated
CREATE TABLE t(a, g AS (a + 1));
CREATE UNIQUE INDEX i ON t(g);
INSERT INTO t(a) VALUES (1),(2);
REINDEX i;
INSERT INTO t(a) VALUES (1);
SELECT a, g FROM t ORDER BY a;
SELECT a FROM t WHERE g = 3;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/reindex-expression-unique
CREATE TABLE t(a, b);
CREATE UNIQUE INDEX i ON t(a + b);
INSERT INTO t VALUES (1,2),(2,2);
REINDEX;
INSERT INTO t VALUES (0,3);
INSERT INTO t VALUES (5,5);
REINDEX i;
INSERT INTO t VALUES (6,4);
SELECT a, b FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/reindex-virtual-generated-index-expression
CREATE TABLE t(a, g AS (a + 1));
CREATE UNIQUE INDEX i ON t(g * 2);
INSERT INTO t(a) VALUES (1),(2);
REINDEX;
INSERT INTO t(a) VALUES (1);
SELECT a, g FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-11/research/integrity-check-constant-check-empty-table
CREATE TABLE t0(c0 CHECK(ABS(-9223372036854775808)));
PRAGMA integrity_check;
PRAGMA quick_check;
-- case: bug-hunt-11/research/integrity-check-constant-check-with-rows
CREATE TABLE t1(a CHECK(abs(a) > 0));
INSERT INTO t1 VALUES(1);
PRAGMA ignore_check_constraints=1;
INSERT INTO t1 VALUES(-9223372036854775807-1);
INSERT INTO t1 VALUES(0);
PRAGMA ignore_check_constraints=0;
PRAGMA integrity_check;
PRAGMA quick_check;
-- case: bug-hunt-11/research/integrity-check-check-errors-for-one-row
CREATE TABLE t1(a, b CHECK(b > 0));
INSERT INTO t1 VALUES(1, 1),(2, 2);
PRAGMA integrity_check;
PRAGMA ignore_check_constraints=1;
INSERT INTO t1 VALUES(3, 0);
PRAGMA ignore_check_constraints=0;
PRAGMA integrity_check;
PRAGMA quick_check;
-- case: bug-hunt-11/research/integrity-check-check-with-rows-ok
CREATE TABLE t0(c0 CHECK(ABS(-9223372036854775807)));
INSERT INTO t0 VALUES (1),(2);
PRAGMA integrity_check;
PRAGMA quick_check;
-- case: bug-hunt-11/research/integrity-check-overflow-check-insert-refused
CREATE TABLE t0(c0 CHECK(ABS(-9223372036854775808)));
INSERT INTO t0 VALUES (1);
PRAGMA integrity_check;
SELECT count(*) FROM t0;
