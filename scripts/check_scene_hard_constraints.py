#!/usr/bin/env python3
"""Check non-negotiable scene source constraints."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from re import Pattern


DEFAULT_ROOTS = (
    "README.md",
    "README.zh-CN.md",
    "src",
    "docs",
    "reverse-engineered",
    "scripts",
    "examples",
    "packaging",
    "completions",
    "build.rs",
    "Cargo.toml",
)

SKIPPED_DIRS = {
    ".git",
    "__pycache__",
    "target",
    "artifacts",
    "references",
}

SKIPPED_SUFFIXES = {
    ".dll",
    ".exe",
    ".lib",
    ".o",
    ".pdb",
    ".png",
    ".pyc",
    ".so",
    ".spv",
    ".zip",
}


def forbidden_patterns() -> tuple[Pattern[str], ...]:
    legacy_binding_a = "descriptor"
    legacy_binding_b = "resource"
    low_b = "set"
    camel_b = low_b.title()
    pool_b = "pool"
    pool_camel = pool_b.title()
    vk_prefix = "V" + "k"
    vk_func_prefix = "v" + "k"
    desc_camel = legacy_binding_a.title()
    desc_upper = legacy_binding_a.upper()
    set_upper = low_b.upper()
    pool_upper = pool_b.upper()
    return (
        re.compile(rf"{legacy_binding_a}\s+{low_b}", re.IGNORECASE),
        re.compile(rf"\b{legacy_binding_a}[\s_-]+{low_b}s?\b", re.IGNORECASE),
        re.compile(rf"\b{legacy_binding_a}{camel_b}s?\b", re.IGNORECASE),
        re.compile(rf"\b{legacy_binding_b}[\s_-]+{low_b}s?\b", re.IGNORECASE),
        re.compile(rf"\b{legacy_binding_b}{camel_b}s?\b", re.IGNORECASE),
        re.compile(r"\bset\d+\.binding\d+\b", re.IGNORECASE),
        re.compile(rf"\b{vk_prefix}(?:Write)?{desc_camel}{camel_b}[A-Za-z0-9_]*\b"),
        re.compile(
            rf"\b{vk_func_prefix}(?:CmdBind|CmdPush|Allocate|Free|Update|Create|Destroy|Reset)"
            rf"{desc_camel}{camel_b}[A-Za-z0-9_]*\b"
        ),
        re.compile(rf"\bVK_[A-Z0-9_]*{desc_upper}_{set_upper}[A-Z0-9_]*\b"),
        re.compile(rf"\b{vk_prefix}{desc_camel}{pool_camel}[A-Za-z0-9_]*\b"),
        re.compile(
            rf"\b{vk_func_prefix}(?:Create|Destroy|Reset){desc_camel}{pool_camel}[A-Za-z0-9_]*\b"
        ),
        re.compile(rf"\bVK_[A-Z0-9_]*{desc_upper}_{pool_upper}[A-Z0-9_]*\b"),
    )


def should_scan(path: Path) -> bool:
    if any(part in SKIPPED_DIRS for part in path.parts):
        return False
    return path.suffix.lower() not in SKIPPED_SUFFIXES


def iter_files(roots: tuple[str, ...]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        root_path = Path(root)
        if root_path.is_file():
            if should_scan(root_path):
                files.append(root_path)
            continue
        if not root_path.exists():
            raise SystemExit(f"missing constraint root: {root}")
        files.extend(path for path in root_path.rglob("*") if path.is_file() and should_scan(path))
    return sorted(files)


def matching_lines(path: Path, patterns: tuple[Pattern[str], ...]) -> list[tuple[int, str]]:
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        text = path.read_text(encoding="utf-8", errors="ignore")
    matches: list[tuple[int, str]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if any(pattern.search(line) for pattern in patterns):
            matches.append((line_number, line.strip()))
    return matches


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("roots", nargs="*", default=DEFAULT_ROOTS)
    args = parser.parse_args()

    patterns = forbidden_patterns()
    failures: list[str] = []
    for path in iter_files(tuple(args.roots)):
        for line_number, line in matching_lines(path, patterns):
            failures.append(f"{path}:{line_number}: legacy binding token: {line}")

    if failures:
        print("scene hard-constraint check failed:")
        print("\n".join(failures))
        return 1

    print("scene hard-constraint check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
