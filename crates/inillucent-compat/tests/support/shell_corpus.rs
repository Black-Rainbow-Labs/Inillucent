//! Running a corpus of SQL scripts through `sqlite3` and `inillucent-shell`
//! and comparing what they print, byte for byte.
//!
//! Invariant: **a case runs alone, in a fresh directory, against a fresh
//! database, under `.mode quote` and `.headers on`, through both shells, and
//! only the two pointer lines under an error are set aside before the outputs
//! are compared.** Shared by `differential::usage_corpus`, which runs the
//! application usage corpus on every change, and `nightly::research_corpus`,
//! which runs the cases gathered from other engines' bug reports every night.
//! The two used to be one file; the second corpus is too large for a change
//! run, so the runner moved here and each suite names its own directory.
//!
//! Not a suite of its own: it holds no test, and each suite includes it with
//! `#[path]`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use inillucent_compat::workspace_root;

/// What every case is prefixed with, so types and column names are compared.
const PREAMBLE: &str = ".mode quote\n.headers on\n";

/// How long one shell may take over one case before it is stopped.
const CASE_LIMIT: Duration = Duration::from_secs(120);

/// One named script from the corpus.
#[derive(Clone, Debug)]
pub(crate) struct Case {
    /// The name after `-- case:`, unique across the corpus.
    pub(crate) name: String,
    /// The file the case came from, relative to the corpus directory.
    pub(crate) file: String,
    /// The SQL and dot commands, without the preamble.
    pub(crate) script: String,
}

/// One case whose two outputs were not the same.
#[derive(Clone, Debug)]
pub(crate) struct Divergence {
    /// The case.
    pub(crate) case: Case,
    /// What `sqlite3` printed.
    pub(crate) sqlite: String,
    /// What `inillucent-shell` printed.
    pub(crate) inillucent: String,
}

/// One row of `known.toml`.
#[derive(Clone, Debug)]
pub(crate) struct Known {
    /// The case name.
    pub(crate) case: String,
    /// `defect` for something to fix, `deviation` for a deliberate difference.
    pub(crate) kind: String,
}

/// Returns the pinned SQLite shell, if it has been downloaded.
pub(crate) fn reference_shell() -> Option<PathBuf> {
    let directory = workspace_root().join(".sqlite-ref/3.53.4/shell");
    let path = directory.join(format!("sqlite3{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Splits one corpus file into its cases.
///
/// A case starts at a line `-- case: <name>` and runs to the next one. Text
/// before the first marker is a file comment and belongs to no case. Text
/// after a `|` on the marker line is a note, such as the bug report a case
/// was taken from, and is not part of the name.
///
/// @param file - the file's name, recorded on each case
/// @param text - the file's contents
pub(crate) fn parse_cases(file: &str, text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("-- case:") {
            cases.push(Case {
                name: name.split('|').next().unwrap_or(name).trim().to_string(),
                file: file.to_string(),
                script: String::new(),
            });
            continue;
        }
        if let Some(case) = cases.last_mut() {
            case.script.push_str(line);
            case.script.push('\n');
        }
    }
    cases
}

/// Reads every `.sql` file of a corpus directory, in name order.
///
/// @param directory - the corpus directory
pub(crate) fn load_corpus(directory: &Path) -> Vec<Case> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("the usage corpus directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .collect();
    files.sort();
    let mut cases = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("a corpus file reads as UTF-8");
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        cases.extend(parse_cases(&name, &text));
    }
    cases
}

/// Reads a corpus directory's `known.toml`.
///
/// @param directory - the corpus directory
pub(crate) fn load_known(directory: &Path) -> Vec<Known> {
    let path = directory.join("known.toml");
    let text = std::fs::read_to_string(&path).expect("known.toml reads");
    let document = inillucent_compat::toml_lite::parse(&text).expect("known.toml parses");
    document
        .array("known")
        .iter()
        .map(|row| {
            let field = |key: &str| {
                row.get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or_else(|| panic!("a known.toml row has no `{key}`"))
                    .to_string()
            };
            Known {
                case: field("case"),
                kind: field("kind"),
            }
        })
        .collect()
}

/// Returns a short directory name for a case: the first 32 characters of its
/// name, then a hash of the whole name so two cases never share one.
///
/// **Short on purpose.** A case named in full put the pinned `sqlite3`'s
/// database path past 260 characters on Windows, where it fails with "unable
/// to open database file" and the case reads as a difference in the engine.
///
/// @param name - the case name
fn directory_for(name: &str) -> String {
    let prefix: String = name
        .chars()
        .take(32)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    // FNV-1a, which needs no dependency and spreads short names well.
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{prefix}_{hash:016x}")
}

/// Runs one shell over a fresh database in a fresh directory and returns its
/// standard output followed by its standard error, with line endings made
/// the same on every platform.
///
/// Each case has a directory of its own, because a script may `ATTACH` or
/// `VACUUM INTO` a relative file name, and two cases running at once must not
/// share it.
///
/// @param program - the shell
/// @param directory - the case's directory, emptied first
/// @param script - the case's script
fn run_shell(program: &Path, directory: &Path, script: &str) -> String {
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory).expect("a case directory can be made");
    let mut child = Command::new(program)
        .arg(directory.join("case.db"))
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shell starts");
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(format!("{PREAMBLE}{script}").as_bytes());
    }
    let output = wait_with_limit(child);
    let mut text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    text.push_str(&String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"));
    without_carets(&text)
}

