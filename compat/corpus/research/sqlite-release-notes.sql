-- Cases taken from SQLite release notes and the SQLancer bug lists. The source of each is on its marker line.
-- Gathered by the bug hunt of October 2026; see tasks/task-2201-bug-hunt-tdd.md.
-- Run every night by nightly::research_corpus against the pinned SQLite.

-- case: research/sqlite-release-notes/partial_index_is_not_null_term | source: https://sqlite.org/partialindex.html
CREATE TABLE t0(c0, c1);
CREATE INDEX i0 ON t0(c0) WHERE c0 IS NOT NULL;
INSERT INTO t0 VALUES(1,1),(NULL,2),(0,3);
SELECT c1 FROM t0 WHERE c0 IS NOT NULL ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 > 0 ORDER BY c1;
SELECT c1 FROM t0 WHERE NOT (c0 IS NULL) ORDER BY c1;

-- case: research/sqlite-release-notes/partial_index_not_expr_true | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0);
CREATE INDEX i0 ON t0(1) WHERE c0 NOT NULL;
INSERT INTO t0 VALUES(0),(NULL),(1);
SELECT c0 FROM t0 WHERE c0 IS NOT 1 ORDER BY c0;

-- case: research/sqlite-release-notes/partial_index_is_true | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0, c1);
CREATE INDEX i0 ON t0(c0) WHERE c0 IS TRUE;
INSERT INTO t0 VALUES(1,1),(0,2),(NULL,3),(2,4);
SELECT c1 FROM t0 WHERE c0 IS TRUE ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 IS NOT FALSE ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 ORDER BY c1;

-- case: research/sqlite-release-notes/partial_index_affinity_text | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 TEXT);
CREATE INDEX i0 ON t0(c0) WHERE c0 > 0;
INSERT INTO t0 VALUES('1'),('a'),('-1'),(NULL);
SELECT c0 FROM t0 WHERE c0 > 0 ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 > '0' ORDER BY c0;
SELECT c0 FROM t0 ORDER BY c0;

-- case: research/sqlite-release-notes/partial_index_or_terms | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX ta ON t(b) WHERE a > 5;
INSERT INTO t VALUES(10,1),(3,2),(6,3),(NULL,4);
SELECT b FROM t WHERE a > 5 OR a > 8 ORDER BY b;
SELECT b FROM t WHERE a > 5 AND b > 1 ORDER BY b;
SELECT b FROM t WHERE a > 4 ORDER BY b;

-- case: research/sqlite-release-notes/partial_index_in_list_implication | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(b) WHERE a IN (1,2);
INSERT INTO t VALUES(1,10),(2,20),(3,30),(NULL,40);
SELECT b FROM t WHERE a IN (1,2,3) ORDER BY b;
SELECT b FROM t WHERE a = 1 ORDER BY b;
SELECT b FROM t WHERE a IN (1) ORDER BY b;

-- case: research/sqlite-release-notes/partial_index_expression_collate | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 TEXT);
CREATE INDEX i0 ON t0(c0) WHERE c0 = 'a' COLLATE NOCASE;
INSERT INTO t0 VALUES('A'),('a'),('b');
SELECT c0 FROM t0 WHERE c0 = 'a' ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 = 'a' COLLATE NOCASE ORDER BY c0;

-- case: research/sqlite-release-notes/partial_index_unique_violations | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t(a, b);
CREATE UNIQUE INDEX u ON t(a) WHERE b > 0;
INSERT INTO t VALUES(1,1);
INSERT INTO t VALUES(1,0);
INSERT INTO t VALUES(1,-5);
INSERT OR IGNORE INTO t VALUES(1,2);
SELECT a,b FROM t ORDER BY b;

-- case: research/sqlite-release-notes/partial_index_update_leaves_range | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(a) WHERE b = 1;
INSERT INTO t VALUES(5,1),(6,1),(7,2);
UPDATE t SET b = 2 WHERE a = 5;
UPDATE t SET b = 1 WHERE a = 7;
SELECT a FROM t WHERE b = 1 ORDER BY a;
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/partial_index_function_in_where_like | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 TEXT, c1);
CREATE INDEX i0 ON t0(c1) WHERE c0 LIKE 'a%';
INSERT INTO t0 VALUES('abc',1),('ABC',2),('b',3);
SELECT c1 FROM t0 WHERE c0 LIKE 'a%' ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 LIKE 'A%' ORDER BY c1;

-- case: research/sqlite-release-notes/partial_index_between_null | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a) WHERE a NOT NULL;
INSERT INTO t VALUES(1,1),(NULL,2);
SELECT count(*) FROM t WHERE a = NULL;
SELECT count(*) FROM t WHERE a IS NULL;
SELECT count(*) FROM t WHERE a IS NOT NULL;
SELECT count(*) FROM t WHERE a IS NOT NULL OR b = 2;

-- case: research/sqlite-release-notes/partial_index_left_join_on | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0);
CREATE TABLE t1(c0, c1);
CREATE INDEX i1 ON t1(c0) WHERE c1 > 0;
INSERT INTO t0 VALUES(1),(2);
INSERT INTO t1 VALUES(1,1),(2,0);
SELECT t0.c0, t1.c0 FROM t0 LEFT JOIN t1 ON t0.c0 = t1.c0 WHERE t1.c1 > 0 OR t1.c1 IS NULL ORDER BY t0.c0;
SELECT t0.c0, t1.c0 FROM t0 LEFT JOIN t1 ON t0.c0 = t1.c0 AND t1.c1 > 0 ORDER BY t0.c0;

-- case: research/sqlite-release-notes/partial_index_rowid_term | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a) WHERE rowid > 2;
INSERT INTO t VALUES(1,1),(1,2),(1,3),(1,4);
SELECT b FROM t WHERE a = 1 AND rowid > 2 ORDER BY b;
SELECT b FROM t WHERE a = 1 ORDER BY b;

-- case: research/sqlite-release-notes/partial_index_on_generated | source: https://sqlite.org/gencol.html
CREATE TABLE t(a INTEGER, g INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL);
CREATE INDEX i ON t(a) WHERE g > 4;
INSERT INTO t(a) VALUES(1),(2),(3),(NULL);
SELECT a FROM t WHERE g > 4 ORDER BY a;
SELECT a, g FROM t ORDER BY a;

-- case: research/sqlite-release-notes/partial_index_reindex_count | source: https://sqlite.org/partialindex.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a) WHERE b;
INSERT INTO t VALUES(1,1),(2,0),(3,1),(4,'0'),(5,'x');
DELETE FROM t WHERE a = 3;
INSERT INTO t VALUES(3,2);
SELECT a FROM t WHERE b ORDER BY a;
SELECT count(*) FROM t INDEXED BY i WHERE b;

-- case: research/sqlite-release-notes/like_nocase_index_prefix | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT COLLATE NOCASE, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('abc',1),('ABD',2),('abz',3),('b',4),('AB',5);
SELECT b FROM t WHERE a LIKE 'ab%' ORDER BY b;
SELECT b FROM t WHERE a LIKE 'AB%' ORDER BY b;
SELECT b FROM t WHERE a GLOB 'ab*' ORDER BY b;

-- case: research/sqlite-release-notes/like_integer_affinity_index | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 INTEGER UNIQUE);
INSERT INTO t0 VALUES(1),(10),(11),(2),(-1);
SELECT c0 FROM t0 WHERE c0 LIKE '1%' ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 LIKE '-%' ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 LIKE '1_' ORDER BY c0;

-- case: research/sqlite-release-notes/like_numeric_text_real | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 REAL);
CREATE INDEX i0 ON t0(c0);
INSERT INTO t0 VALUES(1.0),(1e100),(0.5),(-0.0),(100.25);
SELECT c0 FROM t0 WHERE c0 LIKE '1.0' ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 LIKE '1%' ORDER BY c0;
SELECT c0 FROM t0 WHERE c0 LIKE '%e+%' ORDER BY c0;

-- case: research/sqlite-release-notes/like_escape_percent | source: https://sqlite.org/lang_expr.html
SELECT 'a%b' LIKE 'a\%b' ESCAPE '\';
SELECT 'axb' LIKE 'a\%b' ESCAPE '\';
SELECT 'a_b' LIKE 'a\_b' ESCAPE '\';
SELECT 'ab' LIKE 'a\_b' ESCAPE '\';
SELECT 'a\' LIKE 'a\\' ESCAPE '\';
SELECT '%' LIKE '!%' ESCAPE '!';
SELECT 'x' LIKE 'x' ESCAPE '';

-- case: research/sqlite-release-notes/like_null_operand | source: https://sqlite.org/nulls.html
SELECT NULL LIKE 'a', 'a' LIKE NULL, NULL LIKE NULL;
SELECT 'a' LIKE 'a' ESCAPE NULL;
SELECT NULL NOT LIKE 'a';
SELECT 'a' GLOB NULL, NULL GLOB 'a';

-- case: research/sqlite-release-notes/like_case_sensitive_pragma_index | source: https://www.sqlite.org/src/info/a340eef47b0cad5
CREATE TABLE t(a TEXT, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('abc',1),('ABC',2);
PRAGMA case_sensitive_like = ON;
SELECT b FROM t WHERE a LIKE 'ab%' ORDER BY b;
PRAGMA case_sensitive_like = OFF;
SELECT b FROM t WHERE a LIKE 'ab%' ORDER BY b;

-- case: research/sqlite-release-notes/like_prefix_high_chars | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('ab',1),('ab' || char(127),2),('ab' || char(128),3),('ab' || char(1000),4),('ac',5);
SELECT b FROM t WHERE a LIKE 'ab%' ORDER BY b;
SELECT b FROM t WHERE a GLOB 'ab*' ORDER BY b;

-- case: research/sqlite-release-notes/like_unicode_case | source: https://sqlite.org/lang_expr.html
SELECT 'Ä' LIKE 'ä', 'A' LIKE 'a', 'ß' LIKE 'SS', 'İ' LIKE 'i';
SELECT upper('ä'), lower('Ä'), upper('ß');

-- case: research/sqlite-release-notes/like_pattern_only_wildcards | source: https://sqlite.org/lang_expr.html
SELECT '' LIKE '', '' LIKE '%', '' LIKE '_', 'a' LIKE '', '%%' LIKE '%%%%';
SELECT 'abc' LIKE '%%%b%%%', 'abc' LIKE '_%_%_', 'abc' LIKE '____';

-- case: research/sqlite-release-notes/glob_char_class | source: https://sqlite.org/lang_expr.html
SELECT 'a' GLOB '[a-c]', 'd' GLOB '[a-c]', 'd' GLOB '[^a-c]', ']' GLOB '[]]', '-' GLOB '[a-]', 'a' GLOB '[]';
SELECT 'ab' GLOB 'a?', 'a' GLOB 'a?', 'A' GLOB 'a', '*' GLOB '[*]', '?' GLOB '[?]';
SELECT 'a' GLOB '[c-a]', 'b' GLOB '[c-a]';

-- case: research/sqlite-release-notes/like_blob_operand | source: https://sqlite.org/lang_expr.html
SELECT x'616263' LIKE 'abc', 'abc' LIKE x'616263', x'616263' LIKE x'616263';
SELECT x'6162' GLOB 'ab', typeof(x'6162' LIKE 'ab');

-- case: research/sqlite-release-notes/like_index_column_collate_binary_vs_nocase | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b);
CREATE INDEX i ON t(a COLLATE BINARY);
INSERT INTO t VALUES('Apple',1),('apple',2),('APPLE',3),('banana',4);
SELECT b FROM t WHERE a LIKE 'app%' ORDER BY b;
SELECT b FROM t WHERE a LIKE 'APP%' ORDER BY b;
SELECT b FROM t WHERE a LIKE 'a%' COLLATE NOCASE ORDER BY b;

-- case: research/sqlite-release-notes/like_long_pattern_error | source: https://sqlite.org/lang_expr.html
SELECT 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab' LIKE '%a%a%a%a%a%a%a%a%a%a%a%a%a%a%a%a%c';
SELECT 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab' GLOB '*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*c';

-- case: research/sqlite-release-notes/in_affinity_integer_text | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a INTEGER, b TEXT, c);
INSERT INTO t VALUES(1,'1',1),(2,'2','2');
SELECT a FROM t WHERE a IN ('1','2') ORDER BY a;
SELECT b FROM t WHERE b IN (1,2) ORDER BY b;
SELECT c FROM t WHERE c IN (1,'2') ORDER BY rowid;
SELECT 1 IN ('1'), '1' IN (1), 1 IN (SELECT '1');

-- case: research/sqlite-release-notes/in_subquery_affinity_index | source: https://sqlite.org/datatype3.html
CREATE TABLE t1(a INTEGER PRIMARY KEY, b TEXT);
CREATE TABLE t2(x TEXT);
INSERT INTO t1 VALUES(1,'one'),(2,'two'),(3,'three');
INSERT INTO t2 VALUES('1'),('3'),('abc');
SELECT b FROM t1 WHERE a IN (SELECT x FROM t2) ORDER BY a;
SELECT x FROM t2 WHERE x IN (SELECT a FROM t1) ORDER BY x;

-- case: research/sqlite-release-notes/in_empty_list_null | source: https://sqlite.org/nulls.html
SELECT NULL IN (), NULL NOT IN (), 1 IN (), 1 NOT IN ();
SELECT NULL IN (SELECT 1 WHERE 0), NULL NOT IN (SELECT 1 WHERE 0);
SELECT NULL IN (1,2), 1 IN (NULL), 1 IN (1,NULL), 2 IN (1,NULL), 2 NOT IN (1,NULL);

-- case: research/sqlite-release-notes/in_row_value | source: https://sqlite.org/lang_expr.html
SELECT (1,2) IN (SELECT 1,2), (1,2) IN (SELECT 1,3), (1,NULL) IN (SELECT 1,2), (1,NULL) IN (SELECT 2,2);
SELECT (1,2) IN (VALUES(1,2),(3,4)), (1,2) NOT IN (VALUES(3,4)), (NULL,NULL) IN (VALUES(NULL,NULL));

-- case: research/sqlite-release-notes/in_index_collation_mismatch | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a TEXT COLLATE NOCASE, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('abc',1),('ABC',2),('def',3);
SELECT b FROM t WHERE a IN ('ABC','DEF') ORDER BY b;
SELECT b FROM t WHERE a IN ('ABC' COLLATE BINARY,'def') ORDER BY b;
SELECT b FROM t WHERE a COLLATE BINARY IN ('ABC','def') ORDER BY b;

-- case: research/sqlite-release-notes/in_with_rowid_non_integer | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
INSERT INTO t(rowid,a) VALUES(1,'one'),(2,'two'),(3,'three');
SELECT a FROM t WHERE rowid IN (1.0, 2.5, '3') ORDER BY rowid;
SELECT a FROM t WHERE rowid IN ('1', 'x', NULL) ORDER BY rowid;
SELECT a FROM t WHERE rowid = '2';
SELECT a FROM t WHERE rowid = 2.0;
SELECT a FROM t WHERE rowid = 2.5;

-- case: research/sqlite-release-notes/in_large_list_duplicates | source: https://sqlite.org/lang_expr.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(1.0),('1'),(2),(NULL);
SELECT a, typeof(a) FROM t WHERE a IN (1, 1.0, 1, 1) ORDER BY rowid;
SELECT count(*) FROM t WHERE a NOT IN (1,1,1,1);
SELECT count(*) FROM t WHERE a NOT IN (1,2,NULL);

-- case: research/sqlite-release-notes/in_subquery_correlated_null | source: https://sqlite.org/nulls.html
CREATE TABLE t1(a);
CREATE TABLE t2(b, c);
INSERT INTO t1 VALUES(1),(2),(NULL);
INSERT INTO t2 VALUES(1,NULL),(2,5),(NULL,7);
SELECT a, a IN (SELECT c FROM t2 WHERE b = t1.a) FROM t1 ORDER BY a;
SELECT a, a NOT IN (SELECT b FROM t2) FROM t1 ORDER BY a;
SELECT a FROM t1 WHERE a NOT IN (SELECT b FROM t2) ORDER BY a;

-- case: research/sqlite-release-notes/in_table_name_form | source: https://sqlite.org/lang_expr.html
CREATE TABLE s(v);
INSERT INTO s VALUES(1),(2),(NULL);
SELECT 1 IN s, 3 IN s, 3 NOT IN s, NULL IN s;

-- case: research/sqlite-release-notes/in_between_precedence | source: https://sqlite.org/lang_expr.html
SELECT 5 BETWEEN 1 AND 10 IN (1), 1 IN (1) BETWEEN 0 AND 2, NOT 1 IN (2), NOT 1 BETWEEN 2 AND 3;
SELECT 1 + 2 BETWEEN 3 AND 3, -1 BETWEEN -2 AND 0, 2 BETWEEN 3 AND 1, NULL BETWEEN 1 AND 2, 1 BETWEEN NULL AND 2, 3 BETWEEN NULL AND 2;

-- case: research/sqlite-release-notes/in_integer_pk_text_compare | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1,'x'),(2,'y'),(10,'z');
SELECT b FROM t WHERE a IN (' 1', '2 ', '010') ORDER BY a;
SELECT b FROM t WHERE a IN ('1.0', '0x2') ORDER BY a;
SELECT b FROM t WHERE a = ' 1';

-- case: research/sqlite-release-notes/or_to_in_affinity | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b INTEGER);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('1',1),('2',2),('x',3);
SELECT b FROM t WHERE a = 1 OR a = 2 ORDER BY b;
SELECT b FROM t WHERE a = 1 OR a = '2' ORDER BY b;
SELECT b FROM t WHERE a = 1.0 OR a = 'x' ORDER BY b;

-- case: research/sqlite-release-notes/or_to_in_collate | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('A',1),('a',2),('B',3),('b',4);
SELECT b FROM t WHERE a = 'a' COLLATE NOCASE OR a = 'B' ORDER BY b;
SELECT b FROM t WHERE a = 'a' OR a = 'B' COLLATE NOCASE ORDER BY b;
SELECT b FROM t WHERE a COLLATE NOCASE = 'a' OR a = 'B' ORDER BY b;

-- case: research/sqlite-release-notes/or_multi_index | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b, c);
CREATE INDEX ia ON t(a);
CREATE INDEX ib ON t(b);
INSERT INTO t VALUES(1,1,'x'),(1,2,'y'),(2,1,'z'),(3,3,'w');
SELECT c FROM t WHERE a = 1 OR b = 1 ORDER BY c;
SELECT count(*) FROM t WHERE a = 1 OR b = 1 OR a = 1;
SELECT c FROM t WHERE a = 1 OR b = 1 ORDER BY c LIMIT 2 OFFSET 1;

-- case: research/sqlite-release-notes/or_with_rowid_and_index | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX ib ON t(b);
INSERT INTO t VALUES('a',10),('b',20),('c',30);
SELECT a FROM t WHERE rowid = 1 OR b = 30 ORDER BY a;
SELECT a FROM t WHERE rowid = 1 OR b = 10 ORDER BY a;
SELECT a FROM t WHERE rowid < 2 OR rowid > 2 ORDER BY a;

-- case: research/sqlite-release-notes/or_null_semantics | source: https://sqlite.org/nulls.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(1,NULL),(NULL,1),(NULL,NULL),(2,2);
SELECT rowid FROM t WHERE a = 1 OR b = 1 ORDER BY rowid;
SELECT rowid FROM t WHERE NOT (a = 1 OR b = 1) ORDER BY rowid;
SELECT rowid FROM t WHERE a = 1 OR b IS NULL ORDER BY rowid;
SELECT NULL OR 1, NULL OR 0, NULL AND 0, NULL AND 1, NOT NULL;

