# Documentation rewrite in ASD-STE100 style

Status: in progress locally.
Status: Historical 0.3.2 task; superseded by the current 0.3.12 source/release evidence.

## Scope

Rewrite the public user guides, client guides, maintainer guides, and release
instructions in clear technical English based on ASD-STE100 principles.

The rewrite covers:

- `README.md`;
- `CONTRIBUTING.md` and `SECURITY.md`;
- the guides linked from `docs/INDEX.md`;
- the Python, TypeScript, and npm client guides; and
- the benchmark and fuzzing guides.

The rewrite does not change code identifiers, JSON field names, CLI command
names, schema files, historical changelog entries, issue copies, or design
records under `docs/plan/`. Those items are controlled technical data or
historical records. They are not user-facing prose.

## Writing rules

- Use one main idea in each sentence.
- Use active voice and name the actor.
- Use imperative sentences for procedures.
- Use `must` for a requirement, `may` for an option, and `do not` for a
  prohibition.
- Use the same term for the same concept.
- Define an abbreviation at first use.
- Put a condition before the action when the condition changes the result.
- Use tables for fixed choices and code blocks for exact commands or data.
- State limits, defaults, errors, and unsupported platforms.
- Remove marketing claims, idioms, vague qualifiers, and internal release
  discussion from the first-read documentation.

## Verification

- Check every command against `glass --help` or the source implementation.
- Check every capability statement against the current local code.
- Check every platform statement against the Linux/macOS release contract.
- Run `git diff --check` after each documentation group.
- Run the existing Rust, package, and client checks after the complete rewrite.

## Release wording

The documentation must describe 0.2.0 as published and must distinguish that
fact from incomplete post-release artifact certification. Documentation for
0.2.1 must use local-development wording until its tagged release, artifact
checks, registry publication, and clean-install checks pass. It must not
describe Windows as a supported target.

ASD-STE100 is a controlled technical language with writing rules and a
controlled dictionary. This project applies its clarity principles. It does
not claim formal ASD-STE100 certification.
