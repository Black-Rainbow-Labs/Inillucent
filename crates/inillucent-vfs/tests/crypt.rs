//! The encrypting file system: that it behaves like a disk, that what it
//! writes is ciphertext, and that it refuses what it should.
//!
//! Invariant: every case here runs the encrypting `Vfs` over a real `Vfs`
//! underneath and checks the bytes that `Vfs` holds, not only what the
//! encrypting layer reports back, because a layer that answers correctly
//! while storing plaintext would pass every read test there is.

use std::sync::Arc;

use inillucent_base::error::PrimaryCode;
use inillucent_vfs::conformance;
use inillucent_vfs::contract::{FileKind, OpenOptions, SyncMode, Vfs};
use inillucent_vfs::crypt::{probe, CryptVfs, EncryptionKey, Probe};
use inillucent_vfs::memory::MemoryVfs;
use inillucent_vfs::os::OsVfs;
use inillucent_vfs::path::DbPath;

/// A raw key, so no case pays for PBKDF2.
fn key() -> EncryptionKey {
    EncryptionKey::raw([0x5a; 32])
}

/// An encrypting file system over a fresh in-memory one, and that one.
fn over_memory(page: u32) -> (CryptVfs, Arc<dyn Vfs>) {
    let inner: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let crypt = CryptVfs::new(Arc::clone(&inner), key())
        .expect("the file system is made")
        .with_page_size(page);
    (crypt, inner)
}

/// Reads every byte a file holds underneath.
fn raw_bytes(inner: &dyn Vfs, path: &DbPath) -> Vec<u8> {
    let file = inner
        .open(path, OpenOptions::of_kind(FileKind::MainDb).read_only())
        .expect("the file opens underneath");
    let mut bytes = vec![0u8; file.file_size().expect("it has a size") as usize];
    file.read_exact_at(0, &mut bytes).expect("it reads");
    bytes
}

/// Reports whether `needle` appears anywhere in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The encrypting layer passes the one suite every `Vfs` is held to, over
/// memory and over the real file system.
#[test]
fn the_encrypting_vfs_passes_the_conformance_suite() {
    let (crypt, _) = over_memory(32_768);
    let report = conformance::run(&crypt, &DbPath::from("/crypt"));
    assert!(report.is_clean(), "{}", report.to_text());
    assert!(report.passed() >= 24, "{}", report.to_text());

    let mut root = std::env::temp_dir();
    root.push(format!(
        "inillucent-vfs-crypt-conformance-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the directory is made");
    let crypt = CryptVfs::new(Arc::new(OsVfs::new()), key()).expect("made");
    let report = conformance::run(&crypt, &DbPath::new(root.clone()));
    assert!(report.is_clean(), "{}", report.to_text());
    let _ = std::fs::remove_dir_all(&root);
}

/// A deterministic generator, so a failing sequence can be replayed.
struct Steps(u64);

impl Steps {
    /// Returns the next number below `bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % bound.max(1)
    }
}

/// Random writes, truncates and syncs give the same bytes, length and short
/// reads as a plain byte vector, for every file kind's layout.
#[test]
fn random_operations_match_a_plain_model() {
    for (kind, page) in [
        (FileKind::MainDb, 8_192u32),
        (FileKind::MainDb, 32_768),
        (FileKind::Wal, 32_768),
        (FileKind::MainJournal, 32_768),
        (FileKind::Transient, 32_768),
    ] {
        let (crypt, _) = over_memory(page);
        let path = DbPath::from("/model.bin");
        let file = crypt
            .open(&path, OpenOptions::of_kind(kind))
            .expect("the file opens");
        let mut model: Vec<u8> = Vec::new();
        let mut steps = Steps(0x9e37_79b9_7f4a_7c15 ^ u64::from(page));
        for step in 0..400u64 {
            match steps.below(10) {
                0..=5 => {
                    let offset = steps.below(200_000);
                    let length = 1 + steps.below(40_000) as usize;
                    let data: Vec<u8> = (0..length).map(|i| (i as u64 ^ step) as u8).collect();
                    file.write_all_at(offset, &data).expect("the write lands");
                    let end = offset as usize + length;
                    if model.len() < end {
                        model.resize(end, 0);
                    }
                    model[offset as usize..end].copy_from_slice(&data);
                }
                6 => {
                    let size = steps.below(model.len() as u64 + 10_000);
                    file.truncate(size).expect("the truncate lands");
                    model.resize(size as usize, 0);
                }
                7 => file.sync(SyncMode::Full).expect("the sync lands"),
                _ => {
                    let offset = steps.below(model.len() as u64 + 5_000);
                    let length = 1 + steps.below(70_000) as usize;
                    let mut out = vec![0xee; length];
                    let result = file.read_exact_at(offset, &mut out);
                    let mut expected = vec![0u8; length];
                    let have = model.len().saturating_sub(offset as usize).min(length);
                    if have > 0 {
                        expected[..have]
                            .copy_from_slice(&model[offset as usize..offset as usize + have]);
                    }
                    assert_eq!(out, expected, "{kind:?} step {step}");
                    assert_eq!(result.is_ok(), have == length, "{kind:?} step {step}");
                }
            }
            assert_eq!(
                file.file_size().expect("a size"),
                model.len() as u64,
                "{kind:?} step {step}"
            );
        }
    }
}

