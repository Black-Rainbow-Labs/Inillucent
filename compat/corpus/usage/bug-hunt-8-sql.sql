-- case: bug-hunt-8/sql/glob-nonascii-class
SELECT 'é' GLOB '[é]', 'é' GLOB '[^a]', 'é' GLOB '[à-ü]', '日' GLOB '[日]', 'a日b' GLOB 'a[^b]b', 'aé' GLOB 'a[é]';
-- case: bug-hunt-8/sql/glob-nonascii-class-table
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT);
INSERT INTO t(s) VALUES('abc'),('abé'),('abd');
SELECT id FROM t WHERE s GLOB 'ab[^c]' ORDER BY id;
SELECT id FROM t WHERE s GLOB 'ab[é]' ORDER BY id;
-- case: bug-hunt-8/sql/numeric-affinity-integral-text-above-2p51
CREATE TABLE t(n NUMERIC, i INTEGER);
INSERT INTO t VALUES('1e15', '1e15'), ('4503599627370496.0', '4503599627370496.0'), ('1e16', '1e16'), ('1e18', '1e18'), ('9007199254740993.0', '9007199254740993.0');
SELECT typeof(n), n, typeof(i), i FROM t ORDER BY rowid;
-- case: bug-hunt-8/sql/foreign-key-integer-child-text-parent
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY, t TEXT UNIQUE);
CREATE TABLE c(a REFERENCES p(id), b REFERENCES p(t));
INSERT INTO p VALUES(1, '1');
INSERT INTO c(a) VALUES('1');
INSERT INTO c(b) VALUES(1);
SELECT * FROM c;
-- case: bug-hunt-8/sql/is-affinity-from-derived-table
CREATE TABLE t(id INTEGER PRIMARY KEY, s TEXT);
INSERT INTO t VALUES(1,'1'),(2,'2');
SELECT id FROM t, (SELECT 1 AS x) WHERE s IS x;
SELECT id FROM t, (SELECT 1 AS x) WHERE s IS NOT x ORDER BY id;
SELECT id FROM t, (SELECT 1 AS x) WHERE s = x;
WITH c(x) AS (SELECT 1) SELECT id FROM t, c WHERE s IS x;
-- case: bug-hunt-8/sql/strict-add-column-untyped
CREATE TABLE t(a INT, b TEXT) STRICT;
ALTER TABLE t ADD COLUMN d;
-- case: bug-hunt-8/sql/strict-add-column-unknown-type
CREATE TABLE t(a INT, b TEXT) STRICT;
ALTER TABLE t ADD COLUMN e NUM;
-- case: bug-hunt-8/sql/strict-add-column-varchar
CREATE TABLE t(a INT, b TEXT) STRICT;
ALTER TABLE t ADD COLUMN f VARCHAR(5);
-- case: bug-hunt-8/sql/strict-add-column-numeric
CREATE TABLE s (id INTEGER PRIMARY KEY) STRICT;
ALTER TABLE s ADD COLUMN y NUMERIC;
-- case: bug-hunt-8/sql/strict-add-column-ok
CREATE TABLE s (id INTEGER PRIMARY KEY) STRICT;
ALTER TABLE s ADD COLUMN y INTEGER;
ALTER TABLE s ADD COLUMN z ANY;
SELECT sql FROM sqlite_master;
-- case: bug-hunt-8/sql/detach-unused-database-inside-transaction
CREATE TABLE t (a);
BEGIN;
CREATE TABLE t2 (b);
ATTACH ':memory:' AS m2;
DETACH m2;
ROLLBACK;
SELECT name FROM sqlite_master;
-- case: bug-hunt-8/sql/detach-earlier-of-two-inside-transaction
BEGIN;
ATTACH ':memory:' AS a;
ATTACH ':memory:' AS b;
DETACH a;
CREATE TABLE b.t(x);
INSERT INTO b.t VALUES(1);
COMMIT;
SELECT * FROM b.t;
SELECT name FROM pragma_database_list ORDER BY seq;
-- case: bug-hunt-8/sql/detach-written-database-inside-transaction
BEGIN;
ATTACH ':memory:' AS m2;
CREATE TABLE m2.t(x);
DETACH m2;
-- case: bug-hunt-8/sql/check-message-quoted-column
CREATE TABLE t (a varchar CHECK( "a" IN ('x','y') ));
INSERT INTO t VALUES ('z');
-- case: bug-hunt-8/sql/check-message-plain-column
CREATE TABLE t (a varchar CHECK( a IN ('x','y') ));
INSERT INTO t VALUES ('z');
-- case: bug-hunt-8/sql/check-message-expression
CREATE TABLE t (a, b, CHECK( a + b > 5 ));
INSERT INTO t VALUES (1, 2);
-- case: bug-hunt-8/sql/is-null-binds-tighter-than-comparison
SELECT 1 IS NULL + 2, 1 IS NOT NULL || 2, 1 IS NULL < 2, 1 IS NOT NULL > 2, NULL IS NULL * 0, 1 IS NOT NULL << 2;
-- case: bug-hunt-8/sql/upsert-two-clauses-order
CREATE TABLE t(a, b, c, UNIQUE(a), UNIQUE(b));
INSERT INTO t VALUES(1, 1, 'first');
INSERT INTO t VALUES(2, 2, 'second');
INSERT INTO t VALUES(1, 2, 'both') ON CONFLICT(a) DO UPDATE SET c = 'a-hit' ON CONFLICT(b) DO UPDATE SET c = 'b-hit';
SELECT * FROM t ORDER BY a;
-- case: bug-hunt-8/sql/having-on-group-key-skips-overflow
CREATE TABLE t(g, v);
INSERT INTO t VALUES('a', 1), ('d', 9223372036854775807), ('d', 1);
SELECT sum(v) FROM t GROUP BY g HAVING g = 'a';
-- case: bug-hunt-8/sql/update-or-ignore-row-order
CREATE TABLE t(id INTEGER PRIMARY KEY, c TEXT UNIQUE);
INSERT INTO t VALUES(1,'a'),(2,'ABC');
UPDATE OR IGNORE t SET c = '01' WHERE c > 'A';
SELECT id, c FROM t ORDER BY id;
-- case: bug-hunt-8/sql/update-or-replace-row-order
CREATE TABLE t(id INTEGER PRIMARY KEY, a, c2 UNIQUE);
INSERT INTO t VALUES(1,'x','日本'),(2,'y','é');
UPDATE OR REPLACE t SET c2 = 3;
SELECT * FROM t;
-- case: bug-hunt-8/sql/log-null-and-text-arguments
CREATE TABLE t(b, x);
INSERT INTO t VALUES(10, NULL), (10, 100);
SELECT log(b, x) FROM t;
SELECT log(10, NULL), log(2, NULL), log(2, 'a'), log(2, x'00');
-- case: bug-hunt-8/sql/log-base-below-one
SELECT log(0.5, 2), log(0.25, 2), log(1, 2), log(0, 2), log(-2, 4), log(2, 0), log(2, -1);
-- case: bug-hunt-8/sql/strftime-trailing-percent
SELECT strftime('%', '2024-01-01'), strftime('%Y%', '2024-05-06'), strftime('%Y-%', '2024-05-06');
-- case: bug-hunt-8/sql/strftime-numeric-format
SELECT strftime(5, '2024-01-01'), strftime(0, 0), strftime(1, 1), strftime(1.5, 1);
-- case: bug-hunt-8/sql/strftime-g-negative-year
SELECT strftime('%g', '1970-01-01', '-2000 years'), strftime('%G', '1970-01-01', '-2000 years'), strftime('%g %G', '0001-01-01');
-- case: bug-hunt-8/sql/unistr-escapes
SELECT unistr('a\+000041') AS a, unistr('\0041') AS b, unistr('\+00004a') AS c, unistr('A') AS d, unistr('\U00000041') AS e, unistr('\\') AS f;
-- case: bug-hunt-8/sql/unistr-surrogates
SELECT hex(unistr('\d83d\de00')) AS a, hex(unistr('\D83D\DE00')) AS b, hex(unistr('\+01f600')) AS c;
-- case: bug-hunt-8/sql/unistr-lone-surrogate
SELECT hex(unistr('\d83d')) AS a;
-- case: bug-hunt-8/sql/unistr-past-the-last-code-point
SELECT hex(unistr('\U00110000')) AS a, hex(unistr('\+1fffff')) AS b, hex(unistr('\0000x')) AS c;
-- case: bug-hunt-8/sql/unistr-quote-escape-is-refused
SELECT unistr('\''');
-- case: bug-hunt-8/sql/unistr-short-escape-is-refused
SELECT unistr('\u00');
-- case: bug-hunt-8/sql/unistr-trailing-backslash-is-refused
SELECT unistr('ab\');
-- case: bug-hunt-8/sql/window-range-large-offset
CREATE TABLE rw(v);
INSERT INTO rw VALUES(1),(2),(-9223372036854775808);
SELECT v, sum(v) OVER (ORDER BY v RANGE BETWEEN 9223372036854775807 PRECEDING AND CURRENT ROW) AS s FROM rw ORDER BY v;
SELECT v, sum(v) OVER (ORDER BY v RANGE BETWEEN CURRENT ROW AND 9223372036854775807 FOLLOWING) AS s FROM rw ORDER BY v;
SELECT v, count(*) OVER (ORDER BY v RANGE BETWEEN 9223372036854775807 PRECEDING AND 9223372036854775807 FOLLOWING) AS s FROM rw ORDER BY v;
SELECT v, total(v) OVER (ORDER BY v RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS s FROM rw ORDER BY v;
SELECT v, count(*) OVER (ORDER BY v DESC RANGE BETWEEN 9223372036854775807 PRECEDING AND CURRENT ROW) AS s FROM rw ORDER BY v;
-- case: bug-hunt-8/sql/window-minmax-equal-values-type
CREATE TABLE t(id INTEGER PRIMARY KEY, w);
INSERT INTO t VALUES(1,0.0),(2,0),(3,5);
SELECT id, quote(min(w) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW)), quote(max(w) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW)) FROM t;
-- case: bug-hunt-8/sql/window-frame-start-after-end
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 10), (2, 20);
SELECT a, sum(b) OVER (ORDER BY a ROWS BETWEEN 1 FOLLOWING AND 1 PRECEDING) FROM t;
-- case: bug-hunt-8/sql/window-frame-current-row-after-preceding
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 10), (2, 20);
SELECT a, sum(b) OVER (ORDER BY a ROWS BETWEEN CURRENT ROW AND 1 PRECEDING) FROM t;
-- case: bug-hunt-8/sql/window-frame-following-before-current
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1, 10), (2, 20);
SELECT a, sum(b) OVER (ORDER BY a ROWS BETWEEN 1 FOLLOWING AND CURRENT ROW) FROM t;
-- case: bug-hunt-8/sql/json5-hex-above-int64
SELECT json('[0x8000000000000000]'), json('[0xFFFFFFFFFFFFFFFF]'), json_extract('[0x10000000000000000]', '$[0]');
-- case: bug-hunt-8/sql/json-arrow-real-or-blob-right-side
SELECT 0 -> 1.5, '[1,2]' ->> 1e300, '[1,2]' ->> x'31';
-- case: bug-hunt-8/sql/json-unterminated-comment
SELECT json('[1,2]  /* unterminated');
-- case: bug-hunt-8/sql/json-unterminated-comment-valid
SELECT json_valid('[1,2] /* x', 6);
-- case: bug-hunt-8/sql/json5-nul-escape-followed-by-digit
SELECT json_valid('"\01"', 6);
-- case: bug-hunt-8/sql/printf-hash-q
SELECT hex(printf('%#q', 'a' || char(10) || 'b')), hex(printf('%#Q', 'a' || char(10) || 'b')), printf('%-0*d|', 6, 42);
-- case: bug-hunt-8/sql/ceil-floor-trunc-of-text
SELECT ceil('1e2'), floor('1e2'), trunc('1e2'), ceil('5.5'), typeof(ceil('1e2')), typeof(floor('12')), typeof(trunc('12'));
-- case: bug-hunt-8/sql/unhex-separator-inside-byte
SELECT hex(unhex('4 142', ' ')), hex(unhex('41 42', ' '));
-- case: bug-hunt-8/sql/create-table-column-after-constraint
CREATE TABLE t(a, UNIQUE(a), b);
-- case: bug-hunt-8/sql/keywords-as-column-names
CREATE TABLE t(like INTEGER, glob INTEGER, regexp INTEGER, current_date INTEGER, true INTEGER);
INSERT INTO t VALUES(1, 2, 3, 4, 5);
SELECT like, glob, regexp, true FROM t;
-- case: bug-hunt-8/sql/keywords-as-column-names-more
CREATE TABLE t(current_time INTEGER, current_timestamp INTEGER, false INTEGER, k);
INSERT INTO t VALUES(1, 2, 3, 4);
SELECT false, k, "false", [true] FROM t;
SELECT t.false, t.current_time FROM t;
SELECT true, false FROM (SELECT 1 AS x);
SELECT count(*) FROM t WHERE k LIKE 4 AND k GLOB '4' AND k NOT LIKE 5;
-- case: bug-hunt-8/sql/like-glob-regexp-still-work-as-functions
SELECT like('a%', 'abc'), glob('a*', 'abc'), 'abc' LIKE 'A%', 'abc' GLOB 'A*', 'x' NOT LIKE 'y';
-- case: bug-hunt-8/sql/true-false-literals
SELECT true, false, true + 1, NOT false, 5 IS TRUE, 0 IS NOT TRUE;
-- case: bug-hunt-8/sql/having-moves-to-where-keeps-aggregate-terms
CREATE TABLE t(g, v);
INSERT INTO t VALUES('a', 1), ('a', 2), ('b', 10), ('c', 20);
SELECT g, sum(v) FROM t GROUP BY g HAVING g <> 'b' AND sum(v) > 2 ORDER BY g;
SELECT g, sum(v) FROM t GROUP BY g HAVING g = 'a' OR g = 'c' ORDER BY g;
SELECT g, count(*) FROM t GROUP BY g HAVING g COLLATE NOCASE = 'A';
-- case: bug-hunt-8/sql/is-null-forms
SELECT 1 IS NULL, NULL IS NULL, 1 IS NOT NULL, NULL IS NOT NULL, (1) IS NULL, 1 ISNULL, 1 NOTNULL, 1 NOT NULL;
-- case: bug-hunt-8/sql/is-null-binds-to-the-right-operand-arithmetic
SELECT 1 IS NULL + 2 AS a, NULL IS NULL + 2 AS b, 1 IS NOT NULL + 2 AS c, 1 IS NULL IS NULL AS d;
-- case: bug-hunt-8/sql/window-sliding-min-max-ties
CREATE TABLE t(id INTEGER PRIMARY KEY, w);
INSERT INTO t VALUES(1,0.0),(2,0),(3,5),(4,0.0),(5,0);
SELECT id, quote(min(w) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING)) AS a, quote(max(w) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 2 FOLLOWING)) AS b FROM t;
SELECT id, quote(min(w) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)) AS a, quote(max(w) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)) AS b FROM t;
SELECT id, quote(min(w) OVER (ORDER BY id)) AS a, quote(max(w) OVER ()) AS b FROM t;
-- case: bug-hunt-8/sql/window-rows-following-alone-is-refused
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
SELECT a, sum(b) OVER (ORDER BY a ROWS 1 FOLLOWING) FROM t;
-- case: bug-hunt-8/sql/printf-zero-flag-with-left-justify
SELECT printf('%-05d|', 42) AS a, printf('%-06d|', -42) AS b, printf('%-06x|', 255) AS c, printf('%-06.2f|', 3.14159) AS d, printf('%-06s|', 'ab') AS e, printf('%+-06d|', 42) AS f;
-- case: bug-hunt-8/sql/printf-hash-q-backslash-and-controls
SELECT printf('%#q', 'a\b') AS a, printf('%#Q', 'a\b') AS b, printf('%#Q', 'a' || char(9) || 'b\c') AS c, printf('%#q', 'a' || char(9) || 'b\c') AS d, printf('%#Q', 'it''s' || char(1)) AS e, printf('%#Q', NULL) AS f, printf('%#.3Q', 'abcd' || char(1)) AS g, printf('%#w', 'a"b') AS h;
-- case: bug-hunt-8/sql/json5-hex-integers-past-int64
SELECT json('[0x8000000000000000]') AS a, json('[0xFFFFFFFFFFFFFFFF]') AS b, json('[0x10000000000000000]') AS c, json('[-0x8000000000000000]') AS d, json('[-0xFFFFFFFFFFFFFFFF]') AS e;
-- case: bug-hunt-8/sql/json5-hex-extract-past-int64
SELECT json_extract('[0x8000000000000000]', '$[0]') AS a, json_extract('[-0x8000000000000001]', '$[0]') AS b, json_extract('[-0x8000000000000000]', '$[0]') AS c;
-- case: bug-hunt-8/sql/json5-hex-extract-past-sixteen-digits
SELECT json_extract('[0x10000000000000000]', '$[0]') AS a;
-- case: bug-hunt-8/sql/json-arrow-real-and-blob-labels
SELECT '[1,2]' -> 1.0 AS a, '{"a":1}' ->> x'61' AS b, '{"1.5":7}' ->> 1.5 AS c, '[1,2]' -> NULL AS d, '[1,2]' -> 1 AS e;
-- case: bug-hunt-8/sql/json-comment-closed-and-open
SELECT json_valid('[1,2] /* x */', 6) AS a, json_valid('[1,/* x', 6) AS b, json_valid('[1,2] /* x *', 6) AS c;
-- case: bug-hunt-8/sql/json5-nul-escape-forms
SELECT json_valid('"\0"', 6) AS a, json_valid('"\00"', 6) AS b, json_valid('"\08"', 6) AS c, json_valid('"\0a"', 6) AS d, json_valid('"\x41"', 6) AS e;
-- case: bug-hunt-8/sql/unhex-ignored-characters-between-bytes
SELECT hex(unhex('41 42', ' ')) AS a, hex(unhex(' 41  42 ', ' ')) AS b, hex(unhex('4 1', ' ')) AS c, hex(unhex('414', ' ')) AS d, hex(unhex('', ' ')) AS e, hex(unhex('41-42', '-')) AS f;
-- case: bug-hunt-8/sql/ceil-floor-trunc-types
SELECT quote(ceil('12')) AS a, quote(ceil('12.0')) AS b, quote(floor(' 5 ')) AS c, quote(trunc('1e2')) AS d, quote(ceil(5)) AS e, quote(floor(5.5)) AS f, quote(ceil('x')) AS g;
-- case: bug-hunt-8/sql/strftime-numeric-format-real
SELECT strftime(1.5, 1) AS a, strftime(NULL, 1) AS b;
-- case: bug-hunt-8/sql/update-order-by-rowid-when-index-assigned
CREATE TABLE t(id INTEGER PRIMARY KEY, c TEXT UNIQUE, n);
INSERT INTO t VALUES(1,'z',0),(2,'a',0),(3,'m',0);
UPDATE t SET n = n + 1, c = c || 'x' WHERE c > 'A' RETURNING id;
-- case: bug-hunt-8/sql/upsert-first-clause-wins-over-rowid-order
CREATE TABLE t(a, b, c, UNIQUE(a), UNIQUE(b));
INSERT INTO t VALUES(1, 1, 'first');
INSERT INTO t VALUES(2, 2, 'second');
INSERT INTO t VALUES(1, 2, 'both') ON CONFLICT(b) DO UPDATE SET c = 'b-hit' ON CONFLICT(a) DO UPDATE SET c = 'a-hit';
SELECT * FROM t ORDER BY a;
-- case: bug-hunt-8/sql/glob-class-forms
SELECT 'a' GLOB '[a-c]', 'd' GLOB '[a-c]', ']' GLOB '[]]', 'a' GLOB '[^]]', '-' GLOB '[-a]', 'b' GLOB '[a-]', '-' GLOB '[a-]', 'é' GLOB '[^é]', 'e' GLOB '[^é]', 'a' GLOB '[a', '日' GLOB '[^日本]', '本' GLOB '[日-本]', 'x' GLOB '[a-c-x]';
-- case: bug-hunt-8/sql/chained-ctes-70
WITH c0(x) AS (SELECT 1), c1(x) AS (SELECT x + 1 FROM c0), c2(x) AS (SELECT x + 1 FROM c1), c3(x) AS (SELECT x + 1 FROM c2), c4(x) AS (SELECT x + 1 FROM c3), c5(x) AS (SELECT x + 1 FROM c4), c6(x) AS (SELECT x + 1 FROM c5), c7(x) AS (SELECT x + 1 FROM c6), c8(x) AS (SELECT x + 1 FROM c7), c9(x) AS (SELECT x + 1 FROM c8), c10(x) AS (SELECT x + 1 FROM c9), c11(x) AS (SELECT x + 1 FROM c10), c12(x) AS (SELECT x + 1 FROM c11), c13(x) AS (SELECT x + 1 FROM c12), c14(x) AS (SELECT x + 1 FROM c13), c15(x) AS (SELECT x + 1 FROM c14), c16(x) AS (SELECT x + 1 FROM c15), c17(x) AS (SELECT x + 1 FROM c16), c18(x) AS (SELECT x + 1 FROM c17), c19(x) AS (SELECT x + 1 FROM c18), c20(x) AS (SELECT x + 1 FROM c19), c21(x) AS (SELECT x + 1 FROM c20), c22(x) AS (SELECT x + 1 FROM c21), c23(x) AS (SELECT x + 1 FROM c22), c24(x) AS (SELECT x + 1 FROM c23), c25(x) AS (SELECT x + 1 FROM c24), c26(x) AS (SELECT x + 1 FROM c25), c27(x) AS (SELECT x + 1 FROM c26), c28(x) AS (SELECT x + 1 FROM c27), c29(x) AS (SELECT x + 1 FROM c28), c30(x) AS (SELECT x + 1 FROM c29), c31(x) AS (SELECT x + 1 FROM c30), c32(x) AS (SELECT x + 1 FROM c31), c33(x) AS (SELECT x + 1 FROM c32), c34(x) AS (SELECT x + 1 FROM c33), c35(x) AS (SELECT x + 1 FROM c34), c36(x) AS (SELECT x + 1 FROM c35), c37(x) AS (SELECT x + 1 FROM c36), c38(x) AS (SELECT x + 1 FROM c37), c39(x) AS (SELECT x + 1 FROM c38), c40(x) AS (SELECT x + 1 FROM c39), c41(x) AS (SELECT x + 1 FROM c40), c42(x) AS (SELECT x + 1 FROM c41), c43(x) AS (SELECT x + 1 FROM c42), c44(x) AS (SELECT x + 1 FROM c43), c45(x) AS (SELECT x + 1 FROM c44), c46(x) AS (SELECT x + 1 FROM c45), c47(x) AS (SELECT x + 1 FROM c46), c48(x) AS (SELECT x + 1 FROM c47), c49(x) AS (SELECT x + 1 FROM c48), c50(x) AS (SELECT x + 1 FROM c49), c51(x) AS (SELECT x + 1 FROM c50), c52(x) AS (SELECT x + 1 FROM c51), c53(x) AS (SELECT x + 1 FROM c52), c54(x) AS (SELECT x + 1 FROM c53), c55(x) AS (SELECT x + 1 FROM c54), c56(x) AS (SELECT x + 1 FROM c55), c57(x) AS (SELECT x + 1 FROM c56), c58(x) AS (SELECT x + 1 FROM c57), c59(x) AS (SELECT x + 1 FROM c58), c60(x) AS (SELECT x + 1 FROM c59), c61(x) AS (SELECT x + 1 FROM c60), c62(x) AS (SELECT x + 1 FROM c61), c63(x) AS (SELECT x + 1 FROM c62), c64(x) AS (SELECT x + 1 FROM c63), c65(x) AS (SELECT x + 1 FROM c64), c66(x) AS (SELECT x + 1 FROM c65), c67(x) AS (SELECT x + 1 FROM c66), c68(x) AS (SELECT x + 1 FROM c67), c69(x) AS (SELECT x + 1 FROM c68) SELECT x FROM c69;
-- case: bug-hunt-8/sql/nested-select-60
SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT 1 AS x))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
-- case: bug-hunt-8/sql/chained-ctes-300
WITH c0(x) AS (SELECT 1), c1(x) AS (SELECT x + 1 FROM c0), c2(x) AS (SELECT x + 1 FROM c1), c3(x) AS (SELECT x + 1 FROM c2), c4(x) AS (SELECT x + 1 FROM c3), c5(x) AS (SELECT x + 1 FROM c4), c6(x) AS (SELECT x + 1 FROM c5), c7(x) AS (SELECT x + 1 FROM c6), c8(x) AS (SELECT x + 1 FROM c7), c9(x) AS (SELECT x + 1 FROM c8), c10(x) AS (SELECT x + 1 FROM c9), c11(x) AS (SELECT x + 1 FROM c10), c12(x) AS (SELECT x + 1 FROM c11), c13(x) AS (SELECT x + 1 FROM c12), c14(x) AS (SELECT x + 1 FROM c13), c15(x) AS (SELECT x + 1 FROM c14), c16(x) AS (SELECT x + 1 FROM c15), c17(x) AS (SELECT x + 1 FROM c16), c18(x) AS (SELECT x + 1 FROM c17), c19(x) AS (SELECT x + 1 FROM c18), c20(x) AS (SELECT x + 1 FROM c19), c21(x) AS (SELECT x + 1 FROM c20), c22(x) AS (SELECT x + 1 FROM c21), c23(x) AS (SELECT x + 1 FROM c22), c24(x) AS (SELECT x + 1 FROM c23), c25(x) AS (SELECT x + 1 FROM c24), c26(x) AS (SELECT x + 1 FROM c25), c27(x) AS (SELECT x + 1 FROM c26), c28(x) AS (SELECT x + 1 FROM c27), c29(x) AS (SELECT x + 1 FROM c28), c30(x) AS (SELECT x + 1 FROM c29), c31(x) AS (SELECT x + 1 FROM c30), c32(x) AS (SELECT x + 1 FROM c31), c33(x) AS (SELECT x + 1 FROM c32), c34(x) AS (SELECT x + 1 FROM c33), c35(x) AS (SELECT x + 1 FROM c34), c36(x) AS (SELECT x + 1 FROM c35), c37(x) AS (SELECT x + 1 FROM c36), c38(x) AS (SELECT x + 1 FROM c37), c39(x) AS (SELECT x + 1 FROM c38), c40(x) AS (SELECT x + 1 FROM c39), c41(x) AS (SELECT x + 1 FROM c40), c42(x) AS (SELECT x + 1 FROM c41), c43(x) AS (SELECT x + 1 FROM c42), c44(x) AS (SELECT x + 1 FROM c43), c45(x) AS (SELECT x + 1 FROM c44), c46(x) AS (SELECT x + 1 FROM c45), c47(x) AS (SELECT x + 1 FROM c46), c48(x) AS (SELECT x + 1 FROM c47), c49(x) AS (SELECT x + 1 FROM c48), c50(x) AS (SELECT x + 1 FROM c49), c51(x) AS (SELECT x + 1 FROM c50), c52(x) AS (SELECT x + 1 FROM c51), c53(x) AS (SELECT x + 1 FROM c52), c54(x) AS (SELECT x + 1 FROM c53), c55(x) AS (SELECT x + 1 FROM c54), c56(x) AS (SELECT x + 1 FROM c55), c57(x) AS (SELECT x + 1 FROM c56), c58(x) AS (SELECT x + 1 FROM c57), c59(x) AS (SELECT x + 1 FROM c58), c60(x) AS (SELECT x + 1 FROM c59), c61(x) AS (SELECT x + 1 FROM c60), c62(x) AS (SELECT x + 1 FROM c61), c63(x) AS (SELECT x + 1 FROM c62), c64(x) AS (SELECT x + 1 FROM c63), c65(x) AS (SELECT x + 1 FROM c64), c66(x) AS (SELECT x + 1 FROM c65), c67(x) AS (SELECT x + 1 FROM c66), c68(x) AS (SELECT x + 1 FROM c67), c69(x) AS (SELECT x + 1 FROM c68), c70(x) AS (SELECT x + 1 FROM c69), c71(x) AS (SELECT x + 1 FROM c70), c72(x) AS (SELECT x + 1 FROM c71), c73(x) AS (SELECT x + 1 FROM c72), c74(x) AS (SELECT x + 1 FROM c73), c75(x) AS (SELECT x + 1 FROM c74), c76(x) AS (SELECT x + 1 FROM c75), c77(x) AS (SELECT x + 1 FROM c76), c78(x) AS (SELECT x + 1 FROM c77), c79(x) AS (SELECT x + 1 FROM c78), c80(x) AS (SELECT x + 1 FROM c79), c81(x) AS (SELECT x + 1 FROM c80), c82(x) AS (SELECT x + 1 FROM c81), c83(x) AS (SELECT x + 1 FROM c82), c84(x) AS (SELECT x + 1 FROM c83), c85(x) AS (SELECT x + 1 FROM c84), c86(x) AS (SELECT x + 1 FROM c85), c87(x) AS (SELECT x + 1 FROM c86), c88(x) AS (SELECT x + 1 FROM c87), c89(x) AS (SELECT x + 1 FROM c88), c90(x) AS (SELECT x + 1 FROM c89), c91(x) AS (SELECT x + 1 FROM c90), c92(x) AS (SELECT x + 1 FROM c91), c93(x) AS (SELECT x + 1 FROM c92), c94(x) AS (SELECT x + 1 FROM c93), c95(x) AS (SELECT x + 1 FROM c94), c96(x) AS (SELECT x + 1 FROM c95), c97(x) AS (SELECT x + 1 FROM c96), c98(x) AS (SELECT x + 1 FROM c97), c99(x) AS (SELECT x + 1 FROM c98), c100(x) AS (SELECT x + 1 FROM c99), c101(x) AS (SELECT x + 1 FROM c100), c102(x) AS (SELECT x + 1 FROM c101), c103(x) AS (SELECT x + 1 FROM c102), c104(x) AS (SELECT x + 1 FROM c103), c105(x) AS (SELECT x + 1 FROM c104), c106(x) AS (SELECT x + 1 FROM c105), c107(x) AS (SELECT x + 1 FROM c106), c108(x) AS (SELECT x + 1 FROM c107), c109(x) AS (SELECT x + 1 FROM c108), c110(x) AS (SELECT x + 1 FROM c109), c111(x) AS (SELECT x + 1 FROM c110), c112(x) AS (SELECT x + 1 FROM c111), c113(x) AS (SELECT x + 1 FROM c112), c114(x) AS (SELECT x + 1 FROM c113), c115(x) AS (SELECT x + 1 FROM c114), c116(x) AS (SELECT x + 1 FROM c115), c117(x) AS (SELECT x + 1 FROM c116), c118(x) AS (SELECT x + 1 FROM c117), c119(x) AS (SELECT x + 1 FROM c118), c120(x) AS (SELECT x + 1 FROM c119), c121(x) AS (SELECT x + 1 FROM c120), c122(x) AS (SELECT x + 1 FROM c121), c123(x) AS (SELECT x + 1 FROM c122), c124(x) AS (SELECT x + 1 FROM c123), c125(x) AS (SELECT x + 1 FROM c124), c126(x) AS (SELECT x + 1 FROM c125), c127(x) AS (SELECT x + 1 FROM c126), c128(x) AS (SELECT x + 1 FROM c127), c129(x) AS (SELECT x + 1 FROM c128), c130(x) AS (SELECT x + 1 FROM c129), c131(x) AS (SELECT x + 1 FROM c130), c132(x) AS (SELECT x + 1 FROM c131), c133(x) AS (SELECT x + 1 FROM c132), c134(x) AS (SELECT x + 1 FROM c133), c135(x) AS (SELECT x + 1 FROM c134), c136(x) AS (SELECT x + 1 FROM c135), c137(x) AS (SELECT x + 1 FROM c136), c138(x) AS (SELECT x + 1 FROM c137), c139(x) AS (SELECT x + 1 FROM c138), c140(x) AS (SELECT x + 1 FROM c139), c141(x) AS (SELECT x + 1 FROM c140), c142(x) AS (SELECT x + 1 FROM c141), c143(x) AS (SELECT x + 1 FROM c142), c144(x) AS (SELECT x + 1 FROM c143), c145(x) AS (SELECT x + 1 FROM c144), c146(x) AS (SELECT x + 1 FROM c145), c147(x) AS (SELECT x + 1 FROM c146), c148(x) AS (SELECT x + 1 FROM c147), c149(x) AS (SELECT x + 1 FROM c148), c150(x) AS (SELECT x + 1 FROM c149), c151(x) AS (SELECT x + 1 FROM c150), c152(x) AS (SELECT x + 1 FROM c151), c153(x) AS (SELECT x + 1 FROM c152), c154(x) AS (SELECT x + 1 FROM c153), c155(x) AS (SELECT x + 1 FROM c154), c156(x) AS (SELECT x + 1 FROM c155), c157(x) AS (SELECT x + 1 FROM c156), c158(x) AS (SELECT x + 1 FROM c157), c159(x) AS (SELECT x + 1 FROM c158), c160(x) AS (SELECT x + 1 FROM c159), c161(x) AS (SELECT x + 1 FROM c160), c162(x) AS (SELECT x + 1 FROM c161), c163(x) AS (SELECT x + 1 FROM c162), c164(x) AS (SELECT x + 1 FROM c163), c165(x) AS (SELECT x + 1 FROM c164), c166(x) AS (SELECT x + 1 FROM c165), c167(x) AS (SELECT x + 1 FROM c166), c168(x) AS (SELECT x + 1 FROM c167), c169(x) AS (SELECT x + 1 FROM c168), c170(x) AS (SELECT x + 1 FROM c169), c171(x) AS (SELECT x + 1 FROM c170), c172(x) AS (SELECT x + 1 FROM c171), c173(x) AS (SELECT x + 1 FROM c172), c174(x) AS (SELECT x + 1 FROM c173), c175(x) AS (SELECT x + 1 FROM c174), c176(x) AS (SELECT x + 1 FROM c175), c177(x) AS (SELECT x + 1 FROM c176), c178(x) AS (SELECT x + 1 FROM c177), c179(x) AS (SELECT x + 1 FROM c178), c180(x) AS (SELECT x + 1 FROM c179), c181(x) AS (SELECT x + 1 FROM c180), c182(x) AS (SELECT x + 1 FROM c181), c183(x) AS (SELECT x + 1 FROM c182), c184(x) AS (SELECT x + 1 FROM c183), c185(x) AS (SELECT x + 1 FROM c184), c186(x) AS (SELECT x + 1 FROM c185), c187(x) AS (SELECT x + 1 FROM c186), c188(x) AS (SELECT x + 1 FROM c187), c189(x) AS (SELECT x + 1 FROM c188), c190(x) AS (SELECT x + 1 FROM c189), c191(x) AS (SELECT x + 1 FROM c190), c192(x) AS (SELECT x + 1 FROM c191), c193(x) AS (SELECT x + 1 FROM c192), c194(x) AS (SELECT x + 1 FROM c193), c195(x) AS (SELECT x + 1 FROM c194), c196(x) AS (SELECT x + 1 FROM c195), c197(x) AS (SELECT x + 1 FROM c196), c198(x) AS (SELECT x + 1 FROM c197), c199(x) AS (SELECT x + 1 FROM c198), c200(x) AS (SELECT x + 1 FROM c199), c201(x) AS (SELECT x + 1 FROM c200), c202(x) AS (SELECT x + 1 FROM c201), c203(x) AS (SELECT x + 1 FROM c202), c204(x) AS (SELECT x + 1 FROM c203), c205(x) AS (SELECT x + 1 FROM c204), c206(x) AS (SELECT x + 1 FROM c205), c207(x) AS (SELECT x + 1 FROM c206), c208(x) AS (SELECT x + 1 FROM c207), c209(x) AS (SELECT x + 1 FROM c208), c210(x) AS (SELECT x + 1 FROM c209), c211(x) AS (SELECT x + 1 FROM c210), c212(x) AS (SELECT x + 1 FROM c211), c213(x) AS (SELECT x + 1 FROM c212), c214(x) AS (SELECT x + 1 FROM c213), c215(x) AS (SELECT x + 1 FROM c214), c216(x) AS (SELECT x + 1 FROM c215), c217(x) AS (SELECT x + 1 FROM c216), c218(x) AS (SELECT x + 1 FROM c217), c219(x) AS (SELECT x + 1 FROM c218), c220(x) AS (SELECT x + 1 FROM c219), c221(x) AS (SELECT x + 1 FROM c220), c222(x) AS (SELECT x + 1 FROM c221), c223(x) AS (SELECT x + 1 FROM c222), c224(x) AS (SELECT x + 1 FROM c223), c225(x) AS (SELECT x + 1 FROM c224), c226(x) AS (SELECT x + 1 FROM c225), c227(x) AS (SELECT x + 1 FROM c226), c228(x) AS (SELECT x + 1 FROM c227), c229(x) AS (SELECT x + 1 FROM c228), c230(x) AS (SELECT x + 1 FROM c229), c231(x) AS (SELECT x + 1 FROM c230), c232(x) AS (SELECT x + 1 FROM c231), c233(x) AS (SELECT x + 1 FROM c232), c234(x) AS (SELECT x + 1 FROM c233), c235(x) AS (SELECT x + 1 FROM c234), c236(x) AS (SELECT x + 1 FROM c235), c237(x) AS (SELECT x + 1 FROM c236), c238(x) AS (SELECT x + 1 FROM c237), c239(x) AS (SELECT x + 1 FROM c238), c240(x) AS (SELECT x + 1 FROM c239), c241(x) AS (SELECT x + 1 FROM c240), c242(x) AS (SELECT x + 1 FROM c241), c243(x) AS (SELECT x + 1 FROM c242), c244(x) AS (SELECT x + 1 FROM c243), c245(x) AS (SELECT x + 1 FROM c244), c246(x) AS (SELECT x + 1 FROM c245), c247(x) AS (SELECT x + 1 FROM c246), c248(x) AS (SELECT x + 1 FROM c247), c249(x) AS (SELECT x + 1 FROM c248), c250(x) AS (SELECT x + 1 FROM c249), c251(x) AS (SELECT x + 1 FROM c250), c252(x) AS (SELECT x + 1 FROM c251), c253(x) AS (SELECT x + 1 FROM c252), c254(x) AS (SELECT x + 1 FROM c253), c255(x) AS (SELECT x + 1 FROM c254), c256(x) AS (SELECT x + 1 FROM c255), c257(x) AS (SELECT x + 1 FROM c256), c258(x) AS (SELECT x + 1 FROM c257), c259(x) AS (SELECT x + 1 FROM c258), c260(x) AS (SELECT x + 1 FROM c259), c261(x) AS (SELECT x + 1 FROM c260), c262(x) AS (SELECT x + 1 FROM c261), c263(x) AS (SELECT x + 1 FROM c262), c264(x) AS (SELECT x + 1 FROM c263), c265(x) AS (SELECT x + 1 FROM c264), c266(x) AS (SELECT x + 1 FROM c265), c267(x) AS (SELECT x + 1 FROM c266), c268(x) AS (SELECT x + 1 FROM c267), c269(x) AS (SELECT x + 1 FROM c268), c270(x) AS (SELECT x + 1 FROM c269), c271(x) AS (SELECT x + 1 FROM c270), c272(x) AS (SELECT x + 1 FROM c271), c273(x) AS (SELECT x + 1 FROM c272), c274(x) AS (SELECT x + 1 FROM c273), c275(x) AS (SELECT x + 1 FROM c274), c276(x) AS (SELECT x + 1 FROM c275), c277(x) AS (SELECT x + 1 FROM c276), c278(x) AS (SELECT x + 1 FROM c277), c279(x) AS (SELECT x + 1 FROM c278), c280(x) AS (SELECT x + 1 FROM c279), c281(x) AS (SELECT x + 1 FROM c280), c282(x) AS (SELECT x + 1 FROM c281), c283(x) AS (SELECT x + 1 FROM c282), c284(x) AS (SELECT x + 1 FROM c283), c285(x) AS (SELECT x + 1 FROM c284), c286(x) AS (SELECT x + 1 FROM c285), c287(x) AS (SELECT x + 1 FROM c286), c288(x) AS (SELECT x + 1 FROM c287), c289(x) AS (SELECT x + 1 FROM c288), c290(x) AS (SELECT x + 1 FROM c289), c291(x) AS (SELECT x + 1 FROM c290), c292(x) AS (SELECT x + 1 FROM c291), c293(x) AS (SELECT x + 1 FROM c292), c294(x) AS (SELECT x + 1 FROM c293), c295(x) AS (SELECT x + 1 FROM c294), c296(x) AS (SELECT x + 1 FROM c295), c297(x) AS (SELECT x + 1 FROM c296), c298(x) AS (SELECT x + 1 FROM c297), c299(x) AS (SELECT x + 1 FROM c298) SELECT x FROM c299;
-- case: bug-hunt-8/sql/nested-select-300
SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT x+1 AS x FROM (SELECT 1 AS x))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
