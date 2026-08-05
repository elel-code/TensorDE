#!/usr/bin/env bash
set -u

usage() {
    cat <<'EOF'
Usage: check-runtime-integration.sh [--metadata-only] [--activate-system-helper] [--record FILE]

Checks an installed Tensor Files desktop integration setup.

Environment:
  PREFIX       Installation prefix, default /usr/local
  BINDIR       Binary directory, default $PREFIX/bin
  DATADIR      Data directory, default $PREFIX/share
  SYSCONFDIR   System config directory, default /etc
  DESTDIR      Optional staging root for metadata-only package checks

Options:
  --metadata-only
      Check installed metadata files only. This is safe for DESTDIR package
      checks and skips live D-Bus, polkit, and binary checks.

  --activate-system-helper
      Also introspect org.tensorde.TensorFiles1.Privileged on the system bus.
      This may start the root D-Bus activated helper, but does not call any
      privileged file-operation method.

  --record FILE
      Tee stdout and stderr to FILE with a small report header. This is meant
      for distro/desktop validation runs that need to be compared later.
EOF
}

original_args=("$@")
metadata_only=false
activate_system_helper=false
record_path=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --metadata-only)
            metadata_only=true
            ;;
        --activate-system-helper)
            activate_system_helper=true
            ;;
        --record)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--record requires a file path" >&2
                usage >&2
                exit 2
            fi
            record_path="$2"
            shift
            ;;
        --record=*)
            record_path="${1#--record=}"
            if [[ -z "$record_path" ]]; then
                echo "--record requires a file path" >&2
                usage >&2
                exit 2
            fi
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
    shift
done

prefix="${PREFIX:-/usr/local}"
bindir="${BINDIR:-$prefix/bin}"
datadir="${DATADIR:-$prefix/share}"
sysconfdir="${SYSCONFDIR:-/etc}"
destdir="${DESTDIR:-}"

privileged_bus_name="org.tensorde.TensorFiles1.Privileged"
privileged_object_path="/org/tensorde/TensorFiles1/Privileged"
privileged_interface="org.tensorde.TensorFiles1.Privileged"
polkit_action="org.tensorde.TensorFiles.privileged-helper"

privileged_service="$datadir/dbus-1/system-services/$privileged_bus_name.service"
privileged_policy="$sysconfdir/dbus-1/system.d/$privileged_bus_name.conf"
polkit_policy="$datadir/polkit-1/actions/org.tensorde.TensorFiles.policy"
privileged_interface_xml="$datadir/dbus-1/interfaces/$privileged_bus_name.xml"
tensor_files_binary="$bindir/tensor-files"

failures=0
warnings=0

ok() {
    printf 'ok: %s\n' "$*"
}

warn() {
    printf 'warn: %s\n' "$*" >&2
    warnings=$((warnings + 1))
}

fail() {
    printf 'fail: %s\n' "$*" >&2
    failures=$((failures + 1))
}

start_recording() {
    local path="$1"
    if [[ -z "$path" ]]; then
        return
    fi

    local dir
    dir="$(dirname -- "$path")"
    if [[ "$dir" != "." ]]; then
        mkdir -p -- "$dir" || {
            echo "cannot create report directory: $dir" >&2
            exit 1
        }
    fi

    : > "$path" || {
        echo "cannot write report file: $path" >&2
        exit 1
    }

    exec > >(tee -a "$path") 2> >(tee -a "$path" >&2)

    echo "Tensor Files runtime integration report"
    echo "  recorded_at: $(date -Is 2>/dev/null || date)"
    printf '  command:    %q' "$0"
    local arg
    for arg in "${original_args[@]}"; do
        printf ' %q' "$arg"
    done
    echo
    echo "  report:     $path"
    echo
}

first_line() {
    local text="$1"
    printf '%s' "${text%%$'\n'*}"
}

command_probe() {
    local tool="$1"
    shift

    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'missing'
        return
    fi

    local output
    if output="$("$tool" "$@" 2>&1)"; then
        printf 'available'
        local line
        line="$(first_line "$output")"
        if [[ -n "$line" ]]; then
            printf ' (%s)' "$line"
        fi
    else
        printf 'available, probe failed (%s)' "$(first_line "$output")"
    fi
}

env_state() {
    local name="$1"
    if [[ -n "${!name:-}" ]]; then
        printf 'set'
    else
        printf '<unset>'
    fi
}

