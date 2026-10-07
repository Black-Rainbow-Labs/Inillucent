//! How much of the machine a test run and its builds may take.
//!
//! Invariant: **when [`enter`] returns [`Entered::Held`], this process and
//! every process it starts afterwards may run only on the processors it
//! names, which are the asked share of the machine's logical processors,
//! rounded down.** The limit is the processor affinity mask, which the
//! operating system copies into every child at creation, so no child,
//! grandchild or job breakaway gets past it.
//!
//! ## Why this exists (task-2205)
//!
//! The development machine is an Intel Core Ultra 9 285 with 24 logical
//! processors, and it is also the machine a person works at. `inillucent-testrun`
//! started one test binary per processor, two threads each, after a cargo build
//! that ran one compiler per processor, so a run held the machine at 100% and
//! the editor, the browser and the terminal stopped answering while it ran.
//! The ticket asked for no build to take more than 80%.
//! `tasks/task-2205-build-and-test-cpu-tdd.md` has the measurements.
//!
//! ## Why an affinity mask and not a job object
//!
//! The first version put the runner in a Windows job object with a hard
//! processor rate limit. It printed that the cap was held, and a full run still
//! kept the machine at a median of 97%. On this machine `pwsh` is the Microsoft
//! Store package, Windows runs a packaged program in a job whose flags include
//! `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`, and a program the package starts,
//! such as `cmd` or `cargo`, leaves every job in the chain: the capped one,
//! and a capped job made above the package's job by an unpackaged parent too.
//! Both were measured. An affinity mask is not a job: `cmd`, a second `pwsh`
//! and the `cargo` and `rustc` processes of a build started from a Store `pwsh`
//! all carried the mask it set. It also works on Linux, where there is no job
//! object.
//!
//! A mask has a second advantage for the person at the machine. A rate limit
//! lets the run use every processor for most of each interval, so a key press
//! can still wait behind a compiler. A mask leaves the processors outside it
//! idle all the time.
//!
//! ## Which processors are kept
//!
//! The fastest first: the highest efficiency class on Windows, the highest
//! capacity or frequency on Linux (see [`crate::affinity`]), then the lowest
//! index. On the development machine that is all 8 performance cores and 11 of
//! the 16 efficiency cores, so the run loses 5 of its slowest processors rather
//! than 5 of its fastest.
//!
//! ## Why the priority is left alone
//!
//! Below normal priority was tried first and taken out. The machine usually
//! has other tickets' builds on it at normal priority, and with those holding
//! every processor a below normal process got almost nothing: a helper that
//! kept four threads busy for three seconds was given 1.5 seconds of processor
//! time in 148 seconds of wall time. The test binaries have their own time
//! limits, so a starved run is a slow run and can be a red one for no reason in
//! the code.
//!
//! ## Elsewhere
//!
//! macOS cannot confine a process to processors, and there the lowered counts
//! are the limit. A hosted CI runner is a machine nobody is sitting at, so CI
//! sets `INILLUCENT_CPU_PERCENT=100` and runs uncapped.

use crate::affinity::{self, Processor};

/// The share a run takes when nothing says otherwise, in percent.
pub const DEFAULT_PERCENT: u32 = 80;

/// The environment variable that changes [`DEFAULT_PERCENT`] for every run and
/// script on a machine, without a flag on each command.
pub const PERCENT_VARIABLE: &str = "INILLUCENT_CPU_PERCENT";

/// Set in the environment of everything started under a held share, so a
/// program started inside one (a nested runner, a test that runs the runner)
/// keeps the mask it inherited instead of choosing again.
pub const ENTERED_VARIABLE: &str = "INILLUCENT_CPU_SHARE_ENTERED";

/// The share asked for: `--cpu` when given, else [`PERCENT_VARIABLE`], else
/// [`DEFAULT_PERCENT`].
///
/// @param flag - the value of `--cpu`, when the command line gave one
pub fn requested_percent(flag: Option<&str>) -> Result<u32, String> {
    let from_environment = std::env::var(PERCENT_VARIABLE).ok();
    let (text, source) = match (flag, from_environment.as_deref()) {
        (Some(text), _) => (text, "`--cpu`"),
        (None, Some(text)) => (text, PERCENT_VARIABLE),
        (None, None) => return Ok(DEFAULT_PERCENT),
    };
    parse_percent(text)
        .ok_or_else(|| format!("{source} wants a percentage from 1 to 100, not `{text}`"))
}

