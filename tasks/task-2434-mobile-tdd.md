# inillucent on Android and iOS: technical design

## 1. What this adds

inillucent runs on Windows, Linux and macOS today. An application on a phone cannot use it, because
nothing builds the engine for a phone and no binding exists in the languages phone applications are
written in. This design adds:

| Platform | Package | Language | How it reaches the engine |
|---|---|---|---|
| Android | `packages/android`, a Gradle library that builds an AAR | Kotlin | JNI glue in C over the C library, linked statically into one `libinillucent_jni.so` per ABI |
| iOS | `packages/swift`, a Swift package | Swift | the C library as a static library inside `InillucentFFI.xcframework`, imported as a Clang module |

Both packages run the driver's conformance suite (`drivers/conformance/suite.json`) on a real
device runtime: the Android emulator for Android, the iOS simulator for iOS. That suite is what
"the binding is correct" means in this repository, so both new bindings are graded by the same 35
cases that grade the Rust driver and the Python binding.

## 2. Terms

| Term | Meaning |
|---|---|
| C library | `drivers/inillucent-driver-capi`, built as a `cdylib` and a `staticlib`. |
| ABI (Android) | An Android processor family: `arm64-v8a`, `armeabi-v7a`, `x86_64`. |
| AAR | An Android library archive: classes plus one native library per ABI. |
| JNI | The Java Native Interface. A C function named `Java_<package>_<class>_<method>` is what a Kotlin `external fun` calls. |
| XCFramework | A folder Xcode reads that holds one library per Apple platform (device, simulator, macOS) with its headers. |
| slice | One platform's library inside an XCFramework. |

## 3. Research

What other Rust projects shipping to phones do, and the platform rules that apply in 2026:

1. **Android 16 KB pages.** Android 15 devices can run with 16 KB memory pages, and Google Play
   requires every native library in an app that targets API 35 to have its ELF segments aligned to
   16 KB. NDK r28 aligns by default. This machine has NDK r27, which does not, so every Android link
   passes `-Wl,-z,max-page-size=16384`. The check is `llvm-readelf -l` on the library: every `LOAD`
   segment's alignment must be `0x4000`. The test script checks it on every build.
2. **Rust targets.** `aarch64-linux-android`, `armv7-linux-androideabi` and `x86_64-linux-android`
   cover every phone sold and the emulator on an x86 machine. 32 bit x86 Android is not built: no
   device has shipped with it in years and the emulator images for it are gone from API 30.
3. **One shared library or two.** Mozilla's application services and most cargo ndk projects load
   the Rust `cdylib` and a separate JNI library. Linking the Rust `staticlib` into the JNI library
   instead gives one `.so`, one `System.loadLibrary` call, a proper `DT_SONAME` (the Rust `cdylib`
   for Android has none) and lets the linker drop the C ABI functions nothing calls
   (`--gc-sections`). This design links statically.
4. **JNI and strings.** `NewStringUTF` reads "modified UTF-8", which encodes a character outside the
   Basic Multilingual Plane differently from the UTF-8 the engine stores, and refuses an embedded NUL
   byte. The suite has a NUL byte inside text and an emoji in a value. So every string crosses JNI as
   a `byte[]` of real UTF-8 and is decoded in Kotlin. This is the bug a JNI binding most often has.
5. **iOS packaging.** An XCFramework holds one binary per platform, not per processor, so the two
   simulator slices (`aarch64-apple-ios-sim`, `x86_64-apple-ios`) are joined with `lipo` first, the
   same for the two macOS slices. `xcodebuild -create-xcframework -library <lib> -headers <dir>`
   writes it; the headers folder holds `inillucent_driver.h` and a `module.modulemap` so Swift can
   `import InillucentFFI`. A Swift package names it with `.binaryTarget(path:)`.
6. **Privacy manifest.** Apple asks every SDK shipped as an XCFramework or a Swift package to carry a
   `PrivacyInfo.xcprivacy`. The engine collects nothing and tracks nothing. It does read file
   timestamps (`fstat` for the log segments), which is one of Apple's "required reason" APIs, with
   reason `C617.1`: files inside the app's own container. The Swift package ships that manifest as a
   resource.
7. **Where iOS can be tested.** Only a Mac runs the iOS simulator. This machine is Windows. GitHub
   gives a public repository macOS runners with Xcode at no charge, so the iOS build and its tests
   run in `.github/workflows/mobile.yml` on `macos-latest`. The Android tests run here on the
   emulator, and in the same workflow on `ubuntu-latest`, where `/dev/kvm` can be enabled with a
   udev rule and the emulator runs accelerated.

