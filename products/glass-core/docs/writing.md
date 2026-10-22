# Documentation style

[Documentation index](INDEX.md)

## Structure

The root README introduces the product, explains its practical value, and gives
a short path to the first task. Put configuration, interfaces, limits, examples,
troubleshooting, and implementation details in the relevant guide or reference.

`docs/INDEX.md` routes readers to current guides. `docs/reference/` holds detailed
workflow contracts. `docs/plan/` and `research/` preserve implementation history
and dated investigation records. Historical verification is evidence for its
checkpoint, not a current test result.

## Language

Use standard technical English. Prefer a familiar verb and a concrete object:
“reads metadata,” “rejects a stale revision,” or “opens the selected transcript.”
Keep each paragraph focused on one point. Define specialized terms where used.

Avoid filler, exaggerated adjectives, rhetorical questions, competitive rankings,
and claims of effortless or universal behavior. Explain what happens directly.
Keep product names, command names, statuses, and units consistent. Use `glass-core`
for the project/executable and `gc` for the short command.

## Evidence and examples

Document implemented behavior separately from proposals. State defaults, opt-in
requirements, side effects, ownership, and relevant failure behavior. Examples
must match the command parser and distinguish placeholders from actual values.

Performance numbers need a date, platform, method, and scope. Test counts belong
in dated verification records. Do not infer coding quality, cost savings, or
provider compatibility from local timing fixtures. Preserve source quotations
verbatim and label external research as a dated snapshot.

## Maintenance

Link to the canonical explanation instead of copying it into several guides.
When moving a section, update local links and any incoming README anchors. Keep
stable filenames for linked historical records. Check Markdown links, fragments,
code fences, and working-tree changes before committing documentation edits.