/// Reads a percentage from 1 to 100, with or without a trailing `%`.
///
/// @param text - the value to read
pub fn parse_percent(text: &str) -> Option<u32> {
    let number: u32 = text.trim().trim_end_matches('%').parse().ok()?;
    (1..=100).contains(&number).then_some(number)
}

/// How many logical processors the machine has: the count the platform
/// reports with ranks, else the standard library's count.
pub fn processors() -> usize {
    let reported = affinity::machine_processors().len();
    if reported > 0 {
        return reported;
    }
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
}

/// The number of processes or compilers that fits in a share of the machine:
/// the share of the processors, rounded down, and never less than one.
///
/// @param processors - the machine's logical processors
/// @param percent - the share, from 1 to 100
pub fn jobs_for(processors: usize, percent: u32) -> usize {
    let percent = usize::try_from(percent.min(100)).unwrap_or(100);
    (processors.saturating_mul(percent) / 100).max(1)
}

/// Picks `count` of the processors this process may already use, fastest
/// first, then lowest index. Returns them in index order.
///
/// @param machine - every processor with its rank
/// @param allowed - the processors this process may run on now
/// @param count - how many to keep
pub fn choose(machine: &[Processor], allowed: &[usize], count: usize) -> Vec<usize> {
    let mut candidates: Vec<&Processor> = machine
        .iter()
        .filter(|each| allowed.contains(&each.index))
        .collect();
    candidates.sort_by(|a, b| b.rank.cmp(&a.rank).then(a.index.cmp(&b.index)));
    let mut chosen: Vec<usize> = candidates
        .into_iter()
        .take(count)
        .map(|each| each.index)
        .collect();
    chosen.sort_unstable();
    chosen
}

/// What [`enter`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entered {
    /// Nothing was asked for: a share of 100.
    Uncapped,
    /// A process that started this one already chose the processors, and this
    /// process kept the mask it inherited.
    Inherited(Vec<usize>),
    /// This process and its children may now run only on these processors.
    Held(Vec<usize>),
    /// The processors could not be limited here, for this reason; the job
    /// counts are the only limit.
    NotHeld(String),
}

impl Entered {
    /// One line for the run's output, so a slow run says why it might be slow.
    ///
    /// @param percent - the share asked for
    /// @param processors - the machine's logical processors
    pub fn describe(&self, percent: u32, processors: usize) -> String {
        let how = match self {
            Entered::Uncapped => {
                return format!("processor share: all {processors} processors, uncapped")
            }
            Entered::Inherited(kept) => format!(
                "{} processors {}, chosen by the process that started this one",
                kept.len(),
                affinity::mask_of(kept)
            ),
            Entered::Held(kept) => {
                format!("{} processors {}", kept.len(), affinity::mask_of(kept))
            }
            Entered::NotHeld(reason) => {
                format!("not held: {reason}; the job counts are the only limit")
            }
        };
        format!(
            "processor share: {percent}% of {processors} processors ({how}); \
             --cpu 100 or {PERCENT_VARIABLE}=100 removes it"
        )
    }

    /// How many processors the run may use, for its job counts.
    ///
    /// @param percent - the share asked for
    /// @param processors - the machine's logical processors
    pub fn processors_for_jobs(&self, percent: u32, processors: usize) -> usize {
        match self {
            Entered::Inherited(kept) | Entered::Held(kept) if !kept.is_empty() => kept.len(),
            _ => jobs_for(processors, percent),
        }
    }
}

