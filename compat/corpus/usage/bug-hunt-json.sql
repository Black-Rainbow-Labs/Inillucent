-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/json/json_extract_multiple_paths_and_null
-- json_extract with a NULL among several paths
SELECT json_extract('{"a":1,"b":[2]}', '$.a', '$.b'), json_extract('{"a":1}', '$.a', '$.z'), json_extract('{"a":1}', '$.z', '$.y'), json_extract('{"a":"x"}', '$.a', '$.a'), json_extract('{"a":1}', '$.a', NULL);

-- case: bug-hunt/json/json_string_escapes_control_chars
-- DELETE is left raw in a JSON string
SELECT json_quote(char(1)), json_quote(char(8)), json_quote(char(9)), json_quote(char(10)), json_quote(char(12)), json_quote(char(13)), json_quote(char(31)), json_quote(char(34)), json_quote(char(47)), json_quote(char(92)), json_quote(char(127)), json_quote(char(128)), json_quote(char(128512)), json_quote('');
SELECT json_extract('"\u0000"', '$') IS NOT NULL, hex(json_extract('"\u00e9"', '$')), json_extract('"\/"', '$'), json_valid('"\u12"'), json_valid('"\q"');

-- case: bug-hunt/json/json_array_object_building
-- the same through json_array and json_object
SELECT json_array(1, 1.5, 'x', NULL, json('[1]'), '[2]', 1e100, -0.0, 9e999), json_object('a', 1, 'b', NULL, 'c', json('{"d":1}'), 'e', 'f'), json_array(), json_object(), json_array(1, 'a''b', 'q"q', 'back\slash', char(10), char(31), char(127), char(0));
SELECT json_array(x'01');

-- case: bug-hunt/json/json_group_array_over_subquery_subtype
-- the JSON subtype does not cross a flattened derived table
CREATE TABLE foo (a TEXT, b TEXT);
INSERT INTO foo (a,b) VALUES ('a1','b1'),('a2','b2');
SELECT json_group_array(obj) FROM (SELECT rowid AS id, json_object('a', a, 'b', b) AS obj FROM foo);
SELECT json_group_array(json(obj)) FROM (SELECT rowid AS id, json_object('a', a, 'b', b) AS obj FROM foo);

-- case: bug-hunt/json/json_array_subquery_extract_subtype
-- the same through json_each
SELECT json_array(d) FROM (SELECT json_extract(value, '$') AS d FROM json_each(json_array('{"k1": "v1"}')));
SELECT json_array(json(d)) FROM (SELECT json_extract(value, '$') AS d FROM json_each(json_array('{"k1": "v1"}')));

-- case: bug-hunt/json/json_quote_view_not_pushdown
-- the same for a condition pushed into a view
CREATE TABLE t1 (c0);
CREATE VIEW v0(c0) AS SELECT json(TRUE);
INSERT INTO t1 VALUES ('x');
SELECT * FROM v0, t1;
SELECT NOT json_quote(v0.c0) FROM v0, t1;
SELECT * FROM v0, t1 WHERE NOT json_quote(v0.c0);

-- case: bug-hunt/json/json_whitespace_form_feed_invalid
-- form feed is not RFC 8259 whitespace
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x20),char(0x20),char(0x20),char(0x20),char(0x20),char(0x20)));
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x0C),char(0x0C),char(0x0C),char(0x0C),char(0x0C),char(0x0C)));
SELECT json_valid(printf('%s{%s"x"%s:%s9%s}%s',char(0x20,0x09,0x0a,0x0c,0x0d,0x20),'','','','',''));

-- case: bug-hunt/json/json_deep_nesting_limit
-- nesting of exactly 1000 is accepted
SELECT json_valid(printf('%.1000c0%.1000c','[',']'));
SELECT json_valid(printf('%.1001c0%.1001c','[',']'));
SELECT json_valid(replace(printf('%.1000c0%.1000c','[','}'),'[','{"a":'));
SELECT json_valid(replace(printf('%.1001c0%.1001c','[','}'),'[','{"a":'));

-- case: bug-hunt/json/json-subtype-through-derived-tables
-- json_quote and json_array over derived table columns
CREATE TABLE t1 (c0);
CREATE VIEW v0(c0) AS SELECT json(TRUE);
INSERT INTO t1 VALUES ('x');
SELECT NOT json_quote(v0.c0), json_quote(v0.c0), typeof(v0.c0) FROM v0, t1;
SELECT * FROM v0, t1 WHERE NOT json_quote(v0.c0);
SELECT * FROM v0 WHERE NOT json_quote(v0.c0);
SELECT * FROM (SELECT json(1) AS c0) WHERE json_quote(c0) = '1';
SELECT json_quote(c0) FROM (SELECT json(1) AS c0);
SELECT json_quote(c0) FROM (SELECT json('[1]') AS c0);
SELECT json_array(c0) FROM (SELECT json('[1]') AS c0);
SELECT json_array(c0) FROM (SELECT json('[1]') AS c0 UNION ALL SELECT json('[2]'));
SELECT json_array(c0) FROM (SELECT json('[1]') AS c0 LIMIT 1);
WITH x(c) AS (SELECT json('[1]')) SELECT json_array(c) FROM x;
SELECT json_group_array(c0) FROM (SELECT json('[1]') AS c0);
SELECT json_quote(json('[1]')), json_array(json('[1]'));
SELECT 0.5 IS TRUE COLLATE NOCASE, json_valid(char(12)||'1'), json_valid(printf('%.1000c0%.1000c','[',']'));
-- case: bug-hunt/json/json-each-value-of-a-container-is-json
-- a json_each or json_tree value that is an array or an object carries the JSON subtype
SELECT json_group_array(value) FROM json_each('[{"a":1},[2],3,"x",null,true]');
SELECT json_array(value) FROM json_each('[{"a":1}]');
SELECT json_object('k', value) FROM json_tree('{"x":[1]}') WHERE key='x';
SELECT json_insert('{}', '$.a', value) FROM json_each('[[1,2]]');
SELECT subtype(value), subtype(atom) FROM json_each('[[1],2]');
SELECT json_group_object(key, value) FROM json_each('{"a":{"b":1},"c":"d"}');
SELECT typeof(value), value FROM json_each('[[1],"s"]');
