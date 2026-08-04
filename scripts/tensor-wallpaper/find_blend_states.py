"""Find D3D11CreateDevice call sites and blend state creation."""
import struct
from workspace_paths import WALLPAPER_DISTRIBUTION

EXE = WALLPAPER_DISTRIBUTION / "wallpaper64.exe"
with open(EXE, 'rb') as f:
    f.seek(0x400); d = f.read(0x42490C)

# D3D11CreateDevice IAT entry
iat_vma = 0x140426a70
target = iat_vma

print("=== D3D11CreateDevice call sites ===")
for i in range(len(d)-6):
    if d[i]==0xFF and d[i+1]==0x15:
        disp = struct.unpack('<i', d[i+2:i+6])[0]
        rv = 0x140001000 + i + 6 + disp
        if abs(rv - target) < 0x1000:
            print(f"  call D3D11CreateDevice @ 0x{0x140001000+i:x}")

# Search .rdata for blend state descriptors
# Normal blend: ONE,ZERO → SrcBlend=2,DestBlend=1
# Translucent: SRC_ALPHA,INV_SRC_ALPHA → 5,6
# Additive: SRC_ALPHA,ONE → 5,2
# AlphaToCoverage: AlphaToCoverageEnable=1, ONE,ZERO
print("\n=== Blend state descriptors in .rdata ===")
f.seek(0x424E00)
rd = f.read(0xB51AC)
# Search for AlphaToCoverageEnable=1 followed by SrcBlend/DestBlend
for i in range(len(rd)-36):
    # Check first 4 bytes = 1 (AlphaToCoverage)
    atc = struct.unpack('<I', rd[i:i+4])[0]
    if atc == 1:
        # Next 4 bytes = IndependentBlendEnable (0)
        indep = struct.unpack('<I', rd[i+4:i+8])[0]
        if indep == 0:
            # Next 4 bytes = BlendEnable
            blend_en = struct.unpack('<I', rd[i+8:i+12])[0]
            if blend_en in (0, 1):
                src = struct.unpack('<I', rd[i+12:i+16])[0]
                dst = struct.unpack('<I', rd[i+16:i+20])[0]
                src_a = struct.unpack('<I', rd[i+24:i+28])[0]
                dst_a = struct.unpack('<I', rd[i+28:i+32])[0]
                if src in (2,5) and dst in (1,2,6):
                    vma = 0x140426000 + i
                    print(f"  VMA 0x{vma:x}: A2C=1 Src={src} Dst={dst} SrcA={src_a} DstA={dst_a}")
                    # Also check if there's a non-A2C variant nearby
                    if i >= 36 and struct.unpack('<I', rd[i-36:i-32])[0] == 0:
                        nvma = 0x140426000 + i - 36
                        nsrc = struct.unpack('<I', rd[i-36+12:i-36+16])[0]
                        ndst = struct.unpack('<I', rd[i-36+16:i-36+20])[0]
                        if nsrc == src and ndst == dst:
                            print(f"    (non-A2C variant at 0x{nvma:x}: Src={nsrc} Dst={ndst})")