/// Removes the two lines a shell prints under an error to point at where in
/// the statement it went wrong.
///
/// **Only the pointer is set aside.** Where a shell points depends on the
/// byte offset the engine reports for each kind of error, which is a separate
/// piece of parity with its own cases in `cli.rs`. Comparing it here made a
/// case that returned every right row and the right message fail on the
/// pointer under it, and hid the differences this suite exists to find. The
/// error line, with its message, is still compared.
///
/// @param text - a shell's whole output
pub(crate) fn without_carets(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    while let Some(line) = lines.get(index) {
        kept.push(line);
        index += 1;
        let is_error = line.contains("rror near line ");
        let pointer = lines.get(index + 1).is_some_and(|next| {
            next.ends_with("^--- error here") || next.trim_start().starts_with("error here ---^")
        });
        if is_error && pointer {
            index += 2;
        }
    }
    kept.join("\n")
}

/// Reads a pipe to its end on a thread of its own.
///
/// Both pipes are drained while the shell runs, because a shell that fills
/// one pipe's buffer stops until somebody reads it.
///
/// @param pipe - the child's standard output or standard error
fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Waits for a shell, stopping it once it has run for [`CASE_LIMIT`].
///
/// A case that hangs is reported as a difference with the word `TIMEOUT` in
/// its output, rather than holding the suite until the runner's budget stops
/// the whole target and names nothing.
///
/// @param child - the running shell, with its standard input already closed
fn wait_with_limit(mut child: std::process::Child) -> std::process::Output {
    let started = Instant::now();
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let mut timed_out = false;
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > CASE_LIMIT {
            let _ = child.kill();
            timed_out = true;
            break child.wait().expect("a stopped shell can be waited for");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stderr = err.join().unwrap_or_default();
    if timed_out {
        stderr.extend_from_slice(b"TIMEOUT\n");
    }
    std::process::Output {
        status,
        stdout: out.join().unwrap_or_default(),
        stderr,
    }
}

/// Runs every case through both shells on a pool of threads and returns the
/// cases whose outputs differ, in corpus order.
///
/// @param cases - the corpus
/// @param reference - the pinned `sqlite3`
/// @param ours - `inillucent-shell`
/// @param area - the folder under `_agent_output` the cases run in
pub(crate) fn run_corpus(
    cases: &[Case],
    reference: &Path,
    ours: &Path,
    area: &str,
) -> Vec<Divergence> {
    let area = workspace_root().join("_agent_output").join(area);
    let workers = std::thread::available_parallelism()
        .map_or(4, |count| count.get())
        .min(8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let found = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(case) = cases.get(index) else { break };
                let directory = area.join(directory_for(&case.name));
                let sqlite = run_shell(reference, &directory.join("sqlite"), &case.script);
                let inillucent = run_shell(ours, &directory.join("inillucent"), &case.script);
                if sqlite == inillucent {
                    let _ = std::fs::remove_dir_all(&directory);
                } else {
                    let divergence = Divergence {
                        case: case.clone(),
                        sqlite,
                        inillucent,
                    };
                    found
                        .lock()
                        .expect("no worker panicked")
                        .push((index, divergence));
                }
            });
        }
    });
    let mut found = found.into_inner().expect("no worker panicked");
    found.sort_by_key(|(index, _)| *index);
    found
        .into_iter()
        .map(|(_, divergence)| divergence)
        .collect()
}

/// Returns the first line on which two outputs differ, numbered from one.
///
/// @param divergence - the case and both outputs
pub(crate) fn first_difference(divergence: &Divergence) -> String {
    let mut left = divergence.sqlite.lines();
    let mut right = divergence.inillucent.lines();
    let mut line = 0usize;
    loop {
        line += 1;
        match (left.next(), right.next()) {
            (None, None) => return "the outputs differ only in a trailing newline".to_string(),
            (a, b) if a == b => continue,
            (a, b) => {
                return format!(
                    "line {line}\n    SQLite:     {}\n    inillucent: {}",
                    a.unwrap_or("(end)"),
                    b.unwrap_or("(end)")
                )
            }
        }
    }
}

