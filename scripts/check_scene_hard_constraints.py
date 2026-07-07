#!/usr/bin/env python3
"""Check non-negotiable scene source constraints."""

from __future__ import annotations

import argparse
from pathlib import Path


DEFAULT_ROOTS = (
    "README.md",
    "README.zh-CN.md",
    "src",
    "docs",
    "scripts",
    "examples",
    "packaging",
    "completions",
    "build.rs",
    "Cargo.toml",
)


def forbidden_terms() -> tuple[str, ...]:
    low_a = "descriptor"
    low_b = "set"
    cap_a = "Descriptor"
    cap_b = "Set"
    upper_a = "DESCRIPTOR"
    upper_b = "SET"
    return (
        low_a + " " + low_b,
        low_a + "-" + low_b,
        low_a + "_" + low_b,
        cap_a + cap_b,
        upper_a + "_" + upper_b,
    )


def iter_files(roots: tuple[str, ...]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        root_path = Path(root)
        if root_path.is_file():
            files.append(root_path)
            continue
        if not root_path.exists():
            raise SystemExit(f"missing constraint root: {root}")
        files.extend(path for path in root_path.rglob("*") if path.is_file())
    return sorted(files)


def matching_lines(path: Path, terms: tuple[str, ...]) -> list[tuple[int, str]]:
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        text = path.read_text(encoding="utf-8", errors="ignore")
    matches: list[tuple[int, str]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if any(term in line for term in terms):
            matches.append((line_number, line.strip()))
    return matches


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("roots", nargs="*", default=DEFAULT_ROOTS)
    args = parser.parse_args()

    terms = forbidden_terms()
    failures: list[str] = []
    for path in iter_files(tuple(args.roots)):
        for line_number, line in matching_lines(path, terms):
            failures.append(f"{path}:{line_number}: legacy binding token: {line}")

    if failures:
        print("scene hard-constraint check failed:")
        print("\n".join(failures))
        return 1

    print("scene hard-constraint check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
