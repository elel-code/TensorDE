"""Find GetImmediateContext calls"""
import struct
f=open('/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe','rb')
f.seek(0x400); d=f.read(0x42490C)
sites=[]
for i in range(len(d)-3):
    if d[i]==0xFF and d[i+1]==0x50 and d[i+2]==0x20:
        sites.append(0x140001000+i)
print(f'call [rax+0x20] (GetImmediateContext?): {len(sites)}')
render=[s for s in sites if 0x140100000<s<0x140300000]
print(f'in render area (0x1401xxxxx-0x1402xxxxx): {len(render)}')
for s in render[:15]:
    print(f'  0x{s:x}')
