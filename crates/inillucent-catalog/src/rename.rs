//! Rewriting the stored `CREATE` text when a name changes.
//!
//! Invariant: a rewrite replaces *tokens*, never bytes matched by search. The
//! text a schema row holds is the text a person wrote, comments and all, and
//! `ALTER TABLE` has to give it back changed in exactly one respect. A textual
//! substitution would rewrite the inside of a string literal, the middle of a
//! longer identifier and the word in a comment, and every one of those produces
//! a schema that still parses and means something else.
//!
//! The second invariant is that a rewrite is checked before it is kept. The
//! result is re-parsed, and an `ALTER` whose rewrite does not parse is refused
//! whole rather than written - which is what stops a rename leaving a database
//! whose schema cannot be loaded.

use inillucent_base::limits::Limits;
use inillucent_base::{error, DbResult};
use inillucent_sql::ast::Statement;
use inillucent_sql::lexer::{Lexer, Span, TokenKind};
use inillucent_sql::parser::parse_next_statement;

use crate::paged::ObjectKind;
pub use crate::rename_column::KnownTable;

/// Which kinds of place a table rename rewrites.
///
/// `PRAGMA legacy_alter_table = ON` makes SQLite leave the bodies of views and
/// triggers alone, and rewrite the `REFERENCES` clauses of other tables only
/// while `PRAGMA foreign_keys` is on. The name a statement is about (`CREATE
/// TABLE name`, the `ON name` of an index or trigger) is always rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableRewrite {
    /// Rewrite the `REFERENCES name` clauses.
    pub references: bool,
    /// Rewrite the tables a view or a trigger body reads and writes.
    pub bodies: bool,
}

impl TableRewrite {
    /// Every kind of place, which is SQLite's behaviour with the legacy setting off.
    pub const EVERYWHERE: TableRewrite = TableRewrite {
        references: true,
        bodies: true,
    };
}

/// Returns the stored SQL with a table's name replaced where `which` allows.
///
/// **A table is written quoted.** SQLite's `ALTER TABLE` substitutes `"%w"` for
/// a new *table* name unconditionally, so `RENAME TO people` leaves `CREATE
/// TABLE "people" (...)`. The stored text is compared byte for byte against
/// SQLite's, so the quoting is copied rather than tidied away.
///
/// @param sql - the stored `CREATE` text
/// @param from - the table's current name
/// @param to - the new name
/// @param which - the kinds of place to rewrite
pub fn rewrite_table(sql: &[u8], from: &[u8], to: &[u8], which: TableRewrite) -> DbResult<Vec<u8>> {
    use crate::rename_table::Site;
    let sites = crate::rename_table::table_sites(sql, &from.to_ascii_lowercase())?;
    let edits: Vec<Span> = sites
        .iter()
        .filter(|found| match found.site {
            Site::Head => true,
            Site::Reference => which.references,
            Site::Body => which.bodies,
        })
        .map(|found| found.span)
        .collect();
    Ok(splice(sql, &edits, &quoted(to)))
}

/// What a column rename needs to know about the column and its surroundings.
#[derive(Clone, Copy, Debug)]
pub struct ColumnRename<'a> {
    /// The folded name of the table that owns the column.
    pub table: &'a [u8],
    /// The folded name of the column.
    pub from: &'a [u8],
    /// The new name, as written and unquoted.
    pub to: &'a [u8],
    /// Whether the new name was written quoted in the `ALTER TABLE` statement.
    pub to_quoted: bool,
    /// The tables a view or a trigger body can read, for resolving a column.
    pub known: &'a [KnownTable],
}

/// Returns the stored SQL of one schema row with a column renamed.
///
/// **The new name is quoted when either the statement or the old occurrence was
/// quoted.** `RENAME COLUMN b TO beta` turns a bare `b` into a bare `beta` and a
/// `[b]`, `` `b` `` or `"b"` into `"beta"`, and `RENAME COLUMN b TO "beta"`
/// quotes every occurrence. The stored text is compared byte for byte against
/// SQLite's, so the rule is copied rather than tidied.
///
/// @param sql - the stored `CREATE` text
/// @param kind - what kind of object the row is
/// @param owner - the folded name of the table the row belongs to
/// @param itself - whether the row is the `CREATE TABLE` of the renamed column's table
/// @param ren - the rename
pub fn rewrite_column(
    sql: &[u8],
    kind: ObjectKind,
    owner: &[u8],
    itself: bool,
    ren: &ColumnRename<'_>,
) -> DbResult<Vec<u8>> {
    use crate::rename_column::{
        own_text_edits, reference_edits, trigger_edits, view_edits, Target,
    };
    let target = Target {
        table: ren.table,
        column: ren.from,
    };
    let edits = match kind {
        ObjectKind::Table if itself => own_text_edits(sql, target)?,
        ObjectKind::Table => reference_edits(sql, target)?,
        ObjectKind::Index if owner == ren.table => own_text_edits(sql, target)?,
        ObjectKind::Index => Vec::new(),
        ObjectKind::View => view_edits(sql, target, ren.known)?,
        ObjectKind::Trigger => trigger_edits(sql, owner, target, ren.known)?,
    };
    Ok(splice_each(sql, &edits, |span| {
        let was_quoted = span
            .slice(sql)
            .first()
            .is_some_and(|byte| matches!(byte, b'"' | b'[' | b'`' | b'\''));
        match ren.to_quoted || was_quoted {
            true => quoted(ren.to),
            false => ren.to.to_vec(),
        }
    }))
}

