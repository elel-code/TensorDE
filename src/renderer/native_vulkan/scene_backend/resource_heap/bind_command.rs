//! Scene draw descriptor heap slice bind command recording.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{SceneGraphDraw, SceneGraphTarget, SceneObjectId};

use super::texture_set::{
    NativeVulkanSceneTextureSetKey, scene_mesh_draw_texture_set_key_with_pass_input,
};

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneResourceHeapDrawBindInfo {
    pub draw_index: usize,
    pub object: SceneObjectId,
    pub heap_slice_index: usize,
    pub texture_set: NativeVulkanSceneTextureSetKey,
    pub base_resource_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
    pub resource_bind: vk::BindHeapInfoEXT,
    pub sampler_bind: Option<vk::BindHeapInfoEXT>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneResourceHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) draw_index: usize,
    pub(in crate::renderer::native_vulkan) heap_slice_index: usize,
    pub(in crate::renderer::native_vulkan) texture_set: NativeVulkanSceneTextureSetKey,
    pub(in crate::renderer::native_vulkan) base_resource_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) resource_descriptor_count: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: Vec<&'static str>,
}

impl NativeVulkanSceneResourceHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_draw_and_bind_info(
        draw_index: usize,
        pass_input: Option<SceneGraphTarget>,
        draw: &SceneGraphDraw,
        bind_info: &NativeVulkanSceneResourceHeapDrawBindInfo,
    ) -> Result<Self, String> {
        let texture_set = scene_mesh_draw_texture_set_key_with_pass_input(draw, pass_input)?;
        if texture_set != bind_info.texture_set {
            return Err(format!(
                "scene draw resource heap bind texture-set mismatch for object {:?}: draw {:?}, heap {:?}",
                draw.object, texture_set, bind_info.texture_set
            ));
        }
        if draw_index != bind_info.draw_index {
            return Err(format!(
                "scene draw resource heap bind draw-index mismatch for object {:?}: draw {}, heap {}",
                draw.object, draw_index, bind_info.draw_index
            ));
        }
        if draw.object != bind_info.object {
            return Err(format!(
                "scene draw resource heap bind object mismatch: draw {:?}, heap {:?}",
                draw.object, bind_info.object
            ));
        }
        if texture_set.texture_count() != bind_info.texture_count {
            return Err(format!(
                "scene draw resource heap bind texture count mismatch for object {:?}: draw {}, heap {}",
                draw.object,
                texture_set.texture_count(),
                bind_info.texture_count
            ));
        }
        if bind_info.texture_count > 0 && bind_info.sampler_bind.is_none() {
            return Err(format!(
                "scene draw resource heap bind for object {:?} requires sampler heap for {} textures",
                draw.object, bind_info.texture_count
            ));
        }
        let mut command_order = vec!["cmd_bind_resource_heap_ext"];
        if bind_info.sampler_bind.is_some() {
            command_order.push("cmd_bind_sampler_heap_ext");
        }
        Ok(Self {
            object: draw.object,
            draw_index,
            heap_slice_index: bind_info.heap_slice_index,
            shader_mappings: bind_info.shader_mappings.clone(),
            texture_set,
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            resource_descriptor_count: bind_info.resource_descriptor_count,
            texture_count: bind_info.texture_count,
            command_order,
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_resource_heap_draw_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    draw_index: usize,
    pass_input: Option<SceneGraphTarget>,
    draw: &SceneGraphDraw,
    bind_info: NativeVulkanSceneResourceHeapDrawBindInfo,
) -> Result<NativeVulkanSceneResourceHeapDrawBindPlan, String> {
    let plan = NativeVulkanSceneResourceHeapDrawBindPlan::from_draw_and_bind_info(
        draw_index, pass_input, draw, &bind_info,
    )?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        if let Some(sampler_bind) = bind_info.sampler_bind {
            device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass, SceneGraphResourceBinding,
        SceneGraphResourceRole, SceneMaterialKey, SceneResourceId,
    };

    #[test]
    fn resource_heap_draw_bind_plan_tracks_heap_slice_identity() {
        let draw = mesh_draw(SceneResourceId(7));
        let texture_set =
            scene_mesh_draw_texture_set_key_with_pass_input(&draw, None).expect("texture set");
        let bind_info = draw_bind_info(3, texture_set, 11, 2);

        let plan = NativeVulkanSceneResourceHeapDrawBindPlan::from_draw_and_bind_info(
            3, None, &draw, &bind_info,
        )
        .expect("draw resource heap bind plan");

        assert_eq!(plan.object, SceneObjectId(4));
        assert_eq!(plan.draw_index, 3);
        assert_eq!(plan.heap_slice_index, 11);
        assert_eq!(plan.base_resource_descriptor_index, 2);
        assert_eq!(plan.resource_descriptor_count, 2);
        assert_eq!(plan.texture_count, 1);
        assert_eq!(
            plan.command_order,
            vec!["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn resource_heap_draw_bind_plan_rejects_draw_index_mismatch() {
        let draw = mesh_draw(SceneResourceId(7));
        let texture_set =
            scene_mesh_draw_texture_set_key_with_pass_input(&draw, None).expect("texture set");
        let bind_info = draw_bind_info(2, texture_set, 11, 2);

        let err = NativeVulkanSceneResourceHeapDrawBindPlan::from_draw_and_bind_info(
            3, None, &draw, &bind_info,
        )
        .expect_err("draw index mismatch must fail");

        assert!(err.contains("draw-index mismatch"));
    }

    fn draw_bind_info(
        draw_index: usize,
        texture_set: NativeVulkanSceneTextureSetKey,
        heap_slice_index: usize,
        base_resource_descriptor_index: usize,
    ) -> NativeVulkanSceneResourceHeapDrawBindInfo {
        let texture_count = texture_set.texture_count();
        let shader_mappings = texture_set.shader_mappings();
        NativeVulkanSceneResourceHeapDrawBindInfo {
            draw_index,
            object: SceneObjectId(4),
            heap_slice_index,
            texture_set,
            base_resource_descriptor_index,
            resource_descriptor_count: texture_count + 1,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::default(),
            sampler_bind: Some(vk::BindHeapInfoEXT::default()),
        }
    }

    fn mesh_draw(resource: SceneResourceId) -> SceneGraphDraw {
        SceneGraphDraw {
            object: SceneObjectId(4),
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(8)),
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource,
            }],
            index_count: 6,
        }
    }
}
