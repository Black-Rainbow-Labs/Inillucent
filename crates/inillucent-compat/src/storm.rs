//! Several worker and reader processes on one file, killed at random, and the
//! file graded afterwards.
//!
//! Invariant: **a storm passes only when the file holds every transaction any
//! worker acknowledged, whole, no reader ever saw a broken snapshot, and the
//! run did real work.** The last part matters as much as the first two: a run
//! in which every process was refused, or every reader died at open, has
//! nothing in it to grade and must not pass. Task-2173's first storm found an
//! open that failed with `busy` beside any writer, and it was visible only
//! because a run with no reader checks was counted as a failure.
//!
//! ## How a run goes
//!
//! `inillucent-chaos setup` builds the file. The parent starts the workers and
//! readers, each with its standard output piped to a thread that forwards
//! every line. On a schedule drawn from the seed it kills a live process and
//! starts a new one with a new name in its place, so a killed worker's
//! transactions stay attributed to the run that acknowledged them. When the
//! time is up it stops killing, kills the readers, lets the workers finish
//! their last transactions, and grades the file with [`crate::chaos::verify`]
//! in this process, which wrote none of it.
//!
//! ## What a kill is
//!
//! `Child::kill`: `TerminateProcess` on Windows and `SIGKILL` on Unix. Neither
//! runs a destructor or flushes a buffer, so a worker killed between its
//! commit and its acknowledgement leaves a transaction the parent never heard
//! about. [`crate::chaos::verify`] allows exactly one such transaction per
//! killed worker and no more.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::chaos::{verify, Incarnation, Mix, Settings};

/// One storm's shape.
#[derive(Clone, Debug)]
pub struct Storm {
    /// The case's name, which is also its directory's.
    pub name: String,
    /// How many workers run at once.
    pub workers: usize,
    /// How many readers run at once.
    pub readers: usize,
    /// How long the parent keeps killing and restarting.
    pub duration: Duration,
    /// Whether the parent kills anything.
    pub kills: bool,
    /// The seed the kill schedule, and every worker's seed, come from.
    pub seed: u64,
    /// How many transactions one worker commits before it stops on its own.
    pub transactions: u64,
    /// How the processes open the file and what the workload includes.
    pub settings: Settings,
    /// The least number of acknowledged transactions a passing run must have.
    pub least_acknowledged: i64,
    /// The least number of reader checks a passing run must have.
    pub least_checks: u64,
}

/// What a storm did.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Every worker that ran.
    pub incarnations: Vec<Incarnation>,
    /// How many transactions were acknowledged in all.
    pub acknowledged: i64,
    /// How many snapshots the readers checked in all.
    pub checks: u64,
    /// How many processes the parent killed.
    pub kills: usize,
    /// How many times a worker or reader said it had been refused as busy.
    pub busy_lines: usize,
    /// What the checker said about the file.
    pub verdict: String,
}

/// A process the parent started.
struct Running {
    /// The process.
    child: Child,
    /// Its index in the run's record, for the lines it sends.
    slot: usize,
}

/// What a process was, as far as the parent knows.
#[derive(Clone, Debug)]
struct Record {
    /// The worker's record, or `None` for a reader.
    worker: Option<Incarnation>,
    /// The process's name, for messages.
    name: String,
    /// How many checks a reader reported.
    checks: u64,
    /// Whether the parent killed it.
    killed: bool,
    /// The last line it printed, for messages.
    last: String,
}

/// Returns the arguments every process of the run takes for its settings.
///
/// @param settings - the run's settings
fn options(settings: &Settings) -> Vec<String> {
    let mut out = Vec::new();
    if settings.frames > 0 {
        out.extend(["--frames".to_string(), settings.frames.to_string()]);
    }
    if let Some(key) = &settings.key {
        out.extend(["--key".to_string(), key.clone()]);
    }
    if settings.search {
        out.push("--search".to_string());
    }
    if settings.bulk_parts > 0 {
        out.extend(["--bulk-parts".to_string(), settings.bulk_parts.to_string()]);
    }
    out.extend(["--busy-ms".to_string(), settings.busy_ms.max(1).to_string()]);
    out
}

