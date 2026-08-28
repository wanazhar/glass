#!/usr/bin/env bash
set -euo pipefail

# Browser-free command discoverability and contract smoke checks. This script
# intentionally makes no real-browser performance or parity claim.
bin="${GLASS_BIN:-cargo run -p glass-dev --quiet --}"
read -r -a cmd <<< "$bin"
"${cmd[@]}" --help >/dev/null
"${cmd[@]}" workspace --help >/dev/null
"${cmd[@]}" memory --help >/dev/null
"${cmd[@]}" surfaces --help >/dev/null
"${cmd[@]}" backend --help >/dev/null
"${cmd[@]}" replay --help >/dev/null

echo "experience command discoverability: ok (browser-free)"
