#!/usr/bin/env bash
# Design-review screenshots: iPhone SE and Pro Max, light/dark, large Dynamic Type,
# Reduce Transparency and Reduce Motion. Needs the E2E harness running (scripts/e2e/run-e2e.sh
# with GLASS_E2E_HARNESS_ONLY=1). Usage: scripts/screenshots.sh OUT_DIR  (ONLY="se-light ..." to filter)
set -euo pipefail
cd "$(dirname "$0")/.."
out=$(cd "${1:?usage: screenshots.sh OUT_DIR}" && pwd)
control=${GLASS_E2E_CONTROL:-http://127.0.0.1:51999}
ca=${GLASS_E2E_CA:-$HOME/.glass-e2e/tls/ca.pem}
udid() { xcrun simctl list devices available | awk -v n="$1" '/^-- iOS 26/{f=1;next} /^--/{f=0} f && index($0, "    " n " (")==1 {print; exit}' | sed -E 's/.*\(([0-9A-F-]{36})\).*/\1/'; }

variants=(
  "se-light|iPhone SE (3rd generation)|light|large|NO|NO"
  "se-dark|iPhone SE (3rd generation)|dark|large|NO|NO"
  "se-ax-type|iPhone SE (3rd generation)|light|accessibility-large|NO|NO"
  "promax-light|iPhone 17 Pro Max|light|large|NO|NO"
  "promax-dark|iPhone 17 Pro Max|dark|large|NO|NO"
  "promax-reduce-transparency|iPhone 17 Pro Max|dark|large|YES|NO"
  "promax-reduce-motion|iPhone 17 Pro Max|light|large|NO|YES"
)
for variant in "${variants[@]}"; do
  IFS='|' read -r name device appearance size transparency motion <<<"$variant"
  [[ -z ${ONLY:-} || " $ONLY " == *" $name "* ]] || continue
  id=$(udid "$device")
  [[ -n $id ]] || { echo "no simulator named $device" >&2; exit 1; }
  echo "== $name"
  xcrun simctl boot "$id" 2>/dev/null || true
  xcrun simctl bootstatus "$id" -b >/dev/null
  [[ -f $ca ]] && xcrun simctl keychain "$id" add-root-cert "$ca"
  xcrun simctl ui "$id" appearance "$appearance"
  xcrun simctl ui "$id" content_size "$size"
  xcrun simctl spawn "$id" defaults write com.apple.Accessibility EnhancedBackgroundContrastEnabled -bool "$transparency"
  xcrun simctl spawn "$id" defaults write com.apple.Accessibility ReduceMotionEnabled -bool "$motion"
  log="$out/$name.log"
  # xcodebuild on Xcode 26.6 can linger after the suite finishes; stop it once the result is in.
  TEST_RUNNER_GLASS_SHOTS="$out" TEST_RUNNER_GLASS_SHOT_PREFIX="$name" TEST_RUNNER_GLASS_E2E_CONTROL="$control" \
    xcodebuild test -project Glass.xcodeproj -scheme Glass -collect-test-diagnostics never -destination "platform=iOS Simulator,id=$id" \
    -derivedDataPath build/shots -only-testing:GlassUITests/ScreenshotTourUITests >"$log" 2>&1 &
  pid=$!
  until grep -qE "^Test Suite 'Selected tests' (passed|failed)|^\*\* TEST" "$log" || ! kill -0 "$pid" 2>/dev/null; do sleep 2; done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  grep -E "^Test Case .*(passed|failed)|error:" "$log" || true
  xcrun simctl spawn "$id" defaults write com.apple.Accessibility EnhancedBackgroundContrastEnabled -bool NO
  xcrun simctl spawn "$id" defaults write com.apple.Accessibility ReduceMotionEnabled -bool NO
  xcrun simctl ui "$id" content_size large
done
echo done
ls "$out"
