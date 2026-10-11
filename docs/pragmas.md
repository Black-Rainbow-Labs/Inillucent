# Pragmas

A pragma is a statement that reads or changes a setting of the database, such
as `PRAGMA page_size` or `PRAGMA busy_timeout = 2000`. This page lists every
pragma the engine recognises.

This page is generated. `cargo run -p inillucent-compat --bin inillucent-obligations`
writes it from `inillucent_sql::pragma_register::REGISTER`, and
`cargo test -p inillucent-compat --test tooling harness::` fails when the page and the
register differ. Do not edit it by hand.

There are **71 pragmas**. 64 of them take an argument in parentheses.

A pragma that is not in this table is not recognised. It returns no rows and no
error, which is also what SQLite does. So running a pragma does not tell you
whether the engine knows it. Check this table instead.

The table gives each pragma's name, whether it takes an argument, and the
columns of the rows it returns. [SQL support](sql.md) describes what the
pragmas do.

| pragma | takes an argument | columns of its answer |
|---|---|---|
| `analysis_limit` | yes | one unnamed column |
| `application_id` | yes | one unnamed column |
| `auto_vacuum` | yes | one unnamed column |
| `automatic_index` | yes | one unnamed column |
| `busy_timeout` | yes | `timeout` |
| `cache_size` | yes | one unnamed column |
| `cache_spill` | yes | one unnamed column |
| `case_sensitive_like` | yes | one unnamed column |
| `cell_size_check` | yes | one unnamed column |
| `checkpoint_fullfsync` | yes | one unnamed column |
| `collation_list` | no | `seq`, `name` |
| `compile_options` | no | `compile_options` |
| `count_changes` | yes | one unnamed column |
| `data_store_directory` | yes | one unnamed column |
| `data_version` | yes | one unnamed column |
| `database_list` | no | `seq`, `name`, `file` |
| `default_cache_size` | yes | one unnamed column |
| `defensive` | yes | one unnamed column |
| `defer_foreign_keys` | yes | one unnamed column |
| `empty_result_callbacks` | yes | one unnamed column |
| `encoding` | yes | `encoding` |
| `encryption` | no | `encryption` |
| `foreign_key_check` | yes | `table`, `rowid`, `parent`, `fkid` |
| `foreign_key_list` | yes | `id`, `seq`, `table`, `from`, `to`, `on_update`, `on_delete`, `match` |
| `foreign_keys` | yes | one unnamed column |
| `freelist_count` | yes | one unnamed column |
| `full_column_names` | yes | one unnamed column |
| `fullfsync` | yes | one unnamed column |
| `function_list` | no | `name`, `builtin`, `type`, `enc`, `narg`, `flags` |
| `hard_heap_limit` | yes | one unnamed column |
| `ignore_check_constraints` | yes | one unnamed column |
| `incremental_vacuum` | yes | one unnamed column |
| `index_info` | yes | `seqno`, `cid`, `name` |
| `index_list` | yes | `seq`, `name`, `unique`, `origin`, `partial` |
| `index_xinfo` | yes | `seqno`, `cid`, `name`, `desc`, `coll`, `key` |
| `integrity_check` | yes | `integrity_check` |
| `journal_mode` | yes | `journal_mode` |
| `journal_size_limit` | yes | one unnamed column |
| `key` | yes | one unnamed column |
| `legacy_alter_table` | yes | one unnamed column |
| `locking_mode` | yes | `locking_mode` |
| `max_page_count` | yes | one unnamed column |
| `mmap_size` | yes | one unnamed column |
| `module_list` | no | `name` |
| `optimize` | yes | one unnamed column |
| `page_count` | yes | one unnamed column |
| `page_size` | yes | one unnamed column |
| `pragma_list` | no | `name` |
| `query_only` | yes | one unnamed column |
| `quick_check` | yes | `quick_check` |
| `read_uncommitted` | yes | one unnamed column |
| `recursive_triggers` | yes | one unnamed column |
| `rekey` | yes | one unnamed column |
| `reverse_unordered_selects` | yes | one unnamed column |
| `schema_version` | yes | one unnamed column |
| `secure_delete` | yes | one unnamed column |
| `short_column_names` | yes | one unnamed column |
| `shrink_memory` | yes | one unnamed column |
| `soft_heap_limit` | yes | one unnamed column |
| `synchronous` | yes | one unnamed column |
| `table_info` | yes | `cid`, `name`, `type`, `notnull`, `dflt_value`, `pk` |
| `table_list` | yes | `schema`, `name`, `type`, `ncol`, `wr`, `strict` |
| `table_xinfo` | yes | `cid`, `name`, `type`, `notnull`, `dflt_value`, `pk`, `hidden` |
| `temp_store` | yes | one unnamed column |
| `temp_store_directory` | yes | one unnamed column |
| `threads` | yes | one unnamed column |
| `trusted_schema` | yes | one unnamed column |
| `user_version` | yes | one unnamed column |
| `wal_autocheckpoint` | yes | `wal_autocheckpoint` |
| `wal_checkpoint` | yes | `busy`, `log`, `checkpointed` |
| `writable_schema` | yes | one unnamed column |

