//! WE shader texture-set keys for descriptor-heap draw binding.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/servers/rendering/rendering_device.h`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphDraw, SceneGraphResourceRole, SceneResourceId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureSetKey {
    pub bindings: Vec<NativeVulkanSceneTextureSetBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureSetBinding {
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub resource: SceneResourceId,
}

impl NativeVulkanSceneTextureSetKey {
    pub(in crate::renderer::native_vulkan) fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub(in crate::renderer::native_vulkan) fn texture_count(&self) -> usize {
        self.bindings.len()
    }

    pub(in crate::renderer::native_vulkan) fn slot_mask(&self) -> u32 {
        self.bindings
            .iter()
            .fold(0u32, |mask, binding| mask | (1u32 << binding.slot))
    }

    pub(in crate::renderer::native_vulkan) fn shader_mappings(&self) -> Vec<String> {
        self.bindings
            .iter()
            .map(|binding| scene_shader_texture_mapping(binding.slot))
            .collect()
    }
}

pub(in crate::renderer::native_vulkan) fn scene_mesh_draw_texture_set_key(
    draw: &SceneGraphDraw,
) -> Result<NativeVulkanSceneTextureSetKey, String> {
    let _ = draw.shader_texture_slot_mask()?;
    let mut bindings = draw
        .resources
        .iter()
        .map(|binding| NativeVulkanSceneTextureSetBinding {
            slot: binding.slot,
            role: binding.role,
            resource: binding.resource,
        })
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| binding.slot);
    Ok(NativeVulkanSceneTextureSetKey { bindings })
}

pub(in crate::renderer::native_vulkan) fn scene_shader_texture_mapping(slot: u32) -> String {
    format!("set0.binding{slot}.g_Texture{slot}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass, SceneGraphResourceBinding,
        SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn texture_set_key_sorts_we_texture_slots_and_builds_mask() {
        let draw = mesh_draw(vec![
            SceneGraphResourceBinding {
                slot: 4,
                role: SceneGraphResourceRole::shader_texture(4),
                resource: SceneResourceId(40),
            },
            SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(10),
            },
        ]);

        let key = scene_mesh_draw_texture_set_key(&draw).expect("texture set key");

        assert_eq!(key.bindings[0].slot, 0);
        assert_eq!(key.bindings[1].slot, 4);
        assert_eq!(key.slot_mask(), 0b1_0001);
        assert_eq!(
            key.shader_mappings(),
            vec![
                "set0.binding0.g_Texture0".to_owned(),
                "set0.binding4.g_Texture4".to_owned()
            ]
        );
    }

    #[test]
    fn texture_set_key_rejects_slot_role_mismatch() {
        let draw = mesh_draw(vec![SceneGraphResourceBinding {
            slot: 1,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(10),
        }]);

        let err = scene_mesh_draw_texture_set_key(&draw).expect_err("mismatch must fail");

        assert!(err.contains("does not match WE g_Texture0"));
    }

    fn mesh_draw(resources: Vec<SceneGraphResourceBinding>) -> SceneGraphDraw {
        SceneGraphDraw {
            object: SceneObjectId(4),
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(SceneGeometryId(8)),
            puppet: None,
            resources,
            index_count: 6,
        }
    }
}
