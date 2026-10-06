# How the relational engine works

This page describes the SQL half of inillucent: how rows are stored, how a transaction commits, what
the log holds, what happens on the next open after a crash, and what a checkpoint, a backup and
`--root` do. [Architecture](architecture.md) describes the other half, the retrieval engine.
[Architecture in one page](architecture-overview.md) is a shorter version of both.

The page is for two readers. One is an operator who needs to know what a crash can cost. The other
is an engineer who is about to change the engine. Neither needs to have built a database before.
Each rule on this page names the test that checks it.

## Terms used on this page

| Term | What it means here |
|---|---|
| **Page** | The fixed size block the database file is divided into. Every read and write moves whole pages. The default is 32,768 bytes. |
| **Frame** | One slot in the buffer pool. A frame holds one page. |
| **Buffer pool** | The pages the engine keeps in memory, so a read does not go to the disk. `PRAGMA cache_size` sets its size. |
| **Pin** | Holding a page in its frame while code reads it, so the buffer pool cannot evict that page during the read. |
| **B+tree** | The structure a table or an index is stored in. Rows sit only in the bottom pages, and the pages above hold keys and page numbers. |
| **Leaf** | A page at the bottom of a B+tree. Leaves hold the rows. |
| **WAL** | The write ahead log. Records are appended to it before the database file changes. In inillucent it exists in every journal mode, in files named `<database>-wal.NNNNNNNNNN`. |
| **LSN** | Log sequence number: the byte position of a record in the log. Every page stores the LSN of the last record applied to it. |
| **Redo** | Applying a log record to a page during recovery, so the page gets a change the file did not have yet. |
| **Undo** | Putting back what a transaction changed when it rolls back. |
| **Checkpoint** | Writing the changed pages from memory into the database file, then deleting the log that is no longer needed. The engine code calls this a fold. |
| **fsync** | The operating system call that makes written bytes reach the disk. It is the slow part of a commit. |
| **Snapshot** | The state of the database at one moment. A reader with a snapshot sees the same rows for the whole of its transaction. |
| **Group commit** | Several commits that share one log write and one fsync. |
| **Journal mode** | `PRAGMA journal_mode`. In inillucent it chooses how a checkpoint is protected against a crash. It does not change where a commit goes: a commit always goes to the log. |

[The glossary](glossary.md) has these terms and more, one sentence each.

## Defaults

These are the values a new connection starts with. Each was read from the source and printed by
`PRAGMA <name>` against the 1.0.29 release build.

| Setting | Default | Where the source sets it |
|---|---|---|
| `page_size` | `32768` bytes. `PRAGMA page_size = N` and `VACUUM` do not change it. The engine API `Database::open_at` can create a file with 8192, 16384 or 65536 byte pages | `PAGE_SIZE` in `crates/inillucent-engine/src/connect.rs`, `PageSize::DEFAULT` in `crates/inillucent-pool/src/page.rs` |
| `cache_size` | `-131072`, which is 131,072 KiB: 4,096 frames of 32 KiB, or 128 MiB | `DEFAULT_FRAMES` in `crates/inillucent-engine/src/lib.rs` |
| largest `cache_size` | 262,144 frames. A larger request fails with the status `too_big` | the `CacheSize` row in `crates/inillucent-base/src/limits.rs` |
| `journal_mode` | `delete` | `Pragmas::fresh`, and the `journal_mode` handler in `crates/inillucent-engine/src/pragma/tuning.rs` |
| `synchronous` | `2`, which is `FULL` | `Synchronous` in `crates/inillucent-wal/src/writer.rs` |
| `busy_timeout` | `5000` milliseconds | `DEFAULT_BUSY_MILLIS` in `crates/inillucent-pool/src/file.rs` |
| `locking_mode` | `normal` | the `locking_mode` handler in `crates/inillucent-engine/src/pragma/tuning.rs` |

Run the same check yourself:

```sh
inillucent create demo.rdb
inillucent --db demo.rdb query "PRAGMA page_size"      # 32768
inillucent --db demo.rdb query "PRAGMA cache_size"     # -131072
inillucent --db demo.rdb query "PRAGMA journal_mode"   # delete
inillucent --db demo.rdb query "PRAGMA synchronous"    # 2
inillucent --db demo.rdb query "PRAGMA busy_timeout"   # 5000
inillucent --db demo.rdb query "PRAGMA locking_mode"   # normal
```

---

## 1. How the parts fit together

```mermaid
flowchart TB
    SQL["SQL text"] --> Parse["Parse, bind and plan<br/>inillucent-sql"]
    Catalog["Table and index definitions<br/>inillucent-catalog"] --> Parse
    Parse --> Exec["Run the plan in batches<br/>inillucent-exec"]
    Exec --> Engine["Locks, transactions, checkpoints<br/>inillucent-engine"]
    Engine --> Tree["B+trees<br/>inillucent-tree"]
    Engine --> Wal["The log<br/>inillucent-wal"]
    Tree --> Pool["Buffer pool<br/>inillucent-pool"]
    Tree --> Wal
    Pool --> Vfs["Files, locks, clocks<br/>inillucent-vfs"]
    Wal --> Vfs
```

A statement moves down this picture in five steps.

1. **Parse.** `inillucent-sql` turns the text into a syntax tree. It reads no file and needs no
   catalog. A parse error carries a byte offset into the statement.
2. **Bind.** Names become columns, checked against a snapshot of the catalog. The binder decides
   here whether the statement reads or writes. `--readonly` refuses on that decision, so
   `SELECT 1; DROP TABLE t` is refused and a `SELECT` that contains the word "delete" is not.
3. **Plan.** The planner first copies each derived table, view and CTE into the query where SQLite
   would, and plans an outer join the `WHERE` cannot null extend as an inner join. Then it picks an
   access path and a join order. `EXPLAIN QUERY PLAN` prints its choice as SQLite's tree.
4. **Compile.** The plan becomes a chain of operators. The compiled form is cached by statement text,
   so preparing the same text again is a lookup.
5. **Run.** Batches of column values flow up the chain. A statement runs to the end on its first
   step and keeps the rows it produced. That is why `total` in a result is an exact count.

**Checked by:** the `engine` and `differential` test tiers. The `differential` tier runs the same SQL
through inillucent and a pinned SQLite 3.53.4 and compares the answers.

---

## 2. Who owns what

Each crate knows about the layer below it and nothing above it.

