# auto_vacuum FULL and INCREMENTAL, and max_page_count

A design for making `PRAGMA auto_vacuum = FULL`, `PRAGMA auto_vacuum = INCREMENTAL`,
`PRAGMA incremental_vacuum(N)` and `PRAGMA max_page_count` do what SQLite's do on inillucent's
storage layout. This document is the deliverable of task-2224. It builds nothing. The Opus session
that filed the ticket builds from it.

Every number in this document was measured on 2026-10-09 with the pinned SQLite 3.53.4 in
`.sqlite-ref/3.53.4/shell/sqlite3.exe` and the installed `inillucent-shell` (release 2.x, the build
in `%LOCALAPPDATA%\Programs\inillucent\bin`). The scripts are the probes described in section 3.
Every file and function named here was read in the task-2224 worktree at commit `e7f7bdc7`.

## Terms used in this document

| Term | Meaning |
|---|---|
| page | one block of the database file. 32768 bytes by default; 8192, 16384 or 65536 are allowed |
| page count | the number of pages the database logically has. `Pool::page_count()` in memory, `Meta.page_count` on disk, `PRAGMA page_count` to SQL |
| the end | the page with the highest number below the page count. Shrinking means lowering the page count and cutting the file there |
| free map | inillucent's record of which pages are in use: one bit per page, in pages of kind `FreeMap` chained from `Meta.free_map` (`crates/inillucent-pool/src/freemap.rs`) |
| free page | a page below the page count whose free map bit is clear. `PRAGMA freelist_count` counts them |
| referrer | the page, meta field or catalog row that holds the number of a page. Moving a page means rewriting its referrer |
| swip | a child pointer in an interior page. On disk it is `(page << 1) \| 1`; in memory it may name a pool frame instead (`crates/inillucent-pool/src/swip.rs`) |
| extent | an out of line value. A value over `page_size / 8` bytes leaves the leaf: a value that fits one page goes to a `BlobShared` page that holds several, a larger one goes to a chain of `BlobExtent` pages (`crates/inillucent-pool/src/extent.rs`) |
| tree id | the number in the `TREE` field of every page header that says which table or index owns the page. The catalog maps it to a root page |
| the log | the write ahead log in `crates/inillucent-wal`. Every change is a record there before it reaches the file |
| checkpoint | the step that writes dirty pages, the free map and the meta record into the file (`crates/inillucent-engine/src/checkpoint.rs`, `Pool::checkpoint`) |
| the step | the loop this design adds that moves pages away from the end and lowers the page count. FULL runs it at every commit. INCREMENTAL runs it when `incremental_vacuum` asks |
| a move | copying one page to a lower free page, rewriting its referrers, and freeing the old one |
| pinned | a page the step cannot move. The step stops at it and the page count stays above it |
| `Full` | `PrimaryCode::Full`, SQLite's `SQLITE_FULL` (13), message `database or disk is full`. The driver reports it as status `full` |

## 1. Why

`tasks/task-2217-bug-hunt-8.md` section 4 lists this as found and not fixed: `auto_vacuum` FULL
and INCREMENTAL change nothing, and `max_page_count` is not enforced. An application that was
written against SQLite relies on both. A mobile or desktop application turns on `auto_vacuum` so
the file gives space back after it deletes; a quota or a cache sets `max_page_count` and expects
the write that would go past it to fail. Today the first does nothing and the second succeeds.

The engine stores pages differently from SQLite. SQLite keeps a freelist of unused pages and, in
the vacuum modes, a pointer map that records the parent of every page, so it can move the last
page of the file into a free slot and find the one pointer to rewrite. inillucent keeps a free map
bitmap and no parent record of any kind. This document says how a page's referrer is found without
a pointer map, how a move is logged so a crash in the middle loses nothing, and how the end of the
file is lowered and the file cut in a way the recovery code already understands.

## 2. What SQLite does, measured

All of these were run through the pinned `sqlite3` on an empty file. The default page is 4096
bytes.

| What was run | What SQLite did |
|---|---|
| `PRAGMA auto_vacuum` on a new file | `0` |
| `PRAGMA auto_vacuum = FULL` before any table, then read | prints nothing on the set, reads `1` |
| 2000 rows of 500 bytes, then `DELETE FROM t WHERE a > 100` under FULL | page count 287 to 16, freelist 0. The file is 65536 bytes |
| `PRAGMA auto_vacuum = NONE` once a table exists | ignored, still reads `1`. The same for `0` and `none` on a file in mode 2 |
| `PRAGMA auto_vacuum = INCREMENTAL` on a file in mode 1 with tables | reads `2`. The switch between 1 and 2 works any time, in WAL mode too |
| `PRAGMA auto_vacuum = FULL` on a file in mode 0 with tables | still reads `0`. After `VACUUM` it reads `1` and the next delete shrinks the file. The set is pending until a `VACUUM` |
| `PRAGMA auto_vacuum = bogus`, `= 7` | ignored |
| `= full`, `= 'FULL'`, `= 1`, `= 'incremental'` | accepted. Words and numbers, any case, quoted or not |
| a `DELETE` under INCREMENTAL | page count unchanged, freelist 271 |
| `PRAGMA incremental_vacuum(10)` with 271 free | prints 10 empty lines. Page count falls by 10, freelist by 10 |
| `PRAGMA incremental_vacuum(0)`, `(-1)`, `('abc')`, no argument | frees every free page, one empty line each |
| `PRAGMA incremental_vacuum` under FULL or NONE | nothing |
| `BEGIN; PRAGMA incremental_vacuum; ROLLBACK` | page count back to what it was |
| `PRAGMA max_page_count` on a new connection | `4294967294` |
| `PRAGMA max_page_count = 10` | prints `10`. Every set prints the value now held |
| `PRAGMA max_page_count = 1` when the page count is 2 | prints `2`. A limit below the page count is raised to it. `0` and `-5` and `'xyz'` leave the limit alone and print it |
| `PRAGMA max_page_count = 999999999999` | `4294967294` |
| an `INSERT` that needs a page past the limit | `Error near line 7: database or disk is full`. The statement's rows are gone |
| `BEGIN; INSERT ...` where the insert needs a page past the limit | the error, and then `COMMIT` says `cannot commit - no transaction is active`. The whole transaction was rolled back |
| `CREATE INDEX` past the limit | the same error. Raising the limit lets it run |
| `max_page_count` after a reopen | back to the default. It is held per connection and per attached file |
| exit code of `sqlite3` after the error | 1 |

Two more facts from SQLite's source, confirmed by the runs above. `PRAGMA incremental_vacuum(N)`
is a loop that frees one page per step and emits one row with zero columns per step, so the shell
prints an empty line for each. Under FULL mode every commit runs that loop until the freelist is
empty. Both come from `incrVacuumStep` in `btree.c`, which also refuses to move a root page: SQLite
keeps every root at the start of the file instead, and records the largest root page number at
header offset 52.

## 3. What inillucent does today, measured and read

The same scripts through `inillucent-shell`:

| What was run | What inillucent did |
|---|---|
| `PRAGMA auto_vacuum = FULL` before any table | reads `1`, and the mode survives a reopen. It is in the meta record at offset 116 |
| the 2000 row insert and `DELETE WHERE a > 100` under FULL | page count 39 before and after. Freelist 0 before and after. The file is 1,277,952 bytes |
| `PRAGMA auto_vacuum = INCREMENTAL` on a file in mode 1 with tables | ignored, still `1` |
| `PRAGMA incremental_vacuum(10)` in mode 2 | nothing when fewer than 10 pages are free. When enough are free it runs the whole `VACUUM` rebuild and frees every free page |
| `PRAGMA max_page_count = 10` when the page count is 5 | prints `10`. Later `= 1` prints `39`. The clamp is right |
| an `INSERT` of 2000 rows past a limit of 10 | succeeds. Page count 39 |
| `DELETE FROM v` on a 343 page table of 5000 byte values | 153 pages free, 190 still allocated by an empty table |
| `DELETE FROM u` on a 41 page table of 500 byte values, in two statements | 6 pages free |
| `DELETE FROM t` on a 14 page table of 20000 one byte values | 7 pages free |

The code behind each row, from the surveys of this worktree:

- **Pragmas.** `pragma_auto_vacuum` (`crates/inillucent-engine/src/pragma/tuning.rs:604`) accepts
  0, 1 or 2 while no user table exists, writes the mode with `Database::set_auto_vacuum`
  (`crates/inillucent-pool/src/file.rs:928`) and checkpoints. Nothing reads the mode afterwards.
  `pragma_incremental_vacuum` (`tuning.rs:688`) calls `reclaim_free_pages`
  (`crates/inillucent-engine/src/lib.rs:1195`), which runs `vacuum_in_place`
  (`crates/inillucent-engine/src/rebuild.rs:793`) when at least N pages are free.
  `pragma_max_page_count` (`tuning.rs:460`) stores `asked.max(page_count).min(4294967294)` in
  `pragmas.max_page_count` (`crates/inillucent-engine/src/engine/state.rs:172`) and echoes it.
- **Allocation.** `Database::allocate(count)` (`file.rs:1001`) is the one place a page is taken.
  It asks `FreeMap::allocate_run`, and otherwise grows the map with `FreeMap::ensure` and raises
  `Pool::page_count`. It reads no limit. Its callers are the leaf and interior writers in
  `crates/inillucent-tree/src/write.rs` (lines 2514, 2522, 2885, 2917), the extent writers in
  `crates/inillucent-tree/src/paged.rs` (206, 247), the bulk builder in
  `crates/inillucent-tree/src/paged/bulk.rs` (198, 306), and a test hook in
  `crates/inillucent-engine/src/engine/integrity.rs:258`. `Database::release` (`file.rs:1058`)
  clears a bit. `Database::claim` (`file.rs:1043`) sets one during recovery. None of the three
  writes a log record. The caller appends `AllocPage` or `FreePage`.
