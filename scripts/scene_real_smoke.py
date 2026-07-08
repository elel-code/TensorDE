#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
"""Run a real native Vulkan scene smoke and collect dgop memory evidence."""

from __future__ import annotations

import argparse
import csv
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


DEFAULT_SOURCE = "artifacts/eye-wallpaper-verify/3742497499.gwpdir/assets/scene.gscn"
DEFAULT_SHADER_ROOT = "artifacts/scene-shaders"
DEFAULT_BINARY = "target/release/gilder-native-vulkan"
DEFAULT_FEATURES = "native-vulkan-renderer,native-vulkan-video"
DGOP_COLUMNS = [
    "sample",
    "elapsed_ms",
    "pid",
    "memory_kb",
    "memory_calculation",
    "rss_kb",
    "pss_kb",
    "pss_dirty_kb",
    "anonymous_kb",
    "cpu_percent",
    "pticks",
    "command",
]


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    os.chdir(repo_root)

    source = resolve_existing_file(repo_root, args.source, "scene source")
    shader_root = resolve_path(repo_root, args.shader_artifact_root)
    binary = resolve_binary(repo_root, args.binary)
    if not args.no_build:
        build_binary(args, binary)
    elif not binary.is_file():
        raise SystemExit(f"FAIL: binary missing: {binary}")

    out_dir = Path(args.output_dir) if args.output_dir else (
        Path(args.work_dir) / f"gilder-scene-real-smoke-{args.label}-{int(time.time())}-{os.getpid()}"
    )
    out_dir.mkdir(parents=True, exist_ok=True)

    metadata = {
        "label": args.label,
        "source": str(source),
        "binary": str(binary),
        "shader_artifact_root": str(shader_root),
        "duration_seconds": args.duration,
        "target_fps": args.target_fps if args.target_fps > 0 else "unlimited",
        "display": args.display,
        "dgop": shutil.which("dgop") or "",
        "references": [
            "docs/gilder-scene-engine-architecture.md",
            "reverse-engineered/docs/exe/blend-and-render.md",
            "reverse-engineered/docs/exe/d3d11-context-calls.md",
            "references/godot/servers/rendering/rendering_device_graph.h",
            "references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp",
        ],
    }
    write_json(out_dir / "metadata.json", metadata)
    capture_niri_json(out_dir, "before")

    result = run_scene_smoke(args, binary, source, shader_root, out_dir)
    capture_niri_json(out_dir, "after")

    summary = summarize(args, result, out_dir)
    write_json(out_dir / "summary.json", summary)
    print_summary(summary)
    return 1 if summary["failures"] else 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run --run-scene for a real 10s smoke and collect dgop pss_dirty."
    )
    parser.add_argument("--source", default=DEFAULT_SOURCE)
    parser.add_argument("--shader-artifact-root", default=DEFAULT_SHADER_ROOT)
    parser.add_argument("--binary", default=DEFAULT_BINARY)
    parser.add_argument("--features", default=DEFAULT_FEATURES)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--display", default=os.environ.get("WAYLAND_DISPLAY", "wayland-1"))
    parser.add_argument("--output-name", default="")
    parser.add_argument("--duration", type=int, default=10)
    parser.add_argument(
        "--target-fps",
        type=int,
        default=0,
        help="Positive value caps the scene runtime. Default 0 means --no-fps-limit.",
    )
    parser.add_argument("--no-fps-limit", action="store_true")
    parser.add_argument("--wait-roundtrips", type=int, default=2)
    parser.add_argument("--label", default="scene-mainline-10s")
    parser.add_argument("--work-dir", default=os.environ.get("TMPDIR", "/tmp"))
    parser.add_argument("--output-dir", default="")
    parser.add_argument("--sample-interval", type=float, default=0.25)
    parser.add_argument("--sample-cpu", action="store_true")
    parser.add_argument("--allow-missing-dgop", action="store_true")
    parser.add_argument("--expect-min-average-present-fps", type=float, default=240.0)
    parser.add_argument("--expect-max-dgop-pss-dirty-kib", type=int, default=40 * 1024)
    parser.add_argument("--expect-present-mode", default="fifo-latest-ready")
    parser.add_argument("--no-expectations", action="store_true")
    parser.add_argument("--niri-screenshot", action="store_true")
    parser.add_argument("--niri-screenshot-at", type=float, default=5.0)
    parser.add_argument("--guard-seconds", type=float, default=20.0)
    args = parser.parse_args()
    if args.duration <= 0:
        parser.error("--duration must be positive")
    if args.no_fps_limit:
        args.target_fps = 0
    if args.target_fps < 0:
        parser.error("--target-fps must be positive, or 0 for no limit")
    if args.sample_interval <= 0:
        parser.error("--sample-interval must be positive")
    if args.no_expectations:
        args.expect_min_average_present_fps = 0.0
        args.expect_max_dgop_pss_dirty_kib = 0
        args.expect_present_mode = ""
    return args


