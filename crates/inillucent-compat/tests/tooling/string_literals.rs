//! No string literal in the workspace holds the indentation of a line it was
//! joined from.
//!
//! Invariant: **a run of six or more spaces between two words inside an
//! ordinary string literal is a line continuation that lost its `\`, unless
//! the file and the text are on the allow list below.**
//!
//! ## Why this exists
//!
//! A Rust string that continues onto the next line needs a `\` at the end of
//! the line, which drops the line break and the next line's indentation. At
//! some point a tool joined such literals into one line and dropped the `\`
//! but not the indentation. The text people read then held a run of spaces in
//! the middle of a sentence: the `busy` refusal of a read transaction that
//! tried to write said "so it cannot                  write", and so did the
//! nested transaction refusal, the error for a row larger than a page, the
//! error for an import that is not UTF-8, three commands' `inillucent help`
//! text and three notes of `inillucent capabilities`. The bug hunt found 110 such
//! lines in 79 files. Nothing compared any of these strings with the text a
//! person was meant to read, so nothing failed.

use std::path::{Path, PathBuf};

use inillucent_compat::workspace_root;

/// Files whose literals hold aligned columns on purpose, with the reason.
const ALLOWED_FILES: &[(&str, &str)] = &[
    (
        "crates/inillucent-cli/src/help.rs",
        "SQLite's own dot command help, copied with its column layout",
    ),
    (
        "crates/inillucent-cli/src/main.rs",
        "the shell's option list, aligned in columns as sqlite3 prints it",
    ),
    (
        "crates/inillucent-cli/src/bin/inillucent.rs",
        "the command line's option list, aligned in columns",
    ),
    (
        "crates/inillucent-cli/src/bin/inillucent-mcp.rs",
        "the MCP server's option list, aligned in columns",
    ),
    (
        "crates/inillucent-compat/tests/tooling/string_literals.rs",
        "this suite, whose own cases are the shape it looks for",
    ),
];

/// Text that is allowed wherever it appears, with the reason.
const ALLOWED_TEXT: &[(&str, &str)] = &[
    (
        "Got: [",
        "the TCL test runner's report, which SQLite aligns this way",
    ),
    (
        "\\n      ",
        "an indented continuation of a multi line report, written with an explicit line break",
    ),
    (
        "a                    x",
        "test data for the RTRIM collation, where the spaces are the value",
    ),
];

/// Returns every `.rs` file under a directory, skipping build output.
///
/// @param directory - where to start
fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}

/// Returns the ordinary (not raw) string literals that start and end on one
/// line, as their text between the quotes.
///
/// A line comment ends the scan, and a raw string (`r"`, `r#"`) is skipped
/// whole, because both hold text that is not a message.
///
/// @param line - one line of source
fn literals_on(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut at = 0usize;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'/' && bytes.get(at + 1) == Some(&b'/') {
            break;
        }
        if byte == b'\'' {
            // A character literal such as '"' must not open a string.
            if bytes.get(at + 2) == Some(&b'\'') {
                at += 3;
                continue;
            }
            if bytes.get(at + 1) == Some(&b'\\') && bytes.get(at + 3) == Some(&b'\'') {
                at += 4;
                continue;
            }
        }
        if byte != b'"' {
            at += 1;
            continue;
        }
        let raw = at > 0 && matches!(bytes.get(at - 1), Some(b'r') | Some(b'#'));
        let mut end = at + 1;
        let mut text = String::new();
        let mut closed = false;
        while let Some(&inner) = bytes.get(end) {
            if inner == b'\\' && !raw {
                text.push('\\');
                if let Some(&next) = bytes.get(end + 1) {
                    text.push(next as char);
                }
                end += 2;
                continue;
            }
            if inner == b'"' {
                closed = true;
                break;
            }
            text.push(inner as char);
            end += 1;
        }
        if closed && !raw {
            found.push(text);
        }
        at = end + 1;
    }
    found
}

/// Reports whether a literal holds a run of six or more spaces between the end
/// of one word and the start of the next.
///
/// **Only between words.** A report a measurement binary prints aligns its
/// columns with runs of spaces too, but there the run follows a label's colon
/// or a placeholder's closing brace, or comes before a `:`, `=`, `{` or `[`.
/// A joined line leaves the run between a word or a sentence's punctuation and
/// the next word, which is the shape this asks about.
///
/// @param text - the literal's text
fn has_a_joined_gap(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes.get(index) == Some(&b' ') {
            let start = index;
            while bytes.get(index) == Some(&b' ') {
                index += 1;
            }
            let before = start
                .checked_sub(1)
                .and_then(|at| bytes.get(at))
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || b".,;)`'".contains(byte));
            let after = bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'(');
            if index - start >= 6 && before && after {
                return true;
            }
        } else {
            index += 1;
        }
    }
    false
}

/// Every ordinary string literal in `crates/` and `drivers/` reads as one line
/// of text, with no run of indentation left inside it.
#[test]
fn no_string_literal_holds_a_joined_lines_indentation() {
    let root = workspace_root();
    let mut problems = Vec::new();
    for top in ["crates", "drivers"] {
        for path in rust_files(&root.join(top)) {
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if ALLOWED_FILES.iter().any(|(file, _)| *file == name) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (number, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let bad = literals_on(line).into_iter().any(|literal| {
                    has_a_joined_gap(&literal)
                        && !ALLOWED_TEXT
                            .iter()
                            .any(|(allowed, _)| literal.contains(allowed) || line.contains(allowed))
                });
                if bad {
                    problems.push(format!("{name}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} string literal(s) hold a run of spaces where a line was joined without its `\\`. \
         End the line with `\\` instead, or add the file or text to the allow list with a \
         reason:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The scanner finds the shape it is for, and leaves alone the shapes it must
/// not report.
#[test]
fn the_scanner_finds_a_joined_literal_and_nothing_else() {
    let joined = r#"    .with_message("so it cannot                  write; roll it back")"#;
    assert!(literals_on(joined)
        .iter()
        .any(|text| has_a_joined_gap(text)));
    let continued = r#"    "so it cannot \"#;
    assert!(!literals_on(continued)
        .iter()
        .any(|text| has_a_joined_gap(text)));
    let raw = r##"    let sql = r#"SELECT a,        b FROM t"#;"##;
    assert!(!literals_on(raw).iter().any(|text| has_a_joined_gap(text)));
    let comment = r#"    let a = 1; // "so it cannot                  write""#;
    assert!(!literals_on(comment)
        .iter()
        .any(|text| has_a_joined_gap(text)));
    let character = r#"    if c == '"' { say("one        two") }"#;
    assert!(literals_on(character)
        .iter()
        .any(|text| has_a_joined_gap(text)));
}