- **The page header** (`crates/inillucent-pool/src/page.rs`) is 32 bytes: LSN, checksum, `KIND`,
  `FLAGS`, `LEVEL`, `TREE` (8 bytes) and `RIGHT` (8 bytes). `RIGHT` is the right sibling on a leaf
  and the next page on a free map page. It is zero on interior and extent pages. There is no parent
  field and no pointer map.
- **Who points at a page.** An interior page holds N keys and N+1 swips: slot `i` is child `i`,
  `rightmost_swip` is child N (`crates/inillucent-pool/src/interior.rs:11-22`). A leaf row holds
  an `ExtentRef { first, length, slot, class }` of 16 bytes for each out of line value. A
  `BlobExtent` page holds at offset 32 the number of the next page of its chain. A `BlobShared`
  page holds a directory of 8 byte slots `(offset, length)` and no record of who owns a slot. A
  free map page's predecessor is the previous page in the chain from `Meta.free_map`. A root is
  named by its catalog row (`SchemaEntry.root`) or by `Meta.catalog_root`.
- **The log** (`crates/inillucent-wal/src/record.rs:100`, enum `Body`) has `WritePage { page,
  image }`, `AllocPage { page }`, `FreePage { page }`, the row records, `Structural`, `SplitLeaf`,
  `CompactLeaf`, `BulkBuilt`, `Commit`, `Abort`, `Checkpoint`, `CatalogChange` and `Pad`. There is
  no record that says where the file ends. Recovery replays only transactions whose `Commit`
  record is present, images first and then logical records, then applies the free map changes in
  log order (`crates/inillucent-engine/src/recovery.rs:310-346`).
- **The file end.** `Pool::page_count` is in memory. `Meta.page_count` is written from it at every
  checkpoint (`Database::checkpoint_after_free_map`, `file.rs:1735`). The file grows when a page
  is written past its end. It is shortened in one place: `Database::give_back_the_unclaimed_tail`
  (`file.rs:728`), which `finish_the_open` (`crates/inillucent-engine/src/engine/open.rs:827`) and
  `attach.rs:328` call after recovery when the file is longer than `page_count * page_size`. A
  checkpoint never shortens the file.
- **Statuses.** `PrimaryCode::Full` is code 13 with message `database or disk is full`
  (`crates/inillucent-base/manifests/errors.toml:134`). The driver maps it to `Status::Full`,
  name `full` (`drivers/inillucent-driver/src/error.rs:204`). Every binding lists it. The CLI exits
  1 for it (`crates/inillucent-cli/src/command/outcome.rs:149`). The shell prints
  `Error near line N: database or disk is full`. The manifest marks `Full` as
  `transaction_rolled_back = true`, and the engine does not act on that column: a failed statement
  unwinds by `DbError::unwind()`, which is `Unwind::Statement` for an untagged error
  (`crates/inillucent-engine/src/engine/compiled.rs:1601`, `abandon`).
- **`crates/inillucent-storage`** is the legacy SQLite file format crate. It has a pointer map and
  a `max_page_count` check, and the engine does not depend on it. Nothing in it applies here.

## 4. Goals

1. Under FULL, a committed `DELETE`, `DROP` or `UPDATE` that frees pages leaves no free page below
   the page count, and the file is `page_count * page_size` bytes long when the statement returns.
2. Under INCREMENTAL, `PRAGMA incremental_vacuum(N)` lowers the page count by exactly `min(N,
   freelist_count)` and answers that many rows with zero columns. Without a count, with 0, with a
   negative number or with a word it frees every free page.
3. Every page kind can be moved: leaf, interior, `BlobExtent`, `BlobShared` and `FreeMap`. A root
   never needs to move because roots are kept at the start of the file (section 8).
4. A kill at any point of a move, of a commit that contains moves, or of the truncation, loses no
   committed row and leaves a file that opens. The proof is the crash campaigns in section 11.
5. `PRAGMA max_page_count` is enforced in `Database::allocate`, so every path that takes a page
   is covered, and the failing statement gets `Full` with SQLite's message. The transaction is
   rolled back, as SQLite's was in every run in section 2.
6. The pragma reads, sets, transitions and outputs match SQLite byte for byte in the shell, except
   for the page numbers themselves, which cannot match because the pages are eight times larger.
7. Mode NONE is untouched. A file in mode 0 never moves a page and never lowers its page count.

Not goals: a pointer map; moving pages in mode 0; changing the `VACUUM` rebuild; shrinking the
log; making `page_count` numbers equal SQLite's.

## 5. The design in one page

Four parts, built in this order.

**A. A move primitive.** `PagedTree::relocate(page, to)` in `crates/inillucent-tree`, on top of
three pool primitives (`Pool::rename`, `FreeMap::allocate_at`, `FreeMap::lowest_free_below`) and
two new log records (`Relink`, `PageCount`). It reads the page header to learn the kind and the
tree, finds the referrer by descending the owning tree or by reading a new back pointer, writes the
image at `to` under a `WritePage` record, rewrites the referrer under a `Relink` record, and frees
`page`. Section 7 gives the referrer rule per kind. Finding a referrer by descent needs no new
data for leaves and interior pages. It needs a small addition to the extent pages, so the file
format goes from 2 to 3 (section 6).

**B. The step.** A loop in `crates/inillucent-engine/src/shrink.rs` (new) that looks at the end
of the file, frees it if it is free, moves it to the lowest free page if it is not, and lowers
the page count. It runs inside `commit_across` for every file in mode 1, and for the number of
steps `incremental_vacuum` queued for a file in mode 2. It ends by appending `PageCount` with the
new end. The file is cut at the next checkpoint, after the meta record that carries the lower
count is durable, by the same code that trims an overlong file at open. Section 9.

**C. Roots stay low.** A new meta field `largest_root`. In mode 1 or 2 a new table or index takes
the root slot `largest_root + 1`, moving whoever is there out of the way, and a `DROP` moves the
largest root into the freed slot and rewrites that table's catalog row. The step never meets a root
at the end of the file. This is SQLite's rule, from header offset 52. Section 8.

**D. `max_page_count`.** A field on `inillucent_pool::Database`, set by the pragma, read by
`Database::allocate`, which refuses growth past it with `Full`. Section 10.

Part E, in section 12, is what a `DELETE` must free for any of this to matter. Today a table
emptied by `DELETE FROM` keeps most of its pages. The step moves pages; it cannot move what the
tree has not freed.

## 6. On disk changes: file format 3

`FORMAT_VERSION` in `crates/inillucent-pool/src/meta.rs:42` goes from 2 to 3. (The constant of the
same name in `crates/inillucent-core/src/persist.rs:55` is another format and does not change.)
`OLDEST_FORMAT_VERSION`
stays 1. The rule the file already states holds: this build reads formats 1, 2 and 3, writes 3,
and refuses a newer file with `unsupported`. The changes:

### 6.1 `BlobExtent` pages get a back pointer and an owner

Today, after the 32 byte common header: offset 32 the next page of the chain (8 bytes), 40 the
payload length (4), 44 reserved (4), 48 the payload. Format 3:

| Offset | Size | Field |
|---|---|---|
| 32 | 8 | `next`: the next page of the chain, 0 at the last page |
| 40 | 8 | `prev`: the previous page of the chain, 0 at the first page |
| 48 | 4 | `payload_len`: payload bytes in this page |
| 52 | 2 | `key_len`: on the first page, the length of the owner key; 0 elsewhere |
| 54 | 2 | `flags`: bit 0 `HEAD` set on the first page |
| 56 | `key_len` | on the first page, the owner key |
| 56 + `key_len` | | the payload |

The owner tree is the `TREE` field of the common header, which `BlobExtent` pages already carry.
The owner key is the row's key as `inillucent_tree::key::Key` encodes it (`crates/inillucent-tree/src/key.rs:74`), the same bytes a descent compares. For a rowid table that is 8 bytes. A key is
never stored out of line (`row_larger_than_a_page`, `write.rs:183`), so it always fits a page, and
a key longer than `page_size / 4` makes the writer put the value in a chain and never in a shared
slot (6.2), so the first page always has room for it.

Why `prev`: moving a chain page means rewriting the pointer that names it, which is the `next`
field of the previous page or the `ExtentRef` in the owning row. Without `prev` the step would
walk the chain from its head to find the previous page, which is quadratic in the chain length
when a whole blob moves. SQLite's pointer map holds the same fact (`PTRMAP_OVERFLOW2`). Why the
key: the head's referrer is a row, and the row is found by descending the owning tree with its
key. The tree id alone would need a scan of every leaf.

`write_extent` (`paged.rs:175`) writes `prev` as it chains the pages and `next` as today. It still
takes contiguous runs from `Database::allocate(count)` when the free map has one, so a chain is
usually consecutive pages and `read_extent` still reads it with few calls; the reader follows
`next` page by page in format 3 and no longer assumes the run is contiguous.

### 6.2 `BlobShared` slots get an owner

Today: offset 32 slot count, 36 live count, 40 payload floor, 44 reserved, 48 a directory of 8
byte entries `(u32 offset, u32 length)`, a free slot being `(0, 0)`. Format 3 makes the directory
entry 16 bytes: `(u32 offset, u32 length, u64 owner_tree)`, and starts each slot's bytes with
`u16 key_len` and the owner key, then the value. `length` counts key and value. `ExtentRef.length`
keeps meaning the value's length. A free slot stays `(0, 0, 0)`. The 32 byte common header's `TREE`
field on a shared page keeps meaning the first writer and is not used by the step.

