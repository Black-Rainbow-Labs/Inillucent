-- case: bug-hunt-9/sql/recursive-cte-outer-where-and-limit-stop-the-recursion
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n FROM r WHERE n>3 LIMIT 2;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n FROM r WHERE n%7=0 LIMIT 3 OFFSET 2;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n, n*2 FROM r, (SELECT 1) WHERE n>3 LIMIT 2;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT group_concat(n) FROM (SELECT n FROM r WHERE n>3 LIMIT 3);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n FROM r WHERE n>3 LIMIT 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5000) SELECT count(*), sum(n) FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5000) SELECT n FROM r WHERE n>4990;
WITH RECURSIVE r(n) AS (SELECT 1 UNION SELECT (n*7)%11 FROM r) SELECT n FROM r WHERE n>2;
-- case: bug-hunt-9/sql/recursive-cte-literal-anchor-has-no-affinity
CREATE TABLE t(id INTEGER PRIMARY KEY, p TEXT); INSERT INTO t VALUES(2,'1');
WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT 2 FROM r WHERE x<1) SELECT id, p=x FROM t, r WHERE t.p = r.x;
WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT 2 FROM r WHERE x<1) SELECT id FROM t, r WHERE r.x = t.p;
WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT 2 FROM r WHERE x<1) SELECT id FROM t WHERE p IN (SELECT x FROM r);
WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM r WHERE x<3) SELECT x FROM r WHERE x = (SELECT p FROM t);
-- case: bug-hunt-9/sql/recursive-cte-shapes-sqlite-refuses
WITH RECURSIVE r(n) AS (SELECT n FROM r) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3 UNION ALL SELECT 100) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n IN (SELECT n FROM r)) SELECT * FROM r LIMIT 3;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r, r r2 WHERE r.n<3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT n+1 FROM r UNION ALL SELECT 1) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3 UNION SELECT 100) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION SELECT n+1 FROM r WHERE n<3 EXCEPT SELECT 2) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n FROM (SELECT * FROM r)) SELECT * FROM r LIMIT 3;
-- case: bug-hunt-9/sql/recursive-cte-shapes-sqlite-accepts
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3 UNION ALL SELECT n+10 FROM r WHERE n<3) SELECT * FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT n+1 FROM r WHERE n<3) SELECT * FROM r;
WITH r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3) SELECT * FROM r;
-- case: bug-hunt-9/sql/filter-on-min-or-max-picks-the-bare-column-row
CREATE TABLE t(g,v,w); INSERT INTO t VALUES(1,10,'a'),(1,20,'b'),(2,5,'c');
SELECT max(v) FILTER (WHERE v<20), w FROM t;
SELECT min(v) FILTER (WHERE v>10), w FROM t;
SELECT max(v) FILTER (WHERE 0), w FROM t;
SELECT max(v) FILTER (WHERE v>5), min(v) FILTER (WHERE v<20), w FROM t;
SELECT max(v) FILTER (WHERE v<20), count(*), w FROM t;
SELECT g, max(v) FILTER (WHERE v<20), w FROM t GROUP BY g;
SELECT g, max(v) FILTER (WHERE v>10), w FROM t GROUP BY g;
-- case: bug-hunt-9/sql/unqualified-rowid-over-two-tables-is-ambiguous
CREATE TABLE a(x); CREATE TABLE b(y); INSERT INTO a VALUES(1); INSERT INTO b VALUES(2);
SELECT rowid FROM a, b;
SELECT _rowid_ FROM a, b;
SELECT oid FROM a JOIN b;
SELECT a.rowid, b.oid FROM a, b;
-- case: bug-hunt-9/sql/lag-and-lead-offsets-that-name-no-row
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2),(3);
SELECT lag(a,NULL) OVER (ORDER BY a) FROM t;
SELECT lead(a,1.5,9) OVER (ORDER BY a) FROM t;
SELECT lag(a,1.5) OVER (ORDER BY a) FROM t;
SELECT lag(a,'1') OVER (ORDER BY a) FROM t;
SELECT lag(a,'x') OVER (ORDER BY a) FROM t;
SELECT lag(a,1.0) OVER (ORDER BY a) FROM t;
SELECT lag(a,-1) OVER (ORDER BY a) FROM t;
SELECT lag(a,NULL,7) OVER (ORDER BY a) FROM t;
SELECT lead(a,0) OVER (ORDER BY a) FROM t;
-- case: bug-hunt-9/sql/foreign-key-check-refuses-a-parent-with-no-unique-key
CREATE TABLE p(a PRIMARY KEY, b); CREATE TABLE c(y REFERENCES p(b));
INSERT INTO c VALUES(2); INSERT INTO p VALUES(1,5);
PRAGMA foreign_key_check;
PRAGMA foreign_key_check(c);
-- case: bug-hunt-9/sql/round-with-no-places-adds-a-half-first
SELECT round(0.49999999999999994), round(-0.49999999999999994), round(0.49999999999999994,0);
SELECT round(0.5), round(-0.5), round(1.5), round(2.5), round(-2.5), round(0.4), round(-0.4);
SELECT round(4503599627370497.0), round(-4503599627370497.0), round(1e300), round(0.0), round(-0.0);
-- case: bug-hunt-9/sql/unhex-with-a-null-list-of-ignored-characters
SELECT unhex('4142',NULL), unhex('41',NULL), unhex(NULL,'x'), unhex('4142'), unhex('4 142',' ');
-- case: bug-hunt-9/sql/printf-and-format-with-no-arguments
SELECT printf();
SELECT format();
SELECT printf(NULL), printf('%d', 3), format('%s-%s', 'a', 'b');
-- case: bug-hunt-9/sql/integrity-check-argument-names-a-table
CREATE TABLE t(a);
PRAGMA integrity_check(nosuch);
PRAGMA quick_check(nosuch);
PRAGMA integrity_check(t);
PRAGMA quick_check(t);
PRAGMA integrity_check(10);
PRAGMA quick_check(10);
-- case: bug-hunt-9/sql/attached-memory-database-journal-and-locking-mode
ATTACH ':memory:' AS m;
PRAGMA m.journal_mode;
PRAGMA m.locking_mode;
PRAGMA temp.journal_mode;
PRAGMA temp.locking_mode;
PRAGMA main.locking_mode;
-- case: bug-hunt-9/sql/unknown-collation-in-a-result-column-is-not-looked-up
SELECT 'a' COLLATE bogus;
SELECT 'a' COLLATE bogus = 'a';
-- case: bug-hunt-9/sql/schema-qualified-table-valued-function
SELECT * FROM main.generate_series(1,2);
SELECT * FROM main.json_each('[1]');
SELECT value FROM temp.json_each('[3,4]');
-- case: bug-hunt-9/sql/parenthesized-join-using-puts-the-using-column-first
CREATE TABLE a(x); CREATE TABLE b(x,y); CREATE TABLE c(y,z); INSERT INTO a VALUES(1); INSERT INTO b VALUES(1,10); INSERT INTO c VALUES(10,'c10');
SELECT * FROM a, (b JOIN c USING(y));
SELECT * FROM a LEFT JOIN (b JOIN c USING(y)) ON a.x=b.x;
SELECT * FROM (b JOIN c USING(y)), a;
SELECT * FROM a JOIN (b JOIN c USING(y)) ON a.x=b.x;
SELECT * FROM a, (b NATURAL JOIN c);
SELECT * FROM a, (b JOIN c ON b.y=c.y);
SELECT * FROM a, (c JOIN b USING(y));
SELECT * FROM b JOIN c USING(y);
-- case: bug-hunt-9/sql/rtree-i32-truncates-real-coordinates
CREATE VIRTUAL TABLE r2 USING rtree_i32(id, a0, a1);
INSERT INTO r2 VALUES(1,1.9,2.1),(2,-1.9,-1.1),(3,0.4,0.6),(4,5.5,5.5),(5,'3.9',7);
SELECT * FROM r2 ORDER BY id;
SELECT id FROM r2 WHERE a0>=1 AND a1<=2 ORDER BY id;
-- case: bug-hunt-9/sql/virtual-table-constraint-messages-name-the-table
CREATE VIRTUAL TABLE r USING rtree(id, a0, a1);
INSERT INTO r VALUES(1,0,1);
INSERT INTO r VALUES(1,0,1);
INSERT INTO r VALUES(2,5,1);
-- case: bug-hunt-9/sql/fts5-column-filter-error-has-no-prefix
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES('x y','z');
SELECT * FROM f WHERE f MATCH 'nosuch:x';
SELECT * FROM f WHERE f MATCH '{nosuch}:x';
SELECT * FROM f WHERE f MATCH 'a:x AND nosuch:y';
SELECT a FROM f WHERE f MATCH 'a:x';
