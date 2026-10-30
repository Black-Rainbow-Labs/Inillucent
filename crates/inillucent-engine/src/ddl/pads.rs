//! What a leaf written before `ALTER TABLE ... ADD COLUMN` reads as for the new column.
//!
//! Invariant: **the value a table's leaves give for a column they were written without
//! is a function of the table's declaration alone.** It is the column's `DEFAULT` when
//! that is a literal, and NULL otherwise. Nothing is stored for it. After a restart, or
//! in a process that recovers the log, the same declaration gives the same value, so a
//! row written before the column was added reads the same on every connection and at
//! every time.
//!
//! This is why `ADD COLUMN` takes the cheap path only for a default it can read without
//! running a query: the value has to be re-derived whenever the file is opened. Any
//! other default takes the rewrite that stores the value in every row, as before.
//!
//! The literal is read with the SQL parser's own tokens, and `ADD COLUMN` checks the
//! result against the value the engine computes for the same text before it relies on
//! it, so the two readings cannot drift apart without a statement falling back to the
//! rewrite.

use inillucent_base::DbResult;
use inillucent_exec::physical::SourceLayout;
use inillucent_sql::ast::{Expr, Literal, UnaryOp};
use inillucent_sql::catalog_view::{ColumnInfo, TableInfo};
use inillucent_tree::datum::OwnedDatum;

use super::alter::with_column_affinity;
use crate::*;

/// Reads a `DEFAULT` that is a literal, without running anything.
///
/// A number, a string, a blob, `NULL`, a signed number, or a bare word (which is a string
/// in a default, because a default has no column in scope). Anything else is `None`: an
/// expression, a function, `CURRENT_TIMESTAMP`, a `CAST`, `TRUE`, which SQLite reads as an
/// identifier before it reads it as a number.
///
/// @param default_sql - the default as the catalog holds it
pub(crate) fn literal_default(default_sql: &[u8]) -> Option<OwnedDatum> {
    let limits = inillucent_base::limits::Limits::default();
    let (ast, expr) = inillucent_sql::parser::parse_expression(default_sql, &limits).ok()?;
    match ast.expr(expr)? {
        Expr::Literal(literal) => literal_value(literal, false),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
        } => match ast.expr(*operand)? {
            Expr::Literal(literal) => literal_value(literal, true),
            _ => None,
        },
        Expr::Unary {
            op: UnaryOp::Identity,
            operand,
        } => match ast.expr(*operand)? {
            Expr::Literal(literal) => literal_value(literal, false),
            _ => None,
        },
        Expr::Column {
            database: None,
            table: None,
            column,
        } => Some(OwnedDatum::Text(ast.text(*column).to_vec())),
        _ => None,
    }
}

/// Returns the value of one literal, negated when a minus sign preceded it.
///
/// @param literal - what was written
/// @param negated - whether a unary minus applies to it
fn literal_value(literal: &Literal, negated: bool) -> Option<OwnedDatum> {
    match (literal, negated) {
        (Literal::Null, false) => Some(OwnedDatum::Null),
        (Literal::Integer(text), negated) => integer_value(text, negated),
        (Literal::Float(text), negated) => {
            let number: f64 = std::str::from_utf8(text).ok()?.parse().ok()?;
            Some(OwnedDatum::Real(if negated { -number } else { number }))
        }
        (Literal::String(bytes), false) => Some(OwnedDatum::Text(bytes.clone())),
        (Literal::Blob(bytes), false) => Some(OwnedDatum::Blob(bytes.clone())),
        _ => None,
    }
}

/// Returns an integer literal's value the way SQLite reads it.
///
/// Decimal digits that fit 64 bits are an integer and ones that do not are a real. A
/// hexadecimal literal is a 64 bit pattern, so `0xffffffffffffffff` is -1. The magnitude
/// 9223372036854775808 is the integer minimum only after a minus sign.
///
/// @param text - the digits as written
/// @param negated - whether a unary minus applies to them
fn integer_value(text: &[u8], negated: bool) -> Option<OwnedDatum> {
    let text = std::str::from_utf8(text).ok()?;
    if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        if digits.is_empty() || digits.len() > 16 {
            return None;
        }
        let bits = u64::from_str_radix(digits, 16).ok()?;
        let number = bits as i64;
        return Some(OwnedDatum::Int(match negated {
            true => number.wrapping_neg(),
            false => number,
        }));
    }
    if text == "9223372036854775808" && negated {
        return Some(OwnedDatum::Int(i64::MIN));
    }
    match text.parse::<i64>() {
        Ok(number) => Some(OwnedDatum::Int(match negated {
            true => -number,
            false => number,
        })),
        Err(_) => {
            let number: f64 = text.parse().ok()?;
            Some(OwnedDatum::Real(if negated { -number } else { number }))
        }
    }
}

