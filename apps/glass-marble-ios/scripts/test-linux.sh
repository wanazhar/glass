#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# A dedicated scratch build dir keeps the host test build away from the iOS
# cross-compile .build used by build-ipa-linux.sh.
export SWIFTPM_BUILD_DIR="${SWIFTPM_BUILD_DIR:-/tmp/marble-core-tests.build}"

cd "$APP_DIR"
MARBLE_HOST_TESTS=1 swift test --scratch-path "$SWIFTPM_BUILD_DIR" "$@"
