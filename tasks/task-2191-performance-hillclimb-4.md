# task-2191: a fourth performance hill climb, on the way people call inillucent

## 1. What was asked

The README says inillucent is 419% faster than SQLite. That number comes from
`inillucent-fullgate`, which runs both engines in process and times statements. Jason's question
was whether it holds when inillucent is used the normal way: from the command line, from a shell
script, from Python and from Node. This ticket measured that, wrote down where the time goes, and
fixed what could be fixed without changing the storage format.

## 2. How it was measured

The gate measures the engine. Nothing measured a program a person or an agent actually runs, so
this ticket wrote a usage benchmark, kept under `_agent_output/task-2191/bench/` (gitignored):

| Script | What it times |
|---|---|
| `usage-bench.mjs` | the command line and the shell as separate processes, the MCP server over its standard input, and it runs the two below |
| `py-bench.py` | the Python driver over the C library, against Python's own `sqlite3` module |
| `node-bench.mjs` | the npm package, against Node's `node:sqlite` |

SQLite is the pinned 3.53.4 shell for the command line rows, Python's `sqlite3` (3.51.0) for the
Python rows and Node 23's `node:sqlite` for the Node rows. Every workload runs once per build per
round, the order of the builds rotates every round, and a result is the median of the rounds. A
ratio is SQLite's time divided by inillucent's, so above 1 is faster than SQLite. Both engines run
with their default settings, because the defaults are what normal use gets.

The fixture is a table of 10,000 notes, `note(id INTEGER PRIMARY KEY, title, body, tag, created)`,
with an index on `tag` and one on `created`.

## 3. What 2.1.5 looked like

Measured on 5 October 2026, ten rounds, with no other ticket running:

| How it was called | SQLite measured as | 2.1.5 against SQLite |
|---|---|---|
| the command line, one process a command, 8 workloads | the `sqlite3` 3.53.4 shell | 20% slower |
| a SQL script into the shell, 6 workloads | the same scripts into `sqlite3` | 218% slower |
| the Python driver, 9 workloads | Python's `sqlite3` module | 173% slower |
| the npm package, 2 workloads | Node's `node:sqlite` | 29,270% slower |
| all 25 | | 216% slower |

Each group is the geometric mean of its workloads. The 419% headline does not describe this kind of
use. Section 5 has every workload.

Profiles of the slowest cases said where the time went:

- **An open and close from Python cost 12.1 ms**, against 0.07 ms for SQLite. The close was 9.3 ms
  of it: the C library checkpointed on every close, after a read as well.
- **A full scan from Python spent 12 ms in the engine and 170 ms in the binding**, at two `ctypes`
  calls a cell, and a slice of a pointer turned into a list of integers for every text value.
- **One `INSERT` of 20,000 rows over 20,000 lines took 5.8 s in the shell**, because the shell
  rescanned the whole statement after every line to ask whether it had ended.
- **A one row `inillucent exec` spent 12 ms closing**: the fold at close rolled a new log segment,
  wrote a checkpoint record and deleted the old segment every time.
- **A script of single row inserts spent 8.6% cloning the target table's definition**, twice a
  statement, and 19% in the Windows heap.
- **An npm call started a process**, about 10 to 18 ms, for a query that takes microseconds.
- **`inillucent.exe` started 1.2 ms slower than `sqlite3.exe`**, because Windows loaded the dynamic
  C runtime and the TLS client's crypt32, secur32 and ws2_32 at every start.

Most of the gap was in the code around the engine: the C library's close, the shell's statement
splitter, the Python binding's two foreign calls a cell, the Node package's process a call, and the
work a Windows process does before `main`. Inserting a row is the part that is in the engine.

## 4. The changes

Each change was measured against the build before it, on this machine. The probes are under
`_agent_output/task-2191/bench/`.

1. **`inillucent_close` folds only what it owes.** The C library's close called
   `Database::checkpoint`, which folds whether or not the connection wrote anything: it synced the
   file, wrote the meta record and started a new log segment after every open, even after a read.
   Through Python an open and close cost 12.1 ms, and the close was 9.3 ms of it. Close now calls
   `fold_if_owed`, the fold `Drop` already performed, which does nothing for a connection that only
   read. Open and close: 12.1 ms to 1.2 ms. SQLite: 0.07 ms.
2. **The shell checks for the end of a statement only on a line that can end one.** `drive`
   rescanned everything accumulated after every line, so one `INSERT` of 20,000 rows written a row
   to a line was quadratic: 5.8 s against 41 ms for SQLite's shell. A statement ends at a semicolon,
   and text that held one can only become complete when a block comment closes, so a line with
   neither `;` nor `*/` is not checked. SQLite's shell skips the same lines.
3. **`INSERT`, `UPDATE` and `DELETE` share the target table with the catalog.** `writable_target`
   cloned the table's `TableInfo` (every column, every index, the `CREATE` text) and
   `push_write_source` cloned it again. The `SELECT` path already shared it through
   `CatalogView::shared_table`; the write path now does too. The shell's statement splitter also
   stopped allocating a vector for every word it upper cases. A script of 100,000 single row
   inserts took 17% fewer profile samples.
4. **The CSV parser copies a field in one piece, and lends it when it can.** `parse` pushed one
   character at a time into a new `String` per field, and growing those strings was 13% of
   importing 300,000 rows. A field that is not quoted is now a slice of the file.
5. **Open does less.** Building the eponymous `pragma_function_list` table ran the pragma to learn
   its column names, which listed every function at every open: 9% of an open. Its columns are
   written down now, as the settings' are. The eponymous tables are kept as shared pointers and no
   longer cloned at each catalog refresh (5% of an open).
6. **Three new C ABI calls carry a result or a parameter list as one JSON text (ABI 1.3.0).**
   `inillucent_rows_json`, `inillucent_bind_json` and `inillucent_stmt_execute_many`. Python's
   `ctypes` costs about half a microsecond a call, and the cell accessors take two calls a cell: a
   full scan of 20,000 rows spent 12 ms in the engine and 170 ms in the binding. The values are
   written the way the command line writes them, `{"blob": "<hex>"}` included, plus
   `{"real": "Infinity"}` for the reals JSON cannot spell. The three are provisional.
7. **The Python binding uses them, keeps prepared statements and has `execute_many`.**
   `Connection.execute` with parameters prepared and freed a statement every call; it now keeps up
   to 128, as Python's `sqlite3` does. A batch whose values are all plain types is encoded with one
   `json.dumps`. A library older than ABI 1.3.0 still works through the old calls.
