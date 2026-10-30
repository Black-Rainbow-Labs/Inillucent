//! What each reduction of the query parser builds.
//!
//! Invariant: **each function here is one of the actions in `fts5parse.y` and its helpers in
//! `fts5_expr.c`**, and an error is recorded the way `sqlite3Fts5ParseError` records it, where
//! the first one wins. The functions that build nodes keep the quirks that change which rows a
//! query finds: a string with no tokens in it is dropped from a run of phrases written side by
//! side and kept next to `AND`, `OR` and `NOT`; a column filter inside another is intersected
//! with it; and a column filter left with no column turns the phrase into a node that matches
//! nothing.

use super::*;

/// Returns the columns two column lists share, in order.
///
/// @param left - the columns of one filter, sorted
/// @param right - the columns of the other, sorted
fn shared_columns(left: &[usize], right: &[usize]) -> Vec<usize> {
    left.iter()
        .copied()
        .filter(|column| right.contains(column))
        .collect()
}

/// Applies a column filter to a node and everything under it, as `fts5ParseSetColset` does.
///
/// A phrase with a filter already keeps only the columns both filters name, and becomes a node
/// that matches nothing when none is left. A phrase with none takes the filter.
///
/// @param tree - the node
/// @param colset - the columns, sorted
pub(super) fn apply_colset(tree: &mut Match, colset: &[usize]) {
    match tree {
        Match::Near { first, phrases, .. } => {
            let Some(held) = phrases.first().map(|phrase| phrase.columns.clone()) else {
                return;
            };
            let columns = if held.is_restricted() {
                shared_columns(&held.named, colset)
            } else {
                colset.to_vec()
            };
            if columns.is_empty() && held.is_restricted() {
                let phrases = std::mem::take(phrases);
                *tree = Match::Nothing {
                    first: *first,
                    phrases,
                };
                return;
            }
            for phrase in phrases.iter_mut() {
                phrase.columns = ColumnFilter::only(columns.clone());
            }
        }
        Match::Nothing { .. } => {}
        Match::And(children) | Match::Or(children) => {
            for child in children {
                apply_colset(child, colset);
            }
        }
        Match::Not(left, right) => {
            apply_colset(left, colset);
            apply_colset(right, colset);
        }
    }
}

/// Adds a phrase to a `NEAR` group, as `sqlite3Fts5ParseNearset` does.
///
/// A phrase with no tokens is not kept next to a phrase that has some: a new empty one is
/// dropped, and an empty one already held is replaced by the new phrase.
///
/// @param set - the group so far
/// @param phrase - the phrase just read
fn add_to_near_set(mut set: NearSet, phrase: Phrase) -> NearSet {
    match set.phrases.last() {
        Some(_) if phrase.terms.is_empty() => {}
        Some(last) if last.terms.is_empty() => {
            set.phrases.pop();
            set.phrases.push(phrase);
        }
        _ => set.phrases.push(phrase),
    }
    set
}

/// Returns a group holding one phrase.
///
/// @param phrase - the phrase
fn single_near_set(phrase: Phrase) -> NearSet {
    NearSet {
        phrases: vec![phrase],
        distance: DEFAULT_NEAR,
    }
}

/// Builds the leaf for a group of phrases, as `sqlite3Fts5ParseNode` does for a string node.
///
/// @param set - the group
fn leaf_of(set: NearSet) -> Node {
    let tree = if set.phrases.iter().any(|phrase| phrase.terms.is_empty()) {
        Match::Nothing {
            first: 0,
            phrases: set.phrases,
        }
    } else {
        Match::Near {
            first: 0,
            phrases: set.phrases,
            distance: set.distance,
        }
    };
    Node {
        tree,
        height: 0,
        kids: Vec::new(),
    }
}

/// Builds an `AND`, `OR` or `NOT` node, merging a child of the same kind into it.
///
/// Fails with SQLite's message when the tree would be deeper than 256.
///
/// @param operator - the operator token
/// @param left - the left operand
/// @param right - the right operand
fn binary(operator: Kind, left: Node, right: Node) -> Result<Node, String> {
    let (tree, kids) = match operator {
        Kind::Not => {
            let kids = vec![left.height, right.height];
            (Match::Not(Box::new(left.tree), Box::new(right.tree)), kids)
        }
        Kind::And => merged(left, right, true),
        _ => merged(left, right, false),
    };
    let height = kids.iter().copied().max().unwrap_or(0) + 1;
    if height > MAX_DEPTH {
        return Err(format!(
            "fts5 expression tree is too large (maximum depth {MAX_DEPTH})"
        ));
    }
    Ok(Node { tree, height, kids })
}

