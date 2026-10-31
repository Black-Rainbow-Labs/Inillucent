#!/bin/sh
# Runs the Android library's instrumented tests on an emulator, from a clean machine state.
#
#   sh packages/android/run-tests.sh
#
# 1. Builds the C library for x86_64 Android with cargo, links it with the JNI glue, and builds the
#    library and its test APK, through Gradle.
# 2. Starts an emulator of its own, headless, on console port 5584, unless ANDROID_SERIAL names a
#    device that is already running. Its own AVD is `inillucent_test`, made from the newest x86_64
#    system image installed, so the AVDs a person uses are never touched.
# 3. Runs `connectedDebugAndroidTest`: the conformance suite (ConformanceTest) and the behavior tests
#    (DatabaseTest), on the device.
# 4. Checks that every LOAD segment of every libinillucent_jni.so it built is aligned to 16 KB, the
#    page size Google Play requires of an app that targets Android 15.
# 5. Pulls the conformance record off the device into _agent_output/conformance/android.json.
# 6. Stops the emulator it started.
#
# It prints a line ending in "; skipping" and exits 0 when the machine has no Android SDK, no
# emulator or no x86_64 system image, which is how tools/run-package-tests.sh counts a skip.
#
# Environment it reads, all optional: ANDROID_HOME or ANDROID_SDK_ROOT, JAVA_HOME (17 or later),
# ANDROID_SERIAL, CARGO_TARGET_DIR, INILLUCENT_EMULATOR_FLAGS (extra flags for the emulator).

set -e

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
SDK="${ANDROID_HOME:-$ANDROID_SDK_ROOT}"
PACKAGE=com.blackrainbowlabs.inillucent.test

if [ -z "$SDK" ] || [ ! -d "$SDK" ]; then
  echo "no Android SDK (set ANDROID_HOME); skipping"
  exit 0
fi

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) EXE=.exe; BAT=.bat ;;
  *) EXE=; BAT= ;;
esac
ADB="$SDK/platform-tools/adb$EXE"
EMULATOR="$SDK/emulator/emulator$EXE"
AVDMANAGER="$SDK/cmdline-tools/latest/bin/avdmanager$BAT"

if [ ! -x "$ADB" ] || [ ! -x "$EMULATOR" ]; then
  echo "the Android SDK at $SDK has no adb or no emulator; skipping"
  exit 0
fi

# In the repository, so rustup adds it to the toolchain rust-toolchain.toml pins.
(cd "$ROOT" && rustup target add x86_64-linux-android >/dev/null)

started=""
cleanup() {
  if [ -n "$started" ]; then
    "$ADB" -s "$started" emu kill >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [ -z "$ANDROID_SERIAL" ]; then
  image=$(ls -d "$SDK"/system-images/android-*/*/x86_64 2>/dev/null | sort -t- -k3 -n | tail -1)
  if [ -z "$image" ]; then
    echo "no x86_64 system image under $SDK/system-images; skipping"
    exit 0
  fi
  # system-images/android-36/google_apis_playstore/x86_64 -> system-images;android-36;google_apis_playstore;x86_64
  # Built from the last three folder names, because on Windows $SDK is a backslash path and the
  # prefix cannot be cut off it reliably.
  tag_dir=$(dirname "$image")
  package="system-images;$(basename "$(dirname "$tag_dir")");$(basename "$tag_dir");x86_64"
  if ! "$EMULATOR" -list-avds | grep -qx inillucent_test; then
    echo "making the AVD inillucent_test from $package"
    echo no | "$AVDMANAGER" create avd -n inillucent_test -k "$package" -d pixel_6 --force >/dev/null
  fi
  ANDROID_SERIAL=emulator-5584
  export ANDROID_SERIAL
  echo "starting inillucent_test as $ANDROID_SERIAL"
  # swiftshader keeps the emulator off the graphics cards, which other work on a machine may be using.
  "$EMULATOR" -avd inillucent_test -port 5584 -no-window -no-audio -no-boot-anim -no-snapshot \
    -gpu swiftshader_indirect $INILLUCENT_EMULATOR_FLAGS >"${TMPDIR:-/tmp}/inillucent-emulator.log" 2>&1 &
  started=$ANDROID_SERIAL
  "$ADB" -s "$ANDROID_SERIAL" wait-for-device
  waited=0
  until [ "$("$ADB" -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
    waited=$((waited + 5))
    if [ "$waited" -gt 600 ]; then
      echo "the emulator did not finish booting in ten minutes" >&2
      tail -20 "${TMPDIR:-/tmp}/inillucent-emulator.log" >&2
      exit 1
    fi
    sleep 5
  done
  echo "booted in about ${waited}s"
fi

# The APKs stay installed after the run so the record can be read off the device below; AGP
# uninstalls them by default.
sh "$HERE/gradlew" -p "$HERE" --no-daemon :inillucent:connectedDebugAndroidTest \
  -PinillucentAbis=x86_64 -Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true

# Every LOAD segment of every library built is aligned to 16 KB.
readelf=$(ls "$SDK"/ndk/27.1.12297006/toolchains/llvm/prebuilt/*/bin/llvm-readelf$EXE | head -1)
checked=0
for library in $(find "$HERE/inillucent/build/intermediates" -name libinillucent_jni.so -path '*merged_native_libs*'); do
  misaligned=$("$readelf" -lW "$library" | awk '$1 == "LOAD" && $NF != "0x4000" { print }')
  if [ -n "$misaligned" ]; then
    echo "$library has a LOAD segment not aligned to 16 KB:" >&2
    echo "$misaligned" >&2
    exit 1
  fi
  checked=$((checked + 1))
done
if [ "$checked" -eq 0 ]; then
  echo "found no packaged libinillucent_jni.so to check for 16 KB alignment" >&2
  exit 1
fi
echo "16 KB aligned: $checked libraries"

mkdir -p "$ROOT/_agent_output/conformance"
"$ADB" -s "$ANDROID_SERIAL" exec-out run-as "$PACKAGE" cat files/android.json >"$ROOT/_agent_output/conformance/android.json"
echo "conformance record: _agent_output/conformance/android.json"
"$ADB" -s "$ANDROID_SERIAL" uninstall "$PACKAGE" >/dev/null 2>&1 || true
