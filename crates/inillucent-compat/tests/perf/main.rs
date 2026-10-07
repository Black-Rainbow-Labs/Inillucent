//! The `perf` tier of `inillucent-compat`'s integration tests.
//!
//! What the tier is for: the cost guards, so a regression fails a test rather
//! than a benchmark.
//!
//! Invariant: **this file declares one module per suite and nothing else, and
//! each module is one target in `tests/selection.toml`, run by
//! `inillucent-testrun` in a process of its own.** The tier is exclusive, so
//! the runner starts these targets alone at the end of a run, one thread each.
//! That is why a suite whose answer depends on the machine being otherwise
//! idle, such as one that measures how much processor time a process was
//! given, belongs here and not in the tier its code lives in.
//!
//! A new suite is a file in this directory, a `mod` line here, and a row
//! with `name = "perf"` and `module = "<file>"` in `tests/selection.toml`.

mod processor_share;
