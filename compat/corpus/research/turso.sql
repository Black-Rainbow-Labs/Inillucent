-- Cases taken from issues filed against Turso (formerly limbo), a reimplementation of SQLite. The source of each is on its marker line.
-- Gathered by the bug hunt of October 2026; see tasks/task-2201-bug-hunt-tdd.md.
-- Run every night by nightly::research_corpus against the pinned SQLite.

-- case: research/turso/delete_range_on_index_deletes_all | source: https://github.com/tursodatabase/turso/issues/2004
CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT);
CREATE INDEX idx_name ON test(name);
INSERT INTO test VALUES (1,'A'),(2,'B'),(3,'C'),(4,'D'),(5,'E');
DELETE FROM test WHERE name > 'C';
SELECT * FROM test ORDER BY name;

-- case: research/turso/having_without_group_by_alias | source: https://github.com/tursodatabase/turso/issues/3469
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
SELECT sum(a) s FROM t HAVING s = 14;
SELECT sum(a) s FROM t HAVING s = 15;

-- case: research/turso/nocase_in_join_on_group_distinct | source: https://github.com/tursodatabase/turso/issues/3476
CREATE TABLE a(s TEXT);
CREATE TABLE b(s TEXT);
INSERT INTO a VALUES ('A');
INSERT INTO b VALUES ('a');
SELECT a.s, b.s FROM a JOIN b ON a.s COLLATE NOCASE = b.s;
SELECT s FROM (SELECT 'A' AS s UNION ALL SELECT 'a') GROUP BY s COLLATE NOCASE;
SELECT DISTINCT s COLLATE NOCASE FROM (SELECT 'A' AS s UNION ALL SELECT 'a');

-- case: research/turso/in_list_applies_column_affinity | source: https://github.com/tursodatabase/turso/issues/3477
CREATE TABLE t(a text);
INSERT INTO t VALUES ('1');
SELECT * FROM t WHERE a = 1;
SELECT * FROM t WHERE a IN (1);

-- case: research/turso/rowid_compare_text_literal | source: https://github.com/tursodatabase/turso/issues/3478
CREATE TABLE t(a integer);
INSERT INTO t(rowid, a) VALUES (1, 1);
SELECT * FROM t WHERE rowid = '1';
SELECT * FROM t WHERE a = '1';

-- case: research/turso/left_join_using_two_columns_with_null | source: https://github.com/tursodatabase/turso/issues/3479
CREATE TABLE t(a, b);
CREATE TABLE s(a, b);
INSERT INTO t VALUES (1, NULL), (2, NULL);
INSERT INTO s VALUES (1, NULL), (2, NULL);
SELECT a, b FROM t LEFT JOIN s USING (a, b);
SELECT t.a, t.b, s.a, s.b FROM t LEFT JOIN s USING (a, b);

-- case: research/turso/integer_affinity_scientific_text | source: https://github.com/tursodatabase/turso/issues/3481
CREATE TABLE s(a integer);
INSERT INTO s VALUES ('2e0'), ('1e2'), ('2.5e0'), ('9.2233720368547758e18'), ('0x10');
SELECT typeof(a), quote(a) FROM s;

-- case: research/turso/equijoin_coerces_across_affinities | source: https://github.com/tursodatabase/turso/issues/3482
CREATE TABLE x(a INTEGER);
CREATE TABLE y(b TEXT);
INSERT INTO x VALUES (2),(3);
INSERT INTO y VALUES ('02'),('2'),('3x');
SELECT a, b FROM x JOIN y ON a = b ORDER BY a, b;

-- case: research/turso/update_pk_breaks_dangling_fk | source: https://github.com/tursodatabase/turso/issues/3648
PRAGMA foreign_keys=true;
CREATE TABLE t(a unique);
CREATE TABLE s(a, foreign key(a) references t(a));
INSERT INTO t VALUES (1);
INSERT INTO s VALUES (1);
UPDATE t SET a = a + 1;
SELECT * FROM s;

-- case: research/turso/insert_select_union_all_rows | source: https://github.com/tursodatabase/turso/issues/3946
CREATE TABLE t1 (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t1 VALUES (1, 'a'), (2, 'b');
INSERT INTO t2 SELECT id, name FROM t1 UNION SELECT 3, 'c';
SELECT * FROM t2 ORDER BY id;

-- case: research/turso/desc_index_compare_null | source: https://github.com/tursodatabase/turso/issues/4129
CREATE TABLE t(x);
CREATE INDEX tx ON t(x DESC);
INSERT INTO t VALUES (1),(2),(3);
SELECT * FROM t WHERE x > NULL;
SELECT * FROM t WHERE x < NULL;
SELECT * FROM t WHERE x = NULL;

-- case: research/turso/group_by_alias_order_by_alias_hang | source: https://github.com/tursodatabase/turso/issues/4588
SELECT 1 AS a GROUP BY a ORDER BY a;

-- case: research/turso/order_by_float_literal | source: https://github.com/tursodatabase/turso/issues/4608
SELECT 1 ORDER BY 0.5;

-- case: research/turso/group_by_order_by_float_literal_hang | source: https://github.com/tursodatabase/turso/issues/5238
CREATE TABLE t (a, b);
INSERT INTO t VALUES (1, 2);
SELECT * FROM t GROUP BY a, b ORDER BY 1.5;

-- case: research/turso/composite_pk_update_unique_false_conflict | source: https://github.com/tursodatabase/turso/issues/3463
CREATE TABLE p (a INT NOT NULL, b INT NOT NULL, v INT, PRIMARY KEY (a,b));
INSERT INTO p VALUES (6, 2, 0);
INSERT INTO p VALUES (-4, 4, 0);
UPDATE p SET a = -4, b = 5 WHERE a = -4 AND b = 4;
UPDATE p SET a = -4, b = 4 WHERE a = 6 AND b = 2;
SELECT * FROM p ORDER BY a, b;

-- case: research/turso/trigger_after_insert_subquery_count | source: https://github.com/tursodatabase/turso/issues/5115
CREATE TABLE t(id INTEGER PRIMARY KEY, data TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES('count=' || (SELECT count(*) FROM t)); END;
INSERT INTO t VALUES(1, 'a');
SELECT * FROM log;

-- case: research/turso/after_update_trigger_deletes_same_table | source: https://github.com/tursodatabase/turso/issues/5121
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t VALUES(1, 'a'), (2, 'b'), (3, 'c');
CREATE TRIGGER trg_del AFTER UPDATE ON t WHEN NEW.id = 1 BEGIN DELETE FROM t WHERE id = 2; END;
UPDATE t SET val = 'updated' WHERE id = 1;
SELECT * FROM t;

-- case: research/turso/nan_inf_text_into_integer_column | source: https://github.com/tursodatabase/turso/issues/5130
CREATE TABLE t(c INTEGER, r REAL);
INSERT INTO t VALUES ('nan', 'inf'), ('inf', 'nan'), ('-inf', '-inf'), ('Infinity', 'NaN');
SELECT typeof(c), quote(c), typeof(r), quote(r) FROM t;

-- case: research/turso/join_affinity_untyped_vs_text | source: https://github.com/tursodatabase/turso/issues/5134
CREATE TABLE a(x);
CREATE TABLE b(x TEXT);
INSERT INTO a VALUES (1);
INSERT INTO b VALUES ('1');
SELECT count(*) FROM a JOIN b ON a.x = b.x;

-- case: research/turso/sum_cast_blob_type | source: https://github.com/tursodatabase/turso/issues/5148
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t VALUES(1, '10'), (2, '20'), (3, '30');
SELECT typeof(SUM(CAST(val AS BLOB))), SUM(CAST(val AS BLOB)) FROM t;

-- case: research/turso/expression_index_update_old_entry | source: https://github.com/tursodatabase/turso/issues/5149
CREATE TABLE t(id INTEGER PRIMARY KEY, val INT);
CREATE INDEX idx ON t(val * 2);
INSERT INTO t VALUES(1, 10), (2, 20), (3, 30);
UPDATE t SET val = 99 WHERE id = 2;
PRAGMA integrity_check;
SELECT id FROM t WHERE val * 2 = 40;
SELECT id FROM t WHERE val * 2 = 198;

-- case: research/turso/fk_violation_midway_txn_commit | source: https://github.com/tursodatabase/turso/issues/5153
PRAGMA foreign_keys = ON;
CREATE TABLE parent (id INT PRIMARY KEY);
CREATE TABLE child (id INT, pid INT REFERENCES parent(id));
BEGIN;
INSERT INTO parent VALUES (2);
INSERT INTO child VALUES (999, 999);
INSERT INTO parent VALUES (3);
COMMIT;
SELECT * FROM parent ORDER BY id;

-- case: research/turso/fk_cascade_self_reference_chain | source: https://github.com/tursodatabase/turso/issues/5154
PRAGMA foreign_keys = ON;
CREATE TABLE T (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES T(id) ON DELETE CASCADE);
INSERT INTO T VALUES (1, NULL), (2, 1), (3, 2), (4, 3);
DELETE FROM T WHERE id = 1;
SELECT count(*) FROM T;

-- case: research/turso/json_extract_expression_index_update_all_rows | source: https://github.com/tursodatabase/turso/issues/5155
CREATE TABLE t(id INT PRIMARY KEY, j TEXT);
CREATE INDEX i ON t(json_extract(j, '$.x'));
INSERT INTO t VALUES (1, '{"x":1}'), (2, '{"x":1}'), (3, '{"x":1}'), (4, '{"x":2}');
UPDATE t SET j = '{"x":99}' WHERE json_extract(j, '$.x') = 1;
SELECT id, json_extract(j, '$.x') AS x FROM t ORDER BY id;

-- case: research/turso/strict_alter_add_column_bad_default | source: https://github.com/tursodatabase/turso/issues/5156
CREATE TABLE t1(name TEXT) STRICT;
INSERT INTO t1 VALUES ('alice'), ('bob');
ALTER TABLE t1 ADD COLUMN id INTEGER DEFAULT 'corrupted';
SELECT typeof(id), id + 10 FROM t1;

-- case: research/turso/partial_index_insert_nonmatching | source: https://github.com/tursodatabase/turso/issues/5158
CREATE TABLE t1(id INTEGER PRIMARY KEY, val INTEGER);
CREATE INDEX idx1 ON t1(val) WHERE val > 500;
INSERT INTO t1 VALUES(1, 100);
INSERT INTO t1 VALUES(2, 600);
PRAGMA integrity_check;
SELECT id FROM t1 WHERE val > 500;

-- case: research/turso/partial_index_update_leaves_predicate | source: https://github.com/tursodatabase/turso/issues/5168
CREATE TABLE t(val INT);
CREATE INDEX idx ON t(val) WHERE val > 10;
INSERT INTO t VALUES(15);
UPDATE t SET val = 5 WHERE val = 15;
PRAGMA integrity_check;
DELETE FROM t;
PRAGMA integrity_check;

-- case: research/turso/partial_index_with_or_predicate | source: https://github.com/tursodatabase/turso/issues/5429
CREATE TABLE t(x);
INSERT INTO t VALUES(0);
CREATE INDEX i ON t(x) WHERE x > 5 OR x < -5;
INSERT INTO t VALUES(10),(-10),(3);
PRAGMA integrity_check;
SELECT x FROM t WHERE x > 5 OR x < -5 ORDER BY x;

-- case: research/turso/partial_index_like_escape_or | source: https://github.com/tursodatabase/turso/issues/5162
CREATE TABLE t(x TEXT);
INSERT INTO t VALUES ('test');
CREATE INDEX i ON t(x) WHERE x LIKE 'a%' ESCAPE '\' OR x LIKE 'b%';
INSERT INTO t VALUES ('abc'),('bcd'),('cde');
PRAGMA integrity_check;
SELECT x FROM t WHERE x LIKE 'a%' ESCAPE '\' OR x LIKE 'b%' ORDER BY x;

-- case: research/turso/partial_index_qualified_function_predicate | source: https://github.com/tursodatabase/turso/issues/5430
CREATE TABLE t1(id INTEGER PRIMARY KEY, val TEXT, flag INTEGER);
CREATE INDEX idx_t1 ON t1(val) WHERE LENGTH(val) > 3;
INSERT INTO t1 VALUES (1, 'ab', 1);
INSERT INTO t1 VALUES (2, 'abcdef', 1);
PRAGMA integrity_check;

-- case: research/turso/check_nocase_in_case_expr | source: https://github.com/tursodatabase/turso/issues/5169
CREATE TABLE users(username TEXT COLLATE NOCASE, privilege INT, CHECK(CASE WHEN username='admin' THEN privilege>=100 ELSE 1 END));
INSERT INTO users VALUES ('Admin', 25);
SELECT count(*) FROM users;

-- case: research/turso/check_text_column_numeric_affinity | source: https://github.com/tursodatabase/turso/issues/5170
CREATE TABLE t(val TEXT CHECK(val > 5));
INSERT INTO t VALUES ('10');
SELECT count(*) FROM t;
SELECT '10' > 5;

-- case: research/turso/check_collate_in_concat | source: https://github.com/tursodatabase/turso/issues/5171
CREATE TABLE t(name TEXT CHECK((name COLLATE NOCASE || '') <> 'admin'));
INSERT INTO t VALUES ('Admin');
SELECT count(*) FROM t;

-- case: research/turso/json_valid_blob | source: https://github.com/tursodatabase/turso/issues/5172
SELECT json_valid(x'696e76616c6964');
SELECT json_valid('{"a":1}'), json_valid(x'7b2261223a317d');

-- case: research/turso/trigger_update_set_scalar_subquery | source: https://github.com/tursodatabase/turso/issues/5175
CREATE TABLE t1(id INTEGER PRIMARY KEY);
CREATE TABLE counter(n INTEGER);
INSERT INTO counter VALUES(0);
CREATE TRIGGER tr1 AFTER INSERT ON t1 BEGIN UPDATE counter SET n = (SELECT max(id) FROM t1); END;
INSERT INTO t1 VALUES(1);
INSERT INTO t1 VALUES(2);
SELECT * FROM t1 ORDER BY id;
SELECT * FROM counter;

-- case: research/turso/update_trigger_when_new_ipk | source: https://github.com/tursodatabase/turso/issues/5177
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT, counter INTEGER DEFAULT 0);
INSERT INTO t VALUES(1, 'a', 0), (2, 'b', 0), (3, 'c', 0);
CREATE TRIGGER tr1 AFTER UPDATE ON t WHEN NEW.id = 2 BEGIN UPDATE t SET counter = counter + 1 WHERE id = NEW.id; END;
UPDATE t SET val = 'updated' WHERE id = 2;
SELECT * FROM t WHERE id = 2;

-- case: research/turso/instead_of_trigger_on_view | source: https://github.com/tursodatabase/turso/issues/5178
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
CREATE VIEW v AS SELECT * FROM t;
CREATE TRIGGER trv INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(NEW.id, NEW.val); END;
INSERT INTO v VALUES (1, 'x');
SELECT * FROM t;

-- case: research/turso/raise_abort_in_trigger | source: https://github.com/tursodatabase/turso/issues/5179
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t VALUES(1, 'a');
CREATE TRIGGER tr1 BEFORE UPDATE ON t BEGIN SELECT RAISE(ABORT, 'not allowed'); END;
UPDATE t SET val = 'b' WHERE id = 1;
SELECT * FROM t;

-- case: research/turso/index_range_scan_excludes_nulls | source: https://github.com/tursodatabase/turso/issues/5181
CREATE TABLE t(a INT, b INT, c INT);
CREATE INDEX idx ON t(a, b, c);
INSERT INTO t VALUES (1, 1, 10), (1, NULL, 20), (1, 5, 30);
SELECT b, c FROM t WHERE a = 1 AND b < 3;
SELECT b, c FROM t WHERE a = 1 AND b > 0 ORDER BY b;

-- case: research/turso/correlated_in_subquery_outer_alias | source: https://github.com/tursodatabase/turso/issues/5213
CREATE TABLE t(x);
INSERT INTO t VALUES (1),(2);
SELECT * FROM t AS a WHERE a.x IN (SELECT a.x FROM t);

-- case: research/turso/unique_on_conflict_ignore_in_create | source: https://github.com/tursodatabase/turso/issues/5221
CREATE TABLE t(a, b, UNIQUE(a) ON CONFLICT IGNORE);
INSERT INTO t VALUES (1,1);
INSERT INTO t VALUES (1,2);
SELECT * FROM t;

-- case: research/turso/alter_add_column_bogus_collation | source: https://github.com/tursodatabase/turso/issues/5222
CREATE TABLE t(x);
ALTER TABLE t ADD COLUMN y COLLATE bogus;
SELECT count(*) FROM pragma_table_info('t');

-- case: research/turso/update_self_join_window_balance | source: https://github.com/tursodatabase/turso/issues/5223
CREATE TABLE v0 ( c1 TEXT );
INSERT INTO v0 ( c1 ) VALUES ('a'),('a'),('a'),('a'),('a'),('a'),('a'),('b'),('b'),('b'),('b'),('b'),('b');
UPDATE v0 SET c1 = c1 + 1e18 WHERE ( SELECT * FROM v0 AS a4 JOIN v0 AS a5 USING ( c1 ) JOIN v0 AS a6 USING ( c1 ) ORDER BY c1, sum ( 1e18 ) OVER ( ORDER BY c1 ) ) = 0;
SELECT c1, typeof(c1), count(*) FROM v0 GROUP BY 1, 2 ORDER BY 1;

-- case: research/turso/cte_in_update_set | source: https://github.com/tursodatabase/turso/issues/5224
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 2);
WITH c(v) AS (SELECT 9) UPDATE t SET a = c.v;
SELECT * FROM t;

