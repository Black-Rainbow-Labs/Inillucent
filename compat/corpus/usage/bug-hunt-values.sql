-- Regression cases for wrong values fixed by the bug hunt of October 2026, from SQLite's TCL tests.
-- See tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/values/in_nested_parentheses_around_a_subquery
-- IN over a scalar subquery in any number of parentheses is the subquery form, so every row of the subquery is a member.
CREATE TABLE t1(x INT PRIMARY KEY, y INT);
INSERT INTO t1 VALUES(1,100),(2,200),(3,300),(4,400),(5,500),(6,600);
CREATE TABLE t2(a INT);
INSERT INTO t2 VALUES(2),(4),(6);
SELECT * FROM t1 WHERE x IN ((SELECT a FROM t2));
SELECT * FROM t1 WHERE x IN (((SELECT a FROM t2)));
SELECT * FROM t1 WHERE x IN ((((((SELECT a FROM t2))))));
SELECT x FROM t1 WHERE x NOT IN ((SELECT a FROM t2));

-- case: bug-hunt/values/rtrim_compares_after_trimming_both_sides
-- RTRIM drops the trailing spaces of both strings and then compares bytes, so a lone space is below a control character.
SELECT ' ' > char(20) COLLATE rtrim;
SELECT ' ' < char(20) COLLATE rtrim;
SELECT 'a ' = 'a' COLLATE rtrim, 'a  ' < 'a!' COLLATE rtrim, '' = '   ' COLLATE rtrim;
SELECT char(1) || ' ' > char(1) COLLATE rtrim;

-- case: bug-hunt/values/row_query_left_part_keeps_binary_column_collation
-- A column read by the left row query has its own BINARY collation, which wins over a NOCASE column on the right.
CREATE TABLE t0(aa COLLATE NOCASE, bb);
INSERT INTO t0 VALUES('a', 'A');
SELECT (SELECT +bb,1) >= (aa, 1), (aa,1)<=(SELECT +bb,1) FROM t0;
SELECT (SELECT bb,1) >= (aa, 1), (SELECT bb,1) = (aa, 1) FROM t0;
SELECT (SELECT +bb FROM t0)>=aa FROM t0;
SELECT 2 FROM t0 WHERE (SELECT +bb,1) >= (aa,1);
SELECT 3 FROM t0 WHERE (aa,1) <= (SELECT +bb,1);

-- case: bug-hunt/values/substr_with_no_length_and_a_huge_negative_start
-- With no length argument substr starts from the value length limit, so a very negative start gives an empty string.
SELECT substr('abcdefghijklmnop',-1000000000000000);
SELECT substr('abcdefghijklmnop',-1_000_000_000_000_000);
SELECT substr('abcdefghijklmnop',-3), substr('abcdefghijklmnop',-100), substr('abc',0), substr('abc',2);
SELECT hex(substr(x'0102030405',-1000000000000000)), hex(substr(x'0102030405',-2));

-- case: bug-hunt/values/generated_always_column_has_no_declared_type
-- GENERATED right after the column name is the start of the constraint, so the column has no type and no NUMERIC affinity.
CREATE TABLE t2(a, b GENERATED ALWAYS AS (a+1) VIRTUAL, c REAL, d UNIQUE);
INSERT INTO t2(a, c, d) VALUES(555, 555, 555);
INSERT INTO t2(c, d) VALUES(22, 555) ON CONFLICT(d) DO UPDATE SET a = excluded.c;
SELECT a, b, c, d, typeof(b) FROM t2;
CREATE TABLE t3(a, e GENERATED ALWAYS AS (a+1) STORED);
INSERT INTO t3(a) VALUES(555.0);
SELECT e, typeof(e) FROM t3;
SELECT name, type FROM pragma_table_xinfo('t2');

-- case: bug-hunt/values/arrow_label_with_a_json5_escape
-- A bare label that is not plain letters and digits is looked up as a quoted key, and escapes in a quoted key are read before the comparison.
SELECT '{"abc":123}' ->> 'a\x62c';
SELECT '{"abc":123}' ->> 'a\u0062c';
SELECT '{"abc":123}' ->> '$."a\x62c"';
SELECT json_extract('{"abc":123}', '$."a\x62c"');
SELECT '{"a-c":123}' ->> 'a-c', '{"abc":123}' ->> 'a-c';
SELECT '{"abc":123}' ->> '$.a\x62c';

-- case: bug-hunt/values/using_column_of_a_full_join_keeps_nocase
-- The merged USING column of a FULL JOIN takes the collation of its first column, so a comparison against it is NOCASE.
CREATE TABLE t3(x TEXT COLLATE nocase);
CREATE TABLE t4(x TEXT COLLATE nocase);
INSERT INTO t3 VALUES('abc');
INSERT INTO t4 VALUES('ABC');
SELECT lower(x) FROM t3 FULL JOIN t4 USING(x) WHERE x='Abc';
SELECT x='Abc', x>'ABD' FROM t3 FULL JOIN t4 USING(x);
SELECT x FROM t3 FULL JOIN t4 USING(x) WHERE x IN ('Abc');

-- case: bug-hunt/values/using_chain_with_a_later_full_join_keeps_nocase
-- A USING join after a LEFT JOIN compares through the coalesce of the left copies, which reads the first copy's NOCASE collation.
CREATE TABLE x1(x COLLATE nocase);
CREATE TABLE x2(x);
CREATE TABLE x3(x);
CREATE TABLE t4(y);
INSERT INTO x1 VALUES('ABC');
INSERT INTO x3 VALUES('abc');
SELECT lower(x), quote(y) FROM x1 LEFT JOIN x2 USING (x) JOIN x3 USING (x) FULL JOIN t4;

-- case: bug-hunt/values/where_is_not_pushed_into_a_compound_over_nocase_columns
-- A term is not copied into the arms of an INTERSECT whose result columns are NOCASE, because the arms and the term would compare differently.
CREATE TABLE t1(a COLLATE nocase);
CREATE TABLE t2(b COLLATE nocase);
INSERT INTO t1 VALUES('ABC');
INSERT INTO t2 VALUES('abc');
SELECT * FROM (SELECT a FROM t1 INTERSECT SELECT b FROM t2) WHERE a||'' = 'ABC';
SELECT * FROM (SELECT a FROM t1 INTERSECT SELECT b FROM t2) WHERE a||'' = 'abc';
SELECT * FROM (SELECT a FROM t1 UNION SELECT b FROM t2) WHERE a||'' = 'abc';
SELECT * FROM (SELECT a FROM t1 UNION ALL SELECT b FROM t2) WHERE a||'' = 'abc';
SELECT * FROM (SELECT a FROM t1 EXCEPT SELECT 'x') WHERE a||'' = 'ABC';