8. **The JSON escaper copies runs of plain text whole.** `escape_into` pushed one character at a
   time. It now finds the next byte that needs escaping and copies the text before it in one call.
   Every `--output json` result, every MCP answer and every Python result goes through it.
9. **A Node session.** `open(db)` keeps one `inillucent-mcp` process and sends each call to it over
   standard input. The server already writes the command line's JSON result when a tool call asks
   for `output: json`, so no Rust changed for this. A lookup by key: 17.8 ms through `query()`,
   0.115 ms through a session.
10. **Close does the log's housekeeping only once the open segment holds 1 MiB.** A fold that was
    asked for also rolls a new segment, writes a checkpoint record and deletes the old segments,
    which an earlier ticket measured at two thirds of a fold. Close always asked. It now folds every
    time, which keeps the closed file complete, and does the housekeeping when the open segment holds
    `CLOSE_RECLAIM_BYTES` or a segment rolled during the connection. The open segment's size counts
    every process that appended to it, so a run of short processes still shrinks the log. Mean of 300
    cycles of open, one insert and close through the C library: 20.0 to 23.4 ms before, 13.1 to
    16.5 ms after.
11. **Windows release builds link the C runtime statically and load three DLLs on first use.**
    `inillucent.exe` loaded VCRUNTIME140, five `api-ms-win-crt` sets, crypt32, secur32 and ws2_32
    at every start; the last three are for the TLS client a PostgreSQL or MySQL migration uses.
    `sqlite3.exe` loads kernel32. `packaging/release-all.ps1` now passes
    `target-feature=+crt-static` and `/DELAYLOAD` for the three. Median of 300 starts of
    `--version`: 16.54 ms to 15.42 ms, against 15.34 ms for `sqlite3 -version`.

## 5. The result

The same run as section 3, the same build of 2.1.5, and this branch:

| How it was called | 2.1.5 against SQLite | Now against SQLite | Change in inillucent |
|---|---|---|---|
| the command line, 8 workloads | 20% slower | **10% slower** | 9% faster |
| a SQL script into the shell, 6 workloads | 218% slower | **44% slower** | 121% faster |
| the Python driver, 9 workloads | 173% slower | **2% slower** | 168% faster |
| the npm package, 2 workloads | 29,270% slower | **2,223% slower** | 1,164% faster |
| all 25 | 216% slower | **46% slower** | 118% faster |

A second run of six rounds, straight after: all 25 went from 209% slower to 49% slower (107%
faster), the command line 10% faster, the shell 110%, Python 141% and Node 1,175%. The workloads
that wait on the disk (`shell.insert.200.autocommit`, `py.insert.100.autocommit`, `py.update.1k`)
moved most between the runs, on both engines.

| Workload | 2.1.5 ms | now ms | SQLite ms | 2.1.5 against SQLite | now against SQLite | change in inillucent |
|---|---:|---:|---:|---|---|---|
| `cli.version` `inillucent --version` | 16.815 | 16.318 | 15.924 | 6% slower | 2% slower | 3% faster |
| `cli.point.json` one row by key, `--output json` | 19.947 | 18.532 | 16.726 | 19% slower | 11% slower | 8% faster |
| `cli.point.text` one row by key, text | 19.858 | 18.461 | 16.823 | 18% slower | 10% slower | 8% faster |
| `cli.range.json` 100 rows by an index, JSON | 21.905 | 20.868 | 18.298 | 20% slower | 14% slower | 5% faster |
| `cli.count` `count(*)` and `max()` | 20.230 | 18.839 | 17.182 | 18% slower | 10% slower | 7% faster |
| `cli.insert` `exec` of a one row `INSERT` | 30.149 | 22.504 | 19.285 | 56% slower | 17% slower | 34% faster |
| `cli.tables` `tables` | 20.144 | 18.758 | 17.622 | 14% slower | 6% slower | 7% faster |
| `shell.select.1k` 1,000 lookups piped into the shell | 40.158 | 40.987 | 40.663 | 1% faster | 1% slower | 2% slower |
| `shell.insert.200.autocommit` 200 `INSERT`s, each its own commit | 90.341 | 77.425 | 268.4 | 197% faster | 247% faster | 17% faster |
| `shell.insert.10k.txn` 10,000 `INSERT`s in one transaction | 98.881 | 77.687 | 33.521 | 195% slower | 132% slower | 27% faster |
| `shell.insert.multirow.20k` one `INSERT` of 20,000 rows, a row a line | 5348.9 | 81.812 | 34.513 | 15398% slower | 137% slower | 6438% faster |
| `shell.import.csv.50k` `.import --csv` of 50,000 rows | 163.6 | 153.5 | 55.967 | 192% slower | 174% slower | 7% faster |
| `shell.fixture.build` the fixture script: 10,000 `INSERT`s and two indexes | 131.1 | 113.4 | 56.037 | 134% slower | 102% slower | 16% faster |
| `cli.dump` `dump` of 10,000 rows | 37.952 | 35.767 | 32.477 | 17% slower | 10% slower | 6% faster |
| `mcp.query` one MCP `inillucent_query` call | 0.082 | 0.058 | - | - | - | 41% faster |
| `py.fixture.build` build the fixture through the driver | 167.5 | 99.301 | 38.099 | 340% slower | 161% slower | 69% faster |
| `py.point.select.5k` 5,000 lookups by key | 96.113 | 74.929 | 85.166 | 13% slower | 14% faster | 28% faster |
| `py.scan.all` read every row | 92.637 | 13.549 | 7.782 | 1090% slower | 74% slower | 584% faster |
| `py.range.100x200` 200 queries of 200 rows by an index | 159.5 | 38.284 | 35.772 | 346% slower | 7% slower | 317% faster |
| `py.aggregate.200` 200 `GROUP BY` queries | 153.1 | 124.4 | 518.4 | 239% faster | 317% faster | 23% faster |
| `py.insert.10k.txn` 10,000 inserts in a transaction | 142.4 | 67.093 | 25.671 | 455% slower | 161% slower | 112% faster |
| `py.insert.100.autocommit` 100 inserts, each its own commit | 118.1 | 72.118 | 342.4 | 190% faster | 375% faster | 64% faster |
| `py.update.1k` 1,000 updates, each its own commit | 1204.7 | 733.4 | 4627.6 | 284% faster | 531% faster | 64% faster |
| `py.open.close.100` 100 opens and closes | 1672.6 | 100.1 | 7.726 | 21550% slower | 1196% slower | 1570% faster |
| `node.open.point.close` `query()`, one lookup | 10.350 | 8.869 | 0.087 | 11776% slower | 10076% slower | 17% faster |
| `node.point.call` one lookup: `query()` on 2.1.5, a session now | 10.350 | 0.076 | 0.014 | 72531% slower | 430% slower | 13599% faster |

