//! The built-in functions and the remaining SQL operators, as executor nodes.
//!
//! Invariant: the *semantics* of every function here are `inillucent-scalar`'s, not
//! this module's. `substr`, `strftime`, `LIKE`, `printf`, `CAST`, the bitwise
//! operators and three-valued `IS` all live in one place in this workspace, and
//! the TDD's "port the built-in function set from `inillucent-vm`" was done by
//! moving that code down a layer rather than by copying it. What is in this
//! file is the *bridge*: borrowed page values in, a `Value` call, an answer out.
//! A second implementation of `substr` would be two implementations of `substr`
//! that agree today.
//!
//! ## What the bridge costs, and why it is paid here and not in the scan
//!
//! `inillucent-scalar` takes `Value<'static>`, so a text argument is copied out of
//! the page before the call. That is a real allocation per text argument per
//! row, and it is the reason `length()` is *not* routed through here: it has a
//! specialised node in [`crate::expr`] that reads the leaf's bytes in place,
//! because `range.lookaside` calls it a hundred thousand times.
//!
//! Everything else is on the general path, where a query that calls `upper()`
//! per row was always going to pay for the string it builds. The alternative -
//! a second, borrowing implementation of the whole function set - trades an
//! allocation for a divergence, and the divergence is the expensive one.
//!
//! ## What is not here
//!
//! JSON functions, which need `inillucent-ext`'s binary form and are the TDD's
//! Phase 4; window functions, which are operator state rather than expressions;
//! and subqueries, which are pipelines. Each is refused by name in
//! [`crate::physical`] rather than approximated.

use inillucent_base::error::misuse;
use inillucent_base::DbResult;
use inillucent_scalar::{builtin, datetime, eval, json, mathfn, pattern};

/// What a scalar function may need to know about the connection around it.
///
/// Re-exported so the engine can fill one without a dependency of its own on
/// the function library: it hands a `Params` to the executor, and the executor
/// is what calls the functions.
pub use inillucent_scalar::builtin::Context;
use inillucent_sql::ast::{BinaryOp, UnaryOp};
use inillucent_sql::function::{JsonFunc, MathFunc, ScalarFunc, TimeFunc};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_value::affinity::Affinity;
use inillucent_value::collation::Collation;
use inillucent_value::encoding::TextEncoding;
use inillucent_value::value::Value;
use inillucent_value::{cast, compare};

use crate::batch::Batch;
use crate::expr::{Computed, Eval};

/// The database encoding, which is UTF-8 and only UTF-8.
///
/// The TDD fixes it in the leaf layout: "Text is stored as UTF-8 bytes with no
/// terminator; the database encoding is UTF-8 only." So this is a constant
/// rather than a parameter nobody could vary, and it is named rather than
/// written out at eleven call sites.
const ENCODING: TextEncoding = TextEncoding::Utf8;

/// One of the dialect's pattern operators, for a caller that has two values and
/// needs the answer this crate already knows how to compute.
///
/// **It exists so the engine does not have to reach past this layer.** A virtual
/// table's module may decline a `LIKE`, `GLOB` or `REGEXP` constraint, and what
/// is left is the engine's to test - but `inillucent-engine` sits above
/// `inillucent-scalar` with no edge to it, by the layering contract in
/// `docs/invariants/layering.toml`, so the evaluation belongs here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternOperator {
    /// `subject LIKE pattern`, with `%` and `_`.
    Like,
    /// `subject GLOB pattern`, with `*`, `?` and character classes.
    Glob,
    /// `subject REGEXP pattern`, the port of `ext/misc/regexp.c`.
    Regexp,
}

/// Reports whether one value matches a pattern under one of those operators.
///
/// Both sides are read as text, which is what the pipeline's own `Pattern`
/// expression does. A pattern that will not compile matches nothing, which is
/// the same answer `regexp()` gives.
///
/// @param op - which operator
/// @param subject - the value being tested
/// @param pattern - the pattern
/// @param case_sensitive - `PRAGMA case_sensitive_like`, for `LIKE` only
pub fn matches_pattern(
    op: PatternOperator,
    subject: &Value<'_>,
    pattern: &Value<'_>,
    case_sensitive: bool,
) -> bool {
    let subject = eval::text_bytes(subject, TextEncoding::Utf8);
    let pattern = eval::text_bytes(pattern, TextEncoding::Utf8);
    match op {
        PatternOperator::Like => pattern::like_folding(&pattern, &subject, None, !case_sensitive),
        PatternOperator::Glob => pattern::glob(&pattern, &subject),
        PatternOperator::Regexp => inillucent_scalar::regexp::Regexp::compile(&pattern, false)
            .map(|compiled| compiled.matches(&subject))
            .unwrap_or(false),
    }
}

/// Evaluates a list of argument expressions into owned values.
///
/// @param arguments - the compiled argument expressions
/// @param batch - the batch being evaluated
/// @param nth - the row's position among the live rows
fn arguments_of(
    arguments: &[Box<dyn Eval>],
    batch: &Batch<'_>,
    nth: usize,
) -> DbResult<Vec<Value<'static>>> {
    let mut out = Vec::with_capacity(arguments.len());
    for argument in arguments {
        out.push(Value::from(&argument.value(batch, nth)?.get()).into_owned()?);
    }
    Ok(out)
}

/// A call to one of the dialect's scalar functions.
pub struct ScalarCall {
    /// Which function.
    pub func: ScalarFunc,
    /// The compiled arguments.
    pub arguments: Vec<Box<dyn Eval>>,
    /// The collation the function's comparisons use.
    pub collation: Collation,
    /// What the connection's counters said when the statement began.
    ///
    /// Five built-ins answer a question about the connection rather than about
    /// their arguments, and the new engine used to hand them a default context
    /// - so `changes()`, `total_changes()` and `last_insert_rowid()` answered
    /// `0` for ever and `random()` answered one constant.
    ///
    /// Every field of it is a constant for the length of the statement except
    /// the seed, which is in `stream` because it has to move per call.
    pub context: Context,
    /// The random built-ins' seed, advanced once per evaluation.
    ///
    /// **Per call, not per statement.** `SELECT random(), random()` answers two
    /// different numbers in SQLite, and one call site evaluated once per row
    /// has to answer a different number per row. An atomic rather than a
    /// `Cell` because a compiled expression is `Sync`: the executor's node
    /// trait requires it, and a plan may be shared.
    pub stream: std::sync::atomic::AtomicU64,
}

impl Eval for ScalarCall {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        if let Some(answer) = self.lazy_value(batch, nth)? {
            return Ok(answer);
        }
        let values = arguments_of(&self.arguments, batch, nth)?;
        let context = Context {
            seed: self.next_seed(),
            ..self.context
        };
        // A vector measure over a mismatched pair refuses rather than answering
        // NULL, so a ranking query cannot come back ordered by a distance
        // nobody took.
        if let Some(said) = builtin::refusal_for(self.func, &values) {
            // **A function that refuses is `SQLITE_ERROR`, not `SQLITE_MISUSE`
            // (task-1913).** Every refusal SQLite raises from inside a scalar
            // goes through `sqlite3_result_error`, which sets code 1; 21 is
            // what it answers for misusing the C API, which a caller writing
            // SQL cannot do. Measured against the pinned 3.53.4: `SELECT
            // abs(-9223372036854775808)` and `SELECT unistr('\x')` both come
            // back as code 1.
            return Err(inillucent_base::error::statement_refusal(said));
        }
        // **Before the allocation, where the size is knowable from the
        // arguments (task-1979, section 5.4).** `zeroblob(1073741824)` and
        // `printf('%2000000000d', 1)` say how many bytes they will take before
        // they take them, and a check afterwards is a check the process has
        // already paid 5.7 GB of working set for.
        if let Some(wanted) = size_asked_for(self.func, &values) {
            if !context.permits_length(wanted) {
                return Err(too_big());
            }
            // And the served budget hears about it, so a value that is inside
            // the value bound and outside the request's own ceiling stops here
            // rather than when the row completes.
            if wanted >= MATERIALISED_AT {
                inillucent_base::budget::materialise(wanted)?;
            }
        }
        let answer = builtin::call_with(self.func, &values, self.collation, ENCODING, context);
        let built = OwnedDatum::from(answer);
        // And afterwards for every function whose answer's size only the answer
        // knows - `replace`, `hex`, `char`, `group_concat`'s separator work and
        // the rest.
        let produced = value_bytes(&built);
        if !context.permits_length(produced) {
            return Err(too_big());
        }
        if produced >= MATERIALISED_AT {
            inillucent_base::budget::materialise(produced)?;
        }
        Ok(Computed::Owned(built))
    }
}

