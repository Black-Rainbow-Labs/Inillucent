# Triage of the 261 usage cases that still differ (sqlite3 3.53.4 against inillucent-shell)

Source: round2-failures.json (261 cases: 208 holdout, 53 earlier). Every case was compared statement by statement. "S" is the pinned sqlite3 shell and "I" is inillucent-shell. Case names are shortened to the part after `holdout/` plus the number in the file (the index in round2-failures.json, 0 based, is shown in the mapping table at the end). Where a group says "package 5" the group is probably covered by the agent that is fixing error wording, dates, JSON and shell output (M16, L7, M17, M18, L1, L2, L3).

Reproductions below were run on both shells unless marked "from the case". Near line numbers in outputs are the line in the case script (two lines of prelude included).

## Summary

| Id | Severity | Title | Cases | Area |
|---|---|---|---|---|
| R1 | high | RIGHT or FULL JOIN after an inner join drops the unmatched rows | 3 | planner, joins |
| R2 | high | Subquery used as a value or IN inside outer join ON, RETURNING, or upsert SET and WHERE fails with "the new engine's physical pass does not handle..." | 10 | planner, executor |
| R3 | high | RETURNING subquery reads the old row or the wrong table state | 2 | executor, DML |
| R4 | high | INSERT ... VALUES ... UNION ALL VALUES inserts only the first row | 1 | binder, executor |
| R5 | high | LEFT JOIN json_each(NULL) drops the left row | 1 | table valued functions |
| R6 | high | `col IN (...)` on a hidden column of generate_series or json_each never returns | 1 | table valued functions, planner |
| R7 | high | sum, total, avg of infinity or overflowing floats return NaN | 3 | aggregates |
| R8 | high | `SELECT *` with USING over unaliased subqueries fails with "ambiguous column name" | 2 | binder, joins |
| R9 | high | A generated column used in a SELECT takes the name of the column in its expression | 5 | generated columns, binder |
| R10 | high | Declared type affinity, collation and STRICT type check are not applied to generated columns | 5 | generated columns |
| R11 | high | UNIQUE, NOT NULL and ON CONFLICT are not enforced on virtual generated columns | 9 | generated columns, constraints |
| R12 | high | Foreign keys that involve a generated column are not checked or fail wrongly | 6 | foreign keys, generated columns |
| R13 | high | Generated column values are NULL in OLD and NEW of a trigger and are computed for the NULL row of an outer join | 3 | triggers, generated columns, joins |
| R14 | high | Foreign key matching uses the wrong collation | 2 | foreign keys, collation |
| R15 | high | A row that references itself fails the foreign key check | 1 | foreign keys |
| R16 | high | A row deleted by REPLACE or UPDATE OR REPLACE does not run ON DELETE CASCADE nor its delete triggers | 2 | foreign keys, triggers |
| R17 | high | BEFORE INSERT trigger that inserts into its own table fails with "UNIQUE constraint failed: rowid" | 1 | triggers |
| R18 | high | A change a BEFORE UPDATE trigger makes to the same row is lost, and the index can be corrupted | 1 | triggers, UPDATE |
| R19 | high | COLLATE applied to a result column alias or an ordinal in ORDER BY and GROUP BY | 2 | binder, collation |
| R20 | high | COLLATE or DESC inside a table level UNIQUE or PRIMARY KEY term is ignored | 3 | collation, constraints |
| R21 | high | ALTER TABLE ADD COLUMN stores the new column after the table constraints | 2 | ALTER |
| R22 | high | UNION, INTERSECT and EXCEPT ignore an explicit COLLATE on the right arm | 2 | collation, compound select |
| R23 | high | Compound subquery columns: affinity and WHERE push down (includes the known regression type-affinity/455) | 5 | planner, affinity |
| R24 | high | INSERT DEFAULT VALUES uses the DEFAULT of an INTEGER PRIMARY KEY column | 1 | executor, DML |
| R25 | high | UPDATE that overflows an INTEGER PRIMARY KEY stores a REAL | 1 | executor, DML |
| R26 | high | UPDATE SET with a window function subquery sees rows already updated | 1 | executor, DML |
| R27 | high | A view in main reads a temp table that shadows its own base table | 1 | binder, schema |
| R28 | high | CREATE TABLE AS into a name that contains a double quote leaves an unreadable schema | 1 | DDL |
| R29 | medium | Aggregate of the outer query used inside a correlated subquery is refused | 1 | binder, aggregates |
| R30 | medium | Bare columns next to more than one min or max aggregate come from the wrong row | 2 | aggregates |
| R31 | medium | IN, and row value IN, use the wrong collation and affinity | 2 | collation, affinity |
| R32 | medium | A multi column scalar subquery as a row value is "unsupported" | 2 | binder |
| R33 | medium | A persistent view in main may reference an attached database | 3 | DDL, views |
| R34 | medium | PRAGMA auto_vacuum with a schema qualifier is applied to main | 1 | PRAGMA |
| R35 | medium | A CTE referenced twice is evaluated twice | 1 | planner, CTE |
| R36 | medium | generate_series with a negative step returns no rows | 1 | table valued functions |
| R37 | medium | An integer literal of 2147483648 or more in ORDER BY or GROUP BY is treated as a column number | 1 | binder |
| R38 | medium | A window function in the ORDER BY of an EXISTS subquery fails | 1 | binder, window functions |
| R39 | medium | Operands SQLite never evaluates are evaluated (AND, OR, coalesce, iif, EXISTS select list) | 2 | executor |
| R40 | medium | BEFORE trigger NEW values do not have the column affinity applied | 2 | triggers, affinity |
| R41 | medium | Statement level OR REPLACE does not override a conflict clause in a trigger body | 1 | triggers |
| R42 | medium | Rows skipped by RAISE(IGNORE) in a BEFORE DELETE trigger are still returned by RETURNING | 1 | triggers |
| R43 | medium | RENAME COLUMN does not rewrite NEW.col in a TEMP trigger | 1 | ALTER, triggers |
| R44 | medium | A column assigned twice in UPDATE SET, or listed twice in INSERT, is refused | 3 | binder |
| R45 | medium | ALTER TABLE ADD COLUMN accepts or refuses the wrong things | 8 | ALTER |
| R46 | medium | Infinity prints as `Inf` in quote() and as `9e999` in JSON, SQLite prints `9.0e+999` | 6 | built in functions |
| R47 | medium | INSERT OR ROLLBACK with ON CONFLICT DO UPDATE rolls back the transaction on a later constraint error | 1 | executor, upsert |
| R48 | medium | DROP INDEX and DROP TABLE leave rows in sqlite_stat1 | 1 | DDL, ANALYZE |
| R49 | medium | INDEXED BY a partial index is refused where SQLite accepts it | 2 | planner |
| R50 | medium | Message "a rowid must be an integer, not Text([97])" instead of "datatype mismatch" | 4 | executor |
| R51 | medium | UNIQUE failure on an expression index prints an empty column list | 1 | constraints |
| R52 | medium | INSERT column and value count messages differ | 5 | binder |
| R53 | medium | Parser: AUTOINCREMENT in a table level PRIMARY KEY and a quoted type name | 2 | parser |
| R54 | medium | JSON functions on a BLOB holding JSON text (package 5) | 3 | JSON |
| R55 | medium | JSON functions: NULL path, argument counts, bracket paths (package 5) | 5 | JSON |
| R56 | medium | Date and time functions (package 5) | 6 | built in functions |
| R57 | medium | Error message wording (package 5) | 58 | many |
| R58 | medium | Shell output: unistr(), error context lines, abandoned line, .parameter (package 5) | 15 | shell |
| R59 | low | JSON: JSON5 whitespace, lone surrogates, json_tree ids (package 5) | 3 | JSON |
| R60 | low | Window definitions that extend a named window are not validated | 2 | binder, window functions |
| R61 | low | Text with NUL bytes or invalid UTF-8 | 6 | built in functions |
| R62 | low | atanh differs from SQLite in the last digits | 3 | built in functions |
| R63 | low | Numeric literals with `_` separators | 1 | tokenizer |
| R64 | low | CREATE TABLE AS leaves out the type of a CAST column | 1 | DDL |
| R65 | low | EXPLAIN QUERY PLAN text and plan choice | 8 | planner |
| R66 | low | AUTOINCREMENT bookkeeping in sqlite_sequence | 4 | executor |
| R67 | low | INSERT ... AS excluded alias, and last_insert_rowid() inside RETURNING | 2 | upsert |
| R68 | low | Statements SQLite refuses at CREATE time are accepted | 10 | DDL validation |
| R69 | low | Stored CREATE INDEX text loses a trailing space | 1 | DDL |
| R70 | low | PRAGMA reverse_unordered_selects is ignored | 1 | PRAGMA |
| R71 | low | A run time error discards rows the statement had already returned | 1 | executor, shell |
| N1 | not a defect | Engine configuration defaults and page layout | 12 | configuration |
| N2 | not a defect | UPDATE and DELETE with ORDER BY or LIMIT | 2 | parser |
| N3 | not a defect | Row order where SQLite does not define one | 2 | none |

---

## High

### R1 (high): RIGHT or FULL JOIN after an inner join drops the unmatched rows
Cases: 107 joins/340, 109 joins/346, 116 joins/716. Area: planner, joins.
```
CREATE TABLE a(x INT); CREATE TABLE b(x INT); CREATE TABLE c(x INT);
INSERT INTO a VALUES(1),(2); INSERT INTO b VALUES(2),(3); INSERT INTO c VALUES(3),(4);
SELECT a.x, b.x, c.x FROM a JOIN b ON a.x=b.x RIGHT JOIN c ON c.x=b.x ORDER BY 1,2,3;
SELECT * FROM t1 JOIN t1 AS u ON 0 FULL OUTER JOIN t0 ON true;   -- t1(2), t0(1)
```
S: `NULL,NULL,3` and `NULL,NULL,4`; second query `NULL,NULL,1`. I: no rows at all for both. The same joins written with a LEFT JOIN as the left side (`a LEFT JOIN b ... RIGHT JOIN c`) and `t1 FULL JOIN t0 ON 0` return the right rows in I. The loss happens when the left side of the RIGHT or FULL JOIN is an inner join chain.

### R2 (high): Subquery used as a value or IN is refused in several places
Cases: 103 joins/152, 104 joins/315, 105 joins/316, 111 joins/359, 114 joins/510, 115 joins/513, 169 triggers/442, 191 upsert/109, 195 upsert/186, 197 upsert/386. Area: planner and executor (subquery lowering).
```
CREATE TABLE a(id INTEGER PRIMARY KEY, x INTEGER); INSERT INTO a VALUES(1,5);
CREATE TABLE b(id INTEGER, x INTEGER); INSERT INTO b VALUES(1,7);
INSERT INTO a VALUES(1,0) ON CONFLICT(id) DO UPDATE SET x=(SELECT x FROM b WHERE id=1);
SELECT x FROM a;
```
S: `7`. I: `Error: the new engine's physical pass does not handle a correlated subquery used as a value yet` and x stays 5. The same message (or "...a correlated IN subquery yet") appears for: a subquery in the ON clause of a LEFT or RIGHT JOIN that references an earlier table (`LEFT JOIN t3 ON ... AND EXISTS (SELECT 1 FROM s WHERE s.k = t1.a)`), a subquery in RETURNING (`INSERT ... RETURNING a, (SELECT a FROM u)`, `DELETE ... RETURNING a, (SELECT d FROM t2 WHERE t2.c = t1.a)`), and a subquery in ON CONFLICT DO UPDATE SET or WHERE. Even an uncorrelated `(SELECT a FROM u)` in RETURNING fails. These are valid statements that real applications send.

### R3 (high): RETURNING subquery reads the old row or the wrong table state
Cases: 193 upsert/113, 194 upsert/114. Area: executor, DML.
```
CREATE TABLE t1(a INTEGER PRIMARY KEY, b INTEGER); CREATE TABLE lookup(k INTEGER PRIMARY KEY, v TEXT);
INSERT INTO t1 VALUES(1,10); INSERT INTO lookup VALUES(10,'old'),(20,'new');
UPDATE t1 SET b = 20 WHERE a = 1 RETURNING b, (SELECT v FROM lookup WHERE k = b);
```
S: `20,'new'`. I: `20,'old'` (the subquery sees b as 10, the value before the update). Case 194: `DELETE FROM t1 WHERE a=2 RETURNING *, (SELECT count(*) FROM t1)` S shows 2 remaining, I shows 3. `UPDATE t2 SET b=b+10 RETURNING a,b,(SELECT sum(b) FROM t2)` S shows 610, 620, 630 (each row sees the updates made so far), I shows 600 for every row (sees none).

