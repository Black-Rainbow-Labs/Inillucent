-- case: bug-hunt-10/writes/reindex-in-transaction-then-rollback-index-keeps-row
CREATE TABLE t(a,b);
CREATE INDEX i ON t(b);
BEGIN;
INSERT INTO t VALUES(1,4);
REINDEX t;
ROLLBACK;
PRAGMA integrity_check;
SELECT count(*) FROM t;
SELECT count(*) FROM t INDEXED BY i;

-- case: bug-hunt-10/writes/wor-unique-and-desc-pk-same-column-update-where
CREATE TABLE t0 (a, b UNIQUE, PRIMARY KEY (b DESC)) WITHOUT ROWID;
INSERT INTO t0 VALUES(1,2);
UPDATE t0 SET a = 5 WHERE b = 2;
SELECT * FROM t0;

-- case: bug-hunt-10/writes/wor-unique-and-desc-pk-same-column-delete-where
CREATE TABLE t0 (a, b UNIQUE, PRIMARY KEY (b DESC)) WITHOUT ROWID;
INSERT INTO t0 VALUES(1,2);
DELETE FROM t0 WHERE b = 2;
SELECT count(*) FROM t0;

-- case: bug-hunt-10/writes/insert-or-ignore-notnull-skipped-row-fk-error
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a NOT NULL, k REFERENCES p(k));
INSERT OR IGNORE INTO c VALUES(NULL, 5);
SELECT changes(), count(*) FROM c;

-- case: bug-hunt-10/writes/insert-or-ignore-unique-skipped-row-fk-error
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a UNIQUE, k REFERENCES p(k));
INSERT INTO p VALUES(5);
INSERT INTO c VALUES(1,5);
INSERT OR IGNORE INTO c VALUES(1, 6);
SELECT changes(), count(*) FROM c;

-- case: bug-hunt-10/writes/duplicate-key-insert-with-overflow-row-leaks-pages
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(10,1);
INSERT INTO t VALUES(10,'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/duplicate-key-insert-with-overflow-row-leaks-pages-wor
CREATE TABLE t(a PRIMARY KEY, b) WITHOUT ROWID;
INSERT INTO t VALUES(10,1);
INSERT INTO t VALUES(10,'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx');
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/update-without-where-visits-unique-index-order
CREATE TABLE t(a,b UNIQUE);
INSERT INTO t VALUES(1,9),(2,5),(3,7);
UPDATE t SET a=a+10 RETURNING a,b;

-- case: bug-hunt-10/writes/delete-without-where-trigger-order
CREATE TABLE t(a,b UNIQUE);
CREATE TABLE lg(x);
CREATE TRIGGER tr AFTER DELETE ON t BEGIN INSERT INTO lg VALUES(old.a); END;
INSERT INTO t VALUES(1,9),(2,5),(3,7);
DELETE FROM t;
SELECT rowid, x FROM lg ORDER BY rowid;

-- case: bug-hunt-10/writes/insert-or-fail-multirow-fk-violating-row-kept
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a NOT NULL, k REFERENCES p(k));
INSERT OR FAIL INTO c VALUES(1, 5),(NULL, NULL);
SELECT changes();
SELECT * FROM c;
PRAGMA foreign_key_check;

-- case: bug-hunt-10/writes/raise-fail-in-after-delete-trigger-changes-count
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
CREATE TRIGGER tr AFTER DELETE ON t BEGIN SELECT RAISE(FAIL,'x') WHERE old.a=2; END;
INSERT INTO t VALUES(1,0),(2,0),(3,0);
DELETE FROM t;
SELECT changes(), total_changes();
SELECT * FROM t;

-- case: bug-hunt-10/writes/error-order-fk-before-notnull
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a NOT NULL, k REFERENCES p(k));
INSERT INTO c VALUES(NULL, 5);

-- case: bug-hunt-10/writes/error-order-fk-before-unique
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a UNIQUE, k REFERENCES p(k));
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(1,1);
INSERT INTO c VALUES(1, 5);

-- case: bug-hunt-10/writes/error-order-fk-before-check
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a CHECK(a<>3), k REFERENCES p(k));
INSERT INTO c VALUES(3, 5);

-- case: bug-hunt-10/writes/reindex-in-transaction-integrity-check-unused-page
CREATE TABLE t2(a);
CREATE INDEX i ON t2(a);
BEGIN;
REINDEX;
PRAGMA integrity_check;
COMMIT;