-- case: research/turso/cte_original_name_after_alias | source: https://github.com/tursodatabase/turso/issues/5225
WITH c AS (SELECT 1 x) SELECT (SELECT c.x) FROM c t;

-- case: research/turso/insert_select_unknown_column_same_table | source: https://github.com/tursodatabase/turso/issues/5226
CREATE TABLE t(a);
INSERT INTO t(x) SELECT a FROM t;

-- case: research/turso/group_by_order_by_duplicate_terms | source: https://github.com/tursodatabase/turso/issues/5227
CREATE TABLE t(a);
INSERT INTO t VALUES (2),(1),(2);
SELECT a FROM t GROUP BY a ORDER BY a, a;

-- case: research/turso/sum_overflow_to_infinity | source: https://github.com/tursodatabase/turso/issues/5230
SELECT sum(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT 0);
SELECT total(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT -1e308);

-- case: research/turso/row_value_scalar_subquery_compare | source: https://github.com/tursodatabase/turso/issues/5231
SELECT (SELECT 1, 2) = (SELECT 1, 2);

-- case: research/turso/left_join_empty_table_aggregate_bare_column | source: https://github.com/tursodatabase/turso/issues/5233
CREATE TABLE t(x);
SELECT b.x, COUNT(*) FROM t a LEFT JOIN t b ON a.x=b.x;

-- case: research/turso/multi_index_or_subquery_outer_rows | source: https://github.com/tursodatabase/turso/issues/5234
CREATE TABLE t1(a INTEGER, b INTEGER);
CREATE INDEX idx_a ON t1(a);
CREATE INDEX idx_b ON t1(b);
INSERT INTO t1 VALUES(1,10);
CREATE TABLE t2(x);
INSERT INTO t2 VALUES(1);
INSERT INTO t2 VALUES(2);
SELECT t2.x, sub.a FROM t2, (SELECT a FROM t1 WHERE a=1 OR b=10) sub ORDER BY 1;

-- case: research/turso/cte_name_shadows_view_recursion | source: https://github.com/tursodatabase/turso/issues/5235
CREATE VIEW v AS SELECT 1;
WITH v AS (SELECT * FROM v) SELECT * FROM v;

-- case: research/turso/insert_returning_ipk_with_index | source: https://github.com/tursodatabase/turso/issues/5239
CREATE TABLE v0 (c1 INTEGER PRIMARY KEY AUTOINCREMENT, c2 TEXT);
CREATE INDEX i3 ON v0 (c1);
INSERT INTO v0 (c2) VALUES ('x') RETURNING c1, c2;
PRAGMA integrity_check;

-- case: research/turso/auto_rowid_with_index_on_ipk_upsert | source: https://github.com/tursodatabase/turso/issues/5240
CREATE TABLE v0 (c1 INTEGER PRIMARY KEY, c2 TEXT, c3 INT, c4 TEXT);
CREATE INDEX i6 ON v0 (c1);
INSERT INTO v0 VALUES (10, 'av3 b', 10, 'a__');
INSERT INTO v0 (c4, c2) VALUES ('v1', 18446744073709551615) ON CONFLICT DO UPDATE SET c2 = c4, c4 = c3;
INSERT INTO v0 (c4, c2) VALUES ('v2', 'zz');
PRAGMA integrity_check;
SELECT * FROM v0 ORDER BY c1;

-- case: research/turso/cast_large_real_to_integer | source: https://github.com/tursodatabase/turso/issues/5242
SELECT CAST(18446744073709551488 AS INTEGER);
SELECT CAST(1e19 AS INTEGER), CAST(-1e19 AS INTEGER), CAST(9223372036854775808.0 AS INTEGER);
SELECT substr('abcdefghijklmnopqrstuvwxyz', 18446744073709551488);

-- case: research/turso/returning_multi_column_subquery | source: https://github.com/tursodatabase/turso/issues/5243
CREATE TABLE t ( x );
INSERT INTO t VALUES ( 1 ) RETURNING ( SELECT 1, 2 );

-- case: research/turso/int64_max_vs_overflowed_float_where | source: https://github.com/tursodatabase/turso/issues/5244
CREATE TABLE v0 (v1 INTEGER PRIMARY KEY);
INSERT INTO v0 VALUES (9223372036854775807);
SELECT * FROM v0 WHERE v1 >= (9223372036854775807 + 1);
SELECT v1 >= (9223372036854775807 + 1) FROM v0;
SELECT typeof(9223372036854775807 + 1), 9223372036854775807 + 1;

-- case: research/turso/rename_column_case_mismatch | source: https://github.com/tursodatabase/turso/issues/5246
CREATE TABLE t1(a INT);
ALTER TABLE t1 RENAME COLUMN A TO b;
SELECT name FROM pragma_table_info('t1');

-- case: research/turso/limit_with_in_subquery | source: https://github.com/tursodatabase/turso/issues/5247
SELECT 1 LIMIT (1 IN (SELECT 1));

-- case: research/turso/scalar_subquery_rowid_of_derived_table | source: https://github.com/tursodatabase/turso/issues/5249
SELECT (SELECT t.rowid) FROM (SELECT 1) t;

-- case: research/turso/limit_int64_min | source: https://github.com/tursodatabase/turso/issues/5253
SELECT 1 LIMIT -9223372036854775808;
SELECT 1 LIMIT 5 OFFSET -9223372036854775808;
SELECT 1 LIMIT 9223372036854775807 OFFSET 9223372036854775807;

-- case: research/turso/update_changes_last_insert_rowid | source: https://github.com/tursodatabase/turso/issues/5280
CREATE TABLE t1(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t1 VALUES (1, 'a');
INSERT INTO t1 VALUES (5, 'b');
INSERT INTO t1(val) VALUES ('c');
UPDATE t1 SET val = 'updated' WHERE id = 1;
SELECT last_insert_rowid();

-- case: research/turso/upsert_do_update_unknown_table_ref | source: https://github.com/tursodatabase/turso/issues/5281
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER);
CREATE TABLE other(id INTEGER PRIMARY KEY, val INTEGER);
INSERT INTO t VALUES (1, 10);
INSERT INTO t VALUES (1, 20) ON CONFLICT(id) DO UPDATE SET val = other.val;
SELECT * FROM t;

-- case: research/turso/three_way_and_over_three_indexes | source: https://github.com/tursodatabase/turso/issues/5285
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c INTEGER);
CREATE INDEX idx_a ON t(a);
CREATE INDEX idx_b ON t(b);
CREATE INDEX idx_c ON t(c);
INSERT INTO t VALUES (1, 10, 100, 1000);
SELECT * FROM t WHERE a = 10 AND b = 100 AND c = 1000;

-- case: research/turso/insert_or_replace_not_null_default_other_columns | source: https://github.com/tursodatabase/turso/issues/5286
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER NOT NULL DEFAULT 42, name TEXT NOT NULL DEFAULT 'unnamed');
INSERT OR REPLACE INTO t VALUES (1, 100, 'hello');
SELECT * FROM t;
INSERT OR REPLACE INTO t VALUES (2, NULL, NULL);
SELECT * FROM t ORDER BY id;

-- case: research/turso/upsert_check_on_proposed_row | source: https://github.com/tursodatabase/turso/issues/5287
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER CHECK(val BETWEEN 0 AND 100), name TEXT);
INSERT INTO t VALUES (1, 50, 'one');
INSERT INTO t VALUES (1, 200, 'bad') ON CONFLICT(id) DO UPDATE SET val = 70, name = 'fixed';
SELECT * FROM t;

-- case: research/turso/upsert_not_null_error_message | source: https://github.com/tursodatabase/turso/issues/5288
CREATE TABLE my_tbl(id INTEGER PRIMARY KEY, val INTEGER NOT NULL, name TEXT NOT NULL);
INSERT INTO my_tbl VALUES (1, 10, 'first');
INSERT INTO my_tbl VALUES (1, NULL, 'second') ON CONFLICT(id) DO UPDATE SET val = excluded.val, name = excluded.name;
SELECT * FROM my_tbl;

-- case: research/turso/group_by_constant_select_hang | source: https://github.com/tursodatabase/turso/issues/5300
CREATE TABLE t(name TEXT);
INSERT INTO t VALUES ('a'), ('a'), ('b');
SELECT 'label:', name FROM t GROUP BY name ORDER BY name;
SELECT 42, name FROM t GROUP BY name ORDER BY name;
SELECT 1+1, name FROM t GROUP BY name ORDER BY name;

-- case: research/turso/left_join_ifnull_where_not_inner | source: https://github.com/tursodatabase/turso/issues/5302
CREATE TABLE v0 (c1, c2);
INSERT INTO v0 VALUES ('a', 1), ('b', 2), ('c', 3);
SELECT count(*) FROM v0 AS a4 LEFT JOIN v0 AS a5 ON (a5.c1 = a5.c2) WHERE ifnull(a5.c2, 2147483647) >= 127;

-- case: research/turso/fk_cascade_nocase_parent_key | source: https://github.com/tursodatabase/turso/issues/5408
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES('A');
INSERT INTO c VALUES('a');
DELETE FROM p WHERE id='A';
SELECT count(*) FROM c;

-- case: research/turso/deferred_fk_not_enforced_at_commit | source: https://github.com/tursodatabase/turso/issues/5409
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid TEXT REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c VALUES(1, 'missing');
INSERT INTO c VALUES(2, NULL);
UPDATE c SET pid = pid WHERE id=2;
COMMIT;
SELECT id, quote(pid) FROM c ORDER BY id;

-- case: research/turso/deferred_fk_parent_inserted_later | source: https://github.com/tursodatabase/turso/issues/6083
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid INTEGER PRIMARY KEY REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c(pid) VALUES(1);
INSERT INTO p(id) VALUES(1);
COMMIT;
SELECT * FROM c;

-- case: research/turso/delete_correlated_subquery_sees_deletions | source: https://github.com/tursodatabase/turso/issues/5426
CREATE TABLE t(id INTEGER PRIMARY KEY, grp TEXT, val INTEGER);
INSERT INTO t VALUES(1,'A',10),(2,'A',20),(3,'B',5),(4,'B',15);
DELETE FROM t WHERE val = (SELECT MIN(d2.val) FROM t d2 WHERE d2.grp = t.grp);
SELECT changes();
SELECT * FROM t ORDER BY id;

-- case: research/turso/changes_inside_transaction | source: https://github.com/tursodatabase/turso/issues/5427
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER);
INSERT INTO t VALUES(1,10),(2,20),(3,30);
BEGIN;
DELETE FROM t WHERE val > 15;
SELECT changes(), total_changes();
COMMIT;

-- case: research/turso/expression_index_cast_text_affinity_update | source: https://github.com/tursodatabase/turso/issues/5428
CREATE TABLE t0 (c0 INT);
INSERT INTO t0 VALUES (0);
CREATE INDEX idx ON t0 (CAST(c0 AS TEXT));
UPDATE t0 SET c0 = '0.0';
UPDATE t0 SET c0 = 1;
PRAGMA integrity_check;
SELECT c0, typeof(c0) FROM t0;

-- case: research/turso/any_column_type_affinity | source: https://github.com/tursodatabase/turso/issues/5431
CREATE TABLE t ( c ANY );
INSERT INTO t VALUES ('42');
SELECT typeof(c) FROM t;

-- case: research/turso/rename_column_partial_index_where | source: https://github.com/tursodatabase/turso/issues/5432
CREATE TABLE t ( a, b );
CREATE INDEX idx ON t ( b ) WHERE a = 1;
ALTER TABLE t RENAME COLUMN a TO a2;
INSERT INTO t VALUES (1, 2), (3, 4);
DELETE FROM t;
PRAGMA integrity_check;
SELECT sql FROM sqlite_master WHERE name = 'idx';

-- case: research/turso/delete_correlated_in_subquery_rowid | source: https://github.com/tursodatabase/turso/issues/5434
CREATE TABLE tt(a);
INSERT INTO tt VALUES (1),(2);
DELETE FROM tt WHERE a IN (SELECT a FROM tt tt1 WHERE tt1.rowid = tt.rowid);
SELECT count(*) FROM tt;

-- case: research/turso/last_insert_rowid_after_returning | source: https://github.com/tursodatabase/turso/issues/5490
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
INSERT INTO t1 VALUES(20, 'b') RETURNING *;
SELECT last_insert_rowid();

-- case: research/turso/avg_of_hex_text | source: https://github.com/tursodatabase/turso/issues/5500
CREATE TABLE v0 ( c1 );
INSERT INTO v0 VALUES ( hex(18446744073709551488) );
SELECT AVG(c1), SUM(c1), TOTAL(c1) FROM v0;

-- case: research/turso/real_affinity_nonnumeric_text_kept | source: https://github.com/tursodatabase/turso/issues/5502
CREATE TABLE t (c DOUBLE);
INSERT INTO t VALUES ('23g'), ('23'), (' 23 '), ('2e1x');
SELECT quote(c), typeof(c) FROM t;

-- case: research/turso/alter_add_column_references_enforced | source: https://github.com/tursodatabase/turso/issues/5503
PRAGMA foreign_keys=ON;
CREATE TABLE parent(p TEXT UNIQUE);
CREATE TABLE child(a INTEGER);
ALTER TABLE child ADD COLUMN p TEXT REFERENCES parent(p) ON DELETE RESTRICT ON UPDATE RESTRICT;
INSERT INTO child(a,p) VALUES (1,'missing');
SELECT * FROM child;

-- case: research/turso/order_by_nulls_first_last | source: https://github.com/tursodatabase/turso/issues/5534
CREATE TABLE t(id INTEGER PRIMARY KEY, val INT);
INSERT INTO t VALUES (1, NULL), (2, 3), (3, 1), (4, NULL), (5, 2);
SELECT id, val FROM t ORDER BY val DESC NULLS FIRST, id;
SELECT id, val FROM t ORDER BY val NULLS LAST, id;
SELECT id, val FROM t ORDER BY val ASC NULLS FIRST, id;

