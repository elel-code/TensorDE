"""Check vertex opacity in eyelid region across animation frames.

Extends the skinning data to compute per-vertex opacity from bone animation.
"""
import struct, json, math

MDL = '/tmp/gilder-we-3742497499-extracted/models/眼睛_puppet.mdl'
with open(MDL, 'rb') as f:
    data = f.read()

def find_tag(tag_ver):
    for i in range(len(data) - len(tag_ver)):
        if data[i:i+len(tag_ver)] == tag_ver:
            return i
    raise ValueError(f"{tag_ver} not found")

def section_info(section_offset):
    for meta_off in [section_offset + 9, section_offset + 8]:
        if meta_off + 8 > len(data):
            continue
        end_off = struct.unpack_from('<I', data, meta_off)[0]
        count = struct.unpack_from('<I', data, meta_off + 4)[0]
        if end_off > section_offset and end_off <= len(data) and count < 1000000:
            return end_off, count, meta_off + 8
    raise ValueError(f"section at 0x{section_offset:08X}")

mdls_off = find_tag(b'MDLS0004')
mdla_off = find_tag(b'MDLA0006')
mdls_end, bone_count, mdls_data_start = section_info(mdls_off)
mdla_end, clip_count, mdla_data_start = section_info(mdla_off)

# Parse bones: parent + bind matrix
pos = mdls_data_start
bones = []
for bi in range(bone_count):
    idx = struct.unpack_from('<I', data, pos)[0]; pos += 4
    flags = data[pos]; pos += 1
    parent = struct.unpack_from('<i', data, pos)[0]; pos += 4
    entry_bytes = struct.unpack_from('<I', data, pos)[0]; pos += 4
    mat = list(struct.unpack_from('<16f', data, pos)); pos += 64
    pos += entry_bytes - 64
    while pos < len(data) and data[pos] != 0: pos += 1
    pos += 1
    bones.append((parent, mat))

# Parse mesh
MARKER_SIZE = 9; MESH_HEADER_SIZE = 8; VERTEX_STRIDE = 80
mesh_start = vertex_count = None
for offset in range(MARKER_SIZE, mdls_off - MESH_HEADER_SIZE - 4):
    vertex_bytes = struct.unpack_from('<I', data, offset + 4)[0]
    verts_offset = offset + MESH_HEADER_SIZE
    idx_len_off = verts_offset + vertex_bytes
    if vertex_bytes == 0 or vertex_bytes % VERTEX_STRIDE != 0: continue
    if idx_len_off + 4 > mdls_off: continue
    index_bytes = struct.unpack_from('<I', data, idx_len_off)[0]
    indices_off = idx_len_off + 4
    if index_bytes == 0 or index_bytes % 6 != 0: continue
    if indices_off + index_bytes > mdls_off: continue
    mesh_start = verts_offset; vertex_count = vertex_bytes // VERTEX_STRIDE; break

vertices = []
for vi in range(vertex_count):
    off = mesh_start + vi * VERTEX_STRIDE
    x, y, z = struct.unpack_from('<3f', data, off)
    u, raw_v = struct.unpack_from('<2f', data, off + 72)
    v = 1.0 - raw_v
    b0 = struct.unpack_from('<I', data, off + 40)[0]
    b1 = struct.unpack_from('<I', data, off + 44)[0]
    b2 = struct.unpack_from('<I', data, off + 48)[0]
    b3 = struct.unpack_from('<I', data, off + 52)[0]
    w0, w1, w2, w3 = struct.unpack_from('<4f', data, off + 56)
    vertices.append({'x':x,'y':y,'z':z,'u':u,'v':v,'bones':[b0,b1,b2,b3],'weights':[w0,w1,w2,w3]})


print(f"Parsed {len(vertices)} vertices")

# Parse MDLA animation with opacity tracks
pos = mdla_data_start
clip_id = struct.unpack_from('<I', data, pos)[0]; pos += 4
clip_flags = struct.unpack_from('<I', data, pos)[0]; pos += 4
# skip name
while pos < len(data) and data[pos] != 0: pos += 1
pos += 1
# skip playback
while pos < len(data) and data[pos] != 0: pos += 1
pos += 1
fps = struct.unpack_from('<f', data, pos)[0]; pos += 4
frame_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
reserved = struct.unpack_from('<I', data, pos)[0]; pos += 4
clip_bone_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
sample_count = frame_count + 1

print(f"Clip id={clip_id} fps={fps} frames={frame_count} bones={clip_bone_count} samples={sample_count}")

