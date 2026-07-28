#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: dialog-lifecycle-smoke.sh [OPTIONS]

Runs the detached-dialog lifecycle autosmoke with focused window/event logs.
This should be run inside a real desktop session; headless sessions are not
valid dialog lifecycle evidence.

Options:
  --skip-build
      Do not run cargo build before launching Fika.

  --binary PATH
      Fika binary to run. Default: target/debug/fika.

  --path DIR
      Directory to open. Default: isolated temp fixture.

  --kind create|open-with|rename|settings
      Dialog kind to exercise. Default: open-with.

  --out-dir DIR
      Directory for logs. Default: /tmp.

  --timeout SECONDS
      Timeout for the GUI run. Default: 10.

  --cycles COUNT
      Open and close the dialog this many times. Default: 2.

  -h, --help
      Show this help.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/fika" && pwd)"
workspace_root="$(cd "$root_dir/../.." && pwd)"
binary="$workspace_root/target/debug/fika"
target_path=""
out_dir="/tmp"
timeout_seconds=10
skip_build=false
dialog_kind="open-with"
cycles=2

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)
            skip_build=true
            ;;
        --binary)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--binary requires a path" >&2
                usage >&2
                exit 2
            fi
            binary="$2"
            shift
            ;;
        --binary=*)
            binary="${1#--binary=}"
            ;;
        --path)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--path requires a directory" >&2
                usage >&2
                exit 2
            fi
            target_path="$2"
            shift
            ;;
        --path=*)
            target_path="${1#--path=}"
            ;;
        --out-dir)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--out-dir requires a directory" >&2
                usage >&2
                exit 2
            fi
            out_dir="$2"
            shift
            ;;
        --out-dir=*)
            out_dir="${1#--out-dir=}"
            ;;
        --kind)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--kind requires create, open-with, rename, or settings" >&2
                usage >&2
                exit 2
            fi
            dialog_kind="$2"
            shift
            ;;
        --kind=*)
            dialog_kind="${1#--kind=}"
            ;;
        --timeout)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--timeout requires seconds" >&2
                usage >&2
                exit 2
            fi
            timeout_seconds="$2"
            shift
            ;;
        --timeout=*)
            timeout_seconds="${1#--timeout=}"
            ;;
        --cycles)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--cycles requires a positive integer" >&2
                usage >&2
                exit 2
            fi
            cycles="$2"
            shift
            ;;
        --cycles=*)
            cycles="${1#--cycles=}"
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

case "$dialog_kind" in
    create|open-with|rename|settings)
        ;;
    *)
        echo "--kind must be create, open-with, rename, or settings" >&2
        usage >&2
        exit 2
        ;;
esac

if ! [[ "$cycles" =~ ^[1-9][0-9]*$ ]]; then
    echo "--cycles must be a positive integer" >&2
    usage >&2
    exit 2
fi

mkdir -p "$out_dir"
log_path="$out_dir/fika-dialog-lifecycle-smoke.log"
tmpdir="$(mktemp -d /tmp/fika-dialog-lifecycle-smoke.XXXXXX)"
cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

if [[ -z "$target_path" ]]; then
    target_path="$tmpdir/view"
    mkdir -p "$target_path"
    : >"$target_path/sample.txt"
fi

if [[ "$skip_build" == false ]]; then
    (cd "$workspace_root" && cargo build --bin fika)
fi

set +e
env \
    XDG_CONFIG_HOME="$tmpdir/config" \
    XDG_CACHE_HOME="$tmpdir/cache" \
    XDG_DATA_HOME="$tmpdir/data" \
    FIKA_LOG=1 \
    FIKA_WGPU_DIALOG_TRACE=1 \
    FIKA_WGPU_AUTOSMOKE_DIALOG_LIFECYCLE=1 \
    FIKA_WGPU_AUTOSMOKE_DIALOG_KIND="$dialog_kind" \
    FIKA_WGPU_AUTOSMOKE_DIALOG_CYCLES="$cycles" \
    timeout "${timeout_seconds}s" "$binary" "$target_path" >"$log_path" 2>&1
status=$?
set -e

if [[ $status -ne 0 && $status -ne 124 ]]; then
    echo "fail: dialog lifecycle run exited with status $status" >&2
    echo "log: $log_path" >&2
    exit 1
fi

if ! rg -q "\[fika-wgpu\] dialog-smoke complete" "$log_path"; then
    echo "fail: missing dialog-smoke complete marker" >&2
    echo "log: $log_path" >&2
    exit 1
fi

if ! rg -q "\[fika-wgpu\] renderer-shared-device" "$log_path"; then
    echo "fail: dialog renderer did not reuse the main wgpu device" >&2
    echo "log: $log_path" >&2
    exit 1
fi

if ! rg -q "\[fika-wgpu\] dialog-window-drop-deferred kind=$dialog_kind" "$log_path"; then
    echo "fail: missing deferred dialog drop marker" >&2
    echo "log: $log_path" >&2
    exit 1
fi

if rg -q "\[fika-wgpu\] dialog-window-(close-parked|reuse-parked) kind=$dialog_kind" "$log_path"; then
    echo "fail: dialog lifecycle used hidden parked window fallback" >&2
    echo "log: $log_path" >&2
    exit 1
fi

if rg -q "\[fika-wgpu\] event-loop-exit reason=main-close-requested" "$log_path"; then
    echo "fail: main window close requested during dialog lifecycle smoke" >&2
    echo "log: $log_path" >&2
    exit 1
fi

echo "ok: dialog lifecycle smoke complete"
echo "log: $log_path"
