-- case: bug-hunt-11/pragma/D1-query-only-fts5-delete-update
create virtual table f using fts5(a);
insert into f values('hello'),('world');
pragma query_only=1;
delete from f where rowid=1;
update f set a='zzz' where rowid=2;
select a from f;
-- reopen
select count(*) from f;
-- case: bug-hunt-11/pragma/D1-query-only-rtree-delete
create virtual table r using rtree(id,x0,x1);
insert into r values(1,0,1),(2,2,3);
pragma query_only=1;
delete from r;
select count(*) from r;
-- case: bug-hunt-11/pragma/D1-query-only-fts4-delete
create virtual table g using fts4(a);
insert into g values('a'),('b');
pragma query_only=1;
delete from g;
select count(*) from g;
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-index
create table t(d, e);
insert into t values(1,2);
create index i on t("e");
alter table t drop column e;
select sql from sqlite_master order by name;
insert into t values(5);
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-unique
create table t(d, e, unique(d, "e"));
insert into t values(1,2);
alter table t drop column e;
insert into t values(1);
insert into t values(1);
select * from t;
pragma integrity_check;
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-check
create table t(d, e, check("e" > 0));
alter table t drop column e;
insert into t values(1);
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-partial-index
create table t(d, e);
create index i on t(d) where "e" > 0;
alter table t drop column e;
select name from sqlite_master;
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-generated
create table t(d, e, f as ("e" + 1));
alter table t drop column e;
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D3-query-only-vacuum-analyze
create table t(a);
create index i on t(a);
insert into t values(1);
pragma query_only=1;
vacuum;
analyze;
select count(*) from sqlite_master where name='sqlite_stat1';
-- case: bug-hunt-11/pragma/D3-query-only-reindex
create table t(a);
create index i on t(a);
insert into t values(1);
pragma query_only=1;
reindex;
select count(*) from t;
-- case: bug-hunt-11/pragma/D3-query-only-vacuum-into
create table t(a);
insert into t values(1);
pragma query_only=1;
vacuum into 'copy.db';
select count(*) from t;
-- case: bug-hunt-11/pragma/D3-query-only-alter-and-create
create table t(a);
pragma query_only=1;
alter table t add column b;
create temp table x(a);
create index i on t(a);
select count(*) from sqlite_master;
-- case: bug-hunt-11/pragma/D3-query-only-user-version
pragma query_only=1;
pragma user_version=5;
pragma application_id=6;
pragma schema_version=7;
-- reopen
pragma user_version;
pragma application_id;
-- case: bug-hunt-11/pragma/D3-query-only-optimize-incremental-vacuum
pragma auto_vacuum=2;
create table t(a);
insert into t select randomblob(1000) from generate_series(1,200);
delete from t;
pragma query_only=1;
pragma incremental_vacuum;
pragma optimize;
-- case: bug-hunt-11/pragma/D5-schema-version-attached
attach ':memory:' as aux;
pragma aux.schema_version;
create table aux.t(a);
pragma aux.schema_version;
pragma schema_version;
-- case: bug-hunt-11/pragma/D6-trigger-on-temp-table
create temp table x(a);
create trigger tr after insert on x begin select 1; end;
select name from sqlite_temp_master where type='trigger';
-- case: bug-hunt-11/pragma/D8-integrity-check-check-constraint
create table t(a check(a>0));
insert into t values(1);
pragma ignore_check_constraints=on;
insert into t values(-1);
pragma ignore_check_constraints=off;
pragma integrity_check;
pragma quick_check;
-- case: bug-hunt-11/pragma/D9-ctas-strict-any
create table t(a any, b int) strict;
create table n as select * from t;
select sql from sqlite_master where name='n';
-- case: bug-hunt-11/pragma/D10-pragma-tvf-or-on-arg
create table t(a,b,c);
select count(*) from pragma_table_info where arg='t' or arg='x';
select count(*) from pragma_table_info where arg in ('t');
-- case: bug-hunt-11/pragma/D11-pragma-optimize-writes-no-stat1
create table t(a, b);
create index i on t(a);
insert into t select value, value from generate_series(1,2000);
pragma optimize(0x10002);
select count(*) from sqlite_master where name='sqlite_stat1';
-- case: bug-hunt-11/pragma/D12-temp-and-attached-scoping
attach ':memory:' as aux;
pragma aux.cache_size=10;
pragma cache_size;
pragma temp.synchronous;
pragma temp.journal_mode;
pragma temp.secure_delete;
-- case: bug-hunt-11/pragma/D13-without-rowid-autoindex-names
create table t(a integer primary key, b, c unique) without rowid;
select name from sqlite_master where type='index';
pragma index_list(t);
pragma index_info(t);
-- case: bug-hunt-11/pragma/D14-ctas-reads-its-own-table-from-master
create table t(a);
create table n2 as select m.name, p.name as col from sqlite_master m, pragma_table_info(m.name) p where m.type='table';
select * from n2 order by 1,2;
-- case: bug-hunt-11/pragma/D15-query-only-if-not-exists-noops
create table t(a);
pragma query_only=1;
create table if not exists t(a);
drop table if exists nosuch;
drop index if exists nosuch;
-- case: bug-hunt-11/pragma/D16-defer-foreign-keys-not-reset-by-pragma-write
pragma defer_foreign_keys=on;
pragma user_version=1;
pragma defer_foreign_keys;
-- case: bug-hunt-11/pragma/D17-count-changes-ignored
pragma count_changes=on;
create table t(a);
insert into t values(1),(2);
-- case: bug-hunt-11/pragma/D17-column-names-pragmas-ignored
create table t(a, b);
insert into t values(1,2);
pragma full_column_names=on;
select a from t;
-- case: bug-hunt-11/pragma/D18-index-xinfo-collation-spelling
create table t(a text collate nocase);
create index i on t(a);
pragma index_xinfo(i);
-- case: bug-hunt-11/pragma/D19-table-list-sqlite-master-argument
pragma table_list(sqlite_master);
-- case: bug-hunt-11/pragma/D20-attached-memory-backup
attach ':memory:' as aux;
create table aux.x(a);
.backup aux bk3.db
.open bk3.db
.tables
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-expression-index
create table t(d, e);
create index i on t(d, "e" + 1);
alter table t drop column e;
select sql from sqlite_master order by name;
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-default-stays-a-string
create table t(a, b, c default "q");
alter table t drop column b;
insert into t(a) values(1);
select * from t;
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D2-drop-column-quoted-primary-key-list
create table t(d, e, primary key("d", "e"));
alter table t drop column e;
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D6-trigger-on-temp-table-fires
create temp table x(a);
create temp table log(m);
create trigger tr after insert on x begin insert into log values('fired ' || new.a); end;
insert into x values(1),(2);
select * from log order by 1;
select name, tbl_name from sqlite_temp_master where type='trigger';
select count(*) from sqlite_master where type='trigger';
-- case: bug-hunt-11/pragma/D6-trigger-on-table-in-both-main-and-temp
create table x(a);
create temp table x(a);
create table log(m);
create trigger tr after insert on x begin insert into log values('t'); end;
select count(*) from sqlite_temp_master where type='trigger';
select count(*) from sqlite_master where type='trigger';
-- case: bug-hunt-11/pragma/D6-trigger-on-main-table-stays-in-main
create table x(a);
create trigger tr after insert on x begin select 1; end;
select count(*) from sqlite_master where type='trigger';
select count(*) from sqlite_temp_master where type='trigger';
