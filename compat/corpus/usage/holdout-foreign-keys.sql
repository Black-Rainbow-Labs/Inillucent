-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/foreign-keys/30-update-of-a-parent-key-that-has-a-child-row-must-f
pragma foreign_keys=true;
create table t(a unique);
create table s(a, foreign key(a) references t(a));
insert into t values (1);
insert into s values (1);
update t set a = a + 1;
select * from s;
select * from t;
-- case: holdout/foreign-keys/32-deferred-fk-violation-detected-at-commit-leaves-no
pragma foreign_keys=on;
create table parent(a primary key);
create table child(a, b, foreign key(b) references parent(a) deferrable initially deferred);
insert into child values(1,1);
select count(*) from child;
begin;
insert into child values(2,2);
commit;
select count(*) from child;
-- case: holdout/foreign-keys/51-a-failed-insert-inside-an-explicit-transaction-onl
PRAGMA foreign_keys = ON;
CREATE TABLE parent (id INT PRIMARY KEY);
CREATE TABLE child (id INT, pid INT REFERENCES parent(id));
BEGIN;
INSERT INTO parent VALUES (2);
INSERT INTO child VALUES (999, 999);
INSERT INTO parent VALUES (3);
COMMIT;
SELECT * FROM parent ORDER BY id;
SELECT * FROM child;
-- case: holdout/foreign-keys/52-on-delete-cascade-through-a-self-referencing-chain
PRAGMA foreign_keys = ON;
CREATE TABLE T (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES T(id) ON DELETE CASCADE);
INSERT INTO T VALUES (1, NULL), (2, 1), (3, 2), (4, 3);
DELETE FROM T WHERE id = 1;
SELECT count(*) FROM T;
-- case: holdout/foreign-keys/99-fk-on-delete-cascade-and-on-update-cascade-honour-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES('A');
INSERT INTO c VALUES('a');
DELETE FROM p WHERE id='A';
SELECT count(*) FROM c;
CREATE TABLE p2(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c2(pid TEXT REFERENCES p2(id) ON UPDATE CASCADE);
INSERT INTO p2 VALUES('A');
INSERT INTO c2 VALUES('a');
UPDATE p2 SET id='B' WHERE id='A';
SELECT quote(pid) FROM c2;
-- case: holdout/foreign-keys/100-deferred-fk-still-violated-at-commit-after-a-no-op
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid TEXT REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c VALUES(1, 'missing');
INSERT INTO c VALUES(2, NULL);
UPDATE c SET pid = pid WHERE id=2;
COMMIT;
SELECT id, quote(pid) FROM c ORDER BY id;
-- case: holdout/foreign-keys/101-no-op-update-of-a-deferred-fk-column-inside-a-tran
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c VALUES(1, NULL);
UPDATE c SET pid = pid WHERE id=1;
SELECT 'ok';
ROLLBACK;
-- case: holdout/foreign-keys/111-returning-emits-no-row-when-the-statement-fails-a-
PRAGMA foreign_keys = ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id), val TEXT);
INSERT INTO parent VALUES(1, 'p1');
INSERT INTO child VALUES(1, 1, 'c1');
INSERT INTO child VALUES(2, 999, 'c2') RETURNING *;
DELETE FROM parent WHERE id = 1 RETURNING *;
UPDATE child SET parent_id = 5 WHERE id = 1 RETURNING *;
SELECT * FROM parent; SELECT * FROM child;
-- case: holdout/foreign-keys/116-foreign-key-declared-by-alter-table-add-column-is-
PRAGMA foreign_keys=ON;
CREATE TABLE parent(p TEXT UNIQUE);
CREATE TABLE child(a INTEGER);
ALTER TABLE child ADD COLUMN p TEXT REFERENCES parent(p) ON DELETE RESTRICT ON UPDATE RESTRICT;
INSERT INTO child(a,p) VALUES (1,'missing');
SELECT * FROM child;
-- case: holdout/foreign-keys/120-update-or-replace-deletes-another-row-and-cascades
PRAGMA foreign_keys = ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id INT REFERENCES parent(id) ON DELETE CASCADE);
INSERT INTO parent VALUES (10, 'x'), (20, 'y');
INSERT INTO child VALUES (10, 10), (20, 20);
UPDATE OR REPLACE parent SET name = 'y' WHERE id = 10;
SELECT * FROM child ORDER BY id;
SELECT * FROM parent;
-- case: holdout/foreign-keys/121-trigger-fired-by-a-cascade-sees-the-parent-row-alr
PRAGMA foreign_keys = ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id INT REFERENCES parent(id) ON DELETE CASCADE, val TEXT);
CREATE TABLE audit(msg TEXT);
CREATE TRIGGER child_delete BEFORE DELETE ON child BEGIN INSERT INTO audit VALUES ('parent_count=' || (SELECT COUNT(*) FROM parent)); END;
INSERT INTO parent VALUES (1, 'p1'), (2, 'p2');
INSERT INTO child VALUES (1, 1, 'c1'), (2, 1, 'c2');
DELETE FROM parent WHERE id = 1;
SELECT * FROM audit;
-- case: holdout/foreign-keys/194-deferred-fk-is-satisfied-by-a-parent-row-inserted-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid INTEGER PRIMARY KEY REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c(pid) VALUES(1);
INSERT INTO p(id) VALUES(1);
COMMIT;
SELECT * FROM c;
-- case: holdout/foreign-keys/249-delete-whose-cascade-trigger-updates-the-target-ta
PRAGMA foreign_keys=ON;
CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT, active INTEGER);
CREATE TABLE sessions(id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id) ON DELETE CASCADE);
CREATE TRIGGER t AFTER DELETE ON sessions BEGIN UPDATE users SET active = 0; END;
INSERT INTO users VALUES (1,'alice',0),(2,'bob',1),(3,'carol',1),(4,'dave',1),(5,'eve',1);
INSERT INTO sessions VALUES (100,1);
DELETE FROM users WHERE active = 0;
SELECT COUNT(*) FROM users;
SELECT * FROM users ORDER BY id;
-- case: holdout/foreign-keys/250-update-whose-cascade-trigger-changes-the-target-ta
PRAGMA foreign_keys=ON;
CREATE TABLE parent(key INTEGER UNIQUE, x INTEGER);
CREATE TABLE child(id INTEGER PRIMARY KEY, pkey INTEGER REFERENCES parent(key) ON UPDATE CASCADE);
CREATE TRIGGER tr AFTER UPDATE ON child BEGIN UPDATE parent SET x = 0 WHERE rowid > 0; END;
INSERT INTO parent VALUES (1,100),(2,200),(3,300);
INSERT INTO child VALUES (10,1),(20,2),(30,3);
UPDATE parent SET key = key + 10 WHERE x > 150;
SELECT key, x FROM parent ORDER BY rowid;
SELECT id, pkey FROM child ORDER BY id;
-- case: holdout/foreign-keys/261-self-referencing-fk-with-on-delete-set-null-plus-c
PRAGMA foreign_keys=ON;
CREATE TABLE files (id TEXT PRIMARY KEY, parent_id TEXT, filename TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')), FOREIGN KEY (parent_id) REFERENCES files(id) ON DELETE SET NULL);
CREATE INDEX idx_files_parent_id ON files (parent_id);
CREATE TABLE file_blobs (file_id TEXT NOT NULL, data BLOB NOT NULL, PRIMARY KEY (file_id), FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE);
INSERT INTO files(id, parent_id, filename) VALUES ('a', NULL, 'dir'), ('b', 'a', 'f1');
INSERT INTO file_blobs VALUES ('b', x'01');
DELETE FROM files WHERE id = 'a';
SELECT id, parent_id FROM files;
SELECT count(*) FROM file_blobs;
DELETE FROM files WHERE id = 'b';
SELECT count(*) FROM file_blobs;
-- case: holdout/foreign-keys/298-deferred-fk-counter-when-an-update-is-skipped-by-o
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id PRIMARY KEY ON CONFLICT IGNORE);
CREATE TABLE child(pid INT REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES (40), (195);
INSERT INTO child VALUES (195);
BEGIN;
UPDATE parent SET id=40 WHERE id=195;
COMMIT;
SELECT * FROM parent ORDER BY id;
SELECT * FROM child;
-- case: holdout/foreign-keys/301-orphan-child-row-is-rejected-at-commit-even-if-a-p
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id UNIQUE ON CONFLICT IGNORE);
CREATE TABLE child(id PRIMARY KEY, pid, FOREIGN KEY(pid) REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES (20);
INSERT INTO parent VALUES (24);
INSERT INTO child VALUES (1, 20);
BEGIN;
INSERT INTO child VALUES (2, 276);
UPDATE parent SET id=20 WHERE id=24;
COMMIT;
SELECT count(*) FROM child WHERE pid=276;
-- case: holdout/foreign-keys/302-self-referencing-fk-between-an-integer-and-a-text-
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER UNIQUE, pk TEXT REFERENCES t(k));
INSERT INTO t(id, k, pk) VALUES (1, 1, '1');
SELECT typeof(k), typeof(pk), k, pk FROM t;
CREATE TABLE p(k INTEGER UNIQUE);
CREATE TABLE c(pk TEXT REFERENCES p(k));
INSERT INTO p VALUES (1);
INSERT INTO c VALUES ('1');
SELECT * FROM c;
-- case: holdout/foreign-keys/377-pragma-foreign-keys-off-is-a-no-op-inside-a-transa
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id));
INSERT INTO parent VALUES(1);
INSERT INTO child VALUES(10,1);
BEGIN;
PRAGMA foreign_keys=OFF;
PRAGMA foreign_keys;
DELETE FROM parent WHERE id=1;
ROLLBACK;
SAVEPOINT s;
PRAGMA foreign_keys=OFF;
PRAGMA foreign_keys;
RELEASE s;
PRAGMA foreign_keys=OFF;
PRAGMA foreign_keys;
-- case: holdout/foreign-keys/411-partial-index-on-a-child-fk-column-does-not-hide-c
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid INTEGER REFERENCES p(id), k TEXT);
CREATE INDEX ci ON c(pid) WHERE k IS NULL;
INSERT INTO p VALUES(1),(2);
INSERT INTO c VALUES(1,'x');
INSERT INTO c VALUES(2,NULL);
DELETE FROM p WHERE id=2;
DELETE FROM p WHERE id=1;
SELECT count(*) FROM p;
-- case: holdout/foreign-keys/412-dropping-the-index-on-a-child-fk-column-does-not-c
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(x TEXT REFERENCES p(id));
CREATE TABLE d(x INTEGER REFERENCES p(id));
INSERT INTO p VALUES(1);
INSERT INTO c VALUES('1');
CREATE INDEX ic ON c(x);
DELETE FROM p;
DROP INDEX ic;
DELETE FROM p;
SELECT count(*) FROM p;
SELECT count(*) FROM c;
-- case: holdout/foreign-keys/416-on-delete-cascade-with-a-child-column-that-is-noca
PRAGMA foreign_keys=ON;
CREATE TABLE p(k TEXT PRIMARY KEY);
CREATE TABLE c(k TEXT COLLATE NOCASE REFERENCES p(k) ON DELETE CASCADE ON UPDATE CASCADE);
INSERT INTO p VALUES('A'),('a');
INSERT INTO c VALUES('A'),('a');
DELETE FROM p WHERE k='A';
SELECT count(*) FROM p;
SELECT ifnull(group_concat(k),'(none)') FROM c;
DELETE FROM c;
INSERT INTO p VALUES('A');
INSERT INTO c VALUES('A'),('a');
UPDATE p SET k='z' WHERE k='A';
SELECT group_concat(k) FROM (SELECT k FROM c ORDER BY rowid);
-- case: holdout/foreign-keys/417-self-referencing-on-delete-cascade-and-set-null-ov
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER REFERENCES t(id) ON DELETE CASCADE);
CREATE TABLE u(id INTEGER PRIMARY KEY, k INTEGER REFERENCES u(id) ON DELETE CASCADE);
CREATE TABLE v(id INTEGER PRIMARY KEY, k INTEGER REFERENCES v(id) ON DELETE SET NULL);
CREATE INDEX ui ON u(k);
INSERT INTO t VALUES(1,NULL),(2,1),(3,1),(4,2);
INSERT INTO u VALUES(1,NULL),(2,1),(3,1),(4,2);
INSERT INTO v VALUES(1,NULL),(2,1),(3,1),(4,2);
DELETE FROM t WHERE id=1;
SELECT count(*) FROM t;
DELETE FROM u WHERE id=1;
SELECT count(*) FROM u;
DELETE FROM v WHERE id=1;
SELECT id, quote(k) FROM v ORDER BY id;
-- case: holdout/foreign-keys/446-insert-or-fail-undoes-earlier-rows-of-the-statemen
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY);
CREATE TABLE u(id INTEGER PRIMARY KEY, p REFERENCES t(id));
CREATE TABLE v(x INTEGER PRIMARY KEY);
INSERT INTO t VALUES(1);
INSERT OR FAIL INTO u VALUES(1,1),(2,7);
SELECT count(*) FROM u;
INSERT OR FAIL INTO v VALUES(1),('a');
SELECT count(*) FROM v;
INSERT OR FAIL INTO v VALUES(5),(5);
SELECT count(*) FROM v;
-- case: holdout/foreign-keys/451-add-column-with-references-and-a-non-null-default
PRAGMA foreign_keys = ON;
CREATE TABLE t(id INTEGER PRIMARY KEY);
CREATE TABLE u(id INTEGER PRIMARY KEY);
INSERT INTO t VALUES(1);
INSERT INTO u VALUES(1),(2);
ALTER TABLE u ADD COLUMN k INTEGER DEFAULT 5 REFERENCES t(id);
ALTER TABLE u ADD COLUMN k INTEGER DEFAULT NULL REFERENCES t(id);
ALTER TABLE u ADD COLUMN m INTEGER REFERENCES t(id);
SELECT * FROM u;
PRAGMA foreign_keys = OFF;
ALTER TABLE u ADD COLUMN n INTEGER DEFAULT 5 REFERENCES t(id);
-- case: holdout/foreign-keys/478-rename-column-then-drop-column-on-a-column-with-a-
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE c(parent_id INTEGER REFERENCES parent(id), keep INTEGER);
ALTER TABLE c RENAME COLUMN parent_id TO renamed_parent_id;
SELECT sql FROM sqlite_master WHERE name='c';
ALTER TABLE c DROP COLUMN renamed_parent_id;
SELECT sql FROM sqlite_master WHERE name='c';
-- case: holdout/foreign-keys/479-multi-row-insert-with-a-self-referencing-foreign-k
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent_id REFERENCES n(id));
INSERT INTO n VALUES (1, 2), (2, NULL);
SELECT * FROM n ORDER BY id;
INSERT INTO n VALUES (3, 99);
INSERT INTO n VALUES (4, 4);
SELECT count(*) FROM n;
-- case: holdout/foreign-keys/502-drop-column-on-a-column-with-a-column-level-refere
CREATE TABLE parent(id INTEGER PRIMARY KEY);
CREATE TABLE c(parent_id INTEGER REFERENCES parent(id), keep INTEGER);
ALTER TABLE c DROP COLUMN parent_id;
SELECT sql FROM sqlite_master WHERE name='c';
CREATE TABLE d(a, b, FOREIGN KEY(a) REFERENCES parent(id));
ALTER TABLE d DROP COLUMN a;
-- case: holdout/foreign-keys/515-add-column-keeps-quotes-around-a-keyword-column-na
CREATE TABLE t0("id" INTEGER PRIMARY KEY);
CREATE TABLE t1("id" INTEGER PRIMARY KEY, "order" REAL, FOREIGN KEY("order") REFERENCES "t0"("id"));
ALTER TABLE t1 ADD COLUMN added INTEGER;
SELECT sql FROM sqlite_master WHERE name = 't1';
-- case: holdout/foreign-keys/527-self-referencing-on-delete-cascade-over-rows-that-
CREATE TABLE t(id INTEGER PRIMARY KEY, p REFERENCES t ON DELETE CASCADE, pad);
INSERT INTO t WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM s WHERE x<35) SELECT x, NULL, zeroblob(450) FROM s;
UPDATE t SET p = 35 WHERE id BETWEEN 30 AND 34;
PRAGMA foreign_keys=ON;
DELETE FROM t WHERE id + 0 = 35;
SELECT count(*) FROM t;
-- case: holdout/foreign-keys/528-self-referencing-on-delete-set-null-during-a-delet
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, p REFERENCES t ON DELETE SET NULL);
CREATE INDEX tp ON t(p);
INSERT INTO t VALUES (1, NULL), (2, 1), (3, 1);
DELETE FROM t WHERE p IS NULL;
SELECT * FROM t;
-- case: holdout/foreign-keys/530-update-or-replace-whose-replace-cascades-to-delete
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE, p REFERENCES t(id) ON DELETE CASCADE);
INSERT INTO t VALUES (1, 'x', NULL), (2, 'y', 1), (3, 'z', NULL);
UPDATE OR REPLACE t SET u = 'x' WHERE id = 2;
SELECT * FROM t;
SELECT id FROM t WHERE u = 'z';
PRAGMA integrity_check;
-- case: holdout/foreign-keys/536-parent-delete-where-a-child-has-a-cascade-key-and-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(pid REFERENCES p(id) ON DELETE CASCADE, pname REFERENCES p(name));
INSERT INTO p VALUES(1,'a');
INSERT INTO c VALUES(1,'a');
DELETE FROM p WHERE id=1;
SELECT count(*) FROM p;
SELECT count(*) FROM c;
-- case: holdout/foreign-keys/537-self-referencing-on-delete-set-null-over-a-chain-d
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES t(id) ON DELETE SET NULL);
INSERT INTO t VALUES (1, NULL), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5);
DELETE FROM t WHERE id > 1;
SELECT id FROM t;
-- case: holdout/foreign-keys/538-self-referencing-on-delete-set-null-with-wide-rows
PRAGMA foreign_keys=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES t(id) ON DELETE SET NULL, pad TEXT);
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM s WHERE x < 12) INSERT INTO t SELECT x, CASE WHEN x > 1 THEN x - 1 END, printf('%.900c', 'x') FROM s;
DELETE FROM t WHERE id > 4;
SELECT group_concat(id) FROM t;
-- case: holdout/foreign-keys/539-backward-scan-of-an-index-while-on-delete-cascade-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v INTEGER);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) ON DELETE CASCADE, b TEXT);
CREATE INDEX cb ON c(b);
WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM s WHERE x < 20) INSERT INTO c(id, pid, b) SELECT x, NULL, printf('%02d', x) || printf('%.800c', '.') FROM s;
INSERT INTO p(id, v) VALUES (-1, NULL);
INSERT INTO c(id, pid, b) VALUES (99, -1, 'zz');
INSERT OR REPLACE INTO p(id, v) SELECT CASE WHEN id = 10 THEN -1 ELSE id END, 1 FROM c ORDER BY b DESC;
SELECT count(*) FROM p;
SELECT count(*) FROM c;
-- case: holdout/foreign-keys/663-update-of-a-parent-key-under-on-conflict-replace-w
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY ON CONFLICT REPLACE);
CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES (234), (2);
UPDATE parent SET id=2 WHERE id=234;
SELECT * FROM parent;
INSERT INTO parent VALUES (7);
INSERT INTO child VALUES (1, 7);
UPDATE parent SET id=2 WHERE id=7;
-- case: holdout/foreign-keys/723-rename-to-rewrites-references-in-other-tables-even
CREATE TABLE p (k BLOB NOT NULL PRIMARY KEY, v TEXT);
CREATE TABLE c (k BLOB PRIMARY KEY, b BLOB NOT NULL, FOREIGN KEY (k) REFERENCES p (k) ON DELETE CASCADE);
CREATE TABLE p_new (k BLOB NOT NULL PRIMARY KEY, v TEXT, extra INTEGER);
ALTER TABLE p RENAME TO p_v0;
ALTER TABLE p_new RENAME TO p;
SELECT sql FROM sqlite_master WHERE name = 'c';
PRAGMA legacy_alter_table = ON;
CREATE TABLE q1(a PRIMARY KEY);
CREATE TABLE q2(b REFERENCES q1(a));
ALTER TABLE q1 RENAME TO q1x;
SELECT sql FROM sqlite_master WHERE name = 'q2';
-- case: holdout/foreign-keys/743-update-of-a-non-key-column-does-not-cascade-to-chi
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) ON UPDATE CASCADE ON DELETE CASCADE);
INSERT INTO p VALUES (1, 'a');
INSERT INTO c VALUES (10, 1);
UPDATE p SET name = 'b' WHERE id = 1;
SELECT count(*) FROM c;
UPDATE p SET id = 2 WHERE id = 1;
SELECT * FROM c;
-- case: holdout/foreign-keys/748-insert-or-replace-of-a-parent-row-deletes-child-ro
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, v);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES (1, 'a');
INSERT INTO c VALUES (10, 1);
INSERT OR REPLACE INTO p VALUES (1, 'b');
SELECT * FROM p;
SELECT count(*) FROM c;
REPLACE INTO p VALUES (1, 'c');
SELECT count(*) FROM c;
-- case: holdout/foreign-keys/754-a-deferred-on-delete-no-action-key-is-checked-at-c
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED);
INSERT INTO p VALUES (1);
INSERT INTO c VALUES (10, 1);
BEGIN;
DELETE FROM p WHERE id = 1;
SELECT count(*) FROM p;
INSERT INTO p VALUES (1);
COMMIT;
SELECT count(*) FROM p;
BEGIN;
DELETE FROM p WHERE id = 1;
COMMIT;
SELECT count(*) FROM p;
-- case: holdout/foreign-keys/764-pragma-defer-foreign-keys-defers-immediate-checks-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id));
BEGIN;
PRAGMA defer_foreign_keys = ON;
INSERT INTO c VALUES (1, 5);
INSERT INTO p VALUES (5);
COMMIT;
SELECT * FROM c;
PRAGMA defer_foreign_keys;
BEGIN;
PRAGMA defer_foreign_keys = ON;
INSERT INTO c VALUES (2, 9);
COMMIT;
SELECT count(*) FROM c;
-- case: holdout/foreign-keys/770-on-delete-set-default-with-a-default-that-has-no-p
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER DEFAULT 99 REFERENCES p(id) ON DELETE SET DEFAULT);
INSERT INTO p VALUES (1);
INSERT INTO c VALUES (10, 1);
DELETE FROM p WHERE id = 1;
SELECT * FROM c;
-- case: holdout/foreign-keys/771-on-update-set-default-with-a-default-that-has-no-p
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INTEGER DEFAULT 99 REFERENCES p(id) ON UPDATE SET DEFAULT);
INSERT INTO p VALUES (1);
INSERT INTO c VALUES (10, 1);
UPDATE p SET id = 2 WHERE id = 1;
SELECT * FROM c;
INSERT INTO p VALUES (99);
UPDATE p SET id = 3 WHERE id = 2;
-- case: holdout/foreign-keys/817-foreign-key-check-of-a-table-that-does-not-exist
PRAGMA foreign_keys=ON;
PRAGMA foreign_key_check(missing);
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id));
PRAGMA foreign_keys=OFF;
INSERT INTO c VALUES (1, 77);
PRAGMA foreign_key_check;
PRAGMA foreign_key_check(c);
SELECT * FROM pragma_foreign_key_check('c');
SELECT * FROM pragma_foreign_key_check WHERE "table" = 'c';
