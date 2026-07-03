#!/usr/bin/env python3
"""Validate WE first-class effect graph invariants from a Gilder snapshot."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any


DEFAULT_BODY_LAYERS = (
    "node-59-models-json",
    "node-60-models-json",
    "node-67-models-json",
    "node-68-models-json",
)
DEFAULT_SKIRT_RIBBON_LAYERS = ("node-60-models-json", "node-68-models-json")
DEFAULT_SKIRT_RIBBON_MASKS = (
    "waterwaves_mask_5779d462",
    "waterwaves_mask_ea0aa530",
    "waterwaves_mask_6eb46628",
)
DEFAULT_INVISIBLE_EFFECT_LAYERS = ("node-48-models-6-json", "node-56-models-6-json")
DEFAULT_COMPOSELAYER = "node-7-models-util-composelayer-json"
DEFAULT_SLIDER_RECTANGLES = (
    "node-33-models-util-solidlayer-json",
    "node-91-models-util-solidlayer-json",
)
DEFAULT_SCROLLING_TEXT_LAYERS = (
    "node-28-text",
    "node-29-text",
    "node-30-text",
)
DEFAULT_TRANSPARENT_COLORKEY_TEXT_LAYER = "node-29-text"
DEFAULT_SCROLL_DISPLACEMENT_TEXT_LAYER = "node-30-text"
AUTOSWAY_EFFECT = "effects/workshop/3392386920/auto_sway/effect.json"
WATERWAVES_EFFECT = "effects/waterwaves/effect.json"
SCROLL_EFFECT = "effects/scroll/effect.json"
COLORKEY_EFFECT = "effects/colorkey/effect.json"
CLIPPING_MASK_EFFECT_FRAGMENT = "clipping_mask/effect.json"


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


def assert_we_orthogonal_projection_uses_stretch(runtime: dict[str, Any]) -> None:
    scene_size = runtime.get("scene_size") or {}
    width = f64(scene_size.get("width"), -1.0)
    height = f64(scene_size.get("height"), -1.0)
    if width <= 0.0 or height <= 0.0:
        return
    fit = runtime.get("scene_fit")
    if fit != "stretch":
        fail(
            f"scene_fit is {fit!r}, expected 'stretch': WE orthogonalprojection maps "
            f"0..{width:.0f} x 0..{height:.0f} directly to the viewport, while cover crops "
            "scene X on non-16:9 outputs"
        )


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
        if layer_id in DEFAULT_BODY_LAYERS:
            min_u, max_u = vertex_component_bounds(
                vertices[first : first + count], "effect_uv", 0, layer_id, "base layer-uv domain"
            )
            min_v, max_v = vertex_component_bounds(
                vertices[first : first + count], "effect_uv", 1, layer_id, "base layer-uv domain"
            )
            if min_u > eps or max_u < 1.0 - eps or min_v > eps or max_v < 1.0 - eps:
                fail(
                    f"{layer_id} waterwaves target domain does not cover the authored layer UVs "
                    f"({min_u:.6f}..{max_u:.6f}, {min_v:.6f}..{max_v:.6f})"
                )
            assert_layer_mask_uv_bounds(
                layer_id,
                "base layer-uv domain",
                sizes[layer_id],
                target,
                min_u,
                max_u,
                min_v,
                max_v,
            )
        assert_body_effect_pass_quads_use_pass_space_source_and_layer_mask_uvs(
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


def assert_body_effect_pass_quads_use_pass_space_source_and_layer_mask_uvs(
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
            min_su, max_su = vertex_component_bounds(
                selected, "uv", 0, layer_id, f"step {step_index}"
            )
            min_sv, max_sv = vertex_component_bounds(
                selected, "uv", 1, layer_id, f"step {step_index}"
            )
            assert_pass_space_uv_bounds(
                layer_id, f"step {step_index} source", min_su, max_su, min_sv, max_sv
            )
            min_u, max_u = vertex_component_bounds(
                selected, "effect_uv", 0, layer_id, f"step {step_index}"
            )
            min_v, max_v = vertex_component_bounds(
                selected, "effect_uv", 1, layer_id, f"step {step_index}"
            )
            assert_layer_mask_uv_bounds(
                layer_id, f"step {step_index}", layer_size, target, min_u, max_u, min_v, max_v
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
                assert_pass_space_uv_bounds(
                    layer_id, f"final step {step_index} source", min_u, max_u, min_v, max_v
                )
                assert_layer_mask_uv_bounds(
                    layer_id,
                    f"final step {step_index}",
                    layer_size,
                    target,
                    min_eu,
                    max_eu,
                    min_ev,
                    max_ev,
                )


def assert_skirt_ribbon_uses_layer_uv_domain_puppet_waterwaves(
    runtime: dict[str, Any],
    sizes: dict[str, tuple[float, float]],
) -> None:
    steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    vertices = runtime.get("draw_pass_sampled_image_vertices") or []
    targets = runtime.get("draw_pass_sampled_image_we_graph_targets") or []
    graph_steps = runtime.get("draw_pass_sampled_image_we_graph_steps") or []
    if (
        not isinstance(steps, list)
        or not isinstance(vertices, list)
        or not isinstance(targets, list)
        or not isinstance(graph_steps, list)
    ):
        fail("skirt ribbon overhang validation requires recording and graph arrays")
    targets_by_index = {
        int(target.get("target_index")): target
        for target in targets
        if isinstance(target, dict) and target.get("target_index") is not None
    }
    for layer_id in DEFAULT_SKIRT_RIBBON_LAYERS:
        layer_size = sizes.get(layer_id)
        if layer_size is None:
            fail(f"{layer_id} is missing from draw_ops sizes")
        waterwaves_steps = [
            step
            for step in graph_steps
            if isinstance(step, dict)
            and step.get("layer_id") == layer_id
            and isinstance(step.get("pass"), dict)
            and step["pass"].get("effect_kind") == "water-waves"
        ]
        if len(waterwaves_steps) != 3:
            fail(
                f"{layer_id} expected 3 visible skirt-ribbon waterwaves passes, "
                f"got {len(waterwaves_steps)}"
            )
        mask_sources = []
        for step in waterwaves_steps:
            for slot in step["pass"].get("texture_slots") or []:
                if isinstance(slot, dict) and int(slot.get("slot") or -1) == 1:
                    mask_sources.append(str(slot.get("source") or "").replace("-", "_"))
        for mask in DEFAULT_SKIRT_RIBBON_MASKS:
            if not any(mask in source for source in mask_sources):
                fail(f"{layer_id} is missing skirt-ribbon mask {mask}")
        base_steps = [
            step
            for step in steps
            if isinstance(step, dict)
            and step.get("layer_id") == layer_id
            and step.get("we_graph_step_index") == 0
            and isinstance(step.get("render_target"), dict)
            and step["render_target"].get("type") == "effect-target"
        ]
        if len(base_steps) != 1:
            fail(f"{layer_id} has {len(base_steps)} skirt-ribbon base target steps, expected 1")
        base_step = base_steps[0]
        target_index = int(base_step["render_target"].get("target_index"))
        target = targets_by_index.get(target_index)
        if target is None:
            fail(f"{layer_id} base step references missing target {target_index}")
        layer_width, layer_height = layer_size
        selected = vertex_range(base_step, vertices, layer_id, "skirt-ribbon base step")
        if len(selected) != 4:
            fail(f"{layer_id} base waterwaves source copy has {len(selected)} vertices, expected 4")
        min_base_u, max_base_u = vertex_component_bounds(
            selected, "effect_uv", 0, layer_id, "skirt-ribbon base step"
        )
        min_base_v, max_base_v = vertex_component_bounds(
            selected, "effect_uv", 1, layer_id, "skirt-ribbon base step"
        )
        if (
            min_base_u >= -1.0e-3
            and max_base_u <= 1.0 + 1.0e-3
            and min_base_v >= -1.0e-3
            and max_base_v <= 1.0 + 1.0e-3
        ):
            fail(
                f"{layer_id} skirt-ribbon target did not include overhanging material UVs "
                f"({min_base_u:.6f}..{max_base_u:.6f}, {min_base_v:.6f}..{max_base_v:.6f})"
            )
        assert_layer_mask_uv_bounds(
            layer_id,
            "skirt-ribbon base layer-uv domain",
            layer_size,
            target,
            min_base_u,
            max_base_u,
            min_base_v,
            max_base_v,
        )
        effect_steps = [
            step
            for step in steps
            if isinstance(step, dict)
            and step.get("layer_id") == layer_id
            and int(step.get("we_graph_step_index") or 0) > 0
            and int(step.get("vertex_count") or 0) == 4
        ]
        if not effect_steps:
            fail(f"{layer_id} has no material effect quads for skirt-ribbon validation")
        first_effect_quad = vertex_range(
            effect_steps[0], vertices, layer_id, "skirt-ribbon effect step"
        )
        min_u, max_u = vertex_component_bounds(
            first_effect_quad, "effect_uv", 0, layer_id, "skirt-ribbon effect step"
        )
        min_v, max_v = vertex_component_bounds(
            first_effect_quad, "effect_uv", 1, layer_id, "skirt-ribbon effect step"
        )
        assert_layer_mask_uv_bounds(
            layer_id,
            "skirt-ribbon effect layer-uv domain",
            layer_size,
            target,
            min_u,
            max_u,
            min_v,
            max_v,
        )
        final_steps = [
            step
            for step in steps
            if isinstance(step, dict)
            and step.get("layer_id") == layer_id
            and isinstance(step.get("render_target"), dict)
            and step["render_target"].get("type") == "swapchain"
        ]
        if len(final_steps) != 1:
            fail(f"{layer_id} has {len(final_steps)} final skirt-ribbon scene steps, expected 1")
        final_vertices = vertex_range(final_steps[0], vertices, layer_id, "skirt-ribbon final mesh")
        if len(final_vertices) <= 4:
            fail(f"{layer_id} final waterwaves pass uses a quad, expected retained puppet mesh")
        domain_u = max_base_u - min_base_u
        domain_v = max_base_v - min_base_v
        if domain_u <= 1.0e-6 or domain_v <= 1.0e-6:
            fail(f"{layer_id} skirt-ribbon UV domain is degenerate")
        max_delta = 0.0
        min_source_u = math.inf
        max_source_u = -math.inf
        min_source_v = math.inf
        max_source_v = -math.inf
        for vertex in final_vertices:
            uv = vertex.get("uv")
            effect_uv = vertex.get("effect_uv")
            if not isinstance(uv, list) or not isinstance(effect_uv, list):
                fail(f"{layer_id} final mesh has malformed uv/effect_uv")
            source_u = f64(uv[0], math.nan)
            source_v = f64(uv[1], math.nan)
            layer_u = f64(effect_uv[0], math.nan)
            layer_v = f64(effect_uv[1], math.nan)
            if not all(math.isfinite(value) for value in (source_u, source_v, layer_u, layer_v)):
                fail(f"{layer_id} final mesh has non-finite uv/effect_uv")
            min_source_u = min(min_source_u, source_u)
            max_source_u = max(max_source_u, source_u)
            min_source_v = min(min_source_v, source_v)
            max_source_v = max(max_source_v, source_v)
            expected_source_u = (layer_u - min_base_u) / domain_u
            expected_source_v = (layer_v - min_base_v) / domain_v
            max_delta = max(
                max_delta,
                abs(source_u - expected_source_u),
                abs(source_v - expected_source_v),
            )
        if min_source_u < -1.0e-5 or max_source_u > 1.0 + 1.0e-5:
            fail(
                f"{layer_id} final waterwaves source UV escaped target domain "
                f"({min_source_u:.6f}..{max_source_u:.6f})"
            )
        if min_source_v < -1.0e-5 or max_source_v > 1.0 + 1.0e-5:
            fail(
                f"{layer_id} final waterwaves source V escaped target domain "
                f"({min_source_v:.6f}..{max_source_v:.6f})"
            )
        if max_delta > 1.0e-5:
            fail(
                f"{layer_id} final waterwaves source UV no longer maps from material UV domain "
                f"(max delta {max_delta:.6f})"
            )


def assert_pass_space_uv_bounds(
    layer_id: str,
    label: str,
    min_u: float,
    max_u: float,
    min_v: float,
    max_v: float,
) -> None:
    assert_close(f"{layer_id} {label} effect_uv min_u", min_u, 0.0)
    assert_close(f"{layer_id} {label} effect_uv max_u", max_u, 1.0)
    assert_close(f"{layer_id} {label} effect_uv min_v", min_v, 0.0)
    assert_close(f"{layer_id} {label} effect_uv max_v", max_v, 1.0)


def assert_layer_mask_uv_bounds(
    layer_id: str,
    label: str,
    layer_size: tuple[float, float],
    target: dict[str, Any],
    min_u: float,
    max_u: float,
    min_v: float,
    max_v: float,
) -> None:
    width, height = layer_size
    local_left = f64(target.get("local_left"))
    local_top = f64(target.get("local_top"))
    target_width = f64(target.get("width"), -1.0)
    target_height = f64(target.get("height"), -1.0)
    if width <= 0.0 or height <= 0.0 or target_width <= 0.0 or target_height <= 0.0:
        fail(f"{layer_id} {label} has invalid layer/target size for layer mask UV")
    expected_min_u = local_left / width
    expected_max_u = (local_left + target_width) / width
    expected_min_v = 1.0 - (local_top + target_height) / height
    expected_max_v = 1.0 - local_top / height
    assert_close(f"{layer_id} {label} layer-mask effect_uv min_u", min_u, expected_min_u)
    assert_close(f"{layer_id} {label} layer-mask effect_uv max_u", max_u, expected_max_u)
    assert_close(f"{layer_id} {label} layer-mask effect_uv min_v", min_v, expected_min_v)
    assert_close(f"{layer_id} {label} layer-mask effect_uv max_v", max_v, expected_max_v)


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


def assert_body_base_passes_are_generic(
    runtime: dict[str, Any],
    body_layers: tuple[str, ...],
) -> None:
    graph_steps = runtime.get("draw_pass_sampled_image_we_graph_steps") or []
    if not isinstance(graph_steps, list):
        fail("draw_pass_sampled_image_we_graph_steps is not an array")
    for layer_id in body_layers:
        base_steps = [
            step
            for step in graph_steps
            if isinstance(step, dict)
            and step.get("layer_id") == layer_id
            and step.get("step_index") == 0
        ]
        if len(base_steps) != 1:
            fail(f"{layer_id} has {len(base_steps)} graph base steps, expected 1")
        pass_record = base_steps[0].get("pass")
        if not isinstance(pass_record, dict):
            fail(f"{layer_id} graph base step is missing pass record")
        if pass_record.get("role") != "base-material":
            fail(f"{layer_id} graph step 0 role is {pass_record.get('role')!r}, expected base-material")
        if pass_record.get("shader") is not None:
            fail(f"{layer_id} base material pass inherited effect shader {pass_record.get('shader')!r}")
        if pass_record.get("effect_kind") is not None:
            fail(f"{layer_id} base material pass inherited effect kind {pass_record.get('effect_kind')!r}")
        for field in ("combo_keys", "parameter_keys"):
            value = pass_record.get(field) or []
            if value:
                fail(f"{layer_id} base material pass inherited {field}: {value!r}")
        for field in ("combo_values", "constant_shader_values"):
            value = pass_record.get(field) or {}
            if value:
                fail(f"{layer_id} base material pass inherited {field}: {value!r}")


def effect_records(step_or_op: dict[str, Any]) -> list[dict[str, Any]]:
    effects = step_or_op.get("effect_passes") or []
    if not isinstance(effects, list):
        return []
    return [effect for effect in effects if isinstance(effect, dict)]


def effect_file(effect: dict[str, Any]) -> str:
    value = effect.get("effect_file")
    return value if isinstance(value, str) else ""


def assert_reference_invisible_effects_are_absent(runtime: dict[str, Any]) -> None:
    draw_ops = runtime.get("draw_ops") or []
    steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    if not isinstance(draw_ops, list) or not isinstance(steps, list):
        return
    present_layers = {
        str(op.get("layer_id"))
        for op in draw_ops
        if isinstance(op, dict) and isinstance(op.get("layer_id"), str)
    }
    for layer_id in DEFAULT_INVISIBLE_EFFECT_LAYERS:
        if layer_id not in present_layers:
            continue
        layer_ops = [
            op for op in draw_ops if isinstance(op, dict) and op.get("layer_id") == layer_id
        ]
        files = [
            effect_file(effect)
            for op in layer_ops
            for effect in effect_records(op)
        ]
        if AUTOSWAY_EFFECT in files:
            fail(f"{layer_id} still executes hidden workshop auto_sway")
        waterwaves_count = sum(1 for file in files if file == WATERWAVES_EFFECT)
        if waterwaves_count != 2:
            fail(
                f"{layer_id} expected 2 visible waterwaves passes after skipping the hidden pass, "
                f"got {waterwaves_count}"
            )
        for step in steps:
            if not isinstance(step, dict) or step.get("layer_id") != layer_id:
                continue
            step_files = [effect_file(effect) for effect in effect_records(step)]
            if AUTOSWAY_EFFECT in step_files:
                fail(f"{layer_id} graph step still carries hidden workshop auto_sway")


def assert_composelayer_uses_material_alpha_not_effect_blendmode(
    runtime: dict[str, Any],
    layer_id: str = DEFAULT_COMPOSELAYER,
) -> None:
    draw_ops = runtime.get("draw_ops") or []
    steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    vertices = runtime.get("draw_pass_sampled_image_vertices") or []
    scene_size = runtime.get("scene_size") or {}
    if not isinstance(draw_ops, list) or not isinstance(steps, list) or not isinstance(vertices, list):
        return
    width = f64(scene_size.get("width"), -1.0)
    height = f64(scene_size.get("height"), -1.0)
    if width <= 0.0 or height <= 0.0:
        return
    op = next((op for op in draw_ops if isinstance(op, dict) and op.get("layer_id") == layer_id), None)
    if op is None:
        return
    if op.get("blend_mode") == "max":
        fail(
            f"{layer_id} still promotes watercaustics BLENDMODE to layer colorBlendMode=max; "
            "WE keeps that combo inside the caustics shader"
        )
    if op.get("blend_mode") != "alpha":
        fail(f"{layer_id} blend_mode is {op.get('blend_mode')!r}, expected material alpha")
    scene_steps = [
        step
        for step in steps
        if isinstance(step, dict)
        and step.get("layer_id") == layer_id
        and isinstance(step.get("render_target"), dict)
        and step["render_target"].get("type") == "swapchain"
    ]
    if len(scene_steps) != 1:
        fail(f"{layer_id} has {len(scene_steps)} swapchain composites, expected 1")
    step = scene_steps[0]
    render_state = (step.get("material_pass") or {}).get("render_state") or {}
    blend = render_state.get("blend") or {}
    blend_mode = blend.get("mode")
    if blend_mode == "max":
        fail(f"{layer_id} final scene pass still uses fixed-function max blend")
    if blend_mode != "alpha":
        fail(f"{layer_id} final scene pass blend is {blend_mode!r}, expected alpha")
    selected = vertex_range(step, vertices, layer_id, "final composelayer")
    min_x, max_x = vertex_component_bounds(selected, "position", 0, layer_id, "final composelayer")
    min_y, max_y = vertex_component_bounds(selected, "position", 1, layer_id, "final composelayer")
    assert_close(f"{layer_id} final pass-space min_x", min_x, 0.0)
    assert_close(f"{layer_id} final pass-space max_x", max_x, width)
    assert_close(f"{layer_id} final pass-space min_y", min_y, 0.0)
    assert_close(f"{layer_id} final pass-space max_y", max_y, height)


def assert_slider_rectangles_do_not_use_fixed_screen_rectangles(
    runtime: dict[str, Any],
    layer_ids: tuple[str, ...] = DEFAULT_SLIDER_RECTANGLES,
) -> None:
    draw_ops = runtime.get("draw_ops") or []
    steps = runtime.get("draw_pass_quad_recording_steps") or []
    if not isinstance(draw_ops, list) or not isinstance(steps, list):
        return
    ops_by_layer = {
        op.get("layer_id"): op
        for op in draw_ops
        if isinstance(op, dict) and isinstance(op.get("layer_id"), str)
    }
    steps_by_layer = {
        step.get("layer_id"): step
        for step in steps
        if isinstance(step, dict) and isinstance(step.get("layer_id"), str)
    }
    for layer_id in layer_ids:
        op = ops_by_layer.get(layer_id)
        step = steps_by_layer.get(layer_id)
        if op is None or step is None:
            continue
        if op.get("blend_mode") == "screen" or (step.get("blend") or {}).get("mode") == "screen":
            fail(
                f"{layer_id} still uses fixed-function screen for WE colorBlendMode 28; "
                "that mode is shader HSL Color and screen creates a bright slanted rectangle"
            )
        if op.get("blend_mode") != "alpha":
            fail(f"{layer_id} blend_mode is {op.get('blend_mode')!r}, expected alpha fallback")
        if step.get("kind") != "rounded-rectangle":
            fail(f"{layer_id} records {step.get('kind')!r}, expected rounded-mask geometry")
        if f64(op.get("corner_radius"), 0.0) <= 0.0:
            fail(f"{layer_id} has no lowered rounded-mask corner radius")


def quad_step_vertices(
    runtime: dict[str, Any],
    layer_id: str,
    label: str,
) -> list[dict[str, Any]]:
    steps = runtime.get("draw_pass_quad_recording_steps") or []
    vertices = runtime.get("draw_pass_quad_vertices") or []
    if not isinstance(steps, list) or not isinstance(vertices, list):
        fail(f"{label} requires quad recording steps and vertices")
    layer_steps = [
        step
        for step in steps
        if isinstance(step, dict)
        and step.get("layer_id") == layer_id
        and step.get("kind") == "text"
    ]
    if len(layer_steps) != 1:
        fail(f"{layer_id} has {len(layer_steps)} recorded text steps, expected 1")
    return vertex_range(layer_steps[0], vertices, layer_id, label)


def quad_vertex_bounds(
    runtime: dict[str, Any],
    layer_id: str,
    label: str,
) -> tuple[float, float, float, float]:
    selected = quad_step_vertices(runtime, layer_id, label)
    min_x, max_x = vertex_component_bounds(selected, "position", 0, layer_id, label)
    min_y, max_y = vertex_component_bounds(selected, "position", 1, layer_id, label)
    return min_x, max_x, min_y, max_y


def assert_scrolling_text_effects_are_recorded(
    runtime: dict[str, Any],
    layer_ids: tuple[str, ...] = DEFAULT_SCROLLING_TEXT_LAYERS,
) -> None:
    draw_ops = runtime.get("draw_ops") or []
    sampled_steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
    quads = runtime.get("draw_pass_recordable_quads") or []
    if not isinstance(draw_ops, list) or not isinstance(sampled_steps, list):
        fail("text scroll validation requires draw_ops and sampled-image recording steps")
    ops_by_layer = {
        op.get("layer_id"): op
        for op in draw_ops
        if isinstance(op, dict) and isinstance(op.get("layer_id"), str)
    }
    steps_by_layer = {
        step.get("layer_id"): step
        for step in sampled_steps
        if isinstance(step, dict) and isinstance(step.get("layer_id"), str)
    }
    recordable_text_layers = {
        quad.get("layer_id")
        for quad in quads
        if isinstance(quad, dict) and isinstance(quad.get("layer_id"), str)
    }
    for layer_id in layer_ids:
        op = ops_by_layer.get(layer_id)
        step = steps_by_layer.get(layer_id)
        if op is None:
            fail(f"{layer_id} is missing from draw_ops")
        if step is None:
            fail(f"{layer_id} is missing from sampled-image recording steps")
        if op.get("kind") != "image":
            fail(
                f"{layer_id} is still a {op.get('kind')!r} draw op; "
                "WE text must be rasterized to a font texture, not solid fallback quads"
            )
        if layer_id in recordable_text_layers:
            fail(f"{layer_id} still records solid text geometry fallback")
        source = str(op.get("source") or step.get("source") or "")
        if "font-text-raster.gtex" not in source:
            fail(f"{layer_id} source is not a generated font text raster: {source!r}")
        material = step.get("material_pass") or {}
        effects = material.get("effect_kinds") or []
        if "scroll" not in effects:
            fail(f"{layer_id} sampled-image material lost WE scroll: {effects!r}")
        if material.get("shader") != "effects/scroll":
            fail(f"{layer_id} is not routed to the GPU scroll shader: {material.get('shader')!r}")
    node29_effects = (steps_by_layer[DEFAULT_TRANSPARENT_COLORKEY_TEXT_LAYER].get("material_pass") or {}).get("effect_kinds") or []
    if "color-key" in node29_effects:
        fail("node-29-text still runs color-key at draw time; it should be baked into the text raster so scroll remains the terminal GPU shader")


def assert_scrolling_text_moves_between_snapshots(
    early: dict[str, Any],
    later: dict[str, Any],
    layer_id: str = DEFAULT_SCROLL_DISPLACEMENT_TEXT_LAYER,
) -> None:
    early_time = f64(early.get("snapshot_time_ms"), math.nan)
    later_time = f64(later.get("snapshot_time_ms"), math.nan)
    if not math.isfinite(early_time) or not math.isfinite(later_time) or later_time <= early_time:
        fail("later text-scroll snapshot must have a larger snapshot_time_ms")
    for label, runtime in (("early", early), ("later", later)):
        steps = runtime.get("draw_pass_sampled_image_recording_steps") or []
        step = next(
            (
                step
                for step in steps
                if isinstance(step, dict) and step.get("layer_id") == layer_id
            ),
            None,
        )
        if step is None:
            fail(f"{layer_id} is missing from {label} sampled-image steps")
        material = step.get("material_pass") or {}
        if material.get("shader") != "effects/scroll":
            fail(f"{layer_id} {label} snapshot is not using the scroll shader")
        values = material.get("constant_shader_values") or {}
        speed = max(abs(f64(values.get("speedx"))), abs(f64(values.get("speedy"))))
        if speed <= 0.0:
            fail(f"{layer_id} {label} snapshot has zero scroll speed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("snapshot", type=Path)
    parser.add_argument(
        "--body-layer",
        action="append",
        dest="body_layers",
        help="Layer id that must keep nominal 1571x2621 graph targets; may repeat.",
    )
    parser.add_argument(
        "--later-snapshot",
        type=Path,
        help="Optional later runtime snapshot used to prove WE scroll moves text geometry.",
    )
    args = parser.parse_args()

    runtime = runtime_snapshot(load_json(args.snapshot))
    later_runtime = runtime_snapshot(load_json(args.later_snapshot)) if args.later_snapshot else None
    body_layers = tuple(args.body_layers or DEFAULT_BODY_LAYERS)
    sizes = layer_sizes(runtime)
    assert_we_orthogonal_projection_uses_stretch(runtime)
    assert_effect_targets_cover_base_geometry(runtime, sizes, body_layers)
    assert_skirt_ribbon_uses_layer_uv_domain_puppet_waterwaves(runtime, sizes)
    assert_body_base_passes_are_generic(runtime, body_layers)
    assert_single_body_scene_composite(runtime, body_layers)
    assert_reference_invisible_effects_are_absent(runtime)
    assert_composelayer_uses_material_alpha_not_effect_blendmode(runtime)
    assert_slider_rectangles_do_not_use_fixed_screen_rectangles(runtime)
    assert_scrolling_text_effects_are_recorded(runtime)
    if later_runtime is not None:
        assert_scrolling_text_effects_are_recorded(later_runtime)
        assert_scrolling_text_moves_between_snapshots(runtime, later_runtime)
    print(
        "PASS: WE orthogonalprojection uses stretch viewport mapping, effect targets cover "
        "retained body geometry, waterwaves puppet chains run in their full layer-UV domain "
        "before the final retained mesh composite, base passes stay generic, source/effect "
        "UVs stay pass-space in local targets, hidden reference effects stay absent, body layers "
        "composite once, composelayer keeps material alpha/pass-space geometry, and slider "
        "bars are not fixed-screen rectangles; WE text scroll/colorkey effect passes survive "
        "recordable lowering, text outlines remain visible, the large glyph band sits at the "
        "WE vertical-align center x-position, and the large text moves between snapshots"
    )


if __name__ == "__main__":
    main()
