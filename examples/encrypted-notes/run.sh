#!/usr/bin/env sh
# Encryption at rest with the inillucent command line, from the first key to a
# changed one. Every step prints what it shows. Run it from any directory:
#
#     sh run.sh
#
# It works in a fresh directory of its own, `notes-demo/`, beside this file,
# and removes nothing outside it.
set -eu

here=$(dirname "$0")
work="$here/notes-demo"
rm -rf "$work"
mkdir -p "$work"

# 1. A key file. It holds a passphrase and nothing else. A key is never given
#    as a word on the command line, where every user of the machine can read it.
printf '%s' 'correct horse battery staple' > "$work/notes.key"

# 2. Create an encrypted database and write to it.
inillucent --key-file "$work/notes.key" create "$work/notes.rdb"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" \
  exec "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT)"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" \
  exec "INSERT INTO note (body) VALUES (?1)" --params '["the vault code is 7461"]'

echo "== the row, read with the key"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" query "SELECT body FROM note"

echo "== PRAGMA encryption"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" query "PRAGMA encryption"

# 3. The bytes on the disk. grep finds nothing, because every file is ciphertext.
echo "== searching the files for the row"
if grep -l "vault code" "$work"/notes.rdb* ; then
  echo "found the row in plaintext, which is a bug" >&2
  exit 1
fi
echo "not found in any file"

# 4. No key, or the wrong key, is refused with the status corrupt.
echo "== opening without the key"
inillucent --db "$work/notes.rdb" query "SELECT body FROM note" --output json || true

# 5. The environment is the other way to give a key.
echo "== the key from INILLUCENT_KEY"
INILLUCENT_KEY='correct horse battery staple' \
  inillucent --db "$work/notes.rdb" query "SELECT count(*) FROM note"

# 6. A plaintext copy, and an encrypted copy of that.
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" decrypt "$work/plain.rdb"
inillucent --db "$work/plain.rdb" --key-file "$work/notes.key" encrypt "$work/sealed.rdb"
echo "== the plaintext copy, read with no key"
inillucent --db "$work/plain.rdb" query "SELECT body FROM note"

# 7. Change the key. The old one stops working.
printf '%s' 'a much longer passphrase than the first one' > "$work/new.key"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" rekey --new-key-file "$work/new.key"
echo "== the new key"
inillucent --db "$work/notes.rdb" --key-file "$work/new.key" query "SELECT count(*) FROM note"
echo "== the old key"
inillucent --db "$work/notes.rdb" --key-file "$work/notes.key" query "SELECT 1" || true