-- case: research/sqlite-release-notes/or_in_left_join_on | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,10),(2,20);
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y OR b.z = 20 ORDER BY x, y;
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y AND (b.z = 10 OR b.z IS NULL) ORDER BY x;

-- case: research/sqlite-release-notes/or_distinct_index | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX ia ON t(a);
CREATE INDEX ib ON t(b);
INSERT INTO t VALUES(1,1),(1,1),(2,1),(1,2);
SELECT DISTINCT a, b FROM t WHERE a = 1 OR b = 1 ORDER BY a, b;
SELECT DISTINCT b FROM t WHERE a = 1 OR a = 2 ORDER BY b;

-- case: research/sqlite-release-notes/skip_scan_basic | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b, c);
CREATE INDEX i ON t(a, b);
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 200)
INSERT INTO t SELECT n % 2, n, n FROM s;
ANALYZE;
SELECT count(*) FROM t WHERE b = 50;
SELECT a, b FROM t WHERE b > 195 ORDER BY a, b;
SELECT a, count(*) FROM t WHERE b BETWEEN 10 AND 20 GROUP BY a ORDER BY a;

-- case: research/sqlite-release-notes/skip_scan_null_leading | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a, b);
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 100)
INSERT INTO t SELECT CASE WHEN n % 3 = 0 THEN NULL ELSE n % 2 END, n FROM s;
ANALYZE;
SELECT count(*) FROM t WHERE b = 30;
SELECT count(*) FROM t WHERE b = 31;
SELECT count(*) FROM t WHERE b >= 90;

-- case: research/sqlite-release-notes/skip_scan_desc | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a DESC, b DESC);
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 120)
INSERT INTO t SELECT n % 3, n FROM s;
ANALYZE;
SELECT a, b FROM t WHERE b > 115 ORDER BY a DESC, b DESC;
SELECT a, b FROM t WHERE b = 60 ORDER BY a;

-- case: research/sqlite-release-notes/skip_scan_with_text_affinity | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t(a, b TEXT);
CREATE INDEX i ON t(a, b);
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 100)
INSERT INTO t SELECT n % 2, CAST(n AS TEXT) FROM s;
ANALYZE;
SELECT count(*) FROM t WHERE b = 10;
SELECT count(*) FROM t WHERE b > 9 AND b < 12;

-- case: research/sqlite-release-notes/analyze_stat4_skew | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 500)
INSERT INTO t SELECT CASE WHEN n < 450 THEN 1 ELSE n END, n FROM s;
ANALYZE;
SELECT count(*) FROM t WHERE a = 1;
SELECT count(*) FROM t WHERE a = 460;
SELECT count(*) FROM t WHERE a > 1;
SELECT count(*) FROM t WHERE a BETWEEN 0 AND 1000;

-- case: research/sqlite-release-notes/distinct_index_collation | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t(a TEXT COLLATE NOCASE);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('a'),('A'),('b'),('B'),('a');
SELECT DISTINCT a FROM t ORDER BY a;
SELECT DISTINCT a COLLATE BINARY FROM t ORDER BY 1;
SELECT count(DISTINCT a) FROM t;
SELECT count(DISTINCT a COLLATE BINARY) FROM t;

-- case: research/sqlite-release-notes/distinct_nulls_one_group | source: https://sqlite.org/nulls.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(NULL,1),(NULL,1),(NULL,2),(1,1);
SELECT DISTINCT a FROM t ORDER BY a;
SELECT a, count(*) FROM t GROUP BY a ORDER BY a;
SELECT DISTINCT a, b FROM t ORDER BY a, b;
SELECT count(DISTINCT a), count(a), count(*) FROM t;

-- case: research/sqlite-release-notes/distinct_numeric_equivalents | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(1.0),('1'),(x'31'),(1e0),('1.0');
SELECT DISTINCT a FROM t ORDER BY typeof(a), a;
SELECT a, count(*) FROM t GROUP BY a ORDER BY typeof(a), a;

-- case: research/sqlite-release-notes/group_by_constant | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
SELECT count(*), sum(a), total(a), max(a), min(a), avg(a), group_concat(a) FROM t;
SELECT count(*) FROM t GROUP BY 1;
SELECT count(*) FROM t GROUP BY a;
INSERT INTO t VALUES(1,2),(3,4);
SELECT count(*) FROM t GROUP BY 1;
SELECT 5 FROM t GROUP BY 5;

-- case: research/sqlite-release-notes/group_by_alias_vs_column | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(2,10),(3,20);
SELECT b AS a, count(*) FROM t GROUP BY a ORDER BY a;
SELECT -a AS b FROM t ORDER BY b;
SELECT a AS b FROM t ORDER BY t.b, a;

-- case: research/sqlite-release-notes/group_by_having_no_group_by | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT count(*) FROM t HAVING count(*) > 2;
SELECT count(*) FROM t HAVING count(*) > 5;
SELECT 1 HAVING 1;
SELECT 1 HAVING 0;

-- case: research/sqlite-release-notes/group_by_bare_column_minmax | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,'x'),(5,'y'),(3,'z');
SELECT a, b, max(a) FROM t;
SELECT b, min(a) FROM t;
SELECT b, max(a), min(a) FROM t;
SELECT b, max(a) FROM t WHERE a < 0;

-- case: research/sqlite-release-notes/group_by_index_order_desc | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a DESC, b);
INSERT INTO t VALUES(1,1),(1,2),(2,1),(3,5),(NULL,1),(NULL,2);
SELECT a, count(*), sum(b) FROM t GROUP BY a ORDER BY a DESC;
SELECT a, b FROM t GROUP BY a, b ORDER BY a, b DESC;

-- case: research/sqlite-release-notes/order_by_elimination_collate | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('b',1),('B',2),('a',3),('A',4);
SELECT a, b FROM t ORDER BY a;
SELECT a, b FROM t ORDER BY a COLLATE NOCASE, b;
SELECT a, b FROM t ORDER BY a COLLATE NOCASE DESC, b DESC;

-- case: research/sqlite-release-notes/order_by_nulls_first_last | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(2),(NULL),(1),(NULL),(3);
SELECT a FROM t ORDER BY a;
SELECT a FROM t ORDER BY a DESC;
SELECT a FROM t ORDER BY a NULLS LAST;
SELECT a FROM t ORDER BY a DESC NULLS FIRST;
SELECT a FROM t ORDER BY a DESC NULLS LAST;

-- case: research/sqlite-release-notes/order_by_mixed_types | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a);
INSERT INTO t VALUES('b'),(2),(x'00'),(NULL),(1.5),('a'),(x'ff'),(-1),('');
SELECT quote(a) FROM t ORDER BY a;
SELECT quote(a) FROM t ORDER BY a DESC;

-- case: research/sqlite-release-notes/order_by_column_number_out_of_range | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(2,'x'),(1,'y');
SELECT a, b FROM t ORDER BY 2;
SELECT a, b FROM t ORDER BY 1 DESC;
SELECT a, b FROM t ORDER BY 'b';
SELECT a, b FROM t ORDER BY 1+1, a;
SELECT a, b FROM t ORDER BY 3;

-- case: research/sqlite-release-notes/order_by_limit_offset_negative | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
SELECT a FROM t ORDER BY a LIMIT -1;
SELECT a FROM t ORDER BY a LIMIT 2 OFFSET -1;
SELECT a FROM t ORDER BY a LIMIT 1+1 OFFSET 1;
SELECT a FROM t ORDER BY a LIMIT 10 OFFSET 10;
SELECT a FROM t ORDER BY a LIMIT 0;
SELECT a FROM t ORDER BY a LIMIT 1, 2;
SELECT a FROM t ORDER BY a LIMIT '2';

-- case: research/sqlite-release-notes/order_by_limit_desc_with_index_ties | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(1,3),(2,1),(2,2);
SELECT a, b FROM t ORDER BY a DESC LIMIT 3;
SELECT a, b FROM t ORDER BY a DESC, b LIMIT 3;
SELECT a, b FROM t WHERE a = 1 ORDER BY b DESC LIMIT 2;

-- case: research/sqlite-release-notes/min_max_optimisation_null | source: https://www.sqlite.org/src/tktview?name=41866dc373
CREATE TABLE t0(c0 UNIQUE);
INSERT INTO t0 VALUES(NULL),(NULL),(1),(2);
SELECT min(c0), max(c0) FROM t0;
SELECT min(c0) FROM t0 WHERE c0 IS NULL;
SELECT max(c0) FROM t0 WHERE c0 > 5;
SELECT min(c0), count(*) FROM t0;

-- case: research/sqlite-release-notes/min_max_with_text_numbers | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES(10),('9'),(x'01'),(NULL),(9.5);
SELECT quote(min(a)), quote(max(a)) FROM t;
SELECT min(a, 5), max(a, 5) FROM t ORDER BY rowid;
SELECT max(1, NULL, 3), min(1, NULL, 3);

-- case: research/sqlite-release-notes/group_concat_separator_order | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES('x'),(NULL),('y'),('x');
SELECT group_concat(a) FROM t;
SELECT group_concat(a, '|') FROM t;
SELECT group_concat(DISTINCT a) FROM t;
SELECT group_concat(a, NULL) FROM t;
SELECT group_concat(a, '') FROM t WHERE 0;
SELECT group_concat(1.5), group_concat(x'41');

-- case: research/sqlite-release-notes/string_agg_alias_order_by_in_agg | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,'c'),(2,'a'),(3,'b');
SELECT group_concat(b ORDER BY a DESC) FROM t;
SELECT string_agg(b, ',' ORDER BY b) FROM t;
SELECT group_concat(b ORDER BY b) FILTER (WHERE a > 1) FROM t;

-- case: research/sqlite-release-notes/sum_overflow | source: https://sqlite.org/forum/forumpost/23b8688ef4
CREATE TABLE t(a);
INSERT INTO t VALUES(9223372036854775807),(1);
SELECT total(a), avg(a) FROM t;
SELECT sum(a) FROM t;

-- case: research/sqlite-release-notes/sum_infinity | source: https://sqlite.org/forum/forumpost/23b8688ef4
CREATE TABLE t(a);
INSERT INTO t VALUES(1e999),(-1e999);
SELECT sum(a), total(a) FROM t;
SELECT sum(a) FROM t WHERE a > 0;
SELECT total(1e999), sum(1e308 + 1e308);

-- case: research/sqlite-release-notes/sum_text_and_float_mix | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),('2'),('3.5abc'),(NULL),('x');
SELECT sum(a), total(a), avg(a), count(a) FROM t;
SELECT typeof(sum(a)) FROM t WHERE a IN (1,'2');
SELECT typeof(sum(a)) FROM t WHERE a = 1;

-- case: research/sqlite-release-notes/sum_kahan_precision | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1e16),(1.0),(-1e16),(1.0);
SELECT sum(a), total(a), avg(a) FROM t;
SELECT sum(0.1), 0.1+0.1+0.1;
WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 10) SELECT sum(0.1), total(0.1), avg(0.1) FROM s;

-- case: research/sqlite-release-notes/count_distinct_multiple_nulls | source: https://sqlite.org/nulls.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,NULL),(NULL,NULL),(3,4);
SELECT count(*), count(a), count(b), count(a+b), count(DISTINCT a), count(a IS NULL) FROM t;

-- case: research/sqlite-release-notes/left_join_where_pushdown | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,10),(2,20);
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z = 10 ORDER BY x;
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z IS NULL ORDER BY x;
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y AND b.z = 10 ORDER BY x;
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z = 10 OR b.z IS NULL ORDER BY x;

-- case: research/sqlite-release-notes/left_join_constant_false_on | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(1),(2);
SELECT x, y FROM a LEFT JOIN b ON 0 ORDER BY x;
SELECT x, y FROM a LEFT JOIN b ON 1 ORDER BY x, y;
SELECT x, y FROM a LEFT JOIN b ON NULL ORDER BY x;
SELECT x, y FROM a LEFT JOIN b ON a.x = 1 ORDER BY x, y;
SELECT x, y FROM a LEFT JOIN b ON b.y = 1 ORDER BY x, y;

-- case: research/sqlite-release-notes/left_join_where_on_left_table_only | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1),(1);
SELECT x, y FROM a LEFT JOIN b ON a.x = b.y WHERE a.x > 1 ORDER BY x;
SELECT x, y FROM a LEFT JOIN b ON a.x = b.y WHERE a.x = 1 ORDER BY x;
SELECT count(*) FROM a LEFT JOIN b ON a.x = b.y;

-- case: research/sqlite-release-notes/left_join_is_null_antijoin | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,NULL),(2,5);
SELECT x FROM a LEFT JOIN b ON a.x = b.y WHERE b.z IS NULL ORDER BY x;
SELECT x FROM a LEFT JOIN b ON a.x = b.y WHERE b.y IS NULL ORDER BY x;
SELECT x FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.y = a.x) ORDER BY x;

-- case: research/sqlite-release-notes/left_join_chain_flatten | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(x, y);
CREATE TABLE c(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,10),(2,20);
INSERT INTO c VALUES(10,'p');
SELECT a.x, b.y, c.z FROM a LEFT JOIN b ON a.x = b.x LEFT JOIN c ON b.y = c.y ORDER BY a.x;
SELECT a.x, b.y, c.z FROM a LEFT JOIN b ON a.x = b.x LEFT JOIN c ON b.y = c.y WHERE c.z IS NOT NULL ORDER BY a.x;
SELECT a.x, b.y, c.z FROM a LEFT JOIN (b LEFT JOIN c ON b.y = c.y) ON a.x = b.x ORDER BY a.x;
SELECT a.x, b.y, c.z FROM a LEFT JOIN (b JOIN c ON b.y = c.y) ON a.x = b.x ORDER BY a.x;

-- case: research/sqlite-release-notes/left_join_coalesce_where | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(1,5);
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE coalesce(b.z, 0) = 0 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE ifnull(b.z, 5) = 5 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z IS NOT 5 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE (b.z = 5) IS NOT 1 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z IS NULL OR b.z > 100 ORDER BY x;

-- case: research/sqlite-release-notes/left_join_between_null_rejecting | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(1,5);
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE CASE WHEN b.z IS NULL THEN 1 ELSE b.z = 5 END ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z BETWEEN 1 AND 10 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE b.z NOT BETWEEN 1 AND 3 ORDER BY x;
SELECT x, z FROM a LEFT JOIN b ON a.x = b.y WHERE NOT (b.z IS NULL) ORDER BY x;

-- case: research/sqlite-release-notes/left_join_subquery_flatten | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(1);
SELECT x, s.k, s.y FROM a LEFT JOIN (SELECT 'K' AS k, y FROM b) AS s ON a.x = s.y ORDER BY x;
SELECT x, s.k FROM a LEFT JOIN (SELECT 'K' AS k, y FROM b) AS s ON a.x = s.y WHERE s.k IS NULL ORDER BY x;
SELECT x, s.c FROM a LEFT JOIN (SELECT y, y + 1 AS c FROM b) AS s ON a.x = s.y ORDER BY x;
SELECT x, s.m FROM a LEFT JOIN (SELECT max(y) AS m FROM b WHERE 0) AS s ON 1 ORDER BY x;

-- case: research/sqlite-release-notes/left_join_view_with_union | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b1(y);
CREATE TABLE b2(y);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b1 VALUES(1);
INSERT INTO b2 VALUES(1),(2);
CREATE VIEW v AS SELECT y, 'b1' AS src FROM b1 UNION ALL SELECT y, 'b2' FROM b2;
SELECT x, y, src FROM a LEFT JOIN v ON a.x = v.y ORDER BY x, src;
SELECT x, y, src FROM a LEFT JOIN v ON a.x = v.y WHERE src = 'b2' ORDER BY x;
SELECT x, count(src) FROM a LEFT JOIN v ON a.x = v.y GROUP BY x ORDER BY x;

-- case: research/sqlite-release-notes/right_join_basic | source: https://sqlite.org/releaselog/3_39_0.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(2),(NULL);
INSERT INTO b VALUES(2),(3),(NULL);
SELECT x, y FROM a RIGHT JOIN b ON a.x = b.y ORDER BY y, x;
SELECT x, y FROM a FULL JOIN b ON a.x = b.y ORDER BY x, y;
SELECT x, y FROM a FULL OUTER JOIN b ON a.x = b.y WHERE x IS NULL ORDER BY y;
SELECT count(*) FROM a FULL JOIN b ON 1;

-- case: research/sqlite-release-notes/right_join_where_on_left | source: https://sqlite.org/releaselog/3_39_0.html
CREATE TABLE a(x, p);
CREATE TABLE b(y, q);
INSERT INTO a VALUES(1,'a1'),(2,'a2');
INSERT INTO b VALUES(2,'b2'),(3,'b3');
SELECT x, y FROM a RIGHT JOIN b ON a.x = b.y WHERE a.p = 'a2' ORDER BY y;
SELECT x, y FROM a RIGHT JOIN b ON a.x = b.y WHERE a.p IS NULL ORDER BY y;
SELECT x, y FROM a RIGHT JOIN b ON a.x = b.y AND a.p = 'zz' ORDER BY y;

-- case: research/sqlite-release-notes/full_join_using | source: https://sqlite.org/releaselog/3_39_0.html
CREATE TABLE a(k, av);
CREATE TABLE b(k, bv);
INSERT INTO a VALUES(1,'a1'),(2,'a2');
INSERT INTO b VALUES(2,'b2'),(3,'b3');
SELECT k, av, bv FROM a FULL JOIN b USING(k) ORDER BY k;
SELECT * FROM a FULL JOIN b USING(k) ORDER BY 1;
SELECT a.k, b.k FROM a FULL JOIN b USING(k) ORDER BY 1, 2;
SELECT * FROM a RIGHT JOIN b USING(k) ORDER BY 1;

-- case: research/sqlite-release-notes/full_join_three_tables | source: https://sqlite.org/releaselog/3_39_0.html
CREATE TABLE a(x);
CREATE TABLE b(y);
CREATE TABLE c(z);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(2),(3);
INSERT INTO c VALUES(3),(4);
SELECT x, y, z FROM a FULL JOIN b ON x = y FULL JOIN c ON y = z ORDER BY x, y, z;
SELECT x, y, z FROM a LEFT JOIN b ON x = y RIGHT JOIN c ON y = z ORDER BY x, y, z;

-- case: research/sqlite-release-notes/natural_join_columns | source: https://sqlite.org/lang_select.html
CREATE TABLE a(k, v, w);
CREATE TABLE b(k, v, z);
INSERT INTO a VALUES(1,'x','w1'),(2,'y','w2');
INSERT INTO b VALUES(1,'x','z1'),(2,'q','z2');
SELECT * FROM a NATURAL JOIN b ORDER BY k;
SELECT * FROM a JOIN b USING(k) ORDER BY k;
SELECT * FROM a NATURAL LEFT JOIN b ORDER BY k;
SELECT a.*, b.z FROM a JOIN b USING(k, v) ORDER BY k;

-- case: research/sqlite-release-notes/cross_join_forces_order | source: https://sqlite.org/optoverview.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(2);
INSERT INTO b VALUES(10),(20);
SELECT x, y FROM a CROSS JOIN b ORDER BY x, y;
SELECT x, y FROM b CROSS JOIN a WHERE a.x < y ORDER BY x, y;
SELECT x, y FROM a, b WHERE y > 10 * x ORDER BY x, y;

