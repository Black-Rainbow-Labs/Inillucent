# task-2225: give back interior pages when a table is trimmed from the front

A design document. Nothing in this document is implemented. The Opus session finishing task-2217's
open items builds from it.

## 1. The problem

A table that inserts at the high end of the key and deletes from the low end (a queue, a log kept
to its newest rows, a session table) empties its leftmost leaves one at a time. Since task-2217 an
emptied leaf merges into its right sibling under the same parent: the left page keeps its number
and takes the sibling's rows, the sibling's page is freed, and the parent loses one separator and
one child. That is `PagedTree::merge_if_small` in `crates/inillucent-tree/src/write.rs`, with the
packing in `crates/inillucent-tree/src/write/merge.rs`.

The merge stops one short. `merge_if_small` refuses when the parent holds fewer than two
separators and is not the root:

```rust
if separators.len() < 2 && !path.len().eq(&1) {
    return Ok(());
}
```

So under every interior page the last two leaves never merge, and when the deletes move on, that
interior page and its one remaining leaf stay in the tree holding nothing useful. At the default
32,768 byte page an interior page routes to hundreds of leaves and the whole table of the test fits
under one of them, so the file stays flat. At 4,096 byte pages an interior page holds at most 169
children of a rowid tree (the directory is 48 bytes plus 24 bytes a separator), the 2,000 rows a
round of `space_reuse.rs` inserts span more than one parent, and every round leaves about 6 pages
behind: an interior page and its last leaf, for each parent the round filled.

`crates/inillucent-compat/tests/engine/space_reuse.rs` bounds the growth at small pages at
`first + first / 2` instead of requiring a flat file, and its doc comment names this design as the
reason.

## 2. Terms

| Term | Meaning in this document |
|---|---|
| leaf | a page holding rows, at level 0 |
| interior page | a page holding separators and child pointers, at level 1 or above |
| parent | the interior page directly above a page |
| grandparent | the interior page directly above a parent |
| neighbour | the child directly before or after a page in its parent's child list |
| root | the page the catalog names for the tree; its number never changes |
| dissolve | the operation this document adds: an interior page with one child gives that child to a neighbour and is freed |
| path | the interior pages a descent passed, root first, as `leaf_for` returns them |
| `C`, `P`, `G`, `Q`, `O` | the one child, its parent, the grandparent, the neighbour after `P`, the neighbour before `P` |

## 3. The design in one paragraph

An interior page below the root that is left with one child is dissolved: the child is handed to
the neighbour after it under the same grandparent, or to the neighbour before it when the one after
has no room, the grandparent drops the separator and the pointer that named the dissolved page, and
the dissolved page is freed. The child keeps its page number. The root is never dissolved and never
shrinks, so a tree's height only grows. Each rewritten interior page is logged whole as a
`WritePage` record and the freed page as a `FreePage` record, which the replay already applies by
the page LSN rule, so no log record kind is added and a build older than this change can still read
the log. The change is confined to the tree crate and its tests, plus the engine test that measures
the file.

## 4. The rules

### 4.1 Height only grows

The root keeps its level. When a merge or a dissolve leaves the root with one child, the root stays
an interior page with no separator and one child. That shape is already legal and already occurs:
`merge_if_small` allows a merge under the root that leaves it with no separator, and
`InteriorBuilder::build` and `InteriorRef::child_for` handle a page with one child and no
separator (`crates/inillucent-pool/src/interior.rs`, the test
`a_single_child_page_sends_everything_to_it`).

Collapsing the root into its child was considered and rejected:

- The root page number is fixed by the catalog ("The root page id never changes, by design", in
  `PagedTree::attach`). Collapsing would copy the child's image into the root page and free the
  child, which changes the page number of a live page. For a leaf child that breaks the leaf's
  place in the sibling chain, `PagedTree::shape`'s leftmost leaf, every `LeafHint`, `last_leaf`,
  the `near` page `delete_near` carries between deletes, and the leaf a cursor is standing on.
- Every `PagedTree` handle caches the height in `shape`, and another process's handle keeps its
  copy across the reload `reattach.rs` does. A height that only grows means a cached height is
  never above the real one, and the descent reads page kinds rather than counting levels, so a
  cached height that is too small costs one `Vec` reallocation and nothing else. A height that
  can shrink would make a cached height wrong in the direction the code does not tolerate.
- It saves one page per level, which is a constant, where the dissolve saves pages per round.

So after this change a tree with few rows may be a root with one child above a parent with many
leaves. That is at most `height` pages above the leaves, which is not a leak.

