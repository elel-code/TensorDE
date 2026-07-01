"""Skinning comparison: MDLE vs Gilder inverse_bind at open/closed eye frames.

Parses 眼睛_puppet.mdl:
- Mesh vertices + skin weights
- MDLA animation for frames 0 (open eye) and ~300 (closed eye)
- MDLE inverse bind matrices

Computes skinned vertex positions using both inverse_bind variants,
then compares pupil-region vertex positions between the two.
"""
import struct, json, sys, math
from pathlib import Path

MDL = '/tmp/gilder-we-3742497499-extracted/models/眼睛_puppet.mdl'
with open(MDL, 'rb') as f:
    data = f.read()

# ── Section offsets ────────────────────────────────────────────
def find_tag(tag_ver):
    for i in range(len(data) - len(tag_ver)):
        if data[i:i+len(tag_ver)] == tag_ver:
            return i
    raise ValueError(f"Tag {tag_ver} not found")

def section_info(section_offset):
    for meta_off in [section_offset + 9, section_offset + 8]:
        if meta_off + 8 > len(data):
            continue
        end_off = struct.unpack_from('<I', data, meta_off)[0]
        count = struct.unpack_from('<I', data, meta_off + 4)[0]
        if end_off > section_offset and end_off <= len(data) and count < 1000000:
            data_start = meta_off + 8
            return end_off, count, data_start
    raise ValueError(f"Cannot parse section at 0x{section_offset:08X}")

mdls_off = find_tag(b'MDLS0004')
mdla_off = find_tag(b'MDLA0006')
mdle_off = find_tag(b'MDLE0002')

mdls_end, bone_count, mdls_data_start = section_info(mdls_off)
mdla_end, clip_count, mdla_data_start = section_info(mdla_off)
mdle_end, _, mdle_data_start = section_info(mdle_off)

print(f"Bones: {bone_count}, Clips: {clip_count}")
print(f"MDLS data: 0x{mdls_data_start:08X}..0x{mdls_end:08X}")
print(f"MDLA data: 0x{mdla_data_start:08X}..0x{mdla_end:08X}")
print(f"MDLE data: 0x{mdle_data_start:08X}..0x{mdle_end:08X}")

# ── Parse MDLS bones (parent + bind matrix) ────────────────────
pos = mdls_data_start
bones = []  # (parent, bind_local_matrix)
for bi in range(bone_count):
    idx = struct.unpack_from('<I', data, pos)[0]; pos += 4
    flags = data[pos]; pos += 1
    parent = struct.unpack_from('<i', data, pos)[0]; pos += 4
    entry_bytes = struct.unpack_from('<I', data, pos)[0]; pos += 4
    mat = list(struct.unpack_from('<16f', data, pos)); pos += 64
    pos += entry_bytes - 64  # skip tail
    # skip C-string name
    while pos < len(data) and data[pos] != 0:
        pos += 1
    pos += 1  # skip null
    bones.append((parent, mat))

# ── Parse MDLE matrices ────────────────────────────────────────
mdle_mats = []
for i in range(bone_count):
    mat = list(struct.unpack_from('<16f', data, mdle_data_start + i*64))
    mdle_mats.append(mat)

# ── Compute Gilder inverse_bind ─────────────────────────────────
def mat_mul(a, b):
    r = [0.0]*16
    for i in range(4):
        for j in range(4):
            s = 0.0
            for k in range(4):
                s += a[i + k*4] * b[k + j*4]
            r[i + j*4] = s
    return r

def mat_inv(m):
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

bind_world = [None]*bone_count
for bi in range(bone_count):
    parent, local = bones[bi]
    if parent < 0:
        bind_world[bi] = list(local)
    elif bind_world[parent] is not None:
        bind_world[bi] = mat_mul(bind_world[parent], local)

gilder_ib = [mat_inv(bw) for bw in bind_world]

# ── Parse mesh vertices (before MDLS section) ──────────────────
# Same logic as Rust scene_puppet_mesh():
# Scan from offset 9 to mdls_offset for: [8-byte header][u32 vertex_bytes][vertex data][u32 index_bytes][index data]
# where vertex_bytes is multiple of 80, and the whole block fits before mdls_offset
MARKER_SIZE = 9
MESH_HEADER_SIZE = 8
VERTEX_STRIDE = 80
TRIANGLE_INDEX_BYTES = 6  # 3 * u16

mesh_start = None
vertex_count = None
indices_offset = None
index_count = None

