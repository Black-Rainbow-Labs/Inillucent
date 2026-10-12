//! The case folding and diacritic removal tables of the pinned SQLite's `unicode61` tokenizer.
//!
//! Invariant: `fold` returns for every code point what `sqlite3Fts5UnicodeFold` returns in the
//! pinned 3.53.4 release, for each of the three `remove_diacritics` settings. The tables are
//! copied from `fts5_unicode2.c`, whose header says they are generated from the Unicode
//! `CaseFolding.txt`, so they are data and not code to reason about. They are not derived from
//! Rust's `char::to_lowercase`, which differs from them: U+0130 lowercases to two characters
//! there, and U+0131 is a letter with no diacritic that Rust's table has no rule to fold at all.

/// Code points that map to an ASCII letter when `remove_diacritics` is on: the start of each range
/// in the high bits and the range length in the low three. Index n pairs with `DIACRITIC_TARGET`.
const DIACRITIC_RANGE: [u16; 126] = [
    0, 1797, 1848, 1859, 1891, 1928, 1940, 1995, 2024, 2040, 2060, 2110, 2168, 2206, 2264, 2286,
    2344, 2383, 2472, 2488, 2516, 2596, 2668, 2732, 2782, 2842, 2894, 2954, 2984, 3000, 3028, 3336,
    3456, 3696, 3712, 3728, 3744, 3766, 3832, 3896, 3912, 3928, 3944, 3968, 4008, 4040, 4056, 4106,
    4138, 4170, 4202, 4234, 4266, 4296, 4312, 4344, 4408, 4424, 4442, 4472, 4488, 4504, 6148, 6198,
    6264, 6280, 6360, 6429, 6505, 6529, 61448, 61468, 61512, 61534, 61592, 61610, 61642, 61672,
    61688, 61704, 61726, 61784, 61800, 61816, 61836, 61880, 61896, 61914, 61948, 61998, 62062,
    62122, 62154, 62184, 62200, 62218, 62252, 62302, 62364, 62410, 62442, 62478, 62536, 62554,
    62584, 62604, 62640, 62648, 62656, 62664, 62730, 62766, 62830, 62890, 62924, 62974, 63032,
    63050, 63082, 63118, 63182, 63242, 63274, 63310, 63368, 63390,
];

/// The letter each range of `DIACRITIC_RANGE` becomes. The high bit marks a range that only
/// `remove_diacritics=2` removes.
const DIACRITIC_TARGET: [u8; 126] = [
    0, 97, 99, 101, 105, 110, 111, 117, 121, 121, 97, 99, 100, 101, 101, 103, 104, 105, 106, 107,
    108, 110, 111, 114, 115, 116, 117, 117, 119, 121, 122, 111, 117, 97, 105, 111, 117, 245, 225,
    103, 107, 111, 239, 106, 103, 110, 225, 97, 101, 105, 111, 114, 117, 115, 116, 104, 97, 101,
    239, 111, 239, 121, 0, 0, 0, 0, 0, 0, 0, 0, 97, 98, 227, 100, 100, 229, 101, 229, 102, 103,
    104, 104, 105, 233, 107, 108, 236, 108, 109, 110, 239, 112, 114, 242, 114, 115, 243, 116, 117,
    245, 118, 119, 119, 120, 121, 122, 104, 116, 119, 121, 97, 225, 225, 225, 101, 229, 229, 105,
    111, 239, 239, 239, 117, 245, 245, 121,
];

