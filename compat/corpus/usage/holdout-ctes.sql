-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/ctes/126-union-all-of-aggregate-selects-keeps-each-branch-s
CREATE TABLE t(id INT, val INT);
INSERT INTO t VALUES (1,10),(2,20),(3,30);
WITH c AS (SELECT 'a' as label, MIN(val) as v FROM t UNION ALL SELECT 'b', MAX(val) FROM t) SELECT * FROM c;
WITH c AS (SELECT 1 as label, MIN(val) as v FROM t UNION ALL SELECT 2, MAX(val) FROM t UNION ALL SELECT 3, SUM(val) FROM t) SELECT * FROM c;
-- case: holdout/ctes/187-cte-named-like-a-table-that-reads-the-schema-quali
CREATE TABLE t1(a INTEGER);
INSERT INTO t1 VALUES(1),(2),(3);
WITH t1 AS (SELECT a * 10 AS a FROM main.t1) SELECT * FROM t1 ORDER BY a;
WITH t1 AS (SELECT a * 10 AS a FROM t1) SELECT * FROM t1 ORDER BY a;
-- case: holdout/ctes/252-not-materialized-cte-is-evaluated-at-each-referenc
WITH cte AS NOT MATERIALIZED (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
WITH cte AS MATERIALIZED (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
WITH cte AS (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
-- case: holdout/ctes/276-runtime-error-in-returning-escape-longer-than-one-
CREATE TABLE t(a INT);
INSERT INTO t VALUES(1),(2);
BEGIN;
DELETE FROM t WHERE a=1 RETURNING 'x' LIKE 'x' ESCAPE 'yy';
PRAGMA integrity_check;
SELECT * FROM t ORDER BY a;
COMMIT;
CREATE TABLE u(a INT UNIQUE, b INT);
INSERT INTO u VALUES(1,10);
BEGIN;
INSERT OR REPLACE INTO u VALUES(1,20) RETURNING 'x' LIKE 'x' ESCAPE 'yy';
SELECT rowid,a,b FROM u;
COMMIT;
-- case: holdout/ctes/380-explain-query-plan-text-for-a-materialized-cte-ref
CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, city_id INTEGER);
CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, amount REAL);
EXPLAIN QUERY PLAN WITH spenders AS MATERIALIZED (SELECT user_id, sum(amount) AS total FROM orders GROUP BY user_id) SELECT u.name, s1.total FROM users u JOIN spenders s1 ON s1.user_id = u.id JOIN spenders s2 ON s2.user_id = u.city_id;
-- case: holdout/ctes/413-a-subquery-s-from-name-is-not-captured-by-the-dele
CREATE TABLE t(a INT);
CREATE TABLE u(a INT);
INSERT INTO t VALUES(1),(2),(3);
INSERT INTO u VALUES(2);
DELETE FROM t AS u WHERE a IN (SELECT a FROM u);
SELECT count(*) FROM t;
CREATE TABLE w(a INT);
INSERT INTO w VALUES(5),(6);
WITH w(a) AS (SELECT 9) UPDATE w SET a = (SELECT a FROM w);
SELECT a FROM w ORDER BY a;
-- case: holdout/ctes/483-a-cte-that-shadows-an-outer-cte-of-the-same-name-i
WITH q AS (WITH a(v) AS (VALUES(4)) SELECT v FROM a), a AS (SELECT v FROM q) SELECT v FROM a;
-- case: holdout/ctes/543-like-escape-where-the-escape-character-is-a-litera
select 'a' like 'a' escape 'a';
select 'a' like 'aa' escape 'a';
select 'a%' like 'a!%' escape '!', 'ab' like 'a!%' escape '!', 'a!' like 'a!!' escape '!', 'a_' like 'a\_' escape '\', 'ab' like 'a\_' escape '\';
select 'a' like 'a' escape '';
select 'a' like 'a' escape 'ab';
select 'abc' like 'ab%' escape NULL;
-- case: holdout/ctes/548-unicode-of-a-character-outside-the-bmp
SELECT unicode('😊');
SELECT unicode('é'), unicode('€'), unicode('a'), unicode(''), hex(char(128522)), char(128522), unicode(char(128522));
-- case: holdout/ctes/579-length-stops-at-the-first-nul-character-for-text
CREATE TABLE t(s TEXT);
INSERT INTO t VALUES ('a' || char(0) || 'b');
SELECT length(s), hex(s), typeof(s), length(CAST(s AS BLOB)), octet_length(s) FROM t;
SELECT length('a' || char(0) || 'b'), length(x'610062'), length(CAST('a' || char(0) || 'b' AS BLOB)), upper('a' || char(0) || 'b') = 'A' || char(0) || 'B', hex(upper('a' || char(0) || 'b')), 'a' || char(0) || 'b' = 'a', instr('a' || char(0) || 'b', 'b'), hex(substr('a' || char(0) || 'b', 3)), hex(replace('a' || char(0) || 'b', 'b', 'c'));
-- case: holdout/ctes/592-a-default-expression-that-names-a-column-is-reject
create table t(a, b default (coalesce(`a`, `b`)));
create table t2(a, b default (a));
create table t3(a, b default (1 + 1));
create table t4(a, b default (abs(-1)));
create table t5(a, b default ((a)));
-- case: holdout/ctes/604-cte-referenced-from-a-later-arm-of-a-compound-sele
with t as (select * from (values(2))) values(1) union all select * from t;
with t as (select 1) select 1 limit (select * from t);
with t as (select 1) select 1 limit 1 offset (select * from t) - 1;
with t(x) as (select 1) select x from t union select x + 1 from t order by 1;
-- case: holdout/ctes/629-instr-counts-characters-not-bytes
SELECT instr('日本語test', 'test');
SELECT instr('abc', 'c'), instr('abc', ''), instr('', ''), instr('', 'a'), instr(NULL, 'a'), instr('é', 'é'), instr('aé', 'é'), instr(x'414243', x'42'), instr(x'C3A9', 'é'), instr('abc', x'62'), instr(12345, 34), instr('abc', 98);
-- case: holdout/ctes/631-chain-of-seven-ctes-that-each-select-from-the-prev
CREATE TABLE t (x INT);
INSERT INTO t VALUES (1);
WITH c1 AS (SELECT x FROM t), c2 AS (SELECT x FROM c1), c3 AS (SELECT x FROM c2), c4 AS (SELECT x FROM c3), c5 AS (SELECT x FROM c4), c6 AS (SELECT x FROM c5), c7 AS (SELECT x FROM c6), c8 AS (SELECT x FROM c7), c9 AS (SELECT x FROM c8), c10 AS (SELECT x FROM c9) SELECT * FROM c10;
-- case: holdout/ctes/696-unresolved-column-inside-a-cte-of-a-subquery-that-
CREATE TABLE x(a, b);
INSERT INTO x(a) VALUES (1);
CREATE TABLE z(q INTEGER, p INTEGER);
CREATE TABLE y(id INTEGER PRIMARY KEY, d INTEGER);
UPDATE x SET b = 1 WHERE 1 OR (SELECT d FROM y ORDER BY id, MAX(0, d, EXISTS (WITH c AS (SELECT * FROM z WHERE p) SELECT DISTINCT c0 FROM c WHERE c0 ORDER BY c0)) LIMIT 1) RETURNING a, b;
SELECT a, b FROM x;
-- case: holdout/ctes/757-limit-inside-and-outside-a-recursive-cte
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c LIMIT 5) SELECT x FROM c;
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c) SELECT x FROM c LIMIT 3;
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c LIMIT 5 OFFSET 2) SELECT x FROM c;
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c ORDER BY 1 DESC LIMIT 4) SELECT x FROM c;
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 100 LIMIT -1) SELECT count(*) FROM c;
-- case: holdout/ctes/814-an-unclosed-or-odd-character-class-in-glob
SELECT 'abc' GLOB '[a', 'a' GLOB '[a', '[a' GLOB '[a', 'abc' GLOB 'a[', 'a' GLOB '[]', 'a]' GLOB 'a[]]', ']' GLOB '[]]', 'a' GLOB '[^]]', 'b' GLOB '[a-c]', 'b' GLOB '[c-a]', '-' GLOB '[a-]', 'a' GLOB '[^a]', '^' GLOB '[\^]';
