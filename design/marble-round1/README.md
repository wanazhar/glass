# Marble iOS · round 1

**Design proposal, not shipped behavior.** This round is for [Marble](../../docs/marble.md), the self-hosted personal companion. It does not change the Marble app, its host, or Glass iOS. [PR draft](PR.md) · [Current app](before/) · [Phone proposals](iphone/) · [iPad proposals](ipad/) · [Sheets](sheets/) · [Splash](splash/)

## Recommendation

Choose **Pocket Pair** and make the **personality picker** home. Two abstract, unequal pebbles with a deliberate gap suggest independent companions without borrowing the Lens, a horizon, or optical effects. At 16 pt the silhouette still reads without colour. Use a clay secondary piece and a muted fig primary; the mark itself has no eyes. Reserve faces and the widest range of hues for the individual Marbles, so the launcher doesn't look like an extra assistant.

The picker makes the product promise visible immediately: these are different companions, not interchangeable chats. A short description says why you might choose each one without implying they are limited to a single task. Everyone stays one tap away in the tab bar, and the recent conversation stays one tap away on home. **Everyone-first** is the credible alternative if people mostly talk together: it opens right onto the conversation, puts named people within reach, and keeps the picker out of the critical path. It gives the transcript less room above the fold, and the introduction can get repetitive after the first week.

## Identity and launch

| Concept | Idea | Trade-off |
| --- | --- | --- |
| **Pocket Pair** | A soft hex and a drop sit diagonally like two small objects on a desk. The negative space survives monochrome and small sizes. | A pair could imply only two Marbles; the app should immediately show that people can add more. |
| **Keepsake** | One asymmetrical ceramic piece, separated by a seam, suggests keeping something worth returning to. | The seam does the work and is less robust at 16 pt. One object also says less about several companions. |
| **Patchwork** | Four different cuts form a compact stamp. | The most obviously plural, but at small sizes it reads like an app grid. |

The [identity sheet](sheets/identity-light.png) shows each icon in light, dark and a sample monochromatic tinted treatment, plus the actual-size mark and proposed splash lockup. The tint is an art direction sample, not a system-generated icon export. The [four-frame storyboard](splash/storyboard.png) and [Simulator recording](splash/pocket-pair.mp4) show the chosen mark. Both pieces are present from the start, move toward each other for 620 ms with an ease-out, and hold until the app opens at 1.55 s. No forced wait for the host: move to setup or display the saved-connection state independently. **Skip** is visible and available from the first frame. With Reduce Motion, keep the pieces settled and switch without animation. No splash haptic.

## Visual system

[Light sheet](sheets/system-light.png) · [Dark sheet](sheets/system-dark.png)

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| Paper | `#F7F2EB` | `#201B1A` | Solid canvas, including transcript |
| Card | `#FFFBF6` | `#2D2523` | Solid decisions, inputs, rows |
| Wash | `#EEE3E0` | `#3C2C32` | User bubble and selection |
| Ink | `#282022` | `#FFF4E9` | Body |
| Muted | `#695B59` | `#CBB9AD` | Supporting text, not disabled text |
| Line | `#E2D6CB` | `#51443F` | Decorative separation, not the only focus indicator |
| Fig / on Fig | `#653E52` / `#FFF8F2` | `#E3B7CA` / `#35212C` | Quiet brand, text links and high-emphasis controls |
| Ready | `#425A38` | `#BBD6A7` | Confirmed |
| Waiting | `#76511F` | `#E9C284` | Writing or needs attention |
| Problem | `#913B42` | `#F5ABAF` | Error with a recovery option |

Use status words and symbols as well as colour. On solid Paper and Card, the minimum WCAG text contrast is **5.82:1 light / 7.91:1 dark** across Ink, Muted, Fig and the three status colours. On Fig, the foreground pair is **8.44:1 / 8.46:1** before compositing. Keep Glass controls over quiet backdrops; use an opaque Fig button in the contrast fallback.

| Personality hue | Light fill | Dark fill | Face Ink vs. fill, light / dark |
| --- | --- | --- | --- |
| Clay | `#EAB5A5` | `#DEA28F` | 8.83 / 7.33 |
| Moss | `#C4D0A3` | `#B3C48C` | 9.77 / 8.48 |
| Plum | `#D8BFDE` | `#C9A8D1` | 9.41 / 7.58 |
| Honey | `#ECC988` | `#DDB570` | 10.06 / 8.27 |
| Iris | `#C5C9E9` | `#ADB4DD` | 9.78 / 7.84 |
| Rose | `#E9BDD0` | `#DDA9C3` | 9.59 / 7.98 |

