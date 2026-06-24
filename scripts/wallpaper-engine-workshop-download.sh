#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/wallpaper-engine-workshop-download.sh --item-id <id> [options] [-- matrix-args...]

Download user-accessible Wallpaper Engine Workshop items with SteamCMD into a
local ignored corpus, then optionally probe the downloaded media with the native
Vulkan real-source matrix. The script does not copy Workshop assets into the
tracked repository.

Options:
  --item-id <id>        Steam Workshop item id. May be repeated.
  --item-list <file>    File containing one Workshop item id per line. Blank
                        lines and lines starting with # are ignored.
  --appid <id>          Workshop app id. Default: 431960 (Wallpaper Engine).
  --download-root <dir> SteamCMD install/download root. Default:
                        artifacts/wallpaper-engine-workshop/steamcmd-root.
  --steamcmd <path>     SteamCMD executable. Default: STEAMCMD,
                        artifacts/tools/steamcmd/steamcmd.sh, then steamcmd.
  --steamcmd-dir <dir>  Repository-local SteamCMD install dir. Default:
                        artifacts/tools/steamcmd.
  --install-steamcmd    Download and install SteamCMD into --steamcmd-dir when
                        the executable is missing. Requires curl and tar.
  --install-steamcmd-only
                        Install SteamCMD into --steamcmd-dir, then exit without
                        downloading Workshop items.
  --anonymous           Use anonymous SteamCMD login, ignoring GILDER_STEAM_USER.
  --steam-user <name>   Use a Steam account login. SteamCMD may prompt for
                        password/Steam Guard; this script never stores them.
                        Default: GILDER_STEAM_USER when set, otherwise
                        anonymous.
  --probe-after-download
                        Run scripts/native-vulkan-real-source-matrix.sh against
                        the downloaded Workshop content after SteamCMD exits.
  --matrix-report-dir <dir>
                        Report directory for --probe-after-download. Default:
                        artifacts/video-real-source-matrix/we-<timestamp>.
  --dry-run             Print the SteamCMD and matrix commands without running.
  -h, --help            Show this help text.

Arguments after -- are forwarded to native-vulkan-real-source-matrix.sh, for
example:
  -- --run-video --output-name HDMI-A-1 --audio-clock-probe --duration 10
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

appid=431960
item_ids=()
item_list=""
download_root="$repo_root/artifacts/wallpaper-engine-workshop/steamcmd-root"
steamcmd="${STEAMCMD:-}"
steamcmd_dir="$repo_root/artifacts/tools/steamcmd"
install_steamcmd=0
install_steamcmd_only=0
steam_user="${GILDER_STEAM_USER:-}"
probe_after_download=0
matrix_report_dir=""
dry_run=0
matrix_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --item-id)
      item_ids+=("${2:?--item-id requires an id}")
      shift 2
      ;;
    --item-list)
      item_list="${2:?--item-list requires a file}"
      shift 2
      ;;
    --appid)
      appid="${2:?--appid requires an id}"
      shift 2
      ;;
    --download-root)
      download_root="${2:?--download-root requires a directory}"
      shift 2
      ;;
    --steamcmd)
      steamcmd="${2:?--steamcmd requires a path}"
      shift 2
      ;;
    --steamcmd-dir)
      steamcmd_dir="${2:?--steamcmd-dir requires a directory}"
      shift 2
      ;;
    --install-steamcmd)
      install_steamcmd=1
      shift
      ;;
    --install-steamcmd-only)
      install_steamcmd=1
      install_steamcmd_only=1
      shift
      ;;
    --anonymous)
      steam_user=""
      shift
      ;;
    --steam-user)
      steam_user="${2:?--steam-user requires a username}"
      shift 2
      ;;
    --probe-after-download|--run-matrix)
      probe_after_download=1
      shift
      ;;
    --matrix-report-dir)
      matrix_report_dir="${2:?--matrix-report-dir requires a directory}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    --)
      shift
      matrix_args=("$@")
      break
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! "$appid" =~ ^[0-9]+$ ]]; then
  printf 'FAIL: --appid must be numeric\n' >&2
  exit 2
fi

install_repository_steamcmd() {
  local install_dir="${1:?install dir is required}"
  local archive="$install_dir/steamcmd_linux.tar.gz"

  for tool in curl tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      printf 'FAIL: --install-steamcmd requires %s\n' "$tool" >&2
      exit 1
    fi
  done

  mkdir -p "$install_dir"
  curl -L "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz" \
    -o "$archive"
  tar -xzf "$archive" -C "$install_dir"
  if [[ ! -x "$install_dir/steamcmd.sh" ]]; then
    printf 'FAIL: SteamCMD install did not produce %s\n' "$install_dir/steamcmd.sh" >&2
    exit 1
  fi
}

if [[ -z "$steamcmd" ]]; then
  if [[ -x "$steamcmd_dir/steamcmd.sh" || "$install_steamcmd" -eq 1 ]]; then
    steamcmd="$steamcmd_dir/steamcmd.sh"
  else
    steamcmd="steamcmd"
  fi
fi

if [[ "$install_steamcmd_only" -eq 1 ]]; then
  if [[ "$dry_run" -eq 1 ]]; then
    printf 'DRY-RUN: install SteamCMD into %s\n' "$steamcmd_dir"
    exit 0
  fi
  install_repository_steamcmd "$steamcmd_dir"
  printf 'PASS: SteamCMD installed\n'
  printf 'steamcmd: %s\n' "$steamcmd_dir/steamcmd.sh"
  exit 0
fi

