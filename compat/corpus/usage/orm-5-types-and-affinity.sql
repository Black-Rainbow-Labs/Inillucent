-- case: orm/5/5.1-column-affinity-from-declared-type-names-rules-1-to-5
CREATE TABLE t(a INT, b INTEGER, c TINYINT, d VARCHAR(10), e CLOB, f BLOB, g, h REAL, i DOUBLE, j FLOAT, k NUMERIC, l DECIMAL(10,5), m BOOLEAN, n DATE, o DATETIME, p CHARINT, q POINT, r STRING, s UNSIGNED BIG INT, t CHARACTER(20), u NVARCHAR(100), v TEXT); INSERT INTO t VALUES('1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1','1'); SELECT typeof(a),typeof(b),typeof(c),typeof(d),typeof(e),typeof(f),typeof(g),typeof(h),typeof(i),typeof(j),typeof(k),typeof(l),typeof(m),typeof(n),typeof(o),typeof(p),typeof(q),typeof(r),typeof(s),typeof(t),typeof(u),typeof(v) FROM t;
-- case: orm/5/5.2-integer-affinity-converts-text-that-looks-numeric-leaves-oth
CREATE TABLE t(a INTEGER); INSERT INTO t VALUES('42'),('4.0'),('4.5'),(' 7'),('7 '),('0x10'),('1e2'),('abc'),(''),(NULL); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.3-text-affinity-converts-numbers-to-text
CREATE TABLE t(a TEXT); INSERT INTO t VALUES(1),(1.0),(1.50),(1e20),(x'41'),(-0.0); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.4-real-affinity-converts-integers-to-float
CREATE TABLE t(a REAL); INSERT INTO t VALUES(1),('2'),('3.5'),(9007199254740993); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.5-numeric-affinity-1-0-stored-as-integer-big-values-as-real
CREATE TABLE t(a NUMERIC); INSERT INTO t VALUES('1.0'),(1.0),('1.5'),('9223372036854775807'),('9223372036854775808'),('1e3'),('  12  '),('1a'); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.6-decimal-10-2-has-numeric-affinity-no-rounding-trailing-zeros
CREATE TABLE t(p DECIMAL(10,2)); INSERT INTO t VALUES('19.990'),(19.999),('100'),('0.1'); SELECT p, typeof(p) FROM t; SELECT p+0.2 FROM t WHERE p=0.1;
-- case: orm/5/5.7-boolean-column-is-numeric-true-false-keywords-are-1-0-since-
CREATE TABLE t(b BOOLEAN); INSERT INTO t VALUES(TRUE),(FALSE),(1),(0),('true'),(2); SELECT b, typeof(b) FROM t;
-- case: orm/5/5.8-boolean-expressions-result-integers-0-1-null
SELECT 1=1, 1=2, NULL=1, NOT NULL, 5 AND 2, 0 OR NULL, 'a' AND 1, typeof(1=1);
-- case: orm/5/5.9-comparison-applies-affinity-of-column-to-the-other-operand
CREATE TABLE t(i INTEGER, tx TEXT, n); INSERT INTO t VALUES(1,'1','1'); SELECT i=1, i='1', tx=1, tx='1', n=1, n='1', 1='1', 1=1.0 FROM t;
-- case: orm/5/5.10-literal-compare-with-no-affinity-int-vs-text-never-equal
SELECT 1='1', 1<'1', 'a'<1, x'00'>'zzz', NULL<1;
-- case: orm/5/5.11-cast-rules-text-prefix-parsing
SELECT CAST('12abc' AS INTEGER), CAST('abc' AS INTEGER), CAST('1e2' AS INTEGER), CAST('1.9' AS INTEGER), CAST(1.9 AS INTEGER), CAST(-1.9 AS INTEGER), CAST('0x1A' AS INTEGER), CAST(' 5' AS INTEGER);
-- case: orm/5/5.12-cast-to-numeric-real-text-blob
SELECT CAST('3.0' AS NUMERIC), typeof(CAST('3.0' AS NUMERIC)), CAST('3' AS REAL), CAST(5 AS TEXT), CAST('ab' AS BLOB), typeof(CAST(1 AS BLOB)), CAST(x'3132' AS TEXT), CAST(1e30 AS INTEGER), CAST('9223372036854775808' AS INTEGER);
-- case: orm/5/5.13-cast-to-unknown-type-name-decimal-gets-numeric-rules
SELECT CAST('5.5' AS DECIMAL(10,2)), typeof(CAST('5' AS BIGINT)), CAST('1' AS BOOLEAN), typeof(CAST('x' AS FOO));
-- case: orm/5/5.14-integer-division-and-modulo-division-by-zero-is-null
SELECT 7/2, -7/2, 7%3, -7%3, 7.0/2, 1/0, 1%0, 1.0/0, 5/2.0, typeof(1/0);
-- case: orm/5/5.15-integer-overflow-converts-to-real-in-arithmetic
SELECT 9223372036854775807+1, typeof(9223372036854775807+1), -9223372036854775808, 9223372036854775807*2, abs(-9223372036854775807), 9223372036854775808;
-- case: orm/5/5.16-abs-min-int64-raises-integer-overflow
SELECT abs(-9223372036854775808);
-- case: orm/5/5.17-float-formatting-15-vs-17-digits-exponent-inf-0-0
SELECT 0.1+0.2, 1e100, 1.0, 100.0, 1e15, 1e16, 123456789012345678.0, 1.0/3, -0.0, 9e999, -9e999, 1e-5, 3.14159265358979323846;
-- case: orm/5/5.18-inf-minus-inf-is-null-not-nan
SELECT 9e999-9e999, typeof(9e999-9e999), 9e999 > 1e308, typeof(1e999), 9e999*0, 0.0/0.0;
-- case: orm/5/5.19-numeric-literal-forms-hex-underscore-3-46-leading-dot-expone
SELECT 0x10, 0xFFFFFFFFFFFFFFFF, .5, 5., 1e3, 1E-2, 1_000, typeof(1e3);
-- case: orm/5/5.20-blob-literal-forms-and-hex-unhex-length-of-blobs-and-text
SELECT x'4142', X'', typeof(x''), hex('AB'), hex(x'00ff'), length(x'0001'), length('héé'), octet_length('héé'), unhex('4142'), unhex('zz'), quote(x'ab');
-- case: orm/5/5.21-typeof-for-each-storage-class
SELECT typeof(1), typeof(1.0), typeof('a'), typeof(x'00'), typeof(NULL), typeof(1+1.0), typeof('1'+1), typeof(3/2), typeof(date('now'));
-- case: orm/5/5.22-text-arithmetic-coercion-abc-1-5-5-1-5e1-0
SELECT 'abc'+1, '5'+'5', '1.5e1'+0, '12abc'+0, ' 3 '+0, ''+1, NULL+1, '0x10'+0;
-- case: orm/5/5.23-string-concat-operator-coerces-numbers-null-propagates
SELECT 1||2, 'a'||NULL, 1.0||'x', 1e20||'', typeof(1||2), x'41'||'b';
-- case: orm/5/5.24-uuid-as-text-vs-blob-key-randomblob-hex-lower
SELECT length(randomblob(16)), typeof(randomblob(16)), length(hex(randomblob(16))), lower(hex(x'DEADBEEF')), length(lower(hex(randomblob(4)))), typeof(zeroblob(4)), length(zeroblob(4)), hex(zeroblob(3));
-- case: orm/5/5.25-integer-primary-key-is-the-rowid-alias-only-exactly-integer
CREATE TABLE a(id INTEGER PRIMARY KEY, v); CREATE TABLE b(id INT PRIMARY KEY, v); CREATE TABLE c(id INTEGER PRIMARY KEY DESC, v); INSERT INTO a(v) VALUES(1); INSERT INTO b(v) VALUES(1); INSERT INTO c(v) VALUES(1); SELECT id FROM a; SELECT id, rowid FROM b; SELECT id FROM c;
-- case: orm/5/5.26-integer-primary-key-rejects-non-integer
CREATE TABLE a(id INTEGER PRIMARY KEY, v); INSERT INTO a VALUES('abc', 1);
-- case: orm/5/5.27-integer-primary-key-accepts-5-text-as-5-and-null-as-auto
CREATE TABLE a(id INTEGER PRIMARY KEY, v); INSERT INTO a VALUES('5', 1); INSERT INTO a VALUES(NULL, 2); INSERT INTO a VALUES(5.0, 3);
-- case: orm/5/5.28-non-integer-primary-key-allows-null-legacy-bug
CREATE TABLE t(a TEXT PRIMARY KEY, b); INSERT INTO t VALUES(NULL,1),(NULL,2); SELECT count(*) FROM t;
-- case: orm/5/5.29-without-rowid-primary-key-not-null-enforced
CREATE TABLE t(a PRIMARY KEY, b) WITHOUT ROWID; INSERT INTO t VALUES(NULL,1);
-- case: orm/5/5.30-without-rowid-has-no-rowid-column
CREATE TABLE t(a PRIMARY KEY, b) WITHOUT ROWID; SELECT rowid FROM t;
-- case: orm/5/5.31-without-rowid-requires-primary-key
CREATE TABLE t(a, b) WITHOUT ROWID;
-- case: orm/5/5.32-strict-table-type-enforcement-and-coercion
CREATE TABLE t(i INT, r REAL, s TEXT, b BLOB, a ANY) STRICT; INSERT INTO t VALUES('5', 1, 'x', x'00', 'z'); SELECT i, typeof(i), r, typeof(r) FROM t; INSERT INTO t(i) VALUES('abc');
-- case: orm/5/5.33-strict-unknown-type-name-rejected-datetime-not-allowed
CREATE TABLE t(a DATETIME) STRICT;
-- case: orm/5/5.34-strict-integer-primary-key-and-null
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT) STRICT; INSERT INTO t(s) VALUES(5); SELECT s, typeof(s) FROM t; INSERT INTO t(s) VALUES(x'00');
-- case: orm/5/5.35-strict-column-without-type-rejected
CREATE TABLE t(a) STRICT;
-- case: orm/5/5.36-strict-without-rowid-combined
CREATE TABLE t(a INT PRIMARY KEY, b TEXT) STRICT, WITHOUT ROWID; INSERT INTO t VALUES(1,'x'); SELECT * FROM t;
-- case: orm/5/5.37-generated-column-virtual-vs-stored-cannot-insert-into-it
CREATE TABLE t(a INT, b INT GENERATED ALWAYS AS (a*2) STORED, c AS (a+b)); INSERT INTO t(a) VALUES(3); SELECT * FROM t; INSERT INTO t(a,b) VALUES(1,1);
-- case: orm/5/5.38-generated-column-cannot-be-updated
CREATE TABLE t(a INT, b AS (a*2)); INSERT INTO t(a) VALUES(3); UPDATE t SET b=5;
-- case: orm/5/5.39-default-expression-forms-negative-string-parenthesised-curre
CREATE TABLE t(a INT DEFAULT -1, b TEXT DEFAULT 'it''s', c INT DEFAULT (abs(-3)), d TEXT DEFAULT CURRENT_DATE, e DEFAULT TRUE, f DEFAULT 'x' NOT NULL); INSERT INTO t DEFAULT VALUES; SELECT a,b,c,length(d),e,typeof(e),f FROM t;
-- case: orm/5/5.40-hex-integer-to-column-text-that-looks-hex-stays-text-in-inte
CREATE TABLE t(a INTEGER); INSERT INTO t VALUES(0x10),('0x10'); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.41-very-large-integer-text-into-integer-becomes-real
CREATE TABLE t(a INTEGER); INSERT INTO t VALUES('9223372036854775807'),('9223372036854775808'),(12345678901234567890); SELECT a, typeof(a) FROM t;
-- case: orm/5/5.42-max-min-with-mixed-types-and-null-scalar-max-returns-null-if
SELECT max(1,2,3), max(1,NULL), min('a',1), max('a','B'), min(1,2.5), max(x'00','a');
-- case: orm/5/5.43-round-results-half-away-from-zero-and-binary-float-artefacts
SELECT round(2.5), round(-2.5), round(2.675,2), round(1234.5678,-2), typeof(round(5)), round(NULL), round('3.7'), round(0.5), round(1.005,2), round(5.5,0), round(1234.5678,2);
-- case: orm/5/5.44-math-functions-need-sqlite-enable-math-functions
@@SELECT pow(2,10), sqrt(16), log(100), log10(1000), log2(8), ln(1), mod(7,3), mod(-7,3), exp(0), sign(-5), ceil(1.2), floor(-1.2), trunc(-1.7), ceil(5), sqrt(-1), pi() > 3.14;@@(1024.0, 4.0, 2.0, 3.0, 3.0, 0.0, 1.0, -1.0, 1.0, -1, 2.0, -2.0, -1.0, 5, NULL, 1)
-- case: orm/5/5.45-random-and-randomblob-ranges
SELECT typeof(random()), random() IS NOT NULL, length(randomblob(0)), length(randomblob(-1)), typeof(randomblob(0));
