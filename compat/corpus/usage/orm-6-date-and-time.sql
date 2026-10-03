-- case: orm/6/6.1-datetime-now-format-and-types
SELECT length(datetime('now')), length(date('now')), length(time('now')), typeof(datetime('now')), datetime('now') GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9] [0-9][0-9]:[0-9][0-9]:[0-9][0-9]';
-- case: orm/6/6.2-current-timestamp-default-stores-utc-text-yyyy-mm-dd-hh-mm-s
CREATE TABLE t(id INTEGER PRIMARY KEY, created_at DATETIME DEFAULT CURRENT_TIMESTAMP); INSERT INTO t DEFAULT VALUES; SELECT length(created_at), typeof(created_at), abs(unixepoch(created_at) - unixepoch('now')) < 5 FROM t;
-- case: orm/6/6.3-strftime-s-returns-text-cast-to-integer
SELECT strftime('%s','2024-01-01 00:00:00'), typeof(strftime('%s','2024-01-01')), CAST(strftime('%s','2024-01-01') AS INTEGER), unixepoch('2024-01-01'), typeof(unixepoch('2024-01-01'));
-- case: orm/6/6.4-strftime-formats-y-m-d-h-m-f-j-w-w-a-only-listed
SELECT strftime('%Y-%m-%d %H:%M:%S','2024-03-05 14:07:09'), strftime('%f','2024-03-05 14:07:09.123'), strftime('%j','2024-03-05'), strftime('%w','2024-03-05'), strftime('%W','2024-03-05'), strftime('%J','2024-03-05'), strftime('%%','2024-03-05'), strftime('%Q','2024-03-05');
-- case: orm/6/6.5-strftime-new-specifiers-3-44-e-f-i-k-l-p-p-r-t-u
SELECT strftime('%e|%F|%I|%k|%l|%p|%P|%R|%T|%u','2024-03-05 04:07:09');
-- case: orm/6/6.6-strftime-iso-week-g-g-v-u-3-46
SELECT strftime('%G %g %V %U','2021-01-03');
-- case: orm/6/6.7-invalid-dates-return-null-never-errors
SELECT date('2024-02-30'), date('2024-13-01'), date('garbage'), datetime(''), date(NULL), date('2024-02-29'), date('2023-02-29'), date('2024-2-3');
-- case: orm/6/6.8-date-accepted-input-formats
SELECT date('2024-01-02T03:04:05'), datetime('2024-01-02T03:04:05Z'), datetime('2024-01-02 03:04:05+02:00'), datetime('2024-01-02 03:04'), time('03:04'), datetime('20240102'), datetime(2460311.5), datetime(0,'unixepoch'), datetime(1700000000,'unixepoch'), datetime('now','start of day') IS NOT NULL;
-- case: orm/6/6.9-numeric-argument-without-modifier-is-julian-day
SELECT datetime(2460311.5), datetime(1700000000), datetime(1700000000,'unixepoch'), datetime(1700000000000/1000,'unixepoch'), datetime(1700000000000,'unixepoch');
-- case: orm/6/6.10-unixepoch-auto-modifier-distinguishes-ms-and-s-3-38
SELECT datetime(1700000000,'auto'), datetime(2460311.5,'auto'), unixepoch('2024-01-01 12:00:00.9'), unixepoch('2024-01-01 12:00:00.9','subsec');
-- case: orm/6/6.11-modifiers-n-days-start-of-month-weekday-localtime-absent
SELECT date('2024-01-31','+1 month'), date('2024-03-31','-1 month'), date('2024-05-15','start of month','+1 month','-1 day'), date('2024-05-15','weekday 0'), date('2024-05-15','start of year'), datetime('2024-05-15 10:00','+90 minutes'), date('2024-05-15','+1 year');
-- case: orm/6/6.12-modifier-floor-ceiling-3-46-overflow-days
SELECT date('2024-01-31','+1 month'), date('2024-01-31','+1 month','floor'), date('2024-01-31','+1 month','ceiling');
-- case: orm/6/6.13-julianday-arithmetic-for-age-diff-in-days
SELECT julianday('2024-03-01') - julianday('2024-02-01'), CAST(julianday('2024-03-01') - julianday('2024-02-01') AS INTEGER), julianday('2000-01-01 12:00:00'), typeof(julianday('now'));
-- case: orm/6/6.14-timediff-3-43
SELECT timediff('2024-03-01 12:00:00','2024-02-01 00:00:00');
-- case: orm/6/6.15-fractional-seconds-preserved-by-datetime-subsec
SELECT datetime('2024-01-01 00:00:00.789'), strftime('%Y-%m-%d %H:%M:%f','2024-01-01 00:00:00.789'), strftime('%S','2024-01-01 00:00:00.789'), datetime('2024-01-01 00:00:00.789','subsec');
-- case: orm/6/6.16-dates-as-text-compare-lexicographically-between-on-iso-strin
CREATE TABLE e(ts TEXT); INSERT INTO e VALUES('2024-01-01 10:00:00'),('2024-01-02 00:00:00'),('2024-01-02'); SELECT ts FROM e WHERE ts BETWEEN '2024-01-01' AND '2024-01-02' ORDER BY ts; SELECT ts FROM e WHERE date(ts)='2024-01-02' ORDER BY ts;
-- case: orm/6/6.17-time-zone-suffix-z-and-offsets-shift-to-utc
SELECT datetime('2024-06-01 12:00:00+05:30'), datetime('2024-06-01T12:00:00-08:00'), datetime('2024-06-01 12:00:00 UTC')
-- case: orm/6/6.18-date-now-localtime-shape-utc-modifier
SELECT length(date('now','localtime')), length(datetime('now','utc'));
-- case: orm/6/6.19-year-0000-and-9999-range-limits
SELECT date('0000-01-01'), date('9999-12-31'), date('10000-01-01'), date('-4713-11-24'), datetime(0), datetime(5373484.5), datetime(5373485.5);
-- case: orm/6/6.20-django-datefield-round-trip-text-via-date
CREATE TABLE t(d date NOT NULL, dt datetime NOT NULL, tm time); INSERT INTO t VALUES('2024-01-02','2024-01-02 03:04:05.123456','03:04:05'); SELECT d, dt, tm, typeof(d), date(dt), strftime('%Y', dt) FROM t;
-- case: orm/6/6.21-python-sqlite3-parse-decltypes-column-decltype-retained-as-w
CREATE TABLE t(d DATE, ts TIMESTAMP, "x y" [my type], b BOOLEAN); SELECT name, type FROM pragma_table_info('t');
-- case: orm/6/6.22-datetime-null-and-bad-modifier-return-null
SELECT datetime(NULL), datetime('2024-01-01','banana'), datetime('2024-01-01','+1 fortnight'), date('2024-01-01','+x days');