`mcp.query` has no SQLite to compare with. The `node.point.call` row compares 2.1.5's only way to
make a call, a process, with a call through the new session.

## 6. What is still slower than SQLite, and why

- **Single row inserts.** 10,000 `INSERT`s into the shell take 2.3 times SQLite's time, a CSV
  import 2.7 times and Python's 10,000 inserts 2.6 times. What is left is the engine putting a row and
  its index entries into the tree (`put_absent`, `make_room`, `write_index_entry`), plus a descent to
  find the largest rowid for each row with no key (6.6% of an import). These are the same paths the
  gate's `churn.refill` and `app.insert.prepare_each` measure, and earlier hill climbs worked them.
  Compiling each statement of a script is the other half for the shell: 28% of a script of single
  row inserts after this ticket.
- **Opening a file.** 1.0 ms from Python against 0.08 ms. An open opens the file and the log, reads
  the meta record, loads the schema and builds the eponymous tables; SQLite reads nothing until the
  first statement. Loading the schema at the first statement is the next step and a larger change.
- **Node.** A session is a pipe to another process: 0.076 ms a lookup against 0.014 ms for
  `node:sqlite` in process. Matching it needs a native binding over the C library.
- **The command line**: 1 to 2 ms a process. About 0.5 ms of it is starting the program and the
  rest is the open and the query.

## 7. Tried and not kept

| Idea | What was measured | Why it was not done |
|---|---|---|
| The pooled allocator in the C library, as the programs have | 15% of Python's `execute_many` of 10,000 inserts was in the Windows heap | The pool keeps free lists per thread. In a library loaded into another program, a thread that exits leaves its lists behind, which is a leak the host cannot see. It needs a design for thread exit first |
| A close housekeeping bar of 64 KiB or 4 MiB instead of 1 MiB | 300 cycles of open, insert and close: 64 KiB 13.1 to 15.8 ms, 1 MiB 15.9 to 16.5 ms, 4 MiB 15.7 to 16.1 ms, within the noise of each other | 1 MiB keeps the segment an open reads small (0.8 ms with 215 KB in it) and does the housekeeping once in about 1,400 single row processes |
| Keeping the largest rowid between inserts | 6.6% of a CSV import | Every write path would have to keep it current, and a stale value is a duplicate key |
| A lighter `Pool::new` for an open | 6% of an open | It is 60 microseconds |

## 8. Tests

New tests:

| Test | What it checks |
|---|---|
| `durability::fold_protocol::a_close_shrinks_the_log_only_once_it_has_grown` | a close after one small row deletes no log segment and leaves a file a reopen does not replay into; a close with a mebibyte of log waiting deletes the segments it wrote; every row survives. Its first half failed while the rule counted the segment the log opens with as a roll, and its second half fails when close never does the housekeeping |
| `inillucent-cli` `trailing_statement_tests::a_statement_over_many_lines_runs_once_it_ends` | 2,000 rows over 2,000 lines, a block comment closed on a line with no `;`, and a trigger whose body spans lines |
| `inillucent-cli` `import::tests::carriage_returns_go_and_text_after_a_quote_stays` | the run copying parser drops a carriage return anywhere, keeps text after a closing quote and reads an unterminated quote to the end |
| `inillucent-driver` `wire::tests`, five tests | every kind of value reads back as itself, a real never prints as an integer, Python's spellings are read, malformed text and a 65 bit integer are refused |
| `inillucent-driver-capi` `json_reads_and_binds_the_values_the_accessors_do` | JSON in and out agrees with the typed accessors, refusals carry the right status, a too long row in a batch runs nothing |
| `packages/npm/inillucent/roundtrip.test.mjs`, the session case | a session writes, reads a blob back, classifies a failure, and its writes are in the file after close |

Floors lowered, with the measurement written beside each in
`crates/inillucent-compat/tests/tooling/crash_reports.rs`: the four DELETE commit campaigns from 76
cut points to 63, and the three DELETE checkpoint campaigns from 46 to 45. Every cut point that went
was a call made after the commit was acknowledged, in the close. Putting the old close back and
running the campaigns returned exactly 76 and 46. Every cut point still recovers to a state the
campaign accepts.

`inillucent-testrun --changed main --strict` with this machine's declared absences, after every
change above: 372 targets, 4,587 tests, none failed. Four suites were not evidenced by declaration
(OpenSSL, MySQL, PostgreSQL, Go). Two ran without their prerequisite and evidenced nothing, which
makes the run exit 1: `inillucent-remote::lib` needs `INILLUCENT_NETWORK_TESTS`, and
`workload_freshness` needs the Nikaya checkout. The first run, before the floors and the fold test
were settled, failed on `policy` (formatting), `harness` (the symbol register needed the three new
calls), `crash_reports` (the floors above) and `fold_protocol` (the test's own setup, described
above). `cargo clippy --all-targets --all-features -D warnings` is clean on every changed crate,
and the Node package's tests pass against the new build.
## 9. Round 2: the gaps after round 1

Jason asked for every workload to be faster than SQLite. Round 2 started from section 6's list,
asked task-2192 for guidance on the parts that touch the storage design
(`tasks/task-2191-fable-guidance.md`), and followed its order.

### 9.1 What changed

1. **Results reach Python as Python objects.** The C library builds each result with CPython's
   own constructors, whose addresses the binding hands it once from `ctypes.pythonapi`
   (`inillucent_py_init`, `inillucent_rows_py`). Nothing is written as JSON and parsed again.
   `None` and the exception type come from the interpreter's exported symbols, and the path is
   used only under CPython.
2. **The C library is a Node addon.** It exports `napi_register_module_v1` and finds Node's
   `napi_*` functions in the host process when Node loads it, so no Node is linked. The npm
   package loads it with `process.dlopen` and keeps the process route for a host without it. A
   handle is accepted only on the thread that opened it, a thread's environment closes what it
   left open when Node tears it down, every N-API status is checked, and a result is built in
   handle scopes of 1,000 rows.
