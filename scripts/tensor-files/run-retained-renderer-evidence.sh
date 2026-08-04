#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: run-retained-renderer-evidence.sh [OPTIONS]

Captures and analyzes retained item-view and Places runtime evidence. Run this
from a real desktop session; headless/sandbox sessions are not valid runtime
evidence.

Options:
  --core
      Capture item-view and Places evidence. This is the default.

  --items-only
      Capture only item-view Vulkan frame evidence.

  --places-only
      Capture only Places evidence.

  --metadata-tail-scroll
      Capture a small extensionless-file directory with Vulkan autosmoke scroll
      enabled and gate successful frame submission during tail scrolling.

  --all
      Same as --core --metadata-tail-scroll.

  --analyze-only
      Do not launch Tensor Files. Re-run analyzers against existing logs.

  --skip-build
      Do not run cargo build before launching Tensor Files.

  --binary PATH
      Tensor Files binary to run. Default: target/debug/tensor-files.

  --out-dir DIR
      Directory for logs. Default: /tmp.

  --prefix NAME
      Log filename prefix. Default: tensor-files-evidence.

  --downloads DIR
      Mixed user directory for item-view evidence. Default: $HOME/Downloads.

  --timeout SECONDS
      Timeout for each GUI capture. Default: 8.

  -h, --help
      Show this help.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/../../apps/tensor-files" && pwd)"
workspace_root="$(cd "$root_dir/../.." && pwd)"
binary="$workspace_root/target/debug/tensor-files"
out_dir="/tmp"
prefix="tensor-files-evidence"
downloads_dir="${HOME:-}/Downloads"
timeout_seconds=8
capture_items=false
capture_places=false
capture_metadata_tail_scroll=false
analyze_only=false
skip_build=false
explicit_selection=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --core)
            explicit_selection=true
            capture_items=true
            capture_places=true
            ;;
        --items-only)
            explicit_selection=true
            capture_items=true
            capture_places=false
            ;;
        --places-only)
            explicit_selection=true
            capture_items=false
            capture_places=true
            ;;
        --metadata-tail-scroll)
            explicit_selection=true
            capture_metadata_tail_scroll=true
            ;;
        --all)
            explicit_selection=true
            capture_items=true
            capture_places=true
            capture_metadata_tail_scroll=true
            ;;
        --analyze-only)
            analyze_only=true
            ;;
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
        --out-dir)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--out-dir requires a path" >&2
                usage >&2
                exit 2
            fi
            out_dir="$2"
            shift
            ;;
        --out-dir=*)
            out_dir="${1#--out-dir=}"
            ;;
        --prefix)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--prefix requires a name" >&2
                usage >&2
                exit 2
            fi
            prefix="$2"
            shift
            ;;
        --prefix=*)
            prefix="${1#--prefix=}"
            ;;
        --downloads)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--downloads requires a directory" >&2
                usage >&2
                exit 2
            fi
            downloads_dir="$2"
            shift
            ;;
        --downloads=*)
            downloads_dir="${1#--downloads=}"
            ;;
        --timeout)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "--timeout requires a number of seconds" >&2
                usage >&2
                exit 2
            fi
            timeout_seconds="$2"
            shift
            ;;
        --timeout=*)
            timeout_seconds="${1#--timeout=}"
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

if [[ "$explicit_selection" != true ]]; then
    capture_items=true
    capture_places=true
fi

if [[ "$analyze_only" != true && "$skip_build" != true ]]; then
    (cd "$root_dir" && cargo build)
fi

mkdir -p "$out_dir"

log_path() {
    printf '%s/%s-%s.log' "$out_dir" "$prefix" "$1"
}

run_capture() {
    local label="$1"
    local log="$2"
    shift 2

    if [[ "$analyze_only" == true ]]; then
        if [[ ! -s "$log" ]]; then
            echo "missing log for --analyze-only: $log" >&2
            exit 1
        fi
        return
    fi

    echo "capture: $label -> $log"
    set +e
    timeout "${timeout_seconds}s" "$@" > "$log" 2>&1
    local status=$?
    set -e

    if [[ $status -ne 0 && $status -ne 124 ]]; then
        echo "capture failed ($status): $label" >&2
        tail -80 "$log" >&2 || true
        exit "$status"
    fi
    if grep -q "NoCompositor" "$log"; then
        echo "capture is not valid desktop-session evidence: $label hit GPUI NoCompositor" >&2
        tail -80 "$log" >&2 || true
        exit 1
    fi
}

run_gate() {
    local label="$1"
    shift
    echo "analyze: $label"
    "$@"
}

prepare_metadata_tail_fixture() {
    local dir="$1"
    mkdir -p "$dir"
    find "$dir" -mindepth 1 -maxdepth 1 -type f -delete

    local i
    for i in $(seq -w 1 96); do
        # Extensionless PDF-like payloads force MIME magic role updates from
        # the initial generic binary role without relying on external assets.
        printf '%%PDF-1.7\n%% tensor-files metadata tail fixture %s\n' "$i" > "$dir/payload-$i"
    done
}

expect_gate_failure() {
    local label="$1"
    shift
    echo "analyze expected-fail: $label"
    set +e
    "$@" >/tmp/tensor-files-retained-renderer-expected-fail.out 2>/tmp/tensor-files-retained-renderer-expected-fail.err
    local status=$?
    set -e
    if [[ $status -eq 0 ]]; then
        echo "expected analyzer failure but command passed: $label" >&2
        cat /tmp/tensor-files-retained-renderer-expected-fail.out >&2
        exit 1
    fi
}

