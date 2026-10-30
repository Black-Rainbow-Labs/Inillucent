-- Built by inillucent 2.0.2 itself (`tools/cross/bin/releases/2.0.2`). Releases before 2.1.0
-- stored ALTER TABLE ... ADD COLUMN with the new column after the table constraints, which the
-- grammar refuses. release_format.rs opens the file this script made with the current build.
CREATE TABLE document (id TEXT PRIMARY KEY, title TEXT NOT NULL);
CREATE TABLE chunk (id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES document(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, body TEXT, UNIQUE (document_id, ordinal), CHECK (ordinal >= 0));
CREATE INDEX chunk_body ON chunk (body);
INSERT INTO document VALUES ('d1', 'first'), ('d2', 'second');
INSERT INTO chunk (id, document_id, ordinal, body) VALUES ('c1', 'd1', 0, 'alpha'), ('c2', 'd1', 1, 'beta'), ('c3', 'd2', 0, 'gamma');
ALTER TABLE chunk ADD COLUMN embedded_at INTEGER;
ALTER TABLE chunk ADD COLUMN weight REAL NOT NULL DEFAULT 1.5;
CREATE INDEX chunk_embedded ON chunk (embedded_at);
UPDATE chunk SET embedded_at = 100 WHERE id = 'c2';
INSERT INTO chunk VALUES ('c4', 'd2', 1, 'delta', 200, 2.5);