/// One case folding rule: the first code point, the flags, and how many code points it spans.
/// If the least significant bit of the flags is clear the rule applies to every code point of the
/// range, and if it is set to every second one. The other seven bits index `OFFSET`.
const RULES: [(u16, u8, u8); 163] = [
    (65, 14, 26),
    (181, 64, 1),
    (192, 14, 23),
    (216, 14, 7),
    (256, 1, 48),
    (306, 1, 6),
    (313, 1, 16),
    (330, 1, 46),
    (376, 116, 1),
    (377, 1, 6),
    (383, 104, 1),
    (385, 50, 1),
    (386, 1, 4),
    (390, 44, 1),
    (391, 0, 1),
    (393, 42, 2),
    (395, 0, 1),
    (398, 32, 1),
    (399, 38, 1),
    (400, 40, 1),
    (401, 0, 1),
    (403, 42, 1),
    (404, 46, 1),
    (406, 52, 1),
    (407, 48, 1),
    (408, 0, 1),
    (412, 52, 1),
    (413, 54, 1),
    (415, 56, 1),
    (416, 1, 6),
    (422, 60, 1),
    (423, 0, 1),
    (425, 60, 1),
    (428, 0, 1),
    (430, 60, 1),
    (431, 0, 1),
    (433, 58, 2),
    (435, 1, 4),
    (439, 62, 1),
    (440, 0, 1),
    (444, 0, 1),
    (452, 2, 1),
    (453, 0, 1),
    (455, 2, 1),
    (456, 0, 1),
    (458, 2, 1),
    (459, 1, 18),
    (478, 1, 18),
    (497, 2, 1),
    (498, 1, 4),
    (502, 122, 1),
    (503, 134, 1),
    (504, 1, 40),
    (544, 110, 1),
    (546, 1, 18),
    (570, 70, 1),
    (571, 0, 1),
    (573, 108, 1),
    (574, 68, 1),
    (577, 0, 1),
    (579, 106, 1),
    (580, 28, 1),
    (581, 30, 1),
    (582, 1, 10),
    (837, 36, 1),
    (880, 1, 4),
    (886, 0, 1),
    (902, 18, 1),
    (904, 16, 3),
    (908, 26, 1),
    (910, 24, 2),
    (913, 14, 17),
    (931, 14, 9),
    (962, 0, 1),
    (975, 4, 1),
    (976, 140, 1),
    (977, 142, 1),
    (981, 146, 1),
    (982, 144, 1),
    (984, 1, 24),
    (1008, 136, 1),
    (1009, 138, 1),
    (1012, 130, 1),
    (1013, 128, 1),
    (1015, 0, 1),
    (1017, 152, 1),
    (1018, 0, 1),
    (1021, 110, 3),
    (1024, 34, 16),
    (1040, 14, 32),
    (1120, 1, 34),
    (1162, 1, 54),
    (1216, 6, 1),
    (1217, 1, 14),
    (1232, 1, 88),
    (1329, 22, 38),
    (4256, 66, 38),
    (4295, 66, 1),
    (4301, 66, 1),
    (7680, 1, 150),
    (7835, 132, 1),
    (7838, 96, 1),
    (7840, 1, 96),
    (7944, 150, 8),
    (7960, 150, 6),
    (7976, 150, 8),
    (7992, 150, 8),
    (8008, 150, 6),
    (8025, 151, 8),
    (8040, 150, 8),
    (8072, 150, 8),
    (8088, 150, 8),
    (8104, 150, 8),
    (8120, 150, 2),
    (8122, 126, 2),
    (8124, 148, 1),
    (8126, 100, 1),
    (8136, 124, 4),
    (8140, 148, 1),
    (8152, 150, 2),
    (8154, 120, 2),
    (8168, 150, 2),
    (8170, 118, 2),
    (8172, 152, 1),
    (8184, 112, 2),
    (8186, 114, 2),
    (8188, 148, 1),
    (8486, 98, 1),
    (8490, 92, 1),
    (8491, 94, 1),
    (8498, 12, 1),
    (8544, 8, 16),
    (8579, 0, 1),
    (9398, 10, 26),
    (11264, 22, 47),
    (11360, 0, 1),
    (11362, 88, 1),
    (11363, 102, 1),
    (11364, 90, 1),
    (11367, 1, 6),
    (11373, 84, 1),
    (11374, 86, 1),
    (11375, 80, 1),
    (11376, 82, 1),
    (11378, 0, 1),
    (11381, 0, 1),
    (11390, 78, 2),
    (11392, 1, 100),
    (11499, 1, 4),
    (11506, 0, 1),
    (42560, 1, 46),
    (42624, 1, 24),
    (42786, 1, 14),
    (42802, 1, 62),
    (42873, 1, 4),
    (42877, 76, 1),
    (42878, 1, 10),
    (42891, 0, 1),
    (42893, 74, 1),
    (42896, 1, 4),
    (42912, 1, 10),
    (42922, 72, 1),
    (65313, 14, 26),
];

/// The amounts added to a code point to fold it to lower case.
const OFFSET: [u16; 77] = [
    1, 2, 8, 15, 16, 26, 28, 32, 37, 38, 40, 48, 63, 64, 69, 71, 79, 80, 116, 202, 203, 205, 206,
    207, 209, 210, 211, 213, 214, 217, 218, 219, 775, 7264, 10792, 10795, 23228, 23256, 30204,
    54721, 54753, 54754, 54756, 54787, 54793, 54809, 57153, 57274, 57921, 58019, 58363, 61722,
    65268, 65341, 65373, 65406, 65408, 65410, 65415, 65424, 65436, 65439, 65450, 65462, 65472,
    65476, 65478, 65480, 65482, 65488, 65506, 65511, 65514, 65521, 65527, 65528, 65529,
];