| Layer | Crate | What it knows | What it must not know |
|---|---|---|---|
| file system | `inillucent-vfs` | files, locks, clocks, randomness | anything above it |
| buffer pool | `inillucent-pool` | frames, page headers, the free map, values stored outside a page, the rollback journal | what a column is called |
| log | `inillucent-wal` | log segments, the record format, group commit, the recovery scan | pages and trees |
| B+trees | `inillucent-tree` | pages, keys, row bytes, collations | what a column is called |
| transactions | `inillucent-txn` | snapshots, the writer slot, undo, savepoints | SQL |
| SQL front end | `inillucent-sql` | parsing, binding, planning | files |
| execution | `inillucent-exec` | operators, batches, expressions | names |
| catalog | `inillucent-catalog` | `sqlite_schema`, root pages, declared types | plans |
| the database | `inillucent-engine` | all of the above, assembled into a database a caller opens | nothing is above it inside the engine |

Three rules follow from this table.

- **`inillucent-vfs` is the only crate that touches a file.** A test can therefore replace the disk
  with a simulated one, and the crash tests use that to fail a chosen write or sync. `--root` is
  enforced in `inillucent-vfs` for the same reason (section 8). Encryption at rest is a file system
  that wraps the operating system's one for the same reason: every file the engine opens goes
  through it, and no crate above it knows whether the database is encrypted. See
  [Encryption at rest](encryption.md).
- **The log does not depend on the buffer pool.** A log record stores a page number as a plain
  `u64`. The pool is told the highest LSN that is safe on disk, and it refuses to write a page whose
  LSN is at or above that number. The log therefore reaches the disk before any page it describes.
- **The parser sits below the catalog.** The catalog uses the parser to read the `CREATE` text it
  stores. A parse needs no catalog, so the binder works against a read only view of the catalog and
  does no file access.

**Checked by:** `docs/invariants/layering.toml` lists which crate may depend on which.
`cargo test -p inillucent-compat --test tooling policy::` reads every crate's manifest and fails on a
dependency the file does not allow. The same test fails on an unformatted crate, on `unsafe` code
outside the operating system layer, and on a module that does not state its invariant.

---

## 3. Storage: pages and the buffer pool

A database is one file of equal sized pages. Page 0 and page 1 are the two copies of the meta page,
which records the page size, the catalog's root page, the free map and the log position of the last
checkpoint. The format version is at byte 8 of the meta page (section 6). Every other page belongs to
a B+tree, to the free map, or to a value too large to fit in its leaf.

Every page carries a checksum and the LSN of the last log record applied to it. A page whose
checksum fails is reported as damage. Recovery also treats a page whose checksum fails as a page
with no LSN, so every log record for that page is applied again.

The page that holds byte 1 GiB of the file is never given to data. The locking protocol locks bytes
from that offset, as SQLite does, and on Windows a byte range lock stops every other handle from
reading or writing the bytes it covers. SQLite never allocates that page. Before version 2.0.7 the
free map handed it out like any other page, so on Windows a file larger than 1 GiB that two
processes had open failed reads and folds with `disk I/O error`, and a fold that failed part way
left the file damaged. A file an older release wrote keeps whatever it already stores in that page.

### The buffer pool

The buffer pool holds pages in frames. A frame's memory is taken the first time the frame is used,
so a large `cache_size` costs little until pages are read into it.

| What you want | What to run |
|---|---|
| See the pool size | `PRAGMA cache_size`. A negative number is KiB, a positive number is pages |
| Make the pool larger | `PRAGMA cache_size = -524288` for 512 MiB |
| Make the pool smaller | a smaller `cache_size`. The pool keeps the memory it has and uses fewer frames |
| See hits, misses and evictions | `.stats` in `inillucent-shell` |

`PRAGMA cache_size` above 262,144 frames fails with `too_big`. At 32 KiB pages that ceiling is
8 GiB.

A changed page stays in its frame until a checkpoint writes it out. A transaction can change more
pages than the pool holds, and the pool then has to make room. A connection that holds the file
without the exclusive lock, which is every write under `locking_mode = normal` on a connection with
no database attached, writes such a page to a spill file of its own and reads it back from there
when it needs the page again. The spill file is a temporary file that the operating system deletes
when it is closed and when the process ends, and an encrypted database's spill file is encrypted
with a key of its own. Nothing in it has to survive a crash, because every change it holds is in the
log. A fold writes the spilled pages into the database file, under the exclusive lock, with the rest
of the changed pages.

So a reader in another process keeps reading the last committed state through the whole of a large
transaction. Before version 2.0.7 the pool wrote such a page into the database file early, which
needs the exclusive lock, and the writer kept that lock until it committed: every reader in another
process waited for the rest of the transaction and then failed with `busy`. An `UPDATE` of 20,000
rows of 3 KB values was enough, with the default page cache of 128 MiB.

A connection that holds the exclusive lock already, under `locking_mode = exclusive`, with a
database attached, or in the middle of a fold, writes a changed page whose transaction has committed
to the file early. That early write is protected by a rollback journal, so a crash before the fold
finishes can put the old page back. In `wal` mode the journal file is still created for this case.
A page that holds changes of a transaction that has not committed goes to the spill file even under
the exclusive lock. If the writer were killed with such a page in the database file, other processes
could read rows of a transaction that will never commit before anybody put the journal back.

The journal never outlives the file lock. Under `locking_mode = normal`, a connection whose journal
holds old page images folds the log into the file before it releases the lock, and the fold removes
the journal. So a journal that another process finds beside an unlocked file always belongs to a
process that died while it held the lock. Before version 2.0.3 the journal stayed until the next
fold. A second process that opened the file in that window wrote the old images back over pages
the first connection had committed, and the first connection kept working from pages it believed
were current. That lost committed rows, and it produced pages of zeros and B+tree levels that did
not reach every leaf.

Putting a dead process's journal back moves the meta record's generation on. Other processes may
have read the pages the dead process wrote early before the journal was put back, because putting
it back needs the RESERVED lock and a live writer may hold it. Every connection compares the meta
record on each statement, so the new generation makes each of them rebuild its cache from the log
over the file as it now is. Before version 2.1.5 nothing changed in the meta record or the log when
a journal was put back, and a connection that had read the newer pages could fold without one of
them and lose it for good.

---

## 4. Transactions and isolation

inillucent allows one writer at a time for each file. Other processes can read the file between
writes.

| Setting | What it does |
|---|---|
| `locking_mode = normal` (default) | a statement takes the file lock when it starts and releases it when it ends. A read takes a shared lock. A write takes an exclusive lock. An open transaction keeps the lock from its first write to its `COMMIT`. A query that reads no table, such as `SELECT 1`, takes no lock |
| `locking_mode = exclusive` | the connection keeps the lock until it closes. No other process can use the file |
| `busy_timeout` | how long a statement waits for a lock another process holds. After 5000 milliseconds by default, the statement fails with the status `busy` |

