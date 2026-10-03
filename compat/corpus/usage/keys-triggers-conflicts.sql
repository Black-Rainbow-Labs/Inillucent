-- case: foreign-keys/r2-r14-collate-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE); CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES('A'); INSERT INTO c VALUES('a'); DELETE FROM p WHERE id='A'; SELECT count(*) FROM c;
CREATE TABLE p2(id TEXT PRIMARY KEY COLLATE NOCASE); CREATE TABLE c2(pid TEXT REFERENCES p2(id) ON UPDATE CASCADE);
INSERT INTO p2 VALUES('A'); INSERT INTO c2 VALUES('a'); UPDATE p2 SET id='B' WHERE id='A'; SELECT quote(pid) FROM c2;
-- case: foreign-keys/r2-r14-child-nocase
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY); CREATE TABLE c(pid TEXT COLLATE NOCASE REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES('A'); INSERT INTO c VALUES('a'); SELECT count(*) FROM c;
-- case: foreign-keys/r2-r14-child-nocase-del
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY); CREATE TABLE c(pid TEXT COLLATE NOCASE REFERENCES p(id) ON DELETE CASCADE);
PRAGMA foreign_keys=OFF;
INSERT INTO p VALUES('A'); INSERT INTO c VALUES('a'); INSERT INTO c VALUES('A');
PRAGMA foreign_keys=ON;
DELETE FROM p; SELECT quote(pid) FROM c;
-- case: foreign-keys/r2-r15-self
PRAGMA foreign_keys=ON; CREATE TABLE n(id INTEGER PRIMARY KEY, parent_id REFERENCES n(id));
INSERT INTO n VALUES (1, 2), (2, NULL); INSERT INTO n VALUES (3, 99); INSERT INTO n VALUES (4, 4); SELECT count(*) FROM n;
-- case: triggers/r2-r16-update-replace
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id INT REFERENCES parent(id) ON DELETE CASCADE);
INSERT INTO parent VALUES (10,'x'),(20,'y'); INSERT INTO child VALUES (10,10),(20,20);
UPDATE OR REPLACE parent SET name='y' WHERE id=10; SELECT * FROM child ORDER BY id;
-- case: triggers/r2-r16-trig
PRAGMA recursive_triggers=ON;
CREATE TABLE t1(id INTEGER PRIMARY KEY, v); CREATE TABLE log(m);
CREATE TRIGGER d AFTER DELETE ON t1 BEGIN INSERT INTO log VALUES('deleted id='||OLD.id); END;
CREATE TRIGGER i AFTER INSERT ON t1 BEGIN INSERT INTO log VALUES('inserted id='||NEW.id); END;
INSERT INTO t1 VALUES(1,1); REPLACE INTO t1 VALUES(1,100); SELECT * FROM log;
-- case: triggers/r2-r17
CREATE TABLE u(a); CREATE TRIGGER tu BEFORE INSERT ON u BEGIN INSERT INTO u VALUES(0); END;
INSERT INTO u VALUES(1); INSERT INTO u VALUES(2); SELECT count(*) FROM u;
-- case: triggers/r2-r18
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, v TEXT, UNIQUE(k, v)); INSERT INTO t VALUES (1,'a','old');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET k='mid' WHERE id=OLD.id AND k<>'mid'; END;
UPDATE t SET v='new' WHERE id=1; SELECT * FROM t;
INSERT INTO t VALUES (1,'x','x') ON CONFLICT(id) DO UPDATE SET v='new2'; SELECT * FROM t; PRAGMA integrity_check;
-- case: constraints/r2-r20
CREATE TABLE p(a TEXT, PRIMARY KEY(a COLLATE NOCASE)); INSERT INTO p VALUES ('a'); INSERT INTO p VALUES ('A');
CREATE TABLE q(a TEXT, UNIQUE(a COLLATE NOCASE DESC, a)); INSERT INTO q VALUES ('a'); INSERT INTO q VALUES ('A');
SELECT * FROM pragma_index_xinfo('sqlite_autoindex_q_1');
-- case: dml/r2-r24
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT 100, val TEXT DEFAULT 'x');
INSERT INTO t DEFAULT VALUES; INSERT INTO t(val) VALUES ('y'); INSERT INTO t DEFAULT VALUES; SELECT * FROM t;
-- case: dml/r2-r25
CREATE TABLE t(a INTEGER PRIMARY KEY); INSERT INTO t VALUES (-9223372036854775808); UPDATE t SET a = a - 1; SELECT * FROM t;
-- case: triggers/r2-r40
CREATE TABLE t(a REAL); CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.a)); END; INSERT INTO t VALUES(42);
CREATE TABLE x(x INTEGER) STRICT; CREATE TRIGGER bi BEFORE INSERT ON x BEGIN INSERT INTO log VALUES(typeof(NEW.x)); END;
INSERT INTO x VALUES('1'); SELECT * FROM log;
-- case: triggers/r2-r41
CREATE TABLE t1(a INTEGER PRIMARY KEY, v); CREATE TABLE t2(b INTEGER PRIMARY KEY, w);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.a, NEW.v); END;
INSERT INTO t1 VALUES(1,'first'); INSERT OR REPLACE INTO t1 VALUES(1,'second'); SELECT * FROM t2;
-- case: triggers/r2-r41b
CREATE TABLE t1(a INTEGER PRIMARY KEY, v); CREATE TABLE t2(b INTEGER PRIMARY KEY, w);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT OR IGNORE INTO t2 VALUES(NEW.a, NEW.v); END;
INSERT INTO t1 VALUES(1,'first'); INSERT OR REPLACE INTO t1 VALUES(1,'second'); SELECT * FROM t2;
-- case: triggers/r2-r42
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER td BEFORE DELETE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t RETURNING id, v;
-- case: upsert/r2-r47
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE); INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN; INSERT INTO t VALUES(3,'u3','a3');
INSERT OR ROLLBACK INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT count(*) FROM t WHERE id=3;
-- case: constraints/r2-r51
CREATE TABLE t(a); CREATE UNIQUE INDEX i ON t(lower(a)); INSERT INTO t VALUES('A'); INSERT INTO t VALUES('a');
-- case: autoincrement/r2-r66a
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT CHECK(val<>'bad'));
INSERT INTO t(val) VALUES('a'); UPDATE sqlite_sequence SET seq='5abc'; INSERT INTO t(val) VALUES('b'); SELECT * FROM t;
-- case: autoincrement/r2-r66b
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT CHECK(val<>'bad'));
INSERT INTO t(val) VALUES('a'); INSERT OR IGNORE INTO t(id,val) VALUES(10,'bad'); SELECT * FROM sqlite_sequence;
-- case: autoincrement/r2-r66c
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT UNIQUE);
INSERT INTO t(val) VALUES('a'); INSERT OR IGNORE INTO t(val) VALUES('a'); SELECT * FROM sqlite_sequence; INSERT INTO t(val) VALUES('c'); SELECT * FROM t;
-- case: autoincrement/r2-r66d
CREATE TABLE b(id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO b(x) SELECT seq FROM sqlite_sequence; SELECT * FROM sqlite_sequence;
-- case: autoincrement/r2-r66e
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO t VALUES(9223372036854775807,1); INSERT INTO t(x) VALUES(2);
-- case: upsert/r2-r67a
CREATE TABLE u(k PRIMARY KEY, n); INSERT INTO u VALUES(1,10);
INSERT INTO u AS excluded VALUES(1,99) ON CONFLICT(k) DO UPDATE SET n = excluded.n; SELECT * FROM u;
-- case: upsert/r2-r67b
CREATE TABLE t1(a INTEGER PRIMARY KEY, b); INSERT INTO t1 VALUES(10,'a');
INSERT INTO t1 VALUES(20,'b') RETURNING last_insert_rowid();
-- case: foreign-keys/r2-replace-cascade-set-null
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE);
CREATE TABLE c2(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE SET NULL);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c1 VALUES (1,1),(2,2); INSERT INTO c2 VALUES (1,1),(2,2);
INSERT OR REPLACE INTO p VALUES (3,'a');
SELECT * FROM p ORDER BY id; SELECT * FROM c1 ORDER BY id; SELECT * FROM c2 ORDER BY id;
REPLACE INTO p VALUES (2,'z');
SELECT * FROM p ORDER BY id; SELECT * FROM c1 ORDER BY id; SELECT * FROM c2 ORDER BY id;
-- case: foreign-keys/r2-replace-restrict
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE RESTRICT);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (1,1);
INSERT OR REPLACE INTO p VALUES (5,'a');
SELECT * FROM p ORDER BY id; SELECT * FROM c;
UPDATE OR REPLACE p SET name='a' WHERE id=2;
SELECT * FROM p ORDER BY id; SELECT * FROM c;
-- case: foreign-keys/r2-replace-no-action
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id));
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (1,1);
INSERT OR REPLACE INTO p VALUES (5,'a');
SELECT * FROM p ORDER BY id; SELECT * FROM c;
-- case: foreign-keys/r2-replace-returning-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (1,1),(2,2);
INSERT OR REPLACE INTO p VALUES (3,'a'),(4,'b') RETURNING id, name;
SELECT * FROM p ORDER BY id; SELECT count(*) FROM c;
UPDATE OR REPLACE p SET name='b' WHERE id=3 RETURNING id, name;
SELECT * FROM p ORDER BY id;
-- case: foreign-keys/r2-replace-two-conflicts
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES (1,'a1','b1'),(2,'a2','b2'); INSERT INTO c VALUES (1,1),(2,2),(3,2);
INSERT OR REPLACE INTO p VALUES (9,'a1','b2');
SELECT * FROM p; SELECT * FROM c;
-- case: foreign-keys/r2-replace-on-update-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON UPDATE CASCADE ON DELETE CASCADE);
INSERT INTO p VALUES (1,'a'),(2,'b'),(3,'c'); INSERT INTO c VALUES (1,1),(2,2),(3,3);
UPDATE OR REPLACE p SET id=2, name='b' WHERE id=1;
SELECT * FROM p ORDER BY id; SELECT * FROM c ORDER BY id;
UPDATE OR REPLACE p SET id=10 WHERE id=3;
SELECT * FROM p ORDER BY id; SELECT * FROM c ORDER BY id;
-- case: foreign-keys/r2-replace-self-ref
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent INT REFERENCES n(id) ON DELETE CASCADE, tag TEXT UNIQUE);
INSERT INTO n VALUES (1,NULL,'r'); INSERT INTO n VALUES (2,1,'x'),(3,2,'y'),(4,3,'z');
INSERT OR REPLACE INTO n VALUES (5,NULL,'x');
SELECT * FROM n ORDER BY id;
-- case: triggers/r2-replace-delete-triggers-off
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE log(m);
CREATE TRIGGER bd BEFORE DELETE ON t BEGIN INSERT INTO log VALUES('bd '||OLD.id); END;
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES('ad '||OLD.id); END;
CREATE TRIGGER ai AFTER INSERT ON t BEGIN INSERT INTO log VALUES('ai '||NEW.id); END;
INSERT INTO t VALUES (1,'a'); REPLACE INTO t VALUES (1,'b');
SELECT * FROM log; SELECT * FROM t;
-- case: triggers/r2-replace-delete-triggers-on
PRAGMA recursive_triggers=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE log(m);
CREATE TRIGGER bd BEFORE DELETE ON t BEGIN INSERT INTO log VALUES('bd '||OLD.id); END;
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES('ad '||OLD.id); END;
CREATE TRIGGER ai AFTER INSERT ON t BEGIN INSERT INTO log VALUES('ai '||NEW.id); END;
INSERT INTO t VALUES (1,'a'); REPLACE INTO t VALUES (1,'b');
SELECT * FROM log; SELECT * FROM t;
-- case: triggers/r2-replace-update-triggers-on
PRAGMA recursive_triggers=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, v TEXT); CREATE TABLE log(m);
CREATE TRIGGER bd BEFORE DELETE ON t BEGIN INSERT INTO log VALUES('bd '||OLD.id); END;
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES('ad '||OLD.id); END;
INSERT INTO t VALUES (1,'a','x'),(2,'b','y');
UPDATE OR REPLACE t SET u='a' WHERE id=2;
SELECT * FROM log; SELECT * FROM t;
-- case: triggers/r2-replace-update-triggers-off
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, v TEXT); CREATE TABLE log(m);
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES('ad '||OLD.id); END;
INSERT INTO t VALUES (1,'a','x'),(2,'b','y');
UPDATE OR REPLACE t SET u='a' WHERE id=2;
SELECT * FROM log; SELECT * FROM t;
-- case: triggers/r2-replace-before-delete-ignore
PRAGMA recursive_triggers=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
CREATE TRIGGER bd BEFORE DELETE ON t BEGIN SELECT RAISE(IGNORE); END;
INSERT INTO t VALUES (1,'a');
REPLACE INTO t VALUES (1,'b');
SELECT * FROM t;
-- case: foreign-keys/r2-replace-conflict-clause-on-column
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, u TEXT UNIQUE ON CONFLICT REPLACE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (1,1),(2,2);
INSERT INTO p VALUES (3,'a');
SELECT * FROM p ORDER BY id; SELECT * FROM c ORDER BY id;
UPDATE p SET u='b' WHERE id=3;
SELECT * FROM p ORDER BY id; SELECT * FROM c ORDER BY id;
-- case: triggers/r2-bu-trigger-variations
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b TEXT, c TEXT); CREATE INDEX tb ON t(b); CREATE INDEX tc ON t(c, a);
INSERT INTO t VALUES (1,'a1','b1','c1'),(2,'a2','b2','c2');
CREATE TRIGGER bu BEFORE UPDATE OF a ON t BEGIN UPDATE t SET b = 'trig-'||OLD.id, c = 'tc' WHERE id = OLD.id; END;
UPDATE t SET a = 'new' WHERE id = 1;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
UPDATE t SET a = 'new2', b = 'mine' WHERE id = 2;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
SELECT id FROM t WHERE b = 'mine'; SELECT id FROM t WHERE c = 'tc' ORDER BY id;
-- case: triggers/r2-bu-trigger-deletes-row
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT); CREATE TABLE log(m);
INSERT INTO t VALUES (1,'a1','b1'),(2,'a2','b2');
CREATE TRIGGER bu BEFORE UPDATE ON t WHEN OLD.id = 1 BEGIN DELETE FROM t WHERE id = OLD.id; END;
UPDATE t SET b = 'x'; SELECT * FROM t; PRAGMA integrity_check;
-- case: triggers/r2-bu-trigger-same-column
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT);
INSERT INTO t VALUES (1,'a1','b1'),(2,'a2','b2');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET a = 'zz', b = 'inner' WHERE id = OLD.id; END;
UPDATE t SET a = 'outer' WHERE id = 1;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-bu-trigger-moves-unique
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT);
INSERT INTO t VALUES (1,'a1','b1'),(2,'a2','b2');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET a = 'a2' WHERE id = 1 AND OLD.id = 1 AND a <> 'a2'; END;
UPDATE t SET b = 'q' WHERE id = 1;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-bu-trigger-outer-replace
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT);
INSERT INTO t VALUES (1,'a1','b1'),(2,'a2','b2'),(3,'a3','b3');
CREATE TRIGGER bu BEFORE UPDATE ON t WHEN OLD.id = 1 BEGIN UPDATE t SET a = 'a3' WHERE id = 2; END;
UPDATE OR REPLACE t SET b = 'q' WHERE id = 1;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-bu-trigger-partial-index
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b TEXT); CREATE INDEX pa ON t(a) WHERE a > 5; CREATE INDEX pb ON t(b);
INSERT INTO t VALUES (1,1,'x'),(2,9,'y');
CREATE TRIGGER bu BEFORE UPDATE ON t WHEN OLD.id = 1 BEGIN UPDATE t SET a = 50 WHERE id = 1; END;
UPDATE t SET b = 'z' WHERE id = 1;
SELECT * FROM t ORDER BY id; SELECT id FROM t WHERE a > 5 ORDER BY a; PRAGMA integrity_check;
-- case: dml/r2-bu-trigger-rowid-update
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b TEXT); CREATE TABLE log(m);
INSERT INTO t VALUES (1,'a','b');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES(OLD.id||'->'||NEW.id||' '||OLD.a||'->'||NEW.a); UPDATE t SET b='side' WHERE id=OLD.id; END;
CREATE TRIGGER au AFTER UPDATE ON t BEGIN INSERT INTO log VALUES('after '||OLD.b||'->'||NEW.b); END;
UPDATE t SET id = 7, a = 'aa' WHERE id = 1;
SELECT * FROM t; SELECT * FROM log; PRAGMA integrity_check;
-- case: upsert/r2-upsert-bu-trigger
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INT, w INT); CREATE INDEX tw ON t(w);
INSERT INTO t VALUES (1,'a',1,1);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET w = 99 WHERE id = OLD.id AND w <> 99; END;
INSERT INTO t VALUES (2,'a',5,5) ON CONFLICT(k) DO UPDATE SET v = excluded.v;
SELECT * FROM t; SELECT id FROM t WHERE w = 99; PRAGMA integrity_check;
-- case: triggers/r2-before-insert-own-table
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT);
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(v) VALUES('from-trigger-'||NEW.v); END;
INSERT INTO s(v) VALUES ('a'); INSERT INTO s(v) VALUES ('b'),('c');
SELECT * FROM s ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-before-insert-own-table-explicit-key
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT);
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(id, v) VALUES(NEW.id + 100, 'x'); END;
INSERT INTO s VALUES (1,'a');
SELECT * FROM s ORDER BY id;
INSERT INTO s VALUES (101,'dup');
-- case: autoincrement/r2-before-insert-own-table-autoinc
CREATE TABLE s(id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT);
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(v) VALUES('t-'||NEW.v); END;
INSERT INTO s(v) VALUES ('a'); INSERT INTO s(v) VALUES ('b');
SELECT * FROM s ORDER BY id; SELECT * FROM sqlite_sequence; PRAGMA integrity_check;
-- case: triggers/r2-before-insert-own-table-returning
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT);
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(v) VALUES('t'); END;
INSERT INTO s(v) VALUES ('a') RETURNING id, v;
SELECT * FROM s ORDER BY id;
-- case: triggers/r2-after-insert-own-table
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT);
CREATE TRIGGER ta AFTER INSERT ON s BEGIN INSERT INTO s(v) VALUES('t-'||NEW.v); END;
INSERT INTO s(v) VALUES ('a'); INSERT INTO s(v) VALUES ('b');
SELECT * FROM s ORDER BY id;
-- case: foreign-keys/r2-fk-collate-composite
PRAGMA foreign_keys=ON;
CREATE TABLE p(a TEXT COLLATE NOCASE, b TEXT COLLATE BINARY, PRIMARY KEY(a, b));
CREATE TABLE c(id INTEGER PRIMARY KEY, x TEXT, y TEXT, FOREIGN KEY(x, y) REFERENCES p(a, b) ON DELETE CASCADE ON UPDATE CASCADE);
INSERT INTO p VALUES ('Ab','Q'); INSERT INTO c VALUES (1,'ab','Q'),(2,'AB','q');
SELECT * FROM c ORDER BY id;
UPDATE p SET a='Zed' WHERE a='ab';
SELECT * FROM c ORDER BY id;
DELETE FROM p; SELECT count(*) FROM c;
-- case: foreign-keys/r2-fk-collate-insert-check
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c(pid TEXT REFERENCES p(id));
INSERT INTO p VALUES ('Abc');
INSERT INTO c VALUES ('aBC');
INSERT INTO c VALUES ('abd');
SELECT * FROM c; PRAGMA foreign_key_check;
-- case: foreign-keys/r2-fk-collate-restrict
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE RESTRICT);
INSERT INTO p VALUES ('Abc'); INSERT INTO c VALUES ('aBC');
DELETE FROM p;
SELECT count(*) FROM p;
-- case: foreign-keys/r2-fk-collate-set-null
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE SET NULL);
INSERT INTO p VALUES ('Abc'); INSERT INTO c VALUES ('aBC'),('zzz');
PRAGMA foreign_keys=OFF; INSERT INTO c VALUES ('abc2'); PRAGMA foreign_keys=ON;
DELETE FROM p;
SELECT quote(pid) FROM c ORDER BY rowid;
-- case: foreign-keys/r2-fk-self-ref-variations
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent INT REFERENCES n(id));
INSERT INTO n VALUES (1,1);
INSERT INTO n VALUES (2,3);
INSERT INTO n VALUES (3,2);
UPDATE n SET parent = 5 WHERE id = 1;
UPDATE n SET parent = id WHERE id = 1;
SELECT * FROM n ORDER BY id;
DELETE FROM n WHERE id = 1; SELECT count(*) FROM n;
-- case: foreign-keys/r2-fk-self-ref-text-key
PRAGMA foreign_keys=ON;
CREATE TABLE n(code TEXT PRIMARY KEY, up TEXT REFERENCES n(code));
INSERT INTO n VALUES ('a','a');
INSERT INTO n VALUES ('b','c');
INSERT INTO n VALUES ('c','b');
SELECT * FROM n ORDER BY code;
-- case: foreign-keys/r2-fk-self-ref-composite
PRAGMA foreign_keys=ON;
CREATE TABLE n(a INT, b INT, pa INT, pb INT, PRIMARY KEY(a, b), FOREIGN KEY(pa, pb) REFERENCES n(a, b));
INSERT INTO n VALUES (1,1,1,1);
INSERT INTO n VALUES (2,2,1,2);
INSERT INTO n VALUES (3,3,3,4);
SELECT * FROM n ORDER BY a;
-- case: foreign-keys/r2-fk-self-ref-without-rowid
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INT PRIMARY KEY, parent INT REFERENCES n(id)) WITHOUT ROWID;
INSERT INTO n VALUES (1,1);
INSERT INTO n VALUES (2,9);
SELECT * FROM n;
-- case: constraints/r2-unique-collate-desc-forms
CREATE TABLE t1(a TEXT, b TEXT, UNIQUE(a COLLATE NOCASE DESC, b));
CREATE TABLE t2(a TEXT, b TEXT, PRIMARY KEY(a COLLATE NOCASE, b COLLATE RTRIM));
CREATE TABLE t3(a TEXT, b TEXT, UNIQUE(b DESC), UNIQUE(a COLLATE NOCASE));
INSERT INTO t1 VALUES ('x','y'); INSERT INTO t2 VALUES ('x','y '); INSERT INTO t3 VALUES ('x','y');
INSERT INTO t1 VALUES ('X','y');
INSERT INTO t2 VALUES ('X','y');
INSERT INTO t3 VALUES ('X','z');
INSERT INTO t3 VALUES ('Z','y');
SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name;
SELECT * FROM pragma_index_list('t1'); SELECT * FROM pragma_index_xinfo('sqlite_autoindex_t2_1');
SELECT * FROM pragma_index_xinfo('sqlite_autoindex_t3_1'); SELECT * FROM pragma_index_xinfo('sqlite_autoindex_t3_2');
-- case: constraints/r2-unique-collate-replace
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, UNIQUE(a COLLATE NOCASE) ON CONFLICT REPLACE);
INSERT INTO t VALUES (1,'abc'),(2,'def'); INSERT INTO t VALUES (3,'ABC');
SELECT * FROM t ORDER BY id;
SELECT id FROM t WHERE a = 'DEF'; SELECT id FROM t WHERE a = 'def';
-- case: constraints/r2-unique-collate-alter
CREATE TABLE t(a TEXT, b TEXT, UNIQUE(a COLLATE NOCASE, b));
ALTER TABLE t RENAME COLUMN a TO aa;
SELECT sql FROM sqlite_master WHERE name='t';
INSERT INTO t VALUES ('q','z'); INSERT INTO t VALUES ('Q','z');
-- case: constraints/r2-pk-collate-without-rowid
CREATE TABLE w(a TEXT, b INT, PRIMARY KEY(a COLLATE NOCASE DESC, b)) WITHOUT ROWID;
INSERT INTO w VALUES ('a',1); INSERT INTO w VALUES ('A',1);
INSERT INTO w VALUES ('A',2);
SELECT * FROM w ORDER BY a, b;
SELECT * FROM w WHERE a = 'A'; SELECT * FROM w WHERE a = 'a' COLLATE NOCASE ORDER BY b;
SELECT * FROM w WHERE a = 'a' AND b = 2;
CREATE TABLE w2(a TEXT, b INT, PRIMARY KEY(a COLLATE NOCASE, b)) WITHOUT ROWID;
INSERT INTO w2 VALUES ('a',1),('B',2); SELECT * FROM w2 WHERE a = 'A'; SELECT * FROM w2 WHERE a = 'b' COLLATE NOCASE AND b = 2;
SELECT * FROM w2 ORDER BY a;
-- case: dml/r2-default-values-variations
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT 100, val TEXT DEFAULT 'x');
INSERT INTO t(val) VALUES ('y'); INSERT INTO t(id) VALUES (NULL); INSERT INTO t VALUES (NULL, 'n');
INSERT INTO t DEFAULT VALUES; INSERT INTO t(val) SELECT 'q';
SELECT * FROM t ORDER BY id;
CREATE TABLE u (id INTEGER PRIMARY KEY DEFAULT (abs(-5)), v INT DEFAULT 3);
INSERT INTO u(v) VALUES (1); INSERT INTO u DEFAULT VALUES; SELECT * FROM u;
CREATE TABLE a2 (id INTEGER PRIMARY KEY AUTOINCREMENT DEFAULT 50);
INSERT INTO a2 DEFAULT VALUES; INSERT INTO a2 DEFAULT VALUES; SELECT * FROM a2;
-- case: dml/r2-rowid-update-types
CREATE TABLE t(a INTEGER PRIMARY KEY, b); INSERT INTO t VALUES (1,'x'),(2,'y');
UPDATE t SET a = '5' WHERE a = 1; SELECT * FROM t ORDER BY a;
UPDATE t SET a = 7.0 WHERE a = 2; SELECT * FROM t ORDER BY a;
UPDATE t SET a = 7.5 WHERE a = 5;
UPDATE t SET a = 'abc' WHERE a = 5;
UPDATE t SET a = NULL WHERE a = 5;
SELECT * FROM t ORDER BY a;
UPDATE t SET a = 9223372036854775807 WHERE a = 5;
UPDATE t SET a = a + 1 WHERE a = 9223372036854775807;
SELECT typeof(a), a FROM t ORDER BY a;
UPDATE t SET rowid = 'x' WHERE a = 7;
UPDATE t SET rowid = 1e300 WHERE a = 7;
-- case: dml/r2-rowid-update-plain-table
CREATE TABLE t(a, b); INSERT INTO t VALUES (1,'x'),(2,'y');
UPDATE t SET rowid = rowid + 10; SELECT rowid, * FROM t ORDER BY rowid;
UPDATE t SET rowid = 'q' WHERE rowid = 11;
UPDATE t SET rowid = -9223372036854775807 - 1 WHERE rowid = 11;
UPDATE t SET rowid = rowid - 1 WHERE rowid = -9223372036854775807 - 1;
SELECT rowid, * FROM t ORDER BY rowid;
-- case: triggers/r2-before-trigger-affinity
CREATE TABLE t(i INTEGER, r REAL, tx TEXT, n NUMERIC, b BLOB); CREATE TABLE log(m);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.i)||' '||typeof(NEW.r)||' '||typeof(NEW.tx)||' '||typeof(NEW.n)||' '||typeof(NEW.b)); END;
CREATE TRIGGER tu BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES('u '||typeof(NEW.i)||' '||typeof(NEW.r)||' '||typeof(NEW.tx)||' '||typeof(NEW.n)); END;
INSERT INTO t VALUES ('12', 3, 4, '5.0', 6);
UPDATE t SET i = '77', r = '8', tx = 9, n = '10';
SELECT * FROM log; SELECT typeof(i), typeof(r), typeof(tx), typeof(n), typeof(b) FROM t;
-- case: triggers/r2-before-trigger-strict
CREATE TABLE x(a INTEGER, b TEXT, c REAL, d ANY) STRICT; CREATE TABLE log(m);
CREATE TRIGGER bi BEFORE INSERT ON x BEGIN INSERT INTO log VALUES(typeof(NEW.a)||typeof(NEW.b)||typeof(NEW.c)||typeof(NEW.d)); END;
INSERT INTO x VALUES ('1', 2, '3.5', '4');
INSERT INTO x VALUES ('abc', 2, 3, 4);
SELECT * FROM log;
-- case: triggers/r2-outer-clause-override
CREATE TABLE t1(a INTEGER PRIMARY KEY, v); CREATE TABLE t2(b INTEGER PRIMARY KEY, w UNIQUE);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT OR ABORT INTO t2 VALUES(NEW.a, NEW.v); END;
INSERT INTO t1 VALUES(1,'first'); INSERT OR REPLACE INTO t1 VALUES(2,'first');
SELECT * FROM t2 ORDER BY b;
INSERT OR IGNORE INTO t1 VALUES(3,'first');
SELECT * FROM t2 ORDER BY b; SELECT * FROM t1 ORDER BY a;
-- case: triggers/r2-outer-clause-update
CREATE TABLE t1(a INTEGER PRIMARY KEY, v); CREATE TABLE t2(b INTEGER PRIMARY KEY, w UNIQUE);
INSERT INTO t2 VALUES (1,'x'),(2,'y');
CREATE TRIGGER tr AFTER UPDATE ON t1 BEGIN UPDATE t2 SET w = 'y' WHERE b = 1; END;
INSERT INTO t1 VALUES (1,'a');
UPDATE t1 SET v = 'b';
SELECT * FROM t2 ORDER BY b;
UPDATE OR REPLACE t1 SET v = 'c';
SELECT * FROM t2 ORDER BY b;
-- case: triggers/r2-outer-clause-ignore-then-delete-resets
CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER UNIQUE); CREATE TABLE u(x INTEGER);
INSERT INTO t VALUES(1,10),(2,20); INSERT INTO u VALUES(1);
CREATE TRIGGER tg0 BEFORE INSERT ON t BEGIN DELETE FROM u WHERE x=1; END;
CREATE TRIGGER tg1 AFTER DELETE ON u BEGIN UPDATE t SET k=20 WHERE id=1; END;
INSERT OR REPLACE INTO t VALUES(3,30);
SELECT count(*) FROM u; SELECT * FROM t ORDER BY id;
-- case: triggers/r2-raise-ignore-returning
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER tu BEFORE UPDATE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
UPDATE t SET v = v || '!' RETURNING id, v;
CREATE TRIGGER ti BEFORE INSERT ON t WHEN NEW.v = 'skip' BEGIN SELECT RAISE(IGNORE); END;
INSERT INTO t VALUES (10,'skip'),(11,'ok') RETURNING id, v;
CREATE TRIGGER ta AFTER DELETE ON t WHEN OLD.id = 3 BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t RETURNING id;
SELECT * FROM t ORDER BY id;
-- case: triggers/r2-raise-ignore-delete-changes
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER td BEFORE DELETE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t; SELECT changes(), count(*) FROM t;
-- case: upsert/r2-rollback-upsert-variants
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE); INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN; INSERT INTO t VALUES(3,'u3','a3');
INSERT OR FAIL INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT count(*) FROM t WHERE id=3;
INSERT OR ROLLBACK INTO t(u,a) VALUES('u9','a3');
SELECT count(*) FROM t WHERE id=3;
-- case: upsert/r2-rollback-upsert-ok
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE); INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN; INSERT INTO t VALUES(3,'u3','a3');
INSERT OR ROLLBACK INTO t(u,a) VALUES('u1','a9') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT * FROM t ORDER BY id; COMMIT;
-- case: upsert/r2-rollback-upsert-check
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, q INT CHECK(q < 10)); INSERT INTO t VALUES(1,'u1',1);
BEGIN; INSERT INTO t VALUES(2,'u2',2);
INSERT OR ROLLBACK INTO t VALUES(5,'u1',3) ON CONFLICT(u) DO UPDATE SET q = 50;
SELECT count(*) FROM t; COMMIT; SELECT count(*) FROM t;
-- case: constraints/r2-expr-index-messages
CREATE TABLE t(a, b); CREATE UNIQUE INDEX ie ON t(lower(a), b); CREATE UNIQUE INDEX ip ON t(b, a) WHERE b > 0;
INSERT INTO t VALUES ('X', 1);
INSERT INTO t VALUES ('x', 1);
INSERT INTO t VALUES ('Y', 2);
UPDATE t SET a = 'x', b = 1 WHERE b = 2;
INSERT INTO t VALUES ('q', NULL), ('q', NULL);
SELECT count(*) FROM t;
CREATE TABLE s(a TEXT, UNIQUE(a)); CREATE UNIQUE INDEX sc ON s(a COLLATE NOCASE);
INSERT INTO s VALUES ('a'); INSERT INTO s VALUES ('A');
-- case: autoincrement/r2-autoinc-variants
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT UNIQUE);
INSERT INTO t(v) VALUES ('a'),('b'); DELETE FROM t; INSERT INTO t(v) VALUES ('c');
SELECT * FROM t; SELECT * FROM sqlite_sequence;
REPLACE INTO t VALUES (NULL, 'c'); SELECT * FROM t; SELECT * FROM sqlite_sequence;
INSERT OR IGNORE INTO t(v) VALUES ('c'); SELECT * FROM sqlite_sequence;
INSERT OR IGNORE INTO t VALUES (100, 'c'); SELECT * FROM sqlite_sequence;
INSERT OR IGNORE INTO t VALUES (200, 'e'); SELECT * FROM sqlite_sequence;
-- case: autoincrement/r2-autoinc-seq-edits
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES ('a');
UPDATE sqlite_sequence SET seq = '12abc'; INSERT INTO t(v) VALUES ('b');
UPDATE sqlite_sequence SET seq = 20.9; INSERT INTO t(v) VALUES ('c');
UPDATE sqlite_sequence SET seq = -5; INSERT INTO t(v) VALUES ('d');
UPDATE sqlite_sequence SET seq = NULL; INSERT INTO t(v) VALUES ('e');
UPDATE sqlite_sequence SET seq = 'zzz'; INSERT INTO t(v) VALUES ('f');
SELECT * FROM t ORDER BY id; SELECT * FROM sqlite_sequence;
-- case: autoincrement/r2-autoinc-empty-statements
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) SELECT 1 WHERE 0; SELECT * FROM sqlite_sequence;
CREATE TABLE u(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO u SELECT * FROM t; SELECT * FROM sqlite_sequence ORDER BY name;
-- case: autoincrement/r2-autoinc-rollback
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
BEGIN; INSERT INTO t(v) VALUES (1),(2); ROLLBACK;
SELECT * FROM sqlite_sequence; INSERT INTO t(v) VALUES (3); SELECT * FROM t; SELECT * FROM sqlite_sequence;
-- case: upsert/r2-alias-excluded
CREATE TABLE u(k PRIMARY KEY, n, m); INSERT INTO u VALUES(1,10,20);
INSERT INTO u AS excluded VALUES(1,99,98) ON CONFLICT(k) DO UPDATE SET n = excluded.n + 1, m = excluded.m;
SELECT * FROM u;
INSERT INTO u AS x VALUES(1,5,6) ON CONFLICT(k) DO UPDATE SET n = x.n + excluded.n, m = x.m;
SELECT * FROM u;
INSERT INTO u AS Excluded VALUES(1,5,6) ON CONFLICT(k) DO UPDATE SET n = 0 WHERE excluded.n = 10;
SELECT * FROM u;
-- case: upsert/r2-returning-last-insert-rowid
CREATE TABLE t1(a INTEGER PRIMARY KEY, b); INSERT INTO t1 VALUES(10,'a');
INSERT INTO t1 VALUES(20,'b'),(30,'c') RETURNING a, last_insert_rowid();
INSERT INTO t1(b) VALUES('d') RETURNING a, last_insert_rowid() - a;
INSERT INTO t1 VALUES(30,'z') ON CONFLICT(a) DO UPDATE SET b = 'upd' RETURNING a, b;
SELECT last_insert_rowid();
UPDATE t1 SET b = 'u' WHERE a = 10 RETURNING last_insert_rowid();
DELETE FROM t1 WHERE a = 10 RETURNING last_insert_rowid();
-- case: foreign-keys/r2-replace-cascade-fires-child-triggers
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE); CREATE TABLE log(m);
CREATE TRIGGER cd AFTER DELETE ON c BEGIN INSERT INTO log VALUES('child gone '||OLD.id); END;
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (10,1),(11,1),(12,2);
REPLACE INTO p VALUES (3,'a');
SELECT * FROM log ORDER BY rowid; SELECT * FROM c ORDER BY id;
-- case: foreign-keys/r2-replace-set-default
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT DEFAULT 2 REFERENCES p(id) ON DELETE SET DEFAULT);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (10,1),(11,2);
REPLACE INTO p VALUES (3,'a');
SELECT * FROM c ORDER BY id;
-- case: foreign-keys/r2-replace-without-rowid-parent
PRAGMA foreign_keys=ON;
CREATE TABLE p(k TEXT PRIMARY KEY, v TEXT UNIQUE) WITHOUT ROWID;
CREATE TABLE c(id INTEGER PRIMARY KEY, pk TEXT REFERENCES p(k) ON DELETE CASCADE);
INSERT INTO p VALUES ('a','x'),('b','y'); INSERT INTO c VALUES (1,'a'),(2,'b');
INSERT OR REPLACE INTO p VALUES ('c','x');
SELECT * FROM p ORDER BY k; SELECT * FROM c ORDER BY id;
INSERT OR REPLACE INTO p VALUES ('b','z');
SELECT * FROM p ORDER BY k; SELECT * FROM c ORDER BY id;
-- case: foreign-keys/r2-replace-in-trigger-body-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE);
CREATE TABLE src(x); CREATE TRIGGER s AFTER INSERT ON src BEGIN INSERT OR REPLACE INTO p VALUES (NEW.x, 'a'); END;
INSERT INTO p VALUES (1,'a'); INSERT INTO c VALUES (1,1);
INSERT INTO src VALUES (7);
SELECT * FROM p; SELECT * FROM c;
-- case: foreign-keys/r2-replace-deferred-fk
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO p VALUES (1,'a'); INSERT INTO c VALUES (1,1);
BEGIN; REPLACE INTO p VALUES (2,'a'); COMMIT;
SELECT * FROM p; SELECT * FROM c;
BEGIN; REPLACE INTO p VALUES (1,'a'); REPLACE INTO p VALUES (2,'a'); UPDATE c SET pid = 2; COMMIT;
SELECT * FROM p; SELECT * FROM c;
-- case: foreign-keys/r2-update-or-replace-cascade-child-update
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE ON UPDATE CASCADE, tag TEXT UNIQUE);
INSERT INTO p VALUES (1,'a'),(2,'b'); INSERT INTO c VALUES (1,1,'t1'),(2,2,'t2');
UPDATE OR REPLACE p SET id = 5, name = 'b' WHERE id = 1;
SELECT * FROM p ORDER BY id; SELECT * FROM c ORDER BY id;
-- case: triggers/r2-update-returning-after-before-trigger
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b TEXT);
INSERT INTO t VALUES (1,'a','b');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET b = 'trigger' WHERE id = OLD.id; END;
UPDATE t SET a = 'new' WHERE id = 1 RETURNING id, a, b;
SELECT * FROM t;
-- case: triggers/r2-update-generated-before-trigger
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b INT, g INT GENERATED ALWAYS AS (a + b) STORED, h INT AS (a * 2)); CREATE INDEX tg ON t(g);
INSERT INTO t(id, a, b) VALUES (1,1,2);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET b = 100 WHERE id = OLD.id AND b <> 100; END;
UPDATE t SET a = 5 WHERE id = 1;
SELECT * FROM t; SELECT id FROM t WHERE g = 105; PRAGMA integrity_check;
-- case: triggers/r2-multi-row-bu-trigger
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b INT UNIQUE);
INSERT INTO t VALUES (1,1,10),(2,2,20),(3,3,30);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET b = b + 1000 WHERE id = OLD.id AND b < 1000; END;
UPDATE t SET a = a * 10;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-bu-trigger-update-from
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b INT); CREATE TABLE s(id INT, v INT);
INSERT INTO t VALUES (1,1,1),(2,2,2); INSERT INTO s VALUES (1,50),(2,60);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET b = -OLD.id WHERE id = OLD.id; END;
UPDATE t SET a = s.v FROM s WHERE s.id = t.id;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
-- case: triggers/r2-insert-select-own-table-trigger
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE src(x);
INSERT INTO src VALUES ('a'),('b'),('c');
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(v) VALUES('T'||NEW.v); END;
INSERT INTO s(v) SELECT x FROM src;
SELECT count(*), min(id), max(id) FROM s; SELECT v FROM s ORDER BY id;
-- case: triggers/r2-insert-or-replace-own-table-trigger
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT UNIQUE);
CREATE TRIGGER tb BEFORE INSERT ON s BEGIN INSERT INTO s(v) VALUES('T'||NEW.v); END;
INSERT INTO s(v) VALUES ('a');
INSERT OR REPLACE INTO s(v) VALUES ('Ta');
SELECT * FROM s ORDER BY id;
-- case: triggers/r2-before-insert-skip-then-next
CREATE TABLE s(id INTEGER PRIMARY KEY, v TEXT); CREATE TABLE log(m);
CREATE TRIGGER tb BEFORE INSERT ON s WHEN NEW.v = 'skip' BEGIN INSERT INTO s(v) VALUES('ins-in-trigger'); SELECT RAISE(IGNORE); END;
INSERT INTO s(v) VALUES ('a'),('skip'),('b');
SELECT * FROM s ORDER BY id;
-- case: foreign-keys/r2-fk-self-ref-update-delete-cascade
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent INT REFERENCES n(id) ON DELETE CASCADE ON UPDATE CASCADE);
INSERT INTO n VALUES (1,1); INSERT INTO n VALUES (2,1),(3,2);
UPDATE n SET id = 10 WHERE id = 1;
SELECT * FROM n ORDER BY id;
DELETE FROM n WHERE id = 10; SELECT count(*) FROM n;
-- case: foreign-keys/r2-fk-self-ref-set-null
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent INT REFERENCES n(id) ON DELETE SET NULL);
INSERT INTO n VALUES (1,1); INSERT INTO n VALUES (2,1),(3,3);
DELETE FROM n WHERE id = 1;
SELECT * FROM n ORDER BY id;
-- case: foreign-keys/r2-fk-self-ref-restrict
PRAGMA foreign_keys=ON;
CREATE TABLE n(id INTEGER PRIMARY KEY, parent INT REFERENCES n(id));
INSERT INTO n VALUES (1,1);
DELETE FROM n WHERE id = 1;
SELECT * FROM n;
INSERT INTO n VALUES (2,1);
DELETE FROM n WHERE id = 1;
SELECT count(*) FROM n;
-- case: foreign-keys/r2-fk-collate-text-numeric
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE RTRIM);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES ('a  '); INSERT INTO c VALUES ('a');
SELECT quote(pid) FROM c;
DELETE FROM p; SELECT count(*) FROM c;
-- case: foreign-keys/r2-fk-update-parent-key-collation
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE, n INT);
CREATE TABLE c(pid TEXT REFERENCES p(id) ON UPDATE CASCADE);
INSERT INTO p VALUES ('Abc', 1); INSERT INTO c VALUES ('aBC');
UPDATE p SET id = 'ABC';
SELECT * FROM p; SELECT quote(pid) FROM c;
UPDATE p SET n = 2; SELECT * FROM c;
UPDATE p SET id = 'xyz'; SELECT quote(pid) FROM c;
-- case: upsert/r2-upsert-trigger-own-update
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INT);
CREATE TABLE log(m);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES('bu '||OLD.v||' '||NEW.v); END;
CREATE TRIGGER au AFTER UPDATE ON t BEGIN INSERT INTO log VALUES('au '||OLD.v||' '||NEW.v); END;
INSERT INTO t VALUES (1,'a',1);
INSERT INTO t(k, v) VALUES ('a', 5) ON CONFLICT(k) DO UPDATE SET v = v + excluded.v RETURNING id, v;
SELECT * FROM log ORDER BY rowid; SELECT * FROM t;
-- case: upsert/r2-upsert-statement-clauses
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INT CHECK (v < 100));
INSERT INTO t VALUES (1,'a',1),(2,'b',2);
INSERT OR REPLACE INTO t VALUES (3,'a',50) ON CONFLICT(k) DO UPDATE SET v = 200;
SELECT * FROM t ORDER BY id;
INSERT OR IGNORE INTO t VALUES (3,'a',50) ON CONFLICT(k) DO UPDATE SET k = 'b';
SELECT * FROM t ORDER BY id;
INSERT OR FAIL INTO t VALUES (3,'c',1),(4,'a',1) ON CONFLICT(k) DO UPDATE SET k = 'b';
SELECT * FROM t ORDER BY id;
-- case: upsert/r2-returning-ignore-upsert
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT UNIQUE, v INT);
INSERT INTO t VALUES (1,'a',1);
INSERT INTO t VALUES (2,'a',2) ON CONFLICT DO NOTHING RETURNING id;
INSERT INTO t VALUES (2,'b',2),(3,'a',3) ON CONFLICT DO NOTHING RETURNING id, k;
INSERT OR IGNORE INTO t VALUES (4,'a',3),(5,'e',5) RETURNING id;
-- case: autoincrement/r2-autoinc-trigger-and-replace
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT UNIQUE); CREATE TABLE log(m);
CREATE TRIGGER ta AFTER INSERT ON t BEGIN INSERT INTO log VALUES('i '||NEW.id); END;
INSERT INTO t(v) VALUES ('a'),('b');
REPLACE INTO t(v) VALUES ('a');
INSERT OR REPLACE INTO t VALUES (2, 'zz');
SELECT * FROM t ORDER BY id; SELECT * FROM sqlite_sequence; SELECT * FROM log ORDER BY rowid;
-- case: autoincrement/r2-autoinc-update-rowid
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES ('a'),('b');
UPDATE t SET id = 50 WHERE id = 1;
INSERT INTO t(v) VALUES ('c');
SELECT * FROM t ORDER BY id; SELECT * FROM sqlite_sequence;
UPDATE t SET id = 100 WHERE id = 3;
INSERT INTO t(v) VALUES ('d'); SELECT * FROM t ORDER BY id; SELECT * FROM sqlite_sequence;
-- case: constraints/r2-unique-index-expression-more
CREATE TABLE t(a TEXT, b INT);
CREATE UNIQUE INDEX i1 ON t(a, b+1);
INSERT INTO t VALUES ('x', 1);
INSERT INTO t VALUES ('x', 1);
INSERT INTO t VALUES ('x', 2) ON CONFLICT DO NOTHING;
INSERT INTO t VALUES ('x', 1) ON CONFLICT DO NOTHING;
INSERT INTO t VALUES ('x', 1) ON CONFLICT(a, b+1) DO UPDATE SET b = 5;
SELECT * FROM t ORDER BY b;
INSERT OR REPLACE INTO t VALUES ('x', 5);
SELECT * FROM t ORDER BY b;
UPDATE t SET b = 2 WHERE b = 5;
INSERT INTO t VALUES ('y', 0);
UPDATE t SET a = 'x', b = 2 WHERE a = 'y';
-- case: constraints/r2-unique-collate-index-xinfo
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT, c TEXT, UNIQUE(c DESC, a, b COLLATE RTRIM DESC));
SELECT * FROM pragma_index_xinfo('sqlite_autoindex_t_1');
SELECT * FROM pragma_index_info('sqlite_autoindex_t_1');
SELECT sql FROM sqlite_master;
-- case: constraints/r2-pk-collate-integer-alias
CREATE TABLE a(id INTEGER, v, PRIMARY KEY(id COLLATE NOCASE));
CREATE TABLE b(id INTEGER, v, PRIMARY KEY(id DESC));
CREATE TABLE c(id INTEGER PRIMARY KEY DESC, v);
INSERT INTO a VALUES (1,'x'); INSERT INTO b VALUES (1,'x'); INSERT INTO c VALUES (1,'x');
INSERT INTO a(v) VALUES ('y'); INSERT INTO b(v) VALUES ('y'); INSERT INTO c(v) VALUES ('y');
SELECT rowid, * FROM a; SELECT rowid, * FROM b; SELECT rowid, * FROM c;
SELECT name FROM sqlite_master WHERE type='index' ORDER BY name;
-- case: constraints/r2-unique-collate-multi-and-lookup
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b TEXT, UNIQUE(a COLLATE NOCASE, b COLLATE NOCASE));
INSERT INTO t VALUES (1,'Ab','Cd'),(2,'ab','CE');
INSERT INTO t VALUES (3,'AB','cD');
SELECT id FROM t WHERE a = 'ab' ORDER BY id; SELECT id FROM t WHERE a = 'ab' COLLATE NOCASE ORDER BY id;
EXPLAIN QUERY PLAN SELECT id FROM t WHERE a = 'ab' COLLATE NOCASE AND b = 'cd' COLLATE NOCASE;
-- case: constraints/r2-unique-collate-upsert-target
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, n INT DEFAULT 0, UNIQUE(a COLLATE NOCASE));
INSERT INTO t(a) VALUES ('Abc');
INSERT INTO t(a) VALUES ('aBC') ON CONFLICT(a COLLATE NOCASE) DO UPDATE SET n = n + 1;
SELECT * FROM t;
INSERT INTO t(a) VALUES ('ABC') ON CONFLICT(a) DO UPDATE SET n = n + 1;
-- case: foreign-keys/r2-fk-parent-unique-collate
PRAGMA foreign_keys=ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, code TEXT, UNIQUE(code COLLATE NOCASE));
CREATE TABLE c(code TEXT REFERENCES p(code));
INSERT INTO p VALUES (1,'AB');
INSERT INTO c VALUES ('ab');
SELECT * FROM c;
-- case: triggers/r2-replace-delete-trigger-recursive-after-returning
PRAGMA recursive_triggers=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT UNIQUE); CREATE TABLE log(m);
CREATE TRIGGER ad AFTER DELETE ON t BEGIN INSERT INTO log VALUES('d '||OLD.id||' '||OLD.v); END;
INSERT INTO t VALUES (1,'a'),(2,'b');
INSERT OR REPLACE INTO t VALUES (3,'a'),(4,'b') RETURNING id;
SELECT * FROM log ORDER BY rowid; SELECT * FROM t ORDER BY id; SELECT changes();
-- case: triggers/r2-recursive-delete-trigger-replace-nested
PRAGMA recursive_triggers=ON;
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT UNIQUE); CREATE TABLE u(id INTEGER PRIMARY KEY, w TEXT); CREATE TABLE log(m);
CREATE TRIGGER bd BEFORE DELETE ON t BEGIN INSERT INTO log VALUES('bd '||OLD.v); INSERT OR REPLACE INTO u VALUES (OLD.id, OLD.v); END;
INSERT INTO t VALUES (1,'a');
INSERT INTO u VALUES (1,'orig');
REPLACE INTO t VALUES (2,'a');
SELECT * FROM log; SELECT * FROM u; SELECT * FROM t;
-- case: triggers/r2-update-assign-pk-with-trigger
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT); CREATE TABLE log(m);
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN INSERT INTO log VALUES(OLD.id||'>'||NEW.id); END;
INSERT INTO t VALUES (1,'x'),(2,'y');
UPDATE t SET id = id + 10;
SELECT * FROM t ORDER BY id; SELECT * FROM log;
UPDATE t SET id = 12 WHERE id = 11;
-- case: triggers/r2-before-update-trigger-deletes-and-updates
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b INT);
INSERT INTO t VALUES (1,'p',1),(2,'q',2),(3,'r',3);
CREATE TRIGGER bu BEFORE UPDATE ON t WHEN OLD.id = 2 BEGIN DELETE FROM t WHERE id = 3; UPDATE t SET b = 99 WHERE id = 1; END;
UPDATE t SET b = b + 1;
SELECT * FROM t ORDER BY id; PRAGMA integrity_check;
