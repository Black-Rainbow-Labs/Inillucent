//! `sinh`, `cosh`, `tanh`, `asin` and `acos` as the pinned Windows shell computes them.
//!
//! Invariant: **on Windows each function here returns the same bits as the pinned `sqlite3.exe`
//! for every argument.** That shell links the C runtime statically, and the static runtime's
//! versions of these five functions are AMD's libm. The `ucrtbase.dll` that Rust's `f64` methods
//! call on Windows uses different code, and differed from the shell in the last bit for 109 to
//! 1,695 of 40,000 sampled arguments, depending on the function. A program built with `cl /MT`
//! matched the shell for all of them, and so does this port.
//!
//! On Linux the pinned shell calls the system libm, which is what Rust's `f64` methods call there
//! too, so `mathfn` uses this module on Windows only. It is compiled everywhere so its tests run
//! everywhere: the arithmetic is plain IEEE double arithmetic with no fused multiply and add, and
//! gives the same bits on every platform.
//!
//! Ported from AMD's reference implementations in aocl-libm-ose (`src/ref/sinh.c`, `cosh.c`,
//! `tanh.c`, `asin.c`, `acos.c` and `splitexp` in `include/libm_inlines_amd.h`). The constants,
//! the tables and the order of every operation are AMD's, because the order is what decides the
//! last bit. Their licence asks for this notice:
//!
//! Copyright (C) 2008-2022 Advanced Micro Devices, Inc. All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without modification, are
//! permitted provided that the following conditions are met:
//! 1. Redistributions of source code must retain the above copyright notice, this list of
//!    conditions and the following disclaimer.
//! 2. Redistributions in binary form must reproduce the above copyright notice, this list of
//!    conditions and the following disclaimer in the documentation and/or other materials
//!    provided with the distribution.
//! 3. Neither the name of the copyright holder nor the names of its contributors may be used to
//!    endorse or promote products derived from this software without specific prior written
//!    permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR
//! IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY
//! AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
//! CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
//! CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//! SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
//! THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
//! OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
//! POSSIBILITY OF SUCH DAMAGE.

