"""External-client descriptions and session environment helpers for ``tty.py``."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import os
from pathlib import Path
import shutil


class ClientArgvAction(argparse.Action):
    """Build direct client argv vectors while preserving option order."""

    def __call__(
        self,
        parser: argparse.ArgumentParser,
        namespace: argparse.Namespace,
        value: str,
        option_string: str | None = None,
    ) -> None:
        clients = getattr(namespace, self.dest, None)
        if clients is None:
            clients = []
            setattr(namespace, self.dest, clients)
        if self.option_strings[0] == "--client":
            clients.append([value])
            return
        if not clients:
            parser.error("--client-arg requires a preceding --client PROGRAM")
        clients[-1].append(value)


@dataclass(frozen=True)
class InteractiveClient:
    """A user-visible client launched after Tensor publishes its socket."""

    name: str
    command: tuple[str, ...]
    cwd: Path | None = None


def client_from_argv(argv: list[str]) -> InteractiveClient:
    """Describe one executable argv without invoking a shell."""
    if not argv:
        raise ValueError("--client requires at least one argv item")
    program, *arguments = argv
    if "/" in program:
        path = Path(program).expanduser().resolve()
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ValueError(f"client is not an executable file: {path}")
        return InteractiveClient(path.name, (str(path), *arguments), path.parent)
    executable = shutil.which(program)
    if executable is None:
        raise ValueError(f"client executable was not found on PATH: {program}")
    return InteractiveClient(program, (executable, *arguments))


def fcitx_launcher_command() -> tuple[str, ...]:
    """Find Fcitx's compositor integration launcher on common Linux layouts."""
    discovered = shutil.which("fcitx5-wayland-launcher")
    candidates = [
        Path(discovered) if discovered is not None else None,
        Path("/usr/lib/fcitx5-wayland-launcher"),
        Path("/usr/libexec/fcitx5-wayland-launcher"),
    ]
    for candidate in candidates:
        if candidate is not None and candidate.is_file() and os.access(candidate, os.X_OK):
            return (str(candidate),)
    raise FileNotFoundError("could not find fcitx5-wayland-launcher")


def session_client_environment(
    environment: dict[str, str], socket: Path
) -> dict[str, str]:
    """Recreate Tensor's published values for one external client.

    ``tty.py`` is Tensor's parent, so it cannot inherit the environment Tensor
    publishes to its own autostart children. The stale host X11 display is
    removed; a client may still select its normal Wayland backend.
    """
    client_environment = environment.copy()
    client_environment["WAYLAND_DISPLAY"] = socket.name
    client_environment["XDG_CURRENT_DESKTOP"] = "tensor"
    client_environment["XDG_SESSION_TYPE"] = "wayland"
    client_environment.pop("DISPLAY", None)
    return client_environment
