//! How a key tuple becomes the comparable bytes a tree is ordered by.
//!
//! Invariant: **the bytes this produces sort under `memcmp` in exactly the
//! order the key tuple sorts in.** That is what lets a descent compare a probe
//! against a separator with one `memcmp`, and what lets a bulk build sort by
//! the encoded bytes rather than by a second opinion about the order.
//!
//! Extracted whole from `paged.rs` in task-1932, nothing changed in the move.
//! It is one question - how a key becomes bytes - rather than a slice taken to
//! make a number fit, and `paged.rs` is one of the modules
//! `crates/inillucent-compat/tests/tooling/policy.rs` holds a line ceiling over.

use inillucent_value::collation::Collation;

use crate::datum::Datum;
use crate::key;
use crate::types::{ColumnSpec, PhysicalType};

/// How a tree's keys become comparable bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyEncoding {
    /// One `Int64` column, encoded as eight exact bytes.
    Rowid,
    /// Anything else, through the general memcmp encoding.
    General,
}

impl KeyEncoding {
    /// Chooses the encoding a column directory calls for.
    ///
    /// The rowid form is taken only for a single non-nullable `Int64` key
    /// column, because it has no way to represent a NULL, a text or a real -
    /// and a rowid, by definition, is never any of those.
    ///
    /// @param columns - the column directory, key columns first
    /// @param key_columns - how many leading columns form the key
    pub fn choose(columns: &[ColumnSpec], key_columns: usize) -> KeyEncoding {
        if key_columns != 1 {
            return KeyEncoding::General;
        }
        match columns.first() {
            Some(spec) if spec.physical == PhysicalType::Int64 && !spec.nullable() => {
                KeyEncoding::Rowid
            }
            _ => KeyEncoding::General,
        }
    }

