# WebUI agentic chat components

Status: Implemented; browser verified. Scope: Connect WebUI; existing Core and Glass session protocols.

## Overall structure

```text
Workspace / conversation                         Tools

                         User request [attachments]
Assistant response
  Code card: language / filename / diff counts / copy
  Tool group: activity → target → recorded sources

Checklist: N steps left                         collapse
  completed / working / pending / blocked
Activity indicator · actual phase · observed elapsed time
Composer: attachment previews / text / model / voice / send
Workspace / Git / mode / context controls
```

## Design and state

The supplied BoardUI references guide spacing, hierarchy, disclosure, and motion.
Implement the components locally against existing protocols. Retain Glass's green
accent, consistent light/dark tokens, full-screen reader surfaces, and existing
rounded command/work palettes. Component cards may be rounded within those surfaces.

- Composer attachments show local image thumbnails or file type, measured upload
  progress, cancellation, and native context removal. A completed transfer remains
  in a saving state until the native attachment command succeeds. Cross-thread
  results cannot update the current composer. Object URLs are revoked on removal.
- The checklist reflects native `state/todo` records, not durable memory notes.
  Completed rows have checks, in-progress rows have a ring, pending rows are
  outlined, and blocked rows retain a reason. Progress counts describe tasks,
  never an invented percentage of overall work. Completion collapses the card once; users can expand it afterward. The card sits in the transcript beside task activity, and native checklist inspection restores it even when its record is outside the recent history page.
- Agent phases come from real tool/model events, with browser-observed elapsed
  time. Reduced motion disables shimmer and looping movement. Reconnects do not
  invent model reasoning or historical elapsed durations.
- Code cards retain safe text, original copy/expand behavior, line numbers, and
  fenced filenames when provided. Addition/deletion counts derive from real diffs.
- Search steps expose the actual query and recorded HTTP(S) sources. Source badges
  are local; rendering them does not contact third-party favicon services.
- The file tree lazily expands bounded browse pages. Arrow keys, Home/End, Enter,
  focus, selection, and expanded state follow the WAI-ARIA treeview pattern.
  Existing directory navigation, disk measurement, hidden-file controls, file
  previews, and pagination remain available.
- Microphone input is visible only when the browser supports speech recognition
  in a secure context. It starts only on a user action and preserves typed drafts.
- Context usage is shown only when reported; unavailable usage has no percentage.

## Verification

Exercise mobile, tablet, and desktop widths in both themes. Verify keyboard tree
navigation, checklist transitions/collapse, upload cancellation/progress, literal
code/source rendering, actual source links, draft preservation, reduced motion,
existing activity/history recovery, and authenticated upload behavior. Capture
browser screenshots before installing the built assets and verifying live hashes.

## Research

- [BoardUI composer](https://www.boardui.com/components/composer) and
  [agent-thinking patterns](https://www.boardui.com/components/agent-thinking).
- [WAI-ARIA treeview](https://www.w3.org/WAI/ARIA/apg/patterns/treeview/).
- [MDN upload progress](https://developer.mozilla.org/en-US/docs/Web/API/XMLHttpRequest/upload).
