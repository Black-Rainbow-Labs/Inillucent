# task-2197: a fifth performance hill climb, aiming at three times SQLite's speed

## 1. What was asked

task-2191 left all 25 usage workloads faster than SQLite. This ticket asked for 200% faster, a
ratio of 3.0 (SQLite's time divided by inillucent's), on every read and write, by the same hill
climb: measure, change one thing, measure again, keep what helps.

The eval is task-2191's usage benchmark, kept with this ticket's copies under
`_agent_output/task-2197/bench/` (gitignored): 8 command line workloads (one process a command), 6
SQL scripts piped into the shell, 9 Python driver workloads and 2 npm package workloads, each
against SQLite 3.53.4 used the same way. One arm was added, described in section 2.

## 2. A floor no program can go under

Starting any program from Node on this machine costs about 13 to 16 ms. An empty Rust program
(`hello.exe`) measured 13.6 ms median over 40 starts and `sqlite3 -version` 13.3 ms. SQLite's
command line answers a one row query in 17 to 21 ms. For inillucent to be 200% faster on that call
it would have to finish in about 6 ms, which is less than Windows takes to start an empty program.

So for the 14 workloads that start a process, a ratio of 3.0 in wall time cannot be reached by any
program. The benchmark now times an empty program in every round (`floor.empty`) and reports each
process workload twice: the wall time ratio, and the ratio of the time each engine takes above the
empty program. The second is the number that can move.

**Most of that floor is the benchmark's way of starting programs.** It passes Node's
`windowsHide`, which gives every child a hidden console of its own, about 11 ms a start. Started the
way a terminal starts them, sharing the parent's console, an empty program takes 5.7 ms, SQLite's
one row query 7.95 ms and inillucent's 7.55 ms. A ratio of 3.0 would then need the query in 2.5 ms,
still under the empty program. Section 4 gives the process workloads measured both ways.

Python's open and close has a floor of its own: a bare `CreateFileW` and `CloseHandle` on the same
file from Python cost 11.5 us, against 66 us for Python's `sqlite3.connect` and `close`.

## 3. The changes

Each was measured against the build before it on this machine, and the profiles are under
`_agent_output/task-2197/prof/`.

1. **One foreign call per Python execution** (C ABI 1.4.0). A point query from Python made four
   `ctypes` calls, with a `json.dumps` of its parameters in front of them: bind the JSON, run, build
   the result, free the result. `inillucent_py_stmt_execute` reads the parameters out of their
   Python objects with the CPython readers `execute_many` already uses, runs the statement with the
   interpreter lock let go (`PyEval_SaveThread`, the way the `ctypes.CDLL` call it replaces let it
   go), and answers the result list. A failure comes back as the address of the
   `inillucent_error` the binding raises from, and a value it does not read comes back as `None`,
   so the binding binds that execution as JSON exactly as before. `inillucent_py_execute` is the
   same for a statement with no parameters. Point query from Python: **17.25 us to 6.25 us a
   call.**
2. **A read keeps its shared lock for the next statement** (`inillucent-pool/src/lease.rs`). With
   the Python call fixed, a profile of the point query showed four kernel calls around every
   statement: take SHARED, read the shadow meta record to learn whether another process folded,
   ask the open log segment its length, let SHARED go. They were 6.4 us of the call, against
   0.9 us for the lookup. A statement that only read now keeps SHARED for up to 1 ms with nothing
   running and 250 us in all. While SHARED is held nothing can fold, because a fold needs
   EXCLUSIVE, so the next statement reads no meta record and takes no lock. It still asks the log
   its length, because a writer appends to the log under RESERVED beside a reader. A thread lets
   an idle lease go, a statement that finds 250 us have passed lets the lock go and takes it again
   the ordinary way (which waits behind a writer holding PENDING), and a connection in the same
   process whose raise is refused lets the other connections' leases go and tries once more.
   Point query from Python: **6.25 us to 3.3 us a call.** Python's `sqlite3` takes 18 to 22 us.
3. **The shell writes its output in blocks.** Rust's `stdout()` flushes at every newline, so the
   shell made one write to the pipe for every row. A script of 60,000 point queries spent 62% of
   its time in those writes. When standard output is not a terminal the shell now holds its
   output and writes 64 KiB at a time. It writes what it holds before anything goes to standard
   error, so both streams on one pipe keep their order, and before it reads more input than it
   already has, so a program that sends a statement and waits for the answer gets it. A panic
   writes it out before the process aborts. 60,000 point queries: **340 ms to 170 ms**, against
   1,559 ms for SQLite's shell.
4. **The sort keys of an `ORDER BY` share one buffer.** Each row's key was its own `Vec<u8>`, an
   allocation and a free per row. The keys now go into one buffer, and each is compared first by
   its leading 16 bytes as a number, then by the whole key when those are equal.
