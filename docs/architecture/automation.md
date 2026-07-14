# Agent browser automation

Status: Draft

## Purpose

Define the target contract for a browser control layer that is safe for agents,
predictable for humans, and measurably smaller and faster than general-purpose
automation stacks. This document extends the accepted browser data-plane
contract; it does not replace its compact-observation or explicit-cost rules.

## Product boundary

Glass owns deterministic browser mechanics. It does not own planning or natural
language reasoning.

```text
human / agent
      │ intent + explicit policy
      ▼
CLI / MCP / TUI
      │ typed operation
      ▼
BrowserSession ──► target/frame registry ──► CDP actor ──► Chrome
      │                    │
      ├─ observation cache ├─ wait engine
      ├─ safety policy     └─ event-derived state
      └─ bounded evidence (text, image, trace)
```

## Design priorities

The priorities are ordered. A lower priority never justifies violating a
higher one.

1. **Correctness:** fail on ambiguity; never guess a destructive target.
2. **Safety:** bound inputs and outputs; make powerful operations policy-aware.
3. **Reliability:** expose explicit waits and state; avoid timing folklore.
4. **Agent efficiency:** return stable references and compact structured
   evidence by default.
5. **Human control:** actions remain inspectable, cancellable where possible,
   and usable in headed sessions.
6. **Performance:** minimize CDP round trips, allocations, enabled domains,
   retained state, binary size, and resident memory.
7. **Coverage:** add capabilities only with the same contracts and budgets.

## Non-negotiable resource rules

- Every channel, retained collection, protocol frame, text field, image stream,
  and diagnostic buffer has an explicit bound.
- Default observation enables only the CDP domains it needs. Network, tracing,
  screencast, downloads, and deep DOM are scoped subscriptions.
- Large payloads are streamed or moved once. They are not cloned through
  generic event broadcasts.
- A cached object has an owner, invalidation rule, byte budget, and lifetime.
- Performance reports separate Glass memory from Chrome memory and distinguish
  fresh, cached, and opt-in operations.
- New features must report their steady-state RSS, peak allocation, p50/p95
  latency, payload size, and binary-size delta before merge.

The MCP transport bounds newline/header input to 8 KiB, content-length input to
4 MiB, serialized output to 32 MiB, active requests to eight, and queued
responses to sixteen. Browser operations retain single-session ordering even
though the transport accepts requests concurrently for cancellation.

Initial release budgets are defined in
[`best-in-class-browser.md`](../plan/analysis/best-in-class-browser.md) and may
only change with recorded evidence.

## Targeting contract

Target resolution returns exactly one of:

- `Unique(target)` with provenance and current revision;
- `Ambiguous(candidates)` with bounded candidate summaries; or
- `NotFound` with the attempted strategy.

Accessible names, roles, text, CSS, and ordinal positions are separate locator
strategies. Substring or role-only matching never silently selects the first
candidate. Revisioned backend-node references remain the fastest preferred
agent path.

Before pointer dispatch, Glass verifies that the target is attached, visible,
enabled when relevant, inside the viewport, and is the hit-test result at the
chosen point. Layout movement triggers a bounded re-resolution or an explicit
failure. Glass does not claim that smooth pointer motion defeats automation
detection.

Popup-producing clicks use the explicit `click_expect_popup` operation. Glass
pre-arms a trusted-click witness in an isolated execution world on the uniquely
resolved backend node. Page script cannot read, call, predict, or replace the
witness state or callback: installation uses native `EventTarget` behavior and
only an `isTrusted` event on that exact node counts. Witness and temporary popup
attachment state use cancellation-safe guards with bounded cleanup; they do not
leave page-visible bindings or retain an unbounded session registry.

Glass snapshots the original target, frame, authoritative target IDs, monotonic
topology sequence, and loss epoch immediately before `mouseReleased`. This
operation gives that exact release request a 500 ms acknowledgement deadline;
ordinary `click` and the global CDP response timeout remain unchanged. A normal
acknowledgement remains authoritative. Only expiry of this operation-specific
deadline may enter recovery, and only when the trusted witness fired.

Recovery requires exactly one later live page target that names the original
target as `openerId`, followed by bounded attach and readiness verification.
Before accepting the candidate, its topology sequence and loss epoch must remain
unchanged for a 50 ms quiet interval within the existing two-second recovery
deadline; every late topology event resets that interval. Immediately before
success, Glass repeats authoritative target discovery and requires the candidate
to remain the only live later opener match; it also rechecks the topology
sequence and loss epoch so a second popup, destruction, or late event loss cannot
race the decision. Cleanup and these final checks run without changing the active
target or frame. The result records
`causally_verified_popup`, popup ID, opener ID, release acknowledgement state,
and verification evidence. Missing, multiple, lagged, destroyed, mismatched,
unreadable, cancelled, or cleanup-failed outcomes—and every non-timeout CDP
error—remain bounded typed failures, including through MCP. Ordinary `click`
never suppresses an input timeout or pays this witness cost.

## Wait contract

