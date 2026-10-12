-- case: bug-hunt-8/lifecycle/reopen-basic-rows
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b REAL, c BLOB);
INSERT INTO t VALUES(1,'x',1.5,x'00ff'),(2,NULL,-0.0,x''),(3,'',1e300,NULL);
-- reopen
SELECT id, a, b, hex(c), typeof(c) FROM t ORDER BY id;
-- case: bug-hunt-8/lifecycle/reopen-after-rollback
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
BEGIN; INSERT INTO t VALUES(2); ROLLBACK;
-- reopen
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-uncommitted-transaction-lost
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
BEGIN; INSERT INTO t VALUES(2);
-- reopen
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-savepoint-release
CREATE TABLE t(a);
SAVEPOINT s1; INSERT INTO t VALUES(1); SAVEPOINT s2; INSERT INTO t VALUES(2); ROLLBACK TO s2; INSERT INTO t VALUES(3); RELEASE s1;
-- reopen
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-alter-add-column-default
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b TEXT DEFAULT 'dflt';
ALTER TABLE t ADD COLUMN c INTEGER NOT NULL DEFAULT 7;
-- reopen
INSERT INTO t(a) VALUES(3);
SELECT * FROM t;
-- reopen
SELECT a, b, c FROM t WHERE c = 7 ORDER BY a;
-- case: bug-hunt-8/lifecycle/reopen-drop-column
CREATE TABLE t(a, b, c);
CREATE INDEX ti ON t(c);
INSERT INTO t VALUES(1,2,3),(4,5,6);
ALTER TABLE t DROP COLUMN b;
-- reopen
SELECT * FROM t;
SELECT sql FROM sqlite_schema ORDER BY name;
INSERT INTO t VALUES(7,8);
-- reopen
SELECT * FROM t WHERE c > 3;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-rename-table-with-index-trigger-view
CREATE TABLE a(x PRIMARY KEY, y);
CREATE INDEX ay ON a(y);
CREATE TABLE log(m);
CREATE TRIGGER at AFTER INSERT ON a BEGIN INSERT INTO log VALUES('ins '||new.x); END;
CREATE VIEW av AS SELECT x, y FROM a WHERE y > 0;
ALTER TABLE a RENAME TO b;
-- reopen
INSERT INTO b VALUES(1, 5);
SELECT * FROM log;
SELECT * FROM av;
SELECT name, tbl_name, sql FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-8/lifecycle/reopen-rename-column
CREATE TABLE t(a, b);
CREATE INDEX tb ON t(b);
CREATE VIEW v AS SELECT b FROM t;
INSERT INTO t VALUES(1,2);
ALTER TABLE t RENAME COLUMN b TO bb;
-- reopen
SELECT * FROM v;
SELECT bb FROM t WHERE bb = 2;
SELECT sql FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-8/lifecycle/reopen-autoincrement
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES('a'),('b'),('c');
DELETE FROM t WHERE id = 3;
-- reopen
INSERT INTO t(v) VALUES('d');
SELECT * FROM t;
SELECT * FROM sqlite_sequence;
-- case: bug-hunt-8/lifecycle/reopen-rowid-reuse-without-autoincrement
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t(v) VALUES('a'),('b'),('c');
DELETE FROM t WHERE id = 3;
-- reopen
INSERT INTO t(v) VALUES('d');
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-large-blob
CREATE TABLE t(id INTEGER PRIMARY KEY, b BLOB);
INSERT INTO t VALUES(1, randomblob(300000)), (2, zeroblob(70000)), (3, x'01');
CREATE TABLE h AS SELECT id, hex(b) AS h FROM t;
-- reopen
SELECT t.id, length(t.b), hex(t.b) = h.h FROM t JOIN h USING(id) ORDER BY t.id;
UPDATE t SET b = substr(b, 1, 100) WHERE id = 1;
-- reopen
SELECT id, length(b) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-large-text-update-shrink-grow
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT);
INSERT INTO t VALUES(1, printf('%.*c', 100000, 'x'));
UPDATE t SET s = 'small';
UPDATE t SET s = printf('%.*c', 50000, 'y') WHERE id = 1;
-- reopen
SELECT length(s), substr(s, 1, 3), substr(s, -3) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-many-rows-delete-half
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 20000) INSERT INTO t SELECT i, printf('%08d', i) FROM n;
CREATE INDEX tv ON t(v);
DELETE FROM t WHERE id % 2 = 0;
-- reopen
SELECT count(*), min(id), max(id), sum(id) FROM t;
SELECT count(*) FROM t WHERE v > '00010000';
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-vacuum
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 5000) INSERT INTO t SELECT i, hex(randomblob(50)) FROM n;
DELETE FROM t WHERE id > 100;
VACUUM;
-- reopen
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-user-version-application-id
PRAGMA user_version = 42;
PRAGMA application_id = 1234;
-- reopen
PRAGMA user_version;
PRAGMA application_id;
-- case: bug-hunt-8/lifecycle/reopen-foreign-keys-pragma-not-persistent
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid REFERENCES p(id));
-- reopen
PRAGMA foreign_keys;
INSERT INTO c VALUES(99);
SELECT * FROM c;
-- case: bug-hunt-8/lifecycle/reopen-collation-index
CREATE TABLE t(name TEXT COLLATE NOCASE UNIQUE);
INSERT INTO t VALUES('Alice'),('bob');
-- reopen
INSERT INTO t VALUES('ALICE');
SELECT * FROM t WHERE name = 'BOB';
SELECT * FROM t ORDER BY name;
-- case: bug-hunt-8/lifecycle/reopen-without-rowid
CREATE TABLE kv(k TEXT PRIMARY KEY, v) WITHOUT ROWID;
INSERT INTO kv VALUES('b',2),('a',1),('c',3);
-- reopen
UPDATE kv SET v = v * 10 WHERE k = 'b';
DELETE FROM kv WHERE k = 'a';
-- reopen
SELECT * FROM kv;
-- case: bug-hunt-8/lifecycle/reopen-partial-and-expression-index
CREATE TABLE t(a, b);
CREATE INDEX tp ON t(a) WHERE b IS NOT NULL;
CREATE INDEX te ON t(lower(a));
INSERT INTO t VALUES('X', 1), ('y', NULL), ('Z', 2);
-- reopen
SELECT a FROM t WHERE lower(a) = 'x';
SELECT a FROM t WHERE a > 'A' AND b IS NOT NULL ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-generated-columns
CREATE TABLE t(a INTEGER, b INTEGER GENERATED ALWAYS AS (a * 2) VIRTUAL, c TEXT GENERATED ALWAYS AS (printf('n%d', a)) STORED);
INSERT INTO t(a) VALUES(1),(2);
-- reopen
UPDATE t SET a = 10 WHERE a = 2;
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-temp-table-gone
CREATE TEMP TABLE tt(a);
INSERT INTO tt VALUES(1);
CREATE TABLE keep(a);
-- reopen
SELECT name FROM sqlite_schema;
SELECT * FROM tt;
-- case: bug-hunt-8/lifecycle/reopen-fts5
CREATE VIRTUAL TABLE f USING fts5(title, body);
INSERT INTO f VALUES('hello world', 'the quick brown fox'), ('goodbye', 'lazy dog jumps');
-- reopen
SELECT title FROM f WHERE f MATCH 'quick';
DELETE FROM f WHERE title = 'hello world';
INSERT INTO f VALUES('again', 'quick again');
-- reopen
SELECT title FROM f WHERE f MATCH 'quick';
SELECT count(*) FROM f;
-- case: bug-hunt-8/lifecycle/reopen-drop-table-then-create-same-name
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
DROP TABLE t;
CREATE TABLE t(b, c);
INSERT INTO t VALUES(2, 3);
-- reopen
SELECT * FROM t;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-twelve-step-alter
PRAGMA foreign_keys=OFF;
CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
CREATE INDEX users_name ON users(name);
INSERT INTO users VALUES(1,'a',10),(2,'b',20);
BEGIN;
CREATE TABLE new_users(id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER CHECK(age >= 0), email TEXT);
INSERT INTO new_users(id, name, age) SELECT id, name, age FROM users;
DROP TABLE users;
ALTER TABLE new_users RENAME TO users;
CREATE INDEX users_name ON users(name);
PRAGMA foreign_key_check;
COMMIT;
-- reopen
SELECT * FROM users WHERE name = 'b';
SELECT sql FROM sqlite_schema WHERE name LIKE 'users%' ORDER BY name;
PRAGMA integrity_check;
-- case: bug-hunt-8/lifecycle/reopen-attach-write
ATTACH 'other.db' AS o;
CREATE TABLE o.t(a);
INSERT INTO o.t VALUES(1),(2);
-- reopen
ATTACH 'other.db' AS o;
SELECT * FROM o.t;
-- case: bug-hunt-8/lifecycle/reopen-vacuum-into
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
VACUUM INTO 'copy.db';
-- reopen
ATTACH 'copy.db' AS c;
SELECT * FROM c.t;
-- case: bug-hunt-8/lifecycle/reopen-strict-table
CREATE TABLE t(a INTEGER, b TEXT, c ANY) STRICT;
INSERT INTO t VALUES(1, 'x', 1.5);
-- reopen
INSERT INTO t VALUES('abc', 'y', x'00');
INSERT INTO t VALUES('12', 3, x'00');
SELECT a, typeof(a), b, typeof(b), typeof(c) FROM t;
-- case: bug-hunt-8/lifecycle/reopen-check-constraint
CREATE TABLE t(a CHECK(a > 0));
INSERT INTO t VALUES(1);
-- reopen
INSERT INTO t VALUES(-1);
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-default-expressions
CREATE TABLE t(id INTEGER PRIMARY KEY, a DEFAULT (1+2), b DEFAULT 'x', c DEFAULT (-5), d DEFAULT (lower('ABC')));
-- reopen
INSERT INTO t(id) VALUES(1);
SELECT * FROM t;
-- case: bug-hunt-8/lifecycle/reopen-index-on-many-columns-desc
CREATE TABLE t(a, b, c);
CREATE INDEX i ON t(a DESC, b ASC, c DESC);
INSERT INTO t VALUES(1,2,3),(1,1,4),(2,2,2),(1,2,5);
-- reopen
SELECT * FROM t ORDER BY a DESC, b, c DESC;
SELECT * FROM t WHERE a = 1 ORDER BY b, c DESC;
-- case: bug-hunt-8/lifecycle/reopen-update-primary-key
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES(1,'a'),(2,'b');
UPDATE t SET id = id + 10;
-- reopen
SELECT * FROM t;
SELECT * FROM t WHERE id = 11;
-- case: bug-hunt-8/lifecycle/reopen-text-pk-update
CREATE TABLE t(k TEXT PRIMARY KEY, v);
INSERT INTO t VALUES('a',1),('b',2);
UPDATE t SET k = 'z' WHERE k = 'a';
-- reopen
SELECT * FROM t ORDER BY k;
SELECT * FROM t WHERE k = 'z';
INSERT INTO t VALUES('a', 3);
SELECT * FROM t ORDER BY k;
-- case: bug-hunt-8/lifecycle/reopen-user-version-rolled-back
CREATE TABLE t(a);
PRAGMA user_version = 1;
-- reopen
BEGIN;
PRAGMA user_version = 3;
PRAGMA user_version;
ROLLBACK;
PRAGMA user_version;
-- reopen
PRAGMA user_version;
-- case: bug-hunt-8/lifecycle/reopen-application-id-rolled-back-to-savepoint
PRAGMA application_id = 5;
BEGIN;
SAVEPOINT a;
PRAGMA application_id = 77;
SAVEPOINT b;
PRAGMA application_id = 88;
ROLLBACK TO b;
PRAGMA application_id;
ROLLBACK TO a;
PRAGMA application_id;
PRAGMA application_id = 99;
COMMIT;
-- reopen
PRAGMA application_id;
-- case: bug-hunt-8/lifecycle/reopen-user-version-committed-in-migration
CREATE TABLE t(a);
BEGIN;
ALTER TABLE t ADD COLUMN b;
PRAGMA user_version = 2;
COMMIT;
-- reopen
PRAGMA user_version;
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-8/lifecycle/reopen-user-version-failed-migration
CREATE TABLE t(a UNIQUE);
INSERT INTO t VALUES(1);
PRAGMA user_version = 1;
BEGIN;
PRAGMA user_version = 2;
INSERT INTO t VALUES(1);
ROLLBACK;
-- reopen
PRAGMA user_version;
-- case: bug-hunt-8/lifecycle/vacuum-keeps-autoincrement-high-water-mark
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, a TEXT);
INSERT INTO t(a) VALUES('1'),('2'),('3');
DELETE FROM t WHERE id = 3;
VACUUM;
SELECT * FROM sqlite_sequence;
INSERT INTO t(a) VALUES('4') RETURNING id;
-- case: bug-hunt-8/lifecycle/vacuum-keeps-autoincrement-of-an-emptied-table
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, a TEXT);
INSERT INTO t(a) VALUES('1'),('2');
DELETE FROM t;
VACUUM;
-- reopen
SELECT * FROM sqlite_sequence;
INSERT INTO t(a) VALUES('3') RETURNING id;
-- case: bug-hunt-8/lifecycle/create-table-if-not-exists-sqlite-sequence-when-it-exists
CREATE TABLE a(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO a(v) VALUES(1);
CREATE TABLE IF NOT EXISTS sqlite_sequence(name, seq);
DELETE FROM sqlite_sequence;
INSERT INTO sqlite_sequence VALUES('a', 10);
INSERT INTO a(v) VALUES(2);
SELECT * FROM a;
-- case: bug-hunt-8/lifecycle/vacuum-keeps-wal-mode
PRAGMA journal_mode = WAL;
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
VACUUM;
PRAGMA journal_mode;
-- reopen
PRAGMA journal_mode;
SELECT * FROM t;