# Parse per-bone animation
bone_anim = []
for bi in range(clip_bone_count):
    flags = struct.unpack_from('<I', data, pos)[0]; pos += 4
    byte_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
    frames = []
    for s in range(sample_count):
        tx, ty, tz = struct.unpack_from('<3f', data, pos); pos += 12
        rx, ry, rz = struct.unpack_from('<3f', data, pos); pos += 12
        sx, sy, sz = struct.unpack_from('<3f', data, pos); pos += 12
        frames.append({'t':(tx,ty,tz),'r':(rx,ry,rz),'s':(sx,sy,sz),'opacity':1.0})
    bone_anim.append(frames)

# Parse opacity tracks (after all bone data)
# Scan for the opacity block: each bone has sample_count*4 bytes
opacity_start = pos
# Try to parse opacity tracks (same as Rust: scan for valid block)
track_bytes = sample_count * 4
block_bytes = track_bytes + 8
for preamble in range(0, 17):
    base = opacity_start + preamble
    total = base + bone_count * block_bytes
    if total > len(data): continue
    valid = True
    for bi in range(bone_count):
        block = base + bi * block_bytes
        bc = struct.unpack_from('<I', data, block + 4)[0]
        if bc != track_bytes:
            valid = False; break
    if valid:
        print(f"Opacity tracks at offset 0x{base:08X} preamble={preamble}")
        for bi in range(bone_count):
            block = base + bi * block_bytes
            data_start = block + 8
            data_end = data_start + track_bytes
            opacity_values = []
            for off in range(data_start, data_end, 4):
                opacity_values.append(struct.unpack_from('<f', data, off)[0])
            for s in range(min(sample_count, len(opacity_values))):
                bone_anim[bi][s]['opacity'] = max(0.0, min(1.0, opacity_values[s]))
        break
else:
    print("No opacity tracks found")

# Report non-default opacity bones
for bi in range(bone_count):
    op_vals = [f['opacity'] for f in bone_anim[bi]]
    if any(abs(v - 1.0) > 0.001 for v in op_vals):
        print(f"Bone {bi}: opacity range [{min(op_vals):.4f}..{max(op_vals):.4f}]")

# Compute vertex opacity at frames 0 and 300
def vertex_opacity_at_frame(v, frame_idx):
    opacity = 0.0
    total_w = 0.0
    for slot in range(4):
        w = v['weights'][slot]
        if w <= 0.001: continue
        bi = v['bones'][slot]
        if bi >= bone_count: continue
        bone_op = bone_anim[bi][min(frame_idx, sample_count-1)]['opacity']
        opacity += bone_op * w
        total_w += w
    return opacity / total_w if total_w > 0.001 else 1.0

for frame_idx, label in [(0, 'open'), (300, 'closed')]:
    # Compute opacity for all vertices
    opacities = [vertex_opacity_at_frame(v, frame_idx) for v in vertices]
    
    # Regions
    upper_lid = [i for i,v in enumerate(vertices) if v['v'] > 0.65]
    lower_lid = [i for i,v in enumerate(vertices) if v['v'] < 0.35]
    pupil = [i for i,v in enumerate(vertices) if 0.30 < v['u'] < 0.70 and 0.35 < v['v'] < 0.65]
    
    for region_name, indices in [('upper_eyelid', upper_lid), ('lower_eyelid', lower_lid), ('pupil', pupil)]:
        if not indices: continue
        ops = [opacities[i] for i in indices]
        below_1 = sum(1 for o in ops if o < 0.99)
        below_half = sum(1 for o in ops if o < 0.5)
        print(f"Frame {frame_idx} ({label}) {region_name} ({len(indices)} vtx): opacity=[{min(ops):.4f}..{max(ops):.4f}] <1.0:{below_1} <0.5:{below_half}")
    
    # Check for non-1.0 opacity vertices in upper eyelid
    low_op_ue = [(opacities[i], vertices[i]['u'], vertices[i]['v'], i) for i in upper_lid if opacities[i] < 0.99]
    if low_op_ue:
        print(f"  Non-1.0 upper eyelid vertices ({len(low_op_ue)}):")
        for op, u, v, idx in sorted(low_op_ue)[:10]:
            print(f"    vtx[{idx}] uv=({u:.3f},{v:.3f}) opacity={op:.4f}")

# ── Compute eyelid coverage across animation frames ────────────
# Quick Euler-to-matrix, skinning, etc (copied from skinning script)
def mat_mul(a, b):
    r = [0.0]*16
    for i in range(4):
        for j in range(4):
            s = sum(a[i + k*4] * b[k + j*4] for k in range(4))
            r[i + j*4] = s
    return r

