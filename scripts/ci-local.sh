#!/usr/bin/env bash
# Local Linux pre-merge gates for Glass.
#
# Runs the same checks the Linux CI jobs use (fmt, clippy, tests, deny/audit,
# and Glass Core smokes) with mold and sccache when they are available.
# macOS, Windows, Playwright connect smokes, fuzz, and native certification
# still require GitHub Actions.
#
# Usage:
#   scripts/ci-local.sh [all|fmt|clippy|test|audit|core|smokes]
#   scripts/ci-local.sh --full          # also run scripts/release-certify.sh
#   scripts/ci-local.sh --build-core    # build core-release before smokes
#
# Environment:
#   RUSTC_WRAPPER / SCCACHE_CACHE_SIZE  set automatically when sccache exists
#   CI_LOCAL_JOBS                       smoke worker count (default: nproc)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \?//'
}

want_full=0
want_build_core=0
declare -a selections=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --full) want_full=1; shift ;;
    --build-core) want_build_core=1; shift ;;
    -h|--help) usage; exit 0 ;;
    all|fmt|clippy|test|audit|core|smokes) selections+=("$1"); shift ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ${#selections[@]} -eq 0 ]]; then
  selections=(all)
fi

arch="$(uname -m)"
os="$(uname -s)"

setup_local_speed() {
  if command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"
    export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-5G}"
    export SCCACHE_DIR="${SCCACHE_DIR:-${HOME}/.cache/sccache}"
    echo "using sccache (${SCCACHE_CACHE_SIZE} cache at ${SCCACHE_DIR})"
  else
    echo "sccache not on PATH; Rust compiles will be uncached"
  fi
  if command -v mold >/dev/null 2>&1; then
    echo "mold is installed; use rustflags -C link-arg=-fuse-ld=mold (see scripts/cargo-config.local.toml.example)"
  fi
}

host_triple() {
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64) echo "x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64) echo "aarch64-unknown-linux-musl" ;;
    Darwin-x86_64) echo "x86_64-apple-darwin" ;;
    Darwin-arm64) echo "aarch64-apple-darwin" ;;
    *) echo "" ;;
  esac
}

download_tarball() {
  local url="$1"
  local dest="$2"
  curl -fsSL "$url" -o "$dest"
}

ensure_bin_dir() {
  mkdir -p "${CARGO_HOME:-$HOME/.cargo}/bin"
  case ":$PATH:" in
    *":${CARGO_HOME:-$HOME/.cargo}/bin:"*) ;;
    *) export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH" ;;
  esac
}

install_from_tarball() {
  local url="$1"
  local bin_name="$2"
  ensure_bin_dir
  local tmp
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" RETURN
  echo "installing $bin_name from $url"
  if ! download_tarball "$url" "$tmp/tool.tar.gz"; then
    echo "failed to download $bin_name" >&2
    return 1
  fi
  tar -xzf "$tmp/tool.tar.gz" -C "$tmp"
  local found
  found="$(find "$tmp" -type f -name "$bin_name" | head -n 1)"
  if [[ -z "$found" ]]; then
    echo "archive did not contain $bin_name" >&2
    return 1
  fi
  install -m 0755 "$found" "${CARGO_HOME:-$HOME/.cargo}/bin/$bin_name"
}

install_cargo_deny() {
  local version="0.21.0" triple=""
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64) triple="x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64) triple="aarch64-unknown-linux-musl" ;;
    Darwin-arm64) triple="aarch64-apple-darwin" ;;
    Darwin-x86_64) triple="x86_64-apple-darwin" ;;
    *) echo "no prebuilt cargo-deny for $os-$arch" >&2; return 1 ;;
  esac
  install_from_tarball     "https://github.com/EmbarkStudios/cargo-deny/releases/download/${version}/cargo-deny-${version}-${triple}.tar.gz"     cargo-deny
}