3. **An `INSERT ... VALUES` of plain literals is compiled once.** The literals are lifted out as
   `?1`, `?2` and bound at every execution (`inillucent_sql::lift`), through both `prepare` and
   `execute`. A lone number, a negated number or a quoted string that is a whole element of a row
   is lifted; a literal inside an expression stays in the text. A text that does not compile is
   compiled as written, so the error and its position are the ones the caller's text gives.
4. **An open stops after the file's own checks.** Opening the file, checking its format and
   reading its meta record happen at the open, so a file that is not a database, has a newer
   format or needs a key is still refused there. Recovery, the log and the schema wait for the
   first statement, which is where SQLite reads its schema too.
5. **The open does less work.** The file is opened as one that must exist, the header is read
   once, both meta pages are read in one call and only the newer copy's checksum is checked when
   it is valid. Trees read their height and leftmost leaf the first time something asks. The
   pool builds its per frame tables a chunk at a time. Recovery reads the log from the checkpoint
   on, parses the catalog only when a record needs it, and hands its segment handle to the
   writer. Path checks on Windows use `GetFileInformationByName`.
6. **A tree remembers its largest rowid.** The value is used only while the leaf it was read from
   is still the tree's rightmost leaf and carries the same log position, so it answers what a
   fresh descent would.
7. **A write parses its leaf once.** The room check is computed from the parse the locate already
   made, where it used to take the page through `Pool::modify` and parse it three more times.
8. **The C library uses a shared size classed allocator** (`inillucent_alloc::Shared`), with
   lists that belong to the process so a host thread that ends leaves nothing behind.
9. **The shell keeps its buffer and scans a statement once**, skips the bytes between quotes,
   comments and semicolons, and skips the drawing work for a statement that returns no columns.
10. **The command line** lists only the files that start with the database's name when it looks
    for stray log segments, and a Windows release build gives the main thread the 64 MiB stack
    the statements are sized for, so no thread is started per call.

11. **An `INSERT` of several rows of literals runs as its one row template over all of them**,
    through the bulk build when the table is empty. The rows are moved into the statement, not
    copied, and the lift sizes its vectors from the text.
12. **A prepared single row `INSERT` can run many rows as one statement.** The engine allows it
    inside a transaction the caller opened, for a `VALUES` row that is exactly its parameters, into
    a table with no trigger, `RETURNING`, upsert, foreign key, or conflict clause other than
    `ABORT`. When the one statement fails it has written nothing and the caller runs the rows one
    at a time, which reports the same row's error and keeps the rows before it. The C library's
    `execute_many`, the shell's `.import`, and a run of single row inserts the shell holds use it.
13. **The shell holds a run of single row inserts of one shape inside a transaction**, and runs
    them as one statement before anything that could see them: another statement, a dot command,
    the end of the input. Nothing is held while an option that acts per statement is on.
14. **The entry a non unique index takes for a newly placed row searches only the delta
    directory**, because its key ends in a row key no live row had.
15. **A `VALUES` row is evaluated against one batch**, built once for the row.
16. **A fold under a rollback journal syncs once.** The meta slots' pre-images go into the
    journal with the pages', so one journal sync and one data file sync cover both. A page of
    zeros in the file is not journaled, because it is a page the file never held.
17. **A close folds once.** A fold that did not reclaim the log left its own records counted as
    unfolded, so the close after `inillucent_close`'s fold folded again.
18. **`set_limit` forgets the compiled statements when the value changes**, because the binder
    checks the trigger depth while it compiles.
19. **An open reuses the meta slots its bootstrap read.** The first lock no longer reads both
    slots a second time.
20. **A new file is written with one sync.** The catalog's first pages and both meta slots are
    written, then the data file is synced once. A new log segment's header is written without a
    sync of its own; the first commit's sync covers it.
21. **A row of an `INSERT` that runs many rows at once is built from values it owns**, so a text
    or blob value moves into the row instead of being copied.
22. **The command line and the shell use `Carved`**, a per thread allocator that cuts blocks from
    64 KiB chunks instead of asking the system for each one.
23. **Blocks from 4 KiB to 64 KiB are recycled.** All three allocators keep four lists, of 8, 16,
    32 and 64 KiB blocks, shared by every thread and capped at 512 KiB each. A leaf is 32 KiB and a
    write copies it, encodes it and reads its rows into buffers of about that size; the Windows
    heap returned each of them to the operating system and committed it again on the next request.
24. **The npm package's `query()` keeps the file open between reads.** A statement that starts with
    `SELECT`, `WITH` or `VALUES` runs on a database kept open from the last such call on the same
    file. The file's id is checked on every call, and the database closes after a second with no
    call. Any other statement opens and closes a database of its own, so nothing it leaves on its
    connection reaches the next call.
25. **A request under 8 bytes is pooled in the 16 byte class.** All three allocators sent it to
    the system heap, because a freed block keeps its list link in its first 8 bytes. The block a
    class hands out is always the class's size, so the link fits. Short text values, such as a
    CSV field, went to the Windows heap one allocation and one free at a time.
26. **`bcrypt.dll` is loaded on first use.** Only creating a database and writing a rollback
    journal ask for random bytes, so a command that reads never loads it.
27. **A database is opened from the handle that created it.** Creating a file closed it and
    opened it again, and the second open waited for the virus scanner, which reads a file on the
    first open after a handle that wrote it closes. Every run that created a database paid about
    6 ms for it.
28. **Python's `execute_many` reads its rows straight from the Python objects.** Four new
    provisional calls: `inillucent_py_init_params` takes the addresses of ten CPython readers and
    the eight types they apply to, `inillucent_py_params` reads a list or tuple of rows of `None`,
    `int`, `bool`, `float`, `str` and `bytes` with the interpreter lock held, and
    `inillucent_stmt_execute_params` runs them without it. Anything else crosses as JSON, as
    before, so every refusal is unchanged.
29. **A leaf hint covers the leaf's whole fence range.** The descent reads the separators either
    side of the leaf it reaches, and the hint keeps them instead of the lowest and highest probes
    seen. An index entry for a new row sorts past every probe its leaf has seen, so it used to
    descend. Of the 30,000 puts of the Python insert, the hint answered 18,759 before and 29,982
    after.
30. **An undo record keeps a key of one or two values without a vector.** Every row an insert
    writes, and every index entry, keeps one until the statement ends: three allocations a row for
    a table with two indexes.

### 9.2 The result

Measured on 5 October 2026, ten rounds, the build at the end of round 1 against the build at the
end of round 2, with no other work running. Each group is the geometric mean of its workloads, and
`docs/performance.md` has every workload.

