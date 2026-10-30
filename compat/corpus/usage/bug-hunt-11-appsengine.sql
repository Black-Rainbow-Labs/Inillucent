-- case: bug-hunt-11/appsengine/deferred-fk-violation-written-with-fk-off-does-not-block-later-writes
CREATE TABLE u (id integer PRIMARY KEY);
CREATE TABLE b (id integer PRIMARY KEY, a integer REFERENCES u (id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO b (a) VALUES (999);
PRAGMA foreign_keys = ON;
CREATE TABLE z (x);
INSERT INTO z VALUES (1);
SELECT * FROM z;
UPDATE b SET id = 5;
ALTER TABLE b RENAME TO bx;
SELECT * FROM bx;
-- case: bug-hunt-11/appsengine/deferred-fk-new-violation-still-fails-beside-an-old-one
CREATE TABLE u (id integer PRIMARY KEY);
CREATE TABLE b (id integer PRIMARY KEY, a integer REFERENCES u (id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO b (a) VALUES (999);
PRAGMA foreign_keys = ON;
INSERT INTO b (a) VALUES (998);
SELECT count(*) FROM b;
BEGIN;
INSERT INTO b (a) VALUES (997);
COMMIT;
SELECT count(*) FROM b;
ROLLBACK;
SELECT count(*) FROM b;
-- case: bug-hunt-11/appsengine/deferred-fk-parent-delete-after-old-violation
CREATE TABLE u (id integer PRIMARY KEY);
CREATE TABLE b (id integer PRIMARY KEY, a integer REFERENCES u (id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO u VALUES (1), (2);
INSERT INTO b (a) VALUES (999), (1);
PRAGMA foreign_keys = ON;
DELETE FROM u WHERE id = 2;
SELECT count(*) FROM u;
DELETE FROM u WHERE id = 1;
SELECT count(*) FROM u;
-- case: bug-hunt-11/appsengine/deferred-fk-violation-fixed-in-the-same-transaction
CREATE TABLE u (id integer PRIMARY KEY);
CREATE TABLE b (id integer PRIMARY KEY, a integer REFERENCES u (id) DEFERRABLE INITIALLY DEFERRED);
PRAGMA foreign_keys = ON;
BEGIN;
INSERT INTO b (a) VALUES (7);
INSERT INTO u VALUES (7);
COMMIT;
SELECT count(*) FROM b;
-- case: bug-hunt-11/appsengine/django-twelve-step-rebuild-with-an-old-orphan
PRAGMA foreign_keys = OFF;
CREATE TABLE parent (id integer PRIMARY KEY, name text);
CREATE TABLE child (id integer PRIMARY KEY, parent_id integer REFERENCES parent (id) DEFERRABLE INITIALLY DEFERRED);
INSERT INTO parent VALUES (1, 'a');
INSERT INTO child VALUES (1, 1), (2, 77);
PRAGMA foreign_keys = ON;
BEGIN;
CREATE TABLE parent_new (id integer PRIMARY KEY, name text, extra text);
INSERT INTO parent_new (id, name) SELECT id, name FROM parent;
DROP TABLE parent;
ALTER TABLE parent_new RENAME TO parent;
COMMIT;
SELECT * FROM parent;
SELECT * FROM child;
-- case: bug-hunt-11/appsengine/legacy-alter-table-drop-column-used-by-trigger
PRAGMA legacy_alter_table=ON;
CREATE TABLE c (id INTEGER PRIMARY KEY, note TEXT);
CREATE TABLE audit (m);
CREATE TRIGGER tc AFTER UPDATE OF note ON c WHEN new.note IS NOT old.note BEGIN INSERT INTO audit VALUES (new.note); END;
ALTER TABLE c DROP COLUMN note;
SELECT sql FROM sqlite_master WHERE name IN ('c', 'tc') ORDER BY name;
-- case: bug-hunt-11/appsengine/legacy-alter-table-drop-column-used-by-view
PRAGMA legacy_alter_table=ON;
CREATE TABLE c (id INTEGER PRIMARY KEY, note TEXT);
CREATE VIEW v AS SELECT note FROM c;
ALTER TABLE c DROP COLUMN note;
SELECT sql FROM sqlite_master WHERE name IN ('c', 'v') ORDER BY name;
-- case: bug-hunt-11/appsengine/legacy-alter-table-rename-column-view-and-trigger
PRAGMA legacy_alter_table=ON;
CREATE TABLE p (id INTEGER PRIMARY KEY, name TEXT);
CREATE VIEW vp AS SELECT id, name FROM p;
CREATE TRIGGER tv INSTEAD OF DELETE ON vp BEGIN DELETE FROM p WHERE id = old.id; END;
ALTER TABLE p RENAME COLUMN id TO pk;
SELECT sql FROM sqlite_master ORDER BY name;
-- case: bug-hunt-11/appsengine/rename-column-attached-view-resolves-in-its-own-schema
ATTACH 'apps-h1-a.db' AS a;
CREATE TABLE main.t (x INTEGER PRIMARY KEY, y TEXT);
CREATE TABLE a.t (x INTEGER PRIMARY KEY, y TEXT);
CREATE TEMP TABLE t (x INTEGER PRIMARY KEY, y TEXT);
CREATE VIEW a.v AS SELECT y FROM t;
ALTER TABLE a.t RENAME COLUMN y TO yy;
SELECT sql FROM a.sqlite_master ORDER BY name;
-- case: bug-hunt-11/appsengine/drop-column-attached-view-resolves-in-its-own-schema
ATTACH 'apps-h1-a.db' AS a;
CREATE TABLE main.t (x INTEGER PRIMARY KEY, y TEXT);
CREATE TABLE a.t (x INTEGER PRIMARY KEY, y TEXT);
CREATE VIEW a.v AS SELECT y FROM t;
ALTER TABLE a.t DROP COLUMN y;
SELECT sql FROM a.sqlite_master ORDER BY name;
-- case: bug-hunt-11/appsengine/read-only-connection-allows-temp-tables-and-memory-attach
.open rod1.db
CREATE TABLE keep (a);
INSERT INTO keep VALUES (1);
.open --readonly rod1.db
CREATE TEMP TABLE z(a);
INSERT INTO z VALUES (1);
SELECT * FROM z;
CREATE TEMP VIEW tv AS SELECT 1 AS one;
SELECT * FROM tv;
ATTACH ':memory:' AS m;
CREATE TABLE m.q(a);
INSERT INTO keep VALUES (2);
SELECT * FROM keep;
-- case: bug-hunt-11/appsengine/temp-trigger-may-write-to-another-schema
ATTACH 'apps-g3-a.db' AS a;
CREATE TABLE main.log (m TEXT);
CREATE TABLE temp.t (x INTEGER PRIMARY KEY, y TEXT);
CREATE TRIGGER temp.tr1 AFTER INSERT ON temp.t BEGIN INSERT INTO main.log VALUES ('temp:' || new.y); END;
INSERT INTO temp.t VALUES (9, 'z');
SELECT * FROM main.log;
-- case: bug-hunt-11/appsengine/distinct-order-by-column-not-selected
CREATE TABLE u (id INTEGER PRIMARY KEY, active);
INSERT INTO u VALUES (1, 1), (2, 0), (3, 1), (4, NULL), (5, 0);
SELECT DISTINCT active FROM u ORDER BY id DESC;
SELECT DISTINCT active FROM u ORDER BY id;
-- case: bug-hunt-11/appsengine/returning-names-the-table-and-not-its-alias
CREATE TABLE t (id INTEGER PRIMARY KEY, v INT);
INSERT INTO t AS x VALUES (1, 1) RETURNING x.id;
INSERT INTO t AS x VALUES (2, 1) RETURNING t.id;
UPDATE t AS x SET v = 0 RETURNING x.id;
UPDATE t AS x SET v = 0 RETURNING t.id;
UPDATE t AS x SET v = 1 WHERE x.id = 1 RETURNING id, v;
DELETE FROM t AS x WHERE id = 7 RETURNING x.id;
DELETE FROM t AS x WHERE id = 2 RETURNING t.id;
SELECT * FROM t;
-- case: bug-hunt-11/appsengine/fk-cascade-compares-parent-and-child-without-child-affinity
PRAGMA foreign_keys=ON; CREATE TABLE p3(code UNIQUE); CREATE TABLE c3(k INTEGER REFERENCES p3(code) ON DELETE CASCADE); INSERT INTO p3 VALUES('5'),(5); INSERT INTO c3 VALUES(5); DELETE FROM p3 WHERE code='5'; SELECT quote(k) FROM c3;
-- case: bug-hunt-11/appsengine/strict-insert-or-ignore-resolves-primary-key-before-type
CREATE TABLE r(id INTEGER PRIMARY KEY, a INT) STRICT; INSERT INTO r VALUES (1,1); INSERT OR IGNORE INTO r VALUES (1,'x'); SELECT * FROM r;
-- case: bug-hunt-11/appsengine/update-nonkey-column-ignores-fk-mismatch-on-parent
CREATE TABLE c(x, w); CREATE TABLE d(r REFERENCES c); INSERT INTO c VALUES(1,1); PRAGMA foreign_keys=1; UPDATE c SET w=3; SELECT * FROM c;
-- case: bug-hunt-11/appsengine/update-nonkey-column-ignores-fk-mismatch-on-child
CREATE TABLE p(a, b); CREATE TABLE c(x, y REFERENCES p(b)); INSERT INTO c VALUES (1, NULL); PRAGMA foreign_keys = 1; UPDATE c SET x = 2; SELECT * FROM c;
-- case: bug-hunt-11/appsengine/update-key-column-reports-fk-mismatch
CREATE TABLE c(x, w); CREATE TABLE d(r REFERENCES c); INSERT INTO c VALUES(1,1); PRAGMA foreign_keys=1; UPDATE c SET x=3; SELECT * FROM c;
-- case: bug-hunt-11/appsengine/autoincrement-trigger-rows-during-replace-advance-the-sequence
PRAGMA recursive_triggers=ON; CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, x UNIQUE); INSERT INTO t(x) VALUES ('a'),('b'); CREATE TRIGGER td AFTER DELETE ON t WHEN old.x='b' BEGIN DELETE FROM t WHERE x='a'; INSERT INTO t(x) VALUES ('trig'); END; INSERT OR REPLACE INTO t(x) VALUES ('b'); SELECT id, x FROM t ORDER BY id; SELECT * FROM sqlite_sequence;
-- case: bug-hunt-11/appsengine/autoincrement-sequence-after-trigger-insert-in-replace
PRAGMA recursive_triggers=ON; CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, x UNIQUE); INSERT INTO t(x) VALUES ('a'); CREATE TRIGGER td AFTER DELETE ON t BEGIN INSERT INTO t(x) VALUES ('trig'); END; INSERT OR REPLACE INTO t(x) VALUES ('a'); SELECT id, x FROM t ORDER BY id; SELECT * FROM sqlite_sequence;
-- case: bug-hunt-11/appsengine/column-named-true-in-primary-key
CREATE TABLE t (true INTEGER, b INTEGER, PRIMARY KEY (true, b)); SELECT name FROM pragma_table_info('t');
-- case: bug-hunt-11/appsengine/column-named-false-in-unique
CREATE TABLE t (false INTEGER, b INTEGER, UNIQUE (false)); SELECT name FROM pragma_table_info('t');
-- case: bug-hunt-11/appsengine/drop-table-if-exists-unknown-schema
DROP TABLE IF EXISTS nosuchdb.x;
-- case: bug-hunt-11/appsengine/drop-index-if-exists-unknown-schema
DROP INDEX IF EXISTS nosuchdb.x;
-- case: bug-hunt-11/appsengine/drop-view-and-trigger-if-exists-unknown-schema
DROP VIEW IF EXISTS nosuchdb.x;
DROP TRIGGER IF EXISTS nosuchdb.x;
DROP TABLE nosuchdb.x;
-- case: bug-hunt-11/appsengine/sqlite-master-name-qualifies-sqlite-schema
CREATE TABLE t(a); SELECT sqlite_master.name FROM sqlite_schema;
SELECT sqlite_schema.name FROM sqlite_master;
-- case: bug-hunt-11/appsengine/upsert-set-unknown-column-when-no-conflict-possible
CREATE TABLE t(a, b INTEGER PRIMARY KEY); INSERT INTO t(a) VALUES (1) ON CONFLICT DO UPDATE SET nosuch = 1; SELECT * FROM t;
-- case: bug-hunt-11/appsengine/upsert-set-rowid
CREATE TABLE t(id INTEGER PRIMARY KEY, k UNIQUE, v); INSERT INTO t VALUES (1, 'a', 1); INSERT INTO t VALUES (2, 'a', 2) ON CONFLICT(k) DO UPDATE SET rowid = 10, v = excluded.v; SELECT * FROM t;
-- case: bug-hunt-11/appsengine/integer-pragma-given-a-float-literal
PRAGMA user_version=8.192e3; PRAGMA user_version;
PRAGMA user_version=7.9; PRAGMA user_version;
PRAGMA user_version=-2.5; PRAGMA user_version;
-- case: bug-hunt-11/appsengine/window-function-order-by-inside-arguments
CREATE TABLE t(a,b); INSERT INTO t VALUES(-1,2); SELECT row_number(ORDER BY a) OVER () FROM t;
-- case: bug-hunt-11/appsengine/aggregate-in-update-from-set-expression
CREATE TABLE e(k, v); INSERT INTO e VALUES (1, '[1,2]'), (2, '[3]'); UPDATE e SET v = json_group_array(s.x) FROM (SELECT 7 AS x UNION ALL SELECT 8) s WHERE e.k = 1; SELECT * FROM e;
-- case: bug-hunt-11/appsengine/analyze-removes-stale-stat1-rows
ANALYZE; INSERT INTO sqlite_stat1 VALUES('x',NULL,'1'); ANALYZE; SELECT * FROM sqlite_stat1;
-- case: bug-hunt-11/appsengine/insert-values-with-clause-shadows-outer-cte
CREATE TABLE t(a int); WITH c AS (SELECT 1 AS x) INSERT INTO t WITH c AS (SELECT 2 AS x) VALUES ((SELECT x FROM c)); SELECT * FROM t;
-- case: bug-hunt-11/appsengine/generated-usable-as-type-name
CREATE TABLE t(x, a GENERATED AS (x+1)); INSERT INTO t(x) VALUES(4); SELECT * FROM t;
-- case: bug-hunt-11/appsengine/generated-alone-as-type-name
CREATE TABLE t3(x, a GENERATED); SELECT name, type FROM pragma_table_info('t3');
-- case: bug-hunt-11/appsengine/distinct-order-by-indexed-column-not-selected
CREATE TABLE u (id INTEGER PRIMARY KEY, active, name);
INSERT INTO u VALUES (1, 1, 'a'), (2, 0, 'b'), (3, 1, 'c'), (4, NULL, 'd'), (5, 0, 'e');
CREATE INDEX un ON u(name);
SELECT DISTINCT active FROM u ORDER BY name DESC;
SELECT DISTINCT active FROM u ORDER BY id DESC, name;
