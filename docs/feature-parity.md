# Cross-platform feature parity

The machine-readable [feature parity matrix](feature-parity.json) is the
authoritative implementation inventory for four declared targets:

- Linux x86-64;
- Linux arm64;
- macOS x86-64; and
- macOS arm64.

The matrix records the published 0.2.0 baseline and the 0.2.1 work stream. It
separates implementation from target status. A capability can be implemented
while runtime verification on a particular OS remains incomplete.

Each target status uses one of these values:

| Status | Meaning |
|---|---|
| `certified` | The exact target artifact passed the required evidence. |
| `shippedUncertified` | The capability is shipped, but required target evidence is incomplete. |
| `experimental` | The capability is available only as an explicitly experimental surface. |
| `disabledByPolicy` | Policy disables the capability even though the implementation is present. |
| `blockedBySecurityGate` | A required security boundary is not certified, so the capability fails closed. |
| `unsupported` | The target does not support the capability. |

The current matrix intentionally marks ordinary cross-platform capabilities as
`shippedUncertified`, not unsupported. This is an inventory status, not a local
support claim. Native extensions are `blockedBySecurityGate` until the native
sandbox boundary is verified for the applicable environment. Windows is
outside this matrix and remains unsupported.

The JSON contract is defined by
[feature-parity-v1.schema.json](schema/feature-parity-v1.schema.json). The
matrix is evidence inventory, not a claim that every 0.2.0 artifact has passed
the complete post-release certification suite.

See [platform support and local certification](ci-platform-certification.md)
for the local verification boundary.
