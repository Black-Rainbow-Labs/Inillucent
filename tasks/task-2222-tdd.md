# Keys and indexed values larger than a page

## Summary

A key column is never stored out of line today. An indexed or key value over about 32 KB is
refused with "this row is larger than a page even with its large values stored outside it", and
two keys of about 17 KB cannot share a leaf, so a split with them fails inside the encoder.
SQLite accepts a 50,000 character indexed TEXT value. This design removes the exception.

The change has four parts.

1. **A key column value longer than the spill threshold goes out of line**, the same threshold
   every other column uses: `page_size / 8`, which is 4,096 bytes at the default page size. The
   leaf keeps the sixteen byte extent reference it keeps for any other spilled value, and after
   it the first 512 bytes of the value. A comparison reads the prefix first and reads the rest
   from the extent only when the prefix does not decide. A leaf resolves the rest through a
   pool reference attached to it, the way resolved extents are attached today, and a leaf with
   no pool attached returns an error on a tie. It never guesses an order.
2. **A separator is shortened when it is long.** The separator written at a split is the
   shortest prefix of the right half's first key that sorts above the left half's last key, when
   the full key is over 4,096 encoded bytes. A separator that is still over the limit after
   shortening, which only two neighbouring keys sharing a prefix that long can produce, goes out
   of line with the same reference plus prefix form, in the interior page.
3. **The file format moves from 2 to 3 the first time a file holds a key out of line.** A file
   with no such key keeps saying 2, and every release from 0.1.8 on still opens it. A file that
   says 3 is refused by every earlier release by name, with status `unsupported`. A new log
   record, written before the first key extent, makes a crashed log refused by kind by every
   earlier release, which is how every log record added since 0.1.8 has been handled. Files
   written by released versions open with no migration.
4. **Nothing on the path of an ordinary key changes.** The branches the change adds are inside
   arms that are error paths today, or behind a page flag that is clear on every page an
   ordinary tree writes. The gate is expected to move within noise, and the measurement that
   proves it is in the cost section.

An implementation follows the build order at the end. Each stage has its own red tests.

## Terms used in this document

| Term | Meaning here |
|---|---|
| key column | one of the first `key_columns` columns of a tree's rows. For an index that is every indexed column and then the row identity. For a `WITHOUT ROWID` table it is the primary key |
| spill threshold | `page_size / EXTENT_DIVISOR`, with `EXTENT_DIVISOR = 8` in `crates/inillucent-tree/src/leaf.rs`. 4,096 bytes at the default 32,768 byte page |
| extent | a value stored on pages of its own, named from a leaf by a sixteen byte `ExtentRef` (`crates/inillucent-pool/src/extent.rs`) |
| key extent | an extent holding a key column value, which this design introduces |
| prefix | the first `KEY_PREFIX_BYTES` bytes of a value whose rest is in a key extent |
| separator | the encoded key an interior page holds between two children. Child `i` holds keys `K[i-1] <= k < K[i]` |
| encoded key | the memcmp comparable bytes `KeyEncoding::encode` produces (`crates/inillucent-tree/src/keyenc.rs`). Interior pages and descents compare these. A leaf compares decoded values |
| the pool | `inillucent_pool::Pool`, which reads pages. `read_extent(pool, reference)` in `crates/inillucent-tree/src/paged.rs` reads an extent through it |

## What is there today

These are the facts the design rests on. Line numbers are from commit `e7f7bdc7`.

- A key is ordinary column values. There is no key area in a leaf. `COLUMN_KEY` in the column
  directory marks a key column (`crates/inillucent-tree/src/types.rs:288`). A text key is a heap
  pair in its mini column and bytes in the page heap, like any text.
- One function says keys never spill: `LeafBuilder::threshold` returns `usize::MAX` for
  `column < key_columns` (`crates/inillucent-tree/src/leaf/encode.rs:614`). The delta path has
  the same rule in `PagedTree::out_of_line` (`crates/inillucent-tree/src/write.rs:1505`). The
  tagged form a delta row uses has a `tag::EXTENT` of seventeen bytes, and its comment says a
  key position never carries one (`crates/inillucent-tree/src/datum.rs:81`).
- A leaf compares decoded values under the column's collation: `compare_key_with` reads each
  key column's `Datum` and calls `compare_under` (`crates/inillucent-tree/src/leaf/compare.rs:60`).
  A value of class `Extent` is answered from an `Extents` table the caller attached with
  `LeafRef::with_extents`, and refused with `misuse` when none is attached
  (`crates/inillucent-tree/src/leaf/read.rs:1593`). `LeafRef` has no pool.
- A probe reads only key columns to find the row, and the comment says that is safe "because a
  key is never out of line" (`crates/inillucent-tree/src/paged/cursor.rs:635`).
- An interior page stores the full encoded first key of the right half as the separator, with no
  shortening and no size limit other than the page (`crates/inillucent-pool/src/interior.rs:464`).
  A descent compares with `memcmp` in `child_for` (`interior.rs:235`). The integrity check only
  requires a separator to bound its children (`PagedTree::key_range_under`, `paged.rs:1352`), so
  a shortened separator passes it.
- A split prices the left half and never the right half (`rows_for_the_left_half`, `write.rs:108`;
  `split_carrying`, `write.rs:2434`). With keys of 17 KB the right half can be two rows of 34 KB,
  and the encoder then fails with `misuse("the heap overflowed the page")`, which the front end
  reports as "bad parameter or other API misuse".
- The file format version is a `u32` at byte 8 of the meta page. `FORMAT_VERSION = 2`,
  `OLDEST_FORMAT_VERSION = 1`, and `Meta::encode` writes the constant unconditionally at every
  checkpoint (`crates/inillucent-pool/src/meta.rs:42`, `:320`). A newer version is refused with
  "this database is format version N and this build reads version 2; upgrade inillucent to open
  it", status `unsupported`, exit code 3. The move from 1 to 2 shipped in 0.1.8.
- A log record kind an older build does not know is refused with "a log record has kind N, which
  this format does not define" (`crates/inillucent-wal/src/record.rs:424`). Kinds 15 to 18 were
  added that way.
