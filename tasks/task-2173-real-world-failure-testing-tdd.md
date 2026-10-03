# Testing the ways real programs break a database file

## Terms used in this document

| Term | Meaning |
|---|---|
| worker | a process that commits seeded transactions to a database and prints `ACK <seq>` after each commit |
| reader | a process that opens read transactions on the same database and checks invariants inside each one |
| kill | `Child::kill`: `TerminateProcess` on Windows, `SIGKILL` on Unix. No destructor runs and no buffer is flushed |
| fold | writing the pages the log describes into the database file, under the EXCLUSIVE lock. A checkpoint is a fold somebody asked for |
| resync | what a connection does when it takes the lock and finds that another process moved the meta record or the log: it drops its cached pages and replays the log from the file's checkpoint |
| spill | writing a dirty page that does not fit in the buffer pool to the connection's own temporary file, so the database file is not written without EXCLUSIVE |
| storm | a run of several workers and readers on one file, killed and restarted at moments chosen by a seed |

## 1. Why

The bugs users reported this sprint all came from programs using the database the way real programs
do, and none of them came from SQL:

| Ticket | What a user saw | What caused it |
|---|---|---|
| task-2150 | a page read back as zeros, every request failed with a checksum error | a second process put a live writer's rollback journal back over the file |
| task-2161 | GPU memory not returned after unloading a model | the CUDA context, documented |
| task-2166 | a 3.5 GB database damaged twice while one process wrote and another read | a journal outlived the writer's lock and a new process replayed it |
| task-2167 | facet filters dropped rows, `$N` bound in the wrong order, a crash at exit | SQL and binding defects found by an application |
| task-2169 | readers blocked by large writes, damage after `SIGTERM` following a `ROLLBACK` | eviction needed EXCLUSIVE, a rollback left page splits no replay knew |

The tree already has about thirty durability suites. Each tests one sequence that somebody wrote
down after a bug, and each new bug was a sequence nobody had written down. This ticket adds tests
that produce sequences nobody wrote down: several real processes, seeded, killed at random moments,
graded by a checker that knows what every acknowledged transaction wrote. It also adds the targeted
cases that other embedded databases have documented as the usual causes of damage.

## 2. What exists, and what was missing

