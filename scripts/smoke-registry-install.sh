#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: scripts/smoke-registry-install.sh VERSION}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "expected stable crate version, got: $version" >&2
  exit 2
fi

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
user_agent='glass-browser-release-ci (+https://github.com/wanazhar/glass)'

wait_for_registry_crate() {
  local crate="$1"
  local response_file="$root/${crate}.json"
  local status
  for attempt in $(seq 1 30); do
    status="$(curl --silent --show-error --user-agent "$user_agent" \
      --output "$response_file" --write-out '%{http_code}' \
      "https://crates.io/api/v1/crates/${crate}/${version}")"
    case "$status" in
      200)
        echo "${crate} ${version} is visible in the crates.io API"
        return 0
        ;;
      404)
        ;;
      *)
        echo "unexpected crates.io API response for ${crate} ${version}: HTTP ${status}" >&2
        cat "$response_file" >&2
        return 1
        ;;
    esac
    if [[ "$attempt" != 30 ]]; then
      sleep 10
    fi
  done
  echo "${crate} ${version} did not become visible in the crates.io API" >&2
  return 1
}

install_registry_crate() {
  local crate="$1"
  local install_root="$root/$crate"
  for attempt in $(seq 1 3); do
    rm -rf "$install_root"
    if CARGO_NET_RETRY=2 cargo install "$crate" --version "=$version" \
      --locked --root "$install_root"; then
      return 0
    fi
    if [[ "$attempt" != 3 ]]; then
      sleep 10
    fi
  done
  echo "cargo install failed for ${crate} ${version} after bounded retries" >&2
  return 1
}

browser_registry_status=0
dev_registry_status=0
wait_for_registry_crate glass-browser & browser_registry_pid=$!
wait_for_registry_crate glass-dev & dev_registry_pid=$!
wait "$browser_registry_pid" || browser_registry_status=$?
wait "$dev_registry_pid" || dev_registry_status=$?
test "$browser_registry_status" -eq 0
test "$dev_registry_status" -eq 0

browser_status=0
dev_status=0
install_registry_crate glass-browser & browser_pid=$!
install_registry_crate glass-dev & dev_pid=$!
wait "$browser_pid" || browser_status=$?
wait "$dev_pid" || dev_status=$?
test "$browser_status" -eq 0
test "$dev_status" -eq 0

"$root/glass-browser/bin/glass-browser" --help >/dev/null
"$root/glass-dev/bin/glass" --help >/dev/null
"$root/glass-dev/bin/glass-browser" --help >/dev/null
echo "registry installation smoke passed for glass-browser and glass-dev ${version}"