## Two defaults that matter when several processes share a file

`busy_timeout` starts at **5000** milliseconds. When another process holds the
file, a statement waits up to this long and then fails with `busy`. Set
`busy_timeout` to 0 to make the statement fail at once. `busy_timeout` applies to
waits between processes and to waits inside one process.

Opening a file does not wait for another process. The open reads the meta record
without a lock, and the first statement takes the lock and waits if it has to.
`PRAGMA busy_timeout` cannot be run before the open has finished, so three things
set the value a database starts with as its `busy_timeout`:

| Where | How |
|---|---|
| any program, any language | the environment variable `INILLUCENT_BUSY_TIMEOUT`, in milliseconds |
| the Rust driver | `OpenOptions { busy_timeout: Some(Duration::from_secs(60)), .. }` |
| the C library | `inillucent_open_with_timeout(path, flags, 60000, &db, &error)` |
| the Python binding | `Database(path, busy_timeout_ms=60000)` |

With none of them, the first statement waits 5000 milliseconds. The open itself
waits, for the same time, only when it has to read the file again under the lock,
which it does for a file another process is still creating.

`locking_mode` starts at **normal**, which is also SQLite's default. In normal
mode the file lock is released between statements, so a second process can open
the database. A statement that only read keeps its shared lock for up to a
millisecond, so a statement that follows at once does not take it again; a writer
in another process waits that long at most. In `exclusive` mode the connection
keeps the lock until it closes.
`exclusive` is faster for a program that only ever opens one connection. While
it holds the lock, a second process waits for `busy_timeout` and then fails. Any
value other than `normal` or `exclusive` is an error.

## What `automatic_index` switches

`PRAGMA automatic_index = OFF` turns off two things. A join whose inner table has no
index for the join equality stops reading that table once into a hash table, and walks
the table for each outer row. A correlated subquery whose table has no index for its
equality against the enclosing query stops reading that table once into a hash table
as well, and scans it for each outer row. `EXPLAIN QUERY PLAN` follows the setting.
A subquery inside an `UPDATE`, `DELETE` or `INSERT` always scans.

## What `auto_vacuum`, `incremental_vacuum` and `max_page_count` do

`PRAGMA auto_vacuum` reads the mode of the file: 0 for NONE, 1 for FULL and 2 for
INCREMENTAL. NONE is the default. In NONE mode a page that a delete frees stays in the
file and later writes reuse it, so the file does not become shorter.

In FULL mode every commit moves the pages at the end of the file into free pages nearer
the start, lowers the page count, and the file is cut at the next checkpoint. After a
delete in FULL mode `PRAGMA freelist_count` reads 0. In INCREMENTAL mode the free pages
stay in the file until `PRAGMA incremental_vacuum(N)` is run. It gives back up to N free
pages when the statement commits, and every free page when N is left out, is 0 or is
negative. It prints one empty row for each page it was asked to give back, up to
`freelist_count`, as SQLite does. In NONE and FULL mode `PRAGMA incremental_vacuum`
prints nothing and changes nothing. A `ROLLBACK` discards the request.

`PRAGMA auto_vacuum = NONE`, `FULL`, `INCREMENTAL`, `0`, `1` or `2` sets the mode at
once on a file that has no table, and on a file that is already in FULL or INCREMENTAL
mode. On a file in NONE mode that has tables, the connection remembers the value and
`PRAGMA auto_vacuum` keeps reading 0 until `VACUUM` rebuilds the file in the new mode.
Any other value is ignored. Setting a mode other than NONE raises a file at format 2 to
format 3.

In FULL and INCREMENTAL mode the root page of every table and index is kept at the start
of the file, so the step that gives pages back never has to move one. A new table or
index takes the page after the largest root, and `DROP` moves the highest root into the
slot it frees. The meta record holds the largest root as `largest_root`.

The step moves only pages whose owner it can find. It stops at a root, at a page that no
tree owns, and at an extent page in the layout that format 2 files use. The file then
keeps the pages above that page, `PRAGMA freelist_count` can read more than 0, and
`VACUUM` rebuilds the file without that limit.

`PRAGMA max_page_count` reads the limit on the number of pages in the file.
`PRAGMA max_page_count = N` sets it when N is a positive integer. A value below the
current page count is raised to the page count, and a value above 4294967294 is lowered
to it. Any other value leaves the limit as it was. The limit is held on the open file in
memory and is not written to the file, so it applies to one file on one connection and
a reopened file has the default again.

A statement that would make the file longer than the limit fails with the message
`database or disk is full` and the status `full`. The statement is rolled back and the
transaction it was in stays open. SQLite rolled back the whole transaction in the cases
that were measured, so this differs from SQLite. The sort spill and the temporary files
do not count against the limit.

`compat/api/pragmas.toml` holds the same register in a form a program can read.