/// How large a single value has to be before the request's own budget is
/// charged for it.
///
/// **A threshold rather than every value**, because charging the budget for a
/// four-byte integer would be a thread-local borrow per cell and would count
/// the same bytes the row sink counts. One mebibyte is where a single value
/// stops being a cell and starts being an allocation somebody should be told
/// about (task-1979, section 5.4).
const MATERIALISED_AT: u64 = 1 << 20;

/// Returns the refusal SQLite gives for a value past its length limit.
fn too_big() -> inillucent_base::error::DbError {
    inillucent_base::error::DbError::primary(inillucent_base::error::PrimaryCode::TooBig)
        .with_message("string or blob too big")
        .with_detail("string or blob too big")
}

/// Returns how many bytes a value occupies.
///
/// @param value - the value produced
fn value_bytes(value: &OwnedDatum) -> u64 {
    match value {
        OwnedDatum::Text(bytes) | OwnedDatum::Blob(bytes) => bytes.len() as u64,
        _ => 0,
    }
}

/// Returns how many bytes a call will produce, when its arguments say.
///
/// `None` for every function whose answer's size is not a function of its
/// arguments alone; those are checked once the answer exists.
///
/// @param func - the function being called
/// @param values - its arguments
fn size_asked_for(func: ScalarFunc, values: &[inillucent_value::Value<'static>]) -> Option<u64> {
    match func {
        ScalarFunc::ZeroBlob | ScalarFunc::RandomBlob => {
            let asked = inillucent_value::cast::integer_value(values.first()?);
            (asked > 0).then_some(asked as u64)
        }
        // A format string's width fields are what make `printf` allocate, and
        // they are in the string before anything is written.
        ScalarFunc::Printf => inillucent_scalar::printf::widest_request(values),
        _ => None,
    }
}

impl ScalarCall {
    /// Answers `coalesce`, `ifnull` and `iif` without evaluating the arguments
    /// the answer does not need.
    ///
    /// SQLite stops at the first argument that decides the answer, so
    /// `coalesce(1, abs(-9223372036854775807 - 1))` is 1 and the overflow in the
    /// second argument is never raised. `None` for every other function, which
    /// reads all of its arguments.
    ///
    /// @param batch - the batch being evaluated
    /// @param nth - the row's position among the live rows
    fn lazy_value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Option<Computed<'p>>> {
        match self.func {
            ScalarFunc::Coalesce | ScalarFunc::IfNull => {
                for argument in &self.arguments {
                    let value = argument.value(batch, nth)?;
                    if !value.get().is_null() {
                        return Ok(Some(value));
                    }
                }
                Ok(Some(Computed::Owned(OwnedDatum::Null)))
            }
            ScalarFunc::Iif => {
                let mut at = 0usize;
                while at.saturating_add(1) < self.arguments.len() {
                    let Some(condition) = self.arguments.get(at) else {
                        break;
                    };
                    let verdict = condition.value(batch, nth)?;
                    if crate::expr::truth(&verdict.get()) == Some(true) {
                        return match self.arguments.get(at.saturating_add(1)) {
                            Some(answer) => answer.value(batch, nth).map(Some),
                            None => Ok(Some(Computed::Owned(OwnedDatum::Null))),
                        };
                    }
                    at = at.saturating_add(2);
                }
                match self.arguments.get(at) {
                    Some(otherwise) => otherwise.value(batch, nth).map(Some),
                    None => Ok(Some(Computed::Owned(OwnedDatum::Null))),
                }
            }
            _ => Ok(None),
        }
    }

    /// Returns the next seed for the random built-ins, moving the stream on.
    ///
    /// `splitmix64`, which is the function library's own scrambler, so adjacent
    /// seeds give unrelated values rather than correlated ones.
    fn next_seed(&self) -> u64 {
        let held = self
            .stream
            .fetch_add(0x9E37_79B9_7F4A_7C15, std::sync::atomic::Ordering::Relaxed);
        let mut z = held.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// A call to one of the JSON built-ins.
///
/// **The document is parsed once and kept**, which is the TDD's Phase 4 line
/// about JSON "over a binary form, parsed once rather than per call". The cache
/// is keyed by the first argument's bytes, because that is the document every
/// one of these functions reads and it is the argument that is constant in the
/// shapes that matter: a `WHERE json_extract(body, '$.k') = ?` over a scan reads
/// a different document per row and gains nothing, while
/// `SELECT json_extract('{...}', '$.b.c')` reads one document however many times
/// it is called.
///
/// The cached form is the **JSONB blob**, not the parsed tree. Two reasons, and
/// the second is the load-bearing one: a blob is what `jsonb()` hands an
/// application and what a column holds, so caching it means the cached path and
/// the stored path are the same path; and `json::call` takes values, so a cache
/// of trees would need a second entry point into the function set that could
/// disagree with the first.
///
/// The cache is one entry deep. A second entry would need an eviction policy and
/// a hash, to answer a question - "is this the same document as last time" - that
/// one comparison answers for every shape this is for.
///
/// Behind a `Mutex` rather than a `RefCell` because an `Eval` is `Sync`: the
/// pipeline is single-threaded today and the trait does not promise it will stay
/// that way. An uncontended lock is tens of nanoseconds against a parse of
/// microseconds, and it is taken only on the path that would otherwise parse.
///
/// ## Why a nested JSON call is not an ordinary argument
///
/// A JSON function's answer carries a **subtype**: whether the value *is* JSON
/// or merely looks like it. `json_object('k', json_extract(body, '$.b'))` has to
/// answer an object nested inside an object, and it can only do that if it is
/// told its second argument is a document rather than a string that happens to
/// spell one. Told nothing, it quotes the string - which is what this did until
/// the differential probe asked it.
///
/// The subtype is a runtime fact: `json_extract` marks its answer only when what
/// it extracted is a container. So it cannot be settled by looking at the tree,
/// and the JSON subtree evaluates *itself* - a nested call is held as a
/// `JsonCall` rather than as a `dyn Eval`, and the mark travels with the value
/// to whichever call consumes it. Outside the subtree the mark is gone, which is
/// correct: a `Computed` has nowhere to put one, and a value that leaves the
/// JSON functions is an ordinary SQL value.
pub struct JsonCall {
    /// Which function.
    func: JsonFunc,
    /// The compiled arguments.
    arguments: Vec<JsonOperand>,
    /// The last document seen, and its binary form.
    cached: std::sync::Mutex<Option<(Vec<u8>, Vec<u8>)>>,
    /// The last document and path seen, and what they parsed to.
    ///
    /// **A second cache rather than a bigger one, because it answers a
    /// different question.** `cached` above holds the *JSONB* of a document, so
    /// that a function which hands its argument on as a value can hand on a
    /// blob instead of re-parsed text. This one holds the parsed `Node` and the
    /// parsed path, which is what `json_extract` actually reads - and decoding
    /// that blob back into a tree was the cost the first cache left behind: the
    /// gate's `extension.json` ran the same two literals four thousand times
    /// and paid a full document decode and a path parse on every one.
    ///
    /// One lock rather than two, because the two are read together on every
    /// call and a second lock is a second uncontended atomic for nothing.
    extract: std::sync::Mutex<ExtractCache>,
    /// The answer, kept, when every argument is a literal.
    ///
    /// **A JSON function of literals answers the same thing every time
    /// (task-2183).** The JSON functions are deterministic, so
    /// `SELECT json_extract('{"b":{"c":"d"}}', '$.b.c')` run four thousand
    /// times extracted the same value four thousand times. `None` when an
    /// argument is not a literal; the inner `None` until the first call.
    constant: Option<std::sync::Mutex<Option<json::Answer>>>,
}

/// What a repeated `json_extract` does not have to parse again.
#[derive(Default)]
struct ExtractCache {
    /// The last document seen, and the tree it parsed to.
    document: Option<(Vec<u8>, json::Node)>,
    /// The last path seen, and the steps it parsed to.
    steps: Option<(Vec<u8>, Vec<json::path::Step>)>,
    /// How many calls in a row asked about a document other than the held one.
    ///
    /// **A column of distinct documents misses every row (task-2175).** Each
    /// miss copied the document twice, once to parse and once to keep, and
    /// freed the tree it replaced: `json_extract(body, '$.k') = ?` over 5,000
    /// rows spent 43% of its time in the system heap. After a few misses in a
    /// row the document is parsed where it lies and not kept.
    misses: u32,
}

/// How many misses in a row stop the cache keeping documents.
const EXTRACT_MISSES_BEFORE_PASSING: u32 = 4;

/// One argument of a JSON call.
enum JsonOperand {
    /// Another JSON call, whose answer carries its own subtype.
    Nested(Box<JsonCall>),
    /// Any other expression, whose answer is a plain SQL value.
    Plain(Box<dyn Eval>),
}

impl JsonCall {
    /// Returns a call over operands that may themselves be JSON calls.
    ///
    /// @param func - which JSON function
    /// @param arguments - the operands, in order
    fn over(func: JsonFunc, arguments: Vec<JsonOperand>) -> JsonCall {
        JsonCall {
            func,
            arguments,
            cached: std::sync::Mutex::new(None),
            extract: std::sync::Mutex::new(ExtractCache::default()),
            constant: None,
        }
    }

    /// Returns the document argument as JSONB, parsing it only if it is new.
    ///
    /// `None` when there is nothing to cache - a NULL document, a number, or a
    /// document that is already a blob, which is JSONB and needs no parse.
    ///
    /// @param first - the first argument's value
    fn binary(&self, first: &Value<'static>) -> Option<Value<'static>> {
        let Value::Text(text) = first else {
            return None;
        };
        let source = text.utf8_bytes().into_owned();
        if let Ok(held) = self.cached.lock() {
            if let Some((seen, blob)) = held.as_ref() {
                if *seen == source {
                    return Value::owned_blob(blob).ok();
                }
            }
        }
        let node = json::document(&json::Argument::plain(first)).ok()??;
        let mut blob = Vec::new();
        json::binary::encode(&node, &mut blob);
        let answer = Value::owned_blob(&blob).ok()?;
        if let Ok(mut held) = self.cached.lock() {
            *held = Some((source, blob));
        }
        Some(answer)
    }

    /// Answers `json_extract(document, path)` without allocating an argument
    /// vector, when the call has that shape and its arguments repeat.
    ///
    /// `None` means "not this shape", and the caller falls through to the
    /// general path - which is every other JSON function, every multi-path
    /// extract, and any argument that is not text or a blob.
    ///
    /// **It evaluates its own two operands** rather than being handed the
    /// general path's `values` and `marks`, because building those is two heap
    /// allocations and the whole point of this path is that a repeated call
    /// costs neither a parse nor an allocation.
    ///
    /// The document is keyed by its own bytes, so a column of different
    /// documents misses every time and costs one extra comparison; a literal or
    /// a repeated value hits every time and parses once. That is the same
    /// bargain `cached` already made, applied to the form the answer is
    /// actually read out of.
    ///
    /// @param batch - the batch being evaluated
    /// @param nth - the position among the batch's live rows
    fn extract_cached(&self, batch: &Batch<'_>, nth: usize) -> DbResult<Option<json::Answer>> {
        let binary = match self.func {
            JsonFunc::Extract => false,
            JsonFunc::ExtractB => true,
            _ => return Ok(None),
        };
        let (Some(JsonOperand::Plain(left)), Some(JsonOperand::Plain(right))) =
            (self.arguments.first(), self.arguments.get(1))
        else {
            return Ok(None);
        };
        if self.arguments.len() != 2 {
            return Ok(None);
        }
        // **Read as borrowed bytes, not as `Value`s.** Converting to an
        // owned `Value` copies what it
        // is given, so converting first copied the whole document and the whole
        // path on every call - which on the gate's four thousand identical
        // calls is four thousand copies of a document the cache already holds.
        // The copy now happens only on a miss, where it has to.
        let left = left.value(batch, nth)?;
        let right = right.value(batch, nth)?;
        let Datum::Text(path) = right.get() else {
            return Ok(None);
        };
        let (document, blob) = match left.get() {
            Datum::Text(bytes) => (bytes, false),
            Datum::Blob(bytes) => (bytes, true),
            _ => return Ok(None),
        };
        let Ok(mut held) = self.extract.lock() else {
            return Ok(None);
        };
        if held.steps.as_ref().map(|(seen, _)| seen.as_slice()) != Some(path) {
            let Ok(text) = std::str::from_utf8(path) else {
                return Ok(None);
            };
            held.steps = Some((path.to_vec(), json::path::parse(text)?));
        }
        if held.document.as_ref().map(|(seen, _)| seen.as_slice()) == Some(document) {
            held.misses = 0;
        } else {
            held.misses = held.misses.saturating_add(1);
        }
        // Every 64th miss is still kept, so a column whose documents start
        // repeating finds the cache again.
        if held.misses >= EXTRACT_MISSES_BEFORE_PASSING && held.misses % 64 != 0 && !blob {
            // Parsed from the bytes where they lie, answered, and dropped.
            let Ok(text) = std::str::from_utf8(document) else {
                return Err(json::malformed());
            };
            let Some((_, steps)) = held.steps.as_ref() else {
                return Ok(None);
            };
            // Only the part of the document the path reads is built.
            let node = json::parse::parse_pruned(text, steps)
                .map_err(|_| json::malformed())?
                .node;
            return Ok(Some(json::extract_parsed(&node, steps, binary)?));
        }
        if held.document.as_ref().map(|(seen, _)| seen.as_slice()) != Some(document) {
            let owned = if blob {
                Value::owned_blob(document)
            } else {
                Value::owned_text(document)
            };
            let Ok(owned) = owned else {
                return Ok(None);
            };
            let argument = json::Argument {
                value: &owned,
                json: false,
            };
            let Some(node) = json::document(&argument)? else {
                return Ok(Some(json::Answer {
                    value: Value::Null,
                    json: false,
                }));
            };
            held.document = Some((document.to_vec(), node));
        }
        let (Some((_, node)), Some((_, steps))) = (held.document.as_ref(), held.steps.as_ref())
        else {
            return Ok(None);
        };
        Ok(Some(json::extract_parsed(node, steps, binary)?))
    }

    /// Evaluates the call, keeping the subtype its answer carries.
    ///
    /// @param batch - the batch being evaluated
    /// @param nth - the position among the batch's live rows
    fn answer(&self, batch: &Batch<'_>, nth: usize) -> DbResult<json::Answer> {
        let Some(constant) = &self.constant else {
            return self.computed(batch, nth);
        };
        if let Some(kept) = constant.lock().ok().and_then(|held| held.clone()) {
            return Ok(kept);
        }
        // An error is not kept: the next call reports it again.
        let answer = self.computed(batch, nth)?;
        if let Ok(mut held) = constant.lock() {
            *held = Some(answer.clone());
        }
        Ok(answer)
    }

    /// Evaluates the call, keeping the subtype its answer carries.
    ///
    /// @param batch - the batch being evaluated
    /// @param nth - the position among the batch's live rows
    fn computed(&self, batch: &Batch<'_>, nth: usize) -> DbResult<json::Answer> {
        // The single-path `json_extract` shape, answered from the cache above
        // without parsing or allocating anything a previous row already did.
        // Every other shape falls through to the general path below.
        if let Some(answer) = self.extract_cached(batch, nth)? {
            return Ok(answer);
        }
        let mut values: Vec<Value<'static>> = Vec::with_capacity(self.arguments.len());
        let mut marks: Vec<bool> = Vec::with_capacity(self.arguments.len());
        for operand in &self.arguments {
            match operand {
                JsonOperand::Nested(call) => {
                    let answer = call.answer(batch, nth)?;
                    values.push(answer.value);
                    marks.push(answer.json);
                }
                JsonOperand::Plain(eval) => {
                    values.push(Value::from(&eval.value(batch, nth)?.get()).into_owned()?);
                    marks.push(false);
                }
            }
        }
        // Only the *document* is cached, and only when it arrived as unmarked
        // text. A later argument is read as a value rather than as a document,
        // so swapping one of those for a blob would change the answer.
        //
        // **And `json_valid` is never given the substitution at all**, because
        // it is the one function whose question is about the *text* rather than
        // about the document it denotes: its flags ask whether the argument was
        // RFC-8259, JSON5, or JSONB. Handing it the parsed blob answered every
        // one of those about the blob, so `json_valid('{}')` was 0 where SQLite
        // says 1 and `json_valid('{}', 4)` was 1 where SQLite says 0 - wrong in
        // both directions, from an optimisation that is invisible everywhere
        // else.
        if marks.first() == Some(&false)
            && self.func != JsonFunc::Valid
            && self.func.first_argument_is_a_document()
        {
            let replacement = values.first().and_then(|first| self.binary(first));
            if let (Some(blob), Some(slot)) = (replacement, values.first_mut()) {
                *slot = blob;
            }
        }
        let arguments: Vec<json::Argument<'_>> = values
            .iter()
            .zip(marks.iter())
            .map(|(value, json)| json::Argument { value, json: *json })
            .collect();
        json::call(self.func, &arguments)
    }
}

impl Eval for JsonCall {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        Ok(Computed::Owned(OwnedDatum::from(
            self.answer(batch, nth)?.value,
        )))
    }

    fn json_value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<(Computed<'p>, bool)> {
        let answer = self.answer(batch, nth)?;
        Ok((Computed::Owned(OwnedDatum::from(answer.value)), answer.json))
    }
}

/// Compiles one JSON call and its arguments, keeping the nesting.
///
/// The recursion is what carries the subtype: an argument that is itself a JSON
/// call is compiled as one rather than as a `dyn Eval`, so its answer's mark
/// reaches the call above it.
///
/// @param func - which JSON function
/// @param arguments - the argument expressions
/// @param types - the static type of each input column
pub fn compile_json(
    func: JsonFunc,
    arguments: &[crate::expr::Expr],
    types: &[crate::expr::StaticType],
) -> DbResult<JsonCall> {
    let mut operands = Vec::with_capacity(arguments.len());
    for argument in arguments {
        operands.push(match argument {
            crate::expr::Expr::Json {
                func: inner,
                arguments: nested,
            } => JsonOperand::Nested(Box::new(compile_json(*inner, nested, types)?)),
            other => JsonOperand::Plain(crate::expr::compile(other, types)?),
        });
    }
    let mut call = JsonCall::over(func, operands);
    if all_literal(arguments) {
        call.constant = Some(std::sync::Mutex::new(None));
    }
    Ok(call)
}

/// Reports whether every argument of a JSON call is a literal or a JSON call of literals.
///
/// @param arguments - the call's argument expressions
fn all_literal(arguments: &[crate::expr::Expr]) -> bool {
    arguments.iter().all(|argument| match argument {
        crate::expr::Expr::Literal(_) => true,
        crate::expr::Expr::Json {
            arguments: nested, ..
        } => all_literal(nested),
        _ => false,
    })
}

/// A call to one of the math functions.
pub struct MathCall {
    /// Which function.
    pub func: MathFunc,
    /// The compiled arguments.
    pub arguments: Vec<Box<dyn Eval>>,
}

impl Eval for MathCall {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let values = arguments_of(&self.arguments, batch, nth)?;
        Ok(Computed::Owned(OwnedDatum::from(mathfn::call(
            self.func, &values,
        ))))
    }
}

/// A call to one of the date and time functions.
pub struct TimeCall {
    /// Which function.
    pub func: TimeFunc,
    /// The compiled arguments.
    pub arguments: Vec<Box<dyn Eval>>,
    /// The julian day the statement calls "now".
    ///
    /// Fixed for the whole statement rather than read per row, which is what
    /// SQLite does: every `now` in one statement is the same instant, or a
    /// query could see two.
    pub now: f64,
}

impl Eval for TimeCall {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let values = arguments_of(&self.arguments, batch, nth)?;
        Ok(Computed::Owned(OwnedDatum::from(datetime::call(
            self.func, &values, self.now, ENCODING,
        ))))
    }
}

