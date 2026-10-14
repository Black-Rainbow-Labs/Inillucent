-- case: bug-hunt-10/engine/attached-foreign-key-parent-is-in-the-same-database
PRAGMA foreign_keys=ON; ATTACH ':memory:' AS m;
CREATE TABLE m.p(id PRIMARY KEY); CREATE TABLE m.c(pid REFERENCES p(id));
INSERT INTO m.p VALUES(1);
INSERT INTO m.c VALUES(1);
INSERT INTO m.c VALUES(2);
SELECT * FROM m.c;
-- case: bug-hunt-10/engine/attached-foreign-key-cascade
PRAGMA foreign_keys=ON; ATTACH ':memory:' AS m;
CREATE TABLE m.p(id PRIMARY KEY); CREATE TABLE m.c(pid REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO m.p VALUES(1),(2); INSERT INTO m.c VALUES(1),(2);
DELETE FROM m.p WHERE id=1;
SELECT * FROM m.c;
-- case: bug-hunt-10/engine/attached-foreign-key-ignores-main-table-of-the-same-name
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); INSERT INTO p VALUES(7);
ATTACH ':memory:' AS m;
CREATE TABLE m.p(id PRIMARY KEY); CREATE TABLE m.c(pid REFERENCES p(id));
INSERT INTO m.c VALUES(7);
SELECT * FROM m.c;
-- case: bug-hunt-10/engine/attached-file-foreign-key-after-reopen
PRAGMA foreign_keys=ON; ATTACH 'aux.db' AS m;
CREATE TABLE m.p(id PRIMARY KEY); CREATE TABLE m.c(pid REFERENCES p(id));
INSERT INTO m.p VALUES(1); INSERT INTO m.c VALUES(1);
-- reopen
PRAGMA foreign_keys=ON; ATTACH 'aux.db' AS m;
INSERT INTO m.c VALUES(9);
DELETE FROM m.p;
SELECT * FROM m.c;
-- case: bug-hunt-10/engine/temp-foreign-key
PRAGMA foreign_keys=ON;
CREATE TEMP TABLE p(id PRIMARY KEY); CREATE TEMP TABLE c(pid REFERENCES p(id));
INSERT INTO p VALUES(1); INSERT INTO c VALUES(1); INSERT INTO c VALUES(2);
SELECT * FROM c;
-- case: bug-hunt-10/engine/attached-deferred-foreign-key-at-commit
PRAGMA foreign_keys=ON; ATTACH ':memory:' AS m;
CREATE TABLE m.p(id PRIMARY KEY);
CREATE TABLE m.c(pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN; INSERT INTO m.c VALUES(5); INSERT INTO m.p VALUES(5); COMMIT;
BEGIN; INSERT INTO m.c VALUES(6); COMMIT;
SELECT * FROM m.c ORDER BY 1;
-- case: bug-hunt-10/engine/attached-trigger-body-writes-its-own-database
CREATE TABLE log(x);
ATTACH ':memory:' AS m;
CREATE TABLE m.t(a); CREATE TABLE m.log(x);
CREATE TRIGGER m.tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES(new.a); END;
INSERT INTO m.t VALUES(5);
SELECT 'main', * FROM main.log; SELECT 'm', * FROM m.log;
-- case: bug-hunt-10/engine/attached-trigger-body-reads-its-own-database
CREATE TABLE src(v); INSERT INTO src VALUES('main');
ATTACH ':memory:' AS m;
CREATE TABLE m.src(v); INSERT INTO m.src VALUES('m');
CREATE TABLE m.t(a); CREATE TABLE m.out(v);
CREATE TRIGGER m.tr AFTER INSERT ON t WHEN (SELECT count(*) FROM src) = 1 BEGIN INSERT INTO out SELECT v FROM src; END;
INSERT INTO m.t VALUES(1);
SELECT * FROM m.out;
-- case: bug-hunt-10/engine/temporary-trigger-body-searches-every-database
CREATE TABLE t(a); CREATE TABLE log(x);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN INSERT INTO log VALUES(new.a); END;
INSERT INTO t VALUES(3);
SELECT * FROM log;
-- case: bug-hunt-10/engine/sequence-main-after-attached
ATTACH ':memory:' AS m;
CREATE TABLE m.t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES('x'),('y'),('z');
INSERT INTO m.t(v) VALUES('a');
SELECT 'm', * FROM m.t;
SELECT 'mseq', * FROM m.sqlite_sequence;
SELECT 'mainseq', * FROM main.sqlite_sequence;
-- case: bug-hunt-10/engine/sequence-attached-after-main
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES('x'),('y');
ATTACH ':memory:' AS m;
CREATE TABLE m.u(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO m.u(v) VALUES('a');
SELECT * FROM m.u; SELECT 'mseq', * FROM m.sqlite_sequence; SELECT 'mainseq', * FROM main.sqlite_sequence;
-- case: bug-hunt-10/engine/sequence-attached-drop-and-rename
ATTACH ':memory:' AS m;
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t(v) VALUES(1),(2);
CREATE TABLE m.t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO m.t(v) VALUES(1);
ALTER TABLE m.t RENAME TO r;
SELECT 'mseq', * FROM m.sqlite_sequence; SELECT 'mainseq', * FROM main.sqlite_sequence;
DROP TABLE m.r;
SELECT 'mseq', * FROM m.sqlite_sequence; SELECT 'mainseq', * FROM main.sqlite_sequence;
-- case: bug-hunt-10/engine/sequence-temp
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t(v) VALUES(1),(2),(3);
CREATE TEMP TABLE t2(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t2(v) VALUES(1);
SELECT * FROM t2; SELECT 'tseq', * FROM temp.sqlite_sequence; SELECT 'mainseq', * FROM main.sqlite_sequence;
-- case: bug-hunt-10/engine/sequence-attached-file-reopen
ATTACH 'aux.db' AS m;
CREATE TABLE m.t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES(1),(2),(3); INSERT INTO m.t(v) VALUES(1);
-- reopen
ATTACH 'aux.db' AS m;
INSERT INTO m.t(v) VALUES(2); INSERT INTO t(v) VALUES(4);
SELECT 'm', * FROM m.t; SELECT 'main', * FROM main.t;
SELECT 'mseq', * FROM m.sqlite_sequence; SELECT 'mainseq', * FROM main.sqlite_sequence;
-- case: bug-hunt-10/engine/rollback-drop-keeps-sequence
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t(v) VALUES(1),(2),(3),(4),(5);
DELETE FROM t WHERE id > 2;
BEGIN; DROP TABLE t; ROLLBACK;
SELECT * FROM sqlite_sequence;
INSERT INTO t(v) VALUES(6); SELECT * FROM t;
-- case: bug-hunt-10/engine/rollback-to-savepoint-drop-keeps-sequence
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t(v) VALUES(1),(2),(3),(4),(5);
DELETE FROM t WHERE id > 2;
BEGIN; SAVEPOINT s; DROP TABLE t; ROLLBACK TO s; COMMIT;
INSERT INTO t(v) VALUES(6); SELECT * FROM t;
-- case: bug-hunt-10/engine/rollback-drop-table-with-fts
CREATE VIRTUAL TABLE f USING fts5(x); INSERT INTO f VALUES('hello world');
BEGIN; DROP TABLE f; ROLLBACK;
SELECT * FROM f WHERE f MATCH 'hello';
-- case: bug-hunt-10/engine/attached-drop-autoincrement-table
ATTACH ':memory:' AS m; CREATE TABLE m.k(a);
CREATE TABLE m.t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO m.t(v) VALUES(1); INSERT INTO m.k VALUES(1);
DROP TABLE m.t;
SELECT * FROM m.k; SELECT * FROM m.sqlite_sequence; PRAGMA m.integrity_check;
-- case: bug-hunt-10/engine/cyclic-key-failed-insert-with-trigger
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, q INTEGER REFERENCES p(id) ON DELETE CASCADE);
CREATE TABLE c(id INTEGER PRIMARY KEY, p INTEGER REFERENCES p(id));
CREATE TRIGGER t AFTER INSERT ON c BEGIN SELECT 1; END;
INSERT INTO c(p) VALUES(1);
SELECT count(*) FROM c;
-- reopen
SELECT count(*) FROM c;
-- case: bug-hunt-10/engine/cyclic-key-elsewhere-failed-insert-through-trigger
PRAGMA foreign_keys=ON;
CREATE TABLE z(id INTEGER PRIMARY KEY, q REFERENCES z(id) ON DELETE CASCADE);
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(id INTEGER PRIMARY KEY, p REFERENCES p(id));
CREATE TABLE d(x); CREATE TABLE log(m);
CREATE TRIGGER t AFTER INSERT ON d BEGIN INSERT INTO log VALUES('before'); INSERT INTO c(p) VALUES(new.x); END;
INSERT INTO d VALUES(5);
SELECT count(*) FROM c; SELECT count(*) FROM d; SELECT count(*) FROM log;
-- case: bug-hunt-10/engine/cyclic-key-failed-insert-multi-row
PRAGMA foreign_keys=ON;
CREATE TABLE z(id INTEGER PRIMARY KEY, q REFERENCES z(id) ON DELETE SET NULL);
CREATE TABLE p(id INTEGER PRIMARY KEY); INSERT INTO p VALUES(1);
CREATE TABLE c(id INTEGER PRIMARY KEY, p REFERENCES p(id));
CREATE TRIGGER t BEFORE INSERT ON c BEGIN SELECT 1; END;
INSERT INTO c(p) VALUES(1),(2);
SELECT count(*) FROM c;
-- case: bug-hunt-10/engine/cyclic-cascade-through-trigger-still-settles
PRAGMA foreign_keys=ON;
CREATE TABLE node(id INTEGER PRIMARY KEY, parent REFERENCES node(id) ON DELETE CASCADE);
INSERT INTO node VALUES(1,NULL),(2,1),(3,2),(4,3),(5,4);
CREATE TABLE cmd(id);
CREATE TRIGGER del AFTER INSERT ON cmd BEGIN DELETE FROM node WHERE id = new.id; END;
INSERT INTO cmd VALUES(2);
SELECT id FROM node ORDER BY id;
-- case: bug-hunt-10/engine/cyclic-cascade-in-transaction-rollback
PRAGMA foreign_keys=ON;
CREATE TABLE node(id INTEGER PRIMARY KEY, parent REFERENCES node(id) ON DELETE CASCADE);
INSERT INTO node VALUES(1,NULL),(2,1),(3,2),(4,3);
BEGIN; DELETE FROM node WHERE id = 2; SELECT count(*) FROM node; ROLLBACK;
SELECT count(*) FROM node;
-- case: bug-hunt-10/engine/fk-drop-both-rollback
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id));
INSERT INTO p VALUES(1); INSERT INTO c VALUES(1);
BEGIN; DROP TABLE c; DROP TABLE p; ROLLBACK;
SELECT 'c', * FROM c; SELECT 'p', * FROM p; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/fk-drop-parent-cascade-rollback
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES(1),(2); INSERT INTO c VALUES(1),(2);
BEGIN; DROP TABLE p; ROLLBACK;
SELECT 'c', * FROM c ORDER BY 2; SELECT 'p', * FROM p ORDER BY 2; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/fk-drop-parent-cascade-rollback-reopen
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY, v); CREATE INDEX pv ON p(v); CREATE TABLE c(pid REFERENCES p(id) ON DELETE CASCADE);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<3000) INSERT INTO p SELECT n, 'v'||n FROM r;
INSERT INTO c SELECT id FROM p WHERE id % 3 = 0;
BEGIN; DROP TABLE p; ROLLBACK;
SELECT count(*), sum(id) FROM p; SELECT count(*) FROM c; SELECT id FROM p WHERE v = 'v2999'; PRAGMA integrity_check;
-- reopen
PRAGMA foreign_keys=ON;
SELECT count(*), sum(id) FROM p; SELECT count(*) FROM c; SELECT id FROM p WHERE v = 'v2999'; PRAGMA integrity_check;
DELETE FROM p WHERE id = 3; SELECT count(*) FROM c;
-- case: bug-hunt-10/engine/fk-drop-parent-rollback-to-savepoint
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) ON DELETE SET NULL);
INSERT INTO p VALUES(1); INSERT INTO c VALUES(1);
BEGIN; SAVEPOINT s; DROP TABLE p; ROLLBACK TO s; COMMIT;
SELECT 'c', * FROM c; SELECT 'p', * FROM p;
-- case: bug-hunt-10/engine/fk-drop-child-rollback
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id));
INSERT INTO p VALUES(1); INSERT INTO c VALUES(1),(1);
BEGIN; DROP TABLE c; ROLLBACK;
SELECT count(*) FROM c;
-- case: bug-hunt-10/engine/fk-drop-create-same-name-rollback
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES(1); INSERT INTO c VALUES(1);
BEGIN; DROP TABLE p; CREATE TABLE p(id PRIMARY KEY, extra); INSERT INTO p VALUES(5,'x'); ROLLBACK;
SELECT * FROM p; SELECT * FROM c; PRAGMA table_info(p);
-- case: bug-hunt-10/engine/create-insert-drop-rollback
PRAGMA foreign_keys=ON; CREATE TABLE p(id PRIMARY KEY);
BEGIN; CREATE TABLE x(a REFERENCES p(id)); INSERT INTO p VALUES(1); INSERT INTO x VALUES(1); DROP TABLE x; ROLLBACK;
SELECT name FROM sqlite_schema ORDER BY 1; SELECT count(*) FROM p;
-- case: bug-hunt-10/engine/insert-then-drop-column-rollback
CREATE TABLE t(a, b, c); INSERT INTO t VALUES(1,2,3);
BEGIN; INSERT INTO t VALUES(4,5,6); DELETE FROM t WHERE a=1; ALTER TABLE t DROP COLUMN c; ROLLBACK;
SELECT * FROM t ORDER BY a; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/insert-then-add-column-rollback
CREATE TABLE t(a, b); INSERT INTO t VALUES(1,2);
BEGIN; INSERT INTO t VALUES(4,5); UPDATE t SET b=9 WHERE a=1; ALTER TABLE t ADD COLUMN c DEFAULT 7; ROLLBACK;
SELECT * FROM t ORDER BY a; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/insert-then-add-column-then-insert-rollback-to
CREATE TABLE t(a, b); INSERT INTO t VALUES(1,2);
BEGIN; INSERT INTO t VALUES(4,5); SAVEPOINT s; ALTER TABLE t ADD COLUMN c DEFAULT 7; INSERT INTO t VALUES(8,8,8); ROLLBACK TO s; INSERT INTO t VALUES(10,10); COMMIT;
SELECT * FROM t ORDER BY a; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/many-rows-then-drop-column-rollback
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c); CREATE INDEX tb ON t(b);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<2000) INSERT INTO t SELECT n, n%10, 'x'||n FROM r;
BEGIN; DELETE FROM t WHERE a % 2 = 0; UPDATE t SET b = b + 100; ALTER TABLE t DROP COLUMN c; ROLLBACK;
SELECT count(*), sum(b) FROM t; SELECT count(*) FROM t WHERE b = 3; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/insert-then-rename-rollback
CREATE TABLE t(a); INSERT INTO t VALUES(1);
BEGIN; INSERT INTO t VALUES(2); ALTER TABLE t RENAME TO u; INSERT INTO u VALUES(3); ROLLBACK;
SELECT * FROM t ORDER BY 1;
-- case: bug-hunt-10/engine/update-then-drop-index-rollback
CREATE TABLE t(a, b); CREATE INDEX ib ON t(b); INSERT INTO t VALUES(1,1),(2,2);
BEGIN; UPDATE t SET b = b + 10; DROP INDEX ib; ROLLBACK;
SELECT a FROM t WHERE b = 1; SELECT a FROM t WHERE b = 11; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/delete-then-drop-index-rollback-unique
CREATE TABLE t(a, b UNIQUE); INSERT INTO t VALUES(1,1),(2,2);
BEGIN; DELETE FROM t WHERE a = 1; INSERT INTO t VALUES(3,1); CREATE INDEX x ON t(a); DROP TABLE t; ROLLBACK;
SELECT * FROM t ORDER BY a; INSERT INTO t VALUES(4,2); PRAGMA integrity_check;
-- case: bug-hunt-10/engine/insert-then-drop-table-rollback-no-fk
CREATE TABLE t(a); CREATE INDEX ta ON t(a); INSERT INTO t VALUES(1);
BEGIN; INSERT INTO t VALUES(2); DELETE FROM t WHERE a=1; DROP TABLE t; ROLLBACK;
SELECT * FROM t; SELECT * FROM t WHERE a = 1; PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-1
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a TEXT, b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
BEGIN;
DELETE FROM p WHERE id % 4 = 0;
SAVEPOINT s;
INSERT INTO p(v, w) VALUES(3, 'n767');
ALTER TABLE p DROP COLUMN w;
DROP INDEX IF EXISTS t_b;
DELETE FROM t WHERE a = 2;
INSERT INTO c(pid, x) VALUES(4, 9);
INSERT INTO c(pid, x) VALUES(4, 49);
ROLLBACK TO s; COMMIT;
BEGIN;
SAVEPOINT s;
DROP INDEX IF EXISTS t_b;
ALTER TABLE p DROP COLUMN w;
DELETE FROM c WHERE x > 1;
ALTER TABLE t ADD COLUMN d622 DEFAULT 7;
ALTER TABLE p DROP COLUMN w;
UPDATE p SET id = id + 100000 WHERE id % 5 = 1;
COMMIT;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-7
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON UPDATE CASCADE, x);
CREATE TABLE t(a TEXT, b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX t_b ON t(b);
CREATE INDEX p_v ON p(v DESC);
BEGIN;
DROP TABLE IF EXISTS c;
DROP INDEX IF EXISTS t_b;
UPDATE p SET v = v + 1 WHERE id < 5;
DELETE FROM p WHERE id % 2 = 0;
INSERT OR REPLACE INTO t(a,b,c) VALUES(2,27,16);
UPDATE t SET b = 27 WHERE rowid % 4 = 3;
ROLLBACK;
BEGIN;
INSERT INTO p(v, w) VALUES(0, 'n768');
INSERT INTO c(pid, x) VALUES(5, 35);
DROP INDEX IF EXISTS t_b;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-10
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a TEXT, b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX t_b ON t(b);
CREATE INDEX p_v ON p(v DESC);
BEGIN;
DROP INDEX IF EXISTS t_b;
UPDATE t SET a = 24 WHERE rowid % 4 = 3;
SAVEPOINT s;
INSERT OR REPLACE INTO t(a,b,c) VALUES(15,14,8);
DROP INDEX IF EXISTS t_b;
INSERT INTO p(v, w) VALUES(4, 'n38');
ALTER TABLE p DROP COLUMN w;
ALTER TABLE t ADD COLUMN d142 DEFAULT 6;
ROLLBACK TO s; COMMIT;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-14
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a INTEGER, b, c, PRIMARY KEY(a, b));
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<40) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<40) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX p_v ON p(v DESC);
BEGIN;
DROP TABLE IF EXISTS c;
SAVEPOINT s;
DROP INDEX IF EXISTS t_b;
UPDATE t SET a = 19 WHERE rowid % 4 = 1;
UPDATE p SET id = id + 100000 WHERE id % 9 = 1;
UPDATE t SET a = 6 WHERE rowid % 4 = 0;
UPDATE p SET v = v + 1 WHERE id < 7;
UPDATE p SET v = v + 1 WHERE id < 3;
ROLLBACK;
BEGIN;
UPDATE p SET v = v + 1 WHERE id < 20;
DROP INDEX IF EXISTS t_b;
DELETE FROM p WHERE id % 4 = 0;
DROP INDEX IF EXISTS t_b;
DROP INDEX IF EXISTS t_b;
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
COMMIT;
BEGIN;
INSERT INTO c(pid, x) VALUES(29, 13);
DROP TABLE IF EXISTS c;
INSERT INTO p(v, w) VALUES(1, 'n547');
INSERT INTO c(pid, x) VALUES(1, 47);
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-16
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON UPDATE CASCADE, x);
CREATE TABLE t(a INTEGER, b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
BEGIN;
CREATE INDEX IF NOT EXISTS t_c ON t(c);
DELETE FROM t WHERE b = 4;
DROP TABLE IF EXISTS c;
INSERT OR REPLACE INTO t(a,b,c) VALUES(14,15,24);
ALTER TABLE t ADD COLUMN d868 DEFAULT 1;
INSERT OR REPLACE INTO t(a,b,c,d868) VALUES(14,16,19,23);
DELETE FROM t WHERE c = 5;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-17
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a , b, c, PRIMARY KEY(a, b));
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX t_b ON t(b);
BEGIN;
CREATE INDEX IF NOT EXISTS t_c ON t(c);
SAVEPOINT s;
INSERT OR REPLACE INTO t(a,b,c) VALUES(8,14,20);
DELETE FROM c WHERE x > 6;
UPDATE p SET id = id + 100000 WHERE id % 3 = 1;
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
DROP INDEX IF EXISTS t_b;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-18
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a , b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<400) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<400) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX p_v ON p(v DESC);
BEGIN;
DELETE FROM p WHERE id % 4 = 1;
SAVEPOINT s;
INSERT OR REPLACE INTO t(a,b,c) VALUES(26,14,3);
UPDATE p SET v = v + 1 WHERE id < 357;
ALTER TABLE p ADD COLUMN z DEFAULT 1;
ALTER TABLE p DROP COLUMN w;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-19
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE CASCADE, x);
CREATE TABLE t(a TEXT, b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<40) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<40) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
BEGIN;
SAVEPOINT s;
DELETE FROM c WHERE x > 31;
INSERT INTO p(v, w) VALUES(0, 'n503');
DELETE FROM t WHERE b = 2;
ALTER TABLE p DROP COLUMN w;
UPDATE p SET v = v + 1 WHERE id < 7;
UPDATE p SET v = v + 1 WHERE id < 39;
ROLLBACK;
BEGIN;
DELETE FROM t WHERE a = 3;
INSERT INTO c(pid, x) VALUES(16, 9);
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-23
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a , b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<5) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
BEGIN;
ALTER TABLE t ADD COLUMN d985 DEFAULT 7;
INSERT INTO p(v, w) VALUES(4, 'n655');
COMMIT;
BEGIN;
ALTER TABLE t DROP COLUMN d985;
DROP TABLE IF EXISTS c;
UPDATE t SET c = 26 WHERE rowid % 4 = 3;
ROLLBACK;
BEGIN;
CREATE INDEX IF NOT EXISTS t_c ON t(c);
UPDATE t SET d985 = 19 WHERE rowid % 4 = 1;
DROP INDEX IF EXISTS t_b;
DROP TABLE IF EXISTS c;
INSERT INTO p(v, w) VALUES(5, 'n530');
UPDATE p SET v = v + 1 WHERE id < 1;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-24
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE CASCADE, x);
CREATE TABLE t(a , b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX t_b ON t(b);
CREATE INDEX p_v ON p(v DESC);
BEGIN;
ALTER TABLE t ADD COLUMN d438 DEFAULT 6;
UPDATE t SET c = 9 WHERE rowid % 4 = 3;
DELETE FROM c WHERE x > 1975;
SAVEPOINT s;
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
DELETE FROM t WHERE a = 2;
DELETE FROM t WHERE b = 5;
ROLLBACK TO s; COMMIT;
BEGIN;
INSERT INTO c(pid, x) VALUES(559, 44);
INSERT INTO c(pid, x) VALUES(489, 31);
SAVEPOINT s;
INSERT INTO c(pid, x) VALUES(555, 47);
INSERT OR REPLACE INTO t(a,b,c,d438) VALUES(0,29,21,26);
UPDATE p SET id = id + 100000 WHERE id % 3 = 1;
DELETE FROM c WHERE x > 2850;
ROLLBACK TO s; COMMIT;
BEGIN;
DELETE FROM t WHERE a = 10;
UPDATE p SET id = id + 100000 WHERE id % 4 = 1;
SAVEPOINT s;
UPDATE p SET v = v + 1 WHERE id < 1295;
UPDATE p SET id = id + 100000 WHERE id % 6 = 1;
ALTER TABLE p ADD COLUMN z DEFAULT 1;
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
ROLLBACK TO s; COMMIT;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-25
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a TEXT, b, c, PRIMARY KEY(a, b)) WITHOUT ROWID;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<1500) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
CREATE INDEX t_b ON t(b);
CREATE INDEX p_v ON p(v DESC);
BEGIN;
DROP INDEX IF EXISTS p_v;
SAVEPOINT s;
UPDATE p SET v = v + 1 WHERE id < 360;
INSERT INTO c(pid, x) VALUES(1378, 44);
COMMIT;
BEGIN;
SAVEPOINT s;
CREATE INDEX IF NOT EXISTS t_c ON t(c);
DELETE FROM p WHERE id % 3 = 1;
ALTER TABLE p DROP COLUMN w;
DELETE FROM t WHERE b = 2;
UPDATE t SET a = 13 WHERE rowid % 4 = 2;
UPDATE t SET a = 27 WHERE rowid % 4 = 2;
DELETE FROM t WHERE c = 3;
ROLLBACK;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-10/engine/random-transaction-26
CREATE TABLE p(id INTEGER PRIMARY KEY, v, w);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE SET NULL, x);
CREATE TABLE t(a , b, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<400) INSERT INTO p SELECT n, n % 7, printf('w%04d', n) FROM r;
INSERT INTO c SELECT id, id, id * 2 FROM p WHERE id % 3 = 0;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n<400) INSERT OR IGNORE INTO t SELECT n, n % 11, 'c' || n FROM r;
BEGIN;
CREATE TABLE IF NOT EXISTS extra(q); INSERT INTO extra VALUES(1);
INSERT INTO p(v, w) VALUES(5, 'n635');
ALTER TABLE p ADD COLUMN z DEFAULT 1;
UPDATE t SET b = 7 WHERE rowid % 4 = 2;
DELETE FROM t WHERE a = 4;
SAVEPOINT s;
DROP INDEX IF EXISTS p_v;
ALTER TABLE p ADD COLUMN z DEFAULT 1;
ROLLBACK;
BEGIN;
DELETE FROM t WHERE a = 4;
INSERT OR REPLACE INTO t(a,b,c) VALUES(17,5,7);
INSERT INTO c(pid, x) VALUES(63, 38);
SAVEPOINT s;
INSERT INTO c(pid, x) VALUES(20, 2);
ROLLBACK;
BEGIN;
SAVEPOINT s;
UPDATE p SET v = v + 1 WHERE id < 306;
UPDATE p SET v = v + 1 WHERE id < 170;
INSERT INTO c(pid, x) VALUES(167, 39);
INSERT OR REPLACE INTO t(a,b,c) VALUES(25,28,19);
DELETE FROM c WHERE x > 530;
INSERT OR REPLACE INTO t(a,b,c) VALUES(4,16,29);
ROLLBACK TO s; COMMIT;
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*), sum(id) FROM c;
SELECT * FROM t ORDER BY 1, 2, 3 LIMIT 30;
SELECT count(*) FROM t;
SELECT name FROM sqlite_schema ORDER BY 1;
PRAGMA integrity_check;
-- reopen
SELECT count(*), sum(id), sum(v) FROM p;
SELECT count(*) FROM t;
PRAGMA integrity_check;
