#!/bin/sh
# Runs the Swift package's tests on the Mac and on the iOS simulator.
#
#   sh packages/swift/run-tests.sh
#
# 1. Builds InillucentFFI.xcframework (build-xcframework.sh).
# 2. Copies drivers/conformance/suite.json into the test target's resources.
# 3. `swift test` on the Mac: the conformance suite (ConformanceTests) and the behavior tests
#    (DatabaseTests) against the macOS slice.
# 4. `xcodebuild test` on an iPhone simulator, the newest one Xcode has: the same tests against the
#    simulator slice, in the simulator's iOS.
# 5. Writes the conformance records to _agent_output/conformance/swift-macos.json and ios.json.
#
# It prints a line ending in "; skipping" and exits 0 on anything that is not a Mac with Xcode,
# which is how tools/run-package-tests.sh counts a skip. GitHub's macOS runners have Xcode, and
# .github/workflows/mobile.yml runs this there.

set -e

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RECORDS="$ROOT/_agent_output/conformance"

if [ "$(uname -s)" != "Darwin" ] || ! command -v xcodebuild >/dev/null 2>&1; then
  echo "the Swift package's tests need a Mac with Xcode; skipping"
  exit 0
fi

sh "$HERE/build-xcframework.sh"

mkdir -p "$HERE/Tests/InillucentTests/Resources" "$RECORDS"
cp "$ROOT/drivers/conformance/suite.json" "$HERE/Tests/InillucentTests/Resources/suite.json"

echo "swift test on $(sw_vers -productName) $(sw_vers -productVersion)"
INILLUCENT_RECORD_DIR="$RECORDS" swift test --package-path "$HERE"

# The newest iPhone simulator that is available, by its identifier.
device=$(xcrun simctl list devices available --json | python3 -c '
import json, sys
best = None
for runtime, devices in json.load(sys.stdin)["devices"].items():
    if "iOS" not in runtime:
        continue
    version = tuple(int(part) for part in runtime.rsplit("iOS-", 1)[-1].split("-") if part.isdigit())
    for device in devices:
        if device.get("isAvailable") and device["name"].startswith("iPhone"):
            if best is None or version > best[0]:
                best = (version, device["udid"], device["name"])
if best:
    print(best[1] + " " + best[2] + " iOS " + ".".join(map(str, best[0])))
')
if [ -z "$device" ]; then
  echo "Xcode has no available iPhone simulator" >&2
  exit 1
fi
udid=${device%% *}
echo "xcodebuild test on $device"

# The scheme a package's tests run under is named for the package when it has one product.
scheme=Inillucent
if (cd "$HERE" && xcodebuild -list 2>/dev/null) | grep -q 'Inillucent-Package'; then
  scheme=Inillucent-Package
fi

# A TEST_RUNNER_ prefix is how xcodebuild passes a variable into the test process on the simulator,
# and a simulator can write to the Mac's file system, so the record lands beside the others.
(cd "$HERE" && TEST_RUNNER_INILLUCENT_RECORD_DIR="$RECORDS" xcodebuild test \
  -scheme "$scheme" \
  -destination "platform=iOS Simulator,id=$udid" \
  -derivedDataPath "$ROOT/target/swift-derived-data" \
  CODE_SIGNING_ALLOWED=NO)

for record in swift-macos ios; do
  if [ ! -f "$RECORDS/$record.json" ]; then
    echo "the $record run wrote no conformance record" >&2
    exit 1
  fi
done
echo "conformance records: $RECORDS/swift-macos.json, $RECORDS/ios.json"
