-- Cases taken from SQLite forum posts and the SQLancer bug lists. The source of each is on its marker line.
-- Gathered by the bug hunt of October 2026; see tasks/task-2201-bug-hunt-tdd.md.
-- Run every night by nightly::research_corpus against the pinned SQLite.

-- case: research/sqlite-forum/left_join_transient_index_limit | source: https://sqlite.org/forum/forumpost/0d3200f4f3bcd3a3
CREATE TABLE t1(a INT, b INT);
CREATE TABLE t2(c INT, d INT);
CREATE TABLE t3(e TEXT, f TEXT);
INSERT INTO t1 VALUES(1, 1);
INSERT INTO t2 VALUES(1, 2);
SELECT * FROM t1 JOIN t2 ON (t2.c=t1.a) LEFT JOIN t3 ON (t2.d=1);

-- case: research/sqlite-forum/left_join_fts5_case_match | source: https://sqlite.org/forum/forumpost/428ef7c468
CREATE VIRTUAL TABLE t0 USING fts5(c0, c1);
INSERT INTO t0(c0,c1) VALUES (1,0);
SELECT * FROM t0 LEFT JOIN (SELECT 0 AS col_0) ON ((((t0.c1 MATCH '1')AND(CASE WHEN t0.c0 THEN CAST(t0.c1 AS INTEGER) ELSE 1 END))));

-- case: research/sqlite-forum/window_view_is_null_notnull_reduction | source: https://sqlite.org/forum/forumpost/440f2a2f17
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY, c2 TEXT);
CREATE VIEW v5 AS SELECT c1, COUNT ( * ) AS y, sum ( c2 ) OVER ( PARTITION BY c1) FROM v0;
SELECT c1 from v5;
SELECT c1 FROM v5 WHERE c1 IS NULL;

-- case: research/sqlite-forum/count_view_total_window_where_true | source: https://sqlite.org/forum/forumpost/2cd11c2d37
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
CREATE VIEW v0(c0) AS SELECT TOTAL(0) OVER (PARTITION BY t0.c0) FROM t0;
SELECT COUNT(*) FROM v0 WHERE ('1' IS NOT('1' NOTNULL))-(0);

-- case: research/sqlite-forum/window_min_filter_bounded_frame | source: https://sqlite.org/forum/forumpost/e9126d554a
CREATE TABLE t0 (c0 INTEGER, c1 INTEGER);
INSERT INTO t0 (c0, c1) VALUES (10, 1), (20, -1), (5, 2), (15, 0), (25, 3);
SELECT c0, c1, MIN(c0) FILTER(WHERE c1 > 0) OVER (ORDER BY c0 ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS m FROM t0 ORDER BY c0;
SELECT MIN(c0) FILTER(WHERE false) OVER (ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t0;

-- case: research/sqlite-forum/window_sum_overflow_frame | source: https://sqlite.org/forum/forumpost/ec538b04ce
WITH t(id,x) AS (VALUES (1,-1),(2,9223372036854775807),(3,1),(4,0.5))
SELECT id, sum(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING) AS win_sum, total(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING) AS win_total, avg(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING) AS win_avg, (SELECT total(x) FROM t t2 WHERE t2.id BETWEEN t1.id AND t1.id+2) AS ref_total, (SELECT avg(x) FROM t t2 WHERE t2.id BETWEEN t1.id AND t1.id+2) AS ref_avg FROM t t1 ORDER BY id;

-- case: research/sqlite-forum/right_join_or_where_automatic_index | source: https://sqlite.org/forum/forumpost/f3f546025a
CREATE TABLE t1(a INT);  INSERT INTO t1 VALUES(1);
CREATE TABLE t2(b INT);  INSERT INTO t2 VALUES(2);
CREATE TABLE t3(c INT);  INSERT INTO t3 VALUES(3);
CREATE TABLE t4(d INT);  INSERT INTO t4 VALUES(4);
CREATE TABLE t5(e INT);  INSERT INTO t5 VALUES(5);
SELECT * FROM t1 JOIN t2 ON null RIGHT JOIN t3 ON true LEFT JOIN (t4 JOIN t5 ON d+1=e) ON d=4 WHERE e>0;

-- case: research/sqlite-forum/left_join_fts5_case_expr | source: https://sqlite.org/forum/forumpost/428ef7c468
CREATE VIRTUAL TABLE t0 USING fts5(c0, c1);
INSERT INTO t0(c0,c1) VALUES (1,0);
SELECT * FROM t0 LEFT JOIN (SELECT 0 AS col_0) ON ((((t0.c1 MATCH '1')AND(CASE WHEN t0.c0 THEN CAST(t0.c1 AS INTEGER) ELSE 1 END))));

-- case: research/sqlite-forum/left_join_flatten_view_uncorrelated_subquery_register | source: https://sqlite.org/forum/forumpost/402f05296d
CREATE TABLE t1(x TEXT, y INTEGER);
INSERT INTO t1(x,y) VALUES(NULL,-2),(NULL,1),('0',2);
CREATE TABLE t2(z INTEGER);
INSERT INTO t2(z) VALUES(2),(-2);
CREATE VIEW t3 AS SELECT z, (SELECT count(*) FROM t1) AS w FROM t2;
SELECT * FROM t1 LEFT JOIN t3 ON y=z;

-- case: research/sqlite-forum/left_join_on_is_null_union_all_subquery | source: https://sqlite.org/forum/forumpost/6cf3bb457c3f4685
SELECT * FROM (SELECT NULL val FROM (SELECT 1) UNION ALL SELECT 'missing' FROM (SELECT 1)) a LEFT JOIN (SELECT 1) ON a.val IS NULL;

-- case: research/sqlite-forum/left_join_on_left_only_condition_demoted_to_inner | source: https://sqlite.org/forum/forumpost/f8786ea8e1de76f5
CREATE TABLE t1(a INT);
INSERT INTO t1(a) VALUES(1);
CREATE TABLE t2(b INT);
SELECT * FROM (SELECT 3 AS c FROM t1) AS t3 LEFT JOIN t2 ON c IS NULL;

-- case: research/sqlite-forum/left_join_on_is_null_onerow_union_all | source: https://sqlite.org/forum/forumpost/33c9cd5abf
CREATE TABLE onerow(x INT);
INSERT INTO onerow(x) VALUES(0);
SELECT * FROM (SELECT null AS aaa FROM onerow UNION ALL SELECT 'missing' AS aaa FROM onerow) AS a LEFT JOIN (SELECT 1 AS bbb) AS b ON a.aaa IS NULL;

-- case: research/sqlite-forum/right_join_autoindex_on_null_other_side | source: https://sqlite.org/forum/forumpost/f3f546025a
CREATE TABLE t1(a INT);  INSERT INTO t1 VALUES(1);
CREATE TABLE t2(b INT);  INSERT INTO t2 VALUES(2);
CREATE TABLE t3(c INT);  INSERT INTO t3 VALUES(3);
CREATE TABLE t4(d INT);  INSERT INTO t4 VALUES(4);
CREATE TABLE t5(e INT);  INSERT INTO t5 VALUES(5);
SELECT * FROM t1 JOIN t2 ON null RIGHT JOIN t3 ON true LEFT JOIN (t4 JOIN t5 ON d+1=e) ON d=4 WHERE e>0;

-- case: research/sqlite-forum/right_join_where_is_null_makes_column_null | source: https://sqlite.org/forum/forumpost/41cc3851d8
CREATE TABLE t0 (c0 INT);
CREATE TABLE t1 (c0 INT);
INSERT INTO t0 VALUES(2);
INSERT INTO t1 VALUES(NULL);
SELECT t0.c0, t0.c0 IS NULL FROM t0 RIGHT OUTER JOIN t1;
SELECT t0.c0, t0.c0 IS NULL FROM t0 RIGHT OUTER JOIN t1 WHERE t0.c0 IS NULL;

-- case: research/sqlite-forum/full_join_cross_join_on_false_view | source: https://sqlite.org/forum/forumpost/96cd4a7e9e
CREATE TABLE vt0(c2);
CREATE TABLE t1 (c0 TEXT);
INSERT INTO t1(c0) VALUES (1);
INSERT INTO vt0(c2) VALUES (-1);
CREATE VIEW v0(c0) AS SELECT 0 FROM t1;
SELECT vt0.c2 AS c1 FROM t1 CROSS JOIN v0 ON (0) FULL OUTER JOIN vt0 ON 1;
SELECT vt0.c2 AS c1 FROM t1 CROSS JOIN v0 ON (EXISTS (SELECT v0.c0 FROM v0 WHERE false)) FULL OUTER JOIN vt0 ON 1;

-- case: research/sqlite-forum/right_join_strength_reduction_is_true | source: https://sqlite.org/forum/forumpost/7f74ce0bee
CREATE TABLE rt0 (c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER, c4 INTEGER);
CREATE TABLE rt3 (c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER);
INSERT OR IGNORE INTO rt0(c3, c1) VALUES (x'', '1'), ('-1', -1e500), (1, x'');
CREATE VIEW v6(c0, c1, c2) AS SELECT 0, 0, 0;
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL));
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL)) WHERE (rt0.c1);
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL)) WHERE ((rt0.c1) IS TRUE);

-- case: research/sqlite-forum/right_join_on_false_pushdown_into_distinct_view | source: https://sqlite.org/forum/forumpost/a7d4be7fb6
CREATE TABLE t1(a INT);  INSERT INTO t1(a) VALUES(1);
CREATE TABLE t2(b INT);  INSERT INTO t2(b) VALUES(2);
CREATE TABLE t3(c INT);  INSERT INTO t3(c) VALUES(3);
CREATE TABLE t4(d INT);  INSERT INTO t4(d) VALUES(4);
CREATE VIEW v5(e) AS SELECT DISTINCT d FROM t4;
SELECT * FROM t1 JOIN t2 ON false RIGHT JOIN t3 ON true JOIN v5 ON true;

-- case: research/sqlite-forum/right_join_autoindex_where_term_chain | source: https://sqlite.org/forum/forumpost/51e6959f61
CREATE TABLE t0 (c0 INTEGER);
INSERT INTO t0 VALUES ('x');
CREATE TABLE t1(c0 INTEGER);
INSERT INTO t1 VALUES ('y');
CREATE TABLE t2 (c0 INTEGER);
CREATE TABLE t3 (c0 INTEGER);
SELECT * FROM t2 LEFT OUTER JOIN t3 RIGHT OUTER JOIN t1 RIGHT OUTER JOIN t0 WHERE (t3.c0=t2.c0);
SELECT (t3.c0=t2.c0) IS TRUE FROM t2 LEFT OUTER JOIN t3 RIGHT OUTER JOIN t1 RIGHT OUTER JOIN t0;

-- case: research/sqlite-forum/distinct_union_all_count_one_column | source: https://sqlite.org/forum/forumpost/aeae62275ebbf584
CREATE TABLE IF NOT EXISTS t1(id TEXT);
INSERT INTO t1 VALUES('a'),('b');
SELECT * FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);
SELECT count() FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);
SELECT count(1) FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);

-- case: research/sqlite-forum/distinct_omitted_with_partial_unique_index | source: https://sqlite.org/forum/forumpost/66954e9ece
CREATE TABLE person (pid INT);
CREATE UNIQUE INDEX idx ON person (pid) WHERE pid == 1;
INSERT INTO person VALUES (1), (10), (10);
SELECT DISTINCT pid FROM person;
SELECT DISTINCT pid FROM person WHERE pid = 10;

-- case: research/sqlite-forum/full_join_term_reorder_select_list | source: https://sqlite.org/forum/forumpost/6650cd40b5
CREATE TABLE t1(a1 INT);
CREATE TABLE t2(b2 INT);
CREATE TABLE t3(c3 INT, d3 INT UNIQUE);
CREATE TABLE t4(e4 INT, f4 TEXT);
INSERT INTO t3(c3, d3) VALUES (2, 1);
INSERT INTO t4(f4) VALUES ('x');
CREATE INDEX i0 ON t3(c3) WHERE d3 ISNULL;
ANALYZE main;
SELECT * FROM t1 LEFT JOIN t2 ON true JOIN t3 ON (b2 IN (a1)) FULL JOIN t4 ON true;
SELECT 1 FROM t1 LEFT JOIN t2 ON true JOIN t3 ON (b2 IN (a1)) FULL JOIN t4 ON true;

-- case: research/sqlite-forum/flatten_view_with_full_join_union_all | source: https://sqlite.org/forum/forumpost/174afeae57
CREATE TABLE t1(a INT);
CREATE TABLE t2(b INT, c INT);
CREATE VIEW t3(d) AS SELECT NULL FROM t2 FULL OUTER JOIN t1 ON c=a UNION ALL SELECT b FROM t2;
INSERT INTO t1(a) VALUES (NULL);
INSERT INTO t2(b, c) VALUES (99, NULL);
SELECT DISTINCT * FROM t2, t3 WHERE b<>0 UNION SELECT DISTINCT * FROM t2, t3 WHERE b ISNULL;

-- case: research/sqlite-forum/full_join_false_constant_vs_subquery_count_view | source: https://sqlite.org/forum/info/95849acbe1
CREATE TABLE vt0(c0);
CREATE TABLE rt1(c0);
CREATE VIEW v0(c0) AS SELECT ((rt1.c0) NOTNULL) FROM rt1;
INSERT INTO vt0(c0) VALUES (1);
INSERT INTO rt1(c0) VALUES (1);
SELECT COUNT(v0.c0) AS c0 FROM v0;
SELECT COUNT(*) FROM vt0 INNER JOIN v0 ON ((0)/(1)) FULL OUTER JOIN rt1;
SELECT COUNT(*) FROM vt0 INNER JOIN v0 ON ((0)/(SELECT COUNT(v0.c0) AS c0 FROM v0)) FULL OUTER JOIN rt1;

-- case: research/sqlite-forum/equivalence_transfer_likely_natural_join | source: https://sqlite.org/forum/forumpost/eb8613976acfe23a
CREATE TABLE t0(c0 INT, c1 INT UNIQUE);
CREATE TABLE t1(c0 INT);
INSERT INTO t0(c0, c1) VALUES (0, 1);
INSERT INTO t1(c0) VALUES (1);
SELECT ALL * FROM t1 NATURAL JOIN t0 WHERE (t1.c0=t0.c1);
SELECT ALL * FROM t1 NATURAL JOIN t0 WHERE (likely(t1.c0=t0.c1));
SELECT ALL * FROM t1,t0 WHERE (likely(t1.c0=t0.c1) AND t1.c0=t0.c0);

-- case: research/sqlite-forum/natural_full_join_view_real_pk_is | source: https://sqlite.org/forum/forumpost/68f29a2005
CREATE TABLE t0(c0, c1, c2);
CREATE TABLE t1(c0);
CREATE TABLE t2 (c0 REAL, PRIMARY KEY (c0));
CREATE TEMP VIEW IF NOT EXISTS v0(c0) AS SELECT CAST(t1.c0 AS NUMERIC) AS col_0 FROM t1 NATURAL LEFT JOIN t2;
INSERT INTO t1(c0) VALUES (0.6);
INSERT INTO t2(c0) VALUES (NULL);
SELECT v0.c0, t1.c0 FROM t0 NATURAL CROSS JOIN v0 NATURAL FULL JOIN t1 INNER JOIN t2 ON (((((t2.c0)IS(v0.c0)))));

-- case: research/sqlite-forum/natural_left_join_subquery_cast_right_join | source: https://sqlite.org/forum/forumpost/829306db47
CREATE TABLE t0(c0);
CREATE TABLE t1(c0);
CREATE TABLE t2(c0);
INSERT INTO t0 VALUES ('1.0');
INSERT INTO t2(c0) VALUES (9);
SELECT t0.c0,t2.c0 FROM (SELECT CAST(t0.c0 as REAL) AS c0 FROM t0) as subquery NATURAL LEFT JOIN t1 NATURAL JOIN t0 RIGHT JOIN t2 ON 1;

-- case: research/sqlite-forum/flatten_union_all_cast_text_compare | source: https://sqlite.org/forum/forumpost/1821d30133c8e544
CREATE TABLE t0(c0 INT,c1 INT);
INSERT INTO t0 VALUES(10,10);
SELECT * FROM t0 JOIN (SELECT CAST(c0 AS TEXT) AS c2 FROM t0 UNION ALL SELECT c1 FROM t0) WHERE 10=c2;

-- case: research/sqlite-forum/exists_to_join_limit_offset | source: https://sqlite.org/forum/info/2c43f36255a630e3
CREATE TABLE t2(id INT, data INT);
CREATE TABLE t3(amount INT);
INSERT INTO t2 VALUES (1,0),(2,0);
INSERT INTO t3 VALUES (1),(1);
SELECT COUNT(*) AS matched FROM t2 WHERE EXISTS (SELECT 1 FROM t3 WHERE t3.amount > t2.data);
SELECT COUNT(*) AS rows_out FROM (SELECT id FROM t2 WHERE EXISTS (SELECT 1 FROM t3 WHERE t3.amount > t2.data) LIMIT 2 OFFSET 3);

-- case: research/sqlite-forum/likely_affinity_in_empty_list_view_join | source: https://sqlite.org/forum/forumpost/45ec3d9788
CREATE TABLE vt0(c0 integer);
CREATE TABLE t1 (c0 INT);
CREATE VIEW IF NOT EXISTS v0(c0, c1, c2) AS SELECT t1.c0, t1.c0, CAST(((t1.c0) IS TRUE) AS TEXT) FROM t1, vt0 WHERE (x'') NOTNULL;
INSERT OR ROLLBACK INTO vt0 VALUES ('	x');
CREATE INDEX IF NOT EXISTS i46 ON t1(CAST(((c0) IS TRUE) AS TEXT));
INSERT OR IGNORE INTO t1(c0) VALUES (NULL);
SELECT count(*) FROM t1, v0 WHERE (t1.c0 IN ())<(LIKELY(v0.c2));
SELECT count(*) FROM t1, v0 WHERE (t1.c0 IN ())<(LIKELY(v0.c2)) IS TRUE;

-- case: research/sqlite-forum/cte_union_all_pushdown_where_false_shared_subquery | source: https://sqlite.org/forum/forumpost/2417eac1f3233bf6
WITH t1(x) AS (SELECT 111), t2(y) AS (SELECT 222), t3(z) AS (SELECT * FROM t2 WHERE false UNION ALL SELECT * FROM t2)
SELECT * FROM t1, t3;

-- case: research/sqlite-forum/distinct_right_join_natural_unique | source: https://sqlite.org/forum/forumpost/c06b10ad7e
CREATE TABLE t1 (c0 INTEGER UNIQUE);
CREATE TABLE t2 (c0);
CREATE TABLE t3 (c0);
INSERT INTO t1 VALUES (1);
INSERT INTO t2 VALUES (2);
INSERT INTO t3 VALUES (3);
SELECT t1.c0, t3.c0 FROM t2 NATURAL JOIN t1 RIGHT OUTER JOIN t3 ON t1.c0;
SELECT DISTINCT t1.c0, t3.c0 FROM t2 NATURAL JOIN t1 RIGHT OUTER JOIN t3 ON t1.c0;

-- case: research/sqlite-forum/right_join_json_each_on_constraint | source: https://sqlite.org/forum/forumpost/422e635f3beafbf6
CREATE TABLE a(key TEXT);
INSERT INTO a(key) VALUES('a'),('b');
SELECT a.key, b.value FROM a RIGHT JOIN json_each('["a","c"]') AS b ON a.key=b.value;

-- case: research/sqlite-forum/right_join_isnull_not_null_column_after_left_join_demotion | source: https://sqlite.org/forum/forumpost/b40696f501
CREATE TABLE t0(c0 INTEGER);
INSERT INTO t0 VALUES('x');
CREATE TABLE t1(c0 INTEGER);
INSERT INTO t1 VALUES('y');
CREATE TABLE t2(c0, c1 NOT NULL);
INSERT INTO t2 VALUES('a', 'b');
CREATE TABLE t3(c0 INTEGER);
INSERT INTO t3 VALUES('c');
SELECT * FROM t3 LEFT OUTER JOIN t2 INNER JOIN t0 ON t2.c1 RIGHT OUTER JOIN t1 ON t2.c0;
SELECT * FROM t3 LEFT OUTER JOIN t2 INNER JOIN t0 ON t2.c1 RIGHT OUTER JOIN t1 ON t2.c0 WHERE (((t2.c1) ISNULL));

-- case: research/sqlite-forum/right_join_redundant_on_and_where_constraint | source: https://sqlite.org/forum/forumpost/eeb8173cf8
CREATE TABLE t1(a INT, b BOOLEAN);
CREATE TABLE t2(c INT);
INSERT INTO t2 VALUES(NULL);
CREATE TABLE t3(d INT);
SELECT * FROM t1 JOIN t3 ON (b=TRUE) RIGHT JOIN t2 ON TRUE WHERE (b IS TRUE);

-- case: research/sqlite-forum/right_join_left_join_partial_index | source: https://sqlite.org/forum/forumpost/7dee41d32506c4ae
CREATE TABLE t1(x INT);      INSERT INTO t1(x) VALUES(1);
CREATE TABLE t2(y BOOLEAN);  INSERT INTO t2(y) VALUES(false);
CREATE TABLE t3(z INT);      INSERT INTO t3(z) VALUES(3);
CREATE INDEX t2y ON t2(y) WHERE y;
SELECT z FROM t1 RIGHT JOIN t2 ON y LEFT JOIN t3 ON y;

-- case: research/sqlite-forum/left_join_subquery_total_window_where | source: https://sqlite.org/forum/forumpost/0109bca824
CREATE TABLE t0 (c0 INT);
CREATE TABLE t1 (c0 INT);
INSERT INTO t1 VALUES (1);
INSERT INTO t0 VALUES (0);
SELECT (t0.c0 is null), t1.c0 FROM (SELECT TOTAL(0) OVER () AS col_0 FROM t0) as subQuery LEFT JOIN t0 ON ((CASE 1 WHEN 1 THEN subQuery.col_0 END) LIKE (((((subQuery.col_0)))))) INNER JOIN t1 ON ((subQuery.col_0) == (false));

-- case: research/sqlite-forum/natural_left_join_not_null_column_right_join | source: https://sqlite.org/forum/forumpost/4fc70203b61c7e12
CREATE TABLE t1 (c0 INT, c1 INT);
CREATE TABLE t2 (c0 INT NOT NULL);
INSERT INTO t1(c1) VALUES (1);
SELECT * FROM t2 RIGHT JOIN (SELECT 1) as subQuery1 ON TRUE NATURAL LEFT JOIN t1;

-- case: research/sqlite-forum/right_join_view_subquery_where_inner_join | source: https://sqlite.org/forum/info/5c8a069d23
CREATE TABLE t0(c0 INT, c1 INT);
CREATE TABLE t1(c0 INT, c1 TEXT);
INSERT INTO t1 (c0, c1) VALUES (1, 0);
CREATE VIEW v0(c0) AS SELECT 'a' FROM t0 NATURAL LEFT JOIN t1;
INSERT INTO t0 (c1) VALUES (0);
SELECT t1.c0 FROM v0 INNER JOIN (SELECT '?' AS col0 FROM v0) AS sub0 ON (1|sub0.col0), t1 RIGHT JOIN (SELECT 0 AS col0 FROM v0) AS sub1 ON true;
SELECT t1.c0 FROM v0 INNER JOIN (SELECT '?' AS col0 FROM v0) AS sub0 ON (1|sub0.col0), t1 RIGHT JOIN (SELECT 0 AS col0 FROM v0) AS sub1 ON true WHERE t1.c0;

-- case: research/sqlite-forum/left_right_join_view_subquery_where_column | source: https://sqlite.org/forum/forumpost/3f676b1196
CREATE TABLE t0(c0 INT, c1 INT, c3 INT);
CREATE TABLE t1(c0 INT);
INSERT INTO t0 (c1, c0, c3) VALUES (1, 0, 1);
CREATE VIEW v0(c0) AS SELECT 0 FROM t1 RIGHT JOIN t0 ON 1;
SELECT t0.c3 FROM v0 LEFT JOIN (SELECT 'a' AS col0 FROM v0 WHERE false) AS sub0 ON v0.c0, t0 RIGHT JOIN (SELECT (NULL) AS col0 FROM v0) AS sub1 ON t0.c3;
SELECT t0.c3 FROM v0 LEFT JOIN (SELECT 'a' AS col0 FROM v0 WHERE false) AS sub0 ON v0.c0, t0 RIGHT JOIN (SELECT (NULL) AS col0 FROM v0) AS sub1 ON t0.c3 WHERE t0.c3;

-- case: research/sqlite-forum/left_join_empty_aggregate_bare_column_is_null | source: https://sqlite.org/forum/info/5b4fc7d582bfa90396f6c9907f8ad1086582459a89494c7575edd3fec7e4d1ab
CREATE TABLE t0(X INT NOT NULL);
SELECT t1.X IS NULL, COUNT(*) FROM t0 t1 LEFT JOIN t0 t2 ON t1.X = t2.X WHERE t1.X IS NOT NULL OR 1=1;

-- case: research/sqlite-forum/distinct_union_all_count_pushdown | source: https://sqlite.org/forum/forumpost/a860f5fb2e
CREATE TABLE a(a INT);
INSERT INTO a VALUES (1), (1);
SELECT count() FROM (SELECT DISTINCT * FROM a UNION ALL SELECT * FROM a);

-- case: research/sqlite-forum/full_join_degenerate_index_distinct_view | source: https://sqlite.org/forum/forumpost/ecdfc02339
CREATE VIRTUAL TABLE vt0 USING fts5(c0, c1 UNINDEXED);
CREATE TABLE t1 (c2 float);
CREATE INDEX i0 ON t1(NULL);
INSERT INTO t1(c2) VALUES (0.2);
CREATE VIEW v0(c3) AS SELECT DISTINCT c2 FROM t1;
SELECT c2 FROM v0 FULL OUTER JOIN vt0 ON ((UPPER( c3))<(NULL)) LEFT OUTER JOIN t1 ON 1;
SELECT c2 FROM v0 FULL OUTER JOIN vt0 ON ((UPPER( c3))<(NULL)) LEFT OUTER JOIN t1 ON 1 WHERE c2/0.1;

-- case: research/sqlite-forum/full_join_using_shared_column_value | source: https://sqlite.org/forum/info/9ededf8dbac773000c29fe3d4dce8781fdea85a6db6dccac1bdcb37a0c7800f4
CREATE TABLE t1 (ID INTEGER);
CREATE TABLE t2 (ID INTEGER);
INSERT INTO t1 VALUES (1);
INSERT INTO t2 VALUES (2);
SELECT * FROM t1 FULL JOIN t2 USING (ID);

-- case: research/sqlite-forum/window_view_isnull_notnull_reduction | source: https://sqlite.org/forum/forumpost/440f2a2f17
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY, c2 TEXT);
CREATE VIEW v5 AS SELECT c1, COUNT(*) AS y, sum(c2) OVER (PARTITION BY c1) FROM v0;
SELECT c1 FROM v5 WHERE c1 IS NULL;

-- case: research/sqlite-forum/window_total_view_count_where | source: https://sqlite.org/forum/forumpost/2cd11c2d37
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
CREATE VIEW v0(c0) AS SELECT TOTAL(0) OVER (PARTITION BY t0.c0) FROM t0;
SELECT COUNT(*) FROM v0 WHERE ('1' IS NOT('1' NOTNULL))-(0);

