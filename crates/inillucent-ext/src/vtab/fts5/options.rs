//! What a `CREATE VIRTUAL TABLE ... USING fts5(...)` said, and what it means.
//!
//! Invariant: **an option is honoured or refused, never recorded and ignored.**
//! Accepting `detail='none'` and then answering a phrase query from positions
//! the option says are not stored is the one outcome that is worse than saying
//! no, because the caller has no way to find out (task-1979, R5 and R15).
//!
//! It is its own module because `mod.rs` had grown past the size
//! `crates/inillucent-compat/tests/tooling/policy.rs` records for it, and the argument
//! grammar is the part of it that has nothing to do with the table.

use inillucent_base::DbResult;

use super::tokenize;
use crate::vtab::failure;

/// One column of an FTS5 table, and what was written about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColumnSpec {
    /// The column name.
    pub(super) name: Vec<u8>,
    /// Whether `UNINDEXED` was written, so the column is stored and not indexed.
    pub(super) unindexed: bool,
}

/// What a `CREATE VIRTUAL TABLE ... USING fts5(...)` said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Options {
    /// The indexed and stored columns, in order.
    pub(super) columns: Vec<ColumnSpec>,
    /// The tokenizer's name and its arguments.
    pub(super) tokenizer: Vec<Vec<u8>>,
    /// The table the rows live in, when they are not this table's own.
    ///
    /// `content='c'` makes an **external content** table: the index is built
    /// over rows that belong to `c`, and `%_content` is not created at all. It
    /// is what an application uses when the documents already exist and a
    /// second copy of them would double the file.
    pub(super) content: Option<Vec<u8>>,
    /// The column of the owner that holds each row's rowid, when `content_rowid=` named one.
    ///
    /// An owner that is a table has its rowid. An owner that is a view has none, so the
    /// host reads the view with this column as the rowid. `rowid` when nothing was written,
    /// which is what SQLite uses.
    pub(super) content_rowid: Vec<u8>,
    /// The option this build cannot honour, when the table declares one.
    ///
    /// **Recorded rather than ignored (task-1979, R15).** `detail=` and
    /// `columnsize=` each change what the index stores and what a query may ask
    /// of it, and both fell through to the catch-all that records an option and
    /// changes nothing - so `detail='none'`, under which SQLite refuses a
    /// phrase query because it holds no positions, answered the phrase query
    /// here from positions it had stored anyway.
    ///
    /// **`content_rowid=` is not on the list, because it works.** The review
    /// counted it with the other two; measured against the pinned SQLite on an
    /// external content table whose key is called `key` rather than `id`, both
    /// engines answer the same rows. This module reads an external table's
    /// columns by name, so the name of its rowid needs no separate handling.
    ///
    /// It is a refusal at `CREATE VIRTUAL TABLE` and a refusal on every query
    /// of a table an older build or SQLite wrote, rather than a refusal to
    /// open: a database has to open before the table in it can be dropped.
    pub(super) unsupported: Option<String>,
    /// Whether `content=''` made the table contentless.
    ///
    /// **Accepted and ignored until task-1979, R5.** `content=''` fell through
    /// to the catch-all that records an option and changes nothing, so
    /// `%_content` was created anyway and every column read back the document
    /// text where SQLite answers NULL. That is the difference an application
    /// chooses the option for: the source text is not supposed to be in the
    /// database at all, and it was, silently.
    pub(super) contentless: bool,
    /// The value of `contentless_delete=` when it is not 0.
    ///
    /// **Refused when a table is created, and ignored when one is opened.** Releases 0.1.1 to
    /// 2.3.3 accepted the option and ignored it: the table they stored is a plain contentless
    /// table, a `DELETE` from it was refused, and its rows are in the layout a plain contentless
    /// table uses. 2.3.4 to 2.8.1 refused every query of such a table and every `VACUUM` of its
    /// file, which made data those releases wrote unreadable. A table found in a file is now read
    /// as those releases stored it. A new `CREATE VIRTUAL TABLE` that asks for the option is
    /// still refused, because this build does not delete from a contentless table.
    pub(super) contentless_delete: Option<String>,
    /// What SQLite refuses in these arguments, when anything.
    ///
    /// **Read only when a table is being created.** SQLite refuses a reserved column name, a
    /// column option other than `UNINDEXED`, an option it does not have and a malformed value for
    /// `prefix`, `columnsize`, `detail` and the others. A table in a file that an older build
    /// wrote has to go on opening, so the arguments are still read leniently and the first
    /// problem is kept here for `CREATE VIRTUAL TABLE` to refuse.
    pub(super) invalid: Option<String>,
}

