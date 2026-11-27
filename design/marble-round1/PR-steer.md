# Marble iOS — steer a named reply and polish pairing

## What changed

- A named Marble’s composer lets you submit a correction while its reply is streaming, with Stop still available.
- The host matches the correction to the running task and chat, then sends it to that Marble’s own engine. The correction does not start a new turn.
- Pairing uses a native large title and a single grouped section for the Host and Access token fields. Connection errors appear directly under the field they concern.
- The picker lets the New Marble row scroll fully clear of the floating tab bar.

## Local verification

- `cargo test -p glass-marble --locked` covers HTTP routing to one of two concurrently running named engines and rejects mismatched or settled turns. Checked crate formatting and Clippy locally.
- The iOS 26.5 simulator connected to the locally built Marble host on port 4783. The deterministic runtime fixture streamed an initial answer, received the correction over the runtime protocol, and continued the same reply using the correction. This verifies the app-to-host path, not a live model provider.

## Simulator evidence

1. [The named reply starts streaming](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/named-reply-streaming.png?raw=true)
2. [Steer and Stop are available during the same reply](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/named-steer-and-stop.png?raw=true)
3. [The same reply continues with the correction](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/named-reply-corrected.png?raw=true)

[Screen recording: submit the correction and watch the same reply continue](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/named-steer.mov?raw=true)

| State | Light | Dark |
| --- | --- | --- |
| Pair your host | [Light](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/pairing-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/pairing-dark.png?raw=true) |
| Access token error | [Light](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/pairing-error-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/pairing-error-dark.png?raw=true) |
| Picker scrolled to New Marble | [Light](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/picker-bottom-light.png?raw=true) | [Dark](https://github.com/wanazhar/glass/blob/pr-assets/marble-steer/picker-bottom-dark.png?raw=true) |
