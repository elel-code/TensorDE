#!/usr/bin/env python3
"""Audit hard constraints for the new scene engine path."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"


def main() -> int:
    failures: list[str] = []

    rust_files = list(SRC.rglob("*.rs"))
    mod_rs = [path for path in rust_files if path.name == "mod.rs"]
    if mod_rs:
        failures.append("mod.rs files are forbidden: " + ", ".join(display(path) for path in mod_rs))

    old_scene_files = [
        ROOT / "src/core/scene/binary.rs",
        ROOT / "src/core/scene/binary",
        ROOT / "src/renderer/scene_binary.rs",
        ROOT / "src/renderer/scene_binary",
        ROOT / "src/engine/scene_engine.rs",
        ROOT / "src/engine/scene_engine",
        ROOT / "src/renderer/native_vulkan/scene_backend.rs",
        ROOT / "src/renderer/native_vulkan/scene_backend",
        ROOT / "src/renderer/native_vulkan/present/scene_runtime.rs",
        ROOT / "src/convert/wallpaper_engine.rs",
        ROOT / "src/convert/wallpaper_engine",
    ]
    revived = [path for path in old_scene_files if path.exists()]
    if revived:
        failures.append(
            "old scene compatibility files were revived: "
            + ", ".join(display(path) for path in revived)
        )

    source_text = "\n".join(read_text(path) for path in rust_files)
    if "artifacts/scene-shaders" in source_text:
        failures.append("runtime source references artifacts/scene-shaders")

    native_scene_files = [ROOT / "src/renderer/native_vulkan/scene.rs"]
    native_scene_files.extend((ROOT / "src/renderer/native_vulkan/scene").rglob("*.rs"))
    native_scene_text = "\n".join(read_text(path) for path in native_scene_files if path.exists())
    descriptor_set_tokens = [
        "DescriptorSet",
        "descriptor set",
        "descriptor pool",
        "descriptor layout",
        "push descriptor",
    ]
    found_descriptor_set_tokens = [
        token for token in descriptor_set_tokens if token in native_scene_text
    ]
    if found_descriptor_set_tokens:
        failures.append(
            "native Vulkan scene path mentions legacy descriptor-set binding: "
            + ", ".join(found_descriptor_set_tokens)
        )

    required = [
        ROOT / "src/convert/we_ingest.rs",
        ROOT / "src/convert/we_ingest/ir.rs",
        ROOT / "src/engine/scene/binary.rs",
        ROOT / "src/engine/scene/storage.rs",
        ROOT / "src/engine/scene/rendering_device_graph.rs",
        ROOT / "src/renderer/native_vulkan/scene.rs",
        ROOT / "src/renderer/native_vulkan/scene/backend_plan.rs",
    ]
    missing = [path for path in required if not path.exists()]
    if missing:
        failures.append(
            "new scene engine files are missing: " + ", ".join(display(path) for path in missing)
        )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}")
        return 1

    print("scene-engine-constraints: ok")
    print("checked: no mod.rs, no old scene compatibility files, no shader artifact runtime refs")
    print("checked: native Vulkan scene path has no legacy descriptor-set binding tokens")
    return 0


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def display(path: Path) -> str:
    return str(path.relative_to(ROOT))


if __name__ == "__main__":
    raise SystemExit(main())
