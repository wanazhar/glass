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

# Pinned prebuilt tools. SHA-256 values for nextest and cargo-deny come from
# the GitHub release checksum files; cargo-audit does not publish those, so
# the hashes below were computed from the official release archives.
NEXTEST_VERSION="0.9.152"
CARGO_DENY_VERSION="0.21.0"
CARGO_AUDIT_VERSION="v0.23.0"

ensure_bin_dir() {
  mkdir -p "${CARGO_HOME:-$HOME/.cargo}/bin"
  case ":$PATH:" in
    *":${CARGO_HOME:-$HOME/.cargo}/bin:"*) ;;
    *) export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH" ;;
  esac
}

file_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

install_from_tarball() {
  local url="$1"
  local sha256="$2"
  local bin_name="$3"
  ensure_bin_dir
  local tmp actual
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" RETURN
  echo "installing $bin_name from $url"
  if ! curl -fsSL "$url" -o "$tmp/tool.tar.gz"; then
    echo "failed to download $bin_name" >&2
    return 1
  fi
  actual="$(file_sha256 "$tmp/tool.tar.gz")"
  if [[ "$actual" != "$sha256" ]]; then
    echo "SHA-256 mismatch for $bin_name: got $actual, expected $sha256" >&2
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

install_cargo_nextest() {
  local triple="" sha256=""
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64)
      triple="x86_64-unknown-linux-musl"
      sha256="521c5f790ed07ee69d0a88403d9d86a8c8ab27977f12fc73f22aa4e7e14a0f71"
      ;;
    Linux-aarch64|Linux-arm64)
      triple="aarch64-unknown-linux-musl"
      sha256="f168dbfe9c9c3a4b247cf10d9c260f3dac90a1373f5c4b1bfd3b0b8ac8d77095"
      ;;
    Darwin-x86_64|Darwin-arm64)
      triple="universal-apple-darwin"
      sha256="2fadf57df4243e691f088ee3bff4f4ab9af16da0c95a1db84b5cb1685e098a76"
      ;;
    *) echo "no pinned cargo-nextest build for $os-$arch" >&2; return 1 ;;
  esac
  install_from_tarball     "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST_VERSION}/cargo-nextest-${NEXTEST_VERSION}-${triple}.tar.gz"     "$sha256"     cargo-nextest
}

install_cargo_deny() {
  local triple="" sha256=""
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64)
      triple="x86_64-unknown-linux-musl"
      sha256="a2170b854ff697e79e39c4c538981c9c1bdd803addbae6719a4cb66b4585a903"
      ;;
    Linux-aarch64|Linux-arm64)
      triple="aarch64-unknown-linux-musl"
      sha256="47cc5d88d43cd568028080fece02d729f56f0ff8e6c168563ce6589e3c0f4738"
      ;;
    Darwin-arm64)
      triple="aarch64-apple-darwin"
      sha256="c99a789160775ada8cb54aed72cfdae5a9b72e27434f8868d17c9634c3f52425"
      ;;
    *) echo "no pinned cargo-deny build for $os-$arch" >&2; return 1 ;;
  esac
  install_from_tarball     "https://github.com/EmbarkStudios/cargo-deny/releases/download/${CARGO_DENY_VERSION}/cargo-deny-${CARGO_DENY_VERSION}-${triple}.tar.gz"     "$sha256"     cargo-deny
}

install_cargo_audit() {
  # cargo-audit does not currently ship an aarch64-linux-musl build.
  local triple="" sha256=""
  case "$os-$arch" in
    Linux-x86_64|Linux-amd64)
      triple="x86_64-unknown-linux-musl"
      sha256="4f01c74ed84b2dffd0b1d37012a62cdf2a92c96e25bdf8eec7fcc0d4d3eb7811"
      ;;
    Linux-aarch64|Linux-arm64)
      triple="aarch64-unknown-linux-gnu"
      sha256="00eff3158429b9629abe8748e971f29f99ee53d4cff54a25072cbb15167bdda8"
      ;;
    Darwin-arm64)
      triple="aarch64-apple-darwin"
      sha256="84101204e5b8013c9a8d7251766aebdef3e7fa1a3ef040cde44d65b48b8c4056"
      ;;
    Darwin-x86_64)
      triple="x86_64-apple-darwin"
      sha256="845ec02452dd233c292be887700ac99a38a697299423081ee0d8ea1a0c961464"
      ;;
    *) echo "no pinned cargo-audit build for $os-$arch" >&2; return 1 ;;
  esac
  install_from_tarball     "https://github.com/rustsec/rustsec/releases/download/cargo-audit/${CARGO_AUDIT_VERSION}/cargo-audit-${triple}-${CARGO_AUDIT_VERSION}.tgz"     "$sha256"     cargo-audit
}

tool_version_matches() {
  local wanted="$1"
  shift
  local out
  out="$("$@" 2>/dev/null || true)"
  [[ "$out" == *"$wanted"* ]]
}

ensure_tools() {
  ensure_bin_dir
  if ! tool_version_matches "$NEXTEST_VERSION" cargo nextest --version; then
    install_cargo_nextest       || { echo "cargo-nextest ${NEXTEST_VERSION} is required" >&2; exit 1; }
  fi
  if ! tool_version_matches "$CARGO_DENY_VERSION" cargo deny --version; then
    install_cargo_deny \
      || echo "warning: cargo-deny ${CARGO_DENY_VERSION} not installed; audit step will fail" >&2
  fi
  if ! tool_version_matches "${CARGO_AUDIT_VERSION#v}" cargo audit --version; then
    install_cargo_audit \
      || echo "warning: cargo-audit ${CARGO_AUDIT_VERSION} not installed; audit step will fail" >&2
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
