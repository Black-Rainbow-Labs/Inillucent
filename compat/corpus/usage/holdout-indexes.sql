-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/indexes/41-comparison-with-null-through-a-desc-index-returns-
create table t(x);
create index tx on t(x desc);
insert into t values (1),(2),(3);
select * from t where x > NULL;
select * from t where x < NULL;
select * from t where x = NULL;
select * from t where x is NULL;
-- case: holdout/indexes/54-partial-index-a-row-that-does-not-match-the-predic
CREATE TABLE t1(id INTEGER PRIMARY KEY, val INTEGER);
CREATE INDEX idx1 ON t1(val) WHERE val > 500;
INSERT INTO t1 VALUES(1, 100);
INSERT INTO t1 VALUES(2, 600);
PRAGMA integrity_check;
UPDATE t1 SET val = 700 WHERE id = 1;
UPDATE t1 SET val = 1 WHERE id = 2;
PRAGMA integrity_check;
-- case: holdout/indexes/55-expression-index-over-a-non-deterministic-function
CREATE TABLE t(x);
CREATE INDEX i ON t(sqlite_version());
CREATE INDEX i2 ON t(random());
CREATE INDEX i3 ON t(abs(x));
-- case: holdout/indexes/56-printf-q-in-an-expression-index-and-in-a-plain-que
CREATE TABLE t(x TEXT);
CREATE INDEX idx ON t(printf('%Q', x));
INSERT INTO t VALUES ('test'), ('it''s'), (NULL);
SELECT printf('%Q', x), printf('%q', x), printf('%w', x) FROM t;
-- case: holdout/indexes/58-partial-index-whose-where-has-like-escape-inside-o
CREATE TABLE t(x TEXT);
INSERT INTO t VALUES ('test'), ('a%'), ('b1');
CREATE INDEX i ON t(x) WHERE x LIKE 'a\%%' ESCAPE '\' OR x LIKE 'b%';
PRAGMA integrity_check;
SELECT x FROM t WHERE x LIKE 'b%';
-- case: holdout/indexes/70-range-on-the-second-column-of-a-composite-index-ex
CREATE TABLE t(a INT, b INT, c INT);
CREATE INDEX idx ON t(a, b, c);
INSERT INTO t VALUES (1, 1, 10), (1, NULL, 20), (1, 5, 30);
SELECT b, c FROM t WHERE a = 1 AND b < 3;
SELECT b, c FROM t WHERE a = 1 AND b > 0 ORDER BY b;
SELECT b, c FROM t WHERE a = 1 AND b IS NULL;
-- case: holdout/indexes/91-two-single-column-indexes-anded-inside-a-correlate
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER, grp TEXT);
CREATE INDEX idx_val ON t(val);
CREATE INDEX idx_grp ON t(grp);
INSERT INTO t VALUES (1, 10, 'A'), (2, 20, 'A'), (3, 30, 'B'), (4, 40, 'B'), (5, 50, 'C'), (6, 60, 'C');
SELECT COUNT(*) FROM t a WHERE EXISTS (SELECT 1 FROM t b WHERE b.grp = a.grp AND b.val = 30);
SELECT COUNT(*) FROM t a WHERE NOT EXISTS (SELECT 1 FROM t b WHERE b.grp = a.grp AND b.val = 30);
-- case: holdout/indexes/92-three-single-column-equalities-each-with-its-own-i
CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c INTEGER);
CREATE INDEX idx_a ON t(a);
CREATE INDEX idx_b ON t(b);
CREATE INDEX idx_c ON t(c);
INSERT INTO t VALUES (1, 10, 100, 1000);
SELECT * FROM t WHERE a = 10 AND b = 100;
SELECT * FROM t WHERE a = 10 AND b = 100 AND c = 1000;
SELECT * FROM t WHERE a = 10 OR b = 100 OR c = 1000;
-- case: holdout/indexes/98-composite-index-scan-with-a-range-on-the-second-co
CREATE TABLE v0 (c1 TEXT, c2 TEXT);
CREATE INDEX i1 ON v0 (c2, c1);
INSERT INTO v0 (c2) VALUES ('a'), ('a');
SELECT * FROM v0 WHERE c2 = 'a' AND c1 <= 999;
SELECT * FROM v0 WHERE c2 = 'a' AND c1 > -1;
SELECT count(*) FROM v0 WHERE c2 = 'a' AND c1 IS NULL;
-- case: holdout/indexes/104-partial-index-where-clauses-using-or-and-not-betwe
CREATE TABLE t(x);
INSERT INTO t VALUES(0), (9), (-9);
CREATE INDEX i ON t(x) WHERE x > 5 OR x < -5;
PRAGMA integrity_check;
SELECT x FROM t WHERE x > 5 OR x < -5 ORDER BY x;
CREATE TABLE t0(c0);
INSERT INTO t0 VALUES(1);
CREATE INDEX i0 ON t0(c0) WHERE c0 NOT BETWEEN 0 AND 0;
PRAGMA integrity_check;
-- case: holdout/indexes/129-unique-expression-index-plus-a-correlated-exists
CREATE TABLE t(b, c);
CREATE UNIQUE INDEX idx ON t(c, c + b);
INSERT INTO t VALUES (1, 2), (3, 4);
SELECT c + b FROM t WHERE EXISTS (SELECT 1 FROM t AS t2 WHERE t.b = t2.b);
-- case: holdout/indexes/131-between-inside-an-expression-index
CREATE TABLE t(a INTEGER, b INTEGER);
CREATE INDEX idx ON t(a BETWEEN 1 AND 10);
INSERT INTO t VALUES(5, 5);
INSERT INTO t VALUES(50, 5);
SELECT * FROM t WHERE (a BETWEEN 1 AND 10) = 1;
PRAGMA integrity_check;
-- case: holdout/indexes/200-indexed-by-a-partial-index-that-cannot-answer-the-
CREATE TABLE t(id INT, score INT, flag INT);
INSERT INTO t VALUES(1,10,0),(2,20,0),(3,30,1),(4,40,1),(5,50,0);
CREATE INDEX idx ON t(score) WHERE flag = 1;
SELECT COUNT(*) FROM t INDEXED BY idx;
SELECT id, score, flag FROM t INDEXED BY idx ORDER BY id;
SELECT id FROM t INDEXED BY idx WHERE flag = 1 AND score > 0 ORDER BY id;
SELECT id FROM t NOT INDEXED WHERE flag = 1 ORDER BY id;
-- case: holdout/indexes/239-drop-index-on-a-user-created-unique-index-and-on-a
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT, c UNIQUE);
CREATE UNIQUE INDEX ux_t_ab ON t(a, b);
DROP INDEX ux_t_ab;
DROP INDEX sqlite_autoindex_t_1;
SELECT name FROM sqlite_schema;
-- case: holdout/indexes/263-qualified-column-names-in-an-index-expression
CREATE TABLE t (a, b);
CREATE INDEX idx ON t (t.a + 1);
CREATE INDEX idx2 ON t (a + 1);
CREATE INDEX idx3 ON t ((SELECT 1));
CREATE INDEX idx4 ON t (a + ?1);
-- case: holdout/indexes/280-partial-index-where-with-current-timestamp
CREATE TABLE t(a);
INSERT INTO t VALUES(1),(2),(3);
CREATE INDEX i ON t(a) WHERE CURRENT_TIMESTAMP IS NOT NULL;
CREATE INDEX i2 ON t(a) WHERE random() > 0;
CREATE INDEX i3 ON t(a) WHERE a IN (SELECT 1);
CREATE INDEX i4 ON t(a) WHERE a > ?1;
SELECT type,name FROM sqlite_schema;
-- case: holdout/indexes/284-nulls-first-nulls-last-are-not-accepted-in-create-
CREATE TABLE t(a);
CREATE INDEX i ON t(a NULLS LAST);
CREATE INDEX i2 ON t(a DESC NULLS FIRST);
CREATE TABLE u(a PRIMARY KEY NULLS FIRST);
-- case: holdout/indexes/337-nulls-first-in-create-index-is-accepted-by-the-par
create table t(a);
create index idx on t(a nulls first);
select sql from sqlite_schema;
-- case: holdout/indexes/372-partial-index-predicate-usable-for-a-covering-scan
create table t(a, b);
create index idx on t(a) where b < 5;
insert into t values (-1, 1), (-3, 3), (-5, 5), (-8, 8);
select a from t where b < 5 order by a;
select a from t where b < 4 order by a;
-- case: holdout/indexes/373-indexed-by-a-partial-index-in-a-query-that-cannot-
create table t(a, b);
create index idx on t(a) where b < 5;
insert into t values (-1, 1), (-3, 3), (-5, 5), (-8, 8);
select a from t indexed by idx;
select a from t indexed by idx where b < 5 order by a;
-- case: holdout/indexes/379-mixed-case-index-and-constraint-names-are-stored-a
CREATE TABLE "t" ("id" TEXT PRIMARY KEY, "x" TEXT);
CREATE INDEX "t_x_idx" ON "t" ("x");
CREATE INDEX "tasks_projectID_idx" ON "t" ("x");
SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name;
SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'tasks_projectID_idx');
SELECT * FROM pragma_index_list('t') ORDER BY name;
-- case: holdout/indexes/387-drop-index-and-drop-table-remove-their-sqlite-stat
CREATE TABLE t(x);
CREATE INDEX i ON t(x);
INSERT INTO t VALUES(1),(2),(3);
ANALYZE;
DROP INDEX i;
SELECT count(*) FROM sqlite_stat1;
INSERT INTO t VALUES(4),(5),(6),(7),(8),(9);
CREATE INDEX i ON t(x);
SELECT stat FROM sqlite_stat1 WHERE idx='i';
DROP TABLE t;
ANALYZE;
SELECT count(*) FROM sqlite_stat1;
-- case: holdout/indexes/399-row-value-equality-uses-a-composite-index
CREATE TABLE loyalty (customer_id INT, store_id INT, points INT);
CREATE INDEX idx_loyalty ON loyalty(customer_id, store_id);
INSERT INTO loyalty VALUES (1,2,10),(1,3,20),(2,2,30);
EXPLAIN QUERY PLAN SELECT * FROM loyalty WHERE customer_id = 1 AND store_id = 2;
EXPLAIN QUERY PLAN SELECT * FROM loyalty WHERE (customer_id, store_id) = (1, 2);
SELECT * FROM loyalty WHERE (customer_id, store_id) = (1, 2);
SELECT * FROM loyalty WHERE (customer_id, store_id) > (1, 2) ORDER BY customer_id, store_id;
SELECT * FROM loyalty WHERE (customer_id, store_id) IN ((1,3),(2,2)) ORDER BY customer_id;
-- case: holdout/indexes/487-indexed-by-a-partial-index-when-the-where-implies-
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1, 5), (2, NULL), (3, -1);
CREATE INDEX ni ON t(a) WHERE b IS NOT NULL;
SELECT a FROM t INDEXED BY ni WHERE b = 5;
CREATE INDEX pi ON t(a) WHERE b > 0;
DELETE FROM t INDEXED BY pi;
SELECT * FROM t ORDER BY a;
DELETE FROM t INDEXED BY pi WHERE b > 0;
-- case: holdout/indexes/494-pragma-index-xinfo-name-of-the-rowid-entry-is-null
CREATE TABLE pk(a, b, PRIMARY KEY(a, b));
SELECT seqno, cid, name, typeof(name) FROM pragma_index_xinfo('sqlite_autoindex_pk_1') WHERE cid = -1;
SELECT * FROM pragma_index_xinfo('sqlite_autoindex_pk_1');
PRAGMA index_info(sqlite_autoindex_pk_1);
-- case: holdout/indexes/504-correlated-column-of-a-without-rowid-table-inside-
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
CREATE TABLE t3(a INTEGER, b INTEGER, c TEXT, PRIMARY KEY(a, b)) WITHOUT ROWID;
INSERT INTO t1 VALUES (1, 2, 'y'), (2, 5, 'xy'), (5, 3, 'z');
INSERT INTO t3 VALUES (1, 2, 'X'), (1, 3, 'X'), (2, 0, 'xy'), (3, 1, 'z'), (3, 2, 'y');
SELECT count(*) FROM t1 AS x JOIN t3 AS y ON x.b = y.b WHERE EXISTS (SELECT 1 FROM t3 AS w WHERE y.c IS NULL);
SELECT count(*) FROM t1 AS x JOIN t3 AS y ON x.b = y.b JOIN t1 AS z ON EXISTS (SELECT 1 FROM t1 AS w WHERE y.c IS NULL);
SELECT count(*) FROM t1 AS x JOIN t3 AS y ON x.b = y.b WHERE EXISTS (SELECT 1 FROM t3 AS w WHERE y.c = 'X');
-- case: holdout/indexes/516-without-rowid-table-whose-primary-key-is-not-the-f
CREATE TABLE t(v TEXT, k INTEGER PRIMARY KEY, w INTEGER) WITHOUT ROWID;
INSERT INTO t VALUES ('c',3,30),('a',1,10),('b',2,20);
SELECT k, v, w FROM t;
SELECT v FROM t WHERE k = 2;
SELECT * FROM t WHERE k > 1 ORDER BY k DESC;
PRAGMA index_info(sqlite_autoindex_t_1);
-- case: holdout/indexes/545-comparing-two-columns-of-the-same-table-y-x-must-n
CREATE TABLE t(x primary key, y);
INSERT INTO t VALUES (1, 5), (2, 1), (3, 10);
SELECT * FROM t WHERE y > x ORDER BY x;
SELECT * FROM t WHERE x < y ORDER BY x;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE y > x;
-- case: holdout/indexes/648-a-table-with-a-65536-byte-page-size-and-index-entr
PRAGMA page_size = 65536;
CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
CREATE INDEX idx ON t(b);
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 40) INSERT INTO t SELECT i, printf('%010000d', i) FROM n;
SELECT count(*), sum(length(b)) FROM t;
PRAGMA integrity_check;
DELETE FROM t WHERE a % 2 = 0;
PRAGMA integrity_check;
PRAGMA page_size;
-- case: holdout/indexes/659-partial-index-with-chained-between-expressions
CREATE TABLE t(x);
CREATE INDEX i ON t(x) WHERE x BETWEEN 0 AND CASE WHEN 1 THEN 1 END BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0 BETWEEN 0 AND 0;
INSERT INTO t VALUES(1);
PRAGMA integrity_check;
-- case: holdout/indexes/673-analyze-adds-sqlite-stat1-rows-only-for-the-indexe
CREATE TABLE t(id INTEGER PRIMARY KEY, val TEXT);
CREATE INDEX idx_val ON t(val);
INSERT INTO t VALUES(1, 'a'), (2, 'b'), (3, 'c');
CREATE TABLE u(x);
INSERT INTO u VALUES (1), (2);
ANALYZE;
SELECT tbl, COALESCE(idx, '<NULL>') as idx, stat FROM sqlite_stat1 ORDER BY tbl, idx;
-- case: holdout/indexes/697-secondary-index-on-a-without-rowid-table
CREATE TABLE catalog(tenant TEXT, item TEXT, score INTEGER, PRIMARY KEY(tenant, item)) WITHOUT ROWID;
CREATE INDEX catalog_score ON catalog(score, tenant);
INSERT INTO catalog VALUES ('a', 'x', 3), ('a', 'y', 1), ('b', 'z', 2);
PRAGMA integrity_check;
SELECT tenant, item, score FROM catalog ORDER BY score, tenant;
EXPLAIN QUERY PLAN SELECT tenant, item, score FROM catalog ORDER BY score, tenant;
SELECT item FROM catalog WHERE score = 2;
PRAGMA index_xinfo(catalog_score);
-- case: holdout/indexes/721-drop-index-of-a-mixed-case-index-name-in-any-case
CREATE TABLE entries (id TEXT PRIMARY KEY, legacy_lookup_value TEXT, replacement_value TEXT);
CREATE INDEX CustomerLookupMixedCase ON entries(legacy_lookup_value, id);
BEGIN TRANSACTION;
DROP INDEX "CustomerLookupMixedCase";
ALTER TABLE entries DROP COLUMN legacy_lookup_value;
COMMIT;
SELECT name FROM sqlite_schema ORDER BY name;
DROP INDEX customerlookupmixedcase;
-- case: holdout/indexes/724-pragma-max-page-count-and-index-balancing
PRAGMA page_size=512;
PRAGMA max_page_count=43;
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT);
CREATE INDEX idx ON t(k);
WITH RECURSIVE c(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM c WHERE x<96) INSERT INTO t SELECT x, printf('%080d',x) FROM c;
SELECT count(*) FROM t;
PRAGMA page_count;
PRAGMA integrity_check;
-- case: holdout/indexes/731-create-index-on-a-without-rowid-table
CREATE TABLE items (id TEXT PRIMARY KEY, value TEXT) WITHOUT ROWID;
CREATE INDEX items_value ON items(value);
INSERT INTO items VALUES ('a', 'x'), ('b', 'y');
SELECT id FROM items WHERE value = 'y';
CREATE UNIQUE INDEX items_value_u ON items(value);
PRAGMA integrity_check;
-- case: holdout/indexes/735-a-bound-parameter-or-the-literals-false-and-0-agai
CREATE TABLE receipts(queue TEXT, job_id TEXT, _deleted INTEGER DEFAULT 0);
CREATE UNIQUE INDEX active_receipts ON receipts(queue, job_id) WHERE _deleted = 0;
INSERT INTO receipts VALUES ('work', 'id', 0);
SELECT 1 FROM receipts WHERE queue = 'work' AND job_id = 'id' AND _deleted = false;
SELECT 1 FROM receipts WHERE queue = 'work' AND job_id = 'id' AND _deleted = 0;
EXPLAIN QUERY PLAN SELECT 1 FROM receipts WHERE queue = 'work' AND job_id = 'id' AND _deleted = 0;
EXPLAIN QUERY PLAN SELECT 1 FROM receipts WHERE queue = 'work' AND job_id = 'id' AND _deleted = false;
-- case: holdout/indexes/738-like-and-glob-with-a-literal-prefix-on-an-indexed-
CREATE TABLE t2(a INTEGER PRIMARY KEY, b, f TEXT);
CREATE INDEX t2f ON t2(f);
INSERT INTO t2(f) VALUES ('ghij1'), ('GHIJ2'), ('ghi'), ('ghik'), ('ghij');
SELECT f FROM t2 WHERE f GLOB 'ghij*' ORDER BY f;
SELECT f FROM t2 WHERE f LIKE 'ghij%' ORDER BY f;
EXPLAIN QUERY PLAN SELECT a FROM t2 WHERE f GLOB 'ghij*';
EXPLAIN QUERY PLAN SELECT a FROM t2 WHERE f LIKE 'ghij%';
PRAGMA case_sensitive_like = ON;
SELECT f FROM t2 WHERE f LIKE 'ghij%' ORDER BY f;
-- case: holdout/indexes/744-string-literals-inside-in-in-a-partial-index-where
CREATE TABLE issues (id TEXT PRIMARY KEY, status TEXT NOT NULL DEFAULT 'open', priority INTEGER NOT NULL DEFAULT 2, created_at TEXT NOT NULL DEFAULT '', ephemeral INTEGER NOT NULL DEFAULT 0, pinned INTEGER NOT NULL DEFAULT 0, is_template INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_issues_ready ON issues(status, priority, created_at) WHERE status IN ('open', 'in_progress') AND ephemeral = 0 AND pinned = 0 AND is_template = 0;
INSERT INTO issues(id) VALUES ('a');
SELECT id FROM issues WHERE status IN ('open', 'in_progress') AND ephemeral = 0 AND pinned = 0 AND is_template = 0;
PRAGMA integrity_check;
-- case: holdout/indexes/775-indexed-by-a-partial-index-whose-predicate-the-que
CREATE TABLE t(a, b);
INSERT INTO t VALUES (1, 1), (2, 0), (3, 1);
CREATE INDEX pi ON t(a) WHERE b = 1;
SELECT a FROM t INDEXED BY pi WHERE a > 0;
SELECT a FROM t INDEXED BY pi WHERE a > 0 AND b = 1 ORDER BY a;
-- case: holdout/indexes/795-natural-scan-order-of-a-without-rowid-table-follow
CREATE TABLE t(a PRIMARY KEY DESC) WITHOUT ROWID;
INSERT INTO t VALUES (2), (1), (3);
SELECT a FROM t;
CREATE TABLE u(a, b, PRIMARY KEY(a DESC, b ASC)) WITHOUT ROWID;
INSERT INTO u VALUES (1,2),(3,0),(1,1),(2,1);
SELECT a, b FROM u;
SELECT a, b FROM u WHERE a >= 2;
SELECT a, b FROM u ORDER BY a, b;
-- case: holdout/indexes/796-indexed-by-a-desc-index-scans-in-descending-order
CREATE TABLE t(a);
CREATE INDEX idx ON t(a DESC);
INSERT INTO t VALUES (2), (1), (3);
SELECT a FROM t INDEXED BY idx;
SELECT a FROM t INDEXED BY idx ORDER BY a;
SELECT a FROM t INDEXED BY idx WHERE a > 1;
SELECT a FROM t INDEXED BY idx WHERE a < 3 ORDER BY a DESC;
-- case: holdout/indexes/800-create-index-if-not-exists-with-a-name-used-by-a-t
CREATE TABLE t(a);
CREATE TABLE other(col);
CREATE VIEW vw AS SELECT 1 AS c;
CREATE INDEX IF NOT EXISTS t ON other(col);
CREATE INDEX IF NOT EXISTS vw ON other(col);
CREATE INDEX t ON other(col);
CREATE TABLE IF NOT EXISTS vw(a);
CREATE VIEW IF NOT EXISTS t AS SELECT 1;
CREATE TRIGGER IF NOT EXISTS t AFTER INSERT ON other BEGIN SELECT 1; END;
SELECT type, name FROM sqlite_master ORDER BY type, name;
-- case: holdout/indexes/809-index-info-and-index-xinfo-for-an-expression-index
CREATE TABLE t(name TEXT, b);
CREATE INDEX idx ON t(lower(name), b);
PRAGMA index_info('idx');
PRAGMA index_xinfo('idx');
SELECT seqno, cid, quote(name) FROM pragma_index_info('idx');
-- case: holdout/indexes/835-composite-unique-constraints-on-a-without-rowid-ta
CREATE TABLE t(a, b, c, d, PRIMARY KEY(a), UNIQUE(b, c)) WITHOUT ROWID;
INSERT INTO t VALUES (1, 1, 1, 'x');
INSERT INTO t VALUES (2, 1, 1, 'y');
INSERT INTO t VALUES (3, 1, 2, 'z');
SELECT * FROM t ORDER BY a;
SELECT d FROM t WHERE b = 1 AND c = 2;
PRAGMA integrity_check;
PRAGMA index_list(t);
PRAGMA index_xinfo(sqlite_autoindex_t_2);
-- case: holdout/indexes/836-pragma-index-list-and-index-xinfo-for-a-without-ro
CREATE TABLE w(a TEXT, b INT, PRIMARY KEY(a, b)) WITHOUT ROWID;
PRAGMA index_list(w);
PRAGMA index_info(sqlite_autoindex_w_1);
PRAGMA index_xinfo(sqlite_autoindex_w_1);
CREATE TABLE r(a TEXT, b INT, PRIMARY KEY(a, b));
PRAGMA index_list(r);
PRAGMA index_xinfo(sqlite_autoindex_r_1);
