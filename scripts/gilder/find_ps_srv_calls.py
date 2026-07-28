"""Find and classify PSSetShaderResources-shaped call sites in WE exes."""
from __future__ import annotations

import collections
import re
import subprocess
from pathlib import Path

from workspace_paths import GILDER_ROOT, WALLPAPER_DISTRIBUTION

ROOT = GILDER_ROOT
DIST = WALLPAPER_DISTRIBUTION

EXES = {
    "x64": DIST / "wallpaper64.exe",
    "x86": DIST / "wallpaper32.exe",
}

METHOD_OFFSETS = {
    "x64": 0x40,
    "x86": 0x20,
}

PATTERNS = {
    "x64": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*callq\s+\*0x40\(%"
        r"(r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|10|11|12|13|14|15))\)"
    ),
    "x86": re.compile(
        r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
        r"\s*calll\s+\*0x20\(%"
        r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
    ),
}


X64_EXACT = {
    0x14005A0FA: ("raw", "utility raw context [obj+0x80]; slot0 PS SRV array before Draw(6, 0)"),
    0x14005A70D: ("raw", "utility raw context [obj+0x80]; slot0 PS SRV array before Draw([obj+0x118], 0)"),
    0x14005EA6A: ("raw", "utility raw context [obj+0x80]; slot0 PS SRV array before Draw([obj+0x118], 0)"),
    0x1400EE913: ("raw", "texture/resource binder [5]; raw PSSetShaderResources(slot=arg&0xf, 1, &srv)"),
    0x1400EE9BA: ("raw", "texture/resource binder [6]; raw PSSetShaderResources(slot=arg&0xf, 1, &null_srv)"),
    0x14005A78A: ("custom-status", "post-draw status/cache object at [obj+0x20], zero args, not SRV binding"),
    0x14005EAEC: ("custom-status", "post-draw status/cache object at [obj+0x20], zero args, not SRV binding"),
    0x1400EE8D5: ("custom-status", "optional helper timeout call before texture bind; args include 0x3e8, not SRV binding"),
    0x14004AAE0: ("streambuf-custom", "custom stream/buffer write-style method; return compared with requested length"),
    0x14004C276: ("custom-io", "custom IO/state object returns 0/1/2 enum, not StartSlot/NumViews/ppSRVs"),
    0x140056550: ("shader-reflection", "shader/reflection helper stringifies object metadata, not raw context SRV"),
    0x14006AA75: ("resource-callback", "resource/list callback followed by +0x50 cleanup, not raw context"),
    0x14007132A: ("resource-callback", "custom object init/query callback with stack out param, not raw context"),
    0x140073630: ("resource-callback", "custom resource/update method with one out struct arg, not SRV binding"),
    0x1400CF5AB: ("com-query", "COM/query-style out pointer and HRESULT path, not PS SRV args"),
    0x1400C4CE3: ("material-pass", "custom material/pass bool method at object+0xb30, zero SRV args"),
    0x1400ED649: ("effect-wrapper", "effect/texture helper callback through material state, not raw context"),
    0x1400FEB0E: ("media-com", "Media Foundation attribute UINT64 getter with GUID key/out value"),
    0x14010077E: ("property-wrapper", "custom property wrapper method at object+0x160, not raw context"),
    0x14010123B: ("property-wrapper", "custom property wrapper method after QueryInterface-style call, not raw context"),
    0x140113480: ("scene-callback", "scene/resource callback returns bool before invoking user callback"),
    0x14011BB0D: ("resource-callback", "custom resource/string provider returning pointer/length data"),
    0x14011E993: ("com-query", "COM/shell-style object query with stack out param and follow-up release"),
    0x14013E966: ("com-query", "COM wrapper method with HRESULT/cleanup-style flow"),
    0x14013F225: ("com-query", "COM wrapper method with HRESULT/cleanup-style flow"),
    0x14015111C: ("material-pass", "material/pass scope object at +0x158; small descriptor arg, not raw context"),
    0x140161FA9: ("material-pass", "material/pass or property bridge callback, not raw context"),
    0x1401742E2: ("material-pass", "material/property bridge callback with packed value args"),
    0x1401B2D5C: ("render-wrapper", "render-state wrapper [state+0x1518] creates indexed draw target"),
    0x1401B2DB0: ("render-wrapper", "render-state wrapper [state+0x1518] creates indexed draw target"),
    0x1401C303E: ("render-wrapper", "render-state wrapper [state+0x1518] creates draw target"),
    0x1401C57B3: ("image-helper", "image/buffer helper with custom receiver, not StartSlot/NumViews/ppSRVs"),
    0x1401D40DA: ("image-helper", "image upload/copy helper; args are buffer pointer and mode, not SRV slot"),
    0x1401D7889: ("image-helper", "image/resource helper through custom object, not raw context"),
    0x1401EBE31: ("render-wrapper", "render/layer wrapper enum query through [state+0x1510]"),
    0x1401ECAF5: ("render-wrapper", "render/layer wrapper helper, not raw context"),
    0x1401EE031: ("render-wrapper", "render-state wrapper [8] creates indexed draw target"),
    0x1401FA5EC: ("property-wrapper", "property getter/setter wrapper, not raw context"),
    0x14020A4FF: ("render-wrapper", "render-state wrapper creates fallback image/attachment draw target"),
    0x14020B15E: ("render-wrapper", "render-state wrapper creates alternate layer draw target"),
    0x14020B1E8: ("render-wrapper", "render-state wrapper creates active-material draw target"),
}

