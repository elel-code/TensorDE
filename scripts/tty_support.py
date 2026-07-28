"""Small process and logging primitives shared by Tensor's TTY launcher."""

from __future__ import annotations

from datetime import datetime
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
from typing import BinaryIO


def smoke_command(root: Path, socket: Path, duration: float | None) -> list[str]:
    command = [
        str(root / "target" / "debug" / "tensor-dmabuf-smoke"),
        "--socket",
        socket.name,
    ]
    if duration is not None:
        command.extend(["--timeout", str(max(1, int(duration - 2)))])
    return command


def write_launcher_log(log: BinaryIO, output_lock: threading.Lock, chunk: bytes) -> None:
    with output_lock:
        log.write(chunk)


def note(log: BinaryIO, output_lock: threading.Lock, message: str) -> None:
    timestamp = datetime.now().astimezone().isoformat(timespec="seconds")
    write_launcher_log(log, output_lock, f"[tensor-tty {timestamp}] {message}\n".encode())


def send_signal(process: subprocess.Popen[bytes] | None, signal_number: int) -> bool:
    """Best-effort delivery without making shutdown depend on logging."""
    if process is None or process.poll() is not None:
        return False
    try:
        process.send_signal(signal_number)
    except ProcessLookupError:
        return False
    return True


def send_process_group_signal(
    process: subprocess.Popen[bytes] | None, signal_number: int
) -> bool:
    """Signal an isolated client and any children it kept in its process group."""
    if process is None:
        return False
    try:
        os.killpg(process.pid, signal_number)
    except ProcessLookupError:
        return False
    return True


def terminate_process_group(
    process: subprocess.Popen[bytes] | None, grace_seconds: float, kill_seconds: float
) -> tuple[bool, bool]:
    """Terminate an isolated client group, returning ``(exited, killed)``."""
    if process is None or not _process_group_exists(process.pid):
        return True, False
    send_process_group_signal(process, signal.SIGTERM)
    if _wait_for_process_group_exit(process, grace_seconds):
        return True, False
    send_process_group_signal(process, signal.SIGKILL)
    return _wait_for_process_group_exit(process, kill_seconds), True


def _process_group_exists(group_id: int) -> bool:
    try:
        os.killpg(group_id, 0)
    except ProcessLookupError:
        return False
    return True


def _wait_for_process_group_exit(process: subprocess.Popen[bytes], timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while _process_group_exists(process.pid):
        process.poll()
        if not _process_group_exists(process.pid):
            return True
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return False
        time.sleep(min(0.05, remaining))
    return True


def wait_for_exit(process: subprocess.Popen[bytes], timeout: float) -> int | None:
    """Reap for a bounded period, including a process stuck in kernel sleep."""
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return None
