"""Parse MDLE0002 from 眼睛_puppet.mdl, compare with Gilder's computed inverse_bind."""
import struct, json, sys, math
sys.path.insert(0, '.')
from pathlib import Path

MDL = '/tmp/gilder-we-3742497499-extracted/models/眼睛_puppet.mdl'

# ── 1. Parse raw MDL sections ──────────────────────────────────
with open(MDL, 'rb') as f:
    data = f.read()

# Show all MDLS/MDLE occurrences for debugging
print("All MDLS occurrences:")
for i in range(len(data)-4):
    if data[i:i+4] == b'MDLS':
        print(f"  0x{i:08X}: {data[i:i+20].hex()}")
print("\nAll MDLE occurrences:")
for i in range(len(data)-4):
    if data[i:i+4] == b'MDLE':
        print(f"  0x{i:08X}: {data[i:i+20].hex()}")

# The real sections: look for "MDLS0004" and "MDLE0002" patterns
# In MDL format: TAG + 4-byte version + 1 padding + 4-byte end_offset + 4-byte count
def find_real_section(tag_ver):
    for i in range(len(data)-len(tag_ver)):
        if data[i:i+len(tag_ver)] == tag_ver:
            return i
    return None

mdls_real = find_real_section(b'MDLS0004')
mdla_real = find_real_section(b'MDLA0006')
mdle_real = find_real_section(b'MDLE0002')
print(f"\nReal MDLS0004 at 0x{mdls_real:08X}" if mdls_real else "\nMDLS0004 not found")
print(f"Real MDLA0006 at 0x{mdla_real:08X}" if mdla_real else "MDLA0006 not found")
print(f"Real MDLE0002 at 0x{mdle_real:08X}" if mdle_real else "MDLE0002 not found")


# ── 2. Parse MDLS bones ────────────────────────────────────────
# MDL section format: TAG + 4-byte-version + [1 padding byte] + 4-byte section_end + 4-byte count
# The section_end is an absolute file offset. Count is at metadata_offset + 4.
# metadata_offset is either section_offset + 8 or + 9
mdls_off = mdls_real
mdle_off = mdle_real

def section_end_count(bytes_data, section_offset):
    for meta_off in [section_offset + 9, section_offset + 8]:
        if meta_off + 8 > len(bytes_data):
            continue
        end_off = struct.unpack_from('<I', bytes_data, meta_off)[0]
        count = struct.unpack_from('<I', bytes_data, meta_off + 4)[0]
        if end_off > section_offset and end_off <= len(bytes_data) and count < 1000000:
            return end_off, count
    raise ValueError(f"Cannot parse section at 0x{section_offset:08X}")

mdls_end, bone_count = section_end_count(data, mdls_off)
mdle_end, _ = section_end_count(data, mdle_off)
mdle_sz = mdle_end - mdle_off - 8

print(f"MDLS: offset=0x{mdls_off:08X} end=0x{mdls_end:08X} bone_count={bone_count}")
print(f"MDLE: offset=0x{mdle_off:08X} end=0x{mdle_end:08X} data_size={mdle_sz}")

pos = mdls_off + 14  # TAG(4) + version(4) + padding(1) + end_offset(4) + count(4) = 17? No...
# Actually: TAG(4) + version(4) = 8, + [1 pad] = 9, + end_off(4) = 13, + count(4) = 17
# So data starts at 17 (if pad) or 16 (if no pad)
# The Rust code tries both: offset+8 or offset+9 for metadata, then +8 more for data
# metadata at offset+8 means: TAG(4)+version(4) = 8, then end+count at 8..16, data at 16
# metadata at offset+9 means: TAG(4)+version(4)+pad(1) = 9, then end+count at 9..17, data at 17
metadata_offset = mdls_off + 8 if mdls_end > 0 and mdls_end == struct.unpack_from('<I', data, mdls_off+8)[0] else mdls_off + 9
# Actually: let's just use the convention from Rust: section_end_count_start returns (end, count, position)
# where position is section_offset + 8 + 4 + 4 = section_offset + 16, or + 9 + 4 + 4 = + 17
# Let me just try both
for meta_off in [mdls_off + 9, mdls_off + 8]:
    test_end = struct.unpack_from('<I', data, meta_off)[0]
    if test_end == mdls_end:
        pos = meta_off + 8  # skip end(4) + count(4)
        break
else:
    pos = mdls_off + 17
print(f"\nMDLS bone_count={bone_count}")

class Bone:
    def __init__(self):
        pass

mdls_bones = []
for bi in range(bone_count):
    idx    = struct.unpack_from('<I', data, pos)[0]; pos += 4
    flags  = data[pos]; pos += 1
    parent = struct.unpack_from('<i', data, pos)[0]; pos += 4
    entry_bytes = struct.unpack_from('<I', data, pos)[0]; pos += 4
    # Read 4x4 f32 matrix (column-major little-endian)
    mat = list(struct.unpack_from('<16f', data, pos))
    pos += 64
    skip = entry_bytes - 64
    pos += skip
    # C-string name
    name_end = data.index(0, pos)
    name = data[pos:name_end].decode('ascii', errors='replace')
    pos = name_end + 1
    if bi < 5:
        print(f"  bone[{bi}] parent={parent} name={name}")
    mdls_bones.append({'idx': bi, 'parent': parent, 'name': name, 'bind_local': mat})