A reader in another process waits while a writer holds the exclusive lock. It does not read an older
snapshot past that writer. `inillucent-txn` contains a snapshot implementation, and the shipped
engine does not use it between processes. There is no shared memory index a reader in another
process could use to find the log, so the engine holds the file lock instead. [The roadmap](roadmap.md) describes the change that would let a
reader of another process read a snapshot while a writer works.

`BEGIN` and `BEGIN DEFERRED` take no lock, which is what SQLite's do. The first statement inside the
transaction takes the lock that statement needs: a read takes the shared lock and a write takes the
write lock. The transaction keeps the lock it holds until `COMMIT` or `ROLLBACK`. So a reader's
`BEGIN; SELECT ...; COMMIT` reads beside another process's open write transaction. `BEGIN IMMEDIATE`
and `BEGIN EXCLUSIVE` take the write lock at once. A transaction that reads first and then writes is
refused with `busy` when another process committed after its first read. Roll it back and run it
again, or start it with `BEGIN IMMEDIATE`. Before version 2.0.8 every `BEGIN` took the write lock,
so a read transaction failed with `busy` for as long as any other process had a transaction open.

When a connection takes the lock, it checks whether another process changed the file or the log
since it last looked. If either moved, it drops its cached pages and replays the log from the file's
last checkpoint before it reads anything. If a rollback journal is beside the file at that moment,
a process died while it held the lock, and the connection puts the journal's old pages back before
it replays, as an open does.

The check costs two reads. The connection reads the 120 bytes of the meta record in the second of
the file's two meta pages, the shadow copy, and the length of the log segment it appends to. A
checkpoint writes the same record to both meta pages and always writes the shadow copy first, so a
checkpoint that has changed either page has changed the shadow copy. On Windows a statement outside
a transaction makes six system calls for the lock and the check: three to take the shared lock, the
two reads, and one to release the lock. Version 2.1.4 and earlier read both meta pages, which was a
seventh call.

Three more moments make the same check:

- **The end of an open.** An open replays the log under the shared lock and then releases the lock,
  so the first statement takes its own lock and sees what other processes committed during the
  open. Through `connect::Database` and every driver, the replay is the first statement's: the open
  itself reads the meta record without taking the shared lock, and the first statement takes the
  lock and replays. A meta record read while a checkpoint rewrites it may be the older of the two
  copies, and the first statement's check then finds the shadow copy changed and reads the record
  again. When neither copy the open read checks out, the open reads them again under the lock and
  reports what that read finds. The open writes the file itself only when the log still ends where its replay ended.
  Before version 2.0.8 an open kept its lock, and a commit by its first statement could be appended
  at the end of the log as the open had read it, over another process's committed records.
- **A transaction's number.** A deferred `BEGIN` numbers its transaction before any lock is taken,
  and the first statement's check renumbers it above every number in the log. Recovery decides
  which records to replay by transaction number, so a reused number made the records of another
  process's unfinished transaction look committed.
- **A checkpoint called on the database handle.** `Database::checkpoint`, `backup_to` and an export
  take the lock the way a statement does before they fold. Before version 2.0.8 they folded this
  connection's pages as they stood, over another process's newer commit.

A write the file system refuses, for example because the disk is full, fails its statement with the
status `full` and poisons the connection's log for that statement. The next statement outside a
transaction rebuilds the log from the files, so once space is free the same connection writes
again. A file the process may not write is opened read only. Its rows can be read, and a statement
that writes fails with the status `readonly`.

A connection holding the shared lock never writes the file. A replay can change more pages than
the pool holds, and the pool then spills them, as a writer's pool does (section 3), so the replay
needs no lock beyond the shared one. A replay that fails part way drops every page it produced, so
that no later fold on that connection, and not its close, writes a half applied log into the file.

`ROLLBACK` is done from the log. It works the same way in every journal mode, including `off`.

A rollback puts the rows back through the B+trees, so the page splits and new pages the transaction
made stay in the trees, holding the rows as they were. The undo is logged under the same transaction
as the changes, and a finished rollback ends with a commit record, so a replay applies the
transaction and its undo together and arrives at the pages the connection holds. A statement that
fails in autocommit mode and is undone, for example with `busy`, ends the same way. Before version
2.0.7 no commit record followed a rollback, so a replay skipped the transaction and its undo, and the
records of the next committed transaction named pages the replay did not know: the next open after
a crash failed with `read 0 of 32768 bytes at` the file's own size, or applied rows to an older copy
of a page, and a fold then wrote the result into the file.

Two statements are refused inside an open transaction:

| Statement | Error |
|---|---|
| `DETACH` | `cannot DETACH database within transaction` |
| `VACUUM` | `cannot VACUUM from within a transaction` |

`DETACH` is refused because a transaction records the files it touches by their attachment number,
and removing one would renumber the others. `VACUUM` is refused because it reads committed rows,
and an open transaction has not committed.

A transaction that writes to more than one attached file is decided by a super journal outside those
files. Each file's log holds a vote. Recovery reads the super journal to learn whether the vote
became a commit.

**Checked by:** `inillucent-txn`'s own `transactions` and `durability` suites, and the
`multi_database_commit` and `multi_database_participants` suites in `inillucent-compat`. The model in
`crates/inillucent-model` is a second, separate implementation of the transaction rules. It depends
on one crate only, so it cannot share a bug with the engine.

---

## 5. The log, and what a crash costs

### What a commit does

```mermaid
sequenceDiagram
    participant App as Application
    participant Engine as inillucent-engine
    participant Log as Log file
    participant Pool as Buffer pool
    participant File as Database file
    App->>Engine: COMMIT
    Engine->>Log: append row changes and a commit record
    Engine->>Log: write and fsync
    Log-->>Engine: durable up to this LSN
    Engine->>Pool: pages below this LSN may now be written
    Engine-->>App: ok
    Note over Pool,File: Later, at a checkpoint
    Pool->>File: write the changed pages
```

The rule is **write ahead**: a log record reaches the disk before the page it describes. A commit
appends its row changes and a commit record to the log, then writes and syncs the log. The changed
pages stay in the buffer pool. A later checkpoint writes them into the database file. Until then an
acknowledged commit exists only in the log, and the log is enough to rebuild it.

`inillucent-engine` connects the two halves. After every sync of the log it tells the buffer pool the
new durable LSN. The buffer pool refuses to write a page whose LSN is at or above that number.

**Group commit.** Several connections that commit at the same moment share one log write and one
fsync. The first committer writes and syncs the log for all of them. The others wait until their own
records are on disk. A single connection does one write and one sync for each commit under
`synchronous = FULL`.

**A failed write stops the log.** If a log write or sync fails, the log keeps the error and every
later call returns it. The engine then refuses to start new work, because a log with a gap in its
durable records cannot be trusted.

