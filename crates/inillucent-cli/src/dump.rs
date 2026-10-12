//! `.dump`: the SQL that would rebuild the database.
//!
//! Invariant: what comes out reproduces what went in, and it says so at the
//! top. A dump is wrapped in a transaction and begins with
//! `PRAGMA foreign_keys=OFF`, because the rows come out in `sqlite_master`
//! order rather than in dependency order and a foreign key would refuse a child
//! whose parent is three statements later. That is SQLite's dump format, and a
//! dump that could not be fed back in is not a dump.

use inillucent_value::Value;

use crate::dump_virtual::{self, Kind};
use crate::render::{literal, quote_literal};
use crate::shell::Shell;

/// What `.dump` was asked to leave out or to limit itself to.
#[derive(Default)]
pub struct DumpOptions {
    /// `LIKE` patterns naming the objects to dump. Empty means all of them.
    pub patterns: Vec<String>,
    /// `--data-only`: the rows, with no schema, no transaction and no indexes.
    pub data_only: bool,
    /// `--nosys`: no table whose name begins with `sqlite_`.
    pub nosys: bool,
}

/// Writes the whole database, or the objects the options name, as SQL.
///
/// The order and the special cases are the reference's, because a dump is
/// compared to the reference's byte for byte: tables first with their rows,
/// `sqlite_sequence` last among them, then the indexes, triggers and views in
/// the order they were created.
///
/// @param shell - where the lines go
/// @param options - the patterns and flags `.dump` was given
pub fn dump(shell: &mut Shell, options: &DumpOptions) {
    let pattern = like_clause(&options.patterns);
    let tables: Vec<(String, String)> = table_objects(shell, pattern.as_deref())
        .into_iter()
        .filter(|(name, _)| !(options.nosys && is_reserved(name)))
        .collect();
    // The warning goes above everything, including the `PRAGMA`, because a
    // virtual table is restored by writing `sqlite_schema` directly and a
    // defensive connection refuses that. The reference prints it first and so
    // does this. FTS5, `inillucent_search` and vector index tables are written as
    // statements and need no such write; see `dump_virtual`.
    if tables.iter().any(|(_, sql)| is_written_to_the_schema(sql)) {
        shell.say("/* WARNING: Script requires that SQLITE_DBCONFIG_DEFENSIVE be disabled */");
    }
    if !options.data_only {
        shell.say("PRAGMA foreign_keys=OFF;");
        shell.say("BEGIN TRANSACTION;");
    }
    let mut writable = false;
    let left_out = dump_virtual::private_tables_left_out(&tables);
    for (name, sql) in &tables {
        if left_out.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        let kind = virtual_kind(sql);
        if kind != Kind::Other {
            dump_virtual::write_table(shell, name, sql, &kind, options.data_only);
            continue;
        }
        say_definition(shell, name, sql, &mut writable, options.data_only);
        // A virtual table's rows live in its shadow tables, which are dumped as
        // the ordinary tables they are. Dumping them here as well would insert
        // every row a second time, through a module that is about to rebuild
        // them from the shadows.
        if sql.starts_with("CREATE VIRTUAL TABLE") {
            continue;
        }
        write_rows(shell, name, sql);
    }
    write_indexes_over_tables(shell, &tables, options.data_only);
    if !options.data_only {
        for sql in other_objects(shell, pattern.as_deref()) {
            shell.say(&format!("{sql};"));
        }
    }
    if writable {
        shell.say("PRAGMA writable_schema=OFF;");
    }
    if !options.data_only {
        shell.say("COMMIT;");
    }
}

/// Returns what a table's `CREATE` says it is, for a virtual table, and `Other` for the rest.
///
/// @param sql - the `CREATE` text the schema holds
fn virtual_kind(sql: &str) -> Kind {
    if sql.starts_with("CREATE VIRTUAL TABLE") {
        return dump_virtual::classify(sql);
    }
    Kind::Other
}

/// Reports whether a table is restored by writing `sqlite_schema` directly.
///
/// @param sql - the `CREATE` text the schema holds
fn is_written_to_the_schema(sql: &str) -> bool {
    sql.starts_with("CREATE VIRTUAL TABLE") && virtual_kind(sql) == Kind::Other
}

