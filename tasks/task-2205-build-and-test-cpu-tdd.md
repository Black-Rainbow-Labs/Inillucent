# Build and test processor use, and a share of the machine

Technical design, task-2205. Measured on 2026-10-07 on the development machine: Intel Core Ultra 9
285, 24 logical processors (8 performance cores at logical 0, 1, 10 to 13, 22 and 23, and 16
efficiency cores), 128 GiB, rustc and cargo 1.95.0. Every number has its command, its log and its
processor samples under `_agent_output/task-2205-build-cpu/` in the main checkout: `measure.ps1`
runs one command and samples the machine once a second, `measurements.tsv` has one row per
measurement, and each measurement has a `.log` and a `.cpu.csv`. Two quiet windows were held for
the timings, 13:11Z to 14:10Z and 14:52Z to 15:21Z, with task-2200, task-2202 and task-2203
paused. One run, A1b, ran past the first window and is used only for what it showed about
`inillucent-bench`.

## 1. Introduction

The ticket: our build and tests take a long time and use every processor, which makes the machine
unusable while they run. Measure the build and the tests, find where they can be faster, write this
design, implement it and release. No build may take more than 80% of the processor.

The work found three things.

1. A change cadence run held the machine at a median of 100% for its whole length, and so did a
   release build. Nothing in the runner or the release scripts limited either.
2. The obvious limit, a Windows job object with a hard processor rate cap, does not work on this
   machine. `pwsh` here is the Microsoft Store package, and Windows runs a packaged program inside
   a job that lets every child leave every job. The children left the cap too. A processor
   affinity mask does work, because a child inherits the mask whatever job it leaves.
3. The suite is processor bound, and the debug build leaves its numeric and string work
   unoptimised. `opt-level = 1` for the dev profile saved 18% of the suite's processor time, which
   pays for most of what the 80% limit costs.

## 2. Goals

| # | goal | before | after |
|---|---|---|---|
| G1 | A test run uses at most 80% of the logical processors | median 100% of the machine, B1 | 19 of 24 processors, each of the other 5 carrying only the desktop, A1c |
| G2 | A release build uses at most 80% of the logical processors | median 93.7%, 85% of samples over 80%, B4 | 19 of 24 processors, A4b |
| G3 | The limit holds for every process the run starts, from any shell | not measured | a child that left every job still ran on the 19, `perf::processor_share` |
| G4 | The change cadence's test run is no slower than before the limit | 493.3 s, B1 | 502.9 s, F1, on 19 processors instead of 24 |
| G5 | A cargo command typed by hand can use the same limit | no way | `pwsh tools/capped.ps1 <command>` |

## 3. What was measured before any change

### 3.1 The change cadence, cold

B1 is `inillucent-testrun --cadence change --cpu 100` in an empty target directory. `--cpu 100` is
the behaviour every run had before this change: one test binary per processor, two test threads
each, and cargo's default of one compiler per processor.

| measure | B1 |
|---|---|
| total | 574.0 s |
| build: test targets, programs, the locate step | 54.1 s, 20.2 s, 2.6 s |
| run | 493.3 s for 253 targets and 4,138 tests |
| processor time of the targets, summed | 8,748 s |
| machine, median of samples | 100% |
| samples over 80% | 73% |

The run is processor bound. 8,748 s of target time on 24 processors is 364 s at best, and the run
took 493 s. The targets that set the wall time were the longest ones:

| target | seconds in B1 | tests |
|---|---:|---:|
| `inillucent-core::lib` | 485 | 319 |
| `inillucent-bench::inillucent-bench` | 449 | 182 |
| `inillucent-compat::differential::differential_part8` | 437 | 2 |
| `inillucent-compat::differential::usage_corpus` | 435 | 4 |
| `inillucent-compat::retrieval::bulk_embed` | 410 | 8 |
| `inillucent-compat::differential::cli` | 394 | 24 |
| `inillucent-compat::tooling::policy` | 375 | 24 |
| `inillucent-compat::retrieval::rag_verify` | 374 | 2 |

Each of these took far longer under the full run than alone: the target times include waiting
for a processor.

### 3.2 The release build

B4 is `release-all.ps1 -BuildOnly -NoProfile` in an empty target directory: five targets built at
once, fat LTO and one codegen unit each. 314.8 s, a median of 93.7%, 85% of samples over 80%.

### 3.3 The merge cadence

