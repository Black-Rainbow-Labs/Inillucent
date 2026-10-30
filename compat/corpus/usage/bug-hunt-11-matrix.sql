-- case: bug-hunt-11/aggregate/distinct-sum-in-union-in-derived-table-in-having-subquery
CREATE TABLE t0(c1,c2);
CREATE TABLE t1(c3,c4);
INSERT INTO t0 VALUES(1,2);
INSERT INTO t1 VALUES(1,1);
SELECT count(*) FROM t0 WHERE EXISTS(SELECT 1 FROM t1 GROUP BY c3 HAVING (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(DISTINCT c1))));
-- case: bug-hunt-11/aggregate/outer-column-aggregate-in-union-arm-of-derived-table-counts-each-group
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT c3, (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c4))) FROM t1 GROUP BY c3 ORDER BY c3;
-- case: bug-hunt-11/aggregate/outer-column-aggregate-value-read-from-derived-table-union
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT c3, (SELECT max(x) FROM (SELECT 1 AS x UNION ALL SELECT SUM(c4))) FROM t1 GROUP BY c3 ORDER BY c3;
-- case: bug-hunt-11/aggregate/outer-column-aggregate-in-compound-subquery-without-group-by
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c4))) FROM t1;
-- case: bug-hunt-11/aggregate/outer-column-aggregate-in-union-in-having-keeps-or-drops-groups
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT c3 FROM t1 GROUP BY c3 HAVING (SELECT max(x) FROM (SELECT 1 AS x UNION ALL SELECT SUM(c4))) > 3 ORDER BY c3;
-- case: bug-hunt-11/aggregate/count-in-union-arm-of-scalar-subquery-in-having
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT c3 FROM t1 GROUP BY c3 HAVING (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT count(c4))) = 2 ORDER BY c3;
-- case: bug-hunt-11/aggregate/outer-aggregate-in-nested-derived-table
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1),(1,2),(2,5);
SELECT c3, (SELECT COUNT(*) FROM (SELECT * FROM (SELECT 1 UNION ALL SELECT SUM(c4)))) FROM t1 GROUP BY c3 ORDER BY c3;
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-of-non-aggregate-owner-in-where-is-dropped
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1);
SELECT c4 FROM t1 WHERE (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c4))) > 0;
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-of-grouped-owner-in-where-is-dropped
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1);
SELECT c3 FROM t1 WHERE (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c4))) > 0 GROUP BY c3;
-- case: bug-hunt-11/aggregate/misuse-direct-sum-in-where-subquery
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1);
SELECT c3 FROM t1 WHERE (SELECT SUM(c4)) > 0 GROUP BY c3;
-- case: bug-hunt-11/aggregate/misuse-direct-sum-in-derived-table-without-aggregate-owner
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1);
SELECT (SELECT COUNT(*) FROM (SELECT SUM(c4))) FROM t1 WHERE 1;
-- case: bug-hunt-11/aggregate/misuse-read-union-arm-aggregate-in-where-subquery
CREATE TABLE t1(c3,c4);
INSERT INTO t1 VALUES(1,1);
SELECT c4 FROM t1 WHERE (SELECT max(x) FROM (SELECT 1 AS x UNION ALL SELECT SUM(c4))) > 0;
-- case: bug-hunt-11/aggregate/misuse-read-union-arm-aggregate-in-exists-having
CREATE TABLE t0(c1,c2);
CREATE TABLE t1(c3,c4);
INSERT INTO t0 VALUES(1,2);
INSERT INTO t1 VALUES(1,1);
SELECT count(*) FROM t0 WHERE EXISTS(SELECT 1 FROM t1 GROUP BY c3 HAVING (SELECT max(x) FROM (SELECT 1 AS x UNION ALL SELECT SUM(c1))) > 0);
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-keeps-non-aggregate-owner-rows
CREATE TABLE t0(c1,c2);
INSERT INTO t0 VALUES(1,2),(5,6);
SELECT c1, (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c1))) FROM t0 ORDER BY c1;
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-in-aggregate-owner-result
CREATE TABLE t0(c1,c2);
INSERT INTO t0 VALUES(1,2),(5,6);
SELECT count(*), (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c1))) FROM t0;
-- case: bug-hunt-11/aggregate/read-union-arm-aggregate-in-aggregate-owner-result
CREATE TABLE t0(c1,c2);
INSERT INTO t0 VALUES(1,2),(5,6);
SELECT count(*), (SELECT max(x) FROM (SELECT 1 AS x UNION ALL SELECT SUM(c1))) FROM t0;
-- case: bug-hunt-11/aggregate/three-arm-union-with-grouped-and-outer-aggregates-in-exists-having
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT count(*) FROM t0 WHERE EXISTS(SELECT 1 FROM t1 GROUP BY c3 HAVING (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c3) UNION ALL SELECT SUM(c1))));
-- case: bug-hunt-11/aggregate/outer-aggregate-in-union-in-having-of-in-subquery
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT c1 FROM t0 WHERE c1 IN (SELECT c3 FROM t1 GROUP BY c3 HAVING (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(DISTINCT c1))));
-- case: bug-hunt-11/aggregate/outer-avg-in-union-arm-in-exists-where-subquery
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT c1 FROM t0 WHERE EXISTS(SELECT 1 FROM t1 WHERE (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT avg(c1) UNION ALL SELECT 3)) = 3);
-- case: bug-hunt-11/aggregate/union-arm-aggregates-of-two-owners-in-grouped-result
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT c3, (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT SUM(c3) UNION ALL SELECT max(c1))) FROM t1 GROUP BY c3;
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-in-order-by-subquery
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT c1 FROM t0 ORDER BY (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT sum(c1)));
-- case: bug-hunt-11/aggregate/unread-union-arm-aggregate-in-having-subquery-of-grouped-owner
CREATE TABLE t0(c1,c2); CREATE TABLE t1(c3,c4); INSERT INTO t0 VALUES(1,2),(5,6); INSERT INTO t1 VALUES(1,1),(1,2);
SELECT c1 FROM t0 GROUP BY c1 HAVING (SELECT COUNT(*) FROM (SELECT 1 UNION ALL SELECT sum(c2)));
