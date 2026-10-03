# Triage of 146 usage script failures (sqlite3 3.53.4 versus inillucent-shell)

Source: baseline-failures.json. Case numbers are positions in that file (0 based). Outputs are abbreviated. "sqlite" is the pinned sqlite3 shell, "inillucent" is inillucent-shell.

## Summary

| Id | Severity | Title | Cases |
|---|---|---|---|
| H1 | high | ORDER BY on a TEXT column with an IN list returns rows in the wrong order | 1 |
| H2 | high | LIMIT -1 OFFSET n ignores the OFFSET | 1 |
| H3 | high | x IS TRUE / IS FALSE treat only the integer 1 as true (SQLite treats every nonzero number as true) | 1 |
| H4 | high | Comparison affinity wrong: a scalar subquery value loses its column affinity, and a compound SELECT column keeps an affinity it should lose | 2 |
| H5 | high | pragma_* table valued functions cannot be joined with correlated arguments, and pragma_foreign_key_check does not exist | 4 |
| H6 | high | ALTER TABLE RENAME TO does not rewrite REFERENCES in child tables, and leaves "p." column prefixes in views unchanged | 3 |
| H7 | high | ALTER TABLE RENAME COLUMN does not rewrite trigger bodies, nor REFERENCES parent(column) in child tables | 2 |
| H8 | high | INSTEAD OF UPDATE and INSTEAD OF DELETE triggers on views are not used (UPDATE or DELETE on a view is refused) | 1 |
| H9 | high | A recursive CTE with LIMIT or ORDER BY inside it is refused | 1 |
| M1 | medium | UPSERT conflict target on a partial index or an expression index is refused | 3 |
| M2 | medium | A result column alias cannot be used in WHERE | 1 |
| M3 | medium | Row value IN (subquery) is refused | 1 |
| M4 | medium | Aggregate and window function misuse: count() is refused, and some invalid statements reach an internal "new engine" error | 3 |
| M5 | medium | UPDATE visits rows in index order, so a statement that SQLite rejects with a UNIQUE failure succeeds | 1 |
| M6 | medium | PRAGMA foreign_keys inside a transaction is applied (SQLite ignores it); PRAGMA query_only + BEGIN IMMEDIATE starts a transaction | 3 |
| M7 | medium | PRAGMAs that SQLite accepts are refused or not stored (mmap_size, temp_store=file, wal_autocheckpoint, journal_size_limit, legacy_alter_table, synchronous=EXTRA, user_version overflow) | 7 |
| M8 | medium | ALTER TABLE ADD COLUMN accepts additions SQLite refuses | 3 |
| M9 | medium | ALTER TABLE DROP COLUMN: refuses drops SQLite allows, allows drops SQLite refuses, keeps a double space in the stored SQL, and uses different messages | 9 |
| M10 | medium | ALTER TABLE RENAME does not check dependents (views, triggers) the way SQLite does, and accepts a reserved name | 5 |
| M11 | medium | SQLite 3.53 ALTER TABLE syntax is missing: ALTER COLUMN SET/DROP NOT NULL and ADD CONSTRAINT CHECK/NOT NULL | 2 |
| M12 | medium | Statements that SQLite refuses are accepted | 8 |
| M13 | medium | Statements that SQLite accepts are refused, or give a wrong value | 6 |
| M14 | medium | Result column names: an expression column of a view or in pragma_table_info is empty, duplicate names are not suffixed | 3 |
| M15 | medium | pragma output differs from SQLite: index_list origin, foreign_key_list order and implicit parent column, function_list type, quoted type names, database_list seq, table_list schema | 9 |
| M16 | medium | Error message texts that ORMs and drivers match on differ from SQLite | 10 |
| M17 | medium | datetime: negative years, a +YYYY-MM-DD HH:MM:SS modifier and julianday of year -4713 return NULL | 2 |
| M18 | medium | json: an invalid path in json_extract is an error where SQLite returns NULL, and json_object with an odd argument count gives a parse error where SQLite gives a runtime error | 2 |
| L1 | low | Shell: quote mode prints text containing control characters raw; sqlite3 prints unistr() | 5 |
| L2 | low | Shell: error context lines (statement text and caret) are missing where SQLite prints them, and present where SQLite prints none | 9 |
| L3 | low | Shell: after a parse error SQLite abandons the rest of the line; with several runtime errors on one line SQLite prints only the last one | 4 |
| L4 | low | A runtime error discards rows that were already produced | 1 |
| L5 | low | A BEFORE INSERT trigger sees new.id as the assigned rowid, SQLite shows -1 | 1 |
| L6 | low | EXPLAIN QUERY PLAN text and plan choice differ | 3 |
| L7 | low | Other error message wording differs (same failure, different text) | 29 |
| L8 | low | PRAGMA wal_checkpoint(TRUNCATE) reports a non-zero log size after truncation | 1 |
| N1 | not a defect | UPDATE/DELETE ... LIMIT: the pinned sqlite3 shell is built without SQLITE_ENABLE_UPDATE_DELETE_LIMIT | 2 |
| N2 | not a defect | Engine configuration defaults: page_size (inillucent 32768, sqlite 4096) and cache_size default (-131072 against -2000). PRAGMA page_size=8192 before the first write is ignored by inillucent (decision needed whether it should be honoured) | 3 |
| N3 | not a defect | Harness: the database file name is .db for sqlite and .rdb for inillucent (PRAGMA database_list) | 1 |
| N4 | not a defect | Random output: randomblob(0) returns one random byte | 1 |
| N5 | not a defect | PRAGMA table_list without ORDER BY: row order is not specified (sqlite order is by name, inillucent order is not) | 1 |

One case can sit in several groups. Differences were compared statement by statement. Each case listed in a group has that difference, and may have others listed in other groups.

## H1 (high): ORDER BY on a TEXT column with an IN list returns rows in the wrong order

Cases (1): 0 affinity/index-on-text-column-numeric-lookup

Reproduction:
```
CREATE TABLE t(a TEXT); CREATE INDEX ta ON t(a); INSERT INTO t VALUES('1'),('01'),(1),('1.0');
SELECT a FROM t WHERE a IN (1, '01') ORDER BY a;
```
sqlite:
```
'a' / '01' / '1' / '1'
```
inillucent:
```
'a' / '1' / '1' / '01'  (the plan shows USE TEMP B-TREE FOR ORDER BY, so the sort itself is wrong: '1' sorts before '01'. The same query with the IN list written ('01', 1) returns the right order)
```

## H2 (high): LIMIT -1 OFFSET n ignores the OFFSET

Cases (1): 90 orm/4/4.26-limit-offset-with-literals-negative-limit-offset-without-lim

Reproduction:
```
SELECT a FROM t ORDER BY a LIMIT -1 OFFSET 3;   -- t holds 1..5
```
sqlite:
```
4, 5
```
inillucent:
```
1, 2, 3, 4, 5
```

## H3 (high): x IS TRUE / IS FALSE treat only the integer 1 as true (SQLite treats every nonzero number as true)

Cases (1): 1 affinity/boolean-literals

Reproduction:
```
SELECT 2 IS TRUE, 0.5 IS TRUE;
```
sqlite:
```
1,1
```
inillucent:
```
0,0   (same for a stored column value 2: b IS TRUE returns 0)
```

## H4 (high): Comparison affinity wrong: a scalar subquery value loses its column affinity, and a compound SELECT column keeps an affinity it should lose

