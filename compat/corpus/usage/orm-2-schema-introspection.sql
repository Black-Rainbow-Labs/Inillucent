-- case: orm/2/2.1-pragma-table-info-columns-pk-order-dflt-value-as-sql-text
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT 'x', n INT DEFAULT 5, f REAL DEFAULT 1.5, c DEFAULT CURRENT_TIMESTAMP, e DEFAULT (1+2), b BLOB DEFAULT x'ab', z DEFAULT NULL, neg DEFAULT -1); PRAGMA table_info(t);
-- case: orm/2/2.2-table-info-on-composite-primary-key-reports-pk-position
CREATE TABLE t(a,b,c,PRIMARY KEY(c,a)); PRAGMA table_info(t);
-- case: orm/2/2.3-table-info-on-missing-table-returns-zero-rows-no-error
PRAGMA table_info(nope);
-- case: orm/2/2.4-table-xinfo-shows-hidden-and-generated-columns
CREATE TABLE t(a INT, b INT GENERATED ALWAYS AS (a*2) VIRTUAL, c INT AS (a+1) STORED); PRAGMA table_xinfo(t); PRAGMA table_info(t);
-- case: orm/2/2.5-table-info-for-a-view-and-for-type-less-column
CREATE TABLE t(a, b INTEGER, c VARCHAR(20), d DECIMAL(10,2), e UNSIGNED BIG INT); CREATE VIEW v AS SELECT a, b+1 AS x, upper(c) FROM t; PRAGMA table_info(v); PRAGMA table_info(t);
-- case: orm/2/2.6-table-list-3-37
CREATE TABLE t(a); CREATE VIEW v AS SELECT 1; CREATE TABLE w(a PRIMARY KEY) WITHOUT ROWID; CREATE TABLE s(a INT) STRICT; SELECT * FROM pragma_table_list ORDER BY schema, name;
-- case: orm/2/2.7-index-list-origin-c-u-pk-and-partial-flag
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE, b TEXT, c TEXT, d TEXT); CREATE INDEX ib ON t(b); CREATE UNIQUE INDEX ic ON t(c) WHERE c IS NOT NULL; PRAGMA index_list(t);
-- case: orm/2/2.8-index-info-and-index-xinfo-incl-desc-collation-rowid
CREATE TABLE t(a,b,c); CREATE INDEX i ON t(a DESC, b COLLATE NOCASE, lower(c)); PRAGMA index_info(i); PRAGMA index_xinfo(i);
-- case: orm/2/2.9-foreign-key-list-with-actions-and-match
CREATE TABLE p(id INTEGER PRIMARY KEY, u TEXT UNIQUE); CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT REFERENCES p(id) ON DELETE CASCADE ON UPDATE SET NULL, u TEXT, FOREIGN KEY(u) REFERENCES p(u) DEFERRABLE INITIALLY DEFERRED); PRAGMA foreign_key_list(c);
-- case: orm/2/2.10-foreign-key-list-with-implicit-parent-column-gives-null-to
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p); PRAGMA foreign_key_list(c);
-- case: orm/2/2.11-sqlite-master-rows-and-sql-text-preserved-verbatim
CREATE TABLE  t ( a  INT ,
 b TEXT ) ; CREATE INDEX i ON t(a); CREATE VIEW v AS SELECT * FROM t; CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; END; SELECT type,name,tbl_name,rootpage>0,sql FROM sqlite_master ORDER BY name;
