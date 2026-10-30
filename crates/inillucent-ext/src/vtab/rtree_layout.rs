//! Which layout an `rtree_i32` table's cells are written in.
//!
//! Invariant: **an `rtree_i32` table never reads a cell in a layout other than the one that wrote
//! it.** Releases up to 2.8.1 wrote the coordinates of an `rtree_i32` table as 32 bit floats, as
//! an `rtree` table does, and lost every integer above 16,777,216. SQLite writes them as 32 bit
//! integers and so does this build. A table made by this build holds a marker row in `%_node`
//! (node number -1, which is never a node); a table without one was written by an older release,
//! is read as floats, and is converted to integers by its first write.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use inillucent_base::DbResult;
use inillucent_value::Value;

use crate::shadow::ShadowTables;

use super::Context;

/// The `%_node` row that says the table's cells are 32 bit integers.
pub(super) const MARKER_NODE: i64 = -1;

/// How the cells of one table are known to be written, shared by the table and its cursors.
#[derive(Clone, Default)]
pub(super) struct Layout(Arc<AtomicU8>);

/// What is known about a table's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Known {
    /// Nobody has looked at the file yet.
    Unknown,
    /// The cells are written the way the module's own kind says.
    Current,
    /// An `rtree_i32` table an older release wrote, whose cells are floats.
    Legacy,
}

impl Layout {
    /// Returns what is known.
    pub(super) fn known(&self) -> Known {
        match self.0.load(Ordering::Relaxed) {
            1 => Known::Current,
            2 => Known::Legacy,
            _ => Known::Unknown,
        }
    }

    /// Records what was found.
    ///
    /// @param known - the layout the file was found in
    pub(super) fn set(&self, known: Known) {
        let code = match known {
            Known::Unknown => 0,
            Known::Current => 1,
            Known::Legacy => 2,
        };
        self.0.store(code, Ordering::Relaxed);
    }
}

/// Reads the marker and records the layout, when that has not been done.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param layout - the shared record of what is known
pub(super) fn look(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    layout: &Layout,
) -> DbResult<Known> {
    if layout.known() != Known::Unknown {
        return Ok(layout.known());
    }
    let found = match shadows.read_row(context, b"node", MARKER_NODE)? {
        Some(_) => Known::Current,
        None => Known::Legacy,
    };
    layout.set(found);
    Ok(found)
}

/// Writes the marker row.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
pub(super) fn write_marker(context: &mut Context<'_>, shadows: &ShadowTables) -> DbResult<()> {
    shadows.write_row(
        context,
        b"node",
        MARKER_NODE,
        &[Value::Null, Value::owned_blob(b"i32")?],
    )
}

/// Rewrites every node of a legacy table with integer cells and writes the marker.
///
/// A cell is an eight byte key and then two four byte numbers per dimension, after a four byte
/// header, so each number is read as the float it was and written back as the integer it is.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param dimensions - how many dimensions the table has
pub(super) fn convert_to_integers(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    dimensions: usize,
) -> DbResult<()> {
    let mut nodes: Vec<(i64, Vec<u8>)> = Vec::new();
    shadows.scan(context, b"node", |number, values| {
        if let Some(Value::Blob(blob)) = values.get(1) {
            nodes.push((number, blob.raw().to_vec()));
        }
        Ok(true)
    })?;
    for (number, mut bytes) in nodes {
        convert_node(&mut bytes, dimensions);
        shadows.write_row(
            context,
            b"node",
            number,
            &[Value::Null, Value::owned_blob(&bytes)?],
        )?;
    }
    write_marker(context, shadows)
}

/// Rewrites the numbers of every cell of one node from floats to integers, in place.
///
/// @param node - the node's bytes, header first
/// @param dimensions - how many dimensions the table has
pub(super) fn convert_node(node: &mut [u8], dimensions: usize) {
    let header = 4usize;
    let count = node
        .get(2..4)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map(|bytes| usize::from(u16::from_be_bytes(bytes)))
        .unwrap_or(0);
    let size = 8usize.saturating_add(dimensions.saturating_mul(8));
    for index in 0..count {
        let start = header.saturating_add(index.saturating_mul(size));
        for number in 0..dimensions.saturating_mul(2) {
            let at = start
                .saturating_add(8)
                .saturating_add(number.saturating_mul(4));
            let Some(slot) = node.get_mut(at..at.saturating_add(4)) else {
                return;
            };
            let Ok(word) = <[u8; 4]>::try_from(&*slot) else {
                return;
            };
            let float = f32::from_be_bytes(word);
            slot.copy_from_slice(&(float as i32).to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A float cell becomes the integer cell with the same numbers.
    #[test]
    fn a_float_cell_becomes_an_integer_cell() {
        let mut node = vec![0u8; 4 + 8 + 8];
        node[2..4].copy_from_slice(&1u16.to_be_bytes());
        node[4..12].copy_from_slice(&7i64.to_be_bytes());
        node[12..16].copy_from_slice(&3.0f32.to_be_bytes());
        node[16..20].copy_from_slice(&(-5.0f32).to_be_bytes());
        convert_node(&mut node, 1);
        assert_eq!(&node[12..16], &3i32.to_be_bytes());
        assert_eq!(&node[16..20], &(-5i32).to_be_bytes());
        assert_eq!(&node[4..12], &7i64.to_be_bytes());
    }
}
