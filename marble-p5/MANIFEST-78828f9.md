# Marble P5 Apple validation manifest — 78828f9965e1adb7d668791b7a4c007ced9885a3

Baseline (before): c12b589eab53d19d40ca93b82e4bcde2a8ba98d3. Host fixture for audit runs: cc46036ae23d (mismatch banner intentionally visible in before and after).

## Environment
- macOS 26.6.2 (25G83), Xcode 26.6 (17F113), iOS Simulator runtime 26.5 (23F73), rustc 1.99.0
- Simulators: iPhone 14 Pro Max (1290×2796), iPhone SE 3rd gen (750×1334, smallest iOS 26 logical size 375×667), iPad Pro 13" M5 (2064×2752), iPhone 17 Pro (1206×2622)
- Dynamic Type via `xcrun simctl ui <udid> content_size {large|extra-extra-extra-large|accessibility-medium}`; appearance via `simctl ui <udid> appearance`
- Local/offline fixture only (`MARBLE_FIXTURE_TURN_EVIDENCE=1`), no OCI/provider calls

## macOS checks (78828f9)
- `cargo test -p glass-marble --locked` (clean, idle machine): lib 218 passed/1 ignored; build_metadata 1; host_loop 24; provider_reasoning 1; server_ui 132 passed/1 ignored; 0 failed
  - Earlier run under 4-simulator load: 1 transient failure `everyone_task_follow_up_waits_for_a_tool_free_greeting_to_settle`; passed 3/3 isolated and in the clean full run
- Ignored Core-backed shell test with `GLASS_CORE_TEST_BINARY`: passed
- `cargo test -p glass-core auth::broker::scoped_rotation_survives_the_broker_connection`: passed
- `cargo fmt --all -- --check`; `cargo clippy -p glass-marble --all-targets -- -D warnings`: clean
- `MARBLE_HOST_TESTS=1 swift test` (apps/glass-marble-ios): 241 tests, 12 skipped, 0 failures
- GlassClientKit `swift test`: 118 tests, 0 failures

## Xcode
- `xcodebuild build-for-testing -project GlassMarble.xcodeproj -scheme "Glass Marble" -destination "generic/platform=iOS Simulator" -derivedDataPath ~/dd-head CODE_SIGNING_ALLOWED=NO MARBLE_GIT_COMMIT=<sha>`: succeeded at 66d4588, 714160c, 78828f9
- `xcodebuild test-without-building ... -parallel-testing-enabled NO -only-testing:GlassMarbleUITests/...`
- iPhone 17 Pro @78828f9 (light): CodeAndComposer, EveryoneRouting, PauseAcrossTabs, TurnPresentation, AvatarRosterAndHeader, AvatarEveryone, BuildMismatch — 7/7 pass
- iPhone 17 Pro @78828f9 (dark): CodeAndComposer, EveryoneRouting, PauseAcrossTabs, TurnPresentation — 4/4 pass
- Audit harness `MarbleAuditTests` (test-only, untracked, runs only with MARBLE_AUDIT=1): every run listed below passed

## Unsigned Release IPA
- `MARBLE_GIT_COMMIT="$(git rev-parse HEAD)" xcodebuild build -project GlassMarble.xcodeproj -scheme "Glass Marble" -configuration Release -destination generic/platform=iOS -derivedDataPath ~/dd-ipa CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" MARBLE_GIT_COMMIT="$MARBLE_GIT_COMMIT"`; `ditto -c -k --sequesterRsrc --keepParent Payload GlassMarble-78828f9.ipa`
- MarbleGitCommit 78828f9965e1adb7d668791b7a4c007ced9885a3; com.wanazhar.glassmarble 0.2.0 (6); iphoneos26.5; NSSupportsLiveActivities=true
- PlugIns/MarbleLiveActivity.appex: com.wanazhar.glassmarble.LiveActivity, com.apple.widgetkit-extension (no MarbleGitCommit key in extension plist)
- Both executables arm64, unsigned, 0755

