#!/usr/bin/env bash
set -euo pipefail

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SWIFTPM_BUILD_DIR="${SWIFTPM_BUILD_DIR:-/tmp/marble-core-tests.build}"

# Live checks. By default they run against an isolated `glass-marble serve`
# launched here on 127.0.0.1 (temp data dir, fake engine, temp token) — never
# the real host. To run against an existing host, set both explicitly:
#   MARBLE_LIVE_HOST=http://host:port MARBLE_TOKEN_FILE=/path/to/token ./test-live.sh
ROOT=""
PID=""
cleanup() { [ -n "$PID" ] && kill "$PID" 2>/dev/null || true; [ -n "$ROOT" ] && rm -rf "$ROOT"; }
trap cleanup EXIT

# Hard guard: the isolated instance must never touch the real host's data or
# workspace, even if a caller overrides the temp dir locations.
realpath_of() { python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1"; }
case "$(realpath_of "${MARBLE_TEST_ROOT:-/tmp/marble-live-test.placeholder}")" in
    "$HOME/.local/share/glass-marble"* | "$HOME/work/marble-workspace"*)
        echo "refusing to run an isolated host inside the live data/workspace dir" >&2
        exit 1 ;;
esac

if [ -z "${MARBLE_LIVE_HOST:-}" ]; then
    ROOT="${MARBLE_TEST_ROOT:-$(mktemp -d /tmp/marble-live-test.XXXXXX)}"
    mkdir -p "$ROOT/workspace" "$ROOT/data"
    printf 'e2e fixture\n' > "$ROOT/workspace/notes.txt"
    printf 'live-test-token\n' > "$ROOT/token"
    PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
    BINARY="${MARBLE_BINARY:-$HOME/.local/lib/marble/glass-marble}"
    export GLASS_CORE_BINARY="${GLASS_CORE_BINARY:-$APP_DIR/../../crates/glass-marble/tests/fixtures/fake_glass_core_runtime.py}"
    "$BINARY" --cwd "$ROOT/workspace" --data-dir "$ROOT/data" \
        serve --host 127.0.0.1 --port "$PORT" --token-file "$ROOT/token" \
        > "$ROOT/serve.log" 2>&1 &
    PID=$!
    for _ in $(seq 1 40); do
        curl -sf "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 && break
        sleep 0.5
    done
    export MARBLE_LIVE_HOST="http://127.0.0.1:$PORT"
    export MARBLE_TOKEN_FILE="$ROOT/token"
    # The fixture is disposable, so tests may cause state changes on it.
    export MARBLE_LIVE_ISOLATED=1
else
    # An explicitly provided host is taken as a real one.
    export MARBLE_TOKEN_FILE="${MARBLE_TOKEN_FILE:?MARBLE_TOKEN_FILE is required with MARBLE_LIVE_HOST}"
fi

cd "$APP_DIR"
MARBLE_HOST_TESTS=1 swift test --scratch-path "$SWIFTPM_BUILD_DIR" "$@"