-- case: research/sqlite-release-notes/join_on_with_transitive_constraint | source: https://sqlite.org/forum/forumpost/2568d1f6e6
CREATE TABLE t1(a, b);
CREATE TABLE t2(c, d);
CREATE INDEX t1a ON t1(a);
CREATE INDEX t1b ON t1(b);
CREATE INDEX t2c ON t2(c);
INSERT INTO t1 VALUES(1,5),(2,5),(3,6),(4,7);
INSERT INTO t2 VALUES(1,0),(3,0),(5,0);
SELECT t1.a, t1.b, t2.c FROM t1, t2 WHERE t1.a = t2.c AND t2.c > 0 AND t1.b = 5 ORDER BY 1;
SELECT t1.a, t2.c FROM t1, t2 WHERE t1.a = t2.c AND t1.a > 1 AND t2.c < 5 ORDER BY 1;

-- case: research/sqlite-release-notes/transitive_constraint_collation | source: https://sqlite.org/optoverview.html
CREATE TABLE t1(a TEXT COLLATE NOCASE);
CREATE TABLE t2(b TEXT COLLATE BINARY);
INSERT INTO t1 VALUES('x'),('X');
INSERT INTO t2 VALUES('x'),('X');
SELECT a, b FROM t1, t2 WHERE a = b AND b = 'x' ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE a = b AND a = 'x' ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE a = b ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE b = a ORDER BY a, b;

-- case: research/sqlite-release-notes/transitive_constraint_affinity | source: https://sqlite.org/datatype3.html
CREATE TABLE t1(a INTEGER);
CREATE TABLE t2(b TEXT);
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES('1'),('2'),('1.0');
SELECT a, b FROM t1, t2 WHERE a = b AND b = '1.0' ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE a = b AND a = 1 ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE a = b ORDER BY a, b;
SELECT a, b FROM t1, t2 WHERE a = b AND b = 1 ORDER BY a, b;

-- case: research/sqlite-release-notes/constant_propagation_text_blob | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES('abc', x'616263'),('1', 1),(1, '1');
SELECT rowid FROM t WHERE a = b ORDER BY rowid;
SELECT rowid FROM t WHERE a = b AND a = 1 ORDER BY rowid;
SELECT rowid FROM t WHERE a = b AND b = '1' ORDER BY rowid;

-- case: research/sqlite-release-notes/join_self_with_or_and_index | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(2,1),(2,3);
SELECT x.a, x.b, y.b FROM t x, t y WHERE x.a = y.a AND (x.b = 1 OR y.b = 1) ORDER BY 1,2,3;
SELECT x.b, y.b FROM t x JOIN t y ON x.a = y.a AND x.b < y.b ORDER BY 1,2;

-- case: research/sqlite-release-notes/join_using_with_collation_affinity | source: https://sqlite.org/datatype3.html
CREATE TABLE a(k INTEGER, v);
CREATE TABLE b(k TEXT, w);
INSERT INTO a VALUES(1,'a'),(2,'b');
INSERT INTO b VALUES('1','x'),('02','y');
SELECT * FROM a JOIN b USING(k) ORDER BY 1;
SELECT * FROM a JOIN b ON a.k = b.k ORDER BY 1;
SELECT * FROM a JOIN b ON a.k + 0 = b.k + 0 ORDER BY 1;

-- case: research/sqlite-release-notes/subquery_in_on_clause_correlated | source: https://sqlite.org/lang_select.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,10),(1,20),(2,5);
SELECT x, y, z FROM a LEFT JOIN b ON a.x = b.y AND b.z = (SELECT max(z) FROM b b2 WHERE b2.y = a.x) ORDER BY x;
SELECT x, (SELECT count(*) FROM b WHERE b.y = a.x) FROM a ORDER BY x;

-- case: research/sqlite-release-notes/flatten_subquery_limit_offset | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4),(5);
SELECT a FROM (SELECT a FROM t ORDER BY a LIMIT 3) WHERE a > 1 ORDER BY a;
SELECT a FROM (SELECT a FROM t ORDER BY a DESC LIMIT 3 OFFSET 1) ORDER BY a;
SELECT count(*) FROM (SELECT a FROM t LIMIT 2);
SELECT a FROM (SELECT a FROM t LIMIT 2) LIMIT 1 OFFSET 1;

-- case: research/sqlite-release-notes/flatten_subquery_distinct | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(2,3);
SELECT count(*) FROM (SELECT DISTINCT a FROM t);
SELECT sum(a) FROM (SELECT DISTINCT a FROM t);
SELECT x.a, y.b FROM (SELECT DISTINCT a FROM t) x JOIN t y ON x.a = y.a ORDER BY 1,2;

-- case: research/sqlite-release-notes/flatten_subquery_aggregate_outer_where | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(1,20),(2,30),(3,40);
SELECT * FROM (SELECT a, sum(b) AS s FROM t GROUP BY a) WHERE a > 1 ORDER BY a;
SELECT * FROM (SELECT a, sum(b) AS s FROM t GROUP BY a) WHERE s > 25 ORDER BY a;
SELECT * FROM (SELECT a, sum(b) AS s FROM t GROUP BY a) WHERE a = 1 OR s = 40 ORDER BY a;
SELECT * FROM (SELECT a, max(b) AS m FROM t) WHERE m = 40;

-- case: research/sqlite-release-notes/pushdown_into_window_subquery | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(1,20),(2,30),(2,40);
SELECT * FROM (SELECT a, b, sum(b) OVER (PARTITION BY a) AS s FROM t) WHERE a = 1 ORDER BY b;
SELECT * FROM (SELECT a, b, row_number() OVER (ORDER BY b) AS rn FROM t) WHERE b > 15 ORDER BY b;
SELECT * FROM (SELECT a, b, row_number() OVER (PARTITION BY a ORDER BY b) AS rn FROM t) WHERE rn = 1 ORDER BY a;
SELECT * FROM (SELECT a, b, sum(b) OVER (ORDER BY b) AS s FROM t) WHERE a = 2 ORDER BY b;

-- case: research/sqlite-release-notes/pushdown_into_compound_subquery | source: https://sqlite.org/optoverview.html
CREATE TABLE t1(a, b);
CREATE TABLE t2(a, b);
INSERT INTO t1 VALUES(1,'x'),(2,'y'),(3,'z');
INSERT INTO t2 VALUES(2,'y'),(3,'q'),(4,'r');
SELECT * FROM (SELECT a, b FROM t1 UNION SELECT a, b FROM t2) WHERE a > 2 ORDER BY a, b;
SELECT * FROM (SELECT a, b FROM t1 EXCEPT SELECT a, b FROM t2) WHERE b <> 'x' ORDER BY a;
SELECT * FROM (SELECT a, b FROM t1 INTERSECT SELECT a, b FROM t2) WHERE a = 2;
SELECT * FROM (SELECT a FROM t1 UNION ALL SELECT a FROM t2) WHERE a BETWEEN 2 AND 3 ORDER BY a;

-- case: research/sqlite-release-notes/pushdown_constant_in_subquery_join | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b);
INSERT INTO t VALUES('1',1),('2',2);
SELECT * FROM (SELECT a + 0 AS n, b FROM t) WHERE n = '1';
SELECT * FROM (SELECT a + 0 AS n, b FROM t) WHERE n = 1;
SELECT * FROM (SELECT CAST(a AS INTEGER) AS n FROM t) WHERE n = '2';
SELECT * FROM (SELECT a AS n FROM t) WHERE n = 1;

-- case: research/sqlite-release-notes/flatten_compound_subquery_order_limit | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
SELECT * FROM (SELECT a FROM t WHERE a < 3 UNION ALL SELECT a FROM t WHERE a > 2) ORDER BY a DESC LIMIT 3;
SELECT * FROM (SELECT a FROM t UNION ALL SELECT a * 10 FROM t) WHERE a > 3 ORDER BY a;
SELECT a FROM (SELECT a FROM t ORDER BY a DESC LIMIT 2) UNION ALL SELECT 99 ORDER BY a;

-- case: research/sqlite-release-notes/view_with_compound_order_by | source: https://sqlite.org/optoverview.html
CREATE TABLE t1(a);
CREATE TABLE t2(a);
INSERT INTO t1 VALUES(3),(1);
INSERT INTO t2 VALUES(2),(4);
CREATE VIEW v AS SELECT a FROM t1 UNION ALL SELECT a FROM t2 ORDER BY a LIMIT 3;
SELECT a FROM v;
SELECT a FROM v WHERE a > 1 ORDER BY a;
SELECT count(*) FROM v;

-- case: research/sqlite-release-notes/view_column_aliases_and_star | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,2);
CREATE VIEW v1 AS SELECT a, b, a+b, t.a FROM t;
CREATE VIEW v2(x, y) AS SELECT a, b FROM t;
SELECT * FROM v2;
SELECT name FROM pragma_table_info('v1');
SELECT x FROM v2 WHERE y = 2;

-- case: research/sqlite-release-notes/subquery_scalar_multiple_rows | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(3),(1),(2);
SELECT (SELECT a FROM t);
SELECT (SELECT a FROM t ORDER BY a);
SELECT (SELECT a FROM t WHERE 0);
SELECT (SELECT a, a FROM t LIMIT 1);

-- case: research/sqlite-release-notes/subquery_exists_select_list | source: https://sqlite.org/lang_expr.html
CREATE TABLE t(a);
INSERT INTO t VALUES(NULL);
SELECT EXISTS (SELECT a FROM t), EXISTS (SELECT 1 FROM t WHERE 0), NOT EXISTS (SELECT NULL);
SELECT EXISTS (SELECT 1 FROM t LIMIT 0), EXISTS (SELECT 1 LIMIT 1 OFFSET 1);

-- case: research/sqlite-release-notes/subquery_correlated_in_having | source: https://sqlite.org/lang_select.html
CREATE TABLE t(g, v);
INSERT INTO t VALUES(1,10),(1,20),(2,5),(3,50);
SELECT g, sum(v) FROM t GROUP BY g HAVING sum(v) > (SELECT avg(v) FROM t) ORDER BY g;
SELECT g, (SELECT count(*) FROM t t2 WHERE t2.g = t.g) AS c FROM t GROUP BY g ORDER BY g;
SELECT g, sum((SELECT 1)) FROM t GROUP BY g ORDER BY g;

-- case: research/sqlite-release-notes/cte_materialized_hint | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
WITH c AS MATERIALIZED (SELECT a, a * 2 AS d FROM t) SELECT a, d FROM c WHERE d > 2 ORDER BY a;
WITH c AS NOT MATERIALIZED (SELECT a, a * 2 AS d FROM t) SELECT a, d FROM c WHERE d > 2 ORDER BY a;
WITH c(x) AS (SELECT a FROM t) SELECT c1.x, c2.x FROM c c1, c c2 WHERE c1.x < c2.x ORDER BY 1,2;

-- case: research/sqlite-release-notes/cte_recursive_union_vs_all | source: https://sqlite.org/lang_select.html
WITH RECURSIVE c(x) AS (SELECT 1 UNION SELECT (x % 3) + 1 FROM c) SELECT x FROM c ORDER BY x;
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c ORDER BY 1 DESC LIMIT 5) SELECT x FROM c;
WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM c WHERE x < 5) SELECT sum(x), count(*) FROM c;

-- case: research/sqlite-release-notes/cte_recursive_multiple_recursive_selects | source: https://sqlite.org/lang_select.html
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x * 2 FROM c WHERE x < 20 UNION ALL SELECT x * 3 FROM c WHERE x < 20) SELECT x FROM c ORDER BY x LIMIT 12;
WITH RECURSIVE c(a, b) AS (SELECT 0, 1 UNION ALL SELECT b, a + b FROM c WHERE a < 100) SELECT a FROM c ORDER BY a;

-- case: research/sqlite-release-notes/cte_recursive_outer_join_ref | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3) SELECT t.a, c.x FROM t LEFT JOIN c ON t.a = c.x ORDER BY a;
WITH c(x) AS (SELECT 1) SELECT * FROM c, c AS d;

-- case: research/sqlite-release-notes/values_clause_compound | source: https://sqlite.org/lang_select.html
SELECT * FROM (VALUES(1,'a'),(2,'b'),(1,'c')) ORDER BY 1 DESC, 2;
VALUES(1),(2) UNION ALL VALUES(3) ORDER BY 1 DESC;
VALUES(1),(NULL),('x') EXCEPT VALUES(NULL);
SELECT column1 + column2 FROM (VALUES(1,2),(3,4));

-- case: research/sqlite-release-notes/compound_select_column_count_and_collation | source: https://sqlite.org/lang_select.html
CREATE TABLE a(x TEXT COLLATE NOCASE);
CREATE TABLE b(y TEXT);
INSERT INTO a VALUES('A'),('a');
INSERT INTO b VALUES('a'),('A'),('B');
SELECT x FROM a UNION SELECT y FROM b ORDER BY 1;
SELECT y FROM b UNION SELECT x FROM a ORDER BY 1;
SELECT x FROM a INTERSECT SELECT y FROM b ORDER BY 1;
SELECT y FROM b EXCEPT SELECT x FROM a ORDER BY 1;

-- case: research/sqlite-release-notes/compound_order_by_expression | source: https://sqlite.org/lang_select.html
CREATE TABLE a(x, y);
CREATE TABLE b(x, y);
INSERT INTO a VALUES(1,'p'),(3,'q');
INSERT INTO b VALUES(2,'r'),(3,'q');
SELECT x AS k, y FROM a UNION SELECT x, y FROM b ORDER BY k DESC;
SELECT x, y FROM a UNION ALL SELECT x, y FROM b ORDER BY y, 1;
SELECT x FROM a UNION SELECT x FROM b ORDER BY x LIMIT 2 OFFSET 1;

-- case: research/sqlite-release-notes/compound_limit_with_offset_expr | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT a FROM t UNION ALL SELECT a + 10 FROM t LIMIT 4;
SELECT a FROM t UNION ALL SELECT a + 10 FROM t ORDER BY a DESC LIMIT 2 OFFSET 1;
SELECT a FROM t INTERSECT SELECT a FROM t WHERE a > 1 ORDER BY a LIMIT 1;

-- case: research/sqlite-release-notes/compound_mixed_ops_left_to_right | source: https://sqlite.org/lang_select.html
SELECT 1 UNION SELECT 2 EXCEPT SELECT 1 UNION ALL SELECT 3 ORDER BY 1;
SELECT 1 UNION ALL SELECT 1 INTERSECT SELECT 1;
SELECT 1 EXCEPT SELECT 1 UNION SELECT 2;

-- case: research/sqlite-release-notes/is_not_distinct_from_basic | source: https://sqlite.org/lang_expr.html
SELECT NULL IS NULL, NULL IS NOT NULL, 1 IS NULL, NULL IS 1, 1 IS 1, 1 IS 2;
SELECT NULL IS DISTINCT FROM NULL, NULL IS NOT DISTINCT FROM NULL, 1 IS DISTINCT FROM NULL, 1 IS NOT DISTINCT FROM 1;
SELECT 1 IS NOT 2, 1 IS NOT NULL, NULL IS NOT NULL, NULL IS NOT 1;

-- case: research/sqlite-release-notes/is_with_affinity | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a INTEGER, b TEXT);
INSERT INTO t VALUES(1,'1'),(NULL,NULL),(2,'x');
SELECT a, b FROM t WHERE a IS '1' ORDER BY rowid;
SELECT a, b FROM t WHERE b IS 1 ORDER BY rowid;
SELECT a, b FROM t WHERE a IS b ORDER BY rowid;
SELECT a, b FROM t WHERE a IS NOT b ORDER BY rowid;

-- case: research/sqlite-release-notes/is_with_index_and_collation | source: https://www.manuelrigger.at/dbms-bugs/
CREATE TABLE t0(c0 TEXT COLLATE NOCASE, c1);
CREATE INDEX i0 ON t0(c0);
INSERT INTO t0 VALUES('a',1),('A',2),(NULL,3),('b',4);
SELECT c1 FROM t0 WHERE c0 IS 'A' ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 IS NOT 'a' ORDER BY c1;
SELECT c1 FROM t0 WHERE c0 IS NULL ORDER BY c1;
SELECT c1 FROM t0 WHERE NOT (c0 IS 'a') ORDER BY c1;

-- case: research/sqlite-release-notes/is_true_false_forms | source: https://sqlite.org/lang_expr.html
SELECT 1 IS TRUE, 0 IS FALSE, NULL IS TRUE, NULL IS FALSE, NULL IS NOT TRUE, NULL IS NOT FALSE;
SELECT 'a' IS TRUE, '1' IS TRUE, '0.0' IS FALSE, 0.5 IS TRUE, -1 IS TRUE, x'31' IS TRUE;
SELECT NOT NULL IS TRUE, NOT (NULL IS TRUE), (NULL) IS NOT NULL;
SELECT TRUE, FALSE, TRUE AND FALSE, TRUE + TRUE, typeof(TRUE);

-- case: research/sqlite-release-notes/is_expression_precedence | source: https://sqlite.org/lang_expr.html
SELECT 1 IS 1 IS 1, 2 IS 2 IS 1, NULL IS NULL IS NULL, 1 = 1 IS 1, 1 IS NOT 1 IS 0;
SELECT 1 + 1 IS 2, 1 IS 1 + 0, 1 < 2 IS 1, NOT 1 IS 1;
SELECT 1 == 1, 1 <> 2, 1 != 1;

-- case: research/sqlite-release-notes/not_precedence_nulls | source: https://sqlite.org/lang_expr.html
SELECT NOT 1 = 2, NOT 1 > 0, NOT NULL = NULL, NOT 'a' LIKE 'a', NOT 3 BETWEEN 1 AND 2;
SELECT -1 IS NOT NULL, - NULL IS NULL, ~0, ~NULL, +'a', -'a', -'1', ~'1';

-- case: research/sqlite-release-notes/is_not_null_on_not_null_column | source: https://sqlite.org/forum/forumpost/440f2a2f17
CREATE TABLE t(a NOT NULL, b);
INSERT INTO t VALUES(1,NULL),(2,3);
CREATE TABLE u(x);
INSERT INTO u VALUES(1),(5);
SELECT a, b, x FROM u LEFT JOIN t ON t.a = u.x ORDER BY x;
SELECT x FROM u LEFT JOIN t ON t.a = u.x WHERE t.a IS NULL ORDER BY x;
SELECT x FROM u LEFT JOIN t ON t.a = u.x WHERE t.a IS NOT NULL ORDER BY x;
SELECT x, a IS NULL FROM u LEFT JOIN t ON t.a = u.x ORDER BY x;
SELECT x FROM u LEFT JOIN t ON t.a = u.x WHERE t.b IS NULL ORDER BY x;
SELECT t.a, a ISNULL, a NOTNULL, a NOT NULL FROM t;

-- case: research/sqlite-release-notes/notnull_constraint_with_default_and_conflict | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 7, b);
INSERT INTO t(a,b) VALUES(NULL,1);
INSERT INTO t(b) VALUES(2);
INSERT INTO t VALUES(3, NULL);
INSERT OR IGNORE INTO t VALUES(NULL, 4);
SELECT a, b FROM t ORDER BY b;

