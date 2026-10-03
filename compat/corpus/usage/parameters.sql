-- case: params/named-forms
.parameter init
.parameter set :a 1
.parameter set @b 'two'
.parameter set $c 3.5
.parameter set ?4 NULL
.parameter set ?1 'one'
SELECT :a, @b, $c, ?4, ?1, typeof(:a), typeof(@b), typeof($c);
SELECT :missing, typeof(@missing);
.parameter list
-- case: params/reuse-in-one-statement
.parameter set :x 7
SELECT :x + :x, :x * 2 AS twice;
CREATE TABLE t(a, b);
INSERT INTO t VALUES(:x, :x);
SELECT * FROM t WHERE a = :x AND b = :x;
-- case: params/numbered-and-anonymous
.parameter set ?2 20
.parameter set ?3 30
SELECT ?2, ?3, ?;
SELECT ?1, ?2;
SELECT ?0;
SELECT ?32767;
SELECT ?32768;
-- case: params/types-and-affinity
.parameter set :n '42'
.parameter set :t 42
CREATE TABLE t(i INTEGER, s TEXT);
INSERT INTO t VALUES(:n, :t);
SELECT i, typeof(i), s, typeof(s) FROM t;
SELECT count(*) FROM t WHERE i = :n;
SELECT count(*) FROM t WHERE s = :t;
-- case: params/in-limit-and-like
.parameter set :lim 2
.parameter set :pat 'a%'
CREATE TABLE t(name);
INSERT INTO t VALUES('ab'), ('ac'), ('b'), ('ad');
SELECT name FROM t WHERE name LIKE :pat ORDER BY name LIMIT :lim;
SELECT name FROM t ORDER BY name LIMIT :lim OFFSET :lim;
