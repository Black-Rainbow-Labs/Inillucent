-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/window-functions/96-window-function-inside-a-correlated-exists-subquer
CREATE TABLE t1 (c1 INTEGER PRIMARY KEY, c2 INTEGER);
INSERT INTO t1 VALUES (0, 0), (1, 1), (2, 2);
SELECT COUNT(*) FROM t1 AS a WHERE EXISTS (SELECT 1 FROM t1 AS b WHERE a.c1 = a.c1 ORDER BY +sum(1) OVER (ORDER BY 0));
SELECT COUNT(*) FROM t1 AS a WHERE EXISTS (SELECT 1 FROM t1 AS b WHERE a.c1 = b.c1 ORDER BY sum(a.c2) OVER (ORDER BY 0));
-- case: holdout/window-functions/130-window-function-together-with-group-by-and-order-b
CREATE TABLE t(a);
INSERT INTO t VALUES (1), (1), (2);
SELECT a, count(a) OVER (PARTITION BY a ORDER BY a) FROM t GROUP BY a ORDER BY 2;
SELECT a, count(*) OVER () FROM t GROUP BY a ORDER BY 1;
-- case: holdout/window-functions/202-extending-a-named-window-that-already-has-order-by
CREATE TABLE t(val INT);
INSERT INTO t VALUES(1),(2),(3);
SELECT SUM(val) OVER (w ORDER BY val) FROM t WINDOW w AS (ORDER BY val);
SELECT SUM(val) OVER (w ORDER BY val) FROM t WINDOW w AS (PARTITION BY val);
SELECT SUM(val) OVER (w ROWS UNBOUNDED PRECEDING) FROM t WINDOW w AS (ORDER BY val);
-- case: holdout/window-functions/383-window-function-as-a-column-default-is-rejected
CREATE TABLE d(id INTEGER, v INTEGER DEFAULT (count(*) OVER ()));
CREATE TABLE g(id INTEGER, v INTEGER DEFAULT (count(id) OVER ()));
CREATE TABLE h(id INTEGER, v INTEGER DEFAULT (random()));
CREATE TABLE i(id INTEGER, v INTEGER DEFAULT (id));
CREATE TABLE j(id INTEGER, v INTEGER DEFAULT (SELECT 1));
CREATE TABLE k(id INTEGER, v INTEGER DEFAULT (abs(-1)));
CREATE TABLE l(id INTEGER, v INTEGER DEFAULT (1+1), w DEFAULT CURRENT_TIMESTAMP, x DEFAULT -5, y DEFAULT +5, z DEFAULT 'a' 'b');
-- case: holdout/window-functions/406-a-window-function-does-not-change-last-insert-rowi
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT);
INSERT INTO t VALUES(1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e');
INSERT INTO t(k) VALUES('f');
SELECT count(*) FROM (SELECT row_number() OVER (ORDER BY k) FROM t WHERE id<3);
SELECT last_insert_rowid();
SELECT count(*) FROM (SELECT row_number() OVER (ORDER BY k) FROM t WHERE id<5);
SELECT last_insert_rowid();
INSERT INTO t(k) VALUES('g');
SELECT last_insert_rowid();
SELECT k FROM t ORDER BY k;
SELECT last_insert_rowid();
-- case: holdout/window-functions/437-update-whose-set-subquery-uses-a-window-function-o
CREATE TABLE t(id INTEGER PRIMARY KEY, v INTEGER);
INSERT INTO t VALUES(1,1),(2,2),(3,3),(4,4);
UPDATE t SET v = (SELECT q FROM (SELECT id AS iid, row_number() OVER (ORDER BY v DESC) AS q FROM t) WHERE iid = t.id);
SELECT id, v FROM t ORDER BY id;
CREATE TABLE u(id INTEGER PRIMARY KEY, v INTEGER);
INSERT INTO u VALUES(1,1),(2,2),(3,3);
UPDATE u SET v = (SELECT q FROM (SELECT id AS iid, sum(v) OVER () AS q FROM u) WHERE iid = u.id);
SELECT id, v FROM u ORDER BY id;
-- case: holdout/window-functions/499-column-names-of-window-functions-seen-through-a-de
CREATE TABLE w(id INTEGER PRIMARY KEY, k INTEGER, v INTEGER);
INSERT INTO w VALUES (1,1,10),(2,1,20),(3,2,30);
.headers on
SELECT * FROM (SELECT sum(v) OVER (ORDER BY k), count(*) OVER (PARTITION BY k) FROM w) AS q;
SELECT * FROM (SELECT k, sum(v), rank() OVER (ORDER BY sum(v) DESC) AS r FROM w GROUP BY k) AS q;
-- case: holdout/window-functions/518-aggregate-inside-the-order-by-of-a-window-function
SELECT 1 ORDER BY row_number() OVER (ORDER BY sum(1));
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES (1), (2);
SELECT a FROM t ORDER BY row_number() OVER (ORDER BY sum(a));
SELECT a FROM t ORDER BY sum(a);
SELECT sum(a) FROM t ORDER BY row_number() OVER (ORDER BY sum(a));
-- case: holdout/window-functions/590-a-window-definition-that-extends-a-named-window-wi
create table t(a,b,c);
insert into t values (1,1,1), (1,1,2), (1,2,4), (2,2,8), (2,1,16), (2,1,32);
select sum(a) over (w partition by b) from t window w as (partition by a);
select sum(a) over (w partition by b) from t window w as ();
select sum(c) over (w ORDER BY c) from t window w as (partition by a) ;
select sum(c) over w from t window w as (partition by a order by c);
select sum(c) over (w) from t window w as (partition by a order by c rows between 1 preceding and current row);
select sum(c) over (w rows between 1 preceding and current row) from t window w as (partition by a order by c) order by 1;
-- case: holdout/window-functions/657-correlated-subquery-and-a-window-function-in-the-s
CREATE TABLE t(id INT, grp TEXT, val INT);
INSERT INTO t VALUES (1,'a',10),(2,'a',20),(3,'b',30);
SELECT t.id, (SELECT COUNT(*) FROM t t2 WHERE t2.grp = t.grp) as grp_cnt, SUM(t.val) OVER () as total FROM t ORDER BY t.id;
SELECT t.id, SUM(t.val) OVER () FROM t ORDER BY t.id;
-- case: holdout/window-functions/658-row-value-inside-a-window-aggregate-argument
CREATE TABLE t(a);
INSERT INTO t VALUES(0);
SELECT COUNT(*) FROM t ORDER BY SUM(CASE WHEN a THEN 1 ELSE (1, 1) END) OVER();
SELECT CASE WHEN a THEN 1 ELSE (1, 1) END FROM t;
SELECT sum((1,1)) FROM t;
-- case: holdout/window-functions/669-window-function-in-a-query-whose-where-has-a-scala
CREATE TABLE t(id INT, val INT);
INSERT INTO t VALUES(1,10),(2,20);
SELECT SUM(val) OVER () FROM t WHERE id = (SELECT MIN(id) FROM t);
SELECT id, SUM(val) OVER (ORDER BY id) FROM t WHERE id IN (SELECT id FROM t WHERE val > 5);
SELECT id, SUM(val) OVER () FROM t WHERE EXISTS (SELECT 1 FROM t t2 WHERE t2.id = t.id);
-- case: holdout/window-functions/720-repeated-lag-sum-with-group-by
CREATE TABLE t (d TEXT, x INT);
INSERT INTO t VALUES ('2026-01-01',1),('2026-02-02',2),('2026-03-03',3);
SELECT strftime('%Y-%m', d) AS k, AVG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d)) AS running_avg, 100.0 * (SUM(x) - LAG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d))) / LAG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d)) AS pct_change FROM t GROUP BY 1;
-- case: holdout/window-functions/810-window-functions-with-order-by-nulls-last-and-an-e
CREATE TABLE t(v);
INSERT INTO t VALUES (3), (NULL), (1), (2), (NULL);
SELECT v, sum(v) OVER (ORDER BY v NULLS LAST ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW), count(*) OVER (ORDER BY v NULLS LAST RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t ORDER BY v NULLS LAST, rowid;
SELECT v, sum(v) OVER (ORDER BY v DESC NULLS FIRST ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t ORDER BY v DESC NULLS FIRST, rowid;
SELECT v, row_number() OVER (ORDER BY v NULLS FIRST), rank() OVER (ORDER BY v NULLS LAST), dense_rank() OVER (ORDER BY v) FROM t ORDER BY rowid;
