-- case: bug-hunt-10/vacuum/attached-memory-schema
CREATE TABLE main_t(y);
INSERT INTO main_t VALUES(1);
ATTACH ':memory:' AS m;
CREATE TABLE m.t(a);
INSERT INTO m.t VALUES(1);
VACUUM m;
SELECT name FROM m.sqlite_master;
SELECT * FROM m.t;
-- case: bug-hunt-10/vacuum/temp-schema
CREATE TABLE main_t(y);
INSERT INTO main_t VALUES(1);
CREATE TEMP TABLE tt(a);
INSERT INTO tt VALUES(1);
VACUUM temp;
SELECT * FROM tt;
SELECT name FROM temp.sqlite_master;
SELECT name FROM main.sqlite_master;
-- case: bug-hunt-10/vacuum/attached-keeps-its-own-catalog
CREATE TABLE mn(y);
INSERT INTO mn VALUES('main');
ATTACH ':memory:' AS m;
CREATE TABLE m.att(x);
INSERT INTO m.att VALUES(1),(2),(3);
VACUUM m;
SELECT name FROM m.sqlite_master;
SELECT * FROM m.att;
SELECT * FROM main.mn;
-- case: bug-hunt-10/vacuum/attached-file-keeps-its-rows-and-passes-integrity-check
CREATE TABLE mn(y);
INSERT INTO mn VALUES('main');
ATTACH 'vacuum10-aux.db' AS a;
CREATE TABLE a.t(x, y);
CREATE INDEX a.ti ON t(y);
INSERT INTO a.t SELECT value, value * 2 FROM generate_series(1, 3000);
DELETE FROM a.t WHERE x % 3 <> 0;
VACUUM a;
SELECT count(*), sum(x) FROM a.t;
SELECT name FROM a.sqlite_master ORDER BY 1;
SELECT name FROM main.sqlite_master ORDER BY 1;
PRAGMA a.integrity_check;
-- reopen
ATTACH 'vacuum10-aux.db' AS a;
SELECT count(*), sum(x) FROM a.t;
PRAGMA a.integrity_check;
SELECT * FROM main.mn;
-- case: bug-hunt-10/vacuum/attached-file-keeps-user-version-and-sequence
ATTACH 'vacuum10-seq.db' AS a;
CREATE TABLE a.s(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO a.s(v) VALUES(1),(2),(3);
DELETE FROM a.s WHERE id = 3;
PRAGMA a.user_version = 77;
VACUUM a;
PRAGMA a.user_version;
SELECT name, seq FROM a.sqlite_sequence;
-- reopen
ATTACH 'vacuum10-seq.db' AS a;
PRAGMA a.user_version;
SELECT name, seq FROM a.sqlite_sequence;
PRAGMA a.integrity_check;
-- case: bug-hunt-10/vacuum/into-attached-writes-the-attached-database
CREATE TABLE mm(y);
INSERT INTO mm VALUES('main-row');
ATTACH ':memory:' AS m;
CREATE TABLE m.mt(x);
INSERT INTO m.mt VALUES('attached-row');
VACUUM m INTO 'vacuum10-into-attached.db';
ATTACH 'vacuum10-into-attached.db' AS copy;
SELECT name FROM copy.sqlite_master;
SELECT * FROM copy.mt;
-- case: bug-hunt-10/vacuum/into-attached-file
ATTACH 'vacuum10-src.db' AS a;
CREATE TABLE a.t(x, y UNIQUE);
CREATE VIEW a.v AS SELECT x FROM t;
CREATE TABLE mm(y);
INSERT INTO a.t SELECT value, value FROM generate_series(1, 500);
INSERT INTO mm VALUES('main-row');
VACUUM a INTO 'vacuum10-into-file.db';
-- reopen
ATTACH 'vacuum10-into-file.db' AS copy;
SELECT name FROM copy.sqlite_master ORDER BY 1;
SELECT count(*) FROM copy.t;
SELECT count(*) FROM copy.v;
PRAGMA copy.integrity_check;
-- case: bug-hunt-10/vacuum/into-temp-leaves-main-out
CREATE TABLE mm(y);
INSERT INTO mm VALUES('main-row');
CREATE TEMP TABLE tt(a);
INSERT INTO tt VALUES('temp-row');
VACUUM temp INTO 'vacuum10-into-temp.db';
ATTACH 'vacuum10-into-temp.db' AS copy;
SELECT count(*) FROM copy.sqlite_master WHERE name = 'mm';
-- case: bug-hunt-10/vacuum/unknown-schema
VACUUM nosuch;
SELECT 'done';
-- case: bug-hunt-10/vacuum/into-unknown-schema
VACUUM nosuch INTO 'vacuum10-nowhere.db';
SELECT 'done';
-- case: bug-hunt-10/vacuum/drop-autoincrement-table-attached
ATTACH ':memory:' AS m;
CREATE TABLE m.k(a);
CREATE TABLE m.t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO m.t(v) VALUES(1);
INSERT INTO m.t(v) VALUES(1);
DROP TABLE m.t;
SELECT name FROM m.sqlite_master;
-- case: bug-hunt-10/vacuum/fts5-merge-command-without-rank
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f(f) VALUES('merge');
SELECT 'done';
-- case: bug-hunt-10/vacuum/integrity-check-in-transaction-after-drop-table
CREATE TABLE t1(c0);
BEGIN;
DROP TABLE t1;
PRAGMA integrity_check;
COMMIT;
PRAGMA integrity_check;
-- case: bug-hunt-10/vacuum/integrity-check-in-transaction-after-drop-index
CREATE TABLE t1(c0);
CREATE INDEX i ON t1(c0);
BEGIN;
DROP INDEX i;
PRAGMA integrity_check;
COMMIT;
-- case: bug-hunt-10/vacuum/integrity-check-in-transaction-after-bulk-insert
CREATE TABLE t1(c0);
BEGIN;
INSERT INTO t1 SELECT value FROM generate_series(1,100000);
PRAGMA integrity_check;
COMMIT;
PRAGMA integrity_check;
-- case: bug-hunt-10/vacuum/integrity-check-in-transaction-after-reindex
CREATE TABLE t1(c0, c1);
CREATE INDEX i ON t1(c0);
INSERT INTO t1 SELECT value, value FROM generate_series(1,20000);
BEGIN;
REINDEX;
PRAGMA integrity_check;
COMMIT;
PRAGMA integrity_check;
-- case: bug-hunt-10/vacuum/insert-or-fail-datatype-mismatch-inside-a-transaction
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
BEGIN;
INSERT OR FAIL INTO t VALUES (1,1),('x',2),(3,3);
COMMIT;
SELECT * FROM t;
-- case: bug-hunt-10/vacuum/insert-or-ignore-datatype-mismatch-later-row-in-transaction
CREATE TABLE t1 (c0 INTEGER PRIMARY KEY, c1 VARCHAR(5));
BEGIN;
INSERT OR IGNORE INTO t1 (c0) VALUES (10), ('1.5'), (3);
SELECT * FROM t1;
COMMIT;
-- case: bug-hunt-10/vacuum/on-conflict-do-nothing-datatype-mismatch-later-row-in-transaction
BEGIN;
CREATE TABLE t9 (c0 INTEGER PRIMARY KEY, c1 ANY);
INSERT INTO t9 (c0, c1) VALUES (5, 'ABC'), ('1.5', 'b') ON CONFLICT (c0) DO NOTHING;
SELECT * FROM t9;
COMMIT;
-- case: bug-hunt-10/vacuum/insert-or-replace-datatype-mismatch-inside-a-transaction
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
BEGIN;
INSERT OR REPLACE INTO t VALUES (1,1),('x',2),(3,3);
COMMIT;
SELECT * FROM t;
-- case: bug-hunt-10/vacuum/insert-or-abort-datatype-mismatch-inside-a-transaction
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
BEGIN;
INSERT OR ABORT INTO t VALUES (1,1),('x',2),(3,3);
COMMIT;
SELECT * FROM t;
-- case: bug-hunt-10/vacuum/insert-or-fail-datatype-mismatch-autocommit
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT OR FAIL INTO t VALUES (1,1),('x',2),(3,3);
SELECT * FROM t;
-- case: bug-hunt-10/vacuum/unique-and-primary-key-share-one-index
CREATE TABLE t(c0 UNIQUE, PRIMARY KEY(c0));
SELECT name FROM sqlite_master ORDER BY 1;
PRAGMA index_list(t);
-- case: bug-hunt-10/vacuum/unique-constraints-share-one-index
CREATE TABLE t(c0, UNIQUE(c0), UNIQUE(c0 DESC));
SELECT name FROM sqlite_master ORDER BY 1;
-- case: bug-hunt-10/vacuum/two-column-unique-and-primary-key-share-one-index
CREATE TABLE t(c0, c1, UNIQUE(c0, c1), PRIMARY KEY(c0, c1));
SELECT name FROM sqlite_master ORDER BY 1;
PRAGMA index_list(t);
INSERT INTO t VALUES(1, 2);
INSERT INTO t VALUES(1, 2);
-- case: bug-hunt-10/vacuum/different-collation-unique-keeps-both-indexes
CREATE TABLE t(c0, UNIQUE(c0), UNIQUE(c0 COLLATE NOCASE));
SELECT name FROM sqlite_master ORDER BY 1;
-- case: bug-hunt-10/vacuum/shared-index-survives-reopen-and-vacuum
CREATE TABLE t(c0 UNIQUE, c1, PRIMARY KEY(c0));
INSERT INTO t VALUES(1, 'a'), (2, 'b');
VACUUM;
SELECT name FROM sqlite_master ORDER BY 1;
-- reopen
SELECT name FROM sqlite_master ORDER BY 1;
PRAGMA integrity_check;
INSERT INTO t VALUES(1, 'c');
-- case: bug-hunt-10/vacuum/foreign-key-parent-table-missing-deferred
CREATE TABLE t5 (c0 INTEGER PRIMARY KEY, FOREIGN KEY (c0) REFERENCES t1(c0) DEFERRABLE INITIALLY DEFERRED);
PRAGMA foreign_keys=ON;
INSERT INTO t5 (c0) VALUES (10);
SELECT count(*) FROM t5;
-- case: bug-hunt-10/vacuum/foreign-key-parent-table-missing-delete
CREATE TABLE t5 (c0 INTEGER PRIMARY KEY, c1 REFERENCES t1(c0));
PRAGMA foreign_keys=ON;
DELETE FROM t5;
SELECT 'done';
-- case: bug-hunt-10/vacuum/create-table-if-not-exists-existing-invalid-definition
CREATE TABLE t1(a);
CREATE TABLE IF NOT EXISTS t1(a DEFAULT 1) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS t1(a UNIQUE) STRICT;
SELECT 'done';
-- case: bug-hunt-10/vacuum/default-names-a-function-that-does-not-exist
CREATE TABLE t1(a DEFAULT (nosuchfn()));
SELECT 'created';
INSERT INTO t1 DEFAULT VALUES;
SELECT 'done';
-- case: bug-hunt-10/vacuum/replace-with-after-delete-trigger-naming-a-missing-table
CREATE TABLE t1(a INTEGER PRIMARY KEY, b);
CREATE TRIGGER tr AFTER DELETE ON t1 BEGIN DELETE FROM nosuch WHERE x = old.a; END;
INSERT OR REPLACE INTO t1 VALUES(1, 'x');
SELECT * FROM t1;
-- case: bug-hunt-10/vacuum/fts5vocab-over-missing-table
CREATE VIRTUAL TABLE v USING fts5vocab(nosuch, row);
SELECT 'done';
-- case: bug-hunt-10/vacuum/drop-table-whose-key-names-a-missing-parent
CREATE TABLE c(x REFERENCES nosuch(y));
PRAGMA foreign_keys=ON;
DROP TABLE c;
SELECT count(*) FROM sqlite_master;
-- case: bug-hunt-10/vacuum/key-to-a-missing-parent-with-foreign-keys-off
CREATE TABLE c(x INTEGER PRIMARY KEY, y REFERENCES nosuch(z) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO c VALUES(1, 2);
DELETE FROM c;
SELECT count(*) FROM c;
-- case: bug-hunt-10/vacuum/fts5vocab-over-missing-table-read
CREATE VIRTUAL TABLE v USING fts5vocab(nosuch, row);
SELECT * FROM v;
-- case: bug-hunt-10/vacuum/fts5-merge-with-rank-argument
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f(a) VALUES('x');
INSERT INTO f(f, rank) VALUES('merge', 4);
INSERT INTO f(f, rank) VALUES('merge', NULL);
SELECT count(*) FROM f;
