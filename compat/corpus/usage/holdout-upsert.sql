-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/upsert/6-row-value-assignment-naming-the-same-column-twice-
CREATE TABLE t0 (c0 BLOB);
INSERT INTO t0 VALUES (x'aa');
UPDATE t0 SET (c0, c0)=(x'e715', NULL);
SELECT quote(c0) FROM t0;
-- case: holdout/upsert/7-hex-integer-literal-inserted-into-a-real-column-tu
CREATE TABLE t1 (c0 REAL);
INSERT INTO t1(c0) VALUES (0x2c1cceb9), (0.5476473275978586), ('DBEx');
SELECT quote(c0), typeof(c0) FROM t1;
-- case: holdout/upsert/9-update-assigning-from-a-missing-column-reports-no-
CREATE TABLE t0 (c0 INT);
UPDATE t0 SET c0 = 1, c0 = c1;
-- case: holdout/upsert/10-update-with-repeated-column-in-set-later-assignmen
CREATE TABLE t0 (c0 INT, c1 INT);
INSERT INTO t0 VALUES (0,0), (1, 1);
UPDATE t0 SET c0 = 1, c0 = c1, c1 = c0 + c1 + 3;
SELECT * FROM t0;
-- case: holdout/upsert/13-insert-returning-x-and-returning-turso-returned-no
CREATE TABLE t(x);
INSERT INTO t VALUES (1) RETURNING x;
INSERT INTO t VALUES (2) RETURNING *;
INSERT INTO t VALUES (3) RETURNING x*10 AS y, rowid;
-- case: holdout/upsert/14-range-delete-through-an-index-deleted-every-row
CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT);
CREATE INDEX idx_name ON test(name);
INSERT INTO test VALUES (1, 'A'), (2, 'B'), (3, 'C'), (4, 'D'), (5, 'E');
DELETE FROM test WHERE name > 'C';
SELECT * FROM test ORDER BY name;
-- case: holdout/upsert/18-update-that-changes-the-column-being-scanned-hallo
create table t(x real primary key, y, z);
insert into t values (101,1,1),(102,2,2),(103,3,3);
update t set x = x + 1000 where x > 100;
select x, y, z from t order by x;
-- case: holdout/upsert/33-update-of-integer-primary-key-below-the-minimum-in
create table t(a integer primary key);
insert into t values (-9223372036854775808);
update t set a = a - 1;
select * from t;
-- case: holdout/upsert/34-insert-select-from-values-union-all-values-inserts
create table t(a, b);
insert into t(a, b) select * from (values(3, 3)) union all values(4, 4);
select * from t;
-- case: holdout/upsert/35-insert-values-union-all-values-turso-ignored-the-u
create table t(a, b);
insert into t(a, b) values(3,3) union all values(4, 4);
select * from t;
values(3,3) union all values(4, 4);
-- case: holdout/upsert/37-insert-select-sum-from-the-same-table-reads-a-snap
create table t(a integer primary key, b integer);
insert into t(b) values(1),(2);
insert into t(b) select sum(b) from t;
select * from t;
-- case: holdout/upsert/38-insert-select-union-select-inserts-all-rows-turso-
CREATE TABLE t1 (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t1 VALUES (1, 'a'), (2, 'b');
INSERT INTO t2 SELECT id, name FROM t1 UNION SELECT 3, 'c';
SELECT * FROM t2 ORDER BY id;
-- case: holdout/upsert/39-insert-select-on-conflict-without-where-is-a-synta
CREATE TABLE t1 (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t2 SELECT id, name FROM t1 ON CONFLICT DO NOTHING;
INSERT INTO t2 SELECT id, name FROM t1 WHERE true ON CONFLICT DO NOTHING;
-- case: holdout/upsert/40-insert-select-with-several-correlated-scalar-subqu
CREATE TABLE products(id INTEGER PRIMARY KEY, name TEXT, price REAL);
CREATE TABLE sales(id INTEGER PRIMARY KEY, product_id INTEGER, quantity INTEGER);
CREATE TABLE summary(product_id INTEGER PRIMARY KEY, name TEXT, total_quantity INTEGER, avg_price REAL);
INSERT INTO products VALUES (1, 'A', 10.0), (2, 'B', 20.0), (3, 'C', 30.0);
INSERT INTO sales VALUES (1, 1, 5), (2, 1, 10), (3, 2, 15), (4, 3, 20);
INSERT INTO summary(product_id, name, total_quantity, avg_price) SELECT p.id, p.name, (SELECT SUM(quantity) FROM sales WHERE product_id = p.id), (SELECT AVG(price) FROM products WHERE id = p.id) FROM products p;
SELECT * FROM summary;
-- case: holdout/upsert/45-update-or-replace-that-deletes-another-row-keeps-i
create table t (id integer primary key, x text);
create index t_x on t(x);
insert into t values (1, 'aaa'), (2, 'bbb'), (3, 'ccc');
update or replace t set id = 3 where id = 2;
select * from t;
select * from t where x = 'ccc';
select * from t where x = 'bbb';
pragma integrity_check;
-- case: holdout/upsert/49-expression-index-entry-is-deleted-when-the-row-is-
CREATE TABLE t(id INTEGER PRIMARY KEY, val INT);
CREATE INDEX idx ON t(val * 2);
INSERT INTO t VALUES(1, 10), (2, 20), (3, 30);
UPDATE t SET val = 99 WHERE id = 2;
PRAGMA integrity_check;
SELECT id FROM t WHERE val * 2 = 40;
SELECT id FROM t WHERE val * 2 = 198;
-- case: holdout/upsert/61-partial-index-maintained-across-update-that-leaves
CREATE TABLE t(val INT);
CREATE INDEX idx ON t(val) WHERE val > 10;
INSERT INTO t VALUES(15), (20), (5);
UPDATE t SET val = 5 WHERE val = 15;
PRAGMA integrity_check;
DELETE FROM t WHERE val = 20;
PRAGMA integrity_check;
UPDATE t SET val = 50 WHERE val = 5;
PRAGMA integrity_check;
SELECT val FROM t WHERE val > 10;
-- case: holdout/upsert/66-insert-or-ignore-that-fails-check-does-not-touch-s
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT CHECK(id < 5), val TEXT);
INSERT INTO t(val) VALUES('row1');
INSERT OR IGNORE INTO t(id, val) VALUES(10, 'row10');
SELECT * FROM sqlite_sequence;
SELECT * FROM t;
-- case: holdout/upsert/73-changes-counts-only-table-rows-not-index-entries
CREATE TABLE t (a INT);
CREATE INDEX idx ON t(a);
INSERT INTO t VALUES (1), (2);
DELETE FROM t WHERE a < 3;
SELECT changes(), total_changes();
-- case: holdout/upsert/74-unique-a-on-conflict-ignore-in-create-table
CREATE TABLE t(a, b, UNIQUE(a) ON CONFLICT IGNORE);
INSERT INTO t VALUES (1, 'x'), (1, 'y'), (2, 'z');
SELECT * FROM t;
CREATE TABLE u(a PRIMARY KEY ON CONFLICT REPLACE, b NOT NULL ON CONFLICT IGNORE);
INSERT INTO u VALUES (1, 'x'), (1, 'y'), (2, NULL);
SELECT * FROM u;
-- case: holdout/upsert/77-insert-naming-a-column-that-does-not-exist
CREATE TABLE t(a);
INSERT INTO t(x) SELECT a FROM t;
INSERT INTO t(a, a) VALUES (1, 2);
-- case: holdout/upsert/88-update-does-not-change-last-insert-rowid
CREATE TABLE t1(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t1 VALUES (1, 'a');
INSERT INTO t1 VALUES (5, 'b');
INSERT INTO t1(val) VALUES ('c');
SELECT last_insert_rowid();
UPDATE t1 SET val = 'updated' WHERE id = 1;
SELECT last_insert_rowid();
DELETE FROM t1 WHERE id = 5;
SELECT last_insert_rowid();
-- case: holdout/upsert/93-upsert-do-update-checks-the-check-constraint-again
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER CHECK(val BETWEEN 0 AND 100), name TEXT);
INSERT INTO t VALUES (1, 50, 'one');
INSERT INTO t VALUES (1, 200, 'bad') ON CONFLICT(id) DO UPDATE SET val = 70, name = 'fixed';
SELECT * FROM t;
INSERT INTO t VALUES (1, 200, 'bad') ON CONFLICT(id) DO UPDATE SET val = excluded.val;
-- case: holdout/upsert/94-not-null-violation-from-do-update-names-table-colu
CREATE TABLE my_tbl(id INTEGER PRIMARY KEY, val INTEGER NOT NULL, name TEXT NOT NULL);
INSERT INTO my_tbl VALUES (1, 10, 'first');
INSERT INTO my_tbl VALUES (1, NULL, 'second') ON CONFLICT(id) DO UPDATE SET val = excluded.val, name = excluded.name;
INSERT INTO my_tbl VALUES (2, NULL, 'x');
-- case: holdout/upsert/103-expression-index-key-computed-from-the-post-affini
CREATE TABLE t0 (c0 INT);
INSERT INTO t0 VALUES (0);
CREATE INDEX idx ON t0 (CAST(c0 AS TEXT));
UPDATE t0 SET c0 = '0.0';
UPDATE t0 SET c0 = 1;
PRAGMA integrity_check;
CREATE TABLE t1 (c0 TEXT);
INSERT INTO t1 VALUES ('x');
CREATE INDEX i0 ON t1(c0 = 0);
UPDATE t1 SET c0 = 0;
UPDATE t1 SET c0 = 1;
PRAGMA integrity_check;
-- case: holdout/upsert/109-returning-with-a-correlated-subquery-sees-the-row-
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE lookup(k INTEGER PRIMARY KEY, v TEXT);
INSERT INTO lookup VALUES(1, 'one'), (2, 'two'), (3, 'three');
INSERT INTO t1 VALUES(1, 'x') RETURNING a, (SELECT v FROM lookup WHERE k = a) as val;
INSERT INTO t1 VALUES(2, 'y'), (3, 'z') RETURNING a, (SELECT v FROM lookup WHERE k = a) as val;
INSERT INTO t1 VALUES(2, 'w') ON CONFLICT(a) DO UPDATE SET b = 'upd' RETURNING a, b, (SELECT v FROM lookup WHERE k = a) as val;
-- case: holdout/upsert/110-last-insert-rowid-after-insert-returning
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
INSERT INTO t1 VALUES(10, 'a');
SELECT last_insert_rowid();
INSERT INTO t1 VALUES(20, 'b') RETURNING last_insert_rowid();
SELECT last_insert_rowid();
INSERT INTO t1 VALUES(50, 'c') RETURNING *;
SELECT last_insert_rowid();
-- case: holdout/upsert/113-update-returning-correlated-subquery-sees-the-upda
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER);
CREATE TABLE lookup(k INTEGER PRIMARY KEY, v TEXT);
INSERT INTO t1 VALUES(1, 10);
INSERT INTO lookup VALUES(10, 'old_value'), (20, 'new_value'), (100, 'h'), (200, 'hh'), (300, 'hhh');
UPDATE t1 SET b = 20 WHERE a = 1 RETURNING b, (SELECT v FROM lookup WHERE k = b) as looked_up;
CREATE TABLE t2(a INTEGER PRIMARY KEY, b INTEGER);
INSERT INTO t2 VALUES(1, 10), (2, 20), (3, 30);
UPDATE t2 SET b = b * 10 RETURNING a, b, (SELECT v FROM lookup WHERE k = b) as label;
-- case: holdout/upsert/114-returning-with-a-subquery-over-the-table-being-mod
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT, c INTEGER);
INSERT INTO t1 VALUES(1, 'a', 10), (2, 'b', 20), (3, 'c', 30);
DELETE FROM t1 WHERE a = 2 RETURNING *, (SELECT count(*) FROM t1) as remaining;
CREATE TABLE t2(a INTEGER PRIMARY KEY, b INTEGER);
INSERT INTO t2 VALUES(1, 100), (2, 200), (3, 300);
UPDATE t2 SET b = b + 10 RETURNING a, b, (SELECT sum(b) FROM t2) as total;
-- case: holdout/upsert/186-delete-with-a-subquery-where-and-returning-with-a-
CREATE TABLE t1(a INTEGER, b TEXT);
INSERT INTO t1 VALUES(1,'x'),(2,'y'),(3,'z'),(4,'w');
CREATE TABLE t2(c INTEGER, d TEXT);
INSERT INTO t2 VALUES(2,'p'),(4,'q');
DELETE FROM t1 WHERE a IN (SELECT c FROM t2) RETURNING a, b, (SELECT d FROM t2 WHERE t2.c = t1.a) AS matched_d;
-- case: holdout/upsert/267-without-rowid-insert-applies-affinity-per-declared
CREATE TABLE t(c TEXT, b INTEGER PRIMARY KEY, a REAL) WITHOUT ROWID;
INSERT INTO t VALUES ('x', '1', '2.5');
SELECT typeof(c), typeof(b), typeof(a) FROM t;
INSERT INTO t VALUES (5, '2', '3');
SELECT quote(c), quote(b), quote(a) FROM t ORDER BY b;
-- case: holdout/upsert/270-a-statement-that-aborts-on-a-unique-conflict-leave
CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER NOT NULL);
CREATE VIEW mv AS SELECT v, COUNT(*) AS cnt FROM t GROUP BY v;
INSERT INTO t VALUES (1, 10);
INSERT INTO t VALUES (2, 20);
INSERT INTO t SELECT 3 AS id, 30 AS v UNION ALL SELECT 1, 99;
INSERT INTO t VALUES (4, 40);
SELECT v, cnt FROM mv ORDER BY v;
SELECT * FROM t ORDER BY id;
-- case: holdout/upsert/271-multi-row-insert-values-keeps-every-row
CREATE TABLE v(key, value);
INSERT INTO v(key, value) VALUES ('k1','v1'), ('k2','v2'), ('k3','v3');
SELECT key, value FROM v;
-- case: holdout/upsert/272-insert-or-rollback-with-on-conflict-do-update-that
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE);
INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN;
INSERT INTO t VALUES(3,'u3','a3');
INSERT OR ROLLBACK INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT count(*) FROM t WHERE id=3;
SELECT * FROM t ORDER BY id;
-- case: holdout/upsert/273-insert-or-replace-with-not-null-default-check-runs
CREATE TABLE t(a INT NOT NULL DEFAULT 5 CHECK(a IS NULL));
INSERT OR REPLACE INTO t VALUES(NULL);
SELECT * FROM t;
PRAGMA integrity_check;
-- case: holdout/upsert/274-insert-or-replace-into-a-strict-not-null-column-wi
CREATE TABLE t(a INT NOT NULL DEFAULT 'x') STRICT;
INSERT OR REPLACE INTO t VALUES(NULL);
SELECT typeof(a), a FROM t;
INSERT INTO t DEFAULT VALUES;
SELECT typeof(a), a FROM t;
-- case: holdout/upsert/277-runtime-error-on-the-second-row-of-an-update-undoe
CREATE TABLE t(id INTEGER PRIMARY KEY, x INT);
INSERT INTO t VALUES(1,10),(2,20);
BEGIN;
UPDATE t SET x = CASE WHEN id=1 THEN 11 ELSE (char(120) LIKE char(120) ESCAPE (char(121)||char(121))) END;
SELECT id,x FROM t ORDER BY id;
COMMIT;
SELECT id,x FROM t ORDER BY id;
PRAGMA integrity_check;
-- case: holdout/upsert/283-autoincrement-updates-the-right-sqlite-sequence-ro
CREATE TABLE t1(i INTEGER PRIMARY KEY AUTOINCREMENT);
CREATE TABLE t2(i INTEGER PRIMARY KEY AUTOINCREMENT);
CREATE TABLE t3(i INTEGER PRIMARY KEY AUTOINCREMENT);
INSERT INTO t1 VALUES(NULL);
INSERT INTO t2 VALUES(NULL);
INSERT INTO t3 VALUES(NULL);
UPDATE sqlite_sequence SET name=NULL WHERE name='t2';
INSERT INTO t3 VALUES(NULL);
SELECT rowid, name, seq FROM sqlite_sequence ORDER BY rowid;
-- case: holdout/upsert/386-subquery-in-on-conflict-do-update-set-and-where
CREATE TABLE a(id INTEGER PRIMARY KEY, x INTEGER);
INSERT INTO a VALUES(1,5);
CREATE TABLE b(id INTEGER, x INTEGER);
INSERT INTO b VALUES(1,7);
INSERT INTO a VALUES(1,0) ON CONFLICT(id) DO UPDATE SET x=(SELECT x FROM b WHERE id=1);
SELECT x FROM a;
INSERT INTO a VALUES(1,0) ON CONFLICT(id) DO UPDATE SET x=9 WHERE EXISTS(SELECT 1 FROM b);
SELECT x FROM a;
INSERT INTO a VALUES(1,0) ON CONFLICT(id) DO UPDATE SET x=excluded.x + (SELECT max(x) FROM b) WHERE a.x IN (SELECT x FROM b UNION SELECT 9);
SELECT x FROM a;
-- case: holdout/upsert/404-range-seek-on-an-index-with-a-null-key-matches-not
CREATE TABLE t3(c);
CREATE INDEX i3 ON t3(c);
INSERT INTO t3 VALUES (1),(2),(3);
SELECT count(*) FROM t3 WHERE c > ('{"k":1}' ->> '$.missing');
DELETE FROM t3 WHERE c > ('{"k":1}' ->> '$.missing');
SELECT count(*) FROM t3;
SELECT count(*) FROM t3 WHERE c > ('a' LIKE 'a' ESCAPE NULL);
SELECT count(*) FROM t3 WHERE c <= NULL OR c >= NULL;
SELECT count(*) FROM t3 WHERE c BETWEEN NULL AND 5;
-- case: holdout/upsert/405-a-failed-update-inside-a-transaction-undoes-the-ro
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT);
INSERT INTO t VALUES(1,'a'),(2,'b'),(3,'c'),(10,'j');
UPDATE t SET id = id + 7;
SELECT group_concat(id) FROM (SELECT id FROM t ORDER BY id);
BEGIN;
UPDATE t SET id = id + 7;
COMMIT;
SELECT group_concat(id) FROM (SELECT id FROM t ORDER BY id);
-- case: holdout/upsert/422-upsert-with-a-conflict-target-on-a-table-declared-
CREATE TABLE t(a UNIQUE ON CONFLICT REPLACE, b);
CREATE INDEX i ON t(b);
INSERT INTO t(a,b) VALUES('x',1);
INSERT INTO t(a,b) VALUES('y',2) ON CONFLICT(a) DO UPDATE SET b=9;
INSERT INTO t(a,b) VALUES('x',3) ON CONFLICT(a) DO UPDATE SET b=9;
SELECT * FROM t ORDER BY a;
CREATE TABLE v(a UNIQUE ON CONFLICT IGNORE, b UNIQUE);
INSERT INTO v VALUES('x',1);
INSERT INTO v VALUES('x',2);
INSERT INTO v VALUES('y',1) ON CONFLICT(b) DO NOTHING;
SELECT * FROM v;
-- case: holdout/upsert/432-comparison-of-an-indexed-text-column-with-a-scalar
CREATE TABLE t(x TEXT);
CREATE TABLE u(k INTEGER);
INSERT INTO t VALUES('20');
INSERT INTO u VALUES(20);
CREATE INDEX i ON t(x);
SELECT count(*) FROM t WHERE x = (SELECT k FROM u);
SELECT count(*) FROM t WHERE x > (SELECT k FROM u);
SELECT count(*) FROM t NOT INDEXED WHERE x = (SELECT k FROM u);
SELECT count(*) FROM t WHERE x < (SELECT k FROM u);
DELETE FROM t WHERE x = (SELECT k FROM u);
SELECT count(*) FROM t;
-- case: holdout/upsert/435-insert-or-ignore-with-an-on-conflict-do-update-arm
CREATE TABLE t(a INTEGER PRIMARY KEY, b INTEGER);
INSERT INTO t VALUES(1,1);
INSERT OR IGNORE INTO t VALUES(1,10) ON CONFLICT(a) DO UPDATE SET b=excluded.b RETURNING a,b;
SELECT b, changes() FROM t;
CREATE TABLE u(a INTEGER PRIMARY KEY, b INTEGER);
INSERT INTO u VALUES(1,1);
INSERT OR FAIL INTO u VALUES(1,10) ON CONFLICT(a) DO UPDATE SET b=excluded.b;
SELECT b FROM u;
INSERT OR REPLACE INTO u VALUES(1,20) ON CONFLICT(a) DO UPDATE SET b=excluded.b + 1;
SELECT b FROM u;
-- case: holdout/upsert/441-insert-or-fail-that-fails-a-check-does-not-advance
CREATE TABLE t(a INTEGER PRIMARY KEY AUTOINCREMENT, b INTEGER CHECK(b > 0));
INSERT OR FAIL INTO t VALUES(7, 0);
INSERT INTO t(b) VALUES(1);
SELECT a FROM t;
SELECT seq FROM sqlite_sequence WHERE name='t';
CREATE TABLE u(a INTEGER PRIMARY KEY AUTOINCREMENT CHECK(a < 5), b);
INSERT OR FAIL INTO u VALUES(9, 1);
INSERT INTO u(b) VALUES(1);
SELECT count(*) FROM u;
SELECT * FROM sqlite_sequence ORDER BY name;
-- case: holdout/upsert/456-insert-as-alias-with-an-on-conflict-do-update
CREATE TABLE t(k INT PRIMARY KEY, n INT);
CREATE TABLE u(k INT PRIMARY KEY, n INT);
INSERT INTO t VALUES(1,10);
INSERT INTO u VALUES(1,10);
INSERT INTO u AS excluded VALUES(1,99) ON CONFLICT(k) DO UPDATE SET n = excluded.n;
SELECT n FROM u;
INSERT INTO t AS x VALUES(1,7) ON CONFLICT(k) DO UPDATE SET n = x.n + 1;
SELECT n FROM t;
INSERT INTO t AS x VALUES(1,7) ON CONFLICT(k) DO UPDATE SET n = t.n + 1;
-- case: holdout/upsert/501-insert-default-values-ignores-the-default-of-an-in
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT 100, val TEXT DEFAULT 'x');
INSERT INTO t DEFAULT VALUES;
INSERT INTO t(val) VALUES ('y');
INSERT INTO t DEFAULT VALUES;
SELECT * FROM t;
INSERT INTO t(id) VALUES (NULL);
SELECT * FROM t ORDER BY id;
-- case: holdout/upsert/519-update-whose-where-contains-a-subquery-over-the-ta
CREATE TABLE t(c INTEGER, h INTEGER);
INSERT INTO t VALUES (7, 20), (10, 5);
UPDATE t SET h = 1 WHERE c NOT BETWEEN 1 AND CASE WHEN h < 10 THEN (SELECT sum(h) FROM t) ELSE 1 END;
SELECT * FROM t;
-- case: holdout/upsert/529-update-through-an-index-with-a-column-level-on-con
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE ON CONFLICT REPLACE, a);
CREATE INDEX ta ON t(a);
INSERT INTO t VALUES (1, 1, 1), (2, 2, 2), (3, 3, 3);
UPDATE t SET u = 3 WHERE a <= 2;
SELECT * FROM t;
DELETE FROM t;
INSERT INTO t VALUES (1, 1, 1), (2, 2, 2), (3, 3, 3);
UPDATE t SET u = u - 1 WHERE a >= 2;
SELECT * FROM t ORDER BY id;
-- case: holdout/upsert/534-insert-select-from-sqlite-sequence-does-not-read-t
CREATE TABLE b(id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO b(x) SELECT seq FROM sqlite_sequence;
SELECT * FROM b;
SELECT * FROM sqlite_sequence;
-- case: holdout/upsert/567-autoincrement-never-reuses-ids-after-delete
CREATE TABLE t3(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t3(v) VALUES ('a'),('b'),('c');
DELETE FROM t3;
INSERT INTO t3(v) VALUES ('d');
SELECT id FROM t3;
SELECT * FROM sqlite_sequence;
CREATE TABLE t4(id INTEGER PRIMARY KEY, v);
INSERT INTO t4(v) VALUES ('a'),('b'),('c');
DELETE FROM t4;
INSERT INTO t4(v) VALUES ('d');
SELECT id FROM t4;
-- case: holdout/upsert/576-insert-into-t-select-a-from-select-a-from-t-duplic
create table t(a);
insert into t values (1);
insert into t select a from (select a from t);
select count(*) from t;
insert into t select a from (select a from t);
select count(*) from t;
insert into t select a from t;
select count(*) from t;
-- case: holdout/upsert/595-autoincrement-counter-after-update-of-the-key-colu
create table t(a integer primary key autoincrement);
insert into t default values;
update t set a = a + 1;
select * from sqlite_sequence;
insert into t default values;
select * from sqlite_sequence;
select * from t order by a;
-- case: holdout/upsert/645-returning-with-a-multi-column-scalar-subquery
CREATE TABLE t ( x );
INSERT INTO t VALUES ( 1 ) RETURNING ( SELECT 1, 2 );
SELECT (SELECT 1, 2);
SELECT 1 WHERE (SELECT 1, 2) = 1;
SELECT * FROM t WHERE x = (SELECT 1, 2);
-- case: holdout/upsert/649-do-update-set-naming-a-column-of-another-table
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER);
CREATE TABLE other(id INTEGER PRIMARY KEY, val INTEGER);
INSERT INTO t VALUES (1, 10);
INSERT INTO t VALUES (1, 20) ON CONFLICT(id) DO UPDATE SET val = other.val;
CREATE TABLE balances(id INTEGER PRIMARY KEY, amount INTEGER) STRICT;
INSERT INTO balances VALUES (1, 100);
INSERT INTO balances VALUES (1, 5) ON CONFLICT(id) DO UPDATE SET amount = amount + excluded.amount;
INSERT INTO balances VALUES (1, 5) ON CONFLICT(id) DO UPDATE SET amount = balances.amount + new.amount;
SELECT * FROM balances;
-- case: holdout/upsert/652-do-update-set-refers-to-a-column-that-does-not-exi
CREATE TABLE accounts(id INTEGER PRIMARY KEY, owner TEXT, version INTEGER);
INSERT INTO accounts(id, owner, version) VALUES (1, 'a', 1);
INSERT INTO accounts(id, owner) VALUES (1, 'b') ON CONFLICT(id) DO UPDATE SET version = accounts.versABS + 1;
INSERT INTO accounts(id, owner) VALUES (1, 'b') ON CONFLICT(id) DO UPDATE SET version = excluded.nosuch;
INSERT INTO accounts(id, owner) VALUES (1, 'b') ON CONFLICT(nosuch) DO UPDATE SET version = 2;
-- case: holdout/upsert/653-sqlite-sequence-after-an-insert-or-ignore-with-a-n
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, payload TEXT);
INSERT OR IGNORE INTO t(id,payload) VALUES (-42, 'tx_explicit_0');
SELECT name, seq FROM sqlite_sequence WHERE name='t';
INSERT INTO t(payload) VALUES ('auto');
SELECT id FROM t ORDER BY id;
SELECT name, seq FROM sqlite_sequence WHERE name='t';
-- case: holdout/upsert/655-insert-omitting-a-column-declared-serial-primary-k
CREATE TABLE t_serial (id SERIAL PRIMARY KEY, name text NOT NULL);
INSERT INTO t_serial ("name") VALUES ('alice');
SELECT * FROM t_serial;
CREATE TABLE t_int (id INT PRIMARY KEY, name text);
INSERT INTO t_int (name) VALUES ('alice');
SELECT quote(id), name FROM t_int;
CREATE TABLE t_integer (id INTEGER PRIMARY KEY, name text);
INSERT INTO t_integer (name) VALUES ('alice');
SELECT quote(id), name FROM t_integer;
CREATE TABLE t_bigint (id BIGINT PRIMARY KEY, name text);
INSERT INTO t_bigint (name) VALUES ('alice');
SELECT quote(id), name FROM t_bigint;
-- case: holdout/upsert/679-returning-with-a-table-name-qualifier-when-the-tab
create table "users" (id integer primary key, name text);
insert into "users" values (1, 'a') returning "users".id, "users".name;
insert into "users" values (2, 'b') returning users.id, users.name;
delete from "users" where id = 1 returning "users".id, "users".name;
update "users" set name='c' where id=2 returning "users".id, "users".name;
insert into "users" values (3, 'q') returning main.users.id;
insert into users values (4, 'r') returning other.id;
-- case: holdout/upsert/681-changes-after-update-or-replace-that-deletes-a-con
CREATE TABLE t(a INTEGER PRIMARY KEY, b INT UNIQUE);
INSERT INTO t VALUES(1,10),(2,20),(3,30);
UPDATE OR REPLACE t SET b = 10 WHERE a = 3;
SELECT changes();
INSERT OR REPLACE INTO t VALUES (2, 10);
SELECT changes(), total_changes();
-- case: holdout/upsert/686-insert-or-fail-with-a-check-failure-on-the-second-
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, x INT CHECK(x > 0));
INSERT INTO t(x) VALUES (1);
INSERT OR FAIL INTO t(x) VALUES (2), (-1), (3);
INSERT INTO t(x) VALUES (9);
SELECT 't', id, x FROM t UNION ALL SELECT 's', 0, seq FROM sqlite_sequence WHERE name = 't' ORDER BY 1, 2;
-- case: holdout/upsert/688-delete-from-sqlite-sequence-is-allowed
CREATE TABLE t1(x INTEGER PRIMARY KEY AUTOINCREMENT);
INSERT INTO t1 VALUES(NULL);
DELETE FROM sqlite_sequence WHERE name='t1';
SELECT count(*) FROM sqlite_sequence;
INSERT INTO t1 VALUES(NULL);
SELECT * FROM t1 ORDER BY x;
SELECT * FROM sqlite_sequence;
DROP TABLE sqlite_sequence;
-- case: holdout/upsert/692-on-conflict-target-with-a-where-clause-matches-a-p
CREATE TABLE t (a TEXT, b TEXT, deleted INTEGER DEFAULT 0);
CREATE UNIQUE INDEX t_ab_live ON t (a, b) WHERE deleted = 0;
INSERT INTO t (a, b, deleted) VALUES ('a', 'b', 0);
INSERT INTO t (a, b, deleted) VALUES ('a', 'b', 0) ON CONFLICT (a, b) WHERE deleted = 0 DO UPDATE SET b = excluded.b;
SELECT a, b, deleted FROM t;
INSERT INTO t (a, b, deleted) VALUES ('a', 'b', 1);
INSERT INTO t (a, b, deleted) VALUES ('a', 'b', 0) ON CONFLICT (a, b) DO NOTHING;
INSERT INTO t (a, b, deleted) VALUES ('a', 'b', 0) ON CONFLICT (a, b) WHERE deleted = 1 DO NOTHING;
-- case: holdout/upsert/693-result-column-name-of-a-backtick-quoted-column-in-
CREATE TABLE counters (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO counters (name) VALUES ('test') RETURNING `id`;
INSERT INTO counters (name) VALUES ('t2') RETURNING "id", [name], counters.id, id AS x, id+1;
-- case: holdout/upsert/698-changes-and-last-insert-rowid-after-insert-into-an
CREATE TABLE Foo(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO Foo VALUES(1,'a');
INSERT INTO Foo VALUES(2,'b');
INSERT INTO Foo(name) VALUES('Test Prepare');
SELECT last_insert_rowid(), changes(), total_changes(), (SELECT max(id) FROM Foo);
-- case: holdout/upsert/730-update-from-with-returning-that-names-the-from-tab
CREATE TABLE tgt (id int, note text);
CREATE TABLE src (id int, v text);
INSERT INTO tgt VALUES (1, NULL), (2, NULL);
INSERT INTO src VALUES (2, 'two');
UPDATE tgt SET note = 'x' FROM src WHERE tgt.id = src.id RETURNING tgt.id, src.v;
SELECT * FROM tgt;
UPDATE tgt SET note = src.v FROM src WHERE tgt.id = src.id RETURNING id, note;
-- case: holdout/upsert/733-pragma-max-page-count-with-a-failed-insert-inside-
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
PRAGMA max_page_count=3;
BEGIN;
INSERT INTO t VALUES (2, zeroblob(3000)), (1, zeroblob(3000));
PRAGMA max_page_count=10;
INSERT INTO t VALUES (3, zeroblob(3000)), (4, zeroblob(3000));
COMMIT;
SELECT count(*) FROM t;
-- case: holdout/upsert/734-do-update-that-sets-the-integer-primary-key-to-its
CREATE TABLE preference (singleton INTEGER PRIMARY KEY, value TEXT NOT NULL) STRICT;
INSERT INTO preference VALUES (1, 'saved');
INSERT INTO preference VALUES (1, 'saved') ON CONFLICT(singleton) DO UPDATE SET singleton = excluded.singleton;
SELECT singleton, value FROM preference;
INSERT INTO preference VALUES (1, 'new') ON CONFLICT(singleton) DO UPDATE SET singleton = 2;
SELECT singleton, value FROM preference;
-- case: holdout/upsert/741-primary-key-is-enforced-on-insert
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t VALUES (1, 'a');
INSERT INTO t VALUES (1, 'b');
CREATE TABLE u(k TEXT PRIMARY KEY, v);
INSERT INTO u VALUES ('x', 1);
INSERT INTO u VALUES ('x', 2);
CREATE TABLE w(a, b, PRIMARY KEY(a, b));
INSERT INTO w VALUES (1, 1);
INSERT INTO w VALUES (1, 1);
INSERT INTO w VALUES (NULL, 1);
INSERT INTO w VALUES (NULL, 1);
SELECT count(*) FROM w;
-- case: holdout/upsert/756-excluded-id-when-the-conflict-is-on-a-secondary-un
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, n INT);
INSERT INTO t VALUES (1, 'x', 0);
INSERT INTO t VALUES (7, 'x', 5) ON CONFLICT(u) DO UPDATE SET n = excluded.id;
SELECT * FROM t;
INSERT INTO t VALUES (8, 'x', 6) ON CONFLICT(u) DO UPDATE SET id = excluded.id;
SELECT * FROM t;
-- case: holdout/upsert/761-insert-or-ignore-returning-reports-only-inserted-r
CREATE TABLE t(id INTEGER PRIMARY KEY, v UNIQUE);
INSERT INTO t VALUES (1, 'a');
INSERT OR IGNORE INTO t VALUES (2, 'a'), (3, 'b') RETURNING id, v;
INSERT INTO t VALUES (4, 'b') ON CONFLICT DO NOTHING RETURNING id;
INSERT INTO t VALUES (1, 'z') ON CONFLICT DO NOTHING RETURNING id;
SELECT * FROM t ORDER BY id;
-- case: holdout/upsert/762-update-or-ignore-returning-reports-only-updated-ro
CREATE TABLE t(id INTEGER PRIMARY KEY, v UNIQUE);
INSERT INTO t VALUES (1, 'a'), (2, 'b'), (3, 'c');
UPDATE OR IGNORE t SET v = 'a' RETURNING id, v;
SELECT * FROM t ORDER BY id;
UPDATE t SET v = 'a' WHERE id = 1 RETURNING id, v;
-- case: holdout/upsert/790-insert-or-ignore-that-hits-a-conflict-still-advanc
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v UNIQUE);
INSERT INTO t(v) VALUES ('a');
INSERT OR IGNORE INTO t(v) VALUES ('a');
SELECT * FROM sqlite_sequence;
INSERT INTO t(v) VALUES ('b');
SELECT id, v FROM t ORDER BY id;
SELECT * FROM sqlite_sequence;
-- case: holdout/upsert/791-insert-or-replace-updates-partial-and-expression-i
CREATE TABLE t(id INTEGER PRIMARY KEY, a UNIQUE, b);
CREATE INDEX t_expr ON t(lower(b));
CREATE INDEX t_part ON t(b) WHERE a > 0;
INSERT INTO t VALUES (1, 10, 'X');
INSERT OR REPLACE INTO t VALUES (2, 10, 'Y');
PRAGMA integrity_check;
SELECT id FROM t WHERE lower(b) = 'x';
SELECT id FROM t WHERE lower(b) = 'y';
SELECT id FROM t WHERE b = 'X' AND a > 0;
-- case: holdout/upsert/822-pragma-journal-mode-delete-and-other-modes-on-an-i
PRAGMA journal_mode=DELETE;
PRAGMA journal_mode=WAL;
PRAGMA journal_mode=OFF;
PRAGMA journal_mode=MEMORY;
PRAGMA journal_mode=TRUNCATE;
PRAGMA journal_mode=PERSIST;
PRAGMA journal_mode=bogus;
PRAGMA journal_mode;
-- case: holdout/upsert/827-changes-after-a-multi-row-insert-or-replace
CREATE TABLE runs (run_id TEXT PRIMARY KEY NOT NULL);
CREATE TABLE interactions (run_id TEXT NOT NULL, tick INTEGER NOT NULL CHECK (tick >= 0), seq INTEGER NOT NULL CHECK (seq >= 0), kind TEXT NOT NULL CHECK (kind <> ''), PRIMARY KEY (run_id, tick, seq), FOREIGN KEY (run_id) REFERENCES runs (run_id));
INSERT INTO runs VALUES ('r');
INSERT OR REPLACE INTO interactions VALUES ('r', 1, 1, 'a'), ('r', 1, 2, 'b'), ('r', 1, 3, 'c');
SELECT changes();
INSERT OR REPLACE INTO interactions VALUES ('r', 1, 1, 'x'), ('r', 1, 4, 'd');
SELECT changes(), total_changes();
SELECT count(*) FROM interactions;
-- case: holdout/upsert/831-update-or-ignore-that-conflicts-on-one-unique-inde
CREATE TABLE u(id INTEGER PRIMARY KEY,b UNIQUE,c);
CREATE INDEX u_c ON u(c);
INSERT INTO u VALUES(1,10,1),(2,20,2);
UPDATE OR IGNORE u SET b=10 WHERE id=2;
SELECT id,b,c FROM u INDEXED BY u_c WHERE c=2 ORDER BY id;
PRAGMA integrity_check;
SELECT changes();
-- case: holdout/upsert/839-default-datetime-now-is-stored-as-written-in-the-s
CREATE TABLE oidc_pkce (state TEXT NOT NULL PRIMARY KEY, provider_id TEXT NOT NULL, verifier TEXT NOT NULL, role TEXT, created_at DATETIME NOT NULL DEFAULT (datetime('now')));
SELECT sql FROM sqlite_master WHERE name = 'oidc_pkce';
INSERT INTO oidc_pkce(state, provider_id, verifier) VALUES ('s', 'p', 'v');
SELECT typeof(created_at), length(created_at), created_at LIKE '____-__-__ __:__:__' FROM oidc_pkce;
