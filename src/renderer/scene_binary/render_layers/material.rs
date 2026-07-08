//! Material lowering module boundary for legacy `.gscn` layer assembly.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

mod effect_pass;
mod effect_runtime;
mod render_state;
mod texture_slots;

pub(in crate::renderer::scene_binary) use texture_slots::binary_scene_material_texture_slots_cached;

pub(super) use effect_pass::binary_scene_image_effect_passes_cached;
pub(super) use texture_slots::{binary_scene_alpha_texture_mode, binary_scene_alpha_texture_slot};
