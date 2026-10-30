-- case: bug-hunt-11/window/d1-median-text-is-not-numeric
CREATE TABLE t(a);
INSERT INTO t VALUES ('abc'),(1);
SELECT median(a) FROM t;
-- case: bug-hunt-11/window/d1-percentile-blob-is-not-numeric
CREATE TABLE t(a);
INSERT INTO t VALUES (x'31');
SELECT percentile(a, 50) FROM t;
-- case: bug-hunt-11/window/d1-median-numeric-text-is-not-numeric
CREATE TABLE t(a);
INSERT INTO t VALUES ('1'),('3');
SELECT median(a) FROM t;
-- case: bug-hunt-11/window/d1-percentile-cont-and-disc-text
CREATE TABLE t(a);
INSERT INTO t VALUES (1),('x'),(3);
SELECT percentile_cont(a, 0.5) FROM t;
SELECT percentile_disc(a, 0.5) FROM t;
-- case: bug-hunt-11/window/d1-percentile-skips-null-and-keeps-numbers
CREATE TABLE t(a);
INSERT INTO t VALUES (NULL),(4),(1.5),(NULL),(2),(10);
SELECT median(a), percentile(a, 25), percentile_cont(a, 0.9), percentile_disc(a, 0.4) FROM t;
SELECT median(a) FROM t WHERE a IS NULL;
-- case: bug-hunt-11/window/d1-percentile-over-no-rows-checks-nothing
CREATE TABLE t(a);
SELECT median(a), percentile(a, 101), percentile_cont(a, 7), percentile_disc(a, -1) FROM t;
-- case: bug-hunt-11/window/d2-percentile-fraction-over-range
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT percentile(a, 101) FROM t;
-- case: bug-hunt-11/window/d2-percentile-cont-fraction-over-range
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT percentile_cont(a, 1.5) FROM t;
-- case: bug-hunt-11/window/d2-percentile-disc-fraction-negative
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT percentile_disc(a, -0.5) FROM t;
-- case: bug-hunt-11/window/d2-percentile-fraction-null
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT percentile(a, NULL) FROM t;
-- case: bug-hunt-11/window/d2-percentile-fraction-text-and-blob
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT percentile(a, '50') FROM t;
SELECT percentile(a, 'x') FROM t;
SELECT percentile(a, x'31') FROM t;
-- case: bug-hunt-11/window/d2-percentile-fraction-varies
CREATE TABLE t(a, p);
INSERT INTO t VALUES (1, 10),(2, 20),(3, 30);
SELECT percentile(a, p) FROM t;
-- case: bug-hunt-11/window/d2-fraction-checked-even-when-value-is-null
CREATE TABLE t(a, p);
INSERT INTO t VALUES (NULL, 10),(1, 20);
SELECT percentile(a, p) FROM t;
SELECT percentile(NULL, 101) FROM t;
-- case: bug-hunt-11/window/d2-same-fraction-in-every-row-is-accepted
CREATE TABLE t(a, p);
INSERT INTO t VALUES (NULL, 10),(1, 10),(5, 10);
SELECT percentile(a, p), percentile_disc(a, p / 100.0) FROM t;
-- case: bug-hunt-11/window/d2-percentile-window-fraction-varies
CREATE TABLE t(a, p);
INSERT INTO t VALUES (1, 10),(2, 20),(3, 30);
SELECT a, percentile(a, p) OVER () FROM t;
-- case: bug-hunt-11/window/d2-percentile-window-fraction-differs-from-first-row-after-it-left
CREATE TABLE t(id INTEGER PRIMARY KEY, a, p);
INSERT INTO t VALUES (1, 1, 10),(2, 2, 20),(3, 3, 30);
SELECT id, percentile(a, p) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND CURRENT ROW) FROM t;
-- case: bug-hunt-11/window/d2-median-window-text-after-a-frame-with-numbers
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1, 4),(2, 6),(3, 'x'),(4, 8);
SELECT id, median(a) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t;
-- case: bug-hunt-11/window/d2-median-window-empty-frames-never-add-a-row
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1, 4),(2, 'x'),(3, 8);
SELECT id, median(a) OVER (ORDER BY id ROWS BETWEEN 1 FOLLOWING AND 0 FOLLOWING) FROM t;
-- case: bug-hunt-11/window/d3-percentile-inner-order-by
CREATE TABLE t(a, b);
INSERT INTO t VALUES (3,1),(2,2),(1,3),(10,4);
SELECT percentile(a, 50 ORDER BY b) FROM t;
SELECT percentile_cont(a, 0.5 ORDER BY b DESC) FROM t;
SELECT percentile_disc(a, 0.5 ORDER BY b) FROM t;
SELECT median(a ORDER BY b) FROM t;
-- case: bug-hunt-11/window/d3-percentile-inner-order-by-no-rows
CREATE TABLE t(a, b);
SELECT percentile(a, 50 ORDER BY b) FROM t;
-- case: bug-hunt-11/window/d4-range-real-offset-float-arithmetic
CREATE TABLE t(a);
INSERT INTO t VALUES (0.1),(0.2),(0.3),(0.4),(0.5);
SELECT a, group_concat(a) OVER (ORDER BY a RANGE BETWEEN 0.1 PRECEDING AND 0.1 FOLLOWING) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d4-range-real-offset-desc
CREATE TABLE t(a);
INSERT INTO t VALUES (0.1),(0.2),(0.3),(0.4),(0.5);
SELECT a, group_concat(a) OVER (ORDER BY a DESC RANGE BETWEEN 0.1 PRECEDING AND 0.1 FOLLOWING) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d4-range-real-offset-each-end
CREATE TABLE t(a);
INSERT INTO t VALUES (0.1),(0.2),(0.3),(0.4),(0.5),(0.6),(0.7);
SELECT a, group_concat(a) OVER (ORDER BY a RANGE BETWEEN 0.1 PRECEDING AND 0.1 PRECEDING) FROM t ORDER BY a;
SELECT a, group_concat(a) OVER (ORDER BY a RANGE BETWEEN 0.1 FOLLOWING AND 0.2 FOLLOWING) FROM t ORDER BY a;
SELECT a, group_concat(a) OVER (ORDER BY a DESC RANGE BETWEEN 0.1 FOLLOWING AND 0.2 FOLLOWING) FROM t ORDER BY a;
SELECT a, group_concat(a) OVER (ORDER BY a DESC RANGE BETWEEN 0.2 PRECEDING AND 0.1 PRECEDING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 0.3 PRECEDING AND 0.30000000000000004 FOLLOWING) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d4-range-huge-real-offset
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(1e300),(-1e300);
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 1e300 PRECEDING AND 1e300 FOLLOWING) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d4-range-infinity
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(9e999),(-9e999);
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 9e999 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d4-range-infinite-offset-on-finite-values
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 9e999 PRECEDING AND 9e999 FOLLOWING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 9e999 PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN 1 FOLLOWING AND 9e999 FOLLOWING) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d5-count-star-start-moves-to-unbounded-following
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 3000) INSERT INTO t SELECT n, (n * 7919) % 1000 FROM c;
SELECT sum(x), min(x), max(x) FROM (SELECT count(*) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) AS x FROM t);
SELECT sum(x), min(x), max(x) FROM (SELECT count(v) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) AS x FROM t);
-- case: bug-hunt-11/window/d5-max-start-moves-to-unbounded-following
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 3000) INSERT INTO t SELECT n, (n * 7919) % 1000 FROM c;
SELECT sum(x), min(x), max(x) FROM (SELECT max(v) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) AS x FROM t);
SELECT sum(x), min(x), max(x) FROM (SELECT min(v) OVER (ORDER BY id ROWS BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING) AS x FROM t);
-- case: bug-hunt-11/window/d5-min-max-sliding-rows-and-range
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 3000) INSERT INTO t SELECT n, (n * 7919) % 1000 FROM c;
SELECT sum(x), min(x), max(x) FROM (SELECT min(v) OVER (ORDER BY id ROWS BETWEEN 300 PRECEDING AND 300 FOLLOWING) AS x FROM t);
SELECT sum(x), min(x), max(x) FROM (SELECT max(v) OVER (ORDER BY id RANGE BETWEEN 300 PRECEDING AND 300 FOLLOWING) AS x FROM t);
-- case: bug-hunt-11/window/d5-count-range-and-groups-with-ties
CREATE TABLE t(id INTEGER PRIMARY KEY, g INT);
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 3000) INSERT INTO t SELECT n, n % 100 FROM c;
SELECT sum(x), min(x), max(x) FROM (SELECT count(*) OVER (ORDER BY g RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS x FROM t);
SELECT sum(x), min(x), max(x) FROM (SELECT count(*) OVER (ORDER BY g GROUPS BETWEEN 5 PRECEDING AND 5 FOLLOWING) AS x FROM t);
-- case: bug-hunt-11/window/d5-median-json-and-group-concat-sliding
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 1500) INSERT INTO t SELECT n, (n * 7919) % 1000 FROM c;
SELECT sum(x) FROM (SELECT median(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING) AS x FROM t);
SELECT sum(length(x)) FROM (SELECT json_group_array(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING) AS x FROM t);
SELECT sum(length(x)) FROM (SELECT json_group_object(id, v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING) AS x FROM t);
SELECT sum(length(x)) FROM (SELECT group_concat(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING) AS x FROM t);
SELECT sum(length(x)) FROM (SELECT group_concat(v, '--') OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 30 FOLLOWING) AS x FROM t);
-- case: bug-hunt-11/window/d5-sliding-aggregates-skip-null-and-filtered-rows
CREATE TABLE t(id INTEGER PRIMARY KEY, g, v, w);
INSERT INTO t VALUES (1,1,NULL,'a'),(2,1,3,'b'),(3,1,3,NULL),(4,2,7,'c'),(5,2,NULL,'c'),(6,2,1,'a'),(7,1,2.5,'b'),(8,1,0,'d'),(9,2,9,NULL),(10,1,NULL,NULL);
SELECT id, count(*), count(v), min(v), max(v), min(w), max(w) FROM (SELECT id, 1 FROM t) JOIN t USING (id) GROUP BY id ORDER BY id;
SELECT id, count(*) OVER win, count(v) OVER win, min(v) OVER win, max(v) OVER win, min(w) OVER win, max(w) OVER win FROM t WINDOW win AS (ORDER BY id ROWS BETWEEN 2 PRECEDING AND 1 FOLLOWING) ORDER BY id;
SELECT id, count(*) FILTER (WHERE v > 2) OVER win, max(v) FILTER (WHERE v > 2) OVER win, min(v) FILTER (WHERE v > 2) OVER win, median(v) FILTER (WHERE v > 2) OVER win, group_concat(v) FILTER (WHERE v > 2) OVER win FROM t WINDOW win AS (PARTITION BY g ORDER BY id ROWS BETWEEN 1 FOLLOWING AND 3 FOLLOWING) ORDER BY id;
SELECT id, json_group_array(v) OVER win, json_group_object(w, v) OVER win, group_concat(w, '/') OVER win FROM t WINDOW win AS (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 2 FOLLOWING) ORDER BY id;
SELECT id, median(v) OVER win, percentile(v, 30) OVER win, percentile_cont(v, 0.5) OVER win, percentile_disc(v, 0.5) OVER win FROM t WINDOW win AS (PARTITION BY g ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) ORDER BY id;
-- case: bug-hunt-11/window/d5-sliding-min-max-keep-the-latest-of-equal-values
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES (1, 0.0),(2, 0),(3, 5),(4, 5.0),(5, 1),(6, 1.0);
SELECT id, typeof(min(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)), min(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t;
SELECT id, typeof(max(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)), max(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t;
-- case: bug-hunt-11/window/d5-sliding-min-max-collation-and-mixed-types
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT COLLATE NOCASE, v);
INSERT INTO t VALUES (1,'b',1),(2,'A','x'),(3,'a',x'01'),(4,'C',2.5),(5,'c',NULL),(6,'B',-3);
SELECT id, min(s) OVER win, max(s) OVER win, min(v) OVER win, max(v) OVER win FROM t WINDOW win AS (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 2 FOLLOWING) ORDER BY id;
-- case: bug-hunt-11/window/d5-sliding-frames-descending-and-partitioned
CREATE TABLE t(id INTEGER PRIMARY KEY, g, v);
INSERT INTO t VALUES (1,'a',5),(2,'a',3),(3,'b',8),(4,'a',3),(5,'b',1),(6,'b',8),(7,'a',9),(8,'b',NULL);
SELECT id, max(v) OVER (PARTITION BY g ORDER BY v DESC, id RANGE BETWEEN 2 PRECEDING AND 3 FOLLOWING), count(v) OVER (PARTITION BY g ORDER BY v DESC, id ROWS BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING), min(v) OVER (PARTITION BY g ORDER BY v GROUPS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t ORDER BY id;
-- case: bug-hunt-11/window/d6-frame-offset-scalar-subquery
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN (SELECT 1) PRECEDING AND CURRENT ROW) FROM t ORDER BY a;
-- case: bug-hunt-11/window/d6-frame-offset-function-and-column-read-as-null
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1, 1),(2, 2),(3, 1);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN abs(1) PRECEDING AND CURRENT ROW) FROM t;
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN a PRECEDING AND CURRENT ROW) FROM t;
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND coalesce(1, 2) FOLLOWING) FROM t;
SELECT a, sum(a) OVER (ORDER BY a RANGE BETWEEN (SELECT 1) PRECEDING AND CURRENT ROW) FROM t;
SELECT a, sum(a) OVER (ORDER BY a GROUPS BETWEEN 1 PRECEDING AND length('a') FOLLOWING) FROM t;
-- case: bug-hunt-11/window/d6-frame-offset-expression-of-literals-still-works
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3),(4);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN 1+1 PRECEDING AND CURRENT ROW), sum(a) OVER (ORDER BY a ROWS BETWEEN CAST('1' AS INT) PRECEDING AND CASE WHEN 1 THEN 1 END FOLLOWING) FROM t;
-- case: bug-hunt-11/window/d6-frame-offset-function-with-no-rows-raises-nothing
CREATE TABLE t(a);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN abs(1) PRECEDING AND CURRENT ROW) FROM t;
-- case: bug-hunt-11/window/d7-nth-value-zero-empty-frame
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'x'),(2,'y');
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS CURRENT ROW EXCLUDE GROUP) FROM t;
-- case: bug-hunt-11/window/d7-nth-value-bad-position-in-a-frame-that-has-rows
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'x'),(2,'y');
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t;
SELECT id, nth_value(a, NULL) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE CURRENT ROW) FROM t;
SELECT id, nth_value(a, -1) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING EXCLUDE CURRENT ROW) FROM t;
-- case: bug-hunt-11/window/d7-nth-value-bad-position-when-the-frame-end-reaches-a-row
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'x'),(2,'y');
SELECT id, nth_value(a, NULL) OVER (ORDER BY id ROWS BETWEEN 1000 FOLLOWING AND UNBOUNDED FOLLOWING) FROM t;
-- case: bug-hunt-11/window/d7-nth-value-bad-position-is-checked-even-when-the-frame-is-empty
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'x'),(2,'y'),(3,'z');
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN 5 PRECEDING AND 5 PRECEDING) FROM t;
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN 1 FOLLOWING AND 2 FOLLOWING) FROM t WHERE id = 3;
SELECT id, nth_value(a, 0) OVER (ORDER BY id RANGE BETWEEN 5 PRECEDING AND 5 PRECEDING) FROM t;
-- case: bug-hunt-11/window/d7-nth-value-bad-position-with-exclude-and-nothing-left
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'x'),(2,'y'),(3,'z');
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN 5 PRECEDING AND 5 PRECEDING EXCLUDE GROUP) FROM t;
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN 5 PRECEDING AND 5 PRECEDING EXCLUDE CURRENT ROW) FROM t;
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND CURRENT ROW EXCLUDE CURRENT ROW) FROM t;
SELECT id, nth_value(a, 0) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND CURRENT ROW EXCLUDE TIES) FROM t;
-- case: bug-hunt-11/window/d8-filter-on-row-number
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2);
SELECT row_number() FILTER (WHERE a > 1) OVER () FROM t;
-- case: bug-hunt-11/window/d8-filter-on-lag
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2);
SELECT lag(a) FILTER (WHERE a > 1) OVER (ORDER BY a) FROM t;
-- case: bug-hunt-11/window/d8-filter-on-other-window-only-functions
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2);
SELECT rank() FILTER (WHERE a > 1) OVER () FROM t;
SELECT ntile(2) FILTER (WHERE a > 1) OVER () FROM t;
SELECT first_value(a) FILTER (WHERE a > 1) OVER () FROM t;
SELECT nth_value(a, 1) FILTER (WHERE a > 1) OVER () FROM t;
-- case: bug-hunt-11/window/d8-filter-on-aggregate-windows-still-works
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT a, count(*) FILTER (WHERE a > 1) OVER (), sum(a) FILTER (WHERE a > 1) OVER (ORDER BY a) FROM t;
-- case: bug-hunt-11/window/d9-order-by-in-aggregate-window
CREATE TABLE t(a, b);
INSERT INTO t VALUES (3,1),(2,2),(1,3);
SELECT group_concat(a ORDER BY b DESC) OVER () FROM t;
-- case: bug-hunt-11/window/d9-order-by-in-other-window-calls
CREATE TABLE t(a, b);
INSERT INTO t VALUES (3,1),(2,2),(1,3);
SELECT sum(a ORDER BY b) OVER () FROM t;
SELECT count(a ORDER BY b) OVER () FROM t;
SELECT json_group_array(a ORDER BY b) OVER () FROM t;
SELECT median(a ORDER BY b) OVER () FROM t;
-- case: bug-hunt-11/window/d9-order-by-in-aggregate-without-over-still-works
CREATE TABLE t(a, b);
INSERT INTO t VALUES (3,1),(2,2),(1,3);
SELECT group_concat(a ORDER BY b DESC), json_group_array(a ORDER BY b) FROM t;
-- case: bug-hunt-11/window/d10-duplicate-window-name
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2);
SELECT sum(a) OVER w FROM t WINDOW w AS (ORDER BY a), w AS (ORDER BY a DESC);
-- case: bug-hunt-11/window/d10-duplicate-window-name-uses-the-last-definition
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1,1),(2,2),(3,1);
SELECT a, sum(a) OVER w FROM t WINDOW w AS (ORDER BY a), w AS (ORDER BY a DESC) ORDER BY a;
SELECT a, sum(a) OVER w FROM t WINDOW w AS (ORDER BY a DESC), w AS (ORDER BY a) ORDER BY a;
SELECT a, sum(a) OVER w FROM t WINDOW w AS (ORDER BY a), v AS (w), w AS (ORDER BY a DESC) ORDER BY a;
SELECT a, sum(a) OVER w FROM t WINDOW w AS (ORDER BY a), W AS (ORDER BY a DESC) ORDER BY a;
-- case: bug-hunt-11/window/d10-window-between-two-definitions-sees-the-first
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1,1),(2,2),(3,1);
SELECT a, sum(a) OVER v FROM t WINDOW w AS (ORDER BY a), v AS (w), w AS (ORDER BY a DESC) ORDER BY a;
SELECT a, sum(a) OVER v FROM t WINDOW w AS (ORDER BY a), w AS (ORDER BY a DESC), v AS (w) ORDER BY a;
SELECT a, sum(a) OVER v FROM t WINDOW w AS (PARTITION BY b), w AS (ORDER BY a DESC), v AS (w) ORDER BY a;
-- case: bug-hunt-11/window/d11-group-concat-window-empty-string-sqlite-quirk
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,''),(2,'x'),(3,'y'),(4,''),(5,'ABC'),(6,'');
SELECT id, group_concat(a) OVER (ORDER BY id ROWS 1 PRECEDING) FROM t ORDER BY id;
-- case: bug-hunt-11/window/d11-group-concat-window-without-empty-strings
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,'p'),(2,'x'),(3,'y'),(4,NULL),(5,'ABC'),(6,'q');
SELECT id, group_concat(a) OVER (ORDER BY id ROWS 1 PRECEDING), group_concat(a, '|') OVER (ORDER BY id ROWS BETWEEN 1 FOLLOWING AND 2 FOLLOWING) FROM t ORDER BY id;
-- case: bug-hunt-11/window/d12-group-concat-blob-invalid-utf8
CREATE TABLE t(a);
INSERT INTO t VALUES (x'ff'),(x'fe41');
SELECT hex(group_concat(a)), hex(group_concat(a, x'ff')), hex(string_agg(a, ',')) FROM t;
-- case: bug-hunt-11/window/d12-group-concat-blob-window
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,x'ff'),(2,x'fe41');
SELECT id, hex(group_concat(a) OVER (ORDER BY id)) FROM t ORDER BY id;
-- case: bug-hunt-11/window/d12-group-concat-invalid-utf8-text-and-sliding-frame
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t VALUES (1,x'c3'),(2,x'28'),(3,x'e2828a'),(4,x'41');
SELECT hex(group_concat(CAST(a AS TEXT), '-')) FROM t;
SELECT id, hex(group_concat(a, ',') OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW)), typeof(group_concat(a) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW)) FROM t ORDER BY id;
SELECT id, hex(group_concat(a, a) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 1 FOLLOWING)) FROM t ORDER BY id;
