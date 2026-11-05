#!/bin/sh
# Build and hot-sync the web shell into a running Connect gateway.
# Static assets are read from disk per request, so this does not restart
# gc connect or drop active thread bridges.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
product_root=$repo_root/products/glass-core
dist=$product_root/web/dist
data_dir=${XDG_DATA_HOME:-${HOME:?HOME must be set}/.local/share}/glass-core

resolve_target() {
  if [ -n "${1:-}" ]; then
    printf '%s\n' "$1"
    return
  fi
  if [ -n "${GC_WEB_ASSETS:-}" ]; then
    printf '%s\n' "$GC_WEB_ASSETS"
    return
  fi
  if [ -f "$data_dir/connect.json" ]; then
    python3 - <<PY
import json, pathlib
print(json.loads(pathlib.Path("$data_dir/connect.json").read_text())["assets"])
PY
    return
  fi
  install_root=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
  printf '%s\n' "$install_root/share/glass-core/web"
}

target=$(resolve_target "${1:-}")
if [ ! -f "$target/build.json" ] && [ ! -L "$target" ]; then
  printf '%s\n' "Refusing to sync into missing web bundle: $target" >&2
  printf '%s\n' "Pass the assets directory explicitly or start gc connect once." >&2
  exit 1
fi

npm --prefix "$product_root/web" run build
rsync -a --delete "$dist/" "$target/"

version=$(python3 -c "import json; print(json.load(open('$target/build.json'))['version'])")
printf '%s\n' "Synced web shell $version -> $target"
printf '%s\n' "Connect and thread bridges were not restarted."
printf '%s\n' "Reload the workspace tab (hard refresh on Safari if the shell looks stale)."
