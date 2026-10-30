-- case: bug-hunt-11/migrate/dump-index-trailing-line-comment
CREATE TABLE t(a);
CREATE INDEX i ON t(a) -- note
;
.dump
-- case: bug-hunt-11/migrate/dump-trigger-comment-inside-body
CREATE TABLE t(a);
CREATE TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; -- note
END;
.dump
-- case: bug-hunt-11/migrate/dump-table-block-comment
CREATE TABLE t(a /* one */, b); INSERT INTO t VALUES(1,2);
.dump
