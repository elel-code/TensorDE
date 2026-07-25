#!/usr/bin/env -S uv run --script
"""Launch Tensor as a direct session from a Linux virtual terminal."""

from __future__ import annotations

import argparse
from datetime import datetime
import os
from pathlib import Path
import secrets
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
KILL_GRACE_SECONDS = 2.0
EVENT_LOOP_READY_MARKER = b"entering compositor event loop"


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
            "launch Tensor's GBM linux-dmabuf client after Tensor enters its event loop; "
            "it requires real buffer import, KMS presentation, and release before succeeding"
        ),
    )
    parser.add_argument(
        "--ghostty",
        action="store_true",
        help=(
            "start a fresh Ghostty after Tensor enters its event loop; it uses Ghostty's "
            "normal backend selection with Tensor's session Wayland endpoint"
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
    # A graphical VT is not a usable live log console. Keep the default quiet
    # enough for interactive smoke runs; callers can still opt into a focused
    # `RUST_LOG` filter and inspect the complete file after the session exits.
    environment.setdefault("RUST_LOG", "tensor_compositor=info")
    # A TTY smoke run must not collide with a suspended desktop compositor or
    # a previous interrupted smoke run that used the configured IPC endpoint.
    # Keep this separate from the `tensor-N` Wayland sockets that the launcher
    # discovers for test clients below.
    environment["TENSOR_IPC_SOCKET"] = str(tty_ipc_socket_path())
    if args.render_device is not None:
        environment["TENSOR_RENDER_DEVICE"] = str(args.render_device)
    if args.no_xwayland:
        environment["TENSOR_XWAYLAND"] = "off"
    return environment


def tty_ipc_socket_path() -> Path:
    """Return a unique, short IPC path without perturbing Wayland discovery.

    IPC deliberately refuses to unlink an existing configured path: it might
    belong to a live compositor. TTY runs instead get a private endpoint. The
    leading dot also prevents ``tensor_sockets()`` from mistaking this control
    socket for Tensor's newly-created ``tensor-N`` Wayland socket.
    """
    runtime_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    for _ in range(16):
        name = f".tensor-tty-ipc-{os.getpid()}-{secrets.token_hex(6)}.sock"
        candidate = runtime_dir / name
        # sockaddr_un paths have a small platform-dependent limit. Falling
        # back to /tmp preserves the isolation guarantee for unusual runtime
        # directory layouts rather than letting bind fail later and opaquely.
        if len(os.fsencode(candidate)) >= 100:
            candidate = Path("/tmp") / name
        if not candidate.exists():
            return candidate
    raise RuntimeError("could not allocate an isolated Tensor TTY IPC path")


def log_path_for(path: Path) -> Path:
    return path if path.is_absolute() else ROOT / path


def launcher_log_path_for(tensor_log_path: Path) -> Path:
    """Keep launcher/client diagnostics separate from Tensor-owned tracing."""
    if tensor_log_path.suffix:
        name = f"{tensor_log_path.stem}.launcher{tensor_log_path.suffix}"
    else:
        name = f"{tensor_log_path.name}.launcher.log"
    return tensor_log_path.with_name(name)


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


def tensor_sockets(runtime_dir: Path) -> dict[Path, tuple[int, int, int]]:
    """Return live Tensor sockets keyed by their filesystem identity.

    A previous compositor can leave ``tensor-0`` behind until teardown races
    with the next launch.  The new compositor may legitimately reuse that
    pathname, so callers must distinguish the replacement inode from the old
    socket instead of comparing paths alone.
    """
    try:
        entries = runtime_dir.iterdir()
    except OSError:
        return {}
    sockets = {}
    for entry in entries:
        if not entry.name.startswith("tensor-"):
            continue
        try:
            metadata = entry.stat()
            if stat.S_ISSOCK(metadata.st_mode):
                sockets[entry] = (metadata.st_dev, metadata.st_ino, metadata.st_ctime_ns)
        except OSError:
            continue
    return sockets


def new_tensor_socket(
    runtime_dir: Path, known_sockets: dict[Path, tuple[int, int, int]]
) -> Path | None:
    current = tensor_sockets(runtime_dir)
    return next(
        (
            path
            for path in sorted(current)
            if known_sockets.get(path) != current[path]
        ),
        None,
    )


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
    # This only makes the command a new process. It does not choose a GTK/GDK
    # backend: Ghostty still performs its ordinary Wayland-vs-X11 selection.
    # Without it, a Ghostty already serving the suspended host desktop can
    # accept this request over D-Bus and leave Tensor with no client at all.
    return ["ghostty", "--gtk-single-instance=false"]


def session_client_environment(
    environment: dict[str, str], socket: Path
) -> dict[str, str]:
    """Recreate Tensor's published session values for one external test client.

    ``tty.py`` is the parent of Tensor, so it cannot inherit the environment
    that Tensor publishes to its own autostart children.  The new Wayland
    socket is therefore supplied explicitly.  A stale DISPLAY from the
    suspended desktop is removed just as ProcessLauncher removes the managed
    session values before it installs Tensor's values; this is not a request
    for a particular Ghostty/GDK backend.
    """
    client_environment = environment.copy()
    client_environment["WAYLAND_DISPLAY"] = socket.name
    client_environment["XDG_CURRENT_DESKTOP"] = "tensor"
    client_environment["XDG_SESSION_TYPE"] = "wayland"
    client_environment.pop("DISPLAY", None)
    return client_environment


def write_launcher_log(log: BinaryIO, output_lock: threading.Lock, chunk: bytes) -> None:
    with output_lock:
        log.write(chunk)


def note(log: BinaryIO, output_lock: threading.Lock, message: str) -> None:
    timestamp = datetime.now().astimezone().isoformat(timespec="seconds")
    write_launcher_log(log, output_lock, f"[tensor-tty {timestamp}] {message}\n".encode())


def send_signal(process: subprocess.Popen[bytes] | None, signal_number: int) -> bool:
    """Best-effort signal delivery without making shutdown depend on logging."""
    if process is None or process.poll() is not None:
        return False
    try:
        process.send_signal(signal_number)
    except ProcessLookupError:
        return False
    return True


def wait_for_exit(process: subprocess.Popen[bytes], timeout: float) -> int | None:
    """Reap a child only for a bounded period.

    A wedged graphics driver can leave a process in uninterruptible kernel
    sleep, including after SIGKILL. A TTY recovery tool must report that state
    rather than turn an already unusable virtual terminal into an infinite
    wait.
    """
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return None


def launch(
    command: list[str],
    environment: dict[str, str],
    tensor_log_path: Path,
    duration: float | None,
    dmabuf_smoke: bool,
    ghostty: bool,
) -> int:
    launcher_log_path = launcher_log_path_for(tensor_log_path)
    launcher_log_path.parent.mkdir(parents=True, exist_ok=True)
    completed = threading.Event()
    shutdown_requested = threading.Event()
    force_kill_sent = threading.Event()
    shutdown_unresponsive = threading.Event()
    output_lock = threading.Lock()
    runtime_dir = runtime_dir_for_client() if dmabuf_smoke or ghostty else None
    known_sockets = tensor_sockets(runtime_dir) if runtime_dir is not None else {}
    try:
        tensor_log_offset = tensor_log_path.stat().st_size
    except FileNotFoundError:
        tensor_log_offset = 0

    with launcher_log_path.open("ab", buffering=0) as log:
        started = datetime.now().astimezone().isoformat(timespec="seconds")
        header = (
            f"\n=== Tensor TTY run {started} ===\n"
            f"$ {shlex.join(command)}\n"
            f"clients: dmabuf-smoke={dmabuf_smoke} ghostty={ghostty} "
            f"duration={duration if duration is not None else 'forever'}\n"
            f"ipc-socket: {environment['TENSOR_IPC_SOCKET']}\n"
            f"compositor-log: {tensor_log_path}\n"
        )
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
        tensor_log_tail = b""
        event_loop_ready = False

        def observe_tensor_log() -> None:
            nonlocal event_loop_ready, tensor_log_offset, tensor_log_tail
            if event_loop_ready:
                return
            try:
                with tensor_log_path.open("rb") as tensor_log:
                    tensor_log.seek(0, os.SEEK_END)
                    if tensor_log.tell() < tensor_log_offset:
                        tensor_log_offset = 0
                    tensor_log.seek(tensor_log_offset)
                    chunk = tensor_log.read(64 * 1024)
                    tensor_log_offset = tensor_log.tell()
            except FileNotFoundError:
                return
            if not chunk:
                return
            combined = tensor_log_tail + chunk
            if EVENT_LOOP_READY_MARKER in combined:
                event_loop_ready = True
                note(
                    log,
                    output_lock,
                    "Tensor entered its compositor event loop; client launch gate opened",
                )
                return
            # A direct file append can split a tracing line between polls.
            # Preserve only the suffix that could still become the marker.
            keep = max(0, len(EVENT_LOOP_READY_MARKER) - 1)
            tensor_log_tail = combined[-keep:] if keep else b""

        def start_smoke_client() -> None:
            nonlocal smoke_process
            if (
                not dmabuf_smoke
                or smoke_process is not None
                or runtime_dir is None
                or not event_loop_ready
                or shutdown_requested.is_set()
            ):
                return
            socket = new_tensor_socket(runtime_dir, known_sockets)
            if socket is None:
                return
            client_command = smoke_command(socket, duration)
            note(
                log,
                output_lock,
                f"Tensor is ready on Wayland socket {socket.name}; "
                f"starting dma-buf smoke client: "
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
                or not event_loop_ready
                or shutdown_requested.is_set()
            ):
                return
            socket = new_tensor_socket(runtime_dir, known_sockets)
            if socket is None:
                return
            client_command = ghostty_command()
            note(
                log,
                output_lock,
                f"Tensor is ready on Wayland socket {socket.name}; starting Ghostty "
                f"with its normal backend selection: "
                f"{shlex.join(client_command)}",
            )
            try:
                ghostty_process = subprocess.Popen(
                    client_command,
                    cwd=ROOT,
                    env=session_client_environment(environment, socket),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                )
            except OSError as error:
                ghostty_status = 127
                ghostty_failed = True
                note(log, output_lock, f"failed to start Ghostty: {error}")
                request_shutdown("Ghostty could not start")
                return
            assert ghostty_process.stdout is not None
            selector.register(ghostty_process.stdout, selectors.EVENT_READ, "ghostty")

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
            if not send_signal(smoke_process, signal.SIGTERM):
                return
            note(log, output_lock, f"{reason}; sent SIGTERM to dma-buf smoke client")

        def stop_ghostty(reason: str) -> None:
            if not send_signal(ghostty_process, signal.SIGTERM):
                return
            note(log, output_lock, f"{reason}; sent SIGTERM to Ghostty")

        def request_shutdown(reason: str) -> None:
            if completed.is_set() or shutdown_requested.is_set():
                return
            shutdown_requested.set()
            smoke_stopped = send_signal(smoke_process, signal.SIGTERM)
            ghostty_stopped = send_signal(ghostty_process, signal.SIGTERM)
            tensor_stopped = send_signal(process, signal.SIGTERM)

            def force_shutdown() -> None:
                if completed.wait(SHUTDOWN_GRACE_SECONDS) or process.poll() is not None:
                    return
                if not send_signal(process, signal.SIGKILL):
                    return
                force_kill_sent.set()
                send_signal(smoke_process, signal.SIGKILL)
                send_signal(ghostty_process, signal.SIGKILL)
                if completed.wait(KILL_GRACE_SECONDS) or process.poll() is not None:
                    note(log, output_lock, "Tensor did not exit after SIGTERM; sent SIGKILL")
                    return
                shutdown_unresponsive.set()
                note(
                    log,
                    output_lock,
                    "Tensor remained alive after SIGKILL; launcher will stop waiting so the VT can recover",
                )

            if tensor_stopped:
                threading.Thread(target=force_shutdown, daemon=True).start()
            # Every control action has already happened before any file I/O.
            # This remains true even if a storage failure blocks logging.
            if smoke_stopped:
                note(log, output_lock, f"{reason}; sent SIGTERM to dma-buf smoke client")
            if ghostty_stopped:
                note(log, output_lock, f"{reason}; sent SIGTERM to Ghostty")
            if tensor_stopped:
                note(log, output_lock, f"{reason}; sent SIGTERM to Tensor")

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
            while process.poll() is None:
                if shutdown_unresponsive.is_set():
                    break
                try:
                    observe_tensor_log()
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
                        write_launcher_log(log, output_lock, chunk)
                        continue
                    selector.unregister(stream)
                    if key.data == "tensor":
                        continue
                    if key.data == "dmabuf-smoke":
                        assert smoke_process is not None
                        smoke_status = wait_for_exit(smoke_process, KILL_GRACE_SECONDS)
                        if smoke_status is None:
                            smoke_status = 1
                            note(
                                log,
                                output_lock,
                                "dma-buf smoke client closed output but did not exit promptly",
                            )
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
                    if key.data == "ghostty":
                        continue
                    raise AssertionError(f"unexpected TTY client stream {key.data!r}")
            if shutdown_unresponsive.is_set():
                return 1
            observe_tensor_log()
            if dmabuf_smoke and smoke_process is None:
                smoke_status = 1
                note(
                    log,
                    output_lock,
                    "Tensor exited before opening the dma-buf smoke launch gate",
                )
            if ghostty and ghostty_process is None:
                ghostty_failed = True
                note(
                    log,
                    output_lock,
                    "Tensor exited before opening the Ghostty launch gate",
                )
            compositor_status = process.poll()
            if compositor_status is None:
                compositor_status = wait_for_exit(process, SHUTDOWN_GRACE_SECONDS)
            if compositor_status is None:
                note(
                    log,
                    output_lock,
                    "Tensor did not exit within the bounded launcher wait",
                )
                return 1
            if dmabuf_smoke:
                if smoke_process is None:
                    return 1
                if smoke_status is None:
                    smoke_status = wait_for_exit(smoke_process, SHUTDOWN_GRACE_SECONDS)
                if smoke_status is None:
                    return 1
                if smoke_status != 0:
                    return smoke_status
            if ghostty and (ghostty_process is None or ghostty_failed):
                return ghostty_status if ghostty_status is not None else 1
            return compositor_status
        except KeyboardInterrupt:
            request_shutdown("interrupt received")
            compositor_status = wait_for_exit(
                process, SHUTDOWN_GRACE_SECONDS + KILL_GRACE_SECONDS
            )
            return compositor_status if compositor_status is not None else 1
        finally:
            selector.close()
            if process.poll() is None:
                if not force_kill_sent.is_set():
                    request_shutdown("TTY launcher is stopping")
                if not shutdown_unresponsive.is_set():
                    if wait_for_exit(process, SHUTDOWN_GRACE_SECONDS) is None:
                        if send_signal(process, signal.SIGKILL):
                            force_kill_sent.set()
                        if wait_for_exit(process, KILL_GRACE_SECONDS) is None:
                            shutdown_unresponsive.set()
                if shutdown_unresponsive.is_set():
                    note(
                        log,
                        output_lock,
                        "Tensor could not be reaped after SIGKILL; inspect its PID and GPU kernel logs",
                    )
            if smoke_process is not None and smoke_process.poll() is None:
                stop_smoke_client("TTY launcher is stopping")
                if wait_for_exit(smoke_process, SHUTDOWN_GRACE_SECONDS) is None:
                    if send_signal(smoke_process, signal.SIGKILL):
                        note(log, output_lock, "sent SIGKILL to dma-buf smoke client")
                    if wait_for_exit(smoke_process, KILL_GRACE_SECONDS) is None:
                        note(log, output_lock, "dma-buf smoke client remained alive after SIGKILL")
            if ghostty_process is not None and ghostty_process.poll() is None:
                stop_ghostty("TTY launcher is stopping")
                if wait_for_exit(ghostty_process, SHUTDOWN_GRACE_SECONDS) is None:
                    if send_signal(ghostty_process, signal.SIGKILL):
                        note(log, output_lock, "sent SIGKILL to Ghostty")
                    if wait_for_exit(ghostty_process, KILL_GRACE_SECONDS) is None:
                        note(log, output_lock, "Ghostty remained alive after SIGKILL")
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

    log_path = log_path_for(args.log)
    command = command_for(args)
    environment = environment_for(args)
    # Tensor itself owns its tracing file. The launcher only watches appended
    # readiness records and keeps its small control/client diagnostic log
    # separate, so compositor logging has no parent-pipe backpressure path.
    environment["TENSOR_LOG_FILE"] = str(log_path)
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
                "health gate: wait for Tensor to enter its event loop, then require native "
                "linux-dmabuf import, KMS presentation, and wl_buffer release"
            )
        if args.ghostty:
            print(
                "client: wait for Tensor to enter its event loop, then start a fresh Ghostty "
                "with normal backend selection on Tensor's session endpoint"
            )
        for name in (
            "RUST_LOG",
            "TENSOR_IPC_SOCKET",
            "TENSOR_LOG_FILE",
            "TENSOR_RENDER_DEVICE",
            "TENSOR_XWAYLAND",
        ):
            if name in environment:
                print(f"{name}={environment[name]}")
        return 0

    build_status = build_binaries(args.dmabuf_smoke)
    if build_status != 0:
        return build_status
    print(f"Tensor log: {log_path}")
    print(f"Launcher log: {launcher_log_path_for(log_path)}")
    return launch(command, environment, log_path, duration, args.dmabuf_smoke, args.ghostty)


if __name__ == "__main__":
    raise SystemExit(main())
