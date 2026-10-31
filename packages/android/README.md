# inillucent for Android

This folder is a Gradle library that puts the inillucent engine inside an Android app. The app calls
it from Kotlin or Java. The engine runs in the app's own process, and the database is a file in the
app's storage.

```kotlin
import com.blackrainbowlabs.inillucent.Database

Database.open(File(context.filesDir, "notes/app.rdb")).use { db ->
    db.session().use { conn ->
        conn.execute("CREATE TABLE IF NOT EXISTS note (id INTEGER PRIMARY KEY, body TEXT)")
        conn.execute("INSERT INTO note (body) VALUES (?1)", "hello")
        val rows = conn.query("SELECT id, body FROM note")
        for (row in rows) println("${row[0]}: ${row[1]}")
    }
}
```

## Terms used on this page

| Term | Meaning |
|---|---|
| ABI | An Android processor family. The library is built for `arm64-v8a`, `armeabi-v7a` and `x86_64`. |
| AAR | An Android library archive. It holds the Kotlin classes and one native library for each ABI. |
| JNI | The Java Native Interface. It is how Kotlin calls the C library. |
| NDK | The Android Native Development Kit. Its compiler links the native library. |

## Build it

You need the Android SDK with NDK 27.1.12297006 and CMake 3.22.1, Java 17 or later, and Rust with
the three Android targets:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
sh packages/android/gradlew -p packages/android :inillucent:assembleRelease
```

The AAR is written to `packages/android/inillucent/build/outputs/aar/inillucent-release.aar`. The
build runs `cargo build -p inillucent-driver-capi --release` once for each ABI, with the NDK's clang
as the linker, then links the C library and the JNI code into one `libinillucent_jni.so` for each
ABI. `-PinillucentAbis=arm64-v8a` builds one ABI.

To use the library from an app in another Gradle build, add the AAR as a file dependency, or include
this project with `includeBuild`. The library is not on Maven Central yet.

The minimum Android version is API 24, which is Android 7.0. Every native library is aligned to
16 KB pages, which Google Play requires of an app that targets Android 15.

## Values

| Engine value | Kotlin value |
|---|---|
| NULL | `null` |
| integer | `Long` |
| real | `Double` |
| text | `String` |
| blob | `ByteArray` |

A parameter can also be an `Int`, `Short`, `Byte`, `Float` or `Boolean`. A `Boolean` is bound as 1
or 0. Any other type throws `IllegalArgumentException` before the statement runs.

## Errors

Every failure throws `InillucentException`. Its `status` is one of the names the other bindings use,
such as `SYNTAX`, `NOT_FOUND` or `CONSTRAINT`. When the engine has not built a construct, the
exception is `UnsupportedException`, and its `feature` names the construct. Catch
`UnsupportedException` first:

```kotlin
try {
    conn.query(sql)
} catch (e: UnsupportedException) {
    showMessage("This query uses ${e.feature}, which inillucent does not support yet")
} catch (e: InillucentException) {
    showMessage(e.engineMessage)
}
```

A statement that fails part way through hands back the rows it produced before the failure in
`partialRows`.

## Threads

The engine is single threaded for each file, and the C library has no lock. So every `Database`
holds one lock, and every call on it, on its connections, on its statements and on its transactions
takes that lock. You can call the library from any thread, and two calls on one database run one
after the other.

`Connection.cancel()` does not take the lock. Call it from another thread to stop a long statement.
The statement then fails with `Status.INTERRUPTED`.

## Transactions

```kotlin
conn.transaction { txn ->
    txn.execute("UPDATE account SET balance = balance - 10 WHERE id = 1")
    txn.execute("UPDATE account SET balance = balance + 10 WHERE id = 2")
}
```

`transaction` commits when the block returns and rolls back when the block throws.
`Transaction.affected` lists the rows each statement changed, so a check can run before the commit.

## Encryption

```kotlin
Database.open(file, key = passphrase)
```

The file and its log are encrypted with XChaCha20-Poly1305. A key of the form `x'<64 hex digits>'`
is used as the raw key. Any other key is a passphrase. Opening the file with the wrong key, or with no
key, throws `InillucentException` with `Status.CORRUPT`. Keep the key in the Android Keystore, and do
not put it in the app's code.

## Tests

```sh
sh packages/android/run-tests.sh
```

The script builds the library for `x86_64`, starts an emulator of its own with no window, and runs
the instrumented tests on it:

| Test | What it checks |
|---|---|
| `ConformanceTest` | every case of `drivers/conformance/suite.json`, the suite that every binding passes |
| `DatabaseTest` | an encrypted file, a file opened again, a transaction rolled back by a throw, text with an emoji and a NUL byte, a cancel from another thread, the rows before a failure, and a close refused while a connection is open |

It then checks that every native library it built is aligned to 16 KB, and copies the conformance
record to `_agent_output/conformance/android.json`. It prints a line that ends in `; skipping` when
the machine has no Android SDK, no emulator or no `x86_64` system image.

The emulator is an AVD named `inillucent_test`, made from the newest `x86_64` system image
installed. Set `ANDROID_SERIAL` to run the tests on a device that is already running instead.
`.github/workflows/mobile.yml` runs the same script on Linux.
