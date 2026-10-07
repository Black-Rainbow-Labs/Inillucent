# What is not built yet

This page lists the open work on inillucent, in the order it is being worked. Each item says what
the work is, why a user would want it, and where it stands today. Work that is finished, and work
that was decided against, is in [Closed items](closed-items.md).

Every speed ratio on this page comes from the runs of 7 October 2026 in
[Performance](performance.md). A ratio is SQLite's time divided by inillucent's time on the same
workload. A ratio above 1.00x means inillucent was faster, and one below 1.00x means inillucent was
slower.

## Terms used on this page

| Term | Meaning |
|---|---|
| workload | one timed task in the benchmark against SQLite, such as `join.range` |
| family | a group of workloads that is graded together, such as `read.join` |
| bar | the ratio a family has to reach before the performance contract counts it as met |
| lower bound | the low end of the 95% confidence interval for a family's ratio. The contract grades a family on its lower bound |
| resident memory | the memory a process holds in RAM |
| [HNSW](glossary.md) | the graph index that vector search walks |
| [BM25](glossary.md) | the scoring method keyword search uses. Its index is a list of postings for each term |
| driver | `inillucent-driver`, the Rust library every application and language binding calls |
| wire protocol | the bytes a database client and server send each other over the network |

## The list

| # | Item | Why a user wants it | Status |
|---|---|---|---|
| 1 | Workloads that are slower than SQLite | faster statement preparation and index builds | one weighted workload is still slower than SQLite; `open.prepare` and `schema` miss their bars |
| 2 | Memory held by a retrieval index | a large search index that fits in a small machine's memory | the postings and the chunk text are smaller in memory; the graph is unchanged; the 512 MiB goal is not yet measured |
| 3 | Two lines of the command line that bypass the driver | one code path from every program to the engine | 28 lines are down to 2 |
| 4 | PostgreSQL parity | a server that PostgreSQL clients can connect to | designed, not built |

<a id="1-the-extension-and-join-families-either-side-of-their-bars"></a>

## 1. Workloads that are slower than SQLite

**What it is.** The benchmark against SQLite times 30 weighted workloads. inillucent is faster on
most of them. Some workloads are still slower, and they hold down the families they belong to.

**Why a user wants it.** These workloads are common operations: a join over an index range, building
a full text index, and compiling a short statement.

**Status.** Measured 7 October 2026, the median of three runs:

| Workload | Family | Workload ratio | Family ratio |
|---|---|---:|---:|
| `prepare.trivial` | `open.prepare` | 0.58x | 2.02x |
| `schema.index` | `schema` | 1.48x | 1.47x |

- `prepare.trivial` is the one weighted workload slower than SQLite, 72% slower. It compiles
  `SELECT 1` on every call. `open.prepare` misses its bar of 5.00x, at 2.02x.
- `schema` misses its bar of 3.00x, at 1.47x. `schema` is one workload, `schema.index`, run once a
  round.
- `join.range` (1.02x) and `extension.fts.build` (1.14x), both slower than SQLite on 23 September,
  are now faster. `read.join` is at 5.26x against its bar of 3.00x and `extension` at 2.47x against
  1.50x.

