#!/usr/bin/env node
// Writes the nested join cases of compat/corpus/research/nested-joins.sql to stdout.
//
// The cases cover every pair of join kinds, with the inner pair in
// parentheses on the left or the right, ON forms that read the inner and the
// outer tables, and a WHERE that may turn an outer join into an inner one.
// 2.2.0 and 2.3.1 answer 18 of these cases wrongly; the bug hunt of October
// 2026 fixed them.
//
//   node tools/bug-hunt/nested-joins.mjs > nested.sql   (then add the header)
const kinds = ['JOIN', 'LEFT JOIN', 'RIGHT JOIN', 'FULL JOIN', 'CROSS JOIN'];
const setup = [
  'CREATE TABLE a(k INT, v INT);',
  'CREATE TABLE b(k INT, v INT);',
  'CREATE TABLE c(k INT, v INT);',
  'INSERT INTO a VALUES(1,10),(2,20),(2,21),(NULL,30),(5,50);',
  'INSERT INTO b VALUES(2,200),(3,300),(NULL,310),(5,500);',
  'INSERT INTO c VALUES(1,1000),(3,3000),(5,5000),(6,6000);',
  'CREATE INDEX bk ON b(k);',
].join('\n');
const cols = 'a.k AS ak, a.v AS av, b.k AS bk, b.v AS bv, c.k AS ck, c.v AS cv';
const order = 'ORDER BY 1 NULLS FIRST, 2 NULLS FIRST, 3 NULLS FIRST, 4 NULLS FIRST, 5 NULLS FIRST, 6 NULLS FIRST';
const on = (kind, cond) => (kind === 'CROSS JOIN' ? '' : ` ON ${cond}`);
const wheres = ['', ' WHERE c.k IS NOT NULL', ' WHERE a.v > 15 OR a.v IS NULL', ' WHERE b.k IS NULL'];
const outerConds = ['a.k = c.k', 'b.k = c.k', 'coalesce(a.k, b.k) = c.k'];
let n = 0;
const out = [];
for (const inner of kinds) {
  for (const outer of kinds) {
    for (const oc of outerConds) {
      for (const w of wheres) {
        const left = `(a ${inner} b${on(inner, 'a.k = b.k')}) ${outer} c${on(outer, oc)}`;
        const right = `c ${outer} (a ${inner} b${on(inner, 'a.k = b.k')})${on(outer, oc)}`;
        for (const from of [left, right]) {
          n += 1;
          out.push(`-- case: research/nested-joins/${n}\n${setup}\nSELECT ${cols} FROM ${from}${w} ${order};\nSELECT count(*) FROM ${from}${w};\n`);
        }
        if (outer === 'CROSS JOIN') break;
      }
      if (outer === 'CROSS JOIN') break;
    }
  }
}
process.stdout.write(out.join('\n'));
