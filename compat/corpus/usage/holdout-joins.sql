-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/joins/20-unqualified-using-column-on-the-left-side-of-a-lef
create table t(a, tb);
create table s(a, sb);
insert into t values (1, 't1'), (2, 't2');
insert into s values (1, 's1'), (3, 's3');
select * from t left join s using(a);
select a, tb, sb from t left join s using(a);
-- case: holdout/joins/25-left-join-with-using-a-b-where-b-is-null-on-both-s
create table if not exists t(a, b);
create table if not exists s(a, b);
insert into t values (1, null), (2, null);
insert into s values (1, null), (2, null);
select a, b from t left join s using (a, b);
delete from t; delete from s;
insert into t values (1, 'a'), (2, 'b');
insert into s values (1, 'a'), (3, 'c');
select a, b from t left join s using (a, b);
select s.a, s.b, t.a, t.b from t left join s using (a, b);
-- case: holdout/joins/27-equijoin-of-integer-column-with-text-column-applie
CREATE TABLE x(a INTEGER);
CREATE TABLE y(b TEXT);
INSERT INTO x VALUES (2),(3);
INSERT INTO y VALUES ('02'),('2'),('3x');
SELECT a, b FROM x JOIN y ON a = b ORDER BY a, b;
-- case: holdout/joins/36-left-join-against-a-subquery-honours-the-on-clause
CREATE TABLE t1(a INTEGER);
INSERT INTO t1 VALUES (1), (2), (3);
CREATE TABLE t2(b INTEGER);
INSERT INTO t2 VALUES (2), (4);
SELECT a, b FROM t1 LEFT JOIN (SELECT * FROM t2 WHERE b > 2) ON t1.a = b;
-- case: holdout/joins/47-join-of-a-column-with-no-affinity-to-a-text-column
CREATE TABLE a(x);
CREATE TABLE b(x TEXT);
INSERT INTO a VALUES (1);
INSERT INTO b VALUES ('1');
SELECT count(*) FROM a JOIN b ON a.x = b.x;
SELECT count(*) FROM a, b WHERE b.x = a.x;
-- case: holdout/joins/72-correlated-scalar-subquery-with-an-inner-join-eval
CREATE TABLE outer_loop (id INTEGER PRIMARY KEY);
CREATE TABLE build_table (id INTEGER PRIMARY KEY, key INTEGER);
CREATE TABLE probe_table (id INTEGER PRIMARY KEY, key INTEGER);
INSERT INTO outer_loop VALUES (1), (2), (3);
INSERT INTO build_table VALUES (1, 1), (2, 2), (3, 3), (4, 4), (5, 5);
INSERT INTO probe_table VALUES (1, 3), (2, 4), (3, 5), (4, 6), (5, 7);
ANALYZE;
SELECT o.id AS outer_id, (SELECT COUNT(*) FROM build_table b JOIN probe_table p ON b.key = p.key WHERE o.id IS NOT NULL) AS matches FROM outer_loop o ORDER BY o.id;
-- case: holdout/joins/81-left-join-of-an-empty-table-with-a-bare-aggregate
CREATE TABLE t(x);
SELECT b.x, COUNT(*) FROM t a LEFT JOIN t b ON a.x=b.x;
INSERT INTO t VALUES (1);
SELECT b.x, COUNT(*) FROM t a LEFT JOIN t b ON a.x=b.x + 1;
-- case: holdout/joins/89-join-probing-a-text-index-with-integer-values-from
CREATE TABLE t_text(id INTEGER PRIMARY KEY, val TEXT);
CREATE INDEX idx_text_val ON t_text(val);
INSERT INTO t_text VALUES (1, '100'), (2, '200');
CREATE TABLE t_int(id INTEGER PRIMARY KEY, val INTEGER);
CREATE INDEX idx_int_val ON t_int(val);
INSERT INTO t_int VALUES (10, 100), (20, 200);
SELECT COUNT(*) FROM t_int i JOIN t_text t ON i.val = t.val;
SELECT COUNT(*) FROM t_text t JOIN t_int i ON t.val = i.val;
SELECT COUNT(*) FROM t_int i JOIN t_text t ON +i.val = t.val;
-- case: holdout/joins/90-join-on-a-nocase-column-against-a-column-with-bina
CREATE TABLE t1(id INTEGER PRIMARY KEY, name TEXT COLLATE NOCASE);
CREATE TABLE t2(id INTEGER PRIMARY KEY, ref_name TEXT, data TEXT);
INSERT INTO t1 VALUES (1, 'Alice'), (2, 'Bob');
INSERT INTO t2 VALUES (1, 'alice', 'x'), (2, 'BOB', 'y');
SELECT t1.name, t2.data FROM t1 JOIN t2 ON t1.name = t2.ref_name ORDER BY t1.id;
SELECT t1.name, t2.data FROM t2 JOIN t1 ON t2.ref_name = t1.name ORDER BY t1.id;
-- case: holdout/joins/97-left-join-turned-into-an-inner-join-only-when-the-
CREATE TABLE v0 (c1, c2);
INSERT INTO v0 VALUES ('a', 1), ('b', 2), ('c', 3);
SELECT count(*) FROM v0 AS a4 LEFT JOIN v0 AS a5 ON (a5.c1 = a5.c2) WHERE ifnull(a5.c2, 2147483647) >= 127;
SELECT count(*) FROM v0 AS a4 LEFT JOIN v0 AS a5 ON (a5.c1 = a5.c2) WHERE a5.c2 >= 127;
SELECT count(*) FROM v0 AS a4 LEFT JOIN v0 AS a5 ON (a5.c1 = a5.c2) WHERE a5.c2 IS NULL;
-- case: holdout/joins/123-left-join-is-not-flattened-when-where-accepts-null
CREATE TABLE t1(id INT);
CREATE TABLE t2(id INT, val INT);
INSERT INTO t1 VALUES (1), (2), (3);
INSERT INTO t2 VALUES (1, 10), (3, 30);
SELECT t1.id, t2.val FROM t1 LEFT JOIN t2 ON t1.id = t2.id WHERE CASE WHEN t2.val IS NULL THEN 1 ELSE t2.val END > 0 ORDER BY t1.id;
SELECT t1.id, t2.val FROM t1 LEFT JOIN t2 ON t1.id = t2.id WHERE iif(t2.val IS NULL, 1, t2.val) > 0 ORDER BY t1.id;
SELECT t1.id, t2.val FROM t1 LEFT JOIN t2 ON t1.id = t2.id WHERE coalesce(t2.val, 1) > 0 ORDER BY t1.id;
SELECT t1.id, t2.val FROM t1 LEFT JOIN t2 ON t1.id = t2.id WHERE t2.val > 0 OR t2.val IS NULL ORDER BY t1.id;
-- case: holdout/joins/125-correlated-scalar-subquery-containing-a-join-is-re
CREATE TABLE t1(id INT);
CREATE TABLE t2(id INT, val INT);
CREATE TABLE t3(val INT, label TEXT);
INSERT INTO t1 VALUES (1),(2),(3);
INSERT INTO t2 VALUES (1,10),(2,20),(3,30);
INSERT INTO t3 VALUES (10,'ten'),(20,'twenty'),(30,'thirty');
SELECT t1.id, (SELECT t3.label FROM t2 JOIN t3 ON t2.val = t3.val WHERE t2.id = t1.id) as label FROM t1 ORDER BY t1.id;
-- case: holdout/joins/132-full-outer-join-with-subquery-predicates-in-where
create table t(x);create table u(x);create table v(x);
insert into t values (1),(2); insert into u values (2),(3); insert into v values (2);
select * from t full outer join u on t.x=u.x where not exists (select * from v where v.x=t.x);
select * from t full outer join u on t.x=u.x where u.x in (select * from v);
-- case: holdout/joins/133-full-outer-join-with-where-conditions-on-either-si
create table t(x);create table u(x);
insert into t values (1),(5); insert into u values (5),(7);
select * from t full outer join u on t.x=u.x where t.x=u.x;
select * from t full outer join u on t.x=u.x where t.x=5;
select * from t full outer join u on t.x=u.x where u.x=5;
select * from t full outer join u on t.x=u.x where u.x is null;
-- case: holdout/joins/152-correlated-subquery-in-a-left-join-on-clause-that-
CREATE TABLE t1(b, c);
INSERT INTO t1 VALUES(1, 0);
CREATE TABLE t2(d);
INSERT INTO t2 VALUES(2);
SELECT t2.d FROM t2 LEFT JOIN t1 ON t2.d IN (SELECT t1.c FROM t1 AS x GROUP BY t2.d HAVING SUM(x.b) > 100) ORDER BY 1;
-- case: holdout/joins/156-full-outer-join-with-a-non-equality-on-condition
CREATE TABLE a(x INT);
CREATE TABLE b(x INT);
INSERT INTO a VALUES(1),(3);
INSERT INTO b VALUES(2),(4);
SELECT ifnull(a.x,'N')||','||ifnull(b.x,'N') FROM a FULL OUTER JOIN b ON a.x < b.x ORDER BY 1;
-- case: holdout/joins/157-select-column-order-with-right-join-followed-by-an
create table a(a1 int); create table b(b1 int); create table c(c1 int);
insert into a values(1); insert into b values(2); insert into c values(3);
select * from a right join b on 1=1 join c on 1=1;
select * from a join b on 1=1 right join c on 1=1;
select * from a right join b on 1=0 left join c on 1=1;
-- case: holdout/joins/176-natural-join-of-tables-with-no-common-column-is-a-
CREATE TABLE t1(a, b);
INSERT INTO t1 VALUES(1, 'x'),(2, 'y');
CREATE TABLE t2(c, d);
INSERT INTO t2 VALUES(1, 'a'),(3, 'b');
SELECT * FROM t1 NATURAL JOIN t2;
SELECT * FROM t1 NATURAL LEFT JOIN t2;
-- case: holdout/joins/183-full-outer-join-over-ctes-derived-tables-and-views
CREATE TABLE t1(a INTEGER);
CREATE TABLE t2(b INTEGER);
INSERT INTO t1 VALUES(1),(2),(3);
INSERT INTO t2 VALUES(2),(3),(4);
SELECT * FROM t1 FULL OUTER JOIN t2 ON t1.a = t2.b ORDER BY coalesce(a, b);
WITH cte1 AS (SELECT 1 AS a UNION ALL SELECT 2 UNION ALL SELECT 3), cte2 AS (SELECT 2 AS b UNION ALL SELECT 3 UNION ALL SELECT 4) SELECT * FROM cte1 FULL OUTER JOIN cte2 ON cte1.a = cte2.b ORDER BY coalesce(a, b);
SELECT * FROM (SELECT a FROM t1) s1 FULL OUTER JOIN (SELECT b FROM t2) s2 ON s1.a = s2.b ORDER BY coalesce(a, b);
CREATE VIEW v1 AS SELECT a FROM t1;
SELECT * FROM v1 FULL JOIN t2 ON v1.a = t2.b ORDER BY coalesce(a, b);
-- case: holdout/joins/184-column-collation-of-the-left-operand-applies-in-a-
CREATE TABLE t1(a INTEGER, b TEXT COLLATE NOCASE);
INSERT INTO t1 VALUES(1,'Apple'),(2,'banana'),(3,'CHERRY');
CREATE TABLE t2(c INTEGER, d TEXT COLLATE BINARY);
INSERT INTO t2 VALUES(1,'apple'),(2,'BANANA'),(3,'cherry');
SELECT t1.b, t2.d, t1.b = t2.d AS eq FROM t1 JOIN t2 ON t1.a = t2.c ORDER BY t1.a;
SELECT t1.b, t2.d, t2.d = t1.b AS eq FROM t1 JOIN t2 ON t1.a = t2.c ORDER BY t1.a;
-- case: holdout/joins/188-cross-join-followed-by-left-join-keeps-every-null-
CREATE TABLE a(id INTEGER PRIMARY KEY, v TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, v TEXT);
CREATE TABLE c(id INTEGER PRIMARY KEY, aid INTEGER, bid INTEGER);
INSERT INTO a VALUES(1, 'a1');
INSERT INTO a VALUES(2, 'a2');
INSERT INTO b VALUES(1, 'b1');
INSERT INTO b VALUES(2, 'b2');
INSERT INTO c VALUES(1, 1, 1);
SELECT a.v, b.v, c.id FROM a CROSS JOIN b LEFT JOIN c ON c.aid = a.id AND c.bid = b.id;
-- case: holdout/joins/191-chained-left-joins-do-not-match-a-null-key-against
CREATE TABLE a(id INTEGER PRIMARY KEY);
CREATE TABLE b(id INTEGER PRIMARY KEY, a_id INTEGER);
CREATE TABLE c(id INTEGER PRIMARY KEY, b_id INTEGER);
INSERT INTO a VALUES (1),(2);
INSERT INTO b VALUES (1,1);
INSERT INTO c VALUES (1,NULL);
CREATE INDEX idx_c_bid ON c(b_id);
SELECT a.id, b.id AS bid, c.id AS cid FROM a LEFT JOIN b ON a.id = b.a_id LEFT JOIN c ON b.id = c.b_id ORDER BY a.id;
-- case: holdout/joins/192-left-join-then-inner-join-then-left-join
CREATE TABLE a(id INT PRIMARY KEY);
CREATE TABLE b(id INT PRIMARY KEY, a_id INT);
CREATE TABLE c(id INT PRIMARY KEY);
CREATE TABLE d(id INT PRIMARY KEY, c_id INT);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(1,1);
INSERT INTO c VALUES(1),(2);
INSERT INTO d VALUES(1,1);
SELECT a.id, b.id AS bid, c.id AS cid, d.id AS did FROM a LEFT JOIN b ON a.id = b.a_id INNER JOIN c ON a.id = c.id LEFT JOIN d ON c.id = d.c_id ORDER BY a.id;
-- case: holdout/joins/193-t1-from-a-right-join-using-returns-the-merged-usin
CREATE TABLE t1(id INT, a TEXT);
CREATE TABLE t2(id INT, b TEXT);
INSERT INTO t1 VALUES(1, 'x'), (2, 'y');
INSERT INTO t2 VALUES(2, 'p'), (3, 'q');
SELECT t1.* FROM t1 RIGHT JOIN t2 USING(id) ORDER BY id;
SELECT * FROM t1 RIGHT JOIN t2 USING(id) ORDER BY id;
SELECT t2.*, id FROM t1 FULL JOIN t2 USING(id) ORDER BY id;
-- case: holdout/joins/201-int-column-compared-with-text-column-using
CREATE TABLE ti (a INT);
CREATE TABLE tt (b TEXT);
INSERT INTO ti VALUES (1),(2),(3),(4),(5),(6);
INSERT INTO tt VALUES ('1'),('2'),('3'),('4'),('5'),('6');
SELECT count(*) FROM ti JOIN tt ON a <= b;
SELECT count(*) FROM ti, tt WHERE ti.a < tt.b;
DELETE FROM ti; DELETE FROM tt;
INSERT INTO ti VALUES (5),(10);
INSERT INTO tt VALUES ('2'),('9'),('20');
SELECT a, b FROM ti JOIN tt ON a >= b ORDER BY a, b;
-- case: holdout/joins/242-three-table-join-inside-a-compound-select-cte-and-
CREATE TABLE t1(x INT); INSERT INTO t1 VALUES(1),(2),(3);
CREATE TABLE t2(y INT); INSERT INTO t2 VALUES(2),(3);
CREATE TABLE t3(z INT); INSERT INTO t3 VALUES(3);
SELECT t1.x FROM t1 JOIN t2 ON t1.x=t2.y JOIN t3 ON t2.y=t3.z UNION ALL SELECT 0;
WITH cte AS (SELECT t1.x FROM t1 JOIN t2 ON t1.x=t2.y JOIN t3 ON t2.y=t3.z) SELECT * FROM cte;
CREATE VIEW v AS SELECT t1.x FROM t1 JOIN t2 ON t1.x=t2.y JOIN t3 ON t2.y=t3.z;
SELECT * FROM v;
-- case: holdout/joins/281-partial-index-must-not-be-used-for-a-left-join-whe
CREATE TABLE t(id INTEGER PRIMARY KEY, c INT, x INT);
CREATE TABLE u(x INT);
CREATE TABLE dst(id INT, c INT, x INT, ux INT);
INSERT INTO t VALUES (1,0,10);
CREATE INDEX idx_t_x_c1 ON t(x) WHERE c = 1;
INSERT INTO dst SELECT t.id, t.c, t.x, u.x FROM t LEFT JOIN u ON t.c = 1 AND u.x = t.x WHERE t.x IN (10);
SELECT * FROM dst ORDER BY id;
PRAGMA integrity_check;
-- case: holdout/joins/307-aggregate-with-a-bare-column-from-a-joined-table-o
CREATE TABLE t (a);
SELECT y.a, COUNT(*) FROM t AS x JOIN t AS y ON x.a = y.a;
SELECT y.a, COUNT(*) FROM t AS x LEFT JOIN t AS y ON x.a = y.a;
SELECT x.a, COUNT(*) FROM t AS x LEFT JOIN t AS y ON x.a = y.a;
SELECT x.a, max(y.a), min(x.a), total(x.a) FROM t AS x, t AS y;
-- case: holdout/joins/309-left-join-against-a-derived-join-whose-on-has-or-0
CREATE TABLE l(k INTEGER);
CREATE TABLE rr(k INTEGER);
INSERT INTO l VALUES (0),(1);
INSERT INTO rr VALUES (0),(1);
SELECT a.k, d.k FROM l AS a LEFT JOIN (SELECT b.k FROM l AS b JOIN rr AS c ON b.k = c.k) AS d ON (a.k >= d.k) OR 0 ORDER BY a.k, d.k;
SELECT a.k, d.k FROM l AS a LEFT JOIN (SELECT b.k FROM l AS b JOIN rr AS c ON b.k = c.k) AS d ON (a.k >= d.k) ORDER BY a.k, d.k;
-- case: holdout/joins/310-left-join-over-a-derived-join-with-a-negated-compa
CREATE TABLE l(k INTEGER);
CREATE TABLE a(id INTEGER, r INTEGER);
CREATE TABLE b(r INTEGER);
INSERT INTO l VALUES (1),(2);
INSERT INTO a VALUES (1,0),(2,1);
INSERT INTO b VALUES (0),(1);
SELECT l.k, d.id FROM l LEFT JOIN (SELECT a.id FROM a JOIN b ON a.r = b.r) AS d ON NOT (l.k <= d.id) ORDER BY l.k, d.id;
-- case: holdout/joins/311-join-of-derived-tables-with-on-l-id-r-k-or-l-id-r-
CREATE TABLE l(id INTEGER);
CREATE TABLE r(k INTEGER);
CREATE TABLE a(k INTEGER);
INSERT INTO l VALUES (1), (2);
INSERT INTO r VALUES (2);
INSERT INTO a VALUES (2);
SELECT l.id FROM (SELECT DISTINCT id FROM l) AS l JOIN (SELECT r.k FROM r JOIN a ON r.k = a.k) AS r ON (l.id < r.k) OR (l.id = r.k) ORDER BY 1;
-- case: holdout/joins/312-join-of-derived-tables-with-on-not-l-x-r-y
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(y INTEGER);
CREATE TABLE ra(y INTEGER);
INSERT INTO l VALUES (1), (2);
INSERT INTO la VALUES (1), (2);
INSERT INTO r VALUES (3);
INSERT INTO ra VALUES (3);
SELECT l.x FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l JOIN (SELECT r.y FROM r JOIN ra ON r.y = ra.y) AS r ON NOT (l.x >= r.y) ORDER BY 1;
-- case: holdout/joins/313-right-join-whose-on-is-a-coalesce-of-a-comparison
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(id INTEGER, y INTEGER);
INSERT INTO l VALUES (1);
INSERT INTO la VALUES (1);
INSERT INTO r VALUES (10, 2), (11, 3);
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON COALESCE((l.x <= r.y), 0) WHERE l.x IS NOT NULL ORDER BY 1, 2;
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON COALESCE((l.x <= r.y), 0) ORDER BY 2;
-- case: holdout/joins/314-join-on-l-k-r-id-compared-with-the-same-predicate-
CREATE TABLE l(k INTEGER, x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(id INTEGER PRIMARY KEY);
CREATE TABLE ra(k INTEGER);
INSERT INTO l VALUES (-1, 1);
INSERT INTO la VALUES (1);
INSERT INTO r VALUES (1), (2);
INSERT INTO ra VALUES (1), (2);
SELECT 'direct', l.k, r.id FROM (SELECT l.k FROM l JOIN la ON l.x = la.x) AS l JOIN (SELECT r.id FROM r JOIN ra ON r.id = ra.k) AS r ON l.k < r.id UNION ALL SELECT 'is_true', l.k, r.id FROM (SELECT l.k FROM l JOIN la ON l.x = la.x) AS l JOIN (SELECT r.id FROM r JOIN ra ON r.id = ra.k) AS r ON (l.k < r.id) IS TRUE ORDER BY 1, 2, 3;
-- case: holdout/joins/315-right-join-whose-on-contains-a-correlated-in-subqu
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(id INTEGER, x INTEGER);
INSERT INTO l VALUES (1);
INSERT INTO la VALUES (1);
INSERT INTO r VALUES (10, 1), (11, 1);
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON 1 IN (SELECT 1 WHERE l.x = r.x) WHERE l.x IS NOT NULL ORDER BY 1, 2;
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON EXISTS (SELECT 1 WHERE l.x = r.x) WHERE l.x IS NOT NULL ORDER BY 1, 2;
-- case: holdout/joins/316-left-join-whose-on-is-a-correlated-exists
CREATE TABLE l(id INTEGER);
CREATE TABLE r(id INTEGER, v INTEGER);
CREATE TABLE a(v INTEGER);
INSERT INTO l VALUES (1), (2);
INSERT INTO r VALUES (2, 0);
INSERT INTO a VALUES (0);
SELECT 'direct', l.id, d.id FROM l LEFT JOIN (SELECT r.id FROM r JOIN a ON r.v = a.v) AS d ON l.id = d.id UNION ALL SELECT 'exists', l.id, d.id FROM l LEFT JOIN (SELECT r.id FROM r JOIN a ON r.v = a.v) AS d ON EXISTS (SELECT 1 WHERE l.id = d.id) ORDER BY 1, 2, 3;
-- case: holdout/joins/317-cross-join-against-a-derived-table-that-is-itself-
CREATE TABLE l(x INTEGER);
CREATE TABLE r(y INTEGER);
CREATE TABLE a(y INTEGER);
INSERT INTO l VALUES (1),(2);
INSERT INTO r VALUES (10),(20);
INSERT INTO a VALUES (10),(20);
SELECT COUNT(*) FROM l CROSS JOIN (SELECT r.y FROM r JOIN a ON r.y = a.y) AS d;
-- case: holdout/joins/318-natural-join-on-a-quoted-column-name-that-contains
CREATE TABLE t("NUMERIC-ish" NUMERIC);
INSERT INTO t VALUES (1);
SELECT * FROM t AS a NATURAL JOIN t AS b;
SELECT "NUMERIC-ish" FROM t AS a NATURAL JOIN t AS b;
-- case: holdout/joins/319-text-range-join-with-mixed-collations-compared-aga
CREATE TABLE l(id INTEGER PRIMARY KEY, txt TEXT, rr REAL);
CREATE UNIQUE INDEX l_txt ON l(txt);
CREATE TABLE r(id INTEGER PRIMARY KEY, txt TEXT COLLATE NOCASE, k INTEGER, bucket INTEGER);
INSERT INTO l VALUES (10, '', -1.0e308), (12, 'a', 0.0);
INSERT INTO r VALUES (1, '', 0, 0), (2, 'A', 0, 1), (3, 'a', 1, 1);
SELECT l.id, r.id FROM l JOIN r ON l.txt <= r.txt ORDER BY 1, 2;
SELECT l.id, r.id FROM l CROSS JOIN r WHERE l.txt <= r.txt ORDER BY 1, 2;
ANALYZE;
SELECT l.id, r.id FROM l JOIN r ON l.txt <= r.txt ORDER BY 1, 2;
SELECT l.id, r.id FROM l JOIN r ON r.txt >= l.txt ORDER BY 1, 2;
-- case: holdout/joins/320-text-column-compared-with-an-integer-column-throug
CREATE TABLE l(txt TEXT);
CREATE TABLE r(flag INTEGER);
INSERT INTO l VALUES ('');
INSERT INTO r VALUES (0);
SELECT 'inner', L.txt, R.flag FROM (SELECT txt FROM l) AS L JOIN r AS R ON L.txt <= R.flag UNION ALL SELECT 'cross', L.txt, R.flag FROM (SELECT txt FROM l) AS L CROSS JOIN r AS R WHERE L.txt <= R.flag ORDER BY 1, 2, 3;
SELECT l.txt <= r.flag, l.txt >= r.flag, l.txt = r.flag FROM l, r;
-- case: holdout/joins/321-right-join-on-text-boolean-column
CREATE TABLE l(id INTEGER PRIMARY KEY, txt TEXT);
CREATE TABLE r(id INTEGER PRIMARY KEY, flag BOOLEAN);
INSERT INTO l VALUES (1, '6');
INSERT INTO r VALUES (10, 0);
SELECT l.id, l.txt, typeof(l.txt), r.id, r.flag, typeof(r.flag) FROM (SELECT id, txt FROM l WHERE txt IS NOT NULL) AS l RIGHT JOIN (SELECT DISTINCT id, flag FROM r) AS r ON l.txt >= r.flag;
SELECT l.id, r.id FROM l, r WHERE l.txt >= r.flag;
-- case: holdout/joins/322-not-exists-anti-filter-with-a-derived-table-alias-
CREATE TABLE l(id INTEGER PRIMARY KEY, k INTEGER, bucket INTEGER, txt TEXT, flag INTEGER);
CREATE TABLE r(id INTEGER PRIMARY KEY, k INTEGER, bucket INTEGER, txt TEXT, flag INTEGER);
INSERT INTO l VALUES (1, 1, 1, 'x', 0);
INSERT INTO r VALUES (1, 1, 1, 'x', 0), (2, 1, 1, 'y', 0);
SELECT 'anti', L.id FROM (SELECT rb.id, rb.k, rb.bucket, rb.txt, rb.flag FROM r AS rb JOIN l AS la ON rb.bucket = la.id) AS L WHERE NOT EXISTS (SELECT 1 FROM l AS R WHERE L.flag > R.txt) ORDER BY 2;
SELECT 'left', L.id FROM (SELECT rb.id, rb.k, rb.bucket, rb.txt, rb.flag FROM r AS rb JOIN l AS la ON rb.bucket = la.id) AS L LEFT JOIN l AS R ON L.flag > R.txt WHERE R.id IS NULL ORDER BY 2;
-- case: holdout/joins/323-range-comparison-uses-the-collation-of-the-left-op
CREATE TABLE l(id INTEGER, txt TEXT);
CREATE TABLE r(id INTEGER, txt TEXT COLLATE NOCASE);
INSERT INTO l VALUES (2, 'a');
INSERT INTO r VALUES (11, 'A');
SELECT 'anti', l.id FROM l WHERE NOT EXISTS (SELECT 1 FROM r WHERE l.txt > r.txt) UNION ALL SELECT 'left', l.id FROM l LEFT JOIN (SELECT r.*, 1 AS marker FROM r) AS r ON l.txt > r.txt WHERE r.marker IS NULL ORDER BY 1, 2;
SELECT l.txt > r.txt, r.txt < l.txt, r.txt > l.txt FROM l, r;
-- case: holdout/joins/336-on-clause-refers-to-a-table-that-appears-later-in-
CREATE TABLE a(x INT);
CREATE TABLE b(y INT);
CREATE TABLE c(z INT);
INSERT INTO a VALUES (1);
INSERT INTO b VALUES (1);
INSERT INTO c VALUES (1);
SELECT * FROM a JOIN b ON a.x = c.z JOIN c ON b.y = c.z;
SELECT * FROM a JOIN c ON a.x = c.z JOIN b ON b.y = c.z;
SELECT * FROM a LEFT JOIN b ON a.x = c.z LEFT JOIN c ON b.y = c.z;
-- case: holdout/joins/340-right-and-full-outer-join-with-on-0-on-true-and-us
CREATE TABLE t0(a INT); INSERT INTO t0(a) VALUES (1);
CREATE TABLE t1(b INT); INSERT INTO t1(b) VALUES (2);
CREATE VIEW v2(c) AS SELECT 3 FROM t1;
SELECT * FROM t1 JOIN v2 ON 0 FULL OUTER JOIN t0 ON true;
CREATE TABLE s1(c0 INT, c1 INT); INSERT INTO s1(c0, c1) VALUES(NULL,11);
CREATE TABLE s2(c0 INT NOT NULL);
CREATE TABLE s3(x INT); INSERT INTO s3(x) VALUES(3);
SELECT * FROM s2 RIGHT JOIN s3 ON true LEFT JOIN s1 USING(c0);
CREATE TABLE a(value TEXT); INSERT INTO a(value) VALUES ('a'),('b'),(NULL);
CREATE TABLE b(value TEXT); INSERT INTO b(value) VALUES ('a'),('c'),(NULL);
SELECT a.value, b.value FROM a RIGHT JOIN b ON a.value = b.value ORDER BY 1, 2;
SELECT a.value, b.value FROM a FULL JOIN b ON a.value = b.value ORDER BY 1, 2;
-- case: holdout/joins/341-a-join-of-64-tables-is-accepted-65-is-rejected
CREATE TABLE t14(x);
INSERT INTO t14 VALUES('abcdefghij');
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 64) SELECT group_concat('t14', ', ') FROM n;
SELECT 1 FROM t14 a1, t14 a2, t14 a3, t14 a4, t14 a5, t14 a6, t14 a7, t14 a8, t14 a9, t14 a10, t14 a11, t14 a12, t14 a13, t14 a14, t14 a15, t14 a16, t14 a17, t14 a18, t14 a19, t14 a20, t14 a21, t14 a22, t14 a23, t14 a24, t14 a25, t14 a26, t14 a27, t14 a28, t14 a29, t14 a30, t14 a31, t14 a32, t14 a33, t14 a34, t14 a35, t14 a36, t14 a37, t14 a38, t14 a39, t14 a40, t14 a41, t14 a42, t14 a43, t14 a44, t14 a45, t14 a46, t14 a47, t14 a48, t14 a49, t14 a50, t14 a51, t14 a52, t14 a53, t14 a54, t14 a55, t14 a56, t14 a57, t14 a58, t14 a59, t14 a60, t14 a61, t14 a62, t14 a63, t14 a64;
SELECT 1 FROM t14 a1, t14 a2, t14 a3, t14 a4, t14 a5, t14 a6, t14 a7, t14 a8, t14 a9, t14 a10, t14 a11, t14 a12, t14 a13, t14 a14, t14 a15, t14 a16, t14 a17, t14 a18, t14 a19, t14 a20, t14 a21, t14 a22, t14 a23, t14 a24, t14 a25, t14 a26, t14 a27, t14 a28, t14 a29, t14 a30, t14 a31, t14 a32, t14 a33, t14 a34, t14 a35, t14 a36, t14 a37, t14 a38, t14 a39, t14 a40, t14 a41, t14 a42, t14 a43, t14 a44, t14 a45, t14 a46, t14 a47, t14 a48, t14 a49, t14 a50, t14 a51, t14 a52, t14 a53, t14 a54, t14 a55, t14 a56, t14 a57, t14 a58, t14 a59, t14 a60, t14 a61, t14 a62, t14 a63, t14 a64, t14 a65;
-- case: holdout/joins/346-where-on-a-table-before-a-full-join-is-applied-aft
CREATE TABLE a(id INTEGER, x INTEGER);
CREATE TABLE b(id INTEGER, y INTEGER);
CREATE TABLE c(id INTEGER, z INTEGER);
INSERT INTO a VALUES (1,1),(2,2);
INSERT INTO b VALUES (1,1),(2,2);
INSERT INTO c VALUES (2,2),(3,3);
SELECT a.id, c.id FROM a JOIN b ON a.id = b.id FULL JOIN c ON b.id = c.id WHERE a.x = 1 ORDER BY 1, 2;
CREATE TABLE i(k INTEGER, amt INTEGER);
INSERT INTO i VALUES (1,1),(2,99);
SELECT a.id, c.id FROM a JOIN b ON a.id = b.id FULL JOIN c ON b.id = c.id WHERE a.x IN (SELECT i.amt FROM i WHERE i.k = a.x) ORDER BY 1, 2;
SELECT a.id, b.id, c.id FROM a JOIN b ON a.id = b.id FULL JOIN c ON b.id = c.id ORDER BY 1, 2, 3;
-- case: holdout/joins/351-correlated-scalar-subqueries-inside-a-three-table-
CREATE TABLE a(id INTEGER, k INTEGER);
CREATE TABLE b(id INTEGER, k INTEGER, j INTEGER);
CREATE TABLE c(id INTEGER, j INTEGER, k INTEGER);
CREATE TABLE i(k INTEGER, x INTEGER);
INSERT INTO a VALUES (1,1);
INSERT INTO b VALUES (1,1,1);
INSERT INTO c VALUES (1,1,2);
INSERT INTO i VALUES (2,10),(2,3);
SELECT a.id, (SELECT max(x) FROM i WHERE i.k = c.k) FROM a JOIN b ON a.k = b.k JOIN c ON c.j = b.j;
SELECT a.id, (SELECT sum(x) FROM i WHERE i.k = c.k), (SELECT count(*) FROM i WHERE i.k = b.k) FROM c, b, a WHERE a.k = b.k AND c.j = b.j;
-- case: holdout/joins/352-not-in-subquery-is-not-null-rejecting-for-the-left
CREATE TABLE pa(id INTEGER);
CREATE TABLE pb(aid INTEGER, x INTEGER);
CREATE TABLE z(a INTEGER);
INSERT INTO pa VALUES (1),(2);
INSERT INTO pb VALUES (1,10);
SELECT pa.id FROM pa LEFT JOIN pb ON pa.id = pb.aid WHERE pb.x NOT IN (SELECT a FROM z) ORDER BY 1;
INSERT INTO z VALUES (10);
SELECT pa.id FROM pa LEFT JOIN pb ON pa.id = pb.aid WHERE pb.x NOT IN (SELECT a FROM z WHERE a > 100) ORDER BY 1;
SELECT pa.id FROM pa LEFT JOIN pb ON pa.id = pb.aid WHERE pb.aid NOT IN (SELECT a FROM z WHERE a > 100) ORDER BY 1;
SELECT pa.id, pb.x, pb.x NOT IN (SELECT a FROM z) FROM pa LEFT JOIN pb ON pa.id = pb.aid ORDER BY 1;
SELECT NULL IN (SELECT a FROM z WHERE 0), NULL NOT IN (SELECT a FROM z WHERE 0), NULL IN (), NULL NOT IN ();
-- case: holdout/joins/353-full-join-after-a-cross-join-unmatched-right-rows-
CREATE TABLE a(k INTEGER);
CREATE TABLE b(z INTEGER);
CREATE TABLE c(y INTEGER);
INSERT INTO a VALUES (7);
INSERT INTO b VALUES (100);
INSERT INTO c VALUES (500);
SELECT a.k, b.z, c.y FROM a CROSS JOIN b FULL JOIN c ON c.y = a.k ORDER BY 1,2,3;
INSERT INTO a VALUES (8);
INSERT INTO b VALUES (200);
SELECT a.k, b.z, c.y FROM a CROSS JOIN b FULL JOIN c ON c.y = a.k ORDER BY 1,2,3;
-- case: holdout/joins/355-left-join-is-kept-when-where-reaches-the-right-tab
CREATE TABLE o(a, c);
CREATE TABLE p(a);
CREATE TABLE i(b);
INSERT INTO o VALUES (1, 5);
INSERT INTO p VALUES (2);
SELECT o.a FROM o LEFT JOIN p ON o.a = p.a WHERE o.c > (SELECT count(*) FROM i WHERE i.b = p.a);
SELECT o.a, p.a, (SELECT count(*) FROM i WHERE i.b = p.a) FROM o LEFT JOIN p ON o.a = p.a;
-- case: holdout/joins/356-left-join-with-a-where-equality-that-spans-both-ta
CREATE TABLE o(a);
CREATE TABLE p(a, b);
INSERT INTO o VALUES (0);
INSERT INTO p VALUES (2, 2);
SELECT o.a, p.a, p.b FROM o LEFT JOIN p ON o.a = p.a WHERE coalesce(p.b, 1) = o.a;
SELECT o.a, p.b, coalesce(p.b, 1), coalesce(p.b, 1) = o.a FROM o LEFT JOIN p ON o.a = p.a;
-- case: holdout/joins/357-left-join-is-not-converted-to-inner-by-a-where-ter
CREATE TABLE l(a);
CREATE TABLE r(a, b);
INSERT INTO l VALUES (1);
INSERT INTO r VALUES (2, 7);
SELECT l.a, r.b FROM l LEFT JOIN r ON l.a = r.a WHERE typeof(r.b) = 'null';
SELECT l.a, r.b, typeof(r.b), quote(r.b), (r.b IS NOT NULL) FROM l LEFT JOIN r ON l.a = r.a;
SELECT l.a FROM l LEFT JOIN r ON l.a = r.a WHERE r.b IS NOT 5;
SELECT l.a FROM l LEFT JOIN r ON l.a = r.a WHERE coalesce(r.b, 0) = 0;
SELECT l.a FROM l LEFT JOIN r ON l.a = r.a WHERE r.b IS NULL OR r.b > 100;
-- case: holdout/joins/358-parenthesized-join-in-the-from-clause
CREATE TABLE t1(a); CREATE TABLE t2(a); CREATE TABLE t3(a);
INSERT INTO t1 VALUES (1); INSERT INTO t2 VALUES (1); INSERT INTO t3 VALUES (1);
SELECT * FROM t1 JOIN (t2 JOIN t3 ON t2.a = t3.a) ON t1.a = t2.a;
SELECT * FROM (t1) JOIN ((t2)) ON t1.a = t2.a;
SELECT * FROM (t1, t2, t3);
SELECT * FROM t1 LEFT JOIN (t2 JOIN t3 USING(a)) USING(a);
-- case: holdout/joins/359-correlated-subquery-in-a-left-join-on-clause-that-
CREATE TABLE t1(a); CREATE TABLE t2(a, b); CREATE TABLE t3(a); CREATE TABLE s(k);
INSERT INTO t1 VALUES (2); INSERT INTO t2 VALUES (2, 2);
INSERT INTO t3 VALUES (2); INSERT INTO s VALUES (2);
SELECT t1.a, t2.a, t3.a FROM t1 JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a AND EXISTS (SELECT 1 FROM s WHERE s.k = t1.a);
SELECT t1.a, t2.a, t3.a FROM t1 JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a AND (SELECT count(*) FROM s WHERE t1.a IS NULL) > 0;
-- case: holdout/joins/360-chained-left-joins-with-not-exists-in-where-when-t
CREATE TABLE t1(a); CREATE TABLE t2(a, b); CREATE TABLE t3(a); CREATE TABLE s(k);
INSERT INTO t1 VALUES (1); INSERT INTO t2 VALUES (9, 9);
INSERT INTO t3 VALUES (9); INSERT INTO s VALUES (9);
SELECT t1.a, t2.a, t3.a FROM t1 LEFT JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a WHERE NOT EXISTS (SELECT 1 FROM s WHERE s.k = t3.a);
INSERT INTO t1 VALUES (5); INSERT INTO t2 VALUES (5, 4); INSERT INTO t3 VALUES (4);
SELECT t1.a, t2.a, t3.a FROM t1 LEFT JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a WHERE NOT EXISTS (SELECT 1 FROM s WHERE s.k = t3.a) ORDER BY 1;
-- case: holdout/joins/378-full-outer-join-when-both-join-columns-have-an-ind
create table t1(a);
create table t2(a);
create index idx on t1(a);
create index idx2 on t2(a);
insert into t1 values (1),(2);
insert into t2 values (2),(3);
select * from t1 full outer join t2 on t1.a = t2.a order by coalesce(t1.a, t2.a);
-- case: holdout/joins/394-expression-index-plus-a-join-on-another-column
CREATE TABLE a(x, k);
INSERT INTO a VALUES(5, 1);
CREATE TABLE b(k);
INSERT INTO b VALUES(1);
SELECT a.x FROM a, b WHERE a.k = b.k AND abs(a.x) = 5;
CREATE INDEX ix ON a(abs(x));
SELECT a.x FROM a, b WHERE a.k = b.k AND abs(a.x) = 5;
SELECT a.x FROM a WHERE abs(a.x) = 5;
CREATE TABLE c(k INTEGER PRIMARY KEY);
INSERT INTO c VALUES(1);
SELECT a.x FROM a, c WHERE a.k = c.k AND abs(a.x) = 5;
-- case: holdout/joins/395-order-by-on-a-join-whose-lookup-column-has-no-inde
CREATE TABLE a(id INTEGER PRIMARY KEY, k INTEGER);
INSERT INTO a VALUES (1,10),(2,20),(3,30);
CREATE TABLE b(k INTEGER);
INSERT INTO b VALUES (30),(10),(20);
SELECT a.id FROM a JOIN b ON b.k=a.k WHERE a.id>0 ORDER BY a.id;
SELECT a.id FROM a JOIN b ON b.k=a.k WHERE a.id>0 ORDER BY a.id DESC;
SELECT a.id, row_number() OVER (ORDER BY a.id) FROM a JOIN b ON b.k=a.k WHERE a.id>0 ORDER BY 1;
-- case: holdout/joins/401-nested-left-join-chain-with-is-comparison-and-inne
CREATE TABLE t1(id INTEGER PRIMARY KEY, b INT, c INT);
CREATE TABLE t2(id INTEGER PRIMARY KEY, b INT, c INT);
CREATE TABLE t3(id INTEGER PRIMARY KEY, b INT, d INT);
CREATE TABLE t4(id INTEGER PRIMARY KEY, b INT, d INT);
INSERT INTO t1 VALUES(1, 7, 8);
INSERT INTO t3 VALUES(3, 1, 2);
INSERT INTO t4 VALUES(4, 1, 2);
SELECT t1.id, t2.id, t3.id, t4.id FROM t1 LEFT JOIN t2 ON (t1.b = t2.b AND t1.c = t2.c) LEFT JOIN t3 ON t2.b IS t3.b JOIN t4 ON t3.b = t4.b AND t3.d = t4.d WHERE t3.b IS NOT NULL ORDER BY t1.id, t2.id, t3.id, t4.id LIMIT 50;
-- case: holdout/joins/402-join-of-a-table-with-a-large-blob-column-and-a-sel
CREATE TABLE b AS SELECT zeroblob(8185) AS x;
CREATE TABLE a AS SELECT * FROM b UNION ALL SELECT * FROM b;
SELECT count(*) FROM a AS outer_row, a JOIN b ON a.x = b.x;
SELECT outer_row.rowid, count(*) FROM a AS outer_row, a JOIN b ON a.x = b.x GROUP BY outer_row.rowid ORDER BY outer_row.rowid;
-- case: holdout/joins/403-planner-prefers-the-primary-key-index-for-a-join-w
CREATE TABLE t (a, b, c, PRIMARY KEY (a, b));
CREATE INDEX t_a_c ON t (a, c);
CREATE TABLE u (a, b);
CREATE INDEX u_a ON u (a);
INSERT INTO t VALUES ('x','1','y'),('x','2','z');
INSERT INTO u VALUES ('x','1'),('x','2');
SELECT count(*) FROM u JOIN t ON t.a = u.a AND t.b = u.b WHERE u.a = 'x' AND t.c = 'y';
SELECT count(*) FROM u WHERE u.a = 'x' AND (SELECT count(*) FROM t WHERE t.a = u.a AND t.b = u.b AND t.c = 'y') > 0;
-- case: holdout/joins/410-partial-index-with-two-joins
CREATE TABLE t(id INTEGER PRIMARY KEY, d TEXT, k INTEGER, c TEXT);
CREATE TABLE u(id INTEGER PRIMARY KEY, k INTEGER, d TEXT, c TEXT);
CREATE TABLE v(k INTEGER);
INSERT INTO u(id) VALUES(7);
INSERT INTO t VALUES(1,NULL,7,'a'),(2,NULL,7,'b'),(3,NULL,7,'c');
CREATE INDEX i ON t(k,c) WHERE d IS NULL;
SELECT t.id FROM t JOIN u ON t.k=u.id LEFT JOIN v ON v.k=t.k WHERE t.d IS NULL;
DELETE FROM t WHERE id NOT IN (SELECT t.id FROM t JOIN u ON t.k=u.id LEFT JOIN v ON v.k=t.k WHERE t.d IS NULL);
SELECT count(*) FROM t;
-- case: holdout/joins/415-group-by-over-a-join-with-an-in-list-on-the-join-c
CREATE TABLE a(id INTEGER PRIMARY KEY, k INTEGER);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INTEGER);
CREATE TABLE c(k INTEGER, n INTEGER);
INSERT INTO a VALUES(1,3),(2,4);
INSERT INTO c VALUES(3,10),(4,30),(3,10);
SELECT a.id, count(*) FROM a JOIN c ON c.k=a.k WHERE a.k IN (3,4) GROUP BY a.id;
INSERT INTO b SELECT a.id, sum(c.n) FROM a JOIN c ON c.k=a.k WHERE a.k IN (3,4) GROUP BY a.id;
SELECT * FROM b ORDER BY id;
-- case: holdout/joins/418-correlated-exists-over-a-three-table-join
CREATE TABLE t(id INTEGER PRIMARY KEY);
CREATE TABLE u(id INTEGER, k TEXT);
CREATE TABLE v(k TEXT, x INTEGER);
INSERT INTO t VALUES(1),(2),(3);
INSERT INTO u VALUES(2,'a'),(3,'b');
INSERT INTO v VALUES('a',1),('b',1);
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM u JOIN v ON v.k=u.k JOIN v v2 ON v2.k=u.k WHERE u.id=t.id) ORDER BY id;
DELETE FROM t WHERE NOT EXISTS (SELECT 1 FROM u JOIN v ON v.k=u.k JOIN v v2 ON v2.k=u.k WHERE u.id=t.id);
SELECT count(*) FROM t;
-- case: holdout/joins/419-correlated-subquery-over-a-join-keeps-its-rowid-co
CREATE TABLE t(a INTEGER PRIMARY KEY, g, n);
CREATE TABLE u(g);
CREATE TABLE r(k);
INSERT INTO t VALUES(1,'p',1),(2,'q',2);
INSERT INTO u VALUES('p'),('q');
INSERT INTO r VALUES(1),(2),(3);
SELECT k, (SELECT count(*) FROM t JOIN u ON u.g=t.g WHERE t.a=r.k) FROM r ORDER BY k;
DELETE FROM r WHERE (SELECT count(*) FROM t JOIN u ON u.g=t.g WHERE t.a=r.k) = 0;
SELECT k FROM r ORDER BY k;
-- case: holdout/joins/421-correlated-subquery-over-a-left-joined-nocase-colu
CREATE TABLE o(id INTEGER, s TEXT, k TEXT);
CREATE TABLE l(s TEXT);
CREATE TABLE c(s TEXT COLLATE NOCASE, j TEXT, p INTEGER);
INSERT INTO o VALUES(1,'ab','AB');
INSERT INTO l VALUES('ab');
INSERT INTO c VALUES('AB','AB',10);
SELECT x.id, c.p, (SELECT count(*) FROM o WHERE o.k = c.s) FROM o AS x JOIN l ON l.s = x.s JOIN o AS g ON g.s = l.s LEFT JOIN c ON c.s = g.s;
-- case: holdout/joins/424-update-with-a-correlated-three-table-subquery-eval
CREATE TABLE t(id INTEGER PRIMARY KEY, n INTEGER DEFAULT 0);
CREATE TABLE u(cid INTEGER, k TEXT);
CREATE TABLE v(k TEXT, a INTEGER);
CREATE TABLE w(a INTEGER);
INSERT INTO t(id) VALUES(1),(2),(3),(4);
INSERT INTO u VALUES(1,'a'),(1,'a'),(2,'b'),(3,'a');
INSERT INTO v VALUES('a',1),('b',1);
INSERT INTO w VALUES(1);
UPDATE t SET n = (SELECT count(*) FROM u JOIN v ON v.k=u.k JOIN w ON w.a=v.a WHERE u.cid=t.id);
SELECT group_concat(n) FROM (SELECT n FROM t ORDER BY id);
-- case: holdout/joins/429-recursive-cte-whose-recursive-arm-joins-two-tables
CREATE TABLE t(id INTEGER, p INTEGER);
CREATE TABLE s(id INTEGER);
INSERT INTO t VALUES(2,1),(3,2),(4,3),(5,4);
INSERT INTO s VALUES(2),(3),(4),(5);
WITH RECURSIVE r(id) AS (SELECT 1 UNION SELECT t.id FROM r JOIN t ON t.p=r.id JOIN s ON s.id=t.id) SELECT group_concat(id) FROM (SELECT id FROM r ORDER BY id);
CREATE INDEX i ON s(id) WHERE id>0;
WITH RECURSIVE r(id) AS (SELECT 1 UNION SELECT t.id FROM r JOIN t ON t.p=r.id JOIN s ON s.id=t.id) SELECT group_concat(id) FROM (SELECT id FROM r ORDER BY id);
-- case: holdout/joins/430-offset-over-a-cross-join-skips-result-rows
CREATE TABLE a(k INTEGER PRIMARY KEY);
CREATE TABLE b(k INTEGER PRIMARY KEY);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1),(2);
SELECT a.k,b.k FROM a,b ORDER BY a.k,b.k LIMIT 10 OFFSET 2;
CREATE TABLE d(x,y);
INSERT INTO d SELECT a.k,b.k FROM a,b ORDER BY a.k,b.k LIMIT 10 OFFSET 2;
SELECT count(*) FROM d;
SELECT a.k,b.k FROM a,b LIMIT 3 OFFSET 4;
-- case: holdout/joins/431-two-indexes-on-the-left-table-of-a-left-join-with-
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT, g INTEGER);
INSERT INTO t VALUES(1,'a',1),(2,'b',1);
CREATE TABLE u(uid INTEGER);
INSERT INTO u VALUES(1);
CREATE INDEX i1 ON t(s);
CREATE INDEX i2 ON t(g);
SELECT t.id FROM t LEFT JOIN u ON u.uid=t.id AND t.s='a' WHERE t.g>0 ORDER BY t.id;
DELETE FROM t WHERE id NOT IN (SELECT t.id FROM t LEFT JOIN u ON u.uid=t.id AND t.s='a' WHERE t.g>0);
SELECT count(*) FROM t;
-- case: holdout/joins/438-correlated-predicate-with-the-outer-column-on-the-
CREATE TABLE t(id INT);
CREATE TABLE u(id INT, v INT);
CREATE TABLE w(v INT, x TEXT);
INSERT INTO t VALUES(1),(2),(3);
INSERT INTO u VALUES(1,10),(2,20),(3,30);
INSERT INTO w VALUES(10,'a'),(20,'b'),(30,'c');
SELECT t.id, (SELECT w.x FROM u JOIN w ON u.v = w.v WHERE t.id = u.id) FROM t ORDER BY t.id;
SELECT t.id, (SELECT count(*) FROM u JOIN w ON u.v = w.v WHERE t.id > u.id) FROM t ORDER BY t.id;
SELECT t.id, (SELECT w.x FROM u JOIN w ON u.v = w.v WHERE u.id = t.id) FROM t ORDER BY t.id;
-- case: holdout/joins/448-expression-index-under-a-left-join-with-group-by-a
CREATE TABLE t(id INTEGER PRIMARY KEY, x TEXT);
CREATE TABLE u(k INTEGER);
INSERT INTO t VALUES(1,'a'),(2,'b');
INSERT INTO u VALUES(1),(2);
CREATE INDEX i ON t(lower(x));
SELECT lower(t.x), count(*) FROM t LEFT JOIN u ON u.k=t.id GROUP BY lower(t.x);
SELECT quote(lower(t.x)) FROM t LEFT JOIN u ON u.k=t.id ORDER BY lower(t.x);
-- case: holdout/joins/449-offset-on-an-unindexed-left-join
CREATE TABLE t(id INTEGER PRIMARY KEY, g TEXT);
CREATE TABLE u(g TEXT, k INTEGER);
INSERT INTO t VALUES(1,'a'),(2,'a');
INSERT INTO u VALUES('a',5);
SELECT t.id, quote(u.k) FROM t LEFT JOIN u ON u.g=t.g LIMIT 10 OFFSET 1;
SELECT count(*), quote(sum(k)) FROM (SELECT u.k AS k FROM t LEFT JOIN u ON u.g=t.g LIMIT 10 OFFSET 1);
-- case: holdout/joins/452-where-u-k-in-subquery-on-the-right-table-of-a-left
CREATE TABLE t(id INTEGER PRIMARY KEY);
CREATE TABLE u(k TEXT);
CREATE TABLE v(k TEXT);
INSERT INTO t VALUES(5),(7);
INSERT INTO u VALUES('7');
INSERT INTO v VALUES('7'),('9');
CREATE INDEX ui ON u(k);
SELECT group_concat(t.id) FROM t LEFT JOIN u ON t.id = u.k WHERE u.k IN (SELECT k FROM v);
SELECT count(*), count(u.k) FROM t LEFT JOIN u ON t.id = u.k WHERE u.k IN (SELECT k FROM v WHERE k='a');
-- case: holdout/joins/453-recursive-cte-joined-on-an-integer-column-compared
CREATE TABLE t(id INTEGER, p TEXT);
INSERT INTO t VALUES(1,NULL),(2,'1'),(3,'2');
WITH RECURSIVE w(id) AS (SELECT id FROM t WHERE p IS NULL UNION ALL SELECT t.id FROM t JOIN w ON t.p = w.id) SELECT group_concat(id) FROM w;
WITH RECURSIVE w(id) AS (SELECT id FROM t WHERE p IS NULL UNION ALL SELECT t.id FROM t JOIN w ON t.p = w.id) DELETE FROM t WHERE id IN (SELECT id FROM w);
SELECT count(*) FROM t;
-- case: holdout/joins/464-predicate-on-the-right-side-of-a-left-join-subquer
create table t(a unique);
insert into t values (1),(5);
select * from t left join (select * from t) tt where tt.a = 5 order by 1;
explain query plan select * from t left join (select * from t) tt where tt.a = 5;
-- case: holdout/joins/472-schema-qualified-column-reference-in-where
CREATE TABLE a(id INTEGER, x INTEGER);
CREATE TABLE b(id INTEGER, y INTEGER);
INSERT INTO a VALUES (1,10),(2,20);
INSERT INTO b VALUES (1,100),(2,200),(3,300);
SELECT a.id, b.y FROM a JOIN b ON a.id = b.id WHERE main.a.x > 5 ORDER BY 1;
SELECT main.a.id, main.b.y FROM main.a, main.b WHERE main.a.id = main.b.id ORDER BY 1;
SELECT temp.a.id FROM a;
-- case: holdout/joins/481-join-on-a-without-rowid-table
CREATE TABLE w(v INTEGER, k TEXT PRIMARY KEY) WITHOUT ROWID;
CREATE TABLE other(k TEXT, x INTEGER);
INSERT INTO w VALUES (10, 'a');
INSERT INTO other VALUES ('a', 100);
SELECT w.v, other.x FROM w JOIN other ON w.k = other.k;
SELECT w.v, other.x FROM other JOIN w ON w.k = other.k;
SELECT rowid FROM w;
-- case: holdout/joins/498-a-cte-is-visible-inside-a-subquery-in-a-join-on-cl
WITH c AS (SELECT 1 AS x) SELECT * FROM c a JOIN c b ON b.x = (SELECT max(x) FROM c);
WITH c AS (SELECT 1 AS x) SELECT * FROM c a JOIN c b ON b.x IN (SELECT x FROM c);
WITH c AS (SELECT 1 AS x) SELECT * FROM c a JOIN c b ON EXISTS (SELECT 1 FROM c);
WITH c AS (SELECT 1 AS x) SELECT * FROM c WHERE x = (SELECT max(x) FROM c);
-- case: holdout/joins/503-count-over-a-join-into-a-without-rowid-table
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER, b INTEGER, c TEXT, PRIMARY KEY(a, b)) WITHOUT ROWID;
INSERT INTO t1 VALUES (1, 2, 'y'), (2, 5, 'xy'), (5, 3, 'z');
INSERT INTO t3 VALUES (1, 2, 'X'), (1, 3, 'X'), (2, 0, 'xy'), (3, 1, 'z'), (3, 2, 'y');
SELECT count(*) FROM t1 AS x JOIN t3 AS y ON x.a = y.a;
SELECT count(*) FROM t3 AS x JOIN t3 AS y ON x.a = y.a;
SELECT x.a, y.b FROM t1 AS x JOIN t3 AS y ON x.a = y.a ORDER BY 1, 2;
-- case: holdout/joins/507-left-join-using-on-a-without-rowid-table-followed-
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER, b INTEGER, c TEXT, PRIMARY KEY(a, b)) WITHOUT ROWID;
INSERT INTO t1 VALUES (1, 10, ''), (2, 10, 'y'), (3, -1, ''), (4, NULL, ''), (5, 3, 'x'), (6, 0, 'z'), (7, 0, NULL);
INSERT INTO t3 VALUES (2, 3, ''), (1, 3, 'z'), (4, 1, 'y'), (4, 2, '');
SELECT count(*) FROM t1 AS x LEFT JOIN t3 AS y USING (c) LEFT JOIN t1 AS z ON 0;
SELECT count(*) FROM t1 AS x LEFT JOIN t3 AS y ON x.c = y.c LEFT JOIN t1 AS z ON 0;
-- case: holdout/joins/508-full-join-after-another-join-emits-unmatched-right
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER);
INSERT INTO t1 VALUES (1, 2, 'y'), (2, NULL, 'xy'), (3, 4, 'z');
INSERT INTO t2 VALUES (9, 5, NULL), (5, 1, 'z');
INSERT INTO t3 VALUES (7);
SELECT z.a FROM t1 AS x CROSS JOIN t3 AS y FULL JOIN t2 AS z ON 0 ORDER BY 1;
SELECT count(*) FROM t1 AS x CROSS JOIN t1 AS y FULL JOIN t2 AS z ON 0;
SELECT z.a FROM t1 AS x JOIN t1 AS y ON 1 FULL JOIN t2 AS z ON x.a = z.a ORDER BY 1;
-- case: holdout/joins/510-in-subquery-in-the-on-clause-of-a-second-left-join
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER NOT NULL DEFAULT 0, c TEXT DEFAULT 'd');
INSERT INTO t1 VALUES (1, 1, 'x'), (2, 2, 'y');
INSERT INTO t2 VALUES (1, 1, 'y'), (3, 3, 'z');
INSERT INTO t3 VALUES (1, 2, 'x'), (2, 1, 'X');
SELECT * FROM t2 AS x LEFT JOIN t3 AS y ON 0 LEFT JOIN t1 AS z ON x.a IN (SELECT w.b FROM t1 AS w WHERE y.b = 1);
-- case: holdout/joins/512-using-after-a-right-join-with-an-ambiguous-column
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t1 VALUES (1, 1, 'x');
INSERT INTO t3 VALUES (1, 1, 'y'), (2, 1, 'z');
SELECT count(*) FROM t1 AS x RIGHT JOIN t3 AS y ON x.a = y.a JOIN t3 AS z USING (b);
SELECT count(*) FROM t1 AS x JOIN t3 AS y ON x.a = y.a JOIN t3 AS z USING (b);
SELECT count(*) FROM t1 AS x LEFT JOIN t3 AS y USING (b) JOIN t3 AS z USING (b);
-- case: holdout/joins/513-not-in-subquery-in-a-left-join-on-clause-that-refe
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
INSERT INTO t1 VALUES (1, 1, 'x'), (2, 2, 'y');
INSERT INTO t2 VALUES (1, 1, 'y'), (3, 3, 'z');
SELECT * FROM t1 AS x LEFT JOIN t2 AS y ON x.a = y.a LEFT JOIN t1 AS w ON w.c NOT IN (SELECT z.c FROM t1 AS z WHERE z.a = y.a) ORDER BY 1, 4, 7;
SELECT * FROM t1 AS x LEFT JOIN t2 AS y ON x.a = y.a LEFT JOIN t1 AS w ON w.c NOT IN (SELECT z.c FROM t1 AS z WHERE y.b) ORDER BY 1, 4, 7;
-- case: holdout/joins/520-order-by-the-join-column-of-a-join-using-two-integ
create table t(a integer primary key);
create table tt(a integer primary key);
insert into t values (3),(1),(2);
insert into tt values (2),(3),(4);
select * from t join tt using (a) order by t.a;
select * from t join tt using (a) order by a DESC;
create table u(a integer primary key);
create table v(b);
insert into u values (1),(2);
insert into v values (2),(1),(2);
select * from u join v on u.a = v.b order by v.b;
-- case: holdout/joins/561-pragma-index-list-and-pragma-index-info-joined-wit
CREATE TABLE a(x, y, UNIQUE(x, y));
CREATE TABLE b(z);
CREATE INDEX bz ON b(z);
SELECT DISTINCT m.name || '.' || ii.name AS 'indexed-columns' FROM sqlite_schema AS m, pragma_index_list(m.name) AS il, pragma_index_info(il.name) AS ii WHERE m.type='table' ORDER BY 1;
SELECT m.name, il.name, il."unique", il.origin, il.partial FROM sqlite_schema m, pragma_index_list(m.name) il WHERE m.type = 'table' ORDER BY 1, 2;
-- case: holdout/joins/609-left-join-on-integer-column-equal-to-real-column-w
CREATE TABLE IF NOT EXISTS t1 (a INTEGER, b INTEGER);
CREATE TABLE IF NOT EXISTS t2 (a INTEGER, c REAL);
INSERT INTO t1 (a, b) VALUES (1, NULL), (2, 10);
INSERT INTO t2 (a, c) VALUES (1, 10.0), (3, NULL);
SELECT * FROM t1 LEFT JOIN t2 ON t1.b = t2.c;
SELECT quote(c), typeof(c) FROM t2;
-- case: holdout/joins/637-cte-with-an-indexed-source-referenced-twice-in-a-j
CREATE TABLE t(a);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES (0), (1);
WITH c AS (SELECT 0 c0 FROM (SELECT * FROM t) x) SELECT * FROM (SELECT * FROM c, c c2 JOIN (SELECT 1) u ON c2.c0 = 0) y;
-- case: holdout/joins/691-two-partial-indexes-whose-predicates-are-not-impli
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b INT, c INT, s TEXT);
CREATE TABLE u(id INTEGER PRIMARY KEY, a INT, b INT, c INT, s TEXT);
INSERT INTO t VALUES (1, 2, 3, -1, '');
CREATE INDEX idx_t_a_sy ON t(a) WHERE s = 'y';
CREATE INDEX idx_t_s_c_gt3 ON t(s) WHERE c > 3;
SELECT t.id, u.id FROM t LEFT JOIN u ON u.a = t.b AND u.c > 3 WHERE t.a = 2 AND t.s = '';
SELECT t.id FROM t WHERE t.a = 2 AND t.s = '';
SELECT t.id FROM t WHERE t.a = 2 AND t.s = 'y';
-- case: holdout/joins/713-full-join-with-a-where-that-filters-out-matched-ro
CREATE TABLE t1 (a INT);
CREATE TABLE t2 (b INT);
INSERT INTO t1 VALUES (1);
INSERT INTO t2 VALUES (NULL);
SELECT a, b FROM t1 FULL JOIN t2 ON true WHERE a IS NULL ORDER BY coalesce(a, b, 3);
SELECT a, b FROM t1 FULL JOIN t2 ON true ORDER BY coalesce(a, b, 3);
SELECT a, b FROM t1 FULL JOIN t2 ON false ORDER BY coalesce(a, b, 3);
SELECT a, b FROM t1 FULL JOIN t2 ON true WHERE b IS NULL;
-- case: holdout/joins/716-right-join-following-another-join
CREATE TABLE a(x INT); CREATE TABLE b(x INT); CREATE TABLE c(x INT);
INSERT INTO a VALUES(1),(2); INSERT INTO b VALUES(2),(3); INSERT INTO c VALUES(3),(4);
SELECT a.x, b.x, c.x FROM a JOIN b ON a.x=b.x RIGHT JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a LEFT JOIN b ON a.x=b.x RIGHT JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a, b RIGHT JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a RIGHT JOIN b ON a.x=b.x RIGHT JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
-- case: holdout/joins/717-full-outer-join-chained-after-another-outer-join
CREATE TABLE a(x INT); CREATE TABLE b(x INT); CREATE TABLE c(x INT);
INSERT INTO a VALUES(1),(2); INSERT INTO b VALUES(2),(3); INSERT INTO c VALUES(3),(4);
SELECT a.x, b.x, c.x FROM a FULL OUTER JOIN b ON a.x=b.x FULL OUTER JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a LEFT JOIN b ON a.x=b.x FULL OUTER JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a FULL OUTER JOIN b ON a.x=b.x LEFT JOIN c ON c.x=b.x ORDER BY 1, 2, 3;
SELECT a.x, b.x, c.x FROM a FULL OUTER JOIN b ON a.x=b.x FULL OUTER JOIN c ON c.x=a.x ORDER BY 1, 2, 3;
-- case: holdout/joins/719-text-column-compared-with-a-column-of-a-cte-or-vie
CREATE TABLE orders(order_id INTEGER, status_code TEXT);
INSERT INTO orders VALUES (1, '1'), (2, '2'), (3, '3');
WITH status_labels(code, label) AS (SELECT 1, 'Active' UNION ALL SELECT 2, 'Cancelled' UNION ALL SELECT 3, 'Pending') SELECT o.order_id, sl.label FROM orders o JOIN status_labels sl ON o.status_code = sl.code ORDER BY 1;
CREATE TABLE contacts(phone TEXT);
INSERT INTO contacts VALUES ('4155551234');
CREATE TABLE call_log(raw_number INTEGER);
INSERT INTO call_log VALUES (14155551234);
CREATE VIEW normalized AS SELECT raw_number + 0 AS n FROM call_log;
SELECT count(*) FROM contacts c JOIN normalized v ON c.phone = v.n;
SELECT count(*) FROM contacts c JOIN call_log l ON c.phone = l.raw_number;
SELECT count(*) FROM contacts c JOIN (SELECT raw_number FROM call_log) l ON c.phone = l.raw_number;
-- case: holdout/joins/732-left-join-with-a-constant-equality-on-an-indexed-c
CREATE TABLE t1(a);
CREATE TABLE t2(b, c);
CREATE INDEX i2 ON t2(b);
INSERT INTO t1 VALUES (2);
INSERT INTO t2 VALUES (2, 0);
SELECT t2.rowid FROM t1 LEFT JOIN t2 ON t2.b = t1.a AND t2.b = 1 AND t2.c;
SELECT t1.a, t2.rowid, t2.b FROM t1 LEFT JOIN t2 ON t2.b = t1.a AND t2.c;
SELECT t1.a, t2.rowid, t2.b FROM t1 LEFT JOIN t2 ON t2.b = t1.a AND NOT t2.c;
-- case: holdout/joins/811-update-from-with-a-left-join-as-the-source
CREATE TABLE target(id INTEGER PRIMARY KEY, value);
CREATE TABLE left_side(id INTEGER PRIMARY KEY);
CREATE TABLE source(id INTEGER PRIMARY KEY, value);
INSERT INTO target VALUES (1, 'old1'), (2, 'old2'), (3, 'old3');
INSERT INTO left_side VALUES (1), (2);
INSERT INTO source VALUES (1, 'new1');
UPDATE target SET value = COALESCE(source.value, 'dflt') FROM left_side LEFT JOIN source ON source.id = left_side.id WHERE target.id = left_side.id;
SELECT * FROM target ORDER BY id;
-- case: holdout/joins/828-join-from-an-integer-key-into-an-untyped-indexed-c
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id);
CREATE INDEX child_p ON child(parent_id);
INSERT INTO parent VALUES(1),(2);
INSERT INTO child VALUES(10,1),(20,'2');
SELECT parent.id, child.id FROM parent JOIN child ON child.parent_id = parent.id ORDER BY 1, 2;
SELECT parent.id, child.id FROM child JOIN parent ON child.parent_id = parent.id ORDER BY 1, 2;
SELECT typeof(parent_id) FROM child ORDER BY id;
-- case: holdout/joins/829-three-table-join-where-an-untyped-column-is-compar
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b TEXT, c REAL, d, n NUMERIC);
INSERT INTO t VALUES (1,1,'1',1.0,1,1),(2,2,'2',2.5,'2','2'),(3,3,'01',3.0,'x',3.5),(4,NULL,NULL,NULL,NULL,NULL),(5,5,'1.0',1.5,x'35',5),(6,6,'abc',6.0,6.0,'6'),(12,12,'12',12.0,'12',12);
CREATE TABLE u(id INTEGER PRIMARY KEY, a INTEGER, b TEXT, r REAL, d, n NUMERIC);
INSERT INTO u VALUES (1,1,'1',1.5,'1',1.0),(2,2,'01',2.0,2,'2'),(3,3,'2abc',3.0,'3',3),(4,NULL,'12',12.0,NULL,NULL),(5,5,'1.0',5.0,x'31',5.5),(6,6,'abc',6.0,'6',6),(7,12,' 2',2.5,12,'12');
CREATE TABLE v(id INTEGER PRIMARY KEY, k TEXT UNIQUE, m INTEGER UNIQUE);
INSERT INTO v VALUES (1,'1',1),(2,'2',2),(3,'abc',3),(12,'12',12);
SELECT u.id,v.id,t.id FROM u JOIN v ON v.k=u.d JOIN t ON t.id=v.m ORDER BY 1,2,3;
SELECT u.id,v.id FROM u JOIN v ON v.k=u.d ORDER BY 1,2;
