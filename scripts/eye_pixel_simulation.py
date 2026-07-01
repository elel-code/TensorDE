"""End-to-end pixel simulation: WE vs Gilder for closed-eye eyelid pixel.

Simulates the full compositing pipeline for a pixel in the eyelid area
at the closed-eye frame (frame 300). Computes final RGBA for both WE and
Gilder rendering paths, then computes the final on-screen color when
composited against various backgrounds.

Key insight: this pixel was originally pupil area, but eyelid mesh covers
it after bone animation. The texture sampled is eyelid texture (v>0.65).
"""
import struct, io, math, json
from pathlib import Path

# ── 1. Extract representative pixel values from assets ──────────

# Load eye texture alpha
data = open('/tmp/gilder-we-3742497499-extracted/materials/眼睛.tex', 'rb').read()
png_data = data[data.find(b'\x89PNG'):]

from PIL import Image
img = Image.open(io.BytesIO(png_data)).convert('RGBA')
w, h = img.size
pixels = list(img.getdata())
rows = [pixels[i*w:(i+1)*w] for i in range(h)]

# Eyelid region: uv v > 0.65 → raw_v < 0.35 → texture rows 0 to int(h*0.35)
eyelid_rows = int(h * 0.35)
eyelid_pixels = []
for y in range(eyelid_rows):
    for x in range(w):
        r, g, b, a = rows[y][x]
        if a > 0:
            eyelid_pixels.append((r/255, g/255, b/255, a/255))

# Pupil region: rows int(h*0.35) to int(h*0.65)
pupil_start = int(h * 0.35)
pupil_end = int(h * 0.65)
pupil_pixels = []
for y in range(pupil_start, pupil_end):
    for x in range(w):
        r, g, b, a = rows[y][x]
        if a > 0:
            pupil_pixels.append((r/255, g/255, b/255, a/255))

# Average values
eyelid_rgb = tuple(sum(p[i] for p in eyelid_pixels) / len(eyelid_pixels) for i in range(3))
eyelid_a = sum(p[3] for p in eyelid_pixels) / len(eyelid_pixels)
pupil_rgb = tuple(sum(p[i] for p in pupil_pixels) / len(pupil_pixels) for i in range(3))
pupil_a = sum(p[3] for p in pupil_pixels) / len(pupil_pixels)

print(f"Eyelid: {len(eyelid_pixels)} non-zero pixels, avg RGB=({eyelid_rgb[0]:.3f},{eyelid_rgb[1]:.3f},{eyelid_rgb[2]:.3f}) alpha={eyelid_a:.3f}")
print(f"Pupil:  {len(pupil_pixels)} non-zero pixels, avg RGB=({pupil_rgb[0]:.3f},{pupil_rgb[1]:.3f},{pupil_rgb[2]:.3f}) alpha={pupil_a:.3f}")

# ── 2. Blend simulation functions ─────────────────────────────

def blend_alpha(src_rgb, src_a, dst_rgb, dst_a):
    """Alpha blend: src*src_a + dst*(1-src_a) for both color and alpha."""
    r = src_rgb[0]*src_a + dst_rgb[0]*(1-src_a)
    g = src_rgb[1]*src_a + dst_rgb[1]*(1-src_a)
    b = src_rgb[2]*src_a + dst_rgb[2]*(1-src_a)
    a = src_a*src_a + dst_a*(1-src_a)
    return (r, g, b), a

def blend_normal(src_rgb, src_a, dst_rgb, dst_a):
    """Normal blend: ONE/ZERO — full replace."""
    return src_rgb, src_a

def straight_composite(fg_rgb, fg_a, bg_rgb, bg_a):
    """Compositing with straight alpha: fg*fg_a + bg*bg_a*(1-fg_a)."""
    r = fg_rgb[0]*fg_a + bg_rgb[0]*bg_a*(1-fg_a)
    g = fg_rgb[1]*fg_a + bg_rgb[1]*bg_a*(1-fg_a)
    b = fg_rgb[2]*fg_a + bg_rgb[2]*bg_a*(1-fg_a)
    a = fg_a + bg_a*(1-fg_a)
    return (r, g, b), a