/// The state of a run while it is going.
struct Run<'a> {
    /// The storm.
    storm: &'a Storm,
    /// `inillucent-chaos`.
    binary: &'a Path,
    /// The database file.
    database: PathBuf,
    /// Every process started, in order.
    records: Vec<Record>,
    /// The processes still running.
    running: Vec<Running>,
    /// Where the forwarding threads send lines.
    tell: Sender<(usize, String)>,
    /// Where the parent reads them.
    heard: Receiver<(usize, String)>,
    /// The kill schedule's generator.
    mix: Mix,
    /// How many processes were killed.
    kills: usize,
    /// How many busy lines were heard.
    busy_lines: usize,
    /// Lines that mean a process saw something wrong.
    complaints: Vec<String>,
}

impl<'a> Run<'a> {
    /// Starts a worker with the next name and a seed drawn from the run's.
    fn start_worker(&mut self) -> Result<(), String> {
        let number = self.records.len() + 1;
        let name = format!("w{number}");
        let seed = self
            .storm
            .seed
            .wrapping_mul(1_000_003)
            .wrapping_add(number as u64);
        let mut arguments = vec![
            "worker".to_string(),
            self.database.to_string_lossy().to_string(),
            "--name".to_string(),
            name.clone(),
            "--id".to_string(),
            number.to_string(),
            "--seed".to_string(),
            seed.to_string(),
            "--txns".to_string(),
            self.storm.transactions.to_string(),
        ];
        arguments.extend(options(&self.storm.settings));
        self.records.push(Record {
            worker: Some(Incarnation {
                name: name.clone(),
                seed,
                acked: 0,
                finished: false,
            }),
            name,
            checks: 0,
            killed: false,
            last: String::new(),
        });
        self.spawn(arguments)
    }

    /// Starts a reader; one in three holds each snapshot open for 150 ms.
    fn start_reader(&mut self) -> Result<(), String> {
        let number = self.records.len() + 1;
        let hold = if self.mix.one_in(3) { "150" } else { "0" };
        let mut arguments = vec![
            "reader".to_string(),
            self.database.to_string_lossy().to_string(),
            "--checks".to_string(),
            "1000000".to_string(),
            "--hold-ms".to_string(),
            hold.to_string(),
        ];
        arguments.extend(options(&self.storm.settings));
        self.records.push(Record {
            worker: None,
            name: format!("r{number}"),
            checks: 0,
            killed: false,
            last: String::new(),
        });
        self.spawn(arguments)
    }

