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
if [ "${GC_INSTALL_WEB:-0}" = 1 ]; then
    if [ ! -f "$repo_root/web/dist/app.js" ]; then
        npm --prefix "$repo_root/web" ci --ignore-scripts
        npm --prefix "$repo_root/web" run build
    fi
    web_parent=$install_root/share/glass-core
    mkdir -p "$web_parent"
    web_stage=$(mktemp -d "$web_parent/.web-install.XXXXXX")
    cp -R "$repo_root/web/dist/." "$web_stage/"
    if [ -e "$web_parent/web" ]; then
        if [ -L "$web_parent/web" ] || [ ! -f "$web_parent/web/build.json" ]; then
            printf '%s\n' "Refusing to replace an unrelated web asset directory: $web_parent/web" >&2
            exit 1
        fi
        rm -rf "$web_parent/web"
    fi
    mv "$web_stage" "$web_parent/web"
fi
ln -sfn glass-core "$bin_dir/gc"
printf '%s\n' "Installed $bin_dir/glass-core" "$bin_dir/gc -> glass-core"
