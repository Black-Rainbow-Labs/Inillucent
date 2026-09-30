# Encrypted notes

A notes database encrypted at rest, driven from the `inillucent` command line. `run.sh` creates the
database with a key, writes a row, shows that no file holds the row in plaintext, shows the refusal
without the key, makes a plaintext copy and an encrypted copy, and changes the key.

```sh
sh run.sh
```

It needs a command line that has encryption. `inillucent help encrypt` prints the command's help on
such a build and refuses on an older one. The top level [README](../../README.md#install) lists
every way to install it.

## What each step shows

| Step | Command | What it shows |
|---|---|---|
| 1 | `printf '%s' '...' > notes.key` | the key lives in a file. It is never a word on the command line, where every user of the machine can read it |
| 2 | `inillucent --key-file notes.key create notes.rdb` | a database created with a key is encrypted |
| 3 | `grep -l "vault code" notes.rdb*` | the row is in no file: the database file and its log are ciphertext |
| 4 | `inillucent --db notes.rdb query ...` | with no key the command fails with the status `corrupt` and says the database is encrypted |
| 5 | `INILLUCENT_KEY=... inillucent ...` | the environment is the other way to give the key |
| 6 | `decrypt plain.rdb`, then `encrypt sealed.rdb` | a plaintext copy, and an encrypted copy of a plaintext database |
| 7 | `rekey --new-key-file new.key` | the key changes and the old one stops opening the database |

The script works in `notes-demo/` beside it and removes that folder when it starts again.

## The output

```
== the row, read with the key
body
----------------------
the vault code is 7461
== PRAGMA encryption
encryption
------------------
xchacha20-poly1305
== searching the files for the row
not found in any file
== opening without the key
{
  "ok": false,
  "command": "query",
  "status": "corrupt",
  "message": "could not open \"notes-demo/notes.rdb\": this database is encrypted. Open it with its key: --key-file, the INILLUCENT_KEY environment variable, or a driver's key option",
  ...
}
...
== the old key
Error [corrupt]: could not open "notes-demo/notes.rdb": file is not a database, or the key is wrong
```

## Where to read more

The repository's `docs/encryption.md` describes what is encrypted, how the key reaches each program
and driver, what encryption costs, and what it does not protect.