-- case: research/turso/row_value_in_literal_list | source: https://github.com/tursodatabase/turso/issues/5536
SELECT (1,2) IN ((1,2),(3,4));
CREATE TABLE t(a INT, b INT, c TEXT);
INSERT INTO t VALUES (1,10,'x'),(2,20,'y'),(1,20,'z');
SELECT * FROM t WHERE (a, b) IN ((1,10),(2,20)) ORDER BY a, b;

-- case: research/turso/aggregate_collate_leaks_to_later_aggregate | source: https://github.com/tursodatabase/turso/issues/5539
CREATE TABLE t(a TEXT, b TEXT);
INSERT INTO t VALUES ('z', 'a');
INSERT INTO t VALUES ('a', 'Z');
SELECT MIN(a COLLATE NOCASE), MAX(b) FROM t;

-- case: research/turso/left_join_where_case_iif | source: https://github.com/tursodatabase/turso/issues/5540
CREATE TABLE t1(id INT);
CREATE TABLE t2(id INT, val INT);
INSERT INTO t1 VALUES (1), (2), (3);
INSERT INTO t2 VALUES (1, 10), (3, 30);
SELECT t1.id, t2.val FROM t1 LEFT JOIN t2 ON t1.id = t2.id WHERE CASE WHEN t2.val IS NULL THEN 1 ELSE t2.val END > 0 ORDER BY t1.id;

-- case: research/turso/delete_limit_offset | source: https://github.com/tursodatabase/turso/issues/5542
CREATE TABLE t(id INTEGER PRIMARY KEY);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
DELETE FROM t ORDER BY id LIMIT 1 OFFSET 2;
SELECT * FROM t ORDER BY id;

-- case: research/turso/bare_columns_with_max | source: https://github.com/tursodatabase/turso/issues/5545
CREATE TABLE t(id INT, val INT, name TEXT);
INSERT INTO t VALUES (1,10,'first'),(2,30,'second'),(3,20,'third');
SELECT id, name, MAX(val) FROM t;
SELECT id, name, MIN(val) FROM t;

-- case: research/turso/before_update_trigger_correlated_update | source: https://github.com/tursodatabase/turso/issues/5548
CREATE TABLE t(id INT PRIMARY KEY, val INT);
CREATE TABLE log(x TEXT);
CREATE TRIGGER tr BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES('before'); END;
INSERT INTO t VALUES(1,100),(2,200),(3,300);
UPDATE t SET val = (SELECT SUM(t2.val) FROM t t2 WHERE t2.id != t.id) WHERE id <= 2;
SELECT * FROM t ORDER BY id;

-- case: research/turso/upsert_conflict_target_expression_index | source: https://github.com/tursodatabase/turso/issues/5550
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
CREATE UNIQUE INDEX idx_lower ON t(lower(val));
INSERT INTO t VALUES(1, 'Hello');
INSERT INTO t(val) VALUES('HELLO') ON CONFLICT(lower(val)) DO UPDATE SET val = 'UPDATED';
SELECT * FROM t ORDER BY id;

-- case: research/turso/expression_index_correlated_exists | source: https://github.com/tursodatabase/turso/issues/5551
CREATE TABLE t(b, c);
CREATE UNIQUE INDEX idx ON t(c, c + b);
INSERT INTO t VALUES (1, 2), (3, 4);
SELECT c + b FROM t WHERE EXISTS (SELECT 1 FROM t AS t2 WHERE t.b = t2.b) ORDER BY 1;

-- case: research/turso/window_with_group_by_order_by | source: https://github.com/tursodatabase/turso/issues/5552
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(1),(2);
SELECT a, count(a) OVER (PARTITION BY a ORDER BY a) FROM t GROUP BY a ORDER BY 2;

-- case: research/turso/between_in_expression_index | source: https://github.com/tursodatabase/turso/issues/5554
CREATE TABLE t(a INTEGER, b INTEGER);
CREATE INDEX idx ON t(a BETWEEN 1 AND 10);
INSERT INTO t VALUES(5, 5), (50, 5);
PRAGMA integrity_check;
SELECT a FROM t WHERE (a BETWEEN 1 AND 10) = 1;

-- case: research/turso/scalar_subquery_limit_zero | source: https://github.com/tursodatabase/turso/issues/5694
CREATE TABLE t(a);
INSERT INTO t VALUES (1), (2), (3);
SELECT 'row', (SELECT * FROM t LIMIT 0), (SELECT a FROM t LIMIT 1 OFFSET 5), (SELECT a FROM t ORDER BY a DESC LIMIT 1 OFFSET 1);

-- case: research/turso/round_half_cases | source: https://github.com/tursodatabase/turso/issues/5748
SELECT ROUND(2.25, 1), ROUND(2.35, 1), ROUND(2.45, 1), ROUND(0.5), ROUND(1.5), ROUND(2.5), ROUND(-2.5), ROUND(0.125, 2), ROUND(5.015, 2);

-- case: research/turso/substr_negative_start_utf8 | source: https://github.com/tursodatabase/turso/issues/5749
SELECT substr('café', -2, 3), substr('café', -1, 2), substr('café', -4, 5), substr('日本語', -1, 1), substr('日本語', -2, 2), substr('日本語', -3, 3), substr('hello', -2, 3);
SELECT substr('abc', 0, 2), substr('abc', -5, 3), substr('abc', 2, -1), substr('abc', 4), substr('abc', 1, 0);

-- case: research/turso/json_set_null_input | source: https://github.com/tursodatabase/turso/issues/5750
SELECT typeof(json_set(NULL, '$.a', 1)), typeof(json_insert(NULL, '$.a', 1)), typeof(json_replace(NULL, '$.a', 1)), typeof(json_remove(NULL, '$.a'));
SELECT COALESCE(JSON_INSERT(NULL, '$.a', 1), 'IS_NULL');

-- case: research/turso/json_group_array_empty | source: https://github.com/tursodatabase/turso/issues/5752
CREATE TABLE empty_t(val INTEGER);
SELECT JSON_GROUP_ARRAY(val), JSON_GROUP_OBJECT('k', val) FROM empty_t;

-- case: research/turso/json_group_object_nested_json_value | source: https://github.com/tursodatabase/turso/issues/5754
CREATE TABLE t(grp TEXT, val INT);
INSERT INTO t VALUES ('A', 1), ('A', 2), ('B', 3), ('B', 4);
SELECT json_group_object(grp, vals) FROM (SELECT grp, json_group_array(val) AS vals FROM t GROUP BY grp);

-- case: research/turso/blob_concat_keeps_bytes | source: https://github.com/tursodatabase/turso/issues/5757
SELECT hex(x'AB' || 'text'), hex('text' || x'AB'), hex(printf('%s', x'AB')), hex(REPLACE(X'010203040503', X'03', X'FF'));
SELECT typeof(x'AB' || 'text'), length(x'AB' || 'text');

-- case: research/turso/group_by_alias_shadows_column | source: https://github.com/tursodatabase/turso/issues/5759
CREATE TABLE t(x INT, y INT, z INT);
INSERT INTO t VALUES (1,10,100),(2,10,200),(3,20,300),(4,20,400);
SELECT x AS y, SUM(z) FROM t GROUP BY y ORDER BY y;

-- case: research/turso/pragma_table_info_keeps_type_params | source: https://github.com/tursodatabase/turso/issues/5762
CREATE TABLE t(a VARCHAR(100), b CHAR(50), c DECIMAL(10,2), d NUMERIC(5,3), e FLOAT(24), f NVARCHAR(200));
SELECT name, type FROM pragma_table_info('t');

-- case: research/turso/multiple_window_functions_over_clause | source: https://github.com/tursodatabase/turso/issues/5763
CREATE TABLE sales(id INTEGER PRIMARY KEY, product TEXT, amount REAL);
INSERT INTO sales VALUES (1,'A',100),(2,'B',200),(3,'A',150),(4,'B',50),(5,'A',300);
SELECT id, product, amount, SUM(amount) OVER (PARTITION BY product) AS product_total, SUM(amount) OVER () AS grand_total, amount * 100.0 / SUM(amount) OVER () AS pct_of_total FROM sales ORDER BY id;

-- case: research/turso/window_aggregate_row_value_misuse | source: https://github.com/tursodatabase/turso/issues/5765
CREATE TABLE t(a);
INSERT INTO t VALUES(0);
SELECT COUNT(*) FROM t ORDER BY SUM(CASE WHEN a THEN 1 ELSE (1, 1) END) OVER();

-- case: research/turso/deeply_nested_between_in_partial_index | source: https://github.com/tursodatabase/turso/issues/5767
CREATE TABLE t(x);
CREATE INDEX i ON t(x) WHERE x BETWEEN 0 AND CASE WHEN 1 THEN 1 END BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0;
INSERT INTO t VALUES(1);
PRAGMA integrity_check;
SELECT * FROM t;

-- case: research/turso/full_outer_join_nonequality_on | source: https://github.com/tursodatabase/turso/issues/5788
CREATE TABLE a(x INT);
CREATE TABLE b(x INT);
INSERT INTO a VALUES(1),(3);
INSERT INTO b VALUES(2),(4);
SELECT ifnull(a.x,'N')||','||ifnull(b.x,'N') FROM a FULL OUTER JOIN b ON a.x < b.x ORDER BY 1;

-- case: research/turso/right_join_then_join_column_order | source: https://github.com/tursodatabase/turso/issues/5796
CREATE TABLE a(a1 int); CREATE TABLE b(b1 int); CREATE TABLE c(c1 int);
INSERT INTO a VALUES(1); INSERT INTO b VALUES(2); INSERT INTO c VALUES(3);
SELECT * FROM a RIGHT JOIN b ON 1=1 JOIN c ON 1=1;

-- case: research/turso/update_correlated_exists_self | source: https://github.com/tursodatabase/turso/issues/5806
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
INSERT INTO t VALUES (1,1),(2,2),(3,3),(4,4),(5,5);
UPDATE t SET v = v + 10 WHERE EXISTS (SELECT 1 FROM t AS t2 WHERE t2.id = t.id-1 AND t2.v < 3);
SELECT * FROM t ORDER BY id;

-- case: research/turso/column_name_case_preserved | source: https://github.com/tursodatabase/turso/issues/5842
CREATE TABLE t(aAa);
INSERT INTO t VALUES (1);
SELECT name FROM pragma_table_info('t');
SELECT * FROM t;
SELECT aAa, "AAA", Aaa FROM t;

-- case: research/turso/before_update_trigger_new_rowid | source: https://github.com/tursodatabase/turso/issues/5844
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t VALUES(1, 'hello');
CREATE TRIGGER tr BEFORE UPDATE ON t WHEN NEW.id = 5 BEGIN SELECT RAISE(ABORT, 'blocked'); END;
UPDATE t SET id = 5 WHERE id = 1;
SELECT * FROM t;

-- case: research/turso/trigger_when_between | source: https://github.com/tursodatabase/turso/issues/5861
CREATE TABLE t(a INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t WHEN NEW.a BETWEEN 5 AND 10 BEGIN INSERT INTO log VALUES('in range: ' || NEW.a); END;
INSERT INTO t VALUES(7);
INSERT INTO t VALUES(70);
SELECT * FROM log;

-- case: research/turso/trigger_when_scalar_subquery | source: https://github.com/tursodatabase/turso/issues/5862
CREATE TABLE t(a INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t WHEN (SELECT count(*) FROM t) > 2 BEGIN INSERT INTO log VALUES('more than 2 rows'); END;
INSERT INTO t VALUES(1);
INSERT INTO t VALUES(2);
INSERT INTO t VALUES(3);
SELECT * FROM log;

-- case: research/turso/trigger_when_column_collation | source: https://github.com/tursodatabase/turso/issues/5863
CREATE TABLE t(name TEXT COLLATE NOCASE);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t WHEN NEW.name = 'hello' BEGIN INSERT INTO log VALUES('matched:' || NEW.name); END;
INSERT INTO t VALUES('HELLO');
INSERT INTO t VALUES('hello');
SELECT * FROM log;

-- case: research/turso/trigger_when_affinity | source: https://github.com/tursodatabase/turso/issues/5864
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER t_ai AFTER INSERT ON t WHEN NEW.a = '3' BEGIN INSERT INTO log VALUES('match'); END;
INSERT INTO t VALUES(3, 'z');
SELECT * FROM log;

-- case: research/turso/duplicate_trigger_name_rejected | source: https://github.com/tursodatabase/turso/issues/5866
CREATE TABLE t1(a);
CREATE TABLE t2(a);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO log VALUES('t1'); END;
CREATE TRIGGER tr AFTER INSERT ON t2 BEGIN INSERT INTO log VALUES('t2'); END;
INSERT INTO t1 VALUES(1);
SELECT * FROM log;

-- case: research/turso/trigger_same_name_as_table | source: https://github.com/tursodatabase/turso/issues/5867
CREATE TABLE t(x);
CREATE TABLE log(msg);
CREATE TRIGGER t AFTER INSERT ON t BEGIN INSERT INTO log VALUES('fired'); END;
INSERT INTO t VALUES (1);
SELECT * FROM log;

-- case: research/turso/trigger_quoted_identifiers | source: https://github.com/tursodatabase/turso/issues/5868
CREATE TABLE "my table"("my col" INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER "my trigger" AFTER INSERT ON "my table" BEGIN INSERT INTO log VALUES('inserted: ' || NEW."my col"); END;
INSERT INTO "my table" VALUES(42);
SELECT * FROM log;

-- case: research/turso/total_changes_counts_trigger_writes | source: https://github.com/tursodatabase/turso/issues/5869
CREATE TABLE t(a);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES('x'); END;
INSERT INTO t VALUES(1);
SELECT total_changes(), changes();

-- case: research/turso/last_insert_rowid_with_trigger_insert | source: https://github.com/tursodatabase/turso/issues/5870
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE t2(x INTEGER PRIMARY KEY, y TEXT);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.a + 1000, 'from trigger'); END;
INSERT INTO t1 VALUES(1, 'hello');
SELECT last_insert_rowid();

-- case: research/turso/returning_in_trigger_rejected | source: https://github.com/tursodatabase/turso/issues/5871
CREATE TABLE t(a);
CREATE TABLE other(b);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO other VALUES(NEW.a * 10) RETURNING *; END;
SELECT count(*) FROM sqlite_master WHERE type = 'trigger';

-- case: research/turso/before_insert_trigger_new_affinity | source: https://github.com/tursodatabase/turso/issues/5872
CREATE TABLE t(a REAL);
CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.a)); END;
INSERT INTO t VALUES(42);
SELECT * FROM log;

-- case: research/turso/before_update_trigger_with_or_ignore | source: https://github.com/tursodatabase/turso/issues/5873
CREATE TABLE t(a, b NOT NULL);
CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES('trigger ran'); END;
INSERT INTO t VALUES(1, 'hello');
UPDATE OR IGNORE t SET b = NULL WHERE a = 1;
SELECT * FROM log;
SELECT * FROM t;

-- case: research/turso/outer_conflict_clause_overrides_trigger | source: https://github.com/tursodatabase/turso/issues/5874
CREATE TABLE t1(a INTEGER PRIMARY KEY);
CREATE TABLE t2(b INTEGER PRIMARY KEY);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT OR IGNORE INTO t2 VALUES(NEW.a); END;
INSERT INTO t1 VALUES(1);
INSERT OR REPLACE INTO t1 VALUES(1);
SELECT * FROM t1;
SELECT * FROM t2;