### R4 (high): INSERT ... VALUES ... UNION ALL VALUES inserts only the first row
Case: 188 upsert/35. Area: binder, executor.
```
CREATE TABLE a(a,b); INSERT INTO a(a,b) VALUES(3,3) UNION ALL VALUES(4,4); SELECT count(*) FROM a;
```
S: `2`. I: `1`. The second VALUES is silently dropped. (The bare `VALUES(3,3) UNION ALL VALUES(4,4)` SELECT is correct in I.)

### R5 (high): LEFT JOIN json_each(NULL) drops the left row
Case: 126 json/397. Area: table valued functions, executor.
```
CREATE TABLE t(id INTEGER, tags); INSERT INTO t VALUES(1,'["x"]'),(2,NULL);
SELECT t.id, j.value FROM t LEFT JOIN json_each(t.tags) AS j ON 1 ORDER BY 1;
```
S: `1,'x'` and `2,NULL`. I: `1,'x'` only. The same query with `generate_series(5,1)` null extends correctly, so the loss is specific to json_each and json_tree when the argument is NULL.

### R6 (high): IN on a hidden column of generate_series or json_each never returns
Case: 123 json/368. Area: table valued functions, planner.
```
SELECT count(*) FROM generate_series WHERE start = 1 AND stop IN (5);
SELECT count(*) FROM json_each WHERE json IN ('[1,2,3]');
```
S: `5` and `3`. I: the statement does not return (the harness stopped it after 120 seconds). The same query with `stop = 5` works.

### R7 (high): sum, total and avg of infinity or overflowing floats return NaN
Cases: 5 aggregates/80, 6 aggregates/115, 135 numbers/391. Area: aggregates.
```
SELECT sum(x) FROM (SELECT 1e308 x UNION ALL SELECT 1e308 UNION ALL SELECT 0);
CREATE TABLE t(x REAL); INSERT INTO t VALUES(1e400); SELECT sum(x), avg(x), total(x), max(x) FROM t;
```
S: `Inf` and `Inf,Inf,Inf,Inf`. I: `NaN` and `NaN,NaN,NaN,Inf`. The running compensation term for the sum becomes Inf minus Inf.

### R8 (high): SELECT * with USING over unaliased subqueries fails
Cases: 252 query/cte-and-view-column-spelling, 110 joins/358. Area: binder, joins.
```
CREATE TABLE t(Abc, Def); INSERT INTO t VALUES(1,2);
SELECT * FROM (SELECT abc, def FROM t) JOIN (SELECT abc FROM t) USING (abc);
```
S: `'abc','def'` and `1,2`. I: `Parse error: ambiguous column name: *.subquery.def`. With aliases (`a JOIN ... b USING`) it works. Case 110 has two more differences: `SELECT * FROM t1 LEFT JOIN (t2 JOIN t3 USING(a)) USING(a)` returns `'a'` and `1` in S and two columns `'a','a'` in I, and `SELECT * FROM t1 JOIN (t2 JOIN t3 ON t2.a=t3.a) ON t1.a=t2.a` names the third column `a:1` in S and `a` in I.

### R9 (high): A generated column in a SELECT takes the name of the column in its expression
Cases: 67 generated-columns/219, 70 /258, 76 /291, 77 /294, 79 /296. Area: generated columns, binder.
```
CREATE TABLE t(a TEXT, g AS (a), h AS (a||'x'), k INT AS (a)); INSERT INTO t(a) VALUES('1');
SELECT a, g FROM t;  SELECT g, h, k, typeof(k) FROM t;  SELECT t.g FROM t;
```
S headers: `'a','g'`, `'g','h','k','typeof(k)'`, `'g'`. I headers: `'a','a'`, `'a','h','a','typeof(k)'`, `'a'`. `SELECT *` and `SELECT g AS z` are correct. Result column names matter to every application that reads rows by name. The generated expression is substituted before the result column name is fixed.

### R10 (high): Declared type affinity, collation and STRICT type check are not applied to generated columns
Cases: 61 /211, 67 /219, 76 /291, 77 /294, 79 /296. Area: generated columns.
```
CREATE TABLE t(a TEXT, k INT AS (a)); INSERT INTO t(a) VALUES('1'); SELECT typeof(k) FROM t;
CREATE TABLE t2(a TEXT, b INTEGER GENERATED ALWAYS AS (a) VIRTUAL) STRICT; INSERT INTO t2(a) VALUES('abc');
CREATE TABLE t1(a TEXT, b TEXT AS (a) VIRTUAL COLLATE NOCASE); INSERT INTO t1(a) VALUES('Hello'),('HELLO'),('hello');
SELECT count(*) FROM t1 WHERE b IN ('hello');
```
S: `'integer'`; the STRICT insert fails with `cannot store TEXT value in INTEGER column t2.b` (and a value `'42'` is stored as the integer 42); the NOCASE count is `3`. I: `'text'`; the STRICT insert succeeds and stores text; the count is `1`. The value of a generated column must be converted with the column's affinity, checked against a STRICT type, and compared with the column's collation. This also breaks `WHERE g=1` on an indexed generated column (case 76: the SELECT finds the row in S and returns nothing in I) and UNIQUE on `1` against `01` (case 77).

### R11 (high): UNIQUE, NOT NULL and ON CONFLICT are not enforced on virtual generated columns
Cases: 62 /212, 65 /216, 66 /217, 69 /254, 77 /294, 80 /297, 81 /521, 84 /524, 86 /678. Area: generated columns, constraints, upsert.
```
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL UNIQUE);
INSERT INTO t1(a) VALUES(1); INSERT INTO t1(a) VALUES(1); SELECT count(*) FROM t1;
CREATE TABLE n(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL NOT NULL);
INSERT INTO n(a) VALUES(1); UPDATE n SET a = NULL; INSERT INTO n(a) VALUES(NULL);
CREATE TABLE u(a, b AS(a*2)); CREATE UNIQUE INDEX i ON u(b); INSERT INTO u(a) VALUES(1);
INSERT INTO u(a) VALUES(1) ON CONFLICT(b) DO UPDATE SET a=2;
```
S: UNIQUE: second insert fails with `UNIQUE constraint failed: t1.b` and the count is 1. NOT NULL: the UPDATE and the INSERT fail with `NOT NULL constraint failed: n.b`. Upsert on `ON CONFLICT(b)` updates the row to `1,2,4`. I: both inserts succeed and the count is 2; the NOT NULL statements succeed and store `NULL,NULL`; the ON CONFLICT clause is rejected with `ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint`. INSERT OR REPLACE (case 62) leaves duplicates instead of replacing. The same checks are missing when DO UPDATE changes the source column (cases 81, 84).

### R12 (high): Foreign keys that involve a generated column
Cases: 63 /214, 64 /215, 68 /221, 78 /295, 82 /522, 85 /526. Area: foreign keys, generated columns.
```
PRAGMA foreign_keys=ON;
CREATE TABLE p(a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) VIRTUAL UNIQUE);
CREATE TABLE c1(x INTEGER REFERENCES p(b)); INSERT INTO p(a) VALUES(1); INSERT INTO c1(x) VALUES(2);
UPDATE p SET a=3 WHERE a=1;
CREATE TABLE p2(a TEXT, g INT AS(a)); CREATE UNIQUE INDEX pg ON p2(g); INSERT INTO p2(a) VALUES('1');
CREATE TABLE c2(x REFERENCES p2(g)); INSERT INTO c2 VALUES(1);
```
S: the UPDATE fails with `FOREIGN KEY constraint failed` (child row 2 would lose its parent), and `INSERT INTO c2` succeeds. I: the UPDATE succeeds, and `INSERT INTO c2 VALUES(1)` fails with `FOREIGN KEY constraint failed`. A generated column as a child column (`b ... REFERENCES p(x)`) is not checked on UPDATE or on upsert (cases 64, 82). Deleting a parent row whose key is generated does not cascade or fail (cases 68, 85).

### R13 (high): Generated column values are wrong in triggers and in null extended join rows
Cases: 60 /208, 83 /523, 75 /290. Area: triggers, generated columns, joins.
```
CREATE TABLE t1(a INTEGER, b GENERATED ALWAYS AS (a*2) VIRTUAL); CREATE TABLE log(msg TEXT);
CREATE TRIGGER tr1 AFTER DELETE ON t1 BEGIN INSERT INTO log VALUES('deleted ' || OLD.a || ',' || OLD.b); END;
INSERT INTO t1(a) VALUES(1),(2); DELETE FROM t1 WHERE a = 1; SELECT * FROM log;
CREATE TABLE l(x); CREATE TABLE r(a, g AS (1)); INSERT INTO l VALUES(1);
SELECT x,a,g,typeof(g) FROM l LEFT JOIN r ON false;
```
S: `'deleted 1,2'`; the join returns `1,NULL,NULL,'null'`. I: the log message is `NULL` (OLD.b is NULL); the join returns `1,NULL,1,'integer'` (the expression is computed for the null extended row). In an AFTER UPDATE trigger OLD.c and NEW.c are NULL in I (S: 10 and 50).

### R14 (high): Foreign key matching uses the wrong collation
Cases: 53 foreign-keys/99, 55 foreign-keys/416. Area: foreign keys, collation.
```
PRAGMA foreign_keys=ON;
CREATE TABLE p(id TEXT PRIMARY KEY COLLATE NOCASE); CREATE TABLE c(pid TEXT REFERENCES p(id) ON DELETE CASCADE);
INSERT INTO p VALUES('A'); INSERT INTO c VALUES('a'); DELETE FROM p WHERE id='A'; SELECT count(*) FROM c;
CREATE TABLE p2(id TEXT PRIMARY KEY COLLATE NOCASE); CREATE TABLE c2(pid TEXT REFERENCES p2(id) ON UPDATE CASCADE);
INSERT INTO p2 VALUES('A'); INSERT INTO c2 VALUES('a'); UPDATE p2 SET id='B' WHERE id='A'; SELECT quote(pid) FROM c2;
```
S: `0` and `'B'` (the parent column collation decides which child rows match). I: `1` and `'a'`. The reverse (case 55, child column NOCASE and parent column BINARY): S keeps child `'a'` when parent `'A'` is deleted, I deletes it too.

### R15 (high): A row that references itself fails the foreign key check
Case: 57 foreign-keys/479. Area: foreign keys.
```
PRAGMA foreign_keys=ON; CREATE TABLE n(id INTEGER PRIMARY KEY, parent_id REFERENCES n(id));
INSERT INTO n VALUES (1, 2), (2, NULL); INSERT INTO n VALUES (3, 99); INSERT INTO n VALUES (4, 4); SELECT count(*) FROM n;
```
S: the (3,99) insert fails, the (4,4) insert succeeds, count is `3`. I: both inserts fail, count is `2`. A row whose foreign key refers to its own key must pass.

### R16 (high): A row deleted by REPLACE or UPDATE OR REPLACE does not cascade nor run its delete triggers
Cases: 54 foreign-keys/120, 162 triggers/87. Area: foreign keys, triggers.
```
PRAGMA foreign_keys=ON;
CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE child(id INTEGER PRIMARY KEY, parent_id INT REFERENCES parent(id) ON DELETE CASCADE);
INSERT INTO parent VALUES (10,'x'),(20,'y'); INSERT INTO child VALUES (10,10),(20,20);
UPDATE OR REPLACE parent SET name='y' WHERE id=10; SELECT * FROM child ORDER BY id;
```
S: `10,10` only (row 20 was replaced away and its child cascaded). I: `10,10` and `20,20`. For triggers: with `PRAGMA recursive_triggers=ON`, `REPLACE INTO t1 VALUES(1,100)` fires the AFTER DELETE trigger in S (log `deleted id=1` then `inserted id=1`) and only the insert trigger in I.

