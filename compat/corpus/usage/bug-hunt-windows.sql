-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/windows/window_range_with_nulls_and_desc
-- RANGE frames reach the NULL rows at their sorted end
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(NULL),(4),(NULL),(7);
SELECT a, sum(a) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING), count(*) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a DESC RANGE BETWEEN 2 PRECEDING AND CURRENT ROW) FROM t ORDER BY a DESC;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;

-- case: bug-hunt/windows/window-range-null-ends
-- the same in four directions
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(NULL),(4),(NULL),(7);
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 2 PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a DESC RANGE BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING) FROM t ORDER BY a DESC;
SELECT a, count(*) OVER (ORDER BY a NULLS LAST RANGE BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING) FROM t ORDER BY a NULLS LAST;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 1 FOLLOWING AND 2 FOLLOWING) FROM t ORDER BY a;

-- case: bug-hunt/windows/window-offset-and-nth-value-checks
-- fractional RANGE offsets, and refusals of bad offsets and positions
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(1.4),(2),(3.2),(5);
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 0.5 PRECEDING AND 0.5 FOLLOWING) FROM t;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN '1' PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN 1.0 PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN 1.5 PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN NULL PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN CURRENT ROW AND -1 FOLLOWING) FROM t;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 'x' PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN '1' PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN -0.5 PRECEDING AND CURRENT ROW) FROM t;
SELECT a, count(*) OVER (ORDER BY a GROUPS BETWEEN 1.5 PRECEDING AND CURRENT ROW) FROM t;
SELECT a, nth_value(a, 2.0) OVER (ORDER BY a) FROM t;
SELECT a, nth_value(a, '2') OVER (ORDER BY a) FROM t;
SELECT a, nth_value(a, 1.5) OVER (ORDER BY a) FROM t;
SELECT a, nth_value(a, NULL) OVER (ORDER BY a) FROM t;
SELECT a, nth_value(a, 0) OVER (ORDER BY a) FROM t;

-- case: bug-hunt/windows/window_nth_value_bad_arg
-- nth_value position 0
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
SELECT nth_value(a, 0) OVER () FROM t;

-- case: bug-hunt/windows/window_frame_expression_bounds
-- a negative frame offset
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND '1' FOLLOWING) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN -1 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;

-- case: bug-hunt/windows/window-json-percentile-and-separator-aggregates
-- json_group_array, json_group_object, median, percentile and a per row separator over a window
CREATE TABLE t(a, s);
INSERT INTO t VALUES(1,'-'),(2,'+'),(3,'*');
SELECT json_group_array(a) OVER (ORDER BY a) FROM t;
SELECT json_group_object(s, a) OVER (ORDER BY a) FROM t;
SELECT string_agg(a, ',') OVER (ORDER BY a) FROM t;
SELECT group_concat(DISTINCT a) OVER (ORDER BY a) FROM t;
SELECT median(a) OVER () FROM t;
SELECT percentile(a, 50) OVER () FROM t;
SELECT jsonb_group_array(a) OVER () FROM t;
SELECT group_concat(a, s) OVER (ORDER BY a) FROM t;
SELECT json_group_array(json_object('k', a)) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t;
SELECT json_group_array(NULL) OVER () FROM t;
SELECT percentile_cont(a, 0.5) OVER (ORDER BY a) FROM t;
SELECT percentile_disc(a, 0.5) OVER (ORDER BY a ROWS 1 PRECEDING) FROM t;
SELECT json_group_object(s, a) FILTER (WHERE a > 1) OVER () FROM t;

