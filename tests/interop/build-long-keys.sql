-- What a release that writes format 3 adds to its fixture, after build.sql.
--
-- `tools/build-interop-fixture.ps1` runs this only when the file the release
-- created says format 3 in byte 8. A release that writes format 2 refuses a key
-- longer than about 32 KB, so it never runs this file, and its fixture is the
-- same as before.
--
-- 83 keys of about 5,000 bytes go into an index. A key longer than
-- page_size / 8, 4,096 bytes at the engine's 32,768 byte page, is stored on
-- pages of its own with its first 512 bytes in the leaf. All of them share
-- their first 4,992 bytes, so the index splits and every separator between two
-- of its leaves is longer than page_size / 8, which an interior page does not
-- keep whole, so the separators are stored out of line too. The keys are kept
-- near 5,000 bytes because each fixture is checked in.
-- `replace(hex(zeroblob(n)), '0', 'k')` is 2n letters k, so the keys are
-- deterministic and every release that writes this file writes the same values.
--
-- `crates/inillucent-compat/tests/e2e/release_format.rs` reads them back from
-- every fixture that holds the `long_key` table.

CREATE TABLE long_key (id INTEGER PRIMARY KEY, k TEXT NOT NULL);
CREATE INDEX long_key_k ON long_key (k);

INSERT INTO long_key (id, k) VALUES
  (1, replace(hex(zeroblob(2496)), '0', 'k')),
  (2, replace(hex(zeroblob(2496)), '0', 'k') || 'a'),
  (3, replace(hex(zeroblob(2496)), '0', 'k') || 'b'),
  (4, 'short');

INSERT INTO long_key (id, k)
  SELECT value, replace(hex(zeroblob(2496)), '0', 'k') || printf('%03d', value)
    FROM generate_series(10, 89);