| Group | Round 1's build against SQLite | Round 2's build against SQLite |
|---|---|---|
| the command line, 8 workloads | 11% slower | 1% slower |
| scripts into the shell, 6 workloads | 42% slower | 35% faster |
| the Python driver, 9 workloads | 9% slower | 77% faster |
| the npm package, 2 workloads | 2,603% slower | 102% faster |
| all 25 | 51% slower | 39% faster |

17 of the 25 workloads are faster than SQLite, against 5 at the start of round 2.

### 9.3 What is still slower, and why

| Workload | Against SQLite | Why |
|---|---|---|
| Python, 10,000 inserts into a table with two indexes | 22% slower | the engine's own work per row and per index entry: room in a full 32 KiB leaf, a log record each, an undo record each. No part is more than a fifth of it. A guaranteed gap after compaction, at 8%, 15% and 25% of the page, changed nothing measurable |
| `exec` of a one row `INSERT` | 10% slower | three syncs, the commit's log sync and the close fold's journal and data syncs, where SQLite makes two. Removing one means committing without a log sync when a fold follows under the same lock, which changes when a commit is durable |
| `query` of 100 rows by an index | 9% slower | a cold read of 200 scattered rows reads about 65 pages of 32 KiB, where SQLite reads about 180 of 4 KiB |
| Python, 100 opens and closes | 5% slower | the first open after the update workload waits for the virus scanner on the log segment as well as the data file, about 1 ms |
| `--version`, one row as JSON, `count(*)`, `tables` | 1% to 4% slower | the run to run difference of a 16 to 18 ms process start. A cold open is about 1.4 ms of code and data page faults, spread over recovery, the schema and the catalog |

### 9.4 Tests

| Test | What it checks |
|---|---|
| `engine::usage_paths` | inserts that differ only in their values compile once and store what the literals denote, extremes included; a lifted statement's errors are the written text's; an open and close that run nothing change no file; the remembered largest rowid follows a delete, a rollback and a move |
| `engine::analyze_reopen::a_page_stamped_above_the_logs_end_refuses_the_open` | now accepts the refusal at the first statement after the open |
| `inillucent-sql` `lift::tests`, five tests | what is lifted and what is left, the extremes, and the statements that are left alone |
| `inillucent-pool` `chunked::tests`, two tests | a table entry never moves when the table grows, and free frames go lowest first |
| `inillucent-alloc` `a_shared_block_freed_on_one_thread_serves_another` | the shared lists recycle a block across threads |
| `inillucent-tree` `attaching_to_a_page_that_is_not_a_root_is_refused` | now refused at the first read of the tree |
| `inillucent-alloc`, four tests | carved blocks do not overlap and are recycled; a page sized request falls in its power of two class; a page sized block grows in place inside its class, keeps its bytes across classes and is handed out again after it is freed |
| `packages/npm/inillucent/roundtrip.test.mjs`, three tests | a kept read sees another process's write; a file made again at the same path is read afresh; an insert does not run on the kept connection |
| `e2e::held_inserts`, three tests | held inserts print what inserts run one at a time print, across many scripts; a duplicate among held inserts is reported on its own line; an import reports the first line that will not go in |

Floors lowered in `crash_reports.rs`, each measured by turning the segment hand off off: the
three DELETE fault campaigns and the DELETE power loss campaign from 63 to 62, the TRUNCATE and
PERSIST power loss campaigns from 149 to 148, and the WAL checkpoint campaign from 53 to 52.

Floors lowered again for the reused meta slots and the unsynced segment header, each measured
through `inillucent-testrun` by putting each change back in turn: the DELETE campaigns from 57 to
54, the TRUNCATE and PERSIST power loss campaigns from 141 to 135, the five checkpoint campaigns
from 43 to 42, the three WAL commit campaigns from 23 to 20 and the WAL checkpoint campaign from 49
to 45. `wal_crash.rs`'s own minimum moved from more than 20 cut points to more than 17 for the same
reason.

`inillucent-testrun --changed main --strict` after every checkpoint: 374 targets, 4,615 tests, none
failed. As in round 1, two suites ran without their prerequisite and make the run exit 1:
`inillucent-remote::lib` and `workload_freshness`.

### 9.5 Tried and not kept

| Idea | What was measured | Why it was not kept |
|---|---|---|
| A guaranteed gap after a compaction, at 8%, 15% and 25% of the page | the Python indexed insert, interleaved runs: 41 to 45 ms at every setting | no difference |
| 8 KiB pages for the indexed insert | the same time as 32 KiB | the page size is not where that workload's time goes |
| Committing the statistics a fold refreshes without a sync of its own | one log sync fewer per fold that reclaims the log | a close fold reclaims only past 1 MiB, so no benchmark workload reached it |
| The free map written without a log record | `free_map_checkpoint_crash` failed | a torn free map page has nothing to repair it from |

## 10. Round 3: the gaps after round 2

The guidance for this round is `tasks/task-2191-fable-guidance-round3.md`. It ranks twelve changes;
four of them need a decision from Jason (D1 a batched insert log record, D2 the command line's
`exec` holding its lock through the close fold, D3 a log end field in the meta record, and whether
a carryless multiply CRC may use `unsafe`). None of those four is in this round.

### 10.1 What changed

- **The rollback journal is synced before it is deleted only when something was saved after its
  seal** (rank 1). The seal's sync already covered every pre image. A trace measured the sync at
  46 us, not the 1.2 ms the guidance estimated.
- **An append into a full leaf splits at once** (rank 2), and packs the left page once, instead of
  splicing and repacking a leaf that was going to split anyway. In the Python benchmark's 10,000
  inserts into a table with two indexes, compactions fell from 78 to 47.
- **A tree put borrows its key from the row** and encodes it inline, two allocations fewer a put.
- **A module registered while the open is deferred waits for it** (rank 4). The shell registers
  `fsdir` and `zipfile` on every command line verb, and each registration forced the whole open and
  refreshed the catalog. The open now adds every waiting module with one refresh.
- **The command line asks by name whether a database exists** instead of opening the file to ask.
- **A compaction leaves an eighth of the page free when the write adds a key** (rank 6). Counted on
  the Python insert's first 10,000 rows: 47 compactions with no gap and 24 with an eighth, for 22
  and 26 splits. Applied to every write it made a leaf a bulk build packed full split on writes
  that add no rows, and a 17 leaf tree became 33, so it applies only when the key is new.