/// `32 / ln 2`, which turns an argument into a count of 32nds of a power of two.
const THIRTYTWO_BY_LOG2: f64 = 46.16624130844683;
/// The leading bits of `ln 2 / 32`, few enough that a product with the count is exact.
const LOG2_BY_32_LEAD: f64 = 0.021660849335603416;
/// The rest of `ln 2 / 32`.
const LOG2_BY_32_TAIL: f64 = 5.689487495325456e-11;
/// The argument at and beyond which `sinh` and `cosh` overflow to infinity.
const MAX_HYPERBOLIC_ARGUMENT: f64 = 710.475860073944;
/// The argument from which `cosh` is `exp(|x|) / 2`, as in AMD's `cosh.c`.
const COSH_EXPONENTIAL_ONLY: f64 = 20.0;
/// The argument from which `sinh` is `exp(|x|) / 2`.
///
/// AMD's published `sinh.c` uses 127.6 here, which would index past its 37 row tables, and 2,584
/// of 60,000 sampled arguments then differed from the shell. The static runtime the shell links
/// switches just above 36. The value was measured: the shell agrees only with the table path at
/// 36.1218003 and only with the exponential path at 36.12360180000032, and every argument between
/// that was tried gives the same bits on both paths.
const SINH_EXPONENTIAL_ONLY: f64 = 36.123_601_8;
/// `pi / 2`.
const PI_BY_2: f64 = core::f64::consts::FRAC_PI_2;
/// The part of `pi / 2` that [`PI_BY_2`] cannot hold.
const PI_BY_2_TAIL: f64 = 6.123233995736766e-17;
/// `pi / 4`.
const HALF_PI_BY_2: f64 = core::f64::consts::FRAC_PI_4;
/// `pi`.
const PI: f64 = core::f64::consts::PI;
/// The leading 26 bits of `sinh(i)` for `i` from 0 to 36.
const SINH_LEAD: [u64; 37] = [
    0x0000000000000000,
    0x3ff2cd9fc0000000,
    0x400d03cf60000000,
    0x40240926e0000000,
    0x403b4a3800000000,
    0x40528d0160000000,
    0x406936d228000000,
    0x4081228768000000,
    0x409749ea50000000,
    0x40afa71570000000,
    0x40c5829dc8000000,
    0x40dd3c4488000000,
    0x40f3de1650000000,
    0x410b00b590000000,
    0x412259ac48000000,
    0x4138f0cca8000000,
    0x4150f2ebd0000000,
    0x4167093488000000,
    0x417f4f2208000000,
    0x419546d8f8000000,
    0x41aceb0888000000,
    0x41c3a6e1f8000000,
    0x41dab5adb8000000,
    0x41f226af30000000,
    0x4208ab7fb0000000,
    0x4220c3d390000000,
    0x4236c93268000000,
    0x424ef822f0000000,
    0x42650bba30000000,
    0x427c9aae40000000,
    0x4293704708000000,
    0x42aa6b7658000000,
    0x42c1f43fc8000000,
    0x42d866f348000000,
    0x42f0953e28000000,
    0x430689e220000000,
    0x431ea215a0000000,
];
/// The rest of `sinh(i)` for `i` from 0 to 36.
const SINH_TAIL: [u64; 37] = [
    0x0000000000000000,
    0x3e513ae6096a0092,
    0x3e5db70cfb79a640,
    0x3e8c2526b66dc067,
    0x3e8b81b18647f380,
    0x3ebbc1cdd1e1eb08,
    0x3ecd9f201534fb09,
    0x3edd1c064a4e9954,
    0x3ed4eca65d06ea74,
    0x3f00c259bcc0ecc5,
    0x3f2b5a6647cf9016,
    0x3f09691adefb0870,
    0x3f53410fc29cde38,
    0x3f46a31a50b6fb3c,
    0x3f57defc71805c40,
    0x3f9eb49fd80e0bab,
    0x3f84fffc7bcd5920,
    0x3fc03a93b6c63435,
    0x3fb1940bb255fd1c,
    0x3fded26e14260b50,
    0x3ffb47401fc9f2a2,
    0x40267bb3f55634f1,
    0x401c435ff8194ddc,
    0x404d8fee052ba63a,
    0x40651d7edccde3f6,
    0x40704b1644557d1a,
    0x4076a6b5ca0a9dc4,
    0x40afd9cc72249aba,
    0x40ce58de693edab5,
    0x40d8c70158ac6363,
    0x40e7614764f43e20,
    0x4106337db36fc718,
    0x41212d98b1f611e2,
    0x412392bc108b37cc,
    0x415ce87bdc3473dc,
    0x414bc8d5ae99ad14,
    0x415d20d76744835c,
];
/// The leading 26 bits of `cosh(i)` for `i` from 0 to 36.
const COSH_LEAD: [u64; 37] = [
    0x3ff0000000000000,
    0x3ff8b07550000000,
    0x400e18fa08000000,
    0x402422a490000000,
    0x403b4ee858000000,
    0x40528d6fc8000000,
    0x406936e678000000,
    0x4081228948000000,
    0x409749eaa8000000,
    0x40afa71580000000,
    0x40c5829dd0000000,
    0x40dd3c4488000000,
    0x40f3de1650000000,
    0x410b00b590000000,
    0x412259ac48000000,
    0x4138f0cca8000000,
    0x4150f2ebd0000000,
    0x4167093488000000,
    0x417f4f2208000000,
    0x419546d8f8000000,
    0x41aceb0888000000,
    0x41c3a6e1f8000000,
    0x41dab5adb8000000,
    0x41f226af30000000,
    0x4208ab7fb0000000,
    0x4220c3d390000000,
    0x4236c93268000000,
    0x424ef822f0000000,
    0x42650bba30000000,
    0x427c9aae40000000,
    0x4293704708000000,
    0x42aa6b7658000000,
    0x42c1f43fc8000000,
    0x42d866f348000000,
    0x42f0953e28000000,
    0x430689e220000000,
    0x431ea215a0000000,
];
/// The rest of `cosh(i)` for `i` from 0 to 36.
const COSH_TAIL: [u64; 37] = [
    0x0000000000000000,
    0x3e3d9f5504c2bd28,
    0x3e67cb66f0a4c9fd,
    0x3e8f58617928e588,
    0x3e6bc7d000c38d48,
    0x3eaf7f9d4e329998,
    0x3ec6e6e464885269,
    0x3ecba3a8b946c154,
    0x3ed3f4e76110d5a4,
    0x3f017622515a3e2b,
    0x3ee4dc4b528af3d0,
    0x3f11156278615e10,
    0x3f535ad50ed821f5,
    0x3f46b61055f2935c,
    0x3f57e2794a601240,
    0x3f9eb4b45f6aadd3,
    0x3f85000b967b3698,
    0x3fc03a940fadc092,
    0x3fb1940bf3bf874c,
    0x3fded26e1a2a2110,
    0x3ffb4740205796d6,
    0x40267bb3f55cb85d,
    0x401c435ff81e18ac,
    0x404d8fee052bdea4,
    0x40651d7edccde926,
    0x40704b1644557e0e,
    0x4076a6b5ca0a9e1c,
    0x40afd9cc72249abe,
    0x40ce58de693edab5,
    0x40d8c70158ac6364,
    0x40e7614764f43e20,
    0x4106337db36fc718,
    0x41212d98b1f611e2,
    0x412392bc108b37cc,
    0x415ce87bdc3473dc,
    0x414bc8d5ae99ad14,
    0x415d20d76744835c,
];
/// The leading 25 bits of `2^(j/32)` for `j` from 0 to 31.
const TWO_TO_J_BY_32_LEAD: [u64; 32] = [
    0x3ff0000000000000,
    0x3ff059b0d0000000,
    0x3ff0b55860000000,
    0x3ff11301d0000000,
    0x3ff172b830000000,
    0x3ff1d48730000000,
    0x3ff2387a60000000,
    0x3ff29e9df0000000,
    0x3ff306fe00000000,
    0x3ff371a730000000,
    0x3ff3dea640000000,
    0x3ff44e0860000000,
    0x3ff4bfdad0000000,
    0x3ff5342b50000000,
    0x3ff5ab07d0000000,
    0x3ff6247eb0000000,
    0x3ff6a09e60000000,
    0x3ff71f75e0000000,
    0x3ff7a11470000000,
    0x3ff8258990000000,
    0x3ff8ace540000000,
    0x3ff93737b0000000,
    0x3ff9c49180000000,
    0x3ffa5503b0000000,
    0x3ffae89f90000000,
    0x3ffb7f76f0000000,
    0x3ffc199bd0000000,
    0x3ffcb720d0000000,
    0x3ffd5818d0000000,
    0x3ffdfc9730000000,
    0x3ffea4afa0000000,
    0x3fff507650000000,
];
/// The next 53 bits of `2^(j/32)` for `j` from 0 to 31.
const TWO_TO_J_BY_32_TRAIL: [u64; 32] = [
    0x0000000000000000,
    0x3e48ac2ba1d73e2a,
    0x3e69f3121ec53172,
    0x3df25b50a4ebbf1b,
    0x3e68faa2f5b9bef9,
    0x3e368b9aa7805b80,
    0x3e6ceac470cd83f6,
    0x3e547f7b84b09745,
    0x3e64636e2a5bd1ab,
    0x3e5ceaa72a9c5154,
    0x3e682468446b6824,
    0x3e18624b40c4dbd0,
    0x3e54d8a89c750e5e,
    0x3e5a753e077c2a0f,
    0x3e6a90a852b19260,
    0x3e0d2ac258f87d03,
    0x3e59fcef32422cbf,
    0x3e61d8bee7ba46e2,
    0x3e4f580c36bea881,
    0x3e62999c25159f11,
    0x3e415506dadd3e2a,
    0x3e29b8bc9e8a0388,
    0x3e451f8480e3e236,
    0x3e41f12ae45a1224,
    0x3e62b5a75abd0e6a,
    0x3e47daf237553d84,
    0x3e6b0aa538444196,
    0x3e69df20d22a0798,
    0x3e69f7490e4bb40b,
    0x3e4bdcdaf5cb4656,
    0x3e452486cc2c7b9d,
    0x3e66dc8a80ce9f09,
];

