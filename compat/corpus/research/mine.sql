-- Hand written probes of limits, numbers, the planner, collations, joins, schema changes, triggers and pragmas.
-- Gathered by the bug hunt of October 2026; see tasks/task-2201-bug-hunt-tdd.md.
-- Run every night by nightly::research_corpus against the pinned SQLite.

-- case: research/mine/deep-parens-500
SELECT ((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));

-- case: research/mine/deep-unary-minus
SELECT - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - 1;

-- case: research/mine/deep-nested-subquery-60
SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT (SELECT 1))))))))))))))))))))))))))))));

-- case: research/mine/recursive-cte-deep-concat
WITH RECURSIVE c(i,s) AS (SELECT 1,'' UNION ALL SELECT i+1, s||'x' FROM c WHERE i<3000) SELECT max(i), length(max(s)) FROM c;

-- case: research/mine/recursive-cte-limit-stops
WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c) SELECT count(*) FROM (SELECT i FROM c LIMIT 100000);

-- case: research/mine/recursive-cte-union-dedups
WITH RECURSIVE c(i) AS (SELECT 1 UNION SELECT (i+1)%5 FROM c) SELECT i FROM c ORDER BY i;

-- case: research/mine/many-columns-select
WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c WHERE i<10) SELECT i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i,i FROM c WHERE i=10;

-- case: research/mine/long-string-repeat
SELECT length(replace(hex(zeroblob(500000)),'0','ab'));

-- case: research/mine/long-in-list-literal
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c WHERE i<2000) INSERT INTO t SELECT i, i*2 FROM c;
SELECT count(*), sum(b) FROM t WHERE a IN (1,2,3,5,8,13,21,34,55,89,144,233,377,610,987,1597,2584,4181,6765,1,1,2,NULL);
SELECT count(*) FROM t WHERE a NOT IN (1,2,3,NULL);
SELECT count(*) FROM t WHERE a NOT IN (1,2,3);

-- case: research/mine/zeroblob-large-length
SELECT length(zeroblob(10000000)), typeof(zeroblob(-5)), length(zeroblob(-5));

-- case: research/mine/randomblob-negative
SELECT length(randomblob(-1)), length(randomblob(0)), typeof(randomblob(0));

-- case: research/mine/substr-extremes
SELECT substr('hello', -9223372036854775808, 3), substr('hello', 9223372036854775807), substr('hello', 2, -9223372036854775808), substr('hello', 0, 2), substr('hello',-2,5), substr('hello', 3, -2), substr(x'0102030405', -2, 1);

-- case: research/mine/printf-width-huge
SELECT length(printf('%2000d', 1)), length(printf('%.3000f', 1.0)) > 100;

-- case: research/mine/printf-variants
SELECT printf('%5.2f|%-6d|%06.1f|%x|%X|%o|%e|%g|%c|%%|%q|%Q|%w', 3.14159, 42, -2.5, 255, 255, 8, 12345.678, 0.0001, 'A', 'it''s', NULL, 'a"b');

-- case: research/mine/printf-integer-edge
SELECT printf('%d', 9223372036854775807), printf('%d', -9223372036854775808), printf('%u', -1), printf('%x', -1), printf('%lld', 12), printf('%,d', 1234567), printf('%!.20g', 0.1);

-- case: research/mine/printf-missing-args
SELECT printf('%d %s %f'), printf('%s', 1, 2), printf(NULL), printf('%5s|%-5s|', 'ab', 'cd'), printf('%.2s', 'abcdef');

-- case: research/mine/format-string-types
SELECT format('%s %s %s', 1, 2.5, x'41'), format('%d', '12abc'), format('%d', 3.99), format('%f', '1e3'), format('%i', -0.5);

-- case: research/mine/char-function
SELECT char(65, 0x1F600, 66), length(char(0)), hex(char(128)), char(), char(NULL, 65);

-- case: research/mine/unicode-function
SELECT unicode('é'), unicode(''), unicode(NULL), unicode(x'ff'), unicode('😀x');

-- case: research/mine/deep-case-expression
SELECT CASE WHEN 0 THEN 0 WHEN 0 THEN 1 WHEN 0 THEN 2 WHEN 0 THEN 3 WHEN 0 THEN 4 WHEN 0 THEN 5 WHEN 0 THEN 6 WHEN 0 THEN 7 WHEN 0 THEN 8 WHEN 1 THEN 9 ELSE 10 END;

-- case: research/mine/long-and-chain
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT count(*) FROM t WHERE a<>10 AND a<>11 AND a<>12 AND a<>13 AND a<>14 AND a<>15 AND a<>16 AND a<>17 AND a<>18 AND a<>19 AND a<>20 AND a<>21 AND a<>22 AND a<>23 AND a<>24 AND a<>25 AND a<>26 AND a<>27 AND a<>28 AND a<>29 AND a<>2;

-- case: research/mine/long-or-chain-index
CREATE TABLE t(a INTEGER, b TEXT);
CREATE INDEX ta ON t(a);
WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM c WHERE i<500) INSERT INTO t SELECT i, 'v'||i FROM c;
SELECT b FROM t WHERE a=3 OR a=7 OR a=11 OR a=400 OR a=499 OR a=1000 OR a IS NULL ORDER BY a;

-- case: research/mine/union-many-arms
SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9 UNION SELECT 10 UNION SELECT 11 UNION SELECT 12 UNION SELECT 1 ORDER BY 1 DESC LIMIT 3;

-- case: research/mine/join-many-tables
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
SELECT count(*) FROM t t1, t t2, t t3, t t4, t t5, t t6, t t7, t t8, t t9, t t10, t t11, t t12;

-- case: research/mine/blob-concat-and-compare
SELECT x'41' || x'42', typeof(x'41' || 1), x'00' < '', x'' = '', '' < x'', length(x'00' || x'00'), CAST(x'616263' AS TEXT), CAST('abc' AS BLOB) = x'616263';

-- case: research/mine/text-with-nul
SELECT length(CAST(x'610062' AS TEXT)), hex(CAST(x'610062' AS TEXT)), CAST(x'610062' AS TEXT) = 'a', instr(CAST(x'610062' AS TEXT), 'b'), upper(CAST(x'610062' AS TEXT));

-- case: research/mine/limit-edge-values
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
SELECT a FROM t ORDER BY a LIMIT -1;
SELECT a FROM t ORDER BY a LIMIT 2 OFFSET -1;
SELECT a FROM t ORDER BY a LIMIT 9223372036854775807 OFFSET 2;
SELECT a FROM t ORDER BY a LIMIT 0;
SELECT a FROM t ORDER BY a LIMIT '2';
SELECT a FROM t ORDER BY a LIMIT 1, 2;
SELECT a FROM t ORDER BY a LIMIT 2.0;

-- case: research/mine/limit-non-integer-errors
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
SELECT a FROM t LIMIT 1.5;
SELECT a FROM t LIMIT 'x';
SELECT a FROM t LIMIT NULL;
SELECT a FROM t LIMIT x'01';

-- case: research/mine/limit-subquery-expression
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
SELECT a FROM t ORDER BY a LIMIT (SELECT count(*) FROM t)/2 OFFSET (SELECT 1);
SELECT a FROM t ORDER BY a DESC LIMIT 1+1;

-- case: research/mine/int-overflow-arith
SELECT 9223372036854775807 + 1, -9223372036854775808 - 1, 9223372036854775807 * 2, -9223372036854775808 * -1, -9223372036854775808 / -1, -9223372036854775808 % -1, abs(-9223372036854775807);

-- case: research/mine/abs-min-int-error
SELECT abs(-9223372036854775808);

-- case: research/mine/sum-overflow-error
CREATE TABLE t(a);
INSERT INTO t VALUES(9223372036854775807),(1);
SELECT sum(a) FROM t;

-- case: research/mine/total-and-avg-overflow
CREATE TABLE t(a);
INSERT INTO t VALUES(9223372036854775807),(1);
SELECT total(a), avg(a) FROM t;

-- case: research/mine/sum-mixed-real-int
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2.5),('3'),('x'),(NULL),(x'01');
SELECT sum(a), total(a), avg(a), count(a), typeof(sum(a)) FROM t;

-- case: research/mine/sum-of-texts-that-look-integer
CREATE TABLE t(a TEXT);
INSERT INTO t VALUES('1'),('2'),('3');
SELECT sum(a), typeof(sum(a)), avg(a), max(a), min(a) FROM t;

-- case: research/mine/integer-literal-out-of-range
SELECT 9223372036854775808, -9223372036854775809, 18446744073709551616, typeof(9223372036854775808), 0x7FFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0x8000000000000000;

