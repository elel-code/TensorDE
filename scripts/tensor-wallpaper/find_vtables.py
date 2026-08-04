"""Find D3D11 COM vtable call patterns in wallpaper64.exe"""
import struct
from collections import Counter
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"

with open(EXE, 'rb') as f:
    f.seek(0x400)
    text = f.read(0x42490C)

# Search for 'call QWORD PTR [reg + offset]' patterns
offsets = Counter()
i = 0
while i < len(text) - 3:
    if text[i] == 0xFF and text[i+1] in (0x50, 0x51, 0x52, 0x53, 0x90, 0x91, 0x92, 0x93):
        offset = text[i+2]
        reg = text[i+1] & 0x07
        offsets[(reg, offset)] += 1
    i += 1

reg_names = ["rax", "rcx", "rdx", "rbx"]
for (reg, off), count in offsets.most_common(80):
    print(f'  call [{reg_names[reg]}+0x{off:02x}] count={count}')