A value goes to a shared slot only if `2 + key_len + value_len` fits the page's free space, as the
writer checks today for the value alone. Otherwise it goes to a chain (6.1). This is the one place
the owner key costs space, and a key is 8 bytes for every rowid table.

### 6.3 The meta record

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 128 | 8 | `largest_root` | in mode 1 or 2, the highest page number that is a root. 0 in mode 0 and in files written before format 3 |

`META_RECORD_BYTES` (`meta.rs:228`) becomes `at::LARGEST_ROOT + 8 = 136`. The test
`every_field_lives_below_the_record_length` (`meta.rs:585`) checks it. `Database::largest_root()`
and `set_largest_root()` go beside `auto_vacuum()` and `set_auto_vacuum()` in `file.rs:918-935`.
`encode` writes it when the page is long enough, as it does the other fields. `decode` reads 0 from
an older file.

### 6.4 Two log records

In `crates/inillucent-wal/src/record.rs`, two new `Body` variants:

```rust
/// One referrer of a page that moved from `from` to `to` now names `to`.
Relink { from: u64, to: u64, site: RelinkSite },
/// The file logically ends at `pages` pages. Pages at or above it are free.
PageCount { pages: u64 },
```

```rust
pub enum RelinkSite {
    /// The swip in interior page `parent` of tree `tree` that named `from`.
    Child { tree: u64, parent: u64 },
    /// The `RIGHT` field of leaf `left` of tree `tree`.
    Sibling { tree: u64, left: u64 },
    /// Every `ExtentRef` in leaf `leaf` of tree `tree` whose `first` is `from`.
    Extent { tree: u64, leaf: u64 },
    /// The `next` field of chain page `page`.
    ChainNext { page: u64 },
    /// The `prev` field of chain page `page`.
    ChainPrev { page: u64 },
    /// The `RIGHT` field of free map page `previous`.
    MapChain { previous: u64 },
    /// `Meta.free_map` itself.
    MapHead,
}
```

Both are small. A `Relink` is applied under the page LSN rule to the page it names, like a row
record, and it is idempotent: if the field no longer says `from`, nothing is written. `MapHead` and
`PageCount` name no page; the applier records them on the `Database` and they reach the meta
record at the next checkpoint.

A page's image moves under the existing `WritePage { page: to, image }`. The free map changes use
the existing `AllocPage { to }` and `FreePage { from }`. No new record is needed for those, and
recovery already applies them in log order.

### 6.5 What an older file gets

A format 1 or 2 file opens as today. Its extent pages have no owner and no `prev`, so the step
cannot move them. Rule: **the step runs only on a format 3 file.** A format 2 file in mode 1 or 2
reads its mode as stored and behaves as if the mode were pending (section 9.4) until a `VACUUM`,
which rebuilds it as format 3 with the roots low. Nothing is lost, because mode 1 and 2 did
nothing on those files. New files are created as format 3 in every mode, and the extent owner is
always written, so a mode 0 file can switch to 1 or 2 later without a rebuild of its extents. The
switch still needs `VACUUM` when tables exist, for the root rule (section 8).

## 7. Finding the referrer of a page

The step reads the 32 byte header of the page at the end and branches on `KIND`. Every rule below
is a function on `PagedTree` or `Database`, and the step calls `PagedTree::relocate(page, to)`,
which runs the rule, writes the image, appends the records, and returns `Moved` or `Pinned(why)`.

| Kind | How the referrer is found | Records appended |
|---|---|---|
| `Leaf`, level 0, not a root | Read the first key in the page (sorted rows, else the delta area). Descend the owning tree from its root toward that key, keeping the path. The interior page at level 1 on the path is the parent, and the slot the descent chose must hold the swip of `page`; if it does not, the page is pinned with reason `parent does not name the page`. The left neighbour is the leaf whose `RIGHT` is `page`: it is the previous slot of the parent when `page` is not the parent's first child, otherwise the rightmost leaf under the previous child of the nearest ancestor on the path where our branch is not the first child. The first leaf of a tree has no left neighbour | `WritePage { to }`, `Relink Child { parent }`, `Relink Sibling { left }` when there is one, `AllocPage { to }`, `FreePage { page }` |
| `Interior`, level L of 1 or more, not a root | Read the first separator key. Descend from the root toward it to level L + 1. The page there is the parent, and the chosen slot must hold the swip of `page`. An interior page with no separator (count 0) is pinned with reason `no separator`. Interior pages have no sibling link, so no `Sibling` record | `WritePage { to }`, `Relink Child { parent }`, `AllocPage`, `FreePage` |
| `BlobExtent` with `prev != 0` | The referrer is `prev`'s `next` field. The successor's `prev` field must also change | `WritePage { to }`, `Relink ChainNext { page: prev }`, `Relink ChainPrev { page: next }` when `next != 0`, `AllocPage`, `FreePage` |
| `BlobExtent` with `HEAD` | Read `TREE` and the owner key. Descend the owning tree to the leaf. Find the row by key; its `ExtentRef` whose `first` is `page` is rewritten. The successor's `prev` changes too | `WritePage { to }`, `Relink Extent { leaf }`, `Relink ChainPrev { page: next }` when `next != 0`, `AllocPage`, `FreePage` |
| `BlobShared` | For every live slot, read `owner_tree` and the key, descend that tree to the leaf, and rewrite the `ExtentRef` whose `first` is `page` and whose `slot` matches. Several slots often share one leaf; one `Relink Extent` per distinct leaf. If `Database.shared_extent` names `page`, it now names `to` | `WritePage { to }`, one `Relink Extent { leaf }` per leaf, `AllocPage`, `FreePage` |
| `FreeMap` | Walk the chain from `Meta.free_map`. The previous page's `RIGHT`, or the head itself, names `page`. The in memory `FreeMap.pages` entry changes its page number. The bit for `to` is set and the bit for `page` cleared as for any move | `WritePage { to }`, `Relink MapChain { previous }` or `Relink MapHead`, `AllocPage`, `FreePage` |
| a root (the catalog root, or any `SchemaEntry.root`) | Never moved by the step. Section 8 keeps roots below `largest_root`, so the step does not meet one unless the file predates format 3 or the invariant broke. Pinned with reason `root` |
| a page whose `TREE` is not in the catalog of this file, or whose kind is `Unused` or unknown | Pinned with reason `unowned`. `PageKind::Unused` is never written by production code, so an allocated page with that kind or an unknown tree is a leak, which `PRAGMA integrity_check` already reports. The step does not free it, because freeing a page nobody owns is a guess |
| the page under the lock bytes (`FreeMap.reserved`) | Not a page anyone writes. When it is at the end, the page count moves below it without a move. It stays reserved in the map |

Two details every tree rule shares. The descent goes through `PagedTree` for the tree id, which
the engine finds in `schema.trees` by handle, or for the catalog tree by `SCHEMA_TREE_ID` with its
root in `Meta.catalog_root`. A tree id with no `PagedTree` is `unowned`. And a key found in the
page is always inside the page's key range, whether the row is live or a tombstone, so a tombstone
is as good a guide as a live row.

Why the design descends and keeps no pointer map. A pointer map costs a write to a map page every time a
child pointer changes: every split, every merge, every extent allocation. inillucent's interior
pages already hold the one fact a descent needs, which is the separator keys, and a descent reads
`height` pages, which are almost always resident. The extent pages are the one case where nothing
on the page says who owns it, and 6.1 and 6.2 add that fact to the page itself, where it is written
once when the value is written and never maintained again. The owner key does not go stale when the
row moves to another leaf in a split, because the key does not change; only a leaf page number
would.

## 8. Roots stay at the start of the file

A root cannot move cheaply. Its referrer is a catalog row, and rewriting that row is a schema
change every other connection must notice. SQLite never moves one either. It keeps every root at
the start of the file and remembers the largest root page number, so a root is never the page at
the end. This design does the same.

**Invariant, in mode 1 or 2:** every root page number in the file is at most `Meta.largest_root`.

**When a tree is created** (`CREATE TABLE`, `CREATE INDEX`, every path through
`crates/inillucent-engine/src/ddl/catalog.rs` that calls `allocate_root` (`ddl/catalog.rs:39`) or a bulk build
that makes the first tree of a table):

1. `slot = largest_root + 1`. If `slot` is the page under the lock bytes, `slot += 1`.
2. If the free map says `slot` is allocated, `PagedTree::relocate(slot, Database::allocate(1))`
   moves whatever is there. Section 7 finds its referrer. If it is pinned, the create fails with
   `corrupt` naming the page; this cannot happen on a file that satisfies the invariant, because a
   page at `largest_root + 1` is never a root.
3. `FreeMap::allocate_at(slot)`, write the new root there, `largest_root = slot`. The catalog row
   names `slot`.
4. A bulk build whose root came out elsewhere (`build_interior_levels`, `paged/bulk.rs:306`) moves
   its root into `slot` with the same primitive before the catalog row is written, so the row never
   names a page that then moves.

**When a tree is dropped** (`release_tree`, `crates/inillucent-engine/src/ddl/tree.rs:202`, inside
the dropping transaction):

1. Let `r` be the dropped root. If `r == largest_root`, then `largest_root -= 1`, stepping over
   the lock page, and stop.
2. Otherwise find the tree whose root is `largest_root` in this file's catalog. Move its root page
   into `r` with `relocate`, rewrite its `SchemaEntry.root` through the same catalog write the
   `DROP` already makes, update `schema.trees` so the live `PagedTree` knows its new root, and
   `largest_root -= 1`. The schema cookie bump that every `DROP` already does makes other
   connections reload.
3. The dropped tree's pages reach the free map through `pending_frees` at commit as today, and the
   step then lowers the page count past them.

