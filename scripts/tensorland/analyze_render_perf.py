#!/usr/bin/env -S uv run --script
"""Analyze self-contained Tensor frame-submit performance events."""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, TextIO


COMPOSITIONS = ("direct-single-pass", "backdrop-multi-pass")
BACKDROP_FIELDS = (
    "backdrop_passes",
    "backdrop_sample_pixels",
    "backdrop_filter_pixels",
    "backdrop_filter_texture_samples",
    "backdrop_composite_pixel_upper_bound",
    "backdrop_retained_intermediate_pixels",
)
SHADOW_FIELDS = ("shadow_draws", "shadow_pixel_upper_bound")
WORKLOAD_FIELDS = (*SHADOW_FIELDS, *BACKDROP_FIELDS)
INTEGER_FIELDS = (
    "output_device",
    "output_connector",
    "serial",
    "timeline",
    "output_pixels",
    *SHADOW_FIELDS,
    *BACKDROP_FIELDS,
    "elapsed_us",
)
REQUIRED_FIELDS = (*INTEGER_FIELDS, "composition")
ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
EVENT = re.compile(r"(?:^|:\s)frame submit(?:\s|$)")
FIELD = re.compile(r"(?P<key>[A-Za-z_][A-Za-z0-9_]*)=(?P<value>\"[^\"]*\"|\S+)")


class AnalysisError(ValueError):
    """A log cannot provide unambiguous frame-performance evidence."""


@dataclass(frozen=True)
class FrameSample:
    output_device: int
    output_connector: int
    serial: int
    timeline: int
    output_pixels: int
    shadow_draws: int
    shadow_pixel_upper_bound: int
    backdrop_passes: int
    backdrop_sample_pixels: int
    backdrop_filter_pixels: int
    backdrop_filter_texture_samples: int
    backdrop_composite_pixel_upper_bound: int
    backdrop_retained_intermediate_pixels: int
    elapsed_us: int
    composition: str


def _event_fields(line: str) -> dict[str, str] | None:
    clean = ANSI_ESCAPE.sub("", line)
    match = EVENT.search(clean)
    if match is None:
        return None
    fields: dict[str, str] = {}
    for field_match in FIELD.finditer(clean, match.end()):
        key = field_match.group("key")
        if key in fields:
            raise AnalysisError(f"duplicate field {key!r}")
        value = field_match.group("value")
        fields[key] = value[1:-1] if value.startswith('"') else value
    return fields


def parse_sample(line: str, location: str) -> FrameSample | None:
    try:
        fields = _event_fields(line)
    except AnalysisError as error:
        raise AnalysisError(f"{location}: {error}") from error
    if fields is None:
        return None

    missing = [name for name in REQUIRED_FIELDS if name not in fields]
    if missing:
        raise AnalysisError(f"{location}: frame submit is missing {', '.join(missing)}")
    values: dict[str, int | str] = {"composition": fields["composition"]}
    for name in INTEGER_FIELDS:
        raw = fields[name]
        if not raw.isdecimal():
            raise AnalysisError(f"{location}: {name} must be a non-negative integer")
        values[name] = int(raw)

    sample = FrameSample(**values)
    validate_sample(sample, location)
    return sample


def validate_sample(sample: FrameSample, location: str) -> None:
    if sample.composition not in COMPOSITIONS:
        raise AnalysisError(
            f"{location}: unsupported composition {sample.composition!r}"
        )
    if sample.serial == 0 or sample.timeline == 0:
        raise AnalysisError(f"{location}: serial and timeline must be positive")
    if sample.output_pixels == 0:
        raise AnalysisError(f"{location}: output_pixels must be positive")
    if (sample.shadow_draws == 0) != (sample.shadow_pixel_upper_bound == 0):
        raise AnalysisError(f"{location}: shadow draw count and pixel work must agree")
    if sample.shadow_pixel_upper_bound > sample.output_pixels * sample.shadow_draws:
        raise AnalysisError(f"{location}: shadow pixel work exceeds its output-clipped bounds")

    backdrop_values = [getattr(sample, name) for name in BACKDROP_FIELDS]
    if sample.composition == "direct-single-pass":
        if any(backdrop_values):
            raise AnalysisError(
                f"{location}: direct-single-pass must have zero backdrop workload"
            )
        return

    if any(value == 0 for value in backdrop_values):
        raise AnalysisError(
            f"{location}: backdrop-multi-pass requires non-zero backdrop workload"
        )
    if sample.backdrop_filter_pixels != sample.backdrop_sample_pixels * 2:
        raise AnalysisError(f"{location}: filter pixels must equal two sample lanes")
    if (
        sample.backdrop_filter_texture_samples
        != sample.backdrop_filter_pixels * 9
    ):
        raise AnalysisError(f"{location}: filter samples must describe fixed nine-tap work")
    full_sample_pixels = sample.output_pixels * sample.backdrop_passes
    if sample.backdrop_sample_pixels > full_sample_pixels:
        raise AnalysisError(
            f"{location}: local sample work exceeds full-output work for its passes"
        )
    if sample.backdrop_retained_intermediate_pixels > sample.output_pixels * 2:
        raise AnalysisError(
            f"{location}: retained two-lane capacity exceeds the output extent"
        )