- `CompactLeaf` with an empty image and `SplitLeaf` rebuild pages at replay with a spiller that
  refuses to write an extent (`Refuse`, `crates/inillucent-tree/src/split_log.rs`). A leaf that
  holds any extent takes the repack path and is logged as a full image, and a split that writes
  any extent is logged as three images (`Structural`).
- `PagedTree::read_extents` and `PagedTree::extent_refs` walk every column of every row,
  including key columns (`paged.rs:1188`, `:1226`). DROP, VACUUM's page ownership check and the
  repack frees all go through them.
- The gate's index keys are integers plus the rowid. The only text key in the contract plan is
  `label`, about 50 bytes, under `schema.index`. No workload holds a key over 4,096 bytes.

## Decisions

### 1. A key column value over the spill threshold goes out of line, with a prefix

`LeafBuilder::threshold` returns `page_size / EXTENT_DIVISOR` for a key column the way it does
for any other column. `PagedTree::out_of_line` drops its `column < self.key_columns()` test.
`classify_at` is unchanged: a text or blob over the threshold is `ValueClass::Extent`.

What the heap holds for a key column of class `Extent` is different from what it holds for any
other column:

| bytes | field |
|---|---|
| 0..16 | the `ExtentRef`, exactly the sixteen bytes any other spilled value's slot points at |
| 16..18 | `u16` prefix length, `KEY_PREFIX_BYTES` for every value this build writes |
| 18.. | the prefix: the first `KEY_PREFIX_BYTES` bytes of the value, raw, not encoded |

`KEY_PREFIX_BYTES` is `min(512, page_size / EXTENT_DIVISOR)`: 512 at every page size from 4,096
up, and 64 at a 512 byte page. The prefix is never longer than the threshold, so a spilled value
is always longer than its prefix.

Why a prefix at all: a leaf of long keys holds about 55 rows at a full page, and a binary search
over it makes six comparisons. Without a prefix each of those is an extent read of the whole
value, forty kilobytes copied to decide one byte. With the prefix a search of a leaf of distinct
documents never touches the pool. SQLite keeps a local part of about 500 to 1,000 bytes in the
cell for the same reason.

Why the same threshold as every other column, and not a larger one for keys: one rule, one
test, and a key between 4 KB and 16 KB inline today gives a leaf of six rows. Out of line with a
528 byte heap cost it gives fifty. The one cost of this choice is compatibility: a file holding a
5 KB key written by this build moves to format 3, where today it stays at 2. That is accepted.

Why the length is stored when it is always `KEY_PREFIX_BYTES`: the tagged form in the log is
decoded by `tagged_span`, which has no page size at hand, and two bytes per long key is nothing.
The heap record and the tagged record then have one shape.

What changes for the readers:

- `MiniColumn::extent(row)` reads bytes `0..16` of the heap record and is unchanged.
- A new `MiniColumn::key_prefix(row) -> DbResult<&'p [u8]>` reads the prefix, and refuses with
  `corrupt` when the column is not a key column or the length runs past the page.
- `heap_cost_of` prices a key column's `Extent` at `EXTENT_REF_BYTES + 2 + KEY_PREFIX_BYTES`.
- `Shape::observe` is unchanged: an `Extent` slot holds a heap offset, as it does today.
- `encode_rows_with` writes the record above after the spiller returns the reference. The
  builder has the value in hand, so the prefix is `value[..KEY_PREFIX_BYTES]`.
- `LeafRef::integrity` checks every key extent record: the reference decodes, the prefix length
  equals `KEY_PREFIX_BYTES` for the page size, and the prefix fits the page.

A non key column's heap record is unchanged. Every file written before this change has no key
extent, so "the prefix follows the reference exactly when the column is a key column" holds for
every file.

### 2. How a leaf compares a prefixed key

`compare_key_with` compares one key column at a time and stops at the first that is not equal.
For a held value of class `Extent` in a key column, the comparison runs in three steps:

1. **Class.** `extent_datum`'s rule says whether the held value is text or blob. If the probe is
   `Null`, a number, or the other of text and blob, the class order decides, exactly as
   `Datum::compare` decides it today.
2. **Prefix.** The prefix decides when a byte differs inside the first
   `min(prefix.len(), probe.len())` bytes, under these rules:

   | collation | rule |
   |---|---|
   | BINARY, and every blob | compare the bytes; the first differing byte decides |
   | NOCASE | fold each byte with `to_ascii_lowercase` and compare; the first differing folded byte decides. `compare_nocase` stops only when both sides hold a NUL at the same position, and then it compares lengths, which the prefix cannot do, so that case falls to step 3 |
   | RTRIM, DECIMAL, UINT, a custom collation | the prefix never decides |

   The rule for BINARY and NOCASE is sound because both comparators walk the bytes from the
   front and return at the first difference, so the full comparison would return the same answer
   at the same byte.
3. **The rest.** When the prefix does not decide, the whole value is read and compared with
   `compare_under` as today. Where it is read from is decision 3.

A probe shorter than the prefix that matches the prefix for its whole length is not decided by
the prefix either. It could be: the held value is longer than the probe. That case is left to
step 3 on purpose, so the rule stays one sentence and its test stays one table.

`compare_delta_key` applies the same three steps to a delta row's key column holding the tagged
key extent form of decision 4.

### 3. Who reads the rest: a pool attached to the leaf

`LeafRef` gains `pool: Option<&'p Pool>` beside its `extents: Option<&'p Extents>`, and
`LeafRef::with_pool(self, pool: &'p Pool) -> LeafRef<'p>` sets it, the way `with_extents` does.

Step 3 of decision 2 reads the whole value in this order:

1. from the attached `Extents` table, when one is attached and holds this row and column;
2. else through the attached pool, with `read_extent(pool, reference)`;
3. else it refuses: `misuse("this key is stored out of line; attach the tree's pool to the leaf
   before searching it")`.