## Audit screenshot sets (original PNGs; audit-* = before c12b589, after-* = 78828f9, tail-* = Settings About + Stop confirmation)
| set | PNGs | size |
|---|---|---|
| after-ipadpro13-dark-ax1 | 49 | 2064×2752 |
| after-ipadpro13-dark-default | 53 | 2064×2752 |
| after-ipadpro13-dark-xxxl | 53 | 2064×2752 |
| after-ipadpro13-light-ax1 | 53 | 2064×2752 |
| after-ipadpro13-light-default | 53 | 2064×2752 |
| after-ipadpro13-light-xxxl | 53 | 2064×2752 |
| after-iphone14promax-dark-ax1 | 53 | 1290×2796 |
| after-iphone14promax-dark-default | 49 | 1290×2796 |
| after-iphone14promax-dark-xxxl | 49 | 1290×2796 |
| after-iphone14promax-light-ax1 | 49 | 1290×2796 |
| after-iphone14promax-light-default | 49 | 1290×2796 |
| after-iphone14promax-light-xxxl | 49 | 1290×2796 |
| after-iphonese3-dark-ax1 | 53 | 750×1334 |
| after-iphonese3-dark-default | 45 | 750×1334 |
| after-iphonese3-dark-xxxl | 45 | 750×1334 |
| after-iphonese3-light-ax1 | 42 | 750×1334 |
| after-iphonese3-light-ax1-rerun | 41 | 750×1334 |
| after-iphonese3-light-default | 45 | 750×1334 |
| after-iphonese3-light-xxxl | 45 | 750×1334 |
| audit-ipadpro13-dark-ax1 | 49 | 2064×2752 |
| audit-ipadpro13-dark-ax1-rerun | 49 | 2064×2752 |
| audit-ipadpro13-dark-default | 53 | 2064×2752 |
| audit-ipadpro13-dark-xxxl | 53 | 2064×2752 |
| audit-ipadpro13-light-ax1 | 49 | 2064×2752 |
| audit-ipadpro13-light-ax1-rerun | 49 | 2064×2752 |
| audit-ipadpro13-light-default | 53 | 2064×2752 |
| audit-ipadpro13-light-xxxl | 53 | 2064×2752 |
| audit-iphone14promax-dark-ax1 | 49 | 1290×2796 |
| audit-iphone14promax-dark-default | 49 | 1290×2796 |
| audit-iphone14promax-dark-xxxl | 49 | 1290×2796 |
| audit-iphone14promax-light-ax1 | 8 | 1290×2796 |
| audit-iphone14promax-light-ax1-rerun | 23 | 1290×2796 |
| audit-iphone14promax-light-ax1-rerun2 | 52 | 1290×2796 |
| audit-iphone14promax-light-default | 49 | 1290×2796 |
| audit-iphone14promax-light-xxxl | 49 | 1290×2796 |
| audit-iphonese3-dark-ax1 | 34 | 750×1334 |
| audit-iphonese3-dark-ax1-rerun | 41 | 750×1334 |
| audit-iphonese3-dark-ax1-rerun2 | 41 | 750×1334 |
| audit-iphonese3-dark-default | 49 | 750×1334 |
| audit-iphonese3-dark-xxxl | 45 | 750×1334 |
| audit-iphonese3-light-ax1 | 34 | 750×1334 |
| audit-iphonese3-light-ax1-rerun | 53 | 750×1334 |
| audit-iphonese3-light-default | 49 | 750×1334 |
| audit-iphonese3-light-xxxl | 45 | 750×1334 |
| tail-after-iphone14promax-light-default | 5 | 1290×2796 |
| tail-after-iphonese3-dark-ax1 | 5 | 750×1334 |
| tail-after-iphonese3-dark-default | 5 | 750×1334 |
| tail-after-iphonese3-dark-xxxl | 5 | 750×1334 |
| tail-after-iphonese3-light-ax1 | 5 | 750×1334 |
| tail-after-iphonese3-light-default | 4 | 750×1334 |
| tail-after-iphonese3-light-xxxl | 5 | 750×1334 |
| tail-audit-iphonese3-dark-xxxl | 5 | 750×1334 |
| tail-audit-iphonese3-light-xxxl | 5 | 750×1334 |
Superseded/incomplete sets kept for transparency: audit-iphone14promax-light-ax1 (8, UI query timeout), -rerun (23, harness tab-tap bug); use -rerun2.

