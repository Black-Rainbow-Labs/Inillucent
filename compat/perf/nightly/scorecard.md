# inillucent performance scorecard

Label `nightly-20261025-080002`, platform `windows-x86_64`, 30 paired rounds per scale, bootstrap seed 17900001. Every optimization is on, which is the shipped engine.

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

Weighted geometric mean **4.312x**, 95% interval [4.070, 4.450]. The release bound is a lower bound of at least 3.00x.

### By family

| family | weight | ratio | 95% interval | verdict | required floor |
|---|---:|---:|---|---|---|
| `open.prepare` | 0.08 | 2.330x | [2.197, 2.474] | win | met |
| `read.point` | 0.16 | 19.436x | [18.199, 20.808] | win | met |
| `read.range` | 0.12 | 5.162x | [4.855, 5.513] | win | met |
| `read.analytical` | 0.10 | 11.230x | [10.596, 11.967] | win | met |
| `read.join` | 0.08 | 2.975x | [2.861, 3.104] | win | met |
| `write` | 0.20 | 2.934x | [2.470, 3.433] | win | met |
| `transaction` | 0.10 | 2.694x | [2.499, 2.900] | win | met |
| `schema` | 0.04 | 0.626x | [0.484, 0.836] | loss | **below 1.00x** |
| `extension` | 0.08 | 1.145x | [1.000, 1.271] | inconclusive | met |
| `large.values` | 0.04 | 6.483x | [6.106, 6.936] | win | met |

### By workload

| workload | family | inillucent median | SQLite median | ratio | 95% interval | samples |
|---|---|---:|---:|---:|---|---:|
| `prepare.trivial` | `open.prepare` | 2.49 ms | 2.08 ms | 0.816x | [0.742, 0.868] | 30 |
| `prepare.point` | `open.prepare` | 17.18 ms | 114.88 ms | 6.697x | [6.391, 7.185] | 30 |
| `point.rowid` | `read.point` | 4.56 ms | 107.08 ms | 23.424x | [22.411, 27.552] | 30 |
| `point.index` | `read.point` | 7.85 ms | 87.27 ms | 11.175x | [10.105, 11.522] | 30 |
| `point.miss` | `read.point` | 3.00 ms | 82.72 ms | 27.051x | [25.649, 29.852] | 30 |
| `range.covering` | `read.range` | 2.16 ms | 27.22 ms | 12.749x | [12.185, 14.537] | 30 |
| `range.lookaside` | `read.range` | 36.54 ms | 64.27 ms | 1.842x | [1.670, 1.922] | 30 |
| `range.reverse` | `read.range` | 6.24 ms | 35.78 ms | 5.559x | [5.437, 6.226] | 30 |
| `scan.aggregate` | `read.analytical` | 3.30 ms | 131.95 ms | 41.125x | [37.573, 43.142] | 30 |
| `scan.group` | `read.analytical` | 4.24 ms | 112.54 ms | 26.384x | [25.849, 29.372] | 30 |
| `scan.sort` | `read.analytical` | 36.31 ms | 459.87 ms | 12.489x | [12.096, 14.335] | 30 |
| `scan.distinct` | `read.analytical` | 1.75 ms | 1.93 ms | 1.108x | [1.007, 1.207] | 30 |
| `join.selective` | `read.join` | 6.74 ms | 53.36 ms | 7.916x | [7.695, 8.438] | 30 |
| `join.range` | `read.join` | 38.22 ms | 41.90 ms | 1.084x | [1.054, 1.153] | 30 |
| `correlated.exists` | `read.correlated` | 1.44 ms | 258.50 us | 0.184x | [0.176, 0.201] | 30 |
| `correlated.in` | `read.correlated` | 233.40 us | 164.60 us | 0.665x | [0.591, 0.763] | 30 |
| `correlated.exists.selective` | `read.correlated` | 53.20 us | 34.10 us | 0.650x | [0.546, 0.732] | 30 |
| `correlated.scalar.selective` | `read.correlated` | 46.55 us | 29.10 us | 0.647x | [0.606, 0.819] | 30 |
| `correlated.exists.expression` | `read.correlated` | 13.47 ms | 12.07 ms | 0.914x | [0.881, 0.974] | 30 |
| `correlated.scalar.expression` | `read.correlated` | 12.87 ms | 495.96 ms | 39.176x | [36.768, 41.423] | 30 |
| `write.insert.batch` | `write` | 14.25 ms | 23.84 ms | 1.751x | [1.313, 4.646] | 30 |
| `write.insert.autocommit` | `write` | 123.77 ms | 600.14 ms | 4.827x | [3.392, 6.130] | 30 |
| `write.update.indexed` | `write` | 48.33 ms | 149.71 ms | 3.037x | [2.930, 3.309] | 30 |
| `write.delete` | `write` | 39.51 ms | 152.57 ms | 3.846x | [3.597, 4.186] | 30 |
| `write.upsert` | `write` | 5.84 ms | 8.99 ms | 1.597x | [1.478, 1.748] | 30 |
| `txn.autocommit` | `transaction` | 125.54 ms | 151.92 ms | 1.177x | [0.923, 1.254] | 30 |
| `txn.batched` | `transaction` | 256.51 ms | 1.23 s | 4.770x | [4.708, 6.587] | 30 |
| `txn.large` | `transaction` | 4.05 ms | 12.79 ms | 3.174x | [2.942, 3.372] | 30 |
| `schema.index` | `schema` | 108.40 ms | 67.09 ms | 0.580x | [0.484, 0.836] | 30 |
| `extension.json` | `extension` | 1.94 ms | 1.47 ms | 0.749x | [0.693, 0.804] | 30 |
| `extension.fts.build` | `extension` | 7.11 ms | 6.04 ms | 0.842x | [0.673, 1.162] | 30 |
| `extension.fts.query` | `extension` | 27.58 ms | 18.10 ms | 0.675x | [0.621, 0.712] | 30 |
| `extension.rtree.insert` | `extension` | 3.67 ms | 5.84 ms | 1.589x | [1.073, 1.762] | 30 |
| `extension.rtree.query` | `extension` | 3.52 ms | 10.81 ms | 3.119x | [2.857, 3.172] | 30 |
| `large.read` | `large.values` | 3.12 ms | 39.63 ms | 12.577x | [11.427, 13.918] | 30 |
| `large.write` | `large.values` | 1.60 ms | 5.37 ms | 3.352x | [3.221, 3.496] | 30 |