-- case: research/sqlite-release-notes/isnull_in_check_constraint | source: https://sqlite.org/nulls.html
CREATE TABLE t(a CHECK (a > 0), b CHECK (b IN (1,2)));
INSERT INTO t VALUES(NULL, NULL);
INSERT INTO t VALUES(1, 2);
SELECT a, b FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/row_value_comparison | source: https://sqlite.org/lang_expr.html
SELECT (1,2) < (1,3), (1,2) < (1,NULL), (1,2) = (1,NULL), (2,2) < (1,NULL), (NULL,1) < (NULL,2);
SELECT (1,2) IS (1,2), (1,NULL) IS (1,NULL), (1,NULL) IS NOT (1,2), (1,2) <> (1,3);
SELECT (1,2,3) >= (1,2,3), (1,2,3) > (1,2,3), (1,2,3) <= (1,2,4);

-- case: research/sqlite-release-notes/row_value_with_affinity | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a INTEGER, b TEXT);
INSERT INTO t VALUES(1,'x'),(2,'y'),(10,'a');
SELECT a, b FROM t WHERE (a, b) < ('2', 'z') ORDER BY a;
SELECT a, b FROM t WHERE (a, b) = ('1', 'x');
SELECT a, b FROM t WHERE (a, b) > (1, 'x') ORDER BY a;

-- case: research/sqlite-release-notes/row_value_index_range | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b, c);
CREATE INDEX i ON t(a, b);
INSERT INTO t VALUES(1,1,'p'),(1,2,'q'),(1,3,'r'),(2,1,'s'),(2,2,'t'),(NULL,1,'u'),(3,NULL,'v');
SELECT c FROM t WHERE (a, b) > (1, 2) ORDER BY a, b;
SELECT c FROM t WHERE (a, b) >= (1, 2) AND (a, b) < (2, 2) ORDER BY a, b;
SELECT c FROM t WHERE (a, b) < (3, 0) ORDER BY a, b;
SELECT c FROM t WHERE (a, b) > (NULL, 0) ORDER BY a, b;

-- case: research/sqlite-release-notes/row_value_index_desc_mixed | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b, c);
CREATE INDEX i ON t(a DESC, b ASC);
INSERT INTO t VALUES(1,1,'p'),(1,2,'q'),(2,1,'r'),(2,2,'s'),(3,1,'t');
SELECT c FROM t WHERE (a, b) > (2, 1) ORDER BY a DESC, b;
SELECT c FROM t WHERE (a, b) <= (2, 1) ORDER BY a DESC, b;

-- case: research/sqlite-release-notes/row_value_update_set | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a, b, c);
INSERT INTO t VALUES(1,2,3),(4,5,6);
UPDATE t SET (a, b) = (b, a) WHERE c = 3;
UPDATE t SET (b, c) = (SELECT 10, 20) WHERE a = 4;
SELECT a, b, c FROM t ORDER BY c;

-- case: research/sqlite-release-notes/row_value_in_subquery_size_mismatch | source: https://sqlite.org/lang_expr.html
SELECT (1,2) = (1,2,3);

-- case: research/sqlite-release-notes/row_value_is_null_misuse | source: https://sqlite.org/lang_expr.html
SELECT (1,2) IS NULL;

-- case: research/sqlite-release-notes/row_value_scalar_subquery_multi_col | source: https://sqlite.org/lang_expr.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,2);
SELECT (1,2) = (SELECT a, b FROM t), (1,3) = (SELECT a, b FROM t), (SELECT a, b FROM t) < (2, 0);

-- case: research/sqlite-release-notes/window_frame_rows_range_groups | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(2,10),(3,20),(4,20),(5,30);
SELECT a, sum(a) OVER (ORDER BY b ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY b RANGE BETWEEN 10 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY b GROUPS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY b) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_exclude_clause | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(2,10),(3,20),(4,20),(5,30);
SELECT a, sum(a) OVER (ORDER BY b ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING EXCLUDE CURRENT ROW) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY b ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING EXCLUDE GROUP) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY b ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING EXCLUDE TIES) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY b GROUPS BETWEEN CURRENT ROW AND CURRENT ROW EXCLUDE GROUP) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_empty_frame | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN 2 PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a ROWS BETWEEN 1 FOLLOWING AND 1 FOLLOWING) FROM t ORDER BY a;
SELECT a, max(a) OVER (ORDER BY a ROWS BETWEEN 5 FOLLOWING AND 6 FOLLOWING) FROM t ORDER BY a;
SELECT a, total(a) OVER (ORDER BY a ROWS BETWEEN 5 FOLLOWING AND 6 FOLLOWING) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_range_with_nulls_and_desc | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(NULL),(4),(NULL),(7);
SELECT a, sum(a) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING), count(*) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM t ORDER BY a;
SELECT a, count(*) OVER (ORDER BY a DESC RANGE BETWEEN 2 PRECEDING AND CURRENT ROW) FROM t ORDER BY a DESC;
SELECT a, count(*) OVER (ORDER BY a RANGE BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_ranking_functions | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(1),(2),(3),(3),(3),(4);
SELECT a, rank() OVER w, dense_rank() OVER w, row_number() OVER w, percent_rank() OVER w, cume_dist() OVER w, ntile(3) OVER w FROM t WINDOW w AS (ORDER BY a) ORDER BY a, row_number() OVER w;
SELECT a, ntile(4) OVER (ORDER BY a), ntile(10) OVER (ORDER BY a) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_lead_lag_defaults | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,10),(2,NULL),(3,30);
SELECT a, lag(b) OVER (ORDER BY a), lag(b, 2, -1) OVER (ORDER BY a), lead(b) OVER (ORDER BY a), lead(b, 0) OVER (ORDER BY a), lead(b, 5, 'd') OVER (ORDER BY a) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_first_last_nth_value | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,'x'),(2,'y'),(3,'z'),(4,'w');
SELECT a, first_value(b) OVER w, last_value(b) OVER w, nth_value(b, 2) OVER w FROM t WINDOW w AS (ORDER BY a) ORDER BY a;
SELECT a, last_value(b) OVER (ORDER BY a ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING), nth_value(b, 5) OVER (ORDER BY a ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_nth_value_bad_arg | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
SELECT nth_value(a, 0) OVER () FROM t;

-- case: research/sqlite-release-notes/window_filter_clause | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1),(2,0),(3,1),(4,0);
SELECT a, sum(a) FILTER (WHERE b) OVER (ORDER BY a), count(*) FILTER (WHERE b = 0) OVER () FROM t ORDER BY a;
SELECT sum(a) FILTER (WHERE b = 1), count(*) FILTER (WHERE 0), max(a) FILTER (WHERE b = 0) FROM t;

-- case: research/sqlite-release-notes/window_partition_by_null | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(g, v);
INSERT INTO t VALUES(NULL,1),(NULL,2),(1,3),(1,4),(NULL,5);
SELECT g, v, sum(v) OVER (PARTITION BY g ORDER BY v), row_number() OVER (PARTITION BY g ORDER BY v) FROM t ORDER BY g, v;

-- case: research/sqlite-release-notes/window_with_group_by | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(g, v);
INSERT INTO t VALUES(1,1),(1,2),(2,3),(3,4),(3,5);
SELECT g, sum(v), sum(sum(v)) OVER (ORDER BY g), rank() OVER (ORDER BY sum(v) DESC) FROM t GROUP BY g ORDER BY g;
SELECT g, count(*) OVER () FROM t GROUP BY g ORDER BY g;

-- case: research/sqlite-release-notes/window_in_where_error | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE row_number() OVER () = 1;

-- case: research/sqlite-release-notes/window_order_by_expression_peers | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a TEXT COLLATE NOCASE);
INSERT INTO t VALUES('a'),('A'),('b'),('B');
SELECT a, rank() OVER (ORDER BY a), count(*) OVER (ORDER BY a RANGE CURRENT ROW) FROM t ORDER BY a, rowid;
SELECT a, rank() OVER (ORDER BY a COLLATE BINARY) FROM t ORDER BY a COLLATE BINARY;

-- case: research/sqlite-release-notes/window_chained_base_window | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(g, v);
INSERT INTO t VALUES(1,1),(1,2),(2,3),(2,4);
SELECT g, v, sum(v) OVER (w ORDER BY v), sum(v) OVER (w) FROM t WINDOW w AS (PARTITION BY g) ORDER BY g, v;
SELECT g, v, sum(v) OVER (x ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t WINDOW x AS (PARTITION BY g ORDER BY v) ORDER BY g, v;

-- case: research/sqlite-release-notes/window_group_concat_and_avg_inverse | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,5),(2,NULL),(3,2),(4,9),(5,1);
SELECT a, avg(b) OVER (ORDER BY a ROWS 1 PRECEDING), min(b) OVER (ORDER BY a ROWS 1 PRECEDING), max(b) OVER (ORDER BY a ROWS 1 PRECEDING), group_concat(b) OVER (ORDER BY a ROWS 1 PRECEDING), total(b) OVER (ORDER BY a ROWS 1 PRECEDING) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_sum_float_inverse_precision | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1e16),(2,1.0),(3,1.0),(4,1.0),(5,1.0);
SELECT a, sum(b) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;
SELECT a, total(b) OVER (ORDER BY a ROWS BETWEEN CURRENT ROW AND 1 FOLLOWING) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_integer_sum_overflow | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(9223372036854775807),(1),(-5);
SELECT a, total(a) OVER (ORDER BY rowid ROWS BETWEEN CURRENT ROW AND 1 FOLLOWING) FROM t;
SELECT a, sum(a) OVER (ORDER BY rowid ROWS BETWEEN CURRENT ROW AND 1 FOLLOWING) FROM t;

-- case: research/sqlite-release-notes/window_limit_offset_order_by_distinct | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(3),(1),(2),(1);
SELECT DISTINCT sum(a) OVER (ORDER BY a) FROM t ORDER BY 1;
SELECT a, row_number() OVER (ORDER BY a) FROM t ORDER BY a DESC LIMIT 2;
SELECT a, row_number() OVER (ORDER BY a DESC) rn FROM t ORDER BY rn LIMIT 2 OFFSET 1;

-- case: research/sqlite-release-notes/window_frame_expression_bounds | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND '1' FOLLOWING) FROM t ORDER BY a;
SELECT a, sum(a) OVER (ORDER BY a ROWS BETWEEN -1 PRECEDING AND CURRENT ROW) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/window_range_text_order_key_error | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(a);
INSERT INTO t VALUES('x'),('y');
SELECT a, sum(1) OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t;

-- case: research/sqlite-release-notes/window_subquery_correlated | source: https://sqlite.org/windowfunctions.html
CREATE TABLE t(g, v);
INSERT INTO t VALUES(1,10),(1,20),(2,30);
SELECT g, (SELECT max(r) FROM (SELECT row_number() OVER (ORDER BY v) AS r FROM t t2 WHERE t2.g = t.g)) FROM t GROUP BY g ORDER BY g;
SELECT g, v, (SELECT sum(v) OVER () FROM t t2 WHERE t2.g = t.g LIMIT 1) FROM t ORDER BY g, v;

-- case: research/sqlite-release-notes/window_on_view_and_join | source: https://sqlite.org/windowfunctions.html
CREATE TABLE a(x);
CREATE TABLE b(y, z);
INSERT INTO a VALUES(1),(2),(3);
INSERT INTO b VALUES(1,10),(1,20);
SELECT x, z, count(z) OVER (PARTITION BY x), row_number() OVER (ORDER BY x, z) FROM a LEFT JOIN b ON x = y ORDER BY x, z;

-- case: research/sqlite-release-notes/generated_virtual_stored_basic | source: https://sqlite.org/gencol.html
CREATE TABLE t(a INTEGER, b INTEGER GENERATED ALWAYS AS (a * 2) VIRTUAL, c INTEGER AS (a + b) STORED);
INSERT INTO t(a) VALUES(1),(2);
UPDATE t SET a = a + 10 WHERE a = 1;
SELECT a, b, c FROM t ORDER BY a;

-- case: research/sqlite-release-notes/generated_column_index_expression | source: https://sqlite.org/gencol.html
CREATE TABLE t(a INTEGER, g INTEGER AS (a % 3));
CREATE INDEX i ON t(g);
INSERT INTO t(a) VALUES(1),(2),(3),(4),(5),(6);
SELECT a FROM t WHERE g = 0 ORDER BY a;
SELECT g, count(*) FROM t GROUP BY g ORDER BY g;
UPDATE t SET a = a + 1;
SELECT a FROM t WHERE g = 0 ORDER BY a;
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/generated_column_affinity | source: https://sqlite.org/gencol.html
CREATE TABLE t(a, i INTEGER AS (a), tx TEXT AS (a), r REAL AS (a), n NUMERIC AS (a), b BLOB AS (a));
INSERT INTO t(a) VALUES('5'),(5),(5.5),('abc'),(NULL);
SELECT typeof(a), typeof(i), typeof(tx), typeof(r), typeof(n), typeof(b), i, tx, r FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/generated_column_not_null_check | source: https://www.sqlite.org/src/tktview?name=91e8695101
CREATE TABLE t(a, g INTEGER AS (a + 1) NOT NULL CHECK (g < 10));
INSERT INTO t(a) VALUES(1);
INSERT INTO t(a) VALUES(NULL);
INSERT INTO t(a) VALUES(20);
SELECT a, g FROM t;

-- case: research/sqlite-release-notes/generated_column_integrity_check | source: https://www.sqlite.org/src/tktview?name=bd8c280671
CREATE TABLE t(a INTEGER, b INTEGER AS (a + 1) STORED, c INTEGER AS (b * 2) VIRTUAL);
CREATE INDEX i ON t(c);
INSERT INTO t(a) VALUES(1),(2),(NULL);
PRAGMA integrity_check;
SELECT a, b, c FROM t ORDER BY a;

-- case: research/sqlite-release-notes/generated_column_with_collation_and_pk | source: https://sqlite.org/gencol.html
CREATE TABLE t(a TEXT, g TEXT COLLATE NOCASE AS (a));
INSERT INTO t(a) VALUES('B'),('a'),('A'),('b');
SELECT a FROM t WHERE g = 'a' ORDER BY a;
SELECT a FROM t ORDER BY g, a;
SELECT a FROM t WHERE a = 'a' ORDER BY a;

-- case: research/sqlite-release-notes/generated_column_function_depends_on_columns_order | source: https://sqlite.org/gencol.html
CREATE TABLE t(g1 AS (g2 + 1), g2 AS (a * 2), a);
INSERT INTO t(a) VALUES(3);
SELECT g1, g2, a FROM t;
SELECT * FROM t;

-- case: research/sqlite-release-notes/generated_column_alter_add | source: https://sqlite.org/gencol.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
ALTER TABLE t ADD COLUMN v INTEGER AS (a * 10) VIRTUAL;
SELECT a, v FROM t ORDER BY a;
ALTER TABLE t ADD COLUMN s INTEGER AS (a * 100) STORED;

-- case: research/sqlite-release-notes/generated_column_insert_value_error | source: https://sqlite.org/gencol.html
CREATE TABLE t(a, g AS (a + 1));
INSERT INTO t(a, g) VALUES(1, 5);

-- case: research/sqlite-release-notes/generated_column_rowid_ref | source: https://sqlite.org/gencol.html
CREATE TABLE t(a, g AS (rowid + a));
INSERT INTO t(rowid, a) VALUES(10, 1),(20, 2);
SELECT rowid, a, g FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/generated_column_deterministic_func | source: https://sqlite.org/gencol.html
CREATE TABLE t(d TEXT, y INTEGER AS (CAST(strftime('%Y', d) AS INTEGER)), s TEXT AS (substr(d, 1, 4)));
INSERT INTO t(d) VALUES('2024-02-29'),('1999-12-31'),('bad'),(NULL);
SELECT d, y, s FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/upsert_do_update_excluded | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c);
INSERT INTO t VALUES(1,10,'x'),(2,20,'y');
INSERT INTO t VALUES(1,100,'z'),(2,200,'w'),(3,300,'v') ON CONFLICT(a) DO UPDATE SET b = excluded.b + t.b, c = c || excluded.c WHERE excluded.b > 150;
SELECT a, b, c FROM t ORDER BY a;

-- case: research/sqlite-release-notes/upsert_update_trigger_old_values_clean | source: https://sqlite.org/forum/forumpost/284955a3cd454a15
CREATE TABLE t(a INTEGER PRIMARY KEY, b UNIQUE, c);
CREATE INDEX tc ON t(c);
CREATE TABLE log(o_a, o_b, o_c, n_a, n_b, n_c);
CREATE TRIGGER tr AFTER UPDATE ON t BEGIN INSERT INTO log VALUES(old.a, old.b, old.c, new.a, new.b, new.c); END;
INSERT INTO t VALUES(1,10,'x'),(2,20,'y');
INSERT INTO t VALUES(1,11,'p') ON CONFLICT(a) DO UPDATE SET b = excluded.b, c = excluded.c;
INSERT INTO t VALUES(3,20,'q') ON CONFLICT(b) DO UPDATE SET c = 'upd', a = a + 100;
SELECT * FROM log ORDER BY n_a;
SELECT a, b, c FROM t ORDER BY a;

-- case: research/sqlite-release-notes/upsert_index_out_of_sync | source: https://sqlite.org/forum/forumpost/919c6579c8
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c, d);
CREATE UNIQUE INDEX tb ON t(b);
CREATE INDEX tc ON t(c);
CREATE INDEX td ON t(d, c);
INSERT INTO t VALUES(1,1,1,1),(2,2,2,2);
INSERT INTO t VALUES(3,1,5,5) ON CONFLICT(b) DO UPDATE SET c = excluded.c, d = d + 1;
INSERT INTO t VALUES(2,9,9,9) ON CONFLICT DO UPDATE SET c = 7;
SELECT a, b, c, d FROM t ORDER BY a;
SELECT a FROM t WHERE c = 5;
SELECT a FROM t WHERE c = 7;
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/upsert_multiple_clauses | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a UNIQUE, b UNIQUE, c);
INSERT INTO t VALUES(1,1,'x'),(2,2,'y');
INSERT INTO t VALUES(1,9,'p') ON CONFLICT(a) DO UPDATE SET c = 'a-hit' ON CONFLICT(b) DO UPDATE SET c = 'b-hit';
INSERT INTO t VALUES(8,2,'q') ON CONFLICT(a) DO UPDATE SET c = 'a-hit' ON CONFLICT(b) DO UPDATE SET c = 'b-hit';
INSERT INTO t VALUES(2,2,'r') ON CONFLICT(a) DO NOTHING ON CONFLICT DO UPDATE SET c = 'any';
SELECT a, b, c FROM t ORDER BY a;

-- case: research/sqlite-release-notes/upsert_do_nothing_returning | source: https://sqlite.org/lang_returning.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(1,'x');
INSERT INTO t VALUES(1,'y'),(2,'z') ON CONFLICT DO NOTHING RETURNING a, b;
INSERT INTO t VALUES(1,'q') ON CONFLICT DO UPDATE SET b = 'u' RETURNING *;
INSERT INTO t VALUES(1,'q') ON CONFLICT DO UPDATE SET b = 'v' WHERE 0 RETURNING *;
SELECT a, b FROM t ORDER BY a;

-- case: research/sqlite-release-notes/upsert_select_source_needs_where | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a PRIMARY KEY, b);
CREATE TABLE s(a, b);
INSERT INTO t VALUES(1,0);
INSERT INTO s VALUES(1,5),(2,6),(1,7);
INSERT INTO t SELECT a, b FROM s WHERE true ON CONFLICT(a) DO UPDATE SET b = b + excluded.b;
SELECT a, b FROM t ORDER BY a;