-- case: research/mine/hex-literal-too-big
SELECT 0x10000000000000000;

-- case: research/mine/real-formatting
SELECT 1.0, 1e100, 1e-100, 123456789012345678.0, 0.1, 1.0/3, 2.0/3, -0.0, 1e308*10, -1e308*10, 4.9e-324, 1.7976931348623157e308;

-- case: research/mine/real-to-text
SELECT CAST(1e15 AS TEXT), CAST(1e16 AS TEXT), CAST(123.456 AS TEXT), CAST(-0.0 AS TEXT), CAST(1.0/0 AS TEXT), CAST(0.1+0.2 AS TEXT), 1e15||'', 3.0||'';

-- case: research/mine/text-to-number-cast
SELECT CAST('  12  ' AS INTEGER), CAST('12abc' AS INTEGER), CAST('0x10' AS INTEGER), CAST('1e3' AS INTEGER), CAST('1.9' AS INTEGER), CAST('-1.9' AS INTEGER), CAST('' AS INTEGER), CAST('abc' AS REAL), CAST('1e999' AS REAL), CAST('9223372036854775808' AS INTEGER), CAST('-9223372036854775809' AS INTEGER);

-- case: research/mine/cast-real-to-int-saturates
SELECT CAST(1e19 AS INTEGER), CAST(-1e19 AS INTEGER), CAST(9.2233720368547758e18 AS INTEGER), CAST(1.0/0 AS INTEGER), CAST(-1.0/0 AS INTEGER), CAST(0.0/0 AS INTEGER);

-- case: research/mine/cast-numeric
SELECT CAST('12' AS NUMERIC), CAST('12.0' AS NUMERIC), CAST('12.5' AS NUMERIC), CAST('1e2' AS NUMERIC), CAST('abc' AS NUMERIC), CAST('9223372036854775808' AS NUMERIC), CAST(' 7 ' AS NUMERIC), CAST('0x1A' AS NUMERIC), typeof(CAST('12.0' AS NUMERIC));

-- case: research/mine/cast-blob
SELECT CAST(x'3132' AS INTEGER), CAST(x'3132' AS REAL), CAST(12 AS BLOB), typeof(CAST(12 AS BLOB)), CAST(1.5 AS BLOB), hex(CAST(1.5 AS BLOB));

-- case: research/mine/arithmetic-on-text
SELECT '5' + '3', '5.5' * 2, 'abc' + 1, '1e2' + 0, ' 3 ' + 0, '3x' + 0, x'33' + 0, '0x10' + 0, '-' + 0, '.5' + 0, '5.' + 0, '+5' + 0;

-- case: research/mine/division-and-modulo
SELECT 7/2, -7/2, 7%3, -7%3, 7%-3, 7.5%2, 7%2.5, 7/0, 7%0, 7.0/0, 0/0.0, 5 % 0.5, -9223372036854775808 / 2;

-- case: research/mine/modulo-real
SELECT 10.5 % 3, -10.5 % 3, 1e20 % 7, 7 % 1e20, typeof(10.5 % 3);

-- case: research/mine/bitwise-ops
SELECT 1<<63, 1<<64, 1<<-1, 8>>-1, -1>>1, -1>>63, -1>>64, 1<<65, ~0, ~-1, 5&3, 5|3, 12.7 & 7, '12' | 1;

-- case: research/mine/comparison-int-real-boundary
SELECT 9223372036854775807 = 9223372036854775807.0, 9223372036854775807 < 9223372036854775808.0, 9007199254740993 = 9007199254740992.0, 9007199254740993 > 9007199254740992.0, -9223372036854775808 = -9223372036854775808.0;

-- case: research/mine/compare-int-real-in-table
CREATE TABLE t(a INTEGER, b REAL);
INSERT INTO t VALUES(9007199254740993, 9007199254740992.0);
SELECT a = b, a > b, a < b, b = 9007199254740993, a - b FROM t;

-- case: research/mine/round-function
SELECT round(2.5), round(-2.5), round(0.5), round(1.005, 2), round(1234.5678, -2), round(123.456, 400), round(1e300, 2), round(NULL), round('3.7'), round(9223372036854775807), round(-0.4), typeof(round(5));

-- case: research/mine/math-functions
SELECT ceil(1.2), ceil(-1.2), floor(-1.2), trunc(-1.7), ceil(5), typeof(ceil(5)), sqrt(-1), ln(0), log(100), log(2, 8), pow(2, 0.5), mod(7, -3), sign(-0.0), sign('x'), pi() > 3;

-- case: research/mine/math-domain-errors
SELECT acos(2), log(-1), log10(0), exp(1000), power(0, -1), atan2(0, 0), degrees(pi()), radians(180);

-- case: research/mine/numeric-affinity-on-insert
CREATE TABLE t(i INTEGER, r REAL, n NUMERIC, t TEXT, b BLOB, x);
INSERT INTO t VALUES('12', '12', '12', 12, '12', '12');
INSERT INTO t VALUES('12.0', '12.0', '12.0', 12.0, 12.0, 12.0);
INSERT INTO t VALUES('1e3', '1e3', '1e3', 1e3, x'31', '1e3');
INSERT INTO t VALUES(' 12 ', ' 12 ', ' 12 ', ' 12 ', ' 12 ', ' 12 ');
INSERT INTO t VALUES('9223372036854775808', '9223372036854775808', '9223372036854775808', 'a', 'b', 'c');
INSERT INTO t VALUES('0x10', '0x10', '0x10', NULL, NULL, NULL);
INSERT INTO t VALUES('12.5', '12.5abc', '-0', '-0', '-0', '-0');
INSERT INTO t VALUES(1e20, 1e20, '1e20', 1e20, 1e20, 1e20);
SELECT quote(i), typeof(i), quote(r), typeof(r), quote(n), typeof(n), quote(t), quote(b), quote(x) FROM t;

-- case: research/mine/numeric-real-that-fits-integer
CREATE TABLE t(n NUMERIC, i INTEGER);
INSERT INTO t VALUES(3.0, 3.0), (3.5, 3.5), (1e18, 1e18), (1e19, 1e19), ('3.0', '3.0'), (-0.0, -0.0);
SELECT quote(n), typeof(n), quote(i), typeof(i) FROM t;

-- case: research/mine/strict-table-coercion
CREATE TABLE t(i INTEGER, r REAL, s TEXT, b BLOB, a ANY) STRICT;
INSERT INTO t VALUES('12', 12, 12, x'00', '12');
INSERT INTO t VALUES(1, '1.5', 'x', x'01', 1.0);
SELECT quote(i), quote(r), quote(s), quote(b), quote(a), typeof(a) FROM t;
INSERT INTO t VALUES('12x', 1, 'a', x'00', 1);
INSERT INTO t VALUES(1, 'abc', 'a', x'00', 1);
INSERT INTO t VALUES(1, 1, 'a', 'text', 1);
INSERT INTO t VALUES(1.5, 1, 'a', x'00', 1);
SELECT count(*) FROM t;

-- case: research/mine/strict-integer-from-real-exact
CREATE TABLE t(i INTEGER) STRICT;
INSERT INTO t VALUES(3.0);
INSERT INTO t VALUES('4.0');
INSERT INTO t VALUES(1e18);
SELECT quote(i), typeof(i) FROM t;

-- case: research/mine/quote-function
SELECT quote(1), quote(1.5), quote('a''b'), quote(x'00ff'), quote(NULL), quote(1e100), quote(-0.0), quote(0.1), quote(100.0), quote(1e15), quote(1e16);

-- case: research/mine/real-precision-print
SELECT 0.1+0.2, 1.1*1.1, 100.0*1.1, 3.0*1.1, 1e-7, 123456789.123456789, 0.000001, 1.5e-5, 12345678901234567890.0;

-- case: research/mine/integer-division-by-text-zero
SELECT 5/'0', 5%'0', 5/'abc', 5.0/'0.0', '5'/'2', '5.0'/'2';

-- case: research/mine/unary-plus-keeps-type
SELECT +'12', typeof(+'12'), -'12', typeof(-'12'), -'abc', -x'31', typeof(-NULL), -'1e2', - '9223372036854775808';

-- case: research/mine/negative-zero
SELECT -0.0, 0.0 = -0.0, quote(-0.0), CAST(-0.0 AS INTEGER), -0.0 || '', round(-0.4), printf('%f', -0.0), 1/-0.0;

