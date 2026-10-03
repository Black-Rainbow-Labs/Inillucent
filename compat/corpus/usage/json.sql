-- case: json/extract-and-arrows
SELECT json_extract('{"a":{"b":[1,2,{"c":"x"}]}}', '$.a.b[2].c'), json_extract('{"a":1}', '$.b'), json_extract('{"a":[1,2]}', '$.a'), json_extract('{"a":1,"b":2}', '$.a', '$.b'), json_extract('[1,2,3]', '$[#-1]'), json_extract('{"a b":1}', '$."a b"');
SELECT '{"a":{"b":1}}' -> '$.a', '{"a":{"b":1}}' ->> '$.a', '{"a":"x"}' -> 'a', '{"a":"x"}' ->> 'a', '[10,20]' -> 1, '[10,20]' ->> -1, typeof('{"a":1}' ->> 'a'), typeof('{"a":1.5}' -> 'a');
SELECT json_extract('{"a":true}', '$.a'), '{"a":true}' -> '$.a', '{"a":null}' ->> '$.a', '{"a":null}' -> '$.a', json_extract('{"a":"é"}', '$.a');
SELECT json_extract('not json', '$');
SELECT json_extract('{"a":1}', 'a');
-- case: json/build-and-modify
SELECT json_object('a', 1, 'b', 'x', 'c', NULL, 'd', json('[1,2]'), 'e', 1.5), json_array(1, 'two', NULL, json_array(3)), json_array(), json_object();
SELECT json_set('{"a":1}', '$.b', 2, '$.a', 3), json_insert('{"a":1}', '$.a', 9, '$.c', 3), json_replace('{"a":1}', '$.a', 9, '$.z', 3), json_remove('[1,2,3]', '$[1]'), json_remove('{"a":1,"b":2}', '$.a'), json_set('[1]', '$[#]', 2);
SELECT json_patch('{"a":1,"b":{"c":2}}', '{"b":{"c":null,"d":3},"e":4}'), json_type('{"a":[1]}', '$.a'), json_type('1.5'), json_type('null'), json_valid('{"a":1}'), json_valid('{a:1}'), json_valid('{a:1}', 6), json_array_length('[1,2,3]'), json_array_length('{"a":[1]}', '$.a');
SELECT json('{ "a" : 1 , "b":[ 1 , 2 ] }'), json(' [1, 2] '), json('{"a":1,"a":2}'), json_quote('a"b'), json_quote(1.5), json_quote(NULL), json('{"x":1e400}');
SELECT json('{a:1, b:[1,2,],}'), json('[0x10, .5, +1, Infinity]');
-- case: json/each-and-tree
SELECT key, value, type, atom, id > 0, parent IS NULL, fullkey, path FROM json_each('{"a":1,"b":[2,3],"c":{"d":4}}');
SELECT key, value, type, fullkey, path FROM json_tree('{"a":1,"b":[2,{"c":3}]}');
SELECT value FROM json_each('[5,6,7]') WHERE key > 0;
SELECT value FROM json_each('{"a":{"b":[1,2]}}', '$.a.b');
CREATE TABLE t(id INTEGER PRIMARY KEY, tags TEXT);
INSERT INTO t VALUES(1, '["red","blue"]'), (2, '["blue"]'), (3, '[]'), (4, NULL), (5, 'bad');
SELECT t.id, j.value FROM t, json_each(t.tags) AS j WHERE t.id < 5 ORDER BY t.id, j.key;
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM json_each(t.tags) WHERE value = 'blue') ORDER BY id;
SELECT count(*) FROM t, json_each(t.tags);
-- case: json/aggregates
CREATE TABLE t(g, k, v);
INSERT INTO t VALUES(1, 'a', 1), (1, 'b', 'x'), (2, 'c', NULL);
SELECT g, json_group_array(v), json_group_object(k, v) FROM t GROUP BY g ORDER BY g;
SELECT json_group_array(v ORDER BY k DESC) FROM t;
SELECT json_group_array(json_object('k', k)) FROM t;
-- case: json/stored-column-queries
CREATE TABLE doc(id INTEGER PRIMARY KEY, body TEXT);
INSERT INTO doc VALUES(1, '{"name":"a","n":5,"tags":["x"]}'), (2, '{"name":"b","n":"7"}'), (3, '{"name":"c"}');
SELECT id FROM doc WHERE json_extract(body, '$.n') > 4 ORDER BY id;
SELECT id FROM doc WHERE body ->> '$.n' > 4 ORDER BY id;
SELECT id, body ->> 'name' FROM doc ORDER BY body ->> 'name' DESC;
CREATE INDEX doc_name ON doc(json_extract(body, '$.name'));
SELECT id FROM doc WHERE json_extract(body, '$.name') = 'b';
UPDATE doc SET body = json_set(body, '$.n', 10) WHERE id = 3;
SELECT body FROM doc WHERE id = 3;
SELECT id, json_extract(body, '$.missing') IS NULL FROM doc;
-- case: json/jsonb-functions
SELECT typeof(jsonb('{"a":1}')), json(jsonb('{"a":[1,2]}')), jsonb_extract(jsonb('{"a":5}'), '$.a'), json(jsonb_set(jsonb('{}'), '$.x', 1)), json_valid(jsonb('[1]'), 8), jsonb('{"a":1}') -> '$.a';
SELECT hex(jsonb('1')), hex(jsonb('"a"')), hex(jsonb('[]')), hex(jsonb('{"a":true}'));
-- case: json/errors
SELECT json('{');
SELECT json_extract('{"a":1}', '$.a[');
SELECT json_object('a');
SELECT json_object(1, 2);
SELECT json_each('[1]');
SELECT json_array_length('[1', '$');
SELECT json_error_position('{"a":1,}'), json_error_position('{"a":1}'), json_error_position('[1,2');
-- case: json/r3b-each-and-tree-ids-at-a-path
SELECT key, id, parent FROM json_each('[1,[2,3],{"x":4}]', '$[1]');
SELECT key, id, parent FROM json_tree('[1,[2,3],{"x":4}]', '$[1]');
SELECT key, id, parent FROM json_tree('{"a":{"b":[1,2]},"c":2}', '$.a');
SELECT key, id, parent FROM json_each('{"a":{"b":[1,2]},"c":2}', '$.a.b');
-- case: json/r3b-blob-that-is-not-jsonb-is-read-as-text
SELECT json(X'5B312C325D'), json_valid(X'5B312C325D'), json_type(X'7B2261223A5B5D7D', '$.a');
SELECT json_extract(X'7B2261223A327D', '$.a'), json_array_length(X'5B312C322C335D');
SELECT json_valid(X'ff'), json_valid(X'');
-- case: json/r3b-json5-unicode-whitespace
SELECT json(char(0x1680)||'1'), json(char(0x2007)||'[1]'), json(char(0x202F)||'2'), json(char(0x205F)||'3'), json(char(0x3000)||'4');
SELECT json_valid(char(0x2028)||'1'), json_valid(char(0x00A0)||'1'), json_valid(char(0x200B)||'1');
-- case: json/r3b-lone-surrogates
SELECT hex(json_extract('["\ud800"]','$[0]')), hex(json_extract('["x\udfff"]','$[0]')), hex(json_extract('["\ud800\u0041"]','$[0]'));
SELECT hex(json_extract('["\ud83d\ude00"]','$[0]')), hex(json_extract('["\ude00\ud83d"]','$[0]'));
