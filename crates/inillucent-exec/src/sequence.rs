//! `AUTOINCREMENT`: the high-water mark a table never hands out twice.
//!
//! Invariant: **an `AUTOINCREMENT` table's next key is one past the largest it
//! has ever held**, not one past the largest it holds now. That is the whole of
//! the difference between the two forms, and it is the only thing
//! `AUTOINCREMENT` buys: an ordinary `INTEGER PRIMARY KEY` reuses the numbers
//! its deleted rows had, which makes a foreign key held outside the database -
//! a URL, a log line, a row in another system - point at a different row than
//! the one it was written about.
//!
//! This engine used to parse `AUTOINCREMENT` onto
//! `TableInfo::autoincrement`, resolve `sqlite_sequence`'s root onto
//! `BoundInsert::sequence_root`, and then allocate from the table's own
//! largest key like any other table - so one insert, a delete, and another
//! insert handed out 1 twice where SQLite hands out 1 and then 2.
//!
//! ## Where the mark lives
//!
//! In SQLite's own `sqlite_sequence(name, seq)` table, one row per
//! `AUTOINCREMENT` table, written in the same transaction as the row that moved
//! it. That placement is what makes it roll back with the row: a transaction
//! that inserts and then rolls back leaves the mark where it was, and the next
//! insert reuses the number - which is what SQLite does, and is a *consequence*
//! of the mark being ordinary data rather than a counter beside the file.

use inillucent_base::{DbError, DbResult, PrimaryCode};
use inillucent_sql::catalog_view::TableInfo;
use inillucent_tree::datum::{Datum, OwnedDatum};

use crate::dml::WriteTarget;

/// The table the mark lives in, under SQLite's own name.
pub const SEQUENCE_TABLE: &[u8] = b"sqlite_sequence";

/// The `CREATE` text stored for it, byte for byte SQLite's.
pub const SEQUENCE_SQL: &str = "CREATE TABLE sqlite_sequence(name,seq)";

/// One `sqlite_sequence` row: where it sits, and what it says.
pub struct Mark {
    /// The row's own key, when the row is already there.
    ///
    /// `None` means no row has been written for this table yet, which is the
    /// state between `CREATE TABLE ... AUTOINCREMENT` and its first insert.
    pub rowid: Option<i64>,
    /// The largest key the table has ever handed out.
    pub seq: i64,
}

/// Reads a table's high-water mark.
///
/// The table's own largest key is taken as a floor. It is normally below the
/// stored mark and can only be above it for a table whose rows arrived without
/// the mark being written - a file another engine built, or one this engine
/// wrote before that was fixed - and taking the larger of the two is what stops
/// either of those from handing out a key that is already there.
///
/// @param target - the file and its trees
/// @param sequence_root - the `sqlite_sequence` tree, zero when there is none
/// @param name - the table's name, as `sqlite_sequence` stores it
/// @param floor - the largest key the table itself holds
pub fn read(
    target: &mut dyn WriteTarget,
    sequence_root: u32,
    name: &[u8],
    floor: i64,
) -> DbResult<Mark> {
    let mut mark = Mark {
        rowid: None,
        seq: floor,
    };
    if sequence_root == 0 {
        return Ok(mark);
    }
    let (database, trees, _) = target.parts_for(sequence_root)?;
    let Some(tree) = trees.get(sequence_root) else {
        return Ok(mark);
    };
    tree.visit_leaves(database.pool(), &mut |leaf| {
        for row in leaf.live()? {
            let Some(Datum::Text(held)) = row.get(1) else {
                continue;
            };
            if *held != name {
                continue;
            }
            if let Some(Datum::Int(rowid)) = row.first() {
                mark.rowid = Some(*rowid);
            }
            if let Some(cell) = row.get(2) {
                mark.seq = mark.seq.max(integer_of(cell));
            }
        }
        Ok(true)
    })?;
    Ok(mark)
}

/// Writes a table's high-water mark back, in the caller's transaction.
///
/// @param target - the file and its trees
/// @param sequence_root - the `sqlite_sequence` tree, zero when there is none
/// @param name - the table's name, as `sqlite_sequence` stores it
/// @param mark - the row as it was read, and the value to store
/// @param seq - the new high-water mark
pub fn write(
    target: &mut dyn WriteTarget,
    sequence_root: u32,
    name: &[u8],
    mark: &Mark,
    seq: i64,
) -> DbResult<()> {
    if sequence_root == 0 {
        return Ok(());
    }
    // **A row another statement wrote since `mark` was read is reused.** A
    // trigger body that inserts into the same table runs as its own statement
    // and creates the table's row when its own `mark` had none, so the
    // statement that fired it would then create a second row for the same
    // table. SQLite keeps one counter for the whole statement and writes one
    // row, so a row found now is updated and the larger of the two values is
    // kept.
    let mut existing = mark.rowid;
    let mut seq = seq;
    if existing.is_none() {
        let current = read(target, sequence_root, name, 0)?;
        if current.rowid.is_some() {
            existing = current.rowid;
            seq = seq.max(current.seq);
        }
    }
    let rowid = match existing {
        Some(held) => held,
        None => next_rowid(target, sequence_root)?,
    };
    let row = [
        OwnedDatum::Int(rowid),
        OwnedDatum::Text(name.to_vec()),
        OwnedDatum::Int(seq),
    ];
    let (database, trees, log) = target.parts_for(sequence_root)?;
    let Some(tree) = trees.get_mut(sequence_root) else {
        return Ok(());
    };
    let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
    if existing.is_some() {
        if let Some(key) = borrowed.get(..1) {
            tree.delete(database, log, key)?;
        }
    }
    tree.insert(database, log, &borrowed)?;
    Ok(())
}