The merge cadence adds `durability` and `matrix_deep`. The ledger records 19,969 s of target time
for `matrix_deep` and 6,656 s for `durability`, against 5,988 s for the whole change cadence. B5 ran
the eight shards of `matrix_deep::expression` alone: about 113 s a shard at opt-level 0, with the
machine at a median of 41%, so `matrix_deep` waits on its fixture copies, its fsyncs and the pinned
`sqlite3` oracle as much as it computes. The ledger's 400 s a shard is the same work slowed by the
rest of a full run.

### 3.4 What one target costs alone

| measurement | result |
|---|---|
| C1 `inillucent-core::lib` alone, 2 test threads | 166.0 s |
| C1 the same, 8 test threads | 164.3 s |
| the four longest tests in it | HNSW recall tests: `a_parallel_batch_insert_is_as_accurate_as_the_sequential_one`, `extra_entry_points_do_not_lower_recall`, `filtered_recall_is_high_against_exhaustive_search_within_the_filter`, `index::tests::thirty_appends_stay_close_to_a_clean_rebuild` |
| `inillucent-bench` alone, one test at a time, summed | 285 s, of which 281 s is the five `models::arms` tests that load ONNX models |

More test threads do not shorten `inillucent-core::lib`, because one test is most of its time.

## 4. Why a job object does not work here

The first implementation put `inillucent-testrun` in a job object with
`JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP` at 80%. The runner
printed that the cap was held. A1, the same cold run as B1 under that cap, had a median of 96.8%
and 77% of its samples over 80%. A4, the release build under the same cap from `stage-layout.ps1`,
had a median of 95.6%.

`_agent_output/task-2205-build-cpu/job-probe.ps1`, `job-diag.ps1`, `job-diag2.ps1` and
`job-diag3.ps1` found why, one step at a time:

1. A `pwsh` started from the agent terminal is already in a job whose limit flags are `0x800`,
   `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`.
2. After that `pwsh` puts itself in a new capped job, a `cmd` it starts is in no job at all.
   `IsProcessInJob` succeeded and said so.
3. `cmd`, `ping` and every other program left the capped job. A second `pwsh` stayed in it.
4. `pwsh` here is `C:\Program Files\WindowsApps\Microsoft.PowerShell_7.6.6.0_x64__8wekyb3d8bbwe`,
   the Store package, and no other `pwsh` is installed. Windows runs a packaged program in a
   package job with silent breakaway, so a program outside the package that it starts leaves the
   job. The second `pwsh` stayed because it is part of the package.
5. A capped job made above the package job does not help. Windows PowerShell 5.1, which is not
   packaged, made a capped job, joined it, and started the Store `pwsh`. The `cmd` that `pwsh`
   started was again in no job.

So every cargo, rustc and test process started from a Store `pwsh`, directly or through the
runner, leaves any job a script makes. The runner itself is not packaged, so a job the runner makes
does hold its own children. A1b showed that, and showed the second problem with a rate cap:
`inillucent-bench` took 2,010 s in A1b against 449 s in B1, and spent its last minutes on one
thread. Its tests run ONNX Runtime, whose worker threads spin while they wait. Under a hard rate
cap a spinning thread spends the job's allowance while the thread it waits for is held back.

## 5. The design: an affinity mask

### 5.1 What it does

A process's processor affinity mask names the logical processors it may run on. Windows and Linux
copy it into every child at creation, and it is not a job, so a package job's breakaway does not
remove it. `_agent_output/task-2205-build-cpu/affinity-diag.ps1` set a mask on a Store `pwsh` and
found it on a `cmd` child, on a second `pwsh` started by path and by its alias, on the `cmd` that
`pwsh` started, and on the `cargo` and `rustc` processes of a build. Other tickets' cargo processes
on the machine at the same time had the full mask.

`crates/inillucent-compat/src/processor_share.rs` does this for the runner:

- `requested_percent` reads `--cpu`, else `INILLUCENT_CPU_PERCENT`, else 80.
- `choose` keeps that share of the logical processors, rounded down, fastest first: the highest
  efficiency class on Windows and the highest capacity on Linux, both read by
  `crates/inillucent-compat/src/affinity.rs`, then the lowest index. At 80% here that is mask
  `0xC1FFFF`: all 8 performance cores and 11 efficiency cores.
- `enter` sets the mask on the runner and sets `INILLUCENT_CPU_SHARE_ENTERED`, so a runner started
  inside the run keeps the mask it inherited.