    /// Spawns the process for the record just pushed, with a thread forwarding
    /// its output.
    ///
    /// @param arguments - the command line
    fn spawn(&mut self, arguments: Vec<String>) -> Result<(), String> {
        let slot = self.records.len().saturating_sub(1);
        let mut child = Command::new(self.binary)
            .args(&arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("{} did not start: {error}", self.binary.display()))?;
        if let Some(out) = child.stdout.take() {
            let tell = self.tell.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if tell.send((slot, line)).is_err() {
                        break;
                    }
                }
            });
        }
        self.running.push(Running { child, slot });
        Ok(())
    }

    /// Takes every line the processes have printed so far into the record.
    fn drain(&mut self) {
        while let Ok((slot, line)) = self.heard.try_recv() {
            self.hear(slot, line);
        }
    }

    /// Records one line.
    ///
    /// @param slot - which process printed it
    /// @param line - what it printed
    fn hear(&mut self, slot: usize, line: String) {
        let Some(record) = self.records.get_mut(slot) else {
            return;
        };
        if let Some(seq) = line.strip_prefix("ACK ") {
            if let (Some(worker), Ok(seq)) = (record.worker.as_mut(), seq.trim().parse::<i64>()) {
                worker.acked = worker.acked.max(seq);
            }
        } else if line.starts_with("DONE") {
            if let Some(worker) = record.worker.as_mut() {
                worker.finished = true;
            }
        } else if line.starts_with("CHECKED") {
            record.checks += 1;
        } else if line.starts_with("BUSY") {
            self.busy_lines += 1;
        } else if line.starts_with("VIOLATION") || line.starts_with("ERROR") {
            self.complaints.push(format!("{}: {line}", record.name));
        }
        record.last = line;
    }

    /// Kills one running process, drawn from the schedule, and starts another
    /// of the same kind in its place.
    ///
    /// **A reader is a candidate only once it has checked a snapshot.** A reader
    /// killed while it holds a snapshot is part of what a storm tests, and so is
    /// one that answers; under the full test tier a debug build decrypting the
    /// log could be killed before its first answer every time, and the run then
    /// had nothing from its readers to grade.
    fn kill_one(&mut self) -> Result<(), String> {
        let candidates: Vec<usize> = (0..self.running.len())
            .filter(|index| {
                self.running.get(*index).is_some_and(|running| {
                    self.records
                        .get(running.slot)
                        .is_some_and(|record| record.worker.is_some() || record.checks > 0)
                })
            })
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        let pick = self.mix.range(0, candidates.len() as u64) as usize;
        let Some(index) = candidates.get(pick).copied() else {
            return Ok(());
        };
        let mut victim = self.running.swap_remove(index);
        let _ = victim.child.kill();
        let _ = victim.child.wait();
        self.kills += 1;
        let was_worker = match self.records.get_mut(victim.slot) {
            Some(record) => {
                record.killed = true;
                record.worker.is_some()
            }
            None => false,
        };
        match was_worker {
            true => self.start_worker(),
            false => self.start_reader(),
        }
    }

    /// Reaps processes that ended on their own, and starts a worker for each
    /// worker that finished so the number running stays the same.
    fn reap(&mut self) -> Result<(), String> {
        let mut index = 0;
        while index < self.running.len() {
            let ended = match self.running.get_mut(index) {
                Some(running) => matches!(running.child.try_wait(), Ok(Some(_))),
                None => false,
            };
            if !ended {
                index += 1;
                continue;
            }
            let gone = self.running.swap_remove(index);
            let worker = self
                .records
                .get(gone.slot)
                .is_some_and(|record| record.worker.is_some());
            if worker {
                self.start_worker()?;
            }
        }
        Ok(())
    }
}

/// Runs a storm and grades the file, returning what happened or why it failed.
///
/// @param storm - the shape of the run
/// @param binary - the built `inillucent-chaos`
/// @param directory - an empty directory of the run's own
pub fn run(storm: &Storm, binary: &Path, directory: &Path) -> Result<Report, String> {
    let database = directory.join("storm.rdb");
    let mut setup = vec!["setup".to_string(), database.to_string_lossy().to_string()];
    setup.extend(options(&storm.settings));
    let built = Command::new(binary)
        .args(&setup)
        .output()
        .map_err(|error| format!("setup did not start: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "setup failed: {}",
            String::from_utf8_lossy(&built.stdout)
        ));
    }
    let (tell, heard) = channel();
    let mut run = Run {
        storm,
        binary,
        database: database.clone(),
        records: Vec::new(),
        running: Vec::new(),
        tell,
        heard,
        mix: Mix::new(storm.seed),
        kills: 0,
        busy_lines: 0,
        complaints: Vec::new(),
    };
    let outcome = drive(&mut run);
    // Every process goes, whatever happened above, so a failing case does not
    // leave processes holding the file.
    for running in run.running.iter_mut() {
        let _ = running.child.kill();
        let _ = running.child.wait();
    }
    outcome?;
    finish(run, &database)
}

