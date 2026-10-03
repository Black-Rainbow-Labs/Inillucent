-- case: orm/7/7.1-json-extract-returns-sql-types-text-integer-real-null-json-t
SELECT json_extract('{"a":1,"b":"x","c":null,"d":[1,2],"e":{"f":true},"g":1.5}','$.a','$.b','$.c','$.d','$.e','$.g'), json_extract('{"e":{"f":true}}','$.e.f'), typeof(json_extract('{"e":{"f":true}}','$.e.f'));
-- case: orm/7/7.2-json-extract-single-path-vs-multiple-paths-array-result
SELECT json_extract('{"a":1,"b":2}','$.a'), json_extract('{"a":1,"b":2}','$.a','$.b'), json_extract('{"a":1}','$.zz'), json_extract('{"a":1}','$.a','$.zz');
-- case: orm/7/7.3-returns-json-text-returns-sql-value-3-38
SELECT '{"a":"x","b":[1,2],"c":2}' -> '$.a', '{"a":"x"}' ->> '$.a', '{"b":[1,2]}' -> 'b', '{"b":[1,2]}' ->> 'b', '{"b":[10,20]}' -> '$.b[1]', '[1,2,3]' ->> 1, '[1,2,3]' ->> -1, typeof('{"c":2}' ->> 'c');
-- case: orm/7/7.4-json-extract-path-syntax-bracket-quoted-key-negative-index-1
SELECT json_extract('[10,20,30]','$[1]'), json_extract('[10,20,30]','$[#-1]'), json_extract('{"a b":1}','$."a b"'), json_extract('{"a.b":1}','$."a.b"'), json_extract('[1]','$[5]');
-- case: orm/7/7.5-malformed-json-raises-error
SELECT json_extract('{bad', '$.a');
-- case: orm/7/7.6-path-without-leading-raises-json-path-error
SELECT json_extract('{"a":1}','a');
-- case: orm/7/7.7-json-valid-and-json-type
SELECT json_valid('{"a":1}'), json_valid('{a:1}'), json_valid(NULL), json_valid(''), json_type('{"a":[1]}','$.a'), json_type('1.5'), json_type('"x"'), json_type('null'), json_type('true'), json_type('{"a":1}','$.b');
-- case: orm/7/7.8-json5-accepted-by-json-valid-since-3-42
SELECT json_valid('{a:1}'), json_valid('[1,2,]'), json('{a:1,b:''x''}'), json_valid('{a:1}',1), json_valid('[1,2,]',2), json('[1,2,]');
-- case: orm/7/7.9-json-minifies-json-array-json-object-build-text
SELECT json(' { "a" : [ 1 , 2 ] } '), json_array(1,'a',NULL,1.5,x'41' IS NULL), json_object('k',1,'n',NULL,'o',json_object('z',1)), json_array(json('[1]'), '[1]');
-- case: orm/7/7.10-json-object-with-duplicate-keys-keeps-both-extract-takes-fir
SELECT json_object('a',1,'a',2), json_extract('{"a":1,"a":2}','$.a');
-- case: orm/7/7.11-json-object-odd-arguments-error
SELECT json_object('a');
-- case: orm/7/7.12-json-object-with-blob-value-errors
SELECT json_object('a', x'41');
-- case: orm/7/7.13-json-set-json-insert-json-replace-semantics
SELECT json_set('{"a":1}','$.a',2,'$.b',3), json_insert('{"a":1}','$.a',2,'$.b',3), json_replace('{"a":1}','$.a',2,'$.b',3), json_set('{}','$.a.b',1), json_set('[1]','$[#]',9);
-- case: orm/7/7.14-json-remove-and-json-patch-rfc-7396
SELECT json_remove('{"a":1,"b":2}','$.a'), json_remove('[1,2,3]','$[0]'), json_patch('{"a":1,"b":2}','{"a":null,"c":3}'), json_patch('{"a":{"x":1}}','{"a":{"y":2}}');
-- case: orm/7/7.15-json-array-length-and-json-array-length-of-non-array
SELECT json_array_length('[1,2,3]'), json_array_length('{"a":[1,2]}','$.a'), json_array_length('{}'), json_array_length('5');
-- case: orm/7/7.16-json-each-rows-key-value-type-atom-id-parent-fullkey-path
SELECT key, value, type, atom, fullkey, path FROM json_each('{"a":1,"b":[1,2],"c":null}');
-- case: orm/7/7.17-json-each-over-array-in-a-join-unnest-tags
CREATE TABLE t(id INTEGER PRIMARY KEY, tags TEXT); INSERT INTO t VALUES(1,'["a","b"]'),(2,'["b"]'); SELECT t.id, j.value FROM t, json_each(t.tags) j ORDER BY 1,2; SELECT j.value, count(*) FROM t, json_each(t.tags) j GROUP BY 1 ORDER BY 1;
-- case: orm/7/7.18-json-tree-walks-all-nodes
SELECT id, parent, key, type, fullkey FROM json_tree('{"a":[1,{"b":2}]}');
-- case: orm/7/7.19-json-group-array-json-group-object
CREATE TABLE t(g,v); INSERT INTO t VALUES('a',1),('a',2),('b',NULL); SELECT g, json_group_array(v) FROM t GROUP BY g ORDER BY g; SELECT json_group_object(g, v) FROM t;
-- case: orm/7/7.20-json-extract-filter-in-where-with-numeric-and-boolean-compar
CREATE TABLE t(id INTEGER PRIMARY KEY, doc TEXT); INSERT INTO t VALUES(1,'{"age":30,"ok":true,"name":"Al"}'),(2,'{"age":"30","ok":false}'); SELECT id FROM t WHERE json_extract(doc,'$.age')=30; SELECT id FROM t WHERE json_extract(doc,'$.ok'); SELECT id FROM t WHERE doc->>'$.name'='Al';
-- case: orm/7/7.21-json-true-false-extract-to-integer-1-0-and-json-number-preci
SELECT json_extract('true','$'), json_extract('false','$'), typeof(json_extract('true','$')), json_extract('12345678901234567890','$'), json_extract('1.0','$'), json_extract('1e2','$'), json_extract('"\u00e9"','$');
-- case: orm/7/7.22-json-quote-and-quoting-of-text-vs-json-subtype
SELECT json_quote('a"b'), json_quote(1), json_quote(NULL), json_quote(json('[1]')), json_array('[1]'), json_array(json('[1]'));
-- case: orm/7/7.23-json-stored-in-a-column-check-json-valid-col
CREATE TABLE t(d TEXT CHECK(json_valid(d))); INSERT INTO t VALUES('{"a":1}'); INSERT INTO t VALUES('nope');
-- case: orm/7/7.24-jsonb-functions-jsonb-round-trip-to-text-blob-type
SELECT typeof(jsonb('{"a":1}')), json(jsonb('{"a":[1,2]}')), json_extract(jsonb('{"a":1}'),'$.a'), json_valid(jsonb('{}')), json_valid(jsonb('{}'),8);
-- case: orm/7/7.25-json-pretty-3-46
SELECT json_pretty('{"a":[1,2],"b":{}}');
-- case: orm/7/7.26-json-array-insert-3-53-not-in-3-51
SELECT json_array_insert('[1,2]','$[1]',9);
-- case: orm/7/7.27-django-jsonfield-uses-json-valid-check-constraint-and-json-e
CREATE TABLE t(data text NOT NULL CHECK((JSON_VALID("data") OR "data" IS NULL))); INSERT INTO t VALUES('{"k":{"a":[1,2]}}'); SELECT JSON_EXTRACT(data, '$.k.a[1]'), JSON_EXTRACT(data, '$.k.zz') IS NULL FROM t;
-- case: orm/7/7.28-json-extract-in-generated-column-index
CREATE TABLE t(doc TEXT, name TEXT GENERATED ALWAYS AS (json_extract(doc,'$.name')) VIRTUAL); CREATE INDEX i ON t(name); INSERT INTO t(doc) VALUES('{"name":"z"}'); SELECT name FROM t WHERE name='z'; EXPLAIN QUERY PLAN SELECT * FROM t WHERE name='z';
