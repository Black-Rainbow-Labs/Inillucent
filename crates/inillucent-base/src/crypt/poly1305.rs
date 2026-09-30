//! Poly1305 (RFC 8439 section 2.5).
//!
//! Invariant: the tag matches the published test vectors, which are checked
//! below, and the arithmetic takes the same path whatever the key or message.
//! The five 26 bit limbs are the layout poly1305-donna made standard; every
//! product fits in 64 bits, so no carry is ever lost and no branch depends on
//! a value.

/// The low 26 bits of a limb.
const MASK: u32 = 0x03ff_ffff;

/// Reads a little endian word starting at `at`, reading zero past the end.
///
/// @param bytes - where to read
/// @param at - the first byte
fn le32(bytes: &[u8], at: usize) -> u32 {
    let mut four = [0u8; 4];
    for (offset, slot) in four.iter_mut().enumerate() {
        *slot = bytes.get(at.saturating_add(offset)).copied().unwrap_or(0);
    }
    u32::from_le_bytes(four)
}

/// A Poly1305 computation in progress.
pub struct Poly1305 {
    /// The clamped half of the key, as five limbs.
    r: [u32; 5],
    /// The accumulator, as five limbs.
    h: [u32; 5],
    /// The second half of the key, added at the end.
    pad: [u32; 4],
    /// Bytes of an unfinished 16 byte block.
    pending: [u8; 16],
    /// How many bytes of `pending` are filled.
    filled: usize,
}

impl Poly1305 {
    /// Starts a MAC under a one time key.
    ///
    /// @param key - 32 bytes, used for exactly one message
    pub fn new(key: &[u8; 32]) -> Poly1305 {
        Poly1305 {
            r: [
                le32(key, 0) & 0x03ff_ffff,
                (le32(key, 3) >> 2) & 0x03ff_ff03,
                (le32(key, 6) >> 4) & 0x03ff_c0ff,
                (le32(key, 9) >> 6) & 0x03f0_3fff,
                (le32(key, 12) >> 8) & 0x000f_ffff,
            ],
            h: [0; 5],
            pad: [le32(key, 16), le32(key, 20), le32(key, 24), le32(key, 28)],
            pending: [0; 16],
            filled: 0,
        }
    }

    /// Adds bytes to the message.
    ///
    /// @param data - the next bytes
    pub fn update(&mut self, data: &[u8]) {
        let mut rest = data;
        // Finish a block an earlier call left part filled.
        if self.filled != 0 {
            let missing = 16usize.saturating_sub(self.filled).min(rest.len());
            let (head, tail) = rest.split_at(missing);
            if let Some(target) = self
                .pending
                .get_mut(self.filled..self.filled.saturating_add(missing))
            {
                target.copy_from_slice(head);
            }
            self.filled = self.filled.saturating_add(missing);
            rest = tail;
            if self.filled < 16 {
                return;
            }
            let full = self.pending;
            self.absorb(&full, 1 << 24);
            self.filled = 0;
        }
        // **Whole blocks straight from the input**, which is almost every
        // byte of a database unit, instead of copying each byte through
        // `pending` first. See the note in `chacha::apply` for what the two
        // changes were measured to save.
        let mut blocks = rest.chunks_exact(16);
        for chunk in &mut blocks {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            self.absorb(&block, 1 << 24);
        }
        let tail = blocks.remainder();
        if let Some(target) = self.pending.get_mut(..tail.len()) {
            target.copy_from_slice(tail);
        }
        self.filled = tail.len();
    }

    /// Adds zero bytes up to the next 16 byte boundary, which is how the AEAD
    /// construction pads each of its two parts.
    pub fn pad_to_block(&mut self) {
        if self.filled != 0 {
            let zeros = [0u8; 16];
            let missing = 16usize.saturating_sub(self.filled);
            self.update(zeros.get(..missing).unwrap_or(&[]));
        }
    }

