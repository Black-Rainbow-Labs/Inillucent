-- case: bug-hunt-10/alter/rename-table-keeps-cte-with-same-name-in-view
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,2);
CREATE VIEW v AS SELECT * FROM t WHERE a IN (WITH t AS (SELECT 7 AS a) SELECT a FROM t);
ALTER TABLE t RENAME TO nt;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-table-cte-with-same-name-view-still-works
CREATE TABLE t(a,b);
CREATE VIEW v AS WITH t AS (SELECT 1 AS x) SELECT x FROM t;
ALTER TABLE t RENAME TO nt;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-table-recursive-cte-with-same-name
CREATE TABLE t(a,b);
CREATE VIEW v AS WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM t WHERE n < 3) SELECT n FROM t;
ALTER TABLE t RENAME TO nt;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-table-cte-declared-after-another-cte
CREATE TABLE t(a,b);
INSERT INTO t VALUES(10,20);
CREATE VIEW v AS WITH c1 AS (SELECT a FROM t), t AS (SELECT 5 AS a) SELECT c1.a, t.a AS ta FROM c1, t;
ALTER TABLE t RENAME TO nt;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-table-cte-in-trigger-body
CREATE TABLE t(a,b);
CREATE TABLE log(x);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log SELECT a FROM (WITH t AS (SELECT 9 AS a) SELECT a FROM t); END;
ALTER TABLE t RENAME TO nt;
SELECT sql FROM sqlite_schema WHERE name = 'tr';
INSERT INTO nt VALUES(1,2);
SELECT * FROM log;
-- case: bug-hunt-10/alter/rename-column-keeps-order-by-result-alias
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS SELECT b AS a FROM t ORDER BY a;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-keeps-order-by-result-alias-with-limit
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS SELECT b AS a FROM t ORDER BY a LIMIT 2;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-keeps-order-by-result-alias-distinct
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS SELECT DISTINCT b AS a FROM t ORDER BY a;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-keeps-order-by-result-alias-in-cte
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS WITH c AS (SELECT b AS a FROM t ORDER BY a) SELECT * FROM c;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-keeps-order-by-result-alias-in-subquery
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS SELECT a FROM t WHERE b IN (SELECT b AS a FROM t ORDER BY a LIMIT 2);
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-still-renames-order-by-real-column
CREATE TABLE t(a,b);
INSERT INTO t VALUES(1,5),(2,4),(3,3);
CREATE VIEW v AS SELECT b FROM t ORDER BY a;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/add-generated-column-reads-itself
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS (c);
SELECT * FROM t;
-- case: bug-hunt-10/alter/add-generated-column-reads-itself-then-insert
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS (c + 1);
INSERT INTO t(a) VALUES(1);
-- case: bug-hunt-10/alter/generated-columns-loop-through-two-columns
CREATE TABLE t(a, b, c);
PRAGMA writable_schema=ON;
UPDATE sqlite_schema SET sql='CREATE TABLE t(a, b AS (c), c AS (b), d AS (a))' WHERE name='t';
PRAGMA writable_schema=OFF;
-- reopen
SELECT * FROM t;
-- case: bug-hunt-10/alter/add-generated-column-unknown-column
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS (zz + 1);
SELECT * FROM t;
-- case: bug-hunt-10/alter/add-generated-column-nondeterministic
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS (random());
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-10/alter/add-generated-column-subquery
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS ((SELECT 1));
SELECT sql FROM sqlite_schema;
INSERT INTO t(a) VALUES(1);
SELECT * FROM t;
-- case: bug-hunt-10/alter/add-generated-column-qualified-name
CREATE TABLE t(a);
ALTER TABLE t ADD COLUMN c AS (t.a + 1);
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-10/alter/add-generated-column-valid
CREATE TABLE t(a);
INSERT INTO t VALUES(4);
ALTER TABLE t ADD COLUMN c AS (a * 2);
ALTER TABLE t ADD COLUMN d AS (c + a) STORED;
SELECT * FROM t;
-- case: bug-hunt-10/alter/rename-column-used-in-using-clause
CREATE TABLE t(a,b);
CREATE TABLE u(a,c);
CREATE VIEW v AS SELECT t.b, u.c FROM t JOIN u USING(a);
ALTER TABLE t RENAME COLUMN a TO z;
SELECT sql FROM sqlite_schema WHERE name = 'v';
SELECT * FROM v;
-- case: bug-hunt-10/alter/rename-column-trigger-upsert-target-where
CREATE TABLE s(k UNIQUE, v);
CREATE TABLE o(a);
CREATE TRIGGER tr AFTER INSERT ON o BEGIN INSERT INTO s(k, v) VALUES(new.a, 1) ON CONFLICT(k) WHERE v IS NOT NULL DO NOTHING; END;
ALTER TABLE s RENAME COLUMN v TO w;
SELECT sql FROM sqlite_schema WHERE name = 'tr';
-- case: bug-hunt-10/alter/rename-column-trigger-upsert-update-set-reads-column
CREATE TABLE s(k UNIQUE, v);
CREATE TABLE o(a);
CREATE TRIGGER tr AFTER INSERT ON o BEGIN INSERT INTO s(k, v) VALUES(new.a, 1) ON CONFLICT(k) DO UPDATE SET v = v + excluded.v WHERE v < 5; END;
ALTER TABLE s RENAME COLUMN v TO w;
SELECT sql FROM sqlite_schema WHERE name = 'tr';
INSERT INTO o VALUES(1);
INSERT INTO o VALUES(1);
SELECT * FROM s;
-- case: bug-hunt-10/alter/rename-column-temp-table-shadows-main
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP VIEW tv AS SELECT a FROM main.t;
ALTER TABLE main.t RENAME COLUMN a TO z;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tv';
SELECT * FROM tv;
-- case: bug-hunt-10/alter/rename-column-temp-trigger-on-main-table-shadowed
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN SELECT new.a; END;
ALTER TABLE main.t RENAME COLUMN a TO z;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tr';
-- case: bug-hunt-10/alter/rename-column-leaves-temp-view-of-the-temp-table
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP VIEW tv AS SELECT a FROM t;
ALTER TABLE main.t RENAME COLUMN a TO z;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tv';
SELECT * FROM tv;
-- case: bug-hunt-10/alter/rename-table-temp-table-shadows-main
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP VIEW tv AS SELECT a FROM main.t;
ALTER TABLE main.t RENAME TO nt;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tv';
SELECT * FROM tv;
-- case: bug-hunt-10/alter/rename-table-leaves-temp-view-of-the-temp-table
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP VIEW tv AS SELECT a FROM t;
ALTER TABLE main.t RENAME TO nt;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tv';
SELECT * FROM tv;
-- case: bug-hunt-10/alter/rename-table-temp-trigger-on-main-table-shadowed
CREATE TABLE t(a,b);
CREATE TEMP TABLE t(a,c);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN SELECT 1; END;
ALTER TABLE main.t RENAME TO nt;
SELECT sql FROM temp.sqlite_schema WHERE name = 'tr';
-- case: bug-hunt-10/alter/rename-column-temp-view-over-main-view
CREATE TABLE t(a,b);
CREATE VIEW mv AS SELECT a FROM t;
CREATE TEMP VIEW tv AS SELECT a FROM mv;
ALTER TABLE t RENAME COLUMN a TO z;
SELECT * FROM tv;
-- case: bug-hunt-10/alter/drop-column-read-by-temp-view
CREATE TABLE t(a,b);
CREATE TEMP VIEW v AS SELECT b FROM t;
ALTER TABLE t DROP COLUMN b;
SELECT * FROM v;
-- case: bug-hunt-10/alter/drop-column-read-by-temp-trigger
CREATE TABLE t(a,b);
CREATE TEMP TRIGGER tr AFTER INSERT ON t BEGIN SELECT new.b; END;
ALTER TABLE t DROP COLUMN b;
INSERT INTO t VALUES(1);
-- case: bug-hunt-10/alter/drop-column-not-read-by-temp-view
CREATE TABLE t(a,b);
CREATE TEMP VIEW v AS SELECT a FROM t;
ALTER TABLE t DROP COLUMN b;
SELECT * FROM v;
-- case: bug-hunt-10/alter/legacy-alter-table-partial-index-qualified-where
PRAGMA legacy_alter_table = ON;
CREATE TABLE t(a,c);
CREATE INDEX i ON t(a) WHERE t.c > 5;
ALTER TABLE t RENAME TO nt;
INSERT INTO nt VALUES(1, 10);
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/alter/legacy-alter-table-partial-index-plain-where
PRAGMA legacy_alter_table = ON;
CREATE TABLE t(a,c);
CREATE INDEX i ON t(a) WHERE c > 5;
ALTER TABLE t RENAME TO nt;
INSERT INTO nt VALUES(1, 10);
SELECT sql FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/alter/add-column-default-reads-old-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
ALTER TABLE t ADD COLUMN b DEFAULT 5;
ALTER TABLE t ADD COLUMN c;
ALTER TABLE t ADD COLUMN d TEXT DEFAULT 'x';
SELECT a, b, c, d FROM t ORDER BY a;
SELECT count(*) FROM t WHERE c IS NULL;
UPDATE t SET b = 7 WHERE a = 2;
INSERT INTO t(a) VALUES(4);
SELECT a, b, c, d FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-10/alter/add-column-default-index-created-afterwards
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
ALTER TABLE t ADD COLUMN b DEFAULT 5;
CREATE INDEX ib ON t(b);
SELECT a FROM t WHERE b = 5 ORDER BY a;
UPDATE t SET b = 6 WHERE a = 2;
SELECT a FROM t WHERE b = 5 ORDER BY a;
SELECT a FROM t WHERE b = 6;
CREATE INDEX iab ON t(a, b);
SELECT a, b FROM t WHERE a >= 2 ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-10/alter/add-column-default-after-reopen
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT '5';
ALTER TABLE t ADD COLUMN c REAL DEFAULT 1.5;
-- reopen
SELECT a, b, typeof(b), c FROM t ORDER BY a;
UPDATE t SET c = 2.5 WHERE a = 3;
SELECT a, b, c FROM t ORDER BY a;
CREATE INDEX ic ON t(c);
SELECT a FROM t WHERE c = 1.5 ORDER BY a;
-- reopen
SELECT a FROM t WHERE c = 1.5 ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-10/alter/add-column-check-validated-against-old-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b CHECK (b > 0) DEFAULT 0;
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-10/alter/add-column-check-passes-for-old-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b CHECK (b > 0) DEFAULT 3;
SELECT a, b FROM t ORDER BY a;
INSERT INTO t VALUES(3, 0);
-- case: bug-hunt-10/alter/add-column-not-null-with-default-old-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b NOT NULL DEFAULT 3;
SELECT a, b FROM t ORDER BY a;
ALTER TABLE t ADD COLUMN c NOT NULL;
-- case: bug-hunt-10/alter/add-column-non-constant-default-refused
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
ALTER TABLE t ADD COLUMN b DEFAULT (1 + 1);
-- case: bug-hunt-10/alter/add-column-then-delete-and-drop-column
CREATE TABLE t(a, z);
INSERT INTO t VALUES(1, 'p'),(2, 'q'),(3, 'r');
ALTER TABLE t ADD COLUMN b DEFAULT 9;
DELETE FROM t WHERE a = 2;
ALTER TABLE t DROP COLUMN z;
SELECT a, b FROM t ORDER BY a;
PRAGMA integrity_check;
-- case: bug-hunt-10/alter/add-column-without-rowid-table
CREATE TABLE t(k PRIMARY KEY, a) WITHOUT ROWID;
INSERT INTO t VALUES(1,'x'),(2,'y');
ALTER TABLE t ADD COLUMN b DEFAULT 4;
SELECT k, a, b FROM t ORDER BY k;
UPDATE t SET b = 8 WHERE k = 1;
SELECT k, a, b FROM t ORDER BY k;
-- reopen
SELECT k, a, b FROM t ORDER BY k;
PRAGMA integrity_check;
-- case: bug-hunt-10/alter/add-column-strict-table
CREATE TABLE t(a INTEGER) STRICT;
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT 5;
SELECT a, b FROM t ORDER BY a;
ALTER TABLE t ADD COLUMN c TEXT DEFAULT 5;
-- case: bug-hunt-10/alter/add-column-references-with-foreign-keys-on
PRAGMA foreign_keys = ON;
CREATE TABLE p(id PRIMARY KEY);
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
ALTER TABLE t ADD COLUMN pid REFERENCES p(id);
SELECT a, pid FROM t;
ALTER TABLE t ADD COLUMN pid2 REFERENCES p(id) DEFAULT 1;
-- case: bug-hunt-10/alter/add-column-with-triggers-and-views
CREATE TABLE t(a);
CREATE VIEW v AS SELECT a FROM t;
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN b DEFAULT 5;
CREATE VIEW w AS SELECT a, b FROM t;
SELECT * FROM v;
SELECT * FROM w ORDER BY a;
-- case: bug-hunt-10/alter/create-trigger-on-sqlite-schema
CREATE TRIGGER tr AFTER INSERT ON sqlite_schema BEGIN SELECT 1; END;
-- case: bug-hunt-10/alter/trigger-body-insert-default-values
CREATE TABLE lg(x INTEGER DEFAULT 5);
CREATE TABLE t(a);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO lg DEFAULT VALUES; END;
SELECT name FROM sqlite_schema ORDER BY name;
-- case: bug-hunt-10/alter/create-index-on-virtual-table-message
CREATE VIRTUAL TABLE f USING fts5(x);
CREATE INDEX i ON f(x);
-- case: bug-hunt-10/alter/create-trigger-on-virtual-table-message
CREATE VIRTUAL TABLE f USING fts5(x);
CREATE TRIGGER tr AFTER INSERT ON f BEGIN SELECT 1; END;
-- case: bug-hunt-10/alter/create-table-trailing-comment-not-kept-in-sql
CREATE TABLE t(a) -- trailing
;
ALTER TABLE t ADD COLUMN b;
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-10/alter/create-table-trailing-comment-not-kept-without-alter
CREATE TABLE t(a) -- trailing
;
SELECT sql FROM sqlite_schema;
-- case: bug-hunt-10/alter/add-constraint-check-existing-null
CREATE TABLE t(a,b);
INSERT INTO t VALUES(NULL,2);
ALTER TABLE t ADD CONSTRAINT c1 CHECK (a < b);
SELECT sql FROM sqlite_schema;
