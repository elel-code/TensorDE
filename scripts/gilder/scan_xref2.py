import subprocess, sys
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"

def find_xrefs(target_vma, max_results=12):
    """Scan .text section for RIP-relative LEA/MOV references to target_vma"""
    text_file_off = 0x400
    text_size = 0x42490C
    text_vma_base = 0x140001000
    
    results = []
    with open(EXE, 'rb') as f:
        f.seek(text_file_off)
        data = f.read(text_size)
    
    i = 0
    while i < len(data) - 7:
        # Check for REX.W prefix + LEA/MOV opcode
        if data[i] == 0x48 and data[i+1] in (0x8D, 0x8B):
            modrm = data[i+2]
            if (modrm & 0xC7) == 0x05:  # mod=00, rm=101 → RIP-relative
                disp = int.from_bytes(data[i+3:i+7], 'little', signed=True)
                instr_vma = text_vma_base + i
                ref_vma = instr_vma + 7 + disp
                if abs(ref_vma - target_vma) < 0x2000:
                    results.append((instr_vma, 'LEA' if data[i+1] == 0x8D else 'MOV', ref_vma))
                    if len(results) >= max_results:
                        break
        i += 1
    return results

# Key targets for disassembly
targets = [
    ("Failed loading effect", 0x140490758),
    ("operator", 0x14048fc88),
    ("initializer", 0x14048fce8),
    ("emitter", 0x14048fd20),
    ("renderer", 0x14048fd28),
    ("TRAILRENDERER", 0x140490138),
]

for name, vma in targets:
    print(f"=== {name} (VMA 0x{vma:x}) ===")
    xrefs = find_xrefs(vma)
    for x in xrefs:
        print(f"  XREF @ 0x{x[0]:x} ({x[1]} -> 0x{x[2]:x})")
    if not xrefs:
        print(f"  (no RIP-relative xrefs found)")
    print()