**When the mode is set.** On a file with no user table, `pragma_auto_vacuum` sets
`largest_root` to the highest root in the catalog (the catalog root, and `sqlite_sequence` when
it exists). Those are the only roots, so the invariant holds at once. On a file with tables the
set is pending (section 9.4) and `VACUUM` establishes the invariant: `rebuild_into`
(`rebuild.rs:74`) runs `replay_schema` before `copy_rows`, so every root is created before any row
page, and the roots come out consecutive at the start of the file. `carry_header` (`rebuild.rs:298`)
writes the connection's pending mode into the new file instead of copying the old file's mode, and
`largest_root` is the highest root after `replay_schema`.

Mode 0 keeps `largest_root` at 0 and applies none of this, so a mode 0 file is written exactly as
today.

## 9. The step

### 9.1 The loop

`crates/inillucent-engine/src/shrink.rs`, new:

```text
fn shrink(database, trees, txn, limit) -> Shrunk { freed, pinned_at }
    freed = 0
    while freed < limit:
        end = database.pool().page_count() - 1
        if end < FIRST_DATA_PAGE: break
        if end is the lock page or not allocated:
            database.pool().set_page_count(end); freed += 1; continue
        to = database.lowest_free_below(end)          // FreeMap::lowest_free_below
        if to is None: break                           // the file is dense
        match trees.relocate(end, to, txn):
            Moved  => database.pool().set_page_count(end); freed += 1
            Pinned(why) => pinned_at = Some((end, why)); break
    if the page count changed: txn.append(PageCount { pages })
    return Shrunk
```

Each pass lowers the page count by one and the number of free pages below it by one, which is
what one SQLite `incrVacuumStep` does, so `incremental_vacuum(N)` is `limit = N` and FULL is
`limit = u64::MAX`. The destination is the lowest free page, so the moves fill the file from the
front and the number of moves is the smaller of the live pages above the final end and the free
pages below it.

`relocate` does, in order: `FreeMap::allocate_at(to)` and `AllocPage { to }`; `Pool::rename(end,
to)`, which gives the resident frame the new page number without copying, marks it dirty, and
keeps every swizzled swip that points at the frame valid; `WritePage { to, image }` with the
frame's current bytes; the referrer rewrites and their `Relink` records, each marking its page
dirty; `Database::release(end)` and `FreePage { end }`. Every record carries the transaction's id
and lands before `Commit`.

### 9.2 Where it runs

Inside `ImportedDatabase::commit_across` (`crates/inillucent-engine/src/engine/batch.rs:974`),
after `sync_built_trees` and before `vote`. Every commit goes through `commit_across`: an explicit
`COMMIT` (`commit_batch`), an autocommit statement (`compiled.rs:1398`), a schema change
(`ddl/catalog.rs:873`), the commit of an undo (`commit_the_undo`), and the `OR FAIL` branch of
`abandon` (`compiled.rs:1643`). `flush_pending_frees` moves
to the top of `commit_across` (section 12.2, E3), so dropped trees and deleted values are free
when the step looks, on every path. For each participating file:

- mode 1 (`database.auto_vacuum() == 1`, the file's mode, not the connection's pending one):
  `shrink(limit = u64::MAX)` when the file has a free page below its end. `FreeMap::has_free_below`
  answers that from the allocation hint and a short scan, so a commit that freed nothing costs a
  few hundred nanoseconds here.
- mode 2: `shrink(limit = steps)` where `steps` is what `incremental_vacuum` queued for this file
  since the transaction began (`Writing::incremental_steps: BTreeMap<usize, u64>` keyed by schema
  index, new in `engine/state.rs`). Cleared at commit and at rollback.
- mode 0, or a format 1 or 2 file: nothing.

The step runs with the write lock the commit already holds. It allocates only below the end, so it
never grows the file and never meets `max_page_count`. It writes only to the pool and the log. It
does not truncate.

After `vote` returns, when the step lowered any page count, the commit path asks for a checkpoint
the way `commit_batch` already does for a meta field change (`batch.rs:675-681`). The autocommit
path in `compiled.rs` gets the same call. This checkpoint must not wait: `Pool::hold_for_writing`
waits for `EXCLUSIVE` and returns `busy` when another process keeps `SHARED`, and `commit_batch`
propagates that error, which would report a failure for a commit that is already durable. So the
request goes through a new `ImportedDatabase::checkpoint_if_free`, built on
`Pool::try_hold_for_writing`, which the open path already uses. When `EXCLUSIVE` is not free at
once, nothing happens: the cut waits for the next checkpoint or the next open, and
`PRAGMA page_count` already reports the lower count.

### 9.3 Cutting the file

`Database::give_back_the_unclaimed_tail` (`file.rs:728`) becomes `Database::trim_to_page_count`
with the same body: take `EXCLUSIVE`, `truncate(page_count * page_size)` when the file is longer.
It is called from the two places it is called today, and from one more: the end of
`Pool::checkpoint` (`crates/inillucent-pool/src/pool/fold.rs:178`), after the meta record has been
written to the shadow slot and the primary slot, both syncs have returned, and `finish_journal`
has run. `Pool::checkpoint` returns early before `finish_journal` when an eviction stole an
uncommitted page; the cut goes only on the path where the checkpoint completed.

The order is the whole argument:

1. The `Commit` record is durable. The log says the file ends at `pages`. Nothing in the file has
   changed.
2. The checkpoint writes every dirty page, including the `to` pages and the relinked parents, the
   free map images, and then the meta record with `page_count = pages`. The pages between `pages`
   and the old end are still in the file, free and unread.
3. The file is cut at `pages * page_size`.

A crash before 1 loses the transaction. Recovery replays nothing of it. The page at the old end is
untouched, because a move never writes to the page it moves; it writes to `to`, which was free.
A crash between 1 and 2 recovers by replay: `WritePage` installs the images, `Relink` rewrites the
referrers, `AllocPage` and `FreePage` fix the map, `PageCount` lowers the count, and then
`trim_to_page_count` at open cuts the file, as it cuts any file longer than its header today
(`a_recovered_database_is_no_longer_than_its_header_says`, `tests/durability/durability.rs:204`).
A crash between 2 and 3 leaves a file longer than its header, which is the case `trim_to_page_count`
already exists for. At no point is the file shorter than the header says, so
`a_truncated_database_is_refused` (`tests/durability/fault_shapes.rs:330`) keeps meaning what it
means: a file shorter than its header is damage.

The replay of `PageCount` also discards every pool frame at or above `pages`, including a frame
for a moved page that a reader process still held, and the replay of `Relink Child` on a resident
parent whose swip is swizzled points the swip at the frame of `to` or unswizzles it. The
`Applier` in `crates/inillucent-txn/src/redo.rs` gains both, and `begin_read` (`file.rs:1075`), which
reloads and replays when another process moved the generation, uses the same applier, so a reader
in another process follows a move at its next statement.

The encrypting VFS answers `truncate` for its own layout today at open; the checkpoint call is the
same call, and `tests/engine/encryption_at_rest.rs` gets one test that a FULL delete on an encrypted
file ends with `freelist_count` 0 and a file that reopens.

### 9.4 The pragma semantics, in full

`pragmas.auto_vacuum` (`engine/state.rs:108`) becomes the connection's **pending** mode. The file's
mode is `Database::auto_vacuum()`.

| Statement | Behaviour |
|---|---|
| `PRAGMA [schema.]auto_vacuum` | the file's mode: `0`, `1` or `2`. One row, one unnamed column, as today |
| `PRAGMA [schema.]auto_vacuum = v` where `v` folds to `0`, `none`, `1`, `full`, `2`, `incremental`, quoted or not, any case | no rows. If the file has no user table: set the file's mode, set `largest_root` (section 8), checkpoint. Else if the file is in mode 1 or 2 and `v` is 1 or 2: set the file's mode, checkpoint. Else record `v` as pending on the connection and change nothing |
| `PRAGMA auto_vacuum = anything else` | no rows, nothing changes |
| `VACUUM` | the rebuilt file gets the pending mode when one is set, else the old file's mode; `largest_root` as in section 8. The pending mode is cleared. `VACUUM INTO` writes the same mode the source has |
| `PRAGMA [schema.]incremental_vacuum[(N)]` | a write statement, refused in a read only connection as today. If the file's mode is not 2: no rows. Else `steps = N` when `N` parses as an integer above 0, otherwise unbounded; queue `steps` for the file; answer `min(steps, freelist_count)` rows of zero columns. The moves happen at commit (9.2) |
| `BEGIN; PRAGMA incremental_vacuum; ROLLBACK` | the queue is dropped with the transaction. Page count unchanged, as in SQLite |
| `PRAGMA [schema.]max_page_count` | the file's limit, one row named `max_page_count` |
| `PRAGMA [schema.]max_page_count = N` | if `N` parses as an integer above 0: `limit = N.max(page_count).min(4294967294)`. Answer the limit now held. Held on `inillucent_pool::Database`, so per file and per connection, not in the meta record, and lost at reopen, like SQLite |

The zero column row: `Outcome.rows` is `Vec<Vec<OwnedDatum>>`, so a row with no columns is
representable, and the shell's list mode prints it as an empty line (`shell.rs:910` skips drawing
only when both columns and rows are empty; `separated` in `render.rs:316` joins no cells into an
empty string), which is what `sqlite3` prints. The differential harness compares column count and
rows, so a one column answer would fail parity. The csv, column, box and json modes, the CLI
verbs, MCP and the C API were not checked for a zero column row; the implementer runs
`PRAGMA incremental_vacuum(2)` through each once. Today `pragma_incremental_vacuum` reads the
connection's `pragmas.auto_vacuum` cell; it must read the file's mode, `Database::auto_vacuum()`.