The third answer is the property the design is built around: **a call site that forgets to
attach the pool returns an error on the first tie, never a wrong order.** Ordinary keys are of
class `Typed` and never reach any of the three.

The tree attaches the pool wherever it searches a leaf. The call sites, every one of which has a
pool in hand:

- `PagedTree::probe_leaf` and `hit_in` (`paged/cursor.rs`), the point lookup and the uniqueness
  check;
- `visit_from_reading`, `visit_span`, `visit_range`, `lower_bound`, `upper_bound` and
  `visit_reverse` (`paged/cursor.rs`);
- `skip_scan` (`paged/skip.rs`);
- `LeafMut::locate` and `locate_and_read_previous` (`mutate.rs`, `write.rs`), on the write path;
- `delete_sorted`, `update_sorted` and `repack_run` (`write/sorted.rs`);
- `TreeRows` in `crates/inillucent-txn/src/redo.rs`, which locates by key at replay;
- `PagedTree::check` (`paged.rs`), the integrity check, which compares keys across the leaf
  chain with `compare_rows_under` and must read a full key to do it.

`compare_rows` and `compare_rows_under` take plain `&[Datum]` and keep doing so: a caller that
has materialised rows has already resolved every extent through `read_extents`.

### 4. Delta rows and the tagged form

A delta row is a `u16` length and then one tagged value per column. A key column whose value is
out of line is written as a new tag:

| tag | name | bytes after the tag |
|---|---|---|
| 5 | `EXTENT` | the sixteen byte reference, as today, never in a key column |
| 6 | `KEY_EXTENT` | the sixteen byte reference, a `u16` prefix length, the prefix |

`encode_row_spilling_wide_values` writes `KEY_EXTENT` for a key column whose value
`out_of_line` returns bytes for, after `write_extent` has written the pages. `tagged_span` steps
over it by the stored length. `decode_tagged` refuses it the way it refuses `EXTENT`, with a
message naming the key. `delta_extent_at` and `orphaned_extents` recognise it, so the references
are read, freed and counted exactly as a non key extent's are.

The tag is a new byte value, not a reuse of tag 5 followed by a prefix in key positions, because a
decoder that has to know the column's position to know a value's length is a decoder that reads
the wrong field the first time a caller passes a slice.

`InsertRow` log records carry the row in this form. `DeleteRow`, `DeleteRows` and
`UpdateInPlace` carry the key as tagged datums of the full value, as today; the full value is at
most `MAX_KEY_BYTES` (decision 11), which fits the 4 MiB record limit.

### 5. Separators: shortened when long, out of line when they cannot be shortened

Two constants in `crates/inillucent-pool/src/interior.rs`:

- `SEPARATOR_INLINE_LIMIT = page_size / 8`, 4,096 bytes at the default page size. An interior
  page then always holds at least seven children.
- `SEPARATOR_PREFIX_BYTES = KEY_PREFIX_BYTES`, the same number as decision 1.

**Shortening.** A split and a bulk build have both neighbours of a boundary in hand: `L`, the
encoded last key of the left half, and `R`, the encoded first key of the right half, with
`L < R`. When `R.len() > SEPARATOR_INLINE_LIMIT`, the separator is `R[..i + 1]` where `i` is the
first position at which `L` and `R` differ, or `L.len()` when `L` is a prefix of `R`. This is the
shortest prefix of `R` that sorts above `L`: it has a larger byte at position `i`, or `L` is a
proper prefix of it. It sorts at or below `R` because it is a prefix of `R`. So every key of the
left child is below it and every key of the right child is at or above it, which is the
invariant `K[i-1] <= k < K[i]` that `child_for` and `key_range_under` rely on. Nothing in the
tree relies on a separator being equal to a key.

The shortening runs only when the full separator is over the limit. An ordinary split writes the
bytes it writes today, so the interior pages of an ordinary tree, the `SplitLeaf` records that
carry a separator and their replay are all byte for byte what they are now. The cost on an
ordinary split is one length comparison.

**Out of line.** When the shortened separator is still over the limit, two neighbouring keys
share a prefix longer than 4,096 encoded bytes. That is what an index over a column with
duplicates of a long value produces: the key tuple is the value then the rowid, and the boundary
between two leaves of the same value needs the whole value. The separator then goes out of line:

- its full encoded bytes are written with `write_extent` as an extent of the tree;
- the interior slot's bytes are the sixteen byte reference, a `u16` prefix length and the first
  `SEPARATOR_PREFIX_BYTES` of the encoded separator;
- the slot's `u32` length word carries `SEPARATOR_OUT_OF_LINE = 1 << 31`. A page is at most
  65,536 bytes, so the bit has never been set in any file;
- the interior page's flag byte, byte 13 of the common header, carries
  `INTERIOR_HAS_EXTENTS = 0b0000_0001`. Interior pages use no flag bit today, so every existing
  page has it clear.

The slot form is the leaf's heap record form of decision 1 over encoded bytes instead of raw
ones. One shape, two page kinds.

`InteriorRef` gains:

- `separator_is_out_of_line(slot) -> bool`, from the length word;
- `separator_slot(slot) -> (bool, &[u8])`, the raw bytes with the flag, for code that copies
  slots into a rebuilt page: `split_log::read_interior`, `parent_image`, `insert_separator`'s
  split by count, and `merge_if_small`'s removal. A reference moves between pages as sixteen
  opaque bytes and is written once and freed once;
- `child_for_resolving(probe, resolve: &mut dyn FnMut(ExtentRef) -> DbResult<Vec<u8>>)`, used
  when `INTERIOR_HAS_EXTENTS` is set. It compares the probe against an out of line separator's
  prefix with `memcmp`; a difference inside `min(prefix.len(), probe.len())` bytes decides; a
  probe that is at most `prefix.len()` long and matches the prefix for its whole length is below
  the separator, because the separator is longer; otherwise it calls `resolve` and compares the
  whole bytes. The `leading_u64` interpolation guess reads the first eight bytes of the prefix,
  which are the first eight bytes of the separator, so it is unchanged;
- `child_for(probe)` is unchanged and is what every descent calls when the page flag is clear.