# ── 3. Parse MDLE matrices ─────────────────────────────────────
# MDLE data starts after section header (TAG(4) + version(4) + header)
# Find the metadata offset for MDLE
for meta_off in [mdle_off + 9, mdle_off + 8]:
    test_end = struct.unpack_from('<I', data, meta_off)[0]
    if test_end == mdle_end:
        mdle_data_start = meta_off + 8
        break
else:
    mdle_data_start = mdle_off + 17

mdle_data = data[mdle_data_start:mdle_end]
expected_mdle_bytes = bone_count * 64
print(f"MDLE data: start=0x{mdle_data_start:08X} end=0x{mdle_end:08X} bytes={len(mdle_data)} expected={expected_mdle_bytes}")

mdle_matrices = []
for i in range(min(bone_count, len(mdle_data)//64)):
    mat = list(struct.unpack_from('<16f', mdle_data, i*64))
    mdle_matrices.append(mat)
print(f"MDLE matrices parsed: {len(mdle_matrices)}")

# ── 4. Compute Gilder's bind_world and inverse_bind_world ──────
def mat_mul(a, b):
    """4x4 * 4x4 (column-major arrays, a*b)."""
    r = [0.0]*16
    for i in range(4):
        for j in range(4):
            s = 0.0
            for k in range(4):
                s += a[i + k*4] * b[k + j*4]
            r[i + j*4] = s
    return r

def mat_inv(m):
    """Invert affine 4x4 (column-major). Returns None if singular."""
    a00=m[0]; a01=m[4]; a02=m[8]
    a10=m[1]; a11=m[5]; a12=m[9]
    a20=m[2]; a21=m[6]; a22=m[10]
    det = a00*(a11*a22-a12*a21) - a01*(a10*a22-a12*a20) + a02*(a10*a21-a11*a20)
    if abs(det) < 1e-12: return None
    inv_det = 1.0/det
    b00=(a11*a22-a12*a21)*inv_det; b01=(a02*a21-a01*a22)*inv_det; b02=(a01*a12-a02*a11)*inv_det
    b10=(a12*a20-a10*a22)*inv_det; b11=(a00*a22-a02*a20)*inv_det; b12=(a02*a10-a00*a12)*inv_det
    b20=(a10*a21-a11*a20)*inv_det; b21=(a01*a20-a00*a21)*inv_det; b22=(a00*a11-a01*a10)*inv_det
    tx=m[12]; ty=m[13]; tz=m[14]
    return [b00,b10,b20,0, b01,b11,b21,0, b02,b12,b22,0,
            -(b00*tx+b01*ty+b02*tz), -(b10*tx+b11*ty+b12*tz), -(b20*tx+b21*ty+b22*tz), 1.0]

# Compute Gilder-style bind_world (accumulate through parents)
bind_world = [None]*bone_count
for bi in range(bone_count):
    local = mdls_bones[bi]['bind_local']
    parent = mdls_bones[bi]['parent']
    if parent < 0:
        bind_world[bi] = list(local)
    elif bind_world[parent] is not None:
        bind_world[bi] = mat_mul(bind_world[parent], local)

gilder_inverse_bind = []
for bi in range(bone_count):
    inv = mat_inv(bind_world[bi])
    gilder_inverse_bind.append(inv)

# ── 5. Compare MDLE vs Gilder inverse_bind ─────────────────────
print("\n=== MDLE vs Gilder inverse_bind comparison ===")
print(f"{'bone':>5} {'parent':>6} {'name':>18} {'tx_diff':>10} {'ty_diff':>10} {'tz_diff':>10} {'max_diff':>10} {'max_diag':>10}")
diffs = []
for bi in range(bone_count):
    g = gilder_inverse_bind[bi]
    m = mdle_matrices[bi]
    tx_d = abs(g[12] - m[12])
    ty_d = abs(g[13] - m[13])
    tz_d = abs(g[14] - m[14])
    # Max element-wise diff
    max_d = max(abs(g[i] - m[i]) for i in range(16))
    # Max diff in rotation part only (top-left 3x3)
    rot_d = 0.0
    for r in range(3):
        for c in range(3):
            rot_d = max(rot_d, abs(g[r + c*4] - m[r + c*4]))
    diffs.append((tx_d, ty_d, tz_d, max_d, rot_d, bi))
    if max_d > 0.01:
        name = mdls_bones[bi]['name']
        par = mdls_bones[bi]['parent']
        print(f"{bi:5d} {par:6d} {name:>18} {tx_d:10.4f} {ty_d:10.4f} {tz_d:10.4f} {max_d:10.4f} {rot_d:10.4f}")

# Sort by max diff
diffs.sort(key=lambda x: -x[3])
print(f"\nTop 15 bones by max_diff:")
for tx_d, ty_d, tz_d, max_d, rot_d, bi in diffs[:15]:
    name = mdls_bones[bi]['name']
    par = mdls_bones[bi]['parent']
    print(f"  bone={bi:3d} parent={par:3d} name={name:>20s} max_diff={max_d:.6f} rot_diff={rot_d:.6f} tx={tx_d:.4f} ty={ty_d:.4f} tz={tz_d:.4f}")

# ── 6. Save for later use ──────────────────────────────────────
out = {
    'mdle_matrices': mdle_matrices,
    'gilder_inverse_bind': gilder_inverse_bind,
    'mdls_bones': mdls_bones,
    'bone_count': bone_count
}
with open('/tmp/mdle_comparison.json', 'w') as f:
    json.dump(out, f)
print(f"\nSaved to /tmp/mdle_comparison.json")
