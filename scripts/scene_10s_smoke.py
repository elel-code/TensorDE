#!/usr/bin/env python3
"""Run a 10s native Vulkan scene smoke and collect FPS/Pss_Dirty evidence."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


DEFAULT_PREFIX = "/tmp/gilder-foliage-gain-10s"
DEFAULT_BINARY = "target/release/gilder-native-vulkan"
DEFAULT_SOURCE = "/tmp/gilder-we-3742497499-quality-release/assets/scene.gscn"
DEFAULT_SCENE_ROOT = "/tmp/gilder-we-3742497499-quality-release"


def main() -> int:
    args = parse_args()
    prefix = Path(args.prefix)
    paths = output_paths(prefix)
    clean_outputs(paths)

    env = os.environ.copy()
    env.setdefault("XDG_RUNTIME_DIR", "/run/user/1000")
    env.setdefault("WAYLAND_DISPLAY", "wayland-1")

    cmd = [
        args.binary,
        "--run-scene",
        "--output-name",
        args.output_name,
        "--source",
        args.source,
        "--scene-root",
        args.scene_root,
        "--scene-time-ms",
        str(args.scene_time_ms),
        "--duration",
        str(args.duration),
        "--no-fps-limit",
    ]

    started_monotonic = time.monotonic()
    samples: list[dict[str, Any]] = []
    captured_smaps = False
    status = 1
    timed_out = False

    with paths["json"].open("w") as stdout, paths["stderr"].open("w") as stderr:
        process = subprocess.Popen(cmd, stdout=stdout, stderr=stderr, env=env)
        try:
            while process.poll() is None:
                elapsed = time.monotonic() - started_monotonic
                elapsed_ms = int(elapsed * 1000)
                rollup = read_smaps_rollup(process.pid)
                dgop = sample_dgop(process.pid, sample_cpu=args.sample_cpu) if args.dgop else None
                sample = {
                    "timestamp_ms": int(time.time() * 1000),
                    "elapsed_ms": elapsed_ms,
                    "pid": process.pid,
                    "pss_dirty_kib": rollup.get("Pss_Dirty", 0),
                    "rss_kib": rollup.get("Rss", 0),
                    "pss_kib": rollup.get("Pss", 0),
                    "private_dirty_kib": rollup.get("Private_Dirty", 0),
                }
                if dgop:
                    sample["dgop"] = dgop
                samples.append(sample)
                append_jsonl(paths["dgop_jsonl"], sample)

                if not captured_smaps and elapsed_ms >= args.smaps_at_ms:
                    write_smaps_rollup(process.pid, paths["smaps_total"])
                    write_smaps_report(process.pid, paths["smaps_report"])
                    captured_smaps = True

                if elapsed > args.duration + args.timeout_extra:
                    timed_out = True
                    process.terminate()
                    break

                time.sleep(max(0.01, args.sample_interval))
            if timed_out:
                try:
                    status = process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    status = process.wait()
            else:
                status = process.wait()
        finally:
            if process.poll() is None:
                process.kill()
                status = process.wait()

    if not captured_smaps:
        write_smaps_rollup(process.pid, paths["smaps_total"])
        write_smaps_report(process.pid, paths["smaps_report"])

    runtime = read_runtime_json(paths["json"])
    summary = build_summary(
        args=args,
        cmd=cmd,
        env=env,
        status=status,
        timed_out=timed_out,
        samples=samples,
        runtime=runtime,
        paths=paths,
    )
    paths["summary"].write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print_summary(summary)
    return status


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run native Vulkan scene for 10s and collect FPS/Pss_Dirty evidence."
    )
    parser.add_argument("--prefix", default=DEFAULT_PREFIX)
    parser.add_argument("--binary", default=DEFAULT_BINARY)
    parser.add_argument("--source", default=DEFAULT_SOURCE)
    parser.add_argument("--scene-root", default=DEFAULT_SCENE_ROOT)
    parser.add_argument("--output-name", default="HDMI-A-1")
    parser.add_argument("--scene-time-ms", type=int, default=12500)
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument("--sample-interval", type=float, default=0.2)
    parser.add_argument("--smaps-at-ms", type=int, default=3000)
    parser.add_argument("--timeout-extra", type=float, default=5.0)
    parser.add_argument("--no-dgop", dest="dgop", action="store_false")
    parser.add_argument("--sample-cpu", action="store_true")
    parser.set_defaults(dgop=True)
    return parser.parse_args()


def output_paths(prefix: Path) -> dict[str, Path]:
    return {
        "json": prefix.with_suffix(".json"),
        "stderr": prefix.with_suffix(".stderr"),
        "dgop_jsonl": prefix.with_name(prefix.name + "-dgop.jsonl"),
        "summary": prefix.with_name(prefix.name + "-summary.json"),
        "smaps_total": prefix.with_name(prefix.name + "-smaps-total.txt"),
        "smaps_report": prefix.with_name(prefix.name + "-smaps-report.txt"),
    }


def clean_outputs(paths: dict[str, Path]) -> None:
    for path in paths.values():
        try:
            path.unlink()
        except FileNotFoundError:
            pass


def append_jsonl(path: Path, payload: dict[str, Any]) -> None:
    with path.open("a") as file:
        file.write(json.dumps(payload, sort_keys=True) + "\n")


def read_smaps_rollup(pid: int) -> dict[str, int]:
    return parse_smaps_rollup(Path(f"/proc/{pid}/smaps_rollup"))


def parse_smaps_rollup(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    try:
        with path.open() as file:
            for line in file:
                match = re.match(r"^([A-Za-z_]+):\s+(\d+)\s+kB$", line.strip())
                if match:
                    values[match.group(1)] = int(match.group(2))
    except OSError:
        return {}
    return values


def write_smaps_rollup(pid: int, path: Path) -> None:
    values = read_smaps_rollup(pid)
    lines = [
        f"Rss {values.get('Rss', 0)} KiB",
        f"Pss {values.get('Pss', 0)} KiB",
        f"Pss_Dirty {values.get('Pss_Dirty', 0)} KiB",
        f"Private_Dirty {values.get('Private_Dirty', 0)} KiB",
    ]
    path.write_text("\n".join(lines) + "\n")


def write_smaps_report(pid: int, path: Path) -> None:
    smaps = Path(f"/proc/{pid}/smaps")
    entries: dict[str, dict[str, int]] = {}
    current = ""
    try:
        with smaps.open(errors="replace") as file:
            for line in file:
                if re.match(r"^[0-9a-fA-F]+-[0-9a-fA-F]+", line):
                    current = smaps_entry_name(line)
                    entries.setdefault(current, {"pss_dirty": 0, "private_dirty": 0})
                elif current and line.startswith("Pss_Dirty:"):
                    entries[current]["pss_dirty"] += first_int(line)
                elif current and line.startswith("Private_Dirty:"):
                    entries[current]["private_dirty"] += first_int(line)
    except OSError:
        path.write_text("")
        return

    lines = []
    for name, values in sorted(
        entries.items(), key=lambda item: item[1]["pss_dirty"], reverse=True
    ):
        if values["pss_dirty"] or values["private_dirty"]:
            lines.append(f"{values['pss_dirty']}\t{values['private_dirty']}\t{name}")
    path.write_text("\n".join(lines) + ("\n" if lines else ""))


def smaps_entry_name(line: str) -> str:
    parts = line.strip().split(maxsplit=5)
    if len(parts) >= 6:
        return parts[5]
    return "[anon]"


def first_int(line: str) -> int:
    match = re.search(r"(\d+)", line)
    return int(match.group(1)) if match else 0


def sample_dgop(pid: int, sample_cpu: bool) -> dict[str, Any] | None:
    if not shutil.which("dgop"):
        return None
    cmd = ["dgop", "processes", "--json", "--limit", "0", "--sort", "memory"]
    if not sample_cpu:
        cmd.append("--no-cpu")
    try:
        result = subprocess.run(
            cmd,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=3,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    try:
        payload = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return None
    for process in payload.get("processes") or []:
        if process.get("pid") == pid:
            return process
    return None


def read_runtime_json(path: Path) -> dict[str, Any]:
    try:
        text = path.read_text()
    except OSError:
        return {}
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start >= 0 and end > start:
            try:
                return json.loads(text[start : end + 1])
            except json.JSONDecodeError:
                return {}
    return {}


def build_summary(
    *,
    args: argparse.Namespace,
    cmd: list[str],
    env: dict[str, str],
    status: int,
    timed_out: bool,
    samples: list[dict[str, Any]],
    runtime: dict[str, Any],
    paths: dict[str, Path],
) -> dict[str, Any]:
    present = get_path(runtime, ["snapshot", "present"], {})
    runtime_snapshot = get_path(runtime, ["snapshot", "runtime"], {})
    timing = get_path(present, ["present_loop_timing"], {})
    gpu_timestamp = get_path(timing, ["gpu_timestamp"], {})
    last_command = get_path(present, ["last_command"], {})

    pss_dirty_samples = [int(sample.get("pss_dirty_kib") or 0) for sample in samples]
    pss_dirty_samples = [value for value in pss_dirty_samples if value > 0]
    smaps_3s = parse_rollup_text(paths["smaps_total"])
    dgop_samples = [sample.get("dgop") for sample in samples if sample.get("dgop")]
    dgop_memory = [
        int(sample.get("memoryKB") or 0)
        for sample in dgop_samples
        if isinstance(sample, dict) and int(sample.get("memoryKB") or 0) > 0
    ]

    metrics = {
        "average_present_fps": get_number(present, ["average_present_fps"]),
        "frames_presented": get_number(present, ["frames_presented"]),
        "present_mode": get_path(present, ["swapchain", "present_mode"]),
        "command_record_micros": get_number(timing, ["command_record_micros"]),
        "queue_submit_micros": get_number(timing, ["queue_submit_micros"]),
        "queue_present_micros": get_number(timing, ["queue_present_micros"]),
        "gpu_timestamp_avg_micros": get_number(gpu_timestamp, ["avg_gpu_micros"]),
        "gpu_timestamp_frames": get_number(gpu_timestamp, ["frames_measured"]),
        "draw_call_count": get_number(last_command, ["draw_call_count"]),
        "pipeline_bind_count": get_number(last_command, ["pipeline_bind_count"]),
        "rendering_begin_count": get_number(last_command, ["rendering_begin_count"]),
        "target_switch_count": get_number(last_command, ["target_switch_count"]),
        "image_barrier_count": get_number(last_command, ["image_barrier_count"]),
        "pipeline_barrier_command_count": get_number(
            last_command, ["pipeline_barrier_command_count"]
        ),
        "framebuffer_snapshot_copy_count": get_number(
            last_command, ["framebuffer_snapshot_copy_count"]
        ),
        "ordered_effect_target_run_count": get_number(
            last_command, ["ordered_effect_target_run_count"]
        ),
        "repeated_effect_target_run_count": get_number(
            last_command, ["repeated_effect_target_run_count"]
        ),
        "we_graph_step_count": get_number(
            runtime_snapshot, ["draw_pass_sampled_image_we_graph_step_count"]
        ),
        "we_graph_target_count": get_number(
            runtime_snapshot, ["draw_pass_sampled_image_we_graph_target_count"]
        ),
        "we_graph_execution_pass_count": get_number(
            runtime_snapshot, ["draw_pass_sampled_image_we_graph_execution_pass_count"]
        ),
        "we_graph_base_material_step_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_base_material_step_count"],
        ),
        "source_direct_chain_start_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_source_direct_chain_start_count"],
        ),
        "source_direct_chain_start_candidate_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_source_direct_chain_start_candidate_count"],
        ),
        "source_direct_chain_start_blocked_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_source_direct_chain_start_blocked_count"],
        ),
        "source_direct_chain_start_blocked_reason_counts": get_path(
            runtime_snapshot,
            [
                "draw_pass_sampled_image_we_graph_source_direct_chain_start_blocked_reason_counts"
            ],
            {},
        ),
        "waterwaves_fused2_step_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_waterwaves_fused2_step_count"],
        ),
        "waterwaves_fused2_step_eliminated_count": get_number(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_waterwaves_fused2_step_eliminated_count"],
        ),
        "waterwaves_lowering_blocked_reason_counts": get_path(
            runtime_snapshot,
            ["draw_pass_sampled_image_we_graph_waterwaves_lowering_blocked_reason_counts"],
            {},
        ),
        "waterwaves_lowering_blocked_triple_reason_counts": get_path(
            runtime_snapshot,
            [
                "draw_pass_sampled_image_we_graph_waterwaves_lowering_blocked_triple_reason_counts"
            ],
            {},
        ),
        "pipeline_set_count": get_number(
            present, ["pipeline", "sampled_image_pipeline_set_count"]
        ),
        "graphics_pipeline_count": get_number(
            present, ["pipeline", "sampled_image_graphics_pipeline_count"]
        ),
        "pass_specific_fragment_pipeline_count": get_number(
            present, ["pipeline", "pass_specific_fragment_pipeline_count"]
        ),
        "effect_target_count": get_number(present, ["geometry", "effect_target_count"]),
    }
    return {
        "status": status,
        "timed_out": timed_out,
        "command": cmd,
        "env": {
            "XDG_RUNTIME_DIR": env.get("XDG_RUNTIME_DIR"),
            "WAYLAND_DISPLAY": env.get("WAYLAND_DISPLAY"),
        },
        "duration_seconds": args.duration,
        "sample_interval_seconds": args.sample_interval,
        "metrics": metrics,
        "memory": {
            "last_pss_dirty_kib": pss_dirty_samples[-1] if pss_dirty_samples else 0,
            "max_pss_dirty_kib": max(pss_dirty_samples, default=0),
            "smaps_at_ms": args.smaps_at_ms,
            "smaps_rollup": smaps_3s,
            "dgop_memory_calculation": (
                dgop_samples[-1].get("memoryCalculation")
                if dgop_samples and isinstance(dgop_samples[-1], dict)
                else None
            ),
            "dgop_last_memory_kib": dgop_memory[-1] if dgop_memory else 0,
            "dgop_max_memory_kib": max(dgop_memory, default=0),
        },
        "paths": {key: str(path) for key, path in paths.items()},
    }


def parse_rollup_text(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    try:
        with path.open() as file:
            for line in file:
                parts = line.split()
                if len(parts) >= 2:
                    values[parts[0]] = int(parts[1])
    except (OSError, ValueError):
        return {}
    return values


def get_path(root: Any, path: list[str], default: Any = None) -> Any:
    value = root
    for key in path:
        if not isinstance(value, dict) or key not in value:
            return default
        value = value[key]
    return value


def get_number(root: Any, path: list[str]) -> int | float | None:
    value = get_path(root, path)
    if isinstance(value, (int, float)):
        return value
    return None


def print_summary(summary: dict[str, Any]) -> None:
    metrics = summary["metrics"]
    memory = summary["memory"]
    paths = summary["paths"]
    print(f"status={summary['status']} timed_out={summary['timed_out']}")
    print("command=" + " ".join(summary["command"]))
    print(
        "fps={fps} frames={frames} present_mode={mode} gpu_avg_us={gpu}".format(
            fps=metrics.get("average_present_fps"),
            frames=metrics.get("frames_presented"),
            mode=metrics.get("present_mode"),
            gpu=metrics.get("gpu_timestamp_avg_micros"),
        )
    )
    print(
        "draws={draws} begins={begins} switches={switches} image_barriers={barriers} pipeline_barriers={pipe_barriers}".format(
            draws=metrics.get("draw_call_count"),
            begins=metrics.get("rendering_begin_count"),
            switches=metrics.get("target_switch_count"),
            barriers=metrics.get("image_barrier_count"),
            pipe_barriers=metrics.get("pipeline_barrier_command_count"),
        )
    )
    print(
        "we_steps={steps} targets={targets} base_steps={base} source_direct={direct}/{candidate} blocked={blocked}".format(
            steps=metrics.get("we_graph_step_count"),
            targets=metrics.get("we_graph_target_count"),
            base=metrics.get("we_graph_base_material_step_count"),
            direct=metrics.get("source_direct_chain_start_count"),
            candidate=metrics.get("source_direct_chain_start_candidate_count"),
            blocked=metrics.get("source_direct_chain_start_blocked_count"),
        )
    )
    print(
        "pss_dirty_last_kib={last} pss_dirty_max_kib={maxv} smaps_3s_pss_dirty_kib={smaps}".format(
            last=memory.get("last_pss_dirty_kib"),
            maxv=memory.get("max_pss_dirty_kib"),
            smaps=memory.get("smaps_rollup", {}).get("Pss_Dirty"),
        )
    )
    print(
        "json={json} summary={summary} smaps={smaps}".format(
            json=paths["json"],
            summary=paths["summary"],
            smaps=paths["smaps_total"],
        )
    )


if __name__ == "__main__":
    sys.exit(main())
