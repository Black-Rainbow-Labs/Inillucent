-- Regression cases for defects the bug hunt of October 2026 found and fixed.
-- Each case printed something different from the pinned SQLite before the fix.
-- The design document is tasks/task-2201-bug-hunt-tdd.md.

-- case: bug-hunt/indexes-and-keys/partial-index-wrong-implication
-- a partial index holds only rows whose predicate is true, never NULL
CREATE TABLE t(a, b);
CREATE INDEX ti ON t(b) WHERE a > 5;
INSERT INTO t VALUES(1,'x'),(10,'x'),(6,'y'),(NULL,'x');
SELECT a FROM t WHERE b='x' AND a > 4 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a > 5 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a > 6 ORDER BY a;
SELECT a FROM t WHERE b='x' AND a >= 5 ORDER BY a;

-- case: bug-hunt/indexes-and-keys/partial-index-in-left-join
-- a partial index repeated by a join ON with no WHERE
CREATE TABLE t1(a);
CREATE TABLE t2(b, c);
CREATE INDEX i2 ON t2(b) WHERE c IS NOT NULL;
INSERT INTO t1 VALUES(1),(2);
INSERT INTO t2 VALUES(1,NULL),(2,5);
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b WHERE c IS NULL ORDER BY a;
SELECT a, b, c FROM t1 LEFT JOIN t2 ON a=b AND c IS NOT NULL ORDER BY a;

-- case: bug-hunt/indexes-and-keys/replace_default_index_affinity
-- INSERT OR REPLACE stores a NOT NULL default with the column affinity
CREATE TABLE t(a INT NOT NULL DEFAULT '5');
CREATE INDEX idx ON t(a);
INSERT OR REPLACE INTO t VALUES(NULL);
PRAGMA integrity_check;
SELECT typeof(a), a FROM t;
SELECT count(*) FROM t WHERE a = 5;

-- case: bug-hunt/indexes-and-keys/unique_with_collation_and_affinity
-- INSERT OR IGNORE on a WITHOUT ROWID table keyed COLLATE NOCASE
CREATE TABLE t0(c0 TEXT PRIMARY KEY COLLATE NOCASE, c1) WITHOUT ROWID;
INSERT INTO t0 VALUES('a',1);
INSERT OR IGNORE INTO t0 VALUES('A',2);
INSERT OR REPLACE INTO t0 VALUES('B',3);
INSERT OR REPLACE INTO t0 VALUES('b',4);
SELECT c0, c1 FROM t0 ORDER BY c0;
SELECT c1 FROM t0 WHERE c0 = 'A';
SELECT c1 FROM t0 WHERE c0 > 'A' ORDER BY c1;
PRAGMA integrity_check;

-- case: bug-hunt/indexes-and-keys/without-rowid-collated-key-writes
-- UPDATE, DELETE, OR IGNORE, upsert and REPLACE on WITHOUT ROWID tables keyed NOCASE and RTRIM
CREATE TABLE t(c0 TEXT PRIMARY KEY COLLATE NOCASE, c1) WITHOUT ROWID;
INSERT INTO t VALUES('a',1),('b',2),('c',3);
UPDATE t SET c1=20 WHERE c0='B';
SELECT * FROM t ORDER BY c0;
DELETE FROM t WHERE c0='a';
SELECT * FROM t ORDER BY c0;
UPDATE t SET c0='Z' WHERE c0='c';
SELECT * FROM t ORDER BY c0;
UPDATE t SET c0='z' WHERE c0='Z';
SELECT * FROM t ORDER BY c0;
UPDATE t SET c0='B' WHERE c0='z';
SELECT * FROM t ORDER BY c0;
INSERT OR IGNORE INTO t VALUES('B', 7);
INSERT INTO t VALUES('b', 8) ON CONFLICT DO UPDATE SET c1 = excluded.c1 + 100;
INSERT OR REPLACE INTO t VALUES('Z', 9);
SELECT * FROM t ORDER BY c0;
INSERT INTO t VALUES('b', 1);
PRAGMA integrity_check;
CREATE TABLE r(k TEXT COLLATE RTRIM, j TEXT COLLATE NOCASE, v, PRIMARY KEY(k, j)) WITHOUT ROWID;
INSERT INTO r VALUES('x', 'Q', 1), ('x ', 'p', 2);
INSERT OR IGNORE INTO r VALUES('x  ', 'q', 3);
UPDATE r SET v = v + 10 WHERE k = 'x' AND j = 'P';
DELETE FROM r WHERE k = 'x   ' AND j = 'q';
SELECT * FROM r;
PRAGMA integrity_check;

