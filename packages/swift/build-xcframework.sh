#!/bin/sh
# Builds packages/swift/InillucentFFI.xcframework: the C library as a static library for the iPhone,
# the iOS simulator and the Mac, with the header and a module map Swift imports it by.
#
#   sh packages/swift/build-xcframework.sh
#
# Needs a Mac with Xcode, because xcodebuild and lipo are Apple's. The five Rust targets are added
# to the toolchain rust-toolchain.toml pins if they are missing.
#
# An XCFramework holds one library per platform, not one per processor, so the two simulator builds
# are joined with lipo first, and so are the two Mac builds:
#
#   ios-arm64                    aarch64-apple-ios
#   ios-arm64_x86_64-simulator   aarch64-apple-ios-sim + x86_64-apple-ios
#   macos-arm64_x86_64           aarch64-apple-darwin + x86_64-apple-darwin
#
# Environment it reads, all optional: CARGO_TARGET_DIR, and INILLUCENT_APPLE_PLATFORMS, the slices to
# build out of "ios simulator macos" (the default is all three). An iPhone app that uses only the C
# library, such as the Meditation app's React Native bridge, sets it to "ios simulator" and skips the
# two Mac builds.

set -e

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="$HERE/InillucentFFI.xcframework"
STAGE="$TARGET_DIR/swift-stage"
LIB=libinillucent_driver_capi.a

if [ "$(uname -s)" != "Darwin" ]; then
  echo "building an XCFramework needs a Mac with Xcode" >&2
  exit 1
fi

# The oldest systems the package declares in Package.swift. Without these, rustc builds for its own
# defaults and the linker warns on every object when an app with a lower target links it.
export IPHONEOS_DEPLOYMENT_TARGET=13.0
export MACOSX_DEPLOYMENT_TARGET=11.0

PLATFORMS="${INILLUCENT_APPLE_PLATFORMS:-ios simulator macos}"
TARGETS=""
for platform in $PLATFORMS; do
  case "$platform" in
    ios) TARGETS="$TARGETS aarch64-apple-ios" ;;
    simulator) TARGETS="$TARGETS aarch64-apple-ios-sim x86_64-apple-ios" ;;
    macos) TARGETS="$TARGETS aarch64-apple-darwin x86_64-apple-darwin" ;;
    *) echo "INILLUCENT_APPLE_PLATFORMS: unknown platform '$platform'; use ios, simulator and macos" >&2; exit 1 ;;
  esac
done
(cd "$ROOT" && rustup target add $TARGETS >/dev/null)
for target in $TARGETS; do
  echo "cargo build --target $target"
  # From the repository root, so rustup reads rust-toolchain.toml and builds with the toolchain the
  # targets were just added to. Run from anywhere else, cargo used the caller's default toolchain,
  # which may not have the Apple targets (task-2445, from the Meditation app's checkout).
  (cd "$ROOT" && cargo build -p inillucent-driver-capi --release --locked --target "$target")
done

rm -rf "$STAGE" "$OUT"
mkdir -p "$STAGE/ios" "$STAGE/simulator" "$STAGE/macos" "$STAGE/headers"
set --
for platform in $PLATFORMS; do
  case "$platform" in
    ios) cp "$TARGET_DIR/aarch64-apple-ios/release/$LIB" "$STAGE/ios/$LIB" ;;
    simulator) lipo -create "$TARGET_DIR/aarch64-apple-ios-sim/release/$LIB" "$TARGET_DIR/x86_64-apple-ios/release/$LIB" -output "$STAGE/simulator/$LIB" ;;
    macos) lipo -create "$TARGET_DIR/aarch64-apple-darwin/release/$LIB" "$TARGET_DIR/x86_64-apple-darwin/release/$LIB" -output "$STAGE/macos/$LIB" ;;
  esac
  set -- "$@" -library "$STAGE/$platform/$LIB" -headers "$STAGE/headers"
done

cp "$ROOT/drivers/inillucent-driver-capi/include/inillucent_driver.h" "$STAGE/headers/"
cat >"$STAGE/headers/module.modulemap" <<'EOF'
module InillucentFFI {
    header "inillucent_driver.h"
    export *
}
EOF

# "$@" holds one -library and -headers pair for each platform built above.
xcodebuild -create-xcframework "$@" -output "$OUT"

echo "wrote $OUT"
ls "$OUT"
