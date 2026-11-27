# Glass iOS round 7: Live Activity fix and real-device polish

## Summary

- Live Activity: bump to 0.1.1 (17) and run under a new `LensConnectionAttributes` type (`status`, `pulse`) instead of changing `ConnectionActivityAttributes` in place, so a round 5 or 6 widget never has to decode the new state. Launch ends leftover activities of both types. `KeepAlive.update()` starts the activity whenever it is wanted and none is running, not only on resign-active.
- The Lens in the Dynamic Island and Lock Screen is drawn with SwiftUI shapes (`LensGlyph`), shared with the app's mark, instead of an appex asset. It keeps the teal mark and the gentle pulse, stays still with Reduce Motion, and shows no timer.
- Background connection settings say plainly why it is not running: the toggle is off, location is not allowed, Live Activities are off, or the last start request failed (with its error).
- Threads panel on iPhone: one floating Liquid Glass panel with 30-point continuous corners, inset from the screen edges and full height within the safe area. Header, search, list and footer share that one surface. Rows are transparent; the selection is a soft raised glass capsule (no teal fill) with a semibold title. The list scrolls under soft scroll edges at the top and bottom. The main chat stays visible and dimmed behind it, and the structure is the same in light and dark. The main compose button hides while the panel is open.
- Thread rows: friendly model names (`deepseek/deepseek-v4.1-flash-1017-free` shows as "DeepSeek V4.1 Flash"), no trailing separator, a time instead of repeating "Yesterday". Generated titles skip greetings, retitle from the first real request, and get a number when they would repeat. The thread count line is gone; Jump to group lives in the filter menu.
- Chat: a hard scroll edge under the title bar so text does not show through it. Jump to latest sits above the composer, exists only while you are scrolled away, and no longer covers streamed code. Work in progress no longer expands or collapses on its own as a reply streams.

## Simulator evidence

| Screen | Light | Dark |
| --- | --- | --- |
| Threads panel over a busy chat | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/sidebar-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/sidebar-dark.png?raw=true) |
| Chat | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/chat-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/chat-dark.png?raw=true) |
| iPad split view | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/ipad-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/ipad-dark.png?raw=true) |
| iPad, sidebar hidden | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/ipad-light-sidebar-hidden.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/ipad-dark-sidebar-hidden.png?raw=true) |

- [Streaming reply with Jump to latest](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/streaming-jump-to-latest.mp4?raw=true)
- Live Activity on the Release keep-alive path (toggle on, location allowed, chat open, Home, Lock Screen): [Island](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/compact.png?raw=true) · [Minimal](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/minimal.png?raw=true) · [Expanded](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/expanded.png?raw=true) · [Lock Screen](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/lock-screen.png?raw=true)
- Settings status: [Off](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/settings-off.png?raw=true) · [Ready](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/settings-ready.png?raw=true) · [Running](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/live-activity/settings-running.png?raw=true)
- Upgrade from round 5 (0.1.0 build 16) to this build: [r5 activity up](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/upgrade/1-r5-activity.png?raw=true) · [r7 installed over it](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/upgrade/2-r7-installed-over.png?raw=true) · [r7 launch ends the r5 activity](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/upgrade/3-r7-launch-ends-r5.png?raw=true) · [new r7 activity](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/upgrade/4-r7-activity.png?raw=true)
- [Unsigned Release IPA, 0.1.1 (17)](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round7/Glass.ipa?raw=true)

On the keep-alive path iOS also shows its background-location indicator in the Dynamic Island, so the Glass activity appears in its detached minimal form next to it rather than as a leading/trailing compact pair.

## Local verification

- GlassClientKit unit tests: `swift test --package-path apps/shared/GlassClientKit`
- `apps/glass-ios/scripts/test.sh`: Glass unit and UI tests on iPhone 17 (iOS 26.5).
- Release-path Live Activity states and the round 5 upgrade: Release-built `Glass.app` driven by `LiveHostE2ETests.testKeepAliveLiveActivityStates` and `testUpgradeFromRound5EndsTheOldActivity` against the E2E harness on iPhone 17 Pro Max.
- `testStreamingReplyIsFollowedUntilYouScrollThenResumes` and `testThreadsPanelOverBusyChatShots` on iPhone 17 Pro Max and iPad Pro 13-inch, light and dark.
- `scripts/package-unsigned-ipa.sh round7`: arm64, no `_CodeSignature` or `embedded.mobileprovision` in the app or widget, build 17.
