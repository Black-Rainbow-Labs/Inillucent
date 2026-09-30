# Encryption at rest

## Summary

inillucent has no encryption at rest today. `docs/feature-comparison.md` says so, and `ATTACH ...
KEY` is refused by name. This design adds it.

A database opened with a key is encrypted in every file the engine writes for it: the database
file, every log segment, the rollback journal, and every temporary and spill file. Every write is
authenticated, so a changed byte is found on the next read. The plaintext path does not change,
and a database opened without a key is the same file format it is today.

The whole feature sits in one layer. `inillucent-vfs` is the only crate that touches a file, so an
encrypting `Vfs` that wraps the operating system's `Vfs` covers every file the engine opens, and no
crate above it changes how it reads or writes.

## What other databases do

| Product | Cipher | Where the nonce and tag live | Key derivation | Notes |
|---|---|---|---|---|
| SQLCipher | AES-256-CBC with HMAC-SHA512 per page | reserved bytes at the end of each page | PBKDF2-HMAC-SHA512, 256,000 iterations, 16 byte salt in the first bytes of the file | the main file, rollback journal and WAL are encrypted; the super journal holds only file names and is not |
| Turso (libSQL rewrite) | AEGIS-256 by default, AES-GCM as an option, one AEAD per page | reserved bytes at the end of each page: a 32 byte nonce and a 16 byte tag | the caller supplies the key | uses SQLite's reserved space so the B-tree never writes those bytes |
| SQLite SEE | AES, several modes | reserved bytes for a nonce | the caller supplies the key | a paid extension |