for offset in range(MARKER_SIZE, mdls_off - MESH_HEADER_SIZE - 4):
    vertex_bytes = struct.unpack_from('<I', data, offset + 4)[0]
    verts_offset = offset + MESH_HEADER_SIZE
    idx_len_off = verts_offset + vertex_bytes
    if vertex_bytes == 0 or vertex_bytes % VERTEX_STRIDE != 0:
        continue
    if idx_len_off + 4 > mdls_off:
        continue
    index_bytes = struct.unpack_from('<I', data, idx_len_off)[0]
    indices_off = idx_len_off + 4
    if index_bytes == 0 or index_bytes % TRIANGLE_INDEX_BYTES != 0:
        continue
    if indices_off + index_bytes > mdls_off:
        continue
    mesh_start = verts_offset
    vertex_count = vertex_bytes // VERTEX_STRIDE
    indices_offset = indices_off
    index_count = index_bytes // 2  # u16 indices
    break

if mesh_start is None:
    print("Mesh not found!")
    sys.exit(1)

print(f"Mesh: {vertex_count} vertices at 0x{mesh_start:08X}")
print(f"Indices: {index_count} at 0x{indices_offset:08X}")

# Parse vertices: 80-byte stride
# offsets per Rust code: POSITION=0, BONE_INDEX=40, BONE_WEIGHT=56, UV=72
# Bone indices: u32 (not u16) per Rust code
vertices = []
for vi in range(vertex_count):
    off = mesh_start + vi * VERTEX_STRIDE
    x, y, z = struct.unpack_from('<3f', data, off)
    # UV at offset 72
    u, raw_v = struct.unpack_from('<2f', data, off + 72)
    v = 1.0 - raw_v  # Gilder convention
    # Bone indices at offset 40: 4 * u32
    b0 = struct.unpack_from('<I', data, off + 40)[0]
    b1 = struct.unpack_from('<I', data, off + 44)[0]
    b2 = struct.unpack_from('<I', data, off + 48)[0]
    b3 = struct.unpack_from('<I', data, off + 52)[0]
    # Weights at offset 56: 4 * f32
    w0, w1, w2, w3 = struct.unpack_from('<4f', data, off + 56)
    vertices.append({
        'x': x, 'y': y, 'z': z,
        'u': u, 'v': v,
        'bones': [b0, b1, b2, b3],
        'weights': [w0, w1, w2, w3],
        'opacity': 1.0
    })

print(f"Parsed {len(vertices)} vertices")
print(f"UV range: u=[{min(v['u'] for v in vertices):.4f}..{max(v['u'] for v in vertices):.4f}], "
      f"v=[{min(v['v'] for v in vertices):.4f}..{max(v['v'] for v in vertices):.4f}]")
print(f"Position range: x=[{min(v['x'] for v in vertices):.4f}..{max(v['x'] for v in vertices):.4f}], "
      f"y=[{min(v['y'] for v in vertices):.4f}..{max(v['y'] for v in vertices):.4f}]")

# ── Parse MDLA animation ───────────────────────────────────────
# Per Rust code: clip header then per-bone per-sample data
# Clip header: id(u32) + flags(u32) + name(C-string) + playback(C-string) 
#   + fps(f32) + frame_count(u32) + reserved(u32) + bone_count(u32)
# Per bone: flags(u32) + byte_count(u32) + samples * 36 bytes
# Each sample (9 f32 = 36 bytes): tx,ty,tz, rx,ry,rz, sx,sy,sz
pos = mdla_data_start
clips = []
for ci in range(clip_count):
    clip_id = struct.unpack_from('<I', data, pos)[0]; pos += 4
    clip_flags = struct.unpack_from('<I', data, pos)[0]; pos += 4
    # C-string name
    name_start = pos
    while pos < len(data) and data[pos] != 0:
        pos += 1
    name = data[name_start:pos].decode('ascii', errors='replace')
    pos += 1
    # C-string playback
    pb_start = pos
    while pos < len(data) and data[pos] != 0:
        pos += 1
    playback = data[pb_start:pos].decode('ascii', errors='replace')
    pos += 1
    fps = struct.unpack_from('<f', data, pos)[0]; pos += 4
    frame_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
    reserved = struct.unpack_from('<I', data, pos)[0]; pos += 4
    clip_bone_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
    sample_count = frame_count + 1  # MDLA stores N+1 samples for N frames
    
    bone_frames = []
    for bi in range(clip_bone_count):
        bone_flags = struct.unpack_from('<I', data, pos)[0]; pos += 4
        byte_count = struct.unpack_from('<I', data, pos)[0]; pos += 4
        assert byte_count % 36 == 0, f"Bone {bi} byte_count={byte_count}"
        assert byte_count // 36 == sample_count, f"Bone {bi} sample_count mismatch"
        frames = []
        for s in range(sample_count):
            tx, ty, tz = struct.unpack_from('<3f', data, pos); pos += 12
            rx, ry, rz = struct.unpack_from('<3f', data, pos); pos += 12
            sx, sy, sz = struct.unpack_from('<3f', data, pos); pos += 12
            frames.append({'t': (tx, ty, tz), 'r': (rx, ry, rz), 's': (sx, sy, sz)})
        bone_frames.append(frames)
    clips.append({
        'id': clip_id, 'name': name, 'fps': fps, 'frame_count': frame_count,
        'bone_count': clip_bone_count, 'sample_count': sample_count,
        'bone_frames': bone_frames
    })
    print(f"Clip {ci}: id={clip_id} name={name} fps={fps:.2f} frames={frame_count} bones={clip_bone_count} samples={sample_count}")