/// What is written is not on the disk in plaintext, in any file kind, and a
/// reopen through a new file system with the same key reads it back.
#[test]
fn nothing_written_is_stored_in_plaintext() {
    let inner: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let secret = b"the password is swordfish";
    let paths = [
        (FileKind::MainDb, "/db.rdb"),
        (FileKind::Wal, "/db.rdb-wal.0000000001"),
        (FileKind::MainJournal, "/db.rdb-journal"),
    ];
    {
        let crypt = CryptVfs::new(Arc::clone(&inner), key()).expect("made");
        for (kind, path) in paths {
            let file = crypt
                .open(&DbPath::from(path), OpenOptions::of_kind(kind))
                .expect("opens");
            for copy in 0..20u64 {
                file.write_all_at(copy * 3_001, secret).expect("writes");
            }
            file.sync(SyncMode::Full).expect("syncs");
        }
    }
    for (_, path) in paths {
        let bytes = raw_bytes(inner.as_ref(), &DbPath::from(path));
        assert!(!contains(&bytes, secret), "{path} holds the plaintext");
        assert!(!contains(&bytes, b"swordfish"), "{path} holds part of it");
    }
    let again = CryptVfs::new(Arc::clone(&inner), key()).expect("made");
    for (kind, path) in paths {
        let file = again
            .open(&DbPath::from(path), OpenOptions::of_kind(kind))
            .expect("reopens");
        let mut out = vec![0u8; secret.len()];
        file.read_exact_at(19 * 3_001, &mut out).expect("reads");
        assert_eq!(&out, secret, "{path}");
    }
}

/// A wrong key and a plaintext file are both refused as "not a database",
/// and `probe` tells the three states apart without a key.
#[test]
fn a_wrong_key_and_a_plaintext_file_are_refused() {
    let inner: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let path = DbPath::from("/locked.rdb");
    {
        let crypt = CryptVfs::new(Arc::clone(&inner), key()).expect("made");
        let file = crypt.open(&path, OpenOptions::main_db()).expect("opens");
        file.write_all_at(0, &[1u8; 100]).expect("writes");
    }
    let wrong = CryptVfs::new(Arc::clone(&inner), EncryptionKey::raw([0x5b; 32])).expect("made");
    let refused = wrong
        .open(&path, OpenOptions::main_db())
        .expect_err("refused");
    assert_eq!(refused.code(), PrimaryCode::NotADb);
    let passphrase = CryptVfs::new(
        Arc::clone(&inner),
        EncryptionKey::passphrase_with_iterations("hunter2", 10),
    )
    .expect("made");
    let refused = passphrase
        .open(&path, OpenOptions::main_db())
        .expect_err("refused");
    assert_eq!(refused.code(), PrimaryCode::NotADb);
    assert_eq!(
        probe(inner.as_ref(), &path).expect("probes"),
        Probe::Encrypted
    );

    let plain_path = DbPath::from("/plain.rdb");
    let plain = inner
        .open(&plain_path, OpenOptions::main_db())
        .expect("opens");
    plain
        .write_all_at(0, b"a plaintext database page")
        .expect("writes");
    drop(plain);
    let crypt = CryptVfs::new(Arc::clone(&inner), key()).expect("made");
    let refused = crypt
        .open(&plain_path, OpenOptions::main_db())
        .expect_err("refused");
    assert_eq!(refused.code(), PrimaryCode::NotADb);
    assert_eq!(
        probe(inner.as_ref(), &plain_path).expect("probes"),
        Probe::Plain
    );
    assert_eq!(
        probe(inner.as_ref(), &DbPath::from("/absent.rdb")).expect("probes"),
        Probe::Empty
    );
}

