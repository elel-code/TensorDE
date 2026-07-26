#!/usr/bin/env -S uv run --script
"""Enforce Smithay-free crates and Compio completion-runtime dependencies."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SMITHAY_DEPENDENCIES = {"smithay", "smithay-drm-extras"}
READINESS_DEPENDENCIES = {"calloop", "mio", "polling"}
SMITHAY_CODE = re.compile(r"\bsmithay::|\bextern\s+crate\s+smithay\b")


def dependency_tables(manifest: dict) -> list[dict]:
    tables = [
        manifest.get("dependencies", {}),
        manifest.get("dev-dependencies", {}),
        manifest.get("build-dependencies", {}),
    ]
    for target in manifest.get("target", {}).values():
        tables.extend(
            [
                target.get("dependencies", {}),
                target.get("dev-dependencies", {}),
                target.get("build-dependencies", {}),
            ]
        )
    return tables


def main() -> int:
    failures: list[str] = []
    for source in sorted((ROOT / "src").glob("**/*.rs")):
        relative = source.relative_to(ROOT / "src")
        if relative.parts[0] in {"backend", "protocol"}:
            continue
        if SMITHAY_CODE.search(source.read_text(encoding="utf-8")):
            failures.append(f"{source}: Smithay path outside backend/protocol adapter")

    for manifest_path in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        package = manifest["package"]["name"]
        if package == "tensor-smithay":
            continue
        dependencies = set().union(*(table.keys() for table in dependency_tables(manifest)))
        forbidden = dependencies & SMITHAY_DEPENDENCIES
        if forbidden:
            failures.append(f"{manifest_path}: forbidden dependencies {sorted(forbidden)}")
        for source in sorted(manifest_path.parent.glob("src/**/*.rs")):
            if SMITHAY_CODE.search(source.read_text(encoding="utf-8")):
                failures.append(f"{source}: Smithay path outside adapter crate")

        if package == "tensor-runtime":
            compio = manifest["dependencies"].get("compio", {})
            features = set(compio.get("features", []))
            required = {"runtime", "async-fd", "io-uring"}
            if compio.get("default-features", True) or not required <= features:
                failures.append(
                    f"{manifest_path}: Compio must disable defaults and enable {sorted(required)}"
                )
            direct_readiness = dependencies & READINESS_DEPENDENCIES
            if direct_readiness:
                failures.append(
                    f"{manifest_path}: readiness dependencies are forbidden {sorted(direct_readiness)}"
                )

    for failure in failures:
        print(failure)
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
