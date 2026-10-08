//! Times the common uses of an `inillucent_search` table through the shipped `Connection` API.
//!
//! Invariant: **every case runs on a corpus made by the same seeded generator, and every answer is
//! checked for a row count before its time counts.** `inillucent-searchgate` builds 500 short
//! documents in autocommit, so its build time is 500 file syncs and its queries read a few
//! kilobytes. An application stores thousands of chunks with a vector each, searches by keyword,
//! by vector and by both, opens the file again in a new process, and writes between searches.
//! This program times each of those as its own case, so a change can be measured on the case it
//! targets.
//!
//! Usage:
//!   inillucent-searchprobe `<scratch dir>` [--documents N] [--dims N] [--queries N] [--rounds N] [--only <case>]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use inillucent_engine::connect::{Connection, Database};

#[global_allocator]
static ALLOCATOR: inillucent_alloc::Pooled = inillucent_alloc::Pooled;

/// The sizes a run uses.
#[derive(Clone, Copy)]
struct Sizes {
    /// Rows in the search table.
    documents: usize,
    /// Numbers in each vector.
    dims: usize,
    /// Searches in each timed query case.
    queries: usize,
    /// Times each case runs; the median is reported.
    rounds: usize,
}

/// A small deterministic generator, so every build sees the same corpus.
struct Lcg(u64);

impl Lcg {
    /// Returns the next 31 bits.
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    /// Returns a number in `0..bound`, skewed toward small values so a few words are common.
    ///
    /// @param bound - the exclusive upper limit
    fn skewed(&mut self, bound: u64) -> u64 {
        let a = self.next() % bound;
        let b = self.next() % bound;
        a.min(b)
    }
}

/// Returns the made up word for a number, so the vocabulary needs no word list.
///
/// @param n - the word's number
fn word(n: u64) -> String {
    const SYLLABLES: [&str; 16] = [
        "ka", "lo", "mi", "nu", "pe", "ra", "si", "to", "vu", "ze", "bra", "cle", "dro", "fli",
        "gru", "ste",
    ];
    let mut out = String::new();
    let mut rest = n;
    loop {
        out.push_str(SYLLABLES[(rest % 16) as usize]);
        rest /= 16;
        if rest == 0 {
            break;
        }
    }
    out
}

/// Returns a document's title, body and vector as SQL text.
///
/// @param random - the generator
/// @param dims - numbers in the vector
fn document(random: &mut Lcg, dims: usize) -> (String, String, String) {
    let title: Vec<String> = (0..4).map(|_| word(random.skewed(3000))).collect();
    let body: Vec<String> = (0..40).map(|_| word(random.skewed(3000))).collect();
    (title.join(" "), body.join(" "), vector(random, dims))
}

/// Returns a vector of length 1 as SQL text.
///
/// @param random - the generator
/// @param dims - numbers in the vector
fn vector(random: &mut Lcg, dims: usize) -> String {
    let raw: Vec<f64> = (0..dims)
        .map(|_| (random.next() % 2001) as f64 / 1000.0 - 1.0)
        .collect();
    let length = raw.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
    let parts: Vec<String> = raw.iter().map(|x| format!("{:.5}", x / length)).collect();
    format!("[{}]", parts.join(","))
}