### A copy into an empty table

`INSERT INTO t SELECT ...` writes its rows one at a time like any other insert, with one exception.
When `t` holds no row and the query returns at least 1,024 rows, the engine builds `t`'s tree in one
pass instead:

1. Every row is built and checked first: column affinity, `NOT NULL`, `CHECK`, `STRICT` types, and
   the rowid against the rows already built. Nothing is written yet. A row that fails a check fails
   the statement with the same error, and the same `last_insert_rowid()`, as the row by row insert.
2. The rows are sorted by rowid and packed into full leaves. Each index of `t` is built the way
   `CREATE INDEX` builds one. A duplicate in a `UNIQUE` index hands the statement back to the row by
   row insert, which reports the constraint SQLite reports.
3. The pages go straight into the database file, and the file is synced before the commit. The log
   gets one allocation record per page and no page images. The catalog rows are rewritten to name
   the new roots.

A crash before the commit leaves `t` empty. A crash after it leaves every row. A rollback, a
`ROLLBACK TO` and a statement that fails all leave `t` empty with every page given back.

The engine uses the row by row insert when `t` is a `WITHOUT ROWID` table, has `AUTOINCREMENT`, has
a trigger or a foreign key in either direction, is followed by a vector index, or when the statement
has `RETURNING`, an upsert, or a conflict clause other than `ABORT`. A column's own
`NOT NULL ON CONFLICT IGNORE` or `REPLACE` does not stop the bulk build. `CREATE TABLE ... AS
SELECT` fills its table through the same statement, so it gets the bulk build too.

### A `DELETE` or `UPDATE` of many rows

A `DELETE` or an `UPDATE` that nothing can watch changes each leaf once for all of the statement's
rows in that leaf. Nothing can watch the statement when it has no trigger, no foreign key action and
no `RETURNING`, and, for an `UPDATE`, no `FROM` and no subquery that reads the row being changed.

1. The rows are put in the table's order.
2. For each leaf, the engine reads the leaf once and finds every row of the statement that the leaf
   holds.
3. A `DELETE` writes one `DeleteRows` record for all of the leaf's rows, which names the leaf and
   lists the keys, and one undo entry per row. The undo entry is read from a copy of the leaf. The
   values the statement needs to remove the row's index entries are read from the same copy, so a
   row is copied out only when the leaf holds a value stored outside the page. An `UPDATE` writes
   one log record and one undo entry per row, the records the row by row path writes.
4. It changes the leaf once and stamps it with the last record's LSN.
5. A `DELETE` checks once whether the leaf has emptied enough to merge with its neighbour. Then it
   removes each index's entries the same way, in that index's order.

An `UPDATE` writes a row this way when exactly one column outside the key changes. When a new value
fits where the old one lies, the row is changed in place. When one does not, the leaf is packed again
with every change of the run, and the new page is logged whole as one `CompactLeaf` record; a leaf
the changes no longer fit in is split. The row by row path writes every other row: a row whose key or
index entry changes, and a row with more than one changed column.

A build older than 2.1.3 cannot replay a log that holds a `DeleteRows` record, and refuses it with
`a log record has kind 17, which this format does not define`. A clean close folds the log into the
file, so this only matters for a database whose connection stopped without closing and is then
opened by an older build.

The order rows are written in can change what an `UPDATE` leaves. `OR FAIL` keeps the rows written
before the failing one, and `OR IGNORE` and `OR REPLACE` keep the first row to claim a unique value.
When the statement or a constraint carries one of those clauses, the engine keeps the order the query
found the rows in, and uses the leaf at a time write only when that order is already the table's.
A statement that fails part way has the rows before the failing row written first, so it leaves what
the row by row path leaves.

### A leaf split

A leaf splits when it has no room for a row and its rows do not fit one page after they are packed
again. The first rows stay on the leaf, the rest move to a new page, and the parent gains a key that
points at the new page. When rows arrive in key order the leaf keeps 95% of a page. Otherwise the
rows are split evenly.

The log describes most splits by what they did, in one `SplitLeaf` record. Recovery rebuilds each of
the three pages from that page's own state:

| Page | How recovery rebuilds it |
|---|---|
| the leaf that split | packs again the first rows it keeps, read from the leaf itself |
| the new page | packs the moved rows, which the record carries |
| the parent | adds the key and the pointer, which the record carries |

Each page stands alone because the buffer pool can write one of the three pages to the file before
the other two. If the new page were rebuilt from the old leaf's rows, a crash after the old leaf had
reached the file would leave nothing to rebuild the new page from.

Three kinds of split still log the three pages whole: a split of the root, a split whose parent has
no room for the key, and a split that moved a value stored outside the page. Three whole pages are
98,304 bytes of log at a 32 KiB page. A `SplitLeaf` record is the moved rows, four bytes for each
of them, the key, and 104 bytes.

A build of 2.1.2 or earlier cannot replay a log that holds a `SplitLeaf` record, and refuses it with
`a log record has kind 16, which this format does not define`. A clean close folds the log into the
file, so this only matters for a database whose connection stopped without closing and is then
opened by an older build.

The log is stored in segment files beside the database: `app.rdb-wal.0000000001`,
`app.rdb-wal.0000000002`, and so on. A segment holds up to 64 MiB before the log moves to the next
one. A checkpoint deletes the segments it no longer needs. After a clean close, one small segment
file stays beside the database.

### `PRAGMA synchronous`: what a crash can lose

| `synchronous` | When the log is synced | A crash of the process | A power loss |
|---|---|---|---|
| `2`, `FULL` (default) | on every commit | loses nothing that was acknowledged | loses nothing that was acknowledged |
| `1`, `NORMAL` | when 64 MiB of log has built up, before a checkpoint, and when a statement that wrote releases the file lock | loses nothing that was acknowledged | may lose the most recent commits, and does not damage the database |
| `0`, `OFF` | never | the source makes no promise | may lose commits and may damage the database |

`PRAGMA synchronous = 3` (`EXTRA`) is accepted and behaves as `FULL`.

These rules come from `NORMAL_SYNC_BYTES` and the `Synchronous` type in
`crates/inillucent-wal/src/writer.rs`. Under `NORMAL` a commit is acknowledged when its record has
been written to the operating system. The operating system keeps those bytes when the process dies.
A power loss can drop them.

### Recovery on open

```mermaid
flowchart TB
    A["Open the file"] --> B{"Is a journal left<br/>from a checkpoint?"}
    B -- yes --> C["Put the old page images back"]
    B -- no --> D["Read the meta page:<br/>the LSN of the last checkpoint"]
    C --> D
    D --> E["Scan the log from that LSN"]
    E --> F["Apply each committed record<br/>to pages that do not have it yet"]
    F --> G["Discard transactions<br/>with no commit record"]
    G --> H["Replay page allocations and frees<br/>in log order"]
    H --> I["Cut the log after<br/>its last valid record"]
    I --> J["Read the catalog and<br/>accept statements"]
```