### 4.2 An interior page below the root with one child is dissolved

`P` is an interior page at level `l`, not the root, with no separator and one child `C`. `G` is
its parent, at level `l + 1`, and `P` is child `i` of `G`.

1. If `G` has a child after `P`, call it `Q`, with separator `s` between `P` and `Q` in `G`. The
   new `Q` is the separators `[s] ++ Q.separators` over the children `[C] ++ Q.children`. If
   `InteriorBuilder::fits` says those separators fit, this is the dissolve.
2. Otherwise, if `G` has a child before `P`, call it `O`, with separator `s'` between `O` and
   `P` in `G`. The new `O` is `O.separators ++ [s']` over `O.children ++ [C]`. If that fits,
   this is the dissolve.
3. Otherwise nothing happens now. `P` stays with its one child, which is a legal shape, and the
   dissolve is tried again by the rule in 4.3.

Pulling `s` or `s'` down out of `G` is what keeps the separators strictly between the children:
every key under `C` is at or above the separator before `P` in `G` and below the separator after
`P`, which are exactly the bounds `s'` and `s` give it in its new parent. `PagedTree::check_subtree`
asserts this bound on every child and will fail on any other choice of separator.

The new `G` is `G` with separator `s` (case 1) or `s'` (case 2) removed and the pointer to `P`
removed. If `G` is left with no separator and one child, `G` is itself a one child interior page.
When `G` is not the root, the same step runs on `G`, up to the root. When `G` is the root, it
stays as it is (4.1).

The case where `P` is the only child of `G` has no neighbour, so step 3 applies: the one child
spine `root -> G -> P -> C` stays. It is at most `height` pages and it only arises when the
whole tree has emptied down to one leaf; a refill splits it back out.

`Q` and `O` are read with `split_log::read_interior` and rebuilt with `InteriorBuilder::build`
over `Swip::unswizzled` pointers, the way `insert_separator` and `merge_if_small` rebuild a parent.
Their level must equal `P`'s level; a mismatch is `corrupt`, never a guess.

Why hand one child to a neighbour rather than merging two interior pages whole: `P` has exactly
one child when this runs, so a merge of `P` into `Q` is the same bytes as handing `C` to `Q`. The
general interior merge (two part full interior pages into one) is not needed for the target and
is left out. Why hand `C` across rather than merging the leaf `C` into the leaf after it under
`Q`: `C` is the leaf the last merge kept and holds that merge's rows, so it is not empty, and a
leaf merge across a parent boundary would also have to rewrite the sibling link of the leaf before
`C`, a fourth page under a third parent.

### 4.3 When the dissolve runs

Two triggers, both inside `merge_if_small`, both only on a delete that left a leaf underflowed, so
a delete that empties nothing pays nothing new.

1. **After a leaf merge.** The refusal quoted in section 1 is removed. The leaf merge runs and
   rebuilds the parent. If the rebuilt parent has no separator and the path has a grandparent
   (`path.len() >= 2`), the dissolve runs on the parent with `path[..path.len() - 1]` as its own
   path. Each level up uses the path one shorter. The loop ends at the root or at the first level
   that keeps at least two children.
2. **On a later delete under a one child parent.** When `adjacent_under_one_parent` answers
   `None` because the leaf's right sibling is under a different parent, and the parent on the
   path has no separator, and there is a grandparent, the dissolve is tried. This is what resolves
   step 3 of 4.2 once a neighbour has room, and it costs one `read_interior` of the parent on a
   delete that was already going to read it.

The second trigger can repeat on every delete of a leaf whose parent cannot be dissolved because
both neighbours are full. That is three page reads and no packing, which is cheaper than the merge
attempt the same delete already made. If it shows in a profile, record the parent in
`merge_backoff` the way a failed leaf merge records its leaf, keyed by the parent's page number.
Do not build that until a measurement asks for it.

### 4.4 The writes, in order

For case 1 of 4.2 (case 2 is the same with `O` for `Q`):

1. Build the new `Q`, log `Body::WritePage { page: Q, image }`, stamp the image with the LSN,
   `database.install(Q, image)`.
2. Build the new `G`, log `Body::WritePage { page: G, image }`, stamp, `database.install(G, image)`.
3. Log `Body::FreePage { page: P }`, then `database.release(P, 1)`.
4. `self.last_leaf.set(None)` is not needed, because no leaf changed, but `note_leaves` is not
   called either: the leaf count did not change and no leaf fence moved. The leaf hints stay valid
   (see 5.1).
