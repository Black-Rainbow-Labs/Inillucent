//! The JSON and JSON5 text parser.
//!
//! Invariant: what the pinned release accepts, this accepts, and it records
//! *which* dialect each piece came from. Since 3.45 SQLite reads JSON5 -
//! unquoted keys, single-quoted strings, trailing commas, comments,
//! hexadecimal integers, bare leading and trailing decimal points, `Infinity`
//! and `NaN` - and it remembers the difference, because `json_valid(x, 1)` asks
//! whether the text was strict RFC-8259 and `json_valid(x, 2)` asks whether it
//! was JSON5. A parser that normalised on the way in could not answer the first
//! question at all.
//!
//! Two JSON5 spellings are converted rather than recorded, because the format
//! itself has nowhere to put them: `Infinity` becomes the float `9e999` and
//! `NaN` becomes `null`, which is what the pinned release stores.

use super::node::Node;
use super::path::Step;

/// Why a document would not parse, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseFailure {
    /// The one-based byte position `json_error_position()` reports.
    pub position: usize,
}

/// What a successful parse learned about the text it read.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// The document.
    pub node: Node,
    /// Whether anything outside RFC-8259 was used.
    pub used_json5: bool,
}

/// Parses one JSON or JSON5 document, which must be the whole of the text.
pub fn parse(text: &str) -> Result<Parsed, ParseFailure> {
    parse_pruned(text, &[])
}

/// Parses a whole document but builds only the part one path reads.
///
/// **What `json_extract(column, '$.k')` over a column of different documents
/// needs (task-2183).** Every value of the document is still read and checked,
/// so a malformed document fails exactly as [`parse`] fails, and `used_json5`
/// says what it says there. But a member whose label is not the path's next
/// key, and an element before the path's next index, is read without building
/// a node for it, and nothing after the first matching member is built. Each
/// string and number of a document was an allocation of its own, which was
/// most of the cost of extracting one key.
///
/// The tree comes back with the same answer for `steps` that [`parse`]'s tree
/// gives: [`super::path::lookup`] takes an object's first matching member, and
/// that is the only member kept; an array keeps its elements up to the wanted
/// index, the ones before it as `null`. A step this does not prune by - from
/// the end, the append position, a malformed step - builds the rest of that
/// value whole.
///
/// @param text - the document
/// @param steps - the path that will be looked up in the answer
pub fn parse_pruned(text: &str, steps: &[Step]) -> Result<Parsed, ParseFailure> {
    parse_pruned_set(text, &[steps])
}

/// Parses a whole document but builds only the parts a set of paths read.
///
/// **What several `json_extract` calls over one document need.** Each call reads a different
/// path of the same text, so the document is parsed once for all of them: a member is kept when
/// any path steps into it, and an element up to the highest index any path wants. The tree
/// answers each path in the set exactly as [`parse`]'s tree does, and a path that is not in
/// the set may find nothing there.
///
/// @param text - the document
/// @param paths - the paths that will be looked up in the answer
pub fn parse_pruned_set(text: &str, paths: &[&[Step]]) -> Result<Parsed, ParseFailure> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        at: 0,
        used_json5: false,
        depth: 0,
        skipping: false,
    };
    parser.skip_space();
    // SQLite leaves the error offset at zero for a document with nothing in it and for
    // text after a complete value, so `json_error_position()` answers 1 for both,
    // wherever the text is (bug hunt 10).
    if parser.at == parser.bytes.len() {
        return Err(parser.fail_at(0));
    }
    let node = parser.value_pruned(paths)?;
    parser.skip_space();
    // SQLite reads the text as a C string, so a zero byte after a complete value
    // ends the document and whatever follows it is never read (bug hunt 10).
    if parser.at != parser.bytes.len() && parser.peek() != Some(0) {
        return Err(parser.fail_at(0));
    }
    Ok(Parsed {
        node,
        used_json5: parser.used_json5,
    })
}

/// How deep a document may nest before it is refused.
///
/// SQLite's own limit, and it is a limit rather than a preference: the parser
/// descends recursively, so an unbounded document is a stack overflow rather
/// than an error. Refusing it is the only way the failure can be reported.
const MAX_DEPTH: usize = 1000;

/// The parser's position in one document.
struct Parser<'text> {
    bytes: &'text [u8],
    at: usize,
    used_json5: bool,
    depth: usize,
    /// Whether the value being read is being checked and passed over, so
    /// nothing it holds is kept: a string or number is an empty node and a
    /// container keeps no children.
    skipping: bool,
}