-- case: research/sqlite-forum/window_min_filter_rows_frame_ignored | source: https://sqlite.org/forum/forumpost/e9126d554a
CREATE TABLE t0 (c0 INTEGER, c1 INTEGER);
INSERT INTO t0 (c0, c1) VALUES (10, 1), (20, -1), (5, 2), (15, 0), (25, 3);
SELECT c0, c1, MIN(c0) FILTER(WHERE c1 > 0) OVER (ORDER BY c0 ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS min_filtered_rows_frame FROM t0;

-- case: research/sqlite-forum/window_sum_total_avg_overflow_rows_frame | source: https://sqlite.org/forum/forumpost/ec538b04ce
WITH t(id,x) AS (VALUES (1,-1), (2,9223372036854775807), (3,1), (4,0.5)) SELECT id, sum(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING), total(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING), avg(x) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING) FROM t ORDER BY id;

-- case: research/sqlite-forum/window_cte_in_named_window_partition_by | source: https://sqlite.org/forum/forumpost/6e54dbeb22
CREATE TABLE t_sa ( c_muyat INTEGER NOT NULL, c_d4u TEXT , c_lngdt TEXT NOT NULL, c_c3v INTEGER , primary key(c_c3v), unique(c_muyat), check(1=1) );
WITH cte_0 AS (select ref_0.c_muyat as c1 from t_sa as ref_0 ) select LAG(cast(cast(null as INTEGER) as INTEGER)) over gen8fjew as c0 from t_sa as ref_5 window gen8fjew as ( partition by (select c1 from cte_0 order by c1 limit 1 offset 4));

-- case: research/sqlite-forum/window_group_concat_separator_frame_slide | source: https://sqlite.org/forum/forumpost/ccf3b5673ba852cf?raw
CREATE TABLE persons(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO persons (name) VALUES('John'), ('Paul'), ('George'), ('Ringo');
WITH RECURSIVE g(value) AS (SELECT 4450 UNION ALL SELECT value+1 FROM g WHERE value<4455) SELECT group_concat(value,name) OVER (ORDER BY name ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS result FROM persons, g;

-- case: research/sqlite-forum/window_group_concat_empty_string_is_null | source: https://sqlite.org/forum/forumpost/bf8f43aa522c2299
SELECT group_concat(x) OVER () FROM (SELECT '' AS x);

-- case: research/sqlite-forum/window_nth_value_hides_count_in_order_by | source: https://sqlite.org/forum/forumpost/540fdfef77?t=c
CREATE TABLE v0 (v2 INT, v1 INT);
INSERT INTO v0 VALUES (1, 10), (1, 10), (1, 3), (1, 3);
SELECT rowid, v2, v1, nth_value(v1, 1) OVER () FROM v0 ORDER BY (COUNT());

-- case: research/sqlite-forum/window_count_in_order_by_subquery_inconsistent | source: https://sqlite.org/forum/forumpost/99d452c4a8?t=c&hist
CREATE TABLE v0 ( v2 INT, v1 INT );
INSERT INTO v0 VALUES ( 1, 10 ), ( 1, 0 ), ( 10, 10 ), ( 10, 0 );
CREATE INDEX v3 ON v0 ( v2, v1 );
SELECT * FROM v0 WHERE ( v2 IN ( SELECT v2 FROM v0 ORDER BY max ( nth_value ( v1, 10 ) OVER( ), COUNT () ) ));
SELECT (v2 IN ( SELECT v2 FROM v0 ORDER BY max ( nth_value ( v1, 10 ) OVER( ), COUNT ( ) ) )) FROM v0;

-- case: research/sqlite-forum/window_default_frame_count_peer_groups | source: https://sqlite.org/forum/forumpost/74f22b6905b908e2
create table Test (x integer);
insert into Test values (1), (3), (2), (5), (4), (6), (7), (9), (7), (4), (3), (1);
select x, row_number() over Win1 as Row, count() over Win1 as TotalRows from Test window Win1 as (order by x);

-- case: research/sqlite-forum/window_running_sum_identity_type_name | source: https://sqlite.org/forum/forumpost/97a677f8c1bd5f40ef9404542889602b6ea38fffe8826e1f078275d05372606f
CREATE TABLE CUENTAS22 (id integer IDENTITY primary key, N_CUENT varchar(6) NOT NULL, DEB Numeric(15) NOT NULL DEFAULT 0, HAB Numeric(15) NOT NULL DEFAULT 0);
INSERT INTO CUENTAS22(N_CUENT, DEB, HAB) VALUES ('0001', 1, 0), ('0002', 2, 0), ('0003', 3, 0), ('0004', 0, 10), ('0005', 1, 20), ('0006', 0, 30);
SELECT id, sum(HAB-DEB) over (order by id asc) as runsald FROM CUENTAS22;

-- case: research/sqlite-forum/recursive_cte_distinct_initial_select | source: https://sqlite.org/forum/info/1d3b0519e2
CREATE TABLE t (label TEXT, step INTEGER);
INSERT INTO T VALUES('a', 1);
INSERT INTO T VALUES('a', 1);
INSERT INTO T VALUES('b', 1);
WITH RECURSIVE cte(label, step) AS (SELECT DISTINCT * FROM t UNION ALL SELECT label, step + 1 FROM cte WHERE step < 3) SELECT * FROM cte;

-- case: research/sqlite-forum/recursive_cte_distinct_two_columns | source: https://sqlite.org/forum/info/7d969ba1eaa450b1
CREATE TABLE t1 (a);
INSERT INTO t1 VALUES (1), (1), (1), (2);
WITH RECURSIVE r1 AS (SELECT DISTINCT a, a b FROM t1 UNION ALL SELECT a, b+1 b FROM r1 WHERE b < 3) SELECT * FROM r1;

-- case: research/sqlite-forum/distinct_right_join_unique_empty | source: https://sqlite.org/forum/forumpost/c06b10ad7e
CREATE TABLE t1 (c0 INTEGER UNIQUE);
CREATE TABLE t2 (c0);
CREATE TABLE t3 (c0);
INSERT INTO t1 VALUES (1);
INSERT INTO t2 VALUES (2);
INSERT INTO t3 VALUES (3);
SELECT DISTINCT t1.c0, t3.c0 FROM t2 NATURAL JOIN t1 RIGHT OUTER JOIN t3 ON t1.c0;

-- case: research/sqlite-forum/count_over_distinct_union_all_subquery | source: https://sqlite.org/forum/forumpost/aeae62275ebbf584
CREATE TABLE t1(id TEXT);
INSERT INTO t1 VALUES('a'),('b');
SELECT count(1) FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);

-- case: research/sqlite-forum/avg_infinity_float_column | source: https://sqlite.org/forum/forumpost/8960fb40cc
create table foo (bar float);
insert into foo values (37.5);
insert into foo values (1e10000);
select avg(bar) from foo;

-- case: research/sqlite-forum/sum_double_max_with_where | source: https://sqlite.org/forum/forumpost/23b8688ef4?t=c
CREATE TABLE t0 (c0 DOUBLE);
INSERT INTO t0(c0) VALUES (1),(2),(3);
SELECT SUM(1.7976931348623157E308) as aggr FROM t0 WHERE c0 > 1;

-- case: research/sqlite-forum/compound_cte_pushdown_cache_union_all | source: https://sqlite.org/forum/forumpost/d496c3d29b
WITH T1 (x) AS (SELECT 10), T2 (y) AS (SELECT 1), T3 (z) AS (SELECT * FROM T2 WHERE 0 UNION ALL SELECT * FROM T2) SELECT * FROM T1 INNER JOIN T2 INNER JOIN T3;

-- case: research/sqlite-forum/compound_union_all_view_full_join_flatten | source: https://sqlite.org/forum/forumpost/174afeae5734d42d
CREATE TABLE t1(a INT);
CREATE TABLE t2(b INT, c INT);
CREATE VIEW t3(d) AS SELECT NULL FROM t2 FULL OUTER JOIN t1 ON c=a UNION ALL SELECT b FROM t2;
INSERT INTO t1(a) VALUES (NULL);
INSERT INTO t2(b, c) VALUES (99, NULL);
SELECT DISTINCT * FROM t2, t3 WHERE b<>0 UNION SELECT DISTINCT * FROM t2, t3 WHERE b ISNULL;

-- case: research/sqlite-forum/limit_offset_union_all_view_two_order_by_arms | source: https://sqlite.org/forum/forumpost/6b5e9188f0657616
CREATE TABLE employees (id INTEGER PRIMARY KEY, salary INTEGER);
INSERT INTO employees VALUES (11, 70), (12, 78), (21, 84), (22, 90), (23, 104), (24, 104), (25, 120), (31, 96), (32, 96), (33, 100);
CREATE VIEW v AS SELECT * FROM (SELECT * FROM employees WHERE salary < 100 ORDER BY salary DESC) UNION ALL SELECT * FROM (SELECT * FROM employees WHERE salary >= 100 ORDER BY salary ASC);
SELECT * FROM v LIMIT 5 OFFSET 0;

-- case: research/sqlite-forum/limit_offset_beyond_distinct_order_by_subquery | source: https://sqlite.org/forum/forumpost/a5148faa49?t=c
CREATE TABLE t1(x);
INSERT INTO t1 VALUES(1),(1),(1);
SELECT (SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 100) FROM t1;
SELECT (SELECT DISTINCT x FROM t1 ORDER BY x DESC LIMIT 1 OFFSET 100) FROM t1;

-- case: research/sqlite-forum/exists_is_operator_unique_transitive | source: https://sqlite.org/forum/info/7e7078acd46c32aa
CREATE TABLE t1(a INT);
INSERT INTO t1 VALUES(0),(3);
CREATE TABLE t2(b INT UNIQUE, c INT);
INSERT INTO t2 VALUES(1,4),(0,5);
SELECT * FROM t1 WHERE EXISTS (SELECT 1 FROM t2 WHERE a=c AND a IS b);

-- case: research/sqlite-forum/or_short_circuit_returns_operand | source: https://sqlite.org/forum/info/26663c3b45badce971d81612ffd14aa0fd8b542ebb48a7d0566659fdea1c00b2
CREATE TABLE t0(c0);
INSERT INTO t0 VALUES (1);
SELECT * FROM (SELECT 0 AS col_0) LEFT JOIN (SELECT 1 AS col_1, (0 OR 2) AS col_2 FROM t0) as subQuery ON (subQuery.col_1 % subQuery.col_2);
SELECT 0 OR 2;

-- case: research/sqlite-forum/bloom_filter_rtrim_collation_distinct_view | source: https://sqlite.org/forum/forumpost/0846211821
CREATE TABLE t0 (c0);
INSERT INTO t0 VALUES (0), (0), (0), (' '), (0), (0), (0), (0), (1);
CREATE VIEW v0(c0) AS SELECT DISTINCT t0.c0 FROM t0;
SELECT COUNT(*) FROM t0, v0 WHERE t0.c0 COLLATE RTRIM = '';

-- case: research/sqlite-forum/aggregate_bare_column_empty_subquery_constant | source: https://sqlite.org/forum/info/e4def599a5bd68c5
CREATE TABLE t0(c0);
SELECT COUNT(t1.c1), t1.c1 FROM (SELECT 1 AS c1 FROM t0) AS t1;

-- case: research/sqlite-forum/aggregate_group_concat_correlated_outer_column | source: https://sqlite.org/forum/info/9310273f0061c950
CREATE TABLE x AS SELECT 1 a;
CREATE TABLE y AS SELECT 1 b UNION ALL SELECT 1;
SELECT (SELECT group_concat(a) FROM y) FROM x;

-- case: research/sqlite-forum/aggregate_group_concat_ordered_subquery | source: https://www.sqlite.org/forum/forumpost/f6073078fb?t=c
with t(x) AS ( values(3),(1),(2) ) select group_concat(x, ', ') from (select x from t order by x);

-- case: research/sqlite-forum/aggregate_max_bare_columns_two_maxes_having | source: https://sqlite.org/forum/forumpost/4e1de86205d55444?t=c
CREATE TABLE dalstock(id INTEGER PRIMARY KEY, name TEXT, ist INTEGER, stock INTEGER);
INSERT INTO dalstock VALUES (1,'a',1,10),(2,'a',2,20),(3,'a',3,30),(4,'b',1,5),(5,'b',2,6);
SELECT max(b.id) id, b.ist, b.name, b.stock FROM dalstock a LEFT JOIN dalstock b ON a.name = b.name AND a.ist > b.ist GROUP BY a.name HAVING max(b.ist);

-- case: research/sqlite-forum/compound_order_by_function_not_in_result_error | source: https://sqlite.org/forum/forumpost/f149ab2706
SELECT 'Abc' as name UNION SELECT 'abd' as name ORDER BY lower(name);

-- case: research/sqlite-forum/compound_order_by_substr_not_in_result_error | source: https://sqlite.org/forum/forumpost/e634fe817e
select 'abcdefg' x union select 'hijklmn' order by substr(x, 2, 1);

-- case: research/sqlite-forum/distinct_group_by_subquery_order_by_copy | source: https://sqlite.org/src/tktview/98825a79ce1456863
CREATE TABLE t1(x);
INSERT INTO t1 VALUES('right'),('wrong');
SELECT DISTINCT x FROM (SELECT x FROM t1 GROUP BY x) WHERE x='right' ORDER BY x;

-- case: research/sqlite-forum/upsert_generated_column_before_set_null | source: https://sqlite.org/forum/forumpost/73b9a8ccfb
CREATE TABLE tab (prim DATE PRIMARY KEY, a INTEGER, comp INTEGER AS (a), b INTEGER);
INSERT INTO tab (prim, a, b) VALUES ('2001-01-01', 0, 0);
INSERT INTO tab (prim, b) VALUES ('2001-01-01', 5) ON CONFLICT(prim) DO UPDATE SET b=excluded.b;
SELECT * FROM tab;
PRAGMA integrity_check;

-- case: research/sqlite-forum/update_returning_inconsistent_snapshot | source: https://sqlite.org/forum/forumpost/9470611066
CREATE TABLE Parent (ParentId INT NOT NULL PRIMARY KEY);
CREATE TABLE Child (ChildId INT NOT NULL PRIMARY KEY, ParentId INT NOT NULL);
INSERT INTO Parent (ParentId) VALUES (1), (2);
INSERT INTO Child (ChildId, ParentId) VALUES (1, 1), (2, 1), (3, 2), (4, 2), (5, 2);
UPDATE Child SET ParentId = 1 WHERE ParentId = 2 RETURNING JSON_OBJECT('ChildId', [Child].[ChildId], 'Parent', (SELECT JSON_OBJECT('ParentId', [Parent].[ParentId], 'Children', (SELECT JSON_GROUP_ARRAY(ChildObj) FROM (SELECT JSON_OBJECT('ChildId', child2.ChildId) AS ChildObj FROM Child child2 WHERE child2.ParentId = parent.ParentId))) FROM Parent parent WHERE Parent.ParentId = [Child].[ParentId])) AS data;

-- case: research/sqlite-forum/returning_fk_violation_emits_row | source: https://sqlite.org/forum/forumpost/21127c1160
PRAGMA foreign_keys = 1;
CREATE TABLE Parent(id INTEGER PRIMARY KEY);
CREATE TABLE Child(id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL REFERENCES Parent(id));
INSERT INTO child (parent_id) VALUES (666) RETURNING id;
SELECT count(*) FROM Child;

-- case: research/sqlite-forum/replace_redundant_upsert_clauses_corrupt_index | source: https://sqlite.org/forum/forumpost/919c6579c8
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY ON CONFLICT REPLACE, c2 UNIQUE );
INSERT INTO v0 VALUES ( 0, 33 ), ( 11, 22 );
REPLACE INTO v0 VALUES ( 0, 11 ) ON CONFLICT ( c2 ) DO UPDATE SET c1 = c2, c2 = c2 ON CONFLICT ( c2 ) DO UPDATE SET c1 = c1, c2 = c1;
SELECT count(*) FROM v0;
SELECT count(*) FROM v0 WHERE c2 > 8;
PRAGMA integrity_check;

-- case: research/sqlite-forum/fk_cascade_cached_trigger_after_drop_create | source: https://sqlite.org/forum/forumpost/e3d7ff6532ddde59
CREATE TABLE t1(a PRIMARY KEY,b);
CREATE TABLE t2(c PRIMARY KEY REFERENCES t1 ON DELETE CASCADE,d);
PRAGMA foreign_keys = ON;
DROP TABLE t1;
CREATE TABLE t1(x PRIMARY KEY, y);
INSERT INTO t1 VALUES(1,2);
DELETE FROM t1;
SELECT count(*) FROM t1;
SELECT count(*) FROM t2;
PRAGMA foreign_key_check;

-- case: research/sqlite-forum/partial_index_null_predicate_is_not_null | source: https://sqlite.org/forum/forumpost/67b737942462fdd2
CREATE TABLE v0 ( v2 INT, v1 INT);
INSERT INTO v0 VALUES ( 10, 10 );
CREATE UNIQUE INDEX v4 ON v0 ( v1 ) WHERE v2 > NULL;
SELECT * FROM v0 WHERE v0.v2 IS NOT NULL;
SELECT * FROM v0;

-- case: research/sqlite-forum/partial_index_not_null_where_or_one | source: https://www.sqlite.org/src/tktview/5c6955204c
CREATE TABLE t0(c0);
CREATE INDEX index_0 ON t0(c0) WHERE c0 NOT NULL;
INSERT INTO t0(c0) VALUES (NULL);
SELECT * FROM t0 WHERE c0 OR 1;
SELECT count(*) FROM t0;

-- case: research/sqlite-forum/row_value_in_subquery_update_skips_rows | source: https://sqlite.org/forum/forumpost/b9647a113b465950
CREATE TABLE items (Id INTEGER PRIMARY KEY, Item INTEGER, Test TEXT, Filler, UNIQUE (Item, Id));
INSERT INTO items (Id, Item) VALUES (1, 2), (2, 3), (3, 3), (4, 4);
UPDATE items SET Test = 'ok' WHERE (Id, Item) IN (SELECT Id, Item FROM items);
SELECT Id, Item, Test FROM items;

-- case: research/sqlite-forum/defer_fk_drop_then_rename_commit_fails | source: https://sqlite.org/forum/info/825039816c3f00972b4d66f48eb75a361f9caad0d286ef1c586af0dd4b65f4cd
PRAGMA foreign_keys = 1;
CREATE TABLE Node(node_oid INTEGER PRIMARY KEY NOT NULL);
CREATE TABLE Job(node_oid INTEGER NOT NULL, FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
INSERT INTO Node(node_oid) VALUES (0);
INSERT INTO Job(node_oid) VALUES (0);
BEGIN;
PRAGMA defer_foreign_keys = ON;
CREATE TABLE Node_migration_new(node_oid INTEGER PRIMARY KEY NOT NULL);
INSERT INTO Node_migration_new(node_oid) SELECT node_oid FROM Node;
DROP TABLE Node;
ALTER TABLE Node_migration_new RENAME TO Node;
PRAGMA foreign_key_check;
COMMIT;
SELECT * FROM Node;
SELECT * FROM Job;
PRAGMA foreign_key_check;

-- case: research/sqlite-forum/update_from_join_on_references_target | source: https://sqlite.org/forum/forumpost/df23d80682
CREATE TABLE t1(aa INT, bb INT);
CREATE TABLE t2(mm INT, nn INT);
CREATE TABLE t3(xx INT, yy INT);
INSERT INTO t1 VALUES(1, 0);
INSERT INTO t2 VALUES(1, 5);
INSERT INTO t3 VALUES(5, 0);
UPDATE t1 SET bb = mm+xx FROM t2 INNER JOIN t3 ON nn=xx AND mm=aa;
SELECT * FROM t1;

-- case: research/sqlite-forum/update_from_join_where_references_target | source: https://sqlite.org/forum/forumpost/df23d80682
CREATE TABLE t1(aa INT, bb INT);
CREATE TABLE t2(mm INT, nn INT);
CREATE TABLE t3(xx INT, yy INT);
INSERT INTO t1 VALUES(1, 0);
INSERT INTO t2 VALUES(1, 5);
INSERT INTO t3 VALUES(5, 0);
UPDATE t1 SET bb = mm+xx FROM t2, t3 WHERE nn=xx AND mm=aa;
SELECT * FROM t1;

-- case: research/sqlite-forum/strict_update_text_coerced_to_real | source: https://sqlite.org/forum/forumpost/bdd6bbd473e8dfb3
CREATE TABLE tblTranNonQuery (Fld1 INT, Fld2 TEXT, Fld3 REAL) STRICT;
INSERT INTO tblTranNonQuery VALUES (1, 'Row1', 100), (2, 'Row2', 200);
UPDATE tblTranNonQuery SET Fld3 = '12.22' WHERE Fld1 = 1;
SELECT Fld1, Fld2, Fld3, typeof(Fld3) FROM tblTranNonQuery ORDER BY Fld1;

-- case: research/sqlite-forum/update_returning_is_null_on_not_null_pk_neighbor | source: https://sqlite.org/forum/forumpost/d010a26798915b53
CREATE TABLE bug(id INTEGER PRIMARY KEY NOT NULL, x);
INSERT INTO bug(id,x) VALUES(20, NULL);
UPDATE bug SET x=NULL WHERE id = 20 RETURNING x, x IS NULL;
SELECT x, x IS NULL FROM bug;

-- case: research/sqlite-forum/strict_generated_real_from_integer_literal | source: https://sqlite.org/forum/forumpost/fa012c77796d9399
CREATE TABLE t1(x REAL, y REAL AS (x)) STRICT;
INSERT INTO t1 VALUES(5);
SELECT x, y, typeof(x), typeof(y) FROM t1;

-- case: research/sqlite-forum/strict_stored_generated_column_types_unchecked | source: https://sqlite.org/forum/forumpost/6caf195248a849e4
CREATE TABLE strict (k INTEGER PRIMARY KEY, c1 REAL AS (CASE k WHEN 11 THEN 1.5 WHEN 12 THEN 2 WHEN 13 THEN 'x' WHEN 14 THEN x'34' ELSE 0.0 END) STORED, c2 INT AS (CASE k WHEN 21 THEN 1.5 WHEN 22 THEN 2 WHEN 23 THEN 'x' WHEN 24 THEN x'34' ELSE 0 END) STORED) STRICT;
INSERT INTO strict(k) VALUES(11);
INSERT INTO strict(k) VALUES(13);
SELECT k, c1, typeof(c1) FROM strict ORDER BY k;

-- case: research/sqlite-forum/delete_negated_and_with_self_subquery | source: https://sqlite.org/forum/forumpost/e61252062c9d286d
CREATE TABLE t0 (vkey INTEGER, pkey INTEGER, c1 INTEGER);
INSERT INTO t0 VALUES(2,1,-20);
INSERT INTO t0 VALUES(2,2,NULL);
INSERT INTO t0 VALUES(2,3,0);
INSERT INTO t0 VALUES(8,4,95);
DELETE FROM t0 WHERE NOT ((t0.vkey <= t0.c1) AND (t0.vkey <> (SELECT vkey FROM t0 ORDER BY vkey LIMIT 1 OFFSET 2)));
SELECT * FROM t0 ORDER BY pkey;

-- case: research/sqlite-forum/upsert_before_update_trigger_old_values | source: https://sqlite.org/forum/forumpost/284955a3cd454a15
CREATE TABLE t1 (id INTEGER PRIMARY KEY, hash BLOB NOT NULL, data BLOB NOT NULL);
INSERT INTO t1 VALUES(1,unhex(format('31%.62c', '0')),format('%.36760c', 'a'));
INSERT INTO t1 VALUES(2,unhex(format('31%.62c', '0')),format('%.36760c', 'a'));
INSERT INTO t1 VALUES(3,unhex(format('32%.62c', '0')),format('%.36760c', 'b'));
INSERT INTO t1 VALUES(4,unhex(format('32%.62c', '0')),format('%.36760c', 'b'));
CREATE TABLE t2 (id INTEGER PRIMARY KEY, hash BLOB UNIQUE NOT NULL, data BLOB NOT NULL);
CREATE TABLE collisions (oldhash, newhash, old, new);
CREATE TRIGGER t2_collision BEFORE UPDATE OF data ON t2 WHEN old.hash = new.hash AND old.data != new.data BEGIN INSERT INTO collisions (oldhash, newhash, old, new) VALUES (old.hash, new.hash, old.data, new.data); END;
INSERT INTO t2 (hash, data) SELECT hash, data FROM t1 WHERE true ON CONFLICT (hash) DO UPDATE SET data = excluded.data;
SELECT count(*) FROM collisions;
SELECT id, length(data) FROM t2 ORDER BY id;

-- case: research/sqlite-forum/add_column_not_null_real_default_then_check | source: https://sqlite.org/forum/forumpost/ee4f6fa5ab
CREATE TABLE t (a);
INSERT INTO t (a) VALUES (0);
ALTER TABLE t ADD COLUMN b NOT NULL DEFAULT .25;
ALTER TABLE t ADD COLUMN c CHECK (1);
SELECT * FROM t;
PRAGMA integrity_check;

-- case: research/sqlite-forum/recursive_triggers_default_off | source: https://sqlite.org/forum/forumpost/78994cb84c
CREATE TABLE a(id INTEGER, fromA INTEGER, sharedProp TEXT);
CREATE TABLE b(id INTEGER, fromA INTEGER, sharedProp TEXT);
CREATE TRIGGER a_sharedProp_updated AFTER UPDATE ON a BEGIN UPDATE a SET sharedProp = NEW.sharedProp WHERE fromA = NEW.id; UPDATE b SET sharedProp = NEW.sharedProp WHERE fromA = NEW.id; END;
INSERT INTO a VALUES (1, NULL, 'foo'), (2, 1, 'foo');
INSERT INTO b VALUES (1, 2, 'foo');
UPDATE a SET sharedProp = 'bar' WHERE id = 1;
SELECT sharedProp FROM b;

-- case: research/sqlite-forum/recursive_triggers_enabled | source: https://sqlite.org/forum/forumpost/78994cb84c
PRAGMA recursive_triggers = 1;
CREATE TABLE a(id INTEGER, fromA INTEGER, sharedProp TEXT);
CREATE TABLE b(id INTEGER, fromA INTEGER, sharedProp TEXT);
CREATE TRIGGER a_sharedProp_updated AFTER UPDATE ON a BEGIN UPDATE a SET sharedProp = NEW.sharedProp WHERE fromA = NEW.id; UPDATE b SET sharedProp = NEW.sharedProp WHERE fromA = NEW.id; END;
INSERT INTO a VALUES (1, NULL, 'foo'), (2, 1, 'foo');
INSERT INTO b VALUES (1, 2, 'foo');
UPDATE a SET sharedProp = 'bar' WHERE id = 1;
SELECT sharedProp FROM b;
SELECT id, sharedProp FROM a ORDER BY id;

-- case: research/sqlite-forum/union_all_of_ordered_subqueries_limit_offset | source: https://sqlite.org/forum/forumpost/6b5e9188f0657616
CREATE TABLE t1(id INTEGER PRIMARY KEY, a INT);
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<100) INSERT INTO t1(a) SELECT 100-x FROM c;
SELECT * FROM (SELECT * FROM t1 WHERE a<50 ORDER BY a DESC) UNION ALL SELECT * FROM (SELECT * FROM t1 WHERE a>=50 ORDER BY a ASC) LIMIT 5 OFFSET 47;