-- case: research/turso/raise_message_expression | source: https://github.com/tursodatabase/turso/issues/5875
CREATE TABLE t(a INTEGER);
CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a < 0 BEGIN SELECT RAISE(ABORT, 'bad: ' || NEW.a); END;
INSERT INTO t VALUES(-5);
SELECT count(*) FROM t;

-- case: research/turso/json_each_in_trigger_body | source: https://github.com/tursodatabase/turso/issues/5876
CREATE TABLE t(a TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log SELECT value FROM json_each(NEW.a); END;
INSERT INTO t VALUES('[1,2,3]');
SELECT * FROM log;

-- case: research/turso/natural_join_no_common_columns | source: https://github.com/tursodatabase/turso/issues/5900
CREATE TABLE t1(a, b);
INSERT INTO t1 VALUES(1, 'x'),(2, 'y');
CREATE TABLE t2(c, d);
INSERT INTO t2 VALUES(1, 'a'),(3, 'b');
SELECT * FROM t1 NATURAL JOIN t2 ORDER BY 1, 3;
SELECT * FROM t1 NATURAL LEFT JOIN t2 ORDER BY 1, 3;

-- case: research/turso/json_array_length_null | source: https://github.com/tursodatabase/turso/issues/5901
SELECT json_array_length(NULL), json_array_length('[1,2]'), json_array_length('{"a":1}'), json_array_length('[1,[2,3]]', '$[1]');

-- case: research/turso/regexp_non_text_operands | source: https://github.com/tursodatabase/turso/issues/5903
CREATE TABLE t1(a INTEGER, b REAL, c TEXT, d BLOB);
INSERT INTO t1 VALUES(123, 45.6, '789', X'414243');
SELECT a LIKE '12%', b LIKE '45%', c LIKE '78%', d LIKE 'AB%' FROM t1;
SELECT a GLOB '12*', b GLOB '45*', c GLOB '78*', d GLOB 'AB*' FROM t1;

-- case: research/turso/distinct_aggregate_in_correlated_subquery | source: https://github.com/tursodatabase/turso/issues/5906
CREATE TABLE t1(a INTEGER, b TEXT);
INSERT INTO t1 VALUES(1,'abc'),(2,'def'),(3,'abc'),(4,'ghi'),(5,'abc');
SELECT a, (SELECT count(DISTINCT t2.b) FROM t1 t2 WHERE t2.rowid <= t1.rowid) FROM t1 ORDER BY rowid;

-- case: research/turso/unicode_of_nul_char | source: https://github.com/tursodatabase/turso/issues/5907
SELECT typeof(unicode(char(0))), unicode(char(0)) IS NULL, length(char(0)), hex(char(0, 65));
SELECT unicode(''), unicode('a'), unicode('😀'), char(128512), length('😀');

-- case: research/turso/cte_same_name_as_table_schema_qualified | source: https://github.com/tursodatabase/turso/issues/5912
CREATE TABLE t1(a INTEGER);
INSERT INTO t1 VALUES(1),(2),(3);
WITH t1 AS (SELECT a * 10 AS a FROM main.t1) SELECT * FROM t1 ORDER BY a;

-- case: research/turso/max_on_integer_primary_key_after_delete | source: https://github.com/tursodatabase/turso/issues/5938
CREATE TABLE t(id INT PRIMARY KEY, v INT);
INSERT INTO t VALUES (1, 10), (2, 20), (3, 30);
DELETE FROM t WHERE id = 3;
SELECT MAX(id), MIN(id), COUNT(*) FROM t;

-- case: research/turso/real_vs_integer_compare_above_2_53 | source: https://github.com/tursodatabase/turso/issues/6056
CREATE TABLE t9 (val REAL);
INSERT INTO t9 VALUES (9007199254740992.0);
SELECT val = 9007199254740993, val < 9007199254740993, val > 9007199254740993 FROM t9;
CREATE TABLE t11 (val REAL);
INSERT INTO t11 VALUES (-3036093696168066048.0);
SELECT val = -3036093696168066000, val < -3036093696168066000, val > -3036093696168066000 FROM t11;
SELECT CAST(9007199254740993 AS REAL) = 9007199254740993, 9007199254740993 = 9007199254740992.0;

-- case: research/turso/correlated_subquery_having_via_derived_table | source: https://github.com/tursodatabase/turso/issues/6073
CREATE TABLE t(grp TEXT, cat TEXT, val INTEGER);
INSERT INTO t VALUES('a','x',1),('a','y',2);
SELECT grp, cat, SUM(val) AS s FROM t GROUP BY grp, cat HAVING s = (SELECT sq FROM (SELECT MAX(val) AS sq FROM t t2 WHERE t2.grp = t.grp)) ORDER BY cat;

-- case: research/turso/multi_level_left_join_null_key | source: https://github.com/tursodatabase/turso/issues/6077
CREATE TABLE a(id INTEGER PRIMARY KEY);
CREATE TABLE b(id INTEGER PRIMARY KEY, a_id INTEGER);
CREATE TABLE c(id INTEGER PRIMARY KEY, b_id INTEGER);
INSERT INTO a VALUES (1),(2);
INSERT INTO b VALUES (1,1);
INSERT INTO c VALUES (1,NULL);
CREATE INDEX idx_c_bid ON c(b_id);
SELECT a.id, b.id AS bid, c.id AS cid FROM a LEFT JOIN b ON a.id = b.a_id LEFT JOIN c ON b.id = c.b_id ORDER BY a.id;

-- case: research/turso/right_join_using_star_of_left_table | source: https://github.com/tursodatabase/turso/issues/6081
CREATE TABLE t1(id INT, a TEXT);
CREATE TABLE t2(id INT, b TEXT);
INSERT INTO t1 VALUES(1, 'x'), (2, 'y');
INSERT INTO t2 VALUES(2, 'p'), (3, 'q');
SELECT t1.* FROM t1 RIGHT JOIN t2 USING(id) ORDER BY id;
SELECT id, a, b FROM t1 RIGHT JOIN t2 USING(id) ORDER BY id;

-- case: research/turso/upsert_reads_column_default_for_pre_alter_rows | source: https://github.com/tursodatabase/turso/issues/6087
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t VALUES(1, 'alice');
ALTER TABLE t ADD COLUMN score INTEGER DEFAULT 42;
INSERT INTO t VALUES(1, 'bob', 0) ON CONFLICT(id) DO UPDATE SET score = score + 1;
SELECT * FROM t;

-- case: research/turso/join_comparison_uses_left_column_collation | source: https://github.com/tursodatabase/turso/issues/6088
CREATE TABLE t1(a TEXT);
CREATE TABLE t2(b TEXT COLLATE NOCASE, c INT);
INSERT INTO t1 VALUES('Hello');
INSERT INTO t2 VALUES('HELLO', 1);
SELECT * FROM t1, t2 WHERE t1.a = t2.b;
SELECT * FROM t1, t2 WHERE t2.b = t1.a;

-- case: research/turso/correlated_exists_inside_aggregate_case | source: https://github.com/tursodatabase/turso/issues/6090
CREATE TABLE items (id INTEGER, val INTEGER, cat TEXT);
CREATE TABLE refs (id INTEGER);
INSERT INTO items VALUES (1,10,'A'),(2,20,'A'),(3,30,'B'),(4,40,'B');
INSERT INTO refs VALUES (1),(3);
SELECT cat, SUM(CASE WHEN EXISTS (SELECT 1 FROM refs WHERE refs.id = items.id) THEN val ELSE 0 END) AS s FROM items GROUP BY cat ORDER BY cat;

-- case: research/turso/nullif_uses_column_collation | source: https://github.com/tursodatabase/turso/issues/6092
CREATE TABLE t (col TEXT COLLATE NOCASE);
INSERT INTO t VALUES ('abc'), ('ABC'), ('xyz');
SELECT col, NULLIF(col, 'ABC') FROM t;

-- case: research/turso/indexed_by_partial_index_semantics | source: https://github.com/tursodatabase/turso/issues/6093
CREATE TABLE t(id INT, score INT, flag INT);
INSERT INTO t VALUES(1,10,0),(2,20,0),(3,30,1),(4,40,1),(5,50,0);
CREATE INDEX idx ON t(score) WHERE flag = 1;
SELECT COUNT(*) FROM t INDEXED BY idx;

-- case: research/turso/indexed_by_partial_index_matching_where | source: https://github.com/tursodatabase/turso/issues/8377
create table t(a, b);
create index idx on t(a) where b < 5;
insert into t values (-1, 1), (-3, 3), (-5, 5), (-8, 8);
select a from t indexed by idx order by a;
select a from t indexed by idx where b < 5 order by a;

-- case: research/turso/int_text_range_join_affinity | source: https://github.com/tursodatabase/turso/issues/6095
CREATE TABLE ti (a INT);
CREATE TABLE tt (b TEXT);
INSERT INTO ti VALUES (1),(2),(3),(4),(5),(6);
INSERT INTO tt VALUES ('1'),('2'),('3'),('4'),('5'),('6');
SELECT count(*) FROM ti JOIN tt ON a <= b;
SELECT count(*) FROM ti, tt WHERE a <= b;

-- case: research/turso/named_window_override_order_by | source: https://github.com/tursodatabase/turso/issues/6096
CREATE TABLE t(val INT);
INSERT INTO t VALUES(1),(2),(3);
SELECT SUM(val) OVER (w ORDER BY val) FROM t WINDOW w AS (ORDER BY val);

-- case: research/turso/json_extract_bracket_quoted_key | source: https://github.com/tursodatabase/turso/issues/6099
SELECT json_extract('{"key with spaces":1}', '$["key with spaces"]');
SELECT json_extract('{"a":{"b":[10,20]}}', '$.a.b[1]'), json_extract('{"a":{"b":[10,20]}}', '$.a.b[#-1]');

-- case: research/turso/update_where_empty_string_with_trigger | source: https://github.com/tursodatabase/turso/issues/6111
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
INSERT INTO t VALUES (1, 'a');
CREATE TRIGGER tr BEFORE UPDATE ON t BEGIN SELECT 1; END;
UPDATE t SET v='b' WHERE '';
SELECT * FROM t;

-- case: research/turso/delete_returning_virtual_generated | source: https://github.com/tursodatabase/turso/issues/6149
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
INSERT INTO t1(a) VALUES(1),(2),(3);
DELETE FROM t1 WHERE a = 2 RETURNING b;
DELETE FROM t1 WHERE a > 2 RETURNING *;
DELETE FROM t1 RETURNING a;

-- case: research/turso/generated_column_not_null_on_update | source: https://github.com/tursodatabase/turso/issues/6159
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL NOT NULL);
INSERT INTO t1(a) VALUES(1),(2),(3);
UPDATE t1 SET a = NULL WHERE a = 2;
SELECT * FROM t1 ORDER BY rowid;

-- case: research/turso/generated_column_references_auto_rowid | source: https://github.com/tursodatabase/turso/issues/6265
CREATE TABLE t1(id INTEGER PRIMARY KEY, a TEXT, b AS (id * 10) CHECK(b IS NOT NULL));
INSERT INTO t1(a) VALUES('hello');
SELECT id, a, b FROM t1;

-- case: research/turso/compound_select_limit_offset_nested_union | source: https://github.com/tursodatabase/turso/issues/6311
SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 LIMIT 2;
SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 LIMIT 2 OFFSET 1;

-- case: research/turso/rename_column_trigger_order_by_alias | source: https://github.com/tursodatabase/turso/issues/6332
CREATE TABLE src(a, b);
CREATE TABLE dst(x);
CREATE TABLE log(v);
INSERT INTO src VALUES(2,100),(1,200);
CREATE TRIGGER trig AFTER INSERT ON dst BEGIN INSERT INTO log SELECT a AS b FROM src ORDER BY b; END;
ALTER TABLE src RENAME COLUMN b TO c;
INSERT INTO dst VALUES(1);
SELECT group_concat(v, '|') FROM log;

-- case: research/turso/rename_column_trigger_cte_shadows_table | source: https://github.com/tursodatabase/turso/issues/6333
CREATE TABLE src(a, b);
CREATE TABLE dst(x);
CREATE TABLE log(v);
CREATE TRIGGER trig AFTER INSERT ON dst BEGIN INSERT INTO log WITH src AS (SELECT 100 AS b) SELECT b FROM src; END;
ALTER TABLE src RENAME COLUMN b TO c;
INSERT INTO dst VALUES(1);
SELECT * FROM log;

-- case: research/turso/compound_limit_in_in_subquery | source: https://github.com/tursodatabase/turso/issues/6344
CREATE TABLE t(x INTEGER);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
SELECT * FROM t WHERE x IN (SELECT 1 UNION SELECT 2 UNION SELECT 3 LIMIT 2) ORDER BY x;

-- case: research/turso/drop_user_unique_index | source: https://github.com/tursodatabase/turso/issues/6359
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
CREATE UNIQUE INDEX ux_t_ab ON t(a, b);
DROP INDEX ux_t_ab;
SELECT count(*) FROM sqlite_master WHERE type = 'index';

-- case: research/turso/update_rowid_with_index_on_virtual_column | source: https://github.com/tursodatabase/turso/issues/6404
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, v AS (a * 10) VIRTUAL);
CREATE INDEX idx_v ON t(v);
INSERT INTO t(id, a) VALUES (1, 5);
UPDATE t SET id = 100 WHERE id = 1;
SELECT id, a, v FROM t;
PRAGMA integrity_check;

-- case: research/turso/compound_three_arm_collation | source: https://github.com/tursodatabase/turso/issues/6407
SELECT 'Hello' COLLATE NOCASE UNION SELECT 'hello' UNION SELECT 'HELLO';

-- case: research/turso/compound_order_by_collate | source: https://github.com/tursodatabase/turso/issues/6408
SELECT 'Hello' AS x UNION ALL SELECT 'hello' UNION ALL SELECT 'apple' ORDER BY 1 COLLATE NOCASE;

-- case: research/turso/in_compound_subquery_affinity | source: https://github.com/tursodatabase/turso/issues/6409
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES(1),(2),(3);
SELECT * FROM t WHERE a IN (SELECT '1' UNION SELECT '2') ORDER BY a;
SELECT * FROM t WHERE a IN (SELECT '1' UNION ALL SELECT '2') ORDER BY a;

-- case: research/turso/order_by_column_number_collate | source: https://github.com/tursodatabase/turso/issues/6410
CREATE TABLE t(x TEXT);
INSERT INTO t VALUES('b'),('A'),('c'),('a');
SELECT x FROM t ORDER BY 1 COLLATE NOCASE, x;
SELECT x FROM t GROUP BY 1 COLLATE NOCASE ORDER BY 1;

-- case: research/turso/group_by_literal_having_hang | source: https://github.com/tursodatabase/turso/issues/6412
SELECT 1 AS x GROUP BY 1 HAVING 1;
SELECT 1 AS x GROUP BY 1 HAVING 0;
SELECT 42 GROUP BY 1 HAVING 1;

-- case: research/turso/strict_table_before_trigger_new_type | source: https://github.com/tursodatabase/turso/issues/6416
CREATE TABLE t(x INTEGER) STRICT;
CREATE TABLE log(v TEXT);
CREATE TRIGGER bi BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.x)); END;
INSERT INTO t VALUES('1');
SELECT v FROM log;

-- case: research/turso/cast_invalid_utf8_blob_to_text | source: https://github.com/tursodatabase/turso/issues/6483
SELECT typeof(CAST(x'CAFE' AS TEXT)), hex(CAST(x'CAFE' AS TEXT)), length(CAST(x'CAFE' AS TEXT)), length(CAST(x'CAFE' AS TEXT), 0);
SELECT hex(CAST(x'80' AS TEXT) || 'a'), hex(upper(CAST(x'C3' AS TEXT)));