## 4. The Android package

```
packages/android/
  settings.gradle.kts, build.gradle.kts, gradle.properties, gradlew, gradle/wrapper/
  run-tests.sh                     the package test row: build, start the emulator, run, pull record
  inillucent/
    build.gradle.kts               the library: cargo per ABI, then CMake, then Kotlin
    src/main/cpp/CMakeLists.txt
    src/main/cpp/inillucent_jni.c  the JNI glue
    src/main/kotlin/com/blackrainbowlabs/inillucent/*.kt
    src/androidTest/kotlin/com/blackrainbowlabs/inillucent/ConformanceTest.kt
    src/androidTest/kotlin/com/blackrainbowlabs/inillucent/DatabaseTest.kt
```

**The build.** A Gradle task per ABI runs
`cargo build -p inillucent-driver-capi --release --target <triple>` with the NDK's clang as linker
and `CC`, then CMake links `libinillucent_driver_capi.a` with `inillucent_jni.c` into
`libinillucent_jni.so`. The NDK path comes from the Android Gradle plugin, so nobody sets an
environment variable. `-PinillucentAbis=x86_64` builds one ABI, which is what the emulator test uses
so a test run does not compile three release builds.

**The Kotlin API** follows the Python reference binding, in Kotlin's own forms:

```kotlin
Database.open(path, readOnly = false, key = null, busyTimeoutMs = null).use { db ->
    db.session().use { conn ->
        conn.execute("CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)")
        conn.execute("INSERT INTO people VALUES (?1, ?2)", 1, "Ada")
        val rows = conn.query("SELECT id, name FROM people")      // Rows
        rows.columns; rows.rows; rows.total; rows.more; rows.affected; rows.tag
        conn.transaction { txn -> txn.execute("UPDATE ...") }       // commits, or rolls back on throw
        conn.prepare("INSERT INTO people (name) VALUES (?1)").use { st -> st.execute("Grace") }
    }
}
```

| Engine value | Kotlin value |
|---|---|
| NULL | `null` |
| integer | `Long` |
| real | `Double` |
| text | `String` |
| blob | `ByteArray` |

A parameter may also be `Int`, `Short`, `Byte`, `Float` or `Boolean` (bound as 1 and 0). Anything
else is an `IllegalArgumentException` before the statement runs.

Every failure is an `InillucentException` with `status` (an enum named as the suite names them),
`message`, `feature`, `detail` and `offset`. `UnsupportedException` is its subclass for
`unsupported`, so `catch (e: UnsupportedException)` is the "not built yet" branch drivers/README.md
asks every application to write.

**Threads.** The engine is single threaded per file and the C library has no lock. Android code calls
from many threads, so `Database` holds one lock and every call on it, its connections, statements and
transactions takes that lock. `Connection.cancel()` does not take it, because it is the one call the
C library allows from another thread, and it would wait behind the statement it is meant to stop.

**The JNI glue** is about 400 lines of C. It owns no state. Handles cross as `long`. A result is read
in one JNI call that returns the column names, types, every cell and the counts, and is freed before
the call returns, so Kotlin never holds a `inillucent_rows`. A failure is thrown as a Java exception
built by a static Kotlin factory, so the exception class and its fields are decided in Kotlin.

## 5. The Swift package

```
packages/swift/
  Package.swift
  build-xcframework.sh             cargo for five Apple targets, lipo, xcodebuild -create-xcframework
  run-tests.sh                     the package test row: build, swift test, xcodebuild test on a simulator
  Sources/Inillucent/*.swift
  Sources/Inillucent/PrivacyInfo.xcprivacy
  Tests/InillucentTests/ConformanceTests.swift
  Tests/InillucentTests/DatabaseTests.swift
```

The XCFramework has three slices: `ios-arm64`, `ios-arm64_x86_64-simulator` and `macos-arm64_x86_64`.
The macOS slice lets `swift test` run on the Mac itself, which is the fast check; `xcodebuild test
-destination 'platform=iOS Simulator,...'` is the check that matters.

```swift
let db = try Database(path: path)
let conn = try db.session()
try conn.execute("INSERT INTO people VALUES (?1, ?2)", [1, "Ada"])
let rows = try conn.query("SELECT id, name FROM people")
for row in rows.rows { if case .text(let name) = row[1] { print(name) } }
```