/// Reads the arguments of a `CREATE VIRTUAL TABLE ... USING fts5(...)`.
///
/// An argument is either a column - a bare name, optionally followed by
/// `UNINDEXED` - or an option, written `name = value`. That is FTS5's own
/// grammar, and the reason a column cannot be called `tokenize`.
pub(super) fn parse_options(arguments: &[Vec<u8>]) -> DbResult<Options> {
    let mut columns = Vec::new();
    let mut tokenizer = vec![b"unicode61".to_vec()];
    let mut content: Option<Vec<u8>> = None;
    let mut content_rowid: Vec<u8> = b"rowid".to_vec();
    let mut contentless = false;
    let mut unsupported: Option<String> = None;
    let mut contentless_delete: Option<String> = None;
    let mut not_indexed: Vec<String> = Vec::new();
    let mut invalid: Option<String> = None;
    for argument in arguments {
        let text = String::from_utf8_lossy(argument).trim().to_string();
        if let Some((name, value)) = split_option(&text) {
            if invalid.is_none() {
                invalid = refusal_for_option(&name, &value);
            }
            match name.to_ascii_lowercase().as_str() {
                "tokenize" => tokenizer = tokenize::parse_specification(&value),
                // **An external content table names its rows' owner**, and
                // reaching them is the one thing the module contract does not
                // give a module for free. It is asked for explicitly, by name,
                // through `ShadowTable::owner` - the same grant `fts5vocab`
                // uses - so the reach stays a grant rather than a hole.
                //
                // `content=''` is a *contentless* table, which is a different
                // thing: it stores no rows on purpose.
                "content" if !unquote_option(&value).is_empty() => {
                    content = Some(unquote_option(&value).as_bytes().to_vec())
                }
                "content" => contentless = true,
                "content_rowid" if !unquote_option(&value).is_empty() => {
                    content_rowid = unquote_option(&value).as_bytes().to_vec()
                }
                // FTS4 names a column it must not index as `notindexed=b`,
                // where FTS5 writes `b UNINDEXED`. The column is still stored
                // and returned; a query never finds a row through it.
                "notindexed" => {
                    not_indexed.push(unquote_option(&value).trim().to_ascii_lowercase())
                }
                // **Three options that change what the index holds.** Each is
                // accepted at the value this build does store - which is
                // SQLite's own default - and refused at every other, because
                // answering a query against a `detail='none'` index from
                // positions the option says are not there is the one outcome
                // that is worse than saying no.
                "detail" if !unquote_option(&value).eq_ignore_ascii_case("full") => {
                    unsupported = Some(format!("fts5 detail={}", unquote_option(&value)))
                }
                "columnsize" if unquote_option(&value).trim() != "1" => {
                    unsupported = Some(format!("fts5 columnsize={}", unquote_option(&value)))
                }
                // A contentless table that may be deleted from. Accepted and
                // ignored, a `DELETE` was refused and `count(*)` still counted
                // the row, so the table answered as though the delete had not
                // been written. Refused until the index can find a row's
                // postings without its text.
                "contentless_delete" if unquote_option(&value).trim() != "0" => {
                    contentless_delete = Some(format!(
                        "fts5 contentless_delete={}",
                        unquote_option(&value)
                    ))
                }

                // The options this build understands and the ones it does not
                // are both accepted, because refusing one would make a schema
                // SQLite wrote unreadable. What is not understood is recorded
                // in `%_config` and changes nothing.
                _ => {}
            }
            continue;
        }
        // A column may be written `"a b" UNINDEXED`, so the words are split
        // with the quotes honoured rather than on whitespace - or the column
        // would be called `"a`.
        let words = tokenize::split_words(&text);
        let Some(name) = words.first() else {
            continue;
        };
        let unindexed = words
            .iter()
            .skip(1)
            .any(|word| word.eq_ignore_ascii_case(b"UNINDEXED"));
        if invalid.is_none() {
            invalid = refusal_for_column(&text, &words);
        }
        columns.push(ColumnSpec {
            name: name.clone(),
            unindexed,
        });
    }
    if columns.is_empty() {
        return Err(failure("an fts5 table needs at least one column"));
    }
    for column in columns.iter_mut() {
        let folded = String::from_utf8_lossy(&column.name).to_ascii_lowercase();
        if not_indexed.contains(&folded) {
            column.unindexed = true;
        }
    }
    Ok(Options {
        columns,
        tokenizer,
        content,
        content_rowid,
        contentless,
        contentless_delete,
        unsupported,
        invalid,
    })
}

