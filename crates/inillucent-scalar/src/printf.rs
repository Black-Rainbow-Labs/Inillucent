//! `printf()` and its alias `format()`.
//!
//! Invariant: the conversions consume arguments in the order they appear, and a
//! conversion with no argument left produces what SQLite produces for a missing
//! argument rather than stopping. That matters because the format string is
//! data: it can come from a column, and a statement that errors on one row's
//! format and not another's is worse than one that renders both.
//!
//! The set implemented is the one SQLite documents for its own build: `d i u`
//! for integers, `f e E g G` for reals, `s` for text, `c` for a character, `x X
//! o` for the other bases, `q Q w` for the SQL-quoting conversions, `z` as a
//! synonym for `s`, `%` for a literal, and the flags `- + space 0 #` with a
//! width, a precision, and `*` to take either from an argument.

use inillucent_value::{cast, fpdecode, numeric, TextEncoding, Value};

/// Formats a call to `printf`/`format`.
///
/// The first argument is the format; the rest are consumed left to right.
///
/// **An unknown conversion makes the whole answer NULL, as it does in SQLite**:
/// `printf('%y', 1)`, `printf('%5')`, `printf('%1$d', 1)` and `printf('%hd', 1)`
/// are all NULL. A lone `%` at the very end of the format is a literal one.
pub fn format(arguments: &[Value<'static>], encoding: TextEncoding) -> Value<'static> {
    let Some(first) = arguments
        .first()
        .filter(|value| !matches!(value, Value::Null))
    else {
        // A NULL format is NULL, and a call with no arguments at all is NULL too.
        return Value::Null;
    };
    let template = text_of(Some(first), encoding);
    let mut out: Vec<u8> = Vec::new();
    let mut next = 1usize;
    let mut index = 0usize;
    while let Some(byte) = template.get(index).copied() {
        index = index.saturating_add(1);
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        if index >= template.len() {
            out.push(b'%');
            break;
        }
        let Some((mut spec, after)) = parse_spec(&template, index) else {
            return Value::Null;
        };
        index = after;
        take_widths(&mut spec, arguments, &mut next);
        // `%n` writes nothing and `%%` takes no argument.
        let argument = match spec.conversion {
            b'n' => continue,
            b'%' => None,
            _ => {
                next = next.saturating_add(1);
                arguments.get(next.saturating_sub(1))
            }
        };
        // A character conversion measures its width in characters.
        spec.characters |= spec.conversion == b'c';
        let rendered = render(&spec, argument, encoding);
        pad(&mut out, &rendered, &spec);
    }
    Value::owned_text(&out).unwrap_or(Value::Null)
}

/// Reads a `*` width and a `*` precision from the arguments.
///
/// SQLite reads each as a C `int`, so a value past 32 bits wraps: a width of
/// 2147483648 is read as -2147483648, which asks for left justification and a
/// width of zero. A negative width left justifies, and a negative precision is
/// its absolute value (-1, meaning none, when it is -2147483648).
///
/// @param spec - the specification, with its `*` fields still to fill
/// @param arguments - the call's arguments
/// @param next - the next argument to read, advanced past the ones used
fn take_widths(spec: &mut Spec, arguments: &[Value<'static>], next: &mut usize) {
    if spec.width_from_argument {
        let width = integer_of(arguments.get(*next)) as i32;
        *next = next.saturating_add(1);
        if width < 0 {
            spec.left = true;
        }
        spec.width = if width >= -2_147_483_647 {
            width.unsigned_abs() as usize
        } else {
            0
        };
    }
    if spec.precision_from_argument {
        let precision = integer_of(arguments.get(*next)) as i32;
        *next = next.saturating_add(1);
        spec.precision = match precision {
            0.. => Some(precision as usize),
            -2_147_483_647..=-1 => Some(precision.unsigned_abs() as usize),
            _ => None,
        };
    }
}

/// Returns the longest field a call asks for, before anything is written.
///
/// The width and the precision are in the format string or in the arguments, so
/// the size of the answer is known up front, and a refusal made here costs
/// nothing where one made after the answer exists has already paid for it.
///
/// @param arguments - the call's arguments, the format first
pub fn widest_request(arguments: &[Value<'static>]) -> Option<u64> {
    let first = arguments
        .first()
        .filter(|value| !matches!(value, Value::Null))?;
    let template = text_of(Some(first), TextEncoding::Utf8);
    let (mut widest, mut next, mut index) = (0u64, 1usize, 0usize);
    while let Some(byte) = template.get(index).copied() {
        index = index.saturating_add(1);
        if byte != b'%' {
            continue;
        }
        let Some((mut spec, after)) = parse_spec(&template, index) else {
            return Some(widest).filter(|value| *value > 0);
        };
        index = after;
        take_widths(&mut spec, arguments, &mut next);
        if !matches!(spec.conversion, b'%' | b'n') {
            next = next.saturating_add(1);
        }
        let precision = match spec.conversion {
            b's' | b'z' | b'q' | b'Q' | b'w' | b'%' | b'n' => 0,
            _ => spec.precision.unwrap_or(0),
        };
        widest = widest.max(spec.width as u64).max(precision as u64);
    }
    (widest > 0).then_some(widest)
}

/// One conversion specification.
#[derive(Clone, Copy, Debug)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    alternate: bool,
    /// The `,` flag: group the digits in threes.
    ///
    /// **Measured, not assumed.** It applies to `d`, `i`, `u`, `f`, and to
    /// `g` when `g` chooses the fixed form: `printf('%,.10g', 1234567.0)` is
    /// `1,234,567` in the pinned 3.53.4, while `%,x`, `%,o` and `%,e` are
    /// ungrouped (task-2080). For `%d` it is applied after the zero padding
    /// rather than before, so `printf('%0,12d', 1234567)` is
    /// `000,001,234,567`, which is fifteen characters in a field of twelve.
    group: bool,
    /// The `!` flag: count the width and the precision in characters.
    ///
    /// That is its meaning for the text conversions. `printf('%5s', '日本語')`
    /// answers the three characters unpadded, because they are nine bytes and
    /// nine is past five; `printf('%!5s', ...)` pads them to five characters.
    /// For the real conversions it is SQLite's `flag_altform2`: up to twenty
    /// digits instead of sixteen, and trailing zeros removed.
    characters: bool,
    width: usize,
    width_from_argument: bool,
    precision: Option<usize>,
    precision_from_argument: bool,
    conversion: u8,
}

/// Parses a conversion specification, returning it and where it ended.
fn parse_spec(template: &[u8], start: usize) -> Option<(Spec, usize)> {
    let mut spec = Spec {
        left: false,
        plus: false,
        space: false,
        zero: false,
        alternate: false,
        group: false,
        characters: false,
        width: 0,
        width_from_argument: false,
        precision: None,
        precision_from_argument: false,
        conversion: 0,
    };
    let mut index = start;
    loop {
        match template.get(index).copied() {
            Some(b'-') => spec.left = true,
            Some(b'+') => spec.plus = true,
            Some(b' ') => spec.space = true,
            Some(b'0') => spec.zero = true,
            Some(b'#') => spec.alternate = true,
            Some(b',') => spec.group = true,
            Some(b'!') => spec.characters = true,
            _ => break,
        }
        index = index.saturating_add(1);
    }
    if template.get(index) == Some(&b'*') {
        spec.width_from_argument = true;
        index = index.saturating_add(1);
    } else {
        while template.get(index).is_some_and(u8::is_ascii_digit) {
            let digit = template.get(index).copied().unwrap_or(b'0');
            spec.width = spec
                .width
                .saturating_mul(10)
                .saturating_add(usize::from(digit.saturating_sub(b'0')));
            index = index.saturating_add(1);
        }
    }
    if template.get(index) == Some(&b'.') {
        index = index.saturating_add(1);
        if template.get(index) == Some(&b'*') {
            spec.precision_from_argument = true;
            index = index.saturating_add(1);
        } else {
            // `%.c` is a precision of zero, not no precision.
            let mut precision = 0u32;
            while template.get(index).is_some_and(u8::is_ascii_digit) {
                let digit = template.get(index).copied().unwrap_or(b'0');
                precision = precision
                    .wrapping_mul(10)
                    .wrapping_add(u32::from(digit.saturating_sub(b'0')));
                index = index.saturating_add(1);
            }
            spec.precision = Some((precision & 0x7fff_ffff) as usize);
        }
    }
    // `l` and `ll` are accepted and ignored: every integer here is 64-bit. Any
    // other modifier, `h` included, is not a conversion at all.
    for _ in 0..2 {
        if template.get(index) == Some(&b'l') {
            index = index.saturating_add(1);
        }
    }
    let conversion = template.get(index).copied()?;
    if !b"diuxXopcsrzqQwfeEgGn%".contains(&conversion) {
        return None;
    }
    spec.conversion = conversion;
    Some((spec, index.saturating_add(1)))
}

/// Returns an argument as an integer, or zero when there is none.
fn integer_of(value: Option<&Value<'static>>) -> i64 {
    value.map_or(0, cast::integer_value)
}

/// Renders one conversion, before padding.
fn render(spec: &Spec, argument: Option<&Value<'static>>, encoding: TextEncoding) -> Vec<u8> {
    match spec.conversion {
        b'd' | b'i' | b'u' => integer(spec, integer_of(argument)),
        b'x' | b'p' => based(spec, integer_of(argument), 16, false),
        b'X' => based(spec, integer_of(argument), 16, true),
        b'o' => based(spec, integer_of(argument), 8, false),
        b'r' => ordinal(spec, integer_of(argument)),
        b'f' | b'e' | b'E' | b'g' | b'G' => real(spec, argument),
        b'c' => repeated_character(spec, argument, encoding),
        b'%' => b"%".to_vec(),
        b's' | b'z' => limited(text_of(argument, encoding), spec),
        b'q' => quoted(spec, argument, encoding, false),
        b'Q' => quoted(spec, argument, encoding, true),
        b'w' => {
            // `%w` quotes an identifier: a double quote is doubled and the
            // whole is *not* wrapped, which is what makes it usable inside a
            // `CREATE` statement being assembled. A NULL is `(NULL)`, as it is
            // for `%q`.
            if matches!(argument, None | Some(Value::Null)) {
                return b"(NULL)".to_vec();
            }
            let text = limited(text_of(argument, encoding), spec);
            let mut out = Vec::new();
            for byte in text {
                if byte == b'"' {
                    out.push(b'"');
                }
                out.push(byte);
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Renders `%c`: the first character of the argument as text, repeated
/// `precision` times.
///
/// **It is the first character of the argument as text, not a character code:**
/// the pinned build renders `printf('%c', 65)` as "6", because 65 becomes the
/// text "65". A NULL or empty argument is one NUL byte, which is what SQLite
/// writes. The precision is a repeat count, so `printf('%.3c', 'x')` is `xxx`,
/// and a precision of zero or one is one character. The field width counts the
/// repeats as characters, whatever the flags say.
///
/// @param spec - the conversion as it was written
/// @param argument - the value to take the character from
/// @param encoding - the database encoding
fn repeated_character(
    spec: &Spec,
    argument: Option<&Value<'static>>,
    encoding: TextEncoding,
) -> Vec<u8> {
    let text = text_of(argument, encoding);
    let lead = text.first().copied().unwrap_or(0);
    let mut length = 1usize;
    if lead & 0xc0 == 0xc0 {
        while length < 4 && text.get(length).is_some_and(|byte| byte & 0xc0 == 0x80) {
            length = length.saturating_add(1);
        }
    }
    let character = match text.get(..length) {
        Some(bytes) if !text.is_empty() => bytes.to_vec(),
        _ => vec![0u8],
    };
    let times = spec.precision.unwrap_or(1).max(1);
    let mut out = Vec::with_capacity(character.len().saturating_mul(times));
    for _ in 0..times {
        out.extend_from_slice(&character);
    }
    out
}

/// Cuts a string to the precision, in bytes or, under `!`, in characters.
///
/// **A byte cut may split a character**, and SQLite lets it: the answer for
/// `printf('%.1s', 'éa')` is the single byte `C3`. Only `!` counts whole
/// characters (task-1932, M8): `printf('%!.3s', 'éab')` is `éab`.
///
/// @param text - the rendered argument
/// @param spec - the conversion as it was written
fn limited(mut text: Vec<u8>, spec: &Spec) -> Vec<u8> {
    if let Some(precision) = spec.precision {
        let cut = match spec.characters {
            true => char_boundary_after(&text, precision),
            false => precision.min(text.len()),
        };
        text.truncate(cut);
    }
    text
}

/// Renders `%r`: a number followed by its English ordinal suffix.
///
/// @param spec - the conversion as it was written
/// @param value - the number
fn ordinal(spec: &Spec, value: i64) -> Vec<u8> {
    let mut out = integer(spec, value);
    let tens = value % 100;
    let suffix: &[u8] = if (10..=19).contains(&tens) {
        b"th"
    } else {
        match value % 10 {
            1 => b"st",
            2 => b"nd",
            3 => b"rd",
            _ => b"th",
        }
    };
    out.extend_from_slice(suffix);
    out
}

/// Renders an integer conversion.
fn integer(spec: &Spec, value: i64) -> Vec<u8> {
    let negative = value < 0;
    let magnitude = value.unsigned_abs().to_string();
    let mut digits = magnitude.into_bytes();
    if let Some(precision) = spec.precision {
        while digits.len() < precision {
            digits.insert(0, b'0');
        }
    }
    let sign = if negative {
        Some(b'-')
    } else if spec.plus {
        Some(b'+')
    } else if spec.space {
        Some(b' ')
    } else {
        None
    };
    // **The zero padding happens here rather than in `pad` when the digits are
    // grouped, and the order is what the reference does (task-1932, M8).**
    // `printf('%0,12d', 1234567)` is `000,001,234,567`: the digits are filled
    // to the field width first and the separators are inserted afterwards, so
    // the answer is fifteen characters wide. Doing it the other way round -
    // group, then pad to twelve - gives `0001,234,567`, which is what this
    // answered before and is not what SQLite answers.
    if spec.group && spec.zero && !spec.left {
        let room = spec.width.saturating_sub(usize::from(sign.is_some()));
        while digits.len() < room {
            digits.insert(0, b'0');
        }
    }
    if spec.group {
        digits = grouped(&digits);
    }
    let mut out = Vec::new();
    if let Some(sign) = sign {
        out.push(sign);
    }
    out.extend_from_slice(&digits);
    out
}

/// Returns a run of digits with a comma between every group of three.
///
/// Counted from the right, so a leading zero is grouped like any other digit:
/// the reference answers `00,001,234` for `printf('%,.8d', 1234)`.
///
/// @param digits - the digits, without a sign
fn grouped(digits: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(digits.len().saturating_add(digits.len() / 3));
    for (at, digit) in digits.iter().enumerate() {
        let left = digits.len().saturating_sub(at);
        if at > 0 && left % 3 == 0 {
            out.push(b',');
        }
        out.push(*digit);
    }
    out
}

/// Renders a hexadecimal or octal conversion.
fn based(spec: &Spec, value: i64, base: u32, upper: bool) -> Vec<u8> {
    let unsigned = value as u64;
    let mut text = match base {
        16 if upper => format!("{unsigned:X}"),
        16 => format!("{unsigned:x}"),
        8 => format!("{unsigned:o}"),
        _ => unsigned.to_string(),
    };
    if let Some(precision) = spec.precision {
        while text.len() < precision {
            text.insert(0, '0');
        }
    }
    if spec.alternate && unsigned != 0 {
        match base {
            16 if upper => text.insert_str(0, "0X"),
            16 => text.insert_str(0, "0x"),
            8 => text.insert(0, '0'),
            _ => {}
        }
    }
    text.into_bytes()
}

/// Returns what `%g` would render for a number.
///
/// The one conversion another module needs on its own: `geopoly` writes its
/// coordinates with it, and re-deriving C's shorter-of-the-two rule beside it
/// would be a second implementation that could drift from this one.
///
/// @param value - the number
pub fn general(value: f64) -> String {
    let spec = Spec {
        left: false,
        plus: false,
        space: false,
        zero: false,
        alternate: false,
        group: false,
        characters: false,
        width: 0,
        width_from_argument: false,
        precision: None,
        precision_from_argument: false,
        conversion: b'g',
    };
    String::from_utf8_lossy(&real(&spec, Some(&Value::Real(value)))).into_owned()
}

/// Renders a floating point conversion.
///
/// **Through SQLite's own decoder, not through Rust's formatter** (task-2080).
/// This used to render with `{:e}` and `{:.N}`, which are correctly rounded,
/// and then cut the digits to sixteen, or to twenty under `!`. Two differences
/// were left that no amount of cutting could close. SQLite's `sqlite3FpDecode`
/// scales the double by an approximation of a power of ten, so its last digit
/// is sometimes not the last digit of the exact expansion:
/// `printf('%.20g', 3.1643187021860255e-168)` is `3.164318702186026e-168`
/// there and was `...025e-168` here. And under `!` SQLite stops at however
/// many digits the decoder produced, eighteen for pi and nineteen for `0.1`,
/// where this engine filled out to the precision with zeros:
/// `printf('%!.25f', 0.1)` is `0.1000000000000000056` and was
/// `0.1000000000000000055500000`. [`fpdecode::render`] is a transcription of
/// the decoder and of the code that writes its digits out, so both are the
/// reference's digits by construction.
///
/// @param spec - the conversion as it was written
/// @param argument - the value, read as a real
fn real(spec: &Spec, argument: Option<&Value<'static>>) -> Vec<u8> {
    let value = argument.map_or(0.0, cast::real_value);
    let conversion = match spec.conversion {
        b'e' | b'E' => fpdecode::Conversion::Exponential,
        b'g' | b'G' => fpdecode::Conversion::General,
        _ => fpdecode::Conversion::Fixed,
    };
    let prefix = if spec.plus {
        Some(b'+')
    } else if spec.space {
        Some(b' ')
    } else {
        None
    };
    let format = fpdecode::Format {
        conversion,
        precision: spec.precision,
        prefix,
        alternate: spec.alternate,
        alternate2: spec.characters,
        zero_pad: spec.zero,
        thousands: spec.group,
        upper: matches!(spec.conversion, b'E' | b'G'),
    };
    fpdecode::render(value, &format)
}

/// Returns an argument rendered as text.
pub fn rendered_text(argument: Option<&Value<'static>>, encoding: TextEncoding) -> Vec<u8> {
    text_of(argument, encoding)
}

/// Renders one value the way `%s` would.
fn text_of(argument: Option<&Value<'static>>, encoding: TextEncoding) -> Vec<u8> {
    let _ = encoding;
    match argument {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Integer(integer)) => numeric::integer_to_text(*integer),
        Some(Value::Real(real)) => numeric::real_to_text(*real),
        // `%s` reads its argument as a C string, so text and a blob end at
        // the first NUL: `printf('%s|%s', 'a' || char(0) || 'b', 'c')` is `a|c`.
        Some(Value::Text(text)) => until_nul(&text.utf8_bytes()),
        Some(Value::Blob(blob)) => until_nul(blob.raw()),
    }
}

/// Returns the bytes before the first NUL, as an owned vector.
///
/// @param bytes - the argument's bytes
fn until_nul(bytes: &[u8]) -> Vec<u8> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    bytes.get(..end).unwrap_or(bytes).to_vec()
}

/// Renders `%q` or `%Q`: a string with its single quotes doubled.
///
/// `%Q` also wraps it in quotes and writes a bare `NULL` for a NULL, which is
/// what makes it safe to paste into generated SQL where `%q` is not.
fn quoted(
    spec: &Spec,
    argument: Option<&Value<'static>>,
    encoding: TextEncoding,
    wrap: bool,
) -> Vec<u8> {
    // A NULL is `NULL` unquoted for `%Q` and the literal text `(NULL)` for
    // `%q`, which is the marker SQLite writes wherever a string was expected
    // and none was given.
    if matches!(argument, None | Some(Value::Null)) {
        return if wrap {
            b"NULL".to_vec()
        } else {
            b"(NULL)".to_vec()
        };
    }
    let text = limited(text_of(argument, encoding), spec);
    let mut out = Vec::new();
    if wrap {
        out.push(b'\'');
    }
    for byte in text {
        if byte == b'\'' {
            out.push(b'\'');
        }
        out.push(byte);
    }
    if wrap {
        out.push(b'\'');
    }
    out
}

/// Returns the byte index of the end of the `count`th character.
///
/// @param text - the rendered text
/// @param count - how many characters to keep
fn char_boundary_after(text: &[u8], count: usize) -> usize {
    let mut seen = 0usize;
    for (at, byte) in text.iter().enumerate() {
        // A continuation byte is `10xxxxxx` and starts no character.
        if byte & 0xC0 != 0x80 {
            if seen == count {
                return at;
            }
            seen = seen.saturating_add(1);
        }
    }
    text.len()
}

/// Returns how many characters a rendered conversion occupies.
///
/// @param body - the rendered conversion
fn character_count(body: &[u8]) -> usize {
    body.iter().filter(|byte| *byte & 0xC0 != 0x80).count()
}

/// Pads a rendered conversion to the width the specification asks for.
fn pad(out: &mut Vec<u8>, body: &[u8], spec: &Spec) {
    // **`!` makes the field width a count of characters (task-1932, M8).**
    // `printf('%5s', '日本語')` is the three characters unpadded, because they
    // are nine bytes; `printf('%!5s', ...)` pads them to five characters.
    let measured = match spec.characters {
        true => character_count(body),
        false => body.len(),
    };
    if measured >= spec.width {
        out.extend_from_slice(body);
        return;
    }
    let fill = spec.width.saturating_sub(measured);
    if spec.left {
        out.extend_from_slice(body);
        out.extend(core::iter::repeat_n(b' ', fill));
        return;
    }
    // Zero padding goes *after* a sign, not before it, so `%05d` of -42 is
    // `-0042` rather than `000-42`.
    //
    // **A precision does not switch the zero off.** C says the `0` flag is
    // ignored for `d`, `i`, `o`, `u`, `x` and `X` when a precision is given,
    // and SQLite's own printf does not implement that rule: the pinned 3.53.4
    // renders `%08.3d` of 42 as `00000042` and `%08.3x` of 255 as `000000ff`.
    // Applying the C rule here made `printf('%05.2f', 3.14159)` answer `3.14`
    // against the reference's `03.14` - and the guard was wrong for the integer
    // conversions it was written for as well.
    let numeric_conversion = matches!(
        spec.conversion,
        b'd' | b'i' | b'u' | b'x' | b'X' | b'o' | b'f' | b'e' | b'E' | b'g' | b'G'
    );
    if spec.zero && numeric_conversion {
        let signed = matches!(body.first(), Some(b'-') | Some(b'+') | Some(b' '));
        if signed {
            out.extend_from_slice(body.get(..1).unwrap_or(&[]));
            out.extend(core::iter::repeat_n(b'0', fill));
            out.extend_from_slice(body.get(1..).unwrap_or(&[]));
        } else {
            out.extend(core::iter::repeat_n(b'0', fill));
            out.extend_from_slice(body);
        }
        return;
    }
    out.extend(core::iter::repeat_n(b' ', fill));
    out.extend_from_slice(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formats with owned arguments, returning the text.
    fn run(template: &str, arguments: Vec<Value<'static>>) -> String {
        let mut all = vec![Value::owned_text(template.as_bytes()).expect("the text builds")];
        all.extend(arguments);
        match format(&all, TextEncoding::Utf8) {
            Value::Text(text) => String::from_utf8_lossy(&text.utf8_bytes()).into_owned(),
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn zero_padding_follows_the_sign() {
        assert_eq!(run("%05d", vec![Value::Integer(-42)]), "-0042");
        assert_eq!(run("%05d", vec![Value::Integer(42)]), "00042");
        assert_eq!(run("%-5d|", vec![Value::Integer(42)]), "42   |");
    }

    #[test]
    fn a_half_rounds_away_from_zero() {
        assert_eq!(run("%.0f", vec![Value::Real(2.5)]), "3");
        assert_eq!(run("%.0f", vec![Value::Real(-2.5)]), "-3");
        assert_eq!(run("%.1f", vec![Value::Real(0.25)]), "0.3");
        // Not a half at all: the nearest double to 0.35 is below the midpoint.
        assert_eq!(run("%.1f", vec![Value::Real(0.35)]), "0.3");
    }

    #[test]
    fn a_missing_argument_renders_rather_than_failing() {
        assert_eq!(run("%d-%d", vec![Value::Integer(1)]), "1-0");
        assert_eq!(run("%s!", Vec::new()), "!");
    }

    #[test]
    fn the_quoting_conversions_differ_on_null() {
        assert_eq!(run("%q", vec![Value::Null]), "(NULL)");
        assert_eq!(run("%Q", vec![Value::Null]), "NULL");
        assert_eq!(
            run("%q", vec![Value::owned_text(b"it's").expect("text")]),
            "it''s"
        );
    }
}