-- case: research/mine/big-real-to-int-compare-index
CREATE TABLE t(a INTEGER PRIMARY KEY);
INSERT INTO t VALUES(1),(9223372036854775807),(-9223372036854775808);
SELECT a FROM t WHERE a > 9.3e18;
SELECT a FROM t WHERE a < -9.3e18;
SELECT a FROM t WHERE a >= 9223372036854775807.0;
SELECT a FROM t WHERE a = 1.0;
SELECT a FROM t WHERE a = 1.5;
SELECT a FROM t WHERE a > 0.5 AND a < 1.5;
SELECT a FROM t WHERE a = '1';
SELECT a FROM t WHERE a > 'x';

-- case: research/mine/rowid-max-autoincrement-full
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(9223372036854775807, 'max');
INSERT INTO t(b) VALUES('next');
SELECT count(*), max(a) > 0 FROM t;

-- case: research/mine/autoincrement-at-max
CREATE TABLE t(a INTEGER PRIMARY KEY AUTOINCREMENT, b);
INSERT INTO t VALUES(9223372036854775807, 'max');
INSERT INTO t(b) VALUES('next');
SELECT a, b FROM t;

-- case: research/mine/rowid-text-compare
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(5, 'x');
INSERT INTO t VALUES('6', 'y');
INSERT INTO t VALUES(' 7', 'z');
INSERT INTO t VALUES('8.0', 'w');
SELECT a, typeof(a), b FROM t;
INSERT INTO t VALUES('8.5', 'q');
INSERT INTO t VALUES('abc', 'q');
SELECT count(*) FROM t;

-- case: research/mine/partial-index-is-not-null
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a) WHERE a IS NOT NULL;
INSERT INTO t VALUES(1,1),(NULL,2),(3,3),(NULL,4);
SELECT b FROM t WHERE a IS NULL ORDER BY b;
SELECT b FROM t WHERE a > 0 ORDER BY b;
SELECT b FROM t WHERE a IS NOT 1 ORDER BY b;
SELECT count(*) FROM t WHERE a NOT NULL;

-- case: research/mine/partial-index-wrong-implication
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(b) WHERE a > 5;
INSERT INTO t VALUES(1,'x'),(10,'x'),(6,'y'),(NULL,'x');
SELECT a FROM t WHERE b='x' AND a > 4 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a > 5 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a > 6 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a >= 5 ORDER BY a;

-- case: research/mine/partial-index-text-vs-int
CREATE TABLE t(a TEXT, b);
CREATE INDEX ti ON t(b) WHERE a = 1;
INSERT INTO t VALUES('1','p'),(1,'q'),('01','r');
SELECT b FROM t WHERE a = 1 AND b > '' ORDER BY b;
SELECT b FROM t WHERE a = '1' AND b > '' ORDER BY b;

-- case: research/mine/partial-index-in-left-join
CREATE TABLE t1(a);
CREATE TABLE t2(b, c);
CREATE INDEX i2 ON t2(b) WHERE c IS NOT NULL;
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES(1,NULL),(2,5);
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b WHERE c IS NULL ORDER BY a;
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b AND c IS NOT NULL ORDER BY a;

-- case: research/mine/expression-index-use
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a+b);
INSERT INTO t VALUES(1,2),(2,1),(3,0),(1,1),('1','2');
SELECT a, b FROM t WHERE a+b = 3 ORDER BY a, b;
SELECT a, b FROM t WHERE b+a = 3 ORDER BY a, b;
SELECT a+b FROM t ORDER BY a+b;

-- case: research/mine/expression-index-lower
CREATE TABLE t(name TEXT);
CREATE INDEX ti ON t(lower(name));
INSERT INTO t VALUES('Bob'),('bob'),('BOB'),('Alice'),(NULL);
SELECT name FROM t WHERE lower(name) = 'bob' ORDER BY name;
SELECT name FROM t WHERE lower(name) > 'b' ORDER BY lower(name), name;
UPDATE t SET name = 'Carol' WHERE lower(name) = 'alice';
SELECT name FROM t WHERE lower(name) = 'carol';
PRAGMA integrity_check;

-- case: research/mine/collate-nocase-index-eq
CREATE TABLE t(a TEXT COLLATE NOCASE);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES('abc'),('ABC'),('aBd'),('b');
SELECT a FROM t WHERE a = 'ABC' ORDER BY a, rowid;
SELECT a FROM t WHERE a = 'ABC' COLLATE BINARY;
SELECT a FROM t WHERE a > 'ABC' ORDER BY a, rowid;
SELECT DISTINCT a FROM t ORDER BY 1;
SELECT a, count(*) FROM t GROUP BY a ORDER BY 1;

-- case: research/mine/collate-mismatch-index-binary
CREATE TABLE t(a TEXT);
CREATE INDEX ti ON t(a COLLATE NOCASE);
INSERT INTO t VALUES('abc'),('ABC'),('b');
SELECT a FROM t WHERE a = 'ABC' ORDER BY a;
SELECT a FROM t WHERE a = 'ABC' COLLATE NOCASE ORDER BY a;
SELECT a FROM t WHERE a COLLATE NOCASE > 'ABC' ORDER BY a;
SELECT a FROM t ORDER BY a COLLATE NOCASE, a;

-- case: research/mine/rtrim-collation
CREATE TABLE t(a TEXT COLLATE RTRIM);
INSERT INTO t VALUES('x'),('x  '),('x '),(' x');
CREATE UNIQUE INDEX tu ON t(a);
SELECT count(*) FROM t;

-- case: research/mine/rtrim-collation-compare
CREATE TABLE t(a TEXT COLLATE RTRIM);
INSERT INTO t VALUES('x'),('x  '),(' x'),('y');
SELECT count(*) FROM t WHERE a = 'x';
SELECT count(DISTINCT a) FROM t;
SELECT a, length(a) FROM t ORDER BY a, length(a);
SELECT 'a ' = 'a' COLLATE RTRIM, 'a ' < 'a' COLLATE RTRIM, 'a' = 'a  ' COLLATE RTRIM;

-- case: research/mine/like-optimization-nocase-column
CREATE TABLE t(a TEXT COLLATE NOCASE);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES('abc'),('ABD'),('xyz'),('ab'),('a_c'),('a%c');
SELECT a FROM t WHERE a LIKE 'ab%' ORDER BY a;
SELECT a FROM t WHERE a LIKE 'AB%' ORDER BY a;
SELECT a FROM t WHERE a LIKE 'a\_%' ESCAPE '\' ORDER BY a;
SELECT a FROM t WHERE a GLOB 'a*' ORDER BY a;

-- case: research/mine/like-optimization-binary-column
CREATE TABLE t(a TEXT);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES('abc'),('ABD'),('xyz'),('ab'),(1),(12),('1a');
SELECT a FROM t WHERE a LIKE 'ab%' ORDER BY a;
SELECT a FROM t WHERE a LIKE 'AB%' ORDER BY a;
SELECT a FROM t WHERE a LIKE '1%' ORDER BY a;
SELECT a FROM t WHERE a GLOB 'a*' ORDER BY a;
SELECT a FROM t WHERE a GLOB 'A*' ORDER BY a;

-- case: research/mine/like-on-integer-column-index
CREATE TABLE t(a INTEGER);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES(1),(10),(100),(2),(-1);
SELECT a FROM t WHERE a LIKE '1%' ORDER BY a;
SELECT a FROM t WHERE a LIKE '-%' ORDER BY a;
SELECT a FROM t WHERE a GLOB '1*' ORDER BY a;

-- case: research/mine/like-edge-patterns
SELECT 'abc' LIKE 'ABC', 'abc' LIKE 'a%c%', '' LIKE '%', '' LIKE '_', 'a' LIKE '', NULL LIKE 'a', 'a' LIKE NULL, 'ä' LIKE 'Ä', 'a%' LIKE 'a\%' ESCAPE '\', 'ab' LIKE 'a\%' ESCAPE '\', '%' LIKE '%%';

-- case: research/mine/like-escape-errors
SELECT 'a' LIKE 'a' ESCAPE 'ab';

-- case: research/mine/glob-classes
SELECT 'abc' GLOB 'a[a-c]c', 'a]c' GLOB 'a[]]c', 'a-c' GLOB 'a[a-]c', 'abc' GLOB 'a[^b]c', 'a^c' GLOB 'a[^b]c', 'ABC' GLOB 'a*', 'a*c' GLOB 'a[*]c', 'a' GLOB '[', 'é' GLOB '?', '' GLOB '*';

