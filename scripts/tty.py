#!/usr/bin/env -S uv run --script
"""Launch Tensor as a direct session from a Linux virtual terminal."""

from __future__ import annotations

import argparse
from datetime import datetime
import os
from pathlib import Path
import selectors
import shlex
import signal
import stat
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
    parser.add_argument(
        "--dmabuf-smoke",
        action="store_true",
        help=(
            "launch Tensor's GBM linux-dmabuf client after the Wayland socket is ready; "
            "it requires real buffer import, KMS presentation, and release before succeeding"
        ),
    )
    parser.add_argument(
        "--ghostty",
        action="store_true",
        help=(
            "start a fresh native-Wayland Ghostty after Tensor publishes its socket; "
            "it forces GDK_BACKEND=wayland and clears DISPLAY"
        ),
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
    if args.check and (args.dmabuf_smoke or args.ghostty):
        parser.error("--dmabuf-smoke and --ghostty require a real TTY compositor session")
    if args.forever and args.dmabuf_smoke:
        parser.error("--dmabuf-smoke has its own bounded health loop and cannot use --forever")
    if args.dmabuf_smoke and args.ghostty:
        parser.error("run --dmabuf-smoke and --ghostty as separate focused tests")
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


def build_binaries(dmabuf_smoke: bool) -> int:
    command = ["cargo", "build", "--bin", "tensor-compositor"]
    if dmabuf_smoke:
        command.extend(["--bin", "tensor-dmabuf-smoke"])
    print("Building Tensor binaries...")
    return subprocess.run(
        command, cwd=ROOT, check=False
    ).returncode


def tensor_sockets(runtime_dir: Path) -> set[Path]:
    try:
        entries = runtime_dir.iterdir()
    except OSError:
        return set()
    sockets = set()
    for entry in entries:
        if not entry.name.startswith("tensor-"):
            continue
        try:
            if stat.S_ISSOCK(entry.stat().st_mode):
                sockets.add(entry)
        except OSError:
            continue
    return sockets


def runtime_dir_for_client() -> Path:
    value = os.environ.get("XDG_RUNTIME_DIR")
    if not value:
        raise SystemExit("TTY client tests require XDG_RUNTIME_DIR")
    return Path(value)


def smoke_command(socket: Path, duration: float | None) -> list[str]:
    command = [
        str(ROOT / "target" / "debug" / "tensor-dmabuf-smoke"),
        "--socket",
        socket.name,
    ]
    if duration is not None:
        timeout = max(1, int(duration - 2))
        command.extend(["--timeout", str(timeout)])
    return command


def ghostty_command() -> list[str]:
    # Avoid an existing Ghostty instance from handling this request through
    # the host desktop.  The process we launch must connect to Tensor itself.
    return ["ghostty", "--gtk-single-instance=false"]


def native_wayland_environment(
    environment: dict[str, str], socket: Path
) -> dict[str, str]:
    client_environment = environment.copy()
    client_environment["WAYLAND_DISPLAY"] = socket.name
    client_environment["XDG_CURRENT_DESKTOP"] = "tensor"
    client_environment["XDG_SESSION_TYPE"] = "wayland"
    client_environment["GDK_BACKEND"] = "wayland"
    client_environment.pop("DISPLAY", None)
    return client_environment


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
    dmabuf_smoke: bool,
    ghostty: bool,
) -> int:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    completed = threading.Event()
    shutdown_requested = threading.Event()
    output_lock = threading.Lock()
    runtime_dir = runtime_dir_for_client() if dmabuf_smoke or ghostty else None
    known_sockets = tensor_sockets(runtime_dir) if runtime_dir is not None else set()

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
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ, "tensor")
        smoke_process: subprocess.Popen[bytes] | None = None
        smoke_status: int | None = None
        ghostty_process: subprocess.Popen[bytes] | None = None
        ghostty_status: int | None = None
        ghostty_failed = False
        tensor_output_open = True

        def start_smoke_client() -> None:
            nonlocal smoke_process
            if smoke_process is not None or runtime_dir is None:
                return
            candidates = sorted(tensor_sockets(runtime_dir) - known_sockets)
            if not candidates:
                return
            client_command = smoke_command(candidates[0], duration)
            note(
                log,
                output_lock,
                f"Wayland socket {candidates[0].name} is ready; starting dma-buf smoke client: "
                f"{shlex.join(client_command)}",
            )
            smoke_process = subprocess.Popen(
                client_command,
                cwd=ROOT,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
            assert smoke_process.stdout is not None
            selector.register(smoke_process.stdout, selectors.EVENT_READ, "dmabuf-smoke")

        def start_ghostty() -> None:
            nonlocal ghostty_process, ghostty_status, ghostty_failed
            if (
                not ghostty
                or ghostty_process is not None
                or ghostty_status is not None
                or runtime_dir is None
            ):
                return
            candidates = sorted(tensor_sockets(runtime_dir) - known_sockets)
            if not candidates:
                return
            socket = candidates[0]
            client_command = ghostty_command()
            note(
                log,
                output_lock,
                f"Wayland socket {socket.name} is ready; starting native Ghostty: "
                f"{shlex.join(client_command)}",
            )
            try:
                ghostty_process = subprocess.Popen(
                    client_command,
                    cwd=ROOT,
                    env=native_wayland_environment(environment, socket),
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )
            except OSError as error:
                ghostty_status = 127
                ghostty_failed = True
                note(log, output_lock, f"failed to start Ghostty: {error}")
                request_shutdown("Ghostty could not start")
                return

        def observe_ghostty_exit() -> None:
            nonlocal ghostty_status, ghostty_failed
            if ghostty_process is None or ghostty_status is not None:
                return
            status = ghostty_process.poll()
            if status is None:
                return
            ghostty_status = status
            note(log, output_lock, f"native Ghostty exited with status {status}")
            if not shutdown_requested.is_set():
                ghostty_failed = status != 0
                request_shutdown("native Ghostty closed")

        def stop_smoke_client(reason: str) -> None:
            if smoke_process is None or smoke_process.poll() is not None:
                return
            note(log, output_lock, f"{reason}; sending SIGTERM to dma-buf smoke client")
            try:
                smoke_process.send_signal(signal.SIGTERM)
            except ProcessLookupError:
                return

        def stop_ghostty(reason: str) -> None:
            if ghostty_process is None or ghostty_process.poll() is not None:
                return
            note(log, output_lock, f"{reason}; sending SIGTERM to Ghostty")
            try:
                ghostty_process.send_signal(signal.SIGTERM)
            except ProcessLookupError:
                return

        def request_shutdown(reason: str) -> None:
            if completed.is_set() or shutdown_requested.is_set():
                return
            shutdown_requested.set()
            stop_smoke_client(reason)
            stop_ghostty(reason)
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
                if smoke_process is not None and smoke_process.poll() is None:
                    try:
                        smoke_process.kill()
                    except ProcessLookupError:
                        pass
                if ghostty_process is not None and ghostty_process.poll() is None:
                    try:
                        ghostty_process.kill()
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
            while tensor_output_open:
                try:
                    start_smoke_client()
                    start_ghostty()
                    observe_ghostty_exit()
                    events = selector.select(timeout=0.1)
                except KeyboardInterrupt:
                    request_shutdown("interrupt received")
                    continue
                for key, _ in events:
                    stream = key.fileobj
                    try:
                        chunk = os.read(stream.fileno(), 64 * 1024)
                    except BlockingIOError:
                        continue
                    if chunk:
                        emit(log, output_lock, chunk)
                        continue
                    selector.unregister(stream)
                    if key.data == "tensor":
                        tensor_output_open = False
                        if process.poll() is None:
                            request_shutdown("Tensor closed its output while still running")
                        continue
                    if key.data == "dmabuf-smoke":
                        assert smoke_process is not None
                        smoke_status = smoke_process.wait()
                        note(
                            log,
                            output_lock,
                            f"dma-buf smoke client exited with status {smoke_status}",
                        )
                        if smoke_status == 0:
                            request_shutdown("dma-buf smoke client completed successfully")
                        else:
                            request_shutdown("dma-buf smoke client failed")
                        continue
                    raise AssertionError(f"unexpected TTY client stream {key.data!r}")
                if not tensor_output_open and dmabuf_smoke and smoke_process is None:
                    smoke_status = 1
                    note(
                        log,
                        output_lock,
                        "Tensor exited before creating a new tensor-* Wayland socket for dma-buf smoke",
                    )
                if not tensor_output_open and ghostty and ghostty_process is None:
                    ghostty_failed = True
                    note(
                        log,
                        output_lock,
                        "Tensor exited before creating a new tensor-* Wayland socket for Ghostty",
                    )
            compositor_status = process.wait()
            if dmabuf_smoke:
                if smoke_process is None:
                    return 1
                if smoke_status is None:
                    smoke_status = smoke_process.wait()
                if smoke_status != 0:
                    return smoke_status
            if ghostty and (ghostty_process is None or ghostty_failed):
                return ghostty_status if ghostty_status is not None else 1
            return compositor_status
        except KeyboardInterrupt:
            request_shutdown("interrupt received")
            return process.wait()
        finally:
            selector.close()
            if process.poll() is None:
                request_shutdown("TTY launcher is stopping")
                try:
                    process.wait(timeout=SHUTDOWN_GRACE_SECONDS)
                except subprocess.TimeoutExpired:
                    note(log, output_lock, "forcing Tensor shutdown during launcher cleanup")
                    process.kill()
                    process.wait()
            if smoke_process is not None and smoke_process.poll() is None:
                stop_smoke_client("TTY launcher is stopping")
                try:
                    smoke_process.wait(timeout=SHUTDOWN_GRACE_SECONDS)
                except subprocess.TimeoutExpired:
                    note(log, output_lock, "forcing dma-buf smoke client shutdown")
                    smoke_process.kill()
                    smoke_process.wait()
            if ghostty_process is not None and ghostty_process.poll() is None:
                stop_ghostty("TTY launcher is stopping")
                try:
                    ghostty_process.wait(timeout=SHUTDOWN_GRACE_SECONDS)
                except subprocess.TimeoutExpired:
                    note(log, output_lock, "forcing Ghostty shutdown")
                    ghostty_process.kill()
                    ghostty_process.wait()
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
        if args.dmabuf_smoke:
            print(
                "health gate: wait for a new tensor-* socket, then require native "
                "linux-dmabuf import, KMS presentation, and wl_buffer release"
            )
        if args.ghostty:
            print(
                "client: wait for a new tensor-* socket, then start Ghostty with a forced "
                "native Wayland environment"
            )
        for name in ("RUST_LOG", "TENSOR_RENDER_DEVICE", "TENSOR_XWAYLAND"):
            if name in environment:
                print(f"{name}={environment[name]}")
        return 0

    build_status = build_binaries(args.dmabuf_smoke)
    if build_status != 0:
        return build_status
    print(f"Tensor log: {log_path}")
    return launch(command, environment, log_path, duration, args.dmabuf_smoke, args.ghostty)


if __name__ == "__main__":
    raise SystemExit(main())