-- case: bug-hunt-10/writes/insert-returning-then-deferred-fk-failure-autocommit
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(k REFERENCES p(k) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO c VALUES(18) RETURNING rowid;

-- case: bug-hunt-10/writes/insert-or-replace-notnull-default-leaves-stored-generated-null
CREATE TABLE t(a INTEGER PRIMARY KEY, b NOT NULL DEFAULT 5, g AS (b+1) STORED);
INSERT OR REPLACE INTO t(a,b) VALUES(1,NULL);
SELECT * FROM t;

-- case: bug-hunt-10/writes/upsert-excluded-has-column-affinity
CREATE TABLE t(a INTEGER PRIMARY KEY, c1 TEXT, c2);
INSERT INTO t VALUES(46,'x',1);
INSERT INTO t(a,c1,c2) VALUES(46,21,-5) ON CONFLICT(a) DO UPDATE SET c2=typeof(excluded.c1) RETURNING *;

-- case: bug-hunt-10/writes/upsert-excluded-compare-text-column
CREATE TABLE t(a INTEGER PRIMARY KEY, c1 TEXT, c2);
INSERT INTO t VALUES(46,'x',1);
INSERT INTO t(a,c1,c2) VALUES(46,21,-5) ON CONFLICT(a) DO UPDATE SET c1=17.0 WHERE excluded.c1>3 RETURNING *;

-- case: bug-hunt-10/writes/delete-without-where-secondary-index-order
CREATE TABLE t(a,b); CREATE INDEX i ON t(a);
INSERT INTO t VALUES(3,1),(1,1),(2,1),(5,1);
DELETE FROM t RETURNING a;

-- case: bug-hunt-10/writes/upsert-target-on-index-with-rowid-alias
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c);
CREATE UNIQUE INDEX i ON t(c, a);
INSERT INTO t(a,b) VALUES(1,1) ON CONFLICT (c,a) DO NOTHING;

-- case: bug-hunt-10/writes/wor-unique-column-and-desc-pk-keeps-ascending-scan
CREATE TABLE t0 (a, b UNIQUE, PRIMARY KEY (b DESC)) WITHOUT ROWID;
INSERT INTO t0 VALUES(1,2),(1,3),(1,1);
SELECT * FROM t0;
UPDATE t0 SET a = 5 WHERE b = 3;
DELETE FROM t0 WHERE b = 1;
SELECT * FROM t0;
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/update-or-replace-notnull-default-recomputes-stored-generated
CREATE TABLE t(a INTEGER PRIMARY KEY, b NOT NULL DEFAULT 5, g AS (b+1) STORED);
INSERT INTO t(a,b) VALUES(1,2);
UPDATE OR REPLACE t SET b=NULL;
SELECT * FROM t;

-- case: bug-hunt-10/writes/single-row-insert-foreign-key-leaves-last-insert-rowid
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, a NOT NULL, k REFERENCES p(k));
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(7,1,1);
INSERT INTO c VALUES(8,2,9);
SELECT last_insert_rowid(), changes(), total_changes();
INSERT INTO c VALUES(9,NULL,9);
SELECT last_insert_rowid(), count(*) FROM c;

-- case: bug-hunt-10/writes/insert-or-fail-multirow-foreign-key-inside-transaction
PRAGMA foreign_keys=ON;
CREATE TABLE p(k PRIMARY KEY);
CREATE TABLE c(a NOT NULL, k REFERENCES p(k));
BEGIN;
INSERT INTO c VALUES(0,NULL);
INSERT OR FAIL INTO c VALUES(1,5),(NULL,NULL);
COMMIT;
SELECT * FROM c;

-- case: bug-hunt-10/writes/upsert-clause-refusal-names-its-position
CREATE TABLE t(a INTEGER PRIMARY KEY, b UNIQUE, c);
INSERT INTO t VALUES(1,1,1) ON CONFLICT(c) DO NOTHING ON CONFLICT(b) DO NOTHING;
INSERT INTO t VALUES(1,1,1) ON CONFLICT(b) DO NOTHING ON CONFLICT(c) DO NOTHING;
INSERT INTO t VALUES(1,1,1) ON CONFLICT(c) DO NOTHING;

-- case: bug-hunt-10/writes/duplicate-key-insert-with-overflow-row-indexed-table
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c);
CREATE INDEX i ON t(c);
INSERT INTO t VALUES(10,1,1);
INSERT INTO t VALUES(10,printf('%.9000c','x'),2);
INSERT INTO t VALUES(10,printf('%.9000c','y'),3);
INSERT INTO t VALUES(11,printf('%.9000c','z'),4);
DELETE FROM t WHERE a=11;
PRAGMA integrity_check;
SELECT count(*) FROM t;

