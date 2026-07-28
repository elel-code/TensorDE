"""Find the vtable that has method at offset 0x110"""
import struct
from workspace_paths import WALLPAPER_DISTRIBUTION

f=open(WALLPAPER_DISTRIBUTION / 'wallpaper64.exe','rb')
f.seek(0x424E00); rd=f.read(0xB51AC)

# Find all vtables in rdata with >=35 entries
# Entry at index 34 (offset 0x110) is the render method
text_start=0x140001000; text_end=0x140426000
vtables=[]
run_start=None; run=[]
for i in range(0,len(rd)-8,8):
    v=struct.unpack('<Q',rd[i:i+8])[0]
    if text_start<=v<text_end:
        if run_start is None: run_start=i
        run.append(v)
    else:
        if len(run)>=35:
            vtables.append((0x140426000+run_start,len(run),list(run)))
        run_start=None; run=[]
if run and len(run)>=35:
    vtables.append((0x140426000+run_start,len(run),list(run)))

print(f'Vtables with >=35 entries: {len(vtables)}')
for vma,sz,ptrs in vtables:
    m34=ptrs[34] if sz>34 else 0
    print(f'  0x{vma:x} [{sz}] [34]=0x{m34:016x}')