/// Writes what has to come after every table: the vector indexes and the FTS5 rebuilds.
///
/// A vector index is built from the rows of the table it indexes, and an FTS5 table that reads
/// its text from another table is built from that table's rows, so both wait for every row.
///
/// @param shell - where the lines go
/// @param tables - every table the dump covers, with its `CREATE` text
/// @param data_only - whether `--data-only` was given, which writes no index
fn write_indexes_over_tables(shell: &mut Shell, tables: &[(String, String)], data_only: bool) {
    for statement in dump_virtual::rebuild_statements(tables) {
        shell.say(&statement);
    }
    if data_only {
        return;
    }
    for (name, sql) in tables {
        if let Some(statement) = dump_virtual::index_statement(name, &virtual_kind(sql)) {
            shell.say(&statement);
        }
    }
}

/// Returns the SQL condition that selects the objects the patterns name.
///
/// **A pattern matches the object's own name,** and also the shadow tables of a
/// virtual table the pattern names, which are the tables whose names begin with
/// the virtual table's name and an underscore. So `.dump docs` writes the
/// `docs` table and its shadow tables, and does not write the indexes on
/// `docs`, because an index has a name of its own. That is the reference's
/// rule. Several patterns are alternatives. `None` means every object. The
/// objects are read from `sqlite_master AS o`.
///
/// @param patterns - the `LIKE` patterns `.dump` was given
fn like_clause(patterns: &[String]) -> Option<String> {
    if patterns.is_empty() {
        return None;
    }
    let alternatives: Vec<String> = patterns
        .iter()
        .map(|pattern| {
            let quoted = format!("'{}'", pattern.replace('\'', "''"));
            format!(
                "o.name LIKE {quoted} ESCAPE '\\' OR EXISTS (SELECT 1 FROM sqlite_master WHERE name LIKE {quoted} ESCAPE '\\' AND sql LIKE 'CREATE VIRTUAL TABLE%' AND substr(o.name, 1, length(name) + 1) = (name || '_'))"
            )
        })
        .collect();
    Some(alternatives.join(" OR "))
}

/// Reports whether a table name is one SQLite reserves for itself.
///
/// @param name - the table's name
fn is_reserved(name: &str) -> bool {
    name.get(..7)
        .is_some_and(|start| start.eq_ignore_ascii_case("sqlite_"))
}

/// Returns the tables a dump covers, `sqlite_sequence` last.
///
/// **The reserved names are in, not out.** `sqlite_sequence` carries the
/// AUTOINCREMENT high-water mark and `sqlite_stat1` carries the measurements: a
/// dump that dropped them rebuilds a database that reuses deleted keys and
/// plans every join by guesswork. The reference carries both, each in its own
/// form, and so does this.
///
/// @param shell - the shell to ask
/// @param condition - the condition `like_clause` built from `.dump ?OBJECTS?`, if any
fn table_objects(shell: &Shell, condition: Option<&str>) -> Vec<(String, String)> {
    let mut sql = String::from(
        "SELECT name, sql FROM sqlite_master AS o WHERE type = 'table' AND sql IS NOT NULL",
    );
    if let Some(condition) = condition {
        sql.push_str(&format!(" AND ({condition})"));
    }
    sql.push_str(" ORDER BY tbl_name = 'sqlite_sequence', rowid");
    let Ok((_, rows)) = shell.collect(&sql) else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| (plain(row.first()), plain(row.get(1))))
        .collect()
}

/// Returns the indexes, triggers and views a dump covers, in creation order.
///
/// By type descending and then by creation, which is views, then triggers,
/// then indexes - the order the reference emits and a replayable one, since a
/// trigger may name a view.
///
/// @param shell - the shell to ask
/// @param condition - the condition `like_clause` built from `.dump ?OBJECTS?`, if any
fn other_objects(shell: &Shell, condition: Option<&str>) -> Vec<String> {
    let mut sql = String::from(
        "SELECT sql FROM sqlite_master AS o \
         WHERE sql IS NOT NULL AND type IN ('index', 'trigger', 'view')",
    );
    if let Some(condition) = condition {
        sql.push_str(&format!(" AND ({condition})"));
    }
    sql.push_str(" ORDER BY type DESC, rowid");
    let Ok((_, rows)) = shell.collect(&sql) else {
        return Vec::new();
    };
    rows.iter().map(|row| plain(row.first())).collect()
}

/// Returns a value as plain text.
fn plain(value: Option<&Value<'static>>) -> String {
    match value {
        Some(Value::Text(text)) => String::from_utf8_lossy(text.raw()).into_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => literal(other),
    }
}

