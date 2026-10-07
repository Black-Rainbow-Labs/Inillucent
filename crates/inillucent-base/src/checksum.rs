//! Checksums used by the file formats inillucent reads and writes.
//!
//! Invariant: a checksum function is a pure function of its input bytes and
//! their declared byte order. It never depends on the host's endianness, so a
//! file written on one machine verifies on another.
//!
//! Two algorithms live here. The WAL checksum is the one the SQLite file format
//! specifies for write-ahead log frames, and it is not a general-purpose hash:
//! it is a pair of 32-bit accumulators over 8-byte blocks, chosen for speed on
//! a write path. CRC-32 is used by inillucent's own artifacts, where the format is
//! ours and a standard check value is worth more than a fast one.

use crate::error::{corrupt, DbResult};

/// The byte order a WAL file declares in its header magic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalByteOrder {
    /// Words are read big-endian.
    Big,
    /// Words are read little-endian.
    Little,
}

/// The running pair of accumulators for the WAL frame checksum.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct WalChecksum {
    /// The first accumulator, `s0` in the file-format description.
    pub s0: u32,
    /// The second accumulator, `s1` in the file-format description.
    pub s1: u32,
}

impl WalChecksum {
    /// Starts a checksum from an explicit pair, which is how a frame continues
    /// the checksum of the frame before it.
    pub fn new(s0: u32, s1: u32) -> WalChecksum {
        WalChecksum { s0, s1 }
    }

    /// Folds `data` into the checksum.
    ///
    /// The input must be a whole number of 8-byte blocks; the format never
    /// checksums a partial block, so a caller that asks to is confused about
    /// what it is checksumming and gets an error instead of a padded answer.
    pub fn update(&mut self, data: &[u8], order: WalByteOrder) -> DbResult<()> {
        if !data.len().is_multiple_of(8) {
            return Err(corrupt(
                "WAL checksum input is not a whole number of 8-byte blocks",
            ));
        }
        let (blocks, _) = data.as_chunks::<8>();
        for block in blocks {
            let (first, second) = split_block(block, order);
            self.s0 = self.s0.wrapping_add(first).wrapping_add(self.s1);
            self.s1 = self.s1.wrapping_add(second).wrapping_add(self.s0);
        }
        Ok(())
    }

    /// Folds `data` in and returns the new checksum, leaving the input alone.
    pub fn extended(self, data: &[u8], order: WalByteOrder) -> DbResult<WalChecksum> {
        let mut next = self;
        next.update(data, order)?;
        Ok(next)
    }
}

/// Splits an 8-byte block into its two 32-bit words in the declared order.
fn split_block(block: &[u8; 8], order: WalByteOrder) -> (u32, u32) {
    let word = |bytes: &[u8]| -> u32 {
        let mut value = [0u8; 4];
        for (slot, byte) in value.iter_mut().zip(bytes.iter()) {
            *slot = *byte;
        }
        match order {
            WalByteOrder::Big => u32::from_be_bytes(value),
            WalByteOrder::Little => u32::from_le_bytes(value),
        }
    };
    (
        word(block.get(..4).unwrap_or(&[])),
        word(block.get(4..).unwrap_or(&[])),
    )
}

/// The reflected CRC-32 lookup table for the ISO-HDLC polynomial.
///
/// Built at compile time so the binary carries no initialisation code and the
/// table cannot be corrupted at run time.
const CRC32_TABLE: [u32; 256] = build_crc32_table();

/// Builds the CRC-32 table with the reflected polynomial `0xedb88320`.
///
/// The loop counters and the table index are ordinary arithmetic rather than
/// the checked form the rest of the crate uses. This function runs at compile
/// time over a fixed 256-entry table, so an overflow or an out-of-range index
/// here is a build error rather than something a corrupt file could reach.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
const fn build_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xedb8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

/// The seven further tables slice-by-eight consumes eight bytes with.
///
/// `SLICES[n][i]` is the table entry for a byte that is `n + 1` positions
/// further from the end of the window, which is what lets one iteration fold
/// eight bytes in instead of one.
const CRC32_SLICES: [[u32; 256]; 7] = build_crc32_slices();