5. **An interior page's interpolation step is done in 64 bits.** It multiplied in 128 bits and
   divided, which compiled to a call to `__udivti3`, 2% of a range query. The midpoint is a guess
   the comparison then checks, so the bits it drops cannot change which child is found. With item
   4, 200 Python range queries went from 47.5 ms to 44.7 ms, the mean of two alternations.


## 4. The result

Measured on 6 October 2026, fifteen rounds, release 2.2.0, the round 5 build of `main` (01a5c2ec)
and this ticket's build against SQLite 3.53.4 in the same runs, all three profile guided as
`packaging/release-all.ps1` makes them, with no other work running. A ratio is SQLite's time
divided by inillucent's; 3.0 is the goal.

| Group | 2.2.0 | `main` | Now |
|---|---|---|---|
| the command line, 8 | 1.01 | 1.04 | 1.03 |
| the shell, 6 | 1.38 | 1.41 | 1.46 |
| the shell, 6, above the empty program | 1.52 | 1.55 | 1.69 |
| Python, 9 | 2.07 | 2.06 | 2.36 |
| npm, 2 | 2.14 | 2.26 | 3.07 |
| all 25 | 1.50 | 1.52 | 1.65 |
| workloads at 3.0 or more | 4 | 4 | 6 |

| Workload | `main` | Now |
|---|---:|---:|
| `inillucent --version` | 1.03 | 0.99 |
| `query` of one row as JSON | 1.06 | 1.00 |
| `query` of one row as text | 0.99 | 0.99 |
| `query` of 100 rows by an index, sorted, as JSON | 1.01 | 0.99 |
| `query` of `count(*)` and `max` | 1.02 | 0.99 |
| `exec` of a one row `INSERT` | 1.03 | 1.06 |
| `tables` | 1.03 | 1.02 |
| `dump` | 1.17 | 1.18 |
| 1,000 lookups by key in a script | 1.30 | 1.59 (2.60 above the floor) |
| 200 single row inserts, each its own transaction | 3.19 | 3.22 |
| 10,000 `INSERT` statements in a transaction | 1.09 | 1.07 |
| one `INSERT` of 20,000 rows | 1.07 | 1.09 |
| `.import --csv` of 50,000 rows | 1.22 | 1.20 |
| a 10,000 row table and two indexes, from a script | 1.33 | 1.36 |
| Python, a 10,000 row table and two indexes | 1.20 | 1.25 |
| Python, 5,000 lookups by key | 1.79 | 4.47 |
| Python, read 10,000 rows | 1.34 | 1.34 |
| Python, 200 range queries of 200 rows | 1.96 | 2.21 |
| Python, 200 `GROUP BY` queries | 5.54 | 5.53 |
| Python, 10,000 inserts in a transaction into a table with two indexes | 1.24 | 1.25 |
| Python, 100 single row inserts | 3.66 | 3.68 |
| Python, 1,000 single row updates | 5.06 | 5.05 |
| Python, 100 opens and closes | 0.93 | 1.05 |
| Node, a lookup by key with `query()` | 2.58 | 2.81 |
| Node, a lookup by key through a session | 1.97 | 3.37 |

The times behind every ratio are in `docs/performance.md`, "A sixth round of the same
comparison", and in `_agent_output/task-2197/final.json`.

**The benchmark's process floor is mostly its own.** It starts every child with Node's
`windowsHide`, which gives each child a hidden console of its own and costs about 11 ms a start.
Started the way a terminal or a script starts them, an empty program takes 5.7 ms, SQLite's one
row query 7.95 ms and inillucent's 7.55 ms (40 starts each). The 14 process workloads were run that
way too (`final-console.json`, 15 rounds): the command line group came out at 0.97, 1.05 and 1.04
for 2.2.0, `main` and now, and the shell group at 1.46, 1.49 and 1.56 (1.53, 1.56 and 1.68 above the
empty program).

## 5. What is still under 3.0, and why

| Workloads | Now | Why | What would move it |
|---|---|---|---|
| the 8 command line calls | 0.99 to 1.18 | A one row query is 7.5 ms from a terminal and an empty program 5.7 ms; 3.0 would need 2.5 ms. | Nothing in the engine. A client that keeps a process, as the npm session and the MCP server do. |
| the shell's inserts, `.import` and fixture | 1.07 to 1.36 | The write of a row into the tree and allocation. In a profile of the Python insert into the table with two indexes, writing rows into the trees is 61% of the time, a third of that rewriting leaves whose delta area filled, and the allocator 10%. 10,000 rows into a table with no index take 14.2 ms from Python against SQLite's 12.2 ms. | An append path for rows that arrive in key order that packs leaves directly, and index entries applied in key order once a statement's rows are known. Each is its own ticket, with its own crash campaign. |
| Python fixture build and 10,000 inserts | 1.25 | The same write path. | The same. |
| Python full scan | 1.34 | The engine copies every cell of a result into its own allocation before the C library builds the Python value; about 30% of the scan is the Windows heap serving and freeing those copies. Skipping the driver's own conversion made no difference (section 6). | Handing rows to the caller as the plan produces them, borrowed from the pages, so nothing is copied twice. That means the final sink of a cached compiled plan has to be replaceable for one run without catching the rows of statements nested inside it. |
| Python range queries | 2.21 | A lookup of the table row for each index entry, a sort, and the same copies. | The same streaming, and a leaf hint for lookups that arrive in rowid order. |
| Python open and close | 1.05 | One open, a 64 KiB read of both meta pages (their checksum covers the whole page), a check by name for a rollback journal and a close: 68 us against SQLite's 72 us. A bare open and close costs 11.5 us. | Deferring the journal check to the first statement, which is when SQLite makes it. |
| Node `query()` | 2.81 | Each call checks the file's identity by name, then runs the statement in the addon. | Close; not worked on this round. |
| the shell's 1,000 lookups | 1.59, 2.60 above the floor | Parsing and planning each statement's text, which differs in its literal every time, plus the floor. | Not worked further. |