if not clips:
    print("No animation clips found!")
    sys.exit(1)

clip = clips[0]

# ── Helper: Euler angles to rotation matrix (matches Gilder matrix()) ──
def euler_to_matrix(rx, ry, rz, tx, ty, tz, sx, sy, sz):
    """Convert Euler XYZ + translation + scale to 4x4 column-major matrix.
    Matches ScenePuppetTransform::matrix() order: translation * rotation * scale
    where rotation = rz * ry * rx."""
    cx = math.cos(rx); srx = math.sin(rx)
    cy = math.cos(ry); sry = math.sin(ry)
    cz = math.cos(rz); srz = math.sin(rz)
    # Rotation X
    rx_mat = [1,0,0,0, 0,cx,-srx,0, 0,srx,cx,0, 0,0,0,1]
    # Rotation Y
    ry_mat = [cy,0,sry,0, 0,1,0,0, -sry,0,cy,0, 0,0,0,1]
    # Rotation Z
    rz_mat = [cz,-srz,0,0, srz,cz,0,0, 0,0,1,0, 0,0,0,1]
    rot = mat_mul(mat_mul(rz_mat, ry_mat), rx_mat)
    # Scale
    sc_mat = [sx,0,0,0, 0,sy,0,0, 0,0,sz,0, 0,0,0,1]
    # Translation
    tr_mat = [1,0,0,0, 0,1,0,0, 0,0,1,0, tx,ty,tz,1]
    return mat_mul(tr_mat, mat_mul(rot, sc_mat))

def frame_to_local_mat(frame):
    return euler_to_matrix(
        frame['r'][0], frame['r'][1], frame['r'][2],
        frame['t'][0], frame['t'][1], frame['t'][2],
        frame['s'][0], frame['s'][1], frame['s'][2]
    )

def bone_local_pose(clip, frame_idx, bone_count):
    """Get local pose (relative to parent) for all bones at given frame."""
    pose = []
    for bi in range(min(bone_count, clip['bone_count'])):
        sample_idx = min(frame_idx, clip['sample_count'] - 1)
        frame_data = clip['bone_frames'][bi][sample_idx]
        pose.append(frame_to_local_mat(frame_data))
    return pose

def compute_pose_world(local_pose, bones):
    """Compute world-space pose matrices from local pose."""
    pw = [None]*len(local_pose)
    for bi in range(len(local_pose)):
        parent, _ = bones[bi]
        if parent < 0:
            pw[bi] = list(local_pose[bi])
        elif pw[parent] is not None:
            pw[bi] = mat_mul(pw[parent], local_pose[bi])
    return pw

def transform_point(mat, x, y, z):
    """Apply 4x4 affine matrix to 3D point."""
    return (
        mat[0]*x + mat[4]*y + mat[8]*z + mat[12],
        mat[1]*x + mat[5]*y + mat[9]*z + mat[13],
        mat[2]*x + mat[6]*y + mat[10]*z + mat[14],
    )

