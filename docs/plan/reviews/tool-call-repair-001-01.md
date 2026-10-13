# Tool-call repair verification

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09. Root implementation; two GPT-6 Luna medium reviewers.

## Delivered

- Recover completed responses with invalid JSON, nonobject arguments, schema-invalid fields or unknown tools via the ordinary model loop.
- Validate the whole batch before executing any member. Every rejected call receives a paired error result. Two correction requests per turn, also bounded by existing tool-round and quota budgets; third invalid batch stops explicitly.
- Preserve raw model arguments in durable intent records; canonicalize executable and outgoing history arguments so nested/escaped/duplicate keys cannot change meaning between validation and execution. Invalid wire arguments become an explicitly rejected empty-object placeholder.
- Keep malformed transport/incomplete responses/missing or duplicate in-response identities fatal. Partial calls never execute.
- Preserve normal authorization/read guards on corrected calls; never automatically replay shell failures or completed actions.
- Show compact warning-colored `Tool repair 1/2` notices, including narrow PTYs. Standard error outcomes have an explicit text prefix; Anthropic error results carry `is_error`.
- Validate schema shape and supported structural keywords. No new dependency, default tool, success-path provider request or schema tokens.

## Review findings resolved

The reviewers identified missing common schema constraints, parse-valid malformed schema acceptance, and inconsistent Anthropic error flags. Added alternatives/not/length checks, schema-shape rejection and centralized Outcome::err prefixing, plus assertions for Anthropic denied results. The implementation remains deliberately partial JSON Schema; $ref, regex/format and exclusive numeric constraints belong to extension validation.

## Evidence

- `cargo test --workspace`: 608 passed, zero failures, one ignored doctest.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- New smoke matrix: 33 headless cases (11 modes across Chat Completions, Responses and Anthropic), three denied-call cases, nine PTYs at 80x24/40x24/20x8 across all three wires, one cancel-during-repair PTY. Includes previous completed action exactly once, mixed-batch rejection, raw intent preservation, valid replay payloads, exhaustion after three invalid responses, fatal truncation/duplicate identities and follow-up input.
- Final release: all 46 repair scenarios passed, including nine mobile/desktop PTYs and cancellation during repair.
- All existing 15 release regression suites passed; separate Unicode job regression and performance budgets passed.
- Release SHA-256: `11f472ab008eb6290a3e139a422ccee1abbb623879d675b4b4eb839ea0da191f`. The regression runner verified this binary stayed unchanged throughout all 15 suites.

Research and rationale: [design contract](../tool-call-repair.md).
