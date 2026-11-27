#!/usr/bin/env bash
set -euo pipefail

device="${1:?Pass the simulator UDID}"
app="${2:?Pass the built MarbleRound1.app directory}"
mode="${3:-iphone}"
round="$(cd "$(dirname "$0")/.." && pwd)"
bundle=studio.marble.round1

shot() {
    local state="$1" path="$2"
    xcrun simctl launch --terminate-running-process "$device" "$bundle" "$state"
    sleep 2
    xcrun simctl io "$device" screenshot --mask=black "$round/$path.png"
}

xcrun simctl terminate "$device" com.wanazhar.glassmarble 2>/dev/null || true
xcrun simctl install "$device" "$app"
xcrun simctl status_bar "$device" override --time '9:41' \
    --dataNetwork wifi --wifiMode active --wifiBars 3 --cellularMode active \
    --cellularBars 4 --batteryState discharging --batteryLevel 100
xcrun simctl ui "$device" content_size large

case "$mode" in
    iphone)
        for theme in light dark; do
            xcrun simctl ui "$device" appearance "$theme"
            for state in picker home-everyone everyone chat profile memory edit-memory confirm-memory new-marble \
                activity settings pairing empty error offline connecting disconnected expired threads search; do
                shot "$state" "iphone/$state-$theme"
            done
            for state in identity system components controls; do
                shot "$state" "sheets/$state-$theme"
            done
        done
        xcrun simctl ui "$device" appearance light
        shot storyboard splash/storyboard
        ;;
    ipad)
        for theme in light dark; do
            xcrun simctl ui "$device" appearance "$theme"
            shot picker "ipad/home-$theme"
            shot chat "ipad/chat-$theme"
        done
        ;;
    xl)
        xcrun simctl ui "$device" appearance light
        xcrun simctl ui "$device" content_size extra-large
        for state in picker chat memory new-marble; do shot "$state" "iphone/$state-xl"; done
        ;;
    *) printf 'Unknown capture mode: %s\n' "$mode" >&2; exit 2 ;;
esac

xcrun simctl ui "$device" content_size large