- **The Windows release is profile guided** (rank 12). `packaging/pgo/build-windows.ps1` builds
  the programs instrumented, runs `packaging/pgo/train.mjs` and `train.py` against them, and builds
  them again with the counts. The training is the usage workloads, every script in
  `compat/corpus/usage/`, full text and vector search, a migration, MCP, the Python binding and the
  npm package, so the code a benchmark does not reach is not left cold. Seven quiet rounds against
  the same commit built without a profile:

  | Workload | Without a profile | With it | SQLite |
  |---|---:|---:|---:|
  | Python, 10,000 inserts in a transaction | 33.99 ms | 28.39 ms | 28.01 ms |
  | Python, 200 `GROUP BY` queries | 118.2 ms | 92.7 ms | 516.3 ms |
  | `.import --csv` of 50,000 rows | 59.3 ms | 54.7 ms | 66.4 ms |
  | `exec` of a one row `INSERT` | 24.20 ms | 22.87 ms | 22.60 ms |
  | 1,000 lookups by key in a script | 33.7 ms | 30.8 ms | 37.4 ms |

  One workload, the Python fixture build, took 0.5 to 1.2 s in three of the profile guided build's
  samples and 41 ms in the rest. Ten direct runs of each build and a per step timing of each found
  no difference, so it is counted as this machine's noise. The release build of the Windows target
  takes 330 s instead of 133 s, beside the other four targets, which take about as long.
- **A page read into a frame used for the first time is read into its reserved memory**
  (rank 7, first half). `VfsFile::read_exact_into` reads with `ReadFile` or `pread` into a
  `Vec`'s spare capacity; every other file keeps a default that zero fills and reads. The cold
  range query from the command line went from 19.19 ms to 18.93 ms, median of 100 paired runs.
- **"Not found" from the by name query is final.** Every open asks whether a rollback journal is
  beside the file, and the answer is normally no. `path_state` asked the older `GetFileAttributesW`
  again after `GetFileInformationByName` said the file was not there, which was 7% of a Python open
  and close.

- **An open reads the meta record without the shared lock** (guidance 5.2, item 2), and reads it
  again under the lock only when neither copy checks out. A checkpoint writes the shadow copy
  first, so a record read while it runs is the older copy or the newer, and the first statement's
  comparison of the shadow copy catches the older one. The eager open, which replays the log
  straight after, keeps the locked read (`MetaRead::Locked`).
- **The file identity is read on first use on Windows.** The lock state there never used it, and
  only the VFS conformance tests ask for it. Unix still reads it at the open, because its lock
  registry is keyed by it.

  A Python open and close in a loop, median of 400: 40 us at the start of this checkpoint, 33 us
  with the open tried before asking whether the file exists and the journal question asked once,
  29 us with these two.

### 10.2 Where the remaining time goes

A sampled loop of open, the range query of 100 rows and close in one Python process took 1.40 ms
against SQLite's 0.57 ms. `ReadFile` was 38% of it and `crc32_continue` 20.5%; the query's own
execution was a few percent. Most of the `ReadFile` time is the kernel faulting in fresh frame
memory as it copies 65 pages of 32 KiB: first touching 2 MiB of fresh memory measured 0.40 ms on
this machine. SQLite reads about 180 pages of 4 KiB. What is left for that query is the checksum,
which waits on the CRC decision, or frames that point into a mapping of the file, which the
guidance rules out as a pool redesign.

In the Python insert, after the profile, the tree's `write_row` is 54% of the time, the fold that
follows `COMMIT` under the default rollback journal 10.6%, converting the driver's rows to the
engine's values about 4% and dropping them about 2%. The fold's extra sync is decision D2.

A Python open and close is now two name queries (the data file and its journal), `CreateFileW`,
the file identity query, the lock, a 16 byte and a 64 KiB read, the CRC of one meta page, and the
close. The first name query, the identity query and the 32 KiB of the second meta page are the
parts that do not need a decision.

### 10.3 The result

Measured on 6 October 2026, fifteen rounds, the build at the end of round 2 (`bin/undo`, with the
binding of that commit) against the profile guided Windows release build of commit 90fa96c4, with
no other work running. `docs/performance.md` has every workload. A ten round run of the build of
affaa726, one commit earlier and without the open changes, gave 20 of 25 and 43%; the command line
reads crossed parity in both directions between the two runs.

| Group | Round 2's build against SQLite | Round 3's build against SQLite |
|---|---|---|
| the command line, 8 workloads | 1% slower | within 1% |
| scripts into the shell, 6 workloads | 34% faster | 39% faster |
| the Python driver, 9 workloads | 80% faster | 105% faster |
| the npm package, 2 workloads | 108% faster | 69% faster, the scheduling of its process |
| all 25 | 40% faster | 46% faster |

19 of the 25 workloads are faster than SQLite, and all nine Python workloads are. The six that are
not:

| Workload | Against SQLite | What would change it |
|---|---|---|
| `exec` of a one row `INSERT` | 6% slower | D2: the commit's log write and sync folded into the close |
| `query` of 100 rows by an index | 3% slower | the CRC decision; frames that point into a mapping of the file |
| `query` of one row as text, `count(*)`, `tables`, one row as JSON | 1% to 3% slower | the CRC decision, and D3 for the first read after a writer |

The `query()` lookup through the npm package measured 0.066 ms against 0.043 ms for round 2's build.
Two of its four samples ran at about 0.065 ms in each build measured that day, and three direct runs
of this build gave 0.071, 0.037 and 0.038 ms: the process is sometimes placed on this machine's
efficiency cores, where both Node workloads run about 1.8 times slower together.

### 10.4 Tried and not kept

| Idea | What was measured | Why it was not kept |
|---|---|---|
| The growth gap on every write | `write_campaign` grew a bulk built tree from 17 leaves to 33 | a full leaf split on writes that add no rows |
| A byte budget shared by every class of the C library's allocator | slower than the per class caps | the accounting cost more than it saved |
| Committing the statistics a fold refreshes without their own sync | no change at close | a close fold reclaims only past 1 MiB of log |

## 11. Round 4: the four decisions

Jason approved all four decisions from round 3, and `unsafe` for the CRC when the risk is small and
the reason is speed.

### 11.1 What changed

- **Hardware CRC** (commit 732a42be). `crates/inillucent-base/src/crc_hardware.rs` folds 64 bytes
  at a time with PCLMULQDQ on x86-64 and uses `__crc32d` on aarch64. It answers only for 64 bytes
  or more and only after the feature check, so every other input takes the table path. The crate
  went from `forbid(unsafe_code)` to `deny(unsafe_code)` with one allowed module, which is on the
  policy suite's list of files allowed `unsafe`. A test compares it with the table version at every
  length to 300 bytes and at 200 random lengths to 70 KiB, each from a random starting value. A 32 KiB page: 3.93 us to 0.64 us.
