//! Binary puppet payload decoding boundary for mesh resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

mod animation;
mod attachment;
mod clipping;
mod skin;

pub(super) use animation::binary_scene_puppet_clips_cached;

pub(in crate::renderer::scene_binary) use animation::{
    binary_scene_puppet_clips, binary_scene_puppet_layers,
};
pub(in crate::renderer::scene_binary) use attachment::binary_scene_puppet_attachment_deltas;
pub(in crate::renderer::scene_binary) use clipping::{
    binary_scene_puppet_active_sources, binary_scene_puppet_clipping_records,
};
pub(in crate::renderer::scene_binary) use skin::binary_scene_puppet_skin;