    /// Multiplies one 16 byte block into the accumulator.
    ///
    /// @param block - the block
    /// @param high - `1 << 24` for a full block, zero for the padded last one
    fn absorb(&mut self, block: &[u8; 16], high: u32) {
        let [r0, r1, r2, r3, r4] = self.r.map(u64::from);
        let (s1, s2, s3, s4) = (
            r1.wrapping_mul(5),
            r2.wrapping_mul(5),
            r3.wrapping_mul(5),
            r4.wrapping_mul(5),
        );
        // Constant indices into a fixed array, so no bounds check and no loop:
        // this runs once per 16 bytes of every unit.
        let b = block;
        let t0 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let t1 = u32::from_le_bytes([b[3], b[4], b[5], b[6]]);
        let t2 = u32::from_le_bytes([b[6], b[7], b[8], b[9]]);
        let t3 = u32::from_le_bytes([b[9], b[10], b[11], b[12]]);
        let t4 = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        let h0 = u64::from(self.h[0].wrapping_add(t0 & MASK));
        let h1 = u64::from(self.h[1].wrapping_add((t1 >> 2) & MASK));
        let h2 = u64::from(self.h[2].wrapping_add((t2 >> 4) & MASK));
        let h3 = u64::from(self.h[3].wrapping_add((t3 >> 6) & MASK));
        let h4 = u64::from(self.h[4].wrapping_add((t4 >> 8) | high));
        let product = |a: [u64; 5], b: [u64; 5]| -> u64 {
            a.iter()
                .zip(b.iter())
                .fold(0u64, |sum, (x, y)| sum.wrapping_add(x.wrapping_mul(*y)))
        };
        let h = [h0, h1, h2, h3, h4];
        let d = [
            product(h, [r0, s4, s3, s2, s1]),
            product(h, [r1, r0, s4, s3, s2]),
            product(h, [r2, r1, r0, s4, s3]),
            product(h, [r3, r2, r1, r0, s4]),
            product(h, [r4, r3, r2, r1, r0]),
        ];
        self.carry(d);
    }

    /// Reduces five 64 bit partial sums back to five 26 bit limbs.
    ///
    /// @param d - the partial sums of one multiplication
    fn carry(&mut self, d: [u64; 5]) {
        let mask = u64::from(MASK);
        let mut limbs = [0u64; 5];
        let mut carry = 0u64;
        for (limb, sum) in limbs.iter_mut().zip(d.iter()) {
            let total = sum.wrapping_add(carry);
            *limb = total & mask;
            carry = total >> 26;
        }
        let [l0, l1, l2, l3, l4] = limbs;
        let l0 = l0.wrapping_add(carry.wrapping_mul(5));
        let l1 = l1.wrapping_add(l0 >> 26);
        self.h = [
            (l0 & mask) as u32,
            l1 as u32,
            l2 as u32,
            l3 as u32,
            l4 as u32,
        ];
    }

    /// Finishes the MAC and returns the 16 byte tag.
    pub fn finish(mut self) -> [u8; 16] {
        if self.filled != 0 {
            let mut last = [0u8; 16];
            let filled = self.filled.min(16);
            if let (Some(target), Some(source)) =
                (last.get_mut(..filled), self.pending.get(..filled))
            {
                target.copy_from_slice(source);
            }
            if let Some(marker) = last.get_mut(filled) {
                *marker = 1;
            }
            self.absorb(&last, 0);
        }
        let h = self.fully_reduced();
        let packed = [
            h[0] | (h[1] << 26),
            (h[1] >> 6) | (h[2] << 20),
            (h[2] >> 12) | (h[3] << 14),
            (h[3] >> 18) | (h[4] << 8),
        ];
        let mut tag = [0u8; 16];
        let mut carry = 0u64;
        for ((chunk, word), pad) in tag
            .chunks_exact_mut(4)
            .zip(packed.iter())
            .zip(self.pad.iter())
        {
            let sum = u64::from(*word)
                .wrapping_add(u64::from(*pad))
                .wrapping_add(carry);
            chunk.copy_from_slice(&(sum as u32).to_le_bytes());
            carry = sum >> 32;
        }
        tag
    }

    /// Returns the accumulator reduced modulo 2^130 - 5, without branching.
    fn fully_reduced(&self) -> [u32; 5] {
        let mut h = self.h;
        let mut carry = 0u32;
        for limb in h.iter_mut().skip(1) {
            *limb = limb.wrapping_add(carry);
            carry = *limb >> 26;
            *limb &= MASK;
        }
        h[0] = h[0].wrapping_add(carry.wrapping_mul(5));
        let c = h[0] >> 26;
        h[0] &= MASK;
        h[1] = h[1].wrapping_add(c);
        // g = h + 5 - 2^130. When g does not go negative, h was at least the
        // modulus and g is the reduced value.
        let mut g = [0u32; 5];
        let mut carry = 5u32;
        for (slot, limb) in g.iter_mut().zip(h.iter()) {
            let sum = limb.wrapping_add(carry);
            carry = sum >> 26;
            *slot = sum & MASK;
        }
        g[4] = g[4].wrapping_add(carry << 26).wrapping_sub(1 << 26);
        let keep_g = (g[4] >> 31).wrapping_sub(1);
        let mut out = [0u32; 5];
        for ((slot, a), b) in out.iter_mut().zip(h.iter()).zip(g.iter()) {
            *slot = (a & !keep_g) | (b & keep_g & MASK);
        }
        out
    }
}