-- case: research/sqlite-forum/distinct_order_by_limit_offset_past_end_subquery | source: https://sqlite.org/forum/forumpost/a5148faa49
CREATE TABLE t1(x);
INSERT INTO t1 VALUES(1),(1),(1);
SELECT (SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 100) FROM t1;

-- case: research/sqlite-forum/without_rowid_add_column_or_search_uses_unordered_column | source: https://sqlite.org/forum/forumpost/6c8960f545
CREATE TABLE v0 ( v1 INTEGER PRIMARY KEY ) WITHOUT ROWID;
INSERT INTO v0 VALUES (10);
ALTER TABLE v0 ADD v2 INT;
SELECT * FROM v0 WHERE ( v1 = 20 ) OR ( v1 = 10 AND v2 = 10 );

-- case: research/sqlite-forum/without_rowid_desc_pk_secondary_index_range | source: https://sqlite.org/forum/forumpost/8988341615
CREATE TABLE v0 ( v1 INT PRIMARY KEY DESC, v2 INT ) WITHOUT ROWID;
INSERT INTO v0 VALUES ( 10, 10 );
CREATE INDEX v3 ON v0 ( v2 );
SELECT * FROM v0 WHERE v2 = 10 AND v1 = 10;
SELECT * FROM v0 WHERE v2 = 10 AND v1 < 11;

-- case: research/sqlite-forum/without_rowid_partial_unique_index_update_pk | source: https://sqlite.org/forum/forumpost/e60e4c295d22f8ce
CREATE TABLE Repro (ID INTEGER NOT NULL PRIMARY KEY, Type INTEGER NOT NULL, Val INTEGER) WITHOUT ROWID;
CREATE UNIQUE INDEX Type1Unique ON Repro (Val) WHERE Type=1;
INSERT INTO Repro (ID, Type, Val) VALUES (8, 1, 2);
UPDATE Repro SET ID=0 WHERE Type=1 AND Val=2;
SELECT * FROM Repro;
PRAGMA integrity_check;

-- case: research/sqlite-forum/alter_rename_view_references_missing_table_after_legacy_rename | source: https://sqlite.org/forum/forumpost/2976d4cb91
CREATE TABLE main (column1, column2);
CREATE VIEW sorted AS SELECT * FROM main ORDER BY column1;
PRAGMA LEGACY_ALTER_TABLE = 1;
ALTER TABLE main RENAME TO other;
PRAGMA LEGACY_ALTER_TABLE = 0;
ALTER TABLE other RENAME TO main;
SELECT name FROM sqlite_master ORDER BY name;

-- case: research/sqlite-forum/alter_rename_column_unrelated_view_of_dropped_table | source: https://sqlite.org/forum/forumpost/8baf296bb3440dd5
PRAGMA legacy_alter_table = 0;
CREATE TABLE table1 (t1column1 t1column2);
CREATE TABLE table2 (t2column1 t2column2);
CREATE VIEW view1 AS SELECT t1column1 FROM table1;
DROP TABLE table1;
ALTER TABLE table2 ADD COLUMN newcolumn3;
ALTER TABLE table2 RENAME COLUMN newcolumn3 TO t2column3;
SELECT name, sql FROM sqlite_master ORDER BY name;

-- case: research/sqlite-forum/alter_rename_column_unrelated_recursive_cte_view | source: https://sqlite.org/forum/forumpost/31ceaa8461
CREATE TABLE t2(b,c);
INSERT INTO t2 VALUES(1,2),(1,3),(2,5);
CREATE VIEW v3 AS WITH RECURSIVE t3(x,y,z) AS (SELECT b,c,NULL FROM t2 UNION SELECT x,y,c FROM t3, t2 WHERE b=x ORDER BY y) SELECT * FROM t3;
SELECT * FROM v3;
CREATE TABLE t1(a);
ALTER TABLE t1 RENAME a TO a2;
SELECT name, sql FROM sqlite_master WHERE name = 't1';

-- case: research/sqlite-forum/alter_rename_table_index_on_in_empty_list_expression | source: https://www.sqlite.org/src/tktview/fd76310a5e843e07
CREATE TABLE t0(c0);
CREATE INDEX i0 ON t0('1' IN ());
ALTER TABLE t0 RENAME TO t1;
SELECT name, tbl_name FROM sqlite_master ORDER BY name;

-- case: research/sqlite-forum/upsert_do_nothing_in_trigger_ipk_replace | source: https://sqlite.org/forum/forumpost/06b16b8b29f8c8c3
CREATE TABLE t1 ( c INTEGER PRIMARY KEY ON CONFLICT REPLACE , b TEXT , a UNIQUE ) ;
INSERT INTO t1(b) VALUES(1);
CREATE TRIGGER c0 AFTER UPDATE ON t1 BEGIN INSERT INTO t1 VALUES(new.c, new.b, new.a) ON CONFLICT (a) DO NOTHING; END;
UPDATE t1 SET b = 0;
SELECT * FROM t1;
PRAGMA integrity_check;

-- case: research/sqlite-forum/savepoint_rollback_to_does_not_close_savepoint | source: https://sqlite.org/forum/info/eff47574371036e7
CREATE TABLE t1 (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
SAVEPOINT sp1;
INSERT INTO t1 (name, age) VALUES('item1', 1);
ROLLBACK TO sp1;
SAVEPOINT sp2;
INSERT INTO t1 (name, age) VALUES('item2', 2);
RELEASE sp2;
SELECT * FROM t1;
RELEASE sp1;
SELECT * FROM t1;

-- case: research/sqlite-forum/trigger_cte_in_subquery_allowed | source: https://sqlite.org/forum/forumpost/ee0ce4776135d816
CREATE TABLE x(x);
CREATE TABLE y(x);
CREATE TRIGGER xx AFTER INSERT ON x BEGIN INSERT INTO y SELECT x FROM (WITH t AS (SELECT * FROM new) SELECT x FROM t); END;
INSERT INTO x VALUES (1), (2);
SELECT x FROM y ORDER BY x;

-- case: research/sqlite-forum/trigger_cte_before_insert_syntax_error | source: https://sqlite.org/forum/forumpost/ee0ce4776135d816
CREATE TABLE x(x);
CREATE TABLE y(x);
CREATE TRIGGER xx AFTER INSERT ON x BEGIN WITH t AS (SELECT * FROM new) INSERT INTO y SELECT x FROM t; END;
SELECT count(*) FROM sqlite_master WHERE type = 'trigger';

-- case: research/sqlite-forum/update_from_same_table_ambiguous_column | source: https://sqlite.org/forum/forumpost/c4a2e5219545f0b8
CREATE TABLE activity_overview ("Project ID" INT, "Early Start" INT, D3 INT);
CREATE TABLE query_projects ("Project ID" INT, "Planned Start" INT);
INSERT INTO activity_overview VALUES (1, 1, 0);
INSERT INTO query_projects VALUES (1, 5);
UPDATE activity_overview SET D3 = 1 FROM activity_overview INNER JOIN query_projects ON activity_overview."Project ID" = query_projects."Project ID" WHERE activity_overview."Early Start" < query_projects."Planned Start";
SELECT * FROM activity_overview;

-- case: research/sqlite-forum/trigger_when_unqualified_column_with_upsert | source: https://sqlite.org/forum/forumpost/66d8f21fd1
CREATE TABLE Items (name TEXT PRIMARY KEY);
CREATE TRIGGER uppercase_name AFTER UPDATE ON Items WHEN lower(name) != upper(name) BEGIN UPDATE Items SET name = upper(name); END;
INSERT INTO Items VALUES ('Apple') ON CONFLICT (name) DO UPDATE SET name = excluded.name;
SELECT * FROM Items;

-- case: research/sqlite-forum/returning_table_qualified_wildcard_with_update_from | source: https://sqlite.org/forum/forumpost/417a7e08ff
CREATE TABLE x(a,b);
CREATE TABLE y(c,d);
INSERT INTO x VALUES (1,2);
INSERT INTO y VALUES (9,2);
UPDATE x SET a=c FROM y WHERE b=d RETURNING x.*;
SELECT * FROM x;

-- case: research/sqlite-forum/insert_default_values_into_ill_formed_view_returning | source: https://sqlite.org/forum/forumpost/aec099f0b4
CREATE TABLE v0 (c1 INT);
CREATE VIEW view_2 (c1) AS SELECT CASE WHEN c1 COLLATE TRUE THEN TRUE ELSE TRUE END FROM v0;
INSERT INTO view_2 DEFAULT VALUES RETURNING *;
SELECT count(*) FROM v0;

-- case: research/sqlite-forum/check_constraint_default_value_violates_check | source: https://sqlite.org/forum/forumpost/25a28c18aa64c400
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT NOT NULL DEFAULT 'abc' CHECK(length(s) < 3));
INSERT INTO t(id) VALUES(1);
SELECT count(*) FROM t;
PRAGMA integrity_check;

-- case: research/sqlite-forum/writable_schema_drop_not_null_then_add_column | source: https://sqlite.org/forum/forumpost/564a24a0becf8c91
BEGIN TRANSACTION;
CREATE TABLE NAMES(Id integer PRIMARY KEY, Name text NOT NULL);
INSERT INTO NAMES VALUES(1,'Tom');
INSERT INTO NAMES VALUES(2,'Lucy');
COMMIT;
BEGIN TRANSACTION;
PRAGMA writable_schema = on;
UPDATE sqlite_master SET sql = 'CREATE TABLE NAMES(Id integer PRIMARY KEY, Name text)' WHERE type = 'table' AND name = 'NAMES';
PRAGMA schema_version = 2;
PRAGMA writable_schema = off;
COMMIT;
ALTER TABLE NAMES ADD Age INTEGER DEFAULT 10;
SELECT * FROM NAMES;

-- case: research/sqlite-forum/add_not_null_column_without_default_on_empty_table | source: https://sqlite.org/forum/forumpost/21c7b443aa
CREATE TABLE test (Column1 INT NOT NULL);
ALTER TABLE test ADD COLUMN Column2 INT NOT NULL;
SELECT name, sql FROM sqlite_master;

-- case: research/sqlite-forum/integer_primary_key_compare_overflowed_expression | source: https://sqlite.org/forum/info/91d6da55c1102d05
CREATE TABLE v0(c0 INTEGER PRIMARY KEY);
CREATE TABLE v1(c1 INTEGER);
INSERT INTO v0 VALUES(9223372036854775807);
INSERT INTO v1 VALUES(9223372036854775807);
SELECT * FROM v0 WHERE v0.c0>=(9223372036854775807 + 1);
SELECT * FROM v1 WHERE v1.c1>=(9223372036854775807 + 1);

-- case: research/sqlite-forum/in_early_out_text_affinity | source: https://sqlite.org/forum/forumpost/f004a24c57b9ca21
CREATE TABLE t1(a TEXT, b TEXT);
INSERT INTO t1 VALUES(null,10),(0,10),(10,10);
CREATE INDEX t1ab ON t1(a,b);
SELECT * FROM t1 WHERE b IN (SELECT a FROM t1) AND a=0;

-- case: research/sqlite-forum/like_constprop_where_vs_select_a | source: https://sqlite.org/forum/forumpost/6a062026088e490c
CREATE TABLE v0 ( v2 REAL, v1 );
INSERT INTO v0 VALUES ( 10, 11 );
UPDATE v0 SET v1 = v2;
SELECT * FROM v0 WHERE v1 = 10 and v1 LIKE 10;
SELECT v1 = 10 and v1 LIKE 10 FROM v0;

-- case: research/sqlite-forum/like_constprop_where_vs_select_b | source: https://sqlite.org/forum/forumpost/ffa015ac07
CREATE TABLE v0 ( v2 REAL, v1 );
INSERT INTO v0 VALUES ( 10, 11 );
UPDATE v0 SET v1 = v2;
SELECT * FROM v0 WHERE v1 LIKE 10 AND v1 = 10;
SELECT * FROM v0 WHERE v1 LIKE 10;

-- case: research/sqlite-forum/compound_subquery_mixed_affinity_flatten | source: https://sqlite.org/forum/info/6dc048f81303cb97
CREATE TABLE t0(c0 INT, c1 INT);
INSERT INTO t0 VALUES(10, 10);
SELECT * FROM t0 JOIN (SELECT CAST(c0 AS TEXT) AS c2 FROM t0 UNION ALL SELECT c1 FROM t0) WHERE 10=c2;

-- case: research/sqlite-forum/likely_index_real_glob | source: https://sqlite.org/forum/forumpost/37dc1ebe44
CREATE TABLE t0 (c0 REAL);
INSERT INTO t0(c0) VALUES (0);
INSERT INTO t0(c0) VALUES (1);
CREATE UNIQUE INDEX idx ON t0(likely(c0));
SELECT count(*) FROM t0 INDEXED BY idx WHERE likely(c0) GLOB c0;
SELECT count(*) FROM t0 WHERE likely(c0) GLOB c0;

-- case: research/sqlite-forum/between_text_columns_numeric_literal | source: https://sqlite.org/forum/forumpost/9deb80dec8
CREATE TABLE my_table(id INTEGER PRIMARY KEY, col_a TEXT, col_b TEXT);
INSERT INTO my_table VALUES(1, '4011110000000', '4011119999999');
SELECT * FROM my_table WHERE 401111000001 BETWEEN col_a AND col_b;
SELECT * FROM my_table WHERE 401111000001 BETWEEN CAST(col_a AS INTEGER) AND CAST(col_b AS INTEGER);

-- case: research/sqlite-forum/in_list_text_column_integer_constants | source: https://sqlite.org/forum/forumpost/2c960de498
CREATE TABLE Answers (AnswerID, Answer);
CREATE UNIQUE INDEX idx_answers ON answers (AnswerID);
INSERT INTO Answers VALUES('10120','x'),('10160','y'),('9','z');
SELECT '-'||answerid||'-' FROM answers ORDER BY answerid;
SELECT * FROM answers WHERE answerid IN (10120,10160);
SELECT * FROM answers WHERE CAST(answerid AS integer) IN (10120,10160);

-- case: research/sqlite-forum/nocase_rowvalue_in_swapped_columns | source: https://sqlite.org/forum/forumpost/eab63506cf
CREATE TABLE t (c1 TEXT COLLATE NOCASE, c2 INTEGER, PRIMARY KEY (c1, c2));
INSERT INTO t (c1, c2) VALUES ('a', 1);
SELECT (1, 'a') IN (SELECT c2, c1 FROM t);
SELECT ('a', 1) IN (SELECT c1, c2 FROM t);

-- case: research/sqlite-forum/like_escape_first_char_nocase_int_index | source: https://sqlite.org/src/info/c94369cae9b561b1
CREATE TABLE t1(x INT COLLATE NOCASE UNIQUE);
INSERT INTO t1 VALUES('123');
SELECT x FROM t1 WHERE x LIKE '^1%' ESCAPE '^';

-- case: research/sqlite-forum/replace_empty_pattern_dual_type | source: https://sqlite.org/forum/forumpost/3776b48e71
CREATE TABLE t0(c0 VARCHAR, PRIMARY KEY(c0));
INSERT INTO t0 (c0) VALUES (true);
SELECT * FROM t0;
SELECT * FROM t0 WHERE (t0.c0)=(REPLACE(true, '', false));
SELECT * FROM t0 WHERE NOT ((t0.c0)=(REPLACE(true, '', false)));
SELECT typeof(REPLACE(1, '', 2)), REPLACE(1, '', 2);

-- case: research/sqlite-forum/replace_pattern_starting_with_nul | source: https://sqlite.org/forum/forumpost/ab2225012b14e81f6ca0c425c181d1fb424189da6eb35b7a307ec990f608996f
SELECT hex(replace(x,'0'||x'00',z))=hex('_1'), hex(replace(x,x'00',z))=hex('0_1'), hex(replace(x,x'00'||'1',z))=hex('0_') FROM (SELECT '0'||x'00'||'1' AS x,'_' AS z) t;

-- case: research/sqlite-forum/round_zero_two_args_sign | source: https://sqlite.org/forum/forumpost/fc46f950a0
SELECT round(0.0), round(0.0, 1), round(-0.0), round(-0.0, 1), round(-0.4), round(-0.04, 1);

-- case: research/sqlite-forum/format_zero_precision_f_g | source: https://sqlite.org/forum/info/393708f4a8
SELECT format('%0.0f', 0.9), format('%0.0g', 0.09), format('%0.0g', 1.9), format('%.0f', 0.5), format('%.0f', 1.5), format('%.0f', 2.5);

-- case: research/sqlite-forum/quote_real_thirteen | source: https://sqlite.org/forum/forumpost/2d2ddc5b70c31de5
SELECT quote(13.0), quote(0.1), quote(1e100), quote(-0.0), quote(1.0e-5), quote(3.0), quote(100.0);

-- case: research/sqlite-forum/cast_real_sum_truncation | source: https://sqlite.org/forum/info/6f53df2d99e94512
SELECT CAST((0.1+0.7)*10 AS INTEGER), (0.1+0.7)*10, CAST(round((0.1+0.7)*10) AS INTEGER);

-- case: research/sqlite-forum/insert_returning_real_small_integer | source: https://sqlite.org/forum/forumpost/dd9d7017c785e887
CREATE TABLE t(x REAL, y NUMERIC, z);
INSERT INTO t(x, y, z) VALUES (1, 2.0, 3.0) RETURNING x, typeof(x), y, typeof(y), z, typeof(z);

-- case: research/sqlite-forum/big_integer_literal_to_real | source: https://sqlite.org/forum/forumpost/569a7209179a7f5e
SELECT 18446744073709551488, 18446744073709551615, 9223372036854775808, -9223372036854775808, -9223372036854775809;

-- case: research/sqlite-forum/cast_oversize_real_to_integer | source: https://sqlite.org/src/info/6f53fc7106
SELECT CAST(1e19 AS INTEGER), CAST(-1e19 AS INTEGER), CAST(9223372036854775807.0 AS INTEGER), CAST('9223372036854775808' AS INTEGER);

-- case: research/sqlite-forum/rowid_max_plus_one_compare | source: https://sqlite.org/forum/forumpost/2bdb86a068ba9956
CREATE TABLE v0 ( v1 INTEGER PRIMARY KEY );
INSERT INTO v0 VALUES ( 9223372036854775807 );
SELECT * FROM v0 WHERE v1 >= ( 9223372036854775807 + 1 );
SELECT v1 >= ( 9223372036854775807 + 1 ) FROM v0;

-- case: research/sqlite-forum/json_arrow_numeric_string_key | source: https://sqlite.org/forum/forumpost/9e52cdfe15c3926e
SELECT '{"1":"one"}' -> '1', '{"1":"one"}' ->> '1', json_extract('{"1":"one"}', '$.1'), json_extract('{"1":"one"}', '$."1"');

-- case: research/sqlite-forum/json_extract_escaped_quote_key | source: https://sqlite.org/forum/forumpost/1f5263a98f
SELECT json_extract('{ "a": 1, "\"b\"": 2 }', '$."\"b\""');
SELECT json_extract('{ "a": 1, ""b"": 2 }', '$.""b""');

-- case: research/sqlite-forum/json5_single_quote_string_canonical | source: https://sqlite.org/forum/forumpost/ddcad3e884
SELECT json('''Valid "JSON5" string'''), json_valid(json('''Valid "JSON5" string'''));

-- case: research/sqlite-forum/json_extract_embedded_nul | source: https://sqlite.org/forum/info/e0e7c7c1f1a9f4bb
SELECT json_extract('{"a":"x\u0000y"}', '$.a') AS v, length(json_extract('{"a":"x\u0000y"}', '$.a')), typeof(json_extract('{"a":"x\u0000y"}', '$.a'));

-- case: research/sqlite-forum/json_set_append_twice_arrow | source: https://sqlite.org/forum/forumpost/fc0e3f1e2a
SELECT json, json -> 0, json -> 1 FROM (SELECT json_set(json_set('[]','$[#]',0),'$[#]',1) AS json);

-- case: research/sqlite-forum/json_array_length_after_remove | source: https://sqlite.org/forum/forumpost/0560d5e482
SELECT json_array_length(json_remove('[1,2,3,4]','$[1]'));

-- case: research/sqlite-forum/json_group_array_order_by_subtype | source: https://sqlite.org/forum/forumpost/87347ad2fb5a8f76
CREATE TABLE tab (a TEXT);
INSERT INTO tab (a) VALUES ('ape'), ('cat'), ('boy');
SELECT json_group_array(json_object('a', a)) FROM tab UNION ALL SELECT json_group_array(json_object('a', a) ORDER BY a) FROM tab;

-- case: research/sqlite-forum/json_group_object_null_key | source: https://sqlite.org/forum/forumpost/a108dc5fde245674
SELECT id, json_group_object(key, value) FROM (SELECT 1 AS id, NULL AS key, 2 AS value) GROUP BY id;
SELECT id, json_group_object(key, value) FROM (SELECT 1 AS id, 'k' AS key, 2 AS value UNION ALL SELECT 1, NULL, 3) GROUP BY id;

-- case: research/sqlite-forum/json_group_array_over_subquery_subtype | source: https://sqlite.org/forum/forumpost/e5bd251fb5
CREATE TABLE foo (a TEXT, b TEXT);
INSERT INTO foo (a,b) VALUES ('a1','b1'),('a2','b2');
SELECT json_group_array(obj) FROM (SELECT rowid AS id, json_object('a', a, 'b', b) AS obj FROM foo);
SELECT json_group_array(json(obj)) FROM (SELECT rowid AS id, json_object('a', a, 'b', b) AS obj FROM foo);

-- case: research/sqlite-forum/json_insert_view_subtype_dropped | source: https://sqlite.org/forum/forumpost/340b2fde7a30de50bf3189d6813289f80e6462132a84395804b84ce0dae4e984
CREATE VIEW demo_view AS SELECT json_object('a','b') AS json;
SELECT json_insert(json('[]'),'$[#]', (SELECT json FROM demo_view));
SELECT json_insert(json('[]'),'$[#]', (SELECT json(json) FROM demo_view));

-- case: research/sqlite-forum/json_array_subquery_extract_subtype | source: https://sqlite.org/forum/info/a37d3531d0d1665013007e61d
SELECT json_array(d) FROM (SELECT json_extract(value, '$') AS d FROM json_each(json_array('{"k1": "v1"}')));
SELECT json_array(json(d)) FROM (SELECT json_extract(value, '$') AS d FROM json_each(json_array('{"k1": "v1"}')));

-- case: research/sqlite-forum/json_functions_blob_input | source: https://sqlite.org/forum/forumpost/012136abd5
SELECT json_valid(CAST('[1,2,3]' AS BLOB)), json_valid(CAST('[1,2,3]' AS TEXT)), CAST(CAST('[1,2,3]' AS BLOB) AS TEXT) -> 1;

-- case: research/sqlite-forum/json_tree_value_subtype | source: https://sqlite.org/forum/forumpost/ecb94cd210
SELECT *, subtype(value) FROM json_tree('[1,2,3]');

-- case: research/sqlite-forum/json_each_value_object_column | source: https://sqlite.org/forum/forumpost/1ba5bfc29adc329a
SELECT value FROM json_each(json_array(json_object('arg', 1, 'value', 2, 'z', 4))) AS x_2;

-- case: research/sqlite-forum/json_quote_view_not_pushdown | source: https://sqlite.org/forum/forumpost/3d9caa45cb
CREATE TABLE t1 (c0);
CREATE VIEW v0(c0) AS SELECT json(TRUE);
INSERT INTO t1 VALUES ('x');
SELECT * FROM v0, t1;
SELECT NOT json_quote(v0.c0) FROM v0, t1;
SELECT * FROM v0, t1 WHERE NOT json_quote(v0.c0);

-- case: research/sqlite-forum/json_patch_case_view_group_by | source: https://sqlite.org/forum/forumpost/c96206d45c6122e1c81fc18d220fd3d5e183aac9f08294fb242671398090a44d
CREATE TABLE t0(c0);
INSERT INTO t0 VALUES ('1');
CREATE VIEW v0(c0) AS SELECT CASE WHEN 1 THEN json_patch('1', '1') END FROM t0 GROUP BY t0.c0;
SELECT * FROM v0 WHERE json_quote(v0.c0) != '1';

-- case: research/sqlite-forum/strftime_fraction_with_unixepoch | source: https://sqlite.org/forum/forumpost/2ffbaa2c3fd7fb82
SELECT strftime('%Y-%m-%d %H:%M:%f', 1.234, 'unixepoch'), strftime('%f', 1.234, 'unixepoch'), strftime('%s', 1.234, 'unixepoch'), datetime(1.999, 'unixepoch', 'subsec');

-- case: research/sqlite-forum/strftime_julian_day_pre_400 | source: https://sqlite.org/forum/forumpost/eaa0a09786c6368b
SELECT 'JD 1830692.0: ' || strftime('%F %T', 1830692.0);
SELECT strftime('%F %T', 0.0), strftime('%F %T', 1721059.5), strftime('%F %T', 1721425.5), strftime('%F %T', 5373484.5);

-- case: research/sqlite-forum/date_numeric_arg_is_julian_day | source: https://sqlite.org/forum/forumpost/ac750bbccca4f4c31e195dcf8875ad961e6a777cdbd6dd2d4a37cb0970f3e2a1
SELECT date(1748528160), date(1748528160, 'unixepoch'), date(1748528160, 'auto'), date(2460000.5, 'auto'), datetime(-1, 'unixepoch');

-- case: research/sqlite-forum/start_of_day_after_julian_number | source: https://www.sqlite.org/src/info/f0831cced2c919e4
SELECT datetime(2457754, 'start of day'), datetime(2457754.75, 'start of month'), datetime(2457754.75, 'start of year');

-- case: research/sqlite-forum/julianday_subsecond_roundtrip | source: https://sqlite.org/forum/info/f32ea1685c9461b444d76c257b2f9de9312f938bcaa23b20c2bb151b3f0dc263
SELECT julianday('2022-05-17 13:56:12.569Z'), strftime('%Y-%m-%d %H:%M:%f', julianday('2022-05-17 13:56:12.569Z'));

-- case: research/sqlite-forum/fts5_leftjoin_match_case_on_clause | source: https://sqlite.org/forum/forumpost/428ef7c468
CREATE VIRTUAL TABLE t0 USING fts5(c0, c1);
INSERT INTO t0(c0,c1) VALUES (1,0);
SELECT * FROM t0 LEFT JOIN (SELECT 0 AS col_0) ON ((((t0.c1 MATCH '1') AND (CASE WHEN t0.c0 THEN CAST(t0.c1 AS INTEGER) ELSE 1 END))));

-- case: research/sqlite-forum/fts5_trigram_option_without_value | source: https://sqlite.org/forum/forumpost/81670d1056
CREATE VIRTUAL TABLE t USING fts5(s, tokenize='trigram case_sensitive ');
CREATE VIRTUAL TABLE t2 USING fts5(s, tokenize='trigram remove_diacritics ');
SELECT count(*) FROM sqlite_master;

