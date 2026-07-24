#!/usr/bin/env -S uv run --script
"""Launch Tensor as a direct session from a Linux virtual terminal."""

from __future__ import annotations

import argparse
from datetime import datetime
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import threading
from typing import BinaryIO


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOG = ROOT / "artifacts" / "logs" / "tensor-tty.log"
DEFAULT_SMOKE_DURATION_SECONDS = 20.0
SHUTDOWN_GRACE_SECONDS = 5.0


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
    lifetime = parser.add_mutually_exclusive_group()
    lifetime.add_argument(
        "--duration",
        type=float,
        default=DEFAULT_SMOKE_DURATION_SECONDS,
        help=(
            "stop a hardware smoke test after this many seconds "
            f"(default: {DEFAULT_SMOKE_DURATION_SECONDS:g})"
        ),
    )
    lifetime.add_argument(
        "--forever",
        action="store_true",
        help="keep the compositor running until it is stopped manually",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved command without building or launching it",
    )
    args = parser.parse_args()
    if args.duration <= 0:
        parser.error("--duration must be greater than zero")
    return args


def virtual_terminal() -> str | None:
    try:
        terminal = os.ttyname(sys.stdin.fileno())
    except OSError:
        return None
    return terminal if terminal.startswith("/dev/tty") else None


def command_for(args: argparse.Namespace) -> list[str]:
    command = [str(ROOT / "target" / "debug" / "tensor-compositor")]
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


def smoke_duration_for(args: argparse.Namespace) -> float | None:
    if args.check or args.forever:
        return None
    return args.duration


def build_compositor() -> int:
    print("Building Tensor compositor...")
    return subprocess.run(
        ["cargo", "build", "--bin", "tensor-compositor"], cwd=ROOT, check=False
    ).returncode


def emit(log: BinaryIO, output_lock: threading.Lock, chunk: bytes) -> None:
    with output_lock:
        log.write(chunk)
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()


def note(log: BinaryIO, output_lock: threading.Lock, message: str) -> None:
    timestamp = datetime.now().astimezone().isoformat(timespec="seconds")
    emit(log, output_lock, f"[tensor-tty {timestamp}] {message}\n".encode())


def launch(
    command: list[str],
    environment: dict[str, str],
    log_path: Path,
    duration: float | None,
) -> int:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    completed = threading.Event()
    shutdown_requested = threading.Event()
    output_lock = threading.Lock()

    with log_path.open("ab", buffering=0) as log:
        started = datetime.now().astimezone().isoformat(timespec="seconds")
        header = f"\n=== Tensor TTY run {started} ===\n$ {shlex.join(command)}\n"
        log.write(header.encode())
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        assert process.stdout is not None

        def request_shutdown(reason: str) -> None:
            if completed.is_set() or shutdown_requested.is_set():
                return
            shutdown_requested.set()
            if process.poll() is not None:
                return
            note(log, output_lock, f"{reason}; sending SIGTERM to Tensor")
            try:
                process.send_signal(signal.SIGTERM)
            except ProcessLookupError:
                return

            def force_shutdown() -> None:
                if completed.wait(SHUTDOWN_GRACE_SECONDS) or process.poll() is not None:
                    return
                note(
                    log,
                    output_lock,
                    "Tensor did not exit after SIGTERM; sending SIGKILL",
                )
                try:
                    process.kill()
                except ProcessLookupError:
                    pass

            threading.Thread(target=force_shutdown, daemon=True).start()

        watchdog: threading.Thread | None = None
        if duration is not None:

            def stop_after_smoke_duration() -> None:
                if not completed.wait(duration):
                    request_shutdown(
                        f"bounded smoke duration ({duration:g} seconds) elapsed"
                    )

            watchdog = threading.Thread(target=stop_after_smoke_duration, daemon=True)
            watchdog.start()

        try:
            while True:
                try:
                    chunk = os.read(process.stdout.fileno(), 64 * 1024)
                except KeyboardInterrupt:
                    request_shutdown("interrupt received")
                    continue
                if not chunk:
                    if process.poll() is None:
                        request_shutdown("Tensor closed its output while still running")
                    break
                emit(log, output_lock, chunk)
            return process.wait()
        except KeyboardInterrupt:
            request_shutdown("interrupt received")
            return process.wait()
        finally:
            if process.poll() is None:
                request_shutdown("TTY launcher is stopping")
                try:
                    process.wait(timeout=SHUTDOWN_GRACE_SECONDS)
                except subprocess.TimeoutExpired:
                    note(log, output_lock, "forcing Tensor shutdown during launcher cleanup")
                    process.kill()
                    process.wait()
            completed.set()
            if watchdog is not None:
                watchdog.join()


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
    duration = smoke_duration_for(args)
    if args.dry_run:
        print(f"cwd: {ROOT}")
        print("command:", shlex.join(command))
        print(f"log: {log_path}")
        if args.check:
            print("mode: startup capability check")
        elif duration is None:
            print("mode: persistent session")
        else:
            print(f"mode: bounded {duration:g}-second hardware smoke test")
        for name in ("RUST_LOG", "TENSOR_RENDER_DEVICE", "TENSOR_XWAYLAND"):
            if name in environment:
                print(f"{name}={environment[name]}")
        return 0

    build_status = build_compositor()
    if build_status != 0:
        return build_status
    print(f"Tensor log: {log_path}")
    return launch(command, environment, log_path, duration)


if __name__ == "__main__":
    raise SystemExit(main())
