//! How `.dump` writes the virtual tables that have search indexes behind them.
//!
//! Invariant: **what a dump holds for an FTS5 table, an `inillucent_search` table or a vector
//! index can be replayed by a shell with the defensive setting on.** The reference writes a
//! virtual table as an `INSERT INTO sqlite_schema` under `PRAGMA writable_schema=ON`, followed by
//! its private tables, and a defensive connection refuses both, so a dump of such a table could
//! not be loaded into a database that the shell itself had just opened. Here the table is a
//! `CREATE VIRTUAL TABLE` and its rows are ordinary inserts through the table, which rebuilds the
//! index as it goes. A vector index is a `CREATE INDEX ... USING inillucent_hnsw` over the rows of
//! the table it indexes. Every other virtual table keeps the reference's form.

use std::collections::HashSet;

use crate::render::quote_literal;
use crate::shell::Shell;

/// What a virtual table's `CREATE` statement says it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `fts5`, with the way its rows are kept.
    Fts5(Content),
    /// `inillucent_search` with no source table: the rows are its own.
    Search {
        /// Whether the table has a vector column.
        has_vectors: bool,
    },
    /// The store behind `CREATE INDEX ... USING inillucent_hnsw` or `ivfflat`.
    VectorIndex(IndexDefinition),
    /// Any other virtual table, which is dumped the way the reference dumps it.
    Other,
}

/// Where the text of an FTS5 table's rows is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Content {
    /// In the table's own `%_content`.
    Own,
    /// In another table, which `rebuild` reads. The table's rows are not in the dump.
    External,
    /// Nowhere: `content=''`. Only the index exists, and it cannot be written as inserts.
    None,
}

/// What a vector index is, read back from the store's `CREATE VIRTUAL TABLE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexDefinition {
    /// `inillucent_hnsw` or `ivfflat`.
    pub module: String,
    /// The table that is indexed.
    pub table: String,
    /// The indexed column.
    pub column: String,
    /// The `WITH (...)` settings, as they were written.
    pub settings: Vec<String>,
}

/// Works out what kind of virtual table a `CREATE VIRTUAL TABLE` statement makes.
///
/// @param sql - the statement as the schema holds it
pub(crate) fn classify(sql: &str) -> Kind {
    let Some((module, arguments)) = module_and_arguments(sql) else {
        return Kind::Other;
    };
    match module.to_ascii_lowercase().as_str() {
        "fts5" => Kind::Fts5(content_of(&arguments)),
        "inillucent_search" | "ivfflat" => classify_search(&module, &arguments),
        _ => Kind::Other,
    }
}

/// Classifies a search store as a table of its own or as the store of a vector index.
///
/// @param module - the module's name as written
/// @param arguments - the module's arguments
fn classify_search(module: &str, arguments: &[String]) -> Kind {
    let source = option_value(arguments, "source");
    let column = option_value(arguments, "source_column");
    if let (Some(table), Some(column)) = (source, column) {
        let settings = arguments
            .iter()
            .filter(|argument| !is_option(argument, &["dims", "source", "source_column"]))
            .filter(|argument| argument.contains('='))
            .cloned()
            .collect();
        let module = if module.eq_ignore_ascii_case("ivfflat") {
            "ivfflat"
        } else {
            "inillucent_hnsw"
        };
        return Kind::VectorIndex(IndexDefinition {
            module: module.to_string(),
            table,
            column,
            settings,
        });
    }
    if module.eq_ignore_ascii_case("ivfflat") {
        return Kind::Other;
    }
    Kind::Search {
        has_vectors: option_value(arguments, "dims").is_some(),
    }
}

/// Returns the names of the private tables a kind of virtual table keeps its data in.
///
/// @param kind - what the virtual table is
fn shadow_suffixes(kind: &Kind) -> &'static [&'static str] {
    match kind {
        Kind::Fts5(_) => &["data", "idx", "content", "docsize", "config"],
        Kind::VectorIndex(index) if index.module == "ivfflat" => &["centroid", "row", "config"],
        Kind::Search { .. } | Kind::VectorIndex(_) => {
            &["state", "gen", "delta", "content", "config"]
        }
        Kind::Other => &[],
    }
}

/// Returns the lower case names of the private tables of every virtual table that is dumped as
/// statements, so the dump leaves them out.
///
/// @param tables - every table the dump covers, with its `CREATE` text
pub(crate) fn private_tables_left_out(tables: &[(String, String)]) -> HashSet<String> {
    let mut names = HashSet::new();
    for (name, sql) in tables {
        let kind = classify(sql);
        for suffix in shadow_suffixes(&kind) {
            names.insert(format!("{}_{}", name.to_ascii_lowercase(), suffix));
        }
    }
    names
}

