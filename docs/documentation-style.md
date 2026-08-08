# Documentation style

Glass documentation uses clear technical English based on ASD-STE100
Simplified Technical English principles.

ASD-STE100 is a controlled language for technical documentation. It defines
writing rules and a controlled dictionary. Glass applies these principles to
make commands, limits, safety rules, and release status clear. Glass does not
claim formal ASD-STE100 certification.

## Rules for contributors

- Write one instruction per sentence.
- Use active voice. Name the person, program, or component that performs the
  action.
- Use the imperative form for procedures: `Run the command.`
- Use `must` for a requirement and `may` for an option.
- Use `do not` for a prohibition.
- Use one term for one concept. Do not switch between synonyms.
- Define an abbreviation at first use. Keep common code terms such as `CLI`,
  `MCP`, `CDP`, and `TUI` when they identify a protocol or interface.
- Keep sentences short. Split a sentence when it contains more than one
  action, condition, or result.
- Put conditions before actions when the condition is important.
- State the default, limit, failure result, and recovery action.
- Use exact names for commands, options, fields, errors, and schema versions.
- Use tables for fixed choices. Use code blocks for commands and data.
- Do not use marketing claims, idioms, jokes, vague words, or internal release
  discussion in user guides.
- Do not claim support for a platform, browser, workflow, or release without
  a passing test or a documented limitation.

## Terms

| Term | Meaning |
|---|---|
| Glass | The Rust program and library in this repository. |
| browser session | One Glass connection to one Chrome or Chromium process. |
| daemon | A local Glass process that accepts multiple local clients. |
| lease | A time-limited right to perform browser mutations in a daemon session. |
| revision | The number that identifies an observed browser state. |
| target | A page element reference used by an action. |
| capability | An operation that a Glass process reports as available. |
| extension | A separately declared program that uses the guarded extension host. |

## Procedure pattern

Use this order for a procedure:

1. State the result.
2. State prerequisites.
3. Give one command or action at a time.
4. State the expected result.
5. State the recovery action for a failure.

Example:

```console
cargo install --path crates/glass-dev --locked
glass --help
```

The first command installs the local checkout. The second command confirms
that the `glass` command is available.

## Status pattern

Use explicit status labels:

- `Supported`: the release contract includes the item and validation covers
  it.
- `Local only`: the item exists in this checkout but is not published.
- `Experimental`: the interface may change before publication.
- `Unsupported`: the project does not provide the item.
- `Blocked`: a release gate is not complete.

Review this file and the [documentation index](INDEX.md) when a public command,
capability, platform, or release process changes.

## Validation

Build both debug binaries, then run the inventory/link gate whenever a public
command, MCP tool, example, module, package README, or Markdown path changes:

```console
cargo build --workspace --all-features --locked
python3 scripts/check-documentation-coverage.py
python3 scripts/check-release-documentation.py
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked
cargo test --workspace --all-features --doc --locked
```

The coverage gate derives CLI names from both binaries, MCP names from the
pinned client-conformance fixture, examples from Cargo source, and modules
from the crate root. It also resolves repository-local links across current
guides and historical plan evidence. Keep `tools/list` and installed `--help`
as the exact-version schema and syntax authority.

For the standard definition, see the [official ASD-STE100
site](https://www.asd-ste100.org/).