systemctl_user_probe() {
    if ! command -v systemctl >/dev/null 2>&1; then
        printf 'missing'
        return
    fi

    local output
    if output="$(systemctl --user is-system-running 2>&1)"; then
        printf '%s' "$(first_line "$output")"
    else
        printf 'not-ready (%s)' "$(first_line "$output")"
    fi
}

systemctl_user_service_probe() {
    local service="$1"

    if ! command -v systemctl >/dev/null 2>&1; then
        printf 'systemctl missing'
        return
    fi

    local output
    if output="$(systemctl --user is-active "$service" 2>&1)"; then
        printf '%s' "$(first_line "$output")"
    else
        printf '%s' "$(first_line "$output")"
    fi
}

systemctl_system_service_probe() {
    local service="$1"

    if ! command -v systemctl >/dev/null 2>&1; then
        printf 'systemctl missing'
        return
    fi

    local output
    if output="$(systemctl is-active "$service" 2>&1)"; then
        printf '%s' "$(first_line "$output")"
    else
        printf '%s' "$(first_line "$output")"
    fi
}

polkit_agent_probe() {
    if ! command -v pgrep >/dev/null 2>&1; then
        printf 'unknown (pgrep missing)'
        return
    fi

    local pattern
    pattern='polkit.*agent|polkit-kde-authentication-agent|polkit-gnome-authentication-agent|lxqt-policykit-agent|mate-polkit|xfce-polkit|pantheon-agent-polkit'
    local output
    if output="$(pgrep -a -f "$pattern" 2>/dev/null)"; then
        printf 'detected (%s)' "$(first_line "$output")"
    else
        printf 'not detected'
    fi
}

print_runtime_context() {
    echo "Runtime context"

    if [[ -r /etc/os-release ]]; then
        (
            # shellcheck disable=SC1091
            . /etc/os-release
            echo "  os:         ${PRETTY_NAME:-${ID:-unknown} ${VERSION_ID:-}}"
        )
    else
        echo "  os:         <unknown>"
    fi

    echo "  kernel:     $(uname -srmo 2>/dev/null || uname -a)"
    echo "  desktop:    ${XDG_CURRENT_DESKTOP:-<unset>}"
    echo "  session:    ${XDG_SESSION_TYPE:-<unset>}"
    echo "  wayland:    ${WAYLAND_DISPLAY:-<unset>}"
    echo "  runtime:    ${XDG_RUNTIME_DIR:-<unset>}"
    echo "  session dbus: $(env_state DBUS_SESSION_BUS_ADDRESS)"
    echo "  systemd user: $(systemctl_user_probe)"
    echo "  polkit agent: $(polkit_agent_probe)"
    echo "  gvfs monitor: $(systemctl_user_service_probe gvfs-udisks2-volume-monitor.service)"
    echo "  tool dbus-send: $(command_probe dbus-send --version)"
    echo "  tool busctl:    $(command_probe busctl --version)"
    echo "  tool gdbus:     $(command_probe gdbus --version)"
    echo "  tool pkaction:  $(command_probe pkaction --version)"
    echo "  tool gio:       $(command_probe gio --version)"
    echo
}

staged_path() {
    printf '%s%s' "$destdir" "$1"
}

require_file() {
    local path="$1"
    local full_path
    full_path="$(staged_path "$path")"
    if [[ -f "$full_path" ]]; then
        ok "found $path"
    else
        fail "missing $path"
    fi
}

require_contains() {
    local path="$1"
    local text="$2"
    local full_path
    full_path="$(staged_path "$path")"
    if [[ ! -f "$full_path" ]]; then
        fail "cannot inspect missing $path"
    elif grep -Fq -- "$text" "$full_path"; then
        ok "$path contains $text"
    else
        fail "$path does not contain $text"
    fi
}

require_not_contains() {
    local path="$1"
    local text="$2"
    local full_path
    full_path="$(staged_path "$path")"
    if [[ ! -f "$full_path" ]]; then
        fail "cannot inspect missing $path"
    elif grep -Fq -- "$text" "$full_path"; then
        fail "$path must not contain $text"
    else
        ok "$path does not contain $text"
    fi
}

require_not_contains_tree() {
    local path="$1"
    local text="$2"
    local full_path
    full_path="$(staged_path "$path")"
    if [[ ! -e "$full_path" ]]; then
        return
    fi
    if grep -R -Fq -- "$text" "$full_path"; then
        fail "$path still contains $text"
    else
        ok "$path does not contain $text"
    fi
}

