-- case: bug-hunt-11/lead/create-index-in-an-attached-database-indexes-its-own-table
ATTACH 'lead-a.db' AS a;
ATTACH 'lead-b.db' AS b;
CREATE TABLE a.items (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE b.items (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO a.items (name) VALUES ('a1'), ('a2');
INSERT INTO b.items (name) VALUES ('b1'), ('b2'), ('b3');
CREATE INDEX b.ix ON items(name);
SELECT count(*) FROM b.items WHERE name = 'b1';
SELECT name FROM b.items INDEXED BY ix WHERE name >= 'a' ORDER BY name;
PRAGMA b.integrity_check;
-- case: bug-hunt-11/lead/unique-index-in-an-attached-database-is-enforced-on-its-own-table
ATTACH 'lead-b.db' AS b;
CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE b.items (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO b.items (name) VALUES ('b1'), ('b2');
CREATE UNIQUE INDEX b.u ON items(name);
INSERT INTO b.items (name) VALUES ('b1');
SELECT count(*) FROM b.items;
-- case: bug-hunt-11/lead/create-index-in-an-attached-database-when-a-temp-table-has-the-name
ATTACH 'lead-b.db' AS b;
CREATE TABLE b.items (id INTEGER PRIMARY KEY, name TEXT, v INT);
CREATE TEMP TABLE items (id INTEGER PRIMARY KEY, name TEXT, v INT);
INSERT INTO b.items (name) VALUES ('b1'), ('b2'), ('b3');
INSERT INTO temp.items (name) VALUES ('tmp');
CREATE INDEX b.ix ON items(name);
SELECT count(*) FROM b.items;
PRAGMA b.integrity_check;
-- case: bug-hunt-11/lead/unqualified-create-index-goes-to-the-tables-database
ATTACH 'lead-c.db' AS c;
CREATE TABLE c.only_here (id INTEGER PRIMARY KEY, name TEXT);
INSERT INTO c.only_here (name) VALUES ('x'), ('y');
CREATE INDEX ix_only ON only_here(name);
SELECT name, tbl_name FROM c.sqlite_master WHERE type = 'index';
SELECT count(*) FROM main.sqlite_master WHERE type = 'index';
PRAGMA c.integrity_check;
