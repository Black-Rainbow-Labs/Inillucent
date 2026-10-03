//! The settings SQLite stores and this engine has nothing to apply them to.
//!
//! Invariant: **a setting SQLite accepts is accepted here and reads back the
//! number SQLite would read back.** An application sets `PRAGMA mmap_size`,
//! `PRAGMA wal_autocheckpoint` or `PRAGMA fullfsync` when it opens a
//! connection and does not look at the answer; a refusal here made the whole
//! connection setup fail. Each setting below is parsed with SQLite's own rules
//! for it and stored, and most of them change nothing about how this engine
//! runs. The table says which, in the comment on each row.

use inillucent_base::DbResult;
use inillucent_sql::declare::{
    argument_text, sqlite_atoi, sqlite_boolean, sqlite_int32, sqlite_integer,
};
use inillucent_sql::directive::PragmaArgument;

use super::named_integer;
use crate::Outcome;

/// How a setting's argument is read and what is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `on`, `off`, a number; stored as 0 or 1. A set prints nothing.
    Boolean,
    /// A byte count, kept between 0 and the largest SQLite allows. A negative
    /// number means the default, which is 0.
    MemoryMap,
    /// A frame count; anything not above 0 turns the automatic checkpoint off.
    WalAutocheckpoint,
    /// A byte count; below -1 is -1.
    JournalSizeLimit,
    /// A thread count between 0 and 8, set only by a whole non negative number.
    Threads,
    /// A byte count, set only by a whole non negative number.
    SoftHeapLimit,
    /// A byte count that can only be lowered once set.
    HardHeapLimit,
    /// A page count, with the boolean rule for turning spilling off.
    CacheSpill,
}

/// One remembered setting.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Remembered {
    /// The pragma's folded name, which is also its answer's column.
    pub(crate) name: &'static str,
    /// What a fresh connection reads.
    pub(crate) default: i64,
    /// How the argument is read.
    kind: Kind,
}

/// SQLite's largest memory map size, `SQLITE_MAX_MMAP_SIZE`.
const MAX_MMAP_SIZE: i64 = 0x7fff_0000;

/// SQLite's most worker threads, `SQLITE_MAX_WORKER_THREADS`.
const MAX_WORKER_THREADS: i64 = 8;

/// Every setting that is remembered and reported.
///
/// None of these changes how this engine runs, and each row says why:
///
/// - `mmap_size`: pages are read through the buffer pool and never mapped.
/// - `wal_autocheckpoint`: the log is folded in at an explicit checkpoint.
/// - `journal_size_limit`: the log is not trimmed to a size.
/// - `threads`, `soft_heap_limit`, `hard_heap_limit`: one thread and no heap
///   limit; a hard limit is stored but nothing runs out of memory because of it.
/// - `cache_spill`: the pool evicts by clock instead of at a threshold.
/// - `cell_size_check`, `fullfsync`, `checkpoint_fullfsync`,
///   `read_uncommitted`, `count_changes`, `empty_result_callbacks`,
///   `full_column_names`, `short_column_names`: flags for behaviour of the C
///   interface that this engine does not have.
/// - `reverse_unordered_selects`: remembered here for the read back, and also
///   handed to the planner as `Levers::FORWARD_UNORDERED`, see
///   `pragma_remembered`.
pub(crate) const REMEMBERED: &[Remembered] = &[
    Remembered {
        name: "cache_spill",
        default: 483,
        kind: Kind::CacheSpill,
    },
    Remembered {
        name: "cell_size_check",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "checkpoint_fullfsync",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "count_changes",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "empty_result_callbacks",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "full_column_names",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "fullfsync",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "hard_heap_limit",
        default: 0,
        kind: Kind::HardHeapLimit,
    },
    Remembered {
        name: "journal_size_limit",
        default: -1,
        kind: Kind::JournalSizeLimit,
    },
    Remembered {
        name: "mmap_size",
        default: 0,
        kind: Kind::MemoryMap,
    },
    Remembered {
        name: "read_uncommitted",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "reverse_unordered_selects",
        default: 0,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "short_column_names",
        default: 1,
        kind: Kind::Boolean,
    },
    Remembered {
        name: "soft_heap_limit",
        default: 0,
        kind: Kind::SoftHeapLimit,
    },
    Remembered {
        name: "threads",
        default: 0,
        kind: Kind::Threads,
    },
    Remembered {
        name: "wal_autocheckpoint",
        default: 1000,
        kind: Kind::WalAutocheckpoint,
    },
];