The answer is `min(steps, freelist_count)` and not the count the step achieves, because the step
runs at commit. The two differ only when a page is pinned, which section 7 says cannot happen on a
file that satisfies the invariants. `PRAGMA page_count` after the commit is the truth.

### 9.5 What a move must not break

- **Undo.** The engine rolls a statement back logically, by replaying before images of rows
  through `tree.put` and `tree.delete` (`undo_to_floor`, `batch.rs:126`). Those images hold the
  row's values as bytes (`Before.row` is `Vec<OwnedDatum>`, `engine/write.rs:504-527`;
  `row_in_leaf`, `write/sorted.rs:423`, reads the extent before the image is stored), so a
  restored row writes a fresh extent and never names an old page. A move is therefore safe
  before an undo. The step still runs only inside `commit_across`, and `incremental_vacuum`
  queues its steps for that point, so that a `ROLLBACK` discards the request as in SQLite and
  every move happens in one place. That is why 9.2 defers it.
- **Readers in the same process.** Nothing at the page level protects a reader with an older
  view, and nothing needs to. A reader holds `SHARED` on the file, data pages reach the file only
  at a checkpoint under `EXCLUSIVE`, and `Statement::step` reads every row on its first call. A
  deleted blob's pages are released at the delete today (`free_extent`, `paged.rs:293`) under the
  same rule, and a moved page's old slot is released the same way. The comment in `shrink.rs`
  says so.
- **Pages freed and reused in one transaction.** The step never allocates at or above the page it
  is moving, and it runs last, so a freed `from` is never handed out again before `Commit`.
- **The free map's own pages.** `FreeMap::ensure` takes new map pages from the end of the file, so
  in a file that has grown past one map page the end is often a map page. The `FreeMap` rule in
  section 7 moves it. The step never frees a map page.
- **Bulk built pages.** `sync_built_trees` has run, so their bytes are in the file and in the pool
  before the step reads them.
- **Attached files.** Each has its own `Database`, mode, `largest_root`, queue and step.

## 10. `max_page_count`

### 10.1 Where the limit lives

`inillucent_pool::Database` gets `max_page_count: Cell<u64>`, default `4_294_967_294`, with
`max_page_count()` and `set_max_page_count(limit)`. `pragma_max_page_count` sets it on
`file_of_mut(at)` and reads it from `file_of(at)`. `pragmas.max_page_count` in `engine/state.rs`
goes away, and `ConnectionSettings` in `rebuild.rs:512-616` restores the limit onto the reopened
`Database` after a `VACUUM` instead of onto the pragma group. The constant
`DEFAULT_MAX_PAGE_COUNT` moves from `pragma.rs:510` to the pool crate.

### 10.2 Where it is enforced

`Database::allocate(count)` (`file.rs:1001`), and only there, because every page any component
takes comes through it (section 3). The rule, before anything changes:

1. Ask `FreeMap::allocate_run(count)`. A run whose last page is below `page_count` is inside the
   file and always allowed, because the limit is never below the page count.
2. Otherwise compute `next`, the page count after the growth, including any map page
   `FreeMap::ensure` would add. If `next > max_page_count`, return
   `DbError::primary(PrimaryCode::Full).with_message("database or disk is full")`, tagged
   `Unwind::Transaction`, with nothing taken, no bit set and the page count unchanged.
3. Otherwise grow as today.

The paths this covers, by the callers in section 3: leaf and interior splits, root splits, the
extent writers, the bulk builder (so `CREATE INDEX` past the limit fails as in SQLite), and the
free map's own growth. The root slot relocation of section 8 allocates through the same function.
`Database::claim`, which recovery uses, does not check the limit: a committed transaction must
replay. `Pool::install` raising `page_count` for a page it is handed is recovery's path too.

Spills do not count, by design and in fact. The sort spill (`crates/inillucent-engine/src/spillfile.rs`) is a transient file on the VFS. The pool's eviction spill (`crates/inillucent-pool/src/pool/spill.rs`)
holds copies of pages that are already allocated, in a per connection transient file, and a fold
writes them back to pages the file already has. Neither takes a database page. SQLite's limit does
not count its temporary files either. A test proves a sort that spills under a tight limit still
succeeds (section 11).

### 10.3 What the failure looks like

The error is `Full`, code 13, message `database or disk is full`, no detail, so the shell prints
`Error near line N: database or disk is full`. `--output json` carries status `full`. The CLI exits
1. The driver, the C API and the bindings already have the status. `Unwind::Transaction` rolls the
open transaction back, which is what `sqlite3` did in every run in section 2 and what
`errors.toml` already declares for `Full`. SQLite keeps the transaction open when the failing
statement used a statement journal. A test records that difference (section 11.3), and the
engine does not copy it.

Measured by the differential cases `prag.max.page.count.*` during the build: SQLite rolls back the
whole transaction when a statement of one row needs a page past the limit, and keeps the
transaction open when a statement of many rows fails this way, such as an `INSERT ... SELECT`.
This engine keeps the transaction open in both cases and undoes only the failing statement, which
is part D as built. The case where SQLite rolls the transaction back is graded `Differs` in
`prag.max.page.count.transaction`.
The statement that failed took no page, because the refusal happens before anything is set.
Pages the same statement took earlier are released by the rollback as today.

## 11. Tests

### 11.1 Existing tests that must keep passing, and the two that change

| Test | Why it matters here |
|---|---|
| `tests/engine/vacuum_on_vfs.rs`, `incremental_vacuum_stays_on_the_file_system_too` | **changes.** It asserts `page_count` fell after `incremental_vacuum(1)`. Under this design it falls by exactly 1, and the file is never renamed, so the stray file check stays true for a new reason. Add: `freelist_count` fell by 1 and the root pages of `t` are unchanged |
| `tests/differential/new_engine_pragma.rs`, `an_unknown_pragma_is_silent_on_the_new_engine` | `PRAGMA auto_vacuum = FULL` on a file with tables still reads `0` (pending), and `PRAGMA incremental_vacuum` still answers no rows in mode 0. Its comment at line 376 is out of date and says the pragma does nothing; rewrite it |
| `tests/differential/pragma.rs`, `the_pager_pragmas_describe_the_file` | reads and sets compared with the oracle; `page_count` deliberately excluded |
| `tests/differential/semantics.rs`, `prag.max.page.count`, `prag.freelist.count` | text parity |
| `tests/corpora/matrix/pragma/feature-probe.slt`, `probe-prag.autovacuum`, `probe-prag.max.page`; `retained/bugs.slt`, `retained-bug-17-auto-vacuum` | mode reads survive a reopen |
| `compat/corpus/usage/catalog-pragma.sql` lines 396 to 408 | attached `auto_vacuum` sets stay per file |
| `compat/corpus/usage/holdout-indexes.sql`, case `724-pragma-max-page-count-and-index-balancing`, deviation row in `known.toml:30` | **changes.** Its reason says the page size is ignored. With the limit enforced the case may now agree or differ for a new reason; rerun and rewrite the row |
| `tests/durability/durability.rs`, `a_recovered_database_is_no_longer_than_its_header_says` | the length invariant 9.3 relies on |
| `tests/durability/fault_shapes.rs`, `a_truncated_database_is_refused` | a file shorter than its header is still damage |
| `tests/durability/free_map_checkpoint_crash.rs`, `new_engine_free_map_recovery.rs` | the map images and `AllocPage`/`FreePage` replay the step depends on |
| `tests/durability/vacuum_crash.rs`, `process_maintenance.rs` | `VACUUM` is unchanged except `carry_header` |
| `tests/durability/encrypted_crash.rs`, `tests/engine/encryption_at_rest.rs:297` | the cut goes through the encrypting VFS |
| `tests/tooling/crash_reports.rs` | counts truncate and sync calls per open and close. The checkpoint cut truncates only when the file is longer than the header, which in mode 0 happens only after a crash, so the counts in those scenarios should not move. If they do, the design is wrong somewhere |
| `tests/engine/space_reuse.rs` lines 79 to 97 | mode 0 keeps its free pages and a bounded page count |
| `crates/inillucent-pool/src/meta.rs`, `every_field_lives_below_the_record_length` | `META_RECORD_BYTES` moved |
| `drivers/inillucent-driver/tests/capability.rs`, `the_capability_table_matches_the_engine` | the two new rows (11.4) |
| `tests/tooling/selection.rs`, `tests/tooling/documentation.rs` | every new file has a row; the per tier counts in `tests/inillucent-testing-tdd.md` section 2 move |

### 11.2 Engine tier: `crates/inillucent-compat/tests/engine/auto_vacuum.rs`, new

A `mod auto_vacuum;` line in `tests/engine/main.rs` and a row in `tests/selection.toml` with
`name = "engine"`, `module = "auto_vacuum"`, `tier = "engine"`, covering `inillucent-engine`,
`inillucent-pool`, `inillucent-tree`, `inillucent-wal`, `inillucent-txn`. Each test opens on
`MemoryVfs` at 8192 byte pages, as `vacuum_on_vfs.rs` does, so the numbers are small and the
tests run in seconds. Every one asserts a count, reopens before it reads, and asks the row count
once through the table and once through an index (rules 4 and 6 of the testing standard). Red means
the test fails on the code before the change.

