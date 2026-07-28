"""Find the blend mode dispatch code in WE exe."""
import struct
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"

with open(EXE, 'rb') as f:
    f.seek(0x400)
    text = f.read(0x42490C)

# Find all 'call [rax+0xC0]' sites (CreateBlendState on ID3D11Device)
create_blend = []
for i in range(len(text) - 3):
    if text[i] == 0xFF and text[i+1] == 0x50 and text[i+2] == 0xC0:
        create_blend.append(0x140001000 + i)

print(f"=== CreateBlendState (call [rax+0xC0]) sites: {len(create_blend)} ===")
for addr in create_blend:
    print(f"  0x{addr:x}")

# Find 'cmp BYTE PTR [reg+off], 3' patterns (comparing blend mode)
print("\n=== cmp byte with 3 near alphatocoverage refs ===")
target_vma = 0x14048b510  # "alphatocoverage" string
for i in range(len(text) - 7):
    if text[i] == 0x48 and text[i+1] in (0x8D, 0x8B):
        modrm = text[i+2]
        if (modrm & 0xC7) == 0x05:
            disp = struct.unpack('<i', text[i+3:i+7])[0]
            ref_vma = 0x140001000 + i + 7 + disp
            if abs(ref_vma - target_vma) < 0x2000:
                instr_vma = 0x140001000 + i
                # Look at surrounding code for cmp with small values
                start = max(0, i - 60)
                chunk = text[start:i+30]
                # search for 83 F8 03 or 83 F9 03 or 80 xx 03
                for j in range(len(chunk) - 3):
                    if chunk[j:j+3] == b'\x83\xf8\x03':
                        print(f"  cmp eax,3 at 0x{0x140001000+start+j:x} (near alphatocoverage ref at 0x{instr_vma:x})")
                    elif chunk[j:j+3] == b'\x83\xf9\x03':
                        print(f"  cmp ecx,3 at 0x{0x140001000+start+j:x} (near alphatocoverage ref at 0x{instr_vma:x})")
