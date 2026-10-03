-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/alter-table-and-schema-c/29-rename-to-a-name-starting-with-sqlite
create table t(a);
alter table t rename to sqlite_1234;
-- case: holdout/alter-table-and-schema-c/53-add-column-with-a-default-of-the-wrong-type-in-a-s
CREATE TABLE t1(name TEXT) STRICT;
INSERT INTO t1 VALUES ('alice'), ('bob');
ALTER TABLE t1 ADD COLUMN id INTEGER DEFAULT 'corrupted';
SELECT typeof(id) FROM t1;
-- case: holdout/alter-table-and-schema-c/75-add-column-with-an-unknown-collation
CREATE TABLE t(x);
ALTER TABLE t ADD COLUMN y COLLATE bogus;
CREATE TABLE u(x COLLATE bogus);
SELECT 'a' < 'b' COLLATE bogus;
-- case: holdout/alter-table-and-schema-c/84-rename-column-matches-the-column-name-case-insensi
CREATE TABLE t1(a INT);
ALTER TABLE t1 RENAME COLUMN A TO b;
SELECT sql FROM sqlite_schema;
-- case: holdout/alter-table-and-schema-c/106-rename-column-rewrites-the-where-clause-of-a-parti
CREATE TABLE t ( a, b );
INSERT INTO t VALUES (1, 'x'), (2, 'y');
CREATE INDEX idx ON t ( b ) WHERE a = 1;
ALTER TABLE t RENAME COLUMN a TO a2;
UPDATE t SET a2 = 2;
PRAGMA integrity_check;
SELECT sql FROM sqlite_schema WHERE name = 'idx';
DELETE FROM t;
-- case: holdout/alter-table-and-schema-c/108-rename-column-keeps-an-expression-index-usable
CREATE TABLE t(a,b,c);
CREATE INDEX i_expr ON t(substr(b,1,1));
INSERT INTO t VALUES(1,'bee',2);
ALTER TABLE t RENAME COLUMN b TO bb;
PRAGMA integrity_check;
SELECT sql FROM sqlite_schema WHERE name='i_expr';
SELECT * FROM t WHERE substr(bb,1,1)='b';
-- case: holdout/alter-table-and-schema-c/134-alter-table-on-a-table-whose-name-contains-quote-l
CREATE TABLE "t' OR 1=1 -- " (a);
CREATE TABLE ok (x);
ALTER TABLE "t' OR 1=1 -- " ADD COLUMN b;
SELECT name, sql FROM sqlite_schema ORDER BY name;
-- case: holdout/alter-table-and-schema-c/135-add-column-with-a-check-on-existing-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(-1);
ALTER TABLE t ADD COLUMN b INT CHECK (a>0);
SELECT * FROM t;
ALTER TABLE t ADD COLUMN c INT CHECK (c>0);
SELECT * FROM t;
-- case: holdout/alter-table-and-schema-c/136-alter-table-drop-column-add-column-rewrite-sqlite-
CREATE TABLE "a b"(x,y);
ALTER TABLE "a b" DROP COLUMN y;
CREATE TABLE "select"(a);
ALTER TABLE "select" ADD COLUMN b;
CREATE TABLE t("select" INT);
ALTER TABLE t ADD COLUMN b;
SELECT name, sql FROM sqlite_schema;
PRAGMA integrity_check;
-- case: holdout/alter-table-and-schema-c/196-do-update-reads-the-column-default-for-rows-that-p
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO t VALUES(1, 'alice');
ALTER TABLE t ADD COLUMN score INTEGER DEFAULT 42;
INSERT INTO t VALUES(1, 'bob', 0) ON CONFLICT(id) DO UPDATE SET score = score + 1;
SELECT * FROM t;
-- case: holdout/alter-table-and-schema-c/259-alter-column-type-change-keeps-secondary-indexes-i
CREATE TABLE core(id NUMERIC, row_rank NUMERIC);
CREATE INDEX core_id ON core(id);
CREATE INDEX core_rank ON core(row_rank);
INSERT INTO core VALUES('001','101'),('002','102');
SELECT id, typeof(id), row_rank, typeof(row_rank) FROM core;
CREATE TABLE core2(id TEXT, row_rank TEXT);
INSERT INTO core2 VALUES('001','101');
SELECT id, typeof(id) FROM core2;
-- case: holdout/alter-table-and-schema-c/268-alter-column-of-a-primary-key-column
create table t (a text primary key, b text);
alter table t alter column a to integer;
select name, sql from sqlite_schema;
-- case: holdout/alter-table-and-schema-c/269-alter-column-on-an-autoincrement-column
CREATE TABLE t(a INTEGER PRIMARY KEY AUTOINCREMENT, b TEXT);
INSERT INTO t(b) VALUES('x'),('y');
ALTER TABLE t ALTER COLUMN a TO a2 TEXT;
SELECT * FROM sqlite_sequence;
SELECT sql FROM sqlite_schema WHERE name='t';
-- case: holdout/alter-table-and-schema-c/326-add-column-with-an-explicit-null-constraint
CREATE TABLE t (a INTEGER);
ALTER TABLE t ADD COLUMN b BLOB NULL;
PRAGMA table_info(t);
INSERT INTO t (a) VALUES (1);
SELECT * FROM t;
ALTER TABLE t ADD COLUMN c INT NOT NULL;
ALTER TABLE t ADD COLUMN d INT NOT NULL DEFAULT 5;
SELECT * FROM t;
-- case: holdout/alter-table-and-schema-c/330-drop-column-of-a-column-referenced-by-a-check-cons
CREATE TABLE t(a, b CHECK(b > a));
ALTER TABLE t DROP COLUMN a;
SELECT sql FROM sqlite_master WHERE name='t';
INSERT INTO t(b) VALUES(5);
SELECT * FROM t;
-- case: holdout/alter-table-and-schema-c/364-add-column-keeps-collate-of-existing-columns-in-th
CREATE TABLE tbl(col1 TEXT COLLATE NOCASE);
INSERT INTO tbl VALUES ('ABC');
ALTER TABLE tbl ADD COLUMN extra TEXT;
SELECT sql FROM sqlite_master WHERE name = 'tbl';
SELECT count(*) FROM tbl WHERE col1 = 'abc';
-- case: holdout/alter-table-and-schema-c/407-add-column-keeps-desc-and-collate-inside-table-lev
CREATE TABLE t(a TEXT, UNIQUE(a DESC));
CREATE TABLE u(a TEXT, UNIQUE(a COLLATE NOCASE));
CREATE TABLE v(a TEXT COLLATE NOCASE, UNIQUE(a));
ALTER TABLE t ADD COLUMN q TEXT;
SELECT sql FROM sqlite_master WHERE name='t';
ALTER TABLE u ADD COLUMN q TEXT;
SELECT sql FROM sqlite_master WHERE name='u';
INSERT INTO u(a) VALUES ('a');
INSERT INTO u(a) VALUES ('A');
-- case: holdout/alter-table-and-schema-c/408-add-column-and-drop-column-keep-on-conflict-clause
CREATE TABLE t(id INTEGER, k TEXT UNIQUE ON CONFLICT ROLLBACK, x INTEGER, j TEXT);
CREATE TABLE u(id INTEGER, k TEXT UNIQUE ON CONFLICT ROLLBACK, x INTEGER, j TEXT);
ALTER TABLE t ADD COLUMN note TEXT;
SELECT sql LIKE '%ON CONFLICT ROLLBACK%' FROM sqlite_master WHERE name='t';
ALTER TABLE u DROP COLUMN j;
SELECT sql LIKE '%ON CONFLICT ROLLBACK%' FROM sqlite_master WHERE name='u';
ALTER TABLE u RENAME COLUMN x TO x2;
SELECT sql FROM sqlite_master WHERE name='u';
-- case: holdout/alter-table-and-schema-c/436-add-column-integer-default-4-0-stores-an-integer
CREATE TABLE t(x INTEGER);
INSERT INTO t VALUES(1);
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT 4.0;
SELECT b, typeof(b) FROM t;
INSERT INTO t(x) VALUES(2);
SELECT b, typeof(b) FROM t;
CREATE TABLE u(x INTEGER);
INSERT INTO u VALUES(1);
ALTER TABLE u ADD COLUMN b REAL DEFAULT 4;
SELECT b, typeof(b) FROM u;
ALTER TABLE u ADD COLUMN c TEXT DEFAULT 4.5;
ALTER TABLE u ADD COLUMN d NUMERIC DEFAULT '7';
SELECT c, typeof(c), d, typeof(d) FROM u;
-- case: holdout/alter-table-and-schema-c/462-add-column-with-a-default-whose-type-differs-from-
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES(1);
ALTER TABLE t ADD COLUMN b BLOB DEFAULT '';
SELECT quote(b) FROM t;
ALTER TABLE t ADD COLUMN c INTEGER DEFAULT 3.7;
SELECT c, typeof(c) FROM t;
ALTER TABLE t ADD COLUMN d TEXT DEFAULT 5;
SELECT d, typeof(d) FROM t;
CREATE TABLE v(a INTEGER) STRICT;
INSERT INTO v VALUES(1);
ALTER TABLE v ADD COLUMN b TEXT DEFAULT 5;
ALTER TABLE v ADD COLUMN c INTEGER DEFAULT 'x';
-- case: holdout/alter-table-and-schema-c/463-rename-column-with-collate-in-a-table-level-unique
CREATE TABLE t(a TEXT, b TEXT, UNIQUE(a COLLATE NOCASE));
INSERT INTO t VALUES('a','x');
ALTER TABLE t RENAME COLUMN b TO c;
SELECT c FROM t;
ALTER TABLE t RENAME COLUMN a TO d;
SELECT sql FROM sqlite_schema WHERE name='t';
INSERT INTO t VALUES('A','y');
CREATE TABLE u(a TEXT, b TEXT, PRIMARY KEY(a COLLATE NOCASE));
ALTER TABLE u RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name='u';
-- case: holdout/alter-table-and-schema-c/491-add-column-with-a-non-constant-default-on-an-empty
CREATE TABLE t(a INTEGER);
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT (a + 1);
ALTER TABLE t ADD COLUMN c INTEGER DEFAULT (random());
ALTER TABLE t ADD COLUMN d INTEGER DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE t ADD COLUMN e INTEGER DEFAULT (1+1);
SELECT sql FROM sqlite_master;
-- case: holdout/alter-table-and-schema-c/496-drop-column-on-a-without-rowid-table
CREATE TABLE wr (key INTEGER PRIMARY KEY, removed TEXT, kept INTEGER) WITHOUT ROWID;
INSERT INTO wr VALUES (1, 'discard', 101), (2, 'gone', 202);
ALTER TABLE wr DROP COLUMN removed;
SELECT * FROM wr;
SELECT kept FROM wr WHERE kept = 101;
PRAGMA integrity_check;
-- case: holdout/alter-table-and-schema-c/517-add-column-keeps-the-size-of-typed-columns-in-the-
CREATE TABLE u(a VARCHAR(10), b DECIMAL(10,2) DEFAULT 1);
ALTER TABLE u ADD COLUMN c VARCHAR(20);
SELECT sql FROM sqlite_master WHERE name = 'u';
ALTER TABLE u DROP COLUMN a;
SELECT sql FROM sqlite_master WHERE name = 'u';
-- case: holdout/alter-table-and-schema-c/540-table-row-order-of-columns-after-drop-column-and-r
CREATE TABLE t (id TEXT PRIMARY KEY, a TEXT, b TEXT);
INSERT INTO t VALUES ('x', 'A1', 'B1');
ALTER TABLE t RENAME COLUMN a TO a2;
SELECT * FROM t;
ALTER TABLE t DROP COLUMN a2;
SELECT * FROM t;
INSERT INTO t VALUES ('y', 'B2');
SELECT * FROM t ORDER BY id;
-- case: holdout/alter-table-and-schema-c/555-drop-column-keeps-a-column-whose-name-needs-quotin
CREATE TABLE t(a, b, [c c]);
INSERT INTO t VALUES (1, 2, 3);
ALTER TABLE t DROP COLUMN b;
SELECT * FROM t;
SELECT sql FROM sqlite_master WHERE tbl_name = 't';
SELECT name FROM pragma_table_info('t');
-- case: holdout/alter-table-and-schema-c/582-rename-to-a-name-that-needs-quoting
create table t(a);
insert into t values (1);
alter table t rename to `t t`;
select * from `t t`;
select name, tbl_name, sql from sqlite_schema;
alter table "t t" rename to "x""y";
select name, sql from sqlite_schema;
select * from "x""y";
-- case: holdout/alter-table-and-schema-c/584-add-column-with-an-unquoted-identifier-as-default
create table t(a);
alter table t add column b default asdf;
insert into t default values;
select *, typeof(b) from t;
alter table t add column c default true;
alter table t add column d default false;
alter table t add column e default null;
alter table t add column f default current_date;
alter table t add column g default "q";
insert into t default values;
select a, b, c, d, e, g from t;
-- case: holdout/alter-table-and-schema-c/598-alter-column-type-change-rewrites-stored-values-to
CREATE TABLE t(x NUMERIC);
INSERT INTO t VALUES (1);
INSERT INTO t VALUES ('1');
ALTER TABLE t ALTER COLUMN x TO y TEXT;
SELECT typeof(x), x FROM t;
-- case: holdout/alter-table-and-schema-c/606-rename-to-rewrites-table-names-inside-views
CREATE TABLE t1(id INT);
CREATE VIEW v AS SELECT * FROM t1 WHERE id > 0;
INSERT INTO t1 VALUES (1);
ALTER TABLE t1 RENAME TO t1_new;
SELECT * FROM v;
SELECT sql FROM sqlite_schema ORDER BY name;
-- case: holdout/alter-table-and-schema-c/639-rename-to-updates-the-row-in-sqlite-sequence
CREATE TABLE t1(id INTEGER PRIMARY KEY AUTOINCREMENT, val TEXT);
INSERT INTO t1(val) VALUES ('a');
ALTER TABLE t1 RENAME TO t2;
SELECT * FROM sqlite_sequence;
INSERT INTO t2(val) VALUES ('b');
SELECT * FROM sqlite_sequence;
SELECT * FROM t2;
-- case: holdout/alter-table-and-schema-c/650-rename-column-rewrites-a-view-that-references-the-
CREATE TABLE t(a,b);
CREATE VIEW v AS SELECT a,b FROM t;
ALTER TABLE t RENAME COLUMN b TO c;
SELECT sql FROM sqlite_master WHERE type='view' AND name='v';
CREATE VIEW v2 AS SELECT * FROM t;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_master WHERE type='view' ORDER BY name;
SELECT * FROM v2;
-- case: holdout/alter-table-and-schema-c/687-drop-column-keeps-autoincrement-in-the-stored-sche
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, doomed INT, v TEXT);
INSERT INTO t(doomed, v) VALUES (9, 'a'), (8, 'b');
ALTER TABLE t DROP COLUMN doomed;
INSERT INTO t(v) VALUES ('c');
SELECT 'schema1', sql FROM sqlite_schema WHERE name = 't';
SELECT 'rows1', id, v FROM t ORDER BY id;
SELECT 'seq1', name, seq FROM sqlite_sequence;
DELETE FROM t;
INSERT INTO t(v) VALUES ('d');
SELECT 'rows2', id, v FROM t;
-- case: holdout/alter-table-and-schema-c/727-alter-column-not-null-with-existing-null-values-sq
CREATE TABLE t(a);
INSERT INTO t VALUES (NULL), (1);
ALTER TABLE t ALTER COLUMN a SET NOT NULL;
ALTER TABLE t ALTER COLUMN a TO b NOT NULL;
SELECT sql FROM sqlite_schema WHERE name = 't';
-- case: holdout/alter-table-and-schema-c/755-alter-table-rename-leaves-no-stale-sqlite-sequence
CREATE TABLE t1(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t1(v) VALUES ('a'), ('b');
ALTER TABLE t1 RENAME TO t2;
SELECT * FROM sqlite_sequence;
INSERT INTO t2(v) VALUES ('c');
SELECT * FROM sqlite_sequence;
SELECT id FROM t2 ORDER BY id;
-- case: holdout/alter-table-and-schema-c/758-drop-column-of-a-column-referenced-by-a-check-is-r
CREATE TABLE t(a, b, CHECK (a > 0));
ALTER TABLE t DROP COLUMN a;
CREATE TABLE u(a, b CHECK (b > a));
ALTER TABLE u DROP COLUMN a;
ALTER TABLE u DROP COLUMN b;
SELECT sql FROM sqlite_master WHERE type='table';
-- case: holdout/alter-table-and-schema-c/760-drop-column-of-a-column-with-a-unique-constraint-o
CREATE TABLE t(a UNIQUE, b);
ALTER TABLE t DROP COLUMN a;
CREATE TABLE u(a, b, UNIQUE(a, b));
ALTER TABLE u DROP COLUMN a;
CREATE TABLE v(a PRIMARY KEY, b);
ALTER TABLE v DROP COLUMN a;
CREATE TABLE w(a, b);
CREATE INDEX wi ON w(a);
ALTER TABLE w DROP COLUMN a;
CREATE TABLE x(a, b);
CREATE INDEX xi ON x(b) WHERE a > 0;
ALTER TABLE x DROP COLUMN a;
CREATE TABLE y(a, b, FOREIGN KEY(a) REFERENCES v(a));
ALTER TABLE y DROP COLUMN a;
CREATE TABLE z(a);
ALTER TABLE z DROP COLUMN a;
-- case: holdout/alter-table-and-schema-c/799-add-column-with-non-constant-default-expressions-o
CREATE TABLE t(a);
INSERT INTO t VALUES (1);
ALTER TABLE t ADD COLUMN b DEFAULT (random());
ALTER TABLE t ADD COLUMN c DEFAULT (1+2);
ALTER TABLE t ADD COLUMN d DEFAULT (abs(-3));
ALTER TABLE t ADD COLUMN e DEFAULT (a);
ALTER TABLE t ADD COLUMN f DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE t ADD COLUMN g DEFAULT CURRENT_DATE;
ALTER TABLE t ADD COLUMN h DEFAULT (datetime('now'));
ALTER TABLE t ADD COLUMN i DEFAULT (3);
ALTER TABLE t ADD COLUMN j DEFAULT (-3);
ALTER TABLE t ADD COLUMN k DEFAULT 'a' || 'b';
SELECT sql FROM sqlite_master;
-- case: holdout/alter-table-and-schema-c/812-add-column-with-a-check-that-contains-a-subquery
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN b CHECK((SELECT 1));
CREATE TABLE u(a CHECK((SELECT 1)));
CREATE TABLE v(a, CHECK(a IN (SELECT 1)));
CREATE TABLE w(a CHECK(random() > 0));
CREATE TABLE x(a DEFAULT (SELECT 1));
ALTER TABLE t ADD COLUMN c CHECK(c > 0);
