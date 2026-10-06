//! A whole result, or a whole list of parameters, as one JSON text.
//!
//! **This exists for a binding whose every foreign call is expensive.** Python's
//! `ctypes` costs about half a microsecond a call. The cell accessors of the C
//! ABI take two calls a cell, so reading 20,000 rows of five columns through
//! them took 170 ms, and the engine's own share of that was 12 ms (task-2191).
//! One call that returns the result as JSON, parsed by the host language's own
//! JSON reader, which is written in C in every language that has one, is the
//! cheapest crossing there is. Binding works the same way in the other
//! direction: one call carries every parameter of an execution, or of a
//! thousand executions.
//!
//! Invariant: **a value read through this module is the value the cell
//! accessors return.** An integer is a JSON integer and a real always has a
//! fraction or an exponent, so a reader that types its result from the text
//! cannot take one for the other. A blob is `{"blob": "<hex>"}`, which is what
//! the command line writes and reads. A real JSON cannot spell (an infinity, or
//! NaN) is `{"real": "Infinity"}`, `{"real": "-Infinity"}` or `{"real": "NaN"}`,
//! so the text stays strict JSON that every parser reads.

use crate::{Error, Result, Rows, Status, Value};

/// Writes a result as one JSON object.
///
/// The object holds `columns`, `types`, `rows` (an array of arrays of
/// values), `total`, `more`, `affected` (`null` for a query), `elapsed_us` and
/// `tag`: everything the C ABI's accessors return, in one text.
///
/// @param rows - the result
pub fn rows_to_json(rows: &Rows) -> String {
    let mut out = String::with_capacity(64 + rows.rows.len() * rows.columns.len() * 12);
    out.push_str("{\"columns\":[");
    for (nth, column) in rows.columns.iter().enumerate() {
        if nth > 0 {
            out.push(',');
        }
        push_text(&mut out, &column.name);
    }
    out.push_str("],\"types\":[");
    for (nth, column) in rows.columns.iter().enumerate() {
        if nth > 0 {
            out.push(',');
        }
        push_text(&mut out, &column.declared_type);
    }
    out.push_str("],\"rows\":[");
    for (nth, row) in rows.rows.iter().enumerate() {
        if nth > 0 {
            out.push(',');
        }
        out.push('[');
        for (at, value) in row.iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            push_value(&mut out, value);
        }
        out.push(']');
    }
    out.push_str("],\"total\":");
    out.push_str(&rows.total.to_string());
    out.push_str(",\"more\":");
    out.push_str(if rows.more { "true" } else { "false" });
    out.push_str(",\"affected\":");
    match rows.affected {
        Some(count) => out.push_str(&count.to_string()),
        None => out.push_str("null"),
    }
    out.push_str(",\"elapsed_us\":");
    out.push_str(&rows.elapsed.as_micros().to_string());
    out.push_str(",\"tag\":");
    push_text(&mut out, &rows.tag);
    out.push('}');
    out
}

/// Appends one value.
///
/// @param out - the text being built
/// @param value - the cell
fn push_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Integer(number) => out.push_str(&number.to_string()),
        Value::Real(number) => push_real(out, *number),
        Value::Text(text) => push_text(out, text),
        Value::Blob(bytes) => {
            out.push_str("{\"blob\":\"");
            push_hex(out, bytes);
            out.push_str("\"}");
        }
    }
}

/// Appends a real so that no reader can take it for an integer.
///
/// `{:?}` prints the shortest text that reads back as the same double, and it
/// always writes a fraction or an exponent: `1.0`, `1e300`, `-0.0`. `{}` would
/// write 1e300 as 301 digits, which JSON reads as an integer.
///
/// @param out - the text being built
/// @param number - the value
fn push_real(out: &mut String, number: f64) {
    if number.is_finite() {
        out.push_str(&format!("{number:?}"));
    } else if number.is_nan() {
        out.push_str("{\"real\":\"NaN\"}");
    } else if number > 0.0 {
        out.push_str("{\"real\":\"Infinity\"}");
    } else {
        out.push_str("{\"real\":\"-Infinity\"}");
    }
}

