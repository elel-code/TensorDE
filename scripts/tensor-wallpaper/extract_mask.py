"""Extract embedded image from FreeImage .tex file"""
import struct, sys
from workspace_paths import REVERSE_ENGINEERED_ROOT

tex_path = REVERSE_ENGINEERED_ROOT / "extracted/3742497499/materials/masks/opacity_mask_d2f87f99.tex"
with open(tex_path,'rb') as f:
    d=f.read()

png_off=d.find(b'\x89PNG')
jpg_off=d.find(b'\xff\xd8')
print(f'File size: {len(d)}')
print(f'PNG magic at: {png_off}')
print(f'JPEG magic at: {jpg_off}')

if png_off>0:
    ext='png'
    out_path='/tmp/opacity_mask.png'
    img_data=d[png_off:]
elif jpg_off>0:
    ext='jpg'
    out_path='/tmp/opacity_mask.jpg'
    img_data=d[jpg_off:]
else:
    print('No PNG/JPEG found in TEX file')
    # Show raw TEXB data
    print('TEXB section at 0x34:', d[0x34:0x54].hex())
    sys.exit(1)

with open(out_path,'wb') as f:
    f.write(img_data)
print(f'Extracted {ext} ({len(img_data)} bytes) to {out_path}')
print(f'Dimensions: 331x115 (from TEX header)')