/// An arithmetic, bitwise or concatenation operator over anything.
///
/// The specialised integer arithmetic in [`crate::expr`] covers `+`, `-` and
/// `*`; this covers those too when the operands are not integral, and covers
/// `/`, `%`, `||` and the bitwise operators, which the specialisation never
/// claimed.
pub struct GeneralArith {
    /// Which operator.
    pub op: BinaryOp,
    /// The left operand.
    pub left: Box<dyn Eval>,
    /// The right operand.
    pub right: Box<dyn Eval>,
    /// The largest value this connection admits, in bytes.
    ///
    /// **`||` is the one operator that builds a value out of two others**, so
    /// it is the one that can produce something past `Limit::Length` without
    /// any function being called. `WITH RECURSIVE c(s) AS (SELECT 'aa' UNION
    /// ALL SELECT s||s FROM c)` reached 49 GB before the harness gave up
    /// (task-1979, section 5.4). Zero means unbounded, which is what the unit
    /// tests below construct.
    pub length_limit: i64,
}

impl Eval for GeneralArith {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let left = self.left.value(batch, nth)?;
        let right = self.right.value(batch, nth)?;
        self.apply(left, right)
    }
}

impl GeneralArith {
    /// Applies the operator to two operands already evaluated.
    ///
    /// @param left - the left operand's value
    /// @param right - the right operand's value
    fn apply<'p>(&self, left: Computed<'p>, right: Computed<'p>) -> DbResult<Computed<'p>> {
        // **Two integers are answered here (task-2183).** `a.id % 100` over a
        // scan made each operand an owned value and classified it as a number
        // again for every row, which was half of a 400 row filter. The answer
        // is `integer_arithmetic`'s, the rule the general path applies to two
        // integers first; an overflow still falls through to it.
        if let (Datum::Int(a), Datum::Int(b)) = (left.get(), right.get()) {
            // An integer or a NULL is handed back as it is, with no owned value
            // to build and drop for every row.
            match eval::integer_arithmetic(self.op, a, b) {
                Some(Value::Integer(answer)) => return Ok(Computed::Borrowed(Datum::Int(answer))),
                Some(Value::Null) => return Ok(Computed::Borrowed(Datum::Null)),
                Some(answer) => return Ok(Computed::Owned(OwnedDatum::from(answer))),
                None => {}
            }
        }
        // **Two texts are joined here (task-2183).** `note || '!'` over every
        // row of a table copied both sides into owned values and grew the
        // answer as it went, which was a fifth of `UPDATE side_table SET note =
        // note || '!'`. The bytes are joined once, into a list of the final
        // length, under the same size rules as below.
        if let (BinaryOp::Concat, Datum::Text(first), Datum::Text(second)) =
            (self.op, left.get(), right.get())
        {
            let wanted = first.len().saturating_add(second.len());
            if self.length_limit > 0 && wanted as u64 > self.length_limit as u64 {
                return Err(too_big());
            }
            if wanted as u64 >= MATERIALISED_AT {
                inillucent_base::budget::materialise(wanted as u64)?;
            }
            let mut joined = Vec::with_capacity(wanted);
            joined.extend_from_slice(first);
            joined.extend_from_slice(second);
            return Ok(Computed::Owned(OwnedDatum::Text(joined)));
        }
        let left = Value::from(&left.get()).into_owned()?;
        let right = Value::from(&right.get()).into_owned()?;
        // **Before the concatenation, because the size is the sum of two
        // values already in hand.** Checking afterwards is checking a value the
        // process has already allocated, and the recursive doubling above
        // doubles it every round.
        if self.op == BinaryOp::Concat {
            let wanted = text_length_of(&left).saturating_add(text_length_of(&right));
            if self.length_limit > 0 && wanted > self.length_limit as u64 {
                return Err(too_big());
            }
            if wanted >= MATERIALISED_AT {
                inillucent_base::budget::materialise(wanted)?;
            }
        }
        let answer = eval::arithmetic(self.op, &left, &right, ENCODING);
        Ok(Computed::Owned(OwnedDatum::from(answer)))
    }
}

