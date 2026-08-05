#!/usr/bin/env -S uv run --script
"""Enforce a Smithay-free workspace and Compio completion-runtime dependencies."""

from __future__ import annotations

import re
import tomllib
from pathlib import Path


WORKSPACE_ROOT = Path(__file__).resolve().parents[2]
TENSOR_ROOT = WORKSPACE_ROOT / "apps/tensor-wm"
SMITHAY_DEPENDENCIES = {"smithay", "smithay-drm-extras"}
READINESS_DEPENDENCIES = {"calloop", "mio", "polling"}
REQUIRED_COMPIO_FEATURES = {"runtime", "async-fd", "io-uring"}
FORBIDDEN_COMPIO_FEATURES = {
    "compat",
    "compat-all",
    "compat-futures",
    "compat-tokio",
    "polling",
}
SMITHAY_CODE = re.compile(r"\bsmithay::|\bextern\s+crate\s+smithay\b")
DEFAULT_COMPIO_RUNTIME = re.compile(r"(?:compio::runtime::)?Runtime::new\s*\(")
RAW_GRAPHICS_PIPELINE_LIFECYCLE = re.compile(
    r"\b(?:create_graphics_pipelines|create_pipeline_layout|destroy_pipeline_layout)\s*\("
)
RAW_TENSOR_RENDER_ESCAPE = re.compile(
    r"\b(?:native_device|write_sampled_image_array|allocate_primary_command_buffer|submit_raw)\s*\("
    r"|\bencoder\.raw\s*\("
    r"|\b(?:device|native)\.cmd_[A-Za-z0-9_]*\s*\("
    r"|\b(?:device|queue)\.queue_submit2?\s*\("
)


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
    forbidden = features & FORBIDDEN_COMPIO_FEATURES
    if forbidden:
        failures.append(
            f"{manifest_path}: forbidden Compio features {sorted(forbidden)}"
        )


def main() -> int:
    failures: list[str] = []
    root_manifest_path = TENSOR_ROOT / "Cargo.toml"
    root_manifest = tomllib.loads(root_manifest_path.read_text(encoding="utf-8"))
    check_compio_features(root_manifest_path, root_manifest, failures)
    root_dependencies = set().union(
        *(table.keys() for table in dependency_tables(root_manifest))
    )
    root_forbidden = root_dependencies & SMITHAY_DEPENDENCIES
    if root_forbidden:
        failures.append(
            f"{root_manifest_path}: forbidden dependencies {sorted(root_forbidden)}"
        )
    root_readiness = root_dependencies & READINESS_DEPENDENCIES
    if root_readiness:
        failures.append(
            f"{root_manifest_path}: readiness dependencies are forbidden "
            f"{sorted(root_readiness)}"
        )

    for source in sorted((TENSOR_ROOT / "src").glob("**/*.rs")):
        text = source.read_text(encoding="utf-8")
        if SMITHAY_CODE.search(text):
            failures.append(f"{source}: Smithay code paths are forbidden")
        if DEFAULT_COMPIO_RUNTIME.search(text):
            failures.append(f"{source}: use tensor_runtime::io_uring_runtime with a fixed budget")
        if RAW_GRAPHICS_PIPELINE_LIFECYCLE.search(text):
            failures.append(
                f"{source}: raw graphics-pipeline lifecycle must use vulkan_renderer typed APIs"
            )
        if source.is_relative_to(TENSOR_ROOT / "src" / "render") and RAW_TENSOR_RENDER_ESCAPE.search(text):
            failures.append(
                f"{source}: raw Tensor renderer recording/lifecycle escape must use vulkan_renderer typed APIs"
            )

    for manifest_path in sorted((WORKSPACE_ROOT / "crates").glob("tensor-*/Cargo.toml")):
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        package = manifest["package"]["name"]
        if package == "tensor-smithay":
            failures.append(f"{manifest_path}: a Smithay adapter crate is forbidden")
        dependencies = set().union(*(table.keys() for table in dependency_tables(manifest)))
        forbidden = dependencies & SMITHAY_DEPENDENCIES
        if forbidden:
            failures.append(f"{manifest_path}: forbidden dependencies {sorted(forbidden)}")
        direct_readiness = dependencies & READINESS_DEPENDENCIES
        if direct_readiness:
            failures.append(
                f"{manifest_path}: readiness dependencies are forbidden "
                f"{sorted(direct_readiness)}"
            )
        for source in sorted(manifest_path.parent.glob("src/**/*.rs")):
            text = source.read_text(encoding="utf-8")
            if SMITHAY_CODE.search(text):
                failures.append(f"{source}: Smithay code paths are forbidden")
            if DEFAULT_COMPIO_RUNTIME.search(text):
                failures.append(
                    f"{source}: use tensor_runtime::io_uring_runtime with a fixed budget"
                )

        if package == "tensor-runtime":
            check_compio_features(manifest_path, manifest, failures)

    for failure in failures:
        print(failure)
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