/// Appends a JSON string, escaped by the engine's one escaper.
///
/// @param out - the text being built
/// @param text - the string
fn push_text(out: &mut String, text: &str) {
    out.push('"');
    inillucent_engine::base::json::escape_into(out, text);
    out.push('"');
}

/// Appends bytes as lower case hexadecimal.
///
/// @param out - the text being built
/// @param bytes - the bytes
fn push_hex(out: &mut String, bytes: &[u8]) {
    out.reserve(bytes.len() * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0xf] {
            out.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
    }
}

/// Reads one execution's parameters: a JSON array of values.
///
/// A value is `null`, `true` or `false` (bound as 1 and 0, which is what
/// SQLite stores for a boolean), an integer, a real, a string,
/// `{"blob": "<hex>"}` or `{"real": "Infinity"}`. The bare words `Infinity`,
/// `-Infinity` and `NaN` are read as reals too, because Python's `json.dumps`
/// writes them that way. An integer that does not fit in 64 bits is refused
/// rather than rounded to a real.
///
/// @param text - the JSON text
pub fn params_from_json(text: &str) -> Result<Vec<Value>> {
    let mut reader = Reader::new(text);
    let values = reader.array_of_values()?;
    reader.finish()?;
    Ok(values)
}

/// Reads many executions' parameters: a JSON array of arrays of values.
///
/// @param text - the JSON text
pub fn param_rows_from_json(text: &str) -> Result<Vec<Vec<Value>>> {
    let mut reader = Reader::new(text);
    reader.open(b'[')?;
    let mut rows = Vec::new();
    if !reader.close_if(b']') {
        loop {
            rows.push(reader.array_of_values()?);
            if reader.close_if(b']') {
                break;
            }
            reader.open(b',')?;
        }
    }
    reader.finish()?;
    Ok(rows)
}

/// A cursor over JSON text.
struct Reader<'a> {
    /// The text.
    bytes: &'a [u8],
    /// The next byte to read.
    at: usize,
}

impl<'a> Reader<'a> {
    /// Starts at the beginning of a text.
    ///
    /// @param text - the JSON text
    fn new(text: &'a str) -> Reader<'a> {
        Reader {
            bytes: text.as_bytes(),
            at: 0,
        }
    }

    /// Returns the refusal for malformed text, naming where it went wrong.
    ///
    /// @param what - what was expected
    fn refuse(&self, what: &str) -> Error {
        Error::said(
            Status::InvalidState,
            format!(
                "the parameter JSON is not valid at byte {}: {what}.",
                self.at
            ),
        )
    }

    /// Skips white space.
    fn skip_space(&mut self) {
        while let Some(byte) = self.bytes.get(self.at) {
            if !matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                break;
            }
            self.at += 1;
        }
    }

