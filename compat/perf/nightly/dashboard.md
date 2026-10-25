# inillucent performance dashboard

Platform `windows-x86_64`. Every number is the paired speed ratio, SQLite over inillucent, so above one is faster than the reference. Columns are runs in the order they were taken.

## Scale `medium`

| workload | nightly-20260925-072550 | nightly-20260925-082330 | nightly-20260925-174312 | nightly-20261004-143426 | nightly-20261012-134052 | nightly-20261025-080002 |
|---|---:|---:|---:|---:|---:|---:|
| `*headline*` | 0.759x | 0.751x | 0.754x | 1.738x | 3.145x | 4.312x |
| `*family* open.prepare` | 0.332x | 0.330x | 0.330x | 1.120x | 1.684x | 2.330x |
| `*family* read.point` | 1.504x | 1.478x | 1.481x | 1.647x | 9.530x | 19.436x |
| `*family* read.range` | 0.907x | 0.903x | 0.917x | 1.543x | 3.156x | 5.162x |
| `*family* read.analytical` | 0.308x | 0.308x | 0.308x | 8.854x | 9.813x | 11.230x |
| `*family* read.join` | 0.636x | 0.636x | 0.635x | 1.150x | 2.057x | 2.975x |
| `*family* write` | 0.760x | 0.734x | 0.747x | 1.707x | 2.572x | 2.934x |
| `*family* transaction` | 0.765x | 0.739x | 0.774x | 2.053x | 2.064x | 2.694x |
| `*family* schema` | 1.443x | 1.572x | 1.425x | 0.583x | 0.540x | 0.626x |
| `*family* extension` | 0.568x | 0.546x | 0.532x | 0.838x | 1.111x | 1.145x |
| `*family* large.values` | 1.826x | 1.919x | 1.941x | 2.195x | 4.322x | 6.483x |
| `prepare.trivial` | 0.087x | 0.087x | 0.087x | 0.990x | 0.857x | 0.816x |
| `prepare.point` | 1.252x | 1.253x | 1.250x | 1.315x | 3.612x | 6.697x |
| `point.rowid` | 1.977x | 1.947x | 1.909x | 2.004x | 13.771x | 23.424x |
| `point.index` | 1.180x | 1.173x | 1.181x | 1.477x | 5.689x | 11.175x |
| `point.miss` | 1.441x | 1.426x | 1.441x | 1.446x | 12.249x | 27.051x |
| `range.covering` | 0.540x | 0.531x | 0.545x | 1.663x | 6.956x | 12.749x |
| `range.lookaside` | 0.894x | 0.894x | 0.909x | 1.191x | 1.465x | 1.842x |
| `range.reverse` | 1.529x | 1.553x | 1.561x | 1.654x | 3.490x | 5.559x |
| `scan.aggregate` | 0.402x | 0.404x | 0.401x | 36.534x | 36.771x | 41.125x |
| `scan.group` | 0.371x | 0.367x | 0.366x | 25.876x | 25.509x | 26.384x |
| `scan.sort` | 9.611x | 9.681x | 9.658x | 10.522x | 11.620x | 12.489x |
| `scan.distinct` | 0.006x | 0.006x | 0.006x | 0.654x | 0.841x | 1.108x |
| `join.selective` | 1.106x | 1.103x | 1.107x | 1.586x | 4.895x | 7.916x |
| `join.range` | 0.365x | 0.363x | 0.365x | 0.926x | 0.876x | 1.084x |
| `correlated.exists` | 0.074x | 0.074x | 0.074x | 0.119x | 0.159x | 0.184x |
| `correlated.in` | 0.060x | 0.051x | 0.062x | 0.570x | 0.698x | 0.665x |
| `correlated.exists.selective` | 0.308x | 0.302x | 0.322x | 0.456x | 0.602x | 0.650x |
| `correlated.scalar.selective` | 0.356x | 0.360x | 0.374x | 0.463x | 0.530x | 0.647x |
| `write.insert.batch` | 1.072x | 1.065x | 1.073x | 0.724x | 1.951x | 1.751x |
| `write.insert.autocommit` | 3.202x | 3.160x | 3.190x | 3.279x | 3.364x | 4.827x |
| `write.update.indexed` | 0.157x | 0.153x | 0.158x | 2.347x | 3.140x | 3.037x |
| `write.delete` | 0.519x | 0.514x | 0.520x | 2.421x | 3.853x | 3.846x |
| `write.upsert` | 1.104x | 1.082x | 1.067x | 1.147x | 1.548x | 1.597x |
| `txn.autocommit` | 0.870x | 0.857x | 0.868x | 0.963x | 0.938x | 1.177x |
| `txn.batched` | 3.205x | 3.177x | 3.227x | 3.334x | 3.333x | 4.770x |
| `txn.large` | 0.162x | 0.160x | 0.166x | 2.901x | 2.958x | 3.174x |
| `schema.index` | 1.442x | 1.473x | 1.428x | 0.555x | 0.567x | 0.580x |
| `extension.json` | 0.059x | 0.059x | 0.059x | 0.777x | 0.788x | 0.749x |
| `extension.fts.build` | 1.058x | 1.038x | 1.047x | 0.885x | 1.217x | 0.842x |
| `extension.fts.query` | 0.496x | 0.493x | 0.498x | 0.461x | 0.568x | 0.675x |
| `extension.rtree.insert` | 2.002x | 1.948x | 1.923x | 1.691x | 2.029x | 1.589x |
| `extension.rtree.query` | 0.918x | 0.904x | 0.909x | 0.840x | 1.736x | 3.119x |
| `large.read` | 1.211x | 1.195x | 1.209x | 1.242x | 6.191x | 12.577x |
| `large.write` | 2.976x | 3.243x | 3.296x | 3.492x | 3.446x | 3.352x |
| `correlated.exists.expression` | - | - | - | - | - | 0.914x |
| `correlated.scalar.expression` | - | - | - | - | - | 39.176x |

## Regressions

| scale | workload | arm | best lower bound | now | runs |
|---|---|---|---:|---:|---|
| `medium` | `*family* schema` | shipped | 1.457x | 0.484x | nightly-20261012-134052, nightly-20261025-080002 |
| `medium` | `extension.rtree.insert` | shipped | 1.833x | 1.073x | nightly-20261012-134052, nightly-20261025-080002 |
| `medium` | `prepare.trivial` | shipped | 0.953x | 0.742x | nightly-20261012-134052, nightly-20261025-080002 |
| `medium` | `schema.index` | shipped | 1.457x | 0.484x | nightly-20261012-134052, nightly-20261025-080002 |
