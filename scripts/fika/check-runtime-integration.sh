#!/usr/bin/env bash
set -u

usage() {
    cat <<'EOF'
Usage: check-runtime-integration.sh [--metadata-only] [--activate-system-helper] [--record FILE]

Checks an installed Fika desktop integration setup.

Environment:
  PREFIX       Installation prefix, default /usr/local
  BINDIR       Binary directory, default $PREFIX/bin
  DATADIR      Data directory, default $PREFIX/share
  SYSCONFDIR   System config directory, default /etc
  DESTDIR      Optional staging root for metadata-only package checks

Options:
  --metadata-only
      Check installed metadata files only. This is safe for DESTDIR package
      checks and skips live D-Bus, polkit, portal, and binary checks.

  --activate-system-helper
      Also introspect org.fika.FileManager1.Privileged on the system bus.
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

privileged_bus_name="org.fika.FileManager1.Privileged"
privileged_object_path="/org/fika/FileManager1/Privileged"
privileged_interface="org.fika.FileManager1.Privileged"
polkit_action="org.fika.FileManager.privileged-helper"
portal_bus_name="org.freedesktop.impl.portal.desktop.fika"

privileged_service="$datadir/dbus-1/system-services/$privileged_bus_name.service"
privileged_policy="$sysconfdir/dbus-1/system.d/$privileged_bus_name.conf"
polkit_policy="$datadir/polkit-1/actions/org.fika.FileManager.policy"
privileged_interface_xml="$datadir/dbus-1/interfaces/$privileged_bus_name.xml"
portal_service="$datadir/dbus-1/services/$portal_bus_name.service"
portal_descriptor="$datadir/xdg-desktop-portal/portals/fika.portal"
fika_binary="$bindir/fika"
portal_interface="org.freedesktop.impl.portal.FileChooser"

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

    echo "Fika runtime integration report"
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

trim() {
    local value="$1"
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    printf '%s' "$value"
}

env_state() {
    local name="$1"
    if [[ -n "${!name:-}" ]]; then
        printf 'set'
    else
        printf '<unset>'
    fi
}

backend_list_contains() {
    local value="$1"
    local backend="$2"
    local token
    local -a portal_backend_tokens

    IFS=';' read -r -a portal_backend_tokens <<< "$value"
    for token in "${portal_backend_tokens[@]}"; do
        token="$(trim "$token")"
        if [[ "$token" == "$backend" ]]; then
            return 0
        fi
    done

    return 1
}

portal_config_names() {
    local desktop
    local lowered
    local -a desktop_tokens

    IFS=':' read -r -a desktop_tokens <<< "${XDG_CURRENT_DESKTOP:-}"
    for desktop in "${desktop_tokens[@]}"; do
        desktop="$(trim "$desktop")"
        [[ -n "$desktop" ]] || continue
        lowered="${desktop,,}"
        printf '%s-portals.conf\n' "$lowered"
    done

    printf 'portals.conf\n'
}

portal_config_dirs() {
    local seen=":"
    local xdg_config_home="${XDG_CONFIG_HOME:-}"
    local config_dirs="${XDG_CONFIG_DIRS:-/etc/xdg}"
    local xdg_data_home="${XDG_DATA_HOME:-}"
    local data_dirs="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    local dir
    local -a dirs=()
    local -a split_dirs=()

    if [[ -n "$xdg_config_home" ]]; then
        dirs+=("$xdg_config_home/xdg-desktop-portal")
    elif [[ -n "${HOME:-}" ]]; then
        dirs+=("$HOME/.config/xdg-desktop-portal")
    fi

    IFS=':' read -r -a split_dirs <<< "$config_dirs"
    for dir in "${split_dirs[@]}"; do
        [[ -n "$dir" ]] && dirs+=("$dir/xdg-desktop-portal")
    done

    dirs+=("$sysconfdir/xdg-desktop-portal")

    if [[ -n "$xdg_data_home" ]]; then
        dirs+=("$xdg_data_home/xdg-desktop-portal")
    elif [[ -n "${HOME:-}" ]]; then
        dirs+=("$HOME/.local/share/xdg-desktop-portal")
    fi

    IFS=':' read -r -a split_dirs <<< "$data_dirs"
    for dir in "${split_dirs[@]}"; do
        [[ -n "$dir" ]] && dirs+=("$dir/xdg-desktop-portal")
    done

    dirs+=("$datadir/xdg-desktop-portal")

    for dir in "${dirs[@]}"; do
        case "$seen" in
            *":$dir:"*) continue ;;
        esac
        seen="${seen}${dir}:"

        printf '%s\n' "$dir"
    done
}

