//! What a query reads of one FROM term, which decides whether an index covers it.
//!
//! Invariant: **a `ColumnUse` that cannot list every read says so with `opaque`, and an opaque
//! answer is never covered.** The planner reads an index instead of the table only when every
//! slot the query needs is in the index, so a read missed here is a column read from an index
//! that does not hold it.

use super::BoundExpr;

/// Which of one FROM term's columns a query reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnUse {
    /// The record slots read, ascending and without duplicates.
    pub columns: Vec<u16>,
    /// Whether the term's rowid is read.
    pub rowid: bool,
    /// Whether something was met whose column reads cannot be enumerated.
    ///
    /// An opaque use is never coverable. It is set rather than ignored because
    /// the whole value of this answer is that it is complete: a covering path
    /// that turned out not to cover a column would read it from an index that
    /// does not hold it.
    pub opaque: bool,
    /// The module's auxiliary functions this term is asked for, in the order
    /// they were met, as a folded name and the arguments after the table.
    ///
    /// `score(t)` and `bm25(t)` read the *cursor* rather than a column, so they
    /// are neither a column read nor an opaque one: the module can answer them
    /// per row, and a materialised virtual scan carries the answers beside the
    /// columns. Recorded here because this is already the answer to "what does
    /// this term have to produce", and a second list would be a second thing
    /// that can disagree with it.
    /// **The arguments, not their count.** `highlight(t, 0, '[', ']')` and
    /// `bm25(t, 10.0, 1.0)` are answered by the module from the cursor, and the
    /// module cannot answer either without the values - which used to be
    /// dropped here and replaced with an empty list at the call, so every
    /// auxiliary function saw no arguments at all. Two calls of one name with
    /// different arguments are also two different answers, so the arguments are
    /// part of what identifies a slot rather than a detail hanging off one.
    pub functions: Vec<(Vec<u8>, Vec<BoundExpr>)>,
    /// The record slots read by anything other than the block's `WHERE` clause.
    ///
    /// A partial index whose predicate the `WHERE` repeats holds that predicate true for every
    /// entry, so a column the `WHERE` reads only for that test does not have to be in the index
    /// for the index to cover the query. This is the part of `columns` that is read for other
    /// reasons.
    pub outside_filter: Vec<u16>,
}

impl ColumnUse {
    /// Records that one slot is read.
    pub fn add(&mut self, slot: u16) {
        if let Err(position) = self.columns.binary_search(&slot) {
            self.columns.insert(position, slot);
        }
    }

    /// Records that one of the module's auxiliary functions is read.
    ///
    /// @param name - the function's folded name
    /// @param arguments - the arguments after the table
    pub fn add_function(&mut self, name: &[u8], arguments: &[BoundExpr]) {
        let held = (name.to_vec(), arguments.to_vec());
        if !self.functions.contains(&held) {
            self.functions.push(held);
        }
    }

    /// Folds another use into this one.
    pub fn merge(&mut self, other: &ColumnUse) {
        for slot in &other.columns {
            self.add(*slot);
        }
        self.rowid |= other.rowid;
        self.opaque |= other.opaque;
        for slot in &other.outside_filter {
            if let Err(position) = self.outside_filter.binary_search(slot) {
                self.outside_filter.insert(position, *slot);
            }
        }
        for (name, arguments) in &other.functions {
            self.add_function(name, arguments);
        }
    }
}

impl super::BoundSelect {
    /// Adds what a `WHERE` reads of one FROM term, leaving out the term's own
    /// table function arguments.
    ///
    /// **An argument is the function's input, not a column it hands back.**
    /// `json_each(j.doc)` is bound as `json = j.doc` on the hidden `json`
    /// column, and the module applies it. Counting it as a read made the scan
    /// ask the cursor for `json` on every row, which is the whole document:
    /// a 20,000 element array was copied 20,000 times, and `SELECT count(*)
    /// FROM j, json_each(j.doc)` took seven seconds where SQLite takes a tenth.
    /// An argument the module does not apply is offered back as a recheck, and
    /// the scan reads the column for that.
    ///
    /// @param filter - the block's `WHERE`
    /// @param source - the statement-wide number of the FROM term
    /// @param into - the reads found so far
    pub(super) fn filter_columns_read(
        &self,
        filter: &BoundExpr,
        source: usize,
        into: &mut ColumnUse,
    ) {
        let table = self
            .sources
            .iter()
            .find(|term| term.id == source)
            .map(|term| term.table.as_ref())
            .filter(|table| table.kind == crate::catalog_view::TableKind::Virtual);
        let Some(table) = table else {
            filter.columns_read(source, into);
            return;
        };
        let mut conjuncts = Vec::new();
        crate::plan::split_conjunction(filter, &mut conjuncts);
        for conjunct in &conjuncts {
            let argument = match conjunct {
                BoundExpr::Compare {
                    op: crate::ast::BinaryOp::Equal,
                    left,
                    ..
                } => match left.as_ref() {
                    BoundExpr::Column {
                        source: owner,
                        column,
                        ..
                    } => {
                        *owner == source
                            && table
                                .columns
                                .get(usize::from(*column))
                                .is_some_and(|declared| declared.hidden)
                    }
                    _ => false,
                },
                _ => false,
            };
            if argument {
                // The value side may still read another term, which is that
                // term's read, not this one's.
                if let BoundExpr::Compare { right, .. } = conjunct {
                    right.columns_read(source, into);
                }
                continue;
            }
            conjunct.columns_read(source, into);
        }
    }
}