/// Returns a table entry as a double, or zero for an index past the end.
///
/// Every caller indexes with a value it has already bounded, so zero is never returned.
///
/// @param values - the table, as bit patterns
/// @param index - the entry
fn entry(values: &[u64], index: usize) -> f64 {
    f64::from_bits(values.get(index).copied().unwrap_or(0))
}

/// Returns `2^n` for an exponent a double can hold.
///
/// @param n - the exponent, from -1022 to 1023
fn power_of_two(n: i32) -> f64 {
    f64::from_bits(((i64::from(n) + 1023) as u64) << 52)
}

/// Returns `x * 2^m`, in two steps when `2^m` alone is out of range.
///
/// @param x - the value
/// @param m - the exponent
fn scaled(x: f64, m: i32) -> f64 {
    if (-1022..=1023).contains(&m) {
        return x * power_of_two(m);
    }
    let first = m / 2;
    (x * power_of_two(first)) * power_of_two(m - first)
}

/// Splits `exp(x)` into `2^m * (z1 + z2)`, AMD's `splitexp`.
///
/// @param x - the argument, between 1/16 and the overflow bound
fn split_exp(x: f64) -> (i32, f64, f64) {
    let r = x * THIRTYTWO_BY_LOG2;
    let n = if r > 0.0 {
        (r + 0.5) as i32
    } else {
        (r - 0.5) as i32
    };
    let r1 = x - f64::from(n) * LOG2_BY_32_LEAD;
    let r2 = -f64::from(n) * LOG2_BY_32_TAIL;
    let j = n & 0x1f;
    let f1 = entry(&TWO_TO_J_BY_32_LEAD, j as usize);
    let f2 = entry(&TWO_TO_J_BY_32_TRAIL, j as usize);
    let m = (n - j) / 32;
    let r = r1 + r2;
    let q = r1
        + (r2
            + r * r
                * (0.5
                    + r * (0.16666666666526087
                        + r * (0.04166666666622608
                            + r * (0.008333367984342196 + r * 0.001388894908637772)))));
    (m, f1, f2 + ((f1 + f2) * q))
}