/// Writes one table's definition, in the form the reference writes it.
///
/// Four forms, and which one a table takes is what makes a dump replayable:
///
/// - a statistics table is not recreated at all, because `ANALYZE` recreates
///   it; the line that stands in for it is `ANALYZE sqlite_schema`, which is
///   what makes the target read the rows that follow;
/// - any other reserved name is recreated with `IF NOT EXISTS` over a writable
///   schema, because the target already has one and the rows have to land in
///   the copy it has;
/// - a virtual table is a row written into `sqlite_schema` rather than a
///   `CREATE`, because running its `CREATE VIRTUAL TABLE` would build empty
///   shadow tables over the ones the dump is about to restore;
/// - and an ordinary table is its own text, with `IF NOT EXISTS` inserted when
///   its name was quoted - which is the reference's rule, and is what covers
///   the shadow tables a module names with quotes.
///
/// @param shell - where the lines go
/// @param name - the table's name
/// @param sql - the `CREATE` text the schema holds
/// @param writable - whether `PRAGMA writable_schema=ON` has been written yet
/// @param data_only - whether `--data-only` was given, which writes no definition
fn say_definition(shell: &mut Shell, name: &str, sql: &str, writable: &mut bool, data_only: bool) {
    /// What `CREATE TABLE ` occupies, which is what the rewrite skips.
    const CREATE_TABLE: usize = 13;

    if is_statistics_table(name) {
        if !data_only {
            shell.say("ANALYZE sqlite_schema;");
        }
        return;
    }
    if data_only && !is_reserved(name) {
        return;
    }
    if name.len() >= 7 && name[..7].eq_ignore_ascii_case("sqlite_") {
        make_writable(shell, writable);
        if let Some(rest) = sql.get(CREATE_TABLE..) {
            shell.say(&format!("CREATE TABLE IF NOT EXISTS {rest};"));
        }
        if name.eq_ignore_ascii_case("sqlite_sequence") {
            shell.say("DELETE FROM sqlite_sequence;");
        }
        return;
    }
    if sql.starts_with("CREATE VIRTUAL TABLE") {
        make_writable(shell, writable);
        let escaped_name = name.replace('\'', "''");
        let escaped_sql = sql.replace('\'', "''");
        shell.say(&format!(
            "INSERT INTO sqlite_schema(type,name,tbl_name,rootpage,sql)VALUES('table','{escaped_name}','{escaped_name}',0,'{escaped_sql}');"
        ));
        return;
    }
    let quoted_name = sql.len() > CREATE_TABLE
        && sql[..CREATE_TABLE].eq_ignore_ascii_case("CREATE TABLE ")
        && sql[CREATE_TABLE..].starts_with(['"', '\'']);
    if quoted_name {
        shell.say(&format!(
            "CREATE TABLE IF NOT EXISTS {};",
            &sql[CREATE_TABLE..]
        ));
        return;
    }
    shell.say(&format!("{sql};"));
}

/// Writes `PRAGMA writable_schema=ON` the first time it is needed.
///
/// @param shell - where the line goes
/// @param writable - whether it has already been written
fn make_writable(shell: &mut Shell, writable: &mut bool) {
    if *writable {
        return;
    }
    shell.say("PRAGMA writable_schema=ON;");
    *writable = true;
}

/// Reports whether a name is one of the statistics tables.
///
/// `sqlite_stat1` and `sqlite_stat4`, matched the way the reference matches
/// them: the prefix and exactly one more character.
///
/// @param name - the table's name
fn is_statistics_table(name: &str) -> bool {
    name.len() == 12 && name[..11].eq_ignore_ascii_case("sqlite_stat")
}

