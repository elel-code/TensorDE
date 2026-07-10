#!/usr/bin/env python3
"""Run a release, capture-disabled scene smoke and record pacing/memory evidence."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
PSS_DIRTY_TARGET_KIB = 40 * 1024


def main() -> int:
    args = parse_args()
    with tempfile.TemporaryDirectory(prefix="gilder-scene-runtime-smoke-") as tmp:
        root = Path(tmp)
        write_minimal_we_project(root)
        output = root / "out.gscene"
        artifact_dir = (
            Path(args.artifact_dir)
            if args.artifact_dir
            else Path(tempfile.gettempdir())
            / f"gilder-scene-runtime-smoke-artifacts-{int(time.time())}-{os.getpid()}"
        )
        artifact_dir.mkdir(parents=True, exist_ok=True)

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
        runtime = subprocess.Popen(
            [
                str(binary),
                "--run-scene",
                "--source",
                str(output),
                "--duration",
                str(args.duration),
                "--no-fps-limit",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        samples = sample_process(runtime.pid, args.duration, args.interval)
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
        summary: dict[str, Any] = {
            "runtime_report": "scene-runtime.stdout.json",
            "samples": "scene-runtime-samples.json",
            "duration_seconds": args.duration,
            "build_profile": "release",
            "frame_capture": report.get("frame_capture"),
            "frames_presented": report["frames_presented"],
            "average_present_fps": report["average_present_fps"],
            "present_delta_min_micros": report["present_delta_min_micros"],
            "present_delta_max_micros": report["present_delta_max_micros"],
            "present_delta_over_6250us_count": report["present_delta_over_6250us_count"],
            "present_delta_over_8334us_count": report["present_delta_over_8334us_count"],
            "present_mode": report["present_mode"],
            "descriptor_model": report["descriptor_model"],
            "render_graph_draw_count": report["render_graph_draw_count"],
            "mesh_draw_count": report["mesh_draw_count"],
            "mesh_draw_recording_ready": report["mesh_draw_recording_ready"],
            "mesh_draw_recorded_this_run": report["mesh_draw_recorded_this_run"],
            "runtime_status": report["runtime_status"],
            "max_pss_dirty_kib": max_pss_dirty,
            "max_dgop_pss_dirty_kib": max_dgop_pss_dirty,
            "pss_dirty_target_kib": PSS_DIRTY_TARGET_KIB,
            "pss_dirty_target_met": max(max_pss_dirty, max_dgop_pss_dirty) <= PSS_DIRTY_TARGET_KIB,
        }
        summary_path = artifact_dir / "scene-runtime-summary.json"
        summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")

        if report["present_mode"] != "fifo-latest-ready":
            raise AssertionError(report["present_mode"])
        if report.get("frame_capture") is not None:
            raise AssertionError("performance smoke must run with frame capture disabled")
        if report["frames_presented"] <= 0:
            raise AssertionError(report["frames_presented"])
        if report["descriptor_model"] != "VK_EXT_descriptor_heap":
            raise AssertionError(report["descriptor_model"])
        if not report["mesh_draw_recording_ready"]:
            raise AssertionError(report["runtime_status"])
        if not report["mesh_draw_recorded_this_run"]:
            raise AssertionError(report["runtime_status"])

    print(f"scene-engine-runtime-smoke: ok summary={summary_path}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--artifact-dir", default="")
    return parser.parse_args()


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


def sample_process(pid: int, duration: int, interval: float) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    started = time.monotonic()
    while time.monotonic() - started <= duration:
        if not Path(f"/proc/{pid}").exists():
            break
        sample = {
            "elapsed_ms": int((time.monotonic() - started) * 1000),
            "pss_dirty_kib": parse_smaps_rollup(pid).get("Pss_Dirty", 0),
            "dgop_pss_dirty_kib": dgop_pss_dirty(pid),
        }
        samples.append(sample)
        time.sleep(interval)
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
    if not shutil.which("dgop"):
        return 0
    result = subprocess.run(
        ["dgop", "processes", "--json", "--limit", "0", "--sort", "memory", "--no-cpu"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=3,
    )
    if result.returncode != 0:
        return 0
    try:
        payload = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return 0
    for process in payload.get("processes") or []:
        if int(process.get("pid") or 0) != pid:
            continue
        return int(
            process.get("pssDirtyKB")
            or process.get("pss_dirty_kb")
            or process.get("pssDirtyKb")
            or 0
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
