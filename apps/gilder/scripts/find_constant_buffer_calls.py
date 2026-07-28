"""Find and classify VS/PSSetConstantBuffers-shaped call sites in WE exes."""
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
        0x38: "VSSetConstantBuffers",
        0x80: "PSSetConstantBuffers",
    },
    "x86": {
        0x1C: "VSSetConstantBuffers",
        0x40: "PSSetConstantBuffers",
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


X64_EXACT = {
    0x14005A618: ("raw", "utility raw context [obj+0x80]; VS slot0, count 1, &[obj+0xa8]"),
    0x14005A64D: ("raw", "utility raw context [obj+0x80]; PS slot0, count 1, &[obj+0xa8]"),
    0x14005E94F: ("raw", "utility raw context [obj+0x80]; VS slot0, count 1, &[obj+0xa8]"),
    0x14005E98A: ("raw", "utility raw context [obj+0x80]; PS slot0, count 1, &[obj+0xa8]"),
    0x14009B30C: ("raw", "render-state wrapper dynamic PS buffer; raw context [wrapper+0x70]+0x8, slot3"),
    0x1400D766E: ("raw", "shader-stage commit helper; raw VS slot0, count 1"),
    0x1400D7691: ("raw", "shader-stage commit helper; raw VS slot1, count 1"),
    0x1400D774C: ("raw", "shader-stage commit helper; raw PS slot0, count 1"),
    0x1400D7772: ("raw", "shader-stage commit helper; raw PS slot1, count 1"),
}

X86_EXACT = {
    0x44AD7C: ("raw", "utility raw context; VS slot0, count 1"),
    0x44AD98: ("raw", "utility raw context; PS slot0, count 1"),
    0x477C11: ("raw", "render-state wrapper dynamic PS buffer; raw context [wrapper+0x64]+0x4, slot3"),
    0x477D9C: ("raw", "render-state wrapper dynamic VS buffer; raw context [wrapper+0x64]+0x4, slot2"),
    0x4AD4FD: ("raw", "shader-stage commit helper peer; raw VS slot0, count 1"),
    0x4AD519: ("raw", "shader-stage commit helper peer; raw VS slot1, count 1"),
    0x4AD5A0: ("raw", "shader-stage commit helper peer; raw PS slot0, count 1"),
    0x4AD5BC: ("raw", "shader-stage commit helper peer; raw PS slot1, count 1"),
}


X64_RANGES = [
    (0x140000000, 0x140050000, "streambuf-custom", "C++ stream/facet/runtime virtuals"),
    (0x140050000, 0x140058000, "resource-callback", "resource/shader-reflection callbacks"),
    (0x140058000, 0x140060000, "utility-custom", "utility/helper object callbacks adjacent to raw utility draws"),
    (0x140060000, 0x140078000, "resource-callback", "resource/IO/layout callbacks"),
    (0x140090000, 0x14009C000, "render-wrapper", "render-state wrapper methods"),
    (0x1400D0000, 0x1400E0000, "material-stage-wrapper", "material shader-stage helper methods"),
    (0x1400E0000, 0x1400F1700, "texture-effect-wrapper", "texture/resource/effect helper vtables"),
    (0x1400F1700, 0x1400FEC00, "media-com", "Media Foundation / video COM calls"),
    (0x140100000, 0x140104000, "property-wrapper", "custom property wrapper methods"),
    (0x140110000, 0x140120000, "scene-resource-callback", "scene/resource/property callbacks"),
    (0x140120000, 0x140125200, "media-com", "Media Foundation / video COM calls"),
    (0x14012A000, 0x140142000, "image-com-helper", "image/COM helper callbacks"),
    (0x140150000, 0x140152000, "material-pass", "material/pass bridge callbacks"),
    (0x140170000, 0x1401A9000, "scene-callback", "scene object/property/layer callbacks"),
    (0x1401D0000, 0x140207000, "scene-callback", "scene/model/material callbacks"),
    (0x140207000, 0x140210000, "layer-target", "layer/render-target wrapper callbacks"),
    (0x140210000, 0x140270000, "scene-callback", "scene/model/layer callbacks"),
    (0x140270000, 0x140285500, "custom-parser", "custom parser/container callbacks"),
    (0x140310000, 0x140346000, "font-parser", "PostScript/CFF/font parser callbacks"),
    (0x140346000, 0x140358000, "font-raster", "font/glyph raster callbacks"),
    (0x14035C000, 0x140415000, "image-helper", "image/codec helper callback tables"),
]

X86_RANGES = [
    (0x400000, 0x430000, "streambuf-custom", "C++ stream/facet/runtime virtuals"),
    (0x430000, 0x470000, "app-utility-callback", "app/resource utility callbacks"),
    (0x470000, 0x479000, "render-wrapper", "render-state wrapper methods"),
    (0x4A0000, 0x4B3000, "shader-compiler", "shader compiler/material helper callbacks"),
    (0x4B3000, 0x4C3000, "texture-effect-wrapper", "texture/resource/effect helper callbacks"),
    (0x4C3000, 0x4D2000, "property-wrapper", "custom property wrapper methods"),
    (0x4D2000, 0x4E9000, "property-render-wrapper", "property/render wrapper callbacks"),
    (0x4E9000, 0x50B000, "media-com", "Media Foundation / video COM calls"),
    (0x50B000, 0x525000, "material-pass", "material/pass bridge callbacks"),
    (0x527000, 0x53B000, "scene-callback", "scene object/property callbacks"),
    (0x540000, 0x5A8000, "scene-callback", "scene/layer callbacks"),
    (0x5A8000, 0x607000, "layer-target", "layer/render-target wrapper callbacks"),
    (0x6C0000, 0x704000, "font-parser", "PostScript/CFF/font parser callbacks"),
    (0x710000, 0x730000, "image-helper", "image/codec helper callbacks"),
]


def disassemble(path: Path) -> str:
    return subprocess.run(
        ["llvm-objdump", "-d", str(path)],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


def classify(arch: str, addr: int) -> tuple[str, str]:
    exact = X64_EXACT if arch == "x64" else X86_EXACT
    if addr in exact:
        return exact[addr]

    ranges = X64_RANGES if arch == "x64" else X86_RANGES
    for start, end, bucket, note in ranges:
        if start <= addr < end:
            return bucket, note

    return "unclassified", "receiver not traced"


def find_sites(arch: str, path: Path) -> list[tuple[int, str, str, str, str]]:
    sites = []
    for line in disassemble(path).splitlines():
        match = PATTERNS[arch].match(line)
        if not match:
            continue
        addr = int(match.group(1), 16)
        offset = int(match.group(2), 16)
        method = METHODS[arch].get(offset)
        if method is None:
            continue
        reg = match.group(3)
        bucket, note = classify(arch, addr)
        sites.append((addr, reg, method, bucket, note))
    return sites


def main() -> None:
    for arch, path in EXES.items():
        sites = find_sites(arch, path)
        print(f"\n{arch} VS/PSSetConstantBuffers-shaped call sites ({len(sites)}):")

        method_counts = collections.Counter(method for _, _, method, _, _ in sites)
        print("Methods:")
        for method in sorted(method_counts):
            print(f"  {method:21s} {method_counts[method]}")

        bucket_counts = collections.Counter(bucket for _, _, _, bucket, _ in sites)
        print("Buckets:")
        for bucket in sorted(bucket_counts):
            print(f"  {bucket:23s} {bucket_counts[bucket]}")

        print("Classified sites:")
        for addr, reg, method, bucket, note in sites:
            if bucket == "unclassified":
                continue
            print(f"  0x{addr:x} via %{reg:3s}  {method:21s}  {bucket:23s}  {note}")

        unclassified = [
            (addr, reg, method)
            for addr, reg, method, bucket, _ in sites
            if bucket == "unclassified"
        ]
        if unclassified:
            print("Unclassified sites:")
            for addr, reg, method in unclassified:
                print(f"  0x{addr:x} via %{reg:3s}  {method}")


if __name__ == "__main__":
    main()