/// Returns what a column's default is stored as, after the column's affinity.
///
/// The conversion an `INSERT` into the column does and the one `ADD COLUMN` does for the
/// rows already there. A `STRICT` table also turns an integer into a real for a `REAL`
/// column and refuses a class the column does not admit, which is `None` here.
///
/// @param column - the column
/// @param strict - whether the table is `STRICT`
/// @param raw - what the default evaluated to
pub(crate) fn stored_default(
    column: &ColumnInfo,
    strict: bool,
    raw: OwnedDatum,
) -> Option<OwnedDatum> {
    let mut value = with_column_affinity(raw, Some(column.affinity));
    // A column with REAL affinity holds a real: SQLite turns an integer into one when it
    // stores it, so `ADD COLUMN b REAL DEFAULT 5` gives `typeof(b)` of `real`. The affinity
    // function leaves the integer as it is, and a leaf written with the column converts it
    // in the column's slot, which a pad has no slot to do.
    if let (OwnedDatum::Int(whole), inillucent_value::Affinity::Real) = (&value, column.affinity) {
        value = OwnedDatum::Real(*whole as f64);
    }
    if strict {
        if let OwnedDatum::Int(whole) = value {
            if column.declared_type.eq_ignore_ascii_case(b"REAL") {
                value = OwnedDatum::Real(whole as f64);
            }
        }
        if !super::alter::strict_default_fits(&column.declared_type, &value) {
            return None;
        }
    }
    Some(value)
}

/// Returns how many columns the tree of a rowid table has, without deriving its shape.
///
/// The rowid, then every stored column that is not the rowid's own alias; the same count
/// `table_shape` makes its column list from.
///
/// @param info - the table as the catalog declares it
fn rowid_tree_width(info: &TableInfo) -> usize {
    let alias = info.rowid_alias.map(usize::from);
    let stored = (0..info.columns.len())
        .filter(|declared| !crate::engine::tables::is_virtual_column(info, *declared))
        .filter(|declared| Some(*declared) != alias)
        .count();
    stored.saturating_add(1)
}

/// Returns what each tree column reads as in a leaf written without it.
///
/// One entry per tree column. A column with no literal default, or none at all, reads as
/// NULL, which the entry says by being NULL.
///
/// @param info - the table as the catalog declares it
/// @param layout - how the table's declared columns map onto its tree's columns
pub(crate) fn column_fills(info: &TableInfo, layout: &SourceLayout) -> Vec<OwnedDatum> {
    let mut fills = vec![OwnedDatum::Null; layout.width];
    for (declared, slot) in layout.slots.iter().enumerate() {
        let (Some(slot), Some(column)) = (slot, info.columns.get(declared)) else {
            continue;
        };
        let Some(raw) = column.default_sql.as_deref().and_then(literal_default) else {
            continue;
        };

        if let (Some(value), Some(cell)) = (
            stored_default(column, info.strict, raw),
            fills.get_mut(*slot),
        ) {
            *cell = value;
        }
    }
    fills
}

impl ImportedDatabase {
    /// Gives every table's tree the pad that tells it what an older leaf reads as.
    ///
    /// **Run whenever the catalog is derived, because a pad is derived from it.** A tree
    /// attached by an open, a rollback or a reattach knows its columns and not what a
    /// leaf written before the last `ADD COLUMN` holds for the newest of them. A tree
    /// that already has its pad is left alone, which is every tree after the first
    /// refresh.
    pub(crate) fn refresh_column_pads(&mut self) {
        for position in 0..self.schema.tables.len() {
            self.refresh_column_pad_at(position);
        }
    }

    /// Gives one table's tree the pad that tells it what an older leaf reads as.
    ///
    /// The catalog refreshes that change one table, which a `CREATE TABLE` or `CREATE INDEX`
    /// does, call this for that table alone: every other tree already has the pad it had at the
    /// last full refresh.
    ///
    /// @param position - the table's position in `tables`
    pub(crate) fn refresh_column_pad_at(&mut self, position: usize) {
        let Some(info) = self.schema.tables.get(position) else {
            return;
        };
        {
            if info.kind != inillucent_sql::catalog_view::TableKind::Table || info.module.is_some()
            {
                return;
            }
            let Some(tree) = self.schema.trees.get_mut(&info.root) else {
                return;
            };
            // **A tree that already has its pad and the right width is skipped without deriving
            // its shape.** Deriving it allocates a column list and a layout for every table, at
            // every catalog refresh, so 6,000 tables in one transaction spent most of their time
            // here: the refresh cost time in proportion to the tables that already existed.
            if !info.without_rowid
                && tree.pad().width() != 0
                && tree.columns().len() == rowid_tree_width(info)
            {
                return;
            }
            let Some((columns, layout)) = shape_of_table(info) else {
                return;
            };
            // **The catalog is what says how many columns the tree has.** A tree with fewer
            // is shorter because another process ran `ADD COLUMN` on the file, and the
            // handle kept the shape it was attached with. A tree with more is longer
            // because a statement of this connection added a column and then failed or
            // was rolled back, and the catalog rows were restored without it.
            let changed = columns.len() != tree.columns().len();
            if !changed && tree.pad().width() != 0 {
                return;
            }
            let fills = column_fills(info, &layout);
            tree.set_shape(columns, fills);
            if changed {
                self.schema
                    .layouts
                    .insert(info.root, std::rc::Rc::new(layout));
            }
        }
    }

