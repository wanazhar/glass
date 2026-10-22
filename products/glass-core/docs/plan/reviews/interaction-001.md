# Scoped approvals and native questions

## Implementation

Interactive startup supports `--restricted` for workspace writes with explicit
approval requests and `--yolo` for unrestricted execution. Administrator
requirements remain authoritative. Approvals pause execution, show the complete
bounded arguments and grant one exact call. Headless and delegated agents cannot
approve themselves. Ordinary file integrity checks still apply after approval.

Native user questions support choices, multiple selections and written answers.
The host waits without another provider request. A question gates its entire tool
batch, including operations listed before it. Cancellation stops the turn.
Compact menus include a question preview and a full-text view.

## Review and verification

One GPT-6 Luna reviewer at medium reasoning reviewed the implementation and
follow-up fixes. No blocking findings remained. The workspace suite passed 925
unit tests; one existing doctest remained ignored. Clippy passed with warnings
denied. The final release build passed.

Release PTYs passed question choices at three sizes down to 20×8, written and
multiple answers, cancellation, batch deferral, shell and native approval,
default denial, allow-once expiry, invisible-review resize protection, yolo and
headless behavior. Separate release fixtures passed external file permissions,
live reasoning and continued tools, and exact scroll roundtrip cells and styles
at five sizes. A journal-reader assumption in the interaction fixture was fixed
before the final passing run.

System-control guard tests check exact approvals and their expiry without
executing kernel-control writes. Regular unread files remain protected.