/// Opens the scratch database fresh and fills the search table in one transaction.
///
/// @param path - the database file
/// @param sizes - the corpus size
fn build(path: &Path, sizes: Sizes) -> Result<f64, String> {
    inillucent_base::testing::remove_database(path);
    let _ = std::fs::remove_file(path);
    let database = Database::open(path).map_err(|error| format!("open: {error}"))?;
    let connection = database.session();
    connection
        .execute_batch(&format!(
            "CREATE VIRTUAL TABLE docs USING inillucent_search(title, body, dims = {})",
            sizes.dims
        ))
        .map_err(|error| format!("create: {error}"))?;
    let mut random = Lcg(7);
    let rows: Vec<(String, String, String)> = (0..sizes.documents)
        .map(|_| document(&mut random, sizes.dims))
        .collect();
    let started = Instant::now();
    connection.execute("BEGIN").map_err(|e| e.to_string())?;
    let mut statement = connection
        .prepare("INSERT INTO docs(rowid, title, body, vector) VALUES (?1, ?2, ?3, ?4)")
        .map_err(|error| format!("prepare insert: {error}"))?;
    for (index, (title, body, vector)) in rows.iter().enumerate() {
        statement.reset();
        statement
            .bind_integer(1, index as i64 + 1)
            .and_then(|_| statement.bind_text(2, title))
            .and_then(|_| statement.bind_text(3, body))
            .and_then(|_| statement.bind_text(4, vector))
            .map_err(|error| format!("bind: {error}"))?;
        while statement
            .step()
            .map_err(|error| format!("insert: {error}"))?
        {}
    }
    drop(statement);
    connection.execute("COMMIT").map_err(|e| e.to_string())?;
    Ok(started.elapsed().as_secs_f64() * 1e3)
}

/// Runs a query with its parameters and returns how many rows it answered.
///
/// @param connection - the connection
/// @param sql - the query
/// @param texts - the text parameters, bound in order
fn rows(connection: &Connection<'_>, sql: &str, texts: &[&str]) -> Result<usize, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("{sql}: {error}"))?;
    for (index, text) in texts.iter().enumerate() {
        statement
            .bind_text(index as u32 + 1, text)
            .map_err(|error| format!("{sql}: {error}"))?;
    }
    let mut count = 0;
    while statement
        .step()
        .map_err(|error| format!("{sql}: {error}"))?
    {
        count += 1;
    }
    Ok(count)
}

/// The three query shapes an application uses.
const KEYWORD: &str = "SELECT rowid, title FROM docs WHERE docs MATCH ?1 ORDER BY rank LIMIT 10";
const VECTOR: &str = "SELECT rowid, title FROM docs WHERE vector = ?1 AND k = 10";
const HYBRID: &str =
    "SELECT rowid, title, score(docs) FROM docs WHERE docs MATCH ?1 AND vector = ?2 AND k = 10 ORDER BY rank";