/// `x % n` or `x / n` for a constant integer `n` that is neither 0 nor -1.
///
/// **Neither can overflow nor be NULL for an integer `x` (task-2183)**, so an
/// integer is answered with one machine instruction and nothing else. `key %
/// 50000` over every row of a table went through the general node's operand
/// checks and its answer conversion: 12% of `GROUP BY key % 50000`. Anything
/// that is not an integer is handed to the general node.
pub struct IntByConstant {
    /// The constant divisor, never 0 or -1.
    pub divisor: i64,
    /// The same operator over the same operands, for every other value.
    pub general: GeneralArith,
}

impl Eval for IntByConstant {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let left = self.general.left.value(batch, nth)?;
        if let Datum::Int(number) = left.get() {
            let answer = match self.general.op {
                BinaryOp::Modulo => number.checked_rem(self.divisor),
                _ => number.checked_div(self.divisor),
            };
            if let Some(answer) = answer {
                return Ok(Computed::Borrowed(Datum::Int(answer)));
            }
        }
        let right = self.general.right.value(batch, nth)?;
        self.general.apply(left, right)
    }
}

/// Returns how many bytes a value's text form takes.
///
/// A number's text form is bounded by its own digits, so only text and blobs
/// can make a concatenation large; everything else is counted as the short
/// thing it is.
///
/// @param value - the operand
fn text_length_of(value: &Value<'static>) -> u64 {
    match value {
        Value::Text(text) => text.raw().len() as u64,
        Value::Blob(blob) => blob.raw().len() as u64,
        _ => 32,
    }
}

