-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/numbers-and-numeric-form/8-hex-literal-with-16-digits-wraps-to-a-negative-64-
SELECT 0Xffffffffbfc4330f, typeof(0Xffffffffbfc4330f), 0x7fffffffffffffff, 0xffffffffffffffff;
-- case: holdout/numbers-and-numeric-form/16-mod-of-a-negative-real-by-a-negative-real-turso-re
SELECT (mod((-1.0) * 0.5 * degrees(acos((0.5))), 0.0 / -1.0 - 0.5));
SELECT mod(-30.000000000000004, -0.5), mod(5.5, 2), mod(-5, 3), mod(5, -3), mod(5, 0), mod(5.0, 0.0);
-- case: holdout/numbers-and-numeric-form/59-trunc-atanh-tanh-2-mod-2-0-2-0-2-2-0-turso-3-0
SELECT (trunc(atanh((tanh(0.0 - 2.0) + mod(2.0, 2.0) * (-2.0 - 0.0))))) + -2.0;
SELECT tanh(-2.0), atanh(-0.964027580075817), trunc(-2.2), trunc(2.9), mod(2.0, 2.0);
-- case: holdout/numbers-and-numeric-form/107-atanh-exp-degrees-sin-chain-last-digits-of-the-res
SELECT atanh((-1.0 + exp(degrees(sin(degrees(0.5)))) / 0.5));
SELECT sin(degrees(0.5)), degrees(sin(degrees(0.5))), exp(degrees(sin(degrees(0.5)))), exp(-0.5), exp(1), exp(10);
-- case: holdout/numbers-and-numeric-form/140-round-rounds-half-away-from-zero
SELECT ROUND(2.25, 1), ROUND(2.35, 1), ROUND(2.45, 1), ROUND(0.5), ROUND(1.5), ROUND(2.5), ROUND(-0.5), ROUND(-2.5);
SELECT ROUND(0.285, 2), ROUND(1.005, 2), ROUND(1234.5678, -2), ROUND(5e15+0.5), ROUND(NULL, 1), ROUND(2.5, NULL);
-- case: holdout/numbers-and-numeric-form/189-real-versus-integer-comparison-near-2-61-uses-exac
SELECT 3036093696168066048.0 = 3036093696168066000;
SELECT CAST(3036093696168066048.0 AS REAL) = 3036093696168066000;
CREATE TABLE t1 (val REAL);
INSERT INTO t1 VALUES (3036093696168066048.0);
SELECT val = 3036093696168066000, val != 3036093696168066000, val < 3036093696168066000, val > 3036093696168066000, val <= 3036093696168066000, val >= 3036093696168066000 FROM t1;
SELECT 9007199254740993 = 9007199254740992.0, 9007199254740993 > 9007199254740992.0, 9223372036854775807 = 9223372036854775808.0;
-- case: holdout/numbers-and-numeric-form/391-quote-of-an-infinite-real
CREATE TABLE t(x REAL);
INSERT INTO t VALUES(1e400);
SELECT quote(x) FROM t;
SELECT quote(-1e400), quote(1e200*1e200), quote(CAST('1e400' AS REAL)), typeof(9.0e+999), -9.0e+999;
SELECT json_quote(1e400), quote(X'89504E47'), x, x + 1, x - x, x * 0 FROM t;
SELECT printf('%f', 1e400), printf('%g', -1e400), printf('%.3e', 1e400), printf('%d', 1e400), 1e400 || '', CAST(1e400 AS TEXT);
SELECT sum(x), avg(x), total(x), max(x) FROM t;
-- case: holdout/numbers-and-numeric-form/447-scalar-min-and-max-return-the-last-of-two-tied-arg
CREATE TABLE t(id INTEGER PRIMARY KEY, q REAL, s TEXT);
INSERT INTO t(id,q) VALUES(1,5.0),(2,3.0),(3,9.0);
UPDATE t SET s = min(q,5);
SELECT group_concat(s) FROM (SELECT s FROM t ORDER BY id);
SELECT typeof(min(1.0,1)), typeof(min(1,1.0)), typeof(max(1.0,1)), typeof(max(1,1.0));
SELECT min(1.0,1), min(1,1.0), max(1.0,1), max(1,1.0), min('a','a'), max(2,2.0,2), min(2,2.0,2);
-- case: holdout/numbers-and-numeric-form/474-float-literals-with-large-negative-exponents
SELECT 1e-300, 1e-300 * 1e300, 1e-320, 4.9e-324, 2.2250738585072014e-308, 1e-400, 123456789e-310;
SELECT 1e300 * 1e10, 1.7976931348623157e308, 1.7976931348623159e308, 0.1e1, 1E5, 1.e2, .5e1;
-- case: holdout/numbers-and-numeric-form/492-numeric-literals-with-underscores
SELECT 1._5;
SELECT 1e_2;
SELECT 1_000;
SELECT 1__0;
SELECT 0x1_0;
SELECT 1.5_5;
SELECT 1_0.5;
-- case: holdout/numbers-and-numeric-form/544-mod-atanh-tanh-1-1-divided-by-an-expression-involv
SELECT mod(atanh(tanh(-1.0)), ((1.0))) / ((asinh(-1.0) / 2.0 * 1.0) + pow(0.0, 1.0) + 0.5);
SELECT atanh(tanh(-1.0)), asinh(-1.0), pow(0.0, 1.0), mod(-1.0000000000000002, 1.0);
-- case: holdout/numbers-and-numeric-form/547-mod-acos-0-2-atan2-0-1-expression-near-zero
SELECT mod(acos(0.0 * -2.0) - atan2(0.0, 1.0), (0.5) / (2.0) * asin(-1.0 * 0.5));
SELECT acos(0.0), acos(-0.0), atan2(0.0, 1.0), asin(-0.5), mod(1.5707963267948966, -0.13089969389957471);
-- case: holdout/numbers-and-numeric-form/549-ceil-pow-2-0-expression-using-log-0-5
SELECT ((ceil(pow((((2.0))), (-2.0 - -1.0) / log(0.5)))) - -2.0);
SELECT (-2.0 - -1.0) / log(0.5), pow(2.0, (-2.0 - -1.0) / log(0.5)), log(0.5), log(2, 8), log10(1000), log2(8), ln(1);
-- case: holdout/numbers-and-numeric-form/550-sin-power-with-nested-power-0-0-0-0
SELECT sin(power((((degrees(1.0) + 1.0 + 2.0))), power(2.0 + 1.0 - 0.5 - 2.0, power(0.0, 0.0) * (-2.0))));
SELECT power(0.0, 0.0), power(2.0 + 1.0 - 0.5 - 2.0, -2.0), power(60.29577951308232, 4.0), pow(2, 0.5), pow(-8, 1.0/3), pow(0, -1), pow(2, 1024);
-- case: holdout/numbers-and-numeric-form/559-real-literals-written-as-1-e5
CREATE TABLE floats(id INTEGER PRIMARY KEY, val REAL);
INSERT INTO floats(val) VALUES(1.e5);
INSERT INTO floats(val) VALUES(1.e2), (1.E2), (.5E1), (1.), (1.e+2), (1.e-2);
SELECT val FROM floats;
SELECT 1.e5, 1.e5 + 1, typeof(1.), typeof(1.e0), 1.5.5;
-- case: holdout/numbers-and-numeric-form/603-large-integers-compared-with-reals
select cast('8211127122155032455' as real) = cast('8211127122155032455' as integer);
select 8211127122155032455 = 8211127122155033000;
select 8211127122155032455 = 8211127122155032455.0, 8211127122155032455 < 8211127122155033000.0, 9007199254740993 = 9007199254740992.0, 9007199254740993 > 9007199254740992.0;
select cast('8211127122155032455' as real), cast(8211127122155032455 as real), 8211127122155032455 + 0.0;
-- case: holdout/numbers-and-numeric-form/622-pragma-query-only-given-non-integer-numeric-litera
PRAGMA query_only=1e10;
PRAGMA query_only=1.5;
PRAGMA query_only=0xFF;
PRAGMA query_only=9223372036854775808;
PRAGMA query_only;
PRAGMA query_only=0;
-- case: holdout/numbers-and-numeric-form/627-limit-given-a-hexadecimal-literal
SELECT 1 LIMIT 0x10;
SELECT 1 LIMIT 1 OFFSET 0x0;
SELECT 1 UNION ALL SELECT 2 LIMIT 0x1;
SELECT 1 LIMIT 1.5;
SELECT 1 LIMIT '1';
SELECT 1 LIMIT 'a';
SELECT 1 LIMIT 1e0;
SELECT 1 LIMIT -1;
SELECT 1 LIMIT NULL;
-- case: holdout/numbers-and-numeric-form/647-limit-with-the-minimum-64-bit-integer
SELECT 1 LIMIT -9223372036854775808;
SELECT 1 LIMIT 1 OFFSET -9223372036854775808;
SELECT 1 LIMIT 9223372036854775807 OFFSET 9223372036854775807;
SELECT 1 LIMIT 9223372036854775808;
SELECT 1 LIMIT -1 OFFSET 0;
-- case: holdout/numbers-and-numeric-form/666-round-x-n-rounds-half-away-from-zero
SELECT ROUND(0.25, 1), ROUND(85.25, 1), ROUND(-85.25, 1), ROUND(1.25, 1), ROUND(2.25, 1), ROUND(1.125, 2), ROUND(-2.25, 1);
SELECT ROUND(2.5), ROUND(3.5), ROUND(-2.5), ROUND(0.5), ROUND(1.5), ROUND(2.675, 2), ROUND(1.005, 2), ROUND(0.285, 2), ROUND(8.345, 2), ROUND(1e15+0.5), ROUND(4503599627370496.5), ROUND(-0.4), ROUND(0.0), ROUND(5, 2), ROUND('3.14159', 3), ROUND(NULL), ROUND(3.14159, 20), ROUND(3.14159, -1), ROUND(1234.5, -2);
