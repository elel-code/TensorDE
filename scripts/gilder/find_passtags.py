"""Find all type tag assignments at struct offset 0x30"""
import struct
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"
with open(EXE, 'rb') as f:
    f.seek(0x400); d = f.read(0x42490C)

from collections import Counter
# Search all assignment patterns to [reg+0x30]
tags30 = Counter()
tags34 = Counter()

for i in range(len(d)-7):
    # C7 4X 30 IMM32 → mov DWORD PTR [reg+0x30], imm32
    if d[i] == 0xC7 and d[i+2] == 0x30:
        modrm = d[i+1]
        if (modrm & 0xC0) == 0x40:  # ModR/M with displacement
            val = struct.unpack('<I', d[i+3:i+7])[0]
            if val < 0x100:
                tags30[val] += 1
    # C7 4X 34 IMM32 → mov DWORD PTR [reg+0x34], imm32
    if d[i] == 0xC7 and d[i+2] == 0x34:
        modrm = d[i+1]
        if (modrm & 0xC0) == 0x40:
            val = struct.unpack('<I', d[i+3:i+7])[0]
            if val < 0x100:
                tags34[val] += 1

print("Type tags at offset 0x30 (small values):")
for tag, cnt in tags30.most_common(20):
    print(f"  {tag:3d} (0x{tag:02x}): {cnt} instances")

print("\nType tags at offset 0x34 (small values):")
for tag, cnt in tags34.most_common(20):
    print(f"  {tag:3d} (0x{tag:02x}): {cnt} instances")

# Also search offset 0x38
tags38 = Counter()
for i in range(len(d)-7):
    if d[i] == 0xC7 and d[i+2] == 0x38:
        modrm = d[i+1]
        if (modrm & 0xC0) == 0x40:
            val = struct.unpack('<I', d[i+3:i+7])[0]
            if val < 0x100:
                tags38[val] += 1

print("\nType tags at offset 0x38 (small values):")
for tag, cnt in tags38.most_common(10):
    print(f"  {tag:3d} (0x{tag:02x}): {cnt} instances")
