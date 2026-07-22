#!/usr/bin/env -S uv run --script
"""Enforce the repository's hand-written source line limit."""

from __future__ import annotations

import argparse
import os
from pathlib import Path


SOURCE_SUFFIXES = {".py", ".rs"}


def source_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for directory in (root / "src", root / "scripts", root / "crates"):
        if not directory.exists():
            continue
        files.extend(
            path
            for path in directory.rglob("*")
            if path.is_file() and path.suffix in SOURCE_SUFFIXES
        )
    return sorted(files)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--max-lines",
        type=int,
        default=int(os.environ.get("MAX_FILE_LINES", "800")),
    )
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_lines < 1:
        raise SystemExit("--max-lines must be positive")

    failures = [
        (path, len(path.read_text(encoding="utf-8").splitlines()))
        for path in source_files(args.root)
    ]
    failures = [(path, lines) for path, lines in failures if lines > args.max_lines]
    for path, lines in failures:
        print(f"file exceeds {args.max_lines}-line limit: {path} ({lines} lines)")
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