/// Writes every difference to the file a variable names, when it is set.
///
/// @param divergences - what differed
/// @param total - how many cases ran
/// @param variable - the environment variable holding the report's path
pub(crate) fn write_report(divergences: &[Divergence], total: usize, variable: &str) {
    let Ok(path) = std::env::var(variable) else {
        return;
    };
    let quote = |text: &str| {
        let mut out = String::from("\"");
        for character in text.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                control if (control as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", control as u32));
                }
                other => out.push(other),
            }
        }
        out.push('"');
        out
    };
    let rows: Vec<String> = divergences
        .iter()
        .map(|divergence| {
            format!(
                "{{\"name\":{},\"file\":{},\"sql\":{},\"sqlite\":{},\"inillucent\":{}}}",
                quote(&divergence.case.name),
                quote(&divergence.case.file),
                quote(&divergence.case.script),
                quote(&divergence.sqlite),
                quote(&divergence.inillucent)
            )
        })
        .collect();
    let body = format!(
        "{{\"total\":{total},\"differ\":[\n{}\n]}}\n",
        rows.join(",\n")
    );
    std::fs::write(&path, body).expect("the usage report can be written");
}

/// Prints how many cases matched, by area, so a run says how far parity has
/// come and not only whether it passed.
///
/// @param label - what the corpus is called in the output
/// @param cases - the corpus
/// @param divergences - what differed
pub(crate) fn print_scorecard(label: &str, cases: &[Case], divergences: &[Divergence]) {
    let mut areas: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for case in cases {
        let area = case.name.split('/').next().unwrap_or("").to_string();
        areas.entry(area).or_default().0 += 1;
    }
    for divergence in divergences {
        let area = divergence
            .case
            .name
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        areas.entry(area).or_default().1 += 1;
    }
    eprintln!(
        "{label}: {} of {} cases match SQLite 3.53.4",
        cases.len() - divergences.len(),
        cases.len()
    );
    for (area, (total, differ)) in areas {
        eprintln!("  {area:<12} {:>4} of {total:>4} match", total - differ);
    }
}

/// Runs a corpus and returns what is wrong with it, or `None` when the pinned
/// shell is not there to compare with.
///
/// A case that differs and is not in `known.toml` is a problem, and so is a
/// listed case that now matches, so the list can only shrink by fixing
/// something and can only grow by somebody writing down why.
///
/// @param directory - the corpus directory
/// @param label - what the corpus is called in the output
/// @param variable - the environment variable that names a report file
pub(crate) fn corpus_problems(
    directory: &Path,
    label: &str,
    variable: &str,
) -> Option<Vec<String>> {
    let reference = reference_shell()?;
    let ours = inillucent_compat::cliproc::program("inillucent-shell");
    let cases = load_corpus(directory);
    let known = load_known(directory);
    let area = label.replace(' ', "_");
    let divergences = run_corpus(&cases, &reference, &ours, &area);
    write_report(&divergences, cases.len(), variable);
    print_scorecard(label, &cases, &divergences);
    let listed = |name: &str| known.iter().any(|row| row.case == name);
    let mut problems: Vec<String> = divergences
        .iter()
        .filter(|divergence| !listed(&divergence.case.name))
        .map(|divergence| {
            format!(
                "{} ({}) differs and is not in known.toml: {}",
                divergence.case.name,
                divergence.case.file,
                first_difference(divergence)
            )
        })
        .collect();
    for row in &known {
        if !divergences
            .iter()
            .any(|divergence| divergence.case.name == row.case)
        {
            problems.push(format!(
                "{} is in known.toml as a {} but now matches SQLite; remove its row",
                row.case, row.kind
            ));
        }
    }
    Some(problems)
}

/// Checks that a corpus is well formed and returns its cases.
///
/// Names are unique and carry an area prefix, every case has SQL, and every
/// row of `known.toml` names a case that exists with a kind that is one of the
/// two.
///
/// @param directory - the corpus directory
pub(crate) fn assert_well_formed(directory: &Path) -> Vec<Case> {
    let cases = load_corpus(directory);
    let mut names = std::collections::BTreeSet::new();
    for case in &cases {
        assert!(
            names.insert(case.name.clone()),
            "{} is named twice",
            case.name
        );
        assert!(!case.script.trim().is_empty(), "{} has no SQL", case.name);
        assert!(case.name.contains('/'), "{} has no area prefix", case.name);
    }
    for row in load_known(directory) {
        assert!(
            names.contains(&row.case),
            "known.toml names {}, which no file has",
            row.case
        );
        assert!(
            row.kind == "defect" || row.kind == "deviation",
            "{} has kind {}, which is neither defect nor deviation",
            row.case,
            row.kind
        );
    }
    cases
}
