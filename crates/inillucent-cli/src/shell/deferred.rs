//! Runs of single row `INSERT` statements the shell holds and runs as one.
//!
//! Invariant: **what the shell prints and what the database holds are what
//! running each statement as it arrived would have given.** A script of ten
//! thousand `INSERT INTO t VALUES (...)` lines inside `BEGIN` and `COMMIT` ran
//! ten thousand statements, each paying for a statement's setup and its
//! share of the tree's room making, where one statement over all the rows pays
//! once and an empty table takes the bulk build (task-2191).
//!
//! A statement is held when its literals lift out to the same text as the one
//! before it, `INSERT INTO t VALUES (?1, ?2)`, and the engine says that
//! statement can run many rows at once, which it says only inside a
//! transaction the script opened and only for an insert with nothing that acts
//! per statement. See the engine's `connect::Statement::run_rows_at_once`.
//!
//! The held rows run before anything that could see them or print: the next
//! statement of any other text, a dot command, the end of the input. When the
//! one statement fails it has written nothing, and every held statement is run
//! again as written, one at a time, with the line number, the excerpt and the
//! line of input it came from, so the errors are the ones the script would
//! have printed, in the same places.
//!
//! Nothing is held while an option that acts per statement is on: `.echo`,
//! `.timer`, `.stats`, `.changes`, `.eqp`, `.auth`, `.trace`, `.progress`,
//! `.scanstats`, or an output opened by `.once`.

use inillucent_tree::datum::OwnedDatum;

use super::Shell;

/// How many rows are held before they are run, so a script of millions of
/// lines does not hold all of them.
const MOST_HELD_ROWS: usize = 200_000;

/// The statements held so far.
pub(super) struct Deferred {
    /// The text every held statement lifts out to.
    template: String,
    /// One row of values for each held statement.
    rows: Vec<Vec<OwnedDatum>>,
    /// Each held statement as it arrived.
    sources: Vec<Source>,
}

/// One held statement, with what an error in it is reported against.
struct Source {
    /// The statement as written.
    sql: String,
    /// The line of input it started on.
    line: usize,
    /// The excerpt an error in it shows.
    excerpt: Option<String>,
    /// Which line of input it arrived in; see `Shell::chunk_number`.
    chunk: u64,
}

impl Shell {
    /// Holds a statement instead of running it, when it can join a run.
    ///
    /// Returns whether it was held. A statement that was not held runs as it
    /// always did, after the held ones; see [`Shell::run_held`].
    ///
    /// @param sql - the statement
    pub(super) fn hold_if_it_joins(&mut self, sql: &str) -> bool {
        if !self.holds_statements() {
            return false;
        }
        let Some((template, values)) = inillucent_driver::lifted_insert(sql) else {
            return false;
        };
        let joins = self
            .deferred
            .as_ref()
            .is_some_and(|held| held.template == template);
        if !joins {
            self.run_held();
            if self.chunk_stop || (self.failed && self.bail) || !self.runs_rows_at_once(&template) {
                return false;
            }
            self.deferred = Some(Deferred {
                template,
                rows: Vec::new(),
                sources: Vec::new(),
            });
        }
        let source = Source {
            sql: sql.to_string(),
            line: self.line,
            excerpt: self.excerpt.clone(),
            chunk: self.chunk_number,
        };
        let full = match self.deferred.as_mut() {
            Some(held) => {
                held.rows.push(values);
                held.sources.push(source);
                held.rows.len() >= MOST_HELD_ROWS
            }
            None => return false,
        };
        if full {
            self.run_held();
        }
        true
    }

    /// Reports whether nothing the shell is set to do acts per statement, so a
    /// statement may be held. See the module comment for the list.
    fn holds_statements(&self) -> bool {
        !(self.echo
            || self.timer
            || self.stats
            || self.show_changes
            || self.explain_plan
            || self.auth
            || self.readonly
            || self.output_is_once
            || self.trace.is_some()
            || self.progress_interval > 0
            || self.scanstats != "off")
            && !self.connection().autocommit().unwrap_or(true)
    }

    /// Reports whether the engine can run a statement's rows at once.
    ///
    /// @param template - the statement with its literals lifted out
    fn runs_rows_at_once(&self, template: &str) -> bool {
        self.connection()
            .prepare(template)
            .and_then(|mut statement| statement.can_run_rows_at_once())
            .unwrap_or(false)
    }

    /// Returns how many statements are held, for the tests.
    #[cfg(test)]
    pub(super) fn held_count(&self) -> usize {
        self.deferred.as_ref().map_or(0, |held| held.rows.len())
    }

    /// Runs the held statements, as one statement when that works and one at a
    /// time when it does not.
    pub(crate) fn run_held(&mut self) {
        let Some(held) = self.deferred.take() else {
            return;
        };
        let ran = self
            .connection()
            .prepare(&held.template)
            .and_then(|mut statement| statement.run_rows_at_once(held.rows));
        if let Ok(Some(_)) = ran {
            return;
        }
        self.run_one_at_a_time(held.sources);
    }

    /// Runs held statements again as written, each with the line, excerpt and
    /// line of input it arrived with, printing what a line of input prints
    /// when the line it belonged to has already ended.
    ///
    /// @param sources - the held statements, in the order they arrived
    fn run_one_at_a_time(&mut self, sources: Vec<Source>) {
        let line = self.line;
        let excerpt = self.excerpt.take();
        let holding = self.holding;
        let held_before = self.held_error.take();
        let mut sources = sources.into_iter().peekable();
        let mut skipping: Option<u64> = None;
        while let Some(source) = sources.next() {
            let current = holding && source.chunk == self.chunk_number;
            if skipping != Some(source.chunk) {
                self.line = source.line;
                self.excerpt = source.excerpt;
                self.holding = true;
                self.run_as_written(&source.sql);
                if self.chunk_stop {
                    skipping = Some(source.chunk);
                }
            }
            let chunk_ends = sources.peek().is_none_or(|next| next.chunk != source.chunk);
            if chunk_ends && !current {
                // That line of input has ended, so what it held is printed now,
                // which is where its end would have printed it.
                if let Some(lines) = self.held_error.take() {
                    for line in lines {
                        self.complain(&line);
                    }
                }
                self.chunk_stop = false;
                if self.failed && self.bail {
                    break;
                }
            }
        }
        self.line = line;
        self.excerpt = excerpt;
        self.holding = holding;
        if self.held_error.is_none() {
            self.held_error = held_before;
        }
    }
}