Face Ink stays `#282022` in both appearances. In light appearance, a `#8F736F` outline separates each silhouette from Paper and Card; in dark appearance the fill itself separates it from both. The fill is not a text colour or a status signal. Face, shape and name carry identity together; never require the user to distinguish two hues. Circle, hexagon, shield, drop, star and softened square are available to each Marble.

- **Type:** SF Pro with Dynamic Type; large title 34 semibold, title 20 semibold, body 17 regular, secondary 15 regular, caption 12 medium, sheet eyebrow 11. No rounded variant. Use real weight/line breaks for message emphasis; never show raw markdown markers.
- **Spacing:** 4, 8, 12, 16, 24 and 32 pt as the base scale; 22 pt phone reading inset where it buys another word. Corners: 10 pt symbol tile, 14 pt compact, 18 pt card, 20 pt bubble with 6 pt tail-side corner. Interactive targets at least 44 pt.
- **Avatar scale:** 16 pt dense preview, 24 pt mention/toolbar, 30 pt speaker, 32 pt working row, 36 pt sidebar, 50 pt picker, 58 pt look preview, 88 pt profile. Working adds a dot and a nearby text label; no looping animation needed.
- **Glass vs. solid:** native navigation and tab bar, composer, discrete actions and toolbar controls use real Liquid Glass. Transcripts, replies, bubbles, memory and approval bodies, and suggestion lists use opaque paper or card. No glass-on-glass wrapping a whole conversation or form. Respect system Reduce Transparency; for implementation, use opaque Card and Fig equivalents if contrast is inadequate against the current backdrop.
- **Motion and haptics:** platform transitions and 120–200 ms local feedback; no scroll jump when streaming unless already at the latest message. Reduce Motion removes translation and decorative transitions, not information. Use system selection feedback for a changed shape or hue, subtle confirmation for an explicitly saved memory, and no haptic for tokens, status ticks, arrival or scroll.

## Screens and component rationale

The [transcript sheet](sheets/components-light.png) covers avatars, a user bubble, reply, handoff, thread row, folded tool step and inline Retry. The [controls sheet](sheets/controls-light.png) pairs approval and memory suggestions with the mention composer, Stop and Steer, and the native tab bar. Each has a [dark counterpart](sheets/).

