-- case: text/length-and-substr-unicode
SELECT length('héllo'), length(x'68c3a9'), length(CAST('héllo' AS BLOB)), octet_length('héllo'), substr('héllo', 2, 2), substr('héllo', -2), substr('abc', 0), substr('abc', 0, 2), substr('abc', -5, 3), substr('abc', 2, -1), substr('abc', 4), substring('abcdef', 3);
SELECT substr(x'01020304', 2, 2), typeof(substr(x'01020304', 2, 2)), substr(NULL, 1), substr('abc', NULL), length(NULL), length(123), length(1.50);
-- case: text/upper-lower-non-ascii
SELECT upper('straße'), lower('ÀÉÎ'), upper('ﬁ'), lower('ΑΒΓ'), upper(NULL), upper(12);
-- case: text/trim-family
SELECT trim('  a  '), ltrim('xxaxx', 'x'), rtrim('xxaxx', 'x'), trim('abcba', 'ab'), trim(''), trim(NULL), trim('  a  ', NULL), '[' || trim(char(9) || 'a' || char(10)) || ']';
-- case: text/replace-instr
SELECT replace('aaa', 'a', 'bb'), replace('abc', '', 'x'), replace('abc', 'b', NULL), instr('hello', 'l'), instr('hello', ''), instr('', 'a'), instr(NULL, 'a'), instr(x'0102', x'02'), instr('héllo', 'l');
-- case: text/concat-operators
SELECT 'a' || NULL, concat('a', NULL, 'b'), concat_ws(',', 'a', NULL, 'b'), concat_ws(NULL, 'a'), 1 || 2, typeof(1 || 2), 1.5 || 'x', x'41' || 'b', concat();
-- case: text/char-unicode-hex-quote
SELECT char(72, 105), char(), unicode('é'), unicode(''), hex('é'), hex(12), hex(NULL), unhex('4142'), unhex('41 42'), unhex('4142', ' '), unhex('zz'), quote('it''s'), quote(x'00ff'), quote(NULL), quote(1.5);
-- case: text/like-semantics
SELECT 'abc' LIKE 'ABC', 'abc' LIKE 'a%', 'abc' LIKE 'a_c', 'a%c' LIKE 'a\%c' ESCAPE '\', 'abc' LIKE 'a\%c' ESCAPE '\', 'é' LIKE 'É', 'É' LIKE 'é', '' LIKE '', NULL LIKE 'a', 'a' LIKE NULL, 'abc' NOT LIKE 'b%';
SELECT 10 LIKE '1%', 'a_b' LIKE 'a/_b' ESCAPE '/', 'aXb' LIKE 'a/_b' ESCAPE '/', like('a%', 'abc'), like('a$%', 'a%', '$');
SELECT 'abc' LIKE 'a' ESCAPE 'xx';
-- case: text/case-sensitive-like-pragma
PRAGMA case_sensitive_like = 1;
SELECT 'abc' LIKE 'ABC', 'abc' LIKE 'abc';
PRAGMA case_sensitive_like = 0;
SELECT 'abc' LIKE 'ABC';
-- case: text/glob-semantics
SELECT 'abc' GLOB 'a*', 'abc' GLOB 'A*', 'abc' GLOB 'a?c', 'a1c' GLOB 'a[0-9]c', 'abc' GLOB 'a[^0-9]c', 'a]c' GLOB 'a[]]c', 'a-c' GLOB 'a[a-]c', '*' GLOB '[*]', glob('a*', 'abc'), 'abc' GLOB '';
-- case: text/like-on-indexed-column
CREATE TABLE t(name TEXT);
CREATE INDEX tn ON t(name);
INSERT INTO t VALUES('apple'),('Apple'),('APPLE'),('banana'),('app'),('apricot'),(NULL),(5);
SELECT name FROM t WHERE name LIKE 'ap%' ORDER BY name;
SELECT name FROM t WHERE name GLOB 'ap*' ORDER BY name;
SELECT name FROM t WHERE name LIKE '5' ORDER BY name;
SELECT name FROM t WHERE name >= 'ap' AND name < 'aq' ORDER BY name;
-- case: text/collation-nocase-rtrim
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT COLLATE RTRIM, c TEXT);
INSERT INTO t VALUES('Abc', 'x  ', 'Abc'), ('abc', 'x', 'abc'), ('ÀBC', 'y ', 'àbc');
SELECT a = 'ABC', b = 'x', c = 'ABC', c = 'ABC' COLLATE NOCASE, a = 'àbc', 'x ' = b FROM t;
SELECT count(DISTINCT a), count(DISTINCT b), count(DISTINCT c) FROM t;
SELECT a FROM t GROUP BY a ORDER BY a;
SELECT a FROM t ORDER BY a COLLATE BINARY;
SELECT c FROM t ORDER BY c COLLATE NOCASE, c;
SELECT max(a), min(a) FROM t;
-- case: text/collation-unique-index
CREATE TABLE users(email TEXT COLLATE NOCASE UNIQUE);
INSERT INTO users VALUES('A@x.com');
INSERT INTO users VALUES('a@X.COM');
SELECT * FROM users WHERE email = 'a@x.com';
CREATE TABLE u2(email TEXT);
CREATE UNIQUE INDEX u2e ON u2(email COLLATE NOCASE);
INSERT INTO u2 VALUES('B@x.com');
INSERT INTO u2 VALUES('b@x.com');
SELECT * FROM u2 WHERE email = 'b@x.com';
SELECT * FROM u2 WHERE email = 'b@x.com' COLLATE NOCASE;
-- case: text/collation-precedence
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT COLLATE BINARY);
INSERT INTO t VALUES('x', 'X');
SELECT a = b, b = a, a = b COLLATE BINARY, a COLLATE BINARY = b, (a) = b, a || '' = b, +a = b FROM t;
SELECT 'x' IN (SELECT a FROM t), 'X' IN (SELECT a FROM t), a IN ('X') FROM t;
SELECT a BETWEEN 'W' AND 'Y', b BETWEEN 'w' AND 'y' FROM t;
-- case: text/collation-unknown-error
CREATE TABLE t(a TEXT COLLATE nosuch);
SELECT 'a' = 'A' COLLATE nosuch;
-- case: text/blob-text-comparison
SELECT x'61' = 'a', CAST(x'61' AS TEXT) = 'a', x'61' < 'a', 'a' < x'00', length(x'00112233'), typeof(x''), x'' = '';
-- case: text/embedded-nul
SELECT length('a' || char(0) || 'b'), hex('a' || char(0) || 'b'), length(CAST(x'610062' AS TEXT)), quote(CAST(x'610062' AS TEXT));
CREATE TABLE t(s TEXT);
INSERT INTO t VALUES(CAST(x'610062' AS TEXT));
SELECT length(s), hex(s), s = 'a' FROM t;
-- case: text/string-literal-forms
SELECT 'it''s', "nosuchcolumn", 'a' 'b';
-- case: text/double-quoted-string-fallback
CREATE TABLE t(a);
INSERT INTO t VALUES("hello");
SELECT a, "a", "b" FROM t;
SELECT * FROM t WHERE a = "hello";
-- case: text/soundex-and-friends
SELECT typeof(soundex('Robert'));
SELECT length(printf('%.5000s', 'x')), length(zeroblob(1000)), quote(substr(zeroblob(4), 1, 2));
-- case: text/string-comparison-trailing-space
SELECT 'a' = 'a ', 'a' < 'a ', 'A' < 'a', 'a' < 'B', 'Z' < 'a', '10' < '9', '' < ' ';
-- case: text/likelihood-argument-rules
SELECT likelihood(1, 0.5), likelihood(1, 0.0), likelihood(1, 1.0), likelihood(1, 1e0), likelihood(1, (0.5));
SELECT likelihood(1, 2);
SELECT likelihood(1, 0);
SELECT likelihood(1, 1);
SELECT likelihood(1, -0.1);
SELECT likelihood(1, +0.5);
SELECT likelihood(1, '0.5');
SELECT likelihood(1, NULL);
SELECT likelihood(1, 1 + 0.5);
SELECT likelihood(1, ?1);
SELECT likelihood(1, abs(0.5));
SELECT likelihood(1, TRUE);
SELECT likelihood(1);
SELECT likelihood(1, 0.5, 0.5);
SELECT likely(1, 0.5);
SELECT unlikely();
SELECT likely(7), unlikely(7), likelihood('a', 0.9), typeof(likely(NULL));
CREATE TABLE t(p);
SELECT likelihood(1, p) FROM t;
CREATE INDEX ix ON t(likelihood(p, 2));
-- case: text/like-and-glob-pattern-length-limit
SELECT 'test' LIKE REPLACE(ZEROBLOB(135000), x'00', 'a');
SELECT 'test' LIKE REPLACE(ZEROBLOB(50000), x'00', '%');
SELECT 'test' LIKE REPLACE(ZEROBLOB(50001), x'00', '%');
SELECT 'test' GLOB REPLACE(ZEROBLOB(50000), x'00', '*');
SELECT 'test' GLOB REPLACE(ZEROBLOB(50001), x'00', '*');
SELECT like(REPLACE(ZEROBLOB(50001), x'00', '%'), 'test');
SELECT glob(REPLACE(ZEROBLOB(50001), x'00', '*'), 'test');
SELECT like(REPLACE(ZEROBLOB(50000), x'00', '%'), 'test');
SELECT NULL LIKE REPLACE(ZEROBLOB(50001), x'00', '%');
