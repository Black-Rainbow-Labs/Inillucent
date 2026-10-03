-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/triggers/67-trigger-when-clause-with-not-exists-in-subquery-an
CREATE TABLE t1(id INTEGER PRIMARY KEY, val TEXT);
CREATE TRIGGER tr1 AFTER INSERT ON t1 WHEN NOT EXISTS (SELECT 1 WHERE NEW.id > 100) AND NEW.id < 100 BEGIN INSERT INTO t1 VALUES(NEW.id + 1000, 'auto'); END;
CREATE TRIGGER tr2 AFTER INSERT ON t1 WHEN NEW.id IN (SELECT 3) BEGIN INSERT INTO t1 VALUES(NEW.id + 100, 'in'); END;
INSERT INTO t1 VALUES(2, 'b');
INSERT INTO t1 VALUES(3, 'c');
SELECT * FROM t1 ORDER BY id;
-- case: holdout/triggers/68-subquery-in-the-set-list-of-a-trigger-body-update
CREATE TABLE t1(id INTEGER PRIMARY KEY);
CREATE TABLE counter(n INTEGER);
INSERT INTO counter VALUES(0);
INSERT INTO t1 VALUES(1);
CREATE TRIGGER tr1 AFTER INSERT ON t1 BEGIN UPDATE counter SET n = (SELECT max(id) FROM t1); END;
INSERT INTO t1 VALUES(2);
SELECT * FROM t1 ORDER BY id;
SELECT * FROM counter;
-- case: holdout/triggers/69-new-rowid-alias-in-an-update-trigger-when-clause
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT, counter INTEGER DEFAULT 0);
INSERT INTO t VALUES(1, 'a', 0), (2, 'b', 0), (3, 'c', 0);
CREATE TRIGGER tr1 AFTER UPDATE ON t WHEN NEW.id = 2 BEGIN UPDATE t SET counter = counter + 1 WHERE id = NEW.id AND val = 'updated'; END;
UPDATE t SET val = 'updated' WHERE id = 2;
SELECT * FROM t ORDER BY id;
-- case: holdout/triggers/87-replace-into-fires-delete-triggers-only-with-recur
CREATE TABLE t1(id INTEGER PRIMARY KEY, val INTEGER);
CREATE TABLE log(id INTEGER PRIMARY KEY, msg TEXT);
CREATE TRIGGER tr_delete AFTER DELETE ON t1 BEGIN INSERT INTO log VALUES (NULL, 'deleted id=' || OLD.id); END;
CREATE TRIGGER tr_insert AFTER INSERT ON t1 BEGIN INSERT INTO log VALUES (NULL, 'inserted id=' || NEW.id); END;
INSERT INTO t1 VALUES (1, 50);
DELETE FROM log;
REPLACE INTO t1 VALUES (1, 99);
SELECT * FROM log;
PRAGMA recursive_triggers = ON;
DELETE FROM log;
REPLACE INTO t1 VALUES (1, 100);
SELECT * FROM log;
-- case: holdout/triggers/112-update-returning-reports-the-row-before-after-trig
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TRIGGER t1_au AFTER UPDATE ON t1 BEGIN UPDATE t1 SET c = 'trigger_modified' WHERE a = NEW.a AND c <> 'trigger_modified'; END;
INSERT INTO t1 VALUES(1, 10, 'original');
UPDATE t1 SET b = 20 WHERE a = 1 RETURNING *;
SELECT * FROM t1;
-- case: holdout/triggers/128-rename-column-rewrites-new-col-inside-a-trigger-bo
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES('name=' || NEW.name); END;
INSERT INTO t VALUES(1,'test');
ALTER TABLE t RENAME COLUMN name TO full_name;
INSERT INTO t VALUES(2,'hello');
SELECT * FROM log;
SELECT sql FROM sqlite_master WHERE type='trigger';
-- case: holdout/triggers/150-new-inside-a-subquery-in-a-trigger-body
CREATE TABLE t(id INTEGER PRIMARY KEY, val INT);
CREATE TABLE lookup(id INTEGER PRIMARY KEY, multiplier INT);
CREATE TABLE result(msg TEXT);
INSERT INTO lookup VALUES (1, 10);
CREATE TRIGGER t_ins AFTER INSERT ON t BEGIN INSERT INTO result VALUES ('mult=' || (SELECT multiplier FROM lookup WHERE id = NEW.id)); END;
INSERT INTO t VALUES (1, 42);
SELECT * FROM t;
SELECT * FROM result;
-- case: holdout/triggers/159-before-update-trigger-sees-the-new-rowid-when-the-
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t VALUES(1, 'hello');
CREATE TRIGGER tr BEFORE UPDATE ON t WHEN NEW.id = 5 BEGIN SELECT RAISE(ABORT, 'blocked'); END;
UPDATE t SET id = 5 WHERE id = 1;
CREATE TABLE log(val INTEGER);
CREATE TRIGGER tr2 BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES(NEW.id); END;
UPDATE t SET id = 6 WHERE id = 1;
SELECT * FROM log;
-- case: holdout/triggers/160-between-in-a-trigger-when-clause
CREATE TABLE t(a INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t WHEN NEW.a BETWEEN 5 AND 10 BEGIN INSERT INTO log VALUES('in range: ' || NEW.a); END;
INSERT INTO t VALUES(7);
INSERT INTO t VALUES(70);
SELECT * FROM log;
-- case: holdout/triggers/161-scalar-subquery-in-a-trigger-when-clause
CREATE TABLE t(a INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t WHEN (SELECT count(*) FROM t) > 2 BEGIN INSERT INTO log VALUES('more than 2 rows'); END;
INSERT INTO t VALUES(1);
INSERT INTO t VALUES(2);
INSERT INTO t VALUES(3);
SELECT * FROM log;
-- case: holdout/triggers/162-trigger-when-comparison-on-new-column-uses-the-col
CREATE TABLE t(name TEXT COLLATE NOCASE);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t WHEN NEW.name = 'hello' BEGIN INSERT INTO log VALUES('matched:' || NEW.name); END;
INSERT INTO t VALUES('HELLO');
INSERT INTO t VALUES('hello');
SELECT * FROM log;
-- case: holdout/triggers/163-trigger-when-comparison-of-new-integer-column-with
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER t_ai AFTER INSERT ON t WHEN NEW.a = '3' BEGIN INSERT INTO log VALUES('match'); END;
INSERT INTO t VALUES(3, 'z');
SELECT * FROM log;
-- case: holdout/triggers/164-instead-of-trigger-on-a-view
CREATE TABLE t(a, b);
CREATE VIEW v AS SELECT a, b FROM t;
CREATE TRIGGER tr INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(NEW.a, NEW.b); END;
INSERT INTO v VALUES (1, 2);
SELECT * FROM t;
CREATE TRIGGER tr2 AFTER INSERT ON v BEGIN SELECT 1; END;
-- case: holdout/triggers/165-duplicate-trigger-name
CREATE TABLE t1(a);
CREATE TABLE t2(a);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO log VALUES('t1'); END;
CREATE TRIGGER tr AFTER INSERT ON t2 BEGIN INSERT INTO log VALUES('t2'); END;
CREATE TRIGGER IF NOT EXISTS tr AFTER INSERT ON t2 BEGIN INSERT INTO log VALUES('t2'); END;
INSERT INTO t1 VALUES(1);
SELECT * FROM log;
-- case: holdout/triggers/166-trigger-names-live-in-their-own-namespace
CREATE TABLE t(x);
CREATE TABLE log(msg);
CREATE TRIGGER t AFTER INSERT ON t BEGIN INSERT INTO log VALUES('fired'); END;
INSERT INTO t VALUES (1);
SELECT * FROM log;
CREATE INDEX t ON t(x);
-- case: holdout/triggers/167-trigger-and-table-names-with-spaces-keep-their-quo
CREATE TABLE "my table"("my col" INTEGER);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER "my trigger" AFTER INSERT ON "my table" BEGIN INSERT INTO log VALUES('inserted: ' || NEW."my col"); END;
INSERT INTO "my table" VALUES(42);
SELECT * FROM log;
SELECT name, sql FROM sqlite_schema WHERE type = 'trigger';
-- case: holdout/triggers/168-total-changes-counts-rows-changed-by-triggers-chan
CREATE TABLE t(a);
CREATE TABLE log(msg);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES('x'); END;
INSERT INTO t VALUES(1);
SELECT total_changes(), changes();
-- case: holdout/triggers/169-last-insert-rowid-after-an-insert-whose-trigger-in
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE t2(x INTEGER PRIMARY KEY, y TEXT);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.a + 1000, 'from trigger'); END;
INSERT INTO t1 VALUES(1, 'hello');
SELECT last_insert_rowid();
-- case: holdout/triggers/170-returning-inside-a-trigger-body
CREATE TABLE t(a);
CREATE TABLE other(b);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO other VALUES(NEW.a * 10) RETURNING *; END;
-- case: holdout/triggers/171-before-insert-trigger-sees-new-values-after-column
CREATE TABLE t(a REAL);
CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.a)); END;
INSERT INTO t VALUES(42);
SELECT * FROM log;
CREATE TABLE u(a INTEGER);
CREATE TRIGGER tr2 BEFORE INSERT ON u BEGIN INSERT INTO log VALUES(typeof(NEW.a)); END;
INSERT INTO u VALUES('42');
SELECT * FROM log;
-- case: holdout/triggers/172-before-update-trigger-fires-even-when-or-ignore-th
CREATE TABLE t(a, b NOT NULL);
CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES('trigger ran'); END;
INSERT INTO t VALUES(1, 'hello');
UPDATE OR IGNORE t SET b = NULL WHERE a = 1;
SELECT * FROM log;
SELECT * FROM t;
-- case: holdout/triggers/173-statement-level-or-replace-overrides-conflict-clau
CREATE TABLE t1(a INTEGER PRIMARY KEY, v);
CREATE TABLE t2(b INTEGER PRIMARY KEY, w);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.a, NEW.v); END;
INSERT INTO t1 VALUES(1, 'first');
INSERT OR REPLACE INTO t1 VALUES(1, 'second');
SELECT * FROM t2;
CREATE TABLE t3(a INTEGER PRIMARY KEY, v);
CREATE TABLE t4(b INTEGER PRIMARY KEY, w);
CREATE TRIGGER tr3 AFTER INSERT ON t3 BEGIN INSERT OR IGNORE INTO t4 VALUES(NEW.a, NEW.v); END;
INSERT INTO t3 VALUES(1, 'first');
INSERT OR REPLACE INTO t3 VALUES(1, 'second');
SELECT * FROM t4;
-- case: holdout/triggers/174-raise-with-a-non-literal-message-expression
CREATE TABLE t(a INTEGER);
CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a < 0 BEGIN SELECT RAISE(ABORT, 'bad: ' || NEW.a); END;
INSERT INTO t VALUES(-5);
SELECT RAISE(ABORT, 'x');
-- case: holdout/triggers/175-json-each-used-inside-a-trigger-body
CREATE TABLE t(a TEXT);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log SELECT value FROM json_each(NEW.a); END;
INSERT INTO t VALUES('[1,2,3]');
SELECT * FROM log;
-- case: holdout/triggers/205-trigger-body-that-fails-to-compile-after-add-colum
CREATE TABLE t1 (a INTEGER PRIMARY KEY, b INTEGER, c REAL NOT NULL);
CREATE TRIGGER trg AFTER INSERT ON t1 FOR EACH ROW BEGIN SELECT * FROM t1 WHERE 1 UNION ALL SELECT b, b, a FROM t1; END;
ALTER TABLE t1 ADD COLUMN d TEXT;
INSERT INTO t1 (a, b, c, d) VALUES (1, 2, 3, 'x');
INSERT INTO t1 (a, b, c, d) VALUES (2, 2, 3, 'x');
SELECT count(*) FROM t1;
-- case: holdout/triggers/236-rename-column-does-not-rewrite-a-cte-column-that-s
CREATE TABLE src(a, b);
CREATE TABLE dst(x);
CREATE TABLE log(v);
CREATE TRIGGER trig AFTER INSERT ON dst BEGIN INSERT INTO log WITH src AS (SELECT 100 AS b) SELECT b FROM src; END;
ALTER TABLE src RENAME COLUMN b TO c;
INSERT INTO dst VALUES(1);
SELECT * FROM log;
SELECT sql FROM sqlite_schema WHERE name = 'trig';
-- case: holdout/triggers/237-rename-column-that-makes-a-trigger-ambiguous-fails
CREATE TABLE src(a, b);
CREATE TABLE other(a, d);
CREATE TABLE dst(x);
CREATE TRIGGER trig AFTER INSERT ON dst BEGIN SELECT b FROM src JOIN other ON src.a = other.a WHERE src.a = new.x; END;
ALTER TABLE other RENAME COLUMN d TO b;
-- case: holdout/triggers/248-new-values-in-a-before-trigger-on-a-strict-table-h
CREATE TABLE t(x INTEGER) STRICT;
CREATE TABLE log(v TEXT);
CREATE TRIGGER bi BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.x)); END;
INSERT INTO t VALUES('1');
SELECT v FROM log;
CREATE TABLE t2(x INTEGER) STRICT;
INSERT INTO t2 VALUES(1);
CREATE TRIGGER bu BEFORE UPDATE ON t2 BEGIN INSERT INTO log VALUES('u' || typeof(NEW.x)); END;
UPDATE t2 SET x='2';
SELECT v FROM log;
CREATE TABLE t3(x REAL) STRICT;
CREATE TRIGGER bi3 BEFORE INSERT ON t3 BEGIN INSERT INTO log VALUES('r' || typeof(NEW.x)); END;
INSERT INTO t3 VALUES(5);
SELECT v FROM log;
-- case: holdout/triggers/260-rename-column-back-and-forth-keeps-the-stored-trig
CREATE TABLE log(msg TEXT);
CREATE TABLE core(id TEXT UNIQUE, row_rank TEXT UNIQUE);
CREATE TRIGGER core_ai AFTER INSERT ON core BEGIN INSERT INTO log VALUES(NEW.id || ':' || NEW.row_rank); END;
INSERT INTO core(id,row_rank) VALUES('i-001','A001');
ALTER TABLE core RENAME COLUMN row_rank TO row_rank2;
SELECT sql FROM sqlite_schema WHERE name='core_ai';
ALTER TABLE core RENAME COLUMN row_rank2 TO row_rank;
SELECT sql FROM sqlite_schema WHERE name='core_ai';
INSERT INTO core(id,row_rank) VALUES('i-002','A002');
SELECT * FROM log;
-- case: holdout/triggers/278-trigger-that-inserts-into-sqlite-sequence
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES('seed');
CREATE TRIGGER bi BEFORE INSERT ON t BEGIN INSERT INTO sqlite_sequence(name, seq) VALUES('t', 5); UPDATE sqlite_sequence SET seq = 4 WHERE name = 't'; END;
INSERT INTO t(v) VALUES('x');
PRAGMA integrity_check;
SELECT id, v FROM t ORDER BY id;
SELECT rowid, name, seq FROM sqlite_sequence ORDER BY rowid;
-- case: holdout/triggers/327-alter-table-rename-succeeds-when-an-unrelated-trig
CREATE TABLE a(x);
CREATE TABLE b(y);
CREATE TABLE c(z);
CREATE TRIGGER trg AFTER INSERT ON a BEGIN INSERT INTO b WITH cte_0 AS (SELECT 1 AS y) SELECT y FROM cte_0; END;
ALTER TABLE c RENAME TO c2;
SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;
INSERT INTO a VALUES (1);
SELECT * FROM b;
-- case: holdout/triggers/328-rename-column-with-a-temp-trigger-on-the-table
CREATE TABLE t(a, b);
CREATE TEMP TRIGGER trg BEFORE UPDATE ON t BEGIN SELECT 1; END;
ALTER TABLE t RENAME COLUMN a TO a2;
SELECT group_concat(name) FROM pragma_table_info('t');
-- case: holdout/triggers/423-old-id-and-new-id-in-a-trigger-keep-integer-primar
CREATE TABLE t(id INTEGER PRIMARY KEY, n INTEGER);
CREATE TABLE u(ref);
INSERT INTO t VALUES(1,1),(2,2);
INSERT INTO u VALUES('1');
CREATE TRIGGER ct AFTER DELETE ON t BEGIN DELETE FROM u WHERE ref = OLD.id; END;
DELETE FROM t WHERE id=1;
SELECT count(*) FROM u;
INSERT INTO u VALUES('1');
CREATE TRIGGER ci AFTER INSERT ON t BEGIN DELETE FROM u WHERE ref = NEW.id; END;
INSERT INTO t VALUES(1,1);
SELECT count(*) FROM u;
-- case: holdout/triggers/442-a-subquery-in-returning-reads-the-table-before-the
CREATE TABLE t(a INTEGER PRIMARY KEY);
CREATE TABLE u(a INTEGER);
INSERT INTO u VALUES(1);
CREATE TRIGGER ai AFTER INSERT ON t BEGIN UPDATE u SET a=a+1; END;
INSERT INTO t VALUES(1) RETURNING a, (SELECT a FROM u);
INSERT INTO t VALUES(2),(3) RETURNING a, (SELECT a FROM u);
SELECT * FROM u;
-- case: holdout/triggers/458-or-ignore-on-the-outer-statement-does-not-apply-to
CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER);
CREATE UNIQUE INDEX i1 ON t(k);
CREATE TABLE u(x INTEGER);
INSERT INTO t VALUES(1,10),(2,20);
INSERT INTO u VALUES(1);
CREATE TRIGGER tg0 BEFORE INSERT ON t BEGIN DELETE FROM u WHERE x=1; END;
CREATE TRIGGER tg1 AFTER DELETE ON u BEGIN UPDATE t SET k=k*2 WHERE id=1; END;
INSERT OR IGNORE INTO t VALUES(3,30);
SELECT count(*) FROM u;
SELECT * FROM t ORDER BY id;
-- case: holdout/triggers/476-rename-column-does-not-rewrite-a-same-named-column
CREATE TABLE t(old_col INTEGER);
CREATE TABLE source_rows(old_col INTEGER);
CREATE TABLE audit_log(seen INTEGER);
CREATE TRIGGER t_ai AFTER INSERT ON t BEGIN INSERT INTO audit_log SELECT old_col FROM source_rows; END;
ALTER TABLE t RENAME COLUMN old_col TO new_col;
INSERT INTO t(new_col) VALUES (99);
SELECT sql FROM sqlite_master WHERE name='t_ai';
-- case: holdout/triggers/477-rename-to-does-not-rename-a-trigger-that-has-the-s
CREATE TABLE ft(a INTEGER);
CREATE TABLE ts(a INTEGER);
CREATE TRIGGER ts AFTER INSERT ON ft BEGIN SELECT 1; END;
ALTER TABLE ts RENAME TO ts2;
SELECT type, name, tbl_name FROM sqlite_master WHERE type='trigger';
-- case: holdout/triggers/482-table-alias-in-update-delete-and-insert-inside-a-t
CREATE TABLE source(value INTEGER);
CREATE TABLE audit(id INTEGER PRIMARY KEY, value INTEGER);
INSERT INTO audit VALUES (1, 0), (2, 0);
CREATE TRIGGER tr1 AFTER INSERT ON source BEGIN UPDATE audit AS u SET value = u.value + NEW.value WHERE u.id = 1; END;
CREATE TRIGGER tr2 AFTER INSERT ON source BEGIN DELETE FROM audit AS d WHERE d.id = 2; END;
CREATE TRIGGER tr3 AFTER INSERT ON source BEGIN INSERT INTO audit AS i(id, value) VALUES (3, NEW.value); END;
INSERT INTO source VALUES (5);
SELECT * FROM audit ORDER BY id;
-- case: holdout/triggers/489-create-trigger-with-a-name-starting-with-sqlite
CREATE TABLE t(a);
CREATE TRIGGER sqlite_trg AFTER INSERT ON t BEGIN SELECT 1; END;
CREATE TRIGGER SQLITE_trg2 AFTER INSERT ON t BEGIN SELECT 1; END;
-- case: holdout/triggers/514-a-trigger-body-that-uses-a-table-alias-in-update
CREATE TABLE t(a INTEGER PRIMARY KEY, b INTEGER);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t AS x SET b = 1 WHERE x.a = NEW.a; END;
INSERT INTO t(a, b) VALUES (1, 0);
SELECT * FROM t;
SELECT sql FROM sqlite_master WHERE name = 'tr';
-- case: holdout/triggers/575-sum-raises-integer-overflow-total-does-not
create table t(a);
insert into t values (9223372036854775807);
select sum(a) from t;
insert into t values (1);
select sum(a) from t;
select total(a) from t;
select sum(a + 0.0) from t;
create table u(a);
insert into u values (-9223372036854775807), (-2);
select sum(a) from u;
-- case: holdout/triggers/607-new-inside-a-subquery-of-an-insert-select-in-a-tri
CREATE TABLE t (id INT PRIMARY KEY, n TEXT, m INT);
CREATE TABLE t1 (eid INT PRIMARY KEY, d INT);
CREATE TRIGGER trg AFTER INSERT ON t BEGIN INSERT INTO t1(eid, d) SELECT NEW.id, COALESCE((SELECT d FROM t1 WHERE eid = NEW.m), -1) + 1; END;
INSERT INTO t VALUES (1, 'CEO', NULL);
INSERT INTO t VALUES (2, 'VP', 1);
INSERT INTO t VALUES (3, 'Manager', 2);
SELECT * FROM t1 ORDER BY eid;
-- case: holdout/triggers/614-before-insert-trigger-that-modifies-the-table-read
CREATE TABLE t (a INT, b INT, c TEXT);
INSERT INTO t VALUES (1, 10, 'A'), (2, 20, 'B'), (3, 30, 'C');
CREATE INDEX idx ON t (a, b, c);
CREATE TRIGGER tr BEFORE INSERT ON t FOR EACH ROW BEGIN INSERT INTO t VALUES (999, 999, NEW.c); DELETE FROM t WHERE a = NEW.a; END;
INSERT INTO t SELECT * FROM t;
SELECT COUNT(*) FROM t;
INSERT INTO t SELECT * FROM (SELECT * FROM t);
SELECT COUNT(*) FROM t;
SELECT a, b, c FROM t ORDER BY rowid;
-- case: holdout/triggers/643-instead-of-trigger-on-a-view
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
CREATE VIEW v AS SELECT * FROM t;
SELECT * FROM v;
CREATE TRIGGER trv INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(NEW.id, NEW.val); END;
INSERT INTO v VALUES (1, 'x');
CREATE TRIGGER trd INSTEAD OF DELETE ON v BEGIN DELETE FROM t WHERE id = OLD.id; END;
CREATE TRIGGER tru INSTEAD OF UPDATE ON v BEGIN UPDATE t SET val = NEW.val WHERE id = OLD.id; END;
UPDATE v SET val = 'y' WHERE id = 1;
SELECT * FROM t;
DELETE FROM v;
SELECT count(*) FROM t;
-- case: holdout/triggers/690-upsert-do-update-fires-a-before-update-trigger-tha
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, v TEXT, UNIQUE(k, v));
INSERT INTO t VALUES (1, 'a', 'old');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET k = 'mid' WHERE id = OLD.id AND k <> 'mid'; END;
INSERT INTO t VALUES (1, 'ignored', 'ignored') ON CONFLICT(id) DO UPDATE SET v = 'new';
SELECT id, k, v FROM t ORDER BY id;
PRAGMA integrity_check;
-- case: holdout/triggers/700-drop-column-when-an-unrelated-trigger-names-its-ta
CREATE TABLE t1 (id TEXT PRIMARY KEY, updated_at REAL NOT NULL, extra TEXT);
CREATE TRIGGER trg AFTER UPDATE ON "t1" FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at BEGIN UPDATE t1 SET updated_at = julianday('now') WHERE id = NEW.id; END;
ALTER TABLE t1 DROP COLUMN extra;
SELECT name FROM pragma_table_info('t1');
-- case: holdout/triggers/702-rename-column-rewrites-new-col-in-a-temp-trigger-o
CREATE TABLE t(a, b);
CREATE TEMP TRIGGER trg AFTER UPDATE ON t BEGIN SELECT NEW.a; END;
ALTER TABLE t RENAME COLUMN a TO a2;
SELECT name FROM pragma_table_info('t');
SELECT sql FROM sqlite_temp_master;
-- case: holdout/triggers/703-error-message-text-when-rename-column-breaks-a-tri
CREATE TABLE t (a, b);
CREATE TRIGGER tr BEFORE UPDATE ON t WHEN t.b > 0 BEGIN SELECT 1; END;
ALTER TABLE t RENAME COLUMN b TO c;
CREATE TABLE u (a, b);
CREATE TRIGGER tr2 BEFORE UPDATE ON u WHEN b > 0 BEGIN SELECT 1; END;
ALTER TABLE u RENAME COLUMN b TO c;
CREATE TABLE t_check_qual (a INTEGER, b INTEGER CHECK (t_check_qual.a < b));
ALTER TABLE t_check_qual RENAME COLUMN a TO z;
SELECT sql FROM sqlite_master WHERE name = 't_check_qual';
-- case: holdout/triggers/707-drop-column-validates-triggers-that-use-a-cte-name
CREATE TABLE a(x);
CREATE TABLE b(y);
CREATE TABLE c(z, keep);
CREATE TABLE cte_0(missing);
CREATE TRIGGER trg AFTER INSERT ON a BEGIN INSERT INTO b WITH cte_0 AS (SELECT 1 AS y) SELECT cte_0.missing FROM cte_0; END;
ALTER TABLE c DROP COLUMN z;
SELECT name FROM pragma_table_info('c');
INSERT INTO a VALUES (1);
-- case: holdout/triggers/737-upsert-do-update-that-changes-nothing-still-fires-
CREATE TABLE evidence (sha BLOB PRIMARY KEY, body BLOB NOT NULL) STRICT;
CREATE TRIGGER evidence_append_only BEFORE UPDATE ON evidence BEGIN SELECT RAISE(ABORT, 'append-only'); END;
INSERT INTO evidence VALUES (X'01', X'02');
INSERT INTO evidence (sha, body) VALUES (X'01', X'02') ON CONFLICT(sha) DO UPDATE SET sha = excluded.sha, body = excluded.body;
INSERT INTO evidence (sha, body) VALUES (X'01', X'02') ON CONFLICT(sha) DO NOTHING;
SELECT hex(sha), hex(body) FROM evidence;
-- case: holdout/triggers/739-a-chain-of-recursive-triggers-1000-deep
WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i < 60) SELECT count(*) FROM n;
CREATE TABLE t0(x); CREATE TABLE t1(x); CREATE TABLE t2(x); CREATE TABLE t3(x);
CREATE TRIGGER tr0 AFTER INSERT ON t0 BEGIN INSERT INTO t1 VALUES(NEW.x); END;
CREATE TRIGGER tr1 AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.x); END;
CREATE TRIGGER tr2 AFTER INSERT ON t2 BEGIN INSERT INTO t3 VALUES(NEW.x); END;
INSERT INTO t0 VALUES(1);
SELECT count(*) FROM t3;
CREATE TABLE r(x);
CREATE TRIGGER rr AFTER INSERT ON r WHEN NEW.x < 5 BEGIN INSERT INTO r VALUES(NEW.x + 1); END;
INSERT INTO r VALUES(1);
SELECT count(*) FROM r;
PRAGMA recursive_triggers = ON;
INSERT INTO r VALUES(1);
SELECT count(*) FROM r;
-- case: holdout/triggers/749-rollback-to-a-savepoint-removes-trigger-written-ro
CREATE TABLE users(id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE events(id INTEGER PRIMARY KEY, uid INTEGER, what TEXT);
CREATE VIEW uv AS SELECT users.id AS uid, users.name, events.what FROM users JOIN events ON events.uid = users.id;
CREATE TRIGGER ai AFTER INSERT ON users BEGIN INSERT INTO events(uid, what) VALUES (NEW.id, 'created'); END;
INSERT INTO users VALUES (1, 'base');
SAVEPOINT s;
INSERT INTO users VALUES (2, 'second');
SELECT * FROM uv ORDER BY uid;
ROLLBACK TO s;
SELECT * FROM uv ORDER BY uid;
SELECT count(*) FROM events;
-- case: holdout/triggers/763-instead-of-triggers-and-returning-on-a-view
CREATE TABLE base(id INTEGER PRIMARY KEY, v);
CREATE VIEW vw AS SELECT id, v FROM base;
CREATE TRIGGER ti INSTEAD OF INSERT ON vw BEGIN INSERT INTO base VALUES (NEW.id, NEW.v); END;
CREATE TRIGGER tu INSTEAD OF UPDATE ON vw BEGIN UPDATE base SET v = NEW.v WHERE id = OLD.id; END;
CREATE TRIGGER td INSTEAD OF DELETE ON vw BEGIN DELETE FROM base WHERE id = OLD.id; END;
INSERT INTO vw VALUES (1, 'a') RETURNING id, v;
UPDATE vw SET v = 'b' WHERE id = 1 RETURNING id, v;
DELETE FROM vw WHERE id = 1 RETURNING id, v;
SELECT count(*) FROM base;
-- case: holdout/triggers/779-raise-ignore-in-a-before-delete-trigger-skips-only
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER td BEFORE DELETE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t;
SELECT * FROM t ORDER BY id;
SELECT changes();
-- case: holdout/triggers/780-raise-ignore-in-a-before-update-trigger-skips-only
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER tu BEFORE UPDATE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
UPDATE t SET v = 'z';
SELECT * FROM t ORDER BY id;
SELECT changes();
-- case: holdout/triggers/781-before-insert-raise-ignore-on-some-rows-keeps-retu
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
CREATE TRIGGER ti BEFORE INSERT ON t WHEN NEW.v = 'skip' BEGIN SELECT RAISE(IGNORE); END;
INSERT INTO t VALUES (1, 'a'), (2, 'skip'), (3, 'c') RETURNING id, v;
SELECT * FROM t ORDER BY id;
-- case: holdout/triggers/782-before-update-raise-ignore-on-some-rows-keeps-retu
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER tu BEFORE UPDATE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
UPDATE t SET v = 'z' RETURNING id, v;
-- case: holdout/triggers/783-before-delete-raise-ignore-on-some-rows-keeps-retu
CREATE TABLE t(id INTEGER PRIMARY KEY, v);
INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER td BEFORE DELETE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t RETURNING id, v;
-- case: holdout/triggers/792-new-rowid-and-old-rowid-inside-triggers
CREATE TABLE t(a);
CREATE TABLE log(k, r);
CREATE TRIGGER ai AFTER INSERT ON t BEGIN INSERT INTO log VALUES ('ins', NEW.rowid); END;
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES ('del', OLD.rowid); END;
CREATE TRIGGER au AFTER UPDATE OF a ON t BEGIN INSERT INTO log VALUES ('upd', NEW.rowid || '/' || OLD.rowid); END;
INSERT INTO t VALUES ('x'), ('y'), ('z');
DELETE FROM t WHERE a = 'y';
UPDATE t SET a = 'q' WHERE a = 'z';
UPDATE t SET rowid = 40 WHERE a = 'x';
SELECT * FROM log ORDER BY rowid;
CREATE TABLE u(a);
CREATE TABLE ulog(r);
CREATE TRIGGER uai AFTER INSERT ON u BEGIN INSERT INTO ulog VALUES (NEW.oid || ',' || NEW._rowid_); END;
INSERT INTO u VALUES (1);
SELECT * FROM ulog;
-- case: holdout/triggers/830-changes-after-a-delete-stopped-by-raise-fail
CREATE TABLE t(k INTEGER PRIMARY KEY);
CREATE TABLE log(k);
INSERT INTO t VALUES(1),(2),(3);
CREATE TRIGGER td BEFORE DELETE ON t BEGIN INSERT INTO log VALUES(OLD.k); SELECT CASE WHEN OLD.k=2 THEN RAISE(FAIL,'stop') END; END;
BEGIN;
INSERT INTO t VALUES(4);
DELETE FROM t WHERE k<=3;
SELECT changes();
SELECT k FROM t ORDER BY k;
SELECT k FROM log ORDER BY rowid;
COMMIT;
-- case: holdout/triggers/834-create-trigger-text-is-stored-as-written-in-sqlite
CREATE TABLE t(a, b);
CREATE TRIGGER   tr   AFTER   INSERT ON t WHEN ((NEW.a > 1) AND ((NEW.b < 5)))
BEGIN
  SELECT   1 ;  -- note
END;
SELECT sql FROM sqlite_master WHERE name = 'tr';
CREATE VIEW  v  AS   SELECT   (a) ,  b  FROM t ;
CREATE INDEX   i  ON t ( a ,  b  DESC ) ;
SELECT sql FROM sqlite_master WHERE name IN ('v', 'i') ORDER BY name;