    /// Returns the next byte that is not white space, without taking it.
    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.bytes.get(self.at).copied()
    }

    /// Takes one expected byte.
    ///
    /// @param byte - the byte that has to come next
    fn open(&mut self, byte: u8) -> Result<()> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.refuse(&format!("expected '{}'", char::from(byte))))
        }
    }

    /// Takes a byte when it is the next one, and reports whether it was.
    ///
    /// @param byte - the byte to take
    fn close_if(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    /// Refuses anything after the value.
    fn finish(&mut self) -> Result<()> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(self.refuse("there is text after the value")),
        }
    }

    /// Takes a literal word when it is next.
    ///
    /// @param word - the word
    fn word(&mut self, word: &[u8]) -> bool {
        let found = self
            .bytes
            .get(self.at..self.at + word.len())
            .is_some_and(|here| here == word);
        if found {
            self.at += word.len();
        }
        found
    }

    /// Reads an array of values.
    fn array_of_values(&mut self) -> Result<Vec<Value>> {
        self.open(b'[')?;
        let mut values = Vec::new();
        if self.close_if(b']') {
            return Ok(values);
        }
        loop {
            values.push(self.value()?);
            if self.close_if(b']') {
                return Ok(values);
            }
            self.open(b',')?;
        }
    }

    /// Reads one value.
    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            Some(b'"') => Ok(Value::Text(self.string()?)),
            Some(b'{') => self.object(),
            Some(b'n') if self.word(b"null") => Ok(Value::Null),
            Some(b't') if self.word(b"true") => Ok(Value::Integer(1)),
            Some(b'f') if self.word(b"false") => Ok(Value::Integer(0)),
            Some(b'N') if self.word(b"NaN") => Ok(Value::Real(f64::NAN)),
            Some(b'I') if self.word(b"Infinity") => Ok(Value::Real(f64::INFINITY)),
            Some(b'-') if self.word(b"-Infinity") => Ok(Value::Real(f64::NEG_INFINITY)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.refuse("expected a value")),
        }
    }

    /// Reads a number: an integer when it has no fraction and no exponent.
    fn number(&mut self) -> Result<Value> {
        let start = self.at;
        let mut real = false;
        while let Some(byte) = self.bytes.get(self.at) {
            match byte {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => real = true,
                _ => break,
            }
            self.at += 1;
        }
        let digits = std::str::from_utf8(self.bytes.get(start..self.at).unwrap_or_default())
            .map_err(|_| self.refuse("a number is not ASCII"))?;
        if real {
            return digits
                .parse::<f64>()
                .map(Value::Real)
                .map_err(|_| self.refuse("a number does not parse"));
        }
        digits.parse::<i64>().map(Value::Integer).map_err(|_| {
            Error::said(
                Status::TooBig,
                format!("the parameter {digits} does not fit in a 64-bit integer."),
            )
        })
    }

    /// Reads `{"blob": "<hex>"}` or `{"real": "<word>"}`.
    fn object(&mut self) -> Result<Value> {
        self.open(b'{')?;
        let key = self.string()?;
        self.open(b':')?;
        let body = self.string()?;
        self.open(b'}')?;
        match key.as_str() {
            "blob" => unhex(&body)
                .map(Value::Blob)
                .ok_or_else(|| self.refuse("a blob is not hexadecimal")),
            "real" => match body.as_str() {
                "Infinity" => Ok(Value::Real(f64::INFINITY)),
                "-Infinity" => Ok(Value::Real(f64::NEG_INFINITY)),
                "NaN" => Ok(Value::Real(f64::NAN)),
                _ => Err(self.refuse("a real object names no value")),
            },
            _ => Err(self.refuse("an object is a blob or a real")),
        }
    }

    /// Reads a string, with its escapes.
    fn string(&mut self) -> Result<String> {
        self.open(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let start = self.at;
            while let Some(byte) = self.bytes.get(self.at) {
                if *byte == b'"' || *byte == b'\\' {
                    break;
                }
                self.at += 1;
            }
            out.extend_from_slice(self.bytes.get(start..self.at).unwrap_or_default());
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return String::from_utf8(out)
                        .map_err(|_| self.refuse("a string is not UTF-8"));
                }
                Some(b'\\') => {
                    self.at += 1;
                    self.escape(&mut out)?;
                }
                _ => return Err(self.refuse("a string is not closed")),
            }
        }
    }

    /// Reads the escape after a backslash into a string being built.
    ///
    /// @param out - the string's bytes
    fn escape(&mut self, out: &mut Vec<u8>) -> Result<()> {
        let Some(byte) = self.bytes.get(self.at).copied() else {
            return Err(self.refuse("a string ends in a backslash"));
        };
        self.at += 1;
        let plain = match byte {
            b'"' => b'"',
            b'\\' => b'\\',
            b'/' => b'/',
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                let character = self.unicode()?;
                let mut buffer = [0u8; 4];
                out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                return Ok(());
            }
            _ => return Err(self.refuse("an escape is not one JSON has")),
        };
        out.push(plain);
        Ok(())
    }

    /// Reads the four digits of `\u`, and the second half of a surrogate pair.
    fn unicode(&mut self) -> Result<char> {
        let first = self.four_hex()?;
        if !(0xd800..0xdc00).contains(&first) {
            return char::from_u32(first)
                .ok_or_else(|| self.refuse("a \\u escape is a lone surrogate"));
        }
        if !self.word(b"\\u") {
            return Err(self.refuse("a surrogate has no second half"));
        }
        let second = self.four_hex()?;
        if !(0xdc00..0xe000).contains(&second) {
            return Err(self.refuse("a surrogate has no second half"));
        }
        let code = 0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00);
        char::from_u32(code).ok_or_else(|| self.refuse("a surrogate pair is not a character"))
    }

    /// Reads four hexadecimal digits.
    fn four_hex(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or_else(|| self.refuse("a \\u escape is short"))?;
        let mut code = 0u32;
        for digit in digits {
            let nibble = char::from(*digit)
                .to_digit(16)
                .ok_or_else(|| self.refuse("a \\u escape is not hexadecimal"))?;
            code = code * 16 + nibble;
        }
        self.at += 4;
        Ok(code)
    }
}