/// Returns the columns a dump writes values for, in declaration order.
///
/// **`PRAGMA table_info`, not `SELECT *`.** A generated column is in `*` and is
/// not in `table_info`, which is the distinction that matters here: its value is
/// derived on read, so writing it back is writing a column the target refuses to
/// be given. `.dump` of a table with two generated columns emitted four values
/// per row and the reference answered
/// `table t has 3 columns but 6 values were supplied` - a dump that only this
/// engine could replay.
///
/// Empty when the pragma cannot be read, which the caller turns back into
/// `SELECT *` rather than dumping nothing.
///
/// **A column whose name is empty is a column** (task-2066 §4.1.3). This used
/// to drop it, so `CREATE TABLE d4 ("" TEXT, b INT)` holding one row dumped as
/// `INSERT INTO d4 VALUES(5)` - one value for two columns. Replaying that puts
/// `5` in the first column and leaves the second null, which is worse than
/// losing the row: the restored database holds different data and nothing says
/// so. `""` is a legal column name and the pinned SQLite 3.53.4 dumps both of
/// its values.
///
/// @param shell - the shell to ask
/// @param table - the table's name
fn stored_columns(shell: &mut Shell, table: &str) -> Vec<String> {
    let literal = format!("'{}'", table.replace('\'', "''"));
    let Ok((_, rows)) = shell.collect(&format!("SELECT name FROM pragma_table_info({literal})"))
    else {
        return Vec::new();
    };
    rows.iter().map(|row| plain(row.first())).collect()
}

/// Writes every row of one table as an `INSERT`.
///
/// **The projection quotes every name; the emitted `INSERT` quotes none**
/// (task-2066 §4.1.3). The two are different jobs and used to share one rule.
/// `quote_identifier` exists so the *emitted* text reads the way the reference
/// writes it - a bare word is left bare - and applying that to the `SELECT`
/// this reads the rows with produced `SELECT select,b FROM d1` for
/// `CREATE TABLE d1 ("select" TEXT, b INT)`. That is a syntax error,
/// `shell.collect` answered `Err`, and this returned having written no `INSERT`
/// at all: the table's `CREATE` in the dump, its rows gone, exit 0. A column
/// named `"order"` and a table named `"select"` did the same.
///
/// Losing rows at exit 0 is the worst shape a backup tool can have, so the
/// failure is no longer silent either: a table whose rows cannot be read
/// complains, which sets the shell's failure flag and makes the verb exit
/// non-zero.
///
/// @param shell - where the lines go
/// @param table - the table to write
/// @param sql - the `CREATE` text the schema holds, which decides how the
///   emitted name is spelled
fn write_rows(shell: &mut Shell, table: &str, sql: &str) {
    let quoted = emitted_name(table, sql);
    let columns = stored_columns(shell, table);
    // A plain `VALUES` list with no column names, which is what the reference
    // writes: an unnamed insert maps positionally onto the columns that are not
    // generated, so the two agree without naming anything.
    let projection = if columns.is_empty() {
        "*".to_string()
    } else {
        columns
            .iter()
            .map(|name| always_quoted(name))
            .collect::<Vec<String>>()
            .join(",")
    };
    // The table name is quoted here too, for the same reason and independently
    // of how it is emitted: `SELECT * FROM select` does not parse either.
    let reading = format!("SELECT {projection} FROM {}", always_quoted(table));
    let rows = match shell.collect(&reading) {
        Ok((_, rows)) => rows,
        Err(failure) => {
            // **The engine's error is kept, so the command line reports the
            // cause.** A dump of a damaged file failed with the status for a
            // bad statement because only the complaint below was recorded, and
            // a complaint has no engine error behind it. With the error kept,
            // a damaged page is `corrupt` and a failed read is `io`.
            if shell.first_error.is_none() {
                shell.first_error = failure.error.clone();
            }
            shell.complain(&format!(
                "-- the rows of {table} could not be read, so none are in this dump"
            ));
            return;
        }
    };
    for row in rows {
        let values: Vec<String> = row.iter().map(quote_literal).collect();
        shell.say(&format!(
            "INSERT INTO {quoted} VALUES({});",
            values.join(",")
        ));
    }
}

/// Returns the table name as the emitted `INSERT` spells it.
///
/// **The reference's rule is "however the `CREATE` spelled it", not "quote it
/// when it needs quoting".** Asked the same schema, the pinned SQLite 3.53.4
/// writes `INSERT INTO d1 VALUES('x',1)` for a table whose column is named
/// `"select"`, and `INSERT INTO "select" VALUES('v')` for a table *named*
/// `select` - because that `CREATE` carried the quotes. `quote_identifier`
/// alone answers the first correctly and the second wrongly, since `select` is
/// a bare word by its rule. `interchange.rs` compares this output with the
/// reference's byte for byte, so the difference is a failure rather than a
/// preference.
///
/// This is the same signal `say_definition` reads to decide whether to write
/// `IF NOT EXISTS`, so the `CREATE` and the `INSERT` agree by construction.
///
/// @param table - the table's name
/// @param sql - the `CREATE` text the schema holds
fn emitted_name(table: &str, sql: &str) -> String {
    /// What `CREATE TABLE ` occupies.
    const CREATE_TABLE: usize = 13;

    let quoted_in_the_schema = sql.len() > CREATE_TABLE
        && sql
            .get(..CREATE_TABLE)
            .is_some_and(|head| head.eq_ignore_ascii_case("CREATE TABLE "))
        && sql
            .get(CREATE_TABLE..)
            .is_some_and(|rest| rest.starts_with(['"', '\'']));
    if quoted_in_the_schema {
        return always_quoted(table);
    }
    quote_identifier(table)
}