5. Count it: add `parents_dissolved: u64` to `WriteStats` in `write.rs`, bumped here, and printed
   where `merges` is printed in `inillucent-writeprofile` and `inillucent-writelogattrib`. The unit
   tests in section 7 assert on it.

The order matters for a stale reader inside this process and not for recovery. After step 1 `C`
is reachable through the old `G` by way of `P` and through the new `Q`; after step 2 it is reachable
through `Q` only; `P` is freed last, as the leaf merge frees its sibling after the installs, so a
failure between the steps leaks a page rather than freeing one that is still pointed at. Recovery
does not see a partial dissolve: the log is redo only, a page dirtied by an uncommitted transaction
is never written to the file (`crates/inillucent-txn/src/undo.rs`, "no-steal"), and a transaction
that rolls back writes a commit record after its undo, so either all three records are replayed or
none is.

`P`'s bytes are left as they are. `Pool::install` is not called on `P`. Until the free map hands
`P` out again, `P` is an interior page of this tree whose one child is `C`, so a stale path that
still goes through `P` reaches the right leaf. The leaf merge writes an empty leaf image into the
page it frees because a stale `near` would otherwise find a leaf holding a copy of the key; an
interior page has no such copy, and `is_own_leaf` refuses an interior page anyway.

### 4.5 The log records, and why no kind is added

A dissolve writes two `WritePage` records and one `FreePage` record, inside the transaction that
ran the delete. Nothing else.

- `WritePage` is how `insert_separator` already logs a rebuilt parent, a split interior page and
  a moved root. `Applier::redo_image` and `Applier::redo_record` in
  `crates/inillucent-txn/src/redo.rs` both apply it with `put_image`, under the page LSN rule:
  a page whose stamp is below the record's LSN takes the image, a page already past it is skipped.
  Each image stands alone and needs no other page, which is the property `split_log.rs` explains
  for a split: an eviction writes one dirty page at a time, so after a crash the file can hold the
  new `G` and the old `Q`, and the replay repairs `Q` from its own record without reading `G`.
- `FreePage` goes to `Applier::free_map_changes`, which the engine applies in log order after the
  replay (`crates/inillucent-txn/src/engine.rs`, around line 431). A page freed by a dissolve and
  allocated again later in the same segment comes out allocated, for the reason that function's
  comment gives.
- `Body::Structural` carries three leaf and parent images for a leaf split or merge because the
  logical pass must not derive a leaf from rows another page no longer holds. An interior page has
  no rows and is always logged whole, so a structural kind for a dissolve would carry the same
  bytes under a new kind byte, and a build older than this change would refuse the log with
  `a log record has kind N, which this format does not define`. Two `WritePage` records are read
  by every build since the record format was fixed.

Rollback: the engine's `WalLog` appends every record the tree logs, and `merge_if_small` already
frees its sibling with an immediate `FreePage` and `database.release`. A rollback restores rows,
not structure: the undo puts the deleted rows back through `put`, which lands them in whatever
leaves exist then, and the merged leaf and the dissolved parent stay merged and dissolved. That is
the contract `undo.rs` states ("equal rows, not equal bytes"). The dissolve follows the leaf
merge's convention exactly and must not use `pending_frees`, which is for a `DROP` whose free is
undone by a rollback.

## 5. Readers and cached paths

### 5.1 In this process

One writer holds the file's exclusive lock for the statement, and a dissolve runs inside that
statement, so nothing in this process reads the tree while the three installs happen. What a
later read in the same connection may hold:

| Cached thing | Where | After a dissolve |
|---|---|---|
| height and leftmost leaf | `PagedTree::shape` | unchanged: the root keeps its level, `C` keeps its number, and the leftmost leaf is a leaf |
| the leaf a delete reuses | `delete_near`'s `near`, `PagedTree::last_leaf` | points at a leaf; `C` keeps its number; a freed page is `P`, which `is_own_leaf` refuses because its kind is interior |
| leaf hints | `PagedTree::leaf_hints` | cleared by the leaf merge's `note_leaves` moments earlier; the dissolve moves no fence, so a hint recorded after it stays right |
| largest key | `PagedTree::largest_hint` | checked against the rightmost leaf's page number and LSN, neither of which a dissolve changes |
| failed merge record | `merge_backoff` | keyed by leaf; unchanged |
| a reverse walk's path | `Descent::steps` in `visit_reverse` | a reverse walk never runs across a write in the same connection; a path through `P` would still reach `C` (4.4) |
| swizzled child pointers | the pool's frames | `Pool::install` of an interior image calls `forget_children_of`, so a child evicted later does not write its unswizzled pointer into a slot the new image moved. The new `G` and `Q` hold unswizzled pointers and are swizzled again by the next descent |

