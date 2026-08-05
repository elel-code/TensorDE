#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/tensor-files" && pwd)"

prefix="${PREFIX:-/usr/local}"
bindir="${BINDIR:-$prefix/bin}"
datadir="${DATADIR:-$prefix/share}"
sysconfdir="${SYSCONFDIR:-/etc}"
destdir="${DESTDIR:-}"

install_file() {
    local source="$1"
    local target="$2"
    install -Dm644 "$source" "$destdir$target"
}

install_template() {
    local source="$1"
    local target="$2"
    local tmp
    tmp="$(mktemp)"
    sed "s|@bindir@|$bindir|g" "$source" > "$tmp"
    install -Dm644 "$tmp" "$destdir$target"
    rm -f "$tmp"
}

install_template \
    "$root_dir/data/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service.in" \
    "$datadir/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service"

install_file \
    "$root_dir/data/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf" \
    "$sysconfdir/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf"

install_template \
    "$root_dir/data/polkit-1/actions/org.tensorde.TensorFiles.policy.in" \
    "$datadir/polkit-1/actions/org.tensorde.TensorFiles.policy"

install_file \
    "$root_dir/data/dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml" \
    "$datadir/dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml"

cat <<EOF
Installed Tensor Files desktop integration data:
  bindir:     $bindir
  datadir:    $datadir
  sysconfdir: $sysconfdir
  destdir:    ${destdir:-<none>}
EOF
