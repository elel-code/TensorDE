#!/usr/bin/env python3
"""Inspect Wallpaper Engine puppet MDL embedded clipping records.

The eye puppet in workshop 3742497499 stores clipping-mask records in the
pre-MDLS payload. Gilder's converter currently starts at MDLS for puppet skin
and animation data, so this script makes that ignored data visible and testable.
"""

from __future__ import annotations

import argparse
import json
import struct
from pathlib import Path


VERTEX_STRIDE = 80
MESH_HEADER_SIZE = 8
TRIANGLE_INDEX_BYTES = 6


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def i32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<i", data, offset)[0]


def f32(data: bytes, offset: int) -> float:
    return struct.unpack_from("<f", data, offset)[0]


def c_string(data: bytes, offset: int, limit: int) -> tuple[str, int]:
    end = data.index(0, offset, limit)
    return data[offset:end].decode("utf-8", errors="replace"), end + 1


def section_info(data: bytes, marker: bytes) -> tuple[int, int, int, int]:
    offset = data.index(marker)
    for meta_offset in (offset + 9, offset + 8):
        end = u32(data, meta_offset)
        count = u32(data, meta_offset + 4)
        if offset < end <= len(data) and count < 1_000_000:
            return offset, end, count, meta_offset + 8
    raise ValueError(f"could not parse section header for {marker!r}")