def resolve_existing_file(repo_root: Path, value: str, label: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = repo_root / path
    path = path.resolve()
    if not path.is_file():
        raise SystemExit(f"FAIL: missing {label}: {path}")
    return path


def resolve_path(repo_root: Path, value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = repo_root / path
    return path.resolve()


def resolve_binary(repo_root: Path, value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = repo_root / path
    return path.resolve()


def build_binary(args: argparse.Namespace, binary: Path) -> None:
    cmd = [
        "cargo",
        "build",
        "--release",
        "--features",
        args.features,
        "--bin",
        "gilder-native-vulkan",
    ]
    result = subprocess.run(cmd, check=False)
    if result.returncode != 0:
        raise SystemExit(f"FAIL: cargo build failed with status {result.returncode}")
    if not binary.is_file():
        raise SystemExit(f"FAIL: expected binary missing after build: {binary}")


def run_scene_smoke(
    args: argparse.Namespace,
    binary: Path,
    source: Path,
    shader_root: Path,
    out_dir: Path,
) -> dict[str, Any]:
    stdout_path = out_dir / "scene-stdout.json"
    stderr_path = out_dir / "scene-stderr.txt"
    dgop_path = out_dir / "dgop.csv"
    rollup_path = out_dir / "smaps-rollup.txt"
    smaps_path = out_dir / "smaps.txt"
    peak_rollup_path = out_dir / "peak-smaps-rollup.txt"
    screenshot_path = out_dir / "niri-screen.png"

    cmd = [
        str(binary),
        "--run-scene",
        "--source",
        str(source),
        "--scene-shader-artifact-root",
        str(shader_root),
        "--duration",
        str(args.duration),
        "--wait-roundtrips",
        str(args.wait_roundtrips),
        "--layer",
        "bottom",
    ]
    if args.target_fps > 0:
        cmd.extend(["--target-fps", str(args.target_fps)])
    else:
        cmd.append("--no-fps-limit")
    if args.output_name:
        cmd.extend(["--output-name", args.output_name])

    env = os.environ.copy()
    env["WAYLAND_DISPLAY"] = args.display
    env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")

    samples: list[dict[str, Any]] = []
    with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr, dgop_path.open(
        "w", newline=""
    ) as dgop_file:
        writer = csv.DictWriter(dgop_file, fieldnames=DGOP_COLUMNS)
        writer.writeheader()
        started = time.monotonic()
        process = subprocess.Popen(cmd, stdout=stdout, stderr=stderr, env=env)
        dgop_cursor = ""
        sample_index = 0
        captured_smaps = False
        captured_screenshot = False
        peak_pss_dirty_kb = -1
        guard_deadline = started + args.duration + args.guard_seconds
        time.sleep(min(args.sample_interval, 0.25))
        while process.poll() is None:
            now = time.monotonic()
            elapsed_ms = int((now - started) * 1000)
            sample, dgop_cursor = sample_dgop(
                process.pid, binary.name, dgop_cursor, args.sample_cpu
            )
            if sample:
                sample.update({"sample": sample_index, "elapsed_ms": elapsed_ms, "pid": process.pid})
                samples.append(sample)
                writer.writerow(sample)
                dgop_file.flush()
                pss_dirty = int(sample.get("pss_dirty_kb") or 0)
                if pss_dirty > peak_pss_dirty_kb:
                    peak_pss_dirty_kb = pss_dirty
                    copy_proc_file(Path(f"/proc/{process.pid}/smaps_rollup"), peak_rollup_path, 200)
            if not captured_smaps and elapsed_ms >= 3000:
                copy_proc_file(Path(f"/proc/{process.pid}/smaps_rollup"), rollup_path, 200)
                copy_proc_file(Path(f"/proc/{process.pid}/smaps"), smaps_path, 20000)
                captured_smaps = True
            if (
                args.niri_screenshot
                and not captured_screenshot
                and now - started >= args.niri_screenshot_at
            ):
                capture_niri_screenshot(screenshot_path)
                captured_screenshot = True
            if now > guard_deadline:
                terminate_process(process)
                break
            sample_index += 1
            time.sleep(args.sample_interval)
        status = process.wait()

    telemetry = read_json(stdout_path)
    return {
        "cmd": cmd,
        "pid": process.pid,
        "status": status,
        "stdout": str(stdout_path),
        "stderr": str(stderr_path),
        "dgop": str(dgop_path),
        "smaps_rollup": str(rollup_path),
        "smaps": str(smaps_path),
        "peak_smaps_rollup": str(peak_rollup_path),
        "niri_screenshot": str(screenshot_path) if screenshot_path.exists() else "",
        "samples": samples,
        "telemetry": telemetry,
    }


def sample_dgop(
    pid: int, binary_name: str, cursor: str, sample_cpu: bool
) -> tuple[dict[str, Any] | None, str]:
    if not shutil.which("dgop"):
        return None, cursor
    cmd = ["dgop", "processes", "--json", "--limit", "0", "--sort", "memory"]
    if sample_cpu:
        if cursor:
            cmd.extend(["--cursor", cursor])
    else:
        cmd.append("--no-cpu")
    result = subprocess.run(
        cmd,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=3,
    )
    if result.returncode != 0:
        return None, cursor
    try:
        payload = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return None, cursor
    next_cursor = str(payload.get("cursor") or cursor)
    binary_suffix = "/" + binary_name
    for process in payload.get("processes") or []:
        executable = str(process.get("executablePath") or "")
        if int(process.get("pid") or 0) != pid and not executable.endswith(binary_suffix):
            continue
        sample = {
            "memory_kb": int_value(process.get("memoryKB")),
            "memory_calculation": str(process.get("memoryCalculation") or ""),
            "rss_kb": int_value(process.get("rssKB")),
            "pss_kb": int_value(process.get("pssKB")),
            "pss_dirty_kb": int_value(
                process.get("pssDirtyKB")
                or process.get("pss_dirty_kb")
                or process.get("pssDirtyKb")
            ),
            "anonymous_kb": int_value(
                process.get("anonymousKB")
                or process.get("anonymous_kb")
                or process.get("anonymousKb")
            ),
            "cpu_percent": float_value(process.get("cpu")) if sample_cpu else 0.0,
            "pticks": int_value(process.get("pticks")) if sample_cpu else 0,
            "command": str(process.get("command") or ""),
        }
        if (
            sample["pss_dirty_kb"] == 0
            and sample["memory_calculation"].lower() == "pss_dirty"
        ):
            sample["pss_dirty_kb"] = sample["memory_kb"]
        return sample, next_cursor
    return None, next_cursor


def summarize(args: argparse.Namespace, result: dict[str, Any], out_dir: Path) -> dict[str, Any]:
    telemetry = result["telemetry"]
    swapchain = telemetry.get("swapchain") or {}
    samples = result["samples"]
    peak_smaps = parse_smaps_rollup(Path(result["peak_smaps_rollup"]))
    max_dgop_pss_dirty_kb = max((int(s.get("pss_dirty_kb") or 0) for s in samples), default=0)
    max_dgop_memory_kb = max((int(s.get("memory_kb") or 0) for s in samples), default=0)
    average_present_fps = float_value(telemetry.get("average_present_fps"))
    failures = validate_summary(
        args,
        process_status=int(result["status"]),
        telemetry=telemetry,
        average_present_fps=average_present_fps,
        present_mode=str(swapchain.get("present_mode") or ""),
        dgop_sample_count=len(samples),
        max_dgop_pss_dirty_kb=max_dgop_pss_dirty_kb,
    )
    return {
        "label": args.label,
        "status": "ok" if not failures else "failed",
        "failures": failures,
        "output_dir": str(out_dir),
        "command": result["cmd"],
        "process_status": result["status"],
        "pid": result["pid"],
        "runtime_elapsed_ms": int_value(telemetry.get("runtime_elapsed_ms")),
        "frames_presented": int_value(telemetry.get("frames_presented")),
        "frames_skipped": int_value(telemetry.get("frames_skipped")),
        "frames_skipped_frame_slots_pending": int_value(
            telemetry.get("frames_skipped_frame_slots_pending")
        ),
        "frames_skipped_swapchain_image_pending": int_value(
            telemetry.get("frames_skipped_swapchain_image_pending")
        ),
        "average_present_fps": average_present_fps,
        "present_mode": str(swapchain.get("present_mode") or ""),
        "swapchain_extent": swapchain.get("extent") or [],
        "dgop_sample_count": len(samples),
        "max_dgop_pss_dirty_kb": max_dgop_pss_dirty_kb,
        "max_dgop_memory_kb": max_dgop_memory_kb,
        "peak_smaps_pss_dirty_kb": peak_smaps.get("Pss_Dirty", 0),
        "peak_smaps_pss_kb": peak_smaps.get("Pss", 0),
        "peak_smaps_rss_kb": peak_smaps.get("Rss", 0),
        "scene_stdout": result["stdout"],
        "scene_stderr": result["stderr"],
        "dgop_csv": result["dgop"],
        "peak_smaps_rollup": result["peak_smaps_rollup"],
        "niri_screenshot": result["niri_screenshot"],
    }


def validate_summary(
    args: argparse.Namespace,
    process_status: int,
    telemetry: dict[str, Any],
    average_present_fps: float,
    present_mode: str,
    dgop_sample_count: int,
    max_dgop_pss_dirty_kb: int,
) -> list[str]:
    failures: list[str] = []
    if process_status != 0:
        failures.append(f"process_status={process_status}")
    if not telemetry:
        failures.append("missing_scene_json")
    if not args.allow_missing_dgop and dgop_sample_count == 0:
        failures.append("missing_dgop_samples")
    if args.expect_present_mode and present_mode != args.expect_present_mode:
        failures.append(f"present_mode={present_mode!r} != {args.expect_present_mode!r}")
    if (
        args.expect_min_average_present_fps
        and average_present_fps < args.expect_min_average_present_fps
    ):
        failures.append(
            f"average_present_fps={average_present_fps:.3f} < {args.expect_min_average_present_fps:.3f}"
        )
    if (
        args.expect_max_dgop_pss_dirty_kib
        and max_dgop_pss_dirty_kb > args.expect_max_dgop_pss_dirty_kib
    ):
        failures.append(
            f"max_dgop_pss_dirty_kb={max_dgop_pss_dirty_kb} > {args.expect_max_dgop_pss_dirty_kib}"
        )
    return failures


def capture_niri_json(out_dir: Path, label: str) -> None:
    if not shutil.which("niri"):
        return
    for command in ("focused-output", "layers", "windows"):
        path = out_dir / f"niri-{label}-{command}.json"
        result = subprocess.run(
            ["niri", "msg", "-j", command],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=3,
        )
        if result.returncode == 0:
            path.write_text(result.stdout)
        else:
            path.with_suffix(".err").write_text(result.stderr)


def capture_niri_screenshot(path: Path) -> None:
    if not shutil.which("niri"):
        return
    subprocess.run(
        [
            "niri",
            "msg",
            "action",
            "screenshot-screen",
            "--write-to-disk",
            "true",
            "--show-pointer",
            "false",
            "--path",
            str(path.resolve()),
        ],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=5,
    )


def terminate_process(process: subprocess.Popen[Any]) -> None:
    try:
        process.send_signal(signal.SIGTERM)
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def copy_proc_file(source: Path, target: Path, max_lines: int) -> None:
    try:
        lines = source.read_text(errors="replace").splitlines()
    except OSError:
        return
    target.write_text("\n".join(lines[:max_lines]) + ("\n" if lines else ""))


def parse_smaps_rollup(path: Path) -> dict[str, int]:
    values: dict[str, int] = {}
    try:
        for line in path.read_text().splitlines():
            if ":" not in line:
                continue
            key, rest = line.split(":", 1)
            parts = rest.strip().split()
            if parts and parts[0].isdigit():
                values[key] = int(parts[0])
    except OSError:
        pass
    return values


def read_json(path: Path) -> dict[str, Any]:
    try:
        text = path.read_text()
    except OSError:
        return {}
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start < 0 or end <= start:
            return {}
        try:
            payload = json.loads(text[start : end + 1])
        except json.JSONDecodeError:
            return {}
    return payload if isinstance(payload, dict) else {}


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def int_value(value: Any) -> int:
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return 0


def float_value(value: Any) -> float:
    try:
        return float(value or 0.0)
    except (TypeError, ValueError):
        return 0.0


def print_summary(summary: dict[str, Any]) -> None:
    print(f"summary: {summary['output_dir']}/summary.json")
    print(f"status: {summary['status']}")
    print(f"average_present_fps: {summary['average_present_fps']:.3f}")
    print(f"frames_presented: {summary['frames_presented']}")
    print(f"frames_skipped: {summary['frames_skipped']}")
    print(f"present_mode: {summary['present_mode']}")
    print(f"max_dgop_pss_dirty_kb: {summary['max_dgop_pss_dirty_kb']}")
    if summary["failures"]:
        for failure in summary["failures"]:
            print(f"FAIL: {failure}", file=sys.stderr)


if __name__ == "__main__":
    raise SystemExit(main())