Recovery runs before anything reads a page of a table. It runs the same way for the main database
and for every file `ATTACH` opens.

An open through `connect::Database`, which is what the command line, the shell and every driver
use, runs in two halves, the way `sqlite3_open_v2` does. The open itself puts a journal back when
one is left, opens the file, checks its format and reads the meta record, so a file that is not a
database, has a newer format, needs a key or has a torn meta record is refused by the open. The
rest of the chart, from the log scan on, runs at the first statement, as SQLite reads its schema
at the first statement. A damaged log is therefore reported by the first statement, and an open
that is closed again without a statement reads nothing past the meta record and writes nothing. When the log held committed transactions that the file
did not have yet, the command line prints a line such as this one:

```text
replayed the log: 1 committed transactions were in the log and not yet in the database file (3 records scanned, 2 applied). A connection that still has the file open, or one that ended before a checkpoint, wrote them.
```

That is the normal state while another process has the database open and has not checkpointed, so
the line says nothing about damage. When the log also held a transaction with no commit record, which
is what a process killed in the middle of a transaction leaves, the line starts with `recovered the
log` and counts the transactions it discarded:

```text
recovered the log: 3 records scanned, 2 applied, 1 transactions committed, 1 discarded.
```

With `--output json`, both cases carry the same `recovered` member with the five counts.

Four rules make recovery safe:

1. **Replay can run twice.** A page stores the LSN of the last record applied to it, so a record the
   page already has is skipped.
2. **A record for a page that no longer exists is skipped.** The page may have been freed and the
   file made shorter.
3. **A page stamped with an LSN this log cannot have written is refused.** Such a page would skip
   every later record, and those writes would be lost with no error.
4. **Allocations and frees are replayed in log order.** A page that was freed and then allocated
   again inside the replayed range must come back allocated. If all frees were replayed last, a live
   page would be marked free, and the next allocation would give it to a second owner.

A read only connection replays the log into its own memory and writes nothing to the disk. If a read
only connection finds a journal left by an interrupted checkpoint, it refuses to open, because
putting the old pages back is a write.

**Checked by:** `crates/inillucent-compat/tests/durability/new_engine_free_map_recovery.rs`,
`new_engine_recovery_shapes.rs`, `wal_crash.rs`, `multi_database_crash.rs`, and the `durability` tier.
The `durability` tier stops a simulated machine at a chosen write or sync, then reads back what the
file holds.

### Checkpoints

A checkpoint writes the changed pages from the buffer pool into the database file, records the new
recovery point in the meta page, and deletes the log segments that are no longer needed.

```mermaid
flowchart TB
    A["Sync the log"] --> B{"Journal mode"}
    B -- "wal" --> C["Append the new image of each page<br/>to the log, then sync"]
    B -- "delete, truncate, persist" --> D["Save the old image of each page<br/>to the journal, then sync"]
    B -- "memory, off" --> E["No protection"]
    C --> F["Write the pages into<br/>the database file"]
    D --> F
    E --> F
    F --> G["Write the meta page:<br/>the new recovery LSN"]
    G --> H["Finish the journal"]
    H --> I["Delete log segments<br/>below the recovery LSN"]
```

A checkpoint runs at four moments:

| When | What starts it |
|---|---|
| The log has grown by 4 MiB since the last checkpoint | a statement that wrote, as it releases the file lock. The number is `RECLAIM_BYTES` in `crates/inillucent-engine/src/checkpoint.rs` |
| The connection closes | the connection holds pages the file does not have, or a journal with old page images in it |
| Somebody asks | `PRAGMA wal_checkpoint`, `inillucent checkpoint`, `VACUUM`, `inillucent backup`, an integrity check, or the driver's `checkpoint` |
| The journal mode or the locking mode changes | `PRAGMA journal_mode` or `PRAGMA locking_mode` |

A checkpoint writes pages in place. If the power fails halfway, some pages hold new bytes and some
hold old bytes. The log alone cannot always repair that, because most log records hold row changes
and not whole pages. The journal mode decides what protects the checkpoint:

| `journal_mode` | What protects a checkpoint | After the checkpoint | After a crash during a checkpoint |
|---|---|---|---|
| `delete` (default) | old page images in `<database>-journal`, synced before any page is written | the journal file is deleted, and the directory is synced | the old images are put back, then recovery replays the log |
| `truncate` | the same journal | the journal is cut to zero bytes | the same as `delete` |
| `persist` | the same journal | the journal's header is zeroed | the same as `delete` |
| `wal` | the new image of every page, appended to the log and synced before any page is written | nothing to finish | recovery installs the page images from the log |
| `memory` | nothing | nothing to finish | a half written checkpoint stays half written |
| `off` | nothing | nothing to finish | the same as `memory` |

In `wal` mode a `<database>-journal` file is still created when a transaction's pages outgrow the
buffer pool (section 3). Writing an uncommitted page early needs an old image to undo it, and a log
of new images cannot undo.

`inillucent` and `inillucent-shell` turn on `PRAGMA defensive`. Under `PRAGMA defensive`,
`PRAGMA journal_mode = off` is refused, and the pragma answers with the mode already in force.

The journal has a checksum on its header and on every record. Recovery stops at the first record
that fails its checksum. A record can fail only if it was written after the journal's last sync, and
no page is overwritten before the sync that covers its old image. So every record from the failure
onward names a page the checkpoint had not reached. Each record also carries the transaction's
nonce, so `persist` mode cannot replay a record left from an earlier transaction. The two meta pages
are journaled like any other page, so an interrupted checkpoint cannot leave a meta page that names
a checkpoint which never finished.

### The evidence

The crash campaigns stop a simulated machine at every write and every sync of a workload, then
recover and compare the result. Each campaign runs from a fixed seed and writes its result into
`tests/crash/`. A change to one of these files is a change in what the engine does under failure.

| File in `tests/crash/` | What it records |
|---|---|
| `delete-full-crash.txt` | 94 cut points, 56 acknowledged commits, no detected damage, nothing lost |
| `truncate-full-crash.txt` | 166 cut points, 55 acknowledged commits, no detected damage, nothing lost |
| `persist-full-crash.txt` | 166 cut points, 55 acknowledged commits, no detected damage, nothing lost |
| `delete-full-checkpoint-crash.txt` | 55 cut points inside a checkpoint, every one recovered to the committed state |
| `truncate-full-checkpoint-crash.txt` | 54 cut points inside a checkpoint, every one recovered to the committed state |
| `persist-full-checkpoint-crash.txt` | 54 cut points inside a checkpoint, every one recovered to the committed state |
| `wal-commit.tsv` | 32 cut points in `wal` mode: 29 recovered the old state, 3 the new state, none damaged |
| `wal-checkpoint.tsv` | 62 cut points in a `wal` checkpoint: 29 old, 33 new, none damaged |

