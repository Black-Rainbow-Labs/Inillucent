-- case: orm/8/8.1-collate-nocase-unique-emails
CREATE TABLE u(email TEXT NOT NULL UNIQUE COLLATE NOCASE); INSERT INTO u VALUES('A@x.com'); INSERT INTO u VALUES('a@X.COM');
-- case: orm/8/8.2-nocase-only-folds-ascii
SELECT 'a'='A' COLLATE NOCASE, 'é'='É' COLLATE NOCASE, 'ß'='SS' COLLATE NOCASE, 'a' COLLATE NOCASE < 'B', 'a' < 'B';
-- case: orm/8/8.3-binary-nocase-rtrim-collations
SELECT 'a '='a' COLLATE RTRIM, 'a  '='a ' COLLATE RTRIM, 'a'='A', 'a' COLLATE BINARY = 'a';
-- case: orm/8/8.4-collation-precedence-left-column-collation-wins
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT); INSERT INTO t VALUES('x','X'); SELECT a=b, b=a, a=b COLLATE BINARY, b=a COLLATE NOCASE FROM t;
-- case: orm/8/8.5-unknown-collation-error
CREATE TABLE t(a TEXT COLLATE UNICODE);
-- case: orm/8/8.6-unknown-collation-in-expression-error
SELECT 'a' < 'b' COLLATE nope;
-- case: orm/8/8.7-order-by-collation-default-binary-is-codepoint-ordering
CREATE TABLE t(a); INSERT INTO t VALUES('b'),('B'),('a'),('A'),('é'),('Z'); SELECT group_concat(a, '') FROM (SELECT a FROM t ORDER BY a); SELECT group_concat(a, '') FROM (SELECT a FROM t ORDER BY a COLLATE NOCASE);
-- case: orm/8/8.8-like-is-case-insensitive-for-ascii-only-glob-case-sensitive
SELECT 'ABC' LIKE 'abc', 'é' LIKE 'É', 'abc' GLOB 'A*', 'abc' GLOB 'a*', 'a_c' LIKE 'a\_c' ESCAPE '\', 'abc' LIKE 'a%', 'ab' LIKE 'a_', NULL LIKE 'a';
-- case: orm/8/8.9-like-with-numbers-and-blobs-operands-cast-to-text
SELECT 123 LIKE '1%', 1.5 LIKE '1.%', x'41' LIKE 'A';
-- case: orm/8/8.10-pragma-case-sensitive-like-changes-like
PRAGMA case_sensitive_like=ON; SELECT 'A' LIKE 'a'; PRAGMA case_sensitive_like=OFF; SELECT 'A' LIKE 'a';
-- case: orm/8/8.11-like-pattern-with-escape-longer-than-one-char-error
SELECT 'a' LIKE 'a' ESCAPE 'ab';
-- case: orm/8/8.12-regexp-without-function-installed
SELECT 'abc' REGEXP 'a.c';
-- case: orm/8/8.13-string-functions-substr-negative-start-0-start-length-args
SELECT substr('hello',2), substr('hello',-3), substr('hello',2,3), substr('hello',0), substr('hello',0,2), substr('hello',-3,2), substr('hello',2,-1), substr('hello',10), substr(12345,2,2), substr(x'414243',2,1);
-- case: orm/8/8.14-instr-replace-trim-family-upper-lower-non-ascii
SELECT instr('hello','l'), instr('hello','z'), instr(x'0102',x'02'), replace('aaa','a','bb'), replace('abc','',''), trim('  x  '), ltrim('xxay','x'), rtrim('ayxx','xy'), trim('abcba','ab'), upper('é'), lower('É'), upper('straße');
-- case: orm/8/8.15-length-chars-for-text-bytes-for-blob-stops-at-nul
SELECT length('héllo'), length(x'00ff'), length(NULL), length(123), length(1.50), length('a'||char(0)||'b'), octet_length('é'), length(12.0);
-- case: orm/8/8.16-char-unicode-printf-format
SELECT char(72,105), unicode('A'), unicode(''), format('%05.1f|%d|%s|%x|%5s|%-5s|%q|%Q|%%', 3.14159, 42, 'hi', 255, 'ab', 'ab', 'it''s', NULL), printf('%d', '12abc'), printf('%s', 1.0), printf('%.3s','abcdef'), printf('%c', 65), printf('%i',3.9);
-- case: orm/8/8.17-quote-for-each-type
SELECT quote(1), quote(1.5), quote('it''s'), quote(NULL), quote(x'00ff'), quote(1e100), quote(0.1);
-- case: orm/8/8.18-concat-concat-ws-3-44-treat-null-as-empty
SELECT concat('a',NULL,1), concat_ws('-','a',NULL,'b'), concat_ws(NULL,'a','b'), 'a'||NULL;
-- case: orm/8/8.19-utf-8-text-stored-and-returned-invalid-utf-8-bytes-in-text
CREATE TABLE t(a TEXT); INSERT INTO t VALUES('日本語'),(CAST(x'ff' AS TEXT)); SELECT length(a), hex(a), typeof(a) FROM t;
-- case: orm/8/8.20-soundex-absent-zeroblob-sqlite-compileoption-used
SELECT typeof(sqlite_compileoption_used('ENABLE_FTS5')), typeof(sqlite_compileoption_get(0));
-- case: orm/8/8.21-case-insensitive-identifiers-and-quoting-styles
CREATE TABLE Foo(Bar INT); INSERT INTO foo(BAR) VALUES(1); SELECT "BAR", [bar], `bar`, FOO.bar FROM Foo; SELECT name FROM sqlite_master;
-- case: orm/8/8.22-keyword-as-column-name-and-reserved-word-table-names
CREATE TABLE "order"("group" INT, "select" TEXT); INSERT INTO "order" VALUES(1,'x'); SELECT "group", "select" FROM "order";
-- case: orm/8/8.23-unquoted-keyword-as-table-name-is-a-syntax-error
CREATE TABLE order(a);
-- case: orm/8/8.24-non-reserved-keywords-usable-as-identifiers-key-action-temp
CREATE TABLE t(key INT, action TEXT, temp INT, "index" INT); INSERT INTO t VALUES(1,'a',2,3); SELECT key, action, temp FROM t;
-- case: orm/8/8.25-identifier-length-and-unicode-names
CREATE TABLE "tabla_ñ"("колонка" INT); INSERT INTO "tabla_ñ" VALUES(1); SELECT * FROM "tabla_ñ"; PRAGMA table_info("tabla_ñ");