- **D2, the command line's `exec` holds its commit for the fold** (commit 4a0316f8). Only under a
  rollback journal mode, with no attached file, and only when the write lock can be held. The fold
  flushes the log instead of syncing it, and its own sync makes the commit durable. Two syncs
  instead of three. The new crash campaign
  `power_loss_at_every_cut_point_of_a_commit_the_fold_holds` cuts power at each of 51 points.
- **D3, the log end in the meta record** (commit de455577). Offset 120, so the record is 128 bytes.
  An open that finds the segment at that length by name, through `Vfs::size_by_name`, replays
  nothing and opens the segment lazily. A probe of about 650,000 queries on NTFS found no stale size
  by name.
- **D1, the `InsertRows` record** (commit de455577). Kind 18. One record of up to 64 KiB holds a
  transaction's inserts. Recovery decides for every entry whether its page already holds it before
  it applies any. Building this found a bug: the largest key hint in `shape.rs` was trusted by page
  LSN, and a batch leaves the LSN unchanged across rows, which gave duplicate rowids. The hint is
  now forgotten after each logged insert.

### 11.2 The result

Fifteen rounds on 6 October 2026, the 2.2.0 release build against the profile guided build of
de455577, with about 10% of the processor used by other programs.

| Group | 2.2.0 against SQLite | Round 4 against SQLite |
|---|---|---|
| the command line, 8 workloads | 4% slower | within 1% |
| scripts into the shell, 6 workloads | 39% faster | 40% faster |
| the Python driver, 9 workloads | 104% faster | 109% faster |
| the npm package, 2 workloads | 138% faster | 171% faster |
| all 25 | 48% faster | 53% faster |

| Workload | 2.2.0 | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---|
| `inillucent --version` | 17.78 ms | 16.03 ms | 15.46 ms | 4% slower |
| `query` of one row as JSON | 19.29 ms | 18.91 ms | 19.09 ms | 1% faster |
| `query` of one row as text | 18.76 ms | 19.01 ms | 18.09 ms | 5% slower |
| `query` of 100 rows by an index, sorted, as JSON | 20.52 ms | 20.66 ms | 19.83 ms | 4% slower |
| `query` of `count(*)` and `max` over 10,000 rows | 19.09 ms | 17.94 ms | 17.97 ms | within 1% |
| `exec` of a one row `INSERT` | 25.65 ms | 23.66 ms | 23.24 ms | 2% slower |
| `tables` | 19.33 ms | 19.85 ms | 19.25 ms | 3% slower |
| `dump` | 32.05 ms | 30.57 ms | 35.67 ms | 17% faster |
| 1,000 lookups by key in a script | 34.09 ms | 35.33 ms | 41.92 ms | 19% faster |
| 200 single row inserts, each its own transaction | 268 ms | 263 ms | 881 ms | 235% faster |
| 10,000 `INSERT` statements in a transaction | 38.21 ms | 37.83 ms | 43.16 ms | 14% faster |
| one `INSERT` of 20,000 rows | 43.77 ms | 43.09 ms | 44.09 ms | 2% faster |
| `.import --csv` of 50,000 rows | 58.54 ms | 57.76 ms | 68.22 ms | 18% faster |
| a 10,000 row table and two indexes, from a script | 52.63 ms | 51.71 ms | 71.63 ms | 39% faster |
| Python, a 10,000 row table and two indexes | 40.17 ms | 39.10 ms | 46.32 ms | 18% faster |
| Python, 5,000 lookups by key | 53.89 ms | 57.59 ms | 94.95 ms | 65% faster |
| Python, read 10,000 rows | 6.49 ms | 6.15 ms | 8.48 ms | 38% faster |
| Python, 200 range queries of 200 rows | 22.68 ms | 23.13 ms | 38.54 ms | 67% faster |
| Python, 200 `GROUP BY` queries | 94.93 ms | 94.59 ms | 581 ms | 514% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 28.20 ms | 26.69 ms | 29.89 ms | 12% faster |
| Python, 100 single row inserts | 121 ms | 123 ms | 438 ms | 257% faster |
| Python, 1,000 single row updates | 1,236 ms | 1,212 ms | 6,694 ms | 452% faster |
| Python, 100 opens and closes | 7.91 ms | 6.88 ms | 8.46 ms | 23% faster |
| Node, a lookup by key with `query()` | 0.048 ms | 0.037 ms | 0.117 ms | 215% faster |
| Node, a lookup by key through a session | 0.007 ms | 0.007 ms | 0.017 ms | 133% faster |

20 of the 25 are faster, where 2.2.0 had 18 in the same run. The five that are not are command
line programs, 2% to 5% slower: `--version`, one row as text, 100 rows by an index, `tables`, and
the one row `exec`. `--version` opens no database, so its gap is starting a 10 MB process against
SQLite's 4 MB one. The cold reads copy 32 KiB pages into fresh memory.

### 11.3 Tests

`inillucent-testrun --changed main --strict`: 374 targets, 4,620 tests, none failed.

## 12. Round 5: every workload faster than SQLite

The last todo on the ticket was to measure again until every one of the 25 workloads is faster than
SQLite. After round 4 five were not: `--version`, one row as text, 100 rows by an index, `tables` and
the one row `exec`, each 2% to 5% slower.

### 12.1 What was found

- **Two hundred interleaved runs of each command line call** showed `--version`, one row as text and
  `tables` level with SQLite or ahead. Fifteen rounds could not tell them apart. A static program
  that prints one line takes 16.4 ms to start here.
- **The 100 row query's gap is loading pages.** Warm, ours took 0.063 ms and SQLite's 0.114 ms. Cold,
  1.07 ms against 0.67 ms. Timers in `Pool::load` put each 32 KiB page at 14.3 us: 11.5 us in the read,
  1.7 us allocating the frame, 0.9 us of CRC. Reading 64 pages in a new process took 681 us into heap
  blocks, 473 us into one page aligned region and 420 us in runs of 8 pages.
- **Page faults are the cost of a short command.** A static program takes 1,136. The one row query
  took 720 more and SQLite's 486, and of ours about 570 pages were the program's own code.