## Known coverage gaps
- iPad Pro 13 at AX1 (before and after, light and dark): New agent sheet not captured (harness could not hit `New agent` at AX1). Default and XXXL iPad sets include it.
- Baseline SE 3 dark AX1: chat/agent detail/threads not captured in 3 attempts (Engineer row stays under the tab bar after scrolling). SE 3 light AX1 rerun covers them.
- SE 3 light default tail: Stop confirmation not captured; covered at XXXL/AX1 tails and SE 3 dark default tail.
- Consent sheet: fixture seeds no pending approval. Live Activity is covered by the separate recordings (p5-items1-16-mp4.zip), not by the audit harness.

## Remaining visual findings on 78828f9 (not fixed)
1. Pairing: white (light) / black (dark) rectangle over the lower half after keyboard dismissal — all devices, all sizes
2. Contrast: "Connect to host", "Connect", New chat "Private chat" — dark text on plum (light), white on light pink (dark)
3. iPad: mismatch banner truncated next to floating tab bar ("Marble and your ho…")
4. Stop confirmation popover covers the banner and Settings title on iPhone
5. Settings sub-screen backgrounds: cool grey (light) / pure black (dark) vs warm root screens
6. Settings › Approvals and activity pushes Activity with a large title (large titles should be root-only)
7. Root Settings rows taller than baseline; Stop everything below the fold on 14 Pro Max
8. "Recent decisions" header has a tight gap above it
9. AX1: wrapped Settings row labels (e.g. "Approvals / and activity") continue under the icon instead of aligning with the first line

## Archives
| file | bytes | sha256 |
|---|---|---|
| after-78828f9-ax1-tails.zip | 72407488 | b491143f55ace53c16cb94312050826d31eb63d7781e6ee566a65d1a4e062fd1 |
| after-78828f9-default-xxxl.zip | 81952553 | e7ad00c72bc700c4364eb64bba783041e3f804d6d661372817fa97e162309962 |
| baseline-c12b589-ax1-xxxl-rest.zip | 141249803 | dcaf36778939885fb10182fb6b1856aee9faff592c22e0aad6ae285c76a03a00 |
| baseline-c12b589-dark-default.zip | 23286975 | ae90800e8853fd25878ba23c18753e06a02985d3a9606abec3f59b6e4b4c066c |
| baseline-c12b589-light-default.zip | 28067701 | 7b72ea026bc3d3c9681cfd76af6193ef28dd37f142e241870618e5f9f10fa2d0 |
| baseline-c12b589-xxxl-partial.zip | 47801068 | 8850772b93d9b3c6b7a5eee3ad422bde2e8e2273c21ecfcbc914583efc3e35e8 |
| head-78828f9-iphone17pro-p5.zip | 101137857 | 812a4bb23cfdd700e47196c9f07c4e18d75137de64e775c77553377362420c98 |
| p5-items1-16-mp4.zip | 465882969 | 9849fb2f7937746b400677d0547cd1fe8c472c2c90046a288f1936324b5dbcff |
| p5-items1-16-png.zip | 28935925 | 74f6ef23129337715868f90ef5a4ccfc6bc1adbd04818cadf94cc1bb7fbcf56d |
| GlassMarble-78828f9.ipa | 2334014 | 38262d5b13032f8c0998dd2f1c4877f25e73ac5fe3ece37bf7550e0bb9b34b19 |