/// Removes a table's row, which is what `DROP TABLE` does to it.
///
/// @param target - the file and its trees
/// @param sequence_root - the `sqlite_sequence` tree, zero when there is none
/// @param name - the table's name, as `sqlite_sequence` stores it
pub fn forget(target: &mut dyn WriteTarget, sequence_root: u32, name: &[u8]) -> DbResult<()> {
    let mark = read(target, sequence_root, name, 0)?;
    let Some(rowid) = mark.rowid else {
        return Ok(());
    };
    let (database, trees, log) = target.parts_for(sequence_root)?;
    let Some(tree) = trees.get_mut(sequence_root) else {
        return Ok(());
    };
    tree.delete(database, log, &[Datum::Int(rowid)])?;
    Ok(())
}

/// Returns the key a new `sqlite_sequence` row takes.
///
/// @param target - the file and its trees
/// @param sequence_root - the `sqlite_sequence` tree
fn next_rowid(target: &mut dyn WriteTarget, sequence_root: u32) -> DbResult<i64> {
    let (database, trees, _) = target.parts_for(sequence_root)?;
    let Some(tree) = trees.get(sequence_root) else {
        return Ok(1);
    };
    let mut highest = 0i64;
    tree.visit_leaves(database.pool(), &mut |leaf| {
        for row in leaf.live()? {
            if let Some(Datum::Int(rowid)) = row.first() {
                highest = highest.max(*rowid);
            }
        }
        Ok(true)
    })?;
    Ok(highest.saturating_add(1))
}

/// Returns the key an `AUTOINCREMENT` table hands out next, or the refusal.
///
/// SQLite reports `SQLITE_FULL` - "database or disk is full" - when the mark
/// reaches `i64::MAX`, because there is no next key and the alternative would be
/// to hand out one that is already there.
///
/// @param table - the table being written
/// @param mark - the high-water mark as it stands
pub fn allocate(_table: &TableInfo, mark: i64) -> DbResult<i64> {
    if mark == i64::MAX {
        // The table's name is not in the text: the shell prints the detail in place
        // of the message, and SQLite's text for this failure is the result code's.
        return Err(DbError::primary(PrimaryCode::Full).with_message("database or disk is full"));
    }
    Ok(mark.saturating_add(1))
}

/// Reads a `sqlite_sequence.seq` cell as the integer SQLite takes from it.
///
/// **A `seq` a person edited is read the way SQLite reads it**: text takes its
/// leading integer (`'5abc'` is 5), a real is truncated, and anything else is
/// zero. The value is never refused, because the next insert has to be able to
/// continue from whatever is there.
///
/// @param cell - the `seq` cell of the table's row
fn integer_of(cell: &Datum<'_>) -> i64 {
    match cell {
        Datum::Int(seq) => *seq,
        Datum::Real(seq) => *seq as i64,
        Datum::Text(text) => leading_integer(text),
        Datum::Blob(bytes) => leading_integer(bytes),
        _ => 0,
    }
}

/// Returns the integer a text starts with, saturating at the 64 bit limits.
///
/// @param text - the bytes of the value
fn leading_integer(text: &[u8]) -> i64 {
    let trimmed = text.trim_ascii_start();
    let (negative, digits) = match trimmed.first() {
        Some(b'-') => (true, trimmed.get(1..).unwrap_or_default()),
        Some(b'+') => (false, trimmed.get(1..).unwrap_or_default()),
        _ => (false, trimmed),
    };
    let mut value: i64 = 0;
    for byte in digits.iter().take_while(|byte| byte.is_ascii_digit()) {
        let digit = i64::from(byte - b'0');
        value = match negative {
            true => value.saturating_mul(10).saturating_sub(digit),
            false => value.saturating_mul(10).saturating_add(digit),
        };
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `seq` that was edited to text is read by its leading integer.
    ///
    /// SQLite reads `'5abc'` as 5 when it picks the next key, and a value with
    /// no leading digits as 0, so the table can keep counting from whatever a
    /// person left in `sqlite_sequence`.
    #[test]
    fn text_is_read_by_its_leading_integer() {
        assert_eq!(leading_integer(b"5abc"), 5);
        assert_eq!(leading_integer(b"  -12x"), -12);
        assert_eq!(leading_integer(b"+7"), 7);
        assert_eq!(leading_integer(b"zzz"), 0);
        assert_eq!(leading_integer(b"99999999999999999999"), i64::MAX);
    }

    /// Reals are truncated and every other storage class reads as zero.
    #[test]
    fn other_values_are_truncated_or_zero() {
        assert_eq!(integer_of(&Datum::Real(20.9)), 20);
        assert_eq!(integer_of(&Datum::Null), 0);
        assert_eq!(integer_of(&Datum::Int(4)), 4);
    }
}