**Checked by:** `crates/inillucent-compat/tests/durability/durability.rs` and `wal_crash.rs` write those files.
`crash_reports.rs` fails when a campaign reports fewer cut points than the number a person accepted.
`new_engine_log_retire.rs` checks that a checkpoint deletes the log it no longer needs.

---

## 6. What a file's format version promises

The first eight bytes of a database are `RDB2` and four zero bytes. The next four bytes are the
**format version**. This build writes format `2` and reads formats `1` and `2`. The number is covered
by the meta page's checksum, so a hand edited version fails the checksum.

| Rule | What it means |
|---|---|
| A point release reads every file an earlier point release of the same minor version wrote | a bug fix never changes the format version |
| A change to the layout of a page, a record or the header raises the format version | that is a minor version, and it comes with a migration |
| A build that meets a higher version refuses the file by name | the error is `this database is format version N and this build reads version 2; upgrade inillucent to open it`, with the status `unsupported`. The command line exits 3 |
| A version below 1 is reported as damage | there is no format 0, so a zero means the header was overwritten |

A migration is `inillucent-migrate`, which reads the older file and writes a new one. It does not
rewrite the file in place, because a crash could stop a rewrite halfway.

### Format 2, and how a format 1 file still opens

Format 2 changed two things in a page:

- **A leaf's delta area has a directory.** The delta area holds rows written to a leaf since it was
  last packed. Format 2 starts it with a list of two byte entries in key order, so a lookup is a
  binary search, and the area can use the leaf's whole free space. The layout is in
  `crates/inillucent-tree/src/leaf/delta.rs`.
- **A page's checksum covers its LSN.** In format 1 a flipped bit in the LSN went undetected. The
  rule is in `crates/inillucent-pool/src/page.rs`. The checksum is CRC-32/ISO-HDLC; over a page it
  is computed as four interleaved streams joined with zlib's combine arithmetic, which gives the same
  value as one stream.

A read of a leaf that took writes merges the delta area into the packed rows. The second time a
reader reads such a leaf without the page changing, it packs a copy of the leaf, and the buffer pool
keeps up to 64 of these copies beside their pages. Later reads use the copy until the page changes.
A copy is never written to the file or the log; the pool drops it when the page's bytes change.
The code is `crates/inillucent-pool/src/pool/merged.rs` and `crates/inillucent-tree/src/paged/copies.rs`.

This build reads a format 1 file page by page. A leaf carries the flag `LEAF_DELTA_DIRECTORY` that
says which layout its delta area uses, and a checksum is accepted under either rule. A format 1 leaf
keeps format 1's rules until a compaction or a split rewrites it. That rule exists because recovery
replays the log onto the pages the file holds, and it must produce the same bytes the log was written
against. The file's own version becomes 2 at the next checkpoint.

Releases 0.1.5, 0.1.6 and 0.1.7 refuse a format 2 file with
`this database is format version 2 and this build reads version 1; upgrade inillucent to open it`.
Releases 0.1.1, 0.1.2 and 0.1.3 answer `database disk image is malformed: neither meta page is
readable`. None of them reads the file.

### Values stored outside a page

A value too large for its leaf is stored on other pages. The leaf keeps a sixteen byte **extent
reference**: a page number and a length. Two bits above the page number, `CLASS_STATED` and
`CLASS_TEXT`, say whether the value is text or a blob when the column's declared type would give the
wrong answer. A column declared with no type, as in `CREATE TABLE t (a)`, is such a case.

Every other reference is written exactly as before. A build older than the class bits reads a stated
reference as a page number above 2^62, fails to fetch it, and reports an error. It never returns the
bytes with the wrong type. `ExtentClass` in `crates/inillucent-pool/src/extent.rs` holds the
encoding. `extent_class_for` and `extent_datum` in `crates/inillucent-tree/src/leaf/layout.rs` write
and read it.

### Layouts inside the file

The format version covers pages, records and the header. It does not cover what a virtual table
stores in its shadow tables. Those layouts carry their own numbers:

| Layout | Where its number is | What a newer number does |
|---|---|---|
| pages, records and header | byte 8 of the meta page, `crates/inillucent-pool/src/meta.rs` | the database does not open |
| an FTS5 index | a `%_data` row, `crates/inillucent-ext/src/vtab/fts5/layout.rs` | the database opens and the table's rows read. `MATCH`, any write, and `fts5vocab` fail with `the full-text index on T is in layout N, written by inillucent X.Y.Z, and this build reads layouts up to 2` |
| an `inillucent_search` index | the `format` row of `%_config`, `crates/inillucent-search/src/options.rs` | the database opens. Every read and write of the table fails with `the table is in format N, written by inillucent X.Y.Z, and this build reads formats 1 and 2` |

All three refusals carry the status `unsupported`, so the command line exits 3. An application can
then tell "upgrade and try again" apart from a wrong query and from an empty result.

A search table with no facet column stores format `1`. A search table with a facet column stores
format `2`, so a build that does not know facets refuses that table by name.

A missing FTS5 layout record means "a layout this build can read". Files written before the record
existed have none, and they open. The record is written by `CREATE VIRTUAL TABLE`, `rebuild` and
`delete-all`. An ordinary insert does not write it, because a file written by 0.1.2 through 0.1.7 can
hold rows in both FTS5 layouts. `term_value` in `crates/inillucent-ext/src/vtab/fts5/index.rs`
reads such a file one row at a time.

Release 0.1.1 is the one published build that misreads a later file. It answers
`WHERE note_fts MATCH 'segment'` with no rows on a file whose FTS5 index a later build wrote. Every
other query on that file answers correctly in 0.1.1.

**Checked by:** `crates/inillucent-compat/tests/nightly/release_format_history.rs` runs every published
release's own binary against a file this build wrote. The 0.1.1 `MATCH` answer is a row in its
`KNOWN_GAPS` list. `tests/interop/<version>/` holds a file each release wrote, and
`release_format.rs` opens each one, writes to it, crashes, and recovers. `format_refusal.rs` builds a
record from a future build and checks each refusal, including the command line's exit code.

---

## 7. Backup, `VACUUM` and integrity checks

