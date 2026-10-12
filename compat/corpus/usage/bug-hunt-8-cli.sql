-- case: bug-hunt-8/cli/import-strips-a-utf8-byte-order-mark
.mode list
.headers off
.once bom.csv
SELECT char(65279) || 'id,name' UNION ALL SELECT '1,ann';
.import --csv bom.csv people
.mode quote
.headers on
SELECT id, name FROM people;
SELECT hex(name) FROM pragma_table_info('people');
-- case: bug-hunt-8/cli/import-into-existing-table-with-a-byte-order-mark
CREATE TABLE people(id INTEGER, name TEXT);
.mode list
.headers off
.once bom2.csv
SELECT char(65279) || '1,ann' UNION ALL SELECT '2,bob';
.import --csv bom2.csv people
.mode quote
.headers on
SELECT id, typeof(id), name FROM people;
-- case: bug-hunt-8/cli/import-pads-short-rows-and-ignores-extra-fields
.mode list
.headers off
.once ragged.csv
SELECT 'a,b,c' UNION ALL SELECT '1,2' UNION ALL SELECT '3' UNION ALL SELECT '4,5,6,7,8' UNION ALL SELECT '' UNION ALL SELECT '9,9,9';
.import --csv ragged.csv t
.mode quote
.headers on
SELECT * FROM t;
SELECT count(*) FROM t;
-- case: bug-hunt-8/cli/import-into-existing-table-pads-and-numbers-lines-from-the-top-of-the-file
CREATE TABLE t(a, b, c);
.mode list
.headers off
.once ragged2.csv
SELECT '1,2' UNION ALL SELECT '3,4,5,6' UNION ALL SELECT '7,8,9';
.import --csv ragged2.csv t
.mode quote
.headers on
SELECT * FROM t;
-- case: bug-hunt-8/cli/import-drops-the-carriage-return-ending-a-crlf-header
.mode list
.headers off
.once crlf.csv
SELECT 'a,b' || char(13, 10) || '1,2' || char(13, 10) || '3,"4"';
.import --csv crlf.csv t
.mode quote
.headers on
SELECT hex(name) FROM pragma_table_info('t');
SELECT a, hex(b) FROM t;
-- case: bug-hunt-8/cli/import-renames-duplicate-header-names
.mode list
.headers off
.once dup.csv
SELECT 'a,A,b,a_1,,' UNION ALL SELECT '1,2,3,4,5,6';
.import --csv dup.csv t
.mode quote
.headers on
SELECT name FROM pragma_table_info('t');
SELECT * FROM t;
-- case: bug-hunt-8/cli/import-keeps-the-good-rows-when-one-row-is-rejected
CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);
.mode list
.headers off
.once hdr.csv
SELECT 'id,name' UNION ALL SELECT '1,x' UNION ALL SELECT '2,y' UNION ALL SELECT '2,dup' UNION ALL SELECT '3,z';
.import --csv hdr.csv t
.mode quote
.headers on
SELECT * FROM t;
-- case: bug-hunt-8/cli/import-with-skip-numbers-lines-from-the-top-of-the-file
CREATE TABLE t(a, b);
.mode list
.headers off
.once skip.csv
SELECT 'junk' UNION ALL SELECT '1' UNION ALL SELECT '2,3';
.import --csv --skip 1 skip.csv t
.mode quote
.headers on
SELECT * FROM t;
-- case: bug-hunt-8/cli/import-numbers-a-row-after-a-quoted-field-with-a-newline
CREATE TABLE t(a PRIMARY KEY, b);
.mode list
.headers off
.once q.csv
SELECT '1,"line' UNION ALL SELECT 'two"' UNION ALL SELECT '1,dup' UNION ALL SELECT '2,ok';
.import --csv q.csv t
.mode quote
.headers on
SELECT * FROM t;
-- case: bug-hunt-8/cli/tabs-mode-quotes-a-value-with-a-tab-and-import-reads-it-back
CREATE TABLE src(a, b);
INSERT INTO src VALUES('x' || char(9) || 'y', '"z"'), ('new' || char(10) || 'line', 'p|q'), ('', NULL), ('plain', 'two words');
.mode tabs
.headers off
.once tabs.txt
SELECT * FROM src;
.mode tabs
CREATE TABLE dst(a, b);
.import tabs.txt dst
.mode quote
.headers on
SELECT * FROM dst;
.mode tabs
SELECT * FROM src;
-- case: bug-hunt-8/cli/csv-mode-quotes-spaces-quotes-and-non-ascii-text
.mode csv
.headers on
SELECT 'a b' AS "x y", ' lead', 'trail ', 'q''s', 'caf' || char(233), 'a^b', 'semi;', '', NULL, 12, x'41';
-- case: bug-hunt-8/cli/markdown-keeps-a-value-with-a-newline-inside-its-column
CREATE TABLE m(a, b);
INSERT INTO m VALUES ('line1' || char(10) || 'line2', NULL), ('p|q', 1.5), ('tab' || char(9) || 'x', 'it''s');
.mode markdown
.headers on
SELECT * FROM m;
-- case: bug-hunt-8/cli/table-and-box-draw-a-rule-between-rows-when-a-value-has-a-newline
CREATE TABLE m(a, b);
INSERT INTO m VALUES ('line1' || char(10) || 'line2', NULL), ('x', 22), ('y', 3);
.mode table
.headers on
SELECT * FROM m;
.mode box
SELECT * FROM m;
-- case: bug-hunt-8/cli/html-writes-null-and-escapes-a-single-quote
CREATE TABLE m(a, b);
INSERT INTO m VALUES ('it''s <b> & "q"', NULL), (1, 2.5);
.mode html
.headers on
SELECT * FROM m;
-- case: bug-hunt-8/cli/dump-writes-text-with-control-characters-as-unistr
CREATE TABLE t(id INTEGER PRIMARY KEY, body TEXT);
INSERT INTO t VALUES(1, 'a' || char(1) || 'x' || char(9) || 'y');
INSERT INTO t VALUES(2, 'line' || char(10) || 'two');
INSERT INTO t VALUES(3, 'back\slash' || char(13) || char(10));
INSERT INTO t VALUES(4, 'plain \ backslash and it''s');
INSERT INTO t VALUES(5, 'nul' || char(0) || 'x');
INSERT INTO t VALUES(6, x'0a0d');
.dump
-- case: bug-hunt-8/cli/dump-nosys-leaves-out-sqlite-tables
CREATE TABLE a(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO a(v) VALUES('x'), ('y');
CREATE INDEX av ON a(v);
ANALYZE;
.dump --nosys
.print ----
.dump -nosys a
.print ----
.dump
-- case: bug-hunt-8/cli/dump-data-only-writes-only-the-rows
CREATE TABLE a(id INTEGER PRIMARY KEY, v);
INSERT INTO a VALUES(1, 'x'), (2, 'y');
CREATE INDEX av ON a(v);
CREATE VIEW va AS SELECT v FROM a;
.dump --data-only
-- case: bug-hunt-8/cli/dump-of-a-named-table-writes-that-table-and-not-its-indexes
CREATE TABLE a(id INTEGER PRIMARY KEY, v);
CREATE TABLE b(id INTEGER PRIMARY KEY, w);
INSERT INTO a VALUES(1, 'x');
INSERT INTO b VALUES(2, 'y');
CREATE INDEX av ON a(v);
CREATE INDEX aw ON b(w);
.dump a
.print ----
.dump a b
.print ----
.dump a%
-- case: bug-hunt-8/cli/dump-refuses-an-option-it-does-not-know
CREATE TABLE a(id);
.dump --nope
-- case: bug-hunt-8/cli/schema-indent-puts-each-column-on-its-own-line
CREATE TABLE people(id INTEGER PRIMARY KEY, first_name TEXT NOT NULL, last_name TEXT NOT NULL, born INTEGER, note TEXT);
CREATE TABLE tiny(a, b);
CREATE UNIQUE INDEX people_name ON people(first_name, last_name) WHERE born > 1900 AND note IS NOT NULL AND first_name <> 'x';
CREATE VIEW v AS SELECT id, first_name, last_name FROM people WHERE born > 1900 AND note IS NOT NULL;
CREATE TRIGGER trg AFTER INSERT ON tiny BEGIN
  UPDATE tiny SET a = 1 WHERE b = new.b;
END;
.schema --indent
.print ----
.schema --indent people
.print ----
.schema -indent tiny
-- case: bug-hunt-8/cli/schema-nosys-leaves-out-sqlite-tables-and-an-unknown-option-is-named
CREATE TABLE a(id INTEGER PRIMARY KEY AUTOINCREMENT, v);
INSERT INTO a(v) VALUES('x');
CREATE INDEX av ON a(v);
ANALYZE;
.schema
.print ----
.schema --nosys
.schema --bogus
.schema a b
-- case: bug-hunt-8/cli/parameter-set-reads-a-value-whose-quote-is-not-at-its-start
.parameter set :b x'0102'
.parameter set :c x'41'||'z'
.parameter set :d it's
.parameter set :e 'two words'
.parameter set :f 7
.parameter list
SELECT typeof(:b), hex(:b), hex(:c), :d, :e, :f;
-- case: bug-hunt-8/cli/parameter-set-with-too-many-words-prints-the-help
.parameter set :f 'it''s'
.parameter set :n 'abc' extra
.parameter list
.parameter unset
.parameter frobnicate