def euler_to_matrix(rx, ry, rz, tx, ty, tz, sx, sy, sz):
    cx=math.cos(rx);sx=math.sin(rx);cy=math.cos(ry);sy=math.sin(ry);cz=math.cos(rz);sz=math.sin(rz)
    rx_m=[1,0,0,0,0,cx,-sx,0,0,sx,cx,0,0,0,0,1]
    ry_m=[cy,0,sy,0,0,1,0,0,-sy,0,cy,0,0,0,0,1]
    rz_m=[cz,-sz,0,0,sz,cz,0,0,0,0,1,0,0,0,0,1]
    rot=mat_mul(mat_mul(rz_m,ry_m),rx_m)
    sc=[sx,0,0,0,0,sy,0,0,0,0,sz,0,0,0,0,1]
    tr=[1,0,0,0,0,1,0,0,0,0,1,0,tx,ty,tz,1]
    return mat_mul(tr,mat_mul(rot,sc))

def compute_coverage(bind_world, inverse_bind, frame_idx):
    # Compute pose for given frame
    local_pose = []
    for bi in range(bone_count):
        f = bone_anim[bi][min(frame_idx, sample_count-1)]
        local_pose.append(euler_to_matrix(f['r'][0],f['r'][1],f['r'][2],f['t'][0],f['t'][1],f['t'][2],f['s'][0],f['s'][1],f['s'][2]))
    # Compute pose_world
    pw = [None]*bone_count
    for bi in range(bone_count):
        parent,_ = bones[bi]
        if parent < 0: pw[bi] = list(local_pose[bi])
        elif pw[parent] is not None: pw[bi] = mat_mul(pw[parent], local_pose[bi])
    # Skin and get eyelid/pupil y
    ue_y, pupil_y = [], []
    for vi,v in enumerate(vertices):
        wx,wy,wz,tw = 0,0,0,0
        for slot in range(4):
            w = v['weights'][slot]
            if w <= 0.001: continue
            bi = v['bones'][slot]
            if bi >= bone_count or pw[bi] is None: continue
            sm = mat_mul(pw[bi], inverse_bind[bi])
            pz = v.get('z', 0.0)
            px = sm[0]*v['x']+sm[4]*v['y']+sm[8]*pz+sm[12]
            py = sm[1]*v['x']+sm[5]*v['y']+sm[9]*pz+sm[13]
            wx+=px*w; wy+=py*w; tw+=w
        if tw>0.001: wy/=tw
        if v['v']>0.65: ue_y.append(wy)
        if 0.30<v['u']<0.70 and 0.35<v['v']<0.65: pupil_y.append(wy)
    if len(ue_y) < 3 or len(pupil_y) < 3: return -1
    pmax, pmin = max(pupil_y), min(pupil_y)
    uemin = min(ue_y)
    if abs(pmax - pmin) < 0.001: return -1
    return (pmax - uemin) / (pmax - pmin)

# Compute bind_world
bind_world = [None]*bone_count
for bi in range(bone_count):
    parent, local = bones[bi]
    if parent < 0: bind_world[bi] = list(local)
    elif bind_world[parent] is not None: bind_world[bi] = mat_mul(bind_world[parent], local)

def mat_inv(m):
    a00=m[0];a01=m[4];a02=m[8];a10=m[1];a11=m[5];a12=m[9];a20=m[2];a21=m[6];a22=m[10]
    det=a00*(a11*a22-a12*a21)-a01*(a10*a22-a12*a20)+a02*(a10*a21-a11*a20)
    if abs(det)<1e-12: return None
    id=1/det
    b00=(a11*a22-a12*a21)*id;b01=(a02*a21-a01*a22)*id;b02=(a01*a12-a02*a11)*id
    b10=(a12*a20-a10*a22)*id;b11=(a00*a22-a02*a20)*id;b12=(a02*a10-a00*a12)*id
    b20=(a10*a21-a11*a20)*id;b21=(a01*a20-a00*a21)*id;b22=(a00*a11-a01*a10)*id
    tx=m[12];ty=m[13];tz=m[14]
    return [b00,b10,b20,0,b01,b11,b21,0,b02,b12,b22,0,
            -(b00*tx+b01*ty+b02*tz),-(b10*tx+b11*ty+b12*tz),-(b20*tx+b21*ty+b22*tz),1]

gilder_ib = [mat_inv(bw) for bw in bind_world]

# Parse MDLE
mdle_off = find_tag(b'MDLE0002')
mdle_end, _, mdle_data_start = section_info(mdle_off)
mdle_mats = []
for i in range(bone_count):
    mdle_mats.append(list(struct.unpack_from('<16f', data, mdle_data_start + i*64)))

print()
for frame_idx in [0, 150, 300, 450, 599]:
    g_cov = compute_coverage(bind_world, gilder_ib, frame_idx)
    m_cov = compute_coverage(bind_world, mdle_mats, frame_idx)
    print(f"Frame {frame_idx:3d}: Gilder coverage={g_cov*100:.0f}% MDLE coverage={m_cov*100:.0f}%")
