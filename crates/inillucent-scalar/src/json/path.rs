//! JSON path parsing, lookup, and the four editing operations.
//!
//! Invariant: a lookup that finds nothing is not an error, and a path that does not
//! start with `$` is. `json_extract(x, '$.missing')` is NULL and
//! `json_extract(x, 'missing')` fails with "bad JSON path"; the two look alike
//! from a distance and applications rely on the difference, because the first
//! is a question about the data and the second is a bug in the query.
//!
//! **A malformed step is an error only when the walk reaches it.** SQLite reads a
//! path one step at a time while it walks the document, so
//! `json_extract('{"a":1}', '$.a[')` is NULL: `a` is a number, there is no array to
//! index, and the broken `[` is never read. `json_extract('{"a":[1]}', '$.a[')` is an
//! error. A step written `.` is checked whatever the document holds, and a step
//! written `[` is checked only against an array, which [`Step::Malformed`] records.
//!
//! The editing operations differ only in what they do when the path already
//! exists and when it does not, so they are one walk parameterised by that
//! decision rather than four walks that have to agree with each other.

use inillucent_base::{DbError, DbResult};

use super::node::Node;

/// One step of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// `.name` or `."name"`.
    Key(String),
    /// `[n]`, counting from the start.
    Index(usize),
    /// `[#-n]`, counting back from the end. `[#-1]` is the last element.
    FromEnd(usize),
    /// `[#]`, which names the position one past the end.
    Append,
    /// A step that does not parse, which ends the path.
    ///
    /// It is an error only if the walk gets this far. `array_only` is true for a
    /// broken `[...]`, which SQLite does not read unless the element it is applied
    /// to is an array; a broken `.` or an unknown character is read wherever the
    /// walk stands.
    Malformed {
        /// Whether the step is reported only against an array.
        array_only: bool,
        /// The whole path as written, for the error.
        text: String,
    },
}

/// Parses a path, which must begin with `$`.
///
/// A step that does not parse ends the list with [`Step::Malformed`] and is not
/// reported here; see the module comment for when it is.
pub fn parse(text: &str) -> DbResult<Vec<Step>> {
    let mut characters = text.chars().peekable();
    if characters.next() != Some('$') {
        return Err(bad_path(text));
    }
    let mut steps = Vec::new();
    let malformed = |array_only: bool| Step::Malformed {
        array_only,
        text: text.to_string(),
    };
    loop {
        match characters.next() {
            None => return Ok(steps),
            Some('.') => match read_key(&mut characters) {
                Some(name) => steps.push(Step::Key(name)),
                None => {
                    steps.push(malformed(false));
                    return Ok(steps);
                }
            },
            Some('[') => match read_index(&mut characters) {
                Some(step) => steps.push(step),
                None => {
                    steps.push(malformed(true));
                    return Ok(steps);
                }
            },
            Some(_) => {
                steps.push(malformed(false));
                return Ok(steps);
            }
        }
    }
}

/// Reads the key after a `.`, either a quoted name or a run up to the next `.` or `[`.
///
/// Returns `None` for an empty key and for a quote that is never closed.
///
/// @param characters - the path, positioned just after the `.`
fn read_key(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut name = String::new();
    if characters.peek() == Some(&'"') {
        characters.next();
        loop {
            match characters.next()? {
                '"' => return Some(name),
                '\\' => name.push(characters.next()?),
                character => name.push(character),
            }
        }
    }
    while let Some(character) = characters.peek() {
        if *character == '.' || *character == '[' {
            break;
        }
        name.push(*character);
        characters.next();
    }
    (!name.is_empty()).then_some(name)
}