`InteriorBuilder::build` and `fits` take separators as `(out_of_line: bool, bytes)` pairs and set
the flag byte when any is out of line. `capacity`, the bulk loader's greedy packer, counts the
slot bytes, which for an out of line separator are 18 plus the prefix.

Outside the interior page, a separator is always its full bytes. `descend_fenced` copies fences
into owned vectors and `LeafHint` copies its `low` and `high` from them; both resolve an out of
line separator to its full encoded bytes at copy time. `key_range_under` resolves before it
compares. Those copies are made per descent and per hint, and only for a separator that is out of
line.

### 6. Descent, probe, cursors, hints, skip scan

- A descent (`descend`, `descend_guard`, `descend_fenced` in `paged/descent.rs`) reads the
  interior page's flag byte once and calls `child_for` or `child_for_resolving` with a closure
  over `read_extent(pool, reference)`.
- `probe` is unchanged in shape. Its descent goes through the rule above, its leaf search goes
  through decision 2 with the pool attached, and `read_hit` reads the hit row's extents, which
  now include its key columns, so `value_at` on a key column answers the whole value. The
  comment at `cursor.rs:635` changes to say what is now true: finding the row reads the key
  prefix, and reads the rest only when the prefix ties.
- `run_may_continue` and `probe_right_of`, the prefix probe walk across siblings, compare the
  probe with the next leaf's first key through `compare_key_with` and need no change beyond the
  attached pool.
- `skip_scan` copies the current prefix out of a row with `OwnedDatum::from_datum(&leaf.value(..))`.
  `leaf.value` on a key extent answers from the attached `Extents`, which `with_leaf_extents`
  already resolves for every walk, so the copy is the whole value. The seek after it encodes
  that value and descends.
- `leaf_for_hinted` compares the encoded key against `low` and `high` with `memcmp`, which are
  full bytes by decision 5. Unchanged.

### 7. Splits, repacks, merges and the bulk build

- **The right half is priced.** `split_carrying` prices the right half with
  `builder.pack_with(right_rows, 1.0, Some(&mut Measuring))` before it encodes anything, and
  returns `row_larger_than_a_page()` when the right half does not fit. That turns the encoder's
  `misuse("the heap overflowed the page")` into the statement refusal it should have been. With
  key extents, a row's inline cost is bounded by its column count times the threshold, so the
  case only remains for the "row of many values just under the page's eighth" the refusal text
  already names.
- `rows_for_the_left_half` is unchanged.
- `Measuring`, `Extender` and `Carrying` spill a key column exactly as they spill any other. The
  builder writes the prefix. `Carrying` keeps a reference that is already out of line (`used`),
  so a repack of a leaf with key extents rewrites no extent.
- `repack`, `compact_into`, `merge_if_small` and `merged_image` are unchanged in shape: a leaf
  with any extent already takes the image logging path, and `held` minus `kept` frees what the
  repack dropped. Key extents are in `held` because `extent_refs` walks every column.
- `merge_if_small` removes the right sibling's separator from the parent. When that separator is
  out of line, its reference is freed with `free_extent` after the `Structural` record is logged
  and the pages are installed, beside the leaf extents it frees today.
- `insert_separator` with a parent that does not fit splits it by child count as today. The
  slot bytes move with their flag. No extent is written or freed by an interior split.
- `plan_leaves` (`paged/bulk.rs`) spills key columns through its spiller like any other, and
  computes each leaf's separator against the previous leaf's last key with the rule of decision
  5, writing an extent for one that stays over the limit. `build_interior_levels` passes the
  `(flag, bytes)` pairs to `InteriorBuilder`. A level that does not shrink is still stopped by
  the 32 level guard, and with every separator slot at most 4,096 bytes an interior page always
  holds at least seven separators, so every level is at most an eighth of the one below it.
- The in memory `Tree` (`tree.rs`) is not a paged tree and does not change.

### 8. The write ahead log and recovery

**One new record.** `Body::Format { version: u32 }`, kind 19, owned by no transaction. It is
written once per file, before the first `AllocPage` of the first key extent or separator extent,
by `Database::require_format(3, log)`, which does nothing when the file is already at 3. Replay
raises the in memory meta's format to the record's value and nothing else. An earlier release
meeting it refuses with "a log record has kind 19, which this format does not define", the
answer it gives for kinds 15 to 18.

**Nothing else is new in the log.** The pages of a key extent are logged as `AllocPage` and
`WritePage` before the leaf record, as any extent's are. A leaf holding a key extent is logged as
a full image by `CompactLeaf`, because `choose_fit` already sends a leaf with extents to the
repack path. A split that writes a key extent or a separator extent is logged as `Structural`
with three images, because it moved something. A split whose separator was only shortened is
logged as `SplitLeaf` when it is logical today: the record's `separator` field carries the
shortened bytes and `parent_image` inserts them. The rebuilt parent is therefore the installed
parent, which `check_logical_split` asserts in debug builds.

**Replay of row records.** `TreeRows::insert_row` decodes the key columns from the front of the
record to locate the leaf. A `KEY_EXTENT` tag at the front is decoded by reading the extent
through the pool: its pages were applied before this record, in the first pass that copies
images, so the read answers. The decoded datums are owned for the duration of the locate. The
comment at `redo.rs:590` that the front of the record always decodes because keys are never out
of line is replaced by that sentence. `delete_row` and `update_in_place` decode full values, as
today, and locate with the pool attached.

**Determinism.** `replay_compaction` and `split_left_image` rebuild pages with the `Refuse`
spiller, so a replay never writes an extent. A key column value that is already out of line is a
reference plus prefix in the rows the replay reads, and the builder writes it back as it read it.
A replay is never asked to spill a key, for the same reason it is never asked to spill a value:
the write path logs an image whenever it did.

**Undo.** `KeyCopy::of(key)` copies the key as datums, whole. A 1 MiB key copies 1 MiB. Nothing
changes.

### 9. Freeing, occupancy, VACUUM and the integrity check

