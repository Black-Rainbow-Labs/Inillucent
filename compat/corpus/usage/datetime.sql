-- case: datetime/fixed-inputs
SELECT date('2024-02-29'), date('2023-02-29'), datetime('2024-01-31 10:20:30'), time('10:20'), julianday('2000-01-01'), unixepoch('1970-01-02'), strftime('%Y-%m-%d %H:%M:%f', '2024-05-06 07:08:09.123');
SELECT datetime(0, 'unixepoch'), datetime(1700000000, 'unixepoch'), datetime(1700000000.5, 'unixepoch'), datetime('1700000000', 'unixepoch'), datetime(2460000.5), datetime(2460000.5, 'julianday'), date(1700000000, 'auto');
SELECT date('2024-01-31', '+1 month'), date('2024-01-31', '+1 month', 'floor'), date('2024-03-31', '-1 month'), date('2024-02-29', '+1 year'), date('2024-05-15', 'start of month'), date('2024-05-15', 'start of year'), date('2024-05-15', 'weekday 0'), date('2024-05-15', '+3 days', 'weekday 1');
SELECT datetime('2024-05-15 10:00', '+90 minutes'), datetime('2024-05-15 10:00', '-1 hours'), datetime('2024-05-15 10:00', '+1.5 days'), datetime('2024-05-15', '+1 day', 'start of day'), datetime('2024-05-15 23:59:59', '+1 seconds'), datetime('2024-05-15', '+0001-02-03 04:05:06');
-- case: datetime/strftime-specifiers
SELECT strftime('%s', '2024-01-01'), strftime('%j %W %w %u %U %V %G %g', '2024-01-01'), strftime('%H %I %p %M %S %f %e %k %l', '2024-01-01 15:04:05.678'), strftime('%a %A %b %B %C %D %F %R %T %y %%', '2024-03-09'), strftime('%J', '2024-01-01');
SELECT strftime('%Y', 'nonsense'), strftime('%Q', '2024-01-01'), strftime(NULL, '2024-01-01'), strftime('%Y', NULL), strftime('%Y');
-- case: datetime/invalid-and-edge-inputs
SELECT date('2024-13-01'), date('2024-00-10'), date('2024-02-30'), date('24-01-01'), date('2024-1-1'), datetime('2024-01-01T10:20:30'), datetime('2024-01-01T10:20:30Z'), datetime('2024-01-01 10:20:30+02:00'), datetime('2024-01-01 10:20:30.123456789'), date(''), date(NULL), date('now', 'nonsense');
SELECT time('25:00'), time('12:60'), time('12:30:61'), datetime('0000-01-01'), datetime('9999-12-31 23:59:59', '+1 second'), date('-0001-01-01'), julianday('-4713-11-24 12:00:00'), date(-1), date(1e20);
-- case: datetime/now-shape
SELECT length(date('now')), length(datetime('now')), length(CURRENT_TIMESTAMP), length(CURRENT_DATE), length(CURRENT_TIME), typeof(julianday('now')), typeof(unixepoch()), unixepoch() > 1700000000, date('now') = CURRENT_DATE, datetime('now', 'localtime') IS NOT NULL, datetime('now', 'utc') IS NOT NULL;
SELECT typeof(unixepoch('now', 'subsec')), unixepoch('2024-01-01 00:00:00.5', 'subsec'), datetime('2024-01-01 00:00:00.5', 'subsec'), timediff('2024-03-01', '2024-01-01'), timediff('2024-01-01', '2024-03-01 12:00');
-- case: datetime/stored-timestamps
CREATE TABLE ev(id INTEGER PRIMARY KEY, at TEXT DEFAULT CURRENT_TIMESTAMP, ts INTEGER);
INSERT INTO ev(at, ts) VALUES('2024-01-01 00:00:00', 1704067200), ('2024-06-01 12:00:00', 1717243200), ('2023-12-31 23:59:59', 1704067199);
SELECT id FROM ev WHERE at > datetime('2024-01-01') ORDER BY at;
SELECT id FROM ev WHERE date(at) = '2024-01-01';
SELECT id, datetime(ts, 'unixepoch') = at FROM ev ORDER BY id;
SELECT strftime('%Y-%m', at) AS m, count(*) FROM ev GROUP BY m ORDER BY m;
SELECT id FROM ev WHERE at BETWEEN '2024-01-01' AND '2024-12-31' ORDER BY id;
SELECT CAST(julianday('2024-06-01') - julianday(at) AS INTEGER) FROM ev ORDER BY id;
-- case: datetime/r3b-subsec-keyword-as-the-time-value
SELECT length(datetime('subsec')), length(time('subsecond')), length(datetime('subsec', '+1 day')), length(strftime('%Y-%m-%d %H:%M:%f', 'subsec'));
-- case: datetime/r3b-timediff-month-end-in-both-directions
SELECT timediff('2024-01-31', '2024-03-01'), timediff('2024-03-01', '2024-01-31');
SELECT timediff('2023-01-31', '2023-03-01'), timediff('2023-03-01', '2023-01-31');
SELECT timediff('2024-12-31 23:00:00', '2025-01-31 01:30:00'), timediff('2025-01-31 01:30:00', '2024-12-31 23:00:00');
SELECT datetime('2024-03-01', timediff('2024-01-31', '2024-03-01')), datetime('2024-01-31', timediff('2024-03-01', '2024-01-31'));
SELECT timediff('2024-05-15', '2024-05-15'), timediff('2020-02-29', '2024-02-29');
-- case: datetime/r3b-blob-modifier-and-format
SELECT date('2024-06-15', X'2B3120646179', X'2B3120796561722B'), strftime(X'25592D256D2D2564', '2024-06-15');
SELECT datetime(X'323032342D30362D3135', X'2B3120686F7572');
