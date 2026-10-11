//! The longest key a tree stores.
//!
//! Invariant: **a key longer than [`MAX_KEY_BYTES`] encoded is refused before
//! any page is touched.** A delete and an in place update carry the whole key in
//! their log record, and a record is at most 4 MiB, so the key has to be bounded
//! well below that. One mebibyte is far above the 50,000 character case that
//! motivated storing keys out of line and leaves three quarters of a record for
//! the rest of a batched delete. SQLite's own limit is a gigabyte and nothing
//! here needs it.

use inillucent_base::error::statement_refusal;
use inillucent_base::DbResult;
use inillucent_value::collation::Collation;

use crate::keyenc::KeyEncoding;
use crate::leaf::Rows;

/// The most bytes an encoded key may have: 1,048,576.
pub const MAX_KEY_BYTES: usize = 1 << 20;

/// Returns the refusal for a key over the limit.
fn key_too_long() -> inillucent_base::DbError {
    statement_refusal("an index or primary key entry is longer than 1,048,576 bytes")
}

/// Refuses an encoded key that is over the limit.
///
/// @param encoded_len - the key's length once encoded for the tree
pub(crate) fn refuse_a_key_over_the_limit(encoded_len: usize) -> DbResult<()> {
    match encoded_len > MAX_KEY_BYTES {
        true => Err(key_too_long()),
        false => Ok(()),
    }
}

/// Refuses a bulk build whose rows hold a key over the limit.
///
/// A key's encoding is at most twice its raw bytes plus a few bytes for each
/// column, so only a row whose raw key bytes could reach the limit is encoded to
/// find out. The ordinary row costs one pass over the lengths of its key values.
///
/// @param rows - the rows of the build
/// @param key_columns - how many leading columns are the key
/// @param encoding - how a key tuple becomes comparable bytes
/// @param collations - one per key column
/// @param directions - one per key column, true for descending
pub(crate) fn refuse_rows_with_a_key_over_the_limit<'d>(
    rows: &dyn Rows<'d>,
    key_columns: usize,
    encoding: KeyEncoding,
    collations: &[Collation],
    directions: &[bool],
) -> DbResult<()> {
    let slack = key_columns.saturating_mul(16);
    let mut head = Vec::with_capacity(key_columns);
    for row in 0..rows.len() {
        head.clear();
        let mut raw = 0usize;
        for column in 0..key_columns {
            let value = rows.value(row, column);
            raw = raw.saturating_add(value.as_bytes().map_or(0, <[u8]>::len));
            head.push(value);
        }
        if raw.saturating_mul(2).saturating_add(slack) <= MAX_KEY_BYTES {
            continue;
        }
        let mut encoded = Vec::new();
        encoding.encode_into(&head, collations, directions, &mut encoded);
        refuse_a_key_over_the_limit(encoded.len())?;
    }
    Ok(())
}
