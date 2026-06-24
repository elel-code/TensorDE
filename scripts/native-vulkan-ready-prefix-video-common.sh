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
    if [[ "$present_mode" == "fifo" ]]; then
      printf 'target-fps-cpu-sleep-with-fifo-present\n'
    else
      printf 'target-fps-cpu-sleep\n'
    fi
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
    target-fps-cpu-sleep-with-fifo-present)
      printf 'audio-clock-master-with-target-fps-fallback-and-fifo-present\n'
      ;;
    target-fps-cpu-sleep)
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