/// Reads hexadecimal into bytes, or `None` when it is not hexadecimal.
///
/// @param text - the digits, two a byte
fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| {
            let high = char::from(*pair.first()?).to_digit(16)?;
            let low = char::from(*pair.get(1)?).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_value_reads_back_as_itself() {
        let values = vec![
            Value::Null,
            Value::Integer(i64::MIN),
            Value::Integer(i64::MAX),
            Value::Real(1.0),
            Value::Real(-0.0),
            Value::Real(1e300),
            Value::Real(0.1),
            Value::Real(f64::INFINITY),
            Value::Real(f64::NEG_INFINITY),
            Value::Text("quote \" back \\ tab \t nul \u{0} é 𝄞".to_string()),
            Value::Blob(vec![0, 1, 0xfe, 0xff]),
        ];
        let mut text = String::from("[");
        for (nth, value) in values.iter().enumerate() {
            if nth > 0 {
                text.push(',');
            }
            push_value(&mut text, value);
        }
        text.push(']');
        let read = params_from_json(&text).unwrap();
        assert_eq!(read.len(), values.len());
        for (wrote, got) in values.iter().zip(&read) {
            match (wrote, got) {
                (Value::Real(a), Value::Real(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                _ => assert_eq!(wrote, got),
            }
        }
        let nan = params_from_json("[{\"real\":\"NaN\"}, NaN]").unwrap();
        assert!(nan
            .iter()
            .all(|value| matches!(value, Value::Real(r) if r.is_nan())));
    }

    #[test]
    fn a_real_is_never_written_as_an_integer() {
        let mut text = String::new();
        push_real(&mut text, 1e300);
        assert!(text.contains(['.', 'e']), "{text}");
        text.clear();
        push_real(&mut text, 3.0);
        assert_eq!(text, "3.0");
    }

    #[test]
    fn python_spellings_are_read() {
        let read =
            params_from_json("[true, false, Infinity, -Infinity, \"\\ud834\\udd1e\", -7]").unwrap();
        assert_eq!(read[0], Value::Integer(1));
        assert_eq!(read[1], Value::Integer(0));
        assert_eq!(read[2], Value::Real(f64::INFINITY));
        assert_eq!(read[3], Value::Real(f64::NEG_INFINITY));
        assert_eq!(read[4], Value::Text("𝄞".to_string()));
        assert_eq!(read[5], Value::Integer(-7));
    }

    #[test]
    fn malformed_text_is_refused_not_guessed() {
        for bad in [
            "",
            "[1,]",
            "[1 2]",
            "[\"open]",
            "[{\"blob\":\"abc\"}]",
            "[{\"other\":\"x\"}]",
            "[\"\\ud834\"]",
            "[1] x",
            "[[1]]",
        ] {
            assert!(params_from_json(bad).is_err(), "{bad}");
        }
        let too_big = params_from_json("[9223372036854775808]").unwrap_err();
        assert_eq!(too_big.status, Status::TooBig);
    }

    #[test]
    fn rows_of_parameters_read_as_rows() {
        let rows = param_rows_from_json("[[1,\"a\"],[2,null],[]]").unwrap();
        assert_eq!(
            rows,
            vec![
                vec![Value::Integer(1), Value::Text("a".to_string())],
                vec![Value::Integer(2), Value::Null],
                vec![]
            ]
        );
        assert!(param_rows_from_json("[]").unwrap().is_empty());
    }
}
