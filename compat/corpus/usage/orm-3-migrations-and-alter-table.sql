-- case: orm/3/3.1-rename-table-updates-indexes-triggers-views-and-fk-reference
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); CREATE VIEW v AS SELECT * FROM p; ALTER TABLE p RENAME TO q; SELECT name, sql FROM sqlite_master WHERE name IN ('c','v') ORDER BY name;
-- case: orm/3/3.2-rename-with-legacy-alter-table-on-leaves-fk-parent-reference
PRAGMA foreign_keys=OFF; PRAGMA legacy_alter_table=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); ALTER TABLE p RENAME TO q; SELECT sql FROM sqlite_master WHERE name='c';
-- case: orm/3/3.3-rename-with-foreign-keys-off-and-legacy-alter-table-off-stil
PRAGMA foreign_keys=OFF; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); ALTER TABLE p RENAME TO q; SELECT sql FROM sqlite_master WHERE name='c';
-- case: orm/3/3.4-rename-column-rewrites-index-view-trigger-text
CREATE TABLE t(a, b); CREATE INDEX i ON t(a); CREATE VIEW v AS SELECT a FROM t; ALTER TABLE t RENAME COLUMN a TO z; SELECT name,sql FROM sqlite_master ORDER BY name;
-- case: orm/3/3.5-rename-column-to-existing-name-fails
CREATE TABLE t(a, b); ALTER TABLE t RENAME COLUMN a TO b;
-- case: orm/3/3.6-rename-to-existing-table-fails
CREATE TABLE t(a); CREATE TABLE u(a); ALTER TABLE t RENAME TO u;
-- case: orm/3/3.7-rename-of-missing-table
ALTER TABLE nope RENAME TO x;
-- case: orm/3/3.8-add-column-not-null-without-default-is-rejected
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b INT NOT NULL;
-- case: orm/3/3.9-add-column-not-null-without-default-on-an-empty-table-succee
CREATE TABLE t(a); ALTER TABLE t ADD COLUMN b INT NOT NULL; PRAGMA table_info(t); INSERT INTO t(a) VALUES(1);
-- case: orm/3/3.10-add-column-not-null-with-default-works-and-fills
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b INT NOT NULL DEFAULT 7; SELECT * FROM t;
-- case: orm/3/3.11-add-column-with-unique-or-primary-key-is-rejected
CREATE TABLE t(a); ALTER TABLE t ADD COLUMN b INT UNIQUE;
-- case: orm/3/3.12-add-column-primary-key-rejected
CREATE TABLE t(a); ALTER TABLE t ADD COLUMN b INT PRIMARY KEY;
-- case: orm/3/3.13-add-column-non-constant-default-current-timestamp-rejected
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b TEXT DEFAULT CURRENT_TIMESTAMP;
-- case: orm/3/3.14-add-column-with-parenthesised-expression-default-rejected
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b INT DEFAULT (1+1);
-- case: orm/3/3.15-add-column-with-references-requires-null-default-when-fks-on
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN pid INT REFERENCES p(id) DEFAULT 5;
-- case: orm/3/3.16-add-column-stored-generated-is-rejected-on-a-non-empty-table
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b AS (a+1) STORED;
-- case: orm/3/3.17-add-column-virtual-generated-allowed
CREATE TABLE t(a); INSERT INTO t VALUES(1); ALTER TABLE t ADD COLUMN b AS (a+1); SELECT * FROM t;
-- case: orm/3/3.18-drop-column-success-and-sqlite-master-text
CREATE TABLE t(a, b, c); INSERT INTO t VALUES(1,2,3); ALTER TABLE t DROP COLUMN b; SELECT * FROM t; SELECT sql FROM sqlite_master;
-- case: orm/3/3.19-drop-column-fails-for-primary-key
CREATE TABLE t(a PRIMARY KEY, b); ALTER TABLE t DROP COLUMN a;
-- case: orm/3/3.20-drop-column-fails-for-unique
CREATE TABLE t(a, b UNIQUE); ALTER TABLE t DROP COLUMN b;
-- case: orm/3/3.21-drop-column-fails-when-indexed
CREATE TABLE t(a, b); CREATE INDEX i ON t(b); ALTER TABLE t DROP COLUMN b;
-- case: orm/3/3.22-drop-column-fails-when-used-in-a-view
CREATE TABLE t(a, b); CREATE VIEW v AS SELECT b FROM t; ALTER TABLE t DROP COLUMN b;
-- case: orm/3/3.23-drop-column-fails-when-a-table-level-check-uses-it
CREATE TABLE t(a, b, CHECK(b>0)); ALTER TABLE t DROP COLUMN b;
-- case: orm/3/3.24-drop-column-succeeds-when-the-check-is-on-the-dropped-column
CREATE TABLE t(a, b CHECK(b>0)); ALTER TABLE t DROP COLUMN b; SELECT sql FROM sqlite_master;
-- case: orm/3/3.25-drop-column-only-column
CREATE TABLE t(a); ALTER TABLE t DROP COLUMN a;
-- case: orm/3/3.26-drop-column-fails-when-a-table-level-foreign-key-names-it
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(x, pid, FOREIGN KEY(pid) REFERENCES p(id)); ALTER TABLE c DROP COLUMN pid;
-- case: orm/3/3.27-drop-column-succeeds-for-a-column-level-references-clause
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(x, pid REFERENCES p(id)); ALTER TABLE c DROP COLUMN pid; SELECT sql FROM sqlite_master WHERE name='c';
-- case: orm/3/3.28-12-step-rebuild-new-table-copy-drop-rename-keeps-data
PRAGMA foreign_keys=OFF; BEGIN; CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b INT); INSERT INTO t VALUES(1,'x',2),(2,'y',3); CREATE TABLE new_t(id INTEGER PRIMARY KEY, a TEXT NOT NULL DEFAULT '', b INT); INSERT INTO new_t SELECT id,a,b FROM t; DROP TABLE t; ALTER TABLE new_t RENAME TO t; COMMIT; PRAGMA foreign_key_check; SELECT * FROM t; SELECT sql FROM sqlite_master;
-- case: orm/3/3.29-rebuild-when-foreign-keys-on-drop-table-parent-does-implicit
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); INSERT INTO p VALUES(1); INSERT INTO c VALUES(1); DROP TABLE p;
-- case: orm/3/3.30-rebuild-with-foreign-keys-on-and-defer-foreign-keys-violatio
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); INSERT INTO p VALUES(1); INSERT INTO c VALUES(1); BEGIN; PRAGMA defer_foreign_keys=ON; DROP TABLE p; CREATE TABLE p(id INTEGER PRIMARY KEY); INSERT INTO p VALUES(1); COMMIT; SELECT * FROM p;
-- case: orm/3/3.31-defer-foreign-keys-unresolved-at-commit-fails-with-foreign-k
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); BEGIN; PRAGMA defer_foreign_keys=ON; INSERT INTO c VALUES(5); COMMIT;
-- case: orm/3/3.32-defer-foreign-keys-resets-to-off-after-commit
BEGIN; PRAGMA defer_foreign_keys=ON; PRAGMA defer_foreign_keys; COMMIT; PRAGMA defer_foreign_keys;
-- case: orm/3/3.33-alembic-batch-rename-column-via-temp-table-alembic-tmp-t
CREATE TABLE t(id INTEGER NOT NULL, a VARCHAR(10), PRIMARY KEY(id)); INSERT INTO t VALUES(1,'x'); CREATE TABLE _alembic_tmp_t(id INTEGER NOT NULL, b VARCHAR(10), PRIMARY KEY(id)); INSERT INTO _alembic_tmp_t(id,b) SELECT t.id, t.a FROM t; DROP TABLE t; ALTER TABLE _alembic_tmp_t RENAME TO t; SELECT * FROM t;
-- case: orm/3/3.34-django-pragma-foreign-keys-off-refused-inside-transaction-si
PRAGMA foreign_keys=ON; BEGIN; PRAGMA foreign_keys=OFF; PRAGMA foreign_keys; ROLLBACK;
-- case: orm/3/3.35-rename-table-with-view-referencing-nonexistent-column-breaks
CREATE TABLE t(a); CREATE VIEW v AS SELECT zz FROM t; ALTER TABLE t RENAME TO t2;
-- case: orm/3/3.36-create-table-if-not-exists-silent-when-exists-create-index-i
CREATE TABLE t(a); CREATE TABLE IF NOT EXISTS t(a, b); PRAGMA table_info(t); CREATE TABLE t(a);
-- case: orm/3/3.37-drop-table-if-exists-missing-and-drop-index-missing
DROP TABLE IF EXISTS nope; DROP TABLE nope;
-- case: orm/3/3.38-drop-index-drop-view-drop-trigger-missing-messages
DROP INDEX nope;
-- case: orm/3/3.39-drop-view-missing-message
DROP VIEW nope;
-- case: orm/3/3.40-create-unique-index-fails-on-existing-duplicates
CREATE TABLE t(a); INSERT INTO t VALUES(1),(1); CREATE UNIQUE INDEX i ON t(a);
-- case: orm/3/3.41-create-index-on-missing-column-message
CREATE TABLE t(a); CREATE INDEX i ON t(zz);
-- case: orm/3/3.42-create-index-name-collision-with-table
CREATE TABLE t(a); CREATE INDEX t ON t(a);
-- case: orm/3/3.43-alter-table-rename-in-transaction-rolls-back-cleanly
CREATE TABLE t(a); BEGIN; ALTER TABLE t RENAME TO u; ROLLBACK; SELECT name FROM sqlite_master;
-- case: orm/3/3.44-ddl-is-transactional-create-table-rolled-back
BEGIN; CREATE TABLE t(a); INSERT INTO t VALUES(1); ROLLBACK; SELECT count(*) FROM sqlite_master;
-- case: orm/3/3.45-alter-add-constraint-syntax-not-supported-3-53-adds-not-null
CREATE TABLE t(a, b); ALTER TABLE t ADD CONSTRAINT c1 UNIQUE(a);
-- case: orm/3/3.46-alter-table-alter-column-not-supported-prisma-django-emulate
CREATE TABLE t(a); ALTER TABLE t ALTER COLUMN a SET NOT NULL;
-- case: orm/3/3.47-alter-table-add-column-twice-duplicate-column-name
CREATE TABLE t(a); ALTER TABLE t ADD COLUMN a INT;
-- case: orm/3/3.48-alter-table-on-sqlite-table
ALTER TABLE sqlite_master RENAME TO x;
-- case: orm/3/3.49-writable-schema-off-update-sqlite-master-refused
CREATE TABLE t(a); UPDATE sqlite_master SET name='x' WHERE name='t';
-- case: orm/3/3.50-vacuum-inside-transaction
BEGIN; VACUUM;
-- case: orm/3/3.51-vacuum-into-writes-a-copy
CREATE TABLE t(a); INSERT INTO t VALUES(1); VACUUM INTO ':memory:'; SELECT 'ok';
-- case: orm/3/3.52-reindex-and-analyze-create-sqlite-stat1
CREATE TABLE t(a,b); CREATE INDEX i ON t(a); INSERT INTO t VALUES(1,1),(1,2),(2,3); ANALYZE; SELECT * FROM sqlite_stat1; REINDEX;
