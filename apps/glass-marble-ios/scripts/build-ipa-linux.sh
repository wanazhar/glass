#!/usr/bin/env bash
# Build an unsigned device IPA for Glass Marble on Linux via the local
# elektra-ios Swift SDK (iPhoneOS SDK + Xcode toolchain materials, no Mac).
# The SDK must already be installed (`swift sdk list` shows SWIFT_SDK_ID) with a
# linker that accepts iOS (upstream LLVM ld64.lld) and SwiftUI macro plugins.
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$APP_DIR/../.." && pwd)"
SWIFT_SDK_ID="${SWIFT_SDK_ID:-elektra-ios}"
IOS_SDK="${IOS_SDK:-/srv/elektra/local/iPhoneOS.sdk}"
TOOLCHAIN_LIB="${TOOLCHAIN_LIB:-/srv/elektra/local/materials/toolchain-lib}"
OUT_DIR="${OUT_DIR:-$HOME/work/ipa}"
PRERELEASE="${PRERELEASE:-alpha1}"
MIN_IOS="18.0"
LLVM_OBJDUMP="${LLVM_OBJDUMP:-/usr/local/swift-6.4.2-RELEASE-ubuntu24.04-aarch64/usr/bin/llvm-objdump}"

cd "$APP_DIR"

PBXPROJ="$APP_DIR/GlassMarble.xcodeproj/project.pbxproj"
MARKETING_VERSION="$(grep -m1 'MARKETING_VERSION' "$PBXPROJ" | sed -E 's/.*= *"?([^";]+)"?;.*/\1/')"
CURRENT_PROJECT_VERSION="$(grep -m1 'CURRENT_PROJECT_VERSION' "$PBXPROJ" | sed -E 's/.*= *"?([^";]+)"?;.*/\1/')"
PRODUCT_BUNDLE_IDENTIFIER="$(grep -m1 'PRODUCT_BUNDLE_IDENTIFIER' "$PBXPROJ" | sed -E 's/.*= *"?([^";]+)"?;.*/\1/')"
GIT_SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
SHORT_SHA="$GIT_SHA"
if [ -n "$(git -C "$REPO_ROOT" status --porcelain -- apps/glass-marble-ios)" ]; then
    SHORT_SHA="$SHORT_SHA-dirty"
fi

SDK_VERSION="$(python3 -c "import json;print(json.load(open('$IOS_SDK/SDKSettings.json'))['Version'])")"
SDK_NAME="$(python3 -c "import json;print(json.load(open('$IOS_SDK/SDKSettings.json'))['CanonicalName'])")"
SDK_BUILD="$(python3 -c "import plistlib;print(plistlib.load(open('$IOS_SDK/System/Library/CoreServices/SystemVersion.plist','rb')).get('ProductBuildVersion',''))")"

# Swift 6.4 defaults to the swiftbuild engine, which cannot resolve Apple
# platforms without a host Xcode ('unable to find platform for iphoneos').
# The native build system supports the cross build.
# The extra -platform_version pins the real SDK version into LC_BUILD_VERSION;
# otherwise lld records the deployment target there.
swift build -c release --swift-sdk "$SWIFT_SDK_ID" --build-system native \
    --product GlassMarble \
    -Xlinker -platform_version -Xlinker ios -Xlinker "$MIN_IOS" -Xlinker "$SDK_VERSION"
BIN_PATH="$(swift build -c release --swift-sdk "$SWIFT_SDK_ID" --build-system native --show-bin-path 2>/dev/null | tail -1)/GlassMarble"
[ -f "$BIN_PATH" ] || { echo "error: built binary not found at $BIN_PATH" >&2; exit 1; }

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
APP_BUNDLE="$STAGING/Payload/GlassMarble.app"
mkdir -p "$APP_BUNDLE"
cp "$BIN_PATH" "$APP_BUNDLE/GlassMarble"
chmod 755 "$APP_BUNDLE/GlassMarble"

MARKETING_VERSION="$MARKETING_VERSION" \
CURRENT_PROJECT_VERSION="$CURRENT_PROJECT_VERSION" \
PRODUCT_BUNDLE_IDENTIFIER="$PRODUCT_BUNDLE_IDENTIFIER" \
GIT_SHA="$GIT_SHA" \
MIN_IOS="$MIN_IOS" SDK_VERSION="$SDK_VERSION" SDK_NAME="$SDK_NAME" SDK_BUILD="$SDK_BUILD" \
python3 - "$APP_DIR/Config/Info.plist" "$APP_BUNDLE/Info.plist" <<'PYEOF'
import os, plistlib, sys

src, dst = sys.argv[1], sys.argv[2]
info = plistlib.load(open(src, "rb"))

substitutions = {
    "$(DEVELOPMENT_LANGUAGE)": "en",
    "$(EXECUTABLE_NAME)": "GlassMarble",
    "$(PRODUCT_BUNDLE_IDENTIFIER)": os.environ["PRODUCT_BUNDLE_IDENTIFIER"],
    "$(PRODUCT_NAME)": "Glass Marble",
    "$(PRODUCT_BUNDLE_PACKAGE_TYPE)": "APPL",
    "$(MARKETING_VERSION)": os.environ["MARKETING_VERSION"],
    "$(CURRENT_PROJECT_VERSION)": os.environ["CURRENT_PROJECT_VERSION"],
}

def subst(value):
    if isinstance(value, str):
        for k, v in substitutions.items():
            value = value.replace(k, v)
        return value
    if isinstance(value, list):
        return [subst(v) for v in value]
    if isinstance(value, dict):
        return {subst(k): subst(v) for k, v in value.items()}
    return value

info = subst(info)

def check(value):
    if isinstance(value, str):
        return "$(" in value
    if isinstance(value, list):
        return any(check(v) for v in value)
    if isinstance(value, dict):
        return any(check(k) or check(v) for k, v in value.items())
    return False

if check(info):
    raise SystemExit("error: unsubstituted $(...) variable remains in Info.plist")

# LaunchScreen.storyboard can't be compiled without ibtool; a UIImage-backed
# launch screen replaces it. UIColorName is omitted because it needs a
# compiled asset catalog — the dark style already comes from
# UIUserInterfaceStyle.
info.pop("UILaunchStoryboardName", None)
info["UILaunchScreen"] = {
    "UIImageName": "MarbleLaunch",
    "UIImageRespectsSafeAreaInsets": False,
}

# Loose icon PNGs stand in for a compiled asset catalog; CFBundleIconName is
# intentionally absent so the files are picked up directly.
info["CFBundleIcons"] = {
    "CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60"]}
}
info["CFBundleIcons~ipad"] = {
    "CFBundlePrimaryIcon": {
        "CFBundleIconFiles": ["AppIcon60x60", "AppIcon76x76", "AppIcon83.5x83.5"]
    }
}

# The About screen shows this commit; Xcode builds leave it absent → "dev".
if os.environ.get("GIT_SHA"):
    info["MarbleGitCommit"] = os.environ["GIT_SHA"]

# Keys Xcode normally injects at build time.
info["CFBundleSupportedPlatforms"] = ["iPhoneOS"]
info["MinimumOSVersion"] = os.environ["MIN_IOS"]
info["UIDeviceFamily"] = [1, 2]
info["DTPlatformName"] = "iphoneos"
info["DTPlatformVersion"] = os.environ["SDK_VERSION"]
info["DTSDKName"] = os.environ["SDK_NAME"]
if os.environ.get("SDK_BUILD"):
    info["DTSDKBuild"] = os.environ["SDK_BUILD"]

with open(dst, "wb") as f:
    plistlib.dump(info, f, sort_keys=False)
PYEOF
printf 'APPL????' > "$APP_BUNDLE/PkgInfo"

# Loose icon PNGs resized from the committed 1024 placeholder, and the launch
# images referenced by UILaunchScreen (actool is Mac-only, so no Assets.car).
APP_DIR_ENV="$APP_DIR" python3 - "$APP_BUNDLE" <<'PYEOF'
import os, sys
from PIL import Image

bundle = sys.argv[1]
app_dir = os.environ["APP_DIR_ENV"]
icon_src = os.path.join(
    app_dir, "GlassMarble/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png"
)
sizes = [
    ("AppIcon60x60.png", 120),
    ("AppIcon60x60@2x.png", 120),
    ("AppIcon60x60@3x.png", 180),
    ("AppIcon76x76.png", 76),
    ("AppIcon76x76@2x.png", 152),
    ("AppIcon83.5x83.5@2x~ipad.png", 167),
]
icon = Image.open(icon_src).convert("RGB")
for name, px in sizes:
    icon.resize((px, px), Image.LANCZOS).save(os.path.join(bundle, name))
import shutil
for name in ("MarbleLaunch@2x.png", "MarbleLaunch@3x.png"):
    shutil.copy(os.path.join(app_dir, "Config/Launch", name), bundle)
PYEOF

echo "note: no asset catalog compiled (actool unavailable on Linux); shipping without Assets.car"

# Bundle any non-OS Swift dylibs the executable references. Everything under
# /usr/lib/swift ships in the iOS 18 dyld cache; nothing else is needed today.
FRAMEWORKS_DIR="$APP_BUNDLE/Frameworks"
mkdir -p "$FRAMEWORKS_DIR"
copied=0
while IFS= read -r dep; do
    dep="${dep#"${dep%%[![:space:]]*}"}"
    case "$dep" in
        @rpath/libswift*.dylib|@executable_path/libswift*.dylib|@loader_path/libswift*.dylib)
            name="$(basename "$dep")"
            src="$TOOLCHAIN_LIB/swift/iphoneos/$name"
            if [ -f "$src" ]; then
                cp "$src" "$FRAMEWORKS_DIR/$name"
                copied=$((copied + 1))
            else
                echo "warning: $dep referenced but not found in toolchain libs" >&2
            fi
            ;;
        /usr/lib/swift/*|/System/*|/usr/lib/*)
            ;;
        *)
            echo "note: unhandled dependency $dep" >&2
            ;;
    esac
done < <("$LLVM_OBJDUMP" --macho --dylibs-used "$APP_BUNDLE/GlassMarble" | tail -n +2 | sed 's/ (.*//')
[ "$copied" -eq 0 ] && rmdir "$FRAMEWORKS_DIR" || echo "bundled $copied Swift dylib(s) into Frameworks/"

mkdir -p "$OUT_DIR"
IPA="$OUT_DIR/GlassMarble-$MARKETING_VERSION-$PRERELEASE-$SHORT_SHA.ipa"
rm -f "$IPA"
(cd "$STAGING" && zip -qry -X "$IPA" Payload)

echo "IPA: $IPA"
sha256sum "$IPA"

# Validation
unzip -l "$IPA" | grep -E 'Payload/GlassMarble.app/(GlassMarble|Info.plist)' >/dev/null
"$LLVM_OBJDUMP" --macho --private-headers "$APP_BUNDLE/GlassMarble" | grep -E 'ARM64|EXECUTE|platform ios|minos'
"$LLVM_OBJDUMP" --macho --private-headers "$APP_BUNDLE/GlassMarble" | grep -q 'LC_CODE_SIGNATURE' \
    && echo "note: binary carries an LC_CODE_SIGNATURE" \
    || echo "note: no LC_CODE_SIGNATURE (unsigned build)"
