"""Find D3D11 Map/Unmap call patterns in .text"""
import struct
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"
with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)

# Pattern 1: mov r8d, 4 (WRITE_DISCARD) followed by call [reg+0x70] (Map)
# 41 B8 04 00 00 00 ... FF 50 70 (or 51 70, 52 70, 53 70)
sites=[]
for i in range(len(d)-12):
    if d[i:i+6]==b'\x41\xb8\x04\x00\x00\x00':
        # Look for call [reg+0x70] within next 20 bytes
        for j in range(i+6, min(i+26, len(d)-3)):
            if d[j]==0xFF and d[j+1] in (0x50,0x51,0x52,0x53) and d[j+2]==0x70:
                sites.append(('Map WRITE_DISCARD', 0x140001000+i))
                break

# Pattern 2: mov r8d, 5 (WRITE_NO_OVERWRITE) + call [reg+0x70]
for i in range(len(d)-12):
    if d[i:i+6]==b'\x41\xb8\x05\x00\x00\x00':
        for j in range(i+6, min(i+26, len(d)-3)):
            if d[j]==0xFF and d[j+1] in (0x50,0x51,0x52,0x53) and d[j+2]==0x70:
                sites.append(('Map NO_OVERWRITE', 0x140001000+i))
                break

# Pattern 3: Unmap - call [reg+0x78] (index 15)
unmap=[]
for i in range(len(d)-3):
    if d[i]==0xFF and d[i+1] in (0x50,0x51) and d[i+2]==0x78:
        unmap.append(0x140001000+i)

print(f"Map WRITE_DISCARD: {len([s for s in sites if s[0]=='Map WRITE_DISCARD'])}")
print(f"Map NO_OVERWRITE: {len([s for s in sites if s[0]=='Map NO_OVERWRITE'])}")
print(f"Unmap sites: {len(unmap)}")

print("\nMap WRITE_DISCARD locations:")
for t,a in sites:
    if t=='Map WRITE_DISCARD':
        print(f"  0x{a:x}")
print("\nUnmap locations (first 20):")
for a in unmap[:20]:
    print(f"  0x{a:x}")