/// Applies each edit with a replacement chosen for that edit.
///
/// @param source - the text
/// @param edits - the spans to replace, in order
/// @param replacement - chooses the text for one span
fn splice_each(source: &[u8], edits: &[Span], replacement: impl Fn(Span) -> Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut cursor = 0usize;
    for span in edits {
        let start = span.start as usize;
        let end = span.end as usize;
        if start < cursor || end > source.len() {
            continue;
        }
        out.extend_from_slice(source.get(cursor..start).unwrap_or(&[]));
        out.extend_from_slice(&replacement(*span));
        cursor = end;
    }
    out.extend_from_slice(source.get(cursor..).unwrap_or(&[]));
    out
}

/// Returns the lowercase text of a bare word token, keywords included.
///
/// @param sql - the text the token came from
/// @param token - the token
pub(crate) fn bare_word(sql: &[u8], token: inillucent_sql::lexer::Token) -> Option<Vec<u8>> {
    match token.kind {
        TokenKind::Identifier {
            quote: inillucent_sql::lexer::QuoteForm::Bare,
            ..
        } => Some(token.span.slice(sql).to_ascii_lowercase()),
        _ => None,
    }
}

/// Applies the edits to the source, replacing each span with the replacement.
fn splice(source: &[u8], edits: &[Span], replacement: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut cursor = 0usize;
    for span in edits {
        let start = span.start as usize;
        let end = span.end as usize;
        if start < cursor || end > source.len() {
            continue;
        }
        out.extend_from_slice(source.get(cursor..start).unwrap_or(&[]));
        out.extend_from_slice(replacement);
        cursor = end;
    }
    out.extend_from_slice(source.get(cursor..).unwrap_or(&[]));
    out
}

/// Returns an identifier's text with its quoting removed.
pub(crate) fn unquoted(raw: &[u8]) -> Vec<u8> {
    let Some(first) = raw.first().copied() else {
        return Vec::new();
    };
    let closing = match first {
        b'"' => b'"',
        b'`' => b'`',
        b'[' => b']',
        _ => return raw.to_vec(),
    };
    let inner = raw
        .get(1..raw.len().saturating_sub(1))
        .unwrap_or(&[])
        .to_vec();
    if closing == b']' {
        return inner;
    }
    // A doubled quote inside the identifier is one character.
    let mut out = Vec::with_capacity(inner.len());
    let mut index = 0usize;
    while index < inner.len() {
        let byte = inner.get(index).copied().unwrap_or(0);
        out.push(byte);
        index = index.saturating_add(1);
        if byte == closing && inner.get(index).copied() == Some(closing) {
            index = index.saturating_add(1);
        }
    }
    out
}

/// Returns a slice with its leading and trailing ASCII whitespace removed.
///
/// @param bytes - the slice
fn trimmed(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start = start.saturating_add(1);
    }
    while end > start
        && bytes
            .get(end.saturating_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        end = end.saturating_sub(1);
    }
    bytes.get(start..end).unwrap_or(&[])
}

/// Returns a name written as a quoted identifier.
///
/// **Always quoted, even when the name would parse bare.** SQLite's own
/// `ALTER TABLE` substitutes `"%w"` for the new name without asking whether it
/// needs the quotes, so `ALTER TABLE t RENAME TO people` leaves
/// `CREATE TABLE "people" (...)` in `sqlite_schema`. Quoting only when the name
/// demands it produces text that means the same thing and is not the same
/// bytes - and the stored `CREATE` text is compared byte for byte against
/// SQLite's, because it is what a reader re-parses to learn what the table is.
///
/// The quoting rules the old version applied are still what makes the *escape*
/// correct: a keyword, a space, a leading digit or an embedded quote all have
/// to survive the re-parse, and doubling an interior `"` is what does it.
///
/// @param name - the name to write
pub fn quoted(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len().saturating_add(2));
    out.push(b'"');
    for byte in name {
        if *byte == b'"' {
            out.push(b'"');
        }
        out.push(*byte);
    }
    out.push(b'"');
    out
}

