# task-2181: fewer system calls per statement for the cross process checks

## 1. What was asked

Under `locking_mode = normal` a statement outside a transaction takes the file lock, checks whether
another process folded or committed since this connection last held the lock, and releases the
lock. On Windows that was seven system calls:

| # | Call | What it is for |
|---|---|---|
| 1 | `LockFileEx` on the PENDING byte | stops a reader slipping in between a writer's two lock steps |
| 2 | `LockFileEx` on the shared range | the shared lock itself |
| 3 | `UnlockFileEx` on the PENDING byte | |
| 4 | `ReadFile`, 120 bytes of the primary meta slot | did another process fold? |
| 5 | `ReadFile`, 120 bytes of the shadow meta slot | did another process fold? |
| 6 | `GetFileSizeEx` on the open log segment | did another process commit? |
| 7 | `UnlockFileEx` on the shared range | release |

The ticket listed three options from the earlier investigation: a change counter in shared memory,
reading only the meta slot the next fold writes, and keeping SHARED between statements.

## 2. What changed

Call 4 is gone. The check reads the shadow slot alone.

A fold gives both meta slots the same record. Under the redo log it already wrote the shadow slot,
synced, then wrote the primary and synced again. Under a rollback journal it wrote the primary first
and the shadow second. The rollback journal branch now writes the shadow first as well. So in every
journal mode, a fold that has written any byte of the meta record has changed the shadow slot, and
the primary can only change after the shadow has. Reading the primary as well could not find a
change the shadow had not shown.

The order matters for a writer that is killed between the two slot writes. A machine crash is
handled by the journal or by the second sync either way. A killed process is different, because
both writes are already in the page cache and every other process sees them at once. With the
primary written first under a rollback journal, a reader that read only the shadow would have seen
nothing and gone on without looking for the journal the dead writer left. Writing the shadow first
closes that.

Files:

- `crates/inillucent-pool/src/pool/fold.rs`: `read_meta_records` is `read_shadow_record`, one read;
  the rollback journal branch of `checkpoint` writes `SHADOW_PAGE` before `META_PAGE`.
- `crates/inillucent-pool/src/file.rs`: `LastReadSlots` keeps the shadow's record bytes only.
- `crates/inillucent-pool/tests/journal_ordering.rs`: two new tests, below.
- `crates/inillucent/tests/budget.rs`: `meta_probes` now counts file reads, and the existing bound
  of one per statement is documented as catching a return to two reads.
- `docs/relational-architecture.md`: what the check reads and what it costs.

## 3. Result

Measured on 5 October 2026 under a quiet window for the processor. The only other agent was
rendering on gpu:0. Base is `046b535a`, candidate is this branch, both release builds, run in turn.

`inillucent-hcprobe <dir> --only select.point --autocommit --iterations 200000`, six runs of each:

| Build | Runs (ns per statement) | Median |
|---|---|---:|
| base | 4,736 4,766 4,809 4,922 4,967 5,234 | 4,866 |
| candidate | 3,867 3,914 3,945 3,989 3,995 4,072 | 3,967 |

An autocommit point select is 23% faster. The earlier run of the same probe gave base medians of
4,855 and candidate 4,035.

`inillucent-fullgate medium.db --plan hillclimb --api connection --rounds 10`, scored with
`tools/perf-hillclimb/score.mjs`:

| Run | Pairs | Train geomean change | Test geomean change | Verdict |
|---|---:|---|---|---|
| r1 | 2 | 1.059 (95% 1.047 to 1.068) | 1.054 (1.044 to 1.063) | KEEP |
| r2 | 3 | 1.075 (1.051 to 1.089) | 1.086 (1.070 to 1.095) | KEEP |

Workloads that were faster in both runs with their whole interval above 1.00 are the point reads
(`ai.point.rowid` 1.20 to 1.24, `ai.point.newest` 1.19 to 1.20, `ai.copy.point` 1.20 to 1.22,
`ai.point.name`, `churn.point.name`), `edge.empty` 1.19 to 1.20, `app.view.where`,
`edge.wide.row`, `app.exists`, `app.in.list`, `edge.or.two.indexes` and `app.join.prepare_each`.
These are the workloads whose statement is short enough that one `ReadFile` is a large part of it.
The write workloads (`ai.build`, `app.upsert.counter`, `churn.refill`) moved by up to 48% in r2
with wide intervals and were inside 1.00 in r1. Their time is dominated by `fsync`, which the
hill climb's A/A runs already showed moving by up to 35%, so they are not counted as a gain. No
workload in either run had its whole interval below 1.00. Peak resident memory was 56.8 MiB in
both arms and processor time was within the A/A band.