-- case: research/sqlite-forum/fts5_rank_config_repeated | source: https://sqlite.org/forum/info/a2dd636330
CREATE VIRTUAL TABLE t USING fts5 (a, b);
INSERT INTO t (a, b) VALUES ('data1', 'sentence1'), ('data2', 'sentence2');
INSERT INTO t(t, rank) VALUES ('rank', 'bm25(10.0,1.0)');
SELECT a, b, rank FROM t('data*') ORDER BY RANK;
INSERT INTO t(t, rank) VALUES ('rank', 'bm25(10.0,1.0)');
SELECT a, b, rank FROM t('data*') ORDER BY RANK;

-- case: research/sqlite-forum/fts5_external_content_delete_then_reinsert | source: https://sqlite.org/forum/forumpost/1a249403ce
CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, value TEXT);
INSERT INTO test (name, value) VALUES ('hello', 'world'), ('quick', 'the quick text'), ('brown fox', 'just a brown fox roaming');
CREATE VIRTUAL TABLE test_idx USING fts5(name, value, content=test, content_rowid=id);
INSERT INTO test_idx (test_idx, rowid, name, value) SELECT 'delete', id, name, value FROM test;
INSERT INTO test_idx (rowid, name, value) SELECT id, name, value FROM test;
SELECT name FROM test_idx WHERE test_idx MATCH 'quick' ORDER BY rank;

-- case: research/sqlite-forum/fts5_external_content_view_no_trigger_match | source: https://sqlite.org/forum/forumpost/52e2383750
CREATE TABLE item(id INTEGER PRIMARY KEY, title TEXT);
CREATE VIEW item_view AS SELECT id, title FROM item;
CREATE VIRTUAL TABLE item_search USING fts5(title, content='item_view', content_rowid='id');
INSERT INTO item VALUES (1, 'hello world');
SELECT rowid, title FROM item_search;
SELECT rowid, title FROM item_search WHERE item_search MATCH 'hello';

-- case: research/sqlite-forum/fts5_replace_into_no_recursive_triggers | source: https://sqlite.org/forum/forumpost/21127c1160
CREATE TABLE licenses(id INTEGER PRIMARY KEY, name TEXT);
CREATE VIRTUAL TABLE licenses_fts USING fts5(name, content='licenses', content_rowid='id');
CREATE TRIGGER l_ai AFTER INSERT ON licenses BEGIN INSERT INTO licenses_fts(rowid, name) VALUES (new.id, new.name); END;
CREATE TRIGGER l_ad AFTER DELETE ON licenses BEGIN INSERT INTO licenses_fts(licenses_fts, rowid, name) VALUES ('delete', old.id, old.name); END;
INSERT INTO licenses VALUES (1, 'mit license');
REPLACE INTO licenses VALUES (1, 'apache license');
SELECT rowid FROM licenses_fts WHERE licenses_fts MATCH 'mit';
SELECT rowid FROM licenses_fts WHERE licenses_fts MATCH 'apache';

-- case: research/sqlite-forum/fts5_returning_with_trigger_insert | source: https://sqlite.org/forum/forumpost/ab2bf0a12f
CREATE TABLE a (id INTEGER PRIMARY KEY, name TEXT);
CREATE VIRTUAL TABLE b USING fts5(name);
CREATE TRIGGER a_trigger AFTER INSERT ON a BEGIN INSERT INTO b (name) VALUES ('foo'); END;
INSERT INTO a VALUES (1, 'foo') RETURNING id;

-- case: research/sqlite-forum/affinity_text_column_vs_numeric_literal | source: https://www.sqlite.org/datatype3.html
CREATE TABLE t(a TEXT, b INTEGER, c, d NUMERIC);
INSERT INTO t VALUES(10, '10', 10, '10');
SELECT a = 10, a = '10', b = '10', c = '10', c = 10, d = '10.0', a < 9, b < '9', typeof(a), typeof(b), typeof(c), typeof(d) FROM t;

-- case: research/sqlite-forum/affinity_in_list_and_case_and_cast | source: https://www.sqlite.org/datatype3.html
CREATE TABLE t(a TEXT, b INTEGER);
INSERT INTO t VALUES('1', 1);
SELECT a IN (1), b IN ('1'), a IN ('1', 2), (SELECT a FROM t) IN (SELECT b FROM t), CASE a WHEN 1 THEN 'int' WHEN '1' THEN 'text' END, CASE b WHEN '1' THEN 'text' ELSE 'none' END, CAST(a AS INTEGER) = b, CAST(' 12abc' AS INTEGER), CAST('1e2' AS INTEGER), CAST('0x10' AS INTEGER), CAST('1e2' AS NUMERIC);

-- case: research/sqlite-forum/collation_nocase_rtrim_binary_compare | source: https://www.sqlite.org/datatype3.html
SELECT 'a' = 'A' COLLATE NOCASE, 'a ' = 'a' COLLATE RTRIM, 'a ' = 'a', 'a' < 'B', 'a' < 'B' COLLATE NOCASE, 'a  ' = 'a ' COLLATE RTRIM, 'É' = 'é' COLLATE NOCASE, x'61' = 'a', 'a' < x'61', 1 < 'a', 1.0 = 1, '1' = 1;

-- case: research/sqlite-forum/infinity_nan_negative_zero_values | source: https://www.sqlite.org/lang_expr.html
SELECT 1e999, -1e999, 1e999 - 1e999, typeof(1e999 - 1e999), -0.0, 0.0 = -0.0, 1/0, 1.0/0, 5 % 0, 9223372036854775807 + 1, -9223372036854775808 - 1, abs(-9223372036854775807), 9223372036854775807 * 2;

-- case: research/sqlite-forum/hex_literal_and_integer_edges | source: https://www.sqlite.org/lang_expr.html
SELECT 0x10, typeof(0x10), 0xFFFFFFFFFFFFFFFF, 0x7FFFFFFFFFFFFFFF, -0x8000000000000000, typeof(x'10'), x'10' = 0x10, '0x10' + 0, '1_000' + 0, '  5  ' + 0, ' 5x' + 0, '.5' + 0, '5.' + 0, '1e' + 0;

-- case: research/sqlite-forum/string_functions_unicode_edges | source: https://www.sqlite.org/lang_corefunc.html
SELECT substr('héllo', 2, 3), substr('hello', -3), substr('hello', 0, 3), substr('hello', 2, -1), substr(x'0102030405', 2, 2), instr('héllo', 'l'), instr(x'0102', x'02'), length('a'||x'00'||'b'), length(x'00ff'), unicode('€'), unicode(''), char(8364, 65), upper('é'), lower('É'), trim('  x  '), trim('xxhixx', 'x'), ltrim('abcba', 'ab'), rtrim('abcba', 'ab'), replace('aaa', 'aa', 'b'), replace('abc', '', 'x');

-- case: research/sqlite-forum/like_glob_escape_semantics | source: https://www.sqlite.org/lang_expr.html
SELECT 'ABC' LIKE 'abc', 'ABC' GLOB 'abc', 'a%b' LIKE 'a\%b' ESCAPE '\', 'axb' LIKE 'a\%b' ESCAPE '\', 'a_b' LIKE 'a!_b' ESCAPE '!', 'a[b' GLOB 'a[[]b', 'a-b' GLOB 'a[a-c]b', 'a]b' GLOB 'a[]]b', 'ab' GLOB 'a[^b]', 'é' LIKE 'É', 10 LIKE '1%', 1.5 LIKE '1.%', NULL LIKE 'a', 'a' LIKE NULL;

-- case: research/sqlite-forum/printf_format_edges | source: https://www.sqlite.org/printf.html
SELECT printf('%d', '12abc'), printf('%d', 3.99), printf('%d', -3.99), printf('%5.2f|%-6d|%06d|%+d', 3.14159, 42, 42, 42), printf('%s', NULL), printf('%q', 'it''s'), printf('%Q', 'it''s'), printf('%Q', NULL), printf('%x', -1), printf('%c', 65), printf('%.3s', 'abcdef'), printf('%g', 100000.0), printf('%g', 1000000.0), printf('%e', 12345.6789), printf('%!.15g', 0.1), printf('%,d', 1234567), printf('%%'), printf('%5s|%-5s|', 'ab', 'ab'), printf('%d', 9223372036854775807), printf('%d', 1e30);

-- case: research/sqlite-forum/quote_and_typeof_values | source: https://www.sqlite.org/lang_corefunc.html
SELECT quote('a''b'), quote(NULL), quote(x'ab'), quote(1), quote(1.5), quote(''), quote('a'||x'00'||'b'), typeof(1), typeof(1.0), typeof('1'), typeof(x''), typeof(NULL), typeof(1+1.0), typeof(CAST(1 AS REAL)), typeof(1/2), 1/2, 1.0/2, -7 / 2, -7 % 3, 7 % -3, 5.5 % 2;

-- case: research/sqlite-forum/nullif_coalesce_iif_distinct | source: https://www.sqlite.org/lang_corefunc.html
SELECT nullif(1, 1.0), nullif('a', 'A'), nullif(1, '1'), coalesce(NULL, 0, 1), ifnull(NULL, NULL), iif(0, 'y', 'n'), iif(NULL, 'y', 'n'), iif('0', 'y', 'n'), iif('abc', 'y', 'n'), iif(0.1, 'y', 'n'), 1 IS NULL, NULL IS NULL, NULL IS NOT NULL, 1 IS DISTINCT FROM NULL, NULL IS NOT DISTINCT FROM NULL, 1 IS 1.0, '1' IS 1, NULL = NULL, NULL IN (1, NULL), 1 IN (2, NULL), 1 NOT IN (2, NULL), 1 IN ();

-- case: research/sqlite-forum/round_edges | source: https://www.sqlite.org/lang_corefunc.html
SELECT round(2.5), round(-2.5), round(0.5), round(1.005, 2), round(2.675, 2), round(1e15 + 0.5), round(123.456, -1), round(123.456, 100), round('abc'), round(NULL), round(9223372036854775807), round(1e300, 5), typeof(round(5)), round(5.5, 0), round(-0.5);

-- case: research/sqlite-forum/json_each_tree_paths_and_types | source: https://www.sqlite.org/json1.html
SELECT key, value, type, atom, id, parent, fullkey, path FROM json_tree('{"a":[1,2.5,"x",null,true,{"b":false}],"c":{}}') ORDER BY id;
SELECT key, value, type, atom, fullkey, path FROM json_each('[10,"20",{"k":[1]}]', '$[2]');

-- case: research/sqlite-forum/json_set_insert_replace_patch_remove | source: https://www.sqlite.org/json1.html
SELECT json_set('{"a":1}', '$.b', 2, '$.a', 9), json_insert('{"a":1}', '$.a', 9, '$.c', 3), json_replace('{"a":1}', '$.a', 9, '$.z', 3), json_patch('{"a":1,"b":{"c":2}}', '{"a":null,"b":{"c":3,"d":4}}'), json_remove('[0,1,2,3]', '$[1]', '$[1]'), json_remove('{"a":1}', '$'), json_set('[1,2]', '$[5]', 9), json_set('[1,2]', '$[#]', 3), json_extract('[1,2,3]', '$[#-1]'), json_extract('{"a":[1,2]}', '$.a', '$.a[0]'), json_extract('{"a":null}', '$.a'), json_extract('{"a":1}', '$.b'), '{"a":1.0}' ->> '$.a', '{"a":"x"}' -> '$.a', '{"a":[1]}' ->> '$.a';

-- case: research/sqlite-forum/json_valid_json5_and_flags | source: https://www.sqlite.org/json1.html
SELECT json_valid('{a:1}'), json_valid('{a:1}', 2), json_valid('{"a":1}', 1), json_valid('[1,]'), json_valid('[1,]', 2), json_valid('0x10', 2), json_valid('+1', 2), json_valid('.5', 2), json_valid('Infinity', 2), json_valid('NaN', 2), json('{a:0x1F,b:+.5,c:Infinity,d:NaN,e:''x''}'), json_valid(NULL), json_valid(''), json_valid(' [1] '), json_valid('[1]x'), json_valid('"a\qb"');

-- case: research/sqlite-forum/jsonb_roundtrip_and_types | source: https://www.sqlite.org/json1.html
SELECT typeof(jsonb('{"a":[1,2]}')), json(jsonb('{"a":[1,2]}')), jsonb_extract('{"a":[1,2]}', '$.a'), typeof(jsonb_extract('{"a":[1,2]}', '$.a')), json(jsonb_set('{"a":1}', '$.b', json('[1]'))), json_extract(jsonb('{"a":"x"}'), '$.a'), json_array_length(jsonb('[1,2,3]')), jsonb_array(1, 'a', NULL) IS NOT NULL, json(jsonb_array(1, 'a', NULL)), json_type(jsonb('[1]'), '$[0]'), json_valid(jsonb('{}'), 8), json_valid(x'00', 8);

-- case: research/sqlite-forum/json_big_numbers_and_escapes | source: https://www.sqlite.org/json1.html
SELECT json_extract('[9223372036854775807, 9223372036854775808, 1e999, 1.0, -0, -0.0, 1E2, 12345678901234567890]', '$[0]', '$[1]', '$[2]', '$[3]', '$[4]', '$[5]', '$[6]', '$[7]'), json_quote('a"b\c' || char(10) || char(1) || '/'), json_quote(1.0), json_quote(NULL), json_quote(x'01'), json_array(1.0, 1e100, 9223372036854775807, 'x', NULL, 1 = 1), json_object('a', 1, 'a', 2), json('"😀"'), json_extract('"😀"', '$'), length(json_extract('"😀"', '$'));

-- case: research/sqlite-forum/fts5_match_phrase_near_prefix_bm25 | source: https://www.sqlite.org/fts5.html
CREATE VIRTUAL TABLE f USING fts5(title, body);
INSERT INTO f(rowid, title, body) VALUES (1, 'the quick brown fox', 'jumps over the lazy dog'), (2, 'quick quick quick', 'fox fox'), (3, 'slow turtle', 'the quick brown fox runs far away from here and over the hill'), (4, 'Cafe', 'café CAFE cafe');
SELECT rowid, bm25(f), bm25(f, 10.0, 1.0) FROM f WHERE f MATCH 'quick fox' ORDER BY rank, rowid;
SELECT rowid FROM f WHERE f MATCH 'NEAR(quick fox, 1)' ORDER BY rowid;
SELECT rowid FROM f WHERE f MATCH '"quick brown"' OR f MATCH 'qu* NOT brown' ORDER BY rowid;
SELECT rowid FROM f WHERE f MATCH 'title:quick - body:lazy' ORDER BY rowid;
SELECT rowid, highlight(f, 0, '[', ']'), snippet(f, 1, '<', '>', '...', 4) FROM f WHERE f MATCH 'quick OR dog' ORDER BY rowid;
SELECT rowid FROM f WHERE f MATCH 'cafe' ORDER BY rowid;

-- case: research/sqlite-forum/fts5_contentless_delete_rebuild | source: https://www.sqlite.org/fts5.html
CREATE VIRTUAL TABLE c USING fts5(a, content='', contentless_delete=1);
INSERT INTO c(rowid, a) VALUES (1, 'alpha beta'), (2, 'beta gamma'), (3, 'gamma delta');
DELETE FROM c WHERE rowid = 2;
SELECT rowid FROM c WHERE c MATCH 'beta' ORDER BY rowid;
SELECT rowid FROM c WHERE c MATCH 'gamma' ORDER BY rowid;
UPDATE c SET a = 'zeta beta' WHERE rowid = 1;
SELECT rowid FROM c WHERE c MATCH 'alpha' ORDER BY rowid;
SELECT rowid FROM c WHERE c MATCH 'zeta' ORDER BY rowid;
SELECT count(*) FROM c;

-- case: research/sqlite-forum/fts5_external_content_rebuild_and_delete_all | source: https://www.sqlite.org/fts5.html
CREATE TABLE d(id INTEGER PRIMARY KEY, t TEXT);
INSERT INTO d VALUES (1, 'one two'), (2, 'two three');
CREATE VIRTUAL TABLE e USING fts5(t, content='d', content_rowid='id');
SELECT rowid FROM e WHERE e MATCH 'two' ORDER BY rowid;
INSERT INTO e(e) VALUES ('rebuild');
SELECT rowid FROM e WHERE e MATCH 'two' ORDER BY rowid;
INSERT INTO e(e) VALUES ('delete-all');
SELECT rowid FROM e WHERE e MATCH 'two' ORDER BY rowid;
INSERT INTO e(e) VALUES ('rebuild');
INSERT INTO e(e, rank) VALUES ('integrity-check', 1);
SELECT rowid, t FROM e WHERE e MATCH 'three' ORDER BY rowid;

-- case: research/sqlite-forum/nocase_index_without_rowid | source: https://www.sqlite.org/src/tktview/3182d3879020ef3b2e6db56be2470a0266d3c773
CREATE TABLE test (c1 TEXT PRIMARY KEY) WITHOUT ROWID;
CREATE INDEX index_0 ON test(c1 COLLATE NOCASE);
INSERT INTO test(c1) VALUES ('A');
INSERT INTO test(c1) VALUES ('a');
SELECT * FROM test;

-- case: research/sqlite-forum/glob_unique_index_reindex | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-April/084324.html
CREATE TABLE test (c0, c1 REAL);
CREATE UNIQUE INDEX index_1 ON test(c0 GLOB c1);
INSERT INTO test(c0, c1) VALUES ('1', '1');
INSERT INTO test(c0, c1) VALUES ('0', '1');
REINDEX;
SELECT * FROM test;

-- case: research/sqlite-forum/nocase_index_real_like | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084334.html
CREATE TABLE test (c0 REAL);
CREATE INDEX index_0 ON test(c0 COLLATE NOCASE);
INSERT INTO test(c0) VALUES ('+/');
SELECT * FROM test WHERE (c0 LIKE '+/');

-- case: research/sqlite-forum/typeof_unique_index_real_reindex | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084350.html
CREATE TABLE test (c0 REAL);
CREATE UNIQUE INDEX index_0 ON test(TYPEOF(c0));
INSERT OR IGNORE INTO test(c0) VALUES (0.1);
INSERT OR IGNORE INTO test(c0) VALUES (FALSE);
REINDEX;
SELECT * FROM test;

-- case: research/sqlite-forum/length_neg_real_unique_index | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084366.html
CREATE TABLE test (c0 REAL);
CREATE UNIQUE INDEX index_0 ON test(LENGTH(-c0));
INSERT INTO test(c0) VALUES (0.0), ('10:');
REINDEX;
SELECT * FROM test;

-- case: research/sqlite-forum/partial_index_likely_not_null | source: https://www.sqlite.org/src/tktview?name=5c6955204c
CREATE TABLE t0(c0);
CREATE INDEX index_0 ON t0(c0) WHERE (~c0) NOT NULL;
INSERT INTO t0(c0) VALUES (NULL);
SELECT * FROM t0 WHERE (LIKELY(~c0) OR TRUE);

-- case: research/sqlite-forum/insert_or_fail_fk_unique | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084439.html
PRAGMA foreign_keys=true;
CREATE TABLE t0 (c0 UNIQUE, c1 UNIQUE, FOREIGN KEY(c0) REFERENCES t0(c1));
INSERT OR FAIL INTO t0(c0, c1) VALUES (0, 1), (0, 2);
SELECT * FROM t0;

-- case: research/sqlite-forum/rowid_less_than_text_join | source: https://www.sqlite.org/src/tktview?name=9cf6c9bb51
CREATE TABLE t0(c0);
CREATE TABLE t1(c0 INTEGER PRIMARY KEY);
PRAGMA reverse_unordered_selects=true;
INSERT INTO t1(c0) VALUES (0);
INSERT INTO t0(c0) VALUES ('a');
SELECT * FROM t1, t0 WHERE t1.c0 < t0.c0;

-- case: research/sqlite-forum/like_nocase_unique_dotslash | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084478.html
CREATE TABLE t0(c0 INT UNIQUE COLLATE NOCASE);
INSERT INTO t0(c0) VALUES ('./');
SELECT * FROM t0 WHERE t0.c0 LIKE './';

-- case: research/sqlite-forum/ipk_compare_text_or_reversed | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084480.html
CREATE TABLE t0(c0 INTEGER PRIMARY KEY);
INSERT INTO t0(c0) VALUES (1);
PRAGMA reverse_unordered_selects=true;
SELECT * FROM t0 WHERE ((t0.c0 > 'a') OR (t0.c0 <= 'a'));

-- case: research/sqlite-forum/real_primary_key_update_or_replace | source: https://www.sqlite.org/src/tktview/6c1d3febc00b22d457c7
CREATE TABLE t1 (c0, c1 REAL PRIMARY KEY);
INSERT INTO t1(c0, c1) VALUES (TRUE, 9223372036854775807), (TRUE, 0);
UPDATE t1 SET c0 = NULL;
UPDATE OR REPLACE t1 SET c1 = 1;
SELECT DISTINCT * FROM t1 WHERE (t1.c0 IS NULL);

-- case: research/sqlite-forum/round_infinity_compare | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084497.html
SELECT 1e500 >= 1, CAST(1e500 AS INT) >= CAST(1 AS INT), ROUND(1e500) >= ROUND(1);

-- case: research/sqlite-forum/partial_index_is_not_one | source: https://sqlite.org/src/tktview/80256748471a01
CREATE TABLE IF NOT EXISTS t0 (c0);
CREATE INDEX IF NOT EXISTS i0 ON t0(1) WHERE c0 NOT NULL;
INSERT INTO t0(c0) VALUES(NULL);
SELECT * FROM t0 WHERE t0.c0 IS NOT 1;

-- case: research/sqlite-forum/without_rowid_desc_pk_reindex_in_subquery | source: https://www.sqlite.org/src/tktview?name=bba7b69f98
CREATE TABLE t0 (c0 PRIMARY KEY DESC, c1 UNIQUE DEFAULT NULL) WITHOUT ROWID;
INSERT INTO t0(c0) VALUES (1), (2), (3), (4), (5);
REINDEX;
SELECT * FROM t0 WHERE t0.c0 IN (SELECT c0 FROM t0) AND t0.c1 ISNULL;

-- case: research/sqlite-forum/intersect_rowid_text_or_reversed | source: http://mailinglists.sqlite.org/cgi-bin/mailman/private/sqlite-users/2019-May/084539.html
PRAGMA reverse_unordered_selects=true;
CREATE TABLE t1 (c0, c1);
CREATE TABLE t2 (c0 INT UNIQUE);
INSERT INTO t1(c0, c1) VALUES (0, 0), (0, NULL);
INSERT INTO t2(c0) VALUES (1);
SELECT 1, NULL INTERSECT SELECT * FROM (SELECT t2.c0, t1.c1 FROM t1, t2 WHERE ((t2.rowid <= 'a')) OR (t1.c0 <= t2.c0) ORDER BY 'a' DESC LIMIT 100);

-- case: research/sqlite-forum/real_rounding_depends_on_from | source: https://www.sqlite.org/src/tktview?name=3c27b97e31
CREATE TABLE t0 (c0);
CREATE TABLE t1 (c1 REAL);
INSERT INTO t1(c1) VALUES (8366271098608253588);
INSERT INTO t0(c0) VALUES ('a');
SELECT * FROM t1 WHERE (t1.c1 = CAST(8366271098608253588 AS REAL));
SELECT * FROM t0, t1 WHERE (t1.c1 = CAST(8366271098608253588 AS REAL));

-- case: research/sqlite-forum/composite_pk_or_range_in_ordered | source: https://sqlite.org/src/info/787fa716be3a7f650c
CREATE TABLE t0 (c0, c1, PRIMARY KEY (c0, c1));
CREATE TABLE t1 (c0);
INSERT INTO t1 VALUES (2);
SELECT * FROM t0, t1 WHERE (t0.c1 >= 1 OR t0.c1 < 1) AND t0.c0 IN (1, t1.c0) ORDER BY 1;

-- case: research/sqlite-forum/index_on_missing_column_after_rename | source: https://www.sqlite.org/src/tktview/9b78184be266fd7084e9e8038ad631a21b37eb9e
CREATE TABLE t0(c1, c2);
INSERT INTO t0(c1, c2) VALUES ('a', 1);
CREATE INDEX i0 ON t0("C3");
ALTER TABLE t0 RENAME COLUMN c1 TO c3;
SELECT DISTINCT * FROM t0;

-- case: research/sqlite-forum/nested_boolean_in_is_zero | source: https://www.sqlite.org/src/tktview?name=d3e7f2ba5b
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES ('val');
SELECT * FROM t0 WHERE (((0 IS NOT FALSE) OR NOT (0 IS FALSE OR (t0.c0 IN (-1)))) IS 0);

-- case: research/sqlite-forum/failed_index_in_transaction_abs_minint | source: https://www.sqlite.org/src/tktview/b5ca442af9fadf5eff5b2bf64839516ab82cfc3d
CREATE TABLE IF NOT EXISTS t0(c0);
INSERT INTO t0(c0) VALUES (-9223372036854775808);
BEGIN TRANSACTION;
CREATE INDEX i0 ON t0(ABS(c0));
COMMIT;
CREATE INDEX i0 ON t0(1);
SELECT * FROM t0;

-- case: research/sqlite-forum/cast_dash_as_numeric | source: https://www.sqlite.org/src/tktview?name=4c2d7639f0
SELECT CAST('-' AS NUMERIC), typeof(CAST('-' AS NUMERIC));

-- case: research/sqlite-forum/empty_text_minus_large_int | source: https://www.sqlite.org/src/tktview?name=e8bedb2a18
SELECT '' - 2851427734582196970;

-- case: research/sqlite-forum/cast_text_real_to_numeric_integer | source: https://www.sqlite.org/src/tktview/dd6bffbfb6e61db9ecc9ea833d586427961ccc9d
CREATE TABLE t0 (c0 TEXT);
INSERT INTO t0(c0) VALUES ('1.0');
SELECT CAST(c0 AS NUMERIC), typeof(CAST(c0 AS NUMERIC)) FROM t0;

-- case: research/sqlite-forum/like_int_pk_nocase_space | source: https://www.sqlite.org/src/tktview?name=b1d8c79314
CREATE TABLE t0(c0 INT PRIMARY KEY COLLATE NOCASE);
INSERT INTO t0 VALUES (' 1-');
SELECT * FROM t0 WHERE t0.c0 LIKE ' 1-';

-- case: research/sqlite-forum/unary_minus_dot_text | source: https://www.sqlite.org/src/tktview?name=412bba9b22
SELECT -'.', typeof(-'.');

-- case: research/sqlite-forum/collate_expr_affinity_cast_int | source: https://www.sqlite.org/src/tktview/d60b3cd7cb0bdff8ff39f76c9faf16ba2efa442f
SELECT ((CAST(1 as INT)) COLLATE BINARY) == '1';

-- case: research/sqlite-forum/unary_minus_text_real | source: https://www.sqlite.org/src/tktview?name=1819598c09
SELECT -'1.0', typeof(-'1.0');

-- case: research/sqlite-forum/in_collate_cast_text | source: https://www.sqlite.org/src/tktview?name=57353f8243
SELECT (1 IN (CAST('1' as TEXT) COLLATE NOCASE));

-- case: research/sqlite-forum/cast_large_exponent_text_numeric | source: https://www.sqlite.org/src/tktview?name=afdc5a29dc
SELECT CAST('8.2250617031974513E18' AS NUMERIC), typeof(CAST('8.2250617031974513E18' AS NUMERIC));

