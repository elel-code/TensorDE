#!/usr/bin/env python3
"""Smoke the new scene-engine CLI bridge without creating a Wayland surface."""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="gilder-scene-cli-smoke-") as tmp:
        root = Path(tmp)
        write_minimal_we_project(root)
        output = root / "out.gscene"

        convert = run(
            [
                "cargo",
                "run",
                "--bin",
                "gilder-convert",
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
                "native-vulkan-renderer",
                "--bin",
                "gilder-native-vulkan",
                "--",
                "--scene-backend-plan",
                "--source",
                str(output),
            ]
        )
        report = json.loads(plan.stdout)
        assert report["scene_backend_plan_report_version"] == 1
        assert "we/genericimage4" in report["scene_strings"]
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
        '{"passes":[{"shader":"genericimage4","textures":[null]}]}',
        encoding="utf-8",
    )


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
