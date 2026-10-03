-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/attach-and-temp-schemas/76-with-clause-attached-to-an-update-whose-set-refers
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 2);
WITH c(v) AS (SELECT 9) UPDATE t SET a = c.v;
WITH c(v) AS (SELECT 9) UPDATE t SET a = (SELECT v FROM c);
SELECT * FROM t;
-- case: holdout/attach-and-temp-schemas/204-indexed-by-an-index-that-lives-in-an-attached-data
ATTACH DATABASE ':memory:' AS db2;
CREATE TABLE db2.t(id INT);
INSERT INTO db2.t VALUES(1),(2);
CREATE INDEX db2.idx_t ON t(id);
SELECT * FROM db2.t INDEXED BY idx_t WHERE id=1;
EXPLAIN QUERY PLAN SELECT * FROM db2.t WHERE id=1;
-- case: holdout/attach-and-temp-schemas/223-insert-or-replace-into-an-attached-table-with-a-un
ATTACH '' AS aux2;
CREATE TABLE aux2.t1 (id INTEGER PRIMARY KEY, name TEXT UNIQUE);
INSERT INTO aux2.t1 VALUES (1, 'a');
INSERT OR REPLACE INTO aux2.t1 VALUES (1, 'b');
INSERT OR REPLACE INTO aux2.t1 VALUES (2, 'b');
SELECT * FROM aux2.t1;
-- case: holdout/attach-and-temp-schemas/224-rollback-to-a-savepoint-undoes-changes-made-in-an-
ATTACH '' AS aux;
CREATE TABLE aux.t1 (id INTEGER PRIMARY KEY, name TEXT);
SAVEPOINT sp1;
INSERT INTO aux.t1 VALUES (1, 'a');
SAVEPOINT sp2;
INSERT INTO aux.t1 VALUES (2, 'b');
ROLLBACK TO sp2;
INSERT INTO aux.t1 VALUES (3, 'c');
RELEASE sp1;
SELECT * FROM aux.t1 ORDER BY id;
-- case: holdout/attach-and-temp-schemas/225-same-table-name-in-main-and-an-attached-database
ATTACH '' AS aux;
CREATE TABLE aux.mv1 (id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO aux.mv1 VALUES (1, 'hello');
CREATE TABLE main.t1 (id INTEGER PRIMARY KEY, val INTEGER);
CREATE VIEW mv1 AS SELECT * FROM main.t1;
DELETE FROM aux.mv1 WHERE id = 1;
INSERT INTO aux.mv1 VALUES (2, 'world');
SELECT * FROM aux.mv1;
DROP TABLE aux.mv1;
-- case: holdout/attach-and-temp-schemas/226-view-in-an-attached-database-resolves-unqualified-
ATTACH '' AS aux;
CREATE TABLE aux.t1 (id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO aux.t1 VALUES (1, 'hello'), (2, 'world');
CREATE VIEW aux.v1 AS SELECT * FROM t1;
SELECT * FROM aux.v1 ORDER BY id;
CREATE VIEW main.cross_v AS SELECT * FROM aux.t1;
-- case: holdout/attach-and-temp-schemas/227-detach-while-a-transaction-or-savepoint-is-open
ATTACH ':memory:' AS aux;
CREATE TABLE aux.t1 (id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO aux.t1 VALUES (1, 'committed');
SAVEPOINT sp1;
INSERT INTO aux.t1 VALUES (2, 'in_savepoint');
DETACH aux;
ROLLBACK;
BEGIN;
UPDATE aux.t1 SET val = 'modified' WHERE id = 1;
DETACH aux;
COMMIT;
DETACH aux;
-- case: holdout/attach-and-temp-schemas/228-query-plan-uses-the-index-of-an-attached-database
ATTACH '' AS aux;
CREATE TABLE aux.t1 (id INTEGER PRIMARY KEY, val TEXT, cat TEXT);
CREATE INDEX aux.idx_cat ON t1(cat);
INSERT INTO aux.t1 VALUES (1, 'a', 'X'), (2, 'b', 'Y'), (3, 'c', 'X');
EXPLAIN QUERY PLAN SELECT * FROM aux.t1 WHERE cat = 'X';
EXPLAIN QUERY PLAN DELETE FROM aux.t1 WHERE cat = 'Y';
-- case: holdout/attach-and-temp-schemas/230-correlated-subqueries-over-same-named-tables-in-ma
ATTACH '' AS aux;
CREATE TABLE aux.t1 (id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO aux.t1 VALUES (1, 'a'), (2, 'b'), (3, 'c');
CREATE TABLE main.t1 (id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO main.t1 VALUES (2, 'x');
SELECT * FROM aux.t1 WHERE EXISTS (SELECT 1 FROM main.t1 WHERE main.t1.id = aux.t1.id);
SELECT * FROM aux.t1 WHERE NOT EXISTS (SELECT 1 FROM main.t1 WHERE main.t1.id = aux.t1.id);
SELECT id, val, (SELECT val FROM main.t1 WHERE main.t1.id = aux.t1.id) AS main_val FROM aux.t1;
-- case: holdout/attach-and-temp-schemas/231-unqualified-table-names-resolve-to-an-attached-dat
ATTACH '' AS aux;
CREATE TABLE aux.items (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO aux.items VALUES (1, 'hello');
SELECT * FROM items;
CREATE TABLE main.users (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO main.users VALUES (1, 'alice');
CREATE TABLE aux.orders (id INTEGER PRIMARY KEY, user_id INTEGER, amount REAL);
INSERT INTO aux.orders VALUES (1, 1, 9.99);
SELECT u.name, o.amount FROM users u JOIN orders o ON u.id = o.user_id;
INSERT INTO items VALUES (2, 'x');
UPDATE items SET name = 'y' WHERE id = 2;
DELETE FROM items WHERE id = 1;
SELECT * FROM aux.items;
-- case: holdout/attach-and-temp-schemas/304-schema-table-info-and-reads-of-a-table-in-an-attac
ATTACH ':memory:' AS aux;
CREATE TABLE aux.target_table(id INTEGER PRIMARY KEY, value TEXT);
INSERT INTO aux.target_table(value) VALUES ('hello');
SELECT name FROM aux.sqlite_schema WHERE type = 'table' ORDER BY name;
PRAGMA aux.table_info('target_table');
SELECT COUNT(*) FROM aux."target_table";
-- case: holdout/attach-and-temp-schemas/473-a-view-in-main-reads-the-table-of-its-own-database
CREATE TABLE base(value INTEGER);
INSERT INTO base VALUES(1);
CREATE VIEW ov AS SELECT value FROM base;
CREATE TEMP TABLE base(value INTEGER);
INSERT INTO temp.base VALUES(2);
SELECT * FROM ov;
SELECT * FROM base;
SELECT * FROM main.base;
-- case: holdout/attach-and-temp-schemas/486-temp-view-over-a-table-of-an-attached-database
ATTACH ':memory:' AS aux;
CREATE TABLE aux.t(a);
INSERT INTO aux.t VALUES (1);
CREATE TEMP VIEW tv AS SELECT a FROM aux.t;
SELECT * FROM tv;
CREATE VIEW mv AS SELECT a FROM aux.t;
SELECT * FROM mv;
-- case: holdout/attach-and-temp-schemas/532-pragma-temp-freelist-count-reports-the-temp-databa
CREATE TEMP TABLE t(a);
INSERT INTO t SELECT zeroblob(5000) FROM generate_series(1,10);
DELETE FROM t;
PRAGMA temp.freelist_count;
PRAGMA main.freelist_count;
-- case: holdout/attach-and-temp-schemas/533-pragma-aux-page-size-changes-only-the-attached-dat
ATTACH ':memory:' AS n;
PRAGMA n.page_size=512;
CREATE TABLE n.t(a);
PRAGMA n.page_size;
PRAGMA main.page_size;
-- case: holdout/attach-and-temp-schemas/535-an-error-in-a-statement-that-writes-only-a-temp-ta
CREATE TEMP TABLE t(a);
BEGIN;
INSERT INTO t VALUES(1);
INSERT INTO t SELECT abs(-9223372036854775807 - 1);
COMMIT;
SELECT count(*) FROM t;
-- case: holdout/attach-and-temp-schemas/591-create-index-in-an-attached-database-on-an-existin
attach ':memory:' as b;
create table b.table_in_b(a);
create index b.idx on table_in_b(a);
select name, tbl_name from b.sqlite_schema;
create index b.idx2 on b.table_in_b(a);
-- case: holdout/attach-and-temp-schemas/594-temp-tables-live-in-sqlite-temp-master-not-sqlite-
CREATE TABLE temp.sqlitehelloworld(x);
select * from sqlite_master;
select name, sql from sqlite_temp_master;
select name from temp.sqlite_schema;
create temp table t2(y);
select name from sqlite_temp_schema order by name;
-- case: holdout/attach-and-temp-schemas/672-temp-qualifier-is-a-separate-database-from-main
CREATE TABLE important_data(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO important_data VALUES(1, 'critical');
DROP TABLE temp.important_data;
SELECT * FROM important_data;
CREATE TABLE target(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO target VALUES(1, 'x');
DELETE FROM temp.target;
UPDATE temp.target SET val = 'y';
INSERT INTO temp.target VALUES (2, 'z');
SELECT * FROM target;
SELECT * FROM temp.target;
-- case: holdout/attach-and-temp-schemas/710-create-temp-view-is-stored-in-the-temp-schema
CREATE TABLE t(x);
CREATE TEMP VIEW tv AS SELECT x FROM t;
SELECT name FROM sqlite_schema WHERE type='view';
SELECT name FROM sqlite_temp_schema WHERE type='view';
SELECT name FROM temp.sqlite_schema;
-- case: holdout/attach-and-temp-schemas/740-pragma-schema-auto-vacuum-applies-to-the-named-dat
ATTACH ':memory:' AS aux0;
PRAGMA aux0.auto_vacuum=full;
CREATE TABLE aux0.a(x);
CREATE TABLE main.m(x);
PRAGMA main.auto_vacuum;
PRAGMA aux0.auto_vacuum;
-- case: holdout/attach-and-temp-schemas/750-main-t-is-never-routed-to-a-temp-table-of-the-same
CREATE TABLE main.t(a);
INSERT INTO main.t VALUES (1);
CREATE TEMP TABLE t(a);
INSERT INTO temp.t VALUES (9);
UPDATE t SET a = 8;
INSERT INTO main.t VALUES (2);
SELECT 'main', a FROM main.t ORDER BY a;
SELECT 'temp', a FROM temp.t ORDER BY a;
SELECT 'bare', a FROM t ORDER BY a;
DELETE FROM main.t WHERE a = 1;
SELECT 'main', a FROM main.t ORDER BY a;
-- case: holdout/attach-and-temp-schemas/789-a-persistent-view-that-references-an-attached-data
ATTACH ':memory:' AS aux;
CREATE TABLE aux.t(a);
INSERT INTO aux.t VALUES (1);
CREATE VIEW v AS SELECT a FROM aux.t;
SELECT * FROM v;
CREATE TEMP VIEW tv AS SELECT a FROM aux.t;
SELECT * FROM tv;
-- case: holdout/attach-and-temp-schemas/805-main-sqlite-master-temp-sqlite-master-and-sqlite-m
CREATE TABLE main_t(a);
CREATE TEMP TABLE temp_t(a);
CREATE TEMP VIEW temp_v AS SELECT 1;
CREATE VIEW main_v AS SELECT 1;
SELECT 'main', name FROM main.sqlite_master ORDER BY name;
SELECT 'temp', name FROM temp.sqlite_master ORDER BY name;
SELECT 'bare', name FROM sqlite_master ORDER BY name;
SELECT 'tempschema', name FROM sqlite_temp_master ORDER BY name;
SELECT 'all', name FROM (SELECT * FROM sqlite_master UNION ALL SELECT * FROM sqlite_temp_master) ORDER BY name;
-- case: holdout/attach-and-temp-schemas/808-update-of-an-attached-table-inside-an-explicit-tra
ATTACH ':memory:' AS aux;
CREATE TABLE aux.t(id INTEGER PRIMARY KEY, v);
INSERT INTO aux.t VALUES (1, 'a');
BEGIN;
UPDATE aux.t SET v = 'b';
INSERT INTO aux.t VALUES (2, 'c');
DELETE FROM aux.t WHERE id = 2;
COMMIT;
SELECT * FROM aux.t;
