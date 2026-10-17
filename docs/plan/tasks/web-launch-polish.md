# Glass launch presentation

Status: in-progress. User requested a distinctive, fast, theme-matched startup screen on 2026-10-17, within the broader UI/UX polish request.

## Objective

Present a quiet, crafted Glass launch state immediately from the HTML. Use the existing cube geometry, type and restrained green palette. No stock spinner, gradient orb, fake terminal, percentage, rotating marketing copy, or minimum display time. Avoid enlarging the main application bundle or loading heavy renderers.

## Layout

    system status / safe area

             small Glass cube
             glass.
             short architectural rule
             Opening workspace

    system home indicator / safe area

Use generous negative space and restrained scale; composition should look intentional on both a phone and desktop. Motion should describe the cube/structure briefly and honor reduced motion. System light/dark styling must work on first paint before app initialization. Use existing fonts and inline SVG, without a new asset or animation library.

## Lifecycle

HTML provides launch markup and hides/inerts the underlying shell. A tiny independent bootstrap starts the app immediately, handles module failures, and listens for real readiness/error events. App readiness means the initial host status and thread list (and a requested thread initialization) have settled. No fake progress, delays, or waiting for nonessential diagrams/fonts/service-worker registration. Returning to a backgrounded app or ordinary reconnect must not replay the launch screen. Shell geometry, safe-area sizing, composer position and reader state are unchanged.

The app becomes visible and usable immediately on ready. Failure must stop motion and expose clear recovery; a failed script or module fetch must not strand an invisible shell or an endless loading state. Watchdogs may offer retry after real slow loading but must not delay success. Preserve retry URL/thread/local authorization flow. No inline JavaScript that violates the existing CSP. Service-worker versioning includes all startup-relevant files; old cached shells remain internally consistent. A bootstrap module must not add an avoidable request waterfall: use module preload or equivalent.

## Delivery

Implementation: web/src/{index.html,style.css,app.js,boot.js,sw.js}, web/build.mjs as needed. Native runtime remains unchanged. Performance baseline: main JS 39,867 bytes gzip, CSS 9,168 bytes gzip. Record bootstrap size and startup request changes.

Independent review covers complete DOM binding, bootstrap failures, readiness races, reconnect behavior, CSP, offline shell, theme, reduced motion, and asset versioning. Visual inspection screenshots are useful for design review; no tests are to be added or run unless requested. Builds/syntax checks are allowed. Physical iPhone verification must not be inferred from desktop emulation. Deploy reversible web assets after review without restarting workers or the gateway. Retain a concise delivery record and update current user documentation.
