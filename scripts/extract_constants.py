import struct
from collections import Counter
f=open('/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe','rb')
f.seek(0x424E00); d=f.read(0xB51AC)
vals=Counter()
for i in range(0,len(d)-4,4):
    v=struct.unpack('<f',d[i:i+4])[0]
    if abs(v)>0.0001 and v!=0 and v!=1 and v!=-1:
        vals[round(v,6)]+=1
print(f'Distinct float values: {len(vals)}')
for v,cnt in vals.most_common(60):
    print(f'  {v:.6f} ({cnt}x)')

# Also find known constants
known={
    0.02:'attachment scale',
    0.7:'layer offset',
    0.25:'bloom feather',
    0.27901:'blur kernel[0]',
    0.44198:'blur kernel[1]',
    3.141593:'PI',
    0.001:'iris scale base',
    0.1:'common factor',
    2.0:'double',
    0.5:'half',
    0.333333:'1/3',
    1.333333:'4/3',
}
for v,name in known.items():
    c=sum(1 for i in range(0,len(d)-4,4) if abs(struct.unpack('<f',d[i:i+4])[0]-v)<0.0001)
    if c>0:
        print(f'  KNOWN {name}={v}: {c}x')