-- case: research/mine/in-affinity-index
CREATE TABLE t(a TEXT, b INTEGER);
CREATE INDEX ta ON t(a);
CREATE INDEX tb ON t(b);
INSERT INTO t VALUES('1', 1), ('2', 2), ('x', 3);
SELECT b FROM t WHERE a IN (1, 2) ORDER BY b;
SELECT b FROM t WHERE b IN ('1', '2') ORDER BY b;
SELECT b FROM t WHERE a IN (SELECT 1) ORDER BY b;
SELECT b FROM t WHERE b IN (SELECT '2') ORDER BY b;
SELECT b FROM t WHERE a IN (SELECT b FROM t) ORDER BY b;

-- case: research/mine/in-affinity-no-index
CREATE TABLE t(a TEXT, b INTEGER);
INSERT INTO t VALUES('1', 1), ('2', 2), ('x', 3);
SELECT b FROM t WHERE a IN (1, 2) ORDER BY b;
SELECT b FROM t WHERE b IN ('1', '2') ORDER BY b;
SELECT b FROM t WHERE a IN (SELECT 1) ORDER BY b;
SELECT b FROM t WHERE b IN (SELECT '2') ORDER BY b;
SELECT 1 IN ('1'), '1' IN (1), 1 IN (SELECT '1'), (SELECT a FROM t WHERE b=1) IN (1);

-- case: research/mine/not-in-null-subquery
CREATE TABLE t(a);
CREATE TABLE u(b);
INSERT INTO t VALUES(1),(2),(NULL);
INSERT INTO u VALUES(1),(NULL);
SELECT a FROM t WHERE a NOT IN (SELECT b FROM u);
SELECT a FROM t WHERE a IN (SELECT b FROM u);
SELECT a, a IN (SELECT b FROM u), a NOT IN (SELECT b FROM u) FROM t;
SELECT a FROM t WHERE a NOT IN (SELECT b FROM u WHERE b IS NOT NULL);
SELECT NULL IN (SELECT b FROM u WHERE 0), NULL NOT IN (SELECT 1 WHERE 0), NULL IN ();

-- case: research/mine/in-empty-list
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(NULL);
SELECT a, a IN (), a NOT IN () FROM t;
SELECT count(*) FROM t WHERE a NOT IN ();

-- case: research/mine/row-value-compare
SELECT (1,2) < (1,3), (1,2) = (1,2), (1,NULL) = (1,2), (1,NULL) < (2,0), (1,NULL) < (1,2), (2,NULL) > (1,5), (1,2) IN (VALUES(1,2),(3,4)), (1,2) IN (SELECT 1,2);

-- case: research/mine/row-value-index-range
CREATE TABLE t(a, b, c);
CREATE INDEX ti ON t(a, b);
INSERT INTO t VALUES(1,1,'a'),(1,2,'b'),(1,3,'c'),(2,1,'d'),(2,2,'e'),(NULL,1,'f'),(1,NULL,'g');
SELECT c FROM t WHERE (a,b) > (1,2) ORDER BY a, b;
SELECT c FROM t WHERE (a,b) >= (1,2) AND (a,b) < (2,2) ORDER BY a, b;
SELECT c FROM t WHERE (a,b) IN ((1,2),(2,1),(NULL,1)) ORDER BY c;
SELECT c FROM t WHERE (a,b) = (SELECT 2, 2);

-- case: research/mine/row-value-size-mismatch
SELECT (1,2) = (1,2,3);

-- case: research/mine/row-value-in-update
CREATE TABLE t(a, b, c);
INSERT INTO t VALUES(1,2,3);
UPDATE t SET (a, b) = (b, a), c = (SELECT 9);
UPDATE t SET (a, b) = (SELECT c, c+1);
SELECT * FROM t;

-- case: research/mine/is-distinct-from
SELECT 1 IS DISTINCT FROM 1, 1 IS NOT DISTINCT FROM NULL, NULL IS NOT DISTINCT FROM NULL, NULL IS DISTINCT FROM 1, 1 IS 1.0, '1' IS 1;

-- case: research/mine/is-operator-with-index
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES(1,'a'),(NULL,'b'),(2,'c');
SELECT b FROM t WHERE a IS NULL;
SELECT b FROM t WHERE a IS 1;
SELECT b FROM t WHERE a IS NOT NULL ORDER BY b;
SELECT b FROM t WHERE a IS NOT 2 ORDER BY b;
SELECT b FROM t WHERE NOT (a IS NOT 2);

-- case: research/mine/left-join-where-pushdown
CREATE TABLE t1(a);
CREATE TABLE t2(b, c);
INSERT INTO t1 VALUES(1),(2),(3);
INSERT INTO t2 VALUES(1,10),(2,NULL);
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b WHERE c IS NULL ORDER BY a;
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b WHERE coalesce(c, 0) = 0 ORDER BY a;
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b AND c > 5 ORDER BY a;
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b WHERE b IS NOT NULL OR a = 3 ORDER BY a;
SELECT a, b FROM t1 LEFT JOIN t2 ON 0 ORDER BY a;
SELECT a, b FROM t1 LEFT JOIN t2 ON 1 WHERE b = 2 ORDER BY a;

-- case: research/mine/left-join-constant-on
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES(1);
SELECT a, b FROM t1 LEFT JOIN t2 ON a=1 ORDER BY a;
SELECT a, b FROM t1 LEFT JOIN t2 ON b=2 ORDER BY a;
SELECT a, b FROM t1 LEFT JOIN t2 ON NULL ORDER BY a;
SELECT a, b, b IS NULL FROM t1 LEFT JOIN (SELECT b FROM t2 WHERE b > 0) ON a=b ORDER BY a;

-- case: research/mine/left-join-subquery-constant-column
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES(1);
SELECT a, x FROM t1 LEFT JOIN (SELECT b, 'k' AS x FROM t2) ON a=b ORDER BY a;
SELECT a, x FROM t1 LEFT JOIN (SELECT b, 5 AS x FROM t2) s ON a=b WHERE x IS NULL;
SELECT a, x FROM t1 LEFT JOIN (SELECT b, count(*) AS x FROM t2 GROUP BY b) ON a=b ORDER BY a;
SELECT a, x FROM t1 LEFT JOIN (SELECT b, NULL IS NULL AS x FROM t2) ON a=b ORDER BY a;

-- case: research/mine/left-join-view-with-aggregate
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1 VALUES(1),(2);
CREATE VIEW v AS SELECT count(*) AS c FROM t2;
SELECT a, c FROM t1 LEFT JOIN v ON a = c+1 ORDER BY a;
SELECT a, c FROM t1 LEFT JOIN v ON 0 ORDER BY a;

-- case: research/mine/right-full-join
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1 VALUES(1),(2),(NULL);
INSERT INTO t2 VALUES(2),(3),(NULL);
SELECT a, b FROM t1 RIGHT JOIN t2 ON a=b ORDER BY b, a;
SELECT a, b FROM t1 FULL JOIN t2 ON a=b ORDER BY a, b;
SELECT a, b FROM t1 FULL JOIN t2 ON a=b WHERE a IS NULL ORDER BY b;
SELECT count(*) FROM t1 FULL JOIN t2 ON 0;
SELECT a, b FROM t1 FULL JOIN t2 USING(a) ;

-- case: research/mine/full-join-using
CREATE TABLE t1(k, a);
CREATE TABLE t2(k, b);
INSERT INTO t1 VALUES(1,'a1'),(2,'a2');
INSERT INTO t2 VALUES(2,'b2'),(3,'b3');
SELECT k, a, b FROM t1 FULL JOIN t2 USING(k) ORDER BY k;
SELECT * FROM t1 FULL JOIN t2 USING(k) ORDER BY 1;
SELECT * FROM t1 NATURAL FULL JOIN t2 ORDER BY 1;

-- case: research/mine/right-join-three-way
CREATE TABLE a(x);
CREATE TABLE b(y);
CREATE TABLE c(z);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(2),(3);
INSERT INTO c VALUES(3),(4);
SELECT x, y, z FROM a LEFT JOIN b ON x=y RIGHT JOIN c ON y=z ORDER BY z;
SELECT x, y, z FROM a RIGHT JOIN b ON x=y LEFT JOIN c ON y=z ORDER BY y;
SELECT x, y, z FROM a FULL JOIN b ON x=y FULL JOIN c ON y=z ORDER BY x, y, z;

-- case: research/mine/natural-join-no-common
CREATE TABLE t1(a);
CREATE TABLE t2(b);
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES(3);
SELECT * FROM t1 NATURAL JOIN t2 ORDER BY a;

