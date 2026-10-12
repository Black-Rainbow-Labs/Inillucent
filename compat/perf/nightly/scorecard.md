# inillucent performance scorecard

Label `nightly-20261012-134052`, platform `windows-x86_64`, 30 paired rounds per scale, bootstrap seed 17900001. Every optimization is on, which is the shipped engine.

Both engines read the same plan file. The ratio is SQLite over inillucent, so **above one means inillucent is faster**. A workload whose two engines returned different answers is reported as a correctness failure and is not timed.

## Fair configuration

| setting | value |
|---|---|
| journal mode | `delete` |
| synchronous | `full` |
| page size | 4096 |
| cache | -2000 in SQLite's units: positive is pages, negative is KiB |
| statement reuse | prepared once except the `open.prepare` family |
| database | on disk, cloned from one pristine image per round |

**On memory, which is the setting most easily got wrong.** This scorecard measures the bytecode engine, whose page cache and SQLite's are both governed by the `cache_size` above, so the two arms are given the same memory by construction.

That is *not* automatic for the vectorised engine, and the Phase 1 numbers were inflated because it was not: the prototype's trees were fully resident while SQLite ran at the plan's 2 MB cache. Phase 2's gate (`inillucent-readgate`) closes it by deriving SQLite's `cache_size` from the byte size of inillucent's own buffer pool, so `--frames` moves both sides together and neither engine can be given memory the other is not. The gate prints both figures before it times anything. A ratio measured without that is a ratio between two different machines.

## Scale `medium` - 100000 rows

Weighted geometric mean **3.145x**, 95% interval [3.021, 3.166]. The release bound is a lower bound of at least 3.00x.

### By family

| family | weight | ratio | 95% interval | verdict | required floor |
|---|---:|---:|---|---|---|
| `open.prepare` | 0.08 | 1.684x | [1.626, 1.744] | win | met |
| `read.point` | 0.16 | 9.530x | [9.103, 9.911] | win | met |
| `read.range` | 0.12 | 3.156x | [3.001, 3.291] | win | met |
| `read.analytical` | 0.10 | 9.813x | [9.620, 9.985] | win | met |
| `read.join` | 0.08 | 2.057x | [2.005, 2.107] | win | met |
| `write` | 0.20 | 2.572x | [2.471, 2.682] | win | met |
| `transaction` | 0.10 | 2.064x | [1.872, 2.239] | win | met |
| `schema` | 0.04 | 0.540x | [0.481, 0.588] | loss | **below 1.00x** |
| `extension` | 0.08 | 1.111x | [1.051, 1.175] | inconclusive | met |
| `large.values` | 0.04 | 4.322x | [3.848, 4.666] | win | met |

### By workload

| workload | family | inillucent median | SQLite median | ratio | 95% interval | samples |
|---|---|---:|---:|---:|---|---:|
| `prepare.trivial` | `open.prepare` | 1.98 ms | 1.62 ms | 0.857x | [0.752, 0.847] | 30 |
| `prepare.point` | `open.prepare` | 13.96 ms | 50.02 ms | 3.612x | [3.447, 3.651] | 30 |
| `point.rowid` | `read.point` | 3.16 ms | 43.52 ms | 13.771x | [11.955, 13.942] | 30 |
| `point.index` | `read.point` | 5.71 ms | 32.26 ms | 5.689x | [5.341, 5.749] | 30 |
| `point.miss` | `read.point` | 2.26 ms | 28.35 ms | 12.249x | [11.574, 12.408] | 30 |
| `range.covering` | `read.range` | 1.70 ms | 11.87 ms | 6.956x | [5.838, 6.875] | 30 |
| `range.lookaside` | `read.range` | 25.81 ms | 38.29 ms | 1.465x | [1.360, 1.507] | 30 |
| `range.reverse` | `read.range` | 4.63 ms | 15.82 ms | 3.490x | [3.290, 3.547] | 30 |
| `scan.aggregate` | `read.analytical` | 2.58 ms | 94.44 ms | 36.771x | [35.860, 37.350] | 30 |
| `scan.group` | `read.analytical` | 3.07 ms | 78.93 ms | 25.509x | [24.513, 25.954] | 30 |
| `scan.sort` | `read.analytical` | 26.40 ms | 310.22 ms | 11.620x | [11.477, 11.843] | 30 |
| `scan.distinct` | `read.analytical` | 1.26 ms | 1.08 ms | 0.841x | [0.818, 0.906] | 30 |
| `join.selective` | `read.join` | 4.71 ms | 22.76 ms | 4.895x | [4.667, 4.997] | 30 |
| `join.range` | `read.join` | 27.86 ms | 24.30 ms | 0.876x | [0.853, 0.899] | 30 |
| `correlated.exists` | `read.correlated` | 1.14 ms | 182.00 us | 0.159x | [0.155, 0.172] | 30 |
| `correlated.in` | `read.correlated` | 156.95 us | 109.40 us | 0.698x | [0.620, 0.737] | 30 |
| `correlated.exists.selective` | `read.correlated` | 31.05 us | 18.55 us | 0.602x | [0.576, 0.672] | 30 |
| `correlated.scalar.selective` | `read.correlated` | 31.10 us | 16.25 us | 0.530x | [0.499, 0.544] | 30 |
| `write.insert.batch` | `write` | 10.08 ms | 20.00 ms | 1.951x | [1.414, 1.839] | 30 |
| `write.insert.autocommit` | `write` | 140.33 ms | 470.66 ms | 3.364x | [3.263, 3.658] | 30 |
| `write.update.indexed` | `write` | 36.32 ms | 112.52 ms | 3.140x | [3.040, 3.754] | 30 |
| `write.delete` | `write` | 29.08 ms | 112.22 ms | 3.853x | [3.757, 4.484] | 30 |
| `write.upsert` | `write` | 4.46 ms | 7.28 ms | 1.548x | [1.444, 1.601] | 30 |
| `txn.autocommit` | `transaction` | 140.37 ms | 133.74 ms | 0.938x | [0.936, 1.173] | 30 |
| `txn.batched` | `transaction` | 292.49 ms | 967.52 ms | 3.333x | [2.906, 3.533] | 30 |
| `txn.large` | `transaction` | 3.42 ms | 10.15 ms | 2.958x | [2.159, 3.046] | 30 |
| `schema.index` | `schema` | 89.98 ms | 50.24 ms | 0.567x | [0.481, 0.588] | 30 |
| `extension.json` | `extension` | 1.47 ms | 1.15 ms | 0.788x | [0.758, 0.905] | 30 |
| `extension.fts.build` | `extension` | 4.88 ms | 5.98 ms | 1.217x | [1.106, 1.281] | 30 |
| `extension.fts.query` | `extension` | 16.76 ms | 9.62 ms | 0.568x | [0.554, 0.602] | 30 |
| `extension.rtree.insert` | `extension` | 2.89 ms | 5.89 ms | 2.029x | [1.502, 1.997] | 30 |
| `extension.rtree.query` | `extension` | 2.51 ms | 4.46 ms | 1.736x | [1.630, 1.753] | 30 |
| `large.read` | `large.values` | 2.31 ms | 14.44 ms | 6.191x | [6.105, 6.458] | 30 |
| `large.write` | `large.values` | 1.51 ms | 5.33 ms | 3.446x | [2.374, 3.449] | 30 |

