#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SWIFTPM_BUILD_DIR="${SWIFTPM_BUILD_DIR:-/tmp/marble-core-tests.build}"

# Live checks against a real Marble host. The token is read from the file by
# the tests and never printed.
export MARBLE_LIVE_HOST="${MARBLE_LIVE_HOST:-http://100.98.115.95:4783}"
export MARBLE_TOKEN_FILE="${MARBLE_TOKEN_FILE:-$HOME/.config/marble/token}"

cd "$APP_DIR"
MARBLE_HOST_TESTS=1 swift test --scratch-path "$SWIFTPM_BUILD_DIR" "$@"