- A key extent is freed where a value extent is: after a delta row is overwritten
  (`apply_row`), after a delete of a delta row (`delete_near`), at the next repack for a
  tombstoned sorted row, after a merge, and at commit for a dropped tree. `extent_refs` and
  `orphaned_extents` see it because they walk every column and every tag.
- A separator extent is freed when its separator leaves the tree: on a merge, and when the tree
  is dropped. `PagedTree::released` (`paged/occupancy.rs`) walks interior pages already, as
  owned pages; it adds each page's out of line separators as `Freed::Value`.
- `pages_occupied` lists separator extents as it lists leaf extents, so the page ownership check
  in `crates/inillucent-engine/src/engine/pages.rs` sees every page the tree holds and reports a
  key extent two trees name as corruption.
- VACUUM is a logical copy and reads rows through the normal paths. It needs nothing.
- `PagedTree::check` compares consecutive live keys with full values through `read_extents`,
  `check_subtree` resolves an out of line separator before `key_range_under` compares, and
  `LeafRef::integrity` and `InteriorRef::validate` check each record's shape as decision 1 and 5
  say.

### 10. The format version moves to 3 the first time a file holds a key extent

`crates/inillucent-pool/src/meta.rs`:

- `FORMAT_VERSION` becomes 3. `reads_format` accepts 1 to 3. `wrong_format` is unchanged and now
  says "this build reads version 3".
- A new constant `FORMAT_WITHOUT_KEY_EXTENTS = 2` names what a file is before its first key
  extent.
- `Meta` gains `format: u32`. `decode` reads it from byte 8. `encode` writes
  `max(meta.format, FORMAT_WITHOUT_KEY_EXTENTS)`: a format 1 file is still rewritten as 2 at its
  first checkpoint, exactly as today, and a format 2 file stays 2 until something raises it.
- `require_format(wanted, log)` raises `meta.format` to `wanted` and logs `Body::Format` when
  it was lower. It lives on whichever object owns the in memory `Meta` during a transaction,
  which is the handle the write path allocates pages through (`database.allocate` in
  `split_carrying`) if that handle owns it, and beside the `Meta` otherwise. It is the one place
  the number moves, and decision 8 says who calls it.
- A file created by this build says 2.

What a reader of each release sees:

| file | this build | 0.1.8 to 2.4.2 | 0.1.5 to 0.1.7 | before 0.1.5 |
|---|---|---|---|---|
| format 1, written by 0.1.1 to 0.1.7 | opens, rewrites as 2 at the first checkpoint | the same | opens | opens |
| format 2, no key extent, written by any build from 0.1.8 on, this one included | opens, stays 2 | opens | refused by name, `unsupported` | malformed |
| format 3, holds or held a key extent | opens | refused by name: "format version 3", `unsupported`, exit 3 | the same | malformed |
| format 2 in the file, log holds `Format { 3 }` and key extent pages, after a crash | recovers | refused: "a log record has kind 19, which this format does not define" | the same | the same |

The invariant that makes the last two rows true: **the data file never holds a key extent page
or a key extent leaf while its meta says 2 and its log holds no `Format` record.** In log mode
the pages reach the file at a checkpoint, which rewrites the meta from the in memory `Meta`,
whose format was raised before the first key extent was logged, and the `Format` record is below
the checkpoint LSN until the checkpoint completes. In rollback journal mode the implementer
confirms that the meta page is written in the same commit as the pages, and the test in the
tests section holds it.

A file that once held a key extent and no longer does stays at 3. The number says what a reader
has to be able to read, and lowering it would need a walk of every leaf and interior page.

Why the number moves only when a file needs it, when the move to 2 happened at every
checkpoint: a file with no key extent is byte for byte a format 2 file, and stamping it 3 would make every
release from 0.1.8 refuse a file it reads correctly. The move to 2 changed every leaf a build
wrote, so there was no such file. The price of the lazy move is the `Format` record and the
`format` field, about forty lines and two tests.

### 11. The maximum key

`MAX_KEY_BYTES = 1 << 20`. A key tuple whose encoded form is longer is refused before any page
is touched, with a statement refusal: "an index or primary key entry is longer than 1,048,576
bytes". The refusal runs where the tree encodes the key for the descent, so it covers an insert,
an update, a `CREATE INDEX` over existing rows and a `WITHOUT ROWID` primary key alike.

Why a limit at all: `DeleteRow` and `UpdateInPlace` carry the full key, and a record is at most
4 MiB. Why one megabyte: it is far above the 50,000 character case and leaves three quarters of a
record for the rest of a batched `DeleteRows`. SQLite's limit is one gigabyte and nothing here
needs it.

### 12. What is still refused

- A row whose columns, each under the threshold, add up to more than a page. The refusal text in
  `row_larger_than_a_page` drops its sentence about keys and keeps the one about rows of many
  values just under the page's eighth.
- A key over `MAX_KEY_BYTES`.
- An FTS5 token over `MAX_TOKEN_BYTES = 1000` is still clipped. That is a tokenizer decision and
  not a page one.

## Cost on the gate for ordinary keys

Every key in the contract plan is an integer plus the rowid, or `label` at about 50 bytes. For
those keys these are the only instructions the change adds or removes on a hot path:

| path | today | after | cost per call |
|---|---|---|---|
| `LeafBuilder::threshold` | `if !spilling \|\| column < key_columns` | `if !spilling` | one comparison fewer |
| `PagedTree::out_of_line` | `if column < key_columns { return None }` then the length test | the length test | one comparison fewer |
| `MiniColumn::value`, `compare_key_with` | the `Extent` arm returns `misuse` | the `Extent` arm compares a prefix | zero: the arm is not taken for a `Typed` key, and the `COLUMN_ALL_TYPED` fast path skips the class array entirely |
| `compare_delta_key` | a `match` on the tag byte | the same `match` with one more arm | zero: a jump table does not grow slower by one arm |
| a descent | `child_for` | one read of the page flag byte, then `child_for` | one byte load and a branch per interior page, about a nanosecond, against a `memcmp` loop over up to sixteen slots |
| a split | encode the separator | encode it, compare its length with 4,096 | one comparison per split |
| `LeafRef::parse` | | one more pointer field, `None` | zero: the struct is built once per leaf visit |
| the log | | nothing for a file with no key extent | zero |