Raw samples, logs and scores are in `_agent_output/task-2181/runs/`.

## 4. Tests

- `every_fold_writes_the_shadow_slot_before_the_primary` reads the simulator's trace of one fold
  under each of the three protections (rollback journal, redo log, none) and asserts the shadow slot
  is written before the primary. With the old primary first order it fails under `journal`.
- `a_fold_that_reached_only_the_shadow_slot_is_seen` folds from a second handle, puts the primary
  slot back to its old bytes, and asserts the reader's next lock sees the fold and adopts the higher
  generation. A control step first asserts that the same reader reports no change when nothing was
  written. With the probe reading the primary instead of the shadow it fails.

Both were run against those two mutations and failed, then passed on the real code.

**The crash campaigns lost cut points, and the floors in `crash_reports.rs` were lowered.** A cut
point is armed at every file operation, reads included, so one read fewer per lock is one cut
point fewer per lock. The DELETE campaigns went from 80 to 76, the truncate and persist ones from
153 to 149, the checkpoint ones from 47 to 46 and 46 to 45, `wal-checkpoint.tsv` from 55 to 53 and
the WAL commit ones from 29 to 27. To show the read was the cause, the primary slot's read was put
back into `read_shadow_record` with its bytes discarded and the campaigns run again: every count
returned exactly to its old value. Every cut point still recovers to a state the campaign accepts.
`recovery-crash.txt` changed for the same reason: cut 32 lands one operation later, after the
commit, and reads `new` where it read `old`.

`inillucent-testrun --changed --strict` selected 339 targets (3,265 tests) and ran them in 1,307
seconds. The only failure was `crash_reports`, above, which was fixed and then passed with
`policy`, `documentation`, `selection` and `harness`. The process campaigns
(`process_journal_handoff`, `process_concurrency`, `process_storm`, `process_crash`,
`process_readers`, `process_campaign`, `process_interleavings`) and `durability::concurrency` all
passed. Four suites were not evidenced because this machine declares no Go, MySQL, PostgreSQL or
OpenSSL, and two had no prerequisite (`inillucent-remote::lib` needs network tests turned on,
`workload_freshness` needs the Nikaya checkout beside the worktree). `cargo clippy` with
`-D warnings` over the four touched crates was clean.

## 5. The options not taken, and why

**A change counter in shared memory.** It would replace the meta read and the log length query with
one load from a mapped file (calls 4, 5 and 6). An older inillucent writing the same file does not
bump the counter, so a reader trusting it would miss that writer's commits. The only gate that
excludes older builds is a new file format version, which makes every file this build checkpoints
unreadable by 2.1.4 and earlier. With this change the meta check is already one read, so the counter
would save two calls at the cost of a format bump and a new file beside every database, with its own
crash and permission handling. Not done. If a later release bumps the format for another reason,
the counter can ride on that bump.

**Dropping the meta read entirely and relying on the log length.** The meta read exists because a
fold can roll a new log segment, and a reader's handle on the old segment then reports an unchanged
length while commits go to the new one. A segment is not always deleted by the fold that rolls past
it: `retire_segments_below` keeps a segment that holds a record recovery may still need, and leaves
one it cannot unlink. So "the old segment was deleted" does not answer the question either. Appending a marker to the old segment before a roll would answer it, but an older
build rolling the log does not append one, so it needs the same format gate.

**Keeping SHARED between statements.** It removes calls 1 to 5 and 7 on every statement after the
first. A writer that needs EXCLUSIVE in another process, and every fold, would wait until the reader
lets go, and a reader that goes idle never lets go unless a timer thread releases it. A writer's
close waits its busy timeout for a fold and leaves the log unfolded. The engine has no background
thread today, and "an idle connection holds no lock" is what SQLite's normal mode promises. Not
done.

**Taking the shared range without the PENDING byte.** Rejected in the ticket: a reader can take
the range between a writer's two steps and starve it.

So the floor without a format change is six calls: three to take the lock, the shadow read, the
log length and the release. SQLite's Windows VFS makes the same three calls to take a shared lock.
