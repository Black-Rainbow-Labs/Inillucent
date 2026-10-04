# inillucent performance scorecard

Label `nightly-20261004-143426`, platform `windows-x86_64`, 30 paired rounds per scale, bootstrap seed 17900001. Every optimization is on, which is the shipped engine.

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

Weighted geometric mean **1.738x**, 95% interval [1.638, 1.807]. The release bound is a lower bound of at least 3.00x.

### By family

| family | weight | ratio | 95% interval | verdict | required floor |
|---|---:|---:|---|---|---|
| `open.prepare` | 0.08 | 1.120x | [1.021, 1.239] | inconclusive | met |
| `read.point` | 0.16 | 1.647x | [1.534, 1.765] | win | met |
| `read.range` | 0.12 | 1.543x | [1.451, 1.655] | win | met |
| `read.analytical` | 0.10 | 8.854x | [8.023, 9.739] | win | met |
| `read.join` | 0.08 | 1.150x | [0.937, 1.383] | inconclusive | **below 1.00x** |
| `write` | 0.20 | 1.707x | [1.588, 1.829] | win | met |
| `transaction` | 0.10 | 2.053x | [1.826, 2.248] | win | met |
| `schema` | 0.04 | 0.583x | [0.496, 0.695] | loss | **below 1.00x** |
| `extension` | 0.08 | 0.838x | [0.780, 0.896] | loss | **below 1.00x** |
| `large.values` | 0.04 | 2.195x | [1.796, 2.732] | win | met |

### By workload

| workload | family | inillucent median | SQLite median | ratio | 95% interval | samples |
|---|---|---:|---:|---:|---|---:|
| `prepare.trivial` | `open.prepare` | 2.57 ms | 2.60 ms | 0.990x | [0.953, 1.053] | 30 |
| `prepare.point` | `open.prepare` | 67.92 ms | 82.45 ms | 1.315x | [1.051, 1.512] | 30 |
| `point.rowid` | `read.point` | 34.66 ms | 67.12 ms | 2.004x | [1.884, 2.230] | 30 |
| `point.index` | `read.point` | 40.73 ms | 56.39 ms | 1.477x | [1.312, 1.557] | 30 |
| `point.miss` | `read.point` | 31.75 ms | 47.63 ms | 1.446x | [1.404, 1.668] | 30 |
| `range.covering` | `read.range` | 11.58 ms | 18.61 ms | 1.663x | [1.585, 1.823] | 30 |
| `range.lookaside` | `read.range` | 49.43 ms | 59.71 ms | 1.191x | [1.145, 1.318] | 30 |
| `range.reverse` | `read.range` | 15.15 ms | 25.26 ms | 1.654x | [1.604, 1.975] | 30 |
| `scan.aggregate` | `read.analytical` | 4.32 ms | 160.93 ms | 36.534x | [24.819, 39.205] | 30 |
| `scan.group` | `read.analytical` | 5.31 ms | 137.93 ms | 25.876x | [22.266, 26.925] | 30 |
| `scan.sort` | `read.analytical` | 45.32 ms | 488.00 ms | 10.522x | [10.157, 11.944] | 30 |
| `scan.distinct` | `read.analytical` | 2.55 ms | 1.58 ms | 0.654x | [0.621, 0.826] | 30 |
| `join.selective` | `read.join` | 24.35 ms | 36.73 ms | 1.586x | [1.235, 1.808] | 30 |
| `join.range` | `read.join` | 46.79 ms | 43.19 ms | 0.926x | [0.712, 1.058] | 30 |
| `correlated.exists` | `read.correlated` | 2.42 ms | 280.80 us | 0.119x | [0.073, 0.242] | 30 |
| `correlated.in` | `read.correlated` | 270.80 us | 156.60 us | 0.570x | [0.493, 0.617] | 30 |
| `correlated.exists.selective` | `read.correlated` | 62.25 us | 28.10 us | 0.456x | [0.398, 0.516] | 30 |
| `correlated.scalar.selective` | `read.correlated` | 58.75 us | 27.05 us | 0.463x | [0.402, 0.490] | 30 |
| `write.insert.batch` | `write` | 20.62 ms | 16.30 ms | 0.724x | [0.601, 0.849] | 30 |
| `write.insert.autocommit` | `write` | 152.70 ms | 510.88 ms | 3.279x | [2.661, 3.454] | 30 |
| `write.update.indexed` | `write` | 57.62 ms | 137.42 ms | 2.347x | [2.271, 2.588] | 30 |
| `write.delete` | `write` | 57.35 ms | 136.62 ms | 2.421x | [2.208, 2.498] | 30 |
| `write.upsert` | `write` | 7.42 ms | 8.44 ms | 1.147x | [1.054, 1.343] | 30 |
| `txn.autocommit` | `transaction` | 150.39 ms | 145.49 ms | 0.963x | [0.893, 1.215] | 30 |
| `txn.batched` | `transaction` | 319.24 ms | 1.05 s | 3.334x | [2.822, 3.460] | 30 |
| `txn.large` | `transaction` | 4.36 ms | 12.39 ms | 2.901x | [2.196, 3.033] | 30 |
| `schema.index` | `schema` | 134.21 ms | 74.58 ms | 0.555x | [0.496, 0.695] | 30 |
| `extension.json` | `extension` | 2.29 ms | 1.80 ms | 0.777x | [0.662, 0.796] | 30 |
| `extension.fts.build` | `extension` | 8.04 ms | 6.75 ms | 0.885x | [0.731, 0.911] | 30 |
| `extension.fts.query` | `extension` | 31.18 ms | 15.24 ms | 0.461x | [0.415, 0.509] | 30 |
| `extension.rtree.insert` | `extension` | 4.24 ms | 6.83 ms | 1.691x | [1.530, 2.193] | 30 |
| `extension.rtree.query` | `extension` | 7.91 ms | 6.75 ms | 0.840x | [0.767, 0.925] | 30 |
| `large.read` | `large.values` | 17.82 ms | 22.75 ms | 1.242x | [1.121, 1.319] | 30 |
| `large.write` | `large.values` | 1.77 ms | 6.01 ms | 3.492x | [2.722, 6.064] | 30 |

