# Native engine browser task execution (320)

Status: implemented locally in the native runtime.

This slice routes the normal Task Protocol execution path through
`BrowserRuntimeSession::native_execute_task`. Native CLI and MCP task execution
now share one adapter for form inspection, form validation, bounded multi-field
fill, form submission, field reads, navigation, tabs, menus, extraction,
dialogs, and pagination. The adapter preserves the existing revision-bound
task envelope, confirmation policy, actionability preflight, redacted receipt,
postcondition verification, and bounded retry guidance.

Native actions are resolved from the current native semantic observation and
dispatched through the native engine's target owner. Field values and
extraction records are read through the native document script realm, with
explicit item and byte limits. No `BrowserSession`, Chrome process, CDP
endpoint, or transport selector is created by this path.

The focused native integration test covers a revision-guarded form fill with a
text control and checkbox, including value verification in the native DOM.
Persistent cross-process session ownership, richer native region/Web IR
projection, workflow resume, and the remaining Core Web Profile certification
gates remain subsequent issue #40 work.
