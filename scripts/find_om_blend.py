"""Find and classify OMSetBlendState-shaped call sites in WE exes."""
from __future__ import annotations

import re
import subprocess


ROOT = "/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution"

EXES = {
    "x64": f"{ROOT}/wallpaper64.exe",
    "x86": f"{ROOT}/wallpaper32.exe",
}

CLASSIFICATION = {
    "x64": {
        0x14005A019: ("raw", "direct OMSetBlendState-shaped args"),
        0x14005A5C7: ("raw", "direct OMSetBlendState-shaped args"),
        0x14005E8FE: ("raw", "direct OMSetBlendState-shaped args"),
        0x14009A232: ("raw", "state commit 0x140099f60"),
        0x140115B00: ("wrapper-float", "render-state wrapper +0x1528"),
        0x140180351: ("wrapper-float", "render-state wrapper +0x1528"),
        0x140181D4C: ("wrapper-float", "render-state wrapper +0x1528"),
        0x14019794B: ("wrapper-float", "render-state wrapper +0x1518"),
        0x1401EBA85: ("wrapper-float", "render-state wrapper +0x1518"),
        0x1401EE49A: ("wrapper-float", "render-state wrapper +0x1518"),
        0x140207802: ("wrapper-float", "render-state wrapper +0x1518"),
        0x140208012: ("wrapper-float", "render-state wrapper +0x1518"),
        0x14020D753: ("wrapper-float", "render-state wrapper clear path"),
        0x140257C9D: ("wrapper-float", "render-state wrapper +0x1518"),
        0x1400EBE1B: ("custom", "custom renderer vtable; signature not raw OMSetBlendState"),
        0x140121CBD: ("custom", "DXGI/HRESULT-style out-param vtable"),
        0x1401E8EE8: ("custom", "scene object bool-return vtable"),
    },
    "x86": {
        0x44AA96: ("raw", "direct OMSetBlendState-shaped args"),
        0x44AD43: ("raw", "direct OMSetBlendState-shaped args"),
        0x4770D3: ("raw", "state commit x86 peer"),
        0x4DE8BF: ("wrapper-float", "render-state wrapper +0x143c"),
        0x52DC78: ("wrapper-float", "render-state wrapper +0x143c"),
        0x52EA14: ("wrapper-float", "render-state wrapper +0x143c"),
        0x53D3AD: ("wrapper-float", "render-state wrapper +0x142c"),
        0x57C9A0: ("wrapper-float", "render-state wrapper +0x142c"),
        0x57DB1E: ("wrapper-float", "render-state wrapper +0x142c"),
        0x59691B: ("wrapper-float", "render-state wrapper +0x142c"),
        0x597125: ("wrapper-float", "render-state wrapper +0x142c"),
        0x59B34A: ("wrapper-float", "render-state wrapper clear path"),
        0x5DD21A: ("wrapper-float", "render-state wrapper +0x142c"),
        0x4BCE89: ("custom", "custom renderer vtable; signature not raw OMSetBlendState"),
        0x4E9684: ("custom", "DXGI/HRESULT-style out-param vtable"),
    },
}

PATTERNS = {
    "x64": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*callq\s+\*0x118\(%"
        r"(r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|10|11|12|13|14|15))\)"
    ),
    "x86": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*calll\s+\*0x8c\(%"
        r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
    ),
}


def disassemble(path: str) -> str:
    return subprocess.run(
        ["llvm-objdump", "-d", path],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


for arch, path in EXES.items():
    sites = []
    for line in disassemble(path).splitlines():
        match = PATTERNS[arch].match(line)
        if match:
            sites.append((int(match.group(1), 16), match.group(2)))

    print(f"\n{arch} OMSetBlendState-shaped call sites ({len(sites)}):")
    for addr, reg in sites:
        bucket, note = CLASSIFICATION[arch].get(addr, ("unknown", "unclassified"))
        print(f"  0x{addr:x} via %{reg:3s}  {bucket:13s}  {note}")

    counts = {}
    for addr, _ in sites:
        bucket = CLASSIFICATION[arch].get(addr, ("unknown", ""))[0]
        counts[bucket] = counts.get(bucket, 0) + 1

    print("Buckets:")
    for bucket in sorted(counts):
        print(f"  {bucket:13s} {counts[bucket]}")
