//! A program that entered a processor share holds every process it starts to
//! that share, including when it began inside a job that lets children leave.
//!
//! Invariant: **a child started after `processor_share::enter` runs on exactly
//! the processors `enter` chose, and a child that keeps more threads busy than
//! that uses no more processor time than those processors can give, while the
//! same child unconfined uses more.** The unconfined run is what makes the
//! measured case able to fail: without it a busy machine and a working limit
//! look the same.
//!
//! ## The case that was missed (task-2205)
//!
//! The first version of the share was a Windows job object with a processor
//! rate limit. It read back that the job was set, printed that the cap was
//! held, and a full run still kept the machine at a median of 97%: the Store
//! `pwsh` runs in a package job with `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`,
//! and every program it starts left the capped job. A test of the capped
//! process's own threads passed against that version. These tests look at a
//! **child**, and one of them puts the helper in a job like the package's
//! first.

use std::process::Command;
use std::time::{Duration, Instant};

use inillucent_compat::affinity::{current_processors, mask_of};
use inillucent_compat::processor_share::{
    enter, jobs_for, join_a_job_children_leave, processors, Entered, ENTERED_VARIABLE,
};

/// The environment variable that tells the helper which part to play.
const ROLE: &str = "INILLUCENT_PROCESSOR_SHARE_ROLE";

/// The exact name of the helper, for the `--exact` filter.
const HELPER: &str = "processor_share::the_helper";

/// The share the confined cases ask for, in percent: a quarter, so 6 of the
/// development machine's 24 processors.
const SHARE: u32 = 25;

/// Plays the part `ROLE` names.
///
/// - `spin`: prints the processors it may run on, then keeps twice the share's
///   processor count of threads busy for two seconds and prints how many
///   processors' worth of time it used.
/// - `share`: enters [`SHARE`], prints the processors it chose, and runs a
///   `spin` child, printing what it printed.
/// - `leaky`: joins a job that lets children leave, then does what `share`
///   does.
///
/// With no role it plays no part, and nothing is being skipped.
#[test]
fn the_helper() {
    let role = std::env::var(ROLE).unwrap_or_default();
    match role.as_str() {
        "spin" => {
            println!("child mask {}", mask_of(&current_processors()));
            println!("processors used {:.3}", spin(spinners()));
        }
        "share" => share_and_spin(),
        "leaky" => {
            println!(
                "joined a job children leave: {}",
                join_a_job_children_leave()
            );
            share_and_spin();
        }
        _ => println!("no role asked for"),
    }
}

/// Threads a spinner keeps busy: twice the share's processor count, so a
/// confined spinner wants twice what it may have.
fn spinners() -> usize {
    jobs_for(processors(), SHARE) * 2
}

/// Enters [`SHARE`], prints the chosen processors, and runs a `spin` child.
fn share_and_spin() {
    match enter(SHARE) {
        Entered::Held(kept) => println!("chose {}", mask_of(&kept)),
        other => println!("did not hold: {other:?}"),
    }
    print!("{}", run_helper("spin"));
}

/// Keeps `threads` threads busy for two seconds of wall time and returns the
/// processor time used divided by the wall time.
///
/// @param threads - how many threads to keep busy
fn spin(threads: usize) -> f64 {
    let before = inillucent_compat::procstat::ProcessCost::now();
    let started = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            std::thread::spawn(move || {
                let mut value = 0u64;
                while started.elapsed() < Duration::from_secs(2) {
                    value = std::hint::black_box(value.wrapping_mul(31).wrapping_add(7));
                }
                value
            })
        })
        .collect();
    for handle in handles {
        let _ = handle.join();
    }
    let wall = started.elapsed().as_secs_f64();
    let used = inillucent_compat::procstat::ProcessCost::now()
        .since(&before)
        .cpu_nanos() as f64
        / 1e9;
    used / wall
}

/// Runs this test binary as the helper in `role` and returns its standard
/// output and error. The runner's own share is taken out of the helper's
/// environment, so the helper chooses its processors itself.
///
/// @param role - the part to play
fn run_helper(role: &str) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .env(ROLE, role)
        .env_remove(ENTERED_VARIABLE)
        .args([HELPER, "--exact", "--test-threads", "1", "--nocapture"])
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

/// Reads the text after `label` on the first line that holds it. Under
/// `--nocapture` libtest writes `test <name> ... ` first, so the label is found
/// anywhere in its line.
///
/// @param printed - what the helper printed
/// @param label - the words before the value
fn value_after<'text>(printed: &'text str, label: &str) -> &'text str {
    printed
        .lines()
        .find_map(|line| line.split(label).nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("the helper printed no `{label}`:\n{printed}"))
}

/// Checks a `share` or `leaky` run: the child ran on exactly the chosen
/// processors, and there were the share's number of them.
///
/// @param printed - what the helper printed
fn assert_the_child_was_confined(printed: &str) {
    let chose = value_after(printed, "chose ");
    let child = value_after(printed, "child mask ");
    assert_eq!(
        chose, child,
        "the child ran on {child}, not on the {chose} the share chose:\n{printed}"
    );
    let count = u64::from_str_radix(chose.trim_start_matches("0x"), 16)
        .unwrap()
        .count_ones() as usize;
    assert_eq!(
        count,
        jobs_for(processors(), SHARE),
        "the share chose {count} processors"
    );
}

/// A child of a program that entered the share runs on the processors the
/// share chose, and no others.
#[test]
fn a_child_runs_only_on_the_chosen_processors() {
    let printed = run_helper("share");
    if cfg!(any(windows, target_os = "linux")) {
        assert_the_child_was_confined(&printed);
    } else {
        // macOS has no call that confines a process to processors, and the
        // share has to say so rather than claim a limit.
        assert!(
            printed.contains("did not hold"),
            "a platform with no affinity call claimed a share:\n{printed}"
        );
    }
}

/// Started inside a job that lets children leave, as a Store `pwsh` is, a
/// program that enters the share still confines its children.
#[cfg(windows)]
#[test]
fn a_child_that_leaves_every_job_is_still_confined() {
    let printed = run_helper("leaky");
    assert!(
        printed.contains("joined a job children leave: true"),
        "the helper could not reproduce the package job:\n{printed}"
    );
    assert_the_child_was_confined(&printed);
}

/// A confined child that wants twice its processors uses no more than its
/// processors, while the same child unconfined uses more than that.
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_confined_child_uses_no_more_than_its_processors() {
    let kept = jobs_for(processors(), SHARE) as f64;
    let control: f64 = value_after(&run_helper("spin"), "processors used ")
        .parse()
        .unwrap();
    assert!(
        control > kept * 1.1,
        "an unconfined child used {control:.2} processors, not clearly more than the {kept} a \
         confined one may use, so this run cannot tell a limit from a busy machine"
    );
    let printed = run_helper("share");
    let used: f64 = value_after(&printed, "processors used ").parse().unwrap();
    assert!(
        used <= kept * 1.02 + 0.05,
        "confined to {kept} processors, the child used {used:.2}:\n{printed}"
    );
}