### R17 (high): A BEFORE INSERT trigger that inserts into its own table fails
Case: 170 triggers/614. Area: triggers, rowid allocation.
```
CREATE TABLE u(a); CREATE TRIGGER tu BEFORE INSERT ON u BEGIN INSERT INTO u VALUES(0); END;
INSERT INTO u VALUES(1); INSERT INTO u VALUES(2); SELECT count(*) FROM u;
```
S: `4`. I: both inserts fail with `UNIQUE constraint failed: u.rowid` and the count is 0. The rowid of the outer row is chosen before the trigger runs, and the trigger's insert takes the same rowid. With an INTEGER PRIMARY KEY the message is `UNIQUE constraint failed: s.id`.

### R18 (high): A change a BEFORE UPDATE trigger makes to the same row is lost, and the index can be corrupted
Case: 171 triggers/690. Area: triggers, UPDATE.
```
CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, v TEXT, UNIQUE(k, v)); INSERT INTO t VALUES (1,'a','old');
CREATE TRIGGER bu BEFORE UPDATE ON t BEGIN UPDATE t SET k='mid' WHERE id=OLD.id AND k<>'mid'; END;
UPDATE t SET v='new' WHERE id=1; SELECT * FROM t;
INSERT INTO t VALUES (1,'x','x') ON CONFLICT(id) DO UPDATE SET v='new2'; SELECT * FROM t; PRAGMA integrity_check;
```
S: `1,'mid','new'`, then `1,'mid','new2'`, integrity_check `ok`. I: `1,'a','new'`, then `1,'a','new2'`, integrity_check `wrong # of entries in index sqlite_autoindex_t_1`. The outer UPDATE writes back the row image it read before the trigger ran, so the trigger's change is lost and the unique index keeps an entry for a value the row no longer has.

### R19 (high): COLLATE on a result column alias or an ordinal in ORDER BY and GROUP BY
Cases: 33 collation/246, 48 expressions/425. Area: binder, collation.
```
CREATE TABLE t(x TEXT, v INT); INSERT INTO t VALUES('b',1),('A',2),('c',3),('a',4),('B',1);
SELECT x FROM t ORDER BY 1 COLLATE NOCASE, v;
SELECT x, sum(v) FROM t GROUP BY 1 COLLATE NOCASE;
CREATE TABLE t2(a INT, b INT); INSERT INTO t2 VALUES(1,30),(2,20),(3,10);
SELECT b AS a FROM t2 ORDER BY a COLLATE BINARY;
```
S: `A,a,b,B,c`; groups `'A',6` `'b',2` `'c',3`; `10,20,30`. I: `b,B,A,c,a` (the ordinal is treated as the constant 1); one group `'b',11`; `30,20,10` (the alias is read as the column t2.a). A bare alias or ordinal wrapped in COLLATE still names the result column.

### R20 (high): COLLATE or DESC inside a table level UNIQUE or PRIMARY KEY term is ignored
Cases: 13 alter/407, 15 alter/463, 34 collation/303. Area: collation, constraints.
```
CREATE TABLE p(a TEXT, PRIMARY KEY(a COLLATE NOCASE)); INSERT INTO p VALUES ('a'); INSERT INTO p VALUES ('A');
```
S: the second insert fails with `UNIQUE constraint failed: p.a`. I: it succeeds and the table holds both rows. Same for `UNIQUE(a COLLATE NOCASE)` and after `RENAME COLUMN` (case 15). Case 34 also checks `PRAGMA index_xinfo` for `UNIQUE(a COLLATE NOCASE DESC, b)`.

### R21 (high): ALTER TABLE ADD COLUMN stores the new column after the table constraints
Cases: 13 alter/407, 58 foreign-keys/515. Area: ALTER.
```
CREATE TABLE t(a TEXT, UNIQUE(a DESC)); ALTER TABLE t ADD COLUMN q TEXT; SELECT sql FROM sqlite_master WHERE name='t';
```
S: `CREATE TABLE t(a TEXT, q TEXT, UNIQUE(a DESC))`. I: `CREATE TABLE t(a TEXT, UNIQUE(a DESC), q TEXT)`. A column definition after a table constraint is a syntax error in SQLite, so a file written by inillucent cannot be opened by SQLite after this ALTER. Same for `FOREIGN KEY(...)` constraints.

### R22 (high): UNION, INTERSECT and EXCEPT ignore an explicit COLLATE on the right arm
Cases: 32 collation/243, 37 collation/597. Area: collation, compound select.
```
SELECT 'hello' UNION SELECT 'Hello' COLLATE NOCASE;
SELECT 'A' UNION SELECT 'a' COLLATE NOCASE;
```
S: one row each, `'Hello'` and `'a'` (the explicit COLLATE NOCASE on the second arm decides when rows are equal). I: two rows each, `'hello','Hello'` and `'A','a'`. When the explicit COLLATE is on the left most arm (`SELECT 'a' COLLATE NOCASE UNION SELECT 'A'`) I is correct. The same applies to INTERSECT: `SELECT s FROM t1 INTERSECT SELECT s COLLATE NOCASE FROM t2` (case 37) returns an extra row in I.

### R23 (high): Compound subquery columns: affinity and WHERE push down (includes the known regression type-affinity/455)
Cases: 181 type-affinity/455, 180 type-affinity/396, 182 type-affinity/461, 117 joins/719, 179 type-affinity/371. Area: planner (push down), affinity.
```
CREATE TABLE t(x TEXT); CREATE TABLE v(y INTEGER); INSERT INTO t VALUES('1'),('2'); INSERT INTO v VALUES(9);
SELECT group_concat(x) FROM (SELECT x FROM t UNION ALL SELECT y FROM v) WHERE x > 7;
SELECT group_concat(x) FROM (SELECT x FROM t UNION ALL SELECT y FROM v LIMIT 100) WHERE x > 7;
SELECT group_concat(x) FROM (SELECT x FROM t UNION SELECT y FROM v) WHERE x > 7;
```
S: `'9'`, `'1,2,9'`, `'9'`. I: `'1,2,9'`, `'1,2,9'`, `'9,1,2'`.

Precise description of type-affinity/455. SQLite pushes the outer WHERE term into each arm of the compound subquery (the push down optimization; it is skipped when the subquery has a LIMIT, which is why the second query returns `'1,2,9'` in S). In each arm the column keeps its own affinity: in arm `SELECT x FROM t` the comparison `x > 7` uses TEXT affinity on both sides, so `'1' > '7'` is false and `'2' > '7'` is false; in arm `SELECT y FROM v` it compares integers, `9 > 7`. Only 9 passes. The compound subquery column itself has no usable affinity, so evaluated after the compound (what inillucent does in all three queries, and what SQLite does when push down is blocked), the text values `'1'` and `'2'` compare greater than the integer 7 by storage class and pass. inillucent has no push down into compound arms, so it always gives the unoptimized answer. The same rule gives `DELETE FROM u WHERE x IN (SELECT x FROM (... UNION ALL ...) WHERE x > 7)` deleting 0 rows in S and all rows in I, and `SELECT group_concat(y) FROM (SELECT y FROM v UNION ALL SELECT x FROM t) WHERE y > 7` returning `'9'` in S and `'9,1,2'` in I. The UNION form also returns the rows in a different order in I (the dedupe sorts differently).

Other affinity differences in the same area:
- 180: `CREATE VIEW v AS SELECT amount FROM p UNION ALL SELECT amount FROM q` (p.amount REAL, q.amount INTEGER holding 7): `SELECT typeof(amount), amount/2 FROM v` is `'real',3.5` in S and `'integer',3` in I; the value copied by `INSERT INTO aud SELECT amount FROM v` is `7.0` real in S and `7` integer in I. The left arm affinity is applied to the other arm's values.
- 117: `WITH s(code,label) AS (SELECT 1,'Active' UNION ALL SELECT 2,'Cancelled' ...) SELECT ... FROM orders o JOIN s ON o.status_code = s.code` where status_code is TEXT holding `'1'`: S returns 3 rows, I returns none. The TEXT column compared with a column that has no affinity must apply TEXT affinity to the other side.
- 182: `SELECT count(*) FROM u JOIN (SELECT n FROM t UNION VALUES(1),(2)) w ON u.x = w.n` (u.x TEXT '7','9'; t.n INTEGER): S `0`, I `1`. Without the VALUES arm both give 1.
- 179: `CREATE TABLE probe AS SELECT v FROM (SELECT CAST(i AS REAL) AS v FROM t UNION ALL SELECT i FROM t)`: `pragma_table_info` type is `'NUM'` in S and empty in I.

### R24 (high): INSERT DEFAULT VALUES uses the DEFAULT of an INTEGER PRIMARY KEY column
Case: 199 upsert/501. Area: executor, DML.
```
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT 100, val TEXT DEFAULT 'x');
INSERT INTO t DEFAULT VALUES; INSERT INTO t(val) VALUES ('y'); INSERT INTO t DEFAULT VALUES; SELECT * FROM t;
```
S: `1,'x'`, `2,'y'`, `3,'x'` (the DEFAULT on a rowid alias is ignored). I: `100,'x'`, then the second and third inserts fail with `UNIQUE constraint failed: t.id`.

### R25 (high): UPDATE that overflows an INTEGER PRIMARY KEY stores a REAL
Case: 187 upsert/33. Area: executor, DML.
```
CREATE TABLE t(a INTEGER PRIMARY KEY); INSERT INTO t VALUES (-9223372036854775808); UPDATE t SET a = a - 1; SELECT * FROM t;
```
S: `Error: datatype mismatch`, the row stays `-9223372036854775808`. I: no error, the row now holds `-9.2233720368547758e+18` (a REAL in a rowid column).

### R26 (high): UPDATE SET with a window function subquery sees rows already updated
Case: 210 window-functions/437. Area: executor, DML.
```
CREATE TABLE u(id INTEGER PRIMARY KEY, v INTEGER); INSERT INTO u VALUES(1,1),(2,2),(3,3);
UPDATE u SET v = (SELECT q FROM (SELECT id AS iid, sum(v) OVER () AS q FROM u) WHERE iid = u.id); SELECT * FROM u;
```
S: `1,6` `2,6` `3,6`. I: `1,6` `2,11` `3,20`. The derived table is computed again for each row and reads the rows already changed. With `row_number() OVER (ORDER BY v DESC)` S gives 4,3,2,1 and I gives 4,4,4,4. A plain `UPDATE u SET v = (SELECT sum(v) FROM u)` is correct in I.

### R27 (high): A view in main reads a temp table that shadows its own base table
Case: 25 attach-and-temp-schemas/473. Area: binder, schema.
```
CREATE TABLE base(value INTEGER); INSERT INTO base VALUES(1); CREATE VIEW ov AS SELECT value FROM base;
CREATE TEMP TABLE base(value INTEGER); INSERT INTO temp.base VALUES(2); SELECT * FROM ov;
```
S: `1`. I: `2`. Names inside a persistent view resolve in the view's own database first.

### R28 (high): CREATE TABLE AS into a name that contains a double quote leaves an unreadable schema
Case: 146 schema/832. Area: DDL.
```
CREATE TABLE src(a); INSERT INTO src VALUES (1); CREATE TABLE "o""x" AS SELECT a FROM src; SELECT * FROM "o""x";
```
S: `1`. I: `Error: cannot parse the CREATE TABLE stored in the schema: unrecognized token: unterminated quoted name`. The stored SQL does not double the quote. Afterwards the table cannot be read or dropped, and `no such table: o"x` is reported.

---

## Medium

### R29 (medium): Aggregate of the outer query used inside a correlated subquery is refused
Case: 158 subqueries/511. Area: binder, aggregates.
```
CREATE TABLE t2(a INTEGER, b INTEGER, c TEXT); CREATE TABLE t3(a INTEGER PRIMARY KEY, b INTEGER, c TEXT);
INSERT INTO t2 VALUES (1,5,'p'),(2,1,'q'); INSERT INTO t3 VALUES (1,3,'a'),(2,4,'a'),(3,0,'b');
SELECT x.c, count(*), (SELECT count(*) FROM t2 AS y WHERE y.b > min(x.b)) FROM t3 AS x GROUP BY x.c ORDER BY 1;
```
S: `'a',2,1` and `'b',1,2`. I: `Parse error: misuse of aggregate function min()`. An aggregate whose argument refers only to the outer query belongs to the outer query.

