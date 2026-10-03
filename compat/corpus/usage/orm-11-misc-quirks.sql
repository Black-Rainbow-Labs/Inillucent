-- case: orm/11/11.1-autoincrement-never-reuses-ids-but-plain-rowid-may-after-del
CREATE TABLE a(id INTEGER PRIMARY KEY, v); CREATE TABLE b(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO a(v) VALUES(1),(2); INSERT INTO b(v) VALUES(1),(2); DELETE FROM a WHERE id=2; DELETE FROM b WHERE id=2; INSERT INTO a(v) VALUES(3); INSERT INTO b(v) VALUES(3); SELECT id FROM a ORDER BY id; SELECT id FROM b ORDER BY id;
-- case: orm/11/11.2-rowid-reaches-max-then-picks-random-unused-value
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES(9223372036854775807,1); INSERT INTO t(v) VALUES(2); SELECT count(*), max(id) = 9223372036854775807 FROM t;
-- case: orm/11/11.3-autoincrement-at-max-rowid-raises-sqlite-full
CREATE TABLE t(id INTEGER PRIMARY KEY AUTOINCREMENT, v); INSERT INTO t VALUES(9223372036854775807,1); INSERT INTO t(v) VALUES(2);
-- case: orm/11/11.4-negative-rowid-insert-and-rowid-alias-names
CREATE TABLE t(a); INSERT INTO t(rowid,a) VALUES(-5,'x'); SELECT rowid, _rowid_, oid, a FROM t; INSERT INTO t(a) VALUES('y'); SELECT rowid FROM t ORDER BY rowid;
-- case: orm/11/11.5-order-by-rowid-default-scan-order-after-deletes-inserts-is-r
CREATE TABLE t(a); INSERT INTO t(rowid,a) VALUES(10,'a'),(2,'b'); SELECT a FROM t;
-- case: orm/11/11.6-select-without-order-by-follows-the-chosen-index
CREATE TABLE t(a,b); INSERT INTO t VALUES(3,'x'),(1,'y'),(2,'z'); CREATE INDEX i ON t(a); SELECT a FROM t; SELECT a,b FROM t;
-- case: orm/11/11.7-column-names-in-result-expression-text-alias-table-col
CREATE TABLE t(a,b); INSERT INTO t VALUES(1,2); SELECT a, b+1, t.a, a AS x, "a", upper('q'), t.* FROM t;
-- case: orm/11/11.8-result-column-name-for-a-and-a-0-and-subquery
CREATE TABLE t(a); INSERT INTO t VALUES(1); SELECT (a), a+0, (SELECT a), -a, a || 'x', CAST(a AS TEXT), count(*), NULL, 'lit', 1.50 FROM t;
-- case: orm/11/11.9-duplicate-result-column-names-get-suffix-with-full-column-na
CREATE TABLE a(id, v); CREATE TABLE b(id, w); INSERT INTO a VALUES(1,2); INSERT INTO b VALUES(1,3); SELECT * FROM a JOIN b ON a.id=b.id; SELECT a.id, b.id FROM a JOIN b ON a.id=b.id;
-- case: orm/11/11.10-subquery-in-from-column-names
SELECT x, y FROM (SELECT 1 AS x, 2 y); SELECT * FROM (SELECT 1, 2);
-- case: orm/11/11.11-double-equals-and-and-and
SELECT 1==1, 1=1, 1!=2, 1<>2, 2 !=2;
-- case: orm/11/11.12-bitwise-operators-and-shifts-including-negative-shift-and-sh
SELECT 5&3, 5|3, ~5, 1<<3, 16>>2, 1<<64, 1<<-1, -1>>70, 1<<63, 5&NULL;
-- case: orm/11/11.13-between-with-null-and-reversed-bounds
SELECT 2 BETWEEN 1 AND 3, 2 BETWEEN 3 AND 1, NULL BETWEEN 1 AND 3, 2 NOT BETWEEN 1 AND 3;
-- case: orm/11/11.14-in-with-empty-list-and-with-table-name-right-side-can-be-a-t
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); SELECT 1 IN (), 1 NOT IN (), 1 IN t, 5 IN t, 1 IN (SELECT a FROM t);
-- case: orm/11/11.15-string-literal-compare-vs-with-column-vs-literal-collation
SELECT 'a'='a', 'a' IS 'a', 'a' LIKE 'a';
-- case: orm/11/11.16-union-vs-union-all-ordering-and-dedupe-intersect-except
SELECT 1 UNION SELECT 1 UNION SELECT 2; SELECT 1 UNION ALL SELECT 1; SELECT 1 INTERSECT SELECT 1; SELECT 1 EXCEPT SELECT 1;
-- case: orm/11/11.17-compound-select-with-order-by-and-limit-applies-to-whole
SELECT 3 AS a UNION ALL SELECT 1 UNION ALL SELECT 2 ORDER BY a LIMIT 2;
-- case: orm/11/11.18-values-as-table-column1-column2-names
SELECT * FROM (VALUES (1,'a'),(2,'b')); SELECT column1 FROM (VALUES (7));
-- case: orm/11/11.19-generate-series-virtual-table-compile-option-absence-or-pres
SELECT count(*) FROM generate_series(1,5);
-- case: orm/11/11.20-triggers-new-old-when-update-of-recursive-triggers-default-o
CREATE TABLE t(a,b,upd); CREATE TRIGGER tr AFTER UPDATE OF a ON t BEGIN UPDATE t SET upd=coalesce(upd,0)+1 WHERE rowid=NEW.rowid; END; INSERT INTO t VALUES(1,1,NULL); UPDATE t SET a=2; UPDATE t SET b=5; SELECT * FROM t;
-- case: orm/11/11.21-instead-of-trigger-on-view-django-rails-views
CREATE TABLE t(a); CREATE VIEW v AS SELECT a FROM t; CREATE TRIGGER tr INSTEAD OF INSERT ON v BEGIN INSERT INTO t VALUES(NEW.a*2); END; INSERT INTO v VALUES(4); SELECT * FROM t;
-- case: orm/11/11.22-updatable-timestamp-trigger-updated-at-via-trigger-and-datet
CREATE TABLE t(id INTEGER PRIMARY KEY, v, u TEXT); CREATE TRIGGER tu AFTER UPDATE ON t BEGIN UPDATE t SET u=datetime('now') WHERE id=NEW.id; END; INSERT INTO t(v) VALUES(1); UPDATE t SET v=2; SELECT u IS NOT NULL FROM t;
-- case: orm/11/11.23-partial-unique-index-soft-delete-allows-reuse-after-deletion
CREATE TABLE u(id INTEGER PRIMARY KEY, email TEXT, deleted_at TEXT); CREATE UNIQUE INDEX ue ON u(email) WHERE deleted_at IS NULL; INSERT INTO u(email) VALUES('a'); UPDATE u SET deleted_at='now' WHERE id=1; INSERT INTO u(email) VALUES('a'); INSERT INTO u(email) VALUES('a');
-- case: orm/11/11.24-expression-index-used-for-lower-email-lookups-query-plan-tex
CREATE TABLE u(email TEXT); CREATE INDEX il ON u(lower(email)); EXPLAIN QUERY PLAN SELECT * FROM u WHERE lower(email)='a';
-- case: orm/11/11.25-explain-query-plan-columns-and-text-for-pk-lookup-and-scan
CREATE TABLE t(id INTEGER PRIMARY KEY, a); CREATE INDEX ia ON t(a); EXPLAIN QUERY PLAN SELECT * FROM t WHERE id=1; EXPLAIN QUERY PLAN SELECT * FROM t WHERE a=1; EXPLAIN QUERY PLAN SELECT * FROM t; EXPLAIN QUERY PLAN SELECT a FROM t WHERE a>1 ORDER BY a;
-- case: orm/11/11.26-covering-index-plan-and-automatic-index
CREATE TABLE a(x, y); CREATE TABLE b(x, z); EXPLAIN QUERY PLAN SELECT * FROM a JOIN b ON a.x=b.x;
-- case: orm/11/11.27-index-on-column-with-different-collation-not-used-plan
CREATE TABLE t(a TEXT COLLATE NOCASE); CREATE INDEX i ON t(a); EXPLAIN QUERY PLAN SELECT * FROM t WHERE a='x'; EXPLAIN QUERY PLAN SELECT * FROM t WHERE a='x' COLLATE BINARY;
-- case: orm/11/11.28-like-optimisation-needs-nocase-index-or-case-sensitive-like-
CREATE TABLE t(a TEXT); CREATE INDEX i ON t(a); EXPLAIN QUERY PLAN SELECT * FROM t WHERE a LIKE 'ab%'; EXPLAIN QUERY PLAN SELECT * FROM t WHERE a GLOB 'ab*';
-- case: orm/11/11.29-attach-database-and-schema-qualified-names
ATTACH DATABASE ':memory:' AS aux; CREATE TABLE aux.t(a); INSERT INTO aux.t VALUES(1); SELECT * FROM aux.t; SELECT name FROM aux.sqlite_master; SELECT seq, name, file LIKE '%case.%' FROM pragma_database_list; DETACH aux;
-- case: orm/11/11.30-attach-inside-a-transaction-succeeds-detach-inside-it-fails-
BEGIN; ATTACH ':memory:' AS aux; CREATE TABLE aux.t(a); DETACH aux; ROLLBACK;
-- case: orm/11/11.31-temp-tables-shadow-main-tables-of-the-same-name
CREATE TABLE t(a); INSERT INTO t VALUES(1); CREATE TEMP TABLE t(a); INSERT INTO t VALUES(2); SELECT * FROM t; SELECT * FROM main.t;
-- case: orm/11/11.32-sqlite-version-changes-and-select-1-type-with-no-from
SELECT 1, 'a', NULL, 1.5, x'00';
-- case: orm/11/11.33-empty-statement-and-comment-only-produce-no-rows
; -- comment
 /* block */ ; SELECT 1 /* inline */ + 2 -- tail
