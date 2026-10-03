-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/aggregates/2-count-distinct-text-turso-returned-3
create table temp2 (t1 text);
insert into temp2 values ('1'),('1'),('2');
select count(*) from temp2;
select count(distinct t1) from temp2;
-- case: holdout/aggregates/15-min-max-over-mixed-text-null-and-integers-inside-c
CREATE TABLE test1(f1,f2);
INSERT INTO test1 VALUES(11,22);
INSERT INTO test1 VALUES(33,44);
CREATE TABLE t3(a,b);
INSERT INTO t3 VALUES('abc',NULL);
INSERT INTO t3 VALUES(NULL,'xyz');
INSERT INTO t3 SELECT * FROM test1;
SELECT coalesce(min(a),'xyzzy') FROM t3;
SELECT coalesce(max(a),'xyzzy') FROM t3;
SELECT min(a), max(a), min(b), max(b) FROM t3;
-- case: holdout/aggregates/21-having-without-group-by-using-a-result-column-alia
create table t(a);
insert into t values (1), (2), (3), (4), (5);
select sum(a) s from t having s = 14;
select sum(a) s from t having s = 15;
-- case: holdout/aggregates/44-sum-of-a-text-value-that-looks-like-a-real-turso-r
CREATE TABLE c(c1_0 TEXT);
INSERT INTO c VALUES (CAST(-99.13 AS TEXT));
SELECT c1_0, SUM(c1_0) AS total FROM c GROUP BY c1_0 ORDER BY 1;
SELECT sum(0.1), sum(x), total(x) FROM (SELECT 0.1 x UNION ALL SELECT 0.2 UNION ALL SELECT 0.3);
-- case: holdout/aggregates/71-aggregate-query-with-limit-0-returns-no-rows
CREATE TABLE t (id INTEGER, col INTEGER);
INSERT INTO t VALUES (1, 10), (2, 20);
SELECT COUNT(*) FROM t LIMIT 0;
SELECT COUNT(*) FROM t LIMIT 1 OFFSET 1;
SELECT COUNT(*) FROM t LIMIT -1;
-- case: holdout/aggregates/78-duplicate-order-by-expressions-with-group-by
CREATE TABLE t(a);
INSERT INTO t VALUES (2), (1), (2);
SELECT a FROM t GROUP BY a ORDER BY a, a;
SELECT a, count(*) FROM t GROUP BY a ORDER BY 1, 1 DESC, a;
-- case: holdout/aggregates/80-sum-whose-partial-sums-overflow-to-infinity
SELECT sum(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT 0);
SELECT total(x), avg(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT 0);
SELECT sum(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT -1e308);
SELECT sum(x) FROM (SELECT 9223372036854775807 x UNION ALL SELECT 1);
SELECT total(x) FROM (SELECT 9223372036854775807 x UNION ALL SELECT 1);
-- case: holdout/aggregates/95-constant-expressions-in-the-select-list-or-order-b
CREATE TABLE t(name TEXT);
INSERT INTO t VALUES ('a'), ('a'), ('b');
SELECT 'label:', name FROM t GROUP BY name;
SELECT 42, name FROM t GROUP BY name;
SELECT 1+1, name FROM t GROUP BY name;
CREATE TABLE v0 (c1 INT);
INSERT INTO v0 VALUES (1);
SELECT * FROM v0 GROUP BY c1 ORDER BY 58.058;
SELECT * FROM v0 GROUP BY c1 ORDER BY 'hello';
SELECT * FROM v0 GROUP BY c1 ORDER BY NULL;
WITH cte AS (SELECT 1 GROUP BY 1.0 ORDER BY 1.0) SELECT * FROM cte;
-- case: holdout/aggregates/115-avg-over-the-text-returned-by-hex
CREATE TABLE v0 ( c1 );
INSERT INTO v0 VALUES ( hex(18446744073709551488) );
SELECT AVG(c1), typeof(c1) FROM v0;
DELETE FROM v0;
INSERT INTO v0 VALUES ( hex(42) );
SELECT AVG(c1), SUM(c1), TOTAL(c1), typeof(c1), c1 FROM v0;
-- case: holdout/aggregates/127-bare-columns-next-to-max-come-from-the-row-holding
CREATE TABLE t(id INT, val INT, name TEXT);
INSERT INTO t VALUES (1,10,'first'),(2,30,'second'),(3,20,'third');
SELECT id, name, MAX(val) FROM t;
SELECT id, name, MIN(val) FROM t;
SELECT id, name, MAX(val), MIN(val) FROM t;
SELECT id, name, MAX(val), count(*) FROM t;
-- case: holdout/aggregates/151-group-by-name-that-matches-both-a-result-alias-and
CREATE TABLE t(x INT, y INT, z INT);
INSERT INTO t VALUES (1,10,100),(2,10,200),(3,20,300),(4,20,400);
SELECT x as y, SUM(z) FROM t GROUP BY y ORDER BY y;
SELECT x as y, SUM(z) FROM t GROUP BY y ORDER BY x;
SELECT y as x, SUM(z) FROM t GROUP BY x ORDER BY 1;
SELECT x+100 as y2, SUM(z) FROM t GROUP BY y2 ORDER BY 1;
-- case: holdout/aggregates/339-having-without-group-by-that-names-a-bare-column
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
SELECT count(*) FROM t HAVING a!=400;
SELECT count(*), a FROM t HAVING a = 5;
SELECT count(*) FROM t HAVING max(a) = 5;
SELECT count(*) FROM t HAVING 1;
SELECT count(*) FROM t HAVING 0;
-- case: holdout/aggregates/414-having-n-where-n-is-both-an-output-alias-and-a-tab
CREATE TABLE t(k TEXT, n INTEGER);
INSERT INTO t VALUES('a',3),('a',4),('a',5),('b',20);
SELECT k, sum(n) AS n FROM t GROUP BY k HAVING n >= 10;
SELECT k, sum(n) FROM t GROUP BY k HAVING n >= 10;
DELETE FROM t WHERE k IN (SELECT k FROM (SELECT k, sum(n) AS n FROM t GROUP BY k HAVING n >= 10));
SELECT count(*) FROM t;
-- case: holdout/aggregates/460-group-by-lower-x-with-order-by-lower-x-first
CREATE TABLE t(x TEXT, z INT);
INSERT INTO t VALUES('C',1),('A',2),('B',3);
SELECT min(x) FROM t GROUP BY lower(x) ORDER BY lower(x), min(z);
SELECT min(x) FROM t GROUP BY lower(x) ORDER BY lower(x) DESC, min(z);
SELECT min(x) FROM t GROUP BY x COLLATE NOCASE ORDER BY x COLLATE NOCASE, min(z) LIMIT 1;
-- case: holdout/aggregates/475-sum-of-text-values-that-overflow-64-bits-switches-
CREATE TABLE p(price);
INSERT INTO p VALUES ('9223372036854775807'), ('1'), ('0.5');
SELECT sum(price) FROM p;
CREATE TABLE q(price);
INSERT INTO q VALUES (9223372036854775807), (1);
SELECT sum(price) FROM q;
SELECT total(price), avg(price) FROM q;
CREATE TABLE r(price);
INSERT INTO r VALUES ('9223372036854775807'), ('1');
SELECT sum(price) FROM r;
-- case: holdout/aggregates/495-expression-that-wraps-the-group-by-expression
CREATE TABLE sales(product TEXT, amount REAL);
INSERT INTO sales VALUES ('widget', 10), ('Widget', 25), ('gadget', 45);
SELECT UPPER(product), SUM(amount) FROM sales GROUP BY UPPER(product) ORDER BY 1;
SELECT UPPER(product) || '', SUM(amount) FROM sales GROUP BY UPPER(product) ORDER BY 1;
SELECT COALESCE(UPPER(product), 'x'), SUM(amount) FROM sales GROUP BY UPPER(product) ORDER BY 1;
SELECT length(UPPER(product)), SUM(amount) FROM sales GROUP BY UPPER(product) ORDER BY 1;
-- case: holdout/aggregates/506-having-on-a-group-by-expression-that-is-not-in-the
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t1 VALUES (1, 2, 'y'), (2, 5, 'xy'), (3, 3, 'z');
SELECT c FROM t1 GROUP BY c, (a | 3) HAVING (a | 3) > 1 ORDER BY 1;
SELECT c FROM t1 GROUP BY c, a + b HAVING a + b > 4 ORDER BY 1;
SELECT c, (a | 3) FROM t1 GROUP BY c, (a | 3) HAVING (a | 3) > 1 ORDER BY 1;
-- case: holdout/aggregates/569-offset-applies-to-a-one-row-aggregate-result
create table t(a);
insert into t values (1), (2);
select sum(a) from t limit 10 offset 10;
select sum(a) from t group by a limit 10 offset 10;
select count(*) from t limit 10 offset 1;
select count(*) from t limit 1 offset 0;
select sum(a) from t limit -1 offset 0;
-- case: holdout/aggregates/571-group-concat-ignores-null-values
create table t(a);
insert into t values ('a'), (''), ('b'), (null), ('c');
select group_concat(a) from t;
select group_concat(a, '-'), group_concat(a, NULL), group_concat(DISTINCT a), group_concat(a, a) from t;
select group_concat(1), group_concat(1.5, 2), group_concat(x'41'), group_concat(NULL), group_concat(a) FILTER (WHERE a <> 'b') FROM t;
-- case: holdout/aggregates/583-avg-ignores-null-values
create table avg_nulls(val);
insert into avg_nulls(val) values (1), (NULL), (3);
select avg(val), sum(val), count(val), count(*), total(val) from avg_nulls;
select avg(val) from avg_nulls where val is null;
select avg(x), typeof(avg(x)) from (select 4 x union all select 4);
-- case: holdout/aggregates/588-bare-columns-next-to-max-and-min-with-group-by
create table t(a,b,c);
insert into t values (1, 'a', 'a'), (1, 'b', 'b'), (1, 'c', 'c');
select a, b, max(c) from t group by a;
select a, b, min(c) from t group by a;
insert into t values (2, 'x', 'z'), (2, 'y', 'a');
select a, b, max(c) from t group by a order by a;
select a, b, min(c) from t group by a order by a;
select a, b, max(c), min(c) from t group by a order by a;
select a, b, max(c), count(*) from t group by a order by a;
-- case: holdout/aggregates/654-bare-columns-in-an-aggregate-query-have-no-pragma-
pragma only_full_group_by=true;
CREATE TABLE orders(id INTEGER PRIMARY KEY, customer_id INT, status TEXT, amount INT);
CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT, email TEXT);
INSERT INTO customers VALUES (1, 'a', 'a@x'), (2, 'b', 'b@x');
INSERT INTO orders VALUES (1, 1, 'new', 10), (2, 1, 'old', 20), (3, 2, 'new', 5);
SELECT o.customer_id, c.name, c.email, o.status, SUM(o.amount) FROM orders o JOIN customers c ON c.id = o.customer_id GROUP BY o.customer_id ORDER BY 1;
-- case: holdout/aggregates/674-bare-columns-with-max-and-min-over-groups
CREATE TABLE scores(id INTEGER PRIMARY KEY, player TEXT, game TEXT, score INTEGER);
INSERT INTO scores VALUES(1, 'Alice', 'chess', 50);
INSERT INTO scores VALUES(2, 'Alice', 'go', 80);
INSERT INTO scores VALUES(3, 'Alice', 'checkers', 30);
INSERT INTO scores VALUES(4, 'Bob', 'chess', 90);
INSERT INTO scores VALUES(5, 'Bob', 'go', 20);
INSERT INTO scores VALUES(6, 'Bob', 'checkers', 70);
SELECT player, game, MAX(score) FROM scores GROUP BY player ORDER BY player;
SELECT player, game, MIN(score) FROM scores GROUP BY player ORDER BY player;
SELECT player, game, MAX(score), MIN(score) FROM scores GROUP BY player ORDER BY player;
SELECT game, MAX(score) FROM scores;
-- case: holdout/aggregates/677-group-by-a-literal-with-having
SELECT 1 AS x GROUP BY 1 HAVING 1;
SELECT 1 AS x GROUP BY 1 HAVING 0;
SELECT 1 GROUP BY 1 HAVING 1;
SELECT 42 GROUP BY 1 HAVING 1;
SELECT 1 HAVING 1;
SELECT 1 HAVING 0;
SELECT 5 GROUP BY 'a';
-- case: holdout/aggregates/680-avg-of-large-integers-keeps-full-precision
SELECT avg(x) FROM (SELECT 9007199254740994 AS x UNION ALL SELECT -9007199254740993);
SELECT avg(x), sum(x), total(x) FROM (SELECT 9007199254740993 AS x UNION ALL SELECT 1);
SELECT avg(x) FROM (SELECT 9223372036854775807 AS x UNION ALL SELECT 9223372036854775807);
SELECT sum(x), avg(x) FROM (SELECT 0.1 AS x UNION ALL SELECT 0.2 UNION ALL SELECT 0.3 UNION ALL SELECT 1e16 UNION ALL SELECT -1e16);
-- case: holdout/aggregates/683-a-huge-integer-literal-in-order-by-or-group-by-is-
SELECT 1 ORDER BY 2147483648;
SELECT count(*) FROM (VALUES (1),(2),(3)) GROUP BY 2147483648;
SELECT 1 UNION ALL SELECT 2 ORDER BY 2147483648;
SELECT 1 ORDER BY 2;
SELECT 1 ORDER BY 0;
SELECT 1 ORDER BY -1;
SELECT 1 GROUP BY 2;
SELECT 1 GROUP BY 0;
SELECT 1 ORDER BY 9223372036854775807;
-- case: holdout/aggregates/711-having-without-group-by-that-names-a-table-column
CREATE TABLE t1 (log int);
INSERT INTO t1 VALUES (1), (2), (3);
SELECT count(*) FROM t1 HAVING log != 400;
SELECT count(*) FROM t1 HAVING log = 400;
SELECT count(*), log FROM t1 HAVING log = 3;
SELECT count(*) FROM t1 HAVING log IS NULL;
-- case: holdout/aggregates/726-function-wrapped-around-the-group-by-expression
CREATE TABLE t(a); INSERT INTO t VALUES (1),(2),(2);
SELECT abs(a+0), count(*) FROM t GROUP BY a+0 ORDER BY 1;
SELECT count(*) FROM t GROUP BY a+0 HAVING abs(a+0) > 1;
SELECT a+0, abs(a+0)*2, count(*) FROM t GROUP BY a+0 ORDER BY 1;
-- case: holdout/aggregates/729-sum-of-text-values-that-overflow-switches-to-a-flo
CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT);
INSERT INTO t (a) VALUES ('9223372036854775807');
INSERT INTO t (a) VALUES ('9223372036854775807');
SELECT sum(a) FROM t;
CREATE TABLE u (a INTEGER);
INSERT INTO u VALUES (9223372036854775807), (9223372036854775807);
SELECT sum(a) FROM u;
CREATE TABLE v (a);
INSERT INTO v VALUES (9223372036854775807), ('9223372036854775807');
SELECT sum(a) FROM v;
-- case: holdout/aggregates/742-coalesce-of-an-aggregate-and-a-literal-without-gro
CREATE TABLE schema_migrations (version INTEGER NOT NULL);
SELECT COALESCE(MAX(version), 0) FROM schema_migrations;
INSERT INTO schema_migrations (version) VALUES (1);
INSERT INTO schema_migrations (version) VALUES (2);
SELECT COALESCE(MAX(version), 0) FROM schema_migrations;
SELECT MAX(version) + 1, COALESCE(SUM(version), 0) * 2, ifnull(min(version), -1), count(*) || 'x' FROM schema_migrations;
-- case: holdout/aggregates/776-having-resolves-a-name-to-the-table-column-before-
CREATE TABLE t(g, v);
INSERT INTO t VALUES (1, 10), (1, 20), (2, 5);
SELECT g, sum(v) AS v FROM t GROUP BY g HAVING v > 6 ORDER BY g;
SELECT g, sum(v) AS v FROM t GROUP BY g HAVING sum(v) > 6 ORDER BY g;
SELECT g, sum(v) AS s FROM t GROUP BY g HAVING s > 6 ORDER BY g;
-- case: holdout/aggregates/797-bare-column-in-having-without-group-by
CREATE TABLE t(x);
INSERT INTO t VALUES (1), (2), (3);
SELECT sum(x) FROM t HAVING x = 1;
SELECT sum(x) FROM t HAVING x = 3;
SELECT sum(x), x FROM t;
SELECT sum(x), x FROM t HAVING x = 3;
-- case: holdout/aggregates/798-bare-column-next-to-an-aggregate-without-group-by
CREATE TABLE t(x);
INSERT INTO t VALUES (1), (2), (3);
SELECT sum(x), x FROM t;
SELECT count(*), x FROM t;
SELECT x, sum(x) FROM t;
SELECT avg(x), x FROM t;
SELECT total(x), x FROM t;
CREATE INDEX tx ON t(x DESC);
SELECT sum(x), x FROM t;
-- case: holdout/aggregates/802-aggregate-filter-inside-having-of-a-grouped-query
CREATE TABLE t(g, x);
INSERT INTO t VALUES (1, 1), (1, 5), (2, 1), (2, 2), (3, 9);
SELECT g FROM t GROUP BY g HAVING sum(x) FILTER (WHERE x > 3) > 0 ORDER BY g;
SELECT g FROM t GROUP BY g HAVING count(*) FILTER (WHERE x = 1) = 1 ORDER BY g;
SELECT g FROM t GROUP BY g HAVING max(x) FILTER (WHERE x < 3) = 2 ORDER BY g;
SELECT g, sum(x) FILTER (WHERE x > 100) FROM t GROUP BY g HAVING sum(x) FILTER (WHERE x > 100) IS NULL ORDER BY g;
-- case: holdout/aggregates/803-aggregate-filter-inside-having-without-group-by
CREATE TABLE t(x);
INSERT INTO t VALUES (1), (2), (3);
SELECT count(*) FROM t HAVING sum(x) FILTER (WHERE x > 2) = 3;
SELECT count(*) FROM t HAVING sum(x) FILTER (WHERE x > 5) IS NULL;
SELECT count(*) FROM t HAVING count(*) FILTER (WHERE x > 1) = 2;
SELECT count(*) FROM t HAVING sum(x) FILTER (WHERE x > 5) > 0;
-- case: holdout/aggregates/820-unknown-functions-in-group-by-and-having-are-error
CREATE TABLE t(a, x);
INSERT INTO t VALUES (1, 1), (2, 2);
SELECT count(*) FROM t GROUP BY ROLLUP(a);
SELECT count(*) FROM t GROUP BY CUBE(a);
SELECT count(*) FROM t GROUP BY made_up(a);
SELECT count(*) FROM t HAVING made_up_function(x);
SELECT count(*) FROM t WHERE made_up_function(x);
SELECT made_up_function(1) WHERE 0;
