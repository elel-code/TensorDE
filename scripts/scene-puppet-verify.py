#!/usr/bin/env python3
"""Cross-check Wallpaper Engine puppet attachments against Gilder runtime output.

The script is intentionally dependency-free so it can run through `uv run`
without downloading packages. If ImageMagick's `magick` is available, it also
extracts visible alpha bounds from embedded PNG TEX payloads.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
PUPPET_VERTEX_STRIDE = 80
MDLA_BONE_FRAME_FLOATS = 9
MDLA_BONE_FRAME_BYTES = MDLA_BONE_FRAME_FLOATS * 4


@dataclass(frozen=True)
class Transform2:
    x: float = 0.0
    y: float = 0.0
    scale_x: float = 1.0
    scale_y: float = 1.0
    rotation_deg: float = 0.0

    def compose(self, child: "Transform2") -> "Transform2":
        radians = math.radians(self.rotation_deg)
        cx = child.x * self.scale_x
        cy = child.y * self.scale_y
        rx = cx * math.cos(radians) - cy * math.sin(radians)
        ry = cx * math.sin(radians) + cy * math.cos(radians)
        return Transform2(
            x=self.x + rx,
            y=self.y + ry,
            scale_x=self.scale_x * child.scale_x,
            scale_y=self.scale_y * child.scale_y,
            rotation_deg=self.rotation_deg + child.rotation_deg,
        )


@dataclass(frozen=True)
class RuntimeBBox:
    left: float
    top: float
    right: float
    bottom: float

    @property
    def center_x(self) -> float:
        return (self.left + self.right) * 0.5

    @property
    def center_y(self) -> float:
        return (self.top + self.bottom) * 0.5


@dataclass(frozen=True)
class AlphaBBox:
    width: float
    height: float
    left: float
    top: float


@dataclass(frozen=True)
class MdlBone:
    index: int
    parent: int | None
    translation: tuple[float, float, float]
    target_position: tuple[float, float, float] | None


@dataclass(frozen=True)
class MdlAttachment:
    name: str
    bone_index: int
    matrix_translation: tuple[float, float, float]


@dataclass(frozen=True)
class MdlaAnimation:
    animation_id: int
    name: str
    loop: str
    fps: float
    frames: int
    bone_blocks: list[tuple[int, int]]


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def f64(value: Any, default: float = 0.0) -> float:
    try:
        result = float(value)
    except (TypeError, ValueError):
        return default
    return result if math.isfinite(result) else default


def vector3(value: Any) -> tuple[float, float, float] | None:
    if isinstance(value, str):
        parts = value.split()
        if len(parts) >= 2:
            return (
                f64(parts[0]),
                f64(parts[1]),
                f64(parts[2]) if len(parts) > 2 else 0.0,
            )
    if isinstance(value, list) and len(value) >= 2:
        return (
            f64(value[0]),
            f64(value[1]),
            f64(value[2]) if len(value) > 2 else 0.0,
        )
    if isinstance(value, dict):
        if "value" in value:
            return vector3(value["value"])
        if "x" in value and "y" in value:
            return (f64(value.get("x")), f64(value.get("y")), f64(value.get("z")))
    return None


def node_transform(node: dict[str, Any]) -> Transform2:
    transform = node.get("transform") or {}
    return Transform2(
        x=f64(transform.get("x")),
        y=f64(transform.get("y")),
        scale_x=f64(transform.get("scale_x"), 1.0),
        scale_y=f64(transform.get("scale_y"), 1.0),
        rotation_deg=f64(transform.get("rotation_deg")),
    )


def source_transform(obj: dict[str, Any]) -> Transform2:
    origin = vector3(obj.get("origin")) or (0.0, 0.0, 0.0)
    scale = vector3(obj.get("scale")) or (1.0, 1.0, 1.0)
    angles = vector3(obj.get("angles")) or (0.0, 0.0, 0.0)
    return Transform2(
        x=origin[0],
        y=origin[1],
        scale_x=abs(scale[0]) if abs(scale[0]) > 0.0 else 1.0,
        scale_y=abs(scale[1]) if abs(scale[1]) > 0.0 else 1.0,
        rotation_deg=math.degrees(angles[2]),
    )


def walk_gscene_nodes(nodes: list[dict[str, Any]], parent: Transform2 | None = None):
    parent = parent or Transform2()
    for node in nodes:
        current = parent.compose(node_transform(node))
        yield node, current
        children = node.get("children")
        if isinstance(children, list):
            yield from walk_gscene_nodes(children, current)


def source_chain_transform(source_objects: dict[str, dict[str, Any]], source_id: str) -> Transform2:
    chain: list[dict[str, Any]] = []
    current = source_objects.get(source_id)
    seen: set[str] = set()
    while current is not None:
        sid = str(current.get("id"))
        if sid in seen:
            break
        seen.add(sid)
        chain.append(current)
        parent = current.get("parent")
        current = source_objects.get(str(parent)) if parent is not None else None
    resolved = Transform2()
    for obj in reversed(chain):
        resolved = resolved.compose(source_transform(obj))
    return resolved


def runtime_bboxes(snapshot: dict[str, Any]) -> dict[str, RuntimeBBox]:
    vertices = snapshot.get("draw_pass_sampled_image_vertices") or []
    steps = snapshot.get("draw_pass_sampled_image_recording_steps") or []
    output: dict[str, RuntimeBBox] = {}
    for step in steps:
        first = int(step.get("first_vertex") or 0)
        count = int(step.get("vertex_count") or 0)
        positions = [
            vertex.get("position")
            for vertex in vertices[first : first + count]
            if isinstance(vertex.get("position"), list) and len(vertex["position"]) >= 2
        ]
        if not positions:
            continue
        xs = [f64(position[0]) for position in positions]
        ys = [f64(position[1]) for position in positions]
        output[str(step.get("layer_id"))] = RuntimeBBox(min(xs), min(ys), max(xs), max(ys))
    return output


def find_source_scene(source_root: Path) -> Path:
    for candidate in ["scene.json", "scene.json.json"]:
        path = source_root / candidate
        if path.is_file():
            return path
    project = source_root / "project.json"
    if project.is_file():
        data = load_json(project)
        entry = data.get("file") or data.get("entry")
        if isinstance(entry, str) and (source_root / entry).is_file():
            return source_root / entry
    raise SystemExit(f"cannot find source scene JSON below {source_root}")


def source_objects_from_scene(scene: dict[str, Any]) -> dict[str, dict[str, Any]]:
    objects = scene.get("objects")
    if not isinstance(objects, list):
        return {}
    return {str(obj.get("id")): obj for obj in objects if isinstance(obj, dict) and "id" in obj}


def child_source_ids(source_objects: dict[str, dict[str, Any]], root_id: str) -> set[str]:
    result = {root_id}
    changed = True
    while changed:
        changed = False
        for sid, obj in source_objects.items():
            parent = obj.get("parent")
            if parent is not None and str(parent) in result and sid not in result:
                result.add(sid)
                changed = True
    return result


def tex_png_alpha_bbox(tex_path: Path, cache: dict[Path, AlphaBBox | None]) -> AlphaBBox | None:
    if tex_path in cache:
        return cache[tex_path]
    cache[tex_path] = None
    try:
        data = tex_path.read_bytes()
    except OSError:
        return None
    offset = data.find(PNG_MAGIC)
    if offset < 0:
        return None
    with tempfile.NamedTemporaryFile(suffix=".png") as handle:
        handle.write(data[offset:])
        handle.flush()
        try:
            output = subprocess.check_output(
                [
                    "magick",
                    handle.name,
                    "-alpha",
                    "extract",
                    "-format",
                    "%@",
                    "info:",
                ],
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            return None
    match = re.fullmatch(r"(\d+)x(\d+)\+(\d+)\+(\d+)", output)
    if not match:
        return None
    bbox = AlphaBBox(*(float(part) for part in match.groups()))
    cache[tex_path] = bbox
    return bbox


def material_texture_path(source_root: Path, original_path: str | None) -> Path | None:
    if not original_path:
        return None
    model_path = source_root / original_path
    try:
        model = load_json(model_path)
    except (OSError, json.JSONDecodeError):
        return None
    material_name = model.get("material")
    if not isinstance(material_name, str):
        return None
    material_path = source_root / material_name
    try:
        material = load_json(material_path)
    except (OSError, json.JSONDecodeError):
        return None
    passes = material.get("passes")
    if not isinstance(passes, list) or not passes:
        return None
    textures = passes[0].get("textures")
    if not isinstance(textures, list) or not textures or not isinstance(textures[0], str):
        return None
    return material_path.parent / f"{textures[0]}.tex"


def projected_alpha_bbox(
    runtime_bbox: RuntimeBBox,
    node: dict[str, Any],
    alpha: AlphaBBox,
    flipped_rows: bool,
) -> RuntimeBBox | None:
    width = f64(node.get("width"))
    height = f64(node.get("height"))
    if width <= 0.0 or height <= 0.0:
        return None
    scale_x = (runtime_bbox.right - runtime_bbox.left) / width
    scale_y = (runtime_bbox.bottom - runtime_bbox.top) / height
    left = runtime_bbox.left + alpha.left * scale_x
    top_offset = height - alpha.top - alpha.height if flipped_rows else alpha.top
    top = runtime_bbox.top + top_offset * scale_y
    right = left + alpha.width * scale_x
    bottom = top + alpha.height * scale_y
    return RuntimeBBox(left, top, right, bottom)


def read_cstring(data: bytes, position: int, end: int | None = None) -> tuple[str, int]:
    end = len(data) if end is None else end
    nul = data.index(0, position, end)
    return data[position:nul].decode("utf-8", "replace"), nul + 1


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def i32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<i", data, offset)[0]


def u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def f32(data: bytes, offset: int) -> float:
    return struct.unpack_from("<f", data, offset)[0]


def parse_mdl(path: Path, frame_width: float, frame_height: float):
    data = path.read_bytes()
    mdls = data.find(b"MDLS")
    if mdls < 0:
        return [], [], []
    mdls_end = u32(data, mdls + 9)
    bone_count = u32(data, mdls + 13)
    position = mdls + 17
    bones: list[MdlBone] = []
    for index in range(bone_count):
        _bone_index = u32(data, position)
        position += 4
        position += 1
        parent = i32(data, position)
        position += 4
        entry_bytes = u32(data, position)
        position += 4
        matrix = struct.unpack_from("<16f", data, position)
        position += entry_bytes
        info, position = read_cstring(data, position, mdls_end)
        target = None
        if info:
            try:
                raw_target = json.loads(info).get("tp")
            except json.JSONDecodeError:
                raw_target = None
            parsed = vector3(raw_target)
            if parsed is not None:
                target = (
                    parsed[0] - frame_width * 0.5,
                    parsed[1] - frame_height * 0.5,
                    parsed[2],
                )
        bones.append(
            MdlBone(
                index=index,
                parent=parent if 0 <= parent < bone_count else None,
                translation=(matrix[12], matrix[13], matrix[14]),
                target_position=target,
            )
        )

    attachments: list[MdlAttachment] = []
    mdat = data.find(b"MDAT", mdls_end)
    if mdat >= 0:
        mdat_end = u32(data, mdat + 9)
        attachment_count = u16(data, mdat + 13)
        position = mdat + 15
        for _ in range(attachment_count):
            bone_index = u16(data, position)
            position += 2
            name, position = read_cstring(data, position, mdat_end)
            matrix = struct.unpack_from("<16f", data, position)
            position += 64
            attachments.append(
                MdlAttachment(
                    name=name,
                    bone_index=bone_index,
                    matrix_translation=(matrix[12], matrix[13], matrix[14]),
                )
            )

    animations = parse_mdla(data)
    return bones, attachments, animations


def parse_mdla(data: bytes) -> list[MdlaAnimation]:
    mdla = data.find(b"MDLA")
    if mdla < 0:
        return []
    mdla_end = min(u32(data, mdla + 9), len(data))
    animation_count = u32(data, mdla + 13)
    first_animation_id = u32(data, mdla + 17)
    position = mdla + 25
    animations: list[MdlaAnimation] = []
    for index in range(animation_count):
        if index == 0:
            animation_id = first_animation_id
        else:
            found = None
            for candidate in range(position, min(position + 96, mdla_end - 16)):
                candidate_id = u32(data, candidate)
                candidate_pad = u32(data, candidate + 4)
                if 0 < candidate_id < 100_000 and candidate_pad == 0 and data[candidate + 8] != 0:
                    found = (candidate_id, candidate + 8)
                    break
            if found is None:
                break
            animation_id, position = found
        name, position = read_cstring(data, position, mdla_end)
        loop, position = read_cstring(data, position, mdla_end)
        fps = f32(data, position)
        frames = u32(data, position + 4)
        bone_count = u32(data, position + 12)
        position += 16
        bone_blocks = []
        for _ in range(bone_count):
            _flags = u32(data, position)
            block_bytes = u32(data, position + 4)
            block_position = position + 8
            bone_blocks.append((block_position, block_bytes))
            position = block_position + block_bytes
            if position > mdla_end:
                return animations
        animations.append(
            MdlaAnimation(
                animation_id=animation_id,
                name=name,
                loop=loop,
                fps=fps,
                frames=frames,
                bone_blocks=bone_blocks,
            )
        )
    return animations


def attachment_rest_position(
    bones: list[MdlBone], attachment: MdlAttachment
) -> tuple[float, float, float] | None:
    current: int | None = attachment.bone_index
    seen: set[int] = set()
    accumulated = [0.0, 0.0, 0.0]
    while current is not None and 0 <= current < len(bones):
        if current in seen:
            return None
        seen.add(current)
        bone = bones[current]
        if bone.target_position is not None:
            return (
                bone.target_position[0] + accumulated[0] + attachment.matrix_translation[0],
                bone.target_position[1] + accumulated[1] - attachment.matrix_translation[1],
                bone.target_position[2] + accumulated[2] + attachment.matrix_translation[2],
            )
        accumulated[0] += bone.translation[0]
        accumulated[1] -= bone.translation[1]
        accumulated[2] += bone.translation[2]
        current = bone.parent
    return None


def attachment_chain_sum(
    bones: list[MdlBone], attachment: MdlAttachment
) -> tuple[float, float, float] | None:
    current: int | None = attachment.bone_index
    seen: set[int] = set()
    accumulated = [attachment.matrix_translation[0], -attachment.matrix_translation[1], attachment.matrix_translation[2]]
    while current is not None and 0 <= current < len(bones):
        if current in seen:
            return None
        seen.add(current)
        bone = bones[current]
        accumulated[0] += bone.translation[0]
        accumulated[1] -= bone.translation[1]
        accumulated[2] += bone.translation[2]
        current = bone.parent
    return tuple(accumulated)


def animation_bone_ranges(data: bytes, animation: MdlaAnimation, bone_indices: list[int]) -> dict[int, dict[str, float]]:
    ranges: dict[int, dict[str, float]] = {}
    for bone_index in bone_indices:
        if bone_index >= len(animation.bone_blocks):
            continue
        block_position, block_bytes = animation.bone_blocks[bone_index]
        frame_count = block_bytes // MDLA_BONE_FRAME_BYTES
        if frame_count == 0:
            continue
        xs: list[float] = []
        ys: list[float] = []
        rz: list[float] = []
        for frame in range(frame_count):
            offset = block_position + frame * MDLA_BONE_FRAME_BYTES
            values = struct.unpack_from("<9f", data, offset)
            xs.append(values[0])
            ys.append(values[1])
            rz.append(values[5])
        ranges[bone_index] = {
            "x_min": min(xs),
            "x_max": max(xs),
            "y_min": min(ys),
            "y_max": max(ys),
            "rz_min": min(rz),
            "rz_max": max(rz),
            "frames": frame_count,
        }
    return ranges


def bone_chain(bones: list[MdlBone], bone_index: int) -> list[int]:
    chain = []
    current: int | None = bone_index
    seen: set[int] = set()
    while current is not None and 0 <= current < len(bones) and current not in seen:
        seen.add(current)
        chain.append(current)
        current = bones[current].parent
    return list(reversed(chain))


def fmt_bbox(bbox: RuntimeBBox | None) -> str:
    if bbox is None:
        return "-"
    return (
        f"{bbox.left:.1f},{bbox.top:.1f}..{bbox.right:.1f},{bbox.bottom:.1f} "
        f"c={bbox.center_x:.1f},{bbox.center_y:.1f}"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--gscene", type=Path, required=True)
    parser.add_argument("--runtime-snapshot", type=Path, required=True)
    parser.add_argument("--parent-source-id", default="937")
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()

    source_scene = load_json(find_source_scene(args.source_root))
    source_objects = source_objects_from_scene(source_scene)
    gscene = load_json(args.gscene)
    runtime = load_json(args.runtime_snapshot)
    runtime_by_layer = runtime_bboxes(runtime)

    subtree_ids = child_source_ids(source_objects, args.parent_source_id)
    gnodes = [
        (node, transform)
        for node, transform in walk_gscene_nodes(gscene.get("nodes") or [])
        if str((node.get("provenance") or {}).get("source_id")) in subtree_ids
    ]
    node_by_source = {
        str((node.get("provenance") or {}).get("source_id")): (node, transform)
        for node, transform in gnodes
    }

    alpha_cache: dict[Path, AlphaBBox | None] = {}
    rows: list[dict[str, Any]] = []
    for source_id in sorted(subtree_ids, key=lambda value: int(value) if value.isdigit() else value):
        source_obj = source_objects.get(source_id)
        if source_obj is None:
            continue
        converted = node_by_source.get(source_id)
        node = converted[0] if converted else None
        global_transform = converted[1] if converted else None
        runtime_bbox = runtime_by_layer.get(node.get("id")) if node else None
        provenance = (node or {}).get("provenance") or {}
        original_path = provenance.get("original_path")
        tex_path = material_texture_path(args.source_root, original_path)
        alpha = tex_png_alpha_bbox(tex_path, alpha_cache) if tex_path else None
        projected_top = projected_alpha_bbox(runtime_bbox, node, alpha, False) if node and runtime_bbox and alpha else None
        projected_flipped = projected_alpha_bbox(runtime_bbox, node, alpha, True) if node and runtime_bbox and alpha else None
        source_global = source_chain_transform(source_objects, source_id)
        row = {
            "source_id": source_id,
            "name": source_obj.get("name"),
            "parent": source_obj.get("parent"),
            "attachment": source_obj.get("attachment"),
            "source_global": source_global.__dict__,
            "node_id": node.get("id") if node else None,
            "converted_global": global_transform.__dict__ if global_transform else None,
            "runtime_bbox": runtime_bbox.__dict__ if runtime_bbox else None,
            "alpha_bbox": alpha.__dict__ if alpha else None,
            "visible_bbox_top_down": projected_top.__dict__ if projected_top else None,
            "visible_bbox_flipped_rows": projected_flipped.__dict__ if projected_flipped else None,
        }
        rows.append(row)

    parent_node = node_by_source.get(args.parent_source_id, (None, None))[0]
    puppet_report: dict[str, Any] = {}
    if parent_node is not None:
        model = ((parent_node.get("provenance") or {}).get("model") or {})
        puppet = model.get("puppet")
        width = f64(parent_node.get("width"))
        height = f64(parent_node.get("height"))
        if isinstance(puppet, str):
            puppet_path = args.source_root / puppet
            if puppet_path.is_file():
                bones, attachments, animations = parse_mdl(puppet_path, width, height)
                data = puppet_path.read_bytes()
                attachment_rows = []
                interesting_bones: set[int] = set()
                for attachment in attachments:
                    chain = bone_chain(bones, attachment.bone_index)
                    interesting_bones.update(chain)
                    attachment_rows.append(
                        {
                            "name": attachment.name,
                            "bone_index": attachment.bone_index,
                            "bone_chain": chain,
                            "rest_tp_position": attachment_rest_position(bones, attachment),
                            "raw_chain_sum_position": attachment_chain_sum(bones, attachment),
                        }
                    )
                animation_rows = []
                for animation in animations:
                    animation_rows.append(
                        {
                            "animation_id": animation.animation_id,
                            "name": animation.name,
                            "fps": animation.fps,
                            "frames": animation.frames,
                            "bone_count": len(animation.bone_blocks),
                            "interesting_bone_ranges": animation_bone_ranges(
                                data, animation, sorted(interesting_bones)
                            ),
                        }
                    )
                puppet_report = {
                    "path": str(puppet_path),
                    "bone_count": len(bones),
                    "attachments": attachment_rows,
                    "animations": animation_rows,
                }

    print(f"source subtree {args.parent_source_id}: {len(rows)} converted/source rows")
    print("id\tname\tparent\tattach\tconverted-center\truntime-bbox\tvisible-top\tvisible-flipped")
    for row in rows:
        converted_global = row.get("converted_global")
        converted_center = "-"
        if converted_global:
            converted_center = f"{converted_global['x']:.1f},{converted_global['y']:.1f}"
        print(
            "\t".join(
                [
                    str(row["source_id"]),
                    str(row.get("name") or ""),
                    str(row.get("parent") or ""),
                    str(row.get("attachment") or ""),
                    converted_center,
                    fmt_bbox(RuntimeBBox(**row["runtime_bbox"]) if row.get("runtime_bbox") else None),
                    fmt_bbox(
                        RuntimeBBox(**row["visible_bbox_top_down"])
                        if row.get("visible_bbox_top_down")
                        else None
                    ),
                    fmt_bbox(
                        RuntimeBBox(**row["visible_bbox_flipped_rows"])
                        if row.get("visible_bbox_flipped_rows")
                        else None
                    ),
                ]
            )
        )
    if puppet_report:
        print("\npuppet attachments")
        for attachment in puppet_report["attachments"]:
            print(
                f"{attachment['name']}: bone={attachment['bone_index']} "
                f"chain={attachment['bone_chain']} rest_tp={attachment['rest_tp_position']} "
                f"raw_chain={attachment['raw_chain_sum_position']}"
            )
        print("\nmdla animations")
        for animation in puppet_report["animations"]:
            print(
                f"{animation['animation_id']} {animation['name']}: "
                f"fps={animation['fps']:.1f} frames={animation['frames']} bones={animation['bone_count']}"
            )

    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(
            json.dumps(
                {
                    "parent_source_id": args.parent_source_id,
                    "rows": rows,
                    "puppet": puppet_report,
                },
                ensure_ascii=False,
                indent=2,
            ),
            encoding="utf-8",
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
