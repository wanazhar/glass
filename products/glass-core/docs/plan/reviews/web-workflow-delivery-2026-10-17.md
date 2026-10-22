# Web workflow delivery review — 2026-10-17

Result: source PASS. Reviewer: workflow_verify, independent of implementation agents workflow_host and workflow_client.

Reviewed native bounds and actor authority; stale file/Git/source guards; private uploads and cleanup; bounded artifacts and activity text snapshots; draft hydration/persistence; reader intent and history serialization; unsaved editor/navigation/reconnect lifecycle; keyed visible-only agent refresh; model/profile/queue/task commands; notifications and thread navigation; CSS fonts/mobile input sizing; service-worker integration.

Blocking findings were fixed before integration, including frozen reasoning updates, within-page source truncation, hundreds of serial raw-record fetches, stale controllers on preview return, wrong-thread failed-send restoration, unsaved edits on reconnect, duplicate Git diff payloads, eager hunk DOM, and inherited CSP preventing artifact execution.

Evidence: cargo check, native release build, web build, five JavaScript module syntax checks and whitespace checks passed. No tests were run. Final main compressed shell: app 40,001 bytes, style 9,131 bytes; lazy workflows 5,711 bytes. These sizes do not establish device latency or comparative speed.

Physical iPhone Safari/PWA verification remains outstanding for keyboard geometry, safe areas, touch momentum, and notification permissions. Runtime activation is separate from source review and must preserve active workers.

## Activation follow-up

The initial source pass missed three absent HTML nodes referenced by client handlers. This was corrected in `5ed1433`; the follow-up reviewer confirmed 73 literal ID references, 75 unique markup IDs, zero missing references or duplicate IDs, mobile action-row visibility, and consent/idle/stale-view guards for runtime updates. Final served markup and public capability reads pass; deployed shell is `e372e4ddcdfd`. No worker was restarted by the client deployment.

## Backend runtime correction

Live worker inspection found a second capability discovery bug beyond the control-event reset: the idle attachment replay omitted `web_workflows`, although both busy snapshots included it. The one-line native correction passed source review and release build. Final deployment verified all 19 real workers on the installed binary and all 19 idle replay responses advertising workflow support. The user-requested removal of runtime update UI passed complete DOM binding review; served HTML confirms it is absent. Native transcript files were retained through the update.
