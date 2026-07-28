#!/usr/bin/env bash
set -euo pipefail

# Verify the packaged crate in an isolated Cargo home and exercise an upgrade
# from the last published version. This script never writes to the user's
# Cargo installation.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
previous_version="${GLASS_PREVIOUS_VERSION:-}"
if [[ -z "$previous_version" ]]; then
    echo "GLASS_PREVIOUS_VERSION is required" >&2
    exit 2
fi

version="$(cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --format-version 1 \
    | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "glass-browser"))')"
temp_root="$(mktemp -d "${TMPDIR:-/tmp}/glass-clean-install.XXXXXX")"
trap 'rm -rf "$temp_root"' EXIT

export CARGO_HOME="$temp_root/cargo-home"
export CARGO_TARGET_DIR="$temp_root/target"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR"

echo "--- Installing previous release ${previous_version} ---"
cargo install glass-browser --version "$previous_version" --locked --root "$temp_root/upgrade-root"
previous_binary="$temp_root/upgrade-root/bin/glass"
"$previous_binary" --version | grep -F "${previous_version}" >/dev/null

echo "--- Packaging ${version} ---"
cargo package --manifest-path "$root/Cargo.toml" --locked --allow-dirty
crate="$CARGO_TARGET_DIR/package/glass-browser-${version}.crate"
test -f "$crate"

echo "--- Installing packaged ${version} ---"
mkdir -p "$temp_root/package"
tar -xzf "$crate" -C "$temp_root/package"
cargo install --path "$temp_root/package/glass-browser-${version}" --locked --root "$temp_root/clean-root"
clean_binary="$temp_root/clean-root/bin/glass"
"$clean_binary" --version | grep -F "${version}" >/dev/null
"$clean_binary" capabilities >/dev/null

echo "--- Upgrading the previous installation to ${version} ---"
cargo install --path "$temp_root/package/glass-browser-${version}" --locked \
    --root "$temp_root/upgrade-root" --force
"$previous_binary" --version | grep -F "${version}" >/dev/null
"$previous_binary" capabilities >/dev/null

echo "clean install and upgrade smoke passed: ${previous_version} -> ${version}"