Actions do not acquire hidden universal delays. Callers choose a typed
condition and a deadline:

- document lifecycle or URL condition;
- target attached, visible, hidden, enabled, or stable;
- text or JavaScript predicate;
- download, dialog, popup, or navigation event; or
- bounded network quiet when the Network domain is explicitly active.

Waits use event-derived state when available and bounded polling otherwise.
Timeout errors include the condition, deadline, last observed state, and a
bounded diagnostic snapshot.

## Browser topology

A browser session owns a bounded registry of page targets and their frame
trees. Target and frame identity is explicit in observations and actions.
Popup creation, target closure, frame navigation, and process loss update that
registry through CDP events. No command silently changes the active page.

Cross-origin frames are controlled through CDP target/session routing, not DOM
assumptions. Shadow roots remain part of locator traversal with explicit
boundaries in diagnostics.

## Interaction surface

The complete intended primitive set is:

- navigate, reload, back, and forward;
- observe, screenshot, DOM, console, and scoped network evidence;
- click, click-and-expect-popup, double-click, hover, drag, wheel, key,
  shortcut, type, and clear;
- check/uncheck, select, and file upload;
- list/create/select/close target and select frame;
- accept/dismiss dialog and monitor downloads; and
- evaluate JavaScript when policy permits.

Each primitive returns a typed outcome containing target/frame identity,
resulting revision, and only the evidence required to decide the next step.
Download authorization is bound to the active target's exact browser context,
including disposable incognito contexts. Glass enables lifecycle events and the
approved destination only for that context, then restores `deny` on the same
context before returning or during cancellation cleanup.

## Safety policy

Policy is evaluated before browser or filesystem side effects. It can restrict:

- allowed URL schemes, hosts, and private-network destinations;
- JavaScript evaluation;
- persistent profiles and attach mode;
- downloads, uploads, and screenshot output paths;
- destructive or credential-bearing actions; and
- maximum protocol, DOM, image, and trace sizes.

The default local policy remains useful for development but rejects unbounded
inputs. A hardened preset is available for untrusted agent workloads. Policy
denials are typed and distinguishable from browser failures.

## Evidence and diagnostics

Failures return bounded structured diagnostics rather than raw unbounded DOM or
logs. Optional diagnostic subscriptions expose console errors, failed network
requests, dialogs, downloads, and recent lifecycle events. Secret-bearing
request bodies, cookies, headers, typed text, and evaluated source are redacted
by default.

Diagnostic capture is an explicit, time-bounded scope rather than permanent
session state. A scope has one owner, a maximum 30-second lifetime, and bounded
buffers of 128 console entries, 128 network entries, 16 dialogs, and 32
download transitions. Runtime/Log and Network are enabled on the selected
target only while at least one scope leases them; the final lease disables the
domains and drops retained evidence.

Every entry carries the immutable target/frame route and a monotonic timestamp.
Text fields are UTF-8 bounded to 2 KiB and URLs to 4 KiB. URL query values,
cookies, authorization and proxy-authorization headers, request bodies, and
typed or evaluated source are never retained. Network evidence is metadata
only: method, redacted URL, status or failure class, redirect count, and bounded
safe header names. Broadcast lag increments an explicit dropped-event counter.

Dialogs are reported even without a diagnostic scope because they block page
progress, but only bounded metadata is retained until accept or dismiss.
Downloads require an explicit scoped destination and return lifecycle evidence
from will-begin through progress or completion; Glass never captures response
bodies as diagnostic evidence.

## Visual evidence

Visual capture is always explicit. A request names viewport, full-page, or one
resolved element; PNG, JPEG, and WebP formats; scale; optional CSS-pixel clip;
and lossy quality where applicable. Results report encoded byte length, image
dimensions, device scale factor, selected target/frame identity, and the exact
effective clip. No image bytes enter observation or session caches.

Full-page capture snapshots layout metrics once and captures that bounded
content extent without permanently resizing the user viewport. Element capture
uses the same actionable backend-node resolution and clips to its current box.
Comparison is optional and bounded: Glass reports dimensions, changed-pixel
count, ratio, and one bounded difference box without retaining either input.

Screencast is a distinct opt-in scope with a dedicated capacity-two frame
channel. Every received frame is acknowledged exactly once, overwritten or
lagged frames increment explicit counters, and dropping the scope stops casting
and drains acknowledgements. Streaming never uses the generic CDP broadcast
payload path for image ownership.

## Extension rule

Frontends call `BrowserSession`; they do not issue raw CDP. CDP domain adapters
may be added behind typed session APIs. A new adapter must declare domain
enable/disable lifecycle, retained state, bounds, failure behavior, and tests.

## Required verification

- deterministic contract tests for every locator, wait, and policy outcome;
- real-Chrome integration tests for every frontend-to-session-to-CDP chain;
- adversarial framing, parser, and page-state tests;
- multi-platform browser smoke tests;
- task-success comparisons against mature automation tools; and
- resource regression gates for default and explicitly expensive paths.