Cases (2): 2 affinity/compare-subquery-column, 4 affinity/union-column-affinity

Reproduction:
```
CREATE TABLE t(a TEXT); INSERT INTO t VALUES('3'); SELECT (SELECT a FROM t) = 3;
CREATE TABLE u(b INTEGER); INSERT INTO u VALUES(1); INSERT INTO t VALUES('1');
SELECT x, typeof(x) FROM (SELECT a AS x FROM t UNION ALL SELECT b FROM u) WHERE x = 1;
```
sqlite:
```
1   /   one row: 1,'integer'   (a subquery value keeps the column affinity, so TEXT '3' = 3 is true; a compound SELECT whose arms have different affinities gives a column with no affinity, so text '1' = 1 is false)
```
inillucent:
```
0   /   two rows: '1','text' and 1,'integer'
```

## H5 (high): pragma_* table valued functions cannot be joined with correlated arguments, and pragma_foreign_key_check does not exist

Cases (4): 15 fk/foreign-key-check, 56 orm/2/2.20-rails-schema-dumper-pragma-index-list-joined-to-index-info, 57 orm/2/2.22-ef-core-pragma-table-info-joined-to-sqlite-master, 129 schema/table-info-pragmas

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, a, b); CREATE UNIQUE INDEX ia ON t(a, b DESC);
SELECT il.name, ii.seqno, ii.name FROM pragma_index_list('t') il JOIN pragma_index_info(il.name) ii;
```
sqlite:
```
'ia',0,'a' / 'ia',1,'b'
```
inillucent:
```
Error near line 3: the tree read for FROM term 1 does not carry column 3
Also SELECT * FROM sqlite_master m, pragma_table_info(m.name) fails the same way (cases 129 line 15 and 57, the form EF Core and Rails use).
Also SELECT * FROM pragma_foreign_key_check('c') gives: Parse error: no such table: pragma_foreign_key_check
```

## H6 (high): ALTER TABLE RENAME TO does not rewrite REFERENCES in child tables, and leaves "p." column prefixes in views unchanged

Cases (3): 5 alter/rename-table-updates-dependents, 61 orm/3/3.1-rename-table-updates-indexes-triggers-views-and-fk-reference, 63 orm/3/3.3-rename-with-foreign-keys-off-and-legacy-alter-table-off-stil

Reproduction:
```
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY, name); CREATE TABLE c(id INTEGER PRIMARY KEY, pid REFERENCES p(id));
CREATE VIEW pv AS SELECT p.name, c.id FROM p JOIN c ON c.pid = p.id; ALTER TABLE p RENAME TO parent;
SELECT sql FROM sqlite_master WHERE name IN ('c','pv'); INSERT INTO parent VALUES(1,'x'); SELECT * FROM pv; PRAGMA foreign_key_list(c);
```
sqlite:
```
c: CREATE TABLE c(... pid REFERENCES "parent"(id))
pv: CREATE VIEW pv AS SELECT "parent".name, c.id FROM "parent" JOIN c ON c.pid = "parent".id
the view returns 'x',1; foreign_key_list shows table 'parent', to 'id'
```
inillucent:
```
c keeps REFERENCES p(id); pv becomes SELECT p.name, c.id FROM "parent" JOIN c ON c.pid = p.id (only the FROM item was rewritten)
INSERT INTO parent fails: no such table: main.p ; SELECT * FROM pv fails: no such table: p ; foreign_key_list shows table 'p' and to NULL
Data impact: every child table and view that references the renamed table is broken after the rename.
```

## H7 (high): ALTER TABLE RENAME COLUMN does not rewrite trigger bodies, nor REFERENCES parent(column) in child tables

Cases (2): 6 alter/rename-column-updates-dependents, 14 alter/rename-column-in-fk-of-child

Reproduction:
```
CREATE TABLE t(a,b,c); CREATE TRIGGER tr AFTER UPDATE OF b ON t BEGIN UPDATE t SET c = new.b WHERE rowid = new.rowid; END;
ALTER TABLE t RENAME COLUMN b TO beta; INSERT INTO t VALUES(1,2,3); UPDATE t SET beta = 5; SELECT * FROM t;
-- second case:
PRAGMA foreign_keys=ON; CREATE TABLE p(id INTEGER PRIMARY KEY, code UNIQUE); CREATE TABLE c(pc REFERENCES p(code)); ALTER TABLE p RENAME COLUMN code TO kode;
```
sqlite:
```
trigger sql: ... UPDATE OF beta ... SET c = new.beta ...; the UPDATE gives 1,5,5; c's sql is REFERENCES p(kode) and inserts into c work
```
inillucent:
```
trigger sql keeps new.b, so the UPDATE fails at run time (no such column: b) and the row stays 1,2,3; c keeps REFERENCES p(code) and every INSERT into c fails: foreign key mismatch - "c" referencing "p"
```

## H8 (high): INSTEAD OF UPDATE and INSTEAD OF DELETE triggers on views are not used (UPDATE or DELETE on a view is refused)

Cases (1): 135 schema/instead-of-trigger-on-view

Reproduction:
```
CREATE TABLE t(a,b); CREATE VIEW v AS SELECT a,b FROM t; CREATE TRIGGER vu INSTEAD OF UPDATE OF b ON v BEGIN UPDATE t SET b = new.b WHERE a = old.a; END;
INSERT INTO t VALUES(10,'x'); UPDATE v SET b='z' WHERE a=10; SELECT * FROM t;
```
sqlite:
```
10,'z'
```
inillucent:
```
Parse error: unsupported: writing to a view (INSTEAD OF INSERT triggers do work); the DELETE FROM v case is not executed through the INSTEAD OF DELETE trigger either; t stays 10,'x'
```

## H9 (high): A recursive CTE with LIMIT or ORDER BY inside it is refused

Cases (1): 124 query/cte-forms

Reproduction:
```
WITH RECURSIVE fib(a,b) AS (SELECT 0,1 UNION ALL SELECT b,a+b FROM fib LIMIT 10) SELECT group_concat(a) FROM fib;
```
sqlite:
```
'0,1,1,2,3,5,8,13,21,34'
```
inillucent:
```
Parse error: unsupported: ORDER BY and LIMIT are not allowed on a recursive CTE
(the ORDER BY 2 DESC form in the same case fails the same way. The duplicate WITH name check is in M12.)
```

## M1 (medium): UPSERT conflict target on a partial index or an expression index is refused

Cases (3): 22 dml/upsert-partial-index-target, 23 dml/upsert-expression-index-target, 86 orm/4/4.15-upsert-on-partial-unique-index-needs-matching-where-in-targe

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, email TEXT, deleted INTEGER DEFAULT 0); CREATE UNIQUE INDEX live ON t(email) WHERE deleted=0;
INSERT INTO t(email) VALUES('a'); INSERT INTO t(email) VALUES('a') ON CONFLICT(email) WHERE deleted=0 DO UPDATE SET email='dup';
```
sqlite:
```
the row becomes 'dup'
```
inillucent:
```
Parse error: unsupported: a partial-index conflict target (expression index: "unsupported: an expression in a conflict target"); the row is not updated
```

## M2 (medium): A result column alias cannot be used in WHERE

Cases (1): 48 orm/11/11.38-using-an-alias-of-the-select-list-in-where-works-sqlite-exte