/// Joins two operands under one `AND` or `OR`, flattening operands of the same kind.
///
/// The children of an operand of the same kind are moved, and the left operand's list is the one
/// that grows, so a chain of ten thousand `OR`s is ten thousand pushes and not ten thousand copies
/// of a list that is getting longer.
///
/// @param left - the left operand
/// @param right - the right operand
/// @param and - whether the operator is `AND`
fn merged(left: Node, right: Node, and: bool) -> (Match, Vec<u32>) {
    let mut children = Vec::new();
    let mut kids = Vec::new();
    for node in [left, right] {
        let Node {
            tree,
            height,
            kids: inner_kids,
        } = node;
        match (tree, and) {
            (Match::And(inner), true) | (Match::Or(inner), false) => {
                if children.is_empty() {
                    children = inner;
                    kids = inner_kids;
                } else {
                    children.extend(inner);
                    kids.extend(inner_kids);
                }
            }
            (other, _) => {
                kids.push(height);
                children.push(other);
            }
        }
    }
    let tree = if and {
        Match::And(children)
    } else {
        Match::Or(children)
    };
    (tree, kids)
}

/// Joins two phrases written side by side, as `sqlite3Fts5ParseImplicitAnd` does.
///
/// A node that matches nothing and was written without an operator is dropped, so `hello 😀` is
/// `hello`. The new node replaces one the run ends in, and is dropped when it is one.
///
/// @param left - the run so far
/// @param right - the node just read
fn implicit_and(left: Node, right: Node) -> Result<Node, String> {
    if matches!(right.tree, Match::Nothing { .. }) {
        return Ok(left);
    }
    let last_is_nothing = match &left.tree {
        Match::And(children) => matches!(children.last(), Some(Match::Nothing { .. })),
        other => matches!(other, Match::Nothing { .. }),
    };
    if !last_is_nothing {
        return binary(Kind::And, left, right);
    }
    let Node {
        tree,
        height,
        mut kids,
    } = left;
    match tree {
        Match::And(mut children) => {
            children.pop();
            children.push(right.tree);
            if let Some(last) = kids.last_mut() {
                *last = right.height;
            }
            Ok(Node {
                tree: Match::And(children),
                height,
                kids,
            })
        }
        _ => Ok(right),
    }
}

/// Moves a fixed number of popped entries into an array so a rule can take them by value.
///
/// A rule used to borrow them and clone what it kept, which copied the whole of the left operand
/// of every \`AND\`, \`OR\` and \`NOT\` and made a long chain of them cost the square of its length.
///
/// @param entries - the entries a rule popped
fn unpack<const N: usize>(entries: Vec<Entry>) -> Option<[Entry; N]> {
    entries.try_into().ok()
}

/// Returns the error for a reduction that found something other than what its rule pops.
fn lost() -> String {
    "fts5: syntax error near \"\"".to_string()
}

