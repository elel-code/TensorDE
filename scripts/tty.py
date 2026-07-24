#!/usr/bin/env -S uv run --script
"""Launch Tensor as a direct session from a Linux virtual terminal."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
from datetime import datetime


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOG = ROOT / "artifacts" / "logs" / "tensor-tty.log"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, help="KDL configuration path")
    parser.add_argument(
        "--log",
        type=Path,
        default=DEFAULT_LOG,
        help="append compositor output to this file (default: artifacts/logs/tensor-tty.log)",
    )
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


def log_path_for(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def launch(command: list[str], environment: dict[str, str], log_path: Path) -> int:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("ab", buffering=0) as log:
        started = datetime.now().astimezone().isoformat(timespec="seconds")
        header = f"\n=== Tensor TTY run {started} ===\n$ {' '.join(command)}\n"
        log.write(header.encode())
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        assert process.stdout is not None
        while True:
            try:
                chunk = os.read(process.stdout.fileno(), 64 * 1024)
            except KeyboardInterrupt:
                # The terminal also delivers SIGINT to the compositor's process
                # group. Keep draining while it shuts down so the final
                # diagnostics are persisted instead of risking a full pipe.
                continue
            if not chunk:
                break
            log.write(chunk)
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
        return process.wait()


def main() -> int:
    args = parse_args()
    if not args.check and not args.dry_run and virtual_terminal() is None:
        raise SystemExit(
            "Tensor's DRM/KMS session launcher must run from /dev/ttyN; "
            "switch to a virtual terminal first"
        )

    command = command_for(args)
    environment = environment_for(args)
    log_path = log_path_for(args.log)
    if args.dry_run:
        print(f"cwd: {ROOT}")
        print("command:", " ".join(command))
        print(f"log: {log_path}")
        for name in ("RUST_LOG", "TENSOR_RENDER_DEVICE", "TENSOR_XWAYLAND"):
            if name in environment:
                print(f"{name}={environment[name]}")
        return 0

    print(f"Tensor log: {log_path}")
    return launch(command, environment, log_path)


if __name__ == "__main__":
    raise SystemExit(main())
