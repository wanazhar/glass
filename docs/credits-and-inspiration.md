# Project inspiration and attribution

Glass and Glass Core are independent projects. This page acknowledges product
ideas and research references; it does not claim that Glass copies source code,
is affiliated with, or is endorsed by the projects below. Runtime dependencies
and required license notices are tracked separately in package manifests and
[`products/glass-core/THIRD_PARTY.md`](../products/glass-core/THIRD_PARTY.md).

## Product references

- [Pi](https://github.com/earendil-works/pi) informed research into durable
  coding-agent sessions, provider setup, and extension workflows. Glass Dev
  previously included a Pi SDK runtime, which has been removed; the historical
  record is in the [retired runtime note](pi-sdk-runtime.md). See also the
  dated [Pi durability survey](../products/glass-core/research/pi-durability-2026-10.md).
- [OpenAI Codex](https://github.com/openai/codex) is a reference in Glass Core
  research on interaction, prompting, task orchestration, and sandbox design.
  See the [harness feature survey](../products/glass-core/research/harness-features-2026.md)
  and [measured design notes](../products/glass-core/docs/plan/output-polish.md).
- [T3 Code](https://github.com/pingdotgg/t3code) informed research into streamed
  work logs and conversation surfaces. The Glass Core web layout documents this
  reference in its [frontend guide](../products/glass-core/web/README.md),
  which states that no T3 Code application code is copied. See the dated
  [T3 Code parity research](../products/glass-core/research/t3code-parity-2026-10-10.md).

These references describe design research and ideas. They do not imply
compatibility, integration, shared ownership, or endorsement. The linked
research records retain their dates and scope; consult current guides for
implemented Glass behavior.