def parse_mesh(data: bytes, mdls_offset: int) -> tuple[list[dict], list[int]]:
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
            u, raw_v = struct.unpack_from("<2f", data, base + 72)
            vertices.append(
                {
                    "x": f32(data, base),
                    "y": f32(data, base + 4),
                    "u": u,
                    "v": 1.0 - raw_v,
                    "bone_indices": list(struct.unpack_from("<4I", data, base + 40)),
                    "weights": list(struct.unpack_from("<4f", data, base + 56)),
                }
            )
        indices = [
            struct.unpack_from("<H", data, indices_offset + i * 2)[0]
            for i in range(index_bytes // 2)
        ]
        return vertices, indices
    raise ValueError("no WE puppet mesh block found before MDLS")


def parse_bones(data: bytes, mdls_start: int, mdls_end: int, bone_count: int) -> list[dict]:
    position = mdls_start
    bones = []
    for _ in range(bone_count):
        index = u32(data, position)
        position += 4
        flags = data[position]
        position += 1
        parent = i32(data, position)
        position += 4
        entry_bytes = u32(data, position)
        position += 4
        matrix = list(struct.unpack_from("<16f", data, position))
        position += entry_bytes
        info, position = c_string(data, position, mdls_end)
        bones.append(
            {
                "index": index,
                "flags": flags,
                "parent": parent if parent >= 0 else None,
                "translation": [matrix[12], matrix[13], matrix[14]],
                "info": info,
            }
        )
    return bones


def plausible_record_list_start(
    data: bytes, offset: int, limit: int, bone_count: int
) -> tuple[int, list[int], list[int]] | None:
    for shift in range(0, 8):
        start = offset + shift
        if start + 8 > limit:
            continue
        clipped_bone_count = u32(data, start)
        if clipped_bone_count == 0 or clipped_bone_count > bone_count:
            continue
        bones_start = start + 4
        frame_count_offset = bones_start + clipped_bone_count * 4
        if frame_count_offset + 4 > limit:
            continue
        frame_count = u32(data, frame_count_offset)
        frames_start = frame_count_offset + 4
        frames_end = frames_start + frame_count * 4
        if frame_count == 0 or frame_count > 10_000 or frames_end > limit:
            continue
        clipped_bones = [u32(data, bones_start + i * 4) for i in range(clipped_bone_count)]
        frame_keys = [u32(data, frames_start + i * 4) for i in range(frame_count)]
        if all(bone < bone_count for bone in clipped_bones):
            return frames_end, clipped_bones, frame_keys
    return None


def parse_clipping_records(data: bytes, mdls_offset: int, bone_count: int) -> list[dict]:
    needle = b"masks/clipping_mask_"
    first_path = data.find(needle, 0, mdls_offset)
    if first_path < 0 or first_path < 12:
        return []
    record_count = u32(data, first_path - 12)
    if record_count == 0 or record_count > 256:
        return []
    position = first_path - 8
    records = []
    for _ in range(record_count):
        if position + 8 > mdls_offset:
            break
        duration_frames = u32(data, position)
        flags = u32(data, position + 4)
        path, after_path = c_string(data, position + 8, mdls_offset)
        parsed_lists = plausible_record_list_start(data, after_path, mdls_offset, bone_count)
        if parsed_lists is None:
            break
        position, clipped_bones, frame_keys = parsed_lists
        records.append(
            {
                "mask": path,
                "duration_frames": duration_frames,
                "flags": flags,
                "bones": clipped_bones,
                "frame_keys": frame_keys,
            }
        )
    return records


def influenced_vertex_stats(vertices: list[dict], bones: list[int]) -> dict:
    bone_set = set(bones)
    influenced = []
    for vertex in vertices:
        if any(
            bone in bone_set and weight > 1.0e-5
            for bone, weight in zip(vertex["bone_indices"], vertex["weights"])
        ):
            influenced.append(vertex)
    if not influenced:
        return {"count": 0}
    return {
        "count": len(influenced),
        "x": [min(v["x"] for v in influenced), max(v["x"] for v in influenced)],
        "y": [min(v["y"] for v in influenced), max(v["y"] for v in influenced)],
        "u": [min(v["u"] for v in influenced), max(v["u"] for v in influenced)],
        "v": [min(v["v"] for v in influenced), max(v["v"] for v in influenced)],
    }


def per_bone_vertex_stats(vertices: list[dict], bones: list[int]) -> dict[str, dict]:
    return {
        str(bone): influenced_vertex_stats(vertices, [bone])
        for bone in bones
    }


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
        for i in range(match_len):
            out.append(out[start + i])
    if len(out) != decoded_size:
        raise ValueError(f"LZ4 decoded {len(out)} bytes, expected {decoded_size}")
    return bytes(out)


def mask_report(root: Path, mask: str) -> dict | None:
    tex_path = root / f"{mask}.tex"
    if not tex_path.exists():
        return None
    data = tex_path.read_bytes()
    block = data.index(b"TEXB0004")
    width, height, compression, decoded_size, encoded_size = struct.unpack_from(
        "<IIIII", data, block + 25
    )
    payload = data[block + 45 : block + 45 + encoded_size]
    raw = lz4_decode_block(payload, decoded_size) if compression == 1 else payload
    rows = [raw[y * width : (y + 1) * width] for y in range(height)]
    raw = b"".join(reversed(rows))
    nonzero = [(i % width, i // width) for i, value in enumerate(raw) if value > 0]
    report = {
        "width": width,
        "height": height,
        "nonzero_pixels": len(nonzero),
        "strong_pixels": sum(1 for value in raw if value > 127),
    }
    if nonzero:
        xs = [x for x, _ in nonzero]
        ys = [y for _, y in nonzero]
        report["nonzero_bbox"] = [min(xs), min(ys), max(xs), max(ys)]
        report["nonzero_uv_bbox"] = [
            min(xs) / width,
            min(ys) / height,
            (max(xs) + 1) / width,
            (max(ys) + 1) / height,
        ]
    return report


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mdl", type=Path)
    parser.add_argument(
        "--resource-root",
        type=Path,
        default=None,
        help="Root containing materials/masks/*.tex for mask bbox reports.",
    )
    args = parser.parse_args()

    data = args.mdl.read_bytes()
    mdls_offset, mdls_end, bone_count, mdls_start = section_info(data, b"MDLS0004")
    vertices, indices = parse_mesh(data, mdls_offset)
    bones = parse_bones(data, mdls_start, mdls_end, bone_count)
    records = parse_clipping_records(data, mdls_offset, bone_count)
    resource_root = args.resource_root
    if resource_root is None:
        extracted_root = args.mdl.parent.parent
        resource_root = (
            extracted_root / "materials"
            if (extracted_root / "materials").is_dir()
            else extracted_root
        )
    for record in records:
        record["influenced_vertices"] = influenced_vertex_stats(vertices, record["bones"])
        record["per_bone_vertices"] = per_bone_vertex_stats(vertices, record["bones"])
        record["mask_report"] = mask_report(resource_root, record["mask"])
        record["bone_translations"] = {
            str(bone): bones[bone]["translation"]
            for bone in record["bones"]
            if bone < len(bones)
        }

    print(
        json.dumps(
            {
                "mdl": str(args.mdl),
                "bone_count": bone_count,
                "vertex_count": len(vertices),
                "index_count": len(indices),
                "clipping_record_count": len(records),
                "clipping_records": records,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