check_executable() {
    local path="$1"
    if [[ -x "$path" ]]; then
        ok "executable $path"
    else
        fail "missing executable $path"
    fi
}

dbus_list_activatable_contains() {
    local bus="$1"
    local name="$2"

    if ! command -v dbus-send >/dev/null 2>&1; then
        warn "dbus-send is not available; cannot query $bus bus activatable names"
        return
    fi

    local output
    if ! output="$(dbus-send "--$bus" --dest=org.freedesktop.DBus --print-reply \
        /org/freedesktop/DBus org.freedesktop.DBus.ListActivatableNames 2>&1)"; then
        warn "cannot query $bus bus activatable names: $output"
        return
    fi

    if grep -Fq -- "$name" <<<"$output"; then
        ok "$name is activatable on the $bus bus"
    else
        fail "$name is not activatable on the $bus bus"
    fi
}

dbus_name_has_owner() {
    local bus="$1"
    local name="$2"

    if ! command -v dbus-send >/dev/null 2>&1; then
        warn "dbus-send is not available; cannot query $bus bus owner for $name"
        return 1
    fi

    local output
    if ! output="$(dbus-send "--$bus" --dest=org.freedesktop.DBus --print-reply \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        "string:$name" 2>&1)"; then
        warn "cannot query $bus bus owner for $name: $output"
        return 1
    fi

    if grep -Fq -- "boolean true" <<<"$output"; then
        return 0
    fi

    return 1
}

dbus_optional_activatable_contains() {
    local bus="$1"
    local name="$2"

    if ! command -v dbus-send >/dev/null 2>&1; then
        warn "dbus-send is not available; cannot query $bus bus activatable names for $name"
        return 1
    fi

    local output
    if ! output="$(dbus-send "--$bus" --dest=org.freedesktop.DBus --print-reply \
        /org/freedesktop/DBus org.freedesktop.DBus.ListActivatableNames 2>&1)"; then
        warn "cannot query $bus bus activatable names for $name: $output"
        return 1
    fi

    grep -Fq -- "$name" <<<"$output"
}

check_polkit_action() {
    if ! command -v pkaction >/dev/null 2>&1; then
        warn "pkaction is not available; cannot query installed polkit actions"
        return
    fi

    local output
    if output="$(pkaction --verbose --action-id "$polkit_action" 2>&1)"; then
        ok "polkit action $polkit_action is visible"
        if grep -Fq -- "auth_admin_keep" <<<"$output"; then
            ok "polkit action advertises auth_admin_keep"
        else
            warn "polkit action output did not mention auth_admin_keep"
        fi
    else
        fail "polkit action $polkit_action is not visible: $output"
    fi
}

activate_system_helper() {
    if command -v busctl >/dev/null 2>&1; then
        local output
        if output="$(busctl --system introspect "$privileged_bus_name" \
            "$privileged_object_path" "$privileged_interface" 2>&1)"; then
            ok "system helper activated and exposes $privileged_interface"
        else
            fail "cannot introspect system helper with busctl: $output"
        fi
        return
    fi

    if command -v gdbus >/dev/null 2>&1; then
        local output
        if output="$(gdbus introspect --system --dest "$privileged_bus_name" \
            --object-path "$privileged_object_path" 2>&1)"; then
            ok "system helper activated and exposes $privileged_object_path"
        else
            fail "cannot introspect system helper with gdbus: $output"
        fi
        return
    fi

    fail "neither busctl nor gdbus is available; cannot activate-check system helper"
}

check_devices_runtime() {
    echo "Checking Devices runtime"

    if ! command -v gio >/dev/null 2>&1; then
        warn "gio is not available; cannot query GIO/GVfs mounts and volumes"
        echo
        return
    fi

    local output
    if output="$(gio mount -l 2>&1)"; then
        local mounts volumes
        mounts="$(grep -Ec '^(Mount|Drive)\(' <<<"$output" || true)"
        volumes="$(grep -Ec '^Volume\(' <<<"$output" || true)"
        ok "GIO mount monitor returned $mounts mounted/drive row(s) and $volumes volume row(s)"
        if [[ "$mounts" -eq 0 && "$volumes" -eq 0 ]]; then
            warn "GIO responded but exposed no storage rows; test with real removable media before closing Devices validation"
        fi
    else
        warn "cannot query GIO/GVfs mounts: $output"
    fi

    echo
}

