-- case: bug-hunt-11/files/D5-vacuum-table-name-non-ascii
create table "日本語" (a);
insert into "日本語" values (1);
vacuum;
select * from "日本語";
select name from sqlite_master order by name;
-- case: bug-hunt-11/files/D5-vacuum-table-name-accented
create table "café" (a);
insert into "café" values (1);
vacuum;
select * from "café";
-- case: bug-hunt-11/files/D5-vacuum-column-name-non-ascii
create table t (cölumn int);
insert into t values (1);
vacuum;
select * from t;
-- case: bug-hunt-11/files/D5-vacuum-index-and-view-names-non-ascii
create table "tâble" ("cölumn" int, other text);
insert into "tâble" values (1, 'x'), (2, 'y');
create index "indéx" on "tâble" ("cölumn");
create view "vüe" as select "cölumn" from "tâble";
vacuum;
select * from "vüe" order by 1;
select name from sqlite_master order by name;
pragma integrity_check;
-- case: bug-hunt-11/files/D5-vacuum-fts5-name-non-ascii
create virtual table "ftsé" using fts5(a);
insert into "ftsé" values ('x');
vacuum;
select * from "ftsé";
-- case: bug-hunt-11/files/D5-vacuum-into-table-name-non-ascii
create table "日本語" (a);
insert into "日本語" values (1);
vacuum into 'bug-hunt-11-files-vacuum-into.db';
select * from "日本語";
-- case: bug-hunt-11/files/D8-vacuum-keeps-sqlite-stat1
create table t (a, b);
insert into t values (1, 2), (2, 3), (3, 4);
create index ti on t (a);
analyze;
vacuum;
select * from sqlite_stat1;
-- case: bug-hunt-11/files/D8-vacuum-keeps-sqlite-stat1-after-reopen
create table t (a, b);
insert into t values (1, 2), (2, 3), (3, 4);
create index ti on t (a);
analyze;
-- reopen
vacuum;
-- reopen
select * from sqlite_stat1;
-- case: bug-hunt-11/files/D8-vacuum-keeps-hand-written-sqlite-stat1-rows
create table t (a, b);
create index ti on t (a);
analyze;
delete from sqlite_stat1;
insert into sqlite_stat1 values ('t', 'ti', '100 10');
insert into sqlite_stat1 values ('t', null, '100');
vacuum;
select tbl, idx, stat from sqlite_stat1 order by tbl, idx;
-- case: bug-hunt-11/files/D8-vacuum-without-analyze-makes-no-sqlite-stat1
create table t (a, b);
insert into t values (1, 2);
vacuum;
select name from sqlite_master order by name;
-- case: bug-hunt-11/files/D12-index-xinfo-integer-primary-key-desc
create table t (a integer primary key desc, b);
pragma index_xinfo('sqlite_autoindex_t_1');
select name from sqlite_master order by name;
-- case: bug-hunt-11/files/D12-view-count-column-decltype
create table t (a);
create view v as select count(*) as n from t;
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-view-values-column-decltype
create view v as select * from (values (1, 2));
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-view-recursive-column-decltype
create view v as with recursive c(x) as (select 1 union all select x + 1 from c where x < 3) select x from c;
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-table-xinfo-of-a-broken-view
create table t (a, b);
create view v as select a, b from t;
drop table t;
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-table-info-of-a-broken-view
create table t (a, b);
create view v as select a, b from t;
drop table t;
pragma table_info('v');
-- case: bug-hunt-11/files/D12-view-trailing-comment-kept-in-sqlite-master
create table t (a);
create view v as select a from t -- note
;
select sql from sqlite_master where name = 'v';
-- case: bug-hunt-11/files/D12-fts5-shadow-ddl-after-rename
create virtual table f using fts5(a);
alter table f rename to g;
select name, sql from sqlite_master where name <> 'g_idx' order by name;
-- case: bug-hunt-11/files/D12-view-derived-expression-column-decltype
create table t (a integer, b text, c);
create view v as select * from (select a, b, c, a + 1 as d, cast(a as text) as e from t);
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-view-cte-column-decltype
create view v as with c(x, y) as (values (1, 'z')) select x, y from c;
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-view-recursive-text-column-decltype
create view v as with recursive c(x) as (select 'a' union all select x || 'b' from c where length(x) < 3) select x from c;
pragma table_xinfo('v');
-- case: bug-hunt-11/files/D12-view-block-comment-before-semicolon
create table t (a);
create view v as select a from t /* note */;
select sql from sqlite_master where name = 'v';
-- case: bug-hunt-11/files/D12-view-comment-after-semicolon-is-not-stored
create table t (a);
create view v as select a from t; -- after
select sql from sqlite_master where name = 'v';
-- case: bug-hunt-11/files/D12-view-with-column-list-keeps-trailing-comment
create table t (a);
create view v (x) as select a from t -- c
;
select sql from sqlite_master where name = 'v';
-- case: bug-hunt-11/files/D12-index-trailing-comment-is-not-stored
create table t (a);
create index i on t(a) -- note
;
select sql from sqlite_master where name = 'i';
-- case: bug-hunt-11/files/D12-table-valued-table-xinfo-of-a-broken-view
create table t (a, b);
create view v as select a, b from t;
drop table t;
select * from pragma_table_xinfo('v');
-- case: bug-hunt-11/files/D12-table-valued-table-info-of-a-broken-view
create table t (a, b);
create view v as select a, b from t;
drop table t;
select * from pragma_table_info('v');