| Test | Red | Green |
|---|---|---|
| `a_delete_under_full_leaves_no_free_page_below_the_end` | `freelist_count` is 30 after the delete | 0, and `page_count` equals the count of allocated pages |
| `a_delete_under_full_shortens_the_file_when_the_statement_returns` | the file is as long as before | `file.len() == page_count * page_size` right after the `DELETE`, with no explicit checkpoint |
| `incremental_vacuum_n_frees_exactly_n_pages_and_answers_n_empty_rows` | the pragma either rebuilds or does nothing | `page_count` and `freelist_count` both fall by N; N rows, each with zero columns; the root pages of two untouched tables are the same page numbers as before, which a rebuild cannot promise |
| `incremental_vacuum_without_a_count_frees_every_free_page` | | no argument, `0`, `-1`, `'abc'` all end at `freelist_count` 0 |
| `incremental_vacuum_is_silent_in_modes_none_and_full` | | no rows, nothing moves |
| `a_rolled_back_incremental_vacuum_moves_nothing` | | `BEGIN; PRAGMA incremental_vacuum; ROLLBACK;` leaves `page_count` as it was |
| `a_mode_change_on_a_file_with_tables_waits_for_vacuum` | the set is ignored forever | reads `0` after `= FULL`, reads `1` after `VACUUM`, and the next delete shrinks |
| `full_and_incremental_switch_without_a_vacuum` | `= INCREMENTAL` on a mode 1 file is ignored | reads `2`, and a later `= FULL` reads `1` |
| `a_moved_leaf_keeps_its_place_in_the_sibling_chain` | | after a FULL delete that moved leaves, a full scan in key order returns the same rows as an index scan, and the leaf chain from the first leaf visits every leaf once |
| `a_moved_interior_page_still_routes_every_key` | | a tree three levels deep, delete from the front so an interior page sits at the end, FULL commit, then every remaining key is found by point lookup |
| `a_blob_chain_page_moves_and_the_value_reads_back_byte_for_byte` | | 200 KB values, delete the rows written before them, FULL commit, read each and compare to the bytes written; reopen and compare again |
| `a_shared_extent_page_moves_and_every_owner_still_reads` | | two tables fill one shared page with 5 KB values; delete the pages before it; FULL commit; both tables read their values back |
| `an_unowned_page_pins_the_end_and_the_commit_still_succeeds` | | leak a page at the end with `strand_a_page_unchecked` (`engine/integrity.rs:258`), free pages below it, FULL commit: the commit succeeds, `page_count` is unchanged, `integrity_check` names the leak |
| `an_attached_file_shrinks_on_its_own_mode` | | `ATTACH` a mode 1 file to a mode 0 main; a delete in each; only the attached file shrinks |
| `a_table_created_late_takes_a_low_root_and_does_not_pin_the_end` | the late root is at the end, and the earlier table's delete leaves `page_count` above it | the late table's root is `largest_root` from before plus 1, and the delete shrinks the file past where that root would have been |
| `dropping_a_table_moves_the_largest_root_into_its_slot` | | three tables; drop the middle one; the third table's catalog row now names the dropped root's page; reopen; the third table reads; `largest_root` fell by 1 |
| `vacuum_applies_a_pending_mode_and_packs_the_roots_low` | | mode 0 file with five tables and data; `= FULL`; `VACUUM`; every root is below `largest_root`, and `largest_root` is the number of roots plus the catalog root |
| `max_page_count_refuses_the_write_that_would_grow_past_it` | the insert succeeds and `page_count` passes the limit | status `Full`, message `database or disk is full`, `page_count` unchanged, the open transaction is gone (`COMMIT` fails as in SQLite), the connection runs the next statement |
| `max_page_count_counts_an_index_build_and_a_blob_chain` | | `CREATE INDEX` at the limit fails with `Full`; a 200 KB insert at the limit fails and `freelist_count` is unchanged, so no chain page leaked |
| `max_page_count_is_held_per_file_and_per_connection` | | a limit on an attached file does not bind main; a second connection reads the default; a reopen reads the default |
| `max_page_count_is_raised_to_the_page_count_and_capped` | | already true today; keep it as the parity anchor |
| `a_sort_that_spills_under_a_tight_limit_still_succeeds` | | `max_page_count = page_count`, then an `ORDER BY` over a table larger than the pool: the query returns every row |
| `a_free_map_page_at_the_end_moves_too` | | **nightly**, in `tests/nightly/`: a file past one map page at 8192 byte pages needs 65,280 pages, about 510 MB; fill it, delete the front, FULL commit, the map page that was at the end is now low, `integrity_check` is `ok`, every row reads. A unit test `a_relinked_map_page_keeps_every_bit` in `freemap.rs` covers the chain rewrite cheaply |

### 11.3 Differential tier

Cases in `crates/inillucent-compat/tests/differential/semantics.rs`, graded `Agrees` against the
oracle. None prints a page number.

| Case | Script |
|---|---|
| `prag.autovacuum.transitions` | `= FULL` on empty, read; create a table; `= NONE`, read; `= INCREMENTAL`, read; `= 2`, read; `= bogus`, read; `= 7`, read |
| `prag.autovacuum.pending` | create and fill a table in mode 0; `= FULL`; read; `VACUUM`; read |
| `prag.incremental.vacuum.rows` | `= INCREMENTAL`; 2000 rows of 5000 bytes; `DELETE WHERE a > 100`; `PRAGMA incremental_vacuum(3)`; then `(0)`; then `PRAGMA freelist_count`. Both engines print three empty lines, then as many as were left, then `0` |
| `prag.incremental.vacuum.rollback` | the same setup; `BEGIN; PRAGMA incremental_vacuum(2); ROLLBACK; PRAGMA freelist_count` |
| `prag.incremental.vacuum.silent` | mode 0 and mode 1: `PRAGMA incremental_vacuum(1)` prints nothing |
| `prag.max.page.count.full` | create and fill; `PRAGMA max_page_count = 1` (clamps to the page count); `INSERT` 2000 rows; `SELECT count(*)`. Both shells print `Error near line N: database or disk is full` with the same `N`, because the scripts are the same; `without_carets` in `tests/differential/usage_corpus.rs:90-93` removes only the caret lines under it |
| `prag.max.page.count.zero` | `PRAGMA max_page_count = 1000; PRAGMA max_page_count = 0; PRAGMA max_page_count = -5; PRAGMA max_page_count = 'xyz'; PRAGMA max_page_count`. SQLite leaves the limit at 1000 for a value that is not a positive integer. Today's `pragma_max_page_count` computes `asked.max(page_count)`, which lowers 1000 to the page count, so this case is red today |
| `prag.max.page.count.transaction` | `BEGIN; INSERT ...; COMMIT;` past the limit, then `SELECT count(*)`: the `COMMIT` error text `cannot commit - no transaction is active` and the count agree |
| `prag.max.page.count.statement.journal` | known difference, asserted (rule 3): `BEGIN; INSERT ... SELECT` past the limit; `SELECT count(*); COMMIT`. SQLite keeps the transaction; inillucent rolled it back. Graded `Differs` with the reason in the case |

A usage corpus file `compat/corpus/usage/auto-vacuum.sql` with `-- reopen` lines: the mode survives
a reopen in both engines, `freelist_count` is `0` after a FULL delete and a reopen in both, and
`PRAGMA integrity_check` is `ok` in both.

### 11.4 Capability rows

Two rows in `drivers/inillucent-driver/src/capability.rs`, both checked by
`the_capability_table_matches_the_engine`:

- `auto_vacuum`, `Yes`, `Probe::Answers`: setup `PRAGMA auto_vacuum = FULL; CREATE TABLE t(a INTEGER PRIMARY KEY, b); INSERT 400 rows of 5000 bytes; DELETE FROM t WHERE a > 10;` and sql `PRAGMA freelist_count`, expecting `0`. On the code before the change it answers a number above 0, so the row is checked in both directions.
- `max_page_count`, `Yes`, `Probe::Refuses`: setup `PRAGMA max_page_count = 1` (clamps to the page count) and the refused statement an `INSERT` of 400 rows of 5000 bytes.

### 11.5 Durability tier: `crates/inillucent-compat/tests/durability/auto_vacuum_crash.rs`, new

A `mod` line in `tests/durability/main.rs` and a `selection.toml` row with `tier = "durability"`,
`cadence` inherited (merge), `covers = ["inillucent-engine", "inillucent-txn", "inillucent-wal",
"inillucent-pool", "inillucent-tree", "inillucent-sim", "inillucent-vfs"]`. It uses
`crates/inillucent-compat/src/crashcampaign.rs` (`Campaign`, `Campaign::run`) and the simulated VFS
in `crates/inillucent-sim` (`SimVfs`, `Site`, `Policy`, `Failure`), exactly as `vacuum_crash.rs`
does. Each campaign's schema makes a tree three levels deep with 200 KB chain values and 5 KB
shared values, so every page kind of section 7 is in the file; its probes are the row count, an
index lookup, `PRAGMA integrity_check`, `PRAGMA freelist_count` and `PRAGMA page_count`. The
harness already asserts that no cut produced a mixture of old and new state and that a statement
which reported success is the state after recovery. `CUTS = 200` per campaign; one cut is one
commit, so a campaign stays under two minutes.