/// Reads the index after a `[`: a number, `#`, or `#-` and a number, then the `]`.
///
/// Returns `None` when the text is none of those.
///
/// @param characters - the path, positioned just after the `[`
fn read_index(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<Step> {
    let step = if characters.peek() == Some(&'#') {
        characters.next();
        if characters.peek() == Some(&'-') {
            characters.next();
            Step::FromEnd(read_number(characters)?)
        } else {
            Step::Append
        }
    } else {
        Step::Index(read_number(characters)?)
    };
    (characters.next() == Some(']')).then_some(step)
}

/// Reads a run of decimal digits, refusing an empty one.
fn read_number(characters: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<usize> {
    let mut value: usize = 0;
    let mut digits = 0;
    while let Some(digit) = characters
        .peek()
        .and_then(|character| character.to_digit(10))
    {
        characters.next();
        value = value.checked_mul(10)?.checked_add(digit as usize)?;
        digits += 1;
    }
    (digits > 0).then_some(value)
}

/// Returns the error a malformed path reports.
pub fn bad_path(text: &str) -> DbError {
    DbError::primary(inillucent_base::PrimaryCode::Error)
        .with_detail(format!("bad JSON path: '{text}'"))
}

/// Resolves a path against a document, returning the element it names.
///
/// `Ok(None)` is an element that is not there. An error is a malformed step the
/// walk reached; see the module comment for which those are.
///
/// @param node - the document
/// @param steps - the path
pub fn lookup<'tree>(node: &'tree Node, steps: &[Step]) -> DbResult<Option<&'tree Node>> {
    let Some((step, rest)) = steps.split_first() else {
        return Ok(Some(node));
    };
    match (step, node) {
        (Step::Malformed { array_only, text }, held) => reached_malformed(*array_only, text, held),
        (Step::Key(name), Node::Object(members)) => {
            match members.iter().find(|(label, _)| label_matches(label, name)) {
                Some((_, value)) => lookup(value, rest),
                None => Ok(None),
            }
        }
        (Step::Index(index), Node::Array(items)) => match items.get(*index) {
            Some(item) => lookup(item, rest),
            None => Ok(None),
        },
        (Step::FromEnd(back), Node::Array(items)) => {
            match items
                .len()
                .checked_sub(*back)
                .and_then(|index| items.get(index))
            {
                Some(item) => lookup(item, rest),
                None => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Decides what a malformed step means for the element the walk is standing on.
///
/// @param array_only - whether the step is a broken `[...]`
/// @param text - the path as written
/// @param held - the element the walk reached
fn reached_malformed<T>(array_only: bool, text: &str, held: &Node) -> DbResult<Option<T>> {
    match !array_only || matches!(held, Node::Array(_)) {
        true => Err(bad_path(text)),
        false => Ok(None),
    }
}

/// Returns whether an object label spells a path key.
pub(super) fn label_matches(label: &Node, name: &str) -> bool {
    // A label with no escape is its own spelling, so it is compared as it is.
    // Unescaping it first copied it twice for every member a lookup passed.
    match label {
        Node::Text(text) | Node::TextRaw(text) => text == name,
        _ => String::from_utf8_lossy(&super::render::unescape_bytes(label)) == name,
    }
}

/// What an edit does when the path is already there, and when it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// `json_insert`: write only where nothing is.
    Insert,
    /// `json_replace`: write only where something is.
    Replace,
    /// `json_set`: write either way.
    Set,
}

impl Edit {
    /// Returns whether the operation writes over an existing element.
    fn overwrites(self) -> bool {
        matches!(self, Edit::Replace | Edit::Set)
    }

    /// Returns whether the operation creates a missing element.
    fn creates(self) -> bool {
        matches!(self, Edit::Insert | Edit::Set)
    }
}

/// Applies one edit to a document in place.
///
/// A path that does not apply - `$[0]` into an object, `$.a.b` where `a` is a
/// number - changes nothing and is not an error, which is what makes
/// `json_set` safe to run over a column whose rows are not all the same shape.
pub fn apply(node: &mut Node, steps: &[Step], value: Node, edit: Edit) -> DbResult<()> {
    let Some((step, rest)) = steps.split_first() else {
        if edit.overwrites() {
            *node = value;
        }
        return Ok(());
    };
    match (step, node) {
        (Step::Malformed { array_only, text }, held) => {
            reached_malformed::<()>(*array_only, text, held).map(|_| ())
        }
        (Step::Key(name), Node::Object(members)) => {
            if let Some(position) = members
                .iter()
                .position(|(label, _)| label_matches(label, name))
            {
                let Some((_, existing)) = members.get_mut(position) else {
                    return Ok(());
                };
                return apply(existing, rest, value, edit);
            }
            if !edit.creates() {
                return Ok(());
            }
            let Some(mut fresh) = seed(rest) else {
                return Ok(());
            };
            apply(&mut fresh, rest, value, Edit::Set)?;
            members.push((Node::text_raw(name), fresh));
            Ok(())
        }
        (Step::Index(index), Node::Array(items)) => {
            if let Some(existing) = items.get_mut(*index) {
                return apply(existing, rest, value, edit);
            }
            if !edit.creates() || *index != items.len() {
                return Ok(());
            }
            let Some(mut fresh) = seed(rest) else {
                return Ok(());
            };
            apply(&mut fresh, rest, value, Edit::Set)?;
            items.push(fresh);
            Ok(())
        }
        (Step::FromEnd(back), Node::Array(items)) => {
            let Some(index) = items.len().checked_sub(*back) else {
                return Ok(());
            };
            let Some(existing) = items.get_mut(index) else {
                return Ok(());
            };
            apply(existing, rest, value, edit)
        }
        (Step::Append, Node::Array(items)) => {
            if !edit.creates() {
                return Ok(());
            }
            let Some(mut fresh) = seed(rest) else {
                return Ok(());
            };
            apply(&mut fresh, rest, value, Edit::Set)?;
            items.push(fresh);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Inserts a value into an array, shifting whatever follows it along.
///
/// `json_array_insert(X, P, V)`, where the path's last step names a *position*
/// in an array rather than an element to overwrite. The distinction is the
/// whole function: `json_set('[1,2]','$[0]',9)` answers `[9,2]` and this
/// answers `[9,1,2]`.
///
/// A position past the end changes nothing, `[#]` appends, and a path whose
/// last step is a key is a **refusal** rather than a no-op - because a caller
/// who wrote `$.b` asked to insert into something that is not an array, and
/// answering the document back would look like it had worked.
///
/// @param node - the document, edited in place
/// @param steps - the path
/// @param value - what to insert
/// @param written - the path as it was written, for the refusal
pub fn insert_into_array(
    node: &mut Node,
    steps: &[Step],
    value: Node,
    written: &str,
) -> DbResult<()> {
    let Some((last, init)) = steps.split_last() else {
        return Ok(());
    };
    if matches!(last, Step::Malformed { .. }) {
        lookup(node, steps)?;
        return Ok(());
    }
    if matches!(last, Step::Key(_)) {
        return Err(inillucent_base::error::misuse(format!(
            "not an array element: '{written}'"
        )));
    }
    let Some(container) = follow(node, init) else {
        return Ok(());
    };
    let Node::Array(items) = container else {
        return Ok(());
    };
    let at = match last {
        Step::Index(index) => *index,
        Step::Append => items.len(),
        Step::FromEnd(back) => match items.len().checked_sub(*back) {
            Some(index) => index,
            None => return Ok(()),
        },
        Step::Key(_) | Step::Malformed { .. } => return Ok(()),
    };
    if at > items.len() {
        return Ok(());
    }
    items.insert(at, value);
    Ok(())
}

/// Returns the node a path names, when the whole path resolves.
///
/// @param node - the document
/// @param steps - the path
fn follow<'n>(node: &'n mut Node, steps: &[Step]) -> Option<&'n mut Node> {
    let Some((step, rest)) = steps.split_first() else {
        return Some(node);
    };
    let next = match (step, node) {
        (Step::Key(name), Node::Object(members)) => members
            .iter_mut()
            .find(|(label, _)| label_matches(label, name))
            .map(|(_, held)| held)?,
        (Step::Index(index), Node::Array(items)) => items.get_mut(*index)?,
        (Step::FromEnd(back), Node::Array(items)) => {
            let index = items.len().checked_sub(*back)?;
            items.get_mut(index)?
        }
        _ => return None,
    };
    follow(next, rest)
}

/// Returns the empty container a missing intermediate step needs.
///
/// The next step decides: a key needs an object to live in and an index needs
/// an array. With no next step there is nothing to create - the value itself
/// is about to be written - so the seed is a placeholder the caller
/// immediately overwrites.
fn seed(rest: &[Step]) -> Option<Node> {
    match rest.first() {
        None => Some(Node::Null),
        Some(Step::Key(_)) => Some(Node::Object(Vec::new())),
        Some(Step::Index(0) | Step::Append) => Some(Node::Array(Vec::new())),
        // A broken step is read against whatever the new element is, and SQLite
        // makes an array for a `[` and an object for anything else.
        Some(Step::Malformed { array_only, .. }) => Some(match array_only {
            true => Node::Array(Vec::new()),
            false => Node::Object(Vec::new()),
        }),
        // `$.a[3]` into a document with no `a` would have to create an array
        // with three holes, and JSON has no hole.
        Some(Step::Index(_) | Step::FromEnd(_)) => None,
    }
}

/// Removes the element a path names, reporting whether one was there.
///
/// An error is a malformed step the walk reached, as for [`lookup`].
///
/// @param node - the document, edited in place
/// @param steps - the path
pub fn remove(node: &mut Node, steps: &[Step]) -> DbResult<bool> {
    let Some((step, rest)) = steps.split_first() else {
        return Ok(false);
    };
    match (step, node) {
        (Step::Malformed { array_only, text }, held) => {
            reached_malformed::<()>(*array_only, text, held).map(|_| false)
        }
        (Step::Key(name), Node::Object(members)) => {
            let Some(position) = members
                .iter()
                .position(|(label, _)| label_matches(label, name))
            else {
                return Ok(false);
            };
            if rest.is_empty() {
                members.remove(position);
                return Ok(true);
            }
            match members.get_mut(position) {
                Some((_, value)) => remove(value, rest),
                None => Ok(false),
            }
        }
        (Step::Index(index), Node::Array(items)) => remove_from_array(items, Some(*index), rest),
        (Step::FromEnd(back), Node::Array(items)) => {
            remove_from_array(items, items.len().checked_sub(*back), rest)
        }
        _ => Ok(false),
    }
}

/// Removes an array element, or goes on into it when the path continues.
///
/// @param items - the array
/// @param index - the element the step names, when it is inside the array
/// @param rest - the steps after this one
fn remove_from_array(items: &mut Vec<Node>, index: Option<usize>, rest: &[Step]) -> DbResult<bool> {
    let Some(index) = index.filter(|index| *index < items.len()) else {
        return Ok(false);
    };
    if rest.is_empty() {
        items.remove(index);
        return Ok(true);
    }
    match items.get_mut(index) {
        Some(item) => remove(item, rest),
        None => Ok(false),
    }
}

/// Applies RFC-7386 merge-patch semantics.
///
/// The rule is short and its consequences are not: a member whose patch value
/// is `null` is deleted rather than set to null, two objects merge recursively,
/// and anything else replaces wholesale. An array is "anything else", which is
/// why a merge patch cannot edit one element of a list.
pub fn patch(target: &mut Node, patch: &Node) {
    let Node::Object(updates) = patch else {
        *target = patch.clone();
        return;
    };
    if !matches!(target, Node::Object(_)) {
        *target = Node::Object(Vec::new());
    }
    let Node::Object(members) = target else {
        return;
    };
    for (label, value) in updates {
        let name = super::render::unescape(label);
        let position = members
            .iter()
            .position(|(existing, _)| label_matches(existing, &name));
        if matches!(value, Node::Null) {
            if let Some(position) = position {
                members.remove(position);
            }
            continue;
        }
        match position {
            Some(position) => {
                if let Some((_, existing)) = members.get_mut(position) {
                    self::patch(existing, value);
                }
            }
            None => {
                let mut fresh = Node::Object(Vec::new());
                self::patch(&mut fresh, value);
                members.push((label.clone(), fresh));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{parse as text, render};

    /// Parses a document for a test.
    fn document(source: &str) -> Node {
        text::parse(source).expect("parses").node
    }

    /// Renders a document for a test.
    fn rendered(node: &Node) -> String {
        render::to_text(node)
    }

    /// A path must start at the root, and anything else is an error.
    #[test]
    fn a_path_must_start_at_the_root() {
        assert!(parse("a").is_err());
        assert_eq!(parse("$").expect("parses"), Vec::new());
    }

    /// Keys, quoted keys, and indexes all parse.
    #[test]
    fn the_step_forms_parse() {
        assert_eq!(
            parse("$.a[0].\"b.c\"[#-1][#]").expect("parses"),
            vec![
                Step::Key("a".to_string()),
                Step::Index(0),
                Step::Key("b.c".to_string()),
                Step::FromEnd(1),
                Step::Append,
            ]
        );
    }

    /// A lookup that finds nothing answers nothing rather than failing.
    #[test]
    fn a_missing_element_is_not_an_error() {
        let node = document(r#"{"a":1}"#);
        assert!(lookup(&node, &parse("$.a.b").expect("parses"))
            .expect("looks up")
            .is_none());
        assert!(lookup(&node, &parse("$[0]").expect("parses"))
            .expect("looks up")
            .is_none());
    }

    /// A broken step is an error only when the walk reaches it.
    #[test]
    fn a_malformed_step_is_read_only_where_the_walk_gets_to_it() {
        let number = document(r#"{"a":1}"#);
        let array = document(r#"{"a":[1]}"#);
        // A broken index after a number: the walk stops at the number.
        assert!(lookup(&number, &parse("$.a[").expect("parses"))
            .expect("not reached")
            .is_none());
        // The same broken index after an array is read, and is an error.
        assert!(lookup(&array, &parse("$.a[").expect("parses")).is_err());
        // An empty key is read wherever the walk stands.
        assert!(lookup(&number, &parse("$.").expect("parses")).is_err());
        assert!(lookup(&number, &parse("$x").expect("parses")).is_err());
        // A path that does not start at the root fails before any walk.
        assert!(parse("a").is_err());
    }

    /// The three edits differ only over an existing and a missing path.
    #[test]
    fn the_edits_differ_over_presence() {
        for (edit, expected) in [
            (Edit::Insert, r#"{"a":1,"b":2}"#),
            (Edit::Replace, r#"{"a":9}"#),
            (Edit::Set, r#"{"a":9,"b":2}"#),
        ] {
            let mut node = document(r#"{"a":1}"#);
            apply(
                &mut node,
                &parse("$.a").expect("parses"),
                Node::Int("9".to_string()),
                edit,
            )
            .expect("edits");
            apply(
                &mut node,
                &parse("$.b").expect("parses"),
                Node::Int("2".to_string()),
                edit,
            )
            .expect("edits");
            assert_eq!(rendered(&node), expected, "{edit:?}");
        }
    }

    /// A missing intermediate step is created from the step that follows it.
    #[test]
    fn intermediate_containers_are_created() {
        let mut node = document("{}");
        apply(
            &mut node,
            &parse("$.a.b").expect("parses"),
            Node::Int("1".to_string()),
            Edit::Set,
        )
        .expect("edits");
        assert_eq!(rendered(&node), r#"{"a":{"b":1}}"#);

        let mut array = document("[]");
        apply(
            &mut array,
            &parse("$[0].a").expect("parses"),
            Node::Int("1".to_string()),
            Edit::Set,
        )
        .expect("edits");
        assert_eq!(rendered(&array), r#"[{"a":1}]"#);
    }

    /// Removal takes the element out and reports whether it was there.
    #[test]
    fn removal_reports_what_it_did() {
        let mut node = document("[1,2,3]");
        assert!(remove(&mut node, &parse("$[0]").expect("parses")).expect("removes"));
        assert!(remove(&mut node, &parse("$[0]").expect("parses")).expect("removes"));
        assert_eq!(rendered(&node), "[3]");
        assert!(!remove(&mut node, &parse("$.zz").expect("parses")).expect("removes"));
    }

    /// A merge patch deletes on null and merges objects recursively.
    #[test]
    fn merge_patch_follows_rfc_7386() {
        let mut node = document(r#"{"a":1,"b":2}"#);
        patch(&mut node, &document(r#"{"b":null,"c":3}"#));
        assert_eq!(rendered(&node), r#"{"a":1,"c":3}"#);

        let mut nested = document(r#"{"a":{"b":1}}"#);
        patch(&mut nested, &document(r#"{"a":{"c":2}}"#));
        assert_eq!(rendered(&nested), r#"{"a":{"b":1,"c":2}}"#);

        let mut array = document("[1,2]");
        patch(&mut array, &document(r#"{"a":1}"#));
        assert_eq!(rendered(&array), r#"{"a":1}"#);
    }
}
