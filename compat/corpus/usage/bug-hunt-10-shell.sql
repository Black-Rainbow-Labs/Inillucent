-- case: bug-hunt-10/shell/header-singular-and-prefixes
.header off
select 1 as x;
.head on
select 2 as y;
.h off
select 3 as z;
.hea
-- case: bug-hunt-10/shell/mode-quote-before-the-mode-name-csv
.mode --quote csv
select 'a,b', 'c', 1;
-- case: bug-hunt-10/shell/mode-quote-before-the-mode-name-list
.mode --quote list
select 'a,b', 'c', 1;
-- case: bug-hunt-10/shell/mode-quote-after-the-mode-name
.mode csv --quote
select 'a,b', 'c', 1;
.mode list --quote off
select 'a,b', 'c', 1;
-- case: bug-hunt-10/shell/mode-quote-off-before-the-mode-name
.mode --quote off csv
select 'a,b', 'c', 1;
-- case: bug-hunt-10/shell/trace-writes-each-statement-to-stdout
.trace stdout
select 1;
create table t(a);
insert into t values(1);
.trace off
select 2;
-- case: bug-hunt-10/shell/trace-refuses-an-unknown-option
.trace --bogus stdout
select 1;
-- case: bug-hunt-10/shell/open-readonly-of-a-missing-file-carries-on
.open --readonly missing-file.db
select 1;
-- case: bug-hunt-10/shell/archive-with-no-arguments-prints-the-usage
.archive
-- case: bug-hunt-10/shell/connection-names-the-database-as-it-was-opened
.open :memory:
.connection
.open
.connection
.open ""
.connection
.connection 1
.connection