/// Builds the seven slice tables from the byte table.
///
/// Compile-time, over fixed 256-entry tables, for the reason
/// [`build_crc32_table`] gives.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
const fn build_crc32_slices() -> [[u32; 256]; 7] {
    let mut slices = [[0u32; 256]; 7];
    let mut index = 0usize;
    while index < 256 {
        let mut previous = CRC32_TABLE[index];
        let mut level = 0usize;
        while level < 7 {
            previous = (previous >> 8) ^ CRC32_TABLE[(previous & 0xff) as usize];
            slices[level][index] = previous;
            level += 1;
        }
        index += 1;
    }
    slices
}

/// Returns one entry of one slice table.
///
/// @param level - which slice table, 0 to 6
/// @param index - the byte to look up
fn slice(level: usize, index: u32) -> u32 {
    CRC32_SLICES
        .get(level)
        .and_then(|table| table.get((index & 0xff) as usize))
        .copied()
        .unwrap_or(0)
}

/// Returns one entry of the byte table.
///
/// @param index - the byte to look up
fn byte_entry(index: u32) -> u32 {
    CRC32_TABLE
        .get((index & 0xff) as usize)
        .copied()
        .unwrap_or(0)
}

/// Computes CRC-32/ISO-HDLC over `data`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_continue(0, data)
}

/// Continues a CRC-32 from a previous result, for data arriving in pieces.
///
/// **Eight bytes per iteration, not one.** The byte-at-a-time form is a table
/// lookup and a shift per byte and runs at about 750 MB/s, which was measured
/// as **9.7 ms of the 13.7 ms** a `CREATE INDEX` over a hundred thousand rows
/// spends writing its 7.3 MB of pages to the log - the checksum, not the write.
/// Slice-by-eight folds a whole `u64` in per iteration using seven further
/// tables derived from the same polynomial, so the answer is bit-for-bit the
/// one above; `crc32_matches_the_byte_at_a_time_form` is what says so, over
/// every length from zero to sixteen and a random long buffer.
///
/// **The SSE4.2 `crc32` instruction cannot be used here, and the reason is the
/// polynomial rather than the portability** (task-2000, design 1d). That design
/// asked for the hardware instruction behind `is_x86_feature_detected!`, with this
/// as the fallback, on the measurement that a 32 KiB page is about 2 us in hardware
/// against about 10 in a table. `_mm_crc32_u64` computes CRC-32C, the Castagnoli
/// polynomial `0x82f63b78`; this is CRC-32/ISO-HDLC, the reflected `0xedb88320`
/// that [`build_crc32_table`] builds. They are different functions of the same
/// bytes, so substituting one for the other would make every page and every log
/// record written by an earlier build fail its checksum - a change to the on disk
/// format, which task-2000's own non-goals rule out. The 10 us the design quoted
/// is the byte-at-a-time form, which this is not: slice-by-eight took the same
/// measurement from 9.7 ms to well under a millisecond, and the comment above
/// records it.
///
/// **A different instruction computes this polynomial, and it is used**
/// (task-2191). Carryless multiply on x86-64 and the ARMv8 `CRC32X` both compute
/// the reflected `0xedb88320`, so the values do not change. A piece of 64 bytes
/// or more goes to [`crate::crc_hardware`] when the processor has them, and
/// everything else to [`crc32_continue_table`].
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece
pub fn crc32_continue(previous: u32, data: &[u8]) -> u32 {
    match crate::crc_hardware::crc32_continue(previous, data) {
        Some(found) => found,
        None => crc32_continue_table(previous, data),
    }
}

/// [`crc32_continue`] by the tables alone, for a short piece, a processor
/// without the instructions, and the bytes after the last whole block.
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece
pub(crate) fn crc32_continue_table(previous: u32, data: &[u8]) -> u32 {
    /// How long a piece has to be before it is split into four streams.
    const SPLIT_FROM: usize = 4_096;
    if data.len() < SPLIT_FROM {
        return !fold_tail(!previous, data);
    }
    crc32_four_streams(previous, data)
}

