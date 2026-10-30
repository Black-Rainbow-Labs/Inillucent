-- case: bug-hunt-11/ftsquery/precedence-not-then-implicit-and
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f VALUES ('p'),('p q'),('p r'),('p q r'),('q'),('q r'),('r'),('p q r s'),('s');
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p q NOT q r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q AND r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p AND q NOT r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p OR q NOT r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q OR r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q NOT r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT (q r)';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q r s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p q NOT r s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p AND q r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p q AND r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p OR q r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p q OR r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p q OR r s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q OR r NOT s';
SELECT group_concat(rowid) FROM f WHERE f MATCH '(p NOT q) r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p AND (q NOT r)';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q AND NOT r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT NEAR(q r, 3)';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'NEAR(p q, 2) NOT r s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT "q r" s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p NOT q* r';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p OR q OR r NOT s';
SELECT group_concat(rowid) FROM f WHERE f MATCH 'p AND q AND r NOT s';
-- case: bug-hunt-11/ftsquery/nested-column-filters
CREATE VIRTUAL TABLE f USING fts5(a, b, c);
INSERT INTO f VALUES ('x','y','z'),('y','x','x'),('x','x','y'),('x y','y x','x y z');
SELECT rowid FROM f WHERE f MATCH 'a:x';
SELECT rowid FROM f WHERE f MATCH 'b:(a:x)';
SELECT rowid FROM f WHERE f MATCH '{a}:x';
SELECT rowid FROM f WHERE f MATCH 'b:({a}:x)';
SELECT rowid FROM f WHERE f MATCH '-a:(-b:x)';
SELECT rowid FROM f WHERE f MATCH 'b:(a:x OR b:x)';
SELECT rowid FROM f WHERE f MATCH '{a b}:(a:x)';
SELECT rowid FROM f WHERE f MATCH '{a b}:({b c}:x)';
SELECT rowid FROM f WHERE f MATCH '-a:(c:x)';
SELECT rowid FROM f WHERE f MATCH '-{a b}:(b:x)';
SELECT rowid FROM f WHERE f MATCH '{a b}:({a b}:x)';
SELECT rowid FROM f WHERE f MATCH '{a b}:(-b:x)';
SELECT rowid FROM f WHERE f MATCH '-a:(-a:x)';
SELECT rowid FROM f WHERE f MATCH 'a:(b:x OR c:x)';
SELECT rowid FROM f WHERE f MATCH '{a c}:(-c:x)';
SELECT rowid FROM f WHERE f MATCH 'a:(NEAR(x y, 2))';
SELECT rowid FROM f WHERE f MATCH 'b:(a:NEAR(x y, 2))';
SELECT rowid FROM f WHERE f MATCH '{a b}:NEAR(x y, 2)';
SELECT rowid FROM f WHERE f.b MATCH 'a:x';
SELECT rowid FROM f WHERE f.b MATCH 'a:x OR b:x';
SELECT rowid FROM f WHERE f.b MATCH '{a b}:x';
SELECT rowid FROM f WHERE f.b MATCH '-b:x';
SELECT rowid FROM f WHERE f.b MATCH '-a:x';
SELECT rowid FROM f WHERE f.b MATCH 'x OR a:y';
SELECT rowid FROM f WHERE b MATCH 'a:x' AND c MATCH 'x';
-- case: bug-hunt-11/ftsquery/syntax-errors-and-near-tokens
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('one two','x');
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, x)';
SELECT rowid FROM f WHERE f MATCH 'near(one two)';
SELECT rowid FROM f WHERE f MATCH 'one +';
SELECT rowid FROM f WHERE f MATCH '{}:one';
SELECT rowid FROM f WHERE f MATCH '(one) two';
SELECT rowid FROM f WHERE f MATCH 'a:(one) b:(x)';
SELECT rowid FROM f WHERE f MATCH 'a:b:one';
SELECT rowid FROM f WHERE f MATCH 'one NOT';
SELECT rowid FROM f WHERE f MATCH 'NOT one';
SELECT rowid FROM f WHERE f MATCH '';
SELECT rowid FROM f WHERE f MATCH 'one AND';
SELECT rowid FROM f WHERE f MATCH '(one';
SELECT rowid FROM f WHERE f MATCH 'one)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 1x)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one, two)';
SELECT rowid FROM f WHERE f MATCH 'NEAR()';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two';
SELECT rowid FROM f WHERE f MATCH 'one + + two';
SELECT rowid FROM f WHERE f MATCH '+ one';
SELECT rowid FROM f WHERE f MATCH '* one';
SELECT rowid FROM f WHERE f MATCH '*';
SELECT rowid FROM f WHERE f MATCH '  *  reads';
SELECT rowid FROM f WHERE f MATCH '* id';
SELECT rowid FROM f WHERE f MATCH '* ID ';
SELECT rowid FROM f WHERE f MATCH 'nosuch:one';
SELECT rowid FROM f WHERE f MATCH 'a:';
SELECT rowid FROM f WHERE f MATCH 'one two (three)';
SELECT rowid FROM f WHERE f MATCH 'one (three)';
SELECT rowid FROM f WHERE f MATCH 'one AND two (x)';
SELECT rowid FROM f WHERE f MATCH 'one OR OR two';
SELECT rowid FROM f WHERE f MATCH 'a:(one two) b:x';
SELECT rowid FROM f WHERE f MATCH 'foo(one two)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 1, 2)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two,)';
SELECT rowid FROM f WHERE f MATCH 'one;two';
SELECT rowid FROM f WHERE f MATCH 'a:-one';
SELECT rowid FROM f WHERE f MATCH '^a:one';
SELECT rowid FROM f WHERE f MATCH '"unterminated';
SELECT rowid FROM f WHERE f MATCH 'one two (';
SELECT rowid FROM f WHERE f MATCH '{a b';
SELECT rowid FROM f WHERE f MATCH '{a b}';
SELECT rowid FROM f WHERE f MATCH '{a}: ';
SELECT rowid FROM f WHERE f MATCH 'a:one:two';
SELECT rowid FROM f WHERE f MATCH 'one ^';
SELECT rowid FROM f WHERE f MATCH 'one * *';
SELECT rowid FROM f WHERE f MATCH 'one + two *';
SELECT rowid FROM f WHERE f MATCH '^ ^one';
SELECT rowid FROM f WHERE f MATCH 'one "';
SELECT rowid FROM f WHERE f MATCH '""';
SELECT rowid FROM f WHERE f MATCH '" "';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two) NEAR(';
SELECT rowid FROM f WHERE f MATCH 'NEAR(,)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one, )';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 3) one';
SELECT rowid FROM f WHERE f MATCH 'one''two';
SELECT rowid FROM f WHERE f MATCH 'one,two';
SELECT rowid FROM f WHERE f MATCH 'one{';
SELECT rowid FROM f WHERE f MATCH 'one}';
SELECT rowid FROM f WHERE f MATCH ':one';
SELECT rowid FROM f WHERE f MATCH 'one:';
SELECT rowid FROM f WHERE f MATCH '-';
SELECT rowid FROM f WHERE f MATCH '- a';
SELECT rowid FROM f WHERE f MATCH 'a: -b';
SELECT rowid FROM f WHERE f MATCH '{a b}:';
SELECT rowid FROM f WHERE f MATCH '{a a}:one';
SELECT rowid FROM f WHERE f MATCH '{A}:one';
SELECT rowid FROM f WHERE f MATCH 'A:one';
SELECT rowid FROM f WHERE f MATCH '"a":one';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 0x3)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, +3)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, -3)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 3.5)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 00012)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two, 99999999999)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a:one two)';
SELECT rowid FROM f WHERE f MATCH 'NEAR((one) two)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(NEAR(one two) three)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(^one ^two)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one* "two three"*, 2)';
SELECT rowid FROM f WHERE f MATCH 'AND';
SELECT rowid FROM f WHERE f MATCH 'OR';
SELECT rowid FROM f WHERE f MATCH 'one OR';
SELECT rowid FROM f WHERE f MATCH 'OR one';
SELECT rowid FROM f WHERE f MATCH 'one AND AND two';
SELECT rowid FROM f WHERE f MATCH '( )';
SELECT rowid FROM f WHERE f MATCH '()';
SELECT rowid FROM f WHERE f MATCH '(())';
SELECT rowid FROM f WHERE f MATCH '(one) AND';
SELECT rowid FROM f WHERE f MATCH '(one OR two';
SELECT rowid FROM f WHERE f MATCH 'one NOT NOT two';
SELECT rowid FROM f WHERE f MATCH 'NEAR NEAR(one two)';
SELECT rowid FROM f WHERE f MATCH 'one two NEAR(three four)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(one two) NEAR(three four)';
SELECT rowid FROM f WHERE f MATCH 'a:NEAR(one two)';
SELECT rowid FROM f WHERE f MATCH '{a}:NEAR(one two)';
SELECT rowid FROM f WHERE f MATCH '{a}:(NEAR(one two))';
SELECT rowid FROM f WHERE f MATCH NULL;
SELECT rowid FROM f WHERE f MATCH 'one' || char(11);
SELECT rowid FROM f WHERE f MATCH 'one' || char(12);
SELECT rowid FROM f WHERE f MATCH 'one' || char(10);
SELECT rowid FROM f WHERE f MATCH 'one' || char(13) || 'two';
SELECT rowid FROM f WHERE f MATCH 'one' || char(9) || 'two';
SELECT rowid FROM f WHERE f MATCH 'one' || char(26);
SELECT rowid FROM f WHERE f MATCH 'one' || x'ff';
SELECT rowid FROM f WHERE f MATCH 1;
SELECT rowid FROM f WHERE f MATCH 1.5;
SELECT rowid FROM f WHERE f MATCH x'6f6e65';
SELECT rowid FROM f WHERE f MATCH 'one' AND f MATCH NULL;
SELECT rowid FROM f('one','two');
SELECT rowid FROM f(NULL);
SELECT rowid FROM f('');
SELECT rowid FROM f('one', NULL);
SELECT rowid FROM f WHERE f = 'one';
SELECT rowid FROM f WHERE b MATCH 'one' AND f MATCH 'two';
SELECT rowid FROM f WHERE a MATCH '';
SELECT rowid FROM f WHERE a MATCH 'b:x';
-- case: bug-hunt-11/ftsquery/deep-nesting-is-refused-not-crashed
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f VALUES ('x');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(60000)),'00','(') || 'x' || replace(hex(zeroblob(60000)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(40000)),'00','(') || 'x' || replace(hex(zeroblob(40000)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(97)),'00','(') || 'x' || replace(hex(zeroblob(97)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(98)),'00','(') || 'x' || replace(hex(zeroblob(98)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(99)),'00','(') || 'x' || replace(hex(zeroblob(99)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(100)),'00','(') || 'x' || replace(hex(zeroblob(100)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(50)),'00','(') || 'x OR ' || replace(hex(zeroblob(50)),'00',')') || 'x' || replace(hex(zeroblob(50)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(40)),'00','x AND (') || 'x' || replace(hex(zeroblob(40)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(30)),'00','a:(') || 'x' || replace(hex(zeroblob(30)),'00',')');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(35)),'00','a:(') || 'x' || replace(hex(zeroblob(35)),'00',')');
SELECT rowid FROM f WHERE f MATCH 'x' || replace(hex(zeroblob(255)),'00',' NOT y');
SELECT rowid FROM f WHERE f MATCH 'x' || replace(hex(zeroblob(256)),'00',' NOT y');
SELECT rowid FROM f WHERE f MATCH 'x' || replace(hex(zeroblob(257)),'00',' NOT y');
SELECT rowid FROM f WHERE f MATCH 'x' || replace(hex(zeroblob(3000)),'00',' AND x');
SELECT rowid FROM f WHERE f MATCH 'y' || replace(hex(zeroblob(3000)),'00',' OR x');
SELECT rowid FROM f WHERE f MATCH 'x' || replace(hex(zeroblob(3000)),'00',' x');
SELECT rowid FROM f WHERE f MATCH replace(hex(zeroblob(5000)),'00','(') || 'x';
-- case: bug-hunt-11/ftsquery/rowid-compared-as-a-number
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f(rowid,a) VALUES (5,'five five'),(6,'six five'),(7,'seven');
SELECT rowid FROM f WHERE rowid = '5';
SELECT rowid FROM f WHERE rowid = 5.0;
SELECT rowid FROM f WHERE rowid = ' 5 ';
SELECT rowid FROM f WHERE rowid = '5.0';
SELECT rowid FROM f WHERE rowid = 5.5;
SELECT rowid FROM f WHERE rowid = 'five';
SELECT rowid FROM f WHERE rowid = NULL;
SELECT rowid FROM f WHERE rowid = '6abc';
SELECT rowid FROM f WHERE f MATCH 'five' AND rowid = '5';
SELECT rowid FROM f WHERE f MATCH 'five' AND rowid = 6.0;
SELECT rowid FROM f WHERE f MATCH 'five' AND rowid = 7;
SELECT rowid FROM f WHERE f MATCH 'five' AND rowid = 5.5;
SELECT rowid, rank FROM f WHERE f MATCH 'five' AND rowid = '5' ORDER BY rank;
SELECT count(*) FROM f WHERE f MATCH 'five' AND rowid = '6';
-- case: bug-hunt-11/ftsquery/rank-function-forms
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('one two','one'),('one','x one one'),('three','one');
SELECT rowid FROM f WHERE f MATCH 'one' AND rank MATCH 'bogus()';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(a)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank = 'bm25(2)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(2)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'BM25(2.5, 0)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(1e1)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(1) trailing';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH '  bm25 ( 1 , 2 )  ';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(''3'', x''31'')';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(null, +2)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25()';
SELECT rowid FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(';
SELECT rowid FROM f WHERE f MATCH 'one' AND rank MATCH NULL;
SELECT rowid FROM f WHERE f MATCH 'one' ORDER BY rank;
SELECT rowid FROM f WHERE f MATCH 'one' AND rank MATCH 'bogus()' ORDER BY rank;
SELECT rowid FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(a)' ORDER BY rank;
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(.5, 1.)';
SELECT rowid FROM f WHERE rank MATCH 'bogus()';
SELECT rowid, rank FROM f WHERE rank MATCH 'bm25()';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(5, 5)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'one' AND rank MATCH 'bogus()';
SELECT rowid FROM f WHERE f MATCH 'one' AND rank = 5;
SELECT rowid FROM f WHERE f MATCH 'one' AND rank = 'bm25(0x10)';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank = 'bm25(0, 10)' ORDER BY rank DESC;
INSERT INTO f(f, rank) VALUES('rank', 'bm25(10.0, 1.0)');
SELECT rowid, rank FROM f WHERE f MATCH 'one';
SELECT rowid, rank FROM f WHERE f MATCH 'one' AND rank MATCH 'bm25(1, 10)';
-- case: bug-hunt-11/ftsquery/highlight-and-snippet-arguments
CREATE VIRTUAL TABLE f USING fts5(a, b UNINDEXED);
INSERT INTO f VALUES (123, 'hello 5'),('hello world', 1.5),('say hello', NULL),('dogs here and some more words after it', 'dogs here and some more words after it');
SELECT rowid, highlight(f,1,'[',']') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0,NULL,NULL) FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,'0',1,2.5) FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0.9,'[',x'5d') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,NULL,'[',']') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,-1,'[',']') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,2,'[',']') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,'abc','[',']') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0,'[',']') FROM f WHERE f MATCH '123';
SELECT rowid, snippet(f,0,'[',']','...',NULL) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,0,'[',']','...',100) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,0,'[',']','...',-5) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,0,NULL,NULL,NULL,3) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,'0',1,2,3,'2') FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,0,'[',']','...',1.9) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,5,'[',']','..',3) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,-3,'[',']','..',3) FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,1,'[',']','..',3) FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0,'[') FROM f WHERE f MATCH 'hello';
SELECT rowid, snippet(f,0,'[',']','..') FROM f WHERE f MATCH 'hello';
SELECT highlight(f,0,'[',']') FROM f;
SELECT snippet(f,0,'[',']','..',3) FROM f;
SELECT rowid, bm25(f) FROM f;
SELECT rowid, bm25(f, 'x', 2) FROM f WHERE f MATCH 'hello';
SELECT rowid, bm25(f, NULL) FROM f WHERE f MATCH 'hello';
SELECT rowid, bm25(f, '2abc', x'33') FROM f WHERE f MATCH 'hello';
SELECT rowid, bm25(f, 1,2,3,4) FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0,'[',']','x') FROM f WHERE f MATCH 'hello';
SELECT rowid, highlight(f,0,'[',']'), snippet(f,0,'[',']','..',3) FROM f WHERE f MATCH 'dogs';
SELECT highlight(f,1,'[',']'), snippet(f,1,'[',']','..',3) FROM f WHERE f MATCH 'dogs';
-- case: bug-hunt-11/ftsquery/near-with-three-phrases-and-the-window
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f VALUES ('a x b c'),('a b c'),('a x x b x c'),('c b a'),('a b x x x x c'),('b a c'),('a c b'),('a b'),('p q p z z z z z z z z z z p'),('p q'),('q p'),('p p p q'),('z');
SELECT rowid FROM f WHERE f MATCH 'NEAR(a b c, 1)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a b c, 0)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a b c, 2)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a b c, 4)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(c b a, 1)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a x b c, 1)';
SELECT rowid FROM f WHERE f MATCH 'NEAR("a x" b c, 1)';
SELECT rowid FROM f WHERE f MATCH 'NEAR(a b c)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'NEAR(p q, 2)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'NEAR(p q, 1)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'NEAR(p q, 0)';
SELECT rowid, bm25(f), highlight(f,0,'[',']') FROM f WHERE f MATCH 'NEAR(p q, 3)';
SELECT rowid, highlight(f,0,'[',']') FROM f WHERE f MATCH 'NEAR(a b c, 2)';
SELECT rowid, snippet(f,0,'[',']','..',3) FROM f WHERE f MATCH 'NEAR(a b c, 4)';
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR(a b c, 2) OR NEAR(p q, 1)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/phrases-count-only-where-their-branch-matched
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f VALUES ('q x'),('x'),('y z'),('q y'),('y'),('x y z q'),('rare common'),('common'),('common'),('common'),('common');
SELECT rowid, highlight(f,0,'[',']') FROM f WHERE f MATCH 'q OR zz x';
SELECT rowid, highlight(f,0,'[',']') FROM f WHERE f MATCH 'y OR x NOT (q OR z)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'q OR zz x';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'common OR (nothing AND rare)';
SELECT rowid, bm25(f) FROM f WHERE f MATCH 'common OR (rare AND common)';
SELECT rowid, highlight(f,0,'[',']'), bm25(f) FROM f WHERE f MATCH 'x OR (y AND q)';
SELECT rowid, highlight(f,0,'[',']'), bm25(f) FROM f WHERE f MATCH 'x NOT q';
SELECT rowid, highlight(f,0,'[',']'), bm25(f) FROM f WHERE f MATCH '(x OR y) NOT (q OR z)';
SELECT rowid, highlight(f,0,'[',']'), bm25(f) FROM f WHERE f MATCH 'y OR (x AND z) OR q';
SELECT rowid, snippet(f,0,'[',']','..',3), bm25(f) FROM f WHERE f MATCH 'x OR (y AND q)';
SELECT rowid, rank FROM f WHERE f MATCH 'x OR (y AND q)' ORDER BY rank;
SELECT rowid, rank FROM f WHERE f MATCH 'q OR (y AND z)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/special-queries-answer-one-row
CREATE VIRTUAL TABLE f USING fts5(a);
INSERT INTO f VALUES ('one');
SELECT rowid FROM f WHERE f MATCH '* id';
SELECT rowid FROM f WHERE f MATCH '*ID ';
SELECT rowid FROM f WHERE f MATCH '*reads';
SELECT rowid FROM f WHERE f MATCH '* reads extra';
SELECT rowid FROM f WHERE f MATCH '*';
SELECT rowid FROM f WHERE f MATCH '*nothing';
SELECT rowid FROM f WHERE f MATCH 'one' AND f MATCH '*nothing';
SELECT rowid FROM f WHERE f MATCH '*nothing' AND f MATCH 'one';
SELECT rowid FROM f WHERE f MATCH ':' AND f MATCH '*nothing';
SELECT a FROM f WHERE f MATCH '*id';
-- case: bug-hunt-11/ftsquery/generated-1 | (b:qr) OR (((pq) AND (pq)) AND ("q p" NOT s))
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r s t','r'),('s p t q r t pq','qr qr r'),('s q p p r p t','r q pq qr q r'),('r s s qr qr t qr qr q','t q s q'),('qr r p qr q pq','r'),('r','pq r'),('t pq','p p qr pq s r t qr pq'),('r q qr pq t','s p r s qr r'),('q qr qr s s r p','pq s s pq q p'),('t p r q','pq s t'),('qr s r qr pq t','t p qr pq p'),('s p','s s s p t qr'),('s pq r t qr r pq','t p p s pq p'),('qr','r r s'),('r q p t t s s','p t p qr'),('t s t q t r','t pq pq qr'),('r pq p t p p q pq','t pq r'),('r t p t r s p p','qr r'),('s r s qr p q','s s pq qr s t q t'),('qr r','q p q'),('qr pq q pq','s r pq r s q'),('t t r pq','t pq pq qr pq qr'),('s q q s','t s q qr'),('pq p','r p pq r pq t r r');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(b:qr) OR (((pq) AND (pq)) AND ("q p" NOT s))' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(b:qr) OR (((pq) AND (pq)) AND ("q p" NOT s))' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-2 | NEAR(q s, 2) AND NEAR(q pq, 3) NOT (t*) AND (NEAR(r s, 2)) OR NEAR(p t, 0)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t r','qr t s pq qr r q p'),('t pq r s t','pq'),('pq r qr q q q pq p','qr pq pq'),('s pq s q t r','q pq t q r pq s p'),('pq t qr qr','t q q pq t q'),('qr','qr qr r r'),('s q s t','t'),('p s q s t q qr t','r s s q qr t'),('qr pq t p','q pq t p p p q r r'),('pq qr p qr q t','q t qr'),('pq p qr qr qr p pq qr t','r t pq'),('pq pq','s qr'),('qr','qr pq r qr'),('r t t t','r t qr pq qr s pq t'),('p r p t r pq r p t','pq pq'),('r s pq t','qr q qr t pq pq'),('qr p pq q qr r','pq pq p t t t r t'),('qr pq qr s t t t qr s','p'),('q pq pq r t','r p t qr p t t p r'),('qr r q r q r t pq t','p p t'),('s p pq t t pq t r p','qr pq qr r p qr r s'),('p r t','pq pq qr r t p t t p'),('q','p'),('r','pq qr q r pq t qr p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'NEAR(q s, 2) AND NEAR(q pq, 3) NOT (t*) AND (NEAR(r s, 2)) OR NEAR(p t, 0)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR(q s, 2) AND NEAR(q pq, 3) NOT (t*) AND (NEAR(r s, 2)) OR NEAR(p t, 0)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-3 | (({a b}:NEAR(q t, 2) {a b}:NEAR(t "r qr" pq, 0)) AND (a:NEAR(qr "pq q", 2))) OR (q)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('qr q pq pq qr pq qr t p','qr s'),('pq t r r','q pq q qr t'),('t','t q p pq pq r'),('qr','t p qr s'),('p','s s t pq s r s q'),('q q r','q q qr s q'),('p qr t','t s'),('r s r r t t r','q pq r q'),('pq pq q r pq','q s t pq r s t r r'),('qr t','qr p p'),('p q q q p pq r pq s','q r pq p r qr s qr r'),('q q q','qr s s t q s q p'),('r r qr','pq pq s q r r'),('p p p','p'),('r t p t p r r','s q t qr p pq s qr'),('q s q p p qr','qr t qr p p q'),('p s s','q pq r'),('t p s r r p qr q pq','p pq s r pq s r q q'),('s t s q q s t','r qr pq'),('t qr r t','pq pq pq pq pq r pq t t'),('s r r r t q pq t qr','pq'),('pq pq r','q r qr s q p'),('t s pq','t p'),('r t qr qr qr p','p t s qr pq qr t pq t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(({a b}:NEAR(q t, 2) {a b}:NEAR(t "r qr" pq, 0)) AND (a:NEAR(qr "pq q", 2))) OR (q)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(({a b}:NEAR(q t, 2) {a b}:NEAR(t "r qr" pq, 0)) AND (a:NEAR(qr "pq q", 2))) OR (q)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-4 | t NOT ((NEAR(r "q t", 0)) AND (a:"r r")) AND (b:r* NOT "r pq p")
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r t r pq p r r','p t t t pq qr'),('t s','r pq s q pq r t q'),('t','pq s q p t qr p t q'),('p t s r q q q r','q qr qr qr pq'),('s q pq t q','qr pq s r r r r q r'),('q qr q s p pq p','p qr'),('t pq q pq s qr s t pq','p pq s t p s t t s'),('t p t r','pq qr p p s s r p t'),('t','qr q pq q qr p t t'),('pq s t qr t r','p q'),('t s qr','r qr'),('p','s q p qr t s'),('t','pq q'),('s pq qr s q t pq','q qr qr'),('s pq','qr t q r t s t r q'),('q r pq p t r qr pq r','r qr'),('t qr q','pq t'),('t s qr q qr q','r'),('q p q q p pq qr','t r qr t r pq t r'),('t qr p s','pq qr qr p q p r q'),('pq','t s pq s s'),('t','q t r q s qr p s'),('t qr t','r r s'),('p r qr s','pq p pq p pq q s');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 't NOT ((NEAR(r "q t", 0)) AND (a:"r r")) AND (b:r* NOT "r pq p")' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 't NOT ((NEAR(r "q t", 0)) AND (a:"r r")) AND (b:r* NOT "r pq p")' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-5 | NEAR(p s p, 2) AND q OR "qr t" AND p OR a:"p pq"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq q p s r t p pq','t q q qr pq s p'),('t pq','pq q r s r t r q t'),('qr q s q qr pq qr','q'),('p s t s','pq'),('q p q s p r','pq'),('qr qr qr','t qr t s p'),('pq p','qr p q p qr r q'),('qr','p'),('pq','qr qr t'),('pq r qr r pq pq s pq','pq r q pq q pq q'),('s q q q p','s t p'),('s t p pq qr r r q','qr qr t s'),('t p s pq t p qr','r t t qr qr q q pq'),('r p t q pq qr pq pq','r p pq t'),('s r p qr pq q q','s pq s q qr s s r'),('s pq t r','qr pq r s r qr t p'),('qr qr r p q t p r t','q qr qr pq q q q s'),('t q','r r'),('r pq t pq','qr s qr p'),('pq q r s pq p r','r q t'),('p pq','s t'),('pq r r pq pq r t r','s'),('t pq r r','p t s qr s t s'),('qr t pq qr r p','p p pq s qr pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'NEAR(p s p, 2) AND q OR "qr t" AND p OR a:"p pq"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR(p s p, 2) AND q OR "qr t" AND p OR a:"p pq"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-6 | NEAR(qr qr, 0)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p s pq','s t pq'),('s','pq pq'),('q','p qr p r'),('r p qr pq','qr pq pq r qr q'),('pq r pq s q t pq s','pq q'),('p q t r p pq pq s','qr p p'),('r p pq s s s','q p q qr q'),('r q r pq q','p t t q'),('q pq r p qr s s pq r','t q qr s pq p t s'),('p q','pq p pq pq'),('r pq qr t p q','q q s t t r'),('t p','s'),('qr p p t','s r'),('q pq q t p s p','qr pq p pq qr p qr r'),('qr p r p q','pq r p'),('q pq qr t p q t','q q t q qr t s pq'),('q t r pq pq r','qr p pq p t qr'),('r qr','s qr'),('t','qr qr'),('pq p p qr t t q p','t pq q p pq qr qr pq s'),('qr r r t pq r r','pq qr q r pq t'),('pq','p s q'),('s q q qr p p','r pq pq pq p qr'),('r t s t p r s','r p p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'NEAR(qr qr, 0)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR(qr qr, 0)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-7 | (a:^s) AND (r AND a:q*) OR "t r t" OR qr NOT a:q
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t','pq pq pq'),('q q q pq pq t t','r r q t t qr'),('s qr q t t t q','q pq p r s'),('q p','s p qr s s t'),('q s pq p','p r r r t q s t'),('p t','t p t pq qr p pq'),('r','qr p r s'),('t r pq pq p','p'),('t qr r q pq s s q','p pq p q p t r pq r'),('t s','t q'),('t p','q r t r t'),('q p','p t'),('pq','pq qr p'),('r s p','t s pq'),('s s qr pq pq pq','s p'),('t','q pq qr pq r'),('pq s pq t t pq p pq','s r s'),('s q s t q s qr q','q q qr'),('pq t p q s','t t q s q q s'),('r s r qr','s s'),('q p r p q r s s','qr p q qr'),('r qr s','qr t q qr pq'),('r pq','qr t r pq pq'),('p','q t s r q pq p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(a:^s) AND (r AND a:q*) OR "t r t" OR qr NOT a:q' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(a:^s) AND (r AND a:q*) OR "t r t" OR qr NOT a:q' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-8 | (s OR r) AND (-b:({a b}:t AND p)) OR -b:(NEAR(s "r t" t, 2) AND {a b}:"q pq" AND {a b}:qr*)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p q s t p t','q qr r t'),('q qr qr','p s r t pq s qr'),('p pq pq s qr s qr s qr','pq q s r t qr s s'),('r','q pq pq s q'),('qr r t qr s r r pq t','qr r r p p'),('pq q r t s p s pq','q r pq t'),('p t','p q q'),('r p q t','s r s p s'),('p pq qr r t t q s','pq s r t'),('pq q p s t s pq s','p r p t pq s p p'),('p t q s r s','s'),('r q pq t qr pq t q','s p r r t s'),('qr qr p s','r q s'),('qr r q q','p t t p s qr'),('s t q q qr t','q s s t q p q'),('r p q','t t pq qr pq p r'),('s pq r r qr q','p qr'),('pq q p s pq qr t pq','pq'),('t r','qr q r s q r'),('qr qr t qr q','s s t qr s'),('r r q q r p','qr pq qr p qr s p'),('r','qr q q q'),('t','q qr r'),('p qr r','pq qr qr s p s pq t s');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(s OR r) AND (-b:({a b}:t AND p)) OR -b:(NEAR(s "r t" t, 2) AND {a b}:"q pq" AND {a b}:qr*)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(s OR r) AND (-b:({a b}:t AND p)) OR -b:(NEAR(s "r t" t, 2) AND {a b}:"q pq" AND {a b}:qr*)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-9 | "r r p" OR a:^pq OR p AND "pq qr t"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q r s qr','p s r'),('t s q s t s q p r','t r r q t p r'),('qr q s qr','pq t qr t p q q t'),('q qr r qr q r t q pq','r pq r qr pq t s p pq'),('pq p s s s qr','qr r qr t p'),('p p s pq s t pq r q','q'),('pq q qr r pq qr s t','qr q'),('s q','qr t qr s s r qr qr'),('qr t pq pq q pq pq qr pq','p p q q pq'),('q t p p pq r q qr','qr p r s r pq q s pq'),('qr s','q s q p'),('t qr q q q','qr'),('p r','t r q q t p pq r'),('qr s s q r p','pq t r s p'),('q pq qr qr s','t s t'),('pq s s s p s qr t','q s t t'),('s p r','q r t s qr qr'),('r','q t p s t q'),('pq pq qr r s','q pq q s pq r s'),('t q t','r q r q p t qr qr'),('t pq pq q r t pq pq','p s q'),('t qr q qr p s','r p r t pq r t r t'),('pq','p t'),('s r pq s p t q p t','q q');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '"r r p" OR a:^pq OR p AND "pq qr t"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '"r r p" OR a:^pq OR p AND "pq qr t"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-10 | pq NOT (-a:s) AND (b:p*) NOT ^p OR NEAR(pq pq, 1)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r qr s q pq','qr q t r s qr t'),('q s p q pq qr r p','qr pq p t s t t s q'),('r p','s p s s'),('t pq qr t t pq s p','qr'),('p p qr t t','s q'),('t pq r pq t q','qr q s p pq'),('t p r','s p'),('q','pq p r t r r p r q'),('pq pq r r q pq p','qr pq t'),('qr p t q r','s s qr q pq t s p'),('pq t t pq p t r','p p pq'),('s','t s qr t s pq pq'),('p s p t qr s r p p','r'),('r','pq s s q'),('s p q t p r r','q qr s r s r q p qr'),('r','r'),('q qr','p'),('s t qr t q s t p','p'),('s r p pq q qr','r r qr q'),('qr r s t p pq p s','t'),('p','p pq q'),('q p s s t p','q'),('t t t s','s'),('t s pq s q p s qr qr','q r p r qr pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'pq NOT (-a:s) AND (b:p*) NOT ^p OR NEAR(pq pq, 1)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'pq NOT (-a:s) AND (b:p*) NOT ^p OR NEAR(pq pq, 1)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-11 | "qr q"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q pq p q t pq','qr t p t qr r q pq'),('t r t pq q pq t p qr','s r t p s'),('q t qr q','r r r q s qr'),('qr p r t s','pq r'),('p q t pq r s','s s'),('pq r','r pq q'),('t r q','t p r r p q qr s t'),('t s p q q','p t s pq'),('s pq pq p pq qr','q q p qr s'),('qr qr s qr t q','q s t'),('t q q p p t t q pq','s'),('pq p q qr','pq p qr'),('qr pq s','t s q pq s r'),('qr','t pq p qr'),('p','p p qr t p'),('t s r pq s q s qr p','p p qr r'),('s','t s r'),('p s pq q r qr qr','pq q r r t t'),('q pq q qr r','s r qr'),('t t r q','qr pq r r q qr pq pq t'),('t','p'),('s','t t q q q s r t qr'),('q s pq q q pq','t qr'),('pq r q s s pq','pq r q r t s r');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '"qr q"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '"qr q"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-12 | -a:p
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q r q q q r pq s pq','r p'),('t','qr q q p r q t t'),('qr t t r','s p'),('pq','p p r p'),('s r p t pq r q','q pq q p pq t r pq s'),('t qr r qr pq p','s p'),('qr q qr qr t q s','r s t q pq s p s'),('q pq t p t t p qr','p s qr p qr r pq p s'),('r p q qr t qr qr','s s qr'),('t t s r q r s','qr r s p'),('r p','r qr s pq qr q r'),('q p qr','qr qr p'),('q q p q','s qr'),('qr p qr r r','p'),('qr','qr qr pq s'),('pq','s t q qr t qr t p'),('p qr qr q pq pq q','r pq r pq'),('q','r q t q qr r pq'),('q t t','q q r q pq pq p r'),('p','t s qr r s qr'),('r','q qr q r q t'),('t p q q s s','qr t s'),('qr t','pq'),('qr r r p s p','r qr pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '-a:p' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '-a:p' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-13 | -b:("q q t" AND "r pq") NOT ^r AND qr
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p p qr q','p qr qr p t r qr s s'),('q','p s r s p'),('s p pq r t t','s r p t pq s p'),('pq pq t','p pq qr p q qr r qr'),('qr r s pq t qr t','t'),('p','pq r qr pq qr qr s qr s'),('qr pq p t p r q','s pq t'),('p pq p p p r p pq p','p'),('t s q q p pq p t','p r s pq qr t t q r'),('p qr q t pq t','p t p s r'),('q qr q pq pq pq','q qr q pq q r r'),('pq qr pq s pq t t pq','s r q pq'),('p','pq'),('p t r qr pq s','q p s'),('pq q r r q','pq r q s r r pq t'),('qr q t s','t s pq pq r qr t pq'),('t qr','p qr t r s q s pq qr'),('t q s qr pq','q'),('qr t s p t s qr r r','q pq t'),('p r','t s r qr t s'),('pq p p s s p','qr'),('p q qr r r p s s','q'),('p t q p s r pq pq t','q r q pq p'),('r s p t s p s t','pq t t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '-b:("q q t" AND "r pq") NOT ^r AND qr' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '-b:("q q t" AND "r pq") NOT ^r AND qr' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-14 | pq*
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p r qr r pq qr r pq r','r qr r t t'),('q r q pq','qr p s s p t qr'),('q pq p t r r pq q p','pq pq t qr qr'),('t s r r q s','r s pq r r r pq'),('r pq q s q t p p q','t q r pq'),('p s q q q r p','q p s r p'),('pq pq t r q p','q p r s p pq q p r'),('pq s qr','s p q p pq'),('qr','qr'),('p qr p r r pq pq t s','q qr pq t'),('t t q q s r p pq','p pq q p s r pq'),('q q p','t pq q qr pq s r q'),('q qr s t p p p pq s','qr p q r qr t q s qr'),('p q r','pq p pq s q pq r'),('r s t qr pq qr pq','pq pq pq qr pq pq pq p'),('pq qr','p p pq'),('s r q','p q p'),('t r q t s','q q qr s qr t s'),('t pq q r pq p q r','pq t q t t t q pq'),('r pq qr t r','t pq p qr q q t pq s'),('r p q p','s q'),('t pq qr pq qr p','q s pq t'),('s r pq r qr','pq q p pq t p q qr r'),('qr p qr s s pq q','p r');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'pq*' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'pq*' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-15 | t AND -a:"r r t" AND pq OR t* AND b:"pq pq pq"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t p r t qr t r r','qr'),('q p r s','pq s pq qr p q pq p t'),('s p r s','pq pq t p'),('q pq s s r','qr'),('pq r s','qr qr p r pq r r s'),('p t pq','r p r p p pq'),('t qr s p q q pq pq','t'),('pq s s','t pq t r t r t'),('qr q','t q'),('s r r s q qr r p p','pq r pq t'),('r q','q'),('p qr','qr qr t t pq qr pq'),('s pq pq p s s t p q','r q qr'),('pq q pq p r','q qr pq s pq pq s'),('q pq r pq r','q pq p p t s p q r'),('r','p pq t pq qr qr'),('q q pq qr','t t pq pq t pq q t'),('s q p t pq pq r q','q'),('pq t pq','q'),('t s r','t qr t p qr r qr pq'),('t pq p p p q s','p s t pq t pq'),('q s','t pq q q p qr q'),('pq qr p t r','p r r qr t s'),('t s q q','p pq s t s p s p qr');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 't AND -a:"r r t" AND pq OR t* AND b:"pq pq pq"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 't AND -a:"r r t" AND pq OR t* AND b:"pq pq pq"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-16 | (s AND q*) OR (a:pq NOT qr AND "pq qr" AND a:NEAR(p "p pq" p, 3))
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r t','qr s q p p'),('p p','pq t p t pq t p t qr'),('s r pq qr p q s','r q t p'),('r p s r q qr','r'),('r qr p qr','q'),('s r r','q pq t r q qr qr r'),('t pq pq r qr q t qr pq','q r q t pq r'),('q r pq qr t t','pq pq s'),('p t t p pq t p t pq','q s r qr s p'),('qr p q t q s','q t qr pq r r'),('qr p qr t s t s p','p qr p p pq'),('pq s qr q pq qr t t','qr'),('s s s','qr p qr r q t t q'),('qr pq','pq q'),('r p q t q r','qr pq q r q qr'),('qr pq qr s t','s r qr p p q r p'),('pq','pq r r s'),('p pq q s s','q p qr p r q p s t'),('q pq p t r','s p q'),('q q p q pq r','s pq t qr q qr q'),('pq s pq','t q t s t'),('r','s s p p q'),('qr','p p p s'),('t pq s p','q qr r qr');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(s AND q*) OR (a:pq NOT qr AND "pq qr" AND a:NEAR(p "p pq" p, 3))' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(s AND q*) OR (a:pq NOT qr AND "pq qr" AND a:NEAR(p "p pq" p, 3))' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-17 | a:(s NOT s OR b:p AND a:p*) OR "s r"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq qr s p s pq r pq qr','t pq pq q q t'),('qr p t pq t pq qr pq','p qr'),('s r qr pq s p pq q','p p'),('pq q qr','r q r s qr qr'),('p','qr pq r qr'),('pq pq pq t pq qr','t s qr p qr pq p r q'),('qr q s qr q p','s p t r t r t p'),('s t t t','pq'),('p q t r q pq','t qr q s pq'),('t qr p t p pq p r','s s'),('s qr qr','t t s s p r p'),('qr t t t','pq t qr pq qr t'),('pq p','pq p p p s p r'),('q p t','r q p t'),('q p r r qr','pq'),('pq pq s p t r r qr','t t r r q q s'),('t r','s r t qr s pq s t'),('qr','s qr r q s s p qr'),('qr p qr q p p pq t','pq q q t t p pq'),('p t','t q qr'),('s t r p q pq','s pq q r pq'),('q','r p s pq q q p t'),('s q r r','qr qr p t pq'),('q t r','pq r p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'a:(s NOT s OR b:p AND a:p*) OR "s r"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'a:(s NOT s OR b:p AND a:p*) OR "s r"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-18 | {a b}:NEAR(s t, 0)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t','pq p p'),('qr q q t','r qr r t s qr p p r'),('pq q qr pq qr','r qr r s t r p p'),('pq p s q t','q qr s t pq'),('qr r q t pq p s s t','qr q pq r p pq'),('s t t qr','pq t r p r q'),('r qr s pq q','t s'),('t s q pq s qr','qr s t s r qr'),('q s p q p p p s p','p s qr q q t qr q'),('r r qr','p t t pq r p qr qr'),('r pq p t p q qr r s','pq t q qr'),('r pq pq','s p r qr s'),('q q s q qr t pq pq r','p pq qr q p pq r'),('t s s q','qr r p s s q qr'),('t p t s pq q t','p r pq'),('s pq pq qr qr r q','pq q q t pq'),('p','qr s'),('p s r','r qr p q r'),('t t p pq','r p pq qr s s'),('t p pq','qr q'),('s qr r q r pq pq p','qr t s p t pq pq'),('s pq qr q','pq pq r pq r p'),('q pq','p q s s'),('p s','pq qr pq pq p s r t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '{a b}:NEAR(s t, 0)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '{a b}:NEAR(s t, 0)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-19 | (pq* AND NEAR(p "qr p" pq, 3) OR "q p" OR b:"p qr") OR ("qr p")
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q','r r'),('s p qr s r','t'),('q','p r t t pq'),('pq q r r','t pq q s pq p'),('s t t t pq s q p qr','r t t s qr pq qr s t'),('s s t t pq t pq p t','pq p p r s'),('q q q pq pq pq r q','t s s'),('r pq qr q r qr r pq q','pq t s s t'),('q','s s r pq qr qr'),('qr q','qr p p r qr s r t q'),('qr pq q pq t r t','t q s r q s p'),('q t p s pq qr r qr t','qr qr r r t t s'),('qr pq q qr','t s'),('r pq t q pq p t qr','r q pq q q t'),('s r p qr q','qr t q pq p s'),('r pq','p r r s p t'),('q s p pq q pq','q pq q qr r r p r pq'),('p qr t p','t r t'),('q pq p s p t','qr q pq'),('t pq s r r r s q pq','pq r p pq t t p q t'),('qr p pq r','q s q q r s p r s'),('q pq t s','qr'),('pq r q s r pq q t p','pq p t r r p q pq r'),('q qr qr','q pq t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(pq* AND NEAR(p "qr p" pq, 3) OR "q p" OR b:"p qr") OR ("qr p")' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(pq* AND NEAR(p "qr p" pq, 3) OR "q p" OR b:"p qr") OR ("qr p")' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-20 | p
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q','s p q qr p qr t'),('qr r','q q q t p p qr r t'),('q t pq p s t','r s s t qr'),('s t qr q s q q t pq','t t qr pq pq s pq q p'),('qr qr p r','s s t q t t t qr'),('q r t qr r qr r s p','t'),('r','q t pq qr qr qr'),('p p q r q r t','p'),('s q r','p qr p t r s s qr'),('q','t pq pq'),('r pq pq','s q pq r t p'),('r pq r pq t p q q','s q s qr qr r t'),('p r qr pq t','s q q r pq qr q pq qr'),('q qr p p s q p pq','p s qr qr r r t'),('pq p','t r p qr qr t qr t'),('r q t q s pq t p pq','r pq'),('r t q r r p s s','r s t p'),('s r s r','s p r'),('q','p'),('qr qr','q r s q q p'),('t s qr s s s t','qr'),('q t s p qr','s r p q r p qr s'),('qr','s'),('t t pq s r s q','q qr');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'p' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'p' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-21 | q* p
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('qr r r p','s'),('pq p p t qr','q t'),('pq pq qr qr qr p q p t','r q t p qr t qr'),('q pq r qr r','t pq'),('p t qr r t s qr','s'),('r r p t','q t q s'),('q pq','pq pq s'),('s t s r qr','p pq'),('pq','t qr pq q'),('p t','q qr pq r'),('t s','q q p pq r t t qr p'),('p p','qr s s r'),('pq q s r r p t','pq p pq r r'),('r pq q p pq r q qr q','qr qr'),('r t','pq pq qr r'),('q r qr qr t p t','r pq r r s s p s'),('t qr r qr qr pq p s q','q t pq'),('r s','p s'),('p s s s r q p p','s t q s q t pq'),('t qr s pq','s qr'),('r q t pq p t qr s pq','pq p q q s r t q'),('p q pq','t'),('s r','p q'),('r p s p r q qr','p s s pq q r qr q t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'q* p' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'q* p' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-22 | "pq t" AND "r p" NOT s
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('qr','t t s q pq q t t'),('t qr s pq t pq r r t','r qr s'),('s p qr s q qr','t pq'),('r t s p r s qr q q','pq p'),('q t q q s p s','q s'),('q pq p p','qr r q t'),('qr q p q p q r','q pq r'),('t p t s','r p pq r pq q'),('q t s q q p','p pq t s s s qr q'),('s s s','p'),('q p s','q pq pq t qr pq t q'),('p p t t s','qr t pq pq r q q p q'),('r q pq r s pq s qr','t r'),('qr t pq pq s s','p q q'),('r qr s r p qr r q qr','p t s t r'),('qr','pq p r t qr p'),('pq qr','q qr p'),('p qr q s p','t q q q'),('t s s s qr s t pq','qr qr'),('q t','p s r qr s'),('s s q t pq pq pq','pq s q t qr q qr'),('s s q','p r s'),('s q p p s','s pq s'),('pq qr q pq q s pq r r','t qr qr');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '"pq t" AND "r p" NOT s' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '"pq t" AND "r p" NOT s' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-23 | (a:^t) OR (NEAR(pq r, 1)) OR {a b}:s
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq r t p q qr qr r','pq q pq t qr'),('r','s p qr t pq r'),('q pq p r pq s pq t','p p q qr qr q'),('r q','s q'),('q pq q','s t q p'),('pq q p','pq pq qr'),('s r p q qr q p q','r pq s r r q t r p'),('r t','t q s pq r'),('q p p qr s p','q r r p r qr pq'),('pq pq q q pq q q t','qr pq qr s qr'),('pq pq r','qr p pq qr q t'),('pq s t q t q s t','s q pq q p'),('pq','t s s pq r'),('q','pq p r qr q p'),('s q p','r'),('pq p qr s p pq','p s'),('p r r p t qr s qr','t pq s q r t'),('q t s t pq r','t qr s r'),('qr qr t p','pq p s pq qr'),('s q r t','qr s t q r t'),('pq s s t p s t','r q s r p'),('q t pq t s r r pq p','p r qr'),('p s q r s r pq','s pq p q s p qr q'),('pq','qr qr r s pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(a:^t) OR (NEAR(pq r, 1)) OR {a b}:s' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(a:^t) OR (NEAR(pq r, 1)) OR {a b}:s' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-24 | -a:"p qr t"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r qr t pq p q s t t','t qr qr s p q t'),('t p t qr s t t t s','qr q qr r q'),('q t q q pq s t s p','s pq p r'),('pq r qr r','pq s p'),('t t p r t pq p r','p qr p r pq p'),('qr t pq q qr q s r q','s qr s pq s p'),('qr pq qr','q'),('t p','q t t r q r'),('r t r pq q','s qr s q'),('s s','qr s qr q'),('r p t r t s pq','r t r r pq s'),('r p r pq t q s','r'),('pq q','p'),('s pq s t p t qr p','p r q q pq q t'),('r q pq','pq t r r pq'),('pq qr t qr qr qr r q','t pq t p p r s'),('q p s t','pq p'),('p pq p p qr pq','p t'),('pq q t q qr p r','q pq q qr'),('p r q','qr'),('qr t p q t','q r pq p p q q r t'),('s s','qr q s qr qr p'),('p t r r pq r qr p','pq r pq r s p r t'),('t t p','q qr q q r t r qr p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '-a:"p qr t"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '-a:"p qr t"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-25 | NEAR("qr t" "qr p" pq, 2) OR q NOT NEAR(pq p pq, 0) OR (p) OR (r) NOT t
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq q','t qr qr'),('q t r r r qr r s t','q t q r t'),('s t r p','p'),('r p q r r p','q qr'),('qr r qr s','qr'),('t t pq pq q','q pq t pq p q q p pq'),('p','pq p t'),('t r q q pq','pq s r t s qr t r'),('qr pq t q','qr qr pq r q'),('q qr pq t t t p q pq','pq'),('p pq','pq q'),('qr qr pq','t pq q'),('t q s p t s','r r'),('t s pq t p qr pq','t r t t qr pq qr t'),('t','s r q pq q qr s'),('s r qr qr r s t t','p'),('r r t pq pq q','s q pq s qr r qr p pq'),('s r pq s','qr q qr pq qr p'),('t t qr','q p'),('qr s','qr qr s'),('t r pq qr','r qr pq qr p s s q'),('qr t q s pq qr r p','t q pq pq q qr p'),('pq t qr','q'),('pq t','p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'NEAR("qr t" "qr p" pq, 2) OR q NOT NEAR(pq p pq, 0) OR (p) OR (r) NOT t' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR("qr t" "qr p" pq, 2) OR q NOT NEAR(pq p pq, 0) OR (p) OR (r) NOT t' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-26 | (r pq) AND ({a b}:NEAR(qr q, 1))
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t r pq r p qr t','t s pq s q p p pq'),('t r pq p','t r r p s t q qr'),('t pq t s','t pq'),('q r','qr p q t r qr'),('r s t pq p t','p p p qr r'),('s p qr r t p s r','q r q pq t pq p'),('r s p q q qr t','r'),('r p s qr','qr q pq t t s qr p r'),('s q pq t p r r','qr r q r t s q q'),('r','r qr r t s r q qr q'),('q t qr','r r t t p qr t'),('t r s t','pq p q q t pq q s'),('p s t s s pq pq pq','r qr pq qr q q s s qr'),('p pq t q qr q q t','s p'),('t q','pq r r qr qr pq'),('t q p q s p qr qr s','qr q s s t s'),('t q q q pq','s q r q t'),('s pq qr q s r s','q s s'),('p pq s pq q p s t','q p s pq t qr pq'),('t p r q pq q pq','p qr qr pq'),('pq','q pq p q pq s qr pq p'),('r pq q t q s pq p','qr pq s qr s s pq q p'),('q r p','t s r pq r r'),('q p qr pq t pq s','q pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(r pq) AND ({a b}:NEAR(qr q, 1))' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(r pq) AND ({a b}:NEAR(qr q, 1))' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-27 | "qr p" OR {a b}:"s qr" NOT p NOT q
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p pq pq s','r p t qr q qr t'),('pq','pq p t'),('qr r','p s q r r s'),('q pq qr q p p s','r pq t'),('s r qr s t qr q q r','t r p q s t qr qr'),('pq pq r q qr pq p p q','t pq qr'),('q pq qr r','pq p p s qr r q r q'),('pq p s r q','p q qr'),('r r p q r q s pq s','p s r t s pq qr'),('qr qr q r','p qr t p pq t pq r q'),('s q s t qr r qr pq','t p q s s'),('s qr qr q q r','qr s p p pq q s t pq'),('r pq','r q p'),('t pq q qr t s s','s q q qr p'),('q qr t p qr qr p s','q qr p'),('p pq pq pq','pq r s qr r t p'),('s qr r pq','pq pq q s pq r'),('qr s q t','r p qr qr'),('q r q','qr p s r t pq q'),('s p p qr','pq r p r r s p'),('p qr pq s pq t','p pq p pq'),('q q r t r p s p t','p t pq p r'),('s r s qr s','q t r qr r r'),('p s r qr','p q');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '"qr p" OR {a b}:"s qr" NOT p NOT q' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '"qr p" OR {a b}:"s qr" NOT p NOT q' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-28 | q OR a:NEAR(s "qr t", 3)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('s t t t p q t r','q q q qr s pq qr t t'),('t r qr qr s','p r'),('qr s s pq p','t'),('pq q t t qr pq pq t','q r t r'),('q r t qr q p s','q r t r q s t p t'),('pq s t p qr pq t p r','t t t t qr qr r t r'),('p pq q','qr qr qr q pq p pq'),('q p','r r s pq q'),('r r','q q s r qr r qr pq pq'),('q p pq p q p p','qr'),('pq t q t','q r pq p r t'),('q pq q pq pq','s qr t t q'),('q qr q t r pq pq t','qr q q p pq t'),('q t','s'),('r r t qr','r'),('t pq qr pq qr q q r','qr q'),('s pq q','s p pq t q'),('r q t p r t','t'),('qr pq t qr q r p q s','s'),('q q','pq s qr p r s qr q'),('qr','r pq q qr qr r s pq p'),('qr s','q'),('qr s p q q t r qr','p p t'),('pq s qr q','s s t s');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'q OR a:NEAR(s "qr t", 3)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'q OR a:NEAR(s "qr t", 3)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-29 | -b:(-b:(b:r OR s AND (pq*) OR ("qr s")) AND -b:(b:p AND {a b}:pq) NOT (s) AND (r))
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('s','pq pq q'),('p p pq t t r t r','t p'),('r s q s s q r','q qr r r qr pq'),('p r r s s','qr r t pq r'),('qr','t s qr t qr pq p'),('t r qr pq qr q r p t','s'),('r t r qr qr qr r q','t r p p t s pq q p'),('p pq qr qr q s t','q pq s'),('t t t r p r r p','t s'),('t qr q qr q s pq','qr t s p'),('t','p p pq s r r qr q r'),('pq p','qr pq qr r r s q t'),('r q pq','s qr r'),('q r q r s pq p','r t pq pq s p'),('q','s r'),('qr r','s p'),('r p','qr q'),('p','q'),('q pq','s qr'),('q qr pq','qr pq t q qr q q pq'),('t pq s r pq pq r','pq t t qr pq pq s'),('p p','s q pq r p'),('r r t r p t r pq','s pq t qr qr s q q'),('s','p pq p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '-b:(-b:(b:r OR s AND (pq*) OR ("qr s")) AND -b:(b:p AND {a b}:pq) NOT (s) AND (r))' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '-b:(-b:(b:r OR s AND (pq*) OR ("qr s")) AND -b:(b:p AND {a b}:pq) NOT (s) AND (r))' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-30 | qr
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('r qr r','s q q r r t pq p'),('t pq t qr t','t r p t p q pq'),('p q qr p pq t','s r pq'),('p r r p','pq s qr t p q'),('q','p s t r qr s'),('r q t t r p','pq pq s p s pq t p'),('s r r','s qr q p'),('r s pq r p r q q','p p qr t'),('q r p p','p p'),('s pq t qr p','t s t pq'),('t pq s','q'),('pq r p p qr s r','q p qr p t r t qr'),('s p','p qr'),('s p s p t qr p p','q t q p p q pq'),('q pq r p t','qr p qr r t q'),('s qr pq qr r p q','q qr q s pq q'),('t r r p p t pq q','qr'),('q qr t pq s','s p t r q pq qr r pq'),('q q','q r pq t t'),('qr qr r t pq p','s'),('s q t','pq'),('qr p q','s q r'),('s','p pq q t r r'),('r qr s t t r r p q','pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'qr' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'qr' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-31 | s
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq s s p pq q t','p'),('s q s q pq pq pq p','pq qr'),('s p t','q'),('qr r pq s','p'),('qr t q r s pq t p qr','s s s pq r qr p'),('t s p p pq','s p'),('r t r','s qr t p t s s p'),('pq s p','r p qr q t'),('pq r qr pq qr q q','q q pq r'),('r pq pq','t s s q p'),('qr','pq qr t pq s r pq'),('t r r r p pq r r t','t qr r pq qr pq s qr'),('s qr','pq'),('s p s s','r t s r'),('q pq qr p p','q pq pq p qr'),('s p qr pq t qr qr pq','qr r t qr t'),('t pq qr r pq pq','pq q r qr p p'),('p','q t pq r t qr t t'),('s p p p r t pq','t qr s pq r'),('p qr p pq q r','qr pq'),('t s r p q r','p r s pq t'),('qr qr t','s r s'),('pq pq t p pq s','q pq pq s t t'),('s t q','p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 's' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 's' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-32 | r
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t r r t r q q','s qr pq p pq qr pq t'),('t r r p qr p','p qr qr qr p s'),('qr q pq r q pq qr','t s q r pq r pq qr p'),('t pq q t','r s'),('q','q s qr pq pq r q'),('t s qr t','q r t'),('pq qr qr qr','p s'),('t pq pq qr qr p','pq q'),('s p t r q','t pq t t'),('pq r r p r p','q s p s qr q pq t'),('pq p q qr qr qr','pq pq t qr qr p qr'),('qr p t r q q s','s qr s'),('pq s s pq p','p q pq t p q s r t'),('qr pq t p q pq t qr','q p'),('pq t qr qr q','t s q pq s s'),('t q q','t q q qr r s s q'),('pq s t pq p qr qr','t s t p t pq'),('t r','r qr q q'),('s q t q q','q t s pq t qr r r'),('qr r qr qr t s','pq q p s t qr'),('q t pq','pq t pq'),('q','t t r pq s qr p t'),('pq','r p qr pq p t r r'),('pq qr qr s r q qr','s');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'r' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'r' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-33 | NEAR(q p "t qr", 3) p AND "qr r" OR a:NEAR(t r p, 3) AND NEAR("t q" q, 1)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q r r qr qr p s p','s r p s t p'),('qr s q r q p pq pq t','qr p'),('pq p q s','q pq r s s pq'),('p s t pq qr t r p r','r s s pq s t q pq q'),('p pq p p qr pq','t s q r pq r'),('r q t r pq t pq p','s r qr q pq q r'),('q pq pq qr','q r p r s pq q s p'),('s r s qr qr s s','p qr qr q p p p r pq'),('qr pq pq pq s t pq qr','pq q qr p p r s r pq'),('t t p q','qr r s q qr t'),('s q t t qr','s t qr r q'),('t r','r t t s p q'),('q t qr s q t r t','pq r q'),('pq p p p p s q t q','p qr pq p r'),('q q t q q','pq s pq pq pq'),('r s q t pq q q pq pq','s q pq pq t s qr s'),('r p s q','q t p r r qr p'),('s','q s q q t r p p'),('t q s s p p','t'),('p q qr q s t','r pq q t'),('qr qr qr r qr p p','pq p pq p t q q pq'),('q p qr','r'),('r p t t t q q','p t pq qr p qr'),('pq r p','s qr s p qr q s');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'NEAR(q p "t qr", 3) p AND "qr r" OR a:NEAR(t r p, 3) AND NEAR("t q" q, 1)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'NEAR(q p "t qr", 3) p AND "qr r" OR a:NEAR(t r p, 3) AND NEAR("t q" q, 1)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-34 | (((b:q) AND (NEAR("p qr" r, 2))) OR (pq)) OR (((s*) AND (a:NEAR(t p, 0))) OR (t* AND a:NEAR(p s qr, 3)))
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('t pq','r'),('pq t','s p s t r q'),('p r qr t','r p t r s s r r s'),('p t q p qr','p'),('q q qr q pq qr qr','s r qr qr p q s t r'),('qr q','qr pq p p q'),('qr pq t s s p t t q','s t r q'),('r','t r s'),('qr t pq r','t pq'),('q p r','q pq q s q q'),('s qr r','s q r pq qr s s r'),('qr qr r qr qr qr t qr q','qr p'),('p r qr r s t','t t s q pq'),('pq r pq p pq p q','qr s qr'),('pq q p s t','qr qr'),('s t q r pq pq q','pq pq s qr p p'),('qr s r','qr t q'),('r pq qr q r s q qr','s qr p qr pq p s p'),('r pq qr r p r qr qr r','qr s r s s r pq'),('q r pq s s','r r pq qr r qr t'),('s q pq q','q pq r t s pq r q'),('qr pq t q q p s','qr pq pq r qr pq t qr qr'),('s r r t s r s qr q','q qr t q t t pq p pq'),('p s qr s r t r','t p s r p p pq');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(((b:q) AND (NEAR("p qr" r, 2))) OR (pq)) OR (((s*) AND (a:NEAR(t p, 0))) OR (t* AND a:NEAR(p s qr, 3)))' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(((b:q) AND (NEAR("p qr" r, 2))) OR (pq)) OR (((s*) AND (a:NEAR(t p, 0))) OR (t* AND a:NEAR(p s qr, 3)))' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-35 | (qr*) OR (q AND {a b}:t) AND a:t
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p r qr qr','p'),('q t qr q q qr q p','t s qr qr p qr s p s'),('r r','qr'),('p pq p q p pq qr s','r q q p r t r p'),('s qr t p pq','q r qr qr qr s'),('q r s','qr r t p p p pq q p'),('qr s qr q q t s q pq','q q t t p q s t q'),('t pq qr','q'),('s qr','p p s'),('q p q r t q qr','p q p qr t s'),('qr s q t','p p r q t pq'),('q p q p s r p p qr','qr p r s q pq qr pq t'),('q qr pq qr r r p qr','s p qr'),('r','r q t s p p qr t'),('pq s','p qr p q qr'),('qr q pq','q pq qr q r s'),('r r s qr t q pq qr p','r t t pq pq'),('pq r s q r','r'),('p p s q t qr','s p qr r pq qr q p'),('pq t s q s','pq'),('s r pq','pq pq'),('r','q pq q r p q s r q'),('s pq qr pq q p qr qr s','t s pq t'),('q','s qr q q');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(qr*) OR (q AND {a b}:t) AND a:t' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(qr*) OR (q AND {a b}:t) AND a:t' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-36 | a:((a:NEAR(q qr, 1)) OR (-a:"pq q t") OR "pq p") NOT pq OR NEAR(q pq t, 1) OR "qr q" NOT {a b}:s
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q p t pq q','pq q p r s qr p qr q'),('q qr','s t qr t t q t q t'),('t pq q s qr pq qr p','pq q s pq t p s'),('q q qr q t r q t t','pq s'),('pq t pq pq','s qr t q pq t t'),('q t pq qr r','qr s p p s t r q r'),('s p pq qr t r s','qr t p qr pq t'),('s pq s q qr r','qr r'),('pq t t r p t pq s','s qr pq q'),('r s pq r','qr qr'),('pq','q'),('p qr qr qr t pq q','q qr p t t qr'),('t pq s pq t','r t q r r pq s r r'),('qr q p pq pq r','s pq s t q p q'),('q r p p q p t r','q'),('qr pq p s r p t','s t r'),('r','r s pq q'),('t r r q qr q q t qr','qr r t q r p s'),('q s qr pq p','p r pq q s s s s q'),('s','p s'),('qr r pq','qr s pq s r r'),('q qr r p','qr r'),('p pq r','r q t'),('p p t s','s p p p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'a:((a:NEAR(q qr, 1)) OR (-a:"pq q t") OR "pq p") NOT pq OR NEAR(q pq t, 1) OR "qr q" NOT {a b}:s' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'a:((a:NEAR(q qr, 1)) OR (-a:"pq q t") OR "pq p") NOT pq OR NEAR(q pq t, 1) OR "qr q" NOT {a b}:s' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-37 | b:s AND r NOT qr q s
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('q r t q s s q s','qr p qr r p pq t r s'),('p','s'),('qr r t qr t','t s r qr r qr'),('qr s','qr qr p s p'),('q s t r qr','r r pq s r pq'),('p t t t','s q s'),('pq','s pq s q qr r s qr t'),('t','s pq'),('q t','s r qr r p pq r q'),('s t t','p s'),('t pq','p'),('t pq qr r t t qr r pq','qr qr qr q t pq'),('q q t p qr r s','r pq t r qr pq pq'),('q p r','p p p q q qr qr qr'),('t r p s r q','q s qr q qr s q q'),('pq q q t t q s p','q r s'),('qr','q r qr q q t'),('qr qr pq p t q r','p'),('s q q r','p p s qr p p'),('s pq p r p r t','r'),('r t pq q qr s r','q p r pq q t qr'),('t s q p','t'),('pq qr r s q','r qr t t'),('p r qr t r pq','s r r s q s p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'b:s AND r NOT qr q s' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'b:s AND r NOT qr q s' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-38 | (r) AND (p) AND -a:"qr p r" OR s AND "s qr"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('p r q r pq','qr p s qr q'),('r r','pq q'),('p p q r','qr r q qr r pq'),('q p q qr r pq q pq','pq t'),('q t p q t qr qr','qr t q'),('pq q r r','qr'),('t qr','p t'),('s q pq r q pq s q','q'),('qr','p pq r s'),('pq pq pq','p q qr p'),('qr','q t'),('s qr t','pq s q'),('p r s','p pq qr r pq qr pq'),('q t qr','r p pq s t p'),('r','pq s'),('s p s','q s r r qr q'),('p t s s p t pq s s','q p p t s qr q r pq'),('r q r p t qr','p pq'),('t r p q','s s pq qr pq s'),('qr t qr p q','r s r r qr p q'),('q p','t s'),('r q p s s p','r p pq t pq qr pq'),('r r r s r','r p t s p q pq qr qr'),('s','s p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH '(r) AND (p) AND -a:"qr p r" OR s AND "s qr"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH '(r) AND (p) AND -a:"qr p r" OR s AND "s qr"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-39 | q OR NEAR(r pq t, 3) NOT NEAR(s "pq s", 2) OR b:"p p qr"
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq','s q qr qr p qr p s'),('pq r pq r p','pq qr r qr r'),('t p qr t r r s','p'),('r s','pq q'),('qr pq t pq r pq','t'),('qr r r q t r s r p','p pq t q r pq'),('pq p t','pq r q qr'),('s p qr t r p','pq p'),('q q s q s','q r pq pq s'),('pq p qr qr q','pq'),('q pq p qr s qr p s p','qr r p s s q pq'),('pq pq s p qr r r','p s t pq q pq qr qr'),('pq r','r pq s p pq pq q r t'),('r pq r pq t','r pq'),('r pq p r','t qr s r p q r pq s'),('q pq r p s t q','q p qr'),('t pq s t r r','pq q q pq qr'),('q s p qr pq s pq pq p','qr qr q p t'),('q q s p t pq s','q s r pq'),('q','s p qr t pq t r pq s'),('qr qr qr p qr s t','p pq pq p r qr'),('pq q t s pq r q pq','pq p pq pq q q qr'),('qr qr t r q','p p p q qr s p r'),('pq qr t s pq p p','s t');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 'q OR NEAR(r pq t, 3) NOT NEAR(s "pq s", 2) OR b:"p p qr"' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 'q OR NEAR(r pq t, 3) NOT NEAR(s "pq s", 2) OR b:"p p qr"' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/generated-40 | s OR (-a:s*) OR (a:pq)
CREATE VIRTUAL TABLE f USING fts5(a, b);
INSERT INTO f VALUES ('pq qr q pq t pq','s q r s t qr p t s'),('q qr r pq qr','qr pq p q t r t t'),('p','r t qr pq r t t r t'),('pq p q qr q r r','r qr qr s'),('pq t s q s q t p qr','r qr t pq'),('t qr s t qr t r qr','r pq'),('qr t pq r s p t','pq p'),('qr p pq','t s t'),('q r p p s t pq pq','t r'),('r pq qr t s p r pq r','p p qr qr s p p qr pq'),('r pq pq q','s qr r pq s q t'),('q s q','p s qr'),('p qr','p r pq r pq t qr qr'),('q pq','pq p r s q t qr pq'),('q t r t qr p pq','qr p t'),('s p s r pq pq q pq','p r p s q q t qr r'),('qr q pq qr','q pq t q t p qr t pq'),('r s q r pq qr s t r','t r pq p q pq'),('r q','pq s pq r qr qr s'),('r pq t pq qr pq pq p qr','p r r qr pq q qr'),('t qr q s s r','s q t p q r'),('r','pq r r r'),('t r s r r r r s s','p q'),('qr r','qr p r s p');
SELECT rowid, bm25(f), highlight(f,0,'[',']'), highlight(f,1,'<','>'), snippet(f,-1,'[',']','..',4) FROM f WHERE f MATCH 's OR (-a:s*) OR (a:pq)' ORDER BY rowid;
SELECT rowid, rank FROM f WHERE f MATCH 's OR (-a:s*) OR (a:pq)' ORDER BY rank;
-- case: bug-hunt-11/ftsquery/module-arguments-leave-out-the-comments-around-an-argument
CREATE VIRTUAL TABLE f1 USING fts5(a, -- first
  b /* second */, c);
SELECT sql FROM sqlite_schema WHERE name = 'f1';
INSERT INTO f1 VALUES ('x', 'y', 'z');
SELECT rowid FROM f1 WHERE f1 MATCH 'x';
SELECT name FROM pragma_table_info('f1');
CREATE VIRTUAL TABLE f2 USING fts5(a /* c1 */ UNINDEXED, b -- c2
);
CREATE VIRTUAL TABLE f4 USING fts5(/* lead */ a, b);
SELECT name FROM pragma_table_info('f4');
CREATE VIRTUAL TABLE f5 USING fts5(a, 'b -- not a comment');
SELECT name FROM pragma_table_info('f5');
CREATE VIRTUAL TABLE f8 USING fts4(a, -- c
 b);
SELECT name FROM pragma_table_info('f8');
CREATE VIRTUAL TABLE r1 USING rtree(id, -- c
 x0, x1);
SELECT name FROM pragma_table_info('r1');
CREATE VIRTUAL TABLE f9 USING fts5(a, b, -- trailing comment
);
