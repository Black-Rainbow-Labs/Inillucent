-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/type-affinity/1-offset-on-integer-primary-key-with-mixed-literals-
create table temp (t1 integer, primary key (t1));
insert into temp values (1),(2.0),('3'),('4.0');
select * from temp limit 1 offset 1;
select t1, typeof(t1) from temp;
-- case: holdout/type-affinity/4-seeks-on-integer-primary-key-with-real-and-text-op
create table t(a INTEGER PRIMARY KEY);
insert into t values (1), (2);
select 'a', * from t where a = 2.000000;
select 'b', * from t where a = 2.000001;
select 'c', * from t where a = '2.00000';
select 'd', * from t where a > 1.999999;
select 'e', * from t where a < 1.000001;
-- case: holdout/type-affinity/19-autoindexes-on-real-unique-and-text-primary-key-pa
create table t(a REAL UNIQUE, b TEXT PRIMARY KEY);
insert into t values (1, 'a');
pragma integrity_check;
select quote(a), typeof(a) from t;
-- case: holdout/type-affinity/23-in-list-elements-take-the-left-column-s-affinity-t
create table t(a text);
insert into t values ('1');
select * from t where a = 1;
select * from t where a in (1);
select * from t where a in (1, 2.0);
-- case: holdout/type-affinity/24-rowid-compared-with-a-text-literal-turso-returned-
create table t(a integer);
insert into t(rowid, a) values (1, 1);
select * from t where rowid = '1';
select * from t where a = '1';
-- case: holdout/type-affinity/26-integer-affinity-converts-2e0-to-integer-when-loss
create table s(a integer);
insert into s values ('2e0'), ('2.5e0'), (' 3 '), ('0x10'), ('1e400');
select typeof(a), quote(a) from s;
-- case: holdout/type-affinity/28-range-scan-on-an-indexed-integer-column-with-text-
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES (10),(2),('2'),(7);
CREATE INDEX t_a ON t(a);
SELECT a FROM t WHERE a >= '2' AND a <= '10' ORDER BY a;
-- case: holdout/type-affinity/46-nan-and-inf-text-stay-text-in-an-integer-column
CREATE TABLE t(c INTEGER);
INSERT INTO t VALUES ('nan'), ('inf'), ('-inf'), ('Infinity'), ('1e999');
SELECT typeof(c), quote(c) FROM t;
CREATE TABLE r(c REAL);
INSERT INTO r VALUES ('nan'), ('inf'), ('1e999'), (1e999), (-1e999);
SELECT typeof(c), quote(c) FROM r;
-- case: holdout/type-affinity/48-typeof-sum-cast-text-as-blob-is-real-turso-said-in
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
INSERT INTO t VALUES(1, '10'), (2, '20'), (3, '30');
SELECT typeof(SUM(CAST(val AS BLOB))), SUM(CAST(val AS BLOB)) FROM t;
SELECT typeof(SUM(val)), SUM(val), typeof(TOTAL(val)), typeof(AVG(val)) FROM t;
-- case: holdout/type-affinity/63-check-val-5-on-a-text-column-compares-text-with-an
CREATE TABLE t(val TEXT CHECK(val > 5));
INSERT INTO t VALUES ('10');
SELECT * FROM t;
-- case: holdout/type-affinity/82-cast-and-substr-with-an-integer-literal-beyond-64-
SELECT substr('abcdefghijklmnopqrstuvwxyz', 18446744073709551488);
SELECT CAST(18446744073709551488 AS INTEGER);
SELECT 18446744073709551488, typeof(18446744073709551488);
SELECT CAST('18446744073709551488' AS INTEGER), CAST(1e30 AS INTEGER), CAST(-1e30 AS INTEGER), CAST('9223372036854775808' AS INTEGER);
-- case: holdout/type-affinity/83-int64-max-compared-against-an-integer-overflow-tha
CREATE TABLE v0 (v1 INTEGER PRIMARY KEY);
INSERT INTO v0 VALUES (9223372036854775807);
SELECT * FROM v0 WHERE v1 >= (9223372036854775807 + 1);
SELECT v1 >= (9223372036854775807 + 1) FROM v0;
SELECT 9223372036854775807 + 1, 9223372036854775807 = 9223372036854775808.0, 9223372036854775807 < 9223372036854775808.0;
-- case: holdout/type-affinity/105-column-declared-with-type-any-in-a-non-strict-tabl
CREATE TABLE t ( c ANY );
INSERT INTO t VALUES ('42'), ('4x'), (4.0);
SELECT typeof(c), quote(c) FROM t;
-- case: holdout/type-affinity/149-blob-bytes-are-kept-by
SELECT hex(x'AB' || 'text'), hex('text' || x'AB'), hex(printf('%s', x'AB')), hex(REPLACE(X'010203040503', X'03', X'FF'));
SELECT typeof(x'AB' || 'text'), typeof(x'AB' || x'CD'), typeof(1 || x'CD'), length(x'AB' || 'text');
SELECT hex(substr(x'0102030405', 2, 3)), hex(lower(x'4142')), typeof(lower(x'4142')), typeof(trim(x'20412020')), hex(trim(x'20412020'));
-- case: holdout/type-affinity/234-in-compound-select-applies-the-left-column-s-text-
CREATE TABLE t1(a TEXT);
INSERT INTO t1 VALUES('1'),('2'),('3');
SELECT a FROM t1 WHERE a IN (SELECT 1 UNION SELECT 2);
SELECT a FROM t1 WHERE a IN (SELECT 1);
SELECT a FROM t1 WHERE a IN (1, 2);
SELECT a FROM t1 WHERE a IN (SELECT 1 UNION ALL SELECT 2);
-- case: holdout/type-affinity/245-integer-column-in-compound-select-of-text
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES(1),(2),(3);
SELECT * FROM t WHERE a IN (SELECT '1' UNION SELECT '2');
SELECT * FROM t WHERE a IN (SELECT '1');
SELECT * FROM t WHERE a IN ('1', '2');
-- case: holdout/type-affinity/251-cast-blob-as-text-keeps-invalid-utf-8-bytes
SELECT typeof(CAST(x'CAFE' AS TEXT)), hex(CAST(x'CAFE' AS TEXT)), length(CAST(x'CAFE' AS TEXT)), length(CAST(x'C3A9' AS TEXT));
SELECT hex(CAST(x'61FF62' AS TEXT) || 'x'), upper(CAST(x'61FF62' AS TEXT)) = CAST(x'41FF42' AS TEXT);
SELECT hex(CAST('é' AS BLOB)), CAST(x'6162' AS TEXT), quote(CAST(x'6162' AS TEXT));
-- case: holdout/type-affinity/266-index-seek-on-a-real-column-with-integer-literals-
CREATE TABLE t (f REAL);
CREATE INDEX t_f ON t(f);
INSERT INTO t VALUES (-2701729208700874036);
SELECT count(*) FROM t WHERE f = -2701729208700874000;
SELECT count(*) FROM t WHERE f = -2701729208700874036;
SELECT count(*) FROM t WHERE f = -2701729208700874240;
SELECT count(*) FROM t NOT INDEXED WHERE f = -2701729208700874000;
SELECT quote(f) FROM t;
-- case: holdout/type-affinity/350-case-x-when-y-applies-comparison-affinity
CREATE TABLE o(t TEXT, n INTEGER, r REAL);
INSERT INTO o VALUES ('5', 5, 5.0);
SELECT CASE o.t WHEN 5 THEN 'y' ELSE 'n' END FROM o;
SELECT CASE o.n WHEN '5' THEN 'y' ELSE 'n' END FROM o;
SELECT CASE o.r WHEN '5' THEN 'y' ELSE 'n' END FROM o;
SELECT CASE 5 WHEN o.t THEN 'y' ELSE 'n' END FROM o;
SELECT CASE '5' WHEN o.n THEN 'y' ELSE 'n' END FROM o;
SELECT CASE 5 WHEN '5' THEN 'y' ELSE 'n' END;
SELECT CASE o.t WHEN o.n THEN 'y' ELSE 'n' END FROM o;
-- case: holdout/type-affinity/371-compound-select-with-a-cast-as-real-arm-keeps-exac
CREATE TABLE t(i INTEGER);
INSERT INTO t VALUES(9007199254740993);
CREATE TABLE probe AS SELECT v FROM (SELECT CAST(i AS REAL) AS v FROM t UNION ALL SELECT i FROM t);
SELECT typeof(v), quote(v) FROM probe;
SELECT type FROM pragma_table_info('probe');
SELECT typeof(v), quote(v) FROM (SELECT CAST(i AS REAL) AS v FROM t UNION ALL SELECT i FROM t);
-- case: holdout/type-affinity/375-cast-of-integer-text-beyond-64-bits-to-numeric
SELECT CAST('9223372036854775808' AS NUMERIC);
SELECT printf('%!.20e', CAST('9223372036854775808' AS NUMERIC));
SELECT CAST('9223372036854775808' AS NUMERIC) = 9223372036854775808.0;
SELECT printf('%!.20e', CAST('9223372036854775808.0' AS NUMERIC));
SELECT typeof(CAST('9223372036854775807' AS NUMERIC)), typeof(CAST('9223372036854775808' AS NUMERIC)), typeof(CAST('1e2' AS NUMERIC)), typeof(CAST('1.0' AS NUMERIC)), typeof(CAST('12abc' AS NUMERIC));
-- case: holdout/type-affinity/376-vertical-tab-0x0b-is-leading-whitespace-for-numeri
SELECT CAST(char(11)||'12' AS NUMERIC);
CREATE TABLE u(x INTEGER);
INSERT INTO u VALUES(char(11)||'12');
SELECT typeof(x), x = 12 FROM u;
CREATE TABLE t(x INTEGER PRIMARY KEY);
INSERT INTO t VALUES(char(11)||'12');
SELECT count(*) FROM t;
SELECT CAST(char(11)||'12' AS INTEGER), CAST(char(11)||'12' AS REAL), char(11)||'12' + 0, CAST(char(12)||'12' AS NUMERIC), CAST(char(160)||'12' AS NUMERIC);
-- case: holdout/type-affinity/396-union-all-of-a-real-column-and-an-integer-column-k
CREATE TABLE p(amount REAL);
CREATE TABLE q(amount INTEGER);
INSERT INTO q VALUES(7);
CREATE VIEW v AS SELECT amount FROM p UNION ALL SELECT amount FROM q;
SELECT typeof(amount), amount/2 FROM v;
SELECT type FROM pragma_table_info('v');
CREATE TABLE aud(x);
INSERT INTO aud SELECT amount FROM v;
SELECT typeof(x), x FROM aud;
SELECT typeof(amount) FROM (SELECT amount FROM q UNION ALL SELECT amount FROM p);
-- case: holdout/type-affinity/428-in-over-a-compound-select-with-except-applies-the-
CREATE TABLE t(a INTEGER);
INSERT INTO t VALUES(1),(2),(3);
SELECT quote(a IN (SELECT '1' EXCEPT SELECT '9')) FROM t WHERE a=1;
DELETE FROM t WHERE a IN (SELECT '1' EXCEPT SELECT '9');
SELECT group_concat(a) FROM t;
SELECT quote(a IN (SELECT '2' UNION SELECT '9')), quote(a IN (SELECT '2' INTERSECT SELECT '2')), quote(a IN (SELECT '2' UNION ALL SELECT '9')) FROM t WHERE a=2;
-- case: holdout/type-affinity/433-a-scalar-subquery-keeps-the-affinity-of-the-column
CREATE TABLE t(x TEXT);
INSERT INTO t VALUES('20');
SELECT * FROM (SELECT (SELECT x FROM t) AS x) WHERE x = 20;
CREATE TABLE u(k INTEGER);
INSERT INTO u VALUES(1);
SELECT (SELECT k FROM u) IN (SELECT '1'), k IN (SELECT '1') FROM u;
SELECT * FROM (SELECT x FROM t) WHERE x = 20;
SELECT typeof(x) FROM (SELECT (SELECT x FROM t) AS x);
SELECT (SELECT x FROM t) = 20, (SELECT k FROM u) = '1';
-- case: holdout/type-affinity/454-in-list-on-an-indexed-real-column-with-an-integer-
CREATE TABLE t(x REAL);
INSERT INTO t VALUES(9007199254740992);
CREATE INDEX i ON t(x);
SELECT count(*) FROM t WHERE x IN (9007199254740993);
SELECT (SELECT count(*) FROM t WHERE x IN (9007199254740993)), (SELECT count(*) FROM t WHERE x NOT IN (9007199254740993));
DELETE FROM t WHERE x IN (9007199254740993);
SELECT count(*) FROM t;
-- case: holdout/type-affinity/455-union-all-of-a-text-column-and-an-integer-column-k
CREATE TABLE t(x TEXT);
CREATE TABLE u(x TEXT);
CREATE TABLE v(y INTEGER);
INSERT INTO t VALUES('1'),('2');
INSERT INTO u VALUES('1'),('2');
INSERT INTO v VALUES(9);
SELECT group_concat(x) FROM (SELECT x FROM t UNION ALL SELECT y FROM v) WHERE x > 7;
DELETE FROM u WHERE x IN (SELECT x FROM (SELECT x FROM t UNION ALL SELECT y FROM v) WHERE x > 7);
SELECT count(*) FROM u;
SELECT group_concat(y) FROM (SELECT y FROM v UNION ALL SELECT x FROM t) WHERE y > 7;
-- case: holdout/type-affinity/461-values-arm-in-a-compound-select-keeps-the-first-ar
CREATE TABLE t(x TEXT, n INTEGER);
CREATE TABLE u(x TEXT);
INSERT INTO t VALUES('7',7),('9',8);
INSERT INTO u VALUES('7'),('9');
CREATE VIEW v AS SELECT x FROM t UNION ALL VALUES(NULL);
SELECT count(*) FROM v WHERE x = 7;
SELECT count(*) FROM u JOIN (SELECT n FROM t UNION VALUES(1),(2)) w ON u.x = w.n;
DELETE FROM u WHERE x IN (SELECT x FROM v WHERE x = 7);
SELECT group_concat(x) FROM u;
-- case: holdout/type-affinity/531-substr-of-a-zero-length-blob-is-null
SELECT quote(substr(X'', 1, 10)), quote(substr(zeroblob(0), 1)), quote(substr('', 1)), quote(substr(X'01', 2)), quote(substr(X'0102', 1, 0));
CREATE TABLE t(c);
CREATE INDEX i ON t(substr(c, 1, 10));
INSERT INTO t VALUES (zeroblob(0));
PRAGMA integrity_check;
SELECT quote(substr(c, 1, 10)) FROM t;
-- case: holdout/type-affinity/542-like-on-an-integer-column-holding-integers-and-num
CREATE TABLE alluring_simons (rousing_veidaux INTEGER);
INSERT INTO alluring_simons VALUES ('-4750696213641675957'), ('-5458087213939326485'), (-3919859408639645563), ('5557375199733233444'), ('8012037637129400302');
SELECT count(*) FROM alluring_simons WHERE rousing_veidaux LIKE '%506%6213%6%75957';
SELECT rousing_veidaux FROM alluring_simons WHERE rousing_veidaux > '-3moocfczqdkdrhzfpjj' ORDER BY 1;
SELECT typeof(rousing_veidaux), rousing_veidaux LIKE '-%' FROM alluring_simons ORDER BY 2, 1;
SELECT 12 LIKE '1%', 1.5 LIKE '1.%', x'31' LIKE '1', NULL LIKE 'a', 'a' LIKE NULL;
-- case: holdout/type-affinity/552-limit-with-an-order-by-over-a-mix-of-blob-text-int
CREATE TABLE s (a BLOB, b BLOB, c TEXT, d INTEGER, e TEXT, f REAL);
INSERT INTO s VALUES (X'66616E', X'736C', 'vivid', -665397153462129293, 'zestful', 5925117379.349501), (X'6672', X'6769', 'magnificent', 2714481991574858737, 'resourceful', 9081030660.57735), (X'6162', NULL, NULL, NULL, NULL, NULL);
SELECT * FROM s ORDER BY f DESC LIMIT 2;
SELECT * FROM s ORDER BY a LIMIT 1 OFFSET 1;
SELECT quote(a), quote(f) FROM s ORDER BY c, d LIMIT 3;
-- case: holdout/type-affinity/553-min-over-an-integer-column-that-stored-9-00-keeps-
CREATE TABLE t(x INTEGER);
INSERT INTO t VALUES (9.00), ('xthjqehemf'), ('espkyehmgw'), (NULL), ('qyetebaddr');
SELECT min(x), typeof(min(x)), max(x), typeof(max(x)) FROM t;
SELECT x, typeof(x) FROM t ORDER BY rowid;
-- case: holdout/type-affinity/554-min-over-a-text-column-holding-numbers-stored-as-t
CREATE TABLE t(x TEXT);
INSERT INTO t VALUES ('fuaixsnyyv'), (-3.90), ('shpdhpllah'), (-611), (199);
SELECT min(x), typeof(min(x)), max(x), typeof(max(x)) FROM t;
SELECT x, typeof(x) FROM t ORDER BY rowid;
-- case: holdout/type-affinity/556-comparison-of-a-concatenation-result-with-cast-0-a
SELECT (104614899632619 || 45597) > CAST(0 AS NUMERIC);
SELECT typeof(104614899632619 || 45597), 10461489963261945597 > 0, '10461489963261945597' > 0, '10461489963261945597' > CAST(0 AS NUMERIC), '9' > 10, 9 > '10', '9' > '10';
-- case: holdout/type-affinity/562-lower-of-a-blob-with-bytes-that-are-not-valid-utf-
select hex(lower(x'ffff'));
select hex(upper(x'ffe1')), hex(lower(x'41ff')), typeof(lower(x'ffff')), hex(lower(CAST(x'c3' AS TEXT))), length(x'ffff'), length(CAST(x'ffff' AS TEXT));
-- case: holdout/type-affinity/568-sum-of-numeric-text-values
create table t(a);
insert into t values ('1'), ('2');
select sum(a), typeof(sum(a)), total(a), avg(a) from t;
create table u(a);
insert into u values ('1.5'), ('abc'), (' 3 '), ('0x10'), ('1e1');
select sum(a), typeof(sum(a)), total(a), avg(a) from u;
select sum(a) from (select '5' a union all select 2);
-- case: holdout/type-affinity/573-substr-of-a-blob-returns-a-blob
with t as (select hex(substr(x'414243',2,1)) as slice) select slice, typeof(slice), length(slice) from t;
select typeof(substr(x'414243',2,1)), quote(substr(x'414243',2)), quote(substr(x'414243',-1)), quote(substr(x'414243',0,2)), quote(substr(x'414243',5)), typeof(substr(x'414243',5));
-- case: holdout/type-affinity/599-text-with-a-trailing-non-breaking-space-is-not-con
CREATE TABLE nb1(i INTEGER);
INSERT INTO nb1 VALUES ('12' || CHAR(160));
INSERT INTO nb1 VALUES (CHAR(160) || '12');
INSERT INTO nb1 VALUES ('12' || CHAR(9));
INSERT INTO nb1 VALUES ('12' || CHAR(32));
INSERT INTO nb1 VALUES ('12' || CHAR(10) || CHAR(13) || CHAR(12) || CHAR(11));
SELECT TYPEOF(i), LENGTH(i), quote(i) FROM nb1 ORDER BY rowid;
-- case: holdout/type-affinity/600-index-range-scan-on-a-text-column-compared-with-an
create table t(a text);
insert into t values ('10'), ('2'), ('02'), ('2a');
select * from t where a >= 2 order by rowid;
create index idx on t(a);
select * from t where a >= 2 order by rowid;
select * from t where a = 2;
select * from t where a < 2 order by a;
select * from t where a between 2 and 5 order by a;
-- case: holdout/type-affinity/601-default-true-and-default-false-store-integers-and-
CREATE TABLE _schema_migrations (id INTEGER PRIMARY KEY AUTOINCREMENT, file VARCHAR(255), migrated BOOLEAN DEFAULT false, migrated2 BOOLEAN DEFAULT true);
INSERT INTO _schema_migrations (file) VALUES ('s');
INSERT INTO _schema_migrations (file, migrated, migrated2) VALUES ('s', true, false);
SELECT migrated, typeof(migrated), migrated2, typeof(migrated2) FROM _schema_migrations;
SELECT true, false, TRUE, typeof(true), true + 1, true = 1, 'true' = true, NOT true;
-- case: holdout/type-affinity/613-blob-affinity-columns-keep-values-as-written-and-t
CREATE TABLE b (g BLOB, r REAL, u BLOB, i INTEGER, p BLOB);
INSERT INTO b VALUES (X'696E63', -7768386991.4653015, X'636170', 387955672898174589, X'63');
INSERT INTO b VALUES ('text', '12', 5, '7', 1.5);
SELECT typeof(g), typeof(r), typeof(u), typeof(i), typeof(p) FROM b ORDER BY rowid;
SELECT quote(g), quote(r), quote(u), quote(i), quote(p) FROM b ORDER BY rowid;
-- case: holdout/type-affinity/617-cast-of-numeric-text-at-the-64-bit-boundaries-with
SELECT CAST ('9223372036854775808' AS INTEGER);
SELECT CAST ('9223372036854775808' AS NUMERIC);
SELECT CAST ('9223372036854775807' AS NUMERIC);
SELECT CAST ('-9223372036854775809' AS NUMERIC);
SELECT CAST ('-9223372036854775808' AS NUMERIC), CAST ('-9223372036854775809' AS INTEGER), CAST ('99999999999999999999' AS INTEGER), CAST ('1e19' AS INTEGER), CAST ('1e19' AS NUMERIC), CAST (1e19 AS INTEGER), CAST (-1e19 AS INTEGER), CAST (9.2233720368547758e18 AS INTEGER);
SELECT CAST("9223372036854775808" AS INTEGER);
-- case: holdout/type-affinity/620-cast-without-a-type
SELECT CAST(1 AS);
SELECT CAST(1 AS UNKNOWNTYPE), typeof(CAST(1 AS UNKNOWNTYPE)), CAST('abc' AS FLOAT), CAST(1.9 AS INT), CAST('5' AS BOOLEAN), typeof(CAST(1 AS BOOLEAN)), CAST(1 AS VARCHAR(2)), typeof(CAST(1 AS VARCHAR(2)));
SELECT CAST(1 AS DOUBLE PRECISION), CAST(1 AS UNSIGNED BIG INT), typeof(CAST(1 AS CHARACTER(5))), typeof(CAST(1 AS NVARCHAR)), typeof(CAST(1 AS DECIMAL(5,2)));
-- case: holdout/type-affinity/630-the
SELECT hex(X'ABCD' || X'1234'), typeof(X'ABCD' || X'1234');
SELECT typeof(X'ABCD' || 'a'), typeof(1 || 2), typeof(NULL || X'AB'), typeof(X'' || X''), length(X'ABCD' || X'1234'), quote(X'41' || X'42');
-- case: holdout/type-affinity/632-any-column-type-in-a-table-that-is-not-strict
CREATE TABLE t1(c1 ANY);
INSERT INTO t1 VALUES ('1'), ('1.5'), ('1a');
SELECT c1, typeof(c1) FROM t1 ORDER BY rowid;
CREATE TABLE t2(c1 ANY) STRICT;
INSERT INTO t2 VALUES ('1'), ('1.5'), ('1a'), (1), (x'01'), (NULL);
SELECT quote(c1), typeof(c1) FROM t2 ORDER BY rowid;
-- case: holdout/type-affinity/633-comparing-text-and-integer-or-real-values-of-the-s
SELECT x, x = 2 FROM (SELECT CAST(2 AS TEXT) AS x UNION ALL SELECT 2.0);
SELECT '2' = 2, '2.0' = 2, 2.0 = 2, '2' = 2.0, CAST(2 AS TEXT) = 2, CAST(2 AS TEXT) = 2.0, '2' < 10, '10' < 9;
-- case: holdout/type-affinity/664-index-seeks-on-an-integer-column-with-text-operand
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER);
CREATE INDEX idx_val ON t(val);
INSERT INTO t VALUES (1, 42), (2, 5), (3, 100);
SELECT * FROM t WHERE val = '42';
SELECT * FROM t WHERE val > '5' ORDER BY val;
SELECT * FROM t WHERE val IN ('42', '100') ORDER BY val;
SELECT * FROM t WHERE val BETWEEN '10' AND '50' ORDER BY val;
SELECT * FROM t WHERE val = '42abc';
SELECT * FROM t WHERE val < ' 50' ORDER BY val;
-- case: holdout/type-affinity/668-blob-concatenation-produces-text-that-compares-equ
SELECT typeof(X'41' || X'42');
SELECT X'41' || X'42' = 'AB';
SELECT 'AB' = X'41' || X'42', X'4142' = 'AB', X'4142' = X'4142', X'41' < 'A', 'A' < X'41';
-- case: holdout/type-affinity/671-cast-integer-as-real-integer-beyond-2-53
SELECT CAST(9007199254740993 AS REAL) = 9007199254740993;
SELECT CAST(9007199254740993 AS REAL) != 9007199254740993;
SELECT CAST(9007199254740993 AS REAL) < 9007199254740993;
SELECT CAST(9007199254740993 AS REAL) > 9007199254740993;
CREATE TABLE t1 (id INTEGER PRIMARY KEY, val INTEGER);
INSERT INTO t1 VALUES (1, 9007199254740993), (2, 9007199254740994), (3, 9007199254740992);
SELECT id, val FROM t1 WHERE CAST(val AS REAL) = 9007199254740993;
SELECT id, val FROM t1 WHERE CAST(val AS REAL) = 9007199254740992 ORDER BY id;
SELECT id FROM t1 WHERE val = 9007199254740993.0;
-- case: holdout/type-affinity/675-
SELECT TYPEOF(X'DEAD' || X'BEEF');
SELECT TYPEOF(X'41' || X'42');
SELECT HEX(X'DEAD' || X'BEEF');
CREATE TABLE type_check(id INTEGER PRIMARY KEY, val);
INSERT INTO type_check VALUES(1, X'DEAD' || X'BEEF');
SELECT id, TYPEOF(val), HEX(val) FROM type_check;
SELECT HEX('hello' || X'BEEF'), HEX(X'DE' || 'hello'), typeof(X'DE' || 'hello'), typeof(42 || X'DEAD'), typeof(X'' || X''), typeof(CAST(42 AS BLOB) || X'DE');
-- case: holdout/type-affinity/682-cast-of-text-with-a-zero-fraction-to-numeric-retur
SELECT CAST('3.0' AS NUMERIC), typeof(CAST('3.0' AS NUMERIC));
SELECT typeof(CAST('3.5' AS NUMERIC)), typeof(CAST(3.0 AS NUMERIC)), typeof(CAST('3.0e0' AS NUMERIC)), typeof(CAST('3e0' AS NUMERIC)), typeof(CAST('9.3e18' AS NUMERIC)), typeof(CAST(' 3 ' AS NUMERIC)), CAST('3.0' AS INTEGER), typeof(CAST(NULL AS NUMERIC)), CAST('-0' AS NUMERIC), CAST('-0.0' AS NUMERIC), CAST('+5' AS NUMERIC), CAST('1_0' AS NUMERIC);
-- case: holdout/type-affinity/695-zeroblob-with-a-text-argument-that-has-a-numeric-p
CREATE TABLE out(v);
INSERT INTO out VALUES(zeroblob('3.9suffix'));
PRAGMA integrity_check;
SELECT typeof(v), quote(v), length(v), hex(v) FROM out;
SELECT quote(zeroblob(2.9)), quote(zeroblob('2')), quote(zeroblob(-1)), quote(zeroblob(NULL)), quote(zeroblob('abc')), quote(zeroblob(1e0));
-- case: holdout/type-affinity/772-check-is-evaluated-after-column-affinity-is-applie
CREATE TABLE t(a TEXT CHECK(typeof(a)='text'));
INSERT INTO t VALUES (1);
INSERT INTO t VALUES ('x');
UPDATE t SET a = 2 WHERE a = 'x';
SELECT quote(a) FROM t ORDER BY rowid;
CREATE TABLE u(a INTEGER CHECK(typeof(a)='integer'));
INSERT INTO u VALUES ('7');
INSERT INTO u VALUES ('7.5');
SELECT quote(a) FROM u;
-- case: holdout/type-affinity/785-cast-of-an-invalid-utf-8-blob-to-text-keeps-the-by
SELECT hex(CAST(X'80' AS TEXT)), hex(CAST(X'C3' AS TEXT)), hex(CAST(X'FFFE' AS TEXT)), hex(CAST(X'6180' AS TEXT) || 'b'), length(CAST(X'80' AS TEXT)), length(CAST(X'E282' AS TEXT));
