//! Loading the FTS5 tables of a dump that SQLite wrote.
//!
//! Invariant: **only statements about an FTS5 table that this module itself created from a
//! `sqlite_schema` row are rewritten, and every other statement runs as it was written.** SQLite
//! dumps an FTS5 table as an `INSERT INTO sqlite_schema` row followed by the module's private
//! tables in SQLite's own format: `<name>_data` and `<name>_idx` hold index blocks, `<name>_docsize`
//! holds row sizes and `<name>_config` holds settings. Those blocks mean nothing to this engine's
//! module, which keeps a different layout, so copying them in would give a table that exists and
//! cannot be searched. What is portable is the text. `<name>_content` holds one row per document,
//! and inserting those rows through the table builds an index this engine can read. This is what
//! `inillucent migrate` does for a SQLite file, applied to the statements of a dump as they run.
//!
//! The rewrite is made as the shell runs each statement, and not by parsing the dump first,
//! because a dump is a stream: the statements arrive from `.read`, from standard input or from a
//! command line argument.
//!
//! | The dump says | The shell runs |
//! |---|---|
//! | `INSERT INTO sqlite_schema` with a `CREATE VIRTUAL TABLE ... fts5(...)` row | that `CREATE VIRTUAL TABLE` |
//! | `CREATE TABLE IF NOT EXISTS` for one of the table's five private tables | nothing, they exist |
//! | an insert into `_data`, `_idx` or `_docsize` | nothing |
//! | an insert into `_config` of a key other than `version` | the same setting, as the table's command |
//! | an insert into `_content` | an insert of the same row through the table |
//! | the end of the dump, for a table with an external content table | the table's `rebuild` command |

use crate::dump_virtual::{classify, quoted, visible_columns, Content, Kind};
use crate::shell::Shell;

/// The private tables an FTS5 table keeps in SQLite's dump.
const SHADOW_SUFFIXES: [&str; 5] = ["data", "idx", "content", "docsize", "config"];

/// What the rewrite decided for one statement.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rewrite {
    /// Run the statement as it was written.
    Keep,
    /// Run these statements in its place, which may be none.
    Replace(Vec<String>),
}

/// An FTS5 table this module created from a `sqlite_schema` row.
#[derive(Debug, Clone)]
struct Restored {
    /// The table's name, as the dump wrote it.
    name: String,
    /// The name in lower case, which is what a shadow table's name is matched against.
    folded: String,
    /// Whether its rows are in another table, which `rebuild` reads once that table is loaded.
    external: bool,
    /// Whether the table keeps its text at all. A table made with `content=''` keeps none.
    keeps_content: bool,
    /// The columns a row of `_content` is inserted into, read when the first row arrives.
    columns: Option<Vec<String>>,
}

/// The FTS5 tables restored so far in this session, and which of them still need a rebuild.
#[derive(Debug, Default, Clone)]
pub(crate) struct Restoring {
    /// Every table created from a `sqlite_schema` row.
    tables: Vec<Restored>,
    /// Names of the external content tables whose `rebuild` has not been run.
    pending_rebuilds: Vec<String>,
}

impl Restoring {
    /// Decides what to run in place of one statement.
    ///
    /// @param shell - the shell, asked for a table's columns when the first of its rows arrives
    /// @param sql - the statement as the dump wrote it
    pub(crate) fn rewrite(&mut self, shell: &Shell, sql: &str) -> Rewrite {
        let text = sql.trim_start();
        if starts_with_words(text, &["insert", "into"]) {
            return self.rewrite_insert(shell, text);
        }
        if starts_with_words(text, &["create", "table", "if", "not", "exists"]) {
            return self.rewrite_create(text);
        }
        if self.pending_rebuilds.is_empty() {
            return Rewrite::Keep;
        }
        self.rewrite_end_of_dump(text)
    }

    /// Rewrites an `INSERT`: the schema row of a table, or a row of one of its private tables.
    ///
    /// @param shell - the shell
    /// @param text - the statement
    fn rewrite_insert(&mut self, shell: &Shell, text: &str) -> Rewrite {
        let after = skip_words(text, 2);
        let Some((target, _)) = read_identifier(after) else {
            return Rewrite::Keep;
        };
        let bare = target
            .rsplit('.')
            .next()
            .unwrap_or(&target)
            .to_ascii_lowercase();
        if bare == "sqlite_schema" || bare == "sqlite_master" {
            return self.restore_from_schema_row(text);
        }
        let Some((index, suffix)) = self.shadow_of(&target) else {
            return Rewrite::Keep;
        };
        let Some(table) = self.tables.get(index).cloned() else {
            return Rewrite::Keep;
        };
        let Some(tail) = values_tail(text) else {
            return Rewrite::Keep;
        };
        match suffix {
            "config" => self.setting_row(&table, tail),
            "content" => self.content_row(shell, index, &table, tail),
            _ => Rewrite::Replace(Vec::new()),
        }
    }