### 5.2 In another process

A reader in another process waits out the writer's lock, then `Database::begin_read` finds the log
moved, drops its cached pages and replays the log into its pool. The two `WritePage` images and the
`FreePage` arrive that way, the same as a split's. Its tree handles keep their cached `shape`, which
4.1 keeps valid, and `reattach.rs` rebuilds the handles from the catalog when the catalog moved.
There is no shared memory between processes to invalidate.

## 6. Files the change touches

| File | Change |
|---|---|
| `crates/inillucent-tree/src/write.rs` | remove the two separator refusal in `merge_if_small`; after the parent is installed, call the dissolve when the new parent has no separator and a grandparent exists; in the `adjacent_under_one_parent == None` branch, try the dissolve when the parent has no separator (4.3 trigger 2); add `parents_dissolved` to `WriteStats` and its `Add`; declare `mod dissolve;` beside `mod merge;`. `merge_if_small` is recorded at 157 lines in `crates/inillucent-compat/tests/tooling/policy.rs` and may not grow, so the new calls go into a helper and the function must end shorter than it is |
| `crates/inillucent-tree/src/write/dissolve.rs` | new. `impl PagedTree { fn dissolve_one_child_parents(&mut self, database, log, path: &[PageId]) }` that walks up the path while the lowest page has one child, and `fn dissolve_into_neighbour(...)` doing 4.2 and 4.4 for one level. Both under 150 lines, which `LONGEST_NEW_FUNCTION` enforces. A module doc comment stating the invariant: every row reachable before a dissolve is reachable after it through the same leaf, and `check_subtree`'s separator bound holds at every level |
| `crates/inillucent-compat/src/bin/writeprofile.rs`, `writelogattrib.rs` | print `parents_dissolved` beside `merges` |
| `crates/inillucent-compat/tests/engine/space_reuse.rs` | the small page test asks for a flat file over 40 rounds (section 7.1) and its doc comment says so |
| `crates/inillucent-compat/tests/durability/new_engine_recovery_shapes.rs` | one new crash test (section 7.4) |
| `tests/selection.toml` | no change: the unit tests live in the `inillucent-tree` lib target, which has a row, and the two suite files already have rows |
| `docs/relational-architecture.md` | the delete paragraph (item 5, near line 451) gains one sentence: an interior page left with one child hands it to a neighbour and is freed, and the root never shrinks |
| `CHANGELOG.md` | one line under Unreleased, in the bug hunt entry: at small pages a table trimmed from the front no longer keeps an interior page and a leaf per parent it emptied |
| `tasks/task-2217-bug-hunt-8.md` | section 4: move the first row to the fixed list, or say it is fixed by this change |
| `src/data/documentation.ts` in `sites/inillucent` of `black-rainbow-labs-sites` | only if the published chapter repeats the limitation; grep it for "trimmed" before editing |

No change to `crates/inillucent-wal`, `crates/inillucent-txn`, `crates/inillucent-pool` or the
engine. If the implementer finds one is needed, the design above is wrong somewhere and this
document should be corrected first.

## 7. Tests

### 7.1 The test that is red today and green after

`a_table_trimmed_from_the_front_reuses_its_pages_at_small_pages` in `space_reuse.rs` changes to
40 rounds and the default page's bound:

```rust
let (first, last) = queue_rounds("small-pages", 4_096, 40);
assert!(last <= first + 4, "...");
```

Today it grows by about 6 pages a round, so 40 rounds put `last` about 240 pages over `first`
and the assertion fails. After the change the file is flat. The slack of 4 pages is the same the
default page test allows. Keep the integrity check, the reopen and the `freelist_count` assertion
the helper already makes.

### 7.2 Unit tests in `crates/inillucent-tree/src/write/dissolve.rs`

Build a rowid tree the way `paged.rs`'s test module does (`build(rows, page_size)` over
`MemoryVfs`, `PagedTree::bulk_build`) at a 512 byte page so the tree has two interior levels with
a few rows per leaf, and use `split_log::read_interior` to read the root's children.

1. `deleting_everything_under_the_first_parent_frees_it`: delete every row under the root's
   first child in key order through `delete_near`. Assert the root has one child fewer than it
   had, `tree.write_stats().parents_dissolved == 1`, every remaining key is found by `point`, a
   forward `rows` and a reverse walk agree with a `BTreeMap`, `tree.check(pool)` passes, and the
   freed page comes back from `database.allocate(1)`. Red today: the root keeps its child count.
