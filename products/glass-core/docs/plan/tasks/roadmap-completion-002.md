# Roadmap workflow completion sweep

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: complete. [Final acceptance and explicit limits](../reviews/roadmap-completion-2026-10-11.md). Source acceptance remains the original
43-item roadmap and `roadmap-delivery.md`. Existing delivery notes require
fresh acceptance mapping; candidate numbers from other research are separate.

## Contracts for this sweep

- Hub metadata stays independent of transcript bodies. Unread is the difference
  between recorded events and the last explicitly viewed cursor, not a guess.
  Running status identifies a process lifetime; old process IDs are not liveness.
- Conversation branches include transitive ancestors and descendants within the
  bounded catalogue, and disclose omitted older entries. Opening a
  branch changes conversation context; workspace restoration remains explicit.
- Background inbox delivery records the accepted prompt and cursor in the same
  session append before advancing. Restart cannot replay accepted delivery.
- Handoff export has explicit byte bounds, original intent, current intent,
  partial/failure state and references. Import previews by default, creates a
  new conversation only with explicit apply, and never executes imported text.
- Automations bind host/project/model and expose a skip or once missed-run policy.
  A mismatch parks the schedule; it never changes the workspace under tools.
- PR/CI read operations run only on demand. Publication requires an explicit
  command or confirmed modal action and uses argv, with bounded output/time.
- UI additions reuse searchable modals, native structured reports and existing
  colors. Optional clients and adapters stay lazy.

## Hub and branch interaction

```text
Workspace hub → Conversations (search project / model / status / unread)
                 → Metadata / Resume / Background follow-up
                 → Branch family (ancestors + catalogued descendants)
               Attention / Team / Tasks / Usage / History
```

Desktop uses full searchable rows; narrow terminals wrap within the existing
modal. Navigation never adds model context or triggers a provider request.
Final review, release PTYs and performance checks follow the implementation sweep.