`Value` is an enum (`.null`, `.integer(Int64)`, `.real(Double)`, `.text(String)`, `.blob(Data)`) that
conforms to the literal protocols, so `[1, "Ada", nil]` is a parameter list. A failure is a thrown
`InillucentError` with the same fields as Kotlin's, and `error.isUnsupported`. `Database` holds an
`NSLock` for the same reason as Kotlin.

`Value` compares `.text` by its UTF-8 bytes. Swift's `String ==` uses canonical equivalence, so a
composed and a decomposed accented letter are equal Strings while the engine stores two values, and
the suite's `composed_and_decomposed_text_stay_two_values` case could not fail under the synthesized
`==`.

## 6. Tests

| Test | Where it runs | What it proves |
|---|---|---|
| `ConformanceTest.kt` | Android emulator, x86_64, API 36, here and on ubuntu in CI | all 35 suite cases through Kotlin and JNI; the record lists every case |
| `DatabaseTest.kt` | same | encryption with a key and refusal without it; a file reopened after close; a transaction rolled back on a throw; text with an emoji and a NUL byte round trips; capabilities read; cancel from a second thread; partial rows on a failure |
| `ConformanceTests.swift` | iOS simulator in CI, and macOS through `swift test` | all 35 suite cases through Swift |
| `DatabaseTests.swift` | same | the same list as `DatabaseTest.kt` |
| the 16 KB check in `run-tests.sh` | wherever the Android build runs | every `LOAD` segment of every packaged `.so` is aligned to `0x4000` |

Each conformance runner writes the same record the other runners write (`language`, `lacks`, `ran`,
`skipped`, `failures`), named `android.json`, `ios.json` and (for `swift test` on the Mac) `swift-macos.json`, and fails
itself when `ran` is shorter
than the suite. The Android script pulls the record off the device into
`_agent_output/conformance/`. Both are rows in `packages/package-tests.toml`, and
`bindings::is_a_package_test` learns Kotlin's `*Test.kt` and Swift's `*Tests.swift` names so the rot
check sees them. Each script prints `; skipping` when its platform is absent (no Android SDK, or not a
Mac), which is how `tools/run-package-tests.sh` counts a skip.

The two runners are not added to `RUNNERS` in `bindings.rs`, because that guard runs on every machine
and a phone runtime is not on every machine. The mobile workflow is where they are graded.

## 7. CI

`.github/workflows/mobile.yml`, started by hand, on a schedule beside the nightly, and called from
`tests.yml` so the run `packaging/ci-status.ps1` waits for includes it:

| Job | Runner | Steps |
|---|---|---|
| `android` | `ubuntu-latest` | enable KVM, install the API 36 x86_64 image, `sh packages/android/run-tests.sh` |
| `ios` | `macos-latest` | `sh packages/swift/run-tests.sh` (XCFramework, `swift test`, `xcodebuild test` on a simulator) |

## 8. What implementation found

- **`per_call` cases need one connection.** The first Android run opened a new `Connection` for each
  statement of a `"connection": "per_call"` case, which is a new `inillucent_conn` and a new session,
  and `a_temp_table_survives_a_connection_per_call` failed with `no such table`. The C library
  already continues one session through one handle, so both runners keep one `Connection` per case,
  as the Python runner does.
- **The release archive does not carry the package folders.** `README.md`, `drivers/README.md` and
  `docs/getting-started.md` ship in the archive, and
  `documentation::every_link_in_the_release_archive_resolves` refused links to
  `packages/android/README.md` and `packages/swift/README.md`. Those pages name the paths in
  backticks instead.
- **32 bit ARM compiles.** The engine had never been built for `armv7-linux-androideabi`; it builds
  with no change.
- **Measured sizes** of `libinillucent_jni.so` in the release AAR, unstripped: arm64-v8a 12.4 MB,
  armeabi-v7a 9.6 MB, x86_64 13.3 MB. Every `LOAD` segment is aligned to `0x4000`.

## 9. What is not in this change

- **Publishing to Maven Central and as a Swift package release.** Both need accounts and signing keys
  that do not exist yet (a Sonatype namespace for `com.blackrainbowlabs`, and a release asset URL with
  a checksum for the XCFramework). The packages build from source in the repository today. Adding the
  two routes to `ship.ps1` is a follow up once the credentials exist.
- **The `embed` feature on a phone.** It needs ONNX Runtime built for the phone and a model file in
  the app. Neither package turns it on; `embed()` in SQL returns the engine's usual refusal.
- **A UI.** These are libraries. The Android test APK and the Swift test bundle are the applications
  that exercise them.