Reproduction:
```
CREATE TABLE t(a); INSERT INTO t VALUES(1),(2); SELECT a*2 AS d FROM t WHERE d>2;
```
sqlite:
```
'd' / 4
```
inillucent:
```
Parse error: no such column: d
```

## M3 (medium): Row value IN (subquery) is refused

Cases (1): 125 query/subqueries

Reproduction:
```
CREATE TABLE t(a,b); INSERT INTO t VALUES(1,'x'),(2,'y'),(3,'x'); SELECT a FROM t WHERE (a,b) IN (SELECT a,b FROM t WHERE a>1) ORDER BY a;
```
sqlite:
```
2 / 3
```
inillucent:
```
Parse error: unsupported: a row value IN a query rather than a value list
```

## M4 (medium): Aggregate and window function misuse: count() is refused, and some invalid statements reach an internal "new engine" error

Cases (3): 122 query/aggregate-errors, 92 orm/4/4.53-cannot-use-window-function-in-where, 123 query/window-functions

Reproduction:
```
CREATE TABLE t(a); SELECT count(); SELECT a FROM t GROUP BY sum(a); SELECT * FROM t WHERE row_number() OVER () = 1;
```
sqlite:
```
count() returns 1; GROUP BY sum(a): "aggregate functions are not allowed in the GROUP BY clause"; window in WHERE: "misuse of window function row_number()"
```
inillucent:
```
count(): wrong number of arguments to function count(); GROUP BY sum(a): Error: the new engine's physical pass does not handle the expression an aggregate yet; window in WHERE: Error: the new engine's physical pass does not handle the expression a WindowRef expression yet (internal text, wrong error class, no caret)
```

## M5 (medium): UPDATE visits rows in index order, so a statement that SQLite rejects with a UNIQUE failure succeeds

