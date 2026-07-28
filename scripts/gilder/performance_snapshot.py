#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# ///
"""Sample process memory/CPU evidence for Gilder runs.

Invoke with:
  uv run python scripts/performance_snapshot.py --duration 10 --pid <pid>
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from workspace_paths import WORKSPACE_ROOT

SAMPLE_COLUMNS = [
    "sample",
    "elapsed_ms",
    "pid",
    "rss_kib",
    "pss_kib",
    "pss_dirty_kib",
    "private_clean_kib",
    "private_dirty_kib",
    "private_kib",
    "anonymous_kib",
    "dgop_memory_kib",
    "dgop_memory_calculation",
    "dgop_pss_kib",
    "dgop_pss_dirty_kib",
    "dgop_rss_kib",
    "cpu_percent",
    "command",
    "status_json",
]


@dataclass
class Sample:
    sample: int
    elapsed_ms: int
    pid: int
    rss_kib: int = 0
    pss_kib: int = 0
    pss_dirty_kib: int = 0
    private_clean_kib: int = 0
    private_dirty_kib: int = 0
    private_kib: int = 0
    anonymous_kib: int = 0
    dgop_memory_kib: int = 0
    dgop_memory_calculation: str = ""
    dgop_pss_kib: int = 0
    dgop_pss_dirty_kib: int = 0
    dgop_rss_kib: int = 0
    cpu_percent: float = 0.0
    command: str = ""
    status_json: str = ""

    def row(self) -> dict[str, Any]:
        return {column: getattr(self, column) for column in SAMPLE_COLUMNS}


def main() -> int:
    args = parse_args()
    repo_root = WORKSPACE_ROOT
    os.chdir(repo_root)

    pid = args.pid or find_gilderd_pid()
    if pid <= 0:
        return missing(args, "找不到 gilderd 进程；传入 --pid 可直接采样指定进程")
    if not Path(f"/proc/{pid}").exists():
        return missing(args, f"进程不存在: {pid}")

    out_dir = Path(args.output_dir) if args.output_dir else Path(args.work_dir) / (
        f"gilder-performance-{args.label}-{int(time.time())}-{os.getpid()}"
    )
    out_dir.mkdir(parents=True, exist_ok=True)
    samples_path = out_dir / "samples.csv"
    summary_path = out_dir / "summary.json"
    metadata_path = out_dir / "metadata.json"

    metadata = {
        "label": args.label,
        "pid": pid,
        "duration_seconds": args.duration,
        "interval_seconds": args.interval,
        "dgop": shutil.which("dgop") or "",
        "gilderctl": resolve_gilderctl(args.gilderctl) or "",
        "socket": args.socket,
        "unsupported_expectations": args.unsupported_expectations,
    }
    metadata_path.write_text(json.dumps(metadata, indent=2, ensure_ascii=False) + "\n")

    gilderctl = resolve_gilderctl(args.gilderctl)
    sample_count = max(1, int(args.duration / args.interval) + 1)
    samples: list[Sample] = []
    dgop_cursor = ""
    started = time.monotonic()
    with samples_path.open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=SAMPLE_COLUMNS)
        writer.writeheader()
        for index in range(sample_count):
            if not Path(f"/proc/{pid}").exists():
                break
            elapsed_ms = int((time.monotonic() - started) * 1000)
            sample = Sample(sample=index, elapsed_ms=elapsed_ms, pid=pid)
            apply_proc_sample(sample)
            dgop_cursor = apply_dgop_sample(sample, dgop_cursor, args.sample_cpu)
            if gilderctl and args.status:
                status_path = sample_status(
                    gilderctl, args.socket, out_dir, index, args.allow_missing
                )
                if status_path:
                    sample.status_json = status_path.name
            samples.append(sample)
            writer.writerow(sample.row())
            file.flush()
            if index + 1 < sample_count:
                time.sleep(max(0.0, args.interval))

    summary = summarize(samples)
    summary.update(
        {
            "label": args.label,
            "pid": pid,
            "samples": str(samples_path),
            "metadata": str(metadata_path),
        }
    )
    failures = validate(args, summary)
    summary["failures"] = failures
    summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n")

    print(f"samples: {samples_path}")
    print(f"summary: {summary_path}")
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="采样 Gilder 进程的 dgop/proc 内存、CPU 和可选 gilderctl status。"
    )
    parser.add_argument("--pid", type=int, default=0)
    parser.add_argument("--socket", default=os.environ.get("GILDER_SOCKET", ""))
    parser.add_argument("--gilderctl", default="")
    parser.add_argument("--label", default="sample")
    parser.add_argument("--duration", type=float, default=10.0)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--work-dir", default=os.environ.get("TMPDIR", "/tmp"))
    parser.add_argument("--output-dir", default="")
    parser.add_argument("--allow-missing", action="store_true")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--sample-cpu", action="store_true")
    parser.add_argument("--expect-max-rss-kib-at-most", type=int)
    parser.add_argument("--expect-max-pss-kib-at-most", type=int)
    parser.add_argument("--expect-max-pss-dirty-kib-at-most", type=int)
    parser.add_argument("--expect-max-private-dirty-kib-at-most", type=int)
    parser.add_argument("--expect-max-private-kib-at-most", type=int)
    parser.add_argument("--expect-retained-pss-delta-kib-at-most", type=int)
    parser.add_argument("--expect-retained-private-delta-kib-at-most", type=int)
    args, unknown = parser.parse_known_args()
    args.unsupported_expectations = unknown
    if args.duration <= 0 or args.interval <= 0:
        parser.error("--duration 和 --interval 必须为正数")
    return args


def missing(args: argparse.Namespace, message: str) -> int:
    prefix = "SKIP" if args.allow_missing else "FAIL"
    print(f"{prefix}: {message}", file=sys.stderr)
    return 0 if args.allow_missing else 1


def find_gilderd_pid() -> int:
    result = subprocess.run(
        ["ps", "-eo", "pid=,user=,comm="],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return 0
    user = os.environ.get("USER", "")
    for line in result.stdout.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[1] == user and parts[2] == "gilderd":
            return int(parts[0])
    return 0


def resolve_gilderctl(value: str) -> str:
    candidates = [value] if value else []
    candidates.extend(["target/debug/gilderctl", "target/release/gilderctl", "gilderctl"])
    for candidate in candidates:
        if not candidate:
            continue
        path = Path(candidate)
        if path.exists() and os.access(path, os.X_OK):
            return str(path)
        resolved = shutil.which(candidate)
        if resolved:
            return resolved
    return ""


def apply_proc_sample(sample: Sample) -> None:
    rollup = parse_smaps_rollup(Path(f"/proc/{sample.pid}/smaps_rollup"))
    sample.rss_kib = rollup.get("Rss", 0)
    sample.pss_kib = rollup.get("Pss", 0)
    sample.pss_dirty_kib = rollup.get("Pss_Dirty", 0)
    sample.private_clean_kib = rollup.get("Private_Clean", 0)
    sample.private_dirty_kib = rollup.get("Private_Dirty", 0)
    sample.private_kib = sample.private_clean_kib + sample.private_dirty_kib
    sample.anonymous_kib = rollup.get("Anonymous", 0)


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


def apply_dgop_sample(sample: Sample, cursor: str, sample_cpu: bool) -> str:
    if not shutil.which("dgop"):
        return cursor
    command = ["dgop", "processes", "--json", "--limit", "0", "--sort", "memory"]
    if sample_cpu and cursor:
        command.extend(["--cursor", cursor])
    elif not sample_cpu:
        command.append("--no-cpu")
    result = subprocess.run(
        command,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=3,
    )
    if result.returncode != 0:
        return cursor
    try:
        payload = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return cursor
    next_cursor = str(payload.get("cursor") or cursor)
    for process in payload.get("processes") or []:
        if int(process.get("pid") or 0) != sample.pid:
            continue
        sample.dgop_memory_kib = int(process.get("memoryKB") or 0)
        sample.dgop_memory_calculation = str(process.get("memoryCalculation") or "")
        sample.dgop_pss_kib = int(process.get("pssKB") or 0)
        sample.dgop_pss_dirty_kib = int(
            process.get("pssDirtyKB")
            or process.get("pss_dirty_kb")
            or process.get("pssDirtyKb")
            or 0
        )
        sample.dgop_rss_kib = int(process.get("rssKB") or 0)
        sample.cpu_percent = float(process.get("cpu") or 0.0)
        sample.command = str(process.get("command") or "")
        break
    return next_cursor


def sample_status(
    gilderctl: str, socket: str, out_dir: Path, index: int, allow_missing: bool
) -> Path | None:
    status_path = out_dir / f"status-{index:03d}.json"
    env = os.environ.copy()
    if socket:
        env["GILDER_SOCKET"] = socket
    result = subprocess.run(
        [gilderctl, "status"],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        timeout=5,
    )
    if result.returncode != 0:
        if not allow_missing:
            (out_dir / f"status-{index:03d}.err").write_text(result.stderr)
        return None
    status_path.write_text(result.stdout)
    return status_path


def summarize(samples: list[Sample]) -> dict[str, Any]:
    if not samples:
        return {"sample_count": 0}
    return {
        "sample_count": len(samples),
        "max_rss_kib": max(s.rss_kib for s in samples),
        "max_pss_kib": max(s.pss_kib for s in samples),
        "max_pss_dirty_kib": max(s.pss_dirty_kib for s in samples),
        "max_private_dirty_kib": max(s.private_dirty_kib for s in samples),
        "max_private_kib": max(s.private_kib for s in samples),
        "max_dgop_memory_kib": max(s.dgop_memory_kib for s in samples),
        "last_rss_kib": samples[-1].rss_kib,
        "last_pss_kib": samples[-1].pss_kib,
        "last_pss_dirty_kib": samples[-1].pss_dirty_kib,
        "retained_pss_delta_kib": samples[-1].pss_kib - samples[0].pss_kib,
        "retained_private_delta_kib": samples[-1].private_kib - samples[0].private_kib,
    }


def validate(args: argparse.Namespace, summary: dict[str, Any]) -> list[str]:
    checks = [
        ("expect_max_rss_kib_at_most", "max_rss_kib"),
        ("expect_max_pss_kib_at_most", "max_pss_kib"),
        ("expect_max_pss_dirty_kib_at_most", "max_pss_dirty_kib"),
        ("expect_max_private_dirty_kib_at_most", "max_private_dirty_kib"),
        ("expect_max_private_kib_at_most", "max_private_kib"),
        ("expect_retained_pss_delta_kib_at_most", "retained_pss_delta_kib"),
        ("expect_retained_private_delta_kib_at_most", "retained_private_delta_kib"),
    ]
    failures: list[str] = []
    for arg_name, summary_name in checks:
        limit = getattr(args, arg_name)
        if limit is None:
            continue
        actual = int(summary.get(summary_name) or 0)
        if actual > limit:
            failures.append(f"{summary_name}={actual} > {limit}")
    if args.unsupported_expectations:
        failures.append(
            "unsupported expectation arguments: " + " ".join(args.unsupported_expectations)
        )
    return failures


if __name__ == "__main__":
    raise SystemExit(main())
