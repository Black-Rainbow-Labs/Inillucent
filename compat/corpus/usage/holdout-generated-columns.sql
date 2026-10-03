-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/generated-columns/206-self-join-reading-a-virtual-generated-column-throu
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
INSERT INTO t1(a) VALUES(1),(2),(3);
SELECT a1.b, a2.b FROM t1 a1 JOIN t1 a2 ON a1.a = a2.a;
SELECT * FROM t1 x, t1 y WHERE x.a = y.a;
SELECT x.a, y.a FROM t1 x JOIN t1 y ON x.b = y.b;
SELECT * FROM t1 WHERE EXISTS (SELECT 1 FROM t1 t2 WHERE t2.b = t1.b);
-- case: holdout/generated-columns/207-delete-returning-with-virtual-generated-columns
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
INSERT INTO t1(a) VALUES(1),(2),(3);
DELETE FROM t1 WHERE a = 2 RETURNING b;
DELETE FROM t1 WHERE a > 1 RETURNING *;
DELETE FROM t1 RETURNING a;
-- case: holdout/generated-columns/208-delete-trigger-on-a-table-that-has-a-virtual-gener
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr1 AFTER DELETE ON t1 BEGIN INSERT INTO log VALUES('deleted ' || OLD.a || ',' || OLD.b); END;
INSERT INTO t1(a) VALUES(1),(2);
DELETE FROM t1 WHERE a = 1;
SELECT * FROM log;
-- case: holdout/generated-columns/209-three-table-inner-join-with-the-generated-column-t
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE TABLE t2(x INTEGER);
CREATE TABLE t3(y INTEGER);
INSERT INTO t1(a) VALUES(1),(2),(3);
INSERT INTO t2 VALUES(1),(2),(3);
INSERT INTO t3 VALUES(1),(2),(3);
SELECT t1.a FROM t1 JOIN t2 ON t1.a = t2.x JOIN t3 ON t1.a = t3.y;
SELECT t1.a FROM t1 LEFT JOIN t2 ON t1.a = t2.x JOIN t3 ON t1.a = t3.y;
SELECT t1.a, t1.b FROM t2 JOIN t1 ON t2.x = t1.a JOIN t3 ON t1.a = t3.y;
-- case: holdout/generated-columns/210-delete-and-update-through-an-index-on-a-virtual-ge
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
INSERT INTO t1(a) VALUES(1),(2),(3);
CREATE INDEX idx_b ON t1(b);
DELETE FROM t1 WHERE b = 4;
SELECT * FROM t1;
UPDATE t1 SET a = 10 WHERE b = 6;
SELECT * FROM t1 ORDER BY a;
PRAGMA integrity_check;
CREATE TABLE t2(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL, c TEXT);
CREATE INDEX idx2 ON t2(b, c);
INSERT INTO t2(a) VALUES(1),(2),(3);
DELETE FROM t2 WHERE b = 4;
SELECT * FROM t2;
-- case: holdout/generated-columns/211-collate-on-a-virtual-generated-column-applies-in-w
CREATE TABLE t1(a TEXT, b TEXT GENERATED ALWAYS AS (a) VIRTUAL COLLATE NOCASE);
INSERT INTO t1(a) VALUES('Hello'),('HELLO'),('hello');
SELECT * FROM t1 WHERE b = 'hello';
SELECT count(*) FROM t1 WHERE b IN ('hello');
SELECT count(*) FROM t1 WHERE b = 'hello' AND a <> 'x';
-- case: holdout/generated-columns/212-replace-where-a-virtual-generated-column-is-unique
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL UNIQUE);
INSERT INTO t1(a) VALUES(1),(2),(3);
INSERT OR REPLACE INTO t1(a) VALUES(2);
REPLACE INTO t1(a) VALUES(2);
INSERT OR IGNORE INTO t1(a) VALUES(2);
SELECT * FROM t1 ORDER BY a;
CREATE TABLE t3(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE UNIQUE INDEX idx ON t3(b);
INSERT INTO t3(a) VALUES(1),(2),(3);
UPDATE OR REPLACE t3 SET a = 2 WHERE a = 3;
SELECT * FROM t3 ORDER BY a;
-- case: holdout/generated-columns/213-sqlite-master-keeps-the-create-table-text-with-gen
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE TABLE t2(a INTEGER, b AS (a+1) STORED, c AS (a+2));
SELECT sql FROM sqlite_master ORDER BY name;
PRAGMA table_xinfo(t2);
PRAGMA table_info(t2);
-- case: holdout/generated-columns/214-foreign-key-whose-parent-key-is-a-unique-virtual-g
PRAGMA foreign_keys=ON;
CREATE TABLE p(a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL UNIQUE);
CREATE TABLE c1(x INTEGER REFERENCES p(b));
INSERT INTO p(a) VALUES(1);
INSERT INTO c1(x) VALUES(2);
UPDATE p SET a=3 WHERE a=1;
CREATE TABLE c2(x INTEGER REFERENCES p(b) ON DELETE CASCADE);
INSERT INTO p(a) VALUES(10);
INSERT INTO c2(x) VALUES(20);
DELETE FROM p WHERE a=10;
SELECT * FROM c2;
-- case: holdout/generated-columns/215-foreign-key-on-a-virtual-generated-child-column-is
PRAGMA foreign_keys=ON;
CREATE TABLE p(x INTEGER PRIMARY KEY);
CREATE TABLE c(a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL REFERENCES p(x));
INSERT INTO p VALUES(2),(4);
INSERT INTO c(a) VALUES(1);
UPDATE c SET a=3;
SELECT * FROM c;
-- case: holdout/generated-columns/216-update-on-a-table-with-a-unique-virtual-generated-
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL UNIQUE);
INSERT INTO t1(a) VALUES(1),(2),(3);
UPDATE t1 SET a = 10 WHERE a = 1;
SELECT * FROM t1 ORDER BY a;
UPDATE t1 SET a = 2 WHERE a = 3;
UPDATE OR IGNORE t1 SET a = 2 WHERE a = 3;
SELECT * FROM t1 ORDER BY a;
CREATE TABLE t3(a INT, b INT, c GENERATED ALWAYS AS (a+b) VIRTUAL, UNIQUE(a,c));
INSERT INTO t3(a,b) VALUES(1,10),(2,20);
UPDATE t3 SET b = 20 WHERE a = 1;
SELECT * FROM t3;
-- case: holdout/generated-columns/217-not-null-on-a-virtual-generated-column-is-enforced
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL NOT NULL);
INSERT INTO t1(a) VALUES(1),(2),(3);
UPDATE t1 SET a = NULL WHERE a = 2;
SELECT * FROM t1;
INSERT INTO t1(a) VALUES(NULL);
-- case: holdout/generated-columns/218-virtual-generated-column-selected-in-a-join-with-a
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL);
INSERT INTO t1(a) VALUES(1),(2),(3);
SELECT t1.a, t1.b FROM t1, json_each('[1,2,3]') je WHERE t1.a = je.value;
SELECT t1.a, t1.b FROM t1, generate_series(1, 3) gs WHERE t1.a = gs.value;
SELECT t1.b FROM generate_series(1,3) gs LEFT JOIN t1 ON gs.value = t1.a;
SELECT t1.b FROM generate_series(1,3) gs, t1 WHERE gs.value = t1.a;
-- case: holdout/generated-columns/219-strict-table-checks-the-declared-type-of-a-generat
CREATE TABLE t1(a TEXT, b INTEGER GENERATED ALWAYS AS (a) VIRTUAL) STRICT;
INSERT INTO t1(a) VALUES('abc');
CREATE TABLE t2(a TEXT, b INTEGER GENERATED ALWAYS AS (a) VIRTUAL) STRICT;
INSERT INTO t2(a) VALUES('42');
SELECT a, b, typeof(b) FROM t2;
UPDATE t2 SET a='abc';
CREATE TABLE t3(a BLOB, b TEXT GENERATED ALWAYS AS (a) VIRTUAL) STRICT;
INSERT INTO t3(a) VALUES(x'6162');
SELECT * FROM t3;
-- case: holdout/generated-columns/220-upsert-do-update-that-makes-a-unique-generated-col
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE UNIQUE INDEX idx_b ON t(b);
INSERT INTO t(id,a) VALUES(1,1),(2,2);
INSERT INTO t(id,a) VALUES(1,2) ON CONFLICT(id) DO UPDATE SET a=excluded.a;
SELECT id,a,b FROM t ORDER BY id;
INSERT INTO t(id,a) VALUES(1,1) ON CONFLICT(id) DO UPDATE SET a=excluded.a;
SELECT id,a,b FROM t ORDER BY id;
-- case: holdout/generated-columns/221-parent-key-that-is-a-generated-column-backed-by-a-
PRAGMA foreign_keys=ON;
CREATE TABLE p(a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE UNIQUE INDEX idx_p_b ON p(b);
CREATE TABLE c1(x INTEGER REFERENCES p(b));
INSERT INTO p(a) VALUES(1);
INSERT INTO c1(x) VALUES(2);
INSERT INTO c1(x) VALUES(3);
UPDATE p SET a=3 WHERE a=1;
CREATE TABLE c2(x INTEGER REFERENCES p(b) ON DELETE CASCADE);
INSERT INTO p(a) VALUES(10);
INSERT INTO c2(x) VALUES(20);
DELETE FROM p WHERE a=10;
SELECT * FROM c2;
-- case: holdout/generated-columns/222-generated-column-referencing-an-integer-primary-ke
CREATE TABLE t1(id INTEGER PRIMARY KEY, a TEXT, b AS (id * 10) CHECK(b IS NOT NULL));
INSERT INTO t1(a) VALUES('hello');
SELECT id, a, b FROM t1;
CREATE TABLE t2(id INTEGER PRIMARY KEY, a TEXT, b AS (id * 2));
CREATE INDEX idx_b ON t2(b);
INSERT INTO t2(a) VALUES('one'), ('two'), ('three');
SELECT id, a, b FROM t2 WHERE b = 4;
-- case: holdout/generated-columns/240-update-of-the-rowid-when-an-index-covers-a-virtual
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, v AS (a * 10) VIRTUAL);
CREATE INDEX idx_v ON t(v);
INSERT INTO t(id, a) VALUES (1, 5);
UPDATE t SET id = 100 WHERE id = 1;
SELECT id, a, v FROM t;
PRAGMA integrity_check;
-- case: holdout/generated-columns/241-update-of-a-parent-row-with-a-mid-table-virtual-ge
PRAGMA foreign_keys = ON;
CREATE TABLE parent(a INTEGER, v INTEGER GENERATED ALWAYS AS (a * 10) VIRTUAL, b INTEGER, c INTEGER, PRIMARY KEY (a, b));
CREATE TABLE child(x INTEGER, y INTEGER, FOREIGN KEY (x, y) REFERENCES parent(a, b));
INSERT INTO parent(a, b, c) VALUES (1, 100, 999);
INSERT INTO child VALUES (1, 100);
UPDATE parent SET c = 888 WHERE a = 1 AND b = 100;
SELECT a, v, b, c FROM parent;
UPDATE parent SET b = 101 WHERE a = 1;
-- case: holdout/generated-columns/247-deferred-fk-on-a-generated-column-re-checked-when-
PRAGMA foreign_keys=on;
CREATE TABLE machines(code INTEGER PRIMARY KEY);
CREATE TABLE crew(real_id INTEGER, residual_code INTEGER GENERATED ALWAYS AS (real_id*100) VIRTUAL, FOREIGN KEY(residual_code) REFERENCES machines(code) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO machines VALUES(100),(200);
BEGIN;
INSERT INTO crew(real_id) VALUES(1);
UPDATE crew SET real_id=3;
COMMIT;
SELECT * FROM crew;
-- case: holdout/generated-columns/254-unique-generated-column-whose-expression-uses-anot
CREATE TABLE t (a UNIQUE AS (b), b AS ('x'), c);
INSERT INTO t(c) VALUES (1);
INSERT INTO t(c) VALUES (2);
SELECT count(*) FROM t;
-- case: holdout/generated-columns/255-update-on-a-table-with-a-unique-generated-column-a
CREATE TABLE t (a INTEGER, b INTEGER, c TEXT UNIQUE, f INTEGER UNIQUE AS (b));
INSERT INTO t VALUES (1, 1, 'x');
UPDATE t SET b = 42 WHERE c = 'x';
SELECT * FROM t;
-- case: holdout/generated-columns/256-update-through-an-expression-index-over-a-virtual-
CREATE TABLE t(a TEXT, g INT AS (a = 10) VIRTUAL);
CREATE INDEX idx_expr ON t(g+0);
INSERT INTO t(a) VALUES('10'), ('5');
UPDATE t SET a='11' WHERE g+0=1 RETURNING a, g, g+0;
SELECT 'all', a, g FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: holdout/generated-columns/257-delete-with-an-index-that-covers-a-virtual-generat
CREATE TABLE t (c0 TEXT, c1 INTEGER AS (c2 + 1), c2 INTEGER);
CREATE INDEX i ON t (c1, c0);
DELETE FROM t WHERE c0 < 'X';
INSERT INTO t(c0, c2) VALUES ('A', 1), ('Z', 2);
DELETE FROM t WHERE c0 < 'X';
SELECT * FROM t;
PRAGMA integrity_check;
-- case: holdout/generated-columns/258-update-keeps-an-index-over-a-chained-virtual-gener
CREATE TABLE t (a INTEGER, c TEXT, e AS (a), d AS (e));
INSERT INTO t (a, c) VALUES (1, 'aaa');
CREATE INDEX idx_t ON t (d, c);
UPDATE t SET c = 'bbb';
PRAGMA integrity_check;
SELECT d, c FROM t WHERE d = 1 AND c = 'bbb';
-- case: holdout/generated-columns/262-on-update-cascade-with-a-leading-virtual-generated
PRAGMA foreign_keys=ON;
CREATE TABLE parent(v INTEGER GENERATED ALWAYS AS (key * 10) VIRTUAL, key INTEGER UNIQUE, x INTEGER);
CREATE TABLE child(id INTEGER PRIMARY KEY, pkey INTEGER REFERENCES parent(key) ON UPDATE CASCADE);
INSERT INTO parent(key, x) VALUES (1, 100), (2, 200);
INSERT INTO child VALUES (10, 1), (20, 2);
UPDATE parent SET key = 99 WHERE key = 1;
SELECT id, pkey FROM child ORDER BY id;
SELECT v, key, x FROM parent ORDER BY key;
-- case: holdout/generated-columns/285-alter-table-add-column-with-both-default-and-as
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN b DEFAULT 5 AS (a+1);
CREATE TABLE t2(a, b DEFAULT 5 AS (a+1));
ALTER TABLE t ADD COLUMN c AS (a+1) STORED;
ALTER TABLE t ADD COLUMN d AS (a+1) VIRTUAL;
SELECT sql FROM sqlite_schema WHERE name='t';
-- case: holdout/generated-columns/286-unknown-type-tokens-after-a-generated-column-expre
CREATE TABLE t(a,b AS (a) WAT);
CREATE TABLE t2(a, b INT AS (a) NOT NULL);
CREATE TABLE t3(a, b INT AS (a) VIRTUAL CHECK(b>0));
INSERT INTO t3(a) VALUES (0);
-- case: holdout/generated-columns/287-two-generated-clauses-on-one-column
CREATE TABLE t(a, b, c AS (a+1) AS (a+2));
CREATE TABLE t2(a, b, c GENERATED ALWAYS AS (a+1) GENERATED ALWAYS AS (a+2));
-- case: holdout/generated-columns/288-deterministic-date-functions-in-a-generated-column
CREATE TABLE t(a, b AS (date('2020-01-02')));
INSERT INTO t(a) VALUES(1);
SELECT b FROM t;
CREATE TABLE t2(a, b AS (date('now')));
CREATE TABLE t3(a, b AS (random()));
CREATE TABLE t4(a, b AS (date(a, 'localtime')));
CREATE TABLE t5(a, b AS (unixepoch('now', 'subsec')));
-- case: holdout/generated-columns/289-raise-and-other-special-expressions-in-a-generated
CREATE TABLE t(a, b AS (RAISE(IGNORE)));
CREATE TABLE t2(a, b AS ((SELECT 1)));
CREATE TABLE t3(a, b AS (?1));
CREATE TABLE t4(a, b AS (a IN (SELECT 1)));
CREATE TABLE t5(a, b AS (count(*)));
CREATE TABLE t6(a, b AS (sum(a) OVER ()));
-- case: holdout/generated-columns/290-outer-join-null-extension-of-a-virtual-generated-c
CREATE TABLE l(x);
CREATE TABLE r(a, g AS (1));
INSERT INTO l VALUES(1);
SELECT x,a,g,typeof(g) FROM l LEFT JOIN r ON false;
-- case: holdout/generated-columns/291-seek-on-an-indexed-generated-column-uses-the-colum
CREATE TABLE t(a TEXT, g INT AS(a));
CREATE INDEX i ON t(g);
INSERT INTO t(a) VALUES('1'),('2');
SELECT a,g FROM t WHERE g=1;
DELETE FROM t WHERE g=1;
SELECT a,g FROM t;
PRAGMA integrity_check;
-- case: holdout/generated-columns/292-row-value-comparison-inside-a-generated-column-exp
CREATE TABLE t(a, b AS ((a,1)<(a,2)));
INSERT INTO t(a) VALUES(5);
SELECT * FROM t;
-- case: holdout/generated-columns/293-replace-with-not-null-default-recomputes-generated
CREATE TABLE t(a NOT NULL DEFAULT 5, b AS(a+1) UNIQUE);
REPLACE INTO t(a) VALUES(NULL) RETURNING a,b;
SELECT rowid,a,b FROM t;
PRAGMA integrity_check;
-- case: holdout/generated-columns/294-unique-index-on-a-generated-int-column-compares-va
CREATE TABLE t(a TEXT, g INT AS(a));
CREATE UNIQUE INDEX i ON t(g);
INSERT INTO t(a) VALUES('1');
INSERT INTO t(a) VALUES('01');
SELECT a, g, typeof(g) FROM t;
-- case: holdout/generated-columns/295-fk-to-a-generated-column-that-has-a-unique-index
PRAGMA foreign_keys=ON;
CREATE TABLE p(a TEXT, g INT AS(a));
CREATE UNIQUE INDEX pg ON p(g);
INSERT INTO p(a) VALUES('1');
CREATE TABLE c(x REFERENCES p(g));
INSERT INTO c VALUES(1);
SELECT * FROM c;
-- case: holdout/generated-columns/296-integrity-check-on-a-strict-table-reports-a-genera
CREATE TABLE t(a ANY, b INT AS(a) NOT NULL) STRICT;
INSERT INTO t(a) VALUES('abc');
PRAGMA integrity_check;
SELECT * FROM t;
-- case: holdout/generated-columns/297-on-conflict-target-that-names-a-unique-index-on-a-
CREATE TABLE t(a, b AS(a*2));
CREATE UNIQUE INDEX idx_b ON t(b);
INSERT INTO t(a) VALUES(1);
INSERT INTO t(a) VALUES(1) ON CONFLICT(b) DO UPDATE SET a=2;
SELECT rowid,a,b FROM t;
-- case: holdout/generated-columns/299-parent-row-deletion-is-blocked-by-a-child-fk-on-a-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id PRIMARY KEY);
CREATE TABLE c(a, g AS(a) REFERENCES p(id));
INSERT INTO p VALUES(1);
INSERT INTO c(a) VALUES(1);
DELETE FROM p;
UPDATE p SET id=2;
SELECT * FROM c;
SELECT * FROM p;
-- case: holdout/generated-columns/300-deferred-fk-on-a-generated-child-key-is-satisfied-
PRAGMA foreign_keys=ON;
CREATE TABLE p(id PRIMARY KEY);
CREATE TABLE c(a, g AS(a) REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
BEGIN;
INSERT INTO c(a) VALUES(1);
INSERT INTO p VALUES(1);
COMMIT;
SELECT * FROM c;
-- case: holdout/generated-columns/324-add-column-of-a-virtual-generated-column-and-creat
CREATE TABLE t(a);
INSERT INTO t VALUES (1);
ALTER TABLE t ADD COLUMN b AS (a);
SELECT * FROM t;
CREATE TABLE g(a, b AS (a));
ALTER TABLE t ADD COLUMN c AS (a) STORED;
-- case: holdout/generated-columns/361-explain-comments-for-a-table-with-a-virtual-genera
CREATE TABLE t(a INT, b INT, v INT GENERATED ALWAYS AS (a*2+b) VIRTUAL, c TEXT);
INSERT INTO t(a,b,c) VALUES (1,2,'x');
SELECT a, b, v, c FROM t;
PRAGMA table_xinfo(t);
-- case: holdout/generated-columns/362-self-join-on-virtual-generated-columns-automatic-i
CREATE TABLE g(a INTEGER, v1 AS (a + 1) VIRTUAL, s1 AS (a * 2) VIRTUAL);
INSERT INTO g(a) VALUES (1), (2);
SELECT g1.a, g2.a FROM g g1 JOIN g g2 ON g1.v1 = g2.s1;
SELECT g1.a, g2.a FROM g g1 JOIN g g2 ON g1.v1 = g2.v1;
-- case: holdout/generated-columns/521-upsert-do-update-checks-not-null-on-a-virtual-gene
CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, c AS (nullif(a, 0)) NOT NULL);
INSERT INTO t VALUES (1, 1);
INSERT INTO t(id, a) VALUES (1, 5) ON CONFLICT(id) DO UPDATE SET a = 0;
SELECT id, a, c FROM t;
-- case: holdout/generated-columns/522-upsert-do-update-checks-a-child-foreign-key-that-i
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
INSERT INTO p VALUES (1);
CREATE TABLE ch(id INTEGER PRIMARY KEY, x INTEGER, v AS (x + 1) REFERENCES p(id));
INSERT INTO ch(id, x) VALUES (1, 0);
INSERT INTO ch(id, x) VALUES (1, 0) ON CONFLICT(id) DO UPDATE SET x = 5;
SELECT id, x, v FROM ch;
-- case: holdout/generated-columns/523-after-update-trigger-computes-new-virtual-columns-
CREATE TABLE t(id INTEGER PRIMARY KEY, x INT, c AS (id * 10));
CREATE TABLE log(old_id, old_c, new_id, new_c);
CREATE TRIGGER t_au AFTER UPDATE ON t BEGIN INSERT INTO log VALUES (OLD.id, OLD.c, NEW.id, NEW.c); END;
INSERT INTO t(id, x) VALUES (1, 0);
UPDATE t SET id = 5 WHERE id = 1;
SELECT * FROM log;
-- case: holdout/generated-columns/524-update-checks-not-null-on-a-generated-column-after
CREATE TABLE t(a INTEGER, c AS (nullif(typeof(a), 'text')) NOT NULL);
INSERT INTO t(a) VALUES (1);
UPDATE t SET a = '5';
SELECT a, c FROM t;
CREATE TABLE u(a INTEGER, c AS (nullif(a, 0)) NOT NULL);
CREATE TABLE log(x);
CREATE TRIGGER t_bu BEFORE UPDATE ON u BEGIN INSERT INTO log VALUES (NEW.a); END;
INSERT INTO u(a) VALUES (1);
UPDATE u SET a = 0;
SELECT a, c FROM u;
-- case: holdout/generated-columns/525-upsert-do-update-computes-virtual-columns-after-af
CREATE TABLE t(id INTEGER PRIMARY KEY, b INTEGER, c AS (typeof(b)) CHECK (c = 'integer'));
INSERT INTO t(id, b) VALUES (1, 5);
INSERT INTO t(id, b) VALUES (1, 0) ON CONFLICT(id) DO UPDATE SET b = '6';
SELECT id, b, c FROM t;
-- case: holdout/generated-columns/526-parent-key-built-from-two-virtual-columns-in-forei
PRAGMA foreign_keys = ON;
CREATE TABLE p(a INTEGER, b INTEGER, c AS (a + 1), e AS (b + 100), k AS (c + e) UNIQUE);
CREATE TABLE ch(x REFERENCES p(k));
INSERT INTO p(a, b) VALUES (1, 2);
INSERT INTO ch VALUES (104);
DELETE FROM p;
SELECT count(*) FROM p;
SELECT * FROM ch;
-- case: holdout/generated-columns/678-not-null-on-a-virtual-generated-column-is-enforced
CREATE TABLE t(a INT, b INT, c INT GENERATED ALWAYS AS (a+b) VIRTUAL NOT NULL);
INSERT INTO t(a,b) VALUES(1,2);
UPDATE t SET a = NULL;
SELECT a,b,c FROM t;
-- case: holdout/generated-columns/759-drop-column-that-would-leave-only-a-generated-colu
CREATE TABLE t(a, b AS (a + 1));
ALTER TABLE t DROP COLUMN a;
CREATE TABLE u(a, c, b AS (a + 1));
ALTER TABLE u DROP COLUMN c;
ALTER TABLE u DROP COLUMN a;
SELECT sql FROM sqlite_master ORDER BY name;
-- case: holdout/generated-columns/768-update-and-insert-of-generated-columns-are-refused
CREATE TABLE t(a, v AS (a+1) VIRTUAL, s AS (a+2) STORED);
INSERT INTO t(a) VALUES (1);
UPDATE t SET v = 5;
UPDATE t SET s = 5;
INSERT INTO t(a, v) VALUES (2, 3);
INSERT INTO t VALUES (3, 4, 5);
INSERT INTO t(a) VALUES (4);
SELECT * FROM t ORDER BY a;
-- case: holdout/generated-columns/769-a-generated-column-cannot-be-a-primary-key-or-have
CREATE TABLE t(a, v AS (a+1) PRIMARY KEY);
CREATE TABLE u(a, v AS (a+1), PRIMARY KEY(v));
CREATE TABLE w(a, v AS (a+1) UNIQUE);
CREATE TABLE x(a, v AS (a+1) REFERENCES w(a));
CREATE TABLE y(a, v AS (v+1));
CREATE TABLE z(a, v AS (w), w AS (v));
-- case: holdout/generated-columns/786-generated-columns-as-part-of-a-primary-key-on-a-ro
CREATE TABLE t(a, v AS (a+1), PRIMARY KEY(a));
INSERT INTO t(a) VALUES (1);
SELECT a, v FROM t;
CREATE TABLE u(a, v AS (a+1), PRIMARY KEY(a, v));
CREATE TABLE w(a, v AS (a+1), UNIQUE(a, v));
INSERT INTO w(a) VALUES (1);
SELECT * FROM w;
-- case: holdout/generated-columns/806-pragma-table-info-omits-generated-columns-table-xi
CREATE TABLE t(a, v AS (a+1) VIRTUAL, s AS (a+2) STORED, b);
PRAGMA table_info(t);
PRAGMA table_xinfo(t);
SELECT name, hidden FROM pragma_table_xinfo('t');
SELECT name FROM pragma_table_info('t');
