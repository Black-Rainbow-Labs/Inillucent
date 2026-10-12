-- Regression cases for FTS5 defects found by the bug hunt of October 2026.
-- Each case printed something different from the pinned SQLite before the fix.

-- case: bug-hunt-8/fts5/rebuild-keeps-statistics
-- rebuild resets the row and token totals instead of adding to them
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(rowid,a) VALUES(1,'x y'),(2,'y'),(3,'z'),(4,'z z'),(5,'q');
SELECT rowid, bm25(ft) FROM ft WHERE ft MATCH 'x y';
INSERT INTO ft(ft) VALUES('rebuild');
SELECT rowid, bm25(ft) FROM ft WHERE ft MATCH 'x y';
INSERT INTO ft(ft) VALUES('rebuild');
SELECT rowid, bm25(ft) FROM ft WHERE ft MATCH 'x y';

-- case: bug-hunt-8/fts5/prefix-query-with-pending-write
-- a prefix query after a write in the same transaction answers like SQLite
CREATE VIRTUAL TABLE ft USING fts5(a);
BEGIN;
INSERT INTO ft(rowid,a) VALUES(1,'release'),(2,'relax'),(3,'other');
SELECT rowid FROM ft WHERE ft MATCH 'rel*';
UPDATE ft SET a = 'relevant' WHERE rowid = 3;
SELECT rowid FROM ft WHERE ft MATCH 'rel*';
DELETE FROM ft WHERE rowid = 1;
SELECT rowid FROM ft WHERE ft MATCH 'rel*';
COMMIT;
SELECT rowid FROM ft WHERE ft MATCH 'rel*';

-- case: bug-hunt-8/fts5/update-rowid
-- UPDATE of rowid changes the rowid and leaves the columns alone
CREATE VIRTUAL TABLE ft USING fts5(a, b);
INSERT INTO ft(rowid,a,b) VALUES(1,'one','x'),(2,'two','y');
UPDATE ft SET rowid = 10 WHERE rowid = 2;
SELECT rowid, a, b FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'two';
UPDATE ft SET rowid = 1 WHERE rowid = 10;
SELECT rowid, a, b FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'two';

-- case: bug-hunt-8/fts5/multirow-insert-is-atomic
-- a statement that fails part way leaves none of its rows
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(rowid,a) VALUES(1,'one');
INSERT INTO ft(rowid,a) VALUES(2,'two'),(1,'dup');
SELECT rowid FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'two';
BEGIN;
INSERT INTO ft(rowid,a) VALUES(3,'three');
INSERT INTO ft(rowid,a) VALUES(4,'four'),(1,'dup');
SELECT rowid FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'four';
COMMIT;
SELECT rowid FROM ft ORDER BY rowid;

-- case: bug-hunt-8/fts5/bm25-integer-weights
-- integer, short, zero and negative weights are all weights
CREATE VIRTUAL TABLE ft USING fts5(title, body);
INSERT INTO ft VALUES('apple','banana'),('banana','apple'),('apple apple','q'),('x','apple');
SELECT rowid, bm25(ft,10,1) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,10.0,1.0) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,10) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,0,0) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,-1,2) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,'3',1) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;

-- case: bug-hunt-8/fts5/snippet-column-minus-one
-- column -1 picks the column with the best match
CREATE VIRTUAL TABLE d USING fts5(title, body);
INSERT INTO d VALUES('One','the quick brown fox jumps over the lazy dog');
INSERT INTO d VALUES('fox','the quick brown fox jumps over the lazy dog and another fox');
SELECT snippet(d,-1,'[',']','..',6) FROM d WHERE d MATCH 'fox' ORDER BY rowid;

-- case: bug-hunt-8/fts5/snippet-window-is-centred
-- the window is centred on the match for every size
CREATE VIRTUAL TABLE d USING fts5(title, body);
INSERT INTO d VALUES('Six','alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu fox nu xi omicron pi rho sigma tau');
SELECT snippet(d,1,'[',']','..',2) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',3) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',4) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',5) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',6) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',7) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',8) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',9) FROM d WHERE d MATCH 'fox';
SELECT snippet(d,1,'[',']','..',10) FROM d WHERE d MATCH 'fox';

