//! Typed push-data payloads for native `SPV_EXT_descriptor_heap` scene shaders.

use super::{SceneGpuDrawCommand, ScenePipelineDescriptorLayout};

mod shared;

pub(super) use shared::resolve_scene_shared_descriptor_pushes;

#[derive(Clone, Copy)]
enum BuiltinSceneShaderStage {
    Fragment,
    Vertex,
}

impl BuiltinSceneShaderStage {
    fn label(self) -> &'static str {
        match self {
            Self::Fragment => "fragment",
            Self::Vertex => "vertex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SceneNativeDescriptorPush {
    EngineBuiltIn(Vec<u8>),
    SceneOwned(Vec<u8>),
}

impl SceneNativeDescriptorPush {
    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::EngineBuiltIn(bytes) | Self::SceneOwned(bytes) => bytes,
        }
    }

    fn byte_len(&self) -> u64 {
        self.bytes().len() as u64
    }
}

fn validate_native_push_size(
    key: &str,
    push: &SceneNativeDescriptorPush,
    max_push_data_size: u64,
) -> Result<(), String> {
    if push.byte_len() > max_push_data_size {
        return Err(format!(
            "scene shader {key:?} requires {} descriptor push bytes, exceeding device limit {max_push_data_size}",
            push.byte_len()
        ));
    }
    Ok(())
}

fn sampled_descriptor(
    layout: &ScenePipelineDescriptorLayout,
    draw: &SceneGpuDrawCommand,
    register: u32,
) -> Result<usize, String> {
    Ok(draw.sampled_resource_descriptor_base + sampled_slot_index(layout, register)?)
}

fn sampled_slot_index(layout: &ScenePipelineDescriptorLayout, register: u32) -> Result<usize, String> {
    layout
        .sampled_slots
        .iter()
        .position(|slot| *slot == register)
        .ok_or_else(|| format!("scene shader sampled-image register {register} is not in the retained layout"))
}

#[cfg(test)]
mod tests;
