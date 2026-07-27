#!/usr/bin/env -S uv run --script
"""Enforce Smithay-free crates and Compio completion-runtime dependencies."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SMITHAY_DEPENDENCIES = {"smithay", "smithay-drm-extras"}
READINESS_DEPENDENCIES = {"calloop", "mio", "polling"}
REQUIRED_COMPIO_FEATURES = {"runtime", "async-fd", "io-uring"}
SMITHAY_CODE = re.compile(r"\bsmithay::|\bextern\s+crate\s+smithay\b")
DEFAULT_COMPIO_RUNTIME = re.compile(r"(?:compio::runtime::)?Runtime::new\s*\(")


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


def check_compio_features(
    manifest_path: Path, manifest: dict, failures: list[str]
) -> None:
    compio = manifest.get("dependencies", {}).get("compio")
    if compio is None:
        return
    features = set(compio.get("features", []))
    if compio.get("default-features", True) or not REQUIRED_COMPIO_FEATURES <= features:
        failures.append(
            f"{manifest_path}: Compio must disable defaults and enable "
            f"{sorted(REQUIRED_COMPIO_FEATURES)}"
        )
    if "polling" in features:
        failures.append(f"{manifest_path}: Compio polling fallback must remain disabled")


def main() -> int:
    failures: list[str] = []
    root_manifest_path = ROOT / "Cargo.toml"
    root_manifest = tomllib.loads(root_manifest_path.read_text(encoding="utf-8"))
    check_compio_features(root_manifest_path, root_manifest, failures)

    for source in sorted((ROOT / "src").glob("**/*.rs")):
        relative = source.relative_to(ROOT / "src")
        if relative.parts[0] in {"backend", "protocol"}:
            continue
        if SMITHAY_CODE.search(source.read_text(encoding="utf-8")):
            failures.append(f"{source}: Smithay path outside backend/protocol adapter")
        if DEFAULT_COMPIO_RUNTIME.search(source.read_text(encoding="utf-8")):
            failures.append(f"{source}: use tensor_runtime::io_uring_runtime with a fixed budget")

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
            text = source.read_text(encoding="utf-8")
            if SMITHAY_CODE.search(text):
                failures.append(f"{source}: Smithay path outside adapter crate")
            if DEFAULT_COMPIO_RUNTIME.search(text):
                failures.append(
                    f"{source}: use tensor_runtime::io_uring_runtime with a fixed budget"
                )

        if package == "tensor-runtime":
            check_compio_features(manifest_path, manifest, failures)
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