2. `a_dissolve_hands_the_child_to_the_neighbour_before_it_when_the_one_after_is_full`: fill the
   root's third child to the page's capacity (`InteriorBuilder::capacity` over its separators)
   with inserts in its key range, then empty the second child down to one leaf. Assert the first
   child gained one child and the root lost one. Red today for the same reason as test 1.
3. `a_one_child_parent_with_no_room_beside_it_waits`: both neighbours full, empty the middle
   parent to one leaf. Assert the root's child count did not change, `check` passes, and
   `parents_dissolved == 0`. Then delete rows under the neighbour after it until a leaf merge
   makes room, delete one more row under the one child parent, and assert the dissolve happened
   (trigger 2 of 4.3). Green today for the first half and red for the second.
4. `dissolving_up_to_the_root_leaves_the_root_with_one_child`: a tree whose root has two
   children, each with two leaves; delete everything under the first child. Assert the root has
   one child and no separator, the tree's height did not change (`tree.height(pool)`), and a
   round of inserts splits it back out and `check` passes.

### 7.3 The campaign

`crates/inillucent-tree/tests/write_campaign.rs` compares the tree with a `BTreeMap` after every
operation under a pool small enough to evict, and runs the integrity checker after every campaign.
It must keep passing unchanged. Add one campaign shape to it if it has none: insert ascending,
delete the lowest half, repeat, at a 512 byte page, so a dissolve happens under eviction.

### 7.4 Recovery

In `new_engine_recovery_shapes.rs`, a test `a_trim_that_dissolved_a_parent_replays`: create at
`PAGE_SIZE` 4,096 through `ImportedDatabase::create_on` over the `SimVfs`, insert 2,000 rows,
checkpoint, then run the trim round of `space_reuse.rs` twice inside one transaction and commit,
forget the handle, crash the VFS, reopen. Assert the row count, `PRAGMA integrity_check` answers
`ok`, and `PRAGMA freelist_count` is at least the number of pages the second round freed. The
replay applies the `WritePage` records in `redo_image`, and `report.dropped == 0`.

### 7.5 Tests that must keep passing

Run `target/debug/inillucent-testrun --changed` from the worktree, which selects these by the
dependency closure of `inillucent-tree`:

- `inillucent-tree` lib tests and `write_campaign`, `fuzz_seeded`, `value_round_trip`;
- `inillucent-pool` lib tests, for `interior.rs`;
- `engine::space_reuse`, `engine::new_engine_page_ownership`, `differential::new_engine_extents`
  (`deleting_every_packed_value_returns_the_pages`), `engine::write_speed`;
- `durability::storage_failure` ("Deleting rows: merges, freed overflow chains, and a growing
  freelist"), `durability::new_engine_recovery_shapes`, `durability::wal_crash`;
- the `tooling` suites, in particular `policy::` for the function and module ceilings and
  `documentation::` for the pages in section 6.

Then the merge cadence, `--changed --cadence merge`, because `inillucent-tree` is in the `covers`
of the durability rows. The perf tier's `churn.delete.half` gate must not move: a dissolve runs
once per parent emptied, which is once per hundred leaf merges, and the second trigger reads
three pages without packing.

## 8. Risks

- **A parent image that does not fit.** Case 1 and case 2 of 4.2 add one separator to a
  neighbour; `InteriorBuilder::fits` is asked before anything is logged, as `merge_if_small` asks
  about the parent before packing. A dissolve must never allocate.
- **A level mismatch.** `Q` and `O` are read from `G` and must be at `P`'s level. Check it and
  return `corrupt` with both levels in the message.
- **The retry trigger looping.** Trigger 2 runs once per delete and never recurses into itself.
- **The path.** `merge_if_small` takes the path `leaf_for` built for this delete, or the one
  `delete_near` built on demand after an underflow. Both are fresh. Trigger 1 passes
  `path[..path.len() - 1]` upward; it never reuses a path across two deletes.
- **`PRAGMA page_count` after the change.** The freed pages go to the free map and the file does
  not shrink; `freelist_count` grows and `page_count` stays. The test measures `page_count` across
  rounds, which is right: reuse keeps it flat.

## 9. Recommendation

Build 4.2 to 4.4 as written, in a new `write/dissolve.rs`, with no change to the log format or the
transaction crate. Make the `space_reuse.rs` small page test the red test first, then the four unit
tests, then the recovery test. Leave the root alone.
