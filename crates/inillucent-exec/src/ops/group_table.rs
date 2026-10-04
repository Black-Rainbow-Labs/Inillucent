//! The groups a hash `GROUP BY` has found, keyed by their encoded keys.
//!
//! Invariant: **a group's position never changes once it is given, and two keys
//! are the same group exactly when their encoded bytes are equal.** Positions
//! are handed out in the order groups are first seen, so the accumulators the
//! aggregate keeps beside each position line up with the table for its life.
//!
//! ## Why it is not a `HashMap<Vec<u8>, usize>`
//!
//! That map held each group's key in an allocation of its own and hashed it
//! with SipHash. A `GROUP BY` with 50,000 groups paid 50,000 allocations for
//! the keys, a sixth of the statement in SipHash, and about 56 bytes a group in
//! map entries and key headers (task-2183). Here every key lives in one byte
//! vector, a group costs its key bytes, eight bytes of span and its share of a
//! table of `u64` slots, and the hash is the engine's own
//! [`inillucent_base::table_hash`], which is two multiplications per eight
//! bytes.

use std::hash::Hasher;

use inillucent_base::table_hash::TableHasher;

/// The most groups one table holds: positions are stored as `u32`.
const MOST_GROUPS: usize = u32::MAX as usize - 1;

/// An open addressing table of encoded group keys.
#[derive(Debug, Default)]
pub(crate) struct GroupTable {
    /// One slot per bucket: zero for empty, or one more than a group's
    /// position in the low half and the top 32 bits of its hash in the high half.
    ///
    /// **The hash beside the position (task-2183).** A lookup that met another
    /// group's slot read that group's hash out of `hashes` to reject it, one
    /// more scattered read per probe; 50,000 groups is a table larger than the
    /// processor's nearer caches, so every such read was a miss.
    slots: Vec<u64>,
    /// Every group's encoded key, back to back, in position order.
    keys: Vec<u8>,
    /// Where each group's key lies in `keys`, by position.
    spans: Vec<(u32, u32)>,
    /// Each group's hash, by position, so growing never hashes a key again.
    hashes: Vec<u64>,
}

impl GroupTable {
    /// How many groups the table holds.
    pub(crate) fn len(&self) -> usize {
        self.spans.len()
    }

    /// Reports whether the table holds no group.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Returns one group's encoded key.
    ///
    /// @param position - the group's position
    pub(crate) fn key(&self, position: usize) -> &[u8] {
        match self.spans.get(position) {
            Some((start, length)) => {
                let start = *start as usize;
                self.keys
                    .get(start..start.saturating_add(*length as usize))
                    .unwrap_or(&[])
            }
            None => &[],
        }
    }

    /// Returns a key's position, if a group holds it.
    ///
    /// @param key - the encoded key
    pub(crate) fn find(&self, key: &[u8]) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let hash = hash_of(key);
        let tag = hash >> 32;
        let mask = self.slots.len() - 1;
        let mut at = (hash as usize) & mask;
        loop {
            let slot = *self.slots.get(at)?;
            if slot == 0 {
                return None;
            }
            if slot >> 32 == tag {
                let position = ((slot & 0xffff_ffff) - 1) as usize;
                if self.key(position) == key {
                    return Some(position);
                }
            }
            at = (at + 1) & mask;
        }
    }

    /// Adds a key no group holds and returns its new position.
    ///
    /// The caller has asked [`GroupTable::find`] first; a key added twice
    /// becomes two groups.
    ///
    /// @param key - the encoded key
    pub(crate) fn insert(&mut self, key: &[u8]) -> Option<usize> {
        let position = self.spans.len();
        if position >= MOST_GROUPS || self.keys.len().saturating_add(key.len()) > u32::MAX as usize
        {
            return None;
        }
        if (position + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let hash = hash_of(key);
        self.spans.push((self.keys.len() as u32, key.len() as u32));
        self.keys.extend_from_slice(key);
        self.hashes.push(hash);
        self.place(hash, position);
        Some(position)
    }

    /// Empties the table, keeping its storage.
    pub(crate) fn clear(&mut self) {
        self.slots.iter_mut().for_each(|slot| *slot = 0);
        self.keys.clear();
        self.spans.clear();
        self.hashes.clear();
    }

    /// Puts a position in the first empty slot of its hash's probe sequence.
    ///
    /// @param hash - the key's hash
    /// @param position - the group's position
    fn place(&mut self, hash: u64, position: usize) {
        let mask = self.slots.len() - 1;
        let mut at = (hash as usize) & mask;
        while let Some(slot) = self.slots.get_mut(at) {
            if *slot == 0 {
                *slot = ((hash >> 32) << 32) | (position as u64 + 1);
                return;
            }
            at = (at + 1) & mask;
        }
    }

    /// Doubles the slots and places every group again, from its kept hash.
    fn grow(&mut self) {
        let size = (self.slots.len() * 2).max(16);
        self.slots = vec![0; size];
        for position in 0..self.hashes.len() {
            let hash = self.hashes.get(position).copied().unwrap_or(0);
            self.place(hash, position);
        }
    }
}

/// Hashes an encoded key.
///
/// @param key - the encoded key
fn hash_of(key: &[u8]) -> u64 {
    let mut hasher = TableHasher::default();
    hasher.write(key);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key finds the group it was added as, through growth, and nothing else does.
    #[test]
    fn keys_find_their_groups_through_growth() {
        let mut table = GroupTable::default();
        assert!(table.is_empty());
        let keys: Vec<Vec<u8>> = (0..5_000u32)
            .map(|n| format!("k{n}").into_bytes())
            .collect();
        for (at, key) in keys.iter().enumerate() {
            assert_eq!(table.find(key), None);
            assert_eq!(table.insert(key), Some(at));
        }
        assert_eq!(table.len(), keys.len());
        for (at, key) in keys.iter().enumerate() {
            assert_eq!(table.find(key), Some(at));
            assert_eq!(table.key(at), key.as_slice());
        }
        assert_eq!(table.find(b"absent"), None);
        assert_eq!(table.find(b""), None);
        assert_eq!(table.insert(b""), Some(keys.len()));
        assert_eq!(table.find(b""), Some(keys.len()));
        table.clear();
        assert_eq!(table.find(b"k1"), None);
        assert_eq!(table.insert(b"k1"), Some(0));
    }
}
