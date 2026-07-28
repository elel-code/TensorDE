#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
"""Download Wallpaper Engine Workshop items with SteamCMD.

Invoke with:
  uv run python scripts/wallpaper_engine_workshop_download.py --item-id <id>
"""

from __future__ import annotations

import argparse
import os
import shlex
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request
from pathlib import Path

from workspace_paths import ARTIFACTS_ROOT, WORKSPACE_ROOT

STEAMCMD_URL = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz"


def main() -> int:
    args, matrix_args = parse_args()
    repo_root = WORKSPACE_ROOT
    os.chdir(repo_root)

    item_ids = unique_item_ids(args)
    if not item_ids and not args.install_steamcmd_only:
        print("FAIL: 至少传入一个 --item-id 或 --item-list", file=sys.stderr)
        return 2

    steamcmd = resolve_steamcmd(args, repo_root)
    if args.install_steamcmd_only:
        if args.dry_run:
            print(f"DRY-RUN: install SteamCMD into {args.steamcmd_dir}")
            return 0
        install_steamcmd(Path(args.steamcmd_dir))
        print(f"steamcmd: {Path(args.steamcmd_dir) / 'steamcmd.sh'}")
        return 0

    timestamp = f"{time.strftime('%Y%m%d-%H%M%S')}-{os.getpid()}"
    summary_dir = ARTIFACTS_ROOT / "wallpaper-engine-workshop/reports" / timestamp
    summary_dir.mkdir(parents=True, exist_ok=True)
    summary = summary_dir / "summary.txt"
    commands = summary_dir / "commands.txt"

    # SteamCMD resolves a relative force_install_dir against its own executable
    # directory, not necessarily the caller's cwd. Keep the workshop cache rooted
    # in the repository regardless of where the SteamCMD wrapper changes directory.
    download_root = Path(args.download_root).expanduser().resolve()
    content_dir = download_root / "steamapps/workshop/content" / str(args.appid)
    steam_user = "" if args.anonymous else args.steam_user
    steamcmd_args = [steamcmd, "+force_install_dir", str(download_root)]
    steamcmd_args.extend(["+login", steam_user or "anonymous"])
    for item_id in item_ids:
        steamcmd_args.extend(["+workshop_download_item", str(args.appid), item_id])
    steamcmd_args.append("+quit")

    matrix_report_dir = Path(args.matrix_report_dir) if args.matrix_report_dir else (
        ARTIFACTS_ROOT / "video-ffmpeg-vulkan-matrix" / f"we-{timestamp}"
    )
    matrix_command = [
        "uv",
        "run",
        "python",
        "scripts/gilder/ffmpeg_vulkan_hwdecode_matrix.py",
        "--artifact-prefix",
        "gilder-ffmpeg-vulkan-workshop",
        "--work-dir",
        str(matrix_report_dir),
        "--label",
        "workshop-download",
        "--duration",
        "10",
        "--target-fps",
        "source",
        "--source-dir",
        str(content_dir),
        *matrix_args,
    ]

    commands.write_text(
        shell_line(steamcmd_args)
        + ("\n" + shell_line(matrix_command) if args.probe_after_download else "")
        + "\n"
    )
    summary.write_text(
        "\n".join(
            [
                f"appid: {args.appid}",
                f"item_count: {len(item_ids)}",
                f"download_root: {download_root}",
                f"content_dir: {content_dir}",
                f"steamcmd: {steamcmd}",
                f"steam_user: {steam_user or 'anonymous'}",
                f"probe_after_download: {args.probe_after_download}",
                f"matrix_report_dir: {matrix_report_dir}",
                f"commands: {commands}",
                "item_ids: " + " ".join(item_ids),
            ]
        )
        + "\n"
    )

    if args.dry_run:
        print("DRY-RUN: SteamCMD command:")
        print(shell_line(steamcmd_args))
        if args.probe_after_download:
            print("DRY-RUN: matrix command:")
            print(shell_line(matrix_command))
        print(f"summary: {summary}")
        return 0

    ensure_steamcmd_available(steamcmd, args)
    subprocess.run(steamcmd_args, check=True)
    missing = [content_dir / item_id for item_id in item_ids if not (content_dir / item_id).is_dir()]
    if missing:
        for path in missing:
            print(f"FAIL: Workshop item directory missing: {path}", file=sys.stderr)
        return 1
    if args.probe_after_download:
        subprocess.run(matrix_command, check=True)
    print(f"summary: {summary}")
    return 0