-- case: research/sqlite-forum/likely_unlikely_affinity | source: https://www.sqlite.org/src/tktview?name=0c620df60b
SELECT LIKELY(CAST(1 AS INT)) = '1';
SELECT UNLIKELY(CAST(1 AS INT)) = '1';
SELECT LIKELIHOOD(CAST(1 AS INT), 0.5) = '1';

-- case: research/sqlite-forum/is_true_collate_real | source: https://www.sqlite.org/src/tktview?name=4d01eda811
SELECT 0.5 IS TRUE COLLATE NOCASE;
SELECT 0.5 IS TRUE COLLATE RTRIM;
SELECT 0.5 IS TRUE COLLATE BINARY;

-- case: research/sqlite-forum/cast_negative_zero_text_numeric | source: https://www.sqlite.org/src/tktview/674385aeba91c774d47736f1aefd259b074dc5d3
SELECT CAST('-0.0' AS NUMERIC), typeof(CAST('-0.0' AS NUMERIC));

-- case: research/sqlite-forum/cast_takes_column_collation | source: https://www.sqlite.org/src/tktview?name=b148fa6105
CREATE TABLE t0(c0 COLLATE NOCASE);
INSERT INTO t0(c0) VALUES ('a');
SELECT * FROM t0 WHERE CAST(t0.c0 AS TEXT) = 'A';

-- case: research/sqlite-forum/like_unique_nocase_percent | source: https://www.sqlite.org/src/tktview?name=ce8717f088
CREATE TABLE t0(c0 INT UNIQUE COLLATE NOCASE);
INSERT INTO t0(c0) VALUES ('.1%');
SELECT * FROM t0 WHERE t0.c0 LIKE '.1%';

-- case: research/sqlite-forum/rtrim_collation_without_rowid_pk | source: https://www.sqlite.org/src/tktview?name=f1580ba1b5
CREATE TABLE t0(c0 COLLATE RTRIM, c1 BLOB UNIQUE, PRIMARY KEY (c0, c1)) WITHOUT ROWID;
INSERT INTO t0 VALUES (123, 3), (' ', 1), ('	', 2), ('', 4);
SELECT * FROM t0 WHERE c1 = 1;

-- case: research/sqlite-forum/between_explicit_collate_ignored | source: https://www.sqlite.org/src/tktview?name=e1e07ef202
CREATE TABLE t0 (c3 TEXT);
INSERT INTO t0(c3) VALUES ('0');
SELECT * FROM t0 WHERE (t0.c3 COLLATE NOCASE) BETWEEN 1 AND '5';

-- case: research/sqlite-forum/update_or_replace_unique_expr_index_order_by | source: https://www.sqlite.org/src/tktview?name=ba2f4585cf
CREATE TABLE t0 (c0 REAL, c1);
CREATE UNIQUE INDEX i0 ON t0(c1, 0 | c0);
INSERT INTO t0(c0) VALUES (4750228396194493326), (0);
UPDATE OR REPLACE t0 SET c0 = 'a', c1 = '';
SELECT * FROM t0 ORDER BY t0.c1;

-- case: research/sqlite-forum/analyze_distinct_cross_join | source: https://www.sqlite.org/src/tktview?name=ccbe5759fb
CREATE TABLE t0 (c0, c1, c2, PRIMARY KEY (c0, c1));
CREATE TABLE t1 (c2);
INSERT INTO t0(c2) VALUES (0), (1), (3), (4), (5), (6), (7), (8), (9), (10), (11);
INSERT INTO t0(c1) VALUES ('a');
INSERT INTO t1(c2) VALUES (0);
ANALYZE;
SELECT DISTINCT t0.c0, t1._rowid_, t0.c1 FROM t1 CROSS JOIN t0 ON TRUE ORDER BY t0.c0;

-- case: research/sqlite-forum/analyze_distinct_update_pk | source: https://www.sqlite.org/src/tktview?name=ced41c7c7d
CREATE TABLE t1 (c1 , c2, c3, c4 , PRIMARY KEY (c4, c3));
INSERT INTO t1(c3) VALUES (0), (0), (0), (0), (0), (0), (0), (0), (0), (0), (NULL), (1), (0);
UPDATE t1 SET c2 = 0;
INSERT INTO t1(c1) VALUES (0), (0), (NULL), (0), (0);
ANALYZE t1;
UPDATE t1 SET c3 = 1;
SELECT DISTINCT * FROM t1 WHERE t1.c3 = 1;

-- case: research/sqlite-forum/min_on_unique_column_null | source: https://www.sqlite.org/src/tktview?name=41866dc373
CREATE TABLE t0(c0 UNIQUE, c1);
INSERT INTO t0(c0, c1) VALUES (NULL, 1);
SELECT MIN(t0.c0), t0.c1 FROM t0;

-- case: research/sqlite-forum/min_with_isnull_index_expr | source: https://www.sqlite.org/src/tktview?name=71e183cab6
CREATE TABLE t0 (c0, c1);
CREATE INDEX i0 ON t0(c1, c1 + 1 DESC);
INSERT INTO t0(c0) VALUES (1);
SELECT MIN(t0.c1), t0.c0 FROM t0 WHERE t0.c1 ISNULL;

-- case: research/sqlite-forum/view_sum_text_affinity_compare | source: https://www.sqlite.org/src/tktview?name=d52a29a9e6
CREATE TABLE t0(c0, c1 TEXT);
CREATE VIEW v0(c0) AS SELECT SUM(t0.c1) FROM t0;
INSERT INTO t0(c0, c1) VALUES ('a', 1);
SELECT * FROM v0, t0 WHERE t0.c1 <= v0.c0;

-- case: research/sqlite-forum/view_avg_compare_text | source: https://www.sqlite.org/src/tktview?name=61c853857f
CREATE TABLE t0(c0 TEXT, c1);
INSERT INTO t0(c0, c1) VALUES (-1, 0);
CREATE VIEW v0(c0, c1) AS SELECT t0.c0, AVG(t0.c1) FROM t0;
SELECT * FROM v0 WHERE v0.c1 < v0.c0;
SELECT v0.c1 < v0.c0 FROM v0;

-- case: research/sqlite-forum/view_column_in_affinity | source: https://www.sqlite.org/src/tktview?name=0a5e2c1dcb
CREATE TABLE t0(c0 TEXT);
CREATE VIEW v0(c0) AS SELECT t0.c0 FROM t0;
INSERT INTO t0(c0) VALUES ('0');
SELECT 0 IN (c0) FROM v0;

-- case: research/sqlite-forum/view_min_cast_rowid | source: https://www.sqlite.org/src/tktview?name=f8a7060ece
CREATE TABLE t0(c0 UNIQUE, c1);
INSERT INTO t0(c1) VALUES (0);
INSERT INTO t0(c0) VALUES (0);
CREATE VIEW v0(c0, c1) AS SELECT t0.c1, t0.c0 FROM t0 WHERE CAST(t0.rowid AS INT) = 1;
SELECT v0.c0, MIN(v0.c1) FROM v0;

-- case: research/sqlite-forum/partial_index_constant_null_not_null | source: https://www.sqlite.org/src/tktview?name=9080b6227f
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
CREATE INDEX i0 ON t0(NULL > c0) WHERE (NULL NOT NULL);
SELECT * FROM t0 WHERE ((NULL IS FALSE) IS FALSE);

-- case: research/sqlite-forum/window_function_in_exists_between | source: https://www.sqlite.org/src/tktview?name=256741a16b
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
SELECT * FROM t0 WHERE EXISTS (SELECT MIN(c0) OVER (), CUME_DIST() OVER () FROM t0) BETWEEN 1 AND 1;

-- case: research/sqlite-forum/left_join_view_typeof_not_eq | source: https://www.sqlite.org/src/tktview?name=6710d2f7a1
CREATE TABLE t0(c0);
CREATE VIEW v0(c0) AS SELECT TYPEOF(1) FROM t0;
INSERT INTO t0(c0) VALUES (0), (1);
SELECT * FROM t0 LEFT JOIN v0 ON t0.c0 WHERE NOT(v0.c0 = 'a');

-- case: research/sqlite-forum/view_lower_cast_1e500_not_in | source: https://www.sqlite.org/src/tktview?name=c7a1171907
CREATE TABLE t0(c0);
CREATE VIEW v0(c0) AS SELECT LOWER(CAST('1e500' AS TEXT)) FROM t0;
INSERT INTO t0(c0) VALUES (NULL);
SELECT v0.c0 FROM v0, t0 WHERE t0.rowid NOT IN (0, 0, v0.c0);

-- case: research/sqlite-forum/indexed_by_cast_numeric_group_rowid | source: https://www.sqlite.org/src/tktview?name=f043b1130b
CREATE TABLE t0 (c0, c1);
CREATE INDEX i0 ON t0(CAST(c0 AS NUMERIC));
INSERT INTO t0(c0, c1) VALUES ('a', -1);
SELECT * FROM t0 INDEXED BY i0 WHERE CAST(t0.c0 AS NUMERIC) > LOWER(t0.c1) GROUP BY t0.rowid;

-- case: research/sqlite-forum/distinct_null_is_pk_column_analyze | source: https://www.sqlite.org/src/tktview?name=b86894020e
CREATE TABLE t0 (c0, c1 NOT NULL DEFAULT 1, c2, PRIMARY KEY (c0, c1));
INSERT INTO t0(c2) VALUES (NULL), (NULL), (NULL), (NULL), (NULL), (NULL), (NULL), (NULL), (NULL), (NULL), (NULL);
INSERT INTO t0(c2) VALUES ('a');
ANALYZE t0;
SELECT DISTINCT * FROM t0 WHERE NULL IS t0.c0;

-- case: research/sqlite-forum/ipk_between_text_literal | source: https://www.sqlite.org/src/tktview?name=d9f584e936
CREATE TABLE t0(c0 INTEGER PRIMARY KEY, c1 TEXT);
INSERT INTO t0(c0, c1) VALUES (1, 'a');
SELECT * FROM t0 WHERE '-1' BETWEEN 0 AND t0.c0;

-- case: research/sqlite-forum/text_unique_compare_negated_blob | source: https://www.sqlite.org/src/tktview?name=ac184eb571
CREATE TABLE t0(c0 TEXT UNIQUE, c1);
INSERT INTO t0(c0) VALUES (-1);
SELECT * FROM t0 WHERE - x'ce' >= t0.c0;

-- case: research/sqlite-forum/likely_rowid_compare_text | source: https://www.sqlite.org/src/tktview?name=7e07a3dbf5
CREATE TABLE t0 (c0);
INSERT INTO t0(c0) VALUES ('a');
SELECT * FROM t0 WHERE LIKELY(t0.rowid) <= '0';

-- case: research/sqlite-forum/in_single_column_int_unique_text_lhs | source: https://www.sqlite.org/src/tktview?name=dbaf8a6820
CREATE TABLE t0(c0 INT UNIQUE);
INSERT INTO t0(c0) VALUES (1);
SELECT * FROM t0 WHERE '1' IN (t0.c0);

-- case: research/sqlite-forum/partial_index_is_false_is_false | source: https://www.sqlite.org/src/tktview?name=a6408d42b9
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (NULL);
CREATE INDEX i0 ON t0(1) WHERE c0 NOT NULL;
SELECT * FROM t0 WHERE (t0.c0 IS FALSE) IS FALSE;

-- case: research/sqlite-forum/partial_index_is_false_between | source: https://www.sqlite.org/src/tktview?name=fba33c8b1d
CREATE TABLE t0(c1);
CREATE INDEX i0 ON t0(1) WHERE c1 NOTNULL;
INSERT INTO t0(c1) VALUES (NULL);
SELECT * FROM t0 WHERE t0.c1 IS FALSE BETWEEN FALSE AND TRUE;

-- case: research/sqlite-forum/partial_index_between_in_false | source: https://www.sqlite.org/src/tktview?name=f8f472cbc7
CREATE TABLE t0 (c0);
CREATE INDEX i0 ON t0(1) WHERE c0 NOT NULL;
INSERT INTO t0(c0) VALUES (NULL);
SELECT * FROM t0 WHERE '' BETWEEN t0.c0 AND 1 IN (FALSE);

-- case: research/sqlite-forum/upsert_unique_expr_index_reindex | source: https://www.sqlite.org/src/tktview?name=5a3dba8104
CREATE TABLE t0(c0 REAL UNIQUE, c1);
CREATE UNIQUE INDEX i0 ON t0(0 || c1);
INSERT INTO t0(c0, c1) VALUES (1, 2), (2, 1);
INSERT INTO t0(c0) VALUES (1) ON CONFLICT(c0) DO UPDATE SET c1=excluded.c0;
REINDEX;
SELECT * FROM t0;

-- case: research/sqlite-forum/cast_unary_plus_real_blob_like | source: https://www.sqlite.org/src/tktview?name=57af00b664
CREATE TABLE t0(c0 REAL, c1 TEXT);
CREATE INDEX i0 ON t0(+c0, c0);
INSERT INTO t0(c0) VALUES(0);
SELECT CAST(+ t0.c0 AS BLOB) LIKE 0 FROM t0;

-- case: research/sqlite-forum/real_text_rounding_in_list | source: https://www.sqlite.org/src/tktview?name=2841e99d10
CREATE TABLE t0(c0 REAL UNIQUE);
INSERT INTO t0(c0) VALUES(2.07093491255203046E18);
SELECT * FROM t0 WHERE c0 IN ('2070934912552030444') UNION ALL SELECT c0 IN ('2070934912552030444') FROM t0;

-- case: research/sqlite-forum/is_null_on_pk_after_upsert_update | source: https://www.sqlite.org/src/tktview?name=29f635e0af
CREATE TABLE t0(c0 TEXT, c1 REAL, c2, PRIMARY KEY(c2, c0, c1));
CREATE INDEX i0 ON t0(c1 IN (c0));
INSERT INTO t0(c0, c2) VALUES (0, NULL) ON CONFLICT(c2, c1, c0) DO NOTHING;
UPDATE t0 SET c2 = x'';
SELECT * FROM t0 WHERE t0.c2 IS NULL;

-- case: research/sqlite-forum/nocase_partial_index_compare | source: https://www.sqlite.org/src/tktview?name=767a8cbc6d
CREATE TABLE t0(c0 COLLATE NOCASE, c1);
CREATE INDEX i0 ON t0(0) WHERE c0 >= c1;
REPLACE INTO t0 VALUES('a', 'B');
SELECT * FROM t0 WHERE t0.c1 <= t0.c0;

-- case: research/sqlite-forum/view_between_collate_constant | source: https://www.sqlite.org/src/tktview?name=a7debbe0ad
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES('');
CREATE VIEW v2(c0, c1) AS SELECT 'B' COLLATE NOCASE, 'a' FROM t0 ORDER BY t0.c0;
SELECT v2.c1 BETWEEN v2.c0 AND v2.c1 as count FROM v2;

-- case: research/sqlite-forum/view_distinct_collate_compare | source: https://www.sqlite.org/src/tktview?name=18458b1ad6
CREATE TABLE t0(c0 COLLATE NOCASE);
INSERT INTO t0(c0) VALUES ('B');
CREATE VIEW v0(c0, c1) AS SELECT DISTINCT t0.c0, 'a' FROM t0;
SELECT * FROM v0 WHERE v0.c1 >= v0.c0;

-- case: research/sqlite-forum/glob_unique_negative_int_star | source: https://www.sqlite.org/src/tktview?name=0f0428096f
CREATE TABLE t0(c0 UNIQUE);
INSERT INTO t0 VALUES (-1);
SELECT * FROM t0 WHERE t0.c0 GLOB '-*';

-- case: research/sqlite-forum/instr_blob_pk_where | source: https://www.sqlite.org/src/tktview?name=587791f926
CREATE TABLE t0(c0 PRIMARY KEY, c1);
INSERT INTO t0(c0) VALUES (x'bb'), (0);
SELECT COUNT(*) FROM t0 WHERE INSTR(x'aabb', t0.c0) ORDER BY t0.c0, t0.c1;
SELECT * FROM t0 WHERE INSTR(x'aabb', t0.c0) ORDER BY t0.c0, t0.c1;

-- case: research/sqlite-forum/view_rowid_order_abs_text_compare | source: https://www.sqlite.org/src/tktview?name=b2d4edaffd
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
CREATE VIEW v0(c0) AS SELECT t0.rowid FROM t0 ORDER BY 1;
SELECT COUNT(*) FROM v0 WHERE ABS('1') = v0.c0;

-- case: research/sqlite-forum/fts5_integrity_check_alter_in_txn | source: https://www.sqlite.org/src/tktview?name=8fe768e9c9
CREATE TABLE t0(c0);
CREATE VIRTUAL TABLE vt0 USING fts5(c0);
BEGIN TRANSACTION;
INSERT INTO vt0(c0) VALUES (NULL);
ALTER TABLE t0 ADD COLUMN c5 REAL;
INSERT INTO vt0(vt0) VALUES('integrity-check');
SELECT count(*) FROM vt0;

-- case: research/sqlite-forum/fts5_rebuild_twice_in_txn | source: https://www.sqlite.org/src/tktview?name=e258f008ce
CREATE VIRTUAL TABLE vt0 USING fts5(c0);
INSERT INTO vt0(c0) VALUES (NULL);
BEGIN TRANSACTION;
INSERT INTO vt0(vt0) VALUES('rebuild');
INSERT INTO vt0(vt0) VALUES('rebuild');
INSERT INTO vt0(vt0) VALUES('integrity-check');
SELECT count(*) FROM vt0;

-- case: research/sqlite-forum/fts5_ascii_tokenizer_prefix_integrity | source: https://www.sqlite.org/src/tktview?name=dd1f67bf25
CREATE VIRTUAL TABLE vt0 USING fts5(c0, tokenize = "ascii", prefix = 1);
INSERT INTO vt0(c0) VALUES (x'd1');
INSERT INTO vt0(vt0) VALUES('integrity-check');
SELECT count(*) FROM vt0;

-- case: research/sqlite-forum/left_join_view_not_is_false | source: https://www.sqlite.org/src/tktview?name=a976c487d1
CREATE TABLE t0(c1);
CREATE TABLE t1(c0);
CREATE VIEW v0 AS SELECT c1 FROM t1 LEFT JOIN t0;
INSERT INTO t1 VALUES (1);
SELECT * FROM v0 WHERE NOT(v0.c1 IS FALSE);

-- case: research/sqlite-forum/left_join_view_notnull_notnull | source: https://www.sqlite.org/src/tktview?name=c31034044b
CREATE TABLE t0(c0);
CREATE TABLE t1(c1);
INSERT INTO t0(c0) VALUES(0);
CREATE VIEW v0(c0) AS SELECT t1.c1 FROM t0 LEFT JOIN t1;
SELECT * FROM v0 WHERE v0.c0 NOTNULL NOTNULL;

-- case: research/sqlite-forum/recursive_trigger_replace_duplicate_unique | source: https://www.sqlite.org/src/tktview?name=a8a4847a2d
PRAGMA recursive_triggers = true;
CREATE TABLE t0(c0 UNIQUE);
CREATE TRIGGER tr0 AFTER DELETE ON t0 BEGIN INSERT INTO t0 VALUES(0); END;
INSERT OR REPLACE INTO t0(c0) VALUES(0), (0);
REINDEX;
SELECT * FROM t0;

-- case: research/sqlite-forum/row_value_nocase_plus_column | source: https://www.sqlite.org/src/tktview?name=b47e3627ec
CREATE TABLE t0(c0 COLLATE NOCASE, c1);
INSERT INTO t0 VALUES('a', 'A');
SELECT * FROM t0 WHERE (+ t0.c1, 1) >= (t0.c0, 1);

-- case: research/sqlite-forum/row_value_text_cast_real | source: https://www.sqlite.org/src/tktview?name=6ef984af89
CREATE TABLE t0(c0 TEXT PRIMARY KEY);
INSERT INTO t0(c0) VALUES ('');
SELECT * FROM t0 WHERE (t0.c0, TRUE) > (CAST('' AS REAL), FALSE);

-- case: research/sqlite-forum/row_value_nocase_explicit_collate | source: https://www.sqlite.org/src/tktview?name=135c9da751
CREATE TABLE t0(c0 UNIQUE);
INSERT INTO t0(c0) VALUES('a');
SELECT * FROM t0 WHERE (t0.c0, 0) < ('B' COLLATE NOCASE, 0);

-- case: research/sqlite-forum/replace_after_delete_trigger_recursive | source: https://www.sqlite.org/src/tktview?name=50c09fc2cf
PRAGMA recursive_triggers = true;
CREATE TABLE t0(c0, c1, c2 UNIQUE);
CREATE UNIQUE INDEX i0 ON t0(c1) WHERE c0;
CREATE TRIGGER tr0 AFTER DELETE ON t0 BEGIN DELETE FROM t0; END;
INSERT INTO t0(c2) VALUES(-1572226132);
INSERT INTO t0(c0) VALUES(1), (1);
REPLACE INTO t0(c0, c1, c2) VALUES(2, 0, 0xffffffffa249bbac);
SELECT * FROM t0;

-- case: research/sqlite-forum/trigger_replace_between_case | source: https://www.sqlite.org/src/tktview?name=c1e19e1204
PRAGMA temp.recursive_triggers = true;
CREATE TABLE t0(c0, c1 UNIQUE);
CREATE TRIGGER c DELETE ON t0 BEGIN INSERT INTO t0(c1) VALUES(1); END;
INSERT INTO t0(c1) VALUES(0);
REPLACE INTO t0(c1) VALUES (0);
SELECT t0.c1 BETWEEN 0 AND (CASE WHEN 1 THEN 1 ELSE t0.c0 END NOT NULL) FROM t0;

-- case: research/sqlite-forum/generated_unique_reindex_constant | source: https://www.sqlite.org/src/tktview?name=3ea1755124
CREATE TABLE t0(c0, c1 TEXT GENERATED ALWAYS AS (1) UNIQUE);
INSERT INTO t0(c0) VALUES (1);
REINDEX;
INSERT INTO t0(c0) VALUES (0);
REINDEX;
SELECT * FROM t0;

-- case: research/sqlite-forum/generated_ipk_unique_insert_null | source: https://www.sqlite.org/src/tktview?name=91e8695101
CREATE TABLE t0(c0 INTEGER PRIMARY KEY GENERATED ALWAYS AS(1), c1 UNIQUE GENERATED ALWAYS AS(1), c2 UNIQUE);
INSERT INTO t0 VALUES(NULL);
SELECT * FROM t0;

-- case: research/sqlite-forum/generated_column_vacuum_typeof | source: https://sqlite.org/src/tktview?name=1d2a8efc6c
CREATE TABLE t0(c0 AS(TYPEOF(c1)), c1);
INSERT INTO t0(c1) VALUES(0);
VACUUM;
SELECT * FROM t0;

-- case: research/sqlite-forum/glob_partial_index_replace_count | source: https://sqlite.org/src/tktview?name=a9efb42811
CREATE TABLE t0(c0);
CREATE INDEX i0 ON t0(0) WHERE c0 GLOB c0;
INSERT INTO t0 VALUES (0);
CREATE UNIQUE INDEX i1 ON t0(0);
CREATE UNIQUE INDEX i2 ON t0(0);
REPLACE INTO t0 VALUES(0);
SELECT COUNT(*) FROM t0 WHERE t0.c0 GLOB t0.c0;

-- case: research/sqlite-forum/left_join_view_partial_index_null_in | source: https://sqlite.org/src/tktview?name=623eff57e7
CREATE TABLE t0(c0);
CREATE TABLE t1(c0);
INSERT INTO t1(c0) VALUES (0);
CREATE INDEX i0 ON t0(0) WHERE NULL IN (c0);
CREATE VIEW v0(c0) AS SELECT t0.c0 FROM t1 LEFT JOIN t0;
SELECT COUNT(*) FROM v0 WHERE NULL IN (v0.c0);

-- case: research/sqlite-forum/integrity_check_check_abs_minint | source: https://sqlite.org/src/tktview?name=3c9eadd2a6
CREATE TABLE t0(c0 CHECK(ABS(-9223372036854775808)));
PRAGMA integrity_check;

-- case: research/sqlite-forum/left_join_view_row_value_ne | source: https://sqlite.org/src/tktview?name=02aa2bd02f
CREATE TABLE t0(c0);
CREATE TABLE t1(c0);
CREATE VIEW v0(c0) AS SELECT t0.c0 FROM t1 LEFT JOIN t0;
INSERT INTO t1(c0) VALUES (0);
SELECT * FROM v0 WHERE (v0.c0, x'') != (NULL, 0);

-- case: research/sqlite-forum/replace_generated_not_null | source: https://sqlite.org/src/tktview?name=2399f59861
CREATE TABLE t0(c0 NOT NULL AS(c1), c1);
REPLACE INTO t0(c1) VALUES(NULL);
SELECT * FROM t0;

-- case: research/sqlite-forum/generated_column_between_unique_or | source: https://sqlite.org/src/tktview?name=ce22a07731
CREATE TABLE t0 (c0 GENERATED ALWAYS AS (1), c1 UNIQUE, c2 UNIQUE);
INSERT INTO t0(c1) VALUES (1);
SELECT * FROM t0 WHERE 0 = t0.c2 OR t0.c1 BETWEEN t0.c2 AND 1;

-- case: research/sqlite-forum/update_two_generated_columns_check | source: https://www.sqlite.org/src/tktview?name=299b50ba81
CREATE TABLE t0(c0, c1 AS(c0 + c2), c2 AS(c1) CHECK(c2));
UPDATE t0 SET c0 = NULL;
SELECT * FROM t0;

-- case: research/sqlite-forum/vacuum_without_rowid_duplicate_pk_column | source: https://www.sqlite.org/src/tktview?name=302027baf1
CREATE TABLE t0(c0, c1 UNIQUE COLLATE NOCASE, PRIMARY KEY(c1, c1)) WITHOUT ROWID;
INSERT INTO t0(c1) VALUES(0);
VACUUM;
SELECT * FROM t0;

-- case: research/sqlite-forum/left_join_partial_isnull_index | source: https://www.sqlite.org/src/tktview?name=7f39060a24
CREATE TABLE t0(c0);
CREATE TABLE t1(c0);
CREATE INDEX i0 ON t0(1) WHERE c0 ISNULL;
INSERT INTO t0(c0) VALUES (1);
INSERT INTO t1(c0) VALUES (1);
SELECT * FROM t1 LEFT JOIN t0 WHERE t0.c0 ISNULL;

-- case: research/sqlite-forum/generated_column_empty_table_join | source: https://www.sqlite.org/src/tktview?name=b92e5e8ec2
CREATE TABLE t0(c0 AS (1), c1);
CREATE TABLE t1(c0);
SELECT * FROM t0, t1 WHERE t0.c0 == 0;

-- case: research/sqlite-forum/integrity_check_generated_not_null | source: https://www.sqlite.org/src/tktview?name=bd8c280671
CREATE TABLE t0 (c0, c1 NOT NULL GENERATED ALWAYS AS (c0 = 0));
INSERT INTO t0(c0) VALUES (0);
PRAGMA integrity_check;

-- case: research/sqlite-forum/generated_unique_unlikely_reindex | source: https://www.sqlite.org/src/tktview?name=d7c3f125c9
CREATE TABLE t0(c0 AS (0 = UNLIKELY(c1)) UNIQUE, c1 TEXT);
INSERT INTO t0(c1) VALUES (1), (0);
REINDEX;
SELECT * FROM t0;