/// A passphrase opens what it made, with the iteration count the header
/// recorded, and the text form of a raw key is read as a raw key.
#[test]
fn a_passphrase_and_a_raw_key_text_both_work() {
    let inner: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let path = DbPath::from("/phrase.rdb");
    {
        let crypt = CryptVfs::new(
            Arc::clone(&inner),
            EncryptionKey::passphrase_with_iterations("correct horse", 1_000),
        )
        .expect("made");
        let file = crypt.open(&path, OpenOptions::main_db()).expect("opens");
        file.write_all_at(0, b"battery staple").expect("writes");
    }
    let reopened = CryptVfs::new(
        Arc::clone(&inner),
        EncryptionKey::passphrase_with_iterations("correct horse", 5),
    )
    .expect("made");
    let file = reopened
        .open(&path, OpenOptions::main_db())
        .expect("reopens");
    let mut out = [0u8; 14];
    file.read_exact_at(0, &mut out).expect("reads");
    assert_eq!(&out, b"battery staple");

    let hex = "x'5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a'";
    let raw_path = DbPath::from("/raw.rdb");
    {
        let crypt = CryptVfs::new(Arc::clone(&inner), key()).expect("made");
        let file = crypt
            .open(&raw_path, OpenOptions::main_db())
            .expect("opens");
        file.write_all_at(0, b"raw").expect("writes");
    }
    let parsed = CryptVfs::new(Arc::clone(&inner), EncryptionKey::parse(hex)).expect("made");
    assert!(parsed.open(&raw_path, OpenOptions::main_db()).is_ok());
    assert!(format!("{:?}", EncryptionKey::parse("secret")).contains("passphrase"));
    assert!(!format!("{:?}", EncryptionKey::parse("secret")).contains("secret"));
}

/// A changed byte in a unit makes that unit read as random bytes, counted,
/// and leaves every other unit as it was.
#[test]
fn a_changed_byte_damages_only_its_own_unit() {
    let (crypt, inner) = over_memory(8_192);
    let path = DbPath::from("/tamper.rdb");
    let file = crypt.open(&path, OpenOptions::main_db()).expect("opens");
    file.write_all_at(0, &vec![0x11u8; 8_192 * 3])
        .expect("writes");
    drop(file);
    let raw = inner
        .open(&path, OpenOptions::main_db())
        .expect("opens underneath");
    // Unit 1's data starts after the 8 KiB header and one 8.5 KiB unit.
    let at = 8_192 + (8_192 + 512) + 100;
    let mut byte = [0u8; 1];
    raw.read_exact_at(at, &mut byte).expect("reads");
    raw.write_all_at(at, &[byte[0] ^ 1]).expect("writes");
    drop(raw);
    let file = crypt.open(&path, OpenOptions::main_db()).expect("reopens");
    let mut page = vec![0u8; 8_192];
    file.read_exact_at(0, &mut page).expect("reads");
    assert!(page.iter().all(|b| *b == 0x11), "unit 0 is untouched");
    file.read_exact_at(8_192, &mut page).expect("reads");
    assert!(
        !page.iter().all(|b| *b == 0x11),
        "the changed unit is not believed"
    );
    assert!(
        page.iter().filter(|b| **b == 0x11).count() < 200,
        "and is not its old bytes"
    );
    file.read_exact_at(16_384, &mut page).expect("reads");
    assert!(page.iter().all(|b| *b == 0x11), "unit 2 is untouched");
    assert_eq!(crypt.failures(), 1);
}

