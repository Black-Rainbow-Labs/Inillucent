-- Held out cases: written from other SQLite rewrites bug reports. A fixing
-- agent does not read this file; see section 5.1 of the task-2174 TDD.
-- case: holdout/date-and-time/86-date-arithmetic-with-huge-integer-arguments-return
SELECT date(2147483647, 2147483647, 2147483647);
SELECT date('2147483647-01-01');
SELECT datetime(1e18), datetime(9223372036854775807), datetime(-1e18);
-- case: holdout/date-and-time/384-date-functions-accept-a-blob-holding-the-text-of-a
SELECT date(CAST('2024-01-01' AS BLOB));
SELECT unixepoch(X'323032342D30312D3031');
SELECT date('2024-06-15', X'2B3120646179');
CREATE TABLE t(x TEXT CHECK(date(x) IS NOT NULL));
INSERT INTO t VALUES(X'323032342D30312D3031');
SELECT count(*), typeof(x) FROM t;
SELECT strftime(X'2559','2024-06-15'), strftime('%Y', X'323032342D30362D3135'), time(X'31323A3030'), julianday(X'323032342D30312D3031');
-- case: holdout/date-and-time/385-datetime-x-utc-near-a-daylight-saving-change-needs
SELECT datetime('2024-06-15 03:00:00','utc') IS NOT NULL;
SELECT datetime('2024-03-10 03:00:00','utc','localtime') = '2024-03-10 03:00:00';
SELECT datetime('2024-01-01 12:00:00','localtime','utc');
SELECT datetime('now','utc') IS NOT NULL, datetime('2024-01-01','utc','utc');
-- case: holdout/date-and-time/389-unixepoch-of-a-fractional-time-before-1970-rounds-
CREATE TABLE t(id INTEGER PRIMARY KEY, ts TEXT);
INSERT INTO t VALUES(1,'1964-03-28T03:36:14.230Z'),(2,'1969-12-31T23:59:59.900Z'),(3,'1970-01-01T00:00:00.100Z');
SELECT group_concat(id) FROM (SELECT id FROM t WHERE unixepoch(ts) < 0 ORDER BY id);
SELECT unixepoch('1969-12-31 23:59:59.999');
SELECT date(unixepoch('1969-12-31T23:59:59.500Z'),'unixepoch');
SELECT unixepoch('1969-12-31 23:59:59'), unixepoch('1970-01-01 00:00:00.999'), unixepoch('1969-12-31 23:59:59.999', 'subsec'), unixepoch('1964-03-28T03:36:14.230Z');
-- case: holdout/date-and-time/615-hour-24-in-a-time-string
select date('2024-07-21 24:00:00');
select datetime('2024-07-21 24:00:00'), datetime('2024-07-21 24:00:01'), datetime('2024-07-21 25:00:00'), time('24:00:00'), time('23:59:60'), time('23:60:00'), datetime('2024-07-21 23:59:59.9995');
-- case: holdout/date-and-time/616-now-is-evaluated-once-per-statement
select count(distinct v) from (select datetime('subsecond') v from generate_series(1, 100));
select count(distinct v) from (select datetime('now') v from generate_series(1, 100));
select count(distinct v) from (select julianday('now') v from generate_series(1, 100));
select count(distinct v) from (select strftime('%f', 'now') v from generate_series(1, 100));
select count(distinct v) from (select current_timestamp v from generate_series(1, 100));
-- case: holdout/date-and-time/640-strftime-of-a-number-that-is-out-of-range-returns-
SELECT strftime('%Y', 99999999999999);
SELECT quote(strftime('%Y', 99999999999999)), quote(date(99999999999999)), quote(strftime('%Y', 1e15)), quote(datetime(5373484.5)), quote(datetime(5373485)), quote(strftime('%Y-%m-%d', 0)), quote(strftime('%Y', -1)), quote(date(-1)), quote(date('0000-01-01')), quote(date('9999-12-31')), quote(date('10000-01-01'));
-- case: holdout/date-and-time/715-timediff-and-datetime-b-timediff-a-b
SELECT timediff('2000-01-31', '2000-03-02');
SELECT datetime('2000-03-02', timediff('2000-01-31', '2000-03-02'));
SELECT timediff('2000-03-02', '2000-01-31'), timediff('2024-03-31', '2024-02-29'), timediff('2024-02-29', '2024-03-31'), timediff('2000-01-01 00:00:00.5', '2000-01-01'), timediff('2000-01-01', '2001-02-03 04:05:06');
SELECT datetime('2000-01-31', timediff('2000-03-02', '2000-01-31'));
SELECT datetime('2024-03-31', timediff('2024-02-29', '2024-03-31')), datetime('2024-02-29', timediff('2024-03-31', '2024-02-29'));
-- case: holdout/date-and-time/745-julianday-accepts-rfc-3339-timestamps-with-z-and-o
SELECT julianday('2026-04-07 16:00:00'), julianday('2026-04-07T16:00:00'), julianday('2026-04-07T16:00:00Z'), julianday('2026-04-07T16:00:00+00:00'), julianday('2026-04-07T16:00:00+01:00'), julianday('2026-04-07T16:00:00-05:30');
SELECT datetime('2026-04-07T16:00:00+01:00'), datetime('2026-04-07 16:00:00z'), datetime('2026-04-07T16:00:00.123Z'), datetime('2026-04-07T16:00Z'), datetime('2026-04-07T16:00:00+0100'), datetime('2026-04-07T16:00:00 +01:00'), datetime('2026-04-07T16:00:00+25:00'), datetime('2026-04-07T16:00:00+01:60');
-- case: holdout/date-and-time/777-now-stays-the-same-within-a-statement
SELECT count(DISTINCT datetime('now', 'subsec')) FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) SELECT i FROM n);
SELECT count(DISTINCT julianday('now')) FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) SELECT i FROM n);
SELECT count(DISTINCT strftime('%s%f', 'now')) FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) SELECT i FROM n);
SELECT count(DISTINCT current_time || current_date) FROM (WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 2000) SELECT i FROM n);
-- case: holdout/date-and-time/838-a-datetime-column-has-numeric-affinity-and-keeps-a
create table sample (start_time datetime);
insert into sample values('2024-01-31-05:00');
insert into sample values('2024-01-31');
insert into sample values('20240131');
insert into sample values('2024');
insert into sample values('1e3');
select start_time, typeof(start_time) from sample;