/// Checks that a rewritten statement still parses, returning it unchanged.
///
/// The check is the whole reason a rewrite is safe to keep. An `ALTER` that
/// produced text the parser cannot read would leave a database whose schema
/// fails to load, and that is not recoverable from inside the engine.
///
/// These failures stay `Corrupt`, unlike the stored-schema failures, which
/// were moved off `Corrupt` in `load.rs`. The distinction is whose text it
/// is: a stored `CREATE TABLE` is text SQLite wrote and the caller can read,
/// so a parse failure there is a gap in this engine's grammar and says so.
/// Text *this* engine just generated and cannot read back is an internal
/// defect nothing the caller wrote can cause, and filing it under the
/// caller's typos would hide it.
pub fn reparsed(sql: Vec<u8>) -> DbResult<Vec<u8>> {
    let limits = Limits::default();
    let parsed = parse_next_statement(&sql, 0, &limits).map_err(|reason| {
        error::corrupt(format!(
            "the rewritten schema does not parse: {}",
            reason.message()
        ))
    })?;
    let sane = matches!(
        parsed.statement,
        Statement::CreateTable { .. }
            | Statement::CreateIndex { .. }
            | Statement::CreateView { .. }
            | Statement::CreateTrigger { .. }
    );
    if !sane {
        return Err(error::corrupt("the rewritten schema is not a CREATE"));
    }
    Ok(sql)
}

/// Returns the `CREATE TABLE` text with one column definition appended.
///
/// The column goes before the closing parenthesis of the column list, which is
/// the last one that closes the list rather than the last one in the text: a
/// table with a `CHECK (a > 0)` has parentheses after it.
pub fn add_column(sql: &[u8], definition: &[u8]) -> DbResult<Vec<u8>> {
    let Some(at) = column_list_end(sql) else {
        return Err(error::corrupt("a CREATE TABLE with no column list"));
    };
    // **Trimmed, because the separator is written here.** The definition is a
    // slice of the statement's own source and the span the binder recorded
    // starts at the whitespace after `ADD COLUMN`, so appending it after a
    // literal `", "` left `..., INTEGER,  joined TEXT` - two spaces where
    // SQLite writes one, and the stored text is compared byte for byte.
    let definition = trimmed(definition);
    let mut out = Vec::with_capacity(sql.len().saturating_add(definition.len()).saturating_add(2));
    out.extend_from_slice(sql.get(..at).unwrap_or(&[]));
    out.extend_from_slice(b", ");
    out.extend_from_slice(definition);
    out.extend_from_slice(sql.get(at..).unwrap_or(&[]));
    Ok(out)
}

/// Returns the `CREATE TABLE` text with one new column placed before the table constraints.
///
/// SQLite writes the column in front of the comma that starts the first table
/// constraint (`a TEXT, q TEXT, UNIQUE (a)`). Appending it before the closing
/// parenthesis instead puts a column definition after a constraint, which
/// SQLite cannot parse, so a file written that way could not be opened by it.
///
/// @param sql - the stored CREATE TABLE text
/// @param definition - the new column definition as written
pub fn add_column_definition(sql: &[u8], definition: &[u8]) -> DbResult<Vec<u8>> {
    let Some(at) = constraint_comma(sql) else {
        return add_column(sql, definition);
    };
    let definition = trimmed(definition);
    let mut out = Vec::with_capacity(sql.len().saturating_add(definition.len()).saturating_add(2));
    out.extend_from_slice(sql.get(..at).unwrap_or(&[]));
    out.extend_from_slice(b", ");
    out.extend_from_slice(definition);
    out.extend_from_slice(sql.get(at..).unwrap_or(&[]));
    Ok(out)
}

/// Returns the offset of the comma that starts the first table constraint, if any.
///
/// @param sql - the stored CREATE TABLE text
fn constraint_comma(sql: &[u8]) -> Option<usize> {
    let limits = Limits::default();
    let parsed = parse_next_statement(sql, 0, &limits).ok()?;
    let Statement::CreateTable {
        body:
            inillucent_sql::ast::CreateTableBody::Columns {
                columns,
                constraints,
                ..
            },
        ..
    } = &parsed.statement
    else {
        return None;
    };
    if constraints.is_empty() {
        return None;
    }
    let after = columns.last()?.span.end as usize;
    let mut lexer = Lexer::at(sql, after);
    loop {
        let token = lexer.next_token().ok()?;
        match token.kind {
            TokenKind::EndOfInput => return None,
            TokenKind::Punctuator(inillucent_sql::lexer::Punctuator::Comma) => {
                return Some(token.span.start as usize)
            }
            _ => {}
        }
    }
}

