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

use crate::engine::scene_engine::{
    SceneGraphDraw, SceneGraphResourceRole, SceneObjectId, SceneResourceId,
};

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapDrawBindInfo {
    pub resource: SceneResourceId,
    pub heap_index: usize,
    pub resource_bind: vk::BindHeapInfoEXT,
    pub sampler_bind: vk::BindHeapInfoEXT,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneTextureHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) resource: SceneResourceId,
    pub(in crate::renderer::native_vulkan) heap_index: usize,
    pub(in crate::renderer::native_vulkan) shader_mapping: &'static str,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneTextureHeapDrawBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_draw_and_bind_info(
        draw: &SceneGraphDraw,
        bind_info: NativeVulkanSceneTextureHeapDrawBindInfo,
    ) -> Result<Self, String> {
        let resource = scene_mesh_draw_base_color_resource(draw)?.ok_or_else(|| {
            format!(
                "scene mesh draw {:?} requires BaseColor texture before indexed draw",
                draw.object
            )
        })?;
        if resource != bind_info.resource {
            return Err(format!(
                "scene texture heap bind resource mismatch for object {:?}: draw {:?}, heap {:?}",
                draw.object, resource, bind_info.resource
            ));
        }
        Ok(Self {
            object: draw.object,
            resource,
            heap_index: bind_info.heap_index,
            shader_mapping: "set0.binding0.base_color",
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
    let plan = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(draw, bind_info)?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
    }
    Ok(plan)
}

pub(in crate::renderer::native_vulkan) fn scene_mesh_draw_base_color_resource(
    draw: &SceneGraphDraw,
) -> Result<Option<SceneResourceId>, String> {
    let mut base_color = None;
    for resource in &draw.resources {
        if resource.role != SceneGraphResourceRole::BaseColor {
            return Err(format!(
                "scene texture heap bind only supports BaseColor resources, got {:?} for object {:?}",
                resource.role, draw.object
            ));
        }
        if resource.slot != 0 {
            return Err(format!(
                "scene texture heap bind requires BaseColor at slot 0, got slot {} for object {:?}",
                resource.slot, draw.object
            ));
        }
        if base_color.replace(resource.resource).is_some() {
            return Err(format!(
                "scene texture heap bind got duplicate BaseColor slot 0 for object {:?}",
                draw.object
            ));
        }
    }
    Ok(base_color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass, SceneGraphResourceBinding,
        SceneMaterialKey,
    };

    #[test]
    fn texture_heap_draw_bind_plan_requires_base_color_slot_zero() {
        let draw = mesh_draw(vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::BaseColor,
            resource: SceneResourceId(7),
        }]);

        let plan = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            draw_bind_info(SceneResourceId(7), 2),
        )
        .expect("draw texture heap bind plan");

        assert_eq!(plan.object, SceneObjectId(4));
        assert_eq!(plan.resource, SceneResourceId(7));
        assert_eq!(plan.heap_index, 2);
        assert_eq!(
            plan.command_order,
            ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn texture_heap_draw_bind_plan_rejects_missing_base_color() {
        let draw = mesh_draw(Vec::new());

        let err = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            draw_bind_info(SceneResourceId(7), 2),
        )
        .expect_err("missing BaseColor texture must fail");

        assert!(err.contains("requires BaseColor texture"));
    }

    #[test]
    fn texture_heap_draw_bind_plan_rejects_resource_mismatch() {
        let draw = mesh_draw(vec![SceneGraphResourceBinding {
            slot: 0,
            role: SceneGraphResourceRole::BaseColor,
            resource: SceneResourceId(7),
        }]);

        let err = NativeVulkanSceneTextureHeapDrawBindPlan::from_draw_and_bind_info(
            &draw,
            draw_bind_info(SceneResourceId(9), 2),
        )
        .expect_err("resource mismatch must fail");

        assert!(err.contains("resource mismatch"));
    }

    fn draw_bind_info(
        resource: SceneResourceId,
        heap_index: usize,
    ) -> NativeVulkanSceneTextureHeapDrawBindInfo {
        NativeVulkanSceneTextureHeapDrawBindInfo {
            resource,
            heap_index,
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