if [[ "$capture_metadata_tail_scroll" == true ]]; then
    metadata_tail_log="$(log_path metadata-tail-scroll)"
    metadata_tail_dir="$out_dir/$prefix-metadata-tail-fixture"

    if [[ "$analyze_only" != true ]]; then
        prepare_metadata_tail_fixture "$metadata_tail_dir"
    fi

    run_capture "metadata tail scroll" "$metadata_tail_log" \
        env \
            TENSOR_FILES_LOG=1 \
            TENSOR_FILES_FRAME_LOG_ALL=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL_RAPID=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL_STEP=160 \
            TENSOR_FILES_AUTOSMOKE_SCROLL_FORWARD_COUNT=18 \
            TENSOR_FILES_AUTOSMOKE_SCROLL_BACK_COUNT=0 \
            "$binary" --view icons "$metadata_tail_dir"

    run_gate "metadata tail-scroll Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --gate-scope all \
        "$metadata_tail_log"

    run_gate "metadata tail-scroll autosmoke submission" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --require-autosmoke-scroll \
        --gate-scope reason:autosmoke-scroll \
        "$metadata_tail_log"
fi

if [[ "$capture_items" == true ]]; then
    item_downloads_log="$(log_path item-downloads)"
    item_etc_log="$(log_path item-etc-compact)"
    item_zoom_log="$(log_path item-etc-compact-zoom-scroll)"
    item_icons_log="$(log_path item-etc-icons-zoom-scroll)"
    item_details_log="$(log_path item-etc-details-zoom-scroll)"

    run_capture "item downloads" "$item_downloads_log" \
        env TENSOR_FILES_LOG=1 TENSOR_FILES_FRAME_LOG_ALL=1 "$binary" "$downloads_dir"
    run_capture "item etc compact" "$item_etc_log" \
        env TENSOR_FILES_LOG=1 TENSOR_FILES_FRAME_LOG_ALL=1 "$binary" --view compact /etc
    run_capture "item etc compact zoom-scroll" "$item_zoom_log" \
        env TENSOR_FILES_LOG=1 TENSOR_FILES_FRAME_LOG_ALL=1 \
            TENSOR_FILES_AUTOSMOKE_ZOOM=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL=1 \
            "$binary" --view compact /etc
    run_capture "item etc icons zoom-scroll" "$item_icons_log" \
        env TENSOR_FILES_LOG=1 TENSOR_FILES_FRAME_LOG_ALL=1 \
            TENSOR_FILES_AUTOSMOKE_ZOOM=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL=1 \
            "$binary" --view icons /etc
    run_capture "item etc details zoom-scroll" "$item_details_log" \
        env TENSOR_FILES_LOG=1 TENSOR_FILES_FRAME_LOG_ALL=1 \
            TENSOR_FILES_AUTOSMOKE_ZOOM=1 \
            TENSOR_FILES_AUTOSMOKE_SCROLL=1 \
            "$binary" --view details /etc

    run_gate "item downloads Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        "$item_downloads_log"
    run_gate "item compact Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --gate-scope view:compact \
        "$item_etc_log"
    run_gate "item compact zoom-scroll Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --require-autosmoke-scroll \
        --gate-scope reason:autosmoke-scroll \
        "$item_zoom_log"
    run_gate "item icons zoom-scroll Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --require-autosmoke-scroll \
        --gate-scope view:icons \
        "$item_icons_log"
    run_gate "item details zoom-scroll Vulkan frames" \
        bash "$script_dir/analyze-vulkan-frame-log.sh" \
        --require-frames \
        --require-autosmoke-scroll \
        --gate-scope view:details \
        "$item_details_log"
fi

if [[ "$capture_places" == true ]]; then
    places_targets_log="$(log_path places-targets)"
    places_overflow_log="$(log_path places-overflow)"
    places_layout_log="$(log_path places-layout)"
    places_hit_test_log="$(log_path places-hit-test)"
    places_targeting_log="$(log_path places-targeting)"
    places_dnd_log="$(log_path places-dnd)"

    run_capture "places targets" "$places_targets_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=targets "$binary" /etc
    run_capture "places overflow" "$places_overflow_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=overflow "$binary" /etc
    run_capture "places layout" "$places_layout_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=layout "$binary" /etc
    run_capture "places hit-test" "$places_hit_test_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=hit-test "$binary" /etc
    run_capture "places targeting" "$places_targeting_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=targeting "$binary" /etc
    run_capture "places dnd" "$places_dnd_log" \
        env TENSOR_FILES_PERF_PLACES_VIEW=1 TENSOR_FILES_AUTOSMOKE_PLACES=dnd "$binary" /etc

    places_analyzer="$script_dir/analyze-places-perf.sh"
    places_common=(--require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy)
    run_gate "places targets" "$places_analyzer" --require-autosmoke "${places_common[@]}" "$places_targets_log"
    run_gate "places overflow" "$places_analyzer" --require-overflow-autosmoke "${places_common[@]}" "$places_overflow_log"
    run_gate "places layout" "$places_analyzer" --require-layout-autosmoke "${places_common[@]}" "$places_layout_log"
    run_gate "places hit-test" "$places_analyzer" --require-hit-test-autosmoke "${places_common[@]}" "$places_hit_test_log"
    run_gate "places targeting" "$places_analyzer" --require-retained-targeting-autosmoke "${places_common[@]}" "$places_targeting_log"
    run_gate "places dnd" "$places_analyzer" --require-retained-dnd-autosmoke "${places_common[@]}" "$places_dnd_log"
    run_gate "places full retained-event" \
        "$places_analyzer" --expect-retained-event-policy "$places_dnd_log"
fi

echo "retained renderer evidence complete"
