-- Every pair of join kinds with the inner pair in parentheses on either side, three ON forms and four WHERE
-- forms, over small tables with NULLs and repeated keys. Written by tools/bug-hunt/nested-joins.mjs
-- during the bug hunt of October 2026; see tasks/task-2201-bug-hunt-tdd.md. 2.2.0 and 2.3.1 answer 18 of them wrongly.
-- Run every night by nightly::research_corpus against the pinned SQLite.

-- case: research/nested-joins/1
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k;

-- case: research/nested-joins/2
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/3
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/4
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/5
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/6
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/7
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/8
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/9
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k;

-- case: research/nested-joins/10
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/11
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/12
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/13
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/14
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/15
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/16
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/17
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/18
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/19
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/20
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/21
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/22
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/23
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/24
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/25
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k;

-- case: research/nested-joins/26
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/27
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/28
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/29
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/30
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/31
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/32
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/33
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k;

-- case: research/nested-joins/34
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/35
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/36
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/37
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/38
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/39
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/40
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/41
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/42
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/43
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/44
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/45
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/46
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/47
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/48
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/49
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k;

-- case: research/nested-joins/50
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/51
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/52
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/53
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/54
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/55
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/56
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/57
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k;

-- case: research/nested-joins/58
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/59
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/60
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/61
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/62
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/63
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/64
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/65
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/66
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/67
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/68
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/69
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/70
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/71
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/72
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/73
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k;

-- case: research/nested-joins/74
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/75
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/76
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/77
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/78
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/79
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/80
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/81
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k;

-- case: research/nested-joins/82
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/83
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/84
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/85
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/86
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/87
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/88
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/89
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/90
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/91
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/92
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/93
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/94
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/95
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/96
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/97
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a JOIN b ON a.k = b.k) CROSS JOIN c ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a JOIN b ON a.k = b.k) CROSS JOIN c;

-- case: research/nested-joins/98
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c CROSS JOIN (a JOIN b ON a.k = b.k) ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c CROSS JOIN (a JOIN b ON a.k = b.k);

-- case: research/nested-joins/99
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k;

-- case: research/nested-joins/100
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/101
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/102
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/103
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/104
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/105
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/106
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/107
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k;

-- case: research/nested-joins/108
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/109
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/110
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/111
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/112
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/113
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/114
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/115
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/116
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/117
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/118
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/119
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/120
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/121
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/122
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/123
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k;

-- case: research/nested-joins/124
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/125
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/126
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/127
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/128
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/129
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/130
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/131
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k;

-- case: research/nested-joins/132
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/133
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/134
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/135
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/136
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/137
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/138
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/139
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/140
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/141
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/142
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/143
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/144
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/145
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/146
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/147
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k;

-- case: research/nested-joins/148
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/149
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/150
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/151
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/152
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/153
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/154
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/155
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k;

-- case: research/nested-joins/156
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/157
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/158
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/159
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/160
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/161
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/162
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/163
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/164
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/165
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/166
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/167
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/168
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/169
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/170
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/171
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k;

-- case: research/nested-joins/172
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/173
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/174
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/175
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/176
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/177
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/178
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/179
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k;

-- case: research/nested-joins/180
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/181
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/182
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/183
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/184
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/185
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/186
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/187
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/188
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/189
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/190
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/191
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/192
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/193
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/194
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a LEFT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/195
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a LEFT JOIN b ON a.k = b.k) CROSS JOIN c ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a LEFT JOIN b ON a.k = b.k) CROSS JOIN c;

-- case: research/nested-joins/196
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c CROSS JOIN (a LEFT JOIN b ON a.k = b.k) ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c CROSS JOIN (a LEFT JOIN b ON a.k = b.k);

-- case: research/nested-joins/197
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k;

-- case: research/nested-joins/198
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/199
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/200
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/201
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/202
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/203
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/204
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/205
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k;

-- case: research/nested-joins/206
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/207
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/208
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/209
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/210
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/211
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/212
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/213
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/214
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/215
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/216
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/217
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/218
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/219
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/220
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/221
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k;

-- case: research/nested-joins/222
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/223
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/224
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/225
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/226
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/227
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/228
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/229
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k;

-- case: research/nested-joins/230
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/231
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/232
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/233
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/234
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/235
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/236
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/237
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/238
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/239
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/240
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/241
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/242
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/243
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/244
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/245
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k;

-- case: research/nested-joins/246
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/247
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/248
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/249
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/250
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/251
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/252
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/253
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k;

-- case: research/nested-joins/254
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/255
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/256
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/257
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/258
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/259
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/260
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/261
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/262
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/263
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/264
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/265
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/266
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/267
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/268
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/269
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k;

-- case: research/nested-joins/270
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/271
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/272
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/273
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/274
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/275
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/276
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/277
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k;