/// Writes one virtual table: its `CREATE`, its settings and its rows.
///
/// A vector index writes nothing here, because it is created after the rows of the table it
/// indexes; see [`index_statement`]. Nothing is written for the definition when only data was
/// asked for.
///
/// @param shell - where the lines go
/// @param name - the table's name
/// @param sql - the statement the schema holds
/// @param kind - what the table is
/// @param data_only - whether `--data-only` was given
pub(crate) fn write_table(shell: &mut Shell, name: &str, sql: &str, kind: &Kind, data_only: bool) {
    if matches!(kind, Kind::VectorIndex(_) | Kind::Other) {
        return;
    }
    if !data_only {
        shell.say(&format!("{sql};"));
        write_settings(shell, name, kind);
    }
    match kind {
        Kind::Fts5(Content::Own) => write_rows(shell, name, false),
        Kind::Search { has_vectors } => write_rows(shell, name, *has_vectors),
        _ => {}
    }
}

/// Writes the settings an FTS5 table was given after it was created, such as `rank`.
///
/// They live in `%_config` beside the `version` row, and are set again with the table's command
/// column, which is the column named like the table.
///
/// @param shell - where the lines go
/// @param name - the table's name
/// @param kind - what the table is
fn write_settings(shell: &mut Shell, name: &str, kind: &Kind) {
    if !matches!(kind, Kind::Fts5(_)) {
        return;
    }
    let config = quoted(&format!("{name}_config"));
    let reading = format!("SELECT k, v FROM {config} WHERE k <> 'version' ORDER BY k");
    let Ok((_, rows)) = shell.collect(&reading) else {
        return;
    };
    for row in rows {
        let (Some(key), Some(value)) = (row.first(), row.get(1)) else {
            continue;
        };
        shell.say(&format!(
            "INSERT INTO {}({}, rank) VALUES({},{});",
            quoted(name),
            quoted(name),
            quote_literal(key),
            quote_literal(value)
        ));
    }
}

/// Writes every row of a virtual table as an `INSERT` through the table.
///
/// The rowid is written, because a row's number is what a search returns and what a table that
/// names this one as its source refers to.
///
/// @param shell - where the lines go
/// @param name - the table's name
/// @param with_vector - whether the hidden `vector` column is written too
fn write_rows(shell: &mut Shell, name: &str, with_vector: bool) {
    let columns = visible_columns(shell, name);
    let mut names: Vec<String> = vec!["rowid".to_string()];
    names.extend(columns.iter().map(|column| quoted(column)));
    if with_vector {
        names.push("vector".to_string());
    }
    let list = names.join(",");
    let reading = format!("SELECT {list} FROM {} ORDER BY rowid", quoted(name));
    let Ok((_, rows)) = shell.collect(&reading) else {
        shell.complain(&format!(
            "-- the rows of {name} could not be read, so none are in this dump"
        ));
        return;
    };
    for row in rows {
        let values: Vec<String> = row.iter().map(quote_literal).collect();
        shell.say(&format!(
            "INSERT INTO {}({list}) VALUES({});",
            quoted(name),
            values.join(",")
        ));
    }
}

/// Returns the columns of a virtual table that hold a row's text, in declaration order.
///
/// @param shell - the shell to ask
/// @param table - the table's name
fn visible_columns(shell: &mut Shell, table: &str) -> Vec<String> {
    let literal = format!("'{}'", table.replace('\'', "''"));
    let Ok((_, rows)) = shell.collect(&format!("SELECT name FROM pragma_table_info({literal})"))
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| match row.first() {
            Some(inillucent_value::Value::Text(text)) => {
                Some(String::from_utf8_lossy(text.raw()).into_owned())
            }
            _ => None,
        })
        .collect()
}

/// Returns the `rebuild` commands the external content FTS5 tables of a dump need.
///
/// Such a table holds an index over rows that belong to another table, so its rows are not
/// written. Once every table is back the index is built from them.
///
/// @param tables - every table the dump covers, with its `CREATE` text
pub(crate) fn rebuild_statements(tables: &[(String, String)]) -> Vec<String> {
    tables
        .iter()
        .filter(|(_, sql)| classify(sql) == Kind::Fts5(Content::External))
        .map(|(name, _)| {
            format!(
                "INSERT INTO {}({}) VALUES('rebuild');",
                quoted(name),
                quoted(name)
            )
        })
        .collect()
}

/// Returns the `CREATE INDEX` statement that makes a vector index again, or nothing for a table
/// that is not one.
///
/// @param name - the index's name, which is its store's name
/// @param kind - what the store is
pub(crate) fn index_statement(name: &str, kind: &Kind) -> Option<String> {
    let Kind::VectorIndex(index) = kind else {
        return None;
    };
    let with = if index.settings.is_empty() {
        String::new()
    } else {
        format!(" WITH ({})", index.settings.join(", "))
    };
    Some(format!(
        "CREATE INDEX {} ON {} USING {}({}){with};",
        quoted(name),
        quoted(&index.table),
        index.module,
        quoted(&index.column)
    ))
}