-- case: research/mine/using-column-ambiguity
CREATE TABLE t1(a, b);
CREATE TABLE t2(a, c);
CREATE TABLE t3(a, d);
INSERT INTO t1 VALUES(1,1);
INSERT INTO t2 VALUES(1,2);
INSERT INTO t3 VALUES(1,3);
SELECT a, b, c, d FROM t1 JOIN t2 USING(a) JOIN t3 USING(a);
SELECT * FROM t1 JOIN t2 USING(a) JOIN t3 USING(a);
SELECT t1.a, t2.a, t3.a FROM t1 JOIN t2 USING(a) JOIN t3 USING(a);

-- case: research/mine/distinct-orderby-index
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a, b);
INSERT INTO t VALUES(1,2),(1,2),(1,3),(2,1),(NULL,1),(NULL,1);
SELECT DISTINCT a FROM t ORDER BY a DESC;
SELECT DISTINCT a, b FROM t ORDER BY b, a;
SELECT DISTINCT b FROM t ORDER BY b;
SELECT count(DISTINCT a), count(DISTINCT b), count(DISTINCT a||b) FROM t;

-- case: research/mine/group-by-null-and-collate
CREATE TABLE t(a TEXT COLLATE NOCASE, b);
INSERT INTO t VALUES('x',1),('X',2),(NULL,3),(NULL,4),('y',5);
SELECT a, sum(b) FROM t GROUP BY a ORDER BY a;
SELECT a COLLATE BINARY, sum(b) FROM t GROUP BY a COLLATE BINARY ORDER BY 1;
SELECT upper(a), count(*) FROM t GROUP BY 1 ORDER BY 1;

-- case: research/mine/min-max-optimization
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES(NULL,1),(3,2),(1,3),('x',4),(x'00',5);
SELECT min(a), max(a) FROM t;
SELECT min(a) FROM t WHERE a > 1;
SELECT max(a) FROM t WHERE a < 'a';
SELECT min(a), b FROM t;
SELECT max(a), b FROM t;
SELECT min(a) FROM t WHERE a IS NULL;

-- case: research/mine/min-max-bare-column
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,'one'),(3,'three'),(2,'two'),(3,'three-b');
SELECT max(a), b FROM t;
SELECT min(a), b FROM t;
SELECT max(a), b, min(a) FROM t;
SELECT a, max(b) FROM t GROUP BY a ORDER BY a;

-- case: research/mine/count-star-index
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(b) WHERE b > 0;
INSERT INTO t VALUES(1,0),(2,1),(3,NULL);
SELECT count(*) FROM t;
SELECT count(*) FROM t WHERE b > 0;
SELECT count(b) FROM t;

-- case: research/mine/order-by-desc-index-nulls
CREATE TABLE t(a);
CREATE INDEX ti ON t(a DESC);
INSERT INTO t VALUES(1),(NULL),(3),(2),(NULL);
SELECT a FROM t ORDER BY a;
SELECT a FROM t ORDER BY a DESC;
SELECT a FROM t ORDER BY a NULLS LAST;
SELECT a FROM t ORDER BY a DESC NULLS FIRST;
SELECT a FROM t WHERE a > 1 ORDER BY a DESC;
SELECT a FROM t WHERE a < 3 ORDER BY a;

-- case: research/mine/order-by-mixed-types
CREATE TABLE t(a);
INSERT INTO t VALUES(1),('1'),(1.5),(x'01'),(NULL),('a'),(-1),('A'),(0.0);
SELECT quote(a) FROM t ORDER BY a;
SELECT quote(a) FROM t ORDER BY a DESC;
SELECT quote(a) FROM t ORDER BY a COLLATE NOCASE;

-- case: research/mine/order-by-alias-and-ordinal
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,3),(2,2),(3,1);
SELECT a AS b, b AS a FROM t ORDER BY b;
SELECT a AS x FROM t ORDER BY -x;
SELECT a, b FROM t ORDER BY 2;
SELECT a+b AS s, a FROM t ORDER BY s, a;
SELECT a FROM t ORDER BY 3;

-- case: research/mine/order-by-compound-errors
SELECT 1 AS a UNION SELECT 2 ORDER BY b;

-- case: research/mine/compound-order-by-expression
SELECT 1 AS a UNION SELECT 2 ORDER BY a+1;

-- case: research/mine/compound-with-collate
CREATE TABLE t1(a TEXT COLLATE NOCASE);
CREATE TABLE t2(b TEXT);
INSERT INTO t1 VALUES('a'),('B');
INSERT INTO t2 VALUES('A'),('b');
SELECT a FROM t1 UNION SELECT b FROM t2 ORDER BY 1;
SELECT b FROM t2 UNION SELECT a FROM t1 ORDER BY 1;
SELECT a FROM t1 INTERSECT SELECT b FROM t2 ORDER BY 1;
SELECT b FROM t2 EXCEPT SELECT a FROM t1 ORDER BY 1;

-- case: research/mine/compound-limit-in-arm
SELECT * FROM (SELECT 1 UNION ALL SELECT 2 LIMIT 1) UNION ALL SELECT 3;
SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 LIMIT 2 OFFSET 1;

-- case: research/mine/values-compound
VALUES(1,'a'),(2,'b') UNION ALL VALUES(3,'c') ORDER BY 1 DESC;
SELECT * FROM (VALUES(1),(2)) AS v(x);

-- case: research/mine/subquery-in-order-by
CREATE TABLE t(a);
INSERT INTO t VALUES(2),(1),(3);
SELECT a FROM t ORDER BY (SELECT a);
SELECT a FROM t ORDER BY (SELECT -t.a);
SELECT a FROM t ORDER BY (SELECT 1);

-- case: research/mine/scalar-subquery-multiple-rows
CREATE TABLE t(a);
INSERT INTO t VALUES(2),(1),(3);
SELECT (SELECT a FROM t);
SELECT (SELECT a FROM t ORDER BY a DESC);
SELECT (SELECT a FROM t WHERE 0);

-- case: research/mine/correlated-subquery-agg
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(2,3);
SELECT a, (SELECT sum(b) FROM t t2 WHERE t2.a=t.a) FROM t ORDER BY a, b;
SELECT a, sum(b), (SELECT count(*) FROM t t2 WHERE t2.a=t.a) FROM t GROUP BY a ORDER BY a;
SELECT a FROM t GROUP BY a HAVING (SELECT max(b) FROM t t2 WHERE t2.a=t.a) > 2;
SELECT (SELECT count(t.a)) FROM t;
SELECT (SELECT sum(t.b) FROM t t2 LIMIT 1) FROM t;

-- case: research/mine/exists-variants
CREATE TABLE t(a);
CREATE TABLE u(b);
INSERT INTO t VALUES(1),(2);
INSERT INTO u VALUES(2);
SELECT a FROM t WHERE EXISTS (SELECT 1 FROM u WHERE b=a);
SELECT a FROM t WHERE NOT EXISTS (SELECT 1 FROM u WHERE b=a);
SELECT a, EXISTS (SELECT b FROM u WHERE b > a) FROM t;
SELECT EXISTS (SELECT 1 LIMIT 0), EXISTS (SELECT NULL), EXISTS (SELECT 1 WHERE 0);
SELECT a FROM t WHERE EXISTS (SELECT 1 FROM u LIMIT 0);

-- case: research/mine/having-without-group
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
SELECT count(*) FROM t HAVING count(*) > 1;
SELECT count(*) FROM t HAVING count(*) > 5;
SELECT 1 HAVING 1;
SELECT a FROM t HAVING a > 1;

-- case: research/mine/group-by-alias-shadows-column
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(2,3);
SELECT a+1 AS a, count(*) FROM t GROUP BY a ORDER BY 1;
SELECT b%2 AS a, count(*) FROM t GROUP BY a ORDER BY 1;
SELECT b%2 AS k, count(*) FROM t GROUP BY k ORDER BY 1;

-- case: research/mine/aggregate-in-where-error
CREATE TABLE t(a);
SELECT a FROM t WHERE count(*) > 1;

-- case: research/mine/nested-aggregate-error
CREATE TABLE t(a);
SELECT sum(count(*)) FROM t;

-- case: research/mine/aggregate-of-outer-in-subquery
CREATE TABLE t(a);
CREATE TABLE u(b);
INSERT INTO t VALUES(1),(2);
INSERT INTO u VALUES(10),(20),(30);
SELECT (SELECT max(a) FROM u) FROM t;
SELECT (SELECT max(a)+max(b) FROM u) FROM t;
SELECT (SELECT count(a) FROM u) FROM t;

-- case: research/mine/alter-rename-column-in-trigger-and-view
CREATE TABLE t(a, b);
CREATE TABLE log(x);
CREATE VIEW v AS SELECT a, b FROM t WHERE a > 0;
CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log VALUES(new.a + new.b); END;
ALTER TABLE t RENAME COLUMN a TO aa;
INSERT INTO t VALUES(1, 2);
SELECT * FROM v;
SELECT * FROM log;
SELECT sql FROM sqlite_schema WHERE name IN ('v','tr') ORDER BY name;

