#!/usr/bin/env python3
"""Run a release, capture-disabled scene smoke and record pacing/memory evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from workspace_paths import GILDER_ROOT

ROOT = GILDER_ROOT
PSS_DIRTY_TARGET_KIB = 40 * 1024
PERF_SURFACE_WIDTH = 3840
PERF_SURFACE_HEIGHT = 2160
DGOP_PSS_DIRTY_KEYS = ("pssDirtyKB", "pss_dirty_kb", "pssDirtyKb")


def main() -> int:
    args = parse_args()
    with tempfile.TemporaryDirectory(prefix="gilder-scene-runtime-smoke-") as tmp:
        root = Path(tmp)
        if args.source:
            output = Path(args.source).expanduser().resolve()
            if not output.is_file():
                raise FileNotFoundError(f"scene source does not exist: {output}")
            source_mode = "existing-gscene"
        else:
            write_minimal_we_project(root)
            output = root / "out.gscene"
            run(
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
            source_mode = "generated-minimal-project"
        artifact_dir = (
            Path(args.artifact_dir)
            if args.artifact_dir
            else Path(tempfile.gettempdir())
            / f"gilder-scene-runtime-smoke-artifacts-{int(time.time())}-{os.getpid()}"
        )
        artifact_dir.mkdir(parents=True, exist_ok=True)

        if args.prebuilt_release_binary:
            binary = Path(args.prebuilt_release_binary).expanduser().resolve()
            if not binary.is_file():
                raise FileNotFoundError(f"prebuilt release binary does not exist: {binary}")
            if not os.access(binary, os.X_OK):
                raise PermissionError(f"prebuilt release binary is not executable: {binary}")
            binary_source = "prebuilt-release"
        else:
            run(
                [
                    "cargo",
                    "build",
                    "--release",
                    "--features",
                    "native-vulkan-renderer",
                    "--bin",
                    "gilder-native-vulkan",
                ]
            )
            binary = ROOT / "target/release/gilder-native-vulkan"
            binary_source = "current-release-build"
        binary_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
        dgop_pss_dirty_available = dgop_exposes_pss_dirty()
        runtime = subprocess.Popen(
            [
                str(binary),
                "--run-scene",
                "--source",
                str(output),
                "--duration",
                str(args.duration),
                "--no-fps-limit",
                "--surface-width",
                str(PERF_SURFACE_WIDTH),
                "--surface-height",
                str(PERF_SURFACE_HEIGHT),
            ]
            + (["--gpu-timing"] if args.gpu_timing else []),
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        samples = sample_process(
            runtime.pid,
            args.duration,
            args.interval,
            args.warmup,
            dgop_pss_dirty_available,
        )
        stdout, stderr = runtime.communicate(timeout=max(5, args.duration + 5))

        (artifact_dir / "scene-runtime.stdout.json").write_text(stdout, encoding="utf-8")
        (artifact_dir / "scene-runtime.stderr.txt").write_text(stderr, encoding="utf-8")
        (artifact_dir / "scene-runtime-samples.json").write_text(
            json.dumps(samples, indent=2) + "\n",
            encoding="utf-8",
        )
        if runtime.returncode != 0:
            print(stderr, file=sys.stderr)
            return runtime.returncode or 1

        report = json.loads(stdout)
        max_pss_dirty = max((sample["pss_dirty_kib"] for sample in samples), default=0)
        max_dgop_pss_dirty = max((sample["dgop_pss_dirty_kib"] for sample in samples), default=0)
        retained_samples = [
            sample for sample in samples if sample["elapsed_ms"] >= args.warmup * 1000
        ]
        if not retained_samples and samples:
            retained_samples = samples[-1:]
        max_retained_pss_dirty = max(
            (sample["pss_dirty_kib"] for sample in retained_samples), default=0
        )
        max_retained_dgop_pss_dirty = max(
            (sample["dgop_pss_dirty_kib"] for sample in retained_samples), default=0
        )
        max_observed_pss_dirty = max(max_pss_dirty, max_dgop_pss_dirty)
        max_retained_observed_pss_dirty = max(
            max_retained_pss_dirty, max_retained_dgop_pss_dirty
        )
        summary: dict[str, Any] = {
            "runtime_report": "scene-runtime.stdout.json",
            "samples": "scene-runtime-samples.json",
            "source": str(output),
            "source_mode": source_mode,
            "duration_seconds": args.duration,
            "warmup_seconds": args.warmup,
            "sampling_interval_seconds": args.interval,
            "startup_sampling_interval_seconds": min(args.interval, 0.05),
            "build_profile": "release",
            "renderer_binary": str(binary),
            "renderer_binary_source": binary_source,
            "renderer_binary_sha256": binary_sha256,
            "fps_limit": None,
            "frames_presented": report["frames_presented"],
            "average_present_fps": report["average_present_fps"],
            "present_delta_min_micros": report["present_delta_min_micros"],
            "present_delta_max_micros": report["present_delta_max_micros"],
            "present_delta_over_6250us_count": report["present_delta_over_6250us_count"],
            "present_delta_over_8334us_count": report["present_delta_over_8334us_count"],
            "present_mode": report["present_mode"],
            "surface_extent": report["present"]["swapchain"]["extent"],
            "scene_color_rasterization_samples": report["present"].get(
                "scene_color_rasterization_samples", "1x"
            ),
            "uses_multisampled_render_to_single_sampled": report["present"].get(
                "uses_multisampled_render_to_single_sampled", False
            ),
            "uses_explicit_scene_color_msaa_resolve": report["present"].get(
                "uses_explicit_scene_color_msaa_resolve", False
            ),
            "scene_color_msaa_memory_bytes": report["present"].get(
                "scene_color_msaa_memory_bytes", 0
            ),
            "descriptor_model": report["descriptor_model"],
            "render_graph_draw_count": report["render_graph_draw_count"],
            "mesh_draw_count": report["mesh_draw_count"],
            "frame_slot_count": report["present"].get("frame_slot_count", 1),
            "scene_color_mesh_draw_count": report["present"].get(
                "scene_color_mesh_draw_count", 0
            ),
            "scene_color_recorded_mesh_draw_count": report["present"].get(
                "scene_color_recorded_mesh_draw_count", 0
            ),
            "scene_color_attachment_clear_draw_count": report["present"].get(
                "scene_color_attachment_clear_draw_count", 0
            ),
            "scene_color_attachment_clear_frame_count": report["present"].get(
                "scene_color_attachment_clear_frame_count", 0
            ),
            "released_resource_payload_bytes": report["present"][
                "released_resource_payload_bytes"
            ],
            "released_texture_payload_bytes": report["present"][
                "released_texture_payload_bytes"
            ],
            "released_mesh_vertex_payload_bytes": report["present"][
                "released_mesh_vertex_payload_bytes"
            ],
            "released_mesh_index_payload_bytes": report["present"][
                "released_mesh_index_payload_bytes"
            ],
            "mesh_draw_recording_ready": report["mesh_draw_recording_ready"],
            "mesh_draw_recorded_this_run": report["mesh_draw_recorded_this_run"],
            "runtime_status": report["runtime_status"],
            "composite_scissor_draw_count": report["present"].get(
                "composite_scissor_draw_count", 0
            ),
            "composite_scissor_covered_pixels": report["present"].get(
                "composite_scissor_covered_pixels", 0
            ),
            "composite_scissor_avoided_pixels": report["present"].get(
                "composite_scissor_avoided_pixels", 0
            ),
            "frame_state_update_total_micros": report["present"].get(
                "frame_state_update_total_micros", 0
            ),
            "semantic_incremental_resolve_enabled": report["present"].get(
                "semantic_incremental_resolve_enabled", False
            ),
            "semantic_retained_puppet_resolve_enabled": report["present"].get(
                "semantic_retained_puppet_resolve_enabled", False
            ),
            "semantic_dynamic_entity_count": report["present"].get(
                "semantic_dynamic_entity_count", 0
            ),
            "semantic_resolve_total_micros": report["present"].get(
                "semantic_resolve_total_micros", 0
            ),
            "graph_update_total_micros": report["present"].get(
                "graph_update_total_micros", 0
            ),
            "transform_update_total_micros": report["present"].get(
                "transform_update_total_micros", 0
            ),
            "material_update_total_micros": report["present"].get(
                "material_update_total_micros", 0
            ),
            "skinning_update_total_micros": report["present"].get(
                "skinning_update_total_micros", 0
            ),
            "draw_policy_update_total_micros": report["present"].get(
                "draw_policy_update_total_micros", 0
            ),
            "sampled_descriptor_update_total_micros": report["present"].get(
                "sampled_descriptor_update_total_micros", 0
            ),
            "sampled_descriptor_update_count": report["present"].get(
                "sampled_descriptor_update_count", 0
            ),
            "command_recording_total_micros": report["present"].get(
                "command_recording_total_micros", 0
            ),
            "gpu_timing": report["present"].get("gpu_timing"),
            "max_pss_dirty_kib": max_pss_dirty,
            "max_dgop_pss_dirty_kib": max_dgop_pss_dirty,
            "max_observed_pss_dirty_kib": max_observed_pss_dirty,
            "max_retained_pss_dirty_kib": max_retained_pss_dirty,
            "max_retained_dgop_pss_dirty_kib": max_retained_dgop_pss_dirty,
            "max_retained_observed_pss_dirty_kib": max_retained_observed_pss_dirty,
            "dgop_pss_dirty_available": dgop_pss_dirty_available,
            "pss_dirty_measurement_source": (
                "dgop+smaps_rollup"
                if dgop_pss_dirty_available
                else "smaps_rollup-fallback"
            ),
            "pss_dirty_target_kib": PSS_DIRTY_TARGET_KIB,
            "observed_peak_pss_dirty_target_met": max_observed_pss_dirty
            <= PSS_DIRTY_TARGET_KIB,
            "pss_dirty_target_scope": "retained-after-warmup",
            "pss_dirty_target_met": max_retained_observed_pss_dirty
            <= PSS_DIRTY_TARGET_KIB,
        }
        summary_path = artifact_dir / "scene-runtime-summary.json"
        summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")

        if report["present_mode"] != "fifo-latest-ready":
            raise AssertionError(report["present_mode"])
        if summary["surface_extent"] != [PERF_SURFACE_WIDTH, PERF_SURFACE_HEIGHT]:
            raise AssertionError(summary["surface_extent"])
        if report["frames_presented"] <= 0:
            raise AssertionError(report["frames_presented"])
        if report["descriptor_model"] != "VK_EXT_descriptor_heap":
            raise AssertionError(report["descriptor_model"])
        if not report["mesh_draw_recording_ready"]:
            raise AssertionError(report["runtime_status"])
        if not report["mesh_draw_recorded_this_run"]:
            raise AssertionError(report["runtime_status"])
        if not summary["pss_dirty_target_met"]:
            raise AssertionError(
                "retained Pss_Dirty exceeded target: "
                f"{summary['max_retained_observed_pss_dirty_kib']} "
                f"> {PSS_DIRTY_TARGET_KIB} KiB"
            )

    print(f"scene-engine-runtime-smoke: ok summary={summary_path}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--warmup", type=float, default=1.0)
    parser.add_argument("--artifact-dir", default="")
    parser.add_argument("--source", default="", help="existing .gscene to measure")
    parser.add_argument(
        "--prebuilt-release-binary",
        default="",
        help="run this explicit executable without rebuilding the renderer",
    )
    parser.add_argument(
        "--gpu-timing",
        action="store_true",
        help="enable optional Vulkan scene GPU timestamp queries",
    )
    args = parser.parse_args()
    if args.duration <= 0 or args.interval <= 0:
        parser.error("--duration and --interval must be positive")
    if args.warmup < 0 or args.warmup >= args.duration:
        parser.error("--warmup must be non-negative and less than --duration")
    return args


def write_minimal_we_project(root: Path) -> None:
    (root / "models").mkdir()
    (root / "materials").mkdir()
    (root / "project.json").write_text(
        '{"type":"scene","file":"scene.json","title":"Runtime Smoke"}',
        encoding="utf-8",
    )
    (root / "scene.json").write_text(
        '{"general":{"clearcolor":"0.04 0.02 0.07","orthogonalprojection":{"width":1920,"height":1080}},'
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


def sample_process(
    pid: int,
    duration: int,
    interval: float,
    warmup: float,
    dgop_pss_dirty_available: bool,
) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    started = time.monotonic()
    next_dgop_sample = 0.0
    last_dgop_pss_dirty = 0
    while time.monotonic() - started <= duration:
        if not Path(f"/proc/{pid}").exists():
            break
        elapsed = time.monotonic() - started
        pss_dirty = parse_smaps_rollup(pid).get("Pss_Dirty", 0)
        if dgop_pss_dirty_available and elapsed >= next_dgop_sample:
            last_dgop_pss_dirty = dgop_pss_dirty(pid)
            next_dgop_sample = elapsed + max(interval, 1.0)
        sample = {
            "elapsed_ms": int(elapsed * 1000),
            "pss_dirty_kib": pss_dirty,
            "dgop_pss_dirty_kib": last_dgop_pss_dirty,
        }
        samples.append(sample)
        sample_interval = min(interval, 0.05) if elapsed < warmup else interval
        time.sleep(sample_interval)
    return samples


def parse_smaps_rollup(pid: int) -> dict[str, int]:
    values: dict[str, int] = {}
    try:
        for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines():
            if ":" not in line:
                continue
            key, rest = line.split(":", 1)
            parts = rest.strip().split()
            if parts and parts[0].isdigit():
                values[key] = int(parts[0])
    except OSError:
        pass
    return values


def dgop_pss_dirty(pid: int) -> int:
    payload = dgop_process_payload()
    if payload is None:
        return 0
    for process in payload.get("processes") or []:
        if int(process.get("pid") or 0) != pid:
            continue
        return int(next((process[key] for key in DGOP_PSS_DIRTY_KEYS if process.get(key)), 0))
    return 0


def dgop_exposes_pss_dirty() -> bool:
    payload = dgop_process_payload()
    if payload is None:
        return False
    return any(
        any(key in process for key in DGOP_PSS_DIRTY_KEYS)
        for process in payload.get("processes") or []
    )


def dgop_process_payload() -> dict[str, Any] | None:
    if not shutil.which("dgop"):
        return None
    result = subprocess.run(
        ["dgop", "processes", "--json", "--limit", "0", "--sort", "memory", "--no-cpu"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=3,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return None


if __name__ == "__main__":
    raise SystemExit(main())