/// A unary operator: `-`, `+` or `~`.
pub struct Unary {
    /// Which operator.
    pub op: UnaryOp,
    /// The operand.
    pub operand: Box<dyn Eval>,
}

impl Eval for Unary {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let value = Value::from(&operand.get()).into_owned()?;
        let answer = match self.op {
            UnaryOp::Negate => eval::negate(&value),
            // Unary plus is the identity in SQLite - it does not even apply a
            // numeric affinity - so the operand is handed back unchanged.
            UnaryOp::Identity => return Ok(operand),
            UnaryOp::BitNot => eval::bit_not(&value),
            UnaryOp::Not => eval::logical_not(&value),
        };
        Ok(Computed::Owned(OwnedDatum::from(answer)))
    }
}

/// `CAST(x AS type)`.
pub struct Cast {
    /// The operand.
    pub operand: Box<dyn Eval>,
    /// The affinity the declared type maps to.
    pub affinity: Affinity,
}

impl Eval for Cast {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let value = Value::from(&operand.get()).into_owned()?;
        let answer = cast::cast_value(value, self.affinity, ENCODING)
            .map_err(|_| misuse("a cast could not be evaluated"))?;
        Ok(Computed::Owned(OwnedDatum::from(answer)))
    }
}

/// `IS` and `IS NOT`, which are never NULL.
pub struct IsTest {
    /// Whether `NOT` was written.
    pub negated: bool,
    /// The left operand.
    pub left: Box<dyn Eval>,
    /// The right operand.
    pub right: Box<dyn Eval>,
    /// The affinity applied before comparing.
    pub affinity: Option<Affinity>,
    /// The collation the comparison uses.
    pub collation: Collation,
}

impl Eval for IsTest {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let left = self.left.value(batch, nth)?;
        let right = self.right.value(batch, nth)?;
        let answer = eval::is_comparison(
            self.negated,
            &Value::from(&left.get()).into_owned()?,
            &Value::from(&right.get()).into_owned()?,
            self.affinity,
            self.collation,
            ENCODING,
        );
        Ok(Computed::Owned(OwnedDatum::from(answer)))
    }
}

/// `BETWEEN`, kept as one node so its operand is evaluated once.
pub struct Between {
    /// Whether `NOT` was written.
    pub negated: bool,
    /// The value being tested.
    pub operand: Box<dyn Eval>,
    /// The lower bound.
    pub low: Box<dyn Eval>,
    /// The upper bound.
    pub high: Box<dyn Eval>,
    /// The affinity `operand >= low` applies.
    pub low_affinity: Option<Affinity>,
    /// The collation `operand >= low` uses.
    pub low_collation: Collation,
    /// The affinity `operand <= high` applies.
    pub high_affinity: Option<Affinity>,
    /// The collation `operand <= high` uses.
    pub high_collation: Collation,
}

impl Eval for Between {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let low = self.low.value(batch, nth)?;
        let high = self.high.value(batch, nth)?;
        let operand = Value::from(&operand.get()).into_owned()?;
        let above = eval::comparison(
            BinaryOp::GreaterEqual,
            &operand,
            &Value::from(&low.get()).into_owned()?,
            self.low_affinity,
            self.low_collation,
            ENCODING,
        );
        let below = eval::comparison(
            BinaryOp::LessEqual,
            &operand,
            &Value::from(&high.get()).into_owned()?,
            self.high_affinity,
            self.high_collation,
            ENCODING,
        );
        let inside = eval::logical_and(&above, &below);
        let answer = if self.negated {
            eval::logical_not(&inside)
        } else {
            inside
        };
        Ok(Computed::Owned(OwnedDatum::from(answer)))
    }
}

/// `IN` over a value list.
///
/// The NULL rule is the one people get wrong and the one SQLite documents:
/// `x IN (list)` is false only when `x` matches nothing *and* nothing in the
/// list is NULL; a NULL in the list turns a non-match into NULL rather than
/// into false. `NOT IN` inherits it by negation, which is why the negation is
/// applied to a three-valued answer rather than to a boolean.
pub struct InList {
    /// Whether `NOT` was written.
    pub negated: bool,
    /// The value being tested.
    pub operand: Box<dyn Eval>,
    /// The list.
    pub list: Vec<Box<dyn Eval>>,
    /// The affinity applied before comparing.
    pub affinity: Option<Affinity>,
    /// The collation the comparison uses.
    pub collation: Collation,
}

impl Eval for InList {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let operand = Value::from(&operand.get()).into_owned()?;
        if operand.is_null() {
            // NULL IN (anything) is NULL, and NULL IN () is false. The empty
            // list is the exception SQLite makes and it is worth the branch.
            if self.list.is_empty() {
                return Ok(Computed::Owned(OwnedDatum::Int(i64::from(self.negated))));
            }
            return Ok(Computed::Owned(OwnedDatum::Null));
        }
        let mut saw_null = false;
        for candidate in &self.list {
            let candidate = candidate.value(batch, nth)?;
            let candidate = Value::from(&candidate.get()).into_owned()?;
            if candidate.is_null() {
                saw_null = true;
                continue;
            }
            let equal = eval::comparison(
                BinaryOp::Equal,
                &operand,
                &candidate,
                self.affinity,
                self.collation,
                ENCODING,
            );
            if eval::truth(&equal) == compare::Truth::True {
                return Ok(Computed::Owned(OwnedDatum::Int(i64::from(!self.negated))));
            }
        }
        if saw_null {
            return Ok(Computed::Owned(OwnedDatum::Null));
        }
        Ok(Computed::Owned(OwnedDatum::Int(i64::from(self.negated))))
    }
}

/// `CASE`, in both its forms.
pub struct Case {
    /// The base operand, when the form has one.
    pub operand: Option<Box<dyn Eval>>,
    /// The `WHEN`/`THEN` pairs.
    pub branches: Vec<(Box<dyn Eval>, Box<dyn Eval>)>,
    /// The `ELSE` arm.
    pub otherwise: Option<Box<dyn Eval>>,
    /// The affinity and collation each `WHEN` comparison uses in the base
    /// form, one per branch, and empty in the searched form.
    pub comparisons: Vec<(Option<Affinity>, Collation)>,
}

