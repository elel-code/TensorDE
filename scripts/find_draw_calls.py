"""Find and classify Draw/DrawIndexed-shaped call sites in WE exes."""
from __future__ import annotations

import collections
import re
import subprocess
from pathlib import Path


ROOT = Path("/home/yk/Code/Gilder")
DIST = ROOT / "artifacts/wallpaper-engine-workshop/steamcmd-root/distribution"

EXES = {
    "x64": DIST / "wallpaper64.exe",
    "x86": DIST / "wallpaper32.exe",
}

METHODS = {
    "x64": {
        0x60: "DrawIndexed",
        0x68: "Draw",
    },
    "x86": {
        0x30: "DrawIndexed",
        0x34: "Draw",
    },
}

CLASSIFICATION = {
    "x64": {
        0x14005A10F: ("raw", "utility raw context [obj+0x80]; Draw(6, 0)"),
        0x14005A775: ("raw", "utility raw context [obj+0x80]; Draw([obj+0x118], 0)"),
        0x14005EAD7: ("raw", "utility raw context [obj+0x80]; Draw([obj+0x118], 0)"),
        0x1400EA849: ("raw", "RT method 0x1400ea780; DrawIndexed after 0x140099f60"),
        0x1400EA85D: ("raw", "RT method 0x1400ea780; Draw after 0x140099f60"),
        0x1400EADBB: ("raw", "RT method 0x1400eacd0; DrawIndexed after 0x140099f60"),
        0x140208469: ("layer-custom", "scene/layer object vtable +0x68 bool-return, not D3D Draw"),
    },
    "x86": {
        0x44AB34: ("raw", "utility raw context; Draw(6, 0)"),
        0x44AE5B: ("raw", "utility raw context; Draw([obj+0xa4], 0)"),
        0x4BB95F: ("raw", "RT method peer; DrawIndexed after 0x476e00"),
        0x4BB96B: ("raw", "RT method peer; Draw after 0x476e00"),
        0x4BBD4A: ("raw", "RT method peer; DrawIndexed after 0x476e00"),
    },
}

PATTERNS = {
    "x64": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*callq\s+\*0x([0-9a-f]+)\(%"
        r"(r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|10|11|12|13|14|15))\)"
    ),
    "x86": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*calll\s+\*0x([0-9a-f]+)\(%"
        r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
    ),
}


def disassemble(path: Path) -> str:
    return subprocess.run(
        ["llvm-objdump", "-d", str(path)],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


for arch, path in EXES.items():
    sites = []
    for line in disassemble(path).splitlines():
        match = PATTERNS[arch].match(line)
        if not match:
            continue
        addr = int(match.group(1), 16)
        offset = int(match.group(2), 16)
        reg = match.group(3)
        method = METHODS[arch].get(offset)
        if method is None:
            continue
        bucket, note = CLASSIFICATION[arch].get(addr, ("unclassified", "receiver not traced"))
        sites.append((addr, offset, reg, method, bucket, note))

    print(f"\n{arch} Draw/DrawIndexed-shaped call sites ({len(sites)}):")

    method_counts = collections.Counter(method for _, _, _, method, _, _ in sites)
    print("Methods:")
    for method in sorted(method_counts):
        print(f"  {method:12s} {method_counts[method]}")

    bucket_counts = collections.Counter(bucket for _, _, _, _, bucket, _ in sites)
    print("Buckets:")
    for bucket in sorted(bucket_counts):
        print(f"  {bucket:14s} {bucket_counts[bucket]}")

    print("Classified sites:")
    for addr, _, reg, method, bucket, note in sites:
        if bucket == "unclassified":
            continue
        print(f"  0x{addr:x} via %{reg:3s}  {method:12s}  {bucket:14s}  {note}")
