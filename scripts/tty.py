#!/usr/bin/env -S uv run --script
"""Launch Tensor as a direct session from a Linux virtual terminal."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parent.parent


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, help="KDL configuration path")
    parser.add_argument(
        "--render-device",
        type=Path,
        help="pin Vulkan and Smithay to this DRM primary or render node",
    )
    parser.add_argument(
        "--no-xwayland",
        action="store_true",
        help="disable rootless XWayland for an isolated native-Wayland smoke test",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="run the safe startup capability check instead of entering the TTY event loop",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved command without launching it",
    )
    return parser.parse_args()


def virtual_terminal() -> str | None:
    try:
        terminal = os.ttyname(sys.stdin.fileno())
    except OSError:
        return None
    return terminal if terminal.startswith("/dev/tty") else None


def command_for(args: argparse.Namespace) -> list[str]:
    command = ["cargo", "run", "--bin", "tensor-compositor", "--"]
    if args.config is not None:
        command.extend(["--config", str(args.config)])
    command.append("--check" if args.check else "--session")
    return command


def environment_for(args: argparse.Namespace) -> dict[str, str]:
    environment = os.environ.copy()
    environment.setdefault("RUST_LOG", "tensor_compositor=debug")
    if args.render_device is not None:
        environment["TENSOR_RENDER_DEVICE"] = str(args.render_device)
    if args.no_xwayland:
        environment["TENSOR_XWAYLAND"] = "off"
    return environment


def main() -> int:
    args = parse_args()
    if not args.check and not args.dry_run and virtual_terminal() is None:
        raise SystemExit(
            "Tensor's DRM/KMS session launcher must run from /dev/ttyN; "
            "switch to a virtual terminal first"
        )

    command = command_for(args)
    environment = environment_for(args)
    if args.dry_run:
        print(f"cwd: {ROOT}")
        print("command:", " ".join(command))
        for name in ("RUST_LOG", "TENSOR_RENDER_DEVICE", "TENSOR_XWAYLAND"):
            if name in environment:
                print(f"{name}={environment[name]}")
        return 0

    os.chdir(ROOT)
    os.execvpe(command[0], command, environment)


if __name__ == "__main__":
    raise SystemExit(main())
