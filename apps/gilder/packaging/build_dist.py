#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
"""Build a Gilder distribution tarball without shell scripts.

Invoke with:
  uv run python packaging/build_dist.py
"""

from __future__ import annotations

import argparse
import os
import platform
import re
import shutil
import subprocess
import tarfile
from pathlib import Path


BINARIES = ["gilderd", "gilderctl", "gilder-convert", "gilder-native-vulkan"]
PYTHON_HELPERS = [
    "scripts/ffmpeg_vulkan_hwdecode_matrix.py",
    "scripts/performance_snapshot.py",
    "scripts/wallpaper_engine_workshop_download.py",
]


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    os.chdir(repo_root)

    cargo_profile_args, target_profile_dir = cargo_profile(args.profile)
    if args.build:
        subprocess.run(
            ["cargo", "build", *cargo_profile_args, "--features", args.features],
            check=True,
        )

    version = cargo_version(Path("Cargo.toml"))
    system = platform.system().lower()
    arch = platform.machine()
    package_name = f"gilder-{version}-{system}-{arch}"
    dest_dir = Path(args.dest)
    stage_dir = dest_dir / package_name
    archive_path = dest_dir / f"{package_name}.tar.gz"

    if stage_dir.exists():
        shutil.rmtree(stage_dir)
    for directory in [
        "bin",
        "share/man/man1",
        "share/bash-completion/completions",
        "share/zsh/site-functions",
        "lib/systemd/user",
        "share/doc/gilder/scripts",
    ]:
        (stage_dir / directory).mkdir(parents=True, exist_ok=True)

    for binary in BINARIES:
        source = Path("target") / target_profile_dir / binary
        if not source.exists():
            raise SystemExit(f"missing built binary: {source}")
        install(source, stage_dir / "bin" / binary, executable=True)

    copy_glob("docs/man/*.1", stage_dir / "share/man/man1")
    copy_glob("completions/bash/*", stage_dir / "share/bash-completion/completions")
    copy_glob("completions/zsh/*", stage_dir / "share/zsh/site-functions")
    install(Path("packaging/systemd/gilder.service"), stage_dir / "lib/systemd/user/gilder.service")
    for doc in [
        "README.zh-CN.md",
        "docs/packaging.md",
        "docs/gilder-scene-engine-architecture.md",
        "docs/native-vulkan-video-ffmpeg-mainline.md",
    ]:
        install(Path(doc), stage_dir / "share/doc/gilder" / Path(doc).name)
    for helper in PYTHON_HELPERS:
        install(Path(helper), stage_dir / "share/doc/gilder/scripts" / Path(helper).name)

    manifest = stage_dir / "MANIFEST.txt"
    manifest.write_text(
        "\n".join(
            [
                "name: gilder",
                f"version: {version}",
                f"profile: {args.profile}",
                f"features: {args.features}",
                f"system: {system}",
                f"arch: {arch}",
                "script_policy: python-only, run with uv run python",
            ]
        )
        + "\n"
    )

    dest_dir.mkdir(parents=True, exist_ok=True)
    if archive_path.exists():
        archive_path.unlink()
    with tarfile.open(archive_path, "w:gz") as tar:
        tar.add(stage_dir, arcname=package_name)
    print(f"staged {stage_dir}")
    print(f"archive {archive_path}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="生成 Gilder 发行 tarball。")
    parser.add_argument("--dest", default="dist")
    parser.add_argument("--profile", default="release")
    parser.add_argument(
        "--features", default=os.environ.get("GILDER_DIST_FEATURES", "native-vulkan-video")
    )
    parser.add_argument("--no-build", dest="build", action="store_false")
    parser.set_defaults(build=True)
    return parser.parse_args()


def cargo_profile(profile: str) -> tuple[list[str], str]:
    if profile == "release":
        return ["--release"], "release"
    if profile == "debug":
        return [], "debug"
    return ["--profile", profile], profile


def cargo_version(path: Path) -> str:
    match = re.search(r'^version\s*=\s*"([^"]+)"', path.read_text(), re.MULTILINE)
    if not match:
        raise SystemExit("Cargo.toml missing package version")
    return match.group(1)


def install(source: Path, dest: Path, executable: bool = False) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest)
    if executable:
        dest.chmod(0o755)


def copy_glob(pattern: str, dest_dir: Path) -> None:
    for path in sorted(Path().glob(pattern)):
        if path.is_file():
            install(path, dest_dir / path.name)


if __name__ == "__main__":
    raise SystemExit(main())