    /// Adds a column to a table without writing a leaf, when the table allows it.
    ///
    /// **The cheap `ADD COLUMN`.** The catalog row already names the new column. This gives
    /// the table's tree the longer column list and the pad, so every leaf written before
    /// reads the column as its default, and the first write to a leaf widens it. Pages
    /// written: none. The file moves to format 4 first, because a release that stops at 3
    /// would read a leaf with fewer columns than its table as damaged.
    ///
    /// Declines, and the caller rewrites the table as before, when the default is not a
    /// literal this reads the same way the engine evaluates it, or when the table is a
    /// kind the pad does not cover.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    pub(crate) fn add_column_without_rewriting(
        &mut self,
        at: usize,
        folded: &[u8],
    ) -> DbResult<bool> {
        let Some(info) = self
            .schema
            .tables
            .iter()
            .find(|table| table.database == at && table.folded == folded)
            .cloned()
        else {
            return Ok(false);
        };
        // Inside a transaction the column could be rolled back after a write widened a leaf,
        // which would leave a leaf wider than the table. The transaction takes the rewrite,
        // which a rollback already knows how to undo.
        if info.module.is_some()
            || info.kind != inillucent_sql::catalog_view::TableKind::Table
            || !self.schema.trees.contains_key(&info.root)
            || self.writing.batch().is_some()
        {
            return Ok(false);
        }
        let Some((columns, layout)) = shape_of_table(&info) else {
            return Ok(false);
        };
        let Some(newest) = info.columns.last() else {
            return Ok(false);
        };
        // A virtual generated column has no tree column, so the tree already holds every
        // column the table has. Only the layout, which counts declared columns, grows.
        let Some(Some(slot)) = layout.slots.get(info.columns.len().saturating_sub(1)) else {
            self.schema
                .layouts
                .insert(info.root, std::rc::Rc::new(layout));
            return Ok(true);
        };
        if slot.saturating_add(1) != layout.width {
            return Ok(false);
        }
        // The value the pad will give must be the one the engine computes for the same text.
        if let Some(raw) = newest.default_sql.clone() {
            let read =
                literal_default(&raw).and_then(|raw| stored_default(newest, info.strict, raw));
            let evaluated = self.constant_default(&raw)?;
            if read.is_none() || read != stored_default(newest, info.strict, evaluated) {
                return Ok(false);
            }
        }
        self.require_format_four(at)?;
        let fills = column_fills(&info, &layout);
        self.schema
            .layouts
            .insert(info.root, std::rc::Rc::new(layout));
        if let Some(tree) = self.schema.trees.get_mut(&info.root) {
            tree.set_shape(columns, fills);
        }
        Ok(true)
    }

    /// Moves a file to format 4, logging the move first when this is the first time.
    ///
    /// The record belongs to no transaction, and a release that stops at format 3 refuses
    /// a log that holds it by kind. Does nothing when the file already says 4.
    ///
    /// @param at - the schema whose file is raised
    pub(crate) fn require_format_four(&mut self, at: usize) -> DbResult<()> {
        let raised = self
            .file_of_mut(Some(at))?
            .raise_format(inillucent_pool::meta::FORMAT_FOUR);
        if raised {
            let wal = self.log_of(at).ok_or_else(|| {
                inillucent_base::error::refusal("a statement names a database that is not attached")
            })?;
            wal.append(
                0,
                inillucent_wal::record::Body::FormatRaised {
                    version: inillucent_pool::meta::FORMAT_FOUR,
                },
            )?;
        }
        Ok(())
    }
}

/// Returns a table's tree columns and layout as the catalog declares them.
///
/// @param info - the table
fn shape_of_table(
    info: &TableInfo,
) -> Option<(Vec<inillucent_tree::types::ColumnSpec>, SourceLayout)> {
    match info.without_rowid {
        true => keyed_table_shape(info)
            .ok()
            .map(|(columns, _, layout)| (columns, layout)),
        false => Some(table_shape(info)),
    }
}