-- case: research/turso/correlated_subquery_inside_aggregate | source: https://github.com/tursodatabase/turso/issues/6485
CREATE TABLE t1(grp TEXT, val INTEGER);
CREATE TABLE t2(grp TEXT, factor INTEGER);
INSERT INTO t1 VALUES ('a',1),('a',2),('b',3),('b',4);
INSERT INTO t2 VALUES ('a',10),('b',20);
SELECT grp, sum((SELECT factor FROM t2 WHERE t2.grp = t1.grp)) FROM t1 GROUP BY grp ORDER BY grp;

-- case: research/turso/unique_virtual_generated_depends_on_virtual | source: https://github.com/tursodatabase/turso/issues/6492
CREATE TABLE t (a UNIQUE AS (b), b AS ('x'), c);
INSERT INTO t(c) VALUES (1);
INSERT INTO t(c) VALUES (2);
SELECT count(*) FROM t;

-- case: research/turso/update_via_index_over_virtual_generated | source: https://github.com/tursodatabase/turso/issues/6599
CREATE TABLE t(a TEXT, g INT AS (a = 10) VIRTUAL);
CREATE INDEX idx_expr ON t(g+0);
INSERT INTO t(a) VALUES('10'), ('5');
UPDATE t SET a='11' WHERE g+0=1 RETURNING a, g, g+0;
SELECT 'all', a, g FROM t ORDER BY a;
PRAGMA integrity_check;

-- case: research/turso/delete_index_covers_virtual_generated | source: https://github.com/tursodatabase/turso/issues/6612
CREATE TABLE t (c0 TEXT, c1 INTEGER AS (c2 + 1), c2 INTEGER);
CREATE INDEX i ON t (c1, c0);
INSERT INTO t(c0, c2) VALUES ('A', 1), ('Z', 2);
DELETE FROM t WHERE c0 < 'X';
SELECT * FROM t;
PRAGMA integrity_check;

-- case: research/turso/update_index_chained_virtual_generated | source: https://github.com/tursodatabase/turso/issues/6613
CREATE TABLE t (a INTEGER, c TEXT, e AS (a), d AS (e));
INSERT INTO t (a, c) VALUES (1, 'aaa');
CREATE INDEX idx_t ON t (d, c);
UPDATE t SET c = 'bbb';
PRAGMA integrity_check;

-- case: research/turso/rename_column_round_trip_trigger_sql | source: https://github.com/tursodatabase/turso/issues/6625
CREATE TABLE log(msg TEXT);
CREATE TABLE core(id TEXT UNIQUE, row_rank TEXT UNIQUE);
CREATE TRIGGER core_ai AFTER INSERT ON core BEGIN INSERT INTO log VALUES(NEW.id || ':' || NEW.row_rank); END;
ALTER TABLE core RENAME COLUMN row_rank TO row_rank2;
ALTER TABLE core RENAME COLUMN row_rank2 TO row_rank;
INSERT INTO core(id,row_rank) VALUES('i-001','A001');
SELECT * FROM log;
SELECT sql FROM sqlite_schema WHERE name='core_ai';

-- case: research/turso/text_integer_beyond_int64_arithmetic | source: https://github.com/tursodatabase/turso/issues/6704
SELECT typeof('9223372036854775808'+0), quote('9223372036854775808'+0);
SELECT typeof('-9223372036854775809'+0), quote('-9223372036854775809'+0);
SELECT typeof('9223372036854775807'+0), '9223372036854775807'+1;

-- case: research/turso/real_index_seek_past_2_53 | source: https://github.com/tursodatabase/turso/issues/6715
CREATE TABLE t (f REAL);
CREATE INDEX t_f ON t(f);
INSERT INTO t VALUES (-2701729208700874036);
SELECT count(*) FROM t WHERE f = -2701729208700874000;
SELECT count(*) FROM t WHERE f = -2701729208700874036;
SELECT count(*) FROM t WHERE f = -2701729208700874240;
SELECT quote(f) FROM t;

-- case: research/turso/without_rowid_affinity_in_declared_order | source: https://github.com/tursodatabase/turso/issues/6749
CREATE TABLE t(c TEXT, b INTEGER PRIMARY KEY, a REAL) WITHOUT ROWID;
INSERT INTO t VALUES ('x', '1', '2.5');
SELECT typeof(c), typeof(b), typeof(a) FROM t;

-- case: research/turso/having_aggregate_filter_in_subquery | source: https://github.com/tursodatabase/turso/issues/6807
SELECT 1 GROUP BY 1 HAVING COUNT(*) FILTER (WHERE 1 IN (SELECT 2)) = 1;
SELECT 1 GROUP BY 1 HAVING COUNT(*) FILTER (WHERE 1 IN (SELECT 1)) = 1;

-- case: research/turso/upsert_do_update_index_before_validation | source: https://github.com/tursodatabase/turso/issues/6858
CREATE TABLE t(id INTEGER PRIMARY KEY, u INT UNIQUE, b INT, c INT UNIQUE);
CREATE INDEX idx_b ON t(b);
INSERT INTO t VALUES(1,1,10,10);
INSERT INTO t VALUES(2,2,20,20);
INSERT OR FAIL INTO t VALUES(3,1,30,30) ON CONFLICT(u) DO UPDATE SET b=99,c=20;
PRAGMA integrity_check;
SELECT id,u,b,c FROM t ORDER BY id;

-- case: research/turso/upsert_or_replace_do_update_unique_conflict | source: https://github.com/tursodatabase/turso/issues/6859
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE);
INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
INSERT OR REPLACE INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
PRAGMA integrity_check;
SELECT * FROM t ORDER BY id;

-- case: research/turso/upsert_or_rollback_statement_scope | source: https://github.com/tursodatabase/turso/issues/6860
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE);
INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN;
INSERT INTO t VALUES(3,'u3','a3');
INSERT OR ROLLBACK INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT count(*) FROM t WHERE id=3;

-- case: research/turso/replace_default_substitution_vs_check | source: https://github.com/tursodatabase/turso/issues/6861
CREATE TABLE t(a INT NOT NULL DEFAULT 5 CHECK(a IS NULL));
INSERT OR REPLACE INTO t VALUES(NULL);
SELECT count(*) FROM t;

-- case: research/turso/replace_default_strict_typecheck | source: https://github.com/tursodatabase/turso/issues/6862
CREATE TABLE t(a INT NOT NULL DEFAULT 'x') STRICT;
INSERT OR REPLACE INTO t VALUES(NULL);
SELECT count(*) FROM t;

-- case: research/turso/replace_default_index_affinity | source: https://github.com/tursodatabase/turso/issues/6863
CREATE TABLE t(a INT NOT NULL DEFAULT '5');
CREATE INDEX idx ON t(a);
INSERT OR REPLACE INTO t VALUES(NULL);
PRAGMA integrity_check;
SELECT typeof(a), a FROM t;
SELECT count(*) FROM t WHERE a = 5;

-- case: research/turso/json_error_inside_transaction_keeps_txn | source: https://github.com/tursodatabase/turso/issues/6864
CREATE TABLE t(id INT);
BEGIN;
INSERT INTO t VALUES(9);
SELECT json_extract('not json','$.a');
SELECT count(*) FROM t;
COMMIT;
SELECT count(*) FROM t;

-- case: research/turso/upsert_update_id_affinity_where | source: https://github.com/tursodatabase/turso/issues/6866
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, x INT);
CREATE INDEX idx_x ON t(x);
INSERT INTO t VALUES(1,10,100);
INSERT INTO t VALUES(2,20,200);
INSERT OR FAIL INTO t(id,a,x) VALUES(1,99,999) ON CONFLICT(id) DO UPDATE SET id=2 WHERE a < '2';
PRAGMA integrity_check;
SELECT id,a,x FROM t ORDER BY id;

-- case: research/turso/upsert_where_uses_column_collation | source: https://github.com/tursodatabase/turso/issues/6867
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT COLLATE NOCASE, x INT);
CREATE INDEX idx_x ON t(x);
INSERT INTO t VALUES(1,'A',100);
INSERT INTO t VALUES(2,'B',200);
INSERT OR FAIL INTO t(id,a,x) VALUES(1,'Z',999) ON CONFLICT(id) DO UPDATE SET id=2 WHERE a != 'a';
PRAGMA integrity_check;
SELECT id,a,x FROM t ORDER BY id;

-- case: research/turso/replace_expression_index_error_after_delete | source: https://github.com/tursodatabase/turso/issues/6877
CREATE TABLE t(id INTEGER PRIMARY KEY, b INT);
CREATE INDEX idx ON t(CASE WHEN b=2 THEN 'x' LIKE 'x' ESCAPE 'yy' ELSE b END);
INSERT INTO t VALUES(1,1);
INSERT OR REPLACE INTO t VALUES(1,2);
SELECT id,b FROM t ORDER BY id;
PRAGMA integrity_check;

-- case: research/turso/returning_error_rolls_back_statement | source: https://github.com/tursodatabase/turso/issues/6878
CREATE TABLE t(a INT);
INSERT INTO t VALUES(1),(2);
BEGIN;
DELETE FROM t WHERE a=1 RETURNING 'x' LIKE 'x' ESCAPE 'yy';
SELECT * FROM t ORDER BY a;
COMMIT;

-- case: research/turso/update_row_error_rolls_back_statement | source: https://github.com/tursodatabase/turso/issues/6879
CREATE TABLE t(id INTEGER PRIMARY KEY, x INT);
INSERT INTO t VALUES(1,10),(2,20);
BEGIN;
UPDATE t SET x = CASE WHEN id=1 THEN 11 ELSE (char(120) LIKE char(120) ESCAPE (char(121)||char(121))) END;
SELECT id,x FROM t ORDER BY id;
COMMIT;
SELECT id,x FROM t ORDER BY id;

-- case: research/turso/trigger_writes_sqlite_sequence | source: https://github.com/tursodatabase/turso/issues/6939
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES('seed');
CREATE TRIGGER bi BEFORE INSERT ON t BEGIN INSERT INTO sqlite_sequence(name, seq) VALUES('t', 5); UPDATE sqlite_sequence SET seq = 4 WHERE name = 't'; END;
INSERT INTO t(v) VALUES('x');
SELECT id, v FROM t ORDER BY id;
SELECT rowid, name, seq FROM sqlite_sequence ORDER BY rowid;

-- case: research/turso/left_join_on_term_with_partial_index | source: https://github.com/tursodatabase/turso/issues/6961
CREATE TABLE t(id INTEGER PRIMARY KEY, c INT, x INT);
CREATE TABLE u(x INT);
CREATE TABLE dst(id INT, c INT, x INT, ux INT);
INSERT INTO t VALUES (1,0,10);
CREATE INDEX idx_t_x_c1 ON t(x) WHERE c = 1;
INSERT INTO dst SELECT t.id, t.c, t.x, u.x FROM t LEFT JOIN u ON t.c = 1 AND u.x = t.x WHERE t.x IN (10);
SELECT * FROM dst ORDER BY id;

-- case: research/turso/quoted_integer_type_autoincrement | source: https://github.com/tursodatabase/turso/issues/6964
CREATE TABLE t(a "INTEGER" PRIMARY KEY AUTOINCREMENT);
INSERT INTO t DEFAULT VALUES;
INSERT INTO t DEFAULT VALUES;
SELECT rowid, a FROM t;

-- case: research/turso/autoincrement_sequence_row_with_null_name | source: https://github.com/tursodatabase/turso/issues/6966
CREATE TABLE t1(i INTEGER PRIMARY KEY AUTOINCREMENT);
CREATE TABLE t2(i INTEGER PRIMARY KEY AUTOINCREMENT);
CREATE TABLE t3(i INTEGER PRIMARY KEY AUTOINCREMENT);
INSERT INTO t1 VALUES(NULL);
INSERT INTO t2 VALUES(NULL);
INSERT INTO t3 VALUES(NULL);
UPDATE sqlite_sequence SET name=NULL WHERE name='t2';
INSERT INTO t3 VALUES(NULL);
SELECT rowid, name, seq FROM sqlite_sequence ORDER BY rowid;

-- case: research/turso/generated_column_in_outer_join_null_extension | source: https://github.com/tursodatabase/turso/issues/7064
CREATE TABLE l(x);
CREATE TABLE r(a, g AS (1));
INSERT INTO l VALUES(1);
SELECT x,a,g,typeof(g) FROM l LEFT JOIN r ON false;

-- case: research/turso/generated_column_index_seek_affinity | source: https://github.com/tursodatabase/turso/issues/7065
CREATE TABLE t(a TEXT, g INT AS(a));
CREATE INDEX i ON t(g);
INSERT INTO t(a) VALUES('1'),('2');
SELECT a,g FROM t WHERE g=1;
DELETE FROM t WHERE g=1;
SELECT a,g FROM t;

-- case: research/turso/generated_column_row_value_expression | source: https://github.com/tursodatabase/turso/issues/7066
CREATE TABLE t(a, b AS ((a,1)<(a,2)));
INSERT INTO t(a) VALUES(5);
SELECT * FROM t;

-- case: research/turso/generated_unique_with_replace_default | source: https://github.com/tursodatabase/turso/issues/7068
CREATE TABLE t(a NOT NULL DEFAULT 5, b AS(a+1) UNIQUE);
REPLACE INTO t(a) VALUES(NULL) RETURNING a,b;
SELECT rowid,a,b FROM t;
PRAGMA integrity_check;

-- case: research/turso/generated_unique_index_uses_column_affinity | source: https://github.com/tursodatabase/turso/issues/7069
CREATE TABLE t(a TEXT, g INT AS(a));
CREATE UNIQUE INDEX i ON t(g);
INSERT INTO t(a) VALUES('1');
INSERT INTO t(a) VALUES('01');
SELECT a, g FROM t;

-- case: research/turso/generated_column_upsert_unique_index_target | source: https://github.com/tursodatabase/turso/issues/7072
CREATE TABLE t(a, b AS(a*2));
CREATE UNIQUE INDEX idx_b ON t(b);
INSERT INTO t(a) VALUES(1);
INSERT INTO t(a) VALUES(1) ON CONFLICT(b) DO UPDATE SET a=2;
SELECT rowid,a,b FROM t;

-- case: research/turso/generated_child_fk_blocks_parent_delete | source: https://github.com/tursodatabase/turso/issues/7074
PRAGMA foreign_keys=ON;
CREATE TABLE p(id PRIMARY KEY);
CREATE TABLE c(a, g AS(a) REFERENCES p(id));
INSERT INTO p VALUES(1);
INSERT INTO c(a) VALUES(1);
DELETE FROM p;
SELECT * FROM c;
SELECT * FROM p;

-- case: research/turso/self_ref_fk_different_affinity | source: https://github.com/tursodatabase/turso/issues/7251
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER UNIQUE, pk TEXT REFERENCES t(k));
INSERT INTO t(id, k, pk) VALUES (1, 1, '1');
SELECT typeof(k), typeof(pk), k, pk FROM t;

-- case: research/turso/table_pk_with_collate | source: https://github.com/tursodatabase/turso/issues/7296
CREATE TABLE p(a TEXT, PRIMARY KEY(a COLLATE NOCASE));
INSERT INTO p VALUES ('x');
INSERT INTO p VALUES ('X');
SELECT * FROM p;

-- case: research/turso/between_collation_from_left_operand | source: https://github.com/tursodatabase/turso/issues/7326
CREATE TABLE t(a COLLATE NOCASE, b);
INSERT INTO t VALUES ('ABC','abc');
SELECT a, b FROM t WHERE a BETWEEN b AND b;
SELECT a BETWEEN b AND b, b BETWEEN a AND a FROM t;

