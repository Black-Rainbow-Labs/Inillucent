-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/subqueries/12-drop-table-if-exists-on-an-existing-table-turso-pa
CREATE TABLE t(x);
DROP TABLE IF EXISTS t;
DROP TABLE IF EXISTS t;
select count(*) from sqlite_schema;
-- case: holdout/subqueries/17-in-compound-select-with-order-by-and-limit-turso-p
CREATE TABLE t6 (a TEXT, b TEXT);
INSERT INTO t6 VALUES('a','0');
INSERT INTO t6 VALUES('b','1');
INSERT INTO t6 VALUES('c','2');
INSERT INTO t6 VALUES('d','3');
SELECT a FROM t6 WHERE b IN (SELECT b FROM t6 WHERE a<='b' UNION SELECT '3' AS x ORDER BY 1 LIMIT 1);
-- case: holdout/subqueries/50-correlated-scalar-subquery-in-the-select-list-is-e
CREATE TABLE t1(id INTEGER PRIMARY KEY);
CREATE TABLE t2(id INTEGER PRIMARY KEY, val INT);
INSERT INTO t1 VALUES(1), (2), (3);
INSERT INTO t2 VALUES(1, 100), (2, 200), (3, 300);
SELECT id, (SELECT val FROM t2 WHERE t2.id = t1.id) AS subq FROM t1;
-- case: holdout/subqueries/119-row-value-in-list
SELECT (1,2) IN ((1,2),(3,4));
CREATE TABLE t(a INT, b INT, c TEXT);
INSERT INTO t VALUES (1,10,'x'),(2,20,'y'),(1,20,'z');
SELECT * FROM t WHERE (a, b) IN ((1,10),(2,20)) ORDER BY a, b;
SELECT (1,2) = (1,2), (1,2) < (1,3), (1,NULL) = (1,2), (1,NULL) IS (1,NULL);
-- case: holdout/subqueries/137-limit-0-inside-a-scalar-subquery
create table t(a);
insert into t values (1), (2), (3);
select 'row', (select * from t limit 0);
select 'row', (select a from t order by a desc limit 1 offset 1);
-- case: holdout/subqueries/181-count-distinct-in-a-correlated-subquery-starts-fre
CREATE TABLE t1(a INTEGER, b TEXT);
INSERT INTO t1 VALUES(1,'abc'),(2,'def'),(3,'abc'),(4,'ghi'),(5,'abc');
SELECT a, (SELECT count(DISTINCT t2.b) FROM t1 t2 WHERE t2.rowid <= t1.rowid) FROM t1 ORDER BY rowid;
-- case: holdout/subqueries/185-correlated-subquery-over-the-same-view-as-the-oute
CREATE TABLE t1(a INTEGER, b INTEGER);
INSERT INTO t1 VALUES(1,100),(2,200),(3,300);
CREATE VIEW v1 AS SELECT a, b FROM t1;
SELECT a, b, (SELECT sum(t2.b) FROM v1 t2 WHERE t2.a <= v1.a) AS running FROM v1 ORDER BY a;
-- case: holdout/subqueries/190-correlated-subquery-in-having-through-a-derived-ta
CREATE TABLE t(grp TEXT, cat TEXT, val INTEGER);
INSERT INTO t VALUES('a','x',1),('a','y',2);
SELECT grp, cat, SUM(val) AS s FROM t GROUP BY grp, cat HAVING s = (SELECT sq FROM (SELECT MAX(val) AS sq FROM t t2 WHERE t2.grp = t.grp));
-- case: holdout/subqueries/198-correlated-exists-inside-a-case-inside-an-aggregat
CREATE TABLE items (id INTEGER, val INTEGER, cat TEXT);
CREATE TABLE refs (id INTEGER);
INSERT INTO items VALUES (1,10,'A'),(2,20,'A'),(3,30,'B'),(4,40,'B');
INSERT INTO refs VALUES (1),(3);
SELECT cat, SUM(CASE WHEN EXISTS (SELECT 1 FROM refs WHERE refs.id = items.id) THEN val ELSE 0 END) AS s FROM items GROUP BY cat;
-- case: holdout/subqueries/235-compound-select-with-limit-and-offset
SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 LIMIT 2;
SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 LIMIT 2 OFFSET 1;
SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 LIMIT 2 OFFSET 1;
SELECT 3 UNION SELECT 1 UNION SELECT 2 ORDER BY 1 DESC LIMIT 2;
-- case: holdout/subqueries/238-limit-on-a-compound-select-used-as-in-subquery
CREATE TABLE t(x INTEGER);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
SELECT * FROM t WHERE x IN (SELECT 1 UNION SELECT 2 UNION SELECT 3 LIMIT 2);
SELECT * FROM t WHERE x IN (SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 LIMIT 2);
SELECT * FROM t WHERE x IN (SELECT 3 UNION SELECT 2 UNION SELECT 1 ORDER BY 1 DESC LIMIT 2) ORDER BY x;
-- case: holdout/subqueries/253-correlated-scalar-subquery-as-an-aggregate-argumen
CREATE TABLE t1(grp TEXT, val INTEGER);
CREATE TABLE t2(grp TEXT, factor INTEGER);
INSERT INTO t1 VALUES ('a',1),('a',2),('b',3),('b',4);
INSERT INTO t2 VALUES ('a',10),('b',20);
SELECT grp, sum((SELECT factor FROM t2 WHERE t2.grp = t1.grp)) FROM t1 GROUP BY grp;
SELECT grp, sum(val * (SELECT factor FROM t2 WHERE t2.grp = t1.grp)) FROM t1 GROUP BY grp;
-- case: holdout/subqueries/338-scalar-subquery-over-multi-row-values-returns-the-
SELECT (VALUES(1),(2),(3));
SELECT (SELECT 5 UNION ALL SELECT 6), (SELECT 1 WHERE 0);
SELECT (VALUES(7)) + 1;
-- case: holdout/subqueries/347-subquery-in-having-or-order-by-that-reads-a-bare-c
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES (1,10),(2,20);
SELECT a FROM t GROUP BY a HAVING (SELECT b) = 20;
SELECT a FROM t GROUP BY a HAVING (SELECT b) IS NULL;
CREATE TABLE t2(a INTEGER PRIMARY KEY, b);
INSERT INTO t2 VALUES (1,10),(2,30),(3,20);
SELECT a FROM t2 GROUP BY a ORDER BY (SELECT b);
CREATE TABLE t3(a,b);
INSERT INTO t3 VALUES (1,10),(1,20),(2,30);
SELECT a, count(*) FROM t3 GROUP BY a HAVING b IN (SELECT 10 UNION SELECT 30);
-- case: holdout/subqueries/348-correlated-exists-inside-a-from-less-scalar-subque
CREATE TABLE o(k);
CREATE TABLE i(k);
INSERT INTO o VALUES (1),(2);
INSERT INTO i VALUES (1);
SELECT o.k, (SELECT 1 WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k)) FROM o;
SELECT o.k, (SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM i WHERE i.k = o.k)) FROM o;
SELECT o.k FROM o WHERE (SELECT 1 WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k));
SELECT o.k, (SELECT count(*) WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k)) FROM o;
-- case: holdout/subqueries/349-correlated-exists-over-an-empty-table
CREATE TABLE o(k);
CREATE TABLE i(k);
INSERT INTO o VALUES (1);
SELECT * FROM o WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k);
SELECT * FROM o WHERE NOT EXISTS (SELECT 1 FROM i WHERE i.k = o.k);
SELECT * FROM o WHERE o.k IN (SELECT k FROM i);
SELECT * FROM o WHERE o.k NOT IN (SELECT k FROM i);
-- case: holdout/subqueries/354-correlated-subquery-in-having-of-an-aggregate-quer
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT count(*) FROM t HAVING (SELECT t.a) IS NOT NULL;
SELECT count(*) FROM t HAVING EXISTS (SELECT 1 WHERE t.a IS NOT NULL);
CREATE TABLE u(b); INSERT INTO u VALUES (10),(20);
SELECT u.b FROM u WHERE (SELECT count(*) FROM t HAVING (SELECT u.b) > 0) > 0;
-- case: holdout/subqueries/365-row-value-in-and-not-in-follow-three-valued-logic-
CREATE TABLE w(a, b);
INSERT INTO w VALUES (NULL,1),(1,1),(2,NULL),(2,2);
SELECT a, b FROM w WHERE (a,b) IN ((1,1),(2,2)) ORDER BY 1,2;
SELECT a, b FROM w WHERE (a,b) NOT IN ((1,1),(2,2)) ORDER BY 1,2;
SELECT ((NULL,1) IN ((1,1))) IS NULL, ((2,NULL) IN ((2,2))) IS NULL, ((2,NULL) NOT IN ((2,2))) IS NULL, ((1,1) IN ((1,1),(NULL,2))), ((1,2) IN ((1,1),(1,NULL))) IS NULL;
-- case: holdout/subqueries/369-outer-order-by-over-an-ordered-compound-subquery-w
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c TEXT);
INSERT INTO t(a,b,c) VALUES (1,1,'x'),(3,2,'y'),(97,3,'z'),(99,4,'w'),(50,5,'m'),(2,6,'n');
CREATE INDEX ia ON t(a);
SELECT * FROM (SELECT a FROM t WHERE a<5 UNION ALL SELECT a FROM t WHERE a>95 ORDER BY a) x ORDER BY a LIMIT 10;
SELECT * FROM (SELECT a FROM t WHERE a<5 UNION ALL SELECT a FROM t WHERE a>95 ORDER BY a DESC) x ORDER BY a LIMIT 3 OFFSET 1;
-- case: holdout/subqueries/370-outer-where-applied-to-a-union-all-subquery
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, c TEXT);
CREATE TABLE u(id INTEGER PRIMARY KEY, a INTEGER, c TEXT);
CREATE INDEX ia ON t(a);
CREATE INDEX ua ON u(a);
INSERT INTO t(a,c) VALUES (7,'t7'),(8,'t8');
INSERT INTO u(a,c) VALUES (7,'u7'),(9,'u9');
SELECT * FROM (SELECT a,c FROM t UNION ALL SELECT a,c FROM u) WHERE a=7 ORDER BY c;
EXPLAIN QUERY PLAN SELECT * FROM (SELECT a,c FROM t UNION ALL SELECT a,c FROM u) WHERE a=7;
-- case: holdout/subqueries/390-compound-select-as-a-scalar-subquery-or-exists-ope
CREATE TABLE a(id INTEGER, x TEXT);
CREATE TABLE b(id INTEGER, x TEXT);
INSERT INTO b VALUES(1,'y');
SELECT (SELECT x FROM a WHERE id=1 UNION ALL SELECT x FROM b WHERE id=1 LIMIT 1);
SELECT EXISTS(SELECT 1 FROM a UNION SELECT 1 FROM b);
SELECT 1 IN (SELECT 1 UNION SELECT 2);
SELECT (SELECT x FROM (SELECT x FROM a UNION ALL SELECT x FROM b) LIMIT 1);
SELECT * FROM b WHERE EXISTS (SELECT 1 FROM a UNION ALL SELECT 2);
-- case: holdout/subqueries/393-limit-and-offset-given-as-scalar-subqueries-on-a-c
CREATE TABLE a(x INTEGER);
INSERT INTO a VALUES(1),(2);
CREATE TABLE b(n INTEGER);
INSERT INTO b VALUES(1);
SELECT x FROM a UNION SELECT x FROM a ORDER BY 1 LIMIT (SELECT n FROM b);
SELECT 1 UNION SELECT 2 ORDER BY 1 LIMIT 1 OFFSET (SELECT 1);
SELECT 1 UNION SELECT 2 LIMIT (SELECT 1);
SELECT x FROM a UNION ALL SELECT x FROM a ORDER BY 1 LIMIT (SELECT n FROM b) OFFSET (SELECT n FROM b);
-- case: holdout/subqueries/465-column-resolution-falls-back-to-the-outer-query-wh
SELECT (SELECT (SELECT t.x) FROM (SELECT 2 AS y) AS t) as from_outer, (SELECT (SELECT t.y) FROM (SELECT 2 AS y) AS t) as from_inner FROM (SELECT 1 AS x) AS t;
-- case: holdout/subqueries/480-row-value-between
SELECT (1,2) BETWEEN (1,1) AND (1,3);
SELECT (1,2) NOT BETWEEN (1,1) AND (1,3);
SELECT (1,2) < (1,3), (1,2) <= (1,2), (1,NULL) < (2,0), (1,NULL) < (1,5), (NULL,1) = (NULL,1);
-- case: holdout/subqueries/484-correlated-column-in-the-group-by-of-a-scalar-subq
CREATE TABLE o(x);
CREATE TABLE i(y);
INSERT INTO o VALUES (1);
INSERT INTO i VALUES (1),(2);
SELECT (SELECT count(*) FROM i GROUP BY o.x) FROM o;
SELECT (SELECT count(*) FROM i GROUP BY o.x HAVING o.x = 1) FROM o;
-- case: holdout/subqueries/485-pragma-table-info-used-in-a-scalar-subquery-inside
CREATE TABLE t(a INTEGER PRIMARY KEY, x INTEGER);
SELECT name FROM pragma_table_info('t') WHERE cid = (SELECT cid FROM pragma_table_info('t') WHERE pk = 1);
SELECT name, (SELECT count(*) FROM pragma_table_info('t')) FROM pragma_table_info('t');
-- case: holdout/subqueries/500-a-multi-column-subquery-as-the-left-side-of-in
SELECT (SELECT 1, 2) IN (SELECT 1, 2);
SELECT (SELECT 1, 2) IN (VALUES(1, 2));
SELECT (SELECT 1, 2) IN ((1, 2), (3, 4));
SELECT ((SELECT 1, 2), 3) IN ((1, 3));
SELECT (1, 2) IN (SELECT 1, 2);
SELECT (1, 2) IN (SELECT 1);
-- case: holdout/subqueries/505-order-by-a-correlated-scalar-subquery-in-a-group-b
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t2 VALUES (9, 10, 'p'), (2, 5, 'q'), (2, -1, 'r');
INSERT INTO t3 VALUES (1, 11, ''), (2, 0, 'X'), (3, 6, 'z');
SELECT (SELECT count(*) FROM t2 AS y WHERE y.b > x.b) AS r1, x.c FROM t3 AS x GROUP BY x.c ORDER BY 1 DESC;
-- case: holdout/subqueries/509-row-value-not-in-subquery-with-null-components
CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER, b INTEGER);
INSERT INTO t VALUES (1,1,1),(2,1,2),(3,2,1),(4,2,2),(5,1,NULL),(6,NULL,9);
CREATE TABLE allow (a INTEGER, b INTEGER);
INSERT INTO allow VALUES (1,2),(2,1);
SELECT id FROM t WHERE (a,b) NOT IN (SELECT a,b FROM allow) ORDER BY id;
SELECT id, (a,b) NOT IN (SELECT a,b FROM allow) AS r FROM t ORDER BY id;
SELECT id, (a,b) IN (SELECT a,b FROM allow) AS r FROM t ORDER BY id;
-- case: holdout/subqueries/511-outer-aggregate-used-inside-a-correlated-subquery-
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t2 VALUES (1, 5, 'p'), (2, 1, 'q');
INSERT INTO t3 VALUES (1, 3, 'a'), (2, 4, 'a'), (3, 0, 'b');
SELECT x.c, count(*), (SELECT count(*) FROM t2 AS y WHERE y.b > min(x.b)) AS r1 FROM t3 AS x GROUP BY x.c ORDER BY 1;
-- case: holdout/subqueries/586-queries-that-read-sqlite-schema-or-use-a-compound-
select group_concat(name) over () from sqlite_schema;
values (1) intersect values (1);
PRAGMA integrity_check;
select count(*) from sqlite_schema;
-- case: holdout/subqueries/624-subquery-alias-containing-a-dot-referenced-from-a-
SELECT * FROM (SELECT 1 as x) AS "a.b" WHERE EXISTS (SELECT "a.b".x);
SELECT * FROM (SELECT 1 as x) AS "." WHERE EXISTS (SELECT ".".x);
SELECT * FROM (SELECT 1 as x) AS "foo.bar" WHERE EXISTS (SELECT "foo.bar".x);
SELECT "a.b".x FROM (SELECT 1 as x) AS "a.b";
-- case: holdout/subqueries/628-case-over-in-between-and-row-value-comparisons-in-
create table t0(c0, c1);
insert into t0 values (1, 2), (0, 3), (NULL, 4);
SELECT ALL t0.c0 FROM t0 WHERE (CASE (t0.c0 IN (t0.c0)) WHEN (('348323090') NOT BETWEEN (t0.c0) AND (t0.c0)) THEN (((t0.c0)) BETWEEN ((0.15767189706265294)) AND ((t0.c0))) WHEN ((((((((t0.c0)OR(t0.c0)))OR(t0.c0)))AND(t0.c0)))AND(t0.c0)) THEN (- (t0.c0)) ELSE ((t0.c0) NOT NULL) END) ORDER BY 1;
SELECT (0.97, t0.c0, 'a', x'') <= (1, t0.c0, 'a', x'') FROM t0;
-- case: holdout/subqueries/636-deeply-nested-expression-long-or-chain-and-nested-
CREATE TABLE t(x);
INSERT INTO t VALUES (1);
SELECT count(*) FROM t WHERE 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1 OR 1;
SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT 1 FROM t) FROM t) FROM t) FROM t) FROM t) FROM t) FROM t) FROM t) FROM t) FROM t;
SELECT CASE WHEN 1 THEN CASE WHEN 1 THEN CASE WHEN 1 THEN CASE WHEN 1 THEN CASE WHEN 1 THEN 1 ELSE 0 END ELSE 0 END ELSE 0 END ELSE 0 END ELSE 0 END FROM t;
-- case: holdout/subqueries/684-aggregate-filter-with-an-in-subquery-in-having
SELECT 1 GROUP BY 1 HAVING COUNT(*) FILTER (WHERE 1 IN (SELECT 2)) = 1;
SELECT 1 GROUP BY 1 HAVING COUNT(*) FILTER (WHERE 1 IN (SELECT 1)) = 1;
CREATE TABLE t(a);
INSERT INTO t VALUES (1), (2);
SELECT a, COUNT(*) FILTER (WHERE a IN (SELECT 2)) FROM t GROUP BY a;
-- case: holdout/subqueries/708-having-on-an-aggregate-with-a-scalar-subquery-in-i
CREATE TABLE t (id INTEGER PRIMARY KEY, r REAL);
INSERT INTO t VALUES (1, 2.5);
SELECT id FROM t GROUP BY id HAVING AVG(r) FILTER (WHERE (SELECT 1)) != 0;
SELECT id, AVG(r) FILTER (WHERE (SELECT 1)) FROM t GROUP BY id;
SELECT id, AVG(r) FILTER (WHERE (SELECT 0)) FROM t GROUP BY id;
-- case: holdout/subqueries/709-too-many-terms-in-a-compound-select
SELECT 1 UNION ALL SELECT 1 UNION ALL SELECT 1 UNION ALL SELECT 1;
-- case: holdout/subqueries/712-order-by-before-a-compound-operator
SELECT 1 EXCEPT SELECT 2 ORDER BY 1 COLLATE nocase EXCEPT SELECT 3;
SELECT 1 UNION SELECT 2 ORDER BY 1 UNION SELECT 3;
SELECT 1 UNION SELECT 2 LIMIT 1 UNION SELECT 3;
SELECT 1 ORDER BY 1 UNION SELECT 2;
-- case: holdout/subqueries/746-id-in-select-combined-with-another-in-predicate
CREATE TABLE issues (id TEXT PRIMARY KEY, issue_type TEXT NOT NULL, status TEXT NOT NULL);
CREATE TABLE labels (issue_id TEXT NOT NULL, label TEXT NOT NULL, PRIMARY KEY (issue_id, label), FOREIGN KEY (issue_id) REFERENCES issues(id) ON DELETE CASCADE);
INSERT INTO issues VALUES ('a', 'task', 'open');
INSERT INTO issues VALUES ('b', 'feature', 'open');
INSERT INTO labels VALUES ('a', 'core');
INSERT INTO labels VALUES ('b', 'core');
SELECT id FROM issues WHERE id IN (SELECT issue_id FROM labels WHERE label = 'core') AND issue_type IN ('task') ORDER BY id;
SELECT id FROM issues WHERE id IN (SELECT issue_id FROM labels WHERE label IN ('core')) AND issue_type IN ('task', 'bug') AND status IN ('open') ORDER BY id;
-- case: holdout/subqueries/751-nested-correlated-not-exists-resolves-the-outer-co
CREATE TABLE a(id INTEGER PRIMARY KEY, g);
CREATE TABLE b(id INTEGER PRIMARY KEY, a_id, g);
CREATE TABLE c(id INTEGER PRIMARY KEY, b_id, g);
INSERT INTO a VALUES (1, 'x'), (2, 'y'), (3, 'z');
INSERT INTO b VALUES (10, 1, 'x'), (20, 2, 'q');
INSERT INTO c VALUES (100, 10, 'x'), (200, 20, 'y');
SELECT id FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.a_id = a.id AND NOT EXISTS (SELECT 1 FROM c WHERE c.b_id = b.id AND c.g = a.g)) ORDER BY id;
SELECT id FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.a_id = a.id AND EXISTS (SELECT 1 FROM c WHERE c.b_id = b.id AND c.g = a.g)) ORDER BY id;
-- case: holdout/subqueries/773-row-value-is-and-is-not-compare-components-null-sa
SELECT (1, NULL) IS (1, NULL), (1, NULL) IS NOT (1, NULL), (1, 2) IS (1, NULL), (NULL, NULL) IS (NULL, NULL), (1, 2) IS NOT (1, 3);
SELECT (1, NULL) IS DISTINCT FROM (1, NULL), (1, 2) IS DISTINCT FROM (1, NULL), (1, NULL) IS NOT DISTINCT FROM (1, NULL), (NULL, 1) IS DISTINCT FROM (1, NULL);
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1, NULL), (1, 2), (NULL, NULL), (2, 2);
SELECT a, b FROM t WHERE (a, b) IS DISTINCT FROM (1, NULL) ORDER BY rowid;
SELECT a, b FROM t WHERE (a, b) IS NOT DISTINCT FROM (1, NULL) ORDER BY rowid;
-- case: holdout/subqueries/774-correlated-scalar-subquery-with-order-by-and-limit
CREATE TABLE o(id INTEGER PRIMARY KEY);
CREATE TABLE i(oid INTEGER, v INTEGER);
INSERT INTO o VALUES (1), (2);
INSERT INTO i VALUES (1, 30), (1, 10), (1, 20), (2, 5), (2, 50);
SELECT id, (SELECT v FROM i WHERE i.oid = o.id ORDER BY v ASC LIMIT 1) FROM o;
SELECT id, (SELECT v FROM i WHERE i.oid = o.id ORDER BY v DESC LIMIT 1) FROM o;
SELECT id, (SELECT v FROM i WHERE i.oid = o.id ORDER BY v DESC LIMIT 1 OFFSET 1) FROM o;
-- case: holdout/subqueries/807-row-value-is-distinct-from-is-not-distinct-from-in
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1, NULL), (1, 2), (NULL, NULL), (NULL, 2), (2, NULL);
SELECT a, b FROM t WHERE (a, b) IS DISTINCT FROM (1, NULL) ORDER BY rowid;
SELECT a, b FROM t WHERE (a, b) IS NOT DISTINCT FROM (NULL, NULL) ORDER BY rowid;
SELECT a IS DISTINCT FROM b, a IS NOT DISTINCT FROM b FROM t ORDER BY rowid;
-- case: holdout/subqueries/824-in-select-group-by-having-followed-by-further-pred
CREATE TABLE issues (id TEXT PRIMARY KEY, status TEXT NOT NULL, is_template INTEGER);
CREATE TABLE labels (issue_id TEXT NOT NULL, label TEXT NOT NULL, PRIMARY KEY (issue_id, label));
INSERT INTO issues VALUES ('bd-both', 'open', NULL), ('bd-one', 'open', NULL), ('bd-other', 'open', NULL);
INSERT INTO labels VALUES ('bd-both', 'backend'), ('bd-both', 'urgent'), ('bd-one', 'backend'), ('bd-other', 'urgent');
SELECT COUNT(*) FROM issues WHERE issues.id IN (SELECT issue_id FROM labels WHERE label IN ('backend', 'urgent') GROUP BY issue_id HAVING COUNT(DISTINCT label) = 2);
SELECT COUNT(*) FROM issues WHERE 1=1 AND issues.id IN (SELECT issue_id FROM labels WHERE label IN ('backend', 'urgent') GROUP BY issue_id HAVING COUNT(DISTINCT label) = 2) AND status NOT IN ('closed', 'tombstone', 'deferred') AND (is_template = 0 OR is_template IS NULL);
-- case: holdout/subqueries/825-exists-does-not-evaluate-its-result-list-and-an-ag
CREATE TABLE kx(x, t TEXT);
INSERT INTO kx VALUES (1,'a'),(2,'b'),(5,'e');
CREATE TABLE oc(z INTEGER, w TEXT);
INSERT INTO oc VALUES (1,'a'),(2,'q'),(3,'c'),(NULL,NULL);
SELECT EXISTS (SELECT json('bad') FROM kx);
SELECT z FROM oc WHERE EXISTS (SELECT json('bad') FROM kx WHERE kx.x = oc.z) ORDER BY rowid;
SELECT EXISTS (SELECT abs(-9223372036854775807 - 1) FROM kx);
SELECT z, EXISTS (SELECT count(*) FROM kx WHERE kx.x = oc.z) FROM oc ORDER BY rowid;
SELECT z, EXISTS (SELECT max(t) FROM kx WHERE kx.x = oc.z OR 0) FROM oc ORDER BY rowid;
SELECT z FROM oc WHERE NOT EXISTS (SELECT count(*) FROM kx WHERE kx.x = oc.z) ORDER BY rowid;
