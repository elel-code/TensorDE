"""Extract all property ID mappings from registration code"""
import struct

EXE="/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"

with open(EXE,'rb') as f:
    f.seek(0x400); d=f.read(0x42490C)

# Pattern: after call 0x140017480 (property register), the next instructions
# set a property ID: mov DWORD PTR [reg+offset], IMM
# We need to find the LEA that loaded the property name string

props=[]

# Scan for the registration sequence: 
# lea rdx,[rip+disp] → call 0x140017480 → mov DWORD PTR [...], ID
for i in range(len(d)-20):
    # Find lea rdx,[rip+disp] (48 8D 15 xx xx xx xx)
    if d[i]==0x48 and d[i+1]==0x8D and d[i+2]==0x15:
        disp_str=struct.unpack('<i',d[i+3:i+7])[0]
        str_vma=0x140001000+i+7+disp_str
        # Check if valid rdata address
        if not (0x140426000<=str_vma<0x1404DC000):
            continue
        # Look for call 0x140017480 within next 40 bytes
        for j in range(i+7, min(i+47,len(d)-5)):
            if d[j]==0xE8:
                disp_call=struct.unpack('<i',d[j+1:j+5])[0]
                call_target=0x140001000+j+5+disp_call
                if call_target==0x140017480:
                    # Found registration call. Now look for property ID after it.
                    for k in range(j+5, min(j+30,len(d)-7)):
                        if d[k]==0xC7 and d[k+2] in (0xE0,0xE4,0xE8,0xEC,0xF0,0xF4,0xF8,0xFC):
                            pid=struct.unpack('<I',d[k+3:k+7])[0]
                            if pid<0x1000:
                                # Read string
                                f.seek(0x424E00+(str_vma-0x140426000))
                                s=f.read(64).split(b'\x00')[0]
                                try:
                                    name=s.decode('ascii')
                                    if len(name)>=2:
                                        props.append((pid,name,str_vma))
                                except: pass
                            break  # Only take first ID after call
                    break

# Sort by property ID
props.sort()
print(f"Found {len(props)} property ID mappings:\n")
for pid,name,vma in props:
    print(f"  0x{pid:03x} ({pid:4d}): {name}")
