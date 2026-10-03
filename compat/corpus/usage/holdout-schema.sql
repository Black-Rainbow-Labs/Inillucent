-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/schema/31-integer-primary-key-desc-is-not-a-rowid-alias
create table t(a integer primary key desc);
insert into t(a) values (123);
select rowid, * from t;
create table u(a integer, primary key(a desc));
insert into u(a) values (123);
select rowid, * from u;
-- case: holdout/schema/43-two-primary-key-clauses-on-one-column-or-on-two-co
create table t(a primary key primary key);
create table u(a primary key, b primary key);
-- case: holdout/schema/57-create-view-using-an-aggregate-filter-clause
CREATE TABLE t(val INT, status TEXT);
INSERT INTO t VALUES (100, 'ok'), (200, 'fail');
CREATE VIEW v AS SELECT SUM(val) FILTER (WHERE status = 'ok') FROM t;
SELECT * FROM v;
-- case: holdout/schema/124-rollback-to-a-savepoint-taken-before-create-table
SAVEPOINT s1;
CREATE TABLE t(x INTEGER);
INSERT INTO t VALUES (42);
ROLLBACK TO s1;
SELECT * FROM t;
RELEASE s1;
SELECT count(*) FROM sqlite_schema;
-- case: holdout/schema/138-create-view-with-a-name-starting-with-sqlite
create view sqlite_view as select 1;
create table sqlite_t(a);
create index sqlite_i on sqlite_master(name);
create trigger sqlite_tr after insert on x begin select 1; end;
-- case: holdout/schema/154-pragma-boolean-with-a-float-argument-and-other-odd
PRAGMA query_only = 0.1;
PRAGMA query_only;
PRAGMA query_only = 'off';
PRAGMA query_only = 'yes';
PRAGMA query_only;
PRAGMA query_only = 2;
PRAGMA query_only;
PRAGMA query_only = abc;
PRAGMA query_only;
-- case: holdout/schema/155-sqlite-schema-keeps-the-case-the-object-was-create
create table tTt(aAa);
create view vVv as select * from tTt;
select type, name, tbl_name, sql from sqlite_schema;
select name from pragma_table_info('TTT');
-- case: holdout/schema/158-column-names-keep-their-declared-case-in-results-a
create table t(aAa);
SELECT name FROM pragma_table_info('t') ORDER BY cid;
insert into t values (1), (2), (3);
select * from t;
select aAa, AAA, "aaa" as "Mixed" from t limit 1;
-- case: holdout/schema/279-autoincrement-on-a-column-that-is-not-an-integer-p
CREATE TABLE t(a INT PRIMARY KEY AUTOINCREMENT);
CREATE TABLE u(a INTEGER PRIMARY KEY DESC AUTOINCREMENT);
CREATE TABLE v(a INTEGER, PRIMARY KEY(a AUTOINCREMENT));
CREATE TABLE w(a INTEGER, b INTEGER, PRIMARY KEY(a, b AUTOINCREMENT));
CREATE TABLE x(a INTEGER PRIMARY KEY AUTOINCREMENT) WITHOUT ROWID;
-- case: holdout/schema/282-quoted-type-name-integer-before-primary-key-autoin
CREATE TABLE t(a "INTEGER" PRIMARY KEY AUTOINCREMENT);
INSERT INTO t DEFAULT VALUES;
SELECT rowid, a FROM t;
CREATE TABLE u(a 'INTEGER' PRIMARY KEY);
INSERT INTO u VALUES(5);
SELECT rowid, a FROM u;
CREATE TABLE w(a `INTEGER` PRIMARY KEY, b [INTEGER] PRIMARY KEY);
-- case: holdout/schema/329-unknown-and-unusual-pragma-names-are-ignored-witho
PRAGMA recursive_triggers=OFF;
PRAGMA no_such_pragma_name;
PRAGMA no_such_pragma_name = 5;
PRAGMA main.no_such_pragma_name;
PRAGMA recursive_triggers;
-- case: holdout/schema/331-pragma-table-info-pk-column-gives-the-position-ins
CREATE TABLE t(a, b, c, d, PRIMARY KEY(c, a, d));
SELECT name, pk FROM pragma_table_info('t');
CREATE TABLE u(a, b, PRIMARY KEY(b DESC, a)) WITHOUT ROWID;
SELECT name, pk FROM pragma_table_info('u');
PRAGMA index_info(sqlite_autoindex_t_1);
-- case: holdout/schema/342-autoincrement-adds-only-sqlite-sequence-to-the-sch
CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, x);
INSERT INTO t(x) VALUES (1);
SELECT name FROM sqlite_master WHERE type='table' AND name NOT GLOB 'sqlite*';
SELECT name FROM sqlite_master ORDER BY name;
DROP TABLE t;
SELECT name FROM sqlite_master ORDER BY name;
SELECT * FROM sqlite_sequence;
-- case: holdout/schema/439-pragma-names-are-case-insensitive
CREATE TABLE t(id INTEGER PRIMARY KEY);
PRAGMA USER_VERSION = 9;
PRAGMA user_version;
PRAGMA TABLE_INFO(t);
PRAGMA FOREIGN_KEYS = ON;
CREATE TABLE u(id INTEGER PRIMARY KEY, x INTEGER REFERENCES u(id));
INSERT INTO u VALUES(1,999);
SELECT count(*) FROM u;
PRAGMA Foreign_Keys;
-- case: holdout/schema/488-pragma-with-an-unknown-schema-prefix
PRAGMA nosuch.user_version;
PRAGMA nosuch.concurrent_mode = ON;
PRAGMA nosuch.table_info(t);
PRAGMA main.user_version;
-- case: holdout/schema/493-column-name-of-a-scalar-subquery-and-of-other-expr
CREATE TABLE t(v INTEGER);
SELECT (SELECT max(v) FROM t);
SELECT 1+2, (1+2), -v, +v, v*2 AS dbl, "v", [v], `v`, 'lit', t.v, main.t.v, abs(v) FROM t;
SELECT (SELECT max(v) FROM t) AS x, EXISTS(SELECT 1) , v IN (1,2), v BETWEEN 1 AND 2, CASE WHEN 1 THEN 2 END, CAST(v AS TEXT), v IS NULL FROM t;
-- case: holdout/schema/551-a-quoted-table-name-is-stored-without-the-quotes-i
CREATE TABLE "t1" ("id" integer);
CREATE TABLE t2 ("id" integer);
SELECT name, tbl_name, sql FROM sqlite_master ORDER BY name;
-- case: holdout/schema/563-create-table-with-backtick-quoted-names-and-a-prim
CREATE TABLE `databases` (`id` integer PRIMARY KEY);
CREATE TABLE [t2] ([id] integer PRIMARY KEY, `n` text UNIQUE);
INSERT INTO `databases` VALUES (1);
SELECT `id` FROM [databases];
SELECT name, sql FROM sqlite_schema ORDER BY name;
-- case: holdout/schema/566-oid-and-rowid-are-aliases-of-rowid
create table t(a);
insert into t values (7);
select oid, _rowid_, rowid, ROWID, Oid from t;
select t.oid, t._rowid_ from t;
create table u("rowid" TEXT, v);
insert into u values ('x', 1);
select rowid, _rowid_, oid from u;
-- case: holdout/schema/577-tables-with-the-sqlite-prefix-cannot-be-created-or
drop table sqlite_schema;
drop table sqlite_master;
create table sqlite_x(a);
create table sqlite_sequence2(a);
create table SQLITE_Y(a);
drop table if exists sqlite_sequence;
create table t(a integer primary key autoincrement);
drop table sqlite_sequence;
-- case: holdout/schema/578-a-declared-column-named-rowid-shadows-the-implicit
CREATE TABLE t("rowid" TEXT, v);
INSERT INTO t VALUES ('not_the_rowid', 42);
SELECT "rowid", _rowid_, oid FROM t;
SELECT rowid, t.rowid FROM t;
CREATE TABLE u(oid TEXT, v);
INSERT INTO u VALUES ('o', 1);
SELECT oid, rowid, _rowid_ FROM u;
-- case: holdout/schema/593-create-table-with-duplicate-column-names
create table t(a, a);
create table u(a, A);
create table v(a, "a");
create table w(a, b, a);
create table x(a INTEGER PRIMARY KEY, b, PRIMARY KEY(b));
-- case: holdout/schema/602-a-double-quoted-identifier-with-a-dot-is-a-column-
create table t(a);
insert into t values (123);
select "t.a" from t;
create table u('t.a');
insert into u values (123);
select "t.a" from u;
select u."t.a" from u;
-- case: holdout/schema/634-column-definitions-with-a-repeated-type-name-and-a
CREATE TABLE a ( c1 NUMERIC NUMERIC, c2 INTEGER ( 513886854 ) CONSTRAINT pk PRIMARY KEY, c3, c4 CONSTRAINT nn );
CREATE TABLE b ( c1 NUMERIC NUMERIC );
CREATE TABLE c ( c2 INTEGER ( 513886854 ) CONSTRAINT pk PRIMARY KEY );
CREATE TABLE d ( c4 CONSTRAINT nn );
CREATE TABLE e ( c1 INT(5,6,7) );
CREATE TABLE f ( c1 VARCHAR(10) UNIQUE );
SELECT name, sql FROM sqlite_schema ORDER BY name;
-- case: holdout/schema/638-autoincrement-on-a-column-whose-type-is-not-intege
create table t(potato potato PRIMARY KEY AUTOINCREMENT);
create table u(Potato INT PRIMARY KEY AUTOINCREMENT);
create table v(a INTEGER PRIMARY KEY AUTOINCREMENT);
create table w(a integer PRIMARY KEY AUTOINCREMENT);
create table x(a "INTEGER" PRIMARY KEY AUTOINCREMENT);
-- case: holdout/schema/646-pragma-cache-size-at-the-minimum-64-bit-integer
PRAGMA cache_size(-9223372036854775808);
PRAGMA cache_size;
PRAGMA cache_size = 5022422913188235998;
PRAGMA cache_size;
PRAGMA cache_size = -2000;
PRAGMA cache_size;
-- case: holdout/schema/660-vacuum-into-keeps-rowids-of-tables-with-an-explici
CREATE TABLE t(a TEXT);
INSERT INTO t(rowid, a) VALUES(5, 'x');
VACUUM INTO ':memory:';
SELECT rowid, a FROM t;
CREATE TABLE u(id INTEGER PRIMARY KEY, a TEXT);
INSERT INTO u VALUES (7, 'y');
SELECT rowid, id FROM u;
-- case: holdout/schema/661-drop-table-removes-the-sqlite-sequence-row-of-a-mi
CREATE TABLE "MiXeD Name"(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO "MiXeD Name"(v) VALUES('x');
SELECT name, seq FROM sqlite_sequence;
DROP TABLE "MiXeD Name";
SELECT name, seq FROM sqlite_sequence;
CREATE TABLE "MiXeD Name"(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO "MiXeD Name"(v) VALUES('y');
SELECT id,v FROM "MiXeD Name";
SELECT name,seq FROM sqlite_sequence;
-- case: holdout/schema/662-sqlite-sequence-name-keeps-the-case-of-a-quoted-ta
CREATE TABLE "MiXeD Name"(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO "MiXeD Name"(v) VALUES('x');
UPDATE sqlite_sequence SET seq=100 WHERE name='MiXeD Name';
INSERT INTO "MiXeD Name"(v) VALUES('y');
SELECT id,v FROM "MiXeD Name" ORDER BY id;
SELECT name,seq FROM sqlite_sequence;
-- case: holdout/schema/670-rowid-of-table-valued-functions
SELECT rowid, * FROM json_each('{"a":1}');
SELECT rowid, * FROM json_tree('{"a":1}');
SELECT rowid, * FROM generate_series(1, 3);
SELECT rowid, key FROM json_each('[5,6,7]') WHERE rowid > 0;
-- case: holdout/schema/699-querying-pragma-table-info-does-not-affect-later-w
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);
SELECT name FROM pragma_table_info('t');
INSERT INTO t(v) VALUES ('x');
SELECT * FROM t;
SELECT * FROM pragma_table_info('t') WHERE name = 'v';
UPDATE t SET v = 'y';
SELECT * FROM t;
-- case: holdout/schema/752-savepoint-rollback-restores-the-autoincrement-coun
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO t(v) VALUES ('a');
SAVEPOINT s;
INSERT INTO t(v) VALUES ('b');
INSERT INTO t(v) VALUES ('c');
ROLLBACK TO s;
SELECT * FROM sqlite_sequence;
INSERT INTO t(v) VALUES ('d');
SELECT id, v FROM t ORDER BY id;
RELEASE s;
BEGIN;
INSERT INTO t(v) VALUES ('e');
ROLLBACK;
INSERT INTO t(v) VALUES ('f');
SELECT id, v FROM t ORDER BY id;
-- case: holdout/schema/753-a-double-quoted-token-that-is-not-a-column-is-a-st
CREATE TABLE t(x);
INSERT INTO t VALUES (1), (2);
SELECT "no_such" FROM t ORDER BY x;
SELECT x FROM t WHERE x = "1";
SELECT "x" FROM t ORDER BY "x" DESC;
INSERT INTO t VALUES ("lit");
SELECT x FROM t ORDER BY rowid;
CREATE TABLE u(a DEFAULT "dflt");
INSERT INTO u DEFAULT VALUES;
SELECT a FROM u;
-- case: holdout/schema/804-pragma-reverse-unordered-selects-reverses-scans-wi
CREATE TABLE t(a);
INSERT INTO t VALUES (1), (2), (3);
PRAGMA reverse_unordered_selects;
PRAGMA reverse_unordered_selects = ON;
PRAGMA reverse_unordered_selects;
SELECT a FROM t;
SELECT a FROM t ORDER BY a;
SELECT a FROM t ORDER BY a DESC;
-- case: holdout/schema/818-application-id-and-user-version-are-signed-32-bit-
PRAGMA application_id = -1;
PRAGMA application_id;
PRAGMA user_version = -5;
PRAGMA user_version;
PRAGMA user_version = 2147483648;
PRAGMA user_version;
PRAGMA user_version = 4294967295;
PRAGMA user_version;
PRAGMA user_version = 2147483647;
PRAGMA user_version;
PRAGMA application_id = 4294967296;
PRAGMA application_id;
PRAGMA user_version = 'abc';
PRAGMA user_version;
-- case: holdout/schema/821-readback-of-settings-pragmas
PRAGMA synchronous;
PRAGMA synchronous = OFF;
PRAGMA synchronous;
PRAGMA synchronous = NORMAL;
PRAGMA synchronous;
PRAGMA synchronous = 3;
PRAGMA synchronous;
PRAGMA locking_mode;
PRAGMA locking_mode = EXCLUSIVE;
PRAGMA locking_mode;
PRAGMA journal_mode;
PRAGMA journal_mode = DELETE;
PRAGMA journal_mode = MEMORY;
PRAGMA cache_spill;
PRAGMA cache_spill = OFF;
PRAGMA cache_spill;
PRAGMA secure_delete;
PRAGMA secure_delete = FAST;
PRAGMA secure_delete;
PRAGMA read_uncommitted;
PRAGMA read_uncommitted = 1;
PRAGMA read_uncommitted;
PRAGMA automatic_index;
PRAGMA automatic_index = OFF;
PRAGMA automatic_index;
PRAGMA trusted_schema;
PRAGMA threads;
PRAGMA threads = 4;
PRAGMA threads;
PRAGMA cell_size_check;
PRAGMA cell_size_check = ON;
PRAGMA cell_size_check;
PRAGMA checkpoint_fullfsync;
PRAGMA soft_heap_limit;
PRAGMA soft_heap_limit = 100000;
PRAGMA soft_heap_limit;
PRAGMA hard_heap_limit;
PRAGMA temp_store;
PRAGMA temp_store = FILE;
PRAGMA temp_store;
PRAGMA encoding;
PRAGMA page_size;
PRAGMA max_page_count;
PRAGMA default_cache_size;
PRAGMA wal_autocheckpoint;
PRAGMA busy_timeout;
PRAGMA foreign_keys;
PRAGMA recursive_triggers;
PRAGMA ignore_check_constraints;
PRAGMA count_changes;
PRAGMA full_column_names;
PRAGMA short_column_names;
PRAGMA legacy_file_format;
PRAGMA mmap_size;
PRAGMA query_only;
PRAGMA writable_schema;
PRAGMA case_sensitive_like;
PRAGMA data_version;
PRAGMA schema_version;
PRAGMA freelist_count;
PRAGMA page_count;
PRAGMA auto_vacuum;
PRAGMA analysis_limit;
PRAGMA cache_size;
PRAGMA integrity_check;
PRAGMA quick_check;
-- case: holdout/schema/832-create-table-as-select-into-a-table-name-that-cont
CREATE TABLE other(a);
INSERT INTO other VALUES (0);
CREATE TABLE src(a);
INSERT INTO src VALUES (1);
CREATE TABLE "o""x"("a") ;
DROP TABLE "o""x";
CREATE TABLE "o""x" AS SELECT a FROM src;
SELECT * FROM "o""x";
SELECT * FROM other;
SELECT name FROM sqlite_master ORDER BY name;
-- case: holdout/schema/837-default-cache-size-and-cache-size-defaults
PRAGMA default_cache_size;
PRAGMA cache_size;
PRAGMA default_cache_size = 500;
PRAGMA default_cache_size;
PRAGMA cache_size;