-- case: research/sqlite-release-notes/upsert_on_rowid_alias_text_key | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(5,'x');
INSERT INTO t VALUES('5','y') ON CONFLICT(a) DO UPDATE SET b = 'hit';
INSERT INTO t VALUES(5.0,'z') ON CONFLICT(a) DO UPDATE SET b = b || 'z';
INSERT INTO t VALUES('five','w');
SELECT a, b FROM t ORDER BY a;

-- case: research/sqlite-release-notes/replace_conflict_delete_triggers | source: https://sqlite.org/quirks.html
CREATE TABLE t(a UNIQUE, b);
CREATE TABLE log(m);
CREATE TRIGGER d AFTER DELETE ON t BEGIN INSERT INTO log VALUES('del ' || old.a); END;
INSERT INTO t VALUES(1,'x'),(2,'y');
INSERT OR REPLACE INTO t VALUES(1,'z');
REPLACE INTO t VALUES(2,'w');
SELECT a, b FROM t ORDER BY a;
SELECT m FROM log;

-- case: research/sqlite-release-notes/replace_with_two_unique_conflicts | source: https://www.sqlite.org/src/info/3be1295b264be2fa
CREATE TABLE t(a UNIQUE, b UNIQUE, c);
INSERT INTO t VALUES(1,10,'x'),(2,20,'y'),(3,30,'z');
INSERT OR REPLACE INTO t VALUES(1,20,'new');
SELECT a, b, c FROM t ORDER BY a;
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/multi_row_insert_unique_violation | source: https://www.sqlite.org/src/info/3be1295b264be2fa
CREATE TABLE t0(c0, c1);
CREATE UNIQUE INDEX i0 ON t0(c0, c1);
INSERT INTO t0 VALUES(1,1);
INSERT INTO t0 VALUES(2,2),(1,1),(3,3);
SELECT c0, c1 FROM t0 ORDER BY c0;
INSERT OR IGNORE INTO t0 VALUES(2,2),(1,1),(3,3),(3,3);
SELECT c0, c1 FROM t0 ORDER BY c0;

-- case: research/sqlite-release-notes/unique_nulls_allowed_multiple | source: https://sqlite.org/nulls.html
CREATE TABLE t(a, b, UNIQUE(a, b));
INSERT INTO t VALUES(NULL,NULL),(NULL,NULL),(1,NULL),(1,NULL);
INSERT INTO t VALUES(1,1);
INSERT INTO t VALUES(1,1);
SELECT count(*) FROM t;

-- case: research/sqlite-release-notes/unique_with_collation_and_affinity | source: https://www.sqlite.org/src/tktview/3182d3879020ef3b2e6db56be2470a0266d3c773
CREATE TABLE t0(c0 TEXT PRIMARY KEY COLLATE NOCASE, c1) WITHOUT ROWID;
INSERT INTO t0 VALUES('a',1);
INSERT OR IGNORE INTO t0 VALUES('A',2);
INSERT OR REPLACE INTO t0 VALUES('B',3);
INSERT OR REPLACE INTO t0 VALUES('b',4);
SELECT c0, c1 FROM t0 ORDER BY c0;
SELECT c1 FROM t0 WHERE c0 = 'A';
SELECT c1 FROM t0 WHERE c0 > 'A' ORDER BY c1;
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/without_rowid_nocase_pk_index | source: https://www.sqlite.org/src/tktview/3182d3879020ef3b2e6db56be2470a0266d3c773
CREATE TABLE t0(c0 TEXT COLLATE NOCASE, c1 TEXT, PRIMARY KEY(c0, c1)) WITHOUT ROWID;
CREATE INDEX i0 ON t0(c1);
INSERT INTO t0 VALUES('a','x'),('A','y'),('a','X'),('B','x');
SELECT c0, c1 FROM t0 ORDER BY c0, c1;
SELECT c0, c1 FROM t0 WHERE c1 = 'x' ORDER BY c0;
SELECT count(*) FROM t0 WHERE c0 = 'A';
PRAGMA integrity_check;

-- case: research/sqlite-release-notes/without_rowid_pk_not_null_and_order | source: https://sqlite.org/quirks.html
CREATE TABLE t(a, b, PRIMARY KEY(b DESC, a)) WITHOUT ROWID;
INSERT INTO t VALUES(1,1),(2,1),(1,2),(2,2);
INSERT INTO t VALUES(NULL,3);
SELECT a, b FROM t;
SELECT a, b FROM t ORDER BY b, a;

-- case: research/sqlite-release-notes/primary_key_null_in_rowid_table | source: https://sqlite.org/quirks.html
CREATE TABLE t(a PRIMARY KEY, b);
INSERT INTO t VALUES(NULL,1),(NULL,2);
SELECT a, b FROM t ORDER BY b;
CREATE TABLE u(a INT PRIMARY KEY, b);
INSERT INTO u VALUES(NULL,1),(NULL,2);
SELECT a, b FROM u ORDER BY b;

-- case: research/sqlite-release-notes/integer_primary_key_desc_not_alias | source: https://sqlite.org/quirks.html
CREATE TABLE t1(a INTEGER PRIMARY KEY DESC, b);
CREATE TABLE t2(a INT PRIMARY KEY, b);
CREATE TABLE t3(a INTEGER, b, PRIMARY KEY(a));
CREATE TABLE t4(a INTEGER, b, PRIMARY KEY(a DESC));
INSERT INTO t1(b) VALUES('x'),('y');
INSERT INTO t2(b) VALUES('x'),('y');
INSERT INTO t3(b) VALUES('x'),('y');
INSERT INTO t4(b) VALUES('x'),('y');
SELECT a FROM t1 ORDER BY b;
SELECT a FROM t2 ORDER BY b;
SELECT a FROM t3 ORDER BY b;
SELECT a FROM t4 ORDER BY b;

-- case: research/sqlite-release-notes/autoincrement_sequence | source: https://sqlite.org/quirks.html
CREATE TABLE t(a INTEGER PRIMARY KEY AUTOINCREMENT, b);
INSERT INTO t(b) VALUES('x'),('y'),('z');
DELETE FROM t WHERE a = 3;
INSERT INTO t(b) VALUES('w');
INSERT INTO t(a, b) VALUES(100,'v');
DELETE FROM t;
INSERT INTO t(b) VALUES('u');
SELECT a, b FROM t;
SELECT name, seq FROM sqlite_sequence;

-- case: research/sqlite-release-notes/rowid_max_value_insert | source: https://sqlite.org/quirks.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t VALUES(9223372036854775807,'max');
INSERT INTO t(b) VALUES('next');
SELECT count(*), max(a), min(b) FROM t;
CREATE TABLE u(a INTEGER PRIMARY KEY AUTOINCREMENT, b);
INSERT INTO u VALUES(9223372036854775807,'max');
INSERT INTO u(b) VALUES('next');

-- case: research/sqlite-release-notes/rowid_aliases_and_oid | source: https://sqlite.org/quirks.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES('x','y');
SELECT rowid, oid, _rowid_ FROM t;
CREATE TABLE u(rowid, b);
INSERT INTO u(rowid, b) VALUES('r', 'y');
SELECT rowid, oid, _rowid_ FROM u;

-- case: research/sqlite-release-notes/returning_old_new_values | source: https://sqlite.org/lang_returning.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b, c DEFAULT 'd');
INSERT INTO t(b) VALUES(1),(2),(3) RETURNING a, b, c, b * 10 AS ten;
UPDATE t SET b = b + 1 WHERE a > 1 RETURNING a, b;
DELETE FROM t WHERE b = 3 RETURNING a, b, rowid;
SELECT a, b FROM t ORDER BY a;

-- case: research/sqlite-release-notes/returning_with_trigger_changes | source: https://sqlite.org/lang_returning.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t SET b = b * 100 WHERE a = new.a; END;
INSERT INTO t(b) VALUES(1) RETURNING a, b;
SELECT a, b FROM t;

-- case: research/sqlite-release-notes/returning_in_cte_not_allowed | source: https://sqlite.org/lang_returning.html
CREATE TABLE t(a);
WITH c AS (INSERT INTO t VALUES(1) RETURNING a) SELECT * FROM c;

-- case: research/sqlite-release-notes/returning_star_generated_and_rowid | source: https://sqlite.org/lang_returning.html
CREATE TABLE t(a, g AS (a * 2));
INSERT INTO t(a) VALUES(4) RETURNING *;
CREATE TABLE w(k PRIMARY KEY, v) WITHOUT ROWID;
INSERT INTO w VALUES('x', 1) RETURNING *;
UPDATE w SET v = v + 1 RETURNING k, v;

-- case: research/sqlite-release-notes/update_from_join | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a, b);
CREATE TABLE s(a, v);
INSERT INTO t VALUES(1,0),(2,0),(3,0);
INSERT INTO s VALUES(1,10),(2,20);
UPDATE t SET b = s.v FROM s WHERE t.a = s.a;
SELECT a, b FROM t ORDER BY a;
UPDATE t SET b = t.b + s.v FROM s WHERE s.a = 1 AND t.a <= 2 RETURNING a, b;

-- case: research/sqlite-release-notes/update_order_of_evaluation_swap | source: https://sqlite.org/lang_upsert.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,2);
UPDATE t SET a = b, b = a;
SELECT a, b FROM t;
UPDATE t SET a = a + 1, b = a;
SELECT a, b FROM t;

-- case: research/sqlite-release-notes/update_unique_swap_in_one_statement | source: https://sqlite.org/quirks.html
CREATE TABLE t(a UNIQUE);
INSERT INTO t VALUES(1),(2),(3);
UPDATE t SET a = a + 1;
SELECT a FROM t ORDER BY a;

-- case: research/sqlite-release-notes/delete_limit_order_by | source: https://sqlite.org/lang_delete.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3),(4);
DELETE FROM t WHERE rowid IN (SELECT rowid FROM t ORDER BY a DESC LIMIT 2);
SELECT a FROM t ORDER BY a;

-- case: research/sqlite-release-notes/insert_default_values_and_select_star | source: https://sqlite.org/lang_insert.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b DEFAULT 5, c DEFAULT (1+2), d DEFAULT 'x', e DEFAULT NULL, f DEFAULT -3, g DEFAULT 1.5e0, h DEFAULT TRUE);
INSERT INTO t DEFAULT VALUES;
SELECT *, typeof(b), typeof(c), typeof(f), typeof(g), typeof(h) FROM t;

-- case: research/sqlite-release-notes/insert_column_count_mismatch | source: https://sqlite.org/lang_insert.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1);

-- case: research/sqlite-release-notes/insert_select_same_table | source: https://sqlite.org/lang_insert.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2);
INSERT INTO t SELECT a + 10 FROM t;
INSERT INTO t SELECT a + 100 FROM t WHERE a < 100;
SELECT a FROM t ORDER BY a;

-- case: research/sqlite-release-notes/trigger_instead_of_view | source: https://sqlite.org/lang_createtrigger.html
CREATE TABLE t(a, b);
CREATE VIEW v AS SELECT a, b FROM t;
CREATE TRIGGER i INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(new.a, new.b * 2); END;
CREATE TRIGGER u INSTEAD OF UPDATE ON v BEGIN UPDATE t SET b = new.b WHERE a = old.a; END;
INSERT INTO v VALUES(1, 5),(2, 6);
UPDATE v SET b = 99 WHERE a = 2;
SELECT a, b FROM t ORDER BY a;

-- case: research/sqlite-release-notes/trigger_when_new_null | source: https://sqlite.org/lang_createtrigger.html
CREATE TABLE t(a);
CREATE TABLE log(m);
CREATE TRIGGER tr AFTER INSERT ON t WHEN new.a > 1 BEGIN INSERT INTO log VALUES(new.a); END;
INSERT INTO t VALUES(NULL),(1),(2),(3);
SELECT m FROM log ORDER BY m;

-- case: research/sqlite-release-notes/trigger_recursive_pragma | source: https://sqlite.org/lang_createtrigger.html
PRAGMA recursive_triggers = ON;
CREATE TABLE t(a);
CREATE TABLE log(m);
CREATE TRIGGER tr AFTER INSERT ON t WHEN new.a < 5 BEGIN INSERT INTO t VALUES(new.a + 1); END;
INSERT INTO t VALUES(1);
SELECT a FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/trigger_before_update_of_column | source: https://sqlite.org/lang_createtrigger.html
CREATE TABLE t(a, b);
CREATE TABLE log(m);
CREATE TRIGGER tr BEFORE UPDATE OF a ON t BEGIN INSERT INTO log VALUES('a'); END;
INSERT INTO t VALUES(1,1);
UPDATE t SET b = 2;
UPDATE t SET a = 1;
UPDATE t SET a = a, b = 3;
SELECT count(*) FROM log;

-- case: research/sqlite-release-notes/strict_type_enforcement | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INTEGER, b TEXT, c REAL, d BLOB, e ANY) STRICT;
INSERT INTO t VALUES('12', 5, '1.5', x'00', 'z');
SELECT a, typeof(a), b, typeof(b), c, typeof(c), typeof(d), typeof(e) FROM t;
INSERT INTO t VALUES('abc', 'x', 1.0, x'', 1);

-- case: research/sqlite-release-notes/strict_real_int_conversion | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INTEGER, c REAL) STRICT;
INSERT INTO t VALUES(1.0, 3);
SELECT a, typeof(a), c, typeof(c) FROM t;
INSERT INTO t VALUES(1.5, 1);

-- case: research/sqlite-release-notes/strict_null_and_pk | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT NOT NULL) STRICT;
INSERT INTO t(b) VALUES('x');
INSERT INTO t(b) VALUES(NULL);
CREATE TABLE u(a TEXT PRIMARY KEY, b) STRICT;
INSERT INTO u VALUES(NULL, 1);
SELECT count(*) FROM u;

-- case: research/sqlite-release-notes/strict_column_type_names | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INT, b INTEGER, c REAL, d TEXT, e BLOB, f ANY) STRICT;
CREATE TABLE u(a VARCHAR(10)) STRICT;

-- case: research/sqlite-release-notes/strict_blob_text_mismatch | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a BLOB, b TEXT) STRICT;
INSERT INTO t VALUES('abc', 'ok');
INSERT INTO t VALUES(x'00', 'ok');

-- case: research/sqlite-release-notes/strict_update_type_error | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INTEGER) STRICT;
INSERT INTO t VALUES(1);
UPDATE t SET a = 'x';
UPDATE t SET a = '7';
SELECT a, typeof(a) FROM t;

-- case: research/sqlite-release-notes/strict_integer_text_big | source: https://sqlite.org/stricttables.html
CREATE TABLE t(a INTEGER) STRICT;
INSERT INTO t VALUES('1e3');
INSERT INTO t VALUES('99999999999999999999');
INSERT INTO t VALUES('0x10');
SELECT a, typeof(a) FROM t;

-- case: research/sqlite-release-notes/affinity_text_numeric_column_compare | source: https://sqlite.org/datatype3.html
CREATE TABLE t(i INTEGER, t TEXT, n NUMERIC, b BLOB, x);
INSERT INTO t VALUES(1,'1','1',1,'1');
SELECT i = t, i = n, i = b, i = x, t = n, t = x, n = x, b = x FROM t;
SELECT i < t, t < i, i < '2', t < 2, x < 2, x < '2' FROM t;
SELECT typeof(i), typeof(t), typeof(n), typeof(b), typeof(x) FROM t;

-- case: research/sqlite-release-notes/affinity_numeric_conversion_text | source: https://sqlite.org/datatype3.html
CREATE TABLE t(n NUMERIC);
INSERT INTO t VALUES('12'),('1.0'),('1e2'),(' 5 '),('0x1F'),('1_000'),('+7'),('.5'),('5.'),('1e'),('--1'),('9223372036854775808'),('-9223372036854775808'),('1.0e0');
SELECT n, typeof(n) FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/affinity_integer_float_whole | source: https://sqlite.org/datatype3.html
CREATE TABLE t(i INTEGER, n NUMERIC, r REAL);
INSERT INTO t VALUES(3.0, 3.0, 3), (3.5, 3.5, '3'), (1e18, 1e18, 1e18), (1e19, 1e19, 1e19), ('1e1', '1e1', '1e1');
SELECT i, typeof(i), n, typeof(n), r, typeof(r) FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/affinity_real_text_precision | source: https://sqlite.org/datatype3.html
CREATE TABLE t(r REAL);
INSERT INTO t VALUES('0.1'),('0.30000000000000004'),('123456789012345678'),('1e22'),('1e-5'),(1.0e15),(1.0e16),(123456789.123456789);
SELECT r, typeof(r), CAST(r AS TEXT), r || '' FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/cast_text_to_integer | source: https://sqlite.org/datatype3.html
SELECT CAST('12abc' AS INTEGER), CAST(' 12' AS INTEGER), CAST('12 ' AS INTEGER), CAST('abc' AS INTEGER), CAST('' AS INTEGER);
SELECT CAST('0x10' AS INTEGER), CAST('1e3' AS INTEGER), CAST('1.9' AS INTEGER), CAST('-1.9' AS INTEGER), CAST('+5' AS INTEGER);
SELECT CAST('9223372036854775807' AS INTEGER), CAST('9223372036854775808' AS INTEGER), CAST('-9223372036854775809' AS INTEGER), CAST('99999999999999999999999' AS INTEGER);
SELECT CAST(1e19 AS INTEGER), CAST(-1e19 AS INTEGER), CAST(9.2233720368547758e18 AS INTEGER), CAST(NULL AS INTEGER);

-- case: research/sqlite-release-notes/cast_real_to_text_and_back | source: https://sqlite.org/datatype3.html
SELECT CAST(1.0 AS TEXT), CAST(-0.0 AS TEXT), CAST(1e100 AS TEXT), CAST(0.1 AS TEXT), CAST(1.5e-7 AS TEXT), CAST(100000000000000.0 AS TEXT), CAST(1e15 AS TEXT), CAST(123456789012345.678 AS TEXT);
SELECT CAST(9e999 AS TEXT), CAST(-9e999 AS TEXT), 9e999, -9e999, typeof(9e999);
SELECT CAST('inf' AS REAL), CAST('nan' AS REAL), CAST('1e999' AS REAL), CAST('-1e999' AS REAL), CAST('Infinity' AS REAL);

-- case: research/sqlite-release-notes/cast_to_numeric_and_real | source: https://sqlite.org/datatype3.html
SELECT CAST('3.0' AS NUMERIC), typeof(CAST('3.0' AS NUMERIC)), CAST('3.5' AS NUMERIC), CAST('1e2' AS NUMERIC), typeof(CAST('1e2' AS NUMERIC)), CAST('abc' AS NUMERIC);
SELECT CAST(3 AS REAL), typeof(CAST(3 AS REAL)), CAST('3abc' AS REAL), CAST(x'33' AS REAL), CAST(x'33' AS INTEGER), CAST(x'33' AS TEXT);
SELECT CAST('9223372036854775808' AS NUMERIC), typeof(CAST('9223372036854775808' AS NUMERIC)), CAST('9223372036854775807.0' AS NUMERIC), CAST('0.0' AS NUMERIC), CAST('-0' AS NUMERIC);

