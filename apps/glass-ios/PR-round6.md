# Glass iOS round 6: Lens consistency

## Summary

- Align the chat and Threads bars, search, conversation cards, composer, and footer to the same Lens spacing and control sizes. Carry the rounded groups and selections into Settings, the model picker, thread info, diffs, and the iPad split view.
- Group threads into Pinned, Today, Yesterday, Previous 7 Days, and calendar months. Read timestamps from older hosts, keep titles on one line, and use a meaningful excerpt or generated label for command and greeting titles. Show time and model or device in the secondary line.
- Let the keyboard safe area move the composer during interactive dismissal. Keep an 18-point gap at the end of the transcript, including while Writing is visible.
- Replace the broadcast icon and elapsed timer with the teal Lens mark in the Dynamic Island and Lock Screen. The mark subtly changes intensity on activity updates, and stays still with Reduce Motion.

## Simulator evidence

| Screen | Light | Dark |
| --- | --- | --- |
| Threads | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/sidebar-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/sidebar-dark.png?raw=true) |
| Just-sent chat with keyboard | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/chat-keyboard-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/chat-keyboard-dark.png?raw=true) |
| Settings | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/settings-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/settings-dark.png?raw=true) |
| Model picker | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/model-picker-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/model-picker-dark.png?raw=true) |
| Thread info | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/thread-info-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/thread-info-dark.png?raw=true) |
| iPad split view | [Light](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/ipad-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/ipad-dark.png?raw=true) |

- [Interactive keyboard dismissal](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/keyboard-dismissal.mp4?raw=true)
- Live Activity: [Compact](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/live-activity-compact.png?raw=true) · [Expanded](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/live-activity-expanded.png?raw=true) · [Lock Screen](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/live-activity-lock-screen.png?raw=true)
- [Unsigned Release IPA](https://github.com/wanazhar/glass/blob/pr-assets/glass-ios-round6/Glass.ipa?raw=true)

## Local verification

- GlassClientKit unit tests: `swift test --package-path apps/shared/GlassClientKit`
- Glass scheme simulator checks on iOS 26.5, including iPhone 14 Pro Max and iPad Pro 11-inch.
- Unsigned arm64 Release app and widget packaging checks.
