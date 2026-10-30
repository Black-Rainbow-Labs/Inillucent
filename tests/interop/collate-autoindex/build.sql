-- Built by inillucent 2.0.2 itself (`tools/cross/bin/releases/2.0.2`). Releases from 0.1.1 to 2.0.9
-- wrote the key of an automatic index (a table level UNIQUE or PRIMARY KEY) in binary order and
-- ignored a COLLATE written inside the constraint. release_format.rs opens the file this script
-- made with the current build. The names are chosen so that binary order and NOCASE order differ.
CREATE TABLE person (id INTEGER PRIMARY KEY, first TEXT, last TEXT, UNIQUE (first, last COLLATE NOCASE));
CREATE TABLE tag (name TEXT, note TEXT, PRIMARY KEY (name COLLATE NOCASE));
INSERT INTO person (first, last) VALUES ('Ann', 'Zed'), ('Ann', 'alpha'), ('Ann', 'Bravo'), ('Bob', 'charlie'), ('Bob', 'Delta');
INSERT INTO tag VALUES ('Zulu', 'z'), ('alpha', 'a'), ('Bravo', 'b'), ('charlie', 'c'), ('Delta', 'd');
-- After the checkpoint two more rows were written, so the log segment holds them and `app.rdb`
-- does not.
-- INSERT INTO tag VALUES ('echo', 'the row that only the log holds');
-- INSERT INTO person (first, last) VALUES ('Cy', 'Echo');
