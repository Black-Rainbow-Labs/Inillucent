-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/collation/3-indexed-text-pk-compared-with-explicit-collate-noc
create table t2(a text primary key);
insert into t2 values ('lol'), ('LOL'), ('lOl');
select * from t2 where a COLLATE NOCASE = 'LOL';
select count(*) from t2 where a = 'LOL';
-- case: holdout/collation/22-collate-in-on-group-by-and-distinct-turso-ignored-
CREATE TABLE a(s TEXT);
CREATE TABLE b(s TEXT);
INSERT INTO a VALUES ('A');
INSERT INTO b VALUES ('a');
SELECT a.s, b.s FROM a JOIN b ON a.s COLLATE NOCASE = b.s;
CREATE TABLE t(s TEXT);
INSERT INTO t VALUES ('a'),('A'),('b');
SELECT COUNT(*) AS groups FROM (SELECT s COLLATE NOCASE FROM t GROUP BY s COLLATE NOCASE);
select distinct s collate nocase from t;
-- case: holdout/collation/62-check-constraint-uses-the-column-collation-nocase-
CREATE TABLE users(username TEXT COLLATE NOCASE, privilege INT, CHECK(CASE WHEN username='admin' THEN privilege>=100 ELSE 1 END));
INSERT INTO users VALUES ('Admin', 25);
INSERT INTO users VALUES ('admin', 25);
INSERT INTO users VALUES ('bob', 25);
SELECT * FROM users;
-- case: holdout/collation/64-collate-nocase-operand-inside
CREATE TABLE t(name TEXT CHECK((name COLLATE NOCASE || '') <> 'admin'));
INSERT INTO t VALUES ('Admin');
SELECT 'Admin' COLLATE NOCASE || '' <> 'admin', ('Admin' COLLATE NOCASE) <> 'admin';
-- case: holdout/collation/122-collate-inside-one-aggregate-does-not-leak-to-the-
CREATE TABLE t(a TEXT, b TEXT);
INSERT INTO t VALUES ('z', 'a');
INSERT INTO t VALUES ('a', 'Z');
SELECT MIN(a COLLATE NOCASE), MAX(b) FROM t;
SELECT MAX(a), MIN(a COLLATE NOCASE), MIN(a) FROM t;
-- case: holdout/collation/197-collation-of-the-right-column-applies-only-when-th
CREATE TABLE t1(a TEXT);
CREATE TABLE t2(b TEXT COLLATE NOCASE, c INT);
INSERT INTO t1 VALUES('Hello');
INSERT INTO t2 VALUES('HELLO', 1);
SELECT * FROM t1, t2 WHERE t1.a = t2.b;
SELECT * FROM t1, t2 WHERE t2.b = t1.a;
SELECT t1.a = t2.b, t2.b = t1.a, t1.a COLLATE NOCASE = t2.b FROM t1, t2;
-- case: holdout/collation/199-nullif-uses-the-column-s-declared-collation
CREATE TABLE t (col TEXT COLLATE NOCASE);
INSERT INTO t VALUES ('abc'), ('ABC'), ('xyz');
SELECT col, NULLIF(col, 'ABC') FROM t;
SELECT NULLIF('abc' COLLATE NOCASE, 'ABC'), NULLIF('ABC', 'abc' COLLATE NOCASE), max('a', 'B' COLLATE NOCASE), min('a' COLLATE NOCASE, 'B');
-- case: holdout/collation/243-collation-of-the-first-arm-of-a-three-arm-compound
SELECT 'Hello' COLLATE NOCASE UNION SELECT 'hello' UNION SELECT 'HELLO';
SELECT 'Hello' COLLATE NOCASE UNION SELECT 'hello';
SELECT 'hello' UNION SELECT 'Hello' COLLATE NOCASE;
SELECT 'Hello' COLLATE NOCASE INTERSECT SELECT 'hello';
SELECT 'Hello' COLLATE NOCASE EXCEPT SELECT 'hello';
-- case: holdout/collation/244-order-by-1-collate-nocase-on-a-compound-select
SELECT 'Hello' AS x UNION ALL SELECT 'hello' ORDER BY 1 COLLATE NOCASE;
SELECT 'b' AS x UNION ALL SELECT 'A' UNION ALL SELECT 'a' ORDER BY x COLLATE NOCASE, x DESC;
-- case: holdout/collation/246-order-by-1-collate-nocase-and-group-by-1-collate-n
CREATE TABLE t(x TEXT, v INT);
INSERT INTO t VALUES('b',1),('A',2),('c',3),('a',4),('B',1);
SELECT x FROM t ORDER BY 1 COLLATE NOCASE, v;
SELECT x, sum(v) FROM t GROUP BY 1 COLLATE NOCASE;
SELECT x, sum(v) FROM t GROUP BY x COLLATE NOCASE ORDER BY 1;
-- case: holdout/collation/265-expression-index-sorts-by-the-expression-s-collati
CREATE TABLE t(id INTEGER PRIMARY KEY, c TEXT COLLATE RTRIM);
CREATE INDEX idx_expr ON t(lower(c), id DESC);
INSERT INTO t VALUES (1, ''), (2, ' '), (3, '  '), (4, '');
SELECT id, quote(c) FROM t INDEXED BY idx_expr WHERE lower(c) IS NOT NULL ORDER BY lower(c), id DESC;
SELECT id, quote(c) FROM t NOT INDEXED WHERE lower(c) IS NOT NULL ORDER BY lower(c), id DESC;
-- case: holdout/collation/303-table-level-primary-key-with-a-collate-inside-the-
CREATE TABLE p(a TEXT, PRIMARY KEY(a COLLATE NOCASE));
INSERT INTO p VALUES ('a');
INSERT INTO p VALUES ('A');
SELECT * FROM p;
CREATE TABLE q(a TEXT, b, UNIQUE(a COLLATE NOCASE DESC, b));
CREATE INDEX qi ON q(a COLLATE NOCASE, b DESC);
PRAGMA index_xinfo(qi);
-- case: holdout/collation/305-between-takes-the-collation-of-its-first-operand
CREATE TABLE t(a COLLATE NOCASE, b);
INSERT INTO t VALUES ('ABC','abc');
SELECT a, b FROM t WHERE a BETWEEN b AND b;
SELECT a BETWEEN b AND b FROM t;
SELECT b BETWEEN a AND a FROM t;
SELECT a, b FROM t WHERE b BETWEEN a AND a;
-- case: holdout/collation/332-an-explicit-collate-on-one-in-expression-does-not-
SELECT 'A' COLLATE NOCASE IN ('a'), 'A' IN ('a');
SELECT 'A' COLLATE NOCASE = 'a', 'A' = 'a';
SELECT 'A' IN ('a' COLLATE NOCASE), 'A' IN ('b', 'a' COLLATE NOCASE);
-- case: holdout/collation/367-intersect-union-except-when-only-the-right-arm-col
CREATE TABLE q(x, y TEXT); INSERT INTO q VALUES(1,'a'),(2,'B'),(NULL,'c'),('3','z');
CREATE TABLE p(x, y TEXT COLLATE NOCASE); INSERT INTO p VALUES(1,'A'),(2,'b'),(NULL,'C'),(3,NULL),(2,'B'),(1.0,'a');
SELECT y FROM q INTERSECT SELECT y FROM p;
SELECT y FROM q UNION SELECT y FROM p;
SELECT y FROM q EXCEPT SELECT y FROM p;
SELECT y FROM p INTERSECT SELECT y FROM q;
SELECT y FROM p UNION SELECT y FROM q;
SELECT x FROM q UNION SELECT x FROM p;
-- case: holdout/collation/388-order-by-an-expression-over-a-nocase-column-sorts-
CREATE TABLE t(x TEXT COLLATE NOCASE);
INSERT INTO t VALUES('B'),('a');
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY x);
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY trim(x));
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY trim(x) DESC);
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY lower(x));
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY x || '');
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY (x));
SELECT group_concat(x) FROM (SELECT x FROM t ORDER BY +x);
-- case: holdout/collation/398-partial-index-where-uses-the-column-s-nocase-colla
CREATE TABLE t(s TEXT COLLATE NOCASE);
CREATE UNIQUE INDEX i ON t(s) WHERE s='a';
INSERT INTO t VALUES('A');
INSERT INTO t VALUES('a');
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: holdout/collation/400-comparison-collation-comes-only-from-its-own-opera
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT, id INTEGER);
INSERT INTO t VALUES('A','B',1);
SELECT 'b' = b FROM t;
SELECT 'b' = b FROM t WHERE a IS NOT NULL;
SELECT id, 'a' = a FROM t;
DELETE FROM t WHERE a IS NOT NULL AND 'b' = b;
SELECT count(*) FROM t;
-- case: holdout/collation/409-check-over
CREATE TABLE t(c TEXT COLLATE NOCASE, CHECK (c||'' = 'A'));
INSERT INTO t VALUES('a');
SELECT count(*) FROM t;
CREATE TABLE u(c TEXT COLLATE NOCASE, CHECK (c||'' = 'A'));
INSERT INTO u VALUES('A');
UPDATE u SET c='a';
SELECT count(*), c FROM u;
CREATE TABLE v(c TEXT COLLATE NOCASE, s TEXT, CHECK (c = 'A'), CHECK (CAST(c AS TEXT) = 'A'), CHECK (s||'' = 'A'));
INSERT INTO v VALUES('a', 'A');
INSERT INTO v VALUES('a', 'a');
SELECT * FROM v;
-- case: holdout/collation/426-group-by-a-wrapped-expression-over-a-nocase-column
CREATE TABLE t(e TEXT COLLATE NOCASE, n TEXT);
INSERT INTO t VALUES('a','x'),('A','y');
SELECT count(*) FROM t GROUP BY (e || '');
SELECT count(*) FROM t GROUP BY e;
SELECT count(*) FROM t GROUP BY trim(e);
DELETE FROM t WHERE rowid NOT IN (SELECT min(rowid) FROM t GROUP BY trim(e));
SELECT e FROM t ORDER BY rowid;
-- case: holdout/collation/427-
CREATE TABLE t(id INTEGER PRIMARY KEY, e TEXT COLLATE NOCASE);
INSERT INTO t VALUES(1,'A'),(2,'a');
SELECT id FROM t WHERE (e || '') = 'a';
SELECT id FROM t WHERE e = 'a';
SELECT id FROM t WHERE CAST(e AS TEXT) = 'a';
SELECT id FROM t WHERE lower(e) = 'a';
SELECT id FROM t WHERE +e = 'a';
SELECT id FROM t WHERE (e) = 'a';
SELECT id FROM t WHERE e COLLATE BINARY = 'a';
DELETE FROM t WHERE (e || '') = 'a';
SELECT id FROM t;
-- case: holdout/collation/434-two-indexes-one-on-a-nocase-expression-a-binary-co
CREATE TABLE t(a TEXT, b TEXT);
INSERT INTO t VALUES('B','x');
CREATE INDEX i1 ON t(a COLLATE NOCASE);
CREATE INDEX i2 ON t(b);
SELECT a, (a='b') AS p FROM t WHERE a='b' AND b>'a';
DELETE FROM t WHERE a='b' AND b>'a';
SELECT count(*) FROM t;
SELECT a FROM t WHERE a='b' COLLATE NOCASE AND b>'a';
-- case: holdout/collation/440-expression-index-seek-honours-an-explicit-collate-
CREATE TABLE t(a TEXT, c INTEGER);
INSERT INTO t VALUES('A',1),('a',2),('b',3);
CREATE INDEX ix ON t(lower(a));
SELECT c FROM t WHERE lower(a) = 'A' COLLATE NOCASE ORDER BY c;
SELECT c FROM t WHERE lower(a) > 'A' COLLATE NOCASE ORDER BY c;
DELETE FROM t WHERE lower(a) > 'A' COLLATE NOCASE;
SELECT c FROM t ORDER BY c;
-- case: holdout/collation/445-an-aggregate-argument-keeps-the-column-s-nocase-co
CREATE TABLE t(g INT, a TEXT COLLATE NOCASE);
CREATE TABLE u(g INT PRIMARY KEY, n INT);
INSERT INTO t VALUES(1,'a'),(1,'A'),(2,'a');
SELECT g, sum(a='A') FROM t GROUP BY g;
SELECT g, count(*) FILTER (WHERE a='A') FROM t GROUP BY g;
INSERT INTO u SELECT g, sum(a='A') FROM t GROUP BY g;
SELECT group_concat(n) FROM (SELECT n FROM u ORDER BY g);
-- case: holdout/collation/459-comparison-inside-an-aggregate-uses-the-column-s-n
CREATE TABLE t(g INT, a TEXT COLLATE NOCASE);
CREATE TABLE u(g INT PRIMARY KEY, n INT);
INSERT INTO t VALUES(1,'x'),(1,'X');
SELECT g, sum(a='X') FROM t GROUP BY g;
INSERT INTO u SELECT g, count(*) FILTER (WHERE a='X') FROM t GROUP BY g;
SELECT n FROM u;
CREATE INDEX ix ON t(g) WHERE g>0;
SELECT g, sum(a='X') FROM t GROUP BY g;
-- case: holdout/collation/469-in-uses-the-collation-of-the-left-operand
CREATE TABLE r(value TEXT COLLATE NOCASE);
INSERT INTO r VALUES('a');
SELECT 'A' IN (value), 'A' IN ('a' COLLATE NOCASE, 'x'), ('A' COLLATE NOCASE) IN (value COLLATE BINARY) FROM r;
SELECT value IN ('A'), value IN ('A', 'b'), value NOT IN ('A') FROM r;
-- case: holdout/collation/470-row-value-comparisons-use-column-collation-and-aff
CREATE TABLE c(a TEXT COLLATE NOCASE);
INSERT INTO c VALUES('A');
SELECT (SELECT a, 1 FROM c) = ('a', 1);
SELECT ('a', 1) IN (('A', 1), ('x' COLLATE NOCASE, 2));
SELECT (1, 2) IN ((CAST('1' AS TEXT), 2), (CAST('x' AS BLOB), 3));
SELECT (a, 1) = ('a', 1), ('a', 1) = (a, 1), (a, 1) < ('b', 1) FROM c;
CREATE TABLE d(x INTEGER, y TEXT);
INSERT INTO d VALUES (1, '1');
SELECT (x, y) = ('1', 1), (x, y) = (1, 1), (x, y) IN (('1', 1)) FROM d;
-- case: holdout/collation/471-case-x-when-y-uses-the-collation-of-x
CREATE TABLE cs(l TEXT, r TEXT COLLATE NOCASE);
INSERT INTO cs VALUES('a', 'A');
SELECT l = r, CASE l WHEN r THEN 1 ELSE 0 END FROM cs;
SELECT r = l, CASE r WHEN l THEN 1 ELSE 0 END FROM cs;
SELECT CASE 'a' WHEN r THEN 1 ELSE 0 END, CASE 'a' COLLATE NOCASE WHEN 'A' THEN 1 ELSE 0 END FROM cs;
-- case: holdout/collation/572-select-distinct-uses-the-column-s-nocase-collation
create table t(a text collate nocase);
insert into t values ('a'), ('A');
select distinct a from t;
select count(distinct a) from t;
select a, count(*) from t group by a;
select a from t union select a from t;
-- case: holdout/collation/589-rtrim-collation-ignores-trailing-spaces-only
select 'x' || char(9) = 'x' collate rtrim;
select 'x ' = 'x' collate rtrim, 'x  ' = 'x ' collate rtrim, ' x' = 'x' collate rtrim, 'x' || char(10) = 'x' collate rtrim, 'x' || char(0) = 'x' collate rtrim, 'x ' < 'x!' collate rtrim, 'x  ' < 'x' collate rtrim;
create table t(a text collate rtrim);
insert into t values ('a'), ('a '), ('a  '), ('a' || char(9));
select count(*), count(distinct a) from t;
select hex(a) from t where a = 'a' order by rowid;
-- case: holdout/collation/596-comparisons-use-binary-collation-by-default
select 'a' = 'A', 'a' < 'B', 'a' < 'b', 'B' < 'a', 'a' = 'a ', 'abc' > 'abd', 'Z' < 'a', 'é' > 'z';
select 'a' = 'A' collate nocase, 'a' = 'A' collate binary, 'a  ' = 'a' collate rtrim, 'a' COLLATE NOCASE = 'A' COLLATE BINARY;
-- case: holdout/collation/597-union-and-intersect-deduplicate-with-the-collation
SELECT 'a' COLLATE NOCASE UNION SELECT 'A';
SELECT 'A' UNION SELECT 'a' COLLATE NOCASE;
CREATE TABLE t1(s TEXT);
CREATE TABLE t2(s TEXT);
INSERT INTO t1 VALUES ('A'),('b');
INSERT INTO t2 VALUES ('a'),('C');
SELECT s COLLATE NOCASE FROM t1 INTERSECT SELECT s FROM t2;
SELECT s FROM t1 INTERSECT SELECT s COLLATE NOCASE FROM t2;
SELECT s COLLATE NOCASE FROM t1 EXCEPT SELECT s FROM t2;
-- case: holdout/collation/610-text-column-with-rtrim-collation-and-a-desc-index-
CREATE TABLE "t1" ("c1" TEXT COLLATE RTRIM);
INSERT INTO "t1" VALUES (' ');
CREATE INDEX "i1" ON "t1" ("c1" COLLATE RTRIM DESC);
INSERT INTO "t1" VALUES (1025.1655084065987);
SELECT "c1", typeof(c1) FROM "t1" ORDER BY "c1" COLLATE BINARY DESC, rowid ASC;
SELECT quote(c1) FROM t1 ORDER BY c1;
PRAGMA integrity_check;
-- case: holdout/collation/619-order-by-collate-rtrim-desc-on-a-column-compared-w
CREATE TABLE t6 (c1 TEXT, c2 TEXT, c4 TEXT, c5 BLOB);
INSERT INTO t6 VALUES (NULL, 'bar ', '0.0', X'f768b8');
INSERT INTO t6 VALUES ('x', 'bar', '0', X'01');
SELECT c1, c2, c4, c5 FROM t6 WHERE c4 = 0.0 ORDER BY c2 COLLATE RTRIM DESC, c4 DESC, c5 ASC, rowid ASC;
SELECT quote(c4), c4 = 0.0, c4 = '0.0' FROM t6;
-- case: holdout/collation/656-group-by-a-collate-nocase-over-a-table-with-a-bina
CREATE TABLE t(a TEXT, b INT);
CREATE INDEX i_a ON t(a);
INSERT INTO t VALUES ('a',1),('A',2),('b',3),('B',4),('a',5);
SELECT a COLLATE NOCASE, sum(b) FROM t GROUP BY a COLLATE NOCASE;
SELECT a, sum(b) FROM t GROUP BY a;
SELECT a, sum(b) FROM t GROUP BY a COLLATE NOCASE;
-- case: holdout/collation/714-correlated-in-with-an-upper-of-a-nocase-column
CREATE TABLE o(k INTEGER, v TEXT);
CREATE TABLE i(k INTEGER, x TEXT COLLATE NOCASE);
INSERT INTO o VALUES (1,'a'),(2,'B');
INSERT INTO i VALUES (1,'A'),(2,'b'),(2,'q');
SELECT o.k FROM o WHERE o.v IN (SELECT upper(i.x) FROM i WHERE i.k = o.k) ORDER BY 1;
SELECT o.k FROM o WHERE (o.v IN (SELECT upper(i.x) FROM i WHERE i.k = o.k)) OR o.k = -1 ORDER BY 1;
SELECT o.k FROM o WHERE o.v IN (SELECT i.x FROM i WHERE i.k = o.k) ORDER BY 1;
SELECT o.k FROM o WHERE o.v COLLATE NOCASE IN (SELECT i.x FROM i WHERE i.k = o.k) ORDER BY 1;
-- case: holdout/collation/722-collation-of-a-compound-arm-that-wraps-a-column-in
CREATE TABLE a(x TEXT COLLATE NOCASE);
CREATE TABLE b(x TEXT);
INSERT INTO a VALUES ('A'), ('b');
INSERT INTO b VALUES ('a'), ('B');
SELECT x FROM a UNION SELECT x FROM b ORDER BY 1;
SELECT x || '' FROM a UNION SELECT x FROM b ORDER BY 1;
SELECT lower(x) FROM a UNION SELECT upper(x) FROM b ORDER BY 1;
SELECT +x FROM a UNION SELECT x FROM b ORDER BY 1;
SELECT CAST(x AS TEXT) FROM a UNION SELECT x FROM b ORDER BY 1;
SELECT x FROM a EXCEPT SELECT x FROM b ORDER BY 1;
SELECT x FROM b EXCEPT SELECT x FROM a ORDER BY 1;
-- case: holdout/collation/794-order-by-with-collate-on-the-output-column-of-unio
SELECT 'b' AS x UNION SELECT 'A' ORDER BY x COLLATE NOCASE;
SELECT 'b' AS x UNION ALL SELECT 'A' ORDER BY x COLLATE NOCASE;
SELECT 'b' AS x UNION SELECT 'A' ORDER BY 1 COLLATE NOCASE;
SELECT 'b' AS x EXCEPT SELECT 'A' ORDER BY x COLLATE NOCASE;
SELECT 'b' AS x INTERSECT SELECT 'b' ORDER BY x COLLATE NOCASE;
SELECT 'b' AS x UNION SELECT 'A' ORDER BY x COLLATE NOCASE DESC;
SELECT 'b' AS x UNION SELECT 'A' ORDER BY lower(x);
SELECT 'b' AS x UNION SELECT 'A' ORDER BY x || '';
-- case: holdout/collation/813-between-with-collate-nocase-on-an-operand
SELECT 'B' BETWEEN 'a' COLLATE NOCASE AND 'c' COLLATE NOCASE;
SELECT 'B' BETWEEN 'a' AND 'c', 'B' COLLATE NOCASE BETWEEN 'a' AND 'c', 'B' BETWEEN ('a' COLLATE NOCASE) AND 'c', 'B' BETWEEN 'a' AND 'c' COLLATE NOCASE;
CREATE TABLE t(s TEXT COLLATE NOCASE);
INSERT INTO t VALUES ('B'), ('d'), ('A');
SELECT s FROM t WHERE s BETWEEN 'a' AND 'c' ORDER BY s;
SELECT s FROM t WHERE s NOT BETWEEN 'a' AND 'c' ORDER BY s;
SELECT s FROM t WHERE s BETWEEN 'a' COLLATE BINARY AND 'c' ORDER BY s;
-- case: holdout/collation/826-correlated-comparisons-follow-sqlite-s-operand-rul
CREATE TABLE k(t TEXT, i INTEGER);
INSERT INTO k VALUES ('1',1),('02',2),('abc',NULL),(NULL,4),('ABC ',5);
CREATE TABLE oc(w TEXT COLLATE NOCASE, v TEXT COLLATE RTRIM, z INTEGER, x);
INSERT INTO oc VALUES ('abc','abc',1,1),('ABC','ABC',2,'2'),('Abc ','ABC',3,'02'),(NULL,NULL,NULL,NULL),('zzz','1  ',5,5.0);
SELECT w FROM oc WHERE EXISTS (SELECT 1 FROM k WHERE k.t = oc.w) ORDER BY rowid;
SELECT w FROM oc WHERE EXISTS (SELECT 1 FROM k WHERE k.t = oc.w OR 0) ORDER BY rowid;
SELECT v FROM oc WHERE EXISTS (SELECT 1 FROM k WHERE k.t = oc.v OR 0) ORDER BY rowid;
SELECT x FROM oc WHERE EXISTS (SELECT 1 FROM k WHERE k.t = oc.x) ORDER BY rowid;
SELECT x FROM oc WHERE EXISTS (SELECT 1 FROM k WHERE (k.t, k.i) = (oc.x, oc.z) OR 0) ORDER BY rowid;
