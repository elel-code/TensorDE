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
import subprocess
import sys
import threading

from tty_clients import (
    ClientArgvAction,
    InteractiveClient,
    client_from_argv,
    fcitx_launcher_command,
    session_client_environment,
)
from tty_support import (
    launcher_log_path_for,
    new_tensor_socket,
    note,
    runtime_dir_for_client,
    send_process_group_signal,
    send_signal,
    smoke_command,
    tensor_sockets,
    terminate_process_group,
    wait_for_exit,
    write_launcher_log,
)


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_LOG = ROOT / "artifacts" / "logs" / "tensor-tty.log"
DEFAULT_SMOKE_DURATION_SECONDS = 20.0
SHUTDOWN_GRACE_SECONDS = 5.0
KILL_GRACE_SECONDS = 2.0
EVENT_LOOP_READY_MARKER = b"entering compositor event loop"
INPUT_METHOD_REGISTERED_MARKER = b"input-method client registered"
INPUT_METHOD_KEYBOARD_GRAB_MARKER = b"input-method keyboard grab registered"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, help="TOML configuration path")
    parser.add_argument(
        "--log",
        type=Path,
        default=DEFAULT_LOG,
        help="append compositor output to this file (default: artifacts/logs/tensor-tty.log)",
    )
    parser.add_argument(
        "--render-device",
        type=Path,
        help="pin Vulkan and Tensor tty to this DRM primary or render node",
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
        "--client",
        dest="clients",
        action=ClientArgvAction,
        metavar="PROGRAM",
        help=(
            "start a native Wayland smoke client; repeat to start multiple clients"
        ),
    )
    parser.add_argument(
        "--client-arg",
        dest="clients",
        action=ClientArgvAction,
        metavar="ARG",
        help=(
            "append one direct argv item to the most recent --client PROGRAM "
            "(use --client-arg=ARG for an argument beginning with -)"
        ),
    )
    parser.add_argument(
        "--fcitx",
        action="store_true",
        help=(
            "ask the running Fcitx 5 service to attach a native Wayland input-method "
            "client to Tensor before launching the interactive client"
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
    interactive_client = bool(args.clients)
    if args.check and (args.dmabuf_smoke or interactive_client or args.fcitx):
        parser.error("TTY smoke clients require a real compositor session, not --check")
    if args.forever and args.dmabuf_smoke:
        parser.error("--dmabuf-smoke has its own bounded health loop and cannot use --forever")
    if args.dmabuf_smoke and interactive_client:
        parser.error("run --dmabuf-smoke and an interactive client as separate focused tests")
    if args.fcitx and not interactive_client:
        parser.error("--fcitx requires at least one --client PROGRAM")
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


def launch(
    command: list[str],
    environment: dict[str, str],
    tensor_log_path: Path,
    duration: float | None,
    dmabuf_smoke: bool,
    clients: list[InteractiveClient],
    fcitx: bool,
) -> int:
    launcher_log_path = launcher_log_path_for(tensor_log_path)
    launcher_log_path.parent.mkdir(parents=True, exist_ok=True)
    completed = threading.Event()
    shutdown_requested = threading.Event()
    force_kill_sent = threading.Event()
    shutdown_unresponsive = threading.Event()
    output_lock = threading.Lock()
    runtime_dir = runtime_dir_for_client() if dmabuf_smoke or clients else None
    known_sockets = tensor_sockets(runtime_dir) if runtime_dir is not None else {}
    try:
        tensor_log_offset = tensor_log_path.stat().st_size
    except FileNotFoundError:
        tensor_log_offset = 0

    with launcher_log_path.open("ab", buffering=0) as log:
        started = datetime.now().astimezone().isoformat(timespec="seconds")
        interactive_names = ", ".join(client.name for client in clients) or "none"
        header = (
            f"\n=== Tensor TTY run {started} ===\n"
            f"$ {shlex.join(command)}\n"
            f"clients: dmabuf-smoke={dmabuf_smoke} "
            f"interactive={interactive_names} fcitx={fcitx} "
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
        interactive_processes: list[subprocess.Popen[bytes] | None] = [None] * len(clients)
        interactive_statuses: list[int | None] = [None] * len(clients)
        interactive_failed = False
        interactive_failure_status: int | None = None
        fcitx_process: subprocess.Popen[bytes] | None = None
        fcitx_status: int | None = None
        fcitx_failed = False
        tensor_log_tail = b""
        event_loop_ready = False
        input_method_registered = False
        input_method_keyboard_grab = False

        def observe_tensor_log() -> None:
            nonlocal event_loop_ready, input_method_registered, input_method_keyboard_grab
            nonlocal tensor_log_offset, tensor_log_tail
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
            if not event_loop_ready and EVENT_LOOP_READY_MARKER in combined:
                event_loop_ready = True
                note(
                    log,
                    output_lock,
                    "Tensor entered its compositor event loop; client launch gate opened",
                )
            if INPUT_METHOD_REGISTERED_MARKER in combined:
                input_method_registered = True
            if INPUT_METHOD_KEYBOARD_GRAB_MARKER in combined:
                input_method_keyboard_grab = True
            # A direct file append can split a tracing line between polls.
            # Preserve only the suffix that could still become one of the
            # launch or input-method markers.
            keep = max(
                0,
                max(
                    len(EVENT_LOOP_READY_MARKER),
                    len(INPUT_METHOD_REGISTERED_MARKER),
                    len(INPUT_METHOD_KEYBOARD_GRAB_MARKER),
                )
                - 1,
            )
            tensor_log_tail = combined[-keep:] if keep else b""

        def ready_socket() -> Path | None:
            if runtime_dir is None or not event_loop_ready or shutdown_requested.is_set():
                return None
            return new_tensor_socket(runtime_dir, known_sockets)

        def start_smoke_client() -> None:
            nonlocal smoke_process
            if not dmabuf_smoke or smoke_process is not None:
                return
            socket = ready_socket()
            if socket is None:
                return
            client_command = smoke_command(ROOT, socket, duration)
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

        def start_fcitx() -> None:
            nonlocal fcitx_process, fcitx_status, fcitx_failed
            if (
                not fcitx
                or fcitx_process is not None
                or fcitx_status is not None
                or input_method_keyboard_grab
            ):
                return
            socket = ready_socket()
            if socket is None:
                return
            try:
                fcitx_command = fcitx_launcher_command()
            except FileNotFoundError as error:
                fcitx_status = 127
                fcitx_failed = True
                note(log, output_lock, str(error))
                request_shutdown("Fcitx Wayland launcher could not start")
                return
            note(
                log,
                output_lock,
                f"Tensor is ready on Wayland socket {socket.name}; asking Fcitx to attach "
                f"its Wayland input-method frontend: {shlex.join(fcitx_command)}",
            )
            try:
                fcitx_process = subprocess.Popen(
                    fcitx_command,
                    cwd=ROOT,
                    env=session_client_environment(environment, socket),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                )
            except OSError as error:
                fcitx_status = 127
                fcitx_failed = True
                note(log, output_lock, f"failed to start Fcitx Wayland launcher: {error}")
                request_shutdown("Fcitx Wayland launcher could not start")
                return
            assert fcitx_process.stdout is not None
            selector.register(fcitx_process.stdout, selectors.EVENT_READ, "fcitx")

        def observe_fcitx_exit() -> None:
            nonlocal fcitx_status, fcitx_failed
            if fcitx_process is None or fcitx_status is not None:
                return
            status = fcitx_process.poll()
            if status is None:
                return
            fcitx_status = status
            note(log, output_lock, f"Fcitx Wayland launcher exited with status {status}")
            if not shutdown_requested.is_set():
                fcitx_failed = True
                request_shutdown("Fcitx Wayland launcher exited before Tensor stopped")

        def start_interactive_clients() -> None:
            nonlocal interactive_failed, interactive_failure_status
            if not clients:
                return
            if fcitx and not input_method_registered:
                return
            socket = ready_socket()
            if socket is None:
                return
            for index, client in enumerate(clients):
                if shutdown_requested.is_set():
                    return
                if (
                    interactive_processes[index] is not None
                    or interactive_statuses[index] is not None
                ):
                    continue
                client_command = list(client.command)
                note(
                    log,
                    output_lock,
                    f"Tensor is ready on Wayland socket {socket.name}; starting {client.name} "
                    f"with its normal backend selection: {shlex.join(client_command)}",
                )
                try:
                    interactive_processes[index] = subprocess.Popen(
                        client_command,
                        cwd=client.cwd or ROOT,
                        env=session_client_environment(environment, socket),
                        stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT,
                        start_new_session=True,
                    )
                except OSError as error:
                    interactive_statuses[index] = 127
                    interactive_failed = True
                    interactive_failure_status = 127
                    note(log, output_lock, f"failed to start {client.name}: {error}")
                    request_shutdown(f"{client.name} could not start")
                    return
                interactive_process = interactive_processes[index]
                assert interactive_process is not None and interactive_process.stdout is not None
                selector.register(interactive_process.stdout, selectors.EVENT_READ, "interactive")

        def observe_interactive_client_exits() -> None:
            nonlocal interactive_failed, interactive_failure_status
            for index, (client, interactive_process) in enumerate(
                zip(clients, interactive_processes)
            ):
                if interactive_process is None or interactive_statuses[index] is not None:
                    continue
                status = interactive_process.poll()
                if status is None:
                    continue
                interactive_statuses[index] = status
                note(log, output_lock, f"{client.name} exited with status {status}")
                if not shutdown_requested.is_set() and status != 0:
                    interactive_failed = True
                    interactive_failure_status = status
                    request_shutdown(f"{client.name} failed")
                    return
            if clients and all(status is not None for status in interactive_statuses):
                request_shutdown("all interactive clients closed")

        def stop_smoke_client(reason: str) -> None:
            if not send_signal(smoke_process, signal.SIGTERM):
                return
            note(log, output_lock, f"{reason}; sent SIGTERM to dma-buf smoke client")

        def stop_fcitx(reason: str) -> None:
            if not send_signal(fcitx_process, signal.SIGTERM):
                return
            note(log, output_lock, f"{reason}; sent SIGTERM to Fcitx Wayland launcher")

        def request_shutdown(reason: str) -> None:
            if completed.is_set() or shutdown_requested.is_set():
                return
            shutdown_requested.set()
            smoke_stopped = send_signal(smoke_process, signal.SIGTERM)
            fcitx_stopped = send_signal(fcitx_process, signal.SIGTERM)
            interactive_stopped = [
                client
                for client, interactive_process in zip(clients, interactive_processes)
                if send_process_group_signal(interactive_process, signal.SIGTERM)
            ]
            tensor_stopped = send_signal(process, signal.SIGTERM)

            def force_shutdown() -> None:
                if completed.wait(SHUTDOWN_GRACE_SECONDS) or process.poll() is not None:
                    return
                if not send_signal(process, signal.SIGKILL):
                    return
                force_kill_sent.set()
                send_signal(smoke_process, signal.SIGKILL)
                send_signal(fcitx_process, signal.SIGKILL)
                for interactive_process in interactive_processes:
                    send_process_group_signal(interactive_process, signal.SIGKILL)
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
            if fcitx_stopped:
                note(log, output_lock, f"{reason}; sent SIGTERM to Fcitx Wayland launcher")
            for client in interactive_stopped:
                note(log, output_lock, f"{reason}; sent SIGTERM to {client.name}")
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
                    start_fcitx()
                    observe_fcitx_exit()
                    start_interactive_clients()
                    observe_interactive_client_exits()
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
                    if key.data in {"fcitx", "interactive"}:
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
            for client, interactive_process, interactive_status in zip(
                clients, interactive_processes, interactive_statuses
            ):
                if interactive_process is None and interactive_status is None:
                    interactive_failed = True
                    note(
                        log,
                        output_lock,
                        f"Tensor exited before opening the {client.name} launch gate",
                    )
            if fcitx and not input_method_registered:
                note(
                    log,
                    output_lock,
                    "no Wayland input-method client registered; the active IM did not bind "
                    "Tensor's socket",
                )
            elif fcitx and not input_method_keyboard_grab:
                note(
                    log,
                    output_lock,
                    "a Wayland input-method client registered but no focused text input "
                    "requested its keyboard grab",
                )
            if fcitx and (fcitx_process is None or not input_method_registered):
                fcitx_failed = True
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
            if clients and (any(process is None for process in interactive_processes) or interactive_failed):
                return interactive_failure_status if interactive_failure_status is not None else 1
            if fcitx and fcitx_failed:
                return fcitx_status if fcitx_status is not None else 1
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
            if fcitx_process is not None and fcitx_process.poll() is None:
                stop_fcitx("TTY launcher is stopping")
                if wait_for_exit(fcitx_process, SHUTDOWN_GRACE_SECONDS) is None:
                    if send_signal(fcitx_process, signal.SIGKILL):
                        note(log, output_lock, "sent SIGKILL to Fcitx Wayland launcher")
                    if wait_for_exit(fcitx_process, KILL_GRACE_SECONDS) is None:
                        note(log, output_lock, "Fcitx Wayland launcher remained alive after SIGKILL")
            for client, interactive_process in zip(clients, interactive_processes):
                stopped, killed = terminate_process_group(
                    interactive_process, SHUTDOWN_GRACE_SECONDS, KILL_GRACE_SECONDS
                )
                if killed:
                    note(log, output_lock, f"sent SIGKILL to {client.name}'s process group")
                if not stopped:
                    note(log, output_lock, f"{client.name}'s process group remained alive")
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
    try:
        clients = [client_from_argv(argv) for argv in args.clients or []]
    except ValueError as error:
        raise SystemExit(error) from error
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
        for client in clients:
            print(
                "client: wait for Tensor to enter its event loop, then start "
                f"{shlex.join(client.command)} "
                "with normal backend selection on Tensor's session endpoint"
            )
        if args.fcitx:
            print("input method: attach the running Fcitx 5 daemon before starting the client")
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
    return launch(command, environment, log_path, duration, args.dmabuf_smoke, clients, args.fcitx)


if __name__ == "__main__":
    raise SystemExit(main())