-- case: research/sqlite-forum/left_join_generated_column_is_true | source: https://www.sqlite.org/src/tktview?name=3b84b42943
CREATE TABLE t0(c0);
CREATE TABLE t1(c0, c1 AS(1));
INSERT INTO t0 VALUES(0);
SELECT t1.c1 IS TRUE FROM t0 LEFT JOIN t1;

-- case: research/sqlite-forum/full_join_view_union_all_distinct | source: https://sqlite.org/forum/forumpost/174afeae5734d42d
CREATE TABLE t1(a INT);
CREATE TABLE t2(b INT, c INT);
CREATE VIEW t3(d) AS SELECT NULL FROM t2 FULL OUTER JOIN t1 ON c=a UNION ALL SELECT b FROM t2;
INSERT INTO t1(a) VALUES (NULL);
INSERT INTO t2(b, c) VALUES (99, NULL);
SELECT DISTINCT * FROM t2, t3 WHERE b<>0 UNION SELECT DISTINCT * FROM t2, t3 WHERE b ISNULL;

-- case: research/sqlite-forum/left_join_autoindex_missing_rows | source: https://sqlite.org/forum/forumpost/0d3200f4f3bcd3a3
CREATE TABLE t1(a INT, b INT);
CREATE TABLE t2(c INT, d INT);
CREATE TABLE t3(e TEXT, f TEXT);
INSERT INTO t1 VALUES(1, 1);
INSERT INTO t2 VALUES(1, 2);
SELECT * FROM t1 JOIN t2 ON (t2.c=t1.a) LEFT JOIN t3 ON (t2.d=1);

-- case: research/sqlite-forum/left_join_is_null_bloom_filter | source: https://sqlite.org/forum/forumpost/031e262a89b6a9d2
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INT);
CREATE TABLE t2(c INTEGER PRIMARY KEY, d INT);
WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM c WHERE x<100) INSERT INTO t1(a,b) SELECT x, 1000*x FROM c;
INSERT INTO t2(c,d) SELECT b*2, 1000000*a FROM t1;
SELECT count(*) FROM t1 LEFT JOIN t2 ON c=b WHERE c IS NULL;

-- case: research/sqlite-forum/upsert_old_values_in_update_trigger | source: https://sqlite.org/forum/forumpost/284955a3cd454a15
CREATE TABLE t1 (id INTEGER PRIMARY KEY, hash BLOB NOT NULL, data BLOB NOT NULL);
INSERT INTO t1 VALUES(1,unhex(format('31%.62c', '0')),format('%.36760c', 'a'));
INSERT INTO t1 VALUES(2,unhex(format('31%.62c', '0')),format('%.36760c', 'a'));
INSERT INTO t1 VALUES(3,unhex(format('32%.62c', '0')),format('%.36760c', 'b'));
INSERT INTO t1 VALUES(4,unhex(format('32%.62c', '0')),format('%.36760c', 'b'));
CREATE TABLE t2 (id INTEGER PRIMARY KEY, hash BLOB UNIQUE NOT NULL, data BLOB NOT NULL);
CREATE TABLE collisions (oldhash, newhash, old, new);
CREATE TRIGGER t2_collision BEFORE UPDATE OF data ON t2 WHEN old.hash = new.hash AND old.data != new.data BEGIN INSERT INTO collisions (oldhash, newhash, old, new) VALUES (old.hash, new.hash, old.data, new.data); END;
INSERT INTO t2 (hash, data) SELECT hash, data FROM t1 WHERE true ON CONFLICT (hash) DO UPDATE SET data = excluded.data;
SELECT count(*) FROM collisions;

-- case: research/sqlite-forum/replace_redundant_on_conflict_index_corrupt | source: https://sqlite.org/forum/forumpost/919c6579c8
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY ON CONFLICT REPLACE, c2 UNIQUE );
INSERT INTO v0 VALUES ( 0, 33 ), ( 11, 22 );
REPLACE INTO v0 VALUES ( 0, 11 ) ON CONFLICT ( c2 ) DO UPDATE SET c1 = c2, c2 = c2 ON CONFLICT ( c2 ) DO UPDATE SET c1 = c1, c2 = c1;
SELECT count(*) FROM v0;
SELECT count(*) FROM v0 WHERE c2 > 8;

-- case: research/sqlite-forum/not_null_reduction_is_null_in_aggregate_view | source: https://sqlite.org/forum/forumpost/440f2a2f17
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY, c2 TEXT);
CREATE VIEW v5 AS SELECT c1, COUNT ( * ) AS y, sum ( c2 ) OVER ( PARTITION BY c1) FROM v0;
SELECT c1 from v5;
SELECT c1 FROM v5 WHERE c1 IS NULL;

-- case: research/sqlite-forum/sum_infinity_with_where | source: https://sqlite.org/forum/forumpost/23b8688ef4
CREATE TABLE t0 (c0 DOUBLE);
INSERT INTO t0(c0) VALUES (1),(2),(3);
SELECT SUM(1.7976931348623157E308) as aggr FROM t0;
SELECT SUM(1.7976931348623157E308) as aggr FROM t0 where c0 > 1;

-- case: research/sqlite-forum/update_without_rowid_partial_unique_params | source: https://sqlite.org/forum/forumpost/e60e4c295d22f8ce
CREATE TABLE Repro (ID INTEGER NOT NULL PRIMARY KEY, Type INTEGER NOT NULL, Val INTEGER) WITHOUT ROWID;
CREATE UNIQUE INDEX Type1Unique ON Repro (Val) WHERE Type=1;
INSERT INTO Repro (ID, Type, Val) VALUES (8, 1, 2);
UPDATE Repro SET ID=0 WHERE Type=1 AND Val=2;
SELECT * FROM Repro;

-- case: research/sqlite-forum/concat_ws_empty_string | source: https://sqlite.org/forum/forumpost/52503ac21d
SELECT concat_ws('|', 'a', '', 'b', NULL, 'c');
SELECT concat_ws('|', '', 'a');

-- case: research/sqlite-forum/insert_error_midway_statement_journal | source: https://sqlite.org/forum/forumpost/9b9e4716c0d7bbd1
CREATE TABLE v0 ( c1 INT );
BEGIN;
INSERT INTO v0 ( c1 ) VALUES ( 0 ), ( nth_value ( 0, -1) OVER ( ) );
SELECT * FROM v0;

-- case: research/sqlite-forum/flattener_union_all_cast_join | source: https://sqlite.org/forum/forumpost/1821d30133c8e544
CREATE TABLE t0(c0 INT,c1 INT);
INSERT INTO t0 VALUES(10,10);
SELECT * FROM t0 JOIN (SELECT CAST(c0 AS TEXT) AS c2 FROM t0 UNION ALL SELECT c1 FROM t0) WHERE 10=c2;

-- case: research/sqlite-forum/left_join_view_cached_scalar_subquery | source: https://sqlite.org/forum/forumpost/402f05296d
CREATE TABLE t1(x TEXT, y INTEGER);
INSERT INTO t1(x,y) VALUES(NULL,-2),(NULL,1),('0',2);
CREATE TABLE t2(z INTEGER);
INSERT INTO t2(z) VALUES(2),(-2);
CREATE VIEW t3 AS SELECT z, (SELECT count(*) FROM t1) AS w FROM t2;
SELECT * FROM t1 LEFT JOIN t3 ON y=z;

-- case: research/sqlite-forum/analyze_left_join_in_empty_list | source: https://sqlite.org/forum/forumpost/eecefe6fcc
CREATE TEMPORARY TABLE IF NOT EXISTS t0 (c0 TEXT );
CREATE TABLE IF NOT EXISTS t1 (c0 INT );
CREATE TABLE IF NOT EXISTS t2 (c0 REAL );
INSERT OR REPLACE INTO t0(c0) VALUES ('DM');
CREATE INDEX i54 ON t0(((((c0)&(c0))) NOT BETWEEN ('?~') AND (x'')) DESC) WHERE ((NULL) BETWEEN (c0) AND (c0)) COLLATE RTRIM;
INSERT OR IGNORE INTO t2 VALUES (x'');
UPDATE t2 SET (c0, c0)=(NULL, -705652845);
ANALYZE t0;
ANALYZE;
INSERT OR ROLLBACK INTO t1 VALUES ('YK#4(');
INSERT OR ROLLBACK INTO t1 VALUES ('-940100587');
INSERT OR ROLLBACK INTO t1 VALUES ('嚚零%V-');
INSERT OR ROLLBACK INTO t1 VALUES ('-213986693');
INSERT OR ROLLBACK INTO t1 VALUES (NULL);
CREATE INDEX i20 ON t2((((((c0)OR(0.48846542080589095)))OR(c0)) IN ()));
ANALYZE t1;
SELECT ALL t1.c0 FROM t0 LEFT OUTER JOIN t1 LEFT OUTER JOIN t2 ON (t1.c0 NOTNULL) == (t2.c0 IN ()) WHERE t2.c0;

-- case: research/sqlite-forum/exists_scalar_subquery_total | source: https://sqlite.org/forum/forumpost/8692d94725
CREATE TABLE v0 ( v1 INTEGER);
INSERT INTO v0 VALUES ( 10 ),  ( 7 );
CREATE TABLE v4 ( v5 INTEGER PRIMARY KEY );
INSERT INTO v4 VALUES ( 10 );
SELECT cast ( (SELECT v5 FROM v4 WHERE EXISTS ( SELECT v5 WHERE v5 = v1 )) as BOOL) != 0 FROM v0 ;
SELECT TOTAL(cast ( (SELECT v5 FROM v4 WHERE EXISTS ( SELECT v5 WHERE v5 = v1 )) as BOOL) != 0) FROM v0 ;

-- case: research/sqlite-forum/group_concat_correlated_outer_aggregate | source: https://sqlite.org/forum/forumpost/9f2b929904
CREATE TABLE x AS SELECT 1 a;
CREATE TABLE y AS SELECT 1 b UNION ALL SELECT 1;
SELECT (SELECT group_concat(a) FROM y) unexpected,
       group_concat((SELECT a FROM y)) expected,
       (SELECT group_concat(b) FROM y) expected,
       (SELECT group_concat(a+b) FROM y) expected
  FROM x;
INSERT INTO x VALUES (1);
SELECT (SELECT group_concat(a) FROM y) unexpected FROM x;
SELECT (SELECT group_concat(b) FROM y) expected FROM x;

-- case: research/sqlite-forum/exists_join_limit_offset | source: https://sqlite.org/forum/info/2c43f36255a630e3
CREATE TABLE t2(id INT, data INT);
CREATE TABLE t3(amount INT);
INSERT INTO t2 VALUES (1,0),(2,0);
INSERT INTO t3 VALUES (1),(1);
SELECT COUNT(*) AS matched FROM t2 WHERE EXISTS (SELECT 1 FROM t3 WHERE t3.amount > t2.data);
SELECT COUNT(*) AS rows_out FROM (SELECT id FROM t2 WHERE EXISTS (SELECT 1 FROM t3 WHERE t3.amount > t2.data) LIMIT 2 OFFSET 3);

-- case: research/sqlite-forum/right_join_strength_reduction_view_count | source: https://sqlite.org/forum/forumpost/7f74ce0bee
CREATE TABLE rt0 (c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER, c4 INTEGER);
CREATE TABLE rt3 (c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER);
INSERT OR IGNORE INTO rt0(c3, c1) VALUES (x'', '1'), ('-1', -1e500);
CREATE VIEW v6(c0, c1, c2) AS SELECT 0, 0, 0;
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL));
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL)) WHERE (rt0.c1);
SELECT COUNT(*) FROM rt0 LEFT OUTER JOIN rt3 ON NULL RIGHT OUTER JOIN v6 ON ((CASE v6.c0 WHEN rt0.c4 THEN rt3.c3 END) NOT BETWEEN (rt0.c4) AND (NULL)) WHERE ((rt0.c1) IS TRUE);

-- case: research/sqlite-forum/right_join_on_false_distinct_view | source: https://sqlite.org/forum/forumpost/a7d4be7fb6
CREATE TABLE t1(a INT);  INSERT INTO t1(a) VALUES(1);
CREATE TABLE t2(b INT);  INSERT INTO t2(b) VALUES(2);
CREATE TABLE t3(c INT);  INSERT INTO t3(c) VALUES(3);
CREATE TABLE t4(d INT);  INSERT INTO t4(d) VALUES(4);
CREATE VIEW v5(e) AS SELECT DISTINCT d FROM t4;
SELECT * FROM  t1 JOIN t2 ON false RIGHT JOIN t3 ON true JOIN v5 ON true;

-- case: research/sqlite-forum/right_join_on_null_auto_index | source: https://sqlite.org/forum/forumpost/f3f546025a
CREATE TABLE t1(a INT);  INSERT INTO t1 VALUES(1);
CREATE TABLE t2(b INT);  INSERT INTO t2 VALUES(2);
CREATE TABLE t3(c INT);  INSERT INTO t3 VALUES(3);
CREATE TABLE t4(d INT);  INSERT INTO t4 VALUES(4);
CREATE TABLE t5(e INT);  INSERT INTO t5 VALUES(5);
SELECT * FROM t1 JOIN t2 ON null RIGHT JOIN t3 ON true
       LEFT JOIN (t4 JOIN t5 ON d+1=e) ON d=4
WHERE e>0;

-- case: research/sqlite-forum/left_join_union_all_is_null | source: https://sqlite.org/forum/forumpost/f8786ea8e1de76f5
CREATE TABLE onerow(x INT);
INSERT INTO onerow(x) VALUES(0);
SELECT * FROM (SELECT null AS aaa FROM onerow UNION ALL SELECT 'missing' AS aaa FROM onerow) AS a LEFT JOIN (SELECT 1 AS bbb) AS b ON a.aaa IS NULL;

-- case: research/sqlite-forum/left_join_subquery_is_null_on | source: https://sqlite.org/forum/forumpost/33c9cd5abf17e0fd
CREATE TABLE onerow(x INT);
INSERT INTO onerow(x) VALUES(0);
SELECT * FROM (SELECT 'missing' AS aaa FROM onerow) AS a LEFT JOIN (SELECT 1 AS bbb) AS b ON a.aaa IS NULL;

-- case: research/sqlite-forum/utf16be_glob_index | source: https://sqlite.org/forum/info/d7b90d92ffbfc61f
PRAGMA encoding = 'UTF-16be';
CREATE TABLE Example(word TEXT NOT NULL);
CREATE INDEX Example_word on Example(word);
INSERT INTO Example VALUES('み');
SELECT * FROM Example WHERE word GLOB 'み*';

-- case: research/sqlite-forum/order_by_desc_omit_noop_join | source: https://sqlite.org/forum/forumpost/8a1e467e905b8d27
CREATE TABLE t1(a1 INTEGER PRIMARY KEY, b1 INT);
CREATE TABLE t2(c2 INT, d2 INTEGER PRIMARY KEY);
CREATE TABLE t3(e3 INTEGER PRIMARY KEY);
INSERT INTO t1 VALUES(33,0);
INSERT INTO t2 VALUES(33,1),(33,2);
SELECT t1.a1, t2.d2 FROM (t1 LEFT JOIN t3 ON t3.e3=t1.b1) JOIN t2 ON t2.c2=t1.a1 WHERE t1.a1=33 ORDER BY t2.d2 DESC;

-- case: research/sqlite-forum/order_by_desc_omit_noop_join_original | source: https://sqlite.org/forum/forumpost/8a1e467e905b8d27
CREATE TABLE foo (a INTEGER NOT NULL, b INTEGER NOT NULL, ref INTEGER, PRIMARY KEY(a, b)) STRICT;
CREATE TABLE bar (a INTEGER NOT NULL, b INTEGER NOT NULL, c INTEGER NOT NULL, PRIMARY KEY(a, b, c)) STRICT;
INSERT INTO foo VALUES (42,123,NULL), (42,567,123);
INSERT INTO bar VALUES (42,567,0), (42,567,1);
SELECT foo.a, foo.b, bar.c FROM foo LEFT JOIN foo goo ON (goo.a = foo.a) AND (goo.b = foo.ref) INNER JOIN bar ON (bar.a = foo.a) AND (bar.b = foo.b) WHERE foo.a = 42 AND foo.b = 567 ORDER BY bar.c DESC;

-- case: research/sqlite-forum/nested_join_between_case | source: https://sqlite.org/forum/forumpost/befdab472d
create table t1 (c6 TEXT, primary key(c6));
create table t2 (vkey INTEGER, c12 TEXT);
insert into t1 values ('');
insert into t2 values (88, '');
select 1 from ((t2 as ref_0 left outer join t2 as ref_1 on true) right outer join t1 as ref_2 on (ref_1.c12 = ref_2.c6)) where case when true then (ref_1.vkey between null and ref_0.vkey) else (ref_1.vkey between null and ref_0.vkey) end;
select 1 from ((t2 as ref_0 left outer join t2 as ref_1 on true) right outer join t1 as ref_2 on (ref_1.c12 = ref_2.c6)) where ref_1.vkey between null and ref_0.vkey;

-- case: research/sqlite-forum/update_one_pass_between_subquery | source: https://sqlite.org/forum/info/0ab502f519068a50
CREATE TABLE t(v INT);
INSERT INTO t VALUES(1),(2);
BEGIN;
UPDATE t SET v=100 WHERE v=1 OR (1 BETWEEN 0 AND ((SELECT min(v) FROM t) >= v));
SELECT 'original', group_concat(v,'|') FROM (SELECT v FROM t ORDER BY v);
ROLLBACK;
UPDATE t SET v=100 WHERE v=1 OR ((1 >= 0) AND (1 <= ((SELECT min(v) FROM t) >= v)));
SELECT 'equivalent', group_concat(v,'|') FROM (SELECT v FROM t ORDER BY v);

-- case: research/sqlite-forum/parenthesization_collate_rtrim_join | source: https://sqlite.org/forum/forumpost/af3d07f908
CREATE TABLE t0 (c0);
CREATE TABLE t1(c0);
CREATE VIEW v0(c0) AS SELECT 0.4 FROM t0;
INSERT INTO t1(c0) VALUES (1);
INSERT INTO t0 VALUES (NULL);
SELECT v0.c0 FROM t1 INNER JOIN v0 ON ((v0.c0)AND(t0.c0)) COLLATE RTRIM LEFT OUTER JOIN t0 ON (('a') ISNULL) WHERE ((CASE (t1.c0) BETWEEN (t1.c0) AND (v0.c0) WHEN t1.c0 THEN (v0.c0) END) ISNULL);
SELECT v0.c0 FROM t1 INNER JOIN v0 ON ((v0.c0)AND(t0.c0)) COLLATE RTRIM LEFT OUTER JOIN t0 ON (('a') ISNULL);

-- case: research/sqlite-forum/desc_pk_without_rowid_range | source: https://sqlite.org/forum/forumpost/8988341615
CREATE TABLE v0 ( v1 INT PRIMARY KEY DESC, v2 INT ) WITHOUT ROWID;
INSERT INTO v0 VALUES ( 10, 10 );
CREATE INDEX v3 ON v0 ( v2 );
SELECT * FROM v0 WHERE v2 = 10 AND v1 = 10;
SELECT * FROM v0 WHERE v2 = 10 AND v1 < 11;

-- case: research/sqlite-forum/in_early_out_affinity_char | source: https://sqlite.org/forum/forumpost/9ec1361d32
CREATE TABLE v0 (v2 CHAR(30), v1 CHAR(30));
INSERT INTO v0 (v1) VALUES (10), (0);
CREATE INDEX v19 ON v0 (v2, v1);
INSERT INTO v0 VALUES (10, 10), (0, 10);
SELECT * FROM v0 WHERE (v1 IN (SELECT v2 FROM v0 ORDER BY v2));
SELECT * FROM v0 WHERE (v2 = 0);
SELECT * FROM v0 WHERE (v1 IN (SELECT v2 FROM v0 ORDER BY v2) AND v2 = 0);

-- case: research/sqlite-forum/in_early_out_affinity_minimal | source: https://sqlite.org/forum/forumpost/6a3ec138e9
CREATE TABLE t1(a TEXT, b TEXT);
INSERT INTO t1 VALUES(null,10),(0,10),(10,10);
CREATE INDEX t1ab ON t1(a,b);
SELECT * FROM t1 WHERE b in (SELECT a FROM t1) AND a=0;

-- case: research/sqlite-forum/row_value_in_subquery_update_unique_index | source: https://sqlite.org/forum/forumpost/b9647a113b465950
CREATE TABLE items (Id INTEGER PRIMARY KEY, Item INTEGER, Test TEXT, Filler, UNIQUE (Item, Id));
INSERT INTO items (Id, Item) VALUES (1, 2), (2, 3), (3, 3), (4, 4);
UPDATE items SET Test = 'ok' WHERE (Id, Item) IN (SELECT Id, Item FROM items);
SELECT Id, Item, Test FROM items;

-- case: research/sqlite-forum/limit_offset_distinct_scalar_subquery | source: https://sqlite.org/forum/forumpost/a5148faa497d5594
CREATE TABLE t1(x);
INSERT INTO t1 VALUES(1),(1),(1);
SELECT (SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 100) FROM t1;

-- case: research/sqlite-forum/window_min_filter_rows_frame | source: https://sqlite.org/forum/forumpost/e9126d554a
CREATE TABLE t0 (c0 INTEGER, c1 INTEGER);
INSERT INTO t0 (c0, c1) VALUES (10, 1), (20, -1), (5, 2), (15, 0), (25, 3);
SELECT c0, c1, MIN(c0) FILTER(WHERE c1 > 0) OVER (ORDER BY c0 ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS min_filtered_rows_frame FROM t0;

-- case: research/sqlite-forum/distinct_union_all_count_one | source: https://sqlite.org/forum/forumpost/aeae62275ebbf584
CREATE TABLE t1(id TEXT);
INSERT INTO t1 VALUES('a'),('b');
SELECT * FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);
SELECT count() FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);
SELECT count(1) FROM (SELECT DISTINCT * FROM t1 UNION ALL SELECT * FROM t1);

-- case: research/sqlite-forum/view_group_by_indexed_by_expr_index | source: https://sqlite.org/forum/forumpost/a68313d054
CREATE TABLE t0 (c0);
INSERT INTO t0(c0) VALUES (1);
CREATE INDEX i0 ON t0(c0 > 0);
CREATE VIEW v0(c0) AS SELECT AVG(t0.c0) FROM t0 GROUP BY 1>t0.c0;
SELECT COUNT(*) FROM t0 INDEXED BY i0 WHERE (SELECT COUNT(*) FROM v0 WHERE v0.c0 BETWEEN 0 AND 0);

-- case: research/sqlite-forum/partial_index_gt_null_is_not_null | source: https://sqlite.org/forum/forumpost/67b737942462fdd2
CREATE TABLE v0 ( v2 INT, v1 INT);
INSERT INTO v0 VALUES ( 10, 10 );
CREATE UNIQUE INDEX v4 ON v0 ( v1 ) WHERE v2 > NULL;
SELECT * FROM v0;
SELECT * FROM v0 WHERE v0.v2 IS NOT NULL;

-- case: research/sqlite-forum/cume_dist_view_is_chain | source: https://sqlite.org/forum/forumpost/de16c4abe2
CREATE TABLE t0 (c0);
CREATE VIEW v0(c0, c1) AS SELECT CUME_DIST() OVER (PARTITION BY t0.c0), TRUE FROM t0;
INSERT INTO t0 VALUES ('x');
SELECT * FROM v0, t0 WHERE ('1')IS(v0.c1) IS FALSE;
SELECT ('1')IS(v0.c1) IS FALSE FROM v0, t0;

-- case: research/sqlite-forum/json_arrow_numeric_key | source: https://sqlite.org/forum/forumpost/4b875d7b2a
SELECT '{"1":"one"}' -> '1';

-- case: research/sqlite-forum/and_or_duplicate_like | source: https://sqlite.org/forum/forumpost/7e9f0bdccad2fc2d
-- schema is not shown in the post; table and rows are a reconstruction
CREATE TABLE folder_id_email_list(value TEXT);
CREATE INDEX folder_id_email_list_value ON folder_id_email_list(value);
INSERT INTO folder_id_email_list VALUES('a@jackson.com'),('b@jackson.com'),('c@jackson.com'),('d@other.com'),(NULL);
SELECT * FROM folder_id_email_list AS el WHERE (el.value IS NOT NULL AND el.value LIKE '%jackson.com') OR (el.value IS NOT NULL AND el.value LIKE '%jackson.com');

-- case: research/sqlite-forum/distinct_group_by_order_by_subquery | source: https://sqlite.org/src/tktview/98825a79ce1456863
CREATE TABLE t1(x);
INSERT INTO t1 VALUES('right'),('wrong');
SELECT DISTINCT x FROM (SELECT x FROM t1 GROUP BY x) WHERE x='right' ORDER BY x;

-- case: research/sqlite-forum/partial_index_not_null_or_1 | source: https://sqlite.org/forum/forumpost/d813704d7c
-- reconstruction of a ticket whose repro text was not retrievable; partial index WHERE c0 NOT NULL with a query on c0 OR 1
CREATE TABLE t0(c0);
CREATE INDEX i0 ON t0(1) WHERE c0 NOT NULL;
INSERT INTO t0 VALUES(NULL);
SELECT c0 FROM t0 WHERE c0 OR 1;

-- case: research/sqlite-forum/not_null_reduction_window_view | source: https://sqlite.org/forum/forumpost/440f2a2f17
CREATE TABLE v0 ( c1 INTEGER PRIMARY KEY, c2 TEXT);
CREATE VIEW v5 AS SELECT c1, COUNT ( * ) AS y, sum ( c2 ) OVER ( PARTITION BY c1) FROM v0;
SELECT c1 from v5;
SELECT c1 FROM v5 WHERE c1 IS NULL;

-- case: research/sqlite-forum/generated_column_not_null_added | source: https://sqlite.org/forum/forumpost/c04814903d
CREATE TABLE v0 ( v1);
INSERT INTO v0 VALUES ( 255);
ALTER TABLE v0 ADD COLUMN v3 AS ( NULL) NOT NULL;
SELECT * FROM v0 WHERE v3 NOT IN ( SELECT count ( * ) FROM v0);

-- case: research/sqlite-forum/natural_left_join_right_join_notnull | source: https://sqlite.org/forum/forumpost/4fc70203b61c7e12
CREATE TABLE t1 (c0 INT, c1 INT);
CREATE TABLE t2 (c0 INT NOT NULL);
INSERT INTO t1(c1) VALUES (1);
SELECT * FROM t2 RIGHT JOIN (SELECT 1) as subQuery1 ON TRUE NATURAL LEFT JOIN t1;

-- case: research/sqlite-forum/natural_full_join_cross_view | source: https://sqlite.org/forum/forumpost/68f29a2005
CREATE TABLE t0(c0, c1, c2);
CREATE TABLE t1(c0);
CREATE TABLE t2 (c0 REAL, PRIMARY KEY (c0));
CREATE TEMP VIEW IF NOT EXISTS v0(c0) AS SELECT CAST(t1.c0 AS NUMERIC) AS col_0 FROM t1 NATURAL LEFT JOIN t2;
INSERT INTO t1(c0) VALUES (0.6);
INSERT INTO t2(c0) VALUES (NULL);
SELECT v0.c0, t1.c0 FROM t0 NATURAL CROSS JOIN v0 NATURAL FULL JOIN t1 INNER JOIN t2 ON (((((t2.c0)IS(v0.c0)))));