/// The timed part of a run: start, kill on the schedule, then let the workers
/// finish.
///
/// @param run - the run's state
fn drive(run: &mut Run<'_>) -> Result<(), String> {
    for _ in 0..run.storm.workers {
        run.start_worker()?;
    }
    for _ in 0..run.storm.readers {
        run.start_reader()?;
    }
    let started = Instant::now();
    let mut next_kill = started + Duration::from_millis(50 + run.mix.range(0, 400));
    while started.elapsed() < run.storm.duration {
        std::thread::sleep(Duration::from_millis(10));
        run.drain();
        run.reap()?;
        if run.storm.kills && Instant::now() >= next_kill {
            run.kill_one()?;
            next_kill = Instant::now() + Duration::from_millis(50 + run.mix.range(0, 400));
        }
    }
    stop_readers(run);
    let deadline = Instant::now() + Duration::from_secs(600);
    while !run.running.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        run.drain();
        run.running
            .retain_mut(|running| !matches!(running.child.try_wait(), Ok(Some(_))));
    }
    if !run.running.is_empty() {
        return Err(format!(
            "{} workers had not finished ten minutes after the storm ended",
            run.running.len()
        ));
    }
    // The forwarding threads end when their pipes close; give them a moment
    // to hand over the last lines.
    std::thread::sleep(Duration::from_millis(200));
    run.drain();
    Ok(())
}

/// Kills every reader still running, since a reader never stops on its own.
///
/// @param run - the run's state
fn stop_readers(run: &mut Run<'_>) {
    let records = &run.records;
    let (readers, workers): (Vec<Running>, Vec<Running>) =
        run.running.drain(..).partition(|running| {
            records
                .get(running.slot)
                .is_some_and(|record| record.worker.is_none())
        });
    for mut reader in readers {
        let _ = reader.child.kill();
        let _ = reader.child.wait();
    }
    run.running = workers;
}

/// Grades a finished run.
///
/// @param run - the run's state, with every process ended
/// @param database - the file
fn finish(run: Run<'_>, database: &Path) -> Result<Report, String> {
    let incarnations: Vec<Incarnation> = run
        .records
        .iter()
        .filter_map(|record| record.worker.clone())
        .collect();
    let acknowledged: i64 = incarnations.iter().map(|worker| worker.acked).sum();
    let checks: u64 = run.records.iter().map(|record| record.checks).sum();
    let mut report = Report {
        incarnations,
        acknowledged,
        checks,
        kills: run.kills,
        busy_lines: run.busy_lines,
        verdict: String::new(),
    };
    let unkilled_failures: Vec<String> = run
        .records
        .iter()
        .filter(|record| {
            record
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.finished)
                && !record.killed
        })
        .map(|record| format!("{} stopped early: {}", record.name, record.last))
        .collect();
    let mut problems = run.complaints.clone();
    problems.extend(unkilled_failures);
    if !problems.is_empty() {
        return Err(format!(
            "seed {}: processes reported problems:\n{}",
            run.storm.seed,
            problems.join("\n")
        ));
    }
    if report.acknowledged < run.storm.least_acknowledged {
        return Err(format!(
            "seed {}: only {} transactions were acknowledged, fewer than the {} a run must reach",
            run.storm.seed, report.acknowledged, run.storm.least_acknowledged
        ));
    }
    if report.checks < run.storm.least_checks {
        return Err(format!(
            "seed {}: the readers checked {} snapshots, fewer than the {} a run must reach; \
             the last lines were {:?}",
            run.storm.seed,
            report.checks,
            run.storm.least_checks,
            run.records
                .iter()
                .filter(|record| record.worker.is_none())
                .map(|record| (record.name.clone(), record.last.clone()))
                .collect::<Vec<_>>()
        ));
    }
    report.verdict = verify(database, &run.storm.settings, &report.incarnations)
        .map_err(|why| format!("seed {}: {why}", run.storm.seed))?;
    Ok(report)
}
