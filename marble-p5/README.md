# Marble iOS P5 Simulator evidence

The audit used iOS 26.5 Simulator, Xcode 26.6 and an offline fixture host. Default, XXXL and AX1 are the Simulator’s `large`, `extra-extra-extra-large` and `accessibility-medium` text sizes.

## Full layout and typography audit

The item 17 and 18 baseline is [c12b589](https://github.com/wanazhar/glass/commit/c12b589eab53d19d40ca93b82e4bcde2a8ba98d3). The first audit revision is [78828f9](https://github.com/wanazhar/glass/commit/78828f9965e1adb7d668791b7a4c007ced9885a3). The [first-pass manifest](MANIFEST-78828f9.md) records the Simulator versions, captured screens, validation and findings from that revision.

| Simulator | Before, light | Before, dark | After layout and typography, light | After layout and typography, dark |
| --- | --- | --- | --- | --- |
| iPhone 14 Pro Max | [Default](screenshots/before-c12b589/iphone14promax-light-default), [XXXL](screenshots/before-c12b589/iphone14promax-light-xxxl), [AX1](screenshots/before-c12b589/iphone14promax-light-ax1) | [Default](screenshots/before-c12b589/iphone14promax-dark-default), [XXXL](screenshots/before-c12b589/iphone14promax-dark-xxxl), [AX1](screenshots/before-c12b589/iphone14promax-dark-ax1) | [Default](screenshots/after-78828f9/iphone14promax-light-default), [XXXL](screenshots/after-78828f9/iphone14promax-light-xxxl), [AX1](screenshots/after-78828f9/iphone14promax-light-ax1) | [Default](screenshots/after-78828f9/iphone14promax-dark-default), [XXXL](screenshots/after-78828f9/iphone14promax-dark-xxxl), [AX1](screenshots/after-78828f9/iphone14promax-dark-ax1) |
| iPhone SE (3rd generation) | [Default](screenshots/before-c12b589/iphonese3-light-default), [XXXL](screenshots/before-c12b589/iphonese3-light-xxxl), [AX1](screenshots/before-c12b589/iphonese3-light-ax1) | [Default](screenshots/before-c12b589/iphonese3-dark-default), [XXXL](screenshots/before-c12b589/iphonese3-dark-xxxl), [AX1](screenshots/before-c12b589/iphonese3-dark-ax1) | [Default](screenshots/after-78828f9/iphonese3-light-default), [XXXL](screenshots/after-78828f9/iphonese3-light-xxxl), [AX1](screenshots/after-78828f9/iphonese3-light-ax1) | [Default](screenshots/after-78828f9/iphonese3-dark-default), [XXXL](screenshots/after-78828f9/iphonese3-dark-xxxl), [AX1](screenshots/after-78828f9/iphonese3-dark-ax1) |
| 13-inch iPad Pro | [Default](screenshots/before-c12b589/ipadpro13-light-default), [XXXL](screenshots/before-c12b589/ipadpro13-light-xxxl), [AX1](screenshots/before-c12b589/ipadpro13-light-ax1) | [Default](screenshots/before-c12b589/ipadpro13-dark-default), [XXXL](screenshots/before-c12b589/ipadpro13-dark-xxxl), [AX1](screenshots/before-c12b589/ipadpro13-dark-ax1) | [Default](screenshots/after-78828f9/ipadpro13-light-default), [XXXL](screenshots/after-78828f9/ipadpro13-light-xxxl), [AX1](screenshots/after-78828f9/ipadpro13-light-ax1) | [Default](screenshots/after-78828f9/ipadpro13-dark-default), [XXXL](screenshots/after-78828f9/ipadpro13-dark-xxxl), [AX1](screenshots/after-78828f9/ipadpro13-dark-ax1) |

### Layout retest

The first-audit fixes were retested at `63afc126`. The default text size captures include the roster, agent detail, chat, Everyone, Activity, onboarding, pairing, and the first-level Settings destinations.

| Simulator | Default, light |
| --- | --- |
| iPhone 14 Pro Max | [55 screenshots](screenshots/retest-63afc126/a63-14pm-light-default) |
| iPhone SE (3rd generation) | [54 screenshots](screenshots/retest-63afc126/a63-se3-light-default) |
| 13-inch iPad Pro | [55 screenshots](screenshots/retest-63afc126/a63-ipad-light-default) |

[Before and after in light mode](comparisons/final-layout-light.png) shows the pairing background, Private chat contrast, Stop confirmation and iPad Connection view with the host-build notice.

[iPhone SE chat title](comparisons/audit-se-chat-header.png), [AX1 composer](comparisons/audit-ax1-chat-composer.png), [agent detail](comparisons/audit-se-agent-detail.png), [Everyone thread](comparisons/audit-everyone-thread.png), [Settings labels](comparisons/audit-settings-label-wrap.png), [Connection toggle](comparisons/audit-connection-toggle.png), and [Activity spacing](comparisons/audit-activity-spacing.png) show the first audit fixes.

## Typography comparisons

| Screen | Light | Dark |
| --- | --- | --- |
| Agents | [Before and after](comparisons/typography-light-agents.png) | [Before and after](comparisons/typography-dark-agents.png) |
| Chat | [Before and after](comparisons/typography-light-chat-reply.png) | [Before and after](comparisons/typography-dark-chat-reply.png) |
| Everyone | [Before and after](comparisons/typography-light-everyone.png) | [Before and after](comparisons/typography-dark-everyone.png) |
| Settings | [Before and retest](comparisons/typography-final-light-settings.png) | [Before and first audit pass](comparisons/typography-dark-settings.png) |

## P5 feature and behavior captures

- [Code, composer, and routing](comparisons/p5-feature-review-1.png) shows the horizontal scroll affordance, Copy, the open keyboard and routing to Engineer in light and dark.
- [Everyone and pause](comparisons/p5-feature-review-2.png) shows explicit Ask everyone and the persistent Paused notice in both appearances. The comparison records the confirmation before it moved to a centered alert.
- [New chat and tools](comparisons/p5-feature-review-3.png) shows the warm New chat background and failure text in light and dark.
- [iPhone and iPad in light and dark](screenshots/p5-c12b589) and [iPhone 17 Pro in light and dark](screenshots/p5-78828f9) contain the individual P5 captures.
- [Lock Screen and Dynamic Island](screenshots/live-activity) includes phase, elapsed time, a compact presentation, expanded Stop and Stopping, and the stale Lock Screen. The recordings show [phase changes and completion](recordings/live-activity-run2-finish.mp4) and [Stop and dismissal](recordings/live-activity-run3-exp-stop.mp4).
- The iPhone 17 Pro avatar frames at `63afc126` show [idle breathing](comparisons/a63-light-idle-crops.png) and [a larger rotation and pulse while working](comparisons/a63-light-working-crops.png).
- The P5 app flow was recorded in [light](recordings/final-iphone17pro-light-main.mp4) and [dark](recordings/final-iphone17pro-dark-main.mp4). The [host version mismatch recording](recordings/final-iphone17pro-light-mm.mp4) uses the disposable fixture.

## Earlier unsigned Release IPA

[GlassMarble 0.2.0 (6)](GlassMarble-63afc12.ipa) was built for arm64 iOS with Xcode 26.6 from `63afc126a04f23f69bd5ca55f5b3b886180f11e7`.

| Size | SHA-256 |
| ---: | --- |
| 2,359,962 bytes | `b4cffbeb853629220600d67559277c68a5b6891899adfb3988d7dddd462897da` |
