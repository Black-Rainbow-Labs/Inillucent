-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/transactions/60-a-check-failure-inside-begin-commit-only-undoes-th
CREATE TABLE t(val INT CHECK(val > 0));
BEGIN;
INSERT INTO t VALUES (10);
INSERT INTO t VALUES (20);
INSERT INTO t VALUES (-5);
COMMIT;
SELECT * FROM t;
-- case: holdout/transactions/102-changes-inside-an-explicit-transaction-reflects-th
CREATE TABLE t(id INTEGER PRIMARY KEY, val INTEGER);
INSERT INTO t VALUES(1,10),(2,20),(3,30);
BEGIN;
DELETE FROM t WHERE val > 15;
SELECT changes();
COMMIT;
BEGIN;
INSERT INTO t VALUES(4,40),(5,50);
SELECT changes();
COMMIT;
-- case: holdout/transactions/665-vacuum-into-takes-an-expression
CREATE TABLE p(n TEXT) STRICT;
INSERT INTO p VALUES(':memory:');
VACUUM INTO (SELECT n FROM p);
VACUUM INTO ':memory:';
VACUUM INTO 'a' || 'b';
-- case: holdout/transactions/736-a-failed-statement-inside-a-transaction-does-not-s
CREATE TABLE t(a); INSERT INTO t VALUES (1),(2),(3);
CREATE TABLE u(b UNIQUE); INSERT INTO u VALUES (10);
BEGIN;
INSERT INTO u VALUES (20), (10);
SELECT count(*) FROM u;
SELECT a FROM t;
COMMIT;
SELECT * FROM u;
