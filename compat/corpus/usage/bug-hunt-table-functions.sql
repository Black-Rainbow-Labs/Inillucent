-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/table-functions/json_each_scalar_and_empty
-- json_each beside a derived table
SELECT key, value, type FROM json_each('5');
SELECT count(*) FROM json_each('[]'), (SELECT 1);
SELECT count(*) FROM json_each('{}');
SELECT count(*) FROM json_each(NULL);
SELECT key, value FROM json_each('{"a":1}', '$.zz');
SELECT key, value, type FROM json_each('"str"');

-- case: bug-hunt/table-functions/table-function-after-the-term-its-argument-reads
-- join order keeps json_each after the table it reads
CREATE TABLE t(id INTEGER PRIMARY KEY, v INT);
INSERT INTO t VALUES(3, 30), (6, 60), (7, 70);
CREATE TABLE j(doc TEXT);
INSERT INTO j VALUES('[3,6,9]');
SELECT count(*), sum(t.v) FROM j, json_each(j.doc) e JOIN t ON t.id=e.value;
SELECT e.value, t.v FROM j, json_each(j.doc) e JOIN t ON t.id=e.value;
SELECT e.value, t.v FROM j, json_each(j.doc) e, t WHERE t.id=e.value;
SELECT e.value, t.v FROM j JOIN json_each(j.doc) e JOIN t ON t.id=e.value;
SELECT e.value, t.v FROM json_each('[3,6,9]') e JOIN t ON t.id=e.value;

-- case: bug-hunt/table-functions/table-function-on-the-right-of-right-and-full-join
-- RIGHT and FULL JOIN with json_each and generate_series
CREATE TABLE a(key TEXT);
INSERT INTO a(key) VALUES('a'),('b');
SELECT a.key, b.value FROM a RIGHT JOIN json_each('["a","c"]') AS b ON a.key=b.value;
SELECT a.key, b.value FROM json_each('["a","c"]') AS b LEFT JOIN a ON a.key=b.value;
SELECT a.key, b.value FROM a FULL JOIN json_each('["a","c"]') AS b ON a.key=b.value;
CREATE TABLE c(value TEXT);
INSERT INTO c VALUES('a'),('c');
SELECT a.key, b.value FROM a RIGHT JOIN c AS b ON a.key=b.value;
SELECT a.key, b.value FROM a RIGHT JOIN (SELECT 'a' AS value UNION ALL SELECT 'c') AS b ON a.key=b.value;
SELECT a.key, b.value FROM a RIGHT JOIN generate_series(1,2) AS b ON a.key=b.value;

-- case: bug-hunt/table-functions/right_join_json_each_on_constraint
-- the forum report of the same
CREATE TABLE a(key TEXT);
INSERT INTO a(key) VALUES('a'),('b');
SELECT a.key, b.value FROM a RIGHT JOIN json_each('["a","c"]') AS b ON a.key=b.value;

-- case: bug-hunt/table-functions/fts5-table-valued-and-equality-match
-- SELECT * FROM t(query) and WHERE t = query
CREATE VIRTUAL TABLE t USING fts5 (a, b);
INSERT INTO t (a, b) VALUES ('data1', 'sentence1'), ('data2', 'sentence2');
SELECT a FROM t('data1');
SELECT a FROM t WHERE t MATCH 'data*';
SELECT a FROM t('data*') ORDER BY rank;
SELECT a FROM t WHERE t = 'data1';
SELECT a FROM t('sentence1');
SELECT a FROM t('data1 OR sentence2') ORDER BY rowid;
SELECT a, rank FROM t('b:sentence2');
SELECT count(*) FROM t WHERE a = 'data1';
INSERT INTO t(t, rank) VALUES ('rank', 'bm25(10.0,1.0)');
SELECT a FROM t('data*') ORDER BY rank;
SELECT x.a, y.v FROM (SELECT 'data2' AS v) y JOIN t x ON x.t = y.v;

-- case: bug-hunt/table-functions/fts5-external-content-trigger-pattern
-- the delete command, as SQLite documents it for external content triggers
CREATE TABLE tbl(a INTEGER PRIMARY KEY, b, c);
CREATE VIRTUAL TABLE fts_idx USING fts5(b, c, content='tbl', content_rowid='a');
CREATE TRIGGER tbl_ai AFTER INSERT ON tbl BEGIN
  INSERT INTO fts_idx(rowid, b, c) VALUES (new.a, new.b, new.c);
END;
CREATE TRIGGER tbl_ad AFTER DELETE ON tbl BEGIN
  INSERT INTO fts_idx(fts_idx, rowid, b, c) VALUES('delete', old.a, old.b, old.c);
END;
CREATE TRIGGER tbl_au AFTER UPDATE ON tbl BEGIN
  INSERT INTO fts_idx(fts_idx, rowid, b, c) VALUES('delete', old.a, old.b, old.c);
  INSERT INTO fts_idx(rowid, b, c) VALUES (new.a, new.b, new.c);
END;
INSERT INTO tbl VALUES(1, 'hello world', 'one'), (2, 'goodbye world', 'two');
SELECT rowid FROM fts_idx WHERE fts_idx MATCH 'world' ORDER BY rowid;
UPDATE tbl SET b = 'hello there' WHERE a = 2;
SELECT rowid FROM fts_idx WHERE fts_idx MATCH 'goodbye';
SELECT rowid FROM fts_idx WHERE fts_idx MATCH 'there';
DELETE FROM tbl WHERE a = 1;
SELECT rowid FROM fts_idx WHERE fts_idx MATCH 'hello' ORDER BY rowid;
INSERT INTO fts_idx(fts_idx) VALUES('integrity-check');

-- case: bug-hunt/table-functions/fts5_rank_config_repeated
-- rank configured twice
CREATE VIRTUAL TABLE t USING fts5 (a, b);
INSERT INTO t (a, b) VALUES ('data1', 'sentence1'), ('data2', 'sentence2');
INSERT INTO t(t, rank) VALUES ('rank', 'bm25(10.0,1.0)');
SELECT a, b, rank FROM t('data*') ORDER BY RANK;
INSERT INTO t(t, rank) VALUES ('rank', 'bm25(10.0,1.0)');
SELECT a, b, rank FROM t('data*') ORDER BY RANK;

-- case: bug-hunt/table-functions/json_each_join_over_large_documents
-- a table function joined to a row with a large document it does not read
CREATE TABLE j(id INTEGER PRIMARY KEY, doc TEXT, small TEXT);
WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM c WHERE x<3000)
  INSERT INTO j VALUES(1, (SELECT json_group_array(x) FROM c), 'tiny');
INSERT INTO j VALUES(2, '[7,8]', 'two');
SELECT count(*), sum(e.value) FROM j, json_each(j.doc) e;
SELECT j.id, count(*) FROM j, json_each(j.doc) e WHERE e.value % 1000 = 0 GROUP BY j.id;
SELECT count(*), sum(e.value) FROM json_each('[1,2,3]') e, j;
SELECT e.value, j.small FROM json_each('[1,2]') e, j ORDER BY 1, 2;
SELECT j.id, e.value FROM j LEFT JOIN json_each(j.doc) e ON e.value = 8 ORDER BY 1;
SELECT count(*) FROM j AS a, json_each(a.doc) e, j AS b WHERE b.id = 2;
