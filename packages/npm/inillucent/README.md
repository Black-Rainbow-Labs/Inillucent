# inillucent

inillucent is an embedded SQL database. It speaks SQLite's dialect on its own storage, and it has
keyword search and vector search built in. One `.rdb` file holds the tables and the search indexes,
and one process reads and writes it.

This npm package installs the four inillucent programs and a small JavaScript API that runs them.

## Install

```sh
npm install -g inillucent
```

Or run a program once without installing it:

```sh
npx inillucent help
```

The package needs Node 18 or later. There is no install script and nothing downloads at install
time. The programs come in one extra package per platform, listed as optional dependencies. npm
installs only the one that matches your machine, so `npm ci` works offline and behind a registry
proxy.

| Platform | Package npm installs |
|---|---|
| Windows, x64 | `@blackrainbowlabs/cli-win32-x64` |
| macOS, Apple silicon | `@blackrainbowlabs/cli-darwin-arm64` |
| macOS, Intel | `@blackrainbowlabs/cli-darwin-x64` |
| Linux, x64 | `@blackrainbowlabs/cli-linux-x64` |
| Linux, arm64 | `@blackrainbowlabs/cli-linux-arm64` |

Each platform package holds the four programs in `bin/`, the C library in `lib/`, and its header
`inillucent_driver.h` in `include/`. If you installed with `--no-optional`, install the platform
package by name, for example `npm install @blackrainbowlabs/cli-linux-x64`.

## The four programs

| Program | What it is |
|---|---|
| `inillucent` | the command line: 34 commands, such as `query`, `exec`, `describe`, `import`, `export` and `search` |
| `inillucent-shell` | an interactive shell that works like `sqlite3`, with 63 of its 65 dot commands |
| `inillucent-mcp` | an MCP server: 29 of the same commands served to an AI agent |
| `inillucent-migrate` | builds a database from a legacy retrieval index. `inillucent migrate` copies a SQLite file or a PostgreSQL or MySQL database |

## From a shell

```sh
inillucent create app.rdb
inillucent --db app.rdb exec "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT)"
inillucent --db app.rdb exec "INSERT INTO notes (body) VALUES (?1)" --params '["hello"]'
inillucent --db app.rdb query "SELECT * FROM notes"
inillucent --db app.rdb describe notes
inillucent help
```

`--params` binds `?1`, `?2` and so on, in order. Add `--output json` to any command to get a JSON
object a program can parse.

| Exit code | Meaning |
|---|---|
| `0` | success |
| `1` | the command failed |
| `2` | the command line could not be read, such as an unknown command |
| `3` | the engine has not built that feature. Rewording the SQL does not help |

## From JavaScript

```js
import { query, inillucent } from 'inillucent';

await inillucent('exec', { db: 'app.rdb', sql: 'INSERT INTO notes (body) VALUES (?1)', params: ['world'] });

const rows = await query('SELECT id, body FROM notes WHERE id > ?1', { db: 'app.rdb', params: [1] });
// [ { id: 2, body: 'world' } ]

const described = await inillucent('describe', { db: 'app.rdb', table: 'notes' });
console.log(described.ddl, described.indexes, described.row_count_in_table);
```

`query()`, and `inillucent()` with the `query`, `exec` and `batch` commands, run in this process
when the platform package carries the C library, which it does on every supported platform. The
library is loaded as a Node addon, so a call opens the file, runs the statement and closes the file
without starting anything.

A call whose statement starts with `SELECT`, `WITH` or `VALUES` keeps the file open for the next
such call on the same file, and the file closes one second after the last of them. Opening the file
is most of the cost of a single read, so a lookup by key that took 0.42 ms takes 0.04 ms, against
0.09 ms for `node:sqlite` opening the file, reading and closing it. Nothing a caller sees changes.
An open file holds no lock between statements, every statement checks whether another connection
or process has written since the last one, and a file deleted and made again at the same path is
opened afresh. Every other statement still opens and closes the file, so a transaction or a
setting it leaves on its connection does not reach the next call.

Every other command, and every call that passes `key` or `root`, starts one `inillucent` process
with `--output json` and parses the object it prints. `params` travels to the process on standard
input, so a large value does not hit the operating system's limit on command line length. Starting
a process costs about 18 ms on Windows. Set `INILLUCENT_NO_ADDON=1` to send every call through a
process.

### Many calls: a session

```js
import { open } from 'inillucent';

const session = await open('app.rdb');
for (const id of ids) {
  const rows = await session.query('SELECT body FROM notes WHERE id = ?1', { params: [id] });
}
await session.close();
```

`open()` keeps the file open in this process through the C library, so a call costs what the
statement costs: 0.007 ms for a lookup by key on Windows, against 0.014 ms for Node's own
`node:sqlite`. `session.inillucent(command, options)` and `session.query(sql, options)` return
what `inillucent()` and `query()` return. A command the library does not run, such as `describe`,
goes to an `inillucent-mcp` process the session starts the first time it needs one. `open()` makes
the file when it is missing, unless you pass `create: false`. It also takes `key`, `readonly` and
`root`; with `key` or `root`, the whole session runs through one `inillucent-mcp` process, as it
does where the library is missing.

A handle from `open()` belongs to the thread that opened it. A worker thread opens its own.

A call through `inillucent()` is its own process, and a call through a session is its own
statement, so neither has a transaction that spans two calls. To run several statements as one
transaction, use the `batch` command: `inillucent('batch', { db, sql: 'INSERT ...; UPDATE ...' })`.

### Values

