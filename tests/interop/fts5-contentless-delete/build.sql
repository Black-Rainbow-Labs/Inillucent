-- Built by inillucent 2.0.2 itself (`tools/cross/bin/releases/2.0.2`). Releases from 0.1.1 to 2.3.3
-- accepted `contentless_delete=1` on an FTS5 table and stored a plain contentless table, so a
-- DELETE from it was refused. 2.3.4 to 2.8.1 refused every query of such a table and every VACUUM
-- of its file. release_format.rs opens the file this script made with the current build.
CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT);
CREATE VIRTUAL TABLE note_search USING fts5(body, content='', contentless_delete=1);
INSERT INTO note (id, body) VALUES (1, 'hello world'), (2, 'second row'), (3, 'third hello');
INSERT INTO note_search (rowid, body) VALUES (1, 'hello world'), (2, 'second row'), (3, 'third hello');
-- After the checkpoint one more row was written to the table and to the index, so the log segment
-- holds it and `app.rdb` does not.
-- INSERT INTO note (id, body) VALUES (4, 'the row that only the log holds');
-- INSERT INTO note_search (rowid, body) VALUES (4, 'the row that only the log holds');
