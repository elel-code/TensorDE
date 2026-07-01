"""Extract PNG from WE .tex file and check eyelid alpha channel."""
import struct

data = open('/tmp/gilder-we-3742497499-extracted/materials/眼睛.tex', 'rb').read()
png_start = data.find(b'\x89PNG')
if png_start < 0:
    print("PNG not found in .tex")
    exit(1)

png_data = data[png_start:]
with open('/tmp/eye_texture.png', 'wb') as f:
    f.write(png_data)

ihdr_start = png_data.find(b'IHDR')
w = struct.unpack('>I', png_data[ihdr_start+4:ihdr_start+8])[0]
h = struct.unpack('>I', png_data[ihdr_start+8:ihdr_start+12])[0]
print(f"Extracted PNG: {w}x{h} at offset {png_start}")

# Decode PNG pixels using pypng if available, otherwise start simple
try:
    import png
    reader = png.Reader(bytes=png_data)
    img = reader.asRGBA8()
    width, height, rows, info = img
    pixels = list(rows)

    # Eyelid region: top ~35% of texture (v_converted > 0.65 -> raw_v < 0.35 -> top rows)
    eyelid_top = 0
    eyelid_bottom = int(height * 0.35)  # top 35% = upper eyelid area
    
    alphas = []
    for y in range(eyelid_top, eyelid_bottom):
        for x in range(width):
            # RGBA: bytes[y][x*4+3]
            a = rows[y][x*4+3]
            alphas.append(a)
    
    below_255 = sum(1 for a in alphas if a < 255)
    below_128 = sum(1 for a in alphas if a < 128)
    below_64 = sum(1 for a in alphas if a < 64)
    total = len(alphas)
    
    print(f"Eyelid region (rows 0-{eyelid_bottom-1}): {total} pixels")
    print(f"  alpha<255: {below_255} ({below_255*100/total:.1f}%)")
    print(f"  alpha<128: {below_128} ({below_128*100/total:.1f}%)")
    print(f"  alpha<64:  {below_64} ({below_64*100/total:.1f}%)")
    print(f"  alpha range: [{min(alphas)}..{max(alphas)}]")
    
    # Also check alpha in pupil region (middle)
    pupil_top = int(height * 0.35)
    pupil_bottom = int(height * 0.65)
    palphas = []
    for y in range(pupil_top, pupil_bottom):
        for x in range(width):
            palphas.append(rows[y][x*4+3])
    pbelow_255 = sum(1 for a in palphas if a < 255)
    print(f"\nPupil region (rows {pupil_top}-{pupil_bottom-1}): {len(palphas)} pixels")
    print(f"  alpha<255: {pbelow_255} ({pbelow_255*100/len(palphas):.1f}%)")
    print(f"  alpha range: [{min(palphas)}..{max(palphas)}]")

except ImportError:
    print("pypng not available, trying PIL...")
    try:
        import io
        from PIL import Image
        img = Image.open(io.BytesIO(png_data)).convert('RGBA')
        w, h = img.size
        pixels = list(img.getdata())
        # Reshape to rows
        rows = [pixels[i*w:(i+1)*w] for i in range(h)]
        
        eyelid_bottom = int(h * 0.35)
        alphas = []
        for y in range(0, eyelid_bottom):
            for x in range(w):
                alphas.append(rows[y][x][3])
        
        below_255 = sum(1 for a in alphas if a < 255)
        total = len(alphas)
        
        print(f"Eyelid region (top {eyelid_bottom} rows): {total} pixels")
        print(f"  alpha<255: {below_255} ({below_255*100/total:.1f}%)")
        print(f"  alpha range: [{min(alphas)}..{max(alphas)}]")
    except ImportError:
        print("Neither pypng nor PIL available")