/// Refuses, when an FTS5 table is created, what SQLite refuses in its declaration.
///
/// The table may not be called `rank`, no column may be called `rank` or `rowid`, no two columns
/// may share a name when case is ignored, and no column may share the table's name, because that
/// column is the hidden one the table's `MATCH` is written on. The last two are reported by SQLite
/// as `vtable constructor failed`, which is the words used here.
///
/// @param options - the parsed arguments
/// @param table - the table's name
pub(super) fn refuse_at_create(options: &Options, table: &[u8]) -> DbResult<()> {
    let table = String::from_utf8_lossy(table).into_owned();
    let refuse = |said: String| Err(failure(said.clone()).with_message(said));
    if table.eq_ignore_ascii_case("rank") {
        return refuse(format!("reserved fts5 table name: {table}"));
    }
    if let Some(said) = &options.invalid {
        return refuse(said.clone());
    }
    let mut seen: Vec<String> = Vec::new();
    for column in &options.columns {
        let name = String::from_utf8_lossy(&column.name).to_ascii_lowercase();
        if name == table.to_ascii_lowercase() || seen.contains(&name) {
            return refuse(format!("vtable constructor failed: {table}"));
        }
        seen.push(name);
    }
    Ok(())
}

/// Reports whether an option name is a directive SQLite knows, the way SQLite compares them.
///
/// SQLite tests `strnicmp(directive, name, length of name)`, so any leading part of a directive's
/// name stands for it: `pre` is `prefix`. The directives are tried in the order SQLite tries
/// them, and the first that matches is the one that is meant.
///
/// @param name - the option as written
fn directive_of(name: &str) -> Option<&'static str> {
    const DIRECTIVES: [&str; 10] = [
        "prefix",
        "tokenize",
        "content",
        "contentless_delete",
        "contentless_unindexed",
        "content_rowid",
        "columnsize",
        "locale",
        "detail",
        "tokendata",
    ];
    DIRECTIVES.into_iter().find(|full| {
        full.len() >= name.len()
            && full
                .get(..name.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(name))
    })
}

/// Returns what SQLite refuses about one `name = value` argument, in SQLite's words.
///
/// @param name - the option as written
/// @param value - its value as written, quotes included
fn refusal_for_option(name: &str, value: &str) -> Option<String> {
    let value = unquote_option(value);
    let zero_or_one = |label: &str| {
        (value != "0" && value != "1").then(|| format!("malformed {label}=... directive"))
    };
    match directive_of(name) {
        None => Some(format!("unrecognized option: \"{name}\"")),
        Some("prefix") => malformed_prefix(value),
        Some("contentless_delete") => zero_or_one("contentless_delete"),
        Some("contentless_unindexed") => zero_or_one("contentless_delete"),
        Some("columnsize") => zero_or_one("columnsize"),
        Some("locale") => zero_or_one("locale"),
        Some("tokendata") => zero_or_one("tokendata"),
        Some("detail") => (!["none", "full", "column", "columns"]
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(value)))
        .then(|| "malformed detail=... directive".to_string()),
        Some(_) => None,
    }
}