    /// Encodes a key tuple into comparable bytes.
    ///
    /// A rowid tree's probe that is not an integer falls back to the general
    /// encoding for that one comparison, which cannot match any separator and
    /// therefore lands the descent at the leftmost or rightmost leaf rather
    /// than somewhere arbitrary. The leaf's own search then answers correctly.
    ///
    /// @param self - the tree's encoding
    /// @param values - the key tuple
    pub fn encode(self, values: &[Datum<'_>]) -> Vec<u8> {
        self.encode_under(values, &[])
    }

    /// Encodes a key tuple whose columns have collations.
    ///
    /// @param values - the key tuple
    /// @param collations - one per column; short means BINARY for the rest
    pub fn encode_under(self, values: &[Datum<'_>], collations: &[Collation]) -> Vec<u8> {
        self.encode_ordered(values, collations, &[])
    }

    /// Encodes a key tuple whose columns have collations and directions.
    ///
    /// @param values - the key tuple
    /// @param collations - one per column; short means BINARY for the rest
    /// @param descending - one per column; short means ascending for the rest
    pub fn encode_ordered(
        self,
        values: &[Datum<'_>],
        collations: &[Collation],
        descending: &[bool],
    ) -> Vec<u8> {
        // Sized from the values rather than grown: a number encodes to 18
        // bytes and a text to its length and a few more, so this is enough for
        // nearly every key, and growing it was 3% of an indexed insert
        // (task-2191).
        let wanted = values
            .iter()
            .map(|value| value.tagged_len().saturating_add(10))
            .sum();
        let mut out = Vec::with_capacity(wanted);
        self.encode_into(values, collations, descending, &mut out);
        out
    }

    /// Appends a key tuple's comparable bytes to a buffer.
    ///
    /// **The appending form is what lets a bulk build keep every key in one
    /// arena.** `CREATE INDEX` needs a key per row to sort by, and returning a
    /// `Vec` per row is one heap allocation per row - the exact cost that made
    /// the previous attempt at pre-encoded keys slower than the comparison sort
    /// it replaced. With a single buffer and a run of offsets there is no
    /// per-row allocation at all, and the bytes are the tree's own encoding, so
    /// sorting by `memcmp` over them is sorting by the order the tree is read
    /// in rather than by a second opinion about it.
    ///
    /// @param values - the key tuple
    /// @param collations - one per column; short means BINARY for the rest
    /// @param out - the buffer to append to
    pub fn encode_into(
        self,
        values: &[Datum<'_>],
        collations: &[Collation],
        descending: &[bool],
        out: &mut Vec<u8>,
    ) {
        match (self, values.first()) {
            (KeyEncoding::Rowid, Some(Datum::Int(number))) if values.len() == 1 => {
                out.extend_from_slice(&key::order_preserving_int(*number));
            }
            (KeyEncoding::Rowid, Some(Datum::Real(number))) if values.len() == 1 => {
                // A real probe against a rowid tree: clamp to the integer it
                // sits between, so the descent lands on the leaf that could
                // hold it rather than at an end of the tree.
                let clamped = if *number <= i64::MIN as f64 {
                    i64::MIN
                } else if *number >= i64::MAX as f64 {
                    i64::MAX
                } else {
                    number.floor() as i64
                };
                out.extend_from_slice(&key::order_preserving_int(clamped));
                // **The real itself follows, so two reals between the same
                // pair of integers still sort the way they compare.** An
                // integer is the eight bytes alone, which is a prefix of this
                // and sorts below it: 3 < 3.14 < 4. A rowid tree never holds a
                // real and ignores the tail; a `WITHOUT ROWID` table keyed by
                // an `INTEGER` column can.
                let collation = collations.first().copied().unwrap_or(Collation::Binary);
                key::encode_into_with(&Datum::Real(*number), collation, out);
            }
            (KeyEncoding::Rowid, Some(Datum::Null)) => {
                out.extend_from_slice(&key::order_preserving_int(i64::MIN));
            }
            (KeyEncoding::Rowid, None) => {}
            // **Text and blobs follow the largest integer, in the general
            // order.** The eight bytes of `i64::MAX` are every integer's
            // upper bound, and the general encoding after them puts a text
            // above a number and a blob above a text, which is the order SQLite
            // keeps. Without the tail every text of the tree was one key, and a
            // probe for one text landed in a leaf that held another.
            (KeyEncoding::Rowid, Some(other)) => {
                out.extend_from_slice(&[0xFF; 8]);
                let collation = collations.first().copied().unwrap_or(Collation::Binary);
                key::encode_into_with(other, collation, out);
            }
            (KeyEncoding::General, _) => {
                for (index, value) in values.iter().enumerate() {
                    let collation = collations.get(index).copied().unwrap_or(Collation::Binary);
                    let start = out.len();
                    key::encode_into_with(value, collation, out);
                    // **A descending column is its own bytes, inverted.** The
                    // encoding is order preserving, so complementing every byte
                    // of one column's span reverses that column's order and
                    // leaves every other column's alone - which is what a
                    // `DESC` key column means, and is why the comparison below
                    // and the sort the bulk build does over these bytes both
                    // come out right with no further change.
                    if descending.get(index).copied().unwrap_or(false) {
                        if let Some(span) = out.get_mut(start..) {
                            for byte in span {
                                *byte = !*byte;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Returns the shortest prefix of `right` that sorts above `left`.
///
/// A separator between two leaves has to sort above every key of the left leaf
/// and at or below every key of the right one. The right leaf's first key is
/// always such a key, and it can be long. Its prefix up to and including the
/// first byte that differs from the left leaf's last key is shorter and does
/// the same job: it has a larger byte at that position, or `left` is a proper
/// prefix of it, and it is a prefix of `right`, so it sorts at or below
/// `right`.
///
/// `left` must sort below `right`. When it does not, `right` itself is returned.
///
/// @param left - the encoded last key of the left leaf
/// @param right - the encoded first key of the right leaf
pub fn shorten_separator<'r>(left: &[u8], right: &'r [u8]) -> &'r [u8] {
    let shared = left
        .iter()
        .zip(right.iter())
        .take_while(|(held, other)| held == other)
        .count();
    if shared >= right.len() {
        return right;
    }
    right.get(..shared.saturating_add(1)).unwrap_or(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The separator is the shortest prefix of the right key that sorts above
    /// the left key, and it bounds both sides the way a separator must.
    #[test]
    fn a_separator_is_the_shortest_prefix_that_sorts_above_the_left_key() {
        let mut shared = vec![b'p'; 5_000];
        let mut left_long = shared.clone();
        left_long.push(b'a');
        shared.push(b'b');
        shared.extend_from_slice(b"tail that is not needed");
        let cases: [(&[u8], &[u8], &[u8]); 4] = [
            (b"abc", b"abd", b"abd"),
            (b"abc", b"abcd", b"abcd"),
            (b"a", b"b", b"b"),
            (&left_long, &shared, &shared[..5_001]),
        ];
        for (left, right, wanted) in cases {
            let separator = shorten_separator(left, right);
            assert_eq!(separator, wanted);
            assert!(left < separator, "above the left key");
            assert!(separator <= right, "at or below the right key");
        }
        // Not a case a split produces, but the answer is still the right key.
        assert_eq!(shorten_separator(b"b", b"a"), b"a");
        assert_eq!(shorten_separator(b"a", b"a"), b"a");
    }

    /// A `WITHOUT ROWID` table keyed by an `INTEGER` column can hold a real, a
    /// text or a blob. Each is the eight bytes of the integer it sits against
    /// followed by the general encoding of the value, so integers keep their
    /// eight bytes and the whole order is integers with reals between them,
    /// then text, then blobs, as SQLite compares them.
    #[test]
    fn a_rowid_encoding_orders_reals_text_and_blobs_around_the_integers() {
        let encode = |datum: Datum<'_>| KeyEncoding::Rowid.encode(&[datum]);
        assert!(encode(Datum::Text(b"x")).starts_with(&[0xFF; 8]));
        assert!(encode(Datum::Real(2.5)).starts_with(&encode(Datum::Int(2))));
        let ascending = [
            encode(Datum::Int(i64::MIN)),
            encode(Datum::Int(2)),
            encode(Datum::Real(2.5)),
            encode(Datum::Int(3)),
            encode(Datum::Int(i64::MAX)),
            encode(Datum::Real(f64::MAX)),
            encode(Datum::Text(b"x")),
            encode(Datum::Text(b"y")),
            encode(Datum::Blob(b"a")),
        ];
        for pair in ascending.windows(2) {
            assert!(pair[0] < pair[1], "{:?} below {:?}", pair[0], pair[1]);
        }
    }
}
