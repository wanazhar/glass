# Implement the Glass Lens design in the iOS app

## What changes

The native iOS app now uses the Lens icon, horizon ripple and empty-chat mark from the approved round 4 design. Titles use standard SF Pro. Chat uses a single scroll-edge glass toolbar, a pinned composer, and a plain transcript surface. Work, recovery and connection states keep the conversation visible. Threads and Settings use the same Lens palette, with an iPhone drawer and a native iPad split view.

## Round 4 addendum checklist

- Scroll-edge glass separates the header from the transcript. Empty chat includes a composer.
- Composer buttons have symmetric insets and 44-point circular targets. The placeholder remains while focused, and working state puts elapsed time beside the model and Stop in the send position.
- Teal actions use light/dark foreground pairs. Jump to latest uses a solid pill and only appears away from the bottom.
- The icon, splash and empty state share the Lens mark. The splash can be skipped by tapping, and Reduce Motion bypasses it.
- Run recovery shows one command count, the cause, the full failing command and exit code, and the stderr detail. Continue sends a follow-up.
- Diffs use separate old/new gutters and change counts, with neutral hunk headers. Work logs have a connected trace, live spinner, expandable reasoning and change chips.
- Code wraps with horizontal padding, syntax token colours, and a copy action. Markdown includes a heading scale, typeset math, labelled charts and Dynamic Type-friendly tables.
- The model picker groups choices by provider, marks the selected model and keeps custom IDs behind a disclosure.
- Disconnected and read-only states retain history and expose Reconnect or Take over here.
- Threads have date grouping, search, a running marker, Jump to group and a pinned glass Settings bar. The iPad uses native split navigation with one sidebar toggle and one new-chat control.
- Thread info shows context and estimated cost at the default sheet height and uses “Switch typing to another device”.

## Local validation

- `xcodebuild build -project apps/glass-ios/Glass.xcodeproj -scheme Glass -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.5' CODE_SIGNING_ALLOWED=NO`
- `xcodebuild test -project apps/glass-ios/Glass.xcodeproj -scheme Glass -destination 'platform=iOS Simulator,name=iPhone 17,OS=27.0' -only-testing:GlassTests`: 56 tests passed.
- The unmodified `RoundThreeUITests/testSecondaryScreensAtDynamicTypeXL` passed on iOS 26.5 after fixing the drawer. Before the fix, tapping Desktop during the drawer's slide could hit Settings instead. The drawer now fades in place so its controls keep their hit coordinates, and Desktop has a 44-point glass control.
- `xcodebuild test -project apps/glass-ios/Glass.xcodeproj -scheme Glass -destination 'platform=iOS Simulator,name=iPhone 17 Pro,OS=26.5' -parallel-testing-enabled NO -only-testing:GlassUITests`: 18 passed, 33 skipped, 0 failures. The same full UI target passed on iPhone 17 with iOS 27.0 using `test-without-building` against the same build.
- `RoundThreeUITests/testTabletLayout` passed separately on the 13-inch iPad Pro (M5) with iOS 26.5.
- `swift test --package-path apps/shared/GlassClientKit`: 116 tests passed.
- `xcodebuild build -project apps/glass-ios/Glass.xcodeproj -scheme Glass -configuration Release -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO`
- `unzip -tq Glass.ipa`: verified the unsigned Release package.
- `python3 scripts/check-documentation-wiki.py --base main`: 1,840 Markdown files and catalog entries validated.

No GitHub Actions were used. The standard phone UI runs skipped the iPad-only test and the opt-in live-host, seeded gateway, and screenshot/fixture tours. The live-host harness was not run. Screenshots use the real app with local debug fixtures to show repeatable states.

## Simulator evidence

All phone captures use the iPhone 17 Pro with the same 9:41 status bar and display mask. The iPad captures use the 13-inch iPad Pro (M5).
The working previews start at 1m 12s and show the timer continuing to count.

[Splash recording](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/splash.mp4?raw=true)

| State | Light | Dark |
| --- | --- | --- |
| Splash | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/splash.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/splash.png?raw=true) |
| Empty chat | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/empty.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/empty.png?raw=true) |
| Active chat | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/active.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/active.png?raw=true) |
| Working | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/working.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/working.png?raw=true) |
| Expanded work log | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/work-log-expanded.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/work-log-expanded.png?raw=true) |
| Failed run | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/failed-run.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/failed-run.png?raw=true) |
| Disconnected | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/disconnected.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/disconnected.png?raw=true) |
| Read-only | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/read-only.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/read-only.png?raw=true) |
| Diff | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/diff.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/diff.png?raw=true) |
| Model picker | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/model-picker.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/model-picker.png?raw=true) |
| Thread info | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/thread-info.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/thread-info.png?raw=true) |
| Threads drawer | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/drawer.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/drawer.png?raw=true) |
| Settings | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/settings.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/settings.png?raw=true) |
| Dynamic Type XL | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/dynamic-type-xl.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/dynamic-type-xl.png?raw=true) |
| iPad split view | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/light/ipad.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round5/dark/ipad.png?raw=true) |

Unsigned Release artifact: `/Users/devin/repos/glass-evidence/Glass.ipa`.
