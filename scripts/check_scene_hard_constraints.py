#!/usr/bin/env python3
"""Check non-negotiable scene implementation constraints."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from re import Pattern


DEFAULT_ROOTS = (
    "src",
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


def forbidden_implementation_patterns() -> tuple[Pattern[str], ...]:
    legacy_a = "Descriptor"
    legacy_b = "Set"
    pool = "Pool"
    vk_prefix = "v" + "k"
    vk_type_prefix = "V" + "k"
    upper_a = legacy_a.upper()
    upper_b = legacy_b.upper()
    upper_pool = pool.upper()
    return (
        re.compile(
            rf"\b(?:vk::)?{legacy_a}{legacy_b}(?:Layout|AllocateInfo|LayoutBinding|LayoutCreateInfo)?\b"
        ),
        re.compile(rf"\b(?:vk::)?{legacy_a}{pool}(?:CreateInfo|Size)?\b"),
        re.compile(rf"\b(?:vk::)?Write{legacy_a}{legacy_b}[A-Za-z0-9_]*\b"),
        re.compile(rf"\b(?:PFN_)?{vk_prefix}CmdBind{legacy_a}{legacy_b}s?\b"),
        re.compile(rf"\b(?:PFN_)?{vk_prefix}CmdPush{legacy_a}{legacy_b}[A-Za-z0-9_]*\b"),
        re.compile(
            rf"\b(?:PFN_)?{vk_prefix}(?:Allocate|Free|Update){legacy_a}{legacy_b}s?\b"
        ),
        re.compile(
            rf"\b(?:PFN_)?{vk_prefix}(?:Create|Destroy){legacy_a}{legacy_b}Layout[A-Za-z0-9_]*\b"
        ),
        re.compile(rf"\b(?:PFN_)?{vk_prefix}(?:Create|Destroy|Reset){legacy_a}{pool}[A-Za-z0-9_]*\b"),
        re.compile(r"\b(cmd_bind|cmd_push|allocate|free|update|create|destroy|reset)_descriptor_(sets?|set_layout|pool)\b"),
        re.compile(rf"\b{vk_type_prefix}{legacy_a}{legacy_b}[A-Za-z0-9_]*\b"),
        re.compile(rf"\b{vk_type_prefix}{legacy_a}{pool}[A-Za-z0-9_]*\b"),
        re.compile(rf"\bVK_[A-Z0-9_]*{upper_a}_{upper_b}[A-Z0-9_]*\b"),
        re.compile(rf"\bVK_[A-Z0-9_]*{upper_a}_{upper_pool}[A-Z0-9_]*\b"),
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


def implementation_text(path: Path) -> str:
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        text = path.read_text(encoding="utf-8", errors="ignore")
    if path.suffix != ".rs":
        return text

    cleaned: list[str] = []
    i = 0
    block_depth = 0
    in_line_comment = False
    in_string = False
    in_char = False
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
                cleaned.append(ch)
            else:
                cleaned.append(" ")
            i += 1
            continue

        if block_depth:
            if ch == "/" and nxt == "*":
                block_depth += 1
                cleaned.extend("  ")
                i += 2
            elif ch == "*" and nxt == "/":
                block_depth -= 1
                cleaned.extend("  ")
                i += 2
            else:
                cleaned.append("\n" if ch == "\n" else " ")
                i += 1
            continue

        if in_string:
            if ch == "\\" and nxt:
                cleaned.extend("  ")
                i += 2
            else:
                in_string = ch != '"'
                cleaned.append("\n" if ch == "\n" else " ")
                i += 1
            continue

        if in_char:
            if ch == "\\" and nxt:
                cleaned.extend("  ")
                i += 2
            else:
                in_char = ch != "'"
                cleaned.append("\n" if ch == "\n" else " ")
                i += 1
            continue

        if ch == "/" and nxt == "/":
            in_line_comment = True
            cleaned.extend("  ")
            i += 2
            continue
        if ch == "/" and nxt == "*":
            block_depth = 1
            cleaned.extend("  ")
            i += 2
            continue
        if ch == '"':
            in_string = True
            cleaned.append(" ")
            i += 1
            continue
        if ch == "'":
            in_char = True
            cleaned.append(" ")
            i += 1
            continue

        cleaned.append(ch)
        i += 1

    return "".join(cleaned)


def matching_lines(path: Path, patterns: tuple[Pattern[str], ...]) -> list[tuple[int, str]]:
    text = implementation_text(path)
    matches: list[tuple[int, str]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if any(pattern.search(line) for pattern in patterns):
            matches.append((line_number, line.strip()))
    return matches


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("roots", nargs="*", default=DEFAULT_ROOTS)
    args = parser.parse_args()

    patterns = forbidden_implementation_patterns()
    failures: list[str] = []
    for path in iter_files(tuple(args.roots)):
        for line_number, line in matching_lines(path, patterns):
            failures.append(f"{path}:{line_number}: legacy binding implementation use: {line}")

    if failures:
        print("scene hard-constraint check failed:")
        print("\n".join(failures))
        return 1

    print("scene hard-constraint check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