-- case: bug-hunt-10/writes/drop-table-in-transaction-integrity-check-unused-page
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1,printf('%.9000c','x'));
BEGIN;
DROP TABLE t;
PRAGMA integrity_check;
COMMIT;
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/delete-without-where-keeps-rowid-order-with-two-indexes
CREATE TABLE t(a, b, c);
CREATE INDEX ia ON t(a);
CREATE INDEX ibc ON t(b, c);
INSERT INTO t VALUES(9,2,1),(8,3,2),(7,1,3),(6,5,4);
DELETE FROM t RETURNING rowid, a;

-- case: bug-hunt-10/writes/update-with-where-on-unindexed-column-order
CREATE TABLE t(a UNIQUE, b);
INSERT INTO t VALUES(9,1),(5,1),(7,1);
UPDATE t SET b=b+1 WHERE b=1 RETURNING a;

-- case: bug-hunt-10/writes/wor-select-star-reads-the-table-not-a-full-width-index
CREATE TABLE t0 (c0 TEXT NOT NULL, c1 INTEGER UNIQUE DEFAULT x'00', c2 INT NOT NULL, PRIMARY KEY (c2, c0, c1)) WITHOUT ROWID, STRICT;
INSERT INTO t0 VALUES('3.0',1,27),('K19',20,5);
SELECT * FROM t0;
INSERT INTO t0 (c0, c1, c2) SELECT c0, c1, c0 FROM t0 ON CONFLICT (c2, c0, c1) DO UPDATE SET c0 = coalesce(excluded.c0, c0);
SELECT changes(), total_changes();

-- case: bug-hunt-10/writes/insert-or-fail-after-trigger-failure-counts-the-row
CREATE TABLE t1 (c0 REAL DEFAULT 1.5, c1 INTEGER PRIMARY KEY, c2 VARCHAR(20) NOT NULL, c3 INT DEFAULT 1.5, c4 ANY, c5 VARCHAR(20) NOT NULL);
CREATE TRIGGER tr1 AFTER INSERT ON t1 WHEN new.c0 IS NOT NULL BEGIN INSERT OR IGNORE INTO t1(c0, c1) VALUES(new.c0, new.c1); END;
INSERT INTO t1 (c0, c1, c2, c3, c4, c5) VALUES (30, 6, 'abc', -5, x'0102', 'A');
INSERT OR FAIL INTO t1 (c0, c1, c2, c3, c4, c5) VALUES (37, 7, '', 1, 2, '3');
SELECT changes(), total_changes();
SELECT c1 FROM t1;

-- case: bug-hunt-10/writes/upsert-excluded-numeric-compare-in-where-and-set
CREATE TABLE t(a INTEGER PRIMARY KEY, c1 TEXT, c2);
INSERT INTO t VALUES(46,'x',1);
INSERT INTO t(a,c1,c2) VALUES(46,21,-5) ON CONFLICT(a) DO UPDATE SET c1=excluded.c1, c2=typeof(excluded.c1) || excluded.c1 WHERE excluded.c1>3 RETURNING *, typeof(c1);

-- case: bug-hunt-10/writes/strict-type-checked-before-a-before-insert-trigger
CREATE TABLE t0(c0, c2 PRIMARY KEY);
CREATE TABLE t2 (c0 INTEGER NOT NULL DEFAULT 1, c2 INTEGER NOT NULL) STRICT;
CREATE TRIGGER tr0 BEFORE INSERT ON t2 BEGIN DELETE FROM t0 WHERE c0 = new.c0; END;
INSERT INTO t2(c0) VALUES (x'0102');

-- case: bug-hunt-10/writes/delete-where-no-index-answers-reads-the-table-in-rowid-order
CREATE TABLE t1 (c0 TEXT, c1 INT UNIQUE, c2 INTEGER NOT NULL);
CREATE INDEX ix3 ON t1 (c2, c0 DESC);
INSERT INTO t1 VALUES ('a', 5, 20), ('b', 1, 9), ('c', 3, 2), ('d', 2, 12), ('e', 9, 3);
DELETE FROM t1 WHERE c2 > 7 OR c0 < 'c' RETURNING *;
SELECT * FROM t1;

