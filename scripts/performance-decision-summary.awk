function csv_split(line, out,    i, c, nextc, field, in_quotes, n) {
  delete out
  n = 1
  field = ""
  in_quotes = 0
  for (i = 1; i <= length(line); i += 1) {
    c = substr(line, i, 1)
    if (in_quotes) {
      if (c == "\"") {
        nextc = substr(line, i + 1, 1)
        if (nextc == "\"") {
          field = field "\""
          i += 1
        } else {
          in_quotes = 0
        }
      } else {
        field = field c
      }
    } else if (c == ",") {
      out[n] = field
      n += 1
      field = ""
    } else if (c == "\"" && field == "") {
      in_quotes = 1
    } else {
      field = field c
    }
  }
  out[n] = field
  return n
}

function numeric(value) {
  return value ~ /^[0-9]+$/
}

function update_range(key, value, min_values, max_values) {
  value += 0
  if (!(key in min_values) || value < min_values[key]) {
    min_values[key] = value
  }
  if (!(key in max_values) || value > max_values[key]) {
    max_values[key] = value
  }
}

function print_mode_reason(mode, reason,    key) {
  key = mode "/" reason
  if (mode_reason_count[key] > 0) {
    printf "mode_reason.%s: %d", key, mode_reason_count[key]
    if (key in min_decision_fps) {
      printf " fps_range=%d-%d", min_decision_fps[key], max_decision_fps[key]
    }
    printf "\n"
  }
}

function print_category(prefix, key, count_map) {
  if (key in count_map) {
    printf "%s.%s: %d\n", prefix, key, count_map[key]
  }
}

NR == 1 {
  next
}

{
  columns = csv_split($0, field)
  if (columns < 13) {
    malformed_rows += 1
    next
  }

  rows += 1
  sample = field[1]
  output_name = field[3]
  action = field[4]
  mode = field[5]
  reason = field[6]
  decision_fps = field[7]
  plan_kind = field[9]
  fit = field[11]
  target_fps = field[12]
  muted = field[13]

  if (sample != "" && !(sample in seen_samples)) {
    seen_samples[sample] = 1
    samples += 1
  }
  if (output_name != "" && !(output_name in seen_outputs)) {
    seen_outputs[output_name] = 1
    outputs += 1
  }

  mode_reason = mode "/" reason
  mode_reason_count[mode_reason] += 1
  mode_count[mode] += 1
  reason_count[reason] += 1
  action_count[action] += 1
  if (plan_kind != "") {
    plan_kind_count[plan_kind] += 1
  }
  if (fit != "") {
    fit_count[fit] += 1
  }
  if (muted != "") {
    muted_count[muted] += 1
  }
  if (numeric(decision_fps)) {
    update_range(mode_reason, decision_fps, min_decision_fps, max_decision_fps)
  }
  if (numeric(target_fps)) {
    update_range("all", target_fps, min_target_fps, max_target_fps)
    if (plan_kind != "") {
      update_range(plan_kind, target_fps, min_target_fps, max_target_fps)
    }
  }
}

END {
  printf "decision_rows: %d\n", rows
  printf "samples_with_decisions: %d\n", samples
  printf "outputs_seen: %d\n", outputs
  if (malformed_rows > 0) {
    printf "malformed_rows: %d\n", malformed_rows
  }

  print_mode_reason("active", "interactive")
  print_mode_reason("throttled", "unfocused")
  print_mode_reason("throttled", "battery")
  print_mode_reason("paused", "user-paused")
  print_mode_reason("paused", "session-inactive")
  print_mode_reason("paused", "session-locked")
  print_mode_reason("paused", "output-hidden")
  print_mode_reason("paused", "fullscreen")
  print_mode_reason("paused", "unfocused")
  print_mode_reason("paused", "battery")

  for (key in mode_reason_count) {
    if (key != "active/interactive" &&
        key != "throttled/unfocused" &&
        key != "throttled/battery" &&
        key != "paused/user-paused" &&
        key != "paused/session-inactive" &&
        key != "paused/session-locked" &&
        key != "paused/output-hidden" &&
        key != "paused/fullscreen" &&
        key != "paused/unfocused" &&
        key != "paused/battery") {
      printf "mode_reason.%s: %d", key, mode_reason_count[key]
      if (key in min_decision_fps) {
        printf " fps_range=%d-%d", min_decision_fps[key], max_decision_fps[key]
      }
      printf "\n"
    }
  }

  print_category("mode", "active", mode_count)
  print_category("mode", "throttled", mode_count)
  print_category("mode", "paused", mode_count)
  for (key in mode_count) {
    if (key != "active" && key != "throttled" && key != "paused") {
      print_category("mode", key, mode_count)
    }
  }

  print_category("reason", "interactive", reason_count)
  print_category("reason", "unfocused", reason_count)
  print_category("reason", "battery", reason_count)
  print_category("reason", "user-paused", reason_count)
  print_category("reason", "session-inactive", reason_count)
  print_category("reason", "session-locked", reason_count)
  print_category("reason", "output-hidden", reason_count)
  print_category("reason", "fullscreen", reason_count)
  for (key in reason_count) {
    if (key != "interactive" &&
        key != "unfocused" &&
        key != "battery" &&
        key != "user-paused" &&
        key != "session-inactive" &&
        key != "session-locked" &&
        key != "output-hidden" &&
        key != "fullscreen") {
      print_category("reason", key, reason_count)
    }
  }

  print_category("action", "render", action_count)
  print_category("action", "remove", action_count)
  for (key in action_count) {
    if (key != "render" && key != "remove") {
      print_category("action", key, action_count)
    }
  }

  print_category("plan_kind", "video", plan_kind_count)
  print_category("plan_kind", "static-image", plan_kind_count)
  for (key in plan_kind_count) {
    if (key != "video" && key != "static-image") {
      print_category("plan_kind", key, plan_kind_count)
    }
  }

  print_category("fit", "cover", fit_count)
  print_category("fit", "contain", fit_count)
  print_category("fit", "stretch", fit_count)
  print_category("fit", "tile", fit_count)
  print_category("fit", "center", fit_count)
  for (key in fit_count) {
    if (key != "cover" &&
        key != "contain" &&
        key != "stretch" &&
        key != "tile" &&
        key != "center") {
      print_category("fit", key, fit_count)
    }
  }

  print_category("muted", "true", muted_count)
  print_category("muted", "false", muted_count)
  for (key in muted_count) {
    if (key != "true" && key != "false") {
      print_category("muted", key, muted_count)
    }
  }

  if ("all" in min_target_fps) {
    printf "target_max_fps.all: %d-%d\n", min_target_fps["all"], max_target_fps["all"]
  }
  if ("video" in min_target_fps) {
    printf "target_max_fps.video: %d-%d\n", min_target_fps["video"], max_target_fps["video"]
  }
}
