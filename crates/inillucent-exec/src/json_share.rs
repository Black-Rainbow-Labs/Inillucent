//! Parsed JSON documents kept across the calls of one statement.
//!
//! Invariant: **a document that several JSON calls read with different paths is parsed once for
//! all of them, and a document that one call reads is parsed the way it always was.** SQLite keeps
//! a small cache of parsed documents that every call of a statement shares, so five
//! `json_extract(j, ...)` over the same text parse it once. A call here kept its own cache, so
//! five calls parsed five times: 2,000 documents of 18 KB took 237 ms for five `json_extract`
//! calls and 861 ms for five `->>` calls, against 27 ms in SQLite.
//!
//! The calls do not know about each other when the statement is compiled, so the paths are
//! learned as the calls run. Each call tells this module its path, the module remembers the paths
//! it was told about in the last [`WINDOW`] calls, and the first call that meets a new document
//! parses it for all of those paths at once and keeps the tree. The next calls on the same
//! document find their path in it. While only one path is in use the module says nothing and the
//! caller parses as before, so a statement with one call pays for nothing here.

use std::cell::RefCell;

use inillucent_base::DbResult;
use inillucent_scalar::json::path::Step;
use inillucent_scalar::json::{self, Answer, Node};

/// How many calls a path may go unused before it stops being part of the union.
///
/// A row of five calls tells the module every path once in five calls, so a path in use is never
/// near this. A path left over from a statement that finished is gone within one window of the
/// next statement's calls.
const WINDOW: u64 = 64;

/// The most paths the union holds. The oldest is dropped for a new one.
const MAX_PATHS: usize = 8;

/// The most documents kept, which is SQLite's own cache size.
const MAX_DOCUMENTS: usize = 4;

/// The longest document kept. A longer one is parsed by the caller and dropped.
const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// One document kept, with the paths its tree was built for.
struct Held {
    /// The text of the document.
    bytes: Vec<u8>,
    /// The tree, which holds what the paths in `covers` read and may hold nothing else.
    node: Node,
    /// The paths the tree answers.
    covers: Vec<Vec<Step>>,
}

/// The paths in use and the documents kept for them.
#[derive(Default)]
struct Share {
    /// How many calls have been made on this thread.
    tick: u64,
    /// Each path in use and the call that last used it.
    paths: Vec<(Vec<Step>, u64)>,
    /// The documents kept, the most recently used first.
    documents: Vec<Held>,
}

thread_local! {
    /// What this thread's JSON calls share. Per thread because a statement runs on one.
    static SHARE: RefCell<Share> = RefCell::new(Share::default());
}

impl Share {
    /// Records that a call is about to read a path, and drops the paths nobody has used lately.
    ///
    /// @param steps - the path the call reads
    fn note(&mut self, steps: &[Step]) {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        match self
            .paths
            .iter_mut()
            .find(|(held, _)| held.as_slice() == steps)
        {
            Some((_, last)) => *last = tick,
            None => {
                self.paths
                    .retain(|(_, last)| tick.saturating_sub(*last) <= WINDOW);
                if self.paths.len() >= MAX_PATHS {
                    self.paths.remove(0);
                }
                self.paths.push((steps.to_vec(), tick));
            }
        }
    }
}

/// Answers a JSON call from a document shared with the other calls, or says it should not.
///
/// `Ok(None)` means only one path is in use, the document is not text that is valid UTF-8, or it
/// is too long to keep, and the caller reads the document itself.
///
/// @param document - the document's text
/// @param steps - the path this call reads
/// @param finish - answers the call from the parsed document
pub(crate) fn answer_shared(
    document: &[u8],
    steps: &[Step],
    finish: impl FnOnce(&Node) -> DbResult<Answer>,
) -> DbResult<Option<Answer>> {
    SHARE.with(|cell| {
        let Ok(mut share) = cell.try_borrow_mut() else {
            return Ok(None);
        };
        share.note(steps);
        if share.paths.len() < 2 || document.len() > MAX_DOCUMENT_BYTES {
            return Ok(None);
        }
        let hit = share
            .documents
            .iter()
            .position(|held| held.bytes == document && held.covers.iter().any(|c| c == steps));
        if let Some(at) = hit {
            let held = share.documents.remove(at);
            share.documents.insert(0, held);
            return match share.documents.first() {
                Some(held) => finish(&held.node).map(Some),
                None => Ok(None),
            };
        }
        let Ok(text) = std::str::from_utf8(document) else {
            return Ok(None);
        };
        let covers: Vec<Vec<Step>> = share.paths.iter().map(|(path, _)| path.clone()).collect();
        let union: Vec<&[Step]> = covers.iter().map(Vec::as_slice).collect();
        let node = json::parse::parse_pruned_set(text, &union)
            .map_err(|_| json::malformed())?
            .node;
        let answer = finish(&node);
        share.documents.retain(|held| held.bytes != document);
        share.documents.insert(
            0,
            Held {
                bytes: document.to_vec(),
                node,
                covers,
            },
        );
        share.documents.truncate(MAX_DOCUMENTS);
        answer.map(Some)
    })
}
