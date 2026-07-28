#!/usr/bin/env python3
"""Measure count, brightness, and spatial distribution in isolated particle captures."""

from __future__ import annotations

import argparse
import glob
import json
import re
from pathlib import Path

import numpy as np
from PIL import Image


def main() -> int:
    args = parse_args()
    frames = sorted((Path(path) for path in glob.glob(args.frames)), key=frame_number)
    if not frames:
        raise ValueError(f"particle frame glob matched nothing: {args.frames}")

    rows = [analyze_frame(path, args.threshold, args.bins) for path in frames]
    steady_count = min(args.steady_frames, len(rows))
    steady = rows[-steady_count:]
    report = {
        "frame_count": len(rows),
        "first_frame": frame_number(frames[0]),
        "last_frame": frame_number(frames[-1]),
        "threshold": args.threshold,
        "bins": args.bins,
        "steady_frame_count": steady_count,
        "steady_medians": aggregate(steady),
        "steady_temporal_variation": temporal_variation(steady),
        "frames": rows,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["steady_medians"], indent=2))
    print(json.dumps(report["steady_temporal_variation"], indent=2))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frames", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--threshold", type=int, default=4)
    parser.add_argument("--bins", type=int, default=8)
    parser.add_argument("--steady-frames", type=int, default=3)
    args = parser.parse_args()
    if not 0 <= args.threshold <= 255:
        parser.error("--threshold must be in 0..255")
    if args.bins < 2 or args.steady_frames < 1:
        parser.error("--bins must be at least 2 and --steady-frames must be positive")
    return args


def frame_number(path: Path) -> int:
    matches = re.findall(r"\d+", path.stem)
    if not matches:
        raise ValueError(f"frame path has no numeric index: {path}")
    return int(matches[-1])


def analyze_frame(path: Path, threshold: int, bins: int) -> dict[str, object]:
    with Image.open(path) as image:
        rgb = np.asarray(image.convert("RGB"), dtype=np.uint8)
    intensity = rgb.max(axis=2)
    mask = intensity >= threshold
    height, width = mask.shape
    active = intensity[mask]
    ys, xs = np.nonzero(mask)
    component_areas = connected_component_areas(mask)
    x_counts = np.bincount(xs * bins // width, minlength=bins)
    y_counts = np.bincount(ys * bins // height, minlength=bins)
    active_count = int(active.size)
    center_x = int(((xs >= width // 4) & (xs < width * 3 // 4)).sum())
    center_y = int(((ys >= height // 4) & (ys < height * 3 // 4)).sum())
    weights = active.astype(np.float64)
    weight_sum = float(weights.sum())
    return {
        "frame": frame_number(path),
        "path": str(path),
        "width": width,
        "height": height,
        "active_pixels": active_count,
        "coverage_ratio": active_count / mask.size,
        "intensity_sum": int(active.astype(np.uint64).sum()),
        "intensity_mean": float(active.mean()) if active_count else 0.0,
        "intensity_p50": percentile(active, 50),
        "intensity_p90": percentile(active, 90),
        "intensity_p99": percentile(active, 99),
        "component_count": len(component_areas),
        "component_area_p50": percentile(np.asarray(component_areas), 50),
        "component_area_p90": percentile(np.asarray(component_areas), 90),
        "center_half_x_fraction": center_x / active_count if active_count else 0.0,
        "center_half_y_fraction": center_y / active_count if active_count else 0.0,
        "x_bin_coefficient_of_variation": coefficient_of_variation(x_counts),
        "y_bin_coefficient_of_variation": coefficient_of_variation(y_counts),
        "x_bin_active_pixels": x_counts.tolist(),
        "y_bin_active_pixels": y_counts.tolist(),
        "intensity_weighted_centroid": [
            float(np.dot(xs, weights) / weight_sum) if weight_sum else 0.0,
            float(np.dot(ys, weights) / weight_sum) if weight_sum else 0.0,
        ],
    }


def connected_component_areas(mask: np.ndarray) -> list[int]:
    """Return 8-connected component areas using row runs instead of per-pixel objects."""
    parents: list[int] = []
    areas: list[int] = []
    previous: list[tuple[int, int, int]] = []
    for row in mask:
        padded = np.pad(row.astype(np.int8), (1, 1))
        transitions = np.diff(padded)
        starts = np.flatnonzero(transitions == 1)
        ends = np.flatnonzero(transitions == -1) - 1
        current: list[tuple[int, int, int]] = []
        previous_index = 0
        for start_value, end_value in zip(starts, ends):
            start = int(start_value)
            end = int(end_value)
            label = len(parents)
            parents.append(label)
            areas.append(end - start + 1)
            while previous_index < len(previous) and previous[previous_index][1] < start - 1:
                previous_index += 1
            overlap = previous_index
            while overlap < len(previous) and previous[overlap][0] <= end + 1:
                label = union(parents, areas, label, previous[overlap][2])
                overlap += 1
            current.append((start, end, label))
        previous = current
    return [areas[index] for index, parent in enumerate(parents) if index == parent]


def find(parents: list[int], value: int) -> int:
    while parents[value] != value:
        parents[value] = parents[parents[value]]
        value = parents[value]
    return value


def union(parents: list[int], areas: list[int], left: int, right: int) -> int:
    left = find(parents, left)
    right = find(parents, right)
    if left == right:
        return left
    if areas[left] < areas[right]:
        left, right = right, left
    parents[right] = left
    areas[left] += areas[right]
    return left


def percentile(values: np.ndarray, quantile: int) -> float:
    return float(np.percentile(values, quantile)) if values.size else 0.0


def coefficient_of_variation(values: np.ndarray) -> float:
    mean = float(values.mean())
    return float(values.std() / mean) if mean else 0.0


def aggregate(rows: list[dict[str, object]]) -> dict[str, object]:
    keys = [
        "active_pixels",
        "coverage_ratio",
        "intensity_sum",
        "intensity_mean",
        "intensity_p50",
        "intensity_p90",
        "intensity_p99",
        "component_count",
        "component_area_p50",
        "component_area_p90",
        "center_half_x_fraction",
        "center_half_y_fraction",
        "x_bin_coefficient_of_variation",
        "y_bin_coefficient_of_variation",
    ]
    return {
        key: float(np.median([float(row[key]) for row in rows]))
        for key in keys
    }


def temporal_variation(rows: list[dict[str, object]]) -> dict[str, object]:
    keys = [
        "active_pixels",
        "intensity_sum",
        "intensity_mean",
        "component_count",
    ]
    result: dict[str, object] = {}
    for key in keys:
        values = np.asarray([float(row[key]) for row in rows], dtype=np.float64)
        mean = float(values.mean())
        result[key] = {
            "minimum": float(values.min()),
            "maximum": float(values.max()),
            "mean": mean,
            "coefficient_of_variation": float(values.std() / mean) if mean else 0.0,
            "peak_to_peak_over_mean": float(np.ptp(values) / mean) if mean else 0.0,
        }
    return result


if __name__ == "__main__":
    raise SystemExit(main())
