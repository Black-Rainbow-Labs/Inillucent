-- case: bug-hunt-11/datetime/d1-log-second-argument-text-or-blob
SELECT log(2, x'38'), log(1.5, '12abc'), log(2, 'abc'), log(255, '1_000');
-- case: bug-hunt-11/datetime/d2-modifier-double-sign-accepted
SELECT datetime('2023-01-31', '++1 day'), datetime('2023-01-31', '--3 months'), datetime('2024-02-29 12:34:56', '+-.5 hours'), date('2024-01-01', '+-1 day'), unixepoch('2024-02-29', '+-.5 hours');
-- case: bug-hunt-11/datetime/d3-subsec-rounding
SELECT datetime('2000-01-01 00:00:16.9885', 'subsec'), datetime('2000-01-01 00:00:36.7525', 'subsec'), datetime('2000-01-01 00:00:00.0005', 'subsec');
-- case: bug-hunt-11/datetime/d4-printf-plus-and-space-flags
SELECT printf('%+ d', 5), printf('%+ f', 5.5), printf('%+ .3e', 1.5), printf('%+ 5d|', 5), printf('% +d', 5);
-- case: bug-hunt-11/datetime/d5-printf-hash-with-zero-padding
SELECT printf('%#010x', 255), printf('%#010X', 255), printf('%#08x', 1), printf('%#020x', 12), printf('%#05o|', 1), printf('%#010o', 8);
-- case: bug-hunt-11/datetime/d6-printf-p-conversion
SELECT printf('%#p', 255), printf('%010p', 255), printf('%010p', 0), printf('%#10.1p', 1);
-- case: bug-hunt-11/datetime/d7-printf-precision-on-null-for-q-Q-w
SELECT printf('%.2q', NULL), printf('%.2Q', NULL), printf('%10.2q|', NULL), printf('%.2w', NULL), printf('%.0q', NULL);
-- case: bug-hunt-11/datetime/d8-strftime-J-tiny-values
SELECT strftime('%J', 1e-7), strftime('%J', 1e-5), strftime('%J', 0.00001);
-- case: bug-hunt-11/datetime/d9-like-null-pattern-with-empty-escape
SELECT 'a' LIKE NULL ESCAPE '';
-- case: bug-hunt-11/datetime/d10-utc-modifier-before-2007-rules
SELECT datetime('2000-03-15 12:00:00', 'utc'), datetime('2000-03-15 12:00:00', 'localtime'), datetime('1990-03-15 12:00:00', 'utc');
-- case: bug-hunt-11/datetime/d10b-utc-modifier-after-2037
SELECT datetime('2038-03-15 12:00:00', 'utc'), datetime('2038-04-05 12:00:00', 'utc'), datetime('2050-03-15 12:00:00', 'utc'), datetime('2038-10-30 12:00:00', 'utc');
-- case: bug-hunt-11/datetime/d11-unistr-quote-text-with-nul
SELECT unistr_quote('a' || char(0) || 'b');
-- case: bug-hunt-11/datetime/d12-shell-prints-U+10FFFF-as-byte-ff
SELECT char(1114111);
-- case: bug-hunt-11/datetime/d13-json-array-insert-path-not-an-array
SELECT json_array_insert('3', '$.a', 1), jsonb_array_insert(0, '$.a', 0);
-- case: bug-hunt-11/datetime/d13b-json-valid-null-flags
SELECT json_valid(100, NULL);
-- case: bug-hunt-11/datetime/d13c-json-extract-malformed-before-bad-path
SELECT json_extract('abc', 'x');
-- case: bug-hunt-11/datetime/d13d-json-path-blob-error-text
SELECT json_set('1', x'4142', 1);
-- case: bug-hunt-11/datetime/d13e-json-pretty-indent-argument
SELECT json_pretty('[1,2,3]', 2);
-- case: bug-hunt-11/datetime/d13-json-array-insert-key-steps-by-container
SELECT json_array_insert('[1]', '$.a', 1);
SELECT json_array_insert('[1]', '$[0].x', 1);
SELECT json_array_insert('{"a":1}', '$.a.b', 1);
SELECT json_array_insert('{"a":{"b":1}}', '$.a.c', 1);
SELECT json_array_insert('[[1]]', '$[0][0]', 9);
SELECT json_array_insert('{"a":1}', '$.a', 1);
SELECT json_array_insert('{"a":1}', '$.b', 1);
SELECT json_array_insert('{"a":{"b":1}}', '$.x.c', 1);
-- case: bug-hunt-11/datetime/d13b-json-valid-flags-edge
SELECT json_valid(100, 0);
SELECT json_valid(NULL, NULL);
SELECT json_valid('{}', 16);
-- case: bug-hunt-11/datetime/d13c-json-extract-malformed-before-bad-path-more
SELECT json_extract('abc', '$.x');
SELECT json_extract('[1]', 'x');
SELECT json_extract('abc', NULL);
SELECT json_extract('{"a":1}', 'a');
-- case: bug-hunt-11/datetime/d13d-json-path-blob-read-as-text
SELECT json_extract('{}', x'24');
SELECT json_extract('{"a":5}', x'242e61');
SELECT json_extract('{}', x'4142');
-- case: bug-hunt-11/datetime/d13e-json-pretty-indent-any-type
SELECT json_pretty('[1,2,3]', 'ab');
SELECT json_pretty('[1,2,3]', NULL);
SELECT json_pretty('[1,2,3]', x'2d2d');
SELECT json_pretty('[1,2,3]', 1.5);
SELECT json_pretty('[1,2,3]', '');
-- case: bug-hunt-11/datetime/d1b-log-two-arguments-more
SELECT log(10, NULL), log(10, 100), log(10, '100'), log(10, '1e2x'), log(2, 0), log(2, -1), log(1, 5), log(0.5, 2), log(NULL, 5);
-- case: bug-hunt-11/datetime/d2b-modifier-single-sign-still-accepted
SELECT datetime('2023-01-31', '+1 day'), datetime('2023-01-31', '-3 months'), datetime('2024-02-29 12:34:56', '-.5 hours'), date('2024-01-01', '1 day'), datetime('2023-01-31', '+ 1 day'), datetime('2023-01-31', '+-1 day');
-- case: bug-hunt-11/datetime/d3b-time-subsec-rounding
SELECT time('00:00:16.9885', 'subsec'), time('00:00:59.9996', 'subsec'), strftime('%f', '2000-01-01 00:00:16.9885'), strftime('%f', '2000-01-01 00:00:59.9996'), strftime('%f', '2000-01-01 00:00:16.9884');
-- case: bug-hunt-11/datetime/d5b-printf-hash-octal-and-left-justify
SELECT printf('%#o', 8), printf('%#o', 0), printf('%#x', 0), printf('%-#10x|', 255), printf('%#10x|', 255), printf('%#010o', 8), printf('%0*x', 6, 255);
-- case: bug-hunt-11/datetime/d6b-printf-p-widths
SELECT printf('%10p|', 255), printf('%-10p|', 255), printf('%p', 255), printf('%.4p', 255);
-- case: bug-hunt-11/datetime/d7b-printf-q-null-precision-and-width
SELECT printf('%5.3Q|', NULL), printf('%-8.3q|', NULL), printf('%.1w', NULL), printf('%.9q', NULL), printf('%.2q', 'abc'), printf('%.2Q', 'abc');
-- case: bug-hunt-11/datetime/d8b-strftime-J-values
SELECT strftime('%J', 0), strftime('%J', 2460370.5), strftime('%J', 1e-3), strftime('%J', 12345.678), strftime('%J', '2024-03-15 12:00:00');
-- case: bug-hunt-11/datetime/d9b-like-escape-checked-before-null-operands
SELECT NULL LIKE 'a' ESCAPE 'ab';
SELECT NULL LIKE NULL ESCAPE '';
SELECT 'a' LIKE 'a' ESCAPE NULL;
SELECT 'a' LIKE NULL ESCAPE 'x';
SELECT like('a', NULL, '');
-- case: bug-hunt-11/datetime/d11b-unistr-quote-nul-and-control
SELECT unistr_quote('a' || char(0) || 'b'), unistr_quote(char(0)), unistr_quote('a' || char(1) || char(0) || 'b'), unistr_quote('x' || char(10)), unistr_quote('plain');
-- case: bug-hunt-11/datetime/d12b-shell-prints-noncharacters-whole
SELECT char(1114111), char(1114110), char(1114109), char(65535), hex(char(1114111));