/// [`crc32_continue`] over a long piece, as four CRCs computed side by side.
///
/// **Four streams, then combined (task-2183).** Slice-by-eight is a chain:
/// each eight bytes' lookups wait for the previous eight's result, so the
/// processor has one dependency chain to work on and most of its load ports
/// idle. Reading a 32 KiB page checked its CRC at about 12 us, which was most
/// of a cold index probe. The piece is cut into four quarters, the four CRCs
/// advance in the same loop as four independent chains, and they are joined
/// with zlib's `crc32_combine` arithmetic, which is exact: the answer is the
/// one the single chain gives, bit for bit, so no page or log record written
/// before reads differently. `crc32_matches_the_byte_at_a_time_form` checks it.
/// Measured by `crc32_page_timing` over a 32 KiB page: 12.19 us as one chain,
/// 3.93 us as four. Below 4 KiB the three joins cost more than they save.
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece, at least a few kilobytes
fn crc32_four_streams(previous: u32, data: &[u8]) -> u32 {
    let quarter = (data.len() / 4) & !7;
    let (first, rest) = data.split_at(quarter);
    let (second, rest) = rest.split_at(quarter);
    let (third, fourth) = rest.split_at(quarter);
    let (fourth_head, fourth_tail) = fourth.split_at(quarter);
    let mut crcs = [!0u32; 4];
    let (a, _) = first.as_chunks::<8>();
    let (b, _) = second.as_chunks::<8>();
    let (c, _) = third.as_chunks::<8>();
    let (d, _) = fourth_head.as_chunks::<8>();
    for (((one, two), three), four) in a.iter().zip(b).zip(c).zip(d) {
        let [w, x, y, z] = crcs;
        crcs = [
            fold_eight(w, one),
            fold_eight(x, two),
            fold_eight(y, three),
            fold_eight(z, four),
        ];
    }
    let [one, two, three, four] = crcs;
    let fourth_crc = !fold_tail(four, fourth_tail);
    let joined = crc32_combine(previous, !one, quarter);
    let joined = crc32_combine(joined, !two, quarter);
    let joined = crc32_combine(joined, !three, quarter);
    crc32_combine(joined, fourth_crc, fourth.len())
}

/// Folds eight bytes into a running (inverted) CRC, slice-by-eight.
///
/// @param crc - the running value, inverted as the loop keeps it
/// @param chunk - the next eight bytes
#[inline(always)]
fn fold_eight(crc: u32, chunk: &[u8; 8]) -> u32 {
    let [b0, b1, b2, b3, b4, b5, b6, b7] = *chunk;
    let one = u32::from_le_bytes([b0, b1, b2, b3]) ^ crc;
    let two = u32::from_le_bytes([b4, b5, b6, b7]);
    slice(6, one)
        ^ slice(5, one >> 8)
        ^ slice(4, one >> 16)
        ^ slice(3, one >> 24)
        ^ slice(2, two)
        ^ slice(1, two >> 8)
        ^ slice(0, two >> 16)
        ^ byte_entry(two >> 24)
}

/// Folds any number of bytes into a running (inverted) CRC.
///
/// @param crc - the running value, inverted as the loop keeps it
/// @param data - the bytes
fn fold_tail(crc: u32, data: &[u8]) -> u32 {
    let mut crc = crc;
    let (chunks, rest) = data.as_chunks::<8>();
    for chunk in chunks {
        crc = fold_eight(crc, chunk);
    }
    for byte in rest {
        crc = byte_entry(crc ^ u32::from(*byte)) ^ (crc >> 8);
    }
    crc
}

/// The reflected polynomial, as [`build_crc32_table`] uses it.
const POLYNOMIAL: u32 = 0xedb8_8320;

/// Returns the CRC of two pieces joined, from the CRC of each and the second's length.
///
/// zlib's `crc32_combine`: the first CRC is carried past the second piece's
/// length by multiplying it by x to the power of eight times that length,
/// modulo the polynomial, and the second CRC is added.
///
/// @param first - the CRC of the first piece
/// @param second - the CRC of the second piece
/// @param length - the second piece's length in bytes
fn crc32_combine(first: u32, second: u32, length: usize) -> u32 {
    multiply_mod(x_to_eight_n(length), first) ^ second
}

/// Multiplies two polynomials modulo the CRC polynomial, in the reflected order.
///
/// @param a - one factor
/// @param b - the other
fn multiply_mod(a: u32, b: u32) -> u32 {
    let mut product = 0u32;
    let mut b = b;
    let mut bit = 1u32 << 31;
    while bit != 0 {
        if a & bit != 0 {
            product ^= b;
        }
        b = if b & 1 != 0 {
            (b >> 1) ^ POLYNOMIAL
        } else {
            b >> 1
        };
        bit >>= 1;
    }
    product
}

