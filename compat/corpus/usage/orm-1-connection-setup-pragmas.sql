-- case: orm/1/1.1-journal-mode-wal-returns-the-new-mode-as-a-row
PRAGMA journal_mode=WAL; PRAGMA journal_mode;
-- case: orm/1/1.2-journal-mode-on-an-in-memory-database-stays-memory
PRAGMA journal_mode=WAL;
-- case: orm/1/1.3-journal-mode-delete-default-on-a-file
PRAGMA journal_mode;
-- case: orm/1/1.4-journal-mode-bogus-returns-the-current-mode-no-error
PRAGMA journal_mode=bogus;
-- case: orm/1/1.5-synchronous-default-and-normal-numeric-values
PRAGMA synchronous; PRAGMA synchronous=NORMAL; PRAGMA synchronous; PRAGMA synchronous=FULL; PRAGMA synchronous; PRAGMA synchronous=EXTRA; PRAGMA synchronous; PRAGMA synchronous=OFF; PRAGMA synchronous;
-- case: orm/1/1.6-foreign-keys-default-is-0-setting-returns-no-rows
PRAGMA foreign_keys; PRAGMA foreign_keys=ON; PRAGMA foreign_keys; PRAGMA foreign_keys=off; PRAGMA foreign_keys;
-- case: orm/1/1.7-foreign-keys-on-is-a-no-op-inside-a-transaction
BEGIN; PRAGMA foreign_keys=ON; PRAGMA foreign_keys; COMMIT; PRAGMA foreign_keys;
-- case: orm/1/1.8-busy-timeout-set-returns-the-value-as-a-row
PRAGMA busy_timeout=5000; PRAGMA busy_timeout;
-- case: orm/1/1.9-cache-size-default-and-negative-kib-form
PRAGMA cache_size; PRAGMA cache_size=-64000; PRAGMA cache_size; PRAGMA cache_size=2000; PRAGMA cache_size;
-- case: orm/1/1.10-mmap-size-set-and-read-back
PRAGMA mmap_size=268435456; PRAGMA mmap_size; PRAGMA mmap_size=0; PRAGMA mmap_size;
-- case: orm/1/1.11-temp-store-codes
PRAGMA temp_store; PRAGMA temp_store=MEMORY; PRAGMA temp_store; PRAGMA temp_store=FILE; PRAGMA temp_store; PRAGMA temp_store=DEFAULT; PRAGMA temp_store;
-- case: orm/1/1.12-user-version-round-trip-signed-32-bit-wrap
PRAGMA user_version; PRAGMA user_version=42; PRAGMA user_version; PRAGMA user_version=4294967295; PRAGMA user_version; PRAGMA user_version=-5; PRAGMA user_version;
-- case: orm/1/1.13-application-id-round-trip
PRAGMA application_id; PRAGMA application_id=1234567890; PRAGMA application_id;
-- case: orm/1/1.14-page-size-default-and-change-before-first-write
PRAGMA page_size; PRAGMA page_size=8192; CREATE TABLE t(a); PRAGMA page_size;
-- case: orm/1/1.15-page-size-ignored-after-tables-exist-in-wal-needs-vacuum
CREATE TABLE t(a); PRAGMA page_size=8192; PRAGMA page_size;
-- case: orm/1/1.16-encoding-default
PRAGMA encoding;
-- case: orm/1/1.17-journal-size-limit-and-wal-autocheckpoint
PRAGMA wal_autocheckpoint; PRAGMA wal_autocheckpoint=500; PRAGMA wal_autocheckpoint; PRAGMA journal_size_limit; PRAGMA journal_size_limit=1048576; PRAGMA journal_size_limit;
-- case: orm/1/1.18-wal-checkpoint-truncate-row-on-non-wal-db
PRAGMA wal_checkpoint(TRUNCATE);
-- case: orm/1/1.19-locking-mode-exclusive
PRAGMA locking_mode; PRAGMA locking_mode=EXCLUSIVE; PRAGMA locking_mode;
-- case: orm/1/1.20-case-sensitive-like-and-trusted-schema-and-recursive-trigger
PRAGMA case_sensitive_like; PRAGMA trusted_schema; PRAGMA recursive_triggers; PRAGMA defer_foreign_keys; PRAGMA legacy_alter_table; PRAGMA ignore_check_constraints; PRAGMA query_only; PRAGMA read_uncommitted; PRAGMA automatic_index; PRAGMA secure_delete; PRAGMA auto_vacuum; PRAGMA cell_size_check; PRAGMA count_changes; PRAGMA full_column_names; PRAGMA short_column_names;
-- case: orm/1/1.21-unknown-pragma-is-silently-ignored
PRAGMA no_such_pragma; PRAGMA no_such_pragma=5; SELECT 1;
-- case: orm/1/1.22-pragma-function-form-pragma-table-info-and-pragma-with-schem
CREATE TABLE t(a,b); PRAGMA main.user_version=3; PRAGMA main.user_version; SELECT name FROM pragma_table_info('t');
-- case: orm/1/1.23-compile-options-sqlite-version-sqlite-source-id-shape-check
SELECT typeof(sqlite_version()), typeof(sqlite_source_id()), length(sqlite_version())>=5;
-- case: orm/1/1.24-django-rails-connect-sequence-foreign-keys-then-check-same-t
PRAGMA foreign_keys = ON; PRAGMA foreign_keys; PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;
