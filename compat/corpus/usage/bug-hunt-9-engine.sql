-- case: bug-hunt-9/engine/desc-index-append-past-one-leaf
CREATE TABLE t(a INTEGER PRIMARY KEY, c);
CREATE INDEX ic ON t(c DESC);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3000) INSERT INTO t SELECT n, -1 FROM r;
INSERT INTO t SELECT a + 10000, -1 FROM t;
PRAGMA integrity_check;
SELECT count(*), min(a), max(a) FROM t WHERE c = -1;
-- reopen
PRAGMA integrity_check;
SELECT count(*) FROM t WHERE c = -1 AND a > 10000;
-- case: bug-hunt-9/engine/desc-index-update-moves-keys-between-leaves
CREATE TABLE t(a PRIMARY KEY, b UNIQUE, c);
CREATE INDEX ic ON t(c DESC, a);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3000) INSERT INTO t SELECT n, 'b' || n, n % 7 FROM r;
DELETE FROM t WHERE a % 3 = 0;
UPDATE t SET c = c + 1, b = b || 'x' WHERE a % 5 = 0;
REPLACE INTO t VALUES(1, 'b2', 99);
PRAGMA integrity_check;
SELECT count(*), sum(c) FROM t;
SELECT count(*) FROM t WHERE c = 3;
SELECT count(*) FROM t WHERE c > 4;
SELECT a FROM t WHERE c BETWEEN 2 AND 3 ORDER BY c DESC, a LIMIT 4;
-- case: bug-hunt-9/engine/desc-index-low-values-arriving
CREATE TABLE t(id INTEGER PRIMARY KEY, c, s);
CREATE INDEX i ON t(c, s DESC);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 4000) INSERT INTO t SELECT n, n % 7, printf('%s%d', char(65 + n % 26, 97 + n % 26), n % 500) FROM r;
UPDATE t SET c = -1, s = 'zz' || s WHERE id % 3 = 0;
PRAGMA integrity_check;
SELECT count(*), count(DISTINCT s) FROM t WHERE c = -1;
SELECT id, s FROM t WHERE c = 2 ORDER BY s DESC, id LIMIT 3;
-- case: bug-hunt-9/engine/desc-nocase-index-on-a-table-without-rowid
CREATE TABLE t(id PRIMARY KEY, c, s) WITHOUT ROWID;
CREATE INDEX i ON t(s COLLATE NOCASE DESC, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 4000) INSERT INTO t SELECT n, n % 7, printf('%s%d', char(65 + n % 26, 97 + n % 26), n % 500) FROM r;
DELETE FROM t WHERE id % 4 = 1;
INSERT INTO t SELECT id + 200000, 3 - c, upper(s) FROM t WHERE id % 7 = 0;
PRAGMA integrity_check;
SELECT count(*) FROM t WHERE s COLLATE NOCASE = 'ab3';
SELECT count(*) FROM t WHERE s COLLATE NOCASE > 'Y';
-- case: bug-hunt-9/engine/desc-expression-and-partial-indexes
CREATE TABLE t(id, c, s);
CREATE INDEX i1 ON t(length(s) DESC);
CREATE INDEX i2 ON t(s DESC) WHERE c % 2 = 0;
CREATE UNIQUE INDEX i3 ON t(id DESC, c);
WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 4000) INSERT INTO t SELECT n, n % 7, printf('%s%d', char(65 + n % 26, 97 + n % 26), n % 500) FROM r;
INSERT INTO t SELECT id + 100000, -5, '' FROM t;
UPDATE t SET c = c + 1 WHERE id % 5 = 0;
PRAGMA integrity_check;
SELECT count(*) FROM t WHERE length(s) > 3;
SELECT count(*) FROM t WHERE c % 2 = 0 AND s > 'M';
SELECT count(*) FROM t WHERE id > 100000 AND c = -4;
-- case: bug-hunt-9/engine/group-by-over-a-covering-index-with-a-real-sum
CREATE TABLE p(id INTEGER PRIMARY KEY, cat TEXT); CREATE INDEX p_cat ON p(cat);
CREATE TABLE oi(id INTEGER PRIMARY KEY, pid INTEGER, price REAL); CREATE INDEX oi_p ON oi(pid);
INSERT INTO p VALUES(1,'a'),(2,'b'),(3,'a'),(4,'b');
INSERT INTO oi VALUES(1,1,1.5),(2,2,2.5),(3,3,4.5),(4,4,8.5),(5,1,16.5);
SELECT p.cat, sum(oi.price) FROM p INDEXED BY p_cat JOIN oi ON p.id=oi.pid GROUP BY p.cat;
SELECT p.cat, avg(oi.price), total(oi.price) FROM p INDEXED BY p_cat JOIN oi ON p.id=oi.pid GROUP BY p.cat;
SELECT p.cat, group_concat(oi.id) FROM p INDEXED BY p_cat JOIN oi ON p.id=oi.pid GROUP BY p.cat;
SELECT p.cat, sum(oi.id + 0.5), count(*) FROM p INDEXED BY p_cat JOIN oi ON p.id=oi.pid GROUP BY p.cat ORDER BY p.cat DESC;
SELECT cat, sum(id * 1.5), group_concat(id) FROM p GROUP BY cat;
SELECT sum(oi.price) FROM p INDEXED BY p_cat JOIN oi ON p.id=oi.pid;
-- case: bug-hunt-9/engine/group-by-over-a-covering-index-chosen-by-the-planner
CREATE TABLE products(id INTEGER PRIMARY KEY, category TEXT, name TEXT);
CREATE INDEX products_category ON products(category);
CREATE TABLE order_items(id INTEGER PRIMARY KEY, product_id INTEGER, qty INTEGER, price REAL);
CREATE INDEX order_items_product ON order_items(product_id);
INSERT INTO products SELECT value, 'c' || (value % 5), 'p' || value FROM generate_series(1, 400);
INSERT INTO order_items SELECT value, 1 + value % 400, 1 + value % 3, (value % 50) / 4.0 FROM generate_series(1, 4000);
ANALYZE;
SELECT p.category, sum(oi.qty * oi.price), count(*) FROM order_items oi JOIN products p ON p.id = oi.product_id GROUP BY p.category ORDER BY 2 DESC;
-- case: bug-hunt-9/engine/sum-overflows-on-the-running-total-on-every-read
CREATE TABLE t(i INTEGER);
INSERT INTO t VALUES (1),(9223372036854775807),(-9223372036854775808),(0);
SELECT count(*) FROM t;
SELECT sum(i) FROM t;
SELECT sum(i) FROM t;
SELECT avg(i), total(i) FROM t;
SELECT sum(i) FROM t WHERE i <> 1;
CREATE TABLE u(g, i INTEGER);
INSERT INTO u VALUES (1, 9223372036854775807), (1, 1), (2, 5);
SELECT g, sum(i) FROM u WHERE g = 2 GROUP BY g;
SELECT g, sum(i) FROM u GROUP BY g;
-- case: bug-hunt-9/engine/recursive-triggers-fire-only-for-their-own-event
PRAGMA recursive_triggers=ON;
CREATE TABLE t(a);
CREATE TRIGGER g AFTER INSERT ON t BEGIN UPDATE t SET a=a+1 WHERE rowid=new.rowid; END;
INSERT INTO t VALUES(1);
SELECT * FROM t;
CREATE TABLE u(a);
CREATE TRIGGER h AFTER UPDATE ON u BEGIN INSERT INTO u VALUES(9); END;
INSERT INTO u VALUES(1);
UPDATE u SET a = 3;
SELECT * FROM u;
-- case: bug-hunt-9/engine/recursive-triggers-recurse-on-their-own-event
PRAGMA recursive_triggers=ON;
CREATE TABLE t(a);
CREATE TRIGGER g AFTER INSERT ON t WHEN new.a < 5 BEGIN INSERT INTO t VALUES(new.a + 1); END;
INSERT INTO t VALUES(1);
SELECT * FROM t;
CREATE TABLE v(a, b);
CREATE TRIGGER k AFTER UPDATE OF a ON v WHEN new.a < 5 BEGIN UPDATE v SET a = a + 1, b = b + 1; END;
CREATE TRIGGER k2 AFTER UPDATE OF b ON v BEGIN UPDATE v SET b = b; END;
INSERT INTO v VALUES(1, 0);
UPDATE v SET a = 2;
SELECT * FROM v;
CREATE TABLE w(a);
CREATE TRIGGER d AFTER DELETE ON w BEGIN DELETE FROM w WHERE a = old.a + 1; END;
INSERT INTO w VALUES(1),(2),(3),(4),(10);
DELETE FROM w WHERE a = 1;
SELECT * FROM w;
-- case: bug-hunt-9/engine/a-row-of-many-values-each-under-the-spill-threshold
CREATE TABLE t(id INTEGER PRIMARY KEY, c0 TEXT, c1 TEXT, c2 TEXT, c3 TEXT, c4 TEXT, c5 TEXT, c6 TEXT, c7 TEXT, c8 TEXT, c9 TEXT, c10 TEXT, c11 TEXT, c12 TEXT, c13 TEXT, c14 TEXT, c15 TEXT, c16 TEXT);
CREATE INDEX t3 ON t(c3);
INSERT INTO t SELECT value, printf('%.2000c', 'a') || value, printf('%.2000c', 'b') || value, printf('%.2000c', 'c') || value, printf('%.2000c', 'd') || value, printf('%.2000c', 'e') || value, printf('%.2000c', 'f') || value, printf('%.2000c', 'g') || value, printf('%.2000c', 'h') || value, printf('%.2000c', 'i') || value, printf('%.2000c', 'j') || value, printf('%.2000c', 'k') || value, printf('%.2000c', 'l') || value, printf('%.2000c', 'm') || value, printf('%.2000c', 'n') || value, printf('%.2000c', 'o') || value, printf('%.2000c', 'p') || value, printf('%.2000c', 'q') || value FROM generate_series(1, 300);
SELECT count(*), sum(length(c0)+length(c16)), min(substr(c5,1,3)), max(substr(c16, -3)) FROM t;
UPDATE t SET c1 = c1 || printf('%.1500c', 'z') WHERE id % 3 = 0;
DELETE FROM t WHERE id % 7 = 0;
INSERT INTO t(id, c0, c9) VALUES(1000, printf('%.4000c', 'q'), printf('%.3000c', 'r'));
PRAGMA integrity_check;
SELECT count(*), sum(length(c1)), sum(length(c0)) FROM t;
SELECT id FROM t WHERE c3 = printf('%.2000c', 'd') || 5;
-- reopen
VACUUM;
PRAGMA integrity_check;
SELECT count(*), sum(length(c1)), sum(length(c9)) FROM t;
CREATE TABLE u(x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,x25,x26,x27,x28,x29,x30,x31,x32,x33,x34,x35,x36,x37,x38,x39,x40,x41,x42,x43,x44,x45,x46,x47,x48,x49);
INSERT INTO u SELECT printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y'),printf('%.800c','y') FROM generate_series(1,40);
SELECT count(*), sum(length(x49)) FROM u;
PRAGMA integrity_check;
-- case: bug-hunt-9/engine/substr-and-trim-over-large-and-odd-text
SELECT printf('%#.20x|%,.8d|%.30d|%.12X', 255, 1234, -5, 3054);
CREATE TABLE t(s, b BLOB);
INSERT INTO t VALUES(printf('%.300000c', 'b') || 'é😀x' || printf('%.300000c', 'b'), randomblob(0));
INSERT INTO t VALUES('  xxa😀bxx  ', x'fffe80c3a9');
SELECT length(substr(s, 299999, 6)), substr(s, 300001, 3), length(substr(s, -5)), length(substr(s, 2, -1)) FROM t WHERE rowid = 1;
SELECT length(trim(s, 'b')), trim(s, 'b'), length(ltrim(s, 'b')), length(rtrim(s, 'b')) FROM t WHERE rowid = 1;
SELECT quote(trim(s)), quote(ltrim(s, ' x')), quote(rtrim(s, ' x')), quote(trim(s, ' xab😀')) FROM t WHERE rowid = 2;
SELECT hex(substr(b, 2, 2)), hex(substr(b, -1)), hex(substr(CAST(b AS TEXT), 1, 2)), length(CAST(b AS TEXT)) FROM t WHERE rowid = 2;
SELECT substr('abc', 0, 2), substr('abc', -10, 12), substr('é😀x', 2), substr('é😀x', -2, 1), ltrim('', 'x'), rtrim('xxx', 'x'), trim('xax', '');
-- case: bug-hunt-9/engine/a-join-condition-is-true-by-sqlite-truthiness
CREATE TABLE x(c1, c2);
INSERT INTO x VALUES ('a', 2.5), ('b', '5'), ('c', 7), ('d', 'abc'), ('e', 0.5), ('f','0');
CREATE TABLE y(c1); INSERT INTO y VALUES ('a'),('b'),('c'),('d'),('e'),('f');
SELECT x.c1 FROM y JOIN x ON x.c1 = y.c1 WHERE x.c2 ORDER BY 1;
SELECT x.c1 FROM y, x WHERE x.c1 = y.c1 AND x.c2 ORDER BY 1;
CREATE TABLE t1(c1); INSERT INTO t1 VALUES (2.5), ('3'), (1);
CREATE TABLE t2(c2); INSERT INTO t2 VALUES (7);
SELECT a0.c1, a1.c2 FROM t1 a0 LEFT JOIN t2 a1 ON a0.c1 ORDER BY 1;
SELECT a1.c2 FROM (SELECT 1 AS c1) a0 LEFT JOIN t2 a1 ON 0.5;
SELECT 'single', group_concat(c1) FROM x WHERE c2;
SELECT 'join-on', group_concat(x.c1) FROM y JOIN x ON x.c1 = y.c1 AND x.c2;
SELECT 'cross', group_concat(x.c1) FROM y CROSS JOIN x WHERE x.c1 = y.c1 AND x.c2;
SELECT 'ywhere', group_concat(y.c1) FROM x JOIN y ON x.c1 = y.c1 WHERE x.c2;
SELECT 'expr', group_concat(x.c1) FROM y JOIN x ON x.c1 = y.c1 WHERE x.c2 + 0;
SELECT 'notnot', group_concat(x.c1) FROM y JOIN x ON x.c1 = y.c1 WHERE NOT NOT x.c2;
SELECT 'ne0', group_concat(x.c1) FROM y JOIN x ON x.c1 = y.c1 WHERE x.c2 <> 0;
