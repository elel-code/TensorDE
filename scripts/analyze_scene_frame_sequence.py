#!/usr/bin/env python3
"""Analyze temporal motion in an in-process scene frame sequence."""

from __future__ import annotations

import argparse
import csv
import glob
import json
import re
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont, ImageOps


@dataclass(frozen=True)
class Region:
    name: str
    x: int
    y: int
    width: int
    height: int


def main() -> int:
    args = parse_args()
    frames = sorted((Path(value) for value in glob.glob(args.frames)), key=frame_number)
    if len(frames) < 3:
        raise ValueError("frame sequence analysis requires at least three frames")

    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    first = load_rgb(frames[0])
    height, width, _ = first.shape
    regions = [parse_region(value, width, height) for value in args.region]
    if not regions:
        regions = [Region("full", 0, 0, width, height)]

    profiles = {
        region.name: {
            "x_mean": [],
            "x_p95": [],
            "x_changed": [],
            "y_mean": [],
        }
        for region in regions
    }
    pair_rows: list[dict[str, float | int | str]] = []
    strongest_differences: list[tuple[float, int, np.ndarray]] = []
    previous = first

    for pair_index, path in enumerate(frames[1:]):
        current = load_rgb(path)
        if current.shape != first.shape:
            raise ValueError(f"frame extent changed at {path}: {current.shape}")
        difference = np.abs(current.astype(np.int16) - previous.astype(np.int16))
        difference = difference.mean(axis=2).astype(np.float32)
        full_energy = float(difference.mean())
        strongest_differences.append((full_energy, pair_index, difference))

        row: dict[str, float | int | str] = {
            "pair_index": pair_index,
            "previous_frame": frame_number(frames[pair_index]),
            "current_frame": frame_number(path),
            "full_mean_difference": full_energy,
        }
        for region in regions:
            view = difference[
                region.y : region.y + region.height,
                region.x : region.x + region.width,
            ]
            region_profiles = profiles[region.name]
            x_mean = view.mean(axis=0)
            x_p95 = np.percentile(view, 95, axis=0)
            x_changed = (view >= args.change_threshold).mean(axis=0)
            y_mean = view.mean(axis=1)
            region_profiles["x_mean"].append(x_mean)
            region_profiles["x_p95"].append(x_p95)
            region_profiles["x_changed"].append(x_changed)
            region_profiles["y_mean"].append(y_mean)

            smooth = smooth_profile(x_p95, args.smoothing_width)
            baseline = float(np.percentile(smooth, 20))
            weights = np.maximum(smooth - baseline, 0.0)
            local_positions = np.arange(region.width, dtype=np.float32)
            weight_sum = float(weights.sum())
            centroid = (
                float(np.dot(weights, local_positions) / weight_sum)
                if weight_sum > 0.0
                else 0.0
            )
            peak = int(np.argmax(smooth))
            prefix = region.name
            row[f"{prefix}_mean_difference"] = float(view.mean())
            row[f"{prefix}_peak_x"] = region.x + peak
            row[f"{prefix}_centroid_x"] = region.x + centroid
        pair_rows.append(row)
        previous = current

    write_pair_metrics(output / "adjacent-frame-motion.csv", pair_rows)
    region_reports: dict[str, object] = {}
    for region in regions:
        arrays = {
            name: np.asarray(rows, dtype=np.float32)
            for name, rows in profiles[region.name].items()
        }
        for name, values in arrays.items():
            save_heatmap(
                output / f"{region.name}-{name.replace('_', '-')}-time.png",
                values,
            )
        region_reports[region.name] = analyze_sweeps(
            pair_rows,
            region,
            args.sweep_window,
        )

    strongest_differences.sort(key=lambda item: item[0], reverse=True)
    save_difference_contact_sheet(
        output / "strongest-adjacent-differences.png",
        strongest_differences[: min(12, len(strongest_differences))],
        frames,
    )
    report = {
        "frame_count": len(frames),
        "first_frame": frame_number(frames[0]),
        "last_frame": frame_number(frames[-1]),
        "width": width,
        "height": height,
        "change_threshold": args.change_threshold,
        "smoothing_width": args.smoothing_width,
        "regions": {
            region.name: {
                "bounds": [region.x, region.y, region.width, region.height],
                "sweep_analysis": region_reports[region.name],
            }
            for region in regions
        },
        "strongest_pairs": [
            {
                "mean_difference": energy,
                "previous_frame": frame_number(frames[index]),
                "current_frame": frame_number(frames[index + 1]),
            }
            for energy, index, _ in strongest_differences[:12]
        ],
    }
    (output / "sequence-motion-summary.json").write_text(
        json.dumps(report, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(report, indent=2))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frames", required=True, help="frame glob relative to cwd")
    parser.add_argument("--output", required=True)
    parser.add_argument(
        "--region",
        action="append",
        default=[],
        metavar="NAME=X,Y,WIDTH,HEIGHT",
    )
    parser.add_argument("--change-threshold", type=float, default=6.0)
    parser.add_argument("--smoothing-width", type=int, default=31)
    parser.add_argument("--sweep-window", type=int, default=12)
    return parser.parse_args()


def frame_number(path: Path) -> int:
    matches = re.findall(r"\d+", path.stem)
    if not matches:
        raise ValueError(f"frame path has no numeric index: {path}")
    return int(matches[-1])


def parse_region(value: str, image_width: int, image_height: int) -> Region:
    try:
        name, bounds = value.split("=", 1)
        x, y, width, height = (int(part) for part in bounds.split(","))
    except ValueError as error:
        raise ValueError(f"invalid region {value!r}") from error
    if not name or width <= 0 or height <= 0:
        raise ValueError(f"invalid region {value!r}")
    if x < 0 or y < 0 or x + width > image_width or y + height > image_height:
        raise ValueError(f"region outside {image_width}x{image_height}: {value!r}")
    return Region(name, x, y, width, height)


def load_rgb(path: Path) -> np.ndarray:
    with Image.open(path) as image:
        return np.asarray(image.convert("RGB"), dtype=np.uint8)


def smooth_profile(values: np.ndarray, width: int) -> np.ndarray:
    width = max(1, min(width, len(values)))
    kernel = np.full(width, 1.0 / width, dtype=np.float32)
    return np.convolve(values, kernel, mode="same")


def write_pair_metrics(path: Path, rows: list[dict[str, object]]) -> None:
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.DictWriter(output, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def save_heatmap(path: Path, values: np.ndarray) -> None:
    finite = values[np.isfinite(values)]
    high = float(np.percentile(finite, 99.5)) if finite.size else 1.0
    low = float(np.percentile(finite, 5.0)) if finite.size else 0.0
    scale = max(high - low, 1e-6)
    normalized = np.clip((values - low) / scale, 0.0, 1.0)
    pixels = np.round(normalized * 255.0).astype(np.uint8)
    image = Image.fromarray(pixels, mode="L")
    image = ImageOps.colorize(image, black="#050816", white="#ffed6f", mid="#e84a5f")
    image = image.resize((image.width, max(image.height * 8, 256)), Image.Resampling.NEAREST)
    image.save(path)


def analyze_sweeps(
    rows: list[dict[str, object]],
    region: Region,
    window: int,
) -> dict[str, object]:
    key = f"{region.name}_centroid_x"
    positions = np.asarray([float(row[key]) for row in rows], dtype=np.float64)
    times = np.arange(len(positions), dtype=np.float64)
    correlation = float(np.corrcoef(times, positions)[0, 1])
    windows: list[dict[str, float | int]] = []
    window = max(3, min(window, len(positions)))
    local_time = np.arange(window, dtype=np.float64)
    for start in range(0, len(positions) - window + 1):
        sample = positions[start : start + window]
        slope, intercept = np.polyfit(local_time, sample, 1)
        predicted = slope * local_time + intercept
        residual = float(np.sum((sample - predicted) ** 2))
        variance = float(np.sum((sample - sample.mean()) ** 2))
        r_squared = 1.0 - residual / variance if variance > 1e-9 else 0.0
        windows.append(
            {
                "start_pair": start,
                "end_pair": start + window - 1,
                "slope_pixels_per_pair": float(slope),
                "r_squared": r_squared,
                "travel_pixels": float(sample[-1] - sample[0]),
            }
        )
    windows.sort(
        key=lambda item: abs(float(item["slope_pixels_per_pair"]))
        * max(float(item["r_squared"]), 0.0),
        reverse=True,
    )
    return {
        "whole_sequence_time_correlation": correlation,
        "centroid_min_x": float(positions.min()),
        "centroid_max_x": float(positions.max()),
        "strongest_monotonic_windows": windows[:8],
    }


def save_difference_contact_sheet(
    path: Path,
    differences: list[tuple[float, int, np.ndarray]],
    frames: list[Path],
) -> None:
    if not differences:
        return
    width = 320
    height = 200
    label_height = 24
    sheet = Image.new("RGB", (width * 3, (height + label_height) * len(differences)))
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default()
    for row, (energy, pair_index, difference) in enumerate(differences):
        previous = Image.open(frames[pair_index]).convert("RGB")
        current = Image.open(frames[pair_index + 1]).convert("RGB")
        high = max(float(np.percentile(difference, 99.5)), 1.0)
        enhanced = np.clip(difference / high * 255.0, 0.0, 255.0).astype(np.uint8)
        difference_image = ImageOps.colorize(
            Image.fromarray(enhanced, mode="L"),
            black="#000000",
            white="#fff36d",
            mid="#e84057",
        ).convert("RGB")
        for column, image in enumerate((previous, current, difference_image)):
            image.thumbnail((width, height), Image.Resampling.LANCZOS)
            x = column * width + (width - image.width) // 2
            y = row * (height + label_height)
            sheet.paste(image, (x, y))
        label = (
            f"{frame_number(frames[pair_index])} -> "
            f"{frame_number(frames[pair_index + 1])}, mean={energy:.4f}"
        )
        draw.text((4, row * (height + label_height) + height + 4), label, font=font)
        previous.close()
        current.close()
    sheet.save(path)


if __name__ == "__main__":
    raise SystemExit(main())
