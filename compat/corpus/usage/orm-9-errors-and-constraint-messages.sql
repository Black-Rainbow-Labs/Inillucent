-- case: orm/9/9.1-unique-constraint-failed-single-column
CREATE TABLE t(id INTEGER PRIMARY KEY, email TEXT UNIQUE); INSERT INTO t(email) VALUES('a'); INSERT INTO t(email) VALUES('a');
-- case: orm/9/9.2-unique-constraint-failed-composite-lists-all-columns
CREATE TABLE t(a,b,c, UNIQUE(a,b)); INSERT INTO t VALUES(1,2,3); INSERT INTO t VALUES(1,2,4);
-- case: orm/9/9.3-primary-key-violation-on-integer-primary-key-message
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES(1,1); INSERT INTO t VALUES(1,2);
-- case: orm/9/9.4-primary-key-violation-on-composite-key
CREATE TABLE t(a,b,PRIMARY KEY(a,b)); INSERT INTO t VALUES(1,1); INSERT INTO t VALUES(1,1);
-- case: orm/9/9.5-primary-key-on-without-rowid
CREATE TABLE t(a PRIMARY KEY, b) WITHOUT ROWID; INSERT INTO t VALUES(1,1); INSERT INTO t VALUES(1,2);
-- case: orm/9/9.6-unique-on-expression-index-message-uses-expression-text
CREATE TABLE t(a); CREATE UNIQUE INDEX i ON t(lower(a)); INSERT INTO t VALUES('A'); INSERT INTO t VALUES('a');
-- case: orm/9/9.7-unique-message-for-unique-index-on-multiple-columns-with-qua
CREATE TABLE "my table"("my col" TEXT UNIQUE); INSERT INTO "my table" VALUES('x'); INSERT INTO "my table" VALUES('x');
-- case: orm/9/9.8-not-null-constraint-failed
CREATE TABLE t(a NOT NULL); INSERT INTO t VALUES(NULL);
-- case: orm/9/9.9-not-null-on-update
CREATE TABLE t(a NOT NULL); INSERT INTO t VALUES(1); UPDATE t SET a=NULL;
-- case: orm/9/9.10-not-null-with-no-value-and-no-default
CREATE TABLE t(a INT NOT NULL, b INT); INSERT INTO t(b) VALUES(1);
-- case: orm/9/9.11-check-constraint-failed-unnamed-named-and-column-level
CREATE TABLE t(a INT CHECK(a>0), b INT, CONSTRAINT bpos CHECK(b>0), CHECK(a<b)); INSERT INTO t VALUES(-1,1); INSERT INTO t VALUES(1,-1); INSERT INTO t VALUES(5,2);
-- case: orm/9/9.12-check-is-null-tolerant-null-passes
CREATE TABLE t(a INT CHECK(a>0)); INSERT INTO t VALUES(NULL); SELECT count(*) FROM t;
-- case: orm/9/9.13-foreign-key-constraint-failed-has-no-table-name
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); INSERT INTO c VALUES(1);
-- case: orm/9/9.14-fk-on-parent-delete-restrict
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); INSERT INTO p VALUES(1); INSERT INTO c VALUES(1); DELETE FROM p;
-- case: orm/9/9.15-fk-on-delete-cascade-set-null-set-default
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c1(pid REFERENCES p(id) ON DELETE CASCADE); CREATE TABLE c2(pid REFERENCES p(id) ON DELETE SET NULL); CREATE TABLE c3(pid DEFAULT 9 REFERENCES p(id) ON DELETE SET DEFAULT); INSERT INTO p VALUES(1),(9); INSERT INTO c1 VALUES(1); INSERT INTO c2 VALUES(1); INSERT INTO c3 VALUES(1); DELETE FROM p WHERE id=1; SELECT count(*) FROM c1; SELECT pid FROM c2; SELECT pid FROM c3;
-- case: orm/9/9.16-fk-to-non-unique-parent-columns-foreign-key-mismatch
PRAGMA foreign_keys=ON; CREATE TABLE p(id, x); CREATE TABLE c(pid REFERENCES p(x)); INSERT INTO p VALUES(1,1); INSERT INTO c VALUES(1);
-- case: orm/9/9.17-fk-to-missing-parent-table-no-such-table-on-write
PRAGMA foreign_keys=ON; CREATE TABLE c(pid REFERENCES nope(id)); INSERT INTO c VALUES(1);
-- case: orm/9/9.18-fk-deferred-constraint-fails-only-at-commit
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED); BEGIN; INSERT INTO c VALUES(1); COMMIT;
-- case: orm/9/9.19-fk-deferred-satisfied-within-transaction
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED); BEGIN; INSERT INTO c VALUES(1); INSERT INTO p VALUES(1); COMMIT; SELECT * FROM c;
-- case: orm/9/9.20-fk-not-enforced-when-foreign-keys-off
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id)); INSERT INTO c VALUES(1); SELECT count(*) FROM c;
-- case: orm/9/9.21-syntax-error-message-near-token
SELEC 1;
-- case: orm/9/9.22-syntax-error-at-end-of-input
SELECT * FROM;
-- case: orm/9/9.23-unbalanced-parenthesis
SELECT (1;
-- case: orm/9/9.24-unrecognized-token
SELECT 1 # 2;
-- case: orm/9/9.25-unterminated-string-literal
SELECT 'abc;
-- case: orm/9/9.26-table-already-exists
CREATE TABLE t(a); CREATE TABLE t(a);
-- case: orm/9/9.27-index-already-exists-view-already-exists
CREATE TABLE t(a); CREATE INDEX i ON t(a); CREATE INDEX i ON t(a);
-- case: orm/9/9.28-no-such-function
SELECT nofunc(1);
-- case: orm/9/9.29-wrong-number-of-arguments-to-function
SELECT abs(1,2);
-- case: orm/9/9.30-wrong-number-of-arguments-for-substr-lower
SELECT lower();
-- case: orm/9/9.31-datatype-mismatch-on-rowid
CREATE TABLE t(a); INSERT INTO t(rowid) VALUES('x');
-- case: orm/9/9.32-string-or-blob-too-big-via-zeroblob-printf-limits
SELECT length(zeroblob(2000000000));
-- case: orm/9/9.33-attempt-to-write-a-readonly-database-via-query-only
CREATE TABLE t(a); PRAGMA query_only=ON; INSERT INTO t VALUES(1);
-- case: orm/9/9.34-raise-in-trigger-abort-fail-ignore-messages
CREATE TABLE t(a); CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a<0 BEGIN SELECT RAISE(ABORT,'negative not allowed'); END; INSERT INTO t VALUES(-1);
-- case: orm/9/9.35-raise-ignore-silently-skips-row
CREATE TABLE t(a); CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a<0 BEGIN SELECT RAISE(IGNORE); END; INSERT INTO t VALUES(-1),(1); SELECT * FROM t;
-- case: orm/9/9.36-raise-fail-keeps-earlier-rows-of-the-statement
CREATE TABLE t(a); CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a=3 BEGIN SELECT RAISE(FAIL,'stop'); END; INSERT INTO t VALUES(1),(2),(3),(4); SELECT * FROM t;
-- case: orm/9/9.37-raise-rollback-rolls-back-entire-transaction
CREATE TABLE t(a); CREATE TRIGGER tr BEFORE INSERT ON t WHEN NEW.a=3 BEGIN SELECT RAISE(ROLLBACK,'bye'); END; BEGIN; INSERT INTO t VALUES(1); INSERT INTO t VALUES(3);
-- case: orm/9/9.38-conflict-clause-on-conflict-in-column-constraints-rollback-i
CREATE TABLE t(a UNIQUE ON CONFLICT IGNORE, b); INSERT INTO t VALUES(1,1); INSERT INTO t VALUES(1,2); SELECT * FROM t;
-- case: orm/9/9.39-insert-or-fail-or-abort-or-rollback-statement-scoping
CREATE TABLE t(a UNIQUE); INSERT INTO t VALUES(1),(2); INSERT OR FAIL INTO t VALUES(3),(2),(4); SELECT a FROM t ORDER BY a; INSERT OR ABORT INTO t VALUES(5),(2); SELECT a FROM t ORDER BY a;
-- case: orm/9/9.40-insert-into-a-column-that-does-not-exist
CREATE TABLE t(a); INSERT INTO t(b) VALUES(1);
-- case: orm/9/9.41-update-unknown-column-message
CREATE TABLE t(a); UPDATE t SET zz=1;
-- case: orm/9/9.42-select-on-view-with-changed-table-error
CREATE TABLE t(a); CREATE VIEW v AS SELECT a FROM t; DROP TABLE t; SELECT * FROM v;
-- case: orm/9/9.43-insert-into-view-refused
CREATE TABLE t(a); CREATE VIEW v AS SELECT a FROM t; INSERT INTO v VALUES(1);
-- case: orm/9/9.44-cannot-drop-sqlite-internal-tables-modify
DROP TABLE sqlite_master;
-- case: orm/9/9.45-table-name-begins-with-sqlite-cannot-be-created
CREATE TABLE sqlite_x(a);
-- case: orm/9/9.46-too-many-columns-in-result-set-order-by-term-out-of-range
SELECT 1 ORDER BY 2;
-- case: orm/9/9.47-order-by-term-does-not-match-any-column-in-compound-select
SELECT 1 AS a UNION SELECT 2 ORDER BY b;
-- case: orm/9/9.48-selects-to-the-left-and-right-of-union-do-not-have-the-same-
SELECT 1,2 UNION SELECT 3;
-- case: orm/9/9.49-subquery-returns-more-than-1-column
SELECT (SELECT 1,2);
-- case: orm/9/9.50-row-value-misused
SELECT (1,2)=(1);
-- case: orm/9/9.51-no-tables-specified-distinct-aggregate-misuse
SELECT * ;