/// Returns x to the power of `8 * length`, modulo the CRC polynomial.
///
/// By squaring: `X_POWERS[k]` is x to the power of `2^k`, and the bits of
/// `8 * length` say which of them to multiply together.
///
/// @param length - a byte count
fn x_to_eight_n(length: usize) -> u32 {
    let mut result = 1u32 << 31;
    let mut remaining = length;
    let mut power = 3usize;
    while remaining != 0 {
        if remaining & 1 != 0 {
            result = multiply_mod(X_POWERS.get(power % 32).copied().unwrap_or(0), result);
        }
        remaining >>= 1;
        power = power.saturating_add(1);
    }
    result
}

/// x to the power of `2^k` modulo the polynomial, for `k` from 0 to 31.
const X_POWERS: [u32; 32] = build_x_powers();

/// Builds [`X_POWERS`] by repeated squaring, at compile time.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
const fn build_x_powers() -> [u32; 32] {
    let mut powers = [0u32; 32];
    let mut value = 1u32 << 30;
    let mut index = 0usize;
    while index < 32 {
        powers[index] = value;
        value = const_multiply_mod(value, value);
        index += 1;
    }
    powers
}

/// [`multiply_mod`], for the compile time table.
#[allow(clippy::arithmetic_side_effects)]
const fn const_multiply_mod(a: u32, b: u32) -> u32 {
    let mut product = 0u32;
    let mut b = b;
    let mut bit = 1u32 << 31;
    while bit != 0 {
        if a & bit != 0 {
            product ^= b;
        }
        b = if b & 1 != 0 {
            (b >> 1) ^ POLYNOMIAL
        } else {
            b >> 1
        };
        bit >>= 1;
    }
    product
}

