-- case: catalog/drizzle-columns-pragma-table-xinfo-joined-to-sqlite-master
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name AS tableName, p.name AS columnName, p.type AS columnType, p."notnull" AS isNotNull, p.dflt_value AS defaultValue, p.pk AS pk, p.hidden AS hidden FROM sqlite_master AS m JOIN pragma_table_xinfo(m.name) AS p WHERE m.type = 'table' AND m.tbl_name != 'sqlite_sequence' ORDER BY m.name, p.cid;
-- case: catalog/drizzle-foreign-keys-pragma-foreign-key-list-joined-to-sqlite-master
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name AS tableFrom, f.id AS id, f."table" AS tableTo, f."from", f."to", f."on_update" AS onUpdate, f."on_delete" AS onDelete, f.seq AS seq FROM sqlite_master AS m, pragma_foreign_key_list(m.name) AS f WHERE m.tbl_name != '_cf_KV' ORDER BY m.name, f.id, f.seq;
-- case: catalog/drizzle-indexes-three-way-join-of-sqlite-master-index-list-and-index-info
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.tbl_name AS tableName, il.name AS indexName, ii.name AS columnName, il."unique" AS isUnique, il.seq AS seq FROM sqlite_master AS m, pragma_index_list(m.name) AS il, pragma_index_info(il.name) AS ii WHERE m.type = 'table' AND il.name NOT LIKE 'sqlite_autoindex_%' ORDER BY m.name, il.name, ii.seqno;
-- case: catalog/rails-schema-dumper-index-list-origin-and-partial-per-table
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name AS tbl, il.seq, il.name, il."unique", il.origin, il.partial FROM sqlite_master AS m, pragma_index_list(m.name) AS il WHERE m.type = 'table' ORDER BY m.name, il.seq;
-- case: catalog/rails-index-xinfo-for-every-index-including-without-rowid-trailing-columns
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name AS tbl, il.name AS idx, ix.seqno, ix.cid, ix.name, ix."desc", ix.coll, ix."key" FROM sqlite_master m JOIN pragma_index_list(m.name) il JOIN pragma_index_xinfo(il.name) ix WHERE m.type = 'table' ORDER BY m.name, il.name, ix.seqno;
-- case: catalog/rails-table-and-view-names-excluding-sqlite-sequence
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') AND NOT name = 'sqlite_sequence' ORDER BY name;
-- case: catalog/django-introspection-statements-quoted-and-qualified-pragma-arguments
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
PRAGMA table_info("post"); PRAGMA main.table_info("author"); PRAGMA "table_xinfo"('post'); PRAGMA table_xinfo(post_view); PRAGMA index_list("post"); PRAGMA index_list(tag); PRAGMA index_info(post_title); PRAGMA index_xinfo(post_title); PRAGMA index_xinfo(post_expr); PRAGMA foreign_key_list(post); PRAGMA foreign_key_list(post_tag); PRAGMA foreign_key_list("nosuch");
-- case: catalog/ef-core-prisma-pragma-table-functions-with-where-and-aggregates
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT * FROM pragma_table_info('post') WHERE pk > 0; SELECT cid, name FROM pragma_table_xinfo('post') WHERE hidden > 0 ORDER BY cid; SELECT group_concat(name) FROM pragma_table_info('post'); SELECT count(*) FROM pragma_index_list('post') WHERE origin = 'u'; SELECT name FROM pragma_index_list('post') WHERE "unique" = 1 AND partial = 0 ORDER BY name; SELECT p.name, p.pk FROM pragma_table_info('post_tag') p ORDER BY p.pk DESC, p.cid;
-- case: catalog/pragma-table-function-in-a-scalar-subquery-per-outer-row
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT (SELECT group_concat(name) FROM pragma_index_info(il.name)) AS cols, il.name FROM pragma_index_list('post') il ORDER BY il.name; SELECT m.name, (SELECT count(*) FROM pragma_table_info(m.name)) AS ncols FROM sqlite_master m WHERE m.type = 'table' ORDER BY m.name; SELECT m.name FROM sqlite_master m WHERE EXISTS (SELECT 1 FROM pragma_foreign_key_list(m.name)) ORDER BY m.name; SELECT m.name FROM sqlite_master m WHERE m.type = 'table' AND NOT EXISTS (SELECT 1 FROM pragma_index_list(m.name)) ORDER BY m.name;
-- case: catalog/pragma-table-function-hidden-arg-and-schema-columns
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT * FROM pragma_table_info('post', 'main') LIMIT 2; SELECT * FROM pragma_table_info('post', 'temp') LIMIT 2; SELECT * FROM pragma_table_info('nosuch'); SELECT * FROM pragma_index_list; SELECT * FROM pragma_table_info; SELECT arg, schema FROM pragma_table_info('post') LIMIT 1; SELECT name FROM pragma_table_info WHERE arg = 'author'; SELECT name FROM pragma_table_info WHERE arg = 'author' AND schema = 'main'; SELECT name FROM pragma_table_info('author') WHERE schema = 'main';
-- case: catalog/pragma-table-function-arguments-from-expressions-and-types
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT c.name FROM pragma_table_info(lower('AUTHOR')) c; SELECT c.name FROM pragma_table_info('au' || 'thor') c; SELECT * FROM pragma_table_info(NULL); SELECT * FROM pragma_table_info(42); SELECT * FROM pragma_table_info(x'617574686f72');
-- case: catalog/pragma-table-function-joined-from-an-outer-join-and-a-cross-join
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name AS t, p.name AS c FROM sqlite_master m LEFT JOIN pragma_table_info(m.name) p ON 1 WHERE m.type = 'view' OR m.name = 'author' ORDER BY 1, 2; SELECT m.name, p.cid FROM sqlite_master m LEFT JOIN pragma_table_info(m.name) p WHERE m.name IN ('tag', 'nosuchtable') ORDER BY 1, 2; SELECT m.name, p.cid FROM sqlite_master m CROSS JOIN pragma_table_info('tag') p WHERE m.name IN ('tag', 'author') ORDER BY 1, 2;
-- case: catalog/pragma-table-function-condition-on-outer-term-is-tested-before-the-function-is-called
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT count(*) FROM sqlite_master m, pragma_foreign_key_check(m.name) f WHERE m.type = 'table'; SELECT m.name, c.name FROM sqlite_master m, pragma_table_info(m.name) AS c WHERE m.type = 'table' AND c."notnull" = 1 ORDER BY 1, 2;
-- case: catalog/pragma-table-function-whose-outer-term-is-another-pragma-function
CREATE TABLE author(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, bio TEXT DEFAULT 'none', born INT CHECK (born > 0));
CREATE TABLE post(id INTEGER PRIMARY KEY, author_id INTEGER NOT NULL REFERENCES author(id) ON DELETE CASCADE ON UPDATE NO ACTION, editor_id INT, slug TEXT COLLATE NOCASE, title VARCHAR(200) NOT NULL, score REAL GENERATED ALWAYS AS (id * 1.5) VIRTUAL, stored_len INT GENERATED ALWAYS AS (length(title)) STORED, UNIQUE (author_id, slug), FOREIGN KEY (editor_id) REFERENCES author (id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED);
CREATE INDEX post_title ON post (title DESC, slug);
CREATE UNIQUE INDEX post_slug_live ON post (slug) WHERE editor_id IS NOT NULL;
CREATE INDEX post_expr ON post (lower(title));
CREATE TABLE tag(code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
CREATE TABLE post_tag(post_id INTEGER, tag_code TEXT, PRIMARY KEY (post_id, tag_code), FOREIGN KEY (post_id) REFERENCES post(id), FOREIGN KEY (tag_code) REFERENCES tag);
CREATE TABLE strict_t(a INTEGER, b TEXT, c ANY) STRICT;
CREATE VIEW post_view AS SELECT p.id, a.name, p.title FROM post p JOIN author a ON a.id = p.author_id;
SELECT m.name, p.name FROM pragma_table_list m, pragma_table_info(m.name) p WHERE m.schema = 'main' AND m.name NOT LIKE 'sqlite_%' ORDER BY 1, p.cid; SELECT il.name, ii.seqno, ii.name FROM pragma_index_list('post') il JOIN pragma_index_info(il.name) ii ORDER BY il.name, ii.seqno; SELECT * FROM author, pragma_index_list('post') il, pragma_index_info(il.name) ii WHERE ii.seqno = 5;
-- case: catalog/foreign-key-check-reports-rows-in-rowid-order-with-the-last-declared-key-numbered-0
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
PRAGMA foreign_key_check(c1); PRAGMA foreign_key_check(C1); PRAGMA main.foreign_key_check(c1); PRAGMA foreign_key_check(cw);
-- case: catalog/foreign-key-check-with-a-parent-table-that-does-not-exist-reports-every-row-with-a-complete-key
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
PRAGMA foreign_key_check(d);
-- case: catalog/foreign-key-check-without-rowid-child-reports-a-null-rowid
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
PRAGMA foreign_key_check(wc); PRAGMA foreign_key_check(w);
-- case: catalog/foreign-key-check-table-valued-function-with-and-without-arguments
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
SELECT * FROM pragma_foreign_key_check('cw'); SELECT * FROM pragma_foreign_key_check('cw', 'main'); SELECT "table", rowid, parent, fkid FROM pragma_foreign_key_check('c1') ORDER BY rowid, fkid; SELECT arg, schema FROM pragma_foreign_key_check('c1') LIMIT 1; SELECT count(*) FROM pragma_foreign_key_check;
-- case: catalog/foreign-key-check-joined-to-sqlite-master
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
SELECT m.name, f."table", f.rowid, f.parent, f.fkid FROM sqlite_master m, pragma_foreign_key_check(m.name) f WHERE m.type='table' ORDER BY 1, 3, 5;
-- case: catalog/foreign-key-check-in-temp-and-attached-databases
ATTACH ':memory:' AS aux;
CREATE TABLE aux.ap(id INTEGER PRIMARY KEY);
CREATE TABLE aux.ac(id INTEGER PRIMARY KEY, pid REFERENCES ap(id));
INSERT INTO aux.ap VALUES(1);
INSERT INTO aux.ac VALUES(1, 1), (2, 2), (3, 3);
CREATE TEMP TABLE tp(id INTEGER PRIMARY KEY);
CREATE TEMP TABLE tc(id INTEGER PRIMARY KEY, pid REFERENCES tp(id));
INSERT INTO tc VALUES(1, 7);
PRAGMA aux.foreign_key_check;
PRAGMA aux.foreign_key_check(ac);
PRAGMA temp.foreign_key_check;
PRAGMA temp.foreign_key_check(tc);
SELECT * FROM pragma_foreign_key_check('ac', 'aux');
SELECT * FROM pragma_foreign_key_check('tc', 'temp');
SELECT * FROM pragma_foreign_key_check('ac');
-- case: catalog/foreign-key-check-for-a-table-that-does-not-exist-is-an-error
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
SELECT * FROM pragma_foreign_key_check('nosuch');
-- case: catalog/foreign-key-check-for-a-table-without-keys-is-empty
CREATE TABLE P(id INTEGER PRIMARY KEY, u UNIQUE);
CREATE TABLE c1(id INTEGER PRIMARY KEY, pid REFERENCES p(id), q REFERENCES P(u));
CREATE TABLE d(x, y, FOREIGN KEY(x, y) REFERENCES nosuch(a, b));
CREATE TABLE w(a, b, PRIMARY KEY(a)) WITHOUT ROWID;
CREATE TABLE cw(a REFERENCES p, b REFERENCES w);
CREATE TABLE wc(k PRIMARY KEY, v REFERENCES p) WITHOUT ROWID;
INSERT INTO P VALUES(1, 10);
INSERT INTO c1 VALUES(1, 1, 10), (2, 5, 10), (3, 1, 11), (4, NULL, NULL), (5, 6, 12);
INSERT INTO d VALUES(1, 2), (NULL, 3);
INSERT INTO w VALUES(1,2);
INSERT INTO cw VALUES(7, 1);
INSERT INTO wc VALUES('a', 9);
PRAGMA foreign_key_check(w); SELECT count(*) FROM pragma_foreign_key_check('w');
-- case: catalog/index-list-origin-distinguishes-unique-constraints-primary-keys-and-created-indexes
CREATE TABLE pk1(a PRIMARY KEY, b UNIQUE, c UNIQUE, d, UNIQUE (c, d)); CREATE TABLE pk2(id INTEGER PRIMARY KEY DESC, v); CREATE TABLE pk3(id INTEGER, v, PRIMARY KEY (id DESC)); CREATE TABLE pk4(a INT PRIMARY KEY, b INTEGER NOT NULL); CREATE TABLE wr(a, b, PRIMARY KEY (a, b)) WITHOUT ROWID; CREATE UNIQUE INDEX wr_b ON wr(b); PRAGMA index_list(pk1); PRAGMA index_list(pk2); PRAGMA index_list(pk3); PRAGMA index_list(pk4); PRAGMA index_list(wr); PRAGMA table_info(pk2); PRAGMA table_info(pk3);
-- case: catalog/index-xinfo-of-a-without-rowid-table-lists-the-columns-that-find-the-row
CREATE TABLE w1(a, b, c, d, PRIMARY KEY (b, a)) WITHOUT ROWID; CREATE INDEX w1_c ON w1(c); CREATE UNIQUE INDEX w1_d ON w1(d DESC); CREATE INDEX w1_ab ON w1(a, b); CREATE INDEX w1_expr ON w1(lower(c)); CREATE INDEX w1_part ON w1(c) WHERE d > 1; CREATE TABLE w2(k PRIMARY KEY) WITHOUT ROWID; PRAGMA index_xinfo(sqlite_autoindex_w1_1); PRAGMA index_xinfo(w1_c); PRAGMA index_xinfo(w1_d); PRAGMA index_xinfo(w1_ab); PRAGMA index_xinfo(w1_expr); PRAGMA index_xinfo(w1_part); PRAGMA index_xinfo(sqlite_autoindex_w2_1); PRAGMA index_info(w1_c); PRAGMA index_info(sqlite_autoindex_w1_1); SELECT * FROM pragma_index_xinfo('w1_c'); SELECT * FROM pragma_index_xinfo('w1_c', 'main'); SELECT * FROM pragma_index_xinfo('nosuch'); PRAGMA index_xinfo("W1_C");
-- case: catalog/foreign-key-list-numbers-keys-from-the-last-declared-and-ignores-match
CREATE TABLE pk1(a PRIMARY KEY, b UNIQUE, c UNIQUE, d, UNIQUE (c, d)); CREATE TABLE pk2(id INTEGER PRIMARY KEY DESC, v); CREATE TABLE fk1(a, b, c, FOREIGN KEY (a, b) REFERENCES pk1(a, b) ON UPDATE CASCADE ON DELETE RESTRICT MATCH SIMPLE, FOREIGN KEY (c) REFERENCES pk1 DEFERRABLE INITIALLY IMMEDIATE); CREATE TABLE fk2(x REFERENCES fk1(a) ON DELETE SET DEFAULT, y REFERENCES "pk1" ("A"), z REFERENCES [pk2], w REFERENCES pk2 MATCH FULL); PRAGMA foreign_key_list(fk1); PRAGMA foreign_key_list(fk2); SELECT id, seq, "table", "from", "to", on_delete FROM pragma_foreign_key_list('fk2') ORDER BY id, seq;
-- case: catalog/table-info-types-are-dequoted-and-standard-types-are-upper-case
CREATE TABLE q(a1 Int, a2 integer, a3 "Integer", a4 [text], a5 "Text", a6 "VARCHAR(10)", a7 `real`, a8 "Blob", a9 "any", b1 "float", b2 "my type", b3 [Int], b4 "INT", c1 integer(5), c2 text(5), c3 Real, c4 int unsigned, c5 "a""b"); PRAGMA table_info(q); CREATE TABLE tt(a "Int", b "INTEGER" PRIMARY KEY); INSERT INTO tt VALUES('x', NULL); SELECT * FROM tt; PRAGMA table_info(tt); CREATE TABLE t2(a, b [integer] PRIMARY KEY); INSERT INTO t2 VALUES('x', NULL); SELECT * FROM t2;
-- case: catalog/table-info-of-the-schema-tables-reports-rootpage-as-int
PRAGMA table_info(sqlite_master); PRAGMA table_info(sqlite_schema); PRAGMA table_info(sqlite_temp_master); PRAGMA table_info(sqlite_temp_schema); PRAGMA table_xinfo(sqlite_sequence);
-- case: catalog/table-list-names-the-schema-of-temp-and-attached-tables
CREATE TABLE t(a); CREATE TEMP TABLE tt(a); CREATE TEMP VIEW tv AS SELECT 1 AS x; ATTACH ':memory:' AS aux; CREATE TABLE aux.at1(p); SELECT schema, name, type, ncol, wr, strict FROM pragma_table_list ORDER BY schema, name; PRAGMA aux.table_list; PRAGMA temp.table_list; PRAGMA table_list(tt); SELECT name FROM pragma_table_list('at1');
-- case: catalog/database-list-seq-is-the-database-slot
SELECT seq, name FROM pragma_database_list; ATTACH ':memory:' AS aux; SELECT seq, name FROM pragma_database_list; CREATE TEMP TABLE tt(a); SELECT seq, name FROM pragma_database_list ORDER BY seq; ATTACH ':memory:' AS second; SELECT seq, name FROM pragma_database_list ORDER BY seq; DETACH aux; SELECT seq, name FROM pragma_database_list ORDER BY seq;
-- case: catalog/table-info-index-list-and-foreign-key-list-in-temp-and-attached-databases
CREATE TEMP TABLE tt(a INTEGER PRIMARY KEY, b TEXT UNIQUE); CREATE INDEX tt_b ON tt(b); CREATE TEMP VIEW tv AS SELECT a FROM tt; ATTACH ':memory:' AS aux; CREATE TABLE aux.at1(p INTEGER PRIMARY KEY, q REFERENCES at1(p)); CREATE INDEX aux.at1_q ON at1(q); PRAGMA table_info(tt); PRAGMA temp.table_info(tt); PRAGMA temp.index_list(tt); PRAGMA index_list(tt); PRAGMA table_info(tv); PRAGMA aux.table_info(at1); PRAGMA aux.index_list(at1); PRAGMA aux.foreign_key_list(at1); PRAGMA aux.index_info(at1_q); PRAGMA table_info(at1); SELECT * FROM pragma_table_info('tt', 'temp'); SELECT * FROM pragma_table_info('at1', 'aux'); SELECT * FROM pragma_index_list('tt', 'temp'); SELECT * FROM pragma_foreign_key_list('at1', 'aux'); SELECT * FROM pragma_table_info('tt', 'nosuch');
-- case: catalog/function-list-reports-aggregates-that-can-run-over-a-window-as-w
SELECT name, type, narg FROM pragma_function_list WHERE name IN ('abs', 'sum', 'max', 'min', 'count', 'avg', 'total', 'group_concat', 'row_number', 'rank', 'json_group_array') ORDER BY name, narg;
-- case: catalog/setting-mmap-size-reads-back-what-sqlite-reads-back
PRAGMA mmap_size; PRAGMA mmap_size = 0; PRAGMA mmap_size; PRAGMA mmap_size = 1; PRAGMA mmap_size; PRAGMA mmap_size = 2; PRAGMA mmap_size; PRAGMA mmap_size = 3; PRAGMA mmap_size; PRAGMA mmap_size = 5; PRAGMA mmap_size; PRAGMA mmap_size = -1; PRAGMA mmap_size; PRAGMA mmap_size = -3; PRAGMA mmap_size; PRAGMA mmap_size = on; PRAGMA mmap_size; PRAGMA mmap_size = off; PRAGMA mmap_size; PRAGMA mmap_size = yes; PRAGMA mmap_size; PRAGMA mmap_size = no; PRAGMA mmap_size; PRAGMA mmap_size = true; PRAGMA mmap_size; PRAGMA mmap_size = false; PRAGMA mmap_size; PRAGMA mmap_size = abc; PRAGMA mmap_size; PRAGMA mmap_size = '1'; PRAGMA mmap_size; PRAGMA mmap_size = 1.5; PRAGMA mmap_size; PRAGMA mmap_size = 4294967295; PRAGMA mmap_size; PRAGMA mmap_size = 4294967296; PRAGMA mmap_size; PRAGMA mmap_size = 2147483648; PRAGMA mmap_size; PRAGMA mmap_size = -2147483648; PRAGMA mmap_size; PRAGMA mmap_size = -2147483649; PRAGMA mmap_size;
-- case: catalog/setting-wal-autocheckpoint-reads-back-what-sqlite-reads-back
PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 0; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 1; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 2; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 3; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 5; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = -1; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = -3; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = on; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = off; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = yes; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = no; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = true; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = false; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = abc; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = '1'; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 1.5; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 4294967295; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 4294967296; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = 2147483648; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = -2147483648; PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint = -2147483649; PRAGMA wal_autocheckpoint;
-- case: catalog/setting-journal-size-limit-reads-back-what-sqlite-reads-back
PRAGMA journal_size_limit; PRAGMA journal_size_limit = 0; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 1; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 2; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 3; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 5; PRAGMA journal_size_limit; PRAGMA journal_size_limit = -1; PRAGMA journal_size_limit; PRAGMA journal_size_limit = -3; PRAGMA journal_size_limit; PRAGMA journal_size_limit = on; PRAGMA journal_size_limit; PRAGMA journal_size_limit = off; PRAGMA journal_size_limit; PRAGMA journal_size_limit = yes; PRAGMA journal_size_limit; PRAGMA journal_size_limit = no; PRAGMA journal_size_limit; PRAGMA journal_size_limit = true; PRAGMA journal_size_limit; PRAGMA journal_size_limit = false; PRAGMA journal_size_limit; PRAGMA journal_size_limit = abc; PRAGMA journal_size_limit; PRAGMA journal_size_limit = '1'; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 1.5; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 4294967295; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 4294967296; PRAGMA journal_size_limit; PRAGMA journal_size_limit = 2147483648; PRAGMA journal_size_limit; PRAGMA journal_size_limit = -2147483648; PRAGMA journal_size_limit; PRAGMA journal_size_limit = -2147483649; PRAGMA journal_size_limit;
-- case: catalog/setting-legacy-alter-table-reads-back-what-sqlite-reads-back
PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 0; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 1; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 2; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 3; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 5; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = -1; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = -3; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = on; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = off; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = yes; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = no; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = true; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = false; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = abc; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = '1'; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 1.5; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 4294967295; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 4294967296; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = 2147483648; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = -2147483648; PRAGMA legacy_alter_table; PRAGMA legacy_alter_table = -2147483649; PRAGMA legacy_alter_table;
-- case: catalog/setting-threads-reads-back-what-sqlite-reads-back
PRAGMA threads; PRAGMA threads = 0; PRAGMA threads; PRAGMA threads = 1; PRAGMA threads; PRAGMA threads = 2; PRAGMA threads; PRAGMA threads = 3; PRAGMA threads; PRAGMA threads = 5; PRAGMA threads; PRAGMA threads = -1; PRAGMA threads; PRAGMA threads = -3; PRAGMA threads; PRAGMA threads = on; PRAGMA threads; PRAGMA threads = off; PRAGMA threads; PRAGMA threads = yes; PRAGMA threads; PRAGMA threads = no; PRAGMA threads; PRAGMA threads = true; PRAGMA threads; PRAGMA threads = false; PRAGMA threads; PRAGMA threads = abc; PRAGMA threads; PRAGMA threads = '1'; PRAGMA threads; PRAGMA threads = 1.5; PRAGMA threads; PRAGMA threads = 4294967295; PRAGMA threads; PRAGMA threads = 4294967296; PRAGMA threads; PRAGMA threads = 2147483648; PRAGMA threads; PRAGMA threads = -2147483648; PRAGMA threads; PRAGMA threads = -2147483649; PRAGMA threads;
-- case: catalog/setting-soft-heap-limit-reads-back-what-sqlite-reads-back
PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 0; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 1; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 2; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 3; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 5; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = -1; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = -3; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = on; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = off; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = yes; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = no; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = true; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = false; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = abc; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = '1'; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 1.5; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 4294967295; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 4294967296; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = 2147483648; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = -2147483648; PRAGMA soft_heap_limit; PRAGMA soft_heap_limit = -2147483649; PRAGMA soft_heap_limit;
-- case: catalog/setting-cell-size-check-reads-back-what-sqlite-reads-back
PRAGMA cell_size_check; PRAGMA cell_size_check = 0; PRAGMA cell_size_check; PRAGMA cell_size_check = 1; PRAGMA cell_size_check; PRAGMA cell_size_check = 2; PRAGMA cell_size_check; PRAGMA cell_size_check = 3; PRAGMA cell_size_check; PRAGMA cell_size_check = 5; PRAGMA cell_size_check; PRAGMA cell_size_check = -1; PRAGMA cell_size_check; PRAGMA cell_size_check = -3; PRAGMA cell_size_check; PRAGMA cell_size_check = on; PRAGMA cell_size_check; PRAGMA cell_size_check = off; PRAGMA cell_size_check; PRAGMA cell_size_check = yes; PRAGMA cell_size_check; PRAGMA cell_size_check = no; PRAGMA cell_size_check; PRAGMA cell_size_check = true; PRAGMA cell_size_check; PRAGMA cell_size_check = false; PRAGMA cell_size_check; PRAGMA cell_size_check = abc; PRAGMA cell_size_check; PRAGMA cell_size_check = '1'; PRAGMA cell_size_check; PRAGMA cell_size_check = 1.5; PRAGMA cell_size_check; PRAGMA cell_size_check = 4294967295; PRAGMA cell_size_check; PRAGMA cell_size_check = 4294967296; PRAGMA cell_size_check; PRAGMA cell_size_check = 2147483648; PRAGMA cell_size_check; PRAGMA cell_size_check = -2147483648; PRAGMA cell_size_check; PRAGMA cell_size_check = -2147483649; PRAGMA cell_size_check;
-- case: catalog/setting-checkpoint-fullfsync-reads-back-what-sqlite-reads-back
PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 0; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 1; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 2; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 3; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 5; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = -1; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = -3; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = on; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = off; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = yes; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = no; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = true; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = false; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = abc; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = '1'; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 1.5; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 4294967295; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 4294967296; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = 2147483648; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = -2147483648; PRAGMA checkpoint_fullfsync; PRAGMA checkpoint_fullfsync = -2147483649; PRAGMA checkpoint_fullfsync;
-- case: catalog/setting-fullfsync-reads-back-what-sqlite-reads-back
PRAGMA fullfsync; PRAGMA fullfsync = 0; PRAGMA fullfsync; PRAGMA fullfsync = 1; PRAGMA fullfsync; PRAGMA fullfsync = 2; PRAGMA fullfsync; PRAGMA fullfsync = 3; PRAGMA fullfsync; PRAGMA fullfsync = 5; PRAGMA fullfsync; PRAGMA fullfsync = -1; PRAGMA fullfsync; PRAGMA fullfsync = -3; PRAGMA fullfsync; PRAGMA fullfsync = on; PRAGMA fullfsync; PRAGMA fullfsync = off; PRAGMA fullfsync; PRAGMA fullfsync = yes; PRAGMA fullfsync; PRAGMA fullfsync = no; PRAGMA fullfsync; PRAGMA fullfsync = true; PRAGMA fullfsync; PRAGMA fullfsync = false; PRAGMA fullfsync; PRAGMA fullfsync = abc; PRAGMA fullfsync; PRAGMA fullfsync = '1'; PRAGMA fullfsync; PRAGMA fullfsync = 1.5; PRAGMA fullfsync; PRAGMA fullfsync = 4294967295; PRAGMA fullfsync; PRAGMA fullfsync = 4294967296; PRAGMA fullfsync; PRAGMA fullfsync = 2147483648; PRAGMA fullfsync; PRAGMA fullfsync = -2147483648; PRAGMA fullfsync; PRAGMA fullfsync = -2147483649; PRAGMA fullfsync;
-- case: catalog/setting-count-changes-reads-back-what-sqlite-reads-back
PRAGMA count_changes; PRAGMA count_changes = 0; PRAGMA count_changes; PRAGMA count_changes = 1; PRAGMA count_changes; PRAGMA count_changes = 2; PRAGMA count_changes; PRAGMA count_changes = 3; PRAGMA count_changes; PRAGMA count_changes = 5; PRAGMA count_changes; PRAGMA count_changes = -1; PRAGMA count_changes; PRAGMA count_changes = -3; PRAGMA count_changes; PRAGMA count_changes = on; PRAGMA count_changes; PRAGMA count_changes = off; PRAGMA count_changes; PRAGMA count_changes = yes; PRAGMA count_changes; PRAGMA count_changes = no; PRAGMA count_changes; PRAGMA count_changes = true; PRAGMA count_changes; PRAGMA count_changes = false; PRAGMA count_changes; PRAGMA count_changes = abc; PRAGMA count_changes; PRAGMA count_changes = '1'; PRAGMA count_changes; PRAGMA count_changes = 1.5; PRAGMA count_changes; PRAGMA count_changes = 4294967295; PRAGMA count_changes; PRAGMA count_changes = 4294967296; PRAGMA count_changes; PRAGMA count_changes = 2147483648; PRAGMA count_changes; PRAGMA count_changes = -2147483648; PRAGMA count_changes; PRAGMA count_changes = -2147483649; PRAGMA count_changes;
-- case: catalog/setting-empty-result-callbacks-reads-back-what-sqlite-reads-back
PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 0; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 1; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 2; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 3; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 5; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = -1; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = -3; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = on; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = off; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = yes; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = no; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = true; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = false; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = abc; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = '1'; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 1.5; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 4294967295; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 4294967296; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = 2147483648; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = -2147483648; PRAGMA empty_result_callbacks; PRAGMA empty_result_callbacks = -2147483649; PRAGMA empty_result_callbacks;
-- case: catalog/setting-full-column-names-reads-back-what-sqlite-reads-back
PRAGMA full_column_names; PRAGMA full_column_names = 0; PRAGMA full_column_names; PRAGMA full_column_names = 1; PRAGMA full_column_names; PRAGMA full_column_names = 2; PRAGMA full_column_names; PRAGMA full_column_names = 3; PRAGMA full_column_names; PRAGMA full_column_names = 5; PRAGMA full_column_names; PRAGMA full_column_names = -1; PRAGMA full_column_names; PRAGMA full_column_names = -3; PRAGMA full_column_names; PRAGMA full_column_names = on; PRAGMA full_column_names; PRAGMA full_column_names = off; PRAGMA full_column_names; PRAGMA full_column_names = yes; PRAGMA full_column_names; PRAGMA full_column_names = no; PRAGMA full_column_names; PRAGMA full_column_names = true; PRAGMA full_column_names; PRAGMA full_column_names = false; PRAGMA full_column_names; PRAGMA full_column_names = abc; PRAGMA full_column_names; PRAGMA full_column_names = '1'; PRAGMA full_column_names; PRAGMA full_column_names = 1.5; PRAGMA full_column_names; PRAGMA full_column_names = 4294967295; PRAGMA full_column_names; PRAGMA full_column_names = 4294967296; PRAGMA full_column_names; PRAGMA full_column_names = 2147483648; PRAGMA full_column_names; PRAGMA full_column_names = -2147483648; PRAGMA full_column_names; PRAGMA full_column_names = -2147483649; PRAGMA full_column_names;
-- case: catalog/setting-short-column-names-reads-back-what-sqlite-reads-back
PRAGMA short_column_names; PRAGMA short_column_names = 0; PRAGMA short_column_names; PRAGMA short_column_names = 1; PRAGMA short_column_names; PRAGMA short_column_names = 2; PRAGMA short_column_names; PRAGMA short_column_names = 3; PRAGMA short_column_names; PRAGMA short_column_names = 5; PRAGMA short_column_names; PRAGMA short_column_names = -1; PRAGMA short_column_names; PRAGMA short_column_names = -3; PRAGMA short_column_names; PRAGMA short_column_names = on; PRAGMA short_column_names; PRAGMA short_column_names = off; PRAGMA short_column_names; PRAGMA short_column_names = yes; PRAGMA short_column_names; PRAGMA short_column_names = no; PRAGMA short_column_names; PRAGMA short_column_names = true; PRAGMA short_column_names; PRAGMA short_column_names = false; PRAGMA short_column_names; PRAGMA short_column_names = abc; PRAGMA short_column_names; PRAGMA short_column_names = '1'; PRAGMA short_column_names; PRAGMA short_column_names = 1.5; PRAGMA short_column_names; PRAGMA short_column_names = 4294967295; PRAGMA short_column_names; PRAGMA short_column_names = 4294967296; PRAGMA short_column_names; PRAGMA short_column_names = 2147483648; PRAGMA short_column_names; PRAGMA short_column_names = -2147483648; PRAGMA short_column_names; PRAGMA short_column_names = -2147483649; PRAGMA short_column_names;
-- case: catalog/setting-read-uncommitted-reads-back-what-sqlite-reads-back
PRAGMA read_uncommitted; PRAGMA read_uncommitted = 0; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 1; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 2; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 3; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 5; PRAGMA read_uncommitted; PRAGMA read_uncommitted = -1; PRAGMA read_uncommitted; PRAGMA read_uncommitted = -3; PRAGMA read_uncommitted; PRAGMA read_uncommitted = on; PRAGMA read_uncommitted; PRAGMA read_uncommitted = off; PRAGMA read_uncommitted; PRAGMA read_uncommitted = yes; PRAGMA read_uncommitted; PRAGMA read_uncommitted = no; PRAGMA read_uncommitted; PRAGMA read_uncommitted = true; PRAGMA read_uncommitted; PRAGMA read_uncommitted = false; PRAGMA read_uncommitted; PRAGMA read_uncommitted = abc; PRAGMA read_uncommitted; PRAGMA read_uncommitted = '1'; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 1.5; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 4294967295; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 4294967296; PRAGMA read_uncommitted; PRAGMA read_uncommitted = 2147483648; PRAGMA read_uncommitted; PRAGMA read_uncommitted = -2147483648; PRAGMA read_uncommitted; PRAGMA read_uncommitted = -2147483649; PRAGMA read_uncommitted;
-- case: catalog/setting-reverse-unordered-selects-reads-back-what-sqlite-reads-back
PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 0; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 1; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 2; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 3; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 5; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = -1; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = -3; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = on; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = off; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = yes; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = no; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = true; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = false; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = abc; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = '1'; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 1.5; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 4294967295; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 4294967296; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = 2147483648; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = -2147483648; PRAGMA reverse_unordered_selects; PRAGMA reverse_unordered_selects = -2147483649; PRAGMA reverse_unordered_selects;
-- case: catalog/setting-cache-spill-reads-back-what-sqlite-reads-back
PRAGMA cache_spill; PRAGMA cache_spill = 0; PRAGMA cache_spill; PRAGMA cache_spill = 1; PRAGMA cache_spill; PRAGMA cache_spill = 2; PRAGMA cache_spill; PRAGMA cache_spill = 3; PRAGMA cache_spill; PRAGMA cache_spill = 5; PRAGMA cache_spill; PRAGMA cache_spill = -1; PRAGMA cache_spill; PRAGMA cache_spill = -3; PRAGMA cache_spill; PRAGMA cache_spill = on; PRAGMA cache_spill; PRAGMA cache_spill = off; PRAGMA cache_spill; PRAGMA cache_spill = yes; PRAGMA cache_spill; PRAGMA cache_spill = no; PRAGMA cache_spill; PRAGMA cache_spill = true; PRAGMA cache_spill; PRAGMA cache_spill = false; PRAGMA cache_spill; PRAGMA cache_spill = abc; PRAGMA cache_spill; PRAGMA cache_spill = '1'; PRAGMA cache_spill; PRAGMA cache_spill = 1.5; PRAGMA cache_spill; PRAGMA cache_spill = 4294967295; PRAGMA cache_spill; PRAGMA cache_spill = 4294967296; PRAGMA cache_spill; PRAGMA cache_spill = 2147483648; PRAGMA cache_spill;
-- case: catalog/setting-user-version-and-application-id-are-read-as-a-32-bit-integer
PRAGMA user_version; PRAGMA user_version = 0; PRAGMA user_version; PRAGMA user_version = 1; PRAGMA user_version; PRAGMA user_version = 2; PRAGMA user_version; PRAGMA user_version = 3; PRAGMA user_version; PRAGMA user_version = 5; PRAGMA user_version; PRAGMA user_version = -1; PRAGMA user_version; PRAGMA user_version = -3; PRAGMA user_version; PRAGMA user_version = on; PRAGMA user_version; PRAGMA user_version = off; PRAGMA user_version; PRAGMA user_version = yes; PRAGMA user_version; PRAGMA user_version = no; PRAGMA user_version; PRAGMA user_version = true; PRAGMA user_version; PRAGMA user_version = false; PRAGMA user_version; PRAGMA user_version = abc; PRAGMA user_version; PRAGMA user_version = '1'; PRAGMA user_version; PRAGMA user_version = 1.5; PRAGMA user_version; PRAGMA user_version = 4294967295; PRAGMA user_version; PRAGMA user_version = 4294967296; PRAGMA user_version; PRAGMA user_version = 2147483648; PRAGMA user_version; PRAGMA user_version = -2147483648; PRAGMA user_version; PRAGMA user_version = -2147483649; PRAGMA user_version;
PRAGMA application_id; PRAGMA application_id = 0; PRAGMA application_id; PRAGMA application_id = 1; PRAGMA application_id; PRAGMA application_id = 2; PRAGMA application_id; PRAGMA application_id = 3; PRAGMA application_id; PRAGMA application_id = 5; PRAGMA application_id; PRAGMA application_id = -1; PRAGMA application_id; PRAGMA application_id = -3; PRAGMA application_id; PRAGMA application_id = on; PRAGMA application_id; PRAGMA application_id = off; PRAGMA application_id; PRAGMA application_id = yes; PRAGMA application_id; PRAGMA application_id = no; PRAGMA application_id; PRAGMA application_id = true; PRAGMA application_id; PRAGMA application_id = false; PRAGMA application_id; PRAGMA application_id = abc; PRAGMA application_id; PRAGMA application_id = '1'; PRAGMA application_id; PRAGMA application_id = 1.5; PRAGMA application_id; PRAGMA application_id = 4294967295; PRAGMA application_id; PRAGMA application_id = 4294967296; PRAGMA application_id; PRAGMA application_id = 2147483648; PRAGMA application_id; PRAGMA application_id = -2147483648; PRAGMA application_id; PRAGMA application_id = -2147483649; PRAGMA application_id;
-- case: catalog/setting-temp-store-reads-the-first-character-or-file-or-memory
PRAGMA temp_store; PRAGMA temp_store = 0; PRAGMA temp_store; PRAGMA temp_store = 1; PRAGMA temp_store; PRAGMA temp_store = 2; PRAGMA temp_store; PRAGMA temp_store = 3; PRAGMA temp_store; PRAGMA temp_store = 5; PRAGMA temp_store; PRAGMA temp_store = -1; PRAGMA temp_store; PRAGMA temp_store = on; PRAGMA temp_store; PRAGMA temp_store = off; PRAGMA temp_store; PRAGMA temp_store = file; PRAGMA temp_store; PRAGMA temp_store = FILE; PRAGMA temp_store; PRAGMA temp_store = memory; PRAGMA temp_store; PRAGMA temp_store = MEMORY; PRAGMA temp_store; PRAGMA temp_store = default; PRAGMA temp_store; PRAGMA temp_store = abc; PRAGMA temp_store; PRAGMA temp_store = '1'; PRAGMA temp_store; PRAGMA temp_store = 1.5; PRAGMA temp_store; PRAGMA temp_store = 2147483648; PRAGMA temp_store; PRAGMA temp_store = '2x'; PRAGMA temp_store;
-- case: catalog/setting-synchronous-keeps-three-bits
PRAGMA synchronous; PRAGMA synchronous = 0; PRAGMA synchronous; PRAGMA synchronous = 1; PRAGMA synchronous; PRAGMA synchronous = 2; PRAGMA synchronous; PRAGMA synchronous = 3; PRAGMA synchronous; PRAGMA synchronous = 5; PRAGMA synchronous; PRAGMA synchronous = -1; PRAGMA synchronous; PRAGMA synchronous = -3; PRAGMA synchronous; PRAGMA synchronous = on; PRAGMA synchronous; PRAGMA synchronous = off; PRAGMA synchronous; PRAGMA synchronous = yes; PRAGMA synchronous; PRAGMA synchronous = no; PRAGMA synchronous; PRAGMA synchronous = true; PRAGMA synchronous; PRAGMA synchronous = false; PRAGMA synchronous; PRAGMA synchronous = abc; PRAGMA synchronous; PRAGMA synchronous = '1'; PRAGMA synchronous; PRAGMA synchronous = 1.5; PRAGMA synchronous; PRAGMA synchronous = 4294967295; PRAGMA synchronous; PRAGMA synchronous = 4294967296; PRAGMA synchronous; PRAGMA synchronous = 2147483648; PRAGMA synchronous; PRAGMA synchronous = -2147483648; PRAGMA synchronous; PRAGMA synchronous = -2147483649; PRAGMA synchronous; PRAGMA synchronous = extra; PRAGMA synchronous; PRAGMA synchronous = full; PRAGMA synchronous; PRAGMA synchronous = normal; PRAGMA synchronous; PRAGMA synchronous = EXTRA; PRAGMA synchronous; PRAGMA synchronous = 16; PRAGMA synchronous; PRAGMA synchronous = 7; PRAGMA synchronous; PRAGMA synchronous = 8; PRAGMA synchronous;
-- case: catalog/setting-busy-timeout-reads-a-32-bit-integer-and-clears-on-negative
PRAGMA busy_timeout = 0; PRAGMA busy_timeout = 0; PRAGMA busy_timeout; PRAGMA busy_timeout = 1; PRAGMA busy_timeout; PRAGMA busy_timeout = 2; PRAGMA busy_timeout; PRAGMA busy_timeout = 3; PRAGMA busy_timeout; PRAGMA busy_timeout = 5; PRAGMA busy_timeout; PRAGMA busy_timeout = -1; PRAGMA busy_timeout; PRAGMA busy_timeout = -3; PRAGMA busy_timeout; PRAGMA busy_timeout = on; PRAGMA busy_timeout; PRAGMA busy_timeout = off; PRAGMA busy_timeout; PRAGMA busy_timeout = yes; PRAGMA busy_timeout; PRAGMA busy_timeout = no; PRAGMA busy_timeout; PRAGMA busy_timeout = true; PRAGMA busy_timeout; PRAGMA busy_timeout = false; PRAGMA busy_timeout; PRAGMA busy_timeout = abc; PRAGMA busy_timeout; PRAGMA busy_timeout = '1'; PRAGMA busy_timeout; PRAGMA busy_timeout = 1.5; PRAGMA busy_timeout; PRAGMA busy_timeout = 4294967295; PRAGMA busy_timeout; PRAGMA busy_timeout = 4294967296; PRAGMA busy_timeout; PRAGMA busy_timeout = 2147483648; PRAGMA busy_timeout; PRAGMA busy_timeout = -2147483648; PRAGMA busy_timeout; PRAGMA busy_timeout = -2147483649; PRAGMA busy_timeout;
-- case: catalog/setting-foreign-keys-is-not-changed-inside-a-transaction-or-savepoint
PRAGMA foreign_keys; SAVEPOINT s1; PRAGMA foreign_keys = ON; PRAGMA foreign_keys; RELEASE s1; PRAGMA foreign_keys = ON; PRAGMA foreign_keys; BEGIN; PRAGMA foreign_keys = OFF; PRAGMA foreign_keys; COMMIT; PRAGMA foreign_keys; BEGIN; PRAGMA foreign_keys = OFF; ROLLBACK; PRAGMA foreign_keys; PRAGMA foreign_keys = OFF; BEGIN; PRAGMA foreign_keys = ON; PRAGMA foreign_keys; COMMIT; PRAGMA foreign_keys;
-- case: catalog/setting-foreign-keys-argument-spellings
PRAGMA foreign_keys = on; PRAGMA foreign_keys; PRAGMA foreign_keys = 'on'; PRAGMA foreign_keys; PRAGMA foreign_keys = yes; PRAGMA foreign_keys; PRAGMA foreign_keys = 0; PRAGMA foreign_keys; PRAGMA foreign_keys = 'maybe'; PRAGMA foreign_keys; PRAGMA foreign_keys = 2; PRAGMA foreign_keys; PRAGMA foreign_keys = -1; PRAGMA foreign_keys; PRAGMA foreign_keys = 1.9; PRAGMA foreign_keys; PRAGMA foreign_keys = TRUE; PRAGMA foreign_keys; PRAGMA foreign_keys = false; PRAGMA foreign_keys; PRAGMA foreign_keys = '1'; PRAGMA foreign_keys;
-- case: catalog/setting-foreign-keys-off-inside-a-transaction-still-enforces-keys
PRAGMA foreign_keys = ON;
CREATE TABLE p(id INTEGER PRIMARY KEY);
CREATE TABLE c(pid REFERENCES p(id));
INSERT INTO c VALUES(1);
BEGIN;
PRAGMA foreign_keys = OFF;
INSERT INTO c VALUES(2);
COMMIT;
SELECT * FROM c;
-- case: catalog/setting-settings-survive-vacuum
PRAGMA mmap_size = 1000; PRAGMA fullfsync = 1; PRAGMA synchronous = 3; CREATE TABLE t(a); VACUUM; PRAGMA mmap_size; PRAGMA fullfsync; PRAGMA synchronous;
-- case: catalog/begin-immediate-and-exclusive-fail-under-query-only-and-deferred-does-not
PRAGMA query_only = ON;
BEGIN IMMEDIATE;
COMMIT;
BEGIN EXCLUSIVE;
COMMIT;
BEGIN DEFERRED;
COMMIT;
BEGIN;
SELECT 1;
COMMIT;
PRAGMA query_only = OFF;
BEGIN IMMEDIATE;
COMMIT;
-- case: catalog/wal-checkpoint-truncate-reports-an-empty-log
PRAGMA journal_mode = WAL;
CREATE TABLE t(a);
INSERT INTO t VALUES(1);
PRAGMA wal_checkpoint(TRUNCATE);
PRAGMA wal_checkpoint(TRUNCATE);
PRAGMA wal_checkpoint(truncate);
INSERT INTO t VALUES(2);
PRAGMA wal_checkpoint(TRUNCATE);
SELECT count(*) FROM t;
PRAGMA journal_mode = DELETE;
PRAGMA wal_checkpoint(TRUNCATE);
-- case: catalog/wal-checkpoint-inside-a-transaction-that-wrote-is-locked
PRAGMA journal_mode = WAL;
CREATE TABLE t(a);
BEGIN;
INSERT INTO t VALUES(1);
PRAGMA wal_checkpoint(PASSIVE);
COMMIT;
SELECT count(*) FROM t;
-- case: catalog/explain-query-plan-names-both-ends-of-a-range-and-expression-keys
CREATE TABLE t(a TEXT, b INTEGER, c, d); CREATE INDEX i ON t(a); CREATE INDEX ib ON t(b, c); CREATE INDEX il ON t(lower(c)); CREATE INDEX ie ON t(b+1, d);
EXPLAIN QUERY PLAN SELECT * FROM t WHERE a >= 'ab' AND a < 'ac';
EXPLAIN QUERY PLAN SELECT * FROM t WHERE a BETWEEN 'a' AND 'b';
EXPLAIN QUERY PLAN SELECT * FROM t WHERE a GLOB 'ab*';
EXPLAIN QUERY PLAN SELECT * FROM t WHERE b = 1 AND c > 2;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE b = 1 AND c > 2 AND c < 5;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE b = 1 AND c = 2;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE b >= 1 AND b <= 4;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE lower(c) = 'x';
EXPLAIN QUERY PLAN SELECT * FROM t WHERE b+1 = 3 AND d = 4;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE rowid = 4;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE rowid > 4;
EXPLAIN QUERY PLAN SELECT * FROM t WHERE rowid > 4 AND rowid < 10;
EXPLAIN QUERY PLAN SELECT a FROM t WHERE a = 'x';
-- case: pragma/auto-vacuum-belongs-to-the-named-database
ATTACH ':memory:' AS aux0;
ATTACH ':memory:' AS aux1;
PRAGMA aux0.auto_vacuum;
PRAGMA aux0.auto_vacuum = full;
PRAGMA aux0.auto_vacuum;
PRAGMA aux1.auto_vacuum = incremental;
PRAGMA main.auto_vacuum;
PRAGMA aux1.auto_vacuum;
CREATE TABLE aux0.a(x);
CREATE TABLE main.m(x);
PRAGMA aux0.auto_vacuum = none;
PRAGMA main.auto_vacuum;
PRAGMA aux0.auto_vacuum;
PRAGMA aux1.auto_vacuum;
PRAGMA temp.auto_vacuum;