-- case: research/sqlite-release-notes/cast_blob_text_roundtrip | source: https://sqlite.org/datatype3.html
SELECT CAST(x'4142' AS TEXT), CAST('AB' AS BLOB), hex(CAST('AB' AS BLOB)), length(CAST(x'410042' AS TEXT)), hex(CAST(x'410042' AS TEXT)), typeof(CAST(x'410042' AS TEXT));
SELECT length('a' || char(0) || 'b'), length(x'410042'), quote('a' || char(0) || 'b'), hex('a' || char(0) || 'b');

-- case: research/sqlite-release-notes/numeric_string_comparison_no_affinity | source: https://sqlite.org/datatype3.html
SELECT 1 = '1', 1 < '1', '1' < 1, 1.0 = 1, 1 = 1.0000000000000001, 'a' < x'61', x'61' < 'a', NULL = NULL, '' = 0, '0' = 0, 0 = '0.0';
SELECT 1 < 'a', 'a' < 1, 9999999999 < 'a', -1 < '', typeof(1 = 1);

-- case: research/sqlite-release-notes/integer_real_comparison_precision | source: https://sqlite.org/datatype3.html
SELECT 9223372036854775807 = 9223372036854775807.0, 9223372036854775807 < 9223372036854775808.0, 9007199254740993 = 9007199254740992.0, 9007199254740993 > 9007199254740992.0;
SELECT 9223372036854775807 + 0.0 = 9223372036854775807, -9223372036854775808 = -9223372036854775808.0, 9223372036854775806 < 9223372036854775807.0;
CREATE TABLE t(a INTEGER, b REAL);
INSERT INTO t VALUES(9007199254740993, 9007199254740992.0),(9223372036854775807, 9.3e18);
SELECT a < b, a = b, a > b FROM t ORDER BY rowid;

-- case: research/sqlite-release-notes/integer_overflow_arith | source: https://sqlite.org/datatype3.html
SELECT 9223372036854775807 + 1, -9223372036854775808 - 1, 9223372036854775807 * 2, 4611686018427387904 * 2, -9223372036854775808 * -1;
SELECT typeof(9223372036854775807 + 1), -(-9223372036854775808), 9223372036854775808, -9223372036854775808, typeof(-9223372036854775808);
SELECT abs(-9223372036854775808);

-- case: research/sqlite-release-notes/integer_division_modulo_edge | source: https://sqlite.org/lang_expr.html
SELECT 7 / 2, -7 / 2, 7 / -2, -7 / -2, 7 % 3, -7 % 3, 7 % -3, -7 % -3;
SELECT 1 / 0, 1 % 0, 1.0 / 0, 0.0 / 0, 1 % 0.0, 5.5 % 2, -5.5 % 2, 5 % 2.5;
SELECT -9223372036854775808 / -1, -9223372036854775808 % -1, 9223372036854775807 / -1, typeof(-9223372036854775808 / -1);

-- case: research/sqlite-release-notes/bit_operators_edge | source: https://sqlite.org/lang_expr.html
SELECT 1 << 63, 1 << 64, 1 << -1, -1 >> 1, -1 >> 64, 1 >> -1, 256 >> 4, 5 & 3, 5 | 3, ~5, -8 >> 1;
SELECT 1 << 62, 3 << 62, '3' << 1, 3.9 << 1, NULL << 1, 1 << NULL, 9223372036854775807 << 1;
SELECT x'01' & 1, 'a' | 1, 1.5 & 3, -1.5 | 0;

-- case: research/sqlite-release-notes/unary_minus_literal_edge | source: https://sqlite.org/lang_expr.html
SELECT -9223372036854775808, typeof(-9223372036854775808), -9223372036854775809, typeof(-9223372036854775809);
SELECT 0x7FFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF, 0x8000000000000000, -0x8000000000000000, 0x0, 0Xff;
SELECT 9223372036854775807, 9223372036854775808, typeof(9223372036854775808), 1e3, typeof(1e3), .5, 5., 1_000;

-- case: research/sqlite-release-notes/hex_literal_too_long | source: https://sqlite.org/lang_expr.html
SELECT 0x10000000000000000;

-- case: research/sqlite-release-notes/float_literal_forms | source: https://sqlite.org/lang_expr.html
SELECT 1.0, 1e0, 1E+2, 1e-2, 0.5e1, 1.e1, .1e1, 100000000000000000000.0, 1e300 * 1e300, -1e300 * 1e300, 1e-320, 5e-324, 2.2250738585072014e-308;
SELECT 0.1 + 0.2, 0.1 + 0.2 = 0.3, 1.0 / 3.0, 2.0 / 3.0, 1e15 + 0.3, 123456789012345678.0;

-- case: research/sqlite-release-notes/real_to_text_15_digits | source: https://sqlite.org/printf.html
SELECT 3.14159265358979323846, 1.0/7, 100.0/3, 1e15/7, 1e-10/3, 2.5e-5, 12345678901234567890.0, 0.000001234, 1e21, 1e20, 123456789012345.6, 1234567890123456.7;
SELECT 4.35 * 100, 1.1 * 1.1, 33.33 * 3, 0.07 * 100;

-- case: research/sqlite-release-notes/round_function_edge | source: https://sqlite.org/lang_corefunc.html
SELECT round(2.5), round(-2.5), round(0.5), round(1.5), round(2.675, 2), round(1.005, 2), round(-0.4), round(5.5, 0), round(1234.5678, -2), round(1234.5678, 2);
SELECT round(1e15 + 0.5), round(4503599627370496.5), round(9223372036854775807), round('3.7'), round(NULL), round(1.5, NULL), round(1.5, 100), round(0.0 / 1, 3);
SELECT typeof(round(5)), typeof(round(5.0)), typeof(round(5, 2)), round(-0.0), round(0.15, 1), round(0.25, 1), round(0.35, 1);

-- case: research/sqlite-release-notes/abs_sign_functions | source: https://sqlite.org/lang_corefunc.html
SELECT abs(-5), abs(-5.5), abs('-5'), abs('abc'), abs(NULL), abs(x'31'), abs(-0.0), abs('');
SELECT sign(-5), sign(0), sign(5.5), sign('x'), sign(NULL), sign('-3'), sign(-0.0);

-- case: research/sqlite-release-notes/math_functions_domain | source: https://sqlite.org/lang_mathfunc.html
SELECT sqrt(-1), ln(0), ln(-1), log(0), log10(-1), log2(0), acos(2), asin(-2), atanh(1), acosh(0.5), pow(0, -1), pow(-8, 1.0/3), mod(5, 0), mod(5.5, 2), mod(-5, 3), mod('a', 3);
SELECT sqrt(4), pow(2, 10), power(2, 0.5), exp(0), log(2, 8), log(10, 1000), ceil(1.1), ceiling(-1.1), floor(-1.1), trunc(-1.9), trunc(1.9), degrees(pi()), radians(180), typeof(ceil(5)), typeof(floor(1e20));

-- case: research/sqlite-release-notes/math_functions_inf_nan | source: https://sqlite.org/lang_mathfunc.html
SELECT 9e999 - 9e999, 9e999 * 0, 9e999 + 1, -9e999, sqrt(9e999), exp(1000), exp(-1000), atan(9e999), sin(9e999), cos(9e999);
SELECT 9e999 = 9e999, 9e999 > 1e308, -9e999 < -1e308, typeof(9e999 - 9e999), 9e999 - 9e999 IS NULL;

-- case: research/sqlite-release-notes/trig_functions_values | source: https://sqlite.org/lang_mathfunc.html
SELECT sin(0), cos(0), tan(0), asin(1), acos(1), atan(1), atan2(1, 1), atan2(0, -1), atan2(-1, 0), sinh(0), cosh(0), tanh(0), asinh(0), acosh(1), atanh(0);
SELECT round(sin(pi()), 10), round(cos(pi()), 10), round(atan2(1, 2), 12), round(exp(1), 12), round(ln(10), 12);

-- case: research/sqlite-release-notes/min_max_scalar_collation | source: https://sqlite.org/lang_corefunc.html
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT);
INSERT INTO t VALUES('b','A');
SELECT max(a, b), min(a, b), max(b, a), min(b, a), max(a, 'C'), max('c', a) FROM t;
SELECT max(1, '2'), min(1, '2'), max(NULL, 1), min(1, NULL), max(x'01', 'a'), min(1.5, 1);

-- case: research/sqlite-release-notes/coalesce_ifnull_nullif_iif | source: https://sqlite.org/lang_corefunc.html
SELECT coalesce(NULL, NULL, 3), ifnull(NULL, 'x'), nullif(1, 1), nullif(1, 2), nullif('a', 'A'), nullif(1, '1'), nullif(NULL, NULL), nullif(1, 1.0);
SELECT iif(1, 'y', 'n'), iif(0, 'y', 'n'), iif(NULL, 'y', 'n'), iif('0', 'y', 'n'), iif('1x', 'y', 'n'), iif(0, 1, 0, 2, 3);
SELECT coalesce(1, 1/0), ifnull(1, 1/0), CASE WHEN 1 THEN 1 ELSE 1/0 END;

-- case: research/sqlite-release-notes/case_expression_forms | source: https://sqlite.org/lang_expr.html
CREATE TABLE t(a INTEGER, b TEXT COLLATE NOCASE);
INSERT INTO t VALUES(1,'a'),(NULL,'B');
SELECT CASE a WHEN 1 THEN 'one' WHEN NULL THEN 'null' ELSE 'other' END, CASE WHEN a IS NULL THEN 'n' END FROM t ORDER BY rowid;
SELECT CASE b WHEN 'A' THEN 1 WHEN 'b' THEN 2 END, CASE 'A' WHEN b THEN 1 ELSE 0 END, CASE a WHEN '1' THEN 'match' END FROM t ORDER BY rowid;
SELECT CASE WHEN NULL THEN 1 ELSE 2 END, CASE 1 WHEN 1 THEN NULL ELSE 5 END, typeof(CASE WHEN 0 THEN 1 END);

-- case: research/sqlite-release-notes/collation_expression_precedence | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a TEXT COLLATE NOCASE, b TEXT COLLATE RTRIM, c TEXT);
INSERT INTO t VALUES('X','x ','x');
SELECT a = 'x', b = 'x', c = 'X', a = c, c = a, b = c, c = b, a = b, b = a FROM t;
SELECT a = c COLLATE BINARY, c COLLATE NOCASE = a, (a || '') = 'x', (a) = 'x', +a = 'x', a COLLATE RTRIM = 'x ' FROM t;

-- case: research/sqlite-release-notes/collate_rtrim_comparisons | source: https://sqlite.org/datatype3.html
SELECT 'a' = 'a  ' COLLATE RTRIM, 'a ' = 'a' COLLATE RTRIM, 'a' = 'a' || char(9) COLLATE RTRIM, ' a' = 'a' COLLATE RTRIM, 'a  ' < 'a ' COLLATE RTRIM, '' = '   ' COLLATE RTRIM;
CREATE TABLE t(a TEXT COLLATE RTRIM);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES('x'),('x '),('x  '),('y');
SELECT count(*) FROM t WHERE a = 'x';
SELECT DISTINCT a FROM t ORDER BY a;
SELECT count(*) FROM t WHERE a > 'x';

-- case: research/sqlite-release-notes/collate_nocase_non_ascii | source: https://sqlite.org/datatype3.html
SELECT 'Ä' = 'ä' COLLATE NOCASE, 'a' = 'A' COLLATE NOCASE, 'Z' < 'a' COLLATE NOCASE, '[' < 'a' COLLATE NOCASE, '_' < 'a' COLLATE NOCASE, 'z' < '{' COLLATE NOCASE;
SELECT 'A' < 'b' COLLATE NOCASE, 'a' < 'B' COLLATE NOCASE, 'B' < 'a' COLLATE BINARY, '_' < 'A' COLLATE NOCASE;

-- case: research/sqlite-release-notes/collate_unknown_error | source: https://sqlite.org/datatype3.html
SELECT 'a' = 'b' COLLATE nonexistent;

-- case: research/sqlite-release-notes/text_comparison_binary_utf8 | source: https://sqlite.org/datatype3.html
SELECT 'a' < 'b', 'Z' < 'a', char(65533) < char(65536), char(128512) < char(65533), 'é' > 'z', char(255) < char(256), '' < 'a';
CREATE TABLE t(a);
INSERT INTO t VALUES(char(65533)),(char(128512)),(char(127)),(char(128)),(char(2047)),(char(2048));
SELECT hex(a) FROM t ORDER BY a;

-- case: research/sqlite-release-notes/string_functions_unicode_length | source: https://sqlite.org/lang_corefunc.html
SELECT length('héllo'), length(x'c3a9'), length(CAST('héllo' AS BLOB)), length(''), length(NULL), length(12.50), length(123), length('a' || char(0) || 'b');
SELECT substr('héllo', 2, 3), substr('hello', -3), substr('hello', -3, 2), substr('hello', 0), substr('hello', 0, 2), substr('hello', 2, -1), substr('hello', 10), substr('hello', 1, 0), substr(x'414243', 2, 1), typeof(substr(x'414243', 2, 1));
SELECT instr('hello', 'l'), instr('hello', ''), instr('', 'a'), instr(NULL, 'a'), instr('héllo', 'l'), instr(x'414243', x'42'), instr('abc', x'62');

-- case: research/sqlite-release-notes/substr_huge_and_edge_args | source: https://sqlite.org/lang_corefunc.html
SELECT substr('abcdef', 2, 9223372036854775807), substr('abcdef', -9223372036854775808, 5), substr('abcdef', 9223372036854775807), substr('abcdef', 2.9, 2.9), substr('abcdef', '2', '3'), substr('abcdef', NULL, 2), substr('abcdef', 3, NULL), substr(12345, 2, 3);
SELECT substr('abcdef', -2, -2), substr('abcdef', -1, -3), substr('abcdef', 4, -2), substr('abcdef', 0, -1), substring('abcdef', 2);

-- case: research/sqlite-release-notes/trim_functions | source: https://sqlite.org/lang_corefunc.html
SELECT trim('  a  '), ltrim('  a  '), rtrim('  a  '), trim('xxaxx', 'x'), trim('xyxaxyx', 'xy'), ltrim('abcabc', 'ab'), rtrim('abcabc', 'bc'), trim('abc', ''), trim(NULL), trim('a', NULL), trim(12321, 1), trim(x'202061', x'20');
SELECT trim('  a  ' || char(9)), trim(char(9) || 'a' || char(10)), trim('é', 'é'), length(trim('  ')), quote(trim('   ')), trim('aéa', 'a');

-- case: research/sqlite-release-notes/replace_function_edge | source: https://sqlite.org/lang_corefunc.html
SELECT replace('aaaa', 'aa', 'b'), replace('abc', '', 'x'), replace('abc', 'b', ''), replace(NULL, 'a', 'b'), replace('abc', NULL, 'b'), replace('abc', 'b', NULL), replace(12345, 3, 'x'), replace('aaa', 'a', 'aa'), replace('', '', 'x'), replace('abc', 'ABC', 'x');

-- case: research/sqlite-release-notes/upper_lower_non_ascii | source: https://sqlite.org/lang_corefunc.html
SELECT upper('abcé'), lower('ABCÉ'), upper(NULL), upper(123), upper(x'6162'), typeof(upper(x'6162')), lower('İ'), upper('ı');

-- case: research/sqlite-release-notes/unicode_char_functions | source: https://sqlite.org/lang_corefunc.html
SELECT unicode('A'), unicode('é'), unicode(''), unicode(NULL), unicode('😀'), unicode(x'41'), unicode(65), hex(char(65, 233, 128512)), char(), hex(char(0)), hex(char(1114112)), hex(char(-1)), hex(char(55296)), hex(char(1114111));
SELECT length(char(128512)), length(CAST(char(128512) AS BLOB)), unicode(char(55296, 56320));

-- case: research/sqlite-release-notes/hex_unhex_functions | source: https://sqlite.org/lang_corefunc.html
SELECT hex('abc'), hex(x'00ff'), hex(255), hex(1.5), hex(NULL), hex(''), hex(x'');
SELECT hex(unhex('4142')), unhex('4g'), unhex('414'), hex(unhex('41 42', ' ')), unhex(NULL), hex(unhex('')), typeof(unhex('')), hex(unhex('aBcD')), unhex('4142', 'x');

-- case: research/sqlite-release-notes/quote_function_types | source: https://sqlite.org/lang_corefunc.html
SELECT quote(1), quote(1.0), quote(-0.0), quote('a''b'), quote(x'00'), quote(NULL), quote(''), quote(1e100), quote(9e999), quote(-9e999), quote(0.1), quote(1e-5), quote(123456789012345678);
SELECT quote(char(0)), quote('a' || char(0) || 'b'), quote(x''), quote(CAST(1 AS TEXT)), quote(12345.6789e3);

-- case: research/sqlite-release-notes/typeof_literals_expressions | source: https://sqlite.org/lang_corefunc.html
SELECT typeof(1+1), typeof(1+1.0), typeof('1'+1), typeof('a'||1), typeof(1||1), typeof(NULL+1), typeof(1/1), typeof(1/2.0), typeof(abs(1)), typeof(abs(1.0)), typeof(length('a')), typeof(randomblob(1)), typeof(zeroblob(1)), typeof(x''), typeof(''), typeof(0x10), typeof(1=1), typeof(+'1'), typeof(-'1'), typeof(-'a'), typeof(~1.5);
SELECT '1'+1, '1.5'+1, '1e1'+1, ' 1'+1, '1 '+1, 'a'+1, '0x10'+1, '1_0'+1, '.'+1, '+'+1, '-'+1, '--1'+1, '1e'+1, '١'+1;

-- case: research/sqlite-release-notes/concat_operator_types | source: https://sqlite.org/lang_expr.html
SELECT 'a' || NULL, NULL || NULL, 1 || 2, 1.0 || 2, x'41' || 'b', typeof(x'41' || x'42'), 1 || 2 + 3, 1 + 2 || 3, -1 || 2, 'a' || 1 = 'a1';
SELECT 1.0 || '', 1e100 || '', 100000000000000000000 || '', 0.1 || '', -0.0 || '', 9e999 || '';

-- case: research/sqlite-release-notes/string_concat_in_where_with_index | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT, b TEXT);
CREATE INDEX i ON t(a || '-' || b);
INSERT INTO t VALUES('x','1'),('x','2'),('y','1');
SELECT a, b FROM t WHERE a || '-' || b = 'x-2';
SELECT a, b FROM t WHERE a || '-' || b > 'x-1' ORDER BY a, b;
SELECT a, b FROM t WHERE (a || '-') || b = 'y-1';

-- case: research/sqlite-release-notes/index_on_expression_affinity | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT);
CREATE INDEX i ON t(a + 0);
INSERT INTO t VALUES('1'),('2'),('10'),('abc'),(' 3'),('1.0');
SELECT a FROM t WHERE a + 0 = 1 ORDER BY rowid;
SELECT a FROM t WHERE a + 0 = '1' ORDER BY rowid;
SELECT a FROM t WHERE a + 0 > 1 ORDER BY a + 0, rowid;
SELECT a FROM t WHERE a + 0 IS 0 ORDER BY rowid;

-- case: research/sqlite-release-notes/index_on_expression_nondeterministic_error | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
CREATE INDEX i ON t(random());

-- case: research/sqlite-release-notes/index_on_expression_collate_in_order_by | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a TEXT);
CREATE INDEX i ON t(lower(a) COLLATE BINARY);
INSERT INTO t VALUES('B'),('a'),('C'),('b');
SELECT a FROM t ORDER BY lower(a), a;
SELECT a FROM t WHERE lower(a) = 'b' ORDER BY a;
SELECT a FROM t WHERE lower(a) >= 'b' ORDER BY lower(a), a;