-- case: research/mine/alter-rename-table-referenced-by-fk-and-view
PRAGMA foreign_keys = 1;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid REFERENCES p(id));
CREATE VIEW vp AS SELECT id FROM p;
ALTER TABLE p RENAME TO parent;
SELECT sql FROM sqlite_schema ORDER BY name;
INSERT INTO parent VALUES(1);
INSERT INTO c VALUES(1);
INSERT INTO c VALUES(2);
SELECT * FROM vp;

-- case: research/mine/alter-drop-column-with-index-refused
CREATE TABLE t(a, b, c);
CREATE INDEX ti ON t(b);
ALTER TABLE t DROP COLUMN b;
ALTER TABLE t DROP COLUMN c;
INSERT INTO t VALUES(1, 2);
SELECT * FROM t;
SELECT sql FROM sqlite_schema WHERE name = 't';

-- case: research/mine/alter-drop-column-used-in-view-refused
CREATE TABLE t(a, b);
CREATE VIEW v AS SELECT b FROM t;
ALTER TABLE t DROP COLUMN b;
SELECT name FROM pragma_table_info('t');

-- case: research/mine/alter-add-column-constraints
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
ALTER TABLE t ADD COLUMN b NOT NULL;
ALTER TABLE t ADD COLUMN b NOT NULL DEFAULT 5;
ALTER TABLE t ADD COLUMN c UNIQUE;
ALTER TABLE t ADD COLUMN d PRIMARY KEY;
ALTER TABLE t ADD COLUMN e DEFAULT (1+1);
ALTER TABLE t ADD COLUMN f DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE t ADD COLUMN g AS (a*2);
ALTER TABLE t ADD COLUMN h AS (a*2) STORED;
ALTER TABLE t ADD COLUMN i REFERENCES t(a) DEFAULT 3;
SELECT * FROM t;

-- case: research/mine/generated-columns-basic
CREATE TABLE t(a INTEGER, b AS (a * 2), c TEXT AS (upper(d)) STORED, d TEXT);
INSERT INTO t(a, d) VALUES(3, 'x'), (NULL, NULL);
SELECT a, b, c, d, typeof(b) FROM t;
UPDATE t SET a = a + 1, d = 'yy';
SELECT a, b, c, d FROM t;
INSERT INTO t(a, b) VALUES(1, 2);
CREATE INDEX tb ON t(b);
SELECT a FROM t WHERE b = 8;
PRAGMA table_xinfo(t);

-- case: research/mine/generated-column-in-unique-and-upsert
CREATE TABLE t(a, b AS (a % 3) UNIQUE);
INSERT INTO t(a) VALUES(1);
INSERT INTO t(a) VALUES(4);
INSERT OR REPLACE INTO t(a) VALUES(7);
SELECT a, b FROM t;
INSERT INTO t(a) VALUES(10) ON CONFLICT(b) DO UPDATE SET a = excluded.a + 100;
SELECT a, b FROM t;

-- case: research/mine/upsert-returning-and-changes
CREATE TABLE t(k PRIMARY KEY, v, n DEFAULT 0);
INSERT INTO t VALUES('a', 1, 0) RETURNING *;
INSERT INTO t VALUES('a', 2, 0) ON CONFLICT(k) DO UPDATE SET v = excluded.v, n = n + 1 RETURNING k, v, n;
INSERT INTO t VALUES('a', 3, 0) ON CONFLICT(k) DO NOTHING RETURNING *;
SELECT changes(), total_changes();
INSERT INTO t VALUES('b', 1, 0), ('a', 9, 0) ON CONFLICT DO UPDATE SET v = v * 10 WHERE excluded.v > 5 RETURNING *;
SELECT * FROM t ORDER BY k;

-- case: research/mine/upsert-multiple-targets
CREATE TABLE t(a UNIQUE, b UNIQUE, c);
INSERT INTO t VALUES(1, 1, 'x');
INSERT INTO t VALUES(1, 2, 'y') ON CONFLICT(b) DO UPDATE SET c = 'b' ON CONFLICT(a) DO UPDATE SET c = 'a';
INSERT INTO t VALUES(3, 1, 'z') ON CONFLICT(a) DO UPDATE SET c = 'a2' ON CONFLICT DO UPDATE SET c = 'any';
SELECT * FROM t;

-- case: research/mine/upsert-on-without-rowid-and-partial-unique
CREATE TABLE t(a, b, c, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE UNIQUE INDEX tb ON t(b) WHERE c IS NOT NULL;
INSERT INTO t VALUES(1, 10, 'x');
INSERT INTO t VALUES(2, 10, NULL);
INSERT INTO t VALUES(3, 10, 'y') ON CONFLICT(b) WHERE c IS NOT NULL DO UPDATE SET c = 'z';
INSERT INTO t VALUES(1, 99, 'q') ON CONFLICT(a) DO UPDATE SET b = excluded.b;
SELECT * FROM t ORDER BY a;
PRAGMA integrity_check;

-- case: research/mine/returning-with-triggers-and-defaults
CREATE TABLE t(id INTEGER PRIMARY KEY, a DEFAULT 5, b);
CREATE TABLE log(x);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(new.a); END;
INSERT INTO t(b) VALUES(1) RETURNING id, a, b, (SELECT count(*) FROM log);
UPDATE t SET b = b + 1 RETURNING *, old.b;
DELETE FROM t RETURNING id;
SELECT * FROM log;

-- case: research/mine/returning-errors
CREATE TABLE t(a);
INSERT INTO t VALUES(1) RETURNING count(*);
INSERT INTO t VALUES(1) RETURNING (SELECT max(a) FROM t);
INSERT INTO t VALUES(1) RETURNING row_number() OVER ();

-- case: research/mine/savepoint-nesting
CREATE TABLE t(a);
BEGIN;
INSERT INTO t VALUES(1);
SAVEPOINT s1;
INSERT INTO t VALUES(2);
SAVEPOINT s2;
INSERT INTO t VALUES(3);
ROLLBACK TO s1;
INSERT INTO t VALUES(4);
RELEASE s1;
COMMIT;
SELECT * FROM t;
SAVEPOINT outer1;
INSERT INTO t VALUES(5);
ROLLBACK TO outer1;
SELECT count(*) FROM t;
RELEASE outer1;
RELEASE nosuch;
ROLLBACK;

-- case: research/mine/savepoint-release-commits-outer
CREATE TABLE t(a);
SAVEPOINT a;
INSERT INTO t VALUES(1);
SAVEPOINT b;
INSERT INTO t VALUES(2);
RELEASE a;
ROLLBACK;
SELECT * FROM t;

-- case: research/mine/transaction-errors
BEGIN;
BEGIN;
COMMIT;
COMMIT;
ROLLBACK;
END;
BEGIN IMMEDIATE;
BEGIN EXCLUSIVE;
ROLLBACK;

-- case: research/mine/check-constraint-variants
CREATE TABLE t(a CHECK(a > 0), b CHECK(b IS NULL OR length(b) < 3), CONSTRAINT named CHECK(a <> 5));
INSERT INTO t VALUES(1, 'ab');
INSERT INTO t VALUES(0, 'ab');
INSERT INTO t VALUES(2, 'abc');
INSERT INTO t VALUES(5, NULL);
INSERT INTO t VALUES(NULL, NULL);
INSERT INTO t VALUES('x', NULL);
UPDATE t SET a = -1;
SELECT * FROM t;
PRAGMA ignore_check_constraints = 1;
INSERT INTO t VALUES(-5, 'abcdef');
SELECT count(*) FROM t;

-- case: research/mine/fk-deferred-and-cascade
PRAGMA foreign_keys = 1;
CREATE TABLE p(id INTEGER PRIMARY KEY, v);
CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED);
CREATE TABLE g(cid REFERENCES c(id) ON DELETE SET NULL);
INSERT INTO p VALUES(1, 'a'), (2, 'b');
INSERT INTO c VALUES(10, 1), (20, 2);
INSERT INTO g VALUES(10), (20);
BEGIN;
INSERT INTO c VALUES(30, 3);
INSERT INTO p VALUES(3, 'c');
COMMIT;
UPDATE p SET id = 100 WHERE id = 1;
DELETE FROM p WHERE id = 2;
SELECT * FROM c ORDER BY id;
SELECT * FROM g ORDER BY cid;
BEGIN;
INSERT INTO c VALUES(40, 4);
COMMIT;
SELECT count(*) FROM c;
PRAGMA foreign_key_check;

