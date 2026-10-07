//! CRC-32/ISO-HDLC computed by the processor's own instructions, for the
//! buffers long enough to be worth it.
//!
//! Invariant: **every answer is the one the table implementation in
//! [`crate::checksum`] gives for the same input, bit for bit**, so no page or
//! log record written before reads differently. The polynomial is the reflected
//! `0xEDB88320` in both. `crc_hardware_matches_the_table_at_every_length` in
//! `checksum`'s tests checks it over every length up to 300 bytes and random
//! lengths up to 70 KiB, from random starting values.
//!
//! **Why this module is allowed `unsafe` (task-2191).** Checking a page as it
//! is read was 20% of a cold query of 100 rows from the command line, and the
//! per record checksum of the log a large share of a 10,000 row insert. The
//! table implementation runs at about 8 GB/s; x86-64's carryless multiply
//! (`PCLMULQDQ`) and aarch64's `CRC32X` run several times faster. Both are
//! reached only through `std::arch` intrinsics, which are `unsafe` because
//! executing one on a processor without the feature is undefined. The risk is
//! confined three ways: each intrinsic runs only inside a function compiled with
//! `#[target_feature]` and called only after the matching run time check
//! answered yes; every load reads a whole 16 byte block borrowed from the input
//! slice, never past it; and a processor without the feature, or a short
//! buffer, gets `None` and the caller uses the table.

/// Below this, the table implementation's setup is cheaper than the folding's.
const SHORTEST: usize = 64;

/// CRC-32 of `data` continued from `previous`, by the processor, or `None` when
/// the buffer is short or the processor lacks the instructions.
///
/// `previous` and the answer are finished values, as in
/// [`crate::checksum::crc32_continue`]: zero to start.
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece
#[cfg(target_arch = "x86_64")]
pub(crate) fn crc32_continue(previous: u32, data: &[u8]) -> Option<u32> {
    if data.len() < SHORTEST
        || !std::arch::is_x86_feature_detected!("pclmulqdq")
        || !std::arch::is_x86_feature_detected!("sse4.1")
    {
        return None;
    }
    // SAFETY: both features this function is compiled with were detected on
    // the running processor immediately above.
    Some(unsafe { x86::fold(previous, data) })
}

/// CRC-32 of `data` continued from `previous`, by the processor, or `None` when
/// the buffer is short or the processor lacks the instructions.
///
/// @param previous - the result so far, or zero to start
/// @param data - the next piece
#[cfg(target_arch = "aarch64")]
pub(crate) fn crc32_continue(previous: u32, data: &[u8]) -> Option<u32> {
    if data.len() < SHORTEST || !std::arch::is_aarch64_feature_detected!("crc") {
        return None;
    }
    // SAFETY: the one feature this function is compiled with was detected on
    // the running processor immediately above.
    Some(unsafe { arm::crc(previous, data) })
}

/// No instruction for this target; the table answers.
///
/// @param _previous - the result so far
/// @param _data - the next piece
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub(crate) fn crc32_continue(_previous: u32, _data: &[u8]) -> Option<u32> {
    None
}