portal_config_candidates() {
    local dir
    local name
    local -a names=()

    while IFS= read -r name; do
        [[ -n "$name" ]] && names+=("$name")
    done < <(portal_config_names)

    while IFS= read -r dir; do
        [[ -d "$dir" ]] || continue
        for name in "${names[@]}"; do
            if [[ -f "$dir/$name" ]]; then
                printf '%s\n' "$dir/$name"
            fi
        done
    done < <(portal_config_dirs)
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
    echo "  xdp service:  $(systemctl_user_service_probe xdg-desktop-portal.service)"
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

portal_selection_has_preferred=0
portal_selection_filechooser_seen=0
portal_selection_filechooser_value=""
portal_selection_filechooser_fika=0
portal_selection_default_seen=0
portal_selection_default_value=""
portal_selection_default_fika=0

read_portal_selection_file() {
    local file="$1"
    local in_preferred=false
    local line
    local stripped
    local key
    local value
    local line_no=0

    portal_selection_has_preferred=0
    portal_selection_filechooser_seen=0
    portal_selection_filechooser_value=""
    portal_selection_filechooser_fika=0
    portal_selection_default_seen=0
    portal_selection_default_value=""
    portal_selection_default_fika=0

    while IFS= read -r line || [[ -n "$line" ]]; do
        line_no=$((line_no + 1))
        stripped="${line%%#*}"
        stripped="$(trim "$stripped")"

        [[ -z "$stripped" ]] && continue

        if [[ "$stripped" == \[*\] ]]; then
            if [[ "$stripped" == "[preferred]" ]]; then
                in_preferred=true
                portal_selection_has_preferred=1
            else
                in_preferred=false
            fi
            continue
        fi

        [[ "$in_preferred" == true ]] || continue
        [[ "$stripped" == *=* ]] || continue

        key="$(trim "${stripped%%=*}")"
        value="$(trim "${stripped#*=}")"

        case "$key" in
            "$portal_interface")
                portal_selection_filechooser_seen=1
                portal_selection_filechooser_value="$value"
                if backend_list_contains "$value" "fika"; then
                    portal_selection_filechooser_fika=1
                fi
                ;;
            default)
                portal_selection_default_seen=1
                portal_selection_default_value="$value"
                if backend_list_contains "$value" "fika"; then
                    portal_selection_default_fika=1
                fi
                ;;
        esac
    done < "$file"
}

check_portal_selection_config() {
    local active_config=""
    local file
    local -a configs=()

    echo "Checking portal backend selection"
    echo "  backend:    $portal_bus_name"
    echo "  interface:  $portal_interface"
    echo "  note:       installing fika.portal registers an independent backend; xdg-desktop-portal reads only the highest-priority matching portals.conf"

    while IFS= read -r file || [[ -n "$file" ]]; do
        [[ -n "$file" ]] || continue
        configs+=("$file")
    done < <(portal_config_candidates)

    if [[ "${#configs[@]}" -eq 0 ]]; then
        warn "no portals.conf files found in XDG config/data search paths; descriptor UseIn/default selection will decide the active backend"
    else
        active_config="${configs[0]}"
        echo "  active config: $active_config"
        if [[ "${#configs[@]}" -gt 1 ]]; then
            echo "  ignored lower-priority config(s):"
            for file in "${configs[@]:1}"; do
                echo "    $file"
            done
        fi

        read_portal_selection_file "$active_config"

        if [[ "$portal_selection_has_preferred" -eq 0 ]]; then
            warn "$active_config does not contain a [preferred] section"
        elif [[ "$portal_selection_filechooser_seen" -eq 1 ]]; then
            if [[ "$portal_selection_filechooser_fika" -eq 1 ]]; then
                ok "$active_config explicitly selects fika for $portal_interface"
            elif backend_list_contains "$portal_selection_filechooser_value" "none"; then
                warn "$active_config disables $portal_interface with none"
            elif backend_list_contains "$portal_selection_filechooser_value" "*"; then
                warn "$active_config uses wildcard selection for $portal_interface; cannot prove fika will win"
            else
                warn "$active_config selects $portal_interface without fika: $portal_selection_filechooser_value"
            fi
        elif [[ "$portal_selection_default_seen" -eq 1 ]]; then
            if [[ "$portal_selection_default_fika" -eq 1 ]]; then
                ok "$active_config default preference includes fika and no explicit FileChooser override is present"
            elif backend_list_contains "$portal_selection_default_value" "none"; then
                warn "$active_config disables default portal selection with none"
            elif backend_list_contains "$portal_selection_default_value" "*"; then
                warn "$active_config uses wildcard default portal selection; cannot prove fika will win"
            else
                warn "$active_config default preference does not include fika and no explicit FileChooser override is present: $portal_selection_default_value"
            fi
        else
            warn "$active_config has no $portal_interface or default preference"
        fi
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
    echo "  - Portal backend metadata is independent from FileChooser selection; use portals.conf to opt in to fika for XDP FileChooser validation."
    echo "  - Test with real removable media before closing GIO/GVfs mount/unmount/eject validation."
    echo
}

