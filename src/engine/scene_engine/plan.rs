//! Engine-owned scene plan built from WE scene facts.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::path::PathBuf;

use serde::Serialize;

use super::{SceneFrameContext, SceneObject, SceneResource};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEnginePlan {
    pub source: Option<PathBuf>,
    pub snapshot_time_ms: u64,
    pub target_width: u32,
    pub target_height: u32,
    pub resources: Vec<SceneResource>,
    pub objects: Vec<SceneObject>,
    pub timeline_channel_count: usize,
    pub timeline_owner_count: usize,
    pub puppet_animation_layer_count: usize,
    pub particle_emitter_count: usize,
    pub material_pass_count: usize,
    pub effect_pass_count: usize,
}

impl SceneEnginePlan {
    pub fn frame_context(&self) -> SceneFrameContext {
        SceneFrameContext {
            time_ms: self.snapshot_time_ms,
            target_width: self.target_width.max(1),
            target_height: self.target_height.max(1),
        }
    }
}