-- case: research/nested-joins/278
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/279
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/280
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/281
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/282
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/283
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/284
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/285
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/286
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/287
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/288
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/289
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/290
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/291
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/292
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a RIGHT JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/293
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a RIGHT JOIN b ON a.k = b.k) CROSS JOIN c ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a RIGHT JOIN b ON a.k = b.k) CROSS JOIN c;

-- case: research/nested-joins/294
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c CROSS JOIN (a RIGHT JOIN b ON a.k = b.k) ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c CROSS JOIN (a RIGHT JOIN b ON a.k = b.k);

-- case: research/nested-joins/295
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k;

-- case: research/nested-joins/296
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/297
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/298
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/299
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/300
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/301
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/302
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/303
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k;

-- case: research/nested-joins/304
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/305
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/306
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/307
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/308
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/309
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/310
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/311
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/312
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/313
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/314
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/315
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/316
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/317
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/318
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/319
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k;

-- case: research/nested-joins/320
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/321
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/322
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/323
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/324
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/325
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/326
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/327
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k;

-- case: research/nested-joins/328
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/329
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/330
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/331
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/332
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/333
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/334
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/335
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/336
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/337
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/338
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/339
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/340
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/341
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/342
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/343
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k;

-- case: research/nested-joins/344
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/345
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/346
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/347
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/348
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/349
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/350
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/351
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k;

-- case: research/nested-joins/352
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/353
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/354
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/355
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/356
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/357
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/358
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/359
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/360
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/361
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/362
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/363
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/364
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/365
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/366
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/367
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k;

-- case: research/nested-joins/368
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k;

-- case: research/nested-joins/369
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/370
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/371
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/372
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/373
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/374
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/375
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k;

-- case: research/nested-joins/376
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k;

-- case: research/nested-joins/377
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/378
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/379
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/380
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/381
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/382
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/383
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/384
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/385
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/386
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/387
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/388
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/389
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/390
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a FULL JOIN b ON a.k = b.k) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/391
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a FULL JOIN b ON a.k = b.k) CROSS JOIN c ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a FULL JOIN b ON a.k = b.k) CROSS JOIN c;

-- case: research/nested-joins/392
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c CROSS JOIN (a FULL JOIN b ON a.k = b.k) ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c CROSS JOIN (a FULL JOIN b ON a.k = b.k);

-- case: research/nested-joins/393
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON a.k = c.k;

-- case: research/nested-joins/394
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON a.k = c.k;

-- case: research/nested-joins/395
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/396
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/397
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/398
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/399
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/400
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/401
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON b.k = c.k;

-- case: research/nested-joins/402
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON b.k = c.k;

-- case: research/nested-joins/403
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/404
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/405
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/406
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/407
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/408
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/409
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/410
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/411
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/412
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/413
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/414
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/415
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/416
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/417
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k;

-- case: research/nested-joins/418
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k;

-- case: research/nested-joins/419
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/420
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/421
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/422
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/423
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/424
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/425
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k;

-- case: research/nested-joins/426
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k;

-- case: research/nested-joins/427
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/428
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/429
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/430
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/431
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/432
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/433
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/434
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/435
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/436
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/437
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/438
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/439
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) LEFT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/440
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c LEFT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/441
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k;

-- case: research/nested-joins/442
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k;

-- case: research/nested-joins/443
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/444
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/445
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/446
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/447
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/448
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/449
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k;

-- case: research/nested-joins/450
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k;

-- case: research/nested-joins/451
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/452
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/453
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/454
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/455
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/456
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/457
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/458
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/459
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/460
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/461
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/462
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/463
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) RIGHT JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/464
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c RIGHT JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/465
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k;

-- case: research/nested-joins/466
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k;

-- case: research/nested-joins/467
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/468
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/469
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/470
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/471
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/472
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON a.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/473
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k;

-- case: research/nested-joins/474
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k;

-- case: research/nested-joins/475
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/476
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/477
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/478
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/479
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/480
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON b.k = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/481
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/482
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k;

-- case: research/nested-joins/483
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/484
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE c.k IS NOT NULL;

-- case: research/nested-joins/485
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/486
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE a.v > 15 OR a.v IS NULL;

-- case: research/nested-joins/487
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) FULL JOIN c ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/488
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c FULL JOIN (a CROSS JOIN b) ON coalesce(a.k, b.k) = c.k WHERE b.k IS NULL;

-- case: research/nested-joins/489
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM (a CROSS JOIN b) CROSS JOIN c ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM (a CROSS JOIN b) CROSS JOIN c;

-- case: research/nested-joins/490
CREATE TABLE a(k INT, v INT);
CREATE TABLE b(k INT, v INT);
CREATE TABLE c(k INT, v INT);
INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);
INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);
INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);
CREATE INDEX bk ON b(k);
SELECT a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv FROM c CROSS JOIN (a CROSS JOIN b) ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST;
SELECT count(*) FROM c CROSS JOIN (a CROSS JOIN b);
