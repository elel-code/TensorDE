"""Search for scene object iteration patterns"""
import struct
EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"
with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)
sites=[]
for i in range(len(d)-7):
    if d[i:i+7]==b'\x48\x8b\x81\x10\x03\x00\x00':
        sites.append(('load [rcx+0x310]',0x140001000+i))
    if d[i:i+7]==b'\x48\x3b\x81\x18\x03\x00\x00':
        sites.append(('cmp [rcx+0x318]',0x140001000+i))
print(f'Found {len(sites)} sites')
for t,a in sites:
    print(f'  {t} @ 0x{a:x}')