def premul_composite(fg_rgb, fg_a, bg_rgb, bg_a):
    """Compositing with premultiplied alpha: fg + bg*(1-fg_a)."""
    r = fg_rgb[0] + bg_rgb[0]*(1-fg_a)
    g = fg_rgb[1] + bg_rgb[1]*(1-fg_a)
    b = fg_rgb[2] + bg_rgb[2]*(1-fg_a)
    a = fg_a + bg_a*(1-fg_a)
    return (r, g, b), a

# ── 3. WE pipeline simulation ──────────────────────────────────

def simulate_we(tex_rgb, tex_a, body_rgb, mask, lock_transforms):
    """
    WE node-1530 opacity pass pipeline.
    
    lock_transforms=True: node-1530 uses OPEN eye pupil texture
    lock_transforms=False: node-1530 uses same tex as node-1336 (eyelid)
    """
    # Node-1530's texture depends on lock_transforms
    n1530_rgb = pupil_rgb if lock_transforms else tex_rgb
    n1530_a = pupil_a if lock_transforms else tex_a
    
    # Node-1336 pass 1: material → local FBO (translucent)
    fbo1_rgb = tuple(c * tex_a for c in tex_rgb)
    fbo1_a = tex_a * tex_a  # translucent alpha equation
    
    # Node-1336 pass 2-3: iris + waterripple (assume alpha preserved)
    fbo1_a_final = fbo1_a  # simplified: alpha unchanged by iris/waterripple
    
    # Node-1336 composite → scene (translucent)
    scene_rgb, scene_a = blend_alpha(
        fbo1_rgb, fbo1_a_final,
        body_rgb, 1.0  # opaque body
    )
    
    # Node-1530 pass 1: material → local FBO (translucent)
    fbo2_rgb = tuple(c * n1530_a for c in n1530_rgb)
    fbo2_a = n1530_a * n1530_a
    
    # Node-1530 pass 2: opacity → scene (Normal blend)
    # albedo.a *= mask
    fbo2_a_masked = fbo2_a * mask
    scene_rgb, scene_a = blend_normal(
        fbo2_rgb, fbo2_a_masked,
        scene_rgb, scene_a
    )
    
    # Final composite against body
    return scene_rgb, scene_a

# ── 4. Gilder pipeline simulation ──────────────────────────────

def simulate_gilder(tex_rgb, tex_a, body_rgb, mask, lock_transforms):
    """
    Gilder node-77 (iris first-class target) + node-89 (direct puppet mesh).
    
    lock_transforms=True: node-89 uses OPEN eye pupil texture
    lock_transforms=False: node-89 uses same tex as node-77 (eyelid) — CURRENT
    """
    n89_rgb = pupil_rgb if lock_transforms else tex_rgb
    n89_a = pupil_a if lock_transforms else tex_a
    
    # Node-77 step 1: base mesh → EffectTarget (Alpha blend, clear=transparent)
    et_rgb = tuple(c * tex_a for c in tex_rgb)
    et_a = tex_a * tex_a  # same as WE translucent
    
    # Node-77 step 2: final scene quad → swapchain (Alpha blend)
    swap_rgb, swap_a = blend_alpha(
        et_rgb, et_a,
        body_rgb, 1.0
    )
    
    # Node-89: direct puppet mesh → swapchain (Alpha blend — BUG: should be Normal)
    src_a = n89_a * mask
    swap_rgb, swap_a = blend_alpha(  # <-- Alpha blend (BUG)
        n89_rgb, src_a,
        swap_rgb, swap_a
    )
    
    return swap_rgb, swap_a