| Command | What it does |
|---|---|
| `inillucent backup <file>` | runs a checkpoint, copies the database file to `<file>`, then opens the copy and checks every tree in it. A file already at `<file>` is replaced |
| `inillucent restore <file>` | points this session at `<file>`. It changes no file. To replace a database, copy the backup over it |
| `VACUUM` | rebuilds the database: it runs the stored `CREATE` statements again, copies every row back through the normal write path, writes the result beside the database, and renames it over the database |
| `VACUUM INTO '<file>'` | writes a compacted copy the same way. It refuses a file that already exists, with `output file already exists` |
| `inillucent checkpoint` | runs `PRAGMA wal_checkpoint`. Run it before you copy a database file by hand |

A rename replaces a directory entry in one step, so a crash leaves either the old file or the new
one. `VACUUM` and `VACUUM INTO` both fail inside an open transaction. `--root` confines both
(section 8).

`VACUUM` does all its file work through the connection's own `Vfs`: it creates the new file, renames
it with `Vfs::rename`, and deletes the old log segments through the same `Vfs`. An application that
supplies an encrypting or in memory `Vfs` therefore gets a `VACUUM` that stays inside it. The code is
in `crates/inillucent-engine/src/rebuild.rs`.

**Checked by:** `crates/inillucent-compat/tests/engine/vacuum_on_vfs.rs`, and `vacuum_crash.rs`, which
crashes a simulated machine inside the rename.

### `PRAGMA integrity_check` and `PRAGMA quick_check`

| Pass | What it finds | `quick_check` | `integrity_check` |
|---|---|---|---|
| every tree on its own | a leaf that does not parse, keys out of order, a separator that does not match its child, a sibling chain that skips a leaf | yes | yes |
| every page against every other page and the free map | a page two trees both reach, a page a tree reaches that the free map calls free, a page marked allocated that no tree reaches | yes | yes |
| every index against its table | a duplicate in a `UNIQUE` index, a row with no index entry, an index entry for a row that is not there | no | yes |

The page pass finds a page that two tables share. Each tree looks correct on its own in that case,
so only the page pass can report it.

A page that is allocated and that no tree reaches is lost space. No data is lost with it. No
statement leaves such a page behind: a dropped table, a `CREATE` that rolled back, and a `REINDEX`
all give their pages back.

Both checks read and never repair. Neither proves the database opens, because damage in the log is
outside the file they read. To check a database before you rely on it, open it and run a statement.

---

## 8. Limits on one request, and `--root`

### What one request may spend

`inillucent_base::budget` limits the rows, the bytes and the time one request may use, and holds the
flag `cancel` sets. The engine checks the budget at two points:

- every batch a result collects, which limits what the caller receives;
- every leaf a scan reads, which limits the work done on the way. A `SELECT` whose `WHERE` rejects
  every row still stops.

| Program | Rows scanned | Row bytes | Time | Rows one call returns | Request and reply size |
|---|---|---|---|---|---|
| a Rust application, a driver, `inillucent`, `inillucent-shell` | no limit | no limit | no limit | no limit | no limit |
| `inillucent-mcp` | 10,000,000 | 256 MiB | 60 seconds | 10,000. `limit=0` is refused by name. The default is 200 | a request line up to 1 MiB, a reply up to 8 MiB |

An application that links the engine has no reason to limit itself. A server that hands a database to
an agent does, so only `inillucent-mcp` sets limits. The numbers are `Limits::served` in
`crates/inillucent-base/src/budget.rs` and `MAX_ROWS`, `MAX_REQUEST_BYTES` and `MAX_RESPONSE_BYTES`
in `crates/inillucent-cli/src/mcp.rs`.

`cancel` sets its flag from any thread. The statement fails with the status `interrupted`, and the
connection stays usable. `inillucent capabilities` reports `cancel` as `partial`, because an operator
finishes the piece of work it is in before it sees the flag.

**Checked by:** `crates/inillucent-compat/tests/e2e/budgets.rs`, which drives the shipped
`inillucent-mcp` over `JSON-RPC`. One case checks that the command line has no limit, so a change
that put the limit everywhere fails.

### `--root` confinement

```sh
inillucent --root /srv/data --db /srv/data/app.rdb backup /tmp/copy.rdb
# Error [invalid_state]: "/tmp/copy.rdb" resolves to /tmp/copy.rdb, which is outside /srv/data,
# which this server is confined to.
```

`--root DIR` means no file outside `DIR` is opened for a request. `inillucent-vfs` enforces it, and
`inillucent-vfs` is the only crate that opens files. So `ATTACH DATABASE`, `VACUUM INTO`, `backup`,
`restore`, `import`, `export` and the database named with `--db` all follow the same rule.

The check resolves the real path. Each part of the path is followed through the file system, so a
Windows junction or a Unix symbolic link inside the root is replaced by its target before the
check. `..` removes the last part of the resolved path. A path that does not exist yet is checked up
to its deepest existing parent, so a file about to be created can be allowed.

**Checked by:** `crates/inillucent-compat/tests/e2e/confinement.rs`, which drives the shipped programs.
It first shows that each escape works without `--root`, so a refusal proves the confinement works.

---

## 9. Extensions and virtual tables

The host hands a virtual table module the root pages of its shadow tables. The module never looks up
a name and never opens a transaction of its own. FTS5 and the R-Tree keep their indexes in ordinary
tables, and this rule keeps them away from the catalog and the session.

`inillucent_search` is registered at the connection, one layer above the other modules.
`Registry::with_builtins` sits two crates below the retrieval engine, and registering it there would
link a vector index into every database.

**Checked by:** `fts5.rs`, `rtree.rs`, `vtab.rs`, `new_engine_vtab.rs` and
`new_engine_vtab_stream.rs`.

---

## 10. Keeping a vector index current

An `inillucent_search` table keeps its index in five shadow tables:

| Shadow table | What it holds |
|---|---|
| `%_content` | the rows, one column per declared column, facets included |
| `%_delta` | a log of the rows that changed since the index was last built |
| `%_gen` | the published generations of the built index |
| `%_state` | which generation is current and how far it covers |
| `%_config` | the declaration, including which columns are facets |

A facet's value goes into the index as an attribute of the row, so a search can filter on it before
it ranks.

Four operations touch the index, and each costs a different amount:

| Operation | What it does |
|---|---|
| a write | an `INSERT`, `UPDATE` or `DELETE` writes the row and one delta row. It does not touch the HNSW graph |
| a query | loads the current generation, applies the delta rows it does not cover, and answers from the merged index. BM25 scores depend on the whole corpus, so the delta must be merged before ranking. The merged index is cached on the connection for the same delta entries |
| a commit that folds | when the delta log passes its limit, the commit inserts each pending entry into the current generation and publishes the result as the next generation. The graph work is one insert per delta entry |
| `compact` or `rebuild` | `INSERT INTO docs(docs) VALUES('compact')` reads every row and builds a new graph. It removes the chunks earlier folds marked as deleted. It runs inside the caller's transaction like any other write |

