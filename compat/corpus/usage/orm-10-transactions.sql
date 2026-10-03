-- case: orm/10/10.1-begin-inside-begin-error
BEGIN; BEGIN;
-- case: orm/10/10.2-commit-without-begin
COMMIT;
-- case: orm/10/10.3-rollback-without-begin
ROLLBACK;
-- case: orm/10/10.4-begin-immediate-exclusive-deferred-accepted
BEGIN IMMEDIATE; COMMIT; BEGIN EXCLUSIVE; END; BEGIN DEFERRED TRANSACTION; ROLLBACK;
-- case: orm/10/10.5-savepoint-nesting-and-rollback-to-keeps-savepoint-open
CREATE TABLE t(a); SAVEPOINT s1; INSERT INTO t VALUES(1); SAVEPOINT s2; INSERT INTO t VALUES(2); ROLLBACK TO s2; INSERT INTO t VALUES(3); RELEASE s1; SELECT * FROM t;
-- case: orm/10/10.6-savepoint-outside-transaction-starts-one-release-commits
CREATE TABLE t(a); SAVEPOINT s; INSERT INTO t VALUES(1); RELEASE s; ROLLBACK;
-- case: orm/10/10.7-rollback-to-unknown-savepoint
ROLLBACK TO nope;
-- case: orm/10/10.8-release-unknown-savepoint
RELEASE nope;
-- case: orm/10/10.9-savepoint-inside-begin-release-does-not-commit
CREATE TABLE t(a); BEGIN; SAVEPOINT s; INSERT INTO t VALUES(1); RELEASE s; ROLLBACK; SELECT count(*) FROM t;
-- case: orm/10/10.10-statement-error-inside-transaction-leaves-the-transaction-op
CREATE TABLE t(a UNIQUE); BEGIN; INSERT INTO t VALUES(1); INSERT INTO t VALUES(1); INSERT INTO t VALUES(2); COMMIT; SELECT a FROM t ORDER BY a;
-- case: orm/10/10.11-commit-with-pending-deferred-fk-violation-keeps-transaction-
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p(id) DEFERRABLE INITIALLY DEFERRED); BEGIN; INSERT INTO c VALUES(1); COMMIT; INSERT INTO p VALUES(1); COMMIT; SELECT * FROM c;
-- case: orm/10/10.12-autocommit-state-ddl-in-transaction-then-rollback
BEGIN; CREATE TABLE a(x); CREATE INDEX ai ON a(x); ROLLBACK; SELECT count(*) FROM sqlite_master;
-- case: orm/10/10.13-rollback-to-inside-a-transaction-keeps-the-earlier-insert-th
CREATE TABLE t(a); BEGIN; INSERT INTO t VALUES(1); SAVEPOINT s; INSERT INTO t VALUES(2); ROLLBACK TO s; COMMIT; SELECT count(*) FROM t;
-- case: orm/10/10.14-total-changes-and-changes-after-rollback
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); SELECT changes(), total_changes(); BEGIN; DELETE FROM t; ROLLBACK; SELECT changes(); SELECT count(*) FROM t;
-- case: orm/10/10.15-changes-not-updated-by-ddl-or-trigger-rows
CREATE TABLE t(a); CREATE TABLE l(a); CREATE TRIGGER tr AFTER INSERT ON t BEGIN INSERT INTO l VALUES(1),(2),(3); END; INSERT INTO t VALUES(1); SELECT changes(), total_changes();
-- case: orm/10/10.16-begin-immediate-on-a-read-only-connection
PRAGMA query_only=ON;
BEGIN IMMEDIATE;
COMMIT;
SELECT 1;
-- case: orm/10/10.17-wal-read-transaction-does-not-block-writer-checkpoint-row-sh
PRAGMA journal_mode=WAL; CREATE TABLE t(a); INSERT INTO t VALUES(1); PRAGMA wal_checkpoint(TRUNCATE); PRAGMA wal_checkpoint(TRUNCATE);
-- case: orm/10/10.20-pragma-data-version-changes-only-for-other-connections-commi
CREATE TABLE t(a); PRAGMA data_version; INSERT INTO t VALUES(1); PRAGMA data_version;