-- case: research/sqlite-forum/likely_affinity_in_empty_list | source: https://sqlite.org/forum/forumpost/45ec3d9788
CREATE TABLE vt0(c0 integer);
CREATE TABLE t1 (c0 INT);
CREATE VIEW v0(c0, c1, c2) AS SELECT t1.c0, t1.c0, CAST(((t1.c0) IS TRUE) AS TEXT) FROM t1, vt0 WHERE (x'') NOTNULL;
INSERT INTO vt0 VALUES ('x');
CREATE INDEX i46 ON t1(CAST(((c0) IS TRUE) AS TEXT));
INSERT INTO t1(c0) VALUES (NULL);
SELECT count(*) FROM t1, v0 WHERE (t1.c0 IN ())<(LIKELY(v0.c2));
SELECT count(*) FROM t1, v0 WHERE (t1.c0 IN ())<(LIKELY(v0.c2)) IS TRUE;

-- case: research/sqlite-forum/is_true_precedence | source: https://sqlite.org/forum/forumpost/22c493d86c51f613
SELECT 5 IS TRUE * 10;
SELECT (5 IS TRUE) * 10;
SELECT 5 IS (TRUE * 10);
SELECT '' IS FALSE;
SELECT '' IS 0;

-- case: research/sqlite-forum/count_pushdown_distinct_union_all | source: https://sqlite.org/forum/forumpost/a860f5fb2e
create table a(a int);
insert into a values (1), (1);
select count() from (select distinct * from a union all select * from a);

-- case: research/sqlite-forum/sum_filter_overflow | source: https://sqlite.org/forum/forumpost/23b8688ef4
CREATE TABLE t0 (c0 DOUBLE);
INSERT INTO t0(c0) VALUES (1),(2),(3);
SELECT SUM(1.7976931348623157E308) as aggr FROM t0;
SELECT SUM(1.7976931348623157E308) as aggr FROM t0 where c0 > 1;

-- case: research/sqlite-forum/view_join_or_false | source: https://sqlite.org/forum/forumpost/26387ea7ef
CREATE TABLE t0 ( wkey INTEGER );
INSERT INTO t0 VALUES(1);
CREATE TABLE t1 ( pkey INTEGER PRIMARY KEY, c1 INTEGER );
INSERT INTO t1 VALUES(2,3);
CREATE VIEW v0 as select 1;
select * from t0 as ref_0 where exists ( select 4 as c0 from (t1 as ref_1 left outer join ( select ref_0.wkey as c1 from v0 as ref_4 ) as subq_0 on (ref_1.c1 = subq_0.c1 )) where not ( (not (subq_0.c1 <> (ref_0.wkey + ref_1.pkey))) or false ));
select * from t0 as ref_0 where exists ( select 4 as c0 from (t1 as ref_1 left outer join ( select ref_0.wkey as c1 from v0 as ref_4 ) as subq_0 on (ref_1.c1 = subq_0.c1 )) where not ( (not (subq_0.c1 <> (ref_0.wkey + ref_1.pkey))) ));

-- case: research/sqlite-forum/expr_index_orderby_collate | source: https://sqlite.org/src/tktview/e20dd54ab0e4383
-- reconstruction from the ticket summary: index on substr(x,2) used for ORDER BY ... COLLATE nocase
CREATE TABLE t1(x);
INSERT INTO t1 VALUES('xDEF'),('xabcd'),('xABC');
CREATE INDEX t1x ON t1(substr(x,2));
SELECT substr(x,2) FROM t1 ORDER BY substr(x,2) COLLATE nocase;

-- case: research/sqlite-forum/like_blob_index | source: https://sqlite.org/forum/forumpost/56a527a1ba6dba71
-- reconstruction: table and index definition are not in the retrievable text
CREATE TABLE t1(x);
CREATE INDEX t1x ON t1(x);
INSERT INTO t1(x) VALUES(x'616263');
SELECT 'query-1', x FROM t1 WHERE x LIKE 'a%';
SELECT 'query-2', x FROM t1 WHERE +x LIKE 'a%';

-- case: research/sqlite-forum/left_join_view_constant_column_where | source: https://sqlite.org/src/tktview/7fde638e94287d2c948cd9389
CREATE TABLE t1(a);
INSERT INTO t1 VALUES(1),(2),(3);
CREATE VIEW v2 AS SELECT a, 1 AS b FROM t1;
CREATE TABLE t3(x);
INSERT INTO t3 VALUES(2),(4);
SELECT *, '|' FROM t3 LEFT JOIN v2 ON a=x WHERE b=1;
SELECT *, '|' FROM t3 LEFT JOIN v2 ON a=x WHERE b+1=x;
SELECT *, '|' FROM t3 LEFT JOIN v2 ON a=x ORDER BY b;

-- case: research/sqlite-forum/left_join_subquery_chain_null_row | source: https://sqlite.org/src/info/892fc34f173e99d8
CREATE TABLE t1(id INTEGER PRIMARY KEY);
CREATE TABLE t2(id INTEGER PRIMARY KEY, c2 INTEGER);
CREATE TABLE t3(id INTEGER PRIMARY KEY, c3 INTEGER);
INSERT INTO t1(id) VALUES(456);
INSERT INTO t3(id) VALUES(1),(2);
SELECT t1.id, x2.id, x3.id FROM t1 LEFT JOIN (SELECT * FROM t2) AS x2 ON t1.id=x2.c2 LEFT JOIN t3 AS x3 ON x2.id=x3.c3;

-- case: research/sqlite-forum/left_join_case_when_false_not_null_implication | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t1(a INT, b INT);
INSERT INTO t1 VALUES(1,2),(3,4);
CREATE TABLE t2(x INT, y INT);
SELECT *, 'x' FROM t1 LEFT JOIN t2 WHERE CASE WHEN FALSE THEN a=x ELSE 1 END;
SELECT *, 'x' FROM t1 LEFT JOIN t2 WHERE a IN (1,3,x,y);
SELECT *, 'x' FROM t1 LEFT JOIN t2 WHERE NOT ( 'x'='y' AND t2.y=1 );
SELECT *, 'x' FROM t1 LEFT JOIN t2 WHERE ~ ( 'x'='y' AND t2.y=1 );
SELECT *, 'x' FROM t1 LEFT JOIN t2 WHERE t2.y IS NOT 'abc';

-- case: research/sqlite-forum/left_join_is_true_nested_case_chain | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER);
INSERT INTO t1(a,b) VALUES(1,0),(11,1),(12,1),(13,1),(121,12);
CREATE INDEX t1b ON t1(b);
CREATE TABLE t2(x INTEGER PRIMARY KEY);
INSERT INTO t2(x) VALUES(0),(1);
SELECT a1, a2, a3, a4, a5
  FROM (SELECT a AS a1 FROM t1 WHERE b=0)
       JOIN (SELECT x AS x1 FROM t2)
       LEFT JOIN (SELECT a AS a2, b AS b2 FROM t1) ON x1 IS TRUE AND b2=a1
       JOIN (SELECT x AS x2 FROM t2) ON x2<=CASE WHEN x1 THEN CASE WHEN a2 THEN 1 ELSE -1 END ELSE 0 END
       LEFT JOIN (SELECT a AS a3, b AS b3 FROM t1) ON x2 IS TRUE AND b3=a2
       JOIN (SELECT x AS x3 FROM t2) ON x3<=CASE WHEN x2 THEN CASE WHEN a3 THEN 1 ELSE -1 END ELSE 0 END
       LEFT JOIN (SELECT a AS a4, b AS b4 FROM t1) ON x3 IS TRUE AND b4=a3
       JOIN (SELECT x AS x4 FROM t2) ON x4<=CASE WHEN x3 THEN CASE WHEN a4 THEN 1 ELSE -1 END ELSE 0 END
       LEFT JOIN (SELECT a AS a5, b AS b5 FROM t1) ON x4 IS TRUE AND b5=a4
 ORDER BY a1, a2, a3, a4, a5;

-- case: research/sqlite-forum/left_join_strength_reduction_is_not_null_eq_zero | source: https://sqlite.org/src/tktview/5948e09b8c415bc45da5c
CREATE TABLE t1(a INT);
INSERT INTO t1(a) VALUES(1);
CREATE TABLE t2(b INT);
SELECT a, b FROM t1 LEFT JOIN t2 ON 0 WHERE (b IS NOT NULL)=0;

-- case: research/sqlite-forum/left_join_subquery_constant_not_factored_out | source: https://sqlite.org/src/tktview/6710d2f7a13a299728ab
CREATE TABLE t1(x);
INSERT INTO t1(x) VALUES(0),(1);
SELECT * FROM t1 LEFT JOIN (SELECT abs(1) AS y FROM t1) ON x WHERE NOT(y='a');
SELECT * FROM t1 LEFT JOIN (SELECT abs(1)+2 AS y FROM t1) ON x WHERE NOT(y='a');

-- case: research/sqlite-forum/view_left_join_not_is_false | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t0(a);
CREATE TABLE t1(b);
CREATE VIEW v0 AS SELECT a FROM t1 LEFT JOIN t0;
INSERT INTO t1 VALUES (1);
SELECT * FROM v0 WHERE NOT(v0.a IS FALSE);
SELECT * FROM t1 LEFT JOIN t0 WHERE NOT(a IS FALSE);
SELECT NOT(v0.a IS FALSE) FROM v0;

-- case: research/sqlite-forum/view_left_join_notnull_notnull | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1(a) VALUES(0);
CREATE VIEW v0(c) AS SELECT t2.b FROM t1 LEFT JOIN t2;
SELECT * FROM v0 WHERE v0.c NOTNULL NOTNULL;
SELECT * FROM t1 LEFT JOIN t2 WHERE (b IS NOT NULL) IS NOT NULL;
SELECT (b IS NOT NULL) IS NOT NULL FROM t1 LEFT JOIN t2;
SELECT * FROM t1 LEFT JOIN t2 WHERE (b IS NOT NULL AND b IS NOT NULL) IS NOT NULL;

-- case: research/sqlite-forum/left_join_partial_index_null_in_column | source: https://sqlite.org/src/info/623eff57e76d45f6
CREATE TABLE t1(c1);
CREATE TABLE t0(c0);
INSERT INTO t0(c0) VALUES (0);
SELECT * FROM t0 LEFT JOIN t1 WHERE NULL IN (c1);
CREATE INDEX t1x ON t1(0) WHERE NULL IN (c1);
SELECT * FROM t0 LEFT JOIN t1 WHERE NULL IN (c1);

-- case: research/sqlite-forum/right_join_partial_index_on_boolean | source: https://sqlite.org/forum/forumpost/7dee41d32506c4ae
CREATE TABLE t1(x INT);      INSERT INTO t1(x) VALUES(1);
CREATE TABLE t2(y BOOLEAN);  INSERT INTO t2(y) VALUES(false);
CREATE TABLE t3(z INT);      INSERT INTO t3(z) VALUES(3);
CREATE INDEX t2y ON t2(y) WHERE y;
SELECT quote(z) FROM t1 RIGHT JOIN t2 ON y LEFT JOIN t3 ON y;

-- case: research/sqlite-forum/left_join_partial_index_isnull_must_not_qualify | source: https://sqlite.org/src/info/7f39060a24b47353
CREATE TABLE t0(aa);
CREATE TABLE t1(bb);
INSERT INTO t0(aa) VALUES (1);
INSERT INTO t1(bb) VALUES (1);
SELECT 11, * FROM t1 LEFT JOIN t0 WHERE aa ISNULL;
SELECT 12, * FROM t1 LEFT JOIN t0 WHERE +aa ISNULL;
SELECT 13, * FROM t1 LEFT JOIN t0 ON aa ISNULL;
SELECT 14, * FROM t1 LEFT JOIN t0 ON +aa ISNULL;
CREATE INDEX i0 ON t0(aa) WHERE aa ISNULL;
SELECT 21, * FROM t1 LEFT JOIN t0 WHERE aa ISNULL;
SELECT 22, * FROM t1 LEFT JOIN t0 WHERE +aa ISNULL;
SELECT 23, * FROM t1 LEFT JOIN t0 ON aa ISNULL;
SELECT 24, * FROM t1 LEFT JOIN t0 ON +aa ISNULL;

-- case: research/sqlite-forum/distinct_left_join_subquery_on_indexed_table | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t0(a, b);
CREATE INDEX t0a ON t0(a);
INSERT INTO t0 VALUES(10,10),(10,11),(10,12);
SELECT DISTINCT c FROM t0 LEFT JOIN (SELECT a+1 AS c FROM t0) ORDER BY c;

-- case: research/sqlite-forum/left_join_view_flattener_and_expression_in_on | source: https://sqlite.org/src/info/66e4b0e271c47145
CREATE TABLE t0(c0 INT);
CREATE VIEW v0 AS SELECT (NULL AND 5) as c0 FROM t0;
INSERT INTO t0(c0) VALUES (NULL);
SELECT count(*) FROM v0 LEFT JOIN t0 ON v0.c0;

-- case: research/sqlite-forum/flatten_left_join_in_subquery_with_nulls | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/join.test
CREATE TABLE t1(a INT,b INT,c INT);  INSERT INTO t1 VALUES(NULL,NULL,NULL);
CREATE TABLE t2(d INT,e INT);        INSERT INTO t2 VALUES(NULL,NULL);
CREATE INDEX x2 ON t1(c,b);
CREATE TABLE t3(x INT);              INSERT INTO t3 VALUES(NULL);
WITH t99(b) AS MATERIALIZED (SELECT b FROM t2 LEFT JOIN t1 ON c IN (SELECT x FROM t3)) SELECT 5 FROM t2 JOIN t99 ON b IN (1,2,3);
WITH t99(b) AS NOT MATERIALIZED (SELECT b FROM t2 LEFT JOIN t1 ON c IN (SELECT x FROM t3)) SELECT 5 FROM t2 JOIN t99 ON b IN (1,2,3);
SELECT 5 FROM t2 JOIN (SELECT b FROM t2 LEFT JOIN t1 ON c IN (SELECT x FROM t3)) AS t99 ON b IN (1,2,3);
INSERT INTO t1 VALUES(3,4,NULL);
INSERT INTO t2 VALUES(1,2);
WITH t99(b) AS (SELECT coalesce(b,3) FROM t2 AS x LEFT JOIN t1 ON c IN (SELECT x FROM t3)) SELECT d, e, b FROM t2 JOIN t99 ON b IN (1,2,3) ORDER BY +d, e;

-- case: research/sqlite-forum/full_outer_join_with_false_inner_join_on_view | source: https://sqlite.org/forum/forumpost/96cd4a7e9e
CREATE TABLE t0(a INT); INSERT INTO t0(a) VALUES (1);
CREATE TABLE t1(b INT); INSERT INTO t1(b) VALUES (2);
CREATE VIEW v2(c) AS SELECT 3 FROM t1;
SELECT * FROM t1 JOIN v2 ON 0     FULL OUTER JOIN t0 ON true;
SELECT * FROM t1 JOIN v2 ON 1=0   FULL OUTER JOIN t0 ON true;
SELECT * FROM t1 JOIN v2 ON false FULL OUTER JOIN t0 ON true;

-- case: research/sqlite-forum/right_join_omit_noop_join_optimization | source: https://sqlite.org/forum/forumpost/49f2c7f690
CREATE TABLE t0(z INT);         INSERT INTO t0 VALUES(1),(2);
CREATE TABLE t1(a INT);         INSERT INTO t1 VALUES(1);
CREATE TABLE t2(b INT);         INSERT INTO t2 VALUES(2);
CREATE TABLE t3(c INT, d INT);  INSERT INTO t3 VALUES(3,4);
CREATE TABLE t4(e INT);         INSERT INTO t4 VALUES(5);
CREATE VIEW v5(x,y) AS SELECT c, d FROM t3 LEFT JOIN t4 ON false;
SELECT DISTINCT a, b FROM t1 RIGHT JOIN t2 ON a=b LEFT JOIN v5 ON false WHERE x <= y;
SELECT DISTINCT a, b FROM t0 JOIN t1 ON z=a RIGHT JOIN t2 ON a=b LEFT JOIN v5 ON false WHERE x <= y;
SELECT count(*) FROM t0 JOIN t1 ON z=a RIGHT JOIN t2 ON a=b LEFT JOIN v5 ON false;

-- case: research/sqlite-forum/right_join_then_left_join_using_null_column | source: https://sqlite.org/forum/forumpost/4fc70203b61c7e12
CREATE TABLE t1(c0 INT , c1 INT); INSERT INTO t1(c0, c1) VALUES(NULL,11);
CREATE TABLE t2(c0 INT NOT NULL);
CREATE TABLE t2n(c0 INT);
CREATE TABLE t3(x INT);           INSERT INTO t3(x) VALUES(3);
CREATE TABLE t4(y INT);           INSERT INTO t4(y) VALUES(4);
CREATE TABLE t5(c0 INT, x INT);   INSERT INTO t5 VALUES(NULL, 5);
SELECT quote(c0), quote(x), quote(c1) FROM t2 RIGHT JOIN t3 ON true LEFT JOIN t1 USING(c0);
SELECT quote(c0), quote(x), quote(c1) FROM t2 RIGHT JOIN t3 ON true NATURAL LEFT JOIN t1;
SELECT quote(c0), quote(x), quote(c1) FROM t2n RIGHT JOIN t3 ON true LEFT JOIN t1 USING(c0);
SELECT quote(c0), quote(x), quote(c1) FROM t5 LEFT JOIN t1 USING(c0);
SELECT quote(x), quote(c0), quote(c1) FROM t3 LEFT JOIN t2 ON true LEFT JOIN t1 USING(c0);
SELECT quote(x), quote(c0), quote(y), quote(c1) FROM t3 LEFT JOIN t2 ON true JOIN t4 ON true NATURAL LEFT JOIN t1;

-- case: research/sqlite-forum/right_join_transitive_constraint_is_operator | source: https://sqlite.org/forum/forumpost/68f29a2005
CREATE TABLE t0(w INT);
CREATE TABLE t1(x INT);
CREATE TABLE t2(y INT UNIQUE);
CREATE VIEW v0(z) AS SELECT CAST(x AS INT) FROM t1 LEFT JOIN t2 ON true;
INSERT INTO t1(x) VALUES(123);
INSERT INTO t2(y) VALUES(NULL);
SELECT quote(w), quote(z), quote(x), quote(y) FROM t0 JOIN v0 ON w=z RIGHT JOIN t1 ON true INNER JOIN t2 ON y IS z;
SELECT quote(w), quote(z), quote(x), quote(y) FROM t0 JOIN v0 ON w=z RIGHT JOIN t1 ON true INNER JOIN t2 ON +y IS z;

-- case: research/sqlite-forum/chained_left_join_view_omit_noop | source: https://sqlite.org/forum/forumpost/11a53f2bad
CREATE TABLE t1(a1 INTEGER PRIMARY KEY, b1);
CREATE TABLE t2(a2 INTEGER PRIMARY KEY, b2);
CREATE TABLE t3(a3 INTEGER PRIMARY KEY, b3);
CREATE TABLE t4(a4 INTEGER PRIMARY KEY, b4);
INSERT INTO t1 VALUES(1,11),(2,12),(3,13),(5,15);
INSERT INTO t2 VALUES(1,21),(3,23),(4,24),(5,25);
INSERT INTO t3 VALUES(2,32),(3,33),(5,35);
INSERT INTO t4 VALUES(1,41),(2,42),(4,44),(5,45);
CREATE VIEW vchain AS SELECT a1, b1, b2, b3, b4 FROM t1 LEFT JOIN t2 ON a1=a2 LEFT JOIN t3 ON a2=a3 LEFT JOIN t4 ON a3=a4;
SELECT a1 FROM vchain ORDER BY a1;
SELECT a1, quote(b4) FROM vchain ORDER BY a1;

-- case: research/sqlite-forum/update_add_column_then_index_on_new_column | source: https://sqlite.org/src/tktview/43107840f1c02
CREATE TABLE t15(a INTEGER PRIMARY KEY, b);
INSERT INTO t15(a,b) VALUES(10,'abc'),(20,'def'),(30,'ghi');
ALTER TABLE t15 ADD COLUMN c;
CREATE INDEX t15c ON t15(c);
INSERT INTO t15(a,b) VALUES(5,'zyx'),(15,'wvu'),(25,'tsr'),(35,'qpo');
UPDATE t15 SET c=printf('y%d',a) WHERE c IS NULL;
SELECT a,b,c,'|' FROM t15 ORDER BY a;

-- case: research/sqlite-forum/update_pk_on_conflict_replace_noop | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/update.test
CREATE TABLE t16(a INTEGER PRIMARY KEY ON CONFLICT REPLACE, b UNIQUE);
INSERT INTO t16(a,b) VALUES(1,2),(3,4),(5,6);
UPDATE t16 SET a=a;
SELECT * FROM t16 ORDER BY +a;

-- case: research/sqlite-forum/update_with_partial_index_on_constant | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/update.test
CREATE TABLE t1(x,y);
INSERT INTO t1(x) VALUES(1);
CREATE INDEX t1x1 ON t1(1) WHERE 3;
UPDATE t1 SET x=2, y=3 WHERE 3;
SELECT * FROM t1;

-- case: research/sqlite-forum/update_text_column_from_quote_of_pk | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/update.test
CREATE TABLE t1(a TEXT, b INTEGER PRIMARY KEY UNIQUE);
INSERT INTO t1 VALUES(1,2);
UPDATE t1 SET a = quote(b) WHERE b>=2;
SELECT * FROM t1;

-- case: research/sqlite-forum/update_where_subquery_behind_short_circuit | source: https://sqlite.org/forum/forumpost/0007d1fdb1
CREATE TABLE t1 (vkey INTEGER, c5 INTEGER);
INSERT INTO t1 VALUES(3,NULL),(6,-54);
UPDATE t1 SET vkey = 100 WHERE c5 is null OR NOT (-10*(select min(vkey) from t1) >= c5);
SELECT quote(vkey), quote(c5) FROM t1 ORDER BY vkey, c5;

-- case: research/sqlite-forum/update_where_subquery_not_expression | source: https://sqlite.org/forum/forumpost/0007d1fdb1
CREATE TABLE t1 (vkey INTEGER, c5 INTEGER);
INSERT INTO t1 VALUES(3,NULL),(6,-54);
UPDATE t1 SET vkey = 100 WHERE NOT (-10*(select min(vkey) from t1) >= c5);
SELECT quote(vkey), quote(c5) FROM t1 ORDER BY vkey, c5;

-- case: research/sqlite-forum/update_where_subquery_min_of_updated_column | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/update.test
CREATE TABLE t1(x INT, y INT);
INSERT INTO t1(x) VALUES(1),(2),(3),(4),(5);
UPDATE t1 SET x=x+100, y=x<=(SELECT min(x) FROM t1) WHERE x<3 OR (1 BETWEEN 0 AND x<=(SELECT min(x)+2 FROM t1));
SELECT x FROM t1 WHERE x<100 ORDER BY x;

-- case: research/sqlite-forum/upsert_without_rowid_insert_or_ignore_index | source: https://sqlite.org/src/tktview/79cad5e4b2e219dd197242e9e5f4
CREATE TABLE t1(b UNIQUE, a INT PRIMARY KEY) WITHOUT ROWID;
INSERT OR IGNORE INTO t1(a) VALUES('1') ON CONFLICT(a) DO NOTHING;
DELETE FROM t1;
INSERT OR IGNORE INTO t1(a) VALUES('1'),(1) ON CONFLICT(a) DO NOTHING;
SELECT quote(a), quote(b), (SELECT count(*) FROM t1 INDEXED BY sqlite_autoindex_t1_1) FROM t1;

-- case: research/sqlite-forum/upsert_target_constraint_checked_first | source: https://sqlite.org/src/info/908f001483982c43
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INT, c INT, d INT, e INT);
CREATE UNIQUE INDEX t1b ON t1(b);
CREATE UNIQUE INDEX t1e ON t1(e);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,3,4,5);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,33,44,5) ON CONFLICT(e) DO UPDATE SET c=excluded.c;
SELECT * FROM t1;
DELETE FROM t1;
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,3,4,5);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,33,44,5) ON CONFLICT(b) DO UPDATE SET c=excluded.c;
SELECT * FROM t1;

-- case: research/sqlite-forum/upsert_target_constraint_without_rowid | source: https://sqlite.org/src/info/908f001483982c43
CREATE TABLE t1(a INT PRIMARY KEY, b INT, c INT, d INT, e INT) WITHOUT ROWID;
CREATE UNIQUE INDEX t1a ON t1(a);
CREATE UNIQUE INDEX t1b ON t1(b);
CREATE UNIQUE INDEX t1e ON t1(e);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,3,4,5);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,33,44,5) ON CONFLICT(e) DO UPDATE SET c=excluded.c;
SELECT * FROM t1;
DELETE FROM t1;
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,3,4,5);
INSERT INTO t1(a,b,c,d,e) VALUES(1,2,33,44,5) ON CONFLICT(b) DO UPDATE SET c=excluded.c;
SELECT * FROM t1;

-- case: research/sqlite-forum/upsert_expression_index_real_unique | source: https://sqlite.org/src/info/5a3dba8104421320
CREATE TABLE t0(c0 REAL UNIQUE, c1);
CREATE UNIQUE INDEX test800i0 ON t0(0 || c1);
INSERT INTO t0(c0, c1) VALUES (1, 2), (2, 1);
INSERT INTO t0(c0) VALUES (1) ON CONFLICT(c0) DO UPDATE SET c1=excluded.c0;
SELECT quote(c0), quote(c1) FROM t0 ORDER BY c0;

-- case: research/sqlite-forum/upsert_pk_not_null_without_rowid_insert_or_fail | source: https://sqlite.org/src/info/7c13db5c3bf74001
CREATE TABLE t0(c0 PRIMARY KEY, c1, c2 UNIQUE) WITHOUT ROWID;
INSERT OR FAIL INTO t0(c2) VALUES (0), (NULL) ON CONFLICT(c2) DO UPDATE SET c1 = c0;
SELECT count(*) FROM t0;

-- case: research/sqlite-forum/upsert_do_nothing_with_pk_replace_and_unique | source: https://sqlite.org/forum/forumpost/06b16b8b29f8c8c3
CREATE TABLE t1(a INTEGER PRIMARY KEY ON CONFLICT REPLACE, b UNIQUE);
INSERT INTO t1(b) VALUES(22);
INSERT INTO t1 VALUES(2,22) ON CONFLICT (b) DO NOTHING;
SELECT * FROM t1;

-- case: research/sqlite-forum/upsert_trigger_old_equals_new_large_text | source: https://sqlite.org/forum/forumpost/284955a3cd454a15
CREATE TABLE t1(x INT, y TEXT);
INSERT INTO t1 VALUES (11, replace(hex(zeroblob(4500)),'0','a')), (11, replace(hex(zeroblob(4500)),'0','a')), (33, replace(hex(zeroblob(4500)),'0','b')), (33, replace(hex(zeroblob(4500)),'0','b'));
CREATE TABLE t2(x INT UNIQUE, y TEXT);
CREATE TRIGGER r1 BEFORE UPDATE ON t2 BEGIN
  SELECT raise(ABORT,'Incorrect old.y value passed to trigger!') WHERE old.y != new.y;
