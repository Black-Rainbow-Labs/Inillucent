-- case: auto-vacuum/full-mode-survives-a-reopen-and-a-delete-gives-pages-back
PRAGMA auto_vacuum = FULL;
CREATE TABLE t(a INTEGER PRIMARY KEY, b BLOB);
WITH RECURSIVE c(a) AS (SELECT 1 UNION ALL SELECT a+1 FROM c WHERE a<2000) INSERT INTO t SELECT a, zeroblob(5000) FROM c;
PRAGMA auto_vacuum;
-- reopen
PRAGMA auto_vacuum;
DELETE FROM t WHERE a > 100;
PRAGMA freelist_count;
-- reopen
PRAGMA auto_vacuum;
PRAGMA freelist_count;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: auto-vacuum/incremental-mode-survives-a-reopen-and-a-full-vacuum-empties-the-freelist
PRAGMA auto_vacuum = INCREMENTAL;
CREATE TABLE t(a INTEGER PRIMARY KEY, b BLOB);
WITH RECURSIVE c(a) AS (SELECT 1 UNION ALL SELECT a+1 FROM c WHERE a<2000) INSERT INTO t SELECT a, zeroblob(5000) FROM c;
DELETE FROM t WHERE a > 100;
-- reopen
PRAGMA auto_vacuum;
SELECT freelist_count > 0 FROM pragma_freelist_count;
.output sink.txt
PRAGMA incremental_vacuum;
.output stdout
-- reopen
PRAGMA auto_vacuum;
PRAGMA freelist_count;
SELECT count(*) FROM t;
PRAGMA integrity_check;
-- case: auto-vacuum/a-mode-set-on-a-file-with-a-table-takes-effect-at-vacuum-and-survives-a-reopen
CREATE TABLE t(a INTEGER PRIMARY KEY, b BLOB);
WITH RECURSIVE c(a) AS (SELECT 1 UNION ALL SELECT a+1 FROM c WHERE a<500) INSERT INTO t SELECT a, zeroblob(5000) FROM c;
PRAGMA auto_vacuum = FULL;
PRAGMA auto_vacuum;
VACUUM;
PRAGMA auto_vacuum;
-- reopen
PRAGMA auto_vacuum;
DELETE FROM t;
PRAGMA freelist_count;
PRAGMA integrity_check;