| Test | Mechanism | What it proves |
|---|---|---|
| `a_crash_at_any_call_during_a_full_commit_loses_nothing` | `Campaign`, mode `wal` and `delete`, workload `DELETE FROM t WHERE a > 50` under FULL, `Failure::Crash` at the nth VFS call | every cut recovers to either the pre delete state with the old page count or the post delete state with `freelist_count` 0 |
| `a_crash_at_any_call_during_incremental_vacuum_loses_nothing` | workload `DELETE` then `PRAGMA incremental_vacuum(20)` | the same, with `page_count` either unchanged or lower by exactly 20 |
| `a_crash_inside_the_cut_leaves_a_file_no_longer_than_its_header` | `Site::Truncate` with `Policy::Always(Failure::Crash)` on a FULL delete; assert the site was reached | the recovered file opens, `file.len() == page_count * page_size`, every row reads |
| `a_crash_between_the_commit_and_the_checkpoint_replays_every_move` | `Site::Sync` with `Policy::Nth` chosen so the crash lands after `Commit` and before the meta write; assert from `sites_reached()` | recovery replays `WritePage`, `Relink`, `PageCount`; the pages read through their new parents; the file is cut at open |
| `a_disk_full_during_a_move_fails_the_commit_and_keeps_the_old_layout` | `Failure::DiskFull` on `Site::Write` during the step | the commit returns an error, the file reopens with the pre delete rows, and `integrity_check` is `ok` |
| `a_crash_during_a_root_move_at_drop_table_keeps_the_catalog_and_the_tree_in_step` | `Campaign` whose workload is `DROP TABLE` of a middle table in mode 1 | after every cut the third table reads through whichever root the catalog names |
| `a_crash_while_a_root_slot_is_cleared_for_a_new_table` | `Campaign` whose workload is `CREATE TABLE` in mode 1 on a file whose slot is occupied by a leaf | the occupant's rows read after every cut |
| `an_io_error_inside_a_relink_pins_nothing_permanently` | `Failure::IoError` once on the parent's write during a move, then a clean reopen | the next FULL commit shrinks the file to `freelist_count` 0 |

Two process tests, beside their neighbours:

- `tests/durability/process_maintenance.rs`:
  `incremental_vacuum_in_another_process_loses_no_acknowledged_row`, written like
  `vacuum_in_another_process_loses_no_acknowledged_insert` (line 245), with a `Child::kill()` while
  the second shell runs the pragma.
- `tests/durability/process_readers.rs`: `a_reader_follows_a_page_moved_by_another_process`: a
  reader holds a connection open across a FULL delete in a second process and reads every row
  afterwards.

Each new target gets a timing row from `inillucent-testrun --record`. Until it has one, the runner
gives it the two hour floor.

### 11.6 How to prove the red step

For the engine tests, run the new file against the code before the change: each row in 11.2
names what fails. For the crash campaigns, revert `PageCount` replay only: the campaign for the
commit to checkpoint window then recovers a file whose `page_count` is the old one, and the
`page_count` probe differs from the state the statement reported. Write the result of that run into
the test's comment, as `vacuum_on_vfs.rs` does for its `(1)`.

## 12. Part E: what a `DELETE` must free

The step moves pages. It cannot move a page the tree has not freed, and section 3 measured that
`DELETE FROM v` on a 343 page table left 190 pages allocated. Under FULL that table would end at
190 pages, which no application expects. This part is therefore a dependency of parts A to D, and
the FULL tests in 11.2 assume it.

### 12.1 Why the pages stay

From `crates/inillucent-tree/src/write.rs`:

- A delete of a row in the sorted area sets a tombstone bit (`delete_near`, `write.rs:1561`,
  `set_tombstone`) and frees nothing. The comment at `write.rs:1634` says the tombstoned row
  "still names its extent for the next repack to free". A delete of a row in the delta area frees
  its extent at once (`write.rs:1667`). A pure delete never repacks: `CompactLeaf` is logged only
  in `compact_into` (`write.rs:2381`) and `repack_run` (`write/sorted.rs:825`), and `make_room` runs
  from a delete only when the tombstone bitmap itself needs room (`write.rs:1604`). So the 5000
  byte values of table `v` sat in their shared pages after every row was deleted.
- A leaf whose live rows fall below half (`underflows`, `write.rs:227`) is merged by absorbing
  its **right** sibling (`merge_if_small`, `write.rs:3103`); the right page is freed. The merge is
  refused when there is no right sibling, when the right sibling is under another parent
  (`adjacent_under_one_parent`, `write.rs:2959`), when the parent would keep fewer than two
  children (`write.rs:3124-3135`), or when the absorbed rows do not fit at `TIGHT_FILL` 0.95
  (`write.rs:292`). The first insert into a table is a bulk build, which packs its leaves full, so
  an emptied leaf next to a bulk built sibling almost never absorbs it. That is tables `u` and `t`:
  every leaf became a page of tombstones that could not merge rightward.
- Nothing ever frees an interior page or lowers the height (`height`, `paged.rs:833`).

### 12.2 Three rules

**E1. An empty leaf leaves the tree instead of absorbing its neighbour.** In `merge_candidate`
(`write.rs:3045`), when `live == 0` and the leaf is not the root, the merge goes **left**: the
leaf's slot and separator are removed from its parent, the left neighbour's `RIGHT` is pointed at
the leaf's right sibling, the leaf's extents are queued for freeing (E3), and the page is released
with `FreePage`, exactly as today's rightward merge releases the absorbed page. Nothing has to fit,
because no row moves. The left neighbour is found by the same `find_left_neighbour` the move
primitive uses (section 7); it may be under another parent, which is fine, because only the
emptied leaf's own parent loses a slot. The first leaf of the tree has no left neighbour and only
loses its parent slot. The record is the existing `Structural { kind: Merge, left, right, parent,
left_image, right_image, parent_image }` with `left` the neighbour, `right` the emptied leaf and
`parent` the emptied leaf's parent; recovery already applies the three images under the page LSN
rule. The rightward merge stays for leaves that are small but not empty.

The one guard E1 keeps is the parent's: a non root interior page keeps at least two children
(`write.rs:3124-3135`). So after `DELETE FROM t` a table holds its root, its interior pages, and
one empty leaf under each interior page whose other children all left. For a table of 2000 rows
at 32768 byte pages that is two or three pages. The second rule removes those too.

**E2. A one child interior page collapses.** Recommended, not required for the tests in 11.2. When
a merge leaves a non root interior page with one child, the grandparent's swip that named the
interior page is rewritten to name the child, and the interior page is freed. When the root is
left with one child, the child's bytes are copied into the root, the root's `LEVEL` falls by one,
and the child is freed, so the height drops and the root never moves. Both are one new
`Structural` kind, `Collapse`, with the three images `(grandparent or root, removed page, child)`.
This is the item `tasks/task-2217-bug-hunt-8.md` section 4 lists first, "taking an emptied parent
out of its own parent is a structural change to the tree and its log records". It is a day of
work on top of E1 and makes an emptied table two pages: its root and nothing else.

**E3. A deleted row's out of line value is freed by the statement that deletes it.** `delete_near`
and `delete_run` (`write/sorted.rs:178`) collect the `ExtentRef`s of the sorted rows they
tombstone, as they already collect them for delta rows (`write.rs:1667`, `sorted.rs:213`), and free
them through `free_extent` (`paged.rs:293`) at once. This is safe for a rollback because an undo
image holds the row's values as bytes, so `undo_to_floor` writes a fresh extent (9.5). Two things
must change with it. The repack (`repack`, `write.rs:2324`) frees `extents_of(page)` minus what
the new image keeps, and `extent_refs` (`paged.rs:1226`) ignores the tombstone bitmap, so after E3
the repack would free a tombstoned row's extent a second time: `extent_refs` gains a flag to skip
tombstoned rows, and `free_extent` on a slot that is already clear becomes a no op that
`integrity_check` can count. And `flush_pending_frees` moves to the top of `commit_across` so the
autocommit path drains dropped trees too; `Freed::Value(ExtentRef)` (`engine/state.rs:908`)
already exists there for a dropped tree's values and needs no new variant.

The comment at `write.rs:1634`, "a tombstoned sorted row still names its extent for the next
repack to free", is replaced by one that says a tombstoned row names nothing and why the repack
must skip it.

### 12.3 What the measurements become

With E1 and E3, after `DELETE FROM v` under FULL the table holds its root and at most one empty
leaf per interior page, and every shared page is free and moved away or cut. `page_count` is the
meta pages, the free map page, the catalog root, the table root and those few leaves: under ten
pages for the 343 page file. The engine test `a_delete_under_full_leaves_no_free_page_below_the_end`
asserts `freelist_count` 0 and `page_count` at most ten, and names E1 and E3 in its comment. With
E2 as well it asserts at most six.

Tests for E alone, in `tests/engine/auto_vacuum.rs` beside the others, each red today:

| Test | Green |
|---|---|
| `deleting_every_row_frees_every_shared_page` | mode 0, 2000 rows of 5000 bytes, `DELETE FROM v`, `freelist_count` is at least the number of shared pages the insert allocated |
| `an_emptied_leaf_beside_a_full_sibling_is_freed` | mode 0, a bulk built table, delete the rows of the second leaf only; `freelist_count` rises by one and a scan still returns every other row in order |
| `a_rolled_back_delete_keeps_its_values_readable` | `BEGIN; DELETE FROM v; ROLLBACK;` then every value reads back byte for byte, and `freelist_count` is what it was before the `BEGIN`. This is the test that proves E3 may free at the delete |
| `a_repack_after_a_delete_frees_nothing_twice` | delete half the rows of a leaf, then an `UPDATE` that forces a repack of that leaf; `integrity_check` is `ok` and `freelist_count` moved only by the pages the delete freed |
| `deleting_every_row_leaves_the_root_and_one_leaf_per_parent` | the count E1 promises, and with E2 the root alone |

The crash campaign `a_crash_at_any_call_during_a_full_commit_loses_nothing` covers E1 and E3 as
well, because its workload is the delete that triggers them.

## 13. Every file the change touches