/// Returns what SQLite refuses about a `prefix=` value, which is a list of lengths.
///
/// @param value - the value, quotes removed
fn malformed_prefix(value: &str) -> Option<String> {
    // SQLite's loop in `sqlite3Fts5ConfigParse`: skip spaces, read a number, skip spaces, take one
    // comma if there is one. So `2 3` and `2, 3` are both two lengths, and `2,,3` is malformed.
    let bytes = value.as_bytes();
    let mut at = 0usize;
    let mut seen = 0usize;
    loop {
        while bytes.get(at) == Some(&b' ') {
            at = at.saturating_add(1);
        }
        if at >= bytes.len() {
            return None;
        }
        let digits = bytes
            .get(at..)
            .unwrap_or(&[])
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digits == 0 {
            return Some("malformed prefix=... directive".to_string());
        }
        let length: usize = value
            .get(at..at.saturating_add(digits))
            .and_then(|text| text.parse().ok())
            .unwrap_or(1000);
        if length == 0 || length >= 1000 {
            return Some("prefix length out of range (max 999)".to_string());
        }
        seen += 1;
        if seen > 31 {
            return Some("too many prefix indexes (max 31)".to_string());
        }
        at = at.saturating_add(digits);
        while bytes.get(at) == Some(&b' ') {
            at = at.saturating_add(1);
        }
        if bytes.get(at) == Some(&b',') {
            at = at.saturating_add(1);
        }
    }
}

/// Returns what SQLite refuses about a column argument, in SQLite's words.
///
/// A column is its name and at most one option, which can only be `UNINDEXED`. The name may not
/// be `rank` or `rowid`.
///
/// @param text - the argument as written
/// @param words - the argument split into its words, quotes honoured
fn refusal_for_column(text: &str, words: &[Vec<u8>]) -> Option<String> {
    let name = words.first().map(|word| String::from_utf8_lossy(word));
    if let Some(name) = name {
        if name.eq_ignore_ascii_case("rank") || name.eq_ignore_ascii_case("rowid") {
            return Some(format!("reserved fts5 column name: {name}"));
        }
    }
    if words.len() > 2 {
        return Some(format!("parse error in \"{text}\""));
    }
    match words.get(1) {
        Some(word) if !word.eq_ignore_ascii_case(b"UNINDEXED") => Some(format!(
            "unrecognized column option: {}",
            String::from_utf8_lossy(word)
        )),
        _ => None,
    }
}

/// Strips the quotes an option's value is written inside.
///
/// FTS5 takes `content='c'`, `content="c"` and a bare `content=c` as the same
/// thing, and the difference between the empty value and a name is what decides
/// whether a table is contentless or external.
pub(super) fn unquote_option(value: &str) -> &str {
    let text = value.trim();
    for quote in ['\'', '"', '`'] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            return &text[1..text.len() - 1];
        }
    }
    text
}

/// Splits `name = value`, which is how an option is written.
fn split_option(text: &str) -> Option<(String, String)> {
    let (name, value) = text.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some((name.to_string(), value.trim().to_string()))
}

/// Returns the refusal an option this build cannot honour reports.
///
/// Carried as `unsupported` so the command line exits 3 and the driver reports
/// the status `unsupported`, which is how a caller tells "this engine has not
/// built that" apart from "your statement is wrong".
///
/// @param what - the option and the value it was given
pub(super) fn unsupported_option(what: &str) -> inillucent_base::DbError {
    failure(format!(
        "{what} is not built; this fts5 index stores full positions and one size per column, \
         and finds a row's postings from its text"
    ))
    .with_message(format!("{what} is not built"))
    .with_unsupported(what.to_string())
}
