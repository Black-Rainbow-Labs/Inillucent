-- Regression cases for statement behaviour fixed by the bug hunt of October 2026, from SQLite's TCL tests.
-- See tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/statements/drop_table_keeps_trigger_of_same_name
-- DROP TABLE removes only the triggers on the table, not a trigger on another table that shares its name
CREATE TABLE tbl(a, b);
CREATE TABLE log(x);
CREATE TABLE t1(a, b);
CREATE TRIGGER t1 AFTER INSERT ON tbl BEGIN INSERT INTO log VALUES('fired'); END;
DROP TABLE t1;
INSERT INTO tbl VALUES(1, 2);
SELECT x FROM log;
SELECT type, name FROM sqlite_schema ORDER BY name;

-- case: bug-hunt/statements/parenthesised_table_with_alias_using
-- JOIN (t2) AS x USING (b) merges the USING column like JOIN t2 AS x USING (b)
CREATE TABLE t1(a, b);
INSERT INTO t1 VALUES(1, 2), (11, 12);
CREATE TABLE t2(b, c);
INSERT INTO t2 VALUES(2, 3), (22, 23);
SELECT * FROM t1 JOIN (t2) AS x USING (b);
SELECT * FROM t1 JOIN (t2) USING (b);

-- case: bug-hunt/statements/cte_body_ignores_nested_with
-- a CTE body resolves names in its own WITH, not in a WITH nested in the query that uses it
CREATE TABLE t3 AS SELECT 3 AS x;
CREATE TABLE t4 AS SELECT 4 AS x;
WITH x1 AS (SELECT * FROM t3), x2 AS (WITH t3 AS (SELECT * FROM t4) SELECT * FROM x1) SELECT * FROM x2;

-- case: bug-hunt/statements/cte_chain_ignores_nested_with
-- a CTE that names an earlier CTE of the same WITH is not captured by a nested WITH of the same names
WITH x1 AS (SELECT 10), x2 AS (SELECT 11), x3 AS (SELECT * FROM x1 UNION ALL SELECT * FROM x2), x4 AS (WITH x1 AS (SELECT 12), x2 AS (SELECT 13) SELECT * FROM x3) SELECT * FROM x4;

-- case: bug-hunt/statements/printf_unknown_conversion_after_text
-- an unknown conversion such as %j or %J ends the output at that point, and nothing written is NULL
SELECT format('1-%j-2-%J-3', NULL, NULL);
SELECT format('<%6J>', 'abc');
SELECT format('<%.0J>', 'abcdef');
SELECT format('%y', 1);
SELECT format('ab%', 1);

-- case: bug-hunt/statements/printf_pointer_is_uppercase
-- %p prints uppercase hexadecimal
SELECT printf('%p', -1), printf('%p', 255);

-- case: bug-hunt/statements/order_by_column_equal_to_outer_value
-- ORDER BY a column that WHERE equates to an outer value leaves the rows in scan order
CREATE TABLE t0(i INTEGER);
INSERT INTO t0 VALUES(1);
CREATE TABLE t1(t TEXT);
INSERT INTO t1 VALUES('1'), (' 1');
SELECT (SELECT t FROM t1 WHERE i=t ORDER BY t) FROM t0;
SELECT (SELECT t FROM t1 WHERE t=i ORDER BY t DESC) FROM t0;
SELECT t FROM t1, t0 WHERE i=t ORDER BY t;

-- case: bug-hunt/statements/empty_alias_column_name
-- a result column named with AS '' keeps the empty name and later ones are numbered
CREATE VIEW x1 AS SELECT 123 AS '', 234 AS '', 345 AS '';
SELECT * FROM x1;

-- case: bug-hunt/statements/outer_aggregate_in_subquery_with_window
-- an aggregate inside a subquery that reads the outer query's columns belongs to the outer query, also with a window function
CREATE TABLE t1(b, x);
SELECT 'w', sum(b) OVER (ORDER BY (SELECT 1 AS e ORDER BY sum(x))) FROM t1;
SELECT 'v', (SELECT sum((SELECT x AS c UNION SELECT 1234 ORDER BY c))) FROM t1;