/// Returns `sinh(dy) - dy` for a fraction `dy`, given `dy * dy`.
///
/// @param dy - the fraction, from 0 to 1
/// @param dy2 - its square
fn sinh_less_argument(dy: f64, dy2: f64) -> f64 {
    dy * dy2
        * (0.16666666666666666
            + (0.008333333333333299
                + (0.0001984126984132424
                    + (2.7557319191363643e-06
                        + (2.5052117699413348e-08
                            + (1.605767931219399e-10 + 7.746188980094184e-13 * dy2) * dy2)
                            * dy2)
                        * dy2)
                    * dy2)
                * dy2)
}

/// Returns `cosh(dy) - 1` for a fraction, given its square.
///
/// @param dy2 - the square of the fraction
fn cosh_less_one(dy2: f64) -> f64 {
    dy2 * (0.5
        + (0.04166666666666609
            + (0.0013888888888981485
                + (2.4801587246062242e-05
                    + (2.755733507560166e-07
                        + (2.0874434983147137e-09 + 1.1639213881721737e-11 * dy2) * dy2)
                        * dy2)
                    * dy2)
                * dy2)
            * dy2)
}

/// The hyperbolic sine.
///
/// @param x - the argument
pub(crate) fn sinh(x: f64) -> f64 {
    let magnitude_bits = x.to_bits() & !(1u64 << 63);
    if !(0x3e30_0000_0000_0000..0x7ff0_0000_0000_0000).contains(&magnitude_bits) {
        return x;
    }
    let y = x.abs();
    let z = if y >= MAX_HYPERBOLIC_ARGUMENT {
        f64::INFINITY
    } else if y >= SINH_EXPONENTIAL_ONLY {
        let (m, z1, z2) = split_exp(y);
        scaled(z1 + z2, m - 1)
    } else {
        let whole = y as usize;
        let dy = y - whole as f64;
        let dy2 = dy * dy;
        let sdy = sinh_less_argument(dy, dy2);
        let cdy = cosh_less_one(dy2);
        let sdy1 = f64::from_bits(dy.to_bits() & 0xffff_ffff_f800_0000);
        let sdy2 = sdy + (dy - sdy1);
        let sinh_lead = entry(&SINH_LEAD, whole);
        let sinh_tail = entry(&SINH_TAIL, whole);
        let cosh_lead = entry(&COSH_LEAD, whole);
        let cosh_tail = entry(&COSH_TAIL, whole);
        ((((((cosh_tail * sdy2 + sinh_tail * cdy) + cosh_tail * sdy1) + sinh_tail)
            + cosh_lead * sdy2)
            + sinh_lead * cdy)
            + cosh_lead * sdy1)
            + sinh_lead
    };
    if x.is_sign_negative() {
        -z
    } else {
        z
    }
}