-- case: bug-hunt-8/fts5/rank-match-bm25-config
-- rank MATCH 'bm25(...)' sets the rank function for the query
CREATE VIRTUAL TABLE ft USING fts5(title, body);
INSERT INTO ft VALUES('apple','banana'),('banana','apple'),('apple apple','q');
SELECT rowid FROM ft WHERE ft MATCH 'apple' AND rank MATCH 'bm25(10.0, 1.0)' ORDER BY rank;
SELECT rowid FROM ft WHERE ft MATCH 'apple' AND rank MATCH 'bm25(1.0, 10.0)' ORDER BY rank;
SELECT rowid, rank FROM ft WHERE ft MATCH 'apple' AND rank MATCH 'bm25(10.0, 1.0)' ORDER BY rank;

-- case: bug-hunt-8/fts5/integrity-check-external-content
-- integrity-check with rank 1 compares the content table with the index
CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT);
INSERT INTO t(a) VALUES('one two'),('two three'),('three');
CREATE VIRTUAL TABLE ft USING fts5(a, content='t', content_rowid='id');
INSERT INTO ft(ft) VALUES('rebuild');
INSERT INTO ft(ft, rank) VALUES('integrity-check', 1);
UPDATE t SET a='changed words' WHERE id=1;
INSERT INTO ft(ft, rank) VALUES('integrity-check', 1);

-- case: bug-hunt-8/fts5/hashsize-is-accepted
-- hashsize is a setting
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(ft,rank) VALUES('hashsize',1000);
INSERT INTO ft(rowid,a) VALUES(1,'one');
SELECT rowid FROM ft WHERE ft MATCH 'one';

-- case: bug-hunt-8/fts5/huge-token-insert
-- a row with a 100000 character token is accepted or leaves nothing
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(rowid,a) VALUES(1, 'fine row');
INSERT INTO ft(rowid,a) VALUES(2, 'abc'||hex(zeroblob(100000)));
SELECT rowid, length(a) FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'fine';
SELECT rowid FROM ft WHERE ft MATCH 'abc*';

-- case: bug-hunt-8/fts5/named-rowid-then-automatic-rowid-in-a-transaction
-- an insert that names rowid 0 does not make the next automatic rowid 1
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(rowid,a) VALUES(1,'one'),(2,'two'),(3,'three');
BEGIN;
INSERT INTO ft(rowid,a) VALUES(0,'zero');
INSERT INTO ft(a) VALUES('auto');
COMMIT;
SELECT rowid, a FROM ft ORDER BY rowid;

-- case: bug-hunt-8/fts5/hashsize-values
-- hashsize takes a positive whole number, and a number given as text is stored as a number
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(ft,rank) VALUES('hashsize',0);
INSERT INTO ft(ft,rank) VALUES('hashsize','abc');
INSERT INTO ft(ft,rank) VALUES('hashsize',1.5);
INSERT INTO ft(ft,rank) VALUES('hashsize',-5);
INSERT INTO ft(ft,rank) VALUES('hashsize','2000');
SELECT k, v, typeof(v) FROM ft_config ORDER BY k;

-- case: bug-hunt-8/fts5/bm25-weights-of-other-types
-- NULL and text that is not a number weigh 0, text that starts with a number weighs that number
CREATE VIRTUAL TABLE ft USING fts5(title, body);
INSERT INTO ft VALUES('apple','banana'),('banana','apple');
SELECT rowid, bm25(ft,NULL,2) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,'x',2) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,'3abc',2) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,x'31',2) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;
SELECT rowid, bm25(ft,1,2,3) FROM ft WHERE ft MATCH 'apple' ORDER BY rowid;

-- case: bug-hunt-8/fts5/update-rowid-collision-in-a-multi-row-statement
-- a statement that moves rows onto a taken rowid changes none of them
CREATE VIRTUAL TABLE ft USING fts5(a);
INSERT INTO ft(rowid,a) VALUES(1,'one'),(2,'two'),(5,'five');
UPDATE ft SET rowid = rowid + 3 WHERE rowid < 3;
SELECT rowid, a FROM ft ORDER BY rowid;
SELECT rowid FROM ft WHERE ft MATCH 'one OR two' ORDER BY rowid;
UPDATE ft SET rowid = 'x' WHERE rowid = 1;
SELECT rowid, a FROM ft ORDER BY rowid;

