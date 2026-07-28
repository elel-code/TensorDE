"""Search all reg+offset D3D11 call patterns"""
import struct
EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)

# Search all call patterns: FF 5X OO (where X=reg, OO=offset)
# Check rcx (51), rdx (52), rbx (53) too, not just rax (50)
from collections import Counter
all_calls=Counter()
for i in range(len(d)-3):
    if d[i]==0xFF and d[i+1] in (0x50,0x51,0x52,0x53):
        reg = ['rax','rcx','rdx','rbx'][d[i+1]&3]
        off = d[i+2]
        all_calls[(reg,off)] += 1

# Show top offsets for each register
for reg in ['rax','rcx','rdx','rbx']:
    calls=[(off,cnt) for (r,off),cnt in all_calls.items() if r==reg and off>=0x100]
    calls.sort(key=lambda x:-x[1])
    if calls:
        print(f"\n{reg} high-offset calls:")
        for off,cnt in calls[:20]:
            print(f"  0x{off:02x} ({off//8:3d}): {cnt}")
