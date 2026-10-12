//! `.open`: closing the database and opening another, with the reference's options.
//!
//! Invariant: **a word that begins with a dash is an option or an error, and is
//! never the name of the file.** `.open --new app.db` created a file called
//! `--new`, and `.open --readonly app.db` another called `--readonly`, because the
//! first word was the file. The options the engine can honour are honoured:
//! `--new` empties the file, `--readonly` opens it read only and refuses to create
//! it, `--ifexists` refuses a file that is not there, `--nofollow` refuses a
//! symbolic link, `--normal` and `--maxsize N` change nothing. A name that
//! begins with `file:` is a URI, with `mode=ro`, `mode=rw`, `mode=rwc` and
//! `mode=memory` understood. The options that name a mechanism this engine does
//! not have (`--append`, `--deserialize`, `--zip`, `--hexdb`) are refused by name.

use crate::dot::confine_path;
use crate::shell::Shell;

/// The reason `.open` gives for an option this engine cannot honour.
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "--append",
        "there is no appendvfs: a database is a file of its own",
    ),
    (
        "--deserialize",
        "there is no sqlite3_deserialize: a database is a file, or it is :memory:",
    ),
    ("--zip", "there is no ZIP virtual file system"),
    (
        "--hexdb",
        "there is no loader for the dbtotxt form: a database is a file, or it is :memory:",
    ),
];

/// What `.open` was asked to do, after its words are read.
#[derive(Default)]
struct Request {
    /// The file or URI, when one was named.
    named: Option<String>,
    /// `--new`: start from an empty file.
    fresh: bool,
    /// `--readonly`.
    read_only: bool,
    /// `--ifexists`: refuse a file that is not there.
    must_exist: bool,
    /// `--nofollow`: refuse a symbolic link.
    no_follow: bool,
}

/// Where a `file:` URI points and how it asks to be opened.
#[derive(Debug, PartialEq, Eq)]
pub struct Uri {
    /// The path, with escapes resolved; empty for a database held in memory.
    pub path: String,
    /// `mode=ro`.
    pub read_only: bool,
    /// `mode=ro` or `mode=rw`: the file has to be there already.
    pub must_exist: bool,
}

/// Reads a `file:` URI the way SQLite does.
///
/// `file:PATH`, `file:///PATH` and `file://localhost/PATH` name a path with `%XX`
/// escapes resolved. `?mode=` picks `ro`, `rw`, `rwc` or `memory`; other
/// parameters (`cache`, `immutable`, `nolock`, `vfs`, `psow`) change nothing here.
/// `file::memory:` is a database in memory. A text that does not begin with
/// `file:` is not a URI and is returned as none.
///
/// @param text - what was typed after `.open` or on the command line
pub fn parse_uri(text: &str) -> Option<Result<Uri, String>> {
    let rest = text.strip_prefix("file:")?;
    let (before_fragment, _) = rest.split_once('#').unwrap_or((rest, ""));
    let (location, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    let path = match location.strip_prefix("//") {
        Some(after) => {
            let (authority, path) = match after.find('/') {
                Some(at) => after.split_at(at),
                None => (after, ""),
            };
            if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
                return Some(Err(format!("invalid uri authority: {authority}")));
            }
            path.to_string()
        }
        None => location.to_string(),
    };
    let mut path = percent_decoded(&path);
    // `file:///C:/x.db` is the path `C:/x.db`, not `/C:/x.db`.
    let bytes = path.as_bytes();
    if bytes.first() == Some(&b'/')
        && bytes.get(2) == Some(&b':')
        && bytes.get(1).is_some_and(u8::is_ascii_alphabetic)
    {
        path.remove(0);
    }
    let mut uri = Uri {
        path,
        read_only: false,
        must_exist: false,
    };
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "mode" {
            continue;
        }
        match value {
            "ro" => {
                uri.read_only = true;
                uri.must_exist = true;
            }
            "rw" => uri.must_exist = true,
            "rwc" => {}
            "memory" => uri.path.clear(),
            other => return Some(Err(format!("no such access mode: {other}"))),
        }
    }
    if uri.path == ":memory:" {
        uri.path.clear();
    }
    Some(Ok(uri))
}

/// Resolves `%XX` escapes in a URI path.
///
/// @param text - the escaped text
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0usize;
    while let Some(&byte) = bytes.get(at) {
        let hex = |offset: usize| {
            bytes
                .get(at + offset)
                .and_then(|digit| char::from(*digit).to_digit(16))
        };
        if byte == b'%' {
            if let (Some(high), Some(low)) = (hex(1), hex(2)) {
                out.push((high * 16 + low) as u8);
                at += 3;
                continue;
            }
        }
        out.push(byte);
        at += 1;
    }
    crate::raw::decode(&out)
}

