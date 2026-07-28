"""Find callers of blend mode mapper at 0x1401531c0"""
import struct
EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
target=0x1401531c0
with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)
results=[]
for i in range(len(d)-5):
    if d[i]==0xE8:
        disp=struct.unpack('<i',d[i+1:i+5])[0]
        call_vma=0x140001000+i
        if call_vma+5+disp==target:
            results.append(call_vma)
print(f'Callers of blend mapper: {len(results)}')
for r in results:
    print(f'  0x{r:x}')