/// The byte-at-a-time form, kept as what the fast one is graded against.
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece
#[cfg(test)]
fn crc32_one_byte_at_a_time(previous: u32, data: &[u8]) -> u32 {
    let mut crc = !previous;
    for byte in data {
        crc = byte_entry(crc ^ u32::from(*byte)) ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PrimaryCode;
    use crate::rng::Rng;

    /// The published check value for CRC-32/ISO-HDLC over "123456789".
    #[test]
    fn crc32_matches_the_published_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// Continuing a CRC over pieces must equal computing it over the whole.
    #[test]
    fn crc32_is_the_same_whether_it_arrives_whole_or_in_pieces() {
        let mut rng = Rng::new(0x1782_0003);
        let mut data = vec![0u8; 4096];
        rng.fill(&mut data);
        let whole = crc32(&data);
        let mut running = 0;
        for chunk in data.chunks(97) {
            running = crc32_continue(running, chunk);
        }
        assert_eq!(running, whole);
    }

    /// Slice-by-eight must answer exactly what the byte-at-a-time form does.
    ///
    /// Every length from zero to sixteen, so both the eight-byte body and every
    /// remainder are covered, and a long random buffer for the body itself.
    #[test]
    fn crc32_matches_the_byte_at_a_time_form() {
        let mut rng = Rng::new(0x1833_0001);
        let mut data = vec![0u8; 8_192];
        rng.fill(&mut data);
        for length in 0..=16usize {
            let piece = data.get(..length).unwrap_or(&[]);
            assert_eq!(
                crc32(piece),
                crc32_one_byte_at_a_time(0, piece),
                "length {length}"
            );
        }
        assert_eq!(crc32(&data), crc32_one_byte_at_a_time(0, &data));
        // Long pieces take the four stream path, at every length around a
        // quarter boundary and from a nonzero start.
        for length in (4_090..4_130usize).chain([8_191, 8_192]) {
            let piece = data.get(..length).unwrap_or(&[]);
            assert_eq!(
                crc32_continue(0x1234_5678, piece),
                crc32_one_byte_at_a_time(0x1234_5678, piece),
                "length {length}"
            );
        }
        // And continuing, because the log checksums a record in pieces.
        let mut running = 0;
        let mut slow = 0;
        for chunk in data.chunks(101) {
            running = crc32_continue(running, chunk);
            slow = crc32_one_byte_at_a_time(slow, chunk);
        }
        assert_eq!(running, slow);
    }

    /// The processor's CRC is the table's CRC: every length to 300 bytes, which
    /// covers the 64 byte threshold, every block remainder and the four way
    /// folding's group boundaries, and 200 random lengths to 70 KiB, each from a
    /// random starting value.
    ///
    /// On a processor with the instructions, which every machine this runs on
    /// has, it also asserts that the hardware path answered, so a module that
    /// quietly fell back to the table everywhere would fail here.
    #[test]
    fn crc_hardware_matches_the_table_at_every_length() {
        let mut rng = Rng::new(0x2191_0004);
        let mut data = vec![0u8; 70 * 1024];
        rng.fill(&mut data);
        let mut answered = 0usize;
        let lengths: Vec<usize> = (0..=300usize)
            .chain((0..200).map(|_| (rng.next_u64() % (70 * 1024 + 1)) as usize))
            .collect();
        for length in lengths {
            let piece = data.get(..length).unwrap_or(&[]);
            let start = rng.next_u64() as u32;
            let table = crc32_continue_table(start, piece);
            if let Some(hardware) = crate::crc_hardware::crc32_continue(start, piece) {
                answered += 1;
                assert_eq!(hardware, table, "length {length} from {start:08x}");
            }
            assert_eq!(crc32_continue(start, piece), table, "length {length}");
        }
        assert!(
            answered > 400,
            "the hardware path answered {answered} of the 437 pieces of 64 bytes or more"
        );
    }

    /// Prints how long a 32 KiB page's CRC takes; run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn crc32_page_timing() {
        let mut rng = Rng::new(7);
        let mut data = vec![0u8; 32_768];
        rng.fill(&mut data);
        let started = std::time::Instant::now();
        let mut sum = 0u32;
        for _ in 0..20_000 {
            sum ^= crc32(&data);
        }
        let fast = started.elapsed().as_secs_f64() / 20_000.0;
        let started = std::time::Instant::now();
        for _ in 0..20_000 {
            sum ^= !fold_tail(!0, &data);
        }
        let chain = started.elapsed().as_secs_f64() / 20_000.0;
        println!(
            "page crc: four streams {:.2} us, one chain {:.2} us ({sum})",
            fast * 1e6,
            chain * 1e6
        );
    }

    /// A one-bit change must change the CRC; this is the property the check
    /// value alone does not prove.
    #[test]
    fn crc32_detects_single_bit_flips() {
        let mut rng = Rng::new(0x1782_0004);
        let mut data = vec![0u8; 512];
        rng.fill(&mut data);
        let baseline = crc32(&data);
        // Every byte on an ordinary build; every sixteenth under Miri, which
        // interprets each of the four thousand checksums this otherwise takes
        // and turns a millisecond into many minutes. The property - a single
        // bit flip anywhere is visible - is what is being checked, and a
        // stride still checks it across the whole buffer.
        let stride = if cfg!(miri) { 16 } else { 1 };
        for index in (0..data.len()).step_by(stride) {
            for bit in 0..8 {
                data[index] ^= 1 << bit;
                assert_ne!(
                    crc32(&data),
                    baseline,
                    "flip at {index}:{bit} was invisible"
                );
                data[index] ^= 1 << bit;
            }
        }
    }

    /// The WAL checksum reads the same bytes differently in each byte order,
    /// which is exactly why the header records which one a file uses.
    #[test]
    fn wal_checksum_depends_on_the_declared_byte_order() {
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let big = WalChecksum::default()
            .extended(&data, WalByteOrder::Big)
            .unwrap();
        let little = WalChecksum::default()
            .extended(&data, WalByteOrder::Little)
            .unwrap();
        assert_ne!(big, little);
        assert_eq!(big, WalChecksum::new(0x0102_0304, 0x0608_0a0c));
    }

    /// Checksumming a partial block is a caller mistake, not a padded answer.
    #[test]
    fn wal_checksum_refuses_a_partial_block() {
        let error = WalChecksum::default()
            .update(&[0u8; 5], WalByteOrder::Big)
            .expect_err("five bytes is not a whole block");
        assert_eq!(error.code(), PrimaryCode::Corrupt);
    }

    /// Folding in two halves must equal folding the whole, because a frame's
    /// checksum continues the one before it rather than restarting.
    #[test]
    fn wal_checksum_chains_across_calls() {
        let mut rng = Rng::new(0x1782_0005);
        let mut data = vec![0u8; 1024];
        rng.fill(&mut data);
        let whole = WalChecksum::default()
            .extended(&data, WalByteOrder::Big)
            .unwrap();
        let mut running = WalChecksum::default();
        running.update(&data[..512], WalByteOrder::Big).unwrap();
        running.update(&data[512..], WalByteOrder::Big).unwrap();
        assert_eq!(running, whole);
    }
}