/// Returns the offset of the parenthesis that closes the column list.
fn column_list_end(sql: &[u8]) -> Option<usize> {
    let mut lexer = Lexer::at(sql, 0);
    let mut depth = 0usize;
    let mut opened = false;
    loop {
        let token = lexer.next_token().ok()?;
        match token.kind {
            TokenKind::EndOfInput => return None,
            TokenKind::Punctuator(inillucent_sql::lexer::Punctuator::LeftParen) => {
                depth = depth.saturating_add(1);
                opened = true;
            }
            TokenKind::Punctuator(inillucent_sql::lexer::Punctuator::RightParen) => {
                depth = depth.saturating_sub(1);
                if opened && depth == 0 {
                    return Some(token.span.start as usize);
                }
            }
            _ => {}
        }
    }
}

/// Returns the tables a stored `CREATE` text names, folded.
///
/// A view or trigger that reads one table can have a column renamed inside it
/// unambiguously; one that reads two cannot, because a bare column name in it
/// might belong to either. Knowing which case a row is in is the difference
/// between rewriting it correctly and rewriting the wrong table's column.
pub fn referenced_tables(sql: &[u8]) -> Vec<Vec<u8>> {
    let mut names: Vec<Vec<u8>> = Vec::new();
    let mut lexer = Lexer::at(sql, 0);
    let mut previous: Option<Vec<u8>> = None;
    loop {
        let Ok(token) = lexer.next_token() else {
            return names;
        };
        if token.kind == TokenKind::EndOfInput {
            return names;
        }
        let raw = token.span.slice(sql);
        if let TokenKind::Identifier { keyword: None, .. } = token.kind {
            let names_a_table = matches!(
                previous.as_deref(),
                Some(b"on") | Some(b"from") | Some(b"join") | Some(b"into") | Some(b"update")
            );
            if names_a_table {
                let folded = unquoted(raw).to_ascii_lowercase();
                if !names.contains(&folded) {
                    names.push(folded);
                }
            }
        }
        previous = Some(raw.to_ascii_lowercase());
    }
}

/// Returns the `CREATE TABLE` text with one column definition removed.
///
/// The definition's own span comes from re-parsing, so the cut is exactly the
/// column and its separating comma - not a byte range guessed from the name,
/// which would take the wrong half of `a INTEGER, ab TEXT`.
pub fn drop_column(sql: &[u8], position: usize) -> DbResult<Vec<u8>> {
    let limits = Limits::default();
    // The text being read here is the *stored* `CREATE TABLE`, so a failure is
    // the same fact the import path reports and is reported the same way: the
    // statement could not be parsed, which is not a claim about the disk. The
    // rewrite self-checks below stay corruption on purpose - see `reparsed`.
    let parsed = parse_next_statement(sql, 0, &limits)
        .map_err(|reason| crate::load::unparseable_schema("CREATE TABLE", reason))?;
    let Statement::CreateTable {
        body: inillucent_sql::ast::CreateTableBody::Columns { columns, .. },
        ..
    } = &parsed.statement
    else {
        return Err(crate::load::corrupt_schema(
            "the schema SQL for this table is not a CREATE TABLE",
        ));
    };
    let Some(doomed) = columns.get(position) else {
        return Err(crate::load::corrupt_schema(
            "the column to drop is not in the schema",
        ));
    };
    let start = doomed.span.start as usize;
    // **SQLite's own cut, so the stored text is the same bytes.** A column that
    // is followed by another is removed from its first byte up to the first
    // byte of the next column, which takes the comma and the space after it. The
    // last column is removed from the comma before it up to the comma or the
    // closing parenthesis that follows it, which takes the space before that.
    let (cut_start, cut_end) = match columns.get(position.saturating_add(1)) {
        Some(next) => (start, next.span.start as usize),
        None => (
            previous_comma(sql, start).unwrap_or(start),
            next_token_start(sql, doomed.span.end as usize),
        ),
    };
    let mut out = Vec::with_capacity(sql.len());
    out.extend_from_slice(sql.get(..cut_start).unwrap_or(&[]));
    out.extend_from_slice(sql.get(cut_end..).unwrap_or(&[]));
    Ok(out)
}

