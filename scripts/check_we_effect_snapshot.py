#!/usr/bin/env python3
"""Validate WE first-class effect graph invariants from a Gilder snapshot."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any


DEFAULT_BODY_LAYERS = ("node-59-models-json", "node-67-models-json")


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def runtime_snapshot(data: Any) -> dict[str, Any]:
    if isinstance(data, dict):
        nested = data.get("snapshot")
        if isinstance(nested, dict) and isinstance(nested.get("runtime"), dict):
            return nested["runtime"]
        if isinstance(data.get("runtime"), dict):
            return data["runtime"]
        return data
    raise SystemExit("snapshot root is not a JSON object")


def f64(value: Any, default: float = 0.0) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError):
        return default
    return result if math.isfinite(result) else default


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def assert_close(label: str, actual: float, expected: float, eps: float = 1.0e-3) -> None:
    if abs(actual - expected) > eps:
        fail(f"{label}: expected {expected:.6f}, got {actual:.6f}")


def layer_sizes(runtime: dict[str, Any]) -> dict[str, tuple[float, float]]:
    sizes: dict[str, tuple[float, float]] = {}
    for op in runtime.get("draw_ops") or []:
        layer_id = op.get("layer_id")
        if not isinstance(layer_id, str):
            continue
        width = f64(op.get("width"), -1.0)
        height = f64(op.get("height"), -1.0)
        if width > 0.0 and height > 0.0:
            sizes[layer_id] = (width, height)
    return sizes


def assert_effect_targets_cover_base_geometry(
    runtime: dict[str, Any],
    sizes: dict[str, tuple[float, float]],
    body_layers: tuple[str, ...],
) -> None:
    targets = runtime.get("draw_pass_sampled_image_we_graph_targets") or []
    if not isinstance(targets, list):
        fail("draw_pass_sampled_image_we_graph_targets is not an array")
    steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    vertices = runtime.get("draw_pass_sampled_image_vertices") or []
    if not isinstance(steps, list) or not isinstance(vertices, list):
        fail("recording steps or sampled-image vertices are not arrays")
    targets_by_index = {
        int(target.get("target_index")): target
        for target in targets
        if target.get("target_index") is not None
    }
    body_hits = {layer_id: 0 for layer_id in body_layers}
    for target in targets:
        layer_id = str(target.get("layer_id"))
        local_left = f64(target.get("local_left"))
        local_top = f64(target.get("local_top"))
        if layer_id in sizes and str(target.get("endpoint", "")).startswith("image-local-"):
            scale = f64(target.get("scale"), 1.0) if target.get("scale") is not None else 1.0
            width = int(target.get("width") or 0)
            height = int(target.get("height") or 0)
            nominal_width = math.ceil(sizes[layer_id][0] * scale)
            nominal_height = math.ceil(sizes[layer_id][1] * scale)
            if width < nominal_width or height < nominal_height:
                fail(
                    f"{layer_id} target {target.get('target_index')} size {width}x{height} "
                    f"is smaller than nominal {nominal_width}x{nominal_height}"
                )
        if layer_id in body_hits:
            body_hits[layer_id] += 1
    missing = [layer_id for layer_id, count in body_hits.items() if count == 0]
    if missing:
        fail(f"missing body graph targets: {', '.join(missing)}")
    if not steps or not vertices:
        return
    for layer_id in body_layers:
        base_steps = [
            step
            for step in steps
            if step.get("layer_id") == layer_id
            and step.get("we_graph_step_index") == 0
            and isinstance(step.get("render_target"), dict)
            and step["render_target"].get("type") == "effect-target"
        ]
        if len(base_steps) != 1:
            fail(f"{layer_id} has {len(base_steps)} base effect-target steps, expected 1")
        step = base_steps[0]
        target_index = int(step["render_target"].get("target_index"))
        target = targets_by_index.get(target_index)
        if target is None:
            fail(f"{layer_id} base step references missing target {target_index}")
        first = int(step.get("first_vertex") or 0)
        count = int(step.get("vertex_count") or 0)
        if count <= 0 or first < 0 or first + count > len(vertices):
            fail(f"{layer_id} base step vertex range is invalid")
        positions = [
            vertex.get("position")
            for vertex in vertices[first : first + count]
            if isinstance(vertex, dict) and isinstance(vertex.get("position"), list)
        ]
        if len(positions) != count:
            fail(f"{layer_id} base step has malformed vertex positions")
        xs = [f64(position[0], math.nan) for position in positions]
        ys = [f64(position[1], math.nan) for position in positions]
        if any(not math.isfinite(value) for value in xs + ys):
            fail(f"{layer_id} base step has non-finite vertex positions")
        width = f64(target.get("width"), -1.0)
        height = f64(target.get("height"), -1.0)
        eps = 1.0e-3
        if min(xs) < -eps or min(ys) < -eps or max(xs) > width + eps or max(ys) > height + eps:
            fail(
                f"{layer_id} base mesh bbox "
                f"({min(xs):.3f},{min(ys):.3f})..({max(xs):.3f},{max(ys):.3f}) "
                f"escapes target {target_index} {width:.0f}x{height:.0f}"
            )
        if layer_id in DEFAULT_BODY_LAYERS and width <= sizes[layer_id][0]:
            fail(f"{layer_id} body target was not expanded horizontally ({width:.0f}px)")
        assert_body_effect_pass_quads_use_layer_uvs(
            layer_id, sizes[layer_id], steps, vertices, targets_by_index
        )


def vertex_range(
    step: dict[str, Any],
    vertices: list[Any],
    layer_id: str,
    label: str,
) -> list[dict[str, Any]]:
    first = int(step.get("first_vertex") or 0)
    count = int(step.get("vertex_count") or 0)
    if count <= 0 or first < 0 or first + count > len(vertices):
        fail(f"{layer_id} {label} vertex range is invalid")
    selected = vertices[first : first + count]
    if not all(isinstance(vertex, dict) for vertex in selected):
        fail(f"{layer_id} {label} has malformed vertices")
    return selected


def vertex_component_bounds(
    selected: list[dict[str, Any]],
    field: str,
    component: int,
    layer_id: str,
    label: str,
) -> tuple[float, float]:
    values = []
    for vertex in selected:
        value = vertex.get(field)
        if not isinstance(value, list) or len(value) <= component:
            fail(f"{layer_id} {label} has malformed {field}")
        number = f64(value[component], math.nan)
        if not math.isfinite(number):
            fail(f"{layer_id} {label} has non-finite {field}")
        values.append(number)
    return min(values), max(values)


def assert_body_effect_pass_quads_use_layer_uvs(
    layer_id: str,
    layer_size: tuple[float, float],
    steps: list[Any],
    vertices: list[Any],
    targets_by_index: dict[int, dict[str, Any]],
) -> None:
    layer_steps = [
        step for step in steps if isinstance(step, dict) and step.get("layer_id") == layer_id
    ]
    for step in layer_steps:
        step_index = int(step.get("we_graph_step_index") or -1)
        target_info = step.get("render_target")
        if not isinstance(target_info, dict):
            continue
        selected = vertex_range(step, vertices, layer_id, f"step {step_index}")
        if step_index > 0 and target_info.get("type") == "effect-target":
            if int(step.get("vertex_count") or 0) != 4:
                continue
            target_index = int(target_info.get("target_index"))
            target = targets_by_index.get(target_index)
            if target is None:
                fail(f"{layer_id} step {step_index} references missing target {target_index}")
            width = f64(target.get("width"), -1.0)
            height = f64(target.get("height"), -1.0)
            min_x, max_x = vertex_component_bounds(
                selected, "position", 0, layer_id, f"step {step_index}"
            )
            min_y, max_y = vertex_component_bounds(
                selected, "position", 1, layer_id, f"step {step_index}"
            )
            assert_close(f"{layer_id} step {step_index} target min_x", min_x, 0.0)
            assert_close(f"{layer_id} step {step_index} target max_x", max_x, width)
            assert_close(f"{layer_id} step {step_index} target min_y", min_y, 0.0)
            assert_close(f"{layer_id} step {step_index} target max_y", max_y, height)
            min_u, max_u = vertex_component_bounds(
                selected, "effect_uv", 0, layer_id, f"step {step_index}"
            )
            min_v, max_v = vertex_component_bounds(
                selected, "effect_uv", 1, layer_id, f"step {step_index}"
            )
            assert_layer_uv_bounds(
                layer_id,
                f"step {step_index}",
                layer_size,
                target,
                min_u,
                max_u,
                min_v,
                max_v,
            )
        if target_info.get("type") == "swapchain" and int(step.get("vertex_count") or 0) == 4:
            min_u, max_u = vertex_component_bounds(
                selected, "uv", 0, layer_id, f"step {step_index}"
            )
            min_v, max_v = vertex_component_bounds(
                selected, "uv", 1, layer_id, f"step {step_index}"
            )
            assert_close(f"{layer_id} final source uv min_u", min_u, 0.0)
            assert_close(f"{layer_id} final source uv max_u", max_u, 1.0)
            assert_close(f"{layer_id} final source uv min_v", min_v, 0.0)
            assert_close(f"{layer_id} final source uv max_v", max_v, 1.0)
            input_target_index = step.get("we_graph_input_target_index")
            if input_target_index is not None:
                target = targets_by_index.get(int(input_target_index))
                if target is None:
                    fail(
                        f"{layer_id} final step references missing input target {input_target_index}"
                    )
                min_eu, max_eu = vertex_component_bounds(
                    selected, "effect_uv", 0, layer_id, f"step {step_index}"
                )
                min_ev, max_ev = vertex_component_bounds(
                    selected, "effect_uv", 1, layer_id, f"step {step_index}"
                )
                assert_layer_uv_bounds(
                    layer_id,
                    f"final step {step_index}",
                    layer_size,
                    target,
                    min_eu,
                    max_eu,
                    min_ev,
                    max_ev,
                )


def assert_layer_uv_bounds(
    layer_id: str,
    label: str,
    layer_size: tuple[float, float],
    target: dict[str, Any],
    min_u: float,
    max_u: float,
    min_v: float,
    max_v: float,
) -> None:
    layer_width, layer_height = layer_size
    if layer_width <= 0.0 or layer_height <= 0.0:
        fail(f"{layer_id} {label} has invalid layer size")
    left = f64(target.get("local_left"))
    top = f64(target.get("local_top"))
    width = f64(target.get("width"), -1.0)
    height = f64(target.get("height"), -1.0)
    expected_min_u = left / layer_width
    expected_max_u = (left + width) / layer_width
    expected_min_v = 1.0 - (top + height) / layer_height
    expected_max_v = 1.0 - top / layer_height
    assert_close(f"{layer_id} {label} effect_uv min_u", min_u, expected_min_u)
    assert_close(f"{layer_id} {label} effect_uv max_u", max_u, expected_max_u)
    assert_close(f"{layer_id} {label} effect_uv min_v", min_v, expected_min_v)
    assert_close(f"{layer_id} {label} effect_uv max_v", max_v, expected_max_v)


def assert_single_body_scene_composite(
    runtime: dict[str, Any],
    body_layers: tuple[str, ...],
) -> None:
    steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    if not steps:
        return
    for layer_id in body_layers:
        scene_steps = [
            step
            for step in steps
            if step.get("layer_id") == layer_id
            and isinstance(step.get("render_target"), dict)
            and step["render_target"].get("type") == "swapchain"
        ]
        if len(scene_steps) != 1:
            fail(f"{layer_id} has {len(scene_steps)} swapchain composites, expected 1")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("snapshot", type=Path)
    parser.add_argument(
        "--body-layer",
        action="append",
        dest="body_layers",
        help="Layer id that must keep nominal 1571x2621 graph targets; may repeat.",
    )
    args = parser.parse_args()

    runtime = runtime_snapshot(load_json(args.snapshot))
    body_layers = tuple(args.body_layers or DEFAULT_BODY_LAYERS)
    sizes = layer_sizes(runtime)
    assert_effect_targets_cover_base_geometry(runtime, sizes, body_layers)
    assert_single_body_scene_composite(runtime, body_layers)
    print(
        "PASS: WE effect targets cover retained body geometry when present and body layers composite once"
    )


if __name__ == "__main__":
    main()