/// The hyperbolic cosine.
///
/// @param x - the argument
pub(crate) fn cosh(x: f64) -> f64 {
    let magnitude_bits = x.to_bits() & !(1u64 << 63);
    if magnitude_bits < 0x3e30_0000_0000_0000 {
        return 1.0;
    }
    if magnitude_bits >= 0x7ff0_0000_0000_0000 {
        return x.abs();
    }
    let y = x.abs();
    if y >= MAX_HYPERBOLIC_ARGUMENT {
        return f64::INFINITY;
    }
    if y >= COSH_EXPONENTIAL_ONLY {
        let (m, z1, z2) = split_exp(y);
        return scaled(z1 + z2, m - 1);
    }
    let whole = y as usize;
    let dy = y - whole as f64;
    let dy2 = dy * dy;
    let sdy = sinh_less_argument(dy, dy2);
    let cdy = cosh_less_one(dy2);
    let sinh_lead = entry(&SINH_LEAD, whole);
    let sinh_tail = entry(&SINH_TAIL, whole);
    let cosh_lead = entry(&COSH_LEAD, whole);
    let cosh_tail = entry(&COSH_TAIL, whole);
    ((((((cosh_tail * cdy + sinh_tail * sdy) + sinh_tail * dy) + cosh_tail) + cosh_lead * cdy)
        + sinh_lead * sdy)
        + sinh_lead * dy)
        + cosh_lead
}

/// The hyperbolic tangent.
///
/// @param x - the argument
pub(crate) fn tanh(x: f64) -> f64 {
    let magnitude_bits = x.to_bits() & !(1u64 << 63);
    if !(0x3e30_0000_0000_0000..=0x7ff0_0000_0000_0000).contains(&magnitude_bits) {
        return x;
    }
    let y = x.abs();
    let z = if y > 20.0 {
        1.0
    } else if y <= 1.0 {
        tanh_near_zero(y)
    } else {
        let (m, z1, z2) = split_exp(2.0 * y);
        let first = m / 2;
        let p = ((z1 + z2) * power_of_two(first)) * power_of_two(m - first) + 1.0;
        1.0 - 2.0 / p
    };
    if x.is_sign_negative() {
        -z
    } else {
        z
    }
}

/// The hyperbolic tangent of a magnitude up to 1, as two rational approximations.
///
/// @param y - the magnitude, from 2^-28 to 1
fn tanh_near_zero(y: f64) -> f64 {
    let y2 = y * y;
    if y < 0.9 {
        return y + y
            * y2
            * (-0.27403042465617977
                + (-0.017601634900304468
                    + (-0.0002000476210719095 - 1.4207792637883471e-08 * y2) * y2)
                    * y2)
            / (0.8220912739685393
                + (0.3816414142883289
                    + (0.020156216602693764 + 0.00020911402625291644 * y2) * y2)
                    * y2);
    }
    y + y
        * y2
        * (-0.2277938706590883
            + (-0.014617304728873168 + (-0.00016559704390354995 - 1.154758789961434e-08 * y2) * y2)
                * y2)
        / (0.6833816119772959
            + (0.3172045589772944 + (0.016735877546189656 + 0.00017307605012622596 * y2) * y2) * y2)
}

/// `(asin(v) - v) / v^3` as a rational function of `r = v^2`, for `v` up to 1/2.
///
/// @param r - the square of the reduced argument
fn arcsine_correction(r: f64) -> f64 {
    r * (0.22748583555693502
        + (-0.4450172168676356
            + (0.27555817525693765
                + (-0.054998980923568586
                    + (0.0010924269723507467 + 4.82901920344787e-05 * r) * r)
                    * r)
                * r)
            * r)
        / (1.3649150133416104
            + (-3.2843150572095867
                + (2.76568859157271 + (-0.9436391370324927 + 0.10586942208720437 * r) * r) * r)
                * r)
}

/// Returns the unbiased binary exponent of a double.
///
/// @param x - the value
fn exponent(x: f64) -> i32 {
    ((x.to_bits() >> 52) & 0x7ff) as i32 - 1023
}

/// The reduction both inverse functions use for a magnitude of 1/2 or more.
///
/// Returns `r = (1 - y) / 2`, its square root `s`, and `s` with its low 32 bits cleared.
///
/// @param y - the magnitude, from 1/2 to 1
fn half_complement(y: f64) -> (f64, f64, f64) {
    let r = 0.5 * (1.0 - y);
    let s = r.sqrt();
    (r, s, f64::from_bits(s.to_bits() & 0xffff_ffff_0000_0000))
}

/// The inverse sine. NaN outside [-1, 1].
///
/// @param x - the argument
pub(crate) fn asin(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let power = exponent(x);
    if power < -28 {
        return x;
    }
    if power >= 0 {
        return if x == 1.0 {
            PI_BY_2
        } else if x == -1.0 {
            -PI_BY_2
        } else {
            f64::NAN
        };
    }
    let y = x.abs();
    let v = if power >= -1 {
        let (r, s, s1) = half_complement(y);
        let u = arcsine_correction(r);
        let c = (r - s1 * s1) / (s + s1);
        let p = 2.0 * s * u - (PI_BY_2_TAIL - 2.0 * c);
        let q = HALF_PI_BY_2 - 2.0 * s1;
        HALF_PI_BY_2 - (p - q)
    } else {
        let u = arcsine_correction(y * y);
        y + y * u
    };
    if x.is_sign_negative() {
        -v
    } else {
        v
    }
}

