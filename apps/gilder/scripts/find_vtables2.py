"""Find vtables with 34+ entries (render context candidate)"""
import struct
EXE = "/home/yk/Code/Gilder/artifacts/wallpaper-engine-workshop/steamcmd-root/distribution/wallpaper64.exe"

text_base = 0x140001000
text_end = 0x140426000

with open(EXE, 'rb') as f:
    # Read .rdata
    f.seek(0x424E00)
    rdata = f.read(0xB51AC)

# Find runs of consecutive .text pointers (vtable candidates)
vtables = []
run_start = None
run_ptrs = []
for i in range(0, len(rdata) - 8, 8):
    v = struct.unpack('<Q', rdata[i:i+8])[0]
    if text_base <= v < text_end:
        if run_start is None:
            run_start = i
        run_ptrs.append(v)
    else:
        if 8 <= len(run_ptrs) <= 200:
            vtables.append((0x140426000 + run_start, len(run_ptrs), list(run_ptrs)))
        run_start = None
        run_ptrs = []
if run_ptrs and 8 <= len(run_ptrs) <= 200:
    vtables.append((0x140426000 + run_start, len(run_ptrs), list(run_ptrs)))

print(f"Total vtables (8-200 entries): {len(vtables)}")

# Filter for vtables with entry at index 34 (offset 0x110)
large = [(vma, sz, ptrs) for vma, sz, ptrs in vtables if sz > 34]
print(f"Vtables with >34 entries: {len(large)}")
for vma, sz, ptrs in large:
    print(f"\n  Vtable 0x{vma:x} ({sz} entries):")
    for idx in [0, 1, 2, 3, 4, 34, 35, 36]:
        if idx < len(ptrs):
            print(f"    [{idx:2d}] 0x{ptrs[idx]:016x}")