-- case: research/sqlite-release-notes/covering_index_vs_table_values | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a INTEGER, b TEXT);
CREATE INDEX i ON t(a, b);
INSERT INTO t VALUES('5','7'),(5.0,7),('x','y'),(NULL,NULL);
SELECT a, b, typeof(a), typeof(b) FROM t INDEXED BY i ORDER BY rowid;
SELECT a, b, typeof(a), typeof(b) FROM t NOT INDEXED ORDER BY rowid;

-- case: research/sqlite-release-notes/indexed_by_unusable_error | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(1,2);
SELECT * FROM t INDEXED BY i WHERE b = 2;

-- case: research/sqlite-release-notes/not_indexed_and_indexed_results_equal | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(1,'a'),('1','b'),(1.0,'c'),(x'31','d'),(NULL,'e'),('01','f'),(' 1','g');
SELECT b FROM t INDEXED BY i WHERE a = 1 ORDER BY b;
SELECT b FROM t NOT INDEXED WHERE a = 1 ORDER BY b;
SELECT b FROM t INDEXED BY i WHERE a = '1' ORDER BY b;
SELECT b FROM t NOT INDEXED WHERE a = '1' ORDER BY b;
SELECT b FROM t INDEXED BY i WHERE a > 0 ORDER BY b;
SELECT b FROM t NOT INDEXED WHERE a > 0 ORDER BY b;

-- case: research/sqlite-release-notes/between_with_index_and_affinity | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a TEXT, b INTEGER);
CREATE INDEX ia ON t(a);
CREATE INDEX ib ON t(b);
INSERT INTO t VALUES('1',1),('2',2),('10',10),('3',3),('a',4);
SELECT a FROM t WHERE a BETWEEN 1 AND 3 ORDER BY a;
SELECT a FROM t WHERE a BETWEEN '1' AND '3' ORDER BY a;
SELECT b FROM t WHERE b BETWEEN '2' AND '10' ORDER BY b;
SELECT b FROM t WHERE b BETWEEN 5 AND 1 ORDER BY b;
SELECT b FROM t WHERE b NOT BETWEEN 2 AND 3 ORDER BY b;

-- case: research/sqlite-release-notes/range_scan_with_nulls_and_mixed_types | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(NULL),(-1),(0),(1.5),(2),('a'),('b'),(x'00'),(x'ff');
SELECT quote(a) FROM t WHERE a > 0 ORDER BY a;
SELECT quote(a) FROM t WHERE a < 'a' ORDER BY a;
SELECT quote(a) FROM t WHERE a >= 'a' AND a <= 'b' ORDER BY a;
SELECT quote(a) FROM t WHERE a > x'00' ORDER BY a;
SELECT quote(a) FROM t WHERE a < 1 ORDER BY a;
SELECT quote(a) FROM t WHERE a <= NULL ORDER BY a;

-- case: research/sqlite-release-notes/range_scan_real_integer_bounds | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b INTEGER);
CREATE INDEX i ON t(b);
INSERT INTO t VALUES(1,1),(2,2),(3,3),(4,4);
SELECT a FROM t WHERE a > 1.5 ORDER BY a;
SELECT a FROM t WHERE a >= 1.5 ORDER BY a;
SELECT a FROM t WHERE a < 3.5 ORDER BY a;
SELECT a FROM t WHERE a <= 2.5 ORDER BY a DESC;
SELECT b FROM t WHERE b > 2.0 AND b < 4.0 ORDER BY b;
SELECT b FROM t WHERE b = 2.5;
SELECT b FROM t WHERE b > 9223372036854775807;
SELECT b FROM t WHERE b < 9223372036854775808 ORDER BY b;
SELECT b FROM t WHERE b > -9223372036854775809 ORDER BY b LIMIT 1;
SELECT b FROM t WHERE b >= 1e400 OR b <= -1e400;

-- case: research/sqlite-release-notes/rowid_compare_with_text_and_blob | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
INSERT INTO t VALUES('x'),('y'),('z');
SELECT a FROM t WHERE rowid > 'a';
SELECT a FROM t WHERE rowid < 'a' ORDER BY rowid;
SELECT a FROM t WHERE rowid = '1';
SELECT a FROM t WHERE rowid > x'00';
SELECT a FROM t WHERE rowid < x'00' ORDER BY rowid;
SELECT a FROM t WHERE rowid = NULL;
SELECT a FROM t WHERE rowid IS NULL;
SELECT a FROM t WHERE rowid >= '2' ORDER BY rowid;

-- case: research/sqlite-release-notes/rowid_order_by_desc_limit | source: https://sqlite.org/optoverview.html
CREATE TABLE t(a);
INSERT INTO t VALUES(10),(20),(30),(40),(50);
SELECT rowid, a FROM t ORDER BY rowid DESC LIMIT 2;
SELECT rowid, a FROM t WHERE rowid BETWEEN 2 AND 4 ORDER BY rowid DESC;
SELECT rowid, a FROM t WHERE rowid > 2 ORDER BY rowid DESC LIMIT 1 OFFSET 1;
SELECT max(rowid), min(rowid), count(rowid) FROM t WHERE rowid < 3;

-- case: research/sqlite-release-notes/compare_equal_nan_like_values | source: https://sqlite.org/datatype3.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(1,1),(NULL,2);
SELECT b FROM t WHERE a = NULL;
SELECT b FROM t WHERE a != NULL;
SELECT b FROM t WHERE a < NULL;
SELECT b FROM t WHERE a IN (NULL);
SELECT b FROM t WHERE a NOT IN (NULL);
SELECT b FROM t WHERE a = a ORDER BY b;
SELECT b FROM t WHERE NOT (a = 1) ORDER BY b;
SELECT b FROM t WHERE a <> 1 OR a IS NULL ORDER BY b;

-- case: research/sqlite-release-notes/json_extract_types | source: https://sqlite.org/json1.html
SELECT json_extract('{"a":1,"b":1.5,"c":"x","d":null,"e":true,"f":[1,2],"g":{"h":1}}', '$.a', '$.b', '$.c', '$.d', '$.e', '$.f', '$.g');
SELECT typeof(json_extract('{"a":1}', '$.a')), typeof(json_extract('{"a":1.0}', '$.a')), typeof(json_extract('{"a":null}', '$.a')), typeof(json_extract('{"a":true}', '$.a')), typeof(json_extract('{"a":"1"}', '$.a'));
SELECT json_extract('{"a":9223372036854775808}', '$.a'), json_extract('{"a":1e400}', '$.a'), json_extract('{"a":-0}', '$.a'), json_extract('{"a":12345678901234567890}', '$.a');

-- case: research/sqlite-release-notes/json_extract_path_forms | source: https://sqlite.org/json1.html
SELECT json_extract('[10,20,30]', '$[0]'), json_extract('[10,20,30]', '$[#-1]'), json_extract('[10,20,30]', '$[#-3]'), json_extract('[10,20,30]', '$[#-4]'), json_extract('[10,20,30]', '$[3]'), json_extract('[10,20,30]', '$[-1]');
SELECT json_extract('{"a b":1,"a.b":2}', '$."a b"'), json_extract('{"a b":1,"a.b":2}', '$."a.b"'), json_extract('{"a":{"b":[1,{"c":5}]}}', '$.a.b[1].c'), json_extract('{"a":1}', '$.b'), json_extract('{"a":1}', 'a'), json_extract('{"a":1}', '$');

-- case: research/sqlite-release-notes/json_arrow_operators | source: https://sqlite.org/json1.html
SELECT '{"a":[1,"x",null]}' -> '$.a', '{"a":[1,"x",null]}' -> '$.a[1]', '{"a":[1,"x",null]}' ->> '$.a[1]', '{"a":[1,"x",null]}' ->> '$.a[2]', '{"a":[1,"x",null]}' -> '$.a[2]', '{"a":[1,"x",null]}' ->> 'a', '{"a":[1,"x",null]}' -> 'a';
SELECT '[1,2,3]' -> 1, '[1,2,3]' ->> 1, '{"1":5}' -> 1, '{"1":5}' ->> '1', '{"a":1}' -> '$.b', typeof('{"a":1}' ->> '$.b'), typeof('{"a":"1"}' ->> '$.a'), typeof('{"a":"1"}' -> '$.a');

-- case: research/sqlite-release-notes/json_invalid_input_errors | source: https://sqlite.org/json1.html
SELECT json_valid('{'), json_valid('{"a":1}'), json_valid(''), json_valid(NULL), json_valid('[1,]'), json_valid('{"a":1,}'), json_valid('{a:1}'), json_valid('''a'''), json_valid('01'), json_valid('1.'), json_valid('.5'), json_valid('+1'), json_valid('"\x"'), json_valid('NaN'), json_valid('true'), json_valid('TRUE'), json_valid(' [1] '), json_valid('"a"'), json_valid('"a'||char(10)||'"');
SELECT json_extract('{', '$.a');

-- case: research/sqlite-release-notes/json_valid_flags_and_json5 | source: https://sqlite.org/json1.html
SELECT json_valid('{a:1}', 1), json_valid('{a:1}', 2), json_valid('{a:1}', 4), json_valid('{a:1}', 8), json_valid('{"a":1}', 1), json_valid(x'7b7d', 8), json_valid(x'7b7d'), json_valid('[0x10]', 2), json_valid('[+1,.5,5.,Infinity,-Infinity,NaN]', 2);
SELECT json('{a:1, ''b'': [0x10, .5, +3,],}'), json('[Infinity]'), json('[1e999]');

-- case: research/sqlite-release-notes/json_set_insert_replace | source: https://sqlite.org/json1.html
SELECT json_set('{"a":1}', '$.b', 2), json_insert('{"a":1}', '$.a', 9), json_replace('{"a":1}', '$.b', 9), json_set('{"a":1}', '$.a', NULL), json_set('[1,2]', '$[#]', 3), json_set('[1,2]', '$[5]', 3), json_insert('[1,2]', '$[#]', 3, '$[#]', 4);
SELECT json_set('{}', '$.a.b', 1), json_set('{}', '$.a[0]', 1), json_set('[]', '$[0].a', 1), json_set(NULL, '$.a', 1), json_set('{"a":1}', NULL, 2), json_set('{"a":1}', '$.a', json('[1]')), json_set('{"a":1}', '$.a', '[1]'), json_set('{"a":1}', '$.a', 1.0), json_set('{}', '$.a', x'01');

-- case: research/sqlite-release-notes/json_remove_patch | source: https://sqlite.org/json1.html
SELECT json_remove('[1,2,3,4]', '$[1]', '$[1]'), json_remove('[1,2,3,4]', '$[#-1]'), json_remove('{"a":1,"b":2}', '$.a'), json_remove('{"a":1}', '$.b'), json_remove('{"a":1}', '$'), json_remove('[1,2,3]', '$[0]', '$[1]');
SELECT json_patch('{"a":1,"b":2}', '{"a":null,"c":3}'), json_patch('{"a":{"x":1}}', '{"a":{"y":2}}'), json_patch('{"a":[1]}', '{"a":[2]}'), json_patch('{"a":1}', '5'), json_patch('5', '{"a":1}'), json_patch('{"a":1}', 'null'), json_patch('{}', '{"a":{"b":null}}');

-- case: research/sqlite-release-notes/json_array_object_building | source: https://sqlite.org/json1.html
SELECT json_array(1, 1.5, 'x', NULL, json('[1]'), '[2]', 1e100, -0.0, 9e999), json_object('a', 1, 'b', NULL, 'c', json('{"d":1}'), 'e', 'f'), json_array(), json_object(), json_array(1, 'a''b', 'q"q', 'back\slash', char(10), char(31), char(127), char(0));
SELECT json_array(x'01');

-- case: research/sqlite-release-notes/json_object_odd_args_error | source: https://sqlite.org/json1.html
SELECT json_object('a');

-- case: research/sqlite-release-notes/json_object_nontext_key_error | source: https://sqlite.org/json1.html
SELECT json_object(1, 2);

-- case: research/sqlite-release-notes/json_quote_type | source: https://sqlite.org/json1.html
SELECT json_quote('x'), json_quote(1), json_quote(1.5), json_quote(NULL), json_quote('[1]'), json_quote(json('[1]')), json_quote('a"b'), json_quote(1e100), json_quote(9e999), json_quote(-9e999), json_quote(0.1), json_quote(12345678901234567890);

-- case: research/sqlite-release-notes/json_group_array_object | source: https://sqlite.org/json1.html
CREATE TABLE t(k, v);
INSERT INTO t VALUES('a',1),('b',NULL),('a',3),('c','[4]');
SELECT json_group_array(v), json_group_object(k, v), json_group_array(DISTINCT k) FROM t;
SELECT json_group_array(v), json_group_object(k, v) FROM t WHERE 0;
SELECT json_group_array(json(v)) FROM t WHERE v = '[4]';

-- case: research/sqlite-release-notes/json_each_tree_nesting | source: https://sqlite.org/json1.html
SELECT key, value, type, atom, fullkey, path FROM json_each('{"a":[1,2],"b":{"c":null},"d":"x","e":true,"f":1.5}') ORDER BY id;
SELECT key, type, atom, fullkey, path FROM json_tree('{"a":[1,{"b":2}],"c":3}') ORDER BY id;
SELECT count(*) FROM json_tree('[[],{},[[]]]');
SELECT key, value FROM json_each('[10,20]', '$') ORDER BY key;
SELECT key, value FROM json_each('{"a":[10,20]}', '$.a') ORDER BY key;

-- case: research/sqlite-release-notes/json_each_scalar_and_empty | source: https://sqlite.org/json1.html
SELECT key, value, type FROM json_each('5');
SELECT count(*) FROM json_each('[]'), (SELECT 1);
SELECT count(*) FROM json_each('{}');
SELECT count(*) FROM json_each(NULL);
SELECT key, value FROM json_each('{"a":1}', '$.zz');
SELECT key, value, type FROM json_each('"str"');

-- case: research/sqlite-release-notes/json_each_join_with_table | source: https://sqlite.org/json1.html
CREATE TABLE t(id, doc);
INSERT INTO t VALUES(1,'[1,2,3]'),(2,'[]'),(3,NULL),(4,'{"a":5}');
SELECT id, j.key, j.value FROM t, json_each(t.doc) AS j ORDER BY id, j.key;
SELECT id, count(j.value) FROM t LEFT JOIN json_each(t.doc) AS j GROUP BY id ORDER BY id;
SELECT id, (SELECT sum(value) FROM json_each(t.doc)) FROM t ORDER BY id;

-- case: research/sqlite-release-notes/json_array_length_type | source: https://sqlite.org/json1.html
SELECT json_array_length('[1,2,3]'), json_array_length('{}'), json_array_length('5'), json_array_length('[1,[2,3]]', '$[1]'), json_array_length('[1]', '$.a'), json_array_length(NULL), json_array_length('[]');
SELECT json_type('1'), json_type('1.5'), json_type('"a"'), json_type('true'), json_type('false'), json_type('null'), json_type('[]'), json_type('{}'), json_type('{"a":1}', '$.b'), json_type('{"a":null}', '$.a'), json_type(NULL), json_type('1e2'), json_type('-0');

-- case: research/sqlite-release-notes/json_minify_text_roundtrip | source: https://sqlite.org/json1.html
SELECT json(' { "a" : 1 , "b" : [ 1 , 2 ] } '), json('{"a":1,"a":2}'), json('[1.0,1e2,-0,0.10]'), json('"\u0041"'), json('{"b":1,"a":2}'), json('[1,2]  '), json(' 1 '), json('"é"'), json('"\ud83d\ude00"');
SELECT json_extract('{"a":1,"a":2}', '$.a'), json_extract('{"a":1.0}', '$.a'), json_extract('[1e2]', '$[0]'), json_extract('"\u0041"', '$'), json_extract('"\ud83d\ude00"', '$');

-- case: research/sqlite-release-notes/json_extract_multiple_paths_and_null | source: https://sqlite.org/json1.html
SELECT json_extract('{"a":1,"b":[2]}', '$.a', '$.b'), json_extract('{"a":1}', '$.a', '$.z'), json_extract('{"a":1}', '$.z', '$.y'), json_extract('{"a":"x"}', '$.a', '$.a'), json_extract('{"a":1}', '$.a', NULL);

-- case: research/sqlite-release-notes/json_in_where_comparisons | source: https://sqlite.org/json1.html
CREATE TABLE t(id, j);
INSERT INTO t VALUES(1,'{"n":1}'),(2,'{"n":"1"}'),(3,'{"n":1.0}'),(4,'{"n":null}'),(5,'{"n":true}'),(6,'{}');
SELECT id FROM t WHERE json_extract(j, '$.n') = 1 ORDER BY id;
SELECT id FROM t WHERE json_extract(j, '$.n') = '1' ORDER BY id;
SELECT id FROM t WHERE json_extract(j, '$.n') IS NULL ORDER BY id;
SELECT id FROM t WHERE j ->> '$.n' = 1 ORDER BY id;
SELECT id FROM t WHERE j ->> '$.n' IS NOT NULL ORDER BY id;
SELECT id, json_type(j, '$.n') FROM t ORDER BY id;

-- case: research/sqlite-release-notes/json_blob_jsonb_functions | source: https://sqlite.org/json1.html
SELECT json(jsonb('{"a":[1,2,{"b":null}]}')), json_valid(jsonb('[1]')), typeof(jsonb('[1]')), json_extract(jsonb('{"a":5}'), '$.a'), jsonb_extract('{"a":[1]}', '$.a') IS NOT NULL, typeof(jsonb_extract('{"a":[1]}', '$.a')), json_type(jsonb('{"a":1.5}'), '$.a');
SELECT json_array_length(jsonb_array(1,2,3)), json(jsonb_object('a', 1)), json(jsonb_set('{"a":1}', '$.b', 2)), json(jsonb_insert('[1]', '$[#]', 2)), json(jsonb_remove('[1,2]', '$[0]')), json(jsonb_replace('{"a":1}', '$.a', 3)), json(jsonb_patch('{"a":1}', '{"b":2}'));

-- case: research/sqlite-release-notes/json_set_numeric_text_args | source: https://sqlite.org/json1.html
SELECT json_set('{}', '$.a', '1'), json_set('{}', '$.a', 1), json_set('{}', '$.a', json('1')), json_set('{}', '$.a', '{"b":1}'), json_set('{}', '$.a', json('{"b":1}')), json_set('{}', '$.a', 'true'), json_set('{}', '$.a', 1 = 1), json_set('{}', '$.a', TRUE);

-- case: research/sqlite-release-notes/json_deep_nesting_limit | source: https://sqlite.org/json1.html
SELECT json_valid(replace(printf('%.*c', 500, '['), '[', '[') || replace(printf('%.*c', 500, ']'), ']', ']'));
SELECT json_valid(replace(printf('%.*c', 2000, '['), '[', '[') || replace(printf('%.*c', 2000, ']'), ']', ']'));

-- case: research/sqlite-release-notes/json_string_escapes_control_chars | source: https://sqlite.org/json1.html
SELECT json_quote(char(1)), json_quote(char(8)), json_quote(char(9)), json_quote(char(10)), json_quote(char(12)), json_quote(char(13)), json_quote(char(31)), json_quote(char(34)), json_quote(char(47)), json_quote(char(92)), json_quote(char(127)), json_quote(char(128)), json_quote(char(128512)), json_quote('');
SELECT json_extract('"\u0000"', '$') IS NOT NULL, hex(json_extract('"\u00e9"', '$')), json_extract('"\/"', '$'), json_valid('"\u12"'), json_valid('"\q"');