def read_samples(paths: Iterable[Path]) -> list[FrameSample]:
    samples: list[FrameSample] = []
    for path in paths:
        try:
            with path.open(encoding="utf-8", errors="replace") as stream:
                for line_number, line in enumerate(stream, 1):
                    sample = parse_sample(line, f"{path}:{line_number}")
                    if sample is not None:
                        samples.append(sample)
        except OSError as error:
            raise AnalysisError(f"cannot read {path}: {error}") from error
    if not samples:
        raise AnalysisError("no self-contained 'frame submit' samples found")
    return samples


def nearest_rank(values: list[int], percentile: int) -> int:
    ordered = sorted(values)
    index = max(0, math.ceil(len(ordered) * percentile / 100) - 1)
    return ordered[index]


def summarize_group(samples: list[FrameSample]) -> dict[str, object]:
    elapsed = [sample.elapsed_us for sample in samples]
    totals = {
        field: sum(getattr(sample, field) for sample in samples)
        for field in WORKLOAD_FIELDS
    }
    full_sample_pixels = sum(
        sample.output_pixels * sample.backdrop_passes for sample in samples
    )
    retained_capacity_pixels = sum(sample.output_pixels * 2 for sample in samples)
    return {
        "frames": len(samples),
        "elapsed_us": {
            "p50": nearest_rank(elapsed, 50),
            "p95": nearest_rank(elapsed, 95),
            "p99": nearest_rank(elapsed, 99),
        },
        "workload_totals": totals,
        "sample_localization_percent": round(
            100 * totals["backdrop_sample_pixels"] / full_sample_pixels, 3
        )
        if full_sample_pixels
        else 0.0,
        "retained_capacity_percent": round(
            100
            * totals["backdrop_retained_intermediate_pixels"]
            / retained_capacity_pixels,
            3,
        ),
    }


def summarize(samples: list[FrameSample]) -> dict[str, object]:
    return {
        "schema": "tensor-render-perf-v2",
        "frames": len(samples),
        "groups": {
            composition: summarize_group(
                [sample for sample in samples if sample.composition == composition]
            )
            for composition in COMPOSITIONS
            if any(sample.composition == composition for sample in samples)
        },
    }


def write_text(summary: dict[str, object], stream: TextIO) -> None:
    print(f"Tensor render performance: {summary['frames']} frame(s)", file=stream)
    groups = summary["groups"]
    assert isinstance(groups, dict)
    for composition in COMPOSITIONS:
        if composition not in groups:
            continue
        group = groups[composition]
        assert isinstance(group, dict)
        elapsed = group["elapsed_us"]
        totals = group["workload_totals"]
        assert isinstance(elapsed, dict) and isinstance(totals, dict)
        print(
            f"{composition}: frames={group['frames']} "
            f"elapsed_us[p50={elapsed['p50']} p95={elapsed['p95']} p99={elapsed['p99']}]",
            file=stream,
        )
        print(
            "  workload "
            + " ".join(f"{name}={totals[name]}" for name in WORKLOAD_FIELDS),
            file=stream,
        )
        print(
            "  localization "
            f"sample/full={group['sample_localization_percent']:.3f}% "
            f"retained/full-two-lane={group['retained_capacity_percent']:.3f}%",
            file=stream,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", type=Path)
    parser.add_argument("--format", choices=("text", "json"), default="text")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = summarize(read_samples(args.logs))
    except AnalysisError as error:
        print(f"render performance analysis failed: {error}", file=sys.stderr)
        return 2
    if args.format == "json":
        json.dump(result, sys.stdout, indent=2, sort_keys=True)
        print()
    else:
        write_text(result, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