/// Runs `queries` searches of one shape and returns the milliseconds they took.
///
/// @param connection - the connection
/// @param shape - `keyword`, `vector` or `hybrid`
/// @param sizes - the corpus size and the number of searches
fn searches(connection: &Connection<'_>, shape: &str, sizes: Sizes) -> Result<f64, String> {
    let mut random = Lcg(99);
    let terms: Vec<String> = (0..sizes.queries)
        .map(|_| word(random.skewed(400)))
        .collect();
    let vectors: Vec<String> = (0..sizes.queries)
        .map(|_| vector(&mut random, sizes.dims))
        .collect();
    let started = Instant::now();
    let mut found = 0;
    for (term, query) in terms.iter().zip(&vectors) {
        found += match shape {
            "keyword" => rows(connection, KEYWORD, &[term])?,
            "vector" => rows(connection, VECTOR, &[query])?,
            _ => rows(connection, HYBRID, &[term, query])?,
        };
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e3;
    if found == 0 {
        return Err(format!("{shape}: no search found a row"));
    }
    Ok(elapsed)
}

/// Opens the built database again, as a new process would, and times its first keyword search.
///
/// @param path - the database file
fn cold(path: &Path) -> Result<f64, String> {
    let started = Instant::now();
    let database = Database::open(path).map_err(|error| format!("open: {error}"))?;
    let connection = database.session();
    let found = rows(&connection, KEYWORD, &["ka"])?;
    let elapsed = started.elapsed().as_secs_f64() * 1e3;
    if found == 0 {
        return Err("cold: the first search found no row".to_string());
    }
    Ok(elapsed)
}

/// Writes one row and then searches, `queries` times, inside one transaction.
///
/// @param connection - the connection
/// @param sizes - the corpus size and the number of cycles
/// @param first - the first rowid to write
fn write_then_search(connection: &Connection<'_>, sizes: Sizes, first: i64) -> Result<f64, String> {
    let mut random = Lcg(first as u64);
    let started = Instant::now();
    connection.execute("BEGIN").map_err(|e| e.to_string())?;
    for offset in 0..sizes.queries as i64 {
        let (title, body, vector) = document(&mut random, sizes.dims);
        let mut statement = connection
            .prepare("INSERT INTO docs(rowid, title, body, vector) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|error| error.to_string())?;
        statement
            .bind_integer(1, first + offset)
            .and_then(|_| statement.bind_text(2, &title))
            .and_then(|_| statement.bind_text(3, &body))
            .and_then(|_| statement.bind_text(4, &vector))
            .map_err(|error| error.to_string())?;
        while statement.step().map_err(|error| error.to_string())? {}
        drop(statement);
        rows(connection, KEYWORD, &[&word(random.skewed(400))])?;
    }
    connection.execute("ROLLBACK").map_err(|e| e.to_string())?;
    Ok(started.elapsed().as_secs_f64() * 1e3)
}

/// Returns the median of a set of timings.
///
/// @param values - the timings
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// Runs every case `rounds` times and prints the median of each.
///
/// @param directory - the scratch directory
/// @param sizes - the corpus size and the counts
/// @param only - a case name to run alone
fn run(directory: &Path, sizes: Sizes, only: Option<&str>) -> Result<(), String> {
    let path: PathBuf = directory.join("searchprobe.rdb");
    let wanted = |name: &str| only.is_none_or(|one| one == name);
    let mut times: Vec<(&str, Vec<f64>)> = Vec::new();
    let mut builds = Vec::new();
    for _ in 0..sizes.rounds.min(3) {
        builds.push(build(&path, sizes)?);
    }
    times.push(("search.insert", builds));
    let database = Database::open(&path).map_err(|error| format!("open: {error}"))?;
    let connection = database.session();
    // The first search loads the index; the cases below time searches after it.
    rows(&connection, KEYWORD, &["ka"])?;
    for shape in ["keyword", "vector", "hybrid"] {
        let name = match shape {
            "keyword" => "search.keyword",
            "vector" => "search.vector",
            _ => "search.hybrid",
        };
        if wanted(name) {
            let mut each = Vec::new();
            for _ in 0..sizes.rounds {
                each.push(searches(&connection, shape, sizes)?);
            }
            times.push((name, each));
        }
    }
    if wanted("search.write.search") {
        let mut each = Vec::new();
        for round in 0..sizes.rounds {
            each.push(write_then_search(
                &connection,
                sizes,
                1_000_000 + (round as i64) * 10_000,
            )?);
        }
        times.push(("search.write.search", each));
    }
    drop(database);
    if wanted("search.cold") {
        let mut each = Vec::new();
        for _ in 0..sizes.rounds {
            each.push(cold(&path)?);
        }
        times.push(("search.cold", each));
    }
    println!("case                       median ms");
    for (name, each) in times {
        println!("{name:<24} {:>12.3}", median(each));
    }
    Ok(())
}

/// Returns a numeric flag's value, when it was given.
///
/// @param arguments - the command line
/// @param name - the flag, with its dashes
fn flag(arguments: &[String], name: &str) -> Option<String> {
    let at = arguments.iter().position(|value| value == name)?;
    arguments.get(at + 1).cloned()
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(directory) = arguments.first().filter(|value| !value.starts_with("--")) else {
        eprintln!("usage: inillucent-searchprobe <scratch dir> [--documents N] [--dims N] [--queries N] [--rounds N] [--only <case>]");
        return ExitCode::from(2);
    };
    let number = |name: &str, default: usize| {
        flag(&arguments, name)
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let sizes = Sizes {
        documents: number("--documents", 5000),
        dims: number("--dims", 384),
        queries: number("--queries", 100),
        rounds: number("--rounds", 5).max(1),
    };
    let _ = std::fs::create_dir_all(directory);
    let only = flag(&arguments, "--only");
    match run(Path::new(directory), sizes, only.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("inillucent-searchprobe: {reason}");
            ExitCode::FAILURE
        }
    }
}
