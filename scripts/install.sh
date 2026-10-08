#!/bin/sh
# Install only glass-core, with gc as a symlink. Glass belongs to another project.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
install_root=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-${HOME:?HOME or CARGO_HOME must be set}/.cargo}}
bin_dir=$install_root/bin

# Check the alias before building or modifying installed files.
if [ -e "$bin_dir/gc" ] || [ -L "$bin_dir/gc" ]; then
    if [ ! -L "$bin_dir/gc" ]; then
        printf '%s\n' "Refusing to replace unrelated command: $bin_dir/gc" >&2
        exit 1
    fi
    gc_target=$(readlink "$bin_dir/gc")
    if [ "$gc_target" != glass-core ] && [ "$gc_target" != "$bin_dir/glass-core" ]; then
        printf '%s\n' "Refusing to replace unrelated symlink: $bin_dir/gc -> $gc_target" >&2
        exit 1
    fi
fi

cargo install --locked --force --root "$install_root" --path "$repo_root/crates/glass" --bin glass-core
ln -sfn glass-core "$bin_dir/gc"
printf '%s\n' "Installed $bin_dir/glass-core" "$bin_dir/gc -> glass-core"
