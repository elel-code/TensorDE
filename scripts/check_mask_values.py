"""Check opacity and iris mask values and their relationship to eyelid UV."""
import struct

# Opacity mask (331x115, R8)
gtex = open('/tmp/gilder-we-3742497499-output-restored-placement/assets/scene-resources/scene/resource-206-opacity-mask-d2f87f99-frame-0.gtex','rb').read()
w = struct.unpack('<I', gtex[8:12])[0]
h = struct.unpack('<I', gtex[12:16])[0]
fmt = struct.unpack('<I', gtex[16:20])[0]
print(f"Opacity mask: {w}x{h} format={fmt} (9=R8)")

r8 = gtex[32:32+w*h]
zero = sum(1 for v in r8 if v == 0)
full = sum(1 for v in r8 if v == 255)
mid = len(r8) - zero - full
print(f"Mask: 0={zero} ({zero*100/len(r8):.0f}%) 255={full} ({full*100/len(r8):.0f}%) other={mid} ({mid*100/len(r8):.0f}%)")

# Eyelid area in mask UV space: top ~35% of mask
eyelid_rows = int(h * 0.35)
eyelid_mask = r8[:eyelid_rows * w]
ezero = sum(1 for v in eyelid_mask if v == 0)
efull = sum(1 for v in eyelid_mask if v == 255)
emid = len(eyelid_mask) - ezero - efull
print(f"Eyelid area (top {eyelid_rows} rows): 0={ezero} ({ezero*100/len(eyelid_mask):.0f}%) 255={efull} ({efull*100/len(eyelid_mask):.0f}%) other={emid} ({emid*100/len(eyelid_mask):.0f}%)")
print(f"Range: [{min(eyelid_mask)}..{max(eyelid_mask)}]")

# Pupil area: middle
pupil_start = int(h * 0.35)
pupil_end = int(h * 0.65)
pupil_mask = []
for y in range(pupil_start, pupil_end):
    pupil_mask.extend(r8[y*w:(y+1)*w])
pzero = sum(1 for v in pupil_mask if v == 0)
pfull = sum(1 for v in pupil_mask if v == 255)
pmid = len(pupil_mask) - pzero - pfull
print(f"\nPupil area (rows {pupil_start}-{pupil_end}): 0={pzero} ({pzero*100/len(pupil_mask):.0f}%) 255={pfull} ({pfull*100/len(pupil_mask):.0f}%) other={pmid} ({pmid*100/len(pupil_mask):.0f}%)")
print(f"Range: [{min(pupil_mask)}..{max(pupil_mask)}]")
