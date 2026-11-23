#!/usr/bin/env bash
set -euo pipefail

action="${1:-test}"

# Prefer cargo-nextest when it is on PATH. Doctests are not part of nextest,
# so invocations that cargo test would use to run doctests also run
# `cargo test --doc`.
cargo_test() {
  local -a args=("$@")
  local run_docs=0 has_selector=0 has_all_targets=0
  local -a doc_args=()
  local a

  for a in "${args[@]}"; do
    case "$a" in
      --lib|--tests|--doc|--bins|--examples|--bench|--benches)
        has_selector=1
        ;;
      --all-targets)
        has_all_targets=1
        ;;
    esac
    case "$a" in
      --all-targets|--lib|--tests|--bins|--examples|--bench|--benches|--doc)
        ;;
      *)
        doc_args+=("$a")
        ;;
    esac
  done
  if (( has_all_targets == 1 || has_selector == 0 )); then
    run_docs=1
  fi

  if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run "${args[@]}"
    if (( run_docs == 1 )); then
      cargo test --doc "${doc_args[@]}"
    fi
  else
    cargo test "${args[@]}"
  fi
}

case "$action" in
  fast-check)
    cargo check --package glass-browser-core --no-default-features --lib --locked
    cargo check --package glass-browser --lib --locked
    cargo check --package glass-dev --lib --bins --locked
    ;;
  fast-test)
    cargo build --package glass-browser --features native-engine --bin glass-native-content-worker --locked
    cargo_test --package glass-browser-core --features native-engine --lib --locked
    cargo_test --package glass-browser --lib --locked
    cargo_test --package glass-dev --lib --locked
    ;;
  fast-build)
    cargo build --package glass-dev --bin glass --locked
    ;;
  check)
    cargo check --package glass-browser-core --all-targets --features native-engine --locked
    cargo check --package glass-browser --all-targets --all-features --locked
    cargo check --package glass-dev --all-targets --all-features --locked
    ;;
  test)
    # Native-engine tests spawn this dependency-package process; cargo test
    # does not build binaries from a dependency package on its own.
    cargo build --package glass-browser --features native-engine --bin glass-native-content-worker --locked
    cargo_test --package glass-browser-core --all-targets --all-features --locked
    cargo_test --package glass-browser --all-targets --all-features --locked
    cargo_test --package glass-dev --all-targets --all-features --locked
    ;;
  clippy)
    cargo clippy --package glass-browser-core --all-targets --all-features --locked -- -D warnings
    cargo clippy --package glass-browser --all-targets --all-features --locked -- -D warnings
    cargo clippy --package glass-dev --all-targets --all-features --locked -- -D warnings
    ;;
  *)
    echo "usage: scripts/check-rust-workspace.sh [fast-check|fast-test|fast-build|check|test|clippy]" >&2
    exit 2
    ;;
esac