-- case: bug-hunt/indexes-and-keys/expression_index_collation
-- an expression key is ordered and compared by the collation its COLLATE names
CREATE TABLE t8(a INTEGER PRIMARY KEY, b TEXT);
CREATE UNIQUE INDEX t8bx ON t8(substr(b,2,4) COLLATE nocase);
INSERT INTO t8(a,b) VALUES(1,'Alice'),(2,'Bartholemew'),(3,'Cynthia');
INSERT INTO t8(a,b) VALUES(4,'BARTHMERE');
SELECT * FROM t8;
CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT);
CREATE INDEX vb ON v((b||'') COLLATE nocase);
INSERT INTO v(b) VALUES('b'),('A'),('a'),('B');
SELECT b FROM v INDEXED BY vb WHERE (b||'') COLLATE nocase = 'a' ORDER BY a;
PRAGMA integrity_check;

-- case: bug-hunt/indexes-and-keys/table_constraints_without_commas
-- table constraints may follow each other with no comma, and a CONSTRAINT name may stand alone
CREATE TABLE t1(a,b,c, PRIMARY KEY(a) UNIQUE (a) CONSTRAINT one);
INSERT INTO t1 VALUES(1,2,3);
INSERT INTO t1 VALUES(1,3,4);
CREATE TABLE t2(a,b,c, CONSTRAINT one PRIMARY KEY(a) CONSTRAINT two CHECK(b<10) UNIQUE(b) CONSTRAINT three);
INSERT INTO t2 VALUES(1,2,3);
INSERT INTO t2 VALUES(10,11,12);
SELECT * FROM t2;
CREATE TABLE abc(a, b, c, CONSTRAINT one CONSTRAINT two CHECK (b!=c));
ALTER TABLE abc DROP CONSTRAINT one;
SELECT sql FROM sqlite_schema WHERE name = 'abc';
CREATE TABLE d(a, b, c, CONSTRAINT one CHECK (a>b) FOREIGN KEY(a) REFERENCES d);
ALTER TABLE d DROP CONSTRAINT one;
SELECT sql FROM sqlite_schema WHERE name = 'd';
CREATE TABLE e(a, b, PRIMARY KEY(a) CONSTRAINT one CHECK (b<10) UNIQUE(b));
ALTER TABLE e DROP CONSTRAINT one;
SELECT sql FROM sqlite_schema WHERE name = 'e';
CREATE TABLE f(a, b, UNIQUE(a) CONSTRAINT two CHECK (b>0) ON CONFLICT FAIL);
ALTER TABLE f DROP CONSTRAINT two;
SELECT sql FROM sqlite_schema WHERE name = 'f';
CREATE TABLE g(a, b, CONSTRAINT x CONSTRAINT y CHECK (a>0) CONSTRAINT z CHECK (b>0));
ALTER TABLE g DROP CONSTRAINT y;
ALTER TABLE g DROP CONSTRAINT z;
SELECT sql FROM sqlite_schema WHERE name = 'g';

-- case: bug-hunt/indexes-and-keys/string_literal_key_columns
-- a string in a key column list names a column
CREATE TABLE t1(a, b, c, d, PRIMARY KEY('a'), UNIQUE('b' COLLATE nocase DESC));
CREATE INDEX t1c ON t1('c');
CREATE INDEX t1d ON t1('d' COLLATE binary ASC);
INSERT INTO t1 VALUES(1, 'x', 3, 4);
INSERT INTO t1 VALUES(2, 'X', 3, 4);
SELECT * FROM t1;
PRAGMA index_list(t1);
CREATE TABLE t3(x,y,UNIQUE("x",'y' ASC));
INSERT INTO t3 VALUES(1,11),(2,NULL);
SELECT * FROM t3 WHERE y IS NULL;

-- case: bug-hunt/indexes-and-keys/column_deferral_clause_alone
-- a column constraint may be only a deferral clause
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c1(a NOT NULL NOT DEFERRABLE INITIALLY IMMEDIATE);
CREATE TABLE c2(a REFERENCES p DEFERRABLE INITIALLY DEFERRED NOT NULL);
CREATE TABLE c3(a REFERENCES p NOT NULL DEFERRABLE INITIALLY DEFERRED);
PRAGMA foreign_keys = ON;
BEGIN;
INSERT INTO c3 VALUES(5);
INSERT INTO p VALUES(5);
COMMIT;
SELECT * FROM c3;
PRAGMA foreign_key_list(c3);