-- case: research/mine/fk-set-default-and-restrict
PRAGMA foreign_keys = 1;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid DEFAULT 0 REFERENCES p(id) ON DELETE SET DEFAULT);
CREATE TABLE r(pid REFERENCES p(id) ON DELETE RESTRICT);
INSERT INTO p VALUES(0), (1), (2);
INSERT INTO c VALUES(1);
INSERT INTO r VALUES(2);
DELETE FROM p WHERE id = 1;
SELECT * FROM c;
DELETE FROM p WHERE id = 2;
DELETE FROM p WHERE id = 0;
SELECT * FROM p ORDER BY id;

-- case: research/mine/fk-composite-and-mismatch
PRAGMA foreign_keys = 1;
CREATE TABLE p(a, b, PRIMARY KEY(a, b));
CREATE TABLE c(x, y, FOREIGN KEY(x, y) REFERENCES p(a, b));
INSERT INTO p VALUES(1, 2);
INSERT INTO c VALUES(1, 2);
INSERT INTO c VALUES(1, 3);
INSERT INTO c VALUES(NULL, 3);
CREATE TABLE bad(x REFERENCES p(nosuch));
INSERT INTO bad VALUES(1);

-- case: research/mine/date-functions-edges
SELECT date('2024-02-29', '+1 year'), date('2024-01-31', '+1 month'), date('2023-03-31', '-1 month'), datetime('2024-01-01 12:00', '+1.5 hours'), date('now') IS NOT NULL;
SELECT julianday('2000-01-01'), unixepoch('1970-01-02'), strftime('%j %W %w %u %s', '2024-12-31'), strftime('%f', '2024-01-01 00:00:01.123456');
SELECT date('2024-13-01'), date('2024-02-30'), date(''), date(NULL), datetime(2460000.5), datetime(1700000000, 'unixepoch'), datetime(1700000000.5, 'unixepoch', 'subsec');
SELECT time('12:34:56.789'), time('24:00:00'), datetime('2024-01-01T10:00:00Z'), datetime('2024-01-01 10:00:00+02:00'), datetime('2024-01-01 10:00:00 -05:30');
SELECT date('2024-01-15', 'start of month', '+1 month', '-1 day'), date('2024-01-15', 'weekday 0'), date('2024-01-14', 'weekday 0'), datetime('2024-01-01', '+10 minutes', 'start of day');
SELECT strftime('%Y-%m-%d', '0000-01-01'), date('-0001-01-01'), date('9999-12-31', '+1 day'), timediff('2024-03-01', '2024-01-31'), date('2024-01-01', 'ceiling'), date('2024-01-31', '+1 month', 'floor');

-- case: research/mine/date-julian-and-auto
SELECT datetime(0, 'auto'), datetime(1e10, 'auto'), datetime(2440587.5, 'auto'), datetime('2440587.5', 'julianday'), datetime(86400, 'unixepoch', 'localtime') IS NOT NULL, julianday('2024-06-01 12:00:00.5');

-- case: research/mine/strftime-all-specifiers
SELECT strftime('%a %A %b %B %c %C %d %D %e %F %g %G %H %I %j %J %k %l %m %M %p %P %R %s %S %T %u %U %V %w %W %x %X %y %Y %%', '2024-07-04 15:16:17.5');

-- case: research/mine/string-functions-unicode
SELECT length('héllo'), upper('héllo'), lower('ÀÉ'), substr('héllo', 2, 2), instr('héllo', 'l'), replace('ééé', 'é', 'e'), trim('  x  '), ltrim('xxyxx', 'x'), rtrim('xxyxx', 'xy'), hex('é'), unicode('é'), char(233), reverse('abc') IS NULL;

-- case: research/mine/string-functions-edges
SELECT substr('abc', 0), substr('abc', -1), substr('abc', 1, 0), substr(NULL, 1), substr('abc', NULL), instr('', ''), instr('abc', ''), instr(NULL, 'a'), replace('abc', '', 'x'), replace('aaa', 'a', ''), trim('xxaxx', ''), concat('a', NULL, 1), concat_ws('-', 'a', NULL, 'b'), concat_ws(NULL, 'a'), octet_length('é'), unhex('41ZZ'), unhex('41 42', ' '), format('%5.1s|', 'abc');

-- case: research/mine/like-case-sensitive-pragma
CREATE TABLE t(a);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES('abc'), ('ABC'), ('Abd');
PRAGMA case_sensitive_like = 1;
SELECT a FROM t WHERE a LIKE 'ab%' ORDER BY a;
PRAGMA case_sensitive_like = 0;
SELECT a FROM t WHERE a LIKE 'ab%' ORDER BY a;

-- case: research/mine/view-on-view-chain
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
CREATE VIEW v1 AS SELECT a, a*2 AS b FROM t;
CREATE VIEW v2 AS SELECT a, b, b+1 AS c FROM v1 WHERE a > 1;
CREATE VIEW v3 AS SELECT sum(c) AS s, count(*) AS n FROM v2;
CREATE VIEW v4 AS SELECT s, n, s/n AS avg FROM v3;
SELECT * FROM v4;
SELECT * FROM v2 JOIN v1 USING(a) ORDER BY a;
DROP TABLE t;
SELECT * FROM v4;

-- case: research/mine/view-recursive-reference
CREATE VIEW v AS SELECT 1 AS a;
DROP VIEW v;
CREATE VIEW v AS SELECT * FROM v;
SELECT * FROM v;

-- case: research/mine/cte-shadowing-and-recursion
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
WITH t(a) AS (SELECT 5) SELECT a FROM t;
WITH x AS (SELECT a FROM t), y AS (SELECT a+1 AS a FROM x) SELECT * FROM x, y;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n < 5) SELECT group_concat(n) FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r ORDER BY n DESC LIMIT 5) SELECT group_concat(n) FROM r;
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r WHERE n < 3 UNION ALL SELECT n+10 FROM r WHERE n < 3) SELECT n FROM r ORDER BY n;
WITH RECURSIVE r(n) AS (SELECT n FROM r) SELECT * FROM r;

-- case: research/mine/cte-recursive-aggregate-refused
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT count(*) FROM r) SELECT * FROM r;

-- case: research/mine/strict-and-without-rowid-combo
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE, v ANY) STRICT, WITHOUT ROWID;
INSERT INTO t VALUES(1, 'A', 1.5), (2, 'b', x'00'), (3, 'c', 'text');
INSERT INTO t VALUES('4', 'd', 1);
INSERT INTO t VALUES(5.5, 'e', 1);
SELECT id, typeof(id), name, typeof(v) FROM t WHERE name = 'a' OR name = 'B' ORDER BY id;
UPDATE t SET id = 10 WHERE name = 'c';
SELECT id FROM t ORDER BY id;

-- case: research/mine/integer-primary-key-variants
CREATE TABLE a(id INTEGER PRIMARY KEY DESC, v);
CREATE TABLE b(id INT PRIMARY KEY, v);
CREATE TABLE c(id INTEGER, v, PRIMARY KEY(id));
CREATE TABLE d(id INTEGER PRIMARY KEY ASC, v);
INSERT INTO a(v) VALUES('x'); INSERT INTO b(v) VALUES('x'); INSERT INTO c(v) VALUES('x'); INSERT INTO d(v) VALUES('x');
SELECT id, rowid FROM a; SELECT id, rowid FROM b; SELECT id, rowid FROM c; SELECT id, rowid FROM d;
INSERT INTO b VALUES('7', 'y');
SELECT id, typeof(id) FROM b ORDER BY rowid;

-- case: research/mine/rowid-aliases
CREATE TABLE t(a);
INSERT INTO t VALUES('x');
SELECT rowid, _rowid_, oid FROM t;
CREATE TABLE u(rowid, a);
INSERT INTO u VALUES(5, 'y');
SELECT rowid, _rowid_, oid, a FROM u;
UPDATE t SET rowid = 10;
SELECT rowid FROM t;

-- case: research/mine/insert-select-same-table
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 'x'), (2, 'y');
INSERT INTO t(b) SELECT b FROM t;
SELECT * FROM t;
INSERT INTO t SELECT a + 10, b FROM t ORDER BY a DESC LIMIT 2;
SELECT count(*) FROM t;

-- case: research/mine/update-from-aggregate
CREATE TABLE inv(id INTEGER PRIMARY KEY, qty);
CREATE TABLE sales(item, n);
INSERT INTO inv VALUES(1, 10), (2, 20);
INSERT INTO sales VALUES(1, 3), (1, 2), (2, 5);
UPDATE inv SET qty = qty - s.total FROM (SELECT item, sum(n) AS total FROM sales GROUP BY item) AS s WHERE s.item = inv.id;
SELECT * FROM inv;

