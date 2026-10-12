-- case: bug-hunt-9/shell/tables-schema-and-indexes-list-temp-and-attached
attach ':memory:' as aux;
create table aux.at(a);
create table mt(b);
create temp table tt(c);
create index mi on mt(b);
create index aux.ai on at(a);
.headers off
.tables
.schema
.indexes
-- case: bug-hunt-9/shell/tables-pattern-matches-the-unqualified-name
attach ':memory:' as aux;
create table aux.at(a);
create table mt(b);
create temp table tt(c);
.headers off
.tables %t
.tables aux.%
-- case: bug-hunt-9/shell/mode-insert-quotes-a-table-name
.mode insert 'x y'
select 1;
.mode insert 'a"b'
select 2;
-- case: bug-hunt-9/shell/mode-ascii
.mode ascii
select 1,2;
select 3,4;
-- case: bug-hunt-9/shell/mode-qbox
.mode qbox
select 1,'two',x'03',null;
-- case: bug-hunt-9/shell/mode-tcl
.mode tcl
.headers on
select 1,'two "q"',x'4142',null;
-- case: bug-hunt-9/shell/mode-box-wraps-at-a-width
.mode box --wrap 10
select 'one two three four five', 12345678901234;
-- case: bug-hunt-9/shell/mode-options-are-checked
.mode box --bogus
.mode nosuchmode
.mode box --wrap
.mode box --align xyz
.mode box --escape bogus
-- case: bug-hunt-9/shell/mode-with-no-arguments-prints-the-command
.mode box --null "Z"
.mode
.mode csv
.mode
-- case: bug-hunt-9/shell/separator-sets-the-row-separator-in-list-mode
.mode list
.separator abc defg
select 1,2;
select 3,4;
-- case: bug-hunt-9/shell/csv-honours-a-column-separator
.mode csv
.separator ||
select 1,2;
-- case: bug-hunt-9/shell/double-quoted-arguments-resolve-backslashes
.print "a\"b"
.print "x\x41y"
.print a\tb
.print 'a\tb'
-- case: bug-hunt-9/shell/negative-width-aligns-right
.mode column
.width 3 -4
select 123456 as abcdefgh, 7 as b, 8 as c;
-- case: bug-hunt-9/shell/nullvalue-is-reset-by-a-mode
.nullvalue Z
.mode csv
select null;
.mode list
select null;
-- case: bug-hunt-9/shell/boolean-commands-check-their-arguments
.headers
.headers maybe
.bail
.echo x
.timer
.eqp bogus
.read
.read a b
-- case: bug-hunt-9/shell/exit-and-quit-inside-read
.once r.sql
select '.exit 0';
.print before
.read r.sql
.print after
-- case: bug-hunt-9/shell/once-covers-one-command
.once o1.txt
.print p
select 7;
-- case: bug-hunt-9/shell/output-stderr-and-off
.output stderr
select 10;
.output off
select 11;
.output stdout
select 12;
-- case: bug-hunt-9/shell/backup-of-a-memory-database
create table t(a);
insert into t values(1);
.backup bk9.db
.open bk9.db
select * from t;
-- case: bug-hunt-9/shell/backup-of-an-attached-database
attach 'bk9a.db' as aux;
create table aux.z(a);
insert into aux.z values(9);
create table m(a);
.backup aux bk9b.db
.open bk9b.db
.headers off
.tables
-- case: bug-hunt-9/shell/clone-refuses-an-existing-file
create table t(a);
.clone cl9.db
.clone cl9.db
-- case: bug-hunt-9/shell/open-options-are-not-file-names
.open --new o9.db
.open --readonly nonexistent9.db
.open --bogus x
.open a b
.open file:o9b.db?mode=rwc
create table u(a);
.open --readonly o9b.db
select count(*) from u;
-- case: bug-hunt-9/shell/open-readonly-refuses-writes
.open o9c.db
create table t(a);
.open --readonly o9c.db
insert into t values(1);
-- case: bug-hunt-9/shell/import-reports-a-stray-quote
.once q9.csv
select 'a,b' || char(10) || '"x"y,1' || char(10) || '2,3' || char(10);
.mode csv
.import q9.csv t9
.mode quote
select * from t9;
-- case: bug-hunt-9/shell/limit-compound-select-is-enforced
.limit compound_select 2
select 1 union select 2 union select 3;
.limit compound_select
-- case: bug-hunt-9/shell/dot-command-needs-column-zero
 .print x
-- case: bug-hunt-9/shell/read-nesting-limit
.once rec9.sql
select '.read rec9.sql';
.read rec9.sql