/// Confines this process, and everything it starts from now on, to `percent`
/// of the machine's logical processors.
///
/// A share of 100 changes nothing. A refusal from the operating system is
/// reported in the result rather than as an error, because a run that cannot
/// be limited should still run.
///
/// @param percent - the share, from 1 to 100
pub fn enter(percent: u32) -> Entered {
    if percent >= 100 {
        return Entered::Uncapped;
    }
    let allowed = affinity::current_processors();
    if std::env::var_os(ENTERED_VARIABLE).is_some() {
        return Entered::Inherited(allowed);
    }
    let machine = affinity::machine_processors();
    if machine.is_empty() || allowed.is_empty() {
        return Entered::NotHeld("this platform cannot confine a process to processors".into());
    }
    let chosen = choose(&machine, &allowed, jobs_for(machine.len(), percent));
    if chosen.len() < allowed.len() {
        if let Err(reason) = affinity::pin_processors(&chosen) {
            return Entered::NotHeld(reason);
        }
    }
    std::env::set_var(ENTERED_VARIABLE, "1");
    Entered::Held(affinity::current_processors())
}

/// Puts this process in a job that lets every child leave it, the way the
/// Store `pwsh`'s package job does, so a test can show that [`enter`] holds
/// children that leave every job. Returns whether it worked.
pub fn join_a_job_children_leave() -> bool {
    platform::join_silent_breakaway_job()
}

#[cfg(windows)]
mod platform {
    /// Puts this process in a new job with `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`.
    /// The handle is left open on purpose: the job has to last as long as the
    /// process.
    pub fn join_silent_breakaway_job() -> bool {
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        // SAFETY: null attributes and a null name make an unnamed job with the
        // default security, which is the documented use.
        let job = unsafe { CreateJobObjectW(core::ptr::null(), core::ptr::null()) };
        if job.is_null() {
            return false;
        }
        // SAFETY: an all zero structure is a valid value of this C structure.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { core::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        // SAFETY: `info` is the structure the class names and the length is its
        // size; the pseudo handle for this process is always valid.
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                core::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
                && AssignProcessToJobObject(job, GetCurrentProcess()) != 0
        }
    }
}

#[cfg(not(windows))]
mod platform {
    /// There are no job objects here.
    pub fn join_silent_breakaway_job() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The development machine's layout: performance cores, efficiency class
    /// 1, at 0, 1, 10 to 13, 22 and 23; the rest efficiency class 0.
    fn development_machine() -> Vec<Processor> {
        let fast = [0, 1, 10, 11, 12, 13, 22, 23];
        (0..24)
            .map(|index| Processor {
                index,
                rank: u64::from(fast.contains(&index)),
            })
            .collect()
    }

    /// The share is rounded down and never reaches zero.
    #[test]
    fn the_job_count_is_the_share_rounded_down_and_at_least_one() {
        assert_eq!(jobs_for(24, 80), 19);
        assert_eq!(jobs_for(24, 100), 24);
        assert_eq!(jobs_for(2, 80), 1);
        assert_eq!(jobs_for(1, 10), 1);
        assert_eq!(jobs_for(4, 50), 2);
    }

    /// A percentage is 1 to 100, and the `%` sign is allowed.
    #[test]
    fn a_percentage_outside_one_to_a_hundred_is_refused() {
        assert_eq!(parse_percent("80"), Some(80));
        assert_eq!(parse_percent("80%"), Some(80));
        assert_eq!(parse_percent(" 100 "), Some(100));
        assert_eq!(parse_percent("0"), None);
        assert_eq!(parse_percent("101"), None);
        assert_eq!(parse_percent("most"), None);
    }

    /// At 80% the development machine keeps every performance core and the
    /// eleven lowest numbered efficiency cores.
    #[test]
    fn the_fastest_processors_are_kept_first() {
        let all: Vec<usize> = (0..24).collect();
        let kept = choose(&development_machine(), &all, 19);
        assert_eq!(kept.len(), 19);
        for fast in [0, 1, 10, 11, 12, 13, 22, 23] {
            assert!(kept.contains(&fast), "performance core {fast} was dropped");
        }
        assert_eq!(
            kept,
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 22, 23]
        );
    }

    /// A process already confined to fewer processors keeps only those.
    #[test]
    fn only_processors_already_allowed_are_chosen() {
        let kept = choose(&development_machine(), &[2, 3, 4], 19);
        assert_eq!(kept, vec![2, 3, 4]);
    }
}
