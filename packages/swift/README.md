# inillucent for iOS and macOS

This folder is a Swift package that puts the inillucent engine inside an iOS or macOS app. The
engine runs in the app's own process, and the database is a file in the app's container.

```swift
import Inillucent

let folder = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
let db = try Database(url: folder.appendingPathComponent("app.rdb"))
let conn = try db.session()
try conn.execute("CREATE TABLE IF NOT EXISTS note (id INTEGER PRIMARY KEY, body TEXT)")
try conn.execute("INSERT INTO note (body) VALUES (?1)", ["hello"])
for row in try conn.query("SELECT id, body FROM note") {
    print(row[0], row[1])
}
```

## Terms used on this page

| Term | Meaning |
|---|---|
| XCFramework | A folder Xcode reads that holds one library for each Apple platform, with its header. |
| slice | The library for one platform inside the XCFramework. |
| simulator | The iOS simulator Xcode runs on a Mac. It is a platform of its own, with its own slice. |

## Build it

You need a Mac with Xcode, and Rust:

```sh
sh packages/swift/build-xcframework.sh
```

The script builds the C library five times with `cargo build -p inillucent-driver-capi --release`,
once for each Apple target. It joins the two simulator builds and the two Mac builds with `lipo`,
and writes `packages/swift/InillucentFFI.xcframework` with three slices:

| Slice | Rust targets |
|---|---|
| `ios-arm64` | `aarch64-apple-ios` |
| `ios-arm64_x86_64-simulator` | `aarch64-apple-ios-sim`, `x86_64-apple-ios` |
| `macos-arm64_x86_64` | `aarch64-apple-darwin`, `x86_64-apple-darwin` |

Then add the package to an app with Xcode's "Add Package Dependencies" and its "Add Local" button,
or with a path dependency in another package:

```swift
.package(path: "../inillucent/packages/swift")
```

The XCFramework is built from source and is not checked in. A release that publishes it with a
checksum, so that a package can name it by URL, does not exist yet.

The minimum versions are iOS 13 and macOS 11. The package carries a privacy manifest. It declares
no tracking and no collected data, and it declares the file timestamp API with reason `C617.1`,
because the engine reads the times of its own files with `fstat`.

## Values

`Value` is an enum with five cases: `.null`, `.integer(Int64)`, `.real(Double)`, `.text(String)`
and `.blob(Data)`. It accepts literals, so `[1, "Ada", nil, 2.5, true]` is a list of parameters. A
`true` is bound as 1.

Two `.text` values are equal when their UTF-8 bytes are equal. Swift's `String ==` treats "é"
written as one character and "e" followed by a combining accent as equal. The engine stores them as
two different values, so `Value` compares the bytes.

## Errors

Every failure throws `InillucentError`. Its `status` is one of the names the other bindings use,
such as `.syntax`, `.notFound` or `.constraint`. When the engine has not built a construct,
`isUnsupported` is true and `feature` names the construct:

```swift
do {
    try conn.query(sql)
} catch let error as InillucentError where error.isUnsupported {
    show("This query uses \(error.feature ?? "a construct"), which inillucent does not support yet")
}
```

A statement that fails part way through hands back the rows it produced before the failure in
`partialRows`.

## Threads

The engine is single threaded for each file, and the C library has no lock. So every `Database`
holds an `NSLock`, and every call on it, on its connections, on its statements and on its
transactions takes that lock. You can call the library from any thread or task, and two calls on
one database run one after the other.

`Connection.cancel()` does not take the lock. Call it from another thread to stop a long statement.
The statement then fails with `.interrupted`.

## Transactions

```swift
try conn.transaction { txn in
    try txn.execute("UPDATE account SET balance = balance - 10 WHERE id = 1")
    try txn.execute("UPDATE account SET balance = balance + 10 WHERE id = 2")
}
```

`transaction` commits when the closure returns and rolls back when the closure throws.

## Encryption

```swift
let db = try Database(url: file, key: passphrase)
```

The file and its log are encrypted with XChaCha20-Poly1305. A key of the form `x'<64 hex digits>'`
is used as the raw key. Any other key is a passphrase. Opening the file with the wrong key, or with no
key, throws `InillucentError` with `.corrupt`. Keep the key in the Keychain, and do not put it in
the app's code.

## Tests

```sh
sh packages/swift/run-tests.sh
```

The script builds the XCFramework, then runs the tests twice: with `swift test` on the Mac, and with
`xcodebuild test` on the newest iPhone simulator Xcode has.

| Test | What it checks |
|---|---|
| `ConformanceTests` | every case of `drivers/conformance/suite.json`, the suite that every binding passes |
| `DatabaseTests` | an encrypted file, a file opened again, a transaction rolled back by a throw, text with an emoji and a NUL byte, composed and decomposed text kept as two values, a cancel from another thread, the rows before a failure, and a close refused while a connection is open |

The conformance records are written to `_agent_output/conformance/swift-macos.json` and
`_agent_output/conformance/ios.json`. On anything that is not a Mac with Xcode, the script prints a
line that ends in `; skipping`. `.github/workflows/mobile.yml` runs it on a GitHub macOS runner.
