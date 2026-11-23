# Glass iOS: full-parity plan

Status: plan only. No app code exists yet. This document plans a native iOS
app that replaces the `glass-core` web UI.

| Item | Value |
| --- | --- |
| App | Glass (display name `Glass`) |
| Bundle identifier | `com.wanazhar.glass` |
| Minimum OS | iOS 18.0 (iPhone and iPad) |
| UI | SwiftUI, with UIKit only where SwiftUI has no equivalent |
| App path | `apps/glass-ios` |
| Shared client | `apps/shared/GlassClientKit` (Swift package) |
| Deliverable per phase | Unsigned `.ipa` (a zipped `Payload/Glass.app`); the owner signs it |
| Source snapshot | `main` at `8c90a8a5` |

## Contents

- [1. Sources read](#1-sources-read)
- [2. Toolchain check on this Mac](#2-toolchain-check-on-this-mac)
- [3. Gateway contract the app must speak](#3-gateway-contract-the-app-must-speak)
- [4. Parity matrix](#4-parity-matrix)
- [5. GlassClientKit shared package](#5-glassclientkit-shared-package)
- [6. App architecture](#6-app-architecture)
- [7. Build and unsigned IPA pipeline](#7-build-and-unsigned-ipa-pipeline)
- [8. Phased milestones and test gates](#8-phased-milestones-and-test-gates)
- [9. Required and recommended gateway changes](#9-required-and-recommended-gateway-changes)
- [10. Risks](#10-risks)
- [11. Open questions for the owner](#11-open-questions-for-the-owner)

## 1. Sources read

Web UI (`products/glass-core/web/src`, about 10,000 lines of JavaScript):

- `index.html`: every screen, dialog and control ID in the matrix below.
- `app.js`: boot, threads, timeline, event reducer, composer, command
  palette, settings routing, preferences, voice, shortcuts, and the service
  worker registration.
- `owner-auth.js`: owner capability storage, authenticated `fetch`, the
  authenticated SSE reader, and authenticated upload.
- `settings.js`, `workflows.js` (details tabs: Activity, Reasoning, Context,
  Files, changes, teammates), `file-tree.js`, `desktop.js`,
  `desktop-updates.js`, `teammate-status.js`, `agent-ui.js`,
  `agent-transcript.js`, `activity.js`, `activity-disclosure.js`,
  `command-results.js`, `drafts.js`, `notifications.js`, `select.js`,
  `reasoning-reader.js`, `diagnostic-events.js`, `web-diagnostics.js`,
  `startup-timing.js`, `shell-update.js`, `sw.js`, `manifest.webmanifest`,
  `theme-chrome.js`, `theme-head-entry.js`, and the lazy renderers
  `syntax.js`, `math.js`, `mermaid.js`, `pdf.js`.

Gateway (`crates/glass-core/src`):

- `connect.rs`: Connect server, Tailscale and local auth, `/api/local`
  pairing, every `/api/*` route, SSE with `Last-Event-ID`, uploads, files,
  artifacts, export, desktop transport, body limits, security headers.
- `web_commands.rs`, `actions.rs`: the command catalogue, read-only and
  supported command sets, command forms and bounded results.
- `web_settings.rs`: the settings snapshot and revisioned update.
- `serve.rs`: control ownership (`can_write`) for a shared thread.
- `desktop.rs`, `files.rs`: frame and file size limits.
- `products/glass-core/docs/connect.md`: product behavior of Connect.

Existing native client: `apps/glass-marble-ios` is not on `main` (it was
reverted in `8c90a8a5`). It was read from `origin/feat/marble-selfhosted-ios`:
`MarbleClient.swift`, `MarbleStore.swift`, `MarbleModels.swift`,
`ConnectSettingsView.swift`, and its Xcode project (iOS 18.0, Swift 5,
bundle `com.wanazhar.glassmarble`, storyboard and asset catalog).

## 2. Toolchain check on this Mac

Run on 2026-11-21, macOS 26.6.2 (arm64).

| Check | Result |
| --- | --- |
| `xcodebuild -version` | `Xcode 26.6`, build `17F113` |
| `xcode-select -p` | `/Applications/Xcode.app/Contents/Developer` |
| Swift | Apple Swift 6.3.3 |
| `xcodebuild -showsdks` (iOS) | `iOS 26.5 -sdk iphoneos26.5`, `Simulator - iOS 26.5 -sdk iphonesimulator26.5` |
| Simulator runtimes | iOS 26.5, iOS 27.0; a booted iPhone 17 on iOS 26.5 |
| `xcodegen` / `tuist` | Not installed (the plan does not need them) |

Unsigned device build: **passed**. A trivial SwiftUI app (one `App` struct,
`Base.lproj/LaunchScreen.storyboard`, `Assets.xcassets` with `AppIcon` and an
`AccentColor` color set, generated Info.plist, bundle `com.wanazhar.glass`,
display name `Glass`, deployment target 18.0) was built outside the repo with:

```sh
xcodebuild -project Hello.xcodeproj -scheme Hello -configuration Release \
  -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath build \
  CODE_SIGNING_ALLOWED=NO build
mkdir Payload && cp -R build/Build/Products/Release-iphoneos/Hello.app Payload/
ditto -c -k --sequesterRsrc --keepParent Payload Hello.ipa
```

Observed:

- `** BUILD SUCCEEDED **`, with `CompileStoryboard` and
  `CompileAssetCatalogVariant` steps.
- The IPA (10,282 bytes) contains `Payload/Hello.app/Assets.car`,
  `Payload/Hello.app/Base.lproj/LaunchScreen.storyboardc/*.nib`, the binary
  and `Info.plist`.
- `Info.plist`: `CFBundleIdentifier=com.wanazhar.glass`,
  `CFBundleDisplayName=Glass`, `MinimumOSVersion=18.0`,
  `DTSDKName=iphoneos26.5`, `UILaunchStoryboardName=LaunchScreen`.
- `codesign -dv`: "code object is not signed at all". `lipo -info`: arm64.
- One warning only: "All interface orientations must be supported unless the
  app requires full screen". The real project sets all orientations.

So the per-phase deliverable (unsigned, properly compiled, Payload zip) works
on this Mac today.

## 3. Gateway contract the app must speak

### 3.1 Two connection modes

| Mode | Gateway setting | How the web UI authenticates | Native outcome |
| --- | --- | --- | --- |
| Remote (Tailscale) | `settings.local == false` | `tailscale serve` adds `Tailscale-User-Login`; the page sends `X-Glass-Owner-Capability` | Main mode for iPhone and iPad |
| Local | `settings.local == true`, loopback | `POST /api/local` with the bootstrap token sets an HttpOnly `gc_local` cookie; `Authorization: Bearer <token>` also accepted | Simulator only (it shares the Mac's loopback); a phone cannot reach the Mac's loopback |

### 3.2 Remote request checks (`connect.rs`: `proxy_authorized`, `authorized`)

Every `/api/*` request must pass all of these:

1. `Host` equals the authority of the configured origin or HTTPS URL.
   `URLSession` sets `Host` from the URL, so the app must use exactly that
   URL (for example `https://<machine>.<tailnet>.ts.net`).
2. If `Origin` is present it must equal that origin. `Sec-Fetch-Site`, if
   present, must be `same-origin` or `none`. `URLSession` sends neither by
   default, which passes.
3. `Tailscale-User-Login` equals the configured owner login. Tailscale Serve
   sets this header for tailnet clients. The app never sends it. The phone
   must be on the tailnet through the Tailscale iOS app.
4. `X-Glass-Owner-Capability` equals
   `hex(HMAC-SHA256(gatewayToken, "glass-connect-owner-access-v1\0" + origin))`,
   64 lowercase hex characters.

Mutations have extra checks (`same_origin_https_mutation`,
`same_origin_desktop_mutation`, `same_origin_owner_unlock`): `POST`,
`Content-Type: application/json`, and `Origin` exactly equal to the HTTPS
origin. The native client must set `Origin` itself on every `POST`.
`Origin` is not on the `URLSession` reserved-header list, so this is allowed.

### 3.3 Pairing (`POST /api/local`)

The owner link has the form `https://<host>/#owner=<token>`; the token is 64
hex characters or a 22-character URL-safe compact token (`app.js` around line
4747, `owner_bootstrap_matches`). The secret-entry link uses `#secret=<token>`.

Native pairing flow:

1. The owner scans a QR code or pastes or opens the owner link. Glass
   registers as a handler for pasted links and the share sheet. The fragment
   never leaves the device.
2. Glass derives `origin = scheme://host[:port]` from the link.
3. Glass sends `POST {origin}/api/local` with `Origin: {origin}`,
   `Content-Type: application/json`, body `{"token":"<token>"}`.
4. The gateway checks Tailscale identity, origin and HTTPS ingress, then
   returns `{"ownerCapability": "<64 hex>", "secretCapability": "<…>"|null}`.
5. Glass stores `ownerCapability` in the Keychain
   (`kSecClassGenericPassword`, service `com.wanazhar.glass.owner`, account
   = origin, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not
   synchronizable). A `secretCapability` goes in a separate item
   (`com.wanazhar.glass.secret`, `WhenUnlockedThisDeviceOnly`). The token is
   zeroed in memory after the exchange.
6. Glass calls `GET /api/status` to confirm.

The web UI stores the capability in `localStorage` under
`gc-owner-capability-v1`. The Keychain replaces that. A `403` on any call
clears the stored capability for that origin and returns the user to pairing,
which matches `owner-auth.js`.

Local mode (Simulator tests only): `POST /api/local` with the bootstrap
token. Glass then sends `Authorization: Bearer <token>`, which
`proxy_authorized` accepts. This avoids cookie storage.

### 3.4 Secret capability

`POST /api/secret` and `GET /api/status` carry `X-Glass-Secret-Capability`
when one is held and the origin is HTTPS (`app.js` `api()`). Secret entry
requires HTTPS ingress and the private secret-entry link. The request body is
zeroed by the gateway. Native: `SecureField`, no autocorrect or keyboard
learning, screenshot redaction while visible, and the value is zeroed after
the send.

### 3.5 Events stream (`GET /api/events`)

- `text/event-stream`. Each frame is `id: {epoch}:{cursor}` then
  `data: {json}`.
- On reconnect the client sends `Last-Event-ID: {epoch}:{cursor}`. The
  gateway resumes from `cursor` if `epoch` matches. Otherwise, or if the
  cursor is 0, ahead of the server, or older than the retained window
  (`MAX_EVENTS = 1024`, `MAX_EVENT_BYTES = 1 MiB`), it sends a reset
  snapshot.
- Payloads carry replay or reset state, active status, control ownership,
  pending interaction, live data and an `events` array.
- The web reader (`ownerEventSource`) bounds frames and the buffer to 4 MiB
  and retries after 1 second. Native: an `AsyncSequence` over
  `URLSession.bytes(for:)`, the same 4 MiB bounds, retry with jitter (1 s,
  then up to 15 s), a cursor persisted per thread, cancellation on scene
  background, and resume on foreground with `Last-Event-ID`.
- `GET /api/agent-events` is a second SSE stream for teammate revisions and
  offline signals. It uses the same reader.

### 3.6 Limits the client must enforce first

| Limit | Value | Source |
| --- | --- | --- |
| JSON request body | 64 KiB (`MAX_BODY`) | `connect.rs` |
| Upload body | 8 MiB | `connect.rs` (`/api/upload`) |
| Artifact create body | 512 KiB | `connect.rs` (`/api/artifact`) |
| File read | 8 MiB | `files.rs` |
| Export | 64 MiB bound | `connect.rs` (`/api/export`) |
| Desktop frame | 4 MiB; stale after 1.8 s | `desktop.rs` |
| History record window | 512 KiB per record ("use export for full source") | `app.js` |

### 3.7 Desktop transport

- `GET /api/desktop/stream` is `multipart/x-mixed-replace; boundary=glassframe`.
  Each part has `Content-Type: image/jpeg`, `X-Frame-Seq` and
  `X-Frame-Time-Ms`. Empty parts are keep-alives.
- `GET /api/desktop/frame` returns a single frame (fallback).
- `POST /api/desktop/input` actions: `pointer-move`, `pointer-button`,
  `pointer-click`, `scroll`, `key`, `text`.
- `POST /api/desktop/session` (`start`, `stop`), `POST /api/desktop/profile`
  (`delete`), `POST /api/desktop/lease` (take or return input),
  `GET /api/desktop/status`, `POST /api/desktop/updates` (region updates;
  see `desktop-updates.js`).

## 4. Parity matrix

Status values:

- **NR**: native-ready. The current gateway API is enough.
- **GW**: needs a gateway or API change. See section 9.
- **NP**: not possible natively, or meaningless natively. The replacement is
  stated.

Milestone column: the phase in section 8 that delivers the row.

### 4.1 Launch, shell and platform

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Launch screen with status, detail, Retry | `index.html#launch`, `boot.js`, `launch.css` | `GET /api/status` | `LaunchScreen.storyboard` plus a SwiftUI `ConnectingView` with a Retry button | NR | Plain status call | M1 |
| 2 | Startup timing and web diagnostics | `startup-timing.js`, `web-diagnostics.js`, `diagnostic-events.js` | `GET /api/diagnostics`, `GET /api/history-record` | `os_signpost` and `Logger`; a Diagnostics screen that renders `/api/diagnostics` | NR | Read-only JSON | M1 |
| 3 | Service worker offline shell cache | `sw.js` | none | The app bundle is the shell; offline shows cached threads read-only | NP | No service workers natively; the app binary replaces them | M2 |
| 4 | "An update is ready" banner and Reload | `shell-update.js` | service worker messages | A "Gateway newer than app" banner driven by a protocol version from `/api/status` | GW | The native app needs a protocol version to detect drift (section 9, G1) | M1 |
| 5 | PWA manifest, install, standalone detection | `manifest.webmanifest`, `app.js` `display-mode: standalone` | none | Home Screen icon from `AppIcon` | NP | Not needed: the app is installed | M0 |
| 6 | Theme color in browser chrome | `theme-chrome.js`, `theme-head-entry.js` | none | `preferredColorScheme` and tint | NR | Native styling | M4 |
| 7 | Responsive layout (700 px and 1100 px breakpoints) | `app.js` `matchMedia`, `style.css` | none | `NavigationSplitView` (sidebar, timeline, details inspector) on iPad; stack on iPhone | NR | Size classes | M2 |
| 8 | Coarse-pointer adaptations | `app.js`, `desktop.js` `pointer: coarse` | none | Touch-first by default; pointer and hover on iPad trackpad | NR | `hoverEffect`, `onContinuousHover` | M7 |
| 9 | Bounded draft-preserving reload | `app.js` `reloadShell` | none | Drafts persist through process death (row 52) | NR | Local storage | M3 |

### 4.2 Auth and pairing

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 10 | Owner link `#owner=` (64 hex or 22-char) | `app.js` ~4747 | `POST /api/local` | QR scanner (`DataScannerViewController`), paste, share sheet; parse the fragment on the device | NR | Section 3.3 | M1 |
| 11 | Secret link `#secret=` | `app.js` ~4747 | `POST /api/local` (returns `secretCapability`) | Same pairing flow with a separate Keychain item | NR | HTTPS ingress required | M1 |
| 12 | Owner capability storage | `owner-auth.js` (`gc-owner-capability-v1`) | none | Keychain, per origin, this device only | NR | Section 3.3 | M1 |
| 13 | `X-Glass-Owner-Capability` on every `/api/` call | `owner-auth.js` `ownerFetch` | all | `GlassTransport` request middleware | NR | Same-origin only; never sent to other hosts | M1 |
| 14 | `Origin` on mutations | browser default | all `POST` | Middleware sets `Origin` to the paired origin | NR | Not a reserved header in `URLSession` | M1 |
| 15 | Tailscale identity | gateway (`tailscale-user-login`) | all | The Tailscale iOS app VPN; the app detects "not on tailnet" (DNS or connect failure to `*.ts.net`) and explains | NR | Tailscale Serve injects the header | M1 |
| 16 | Local mode cookie `gc_local` | `connect.rs` `/api/local` (local) | `POST /api/local` | Simulator test mode using `Authorization: Bearer` | NR (Simulator) / NP (device) | Loopback cannot be reached from a phone | M1 |
| 17 | LAN HTTP origin unlock (`settings.http > 0`) | `same_origin_owner_unlock` | `POST /api/local` | Supported behind an "Allow plain HTTP on local network" toggle; ATS `NSAllowsLocalNetworking` | NR | ATS exception needed; secrets stay disabled (HTTPS only) | M1 |
| 18 | Clear capability on 403, re-pair | `owner-auth.js` | all | Same; also "Forget this host" in Settings | NR | | M1 |
| 19 | Multiple hosts | web is one origin per tab | none | Host list in Settings with a Keychain item per origin | NR | Beyond web parity, cheap | M7 |

### 4.3 Threads and sidebar

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 20 | Thread list with count | `#threads`, `#thread-count` | `GET /api/threads` | `List` in the sidebar column | NR | | M2 |
| 21 | More threads (paging) | `#more-threads` | `GET /api/threads` | Infinite scroll | NR | | M2 |
| 22 | Search conversations | `#search`, `⌘K` | `GET /api/threads` (query) | `.searchable`, `⌘K` key command | NR | | M2 |
| 23 | Project scope filter | `#project-scope` | `GET /api/threads` | Picker in the sidebar header | NR | | M2 |
| 24 | New thread | `#new`, `#welcome-new` | `POST /api/new` | Toolbar button, `⌘N` | NR | | M2 |
| 25 | Open thread (including other origins) | `open_thread`, `#open-chat` | `POST /api/open` | Row tap; `sourceRef` kept in the model | NR | | M2 |
| 26 | Close thread | `app.js` | `POST /api/close` | Swipe action and context menu | NR | | M2 |
| 27 | Thread title and origin label | `#thread-name`, `#thread-origin`, `#workspace-name` | `/api/threads`, `meta` event | Navigation title and subtitle | NR | | M2 |
| 28 | Rename thread | command `name` | `POST /api/send` `{t:"command"}` | Rename in the context menu | NR | Through the command path | M4 |
| 29 | Mobile sidebar, scrim, Tab focus trap | `#sidebar`, `#scrim`, `#menu` | none | `NavigationSplitView` column visibility | NR | | M2 |
| 30 | Welcome state (New, Files) | `#welcome` | none | Empty-state view | NR | | M2 |

### 4.4 Conversation, history and events

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 31 | Initial and compact history | `app.js` ~1140 | `GET /api/history?id&sourceRef&compact=1` | `HistoryLoader` into the timeline model | NR | | M2 |
| 32 | Load older history | `#older` | `GET /api/history?before=` | Scroll-to-top loader; scroll position kept | NR | | M2 |
| 33 | Oversized record placeholder | `app.js` ~1234, 1284 | `GET /api/activity-text`, `GET /api/export` | "Read large activity" sheet; Export | NR | | M5 |
| 34 | Live events with resume | `owner-auth.js` `ownerEventSource` | `GET /api/events` + `Last-Event-ID` | `EventStream` (section 3.5) | NR | | M2 |
| 35 | Reset snapshot handling | `app.js` `replay`, `state` | `/api/events` | Reducer replaces the thread state on reset | NR | | M2 |
| 36 | Follow latest and Latest button | `#latest` | none | `ScrollViewReader` with an auto-follow flag | NR | | M2 |
| 37 | Rendering for event types: `user`, `assistant`, `text`, `reasoning`, `tool`, `tool_call`, `tool_end`, `call`, `bash`, `image`, `custom`, `meta`, `state`, `turn_start`, `done`, `waiting`, `queued`, `steering`, `retrying`, `duplicate`, `error`, `provider_error`, `provider_notice`, `disconnected`, `control`, `interaction`, `interaction_done`, `command_catalogue`, `command_queued`, `command_start`, `command_result`, `command_done`, `open_thread`, `replay` | `app.js` reducer | `/api/events`, `/api/history` | One `GlassEvent` enum (unknown cases decode to `.unknown` and render as a generic card); one SwiftUI row view per case | NR | Tolerant decoding prevents crashes when the gateway adds types | M2 (core), M3–M5 (rest) |
| 38 | Markdown and code blocks | `app.js`, `syntax.js` | none | `AttributedString(markdown:)` for prose; a native highlighter for code | NR | | M2 |
| 39 | Math | `math.js` (lazy) | none | `WKWebView` with the same renderer bundled offline | NR | No native TeX layout; a bundled renderer keeps parity | M5 |
| 40 | Mermaid diagrams | `mermaid.js` (lazy) | none | Bundled offline `WKWebView` renderer | NR | Same reason | M5 |
| 41 | PDF preview | `pdf.js` (lazy) | `GET /api/file`, `/api/attachment` | `PDFKit` `PDFView` | NR | | M5 |
| 42 | Images in timeline | `image` event | `GET /api/attachment` | `AsyncImage` through the authenticated loader | NR | `AsyncImage` cannot add headers; use a custom loader | M3 |
| 43 | Reasoning cards ("Thinking", "Thoughts") and reader | `app.js` ~680, `reasoning-reader.js`, `#reasoning-latest` | events | Collapsible row and a full-screen reader | NR | | M5 |
| 44 | Activity disclosure and tool density | `activity.js`, `activity-disclosure.js` | events | `DisclosureGroup`; density preference (row 84) | NR | | M3 |
| 45 | Activity location jump and edit action | `app.js` ~805 | `GET /api/activity-location` | Context-menu action with a confirmation dialog | NR | | M5 |
| 46 | Copy message or code | `app.js` `navigator.clipboard` | none | `UIPasteboard`, `.textSelection(.enabled)` | NR | | M3 |
| 47 | Task checklist | `#task-checklist` | events, command `todo`/`tasks` | Pinned checklist section | NR | | M3 |
| 48 | Notice banner | `#notice` | none | Toast overlay | NR | | M2 |

### 4.5 Composer and send

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 49 | Prompt send | `#prompt`, `#send` | `POST /api/send {t:"prompt"}` | Multiline `TextField(axis:.vertical)`, Send button | NR | | M3 |
| 50 | Enter sends, Shift+Enter newline | `app.js` | none | Hardware keyboard: `onKeyPress(.return)` checks Shift; software keyboard: Return inserts a newline and the Send button sends | NR | Software keyboard has no Shift+Return | M3 |
| 51 | Steer vs queue while active (`#send-mode`) | `app.js` ~2214, ~4897 | `{t:"steer"}` or queued prompt | Segmented toggle shown while a turn is active | NR | | M3 |
| 52 | Bounded device-local drafts (24 drafts, 30 days, 60,000 chars) | `drafts.js` | none | `Application Support` JSON store with the same bounds | NR | | M3 |
| 53 | Draft editor (full screen) | `#draft-editor` | none | Full-screen `TextEditor` sheet | NR | | M3 |
| 54 | Stop | `#stop` | `{t:"cancel"}` | Stop button | NR | | M3 |
| 55 | Cancel command | `app.js` ~4089 | `{t:"cancel_command", request_id}` | Cancel in the command view | NR | | M4 |
| 56 | Attach files | `#attach`, `#attachment-input` | `POST /api/upload` (8 MiB) | `PhotosPicker`, `.fileImporter`, camera, drag-and-drop on iPad | NR | Client size check before upload | M3 |
| 57 | Upload previews and progress | `#upload-previews`, `owner-auth.js` XHR | `POST /api/upload` | `URLSession` upload task with progress | NR | | M3 |
| 58 | Context chips | `#context-chips` | `GET /api/context` | Chip row above the composer | NR | | M3 |
| 59 | Model selector | `#model` (`select.js`) | settings, command `model` | `Menu` picker | NR | | M3 |
| 60 | Profile selector | `#profile` | settings, command `profiles` | `Menu` picker | NR | | M3 |
| 61 | Voice input | `#voice-input`, `app.js` `SpeechRecognition` | none | `SFSpeechRecognizer` with on-device recognition; mic and speech usage strings | NR | Needs two Info.plist keys | M3 |
| 62 | Context, Work, Queue toggles | `#context-toggle`, `#work-toggle`, `#queue-toggle` | `/api/context`, events | Toolbar buttons opening sheets | NR | | M3 |
| 63 | Workspace location | `#workspace-location` | `/api/status` | Footer label | NR | | M3 |
| 64 | Usage | `#usage`, command `usage` | `GET /api/usage` | Footer label and a usage sheet | NR | | M5 |

### 4.6 Status, control and interaction

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 65 | Host and connection indicator | `#host`, `#connection` | `/api/status`, stream state | Toolbar status dot with detail popover | NR | | M1 |
| 66 | Activity row and text | `#activity`, `#activity-text` | events | Bottom status row | NR | | M2 |
| 67 | Control ownership and handoff (Take / Release) | `#handoff`, `#ownership`, `app.js` ~2285, `serve.rs` `can_write` | `{t:"control", action:"claim"|"release"}` | Banner with Take control / Release | NR | | M3 |
| 68 | Read-only state when not in control | `serve.rs` | events (`control`) | Composer disabled with a reason | NR | | M3 |
| 69 | Interaction prompts (choice, text) | `#interaction`, `app.js` ~2320 | `{t:"answer", id, answer}` | Inline card or sheet | NR | | M3 |
| 70 | Secret interaction | `app.js` ~2328 | `POST /api/secret` + secret capability | `SecureField` sheet, redacted in the app switcher | NR | HTTPS required | M3 |
| 71 | Thread runtime restart | gateway | `POST /api/upgrade {id, sourceRef, confirmed:true}` | "Restart runtime" with a confirmation dialog | NR | Not wired in the web UI today; native adds it with the confirmation the gateway requires | M5 |

### 4.7 Commands

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 72 | Command palette (groups: Conversations, Work, Settings), search, help | `#commands`, `#command-search`, `#command-list`, `#command-help`, `#open-commands` | `GET /api/commands` | Searchable sheet; `⌘K`/`⌘/` | NR | | M4 |
| 73 | Palette keyboard: arrows, Home/End, Enter, Space, Escape back | `app.js`, `select.js` | none | `onKeyPress` and `focusable` list; `UIKeyCommand` for Escape | NR | | M4 |
| 74 | Command forms | `#command-form` | catalogue form schema, `POST /api/command` | Generated `Form` from the schema | NR | | M4 |
| 75 | Command run, status, result | `#command-view`, `#command-status`, `#command-result`, `command-results.js` | `POST /api/send {t:"command", name, values, request_id}`, `POST /api/command`, `command_*` events | Command sheet with a progress state and result renderer | NR | | M4 |
| 76 | Read-only commands: usage, requests, history, recap, recovery, diagnostics, guidance, turns, review, skills, skill-doctor, prompts, attention, branches, checkpoints | `web_commands.rs` | `POST /api/command` | Same renderer | NR | | M4 |
| 77 | Supported commands: handoff, doctor, tasks, search, context, settings-info, status, name, forget, unforget, rewrite, compact, attach, file, quote, export, constraints, loop, send, branch, checkpoint, diff, undo, restore, todo, budget, pacing, model, prewalk, mods, jobs, job, bg, security, neuralnets, profiles, login, logout, plan, init, btw, code-review, advisor, pr, git, capture, rewind, extensions, agents, retry, queue, queue-run | `web_commands.rs`, `actions.rs` | `POST /api/send {t:"command"}` | Generic command runner; no per-command native code | NR | The catalogue drives the UI, so new commands appear without an app update | M4 |
| 78 | Local palette actions: help, layout, clear/new, tools density, draft editor, animation, conversations, work, settings, desktop, outputs, thinking, context, status, usage, todo, tasks, agents, attention, queue, theme, redraw, search, keys | `app.js` palette | none or the listed APIs | Mapped to native navigation actions | NR | | M4 |
| 79 | Export download | `app.js` ~4181 | `GET /api/export` | `fileExporter` or `ShareLink` with a temporary file | NR | | M5 |
| 80 | Command login provider fields (`api`, `callback`, `create`) | `app.js` ~3965–4027 | command forms | Form fields; secrets use `SecureField` | NR | | M4 |

### 4.8 Settings and preferences

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 81 | Settings page with sections General, Connections, Tools & skills, Instructions, Appearance, Advanced | `#settings-page`, `#settings-sections`, `settings.js` | `GET /api/settings` | `Form` with sections | NR | | M4 |
| 82 | Fields: provider, base URL, model, max tokens, reasoning budget, cost limit, adaptive pacing, image input, system prompt, skills directory, permission profile, sandbox, mods, active connection, connections, extensions, profiles, skills | `web_settings.rs` | `GET`/`POST /api/settings` | Typed controls; validation mirrors the gateway | NR | | M4 |
| 83 | Revisioned save, conflict, "unsaved edits are retained" refresh | `settings.js` ~57, ~170 | `POST /api/settings {revision, …}` | Same model; conflict alert keeps local edits | NR | | M4 |
| 84 | Appearance: theme system/light/dark (`glass:theme`), animation (`glass:animation`, default `dots`), tools compact/expanded (`glass:tools`) | `app.js` ~2994, ~4309, ~4509, `#theme`, `#animations` | none | `@AppStorage`; animation respects Reduce Motion | NR | | M4 |
| 85 | Working-animation preview dialog | `#animations` | none | Picker with live preview | NR | | M4 |
| 86 | Notifications toggle (`glass.notifications.v1`) | `notifications.js`, `#notifications` | none | `UNUserNotificationCenter` local notifications while connected | NR | Web is also connected-only ("Background iPhone push is not provided") | M7 |
| 87 | Background push when the app is closed | not in web | none | APNs | GW | Needs gateway push support and an APNs key; beyond web parity (G4) | later |

### 4.9 Details panel (Activity, Reasoning, Context, Files) and teammates

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 88 | Details panel with tabs | `#details`, `#details-toggle`, `#detail-content`, `workflows.js` | several | iPad `inspector`; iPhone sheet with tabs | NR | | M5 |
| 89 | Activity tab and paged activity text | `workflows.js` ~923 | `GET /api/activity-text?recordOffset` | Paged text view | NR | | M5 |
| 90 | Context tab | `workflows.js` ~820 | `GET /api/context` | List | NR | | M5 |
| 91 | Files tab and file tree with paging | `file-tree.js`, `#files-toggle` | `GET /api/browse?path&offset` | `OutlineGroup` with lazy children | NR | | M5 |
| 92 | File view | `app.js`, `workflows.js` | `GET /api/file` (8 MiB) | Text, image, PDF or Quick Look | NR | | M5 |
| 93 | Edits view | `workflows.js` ~211 | `GET /api/edit?path` | Diff view | NR | | M5 |
| 94 | Changes view | `workflows.js` ~300, `#changes-toggle` | `GET /api/changes` | Changed-file list with diffs | NR | | M5 |
| 95 | Teammates list and transcript | `workflows.js` ~730, ~759, `agent-ui.js`, `agent-transcript.js` | `GET /api/agents`, `GET /api/agent?agent=` | List and transcript screen | NR | | M5 |
| 96 | Teammate status strip with live updates | `teammate-status.js`, `#teammate-status` | `GET /api/agents`, `GET /api/agent-events` (SSE) | Strip above the composer; second `EventStream` | NR | | M5 |
| 97 | Agent activity indicator | `#agent-activity` | events | Toolbar badge | NR | | M5 |

### 4.10 Previews, attachments and artifacts

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 98 | Preview dialog with kind, title, Download | `#preview`, `#download` | `GET /api/file`, `/api/attachment` | Quick Look (`QLPreviewController`) and `ShareLink` | NR | | M5 |
| 99 | Attachments | `app.js` | `GET /api/attachment` | Authenticated download to a cache | NR | | M3 |
| 100 | Create artifact | `app.js` | `POST /api/artifact` (512 KiB) | Same call | NR | | M5 |
| 101 | Interactive HTML artifacts (sandboxed CSP document) | `connect.rs` `artifact_document` | `GET /api/artifact` | `WKWebView` loading through a `WKURLSchemeHandler` that fetches with owner headers; JavaScript allowed, no network; nonpersistent data store | NR | Scheme handler supplies headers, which `WKWebView` subresource loads cannot | M5 |

### 4.11 Shared desktop

| # | Web feature | Source | API | Native equivalent | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 102 | Desktop toggle and viewer | `#desktop-toggle`, `#desktop-viewer`, `desktop.js` | `GET /api/desktop/status` | Full-screen cover | NR | | M6 |
| 103 | Start, stop, retry, reconnect | `#desktop-start`, `#desktop-stop`, `#desktop-retry`, `#desktop-reconnect` | `POST /api/desktop/session` | Buttons | NR | | M6 |
| 104 | Environment picker: quick vs reusable profile; delete profile | `#desktop-environment-picker`, `#desktop-preset-*`, `#desktop-profile-delete` | `POST /api/desktop/session`, `POST /api/desktop/profile` | Picker; delete with confirmation | NR | | M6 |
| 105 | Input lease: owner, take, return | `#desktop-owner`, `#desktop-take`, `#desktop-return` | `POST /api/desktop/lease` | Lease banner | NR | | M6 |
| 106 | MJPEG stream with sequence and time | `#desktop-stream` | `GET /api/desktop/stream` | Multipart parser to `CGImage`, shown in a `UIView` layer; stale indicator after 1.8 s | NR | Section 3.7 | M6 |
| 107 | Single-frame fallback | `desktop.js` | `GET /api/desktop/frame` | Same | NR | | M6 |
| 108 | Region updates | `desktop-updates.js`, `#desktop-regions` | `POST /api/desktop/updates` | Same | NR | | M6 |
| 109 | Trackpad, touch and drag modes | `#desktop-trackpad`, `#desktop-touch-mode`, `#desktop-drag-mode`, `#desktop-touch-hint` | `POST /api/desktop/input` | UIKit gesture recognizers (relative trackpad, absolute tap, long-press drag) | NR | | M6 |
| 110 | Pointer move, button, click, scroll | `desktop.js` | `pointer-move`, `pointer-button`, `pointer-click`, `scroll` | Gestures and iPad pointer or trackpad events (`UIHoverGestureRecognizer`, two-finger scroll) | NR | | M6 |
| 111 | Keyboard, modifiers (Control, Alt), shortcut row, text send | `#desktop-keyboard-open`, `#desktop-mod-*`, `#desktop-shortcuts`, `#desktop-keyboard-row`, `#desktop-text`, `#desktop-send-text` | `key`, `text` | Input accessory bar; hardware keys through `pressesBegan` | NR | Some system shortcuts (for example `⌘Tab`, `⌘Space`, Globe) are taken by iOS and never reach the app | M6 |
| 112 | Zoom, pan, view reset, cursor overlay | `#desktop-view-reset`, `#desktop-cursor` | none | `UIScrollView` zoom; cursor layer | NR | | M6 |
| 113 | Desktop status, resolution, details | `#desktop-status`, `#desktop-resolution`, `#desktop-details` | `GET /api/desktop/status` | Info popover | NR | | M6 |

### 4.12 Keyboard shortcuts

| # | Web shortcut | Source | Native | Status | Why | M |
| --- | --- | --- | --- | --- | --- | --- |
| 114 | Enter / Shift+Enter in composer | `app.js` | Row 50 | NR | | M3 |
| 115 | `⌘K` / `Ctrl+K` search | `app.js` | `.keyboardShortcut("k")` and the iPad shortcut overlay | NR | | M4 |
| 116 | Escape closes view or goes back in palette | `app.js` | `.keyboardShortcut(.escape)`, `onExitCommand` | NR | | M4 |
| 117 | Arrows, Home, End, Enter, Space in lists and selects | `app.js`, `select.js` | `onKeyPress`, focus | NR | | M4 |
| 118 | Desktop viewer key and modifier mapping | `desktop.js` | Row 111 | NR (partial) | Some system shortcuts are reserved by iOS | M6 |
| 119 | Added: `⌘N` new thread, `⌘.` stop, `⌘,` settings | none | Key commands | NR | Native convention | M7 |

### 4.13 Every API call, by route

| Route | Web caller | Native caller (GlassClientKit) | Status | M |
| --- | --- | --- | --- | --- |
| `POST /api/local` | `app.js` boot | `Pairing.exchange(link:)` | NR | M1 |
| `GET /api/status` | `app.js` | `client.status()` | NR (+G1 version field) | M1 |
| `GET /api/diagnostics` | `app.js` | `client.diagnostics()` | NR | M1 |
| `GET /api/settings` | `settings.js` | `client.settings()` | NR | M4 |
| `POST /api/settings` | `settings.js` | `client.updateSettings(revision:change:)` | NR | M4 |
| `GET /api/commands` | `app.js` | `client.commandCatalogue()` | NR | M4 |
| `POST /api/command` | `app.js` | `client.runCommand(_:)` | NR | M4 |
| `GET /api/threads` | `app.js` | `client.threads(query:scope:cursor:)` | NR | M2 |
| `POST /api/new` | `app.js` | `client.newThread()` | NR | M2 |
| `POST /api/open` | `app.js` | `client.open(id:sourceRef:)` | NR | M2 |
| `POST /api/close` | `app.js` | `client.close(id:)` | NR | M2 |
| `POST /api/send` | `app.js` (`prompt`, `steer`, `cancel`, `cancel_command`, `control`, `answer`, `command`) | `client.send(_ message: SendMessage)` | NR | M3 |
| `GET /api/history` | `app.js` | `client.history(id:before:compact:)` | NR | M2 |
| `GET /api/history-record` | `diagnostic-events.js` | `client.historyRecord(...)` | NR | M5 |
| `GET /api/events` (SSE) | `owner-auth.js` | `EventStream` | NR | M2 |
| `GET /api/agents` | `teammate-status.js`, `workflows.js` | `client.agents(...)` | NR | M5 |
| `GET /api/agent` | `workflows.js` | `client.agent(id:)` | NR | M5 |
| `GET /api/agent-events` (SSE) | `teammate-status.js` | `EventStream` | NR | M5 |
| `GET /api/activity-location` | `app.js` | `client.activityLocation(...)` | NR | M5 |
| `GET /api/activity-text` | `workflows.js` | `client.activityText(...)` | NR | M5 |
| `GET /api/context` | `workflows.js`, `app.js` | `client.context(...)` | NR | M3 |
| `POST /api/upload` | `owner-auth.js` | `client.upload(_:progress:)` | NR | M3 |
| `GET /api/file` | `app.js`, `file-tree.js`, `workflows.js` | `client.file(path:)` | NR | M5 |
| `GET /api/attachment` | `app.js` | `client.attachment(...)` | NR | M3 |
| `GET /api/edit` | `workflows.js` | `client.edit(path:)` | NR | M5 |
| `GET /api/browse` | `file-tree.js` | `client.browse(path:offset:)` | NR | M5 |
| `GET /api/usage` | `app.js`, `command-results.js` | `client.usage()` | NR | M5 |
| `GET /api/changes` | `workflows.js` | `client.changes(...)` | NR | M5 |
| `POST /api/artifact` | `app.js` | `client.createArtifact(_:)` | NR | M5 |
| `GET /api/artifact` | `app.js` | `ArtifactSchemeHandler` | NR | M5 |
| `GET /api/export` | `app.js` | `client.export(id:)` | NR | M5 |
| `POST /api/secret` | `app.js` | `client.submitSecret(_:)` | NR | M3 |
| `POST /api/upgrade` | not called by web | `client.restartRuntime(id:)` | NR | M5 |
| `GET /api/desktop/status` | `desktop.js` | `DesktopClient.status()` | NR | M6 |
| `POST /api/desktop/session` | `desktop.js` | `DesktopClient.start/stop` | NR | M6 |
| `POST /api/desktop/profile` | `desktop.js` | `DesktopClient.deleteProfile` | NR | M6 |
| `POST /api/desktop/lease` | `desktop.js` | `DesktopClient.take/return` | NR | M6 |
| `POST /api/desktop/input` | `desktop.js` | `DesktopClient.send(_ input:)` | NR | M6 |
| `GET /api/desktop/frame` | `desktop.js` | `DesktopClient.frame()` | NR | M6 |
| `GET /api/desktop/stream` | `desktop.js` | `DesktopFrameStream` | NR | M6 |
| `POST /api/desktop/updates` | `desktop-updates.js` | `DesktopClient.updates(...)` | NR | M6 |

### 4.14 Summary

- 119 feature rows and 41 routes.
- NP rows: 3 (service worker), 5 (PWA install), 16 on device (loopback
  local mode). Each has a native replacement or does not apply.
- GW rows: 4 (protocol version for drift detection) and 87 (background push,
  beyond web parity).
- Partial: 111 and 118 (iOS reserves some system shortcuts).
- Everything else works against the current gateway. The only header work
  is that the client must send `Origin` on `POST`, which iOS allows.

## 5. GlassClientKit shared package

Location: `apps/shared/GlassClientKit`, a Swift package (tools 6.0, Swift 6
language mode with strict concurrency, platforms `.iOS(.v18)` and
`.macOS(.v15)` so pure logic tests can run with `swift test`). No third-party
dependencies.

| Target | Contents | Used by |
| --- | --- | --- |
| `GlassTransport` | `GlassEndpoint` (origin, mode), `AuthScheme` protocol with `OwnerCapabilityAuth` (Glass) and `BearerAuth` (Marble and local mode), request builder that sets `Origin`/`Content-Type` on `POST`, body-limit checks, typed `GlassAPIError` (403 → `.unauthorized`, network, decode, gateway `{"error":…}`), retry policy | Glass, Marble |
| `GlassAuth` | `OwnerLink` parser (`#owner=`, `#secret=`, 64-hex and 22-char forms), `PairingService` (`/api/local`), `CapabilityStore` protocol with `KeychainCapabilityStore` and `InMemoryCapabilityStore` (tests) | Glass, Marble |
| `GlassEvents` | `SSEParser` (bytes to frames; `id`, `data`, `event`, `retry`, comments; 4 MiB bounds), `EventStream` (`AsyncThrowingStream`, reconnect with backoff, `Last-Event-ID` cursor `{epoch}:{cursor}`, reset detection) | Glass, Marble |
| `GlassModels` | `Codable` models: status, diagnostics, thread summary, history page, `GlassEvent` enum with `.unknown(raw:)`, send messages, interaction, control, command catalogue and forms, command result, settings snapshot and update, context, browse, file, usage, agents | Glass, Marble (subset) |
| `GlassAPI` | `GlassClient` actor: one async method per route in 4.13 | Glass, Marble |
| `GlassDesktop` | Multipart JPEG frame stream parser, `DesktopInput` encoder, lease and session calls | Glass |
| `GlassTestSupport` | `StubURLProtocol`, recorded gateway fixtures (JSON and SSE transcripts), fake clock | Tests in both apps |

Rules:

- The package has no UI and no global state. App stores own UI state.
- All public types are `Sendable`. `GlassClient` is an `actor`.
- Unknown JSON fields are ignored and unknown event types decode to
  `.unknown`, so a newer gateway never crashes an older app.
- Fixtures are recorded from a real `glass-core connect` run and kept next to
  the tests. A contract test fails when the gateway's route list in
  `connect.rs` changes without a matching client update (it greps the route
  strings during the test run).

Marble adoption (separate PR, after M2): Marble replaces `MarbleClient` HTTP
and JSON helpers with `GlassTransport` + `BearerAuth` and its stream code with
`GlassEvents`. Marble keeps its own models where its server differs. Marble's
bundle (`com.wanazhar.glassmarble`) and UI stay unchanged. The Marble app is
not on `main` today, so this depends on it landing again (question Q8).

## 6. App architecture

```text
apps/glass-ios/
  Glass.xcodeproj            (checked in; no generator)
  Glass/
    GlassApp.swift           (@main, scene phase handling)
    Base.lproj/LaunchScreen.storyboard
    Assets.xcassets          (AppIcon, AccentColor, symbols)
    Info.plist keys          (generated + explicit keys below)
    Features/Pairing, Threads, Timeline, Composer, Commands,
             Settings, Details, Desktop, Diagnostics
    Stores/                  (@Observable, @MainActor)
  GlassTests/                (unit, XCTest/Swift Testing)
  GlassUITests/              (XCUITest)
  scripts/package-unsigned-ipa.sh
```

- State: `@Observable` stores per feature (`SessionStore`, `ThreadListStore`,
  `TimelineStore`, `ComposerStore`, `DesktopStore`), fed by `GlassClient` and
  `EventStream`.
- Scene phase: on background, cancel streams and save cursors. On
  foreground, resume with `Last-Event-ID`. A short
  `beginBackgroundTask` lets an in-flight send or upload finish.
- Info.plist keys: `NSCameraUsageDescription` (QR pairing, photo attach),
  `NSMicrophoneUsageDescription` and `NSSpeechRecognitionUsageDescription`
  (voice), `NSPhotoLibraryUsageDescription` is not needed with
  `PhotosPicker`, `NSLocalNetworkUsageDescription` and ATS
  `NSAllowsLocalNetworking` (LAN HTTP only), all orientations,
  `UILaunchStoryboardName`.
- Entitlements: none required for an unsigned build. Keychain uses the
  default access group, which the signing team sets. Sharing Keychain items
  with Marble would need `keychain-access-groups` (question Q5).
- Accessibility: Dynamic Type, VoiceOver labels taken from the web
  `aria-label` strings, Reduce Motion for working animations.

## 7. Build and unsigned IPA pipeline

Every phase uses the same commands. They are proven on this Mac (section 2).

```sh
# Unit + UI tests (Simulator, this Mac)
xcodebuild test -project apps/glass-ios/Glass.xcodeproj -scheme Glass \
  -destination 'platform=iOS Simulator,name=iPhone 17,OS=26.5'
xcodebuild test -project apps/glass-ios/Glass.xcodeproj -scheme Glass \
  -destination 'platform=iOS Simulator,name=<first available iPad>,OS=26.5' \
  -only-testing:GlassUITests
swift test --package-path apps/shared/GlassClientKit

# Unsigned device build
xcodebuild -project apps/glass-ios/Glass.xcodeproj -scheme Glass \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  -derivedDataPath build/glass-ios CODE_SIGNING_ALLOWED=NO build

# Package
rm -rf build/ipa && mkdir -p build/ipa/Payload
cp -R build/glass-ios/Build/Products/Release-iphoneos/Glass.app build/ipa/Payload/
(cd build/ipa && ditto -c -k --sequesterRsrc --keepParent Payload \
  "Glass-M<n>-<version>-unsigned.ipa")
shasum -a 256 build/ipa/*.ipa
```

Packaging gate (all must pass before a phase IPA is handed over):

- `Assets.car` and `LaunchScreen.storyboardc` are in the IPA.
- `plutil` shows `CFBundleIdentifier=com.wanazhar.glass`,
  `CFBundleDisplayName=Glass`, `MinimumOSVersion=18.0`, and the expected
  `CFBundleShortVersionString`/`CFBundleVersion`.
- `codesign -dv` reports unsigned. `lipo -info` reports arm64.
- No `embedded.mobileprovision`, no secrets, no test fixtures in the bundle.
- The SHA-256 is recorded in the phase notes.

The owner signs the IPA himself (for example with his own certificate and
profile, or a sideloading tool). The simulator iPad device name depends on
installed device types; the script picks the first available iPad.

## 8. Phased milestones and test gates

Each phase ends with: green unit tests, green Simulator UI tests on this Mac
(iPhone 17 and one iPad, iOS 26.5; iOS 18 runtime too if installed, Q10),
a live-host e2e run, the packaging gate in section 7, and an unsigned IPA.

Test hosts:

- **Stub**: `StubURLProtocol` with recorded fixtures. Fully deterministic.
- **Local gateway**: `glass-core connect` in local mode on this Mac. The
  Simulator reaches it over loopback with `BearerAuth`. Used for UI
  integration tests.
- **Live host**: the owner's real Connect over Tailscale HTTPS. This Mac is
  on the tailnet; the Simulator shares the Mac's network, so Tailscale Serve
  adds the identity header. Pairing uses an owner link supplied at run time
  through an environment variable in the test plan, never committed. After
  the owner signs, the same checklist runs once on a physical iPhone.

### M0: scaffolding and pipeline

- Scope: `Glass.xcodeproj` (storyboard launch screen, asset catalog, all
  orientations), `GlassClientKit` package skeleton, test targets, packaging
  script, a placeholder home screen showing the app version.
- Unit: package builds; one model decode test.
- UI: app launches and shows the placeholder on iPhone and iPad.
- E2E: none (no network yet); record "N/A by design".
- IPA: `Glass-M0` installs and launches after the owner signs it.

### M1: pairing, auth, status, diagnostics

- Scope: rows 1, 2, 4, 10–18, 65. `GlassTransport`, `GlassAuth`, Keychain
  store, QR and paste pairing, `/api/status`, diagnostics screen, "not on
  tailnet" message, 403 re-pair, LAN HTTP toggle.
- Unit: owner-link parsing (both token forms, `#secret=`, bad input), HMAC
  shape check (64 hex), `Origin` and `Content-Type` set on every `POST` and
  absent capability on other hosts, Keychain round trip and per-origin
  isolation, 403 clears the capability, body-limit rejection.
- UI: pair against the local gateway; wrong token shows the gateway error;
  relaunch keeps pairing; Forget host returns to pairing.
- E2E: pair with the live host from a real owner link; `/api/status` shows
  the host; revoke by rotating the gateway token, then confirm the app
  returns to pairing.
- IPA: `Glass-M1`.

### M2: threads, history, live events

- Scope: rows 3, 7, 20–27, 29–32, 34–38, 48, 66. Sidebar, search, scope,
  new, open, close, history and older history, `EventStream` with resume,
  reset handling, core event rendering, follow latest.
- Unit: `SSEParser` (split frames across chunks, CRLF, comments, multi-line
  `data`, 4 MiB overflow), cursor persistence, reconnect sends
  `Last-Event-ID`, reset snapshot replaces state, every event type in
  fixtures decodes, unknown type decodes to `.unknown`, history paging merge
  without duplicates.
- UI: list, search, open, scroll to load older, new thread appears, kill
  the local gateway and restart it: the banner shows "reconnecting" and the
  timeline continues without duplicates.
- E2E: open a real thread; send from the web UI and see it live in the app;
  background the app for 2 minutes, return, and confirm resume without a
  full reload; force a window overflow (more than 1,024 events) and confirm
  a clean reset.
- IPA: `Glass-M2`.

### M3: composer, send, control, interactions, uploads

- Scope: rows 9, 42, 44, 46, 47, 49–54, 56–63, 67–70, 99.
- Unit: every `SendMessage` encodes to the exact gateway JSON (`prompt`,
  `steer`, `cancel`, `control` claim/release, `answer`), draft store bounds
  (24, 30 days, 60,000 chars), upload size check (8 MiB), secret buffer is
  cleared after send.
- UI: type and send, Shift+Return with a hardware keyboard (Simulator
  keyboard events), stop, steer vs queue, take and release control, answer
  an interaction, attach a photo and a file with progress, voice button
  permission flow, draft survives app termination.
- E2E: full turn on the live host with an attachment; take control from the
  web UI and see the app become read-only; secret prompt over HTTPS.
- IPA: `Glass-M3`.

### M4: commands, settings, appearance

- Scope: rows 6, 28, 55, 72–78, 80–85, 115–117.
- Unit: catalogue decode, form schema to field mapping for every field type
  in fixtures, `command_*` event sequence handling, settings revision
  conflict keeps local edits, appearance preferences persist.
- UI: open the palette with `⌘K`, search, run a read-only command, run a
  form command, cancel a running command, edit and save a setting, trigger
  a conflict, switch theme, density and animation.
- E2E: run `status`, `usage`, `todo` and `name` on the live host; change a
  setting and confirm in the web UI that it applies to new conversations.
- IPA: `Glass-M4`.

### M5: details, files, teammates, previews, artifacts, rich rendering

- Scope: rows 33, 39–41, 43, 45, 64, 71, 79, 88–98, 100, 101.
- Unit: browse paging, activity-text paging, file size cap (8 MiB) and content-type mapping for
  file reads, artifact scheme handler adds headers and blocks other hosts,
  export writes a temporary file and removes it.
- UI: open each details tab; browse and preview a file; open a PDF; show a
  teammate transcript; render math and Mermaid offline (airplane mode in
  Simulator by stopping the local gateway after load); open an interactive
  artifact.
- E2E: on the live host, teammate status updates live through
  `/api/agent-events`; export a thread and open it in Files; restart a
  thread runtime with confirmation.
- IPA: `Glass-M5`.

### M6: shared desktop

- Scope: rows 102–113, 118.
- Unit: multipart JPEG parser (boundary split across chunks, empty
  keep-alive parts, 4 MiB cap, sequence ordering), input encoder for every
  action, coordinate mapping under zoom and pan, stale detection at 1.8 s.
- UI: start a quick desktop on the local gateway, take the lease, tap,
  drag, scroll, type text, send a modifier shortcut, zoom and reset, return
  the lease, stop.
- E2E: on the live host, start a reusable profile, interact, measure frame
  rate and input latency (target: no visible stall over 60 s on Wi-Fi),
  delete a profile.
- IPA: `Glass-M6`.

### M7: notifications, iPad polish, accessibility, Marble adoption, sign-off

- Scope: rows 8, 19, 86, 119; Marble moves to `GlassClientKit` (separate
  PR); full parity walk-through.
- Unit: notification only fires when the app is not active; key command
  table has no duplicates.
- UI: VoiceOver audit (`performAccessibilityAudit()` in XCUITest) on every
  screen; Dynamic Type at the largest size; iPad split view and Stage
  Manager resize.
- E2E: walk every row in section 4 on the live host on a physical iPhone and
  iPad (owner-signed build); record pass or fail per row in the PR.
- IPA: `Glass-M7` (release candidate).

## 9. Required and recommended gateway changes

None is required to reach web parity. These are recommended:

| ID | Change | Why | Needed by |
| --- | --- | --- | --- |
| G1 | Add `protocolVersion` (integer) and `capabilities` (string list) to `GET /api/status` | The web UI updates itself through the service worker; a native app cannot, so it must detect an incompatible gateway and say so | M1 (the app tolerates absence) |
| G2 | Optional `pairingURL` QR output in `glass-core connect` for the owner link | Easier phone pairing; the link already works when pasted | M1 (nice to have) |
| G3 | Per-device capability revocation | The capability is derived from the gateway token and origin, so revoking one phone means rotating the token for every client | later |
| G4 | APNs push for "turn done" and "needs input" | The web has no background push either; this goes beyond parity | later |

## 10. Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| The phone must run the Tailscale iOS VPN | No remote access without it | Clear "not on tailnet" screen; document setup |
| `Origin` must be set manually on `POST` | Every mutation fails with 403 if missed | Middleware plus a unit test on every `POST` route |
| iOS suspends background apps and drops SSE | Missed live updates | `Last-Event-ID` resume; reset snapshot when the 1,024-event window is passed; local notifications only while running |
| Gateway changes the event or route set | Silent parity drift | Tolerant decoding, `.unknown` rendering, route contract test, G1 |
| Capability cannot be revoked per device | Lost phone keeps access until token rotation | Keychain "this device only"; G3; document token rotation |
| LAN HTTP needs an ATS exception | Weaker transport | Off by default; secrets refused on HTTP (gateway already does this) |
| Long timelines in SwiftUI | Scroll jank | `LazyVStack` with stable IDs, compact history, paging; profile in M2 |
| Math and Mermaid need web renderers | Bundle size, `WKWebView` cost | Lazy load, offline bundle, one shared web view process |
| Simulator iOS 26.5 vs minimum iOS 18 | iOS 18-only bugs missed | Install an iOS 18 Simulator runtime (Q10); device checklist in M7 |
| Xcode 26.6 builds with SDK 26.5 | New SDK behavior on old OS | Availability checks; test on iOS 18 |
| Unsigned IPA re-signing | Free accounts expire builds after 7 days; App Groups and Keychain sharing need a paid team | Owner decides (Q4, Q5) |
| Documentation CI | This doc is not in `docs/INDEX.md` or `docs/documentation-catalog.json` because only this file may change | Follow-up docs PR if the wiki check flags it |
| Marble is not on `main` | Shared package adoption has no target yet | Build `GlassClientKit` independently; adopt when Marble lands |

## 11. Open questions for the owner

1. **Hosts**: one Connect host, or several (row 19)?
2. **LAN HTTP**: do you use the plain-HTTP LAN origin, or only Tailscale
   HTTPS? If only HTTPS, the ATS exception is dropped.
3. **Pairing**: is pasting the owner link enough, or should `glass-core`
   print a QR code (G2)?
4. **Signing**: paid Apple developer team or free account? This affects
   build lifetime and which entitlements are usable.
5. **Keychain sharing with Marble**: should Glass and Marble share paired
   hosts? That needs a shared `keychain-access-groups` entitlement and the
   same team.
6. **Push**: is connected-only notification parity enough, or do you want
   APNs (G4)?
7. **Gateway changes**: OK to land G1 (`protocolVersion`) in `glass-core`
   before M1 ships?
8. **Marble**: will `apps/glass-marble-ios` return to `main`, and on which
   branch should `GlassClientKit` adoption land?
9. **iPad**: equal priority with iPhone, or iPhone first?
10. **Test runtimes**: may I install an iOS 18 Simulator runtime on this Mac
    for the minimum-OS gate?
11. **Live host for e2e**: which host and tailnet should automated e2e use,
    and can it run a disposable test workspace so e2e threads do not mix
    with real work?
12. **Versioning**: start at `0.1.0` with build numbers per phase, or match
    the `glass-core` version?
