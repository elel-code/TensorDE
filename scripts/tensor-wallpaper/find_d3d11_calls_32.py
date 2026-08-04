"""Count 32-bit ID3D11DeviceContext-shaped vtable call candidates.

The old byte scanner treated bytes like ``ff 50 e8`` as ``call [eax+0xe8]``.
That is unsafe: it can match inside other instructions, and ``e8`` is a signed
disp8 when it is actually decoded. Use llvm-objdump instruction output instead.
"""
from __future__ import annotations

import collections
import re
import subprocess
from workspace_paths import WALLPAPER_DISTRIBUTION


EXE = WALLPAPER_DISTRIBUTION / "wallpaper32.exe"

METHODS = {
    7: "VSSetConstantBuffers",
    8: "PSSetShaderResources",
    12: "DrawIndexed",
    13: "Draw",
    14: "Map",
    15: "Unmap",
    33: "OMSetRenderTargets",
    35: "OMSetBlendState",
    36: "OMSetDepthStencilState",
    50: "ClearRenderTargetView",
    53: "ClearDepthStencilView",
    57: "ResolveSubresource",
    58: "ExecuteCommandList",
}

pattern = re.compile(
    r"^\s*([0-9a-f]+):\s+(?:[0-9a-f]{2}\s+)+"
    r"\s*calll\s+\*0x([0-9a-f]+)\(%"
    r"(e(?:ax|bx|cx|dx|si|di|bp|sp))\)"
)

output = subprocess.run(
    ["llvm-objdump", "-d", EXE],
    check=True,
    stdout=subprocess.PIPE,
    text=True,
).stdout

counts = collections.Counter()
samples: dict[int, list[tuple[int, str]]] = collections.defaultdict(list)
for line in output.splitlines():
    match = pattern.match(line)
    if not match:
        continue
    addr = int(match.group(1), 16)
    offset = int(match.group(2), 16)
    reg = match.group(3)
    if offset % 4 != 0:
        continue
    index = offset // 4
    counts[index] += 1
    if len(samples[index]) < 5:
        samples[index].append((addr, reg))

print("32-bit EXE disassembled context-vtable calls:")
for index, name in METHODS.items():
    sample = ", ".join(f"0x{addr:x}/{reg}" for addr, reg in samples[index])
    print(f"  idx={index:3d} off=0x{index * 4:03x} {name:22s}: {counts[index]:3d} {sample}")
