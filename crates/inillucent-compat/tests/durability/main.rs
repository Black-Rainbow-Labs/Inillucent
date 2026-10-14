//! The `durability` tier of `inillucent-compat`'s integration tests.
//!
//! What the tier is for: crashes, injected faults, corruption and concurrency.
//!
//! Invariant: **this file declares one module per suite and nothing else, and
//! each module is one target in `tests/selection.toml`, run by
//! `inillucent-testrun` in a process of its own.** The suites used to be one
//! integration test binary each, 149 of them, and every one linked the same
//! compat library and 23 crates. One binary per tier links them once. The runner
//! lists this binary's tests, groups them by module, and starts the binary once
//! per module with `--exact` and that module's names, so a suite still has its
//! own process, its own timing row and its own kill budget.
//!
//! A new suite is a file in this directory, a `mod` line here, and a row
//! with `name = "durability"` and `module = "<file>"` in
//! `tests/selection.toml`. `selection::discover` finds a module file with no
//! row, so `no_test_hides_outside_the_map` names one that was forgotten.

mod auto_vacuum_crash;
mod btree_model;
mod bulk_build_crash;
mod bulk_insert_crash;
mod busy_timeout;
mod concurrency;
mod corruption;
mod durability;
mod encrypted_crash;
mod environment;
mod fault_shapes;
mod fault_sweep;
mod faults;
mod file_damage;
mod fold_protocol;
mod free_map_checkpoint_crash;
mod large_transactions;
mod log_oplock;
mod log_roll_between_connections;
mod logical_split_crash;
mod long_key_crash;
mod multi_database_crash;
mod new_engine_free_map_recovery;
mod new_engine_recovery_shapes;
mod overflow_crash;
mod phase2_campaigns;
mod process_campaign;
mod process_concurrency;
mod process_crash;
mod process_create;
mod process_deferred;
mod process_interleavings;
mod process_journal_handoff;
mod process_maintenance;
mod process_readers;
mod process_schema_change;
mod process_storm;
mod read_only_connection;
mod recovery_discard_runs_once;
mod reindex_crash;
mod restored_copy_under_newer_log;
mod reused_key_extent;
mod search_crash;
mod storage_failure;
mod story_nikaya_crash;
mod two_connections_end_a_transaction;
mod vacuum_crash;
mod wal_crash;
