#!/usr/bin/env python3
"""Run FFmpeg Vulkan HW decode video matrices with dgop memory sampling."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path
from typing import Any

from workspace_paths import WORKSPACE_ROOT

DEFAULT_CODEC_SOURCES = {
    "h264": "artifacts/gilder/video-sources/h264/h264-high-b0-ref2-weightp0-weightb0-3840x2160-240fps-2402frames-g2401-d2400.mp4",
    "h265-main8": "artifacts/gilder/video-sources/h265/h265-main-8-b0-ref1-3840x2160-240fps-2402frames-g240-d2400.mp4",
    "h265-main10": "artifacts/gilder/video-sources/h265/h265-main-10-b0-ref1-3840x2160-240fps-566frames-g240-d240.mp4",
    "av1-main8": "artifacts/gilder/video-sources/av1/av1-main8-3840x2160-240fps-566frames-g240.webm",
    "av1-main10": "artifacts/gilder/video-sources/av1/av1-main10-3840x2160-240fps-566frames-g240.webm",
}

CODEC_CLI_NAMES = {
    "h264": "h264",
    "h265-main8": "h265",
    "h265-main10": "h265-main-10",
    "av1-main8": "av1",
    "av1-main10": "av1-main-10",
}

MEDIA_SUFFIXES = {".mp4", ".m4v", ".mov", ".mkv", ".webm"}
MATRIX_COLUMNS = [
    "codec",
    "status",
    "source",
    "width",
    "height",
    "source_codec_name",
    "source_profile",
    "source_level",
    "source_pix_fmt",
    "source_fps",
    "source_has_b_frames",
    "source_refs",
    "source_bit_rate",
    "source_nb_frames",
    "source_duration_seconds",
    "audio_clock_probe_requested",
    "audio_output_mode",
    "audio_stream_found",
    "audio_stream_error",
    "audio_master_clock_enabled",
    "audio_master_clock_start_ns",
    "audio_video_master_clock_ready",
    "audio_playback_target_reached",
    "audio_playback_coverage_percent",
    "audio_output_backend",
    "audio_output_xrun_count",
    "surface_host_binding",
    "surface_host_platform_backend",
    "surface_host_event_loop_backend",
    "surface_host_wait_configure_roundtrips",
    "surface_host_buffer_width",
    "surface_host_buffer_height",
    "decoder_codec",
    "decoder_name",
    "coded_width",
    "coded_height",
    "decoder_thread_count",
    "decoder_thread_type",
    "decoder_active_thread_type",
    "decoder_extra_hw_frames",
    "decoder_hw_frames_initial_pool_size",
    "decoder_low_delay_flag",
    "decoder_fast_flag",
    "decoder_has_b_frames",
    "decoder_codec_delay",
    "decoder_h264_enable_er",
    "decoder_max_packet_size_bytes",
    "inferred_min_ffmpeg_slice_buffer_slot_bytes",
    "inferred_min_ffmpeg_slice_buffer_slot_kb",
    "codec_host_memory_model",
    "inferred_codec_resolution_scaled_host_bytes",
    "inferred_codec_resolution_scaled_host_kb",
    "inferred_h264_refstruct_min_three_picture_bytes",
    "inferred_hevc_refstruct_min_three_picture_bytes",
    "inferred_hevc_layer_tables_bytes",
    "target_fps",
    "playback_frames",
    "max_memory_kb",
    "max_memory_minus_codec_resolution_scaled_host_kb",
    "last_memory_kb",
    "memory_calculation",
    "memory_sample_count",
    "ignored_memory_sample_count",
    "raw_max_memory_kb",
    "avg_cpu_percent",
    "max_cpu_percent",
    "last_cpu_percent",
    "cpu_sample_count",
    "peak_smaps_rollup",
    "peak_smaps_rss_kb",
    "peak_smaps_pss_kb",
    "peak_smaps_pss_dirty_kb",
    "peak_smaps_private_dirty_kb",
    "peak_smaps_anonymous_kb",
    "dgop_minus_peak_smaps_pss_dirty_kb",
    "average_present_fps",
    "average_present_teardown_inclusive_fps",
    "presented_frame_count",
    "all_zero_copy_presented",
    "present_mode",
    "present_delta_over_6250us_count",
    "present_delta_over_8334us_count",
    "frame_sleep_count",
    "total_pacing_sleep_micros",
    "present_sleep_guard_micros",
    "present_spin_guard_micros",
    "present_handoff_route",
    "present_handoff_capacity_frames",
    "present_handoff_peak_depth",
    "ffmpeg_retained_avframe_count",
    "ffmpeg_retained_avframe_peak_count",
    "descriptor_sampler_cache_entry_count",
    "descriptor_sampler_cache_peak_entry_count",
    "descriptor_sampler_cache_rewrite_count",
    "descriptor_sampler_cache_recreate_count",
    "descriptor_sampler_cache_resource_heap_kb",
    "descriptor_sampler_cache_sampler_heap_kb",
    "descriptor_sampler_cache_total_heap_kb",
    "telemetry",
]


@dataclass(frozen=True)
class VideoProbe:
    codec_cli: str
    width: int
    height: int
    fps: float
    codec_name: str
    profile: str
    pix_fmt: str
    level: int
    has_b_frames: int
    refs: int
    bit_rate: int
    nb_frames: int
    duration_seconds: float


@dataclass(frozen=True)
class VideoCase:
    key: str
    source: Path
    probe: VideoProbe
    target_fps: int
    playback_frames: int


def main() -> int:
    args = parse_args()
    repo_root = WORKSPACE_ROOT
    os.chdir(repo_root)

    if not args.display:
        print("FAIL: WAYLAND_DISPLAY is empty; pass --display", file=sys.stderr)
        return 2

    if not args.no_build:
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "--features",
                "native-vulkan-video",
                "--bin",
                "gilder-native-vulkan",
            ],
            check=True,
        )

    cases = collect_cases(args, repo_root)
    if not cases:
        print("FAIL: no video cases selected", file=sys.stderr)
        return 2

    work_dir = Path(args.work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)
    matrix_csv = work_dir / f"{args.artifact_prefix}-{args.label}-matrix.csv"
    overall_status = 0
    with matrix_csv.open("w", newline="") as matrix_file:
        writer = csv.DictWriter(matrix_file, fieldnames=MATRIX_COLUMNS)
        writer.writeheader()
        for case in cases:
            row, summary = run_case(args, case, work_dir)
            writer.writerow(row)
            matrix_file.flush()
            print(summary)
            if row["status"] != 0:
                overall_status = 1
                if args.fail_fast:
                    break

    print(f"matrix: {matrix_csv}")
    return overall_status


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run FFmpeg Vulkan HW decode sources and collect dgop memory plus "
            "present telemetry."
        )
    )
    parser.add_argument("--display", default=os.environ.get("WAYLAND_DISPLAY", ""))
    parser.add_argument("--output-name", "--output", dest="output_name", default="")
    parser.add_argument("--label", default="matrix")
    parser.add_argument("--work-dir", default=os.environ.get("TMPDIR", "/tmp"))
    parser.add_argument(
        "--codecs",
        default="h264,h265-main8,h265-main10,av1-main8,av1-main10",
        help="Comma list for the built-in 4K240 set, or a filter for explicit sources.",
    )
    parser.add_argument("--source", action="append", default=[])
    parser.add_argument("--source-dir", action="append", default=[])
    parser.add_argument("--source-glob", action="append", default=[])
    parser.add_argument(
        "--all-video-sources",
        action="store_true",
        help="Scan artifacts/gilder/video-sources recursively.",
    )
    parser.add_argument("--max-sources", type=int, default=0)
    parser.add_argument("--frames", type=int, default=0)
    parser.add_argument(
        "--duration",
        type=float,
        default=0.0,
        help="Playback seconds. Overrides --frames when positive.",
    )
    parser.add_argument(
        "--target-fps",
        default="240",
        help="Integer target FPS, or 'source' to use probed source FPS.",
    )
    parser.add_argument("--present-mode-policy", default="")
    parser.add_argument("--wait-after-present", action="store_true")
    parser.add_argument("--audio-clock-probe", action="store_true")
    parser.add_argument(
        "--audio-output",
        default="clock-only",
        choices=["clock-only", "auto"],
        help="Audio output mode when --audio-clock-probe is enabled.",
    )
    parser.add_argument(
        "--release-frame-after-render-fence",
        action="store_true",
        help="Set GILDER_FFMPEG_VULKAN_HWDECODE_RELEASE_FRAME_AFTER_RENDER_FENCE=1.",
    )
    parser.add_argument("--sample-interval", type=float, default=0.1)
    parser.add_argument(
        "--sample-cpu",
        action="store_true",
        help="Ask dgop to calculate target CPU percent. This is diagnostic and costs more than memory-only sampling.",
    )
    parser.add_argument("--artifact-prefix", default="gilder-ffmpeg-vulkan-hwdecode")
    parser.add_argument("--binary", default="target/release/gilder-native-vulkan")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--fail-fast", action="store_true")
    return parser.parse_args()


def collect_cases(args: argparse.Namespace, repo_root: Path) -> list[VideoCase]:
    codec_filter = {item.strip() for item in args.codecs.split(",") if item.strip()}
    explicit_sources = selected_sources(args, repo_root)
    cases: list[VideoCase] = []

    if not explicit_sources:
        for codec in [item.strip() for item in args.codecs.split(",") if item.strip()]:
            source = DEFAULT_CODEC_SOURCES.get(codec)
            if source is None:
                raise SystemExit(f"FAIL: unsupported built-in codec label: {codec}")
            probe = probe_source(repo_root / source, CODEC_CLI_NAMES[codec])
            cases.append(make_case(args, codec, repo_root / source, probe))
        return cases

    for source in explicit_sources:
        probe = probe_source(source, None)
        codec_label = codec_label_from_cli(probe.codec_cli)
        if codec_filter and codec_filter != set(DEFAULT_CODEC_SOURCES) and codec_label not in codec_filter:
            continue
        cases.append(make_case(args, source_key(source, probe), source, probe))

    if args.max_sources > 0:
        cases = cases[: args.max_sources]
    return cases


def selected_sources(args: argparse.Namespace, repo_root: Path) -> list[Path]:
    sources = [Path(item) for item in args.source]
    for pattern in args.source_glob:
        sources.extend(Path(path) for path in sorted(repo_root.glob(pattern)))
    for directory in args.source_dir:
        sources.extend(scan_media_dir(Path(directory)))
    if args.all_video_sources:
        sources.extend(scan_media_dir(repo_root / "artifacts/gilder/video-sources"))

    resolved: list[Path] = []
    seen: set[Path] = set()
    for source in sources:
        path = source if source.is_absolute() else repo_root / source
        path = path.resolve()
        if path in seen:
            continue
        if not path.is_file():
            raise SystemExit(f"FAIL: source missing: {path}")
        if path.suffix.lower() not in MEDIA_SUFFIXES:
            continue
        seen.add(path)
        resolved.append(path)
    return sorted(resolved)


def scan_media_dir(directory: Path) -> list[Path]:
    if not directory.is_dir():
        raise SystemExit(f"FAIL: source directory missing: {directory}")
    return sorted(
        path
        for path in directory.rglob("*")
        if path.is_file() and path.suffix.lower() in MEDIA_SUFFIXES
    )


def probe_source(source: Path, forced_codec_cli: str | None) -> VideoProbe:
    ffprobe = shutil.which("ffprobe")
    data: dict[str, Any] = {}
    if ffprobe:
        result = subprocess.run(
            [
                ffprobe,
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=codec_name,profile,pix_fmt,width,height,avg_frame_rate,r_frame_rate,level,refs,has_b_frames,bit_rate,nb_frames,duration",
                "-of",
                "json",
                str(source),
            ],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if result.returncode == 0:
            streams = json.loads(result.stdout or "{}").get("streams") or []
            data = streams[0] if streams else {}

    width = int(data.get("width") or filename_resolution(source)[0] or 0)
    height = int(data.get("height") or filename_resolution(source)[1] or 0)
    fps = fps_from_rate(data.get("avg_frame_rate")) or fps_from_rate(
        data.get("r_frame_rate")
    ) or filename_fps(source)
    codec_name = str(data.get("codec_name") or "").lower()
    profile = str(data.get("profile") or "")
    pix_fmt = str(data.get("pix_fmt") or "")
    codec_cli = forced_codec_cli or infer_codec_cli(source, codec_name, profile, pix_fmt)

    if width <= 0 or height <= 0:
        raise SystemExit(f"FAIL: could not probe dimensions for {source}")
    if fps <= 0:
        raise SystemExit(f"FAIL: could not probe FPS for {source}")
    return VideoProbe(
        codec_cli,
        width,
        height,
        fps,
        codec_name,
        profile,
        pix_fmt,
        int_value(data.get("level")),
        int_value(data.get("has_b_frames")),
        int_value(data.get("refs")),
        int_value(data.get("bit_rate")),
        int_value(data.get("nb_frames")),
        float_value(data.get("duration")),
    )


def make_case(
    args: argparse.Namespace, key: str, source: Path, probe: VideoProbe
) -> VideoCase:
    target_fps = target_fps_for_case(args.target_fps, probe)
    if args.duration > 0:
        playback_frames = max(1, round(target_fps * args.duration))
    else:
        playback_frames = args.frames if args.frames > 0 else 2400
    return VideoCase(sanitize_key(key), source, probe, target_fps, playback_frames)


def run_case(
    args: argparse.Namespace, case: VideoCase, work_dir: Path
) -> tuple[dict[str, Any], str]:
    prefix = work_dir / f"{args.artifact_prefix}-{args.label}-{case.key}"
    telemetry_path = prefix.with_name(prefix.name + "-telemetry.json")
    stderr_path = prefix.with_suffix(".stderr")
    dgop_path = prefix.with_name(prefix.name + "-dgop.csv")
    rollup_path = prefix.with_name(prefix.name + "-smaps-rollup.txt")
    peak_rollup_path = prefix.with_name(prefix.name + "-peak-smaps-rollup.txt")
    smaps_path = prefix.with_name(prefix.name + "-smaps.txt")
    summary_path = prefix.with_name(prefix.name + "-summary.txt")
    for path in [
        telemetry_path,
        stderr_path,
        dgop_path,
        rollup_path,
        peak_rollup_path,
        smaps_path,
        summary_path,
    ]:
        path.unlink(missing_ok=True)

    cmd = [
        args.binary,
        "--run-video",
        "--source",
        str(case.source),
        "--video-codec",
        case.probe.codec_cli,
        "--width",
        str(case.probe.width),
        "--height",
        str(case.probe.height),
        "--target-fps",
        str(case.target_fps),
        "--playback-frames",
        str(case.playback_frames),
        "--layer",
        "bottom",
        "--wait-roundtrips",
        "2",
    ]
    if args.output_name:
        cmd.extend(["--output-name", args.output_name])
    if args.audio_clock_probe:
        cmd.append("--audio-clock-probe")
    if args.audio_clock_probe or args.audio_output != "clock-only":
        cmd.extend(["--audio-output", args.audio_output])

    env = os.environ.copy()
    env["WAYLAND_DISPLAY"] = args.display
    env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    if args.present_mode_policy and args.present_mode_policy != "default":
        env["GILDER_VULKAN_PRESENT_MODE_POLICY"] = args.present_mode_policy
    if args.wait_after_present:
        env["GILDER_VULKAN_PRESENT_WAIT_AFTER_PRESENT"] = "1"
    if args.release_frame_after_render_fence:
        env["GILDER_FFMPEG_VULKAN_HWDECODE_RELEASE_FRAME_AFTER_RENDER_FENCE"] = "1"

    samples: list[dict[str, Any]] = []
    with telemetry_path.open("w") as stdout, stderr_path.open("w") as stderr, dgop_path.open(
        "w", newline=""
    ) as dgop_file:
        dgop_writer = csv.DictWriter(
            dgop_file,
            fieldnames=[
                "sample",
                "elapsed_ms",
                "pid",
                "memory_kb",
                "memory_calculation",
                "cpu_percent",
                "pticks",
                "rss_kb",
                "pss_kb",
                "pss_dirty_kb",
                "anonymous_kb",
                "command",
            ],
        )
        dgop_writer.writeheader()
        started = time.monotonic()
        process = subprocess.Popen(cmd, stdout=stdout, stderr=stderr, env=env)
        sample_index = 0
        captured_smaps = False
        peak_memory_kb = 0
        dgop_cursor = ""
        time.sleep(max(0.0, args.sample_interval))
        while process.poll() is None:
            elapsed_ms = int((time.monotonic() - started) * 1000)
            sample, dgop_cursor = sample_dgop(
                process.pid, args.binary, dgop_cursor, args.sample_cpu
            )
            if sample:
                sample.update({"sample": sample_index, "elapsed_ms": elapsed_ms, "pid": process.pid})
                samples.append(sample)
                dgop_writer.writerow(sample)
                dgop_file.flush()
                if valid_memory_sample(sample) and sample["memory_kb"] > peak_memory_kb:
                    peak_memory_kb = sample["memory_kb"]
                    copy_proc_file(
                        Path(f"/proc/{process.pid}/smaps_rollup"),
                        peak_rollup_path,
                        200,
                    )
            if not captured_smaps and elapsed_ms > 3000:
                copy_proc_file(Path(f"/proc/{process.pid}/smaps_rollup"), rollup_path, 200)
                copy_proc_file(Path(f"/proc/{process.pid}/smaps"), smaps_path, 20000)
                captured_smaps = True
            sample_index += 1
            time.sleep(max(0.0, args.sample_interval))
        status = process.wait()

    memory = aggregate_memory(samples)
    cpu = aggregate_cpu(samples)
    peak_smaps = parse_smaps_rollup(peak_rollup_path)
    telemetry = read_json(telemetry_path)
    decoder = telemetry.get("decoder") or {}
    surface_host = telemetry.get("surface_host") or {}
    decoder_coded_extent = list_value(decoder.get("coded_extent"))
    surface_buffer_size = list_value(surface_host.get("buffer_size"))
    codec_host_bytes = int_value(decoder.get("inferred_codec_resolution_scaled_host_bytes"))
    codec_host_kb = bytes_to_kb(codec_host_bytes)
    seq = telemetry.get("decoded_image_present_sequence") or {}
    audio = telemetry.get("audio_clock") or {}
    present_handoff = seq.get("present_handoff") or {}
    swapchain = (telemetry.get("device") or {}).get("swapchain") or {}
    if not telemetry:
        status = 1
        matrix_status = "missing-runtime-json"
    elif not (
        seq.get("presented_frame_count") == telemetry.get("requested_present_frame_count")
        and telemetry.get("decoded_image_zero_copy_presented") is True
    ):
        status = 1
        matrix_status = "failed-zero-copy-present-contract"
    else:
        matrix_status = "ok"

    row = {
        "codec": case.key,
        "status": status,
        "source": str(case.source),
        "width": case.probe.width,
        "height": case.probe.height,
        "source_codec_name": case.probe.codec_name,
        "source_profile": case.probe.profile,
        "source_level": case.probe.level,
        "source_pix_fmt": case.probe.pix_fmt,
        "source_fps": case.probe.fps,
        "source_has_b_frames": case.probe.has_b_frames,
        "source_refs": case.probe.refs,
        "source_bit_rate": case.probe.bit_rate,
        "source_nb_frames": case.probe.nb_frames,
        "source_duration_seconds": case.probe.duration_seconds,
        "audio_clock_probe_requested": telemetry.get(
            "audio_clock_probe_requested", args.audio_clock_probe
        ),
        "audio_output_mode": telemetry.get("audio_output_mode", args.audio_output),
        "audio_stream_found": audio.get("audio_stream_found", False),
        "audio_stream_error": audio.get("audio_stream_error", ""),
        "audio_master_clock_enabled": telemetry.get("audio_master_clock_enabled", False),
        "audio_master_clock_start_ns": telemetry.get("audio_master_clock_start_ns", ""),
        "audio_video_master_clock_ready": audio.get("video_master_clock_ready", False),
        "audio_playback_target_reached": audio.get("playback_target_reached", False),
        "audio_playback_coverage_percent": audio.get("playback_coverage_percent", 0),
        "audio_output_backend": audio.get("audio_output_backend", ""),
        "audio_output_xrun_count": audio.get("audio_output_xrun_count", 0),
        "surface_host_binding": surface_host.get("binding", ""),
        "surface_host_platform_backend": surface_host.get("platform_backend", ""),
        "surface_host_event_loop_backend": surface_host.get("event_loop_backend", ""),
        "surface_host_wait_configure_roundtrips": int_value(
            surface_host.get("wait_configure_roundtrips")
        ),
        "surface_host_buffer_width": int_at(surface_buffer_size, 0),
        "surface_host_buffer_height": int_at(surface_buffer_size, 1),
        "decoder_codec": decoder.get("codec", ""),
        "decoder_name": decoder.get("decoder_name", ""),
        "coded_width": int_at(decoder_coded_extent, 0),
        "coded_height": int_at(decoder_coded_extent, 1),
        "decoder_thread_count": int_value(decoder.get("thread_count")),
        "decoder_thread_type": int_value(decoder.get("thread_type")),
        "decoder_active_thread_type": int_value(decoder.get("active_thread_type")),
        "decoder_extra_hw_frames": int_value(decoder.get("extra_hw_frames")),
        "decoder_hw_frames_initial_pool_size": int_value(
            decoder.get("hw_frames_initial_pool_size")
        ),
        "decoder_low_delay_flag": decoder.get("low_delay_flag", False),
        "decoder_fast_flag": decoder.get("fast_flag", False),
        "decoder_has_b_frames": int_value(decoder.get("has_b_frames")),
        "decoder_codec_delay": int_value(decoder.get("codec_delay")),
        "decoder_h264_enable_er": int_value(decoder.get("h264_enable_er")),
        "decoder_max_packet_size_bytes": int_value(decoder.get("max_packet_size_bytes")),
        "inferred_min_ffmpeg_slice_buffer_slot_bytes": int_value(
            decoder.get("inferred_min_ffmpeg_slice_buffer_slot_bytes")
        ),
        "inferred_min_ffmpeg_slice_buffer_slot_kb": bytes_to_kb(
            int_value(decoder.get("inferred_min_ffmpeg_slice_buffer_slot_bytes"))
        ),
        "codec_host_memory_model": decoder.get("codec_host_memory_model", ""),
        "inferred_codec_resolution_scaled_host_bytes": codec_host_bytes,
        "inferred_codec_resolution_scaled_host_kb": codec_host_kb,
        "inferred_h264_refstruct_min_three_picture_bytes": int_value(
            decoder.get("inferred_h264_refstruct_min_three_picture_bytes")
        ),
        "inferred_hevc_refstruct_min_three_picture_bytes": int_value(
            decoder.get("inferred_hevc_refstruct_min_three_picture_bytes")
        ),
        "inferred_hevc_layer_tables_bytes": int_value(
            decoder.get("inferred_hevc_layer_tables_bytes")
        ),
        "target_fps": case.target_fps,
        "playback_frames": case.playback_frames,
        "max_memory_kb": memory["max_memory_kb"],
        "max_memory_minus_codec_resolution_scaled_host_kb": (
            memory["max_memory_kb"] - codec_host_kb
        )
        if memory["max_memory_kb"] > 0
        else 0,
        "last_memory_kb": memory["last_memory_kb"],
        "memory_calculation": memory["memory_calculation"],
        "memory_sample_count": memory["memory_sample_count"],
        "ignored_memory_sample_count": memory["ignored_memory_sample_count"],
        "raw_max_memory_kb": memory["raw_max_memory_kb"],
        "avg_cpu_percent": cpu["avg_cpu_percent"],
        "max_cpu_percent": cpu["max_cpu_percent"],
        "last_cpu_percent": cpu["last_cpu_percent"],
        "cpu_sample_count": cpu["cpu_sample_count"],
        "peak_smaps_rollup": str(peak_rollup_path),
        "peak_smaps_rss_kb": peak_smaps.get("Rss", 0),
        "peak_smaps_pss_kb": peak_smaps.get("Pss", 0),
        "peak_smaps_pss_dirty_kb": peak_smaps.get("Pss_Dirty", 0),
        "peak_smaps_private_dirty_kb": peak_smaps.get("Private_Dirty", 0),
        "peak_smaps_anonymous_kb": peak_smaps.get("Anonymous", 0),
        "dgop_minus_peak_smaps_pss_dirty_kb": (
            memory["max_memory_kb"] - peak_smaps.get("Pss_Dirty", 0)
        )
        if peak_smaps.get("Pss_Dirty", 0) > 0 and memory["max_memory_kb"] > 0
        else 0,
        "average_present_fps": seq.get("average_present_fps", 0),
        "average_present_teardown_inclusive_fps": seq.get(
            "average_present_teardown_inclusive_fps", 0
        ),
        "presented_frame_count": seq.get("presented_frame_count", 0),
        "all_zero_copy_presented": seq.get("all_zero_copy_presented", False),
        "present_mode": swapchain.get("present_mode", "unknown"),
        "present_delta_over_6250us_count": seq.get("present_delta_over_6250us_count", 0),
        "present_delta_over_8334us_count": seq.get("present_delta_over_8334us_count", 0),
        "frame_sleep_count": seq.get("frame_sleep_count", 0),
        "total_pacing_sleep_micros": seq.get("total_pacing_sleep_micros", 0),
        "present_sleep_guard_micros": seq.get("present_sleep_guard_micros", 0),
        "present_spin_guard_micros": seq.get("present_spin_guard_micros", 0),
        "present_handoff_route": present_handoff.get("route", ""),
        "present_handoff_capacity_frames": present_handoff.get("capacity_frames", 0),
        "present_handoff_peak_depth": present_handoff.get("peak_depth", 0),
        "ffmpeg_retained_avframe_count": seq.get("ffmpeg_retained_avframe_count", 0),
        "ffmpeg_retained_avframe_peak_count": seq.get(
            "ffmpeg_retained_avframe_peak_count", 0
        ),
        "descriptor_sampler_cache_entry_count": seq.get(
            "descriptor_sampler_cache_entry_count", 0
        ),
        "descriptor_sampler_cache_peak_entry_count": seq.get(
            "descriptor_sampler_cache_peak_entry_count", 0
        ),
        "descriptor_sampler_cache_rewrite_count": seq.get(
            "descriptor_sampler_cache_rewrite_count", 0
        ),
        "descriptor_sampler_cache_recreate_count": seq.get(
            "descriptor_sampler_cache_recreate_count", 0
        ),
        "descriptor_sampler_cache_resource_heap_kb": bytes_to_kb(
            int_value(seq.get("descriptor_sampler_cache_resource_heap_bytes"))
        ),
        "descriptor_sampler_cache_sampler_heap_kb": bytes_to_kb(
            int_value(seq.get("descriptor_sampler_cache_sampler_heap_bytes"))
        ),
        "descriptor_sampler_cache_total_heap_kb": bytes_to_kb(
            int_value(seq.get("descriptor_sampler_cache_total_heap_bytes"))
        ),
        "telemetry": str(telemetry_path),
    }
    summary = "\n".join(f"{key}: {value}" for key, value in row.items())
    summary += f"\nmatrix_status: {matrix_status}\nsummary: {summary_path}"
    summary_path.write_text(summary + "\n")
    return row, str(summary_path)


def sample_dgop(
    pid: int, binary: str, cursor: str, sample_cpu: bool
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
    payload = json.loads(result.stdout or "{}")
    next_cursor = str(payload.get("cursor") or cursor)
    processes = payload.get("processes") or []
    binary_suffix = "/" + binary
    for process in processes:
        executable = str(process.get("executablePath") or "")
        if process.get("pid") == pid or executable.endswith(binary_suffix):
            sample = {
                "memory_kb": int(process.get("memoryKB") or 0),
                "memory_calculation": process.get("memoryCalculation") or "",
                "rss_kb": int(process.get("rssKB") or 0),
                "pss_kb": int(process.get("pssKB") or 0),
                "pss_dirty_kb": int(
                    process.get("pssDirtyKB")
                    or process.get("pss_dirty_kb")
                    or process.get("pssDirtyKb")
                    or 0
                ),
                "anonymous_kb": int(
                    process.get("anonymousKB")
                    or process.get("anonymous_kb")
                    or process.get("anonymousKb")
                    or 0
                ),
                "command": process.get("command") or "",
            }
            if sample_cpu:
                sample["cpu_percent"] = float_value(process.get("cpu"))
                sample["pticks"] = int_value(process.get("pticks"))
            return sample, next_cursor
    return None, next_cursor


def aggregate_memory(samples: list[dict[str, Any]]) -> dict[str, Any]:
    valid_samples = [sample for sample in samples if valid_memory_sample(sample)]
    if not valid_samples:
        return {
            "memory_calculation": "unknown",
            "memory_sample_count": 0,
            "ignored_memory_sample_count": len(samples),
            "max_memory_kb": 0,
            "last_memory_kb": 0,
            "raw_max_memory_kb": 0,
        }
    selected = (
        "pss_dirty"
        if any(s["memory_calculation"] == "pss_dirty" for s in valid_samples)
        else valid_samples[0]["memory_calculation"]
    )
    selected_samples = [s for s in valid_samples if s["memory_calculation"] == selected]
    return {
        "memory_calculation": selected,
        "memory_sample_count": len(selected_samples),
        "ignored_memory_sample_count": len(samples) - len(selected_samples),
        "max_memory_kb": max((s["memory_kb"] for s in selected_samples), default=0),
        "last_memory_kb": selected_samples[-1]["memory_kb"] if selected_samples else 0,
        "raw_max_memory_kb": max(s["memory_kb"] for s in samples),
    }


def aggregate_cpu(samples: list[dict[str, Any]]) -> dict[str, Any]:
    cpu_samples = [
        float_value(sample.get("cpu_percent"))
        for sample in samples
        if "cpu_percent" in sample
    ]
    if len(cpu_samples) > 1:
        cpu_samples = cpu_samples[1:]
    if not cpu_samples:
        return {
            "avg_cpu_percent": 0,
            "max_cpu_percent": 0,
            "last_cpu_percent": 0,
            "cpu_sample_count": 0,
        }
    return {
        "avg_cpu_percent": round(sum(cpu_samples) / len(cpu_samples), 2),
        "max_cpu_percent": round(max(cpu_samples), 2),
        "last_cpu_percent": round(cpu_samples[-1], 2),
        "cpu_sample_count": len(cpu_samples),
    }


def valid_memory_sample(sample: dict[str, Any]) -> bool:
    memory_kb = int_value(sample.get("memory_kb"))
    rss_kb = int_value(sample.get("rss_kb"))
    calculation = str(sample.get("memory_calculation") or "")
    if memory_kb <= 0:
        return False
    if calculation in {"pss_dirty", "pss", "rss"} and rss_kb > 0 and memory_kb > rss_kb:
        return False
    return True


def read_json(path: Path) -> dict[str, Any]:
    try:
        with path.open() as file:
            return json.load(file)
    except (OSError, json.JSONDecodeError):
        return {}


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


def list_value(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


def int_at(values: list[Any], index: int) -> int:
    if index >= len(values):
        return 0
    return int_value(values[index])


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


def bytes_to_kb(value: int) -> int:
    return (max(0, value) + 1023) // 1024


def copy_proc_file(source: Path, dest: Path, line_limit: int) -> None:
    try:
        with source.open() as src, dest.open("w") as dst:
            for index, line in enumerate(src):
                if index >= line_limit:
                    break
                dst.write(line)
    except OSError:
        return


def infer_codec_cli(source: Path, codec_name: str, profile: str, pix_fmt: str) -> str:
    lower = str(source).lower()
    is_10_bit = "main10" in lower or "main-10" in lower or "p010" in pix_fmt.lower() or "10" in profile
    if "av1" in lower or codec_name == "av1":
        return "av1-main-10" if is_10_bit else "av1"
    if "h265" in lower or "hevc" in lower or codec_name == "hevc":
        return "h265-main-10" if is_10_bit else "h265"
    if "h264" in lower or codec_name == "h264":
        return "h264"
    raise SystemExit(f"FAIL: unsupported video codec for {source}: {codec_name}")


def codec_label_from_cli(codec_cli: str) -> str:
    return {
        "h264": "h264",
        "h265": "h265-main8",
        "h265-main-10": "h265-main10",
        "av1": "av1-main8",
        "av1-main-10": "av1-main10",
    }[codec_cli]


def source_key(source: Path, probe: VideoProbe) -> str:
    stem = sanitize_key(source.stem)[:96] or "source"
    digest = hashlib.sha1(str(source).encode("utf-8")).hexdigest()[:10]
    return f"{codec_label_from_cli(probe.codec_cli)}-{stem}-{digest}"


def sanitize_key(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", value).strip("-")[:160]


def target_fps_for_case(value: str, probe: VideoProbe) -> int:
    if value in {"source", "auto"}:
        return max(1, round(probe.fps))
    target_fps = int(value)
    if target_fps < 1:
        raise SystemExit("FAIL: --target-fps must be positive or 'source'")
    return target_fps


def filename_resolution(source: Path) -> tuple[int, int]:
    match = re.search(r"(\d{2,5})x(\d{2,5})", source.name)
    if not match:
        return (0, 0)
    return (int(match.group(1)), int(match.group(2)))


def filename_fps(source: Path) -> float:
    match = re.search(r"(\d+(?:\.\d+)?)fps", source.name)
    return float(match.group(1)) if match else 0.0


def fps_from_rate(value: Any) -> float:
    if not value or value == "0/0":
        return 0.0
    try:
        fraction = Fraction(str(value))
    except ValueError:
        return 0.0
    if fraction.denominator == 0:
        return 0.0
    return float(fraction)


if __name__ == "__main__":
    raise SystemExit(main())
