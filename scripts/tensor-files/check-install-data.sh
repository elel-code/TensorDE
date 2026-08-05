#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/tensor-files" && pwd)"
tmpdir="$(mktemp -d)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

DESTDIR="$tmpdir" \
PREFIX=/usr \
BINDIR=/usr/lib/tensor-files \
DATADIR=/usr/share \
SYSCONFDIR=/etc \
    "$script_dir/install-data.sh" >/dev/null

bash -n "$script_dir/check-runtime-integration.sh"

require_file() {
    local path="$1"
    if [[ ! -f "$tmpdir$path" ]]; then
        echo "missing installed file: $path" >&2
        exit 1
    fi
}

require_contains() {
    local path="$1"
    local text="$2"
    if ! grep -Fq "$text" "$tmpdir$path"; then
        echo "installed file $path does not contain: $text" >&2
        exit 1
    fi
}

require_not_contains() {
    local path="$1"
    local text="$2"
    if grep -Fq "$text" "$tmpdir$path"; then
        echo "installed file $path still contains: $text" >&2
        exit 1
    fi
}

require_file /usr/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service
require_file /etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf
require_file /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy
require_file /usr/share/dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml

require_contains \
    /usr/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service \
    "Name=org.tensorde.TensorFiles1.Privileged"
require_contains \
    /usr/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service \
    "Exec=/usr/lib/tensor-files/tensor-files-privileged-helper --system-bus"
require_contains \
    /usr/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service \
    "User=root"
require_contains \
    /etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf \
    '<policy user="root">'
require_contains \
    /etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf \
    '<allow own="org.tensorde.TensorFiles1.Privileged"/>'
require_contains \
    /etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf \
    '<policy context="default">'
require_contains \
    /etc/dbus-1/system.d/org.tensorde.TensorFiles1.Privileged.conf \
    '<allow send_destination="org.tensorde.TensorFiles1.Privileged"/>'
for method in CreateFolder CreateFile Rename Trash Transfer PrepareExternalEdit CommitExternalEdit DiscardExternalEdit AssociateExternalEditUnit; do
    require_contains \
        /usr/share/dbus-1/interfaces/org.tensorde.TensorFiles1.Privileged.xml \
        "<method name=\"$method\">"
done
require_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "org.tensorde.TensorFiles.privileged-helper"
require_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "<description>Modify protected files with Tensor Files</description>"
require_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "<message>Authentication is required to modify protected files</message>"
require_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "<allow_active>auth_admin_keep</allow_active>"
require_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "<allow_any>no</allow_any>"
if grep -R "@bindir@" "$tmpdir" >/dev/null; then
    echo "installed data still contains @bindir@ placeholder" >&2
    exit 1
fi

if grep -R "example.invalid" "$tmpdir" >/dev/null; then
    echo "installed data still contains placeholder example.invalid metadata" >&2
    exit 1
fi

require_not_contains \
    /usr/share/dbus-1/system-services/org.tensorde.TensorFiles1.Privileged.service \
    "@bindir@"
require_not_contains \
    /usr/share/polkit-1/actions/org.tensorde.TensorFiles.policy \
    "example.invalid"

DESTDIR="$tmpdir" \
PREFIX=/usr \
BINDIR=/usr/lib/tensor-files \
DATADIR=/usr/share \
SYSCONFDIR=/etc \
    "$script_dir/check-runtime-integration.sh" --metadata-only >/dev/null

echo "install-data check passed"