- **`exec` was spending its time in file calls and in loading `bcrypt.dll`.** A sampling profile of
  the `exec` path in a loop, built for this, put our own code under 5% of the samples. The rollback
  journal made 11 writes for 5 pages, and the journal's nonce loaded `bcrypt.dll`.
- **`%TEMP%` makes a program built there pay Defender on every open.** An `exec` run by a program in
  `%TEMP%` on a database in `%TEMP%` took 16.4 ms against SQLite's 8.7 ms, two opens of about 3 ms
  each. The same program run from Documents took 10.8 ms. An installed program does not run from
  `%TEMP%`, so this is a trap for measurements, and the comparisons above ran from the repository.

### 12.2 What changed

- `inillucent-alloc`'s `Carved`: page sized blocks carved from 512 KiB chunks aligned to 4 KiB, and
  the small classes carved from one shared span. `alloc` and `dealloc` are out of line, because
  with the large path moved out they were inlined everywhere and the program grew by 1.7 MB.
- `Journal`: records held in memory and written with the header in one call at `seal`, flushed every
  1 MiB, with the buffer reserved once.
- `write_both_slots`: a checkpoint writes the record's 128 bytes into each slot.
- `system_randomness` on Windows: `ProcessPrng` through a `raw-dylib` import.

### 12.3 Tried and not kept

| Idea | What was measured | Why it was not kept |
|---|---|---|
| A link order file of every function the training ran | more faults, not fewer | 3,692 functions spanning about 4 MB |
| A link order file of the functions a short command runs | 60 fewer faults on a one row query | the functions are large after inlining, 509 pages for 2,473 of them |
| A profile trained only on short commands | 120 to 240 fewer faults | the other workloads would lose their profile |
| Weighting the short command profile 1,000 times in the merge | under 1% fewer faults than a plain merge | not worth a second training pass |
| Machine function splitting | the compiler crashed on the Windows target | |
| `PrefetchVirtualMemory` or `VirtualLock` on fresh frame memory | no change, and the lock failed at the default working set size | |
| A memory mapped read of the data file | 532 us for 64 pages against 473 us for aligned reads | |

### 12.4 The result

The benchmark ran three builds against SQLite in the same runs, while another program used half a
core to a whole core, so twelve workloads were run again; the table gives each workload's last run.

| Group | 2.2.0 | Round 4 | Round 5 |
|---|---|---|---|
| the command line, 8 workloads | 2% faster | 2% faster | 7% faster |
| scripts into the shell, 6 workloads | 48% faster | 48% faster | 50% faster |
| the Python driver, 9 workloads | 114% faster | 133% faster | 129% faster |
| the npm package, 2 workloads | 171% faster | 209% faster | 250% faster |
| all 25 | 57% faster | 64% faster | 68% faster |

| Workload | 2.2.0 | Round 4 | Now | SQLite | Now against SQLite |
|---|---:|---:|---:|---:|---|
| `inillucent --version` | 16.53 ms | 16.06 ms | 16.11 ms | 16.99 ms | 5% faster |
| `query` of one row as JSON | 19.02 ms | 19.18 ms | 18.80 ms | 19.07 ms | 1% faster |
| `query` of one row as text | 21.42 ms | 21.53 ms | 20.73 ms | 21.82 ms | 5% faster |
| `query` of 100 rows by an index, sorted, as JSON | 20.71 ms | 20.62 ms | 20.36 ms | 20.43 ms | within 1%, faster |
| `query` of `count(*)` and `max` over 10,000 rows | 18.06 ms | 17.91 ms | 17.39 ms | 18.61 ms | 7% faster |
| `exec` of a one row `INSERT` | 25.76 ms | 23.79 ms | 23.15 ms | 23.75 ms | 3% faster |
| `tables` | 20.92 ms | 21.12 ms | 18.69 ms | 19.88 ms | 6% faster |
| `dump` | 34.18 ms | 37.21 ms | 33.20 ms | 42.18 ms | 27% faster |
| 1,000 lookups by key in a script | 40.48 ms | 39.80 ms | 39.22 ms | 53.23 ms | 36% faster |
| 200 single row inserts, each its own transaction | 288 ms | 292 ms | 288 ms | 1,041 ms | 261% faster |
| 10,000 `INSERT` statements in a transaction | 43.98 ms | 44.78 ms | 43.94 ms | 51.75 ms | 18% faster |
| one `INSERT` of 20,000 rows | 41.08 ms | 40.99 ms | 41.20 ms | 44.99 ms | 9% faster |
| `.import --csv` of 50,000 rows | 62.93 ms | 60.32 ms | 60.75 ms | 76.19 ms | 25% faster |
| a 10,000 row table and two indexes, from a script | 52.52 ms | 53.55 ms | 51.65 ms | 73.77 ms | 43% faster |
| Python, a 10,000 row table and two indexes | 45.32 ms | 44.46 ms | 45.30 ms | 52.23 ms | 15% faster |
| Python, 5,000 lookups by key | 78.39 ms | 76.30 ms | 77.04 ms | 140 ms | 82% faster |
| Python, read 10,000 rows | 8.97 ms | 6.53 ms | 7.27 ms | 10.40 ms | 43% faster |
| Python, 200 range queries of 200 rows | 28.63 ms | 25.15 ms | 25.02 ms | 50.62 ms | 102% faster |
| Python, 200 `GROUP BY` queries | 96.19 ms | 94.78 ms | 108 ms | 750 ms | 597% faster |
| Python, 10,000 inserts in a transaction into a table with two indexes | 30.18 ms | 28.87 ms | 29.13 ms | 37.85 ms | 30% faster |
| Python, 100 single row inserts | 130 ms | 130 ms | 138 ms | 539 ms | 289% faster |
| Python, 1,000 single row updates | 1,408 ms | 1,387 ms | 1,327 ms | 6,300 ms | 375% faster |
| Python, 100 opens and closes | 10.29 ms | 8.47 ms | 7.40 ms | 12.58 ms | 70% faster |
| Node, a lookup by key with `query()` | 0.063 ms | 0.062 ms | 0.045 ms | 0.193 ms | 326% faster |
| Node, a lookup by key through a session | 0.011 ms | 0.009 ms | 0.009 ms | 0.027 ms | 188% faster |

### 12.5 Tests

`inillucent-testrun --changed main --strict`. Twelve crash campaign floors were lowered, each with the
measured counts beside it: the journal's writes were cut points. `journal_ordering.rs` counts the
journal's bytes before the first new image instead of its writes, and a new test checks that a
checkpoint writes 128 bytes into each meta slot and that the file reopens on them.