/// Returns the Poly1305 tag of a message under a one time key.
///
/// @param key - 32 bytes, used for exactly one message
/// @param message - the bytes to authenticate
pub fn tag(key: &[u8; 32], message: &[u8]) -> [u8; 16] {
    let mut mac = Poly1305::new(key);
    mac.update(message);
    mac.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt::from_hex;

    /// RFC 8439 section 2.5.2.
    #[test]
    fn the_tag_matches_rfc_8439_2_5_2() {
        let key: [u8; 32] =
            from_hex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
        let expected: [u8; 16] = from_hex("a8061dc1305136c6c22b8baf0c0127a9");
        assert_eq!(tag(&key, b"Cryptographic Forum Research Group"), expected);
    }

    /// RFC 8439 appendix A.3, test vector 1: a zero key gives a zero tag.
    #[test]
    fn a_zero_key_gives_a_zero_tag() {
        assert_eq!(tag(&[0u8; 32], &[0u8; 64]), [0u8; 16]);
    }

    /// RFC 8439 appendix A.3, test vector 2.
    #[test]
    fn the_tag_matches_rfc_8439_a_3_vector_2() {
        let key: [u8; 32] =
            from_hex("0000000000000000000000000000000036e5f6b5c5e06070f0efca96227a863e");
        let message = b"Any submission to the IETF intended by the Contributor for publication as all or part of an IETF Internet-Draft or RFC and any statement made within the context of an IETF activity is considered an \"IETF Contribution\". Such statements include oral statements in IETF sessions, as well as written and electronic communications made at any time or place, which are addressed to";
        let expected: [u8; 16] = from_hex("36e5f6b5c5e06070f0efca96227a863e");
        assert_eq!(tag(&key, message), expected);
    }

    /// RFC 8439 appendix A.3, test vectors 5 to 10: the edge cases of the
    /// final reduction, where the accumulator lands on or just past 2^130 - 5.
    #[test]
    fn the_reduction_edge_cases_match_rfc_8439_a_3() {
        let cases: [(&str, &str, &str); 6] = [
            (
                "0200000000000000000000000000000000000000000000000000000000000000",
                "ffffffffffffffffffffffffffffffff",
                "03000000000000000000000000000000",
            ),
            (
                "02000000000000000000000000000000ffffffffffffffffffffffffffffffff",
                "02000000000000000000000000000000",
                "03000000000000000000000000000000",
            ),
            (
                "0200000000000000000000000000000000000000000000000000000000000000",
                "fdffffffffffffffffffffffffffffff",
                "faffffffffffffffffffffffffffffff",
            ),
            (
                "0100000000000000000000000000000000000000000000000000000000000000",
                "ffffffffffffffffffffffffffffffff\
                 fbfefefefefefefefefefefefefefefe\
                 01010101010101010101010101010101",
                "00000000000000000000000000000000",
            ),
            (
                "0100000000000000000000000000000000000000000000000000000000000000",
                "fffffffffffffffffffffffffffffffff0ffffffffffffffffffffffffffffff\
                 11000000000000000000000000000000",
                "05000000000000000000000000000000",
            ),
            (
                "0100000000000000040000000000000000000000000000000000000000000000",
                "e33594d7505e43b900000000000000003394d7505e4379cd0100000000000000\
                 0000000000000000000000000000000001000000000000000000000000000000",
                "14000000000000005500000000000000",
            ),
        ];
        for (key, message, expected) in cases {
            let key: [u8; 32] = from_hex(key);
            let message = crate::crypt::hex_bytes(message);
            let expected: [u8; 16] = from_hex(expected);
            assert_eq!(tag(&key, &message), expected);
        }
    }

    /// Feeding a message in pieces gives the same tag as one call.
    #[test]
    fn a_streamed_tag_matches_a_single_call() {
        let key = [3u8; 32];
        let message: Vec<u8> = (0..100u32).map(|value| value as u8).collect();
        let mut mac = Poly1305::new(&key);
        for piece in message.chunks(7) {
            mac.update(piece);
        }
        assert_eq!(mac.finish(), tag(&key, &message));
    }
}