| Crate or area | File | Change |
|---|---|---|
| `inillucent-pool` | `src/meta.rs` | `FORMAT_VERSION = 3`, `at::LARGEST_ROOT = 128`, `largest_root` field, `META_RECORD_BYTES = 136`, encode and decode, the field test |
| | `src/page.rs` | extent header offsets of 6.1; a `HEAD` flag |
| | `src/extent.rs` | chain `prev`, owner key on the head, 16 byte shared slot entries with the owner, slot bytes prefixed by the key; readers skip the prefix |
| | `src/freemap.rs` | `lowest_free_below(limit)`, `has_free_below(limit)`, `allocate_at` reused, `relink_map_page(from, to)`, the unit test |
| | `src/file.rs` | `max_page_count` field and accessors; the limit check in `allocate`; `largest_root` accessors; `give_back_the_unclaimed_tail` renamed `trim_to_page_count`; `lowest_free_below`; `shared_extent` follow on a move |
| | `src/pool.rs`, `src/pool/fold.rs`, `src/pool/swizzle.rs` | `Pool::rename(from, to)`; `Pool::discard_at_or_above(pages)`; the cut at the end of `Pool::checkpoint` |
| `inillucent-wal` | `src/record.rs` | `Body::Relink`, `Body::PageCount`, `RelinkSite`, encode and decode, the round trip tests |
| `inillucent-txn` | `src/redo.rs` | the applier for both records, swizzled swip handling |
| | `src/engine.rs` | page count and map head changes applied after the free map changes |
| `inillucent-tree` | `src/paged.rs` | `relocate(page, to, txn)`, `find_parent`, `find_left_neighbour`, `relink_extent(leaf, from, to)`, owner key in `write_extent` and `write_packed_extent`, `read_extent` follows `next` |
| | `src/write.rs` | `prev` maintained where chains are written; nothing else |
| | `src/paged/bulk.rs` | a root slot parameter |
| | `src/paged/occupancy.rs` | unchanged, used by the tests |
| `inillucent-engine` | `src/shrink.rs` (new) | the step of 9.1 |
| | `src/engine/batch.rs` | the step inside `commit_across`; the checkpoint request after `vote` |
| | `src/engine/compiled.rs` | the checkpoint request on the autocommit path |
| | `src/engine/state.rs` | `incremental_steps` queue; `max_page_count` cell removed; `auto_vacuum` cell documented as pending |
| | `src/pragma/tuning.rs` | `pragma_auto_vacuum` with the pending rule and `largest_root`; `pragma_incremental_vacuum` queues and answers rows; `pragma_max_page_count` on the file |
| | `src/pragma.rs` | the constant moves out; dispatch unchanged |
| | `src/lib.rs` | `reclaim_free_pages` removed |
| | `src/rebuild.rs` | `carry_header` writes the pending mode and `largest_root`; `ConnectionSettings` restores the limit onto the `Database` |
| | `src/ddl/catalog.rs`, `src/ddl/tree.rs` | the root slot at create; the root move and catalog rewrite at drop |
| | `src/engine/open.rs`, `src/attach.rs` | the rename of the trim; format 3 accepted |
| | `src/recovery.rs` | the two records in the open path replay |
| | `src/checkpoint.rs` | nothing but the comment that the cut now happens in `Pool::checkpoint` |
| | `src/readonly.rs` | the comment at 97 to 99 that says `max_page_count` writes a header field, corrected |
| `inillucent-cli` | `src/shell.rs` | prints a row with zero columns as an empty line, if it does not already |
| | `src/diagnose.rs` | `.dbinfo` line for `largest root` beside `incremental vacuum` |
| `inillucent-driver` | `src/capability.rs` | the two rows of 11.4 |
| tests | `tests/selection.toml`, `tests/timings.toml` | rows for `engine::auto_vacuum`, `durability::auto_vacuum_crash`, the nightly test |
| | `crates/inillucent-compat/tests/engine/auto_vacuum.rs`, `tests/durability/auto_vacuum_crash.rs`, `tests/nightly/<name>.rs` | new |
| | `tests/engine/vacuum_on_vfs.rs`, `tests/differential/new_engine_pragma.rs`, `tests/differential/semantics.rs`, `tests/durability/process_maintenance.rs`, `tests/durability/process_readers.rs`, `tests/engine/encryption_at_rest.rs` | the changes of 11.1 to 11.5 |
| | `compat/corpus/usage/auto-vacuum.sql`, `compat/corpus/usage/known.toml` | the corpus cases; the rewritten deviation row |
| docs | `docs/pragmas.md` lines 26, 55, 65 | what each pragma does now |
| | `docs/sql.md:366` | `max_page_count` is enforced; the 32768 byte page note stays |
| | `docs/feature-comparison.md` 536, 545, 572 | `prag.autovacuum` and `prag.max.page` results; `incremental_vacuum` leaves the silent pragma list |
| | `docs/relational-architecture.md` sections 3, 5, 6, 7 | format 3, the extent layout, the two records, the step, the root rule |
| | `docs/glossary.md` | auto vacuum, incremental vacuum |
| | `CHANGELOG.md` | the entry |
| | `agent-skills/inillucent-troubleshoot/`, `agent-skills/inillucent-query/` | status `full` means the file is at its `max_page_count`; then `node tools/sync-skills.mjs` |
| | `tests/inillucent-testing-tdd.md` section 2 | the per tier counts |
| | `src/data/documentation.ts` in `sites/inillucent` of `black-rainbow-labs-sites` | the pragma chapter |
| | `tasks/task-2217-bug-hunt-8.md` section 4 | the row moves to section 3 when the fix lands |

`docs/invariants/layering.toml` does not change: `inillucent-pool` still depends only on
`inillucent-base` and `inillucent-vfs`, which is why the step lives in the engine and the records
in the log crate, with the pool offering primitives, as `log_free_map_pages` does today.

## 14. Order of work, and what to check first

1. **Format 3 and the primitives.** `meta.rs`, `page.rs`, `extent.rs`, `freemap.rs`, `file.rs`,
   `pool.rs`, `record.rs`, `redo.rs`. Every existing test still passes with no caller of the new
   code. Then `relocate` in `paged.rs` with the engine tests of 11.2 that move a single leaf,
   interior, chain and shared page.
2. **The step and the pragmas.** `shrink.rs`, `commit_across`, the pragma rules, the checkpoint
   cut. The rest of 11.2 and all of 11.3.
3. **Roots.** `largest_root`, create, drop, `VACUUM`. The three root tests.
4. **`max_page_count`.** Half a day. The five tests.
5. **The crash campaigns.** Last, because they grade everything above.
6. **Part E** (E1 and E3) goes before step 2, because the FULL tests in 11.2 assert
   `freelist_count` 0 and a small `page_count` after a delete, which only holds once the delete
   frees its leaves and its values. E2 goes last, after the campaigns are green.

Three things to check before writing code, each a one line answer:

- Does every output mode of the shell, the CLI, MCP and the C API carry a row with zero columns?
  (9.4) List mode does.
- Does the undo of a tombstoned sorted row go through `undo_to_floor` with the value bytes, as a
  delta row's does? (9.5, 12.2 E3) The before image type says yes; read `delete_near` once to
  confirm the image is taken before the tombstone is set.
- Does `header_accounts_for_every_object` (`open.rs:907`) still hold after a `PageCount` replay
  lowered the count? It must, since every root is below `largest_root` and `largest_root` is below
  the new end. Assert it in the recovery path, and do not skip the trim when it fails.

Risks, named:

- **Write amplification in FULL mode.** A delete that frees half of a 10 GB table moves up to half
  the live pages above the new end, each a 32 KB image in the log, and then checkpoints. SQLite's
  FULL mode has the same cost and the same reputation. INCREMENTAL exists for applications that
  cannot pay it at commit. The documentation says so.
- **The checkpoint at commit.** It is the price of the file being short when the statement
  returns. It runs only when `EXCLUSIVE` is free at once (9.2), so a busy multi process file
  shrinks at the next checkpoint instead, and a commit never reports `busy` for a cut.
- **Format 3.** Older builds refuse the new files, by the rule the meta record already states.
  The release notes say so, and `inillucent migrate` is the way back.

## 15. Jargon

| Written here | Meaning |
|---|---|
| `Relink` | the new log record that says one referrer of a moved page now names the new page |
| `PageCount` | the new log record that says where the file logically ends |
| `largest_root` | the new meta field: the highest root page number, in mode 1 or 2 |
| root slot | `largest_root + 1`, the page a new table or index gets as its root |
| the lock page | the page under the operating system lock bytes at 1 GiB, which the free map always shows as allocated |
| pending mode | an `auto_vacuum` value set on a file with tables, held on the connection until `VACUUM` applies it |

## 15. Deviations from this design, as built

- **The file is cut by an engine checkpoint that moved the recovery point**, not by the pool's
  checkpoint. The pool's checkpoint also runs from the open after a replay, where the log still
  holds records that name the pages past the new end. `Pool::lower_page_count` marks a cut as
  pending and `Database::cut_to_page_count` cuts, called from `checkpoint_within` and
  `checkpoint_attached`. The open does not trim a file while a cut is pending.
- **The checkpoint after a shrink syncs the data file between its pages and its meta record**
  (`Pool::sync_pages_before_the_next_meta`). The ordinary checkpoint writes the pages and the
  shadow meta slot and syncs once. A power loss inside that sync can leave the new record
  durable beside a torn catalog page that the log no longer covers, because the new recovery
  point is past the page images. A crash campaign at every sync of a `FULL` commit found it. The
  ordinary checkpoint has the same window and is left as it is; it is listed as remaining work.
- **A deleted row's out of line values are freed by overwriting the row's reference with a dead
  reference** (`ExtentRef::dead`), so a later repack and a later delete do not free the same
  chain twice and a page walk skips it.
- **The referrer of a page is found by a depth first walk of the tree that can own it**, using the
  owner tree and key stored in an owned extent page, with no pointer map.
- **`max_page_count` is held per file and per connection** on `inillucent_pool::Database`, and
  `ConnectionSettings` restores it onto the reopened `Database` after `VACUUM`.
