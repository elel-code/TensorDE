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

gilder_append_ready_prefix_runtime_env() {
  local env_array_name="${1:?runtime env array name is required}"
  local -n runtime_env_ref="$env_array_name"
  local glibc_tunables="${GLIBC_TUNABLES:-}"

  runtime_env_ref+=("MALLOC_ARENA_MAX=${MALLOC_ARENA_MAX:-1}")
  runtime_env_ref+=("MALLOC_MMAP_THRESHOLD_=${MALLOC_MMAP_THRESHOLD_:-131072}")
  runtime_env_ref+=("MALLOC_TRIM_THRESHOLD_=${MALLOC_TRIM_THRESHOLD_:-0}")

  if [[ "$glibc_tunables" != *glibc.malloc.tcache_count=* ]]; then
    if [[ -n "$glibc_tunables" ]]; then
      glibc_tunables="${glibc_tunables}:glibc.malloc.tcache_count=0"
    else
      glibc_tunables="glibc.malloc.tcache_count=0"
    fi
  fi
  runtime_env_ref+=("GLIBC_TUNABLES=$glibc_tunables")

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
      printf 'audio-clock-master-with-target-fps-fallback\n'
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