Cases (1): 24 dml/update-or-replace

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, u UNIQUE); INSERT INTO t VALUES(1,'a'),(2,'b'),(3,'c'); UPDATE OR REPLACE t SET u='a' WHERE id=3; UPDATE t SET id=id+1; SELECT * FROM t;
```
sqlite:
```
after UPDATE OR REPLACE: 2,'b' / 3,'a'; then UPDATE t SET id=id+1 fails: UNIQUE constraint failed: t.id (rows are visited in rowid order, 2->3 collides)
```
inillucent:
```
after UPDATE OR REPLACE: 3,'a' / 2,'b' (covering index scan order, legal without ORDER BY); then the UPDATE succeeds with ids 4 and 3 (rows visited in index order). SELECT order is not a defect by itself; the UPDATE visit order changes whether the statement succeeds.
```

## M6 (medium): PRAGMA foreign_keys inside a transaction is applied (SQLite ignores it); PRAGMA query_only + BEGIN IMMEDIATE starts a transaction

Cases (3): 30 orm/1/1.7-foreign-keys-on-is-a-no-op-inside-a-transaction, 74 orm/3/3.34-django-pragma-foreign-keys-off-refused-inside-transaction-si, 38 orm/10/10.16-begin-immediate-on-a-read-only-connection

Reproduction:
```
BEGIN; PRAGMA foreign_keys=ON; PRAGMA foreign_keys; COMMIT;
```
sqlite:
```
0 (setting inside a transaction does nothing)
```
inillucent:
```
1   (and PRAGMA foreign_keys=OFF inside a transaction after ON returns 0 where SQLite keeps 1; Django relies on this)
query_only: PRAGMA query_only=ON; BEGIN IMMEDIATE; COMMIT; gives in sqlite: Error cannot commit - no transaction is active (BEGIN IMMEDIATE did not start one); inillucent: no error
```

## M7 (medium): PRAGMAs that SQLite accepts are refused or not stored (mmap_size, temp_store=file, wal_autocheckpoint, journal_size_limit, legacy_alter_table, synchronous=EXTRA, user_version overflow)

Cases (7): 11 alter/rename-with-legacy-alter-table, 62 orm/3/3.2-rename-with-legacy-alter-table-on-leaves-fk-parent-reference, 29 orm/1/1.5-synchronous-default-and-normal-numeric-values, 32 orm/1/1.10-mmap-size-set-and-read-back, 33 orm/1/1.11-temp-store-codes, 34 orm/1/1.12-user-version-round-trip-signed-32-bit-wrap, 37 orm/1/1.17-journal-size-limit-and-wal-autocheckpoint

Reproduction:
```
PRAGMA mmap_size=268435456; PRAGMA wal_autocheckpoint=500; PRAGMA journal_size_limit=1048576; PRAGMA temp_store=1; PRAGMA legacy_alter_table=ON; PRAGMA synchronous=EXTRA; PRAGMA synchronous;
PRAGMA user_version=4294967295; PRAGMA user_version;
```
sqlite:
```
each setter succeeds and reads back (synchronous reads 3; user_version reads 0)
```
inillucent:
```
Error near line N: this engine's mmap_size is 0 and cannot be set to 268435456 (same wording for wal_autocheckpoint, journal_size_limit, legacy_alter_table); temp_store: "temp_store file is not available here; temporary tables live in memory"; synchronous=EXTRA reads back 2; user_version=4294967295 reads -1
Applications that run these PRAGMAs at connection start see a failed statement. Case 11 also differs because legacy_alter_table cannot be turned on: the view is rewritten to "p_old" where SQLite leaves it alone.
```

## M8 (medium): ALTER TABLE ADD COLUMN accepts additions SQLite refuses

Cases (3): 7 alter/add-column-rules, 65 orm/3/3.13-add-column-non-constant-default-current-timestamp-rejected, 66 orm/3/3.15-add-column-with-references-requires-null-default-when-fks-on

Reproduction:
```
ALTER TABLE t ADD COLUMN g DEFAULT CURRENT_TIMESTAMP;
PRAGMA foreign_keys=ON; ALTER TABLE t ADD COLUMN pid INT REFERENCES p(id) DEFAULT 5;
```
sqlite:
```
Error: Cannot add a column with non-constant default / Error: Cannot add a REFERENCES column with non-NULL default value
```
inillucent:
```
both succeed silently (existing rows get a computed timestamp)
```

## M9 (medium): ALTER TABLE DROP COLUMN: refuses drops SQLite allows, allows drops SQLite refuses, keeps a double space in the stored SQL, and uses different messages

Cases (9): 8 alter/drop-column-rules, 9 alter/drop-last-column-and-generated, 67 orm/3/3.18-drop-column-success-and-sqlite-master-text, 68 orm/3/3.19-drop-column-fails-for-primary-key, 69 orm/3/3.20-drop-column-fails-for-unique, 70 orm/3/3.21-drop-column-fails-when-indexed, 71 orm/3/3.23-drop-column-fails-when-a-table-level-check-uses-it, 72 orm/3/3.24-drop-column-succeeds-when-the-check-is-on-the-dropped-column, 73 orm/3/3.26-drop-column-fails-when-a-table-level-foreign-key-names-it

Reproduction:
```
CREATE TABLE t(a, b CHECK(b>0)); ALTER TABLE t DROP COLUMN b; SELECT sql FROM sqlite_master;
CREATE TABLE c(x, y, FOREIGN KEY(y) REFERENCES p(id)); ALTER TABLE c DROP COLUMN y;
CREATE TABLE t2(a, b, c); ALTER TABLE t2 DROP COLUMN b; SELECT sql FROM sqlite_master WHERE name='t2';
```
sqlite:
```
b with its own column CHECK drops: CREATE TABLE t(a); dropping y named by a table level FOREIGN KEY: Error: error in table c after drop column: unknown column "y" in foreign key definition; t2 text: CREATE TABLE t2(a, c)
```
inillucent:
```
b: Parse error: error in table t: cannot drop column "b" (not dropped); y: dropped silently; t2 text: CREATE TABLE t2(a,  c) (two spaces)
Message pairs (sqlite then inillucent): cannot drop PRIMARY KEY column: "a" / cannot drop column "a": PRIMARY KEY ; cannot drop UNIQUE column: "c" / cannot drop column "c": indexed ; error in index i after drop column: no such column: b (class Error) / cannot drop column "b": indexed (class Parse error) ; error in table t after drop column: no such column: b / error in table t: cannot drop column "b". Case 9, dropping a column that a generated column uses: error in table g after drop column: no such column: a / error in table g: cannot drop column "a". DROP COLUMN nosuch: no such column: "nosuch" with caret / no such column: nosuch.
```

## M10 (medium): ALTER TABLE RENAME does not check dependents (views, triggers) the way SQLite does, and accepts a reserved name

Cases (5): 10 alter/twelve-step-table-rebuild, 12 alter/rename-breaks-view-error, 13 alter/rename-errors, 64 orm/3/3.5-rename-column-to-existing-name-fails, 75 orm/3/3.35-rename-table-with-view-referencing-nonexistent-column-breaks

Reproduction:
```
CREATE TABLE t(a); CREATE VIEW v AS SELECT nosuch FROM t; ALTER TABLE t RENAME TO t2;
CREATE TABLE u(a); ALTER TABLE u RENAME TO sqlite_x; ALTER TABLE u RENAME COLUMN a TO a;
```
sqlite:
```
the first rename fails: Error: error in view v: no such column: nosuch (nothing renamed); sqlite_x: object name reserved for internal use: sqlite_x ; RENAME COLUMN a TO a is accepted silently
```
inillucent:
```
the rename succeeds (table is now t2, view broken); sqlite_x is accepted; RENAME COLUMN a TO a: duplicate column name: a
Case 10 is the standard twelve step rebuild (DROP TABLE book, ALTER TABLE new_book RENAME TO book) with a view over book. SQLite 3.53 refuses the RENAME ("error in view recent: no such table: main.book") and the script keeps a table named new_book; inillucent accepts and renames. Case 64: sqlite "error in table t after rename: duplicate column name: b" (class Error), inillucent "duplicate column name: b" (Parse error).
```

## M11 (medium): SQLite 3.53 ALTER TABLE syntax is missing: ALTER COLUMN SET/DROP NOT NULL and ADD CONSTRAINT CHECK/NOT NULL

Cases (2): 79 orm/3/3.45-alter-add-constraint-syntax-not-supported-3-53-adds-not-null, 80 orm/3/3.46-alter-table-alter-column-not-supported-prisma-django-emulate

Reproduction:
```
CREATE TABLE t(a); ALTER TABLE t ALTER COLUMN a SET NOT NULL;
```
sqlite:
```
accepted (no output)
```
inillucent:
```
Parse error: near "ALTER": syntax error
ADD CONSTRAINT c1 UNIQUE(a): both refuse, but sqlite reports near "UNIQUE" and inillucent near "CONSTRAINT" (error token and caret position differ).
```

## M12 (medium): Statements that SQLite refuses are accepted

Cases (8): 19 trigger/trigger-qualified-name-errors, 28 numbers/hex-literals, 47 orm/11/11.37-self-join-of-table-with-no-alias-errors, 78 orm/3/3.42-create-index-name-collision-with-table, 85 orm/4/4.12-upsert-parse-ambiguity-select-source-needs-where-true, 103 orm/8/8.5-unknown-collation-error, 124 query/cte-forms, 138 schema/default-expressions

Reproduction:
```
CREATE TABLE t(a); CREATE TRIGGER x AFTER INSERT ON t BEGIN INSERT INTO main.t VALUES(1); END;
SELECT * FROM t, t;    CREATE INDEX t ON t(a);    INSERT INTO t SELECT a FROM t ON CONFLICT(a) DO NOTHING;
CREATE TABLE u(a TEXT COLLATE UNICODE);    CREATE TABLE bad(a DEFAULT (nosuch));    CREATE TABLE bad2(a DEFAULT abs(1));
SELECT 0x10000000000000000;    WITH x AS (SELECT 1), x AS (SELECT 2) SELECT 1;
```
sqlite:
```
19: qualified table names are not allowed on INSERT, UPDATE, and DELETE statements within triggers ; 47: ambiguous column name: main.t.a ; 78: there is already a table named t ; 85: near "DO": syntax error ; 103: no such collation sequence: UNICODE ; 138: default value of column [a] is not constant, and near "(": syntax error for abs(1) ; 28: hex literal too big ; 124: duplicate WITH table name: x
```
inillucent:
```
all accepted. 28 prints the text '0x10000000000000000' then 0. 138 stores the invalid defaults. 103 creates a table with an unknown collation.
```

## M13 (medium): Statements that SQLite accepts are refused, or give a wrong value

Cases (6): 82 orm/3/3.51-vacuum-into-writes-a-copy, 145 tx/vacuum-into, 141 text/string-literal-forms, 142 tx/begin-modes-and-errors, 46 orm/11/11.35-large-literal-sqlite-max-length-and-1000000-char-text-length, 134 schema/views

Reproduction:
```
VACUUM INTO ':memory:'; SELECT 'a' 'b'; BEGIN TRANSACTION named; CREATE VIEW v3(x) AS SELECT a,b FROM t; SELECT length(printf('%.*c', 1000000, 'x'));
```
sqlite:
```
VACUUM INTO ':memory:' succeeds; 'a' 'b' parses (the second string is an alias); BEGIN TRANSACTION named parses; CREATE VIEW with a column list of the wrong width is created (error only when queried); printf returns 1000000 characters
```
inillucent:
```
Error: VACUUM could not create the file to rebuild into, :memory:: unable to open database file ; Parse error near "'b'": syntax error ; Parse error near "named": syntax error ; Parse error: expected 1 columns for v3 but got 2 ; length is 1 (printf ignores the * width)
```

## M14 (medium): Result column names: an expression column of a view or in pragma_table_info is empty, duplicate names are not suffixed

Cases (3): 49 orm/2/2.5-table-info-for-a-view-and-for-type-less-column, 128 query/column-names, 134 schema/views

Reproduction:
```
CREATE TABLE t(a,b); CREATE VIEW v2 AS SELECT a+b, count(*) FROM t GROUP BY 1; SELECT * FROM v2;
CREATE VIEW v AS SELECT upper(a) FROM t; PRAGMA table_info(v);  SELECT * FROM (SELECT a, a FROM t);
```
sqlite:
```
header 'a + b','count(*)' ; table_info name 'upper(a)' ; header 'a','a:1'
```
inillucent:
```
header '','' ; table_info name '' ; header 'a','a'
Case 134 also: DROP VIEW t on a table says "no such view: t" where sqlite says "use DROP TABLE to delete table t"
```

## M15 (medium): pragma output differs from SQLite: index_list origin, foreign_key_list order and implicit parent column, function_list type, quoted type names, database_list seq, table_list schema

Cases (9): 44 orm/11/11.29-attach-database-and-schema-qualified-names, 51 orm/2/2.7-index-list-origin-c-u-pk-and-partial-flag, 52 orm/2/2.9-foreign-key-list-with-actions-and-match, 53 orm/2/2.10-foreign-key-list-with-implicit-parent-column-gives-null-to, 59 orm/2/2.27-pragma-function-list-module-list-contain-core-items, 60 orm/2/2.28-alembic-reflection-column-type-strings-come-back-exactly-as-, 100 orm/6/6.21-python-sqlite3-parse-decltypes-column-decltype-retained-as-w, 129 schema/table-info-pragmas, 131 schema/table-list-pragma

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT UNIQUE); PRAGMA index_list(t);
CREATE TABLE p(id INTEGER PRIMARY KEY); CREATE TABLE c(pid REFERENCES p); PRAGMA foreign_key_list(c);
CREATE TABLE q(d "My Type", e [my type]); SELECT type FROM pragma_table_info('q');
CREATE TEMP TABLE tt(a); SELECT schema FROM pragma_table_list WHERE name='tt'; ATTACH ':memory:' AS aux; PRAGMA database_list;
```
sqlite:
```
index_list origin 'u' (UNIQUE constraint) ; foreign_key_list to=NULL ; types 'My Type' and 'my type' ; tt schema 'temp' ; aux has seq 2 ; function_list max/sum window entries type 'w' ; with two FKs on c the one declared second has id 0
```
inillucent:
```
origin 'pk' ; to='id' ; types '"My Type"' and '[my type]' (identifier quoting kept) ; tt schema 'main' ; aux seq 1 ; type 'a' ; foreign_key_list ids are in declaration order
```

