# Marble iOS — round 2

## What changed

- The personality picker is home, with a visible New Marble row and a separate Everyone tab. On iPad, the picker and the selected conversation share a native split view.
- Pocket Pair is the app identity. Avatars use shape and hue without eyes. The launch screen and cold-start splash use the same adaptive paper colour.
- The app pairs with a Marble host and uses host data for chat, threads, search, profiles, memories, suggestions and Activity. Transcripts and memory use opaque surfaces; native navigation and discrete controls use Liquid Glass.
- The app requires iOS 26. The design prototype has been removed; the round-1 design images remain as a reference.

## Verification

- Built for the iOS 26.5 simulator and built an unsigned Release app for iOS devices.
- Ran the iOS UI suite and the Swift package tests, including the live suite against a local Marble host with the repository’s fake runtime. The fixture exercises real host requests and state changes; it does not exercise a live model provider.
- Named chat streams replies and supports Stop. While a named Marble is working, its composer shows Stop and explains that corrections require stopping and resending. In-flight Steer remains unavailable for named chats pending host support. No Marble host code changed.

## Simulator evidence

- Picker: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/picker-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/picker-dark.png?raw=true)
- Memory ownership: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/memory-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/memory-dark.png?raw=true)
- iPad split view: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/ipad-chat-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/ipad-chat-dark.png?raw=true)
- Named chat streaming with Stop: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/named-streaming-stop-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/named-streaming-stop-dark.png?raw=true). After Stop: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/named-stopped-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/named-stopped-dark.png?raw=true)
- Offline: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/offline-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/offline-dark.png?raw=true). Expired token: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/expired-token-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/expired-token-dark.png?raw=true)
- New Marble at Dynamic Type XL after scrolling: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/new-marble-xl-scrolled-light.png?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/new-marble-xl-scrolled-dark.png?raw=true)
- Splash recordings: [light](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/splash-light.mov?raw=true) · [dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/splash-dark.mov?raw=true) · [dark with Reduce Motion enabled](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/splash-reduce-motion-dark.mov?raw=true)
- [Unsigned Release IPA](https://github.com/wanazhar/glass/blob/pr-assets/marble-round2/Marble.ipa?raw=true)
