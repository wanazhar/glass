#!/usr/bin/env bash
# Runs the GlassClientKit unit tests and the app's unit and UI tests on Simulators.
# Usage: scripts/test.sh            (iPhone 17 on the newest runtime, plus iOS 18 when installed)
#        DEVICES="iPhone 17" scripts/test.sh
set -euo pipefail
cd "$(dirname "$0")/.."

swift test --package-path ../shared/GlassClientKit

destinations=()
if [[ -n "${DESTINATIONS:-}" ]]; then
  IFS=';' read -ra destinations <<<"$DESTINATIONS"
else
  destinations+=("platform=iOS Simulator,name=iPhone 17,OS=26.5")
  ios18=$(xcrun simctl list devices available | awk '/^-- iOS 18/{f=1;next} /^--/{f=0} f && /iPhone/{print; exit}' \
    | sed -E 's/^ +(.*) \(([0-9A-F-]{36})\).*/\2/')
  [[ -n "$ios18" ]] && destinations+=("platform=iOS Simulator,id=$ios18")
fi

for destination in "${destinations[@]}"; do
  echo "== $destination"
  slug=$(echo "$destination" | tr -c 'A-Za-z0-9' '-')
  rm -rf "build/results/$slug.xcresult"
  xcodebuild test -project Glass.xcodeproj -scheme Glass -collect-test-diagnostics never -destination "$destination" \
    -derivedDataPath build/test -resultBundlePath "build/results/$slug.xcresult" -quiet
done
