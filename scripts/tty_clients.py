"""External-client descriptions and session environment helpers for ``tty.py``."""

from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import shutil


@dataclass(frozen=True)
class InteractiveClient:
    """A user-visible client launched after Tensor publishes its socket."""

    name: str
    command: tuple[str, ...]
    cwd: Path | None = None


def ghostty_client() -> InteractiveClient:
    # This only makes the command a new process. It does not choose a GTK/GDK
    # backend: Ghostty still performs its ordinary Wayland-vs-X11 selection.
    # Without it, a Ghostty already serving the suspended host desktop can
    # accept this request over D-Bus and leave Tensor with no client at all.
    return InteractiveClient("Ghostty", ("ghostty", "--gtk-single-instance=false"))


def application_client(path: Path) -> InteractiveClient:
    """Describe one executable application without invoking a shell."""
    path = path.expanduser().resolve()
    if not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError(f"application is not an executable file: {path}")
    return InteractiveClient(path.name, (str(path),), path.parent)


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