/// The inverse cosine. NaN outside [-1, 1].
///
/// @param x - the argument
pub(crate) fn acos(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let power = exponent(x);
    if power < -56 {
        return PI_BY_2;
    }
    if power >= 0 {
        return if x == 1.0 {
            0.0
        } else if x == -1.0 {
            PI
        } else {
            f64::NAN
        };
    }
    if power < -1 {
        let u = arcsine_correction(x * x);
        return PI_BY_2 - (x - (PI_BY_2_TAIL - x * u));
    }
    let (r, s, s1) = half_complement(x.abs());
    let u = arcsine_correction(r);
    if x.is_sign_negative() {
        return PI - 2.0 * (s + (s * u - PI_BY_2_TAIL));
    }
    let c = (r - s1 * s1) / (s + s1);
    2.0 * s1 + (2.0 * c + 2.0 * s * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arguments where Windows' `ucrtbase.dll` and the pinned shell disagree in the last bit,
    /// with the bits the shell returns. Taken from 330,000 random arguments compared with
    /// `hex(ieee754_to_blob(f(x)))` in the pinned 3.53.4 shell.
    const SHELL: [(&str, u64, u64); 15] = [
        ("sinh", 0xc065d052cf945415, 0xcf9b2ffc26c78703),
        ("sinh", 0xc0814ed6eef1503f, 0xf1d07e38dbe7a0be),
        ("sinh", 0xc029c4809d0129b7, 0xc1080a53a9336c63),
        ("cosh", 0x4020f9270a31bc79, 0x40a2f179f4d5faa9),
        ("cosh", 0xc02a9b184df6fb40, 0x4112473bb64aa930),
        ("cosh", 0xc019d4aaaa796aef, 0x4073ecaa5ae13af7),
        ("tanh", 0xbfed627ba9fd3b95, 0xbfe733d8906229c1),
        ("tanh", 0x3fed87059fef4c94, 0x3fe7451e73136a4a),
        ("tanh", 0x3feba504a2b4c7df, 0x3fe658220f5ee7fc),
        ("asin", 0xbfe2ee399826bf2b, 0xbfe441ada98eea49),
        ("asin", 0xbfe427cdbc28a6d8, 0xbfe5cdcb8690f953),
        ("asin", 0xbfe3db5f9fb31170, 0xbfe56bdd6b643c6a),
        ("acos", 0x3fe499b3288394a9, 0x3febe270e64f518b),
        ("acos", 0x3fe6cb6647a806ed, 0x3fe8e54144fd5ab1),
        ("acos", 0x3fe6b43a2b325065, 0x3fe906343c1b1586),
    ];

    /// Calls the port of one function by name.
    ///
    /// @param name - the function
    /// @param x - the argument
    fn call(name: &str, x: f64) -> f64 {
        match name {
            "sinh" => sinh(x),
            "cosh" => cosh(x),
            "tanh" => tanh(x),
            "asin" => asin(x),
            _ => acos(x),
        }
    }

    /// Each port returns the shell's bits where the dynamic runtime does not.
    #[test]
    fn the_port_returns_the_shells_bits() {
        for (name, argument, expected) in SHELL {
            let got = call(name, f64::from_bits(argument)).to_bits();
            assert_eq!(got, expected, "{name}({})", f64::from_bits(argument));
        }
    }

    /// The edges: zero keeps its sign, the overflow bound gives infinity, and an argument
    /// outside [-1, 1] has no inverse sine or cosine.
    #[test]
    fn the_edges_follow_the_c_library() {
        assert_eq!(sinh(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(cosh(0.0), 1.0);
        assert_eq!(sinh(800.0), f64::INFINITY);
        assert_eq!(sinh(-800.0), f64::NEG_INFINITY);
        assert_eq!(cosh(-800.0), f64::INFINITY);
        assert_eq!(tanh(-30.0), -1.0);
        assert_eq!(asin(1.0), PI_BY_2);
        assert_eq!(acos(-1.0), PI);
        assert_eq!(acos(1.0), 0.0);
        assert!(asin(1.5).is_nan());
        assert!(acos(-1.5).is_nan());
    }
}
