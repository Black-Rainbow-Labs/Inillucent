// The inillucent Android library: cargo builds the C library for each ABI, CMake links it with the
// JNI glue into libinillucent_jni.so, and the Kotlin classes in src/main/kotlin call that.
//
// Nothing here needs an environment variable. The NDK comes from the Android Gradle plugin's own
// `ndkVersion`, and its clang is handed to cargo as the linker and as CC for any C a build script
// compiles. CARGO_TARGET_DIR, when set, is where cargo writes, as it is for any cargo build.

import java.io.File

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

/** The repository root: this file is packages/android/inillucent/build.gradle.kts. */
val repositoryRoot: File = rootProject.projectDir.parentFile.parentFile

/** The ABIs to build, from -PinillucentAbis or gradle.properties. */
val abis: List<String> = (findProperty("inillucentAbis") as String).split(",").map { it.trim() }.filter { it.isNotEmpty() }

/** Each ABI's Rust target and the NDK clang that links for it at API 24. */
val rustTargets = mapOf(
    "arm64-v8a" to ("aarch64-linux-android" to "aarch64-linux-android24"),
    "armeabi-v7a" to ("armv7-linux-androideabi" to "armv7a-linux-androideabi24"),
    "x86_64" to ("x86_64-linux-android" to "x86_64-linux-android24"),
)

/** Where cargo writes. */
val cargoTargetDir: String = System.getenv("CARGO_TARGET_DIR")?.takeIf { it.isNotBlank() } ?: File(repositoryRoot, "target").path

android {
    namespace = "com.blackrainbowlabs.inillucent"
    compileSdk = 35
    ndkVersion = "27.1.12297006"

    defaultConfig {
        minSdk = 24
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
        ndk { abiFilters += abis }
        externalNativeBuild {
            cmake {
                arguments += listOf(
                    "-DINILLUCENT_RUST_TARGET_ROOT=${cargoTargetDir.replace('\\', '/')}",
                    "-DINILLUCENT_INCLUDE_DIR=${File(repositoryRoot, "drivers/inillucent-driver-capi/include").path.replace('\\', '/')}",
                )
            }
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    // The conformance suite is read from the one copy in the repository, packed into the test APK.
    sourceSets["androidTest"].assets.srcDirs(File(repositoryRoot, "drivers/conformance"))

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        // Keep the library's symbols for a readable native crash; the release build already strips.
        jniLibs.keepDebugSymbols += "**/libinillucent_jni.so"
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("junit:junit:4.13.2")
}

/**
 * The NDK's prebuilt toolchain folder for the machine running the build.
 * @param ndk - the NDK root
 */
fun ndkToolchain(ndk: File): File {
    val os = System.getProperty("os.name").lowercase()
    val host = when {
        os.contains("windows") -> "windows-x86_64"
        os.contains("mac") -> "darwin-x86_64"
        else -> "linux-x86_64"
    }
    return File(ndk, "toolchains/llvm/prebuilt/$host/bin")
}

/** One cargo build of the C library per ABI, which the CMake configure step for that ABI waits on. */
val cargoTasks = abis.map { abi ->
    val (triple, clang) = rustTargets[abi] ?: error("no Rust target for the ABI $abi")
    tasks.register<Exec>("cargoBuild-$abi") {
        group = "build"
        description = "cargo builds inillucent-driver-capi for $triple"
        workingDir = repositoryRoot
        val windows = System.getProperty("os.name").lowercase().contains("windows")
        doFirst {
            val bin = ndkToolchain(android.ndkDirectory)
            val linker = File(bin, if (windows) "$clang-clang.cmd" else "$clang-clang")
            val upper = triple.uppercase().replace('-', '_')
            val lower = triple.replace('-', '_')
            environment("CARGO_TARGET_${upper}_LINKER", linker.path)
            environment("CC_$lower", linker.path)
            environment("AR_$lower", File(bin, if (windows) "llvm-ar.exe" else "llvm-ar").path)
            environment("CARGO_TARGET_DIR", cargoTargetDir)
        }
        commandLine("cargo", "build", "-p", "inillucent-driver-capi", "--release", "--target", triple, "--locked")
    }
}

tasks.configureEach {
    if (name.startsWith("configureCMake") || name.startsWith("buildCMake")) dependsOn(cargoTasks)
}