    /// Turns the `sqlite_schema` row of an FTS5 table into the `CREATE VIRTUAL TABLE` it holds.
    ///
    /// A row for anything else, or in a column order other than the one a dump writes, runs as it
    /// was written.
    ///
    /// @param text - the statement
    fn restore_from_schema_row(&mut self, text: &str) -> Rewrite {
        let header = text.get(..text.len().min(160)).unwrap_or("");
        let squeezed: String = header
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        if !squeezed.contains("(type,name,tbl_name,rootpage,sql)") {
            return Rewrite::Keep;
        }
        let Some(tail) = values_tail(text) else {
            return Rewrite::Keep;
        };
        let literals = read_literals(tail);
        let (Some(kind), Some(name), Some(sql)) =
            (literals.first(), literals.get(1), literals.get(4))
        else {
            return Rewrite::Keep;
        };
        let (Literal::Text(kind), Literal::Text(name), Literal::Text(sql)) = (kind, name, sql)
        else {
            return Rewrite::Keep;
        };
        if !kind.eq_ignore_ascii_case("table") {
            return Rewrite::Keep;
        }
        let Kind::Fts5(content) = classify(sql) else {
            return Rewrite::Keep;
        };
        self.tables
            .retain(|held| held.folded != name.to_ascii_lowercase());
        self.tables.push(Restored {
            name: name.clone(),
            folded: name.to_ascii_lowercase(),
            external: content == Content::External,
            keeps_content: content != Content::None,
            columns: None,
        });
        if content == Content::External {
            self.pending_rebuilds.push(name.clone());
        }
        Rewrite::Replace(vec![sql.clone()])
    }

    /// Drops the `CREATE TABLE IF NOT EXISTS` of a private table, which the module already made.
    ///
    /// @param text - the statement
    fn rewrite_create(&self, text: &str) -> Rewrite {
        let after = skip_words(text, 5);
        match read_identifier(after) {
            Some((target, _)) if self.shadow_of(&target).is_some() => Rewrite::Replace(Vec::new()),
            _ => Rewrite::Keep,
        }
    }

    /// Runs the `rebuild` commands at the end of the dump's transaction.
    ///
    /// The tables an external content table reads are in the dump ahead of that point, whatever
    /// order they were written in, so the rebuild runs once they are all loaded.
    ///
    /// @param text - the statement
    fn rewrite_end_of_dump(&mut self, text: &str) -> Rewrite {
        let words: Vec<String> = text
            .trim_end_matches(|c: char| c == ';' || c.is_whitespace())
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        let ends = match words.first().map(String::as_str) {
            Some("commit") | Some("end") => true,
            Some("pragma") => {
                let joined = words.get(1..).unwrap_or(&[]).join("");
                joined == "writable_schema=off" || joined == "writable_schema=0"
            }
            _ => false,
        };
        if !ends {
            return Rewrite::Keep;
        }
        let mut statements: Vec<String> = std::mem::take(&mut self.pending_rebuilds)
            .iter()
            .map(|name| format!("INSERT INTO {0}({0}) VALUES('rebuild');", quoted(name)))
            .collect();
        statements.push(text.to_string());
        Rewrite::Replace(statements)
    }

    /// Finds the table a private table belongs to, and which of the five it is.
    ///
    /// @param target - the name a statement wrote
    fn shadow_of(&self, target: &str) -> Option<(usize, &'static str)> {
        let bare = target
            .rsplit('.')
            .next()
            .unwrap_or(target)
            .to_ascii_lowercase();
        self.tables.iter().enumerate().find_map(|(index, held)| {
            let rest = bare.strip_prefix(&held.folded)?.strip_prefix('_')?;
            SHADOW_SUFFIXES
                .iter()
                .find(|suffix| **suffix == rest)
                .map(|suffix| (index, *suffix))
        })
    }