impl Eval for Case {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let base = match &self.operand {
            Some(operand) => Some(Value::from(&operand.value(batch, nth)?.get()).into_owned()?),
            None => None,
        };
        for (branch, (when, then)) in self.branches.iter().enumerate() {
            let candidate = when.value(batch, nth)?;
            let matched = match &base {
                // `CASE x WHEN y` compares; `CASE WHEN p` tests a predicate.
                Some(base) => {
                    let (affinity, collation) = self
                        .comparisons
                        .get(branch)
                        .copied()
                        .unwrap_or((None, Collation::Binary));
                    let equal = eval::comparison(
                        BinaryOp::Equal,
                        base,
                        &Value::from(&candidate.get()).into_owned()?,
                        affinity,
                        collation,
                        ENCODING,
                    );
                    eval::truth(&equal) == compare::Truth::True
                }
                None => {
                    eval::truth(&Value::from(&candidate.get()).into_owned()?)
                        == compare::Truth::True
                }
            };
            if matched {
                return then.value(batch, nth);
            }
        }
        match &self.otherwise {
            Some(otherwise) => otherwise.value(batch, nth),
            None => Ok(Computed::Borrowed(Datum::Null)),
        }
    }
}

/// Which pattern operator a [`Pattern`] node applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternKind {
    /// `LIKE`, case-insensitive for ASCII.
    Like,
    /// `GLOB`, case-sensitive with shell wildcards.
    Glob,
}

/// `LIKE` and `GLOB`.
pub struct Pattern {
    /// Whether `NOT` was written.
    pub negated: bool,
    /// Which operator.
    pub kind: PatternKind,
    /// The value being matched.
    pub operand: Box<dyn Eval>,
    /// The pattern.
    pub pattern: Box<dyn Eval>,
    /// The `ESCAPE` argument, for `LIKE`.
    pub escape: Option<Box<dyn Eval>>,
    /// Whether `LIKE` compares ASCII letters exactly.
    ///
    /// `PRAGMA case_sensitive_like`, read from the catalog when the expression
    /// was translated. It is a compile-time property because the pragma empties
    /// the statement cache when it changes, exactly as `foreign_keys` does -
    /// so a statement compiled under one setting is never run under the other.
    pub case_sensitive: bool,
}

impl Eval for Pattern {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let pattern = self.pattern.value(batch, nth)?;
        // SQLite measures the pattern before it looks at the subject, so a
        // pattern over the limit is an error even when the subject is NULL.
        if !pattern.is_null() {
            // A text pattern is measured where it is; only a number or a blob
            // has to be turned into text first, and those are rare.
            let measured = match pattern.get() {
                Datum::Text(text) => text.len(),
                other => eval::text_bytes(&Value::from(&other).into_owned()?, ENCODING).len(),
            };
            if measured > inillucent_scalar::builtin::LIKE_PATTERN_LENGTH_LIMIT {
                return Err(inillucent_base::error::statement_refusal(
                    inillucent_scalar::builtin::LIKE_PATTERN_TOO_COMPLEX,
                ));
            }
        }
        if operand.is_null() || pattern.is_null() {
            return Ok(Computed::Borrowed(Datum::Null));
        }
        // **The escape is one character, and one that is not is refused**
        // (task-2066 section 4.2, item 27). This took the first byte of
        // whatever was written, so `ESCAPE ''` silently meant "no escape" -
        // `'a%b' LIKE 'a\%b' ESCAPE ''` answered where SQLite raises - and a
        // escape character of more than one byte escaped on its first byte.
        let escape = match &self.escape {
            Some(expression) => {
                let value = expression.value(batch, nth)?;
                if value.is_null() {
                    return Ok(Computed::Borrowed(Datum::Null));
                }
                let bytes = eval::text_bytes(&Value::from(&value.get()).into_owned()?, ENCODING);
                inillucent_scalar::builtin::single_character_escape(&bytes)
                    // `SQLITE_ERROR` (1), SQLite's code for this refusal, with
                    // its sentence as the message a caller reads.
                    .map_err(inillucent_base::error::statement_refusal)?;
                Some(bytes)
            }
            None => None,
        };
        // **Text against text needs no copy, and the common shapes no matcher
        // (task-2175).** Copying both sides into owned values for every row was
        // nearly half of `label LIKE '%9 lorem%'` over 100,000 rows, and the
        // general matcher most of the rest. See `like_shape`.
        if let (PatternKind::Like, None, Datum::Text(subject), Datum::Text(written)) =
            (self.kind, &escape, operand.get(), pattern.get())
        {
            if let Some(matched) = like_shape(written, subject, !self.case_sensitive) {
                return Ok(Computed::Borrowed(Datum::Int(i64::from(
                    matched != self.negated,
                ))));
            }
        }
        let subject = eval::text_bytes(&Value::from(&operand.get()).into_owned()?, ENCODING);
        let pattern_bytes = eval::text_bytes(&Value::from(&pattern.get()).into_owned()?, ENCODING);
        let matched = match self.kind {
            PatternKind::Like => pattern::like_folding(
                &pattern_bytes,
                &subject,
                escape.as_deref(),
                !self.case_sensitive,
            ),
            PatternKind::Glob => pattern::glob(&pattern_bytes, &subject),
        };
        Ok(Computed::Borrowed(Datum::Int(i64::from(
            matched != self.negated,
        ))))
    }
}

/// Answers a `LIKE` with no `ESCAPE` whose pattern is one of four common shapes, or `None`.
///
/// The shapes are a literal, `literal%`, `%literal` and `%literal%`, where the
/// literal holds no `%` and no `_`; a pattern of nothing but `%` matches any
/// text. Every other pattern, and any text holding a NUL, returns `None` and
/// goes to the general matcher.
/// Letters are compared without regard to case when `fold` is set, and only
/// ASCII letters, which is SQLite's rule without ICU: a byte of a multi byte
/// UTF-8 character is never an ASCII letter, so folding byte by byte folds
/// exactly the characters SQLite folds.
///
/// @param pattern - the pattern's text
/// @param subject - the text being matched
/// @param fold - whether ASCII letters match regardless of case
pub(crate) fn like_shape(pattern: &[u8], subject: &[u8], fold: bool) -> Option<bool> {
    // SQLite reads a pattern and a subject as C strings, so each ends at its
    // first NUL: `'a' LIKE ('a' || x'00' || 'zz')` is 1. The general matcher
    // keeps that rule, and text holding a NUL goes to it.
    if pattern.contains(&0) || subject.contains(&0) {
        return None;
    }
    if pattern.iter().all(|byte| *byte == b'%') && !pattern.is_empty() {
        return Some(true);
    }
    let leading = pattern.first() == Some(&b'%');
    let trailing = pattern.len() > 1 && pattern.last() == Some(&b'%');
    let from = usize::from(leading);
    let to = pattern.len().saturating_sub(usize::from(trailing));
    let literal = pattern.get(from..to)?;
    if literal.iter().any(|byte| *byte == b'%' || *byte == b'_') {
        return None;
    }
    let same = |left: &[u8], right: &[u8]| {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(a, b)| a == b || (fold && a.eq_ignore_ascii_case(b)))
    };
    Some(match (leading, trailing) {
        (false, false) => same(literal, subject),
        (false, true) => subject
            .get(..literal.len())
            .is_some_and(|head| same(literal, head)),
        (true, false) => subject
            .len()
            .checked_sub(literal.len())
            .and_then(|at| subject.get(at..))
            .is_some_and(|tail| same(literal, tail)),
        (true, true) => literal.is_empty() || holds_literal(subject, literal, fold),
    })
}