install_cargo_audit() {
  local version="v0.23.0" triple=""
  # cargo-audit does not currently ship an aarch64-linux-musl build.
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64) triple="x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64) triple="aarch64-unknown-linux-gnu" ;;
    Darwin-arm64) triple="aarch64-apple-darwin" ;;
    Darwin-x86_64) triple="x86_64-apple-darwin" ;;
    *) echo "no prebuilt cargo-audit for $os-$arch" >&2; return 1 ;;
  esac
  install_from_tarball     "https://github.com/rustsec/rustsec/releases/download/cargo-audit/${version}/cargo-audit-${triple}-${version}.tgz"     cargo-audit
}

ensure_tools() {
  ensure_bin_dir
  if ! cargo nextest --version >/dev/null 2>&1; then
    local nextest_arch
    case "$os-$arch" in
      Linux-x86_64|Linux-amd64) nextest_arch="linux" ;;
      Linux-aarch64|Linux-arm64) nextest_arch="linux-arm" ;;
      Darwin-x86_64) nextest_arch="mac" ;;
      Darwin-arm64) nextest_arch="mac-arm" ;;
      *) nextest_arch="" ;;
    esac
    if [[ -n "$nextest_arch" ]]; then
      echo "installing cargo-nextest ($nextest_arch)"
      curl -LsSf "https://get.nexte.st/latest/${nextest_arch}" | tar zxf - -C "${CARGO_HOME:-$HOME/.cargo}/bin"
    else
      echo "cargo-nextest is required; install it from https://nexte.st" >&2
      exit 1
    fi
  fi
  if ! cargo deny --version >/dev/null 2>&1; then
    install_cargo_deny \
      || echo "warning: cargo-deny not installed; audit step will fail" >&2
  fi
  if ! cargo audit --version >/dev/null 2>&1; then
    install_cargo_audit \
      || echo "warning: cargo-audit not installed; audit step will fail" >&2
  fi
}

selected() {
  local name="$1"
  local item
  for item in "${selections[@]}"; do
    if [[ "$item" == "all" || "$item" == "$name" ]]; then
      return 0
    fi
  done
  return 1
}

core_binary() {
  if [[ -n "${GLASS_CORE_TEST_BINARY:-}" && -x "${GLASS_CORE_TEST_BINARY}" ]]; then
    printf '%s\n' "$GLASS_CORE_TEST_BINARY"
    return 0
  fi
  local candidate="$root/target/core-release/glass-core"
  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi
  return 1
}

setup_local_speed
ensure_tools

if (( want_full == 1 )); then
  echo "== release-certify =="
  scripts/release-certify.sh
fi

if selected fmt; then
  echo "== fmt =="
  cargo fmt --all -- --check
fi

if selected clippy; then
  echo "== clippy =="
  scripts/check-rust-workspace.sh clippy
fi

if selected test; then
  echo "== test =="
  scripts/check-rust-workspace.sh test
fi

if selected audit; then
  echo "== deny / audit =="
  cargo deny check
  cargo audit
fi

if selected core; then
  echo "== glass-core crates =="
  cargo clippy -p glass-core -p glass-mod-api -p glass-runtime-client -p glass-tui --all-targets --locked -- -D warnings
  cargo nextest run -p glass-core -p glass-mod-api -p glass-runtime-client -p glass-tui --locked
  cargo test -p glass-core -p glass-mod-api -p glass-runtime-client -p glass-tui --doc --locked
fi

if selected smokes; then
  echo "== glass-core smokes =="
  if (( want_build_core == 1 )) || ! core_binary >/dev/null; then
    echo "building glass-core with profile core-release"
    cargo build --profile core-release --locked --package glass-core --bin glass-core
  fi
  if ! bin="$(core_binary)"; then
    echo "no glass-core binary; pass --build-core or set GLASS_CORE_TEST_BINARY" >&2
    exit 1
  fi
  echo "using $bin"
  jobs="${CI_LOCAL_JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)}"
  GLASS_CORE_TEST_BINARY="$bin" python3 scripts/run-smokes.py \
    --jobs "$jobs" \
    --cwd "$root/products/glass-core" \
    --manifest "$root/products/glass-core/scripts/ci-smokes.txt"
fi

if command -v sccache >/dev/null 2>&1; then
  echo "== sccache stats =="
  sccache --show-stats || true
fi

echo "ci-local finished"