/// Returns an identifier quoted only when it has to be.
///
/// @param name - the identifier
fn quoted(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !name.starts_with(|character: char| character.is_ascii_digit());
    if plain {
        return name.to_string();
    }
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Splits `CREATE VIRTUAL TABLE name USING module(arguments)` into the module and its arguments.
///
/// @param sql - the statement
fn module_and_arguments(sql: &str) -> Option<(String, Vec<String>)> {
    let upper = sql.to_ascii_uppercase();
    let using = upper.find(" USING ")?;
    let rest = sql.get(using + " USING ".len()..)?;
    let open = rest.find('(')?;
    let module = rest.get(..open)?.trim().to_string();
    let close = rest.rfind(')')?;
    let inside = rest.get(open + 1..close)?;
    Some((module, split_arguments(inside)))
}

/// Splits an argument list at the commas that are not inside quotes or brackets.
///
/// @param text - the text between the module's parentheses
fn split_arguments(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    for character in text.chars() {
        match (quote, character) {
            (Some(open), found) if found == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"' | '`') => quote = Some(character),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                parts.push(core::mem::take(&mut current).trim().to_string());
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Reports whether an argument is `name = value` for one of the names, ignoring case.
///
/// @param argument - one argument as written
/// @param names - the option names to look for
fn is_option(argument: &str, names: &[&str]) -> bool {
    let Some((key, _)) = argument.split_once('=') else {
        return false;
    };
    names
        .iter()
        .any(|name| key.trim().eq_ignore_ascii_case(name))
}

/// Returns the value of `name = value` among the arguments, without its quotes.
///
/// @param arguments - the module's arguments
/// @param name - the option to look for
fn option_value(arguments: &[String], name: &str) -> Option<String> {
    arguments.iter().find_map(|argument| {
        let (key, value) = argument.split_once('=')?;
        if !key.trim().eq_ignore_ascii_case(name) {
            return None;
        }
        let value = value.trim();
        let bare = ['\'', '"', '`']
            .into_iter()
            .find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
            .unwrap_or(value);
        Some(bare.to_string())
    })
}

/// Reads the `content` option of an FTS5 table.
///
/// @param arguments - the module's arguments
fn content_of(arguments: &[String]) -> Content {
    match option_value(arguments, "content") {
        None => Content::Own,
        Some(value) if value.is_empty() => Content::None,
        Some(_) => Content::External,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain FTS5 table keeps its rows, an external one is rebuilt and a contentless one has none.
    #[test]
    fn the_content_option_decides_how_rows_are_kept() {
        assert_eq!(
            classify("CREATE VIRTUAL TABLE f USING fts5(a)"),
            Kind::Fts5(Content::Own)
        );
        assert_eq!(
            classify("CREATE VIRTUAL TABLE f USING fts5(a, content='t')"),
            Kind::Fts5(Content::External)
        );
        assert_eq!(
            classify("CREATE VIRTUAL TABLE f USING fts5(a, content='')"),
            Kind::Fts5(Content::None)
        );
    }

    /// The store of a vector index becomes the `CREATE INDEX` that made it.
    #[test]
    fn a_vector_index_store_is_written_as_its_create_index() {
        let sql = "CREATE VIRTUAL TABLE ti USING inillucent_search(body, dims=3, source=t, \
                   source_column=v, m=8, metric='l2', mode=approximate)";
        let kind = classify(sql);
        assert_eq!(
            index_statement("ti", &kind).as_deref(),
            Some("CREATE INDEX ti ON t USING inillucent_hnsw(v) WITH (m=8, metric='l2', mode=approximate);")
        );
    }

    /// A search table of its own is dumped as rows, with its vector when it has a width.
    #[test]
    fn a_search_table_with_a_width_writes_its_vector() {
        assert_eq!(
            classify("CREATE VIRTUAL TABLE s USING inillucent_search(body, dims=2)"),
            Kind::Search { has_vectors: true }
        );
        assert_eq!(
            classify("CREATE VIRTUAL TABLE s USING inillucent_search(body)"),
            Kind::Search { has_vectors: false }
        );
    }

    /// Commas inside quotes and brackets do not split an argument.
    #[test]
    fn arguments_split_at_top_level_commas_only() {
        assert_eq!(
            split_arguments("a, tokenize='unicode61 x,y', b"),
            vec!["a", "tokenize='unicode61 x,y'", "b"]
        );
    }

    /// Other modules keep the reference's form, and their private tables stay in the dump.
    #[test]
    fn other_modules_are_left_alone() {
        assert_eq!(
            classify("CREATE VIRTUAL TABLE r USING rtree(id, a, b)"),
            Kind::Other
        );
        let tables = vec![
            (
                "r".to_string(),
                "CREATE VIRTUAL TABLE r USING rtree(id, a, b)".to_string(),
            ),
            (
                "f".to_string(),
                "CREATE VIRTUAL TABLE f USING fts5(a)".to_string(),
            ),
        ];
        let left_out = private_tables_left_out(&tables);
        assert!(left_out.contains("f_data"));
        assert!(!left_out.contains("r_node"));
    }
}
