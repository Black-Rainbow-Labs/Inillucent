-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/string-functions/5-sqlite-schema-stores-the-unquoted-name-and-the-sql
create table "a""a"(a);
select type, name, tbl_name, rootpage, sql from sqlite_schema;
-- case: holdout/string-functions/11-unique-violation-message-text
CREATE TABLE test (id INTEGER unique);
insert into test values (1);
insert into test values (1);
SELECT * FROM test;
-- case: holdout/string-functions/117-nulls-first-nulls-last
CREATE TABLE t(id INTEGER PRIMARY KEY, val INT);
INSERT INTO t VALUES (1, NULL), (2, 3), (3, 1), (4, NULL), (5, 2);
SELECT * FROM t ORDER BY val DESC NULLS FIRST, id;
SELECT * FROM t ORDER BY val NULLS LAST, id;
SELECT * FROM t ORDER BY val ASC NULLS FIRST, id;
SELECT * FROM t ORDER BY val DESC, id;
-- case: holdout/string-functions/139-table-and-view-names-compare-case-insensitively-fo
create table é(a);
create table É(a);
create table abc(a);
create table ABC(a);
select name from sqlite_schema;
-- case: holdout/string-functions/141-substr-with-a-negative-start-on-multi-byte-text
SELECT substr('café', -2, 3), substr('café', -1, 2), substr('café', -4, 5);
SELECT substr('日本語', -1, 1), substr('日本語', -2, 2), substr('日本語', -3, 3);
SELECT substr('hello', -2, 3), substr('hello', 0, 3), substr('hello', -10, 7), substr('hello', 2, -2), substr('hello', 6), substr('hello', 7);
-- case: holdout/string-functions/153-pragma-table-info-keeps-the-type-text-with-its-par
CREATE TABLE t(a VARCHAR(100), b CHAR(50), c DECIMAL(10,2), d NUMERIC(5,3), e FLOAT(24), f NVARCHAR(200), "g" unsigned big int, h double precision);
PRAGMA table_info(t);
SELECT name, type FROM pragma_table_xinfo('t');
-- case: holdout/string-functions/179-regexp-on-non-text-operands-without-a-registered-r
CREATE TABLE t1(a INTEGER, b REAL, c TEXT, d BLOB);
INSERT INTO t1 VALUES(123, 45.6, '789', X'414243');
SELECT a REGEXP '12', b REGEXP '45', c REGEXP '78', d REGEXP '41' FROM t1;
SELECT 1 REGEXP '[1]';
-- case: holdout/string-functions/182-unicode-char-0-is-null
SELECT typeof(unicode(char(0))), unicode(char(0)) IS NULL;
SELECT unicode(''), unicode('a'), unicode('é'), unicode('😀'), unicode(x'41'), unicode(65), unicode(NULL);
SELECT hex(char(0)), length(char(0)), hex(char(65, 0, 66)), char(), hex(char(1114112)), hex(char(-1)), hex(char(55296));
-- case: holdout/string-functions/195-generate-series-1-null-returns-no-rows
SELECT count(*) FROM (SELECT * FROM generate_series(1, NULL) LIMIT 5);
SELECT * FROM generate_series(NULL, 5);
SELECT count(*) FROM generate_series(1, 5, NULL);
SELECT count(*) FROM (SELECT * FROM generate_series(1) LIMIT 7);
SELECT value FROM generate_series(1) LIMIT 3;
-- case: holdout/string-functions/264-arithmetic-on-text-integers-beyond-64-bits-turns-i
SELECT typeof('9223372036854775808'+0), quote('9223372036854775808'+0);
SELECT typeof('9223372036854775807'+0), quote('-9223372036854775808'+0), quote('-9223372036854775809'+0);
CREATE TABLE t(a);
INSERT INTO t(rowid,a) VALUES(2,'9223372036854775807');
INSERT INTO t(rowid,a) VALUES(1,'9223372036854775808');
SELECT rowid, quote(a), typeof(a+0), quote(a+0) FROM t ORDER BY a+0;
SELECT '1e3'+0, typeof('1e3'+0), '0x10'+0, ' 12 '+0, '12abc'+0, '.5'+0, '5.'+0, '1_000'+0;
-- case: holdout/string-functions/343-no-such-column-error-uses-the-identifier-text-from
CREATE TABLE MAIN (Id INTEGER, Id1 INTEGER);
CREATE TABLE B (Id INTEGER, Id1 INTEGER);
CREATE VIEW v2 AS SELECT * FROM MAIN;
INSERT INTO B SELECT * FROM main WHERE id > 10 AND (SELECT count(*) FROM v2 GROUP BY main.id);
SELECT nosuch FROM main;
SELECT MAIN.NOSUCH FROM main;
SELECT ID9 FROM Main;
-- case: holdout/string-functions/363-sqlite-master-keeps-the-case-of-names-and-the-quot
CREATE TABLE Foo(Bar INTEGER, BazQux TEXT);
CREATE INDEX MyIdx ON Foo(Bar);
CREATE VIEW MyView AS SELECT Bar FROM Foo;
CREATE TRIGGER MyTrig AFTER INSERT ON Foo BEGIN SELECT 1; END;
SELECT type, name, tbl_name FROM sqlite_master ORDER BY type, name;
SELECT count(*) FROM sqlite_master WHERE name = 'Foo';
CREATE TABLE "My Tbl"(a);
SELECT name, sql FROM sqlite_master WHERE name = 'My Tbl';
-- case: holdout/string-functions/420-like-with-a-literal-pattern-and-a-column-or-subque
CREATE TABLE t(s TEXT, p TEXT, e TEXT);
CREATE TABLE u(id INTEGER PRIMARY KEY, s TEXT);
CREATE TABLE c(esc TEXT);
INSERT INTO t VALUES('a','a','#');
INSERT INTO u VALUES(1,'a%'),(2,'ax'),(3,'b');
INSERT INTO c VALUES('#');
SELECT s LIKE 'a' ESCAPE e FROM t;
SELECT id FROM u WHERE s NOT LIKE 'a#%' ESCAPE (SELECT esc FROM c) ORDER BY id;
DELETE FROM u WHERE s NOT LIKE 'a#%' ESCAPE (SELECT esc FROM c);
SELECT count(*) FROM u;
-- case: holdout/string-functions/450-a-text-value-holding-invalid-utf-8-can-be-stored-a
CREATE TABLE u(x TEXT); INSERT INTO u VALUES('a'),('b');
CREATE TABLE t(x TEXT); INSERT INTO t VALUES(CAST(x'ff' AS TEXT));
SELECT count(*) FROM u;
SELECT hex(x), length(x), typeof(x), quote(x) FROM t;
PRAGMA integrity_check;
-- case: holdout/string-functions/497-a-user-table-named-like-a-pragma-table-valued-func
CREATE TABLE pragma_table_info(x);
CREATE TEMP TABLE pragma_database_list(v);
INSERT INTO pragma_database_list VALUES (2);
SELECT * FROM pragma_database_list;
SELECT * FROM pragma_table_info;
-- case: holdout/string-functions/546-nested-logical-expression-with-ifnull-not-not-and-
CREATE TABLE t(x INTEGER, z INTEGER);
INSERT INTO t VALUES (3, 1), (NULL, 2), (5, NULL), (0, 0);
SELECT x, z, ifnull(CAST(( NOT NOT 0 ) AS NUMERIC) = ( ( z <= x ) || ( 22679052560859 / x ) ) = ( ( z IS NOT 229358971 ) ), 1.5 = -2.0) FROM t ORDER BY 1, 2;
SELECT NOT NOT 0, NOT NOT 5, NOT NULL, (1 <= 2) || 7, 22679052560859 / 0, 22679052560859 / 0.0, -5 / 2, -5 % 2, 5 % -2;
-- case: holdout/string-functions/557-concat-ws-skips-null-arguments-and-their-separator
SELECT concat_ws('x', 'y', NULL, 'y');
SELECT concat_ws(',', 'a'), concat_ws(',', NULL), concat_ws(NULL, 'a', 'b'), concat_ws(',', 1, 2.5, x'41', NULL, ''), concat('a', NULL, 1, 2.5), concat(NULL);
-- case: holdout/string-functions/558-strict-table-with-explicit-null-constraints
create table if not exists server_nodes (node_id integer primary key autoincrement, node_label text, updated_at text NULL, created_on text NULL) strict;
insert into server_nodes (node_label) values ('ok');
select * from server_nodes;
create table s2(a int NULL NOT NULL) strict;
create table s3("" text) strict;
create table "" (a);
-- case: holdout/string-functions/565-null-in-null-is-null
with t as (select null in (null) as res) select typeof(res), res from t;
select null in (1), null in (), null not in (), 1 in (null), 1 in (null, 1), 1 not in (null), 1 not in (null, 1), 1 not in (null, 2);
-- case: holdout/string-functions/574-printf-d-of-a-real-number-truncates
select printf('%d', 3.9);
select printf('%d', -3.9), printf('%d', '3.9'), printf('%d', 'abc'), printf('%d', NULL), printf('%i', 4.2), printf('%5d|%-5d|%05d', 42, 42, 42), printf('%d', 1e30), printf('%d', 9223372036854775807), printf('%d', 9.3e18);
-- case: holdout/string-functions/580-like-folds-ascii-case-only
SELECT 'A' LIKE 'a', 'Ä' LIKE 'ä', 'a' LIKE 'A', 'É' LIKE 'é';
PRAGMA case_sensitive_like = ON;
SELECT 'A' LIKE 'a', 'a' LIKE 'a';
PRAGMA case_sensitive_like = OFF;
SELECT 'A' LIKE 'a', 'a' GLOB 'A', 'a' GLOB 'a', 'Ä' GLOB 'Ä';
-- case: holdout/string-functions/581-trim-removes-spaces-only-by-default
select quote(trim(char(9) || 'a')), quote(trim(char(10) || 'a')), quote(trim(' ' || 'a ')), quote(ltrim('  a  ')), quote(rtrim('  a  ')), quote(trim('xxaxx', 'x')), quote(trim('abcba', 'ab')), quote(trim('a', '')), quote(trim(NULL)), quote(trim('a', NULL)), quote(trim(x'2061 20')), quote(trim(12 || ' '));
select hex(trim(char(9) || 'a' || char(13))), hex(trim(char(160) || 'a')), hex(trim(' é ', ' é'));
-- case: holdout/string-functions/587-double-quoted-text-is-a-string-literal-when-it-is-
select printf("%d",3.2);
select "abc", "a b", typeof("abc");
create table t(a);
insert into t values (5);
select "a", "b" from t;
select 1 where "x" = 'x';
-- case: holdout/string-functions/605-nulls-are-distinct-in-a-unique-column
create table t(a int, x int unique);
insert into t (a) values (1);
select * from t;
insert into t (a) values (2);
select * from t;
insert into t values (3, NULL);
select count(*), count(x) from t;
create table u(a, b, unique(a, b));
insert into u values (1, null), (1, null), (null, null);
select count(*) from u;
-- case: holdout/string-functions/608-format-is-an-alias-of-printf
SELECT printf('%s', 'hello world');
SELECT format('%s', 'hello world');
SELECT format('%05.1f|%+d|%x|%X|%o|%e|%g|%c|%%|%q|%Q|%w', 3.14159, 5, 255, 255, 8, 12345.678, 0.0001, 65, 'it''s', 'it''s', 'a"b');
SELECT format('%s'), format('%d', 'abc'), format('%z', 1), format(NULL), format('%s %s', 1);
-- case: holdout/string-functions/618-concat-and
create table t (name text);
insert into t values ('AA');
select 'prefix ' || name || ' suffix' from t;
select concat('prefix ', name, ' suffix') from t;
select concat(1, 2.5, 1e20, 0.1, -0.0, 1e-7), 1 || 2.5 || 1e20 || 0.1 || -0.0 || 1e-7, concat('a', NULL, 'b'), 'a' || NULL || 'b', typeof(1 || 2), 1 || 2 + 1;
-- case: holdout/string-functions/626-like-with-a-very-long-pattern
SELECT 'test' LIKE REPLACE(ZEROBLOB(135000), x'00', 'a');
SELECT 'test' LIKE REPLACE(ZEROBLOB(11000), x'00', '%');
SELECT 'test' LIKE REPLACE(ZEROBLOB(50000), x'00', '%') || 't';
SELECT 'test' GLOB REPLACE(ZEROBLOB(50000), x'00', '*');
-- case: holdout/string-functions/635-where-keeps-rows-where-null-or-true-is-true
CREATE TABLE t6(c0 INT, c1 INT);
INSERT INTO t6 VALUES (NULL, 1), (1, 2), (2, 3);
SELECT COUNT(*) FROM t6 WHERE (((t6.c0 NOT IN (t6.c0))) OR (true)) AND ((NOT false));
SELECT SUM(((((t6.c0 NOT IN (t6.c0))) OR (true)) AND ((NOT false))) IS TRUE) FROM t6;
SELECT c0, c0 NOT IN (c0), (c0 NOT IN (c0)) OR 1, (c0 NOT IN (c0)) AND 0, NULL OR 1, NULL AND 0, NOT NULL FROM t6;
-- case: holdout/string-functions/642-text-column-holding-bytes-that-are-not-utf-8
CREATE TABLE t(val TEXT);
INSERT INTO t VALUES(CAST(X'FF' AS TEXT));
SELECT val LIKE '%a%', val GLOB '*a*', hex(val), length(val), typeof(val), val = CAST(X'FF' AS TEXT), val < 'a', instr(val, 'a'), hex(upper(val)), hex(substr(val, 1, 1)) FROM t;
-- case: holdout/string-functions/644-equality-with-null-in-the-where-of-a-table-with-a-
CREATE TABLE t8 (c1 TEXT, c2 TEXT);
INSERT INTO t8 VALUES ('  ', NULL), ('  ', 'a'), (NULL, NULL);
SELECT c2 FROM t8 WHERE c1 = '  ' AND c2 = NULL ORDER BY c2 ASC, rowid ASC;
SELECT c2 FROM t8 WHERE c1 = '  ' AND c2 IS NULL ORDER BY c2 ASC, rowid ASC LIMIT 5;
-- case: holdout/string-functions/651-table-name-containing-quotes-and-sql-text
create table t(x);
insert into t values('test');
begin;
create table "x' and 0 union select 'table','t2','t2',2,'create table t2(z)' --"(a);
insert into t2 values('no');
commit;
drop table t2;
select * from t;
select name from sqlite_schema ORDER BY name;
-- case: holdout/string-functions/676-substr-like-and-glob-on-text-with-embedded-nul-byt
SELECT hex(substr('a' || char(0) || 'b' || char(0) || 'c', 1, 2));
SELECT hex(substr('a' || char(0) || 'b' || char(0) || 'c', 2, 1));
SELECT hex(substr('a' || char(0) || 'b' || char(0) || 'c', 3, 1));
SELECT ('a' || char(0) || 'b') LIKE 'a', ('a' || char(0) || 'b') LIKE 'a%', ('a' || char(0) || 'b') GLOB 'a', ('a' || char(0) || 'b') GLOB 'a*', 'a' LIKE ('a' || char(0) || 'b');
SELECT hex(substr(CAST('a' || char(0) || 'b' AS BLOB), 2, 2)), length('a' || char(0) || 'b'), length(CAST('a' || char(0) || 'b' AS BLOB));
-- case: holdout/string-functions/685-replace-with-an-empty-search-string-and-a-null-rep
select replace('Av__L1 _Ro', '', null), typeof(replace('Av__L1 _Ro', '', null));
select replace('abc', '', 'x'), replace('abc', 'b', NULL), replace(NULL, 'b', 'c'), replace('abc', NULL, 'c'), replace('', '', 'x'), replace('aaa', 'aa', 'b'), replace('abc', 'B', 'x'), replace(123, 2, 9), typeof(replace(123, 2, 9));
-- case: holdout/string-functions/689-sqlite-sequence-seq-edited-to-text-or-to-the-maxim
CREATE TABLE t(a INTEGER PRIMARY KEY AUTOINCREMENT);
INSERT INTO t DEFAULT VALUES;
UPDATE sqlite_sequence SET seq='5abc' WHERE name='t';
INSERT INTO t DEFAULT VALUES;
SELECT a FROM t ORDER BY a;
SELECT typeof(seq), seq FROM sqlite_sequence WHERE name='t';
UPDATE sqlite_sequence SET seq='9223372036854775807' WHERE name='t';
INSERT INTO t DEFAULT VALUES;
-- case: holdout/string-functions/694-sqlite3-prepare-v2-honours-the-length-argument-sho
SELECT 1; SELECT 2
;
SELECT 3;
-- case: holdout/string-functions/701-pragma-table-info-dflt-value-keeps-the-text-of-the
CREATE TABLE t (a INTEGER);
ALTER TABLE t ADD COLUMN c BLOB NOT NULL DEFAULT x'00';
ALTER TABLE t ADD COLUMN d BLOB NOT NULL DEFAULT X'AB';
ALTER TABLE t ADD COLUMN e TEXT DEFAULT ('a' || 'b');
ALTER TABLE t ADD COLUMN f INT DEFAULT -5;
ALTER TABLE t ADD COLUMN g REAL DEFAULT 1.50;
ALTER TABLE t ADD COLUMN h TEXT DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE t ADD COLUMN i TEXT DEFAULT NULL;
ALTER TABLE t ADD COLUMN j INT DEFAULT +7;
ALTER TABLE t ADD COLUMN k INT DEFAULT (1+2);
SELECT name, quote(dflt_value) FROM pragma_table_info('t');
-- case: holdout/string-functions/718-in-list-built-from-many-rows-including-an-empty-te
CREATE TABLE a(i INTEGER PRIMARY KEY, a);
INSERT INTO a WITH RECURSIVE c(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM c WHERE i < 399) SELECT i, i FROM c;
INSERT INTO a VALUES(4000, '');
INSERT INTO a WITH RECURSIVE c(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM c WHERE i < 399) SELECT NULL, printf('%.60d', i) FROM c;
SELECT count(*) FROM a WHERE a IN (SELECT a FROM a);
SELECT count(*) FROM a WHERE a IN (SELECT a FROM a WHERE a = '' OR i < 3);
-- case: holdout/string-functions/728-real-to-text-for-magnitudes-between-1e15-and-1e17
SELECT CAST(1e15 AS TEXT), 1.5e15, 1e16, 1e17, 1e18, 123456789012345.6, 1234567890123456.7, 12345678901234567.0, 1e14, 999999999999999.0;
SELECT CAST(1e15 AS TEXT), CAST(1.5e15 AS TEXT), CAST(1e16 AS TEXT), CAST(1e17 AS TEXT), CAST(1e20 AS TEXT), CAST(-1e15 AS TEXT), CAST(1e-4 AS TEXT), CAST(1e-5 AS TEXT), CAST(0.00012345 AS TEXT), CAST(100.0 AS TEXT), CAST(1.0e21 AS TEXT), CAST(9007199254740993.0 AS TEXT), CAST(0.1+0.2 AS TEXT), CAST(1.0/3 AS TEXT), CAST(2.0/3 AS TEXT), CAST(1e300*10 AS TEXT), CAST(5e-324 AS TEXT);
SELECT 1e15 || '', 1.5e15 || '', 1e16 || '', 3.0 || '', -0.0 || '', 0.0 || '';
-- case: holdout/string-functions/747-check-a-is-null-b-is-null-and-the-precedence-of-po
CREATE TABLE runs (id INTEGER PRIMARY KEY, t_end INTEGER, outcome TEXT, CHECK ((t_end IS NULL) = (outcome IS NULL))) STRICT;
INSERT INTO runs (id, t_end, outcome) VALUES (1, 200, 'ok');
INSERT INTO runs (id, t_end, outcome) VALUES (2, NULL, NULL);
INSERT INTO runs (id, t_end, outcome) VALUES (3, 5, NULL);
SELECT id FROM runs ORDER BY id;
SELECT sql FROM sqlite_master WHERE name = 'runs';
SELECT 1 IS NULL = 0, 1 IS NOT NULL = 1, NULL IS NULL = 1, 1 NOT NULL = 1, 1 NOTNULL, 5 ISNULL = 0;
-- case: holdout/string-functions/765-strict-integer-accepts-integer-text-and-exact-real
CREATE TABLE t(a INTEGER) STRICT;
INSERT INTO t VALUES ('12');
INSERT INTO t VALUES (3.0);
INSERT INTO t VALUES (' 4 ');
INSERT INTO t VALUES ('5x');
INSERT INTO t VALUES (3.5);
INSERT INTO t VALUES ('3.0');
INSERT INTO t VALUES (X'31');
INSERT INTO t VALUES (NULL);
INSERT INTO t VALUES ('0x10');
INSERT INTO t VALUES (1e20);
SELECT quote(a), typeof(a) FROM t ORDER BY rowid;
-- case: holdout/string-functions/766-strict-real-accepts-numeric-text-and-integers
CREATE TABLE t(a REAL) STRICT;
INSERT INTO t VALUES ('1.5');
INSERT INTO t VALUES (2);
INSERT INTO t VALUES ('3');
INSERT INTO t VALUES ('abc');
INSERT INTO t VALUES (X'31');
INSERT INTO t VALUES ('1e2');
SELECT quote(a), typeof(a) FROM t ORDER BY rowid;
-- case: holdout/string-functions/767-strict-text-accepts-integers-and-reals-as-text
CREATE TABLE t(a TEXT) STRICT;
INSERT INTO t VALUES (1);
INSERT INTO t VALUES (1.5);
INSERT INTO t VALUES (X'31');
INSERT INTO t VALUES ('s');
UPDATE t SET a = 42 WHERE a = 's';
SELECT quote(a), typeof(a) FROM t ORDER BY rowid;
CREATE TABLE u(a BLOB) STRICT;
INSERT INTO u VALUES ('x');
INSERT INTO u VALUES (1);
INSERT INTO u VALUES (X'01');
SELECT quote(a), typeof(a) FROM u;
-- case: holdout/string-functions/778-the-flag-in-printf-and-format
SELECT printf('%!.15g', 3.0), printf('%!.20e', 1.0), printf('%!f', 2.5), printf('%!.3f', 2), printf('%.15g', 3.0), printf('%!.15g', 0.1), printf('%!d', 5), printf('%!s', 'x');
SELECT format('%!.15g', 3.0), format('%!.17g', 0.1), printf('%!.0f', 2.5), printf('%.0f', 2.5), printf('%.0f', 3.5);
-- case: holdout/string-functions/787-likelihood-likely-and-unlikely-validate-their-argu
SELECT likelihood(1, 2);
SELECT likelihood(1, 0.5), likelihood(1, 0), likelihood(1, 1), likelihood(1, -0.1);
CREATE TABLE t(p);
INSERT INTO t VALUES (0.5);
SELECT likelihood(1, p) FROM t;
SELECT likely(7), unlikely(7), likelihood('a', 0.9), typeof(likely(NULL));
-- case: holdout/string-functions/815-printf-g-and-f-of-negative-zero
SELECT printf('%g', -0.0), format('%g', -0.0), printf('%f', -0.0), printf('%e', -0.0), printf('%d', -0.0), printf('%.1f', -0.04), printf('%g', 0.0), -0.0, quote(-0.0), CAST(-0.0 AS TEXT), -0.0 || '', printf('%s', -0.0), json_array(-0.0), typeof(-0.0), -0.0 = 0.0;
