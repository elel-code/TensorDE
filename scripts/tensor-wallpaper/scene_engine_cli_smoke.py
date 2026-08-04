#!/usr/bin/env python3
"""Smoke the new scene-engine CLI bridge without creating a Wayland surface."""

from __future__ import annotations

import json
import struct
import subprocess
import tempfile
from pathlib import Path

from workspace_paths import TENSOR_WALLPAPER_ROOT

ROOT = TENSOR_WALLPAPER_ROOT


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="tensor-wallpaper-scene-cli-smoke-") as tmp:
        root = Path(tmp)
        write_minimal_we_project(root)
        output = root / "out.gscene"

        convert = run(
            [
                "cargo",
                "run",
                "--bin",
                "tensor-wallpaper-convert",
                "--",
                "wallpaper-engine",
                str(root),
                str(output),
            ]
        )
        if "meshes=1 vertices=4 indices=6" not in convert.stdout:
            raise AssertionError(convert.stdout)
        if "fifo_latest_ready=true" not in convert.stdout:
            raise AssertionError(convert.stdout)

        plan = run(
            [
                "cargo",
                "run",
                "--features",
                "rendering-device",
                "--bin",
                "tensor-wallpaper",
                "--",
                "--scene-execution-plan",
                "--source",
                str(output),
            ]
        )
        report = json.loads(plan.stdout)
        assert report["scene_execution_plan_report_version"] == 14
        assert "we/genericimage4" in report["scene_strings"]
        assert len(report["scene_objects"]) == 1
        scene_object = report["scene_objects"][0]
        assert scene_object["id"] == 0
        assert scene_object["render_graph"] == 0
        scene_resource = next(
            resource
            for resource in report["scene_resources"]
            if resource["id"] == scene_object["resource"]
        )
        material_bindings = report["rendering_device_graph"][
            "material_sampled_bindings"
        ]
        assert len(material_bindings) == 1
        sampled_binding = material_bindings[0]
        assert len(report["scene_textures"]) == 1
        scene_texture = report["scene_textures"][0]
        sampled_resource = next(
            resource
            for resource in report["scene_resources"]
            if resource["id"] == sampled_binding["resource"]
        )
        assert scene_resource["kind"] == "model-json"
        assert sampled_binding["draw_index"] == 0
        assert sampled_resource["kind"] == "texture-tex"
        assert scene_texture["resource"] == sampled_binding["resource"]
        assert scene_texture["width"] > 0 and scene_texture["height"] > 0
        assert len(scene_texture["alpha_coverage_rows"]) == 32
        assert report["scene_render_passes"][0]["pipeline_blend"] == "normal"
        assert report["scene_render_passes"][0]["color_write_mask"] == "rgba"
        assert report["present_mode"] == "fifo-latest-ready"
        assert report["descriptor_heap_only"] is True
        assert report["renderer_scene_render"]["mesh_count"] == 1
        assert report["renderer_scene_render"]["mesh_vertex_count"] == 4
        assert report["renderer_scene_render"]["mesh_index_count"] == 6
        assert report["descriptor_heap"]["resource_descriptor_count"] == 3
        assert report["descriptor_heap"]["sampled_image_descriptor_count"] == 1
        assert report["descriptor_heap"]["uniform_buffer_descriptor_count"] == 2
        assert report["descriptor_heap"]["sampler_descriptor_count"] == 1
        assert report["resource_storage"]["mesh_buffer"]["draw_count"] == 1
        assert report["pipeline_cache"]["shader_catalog_hit_count"] == 1
        assert report["pipeline_cache"]["missing_shader_keys"] == []
        assert report["pipeline_cache"]["shader_program_count"] == 1
        pipeline_entry = report["pipeline_cache"]["entries"][0]
        assert "vertex_spirv_bytes" not in pipeline_entry
        assert "fragment_spirv_bytes" not in pipeline_entry
        shader_programs = report["pipeline_cache"]["shader_programs"][0]
        assert shader_programs["shader_catalog_key"] == "we/genericimage4"
        vertex_program = shader_programs["vertex_programs"][0]
        assert vertex_program["primitive"] == "object-mesh"
        assert vertex_program["spirv_bytes"] == len(vertex_program["spirv_words"]) * 4
        assert vertex_program["spirv_words"][0] == 0x07230203
        fragment_program = shader_programs["base_fragment_program"]
        assert fragment_program["spirv_bytes"] == len(fragment_program["spirv_words"]) * 4
        assert shader_programs["local_read_fragment_program"] is None
        assert report["checkpoint_scene_time_seconds"] == 0.0
        assert report["checkpoint_draw_visibility"] == [
            {
                "draw_index": 0,
                "object": 0,
                "resolved_object_index": 0,
                "object_resolved_visible": True,
                "object_visibility_allows_draw": True,
            }
        ]
        assert report["render_graph_executor"]["draw_count"] == 1
        assert (
            report["render_graph_executor"]["executor_status"]
            == "scene-render-graph-ready-for-vulkan-recording"
        )

    print("scene-engine-cli-smoke: ok")
    return 0


def write_minimal_we_project(root: Path) -> None:
    (root / "models").mkdir()
    (root / "materials").mkdir()
    (root / "textures").mkdir()
    (root / "project.json").write_text(
        '{"type":"scene","file":"scene.json","title":"CLI Smoke"}',
        encoding="utf-8",
    )
    (root / "scene.json").write_text(
        '{"general":{"orthogonalprojection":{"width":1920,"height":1080}},'
        '"objects":[{"id":7,"name":"layer","image":"models/layer.json"}]}',
        encoding="utf-8",
    )
    (root / "models/layer.json").write_text(
        '{"width":64,"height":64,"material":"materials/layer.json"}',
        encoding="utf-8",
    )
    (root / "materials/layer.json").write_text(
        '{"passes":[{"shader":"genericimage4","textures":["textures/layer.tex"]}]}',
        encoding="utf-8",
    )
    write_rgba_tex(root / "textures/layer.tex")


def write_rgba_tex(path: Path) -> None:
    width = 4
    height = 4
    pixels = bytes([255, 255, 255, 255] * width * height)
    values = [0, 2, width, height, width, height]
    data = bytearray(b"TEXV0005\0TEXI0001\0")
    data.extend(struct.pack("<6I", *values))
    data.extend(struct.pack("<HBB", 0, 1, 0xFF))
    data.extend(b"TEXB0004\0")
    data.extend(struct.pack("<6I", 1, 0xFFFFFFFF, 0, 1, width, height))
    data.extend(struct.pack("<3I", 0, len(pixels), len(pixels)))
    data.extend(pixels)
    path.write_bytes(data)


def run(args: list[str]) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        args,
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            "command failed: "
            + " ".join(args)
            + f"\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


if __name__ == "__main__":
    raise SystemExit(main())