The expected movement of every family ratio is inside the bootstrap interval the gate prints,
and the estimate is that no workload moves by more than a quarter of a percent. The descent flag
read is the only addition on a read path, and `read.point` makes three descents per lookup at
the medium fixture against about 170 ns per lookup.

The measurement that proves it, run by the implementer after stage 4 of the build order:

```sh
pwsh tools/capped.ps1 cargo build --release -p inillucent-compat --bin inillucent-fullgate
target/release/inillucent-fullgate _agent_output/fixtures/medium-run1.db --scale medium --rounds 30 --page-size 32768 --frames 4096 --samples _agent_output/task-2222/after.json
```

against the same command from a release build of `main` writing `before.json`. Both runs follow
the rules in `docs/performance.md`: the pinned SQLite, a fresh fixture copy, the performance
cores, and a second run after a quiet window because the first timing on this machine drifts
(two runs of each; keep the pair whose SQLite times agree within 3%). The change passes when,
for each of `write.insert.batch`, `write.upsert`, `write.update.indexed`, `write.delete`,
`point.index`, `range.covering`, `range.lookaside`, `scan.sort`, `schema.index` and `txn.large`,
the after ratio lies inside the before run's 95% interval. The numbers go in the implementation
ticket's comment and in `docs/performance.md` only if a published number moves.

## Tests

### Existing tests that must keep passing

Run with `target/debug/inillucent-testrun --changed origin/main --strict`, with `.sqlite-ref/`
and `_agent_output/fixtures/` copied into the worktree. These are the ones the change bears on:

- `inillucent-tree` lib (unit, `requires = ["narrow-slots"]`), in particular
  `leaf/delta/tests.rs`, `leaf/read/tests.rs` and the layout tests in `leaf/layout.rs`. The test
  at `layout.rs:546`, "a key column's threshold is `usize::MAX`", asserts the rule this design
  removes and is replaced by the red test U1 below.
- `inillucent-pool` lib, the `extent.rs` and `interior.rs` and `meta.rs` tests, including
  `a_format_one_record_is_read_and_rewritten_as_format_two` at `meta.rs:770`, which stays true.
- `inillucent-compat::differential::new_engine_extents` (`requires = ["oracle"]`), at a 4,096
  byte page: eleven tests over values around a page, a created index over large values, and a
  text key beside a spilled value.
- `inillucent-compat::engine::new_engine_extent_packing`, `engine::page_sizes`,
  `engine::format_refusal`.
- `inillucent-compat::durability::overflow_crash`, `logical_split_crash`, `vacuum_crash` and
  the `wal_crash` suites. Their rows' `covers` name `inillucent-tree` and `inillucent-pool`, so
  `--changed` selects them.
- `inillucent-compat::e2e::release_format`: every fixture in `tests/interop/<version>/` opens
  and answers as recorded. These are format 1 and format 2 files, and the new build does not
  raise them.
- `inillucent-compat::e2e::cli_commands`, the format 99 refusal at line 2681.
- `inillucent-compat::nightly::release_format_history` (`requires = ["previous-release"]`):
  the `named` string at line 124 becomes "format version 3", and the test's file must be one
  that holds a key extent, or the earlier release opens it. Red test F3 below is the explicit
  form.
- `inillucent-compat::tooling::policy` (`cargo fmt`, line ceilings over `paged.rs`), `selection`,
  `string_literals`, `escapes`, `documentation`, `ci_workflows`.
- The tree crate's `write_campaign`, `value_round_trip` and `fuzz_seeded` targets, and the
  pool crate's `fuzz_seeded`, `journal_ordering` and `fault_campaign`.
- The gate measurement above.

### New tests, red before the change and green after

Unit, in `#[cfg(test)]` of the crate:

- **U1** `crates/inillucent-tree/src/leaf/layout.rs`: a key column's threshold equals
  `page_size / EXTENT_DIVISOR`, and `heap_cost_of` prices a key column `Extent` at
  `18 + KEY_PREFIX_BYTES`.
- **U2** `crates/inillucent-tree/src/leaf/compare.rs`: a table over the collations of decision
  2. For each of BINARY, NOCASE, RTRIM: a probe that differs inside the prefix is decided
  without a pool attached; a probe that ties over the prefix returns `misuse` naming the pool
  when none is attached, and the right order when a fake pool that answers the full value is
  attached. The NOCASE row includes a value holding a NUL inside the prefix on both sides.
- **U3** `crates/inillucent-tree/src/leaf/delta/tests.rs`: a delta row with a `KEY_EXTENT` key
  column round trips through `tagged_span`, `delta_extent_at` and `compare_delta_key`, and an
  `EXTENT` tag in a key position is refused as corrupt.
- **U4** `crates/inillucent-tree/src/leaf/encode.rs`: `pack_rows` over three rows with 40 KB
  keys at a 32 KB page places all three, their heap records decode to the right reference and
  prefix, and `LeafRef::integrity` accepts the page and rejects one whose prefix length was
  altered.
- **U5** `crates/inillucent-pool/src/interior.rs`: the shortening rule over the pairs
  `(abc, abd)`, `(abc, abcd)`, `(a, b)`, two keys sharing 5,000 bytes; a page with one out of
  line separator sets `INTERIOR_HAS_EXTENTS`, `child_for_resolving` routes a probe that ties the
  prefix through the resolver and a probe that is shorter than the prefix to the left without
  calling it; `validate` rejects a slot whose flag is set and whose bytes are shorter than 18.
- **U6** `crates/inillucent-pool/src/meta.rs`: a format 2 record encodes as 2; after
  `require_format(3)` it encodes as 3; format 3 is read; format 4 is refused by name.
- **U7** `crates/inillucent-wal/src/record.rs`: `Body::Format` round trips and has kind 19.