impl Machine<'_> {
    /// Returns which column a name is, or SQLite's message when the table has no such column.
    ///
    /// @param token - the string token naming the column
    fn column_of(&self, token: &Token) -> Result<usize, String> {
        let name = dequote(&token.text);
        self.columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("no such column: {}", String::from_utf8_lossy(&name)))
    }

    /// Returns the columns not in a list.
    ///
    /// @param list - the columns, sorted
    fn inverted(&self, list: &[usize]) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|column| !list.contains(column))
            .collect()
    }

    /// Adds a column to a sorted list, leaving one already there alone.
    ///
    /// @param list - the columns so far
    /// @param column - the column to add
    fn with_column(mut list: Vec<usize>, column: usize) -> Vec<usize> {
        if let Err(at) = list.binary_search(&column) {
            list.insert(at, column);
        }
        list
    }

    /// Records the outcome of a reduction: the entry it built, or the first error.
    ///
    /// @param result - what the reduction produced
    fn finish(&mut self, result: Result<Entry, String>) {
        match result {
            Ok(entry) => self.push(entry),
            Err(said) => self.fail(said),
        }
    }

    /// Reduces the rules that build a column filter.
    ///
    /// @param rule - the rule
    pub(super) fn reduce_colset(&mut self, rule: Rule) {
        let result = match rule {
            Rule::ColsetStr => match unpack::<1>(self.pop(1)) {
                Some([Entry::Tok(name)]) => self.column_of(&name).map(|at| Entry::Colset(vec![at])),
                _ => Err(lost()),
            },
            Rule::ColsetMinusStr => match unpack::<2>(self.pop(2)) {
                Some([_, Entry::Tok(name)]) => self
                    .column_of(&name)
                    .map(|at| Entry::Colset(self.inverted(&[at]))),
                _ => Err(lost()),
            },
            Rule::ColsetListFirst => match unpack::<1>(self.pop(1)) {
                Some([Entry::Tok(name)]) => {
                    self.column_of(&name).map(|at| Entry::ColsetList(vec![at]))
                }
                _ => Err(lost()),
            },
            Rule::ColsetListMore => match unpack::<2>(self.pop(2)) {
                Some([Entry::ColsetList(list), Entry::Tok(name)]) => self
                    .column_of(&name)
                    .map(|at| Entry::ColsetList(Self::with_column(list, at))),
                _ => Err(lost()),
            },
            Rule::ColsetBraces => match unpack::<3>(self.pop(3)) {
                Some([_, Entry::ColsetList(list), _]) => Ok(Entry::Colset(list)),
                _ => Err(lost()),
            },
            _ => match unpack::<4>(self.pop(4)) {
                Some([_, _, Entry::ColsetList(list), _]) => Ok(Entry::Colset(self.inverted(&list))),
                _ => Err(lost()),
            },
        };
        self.finish(result);
    }

    /// Tokenizes the text of a string token into terms.
    ///
    /// @param token - the string token
    fn terms_of(&self, token: &Token) -> Vec<Term> {
        self.tokenizer
            .tokens(&dequote(&token.text))
            .into_iter()
            .map(|token| Term {
                token,
                prefix: false,
            })
            .collect()
    }

    /// Reduces the rules that build a phrase.
    ///
    /// @param rule - the rule
    fn reduce_word(&mut self, rule: Rule) -> Result<Entry, String> {
        match rule {
            Rule::PhraseNew => match unpack::<2>(self.pop(2)) {
                Some([Entry::Tok(word), Entry::StarOpt(star)]) => {
                    let mut terms = self.terms_of(&word);
                    if let Some(last) = terms.last_mut() {
                        last.prefix = star;
                    }
                    Ok(Entry::Phrase(plain_phrase(terms)))
                }
                _ => Err(lost()),
            },
            _ => match unpack::<4>(self.pop(4)) {
                Some([Entry::Phrase(mut phrase), _, Entry::Tok(word), Entry::StarOpt(star)]) => {
                    phrase.terms.extend(self.terms_of(&word));
                    if let Some(last) = phrase.terms.last_mut() {
                        last.prefix = star;
                    }
                    Ok(Entry::Phrase(phrase))
                }
                _ => Err(lost()),
            },
        }
    }

    /// Reduces the rules that build a phrase, a \`NEAR\` group and its distance.
    ///
    /// @param rule - the rule
    pub(super) fn reduce_phrase(&mut self, rule: Rule) {
        let result = match rule {
            Rule::PhraseNew | Rule::PhraseAppend => self.reduce_word(rule),
            Rule::NearDistComma => match unpack::<2>(self.pop(2)) {
                Some([_, Entry::Tok(number)]) => Ok(Entry::NearDist(Some(number))),
                _ => Err(lost()),
            },
            Rule::NearphrasesFirst => match unpack::<1>(self.pop(1)) {
                Some([Entry::Phrase(phrase)]) => Ok(Entry::NearPhrases(single_near_set(phrase))),
                _ => Err(lost()),
            },
            Rule::NearphrasesMore => match unpack::<2>(self.pop(2)) {
                Some([Entry::NearPhrases(set), Entry::Phrase(phrase)]) => {
                    Ok(Entry::NearPhrases(add_to_near_set(set, phrase)))
                }
                _ => Err(lost()),
            },
            Rule::NearsetPhrase => match unpack::<1>(self.pop(1)) {
                Some([Entry::Phrase(phrase)]) => Ok(Entry::Nearset(single_near_set(phrase))),
                _ => Err(lost()),
            },
            Rule::NearsetCaret => match unpack::<2>(self.pop(2)) {
                Some([_, Entry::Phrase(mut phrase)]) => {
                    phrase.anchored = !phrase.terms.is_empty();
                    Ok(Entry::Nearset(single_near_set(phrase)))
                }
                _ => Err(lost()),
            },
            _ => match unpack::<5>(self.pop(5)) {
                Some(
                    [Entry::Tok(name), _, Entry::NearPhrases(set), Entry::NearDist(distance), _],
                ) => finish_near(&name, set, distance.as_ref()).map(Entry::Nearset),
                _ => Err(lost()),
            },
        };
        self.finish(result);
    }

    /// Reduces the rules that build an expression.
    ///
    /// @param rule - the rule
    pub(super) fn reduce_expression(&mut self, rule: Rule) {
        let result = match rule {
            Rule::CNearset => match unpack::<1>(self.pop(1)) {
                Some([Entry::Nearset(set)]) => Ok(Entry::CNearset(leaf_of(set))),
                _ => Err(lost()),
            },
            Rule::CNearsetColset => match unpack::<3>(self.pop(3)) {
                Some([Entry::Colset(colset), _, Entry::Nearset(set)]) => {
                    let mut node = leaf_of(set);
                    apply_colset(&mut node.tree, &colset);
                    Ok(Entry::CNearset(node))
                }
                _ => Err(lost()),
            },
            Rule::ExprListFirst => match unpack::<1>(self.pop(1)) {
                Some([Entry::CNearset(node)]) => Ok(Entry::ExprList(node)),
                _ => Err(lost()),
            },
            Rule::ExprListMore => match unpack::<2>(self.pop(2)) {
                Some([Entry::ExprList(left), Entry::CNearset(right)]) => {
                    implicit_and(left, right).map(Entry::ExprList)
                }
                _ => Err(lost()),
            },
            Rule::ExprFromList => match unpack::<1>(self.pop(1)) {
                Some([Entry::ExprList(node)]) => Ok(Entry::Expr(node)),
                _ => Err(lost()),
            },
            Rule::ExprBinary => match unpack::<3>(self.pop(3)) {
                Some([Entry::Expr(left), Entry::Tok(operator), Entry::Expr(right)]) => {
                    binary(operator.kind, left, right).map(Entry::Expr)
                }
                _ => Err(lost()),
            },
            Rule::ExprParen => match unpack::<3>(self.pop(3)) {
                Some([_, Entry::Expr(node), _]) => Ok(Entry::Expr(node)),
                _ => Err(lost()),
            },
            _ => match unpack::<5>(self.pop(5)) {
                Some([Entry::Colset(colset), _, _, Entry::Expr(mut node), _]) => {
                    apply_colset(&mut node.tree, &colset);
                    Ok(Entry::Expr(node))
                }
                _ => Err(lost()),
            },
        };
        self.finish(result);
    }
}

