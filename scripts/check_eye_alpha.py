#!/usr/bin/env python3
"""Find pupil location in closed frame by tracing UV."""
import json, subprocess, math

SCENE='/tmp/gilder-we-3742497499-cwe-effect-progress/assets/scene.gscn'
ROOT='/tmp/gilder-we-3742497499-cwe-effect-progress'
result=subprocess.run(['target/release/gilder-native-vulkan','--scene-runtime-snapshot','--source',SCENE,'--scene-root',ROOT,'--scene-time-ms','10000'],capture_output=True,text=True,timeout=60)
data=json.loads(result.stdout)
for s in data.get('draw_pass_sampled_image_recording_steps',[]):
 if s.get('layer_id')=='node-77-models-json' and s.get('we_graph_step_index')==0:
  step=s; break
fi=step['first_vertex']; ii=step['first_index']; ic=step['index_count']
verts=data['draw_pass_sampled_image_vertices']; indices=data['draw_pass_sampled_image_indices'][ii:ii+ic]
def sample_uv(x,y):
 for off in range(0,len(indices),3):
  t0,t1,t2=indices[off:off+3]; v0,v1,v2=verts[t0],verts[t1],verts[t2]
  x0,y0=v0['position'];x1,y1=v1['position'];x2,y2=v2['position']
  denom=(y1-y2)*(x0-x2)+(x2-x1)*(y0-y2)
  if abs(denom)<1e-6: continue
  w0=((y1-y2)*(x-x2)+(x2-x1)*(y-y2))/denom
  w1=((y2-y0)*(x-x2)+(x0-x2)*(y-y2))/denom; w2=1-w0-w1
  if w0<-1e-5 or w1<-1e-5 or w2<-1e-5: continue
  u=w0*v0['uv'][0]+w1*v1['uv'][0]+w2*v2['uv'][0]; v=w0*v0['uv'][1]+w1*v1['uv'][1]+w2*v2['uv'][1]
  return (u,v)
 return None

# Find where pupil UV (0.90,0.50) appears in closed frame
# Check wider range
ref_u,ref_v=0.9036,0.5014
print("Looking for pupil UV (0.90,0.50) in closed frame:")
best_y = None
best_d = 999
for dy in range(-30, 80):
    uv = sample_uv(340, 120+dy)
    if uv:
        d = ((uv[0]-ref_u)**2 + (uv[1]-ref_v)**2)**0.5
        if d < best_d:
            best_d = d
            best_y = 120+dy
        if d < 0.15:
            print(f'  y={120+dy}: UV=({uv[0]:.4f},{uv[1]:.4f}) d={d:.4f} ← CLOSE TO PUPIL')
print(f'\nClosest match: y={best_y} d={best_d:.4f}')

# Also check: in open frame, what UV is at the position where closed frame shows pupil?
# In closed frame, y=best_y shows pupil UV. In open frame, what's there?
print(f'\nIn open frame at y={best_y}, same check:')
result0=subprocess.run(['target/release/gilder-native-vulkan','--scene-runtime-snapshot','--source',SCENE,'--scene-root',ROOT,'--scene-time-ms','0'],capture_output=True,text=True,timeout=60)
data0=json.loads(result0.stdout)
for s in data0.get('draw_pass_sampled_image_recording_steps',[]):
 if s.get('layer_id')=='node-77-models-json' and s.get('we_graph_step_index')==0:
  step0=s; break
fi0=step0['first_vertex']; ii0=step0['first_index']; ic0=step0['index_count']
verts0=data0['draw_pass_sampled_image_vertices']; indices0=data0['draw_pass_sampled_image_indices'][ii0:ii0+ic0]
def sample_uv0(x,y):
 for off in range(0,len(indices0),3):
  t0,t1,t2=indices0[off:off+3]; v0,v1,v2=verts0[t0],verts0[t1],verts0[t2]
  x0,y0=v0['position'];x1,y1=v1['position'];x2,y2=v2['position']
  denom=(y1-y2)*(x0-x2)+(x2-x1)*(y0-y2)
  if abs(denom)<1e-6: continue
  w0=((y1-y2)*(x-x2)+(x2-x1)*(y-y2))/denom
  w1=((y2-y0)*(x-x2)+(x0-x2)*(y-y2))/denom; w2=1-w0-w1
  if w0<-1e-5 or w1<-1e-5 or w2<-1e-5: continue
  u=w0*v0['uv'][0]+w1*v1['uv'][0]+w2*v2['uv'][0]; v=w0*v0['uv'][1]+w1*v1['uv'][1]+w2*v2['uv'][1]
  return (u,v)
 return None
uv_open = sample_uv0(340, best_y) if best_y else None
if uv_open:
    d = ((uv_open[0]-ref_u)**2 + (uv_open[1]-ref_v)**2)**0.5
    print(f'  open frame y={best_y}: UV=({uv_open[0]:.4f},{uv_open[1]:.4f}) d={d:.4f}')
    # This shows where the pupil WAS in the open frame
    # In closed frame, the eyelid covers this position at y=120
    # The pupil moved DOWN to y=best_y
    delta_y = best_y - 120 if best_y else 0
    print(f'  Pupil moved down by {delta_y} pixels in closed frame')