-- case: bug-hunt/windows/window_group_concat_separator_frame_slide
-- group_concat with a per row separator over a sliding frame
CREATE TABLE persons(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO persons (name) VALUES('John'), ('Paul'), ('George'), ('Ringo');
WITH RECURSIVE g(value) AS (SELECT 4450 UNION ALL SELECT value+1 FROM g WHERE value<4455) SELECT group_concat(value,name) OVER (ORDER BY name ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS result FROM persons, g;

-- case: bug-hunt/windows/window-running-and-sliding-aggregates
-- running and sliding aggregates, FILTER, first_value, last_value and nth_value
CREATE TABLE t(g, a, s);
INSERT INTO t VALUES(1,1,'x'),(1,2,'y'),(1,2,'z'),(1,NULL,'w'),(2,5,'q'),(2,3,NULL),(2,3,'r'),(1,7,'v');
SELECT g, a, sum(a) OVER w, count(*) OVER w, count(a) OVER w, max(a) OVER w, min(s) OVER w, group_concat(s) OVER w, avg(a) OVER w, total(a) OVER w FROM t WINDOW w AS (PARTITION BY g ORDER BY a) ORDER BY g, a, s;
SELECT g, a, sum(a) OVER (PARTITION BY g ORDER BY a ROWS 1 PRECEDING), sum(a) OVER (ORDER BY a ROWS BETWEEN 2 PRECEDING AND 1 PRECEDING), sum(a) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING), avg(a) OVER (ORDER BY a GROUPS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t ORDER BY g, a, s;
SELECT a, first_value(s) OVER w, last_value(s) OVER w, nth_value(s, 2) OVER w, nth_value(s, 9) OVER w FROM t WINDOW w AS (ORDER BY a ROWS BETWEEN 1 PRECEDING AND 2 FOLLOWING) ORDER BY a, s;
SELECT a, first_value(s) OVER w, last_value(s) OVER w, nth_value(s, 3) OVER w FROM t WINDOW w AS (ORDER BY a) ORDER BY a, s;
SELECT a, last_value(s) OVER (ORDER BY a ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING EXCLUDE CURRENT ROW) FROM t ORDER BY a, s;
SELECT a, sum(a) FILTER (WHERE a > 2) OVER (ORDER BY a), count(*) FILTER (WHERE s IS NOT NULL) OVER (ORDER BY a ROWS 2 PRECEDING) FROM t ORDER BY a, s;
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), json_group_array(a) OVER (ORDER BY a ROWS UNBOUNDED PRECEDING) FROM t ORDER BY a, s;
SELECT a, sum(a) OVER (), max(a) OVER (PARTITION BY g) FROM t ORDER BY a, s;
SELECT a, sum(a) OVER (ORDER BY a DESC ROWS BETWEEN 1 FOLLOWING AND 3 FOLLOWING) FROM t ORDER BY a, s;

-- case: bug-hunt/windows/window_in_first_arm_of_compound_derived_table
-- a window function in the first arm of a derived table's compound keeps the other arms
CREATE TABLE t0(x);
CREATE TABLE t1(a);
INSERT INTO t1 VALUES(1000);
INSERT INTO t1 VALUES(1000);
INSERT INTO t0 VALUES(10000);
SELECT * FROM (SELECT sum(a) OVER() FROM t1 UNION ALL SELECT x FROM t0);
CREATE TABLE t2(a VARCHAR(20), b FLOAT);
INSERT INTO t2 VALUES('1',10.0),('2',5.0),('3',15.0);
SELECT * FROM (SELECT sum(b) OVER() AS c FROM t2 UNION SELECT b AS c FROM t2) WHERE c>10;

-- case: bug-hunt/windows/row_value_compound_subquery_order_by
-- a row value read from a compound with ORDER BY takes its first row in the compound's order
SELECT (1,2) = (SELECT 5,6 UNION SELECT 3,4 ORDER BY 1);
SELECT (3,4) = (SELECT 5,6 UNION SELECT 3,4 ORDER BY 1);
SELECT (5,6) = (SELECT 5,6 UNION SELECT 3,4 ORDER BY 2 DESC);
CREATE TABLE t3(p, q);
INSERT INTO t3 VALUES(0, 0);
UPDATE t3 SET (p, q) = (SELECT 5,6 UNION SELECT 3,4 ORDER BY 1);
SELECT * FROM t3;