/// Reads the words after `.open`.
///
/// @param shell - where a complaint goes
/// @param arguments - the words after the command
fn read_request(shell: &mut Shell, arguments: &[&str]) -> Option<Request> {
    let mut request = Request::default();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        if let Some(flag) = word.strip_prefix("--").filter(|flag| !flag.is_empty()) {
            let flag = format!("--{flag}");
            match flag.as_str() {
                "--new" => request.fresh = true,
                "--readonly" => request.read_only = true,
                "--ifexists" => request.must_exist = true,
                "--nofollow" => request.no_follow = true,
                "--normal" => {}
                "--maxsize" => {
                    if words.next().is_none() {
                        shell.complain("unknown option: --maxsize");
                        return None;
                    }
                }
                other => {
                    if let Some((_, why)) = UNSUPPORTED.iter().find(|(name, _)| *name == other) {
                        shell.complain(&format!("Error: .open {other} is not supported: {why}"));
                    } else {
                        shell.complain(&format!("unknown option: {other}"));
                    }
                    return None;
                }
            }
            continue;
        }
        if word.starts_with('-') && word.len() > 1 {
            shell.complain(&format!("unknown option: {word}"));
            return None;
        }
        if request.named.is_some() {
            shell.complain(&format!("extra argument: \"{word}\""));
            return None;
        }
        request.named = Some((*word).to_string());
    }
    Some(request)
}

/// `.open ?OPTIONS? ?FILE?`: closes the current database and opens another.
///
/// **A database that cannot be opened is replaced by one in memory**, with the
/// notice the reference prints, so the commands that follow have something to run
/// on. The error names the file the person typed.
///
/// @param shell - the shell
/// @param arguments - the words after the command
pub fn open(shell: &mut Shell, arguments: &[&str]) {
    let Some(request) = read_request(shell, arguments) else {
        return;
    };
    let named = request.named.clone().unwrap_or_default();
    // **The reference refuses the file, not the command.** `sqlite3 -safe`
    // answers `cannot open disk-based database files in safe mode` and still
    // allows `.open :memory:`, so `.open` is not in `REFUSED_IN_SAFE_MODE` and
    // says this instead.
    if shell.safe && named != ":memory:" && !named.is_empty() {
        shell.complain("Error: cannot open disk-based database files in safe mode");
        return;
    }
    let (path, read_only, must_exist) = match resolve(shell, &request, &named) {
        Some(resolved) => resolved,
        None => return,
    };
    if let Some(reason) = refusal(&request, &path, must_exist, read_only) {
        substitute_memory(shell, &named, &reason);
        return;
    }
    if request.fresh && !path.is_empty() {
        crate::copy::remove_database_files(&path);
    }
    let target = if path.is_empty() { ":memory:" } else { &path };
    if let Err(message) = shell.reopen_as(target, read_only) {
        substitute_memory(shell, &named, &message);
    }
}

/// Works out the file, and whether it opens read only and has to exist.
///
/// @param shell - where a complaint goes
/// @param request - what was asked for
/// @param named - the file or URI as typed, empty when none was
fn resolve(shell: &mut Shell, request: &Request, named: &str) -> Option<(String, bool, bool)> {
    let mut read_only = request.read_only;
    let mut must_exist = request.must_exist;
    let mut path = named.to_string();
    if let Some(parsed) = parse_uri(named) {
        match parsed {
            Ok(uri) => {
                read_only |= uri.read_only;
                must_exist |= uri.must_exist;
                path = uri.path;
            }
            Err(message) => {
                shell.complain(&format!(
                    "Error: unable to open database \"{named}\": {message}"
                ));
                return None;
            }
        }
    }
    if path == ":memory:" {
        path.clear();
    }
    if path.is_empty() {
        return Some((path, read_only, must_exist));
    }
    let confined = confine_path(shell, &path)?;
    Some((confined, read_only, must_exist))
}

/// Returns why the file may not be opened under the options, if it may not.
///
/// @param request - what was asked for
/// @param path - the file, empty for memory
/// @param must_exist - whether the file has to be there
/// @param read_only - whether it is opened read only
fn refusal(request: &Request, path: &str, must_exist: bool, read_only: bool) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let file = std::path::Path::new(path);
    if request.no_follow {
        if let Ok(about) = std::fs::symlink_metadata(file) {
            if about.file_type().is_symlink() {
                return Some("unable to open database file".to_string());
            }
        }
    }
    let exists = file.exists() && !request.fresh;
    if (must_exist || read_only) && !exists {
        return Some("unable to open database file".to_string());
    }
    None
}

/// Reports that a database could not be opened and opens one in memory instead.
///
/// @param shell - the shell
/// @param named - the file as typed
/// @param reason - why it could not be opened
fn substitute_memory(shell: &mut Shell, named: &str, reason: &str) {
    shell.complain(&format!(
        "Error: unable to open database \"{named}\": {reason}"
    ));
    shell.note(&format!(
        "Notice: using substitute in-memory database instead of \"{named}\""
    ));
    if let Err(message) = shell.reopen(":memory:") {
        shell.complain(&format!("Error: {message}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A URI names a path, with escapes resolved, and a mode.
    #[test]
    fn a_uri_names_a_path_and_a_mode() {
        let plain = parse_uri("file:o.db?mode=rwc").unwrap().unwrap();
        assert_eq!(plain.path, "o.db");
        assert!(!plain.read_only && !plain.must_exist);
        let read = parse_uri("file:///C:/a%20b.db?mode=ro").unwrap().unwrap();
        assert_eq!(read.path, "C:/a b.db");
        assert!(read.read_only && read.must_exist);
        assert!(parse_uri("file:x?mode=memory")
            .unwrap()
            .unwrap()
            .path
            .is_empty());
        assert!(parse_uri("plain.db").is_none());
        assert!(parse_uri("file:x?mode=bogus").unwrap().is_err());
    }
}
