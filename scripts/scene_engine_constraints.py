#!/usr/bin/env python3
"""Audit hard constraints for the new scene engine path."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MAX_RUST_FILE_LINES = 1000
RUST_SOURCE_ROOTS = ("src", "build")


def main() -> int:
    failures: list[str] = []

    rust_files = owned_rust_files(ROOT)
    oversized_rust_files = [
        (path, line_count(path)) for path in rust_files if line_count(path) > MAX_RUST_FILE_LINES
    ]
    if oversized_rust_files:
        failures.append(
            "Rust files must be <= "
            + str(MAX_RUST_FILE_LINES)
            + " lines: "
            + ", ".join(
                f"{display(path)}={count}" for path, count in oversized_rust_files
            )
        )

    mod_rs = [path for path in rust_files if path.name == "mod.rs"]
    if mod_rs:
        failures.append("mod.rs files are forbidden: " + ", ".join(display(path) for path in mod_rs))

    mechanical_split_files = [path for path in rust_files if "__split" in path.name]
    if mechanical_split_files:
        failures.append(
            "mechanical split Rust filenames are forbidden; use semantic names: "
            + ", ".join(display(path) for path in mechanical_split_files)
        )

    same_name_module_dirs = [
        ROOT / "src/convert/we_ingest",
        ROOT / "src/engine/scene",
        ROOT / "src/renderer/native_vulkan",
        ROOT / "src/renderer/native_vulkan/scene",
    ]
    missing_same_name_pairs = [
        path for path in same_name_module_dirs if not path.is_dir() or not path.with_suffix(".rs").is_file()
    ]
    if missing_same_name_pairs:
        failures.append(
            "scene modules must use same-name file+directory layout: "
            + ", ".join(display(path.with_suffix(".rs")) + " + " + display(path) for path in missing_same_name_pairs)
        )

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

    architecture_doc = read_text(ROOT / "docs/gilder-scene-engine-architecture.md")
    required_scene_effect_doc_markers = [
        "### Scene 能力与特效同步推进",
        "ResolvedSemanticFrame",
        "constantshadervalues",
        "first-class effect target",
        "copy/swap",
    ]
    missing_scene_effect_doc_markers = [
        marker for marker in required_scene_effect_doc_markers if marker not in architecture_doc
    ]
    if missing_scene_effect_doc_markers:
        failures.append(
            "architecture doc must keep scene capability and effect co-development markers: "
            + ", ".join(missing_scene_effect_doc_markers)
        )

    required = [
        ROOT / "src/convert/we_ingest.rs",
        ROOT / "src/convert/we_ingest/ir.rs",
        ROOT / "src/engine/scene/binary.rs",
        ROOT / "src/engine/scene/storage.rs",
        ROOT / "src/engine/scene/rendering_device_graph.rs",
        ROOT / "src/renderer/native_vulkan/scene.rs",
        ROOT / "src/renderer/native_vulkan/scene/backend_plan.rs",
        ROOT / "src/renderer/native_vulkan/scene/resource_storage.rs",
        ROOT / "src/renderer/native_vulkan/scene/pipeline_cache.rs",
        ROOT / "src/renderer/native_vulkan/scene/render_graph_executor.rs",
        ROOT / "src/renderer/native_vulkan/scene/runtime.rs",
        ROOT / "src/renderer/native_vulkan/scene/shader_catalog.rs",
        ROOT / "scripts/scene_engine_cli_smoke.py",
        ROOT / "scripts/scene_engine_runtime_smoke.py",
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
    print(f"checked: Rust files are <= {MAX_RUST_FILE_LINES} lines")
    print("checked: no mod.rs, no mechanical split names, and scene modules use same-name file+directory layout")
    print("checked: no old scene compatibility files, no shader artifact runtime refs")
    print("checked: native Vulkan scene path has no legacy descriptor-set binding tokens")
    print("checked: scene capability and effect co-development remains documented")
    return 0


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def owned_rust_files(root: Path) -> list[Path]:
    """Return repository-owned Rust sources without scanning ignored references."""
    rust_files = []
    root_build = root / "build.rs"
    if root_build.is_file():
        rust_files.append(root_build)
    for relative_root in RUST_SOURCE_ROOTS:
        source_root = root / relative_root
        if source_root.is_dir():
            rust_files.extend(source_root.rglob("*.rs"))
    return sorted(rust_files)


def line_count(path: Path) -> int:
    return len(read_text(path).splitlines())


def display(path: Path) -> str:
    return str(path.relative_to(ROOT))


if __name__ == "__main__":
    raise SystemExit(main())