- The runner sets `CARGO_BUILD_JOBS` to the number of processors kept, unless the caller set it,
  and runs that many test binaries at once instead of one per processor.
- The first line of every run names the share and the mask.

`Enter-ProcessorShare` in `packaging/stage-layout.ps1` does the same for `ship.ps1`,
`release-all.ps1` and `nightly.ps1`, reading the processor classes with
`GetSystemCpuSetInformation`. `tools/capped.ps1` runs any command under it, after importing the
MSVC environment that a raw cargo build of `onig_sys` needs.

CI sets `INILLUCENT_CPU_PERCENT=100` in both workflows, because a hosted runner has nobody at it.

### 5.2 Why a mask is better for the person at the machine

A rate cap of 80% lets the run use all 24 processors for most of each scheduling interval, so a key
press can still wait behind a compiler on every processor. A mask leaves 5 processors with nothing
of the run's on them at any time. Sampled per processor during A4b, the 19 processors in the mask
were at 100% and the 5 outside it averaged 58%. What ran on those 5 was the rest of the desktop,
about 2 processors' worth: Edge WebView, the Claude sessions, Node, Firefox and the Service
Manager.

### 5.3 What was tried and taken out

- **A job object with a hard rate cap.** Section 4.
- **Starting the program again outside the package job.** It fixed the runner, which is not
  packaged, and could not fix a `pwsh` script, whose children leave every job. The affinity mask
  made it unnecessary, so it was removed.
- **Below normal priority.** With other tickets' builds at normal priority holding every processor,
  a below normal helper that kept four threads busy for three seconds got 1.5 s of processor time
  in 148 s. Test binaries have their own time limits, so a starved run is a red run for no reason
  in the code.
- **More test threads for `inillucent-core::lib`.** C1: no change.

## 6. The design: opt-level 1 for the dev profile

`Cargo.toml` sets `opt-level = 1` in `[profile.dev]`, which `[profile.test]` inherits. The release
profile does not change. Debug assertions and overflow checks stay on: they are separate profile
settings.

| measurement | opt-level 0 | opt-level 1 |
|---|---:|---:|
| change cadence cold, uncapped, total (B1, B2) | 574.0 s | 536.4 s |
| build: test targets, programs | 54.1 s, 20.2 s | 95 s, 35.7 s |
| run | 493.3 s | 400.0 s |
| target processor time, summed | 8,748 s | 7,212 s |
| `inillucent-core::lib` in the run | 485 s | 281 s |
| `inillucent-compat::matrix::lists` | 176 s | 15 s |
| `inillucent-compat::tooling::matrix_inventory` | 157 s | 7 s |
| `inillucent-compat::tooling::policy` | 375 s | 243 s |
| `matrix_deep::expression`, eight shards alone (B5) | 164.1 s | 144.0 s |
| rebuild after an edit to `inillucent-sql/src/lib.rs` (I2) | 27.8 s | 20.8 s |

A cold build is about 57 s longer. The rebuild a ticket repeats after each edit is not longer. Every
run at opt-level 1 passed: B2, 253 targets, 4,138 tests.

## 7. Testing

- `processor_share`'s unit tests: the job count is the share rounded down and at least one; a
  percentage outside 1 to 100 is refused; at 80% the development machine's layout keeps every
  performance core and the eleven lowest numbered efficiency cores; a process already confined
  keeps only processors it had.
- `inillucent-compat::perf::processor_share`, a new target in the exclusive `perf` tier, so it runs
  alone at the end of a run:
  - a child of a program that entered a 25% share runs on exactly the processors the share chose,
    read from the child's own mask;
  - the same holds when the program first joins a job with `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`,
    the way a Store `pwsh` runs. This is the case the job object failed;
  - a child that keeps twice its processors' worth of threads busy uses no more than its
    processors, while the same child unconfined uses more than that. The unconfined control is
    what lets the case fail on a machine too busy to tell a limit from the load.
- `tooling::ci_workflows`, `selection`, `policy` and `documentation` cover the new row, the new
  unsafe file, the workflow variables and the tier table.

## 8. Rollout

1. `processor_share.rs`, the runner's `--cpu` flag, and the `perf` target.
2. `Enter-ProcessorShare`, called at the top of `ship.ps1`, `release-all.ps1` and `nightly.ps1`,
   and `tools/capped.ps1`.