### R30 (medium): Bare columns next to more than one min or max come from the wrong row
Cases: 7 aggregates/588, 8 aggregates/674. Area: aggregates.
```
create table t(a,b,c); insert into t values (2,'x','z'),(2,'y','a');
select a, b, max(c), min(c) from t group by a;
```
S: `2,'y','z','a'` (the bare column comes from the row that last updated any min or max accumulator). I: `2,'x','z','a'` (first row). Case 674 (`player, game, MAX(score), MIN(score)`): S gives `'Alice','checkers',80,30` and `'Bob','go',90,20`, I gives `'chess'` for both. With a single min or max aggregate both agree.

### R31 (medium): IN, and row value IN, use the wrong collation and affinity
Cases: 35 collation/469, 36 collation/470. Area: collation, affinity.
```
CREATE TABLE r(value TEXT COLLATE NOCASE); INSERT INTO r VALUES('a');
SELECT 'A' IN (value), 'A' IN ('a' COLLATE NOCASE, 'x'), ('A' COLLATE NOCASE) IN (value COLLATE BINARY) FROM r;
SELECT (1, 2) IN ((CAST('1' AS TEXT), 2), (CAST('x' AS BLOB), 3));
SELECT ('a', 1) IN (('A', 1), ('x' COLLATE NOCASE, 2));
```
S: `0,0,1`; `0`; `1`. I: `1,1,1`; `1`; `0`. For `x IN (list)` SQLite takes the collation from the left operand (an explicit COLLATE on a list element does not count), and a list element with TEXT affinity does not trigger numeric conversion of an integer on the left. For the row value form S uses the collation of the vector element that carries one.

### R32 (medium): A multi column scalar subquery as a row value is "unsupported"
Cases: 36 collation/470, 157 subqueries/500 (message differences in 201, 246, 249). Area: binder.
```
SELECT (SELECT 1, 2) IN (SELECT 1, 2);  SELECT (SELECT 1, 2) IN (VALUES(1, 2));
CREATE TABLE c(a TEXT COLLATE NOCASE); INSERT INTO c VALUES('A'); SELECT (SELECT a, 1 FROM c) = ('a', 1);
```
S: `1`, `1`, `1`. I: `Parse error: unsupported: sub-select returns more than one column`. Where SQLite also errors (`SELECT (SELECT 1,2) = 1`) the text is `row value misused` or `sub-select returns 2 columns - expected 1`.

### R33 (medium): A persistent view in main may reference an attached database
Cases: 22 attach/226, 26 attach/486, 31 attach/789. Area: DDL, views.
```
ATTACH ':memory:' AS aux; CREATE TABLE aux.t(a); CREATE VIEW mv AS SELECT a FROM aux.t; SELECT * FROM mv;
CREATE TEMP VIEW tv AS SELECT a FROM aux.t;
```
S: `Parse error: view mv cannot reference objects in database aux`; the TEMP view is allowed. I accepts the persistent view and returns rows. The same for `CREATE VIEW main.cross_v AS SELECT * FROM aux.t1`.

### R34 (medium): PRAGMA auto_vacuum with a schema qualifier is applied to main
Case: 30 attach/740. Area: PRAGMA.
```
ATTACH ':memory:' AS aux0; PRAGMA aux0.auto_vacuum=full; CREATE TABLE aux0.a(x); CREATE TABLE main.m(x);
PRAGMA main.auto_vacuum; PRAGMA aux0.auto_vacuum;
```
S: `0` then `1`. I: `1` then `1`. In I the setting leaks into main and is not kept per database (reading `aux0.auto_vacuum` before any table exists returns 0).

### R35 (medium): A CTE referenced twice is evaluated twice
Case: 39 ctes/252. Area: planner, CTE.
```
WITH cte AS (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
WITH cte AS MATERIALIZED (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
WITH cte AS NOT MATERIALIZED (SELECT random() AS r) SELECT (SELECT r FROM cte) = (SELECT r FROM cte);
```
S: `1`, `1`, `0` (a CTE used twice is materialized once, unless NOT MATERIALIZED). I: `0`, `0`, `0`.

### R36 (medium): generate_series with a negative step returns no rows
Case: 46 expressions/148. Area: table valued functions.
```
SELECT * FROM generate_series(10, 1, -4);
```
S: `10`, `6`, `2`. I: no rows.

### R37 (medium): An integer literal of 2147483648 or more in ORDER BY or GROUP BY is treated as a column number
Case: 9 aggregates/683. Area: binder.
```
SELECT 1 ORDER BY 2147483648;  SELECT count(*) FROM (VALUES (1),(2),(3)) GROUP BY 2147483648;
SELECT 1 UNION ALL SELECT 2 ORDER BY 2147483648;  SELECT 1 ORDER BY 9223372036854775807;
```
S: `1`; `3`; `1` (the literal is a constant expression); for the compound `1st ORDER BY term does not match any column in the result set`. I: `1st ORDER BY term out of range - should be between 1 and 1` for all four (the `1 ORDER BY 9223372036854775807` line differs only in the same way).

### R38 (medium): A window function in the ORDER BY of an EXISTS subquery fails
Case: 207 window-functions/96. Area: binder, window functions.
```
CREATE TABLE t1 (c1 INTEGER PRIMARY KEY, c2 INTEGER); INSERT INTO t1 VALUES (0,0),(1,1),(2,2);
SELECT COUNT(*) FROM t1 AS a WHERE EXISTS (SELECT 1 FROM t1 AS b WHERE a.c1 = b.c1 ORDER BY sum(a.c2) OVER (ORDER BY 0));
```
S: `3`. I: `Parse error: 1st ORDER BY term out of range - should be between 1 and 0`. The select list of the EXISTS subquery appears to be emptied before the ORDER BY is resolved.

### R39 (medium): Operands SQLite never evaluates are evaluated
Cases: 50 expressions/467, 160 subqueries/825. Area: executor.
```
SELECT coalesce(1, abs(-9223372036854775807 - 1));  SELECT iif(1, 1, abs(-9223372036854775807 - 1));
SELECT 0 AND abs(-9223372036854775807 - 1);
CREATE TABLE kx(x, t TEXT); INSERT INTO kx VALUES (1,'a'); SELECT EXISTS (SELECT json('bad') FROM kx);
```
S: `1`, `1`, `0`, `1`. I: `Error: integer overflow` for the first three and `Error: malformed JSON` for the EXISTS. The select list of an EXISTS subquery is never evaluated, and the right operand of AND, OR, coalesce and iif is skipped when the left decides the result.

### R40 (medium): BEFORE trigger NEW values do not have the column affinity applied
Cases: 164 triggers/171, 168 triggers/248. Area: triggers, affinity.
```
CREATE TABLE t(a REAL); CREATE TABLE log(msg);
CREATE TRIGGER tr BEFORE INSERT ON t BEGIN INSERT INTO log VALUES(typeof(NEW.a)); END; INSERT INTO t VALUES(42);
CREATE TABLE x(x INTEGER) STRICT; CREATE TRIGGER bi BEFORE INSERT ON x BEGIN INSERT INTO log VALUES(typeof(NEW.x)); END;
INSERT INTO x VALUES('1');
```
S logs `'real'` and `'integer'`. I logs `'integer'` and `'text'`. NEW.col in a BEFORE trigger already has the column affinity applied (including STRICT conversions).

