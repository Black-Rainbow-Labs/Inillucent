-- case: bug-hunt-10/drop/fts5-shadow-table-cannot-be-dropped
CREATE VIRTUAL TABLE f USING fts5(x);
INSERT INTO f VALUES('hello');
DROP TABLE f_content;
DROP TABLE f_config;
DROP TABLE f_data;
DROP TABLE f_docsize;
DROP TABLE f_idx;
SELECT * FROM f WHERE f MATCH 'hello';
-- reopen
SELECT * FROM f WHERE f MATCH 'hello';
-- case: bug-hunt-10/drop/fts5-shadow-table-cannot-be-altered
CREATE VIRTUAL TABLE f USING fts5(x);
ALTER TABLE f_content ADD COLUMN y;
ALTER TABLE f_config RENAME TO g;
ALTER TABLE f_data RENAME COLUMN id TO i;
ALTER TABLE f_docsize DROP COLUMN sz;
INSERT INTO f VALUES('hello');
SELECT * FROM f WHERE f MATCH 'hello';
-- case: bug-hunt-10/drop/fts5-shadow-table-cannot-have-triggers
CREATE VIRTUAL TABLE f USING fts5(x);
CREATE TRIGGER tr AFTER INSERT ON f_content BEGIN SELECT 1; END;
CREATE TRIGGER tr2 AFTER INSERT ON f_idx BEGIN SELECT 1; END;
SELECT name FROM sqlite_schema WHERE type = 'trigger';
-- case: bug-hunt-10/drop/drop-view-drops-its-instead-of-trigger
CREATE TABLE t(a);
CREATE VIEW v AS SELECT a FROM t;
CREATE TRIGGER tr INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(new.a); END;
DROP VIEW v;
SELECT name FROM sqlite_schema ORDER BY name;
CREATE VIEW v AS SELECT a FROM t;
INSERT INTO v VALUES(1);
-- case: bug-hunt-10/drop/drop-view-drops-a-temp-trigger-on-it
CREATE TABLE t(a);
CREATE VIEW v AS SELECT a FROM t;
CREATE TEMP TRIGGER tr INSTEAD OF INSERT ON main.v BEGIN INSERT INTO t VALUES(new.a); END;
DROP VIEW v;
SELECT name FROM temp.sqlite_schema;
-- case: bug-hunt-10/drop/drop-table-drops-a-temp-trigger-on-it
CREATE TABLE t(a);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN SELECT 1; END;
CREATE TEMP TRIGGER tr2 AFTER INSERT ON t BEGIN SELECT 1; END;
DROP TABLE t;
SELECT name FROM temp.sqlite_schema;
-- case: bug-hunt-10/drop/drop-table-keeps-a-temp-trigger-on-the-temp-table-of-the-same-name
CREATE TABLE t(a);
CREATE TEMP TABLE t(a);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN SELECT 1; END;
CREATE TEMP TRIGGER tr2 AFTER INSERT ON temp.t BEGIN SELECT 1; END;
DROP TABLE main.t;
SELECT name FROM temp.sqlite_schema ORDER BY name;
-- case: bug-hunt-10/drop/drop-parent-table-deferred-foreign-key-violation
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(1, 1);
BEGIN;
DROP TABLE p;
COMMIT;
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/drop/drop-parent-table-defer-foreign-keys-pragma
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id));
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(1, 1);
PRAGMA defer_foreign_keys = ON;
BEGIN;
DROP TABLE p;
COMMIT;
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/drop/drop-parent-table-deferred-key-repaired-before-commit
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO p VALUES(1);
INSERT INTO c VALUES(1, 1);
BEGIN;
DROP TABLE p;
CREATE TABLE p(id INTEGER PRIMARY KEY);
INSERT INTO p VALUES(1);
COMMIT;
SELECT * FROM c;
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/drop/django-remake-with-a-deferred-key
PRAGMA foreign_keys=ON;
CREATE TABLE "parent" ("id" integer PRIMARY KEY);
CREATE TABLE "t" ("id" integer PRIMARY KEY, "pid" integer REFERENCES "parent" ("id") DEFERRABLE INITIALLY DEFERRED);
CREATE TABLE "ch" ("id" integer PRIMARY KEY, "tid" integer REFERENCES "t" ("id") DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES(1); INSERT INTO t VALUES(1,1); INSERT INTO ch VALUES(1,1);
BEGIN;
CREATE TABLE "new__t" ("id" integer NOT NULL PRIMARY KEY, "pid" integer NULL REFERENCES "parent" ("id") DEFERRABLE INITIALLY DEFERRED);
INSERT INTO "new__t" ("id", "pid") SELECT "id", "pid" FROM "t";
DROP TABLE "t";
ALTER TABLE "new__t" RENAME TO "t";
COMMIT;
SELECT name FROM sqlite_schema ORDER BY name;
PRAGMA foreign_key_check;
-- case: bug-hunt-10/drop/django-remake-renaming-the-old-table-first
PRAGMA foreign_keys=ON;
CREATE TABLE "parent" ("id" integer PRIMARY KEY);
CREATE TABLE "t" ("id" integer PRIMARY KEY, "pid" integer REFERENCES "parent" ("id") DEFERRABLE INITIALLY DEFERRED);
CREATE TABLE "ch" ("id" integer PRIMARY KEY, "tid" integer REFERENCES "t" ("id") DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES(1); INSERT INTO t VALUES(1,1); INSERT INTO ch VALUES(1,1);
BEGIN;
ALTER TABLE "t" RENAME TO "t__old";
CREATE TABLE "t" ("id" integer PRIMARY KEY, "pid" integer REFERENCES "parent" ("id") DEFERRABLE INITIALLY DEFERRED, "x" integer DEFAULT 0);
INSERT INTO "t"("id", "pid") SELECT "id", "pid" FROM "t__old";
DROP TABLE "t__old";
COMMIT;
SELECT name FROM sqlite_schema ORDER BY name;
PRAGMA foreign_key_list(ch);
PRAGMA foreign_key_check;
-- case: bug-hunt-10/drop/rename-fts5-table
CREATE VIRTUAL TABLE f USING fts5(x);
INSERT INTO f VALUES('hello');
ALTER TABLE f RENAME TO g;
SELECT name FROM sqlite_schema ORDER BY name;
SELECT name, sql FROM sqlite_schema WHERE name <> 'g_idx' ORDER BY name;
SELECT * FROM g WHERE g MATCH 'hello';
INSERT INTO g VALUES('hello again');
SELECT rowid, x FROM g WHERE g MATCH 'hello' ORDER BY rowid;
-- reopen
SELECT name FROM sqlite_schema ORDER BY name;
SELECT rowid, x FROM g WHERE g MATCH 'hello' ORDER BY rowid;
INSERT INTO g VALUES('third hello');
SELECT count(*) FROM g WHERE g MATCH 'hello';
PRAGMA integrity_check;
-- case: bug-hunt-10/drop/rename-fts5-table-with-a-quoted-name
CREATE VIRTUAL TABLE "my docs" USING fts5(x, y);
INSERT INTO "my docs" VALUES('a b', 'c');
ALTER TABLE "my docs" RENAME TO "new docs";
SELECT name FROM sqlite_schema ORDER BY name;
SELECT x FROM "new docs" WHERE "new docs" MATCH 'a';
-- reopen
SELECT x FROM "new docs" WHERE "new docs" MATCH 'a';
-- case: bug-hunt-10/drop/rename-fts5-table-in-a-rolled-back-transaction
CREATE VIRTUAL TABLE f USING fts5(x);
INSERT INTO f VALUES('hello');
BEGIN;
ALTER TABLE f RENAME TO g;
ROLLBACK;
SELECT name FROM sqlite_schema ORDER BY name;
SELECT * FROM f WHERE f MATCH 'hello';
-- case: bug-hunt-10/drop/rename-fts5-table-onto-an-existing-name
CREATE VIRTUAL TABLE f USING fts5(x);
CREATE TABLE g(a);
ALTER TABLE f RENAME TO g;
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/drop/rename-fts5-table-with-a-trigger-on-its-content-table
CREATE TABLE c(id INTEGER PRIMARY KEY, x);
CREATE VIRTUAL TABLE f USING fts5(x, content='c', content_rowid='id');
INSERT INTO c VALUES(1, 'hello');
INSERT INTO f(rowid, x) VALUES(1, 'hello');
ALTER TABLE f RENAME TO g;
SELECT name FROM sqlite_schema ORDER BY name;
SELECT rowid FROM g WHERE g MATCH 'hello';
-- case: bug-hunt-10/drop/schema-version-after-rolled-back-alter
CREATE TABLE t(a);
PRAGMA schema_version;
BEGIN;
ALTER TABLE t ADD COLUMN b;
ROLLBACK;
PRAGMA schema_version;
-- reopen
PRAGMA schema_version;
-- case: bug-hunt-10/drop/schema-version-after-rolled-back-create-and-drop
CREATE TABLE t(a);
PRAGMA schema_version;
BEGIN;
CREATE TABLE u(a);
DROP TABLE t;
PRAGMA schema_version;
ROLLBACK;
PRAGMA schema_version;
-- case: bug-hunt-10/drop/schema-version-after-committed-ddl-in-a-transaction
CREATE TABLE t(a);
BEGIN;
CREATE TABLE u(a);
ALTER TABLE u ADD COLUMN b;
COMMIT;
PRAGMA schema_version;
-- reopen
PRAGMA schema_version;
-- case: bug-hunt-10/drop/table-xinfo-of-a-table-valued-function
PRAGMA table_xinfo(generate_series);
PRAGMA table_info(generate_series);