### The settings

| Declaration | What it does | What it costs |
|---|---|---|
| no `compact` clause | the delta log folds at 1,024 entries (`COMPACT_FLOOR` in `crates/inillucent-search/src/options.rs`) | a fold builds a segment from its own batch and writes nothing else, so a commit's cost does not grow with the table |
| `compact = N` | the delta log folds at `N` entries | a larger `N` means fewer, larger segments: less work at query time, more work in the commit that folds |
| `compact = 0` | nothing folds automatically | the delta log grows without limit, and every query reads all of it until `compact` runs. A bulk load wants this, and the migration declares it |
| `threads = N` | a fold and a build use `N` cores | `threads = 1` keeps the engine on one core and makes a fold about `N` times slower |
| `mode = 'exact'` (the default for a table declared `USING inillucent_search`) | a vector search compares every row | recall is 1.000, and a fold cannot change an answer |
| `mode = 'approximate'` (the default for an index made with `CREATE INDEX ... USING inillucent_hnsw`) | a vector search walks the HNSW graph | recall depends on the graph |

### What folding costs

`inillucent-foldgate` runs two arms on the same corpus, vectors and commit boundaries, one row per
transaction, each arm in its own process. The `fold` arm folds. The `build` arm declares
`compact = 0` and runs `compact` at the commits where `fold` folds, which is the older behavior. The
run below used 40,000 documents of 64 dimensions, `mode = 'approximate'`, on a 24 core Windows
machine. It was recorded on this page on 9 September 2026.

| | fold | build |
|---|---:|---:|
| chunks inserted into the graph, whole run | 35,579 | 255,989 |
| chunks inserted by the largest single publish | 4,447 | 35,579 |
| generations published | 19 | 19 |
| commit time, median | 5.0 ms | 5.3 ms |
| commit time, 99th percentile | 17.1 ms | 17.2 ms |
| commit time, slowest | 5,189.0 ms | 4,085.1 ms |
| recall at ten, against an exact scan | 0.704 | 0.637 |
| peak memory | 180.9 MiB | 196.4 MiB |
| database file | 204.6 MiB | 204.6 MiB |
| reopen and answer the query set | 2,875.6 ms | 3,015.4 ms |
| reads served by a second connection during the run | 14,700 at 5.2 ms median | 17,912 at 5.0 ms median |
| reads refused | 0 | 0 |

Both arms answered the query set the same way after a close and a reopen. Folding did 7.2 times less
graph work over the run, and its largest publish was 8.0 times smaller. The slowest commit was
slower with folding, because a fold reads and decodes the generation it folds into before it writes
the new one. The median and 99th percentile commits were the same or faster.

Segmented generations then made the default fold limit a constant 1,024 entries. Measured on
100,000 documents, one row per commit, the median commit went from 0.152 ms to 0.032 ms and the
99th percentile from 0.923 ms to 0.097 ms. That run was recorded on this page on 13 September 2026.

Version 2.0.7 changed three things about a commit that merges segments, and one about the first
query after another process writes:

- **A segment is read by rowid.** `%_state` holds `gen_rows`, a map of where each segment's rows are
  in `%_gen`. Without it, loading one segment read every row of `%_gen`, each a blob of up to
  512 KiB: on a table of 120,000 rows of 768 numbers that was about 1 GB a load. A table an older
  release wrote has no map, and the first write that adds a segment builds it.
- **A fold is one batch on every core.** Folding a segment into a merge, and applying the delta rows
  a query has not seen, used to insert one document at a time into the graph on one core.
- **A merge stays within `merge_budget`.** A second fold in one commit happens only when it fits
  what is left of the budget.
- **A reader catches up in one batch.** After another process deleted and inserted 2,000 rows, a
  connection's next search took 3.26 s and now takes 0.52 s.

Measured with 2,000 rows of 768 numbers a commit and the default options, on a 24 core Windows
machine: the commits that merge took 13.7, 17.7 and 21.3 s at 40,000, 80,000 and 120,000 rows, and
take 2.1, 2.8 and 3.3 s.

### What an application can read back

`%_state` is an ordinary table:

| Key | What it says |
|---|---|
| `rows` | live rows in `%_content` |
| `chunks` | chunks in the current generation, live and deleted |
| `inserted` | chunks the last build inserted into the graph |
| `folds` | folds since the last `compact` or `rebuild` |
| `generation` | which generation is current |
| `covered` | the highest delta sequence the current generation contains |
| `gen_rows` | where each segment's rows are in `%_gen`, as three integers a segment: its id, its first rowid and its number of rows |

`chunks` minus `rows` is how many chunks the graph holds that no live row points at. An update marks
the old chunk deleted and adds a new one, and only `compact` or `rebuild` removes the old one. When
that difference or `folds` is large, run `compact`.

### What a failure leaves

Publishing a generation is a set of ordinary writes inside the caller's transaction. A crash during
a publish leaves the old generation current and the delta log unchanged. The replaced generation's
rows stay in the file until `drop-old-generations` runs, because a snapshot opened before the switch
may still read them.

A merge that takes more than one commit leaves a segment stored as a chain: the segment's id names
its last link, and each link names the one before it. `drop-old-generations` keeps every link of
every live segment. Before version 2.0.7 it kept only the id the manifest names and deleted the
earlier links, and the table could no longer be read (`segment 42 is unreadable`). `rebuild`
repairs a table that lost links that way.

If a generation cannot be read back, the query fails with an error, and `rebuild` repairs it.
`%_content` holds every row, so the index can always be built again from the database alone.
`integrity-check` compares the recorded row count with the rows, checks every vector's width, and
names any delta entry whose row is missing.

**Checked by:** `search.rs`, `search_crash.rs`, `vector.rs`, `new_engine_search.rs`, the graph's own
tests in `crates/inillucent-core/src/hnsw.rs`, and `inillucent-foldgate` for the numbers.

---

## 11. Where to go next

| Question | Page |
|---|---|
| Which SQL runs, and what differs from SQLite? | [SQL support](sql.md) |
| What does each pragma do? | [Pragmas](pragmas.md) |
| How fast is it, and on what machine? | [Performance](performance.md) |
| What do the 416 measured cases show? | [Feature comparison](feature-comparison.md) |
| How does the retrieval engine work? | [Architecture](architecture.md) |
| What may a crate link? | [Dependency policy](dependency-policy.md) |
| What is not built yet? | [Roadmap](roadmap.md) |
| What is each crate for? | [Repository](repository.md) |
| What does a term mean? | [Glossary](glossary.md) |
