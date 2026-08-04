# Tensor Wallpaper scripts

Run Python tools as `uv run python scripts/tensor-wallpaper/<tool>.py` from the TensorDE
workspace root.

## Required gates and runtime evidence

- `scene_engine_constraints.py` enforces module, line, descriptor-heap, and
  scene-engine structure contracts; `test_scene_engine_constraints.py` tests
  the gate itself.
- `scene_engine_cli_smoke.py` and `scene_engine_runtime_smoke.py` exercise fresh
  conversion and typed-plan/runtime behavior.
- `performance_snapshot.py` samples bounded CPU and memory evidence.
- `video_decode_matrix.py` runs the GPU video decode and presentation matrix.
- `analyze_particle_frame_sequence.py` analyzes particle frame evidence.
- `wallpaper_engine_workshop_download.py` downloads Workshop inputs into
  `artifacts/tensor-wallpaper/` and can launch the video matrix.

## Disassembly and semantic recovery

- Extraction: `extract_constants.py`, `extract_mask.py`,
  `extract_properties.py`, and `extract_spv.py`.
- D3D11 and render call discovery: `find_d3d11_calls.py`,
  `find_d3d11_calls2.py`, `find_d3d11_calls_32.py`,
  `find_d3d11_context_calls.py`, `find_d3d11_map.py`, `find_draw_calls.py`,
  `find_om_blend.py`, `find_ps_srv_calls.py`, and `find_render_vtable.py`.
- Blend/pass/object discovery: `find_blend_callers.py`,
  `find_blend_dispatch.py`, `find_blend_states.py`,
  `find_constant_buffer_calls.py`, `find_context.py`, `find_obj_iter.py`,
  `find_pass_types.py`, and `find_passtags.py`.
- Vtable/xref scans: `find_vtables.py`, `find_vtables2.py`, `scan_xref.py`,
  and `scan_xref2.py`.
- Reconstructed-format analysis: `mdle_compare.py` and
  `we_mdl_clipping_records.py`.

`workspace_paths.py` is the shared path module for these tools and is not a
standalone command. Outputs belong under `artifacts/tensor-wallpaper/` or
`reverse-engineered/tensor-wallpaper/`, never beside the scripts.
