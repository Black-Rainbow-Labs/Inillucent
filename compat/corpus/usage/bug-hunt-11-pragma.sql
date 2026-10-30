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
-- case: bug-hunt-11/pragma/D8-integrity-check-reports-each-row-and-each-table
create table t(a check(a>0), b, c as (a*2) check(c<100), constraint c2 check(b<10 or b is null));
create table u(x check(x>0));
pragma ignore_check_constraints=on;
insert into t(a,b) values(-1,1),(1,50),(-2,60),(5,null);
insert into u values(-5),(3),(-7);
pragma integrity_check;
pragma integrity_check(1);
pragma integrity_check(t);
pragma quick_check(u);
pragma ignore_check_constraints=off;
pragma integrity_check(2);
pragma quick_check;
-- case: bug-hunt-11/pragma/D8-integrity-check-ignores-checks-while-they-are-ignored
create table u(x check(x>0));
pragma ignore_check_constraints=on;
insert into u values(-5);
pragma integrity_check;
-- case: bug-hunt-11/pragma/D8-integrity-check-per-database
create table m(x check(x>0));
create temp table z(a check(a>1));
pragma ignore_check_constraints=on;
insert into m values(-1);
insert into z values(0),(1);
pragma ignore_check_constraints=off;
pragma main.integrity_check;
pragma temp.integrity_check;
pragma integrity_check;
pragma integrity_check(z);
-- case: bug-hunt-11/pragma/D9-ctas-strict-any-in-every-source
create table t(a any, b int, c text, d real, e blob, f any) strict;
insert into t values('1',2,'x',1.5,x'00',3);
select typeof(a), typeof(f) from t;
create table n as select * from t;
select sql from sqlite_master where name='n';
create view v as select * from t;
create table n2 as select * from v;
select sql from sqlite_master where name='n2';
create table n3 as select a as z, a+0 as y, cast(a as any) w, '1' as q from t;
select sql from sqlite_master where name='n3';
create table w(a any, b int);
create table n4 as select * from w;
select sql from sqlite_master where name='n4';
-- case: bug-hunt-11/pragma/D10-pragma-function-arg-lists-and-or
create table t(a,b,c);
select count(*) from pragma_table_info where arg='t';
select count(*) from pragma_table_info where arg in ('t');
select count(*) from pragma_table_info where arg in ('t','x');
select count(*) from pragma_table_info where arg='t' or arg='x';
select count(*) from pragma_table_info('t');
select count(*) from pragma_table_info where arg='x' or arg='t';
select count(*) from pragma_table_info where arg='t' and name='a';
select name from pragma_table_info where arg in ('t', 'x', 't');
create table u(p,q);
select arg, name from pragma_table_info where arg='u' or arg='t' order by arg, name;
-- case: bug-hunt-11/pragma/D11-optimize-debug-default-and-all-tables
create table t(a, b);
create index i on t(a);
create table n(x);
create table w(a primary key, b) without rowid;
insert into t select value, value from generate_series(1,2000);
insert into n select value from generate_series(1,50);
insert into w select value, 1 from generate_series(1,30);
select 'dbg'; pragma optimize(0x10003);
select 'default'; pragma optimize;
select count(*) from sqlite_master where name='sqlite_stat1';
select 'debug-default'; pragma optimize(1);
select 'q'; select count(*) from t where a=5;
select 'debug-after-use'; pragma optimize(3);
select 'run-after-use'; pragma optimize;
select * from sqlite_stat1 order by 1,2;
select 'debug-again'; pragma optimize(0x10003);
insert into t select value, value from generate_series(2001,60000);
-- case: bug-hunt-11/pragma/D11-optimize-reanalyzes-a-table-that-grew-25-times
create table t(a, b);
create index i on t(a);
insert into t select value, value from generate_series(1,100);
pragma optimize;
select * from sqlite_stat1;
insert into t select value, value from generate_series(101,2000);
pragma optimize(3);
pragma optimize;
select * from sqlite_stat1;
-- case: bug-hunt-11/pragma/D11-optimize-mask-without-the-analyze-bit-does-nothing
create table t(a, b);
create index i on t(a);
insert into t values(1,2);
pragma optimize(1);
pragma optimize(0);
pragma optimize(0x10000);
select count(*) from sqlite_master where name='sqlite_stat1';
-- case: bug-hunt-11/pragma/D11-optimize-under-query-only
create table t(a, b);
create index i on t(a);
insert into t values(1,2);
pragma query_only=1;
pragma optimize(3);
pragma optimize;
select count(*) from sqlite_master where name='sqlite_stat1';
-- case: bug-hunt-11/pragma/D2-drop_column_indexed_non_ascii
create table t (a, "é", y);
insert into t values (1, 2, 3);
create index i on t("é");
alter table t drop column "é";
select * from t;
insert into t (a, y) values (5, 6);
select * from t;
-- case: bug-hunt-11/pragma/D2-drop_column_check_non_ascii
create table t (a, "é", y, check ("é" > 0));
alter table t drop column "é";
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D2-drop_column_unique_non_ascii
create table t (a, "é", y, unique ("é"));
alter table t drop column "é";
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D2-drop_column_generated_non_ascii
create table t (a, "é", g as ("é" + 1));
alter table t drop column "é";
select sql from sqlite_master;
-- case: bug-hunt-11/pragma/D12-cache-size-belongs-to-each-database
attach ':memory:' as aux;
pragma aux.cache_size=10;
pragma aux.cache_size;
pragma temp.cache_size;
pragma temp.cache_size=7;
pragma temp.cache_size;
pragma main.cache_size=99;
pragma main.cache_size;
pragma aux.cache_size;
pragma temp.cache_size;
-- case: bug-hunt-11/pragma/D12-synchronous-belongs-to-each-database
attach ':memory:' as aux;
pragma synchronous;
pragma temp.synchronous;
pragma aux.synchronous;
pragma temp.synchronous=1;
pragma temp.synchronous;
pragma aux.synchronous=0;
pragma aux.synchronous;
pragma main.synchronous;
pragma synchronous=1;
pragma aux.synchronous;
pragma main.synchronous;
-- case: bug-hunt-11/pragma/D12-journal-mode-belongs-to-each-database
attach 'aux.db' as aux;
attach ':memory:' as mem;
pragma temp.journal_mode=wal;
pragma temp.journal_mode;
pragma temp.journal_mode=memory;
pragma temp.journal_mode;
pragma aux.journal_mode;
pragma aux.journal_mode=truncate;
pragma aux.journal_mode;
pragma main.journal_mode;
pragma mem.journal_mode;
pragma mem.journal_mode=delete;
pragma mem.journal_mode;
-- case: bug-hunt-11/pragma/D12-secure-delete-numbers-are-booleans
pragma secure_delete=fast;
pragma secure_delete;
pragma secure_delete=2;
pragma secure_delete;
pragma secure_delete=3;
pragma secure_delete;
pragma secure_delete=off;
pragma secure_delete;
-- case: bug-hunt-11/pragma/D13-without-rowid-desc-and-unique-keys
create table t(a integer primary key desc,b,c unique) without rowid;
create index si on t(b);
create table u(x, y, z, primary key(y desc, x), unique(z)) without rowid;
select name from sqlite_master where type='index' order by 1;
pragma index_list(t);
pragma index_info(sqlite_autoindex_t_1);
pragma index_info(sqlite_autoindex_t_2);
pragma index_xinfo(sqlite_autoindex_t_1);
pragma index_xinfo(sqlite_autoindex_t_2);
pragma index_xinfo(si);
pragma index_list(u);
pragma index_xinfo(sqlite_autoindex_u_2);
pragma index_xinfo(sqlite_autoindex_u_1);
-- case: bug-hunt-11/pragma/D13-without-rowid-integer-key-is-numbered-last
create table t(a integer primary key,b,c unique) without rowid;
select name from sqlite_master where type='index' order by 1;
pragma index_list(t);
pragma index_info(sqlite_autoindex_t_1);
pragma index_info(sqlite_autoindex_t_2);
pragma index_xinfo(sqlite_autoindex_t_1);
pragma index_xinfo(sqlite_autoindex_t_2);
-- case: bug-hunt-11/pragma/D14-ctas-cannot-select-from-itself
create table n3 as select * from n3;
select count(*) from sqlite_master;
-- case: bug-hunt-11/pragma/D14-ctas-of-schema-keeps-types-and-counts
create table t(a integer, b text);
create table n4 as select name, type, sql from sqlite_master;
select count(*), typeof(name) from n4;
select sql from sqlite_master where name='n4';