-- case: bug-hunt/indexes-and-keys/last_insert_rowid_inside_triggers
-- a trigger body sees the row just written and its own inserts, and the value is restored after it
CREATE TABLE t1(k INTEGER PRIMARY KEY);
CREATE TABLE t2(k INTEGER PRIMARY KEY, val1, val2, val3);
INSERT INTO t1 VALUES(1);
CREATE TRIGGER r1 AFTER INSERT ON t1 FOR EACH ROW BEGIN
  INSERT INTO t2 VALUES (NEW.k*2, last_insert_rowid(), NULL, NULL);
  UPDATE t2 SET k=k+10, val2=100+last_insert_rowid();
  UPDATE t2 SET val3=1000+last_insert_rowid();
END;
INSERT INTO t1 VALUES(13);
SELECT * FROM t2;
SELECT last_insert_rowid();

-- case: bug-hunt/indexes-and-keys/autoincrement_with_inserting_triggers
-- triggers that insert into an AUTOINCREMENT table leave one sqlite_sequence row with the largest value
CREATE TABLE t3928(a INTEGER PRIMARY KEY AUTOINCREMENT, b);
CREATE TRIGGER t3928r1 BEFORE INSERT ON t3928 BEGIN
  INSERT INTO t3928(b) VALUES('before1');
  INSERT INTO t3928(b) VALUES('before2');
END;
CREATE TRIGGER t3928r2 AFTER INSERT ON t3928 BEGIN
  INSERT INTO t3928(b) VALUES('after1');
  INSERT INTO t3928(b) VALUES('after2');
END;
INSERT INTO t3928(b) VALUES('test');
SELECT * FROM sqlite_sequence;
SELECT * FROM t3928 ORDER BY a;

-- case: bug-hunt/indexes-and-keys/virtual_generated_column_before_read_columns
-- a VIRTUAL generated column declared first does not shift the columns a scan decodes
PRAGMA foreign_keys=ON;
CREATE TABLE t1(gcb AS (b*1), a INTEGER PRIMARY KEY, gcc AS (c+0), b UNIQUE, gca AS (1*a+0), c UNIQUE) WITHOUT ROWID;
INSERT INTO t1 VALUES(1,2,3),(4,5,6),(7,8,9);
CREATE TABLE t1a(gcx AS (x+0) REFERENCES t1(a) ON DELETE CASCADE, id, x, gcid AS (1*id));
INSERT INTO t1a VALUES(1, 1),(2, 4),(3, 7);
SELECT rowid FROM t1a WHERE x = 4;
DELETE FROM t1 WHERE b=5;
SELECT id, x FROM t1a ORDER BY id;
UPDATE t1a SET id = 30 WHERE x = 7;
SELECT id, x FROM t1a ORDER BY id;

-- case: bug-hunt/indexes-and-keys/replace_runs_cascade_before_checking_new_row
-- INSERT OR REPLACE runs the replaced row's cascade before the new row's foreign key is checked
PRAGMA foreign_keys=ON;
CREATE TABLE t11(x INTEGER PRIMARY KEY, parent REFERENCES t11 ON DELETE CASCADE);
INSERT INTO t11 VALUES (1, NULL), (2, 1), (3, 2);
INSERT OR REPLACE INTO t11 VALUES (2, 3);
SELECT * FROM t11;
CREATE TABLE p(id INTEGER PRIMARY KEY, k UNIQUE);
CREATE TABLE c(pid REFERENCES p ON DELETE CASCADE);
INSERT INTO p VALUES(1,'a');
INSERT INTO c VALUES(1);
INSERT OR REPLACE INTO p VALUES(2,'a');
SELECT * FROM c;

-- case: bug-hunt/indexes-and-keys/replace_does_not_override_abort_constraint
-- a REPLACE constraint does not replace a row another constraint refuses
CREATE TABLE t1(x PRIMARY KEY, y, UNIQUE(y) ON CONFLICT REPLACE);
INSERT INTO t1 VALUES(1, 1);
INSERT INTO t1 VALUES(1, 2);
INSERT INTO t1 VALUES(2, 1);
SELECT * FROM t1;

-- case: bug-hunt/indexes-and-keys/row_larger_than_the_length_limit
-- a row whose values together pass the length limit is refused, as SQLite refuses the record
.limit length 1000
CREATE TABLE t(a, b, c);
INSERT INTO t VALUES(zeroblob(400), zeroblob(400), 1);
INSERT INTO t VALUES(zeroblob(600), zeroblob(600), 1);
INSERT INTO t VALUES(randomblob(990), NULL, NULL);
INSERT INTO t VALUES(randomblob(1000), 1, 2);
UPDATE t SET b = zeroblob(900) WHERE c = 1;
INSERT INTO t SELECT zeroblob(500), zeroblob(500), 3;
INSERT INTO t VALUES(1, 2, 3) ON CONFLICT DO NOTHING;
SELECT count(*), sum(length(a)), sum(length(b)) FROM t;
.limit length