    /// Turns a row of `_config` into the table's command, except the version row.
    ///
    /// @param table - the table
    /// @param tail - the statement from `VALUES` on
    fn setting_row(&self, table: &Restored, tail: &str) -> Rewrite {
        let literals = read_literals(tail);
        if let Some(Literal::Text(key)) = literals.first() {
            if key.eq_ignore_ascii_case("version") {
                return Rewrite::Replace(Vec::new());
            }
        }
        let name = quoted(&table.name);
        Rewrite::Replace(vec![format!("INSERT INTO {name}({name}, rank) {tail}")])
    }

    /// Turns a row of `_content` into an insert through the table.
    ///
    /// The row is `id`, then one value for each column. The table keeps its text only when it has
    /// a `_content` table, and a row for one that has none is dropped.
    ///
    /// @param shell - the shell, asked for the table's columns
    /// @param index - the table's place in the list
    /// @param table - the table
    /// @param tail - the statement from `VALUES` on
    fn content_row(
        &mut self,
        shell: &Shell,
        index: usize,
        table: &Restored,
        tail: &str,
    ) -> Rewrite {
        if !table.keeps_content || table.external {
            return Rewrite::Replace(Vec::new());
        }
        let columns = match &table.columns {
            Some(known) => known.clone(),
            None => {
                let read = visible_columns(shell, &table.name);
                if let Some(held) = self.tables.get_mut(index) {
                    held.columns = Some(read.clone());
                }
                read
            }
        };
        if columns.is_empty() {
            return Rewrite::Keep;
        }
        let list: Vec<String> = columns.iter().map(|column| quoted(column)).collect();
        Rewrite::Replace(vec![format!(
            "INSERT INTO {}(rowid,{}) {tail}",
            quoted(&table.name),
            list.join(",")
        )])
    }
}

/// One value written in a `VALUES` list.
#[derive(Debug, PartialEq, Eq)]
enum Literal {
    /// A quoted string, with its doubled quotes undone.
    Text(String),
    /// Anything else: a number, `NULL` or a blob literal, as written.
    Other(String),
}

/// Returns the statement from its `VALUES` keyword on, or `None` for a statement without one.
///
/// Only the start of the statement is searched, because the keyword follows the table name and a
/// row can hold a megabyte of blob.
///
/// @param text - the statement
fn values_tail(text: &str) -> Option<&str> {
    let window = text.as_bytes().get(..text.len().min(400)).unwrap_or(&[]);
    let at = window
        .windows(6)
        .position(|word| word.eq_ignore_ascii_case(b"values"))?;
    text.get(at..)
}

/// Reads the values of the first tuple after a `VALUES` keyword.
///
/// @param tail - the statement from `VALUES` on
fn read_literals(tail: &str) -> Vec<Literal> {
    let bytes = tail.as_bytes();
    let Some(open) = bytes.iter().position(|byte| *byte == b'(') else {
        return Vec::new();
    };
    let mut literals = Vec::new();
    let mut at = open + 1;
    while at < bytes.len() {
        while bytes.get(at).is_some_and(|byte| byte.is_ascii_whitespace()) {
            at += 1;
        }
        if bytes.get(at) == Some(&b'\'') {
            let (text, next) = read_quoted(tail, at);
            literals.push(Literal::Text(text));
            at = next;
        } else {
            let start = at;
            while bytes
                .get(at)
                .is_some_and(|byte| *byte != b',' && *byte != b')')
            {
                if bytes.get(at) == Some(&b'\'') {
                    at = read_quoted(tail, at).1;
                } else {
                    at += 1;
                }
            }
            literals.push(Literal::Other(
                tail.get(start..at).unwrap_or("").trim().to_string(),
            ));
        }
        while bytes.get(at).is_some_and(|byte| byte.is_ascii_whitespace()) {
            at += 1;
        }
        match bytes.get(at) {
            Some(b',') => at += 1,
            _ => break,
        }
    }
    literals
}

/// Reads a quoted string that starts at `start`, and returns it with the offset after it.
///
/// @param text - the text
/// @param start - the offset of the opening quote
fn read_quoted(text: &str, start: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut at = start + 1;
    while let Some(byte) = bytes.get(at).copied() {
        if byte == b'\'' {
            if bytes.get(at + 1) == Some(&b'\'') {
                out.push(b'\'');
                at += 2;
                continue;
            }
            return (String::from_utf8_lossy(&out).into_owned(), at + 1);
        }
        out.push(byte);
        at += 1;
    }
    (String::from_utf8_lossy(&out).into_owned(), at)
}