| View | Why it is laid out this way |
| --- | --- |
| [Picker home](iphone/picker-light.png) | Face, name and short personality cue form one generous hit target. A recent Everyone row preserves continuity without demanding that a transcript be the first thing the app shows. |
| [Everyone home](iphone/home-everyone-light.png) | Show one short welcome and named choices, not the old unlabeled strip. The first messages start just below the choices, so it's recognisably a shared chat. |
| [Everyone](iphone/everyone-light.png) | The main shared transcript gives width back to the people talking. Thread entry and handoffs read as sentences. The hint is brief; use the completion list for names and show who will answer as the draft changes, including the default responder when nobody is addressed. |
| [Streaming chat](iphone/chat-light.png) | A person is the navigation title, with their face leading to their profile. Prose is full-width beside a name; reasoning and tools are folded. Keep draft and transcript visible if a turn stops. Stop ends the current turn; Steer changes what the person asks for while it is running. The pushed one-to-one view avoids stacking its composer on a tab bar. |
| [Threads](iphone/threads-light.png) · [Find](iphone/search-light.png) | Secondary options are visible from the chat menu. Rows carry human-readable topics and counts; search results should return the reader to the matching message, not switch conversations silently. |
| [Profile](iphone/profile-light.png) | Show name, face and how this Marble behaves first. Memory is a primary route; editing a look, workspace or abilities is a later choice. |
| [Memory](iphone/memory-light.png) | Put “Only Ada” and “Shared with everyone” in separate groups, and mark suggestions as unsaved. Ownership should always be shown before editing, forgetting or confirming. |
| [Edit memory](iphone/edit-memory-light.png) · [Review suggestion](iphone/confirm-memory-light.png) | Edit wording without changing the owner. Review shows where a new memory will go and leaves saving explicit. Confirm against the same suggestion rather than adding a second fact. |
| [New Marble](iphone/new-marble-light.png) | Name and personality precede appearance; starters are editable examples. Preview both shape and hue, and label both selections even when colour is unavailable. |
| [Activity](iphone/activity-light.png) | Start with a decision someone can make. Then show work under way, Pause new work, Stop everything and past decisions. In implementation, distinguish a reversible pause from cancelling in-progress work; confirm the latter and keep its result visible. |
| [Settings](iphone/settings-light.png) | Keep current host status visible without making it a diagnostics panel. Schedules and quiet hours, notifications, voice, model and permissions, background keep-alive and Live Activity are one level down. Group export and clear under data; explain what stays before clearing. |
| [Pairing](iphone/pairing-light.png) | Host URL and masked access token are the entire first form. Prefer trusted Tailscale HTTPS (`https://<name>.ts.net:4783`), accept tailnet HTTP (`http://100.x.y.z:4783`), and prove/pin a self-signed HTTPS host using `/api/tls-proof` before trusting it. A failed proof must not become a bypass. Keep token in Keychain; keep the host URL on device. Protocol details belong here in the proposal, not in the UI. |
| [Empty](iphone/empty-light.png) · [Error](iphone/error-light.png) | Empty chat offers a starting point rather than a fake welcome message. A failed reply remains in context and leaves the original message visible; Retry is a specific recovery, not an instruction to figure out what went wrong. |
| [Offline](iphone/offline-light.png) · [Connecting](iphone/connecting-light.png) | An existing address is still configured when the network is down. Keep it intact, say what the person can check, and offer retry. Connecting says what is being checked and offers Cancel without discarding previously saved credentials. |
| [Not connected](iphone/disconnected-light.png) · [Expired token](iphone/expired-light.png) | No saved connection and an invalid credential are different states. The latter keeps the host and asks for a replacement token, not new setup. On a later interruption, put the same copy near the composer without hiding already loaded content. |
| [iPad home](ipad/home-light.png) · [iPad chat](ipad/chat-light.png) | Native split view keeps companions visible while you talk. The welcome detail does not duplicate the picker in the sidebar; the open chat marks the selected person in the sidebar. Cap reading width instead of stretching bubbles across a tablet. |

## Local review and reproduction

The [baseline](before/) is the unmodified Marble app from main (`ca3e30c4`), on an iPhone 17 Pro with a seeded local Marble host and the repository's fake Core fixture. The copied prompts and raw operational line are fixture output, not sample copy for the proposal. Compare it with the proposal on the same iOS 26.5 device and status bar. The older PR #131 assets are reference only; the baseline was captured before rendering the proposal.

The prototype used deterministic SwiftUI fixtures, not production host traffic. It was removed when the design moved into the [Marble app](../../apps/glass-marble-ios/). The images here record the original proposal, including the Everyone-first variant that was not selected.

Use the iOS 26+ runtime. Launch with no state argument or with `splash` to see the greeting. The XL captures cover [picker](iphone/picker-xl.png), [chat](iphone/chat-xl.png), [memory](iphone/memory-xl.png) and [creation](iphone/new-marble-xl.png). Layouts scroll instead of shrinking content as text grows. Further checks should include the larger accessibility categories, VoiceOver focus order, Increase Contrast and a real keyboard.

## Self-critique

- The picker is an extra tap before shared chat if that is the main habit. Compare first-day comprehension with return visits. If the group wins, fold the greetings and named shortcuts once there is a long timeline rather than making them perpetual chrome.
- Small shapes get harder to tell apart as they shrink. Make name and speaker labels invariant. The set is not a substitute for colour vision or low-vision testing, especially while moving quickly through a transcript.
- The chat sample is deliberately short and calm. Long Markdown, bigger text, multiple concurrent decisions, a real keyboard and a long stream can expose scroll and focus problems that the screenshots cannot. The compact controls are intentionally opaque enough for this backdrop; check the actual glass composite with harder backgrounds.
- The welcome motion is optional, but even 1.55 s can feel long on a warm return. Only show it on a cold start in implementation, and go straight to the prior screen when it can be restored.
- Connection views depict the decisions, not transport behavior. Test interrupted pairing, certificate replacement and reconnection on a real tailnet before implementing final copy or treating any success state as verified.