-- case: orm/2/2.12-autoindex-entries-have-null-sql-in-sqlite-master
CREATE TABLE t(a TEXT UNIQUE, b TEXT PRIMARY KEY); SELECT type,name,tbl_name,sql IS NULL FROM sqlite_master ORDER BY name;
-- case: orm/2/2.13-sqlite-sequence-appears-only-after-an-autoincrement-table
SELECT count(*) FROM sqlite_master; CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); SELECT name FROM sqlite_master ORDER BY name; SELECT * FROM sqlite_sequence; INSERT INTO t(v) VALUES(1),(2); SELECT * FROM sqlite_sequence;
-- case: orm/2/2.14-sqlite-sequence-after-delete-and-explicit-big-rowid
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t(v) VALUES(1),(2); DELETE FROM t; INSERT INTO t(v) VALUES(3); SELECT id FROM t; INSERT INTO t(id,v) VALUES(100,4); SELECT * FROM sqlite_sequence;
-- case: orm/2/2.15-sqlite-master-is-also-sqlite-schema-temp-schema-table
SELECT count(*) FROM sqlite_schema; CREATE TEMP TABLE t(a); SELECT name FROM sqlite_temp_master; SELECT name FROM sqlite_temp_schema;
-- case: orm/2/2.16-prisma-style-tables-list-excluding-sqlite-internal
CREATE TABLE a(x); CREATE TABLE b(y); CREATE TABLE c(id INTEGER PRIMARY KEY AUTOINCREMENT); SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name;
-- case: orm/2/2.17-like-escape-underscore-in-sqlite-matches-any-char-orm-gotcha
CREATE TABLE sqliteXfoo(a); CREATE TABLE sqlite_foo(a); CREATE TABLE ok(a); SELECT name FROM sqlite_master WHERE name LIKE 'sqlite_%' ESCAPE '\' ORDER BY name; SELECT name FROM sqlite_master WHERE name LIKE 'sqlite\_%' ESCAPE '\' ORDER BY name;
-- case: orm/2/2.18-django-get-table-description-table-info-with-without-rowid-c
CREATE TABLE t(a INTEGER PRIMARY KEY, b) WITHOUT ROWID; SELECT sql FROM sqlite_master WHERE name='t'; SELECT type FROM pragma_table_list WHERE name='t'; SELECT wr FROM pragma_table_list WHERE name='t';
-- case: orm/2/2.19-django-pragma-foreign-key-list-sqlite-master-get-constraints
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(id INTEGER PRIMARY KEY, pid INT NOT NULL REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED); SELECT * FROM pragma_foreign_key_list('c'); SELECT sql FROM sqlite_master WHERE name='c';
-- case: orm/2/2.20-rails-schema-dumper-pragma-index-list-joined-to-index-info
CREATE TABLE t(id INTEGER PRIMARY KEY, a, b); CREATE UNIQUE INDEX ia ON t(a, b DESC); SELECT il.name, il."unique", il.origin, il.partial, ii.seqno, ii.name FROM pragma_index_list('t') il JOIN pragma_index_info(il.name) ii;
-- case: orm/2/2.21-rails-sqlite-master-where-sql-like-for-partial-index-where-c
CREATE TABLE t(id INTEGER PRIMARY KEY, a, deleted_at); CREATE UNIQUE INDEX iu ON t(a) WHERE deleted_at IS NULL; SELECT sql FROM sqlite_master WHERE name='iu'; SELECT partial FROM pragma_index_list('t');
-- case: orm/2/2.22-ef-core-pragma-table-info-joined-to-sqlite-master
CREATE TABLE t(Id INTEGER PRIMARY KEY, Name TEXT, Price NUMERIC(10,2)); SELECT m.name, p.cid, p.name, p.type, p."notnull", p.dflt_value, p.pk FROM sqlite_master m, pragma_table_info(m.name) p WHERE m.type='table' ORDER BY 1,2;
-- case: orm/2/2.23-table-info-hidden-column-number-type-for-virtual-table-colum
CREATE VIRTUAL TABLE ft USING fts5(a, b); PRAGMA table_xinfo(ft);
-- case: orm/2/2.24-database-list-and-pragma-collation-list
SELECT seq, name, file LIKE '%case.%' FROM pragma_database_list; PRAGMA collation_list;
-- case: orm/2/2.25-pragma-foreign-key-check-output-columns
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id)); INSERT INTO c VALUES(1,99),(2,NULL); PRAGMA foreign_key_check; PRAGMA foreign_key_check(c);
-- case: orm/2/2.26-pragma-integrity-check-and-quick-check-ok
CREATE TABLE t(a); PRAGMA integrity_check; PRAGMA quick_check; PRAGMA integrity_check(1);
-- case: orm/2/2.27-pragma-function-list-module-list-contain-core-items
SELECT count(*)>50 FROM pragma_function_list; SELECT name,builtin,type,narg FROM pragma_function_list WHERE name IN ('abs','max','sum','json_extract') ORDER BY name,narg;
-- case: orm/2/2.28-alembic-reflection-column-type-strings-come-back-exactly-as-
CREATE TABLE t(a VARCHAR(30), b numeric(10, 2), c DOUBLE PRECISION, d "My Type", e TIMESTAMP WITHOUT TIME ZONE, f int  unsigned); SELECT name, type FROM pragma_table_info('t');