fika_diagnostics_command() {
    if [[ -x "$fika_binary" ]]; then
        printf '%s' "$fika_binary"
        return 0
    fi

    if command -v fika >/dev/null 2>&1; then
        command -v fika
        return 0
    fi

    return 1
}

check_fika_device_model() {
    echo "Checking Fika Devices model"

    local fika_cmd
    if ! fika_cmd="$(fika_diagnostics_command)"; then
        warn "cannot find fika binary for --diagnose-devices; skipping UI model probe"
        echo
        return
    fi

    local output
    if command -v timeout >/dev/null 2>&1; then
        output="$(timeout 5 "$fika_cmd" --diagnose-devices 2>&1)"
    else
        output="$("$fika_cmd" --diagnose-devices 2>&1)"
    fi
    local status=$?

    if [[ "$status" -eq 0 ]]; then
        ok "$fika_cmd --diagnose-devices completed"
        while IFS= read -r line; do
            printf '  %s\n' "$line"
        done <<<"$output"
    elif [[ "$status" -eq 124 ]]; then
        warn "$fika_cmd --diagnose-devices timed out"
    else
        warn "$fika_cmd --diagnose-devices failed: $(first_line "$output")"
    fi

    echo
}

start_recording "$record_path"

echo "Checking Fika integration metadata"
echo "  bindir:     $bindir"
echo "  datadir:    $datadir"
echo "  sysconfdir: $sysconfdir"
echo "  destdir:    ${destdir:-<none>}"
echo "  record:     ${record_path:-<none>}"
echo

if [[ "$metadata_only" == false ]]; then
    print_runtime_context
    check_devices_runtime
    check_fika_device_model
    print_live_validation_notes
fi

require_file "$privileged_service"
require_file "$privileged_policy"
require_file "$polkit_policy"
require_file "$privileged_interface_xml"
require_file "$portal_service"
require_file "$portal_descriptor"

require_contains "$privileged_service" "Name=$privileged_bus_name"
require_contains "$privileged_service" "Exec=$bindir/fika-privileged-helper --system-bus"
require_contains "$privileged_service" "User=root"
require_contains "$privileged_policy" '<policy user="root">'
require_contains "$privileged_policy" "<allow own=\"$privileged_bus_name\"/>"
require_contains "$privileged_policy" '<policy context="default">'
require_contains "$privileged_policy" "<allow send_destination=\"$privileged_bus_name\"/>"
require_contains "$polkit_policy" "$polkit_action"
require_contains "$polkit_policy" "<allow_active>auth_admin_keep</allow_active>"
require_contains "$portal_service" "Name=$portal_bus_name"
require_contains "$portal_service" "Exec=$bindir/fika-xdp-filechooser"
require_contains "$portal_descriptor" "DBusName=$portal_bus_name"
require_contains "$portal_descriptor" "Interfaces=org.freedesktop.impl.portal.FileChooser;"
require_contains "$portal_descriptor" "UseIn=fika"

for metadata_path in \
    "$privileged_service" \
    "$privileged_policy" \
    "$polkit_policy" \
    "$privileged_interface_xml" \
    "$portal_service" \
    "$portal_descriptor"; do
    require_not_contains "$metadata_path" "@bindir@"
    require_not_contains "$metadata_path" "example.invalid"
done

require_not_contains "$privileged_service" "pkexec"
require_not_contains "$privileged_service" "--session-bus"
require_not_contains "$portal_service" "pkexec"
require_not_contains "$portal_service" "--session-bus"

for method in CreateFolder CreateFile Rename Trash Transfer PrepareExternalEdit CommitExternalEdit DiscardExternalEdit AssociateExternalEditUnit; do
    require_contains "$privileged_interface_xml" "<method name=\"$method\">"
done

if [[ -n "$destdir" ]]; then
    require_not_contains_tree "/" "@bindir@"
    require_not_contains_tree "/" "example.invalid"
fi

if [[ "$metadata_only" == false ]]; then
    check_portal_selection_config
    check_executable "$bindir/fika-privileged-helper"
    check_executable "$bindir/fika-xdp-filechooser"
    dbus_list_activatable_contains system "$privileged_bus_name"
    dbus_list_activatable_contains session "$portal_bus_name"
    check_polkit_action
fi

if [[ "$activate_system_helper" == true ]]; then
    activate_system_helper
fi

if [[ "$failures" -gt 0 ]]; then
    echo "Fika integration check failed: $failures failure(s), $warnings warning(s)" >&2
    exit 1
fi

if [[ "$warnings" -gt 0 ]]; then
    echo "Fika integration check completed with $warnings warning(s)"
else
    echo "Fika integration check passed"
fi