Differential, against the pinned SQLite, in a new file
`crates/inillucent-compat/tests/differential/new_engine_long_keys.rs`, with a `mod` line in
`differential/main.rs` and a row in `tests/selection.toml` with `covers = ["inillucent-engine",
"inillucent-tree", "inillucent-pool"]` and `requires = ["oracle"]`, at a 4,096 byte page so a
key over 512 bytes is out of line and a separator over 512 bytes is shortened:

- **D1** a 50,000 character TEXT value in an indexed column: insert, equality lookup, range
  lookup, `ORDER BY` through the index, `DISTINCT`, `GROUP BY`, delete, reinsert, reopen, each
  answer equal to SQLite's. This is the bug hunt's defect.
- **D2** a `UNIQUE` index over a long column: a duplicate 50,000 character value is refused with
  the constraint status, and an `INSERT OR REPLACE` replaces it.
- **D3** a `WITHOUT ROWID` table with a 20,000 character TEXT primary key, and an `UPDATE` of
  a non key column of that row.
- **D4** 2,000 rows whose keys share a 6,000 byte prefix and differ after it, under BINARY and
  under NOCASE, inserted in random order: every lookup, a full index scan and `integrity_check`
  agree with SQLite. This forces shortened separators that stay over the limit, so separators
  go out of line.
- **D5** 500 rows with the same 40,000 character value in an indexed column: duplicates of a
  long value, the case that forces out of line separators by the rowid tail; then delete every
  other row, then `VACUUM`, then compare, then `integrity_check`.
- **D6** a blob key of 16,000 zero bytes beside one of 15,999, the case where the encoded
  separator is twice the raw key because every zero is escaped.
- **D7** keys of 17,000 bytes, three of them, inserted in the order small, large, large, which
  is the unpriced right half the survey found.
- **D8** a multi column index whose first column is long and whose second decides the order.
- **D9** a key of `MAX_KEY_BYTES + 1` bytes is refused with the statement refusal of decision
  11, and SQLite accepts it; the test records the known difference the way rule 1.3 of the
  testing standard says.

Engine, in `crates/inillucent-compat/tests/engine/`:

- **E1** `long_keys_at_every_page_size.rs`, patterned on `page_sizes.rs`: at 512, 4,096, 32,768
  and 65,536 byte pages, a thousand rows with 20 KB keys, reopen, every row found; the page
  ownership check (`hold_every_page`) reports no page held twice and no leaked page after the
  deletes, which is what proves separator and key extents are freed.
- **E2** `CREATE INDEX` over an existing table of long keys, the bulk path, then `REINDEX`, then
  `integrity_check`.

Durability, in `crates/inillucent-compat/tests/durability/`, a row whose `covers` names
`inillucent-engine`, `inillucent-txn`, `inillucent-wal`, `inillucent-pool` and `inillucent-tree`:

- **C1** `long_key_crash.rs`, patterned on `overflow_crash.rs`: under the simulator, crash at
  every record boundary of a transaction that inserts a 40 KB key into an index whose leaf splits
  and whose separator goes out of line, then recover; after each, `integrity_check` is clean and
  the row is present exactly when the commit record was written. A second arm crashes between
  the `Format` record and the checkpoint and asserts the recovered file's meta says 3.

Format, in `crates/inillucent-compat/tests/e2e/` and `nightly/`:

- **F1** `release_format.rs`: unchanged fixtures still open; a new assertion that opening and
  reading a format 2 fixture and then checkpointing leaves it at 2.
- **F2** a new format 3 fixture in `tests/interop/<this version>/`, built by `build.sql` with
  one long indexed value added, so every later build proves it reads a format 3 file.
- **F3** `release_format_history.rs`: the previous release refuses a file this build wrote with
  a long key, naming "format version 3"; and it opens a file this build wrote with no long key.
- **F4** in `e2e`, with the real shell: a file holding a key extent is checkpointed, its meta
  page byte 8 reads 3, and the shell's `.dbinfo` or `PRAGMA` that reports the format says 3.

Ledger:

- a row in `tests/escapes.toml` for the bug hunt defect, `held_by` D1.

## Every file the change touches

Tree crate, `crates/inillucent-tree/src/`:

- `leaf.rs`: `KEY_PREFIX_BYTES`, the `pool` field and `with_pool`, `integrity` for key extent
  records, the module comment on what a key column may hold.
- `leaf/layout.rs`: `heap_cost_of`, the threshold test, a `key_prefix_bytes(page_size)` helper.
- `leaf/encode.rs`: `threshold`, `encode_rows_with` writing the prefix, `Spill` doc.
- `leaf/read.rs`: `MiniColumn::key_prefix`, `MiniColumn::value`'s `Extent` arm for key columns,
  the `pool` field on `KeyView` or `LeafRef`.
- `leaf/compare.rs`: `compare_key_with`, a `compare_prefixed` helper carrying the table of
  decision 2.
- `leaf/delta.rs`: `compare_delta_key`, `delta_extent_at`, `delta_key` doc.
- `leaf/splice.rs`: no change; it already refuses a leaf with extents. Its comment says so.
- `datum.rs`: `tag::KEY_EXTENT`, `tagged_span`, `decode_tagged`, `encode_key_extent_tagged`.
- `mutate.rs`: `LeafMut::locate` attaches the pool; `offsets_for` is unchanged.
- `write.rs`: `out_of_line`, `encode_row_spilling_wide_values`, `row_larger_than_a_page` text,
  `split_carrying` pricing the right half and the separator rule, `insert_separator`,
  `merge_if_small` freeing a separator extent, `apply_row` and `delete_near` orphan lists,
  `leaf_for_hinted` and `note_leaf_hint`, `locate_and_read_previous`, `MAX_KEY_BYTES` and its
  refusal.
- `write/sorted.rs`: pool attached in `delete_sorted`, `update_sorted`, `repack_run`.
- `write/merge.rs`: the separator removal.
- `split_log.rs`: `read_interior` and `parent_image` carrying the slot flag.
- `paged.rs`: `encode_key` applying `MAX_KEY_BYTES`, `check`, `check_subtree`,
  `key_range_under`, a `write_separator_extent` beside `write_extent`.