X86_EXACT = {
    0x44AB27: ("raw", "utility raw context; slot0 PS SRV array before Draw(6, 0)"),
    0x44AE14: ("raw", "utility raw context; slot0 PS SRV array before Draw([obj+0xa4], 0)"),
    0x4BE3A6: ("raw", "texture/resource binder [5] peer; raw PSSetShaderResources(slot, 1, &srv)"),
    0x4BE40F: ("raw", "texture/resource binder [6] peer; raw PSSetShaderResources(slot, 1, &null_srv)"),
    0x44AE6E: ("custom-status", "post-draw status/cache object, zero args, not SRV binding"),
    0x4BE36E: ("custom-status", "optional helper timeout call before texture bind; args include 0x3e8"),
    0x40CAE4: ("streambuf-custom", "custom text/stream buffer virtual call, not raw context"),
    0x444331: ("app-callback", "app/config callback with custom receiver, not PS SRV args"),
    0x453ECD: ("app-callback", "app/UI callback with custom receiver, not PS SRV args"),
    0x459573: ("app-callback", "app/UI callback with custom receiver, not PS SRV args"),
    0x45AB29: ("app-callback", "app/UI callback with custom receiver, not PS SRV args"),
    0x4A796E: ("shader-reflection", "shader/compiler wrapper callback, not raw context"),
    0x517396: ("material-pass", "custom material/pass layout update callback, not raw context"),
    0x4CD34B: ("property-wrapper", "custom property wrapper method, not raw context"),
    0x4CEA66: ("property-wrapper", "custom property wrapper method, not raw context"),
    0x4CF228: ("property-wrapper", "custom property wrapper method, not raw context"),
    0x4E712F: ("property-wrapper", "custom property wrapper method, not raw context"),
    0x565F74: ("stack-callback", "function pointer from stack slot +0x20, not receiver vtable"),
    0x56AFC4: ("scene-callback", "scene/layer callback with custom receiver"),
    0x6C8E31: ("font-parser", "font/parser callback table, not raw context"),
    0x6FB3E1: ("font-parser", "PostScript/CFF parser dispatch, not raw context"),
    0x71AF66: ("image-helper", "image/codec callback with custom receiver, not raw context"),
    0x72D75A: ("image-helper", "image/codec callback with custom receiver, not raw context"),
}


X64_RANGES = [
    (0x140000000, 0x140020000, "streambuf-custom", "C++ stream/facet small-buffer virtuals"),
    (0x1400F1700, 0x1400FEC00, "media-com", "Media Foundation / video COM calls"),
    (0x140120000, 0x140124700, "media-com", "Media Foundation / video COM calls"),
    (0x140172000, 0x140175000, "material-pass", "material property/pass bridge callbacks"),
    (0x140177000, 0x14018AB00, "scene-callback", "scene object/property callbacks"),
    (0x140190000, 0x140198000, "scene-callback", "scene object/property callbacks"),
    (0x140208000, 0x14020E000, "layer-target", "layer RT target method [8], not PS SRV"),
    (0x140256000, 0x140262000, "scene-callback", "scene/model lifecycle callbacks"),
    (0x140275000, 0x140285000, "custom-parser", "custom parser/container callbacks"),
    (0x140310000, 0x140344000, "font-parser", "PostScript/CFF/font parser callbacks"),
    (0x14035C000, 0x1403D2000, "image-helper", "image/codec helper callback tables"),
]

X86_RANGES = [
    (0x4C0E00, 0x4C2600, "effect-wrapper", "effect/texture helper callbacks"),
    (0x4E3000, 0x50B000, "media-com", "Media Foundation / video COM calls"),
    (0x524800, 0x524C00, "material-pass", "material property/pass bridge callbacks"),
    (0x527000, 0x53A000, "scene-callback", "scene object/property callbacks"),
    (0x57D000, 0x5A0000, "scene-callback", "scene/layer callbacks"),
    (0x5DB000, 0x607000, "layer-target", "layer/render-target wrapper callbacks"),
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


def find_sites(arch: str, path: Path) -> list[tuple[int, str, str, str]]:
    sites = []
    for line in disassemble(path).splitlines():
        match = PATTERNS[arch].match(line)
        if not match:
            continue
        addr = int(match.group(1), 16)
        reg = match.group(2)
        bucket, note = classify(arch, addr)
        sites.append((addr, reg, bucket, note))
    return sites


def main() -> None:
    for arch, path in EXES.items():
        sites = find_sites(arch, path)
        print(f"\n{arch} PSSetShaderResources-shaped call sites ({len(sites)}):")

        bucket_counts = collections.Counter(bucket for _, _, bucket, _ in sites)
        print("Buckets:")
        for bucket in sorted(bucket_counts):
            print(f"  {bucket:16s} {bucket_counts[bucket]}")

        print("Classified sites:")
        for addr, reg, bucket, note in sites:
            if bucket == "unclassified":
                continue
            print(f"  0x{addr:x} via %{reg:3s}  {bucket:16s}  {note}")

        unclassified = [(addr, reg) for addr, reg, bucket, _ in sites if bucket == "unclassified"]
        if unclassified:
            print("Unclassified sites:")
            for addr, reg in unclassified:
                print(f"  0x{addr:x} via %{reg:3s}")


if __name__ == "__main__":
    main()
