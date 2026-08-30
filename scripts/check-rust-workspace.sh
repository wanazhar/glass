#!/usr/bin/env bash
set -euo pipefail

action="${1:-test}"

case "$action" in
  fast-check)
    cargo check --package glass-browser --lib --locked
    cargo check --package glass-dev --lib --bins --locked
    ;;
  fast-test)
    cargo test --package glass-browser --lib --locked
    cargo test --package glass-dev --lib --locked
    ;;
  fast-build)
    cargo build --package glass-dev --bin glass --locked
    ;;
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
    echo "usage: scripts/check-rust-workspace.sh [fast-check|fast-test|fast-build|check|test|clippy]" >&2
    exit 2
    ;;
esac