/// Returns an identifier quoted, always.
///
/// For SQL this module *runs* rather than SQL it writes out. Nothing reads it,
/// so there is no reason to leave a name bare and every reason not to: a
/// reserved word is a bare word by `quote_identifier`'s rule, and leaving it
/// bare is what made a dump lose every row of a table with a column named
/// `"select"` (task-2066 §4.1.3).
///
/// @param name - the identifier
fn always_quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Returns an identifier, quoted only when it has to be.
///
/// For the SQL this module **writes out**. A dump is read by a person as often
/// as by a program, and quoting every name makes it noisier than the schema it
/// came from. The rule is the usual one: a name that is a bare word needs
/// nothing - and it is the reference's rule, asked of the pinned SQLite 3.53.4
/// on the same schema, which emits `INSERT INTO d1 VALUES('x',1)` for a table
/// with a column named `"select"` and quotes the table name only where the
/// `CREATE` quoted it. `interchange.rs` compares the two byte for byte, so this
/// is a contract rather than a preference.
///
/// @param name - the identifier
fn quote_identifier(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare word is left alone and anything else is quoted, with a quote
    /// inside it doubled rather than dropped.
    #[test]
    fn an_identifier_is_quoted_only_when_it_has_to_be() {
        assert_eq!(quote_identifier("t"), "t");
        assert_eq!(quote_identifier("with_underscore"), "with_underscore");
        assert_eq!(quote_identifier("has space"), "\"has space\"");
        assert_eq!(quote_identifier("1leading"), "\"1leading\"");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    /// The name a `SELECT` is built from is always quoted.
    ///
    /// **Including a reserved word, which is the whole point** (task-2066
    /// §4.1.3): `quote_identifier` leaves `select` bare because it is a bare
    /// word, and `SELECT select,b FROM d1` does not parse - so a table with a
    /// column of that name dumped with no rows and exit 0.
    #[test]
    fn a_name_a_query_is_built_from_is_always_quoted() {
        assert_eq!(always_quoted("t"), "\"t\"");
        assert_eq!(always_quoted("select"), "\"select\"");
        assert_eq!(always_quoted("order"), "\"order\"");
        assert_eq!(always_quoted(""), "\"\"");
        assert_eq!(always_quoted("has space"), "\"has space\"");
        assert_eq!(always_quoted("a\"b"), "\"a\"\"b\"");
    }

    /// The emitted name follows the schema's spelling, not a quoting rule.
    ///
    /// The reference's own behaviour, measured against the pinned SQLite
    /// 3.53.4: a bare `CREATE TABLE d1` gives `INSERT INTO d1`, and a quoted
    /// `CREATE TABLE "select"` gives `INSERT INTO "select"` - even though
    /// `select` is a bare word.
    #[test]
    fn the_emitted_name_is_spelled_the_way_the_schema_spells_it() {
        assert_eq!(
            emitted_name("d1", "CREATE TABLE d1 (\"select\" TEXT, b INT)"),
            "d1"
        );
        assert_eq!(
            emitted_name("select", "CREATE TABLE \"select\"(x TEXT)"),
            "\"select\""
        );
        assert_eq!(
            emitted_name("has space", "CREATE TABLE \"has space\"(\"a b\" TEXT)"),
            "\"has space\""
        );
    }

    /// And the two answer differently for exactly the case that mattered.
    ///
    /// A test that pinned only the values above would still pass if somebody
    /// made `always_quoted` an alias of `quote_identifier`, which is the one
    /// change that would bring the defect back.
    #[test]
    fn the_two_quoting_rules_differ_on_a_reserved_word() {
        assert_ne!(always_quoted("select"), quote_identifier("select"));
        assert_eq!(quote_identifier("select"), "select");
    }
}