/// Reports whether a text holds a literal anywhere, letters compared as `like_shape` compares them.
///
/// **The literal's first byte is looked for, and only where it is found is
/// the rest compared (task-2183).** Comparing the literal at every offset was
/// most of `label LIKE '%9 lorem%'`; a byte search over the text, two bytes
/// when a letter matches either case, skips the offsets that cannot start a
/// match.
///
/// @param subject - the text being matched
/// @param literal - the literal, not empty
/// @param fold - whether ASCII letters match regardless of case
fn holds_literal(subject: &[u8], literal: &[u8], fold: bool) -> bool {
    let Some((&first, rest)) = literal.split_first() else {
        return true;
    };
    let Some(last_start) = subject.len().checked_sub(literal.len()) else {
        return false;
    };
    let (lower, upper) = match fold {
        true => (first.to_ascii_lowercase(), first.to_ascii_uppercase()),
        false => (first, first),
    };
    let same = |left: &[u8], right: &[u8]| {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(a, b)| a == b || (fold && a.eq_ignore_ascii_case(b)))
    };
    let mut at = 0usize;
    while at <= last_start {
        let Some(window) = subject.get(at..=last_start) else {
            return false;
        };
        let found = match lower == upper {
            true => window.iter().position(|byte| *byte == lower),
            false => window
                .iter()
                .position(|byte| *byte == lower || *byte == upper),
        };
        let Some(offset) = found else {
            return false;
        };
        let start = at.saturating_add(offset);
        let tail = subject.get(start.saturating_add(1)..start.saturating_add(literal.len()));
        if tail.is_some_and(|tail| same(rest, tail)) {
            return true;
        }
        at = start.saturating_add(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Vector;
    use crate::expr::{compile, Expr, StaticType};

    /// The `LIKE` shortcut answers what the general matcher answers, for every shape it takes.
    #[test]
    fn like_shapes_agree_with_the_matcher() {
        let patterns: [&[u8]; 18] = [
            b"",
            b"%",
            b"%%",
            b"abc",
            b"ABC",
            b"abc%",
            b"%abc",
            b"%abc%",
            b"%B%",
            b"a%",
            b"%c",
            b"%9 lorem%",
            b"%\xc3\xa9%",
            b"\xc3\x89t\xc3\xa9",
            b"%a_c%",
            b"a%c",
            b"_",
            b"%%a",
        ];
        let subjects: [&[u8]; 10] = [
            b"",
            b"abc",
            b"ABC",
            b"xabcx",
            b"aBc",
            b"row 19 lorem ipsum",
            b"\xc3\xa9t\xc3\xa9",
            b"\xc3\x89t\xc3\xa9",
            b"a",
            b"cab",
        ];
        for fold in [true, false] {
            for pattern in patterns {
                for subject in subjects {
                    let general = pattern::like_folding(pattern, subject, None, fold);
                    if let Some(short) = like_shape(pattern, subject, fold) {
                        assert_eq!(short, general, "{pattern:?} LIKE {subject:?}, fold {fold}");
                    }
                }
            }
        }
        assert_eq!(
            like_shape(b"%a_c%", b"abc", true),
            None,
            "an underscore goes to the matcher"
        );
        assert_eq!(
            like_shape(b"a\x00zz", b"a", true),
            None,
            "a NUL goes to the matcher"
        );
        assert_eq!(
            like_shape(b"a%", b"a\x00b", true),
            None,
            "a NUL goes to the matcher"
        );
    }

    /// Returns a one-row batch over the given values.
    fn one_row<'p>(values: &[Datum<'p>]) -> Batch<'p> {
        Batch::new(
            1,
            values.iter().map(|value| Vector::Const(*value)).collect(),
        )
    }

    /// Evaluates a node over a one-row batch.
    fn eval_one<'p>(node: &dyn Eval, values: &[Datum<'p>]) -> OwnedDatum {
        let batch = one_row(values);
        node.value(&batch, 0).unwrap().into_owned()
    }

    /// Returns a compiled column reference.
    fn column(index: usize, count: usize) -> Box<dyn Eval> {
        compile(&Expr::Column(index), &vec![StaticType::Unknown; count]).unwrap()
    }

    /// `substr` answers what SQLite answers, through the shared implementation.
    #[test]
    fn a_scalar_call_reaches_the_shared_implementation() {
        let node = ScalarCall {
            func: ScalarFunc::Substr,
            arguments: vec![column(0, 3), column(1, 3), column(2, 3)],
            collation: Collation::Binary,
            context: Context::default(),
            stream: std::sync::atomic::AtomicU64::new(0),
        };
        let answer = eval_one(
            &node,
            &[Datum::Text(b"abcdefgh"), Datum::Int(3), Datum::Int(4)],
        );
        assert_eq!(answer, OwnedDatum::Text(b"cdef".to_vec()));
        // SQLite counts from one and accepts a negative start.
        let answer = eval_one(
            &node,
            &[Datum::Text(b"abcdefgh"), Datum::Int(-3), Datum::Int(2)],
        );
        assert_eq!(answer, OwnedDatum::Text(b"fg".to_vec()));
    }

    /// `upper` builds a value that was not in the page, which is the case the
    /// borrowed-only evaluator could not express.
    #[test]
    fn a_function_can_return_a_value_the_page_never_held() {
        let node = ScalarCall {
            func: ScalarFunc::Upper,
            arguments: vec![column(0, 1)],
            collation: Collation::Binary,
            context: Context::default(),
            stream: std::sync::atomic::AtomicU64::new(0),
        };
        assert_eq!(
            eval_one(&node, &[Datum::Text(b"mixed Case")]),
            OwnedDatum::Text(b"MIXED CASE".to_vec())
        );
    }

    /// Concatenation and the bitwise operators go through the general node.
    #[test]
    fn the_general_operators_answer() {
        let concat = GeneralArith {
            // Unbounded, which is what a unit test of the operator asks for.
            length_limit: 0,
            op: BinaryOp::Concat,
            left: column(0, 2),
            right: column(1, 2),
        };
        assert_eq!(
            eval_one(&concat, &[Datum::Text(b"ab"), Datum::Int(7)]),
            OwnedDatum::Text(b"ab7".to_vec())
        );
        let and = GeneralArith {
            length_limit: 0,
            op: BinaryOp::BitAnd,
            left: column(0, 2),
            right: column(1, 2),
        };
        assert_eq!(
            eval_one(&and, &[Datum::Int(0b1100), Datum::Int(0b1010)]),
            OwnedDatum::Int(0b1000)
        );
        // NULL poisons an arithmetic operator, as it must.
        assert_eq!(
            eval_one(&concat, &[Datum::Null, Datum::Int(7)]),
            OwnedDatum::Null
        );
    }

    /// The unary operators, including the one that does nothing.
    #[test]
    fn the_unary_operators_answer() {
        for (op, input, wanted) in [
            (UnaryOp::Negate, Datum::Int(5), OwnedDatum::Int(-5)),
            (
                UnaryOp::Identity,
                Datum::Text(b"x"),
                OwnedDatum::Text(b"x".to_vec()),
            ),
            (UnaryOp::BitNot, Datum::Int(0), OwnedDatum::Int(-1)),
            (UnaryOp::Not, Datum::Int(0), OwnedDatum::Int(1)),
            (UnaryOp::Not, Datum::Null, OwnedDatum::Null),
        ] {
            let node = Unary {
                op,
                operand: column(0, 1),
            };
            assert_eq!(eval_one(&node, &[input]), wanted, "{op:?}");
        }
    }

    /// `CAST` converts through the shared rules.
    #[test]
    fn a_cast_converts() {
        let node = Cast {
            operand: column(0, 1),
            affinity: Affinity::Integer,
        };
        assert_eq!(
            eval_one(&node, &[Datum::Text(b"42abc")]),
            OwnedDatum::Int(42)
        );
        let node = Cast {
            operand: column(0, 1),
            affinity: Affinity::Text,
        };
        assert_eq!(
            eval_one(&node, &[Datum::Int(42)]),
            OwnedDatum::Text(b"42".to_vec())
        );
    }

    /// `IS` is never NULL, where `=` is.
    #[test]
    fn is_is_never_null() {
        let node = IsTest {
            negated: false,
            left: column(0, 2),
            right: column(1, 2),
            affinity: None,
            collation: Collation::Binary,
        };
        assert_eq!(
            eval_one(&node, &[Datum::Null, Datum::Null]),
            OwnedDatum::Int(1)
        );
        assert_eq!(
            eval_one(&node, &[Datum::Null, Datum::Int(1)]),
            OwnedDatum::Int(0)
        );
        let node = IsTest {
            negated: true,
            left: column(0, 2),
            right: column(1, 2),
            affinity: None,
            collation: Collation::Binary,
        };
        assert_eq!(
            eval_one(&node, &[Datum::Null, Datum::Null]),
            OwnedDatum::Int(0)
        );
    }

    /// `BETWEEN` is inclusive at both ends and NULL-poisoned.
    #[test]
    fn between_is_inclusive() {
        let node = Between {
            negated: false,
            operand: column(0, 3),
            low: column(1, 3),
            high: column(2, 3),
            low_affinity: None,
            low_collation: Collation::Binary,
            high_affinity: None,
            high_collation: Collation::Binary,
        };
        for (value, wanted) in [(4i64, 0i64), (5, 1), (7, 1), (10, 1), (11, 0)] {
            assert_eq!(
                eval_one(&node, &[Datum::Int(value), Datum::Int(5), Datum::Int(10)]),
                OwnedDatum::Int(wanted),
                "{value}"
            );
        }
        assert_eq!(
            eval_one(&node, &[Datum::Null, Datum::Int(5), Datum::Int(10)]),
            OwnedDatum::Null
        );
    }

    /// Each bound of `BETWEEN` compares with its own collation (task-2088).
    ///
    /// `'b' BETWEEN 'a' AND 'B' COLLATE NOCASE` is `'b' >= 'a'` under BINARY
    /// and `'b' <= 'B'` under NOCASE, which 3.53.4 answers 1. With one
    /// collation for both halves the answer is 0 under BINARY, where `'b' >
    /// 'B'`, and 1 under NOCASE only by accident. The swapped node shows the
    /// collations are read from their own halves: NOCASE on the lower half and
    /// BINARY on the upper makes `'b' <= 'B'` false.
    #[test]
    fn between_compares_each_bound_with_its_own_collation() {
        let text = |bytes: &'static [u8]| Datum::Text(bytes);
        let row = [text(b"b"), text(b"a"), text(b"B")];
        let node = Between {
            negated: false,
            operand: column(0, 3),
            low: column(1, 3),
            high: column(2, 3),
            low_affinity: None,
            low_collation: Collation::Binary,
            high_affinity: None,
            high_collation: Collation::NoCase,
        };
        assert_eq!(eval_one(&node, &row), OwnedDatum::Int(1));
        let swapped = Between {
            negated: false,
            operand: column(0, 3),
            low: column(1, 3),
            high: column(2, 3),
            low_affinity: None,
            low_collation: Collation::NoCase,
            high_affinity: None,
            high_collation: Collation::Binary,
        };
        assert_eq!(eval_one(&swapped, &row), OwnedDatum::Int(0));
    }

    /// `IN` follows SQLite's NULL rule, including the empty list.
    #[test]
    fn in_follows_the_null_rule() {
        let with_null = InList {
            negated: false,
            operand: column(0, 3),
            list: vec![column(1, 3), column(2, 3)],
            affinity: None,
            collation: Collation::Binary,
        };
        // A match wins even with a NULL in the list.
        assert_eq!(
            eval_one(&with_null, &[Datum::Int(1), Datum::Int(1), Datum::Null]),
            OwnedDatum::Int(1)
        );
        // No match plus a NULL in the list is NULL, not false.
        assert_eq!(
            eval_one(&with_null, &[Datum::Int(2), Datum::Int(1), Datum::Null]),
            OwnedDatum::Null
        );
        // No match and no NULL is false.
        assert_eq!(
            eval_one(&with_null, &[Datum::Int(2), Datum::Int(1), Datum::Int(3)]),
            OwnedDatum::Int(0)
        );
        // A NULL operand is NULL...
        assert_eq!(
            eval_one(&with_null, &[Datum::Null, Datum::Int(1), Datum::Int(3)]),
            OwnedDatum::Null
        );
        // ...except against the empty list, which is false.
        let empty = InList {
            negated: false,
            operand: column(0, 1),
            list: Vec::new(),
            affinity: None,
            collation: Collation::Binary,
        };
        assert_eq!(eval_one(&empty, &[Datum::Null]), OwnedDatum::Int(0));
        let empty_not = InList {
            negated: true,
            operand: column(0, 1),
            list: Vec::new(),
            affinity: None,
            collation: Collation::Binary,
        };
        assert_eq!(eval_one(&empty_not, &[Datum::Null]), OwnedDatum::Int(1));
    }

    /// `CASE` in both forms, including the missing `ELSE`.
    #[test]
    fn case_takes_the_first_matching_branch() {
        let searched = Case {
            operand: None,
            branches: vec![(column(0, 4), column(1, 4)), (column(2, 4), column(3, 4))],
            otherwise: None,
            comparisons: Vec::new(),
        };
        assert_eq!(
            eval_one(
                &searched,
                &[Datum::Int(0), Datum::Int(10), Datum::Int(1), Datum::Int(20)]
            ),
            OwnedDatum::Int(20)
        );
        assert_eq!(
            eval_one(
                &searched,
                &[Datum::Int(0), Datum::Int(10), Datum::Int(0), Datum::Int(20)]
            ),
            OwnedDatum::Null,
            "no branch and no ELSE is NULL"
        );
        let simple = Case {
            operand: Some(column(0, 3)),
            branches: vec![(column(1, 3), column(2, 3))],
            otherwise: None,
            comparisons: vec![(None, Collation::Binary)],
        };
        assert_eq!(
            eval_one(&simple, &[Datum::Int(7), Datum::Int(7), Datum::Int(99)]),
            OwnedDatum::Int(99)
        );
        assert_eq!(
            eval_one(&simple, &[Datum::Int(7), Datum::Int(8), Datum::Int(99)]),
            OwnedDatum::Null
        );
    }

    /// `LIKE` and `GLOB` reach the shared matcher, with the negation and the
    /// escape applied here.
    #[test]
    fn the_pattern_operators_match() {
        let like = Pattern {
            negated: false,
            kind: PatternKind::Like,
            operand: column(0, 2),
            pattern: column(1, 2),
            escape: None,
            case_sensitive: false,
        };
        assert_eq!(
            eval_one(&like, &[Datum::Text(b"Hello"), Datum::Text(b"h%o")]),
            OwnedDatum::Int(1),
            "LIKE is ASCII case-insensitive"
        );
        let glob = Pattern {
            negated: false,
            kind: PatternKind::Glob,
            operand: column(0, 2),
            pattern: column(1, 2),
            escape: None,
            case_sensitive: false,
        };
        assert_eq!(
            eval_one(&glob, &[Datum::Text(b"Hello"), Datum::Text(b"h*o")]),
            OwnedDatum::Int(0),
            "GLOB is case-sensitive"
        );
        let not_like = Pattern {
            negated: true,
            kind: PatternKind::Like,
            operand: column(0, 2),
            pattern: column(1, 2),
            escape: None,
            case_sensitive: false,
        };
        assert_eq!(
            eval_one(&not_like, &[Datum::Text(b"Hello"), Datum::Text(b"h%o")]),
            OwnedDatum::Int(0)
        );
        assert_eq!(
            eval_one(&like, &[Datum::Null, Datum::Text(b"x")]),
            OwnedDatum::Null
        );
    }

    /// The value bridge round-trips every class.
    ///
    /// The bridge is `inillucent-tree`'s pair of `From` impls since task-1961's
    /// A6; this executor had its own copy of it until then. The exhaustive
    /// round trip, including text that is not valid UTF-8, is
    /// `crates/inillucent-tree/tests/value_round_trip.rs`. What this asserts
    /// is that the executor reaches that one bridge and gets its answer back.
    #[test]
    fn the_value_bridge_round_trips() {
        let cases = [
            Datum::Null,
            Datum::Int(-7),
            Datum::Real(1.5),
            Datum::Text(b"text"),
            Datum::Blob(b"\x00\xFFbytes"),
        ];
        for datum in cases {
            let owned = Value::from(&datum).into_owned().unwrap();
            let round = OwnedDatum::from(owned);
            assert_eq!(round, OwnedDatum::from_datum(&datum), "{datum:?}");
        }
    }

    /// A math function reaches the shared implementation.
    #[test]
    fn a_math_call_answers() {
        let node = MathCall {
            func: MathFunc::Sqrt,
            arguments: vec![column(0, 1)],
        };
        assert_eq!(eval_one(&node, &[Datum::Int(16)]), OwnedDatum::Real(4.0));
        assert_eq!(eval_one(&node, &[Datum::Null]), OwnedDatum::Null);
    }

    /// A time function reaches the shared implementation, at a fixed instant.
    #[test]
    fn a_time_call_answers_at_a_fixed_instant() {
        let node = TimeCall {
            func: TimeFunc::Date,
            arguments: vec![column(0, 1)],
            now: 2_451_545.0,
        };
        assert_eq!(
            eval_one(&node, &[Datum::Text(b"2024-03-04 05:06:07")]),
            OwnedDatum::Text(b"2024-03-04".to_vec())
        );
    }
}
