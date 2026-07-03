"""Find effect pass type tags set in the binary"""
import struct
EXE = "/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
with open(EXE, 'rb') as f:
    f.seek(0x400); d = f.read(0x42490C)

# Search for: mov DWORD PTR [reg+0x30], IMM
# Pattern: C7 4X 30 IMM32  (where X depends on register)
# This stores a type tag at offset 0x30
from collections import Counter
tags = Counter()
for i in range(len(d)-7):
    if d[i] == 0xC7 and (d[i+1] & 0xC7) == 0x40 and d[i+2] == 0x30:
        val = struct.unpack('<I', d[i+3:i+7])[0]
        if val < 0x1000:  # Only small values (likely type tags)
            tags[val] += 1

print("Type tags at offset 0x30:")
for tag, count in tags.most_common(30):
    print(f"  0x{tag:03x} ({tag}): {count} instances")