print_live_validation_notes() {
    if [[ "$metadata_only" == true ]]; then
        return
    fi

    echo "Live validation notes"
    echo "  - Keep this output with the distro name, desktop, session type, and package version."
    echo "  - Re-run with --activate-system-helper when validating packaged system-bus activation."
    echo "  - Test with real removable media before closing GIO/GVfs mount/unmount/eject validation."
    echo
}

tensor_files_diagnostics_command() {
    if [[ -x "$tensor_files_binary" ]]; then
        printf '%s' "$tensor_files_binary"
        return 0
    fi

    if command -v tensor-files >/dev/null 2>&1; then
        command -v tensor-files
        return 0
    fi

    return 1
}

check_tensor_files_device_model() {
    echo "Checking Tensor Files Devices model"

    local tensor_files_cmd
    if ! tensor_files_cmd="$(tensor_files_diagnostics_command)"; then
        warn "cannot find tensor-files binary for --diagnose-devices; skipping UI model probe"
        echo
        return
    fi

    local output
    if command -v timeout >/dev/null 2>&1; then
        output="$(timeout 5 "$tensor_files_cmd" --diagnose-devices 2>&1)"
    else
        output="$("$tensor_files_cmd" --diagnose-devices 2>&1)"
    fi
    local status=$?

    if [[ "$status" -eq 0 ]]; then
        ok "$tensor_files_cmd --diagnose-devices completed"
        while IFS= read -r line; do
            printf '  %s\n' "$line"
        done <<<"$output"
    elif [[ "$status" -eq 124 ]]; then
        warn "$tensor_files_cmd --diagnose-devices timed out"
    else
        warn "$tensor_files_cmd --diagnose-devices failed: $(first_line "$output")"
    fi

    echo
}

start_recording "$record_path"

echo "Checking Tensor Files integration metadata"
echo "  bindir:     $bindir"
echo "  datadir:    $datadir"
echo "  sysconfdir: $sysconfdir"
echo "  destdir:    ${destdir:-<none>}"
echo "  record:     ${record_path:-<none>}"
echo

if [[ "$metadata_only" == false ]]; then
    print_runtime_context
    check_devices_runtime
    check_tensor_files_device_model
    print_live_validation_notes
fi

require_file "$privileged_service"
require_file "$privileged_policy"
require_file "$polkit_policy"
require_file "$privileged_interface_xml"

require_contains "$privileged_service" "Name=$privileged_bus_name"
require_contains "$privileged_service" "Exec=$bindir/tensor-files-privileged-helper --system-bus"
require_contains "$privileged_service" "User=root"
require_contains "$privileged_policy" '<policy user="root">'
require_contains "$privileged_policy" "<allow own=\"$privileged_bus_name\"/>"
require_contains "$privileged_policy" '<policy context="default">'
require_contains "$privileged_policy" "<allow send_destination=\"$privileged_bus_name\"/>"
require_contains "$polkit_policy" "$polkit_action"
require_contains "$polkit_policy" "<allow_active>auth_admin_keep</allow_active>"

for metadata_path in \
    "$privileged_service" \
    "$privileged_policy" \
    "$polkit_policy" \
    "$privileged_interface_xml"; do
    require_not_contains "$metadata_path" "@bindir@"
    require_not_contains "$metadata_path" "example.invalid"
done

require_not_contains "$privileged_service" "pkexec"
require_not_contains "$privileged_service" "--session-bus"

for method in CreateFolder CreateFile Rename Trash Transfer PrepareExternalEdit CommitExternalEdit DiscardExternalEdit AssociateExternalEditUnit; do
    require_contains "$privileged_interface_xml" "<method name=\"$method\">"
done

if [[ -n "$destdir" ]]; then
    require_not_contains_tree "/" "@bindir@"
    require_not_contains_tree "/" "example.invalid"
fi

if [[ "$metadata_only" == false ]]; then
    check_executable "$bindir/tensor-files-privileged-helper"
    dbus_list_activatable_contains system "$privileged_bus_name"
    check_polkit_action
fi

if [[ "$activate_system_helper" == true ]]; then
    activate_system_helper
fi

if [[ "$failures" -gt 0 ]]; then
    echo "Tensor Files integration check failed: $failures failure(s), $warnings warning(s)" >&2
    exit 1
fi

if [[ "$warnings" -gt 0 ]]; then
    echo "Tensor Files integration check completed with $warnings warning(s)"
else
    echo "Tensor Files integration check passed"
fi