### R41 (medium): Statement level OR REPLACE does not override a conflict clause in a trigger body
Case: 165 triggers/173. Area: triggers.
```
CREATE TABLE t1(a INTEGER PRIMARY KEY, v); CREATE TABLE t2(b INTEGER PRIMARY KEY, w);
CREATE TRIGGER tr AFTER INSERT ON t1 BEGIN INSERT INTO t2 VALUES(NEW.a, NEW.v); END;
INSERT INTO t1 VALUES(1,'first'); INSERT OR REPLACE INTO t1 VALUES(1,'second'); SELECT * FROM t2;
```
S: `1,'second'` (the statement level REPLACE applies to the trigger's INSERT). I: `UNIQUE constraint failed: t2.b` and `1,'first'`. With a trigger body of `INSERT OR IGNORE` S still replaces (`1,'second'`), I keeps `1,'first'`.

### R42 (medium): Rows skipped by RAISE(IGNORE) in a BEFORE DELETE trigger are still returned by RETURNING
Case: 174 triggers/783. Area: triggers.
```
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES (1,'a'),(2,'keep'),(3,'c');
CREATE TRIGGER td BEFORE DELETE ON t WHEN OLD.v = 'keep' BEGIN SELECT RAISE(IGNORE); END;
DELETE FROM t RETURNING id, v;
```
S: `1,'a'` and `3,'c'`. I: also `2,'keep'`.

### R43 (medium): RENAME COLUMN does not rewrite NEW.col in a TEMP trigger
Case: 172 triggers/702. Area: ALTER, triggers.
```
CREATE TABLE t(a, b); CREATE TEMP TRIGGER trg AFTER UPDATE ON t BEGIN SELECT NEW.a; END;
ALTER TABLE t RENAME COLUMN a TO a2; SELECT sql FROM sqlite_temp_master;
```
S: `... SELECT NEW.a2; END`. I: `... SELECT NEW.a; END` (the trigger now names a column that does not exist).

### R44 (medium): A column assigned twice in UPDATE SET, or listed twice in INSERT, is refused
Cases: 185 upsert/6, 186 upsert/10, 190 upsert/77. Area: binder.
```
CREATE TABLE t0 (c0 INT, c1 INT); INSERT INTO t0 VALUES (0,0),(1,1);
UPDATE t0 SET c0 = 1, c0 = c1, c1 = c0 + c1 + 3; SELECT * FROM t0;
CREATE TABLE a(a); INSERT INTO a(a, a) VALUES (1, 2);
```
S: `0,3` and `1,5` (the last assignment wins); the INSERT with a repeated column succeeds. I: `Parse error: column c0 is assigned twice` and `column a is named twice`. `UPDATE t0 SET (c0, c0) = (x'e715', NULL)` sets NULL in S.

### R45 (medium): ALTER TABLE ADD COLUMN accepts or refuses the wrong things
Cases: 11 alter/53, 12 alter/75, 14 alter/462, 16 alter/491, 17 alter/584, 18 alter/799, 19 alter/812, 71 generated-columns/285. Area: ALTER.
```
CREATE TABLE t1(name TEXT) STRICT; ALTER TABLE t1 ADD COLUMN id INTEGER DEFAULT 'corrupted';
CREATE TABLE t(a); ALTER TABLE t ADD COLUMN b INTEGER DEFAULT (a + 1);
ALTER TABLE t ADD COLUMN y COLLATE bogus;
ALTER TABLE t ADD COLUMN b DEFAULT 5 AS (a+1);
create table r(a); insert into r default values; alter table r add column g default "q";
```
S errors: `type mismatch on DEFAULT`; `default value of column [b] is not constant`; `no such collation sequence: bogus`; `error in generated column "b"`. I accepts all four (and stores the text `corrupted` in a STRICT INTEGER column). `ADD COLUMN g DEFAULT "q"` on a table that has rows works in S (g becomes 'q') and fails in I with `no such column: "q"`. Wording only: `ALTER TABLE t ADD COLUMN c CHECK((SELECT 1))` S prints `error in table t after add column: subqueries prohibited in CHECK constraints` as a run time error, I prints the short text as a parse error; a non constant default via ADD COLUMN is `Cannot add a column with non-constant default` in I and `default value of column [e] is not constant` in S.

### R46 (medium): Infinity prints as `Inf` in quote() and as `9e999` in JSON
Cases: 49 expressions/466, 128 json/612, 131 json/793, 135 numbers/391, 176 type-affinity/26, 177 type-affinity/46. Area: built in functions.
```
CREATE TABLE t(x REAL); INSERT INTO t VALUES(1e400);
SELECT quote(x), quote(-1e400), json_quote(1e400), json_array(1e999), json_valid(1e999) FROM t;
```
S: `'9.0e+999'`, `'-9.0e+999'`, `'9.0e+999'`, `'[9.0e+999]'`, `0`. I: `'Inf'`, `'-Inf'`, `'9e999'`, `'[9e999]'`, `1`. `typeof` and plain printing (`Inf`) agree. `quote()` output is what dump and copy tools write back, so the value does not round trip.

### R47 (medium): INSERT OR ROLLBACK with ON CONFLICT DO UPDATE rolls back on a later constraint error
Case: 196 upsert/272. Area: executor, upsert.
```
CREATE TABLE t(id INTEGER PRIMARY KEY, u TEXT UNIQUE, a TEXT UNIQUE); INSERT INTO t VALUES(1,'u1','a1'),(2,'u2','a2');
BEGIN; INSERT INTO t VALUES(3,'u3','a3');
INSERT OR ROLLBACK INTO t(u,a) VALUES('u1','a2') ON CONFLICT(u) DO UPDATE SET a=excluded.a;
SELECT count(*) FROM t WHERE id=3;
```
S: the statement fails with `UNIQUE constraint failed: t.a` and the transaction is still open: count is `1`. I: count is `0` (the whole transaction was rolled back). The DO UPDATE branch uses ABORT semantics.

### R48 (medium): DROP INDEX and DROP TABLE leave rows in sqlite_stat1
Case: 96 indexes/387. Area: DDL, ANALYZE.
```
CREATE TABLE t(x); CREATE INDEX i ON t(x); INSERT INTO t VALUES(1),(2),(3); ANALYZE; DROP INDEX i; SELECT count(*) FROM sqlite_stat1;
```
S: `0`. I: `1`. After DROP TABLE and ANALYZE the count is `0` in S and `1` in I, and a recreated index starts with the old `'3 1'` row.

### R49 (medium): INDEXED BY a partial index is refused where SQLite accepts it
Cases: 92 indexes/200, 97 indexes/487. Area: planner.
```
CREATE TABLE t(id INT, score INT, flag INT); INSERT INTO t VALUES(1,10,0),(3,30,1); CREATE INDEX idx ON t(score) WHERE flag = 1;
SELECT COUNT(*) FROM t INDEXED BY idx;
CREATE TABLE u(a,b); CREATE INDEX pi ON u(a) WHERE b > 0; DELETE FROM u INDEXED BY pi;
```
S: the count query returns a number and the DELETE runs. I: `Parse error: no query solution` for both.

### R50 (medium): Message "a rowid must be an integer, not Text([97])" instead of "datatype mismatch"
Cases: 0 affinity/rowid-text-lookup, 56 foreign-keys/446, 232 orm/5.26, 242 orm/9.31. Area: executor.
```
CREATE TABLE t(id INTEGER PRIMARY KEY, v); INSERT INTO t VALUES('x','d'); INSERT INTO t VALUES(5.5,'f');
```
S: `Error: datatype mismatch` for both. I: `Error: a rowid must be an integer, not Text([120])` and `...not Real(5.5)`. The text is a Rust debug dump, and ORMs match on `datatype mismatch`.

### R51 (medium): UNIQUE failure on an expression index prints an empty column list
Case: 238 orm/9.6. Area: constraints.
```
CREATE TABLE t(a); CREATE UNIQUE INDEX i ON t(lower(a)); INSERT INTO t VALUES('A'); INSERT INTO t VALUES('a');
```
S: `UNIQUE constraint failed: index 'i'`. I: `UNIQUE constraint failed: ` (nothing after the colon).

### R52 (medium): INSERT column and value count messages differ
Cases: 88 generated-columns/768, 190 upsert/77, 228 orm/4.22, 229 orm/4.24, 244 orm/9.40. Area: binder.
```
CREATE TABLE t(a,b); INSERT INTO t VALUES(1); INSERT INTO t(zz) VALUES(1);
```
S: `table t has 2 columns but 1 values were supplied` and `table t has no column named zz`. I: `1 values for 2 columns` and `no such column: zz`. Frameworks parse these two messages.

### R53 (medium): Parser: AUTOINCREMENT in a table level PRIMARY KEY and a quoted type name
Cases: 140 schema/279, 141 schema/282. Area: parser.
```
CREATE TABLE v(a INTEGER, PRIMARY KEY(a AUTOINCREMENT));
CREATE TABLE t(a "INTEGER" PRIMARY KEY AUTOINCREMENT);  CREATE TABLE u(a 'INTEGER' PRIMARY KEY);
```
S accepts all three (and `PRIMARY KEY(a, b AUTOINCREMENT)` fails with `AUTOINCREMENT is only allowed on an INTEGER PRIMARY KEY`). I: `near "AUTOINCREMENT": syntax error` for the first and `near "'INTEGER'": syntax error` for the third, so the later INSERTs report `no such table`.

### R54 (medium, package 5): JSON functions on a BLOB holding JSON text
Cases: 118 json/65, 127 json/611, 132 json/816. Area: JSON.
```
SELECT json_valid(x'7b7d'), json(x'7B2261223A317D'), json_type(x'7B7D'), json_valid(1e999);
```
S: `1`, `'{"a":1}'`, `'object'`, `0`. I: `0`, `Error: malformed JSON`, and `json_valid(1e999)` is `1`.

### R55 (medium, package 5): JSON functions: NULL path, argument counts, bracket paths
Cases: 119 json/142, 120 json/203, 130 json/704, 214 json/errors, 236 orm/7.11. Area: JSON.
```
SELECT json_set('{"a":1}', NULL, 2);  SELECT json_set('{}');  SELECT json_remove('{}');
SELECT json_extract('{"a":1}', '$.a[');  SELECT json_extract('{"key with spaces":1}', '$["key with spaces"]');
SELECT json_insert('{}', '$.a');  SELECT json_object('a');
```
S: `'{"a":1}'`, `'{}'`, `'{}'`, `NULL`, `NULL`, `Error: json_insert() needs an odd number of arguments`, `Error: json_object() requires an even number of arguments`. I: `bad JSON path: 'NULL'`, `Parse error: wrong number of arguments to function json_set()`, the same for json_remove, `bad JSON path: '$.a['`, `bad JSON path: '$["key with spaces"]'`, and the wrong number of arguments parse error for json_insert and json_object. `json_set('{"a":1}','$.a',NULL)` gives `{"a":null}` in S and an error in I.

### R56 (medium, package 5): Date and time functions
Cases: 2 datetime/fixed-inputs, 3 datetime/invalid-and-edge-inputs, 42 date-and-time/384, 43 date-and-time/616, 44 date-and-time/715, 45 date-and-time/745. Area: built in functions.
```
SELECT datetime('2024-05-15', '+0001-02-03 04:05:06');  SELECT date('-0001-01-01'), julianday('-4713-11-24 12:00:00');
SELECT date(CAST('2024-01-01' AS BLOB));  SELECT timediff('2000-01-31', '2000-03-02');
SELECT datetime('2000-03-02', timediff('2000-01-31', '2000-03-02'));
SELECT datetime('2026-04-07T16:00:00 +01:00'), datetime('2026-04-07T16:00:00+25:00'), datetime('2026-04-07T16:00:00+01:60');
select count(distinct v) from (select datetime('subsecond') v from generate_series(1, 100));
```
S: `'2025-07-18 04:05:06'`; `'-0001-01-01'`, `0.0`; `'2024-01-01'`; `'-0000-01-02 00:00:00.000'`; `'2000-01-31 00:00:00'`; `'2026-04-07 15:00:00'`, `NULL`, `NULL`; `1`. I: `NULL`; `NULL`, `NULL`; `NULL`; `'-0000-01-00 00:00:00.000'`; `NULL`; `NULL`, `'2026-04-06 15:00:00'`, `'2026-04-07 14:00:00'`; `0`. Differences: negative years, the `+YYYY-MM-DD HH:MM:SS` modifier, a BLOB as input (also in a CHECK constraint), timediff for a negative interval, a space before the zone offset, offset limits, and a lone `'subsecond'` argument.

### R57 (medium, package 5): Error message wording
Cases (message text or error class is the only difference, or the wording is the main difference): 1, 10, 18, 19, 20, 23, 29, 38, 47, 59, 72, 74, 87, 88, 89, 94, 95, 113, 121, 136, 139, 142, 150, 154, 159, 161, 163, 166, 167, 173, 198, 201, 202, 203, 204, 215, 216, 220, 222, 225, 226, 227, 233, 234, 235, 240, 241, 245, 246, 247, 249, 250, 251, 254, 255, 256, 259, 260. Area: many.

Families, each with one example (S then I):
- Messages that start with `unsupported:` in I: `RAISE() may only be used within a trigger-program` against `unsupported: RAISE outside a trigger` (1, 74, 166); `circular reference: t1` against `unsupported: circular reference in a CTE` (38); `SELECTs to the left and right of UNION do not have the same number of result columns` against `... of a compound operator ...` (167, 245, 250); `all VALUES must have the same number of terms` against `unsupported: all VALUES rows must have the same width` (227); `sub-select returns 2 columns - expected 1` against `unsupported: sub-select returns more than one column` (201, 246, 249); `cannot use RETURNING in a trigger` against `unsupported: RETURNING is not available in triggers` (163); `VACUUM INTO` with an expression (161, also an accepted statement in S).
- A table name is missing or extra: STRICT and generated column messages use `t.a` in S: `unknown datatype for t.a: "DATETIME"`, `missing datatype for t.a`, `unknown datatype for c.a: "jsonb"` against `... for a ...` (121, 233, 234, 254, 255); `generated column loop on "v"` and `generated columns cannot be part of the PRIMARY KEY` against `generated column loop on v` and `...PRIMARY KEY: v` (89, 256).
- Qualified names: `no such column: c.v` against `no such table: c` (20); `no such table: temp.important_data` against `no such table: important_data` (29); `no such column: temp.a.id` against `no such column: a.id` (113); `no such column: other.val`, `new.amount`, `excluded.nosuch`, `main.users.id`, `other.id`, `t.n`, `z.x` against `no such table: other` / `new` / `nosuch` (198, 202, 203, 204, 226, 251); `no such table: aux.t` against `no such table: t` (259); `cannot join using column nosuch - column not present in both tables` against `no such column: nosuch` (251); rename column breaking a trigger: `no such column: t.b` against `no such table: main.t` (173).
- Tokenizer messages: `unrecognized token: "#"` against `unrecognized token` (240); `unrecognized token: "'abc;"` against `unrecognized token: unterminated quoted name` (241); `unrecognized token: "@"` against `malformed parameter` (235); `unrecognized token: "x'2061 20'"` against `malformed blob literal` (150); `unrecognized token: "1._5"` against `malformed numeric literal` (136).
- Run time against parse time: `MATCH` misuse (47), `foreign_key_check(missing)` (59), RAISE in a generated column (74), `ALTER` errors (19, 18) are `Error` in one shell and `Parse error` in the other; DETACH inside a transaction `database aux is locked` against `cannot DETACH database within transaction` (23, 222).
- Other statements with different text: `NULLS FIRST` in CREATE INDEX is `unsupported use of NULLS FIRST` in S and `near "NULLS": syntax error` in I (94, 95); `ORDER BY clause should come after UNION not before` and `LIMIT clause should come after UNION not before` against `near "UNION": syntax error` (159); a column type after a generated column expression, `CREATE TABLE t(a,b AS (a) WAT)`, is `error in generated column "b"` against `near "WAT": syntax error` (72); `ALTER TABLE t DROP COLUMN a` that would leave only generated columns is `must have at least one non-generated column` against `no such column: a` (87).
- Names: `no such function: ROLLUP` against `rollup` (S keeps the case typed, case 10); `table sqlite_master may not be dropped` against `sqlite_schema` (142); `table sqlite_master may not be modified` against `writing to sqlite_schema needs PRAGMA writable_schema = ON` (225); `too many columns on w` against `too many columns on table exceeded` (215); `too many terms in compound SELECT` (216); `database or disk is full` against `t has handed out every AUTOINCREMENT key` (220); `unable to identify the object to be reindexed` against `near "unable to identify the object to be reindexed: nosuch": syntax error` (260); `no such table: main.x` against `object name reserved for internal use: sqlite_tr` (139, the order of two checks); `cannot store INT value in BLOB column` against `INTEGER` (154).

### R58 (medium, package 5): Shell output
Cases: 52 expressions/833, 124 json/374, 138 other/784, 175 triggers/834, 223 orm/2.11, 237 orm/7.25, 253 schema/create-table-sql-preserved, 257 schema/create-table-as, 258 text/trim-family (quote mode prints text with control characters raw, S prints `unistr('...\u000a...')`); 51 expressions/560 (error near line counts from the first line of a statement that begins with a comment; S counts from the line where the statement text starts); 224 orm/2.17, 231 orm/4.60, 239 orm/9.11, 243 orm/9.39 (after a parse error S abandons the rest of the line, and S prints one error where several statements on one line fail); 248 params/named-forms. Area: shell.
```
.parameter set :a 1
.parameter set ?4 NULL
.parameter set ?1 'one'
SELECT :a, ?4, ?1;
```
S: `1`, `NULL`, `1` (`:a` and `?1` are parameter 1, so the value set by name `:a` is used, and NULL is the SQL NULL). I: `'one'`, `'NULL'`, `'one'`.

---

## Low

### R59 (low, package 5): JSON edge cases
Cases: 122 json/334, 125 json/382, 129 json/667. Area: JSON.
S accepts `json(char(0x2028)||...)` and other JSON5 whitespace (U+2000, U+2028, U+2029); I returns `malformed JSON`. `json_extract('["a\ud800b"]','$[0]')` keeps the lone surrogate bytes `EDA080` in S and writes `EFBFBD` in I. `json_tree('{"a":{"b":1},"c":2}','$.a')` has ids `1,NULL` and `4,1` in S and `3,NULL` and `4,3` in I.

### R60 (low): Window definitions that extend a named window are not validated
Cases: 208 window-functions/202, 212 window-functions/590. Area: binder, window functions.
```
CREATE TABLE t(val INT); INSERT INTO t VALUES(1),(2),(3);
SELECT SUM(val) OVER (w ORDER BY val) FROM t WINDOW w AS (ORDER BY val);
```
S: `cannot override ORDER BY clause of window: w`. I runs the query. S also rejects `w PARTITION BY` when w has a PARTITION BY, and a frame when w has a frame (`cannot override PARTITION clause of window: w`, `cannot override frame specification of window: w`).

### R61 (low): Text with NUL bytes or invalid UTF-8
Cases: 41 ctes/579, 149 string-functions/182, 152 string-functions/676, 178 type-affinity/149, 183 type-affinity/630, 184 type-affinity/785. Area: built in functions.
```
SELECT hex(substr('a' || char(0) || 'b', 3)), length(x'AB' || 'text'), length(CAST(X'80' AS TEXT)), hex(char(55296));
```
S: `''` (text functions stop at the first NUL), `5`, `1`, `'EDA080'`. I: `'62'`, `4`, `0`, `'EFBFBD'`. substr, length and the character count of invalid UTF-8 byte sequences differ.

### R62 (low): atanh differs from SQLite in the last digits
Cases: 133, 134, 137 (numbers-and-numeric-form). Area: built in functions.
```
SELECT atanh(tanh(-1.0)), (trunc(atanh(tanh(-2.0))));
```
S: `-0.99999999999999989` and `-2.0`. I: `-1.0` and `-1.0`. The result of the other functions (tanh, asinh, exp, sin) matches.

### R63 (low): Numeric literals with `_` separators
Case: 136 numbers/492. Area: tokenizer.
```
SELECT 0x1_0, 1.5_5, 1_0.5;
```
S: `16`, `1.55`, `10.5`. I: `Parse error: unrecognized token: malformed numeric literal` for some, and `1.5` for `1.5_5` (the `_5` is read as an alias).

### R64 (low): CREATE TABLE AS leaves out the type of a CAST column
Case: 138 other/784. Area: DDL.
`CREATE TABLE u AS SELECT CAST(1 AS TEXT) AS c` stores `c TEXT` in S and `c` in I.

### R65 (low): EXPLAIN QUERY PLAN text and plan choice
Cases: 21 attach/204, 24 attach/228, 40 ctes/380, 99 indexes/697, 101 indexes/735, 112 joins/464, 156 subqueries/370, 221 orm/11.26. Area: planner.
Examples: `SEARCH db2.t USING COVERING INDEX idx_t` (S) against `SEARCH t ...` (I) for an attached schema; a WITHOUT ROWID table ordered by a secondary index is `SCAN catalog USING COVERING INDEX catalog_score` in S and a temp b-tree sort in I; a CTE used twice is `MATERIALIZE` with an automatic covering index in S and two scans in I; a WHERE over a UNION ALL subquery is pushed into the arms in S (`COMPOUND QUERY ... SEARCH t USING INDEX ia (a=?)`) and `SCAN subquery` in I (see R23); a partial index used for `_deleted = false` is `COVERING` in S for the first plan and a `SCAN` for `= false` in the second.

### R66 (low): AUTOINCREMENT bookkeeping in sqlite_sequence
Cases: 153 string-functions/689, 189 upsert/66, 200 upsert/534, 206 upsert/790. Area: executor.
- `UPDATE sqlite_sequence SET seq='5abc'` then an insert: S uses the numeric prefix (next id 6), I ignores it (id 2).
- `INSERT OR IGNORE INTO t(id,val) VALUES(10,'x')` that fails a CHECK: S records seq 10, I keeps 1.
- `INSERT OR IGNORE` that hits a conflict: S advances seq to 2 (next id 3), I stays at 1 (next id 2).
- `INSERT INTO b(x) SELECT seq FROM sqlite_sequence` on a new AUTOINCREMENT table inserts no row, but S still creates the row `'b',0` in sqlite_sequence.
- Setting seq to 9223372036854775807 and inserting gives `database or disk is full` in S (case 153, see R57 for the message of case 220).

### R67 (low): INSERT ... AS excluded alias, and last_insert_rowid() inside RETURNING
Cases: 198 upsert/456, 192 upsert/110. Area: upsert, DML.
`INSERT INTO u AS excluded VALUES(1,99) ON CONFLICT(k) DO UPDATE SET n = excluded.n` leaves n at 10 in S (the alias names the existing row) and sets 99 in I. `INSERT INTO t1 VALUES(20,'b') RETURNING last_insert_rowid()` returns 20 in S and 10 in I.

### R68 (low): Statements SQLite refuses at CREATE time are accepted
Cases: 73 generated-columns/287, 90 generated-columns/786, 91 indexes/55, 93 indexes/263, 106 joins/336, 108 joins/341, 151 string-functions/626, 155 string-functions/787, 161 transactions/665, 209 window-functions/383. Area: DDL and query validation.
- Two `AS (...)` clauses on one generated column are accepted (S: `error in generated column "c"`); a generated column in `PRIMARY KEY(a, v)` is accepted (S: `generated columns cannot be part of the PRIMARY KEY`).
- `CREATE INDEX i ON t(sqlite_version())` and `ON t(random())`: S `non-deterministic functions prohibited in index expressions`; `ON t(t.a + 1)`: S `the "." operator prohibited in index expressions`; also a subquery or parameter in an index expression.
- `SELECT * FROM a JOIN b ON a.x = c.z JOIN c ON ...`: S `ON clause references tables to its right`.
- A join of 65 tables: S `at most 64 tables in a join`; I runs it.
- `'test' LIKE <pattern of 135000 characters>`: S `LIKE or GLOB pattern too complex`.
- `likelihood(1, 2)`: S `second argument to likelihood() must be a constant between 0.0 and 1.0`.
- A window function in a column DEFAULT: S `default value of column [v] is not constant`.
- `VACUUM INTO (SELECT n FROM p)` and `VACUUM INTO 'a' || 'b'`: S accepts an expression, I answers `unsupported: VACUUM INTO with a file name that is not a literal`.

### R69 (low): Stored CREATE INDEX text loses a trailing space
Case: 175 triggers/834. Area: DDL.
`CREATE INDEX   i  ON t ( a ,  b  DESC ) ;` is stored by S as `CREATE INDEX i  ON t ( a ,  b  DESC ) ` (the space before the semicolon is kept) and by I without it. The trigger text difference in the same case is the shell unistr issue (R58).

### R70 (low): PRAGMA reverse_unordered_selects is ignored
Case: 144 schema/804. Area: PRAGMA.
After `PRAGMA reverse_unordered_selects = ON`, `SELECT a FROM t` returns 3,2,1 in S and 1,2,3 in I.

### R71 (low): A run time error discards rows the statement had already returned
Case: 213 json/each-and-tree. Area: executor, shell output.
```
CREATE TABLE t(id INTEGER PRIMARY KEY, tags TEXT); INSERT INTO t VALUES(1,'["red","blue"]'),(2,'["blue"]'),(3,'[]'),(4,NULL),(5,'bad');
SELECT id FROM t WHERE EXISTS (SELECT 1 FROM json_each(t.tags) WHERE value = 'blue') ORDER BY id;
```
S: `1`, `2`, then `Error: malformed JSON` (row 5 fails after rows 1 and 2 were printed). I: only the error. Without ORDER BY the rows are streamed in S.

---

## Not defects

### N1: Engine configuration defaults and page layout
Cases: 27 attach/532, 28 attach/533, 98 indexes/648, 100 indexes/724, 143 schema/646, 145 schema/821, 147 schema/837, 148 string-functions/5, 205 upsert/733, 217 orm/1.9, 218 orm/1.14, 219 orm/1.15.
inillucent uses a 32768 byte page where SQLite uses 4096, a default cache_size of -131072 where SQLite uses -2000, a default busy timeout of 5000 where SQLite uses 0, creates 4 pages for an empty database where SQLite creates 0, rejects absurd cache_size values with a message about pool frames, ignores `PRAGMA page_size = N` set before the first write, and reports its own page counts, freelist counts, rootpage numbers (`sqlite_schema.rootpage` 4 against 2) and `max_page_count` after a failed insert. All of these depend on the page size and cache configuration, which is a decision for the project and not a bug in one statement. Case 205 also shows `database or disk is full` coming from a smaller page count in S.

### N2: UPDATE and DELETE with ORDER BY or LIMIT
Cases: 4 dml/delete-update-limit-refused, 230 orm/4.44.
The pinned sqlite3 shell is built without SQLITE_ENABLE_UPDATE_DELETE_LIMIT and answers `near "LIMIT": syntax error`. inillucent accepts the statement. Whether inillucent should refuse it to match this build is a project decision.

### N3: Row order where SQLite does not define one
Cases: 102 indexes/795, 211 window-functions/499.
- 102: `SELECT a FROM t` on `CREATE TABLE t(a PRIMARY KEY DESC) WITHOUT ROWID` returns 3,2,1 in S and 1,2,3 in I. There is no ORDER BY, so both orders are legal. Every ORDER BY query in the case matches. (An application that expects the stored order of a WITHOUT ROWID table to follow a DESC key will notice, but SQLite does not promise it.)
- 211: `SELECT k, sum(v), rank() OVER (ORDER BY sum(v) DESC) FROM w GROUP BY k` where both groups have sum 30 returns the groups in a different order. The two rows tie on the window sort key.

---

## Case to group table

| Index | Case | Groups |
|---|---|---|
| 0 | affinity/rowid-text-lookup | R50 |
| 1 | trigger/raise-forms | R57 |
| 2 | datetime/fixed-inputs | R56 |
| 3 | datetime/invalid-and-edge-inputs | R56 |
| 4 | dml/delete-update-limit-refused | N2 |
| 5 | holdout/aggregates/80-sum-whose-partial-sums-overflow-to-infinity | R7 |
| 6 | holdout/aggregates/115-avg-over-the-text-returned-by-hex | R7 |
| 7 | holdout/aggregates/588-bare-columns-next-to-max-and-min-with-group-by | R30 |
| 8 | holdout/aggregates/674-bare-columns-with-max-and-min-over-groups | R30 |
| 9 | holdout/aggregates/683-a-huge-integer-literal-in-order-by-or-group-by-is- | R37 |
| 10 | holdout/aggregates/820-unknown-functions-in-group-by-and-having-are-error | R57 |
| 11 | holdout/alter-table-and-schema-c/53-add-column-with-a-default-of-the-wrong-type-in-a-s | R45 |
| 12 | holdout/alter-table-and-schema-c/75-add-column-with-an-unknown-collation | R45 |
| 13 | holdout/alter-table-and-schema-c/407-add-column-keeps-desc-and-collate-inside-table-lev | R20, R21 |
| 14 | holdout/alter-table-and-schema-c/462-add-column-with-a-default-whose-type-differs-from- | R45 |
| 15 | holdout/alter-table-and-schema-c/463-rename-column-with-collate-in-a-table-level-unique | R20 |
| 16 | holdout/alter-table-and-schema-c/491-add-column-with-a-non-constant-default-on-an-empty | R45 |
| 17 | holdout/alter-table-and-schema-c/584-add-column-with-an-unquoted-identifier-as-default | R45 |
| 18 | holdout/alter-table-and-schema-c/799-add-column-with-non-constant-default-expressions-o | R45, R57 |
| 19 | holdout/alter-table-and-schema-c/812-add-column-with-a-check-that-contains-a-subquery | R45, R57 |
| 20 | holdout/attach-and-temp-schemas/76-with-clause-attached-to-an-update-whose-set-refers | R57 |
| 21 | holdout/attach-and-temp-schemas/204-indexed-by-an-index-that-lives-in-an-attached-data | R65 |
| 22 | holdout/attach-and-temp-schemas/226-view-in-an-attached-database-resolves-unqualified- | R33 |
| 23 | holdout/attach-and-temp-schemas/227-detach-while-a-transaction-or-savepoint-is-open | R57 |
| 24 | holdout/attach-and-temp-schemas/228-query-plan-uses-the-index-of-an-attached-database | R65 |
| 25 | holdout/attach-and-temp-schemas/473-a-view-in-main-reads-the-table-of-its-own-database | R27 |
| 26 | holdout/attach-and-temp-schemas/486-temp-view-over-a-table-of-an-attached-database | R33 |
| 27 | holdout/attach-and-temp-schemas/532-pragma-temp-freelist-count-reports-the-temp-databa | N1 |
| 28 | holdout/attach-and-temp-schemas/533-pragma-aux-page-size-changes-only-the-attached-dat | N1 |
| 29 | holdout/attach-and-temp-schemas/672-temp-qualifier-is-a-separate-database-from-main | R57 |
| 30 | holdout/attach-and-temp-schemas/740-pragma-schema-auto-vacuum-applies-to-the-named-dat | R34 |
| 31 | holdout/attach-and-temp-schemas/789-a-persistent-view-that-references-an-attached-data | R33 |
| 32 | holdout/collation/243-collation-of-the-first-arm-of-a-three-arm-compound | R22 |
| 33 | holdout/collation/246-order-by-1-collate-nocase-and-group-by-1-collate-n | R19 |
| 34 | holdout/collation/303-table-level-primary-key-with-a-collate-inside-the- | R20 |
| 35 | holdout/collation/469-in-uses-the-collation-of-the-left-operand | R31 |
| 36 | holdout/collation/470-row-value-comparisons-use-column-collation-and-aff | R31, R32 |
| 37 | holdout/collation/597-union-and-intersect-deduplicate-with-the-collation | R22 |
| 38 | holdout/ctes/187-cte-named-like-a-table-that-reads-the-schema-quali | R57 |
| 39 | holdout/ctes/252-not-materialized-cte-is-evaluated-at-each-referenc | R35 |
| 40 | holdout/ctes/380-explain-query-plan-text-for-a-materialized-cte-ref | R65 |
| 41 | holdout/ctes/579-length-stops-at-the-first-nul-character-for-text | R61 |
| 42 | holdout/date-and-time/384-date-functions-accept-a-blob-holding-the-text-of-a | R56 |
| 43 | holdout/date-and-time/616-now-is-evaluated-once-per-statement | R56 |
| 44 | holdout/date-and-time/715-timediff-and-datetime-b-timediff-a-b | R56 |
| 45 | holdout/date-and-time/745-julianday-accepts-rfc-3339-timestamps-with-z-and-o | R56 |
| 46 | holdout/expressions/148-generate-series-with-negative-bounds | R36 |
| 47 | holdout/expressions/178-match-on-a-regular-table-column | R57 |
| 48 | holdout/expressions/425-order-by-an-expression-over-a-name-that-is-both-an | R19 |
| 49 | holdout/expressions/466-strict-table-columns-reject-nan-and-inf-in-non-rea | R46 |
| 50 | holdout/expressions/467-and-or-in-the-select-list-skip-the-operand-sqlite- | R39 |
| 51 | holdout/expressions/560-a-line-comment-ends-at-the-end-of-the-line | R58 |
| 52 | holdout/expressions/833-check-constraint-of-a-create-table-that-begins-wit | R58 |
| 53 | holdout/foreign-keys/99-fk-on-delete-cascade-and-on-update-cascade-honour- | R14 |
| 54 | holdout/foreign-keys/120-update-or-replace-deletes-another-row-and-cascades | R16 |
| 55 | holdout/foreign-keys/416-on-delete-cascade-with-a-child-column-that-is-noca | R14 |
| 56 | holdout/foreign-keys/446-insert-or-fail-undoes-earlier-rows-of-the-statemen | R50 |
| 57 | holdout/foreign-keys/479-multi-row-insert-with-a-self-referencing-foreign-k | R15 |
| 58 | holdout/foreign-keys/515-add-column-keeps-quotes-around-a-keyword-column-na | R21 |
| 59 | holdout/foreign-keys/817-foreign-key-check-of-a-table-that-does-not-exist | R57 |
| 60 | holdout/generated-columns/208-delete-trigger-on-a-table-that-has-a-virtual-gener | R13 |
| 61 | holdout/generated-columns/211-collate-on-a-virtual-generated-column-applies-in-w | R10 |
| 62 | holdout/generated-columns/212-replace-where-a-virtual-generated-column-is-unique | R11 |
| 63 | holdout/generated-columns/214-foreign-key-whose-parent-key-is-a-unique-virtual-g | R12 |
| 64 | holdout/generated-columns/215-foreign-key-on-a-virtual-generated-child-column-is | R12 |
| 65 | holdout/generated-columns/216-update-on-a-table-with-a-unique-virtual-generated- | R11 |
| 66 | holdout/generated-columns/217-not-null-on-a-virtual-generated-column-is-enforced | R11 |
| 67 | holdout/generated-columns/219-strict-table-checks-the-declared-type-of-a-generat | R9, R10 |
| 68 | holdout/generated-columns/221-parent-key-that-is-a-generated-column-backed-by-a- | R12 |
| 69 | holdout/generated-columns/254-unique-generated-column-whose-expression-uses-anot | R11 |
| 70 | holdout/generated-columns/258-update-keeps-an-index-over-a-chained-virtual-gener | R9 |
| 71 | holdout/generated-columns/285-alter-table-add-column-with-both-default-and-as | R45 |
| 72 | holdout/generated-columns/286-unknown-type-tokens-after-a-generated-column-expre | R57 |
| 73 | holdout/generated-columns/287-two-generated-clauses-on-one-column | R68 |
| 74 | holdout/generated-columns/289-raise-and-other-special-expressions-in-a-generated | R57 |
| 75 | holdout/generated-columns/290-outer-join-null-extension-of-a-virtual-generated-c | R13 |
| 76 | holdout/generated-columns/291-seek-on-an-indexed-generated-column-uses-the-colum | R9, R10 |
| 77 | holdout/generated-columns/294-unique-index-on-a-generated-int-column-compares-va | R9, R10, R11 |
| 78 | holdout/generated-columns/295-fk-to-a-generated-column-that-has-a-unique-index | R12 |
| 79 | holdout/generated-columns/296-integrity-check-on-a-strict-table-reports-a-genera | R9, R10 |
| 80 | holdout/generated-columns/297-on-conflict-target-that-names-a-unique-index-on-a- | R11 |
| 81 | holdout/generated-columns/521-upsert-do-update-checks-not-null-on-a-virtual-gene | R11 |
| 82 | holdout/generated-columns/522-upsert-do-update-checks-a-child-foreign-key-that-i | R12 |
| 83 | holdout/generated-columns/523-after-update-trigger-computes-new-virtual-columns- | R13 |
| 84 | holdout/generated-columns/524-update-checks-not-null-on-a-generated-column-after | R11 |
| 85 | holdout/generated-columns/526-parent-key-built-from-two-virtual-columns-in-forei | R12 |
| 86 | holdout/generated-columns/678-not-null-on-a-virtual-generated-column-is-enforced | R11 |
| 87 | holdout/generated-columns/759-drop-column-that-would-leave-only-a-generated-colu | R57 |
| 88 | holdout/generated-columns/768-update-and-insert-of-generated-columns-are-refused | R52, R57 |
| 89 | holdout/generated-columns/769-a-generated-column-cannot-be-a-primary-key-or-have | R57 |
| 90 | holdout/generated-columns/786-generated-columns-as-part-of-a-primary-key-on-a-ro | R68 |
| 91 | holdout/indexes/55-expression-index-over-a-non-deterministic-function | R68 |
| 92 | holdout/indexes/200-indexed-by-a-partial-index-that-cannot-answer-the- | R49 |
| 93 | holdout/indexes/263-qualified-column-names-in-an-index-expression | R68 |
| 94 | holdout/indexes/284-nulls-first-nulls-last-are-not-accepted-in-create- | R57 |
| 95 | holdout/indexes/337-nulls-first-in-create-index-is-accepted-by-the-par | R57 |
| 96 | holdout/indexes/387-drop-index-and-drop-table-remove-their-sqlite-stat | R48 |
| 97 | holdout/indexes/487-indexed-by-a-partial-index-when-the-where-implies- | R49 |
| 98 | holdout/indexes/648-a-table-with-a-65536-byte-page-size-and-index-entr | N1 |
| 99 | holdout/indexes/697-secondary-index-on-a-without-rowid-table | R65 |
| 100 | holdout/indexes/724-pragma-max-page-count-and-index-balancing | N1 |
| 101 | holdout/indexes/735-a-bound-parameter-or-the-literals-false-and-0-agai | R65 |
| 102 | holdout/indexes/795-natural-scan-order-of-a-without-rowid-table-follow | N3 |
| 103 | holdout/joins/152-correlated-subquery-in-a-left-join-on-clause-that- | R2 |
| 104 | holdout/joins/315-right-join-whose-on-contains-a-correlated-in-subqu | R2 |
| 105 | holdout/joins/316-left-join-whose-on-is-a-correlated-exists | R2 |
| 106 | holdout/joins/336-on-clause-refers-to-a-table-that-appears-later-in- | R68 |
| 107 | holdout/joins/340-right-and-full-outer-join-with-on-0-on-true-and-us | R1 |
| 108 | holdout/joins/341-a-join-of-64-tables-is-accepted-65-is-rejected | R68 |
| 109 | holdout/joins/346-where-on-a-table-before-a-full-join-is-applied-aft | R1 |
| 110 | holdout/joins/358-parenthesized-join-in-the-from-clause | R8 |
| 111 | holdout/joins/359-correlated-subquery-in-a-left-join-on-clause-that- | R2 |
| 112 | holdout/joins/464-predicate-on-the-right-side-of-a-left-join-subquer | R65 |
| 113 | holdout/joins/472-schema-qualified-column-reference-in-where | R57 |
| 114 | holdout/joins/510-in-subquery-in-the-on-clause-of-a-second-left-join | R2 |
| 115 | holdout/joins/513-not-in-subquery-in-a-left-join-on-clause-that-refe | R2 |
| 116 | holdout/joins/716-right-join-following-another-join | R1 |
| 117 | holdout/joins/719-text-column-compared-with-a-column-of-a-cte-or-vie | R23 |
| 118 | holdout/json/65-json-valid-on-a-blob-that-holds-text-bytes-and-on- | R54 |
| 119 | holdout/json/142-json-set-json-insert-json-remove-with-a-null-docum | R55 |
| 120 | holdout/json/203-bracket-quoted-key-in-a-json-path | R55 |
| 121 | holdout/json/325-jsonb-blob-stored-in-a-strict-table-column-declare | R57 |
| 122 | holdout/json/334-json5-whitespace-and-line-continuations-accepted-b | R59 |
| 123 | holdout/json/368-in-constraint-on-generate-series-and-json-each-hid | R6 |
| 124 | holdout/json/374-json-each-returns-decoded-sql-text-for-json-string | R58 |
| 125 | holdout/json/382-unpaired-surrogate-escapes-in-json-strings | R59 |
| 126 | holdout/json/397-json-each-over-a-null-column-yields-no-rows | R5 |
| 127 | holdout/json/611-json-on-a-blob-argument | R54 |
| 128 | holdout/json/612-json-object-with-an-infinite-real | R46 |
| 129 | holdout/json/667-the-id-and-parent-columns-of-json-tree-and-json-ea | R59 |
| 130 | holdout/json/704-json-insert-and-json-replace-need-an-odd-number-of | R55 |
| 131 | holdout/json/793-json-array-1e999-is-accepted | R46 |
| 132 | holdout/json/816-json-valid-and-json-type-of-integer-and-real-sql-v | R54 |
| 133 | holdout/numbers-and-numeric-form/59-trunc-atanh-tanh-2-mod-2-0-2-0-2-2-0-turso-3-0 | R62 |
| 134 | holdout/numbers-and-numeric-form/107-atanh-exp-degrees-sin-chain-last-digits-of-the-res | R62 |
| 135 | holdout/numbers-and-numeric-form/391-quote-of-an-infinite-real | R7, R46 |
| 136 | holdout/numbers-and-numeric-form/492-numeric-literals-with-underscores | R57, R63 |
| 137 | holdout/numbers-and-numeric-form/544-mod-atanh-tanh-1-1-divided-by-an-expression-involv | R62 |
| 138 | holdout/other/784-create-table-as-select-keeps-the-storage-class-of- | R58, R64 |
| 139 | holdout/schema/138-create-view-with-a-name-starting-with-sqlite | R57 |
| 140 | holdout/schema/279-autoincrement-on-a-column-that-is-not-an-integer-p | R53 |
| 141 | holdout/schema/282-quoted-type-name-integer-before-primary-key-autoin | R53 |
| 142 | holdout/schema/577-tables-with-the-sqlite-prefix-cannot-be-created-or | R57 |
| 143 | holdout/schema/646-pragma-cache-size-at-the-minimum-64-bit-integer | N1 |
| 144 | holdout/schema/804-pragma-reverse-unordered-selects-reverses-scans-wi | R70 |
| 145 | holdout/schema/821-readback-of-settings-pragmas | N1 |
| 146 | holdout/schema/832-create-table-as-select-into-a-table-name-that-cont | R28 |
| 147 | holdout/schema/837-default-cache-size-and-cache-size-defaults | N1 |
| 148 | holdout/string-functions/5-sqlite-schema-stores-the-unquoted-name-and-the-sql | N1 |
| 149 | holdout/string-functions/182-unicode-char-0-is-null | R61 |
| 150 | holdout/string-functions/581-trim-removes-spaces-only-by-default | R57 |
| 151 | holdout/string-functions/626-like-with-a-very-long-pattern | R68 |
| 152 | holdout/string-functions/676-substr-like-and-glob-on-text-with-embedded-nul-byt | R61 |
| 153 | holdout/string-functions/689-sqlite-sequence-seq-edited-to-text-or-to-the-maxim | R66 |
| 154 | holdout/string-functions/767-strict-text-accepts-integers-and-reals-as-text | R57 |
| 155 | holdout/string-functions/787-likelihood-likely-and-unlikely-validate-their-argu | R68 |
| 156 | holdout/subqueries/370-outer-where-applied-to-a-union-all-subquery | R65 |
| 157 | holdout/subqueries/500-a-multi-column-subquery-as-the-left-side-of-in | R32 |
| 158 | holdout/subqueries/511-outer-aggregate-used-inside-a-correlated-subquery- | R29 |
| 159 | holdout/subqueries/712-order-by-before-a-compound-operator | R57 |
| 160 | holdout/subqueries/825-exists-does-not-evaluate-its-result-list-and-an-ag | R39 |
| 161 | holdout/transactions/665-vacuum-into-takes-an-expression | R57, R68 |
| 162 | holdout/triggers/87-replace-into-fires-delete-triggers-only-with-recur | R16 |
| 163 | holdout/triggers/170-returning-inside-a-trigger-body | R57 |
| 164 | holdout/triggers/171-before-insert-trigger-sees-new-values-after-column | R40 |
| 165 | holdout/triggers/173-statement-level-or-replace-overrides-conflict-clau | R41 |
| 166 | holdout/triggers/174-raise-with-a-non-literal-message-expression | R57 |
| 167 | holdout/triggers/205-trigger-body-that-fails-to-compile-after-add-colum | R57 |
| 168 | holdout/triggers/248-new-values-in-a-before-trigger-on-a-strict-table-h | R40 |
| 169 | holdout/triggers/442-a-subquery-in-returning-reads-the-table-before-the | R2 |
| 170 | holdout/triggers/614-before-insert-trigger-that-modifies-the-table-read | R17 |
| 171 | holdout/triggers/690-upsert-do-update-fires-a-before-update-trigger-tha | R18 |
| 172 | holdout/triggers/702-rename-column-rewrites-new-col-in-a-temp-trigger-o | R43 |
| 173 | holdout/triggers/703-error-message-text-when-rename-column-breaks-a-tri | R57 |
| 174 | holdout/triggers/783-before-delete-raise-ignore-on-some-rows-keeps-retu | R42 |
| 175 | holdout/triggers/834-create-trigger-text-is-stored-as-written-in-sqlite | R58, R69 |
| 176 | holdout/type-affinity/26-integer-affinity-converts-2e0-to-integer-when-loss | R46 |
| 177 | holdout/type-affinity/46-nan-and-inf-text-stay-text-in-an-integer-column | R46 |
| 178 | holdout/type-affinity/149-blob-bytes-are-kept-by | R61 |
| 179 | holdout/type-affinity/371-compound-select-with-a-cast-as-real-arm-keeps-exac | R23 |
| 180 | holdout/type-affinity/396-union-all-of-a-real-column-and-an-integer-column-k | R23 |
| 181 | holdout/type-affinity/455-union-all-of-a-text-column-and-an-integer-column-k | R23 |
| 182 | holdout/type-affinity/461-values-arm-in-a-compound-select-keeps-the-first-ar | R23 |
| 183 | holdout/type-affinity/630-the | R61 |
| 184 | holdout/type-affinity/785-cast-of-an-invalid-utf-8-blob-to-text-keeps-the-by | R61 |
| 185 | holdout/upsert/6-row-value-assignment-naming-the-same-column-twice- | R44 |
| 186 | holdout/upsert/10-update-with-repeated-column-in-set-later-assignmen | R44 |
| 187 | holdout/upsert/33-update-of-integer-primary-key-below-the-minimum-in | R25 |
| 188 | holdout/upsert/35-insert-values-union-all-values-turso-ignored-the-u | R4 |
| 189 | holdout/upsert/66-insert-or-ignore-that-fails-check-does-not-touch-s | R66 |
| 190 | holdout/upsert/77-insert-naming-a-column-that-does-not-exist | R44, R52 |
| 191 | holdout/upsert/109-returning-with-a-correlated-subquery-sees-the-row- | R2 |
| 192 | holdout/upsert/110-last-insert-rowid-after-insert-returning | R67 |
| 193 | holdout/upsert/113-update-returning-correlated-subquery-sees-the-upda | R3 |
| 194 | holdout/upsert/114-returning-with-a-subquery-over-the-table-being-mod | R3 |
| 195 | holdout/upsert/186-delete-with-a-subquery-where-and-returning-with-a- | R2 |
| 196 | holdout/upsert/272-insert-or-rollback-with-on-conflict-do-update-that | R47 |
| 197 | holdout/upsert/386-subquery-in-on-conflict-do-update-set-and-where | R2 |
| 198 | holdout/upsert/456-insert-as-alias-with-an-on-conflict-do-update | R57, R67 |
| 199 | holdout/upsert/501-insert-default-values-ignores-the-default-of-an-in | R24 |
| 200 | holdout/upsert/534-insert-select-from-sqlite-sequence-does-not-read-t | R66 |
| 201 | holdout/upsert/645-returning-with-a-multi-column-scalar-subquery | R57 |
| 202 | holdout/upsert/649-do-update-set-naming-a-column-of-another-table | R57 |
| 203 | holdout/upsert/652-do-update-set-refers-to-a-column-that-does-not-exi | R57 |
| 204 | holdout/upsert/679-returning-with-a-table-name-qualifier-when-the-tab | R57 |
| 205 | holdout/upsert/733-pragma-max-page-count-with-a-failed-insert-inside- | N1 |
| 206 | holdout/upsert/790-insert-or-ignore-that-hits-a-conflict-still-advanc | R66 |
| 207 | holdout/window-functions/96-window-function-inside-a-correlated-exists-subquer | R38 |
| 208 | holdout/window-functions/202-extending-a-named-window-that-already-has-order-by | R60 |
| 209 | holdout/window-functions/383-window-function-as-a-column-default-is-rejected | R68 |
| 210 | holdout/window-functions/437-update-whose-set-subquery-uses-a-window-function-o | R26 |
| 211 | holdout/window-functions/499-column-names-of-window-functions-seen-through-a-de | N3 |
| 212 | holdout/window-functions/590-a-window-definition-that-extends-a-named-window-wi | R60 |
| 213 | json/each-and-tree | R71 |
| 214 | json/errors | R55 |
| 215 | limits/too-many-columns | R57 |
| 216 | limits/compound-select-too-many-terms | R57 |
| 217 | orm/1/1.9-cache-size-default-and-negative-kib-form | N1 |
| 218 | orm/1/1.14-page-size-default-and-change-before-first-write | N1 |
| 219 | orm/1/1.15-page-size-ignored-after-tables-exist-in-wal-needs-vacuum | N1 |
| 220 | orm/11/11.3-autoincrement-at-max-rowid-raises-sqlite-full | R57 |
| 221 | orm/11/11.26-covering-index-plan-and-automatic-index | R65 |
| 222 | orm/11/11.30-attach-inside-a-transaction-succeeds-detach-inside-it-fails- | R57 |
| 223 | orm/2/2.11-sqlite-master-rows-and-sql-text-preserved-verbatim | R58 |
| 224 | orm/2/2.17-like-escape-underscore-in-sqlite-matches-any-char-orm-gotcha | R58 |
| 225 | orm/3/3.49-writable-schema-off-update-sqlite-master-refused | R57 |
| 226 | orm/4/4.6-returning-cannot-reference-other-tables-subquery-in-column-l | R57 |
| 227 | orm/4/4.21-multi-row-values-with-different-arities-fails | R57 |
| 228 | orm/4/4.22-insert-with-too-many-few-values-message | R52 |
| 229 | orm/4/4.24-insert-unknown-column-message | R52 |
| 230 | orm/4/4.44-update-with-limit-needs-sqlite-enable-update-delete-limit-de | N2 |
| 231 | orm/4/4.60-double-quoted-string-literal-fallback-dqs-when-column-missin | R58 |
| 232 | orm/5/5.26-integer-primary-key-rejects-non-integer | R50 |
| 233 | orm/5/5.33-strict-unknown-type-name-rejected-datetime-not-allowed | R57 |
| 234 | orm/5/5.35-strict-column-without-type-rejected | R57 |
| 235 | orm/5/5.44-math-functions-need-sqlite-enable-math-functions | R57 |
| 236 | orm/7/7.11-json-object-odd-arguments-error | R55 |
| 237 | orm/7/7.25-json-pretty-3-46 | R58 |
| 238 | orm/9/9.6-unique-on-expression-index-message-uses-expression-text | R51 |
| 239 | orm/9/9.11-check-constraint-failed-unnamed-named-and-column-level | R58 |
| 240 | orm/9/9.24-unrecognized-token | R57 |
| 241 | orm/9/9.25-unterminated-string-literal | R57 |
| 242 | orm/9/9.31-datatype-mismatch-on-rowid | R50 |
| 243 | orm/9/9.39-insert-or-fail-or-abort-or-rollback-statement-scoping | R58 |
| 244 | orm/9/9.40-insert-into-a-column-that-does-not-exist | R52 |
| 245 | orm/9/9.48-selects-to-the-left-and-right-of-union-do-not-have-the-same- | R57 |
| 246 | orm/9/9.49-subquery-returns-more-than-1-column | R57 |
| 247 | orm/9/9.51-no-tables-specified-distinct-aggregate-misuse | R57 |
| 248 | params/named-forms | R58 |
| 249 | query/subqueries | R57 |
| 250 | query/compound-and-order | R57 |
| 251 | query/ambiguous-and-missing-columns | R57 |
| 252 | query/cte-and-view-column-spelling | R8 |
| 253 | schema/create-table-sql-preserved | R58 |
| 254 | schema/table-list-pragma | R57 |
| 255 | schema/strict-tables | R57 |
| 256 | schema/generated-columns | R57 |
| 257 | schema/create-table-as | R58 |
| 258 | text/trim-family | R58 |
| 259 | tx/attach-and-cross-db | R57 |
| 260 | tx/reindex-analyze-optimize | R57 |
