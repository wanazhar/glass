# Glass native engine browser slice 294: worker runtime identity

Status: completed locally.

## Objective

Give dedicated workers the runtime identity and URL primitives expected by
ordinary browser libraries without adding a second resource or security
owner.

## Contract

- A dedicated worker exposes a stable `self === globalThis` realm with no
  document, a structured `location`, and a bounded `navigator` snapshot.
- Worker `URL` accepts bounded absolute and base-resolved relative URLs and
  projects protocol, host, hostname, port, pathname, search, hash, origin,
  credentials, `href`, `toString()`, and `toJSON()` values.
- Worker `URLSearchParams` accepts query strings, records, pair arrays, pair
  iterables, and another worker `URLSearchParams`; duplicate entries,
  percent decoding/encoding, append/set/delete/get/getAll/has, stable sort,
  iteration, `forEach()`, `size`, and `toString()` are available.
- URL and parameter state is bounded by explicit entry and serialized-size
  limits. Invalid URL bases, malformed parameter pairs, and limit violations
  fail inside the worker realm rather than crossing the host boundary.
- Re-entering the worker realm for messages, timers, or network continuations
  preserves constructor and runtime identity.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker bootstrap now installs bounded `URLSearchParams` and `URL`
constructors before the existing Fetch and transport surfaces. URL parsing
handles scheme/authority, credentials, host/hostname/port decomposition,
path normalization, query, fragment, HTTP/WebSocket origin mapping, and
relative resolution against a supplied base. Worker `location` is rebuilt
from the worker resource URL with the same projected fields. A frozen
`navigator` snapshot provides stable user-agent, locale, online, cookie,
platform, product, and hardware-concurrency values across host turns.

The test witness exercises duplicate query parameters, sorting, mutation,
space encoding, URL port/origin fields, relative path resolution, worker
global identity, document absence, and navigator values. The change remains
inside the existing isolated worker realm and does not alter the resource
loader or content-process protocol.

## Tradeoffs and follow-up

Keeping URL and navigator projection in the worker bootstrap improves library
compatibility without duplicating navigation, cookie, or network policy. The
parser is intentionally bounded and does not claim the complete URL standard:
live synchronization between `URL.searchParams` and `URL.search`, full IDL
property descriptors, Unicode/credential normalization, URLPattern, blob and
filesystem URL behavior, and every standards edge case remain follow-up work.
Shared/service/worklet workers, automatic background task-source scheduling,
and the final native/CDP replacement gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_worker_exposes_url_search_params_and_navigator --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (18 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
