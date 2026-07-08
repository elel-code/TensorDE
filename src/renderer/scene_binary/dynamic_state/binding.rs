//! Runtime property binding target lowering for binary scene dynamic state.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use super::super::schema::{
    BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS, BINARY_TRANSFORM_PROPERTY_HEIGHT,
    BINARY_TRANSFORM_PROPERTY_OPACITY, BINARY_TRANSFORM_PROPERTY_ROTATION_DEG,
    BINARY_TRANSFORM_PROPERTY_SCALE_X, BINARY_TRANSFORM_PROPERTY_SCALE_Y,
    BINARY_TRANSFORM_PROPERTY_WIDTH, BINARY_TRANSFORM_PROPERTY_X, BINARY_TRANSFORM_PROPERTY_Y,
};
use super::metadata::BinarySceneRuntimeMetadataPropertyBinding;

#[derive(Debug, Clone)]
pub(in crate::renderer::scene_binary) struct BinarySceneDynamicPropertyBinding {
    pub(in crate::renderer::scene_binary) property: String,
    pub(in crate::renderer::scene_binary) target_node: Option<String>,
    pub(in crate::renderer::scene_binary) target: u16,
    pub(in crate::renderer::scene_binary) scale: f64,
    pub(in crate::renderer::scene_binary) offset: f64,
}

impl BinarySceneDynamicPropertyBinding {
    pub(super) fn from_metadata(
        binding: BinarySceneRuntimeMetadataPropertyBinding,
    ) -> Option<Self> {
        Some(Self {
            property: binding.property,
            target_node: binding.target_node,
            target: binary_scene_dynamic_property_target(&binding.target)?,
            scale: binding.scale,
            offset: binding.offset,
        })
    }
}

fn binary_scene_dynamic_property_target(target: &str) -> Option<u16> {
    match target {
        "x" => Some(BINARY_TRANSFORM_PROPERTY_X),
        "y" => Some(BINARY_TRANSFORM_PROPERTY_Y),
        "scale_x" | "scaleX" | "scalex" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_X),
        "scale_y" | "scaleY" | "scaley" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_Y),
        "opacity" | "alpha" => Some(BINARY_TRANSFORM_PROPERTY_OPACITY),
        "rotation" | "rotation_deg" | "angle" => Some(BINARY_TRANSFORM_PROPERTY_ROTATION_DEG),
        "width" => Some(BINARY_TRANSFORM_PROPERTY_WIDTH),
        "height" => Some(BINARY_TRANSFORM_PROPERTY_HEIGHT),
        "corner_radius" | "cornerRadius" => Some(BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS),
        _ => None,
    }
}
