#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"

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
    "$root_dir/data/dbus-1/system-services/org.fika.FileManager1.Privileged.service.in" \
    "$datadir/dbus-1/system-services/org.fika.FileManager1.Privileged.service"

install_file \
    "$root_dir/data/dbus-1/system.d/org.fika.FileManager1.Privileged.conf" \
    "$sysconfdir/dbus-1/system.d/org.fika.FileManager1.Privileged.conf"

install_template \
    "$root_dir/data/polkit-1/actions/org.fika.FileManager.policy.in" \
    "$datadir/polkit-1/actions/org.fika.FileManager.policy"

install_file \
    "$root_dir/data/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml" \
    "$datadir/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml"

install_template \
    "$root_dir/data/dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service.in" \
    "$datadir/dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service"

install_file \
    "$root_dir/data/xdg-desktop-portal/portals/fika.portal" \
    "$datadir/xdg-desktop-portal/portals/fika.portal"

cat <<EOF
Installed Fika desktop integration data:
  bindir:     $bindir
  datadir:    $datadir
  sysconfdir: $sysconfdir
  destdir:    ${destdir:-<none>}
EOF