if [[ -n "$item_list" ]]; then
  if [[ ! -f "$item_list" ]]; then
    printf 'FAIL: item list does not exist: %s\n' "$item_list" >&2
    exit 1
  fi
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%#*}"
    line="${line//[[:space:]]/}"
    if [[ -n "$line" ]]; then
      item_ids+=("$line")
    fi
  done <"$item_list"
fi
if [[ "${#item_ids[@]}" -eq 0 ]]; then
  printf 'FAIL: pass at least one --item-id or --item-list\n' >&2
  usage >&2
  exit 2
fi

unique_ids=()
seen_ids=()
for item_id in "${item_ids[@]}"; do
  if [[ ! "$item_id" =~ ^[0-9]+$ ]]; then
    printf 'FAIL: Workshop item id must be numeric: %s\n' "$item_id" >&2
    exit 2
  fi
  already_seen=0
  for seen in "${seen_ids[@]}"; do
    if [[ "$seen" == "$item_id" ]]; then
      already_seen=1
      break
    fi
  done
  if [[ "$already_seen" -eq 0 ]]; then
    seen_ids+=("$item_id")
    unique_ids+=("$item_id")
  fi
done

if [[ "$dry_run" -eq 0 ]]; then
  if ! command -v "$steamcmd" >/dev/null 2>&1 && [[ ! -x "$steamcmd" ]]; then
    if [[ "$install_steamcmd" -eq 1 ]]; then
      install_repository_steamcmd "$steamcmd_dir"
      steamcmd="$steamcmd_dir/steamcmd.sh"
    else
      printf 'FAIL: missing SteamCMD executable: %s\n' "$steamcmd" >&2
      printf 'Install steamcmd, pass --steamcmd /path/to/steamcmd, or pass --install-steamcmd.\n' >&2
      exit 1
    fi
  fi
fi

timestamp="$(date +%Y%m%d-%H%M%S)-$$"
if [[ -z "$matrix_report_dir" ]]; then
  matrix_report_dir="$repo_root/artifacts/video-real-source-matrix/we-$timestamp"
fi
summary_dir="$repo_root/artifacts/wallpaper-engine-workshop/reports/$timestamp"
mkdir -p "$summary_dir"
summary="$summary_dir/summary.txt"
commands_file="$summary_dir/commands.sh"
content_dir="$download_root/steamapps/workshop/content/$appid"

steamcmd_args=("$steamcmd" +force_install_dir "$download_root")
if [[ -n "$steam_user" ]]; then
  steamcmd_args+=(+login "$steam_user")
else
  steamcmd_args+=(+login anonymous)
fi
for item_id in "${unique_ids[@]}"; do
  steamcmd_args+=(+workshop_download_item "$appid" "$item_id")
done
steamcmd_args+=(+quit)

matrix_command=(
  "$repo_root/scripts/native-vulkan-real-source-matrix.sh"
  --workshop-dir "$content_dir"
  --report-dir "$matrix_report_dir"
)
if [[ "${#matrix_args[@]}" -gt 0 ]]; then
  matrix_command+=("${matrix_args[@]}")
fi

write_quoted_command() {
  local -n command_ref="$1"
  local part
  for part in "${command_ref[@]}"; do
    printf '%q ' "$part"
  done
  printf '\n'
}

{
  printf '#!/usr/bin/env bash\n'
  printf 'set -euo pipefail\n'
  write_quoted_command steamcmd_args
  if [[ "$probe_after_download" -eq 1 ]]; then
    write_quoted_command matrix_command
  fi
} >"$commands_file"
chmod +x "$commands_file"

{
  printf 'appid: %s\n' "$appid"
  printf 'item_count: %s\n' "${#unique_ids[@]}"
  printf 'download_root: %s\n' "$download_root"
  printf 'content_dir: %s\n' "$content_dir"
  printf 'steamcmd: %s\n' "$steamcmd"
  printf 'steamcmd_dir: %s\n' "$steamcmd_dir"
  printf 'install_steamcmd: %s\n' "$([[ "$install_steamcmd" -eq 1 ]] && printf yes || printf no)"
  printf 'steam_user: %s\n' "$([[ -n "$steam_user" ]] && printf '%s' "$steam_user" || printf anonymous)"
  printf 'probe_after_download: %s\n' "$([[ "$probe_after_download" -eq 1 ]] && printf yes || printf no)"
  printf 'matrix_report_dir: %s\n' "$matrix_report_dir"
  printf 'commands: %s\n' "$commands_file"
  printf 'item_ids:'
  for item_id in "${unique_ids[@]}"; do
    printf ' %s' "$item_id"
  done
  printf '\n'
} >"$summary"

if [[ "$dry_run" -eq 1 ]]; then
  printf 'DRY-RUN: SteamCMD command:\n'
  write_quoted_command steamcmd_args
  if [[ "$probe_after_download" -eq 1 ]]; then
    printf 'DRY-RUN: matrix command:\n'
    write_quoted_command matrix_command
  fi
  printf 'summary: %s\n' "$summary"
  exit 0
fi

"${steamcmd_args[@]}"

downloaded_count=0
for item_id in "${unique_ids[@]}"; do
  item_dir="$content_dir/$item_id"
  if [[ -d "$item_dir" ]]; then
    downloaded_count=$((downloaded_count + 1))
  else
    printf 'WARN: expected downloaded item directory is missing: %s\n' "$item_dir" >&2
  fi
done
{
  printf 'downloaded_item_dirs: %s\n' "$downloaded_count"
} >>"$summary"

if [[ "$probe_after_download" -eq 1 ]]; then
  "${matrix_command[@]}"
fi

printf 'PASS: Wallpaper Engine Workshop download step completed\n'
printf 'summary: %s\n' "$summary"
