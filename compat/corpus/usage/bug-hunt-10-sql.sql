-- case: bug-hunt-10/sql/recursive-cte-step-compares-text-column-with-cte-column
CREATE TABLE n(id INTEGER PRIMARY KEY, parent TEXT); INSERT INTO n VALUES(1,NULL),(2,'1'),(3,'2');
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT n.id FROM s JOIN n ON n.parent=s.x) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT n.id FROM s, n WHERE s.x=n.parent) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT (SELECT n.id FROM n WHERE n.parent=s.x) FROM s WHERE x<3) SELECT x FROM s;
WITH s(x) AS (SELECT 1) SELECT n.id FROM s JOIN n ON n.parent=s.x;
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT n.id FROM s, n WHERE n.parent IN (s.x)) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT n.id FROM s, n WHERE s.x IN (n.parent)) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT n.id FROM s JOIN n ON n.parent=s.x+0) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT '1' UNION ALL SELECT n.parent FROM s JOIN n ON n.id=s.x AND n.parent IS NOT NULL) SELECT x, typeof(x) FROM s;
-- case: bug-hunt-10/sql/recursive-cte-step-affinity-follows-every-arm
CREATE TABLE b(k INTEGER PRIMARY KEY, v); INSERT INTO b VALUES(10,1),(11,'1'),(12,'1.0'),(13,1.0);
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT k FROM s, b WHERE s.x = b.v AND k<100) SELECT group_concat(x) FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT k+0 FROM s, b WHERE s.x = b.v AND k<100) SELECT group_concat(x) FROM s;
WITH RECURSIVE s(x) AS (SELECT '1' UNION SELECT k+0 FROM s, b WHERE s.x = b.v AND k<100) SELECT group_concat(x) FROM s;
WITH RECURSIVE s(x) AS (SELECT '1' UNION SELECT k FROM s, b WHERE s.x = b.v AND k<100) SELECT group_concat(x) FROM s;
WITH RECURSIVE s(x) AS (SELECT '5' UNION SELECT 1+x FROM s WHERE x < 7) SELECT group_concat(x) FROM s;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent TEXT); INSERT INTO n VALUES(1,NULL),(2,'01'),(3,'2');
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT id FROM s, n WHERE s.x = n.parent AND id<5) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT id FROM s, n WHERE s.x IN (n.parent) AND id<5) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT id FROM s, n WHERE n.parent IN (s.x) AND id<5) SELECT x FROM s;
WITH RECURSIVE s(x) AS (SELECT 1 UNION SELECT id FROM s, n WHERE s.x BETWEEN n.parent AND n.parent AND id<5) SELECT x FROM s;
-- case: bug-hunt-10/sql/derived-table-over-derived-table-has-no-affinity
CREATE TABLE n(id INTEGER PRIMARY KEY, parent TEXT); INSERT INTO n VALUES(1,NULL),(2,'1'),(3,'2');
SELECT n.id FROM (SELECT x FROM (SELECT 1 AS x)) AS s JOIN n ON n.parent=s.x;
SELECT n.id FROM (SELECT 1 AS x) AS s JOIN n ON n.parent=s.x;
SELECT n.id FROM (SELECT x AS y FROM (SELECT 1 AS x) AS q) AS s JOIN n ON n.parent=s.y;
SELECT n.id FROM (SELECT x FROM (SELECT 1 AS x LIMIT 5)) AS s JOIN n ON n.parent=s.x;
SELECT n.id FROM (SELECT x FROM (SELECT 1 AS x UNION ALL SELECT 7)) AS s JOIN n ON n.parent=s.x;
WITH c(x) AS (SELECT 1) SELECT n.id FROM (SELECT x FROM c) AS s JOIN n ON n.parent=s.x;
WITH c(x) AS (SELECT 1), d(y) AS (SELECT x FROM c) SELECT n.id FROM d JOIN n ON n.parent=d.y;
-- case: bug-hunt-10/sql/recursive-cte-union-dedupes-with-column-collation
CREATE TABLE t(nc TEXT COLLATE NOCASE); INSERT INTO t VALUES('a');
WITH RECURSIVE c(k) AS (SELECT nc FROM t UNION SELECT 'A' FROM c) SELECT k FROM c;
WITH RECURSIVE c(k) AS (SELECT nc FROM t UNION SELECT upper(k) FROM c) SELECT k FROM c;
WITH RECURSIVE c(k) AS (SELECT 'x' COLLATE NOCASE UNION SELECT 'X' FROM c) SELECT k FROM c;
WITH c(k) AS (SELECT nc FROM t UNION SELECT 'A') SELECT k FROM c;
-- case: bug-hunt-10/sql/uncorrelated-scalar-subquery-runs-in-an-unreached-branch
CREATE TABLE t(a); INSERT INTO t VALUES(1);
SELECT CASE WHEN 0 THEN (SELECT abs(-9223372036854775808)) ELSE 1 END;
SELECT CASE WHEN a=2 THEN (SELECT abs(-9223372036854775808)) ELSE 1 END FROM t;
SELECT a FROM t WHERE a=1 OR (SELECT abs(-9223372036854775808));
SELECT coalesce(a, (SELECT abs(-9223372036854775808))) FROM t;
SELECT iif(a=1, 1, (SELECT abs(-9223372036854775808))) FROM t;
SELECT ifnull(a, (SELECT abs(-9223372036854775808))) FROM t;
SELECT a IN (1, (SELECT abs(-9223372036854775808))) FROM t;
SELECT CASE WHEN a=1 THEN (SELECT max(a) FROM t) ELSE (SELECT abs(-9223372036854775808)) END FROM t;
SELECT CASE WHEN a=1 THEN (SELECT max(a) FROM t) END, (SELECT count(*) FROM t) FROM t;
-- case: bug-hunt-10/sql/bare-column-beside-max-with-group-by-on-that-column
CREATE TABLE t(a,c); INSERT INTO t VALUES(1,10),(1.0,70),('1',60);
SELECT a, max(c) FROM t GROUP BY a;
SELECT a, typeof(a), max(c) FROM t GROUP BY a;
SELECT max(c), a FROM t GROUP BY a;
SELECT a, min(c) FROM t GROUP BY a;
SELECT a, typeof(a), max(c) FROM t GROUP BY 1;
SELECT a+0, typeof(a), max(c) FROM t GROUP BY a+0;
SELECT typeof(a), max(c) FROM t GROUP BY a HAVING max(c)>0;
SELECT typeof(a), max(c) FROM t GROUP BY a ORDER BY typeof(a);
SELECT typeof(a), min(c) FROM t GROUP BY a HAVING typeof(a)='integer' OR typeof(a)='real';
-- case: bug-hunt-10/sql/compound-order-by-term-matches-left-arm-expression
CREATE TABLE t(a,b); CREATE TABLE u(a,b); INSERT INTO t VALUES(1,'x'),(2,'y'); INSERT INTO u VALUES(2,'y'),(3,'z');
SELECT a FROM t UNION SELECT a FROM u ORDER BY t.a;
SELECT a+1 FROM t UNION SELECT a FROM u ORDER BY a+1;
SELECT a, b||'!' FROM t UNION SELECT a, b FROM u ORDER BY b||'!';
SELECT upper(b) FROM t UNION ALL SELECT b FROM u ORDER BY upper(b);
SELECT t.a FROM t UNION ALL SELECT a FROM u ORDER BY t.a DESC;
SELECT a FROM t EXCEPT SELECT a FROM u ORDER BY t.a;
-- case: bug-hunt-10/sql/column-constraint-null-on-conflict
CREATE TABLE f1(a NULL ON CONFLICT FAIL);
CREATE TABLE f2(a INT NULL ON CONFLICT IGNORE);
CREATE TABLE f3(a NULL ON CONFLICT REPLACE, b NOT NULL ON CONFLICT FAIL);
CREATE TABLE f4(a); ALTER TABLE f4 ADD COLUMN z NULL ON CONFLICT FAIL;
SELECT name FROM sqlite_schema ORDER BY 1;
-- case: bug-hunt-10/sql/column-constraint-null-on-conflict-keeps-nulls
CREATE TABLE f1(a NULL ON CONFLICT ROLLBACK, b UNIQUE NULL ON CONFLICT ABORT);
INSERT INTO f1 VALUES(NULL, NULL), (NULL, NULL), (1, 2);
SELECT * FROM f1;
-- case: bug-hunt-10/sql/empty-quoted-type-name-has-numeric-affinity
CREATE TABLE e1(s ""); INSERT INTO e1 VALUES('1'); SELECT typeof(s) FROM e1;
CREATE TABLE e2(s ''); INSERT INTO e2 VALUES('1.5'); SELECT typeof(s), s FROM e2;
CREATE TABLE e3(s []); INSERT INTO e3 VALUES('1'); SELECT typeof(s) FROM e3;
CREATE TABLE e4(s); INSERT INTO e4 VALUES('1'); SELECT typeof(s) FROM e4;
SELECT name, type FROM pragma_table_info('e1');
-- case: bug-hunt-10/sql/indexed-by-partial-index-implication
CREATE TABLE t(a, s); CREATE INDEX j ON t(a) WHERE s NOT IN ('p', 'q') AND s IS NOT 'z'; CREATE INDEX m ON t(a) WHERE s = 'x' OR s = 'y'; INSERT INTO t VALUES(1,'x'),(2,'y'),(3,'z'),(4,'only'),(5,'p');
SELECT a FROM t INDEXED BY j WHERE s NOT IN ('p','q') AND s IS NOT 'z' ORDER BY a;
SELECT a FROM t INDEXED BY m WHERE s = 'x' AND a>0 ORDER BY a;
SELECT a FROM t INDEXED BY m WHERE s = 'y' ORDER BY a;
SELECT a FROM t INDEXED BY m WHERE s IN ('x','y') ORDER BY a;
SELECT a FROM t INDEXED BY m WHERE s = 'x' OR s = 'y' ORDER BY a;
SELECT a FROM t INDEXED BY j WHERE s NOT IN ('p','q','r') AND s IS NOT 'z' AND a>0 ORDER BY a;
-- case: bug-hunt-10/sql/json-error-position-counts-characters-and-points-at-the-number
SELECT json_error_position('[1e]'), json_error_position('01'), json_error_position('[-]'), json_error_position('{"a":01}');
SELECT json_error_position(' '), json_error_position('[1,2]x'), json_error_position('[1,2] x'), json_error_position('["é",');
-- case: bug-hunt-10/sql/changes-counts-row-whose-after-insert-trigger-raised-fail
CREATE TABLE w(k); INSERT INTO w VALUES(1),(2),(3); CREATE TRIGGER tw AFTER INSERT ON w BEGIN SELECT CASE WHEN new.k=5 THEN RAISE(FAIL,'stop') END; END;
INSERT INTO w SELECT value+3 FROM generate_series(1,4);
SELECT changes(), total_changes();
SELECT * FROM w;
-- case: bug-hunt-10/sql/create-temp-index-is-a-syntax-error
CREATE TEMP TABLE tt(a);
CREATE TEMP INDEX tti ON tt(a);
CREATE TEMPORARY INDEX tti2 ON tt(a);
CREATE TEMP UNIQUE INDEX tti3 ON tt(a);
SELECT name FROM sqlite_temp_schema ORDER BY 1;
-- case: bug-hunt-10/sql/subquery-limit-keeps-the-rows-it-would-return-alone
CREATE TABLE t(a,b); INSERT INTO t VALUES(5,0),(1,0),(2,0),(1,0),(5,0),(1,0); CREATE INDEX ia ON t(a);
SELECT a FROM (SELECT a,b FROM t LIMIT 3) WHERE a=1;
SELECT a FROM (SELECT a,b FROM t LIMIT 3) WHERE a>1;
SELECT count(*) FROM (SELECT a,b FROM t LIMIT 3) WHERE a=5;
SELECT a FROM (SELECT a,b FROM t LIMIT 3 OFFSET 1) WHERE a=1;
SELECT a FROM (SELECT a,b FROM t LIMIT 3) AS q JOIN (SELECT 1 AS z) ON q.a=z;
SELECT a FROM (SELECT a,b FROM t ORDER BY b LIMIT 3) WHERE a=1;
-- case: bug-hunt-10/sql/subquery-window-and-aggregate-keep-their-rows
CREATE TABLE t(a,b); INSERT INTO t VALUES(5,0),(1,0),(2,0),(1,0),(5,0),(1,0); CREATE INDEX ia ON t(a);
SELECT a, r FROM (SELECT a, row_number() OVER () AS r FROM t) WHERE a=1;
SELECT a, r FROM (SELECT a, row_number() OVER (ORDER BY a) AS r FROM t) WHERE a=1;
SELECT a, c FROM (SELECT a, count(*) AS c FROM t GROUP BY a) WHERE a=1;
SELECT a, s FROM (SELECT a, sum(a) OVER (PARTITION BY a) AS s FROM t) WHERE a=1;
SELECT x FROM (SELECT count(*) AS x FROM t) WHERE x>0;
-- case: bug-hunt-10/sql/generate-series-text-start-and-pragma-xinfo
SELECT count(*) FROM generate_series('a',3);
SELECT name, hidden FROM pragma_table_xinfo('generate_series');
SELECT name FROM pragma_table_info('generate_series');
SELECT * FROM generate_series(1,2,3,4);
-- case: bug-hunt-10/sql/window-range-offset-needs-one-order-by-message
SELECT sum(1) OVER (ORDER BY 1,2 RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING);
SELECT sum(1) OVER (RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING);
