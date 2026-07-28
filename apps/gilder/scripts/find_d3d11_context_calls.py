"""Count ID3D11DeviceContext-shaped vtable call candidates via disassembly.

This intentionally parses llvm-objdump output instead of scanning raw bytes:
short ModR/M displacements are signed, and raw byte scans can match immediates
inside unrelated instructions. This is a call-site baseline; tail jumps are
handled manually in the reverse-engineering notes when they matter.
"""
from __future__ import annotations

import collections
import re
import subprocess
from pathlib import Path


ROOT = Path("/home/yk/Code/Gilder")
EXES = {
    "x64": ROOT / "artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe",
    "x86": ROOT / "artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper32.exe",
}

# ID3D11DeviceContext indices, including inherited IUnknown/ID3D11DeviceChild.
METHODS = [
    (7, "VSSetConstantBuffers"),
    (8, "PSSetShaderResources"),
    (10, "PSSetSamplers"),
    (12, "DrawIndexed"),
    (13, "Draw"),
    (14, "Map"),
    (15, "Unmap"),
    (16, "PSSetConstantBuffers"),
    (17, "IASetInputLayout"),
    (18, "IASetVertexBuffers"),
    (19, "IASetIndexBuffer"),
    (24, "IASetPrimitiveTopology"),
    (25, "VSSetShaderResources"),
    (26, "VSSetSamplers"),
    (33, "OMSetRenderTargets"),
    (35, "OMSetBlendState"),
    (36, "OMSetDepthStencilState"),
    (50, "ClearRenderTargetView"),
    (53, "ClearDepthStencilView"),
    (57, "ResolveSubresource"),
    (58, "ExecuteCommandList"),
]


def disassemble(path: Path) -> str:
    return subprocess.run(
        ["llvm-objdump", "-d", str(path)],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


def parse_calls(arch: str, text: str):
    if arch == "x64":
        ptr_size = 8
        pattern = re.compile(
            r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
            r"\s*callq\s+\*0x([0-9a-f]+)\(%"
            r"(r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|10|11|12|13|14|15))\)"
        )
    else:
        ptr_size = 4
        pattern = re.compile(
            r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
            r"\s*calll\s+\*0x([0-9a-f]+)\(%"
            r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
        )

    counts = collections.Counter()
    samples: dict[int, list[tuple[int, str]]] = collections.defaultdict(list)
    for line in text.splitlines():
        match = pattern.match(line)
        if not match:
            continue
        addr = int(match.group(1), 16)
        offset = int(match.group(2), 16)
        reg = match.group(3)
        if offset % ptr_size != 0:
            continue
        counts[offset // ptr_size] += 1
        if len(samples[offset // ptr_size]) < 5:
            samples[offset // ptr_size].append((addr, reg))
    return counts, samples


def main() -> None:
    for arch, path in EXES.items():
        ptr_size = 8 if arch == "x64" else 4
        counts, samples = parse_calls(arch, disassemble(path))
        print(f"\n{arch} {path.name}")
        print("index  offset  count  method                  sample sites")
        print("-----  ------  -----  ----------------------  ----------------")
        for index, name in METHODS:
            offset = index * ptr_size
            sample = ", ".join(f"0x{addr:x}/{reg}" for addr, reg in samples.get(index, []))
            print(f"{index:5d}  0x{offset:04x}  {counts[index]:5d}  {name:22s}  {sample}")


if __name__ == "__main__":
    main()