-- case: research/turso/aggregate_bare_column_from_joined_table_empty | source: https://github.com/tursodatabase/turso/issues/7351
CREATE TABLE t (a);
SELECT y.a, COUNT(*) FROM t AS x JOIN t AS y ON x.a = y.a;

-- case: research/turso/left_join_derived_table_or_false | source: https://github.com/tursodatabase/turso/issues/7362
CREATE TABLE l(k INTEGER);
CREATE TABLE rr(k INTEGER);
INSERT INTO l VALUES (0),(1);
INSERT INTO rr VALUES (0),(1);
SELECT a.k, d.k FROM l AS a LEFT JOIN (SELECT b.k FROM l AS b JOIN rr AS c ON b.k = c.k) AS d ON (a.k >= d.k) OR 0 ORDER BY a.k, d.k;

-- case: research/turso/left_join_derived_table_not_le | source: https://github.com/tursodatabase/turso/issues/7363
CREATE TABLE l(k INTEGER);
CREATE TABLE a(id INTEGER, r INTEGER);
CREATE TABLE b(r INTEGER);
INSERT INTO l VALUES (1),(2);
INSERT INTO a VALUES (1,0),(2,1);
INSERT INTO b VALUES (0),(1);
SELECT l.k, d.id FROM l LEFT JOIN (SELECT a.id FROM a JOIN b ON a.r = b.r) AS d ON NOT (l.k <= d.id) ORDER BY l.k, d.id;

-- case: research/turso/join_or_lt_eq_over_derived_tables | source: https://github.com/tursodatabase/turso/issues/7364
CREATE TABLE l(id INTEGER);
CREATE TABLE r(k INTEGER);
CREATE TABLE a(k INTEGER);
INSERT INTO l VALUES (1), (2);
INSERT INTO r VALUES (2);
INSERT INTO a VALUES (2);
SELECT l.id FROM (SELECT DISTINCT id FROM l) AS l JOIN (SELECT r.k FROM r JOIN a ON r.k = a.k) AS r ON (l.id < r.k) OR (l.id = r.k) ORDER BY 1;

-- case: research/turso/join_not_ge_over_derived_tables | source: https://github.com/tursodatabase/turso/issues/7365
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(y INTEGER);
CREATE TABLE ra(y INTEGER);
INSERT INTO l VALUES (1), (2);
INSERT INTO la VALUES (1), (2);
INSERT INTO r VALUES (3);
INSERT INTO ra VALUES (3);
SELECT l.x FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l JOIN (SELECT r.y FROM r JOIN ra ON r.y = ra.y) AS r ON NOT (l.x >= r.y) ORDER BY 1;

-- case: research/turso/right_join_coalesce_on | source: https://github.com/tursodatabase/turso/issues/7366
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(id INTEGER, y INTEGER);
INSERT INTO l VALUES (1);
INSERT INTO la VALUES (1);
INSERT INTO r VALUES (10, 2), (11, 3);
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON COALESCE((l.x <= r.y), 0) WHERE l.x IS NOT NULL ORDER BY 1, 2;

-- case: research/turso/right_join_correlated_subquery_on | source: https://github.com/tursodatabase/turso/issues/7368
CREATE TABLE l(x INTEGER);
CREATE TABLE la(x INTEGER);
CREATE TABLE r(id INTEGER, x INTEGER);
INSERT INTO l VALUES (1);
INSERT INTO la VALUES (1);
INSERT INTO r VALUES (10, 1), (11, 1);
SELECT l.x, r.id FROM (SELECT l.x FROM l JOIN la ON l.x = la.x) AS l RIGHT JOIN r ON 1 IN (SELECT 1 WHERE l.x = r.x) WHERE l.x IS NOT NULL ORDER BY 1, 2;

-- case: research/turso/cross_join_derived_join_count | source: https://github.com/tursodatabase/turso/issues/7370
CREATE TABLE l(x INTEGER);
CREATE TABLE r(y INTEGER);
CREATE TABLE a(y INTEGER);
INSERT INTO l VALUES (1),(2);
INSERT INTO r VALUES (10),(20);
INSERT INTO a VALUES (10),(20);
SELECT COUNT(*) FROM l CROSS JOIN (SELECT r.y FROM r JOIN a ON r.y = a.y) AS d;

-- case: research/turso/natural_join_quoted_hyphen_column | source: https://github.com/tursodatabase/turso/issues/7371
CREATE TABLE t("NUMERIC-ish" NUMERIC);
INSERT INTO t VALUES (1);
SELECT * FROM t AS a NATURAL JOIN t AS b;

-- case: research/turso/derived_join_text_le_integer_affinity | source: https://github.com/tursodatabase/turso/issues/7373
CREATE TABLE l(txt TEXT);
CREATE TABLE r(flag INTEGER);
INSERT INTO l VALUES ('');
INSERT INTO r VALUES (0);
SELECT 'inner', L.txt, R.flag FROM (SELECT txt FROM l) AS L JOIN r AS R ON L.txt <= R.flag
UNION ALL
SELECT 'cross', L.txt, R.flag FROM (SELECT txt FROM l) AS L CROSS JOIN r AS R WHERE L.txt <= R.flag
ORDER BY 1, 2, 3;

-- case: research/turso/right_join_mixed_affinity_boolean | source: https://github.com/tursodatabase/turso/issues/7374
CREATE TABLE l(id INTEGER PRIMARY KEY, txt TEXT);
CREATE TABLE r(id INTEGER PRIMARY KEY, flag BOOLEAN);
INSERT INTO l VALUES (1, '6');
INSERT INTO r VALUES (10, 0);
SELECT l.id, l.txt, typeof(l.txt), r.id, r.flag, typeof(r.flag) FROM (SELECT id, txt FROM l WHERE txt IS NOT NULL) AS l RIGHT JOIN (SELECT DISTINCT id, flag FROM r) AS r ON l.txt >= r.flag;

-- case: research/turso/left_join_materialized_subquery_collation | source: https://github.com/tursodatabase/turso/issues/7393
CREATE TABLE l(id INTEGER, txt TEXT);
CREATE TABLE r(id INTEGER, txt TEXT COLLATE NOCASE);
INSERT INTO l VALUES (2, 'a');
INSERT INTO r VALUES (11, 'A');
SELECT 'anti', l.id FROM l WHERE NOT EXISTS (SELECT 1 FROM r WHERE l.txt > r.txt)
UNION ALL
SELECT 'left', l.id FROM l LEFT JOIN (SELECT r.*, 1 AS marker FROM r) AS r ON l.txt > r.txt WHERE r.marker IS NULL
ORDER BY 1, 2;

-- case: research/turso/add_column_generated_virtual_reopen | source: https://github.com/tursodatabase/turso/issues/7418
CREATE TABLE t(a);
INSERT INTO t VALUES (3);
ALTER TABLE t ADD COLUMN b AS (a);
SELECT * FROM t;

-- case: research/turso/rename_with_unrelated_trigger_cte | source: https://github.com/tursodatabase/turso/issues/7641
CREATE TABLE a(x);
CREATE TABLE b(y);
CREATE TABLE c(z);
CREATE TRIGGER trg AFTER INSERT ON a BEGIN INSERT INTO b WITH cte_0 AS (SELECT 1 AS y) SELECT y FROM cte_0; END;
ALTER TABLE c RENAME TO c2;
SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;

-- case: research/turso/rename_column_with_temp_trigger | source: https://github.com/tursodatabase/turso/issues/7644
CREATE TABLE t(a, b);
CREATE TEMP TRIGGER trg BEFORE UPDATE ON t BEGIN SELECT 1; END;
ALTER TABLE t RENAME COLUMN a TO a2;
SELECT group_concat(name) FROM pragma_table_info('t');

-- case: research/turso/pragma_table_info_pk_position | source: https://github.com/tursodatabase/turso/issues/7647
CREATE TABLE t(a, b, c, d, PRIMARY KEY(c, a, d));
SELECT name, pk FROM pragma_table_info('t');

-- case: research/turso/explicit_collate_leaks_into_later_in | source: https://github.com/tursodatabase/turso/issues/7649
SELECT 'A' COLLATE NOCASE IN ('a'), 'A' IN ('a');
SELECT 'A' IN ('a'), 'A' COLLATE NOCASE IN ('a'), 'A' = 'a';

-- case: research/turso/json_valid_strict_rfc | source: https://github.com/tursodatabase/turso/issues/7650
SELECT json_valid('[1,2,]'), json_valid('{"a":1,}'), json_valid('{a:1}'), json_valid('[1,2]//c'), json_valid('[1,2]'), json_valid("{'a':1}"), json_valid('[01]'), json_valid('[1.]'), json_valid('[.5]'), json_valid('[+1]'), json_valid('[0x10]'), json_valid('[NaN]');
SELECT json_valid('{"a":1}', 1), json_valid('{a:1}', 1), json_valid('{a:1}', 2), json_valid('{a:1}', 4), json_valid('{a:1}', 8);

-- case: research/turso/json_group_object_numeric_label | source: https://github.com/tursodatabase/turso/issues/7757
SELECT json_group_object(1, 2);
CREATE TABLE t(c1 INT, c2 INT);
INSERT INTO t VALUES (1, 2);
SELECT json_group_object(c1, c2) FROM t;
SELECT json_object(1, 2), json_object('a', NULL), json_object(1.5, 'x');

-- case: research/turso/join_on_references_later_table | source: https://github.com/tursodatabase/turso/issues/7765
CREATE TABLE a(x INT);
CREATE TABLE b(y INT);
CREATE TABLE c(z INT);
INSERT INTO a VALUES (1);
INSERT INTO b VALUES (1);
INSERT INTO c VALUES (1);
SELECT * FROM a JOIN b ON a.x = c.z JOIN c ON b.y = c.z;

-- case: research/turso/replace_delete_trigger_reinserts_key | source: https://github.com/tursodatabase/turso/issues/7975
PRAGMA recursive_triggers = true;
PRAGMA foreign_keys = 1;
CREATE TABLE p1 (a, b UNIQUE);
CREATE TABLE c1 (c, d REFERENCES p1(b) ON DELETE CASCADE);
CREATE TRIGGER tr6 AFTER DELETE ON c1 BEGIN INSERT INTO p1 VALUES(4, 1); END;
INSERT INTO p1 VALUES(1, 1);
INSERT INTO c1 VALUES(2, 1);
REPLACE INTO p1 VALUES(3, 1);
SELECT * FROM p1;
PRAGMA integrity_check;

-- case: research/turso/scalar_subquery_multirow_values | source: https://github.com/tursodatabase/turso/issues/7976
SELECT (VALUES(1),(2),(3));
SELECT (SELECT 5 UNION ALL SELECT 6), (SELECT 1 WHERE 0);

-- case: research/turso/having_no_group_by_bare_column | source: https://github.com/tursodatabase/turso/issues/7977
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3),(4),(5);
SELECT count(*) FROM t HAVING a!=400;
SELECT count(*) FROM t HAVING a=1;

-- case: research/turso/right_full_join_extra_table_and_view | source: https://github.com/tursodatabase/turso/issues/7978
CREATE TABLE t0(a INT); INSERT INTO t0(a) VALUES (1);
CREATE TABLE t1(b INT); INSERT INTO t1(b) VALUES (2);
CREATE VIEW v2(c) AS SELECT 3 FROM t1;
SELECT * FROM t1 JOIN v2 ON 0 FULL OUTER JOIN t0 ON true;

-- case: research/turso/sixty_four_way_self_join | source: https://github.com/tursodatabase/turso/issues/7979
CREATE TABLE t14(x);
INSERT INTO t14 VALUES('abcdefghij');
SELECT count(*) FROM t14 a1, t14 a2, t14 a3, t14 a4, t14 a5, t14 a6, t14 a7, t14 a8, t14 a9, t14 a10, t14 a11, t14 a12, t14 a13, t14 a14, t14 a15, t14 a16, t14 a17, t14 a18, t14 a19, t14 a20, t14 a21, t14 a22, t14 a23, t14 a24, t14 a25, t14 a26, t14 a27, t14 a28, t14 a29, t14 a30, t14 a31, t14 a32, t14 a33, t14 a34, t14 a35, t14 a36, t14 a37, t14 a38, t14 a39, t14 a40, t14 a41, t14 a42, t14 a43, t14 a44, t14 a45, t14 a46, t14 a47, t14 a48, t14 a49, t14 a50, t14 a51, t14 a52, t14 a53, t14 a54, t14 a55, t14 a56, t14 a57, t14 a58, t14 a59, t14 a60, t14 a61, t14 a62, t14 a63;

-- case: research/turso/autoincrement_hides_internal_tables | source: https://github.com/tursodatabase/turso/issues/7980
CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO t(x) VALUES (1);
SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;

-- case: research/turso/compound_select_many_terms_stack | source: https://github.com/tursodatabase/turso/issues/8104
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 600)
SELECT count(*) FROM n;
SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9 UNION ALL SELECT 10 UNION ALL SELECT 11 UNION ALL SELECT 12 UNION ALL SELECT 13 UNION ALL SELECT 14 UNION ALL SELECT 15 UNION ALL SELECT 16 UNION ALL SELECT 17 UNION ALL SELECT 18 UNION ALL SELECT 19 UNION ALL SELECT 20;

-- case: research/turso/full_join_where_on_build_side | source: https://github.com/tursodatabase/turso/issues/8220
CREATE TABLE a(id INTEGER, x INTEGER);
CREATE TABLE b(id INTEGER, y INTEGER);
CREATE TABLE c(id INTEGER, z INTEGER);
INSERT INTO a VALUES (1,1),(2,2);
INSERT INTO b VALUES (1,1),(2,2);
INSERT INTO c VALUES (2,2),(3,3);
SELECT a.id, c.id FROM a JOIN b ON a.id = b.id FULL JOIN c ON b.id = c.id WHERE a.x = 1 ORDER BY 1, 2;

-- case: research/turso/having_subquery_bare_column_group_by | source: https://github.com/tursodatabase/turso/issues/8221
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES (1,10),(2,20);
SELECT a FROM t GROUP BY a HAVING (SELECT b) = 20;

-- case: research/turso/correlated_exists_in_fromless_subquery | source: https://github.com/tursodatabase/turso/issues/8222
CREATE TABLE o(k);
CREATE TABLE i(k);
INSERT INTO o VALUES (1),(2);
INSERT INTO i VALUES (1);
SELECT o.k, (SELECT 1 WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k)) FROM o ORDER BY o.k;
SELECT o.k, (SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM i WHERE i.k = o.k)) FROM o ORDER BY o.k;

-- case: research/turso/correlated_exists_empty_inner_table | source: https://github.com/tursodatabase/turso/issues/8224
CREATE TABLE o(k);
CREATE TABLE i(k);
INSERT INTO o VALUES (1);
SELECT * FROM o WHERE EXISTS (SELECT 1 FROM i WHERE i.k = o.k);
SELECT * FROM o WHERE NOT EXISTS (SELECT 1 FROM i WHERE i.k = o.k);

-- case: research/turso/case_when_comparison_affinity | source: https://github.com/tursodatabase/turso/issues/8225
CREATE TABLE o(t TEXT, n INTEGER, r REAL);
INSERT INTO o VALUES ('5', 5, 5.0);
SELECT CASE o.t WHEN 5 THEN 'y' ELSE 'n' END FROM o;
SELECT CASE o.n WHEN '5' THEN 'y' ELSE 'n' END FROM o;
SELECT CASE o.r WHEN '5' THEN 'y' ELSE 'n' END FROM o;
SELECT CASE 5 WHEN o.t THEN 'y' ELSE 'n' END FROM o;