A read of every durability suite (the inventory is kept with this ticket's evidence) found these
covered well:

- a crash at every injectable call of a commit, a checkpoint, a vacuum, an index build, a search
  table commit, an overflow chain and an encrypted commit, on the simulated file system (`SimVfs`);
- real kills of one writer under `locking_mode = exclusive`, at 20 and 40 seeded points;
- two writer processes inserting, four readers beside one writer, a reader beside a transaction
  larger than the cache, a second process opening a held file;
- a misdirected write, a lying sync, a torn page, a truncated file, random damage to SQLite files.

These were missing:

| Scenario | Before this ticket |
|---|---|
| more than two writer processes, with readers, at once | not found |
| a kill under `locking_mode = normal` | every real kill was under `exclusive` |
| a reader holding a read transaction open while writers commit and fold | not found across processes |
| a kill of a reader, or of a process holding only SHARED | not found |
| `inillucent_search` written by one process while another searches, and killed during it | simulated only |
| an encrypted database with several processes and kills | simulated only |
| a checkpoint, backup or export called on the handle between statements, after another process committed | not found |
| a file copied while a writer is active, then opened | not found |
| a rollback journal or a log segment deleted after a crash | the log moved aside, asserting only "fewer rows" |
| a log segment of another database placed beside the file | covered in `inillucent-wal` only |
| a file extended with zeros, or damaged at every page, then checked and dumped | partly |
| disk full in a live connection, then space freed and the connection used again | not found |
| a read only file or a read only directory | not found |
| one file opened through two different paths | not found |
| a path with non ASCII characters, or longer than 260 characters | not found |

## 3. What other databases learned

A survey of SQLite's "How To Corrupt An SQLite Database File", its locking and WAL pages and its
testing page, RocksDB's crash test, the ALICE and CrashMonkey papers, and issues filed against
LMDB, LevelDB and DuckDB produced forty failure classes. The ones that apply to this engine, and
the test each one gets here:

| Class | Who hit it | Test in this ticket |
|---|---|---|
| a second process rolls back a live writer's journal | SQLite (hot journal rules), this engine in task-2150 and task-2166 | storm with a small pool and bulk transactions, which journals and spills all the time |
| a stale cache after another process writes | SQLite 3.7.5 and earlier | `process_interleavings`, and every storm reader |
| a checkpoint that misses a log reset by another connection | SQLite 3.7.0 to 3.51.2 (the WAL reset bug) | storm workers checkpoint at random; `process_interleavings` checkpoints between statements |
| readers blocked by a writer | this engine in task-2169 | storm readers hold transactions; `process_interleavings` reads beside a held writer |
| a kill at any point of a commit, a rollback or a fold | SQLite TH3 and the crash VFS, RocksDB crash test | storm kills under `normal`; the existing `SimVfs` campaigns |
| crash during recovery, and recovery idempotence | DuckDB issue 26106, a LevelDB fork | storm: a process opened after a kill is itself killed |
| a log or journal from another database, or a deleted hot journal | SQLite | `file_damage` |
| zeroed pages, truncation, bit flips | SQLite dbsqlfuzz | `file_damage` at every page |
| a file copied while open | SQLite | `file_damage` |
| disk full during a commit or a fold | SQLite, RocksDB issue 7784 | `environment`, on `SimVfs` with a live connection |
| read only file system or permission change | SQLite before 3.22 | `environment` |
| one file under two names | SQLite (8.3 names, case on Windows) | `environment` |

Classes that do not apply, or are not tested here, and why:

- **`close()` of any descriptor drops POSIX locks.** The Unix VFS keeps one descriptor per file per
  process; a test needs Linux and is left to the existing VFS conformance suite.
- **`fork()` with an open connection.** Not available on Windows, where the development machine
  runs. Recorded as a follow up for the Linux CI job.
- **Network file systems and lying disks beyond `SimVfs`.** Cannot be run in CI. `SimVfs` already
  models a sync that lies.
- **Power loss that loses the operating system's page cache.** A kill keeps the page cache. The
  `SimVfs` campaigns model the lost cache; a real power loss needs a virtual machine.

## 4. The stress harness

### 4.1 The workload

`crates/inillucent-compat/src/chaos.rs` holds the workload and every check;
`src/bin/chaos.rs` is the program a test starts once per process:

```text
inillucent-chaos setup  <db> [options]
inillucent-chaos worker <db> --name N --id I --seed S --txns T [options]
inillucent-chaos reader <db> --checks C [--hold-ms H] [options]
inillucent-chaos verify <db> --incarnation name:seed:acked:finished ... [options]
options: --frames F  --key K  --search  --bulk-parts B  --busy-ms M
```

Transaction `seq` of a worker is a pure function of the worker's seed and `seq`. It inserts one to
four `ledger` rows (one value in twenty larger than a page), moves money between two of 32
accounts, deletes the rows of transaction `seq - 12`, records `seq` in `progress`, and optionally
writes and deletes `inillucent_search` rows. One transaction in 29 is a bulk transaction of
`--bulk-parts` rows of 24 KiB, larger than a small pool. One in nine is preceded by a transaction
that writes and rolls back. One in fifteen is followed by a checkpoint.

### 4.2 What is checked

A reader opens a deferred `BEGIN`, checks, optionally sleeps `--hold-ms` with the transaction open,
and rolls back:

- the balances add up to the opening total, so half a transfer is never visible;
- every `(worker, seq)` has all its parts and no others;
- each worker's rows are exactly its last 12 transactions, ending at the `seq` `progress` records;
- no row a rolled back transaction wrote is visible;
- the table and its index on `digest` hold the same number of rows;
- a sample of values still match their digests;
- with the search table, it holds one row per transaction and a keyword search finds a worker's rows.

`verify` runs after every process has stopped, in a process that wrote nothing. On top of the
reader's checks it runs the integrity check, and for each worker it requires that the recorded
`seq` is the last acknowledged one, or one more when the worker was killed between its commit and
its acknowledgement, and that every row in the worker's window holds exactly the bytes the seed says.

### 4.3 The suites

`crates/inillucent-compat/tests/durability/process_storm.rs` runs storms at fixed seeds:

| Case | Workers | Readers | Options |
|---|---|---|---|
| default | 4 | 2 | none |
| a pool smaller than a transaction | 4 | 2 | `--frames 64 --bulk-parts 120` |
| the search table | 4 | 2 | `--search` |
| encrypted | 3 | 2 | `--key` |
| no kills | 4 | 2 | none, to separate concurrency defects from crash defects |

The parent kills a random live process every 50 to 450 ms and starts a new one in its place. The
assertion is that `verify` passes, that no reader reported a violation, and that the run did work:
a minimum number of acknowledged transactions and reader checks, so a run where every process was
refused cannot pass. The seed, the incarnations and the failing message are in the failure text.

The durability tier runs each case for fifteen seconds. The nightly tier runs eight workers and four
readers for three minutes per case, including the three settings together.

## 5. The targeted cases

### 5.1 `process_interleavings.rs`

Two processes interleaved at a moment the case chooses:

- a checkpoint called between statements after another process committed;
- a backup called the same way;
- an open beside a writer that holds RESERVED, with the log holding committed rows;
- a reader with a 64 page pool, beside a writer holding 60 and 120 committed values of 24 KiB;
- a read transaction (`BEGIN; SELECT; COMMIT`, `SAVEPOINT`) beside a writer holding a transaction;
- a transaction that starts with the same counter as a process killed in the middle of its own,
  read by a third process before the commit is folded: the killed transaction stays unfinished;
- `inillucent batch` refused because another process holds the file reports `busy`.

### 5.2 `file_damage.rs`

- copy the file and its log while a writer runs, open the copy: it opens and passes its integrity
  check, or it is refused with status `corrupt`; it never answers wrong rows without an error;
- kill a writer with unfolded log, delete each log segment in turn, open;
- put another database's log segment beside the file: ignored or refused, never applied;
- zero, flip one bit in, or truncate the file at every page of a small database: every statement
  that reads it returns rows or an error, never panics, and `integrity-check` names the damage.

### 5.3 `environment.rs`

- disk full while a live connection commits and folds, on `SimVfs`; the connection reports `full`,
  and after space is freed the same connection commits again and the file passes its check;
- a read only database file, and a read only directory: reads work or are refused by name, nothing
  is written;
- one file opened through a relative path, an absolute path and, on Windows, a different case:
  two writers through different names lose nothing;
- a path with non ASCII characters, and one longer than 260 characters.

## 6. Defects found

Each defect has a test that fails without its fix. "Storm" means the stress harness found it;
the targeted test that holds it is named beside it.

| # | Defect | Found by | Fix | Held by |
|---|---|---|---|---|
| 1 | `Database::checkpoint` and `backup_to`, called between statements, folded this process's older pages over another process's commit: deleted rows came back, an extent page was overwritten with a leaf | storm, 2 writers, no kills | `ImportedDatabase::checkpoint` goes through `enter` and `leave` when no statement is running | `process_interleavings::a_checkpoint_called_between_statements_keeps_another_processes_commit`, `a_backup_called_between_statements_copies_another_processes_commit` |
| 2 | an open beside a writer holding RESERVED failed with `busy` at once, whatever the busy timeout | storm: every reader died at open | the open's fold of replayed pages is skipped when the write lock is refused | `an_open_beside_a_writer_holding_the_file_reads_its_committed_rows` |
| 3 | an open with a small pool beside a writer failed with `every frame in the buffer pool is pinned` | storm with 64 frames | the spill file is registered before the open replays the log | `a_reader_with_a_small_pool_reads_beside_a_writer_holding_the_file` |
| 4 | every `BEGIN` took the write lock, so a read transaction in one process failed with `busy` while any other process had a transaction open | the storm's readers | a deferred `BEGIN` and a `SAVEPOINT` that opens a transaction take no lock; `COMMIT`, `ROLLBACK`, `RELEASE` take none of their own | `a_read_transaction_reads_beside_a_writer_holding_one_open` |
| 5 | with a deferred `BEGIN`, `begin_batch` raised the lock after `enter`'s checks and ignored what it found: up to 12 of 120 acknowledged inserts lost (the reason a deferred `BEGIN` had been taken back once before) | `process_concurrency::two_writer_processes_lose_nothing_one_statement_each`, traced | `begin_batch` takes no lock | the same test |
| 6 | a transaction holding no lock waited for RESERVED while keeping SHARED, a cycle with a writer waiting to fold | the same test, as `busy` refusals | such a transaction may let go while it waits | the same test |
| 7 | a commit of a transaction that wrote nothing appended a commit record to the shared log while holding only SHARED | review of defect 4 | no record without the writer's slot | `a_read_transaction_reads_beside_a_writer_holding_one_open` |
| 8 | an open kept its lock: the first statement inside a lockless transaction trusted the open's replay, and its commit was appended at the replay's end of the log, over another process's committed records; every later recovery stopped there (`a log record declares 11401728 bytes`), losing acknowledged transactions and leaving indexes out of step with their tables | debug storms under load, traced | an open releases the lock when done; its own writes (the fold, the tail truncation) go ahead only when the log still ends where its replay ended | `process_storm` (24 of 24 small pool storms under load, 5 of 6 failing before) |
| 9 | a deferred `BEGIN` numbered its transaction before the first statement's resync raised the counter, so numbers were reused; a commit under a reused number made a killed process's unfinished transaction look committed, and readers saw 31 and then 95 of its 120 rows | debug storms, traced | a transaction that has written nothing is renumbered when the resync raises the counter | `process_storm` |
| 10 | a failed log write poisoned the log for good: after a full disk, the same connection refused every statement until it was closed | `environment` | the next statement outside a transaction rebuilds the log from the files | `environment::a_full_disk_refuses_the_write_and_the_same_connection_writes_once_space_returns` |
| 11 | a file the process may not write could not be opened even to read (`access permission denied`, status `invalid_state`) | `environment` | the open falls back to read only, as SQLite does | `environment::a_read_only_file_is_read_and_refuses_writes_by_name` |
| 12 | a connection opened read only accepted an `INSERT`, reported one row changed, and kept the row only in its own memory | the same test | the engine refuses a statement that writes on a read only connection, with status `readonly` | the same test |
| 13 | a busy refusal reported as `syntax` by `inillucent batch` and `inillucent analyze`, and out of memory reported as `syntax` by the driver | `inillucent batch` while probing; the driver's open | the engine's error is kept; out of memory is `too_big` | `a_batch_refused_by_a_held_file_reports_busy`, the driver's `out_of_memory_is_too_big_and_not_a_syntax_error` |

### Not reproduced on the final build

Two storms out of about sixty with a 64 page pool, bulk transactions and the search table all
together failed in a way the fixes above do not obviously explain: once a reader saw `progress`
newer than `ledger` in one snapshot while the final file was correct, and once the final file had
120 pages that no tree reached, after a worker saw `a child is not in the parent that routes to it`.
Both were on builds before defects 10 to 12 were fixed. On the final build, 162 storms of that shape
in a row (eight workers, four readers, 30 seconds each, release binaries) passed with every reader
check and the final verification, and none of 144 storms with any two of the three settings failed
either. `storm_nightly`'s `a_long_storm_with_a_small_pool_bulk_and_search_loses_nothing` runs that
combination every night, so if it comes back it fails a nightly run with its seed and its files.

### Experiments that found no defect

These became tests, so a regression is caught:

- 25 copies of the files taken during writes all opened and passed every invariant.
- Damage at every page of a 65 page file (zeroed, one bit flipped, cut there) was always found by
  `integrity-check` or touched only free pages: 42 cuts, 39 zeroed pages and 39 flipped pages found,
  the rest harmless, and no wrong answer without an error, no panic and no hang.
- A log segment removed after a kill opens at the last fold, consistent. Another database's segment
  beside the file is ignored.
- Paths with non ASCII characters and paths of 314 characters work, and on Windows one file reached
  through `.`, `..` and a different case loses nothing with two writers.

A deleted rollback journal was not tested: since 2.0.7 a writer under `locking_mode = normal`
spills pages to its own file, and in two attempts the exclusive mode path that still writes pages
early did not produce a journal before the statement finished.

## 7. Cadence and cost

| Suite | Tier | Cadence | Time on the development machine |
|---|---|---|---|
| `process_interleavings` | durability | merge | about 10 s |
| `process_storm` | durability | merge | about 2 minutes |
| `file_damage` | durability | merge | under a minute |
| `environment` | durability | merge | under a minute |
| `process_storm_nightly` | nightly | nightly | about 15 minutes |

Each row is in `tests/selection.toml`. `covers` names `inillucent-engine`, `inillucent-pool`,
`inillucent-wal`, `inillucent-vfs` and `inillucent-driver`, so a change to the lock or recovery code
runs the storm on `--changed`.