## M16 (medium): Error message texts that ORMs and drivers match on differ from SQLite

Cases (10): 3 affinity/rowid-text-lookup, 95 orm/5/5.26-integer-primary-key-rejects-non-integer, 110 orm/9/9.31-datatype-mismatch-on-rowid, 40 orm/11/11.3-autoincrement-at-max-rowid-raises-sqlite-full, 88 orm/4/4.22-insert-with-too-many-few-values-message, 89 orm/4/4.24-insert-unknown-column-message, 112 orm/9/9.40-insert-into-a-column-that-does-not-exist, 105 orm/9/9.6-unique-on-expression-index-message-uses-expression-text, 114 orm/9/9.43-insert-into-view-refused, 87 orm/4/4.21-multi-row-values-with-different-arities-fails

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES('abc', 1);
CREATE TABLE a(x,y); INSERT INTO a VALUES(1); INSERT INTO a(zz) VALUES(1); INSERT INTO a VALUES(1,2),(3);
CREATE TABLE s(a); CREATE UNIQUE INDEX i ON s(lower(a)); INSERT INTO s VALUES('A'); INSERT INTO s VALUES('a');
CREATE TABLE u(id INTEGER PRIMARY KEY AUTOINCREMENT); INSERT INTO u VALUES(9223372036854775807); INSERT INTO u VALUES(NULL);
CREATE VIEW v AS SELECT 1; INSERT INTO v VALUES(1);
```
sqlite:
```
datatype mismatch ; table a has 2 columns but 1 values were supplied ; table a has no column named zz ; all VALUES must have the same number of terms ; UNIQUE constraint failed: index 'i' ; database or disk is full ; cannot modify v because it is a view
```
inillucent:
```
a rowid must be an integer, not Text([97, 98, 99]) (Rust debug text leaks; also Real(5.5) in case 3) ; 1 values for 2 columns ; no such column: zz ; unsupported: all VALUES rows must have the same width ; UNIQUE constraint failed:  (empty) ; u has handed out every AUTOINCREMENT key ; unsupported: writing to a view
A driver that maps by message text sees a different result code for datatype mismatch, the UNIQUE failure and database full.
```

## M17 (medium): datetime: negative years, a +YYYY-MM-DD HH:MM:SS modifier and julianday of year -4713 return NULL

Cases (2): 20 datetime/fixed-inputs, 21 datetime/invalid-and-edge-inputs

Reproduction:
```
SELECT datetime('2024-05-15','+0001-02-03 04:05:06'), date('-0001-01-01'), julianday('-4713-11-24 12:00:00');
```
sqlite:
```
'2025-07-18 04:05:06' / '-0001-01-01' / 0.0
```
inillucent:
```
NULL / NULL / NULL
```

## M18 (medium): json: an invalid path in json_extract is an error where SQLite returns NULL, and json_object with an odd argument count gives a parse error where SQLite gives a runtime error

Cases (2): 27 json/errors, 101 orm/7/7.11-json-object-odd-arguments-error

Reproduction:
```
SELECT json_extract('{"a":1}','$.a['); SELECT json_object('a');
```
sqlite:
```
NULL ; Error near line N: json_object() requires an even number of arguments
```
inillucent:
```
Error: bad JSON path: '$.a[' ; Parse error: wrong number of arguments to function json_object() (with caret)
```

## L1 (low): Shell: quote mode prints text containing control characters raw; sqlite3 prints unistr()

Cases (5): 54 orm/2/2.11-sqlite-master-rows-and-sql-text-preserved-verbatim, 102 orm/7/7.25-json-pretty-3-46, 130 schema/create-table-sql-preserved, 137 schema/create-table-as, 139 text/trim-family

Reproduction:
```
SELECT 'a'||char(10)||'b';
```
sqlite:
```
unistr('a\u000ab')
```
inillucent:
```
'a<newline>b' printed raw over two lines
```

## L2 (low): Shell: error context lines (statement text and caret) are missing where SQLite prints them, and present where SQLite prints none

Cases (9): 76 orm/3/3.36-create-table-if-not-exists-silent-when-exists-create-index-i, 77 orm/3/3.41-create-index-on-missing-column-message, 109 orm/9/9.26-table-already-exists, 130 schema/create-table-sql-preserved, 104 orm/8/8.6-unknown-collation-in-expression-error, 120 orm/9/9.50-row-value-misused, 140 text/collation-unknown-error, 79 orm/3/3.45-alter-add-constraint-syntax-not-supported-3-53-adds-not-null, 141 text/string-literal-forms

Reproduction:
```
CREATE TABLE t(a); CREATE TABLE t(a);   -- sqlite adds the SQL line and ^--- error here
SELECT 'a' < 'b' COLLATE nope;           -- inillucent adds the SQL line and caret, sqlite prints the message only
```
sqlite:
```
message plus caret for "already exists" and "no such column" in CREATE INDEX; no caret for no such collation sequence or row value misused
```
inillucent:
```
the reverse. Cases 12,17,75,80,87,101,116 to 127,135,142 also differ in caret lines, but only as a consequence of a difference listed in another group.
```

## L3 (low): Shell: after a parse error SQLite abandons the rest of the line; with several runtime errors on one line SQLite prints only the last one

Cases (4): 55 orm/2/2.17-like-escape-underscore-in-sqlite-matches-any-char-orm-gotcha, 93 orm/4/4.60-double-quoted-string-literal-fallback-dqs-when-column-missin, 106 orm/9/9.11-check-constraint-failed-unnamed-named-and-column-level, 111 orm/9/9.39-insert-or-fail-or-abort-or-rollback-statement-scoping

Reproduction:
```
CREATE TABLE t(a INT CHECK(a>0), b, CHECK(a<b)); INSERT INTO t VALUES(-1,1); INSERT INTO t VALUES(5,2);
SELECT "zz" FROM t; SELECT "a" FROM t;
```
sqlite:
```
one error line (CHECK constraint failed: a<b); the second SELECT is not run after the parse error
```
inillucent:
```
one error line per failing statement; the second SELECT runs and prints a row
Both are properties of the sqlite3 shell loop. They matter only for scripts that put several statements on one line.
```

## L4 (low): A runtime error discards rows that were already produced

Cases (1): 26 json/each-and-tree

Reproduction:
```
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM json_each(t.tags) WHERE value='blue') ORDER BY id;   -- the last row of t has malformed JSON
```
sqlite:
```
header and rows 1,2 printed, then Error: malformed JSON
```
inillucent:
```
only Error: malformed JSON
```

## L5 (low): A BEFORE INSERT trigger sees new.id as the assigned rowid, SQLite shows -1

Cases (1): 16 trigger/before-after-order-and-new-old

Reproduction:
```
CREATE TABLE t(id INTEGER PRIMARY KEY, a); CREATE TABLE log(m); CREATE TRIGGER b BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(quote(new.id)); END; INSERT INTO t(a) VALUES('x'); SELECT * FROM log;
```
sqlite:
```
-1
```
inillucent:
```
1
```

## L6 (low): EXPLAIN QUERY PLAN text and plan choice differ

Cases (3): 41 orm/11/11.24-expression-index-used-for-lower-email-lookups-query-plan-tex, 42 orm/11/11.26-covering-index-plan-and-automatic-index, 43 orm/11/11.28-like-optimisation-needs-nocase-index-or-case-sensitive-like-

Reproduction:
```
CREATE INDEX il ON u(lower(email)); EXPLAIN QUERY PLAN SELECT * FROM u WHERE lower(email)=?;
a join where SQLite builds an automatic index; SELECT * FROM t WHERE a LIKE 'ab%' with an index on a
```
sqlite:
```
SEARCH u USING INDEX il (<expr>=?) ; BLOOM FILTER ON b (x=?) and SEARCH b USING AUTOMATIC COVERING INDEX (x=?) ; SEARCH t USING COVERING INDEX i (a>? AND a<?)
```
inillucent:
```
SEARCH u USING INDEX il (?=?) ; SCAN b ; SEARCH t USING COVERING INDEX i (a>?) (no upper bound)
```

## L7 (low): Other error message wording differs (same failure, different text)

Cases (29): 17 trigger/raise-forms, 18 trigger/drop-table-drops-triggers, 27 json/errors, 45 orm/11/11.30-attach-inside-a-transaction-succeeds-detach-inside-it-fails-, 81 orm/3/3.49-writable-schema-off-update-sqlite-master-refused, 83 orm/4/4.5-returning-with-order-by-on-insert-select-is-unordered-return, 84 orm/4/4.6-returning-cannot-reference-other-tables-subquery-in-column-l, 94 orm/4/4.61-misuse-of-aggregate-nested-aggregate, 96 orm/5/5.33-strict-unknown-type-name-rejected-datetime-not-allowed, 97 orm/5/5.35-strict-column-without-type-rejected, 98 orm/5/5.44-math-functions-need-sqlite-enable-math-functions, 107 orm/9/9.24-unrecognized-token, 108 orm/9/9.25-unterminated-string-literal, 113 orm/9/9.42-select-on-view-with-changed-table-error, 115 orm/9/9.44-cannot-drop-sqlite-internal-tables-modify, 116 orm/9/9.46-too-many-columns-in-result-set-order-by-term-out-of-range, 117 orm/9/9.47-order-by-term-does-not-match-any-column-in-compound-select, 118 orm/9/9.48-selects-to-the-left-and-right-of-union-do-not-have-the-same-, 119 orm/9/9.49-subquery-returns-more-than-1-column, 121 orm/9/9.51-no-tables-specified-distinct-aggregate-misuse, 122 query/aggregate-errors, 125 query/subqueries, 126 query/compound-and-order, 127 query/ambiguous-and-missing-columns, 132 schema/strict-tables, 133 schema/generated-columns, 136 schema/drop-and-if-exists, 143 tx/attach-and-cross-db, 144 tx/reindex-analyze-optimize

| Cases | Statement | sqlite | inillucent |
|---|---|---|---|
| 17 | RAISE(ABORT, ..) outside a trigger | RAISE() may only be used within a trigger-program | unsupported: RAISE outside a trigger |
| 18,136,143,113 | statement on a missing table | no such table: main.nosuch / aux.t / main.t (schema qualified) | no such table: nosuch / t |
| 27,101 | json_object('a') | json_object() requires an even number of arguments | wrong number of arguments to function json_object() |
| 45 | DETACH inside a transaction | database aux is locked | cannot DETACH database within transaction |
| 81 | UPDATE sqlite_master | table sqlite_master may not be modified | writing to sqlite_schema needs PRAGMA writable_schema = ON |
| 83,94,122 | misuse of aggregate | misuse of aggregate function count() / sum() | unsupported: misuse of aggregate function |
| 84 | RETURNING u.x | no such column: u.x | no such table: u |
| 96,97,132 | STRICT table errors | unknown datatype for t.a: "DATETIME" / missing datatype for t.a | unknown datatype for a: ... / missing datatype for a (table name missing); also case 131 |
| 98 | garbage input @@SELECT | unrecognized token: "@" | unrecognized token: malformed parameter (printed twice) |
| 107 | SELECT # | unrecognized token: "#" | unrecognized token |
| 108 | unterminated string | unrecognized token: "'abc;" | unrecognized token: unterminated quoted name |
| 115 | DROP TABLE sqlite_master | table sqlite_master may not be dropped | no such table: sqlite_master |
| 116,122,126 | ORDER BY / GROUP BY term out of range | 1st ORDER BY term out of range - should be between 1 and 1 ; 1st GROUP BY term out of range ... | 2th ORDER BY term out of range - should be between 1 and the number of result columns ; unsupported: GROUP BY term is out of range |
| 117,126 | ORDER BY term not in compound | 1st ORDER BY term does not match any column in the result set | ORDER BY term does not match any column in the result set |
| 118,126 | compound column count | SELECTs to the left and right of UNION do not have the same number of result columns | unsupported: SELECTs to the left and right of a compound operator do not have the same number of result columns |
| 119,125 | scalar subquery with 2 columns | sub-select returns 2 columns - expected 1 | unsupported: sub-select returns more than one column |
| 121 | SELECT * ; | no tables specified | near "*": syntax error |
| 127 | SELECT z.x FROM a / JOIN USING (nosuch) | no such column: z.x ; cannot join using column nosuch - column not present in both tables | no such table: z ; no such column: nosuch |
| 133 | generated column loop / PK | generated column loop on "c" (class Error) ; generated columns cannot be part of the PRIMARY KEY | generated column loop on b (Parse error) ; ...PRIMARY KEY: a |
| 144 | REINDEX nosuch | unable to identify the object to be reindexed | near "unable to identify the object to be reindexed: nosuch": syntax error |

## L8 (low): PRAGMA wal_checkpoint(TRUNCATE) reports a non-zero log size after truncation

Cases (1): 39 orm/10/10.17-wal-read-transaction-does-not-block-writer-checkpoint-row-sh

Reproduction:
```
PRAGMA journal_mode=WAL; CREATE TABLE t(a); INSERT INTO t VALUES(1); PRAGMA wal_checkpoint(PASSIVE); PRAGMA wal_checkpoint(TRUNCATE);
```
sqlite:
```
0,3,3 then 0,0,0
```
inillucent:
```
0,5,5 then 0,3,3 (frame counts depend on the WAL format, but TRUNCATE should report a log of 0)
```

## N1 (not a defect): UPDATE/DELETE ... LIMIT: the pinned sqlite3 shell is built without SQLITE_ENABLE_UPDATE_DELETE_LIMIT

Cases (2): 25 dml/delete-update-limit-refused, 91 orm/4/4.44-update-with-limit-needs-sqlite-enable-update-delete-limit-de

Reproduction:
```
DELETE FROM t ORDER BY a LIMIT 1;
```
sqlite:
```
near "ORDER": syntax error
```
inillucent:
```
executes
```

## N2 (not a defect): Engine configuration defaults: page_size (inillucent 32768, sqlite 4096) and cache_size default (-131072 against -2000). PRAGMA page_size=8192 before the first write is ignored by inillucent (decision needed whether it should be honoured)

Cases (3): 31 orm/1/1.9-cache-size-default-and-negative-kib-form, 35 orm/1/1.14-page-size-default-and-change-before-first-write, 36 orm/1/1.15-page-size-ignored-after-tables-exist-in-wal-needs-vacuum

Reproduction:
```
PRAGMA page_size; PRAGMA cache_size;
```
sqlite:
```
4096 ; -2000
```
inillucent:
```
32768 ; -131072
```

## N3 (not a defect): Harness: the database file name is .db for sqlite and .rdb for inillucent (PRAGMA database_list)

Cases (1): 58 orm/2/2.24-database-list-and-pragma-collation-list

Reproduction:
```
PRAGMA database_list;
```
sqlite:
```
...x.db
```
inillucent:
```
...x.rdb
```

## N4 (not a defect): Random output: randomblob(0) returns one random byte

Cases (1): 99 orm/5/5.45-random-and-randomblob-ranges

Reproduction:
```
SELECT randomblob(0);
```
sqlite:
```
x'49'
```
inillucent:
```
x'3f'
```

## N5 (not a defect): PRAGMA table_list without ORDER BY: row order is not specified (sqlite order is by name, inillucent order is not)

Cases (1): 50 orm/2/2.6-table-list-3-37

Reproduction:
```
PRAGMA table_list;
```
sqlite:
```
s, sqlite_schema, t, v, w
```
inillucent:
```
s, w, v, t, sqlite_schema
```

## Case to group table

| # | Case | Groups |
|---|---|---|
| 0 | affinity/index-on-text-column-numeric-lookup | H1 |
| 1 | affinity/boolean-literals | H3 |
| 2 | affinity/compare-subquery-column | H4 |
| 3 | affinity/rowid-text-lookup | M16 |
| 4 | affinity/union-column-affinity | H4 |
| 5 | alter/rename-table-updates-dependents | H6 |
| 6 | alter/rename-column-updates-dependents | H7 |
| 7 | alter/add-column-rules | M8 |
| 8 | alter/drop-column-rules | M9 |
| 9 | alter/drop-last-column-and-generated | M9 |
| 10 | alter/twelve-step-table-rebuild | M10 |
| 11 | alter/rename-with-legacy-alter-table | M7 |
| 12 | alter/rename-breaks-view-error | M10 |
| 13 | alter/rename-errors | M10 |
| 14 | alter/rename-column-in-fk-of-child | H7 |
| 15 | fk/foreign-key-check | H5 |
| 16 | trigger/before-after-order-and-new-old | L5 |
| 17 | trigger/raise-forms | L7 |
| 18 | trigger/drop-table-drops-triggers | L7 |
| 19 | trigger/trigger-qualified-name-errors | M12 |
| 20 | datetime/fixed-inputs | M17 |
| 21 | datetime/invalid-and-edge-inputs | M17 |
| 22 | dml/upsert-partial-index-target | M1 |
| 23 | dml/upsert-expression-index-target | M1 |
| 24 | dml/update-or-replace | M5 |
| 25 | dml/delete-update-limit-refused | N1 |
| 26 | json/each-and-tree | L4 |
| 27 | json/errors | M18, L7 |
| 28 | numbers/hex-literals | M12 |
| 29 | orm/1/1.5-synchronous-default-and-normal-numeric-values | M7 |
| 30 | orm/1/1.7-foreign-keys-on-is-a-no-op-inside-a-transaction | M6 |
| 31 | orm/1/1.9-cache-size-default-and-negative-kib-form | N2 |
| 32 | orm/1/1.10-mmap-size-set-and-read-back | M7 |
| 33 | orm/1/1.11-temp-store-codes | M7 |
| 34 | orm/1/1.12-user-version-round-trip-signed-32-bit-wrap | M7 |
| 35 | orm/1/1.14-page-size-default-and-change-before-first-write | N2 |
| 36 | orm/1/1.15-page-size-ignored-after-tables-exist-in-wal-needs-vacuum | N2 |
| 37 | orm/1/1.17-journal-size-limit-and-wal-autocheckpoint | M7 |
| 38 | orm/10/10.16-begin-immediate-on-a-read-only-connection | M6 |
| 39 | orm/10/10.17-wal-read-transaction-does-not-block-writer-checkpoint-row-sh | L8 |
| 40 | orm/11/11.3-autoincrement-at-max-rowid-raises-sqlite-full | M16 |
| 41 | orm/11/11.24-expression-index-used-for-lower-email-lookups-query-plan-tex | L6 |
| 42 | orm/11/11.26-covering-index-plan-and-automatic-index | L6 |
| 43 | orm/11/11.28-like-optimisation-needs-nocase-index-or-case-sensitive-like- | L6 |
| 44 | orm/11/11.29-attach-database-and-schema-qualified-names | M15 |
| 45 | orm/11/11.30-attach-inside-a-transaction-succeeds-detach-inside-it-fails- | L7 |
| 46 | orm/11/11.35-large-literal-sqlite-max-length-and-1000000-char-text-length | M13 |
| 47 | orm/11/11.37-self-join-of-table-with-no-alias-errors | M12 |
| 48 | orm/11/11.38-using-an-alias-of-the-select-list-in-where-works-sqlite-exte | M2 |
| 49 | orm/2/2.5-table-info-for-a-view-and-for-type-less-column | M14 |
| 50 | orm/2/2.6-table-list-3-37 | N5 |
| 51 | orm/2/2.7-index-list-origin-c-u-pk-and-partial-flag | M15 |
| 52 | orm/2/2.9-foreign-key-list-with-actions-and-match | M15 |
| 53 | orm/2/2.10-foreign-key-list-with-implicit-parent-column-gives-null-to | M15 |
| 54 | orm/2/2.11-sqlite-master-rows-and-sql-text-preserved-verbatim | L1 |
| 55 | orm/2/2.17-like-escape-underscore-in-sqlite-matches-any-char-orm-gotcha | L3 |
| 56 | orm/2/2.20-rails-schema-dumper-pragma-index-list-joined-to-index-info | H5 |
| 57 | orm/2/2.22-ef-core-pragma-table-info-joined-to-sqlite-master | H5 |
| 58 | orm/2/2.24-database-list-and-pragma-collation-list | N3 |
| 59 | orm/2/2.27-pragma-function-list-module-list-contain-core-items | M15 |
| 60 | orm/2/2.28-alembic-reflection-column-type-strings-come-back-exactly-as- | M15 |
| 61 | orm/3/3.1-rename-table-updates-indexes-triggers-views-and-fk-reference | H6 |
| 62 | orm/3/3.2-rename-with-legacy-alter-table-on-leaves-fk-parent-reference | M7 |
| 63 | orm/3/3.3-rename-with-foreign-keys-off-and-legacy-alter-table-off-stil | H6 |
| 64 | orm/3/3.5-rename-column-to-existing-name-fails | M10 |
| 65 | orm/3/3.13-add-column-non-constant-default-current-timestamp-rejected | M8 |
| 66 | orm/3/3.15-add-column-with-references-requires-null-default-when-fks-on | M8 |
| 67 | orm/3/3.18-drop-column-success-and-sqlite-master-text | M9 |
| 68 | orm/3/3.19-drop-column-fails-for-primary-key | M9 |
| 69 | orm/3/3.20-drop-column-fails-for-unique | M9 |
| 70 | orm/3/3.21-drop-column-fails-when-indexed | M9 |
| 71 | orm/3/3.23-drop-column-fails-when-a-table-level-check-uses-it | M9 |
| 72 | orm/3/3.24-drop-column-succeeds-when-the-check-is-on-the-dropped-column | M9 |
| 73 | orm/3/3.26-drop-column-fails-when-a-table-level-foreign-key-names-it | M9 |
| 74 | orm/3/3.34-django-pragma-foreign-keys-off-refused-inside-transaction-si | M6 |
| 75 | orm/3/3.35-rename-table-with-view-referencing-nonexistent-column-breaks | M10 |
| 76 | orm/3/3.36-create-table-if-not-exists-silent-when-exists-create-index-i | L2 |
| 77 | orm/3/3.41-create-index-on-missing-column-message | L2 |
| 78 | orm/3/3.42-create-index-name-collision-with-table | M12 |
| 79 | orm/3/3.45-alter-add-constraint-syntax-not-supported-3-53-adds-not-null | M11, L2 |
| 80 | orm/3/3.46-alter-table-alter-column-not-supported-prisma-django-emulate | M11 |
| 81 | orm/3/3.49-writable-schema-off-update-sqlite-master-refused | L7 |
| 82 | orm/3/3.51-vacuum-into-writes-a-copy | M13 |
| 83 | orm/4/4.5-returning-with-order-by-on-insert-select-is-unordered-return | L7 |
| 84 | orm/4/4.6-returning-cannot-reference-other-tables-subquery-in-column-l | L7 |
| 85 | orm/4/4.12-upsert-parse-ambiguity-select-source-needs-where-true | M12 |
| 86 | orm/4/4.15-upsert-on-partial-unique-index-needs-matching-where-in-targe | M1 |
| 87 | orm/4/4.21-multi-row-values-with-different-arities-fails | M16 |
| 88 | orm/4/4.22-insert-with-too-many-few-values-message | M16 |
| 89 | orm/4/4.24-insert-unknown-column-message | M16 |
| 90 | orm/4/4.26-limit-offset-with-literals-negative-limit-offset-without-lim | H2 |
| 91 | orm/4/4.44-update-with-limit-needs-sqlite-enable-update-delete-limit-de | N1 |
| 92 | orm/4/4.53-cannot-use-window-function-in-where | M4 |
| 93 | orm/4/4.60-double-quoted-string-literal-fallback-dqs-when-column-missin | L3 |
| 94 | orm/4/4.61-misuse-of-aggregate-nested-aggregate | L7 |
| 95 | orm/5/5.26-integer-primary-key-rejects-non-integer | M16 |
| 96 | orm/5/5.33-strict-unknown-type-name-rejected-datetime-not-allowed | L7 |
| 97 | orm/5/5.35-strict-column-without-type-rejected | L7 |
| 98 | orm/5/5.44-math-functions-need-sqlite-enable-math-functions | L7 |
| 99 | orm/5/5.45-random-and-randomblob-ranges | N4 |
| 100 | orm/6/6.21-python-sqlite3-parse-decltypes-column-decltype-retained-as-w | M15 |
| 101 | orm/7/7.11-json-object-odd-arguments-error | M18 |
| 102 | orm/7/7.25-json-pretty-3-46 | L1 |
| 103 | orm/8/8.5-unknown-collation-error | M12 |
| 104 | orm/8/8.6-unknown-collation-in-expression-error | L2 |
| 105 | orm/9/9.6-unique-on-expression-index-message-uses-expression-text | M16 |
| 106 | orm/9/9.11-check-constraint-failed-unnamed-named-and-column-level | L3 |
| 107 | orm/9/9.24-unrecognized-token | L7 |
| 108 | orm/9/9.25-unterminated-string-literal | L7 |
| 109 | orm/9/9.26-table-already-exists | L2 |
| 110 | orm/9/9.31-datatype-mismatch-on-rowid | M16 |
| 111 | orm/9/9.39-insert-or-fail-or-abort-or-rollback-statement-scoping | L3 |
| 112 | orm/9/9.40-insert-into-a-column-that-does-not-exist | M16 |
| 113 | orm/9/9.42-select-on-view-with-changed-table-error | L7 |
| 114 | orm/9/9.43-insert-into-view-refused | M16 |
| 115 | orm/9/9.44-cannot-drop-sqlite-internal-tables-modify | L7 |
| 116 | orm/9/9.46-too-many-columns-in-result-set-order-by-term-out-of-range | L7 |
| 117 | orm/9/9.47-order-by-term-does-not-match-any-column-in-compound-select | L7 |
| 118 | orm/9/9.48-selects-to-the-left-and-right-of-union-do-not-have-the-same- | L7 |
| 119 | orm/9/9.49-subquery-returns-more-than-1-column | L7 |
| 120 | orm/9/9.50-row-value-misused | L2 |
| 121 | orm/9/9.51-no-tables-specified-distinct-aggregate-misuse | L7 |
| 122 | query/aggregate-errors | M4, L7 |
| 123 | query/window-functions | M4 |
| 124 | query/cte-forms | H9, M12 |
| 125 | query/subqueries | M3, L7 |
| 126 | query/compound-and-order | L7 |
| 127 | query/ambiguous-and-missing-columns | L7 |
| 128 | query/column-names | M14 |
| 129 | schema/table-info-pragmas | H5, M15 |
| 130 | schema/create-table-sql-preserved | L1, L2 |
| 131 | schema/table-list-pragma | M15 |
| 132 | schema/strict-tables | L7 |
| 133 | schema/generated-columns | L7 |
| 134 | schema/views | M13, M14 |
| 135 | schema/instead-of-trigger-on-view | H8 |
| 136 | schema/drop-and-if-exists | L7 |
| 137 | schema/create-table-as | L1 |
| 138 | schema/default-expressions | M12 |
| 139 | text/trim-family | L1 |
| 140 | text/collation-unknown-error | L2 |
| 141 | text/string-literal-forms | M13, L2 |
| 142 | tx/begin-modes-and-errors | M13 |
| 143 | tx/attach-and-cross-db | L7 |
| 144 | tx/reindex-analyze-optimize | L7 |
| 145 | tx/vacuum-into | M13 |
