# Security policy

Glass is a local development environment and browser control plane. Its trust
boundary includes project files, child processes, agent tool calls, Chrome
DevTools Protocol (CDP), browser profiles, MCP clients, and optional remote
views. Installing Glass does not make those resources safe to expose to an
untrusted network or untrusted automation.

## Supported versions

Security fixes target the latest published 0.x release. The repository may
contain a newer release candidate; its status is recorded in
[`docs/release-evidence.md`](docs/release-evidence.md) and does not make it a
published supported release.

## Trust boundaries

| Boundary | Authority it carries | Safe default | Primary risk |
|---|---|---|---|
| Project root | read and bounded mutation below one canonical directory | canonicalize every path; reject escapes and conflicting edits | source or secret disclosure, unintended writes |
| Workspace trust | activate repository-supplied commands and agent context | untrusted static inspection; local explicit decision; external identity-bound store | code execution while opening an unfamiliar repository |
| PTY/process manager | execute commands with the current OS user's privileges | bounded resident sessions and explicit stop/detach | arbitrary local command execution |
| Agent gateway | request schema-validated project and browser tools | built-in Pi tools disabled; mutations require authority and confirmation | prompt-driven local or browser mutation |
| Browser/CDP | inspect and control the selected Chrome profile | locally owned endpoint, dedicated profile, structured observation | cookies, authenticated pages, downloads, arbitrary page script |
| MCP stdio/socket | invoke the negotiated CLI-equivalent tool surface | local transport, initialization, capability checks, daemon mutation leases | confused-deputy calls from an untrusted client |
| Remote View/application server | display current local state and accept bounded control | loopback-only listener, short-lived token, SSH forwarding | session observation or control by another network peer |
| Profiles and evidence | persist browser state, events, reports, screenshots, and diagnostics | bounded retention and explicit capture | recovery of secrets after the interactive session |

Glass is not an operating-system sandbox. Project commands, language servers,
editors, native extensions, Chrome, and agent adapters retain the permissions
of the user that launched them. Use an OS account, container, or VM with the
minimum access required when the project or automation input is untrusted.

## Browser and network safety

CDP can read page data, execute JavaScript, navigate, capture screenshots, and
reuse authenticated sessions. Keep the debugging port on loopback. Never
publish it through a firewall, reverse proxy, public SSH bind, or shared tunnel.
Glass requires `--attach` before adopting an occupied CDP endpoint; this avoids
accidental takeover but does not authenticate the endpoint.

Prefer a dedicated profile. Use `--incognito` when persistence is unnecessary.
An owned persistent session should close through `Browser.close` so Chrome can
flush state; attach mode never owns or closes the external browser. Treat
profile directories, cookies, storage, DOM/AX output, PDFs, screenshots,
downloads, evaluated results, and browser diagnostics as sensitive.

For untrusted navigation, start with hardened policy:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
```

Hardened startup requires exact `--policy-allow-host` values. Glass resolves
each host, rejects non-public addresses, pins the accepted address in an owned
Chrome resolver, and checks document navigations, redirects, popups, and
attached frames before they continue. This reduces server-side request forgery
risk; it is not a general network sandbox for processes launched from a project.

Hardened mode denies attach, persistent profiles, JavaScript evaluation,
uploads, downloads, screenshots, and raw CDP by default. Enable only the exact
capability needed with `--policy-allow CAPABILITY`, or require confirmation with
`--policy-confirm CAPABILITY`. `--policy-confirm-once CAPABILITY` produces a
one-use authorization. These flags are trusted deployment configuration and
must not be copied from model output, page content, or another untrusted input.

Raw CDP exposes unrestricted protocol authority. It cannot use a one-use token
and requires an explicit `raw-cdp` grant.

## Project, process, and agent safety

Development Runtime paths are canonicalized below the project root. Existing
symlinks that resolve outside it are rejected. Atomic saves protect file
replacement, while revision checks and edit claims reject stale or conflicting
writers. These checks do not replace filesystem permissions: a command running
inside a project can still access anything allowed to the OS user.

Every project command is real code execution. Glass opens unmatched projects
as `Untrusted`, does not run `workspace.opened`, and does not inject project
skills into privileged Pi instructions. Inspect exact `glass.toml`,
`.glass.toml`, skill, hook, tool, test, LSP, and DAP sources before choosing
trust-once or identity-bound project trust. A shell tool cannot bypass this
boundary with `mutating = false`, and `--yolo` cannot elevate trust. See
[`docs/workspace-trust.md`](docs/workspace-trust.md).

After trust, review command arguments, environment variables, compiler/build
scripts, and language-server configuration before starting them. Stop or
detach resident PTYs deliberately;
closing a client does not imply that a daemon-owned process should be killed.

Pi starts with its built-in tools disabled. Glass exposes only its validated
tool gateway and keeps one-use broker material private. Browser tools remain
unavailable until a Browser Workspace is attached. Mutating tools require an
actor with authority plus explicit confirmation. Do not grant authority merely
because a request originated from an agent, repository file, browser page, or
MCP client.

Prompt text, authored task values, and tool arguments are excluded from raw
audit events, but invoked tools can still read sensitive project or browser
state. Review retained events and artifacts before sharing them.

## MCP, daemon, and remote access

MCP over stdio is intended for a trusted local client. stdout is protocol-only;
redirect diagnostics from stderr with the same care as other logs. A local
daemon uses filesystem-scoped transport and requires a mutation lease for
writes. Socket permissions and leases reduce accidental cross-client mutation;
they do not make a hostile client safe.

Remote View and the application server bind to loopback and are designed to be
reached through SSH local forwarding. Keep the generated token private, revoke
the view when finished, and do not use `ssh -R`, a public `-L` bind, or a public
proxy to expose it. Terminal-native live frames are ephemeral presentation;
they do not weaken the policy gates on screenshots or browser actions.

For SSH/Mosh/iPhone deployment details and failure recovery, see
[`docs/mobile-remote.md`](docs/mobile-remote.md). For daemon ownership and
shutdown behavior, see [`docs/daemon.md`](docs/daemon.md).

## Secret and artifact handling

- Keep credentials in an OS secret facility or process environment, not
  `glass.toml`, prompts, task files, screenshots, or committed fixtures.
- Do not commit `.glass-worktrees`, browser profiles, downloaded files, MCP
  transcripts, reconnect capsules, or authenticated-page evidence.
- Inspect query strings, filenames, terminal output, diffs, and diagnostic
  messages before publishing logs; redaction is bounded, not omniscient.
- Remove persistent state explicitly during uninstall if the machine should no
  longer retain profiles or configuration. See
  [installation and uninstall guide](docs/installation.md#fully-uninstall-glass).
- Keep Glass, Rust dependencies, Chrome/Chromium, language servers, agent
  adapters, and the host OS current.

## Deployment checklist

1. Run Glass as a non-privileged user in a dedicated project root.
2. Keep CDP, daemon sockets, the application server, and Remote View local.
3. Use incognito plus hardened host and capability policy for untrusted sites.
4. Review project commands and agent authority before enabling mutations.
5. Bound retained sessions and artifacts; delete sensitive state when done.
6. Verify the exact release and platform status in the release evidence.

## Report a vulnerability

Do not open a public issue with exploit details, credentials, profile data, or
other sensitive data. Use the
[private GitHub security advisory form](https://github.com/wanazhar/glass/security/advisories/new).

Include the affected version and commit, platform, trust-boundary assumptions,
minimal reproduction, observed impact, and a suggested mitigation if known.
Remove live credentials and replace private project/page content with a local
fixture whenever possible.
