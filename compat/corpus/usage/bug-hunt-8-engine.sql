-- case: bug-hunt-8/engine/in-list-equal-after-affinity-over-an-index
CREATE TABLE t(id INTEGER PRIMARY KEY, n INTEGER, x);
CREATE INDEX tn ON t(n);
INSERT INTO t VALUES(1,1,10),(2,2,20);
SELECT count(*) FROM t WHERE n IN (1, '1');
SELECT count(*) FROM t WHERE n IN (1, 1.0);
SELECT count(*) FROM t WHERE n IN ('2', 2, 2.0, '2.0');
SELECT id FROM t WHERE n IN (1, '1') ORDER BY n;
UPDATE t SET x = x + 1 WHERE n IN (1, '1');
SELECT * FROM t ORDER BY id;
DELETE FROM t WHERE n IN (2, '2') RETURNING id;
-- case: bug-hunt-8/engine/in-list-equal-under-nocase-over-an-index
CREATE TABLE u(id INTEGER PRIMARY KEY, email TEXT COLLATE NOCASE);
CREATE INDEX ue ON u(email);
INSERT INTO u VALUES(1, 'a@x.com'), (2, 'b@x.com');
SELECT count(*) FROM u WHERE email IN ('a@x.com', 'A@X.COM');
SELECT id FROM u WHERE email IN ('B@x.com', 'b@X.com', 'a@x.com') ORDER BY email;
-- case: bug-hunt-8/engine/in-list-equal-after-affinity-on-a-rowid
CREATE TABLE r(id INTEGER PRIMARY KEY, v);
INSERT INTO r VALUES(1,'a'),(2,'b');
SELECT count(*) FROM r WHERE id IN (1, '1', 1.0);
SELECT v FROM r WHERE rowid IN ('2', 2) ORDER BY id;
-- case: bug-hunt-8/engine/in-list-equal-after-affinity-on-a-composite-index
CREATE TABLE c(a INTEGER, b TEXT, v);
CREATE INDEX cab ON c(a, b);
INSERT INTO c VALUES(1,'x',1),(1,'y',2),(2,'x',3);
SELECT count(*) FROM c WHERE a IN (1, '1') AND b = 'x';
SELECT count(*) FROM c WHERE a = 1 AND b IN ('x', 'x');
SELECT sum(v) FROM c WHERE a IN (1, 1.0, '1') AND b IN ('x', 'y');
-- case: bug-hunt-8/engine/insert-names-the-rowid-of-an-integer-primary-key-table
CREATE TABLE old(id INTEGER PRIMARY KEY, a, b);
INSERT INTO old VALUES(10,'x','p'),(20,'y','q'),(30,'z','r');
CREATE TABLE new(id INTEGER PRIMARY KEY, a, b);
INSERT INTO new(rowid, a, b) SELECT rowid, a, b FROM old;
SELECT * FROM new;
INSERT INTO new(oid, a) VALUES(40, 'w');
INSERT INTO new(_rowid_, a) VALUES(50, 'v');
REPLACE INTO new(rowid, a) VALUES(50, 'u');
SELECT * FROM new WHERE id >= 40;
-- case: bug-hunt-8/engine/insert-names-the-rowid-with-text
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t(rowid, a) VALUES('abc', 1);
INSERT INTO t(rowid, a) VALUES('7', 2);
SELECT * FROM t;
-- case: bug-hunt-8/engine/insert-names-both-rowid-and-the-key
CREATE TABLE t(id INTEGER PRIMARY KEY, a);
INSERT INTO t(rowid, id, a) VALUES(5, 6, 'x');
SELECT * FROM t;
-- case: bug-hunt-8/engine/fk-action-order-D1-after-delete-trigger-on-parent-sees-children-not-yet-cascaded
PRAGMA foreign_keys=ON;
CREATE TABLE a (id INTEGER PRIMARY KEY);
CREATE TABLE b (id INTEGER PRIMARY KEY, p INTEGER REFERENCES a(id) ON DELETE CASCADE);
CREATE TABLE log (id INTEGER PRIMARY KEY, n INT);
CREATE TRIGGER ta AFTER DELETE ON a BEGIN INSERT INTO log(n) SELECT count(*) FROM b WHERE p=old.id; END;
INSERT INTO a VALUES (1); INSERT INTO b VALUES (10,1),(11,1);
DELETE FROM a WHERE id=1;
SELECT * FROM log;
-- case: bug-hunt-8/engine/fk-action-order-D1b-cascade-trigger-firing-order
PRAGMA foreign_keys=ON;
CREATE TABLE a (id INTEGER PRIMARY KEY);
CREATE TABLE b (id INTEGER PRIMARY KEY, p INTEGER REFERENCES a(id) ON DELETE CASCADE);
CREATE TABLE log (id INTEGER PRIMARY KEY, t TEXT);
CREATE TRIGGER tb AFTER DELETE ON b BEGIN INSERT INTO log(t) VALUES ('b'||old.id); END;
CREATE TRIGGER ta AFTER DELETE ON a BEGIN INSERT INTO log(t) VALUES ('a'||old.id); END;
INSERT INTO a VALUES (1); INSERT INTO b VALUES (10,1);
DELETE FROM a WHERE id=1;
SELECT * FROM log ORDER BY id;
-- case: bug-hunt-8/engine/fk-action-order-D1c-delete-returning-subquery-sees-cascaded-children
PRAGMA foreign_keys=ON;
CREATE TABLE a (id INTEGER PRIMARY KEY, n TEXT);
CREATE TABLE b (id INTEGER PRIMARY KEY, p INTEGER REFERENCES a(id) ON DELETE CASCADE);
INSERT INTO a VALUES (1,'x'),(2,'y'); INSERT INTO b VALUES (1,1),(2,1),(3,2);
DELETE FROM a WHERE id=1 RETURNING id, n, (SELECT count(*) FROM b);
-- case: bug-hunt-8/engine/fk-action-order-D1d-after-update-trigger-on-parent-key-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE a (id INTEGER PRIMARY KEY);
CREATE TABLE b (id INTEGER PRIMARY KEY, p INTEGER REFERENCES a(id) ON UPDATE CASCADE);
CREATE TABLE log (id INTEGER PRIMARY KEY, n INT, m INT);
CREATE TRIGGER ta AFTER UPDATE ON a BEGIN INSERT INTO log(n,m) SELECT (SELECT count(*) FROM b WHERE p = old.id), (SELECT count(*) FROM b WHERE p = new.id); END;
INSERT INTO a VALUES (1); INSERT INTO b VALUES (10,1),(11,1);
UPDATE a SET id = 2;
SELECT * FROM log;
-- case: bug-hunt-8/engine/dump-reload-creates-sqlite-sequence-if-not-exists
.dbconfig defensive off
CREATE TABLE a(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO a(v) VALUES(1),(2);
PRAGMA writable_schema=ON;
CREATE TABLE IF NOT EXISTS sqlite_sequence(name,seq);
DELETE FROM sqlite_sequence;
INSERT INTO sqlite_sequence VALUES('a',7);
PRAGMA writable_schema=OFF;
INSERT INTO a(v) VALUES(3);
SELECT * FROM a;
CREATE TABLE IF NOT EXISTS sqlite_sequence(name,seq);
-- case: bug-hunt-8/engine/detach-of-a-database-the-transaction-wrote-is-refused
ATTACH ':memory:' AS m2;
CREATE TABLE m2.t(a);
BEGIN;
INSERT INTO m2.t VALUES(1);
DETACH m2;
SELECT * FROM m2.t;
COMMIT;
DETACH m2;
-- case: bug-hunt-8/engine/distinct-over-index-filled-by-many-processes
CREATE TABLE t(id INTEGER PRIMARY KEY, r INT, k INT, v TEXT);
CREATE INDEX tr ON t(r);
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 1, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 2, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 3, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 4, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 5, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 6, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 7, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 8, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 9, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 10, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 11, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 12, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 13, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 14, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 15, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 16, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 17, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 18, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 19, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 20, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 21, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 22, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 23, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 24, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 25, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 26, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 27, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 28, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 29, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 30, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 31, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 32, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 33, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 34, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 35, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 36, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 37, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 38, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 39, i, 'payload ' || i FROM n;
-- reopen
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100) INSERT INTO t(r, k, v) SELECT 40, i, 'payload ' || i FROM n;
-- reopen
SELECT count(*) FROM (SELECT DISTINCT r FROM t);
SELECT group_concat(r) FROM (SELECT DISTINCT r FROM t);
-- case: bug-hunt-8/engine/detach-of-an-earlier-database-after-a-later-one-was-written-commits
ATTACH ':memory:' AS a1;
ATTACH ':memory:' AS a2;
CREATE TABLE a2.t(x);
BEGIN;
INSERT INTO a2.t VALUES(1);
DETACH a1;
SELECT * FROM a2.t;
COMMIT;
SELECT * FROM a2.t;
SELECT name FROM pragma_database_list ORDER BY seq;
-- case: bug-hunt-8/engine/detach-of-an-earlier-database-after-a-later-one-was-written-rolls-back
ATTACH ':memory:' AS a1;
ATTACH ':memory:' AS a2;
CREATE TABLE a2.t(x);
INSERT INTO a2.t VALUES(1);
BEGIN;
INSERT INTO a2.t VALUES(2);
CREATE TABLE a2.v(z);
INSERT INTO a2.v VALUES(9);
DETACH a1;
ROLLBACK;
SELECT * FROM a2.t;
SELECT name FROM a2.sqlite_schema ORDER BY name;
-- case: bug-hunt-8/engine/detach-of-an-earlier-database-after-a-later-one-dropped-and-created
ATTACH ':memory:' AS a1;
ATTACH ':memory:' AS a2;
CREATE TABLE a2.t(x);
INSERT INTO a2.t VALUES(1);
BEGIN;
DROP TABLE a2.t;
CREATE TABLE a2.v(z);
INSERT INTO a2.v VALUES(9);
DETACH a1;
COMMIT;
SELECT name FROM a2.sqlite_schema ORDER BY name;
SELECT * FROM a2.v;
PRAGMA a2.integrity_check;
