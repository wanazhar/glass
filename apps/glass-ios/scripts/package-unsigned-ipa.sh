#!/usr/bin/env bash
# Builds Glass for iphoneos without signing and zips it into an unsigned .ipa.
# Usage: scripts/package-unsigned-ipa.sh <milestone, e.g. M0>
set -euo pipefail
cd "$(dirname "$0")/.."
milestone="${1:?milestone, e.g. M0}"
derived=build/device
out=build/ipa

xcodebuild -project Glass.xcodeproj -scheme Glass -configuration Release \
  -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath "$derived" \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" build -quiet

app="$derived/Build/Products/Release-iphoneos/Glass.app"
plist="$app/Info.plist"
version=$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$plist")
build=$(/usr/libexec/PlistBuddy -c 'Print CFBundleVersion' "$plist")

fail() { echo "packaging gate failed: $*" >&2; exit 1; }
check() { [[ "$(/usr/libexec/PlistBuddy -c "Print $1" "$plist")" == "$2" ]] || fail "$1 != $2"; }
check CFBundleIdentifier com.wanazhar.glass
check CFBundleDisplayName Glass
check MinimumOSVersion 18.0
/usr/libexec/PlistBuddy -c "Print NSCameraUsageDescription" "$plist" >/dev/null || fail "NSCameraUsageDescription missing"
/usr/libexec/PlistBuddy -c "Print NSLocationWhenInUseUsageDescription" "$plist" >/dev/null || fail "NSLocationWhenInUseUsageDescription missing"
check NSSupportsLiveActivities true
check UIBackgroundModes:0 location
widgets="$app/PlugIns/GlassWidgets.appex"
[[ -d "$widgets" ]] || fail "GlassWidgets.appex missing"
[[ "$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$widgets/Info.plist")" == com.wanazhar.glass.widgets ]] || fail "widget bundle id"
[[ "$(/usr/libexec/PlistBuddy -c 'Print NSExtension:NSExtensionPointIdentifier' "$widgets/Info.plist")" == com.apple.widgetkit-extension ]] || fail "widget extension point"
[[ ! -e "$widgets/_CodeSignature" ]] || fail "widget extension is signed"
[[ -f "$app/Assets.car" ]] || fail "Assets.car missing"
[[ -d "$app/Base.lproj/LaunchScreen.storyboardc" ]] || fail "LaunchScreen.storyboardc missing"
[[ ! -e "$app/embedded.mobileprovision" ]] || fail "embedded.mobileprovision present"
[[ ! -e "$app/_CodeSignature" ]] || fail "bundle is signed"
lipo -info "$app/Glass" | grep -q 'arm64' || fail "binary is not arm64"
if find "$app" -name '*.xctest' -o -name 'Fixtures' | grep -q .; then fail "test content in bundle"; fi

rm -rf "$out" && mkdir -p "$out/Payload"
cp -R "$app" "$out/Payload/"
ipa="Glass-$milestone-$version-$build-unsigned.ipa"
(cd "$out" && ditto -c -k --sequesterRsrc --keepParent Payload "$ipa")
rm -rf "$out/Payload"

echo "ipa: $PWD/$out/$ipa"
echo "version: $version ($build)"
echo "size: $(stat -f %z "$out/$ipa") bytes"
echo "sha256: $(shasum -a 256 "$out/$ipa" | cut -d' ' -f1)"
