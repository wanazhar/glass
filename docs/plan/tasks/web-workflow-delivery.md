# Web workflow delivery

Status: in-progress. User authorized the complete Hermes/T3 gap delivery on 2026-10-16.
Priorities: 1. performance 2. pretty. Preserve the Glass typography, palette and lightweight architecture.

## Objective and contract

Deliver dependable reading and history, durable composer drafts and attachments/context references, direct everyday controls, visual changes review and file editing, live teammate/task visibility, consistent expanded outputs, attention/notification support and richer artifacts. Reuse native runtime operations and the shared writer authority; do not create a competing execution path. No automatic prompt submission, no service interruption or publication.

## Layout

    sidebar (projects, attention) | toolbar (project/thread)
                                 | transcript (reader owns scrolling)
                                 | contextual work / changes / agents
                                 | attachments / references / queue
                                 | composer + model / execution controls

Expanded readers preserve inline typography, wrap text, and restore their parent and scroll position. Keyboard overlays center inside the visible viewport. Stream following pauses immediately on reader intent; Latest resumes it. History prepends preserve a visible anchor. Async work cannot overwrite a different thread or newer edits.

Drafts are versioned, bounded, per host/thread, persisted on edits with debounce; storage failures remain visible and do not prevent typing. Attachments are explicitly selected, bounded and submitted through supported native image/context inputs. File writes require current writer authority and stale-content checks. Changes review uses real Git state and supported hunk operations. Model/profile controls use native command receipts. Agent status is fetched only for visible views with bounded polling, canceled on close/switch. Notifications are opt-in, capability-detected, and do not promise background iOS push without infrastructure. Scripted artifacts remain isolated from host/session APIs and require a defined sandbox contract; no placeholder interactive controls.

## Task split

- web-readers: reading/history/performance and expanded output consistency; source-owned module extraction where useful. Files: reading code extracted from app.js only after coordination; backend history improvements can precede client work.
- web-host: native APIs for browser attachments, stale-checked file edits, changes review; authoritative ownership and typed failures. Files: crates/glass/src/connect.rs, web_commands.rs and necessary Rust modules; no frontend edits.
- web-client: composer, direct controls, changes/files, agent/tasks, notification/attention, reader reliability and artifacts. Files: web/src, web/build.mjs. Integrate host contracts after agreement.

## Verification and delivery

Independent source review required. Builds and syntax/compile checks permitted. Do not add or run tests unless user explicitly requests them. Record evidence, remaining limitations, bundle size and all unfinished scope here. Do not label incomplete features shipped. No gateway restart, push or publication without authorization. Update docs/connect.md to describe implemented behavior only.

## Acceptance ledger

| Deliverable | Acceptance | State |
| --- | --- | --- |
| Reader/history | Earlier reasoning appears while following is paused; older/newer windows bounded; concurrent prepend/refresh serialized; visible anchor survives prepends | implementing |
| Mobile/expanded readers | Safe-area/keyboard layout preserved; text wraps; table presentation consistent; closing nested view renews parent lifecycle | implementing |
| Drafts/context | Versioned bounded drafts per thread, restoration cannot overwrite another thread; explicit upload/reference chips; no auto-send | implementing |
| Direct controls | Model/profile controls invoke native commands; queued message management uses durable native receipts; edit/retry preserve history | implementing |
| Changes | Actual status/diffs; hunk actions require reviewed fingerprint and confirmation | implementing |
| File editor | Bounded text editing, stale revision refusal, writer control, unsaved edit handling | implementing |
| Agent/task views | Visible-only refresh, bounded transcript, reopened parent works, assignment/status/results readable | implementing |
| Attention/notifications | Needs-input/failure/completion visible; opt-in capability-detected alerts with clear foreground-only scope | implementing |
| Artifacts | Consistent expansion; optional scripts explicitly started in isolated opaque sandbox without host access/network | implementing |

Baseline existing build: app.js.gz 36,400 bytes; style.css.gz 8,586 bytes. Bundle sizes are size measurements, not latency benchmarks. New workflow UI should be lazy-loaded rather than eagerly growing the initial shell. No competitive speed claim.