def skin_vertex(v, pose_world, ib, bones_list):
    """Skin a single vertex using pose_world * inverse_bind."""
    wx, wy, wz = 0.0, 0.0, 0.0
    total_w = 0.0
    for slot in range(4):
        w = v['weights'][slot]
        if w <= 0.001:
            continue
        bi = v['bones'][slot]
        if bi >= len(pose_world) or pose_world[bi] is None:
            continue
        skin_mat = mat_mul(pose_world[bi], ib[bi])
        px, py, pz = transform_point(skin_mat, v['x'], v['y'], v['z'])
        wx += px * w
        wy += py * w
        wz += pz * w
        total_w += w
    if total_w < 0.001:
        return v['x'], v['y'], v['z']
    return wx/total_w, wy/total_w, wz/total_w

# ── 5. Skinning comparison: open eye (frame 0) and closed eye (frame ~300) ──
# Closed eye: frame ~300 at 30fps, played at 0.8 rate. 300/0.8 ~= 375 
# But the clip frame_count is 600, so let's try frame 300 directly
FRAME_OPEN = 0
FRAME_CLOSED = 300  # from docs: "the current pose_world * inverse_bind_world skinning changes the eye around frame 300"

for frame_idx in [FRAME_OPEN, FRAME_CLOSED]:
    local_pose = bone_local_pose(clip, frame_idx, bone_count)
    pose_world = compute_pose_world(local_pose, bones)
    
    gilder_skinned = []
    mdle_skinned = []
    
    for vi, v in enumerate(vertices):
        gx, gy, gz = skin_vertex(v, pose_world, gilder_ib, bones)
        mx, my, mz = skin_vertex(v, pose_world, mdle_mats, bones)
        gilder_skinned.append((gx, gy, gz))
        mdle_skinned.append((mx, my, mz))
    
    # Compute differences
    max_dx = max(abs(gilder_skinned[i][0] - mdle_skinned[i][0]) for i in range(len(vertices)))
    max_dy = max(abs(gilder_skinned[i][1] - mdle_skinned[i][1]) for i in range(len(vertices)))
    rms_dx = math.sqrt(sum((gilder_skinned[i][0] - mdle_skinned[i][0])**2 for i in range(len(vertices))) / len(vertices))
    rms_dy = math.sqrt(sum((gilder_skinned[i][1] - mdle_skinned[i][1])**2 for i in range(len(vertices))) / len(vertices))
    
    print(f"\n--- Frame {frame_idx} ({'open' if frame_idx == 0 else 'closed'} eye) ---")
    print(f"  Max position diff: dx={max_dx:.4f} dy={max_dy:.4f}")
    print(f"  RMS position diff: dx={rms_dx:.4f} dy={rms_dy:.4f}")
    
    # Find pupil-region vertices (UV ~0.3-0.7, v~0.3-0.7)
    pupil_verts = [vi for vi, v in enumerate(vertices) if 0.25 <= v['u'] <= 0.75 and 0.25 <= v['v'] <= 0.75]
    if pupil_verts:
        pupil_diffs = [(abs(gilder_skinned[vi][0] - mdle_skinned[vi][0]),
                        abs(gilder_skinned[vi][1] - mdle_skinned[vi][1]),
                        vi) for vi in pupil_verts]
        pupil_diffs.sort(key=lambda x: -(x[0]+x[1]))
        print(f"  Pupil region vertices ({len(pupil_verts)} total):")
        print(f"    Max pupil dx: {max(d[0] for d in pupil_diffs):.4f}")
        print(f"    Max pupil dy: {max(d[1] for d in pupil_diffs):.4f}")
        print(f"    Avg pupil diff: {sum(d[0]+d[1] for d in pupil_diffs)/(2*len(pupil_diffs)):.4f}")
        print(f"    Top 5 largest pupil diffs:")
        for dx, dy, vi in pupil_diffs[:5]:
            v = vertices[vi]
            gx, gy, _ = gilder_skinned[vi]
            mx, my, _ = mdle_skinned[vi]
            print(f"      vtx[{vi}] uv=({v['u']:.3f},{v['v']:.3f}) gilder=({gx:.2f},{gy:.2f}) mdle=({mx:.2f},{my:.2f}) d=({dx:.2f},{dy:.2f})")
    
    # Save full skinned vertices for this frame
    out = {
        'frame': frame_idx,
        'gilder': [[gx, gy, gz] for gx, gy, gz in gilder_skinned],
        'mdle': [[mx, my, mz] for mx, my, mz in mdle_skinned],
        'uvs': [[v['u'], v['v']] for v in vertices]
    }
    with open(f'/tmp/eye_skinning_frame_{frame_idx}.json', 'w') as f:
        json.dump(out, f)

print("\nSaved skinning results to /tmp/eye_skinning_frame_*.json")