;
-- case: orm/11/11.34-concurrent-semicolon-in-string-and-begin-end-trigger-splitti
CREATE TABLE t(a); CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 'a;b'; SELECT 2; END; INSERT INTO t VALUES('x;y'); SELECT a FROM t;
-- case: orm/11/11.35-large-literal-sqlite-max-length-and-1000000-char-text-length
SELECT length(printf('%.*c', 1000000, 'x')), length(hex(zeroblob(1000)));
-- case: orm/11/11.36-analyze-then-pragma-optimize-returns-no-rows
CREATE TABLE t(a); PRAGMA optimize;
-- case: orm/11/11.37-self-join-of-table-with-no-alias-errors
CREATE TABLE t(a); SELECT * FROM t, t;
-- case: orm/11/11.38-using-an-alias-of-the-select-list-in-where-works-sqlite-exte
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); SELECT a*2 AS d FROM t WHERE d>2;
-- case: orm/11/11.39-group-by-on-expression-with-having-referring-to-alias
CREATE TABLE t(a); INSERT INTO t VALUES(1),(1),(2); SELECT a AS k, count(*) AS n FROM t GROUP BY k HAVING n>1;
-- case: orm/11/11.40-empty-string-vs-null-is-null-length-and-oracle-compat-none
SELECT '' IS NULL, length(''), ''=NULL, coalesce('','x'), NULL||'a' IS NULL;
-- case: orm/11/11.41-unary-minus-on-text-and-on-min-int-negative-zero-string-outp
SELECT -'5', -'a', -(-9223372036854775808), - -3, +'x', typeof(-'5');
-- case: orm/11/11.42-dates-stored-in-integer-column-survive-2038-boundary
CREATE TABLE t(ts INTEGER); INSERT INTO t VALUES(2147483648), (4102444800); SELECT ts, datetime(ts,'unixepoch') FROM t;
