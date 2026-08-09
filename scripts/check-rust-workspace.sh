#!/usr/bin/env bash
set -euo pipefail

action="${1:-test}"

case "$action" in
  check)
    cargo check --package glass-browser --all-targets --all-features --locked
    cargo check --package glass-dev --all-targets --all-features --locked
    ;;
  test)
    cargo test --package glass-browser --all-targets --all-features --locked
    cargo test --package glass-dev --all-targets --all-features --locked
    ;;
  clippy)
    cargo clippy --package glass-browser --all-targets --all-features --locked -- -D warnings
    cargo clippy --package glass-dev --all-targets --all-features --locked -- -D warnings
    ;;
  *)
    echo "usage: scripts/check-rust-workspace.sh [check|test|clippy]" >&2
    exit 2
    ;;
esac
