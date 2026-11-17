#!/usr/bin/env bash
# Package size-optimized Glass user binaries for a GitHub Release asset.
# Archives are named to match [package.metadata.binstall] in glass-dev and
# glass-browser: glass-<rustc-target>.tar.gz (or .zip on Windows).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: package-release-binaries.sh --target <triple> --output-dir <dir> [--profile release-size] [--bin-dir <dir>]

Copies glass, glass-browser, and glass-native-content-worker out of the
profile directory (or --bin-dir), packs them, and writes a SHA-256 checksum
next to the archive.
EOF
}

target=""
profile="release-size"
output_dir=""
bin_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:-}"
      shift 2
      ;;
    --profile)
      profile="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    --bin-dir)
      bin_dir="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$target" || -z "$output_dir" ]]; then
  usage >&2
  exit 2
fi

if [[ ! "$target" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  echo "invalid target triple: $target" >&2
  exit 2
fi

if [[ -z "$bin_dir" ]]; then
  bin_dir="target/${profile}"
fi

suffix=""
archive_name="glass-${target}.tar.gz"
if [[ "$target" == *-windows-* ]]; then
  suffix=".exe"
  archive_name="glass-${target}.zip"
fi

bins=(glass glass-browser glass-native-content-worker)
staging="$(mktemp -d "${TMPDIR:-/tmp}/glass-release-pack.XXXXXX")"
cleanup() { rm -rf "$staging"; }
trap cleanup EXIT

for name in "${bins[@]}"; do
  src="${bin_dir}/${name}${suffix}"
  if [[ ! -f "$src" ]]; then
    echo "missing binary: $src" >&2
    exit 1
  fi
  cp "$src" "${staging}/${name}${suffix}"
  if [[ -z "$suffix" ]]; then
    chmod 0755 "${staging}/${name}${suffix}"
  fi
done

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
archive_path="${output_dir}/${archive_name}"

# macOS tar otherwise embeds AppleDouble metadata in the archive.
export COPYFILE_DISABLE=1
export COPY_EXTENDED_ATTRIBUTES_DISABLE=1

python3 - "$staging" "$archive_path" "$suffix" <<'PY'
import hashlib
import sys
import tarfile
import zipfile
from pathlib import Path

staging = Path(sys.argv[1])
archive = Path(sys.argv[2])
windows = sys.argv[3] == ".exe"
names = sorted(path.name for path in staging.iterdir() if path.is_file())
if archive.suffix == ".zip" or windows:
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for name in names:
            zf.write(staging / name, arcname=name)
else:
    with tarfile.open(archive, "w:gz") as tf:
        for name in names:
            tf.add(staging / name, arcname=name)

digest = hashlib.sha256(archive.read_bytes()).hexdigest()
checksum = archive.with_name(archive.name + ".sha256")
checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
print(f"wrote {archive.name} ({archive.stat().st_size} bytes)")
print(f"sha256 {digest}")
PY
