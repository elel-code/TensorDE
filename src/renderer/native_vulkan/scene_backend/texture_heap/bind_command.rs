//! Scene texture descriptor heap bind command recording.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{SceneGraphDraw, SceneObjectId};

use super::texture_set::{NativeVulkanSceneTextureSetKey, scene_mesh_draw_texture_set_key};

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapDrawBindInfo {
    pub texture_set: NativeVulkanSceneTextureSetKey,
    pub base_heap_index: usize,
    pub texture_count: usize,
    pub resource_bind: vk::BindHeapInfoEXT,
    pub sampler_bind: vk::BindHeapInfoEXT,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneTextureHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) texture_set: NativeVulkanSceneTextureSetKey,
    pub(in crate::renderer::native_vulkan) base_heap_index: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneTextureHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_draw_and_bind_info(
        draw: &SceneGraphDraw,
        bind_info: &NativeVulkanSceneTextureHeapDrawBindInfo,
    ) -> Result<Self, String> {
        let texture_set = scene_mesh_draw_texture_set_key(draw)?;
        if texture_set.is_empty() {
            return Err(format!(
                "scene mesh draw {:?} requires a WE texture set before indexed draw",
                draw.object
            ));
        }
        if texture_set != bind_info.texture_set {
            return Err(format!(
                "scene texture heap bind texture-set mismatch for object {:?}: draw {:?}, heap {:?}",
                draw.object, texture_set, bind_info.texture_set
            ));
        }
        if texture_set.texture_count() != bind_info.texture_count {
            return Err(format!(
                "scene texture heap bind count mismatch for object {:?}: draw {}, heap {}",
                draw.object,
                texture_set.texture_count(),
                bind_info.texture_count
            ));
        }
        Ok(Self {
            object: draw.object,
            shader_mappings: texture_set.shader_mappings(),
            texture_set,
            base_heap_index: bind_info.base_heap_index,
            texture_count: bind_info.texture_count,
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_texture_heap_draw_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    draw: &SceneGraphDraw,
    bind_info: NativeVulkanSceneTextureHeapDrawBindInfo,
) -> Result<NativeVulkanSceneTextureHeapDrawBindPlan, String> {
    let plan = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(draw, &bind_info)?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass, SceneGraphResourceBinding,
        SceneGraphResourceRole, SceneMaterialKey, SceneObjectId, SceneResourceId,
    };

    #[test]
    fn texture_heap_draw_bind_plan_requires_we_texture_set() {
        let draw = mesh_draw(vec![
            SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(7),
            },
            SceneGraphResourceBinding {
                slot: 4,
                role: SceneGraphResourceRole::shader_texture(4),
                resource: SceneResourceId(8),
            },
        ]);
        let texture_set = scene_mesh_draw_texture_set_key(&draw).expect("texture set");

        let plan = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            &draw_bind_info(texture_set, 3),
        )
        .expect("draw texture heap bind plan");

        assert_eq!(plan.object, SceneObjectId(4));
        assert_eq!(plan.base_heap_index, 3);
        assert_eq!(plan.texture_count, 2);
        assert_eq!(
            plan.shader_mappings,
            vec![
                "set0.binding0.g_Texture0".to_owned(),
                "set0.binding4.g_Texture4".to_owned()
            ]
        );
        assert_eq!(
            plan.command_order,
            ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn texture_heap_draw_bind_plan_rejects_missing_texture_set() {
        let draw = mesh_draw(Vec::new());
        let texture_set = NativeVulkanSceneTextureSetKey {
            bindings: Vec::new(),
        };

        let err = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            &draw_bind_info(texture_set, 2),
        )
        .expect_err("missing texture set must fail");

        assert!(err.contains("requires a WE texture set"));
    }

    #[test]
    fn texture_heap_draw_bind_plan_rejects_texture_set_mismatch() {
        let draw = mesh_draw(vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(7),
        }]);
        let different = mesh_draw(vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::shader_texture(0),
            resource: SceneResourceId(9),
        }]);
        let texture_set = scene_mesh_draw_texture_set_key(&different).expect("texture set");

        let err = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            &draw_bind_info(texture_set, 2),
        )
        .expect_err("texture set mismatch must fail");

        assert!(err.contains("texture-set mismatch"));
    }

    fn draw_bind_info(
        texture_set: NativeVulkanSceneTextureSetKey,
        base_heap_index: usize,
    ) -> NativeVulkanSceneTextureHeapDrawBindInfo {
        let texture_count = texture_set.texture_count();
        NativeVulkanSceneTextureHeapDrawBindInfo {
            texture_set,
            base_heap_index,
            texture_count,
            resource_bind: vk::BindHeapInfoEXT::default(),
            sampler_bind: vk::BindHeapInfoEXT::default(),
        }
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
