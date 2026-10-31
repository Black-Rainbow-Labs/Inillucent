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
# Environment it reads, all optional: CARGO_TARGET_DIR.

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

TARGETS="aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin"
(cd "$ROOT" && rustup target add $TARGETS >/dev/null)
for target in $TARGETS; do
  echo "cargo build --target $target"
  cargo build --manifest-path "$ROOT/Cargo.toml" -p inillucent-driver-capi --release --locked --target "$target"
done

rm -rf "$STAGE" "$OUT"
mkdir -p "$STAGE/ios" "$STAGE/simulator" "$STAGE/macos" "$STAGE/headers"
cp "$TARGET_DIR/aarch64-apple-ios/release/$LIB" "$STAGE/ios/$LIB"
lipo -create "$TARGET_DIR/aarch64-apple-ios-sim/release/$LIB" "$TARGET_DIR/x86_64-apple-ios/release/$LIB" -output "$STAGE/simulator/$LIB"
lipo -create "$TARGET_DIR/aarch64-apple-darwin/release/$LIB" "$TARGET_DIR/x86_64-apple-darwin/release/$LIB" -output "$STAGE/macos/$LIB"

cp "$ROOT/drivers/inillucent-driver-capi/include/inillucent_driver.h" "$STAGE/headers/"
cat >"$STAGE/headers/module.modulemap" <<'EOF'
module InillucentFFI {
    header "inillucent_driver.h"
    export *
}
EOF

xcodebuild -create-xcframework \
  -library "$STAGE/ios/$LIB" -headers "$STAGE/headers" \
  -library "$STAGE/simulator/$LIB" -headers "$STAGE/headers" \
  -library "$STAGE/macos/$LIB" -headers "$STAGE/headers" \
  -output "$OUT"

echo "wrote $OUT"
ls "$OUT"