impl Parser<'_> {
    /// Returns the failure for the current position.
    ///
    /// The position is one-based and counts bytes, which is what
    /// `json_error_position()` reports; a zero answer from that function means
    /// the document parsed.
    fn fail(&self) -> ParseFailure {
        ParseFailure {
            position: self.at.saturating_add(1),
        }
    }

    /// Returns the failure for the byte at `index`.
    ///
    /// SQLite points a malformed number at a byte other than the one the parser
    /// stopped on: the exponent letter or its sign, the `x` of a hexadecimal
    /// number with no digits, the second digit of a number with a leading zero,
    /// and the start of a number that has no digits at all (bug hunt 10).
    ///
    /// @param index - the zero based byte offset of the error
    fn fail_at(&self, index: usize) -> ParseFailure {
        ParseFailure {
            position: index.saturating_add(1),
        }
    }

    /// Returns the byte at the cursor.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Returns the byte at an offset from the cursor.
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.at.saturating_add(offset)).copied()
    }

    /// Returns the byte length of the three byte Unicode space at the cursor, or zero.
    ///
    /// These are the separators the pinned release reads as JSON5 whitespace beyond the
    /// no-break space: U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F, U+205F, U+3000
    /// and the byte-order mark U+FEFF.
    fn unicode_space_len(&self) -> usize {
        let (Some(a), Some(b), Some(c)) = (self.peek(), self.peek_at(1), self.peek_at(2)) else {
            return 0;
        };
        let is_space = matches!(
            (a, b, c),
            (0xe1, 0x9a, 0x80)
                | (0xe2, 0x81, 0x9f)
                | (0xe3, 0x80, 0x80)
                | (0xef, 0xbb, 0xbf)
                | (0xe2, 0x80, 0x80..=0x8a | 0xa8 | 0xa9 | 0xaf)
        );
        if is_space {
            3
        } else {
            0
        }
    }

    /// Skips whitespace and, in JSON5, comments.
    fn skip_space(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.at += 1,
                // Vertical tab and form feed are JSON5 whitespace and not
                // RFC 8259 whitespace, so `json_valid(X)` with its default
                // flags refuses a document that uses them, as SQLite does.
                Some(0x0b | 0x0c) => {
                    self.used_json5 = true;
                    self.at += 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    self.used_json5 = true;
                    self.at += 2;
                    // **A line comment ends at a line feed, a carriage return or U+2028 and
                    // U+2029**, and the terminator belongs to the comment, as in SQLite.
                    while let Some(byte) = self.peek() {
                        let separator = byte == 0xe2
                            && self.peek_at(1) == Some(0x80)
                            && matches!(self.peek_at(2), Some(0xa8 | 0xa9));
                        if separator {
                            self.at += 3;
                            break;
                        }
                        self.at += 1;
                        if matches!(byte, b'\n' | b'\r') {
                            break;
                        }
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    self.used_json5 = true;
                    let opened = self.at;
                    self.at += 2;
                    while self.peek().is_some()
                        && !(self.peek() == Some(b'*') && self.peek_at(1) == Some(b'/'))
                    {
                        self.at += 1;
                    }
                    if self.peek().is_none() {
                        // **A comment that never closes is an error.** Leaving the `/` where it
                        // is makes the parser meet a character no value starts with, so the
                        // document fails, where it used to be accepted with the comment run to
                        // the end of the text.
                        self.at = opened;
                        return;
                    }
                    self.at += 2;
                }
                // JSON5 counts the Unicode space separators as whitespace.
                Some(0xc2) if self.peek_at(1) == Some(0xa0) => {
                    self.used_json5 = true;
                    self.at += 2;
                }
                Some(0xe1 | 0xe2 | 0xe3 | 0xef) if self.unicode_space_len() > 0 => {
                    self.used_json5 = true;
                    self.at += self.unicode_space_len();
                }
                _ => return,
            }
        }
    }

    /// Parses one value, building only what a set of paths will read; see [`parse_pruned_set`].
    ///
    /// @param paths - the rest of each path, at this value
    fn value_pruned(&mut self, paths: &[&[Step]]) -> Result<Node, ParseFailure> {
        // A path that ends here, or whose next step is not one this prunes by (from the end, the
        // append position, a malformed step), needs the whole value.
        let prunable = |path: &&[Step]| matches!(path.first(), Some(Step::Key(_) | Step::Index(_)));
        if !paths.iter().all(prunable) {
            return self.value();
        }
        match self.peek() {
            Some(b'{') => self.object_pruned(paths),
            Some(b'[') => self.array_pruned(paths),
            // A key or an index meeting a value it cannot step into: the
            // lookup answers nothing whatever the value is.
            _ => {
                self.skip_value()?;
                Ok(Node::Null)
            }
        }
    }

    /// Reads one value and keeps nothing of it.
    fn skip_value(&mut self) -> Result<(), ParseFailure> {
        let was = self.skipping;
        self.skipping = true;
        let read = self.value();
        self.skipping = was;
        read.map(|_| ())
    }

    /// Parses an object, keeping only the first member for each key a path steps into.
    ///
    /// @param paths - the paths, each starting with a step into this object
    fn object_pruned(&mut self, paths: &[&[Step]]) -> Result<Node, ParseFailure> {
        if self.depth >= MAX_DEPTH {
            return Err(self.fail());
        }
        // The keys wanted, each with what follows it in every path that names it.
        let mut wanted: Vec<(&str, Vec<&[Step]>)> = Vec::new();
        for path in paths {
            if let Some((Step::Key(name), rest)) = path.split_first() {
                match wanted.iter_mut().find(|(held, _)| *held == name.as_str()) {
                    Some((_, rests)) => rests.push(rest),
                    None => wanted.push((name.as_str(), vec![rest])),
                }
            }
        }
        let mut taken = vec![false; wanted.len()];
        self.at += 1;
        self.depth += 1;
        let mut members = Vec::new();
        let mut seen = false;
        loop {
            self.skip_space();
            if self.peek() == Some(b'}') {
                if seen {
                    self.used_json5 = true;
                }
                self.at += 1;
                break;
            }
            seen = true;
            let label = self.label_if(&wanted, &taken)?;
            self.skip_space();
            if self.peek() != Some(b':') {
                return Err(self.missing_colon());
            }
            self.at += 1;
            self.skip_space();
            match label {
                Some((label, which)) => {
                    if let Some(flag) = taken.get_mut(which) {
                        *flag = true;
                    }
                    let rests = wanted.get(which).map(|(_, rests)| rests.as_slice());
                    let value = self.value_pruned(rests.unwrap_or(&[]))?;
                    members.push((label, value));
                }
                None => self.skip_value()?,
            }
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.fail()),
            }
        }
        self.depth -= 1;
        Ok(Node::Object(members))
    }

    /// Reads one label and keeps nothing of it.
    fn skip_label(&mut self) -> Result<(), ParseFailure> {
        let was = self.skipping;
        self.skipping = true;
        let read = self.label();
        self.skipping = was;
        read.map(|_| ())
    }

    /// Reads one label and returns it, with which wanted key it spells, when it spells one that
    /// no earlier member took.
    ///
    /// A label with no escape in it is its own spelling, so it is compared
    /// where it lies, with nothing allocated; only a label holding an escape
    /// is built and unescaped to be compared.
    ///
    /// @param wanted - the keys the paths step into, each with the rest of its paths
    /// @param taken - which of them an earlier member already supplied
    fn label_if(
        &mut self,
        wanted: &[(&str, Vec<&[Step]>)],
        taken: &[bool],
    ) -> Result<Option<(Node, usize)>, ParseFailure> {
        let start = self.at;
        if taken.iter().all(|done| *done) {
            self.skip_label()?;
            return Ok(None);
        }
        let was = self.skipping;
        self.skipping = true;
        let read = self.label();
        self.skipping = was;
        let kind = read?;
        let quoted = matches!(self.bytes.get(start), Some(b'"' | b'\''));
        let (from, to) = match quoted {
            true => (start.saturating_add(1), self.at.saturating_sub(1)),
            false => (start, self.at),
        };
        let candidates = wanted
            .iter()
            .enumerate()
            .filter(|(which, _)| !taken.get(*which).copied().unwrap_or(true));
        if matches!(kind, Node::Text(_)) || !quoted {
            for (which, (name, _)) in candidates {
                if self.bytes.get(from..to) == Some(name.as_bytes()) {
                    return Ok(Some((Node::Text((*name).to_string()), which)));
                }
            }
            return Ok(None);
        }
        // An escape: read it again, kept, and compare what it denotes.
        let end = self.at;
        self.at = start;
        let label = self.label()?;
        self.at = end;
        for (which, (name, _)) in candidates {
            if super::path::label_matches(&label, name) {
                return Ok(Some((label, which)));
            }
        }
        Ok(None)
    }

    /// Parses an array, keeping the elements up to the highest index a path wants, the
    /// unwanted ones before it as `null`.
    ///
    /// @param paths - the paths, each starting with a step into this array
    fn array_pruned(&mut self, paths: &[&[Step]]) -> Result<Node, ParseFailure> {
        if self.depth >= MAX_DEPTH {
            return Err(self.fail());
        }
        let mut wanted: Vec<(usize, Vec<&[Step]>)> = Vec::new();
        for path in paths {
            if let Some((Step::Index(index), rest)) = path.split_first() {
                match wanted.iter_mut().find(|(held, _)| held == index) {
                    Some((_, rests)) => rests.push(rest),
                    None => wanted.push((*index, vec![rest])),
                }
            }
        }
        let last = wanted.iter().map(|(index, _)| *index).max();
        self.at += 1;
        self.depth += 1;
        let mut items = Vec::new();
        let mut count = 0usize;
        loop {
            self.skip_space();
            if self.peek() == Some(b']') {
                if count > 0 {
                    self.used_json5 = true;
                }
                self.at += 1;
                break;
            }
            match wanted.iter().find(|(index, _)| *index == count) {
                Some((_, rests)) => {
                    let item = self.value_pruned(rests)?;
                    items.push(item);
                }
                None => {
                    self.skip_value()?;
                    if last.is_some_and(|last| count < last) {
                        items.push(Node::Null);
                    }
                }
            }
            count = count.saturating_add(1);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.fail()),
            }
        }
        self.depth -= 1;
        Ok(Node::Array(items))
    }

    /// Parses one value.
    fn value(&mut self) -> Result<Node, ParseFailure> {
        // Only an array or an object goes a level deeper. Refusing every value
        // at the limit refused `[[...[0]...]]` nested exactly 1000 deep, which
        // SQLite accepts, while the same nesting around nothing passed.
        if self.depth >= MAX_DEPTH && matches!(self.peek(), Some(b'{' | b'[')) {
            return Err(self.fail());
        }
        match self.peek() {
            None => Err(self.fail()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string(b'"'),
            Some(b'\'') => {
                self.used_json5 = true;
                self.string(b'\'')
            }
            Some(b't') if self.word(b"true") => Ok(Node::True),
            Some(b'f') if self.word(b"false") => Ok(Node::False),
            Some(b'n') if self.word(b"null") => Ok(Node::Null),
            // **`nan`, `inf` and `infinity` in any letter case (bug hunt 10).**
            // SQLite compares them without regard to case, so `json('inf')` is
            // `9e999` and `json('nan')` is `null`.
            Some(b'N' | b'n') if self.word_ignoring_case(b"nan") => {
                self.used_json5 = true;
                Ok(Node::Null)
            }
            Some(b'Q' | b'q' | b'S' | b's')
                if self.word_ignoring_case(b"qnan") || self.word_ignoring_case(b"snan") =>
            {
                self.used_json5 = true;
                Ok(Node::Null)
            }
            Some(b'I' | b'i')
                if self.word_ignoring_case(b"infinity") || self.word_ignoring_case(b"inf") =>
            {
                self.used_json5 = true;
                Ok(Node::Float("9e999".to_string()))
            }
            _ => self.number(),
        }
    }

    /// Consumes a bare keyword when it is the next thing in the text.
    ///
    /// The check that follows matters: `nullx` is not `null` followed by `x`,
    /// it is a document that does not parse, and a bare `match` on the first
    /// byte would accept it.
    fn word(&mut self, keyword: &[u8]) -> bool {
        let end = self.at.saturating_add(keyword.len());
        if self.bytes.get(self.at..end) != Some(keyword) {
            return false;
        }
        if self
            .bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return false;
        }
        self.at = end;
        true
    }

    /// Consumes a bare keyword, whatever its letter case, when it is the next
    /// thing in the text and is not the start of a longer word.
    ///
    /// @param keyword - the keyword in lower case
    fn word_ignoring_case(&mut self, keyword: &[u8]) -> bool {
        let end = self.at.saturating_add(keyword.len());
        let Some(found) = self.bytes.get(self.at..end) else {
            return false;
        };
        if !found.eq_ignore_ascii_case(keyword) {
            return false;
        }
        if self
            .bytes
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return false;
        }
        self.at = end;
        true
    }

    /// Parses an object.
    fn object(&mut self) -> Result<Node, ParseFailure> {
        self.at += 1;
        self.depth += 1;
        let mut members = Vec::new();
        let mut seen = false;
        loop {
            self.skip_space();
            if self.peek() == Some(b'}') {
                if seen {
                    // A `}` straight after a comma is JSON5's trailing comma.
                    self.used_json5 = true;
                }
                self.at += 1;
                break;
            }
            seen = true;
            let label = self.label()?;
            self.skip_space();
            if self.peek() != Some(b':') {
                return Err(self.missing_colon());
            }
            self.at += 1;
            self.skip_space();
            let value = self.value()?;
            if !self.skipping {
                members.push((label, value));
            }
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.fail()),
            }
        }
        self.depth -= 1;
        Ok(Node::Object(members))
    }

    /// Parses an object label, which JSON5 also spells without quotes.
    fn label(&mut self) -> Result<Node, ParseFailure> {
        match self.peek() {
            Some(b'"') => self.string(b'"'),
            Some(b'\'') => {
                self.used_json5 = true;
                self.string(b'\'')
            }
            _ if self.starts_bare_key() => self.bare_key(),
            _ => Err(self.fail()),
        }
    }

    /// Reports whether the text at the cursor can be a JSON5 key written without quotes.
    ///
    /// **SQLite reads a key as a value first, and only a text value is a key.** `{null:1}`,
    /// `{true:1}`, `{Infinity:1}` and `{NaN:1}` are refused because the key parses as a
    /// `null`, boolean or number, and a word that merely begins with one of them (`nullx`) is a
    /// key. A key may begin with a letter, `$`, `_`, a `\uXXXX` escape or any byte of a multi byte
    /// character, so `{é:1}` is accepted.
    fn starts_bare_key(&self) -> bool {
        let Some(first) = self.peek() else {
            return false;
        };
        let identifier = first.is_ascii_alphabetic()
            || first == b'_'
            || first == b'$'
            || first >= 0x80
            || self.unicode_escape_at(self.at) > 0;
        identifier && !self.reads_as_a_value()
    }

    /// Reports whether the word at the cursor is `true`, `false`, `null` or a spelling of
    /// infinity or not a number, ending where an identifier would not continue.
    fn reads_as_a_value(&self) -> bool {
        let rest = self.bytes.get(self.at..).unwrap_or(&[]);
        let exact: [&[u8]; 3] = [b"true", b"false", b"null"];
        let any_case: [&[u8]; 5] = [b"infinity", b"inf", b"nan", b"qnan", b"snan"];
        let ends = |length: usize| {
            !rest
                .get(length)
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
        };
        exact
            .iter()
            .any(|word| rest.starts_with(word) && ends(word.len()))
            || any_case.iter().any(|word| {
                rest.get(..word.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(word))
                    && ends(word.len())
            })
    }

    /// Returns 6 when a `\uXXXX` escape starts at `index`, and 0 otherwise.
    ///
    /// @param index - the byte offset to look at
    fn unicode_escape_at(&self, index: usize) -> usize {
        let matches = self.bytes.get(index) == Some(&b'\\')
            && self.bytes.get(index + 1) == Some(&b'u')
            && (2..6).all(|offset| {
                self.bytes
                    .get(index + offset)
                    .is_some_and(u8::is_ascii_hexdigit)
            });
        if matches {
            6
        } else {
            0
        }
    }

    /// Parses a key written without quotes, which is JSON5 text.
    ///
    /// A `\uXXXX` escape in it makes the label the escaped kind, as SQLite stores it, and the
    /// escape is kept as it was written. A byte that begins a Unicode space ends the key.
    fn bare_key(&mut self) -> Result<Node, ParseFailure> {
        self.used_json5 = true;
        let start = self.at;
        let mut escaped = false;
        loop {
            let escape = self.unicode_escape_at(self.at);
            if escape > 0 {
                escaped = true;
                self.at += escape;
                continue;
            }
            let continues = self.peek().is_some_and(|byte| {
                byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
            });
            if !continues || (self.at > start && self.unicode_space_len() > 0) {
                break;
            }
            self.at += 1;
        }
        let name = self.slice(start, self.at)?;
        Ok(if escaped {
            Node::TextJ(name)
        } else {
            Node::Text(name)
        })
    }

    /// Returns the failure for an object label that no colon follows.
    ///
    /// **SQLite tries to read a value where the colon should be**, and when that value is itself
    /// malformed it reports the position of the malformation, not of the missing colon:
    /// `{"a"":1}` fails at 9, inside the string that starts at the second quote, while `{"a" 1}`
    /// fails at 6, where the value begins.
    fn missing_colon(&mut self) -> ParseFailure {
        let at = self.at;
        if matches!(self.peek(), None | Some(b'}' | b']' | b',')) {
            return self.fail();
        }
        match self.value() {
            Err(inside) => inside,
            Ok(_) => self.fail_at(at),
        }
    }

    /// Parses an array.
    fn array(&mut self) -> Result<Node, ParseFailure> {
        self.at += 1;
        self.depth += 1;
        let mut items = Vec::new();
        let mut seen = false;
        loop {
            self.skip_space();
            if self.peek() == Some(b']') {
                if seen {
                    self.used_json5 = true;
                }
                self.at += 1;
                break;
            }
            seen = true;
            let item = self.value()?;
            if !self.skipping {
                items.push(item);
            }
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.fail()),
            }
        }
        self.depth -= 1;
        Ok(Node::Array(items))
    }

    /// Returns a byte range of the source as text.
    fn slice(&self, start: usize, end: usize) -> Result<String, ParseFailure> {
        let bytes = self.bytes.get(start..end).ok_or_else(|| self.fail())?;
        // A value being passed over keeps nothing, and the document is text
        // already, so there is nothing about these bytes to check.
        if self.skipping {
            return Ok(String::new());
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| self.fail())
    }

    /// Parses a string, and decides which of the three text kinds it is.
    ///
    /// The content is kept exactly as written, escapes included, so that
    /// `json()` can hand back the spelling it read. What the scan decides is
    /// only which element type the content belongs to, which is the question
    /// the binary format asks and the renderer answers.
    fn string(&mut self, quote: u8) -> Result<Node, ParseFailure> {
        self.at += 1;
        let start = self.at;
        let mut has_escape = false;
        // The quote decides the dialect of the *document*; the content
        // decides the element type. `'sq'` is JSON5 text holding a plain
        // `TEXT` element, because nothing in `sq` is spelt differently in
        // the two dialects - and the pinned release stores it as `TEXT`.
        let mut needs_json5 = false;
        if quote == b'\'' {
            self.used_json5 = true;
        }
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.fail());
            };
            if byte == quote {
                break;
            }
            // SQLite reads the text as a C string, so a zero byte inside a string is an error.
            if byte == 0 {
                return Err(self.fail());
            }
            if byte == b'"' {
                // A double quote inside a JSON5 single-quoted string has to be
                // escaped on the way out, so the content is not plain text.
                needs_json5 = true;
                self.at += 1;
                continue;
            }
            if byte < 0x20 {
                // A raw control character is JSON5 only, and it too has to be
                // escaped on the way out.
                needs_json5 = true;
                self.at += 1;
                continue;
            }
            if byte != b'\\' {
                self.at += 1;
                continue;
            }
            has_escape = true;
            self.at += 1;
            let Some(escape) = self.peek() else {
                return Err(self.fail());
            };
            match escape {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => self.at += 1,
                b'u' => {
                    // A bad escape is reported at its letter, not at the digit that was missing.
                    let letter = self.at;
                    self.at += 1;
                    for _ in 0..4 {
                        if !self.peek().is_some_and(|byte| byte.is_ascii_hexdigit()) {
                            return Err(self.fail_at(letter));
                        }
                        self.at += 1;
                    }
                }
                // JSON5's own escapes: a line continuation, `\0`, `\v`, `\'`
                // and a two-digit hexadecimal byte.
                b'\n' | b'\'' | b'0' | b'v' | b'\r' => {
                    // `\0` may not be followed by a decimal digit, which would read as an
                    // octal escape that JSON5 does not have.
                    if escape == b'0' && self.peek_at(1).is_some_and(|byte| byte.is_ascii_digit()) {
                        return Err(self.fail());
                    }
                    needs_json5 = true;
                    self.at += 1;
                }
                b'x' => {
                    needs_json5 = true;
                    let letter = self.at;
                    self.at += 1;
                    for _ in 0..2 {
                        if !self.peek().is_some_and(|byte| byte.is_ascii_hexdigit()) {
                            return Err(self.fail_at(letter));
                        }
                        self.at += 1;
                    }
                }
                _ => return Err(self.fail()),
            }
        }
        let content = self.slice(start, self.at)?;
        self.at += 1;
        if needs_json5 {
            self.used_json5 = true;
            return Ok(Node::Text5(content));
        }
        if has_escape {
            return Ok(Node::TextJ(content));
        }
        Ok(Node::Text(content))
    }

    /// Parses a number, in either dialect.
    fn number(&mut self) -> Result<Node, ParseFailure> {
        let start = self.at;
        let mut json5 = false;
        // **A leading `+` is JSON5 and is not stored.** SQLite writes `+1` as the integer `1`,
        // so `jsonb('+1')` is the two bytes 1331 and the offsets after it are one byte smaller.
        let plus = matches!(self.peek(), Some(b'+'));
        if plus {
            self.used_json5 = true;
            self.at += 1;
        } else if matches!(self.peek(), Some(b'-')) {
            self.at += 1;
        }
        if self.word_ignoring_case(b"infinity") || self.word_ignoring_case(b"inf") {
            self.used_json5 = true;
            let negative = self.bytes.get(start) == Some(&b'-');
            return Ok(Node::Float(if negative {
                "-9e999".to_string()
            } else {
                "9e999".to_string()
            }));
        }
        // SQLite refuses a signed `nan`: `+nan` and `-nan` are not JSON5 numbers
        // there, though `-inf` is (bug hunt 10).
        if self.at == start && self.word_ignoring_case(b"nan") {
            self.used_json5 = true;
            return Ok(Node::Null);
        }
        // Hexadecimal, which only JSON5 has and which is always an integer.
        if self.peek() == Some(b'0') && matches!(self.peek_at(1), Some(b'x' | b'X')) {
            self.at += 2;
            let digits = self.at;
            while self.peek().is_some_and(|byte| byte.is_ascii_hexdigit()) {
                self.at += 1;
            }
            if self.at == digits {
                return Err(self.fail_at(digits.saturating_sub(1)));
            }
            self.used_json5 = true;
            return Ok(Node::Int5(self.slice(start + usize::from(plus), self.at)?));
        }
        let mut is_float = false;
        let integer_digits = self.digits();
        if integer_digits == 0 && self.peek() != Some(b'.') {
            return Err(self.fail_at(start));
        }
        // RFC-8259 forbids a leading zero on a multi-digit integer; JSON5 does
        // too, so this is a parse failure in both dialects.
        let first_at = start.saturating_add(usize::from(matches!(
            self.bytes.get(start),
            Some(b'-' | b'+')
        )));
        if integer_digits > 1 && self.bytes.get(first_at) == Some(&b'0') {
            return Err(self.fail_at(first_at.saturating_add(1)));
        }
        if self.peek() == Some(b'.') {
            is_float = true;
            self.at += 1;
            let fraction_digits = self.digits();
            if integer_digits == 0 && fraction_digits == 0 {
                return Err(self.fail_at(start));
            }
            if integer_digits == 0 || fraction_digits == 0 {
                // `.5` and `5.` are JSON5 spellings of a float.
                json5 = true;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            // The error points at the exponent letter, or at its sign when it has one.
            let mut marked = self.at;
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                marked = self.at;
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.fail_at(marked));
            }
        }
        let text = self.slice(start + usize::from(plus), self.at)?;
        if json5 {
            self.used_json5 = true;
        }
        Ok(match (is_float, json5) {
            (true, false) => Node::Float(text),
            (true, true) => Node::Float5(text),
            (false, false) => Node::Int(text),
            (false, true) => Node::Int5(text),
        })
    }

    /// Consumes decimal digits, returning how many there were.
    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        self.at.saturating_sub(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses text, expecting it to succeed.
    fn ok(text: &str) -> Parsed {
        parse(text).expect("the document parses")
    }

    /// Strict JSON parses and is not reported as JSON5.
    #[test]
    fn strict_json_is_not_json5() {
        let parsed = ok(r#"{"a":1,"b":[2,3.5,null,true,false]}"#);
        assert!(!parsed.used_json5);
        assert_eq!(parsed.node.type_name(), "object");
    }

    /// A number keeps the spelling it was written with.
    #[test]
    fn numbers_keep_their_spelling() {
        assert_eq!(ok("1.50").node, Node::Float("1.50".to_string()));
        assert_eq!(ok("-3").node, Node::Int("-3".to_string()));
        assert_eq!(ok("1e3").node, Node::Float("1e3".to_string()));
    }

    /// The JSON5 spellings parse and are recorded as JSON5.
    #[test]
    fn json5_spellings_are_recorded() {
        let parsed = ok("{a:1, b:0x10, c:.5, d:+Infinity, e:NaN, f:'sq', g:[1,2,],}");
        assert!(parsed.used_json5);
        let Node::Object(members) = parsed.node else {
            panic!("an object");
        };
        assert_eq!(members[1].1, Node::Int5("0x10".to_string()));
        assert_eq!(members[2].1, Node::Float5(".5".to_string()));
        assert_eq!(members[3].1, Node::Float("9e999".to_string()));
        assert_eq!(members[4].1, Node::Null);
        assert_eq!(members[5].1, Node::Text("sq".to_string()));
    }

    /// Comments are JSON5 and are skipped wherever whitespace is allowed.
    #[test]
    fn comments_are_skipped() {
        let parsed = ok("[1,/*c*/2, // trailing\n3]");
        assert!(parsed.used_json5);
        let Node::Array(items) = parsed.node else {
            panic!("an array");
        };
        assert_eq!(items.len(), 3);
    }

    /// The Unicode space separators JSON5 allows are whitespace, and each makes the document JSON5.
    #[test]
    fn unicode_spaces_are_whitespace() {
        for space in [
            "\u{a0}", "\u{1680}", "\u{2000}", "\u{200a}", "\u{2028}", "\u{2029}", "\u{202f}",
            "\u{205f}", "\u{3000}", "\u{feff}",
        ] {
            let parsed = ok(&format!("{space}[1,{space}2]"));
            assert!(parsed.used_json5, "{space:?}");
        }
        assert!(parse("\u{200b}1").is_err());
    }

    /// An escape makes a string `TEXTJ`, and no escape leaves it `TEXT`.
    #[test]
    fn string_kinds_follow_their_escapes() {
        assert_eq!(ok(r#""ab""#).node, Node::Text("ab".to_string()));
        assert_eq!(ok(r#""a\nb""#).node, Node::TextJ("a\\nb".to_string()));
        assert_eq!(ok(r#""a\x41""#).node, Node::Text5("a\\x41".to_string()));
    }

    /// A leading zero is refused, in both dialects.
    #[test]
    fn a_leading_zero_is_refused() {
        assert!(parse("01").is_err());
        assert!(parse("[01]").is_err());
        assert_eq!(ok("0").node, Node::Int("0".to_string()));
    }

    /// A failure reports a one-based byte position.
    #[test]
    fn failures_report_a_position() {
        assert_eq!(parse("[1,2").unwrap_err().position, 5);
        assert_eq!(parse("").unwrap_err().position, 1);
    }

    /// Trailing text after a complete value is a failure, not a second value.
    #[test]
    fn trailing_text_is_refused() {
        assert!(parse("1 2").is_err());
        assert!(parse("nullx").is_err());
    }

    /// Nesting is bounded, so a hostile document is an error and not a crash.
    #[test]
    fn nesting_is_bounded() {
        let deep = "[".repeat(2000);
        assert!(parse(&deep).is_err());
        let pruned = super::super::path::parse("$.a").unwrap();
        assert!(parse_pruned(&deep, &pruned).is_err());
    }

    /// A pruned parse answers every path the way a whole parse does, and fails where it fails.
    ///
    /// The documents hold a key twice (the first wins), JSON5 spellings, a
    /// trailing comma, nested containers and values of the wrong kind for the
    /// step; the paths step through objects and arrays, past the end, from the
    /// end and into a scalar.
    #[test]
    fn a_pruned_parse_answers_like_a_whole_one() {
        let documents = [
            r#"{"k":5,"name":"row 5","tags":[5,5]}"#,
            r#"{"k":1,"k":2,"a":{"b":[10,{"c":"x
y"}]}}"#,
            r#"{k:0x10, 'name':'sq', list:[1,2,3,], nested:{deep:[[1],[2,[3]]]}}"#,
            r#"[1,{"k":[true,false,null]},"text",1.5e3]"#,
            r#""just text""#,
            "42",
            r#"{"a":[1,2,3]}"#,
        ];
        let paths = [
            "$",
            "$.k",
            "$.name",
            "$.tags",
            "$.tags[1]",
            "$.a.b[1].c",
            "$.a.b[5]",
            "$.list[3]",
            "$.list[#-1]",
            "$.nested.deep[1][1][0]",
            "$[1].k[2]",
            "$[3]",
            "$.missing",
            "$[0]",
            "$.k.inner",
            "$.a[#]",
        ];
        for document in documents {
            let whole = parse(document).unwrap();
            for path in paths {
                let steps = super::super::path::parse(path).unwrap();
                let pruned = parse_pruned(document, &steps).unwrap();
                assert_eq!(pruned.used_json5, whole.used_json5, "{document} {path}");
                let expected = super::super::path::lookup(&whole.node, &steps)
                    .ok()
                    .flatten();
                let found = super::super::path::lookup(&pruned.node, &steps)
                    .ok()
                    .flatten();
                assert_eq!(found, expected, "{document} {path}");
            }
        }
        for broken in [
            r#"{"k":5,"x":[1,}"#,
            r#"{"a":1} x"#,
            r#"{"a":01}"#,
            r#"[1,2"#,
        ] {
            let steps = super::super::path::parse("$.a").unwrap();
            assert_eq!(
                parse_pruned(broken, &steps).err(),
                parse(broken).err(),
                "{broken}"
            );
        }
    }
}
