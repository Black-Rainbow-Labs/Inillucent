-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/json/65-json-valid-on-a-blob-that-holds-text-bytes-and-on-
SELECT json_valid(x'696e76616c6964');
SELECT json_valid(x'7b7d');
SELECT json_valid(jsonb('{"a":1}')), json_valid('{"a":1}'), json_valid('{"a":1}', 8), json_valid(jsonb('{"a":1}'), 8);
CREATE TABLE test(data BLOB CHECK(json_valid(data)));
INSERT INTO test VALUES (x'696e76616c6964');
-- case: holdout/json/85-json-valid-with-no-arguments-and-with-too-many
SELECT json_valid();
SELECT json_valid('1', 2, 3);
SELECT json_array_length();
SELECT abs();
-- case: holdout/json/118-json-each-with-a-column-argument-and-a-path
CREATE TABLE t(id INT, data TEXT);
INSERT INTO t VALUES (1, '{"tags":["a","b"]}'), (2, '{"tags":["c"]}');
SELECT t.id, j.value FROM t, JSON_EACH(t.data, '$.tags') j ORDER BY t.id, j.key;
SELECT t.id, j.key, j.value FROM t, JSON_EACH(t.data) j ORDER BY t.id, j.key;
CREATE TABLE t2(id INT, data TEXT);
INSERT INTO t2 VALUES (1, '{"name":"alice","age":30}');
SELECT j.key, j.value, j.type FROM t2, JSON_TREE(t2.data) j WHERE j.type NOT IN ('object');
-- case: holdout/json/142-json-set-json-insert-json-remove-with-a-null-docum
SELECT typeof(json_set(NULL, '$.a', 1)), typeof(json_insert(NULL, '$.a', 1)), typeof(json_replace(NULL, '$.a', 1)), typeof(json_remove(NULL, '$.a'));
SELECT COALESCE(JSON_INSERT(NULL, '$.a', 1), 'IS_NULL');
SELECT json_set('{"a":1}', '$.a', NULL), json_set('{"a":1}', NULL, 2), json_set('{"a":1}', '$.b', NULL);
-- case: holdout/json/143-json-extract-and-unescape-json-string-escapes
SELECT JSON_EXTRACT('{"key":"value with \"quotes\""}', '$.key');
SELECT JSON_EXTRACT('{"a":"line1\nline2"}', '$.a') = 'line1' || char(10) || 'line2';
SELECT LENGTH(JSON_EXTRACT('{"a":"hello\nworld"}', '$.a'));
SELECT JSON_EXTRACT('{"a":"back\\slash"}', '$.a');
SELECT '{"a":"xé😀"}' ->> '$.a', '{"a":"xé"}' -> '$.a';
-- case: holdout/json/144-json-group-array-json-group-object-over-zero-rows
CREATE TABLE empty_t(val INTEGER);
SELECT JSON_GROUP_ARRAY(val) FROM empty_t;
SELECT JSON_GROUP_OBJECT('k', val) FROM empty_t;
SELECT COALESCE(JSON_GROUP_ARRAY(val), 'IS_NULL') FROM empty_t;
SELECT group_concat(val), sum(val), total(val), count(val), max(val) FROM empty_t;
-- case: holdout/json/145-json-group-array-over-text-produced-by-json-extrac
CREATE TABLE config(id INTEGER PRIMARY KEY, data TEXT);
INSERT INTO config VALUES (1, '{"name":"Alice","age":30}');
INSERT INTO config VALUES (2, '{"name":"Bob","age":25}');
SELECT JSON_GROUP_ARRAY(JSON_EXTRACT(data, '$.name')) FROM config;
SELECT JSON_GROUP_ARRAY(data ->> '$.name') FROM config;
SELECT JSON_GROUP_ARRAY(JSON_EXTRACT(data, '$.age')) FROM config;
SELECT JSON_GROUP_ARRAY(data -> '$.name') FROM config;
-- case: holdout/json/146-json-group-object-stringifies-a-value-that-came-fr
CREATE TABLE t(grp TEXT, val INT);
INSERT INTO t VALUES ('A', 1), ('A', 2), ('B', 3), ('B', 4);
SELECT json_group_object(grp, vals) FROM (SELECT grp, json_group_array(val) AS vals FROM t GROUP BY grp);
SELECT json_group_object(grp, json(vals)) FROM (SELECT grp, json_group_array(val) AS vals FROM t GROUP BY grp);
-- case: holdout/json/147-json-subtype-survives-group-by-in-json-group-array
CREATE TABLE t(id INTEGER PRIMARY KEY, grp TEXT, key TEXT, val INT);
INSERT INTO t VALUES (1, 'a', 'x', 10), (2, 'a', 'y', 20);
INSERT INTO t VALUES (3, 'b', 'x', 30), (4, 'b', 'z', 40);
SELECT grp, json_group_array(json_object('k', key, 'v', val)) FROM t GROUP BY grp ORDER BY grp;
SELECT json_group_array(json_object('k', 'x', 'v', 10)) FROM (SELECT 1 UNION ALL SELECT 2);
SELECT grp, json_group_array(json(json_object('k', key))) FROM t GROUP BY grp ORDER BY grp;
-- case: holdout/json/177-json-array-length-null-is-null
SELECT json_array_length(NULL), json_array_length('[]'), json_array_length('{}'), json_array_length('[1,[2,3]]', '$[1]'), json_array_length('[1]', '$.x'), json_array_length('abc');
-- case: holdout/json/180-json-each-on-a-column-with-order-by-a-path-and-whe
CREATE TABLE t1(id INTEGER PRIMARY KEY, a TEXT);
INSERT INTO t1 VALUES(1,'[1,2]'),(2,'[3,4]');
SELECT t1.id, j.value FROM t1, json_each(t1.a) AS j ORDER BY t1.id;
CREATE TABLE t2(data TEXT);
INSERT INTO t2 VALUES('{"items":[1,2,3]}');
SELECT j.value FROM t2, json_each(t2.data, '$.items') AS j;
CREATE TABLE t3(id INTEGER PRIMARY KEY, data TEXT);
INSERT INTO t3 VALUES(1, '{"items":[1,2,3]}');
SELECT j.value FROM t3, json_each(t3.data, '$.items') AS j WHERE j.value > 1 AND t3.id = 1;
SELECT j.value FROM t3, json_each(json_extract(t3.data, '$.items')) AS j WHERE j.value >= 2;
-- case: holdout/json/203-bracket-quoted-key-in-a-json-path
SELECT json_extract('{"key with spaces":1}', '$["key with spaces"]');
SELECT json_extract('{"key with spaces":1}', '$."key with spaces"');
SELECT json_extract('{"a":[1,2,3]}', '$.a[#-1]'), json_extract('{"a":[1,2,3]}', '$.a[5]'), json_extract('[1,2]', '$[0]', '$[1]');
-- case: holdout/json/233-json-each-with-a-correlated-column-and-a-path-argu
CREATE TABLE jdata(id INTEGER PRIMARY KEY, doc TEXT);
INSERT INTO jdata VALUES(1, '{"tags":["x","y"]}');
SELECT jdata.id, je.value FROM jdata, json_each(jdata.doc, '$.tags') je;
SELECT value FROM json_each('{"tags":["x","y"]}', '$.tags');
-- case: holdout/json/275-a-json-parse-error-in-a-select-does-not-roll-back-
CREATE TABLE t(id INT);
BEGIN;
INSERT INTO t VALUES(9);
SELECT json_extract('not json','$.a');
SELECT count(*) FROM t;
COMMIT;
SELECT count(*) FROM t;
-- case: holdout/json/308-json-set-with-an-append-element-after-an-array-ind
SELECT json_set('[[1,2,3]]', '$[0][#]', 4);
SELECT json_set('[1,2]', '$[#]', 3), json_set('[1,2]', '$[#-1]', 9), json_insert('[1,2]', '$[#]', 3), json_set('{}', '$.a[#]', 1), json_set('[1]', '$[5]', 2);
-- case: holdout/json/325-jsonb-blob-stored-in-a-strict-table-column-declare
CREATE TABLE a (a BLOB) STRICT;
INSERT INTO a VALUES (jsonb('{"a": "b"}'));
CREATE TABLE b (a TEXT) STRICT;
INSERT INTO b VALUES (jsonb('{"a": "b"}'));
INSERT INTO b VALUES (json('{"a": "b"}'));
CREATE TABLE c (a jsonb) STRICT;
SELECT typeof(a), json(a) FROM a;
-- case: holdout/json/333-json-valid-x-with-one-argument-is-strict-rfc-8259-
SELECT json_valid('[1,2,]'), json_valid('{"a":1,}'), json_valid('{a:1}'), json_valid('[1,2]//c'), json_valid('[1,2]'), json_valid('[+1]'), json_valid('[0x10]'), json_valid('[.5]'), json_valid(''' ');
SELECT json_valid('[1,2,]', 2), json_valid('{a:1}', 2), json_valid('{a:1}', 6), json_valid('[1,2]', 8), json_valid('{a:1}', 1);
SELECT json('{a:1}'), json('[1,2,]'), json('[0x10]');
-- case: holdout/json/334-json5-whitespace-and-line-continuations-accepted-b
select json(char(0x00A0)||'1');
select json(char(0xFEFF)||'{"a":1}');
select json('[1,'||char(0x2028)||'2]');
select json('[1,'||char(0x2029)||'2]');
select json(char(0x2000)||'1');
select json(char(0x000B)||'1');
select json(char(0x000C)||'1');
select json('"a\' || char(10) || 'b"');
select json('[1, /* c */ 2] // x');
-- case: holdout/json/335-json-group-object-with-a-numeric-label
SELECT json_group_object(1, 2);
SELECT json_group_object('a', 'b');
CREATE TABLE t(c1 INT, c2 INT);
INSERT INTO t VALUES (1, 2);
SELECT json_group_object(c1, c2) FROM t;
CREATE TABLE t2(c1 TEXT, c2 INT);
INSERT INTO t2 VALUES ('1', 2);
SELECT json_group_object(c1, c2) FROM t2;
SELECT json_group_object(NULL, 1), json_group_object(1.5, 1), json_group_object(x'41', 1);
-- case: holdout/json/368-in-constraint-on-generate-series-and-json-each-hid
SELECT count(*) FROM generate_series WHERE start = 1 AND stop = 5;
SELECT count(*) FROM generate_series WHERE start = 1 AND stop IN (5);
SELECT count(*) FROM json_each WHERE json IN ('[1,2,3]');
SELECT count(*) FROM json_each('[1,2,3]') WHERE json = '[1,2,3]';
-- case: holdout/json/374-json-each-returns-decoded-sql-text-for-json-string
SELECT key, value, hex(value), length(value) FROM json_each('["plain","with \"quotes\"","back\\slash","new\nline"]');
SELECT value, hex(value), length(value) FROM json_each('["ctrl\u0001end"]');
SELECT atom, hex(atom), type FROM json_each('["a\"b", 1, 2.5, true, null, {"k":"v"}, [1]]');
-- case: holdout/json/381-jsonb-insert-and-jsonb-set-that-grow-an-array-prod
SELECT json(jsonb_insert('[]','$[#]',1));
SELECT hex(jsonb_insert('[]','$[#]',1));
SELECT hex(jsonb_insert('[]','$[#]',1,'$[#]',2));
SELECT hex(jsonb_insert('{"t":[[[]]]}','$.t[0][0][#]',1));
SELECT json(jsonb_set('{"t":[[[]]]}','$.t[0][0][#]',1));
SELECT json(jsonb_set('[1]','$[1]','two')), json(jsonb_insert('[1,2]','$[1]','x')), json(jsonb_replace('[1,2]','$[5]','x'));
-- case: holdout/json/382-unpaired-surrogate-escapes-in-json-strings
SELECT length(json_extract('["a\ud800b"]','$[0]'));
SELECT hex(json_extract('["a\ud800bcd"]','$[0]'));
SELECT hex(json_extract('["a\udc00bcd"]','$[0]'));
SELECT hex(json_extract('["😀"]','$[0]')), hex(json_extract('["\ud83d"]','$[0]')), hex(json_extract('["\u0000x"]','$[0]'));
-- case: holdout/json/392-json-functions-render-reals-with-15-significant-di
CREATE TABLE t(x REAL);
INSERT INTO t VALUES(1),(2),(4);
SELECT json_object('avg',avg(x)) FROM t;
SELECT json_quote(1e-5);
SELECT json_quote(-8487739174.3030205);
SELECT CAST(avg(x) AS TEXT) FROM t;
SELECT json_array(0.1+0.2, 1e100, 1e-7, 123456789012345678, 1.5e15, 1e15, 1e16, -0.0, 100.0, 2.5e-5);
SELECT json_quote(0.1+0.2), json_quote(1.0), json_quote(1e22);
-- case: holdout/json/397-json-each-over-a-null-column-yields-no-rows
CREATE TABLE t(id INTEGER, tags);
INSERT INTO t VALUES(1,'["x"]'),(2,NULL);
SELECT count(*) FROM t, json_each(t.tags);
SELECT quote(key), quote(value), type FROM t, json_each(t.tags) WHERE t.id=2;
SELECT count(*) FROM t, json_tree(t.tags);
SELECT count(*) FROM json_each('[]');
SELECT count(*) FROM json_each(NULL);
SELECT count(*) FROM t LEFT JOIN json_each(t.tags) ON 1;
-- case: holdout/json/443-text-column-compared-with-the-value-column-of-json
CREATE TABLE t(x TEXT);
CREATE TABLE u(x TEXT);
INSERT INTO t VALUES('1'),('01'),('x');
INSERT INTO u VALUES('1'),('01'),('x');
SELECT count(*) FROM t JOIN json_each('[1]') ON x = value;
SELECT count(*) FROM t JOIN generate_series(1,1) ON x = value;
DELETE FROM u WHERE x IN (SELECT value FROM json_each('[1]'));
SELECT group_concat(x) FROM u;
SELECT x FROM t WHERE x IN (SELECT value FROM generate_series(1,1)) ORDER BY x;
SELECT typeof(value) FROM json_each('[1]'), generate_series(1,1) LIMIT 1;
-- case: holdout/json/444-jsonb-null-is-null
CREATE TABLE t(id INTEGER PRIMARY KEY, j TEXT, b BLOB);
CREATE TABLE u(b BLOB NOT NULL);
INSERT INTO t(id,j) VALUES(1,'{"a":1}'),(2,NULL);
UPDATE t SET b = jsonb(j);
SELECT count(*) FROM t WHERE b IS NULL;
SELECT quote(b), typeof(b) FROM t WHERE id = 2;
INSERT INTO u VALUES(jsonb(NULL));
SELECT json(NULL), jsonb_array(NULL), json_array(NULL), json_object('a', NULL), jsonb_extract(NULL, '$'), json_valid(NULL), json_type(NULL);
-- case: holdout/json/457-json-group-object-with-a-null-label
CREATE TABLE t(k TEXT, n INT);
CREATE TABLE u(b BLOB);
INSERT INTO t VALUES('a',1),(NULL,2),('b',3);
INSERT INTO u SELECT jsonb_group_object(k,n) FROM t;
SELECT hex(b) FROM u;
SELECT json(b) FROM u;
SELECT json_group_object(k,n) FROM t;
-- case: holdout/json/468-jsonb-extract-returns-a-jsonb-blob-for-arrays-and-
SELECT typeof(jsonb_extract(jsonb('{"a":[1]}'), '$.a'));
SELECT typeof(jsonb_extract(jsonb('{"a":{"b":1}}'), '$.a'));
SELECT typeof(jsonb_extract('{"a":[1]}', '$.a')), typeof(json_extract('{"a":[1]}', '$.a')), typeof(jsonb_extract('{"a":1}', '$.a')), typeof(jsonb_extract('{"a":"x"}', '$.a')), typeof(jsonb_extract('{"a":null}', '$.a'));
-- case: holdout/json/541-duplicate-keys-in-json-and-json-patch
SELECT json('{"a": 2, "a": 5}');
SELECT json('{"a": 5, "a": 4}');
SELECT json_patch('{"a": 5, "a": 4}', '{"a":3, "b": 5}');
SELECT json_extract('{"a": 5, "a": 4}', '$.a'), json_set('{"a":1,"a":2}', '$.a', 9), json_remove('{"a":1,"a":2}', '$.a'), jsonb('{"a":1,"a":2}') = jsonb('{"a":1,"a":2}');
SELECT json_group_object(k, v) FROM (SELECT 'a' k, 1 v UNION ALL SELECT 'a', 2);
SELECT json_object('a', 1, 'a', 2);
-- case: holdout/json/564-json-set-with-a-path-element-on-an-array
select json_set('[0,1,2]','$[#]','new');
select json_set('[0,1,2]','$[#-1]','new'), json_insert('[0,1,2]','$[#]','new'), json_replace('[0,1,2]','$[#]','new'), json_remove('[0,1,2]','$[#-1]'), json_extract('[0,1,2]','$[#-1]'), json_extract('[0,1,2]','$[#]');
-- case: holdout/json/611-json-on-a-blob-argument
select cast(x'48656C6C6F' as text);
select json(x'48656C6C6F');
select json(x'7B2261223A317D');
CREATE TABLE t(a);
INSERT INTO t VALUES (x'74727565');
SELECT json_object('value', a) FROM t;
SELECT json_array(x'41'), json_quote(x'41');
SELECT json_valid(x'48656C6C6F'), json_valid(x'7B7D'), json_type(x'7B7D');
-- case: holdout/json/612-json-object-with-an-infinite-real
create table t(a);
insert into t values (1e309);
select json_object('k', a) from t;
select json_array(1e309, -1e309), json_quote(1e309), json(9.0e+999), json_extract('{"a":9.0e+999}', '$.a'), json_extract('{"a":1e999}', '$.a'), json_extract('[1e999]', '$[0]') = 1e999;
-- case: holdout/json/625-the-array-length-operator-in-json-paths
SELECT json_type('[1,2,3]', '$[#]');
SELECT json_extract('[1,2]', '$[#]');
SELECT json_set('[1,2,3]', '$[#]', 4);
SELECT json_insert('[1,2,3]', '$[#]', 4);
SELECT json_replace('[1,2,3]', '$[#]', 4);
SELECT json_remove('[1,2,3]', '$[#]');
SELECT * FROM json_each('[1,2,3]', '$[#]');
SELECT * FROM json_tree('[1,2,3]', '$[#]');
SELECT json_extract('[1,2,3]', '$[#-1]'), json_extract('[1,2,3]', '$[#-4]'), json_extract('[1,2,3]', '$[#-0]');
-- case: holdout/json/641-json-each-inside-nested-subqueries
SELECT EXISTS (SELECT 1 FROM json_each('[{"x":1}]') WHERE value IN (SELECT value FROM json_each('[1]')));
CREATE TABLE important_data (id INTEGER PRIMARY KEY, value TEXT);
INSERT INTO important_data VALUES (1, 'keep'), (2, 'delete'), (3, 'keep');
DELETE FROM important_data WHERE EXISTS (SELECT 1 FROM json_each('["delete"]') WHERE value IN (SELECT value FROM json_each('["delete"]')) AND important_data.value = json_each.value);
SELECT COUNT(*) FROM important_data;
-- case: holdout/json/667-the-id-and-parent-columns-of-json-tree-and-json-ea
SELECT key, value, id, parent, fullkey, path, atom, type FROM JSON_TREE('{"a":{"b":1},"c":2}');
SELECT key, value, id, parent FROM JSON_EACH('{"a":{"b":1},"c":[1,2]}');
SELECT key, id, parent FROM JSON_TREE('[1,[2,3],{"x":4}]');
SELECT id, parent FROM JSON_TREE('{"a":{"b":1},"c":2}', '$.a');
-- case: holdout/json/704-json-insert-and-json-replace-need-an-odd-number-of
SELECT json_insert('{}', '$.a');
SELECT json_replace('{}', '$.a');
SELECT jsonb_insert('{}', '$.a');
SELECT jsonb_replace('{}', '$.a');
SELECT json_insert('{}', '$.a', 1);
SELECT json_set('{}', '$.a');
SELECT json_set('{}');
SELECT json_set('{}', '$.a', 1, '$.b');
SELECT json_remove('{}'), json_remove('{"a":1}', '$.a', '$.b');
-- case: holdout/json/705-jsonb-group-object-with-a-numeric-label
SELECT hex(jsonb_group_object(1, 2));
SELECT json(jsonb_group_object(1, 2));
SELECT hex(jsonb_group_object('1', 2));
SELECT json(jsonb_group_object('1', 2));
CREATE TABLE t(c1 INT, c2 INT);
INSERT INTO t VALUES (1, 2);
SELECT hex(jsonb_group_object(c1, c2)) FROM t;
-- case: holdout/json/706-jsonb-patch-exists
SELECT jsonb_patch('{"a":1}', '{"b":2}') IS NOT NULL;
SELECT json(jsonb_patch('{"a":1}', '{"b":2}'));
SELECT name FROM pragma_function_list WHERE name = 'jsonb_patch';
SELECT json_patch('{"a":1}', '{"b":2}'), json_patch('{"a":1,"b":2}', '{"a":null}'), json_patch('{"a":{"b":1}}', '{"a":{"c":2}}'), json_patch('[1,2]', '{"a":1}'), json_patch('{"a":1}', '[1]'), json_patch(NULL, '{}'), json_patch('{}', NULL);
-- case: holdout/json/793-json-array-1e999-is-accepted
SELECT json_array(1e999);
SELECT json_extract(json_array(1e999), '$[0]'), json_array(-1e999), json_array(1e999) = '[9.0e+999]', json_type(json_array(1e999), '$[0]');
-- case: holdout/json/801-json-set-json-insert-and-json-replace-embed-json-v
SELECT json_insert('{}', '$.a', json('{"x":2}'));
SELECT json_set('{"a":1}', '$.a', json('[1,2]')), json_replace('{"a":1}', '$.a', json_array(1,2)), json_insert('{}', '$.a', json_object('k', 1));
CREATE TABLE t(g, v);
INSERT INTO t VALUES (1, 'a'), (1, 'b');
SELECT json_set('{}', '$.arr', (SELECT json_group_array(v) FROM t));
SELECT json_set('{}', '$.arr', (SELECT json_group_array(v) FROM t) || '');
SELECT json_set('{}', '$.s', '{"x":2}'), json_set('{}', '$.s', json_quote('{"x":2}'));
-- case: holdout/json/816-json-valid-and-json-type-of-integer-and-real-sql-v
SELECT json_valid(123), json_valid(1.5), json_valid(NULL), json_valid(x'7B7D'), json_valid(1e999), json_valid(-0.0);
SELECT json_type(1), json_type(1.5), json_type(NULL), json_type('1'), json_type('null'), json_type('true'), json_type(1e999);
SELECT json_array_length(1), json_extract(1, '$'), json_extract(1.5, '$'), json(1), json(1.5), json_quote(1), json_extract(5, '$.a');
-- case: holdout/json/819-json-group-array-and-json-group-object-with-aggreg
CREATE TABLE t(k, v);
INSERT INTO t VALUES ('b', 2), ('a', 1), ('c', 3), ('a', 1);
SELECT json_group_array(v ORDER BY k) FROM t;
SELECT json_group_array(v ORDER BY k DESC) FROM t;
SELECT json_group_array(DISTINCT v) FROM t;
SELECT json_group_array(DISTINCT v ORDER BY v DESC) FROM t;
SELECT json_group_object(k, v ORDER BY k DESC) FROM t;
SELECT group_concat(k ORDER BY v DESC, k), group_concat(DISTINCT k ORDER BY k DESC) FROM t;
SELECT json_group_array(v ORDER BY -v) FROM t;