-- case: research/turso/hash_join_correlated_subquery_in_select | source: https://github.com/tursodatabase/turso/issues/8226
CREATE TABLE a(id INTEGER, k INTEGER);
CREATE TABLE b(id INTEGER, k INTEGER, j INTEGER);
CREATE TABLE c(id INTEGER, j INTEGER, k INTEGER);
CREATE TABLE i(k INTEGER, x INTEGER);
INSERT INTO a VALUES (1,1);
INSERT INTO b VALUES (1,1,1);
INSERT INTO c VALUES (1,1,2);
INSERT INTO i VALUES (2,10),(2,3);
SELECT a.id, (SELECT sum(i.x) FROM i WHERE i.k = c.k) FROM a, b, c WHERE a.k = b.k AND b.j = c.j;

-- case: research/turso/left_join_not_in_empty_subquery | source: https://github.com/tursodatabase/turso/issues/8227
CREATE TABLE pa(id INTEGER);
CREATE TABLE pb(aid INTEGER, x INTEGER);
CREATE TABLE z(a INTEGER);
INSERT INTO pa VALUES (1),(2);
INSERT INTO pb VALUES (1,10);
SELECT pa.id FROM pa LEFT JOIN pb ON pa.id = pb.aid WHERE pb.x NOT IN (SELECT a FROM z) ORDER BY 1;

-- case: research/turso/full_join_three_table_chain_unmatched | source: https://github.com/tursodatabase/turso/issues/8228
CREATE TABLE a(k INTEGER);
CREATE TABLE b(z INTEGER);
CREATE TABLE c(y INTEGER);
INSERT INTO a VALUES (7);
INSERT INTO b VALUES (100);
INSERT INTO c VALUES (500);
SELECT a.k, b.z, c.y FROM a CROSS JOIN b FULL JOIN c ON c.y = a.k ORDER BY 1, 2, 3;

-- case: research/turso/correlated_subquery_having_no_group_by | source: https://github.com/tursodatabase/turso/issues/8229
CREATE TABLE t(a);
INSERT INTO t VALUES (1),(2),(3);
SELECT count(*) FROM t HAVING (SELECT t.a) IS NOT NULL;

-- case: research/turso/left_join_where_via_correlated_subquery | source: https://github.com/tursodatabase/turso/issues/8230
CREATE TABLE o(a, c);
CREATE TABLE p(a);
CREATE TABLE i(b);
INSERT INTO o VALUES (1, 5);
INSERT INTO p VALUES (2);
SELECT o.a FROM o LEFT JOIN p ON o.a = p.a WHERE o.c > (SELECT count(*) FROM i WHERE i.b = p.a);

-- case: research/turso/left_join_where_coalesce_spans_tables | source: https://github.com/tursodatabase/turso/issues/8231
CREATE TABLE o(a);
CREATE TABLE p(a, b);
INSERT INTO o VALUES (0);
INSERT INTO p VALUES (2, 2);
SELECT o.a, p.a, p.b FROM o LEFT JOIN p ON o.a = p.a WHERE coalesce(p.b, 1) = o.a;

-- case: research/turso/left_join_where_typeof_null | source: https://github.com/tursodatabase/turso/issues/8236
CREATE TABLE l(a);
CREATE TABLE r(a, b);
INSERT INTO l VALUES (1);
INSERT INTO r VALUES (2, 7);
SELECT l.a, r.b FROM l LEFT JOIN r ON l.a = r.a WHERE typeof(r.b) = 'null';

-- case: research/turso/parenthesized_join_in_from | source: https://github.com/tursodatabase/turso/issues/8237
CREATE TABLE t1(a); CREATE TABLE t2(a); CREATE TABLE t3(a);
INSERT INTO t1 VALUES (1); INSERT INTO t2 VALUES (1); INSERT INTO t3 VALUES (1);
SELECT * FROM t1 JOIN (t2 JOIN t3 ON t2.a = t3.a) ON t1.a = t2.a;

-- case: research/turso/on_clause_correlated_subquery_outer_table | source: https://github.com/tursodatabase/turso/issues/8240
CREATE TABLE t1(a); CREATE TABLE t2(a, b); CREATE TABLE t3(a); CREATE TABLE s(k);
INSERT INTO t1 VALUES (2); INSERT INTO t2 VALUES (2, 2);
INSERT INTO t3 VALUES (2); INSERT INTO s VALUES (2);
SELECT t1.a, t2.a, t3.a FROM t1 JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a AND EXISTS (SELECT 1 FROM s WHERE s.k = t1.a);

-- case: research/turso/chained_left_joins_not_exists_where | source: https://github.com/tursodatabase/turso/issues/8241
CREATE TABLE t1(a); CREATE TABLE t2(a, b); CREATE TABLE t3(a); CREATE TABLE s(k);
INSERT INTO t1 VALUES (1); INSERT INTO t2 VALUES (9, 9);
INSERT INTO t3 VALUES (9); INSERT INTO s VALUES (9);
SELECT t1.a, t2.a, t3.a FROM t1 LEFT JOIN t2 ON t1.a = t2.a LEFT JOIN t3 ON t2.b = t3.a WHERE NOT EXISTS (SELECT 1 FROM s WHERE s.k = t3.a);

-- case: research/turso/self_join_on_generated_columns | source: https://github.com/tursodatabase/turso/issues/8252
CREATE TABLE g(a INTEGER, v1 AS (a + 1) VIRTUAL, s1 AS (a * 2) VIRTUAL);
INSERT INTO g(a) VALUES (1), (2);
SELECT g1.a, g2.a FROM g g1 JOIN g g2 ON g1.v1 = g2.s1 ORDER BY 1, 2;

-- case: research/turso/sqlite_master_name_case | source: https://github.com/tursodatabase/turso/issues/8253
CREATE TABLE "My Tbl"(a);
SELECT name, tbl_name, sql FROM sqlite_master;
SELECT count(*) FROM sqlite_master WHERE name = 'My Tbl';

-- case: research/turso/alter_add_column_keeps_collate_in_schema | source: https://github.com/tursodatabase/turso/issues/8254
CREATE TABLE tbl(col1 TEXT COLLATE NOCASE);
INSERT INTO tbl VALUES ('ABC');
ALTER TABLE tbl ADD COLUMN extra TEXT;
SELECT sql FROM sqlite_master WHERE name = 'tbl';
SELECT count(*) FROM tbl WHERE col1 = 'abc';

-- case: research/turso/row_value_in_null_three_valued | source: https://github.com/tursodatabase/turso/issues/8255
CREATE TABLE w(a, b);
INSERT INTO w VALUES (NULL,1),(1,1),(2,NULL),(2,2);
SELECT a, b FROM w WHERE (a,b) IN ((1,1),(2,2)) ORDER BY 1,2;
SELECT a, b FROM w WHERE (a,b) NOT IN ((1,1),(2,2)) ORDER BY 1,2;
SELECT a, b, (a,b) IN ((1,1),(2,2)), (a,b) NOT IN ((1,1),(2,2)) FROM w ORDER BY 1,2;

-- case: research/turso/abs_int64_min_overflow_then_next | source: https://github.com/tursodatabase/turso/issues/8256
SELECT abs(-9223372036854775808);
SELECT 'after';

-- case: research/turso/compound_collation_left_arm_binary | source: https://github.com/tursodatabase/turso/issues/8272
CREATE TABLE q(x, y TEXT); INSERT INTO q VALUES(1,'a'),(2,'B'),(NULL,'c'),('3','z');
CREATE TABLE p(x, y TEXT COLLATE NOCASE); INSERT INTO p VALUES(1,'A'),(2,'b'),(NULL,'C'),(3,NULL),(2,'B'),(1.0,'a');
SELECT y FROM q INTERSECT SELECT y FROM p ORDER BY 1;
SELECT y FROM q UNION SELECT y FROM p ORDER BY 1;
SELECT y FROM q EXCEPT SELECT y FROM p ORDER BY 1;

-- case: research/turso/window_lag_sum_repeated | source: https://github.com/tursodatabase/turso/issues/8336
CREATE TABLE t (d TEXT, x INT);
INSERT INTO t VALUES ('2026-01-01',1),('2026-02-02',2),('2026-03-03',3);
SELECT strftime('%Y-%m', d) AS k, AVG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d)) AS running_avg, 100.0 * (SUM(x) - LAG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d))) / LAG(SUM(x)) OVER (ORDER BY strftime('%Y-%m', d)) AS pct_change FROM t GROUP BY 1;

-- case: research/turso/compound_cast_arm_integer_above_2_53 | source: https://github.com/tursodatabase/turso/issues/8368
CREATE TABLE t(i INTEGER);
INSERT INTO t VALUES(9007199254740993);
SELECT typeof(v), quote(v) FROM (SELECT CAST(i AS REAL) AS v FROM t UNION ALL SELECT i FROM t);

-- case: research/turso/json_each_decoded_string_values | source: https://github.com/tursodatabase/turso/issues/8420
SELECT key, value, hex(value), length(value) FROM json_each('["plain","with \"quotes\"","back\\slash","new\nline"]');

-- case: research/turso/index_seek_with_null_key | source: https://github.com/tursodatabase/turso/issues/8690
CREATE TABLE t3(c);
CREATE INDEX i3 ON t3(c);
INSERT INTO t3 VALUES (1),(2),(3);
SELECT count(*) FROM t3 WHERE c > ('{"k":1}' ->> '$.missing');
DELETE FROM t3 WHERE c <= ('{"k":1}' ->> '$.missing');
SELECT count(*) FROM t3;

-- case: research/turso/pragma_foreign_keys_inside_transaction | source: https://github.com/tursodatabase/turso/issues/8468
PRAGMA foreign_keys=OFF;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(10,1);
BEGIN;
PRAGMA foreign_keys=ON;
DELETE FROM p WHERE id=1;
COMMIT;
SELECT count(*) FROM c;

-- case: research/turso/update_multi_index_or_scan | source: https://github.com/tursodatabase/turso/issues/8510
CREATE TABLE t (a INTEGER, b INTEGER, c INTEGER);
CREATE INDEX ia ON t(a);
CREATE INDEX ib ON t(b);
INSERT INTO t VALUES (1, 2, 0), (5, 2, 0), (1, 9, 0), (7, 7, 0);
UPDATE t SET c = 8 WHERE a = 1 OR b = 2;
SELECT * FROM t ORDER BY a, b;

-- case: research/turso/scalar_subquery_shadowed_table_resolution | source: https://github.com/tursodatabase/turso/issues/8950
SELECT (SELECT (SELECT t.y) FROM (SELECT 2 AS y) AS t) AS from_inner FROM (SELECT 1 AS x) AS t;

-- case: research/turso/or_skips_error_operand | source: https://github.com/tursodatabase/turso/issues/9038
SELECT 1 OR json_extract('bare','$.a');
SELECT 0 AND json_extract('bare','$.a');
SELECT CASE WHEN (SELECT abs(-9223372036854775807 - 1)) AND 0 THEN 'y' ELSE 'n' END;

-- case: research/turso/jsonb_extract_returns_blob | source: https://github.com/tursodatabase/turso/issues/9039
SELECT typeof(jsonb_extract(jsonb('{"a":[1]}'), '$.a')), typeof(jsonb_extract(jsonb('{"a":{"b":1}}'), '$.a')), typeof(jsonb_extract(jsonb('{"a":1}'), '$.a')), typeof(jsonb_extract(jsonb('{"a":"x"}'), '$.a'));

-- case: research/turso/in_operator_left_operand_collation | source: https://github.com/tursodatabase/turso/issues/9040
CREATE TABLE r(value TEXT COLLATE NOCASE);
INSERT INTO r VALUES('a');
SELECT 'A' IN (value), 'A' IN ('a' COLLATE NOCASE, 'x'), ('A' COLLATE NOCASE) IN (value COLLATE BINARY) FROM r;

-- case: research/turso/row_value_compare_collation_and_affinity | source: https://github.com/tursodatabase/turso/issues/9041
CREATE TABLE c(a TEXT COLLATE NOCASE);
INSERT INTO c VALUES('A');
SELECT (SELECT a, 1 FROM c) = ('a', 1);
SELECT ('a', 1) IN (('A', 1), ('x' COLLATE NOCASE, 2));
SELECT (1, 2) IN ((CAST('1' AS TEXT), 2), (CAST('x' AS BLOB), 3));

-- case: research/turso/case_x_when_y_uses_x_collation | source: https://github.com/tursodatabase/turso/issues/9042
CREATE TABLE cs(l TEXT, r TEXT COLLATE NOCASE);
INSERT INTO cs VALUES('a', 'A');
SELECT l = r, CASE l WHEN r THEN 1 ELSE 0 END, CASE r WHEN l THEN 1 ELSE 0 END FROM cs;

-- case: research/turso/schema_qualified_column_in_where_join | source: https://github.com/tursodatabase/turso/issues/9043
CREATE TABLE a(id INTEGER, x INTEGER);
CREATE TABLE b(id INTEGER, y INTEGER);
INSERT INTO a VALUES (1,10),(2,20);
INSERT INTO b VALUES (1,100),(2,200),(3,300);
SELECT a.id, b.y FROM a JOIN b ON a.id = b.id WHERE main.a.x > 5 ORDER BY 1;

-- case: research/turso/view_ignores_temp_table_with_same_name | source: https://github.com/tursodatabase/turso/issues/9044
CREATE TABLE base(value INTEGER);
INSERT INTO base VALUES(1);
CREATE VIEW ov AS SELECT value FROM base;
CREATE TEMP TABLE base(value INTEGER);
INSERT INTO temp.base VALUES(2);
SELECT * FROM ov;

-- case: research/turso/float_literal_large_negative_exponent | source: https://github.com/tursodatabase/turso/issues/9045
SELECT 1e-300, 1e-300 * 1e300, 1e-320, 4.9e-324, 1.7976931348623157e308, 123456789012345678e-30;
SELECT 0.1 + 0.2, 1e15 + 0.3, 1e16, 1e-5, 100.0, 1e100, -0.0, 5e-324 / 2;

-- case: research/turso/sum_text_integers_overflow_to_float | source: https://github.com/tursodatabase/turso/issues/9046
CREATE TABLE p(price);
INSERT INTO p VALUES ('9223372036854775807'), ('1'), ('0.5');
SELECT sum(price) FROM p;
CREATE TABLE q(v);
INSERT INTO q VALUES (9223372036854775807), (1);
SELECT sum(v) FROM q;

-- case: research/turso/rename_table_leaves_same_named_trigger | source: https://github.com/tursodatabase/turso/issues/9048
CREATE TABLE ft(a INTEGER);
CREATE TABLE ts(a INTEGER);
CREATE TRIGGER ts AFTER INSERT ON ft BEGIN SELECT 1; END;
ALTER TABLE ts RENAME TO ts2;
SELECT type, name FROM sqlite_master WHERE type='trigger';

-- case: research/turso/rename_column_then_drop_with_fk_clause | source: https://github.com/tursodatabase/turso/issues/9049
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE c(parent_id INTEGER REFERENCES parent(id), keep INTEGER);
ALTER TABLE c RENAME COLUMN parent_id TO renamed_parent_id;
ALTER TABLE c DROP COLUMN renamed_parent_id;
SELECT sql FROM sqlite_master WHERE name = 'c';

-- case: research/turso/multirow_insert_self_referencing_fk | source: https://github.com/tursodatabase/turso/issues/9050
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent_id REFERENCES n(id));
INSERT INTO n VALUES (1, 2), (2, NULL);
SELECT * FROM n ORDER BY id;

-- case: research/turso/row_value_between | source: https://github.com/tursodatabase/turso/issues/9051
SELECT (1,2) BETWEEN (1,1) AND (1,3);

