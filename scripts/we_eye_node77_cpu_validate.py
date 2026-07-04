#!/usr/bin/env python3
"""CPU-raster validation for the WE 3742497499 closed-eye residual.

The script consumes Gilder's scene-runtime snapshot, replays node-77's base
puppet pass into its effect target, and reports how many blue iris/pupil pixels
remain under several triangle ordering/culling hypotheses.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import subprocess
from collections import Counter
from pathlib import Path


PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
VERTEX_STRIDE = 80
MESH_HEADER_SIZE = 8
TRIANGLE_INDEX_BYTES = 6


def run_snapshot(args: argparse.Namespace) -> dict:
    if args.snapshot_json:
        return json.loads(args.snapshot_json.read_text())
    command = [
        str(args.binary),
        "--scene-runtime-snapshot",
        "--source",
        str(args.scene),
        "--scene-root",
        str(args.scene_root),
        "--scene-time-ms",
        str(args.time_ms),
    ]
    result = subprocess.run(command, check=True, stdout=subprocess.PIPE, text=True)
    return json.loads(result.stdout)


def embedded_png(tex_path: Path) -> bytes:
    data = tex_path.read_bytes()
    start = data.index(PNG_MAGIC)
    end = data.index(b"IEND", start) + 8
    return data[start:end]


def load_png_rgba(tex_path: Path) -> tuple[int, int, bytes]:
    png = embedded_png(tex_path)
    identify = subprocess.run(
        ["magick", "identify", "-format", "%w %h", "png:-"],
        input=png,
        stdout=subprocess.PIPE,
        check=True,
    )
    width, height = (int(part) for part in identify.stdout.decode().split())
    raw = subprocess.run(
        ["magick", "png:-", "-depth", "8", "rgba:-"],
        input=png,
        stdout=subprocess.PIPE,
        check=True,
    ).stdout
    if len(raw) != width * height * 4:
        raise ValueError(f"decoded texture byte size mismatch: {len(raw)} vs {width}x{height}")
    return width, height, raw


def lz4_decode_block(payload: bytes, decoded_size: int) -> bytes:
    out = bytearray()
    offset = 0
    while offset < len(payload):
        token = payload[offset]
        offset += 1
        literal_len = token >> 4
        if literal_len == 15:
            while True:
                extra = payload[offset]
                offset += 1
                literal_len += extra
                if extra != 255:
                    break
        out.extend(payload[offset : offset + literal_len])
        offset += literal_len
        if offset >= len(payload):
            break
        match_offset = payload[offset] | (payload[offset + 1] << 8)
        offset += 2
        match_len = token & 0x0F
        if match_len == 15:
            while True:
                extra = payload[offset]
                offset += 1
                match_len += extra
                if extra != 255:
                    break
        match_len += 4
        start = len(out) - match_offset
        if start < 0:
            raise ValueError("invalid LZ4 match offset")
        for index in range(match_len):
            out.append(out[start + index])
    if len(out) != decoded_size:
        raise ValueError(f"LZ4 decoded {len(out)} bytes, expected {decoded_size}")
    return bytes(out)


def load_texb_r8(tex_path: Path) -> tuple[int, int, bytes]:
    data = tex_path.read_bytes()
    block = data.index(b"TEXB0004")
    width, height, compression, decoded_size, encoded_size = struct.unpack_from(
        "<IIIII", data, block + 25
    )
    payload = data[block + 45 : block + 45 + encoded_size]
    raw = lz4_decode_block(payload, decoded_size) if compression == 1 else payload
    if len(raw) != width * height:
        raise ValueError(f"decoded mask byte size mismatch: {len(raw)} vs {width}x{height}")
    rows = [raw[y * width : (y + 1) * width] for y in range(height)]
    return width, height, b"".join(reversed(rows))


def write_png(path: Path, width: int, height: int, rgba: bytes) -> None:
    subprocess.run(
        ["magick", "-size", f"{width}x{height}", "-depth", "8", "rgba:-", str(path)],
        input=rgba,
        check=True,
    )


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def parse_mdl_vertices(mdl_path: Path) -> list[dict]:
    data = mdl_path.read_bytes()
    mdls_offset = data.index(b"MDLS0004")
    for offset in range(9, mdls_offset - MESH_HEADER_SIZE - 4):
        vertex_bytes = u32(data, offset + 4)
        vertices_offset = offset + MESH_HEADER_SIZE
        index_length_offset = vertices_offset + vertex_bytes
        if (
            vertex_bytes == 0
            or vertex_bytes % VERTEX_STRIDE != 0
            or index_length_offset + 4 > mdls_offset
        ):
            continue
        index_bytes = u32(data, index_length_offset)
        indices_offset = index_length_offset + 4
        if (
            index_bytes == 0
            or index_bytes % TRIANGLE_INDEX_BYTES != 0
            or indices_offset + index_bytes > mdls_offset
        ):
            continue
        vertices = []
        for index in range(vertex_bytes // VERTEX_STRIDE):
            base = vertices_offset + index * VERTEX_STRIDE
            bone_indices = list(struct.unpack_from("<4I", data, base + 40))
            weights = list(struct.unpack_from("<4f", data, base + 56))
            dominant = max(zip(weights, bone_indices), key=lambda item: item[0])[1]
            vertices.append({"bones": bone_indices, "weights": weights, "dominant_bone": dominant})
        return vertices
    raise ValueError(f"no puppet mesh block found in {mdl_path}")


def node_step(snapshot: dict, layer_id: str, step_index: int) -> dict:
    for step in snapshot["draw_pass_sampled_image_recording_steps"]:
        if step.get("layer_id") == layer_id and step.get("we_graph_step_index") == step_index:
            return step
    raise ValueError(f"step {step_index} for {layer_id} not found")


def effect_target_extent(snapshot: dict, step: dict) -> tuple[int, int]:
    target_index = step["render_target"]["target_index"]
    for target in snapshot["draw_pass_sampled_image_effect_targets"]:
        if target["effect_target_index"] == target_index:
            return int(target["width"]), int(target["height"])
    raise ValueError(f"effect target {target_index} not found")


def texture_sample_nearest(texture: tuple[int, int, bytes], u: float, v: float) -> tuple[float, float, float, float]:
    width, height, rgba = texture
    u = min(max(u, 0.0), 1.0)
    v = min(max(v, 0.0), 1.0)
    x = min(width - 1, max(0, int(u * (width - 1) + 0.5)))
    y = min(height - 1, max(0, int((1.0 - v) * (height - 1) + 0.5)))
    offset = (y * width + x) * 4
    return tuple(channel / 255.0 for channel in rgba[offset : offset + 4])


def texture_sample_r8(texture: tuple[int, int, bytes], u: float, v: float) -> float:
    width, height, r8 = texture
    u = min(max(u, 0.0), 1.0)
    v = min(max(v, 0.0), 1.0)
    x = min(width - 1, max(0, int(u * (width - 1) + 0.5)))
    y = min(height - 1, max(0, int((1.0 - v) * (height - 1) + 0.5)))
    return r8[y * width + x] / 255.0


def image_sample_nearest(
    pixels: list[list[float]], width: int, height: int, u: float, v: float
) -> list[float]:
    u = min(max(u, 0.0), 1.0)
    v = min(max(v, 0.0), 1.0)
    x = min(width - 1, max(0, int(u * (width - 1) + 0.5)))
    y = min(height - 1, max(0, int((1.0 - v) * (height - 1) + 0.5)))
    return pixels[y * width + x]


def is_blue(rgb: tuple[float, float, float], alpha: float = 1.0) -> bool:
    r, g, b = rgb
    return alpha > 0.04 and b > 0.22 and b > r * 1.35 and b > g * 1.08


def tri_signed_area(a: dict, b: dict, c: dict) -> float:
    ax, ay = a["position"]
    bx, by = b["position"]
    cx, cy = c["position"]
    return (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)


def dominant_triangle_bone(mdl_vertices: list[dict], tri: tuple[int, int, int]) -> int | None:
    if not mdl_vertices:
        return None
    weights: Counter[int] = Counter()
    for index in tri:
        if index >= len(mdl_vertices):
            continue
        vertex = mdl_vertices[index]
        for bone, weight in zip(vertex["bones"], vertex["weights"]):
            if weight > 1.0e-5:
                weights[bone] += weight
    if not weights:
        return None
    return weights.most_common(1)[0][0]


def triangle_blue_score(vertices: list[dict], tri: tuple[int, int, int], texture: tuple[int, int, bytes]) -> float:
    score = 0.0
    for index in tri:
        vertex = vertices[index]
        r, g, b, a = texture_sample_nearest(texture, vertex["uv"][0], vertex["uv"][1])
        if is_blue((r, g, b), a):
            score += 1.0
    return score


def make_triangles(
    snapshot: dict,
    step: dict,
    texture: tuple[int, int, bytes],
    mdl_vertices: list[dict],
) -> list[dict]:
    first_vertex = step["first_vertex"]
    first_index = step["first_index"]
    vertices = snapshot["draw_pass_sampled_image_vertices"]
    raw_indices = snapshot["draw_pass_sampled_image_indices"][
        first_index : first_index + step["index_count"]
    ]
    triangles = []
    for order, chunk in enumerate(range(0, len(raw_indices), 3)):
        absolute = tuple(raw_indices[chunk : chunk + 3])
        local = tuple(index - first_vertex for index in absolute)
        tri_vertices = tuple(vertices[index] for index in absolute)
        area = tri_signed_area(*tri_vertices)
        score = triangle_blue_score(vertices, absolute, texture)
        triangles.append(
            {
                "order": order,
                "absolute": absolute,
                "local": local,
                "vertices": tri_vertices,
                "area": area,
                "blue_score": score,
                "dominant_bone": dominant_triangle_bone(mdl_vertices, local),
            }
        )
    return triangles


def order_triangles(triangles: list[dict], variant: str) -> list[dict]:
    if variant == "original":
        return triangles
    if variant == "reverse-triangles":
        return list(reversed(triangles))
    if variant == "cull-area-positive":
        return [triangle for triangle in triangles if triangle["area"] <= 0.0]
    if variant == "cull-area-negative":
        return [triangle for triangle in triangles if triangle["area"] >= 0.0]
    if variant == "blue-first":
        return sorted(triangles, key=lambda triangle: (-triangle["blue_score"], triangle["order"]))
    if variant == "blue-last":
        return sorted(triangles, key=lambda triangle: (triangle["blue_score"], triangle["order"]))
    raise ValueError(f"unknown variant {variant}")


def blend_alpha(dst: list[float], src: tuple[float, float, float, float], vertex_alpha: float) -> list[float]:
    sr, sg, sb, sa = src
    sa *= vertex_alpha
    return [
        sr * sa + dst[0] * (1.0 - sa),
        sg * sa + dst[1] * (1.0 - sa),
        sb * sa + dst[2] * (1.0 - sa),
        sa * sa + dst[3] * (1.0 - sa),
    ]


def rasterize(
    width: int,
    height: int,
    triangles: list[dict],
    texture: tuple[int, int, bytes],
    initial_pixels: list[list[float]] | None = None,
) -> tuple[list[list[float]], list[int | None], list[int | None]]:
    pixels = (
        [pixel.copy() for pixel in initial_pixels]
        if initial_pixels is not None
        else [[0.0, 0.0, 0.0, 0.0] for _ in range(width * height)]
    )
    last_bone: list[int | None] = [None for _ in range(width * height)]
    last_triangle: list[int | None] = [None for _ in range(width * height)]
    for triangle in triangles:
        v0, v1, v2 = triangle["vertices"]
        x0, y0 = v0["position"]
        x1, y1 = v1["position"]
        x2, y2 = v2["position"]
        denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
        if abs(denom) < 1.0e-6:
            continue
        min_x = max(0, math.floor(min(x0, x1, x2)))
        max_x = min(width - 1, math.ceil(max(x0, x1, x2)))
        min_y = max(0, math.floor(min(y0, y1, y2)))
        max_y = min(height - 1, math.ceil(max(y0, y1, y2)))
        for py in range(min_y, max_y + 1):
            y = py + 0.5
            for px in range(min_x, max_x + 1):
                x = px + 0.5
                w0 = ((y1 - y2) * (x - x2) + (x2 - x1) * (y - y2)) / denom
                w1 = ((y2 - y0) * (x - x2) + (x0 - x2) * (y - y2)) / denom
                w2 = 1.0 - w0 - w1
                if w0 < -1.0e-5 or w1 < -1.0e-5 or w2 < -1.0e-5:
                    continue
                uv = [
                    w0 * v0["uv"][0] + w1 * v1["uv"][0] + w2 * v2["uv"][0],
                    w0 * v0["uv"][1] + w1 * v1["uv"][1] + w2 * v2["uv"][1],
                ]
                opacity = w0 * v0["opacity"] + w1 * v1["opacity"] + w2 * v2["opacity"]
                src = texture_sample_nearest(texture, uv[0], uv[1])
                pixel_index = py * width + px
                pixels[pixel_index] = blend_alpha(pixels[pixel_index], src, opacity)
                if src[3] * opacity > 0.01:
                    last_bone[pixel_index] = triangle["dominant_bone"]
                    last_triangle[pixel_index] = triangle["order"]
    return pixels, last_bone, last_triangle


def composite_opacity_duplicate(
    base_pixels: list[list[float]],
    overlay_pixels: list[list[float]],
    width: int,
    height: int,
    mask: tuple[int, int, bytes],
    mode: str,
) -> list[list[float]]:
    pixels = [pixel.copy() for pixel in base_pixels]
    for py in range(height):
        v = 1.0 - (py + 0.5) / height
        for px in range(width):
            u = (px + 0.5) / width
            src = image_sample_nearest(overlay_pixels, width, height, u, v)
            mask_alpha = texture_sample_r8(mask, u, v)
            pixel_index = py * width + px
            if mode == "alpha":
                pixels[pixel_index] = blend_alpha(
                    pixels[pixel_index], tuple(src), mask_alpha
                )
            elif mode == "coverage-replace":
                if mask_alpha >= 0.5 and src[3] > 0.04:
                    pixels[pixel_index] = src.copy()
            elif mode == "coverage-rgb":
                if mask_alpha >= 0.5 and src[3] > 0.04:
                    pixels[pixel_index][0:3] = src[0:3]
            elif mode == "inverse-alpha":
                pixels[pixel_index] = blend_alpha(
                    pixels[pixel_index], tuple(src), 1.0 - mask_alpha
                )
            elif mode == "normal-replace":
                pixels[pixel_index] = [src[0], src[1], src[2], src[3] * mask_alpha]
            else:
                raise ValueError(f"unknown opacity duplicate composite mode {mode}")
    return pixels


def summarize(
    width: int,
    height: int,
    pixels: list[list[float]],
    bones: list[int | None],
    triangles: list[dict] | None = None,
    last_triangles: list[int | None] | None = None,
) -> dict:
    blue_indices = [
        index
        for index, pixel in enumerate(pixels)
        if is_blue((pixel[0], pixel[1], pixel[2]), pixel[3])
    ]
    blue_rgb_indices = [
        index
        for index, pixel in enumerate(pixels)
        if is_blue((pixel[0], pixel[1], pixel[2]), 1.0)
    ]
    bone_counts = Counter(bones[index] for index in blue_indices if bones[index] is not None)
    summary = {
        "blue_pixel_count": len(blue_indices),
        "blue_rgb_ignoring_alpha_count": len(blue_rgb_indices),
        "top_bones": [[bone, count] for bone, count in bone_counts.most_common(12)],
    }
    if blue_indices:
        xs = [index % width for index in blue_indices]
        ys = [index // width for index in blue_indices]
        summary["blue_bbox"] = [min(xs), min(ys), max(xs), max(ys)]
    if blue_rgb_indices:
        xs = [index % width for index in blue_rgb_indices]
        ys = [index // width for index in blue_rgb_indices]
        summary["blue_rgb_ignoring_alpha_bbox"] = [min(xs), min(ys), max(xs), max(ys)]
    if triangles is not None and last_triangles is not None:
        by_order = {triangle["order"]: triangle for triangle in triangles}
        orders = [
            last_triangles[index]
            for index in blue_indices
            if last_triangles[index] is not None
        ]
        order_counts = Counter(orders)
        summary["top_blue_last_triangles"] = [
            [
                order,
                count,
                by_order[order]["dominant_bone"] if order in by_order else None,
                round(by_order[order]["blue_score"], 3) if order in by_order else None,
            ]
            for order, count in order_counts.most_common(12)
        ]
        if orders:
            summary["blue_last_triangle_order_range"] = [min(orders), max(orders)]
    return summary


def pixels_to_rgba(pixels: list[list[float]]) -> bytes:
    out = bytearray()
    for pixel in pixels:
        out.extend(int(min(max(channel, 0.0), 1.0) * 255.0 + 0.5) for channel in pixel)
    return bytes(out)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scene", type=Path, default=Path("/tmp/gilder-we-3742497499-current-release/assets/scene.gscn"))
    parser.add_argument("--scene-root", type=Path, default=Path("/tmp/gilder-we-3742497499-current-release"))
    parser.add_argument("--time-ms", type=int, default=12500)
    parser.add_argument("--binary", type=Path, default=Path("target/release/gilder-native-vulkan"))
    parser.add_argument("--snapshot-json", type=Path)
    parser.add_argument("--texture", type=Path, default=Path("reverse-engineered/extracted/3742497499/materials/眼睛.tex"))
    parser.add_argument("--opacity-mask", type=Path, default=Path("reverse-engineered/extracted/3742497499/materials/masks/opacity_mask_d2f87f99.tex"))
    parser.add_argument("--mdl", type=Path, default=Path("reverse-engineered/extracted/3742497499/models/眼睛_puppet.mdl"))
    parser.add_argument("--layer-id", default="node-77-models-json")
    parser.add_argument("--step-index", type=int, default=0)
    parser.add_argument("--overlay-layer-id", default="node-89-models-json")
    parser.add_argument("--overlay-step-index", type=int, default=0)
    parser.add_argument("--out-dir", type=Path, default=Path("/tmp/gilder-eye-node77-validate"))
    parser.add_argument("--write-previews", action="store_true")
    args = parser.parse_args()

    snapshot = run_snapshot(args)
    texture = load_png_rgba(args.texture)
    mdl_vertices = parse_mdl_vertices(args.mdl)
    step = node_step(snapshot, args.layer_id, args.step_index)
    width, height = effect_target_extent(snapshot, step)
    triangles = make_triangles(snapshot, step, texture, mdl_vertices)
    overlay_step = node_step(snapshot, args.overlay_layer_id, args.overlay_step_index)
    overlay_width, overlay_height = effect_target_extent(snapshot, overlay_step)
    overlay_triangles = make_triangles(snapshot, overlay_step, texture, mdl_vertices)
    opacity_mask = load_texb_r8(args.opacity_mask)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    report = {
        "time_ms": args.time_ms,
        "layer_id": args.layer_id,
        "step_index": args.step_index,
        "target_extent": [width, height],
        "triangle_count": len(triangles),
        "overlay_layer_id": args.overlay_layer_id,
        "overlay_triangle_count": len(overlay_triangles),
        "area_counts": {
            "positive": sum(1 for triangle in triangles if triangle["area"] > 0.0),
            "negative": sum(1 for triangle in triangles if triangle["area"] < 0.0),
            "zero": sum(1 for triangle in triangles if triangle["area"] == 0.0),
        },
        "variants": {},
        "composites": {},
    }
    for variant in [
        "original",
        "reverse-triangles",
        "cull-area-positive",
        "cull-area-negative",
        "blue-first",
        "blue-last",
    ]:
        ordered = order_triangles(triangles, variant)
        pixels, bones, last_triangles = rasterize(width, height, ordered, texture)
        summary = summarize(width, height, pixels, bones, triangles, last_triangles)
        summary["drawn_triangle_count"] = len(ordered)
        report["variants"][variant] = summary
        if args.write_previews:
            write_png(args.out_dir / f"{args.layer_id}-{args.time_ms}-{variant}.png", width, height, pixels_to_rgba(pixels))

    if (overlay_width, overlay_height) == (width, height):
        base_pixels, base_bones, base_last_triangles = rasterize(width, height, triangles, texture)
        overlay_pixels, _, _ = rasterize(width, height, overlay_triangles, texture)
        for mode in ["alpha", "coverage-replace", "coverage-rgb", "inverse-alpha", "normal-replace"]:
            composite = composite_opacity_duplicate(
                base_pixels, overlay_pixels, width, height, opacity_mask, mode
            )
            summary = summarize(width, height, composite, base_bones, triangles, base_last_triangles)
            report["composites"][f"{args.layer_id}+{args.overlay_layer_id}-{mode}"] = summary
            if args.write_previews:
                write_png(
                    args.out_dir
                    / f"{args.layer_id}-{args.overlay_layer_id}-{args.time_ms}-{mode}.png",
                    width,
                    height,
                    pixels_to_rgba(composite),
                )
    else:
        report["composites"]["error"] = (
            f"overlay extent {overlay_width}x{overlay_height} != base {width}x{height}"
        )

    report_path = args.out_dir / f"{args.layer_id}-{args.time_ms}-report.json"
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    print(f"report: {report_path}")


if __name__ == "__main__":
    main()