/// Reads a table name that starts the text, quoted or bare, and returns it with what follows it.
///
/// @param text - the text, which may start with white space
fn read_identifier(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let first = text.chars().next()?;
    let (open, close) = match first {
        '\'' | '"' | '`' => (first, first),
        '[' => ('[', ']'),
        _ => {
            let end = text
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '$'))
                .unwrap_or(text.len());
            let name = text.get(..end)?;
            return (!name.is_empty()).then(|| (name.to_string(), text.get(end..).unwrap_or("")));
        }
    };
    let body = text.get(open.len_utf8()..)?;
    let end = body.find(close)?;
    let name = body.get(..end)?;
    Some((
        name.to_string(),
        body.get(end + close.len_utf8()..).unwrap_or(""),
    ))
}

/// Reports whether the text starts with these words, in any case, separated by white space.
///
/// @param text - the statement
/// @param words - the words, in lower case
fn starts_with_words(text: &str, words: &[&str]) -> bool {
    let mut rest = text;
    for word in words {
        rest = rest.trim_start();
        let Some(head) = rest.as_bytes().get(..word.len()) else {
            return false;
        };
        if !head.eq_ignore_ascii_case(word.as_bytes()) {
            return false;
        }
        rest = rest.get(word.len()..).unwrap_or("");
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '(' || c == '\'' || c == '"') {
            return false;
        }
    }
    true
}

/// Returns the text after the first `count` words.
///
/// @param text - the statement
/// @param count - how many words to skip
fn skip_words(text: &str, count: usize) -> &str {
    let mut rest = text;
    for _ in 0..count {
        rest = rest.trim_start();
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        rest = rest.get(end..).unwrap_or("");
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema row of an FTS5 table is read for its name and its `CREATE` text.
    #[test]
    fn a_schema_row_is_read_into_its_statement() {
        let mut restoring = Restoring::default();
        let sql = "INSERT INTO sqlite_schema(type,name,tbl_name,rootpage,sql)VALUES('table','ft','ft',0,'CREATE VIRTUAL TABLE ft USING fts5(a, b UNINDEXED)');";
        let Rewrite::Replace(statements) = restoring.restore_from_schema_row(sql) else {
            panic!("the row was not rewritten");
        };
        assert_eq!(
            statements,
            vec!["CREATE VIRTUAL TABLE ft USING fts5(a, b UNINDEXED)".to_string()]
        );
        assert_eq!(
            restoring.shadow_of("ft_data").map(|found| found.1),
            Some("data")
        );
        assert_eq!(restoring.shadow_of("ft_other"), None);
    }

    /// A schema row for another module is left for the engine to decide about.
    #[test]
    fn a_schema_row_of_another_module_is_kept() {
        let mut restoring = Restoring::default();
        let sql = "INSERT INTO sqlite_schema(type,name,tbl_name,rootpage,sql)VALUES('table','r','r',0,'CREATE VIRTUAL TABLE r USING rtree(id,a,b)');";
        assert_eq!(restoring.restore_from_schema_row(sql), Rewrite::Keep);
    }

    /// Doubled quotes inside a literal are undone and commas inside it do not split it.
    #[test]
    fn literals_are_read_with_their_quotes_undone() {
        let literals = read_literals("VALUES('a''b,c',2,x'00ff')");
        assert_eq!(
            literals,
            vec![
                Literal::Text("a'b,c".to_string()),
                Literal::Other("2".to_string()),
                Literal::Other("x'00ff'".to_string()),
            ]
        );
    }

    /// Quoted and bare names are both read.
    #[test]
    fn identifiers_are_read_quoted_or_bare() {
        assert_eq!(
            read_identifier("'ft_data'(id INTEGER)").map(|found| found.0),
            Some("ft_data".to_string())
        );
        assert_eq!(
            read_identifier(" ft_idx VALUES(1)").map(|found| found.0),
            Some("ft_idx".to_string())
        );
    }

    /// Statement openings are matched without regard to case or spacing.
    #[test]
    fn statements_are_matched_by_their_first_words() {
        assert!(starts_with_words("INSERT  INTO x", &["insert", "into"]));
        assert!(starts_with_words(
            "create table if not exists 'x'(a)",
            &["create", "table", "if", "not", "exists"]
        ));
        assert!(!starts_with_words("INSERTED INTO", &["insert", "into"]));
    }
}