def simulate_gilder_fixed(tex_rgb, tex_a, body_rgb, mask, lock_transforms):
    """Gilder with Normal blend fix for node-89."""
    n89_rgb = pupil_rgb if lock_transforms else tex_rgb
    n89_a = pupil_a if lock_transforms else tex_a
    
    et_rgb = tuple(c * tex_a for c in tex_rgb)
    et_a = tex_a * tex_a
    
    swap_rgb, swap_a = blend_alpha(
        et_rgb, et_a,
        body_rgb, 1.0
    )
    
    src_a = n89_a * mask
    swap_rgb, swap_a = blend_normal(  # <-- Normal blend (FIXED)
        n89_rgb, src_a,
        swap_rgb, swap_a
    )
    
    return swap_rgb, swap_a

# ── 5. Run simulation ─────────────────────────────────────────

BODY_RGB = (0.92, 0.85, 0.78)  # skin color (body)
BODY_A = 1.0

print("\n=== Pixel simulation: eyelid area pixel ===")
print(f"  tex_rgb=eyelid({eyelid_rgb[0]:.3f},{eyelid_rgb[1]:.3f},{eyelid_rgb[2]:.3f}) tex_a={eyelid_a:.3f}")
print(f"  body_rgb=skin({BODY_RGB[0]:.2f},{BODY_RGB[1]:.2f},{BODY_RGB[2]:.2f})")

for mask_val in [0.0, 0.5, 1.0]:
    print(f"\n  --- Mask={mask_val:.1f} ---")
    
    # WE: lock_transforms=TRUE (node-1530 shows open eye pupil)
    we_rgb, we_a = simulate_we(eyelid_rgb, eyelid_a, BODY_RGB, mask_val, lock_transforms=True)
    we_final, we_final_a = straight_composite(we_rgb, we_a, BODY_RGB, BODY_A)
    
    # Gilder current: lock_transforms=FALSE (node-89 same as node-77)
    gilder_rgb, gilder_a = simulate_gilder(eyelid_rgb, eyelid_a, BODY_RGB, mask_val, lock_transforms=False)
    gilder_final, gilder_final_a = straight_composite(gilder_rgb, gilder_a, BODY_RGB, BODY_A)
    
    # Gilder fixed: lock_transforms=TRUE + Normal blend
    fixed_rgb, fixed_a = simulate_gilder_fixed(eyelid_rgb, eyelid_a, BODY_RGB, mask_val, lock_transforms=True)
    fixed_final, fixed_final_a = straight_composite(fixed_rgb, fixed_a, BODY_RGB, BODY_A)
    
    print(f"    WE (lt=True):  scene_a={we_a:.3f} final=({we_final[0]:.3f},{we_final[1]:.3f},{we_final[2]:.3f})")
    print(f"    Gilder (lt=False,Alpha): scene_a={gilder_a:.3f} final=({gilder_final[0]:.3f},{gilder_final[1]:.3f},{gilder_final[2]:.3f})")
    print(f"    Fixed (lt=True,Normal):  scene_a={fixed_a:.3f} final=({fixed_final[0]:.3f},{fixed_final[1]:.3f},{fixed_final[2]:.3f})")

# ── 6. Summary: eyelid pixel opacity ───────────────────────────
print("\n=== Summary: eyelid pixel effective opacity ===")
print(f"  Texture alpha: {eyelid_a:.3f}")
print(f"  WE (lt=True, M=1): scene_alpha = {eyelid_a**2:.3f}")
print(f"  WE (lt=True, M=0): scene_alpha = 0.0 (fully transparent)")
print(f"  Gilder (no lt, M=any): scene_alpha ≈ {simulate_gilder(eyelid_rgb, eyelid_a, BODY_RGB, 0.0, False)[1]:.3f}..{simulate_gilder(eyelid_rgb, eyelid_a, BODY_RGB, 1.0, False)[1]:.3f}")
print(f"  Gilder vs WE difference: mask has NO functional effect (no lock_transforms, same animation)")