## 6. Tried and not kept

- **A larger free list for the library's allocator.** `Shared` kept 64 KiB of each size class;
  keeping 4 MiB a class and 32 MiB in all was meant to stop a burst of 10,000 Python rows going to
  the Windows heap. 10,000 inserts from Python got slower: 13.7 ms to 16.1 to 17.5 ms with no
  index, 28.7 to 29.5 ms to 33.3 to 34.4 ms with two, over two alternations of five runs. A block
  taken back off a long intrusive free list is a cold cache line, read for its link, where the
  Windows heap hands out a burst side by side.
- **The command line's carving allocator in the library.** Not tried: its lists are per thread,
  so a block freed on one Python or Node thread stays on that thread's list, and a host that ends
  threads would leak what they held.
- **A shorter lease for a writer beside a busy reader.** A Python process running `SELECT`s back to
  back beside a process running one row `exec`s made each `exec` wait 3 to 7 ms for EXCLUSIVE.
  The lease limit made no difference: 50 us, 250 us and 4 ms all measured the same, and so did the
  lease switched off. The wait is the reader replaying the writer's commit from the log under
  SHARED before the writer can fold. With the lease, a busy reader catches the commit on most
  `exec`s rather than on some, so the writer's median went from 12.8 ms to 19 ms. A writer beside
  an idle reader, or one that reads now and then, does not meet it.
- **The Python result built from the engine's rows.** `Connection::query_raw` moved each engine
  row out of the statement whole, and the C library built the Python values from it, skipping the
  driver's conversion to its own values: a new list a row and a UTF-8 check of every text. Three
  alternations of a 10,000 row scan and 200 range queries: scan 10.3 to 11.3 ms before and 10.4 to
  10.9 ms after, range 44.8 to 45.4 ms before and 44.4 to 46.0 ms after. No difference, so it was
  taken out again. The engine's own copy of every cell, and the heap work it causes, is the cost.
- **Reading less of the meta pages at an open.** The open reads both 32 KiB meta pages, 10 us of a
  62 us Python open and close. The record's checksum covers the whole page, so reading less would
  change the file format.


## 7. Tests

- `crates/inillucent-pool/tests/lease.rs`: a lease holds SHARED against another handle and the
  releasing thread lets it go once idle; a writer in the same process with a busy timeout of zero
  takes the lease away; a statement that follows claims the lease and nothing gets in between.
- `crates/inillucent-compat/tests/e2e/shell_output.rs`: rows and errors on one pipe keep their
  order; a program that waits for each answer gets it; 100,000 lines of output arrive whole.
- `drivers/bindings/python/run_conformance.py`: the suite's 34 cases run through the one call path,
  and `check_one_call` runs every value type, a constraint failure, a syntax error, too many
  values, values neither path can bind and a statement with no parameters through both paths and
  compares the answers and the errors.
- Five crash campaigns cover two fewer cut points, because the statement after a read takes no
  lock and reads no meta record. Each floor was lowered only after running the campaign with the
  lease switched off and getting the old count back (43, 39, 43, 43, 43 off; 41, 37, 41, 41, 41
  on). Every cut point still recovers. `recovery-crash.txt` crashes the workload at call 24 and
  then crashes its recovery at each of twelve calls. Call 24 is now two calls later in the
  commit, so all twelve of those rows read `new` where they read `old`; the campaign accepts
  either, and its row count is unchanged.
- `inillucent-testrun --changed main --strict`: 376 targets, 4,630 tests. It found three suites to
  fix, all in what this ticket wrote: `release_if_idle` past 150 lines, two new test modules
  without an invariant line and an early return in one, the test tier counts, two phrases in the
  performance page, and the ABI test's list of provisional symbols. After the fixes those five
  suites and the two new ones pass. The suites that need MySQL, PostgreSQL, the network, Go or the
  gate fixtures ran without them, as on `main`. `cargo clippy --workspace --all-targets
  --all-features --locked -- -D warnings` is clean on Windows.
