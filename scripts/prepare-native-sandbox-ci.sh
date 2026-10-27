#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update
sudo apt-get install --yes bubblewrap uidmap

runner_user="$(id -un)"
runner_group="$(id -gn)"
ensure_subordinate_range() {
    local kind="$1"
    local file="/etc/sub${kind}"
    if awk -F: -v user="$runner_user" \
        '$1 == user && $3 >= 65536 { found = 1 } END { exit !found }' "$file"; then
        return
    fi
    local range
    range="$(python3 - "$file" <<'PY'
import sys

path = sys.argv[1]
used = []
try:
    with open(path, encoding="ascii") as source:
        for line in source:
            fields = line.rstrip("\n").split(":")
            if len(fields) != 3:
                continue
            try:
                start, count = int(fields[1]), int(fields[2])
            except ValueError:
                continue
            if count > 0:
                used.append((start, start + count - 1))
except FileNotFoundError:
    pass

start = 100_000
while any(start <= end and start + 65_535 >= begin for begin, end in used):
    start = max(end for begin, end in used if start <= end and start + 65_535 >= begin) + 1
if start + 65_535 > 2**31 - 1:
    raise SystemExit("no free subordinate ID range is available")
print(f"{start}-{start + 65_535}")
PY
    )"
    printf 'Adding subordinate %s range %s for %s.\n' "$kind" "$range" "$runner_user"
    if [[ "$kind" == uid ]]; then
        sudo usermod --add-subuids "$range" "$runner_user"
    else
        sudo usermod --add-subgids "$range" "$runner_user"
    fi
}

ensure_subordinate_range uid
ensure_subordinate_range gid

printf 'Native sandbox runner=%s group=%s uid=%s gid=%s\n' \
    "$runner_user" "$runner_group" "$(id -u)" "$(id -g)"
printf 'Bubblewrap: %s\n' "$(bwrap --version)"
printf 'Subordinate UID/GID ranges of at least 65536 IDs: '
for kind in uid gid; do
    file="/etc/sub${kind}"
    if awk -F: -v user="$runner_user" \
        '$1 == user && $3 >= 65536 { found = 1 } END { exit !found }' "$file"; then
        printf '%s=ready ' "$kind"
    else
        printf '%s=missing ' "$kind"
    fi
done
printf '\n'
printf 'Bubblewrap mode=%s newuidmap=%s newgidmap=%s\n' \
    "$(stat -c '%a' "$(command -v bwrap)")" \
    "$(command -v newuidmap || printf unavailable)" \
    "$(command -v newgidmap || printf unavailable)"
if [[ -r /proc/self/status ]]; then
    awk '/^(NoNewPrivs|Seccomp):/ { print $1 "=" $2 }' /proc/self/status
fi
for setting in /proc/sys/kernel/unprivileged_userns_clone /proc/sys/user/max_user_namespaces; do
    if [[ -r "$setting" ]]; then
        printf '%s=%s\n' "$setting" "$(cat "$setting")"
    fi
done

# This mirrors the production outer namespaces and the nested-userns denial
# controls. CI must provide the same isolation the native worker requires.
bwrap \
    --unshare-user \
    --unshare-pid \
    --unshare-uts \
    --unshare-ipc \
    --disable-userns \
    --assert-userns-disabled \
    --uid 0 \
    --gid 0 \
    --ro-bind / / \
    --dev /dev \
    --proc /proc \
    --chdir / \
    /usr/bin/true

printf 'Bubblewrap production-equivalent namespace probe passed.\n'
