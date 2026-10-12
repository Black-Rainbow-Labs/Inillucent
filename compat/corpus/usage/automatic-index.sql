-- case: automatic-index/answers-of-the-fourteen-statements
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
SELECT count(*) FROM a JOIN b ON a.k = b.k;
SELECT count(*) FROM a, b WHERE a.k = b.k;
SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k;
SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k WHERE b.id IS NULL;
SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
SELECT count(*) FROM a WHERE a.k IN (SELECT k FROM b WHERE b.id > a.id);
SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a;
SELECT count(*) FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0;
SELECT count(*) FROM a JOIN b ON a.k = b.k + 1;
SELECT count(*) FROM a JOIN b ON a.k = b.k AND a.id < b.id;
SELECT count(*) FROM a x JOIN a y ON x.k = y.k JOIN b z ON z.k = y.k;
SELECT count(*) FROM a WHERE k IN (SELECT k FROM b);
SELECT count(*) FROM a JOIN (SELECT k, count(*) n FROM b GROUP BY k) s ON s.k = a.k;
-- case: automatic-index/answers-with-the-pragma-off
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
PRAGMA automatic_index = OFF;
SELECT count(*) FROM a JOIN b ON a.k = b.k;
SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a;
SELECT count(*) FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0;
SELECT count(*) FROM a JOIN b ON a.k = b.k + 1;
SELECT count(*) FROM a JOIN b ON a.k = b.k AND a.id < b.id;
-- case: automatic-index/outer-joins-with-more-than-a-key
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
ALTER TABLE b ADD COLUMN active INT;
UPDATE b SET active = id % 3 = 0;
SELECT count(*), count(b.id) FROM a LEFT JOIN b ON a.k = b.k AND b.active = 1;
SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k AND a.id < b.id WHERE b.id IS NULL;
SELECT count(*), count(a.id), count(b.id) FROM a RIGHT JOIN b ON a.k = b.k AND b.active = 1;
SELECT count(*), count(a.id), count(b.id) FROM a FULL JOIN b ON a.k = b.k AND a.id < b.id;
SELECT count(*) FROM a JOIN b ON a.k = abs(b.k);
SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k + 1 WHERE b.id IS NULL;
SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k AND b.active = 1);
SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k AND b.id > a.id)) FROM a;
-- case: automatic-index/plan-of-statement-1
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a JOIN b ON a.k = b.k;
-- case: automatic-index/plan-of-statement-3
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a LEFT JOIN b ON a.k = b.k;
-- case: automatic-index/plan-of-statement-5
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
-- case: automatic-index/plan-of-statement-7
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
-- case: automatic-index/plan-of-statement-8
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a;
-- case: automatic-index/plan-of-statement-9
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0;
-- case: automatic-index/plan-of-statement-10
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a JOIN b ON a.k = b.k + 1;
-- case: automatic-index/plan-of-statement-11
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
EXPLAIN QUERY PLAN SELECT count(*) FROM a JOIN b ON a.k = b.k AND a.id < b.id;
-- case: automatic-index/plan-of-statement-5-with-the-pragma-off
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
PRAGMA automatic_index = OFF;
EXPLAIN QUERY PLAN SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
-- case: automatic-index/plan-of-statement-7-with-the-pragma-off
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
PRAGMA automatic_index = OFF;
EXPLAIN QUERY PLAN SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
-- case: automatic-index/plan-of-statement-8-with-the-pragma-off
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT);
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT);
INSERT INTO a SELECT value, value % 300, 'aaaaaaaaaaaaaaaaaaaa' FROM generate_series(1, 300);
INSERT INTO b SELECT value, (value * 7) % 300, 'bbbbbbbbbbbbbbbbbbbb' FROM generate_series(1, 300);
PRAGMA automatic_index = OFF;
EXPLAIN QUERY PLAN SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a;
-- case: automatic-index/a-trigger-that-writes-the-table-its-subquery-reads
CREATE TABLE a(k);
CREATE TABLE b(k);
CREATE TABLE log(n);
INSERT INTO b VALUES(1);
CREATE TRIGGER tr AFTER INSERT ON a BEGIN
  INSERT INTO b VALUES(new.k);
  INSERT INTO log SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k);
END;
INSERT INTO a VALUES(1);
INSERT INTO a SELECT value FROM generate_series(2, 5);
SELECT group_concat(n) FROM log;
CREATE TABLE c(k, n);
INSERT INTO c VALUES(1,0),(2,0),(3,0);
CREATE TRIGGER tc AFTER UPDATE ON c BEGIN INSERT INTO b VALUES(new.k + 10); END;
SELECT k, (SELECT count(*) FROM b WHERE b.k = c.k + 10) FROM c;
UPDATE c SET n = (SELECT count(*) FROM b WHERE b.k = c.k + 9);
SELECT group_concat(k || ':' || n) FROM c;
SELECT a.k, (SELECT count(*) FROM b WHERE b.k = a.k) FROM a ORDER BY a.k;