-- case: research/turso/join_without_rowid_table | source: https://github.com/tursodatabase/turso/issues/9052
CREATE TABLE w(v INTEGER, k TEXT PRIMARY KEY) WITHOUT ROWID;
CREATE TABLE other(k TEXT, x INTEGER);
INSERT INTO w VALUES (10, 'a');
INSERT INTO other VALUES ('a', 100);
SELECT w.v, other.x FROM w JOIN other ON w.k = other.k;

-- case: research/turso/nested_cte_shadows_outer_cte_name | source: https://github.com/tursodatabase/turso/issues/9055
WITH q AS (WITH a(v) AS (VALUES(4)) SELECT v FROM a), a AS (SELECT v FROM q) SELECT v FROM a;

-- case: research/turso/correlated_column_in_subquery_group_by | source: https://github.com/tursodatabase/turso/issues/9056
CREATE TABLE o(x);
CREATE TABLE i(y);
INSERT INTO o VALUES (1);
INSERT INTO i VALUES (1),(2);
SELECT (SELECT count(*) FROM i GROUP BY o.x) FROM o;

-- case: research/turso/pragma_function_in_scalar_subquery | source: https://github.com/tursodatabase/turso/issues/9057
CREATE TABLE t(a INTEGER PRIMARY KEY, x INTEGER);
SELECT name FROM pragma_table_info('t') WHERE cid = (SELECT cid FROM pragma_table_info('t') WHERE pk = 1);

-- case: research/turso/indexed_by_partial_index_uses_where | source: https://github.com/tursodatabase/turso/issues/9059
CREATE TABLE t(a, b);
CREATE INDEX ni ON t(a) WHERE b IS NOT NULL;
INSERT INTO t VALUES (1, 5), (2, NULL);
SELECT a FROM t INDEXED BY ni WHERE b = 5;
CREATE INDEX pi ON t(a) WHERE b > 0;
DELETE FROM t INDEXED BY pi;
SELECT count(*) FROM t;

-- case: research/turso/filter_on_scalar_function | source: https://github.com/tursodatabase/turso/issues/9062
CREATE TABLE t(a INT);
SELECT abs(a) FILTER (WHERE a > 0) FROM t;

-- case: research/turso/add_column_nonconstant_default | source: https://github.com/tursodatabase/turso/issues/9063
CREATE TABLE t(a INTEGER);
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT (a + 1);
SELECT count(*) FROM pragma_table_info('t');

-- case: research/turso/numeric_literal_misplaced_underscore | source: https://github.com/tursodatabase/turso/issues/9064
SELECT 1._5;
SELECT 1e_2;
SELECT 1_000;

-- case: research/turso/scalar_subquery_column_name | source: https://github.com/tursodatabase/turso/issues/9065
CREATE TABLE t(v INTEGER);
INSERT INTO t VALUES (3);
SELECT (SELECT max(v) FROM t);
SELECT t.v, v, t.v + 1, "v" FROM t;

-- case: research/turso/index_xinfo_rowid_name_null | source: https://github.com/tursodatabase/turso/issues/9066
CREATE TABLE pk(a, b, PRIMARY KEY(a, b));
SELECT seqno, cid, name, typeof(name) FROM pragma_index_xinfo('sqlite_autoindex_pk_1') WHERE cid = -1;

-- case: research/turso/drop_column_without_rowid | source: https://github.com/tursodatabase/turso/issues/9071
CREATE TABLE wr (key INTEGER PRIMARY KEY, removed TEXT, kept INTEGER) WITHOUT ROWID;
INSERT INTO wr VALUES (1, 'discard', 101), (2, 'gone', 202);
ALTER TABLE wr DROP COLUMN removed;
SELECT * FROM wr;
SELECT kept FROM wr WHERE kept = 101;

-- case: research/turso/default_values_with_ipk_default | source: https://github.com/tursodatabase/turso/issues/9076
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT 100, val TEXT DEFAULT 'x');
INSERT INTO t DEFAULT VALUES;
INSERT INTO t(val) VALUES ('y');
INSERT INTO t DEFAULT VALUES;
SELECT * FROM t;

-- case: research/turso/drop_column_with_references_clause | source: https://github.com/tursodatabase/turso/issues/9077
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE c(parent_id INTEGER REFERENCES parent(id), keep INTEGER);
ALTER TABLE c DROP COLUMN parent_id;
SELECT sql FROM sqlite_master WHERE name = 'c';

-- case: research/turso/order_by_correlated_subquery_with_group_by | source: https://github.com/tursodatabase/turso/issues/9083
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t2 VALUES (9, 10, 'p'), (2, 5, 'q'), (2, -1, 'r');
INSERT INTO t3 VALUES (1, 11, ''), (2, 0, 'X'), (3, 6, 'z');
SELECT (SELECT count(*) FROM t2 AS y WHERE y.b > x.b) AS r1, x.c FROM t3 AS x GROUP BY x.c ORDER BY 1 DESC;

-- case: research/turso/row_value_not_in_subquery_null_component | source: https://github.com/tursodatabase/turso/issues/9087
CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER, b INTEGER);
INSERT INTO t VALUES (1,1,1),(2,1,2),(3,2,1),(4,2,2),(5,1,NULL),(6,NULL,9);
CREATE TABLE allow (a INTEGER, b INTEGER);
INSERT INTO allow VALUES (1,2),(2,1);
SELECT id FROM t WHERE (a,b) NOT IN (SELECT a,b FROM allow) ORDER BY id;
SELECT id, (a,b) NOT IN (SELECT a,b FROM allow) AS r FROM t ORDER BY id;
SELECT id, (a,b) IN (SELECT a,b FROM allow) AS r FROM t ORDER BY id;

-- case: research/turso/outer_aggregate_in_correlated_subquery | source: https://github.com/tursodatabase/turso/issues/9089
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t2 VALUES (9, 10, 'p'), (2, 5, 'q'), (2, -1, 'r');
INSERT INTO t3 VALUES (1, 11, ''), (2, 0, 'X'), (3, 6, 'z');
SELECT x.c, count(*), (SELECT count(*) FROM t2 AS y WHERE y.b > min(x.b)) AS r1 FROM t3 AS x GROUP BY x.c ORDER BY x.c;

-- case: research/turso/using_ambiguous_after_right_join | source: https://github.com/tursodatabase/turso/issues/9090
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t1 VALUES (1, 1, 'x');
INSERT INTO t3 VALUES (1, 1, 'y'), (2, 1, 'z');
SELECT count(*) FROM t1 AS x RIGHT JOIN t3 AS y ON x.a = y.a JOIN t3 AS z USING (b);

-- case: research/turso/window_order_by_aggregate_non_aggregate_query | source: https://github.com/tursodatabase/turso/issues/9096
SELECT 1 ORDER BY row_number() OVER (ORDER BY sum(1));
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES (1), (2);
SELECT a FROM t ORDER BY row_number() OVER (ORDER BY sum(a));

-- case: research/turso/update_not_between_halloween | source: https://github.com/tursodatabase/turso/issues/9160
CREATE TABLE t(c INTEGER, h INTEGER);
INSERT INTO t VALUES (7, 20), (10, 5);
UPDATE t SET h = 1 WHERE c NOT BETWEEN 1 AND CASE WHEN h < 10 THEN (SELECT sum(h) FROM t) ELSE 1 END;
SELECT * FROM t ORDER BY c;

-- case: research/turso/upsert_virtual_not_null | source: https://github.com/tursodatabase/turso/issues/9268
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, c AS (nullif(a, 0)) NOT NULL);
INSERT INTO t VALUES (1, 1);
INSERT INTO t(id, a) VALUES (1, 5) ON CONFLICT(id) DO UPDATE SET a = 0;
SELECT id, a, c FROM t;

-- case: research/turso/after_update_trigger_virtual_column_new_rowid | source: https://github.com/tursodatabase/turso/issues/9271
CREATE TABLE t(id INTEGER PRIMARY KEY, x INT, c AS (id * 10));
CREATE TABLE log(old_id, old_c, new_id, new_c);
CREATE TRIGGER t_au AFTER UPDATE ON t BEGIN INSERT INTO log VALUES (OLD.id, OLD.c, NEW.id, NEW.c); END;
INSERT INTO t(id, x) VALUES (1, 0);
UPDATE t SET id = 5 WHERE id = 1;
SELECT * FROM log;

-- case: research/turso/update_virtual_not_null_check_after_affinity | source: https://github.com/tursodatabase/turso/issues/9272
CREATE TABLE t(a INTEGER, c AS (nullif(typeof(a), 'text')) NOT NULL);
INSERT INTO t(a) VALUES (1);
UPDATE t SET a = '5';
SELECT a, c FROM t;

-- case: research/turso/upsert_virtual_column_after_affinity_check | source: https://github.com/tursodatabase/turso/issues/9273
CREATE TABLE t(id INTEGER PRIMARY KEY, b INTEGER, c AS (typeof(b)) CHECK (c = 'integer'));
INSERT INTO t(id, b) VALUES (1, 5);
INSERT INTO t(id, b) VALUES (1, 0) ON CONFLICT(id) DO UPDATE SET b = '6';
SELECT id, b, c FROM t;

-- case: research/turso/fk_parent_key_two_virtual_columns | source: https://github.com/tursodatabase/turso/issues/9274
PRAGMA foreign_keys = ON;
CREATE TABLE p(a INTEGER, b INTEGER, c AS (a + 1), e AS (b + 100), k AS (c + e) UNIQUE);
CREATE TABLE ch(x REFERENCES p(k));
INSERT INTO p(a, b) VALUES (1, 2);
INSERT INTO ch VALUES (104);
DELETE FROM p;
SELECT count(*) FROM p;

-- case: research/turso/self_ref_cascade_delete_wide_rows | source: https://github.com/tursodatabase/turso/issues/9277
CREATE TABLE t(id INTEGER PRIMARY KEY, p REFERENCES t ON DELETE CASCADE, pad);
INSERT INTO t WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM s WHERE x<35) SELECT x, NULL, zeroblob(450) FROM s;
UPDATE t SET p = 35 WHERE id BETWEEN 30 AND 34;
PRAGMA foreign_keys=ON;
DELETE FROM t WHERE id + 0 = 35;
SELECT count(*) FROM t;

-- case: research/turso/self_ref_set_null_delete_scan | source: https://github.com/tursodatabase/turso/issues/9279
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, p REFERENCES t ON DELETE SET NULL);
CREATE INDEX tp ON t(p);
INSERT INTO t VALUES (1, NULL), (2, 1), (3, 1);
DELETE FROM t WHERE p IS NULL;
SELECT * FROM t ORDER BY id;

-- case: research/turso/update_via_index_column_on_conflict_replace | source: https://github.com/tursodatabase/turso/issues/9280
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE ON CONFLICT REPLACE, a);
CREATE INDEX ta ON t(a);
INSERT INTO t VALUES (1, 1, 1), (2, 2, 2), (3, 3, 3);
UPDATE t SET u = 3 WHERE a <= 2;
SELECT * FROM t ORDER BY id;

-- case: research/turso/update_or_replace_cascade_deletes_updated_row | source: https://github.com/tursodatabase/turso/issues/9281
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE, p REFERENCES t(id) ON DELETE CASCADE);
INSERT INTO t VALUES (1, 'x', NULL), (2, 'y', 1), (3, 'z', NULL);
UPDATE OR REPLACE t SET u = 'x' WHERE id = 2;
SELECT * FROM t ORDER BY id;
SELECT id FROM t WHERE u = 'z';
PRAGMA integrity_check;

-- case: research/turso/substr_empty_blob_returns_null | source: https://github.com/tursodatabase/turso/issues/9283
SELECT quote(substr(X'', 1, 10)), quote(substr(zeroblob(0), 1)), quote(substr(X'0102', 3)), quote(substr('', 1)), quote(substr(X'0102', 1, 0));

-- case: research/turso/insert_select_from_sqlite_sequence | source: https://github.com/tursodatabase/turso/issues/9288
CREATE TABLE b(id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO b(x) SELECT seq FROM sqlite_sequence;
SELECT * FROM b;
SELECT * FROM sqlite_sequence;

-- case: research/turso/error_in_temp_only_statement_keeps_transaction | source: https://github.com/tursodatabase/turso/issues/9289
CREATE TEMP TABLE t(a);
BEGIN;
INSERT INTO t VALUES(1);
INSERT INTO t SELECT abs(-9223372036854775807 - 1);
COMMIT;
SELECT count(*) FROM t;

-- case: research/turso/cascade_and_noaction_fk_to_same_parent | source: https://github.com/tursodatabase/turso/issues/9290
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(pid REFERENCES p(id) ON DELETE CASCADE, pname REFERENCES p(name));
INSERT INTO p VALUES(1,'a');
INSERT INTO c VALUES(1,'a');
DELETE FROM p WHERE id=1;
SELECT count(*) FROM p;
SELECT count(*) FROM c;

-- case: research/turso/self_ref_set_null_delete_chain | source: https://github.com/tursodatabase/turso/issues/9293
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES t(id) ON DELETE SET NULL);
INSERT INTO t VALUES (1, NULL), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5);
DELETE FROM t WHERE id > 1;
SELECT id FROM t;

-- case: research/turso/self_ref_set_null_delete_across_pages | source: https://github.com/tursodatabase/turso/issues/9294
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES t(id) ON DELETE SET NULL, pad TEXT);
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM s WHERE x < 12)
INSERT INTO t SELECT x, CASE WHEN x > 1 THEN x - 1 END, printf('%.900c', 'x') FROM s;
DELETE FROM t WHERE id > 4;
SELECT group_concat(id) FROM t;

-- case: research/turso/select_distinct_aggregate_no_group_by | source: https://github.com/tursodatabase/turso/issues/9517
CREATE TABLE t(x INT);
INSERT INTO t VALUES (1), (2);
SELECT DISTINCT count(*) FROM t;

-- case: research/turso/delete_returning_from_subquery_reads_deleted_row | source: https://github.com/tursodatabase/turso/issues/9518
CREATE TABLE log(id INTEGER PRIMARY KEY, v INT);
INSERT INTO log VALUES (1, 20), (2, NULL), (3, 0);
DELETE FROM log RETURNING id, (SELECT s.w FROM (SELECT log.v * 2 AS w) AS s);

-- case: research/turso/aggregate_two_subquery_levels_deep | source: https://github.com/tursodatabase/turso/issues/9519
CREATE TABLE t1(a INT);
INSERT INTO t1 VALUES (1), (2), (NULL);
SELECT (SELECT sum(t1.a)) FROM t1;
SELECT (SELECT (SELECT sum(t1.a))) FROM t1;

-- case: research/turso/rename_column_view_through_from_subquery | source: https://github.com/tursodatabase/turso/issues/9520
CREATE TABLE t2(y);
CREATE VIEW v AS SELECT s.y FROM (SELECT y FROM t2) AS s;
ALTER TABLE t2 RENAME COLUMN y TO yy;
SELECT sql FROM sqlite_master WHERE name = 'v';

-- case: research/turso/having_without_group_by_dropped_filter | source: https://github.com/tursodatabase/turso/issues/9522
CREATE TABLE t(id int PRIMARY KEY, x int, y int);
INSERT INTO t VALUES (1, 1, 100), (2, 2, 200);
SELECT max(y) FROM t HAVING max(y) > 1000;
SELECT max(y) FROM t HAVING max(y) > 100;

-- case: research/turso/count_distinct_integer_vs_real_vs_text | source: https://github.com/tursodatabase/turso/issues/5806
CREATE TABLE t(v);
INSERT INTO t VALUES (1), (1.0), ('1'), (x'31'), (NULL), (1e0);
SELECT count(DISTINCT v), count(v), count(*) FROM t;
SELECT DISTINCT typeof(v), quote(v) FROM t ORDER BY 1, 2;
SELECT v, count(*) FROM t GROUP BY v ORDER BY 1;
