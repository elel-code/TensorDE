//! Static event-binding records stored in the scene binary.

use serde::{Deserialize, Serialize};

use super::SceneObjectHandle;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneCameraParallaxRecord {
    pub enabled: bool,
    pub amount: f32,
    pub delay: f32,
    pub mouse_influence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectParallaxDepthRecord {
    pub object: SceneObjectHandle,
    pub depth: [f32; 2],
}
