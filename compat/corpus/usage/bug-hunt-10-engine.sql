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