-- case: bug-hunt-10/writes/reindex-inside-a-transaction-rolls-back-every-index
CREATE TABLE t(a,b,c);
CREATE INDEX i ON t(b);
CREATE UNIQUE INDEX j ON t(c);
INSERT INTO t VALUES(1,4,1),(2,5,2),(3,6,3);
BEGIN;
INSERT INTO t VALUES(4,7,4);
DELETE FROM t WHERE a=1;
REINDEX t;
INSERT INTO t VALUES(5,8,5);
REINDEX i;
PRAGMA integrity_check;
ROLLBACK;
PRAGMA integrity_check;
SELECT count(*) FROM t INDEXED BY i;
SELECT count(*) FROM t INDEXED BY j;
SELECT a FROM t INDEXED BY i WHERE b>4;
BEGIN;
REINDEX;
SAVEPOINT s;
REINDEX t;
INSERT INTO t VALUES(10,10,10);
ROLLBACK TO s;
SELECT count(*) FROM t INDEXED BY i;
RELEASE s;
COMMIT;
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/insert-or-replace-default-for-null-checks-the-foreign-key
PRAGMA foreign_keys=ON;
CREATE TABLE t0 (c0 TEXT COLLATE NOCASE UNIQUE);
CREATE TABLE t1 (c0 TEXT NOT NULL DEFAULT 2, c1 TEXT NOT NULL, FOREIGN KEY (c0) REFERENCES t0(c0));
INSERT OR REPLACE INTO t1 (c0, c1) VALUES (NULL, '1');
SELECT changes(), count(*) FROM t1;
INSERT INTO t0 VALUES('2');
INSERT OR REPLACE INTO t1 (c0, c1) VALUES (NULL, '1');
SELECT changes(), count(*) FROM t1;

-- case: bug-hunt-10/writes/strict-type-checked-before-a-before-update-trigger
CREATE TABLE t0 (c0 INTEGER PRIMARY KEY);
CREATE TABLE t2 (c0 INTEGER PRIMARY KEY, c5 INT UNIQUE) STRICT;
CREATE TRIGGER tr0 BEFORE UPDATE ON t2 BEGIN INSERT OR REPLACE INTO t0(c0) VALUES(new.c0 + 1); END;
INSERT INTO t2 VALUES (53, 14);
UPDATE t2 SET c5 = 'é';
SELECT changes(), total_changes();
SELECT count(*) FROM t0;

-- case: bug-hunt-10/writes/trigger-new-and-old-compare-without-the-column-affinity
CREATE TABLE t0 (c0 TEXT, c2 TEXT);
CREATE TABLE lg(x, y);
CREATE TRIGGER tr1 AFTER UPDATE ON t0 BEGIN INSERT INTO lg VALUES(new.c0 = 7, old.c0 = 7); END;
CREATE TRIGGER tr2 AFTER INSERT ON t0 BEGIN INSERT INTO lg VALUES(new.c0 = 7, new.c0 = '7'); END;
INSERT INTO t0 VALUES(7, 1);
UPDATE t0 SET c2 = 6;
SELECT * FROM lg;

-- case: bug-hunt-10/writes/wor-long-key-with-an-after-insert-trigger-keeps-accepting-inserts
CREATE TABLE lg(x);
CREATE TABLE t1 (c0, c2 NOT NULL PRIMARY KEY) WITHOUT ROWID;
CREATE TRIGGER tr0 AFTER INSERT ON t1 BEGIN INSERT INTO lg VALUES(new.c0); END;
INSERT INTO t1 VALUES ('e', printf('%.5000c', 'q'));
SELECT 'one', count(*) FROM t1;
INSERT INTO t1 VALUES ('f', printf('%.5000c', 'q') || 'z');
SELECT 'two', count(*) FROM t1;
INSERT INTO t1 VALUES ('g', 'short');
SELECT 'three', count(*), (SELECT count(*) FROM lg) FROM t1;
PRAGMA integrity_check;

-- case: bug-hunt-10/writes/insert-or-fail-not-null-with-a-deferred-key-fails-on-the-key
PRAGMA foreign_keys=ON;
CREATE TABLE t0 (c0 INTEGER PRIMARY KEY);
CREATE TABLE t1 (c0, c1 NOT NULL, c3 REAL NOT NULL, c4 REAL, FOREIGN KEY (c4) REFERENCES t0(c0) DEFERRABLE INITIALLY DEFERRED);
INSERT OR FAIL INTO t1 (c0, c1, c3, c4) VALUES ('2', 'A', '1.0', 'abc'), (4, 4.0, NULL, 5);
SELECT changes(), total_changes();
SELECT count(*) FROM t1;