3. `INILLUCENT_CPU_PERCENT: 100` in `tests.yml` and `nightly.yml`.
4. `opt-level = 1` in `[profile.dev]`.
5. `AGENTS.md`, the `inillucent-develop` skill, `tests/inillucent-testing-tdd.md`, `CHANGELOG.md`
   and `.claude/repo-plan.md`.

## 9. Measured after implementation

F1 is the B1 command with the final configuration: `opt-level = 1` from `Cargo.toml`, the 80% share
by default, an empty target directory. It ran from 16:00Z in a quiet window in which task-2200 did
not answer the pause, so one other ticket's work may have been on the machine. The machine sat at
12% before it started, the same as before every other measurement here.

| measure | B1, before | A1c, share only | F1, share and opt-level 1 |
|---|---:|---:|---:|
| total | 574.0 s | 687.2 s | 647.4 s |
| build: test targets, programs | 54.1 s, 20.2 s | 59.3 s, 21.0 s | 102 s, 36.3 s |
| run | 493.3 s | 600.4 s | 502.9 s |
| target processor time, summed | 8,748 s | 8,851 s | 7,827 s |
| processors the run may use | 24 | 19 | 19 |
| machine, median of samples | 100% | 87.0% | 89.1% |

The test run with the share and opt-level 1 took 502.9 s against 493.3 s before, with a fifth of
the machine left free. The cold build is longer; the rebuild after an edit, which is what a ticket
pays on every run after the first, is shorter (section 6). A whole cold run is 73 s longer than B1
and 40 s shorter than the share alone.

The machine median includes the rest of the desktop, about 2 processors' worth, which runs on the 5
processors outside the mask (section 5.2). The run itself uses at most 19 of 24 processors, 79.2%.

| target | B1 | F1 |
|---|---:|---:|
| `inillucent-core::lib` | 485 s | 389 s |
| `inillucent-bench::inillucent-bench` | 449 s | 497 s |
| `inillucent-compat::differential::differential_part8` | 437 s | 458 s |
| `inillucent-compat::differential::usage_corpus` | 435 s | 434 s |
| `inillucent-compat::retrieval::bulk_embed` | 410 s | 483 s |
| `inillucent-compat::tooling::policy` | 375 s | 221 s |

The five target release build under the share, A4b, took 332.6 s against 314.8 s in B4. Most of a
fat LTO build is one thread per target, so five fewer processors cost it 6%.

The full verification, `inillucent-testrun --changed --strict` with every change and merge tier
selected, ran 377 targets and 4,638 tests with none failed, in 1,166 s, under the share.

F1 had one failure: `supervise::tests::a_child_that_is_still_printing_is_never_killed_for_being_slow`
in `inillucent-compat::lib`, which fails when a child that prints every 50 ms is not scheduled for 2
s. Its own comment records the same failure on 2026-09-23 under four concurrent runs. It passed in
A1c, in the full verification run, and three times in a row alone afterwards. A share makes this
more likely when another program's load spreads over all 24 processors, because the run's 19 then
carry part of it as well.

## 10. Recommendations not built here

- **Give `inillucent-bench`'s ONNX tests a small thread count.** `EmbedderOptions::intra_threads`
  is `None` in the tests, so each session starts ONNX Runtime's default pool, one spinning thread
  per core, while the runner runs other binaries beside it. K1 ran the binary alone under the mask
  in 152 s. Two threads a session would stop the spinning from taking processors other targets are
  waiting for. It changes what the tests exercise, so it is a decision for the retrieval tests'
  owner.
- **Shrink the HNSW recall tests in `inillucent-core`.** Four tests are most of the crate's 166 s.
  A smaller corpus with the same recall threshold would test the same property faster, but the
  thresholds were chosen for these corpus sizes.
- **Drop ledger rows for targets that no longer exist.** `tests/timings.toml` still holds
  `nightly::matrix#k/8` rows beside the current `#k/20` ones. `write_ledger` keeps every earlier
  row. They do no harm to scheduling, but they make the ledger's totals overstate the suite.

## 11. Jargon

| term | meaning here |
|---|---|
| affinity mask | the set of logical processors a process may run on, one bit each |
| job object | a Windows object that groups processes and can limit them together |
| silent breakaway | a job flag that makes every child a process starts leave the job |
| package job | the job Windows runs a Microsoft Store program in |
| change cadence, merge cadence | the tiers a change runs, and the tiers that also run before a merge; see `AGENTS.md` |
