#!/usr/bin/env bash

gilder_default_source_cache_dir() {
  local codec="${1:?codec is required}"
  printf 'artifacts/video-sources/%s\n' "$codec"
}

gilder_ensure_source_cache_dir() {
  local dir="${1:?source cache dir is required}"
  mkdir -p "$dir"
}

gilder_pts_delta_expected_min_ms() {
  local fps="${1:?target fps is required}"
  printf '%s\n' $((1000 / fps))
}

gilder_pts_delta_expected_max_ms() {
  local fps="${1:?target fps is required}"
  printf '%s\n' $(((1000 + fps - 1) / fps))
}

gilder_pts_delta_expected_bounds_ms() {
  local fps="${1:?target fps is required}"
  printf '%s %s\n' \
    "$(gilder_pts_delta_expected_min_ms "$fps")" \
    "$(gilder_pts_delta_expected_max_ms "$fps")"
}

gilder_is_uint() {
  local value="${1:?value is required}"
  [[ "$value" =~ ^[0-9]+$ ]]
}

gilder_summary_value() {
  local summary="${1:?summary is required}"
  local key="${2:?key is required}"

  awk -F': ' -v key="$key" '
    $1 == key {
      print $2
      found = 1
      exit
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$summary"
}

gilder_summary_uint_or_zero() {
  local summary="${1:?summary is required}"
  local key="${2:?key is required}"
  local value

  value="$(gilder_summary_value "$summary" "$key" 2>/dev/null || true)"
  if gilder_is_uint "$value"; then
    printf '%s\n' "$value"
  else
    printf '0\n'
  fi
}

gilder_sync_rebuilt_executable() {
  local binary_path="${1:?binary path is required}"
  local binary_dir

  [[ -e "$binary_path" ]] || return 1
  binary_dir="$(dirname "$binary_path")"

  sync -d "$binary_path"
  sync "$binary_path"
  sync "$binary_dir" 2>/dev/null || true
}

gilder_rebuild_mapping_dirty_contaminated() {
  local summary="${1:?summary is required}"
  local limit="${2:?limit is required}"
  local min_mapping_dirty="${3:-1024}"
  local max_private_dirty
  local file_mapping_dirty
  local gilder_binary_dirty
  local suspect_mapping_dirty

  [[ -r "$summary" ]] || return 1
  gilder_is_uint "$limit" || return 1
  gilder_is_uint "$min_mapping_dirty" || return 1

  max_private_dirty="$(gilder_summary_uint_or_zero "$summary" max_private_dirty_kib)"
  file_mapping_dirty="$(gilder_summary_uint_or_zero "$summary" memory_category_file_mapping_private_dirty_kib)"
  gilder_binary_dirty="$(gilder_summary_uint_or_zero "$summary" memory_category_gilder_binary_private_dirty_kib)"

  suspect_mapping_dirty="$file_mapping_dirty"
  if [[ "$gilder_binary_dirty" -gt "$suspect_mapping_dirty" ]]; then
    suspect_mapping_dirty="$gilder_binary_dirty"
  fi

  [[ "$max_private_dirty" -gt "$limit" && "$suspect_mapping_dirty" -ge "$min_mapping_dirty" ]]
}

gilder_rebuild_heap_dirty_contaminated() {
  local summary="${1:?summary is required}"
  local limit="${2:?limit is required}"
  local min_heap_dirty="${3:-8192}"
  local max_private_dirty
  local heap_dirty
  local file_mapping_dirty
  local gilder_binary_dirty

  [[ -r "$summary" ]] || return 1
  gilder_is_uint "$limit" || return 1
  gilder_is_uint "$min_heap_dirty" || return 1

  max_private_dirty="$(gilder_summary_uint_or_zero "$summary" max_private_dirty_kib)"
  heap_dirty="$(gilder_summary_uint_or_zero "$summary" memory_category_heap_private_dirty_kib)"
  file_mapping_dirty="$(gilder_summary_uint_or_zero "$summary" memory_category_file_mapping_private_dirty_kib)"
  gilder_binary_dirty="$(gilder_summary_uint_or_zero "$summary" memory_category_gilder_binary_private_dirty_kib)"

  [[ "$max_private_dirty" -gt "$limit" && "$heap_dirty" -ge "$min_heap_dirty" && "$file_mapping_dirty" -lt 1024 && "$gilder_binary_dirty" -lt 1024 ]]
}

gilder_rebuild_dirty_contaminated() {
  local summary="${1:?summary is required}"
  local limit="${2:?limit is required}"

  gilder_rebuild_mapping_dirty_contaminated "$summary" "$limit" \
    || gilder_rebuild_heap_dirty_contaminated "$summary" "$limit"
}

gilder_append_ready_prefix_runtime_env() {
  local env_array_name="${1:?runtime env array name is required}"
  local -n runtime_env_ref="$env_array_name"
  runtime_env_ref=(
    -u MALLOC_ARENA_MAX
    -u MALLOC_MMAP_THRESHOLD_
    -u MALLOC_TRIM_THRESHOLD_
    -u GLIBC_TUNABLES
    "${runtime_env_ref[@]}"
  )

  for passthrough_env in \
    VK_LOADER_LAYERS_ENABLE \
    VK_LAYER_KHRONOS_validation_LOG_FILENAME; do
    if [[ -n "${!passthrough_env:-}" ]]; then
      runtime_env_ref+=("${passthrough_env}=${!passthrough_env}")
    fi
  done
}

gilder_pts_delta_in_expected_range() {
  local actual_min="${1:?actual min pts delta is required}"
  local actual_max="${2:?actual max pts delta is required}"
  local fps="${3:?target fps is required}"
  local expected_min
  local expected_max

  if ! gilder_is_uint "$actual_min" || ! gilder_is_uint "$actual_max"; then
    return 1
  fi

  expected_min="$(gilder_pts_delta_expected_min_ms "$fps")"
  expected_max="$(gilder_pts_delta_expected_max_ms "$fps")"
  [[ "$actual_min" -ge "$expected_min" && "$actual_max" -le "$expected_max" ]]
}

gilder_expected_pacing_strategy() {
  local present_mode="${1:?present mode is required}"
  local fps="${2:?target fps is required}"

  if [[ "$fps" -gt 0 ]]; then
    printf 'ffmpeg-frame-timer-pts-delta-sleep\n'
  elif [[ "$present_mode" == "fifo" ]]; then
    printf 'fifo-present-blocking-no-cpu-sleep\n'
  else
    printf 'unlimited\n'
  fi
}

gilder_native_video_present_mode_allowed() {
  local present_mode="${1:?present mode is required}"

  case "$present_mode" in
    fifo-latest-ready|fifo-relaxed|fifo)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

gilder_expected_pacing_strategy_with_master() {
  local present_mode="${1:?present mode is required}"
  local fps="${2:?target fps is required}"
  local pacing_master="${3:-target}"
  local base

  base="$(gilder_expected_pacing_strategy "$present_mode" "$fps")"
  if [[ "$pacing_master" != "audio" ]]; then
    printf '%s\n' "$base"
    return
  fi

  case "$base" in
    ffmpeg-frame-timer-pts-delta-sleep)
      printf 'audio-clock-master-pts-sync-sleep\n'
      ;;
    fifo-present-blocking-no-cpu-sleep)
      printf 'audio-clock-master-with-fifo-present\n'
      ;;
    *)
      printf 'audio-clock-master\n'
      ;;
  esac
}

gilder_pacing_strategy_matches_expected() {
  local actual="${1:?actual pacing strategy is required}"
  local expected="${2:?expected pacing strategy is required}"
  local fps="${3:?target fps is required}"

  if [[ "$actual" == "$expected" ]]; then
    return 0
  fi
  if [[ "$fps" -gt 0 ]]; then
    case "$actual" in
      ffmpeg-frame-timer-first-frame|\
      ffmpeg-frame-timer-pts-delta-sleep|\
      ffmpeg-frame-timer-last-duration-sleep|\
      ffmpeg-frame-timer-duration-sleep|\
      ffmpeg-frame-timer-target-fps-sleep)
        return 0
        ;;
      pts-video-clock-sleep|pts-ns-video-clock-sleep)
        return 0
        ;;
    esac
  fi
  return 1
}