[The workloads that are slower](performance.md#the-workloads-that-are-slower) has the time each
workload takes and the reason. [By family](performance.md#by-family) has every family's ratio, lower
bounds and bar.

## 2. Memory held by a retrieval index

**What it is.** A retrieval index has four parts: the BM25 postings (`lexical.bin`), the chunk text
and its dictionaries (`store.bin`), the HNSW graph (`graph.bin`) and the vectors (`vectors.bin`). The
vectors are already read from the file when a search needs them. The goal is to read the other
parts from the file the same way.

**Why a user wants it.** A process that opens a large index should not need the whole index in
memory. The goal is under 512 MiB of resident memory at the default buffer pool size, with the same
top results as today, a median search time within 1.5x of today's, and a 99th percentile search time
within 2x.

**Status.** `inillucent-indexresidency` measures what each part of an index costs in memory. On 15
September 2026 it measured an index of 600,589 chunks at 768 dimensions, with 1,705,097 terms and
the vectors left in the file:

| Part | On disk, MiB | Resident, MiB | Share of resident |
|---|---:|---:|---:|
| `lexical.bin`, the BM25 postings | 614.8 | 890.3 | 53% |
| `store.bin`, the chunks and their dictionaries | 564.8 | 620.3 | 37% |
| `graph.bin`, the HNSW graph | 87.7 | 154.2 | 9% |
| `vectors.bin` | 1,759.5 | 0.0 | none |
| total | 3,026.9 | 1,664.9 | |

The postings are the largest part. `store.bin` alone is over 512 MiB, so the goal needs
`store.bin` read from the file as well.

Two changes since that measurement:

- **The postings are stored as flat arrays.** Before, each term had its own heap block, a string in
  a hash map and a second string in a sorted list. Now there is one byte array for all the terms,
  one array for all the postings, and a binary search to find a term.
- **The chunk text is read from the file** when a result needs it, the way the vectors already were.
  An index written by an older build still opens, because neither change alters the file format.

On 22 September 2026, `inillucent-indexresidency --through-load` measured a smaller index of 185,078
chunks and 494,293 terms. After both changes, a process that opens that index holds 424.2 MiB. It
held 677.0 MiB before, so the two changes saved 37.3%.

The goal is not met yet:

- The 424.2 MiB figure is for an index one third the size of the 600,589 chunk index. The large
  index has not been measured since the changes.
- The HNSW graph is still read into memory whole.
- The search times and the top results have not been measured again since the changes.

## 3. Two lines of the command line that bypass the driver

**What it is.** Applications, the C library and the language bindings reach the engine through the
driver, `inillucent-driver`. The command line programs used to import the engine crate
`inillucent_engine` directly in 28 lines across five files. The driver gained the functions those
lines needed, and 2 lines are left:

| File | Lines that import the engine |
|---|---:|
| `crates/inillucent-cli/src/shell.rs` | 1 |
| `crates/inillucent-cli/src/import.rs` | 1 |

`no_shell_file_reaches_past_the_driver_more_than_it_is_recorded_at` in
`crates/inillucent-compat/tests/tooling/policy.rs` records the count for each file. The test fails if a
count goes up.

**Why a user wants it.** When every program uses the driver, a fix in the driver reaches every
program. [The driver documentation](../drivers/README.md) says the driver is the one way an
application reaches the engine. That sentence is true for applications, and it becomes true for this
repository when both counts are zero.

**Status.** Both lines use the engine's `Connection` and `Database` types. The shell prints rows
with the engine's value type, `inillucent_value::Value`. The driver returns rows with a different
value type, `inillucent_driver::Value`. Removing the last two lines needs a decision about which
value type the shell uses. One choice converts every printed cell. The other rewrites the shell's
renderer, whose output for 63 dot commands is compared line by line with the `sqlite3` shell. That
decision has not been made.

## 4. PostgreSQL parity

**What it is.** A set of features that would let inillucent stand in for a PostgreSQL server. None of
them is built. inillucent has no network listener, no replica, no roles and no passwords. A reader in
another process already reads the last committed state while a write transaction runs; item 3
below is a snapshot a reader keeps for a whole transaction, inside a server. A PostgreSQL client has nothing to connect to.

**Why a user wants it.** Existing tools such as `psql` and `pg_dump` would work against inillucent,
several machines could share one database, and a second machine could hold a current copy.

**Status.** Designed in
[the path from an embedded engine to PostgreSQL parity](../tasks/task-1998-postgres-parity-tdd.md).
The work is planned in this order:

1. A server that runs as a service and speaks the PostgreSQL wire protocol.
2. A primary server with a replica, fed from the redo log.
3. Readers that see a snapshot while the one writer works.
4. Roles and row level security policies.
5. The PostgreSQL dialect of SQL.
6. The operational commands.

The design records two measurements. With both engines syncing every commit, one inillucent writer
commits 38 single rows a second, and PostgreSQL commits 2,837. The default journal mode syncs three
times for each commit. A probe of 174 statements, one for each PostgreSQL feature, has 59 accepted.
The probe lists the tokens, types, functions and catalog tables the dialect step has to add.

The work is done when `psql`, the `postgres` library for Node and `pg_dump` work against the server
unchanged, and a second server holds a copy of the data that stays current.

## Not on this list

- **Statements running in parallel.** One database can be shared by many threads through
  `SharedDatabase` in `inillucent-driver`, and many processes can open the same file. One statement
  runs at a time. A parallel executor is not planned. [The architecture
  overview](architecture-overview.md#threads) explains why.
- **SQL constructs the engine refuses.** `inillucent capabilities` lists them. A row marked `no` is a
  construct the engine refuses with exit code 3, and the row's note gives the reason.

## Where to go next

- [Closed items](closed-items.md): work that came off this list, and the measurement that closed it
- [Performance](performance.md): the measurements behind items 1 and 2
- [Feature comparison](feature-comparison.md): the full benchmark run, for each workload
- [Repository](repository.md): the crates and the test runner
