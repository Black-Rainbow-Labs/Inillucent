-- case: bug-hunt-11/planner/expression-index-range-in-between-order
CREATE TABLE ix(id INTEGER PRIMARY KEY, p TEXT, k INT, name TEXT);
INSERT INTO ix(p, k, name) VALUES ('{"k":5,"s":"b"}', 5, 'b'), ('{"k":1,"s":"a"}', 1, 'a'), ('{"k":9,"s":"C"}', 9, 'C'), ('{"k":null,"s":"d"}', NULL, 'd'), ('{"s":"e"}', NULL, 'e'), ('{"k":5,"s":"f"}', 5, 'f'), ('{"k":"x","s":"g"}', 7, 'g'), ('{"k":2.5,"s":"h"}', 2, 'h');
CREATE INDEX ixe ON ix(json_extract(p, '$.k'));
EXPLAIN QUERY PLAN SELECT id FROM ix WHERE json_extract(p, '$.k') > 2;
EXPLAIN QUERY PLAN SELECT id FROM ix WHERE json_extract(p, '$.k') BETWEEN 2 AND 6;
EXPLAIN QUERY PLAN SELECT id FROM ix WHERE json_extract(p, '$.k') IN (1, 9);
EXPLAIN QUERY PLAN SELECT id FROM ix ORDER BY json_extract(p, '$.k') LIMIT 3;
SELECT id FROM ix WHERE json_extract(p, '$.k') > 2 ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') >= 5 ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') < 5 ORDER BY id;
SELECT id FROM ix WHERE 5 <= json_extract(p, '$.k') ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') BETWEEN 2 AND 6 ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') NOT BETWEEN 2 AND 6 ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') IN (1, 9, 'x') ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') IN (9, 1, 9) ORDER BY id;
SELECT id FROM ix WHERE json_extract(p, '$.k') > NULL;
SELECT id FROM ix WHERE json_extract(p, '$.k') > 1 AND json_extract(p, '$.k') < 9 ORDER BY id;
SELECT id FROM ix ORDER BY json_extract(p, '$.k'), id LIMIT 4;
SELECT id FROM ix ORDER BY json_extract(p, '$.k') DESC, id DESC LIMIT 4;
SELECT id, name FROM ix ORDER BY json_extract(p, '$.k') LIMIT 3 OFFSET 2;
SELECT id FROM ix WHERE json_extract(p, '$.k') > 1 ORDER BY json_extract(p, '$.k') DESC, id LIMIT 3;
SELECT count(*) FROM ix WHERE json_extract(p, '$.k') BETWEEN 1 AND 9;
SELECT json_extract(p, '$.k') AS v FROM ix ORDER BY v LIMIT 3;
-- case: bug-hunt-11/planner/expression-index-second-key-range
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, p TEXT);
INSERT INTO t(a, p) VALUES (1, '{"v":3}'), (1, '{"v":8}'), (1, '{"v":12}'), (2, '{"v":4}'), (2, '{"v":9}'), (3, '{"v":1}');
CREATE INDEX tae ON t(a, json_extract(p, '$.v'));
EXPLAIN QUERY PLAN SELECT id FROM t WHERE a = 1 AND json_extract(p, '$.v') > 5;
EXPLAIN QUERY PLAN SELECT id FROM t WHERE a = 1 ORDER BY json_extract(p, '$.v') LIMIT 2;
SELECT id FROM t WHERE a = 1 AND json_extract(p, '$.v') > 5 ORDER BY id;
SELECT id FROM t WHERE a = 2 AND json_extract(p, '$.v') BETWEEN 4 AND 8 ORDER BY id;
SELECT id FROM t WHERE a = 1 ORDER BY json_extract(p, '$.v') DESC LIMIT 2;
SELECT id FROM t WHERE a IN (1, 2) AND json_extract(p, '$.v') > 3 ORDER BY id;
SELECT id FROM t WHERE a = 1 AND json_extract(p, '$.v') IN (3, 12) ORDER BY id;
-- case: bug-hunt-11/planner/expression-index-lower-text-range
CREATE TABLE u(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO u(name) VALUES ('Bob'), ('alice'), ('Carol'), ('dave'), ('ALICE'), (NULL), ('bob');
CREATE INDEX ul ON u(lower(name));
EXPLAIN QUERY PLAN SELECT id FROM u WHERE lower(name) >= 'b' AND lower(name) < 'd';
SELECT id FROM u WHERE lower(name) >= 'b' AND lower(name) < 'd' ORDER BY id;
SELECT id FROM u WHERE lower(name) BETWEEN 'alice' AND 'bob' ORDER BY id;
SELECT id FROM u WHERE lower(name) IN ('alice', 'dave') ORDER BY id;
SELECT id FROM u ORDER BY lower(name) LIMIT 4;
SELECT id FROM u ORDER BY lower(name) DESC, id LIMIT 4;
SELECT id, name FROM u ORDER BY lower(name), id LIMIT 3;
SELECT id FROM u WHERE lower(name) > 'b' COLLATE NOCASE ORDER BY id;
-- case: bug-hunt-11/planner/expression-index-covering-count
CREATE TABLE e(id INTEGER PRIMARY KEY, payload TEXT);
INSERT INTO e(payload) VALUES ('{"n":1}'), ('{"n":5}'), ('{"n":7}'), ('{"n":5}'), ('{"m":1}');
CREATE INDEX en ON e(json_extract(payload, '$.n'));
EXPLAIN QUERY PLAN SELECT count(*) FROM e WHERE json_extract(payload, '$.n') > 4;
SELECT count(*) FROM e WHERE json_extract(payload, '$.n') > 4;
SELECT count(*) FROM e WHERE json_extract(payload, '$.n') BETWEEN 1 AND 5;
SELECT count(*) FROM e WHERE json_extract(payload, '$.n') IN (1, 7);
SELECT id FROM e ORDER BY json_extract(payload, '$.n') LIMIT 3;
SELECT payload FROM e WHERE json_extract(payload, '$.n') > 4 ORDER BY id;
-- case: bug-hunt-11/planner/view-over-recursive-cte-with-limit
CREATE VIEW v1(term) AS WITH r(term, next) AS (SELECT 1, 1 UNION ALL SELECT next, term + next FROM r) SELECT term FROM r;
SELECT term FROM v1 LIMIT 5;
SELECT term FROM v1 WHERE term > 3 LIMIT 3;
SELECT count(*) FROM (SELECT term FROM v1 LIMIT 7);
-- case: bug-hunt-11/planner/view-with-cte-named-like-the-view
CREATE VIEW fib(term) AS WITH fib(term, next) AS (SELECT 1, 1 UNION ALL SELECT next, term + next FROM fib) SELECT term FROM fib;
SELECT term FROM fib LIMIT 5;
-- case: bug-hunt-11/planner/materialized-cte-over-a-limited-recursive-cte
WITH RECURSIVE fib(a, b) AS (SELECT 1, 1 UNION ALL SELECT b, a + b FROM fib), top10 AS MATERIALIZED (SELECT a FROM fib LIMIT 10) SELECT a FROM top10;
WITH RECURSIVE fib(a, b) AS (SELECT 1, 1 UNION ALL SELECT b, a + b FROM fib), top10 AS NOT MATERIALIZED (SELECT a FROM fib LIMIT 10) SELECT sum(a) FROM top10;
-- case: bug-hunt-11/planner/window-group-concat-with-outer-order-by
CREATE TABLE p(x TEXT);
WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i + 1 FROM n WHERE i < 8000) INSERT INTO p SELECT PRINTF('%020d', i) FROM n;
SELECT count(*), max(l) FROM (SELECT LENGTH(GROUP_CONCAT(x) OVER ()) AS l FROM p ORDER BY x);
-- case: bug-hunt-11/planner/correlated-exists-on-the-outer-row-only
CREATE TABLE t1(id, x);
CREATE TABLE t2(id, x, y, z);
INSERT INTO t1(id,x) VALUES(1,2);
WITH RECURSIVE c(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM c WHERE n<2500) INSERT INTO t2(id,x,y,z) SELECT n, 0, 1, 44 FROM c;
CREATE INDEX t1x ON t1(x);
CREATE INDEX t2y ON t2(y);
SELECT count(t1.id), count(t2.id) FROM t1, t2 WHERE t1.id = t2.y AND t1.x = 2 AND EXISTS(SELECT 1 FROM t2 WHERE t2.y = t1.id AND t2.z = 33);
-- case: bug-hunt-11/planner/recursive-cte-limit-is-a-scalar-subquery
WITH RECURSIVE p(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM p LIMIT (SELECT 3)) SELECT count(*) FROM p;
CREATE TABLE x(a);
INSERT INTO x VALUES(1),(2),(3);
WITH RECURSIVE p(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM p LIMIT (SELECT count(*)+1 FROM x)) SELECT count(*) FROM p;
-- case: bug-hunt-11/planner/uncorrelated-subquery-not-run-when-where-is-false
CREATE TABLE t(a);
INSERT INTO t VALUES (9223372036854775807), (1);
SELECT 'q4', 1 FROM t WHERE NULL AND (SELECT sum(a) FROM t);
SELECT 'q5', 1 FROM t WHERE 0 AND (SELECT sum(a) FROM t);
SELECT 'q6', count(*) FROM t WHERE 1 = 0 AND (SELECT sum(a) FROM t);
-- case: bug-hunt-11/planner/cte-over-a-view-whose-body-has-a-cte
CREATE VIEW fibv AS WITH RECURSIVE fib(a, b) AS (SELECT 1, 1 UNION ALL SELECT b, a + b FROM fib) SELECT a FROM fib;
WITH top AS MATERIALIZED (SELECT a FROM fibv LIMIT 4) SELECT sum(a) FROM top;
WITH fib AS MATERIALIZED (SELECT a FROM fibv LIMIT 4) SELECT sum(a) FROM fib;
WITH top AS (SELECT a FROM fibv LIMIT 4) SELECT sum(a) FROM top;
SELECT sum(a) FROM (SELECT a FROM fibv LIMIT 4);
-- case: bug-hunt-11/planner/derived-recursive-cte-with-limit-and-offset
SELECT n FROM (WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r) LIMIT 3 OFFSET 2;
SELECT n FROM (WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r LIMIT 4) LIMIT 10;
SELECT n FROM (WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r LIMIT 20) LIMIT 2;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r), w AS (SELECT n * 2 AS d FROM r) SELECT d FROM w LIMIT 3;
-- case: bug-hunt-11/planner/recursive-cte-limit-and-offset-subqueries
CREATE TABLE x(a);
INSERT INTO x VALUES(1),(2),(3);
WITH RECURSIVE p(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM p LIMIT 10 OFFSET (SELECT 2)) SELECT min(n), max(n) FROM p;
WITH RECURSIVE p(n) AS (SELECT (SELECT 5) UNION ALL SELECT n+(SELECT 1) FROM p WHERE n < (SELECT 8)) SELECT sum(n) FROM p;
-- case: bug-hunt-11/planner/constant-where-terms-stop-at-the-first-false-one
CREATE TABLE t(a);
INSERT INTO t VALUES (9223372036854775807), (1);
SELECT 1 FROM t WHERE NULL AND (SELECT sum(a) FROM t);
SELECT 1 FROM t WHERE 0 AND (SELECT sum(a) FROM t);
SELECT count(*) FROM t WHERE 1 = 0 AND (SELECT sum(a) FROM t);
SELECT count(*) FROM t WHERE 1 AND (SELECT sum(a) FROM t);
-- case: bug-hunt-11/planner/constant-where-term-after-a-true-one-still-raises
CREATE TABLE t(a);
INSERT INTO t VALUES (9223372036854775807), (1);
SELECT a FROM t WHERE a > 0 AND NULL AND (SELECT sum(a) FROM t);
SELECT a FROM t WHERE (SELECT sum(a) FROM t) AND 0;
-- case: bug-hunt-11/planner/nested-cte-flattens-past-the-from-term-limit
WITH RECURSIVE c AS NOT MATERIALIZED (WITH RECURSIVE c AS NOT MATERIALIZED (WITH RECURSIVE c AS NOT MATERIALIZED (WITH RECURSIVE c AS NOT MATERIALIZED (WITH c AS (VALUES(0)) SELECT 1 FROM c LEFT JOIN c ON ltrim(1)) SELECT 1 FROM c,c,c,c,c,c,c,c,c) SELECT 2 FROM c,c,c,c,c,c,c,c,c) SELECT 3 FROM c,c,c,c,c,c,c,c,c) SELECT 4 FROM c,c,c,c,c,c,c,c,c;
-- case: bug-hunt-11/planner/nested-cte-below-the-from-term-limit
WITH c AS NOT MATERIALIZED (WITH c AS NOT MATERIALIZED (WITH c AS NOT MATERIALIZED (VALUES(0)) SELECT 1 FROM c,c,c,c) SELECT 2 FROM c,c,c,c) SELECT 3 FROM c,c,c,c;
WITH c AS NOT MATERIALIZED (WITH c AS NOT MATERIALIZED (WITH c AS NOT MATERIALIZED (VALUES(0)) SELECT 1 FROM c,c,c,c,c,c) SELECT 2 FROM c,c,c,c,c,c) SELECT 3 FROM c,c,c,c,c,c;
-- case: bug-hunt-11/planner/join-wider-than-64-tables
WITH c AS NOT MATERIALIZED (WITH c AS NOT MATERIALIZED (VALUES(0)) SELECT 1 FROM c,c,c,c,c,c,c,c,c) SELECT 2 FROM c,c,c,c,c,c,c,c,c;