/// The carryless multiply folding, after Intel's "Fast CRC Computation for
/// Generic Polynomials Using PCLMULQDQ Instruction", in the bit reflected form
/// zlib, Linux and the `crc32fast` crate use for this polynomial.
#[cfg(target_arch = "x86_64")]
mod x86 {
    use std::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_clmulepi64_si128, _mm_cvtsi32_si128, _mm_extract_epi32,
        _mm_loadu_si128, _mm_set_epi32, _mm_set_epi64x, _mm_srli_si128, _mm_xor_si128,
    };

    /// x^(4*128+32) mod P and x^(4*128-32) mod P, reflected: folds 64 bytes.
    const K1: i64 = 0x1_5444_2bd4;
    /// The pair's other half.
    const K2: i64 = 0x1_c6e4_1596;
    /// x^(128+32) mod P and x^(128-32) mod P, reflected: folds 16 bytes.
    const K3: i64 = 0x1_7519_97d0;
    /// The pair's other half.
    const K4: i64 = 0x0_ccaa_009e;
    /// x^64 mod P, reflected: from 96 bits to 64.
    const K5: i64 = 0x1_63cd_6124;
    /// The polynomial itself, reflected, with its x^32 term.
    const P_X: i64 = 0x1_DB71_0641;
    /// floor(x^64 / P), reflected: Barrett reduction's constant.
    const U_PRIME: i64 = 0x1_F701_1641;

    /// Folds `data`, at least 64 bytes long, into a CRC continued from `previous`.
    ///
    /// @param previous - the finished result so far
    /// @param data - the buffer
    ///
    /// # Safety
    ///
    /// The processor must have `pclmulqdq` and `sse4.1`.
    #[target_feature(enable = "pclmulqdq,sse4.1")]
    pub(super) unsafe fn fold(previous: u32, data: &[u8]) -> u32 {
        let (blocks, tail) = data.as_chunks::<16>();
        let (first, rest) = blocks.split_at(blocks.len().min(4));
        let [b0, b1, b2, b3] = first else {
            // `crc32_continue` admits 64 bytes or more, so four blocks are there.
            return crate::checksum::crc32_continue_table(previous, data);
        };
        // The running value enters as an xor into the first block, inverted
        // the way the table form keeps it.
        let mut x3 = _mm_xor_si128(load(b0), _mm_cvtsi32_si128(!previous as i32));
        let mut x2 = load(b1);
        let mut x1 = load(b2);
        let mut x0 = load(b3);
        let k1k2 = _mm_set_epi64x(K2, K1);
        let groups = rest.chunks_exact(4);
        let singles = groups.remainder();
        for group in groups {
            if let [g0, g1, g2, g3] = group {
                x3 = reduce(x3, load(g0), k1k2);
                x2 = reduce(x2, load(g1), k1k2);
                x1 = reduce(x1, load(g2), k1k2);
                x0 = reduce(x0, load(g3), k1k2);
            }
        }
        let k3k4 = _mm_set_epi64x(K4, K3);
        let mut x = reduce(x3, x2, k3k4);
        x = reduce(x, x1, k3k4);
        x = reduce(x, x0, k3k4);
        for block in singles {
            x = reduce(x, load(block), k3k4);
        }
        // 128 bits to 64: the low half times K4, xored with the high half,
        // then the low 32 bits of that times K5, xored with the rest.
        let low32 = _mm_set_epi32(0, 0, 0, -1);
        let x = _mm_xor_si128(_mm_clmulepi64_si128(x, k3k4, 0x10), _mm_srli_si128(x, 8));
        let x = _mm_xor_si128(
            _mm_clmulepi64_si128(_mm_and_si128(x, low32), _mm_set_epi64x(0, K5), 0x00),
            _mm_srli_si128(x, 4),
        );
        // Barrett reduction from 64 bits to the 32 bit remainder, reflected.
        let pu = _mm_set_epi64x(U_PRIME, P_X);
        let t1 = _mm_clmulepi64_si128(_mm_and_si128(x, low32), pu, 0x10);
        let t2 = _mm_clmulepi64_si128(_mm_and_si128(t1, low32), pu, 0x00);
        let folded = !(_mm_extract_epi32(_mm_xor_si128(x, t2), 1) as u32);
        crate::checksum::crc32_continue_table(folded, tail)
    }

    /// Folds `a` forward by the distance the keys encode and adds `b`.
    ///
    /// @param a - the running 128 bits
    /// @param b - the next block
    /// @param keys - the pair of folding constants
    #[target_feature(enable = "pclmulqdq,sse4.1")]
    fn reduce(a: __m128i, b: __m128i, keys: __m128i) -> __m128i {
        let low = _mm_clmulepi64_si128(a, keys, 0x00);
        let high = _mm_clmulepi64_si128(a, keys, 0x11);
        _mm_xor_si128(_mm_xor_si128(b, low), high)
    }

    /// Loads one 16 byte block.
    ///
    /// @param block - sixteen bytes borrowed from the input
    #[target_feature(enable = "pclmulqdq,sse4.1")]
    fn load(block: &[u8; 16]) -> __m128i {
        // SAFETY: `block` is sixteen readable bytes for the length of the
        // call, and the unaligned load reads exactly those sixteen.
        unsafe { _mm_loadu_si128(block.as_ptr().cast::<__m128i>()) }
    }
}

/// The ARMv8 CRC32 instructions, which compute this same reflected polynomial
/// (the `crc32c` ones are the other polynomial and are not used).
#[cfg(target_arch = "aarch64")]
mod arm {
    use std::arch::aarch64::{__crc32b, __crc32d};

    /// CRC-32 of `data` continued from `previous`, eight bytes an instruction.
    ///
    /// @param previous - the finished result so far
    /// @param data - the buffer
    ///
    /// # Safety
    ///
    /// The processor must have the `crc` feature.
    #[target_feature(enable = "crc")]
    pub(super) unsafe fn crc(previous: u32, data: &[u8]) -> u32 {
        let (words, tail) = data.as_chunks::<8>();
        let mut state = !previous;
        for word in words {
            state = __crc32d(state, u64::from_le_bytes(*word));
        }
        for byte in tail {
            state = __crc32b(state, *byte);
        }
        !state
    }
}
