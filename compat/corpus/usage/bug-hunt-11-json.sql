-- case: bug-hunt-11/json/each-over-recursive-cte-table
WITH RECURSIVE t(d, n) AS (SELECT '[1,[2,[3]]]', 0 UNION ALL SELECT e.value, n+1 FROM t, json_each(t.d) e WHERE e.type = 'array') SELECT n, d FROM t;
-- case: bug-hunt-11/json/each-recursive-key-path-walk
WITH RECURSIVE t(k, v) AS (SELECT key, value FROM json_each('{"a":{"b":1},"c":2}') UNION ALL SELECT t.k || '.' || e.key, e.value FROM t, json_each(t.v) e WHERE json_type(t.v) = 'object') SELECT k, v FROM t ORDER BY k;
-- case: bug-hunt-11/json/tree-over-recursive-cte-table
WITH RECURSIVE t(d, n) AS (SELECT '[1,[2,[3]]]', 0 UNION ALL SELECT e.value, n+1 FROM t, json_tree(t.d) e WHERE e.type = 'array' AND e.id > 0) SELECT n FROM t LIMIT 5;
-- case: bug-hunt-11/json/nested-each-over-array-elements
CREATE TABLE s(j);
INSERT INTO s SELECT json_group_array(json_object('id', value, 'tags', json_array(1,2,3))) FROM generate_series(1, 2000);
SELECT count(*), sum(t.value) FROM s, json_each(s.j) e, json_each(e.value) t WHERE t.type = 'integer';
-- case: bug-hunt-11/json/set-does-not-leave-empty-containers
SELECT json_set('{}', '$.a.b[1]', 1);
SELECT json_insert('{}', '$.a.b[1].c', 1);
SELECT json_set('[1,2]', '$[2][0]', 5);
SELECT json_set('{"a":1}', '$.k.z[1]', 5);
SELECT json_set('{}', '$.a.b[0]', 1), json_set('{}', '$.a.b[#]', 1), json_set('{}', '$.a[#-1]', 1), json_set('{}', '$.a.b.c', 1);
SELECT json_replace('{}', '$.a.b[1]', 1), json_insert('[]', '$[0][0][0]', 1), json_set('[[1]]', '$[1][1]', 1);
-- case: bug-hunt-11/json/fullkey-quotes-keys-that-are-not-letters-and-digits
SELECT fullkey, path FROM json_tree('{"user_id":{"x_y":[1]}}');
SELECT fullkey FROM json_each('{"a1":1,"1a":2,"":3,"a b":4,"é":5,"A":6}');
-- case: bug-hunt-11/json/fullkey-prints-the-stored-key
SELECT fullkey FROM json_each('{"a\"b":1}');
SELECT fullkey FROM json_each('{"a\\b":1}');
SELECT fullkey FROM json_each('{"a\nb":1}');
SELECT fullkey FROM json_each('{"a\u0000b":1}');
SELECT fullkey FROM json_each('{''a"b'':1}');
-- case: bug-hunt-11/json/tree-root-path-key-and-path
SELECT path, fullkey FROM json_tree('{"a.b":100.0}', '$."a.b"');
SELECT * FROM json_tree('{"a":[1,2,3]}', '$.a[1]');
SELECT * FROM json_tree('{"a":[1,2,3],"b":2}', '$.b');
SELECT * FROM json_tree('{"a":[1,2,3],"b":2}', '$.a');
SELECT * FROM json_tree('[[1,2],[3,4]]', '$[1]');
SELECT * FROM json_tree('[[1,2],[3,4]]', '$[0]');
SELECT * FROM json_tree('{"a":{"b":1,"c":2}}', '$.a.b');
SELECT * FROM json_tree('{"a":{"b":1,"c":2}}', '$.a.c');
SELECT * FROM json_each('{"a":{"b":1,"c":2}}', '$.a.c');
-- case: bug-hunt-11/json/arrow-text-answer-is-not-json
SELECT json_array('{"x":[1]}' ->> '$.x');
SELECT json_object('k', '{"x":{"y":2}}' ->> 'x');
SELECT json_set('{}', '$.k', '{"x":[1]}' ->> '$.x');
SELECT json_group_array(j->>'x') FROM (SELECT '{"x":{"y":2}}' AS j);
SELECT json_array('{"x":[1]}' -> '$.x'), json_array('{"x":1}' -> 'x');
-- case: bug-hunt-11/json/subtype-through-functions-that-hand-on-an-argument
SELECT json_array(coalesce(json('[1]'), 0));
SELECT json_array(ifnull(json('[1]'), 0));
SELECT json_array(CASE WHEN 1 THEN json('[1]') END);
SELECT json_array(CASE 1 WHEN 1 THEN json('[1]') ELSE 2 END);
SELECT json_array(iif(1, json('[1]'), 2));
SELECT json_array(max(json('[1]'), json('[2]')));
SELECT json_array(min(json('[1]'), json('[2]')));
SELECT json_array(nullif(json('[1]'), 0));
SELECT json_array(coalesce(NULL, json('[1]')));
SELECT json_array(cast(json('[1]') AS TEXT));
SELECT json_array(cast(json('[1]') AS BLOB) IS NULL);
SELECT json_array(+json('[1]'));
SELECT json_array(json_pretty('[1]'));
SELECT json_object('k', coalesce(json('{"a":1}'), 0));
SELECT json_group_array(coalesce(json('[1]'), 0));
-- case: bug-hunt-11/json/text-that-is-not-utf8-passes-through
SELECT hex(json_array(CAST(x'ff' AS TEXT)));
SELECT hex(json_quote(CAST(x'c328' AS TEXT)));
SELECT hex(json_group_array(CAST(x'ff' AS TEXT)));
SELECT hex(json_set('{}', '$.a', CAST(x'ff' AS TEXT)));
SELECT hex(json('["' || CAST(x'ff' AS TEXT) || '"]'));
SELECT json_valid('"' || CAST(x'c0af' AS TEXT) || '"');
SELECT hex(json_extract('["' || CAST(x'ff41' AS TEXT) || '"]', '$[0]'));
SELECT hex(json_object('k' || CAST(x'80' AS TEXT), 1));
-- case: bug-hunt-11/json/each-index-from-end-path-on-object
SELECT * FROM json_each('{"a":1}', '$[#-1]');
SELECT key, value FROM json_tree('[1,2,3]', '$[#-1]');
SELECT * FROM json_tree('[[1,2],[3,4]]', '$[#-1]');
SELECT * FROM json_each('[1,2,3]', '$[#-1]');
-- case: bug-hunt-11/json/five-arrow-calls-over-a-document
CREATE TABLE d(id INTEGER PRIMARY KEY, j);
INSERT INTO d SELECT value, json_object('id', value, 'items', (SELECT json_group_array(json_object('sku', 'S'||x.value, 'qty', x.value)) FROM generate_series(1,30) x), 'meta', json_object('a',1,'b',2,'c',3,'d',4)) FROM generate_series(1,50);
SELECT sum(j->>'$.meta.a' + j->>'$.meta.b' + j->>'$.meta.c' + j->>'$.meta.d' + j->>'$.id') FROM d;
SELECT sum(json_extract(j,'$.meta.a') + json_extract(j,'$.meta.b') + json_extract(j,'$.items[3].qty') + j->>'$.items[#-1].qty' + j->'$.id') FROM d;
SELECT j->>'$.meta.c', j->'meta', j->>'items[0].sku', j->>'$.id' FROM d WHERE id IN (1, 25, 50);
SELECT sum(j->>'$.meta.a' + (j->>'nosuch' IS NULL) + j->>'$.items[1000].qty' IS NULL) FROM d;
-- case: bug-hunt-11/json/plus-sign-in-json5-numbers
SELECT hex(jsonb('+1'));
SELECT hex(jsonb('[0x10,+5]')), hex(jsonb('+0x10')), hex(jsonb('+.5')), hex(jsonb('+1.5')), hex(jsonb('+1e3')), hex(jsonb('[+Infinity,+inf]'));
SELECT json_valid('+1', 1), json_valid('+1', 2), json('+1'), json('[+1,+2.5]');
SELECT id FROM json_each('[0x10,+5,.5,5.,Infinity,-Infinity,NaN]');
-- case: bug-hunt-11/json/huge-array-index-in-path
SELECT json_extract('[1]', '$[99999999999999999999]');
SELECT json_extract('[1]', '$[#-99999999999999999999]');
SELECT json_set('[1]', '$[99999999999999999999]', 5), json_remove('[1]', '$[#-99999999999999999999]');
-- case: bug-hunt-11/json/hash-minus-zero-in-path
SELECT json_set('[1]', '$[#-0]', 'x');
SELECT json_insert('[]', '$[#-0]', 'x');
SELECT json_extract('[1,2]', '$[#-0]'), json_extract('[1,2]', '$[#-1]');
SELECT json_array_insert('[1]', '$[#-0]', 'x');
-- case: bug-hunt-11/json/valid-blob-flags
SELECT json_valid(jsonb('123')), json_valid(jsonb('123'),4), json_valid(jsonb('123'),8), json_valid(jsonb('[1,2,{"a":"b"}]'),1), json_valid(jsonb('[1,2,{"a":"b"}]'),4), json_valid(jsonb('[1,2,{"a":"b"}]'),8), json_valid(jsonb('[1,2,{"a":"b"}]'),12);
SELECT json_valid(x'0d', 4), json_valid(x'1331ff', 4), json_valid(x'0e', 4), json_valid(zeroblob(4), 4), json_valid(x'c000', 8), json_valid(x'd00000', 8), json_valid(x'c000', 4), json_valid(x'00', 8), json_valid(x'00',4), json_valid(x'00',1);
SELECT json_valid(x'7b7d'), json_valid(x'7b7d', 4), json_valid(x'7b7d',8), json_valid(x'5b315d', 3), json_valid(x'5b31', 3);
SELECT json_valid(1e999, 7), json_valid(-1e999, 3), json_valid(1e999), json_valid(1e999,2), json_valid(12), json_valid(1.5, 2);
SELECT json_valid(x'9b0000000000', 8), json_valid(x'cb08', 8), json_valid(x'cc03', 8), json_valid(x'ab', 8);
SELECT json_valid(jsonb('{"a":1.5e3,"b":[true,false,null],"c":"x\ny","d":"A"}'), 8), json_valid(jsonb('{a:0x1F,b:.5,c:5.,d:+3,e:Infinity}'), 8), json_valid(jsonb('"a\x41\v\0"'),8);
-- case: bug-hunt-11/json/json5-bare-keys
SELECT json_valid('{é:1}', 2), json_valid('{null:1}', 2), json_valid('{true:1}', 2), json_valid('{Infinity:1}', 2), json_valid('{nullx:1}', 2), json_valid('{NaN:1}', 2), json_valid('{inf:1}', 2), json_valid('{QNaN:1}', 2), json_valid('{SNaN:1}', 2), json_valid('{false_:1}', 2), json_valid('{a1:1, $b:2, _c:3}', 2), json_valid('{Ab:1}', 2), json_valid('{aA:1}', 2);
SELECT json('{é:1}'), json('{nullx:1}'), json('{Ab:1}'), jsonb('{ab:1}') = jsonb('{"ab":1}');
SELECT hex(jsonb('{é:1}')), hex(jsonb('{Ab:1}'));
SELECT json_valid('[QNaN]', 2), json_valid('[snan,-inf]', 2), json('[QNaN,SNaN]');
SELECT json('{null:1}');
-- case: bug-hunt-11/json/json5-line-comment-ends-at-carriage-return
SELECT json('[1,// c' || char(13) || '2]');
SELECT json('[1,// c' || char(10) || '2]');
SELECT json('[1,// c' || char(226) || char(128) || char(168) || '2]');
SELECT json_valid('[1,// c', 2);
-- case: bug-hunt-11/json/array-insert-creates-the-path
SELECT json_array_insert('{"a":[1,2]}', '$.b[0]', 'x');
SELECT json_array_insert('[[1],[2]]', '$[2][0]', 5);
SELECT json_array_insert('{"a":[1,2]}', '$.b.c[0]', 'x'), json_array_insert('{"a":[1,2]}', '$.b[1]', 'x'), json_array_insert('[1,2]', '$[#][0]', 'x'), json_array_insert('{}', '$.a[#]', 'x');
SELECT json_array_insert('[1,2,3]', '$[3]', 9), json_array_insert('[1,2,3]', '$[4]', 9), json_array_insert('{"a":[1]}', '$.a[#]', 9), json_array_insert('{"a":[1]}', '$.a[1]', 9);
-- case: bug-hunt-11/json/each-id-after-json5-and-tree-root-index
SELECT id FROM json_each('[0x10,+5,.5,5.,Infinity,-Infinity,NaN]');
SELECT * FROM json_tree('{"a":[1,2,3]}', '$.a[1]');
-- case: bug-hunt-11/json/error-position
SELECT json_error_position('["\u12"]');
SELECT json_error_position('{"a"":1}');
SELECT json_error_position('["' || char(0) || '"]');
SELECT json_error_position('{"a" 1}'), json_error_position('{"a"x:1}'), json_error_position('{"a"}'), json_error_position('{"a",1}'), json_error_position('["\x1"]'), json_error_position('["\q"]'), json_error_position('{"a" [1,'), json_error_position('{"a" "b":1}');
SELECT json_error_position('{"a":1,"b"":2}'), json_error_position('[{"a"":1}]'), json_error_position('{"a" @}');
-- case: bug-hunt-11/json/table-info-of-json-each-has-no-declared-types
SELECT name, type FROM pragma_table_info('json_each');
SELECT name, type, hidden FROM pragma_table_xinfo('json_tree');
SELECT count(*) FROM json_each('{"a":1}') WHERE fullkey < 0;
SELECT typeof(id), typeof(parent), id = '0' FROM json_each('[1]');
-- case: bug-hunt-11/json/function-list-flags-of-json-functions
SELECT name, narg, flags FROM pragma_function_list WHERE name LIKE 'json%' OR name IN ('->', '->>') ORDER BY name, narg;
-- case: bug-hunt-11/json/too-many-arguments-names-the-function
SELECT json_array(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255,256,257,258,259,260,261,262,263,264,265,266,267,268,269,270,271,272,273,274,275,276,277,278,279,280,281,282,283,284,285,286,287,288,289,290,291,292,293,294,295,296,297,298,299,300,301,302,303,304,305,306,307,308,309,310,311,312,313,314,315,316,317,318,319,320,321,322,323,324,325,326,327,328,329,330,331,332,333,334,335,336,337,338,339,340,341,342,343,344,345,346,347,348,349,350,351,352,353,354,355,356,357,358,359,360,361,362,363,364,365,366,367,368,369,370,371,372,373,374,375,376,377,378,379,380,381,382,383,384,385,386,387,388,389,390,391,392,393,394,395,396,397,398,399,400,401,402,403,404,405,406,407,408,409,410,411,412,413,414,415,416,417,418,419,420,421,422,423,424,425,426,427,428,429,430,431,432,433,434,435,436,437,438,439,440,441,442,443,444,445,446,447,448,449,450,451,452,453,454,455,456,457,458,459,460,461,462,463,464,465,466,467,468,469,470,471,472,473,474,475,476,477,478,479,480,481,482,483,484,485,486,487,488,489,490,491,492,493,494,495,496,497,498,499,500,501,502,503,504,505,506,507,508,509,510,511,512,513,514,515,516,517,518,519,520,521,522,523,524,525,526,527,528,529,530,531,532,533,534,535,536,537,538,539,540,541,542,543,544,545,546,547,548,549,550,551,552,553,554,555,556,557,558,559,560,561,562,563,564,565,566,567,568,569,570,571,572,573,574,575,576,577,578,579,580,581,582,583,584,585,586,587,588,589,590,591,592,593,594,595,596,597,598,599,600,601,602,603,604,605,606,607,608,609,610,611,612,613,614,615,616,617,618,619,620,621,622,623,624,625,626,627,628,629,630,631,632,633,634,635,636,637,638,639,640,641,642,643,644,645,646,647,648,649,650,651,652,653,654,655,656,657,658,659,660,661,662,663,664,665,666,667,668,669,670,671,672,673,674,675,676,677,678,679,680,681,682,683,684,685,686,687,688,689,690,691,692,693,694,695,696,697,698,699,700,701,702,703,704,705,706,707,708,709,710,711,712,713,714,715,716,717,718,719,720,721,722,723,724,725,726,727,728,729,730,731,732,733,734,735,736,737,738,739,740,741,742,743,744,745,746,747,748,749,750,751,752,753,754,755,756,757,758,759,760,761,762,763,764,765,766,767,768,769,770,771,772,773,774,775,776,777,778,779,780,781,782,783,784,785,786,787,788,789,790,791,792,793,794,795,796,797,798,799,800,801,802,803,804,805,806,807,808,809,810,811,812,813,814,815,816,817,818,819,820,821,822,823,824,825,826,827,828,829,830,831,832,833,834,835,836,837,838,839,840,841,842,843,844,845,846,847,848,849,850,851,852,853,854,855,856,857,858,859,860,861,862,863,864,865,866,867,868,869,870,871,872,873,874,875,876,877,878,879,880,881,882,883,884,885,886,887,888,889,890,891,892,893,894,895,896,897,898,899,900,901,902,903,904,905,906,907,908,909,910,911,912,913,914,915,916,917,918,919,920,921,922,923,924,925,926,927,928,929,930,931,932,933,934,935,936,937,938,939,940,941,942,943,944,945,946,947,948,949,950,951,952,953,954,955,956,957,958,959,960,961,962,963,964,965,966,967,968,969,970,971,972,973,974,975,976,977,978,979,980,981,982,983,984,985,986,987,988,989,990,991,992,993,994,995,996,997,998,999,1000,1001);
-- case: bug-hunt-11/json/constant-document-with-a-path-from-an-earlier-term
CREATE TABLE n(i); INSERT INTO n VALUES (0),(1),(2);
SELECT n.i, j.value FROM n, json_each('[[1],[2],[3]]', '$[' || n.i || ']') j;
SELECT s.value, j.value FROM generate_series(0,2) s, json_each('[[1],[2],[3]]', '$[' || s.value || ']') j;
SELECT n.i, j.value FROM n, json_tree('{"a":[[1],[2],[3]]}', '$.a[' || n.i || ']') j;
SELECT n.i, j.value FROM n, json_each('[[1],[2],[3]]', printf('$[%d]', n.i)) j;
SELECT n.i, j.value FROM n LEFT JOIN json_each('[[1],[2]]', '$[' || n.i || ']') j ORDER BY n.i;
-- case: bug-hunt-11/json/subtype-kept-for-a-group-by-term
CREATE TABLE t(a, b); INSERT INTO t VALUES (1, 'x'), (2, 'y');
SELECT json_array(json_object('k', b)) FROM t GROUP BY json_object('k', b);
SELECT json_array(json_quote(b)) FROM t GROUP BY json_quote(b);
SELECT json_object('k', b), count(*) FROM t GROUP BY json_object('k', b);
SELECT json_array(json_array(b, a)) FROM t GROUP BY json_array(b, a) ORDER BY 1;