- `paged/descent.rs`: the flag read and `child_for_resolving`; fences resolved at copy.
- `paged/cursor.rs`: `probe_leaf`, `hit_in`, `read_hit`, every `visit_*` attaching the pool.
- `paged/skip.rs`: nothing beyond the attached pool.
- `paged/bulk.rs`: `plan_leaves` separators and `build_interior_levels` pairs.
- `paged/occupancy.rs`: separator extents in `released` and `pages_occupied`.
- `keyenc.rs`, `key.rs`: no change. A new `shorten_separator(left, right) -> &[u8]` beside
  them, in `keyenc.rs`, with U5's cases.

Pool crate, `crates/inillucent-pool/src/`:

- `interior.rs`: the constants, the slot flag, `separator_slot`, `child_for_resolving`,
  `InteriorBuilder` over pairs, `validate`.
- `meta.rs`: `FORMAT_VERSION = 3`, `FORMAT_WITHOUT_KEY_EXTENTS`, `Meta.format`, `encode`,
  `decode`, `require_format`.
- `page.rs`: `INTERIOR_HAS_EXTENTS`.
- `extent.rs`: a doc paragraph saying a key extent and a separator extent are ordinary extents.

Log and recovery:

- `crates/inillucent-wal/src/record.rs`: `Body::Format`, kind 19, encode and decode, the
  comment in the pattern of kinds 15 to 18.
- `crates/inillucent-txn/src/redo.rs`: `Format` replay, `insert_row`'s key decode, pool attached
  in every locate, `decode_all` for `KEY_EXTENT`.
- `crates/inillucent-engine/src/recovery.rs`: `LearningRows` passes `Format` through.

Engine and executor:

- `crates/inillucent-engine/src/engine/batch.rs`: `free_dropped_value` for separator extents, the
  `require_format` call site when a transaction's first key extent is written in rollback
  journal mode.
- `crates/inillucent-engine/src/engine/pages.rs`: nothing, if `pages_occupied` lists separator
  extents. Its test E1 proves it.
- `crates/inillucent-engine/src/analyze.rs:277`, `crates/inillucent-exec/src/dml/target.rs:837`:
  the two comments that say a key column is never out of line, and whatever those lines do with
  that: `analyze` reads sample keys and must go through `read_extents`; `target.rs` must be read
  by the implementer and changed if it decodes a key from the page.
- `crates/inillucent-engine/src/ddl/index.rs` and `ddl/reindex.rs`: nothing beyond the bulk path.

Tests and ledgers:

- `tests/selection.toml`: rows for `differential::new_engine_long_keys`,
  `engine::long_keys_at_every_page_size`, `durability::long_key_crash`.
- `tests/escapes.toml`: the row.
- `tests/interop/<version>/build.sql`: the long indexed value.
- `crates/inillucent-compat/tests/nightly/release_format_history.rs`: F3.
- `crates/inillucent-compat/tests/e2e/release_format.rs`: F1.

Documentation, per AGENTS.md section 2 item 6, with no ticket number in any of them:

- `docs/relational-architecture.md` section 6: format 3, what it promises, when a file moves to
  it, the table of decision 10; the leaf and interior layout sections for the key extent record
  and the separator slot flag; the sentence at line 731 that a value too large for its leaf is
  stored on other pages gains "including a key".
- `docs/sql.md`: the "values larger than a page" cell gains "in a key or index as well", and a
  limits paragraph naming 1,048,576 bytes per key.
- `docs/feature-comparison.md`: the row for large indexed values, measured.
- `docs/roadmap.md`: the item, if listed, closed.
- `CHANGELOG.md`, `Unreleased`: the change and the format move, in the words of the 0.1.8 entry.
- `agent-skills/inillucent-troubleshoot/` and `inillucent-develop/`: the refusal text that no
  longer exists, then `node tools/sync-skills.mjs`.
- `src/data/documentation.ts` in `sites/inillucent` of `black-rainbow-labs-sites`: the format
  version chapter.
- `tasks/task-2217-bug-hunt-8.md` section 4: the row moves to section 3 when the fix ships.

## Build order

Each stage ends with `inillucent-testrun --changed origin/main --strict` at 0, `cargo fmt`, and
the stage's red tests green. The stages are separable commits on one branch.

1. **The leaf.** Decisions 1 to 4 and the unit tests U1 to U4. At the end of this stage the
   tree crate can hold, search and repack leaves with key extents, and every tree level call
   site attaches the pool. D1 passes for its single row, because one row never splits. Any test
   that splits a leaf of long keys still fails on the separator, so D4 to D8 wait for the next
   two stages.
2. **Separators.** Decision 5's shortening, decision 7's right half pricing, decision 6, U5's
   shortening cases, D1, D3, D6, D7, D8 at a 4,096 byte page. D4 and D5 are still red.
3. **Separators out of line.** The rest of decision 5, decision 9, U5's remaining cases, D4,
   D5, E1, E2, C1's first arm.
4. **The format.** Decisions 8, 10 and 11, U6, U7, D9, C1's second arm, F1 to F4, the escapes
   row, the gate measurement, the documentation.

Stage 1 and 2 could ship without 3 only with a refusal for the case 3 covers, and that refusal
would say "the index holds more than one leaf of keys sharing their first 4,096 bytes". The
design does not recommend shipping without stage 3: duplicates of a long value in an indexed
column are the common form of a long key, not the rare one.

## What this design does not do

- It does not store a key encoded. The leaf keeps comparing decoded values under collations, and
  the interior keeps comparing encoded bytes, as today. A design that stored encoded keys in
  leaves would make every leaf comparison a `memcmp` and is a different, larger change.
- It does not shorten every separator. Only one over 4,096 bytes is shortened, so ordinary trees
  are byte for byte what they are now. Shortening every separator would make interior pages
  smaller and is a measurable change to try on its own, after this one.
- It does not change the spill threshold, the prefix length by page size beyond the one formula,
  or the delta area.