END;
INSERT INTO t2(x, y) SELECT x, y FROM t1 WHERE true ON CONFLICT (x) DO UPDATE SET y = excluded.y;
SELECT x, length(y) FROM t2 ORDER BY x;

-- case: research/sqlite-forum/insert_more_select_columns_than_target_with_rowid | source: https://sqlite.org/src/info/e9654505cfda9361
CREATE TABLE t12a(a,b,c,d,e,f,g);
INSERT INTO t12a VALUES(101,102,103,104,105,106,107);
CREATE TABLE t12b(x);
INSERT INTO t12b(x,rowid,x,x,x,x,x) SELECT * FROM t12a;
SELECT rowid, x FROM t12b;
CREATE TABLE tab1( value INTEGER);
INSERT INTO tab1 (value, _rowid_) values( 11, 1);
INSERT INTO tab1 (value, _rowid_) SELECT 22,999;
SELECT * FROM tab1;

-- case: research/sqlite-forum/insert_replace_with_expression_index | source: https://sqlite.org/src/info/c2432ef9089ee73b
CREATE TABLE t13(a INTEGER PRIMARY KEY,b UNIQUE);
CREATE INDEX t13x1 ON t13(-b=b);
INSERT INTO t13 VALUES(1,5),(6,2);
REPLACE INTO t13 SELECT b,0 FROM t13;
SELECT * FROM t13 ORDER BY +b;

-- case: research/sqlite-forum/insert_null_into_integer_primary_key_via_case | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/insert.test
CREATE TABLE t14(x INTEGER PRIMARY KEY);
INSERT INTO t14 VALUES(CASE WHEN 1 THEN null END);
SELECT x FROM t14;

-- case: research/sqlite-forum/left_join_or_in_where_with_inner_join | source: https://sqlite.org/src/info/b7c8682cc17f32903f03a610bd0d35ffd3c1e6e4
CREATE TABLE t81(a INTEGER PRIMARY KEY, b, c, d);
CREATE TABLE t82(x INTEGER PRIMARY KEY, y);
CREATE TABLE t83(p INTEGER PRIMARY KEY, q);
INSERT INTO t81 VALUES(2,3,4,5);
INSERT INTO t81 VALUES(3,4,5,6);
INSERT INTO t82 VALUES(2,4);
INSERT INTO t83 VALUES(5,55);
SELECT * FROM t81 LEFT JOIN t82 ON y=b JOIN t83 WHERE c==p OR d==p ORDER BY +a;

-- case: research/sqlite-forum/left_join_or_in_on_clause | source: https://sqlite.org/src/info/f2369304e47167e3e644e2f1fe9736063391d7b7
CREATE TABLE t91(x); INSERT INTO t91 VALUES(1);
CREATE TABLE t92(y INTEGER PRIMARY KEY,a,b);
INSERT INTO t92 VALUES(1,2,3);
SELECT 1 FROM t91 LEFT JOIN t92 ON a=2 OR b=3;
SELECT 2 FROM t91 LEFT JOIN t92 ON a=2 AND b=3;
SELECT 3 FROM t91 LEFT JOIN t92 ON (a=2 OR b=3) AND y IS NULL;
SELECT 4 FROM t91 LEFT JOIN t92 ON (a=2 AND b=3) AND y IS NULL;
SELECT 6 FROM t91 LEFT JOIN t92 ON a=2 OR b=3 WHERE y IS NULL;
SELECT 7 FROM t91 LEFT JOIN t92 ON a=2 AND b=3 WHERE y IS NULL;
SELECT 8 FROM t91 LEFT JOIN t92 ON a=22 OR b=33 WHERE y IS NULL;
SELECT 9 FROM t91 LEFT JOIN t92 ON a=22 AND b=33 WHERE y IS NULL;

-- case: research/sqlite-forum/left_join_then_inner_join_with_or_on_right | source: https://sqlite.org/src/info/bc878246eafe0f52c519e29049b2fe4a99491b27
CREATE TABLE t101 (id INTEGER PRIMARY KEY);
INSERT INTO t101 VALUES (1);
SELECT * FROM t101 AS t0 LEFT JOIN t101 AS t1 ON t1.id BETWEEN 10 AND 20 JOIN t101 AS t2 ON (t2.id = t0.id OR (t2.id<>555 AND t2.id=t1.id));
CREATE TABLE t102 (id TEXT UNIQUE NOT NULL);
INSERT INTO t102 VALUES ('1');
SELECT * FROM t102 AS t0 LEFT JOIN t102 AS t1 ON t1.id GLOB 'abc%' JOIN t102 AS t2 ON (t2.id = t0.id OR (t2.id<>555 AND t2.id=t1.id));

-- case: research/sqlite-forum/union_with_where_zero_first_arm | source: https://sqlite.org/src/info/490a4b7235624298
CREATE TABLE t61(a);
CREATE TABLE t62(b);
INSERT INTO t61 VALUES(111);
INSERT INTO t62 VALUES(222);
SELECT a FROM t61 WHERE 0 UNION SELECT b FROM t62;
SELECT a FROM t61 WHERE 0 UNION ALL SELECT b FROM t62;
SELECT a FROM t61 UNION SELECT b FROM t62 WHERE 0;

-- case: research/sqlite-forum/left_join_order_by_right_column_distinct | source: https://sqlite.org/src/info/be84e357c035d068135f20bcfe82761bbf95006b
CREATE TABLE t181(a);
CREATE TABLE t182(b,c);
INSERT INTO t181 VALUES(1);
SELECT DISTINCT a FROM t181 LEFT JOIN t182 ON a=b ORDER BY c IS NULL;
SELECT DISTINCT a FROM t182 RIGHT JOIN t181 ON a=b ORDER BY c IS NULL;
SELECT DISTINCT a FROM t181 LEFT JOIN t182 ON a=b ORDER BY +c;
SELECT DISTINCT a FROM t181 LEFT JOIN t182 ON a=b ORDER BY c;

-- case: research/sqlite-forum/left_join_expression_index_ifnull | source: https://sqlite.org/src/info/4ba5abf65c5b0f9a
CREATE TABLE t201(x);
CREATE TABLE t202(y, z);
INSERT INTO t201 VALUES('key');
INSERT INTO t202 VALUES('key', -1);
CREATE INDEX t202i ON t202(y, ifnull(z, 0));
SELECT count(*) FROM t201 LEFT JOIN t202 ON (x=y) WHERE ifnull(z, 0) >=0;

-- case: research/sqlite-forum/left_join_on_is_not_null_with_index | source: https://sqlite.org/src/tktview/65eb38f6e46de8c75e188a
CREATE TABLE t1(a INT);
CREATE INDEX t1a ON t1(a);
INSERT INTO t1(a) VALUES(NULL),(NULL),(42),(NULL),(NULL);
CREATE TABLE t2(dummy INT);
SELECT count(*) FROM t1 LEFT JOIN t2 ON a IS NOT NULL;

-- case: research/sqlite-forum/where_transitive_pk_equals_constant | source: https://sqlite.org/src/info/fa792714ae62fa98
CREATE TABLE t1(a INTEGER PRIMARY KEY);
INSERT INTO t1(a) VALUES(1),(2),(3);
CREATE TABLE t2(x INTEGER PRIMARY KEY, y INT);
INSERT INTO t2(y) VALUES(2),(3);
SELECT * FROM t1, t2 WHERE a=y AND y=3;

-- case: research/sqlite-forum/where_between_text_constant_integer_affinity | source: https://sqlite.org/src/info/d9f584e936c7a8d0
CREATE TABLE t0(c0 INTEGER PRIMARY KEY, c1 TEXT);
INSERT INTO t0(c0, c1) VALUES (1, 'a');
CREATE TABLE t1(c0 INT PRIMARY KEY, c1 TEXT);
INSERT INTO t1(c0, c1) VALUES (1, 'a');
SELECT * FROM t0 WHERE '-1' BETWEEN 0 AND t0.c0;
SELECT * FROM t1 WHERE '-1' BETWEEN 0 AND t1.c0;
SELECT * FROM t0 WHERE '-1'>=0 AND '-1'<=t0.c0;
SELECT * FROM t1 WHERE '-1'>=0 AND '-1'<=t1.c0;
SELECT '-1' BETWEEN 0 AND t0.c0 FROM t0;
SELECT '-1' BETWEEN 0 AND t1.c0 FROM t1;

-- case: research/sqlite-forum/where_rowid_compare_with_float_near_int64_max | source: https://sqlite.org/forum/forumpost/2bdb86a068
CREATE TABLE t1(a INTEGER PRIMARY KEY);
INSERT INTO t1(a) VALUES(9223372036854775807);
SELECT 1 FROM t1 WHERE a>=(9223372036854775807+1);
SELECT a>=9223372036854775807+1 FROM t1;

-- case: research/sqlite-forum/update_where_in_subquery_and_multicolumn_index | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/where.test
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INT);
CREATE INDEX t1b ON t1(b,b,b,b,b,b,b,b,b,b,b,b,b);
INSERT INTO t1(a,b) VALUES(1,1),(15,2),(19,5);
UPDATE t1 SET b=999 WHERE a IN (SELECT 15) AND b IN (1,2);
SELECT * FROM t1 ORDER BY a;

-- case: research/sqlite-forum/select_distinct_many_duplicate_order_by_terms | source: https://sqlite.org/forum/forumpost/dfe8084751
CREATE TABLE t(x);
INSERT INTO t VALUES(1);
SELECT DISTINCT 'xyz' FROM t WHERE rowid OR abs(0) ORDER BY 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1;

-- case: research/sqlite-forum/cte_except_name_resolution_two_recursive | source: https://sqlite.org/src/info/31a19d11b97088296ac104aaff113a9790394927
WITH RECURSIVE
  t1(x) AS (VALUES(2) UNION ALL SELECT x+2 FROM t1 WHERE x<20),
  t2(y) AS (VALUES(3) UNION ALL SELECT y+3 FROM t2 WHERE y<20)
SELECT x FROM t1 EXCEPT SELECT y FROM t2 ORDER BY 1;

-- case: research/sqlite-forum/cte_in_view_cross_joined_with_cte_in_view | source: https://sqlite.org/src/tktview/ce823231949d3abf42453c8f20
CREATE TABLE t1(id INTEGER NULL PRIMARY KEY, name Text);
INSERT INTO t1 VALUES (1, 'john');
INSERT INTO t1 VALUES (2, 'james');
INSERT INTO t1 VALUES (3, 'jingle');
CREATE VIEW v2 AS WITH t4(Name) AS (VALUES ('A'), ('B')) SELECT Name Name FROM t4;
CREATE VIEW v3 AS
  WITH t4(Att, Val, Act) AS (VALUES ('C', 'D', 'E'), ('F', 'G', 'H'))
  SELECT D.Id Id, P.Name Protocol, T.Att Att, T.Val Val, T.Act Act
  FROM t1 D CROSS JOIN v2 P CROSS JOIN t4 T;
SELECT * FROM v3;

-- case: research/sqlite-forum/cte_generated_name_true_boolean_literal | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/with1.test
CREATE TABLE dual AS SELECT 'X' AS dummy;
WITH cte1 AS (
  SELECT TRUE, (
    WITH cte2 AS (SELECT avg(DISTINCT TRUE) FROM dual)
    SELECT 2571 FROM cte2
  ) AS subquery1
  FROM dual
  GROUP BY 1
)
SELECT (SELECT 1324 FROM cte1) FROM cte1;

-- case: research/sqlite-forum/recursive_cte_distinct_in_union_all_base | source: https://sqlite.org/src/info/c51489c3b8f919c5
CREATE TABLE t (label VARCHAR(10), step INTEGER);
INSERT INTO T VALUES('a', 1);
INSERT INTO T VALUES('a', 1);
INSERT INTO T VALUES('b', 1);
WITH RECURSIVE cte(label, step) AS (
    SELECT DISTINCT * FROM t
  UNION ALL
    SELECT label, step + 1 FROM cte WHERE step < 3
)
SELECT * FROM cte ORDER BY +label, +step;
WITH RECURSIVE cte(label, step) AS (
    SELECT * FROM t
  UNION
    SELECT label, step + 1 FROM cte WHERE step < 3
)
SELECT * FROM cte ORDER BY +label, +step;

-- case: research/sqlite-forum/recursive_cte_distinct_in_recursive_arm | source: https://sqlite.org/src/info/c51489c3b8f919c5
CREATE TABLE t (label VARCHAR(10), step INTEGER);
INSERT INTO T VALUES('a', 1);
INSERT INTO T VALUES('b', 1);
CREATE TABLE tworow(x);
INSERT INTO tworow(x) VALUES(1),(2);
WITH RECURSIVE cte(label, step) AS (
    SELECT * FROM t
  UNION ALL
    SELECT DISTINCT label, step + 1 FROM cte, tworow WHERE step < 3
)
SELECT * FROM cte ORDER BY +label, +step;

-- case: research/sqlite-forum/cte_in_trigger_shadows_main_table | source: https://sqlite.org/forum/forumpost/8590e3f6dc
CREATE TABLE t1(k);
CREATE TABLE log(k, cte_map, main_map);
CREATE TABLE map(k, v);
INSERT INTO map VALUES(1, 'main1'), (2, 'main2');
CREATE TRIGGER tr1 AFTER INSERT ON t1 BEGIN
  INSERT INTO log
    WITH map(k,v) AS (VALUES(1,'cte1'),(2,'cte2'))
    SELECT new.k, (SELECT v FROM map WHERE k=new.k), (SELECT v FROM main.map WHERE k=new.k);
END;
INSERT INTO t1 VALUES(1);
INSERT INTO t1 VALUES(2);
SELECT k, cte_map, main_map, '|' FROM log ORDER BY k;

-- case: research/sqlite-forum/recursive_cte_union_with_null_in_correlated_subquery | source: https://sqlite.org/forum/forumpost/2026-03-04T05:06:26Z
CREATE TABLE t1(x INTEGER PRIMARY KEY);
INSERT INTO t1 VALUES(1),(4),(999);
SELECT quote((
  WITH RECURSIVE t2(y) AS (
    SELECT 4
    UNION
    SELECT NULL
    UNION
    SELECT y+1 FROM t2 WHERE y=4 ORDER BY 1
  )
  SELECT 1 FROM t2 WHERE y=x
)) FROM t1;

-- case: research/sqlite-forum/cte_empty_arm_union_all_join | source: https://sqlite.org/forum/forumpost/d496c3d29bc93736
WITH
  t1(x) AS (SELECT 111),
  t2(y) AS (SELECT 222),
  t3(z) AS (SELECT * FROM t2 WHERE false UNION ALL SELECT * FROM t2)
SELECT * FROM t1, t3;

-- case: research/sqlite-forum/cte_nested_in_view_selfreference_then_rename | source: https://sqlite.org/forum/forumpost/aa4a7a3980
CREATE TABLE t1(a);
CREATE VIEW v2(c) AS
  WITH x AS (
    WITH y AS (
      WITH z AS(SELECT * FROM t1)
      SELECT * FROM v2
    ) SELECT a
  ) SELECT * from t1;
ALTER TABLE t1 RENAME COLUMN a TO b;
SELECT sql FROM sqlite_schema WHERE name='t1';
INSERT INTO t1 VALUES(55);
SELECT * FROM v2;

-- case: research/sqlite-forum/cte_nested_view_rename_table_values | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/with2.test
CREATE TABLE t1(a);
INSERT INTO t1 VALUES(1),('hello'),(4.25),(NULL),(x'3c626c6f623e');
CREATE VIEW v2(c) AS WITH x AS (WITH y AS (WITH z AS(SELECT * FROM t1) SELECT * FROM v2) SELECT a) SELECT * from t1;
CREATE VIEW v3(c) AS WITH x AS (WITH y AS (WITH z AS(SELECT * FROM v2) SELECT * FROM v3) SELECT a) SELECT * from t1;
ALTER TABLE t1 RENAME TO t1x;
SELECT quote(c) FROM v3;

-- case: research/sqlite-forum/cte_nested_with_repeated_references | source: https://sqlite.org/src/info/bb8a9fd4a9b7fce5
WITH xyz(a) AS (
  WITH abc AS ( SELECT 1234 ) SELECT * FROM abc
)
SELECT * FROM xyz AS one, xyz AS two, (SELECT * FROM xyz UNION ALL SELECT * FROM xyz);

-- case: research/sqlite-forum/in_list_or_terms_with_index | source: https://sqlite.org/src/info/6dcbfd11cf666e21
CREATE TABLE t1(a,b,c);
CREATE INDEX t1abc ON t1(a,b,c);
CREATE INDEX t1bca on t1(b,c,a);
INSERT INTO t1 VALUES(56,1119,1115);
INSERT INTO t1 VALUES(57,1147,1137);
INSERT INTO t1 VALUES(100,1050,1023);
INSERT INTO t1 VALUES(101,1050,1023);
SELECT * FROM t1 NOT INDEXED WHERE (b = 1137 AND c IN (97, 98)) OR (b = 1119 AND c IN (1115, 1023));
SELECT * FROM t1 WHERE (b = 1137 AND c IN (97, 98)) OR (b = 1119 AND c IN (1115, 1023));

-- case: research/sqlite-forum/in_list_and_between_on_trailing_index_columns | source: https://sqlite.org/src/info/5981a8c041a3c2f3
CREATE TABLE t1(id INTEGER PRIMARY KEY, a INT, b INT, c INT);
INSERT INTO t1 VALUES(10,1,2,5);
INSERT INTO t1 VALUES(20,1,3,5);
INSERT INTO t1 VALUES(30,1,2,4);
INSERT INTO t1 VALUES(40,1,3,4);
CREATE INDEX t1x ON t1(a,b,c);
SELECT * FROM t1 WHERE a=1 AND b IN (2,3) AND c BETWEEN 4 AND 5 ORDER BY +id;

-- case: research/sqlite-forum/in_list_with_scalar_subquery_and_text_between | source: https://sqlite.org/src/info/e41762333a4d6e90a49e628f488d0873b2dba4c5
CREATE TABLE t1(a TEXT, b INT, c INT, d INT);
INSERT INTO t1 VALUES('abc',123,4,5);
INSERT INTO t1 VALUES('xyz',1,'abcdefxyz',99);
CREATE INDEX t1abc ON t1(b,b,c);
SELECT * FROM t1 WHERE b IN (345, (SELECT 1 FROM t1 WHERE b IN (coalesce(1,abs(1))) AND c GLOB 'abc*xyz')) AND c BETWEEN 'abc' AND 'xyz';

-- case: research/sqlite-forum/distinct_zeroblob_equals_blob_of_zeros | source: https://sqlite.org/src/info/fccbde530a6583bf2748400919f1603d5425995c
CREATE TABLE t1(a INTEGER);
INSERT INTO t1 VALUES(3);
INSERT INTO t1 VALUES(2);
INSERT INTO t1 VALUES(1);
INSERT INTO t1 VALUES(2);
INSERT INTO t1 VALUES(3);
INSERT INTO t1 VALUES(1);
CREATE TABLE t2(x);
INSERT INTO t2 SELECT DISTINCT CASE a WHEN 1 THEN x'0000000000' WHEN 2 THEN zeroblob(5) ELSE 'xyzzy' END FROM t1;
SELECT quote(x) FROM t2 ORDER BY 1;

-- case: research/sqlite-forum/distinct_order_by_with_descending_index | source: https://sqlite.org/src/info/c5ea805691bfc4204b1cb9e9aa0103bd48bc7d34
CREATE TABLE t1(x);
INSERT INTO t1(x) VALUES(3),(1),(5),(2),(6),(4),(5),(1),(3);
CREATE INDEX t1x ON t1(x DESC);
SELECT DISTINCT x FROM t1 ORDER BY x ASC;
SELECT DISTINCT x FROM t1 ORDER BY x DESC;
DROP INDEX t1x;
CREATE INDEX t1x ON t1(x ASC);
SELECT DISTINCT x FROM t1 ORDER BY x DESC;

-- case: research/sqlite-forum/distinct_compound_subquery_max_name | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/distinct.test
CREATE TABLE jjj(x);
SELECT (SELECT 'mmm' UNION SELECT DISTINCT max(name) ORDER BY 1) FROM sqlite_master;
CREATE TABLE nnn(x);
SELECT (SELECT 'mmm' UNION SELECT DISTINCT max(name) ORDER BY 1) FROM sqlite_master;

-- case: research/sqlite-forum/distinct_cte_order_by_in_correlated_count | source: https://sqlite.org/src/info/9c944882
CREATE TABLE t1(a INTEGER PRIMARY KEY);
CREATE TABLE t3(a INTEGER PRIMARY KEY);
CREATE TABLE t4(x);
CREATE TABLE t5(y);
INSERT INTO t5 VALUES(1), (2), (2);
INSERT INTO t1 VALUES(2);
INSERT INTO t3 VALUES(2);
INSERT INTO t4 VALUES(2);
WITH t2(b) AS (SELECT DISTINCT y FROM t5 ORDER BY y)
SELECT * FROM t4 CROSS JOIN t3 CROSS JOIN t1 WHERE (t1.a=t3.a) AND (SELECT count(*) FROM t2 AS y WHERE t4.x!='abc')=t1.a;

-- case: research/sqlite-forum/distinct_with_partial_unique_index | source: https://sqlite.org/forum/forumpost/66954e9ece
CREATE TABLE person ( pid INT) ;
CREATE UNIQUE INDEX idx ON person ( pid ) WHERE pid == 1;
INSERT INTO person VALUES (1), (10), (10);
SELECT DISTINCT pid FROM person where pid = 10;

-- case: research/sqlite-forum/distinct_nocase_collation_with_indexes | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/distinct.test
CREATE TABLE t1(a, b);
INSERT INTO t1 VALUES('a', 'a');
INSERT INTO t1 VALUES('a', 'b');
INSERT INTO t1 VALUES('a', 'c');
INSERT INTO t1 VALUES('b', 'a');
INSERT INTO t1 VALUES('b', 'b');
INSERT INTO t1 VALUES('b', 'c');
INSERT INTO t1 VALUES('a', 'a');
INSERT INTO t1 VALUES('b', 'b');
INSERT INTO t1 VALUES('A', 'A');
INSERT INTO t1 VALUES('B', 'B');
CREATE INDEX i1 ON t1(a COLLATE nocase, b COLLATE nocase);
SELECT DISTINCT a, b FROM t1 ORDER BY a, b;
SELECT DISTINCT a COLLATE nocase, b COLLATE nocase FROM t1 ORDER BY a COLLATE nocase, b COLLATE nocase;

-- case: research/sqlite-forum/subquery_distinct_order_limit_offset | source: https://sqlite.org/forum/forumpost/0ec80f12d02acb3f
CREATE TABLE t1(x);
INSERT INTO t1 VALUES(1),(1),(1);
SELECT quote((SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 100)) FROM t1;
SELECT (SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 0) FROM t1;
INSERT INTO t1 VALUES(2);
SELECT (SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 1) FROM t1;
SELECT quote((SELECT DISTINCT x FROM t1 ORDER BY +x LIMIT 1 OFFSET 2)) FROM t1;

-- case: research/sqlite-forum/rename_table_in_view_with_nested_join_parens | source: https://sqlite.org/src/info/f50af3e8a565776b
CREATE TABLE t1(x);
CREATE VIEW t2 AS SELECT 1 FROM t1, (t1 AS a0, t1);
ALTER TABLE t1 RENAME TO t3;
SELECT sql FROM sqlite_master ORDER BY name;
INSERT INTO t3(x) VALUES(123);
SELECT * FROM t2;
INSERT INTO t3(x) VALUES('xyz');
SELECT * FROM t2;

-- case: research/sqlite-forum/add_column_after_schema_version_reload_not_null | source: https://sqlite.org/forum/forumpost/ddbe1c7efa
CREATE TABLE t1(a INT, b TEXT NOT NULL);
INSERT INTO t1 VALUES(1,2),('a','b');
ALTER TABLE t1 ADD COLUMN c INT DEFAULT 78;
SELECT * FROM t1;

-- case: research/sqlite-forum/json_control_characters_escaped | source: https://sqlite.org/src/info/ad2559db380abf8e
CREATE TABLE t8(a,b);
INSERT INTO t8(a) VALUES('abc' || char(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35) || 'xyz');
UPDATE t8 SET b=json_array(a);
SELECT b FROM t8;
SELECT a=json_extract(b,'$[0]') FROM t8;

-- case: research/sqlite-forum/json_whitespace_form_feed_invalid | source: https://sqlite.org/src/info/57eec374ae1d0a1d4a23077a95f4e173fe269113
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x20),char(0x20),char(0x20),char(0x20),char(0x20),char(0x20)));
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x0C),char(0x0C),char(0x0C),char(0x0C),char(0x0C),char(0x0C)));
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x20,0x09,0x0a,0x0c,0x0d,0x20),'','','','',''));

-- case: research/sqlite-forum/json_deep_nesting_limit | source: https://sqlite.org/src/info/981329adeef51011
SELECT json_valid(printf('%.1000c0%.1000c','[',']'));
SELECT json_valid(printf('%.1001c0%.1001c','[',']'));
SELECT json_valid(replace(printf('%.1000c0%.1000c','[','}'),'[','{"a":'));
SELECT json_valid(replace(printf('%.1001c0%.1001c','[','}'),'[','{"a":'));

-- case: research/sqlite-forum/json_empty_key_path | source: https://sqlite.org/forum/forumpost/c082aeab43
SELECT json_valid('{"":5}');
SELECT json_extract('{"":5}', '$.""');
SELECT json_extract('[3,{"a":4,"":[5,{"hi":6},7]},8]', '$[1].""[1].hi');
SELECT json_extract('[3,{"a":4,"":[5,{"hi":6},7]},8]', '$[1].""[1]."hi"');

-- case: research/sqlite-forum/json_infinity_literals | source: https://raw.githubusercontent.com/sqlite/sqlite/master/test/json101.test
SELECT json_object('a',2e370,'b',-3e380);
SELECT json_object('a',2e370,'b',-3e380)->>'a';
SELECT json_object('a',2e370,'b',-3e380)->>'b';

-- case: research/sqlite-forum/json_null_input_returns_null | source: https://sqlite.org/forum/forumpost/06c6334412
SELECT quote(json_error_position(NULL));
SELECT quote(json_valid(NULL));
SELECT quote(json(NULL));

-- case: research/sqlite-forum/json_set_repeated_paths_with_json_args | source: https://sqlite.org/forum/forumpost/b25edc1d46
SELECT json_set('{}', '$.a', json('1'), '$.a', json('2'), '$.b', json('3'), '$.b', json('4'), '$.c', json('5'), '$.c', json('6'));
SELECT json_replace('{"a":7,"b":8,"c":9}', '$.a', json('1'), '$.a', json('2'), '$.b', json('3'), '$.b', json('4'), '$.c', json('5'), '$.c', json('6'));

-- case: research/sqlite-forum/json_set_append_index_then_read | source: https://sqlite.org/forum/forumpost/fc0e3f1e2a
SELECT j, j->>0, j->>1 FROM (SELECT json_set(json_set('[]','$[#]',0), '$[#]',1) AS j);
SELECT j, j->>0, j->>1 FROM (SELECT json_set('[]','$[#]',0,'$[#]',1) AS j);
