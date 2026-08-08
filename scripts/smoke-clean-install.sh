#!/usr/bin/env bash
set -euo pipefail

# Verify both packaged products in isolated Cargo roots and exercise ownership
# transitions for the shared `glass-browser` command. This script never writes
# to the user's Cargo installation.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
previous_version="${GLASS_PREVIOUS_VERSION:-}"

version="$(cargo metadata --manifest-path "$root/Cargo.toml" --no-deps --locked --format-version 1 \
    | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "glass-dev"))')"
temp_root="$(mktemp -d "${TMPDIR:-/tmp}/glass-clean-install.XXXXXX")"
trap 'rm -rf "$temp_root"' EXIT

export CARGO_HOME="$temp_root/cargo-home"
export CARGO_TARGET_DIR="$temp_root/target"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR"

echo "--- Packaging core and full-suite ${version} ---"
cargo package --manifest-path "$root/crates/glass-browser/Cargo.toml" --locked --allow-dirty
cargo package --manifest-path "$root/crates/glass-dev/Cargo.toml" --locked --allow-dirty \
    --config "patch.crates-io.glass-browser.path='$root/crates/glass-browser'"
browser_crate="$CARGO_TARGET_DIR/package/glass-browser-${version}.crate"
dev_crate="$CARGO_TARGET_DIR/package/glass-dev-${version}.crate"
test -f "$browser_crate"
test -f "$dev_crate"

echo "--- Extracting packaged ${version} products ---"
mkdir -p "$temp_root/package"
tar -xzf "$browser_crate" -C "$temp_root/package"
tar -xzf "$dev_crate" -C "$temp_root/package"
browser_path="$temp_root/package/glass-browser-${version}"
dev_path="$temp_root/package/glass-dev-${version}"
patch="patch.crates-io.glass-browser.path='$browser_path'"

install_core() {
    local install_root="$1"
    shift
    cargo install --path "$browser_path" --locked --root "$install_root" "$@"
}

install_full() {
    local install_root="$1"
    shift
    cargo install --path "$dev_path" --locked --root "$install_root" \
        --config "$patch" "$@"
}

assert_core() {
    local install_root="$1"
    test -x "$install_root/bin/glass-browser"
    "$install_root/bin/glass-browser" --version | grep -F "$version" >/dev/null
    "$install_root/bin/glass-browser" --help >/dev/null
}

assert_full() {
    local install_root="$1"
    test -x "$install_root/bin/glass"
    assert_core "$install_root"
    "$install_root/bin/glass" --version | grep -F "$version" >/dev/null
    "$install_root/bin/glass" --help >/dev/null
    "$install_root/bin/glass" capabilities >/dev/null
}

echo "--- Clean core-only installation ---"
install_core "$temp_root/core-root"
assert_core "$temp_root/core-root"
test ! -e "$temp_root/core-root/bin/glass"

echo "--- Clean full-suite installation exposes both commands ---"
install_full "$temp_root/full-root"
assert_full "$temp_root/full-root"

echo "--- Transition: core only -> full suite ---"
install_core "$temp_root/core-to-full-root"
install_full "$temp_root/core-to-full-root" --force
assert_full "$temp_root/core-to-full-root"

echo "--- Transition: full suite -> newer/reinstalled full suite ---"
install_full "$temp_root/full-to-full-root"
install_full "$temp_root/full-to-full-root" --force
assert_full "$temp_root/full-to-full-root"

echo "--- Transition: full suite -> core command ownership ---"
install_full "$temp_root/full-to-core-root"
install_core "$temp_root/full-to-core-root" --force
assert_core "$temp_root/full-to-core-root"
test -x "$temp_root/full-to-core-root/bin/glass"

if [[ -n "$previous_version" ]]; then
    echo "--- Transition: published full suite ${previous_version} -> candidate ${version} ---"
    cargo install glass-dev --version "$previous_version" --locked \
        --root "$temp_root/previous-to-full-root"
    "$temp_root/previous-to-full-root/bin/glass" --version \
        | grep -F "$previous_version" >/dev/null
    install_full "$temp_root/previous-to-full-root" --force
    assert_full "$temp_root/previous-to-full-root"
fi

echo "core/full clean installs and ownership transitions passed at ${version}"