/// Removes the diacritic from a lower case letter that carries one, as SQLite does.
///
/// Returns the plain letter, or the code point itself when it has no plain form. The ranges that
/// only `remove_diacritics=2` removes are left alone when `complex` is false.
///
/// @param c - the code point, already folded to lower case
/// @param complex - whether `remove_diacritics=2` was asked for
fn remove_diacritic(c: u32, complex: bool) -> u32 {
    let key = (c << 3) | 0x07;
    let mut result = 0usize;
    let mut low = 0usize;
    let mut high = DIACRITIC_RANGE.len() - 1;
    loop {
        let test = (high + low) / 2;
        if key >= u32::from(DIACRITIC_RANGE.get(test).copied().unwrap_or(u16::MAX)) {
            result = test;
            low = test + 1;
        } else if test == 0 {
            break;
        } else {
            high = test - 1;
        }
        if low > high {
            break;
        }
    }
    let range = u32::from(DIACRITIC_RANGE.get(result).copied().unwrap_or(0));
    let target = u32::from(DIACRITIC_TARGET.get(result).copied().unwrap_or(0));
    if !complex && target & 0x80 != 0 {
        return c;
    }
    if c > (range >> 3) + (range & 0x07) {
        c
    } else {
        target & 0x7f
    }
}

/// Reports whether a code point is one of the combining marks `unicode61` keeps inside a token.
///
/// A mark of this kind does not start a token, but when it follows a letter it belongs to the
/// token, and is dropped by the fold when `remove_diacritics` is on. Code points 768 to 817 only.
///
/// @param c - the code point
pub(super) fn is_diacritic(c: u32) -> bool {
    const MASK_LOW: u32 = 0x0802_9FDF;
    const MASK_HIGH: u32 = 0x0003_61F8;
    if !(768..=817).contains(&c) {
        return false;
    }
    if c < 768 + 32 {
        MASK_LOW & (1 << (c - 768)) != 0
    } else {
        MASK_HIGH & (1 << (c - 768 - 32)) != 0
    }
}

/// Folds a code point to lower case, and removes its diacritic when asked to.
///
/// Returns 0 for a combining mark that the fold removes, which the tokenizer leaves out of the
/// token. The result matches `sqlite3Fts5UnicodeFold` for every code point.
///
/// @param c - the code point
/// @param remove_diacritics - 0 keeps diacritics, 1 removes them, 2 removes the extra ranges too
pub(super) fn fold(c: u32, remove_diacritics: u8) -> u32 {
    if c < 128 {
        return if (b'A' as u32..=b'Z' as u32).contains(&c) {
            c + 32
        } else {
            c
        };
    }
    if c >= 65536 {
        return if (66560..66600).contains(&c) {
            c + 40
        } else {
            c
        };
    }
    let mut result = c;
    let mut found: Option<(u16, u8, u8)> = None;
    let (mut low, mut high) = (0usize, RULES.len() - 1);
    loop {
        let test = (high + low) / 2;
        let Some(rule) = RULES.get(test).copied() else {
            break;
        };
        if c >= u32::from(rule.0) {
            found = Some(rule);
            low = test + 1;
        } else if test == 0 {
            break;
        } else {
            high = test - 1;
        }
        if low > high {
            break;
        }
    }
    if let Some((first, flags, count)) = found {
        let covered = c < u32::from(first) + u32::from(count);
        if covered && (u32::from(flags) & 0x01 & (u32::from(first) ^ c)) == 0 {
            let offset = OFFSET.get(usize::from(flags >> 1)).copied().unwrap_or(0);
            result = (c + u32::from(offset)) & 0xFFFF;
        }
    }
    if remove_diacritics != 0 {
        result = remove_diacritic(result, remove_diacritics == 2);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two code points that started this: `İ` folds to `i` and `ı` stays `ı`.
    #[test]
    fn the_turkish_i_folds_as_sqlite_folds_it() {
        assert_eq!(fold(0x130, 1), u32::from('i'));
        assert_eq!(fold(0x131, 1), 0x131);
        assert_eq!(fold(0x131, 2), 0x131);
        assert_eq!(fold(0x130, 0), 0x130);
    }

    /// Case folding and the diacritic removal keep the results of the Latin ranges.
    #[test]
    fn latin_letters_fold_and_lose_their_accents() {
        assert_eq!(fold(u32::from('A'), 1), u32::from('a'));
        assert_eq!(fold(0xC9, 1), u32::from('e'));
        assert_eq!(fold(0xC9, 0), 0xE9);
    }

    /// A combining mark is dropped when diacritics are removed and kept when they are not.
    #[test]
    fn a_combining_mark_is_a_token_character_that_folds_away() {
        assert!(is_diacritic(0x301));
        assert!(!is_diacritic(0x300 - 1));
    }
}