def parse_args() -> tuple[argparse.Namespace, list[str]]:
    parser = argparse.ArgumentParser(
        description="用 SteamCMD 下载 Wallpaper Engine Workshop 项，并可选运行 FFmpeg Vulkan 矩阵。"
    )
    parser.add_argument("--item-id", action="append", default=[])
    parser.add_argument("--item-list", default="")
    parser.add_argument("--appid", type=int, default=431960)
    parser.add_argument(
        "--download-root",
        default=str(ARTIFACTS_ROOT / "wallpaper-engine-workshop/steamcmd-root"),
    )
    parser.add_argument("--steamcmd", default=os.environ.get("STEAMCMD", ""))
    parser.add_argument("--steamcmd-dir", default=str(ARTIFACTS_ROOT / "tools/steamcmd"))
    parser.add_argument("--install-steamcmd", action="store_true")
    parser.add_argument("--install-steamcmd-only", action="store_true")
    parser.add_argument("--anonymous", action="store_true")
    parser.add_argument("--steam-user", default=os.environ.get("GILDER_STEAM_USER", ""))
    parser.add_argument("--probe-after-download", "--run-matrix", action="store_true")
    parser.add_argument("--matrix-report-dir", default="")
    parser.add_argument("--dry-run", action="store_true")
    args, tail = parser.parse_known_args()
    if tail and tail[0] == "--":
        tail = tail[1:]
    return args, tail


def unique_item_ids(args: argparse.Namespace) -> list[str]:
    values = list(args.item_id)
    if args.item_list:
        for line in Path(args.item_list).read_text().splitlines():
            value = line.split("#", 1)[0].strip()
            if value:
                values.append(value)
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        if not value.isdigit():
            raise SystemExit(f"FAIL: Workshop item id must be numeric: {value}")
        if value not in seen:
            seen.add(value)
            result.append(value)
    return result


def resolve_steamcmd(args: argparse.Namespace, repo_root: Path) -> str:
    if args.steamcmd:
        return args.steamcmd
    local = repo_root / args.steamcmd_dir / "steamcmd.sh"
    if local.exists() or args.install_steamcmd or args.install_steamcmd_only:
        return str(local)
    return "steamcmd"


def ensure_steamcmd_available(steamcmd: str, args: argparse.Namespace) -> None:
    if Path(steamcmd).exists() or shutil.which(steamcmd):
        return
    if args.install_steamcmd:
        install_steamcmd(Path(args.steamcmd_dir))
        return
    raise SystemExit(
        f"FAIL: missing SteamCMD executable: {steamcmd}; pass --steamcmd or --install-steamcmd"
    )


def install_steamcmd(install_dir: Path) -> None:
    install_dir.mkdir(parents=True, exist_ok=True)
    archive = install_dir / "steamcmd_linux.tar.gz"
    with urllib.request.urlopen(STEAMCMD_URL, timeout=60) as response:
        archive.write_bytes(response.read())
    with tarfile.open(archive, "r:gz") as tar:
        tar.extractall(install_dir)
    steamcmd = install_dir / "steamcmd.sh"
    if not steamcmd.exists():
        raise SystemExit(f"FAIL: SteamCMD install did not produce {steamcmd}")


def shell_line(command: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


if __name__ == "__main__":
    raise SystemExit(main())