/// Returns the offset of the first token that starts at or after a position.
///
/// @param sql - the text
/// @param from - where to start looking
fn next_token_start(sql: &[u8], from: usize) -> usize {
    match Lexer::at(sql, from).next_token() {
        Ok(token) if token.kind != TokenKind::EndOfInput => token.span.start as usize,
        _ => from,
    }
}

/// Returns the offset of the last comma before a position.
fn previous_comma(sql: &[u8], before: usize) -> Option<usize> {
    sql.get(..before)?.iter().rposition(|byte| *byte == b',')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renaming replaces the table's own name and nothing that merely looks
    /// like it - not the text inside a string, not a longer identifier, and not
    /// a column that happens to share the name.
    #[test]
    fn a_rename_replaces_tokens_rather_than_bytes() {
        let sql = b"CREATE TABLE t (t TEXT DEFAULT 't', ts INTEGER, note TEXT DEFAULT 'about t')";
        let out = rewrite_table(sql, b"t", b"u", TableRewrite::EVERYWHERE).expect("it rewrites");
        assert_eq!(
            String::from_utf8_lossy(&out),
            // Quoted, because SQLite quotes a renamed table unconditionally.
            "CREATE TABLE \"u\" (t TEXT DEFAULT 't', ts INTEGER, note TEXT DEFAULT 'about t')"
        );
    }

    /// An index names its table after `ON`, and that is the occurrence a table
    /// rename has to change.
    #[test]
    fn an_index_follows_its_table() {
        let sql = b"CREATE INDEX t_name ON t (name)";
        let out = rewrite_table(sql, b"t", b"u", TableRewrite::EVERYWHERE).expect("it rewrites");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "CREATE INDEX t_name ON \"u\" (name)"
        );
    }

    /// A new name that is a keyword comes back quoted, because an unquoted one
    /// would parse as the keyword.
    #[test]
    fn a_keyword_name_is_quoted() {
        assert_eq!(quoted(b"order"), b"\"order\"".to_vec());
        assert_eq!(quoted(b"two words"), b"\"two words\"".to_vec());
        // Quoted even when it would have parsed bare, which is what SQLite
        // writes and what the stored text is compared against.
        assert_eq!(quoted(b"plain"), b"\"plain\"".to_vec());
        assert_eq!(quoted(b"a\"b"), b"\"a\"\"b\"".to_vec());
    }

    /// A column is added inside the list rather than after whatever closes the
    /// statement.
    #[test]
    fn a_column_is_added_inside_the_list() {
        let sql = b"CREATE TABLE t (a INTEGER, b TEXT CHECK (b <> ''))";
        let out = add_column(sql, b"c REAL").expect("it appends");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "CREATE TABLE t (a INTEGER, b TEXT CHECK (b <> ''), c REAL)"
        );
    }

    /// A column is cut with its separator, and the name is not searched for -
    /// so `a` goes and `ab` stays.
    #[test]
    fn a_dropped_column_takes_its_comma() {
        let sql = b"CREATE TABLE t (a INTEGER, ab TEXT, c REAL)";
        let out = drop_column(sql, 0).expect("it drops");
        assert_eq!(
            String::from_utf8_lossy(&out),
            "CREATE TABLE t (ab TEXT, c REAL)"
        );
        let last = drop_column(sql, 2).expect("it drops");
        assert_eq!(
            String::from_utf8_lossy(&last),
            "CREATE TABLE t (a INTEGER, ab TEXT)"
        );
    }

    /// A view that reads one table can have its columns renamed; one that
    /// reads two is ambiguous, and the caller has to know which it is.
    #[test]
    fn the_tables_a_statement_reads_are_listed() {
        assert_eq!(
            referenced_tables(b"CREATE VIEW v AS SELECT a FROM t WHERE a > 1"),
            vec![b"t".to_vec()]
        );
        assert_eq!(
            referenced_tables(b"CREATE VIEW v AS SELECT a FROM t JOIN u ON t.k = u.k"),
            vec![b"t".to_vec(), b"u".to_vec()]
        );
        assert_eq!(
            referenced_tables(b"CREATE INDEX i ON t (name)"),
            vec![b"t".to_vec()]
        );
    }

    /// A rewrite that does not parse is refused rather than kept.
    #[test]
    fn an_unparseable_rewrite_is_refused() {
        assert!(reparsed(b"CREATE TABLE".to_vec()).is_err());
        assert!(reparsed(b"SELECT 1".to_vec()).is_err());
        assert!(reparsed(b"CREATE TABLE t (a)".to_vec()).is_ok());
    }
}