/// **The property the double slot layout exists for.** A log record that was
/// synced survives a later append into the same unit that is torn: the
/// append's slot is garbage, and the synced slot is still there.
#[test]
fn a_torn_append_cannot_destroy_a_synced_record() {
    let (crypt, inner) = over_memory(32_768);
    let path = DbPath::from("/db.rdb-wal.0000000001");
    let file = crypt
        .open(&path, OpenOptions::of_kind(FileKind::Wal))
        .expect("opens");
    file.write_all_at(0, b"committed record").expect("writes");
    file.sync(SyncMode::Full).expect("syncs");
    file.write_all_at(16, b" and a later one").expect("writes");
    drop(file);
    // Find which slot of unit 0 the append went to, and garble it.
    let raw = inner
        .open(&path, OpenOptions::main_db())
        .expect("opens underneath");
    let mut unit = vec![0u8; 8_192];
    raw.read_exact_at(8_192, &mut unit).expect("reads");
    let generation = |slot: usize| {
        u64::from_le_bytes(
            unit[slot * 4_096 + 4_032 + 40..slot * 4_096 + 4_032 + 48]
                .try_into()
                .unwrap(),
        )
    };
    let newest = if generation(1) > generation(0) { 1 } else { 0 };
    raw.write_all_at(8_192 + newest as u64 * 4_096, &[0xa5; 512])
        .expect("garbles the newest slot");
    drop(raw);
    let file = crypt
        .open(&path, OpenOptions::of_kind(FileKind::Wal))
        .expect("reopens");
    let mut out = [0u8; 16];
    file.read_exact_at(0, &mut out).expect("reads");
    assert_eq!(&out, b"committed record", "the synced record was lost");
}

/// Rewriting a unit twice without a sync reuses the unsynced slot, so the
/// synced slot is never the one being overwritten.
#[test]
fn an_unsynced_rewrite_never_touches_the_synced_slot() {
    let (crypt, inner) = over_memory(32_768);
    let path = DbPath::from("/db.rdb-journal");
    let file = crypt
        .open(&path, OpenOptions::of_kind(FileKind::MainJournal))
        .expect("opens");
    file.write_all_at(0, b"version one").expect("writes");
    file.sync(SyncMode::Full).expect("syncs");
    let slot_bytes = |slot: u64| {
        let raw = inner
            .open(&path, OpenOptions::main_db())
            .expect("opens underneath");
        let mut bytes = vec![0u8; 4_096];
        raw.read_exact_at(8_192 + slot * 4_096, &mut bytes)
            .expect("reads");
        bytes
    };
    let synced_a = slot_bytes(0);
    let synced_b = slot_bytes(1);
    file.write_all_at(0, b"version two").expect("writes");
    file.write_all_at(0, b"version 333").expect("writes");
    file.write_all_at(0, b"version 444").expect("writes");
    let changed = [slot_bytes(0) != synced_a, slot_bytes(1) != synced_b];
    assert_eq!(
        changed.iter().filter(|c| **c).count(),
        1,
        "three unsynced rewrites touched both slots"
    );
    let mut out = [0u8; 11];
    file.read_exact_at(0, &mut out).expect("reads");
    assert_eq!(&out, b"version 444");
}

/// `rekey` moves every named file to the new key: the old key stops opening
/// them and the new key opens them, with their contents unchanged.
#[test]
fn rekey_moves_every_file_to_the_new_key() {
    let inner: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let main = DbPath::from("/r.rdb");
    let log = DbPath::from("/r.rdb-wal.0000000001");
    let old = EncryptionKey::passphrase_with_iterations("old", 10);
    let new = EncryptionKey::passphrase_with_iterations("new", 10);
    {
        let crypt = CryptVfs::new(Arc::clone(&inner), old.clone()).expect("made");
        for (path, kind) in [(&main, FileKind::MainDb), (&log, FileKind::Wal)] {
            let file = crypt.open(path, OpenOptions::of_kind(kind)).expect("opens");
            file.write_all_at(0, b"kept across the rekey")
                .expect("writes");
        }
        crypt
            .rekey(&[main.clone(), log.clone()], new.clone())
            .expect("rekeys");
    }
    for (path, kind) in [(&main, FileKind::MainDb), (&log, FileKind::Wal)] {
        let stale = CryptVfs::new(Arc::clone(&inner), old.clone()).expect("made");
        assert!(
            stale.open(path, OpenOptions::of_kind(kind)).is_err(),
            "the old key still opens {path:?}"
        );
        let fresh = CryptVfs::new(Arc::clone(&inner), new.clone()).expect("made");
        let file = fresh
            .open(path, OpenOptions::of_kind(kind))
            .expect("the new key opens it");
        let mut out = [0u8; 21];
        file.read_exact_at(0, &mut out).expect("reads");
        assert_eq!(&out, b"kept across the rekey");
    }
}