Sources: [SQLCipher design](https://www.zetetic.net/sqlcipher/design/),
[Turso encryption](https://docs.turso.tech/tursodb/encryption),
[OWASP password storage cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
(PBKDF2-HMAC-SHA256 at 600,000 iterations is its current figure).

All three put the nonce and tag inside the page, which needs the pager to know about a reserved
area. inillucent's page has no reserved area, and adding one changes the capacity of every page
kind in `inillucent-tree`. This design stores the nonce and tag beside the data instead, inside the
`Vfs`, so the page format does not change.

## Decisions

### 1. The primitives are first party

`docs/dependency-policy.md` already lists SHA-256, SHA3-256 and CRC-32 as first party, because
each is part of a file format a dependency upgrade must not change. An encrypted file is a file
format too. So these are written in `inillucent-base::crypt`:

| Primitive | Standard | Checked against |
|---|---|---|
| ChaCha20 | RFC 8439 section 2.4 | RFC 8439 test vectors 2.4.2 and A.2 |
| Poly1305 | RFC 8439 section 2.5 | RFC 8439 test vectors 2.5.2 and A.3 |
| ChaCha20-Poly1305 AEAD | RFC 8439 section 2.8 | RFC 8439 test vector 2.8.2 and A.5 |
| HChaCha20 and XChaCha20-Poly1305 | draft-irtf-cfrg-xchacha-03 | the draft's test vectors 2.2.1 and A.3.1 |
| HMAC-SHA256 | RFC 2104 | RFC 4231 test cases 1 to 4, 6 and 7 |
| PBKDF2-HMAC-SHA256 | RFC 8018 | the RFC 7914 section 11 vectors |
| constant time comparison | | a unit test |

ChaCha20 was chosen over AES because it is constant time in plain Rust with no table lookups and no
processor instructions. A first party AES without AES-NI would either leak timing through its
tables or be a bitsliced implementation several times the size. XChaCha20 was chosen over
ChaCha20 because its 192 bit nonce makes random nonces safe for any number of writes.

No new crate is added, so `layering.toml` does not change.

### 2. Keys

| What the caller gives | How the key encryption key is made |
|---|---|
| a passphrase | PBKDF2-HMAC-SHA256, 600,000 iterations, a random 16 byte salt stored in the file header |
| `x'` followed by 64 hex digits and `'` | used as the 32 byte key directly, as SQLCipher does |

The key encryption key never encrypts data. Each database gets a random 32 byte data key when it is
created, and the header stores that data key encrypted with XChaCha20-Poly1305 under the key
encryption key. So `PRAGMA rekey` rewrites one header, not every page, and a wrong key is found by
one AEAD check on the header before any page is read.

The iteration count is stored in the header, so a later release can raise it for new files and
still open old ones.

### 3. The encrypted file layout

Every file the encrypting `Vfs` opens, apart from temporary files, starts with an 8,192 byte header
area: two header slots of 4,096 bytes. After the header come the units. A unit is the amount of
plaintext encrypted together.

**Header slot** (all integers little endian):

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | magic `INLCRYPT` |
| 8 | 4 | format version, 1 |
| 12 | 4 | cipher, 1 means XChaCha20-Poly1305 |
| 16 | 4 | key derivation, 0 means a raw key, 1 means PBKDF2-HMAC-SHA256 |
| 20 | 4 | iterations |
| 24 | 4 | layout, 1 means single slot units, 2 means double slot units |
| 28 | 4 | unit size in plaintext bytes |
| 32 | 16 | salt |
| 48 | 16 | database id, shared by the database file and every file beside it |
| 64 | 16 | file id, random per file |
| 80 | 8 | slot generation |
| 88 | 24 | nonce of the wrapped data key |
| 112 | 48 | the wrapped data key: 32 bytes of ciphertext and a 16 byte tag |
| 160 | 32 | SHA-256 of bytes 0 to 160 |

The wrapped data key's associated data is bytes 0 to 80, so every field apart from the generation
is authenticated by the key. The SHA-256 at 160 needs no key and only tells a torn slot from a
whole one. On open the newest whole slot is the header, and only that slot. `PRAGMA rekey` writes
the older slot with the next generation, syncs, then writes the other slot and syncs again, so a
crash at any point leaves one whole slot, and the old key stops working as soon as the first write
is on the disk.

**Unit trailer** (the last 64 bytes of each unit's physical space):

| Offset | Size | Field |
|---|---|---|
| 0 | 24 | nonce |
| 24 | 16 | Poly1305 tag |
| 40 | 8 | unit generation |
| 48 | 4 | plaintext length of this unit, which is less than the unit size only for the last unit |
| 52 | 12 | zero |

The associated data of a unit is the file id, the unit number, the generation and the length. A
unit moved to another position or another file fails its tag. An all zero trailer is a unit that
was never written, and it reads as zeros, which is what a hole in a plaintext file reads as.

**Nonces** are a 16 byte random prefix taken from the operating system when the `Vfs` is made,
followed by an 8 byte counter. XChaCha20's 192 bit nonce makes that unique across processes with
overwhelming probability, and it costs no system call per write.

### 4. The three layouts, and why each is safe against a torn write

The durability suites run on `inillucent_sim`'s pessimistic disk model: a crash can apply, drop,
tear or garble each 512 byte sector an unsynced write touched. Plaintext pages survive that model
because the engine never lets a write share a sector with data that must survive. An AEAD unit
breaks that assumption in one way: rewriting any byte of a unit rewrites all of it with a new
nonce, so a torn rewrite damages bytes the engine did not ask to change. Each file kind gets the
layout that keeps the plaintext guarantee.

| File kind | Layout | Unit | Physical space per unit | Why |
|---|---|---|---|---|
| database file | single slot | the page size, from 8 KiB to 64 KiB | the unit plus one 512 byte sector for the trailer | the pool writes whole pages at page offsets, so a unit is never shared by two pages. A torn page write damages only that page, which is what a torn plaintext page does, and recovery already rebuilds such a page from the log |
| log segment, rollback journal, sub journal | double slot | 4,032 bytes | two slots of 4,096 bytes | these files append records that do not line up with any unit, and a later append rewrites the unit an earlier synced record ends in. The rewrite goes to the other slot, so a tear leaves the older synced slot readable |
| temporary and spill files | single slot, no header | 4,032 bytes | 4,096 bytes | nothing reads them after a crash. They use a random key made for the `Vfs` and never stored |

**The double slot rule.** Each slot carries a generation. A read takes the valid slot with the
highest generation. A write goes to the slot that does not hold the newest valid version, with the
next generation, unless this handle wrote the newest version itself and has not synced since. In
that case it overwrites its own unsynced slot, so the slot holding the last synced version is never
touched before a sync. A handle forgets what it wrote at every `sync`.

The cost is space: a log segment takes a little over twice its plaintext size on disk. Log
segments are removed at every checkpoint, so this is temporary space.

The page size is known to the encrypting `Vfs` from the header of an existing file. For a new file
the engine passes the page size it is creating with.

### 5. A unit that fails its tag

A torn write and a deliberate change look the same to the `Vfs`. The engine already handles a torn
page: the page checksum fails and recovery applies every log record for it again. So a unit that
fails its tag is not an I/O error. The read returns random bytes from the operating system, which
fail the page checksum and every log record checksum, and the `Vfs` counts the failure.

Returning the ciphertext instead would let whoever changed the file choose what the engine reads,
so random bytes are used instead of the stored bytes.

The header is the exception. A header whose wrapped data key fails its tag means the key is wrong
or the header was changed, and the open fails with `file is not a database, or the key is wrong`.

### 6. What a caller sees

| Surface | How a key is given |
|---|---|
| Rust engine | `Database::open_encrypted(path, key)`; `EncryptionKey::passphrase(text)` or `EncryptionKey::raw(bytes)` |
| Rust driver | `OpenOptions { key: Some(Key::passphrase(..)), .. }` |
| C | `inillucent_open_with_key(path, flags, key, key_length, out, error)` |
| `inillucent` command line, `inillucent-shell`, `inillucent-mcp` | `--key-file <path>`, or the `INILLUCENT_KEY` environment variable. There is no `--key <text>`, because a key on a command line can be read by every user of the machine |
| Python, Node, Go and PHP packages | a `key` option that sets `INILLUCENT_KEY` for the child process |
| SQL | `PRAGMA rekey = 'new passphrase'`, `PRAGMA encryption` (reads `xchacha20-poly1305` or `none`), `ATTACH 'x.rdb' AS x KEY 'passphrase'` |

`ATTACH` without `KEY` on an encrypted connection uses the connection's key, as SQLCipher does.
`KEY ''` attaches a plaintext file.

`VACUUM` and `VACUUM INTO` go through the connection's `Vfs`, so their output is encrypted with the
same key. `backup` copies the encrypted file and opens the copy with the same key to check it.

Two commands convert a file: `inillucent encrypt <output>` writes an encrypted copy of a plaintext
database, and `inillucent decrypt <output>` writes a plaintext copy of an encrypted one. Both
rebuild through the same code as `VACUUM INTO`.

An encrypted file opened without a key fails with `this database is encrypted. Open it with a
key`. A plaintext file opened with a key fails with `this database is not encrypted`. Both have
the status `corrupt`, which is what SQLite and SQLCipher answer for a file they cannot read as a
database.

### 7. What this does not protect

- **Rollback of a unit or a whole file.** Someone who can write the file can put back an older copy
  of a unit or of the file. The tag passes because the old unit was genuine. SQLCipher and Turso
  have the same limit. Detecting it needs a hash tree over every unit, which is not in scope.
- **Sizes and write patterns.** The file size, the number of log segments, and which units changed
  between two copies of the file are visible.
- **Memory.** Keys are in process memory while the database is open. They are overwritten with
  zeros when the `Vfs` is dropped, but a swap file or a crash dump can hold them.
- **Password guessing on a GPU.** PBKDF2 is the weakest of the OWASP options against GPUs. A raw
  32 byte key from a key manager avoids the question.
- **The super journal.** It holds file names and no rows, and is written outside the `Vfs`, as
  SQLCipher leaves its super journal plaintext.

## Tests

| Test | Where | What it proves |
|---|---|---|
| published vectors for every primitive | `inillucent-base` unit tests | each primitive matches its standard |
| round trip, wrong key, no key, key on a plaintext file | `inillucent-vfs` unit tests and an engine suite | the header check and the three refusals |
| no plaintext on disk | engine suite | a table of known strings, a search index and the log are written, then every file beside the database is searched for the strings and none is found |
| tamper | engine suite | one byte of a page is flipped; the read reports damage and `PRAGMA integrity_check` names the page |
| double slot tear safety | `inillucent-vfs` unit tests on `inillucent_sim` | a synced record survives every crash cut of a later append to the same unit |
| the durability crash campaign over the encrypting `Vfs` | `inillucent-compat` durability tier | every crash cut of a workload is recoverable, as it is in plaintext |
| `PRAGMA rekey` | engine suite | the old key stops opening the file and the new one opens it; a crash between the two header writes leaves a file one of the keys opens |
| `ATTACH ... KEY`, `VACUUM`, `VACUUM INTO`, `backup`, `encrypt`, `decrypt` | engine and command line suites | each output is encrypted, or not, as stated, and holds the same rows |
| `encryption` capability row | the capability test | the row says yes and the check opens an encrypted database |
| command table parity | tooling | the two new commands appear in the command line and in MCP |

## Performance

Measured with the release command line against the plaintext engine on the same machine, each
workload run twice as a whole program. The table is in `docs/encryption.md`. In short: a 100,000
row insert in one statement takes about 1.55 times as long, a cold full scan about 1.35 times, 1,000
separate commits about 1.07 times, the database file is 1.6% larger, and opening with a passphrase
costs about 200 ms more than with a raw key. The first measurement found ChaCha20 building its
state for every 64 byte block and Poly1305 copying every byte through its buffer. Fixing both took
the encrypted scan from 130 ms to 97 ms against 72 ms in plaintext. A third fix answers a database
file's length from its last trailer instead of decrypting its last page, which the pool asks for
once per page a checkpoint journals. It took the encrypted insert from about 990 ms to about 770 ms,
against about 500 ms in plaintext.

## Pages to update

`docs/encryption.md` (new), `docs/sql.md`, `docs/pragmas.md`, `docs/feature-comparison.md`,
`docs/dependency-policy.md`, `docs/glossary.md`, `docs/architecture-overview.md`,
`docs/relational-architecture.md`, `docs/README.md`, `README.md`, `SECURITY.md`, `CHANGELOG.md`,
`drivers/README.md`, the package readmes, the skills under `agent-skills/`, an example, and the
documentation chapter on inillucent.com.

## What changed while it was built

These are the places the implementation differs from the design above, and why.

1. **A log or journal whose header cannot be read opens as an empty file.** The first run of the
   crash campaign over the encrypting file system found it: a log segment created, written and
   never synced can come back from a crash with a garbled header, and the design refused such a
   file as not encrypted. The plaintext engine reads a segment whose own header is garbage as the
   end of the chain, and the header is the only place a file's id is kept, so nothing in such a
   file can be read by anybody. A read only open now sees an empty file, and a writable open
   writes a fresh header. The database file keeps the strict rule: an unreadable header is refused.
2. **`key`, `rekey` and `encryption` are in the pragma register.** `docs/pragmas.md` says a pragma
   not in its table is not recognised, so the three are registered. `pragma_list` still reports
   SQLite's own list, which does not have them.
3. **`encrypt`, `decrypt` and `rekey` are command line only.** An MCP client has no way to give a
   key, a plaintext copy of an encrypted database is a decision for the person who holds the key,
   and an agent that could change the key could lock the owner out.
4. **The C ABI went to 1.1.0** for `inillucent_open_with_key`, which takes the key as a C string
   and is stable. A binding written against 1.0.0 still finds every symbol it calls.
5. **`ATTACH` with a bound parameter as its file name** was refused before this change under the
   `attach_with_key` capability row, because the statement matrix case that reached it also had a
   `KEY` clause. With `KEY` supported, that refusal has a row of its own, `attach_computed_path`.
6. **The engine checks the file before it opens it**, so a plaintext file opened with a key and an
   encrypted file opened without one each get a sentence naming the fix rather than "file is not a
   database".
