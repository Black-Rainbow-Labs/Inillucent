-- case: orm/4/4.1-insert-returning-id
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t(v) VALUES('a') RETURNING id; INSERT INTO t(v) VALUES('b'),('c') RETURNING id, v;
-- case: orm/4/4.2-insert-returning-and-expression-and-alias
CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT DEFAULT 'd'); INSERT INTO t DEFAULT VALUES RETURNING *; INSERT INTO t(v) VALUES('x') RETURNING id AS new_id, upper(v) AS u, id*10;
-- case: orm/4/4.3-update-returning-sees-new-values
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); INSERT INTO t VALUES(1,10),(2,20); UPDATE t SET n=n+1 WHERE id<=2 RETURNING id, n;
-- case: orm/4/4.4-delete-returning
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); INSERT INTO t VALUES(1,10),(2,20); DELETE FROM t WHERE n>10 RETURNING *; SELECT count(*) FROM t;
-- case: orm/4/4.5-returning-with-order-by-on-insert-select-is-unordered-return
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); INSERT INTO t(n) VALUES(1) RETURNING count(*);
-- case: orm/4/4.6-returning-cannot-reference-other-tables-subquery-in-column-l
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); CREATE TABLE u(x); INSERT INTO t(n) VALUES(1) RETURNING u.x;
-- case: orm/4/4.7-returning-from-a-statement-with-an-after-trigger-shows-pre-t
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); CREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t SET n=99 WHERE id=NEW.id; END; INSERT INTO t(n) VALUES(1) RETURNING n; SELECT n FROM t;
-- case: orm/4/4.8-upsert-do-update-with-excluded
CREATE TABLE t(k TEXT PRIMARY KEY, v INT); INSERT INTO t VALUES('a',1); INSERT INTO t VALUES('a',5) ON CONFLICT(k) DO UPDATE SET v=excluded.v+t.v RETURNING *;
-- case: orm/4/4.9-upsert-do-nothing-returns-no-returning-row
CREATE TABLE t(k TEXT PRIMARY KEY, v INT); INSERT INTO t VALUES('a',1); INSERT INTO t VALUES('a',5) ON CONFLICT DO NOTHING RETURNING *; SELECT * FROM t;
-- case: orm/4/4.10-upsert-target-must-match-a-unique-index
CREATE TABLE t(k TEXT, v INT); INSERT INTO t VALUES('a',1) ON CONFLICT(k) DO NOTHING;
-- case: orm/4/4.11-upsert-with-where-on-do-update
CREATE TABLE t(k PRIMARY KEY, v INT); INSERT INTO t VALUES('a',5); INSERT INTO t VALUES('a',3) ON CONFLICT(k) DO UPDATE SET v=excluded.v WHERE excluded.v>t.v RETURNING v; SELECT v FROM t;
-- case: orm/4/4.12-upsert-parse-ambiguity-select-source-needs-where-true
CREATE TABLE t(k PRIMARY KEY, v); CREATE TABLE s(k, v); INSERT INTO s VALUES('a',1); INSERT INTO t SELECT k,v FROM s ON CONFLICT(k) DO NOTHING;
-- case: orm/4/4.13-upsert-with-where-true-on-select-source-works
CREATE TABLE t(k PRIMARY KEY, v); CREATE TABLE s(k, v); INSERT INTO s VALUES('a',1),('a',2); INSERT INTO t SELECT k,v FROM s WHERE true ON CONFLICT(k) DO UPDATE SET v=excluded.v; SELECT * FROM t;
-- case: orm/4/4.14-upsert-multiple-on-conflict-clauses-3-35
CREATE TABLE t(a UNIQUE, b UNIQUE, c); INSERT INTO t VALUES(1,1,'x'); INSERT INTO t VALUES(1,2,'y') ON CONFLICT(a) DO UPDATE SET c='A' ON CONFLICT(b) DO UPDATE SET c='B'; SELECT * FROM t;
-- case: orm/4/4.15-upsert-on-partial-unique-index-needs-matching-where-in-targe
CREATE TABLE t(email TEXT, deleted INT); CREATE UNIQUE INDEX ux ON t(email) WHERE deleted=0; INSERT INTO t VALUES('a',0); INSERT INTO t VALUES('a',0) ON CONFLICT(email) WHERE deleted=0 DO UPDATE SET email='b'; SELECT * FROM t;
-- case: orm/4/4.16-upsert-target-without-index-where-for-partial-index-fails
CREATE TABLE t(email TEXT, deleted INT); CREATE UNIQUE INDEX ux ON t(email) WHERE deleted=0; INSERT INTO t VALUES('a',0); INSERT INTO t VALUES('a',0) ON CONFLICT(email) DO NOTHING;
-- case: orm/4/4.17-insert-or-replace-deletes-then-inserts-new-rowid-for-no-expl
CREATE TABLE t(k TEXT UNIQUE, v INT); INSERT INTO t VALUES('a',1); SELECT rowid FROM t; INSERT OR REPLACE INTO t VALUES('a',2); SELECT rowid,* FROM t;
-- case: orm/4/4.18-insert-or-ignore-counts-and-constraint-skipping
CREATE TABLE t(k UNIQUE, n NOT NULL); INSERT OR IGNORE INTO t VALUES(1,1),(1,2),(2,NULL),(3,3); SELECT * FROM t; SELECT changes();
-- case: orm/4/4.19-replace-into-shorthand
CREATE TABLE t(k PRIMARY KEY, v); REPLACE INTO t VALUES(1,'a'); REPLACE INTO t VALUES(1,'b'); SELECT * FROM t;
-- case: orm/4/4.20-batched-multi-row-values-with-parameter-like-literals
CREATE TABLE t(a,b); INSERT INTO t VALUES(1,'x'),(2,'y'),(3,NULL); SELECT * FROM t ORDER BY a; SELECT changes(), total_changes();
-- case: orm/4/4.21-multi-row-values-with-different-arities-fails
CREATE TABLE t(a,b); INSERT INTO t VALUES(1,2),(3);
-- case: orm/4/4.22-insert-with-too-many-few-values-message
CREATE TABLE t(a,b); INSERT INTO t VALUES(1);
-- case: orm/4/4.23-insert-with-column-list-mismatch-message
CREATE TABLE t(a,b); INSERT INTO t(a,b) VALUES(1);
-- case: orm/4/4.24-insert-unknown-column-message
CREATE TABLE t(a,b); INSERT INTO t(a,zz) VALUES(1,2);
-- case: orm/4/4.25-insert-default-values
CREATE TABLE t(id INTEGER PRIMARY KEY, c TEXT DEFAULT 'q', d INT); INSERT INTO t DEFAULT VALUES; SELECT * FROM t;
-- case: orm/4/4.26-limit-offset-with-literals-negative-limit-offset-without-lim
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2),(3),(4),(5); SELECT a FROM t ORDER BY a LIMIT 2 OFFSET 1; SELECT a FROM t ORDER BY a LIMIT -1 OFFSET 3; SELECT a FROM t ORDER BY a LIMIT 2, 1; SELECT a FROM t ORDER BY a LIMIT 0;
-- case: orm/4/4.27-limit-with-non-integer-or-text-value
SELECT 1 LIMIT 'abc';
-- case: orm/4/4.28-limit-with-float-that-is-integral-works-fractional-errors
SELECT 1 LIMIT 1.0; SELECT 1 LIMIT 1.5;
-- case: orm/4/4.29-limit-expression-with-subquery
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2),(3); SELECT a FROM t ORDER BY a LIMIT (SELECT count(*)-1 FROM t);
-- case: orm/4/4.30-order-by-nulls-first-last-3-30-and-default-null-ordering
CREATE TABLE t(a); INSERT INTO t VALUES(2),(NULL),(1); SELECT a FROM t ORDER BY a; SELECT a FROM t ORDER BY a DESC; SELECT a FROM t ORDER BY a NULLS LAST; SELECT a FROM t ORDER BY a DESC NULLS FIRST;
-- case: orm/4/4.31-order-by-mixed-types-null-numbers-text-blob
CREATE TABLE t(a); INSERT INTO t VALUES('b'),(2),(NULL),(x'01'),(1.5),('a'); SELECT a, typeof(a) FROM t ORDER BY a;
-- case: orm/4/4.32-pagination-count-with-window-function-over
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2),(3); SELECT a, count(*) OVER () AS total FROM t ORDER BY a LIMIT 2;
-- case: orm/4/4.33-select-distinct-with-nulls-treats-nulls-equal
CREATE TABLE t(a); INSERT INTO t VALUES(NULL),(NULL),(1),(1); SELECT DISTINCT a FROM t ORDER BY a; SELECT count(DISTINCT a), count(a), count(*) FROM t;
-- case: orm/4/4.34-group-by-with-bare-column-picks-arbitrary-row-min-max-specia
CREATE TABLE t(g, v, w); INSERT INTO t VALUES(1,5,'a'),(1,9,'b'),(1,2,'c'); SELECT g, max(v), w FROM t GROUP BY g; SELECT g, min(v), w FROM t GROUP BY g;
-- case: orm/4/4.35-group-by-alias-and-positional
CREATE TABLE t(a,b); INSERT INTO t VALUES(1,1),(1,2),(2,3); SELECT a AS x, sum(b) FROM t GROUP BY x; SELECT a, sum(b) FROM t GROUP BY 1 ORDER BY 1;
-- case: orm/4/4.36-having-without-group-by-3-39
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); SELECT count(*) FROM t HAVING count(*)>1; SELECT count(*) FROM t HAVING count(*)>5;
-- case: orm/4/4.37-aggregates-on-empty-set-sum-null-total-0-0-count-0-max-null
CREATE TABLE t(a); SELECT sum(a), total(a), count(a), max(a), avg(a), group_concat(a) FROM t;
-- case: orm/4/4.38-sum-integer-overflow-raises-total-does-not
SELECT sum(x) FROM (SELECT 9223372036854775807 AS x UNION ALL SELECT 1); SELECT total(x) FROM (SELECT 9223372036854775807 AS x UNION ALL SELECT 1);
-- case: orm/4/4.39-group-concat-separator-and-order-null-skipped
CREATE TABLE t(a); INSERT INTO t VALUES('x'),(NULL),('y'); SELECT group_concat(a), group_concat(a,'-'), group_concat(a, NULL) FROM t;
-- case: orm/4/4.40-group-concat-order-by-and-string-agg-3-44
CREATE TABLE t(a); INSERT INTO t VALUES('b'),('c'),('a'); SELECT group_concat(a ORDER BY a DESC), string_agg(a, ',' ORDER BY a) FROM t;
-- case: orm/4/4.41-exists-and-in-subqueries-with-null-not-in-trap
SELECT 1 NOT IN (2, NULL), 1 IN (1, NULL), 1 IN (2, NULL), NULL IN (1), NULL NOT IN ();
-- case: orm/4/4.42-row-values-in-values-and-tuple-comparison
SELECT (1,2) < (1,3), (1,2) = (1,2), (1,NULL) = (1,2), (1,2) IN (VALUES (1,2),(3,4));
-- case: orm/4/4.43-joined-update-from-3-33
CREATE TABLE t(id INTEGER PRIMARY KEY, n INT); CREATE TABLE s(id INT, n INT); INSERT INTO t VALUES(1,0),(2,0); INSERT INTO s VALUES(1,10),(1,11); UPDATE t SET n=s.n FROM s WHERE t.id=s.id; SELECT * FROM t;
-- case: orm/4/4.44-update-with-limit-needs-sqlite-enable-update-delete-limit-de
CREATE TABLE t(a); UPDATE t SET a=1 LIMIT 1;
-- case: orm/4/4.45-delete-without-where-truncate-optimisation-changes
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2),(3); DELETE FROM t; SELECT changes(); SELECT count(*) FROM t;
-- case: orm/4/4.46-last-insert-rowid-after-multi-row-insert-and-after-failed-in
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE); INSERT INTO t(u) VALUES('a'),('b'); SELECT last_insert_rowid(); INSERT OR IGNORE INTO t(u) VALUES('a'); SELECT last_insert_rowid(), changes();
-- case: orm/4/4.47-last-insert-rowid-unchanged-by-update-and-by-trigger-scope
CREATE TABLE t(id INTEGER PRIMARY KEY, v); CREATE TABLE log(id INTEGER PRIMARY KEY, m); CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO log(m) VALUES('x'),('y'),('z'); END; INSERT INTO t(v) VALUES(1); SELECT last_insert_rowid(); SELECT count(*) FROM log;
-- case: orm/4/4.48-insert-select-from-same-table
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); INSERT INTO t SELECT a+10 FROM t; SELECT a FROM t ORDER BY a;
-- case: orm/4/4.49-cte-with-insert-and-recursive-generate
CREATE TABLE t(n); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x<5) INSERT INTO t SELECT x FROM c; SELECT sum(n), count(*) FROM t;
-- case: orm/4/4.50-cte-materialized-hints-parse
WITH a AS MATERIALIZED (SELECT 1 AS x), b AS NOT MATERIALIZED (SELECT 2 AS y) SELECT x, y FROM a, b;
-- case: orm/4/4.51-window-functions-row-number-running-sum-lag
CREATE TABLE t(g,v); INSERT INTO t VALUES('a',1),('a',2),('b',3),('a',4); SELECT g, v, row_number() OVER (PARTITION BY g ORDER BY v), sum(v) OVER (PARTITION BY g ORDER BY v), lag(v) OVER (ORDER BY v) FROM t ORDER BY v;
-- case: orm/4/4.52-window-default-frame-range-with-peers
CREATE TABLE t(v); INSERT INTO t VALUES(1),(1),(2); SELECT v, sum(v) OVER (ORDER BY v), sum(v) OVER (ORDER BY v ROWS UNBOUNDED PRECEDING), rank() OVER (ORDER BY v), dense_rank() OVER (ORDER BY v) FROM t;
-- case: orm/4/4.53-cannot-use-window-function-in-where
CREATE TABLE t(v); SELECT * FROM t WHERE row_number() OVER ()=1;
-- case: orm/4/4.54-cross-join-join-using-natural-join-result-columns
CREATE TABLE a(id, x); CREATE TABLE b(id, y); INSERT INTO a VALUES(1,'x'); INSERT INTO b VALUES(1,'y'); SELECT * FROM a JOIN b USING(id); SELECT * FROM a NATURAL JOIN b; SELECT * FROM a JOIN b ON a.id=b.id;
-- case: orm/4/4.55-right-and-full-outer-join-3-39
CREATE TABLE a(i); CREATE TABLE b(i); INSERT INTO a VALUES(1),(2); INSERT INTO b VALUES(2),(3); SELECT a.i,b.i FROM a RIGHT JOIN b ON a.i=b.i ORDER BY 2; SELECT a.i,b.i FROM a FULL JOIN b ON a.i=b.i ORDER BY coalesce(a.i,b.i);
-- case: orm/4/4.56-is-distinct-from-3-39-and-is-operator
SELECT 1 IS DISTINCT FROM NULL, NULL IS NOT DISTINCT FROM NULL, 1 IS 1, NULL IS NULL, NULL = NULL, 1 IS NOT NULL;
-- case: orm/4/4.57-ambiguous-column-name-error
CREATE TABLE a(id); CREATE TABLE b(id); SELECT id FROM a, b;
-- case: orm/4/4.58-no-such-table-no-such-column-messages
SELECT * FROM nope;
-- case: orm/4/4.59-no-such-column-message
CREATE TABLE t(a); SELECT zz FROM t;
-- case: orm/4/4.60-double-quoted-string-literal-fallback-dqs-when-column-missin
CREATE TABLE t(a); INSERT INTO t VALUES('x'); SELECT "zz" FROM t; SELECT "a" FROM t;
-- case: orm/4/4.61-misuse-of-aggregate-nested-aggregate
CREATE TABLE t(a); SELECT sum(sum(a)) FROM t;
-- case: orm/4/4.62-case-expression-with-null-and-different-types
SELECT CASE WHEN NULL THEN 1 ELSE 2 END, CASE 1 WHEN 1.0 THEN 'eq' END, CASE 'a' WHEN 'A' THEN 'ci' ELSE 'cs' END;
-- case: orm/4/4.63-iif-and-coalesce-ifnull-nullif
SELECT iif(1,'y','n'), iif(0,'y','n'), coalesce(NULL,NULL,3), ifnull(NULL,4), nullif(1,1), nullif(1,2);