/// Finishes \`NAME(phrase phrase, distance)\`: the name has to be \`NEAR\`, and the distance an integer.
///
/// @param name - the string token before the parenthesis
/// @param set - the phrases
/// @param distance - the token after the comma, when there was one
fn finish_near(
    name: &Token,
    mut set: NearSet,
    distance: Option<&Token>,
) -> Result<NearSet, String> {
    if name.text != b"NEAR" {
        let near = String::from_utf8_lossy(&name.text).into_owned();
        return Err(format!("fts5: syntax error near \"{near}\""));
    }
    set.distance = match distance {
        Some(token) => near_distance(&token.text)?,
        None => DEFAULT_NEAR,
    };
    Ok(set)
}

/// Returns a phrase with no column filter and no anchor.
///
/// @param terms - the terms
fn plain_phrase(terms: Vec<Term>) -> Phrase {
    Phrase {
        terms,
        columns: ColumnFilter::default(),
        anchored: false,
    }
}

/// Reads the distance of a `NEAR` from the token after the comma, as `SetDistance` does.
///
/// Anything that is not digits is `expected integer, got "..."`, quotes included. A distance
/// stops growing once it passes about 214 million.
///
/// @param text - the token's bytes
fn near_distance(text: &[u8]) -> Result<u32, String> {
    let mut value = 0u32;
    for byte in text {
        if !byte.is_ascii_digit() {
            return Err(format!(
                "expected integer, got \"{}\"",
                String::from_utf8_lossy(text)
            ));
        }
        if value < 214_748_363 {
            value = value * 10 + u32::from(byte - b'0');
        }
    }
    Ok(value)
}