| JavaScript value | Bound as |
|---|---|
| `null` or `undefined` | `NULL` |
| a number | `INTEGER` or `REAL`. `-0` keeps its sign |
| `NaN`, `Infinity` | refused with a `TypeError`, because SQL has no value for them |
| a string | `TEXT` |
| a `Uint8Array` | `BLOB` |

`query()` returns a BLOB column as a `Uint8Array`, so bytes read by one query can be bound into the
next. Through the C library an integer beyond 2^53 comes back as a `BigInt`, so it is exact; through
a process it is a number, which JSON rounds.

## Encrypted databases

Pass `key` beside `db`:

```js
const key = `x'${'5a'.repeat(32)}'`;
await inillucent('exec', { db: 'app.rdb', key, sql: 'CREATE TABLE note (body TEXT)' });
const rows = await query('SELECT body FROM note', { db: 'app.rdb', key });
```

The key travels to the program in the `INILLUCENT_KEY` environment variable. It is never put on the command line, because a command line is visible in the process list. When you pass no key, the environment the program inherits is left alone.

A key written `x'` followed by 64 hexadecimal digits and `'` is a raw 32 byte key. Any other text is a passphrase, which is stretched with 600,000 rounds of PBKDF2 on every open, so a raw key opens faster.

A database created with a key is encrypted. Opening it without the key, or with a wrong key, fails with status `corrupt`. Opening a plaintext database with a key fails the same way.

A call without `key` on an encrypted database returns `{ ok: false, status: 'corrupt' }`.

## Errors

```js
const result = await inillucent('query', { db: 'app.rdb', sql: 'SELECT * FROM absent' });
// result.ok === false, result.status === 'not_found', result.message === 'no such table: absent'

try {
  await query('SELECT * FROM absent', { db: 'app.rdb' });
} catch (error) {
  console.log(error.status); // 'not_found'
}
```

`inillucent()` returns a refusal as its result object with `ok: false`. `query()` throws an `Error`
with three extra fields: `status`, `message` and `feature`. Both throw only when the program could
not be run at all.

`status` is one of thirteen names: `unsupported`, `syntax`, `not_found`, `constraint`, `readonly`,
`busy`, `interrupted`, `corrupt`, `io`, `full`, `too_big`, `invalid_state` and `internal`.

`unsupported` means the engine has not built that feature, and `feature` names it. The SQL is not
wrong, and a different spelling fails the same way. Check `inillucent capabilities` before you
write an unusual statement.

## For an AI agent

MCP, the Model Context Protocol, is how an AI agent calls tools. Add `inillucent-mcp` to an MCP
client's configuration:

```json
{
  "mcpServers": {
    "inillucent": {
      "command": "npx",
      "args": ["-y", "-p", "inillucent", "inillucent-mcp", "--db", "app.rdb"]
    }
  }
}
```

`inillucent-mcp` serves 29 of the command line's commands as MCP tools. The tools are generated from
the same command table as the command line. `--readonly` refuses every statement that changes data.
`--root DIR` refuses every path outside `DIR`.

## The API

`cargo test -p inillucent-compat --test tooling documentation::` reads this table and fails if a name in the
first column is not declared in `index.mjs` or `resolve.mjs`.

| what | one line |
|---|---|
| `inillucent(command, options)` | runs one command and resolves to its parsed JSON result. `options.db` names the file. Every other key becomes a flag: `{ table: 'notes' }` is `--table notes`, `true` is a bare flag. |
| `query(sql, options)` | runs one query and resolves to its rows as objects keyed by column name. `options` takes `db`, `params` and `limit`. Throws on a refusal. |
| `open(db, options)` | opens the file in this process through the C library, or starts one `inillucent-mcp` process over it when the library is missing or `key` or `root` is given, and resolves to a session. `options` takes `key`, `readonly`, `root` and `create`. |
| `Session` | a session over one `inillucent-mcp` process: `inillucent(command, options)` and `query(sql, options)` work as the functions above without `db`, and `close()` stops the process. |
| `NativeSession` | a session in this process through the C library, with the same three methods. |
| `resolveBinary(program)` | returns the path to one of the four programs on this machine. `INILLUCENT_BIN` overrides it. |
| `resolveLibrary()` | returns the path to the C library on this machine, or `null` when there is none. With `INILLUCENT_BIN` set, it is looked for beside that binary. |
| `platformPackage()` | returns the platform package this machine needs, or `null` when there is none. |
| `PROGRAMS` | an object naming the four programs. |

A result object from `inillucent()` has these fields on success: `ok`, `command`, `columns`, `rows`,
`row_count`, `total`, `more`, `changes`, `last_insert_rowid`, `elapsed_ms` and, from a process,
`text`. Some commands
add their own, such as `ddl` and `indexes` from `describe`. `total` counts every row the statement
produced, even when `limit` cut the rows returned.

The `query` command returns at most 200 rows unless you pass `limit`. `limit: 0` returns every row.
`more: true` in the result means rows were left out.

`INILLUCENT_BIN` names an `inillucent` binary to use in place of the platform package. The other
three programs are then looked for in the same folder.

## More

- [Getting started](https://github.com/Black-Rainbow-Labs/Inillucent/blob/main/docs/getting-started.md)
- [SQL support](https://github.com/Black-Rainbow-Labs/Inillucent/blob/main/docs/sql.md)
- [Vector and keyword search](https://github.com/Black-Rainbow-Labs/Inillucent/blob/main/docs/vector-search.md)
- [Glossary](https://github.com/Black-Rainbow-Labs/Inillucent/blob/main/docs/glossary.md)

MIT licence. Source: <https://github.com/Black-Rainbow-Labs/Inillucent>