/// Returns the remembered setting a name spells.
///
/// @param name - the pragma's folded name
pub(crate) fn remembered_setting(name: &[u8]) -> Option<&'static Remembered> {
    REMEMBERED.iter().find(|held| held.name.as_bytes() == name)
}

/// Returns the value a set stores, or `None` when SQLite ignores the argument.
///
/// @param setting - the setting being set
/// @param text - the argument's text
/// @param current - what the setting reads now
fn stored_value(setting: &Remembered, text: &str, current: i64) -> Option<i64> {
    match setting.kind {
        Kind::Boolean => Some(i64::from(sqlite_boolean(text, false))),
        Kind::MemoryMap => {
            let (asked, _) = sqlite_integer(text);
            Some(asked.clamp(0, MAX_MMAP_SIZE))
        }
        Kind::WalAutocheckpoint => Some(i64::from(sqlite_atoi(text).max(0))),
        Kind::JournalSizeLimit => Some(sqlite_integer(text).0.max(-1)),
        Kind::Threads => {
            let (asked, whole) = sqlite_integer(text);
            (whole && asked >= 0).then(|| (asked & 0x7fff_ffff).min(MAX_WORKER_THREADS))
        }
        Kind::SoftHeapLimit => {
            let (asked, whole) = sqlite_integer(text);
            (whole && asked >= 0).then_some(asked)
        }
        Kind::HardHeapLimit => {
            let (asked, whole) = sqlite_integer(text);
            if whole && asked > 0 && (current == 0 || current > asked) {
                Some(asked)
            } else {
                None
            }
        }
        // SQLite keeps a spill size as well, and reads back its own computed
        // size and not the number given; this engine has no spill threshold, so
        // spilling is either on, which reads as the default, or off, which
        // reads 0.
        Kind::CacheSpill => {
            let size = sqlite_int32(text);
            let enabled = sqlite_boolean(text, size.is_none_or(|held| held != 0));
            Some(if enabled { 483 } else { 0 })
        }
    }
}

impl crate::ImportedDatabase {
    /// Returns what a remembered setting reads now.
    ///
    /// @param setting - the setting
    pub(crate) fn remembered_value(&self, setting: &Remembered) -> i64 {
        self.pragmas
            .remembered(setting.name)
            .unwrap_or(setting.default)
    }

    /// Reads or sets a remembered setting.
    ///
    /// A set prints the new value for every kind but the booleans, which is
    /// what SQLite does: `PRAGMA mmap_size = 1` answers a row and
    /// `PRAGMA fullfsync = 1` answers none.
    ///
    /// @param setting - the setting
    /// @param argument - the value it was given, when it was given one
    pub(crate) fn pragma_remembered(
        &mut self,
        setting: &Remembered,
        argument: Option<&PragmaArgument>,
    ) -> DbResult<Outcome> {
        let current = self.remembered_value(setting);
        let Some(argument) = argument else {
            return Ok(named_integer(setting.name, current));
        };
        let text = argument_text(argument);
        let held = match stored_value(setting, &text, current) {
            Some(value) => {
                self.pragmas.remember(setting.name, value);
                if setting.name == "reverse_unordered_selects" {
                    // A plan carries the levers it was built under, so the
                    // statements compiled before the change are dropped.
                    if (value != 0) != (current != 0) {
                        self.forget_compiled_statements();
                    }
                    self.pragmas.set_reverse_unordered(value != 0);
                }
                value
            }
            None => current,
        };
        if matches!(setting.kind, Kind::Boolean | Kind::CacheSpill) {
            return Ok(Outcome::empty());
        }
        Ok(named_integer(setting.name, held))
    }
}