-- case: bug-hunt-8/fts5/snippet-match-in-the-first-words
-- a match in the first words of a longer column is shown from the first word
CREATE VIRTUAL TABLE d USING fts5(body);
INSERT INTO d VALUES('w0 w1 w2 fox w4 w5 w6 w7 w8 w9 w10 w11 w12 w13 w14 w15 w16 w17 w18 w19');
INSERT INTO d VALUES('w0 w1 w2 w3 w4 w5 w6 w7 w8 fox w10 w11 w12 w13 w14 w15 w16 w17 w18 w19');
INSERT INTO d VALUES('w0 fox w2');
SELECT rowid, snippet(d,0,'[',']','..',4) FROM d WHERE d MATCH 'fox' ORDER BY rowid;
SELECT rowid, snippet(d,0,'[',']','..',6) FROM d WHERE d MATCH 'fox' ORDER BY rowid;
SELECT rowid, snippet(d,0,'[',']','..',10) FROM d WHERE d MATCH 'fox' ORDER BY rowid;

-- case: bug-hunt-8/fts5/snippet-sentences-and-columns
-- the start of a sentence is a window of its own, and the best column is chosen across columns
CREATE VIRTUAL TABLE d USING fts5(title, body);
INSERT INTO d VALUES('fox', 'x1 x2 x3 x4 x5 x6 x7 x8 x9 x10. The fox runs. w1 w2 w3 w4 w5 w6 w7 w8 w9 w10 w11 w12');
INSERT INTO d VALUES('fox', 'a fox');
INSERT INTO d VALUES('fox', 'fox fox');
INSERT INTO d VALUES('none', 'x1 x2 x3 x4. Then fox: here w1 w2 w3 w4 w5 w6 w7 w8 w9 w10');
SELECT rowid, snippet(d,-1,'[',']','..',6) FROM d WHERE d MATCH 'fox' ORDER BY rowid;
SELECT rowid, snippet(d,1,'[',']','..',5) FROM d WHERE d MATCH 'fox' ORDER BY rowid;
SELECT rowid, snippet(d,-1,'<b>','</b>','...',3) FROM d WHERE d MATCH 'fox OR runs' ORDER BY rowid;

-- case: bug-hunt-8/fts5/snippet-phrases-and-near
-- a phrase is one mark, overlapping phrases are joined, and NEAR marks only the words that are near
CREATE VIRTUAL TABLE d USING fts5(body);
INSERT INTO d VALUES('the quick brown fox jumps over the lazy dog and the quick brown cat');
INSERT INTO d VALUES('fox then dog and later much much much much much much a fox near a dog');
SELECT rowid, snippet(d,0,'[',']','..',8) FROM d WHERE d MATCH '"quick brown" OR "brown fox"' ORDER BY rowid;
SELECT rowid, highlight(d,0,'[',']') FROM d WHERE d MATCH '"quick brown" OR "brown fox"' ORDER BY rowid;
SELECT rowid, highlight(d,0,'[',']') FROM d WHERE d MATCH 'quick AND brown AND fox' ORDER BY rowid;
SELECT rowid, highlight(d,0,'[',']') FROM d WHERE d MATCH 'NEAR(fox dog, 3)' ORDER BY rowid;
SELECT rowid, snippet(d,0,'[',']','..',6) FROM d WHERE d MATCH 'NEAR(fox dog, 3)' ORDER BY rowid;

-- case: bug-hunt-8/fts5/highlight-column-out-of-range
-- highlight of a column that does not exist is an empty string
CREATE VIRTUAL TABLE d USING fts5(a, b);
INSERT INTO d VALUES('fox one', 'fox two');
SELECT highlight(d,0,'[',']'), highlight(d,1,'[',']'), highlight(d,2,'[',']'), highlight(d,-1,'[',']') FROM d WHERE d MATCH 'fox';

-- case: bug-hunt-8/fts5/automatic-rowid-after-deleting-the-highest-row
-- an insert with no rowid gets one more than the highest rowid the table holds now
CREATE VIRTUAL TABLE ft USING fts5(a);
BEGIN;
INSERT INTO ft(a) VALUES('x');
INSERT INTO ft(a) VALUES('y');
INSERT INTO ft(a) VALUES('z');
DELETE FROM ft WHERE rowid = 3;
INSERT INTO ft(a) VALUES('w');
COMMIT;
SELECT rowid, a FROM ft ORDER BY rowid;
BEGIN;
DELETE FROM ft WHERE rowid = 3;
INSERT INTO ft(rowid,a) VALUES(10,'v');
DELETE FROM ft WHERE rowid = 10;
INSERT INTO ft(a) VALUES('u');
COMMIT;
SELECT rowid, a FROM ft ORDER BY rowid;