-- case: research/mine/delete-with-subquery-on-self
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4),(5);
DELETE FROM t WHERE a > (SELECT avg(a) FROM t);
SELECT * FROM t;
DELETE FROM t WHERE a IN (SELECT a FROM t ORDER BY a DESC LIMIT 1);
SELECT * FROM t;

-- case: research/mine/attach-and-qualified-names
ATTACH ':memory:' AS aux;
CREATE TABLE aux.t(a);
CREATE TABLE main.t(b);
INSERT INTO aux.t VALUES(1);
INSERT INTO t VALUES(2);
SELECT * FROM t;
SELECT * FROM aux.t;
SELECT a, b FROM aux.t, main.t;
CREATE TEMP TABLE t(c);
INSERT INTO t VALUES(3);
SELECT * FROM t;
SELECT name FROM pragma_database_list ORDER BY seq;
DETACH aux;
SELECT * FROM aux.t;

-- case: research/mine/temp-trigger-on-main-table
CREATE TABLE t(a);
CREATE TABLE log(x);
CREATE TEMP TRIGGER tr AFTER INSERT ON main.t BEGIN INSERT INTO log VALUES(new.a); END;
INSERT INTO t VALUES(7);
SELECT * FROM log;

-- case: research/mine/vacuum-into-and-reopen
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
VACUUM INTO 'copy.db';
ATTACH 'copy.db' AS c;
SELECT count(*) FROM c.t;
VACUUM INTO 'copy.db';

-- case: research/mine/integrity-and-quick-check
CREATE TABLE t(a UNIQUE, b);
CREATE INDEX tb ON t(b);
INSERT INTO t VALUES(1, 'x'), (2, 'y');
PRAGMA integrity_check;
PRAGMA quick_check;
PRAGMA integrity_check(1);
PRAGMA integrity_check(t);
PRAGMA main.integrity_check;

-- case: research/mine/pragma-table-info-variants
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT NOT NULL DEFAULT 'x', c REAL, "d e" BLOB, f, g AS (a+1));
SELECT * FROM pragma_table_info('t');
SELECT * FROM pragma_table_xinfo('t');
PRAGMA index_list(t);
CREATE UNIQUE INDEX tbc ON t(b DESC, c COLLATE NOCASE) WHERE c > 0;
SELECT * FROM pragma_index_list('t');
SELECT * FROM pragma_index_info('tbc');
SELECT * FROM pragma_index_xinfo('tbc');

-- case: research/mine/pragma-function-and-module-list
SELECT count(*) > 50 FROM pragma_function_list;
SELECT name FROM pragma_module_list WHERE name IN ('fts5', 'json_each', 'generate_series', 'rtree') ORDER BY name;
SELECT name, builtin, type, narg FROM pragma_function_list WHERE name = 'substr' ORDER BY narg;

-- case: research/mine/collate-in-various-places
CREATE TABLE t(a TEXT);
INSERT INTO t VALUES('b'), ('A'), ('a'), ('B');
SELECT a FROM t ORDER BY a COLLATE NOCASE, a;
SELECT DISTINCT a COLLATE NOCASE FROM t ORDER BY 1;
SELECT count(DISTINCT a COLLATE NOCASE) FROM t;
SELECT a FROM t WHERE a = 'a' COLLATE NOCASE ORDER BY a;
SELECT a FROM t WHERE a COLLATE NOCASE IN ('A') ORDER BY a;
SELECT a FROM t GROUP BY a COLLATE NOCASE ORDER BY 1;
SELECT max(a COLLATE NOCASE), min(a COLLATE NOCASE) FROM t;
SELECT a FROM t WHERE a BETWEEN 'a' AND 'b' COLLATE NOCASE ORDER BY a;
SELECT 'a' = 'A' COLLATE NOCASE, 'a' COLLATE NOCASE = 'A', ('a' COLLATE BINARY) = ('A' COLLATE NOCASE);

-- case: research/mine/collate-unknown
SELECT 'a' COLLATE nosuch;
CREATE TABLE t(a COLLATE nosuch);

-- case: research/mine/numeric-text-in-index-order
CREATE TABLE t(a);
CREATE INDEX ti ON t(a);
INSERT INTO t VALUES('10'), (9), ('9'), (10), ('abc'), (9.5), (x'39'), (NULL);
SELECT quote(a) FROM t ORDER BY a;
SELECT quote(a) FROM t WHERE a > 9 ORDER BY a;
SELECT quote(a) FROM t WHERE a > '9' ORDER BY a;
SELECT quote(a) FROM t WHERE a = 9;

-- case: research/mine/aggregates-distinct-filter-orderby
CREATE TABLE t(g, v);
INSERT INTO t VALUES(1,'b'),(1,'a'),(1,'b'),(2,'c'),(2,NULL);
SELECT g, group_concat(DISTINCT v), group_concat(v ORDER BY v DESC), count(v) FILTER (WHERE v > 'a'), string_agg(v, ';' ORDER BY v) FROM t GROUP BY g;
SELECT count(DISTINCT v) FILTER (WHERE g = 1), sum(DISTINCT g), avg(DISTINCT g) FROM t;
SELECT group_concat(DISTINCT v, '-') FROM t;

-- case: research/mine/aggregate-with-no-rows-and-groups
CREATE TABLE t(a, b);
SELECT count(*), sum(a), max(a), group_concat(a), total(a), avg(a) FROM t;
SELECT count(*) FROM t GROUP BY a;
SELECT a, count(*) FROM t GROUP BY a HAVING count(*) > 0;
SELECT 1 FROM t HAVING count(*) = 0;
SELECT max(a), a FROM t;

-- case: research/mine/distinct-on-null-and-real-int
CREATE TABLE t(a);
INSERT INTO t VALUES(1), (1.0), ('1'), (NULL), (NULL), (0.0), (-0.0), (0);
SELECT DISTINCT quote(a) FROM t ORDER BY 1;
SELECT DISTINCT a FROM t ORDER BY quote(a);
SELECT count(DISTINCT a) FROM t;
SELECT a, count(*) FROM t GROUP BY a ORDER BY quote(a);

-- case: research/mine/compound-type-and-dup-rules
SELECT 1 UNION SELECT 1.0;
SELECT 1 UNION SELECT '1';
SELECT NULL UNION SELECT NULL;
SELECT 1 INTERSECT SELECT 1.0;
SELECT 1, 2 UNION SELECT 1;
SELECT 0.0 UNION SELECT -0.0;
SELECT x'01' UNION SELECT x'01' UNION SELECT '01';

-- case: research/mine/in-with-subquery-limit-and-order
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
SELECT a FROM t WHERE a IN (SELECT a FROM t ORDER BY a DESC LIMIT 2) ORDER BY a;
SELECT a FROM t WHERE a NOT IN (SELECT a FROM t LIMIT 0) ORDER BY a;
SELECT a FROM t WHERE a IN (SELECT a FROM t LIMIT 1 OFFSET 2);
SELECT (SELECT a FROM t ORDER BY a LIMIT 1 OFFSET 10);

-- case: research/mine/values-and-select-star-edge
VALUES(1), (2, 3);
SELECT * FROM (VALUES(1, 2)) WHERE column1 = 1;
SELECT column1, column2 FROM (VALUES(1, 'a'), (2, 'b')) ORDER BY column2 DESC;
SELECT *;
SELECT t.* FROM (SELECT 1 AS a) AS t;
SELECT x.* FROM (SELECT 1 AS a) AS t;

-- case: research/mine/expression-misc
SELECT coalesce(NULL, NULL), coalesce(NULL, 2, 1/0), ifnull(NULL, 'x'), nullif(1, 1), nullif(1, 2), iif(1, 'y', 'n'), iif(NULL, 'y', 'n'), likelihood(1, 0.5), likely(0), unlikely(1), typeof(NULL), coalesce(1);
SELECT CASE 1 WHEN 1.0 THEN 'eq' ELSE 'ne' END, CASE '1' WHEN 1 THEN 'eq' ELSE 'ne' END, CASE NULL WHEN NULL THEN 'eq' ELSE 'ne' END, CASE WHEN NULL THEN 1 END;
SELECT max(1, 'a', NULL), min(1, 'a'), max(), min(NULL);

-- case: research/mine/max-min-scalar-errors
SELECT max();
SELECT min(1);
SELECT coalesce();
SELECT nullif(1);