-- case: research/sqlite-release-notes/date_basic_formats | source: https://sqlite.org/lang_datefunc.html
SELECT date('2024-02-29'), time('12:34:56'), datetime('2024-02-29 12:34:56'), date('2024-02-29T12:34:56'), time('12:34'), datetime('2024-02-29 12:34'), date('20240229'), julianday('2000-01-01 12:00:00'), julianday('2024-02-29');
SELECT date('2024-02-30'), date('2023-02-29'), date('2024-13-01'), date('2024-00-10'), date('2024-01-00'), date('2024-1-1'), date('24-01-01'), time('25:00:00'), time('24:00:00'), time('12:60'), datetime('2024-01-01 24:00:00');

-- case: research/sqlite-release-notes/date_modifiers_months_overflow | source: https://sqlite.org/lang_datefunc.html
SELECT date('2024-01-31', '+1 month'), date('2024-03-31', '-1 month'), date('2023-02-28', '+1 year'), date('2024-02-29', '+1 year'), date('2024-02-29', '-1 year'), date('2024-01-31', '+13 months'), date('2024-12-31', '+2 months');
SELECT date('2024-01-01', '+1 day', '+1 month'), date('2024-01-01', '-1 day'), date('2024-01-01', '+365 days'), date('2024-01-01', '+0 days'), datetime('2024-01-01 12:00:00', '+36 hours'), datetime('2024-01-01 12:00:00', '-12.5 hours'), datetime('2024-01-01 00:00:00', '+90 minutes', '+30 seconds'), datetime('2024-01-01', '+1.5 seconds');

-- case: research/sqlite-release-notes/date_start_of_and_weekday | source: https://sqlite.org/lang_datefunc.html
SELECT date('2024-05-17', 'start of month'), date('2024-05-17', 'start of year'), datetime('2024-05-17 13:45:10', 'start of day'), date('2024-05-17', 'weekday 0'), date('2024-05-19', 'weekday 0'), date('2024-05-17', 'weekday 5'), date('2024-05-17', 'weekday 1'), date('2024-05-17', 'start of month', '+1 month', '-1 day');
SELECT date('2024-05-17', 'weekday 7');

-- case: research/sqlite-release-notes/date_unixepoch_and_auto | source: https://sqlite.org/lang_datefunc.html
SELECT datetime(0, 'unixepoch'), datetime(1700000000, 'unixepoch'), datetime(1700000000.5, 'unixepoch'), unixepoch('2024-01-01'), unixepoch('2024-01-01 00:00:00.999'), unixepoch('1969-12-31 23:59:59'), datetime(1700000000, 'auto'), datetime(2460000.5, 'auto'), datetime(2460000.5), datetime(-1, 'unixepoch');
SELECT strftime('%s', '2024-01-01'), strftime('%J', '2000-01-01 12:00'), strftime('%j', '2024-12-31'), strftime('%w', '2024-05-19'), strftime('%u', '2024-05-19'), strftime('%W', '2024-01-01'), strftime('%U', '2024-01-01'), strftime('%f', '2024-01-01 00:00:05.123'), strftime('%H:%M:%S', '2024-01-01 13:14:15'), strftime('%Y-%m-%d', 1700000000, 'unixepoch'), strftime('%%'), strftime('%q', '2024-01-01');

-- case: research/sqlite-release-notes/date_subsec_modifiers | source: https://sqlite.org/lang_datefunc.html
SELECT datetime('2024-01-01 12:00:00.123'), datetime('2024-01-01 12:00:00.123', 'subsec'), time('12:00:00.987', 'subsec'), strftime('%f', '2024-01-01 12:00:00.987'), time('12:00:00.9999'), datetime('2024-01-01 12:00:00.999999', 'subsec'), datetime('2024-01-01 23:59:59.9996', 'subsec'), strftime('%s', '2024-01-01 12:00:00.9');

-- case: research/sqlite-release-notes/date_julian_range_edges | source: https://sqlite.org/lang_datefunc.html
SELECT date('0000-01-01'), date('0000-01-01', '-1 day'), date('9999-12-31'), date('9999-12-31', '+1 day'), datetime('9999-12-31 23:59:59', '+1 second'), julianday('0000-01-01'), julianday('9999-12-31'), date(0), date(-1), date(5373484.5), date(5373485), date(-0.5);
SELECT date('-4713-11-24'), date('1582-10-10'), date('1582-10-04', '+1 day'), julianday('1582-10-15') - julianday('1582-10-04');

-- case: research/sqlite-release-notes/date_timezone_suffix | source: https://sqlite.org/lang_datefunc.html
SELECT datetime('2024-01-01 12:00:00Z'), datetime('2024-01-01 12:00:00+02:00'), datetime('2024-01-01 12:00:00-05:30'), datetime('2024-01-01T12:00:00+00:00'), time('12:00+01:00'), datetime('2024-01-01 00:30:00+01:00'), datetime('2024-01-01 12:00:00 +02:00'), datetime('2024-01-01 12:00:00+2');

-- case: research/sqlite-release-notes/date_invalid_modifier_null | source: https://sqlite.org/lang_datefunc.html
SELECT date('2024-01-01', 'garbage'), date('2024-01-01', '+1 fortnight'), date('2024-01-01', '+x days'), date('2024-01-01', NULL), date(NULL), date(''), date('now', 'garbage'), datetime('2024-01-01', '+1 day', 'bogus'), date('2024-01-01', '1 day'), date('2024-01-01', '+ 1 day'), date('2024-01-01', '+1day'), date('2024-01-01', 'START OF MONTH'), date('2024-01-01', '+1 DAYS');

-- case: research/sqlite-release-notes/date_julianday_float_text | source: https://sqlite.org/lang_datefunc.html
SELECT date(2460000), date('2460000'), date(2460000.4999), datetime(2460000.5), datetime('2460000.5'), date(0.5), datetime(0), datetime(1e10), datetime(1e20), date(1e7), date(9999999.9999), date('abc'), date(x'32303234'), date(20240101);

-- case: research/sqlite-release-notes/date_diff_julianday_arith | source: https://sqlite.org/lang_datefunc.html
SELECT julianday('2024-03-01') - julianday('2024-02-01'), CAST(julianday('2024-12-25') - julianday('2024-01-01') AS INTEGER), (strftime('%s','2024-01-02') - strftime('%s','2024-01-01')), CAST(strftime('%Y', '2024-06-15') AS INTEGER) - 2000, strftime('%Y-%m-%d %H:%M:%S', '2024-06-15 01:02:03'), typeof(strftime('%s', '2024-01-01')), typeof(strftime('%Y', '2024-01-01')), typeof(julianday('2024-01-01')), typeof(unixepoch('2024-01-01'));

-- case: research/sqlite-release-notes/timediff_function | source: https://sqlite.org/lang_datefunc.html
SELECT timediff('2024-03-01', '2024-02-01'), timediff('2024-02-01', '2024-03-01'), timediff('2024-01-01 12:00:00', '2024-01-01 11:00:00.5'), timediff('2025-01-01', '2024-01-01'), timediff('2024-03-31', '2024-02-29'), timediff('2024-02-29', '2024-03-31'), timediff('bad', '2024-01-01'), timediff('2024-01-01', '2024-01-01');

-- case: research/sqlite-release-notes/date_ceiling_floor_modifiers | source: https://sqlite.org/lang_datefunc.html
SELECT date('2024-01-31', '+1 month', 'ceiling'), date('2024-01-31', '+1 month', 'floor'), date('2024-01-31', '+1 month'), date('2023-03-31', '-1 month', 'floor'), date('2024-02-29', '+1 year', 'ceiling'), date('2024-02-29', '+1 year', 'floor'), date('2024-01-31', 'floor');

-- case: research/sqlite-release-notes/printf_integer_formats | source: https://sqlite.org/printf.html
SELECT printf('%d|%5d|%-5d|%05d|%+d|% d|%x|%X|%o|%#x|%#o|%c|%%', 42, 42, 42, 42, 42, 42, 255, 255, 8, 255, 8, 65);
SELECT printf('%d', -9223372036854775808), printf('%u', -1), printf('%x', -1), printf('%o', -1), printf('%d', 9223372036854775807), printf('%d', 1e30), printf('%d', '12abc'), printf('%d', NULL), printf('%d', 'x'), printf('%d', 3.99), printf('%d', -3.99), printf('%5.3d', 7), printf('%.0d', 0), printf('%lld', 5), printf('%ld', 5), printf('%i', 5), printf('%2$d %1$d', 1, 2);

-- case: research/sqlite-release-notes/printf_float_formats | source: https://sqlite.org/printf.html
SELECT printf('%f', 1.5), printf('%.2f', 2.675), printf('%.0f', 0.5), printf('%.0f', 1.5), printf('%.0f', 2.5), printf('%10.3f|', 3.14159), printf('%-10.3f|', 3.14159), printf('%e', 12345.6789), printf('%.3e', 0.000123), printf('%g', 0.0001), printf('%g', 0.00001), printf('%g', 1e15), printf('%g', 123456789), printf('%G', 1e-10), printf('%.10g', 1.0/3), printf('%f', 1e20), printf('%.20f', 0.1);
SELECT printf('%f', 9e999), printf('%f', -9e999), printf('%e', 9e999), printf('%g', 9e999), printf('%f', NULL), printf('%f', 'abc'), printf('%f', '1.5x'), printf('%.3f', 1), printf('%f', -0.0), printf('%+.1f', 0.0), printf('%05.1f', -1.5), printf('%f', 1e300 * 1e10), printf('%!.3f', 1.5), printf('%.15f', 0.1), printf('%.16f', 0.1), printf('%.50f', 0.5);

-- case: research/sqlite-release-notes/printf_string_formats | source: https://sqlite.org/printf.html
SELECT printf('%s|%10s|%-10s|%.2s|%5.1s|', 'abc', 'abc', 'abc', 'abc', 'abc'), printf('%s', NULL), printf('%s', 12), printf('%s', 1.5), printf('%s', x'41'), printf('%q', 'it''s'), printf('%Q', 'it''s'), printf('%Q', NULL), printf('%q', NULL), printf('%w', 'a"b'), printf('%Q', 5), printf('%Q', 1.5), printf('%q', x'41');
SELECT printf('%5s|', 'é'), printf('%.1s', 'éa'), printf('%-5s|', 'éé'), printf('%3c|', 'é'), printf('%c', 'abc'), printf('%c', 256), printf('%c', ''), printf('%c', 0), length(printf('%c', 0)), printf('%s %s', 'a'), printf('%s', 'a', 'b'), printf('%z', 'a'), printf('%', 1), printf('%5%'), printf('%-5%|');

-- case: research/sqlite-release-notes/printf_star_width_and_misc | source: https://sqlite.org/printf.html
SELECT printf('%*d|', 6, 42), printf('%-*d|', 6, 42), printf('%.*f', 2, 3.14159), printf('%*.*f|', 8, 2, 3.14159), printf('%*d|', -6, 42), printf('%.*s', 2, 'abcdef'), printf('%.*s', -1, 'abcdef'), printf('hello'), printf('%d%%', 50), printf(''), printf(NULL), printf('%d %d', 1), printf('%s', ''), printf('%0*d', 5, 7);
SELECT format('%d', 5), format('%s-%s', 'a', 'b'), printf('%,d', 1234567), printf('%,.2f', 1234567.891), printf('%!d', 5), printf('%!s', 'x');

-- case: research/sqlite-release-notes/printf_precision_limits | source: https://sqlite.org/printf.html
SELECT length(printf('%500d', 1)), length(printf('%.500f', 1.5)), length(printf('%.1000f', 1.5)), length(printf('%5000s', 'x')), length(printf('%1000000s', 'x')), length(printf('%.1000000f', 1.0));

-- case: research/sqlite-release-notes/printf_g_flag_alt_form | source: https://sqlite.org/printf.html
SELECT printf('%#g', 1.0), printf('%g', 1.0), printf('%#.0f', 5.0), printf('%.0f', 5.0), printf('%#.3g', 1.0), printf('%#e', 1.0), printf('%!g', 1.0), printf('%!.15g', 0.1), printf('%!.20g', 0.1), printf('%.20g', 0.1), printf('%g', 100000), printf('%g', 1000000), printf('%g', 0.1 + 0.2), printf('%.17g', 0.1 + 0.2);

-- case: research/sqlite-release-notes/random_blob_zeroblob_lengths | source: https://sqlite.org/lang_corefunc.html
SELECT length(zeroblob(0)), length(zeroblob(5)), typeof(zeroblob(0)), length(zeroblob(-1)), length(randomblob(0)), length(randomblob(-5)), length(randomblob(16)), typeof(randomblob(4)), hex(zeroblob(3)), length(zeroblob('3')), length(zeroblob(2.9)), zeroblob(NULL), hex(zeroblob(1) || x'01');

-- case: research/sqlite-release-notes/zeroblob_too_big_error | source: https://sqlite.org/lang_corefunc.html
SELECT length(zeroblob(2000000000));

-- case: research/sqlite-release-notes/like_likelihood_unlikely_funcs | source: https://sqlite.org/lang_corefunc.html
SELECT likely(5), unlikely('a'), likelihood(7, 0.5), likelihood(NULL, 0.1), typeof(likely(1.5));
SELECT likelihood(1, 2);

-- case: research/sqlite-release-notes/sqlite_functions_misc | source: https://sqlite.org/lang_corefunc.html
CREATE TABLE t(a INTEGER PRIMARY KEY, b);
INSERT INTO t(b) VALUES('x'),('y');
SELECT last_insert_rowid(), changes(), total_changes();
UPDATE t SET b = 'z';
SELECT changes(), total_changes(), last_insert_rowid();
DELETE FROM t WHERE a = 1;
SELECT changes(), total_changes();
INSERT INTO t VALUES(10,'q') ON CONFLICT DO NOTHING;
INSERT INTO t VALUES(10,'q') ON CONFLICT DO NOTHING;
SELECT changes(), last_insert_rowid();

-- case: research/sqlite-release-notes/octet_length_and_concat_ws | source: https://sqlite.org/lang_corefunc.html
SELECT octet_length('é'), octet_length(x'0102'), octet_length(123), octet_length(1.5), octet_length(NULL), octet_length(''), length('é');
SELECT concat('a', NULL, 1, 2.5, x'41'), concat(), concat(NULL), concat_ws('-', 'a', NULL, 'b'), concat_ws(NULL, 'a', 'b'), concat_ws('-'), concat_ws('-', NULL), concat_ws('', 1, 2), concat_ws(',', 1.5, 'x');

-- case: research/sqlite-release-notes/iif_in_group_by_and_aggregate_filter | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,1),(1,2),(2,3),(NULL,4);
SELECT count(DISTINCT a) FILTER (WHERE b > 1), sum(DISTINCT a), avg(DISTINCT a), total(DISTINCT a), group_concat(DISTINCT a) FROM t;
SELECT count(DISTINCT a, b) FROM t;

-- case: research/sqlite-release-notes/avg_and_total_types | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT avg(a), typeof(avg(a)), sum(a), typeof(sum(a)), total(a), typeof(total(a)) FROM t;
SELECT avg(a), sum(a), total(a), typeof(avg(a)), typeof(sum(a)), typeof(total(a)) FROM t WHERE 0;
SELECT sum(a / 2), sum(a * 1.0), avg('3'), sum('3'), sum('3.0'), sum('0x10'), sum(x'31'), sum(1e0), typeof(sum(1e0)) FROM t;

-- case: research/sqlite-release-notes/aggregate_in_where_error | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
SELECT a FROM t WHERE sum(a) > 1;

-- case: research/sqlite-release-notes/aggregate_nested_error | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
SELECT sum(count(*)) FROM t;

-- case: research/sqlite-release-notes/count_star_with_index_optimisation | source: https://sqlite.org/lang_aggfunc.html
CREATE TABLE t(a, b);
CREATE INDEX i ON t(a);
INSERT INTO t VALUES(1,1),(NULL,2),(NULL,3),(2,4);
SELECT count(*), count(a), count(b), count(a) + count(*) FROM t;
SELECT count(*) FROM t WHERE a IS NULL;
SELECT count(*) FROM t WHERE a > 0;
SELECT count(a) FROM t WHERE a IS NOT NULL;
DELETE FROM t WHERE a IS NULL;
SELECT count(*), count(a) FROM t;

-- case: research/sqlite-release-notes/select_star_column_naming | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,2);
SELECT a, a AS x, t.a, t.a + 1, (a), 'lit', 1+1, a || b, count(*), max(a), -a, (SELECT a), CAST(a AS TEXT), a IS NULL FROM t;
SELECT * FROM (SELECT a, b AS bb FROM t), t AS u;
SELECT t.*, u.* FROM t, t AS u;

-- case: research/sqlite-release-notes/select_without_from_aggregates | source: https://sqlite.org/lang_select.html
SELECT count(*), sum(1), max(5), min('a'), avg(2), total(3), group_concat('x');
SELECT count(*) WHERE 0;
SELECT 1 WHERE 1;
SELECT 1 WHERE NULL;
SELECT 1 LIMIT 0;
SELECT 1 ORDER BY 1 LIMIT 5 OFFSET 0;

-- case: research/sqlite-release-notes/select_distinct_with_order_by_nonselected | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a, b);
INSERT INTO t VALUES(1,5),(1,3),(2,4),(2,4);
SELECT DISTINCT a FROM t ORDER BY b;
SELECT DISTINCT a, b FROM t ORDER BY b DESC;
SELECT a FROM t GROUP BY a ORDER BY min(b), a;
SELECT a FROM t GROUP BY a ORDER BY max(b) DESC;

-- case: research/sqlite-release-notes/select_limit_with_distinct_and_join | source: https://sqlite.org/lang_select.html
CREATE TABLE a(x);
CREATE TABLE b(y);
INSERT INTO a VALUES(1),(1),(2);
INSERT INTO b VALUES(1),(1);
SELECT DISTINCT x FROM a ORDER BY x LIMIT 1;
SELECT x, y FROM a JOIN b ON x = y ORDER BY x LIMIT 3;
SELECT DISTINCT x FROM a JOIN b ON x = y;
SELECT count(*) FROM (SELECT DISTINCT x, y FROM a, b);

-- case: research/sqlite-release-notes/expression_in_limit_subquery_not_allowed | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
SELECT a FROM t ORDER BY a LIMIT (SELECT 2);
SELECT a FROM t ORDER BY a LIMIT 1.0;
SELECT a FROM t ORDER BY a LIMIT 1.5;

-- case: research/sqlite-release-notes/limit_text_non_integer_error | source: https://sqlite.org/lang_select.html
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
SELECT a FROM t LIMIT 'abc';

-- case: research/sqlite-release-notes/empty_string_and_null_in_unique_text | source: https://sqlite.org/nulls.html
CREATE TABLE t(a UNIQUE);
INSERT INTO t VALUES(''),(NULL),(NULL),(' ');
SELECT quote(a) FROM t ORDER BY a;
SELECT count(*) FROM t WHERE a = '';
SELECT count(*) FROM t WHERE a IS NULL;
SELECT '' IS NULL, '' = NULL, '' < ' ', NULL < '';
