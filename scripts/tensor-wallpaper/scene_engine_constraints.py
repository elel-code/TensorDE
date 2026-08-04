#!/usr/bin/env python3
"""Audit hard constraints for the new scene engine path."""

from __future__ import annotations

import tomllib
from pathlib import Path

from workspace_paths import DOCS_ROOT, TENSOR_WALLPAPER_ROOT, WORKSPACE_ROOT

ROOT = TENSOR_WALLPAPER_ROOT
MAX_RUST_FILE_LINES = 800
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
        ROOT / "src/renderer/rendering_device",
        ROOT / "src/renderer/rendering_device/scene",
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
        ROOT / "src/renderer/rendering_device/scene_backend.rs",
        ROOT / "src/renderer/rendering_device/scene_backend",
        ROOT / "src/renderer/rendering_device/present/scene_runtime.rs",
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

    product_vulkan_escape_tokens = [
        "use vulkanalia",
        "vulkanalia::",
        "vulkan_renderer::vulkanalia",
        "vulkan_renderer::vk",
    ]
    found_product_vulkan_escape_tokens = [
        token for token in product_vulkan_escape_tokens if token in source_text
    ]
    if found_product_vulkan_escape_tokens:
        failures.append(
            "Tensor Wallpaper must use typed vulkan-renderer APIs without a raw Vulkan escape: "
            + ", ".join(found_product_vulkan_escape_tokens)
        )

    manifest = tomllib.loads(read_text(ROOT / "Cargo.toml"))
    forbidden_direct_vulkan_dependencies = {"ash", "vulkanalia"}
    found_direct_vulkan_dependencies = sorted(
        forbidden_direct_vulkan_dependencies & manifest_dependency_names(manifest)
    )
    if found_direct_vulkan_dependencies:
        failures.append(
            "Tensor Wallpaper must not declare a direct raw Vulkan dependency: "
            + ", ".join(found_direct_vulkan_dependencies)
        )
    found_raw_feature_dependencies = sorted(manifest_raw_feature_dependencies(manifest))
    if found_raw_feature_dependencies:
        failures.append(
            "Tensor Wallpaper features must not restore a raw Vulkan dependency: "
            + ", ".join(found_raw_feature_dependencies)
        )

    product_presentation_owner_tokens = [
        "SharedPresentationBootstrap",
        "SharedPresentationBootstrapDescriptor",
        "Instance::new(",
        "InstanceDescriptor::for_window(",
        ".request_adapter(",
        ".create_surface(",
        ".create_swapchain(",
        ".create_memory_allocator(",
        ".create_upload_belt(",
        "PipelineBinaryArchiveCache::new(",
    ]
    found_product_presentation_owner_tokens = [
        token for token in product_presentation_owner_tokens if token in source_text
    ]
    if found_product_presentation_owner_tokens:
        failures.append(
            "Tensor Wallpaper must not recreate renderer-owned presentation lifecycle: "
            + ", ".join(found_product_presentation_owner_tokens)
        )

    required_shared_terminal_tokens = [
        "create_fullscreen_sampled_surface_terminal(",
        "create_decoded_video_surface_terminal(",
    ]
    missing_shared_terminal_tokens = [
        token for token in required_shared_terminal_tokens if token not in source_text
    ]
    if missing_shared_terminal_tokens:
        failures.append(
            "Tensor Wallpaper must use renderer-owned offscreen and decoded-video terminals: "
            + ", ".join(missing_shared_terminal_tokens)
        )

    required_terminal_abi_tokens = [
        "FullscreenSampledSurfaceTerminalDescriptor",
        "OffscreenSamplerTopology::PerFrameSlot",
    ]
    missing_terminal_abi_tokens = [
        token for token in required_terminal_abi_tokens if token not in source_text
    ]
    if missing_terminal_abi_tokens:
        failures.append(
            "Tensor Wallpaper must preserve the renderer-owned terminal descriptor ABI: "
            + ", ".join(missing_terminal_abi_tokens)
        )

    obsolete_product_terminal_tokens = [
        "SharedSceneTerminalResources",
        "SharedDecodedVideoSurfaceResources",
    ]
    found_obsolete_product_terminal_tokens = [
        token for token in obsolete_product_terminal_tokens if token in source_text
    ]
    if found_obsolete_product_terminal_tokens:
        failures.append(
            "Tensor Wallpaper must not revive product-owned terminal resource implementations: "
            + ", ".join(found_obsolete_product_terminal_tokens)
        )

    rendering_device_scene_files = [ROOT / "src/renderer/rendering_device/scene.rs"]
    rendering_device_scene_files.extend((ROOT / "src/renderer/rendering_device/scene").rglob("*.rs"))
    scene_execution_text = "\n".join(read_text(path) for path in rendering_device_scene_files if path.exists())
    descriptor_set_tokens = [
        "DescriptorSet",
        "descriptor set",
        "descriptor pool",
        "descriptor layout",
        "push descriptor",
    ]
    found_descriptor_set_tokens = [
        token for token in descriptor_set_tokens if token in scene_execution_text
    ]
    if found_descriptor_set_tokens:
        failures.append(
            "Vulkan scene path mentions legacy descriptor-set binding: "
            + ", ".join(found_descriptor_set_tokens)
        )

    architecture_doc = read_text(DOCS_ROOT / "tensor-wallpaper-scene-engine-architecture.md")
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
        ROOT / "src/renderer/rendering_device/scene.rs",
        ROOT / "src/renderer/rendering_device/scene/execution_plan.rs",
        ROOT / "src/renderer/rendering_device/scene/resource_storage.rs",
        ROOT / "src/renderer/rendering_device/scene/pipeline_cache.rs",
        ROOT / "src/renderer/rendering_device/scene/render_graph_executor.rs",
        ROOT / "src/renderer/rendering_device/scene/runtime.rs",
        ROOT / "src/renderer/rendering_device/scene/shader_catalog.rs",
        WORKSPACE_ROOT / "scripts/tensor-wallpaper/scene_engine_cli_smoke.py",
        WORKSPACE_ROOT / "scripts/tensor-wallpaper/scene_engine_runtime_smoke.py",
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
    print("checked: Tensor Wallpaper has no raw Vulkan escape/dependency or product-owned presentation lifecycle")
    print("checked: shared renderer owns offscreen and decoded-video terminal resources")
    print("checked: Tensor Wallpaper preserves the explicit shared-terminal descriptor ABI")
    print("checked: Vulkan scene path has no legacy descriptor-set binding tokens")
    print("checked: scene capability and effect co-development remains documented")
    return 0


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def manifest_dependency_names(manifest: dict[str, object]) -> set[str]:
    names: set[str] = set()

    def visit(table: dict[str, object]) -> None:
        for section_name in ("dependencies", "dev-dependencies", "build-dependencies"):
            section = table.get(section_name)
            if isinstance(section, dict):
                names.update(str(name) for name in section)
        targets = table.get("target")
        if isinstance(targets, dict):
            for target in targets.values():
                if isinstance(target, dict):
                    visit(target)

    visit(manifest)
    return names


def manifest_raw_feature_dependencies(manifest: dict[str, object]) -> set[str]:
    features = manifest.get("features")
    if not isinstance(features, dict):
        return set()
    forbidden = {"dep:ash", "dep:vulkanalia"}
    return {
        value
        for values in features.values()
        if isinstance(values, list)
        for value in values
        if isinstance(value, str) and value in forbidden
    }


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
