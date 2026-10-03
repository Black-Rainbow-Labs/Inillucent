-- The corpus preamble sets .headers on, so these cases cover an explicit
-- header choice. The default per mode is covered by differential::cli.
-- case: shell/headers-empty-list
.mode list
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-csv
.mode csv
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-quote
.mode quote
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-line
.mode line
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-insert
.mode insert
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-json
.mode json
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-column
.mode column
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-markdown
.mode markdown
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-table
.mode table
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-box
.mode box
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-html
.mode html
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/headers-empty-tabs
.mode tabs
CREATE TABLE t(a);
SELECT a FROM t;
INSERT INTO t VALUES(1);
SELECT a FROM t WHERE a>5;
SELECT a, a+1 AS b FROM t;
-- case: shell/insert-names
.mode insert x
SELECT 1 AS "a b", 2 AS "select", 3 AS c_1, 4 AS "q""x", 5 AS _u, 6 AS "9z", 7 AS "key";
-- case: shell/headers-choice-survives-mode-changes
.headers off
.mode box
SELECT 1 AS a;
.mode list
SELECT 2 AS a;
.headers on
.mode list
SELECT 3 AS a;
.mode insert
SELECT 4 AS a;
.mode box
SELECT 5 AS a;
-- case: shell/headers-off-in-drawn-modes
.headers off
.mode box
SELECT 1 AS longname, 22 AS b;
.mode table
SELECT 1 AS longname;
.mode markdown
SELECT 1 AS longname;
.mode column
SELECT 1 AS longname, 333 AS c;
.mode html
SELECT 1 AS longname;
