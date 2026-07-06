//! Scene graphics pipeline key and bind command boundary.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/blending-modes.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneBlendContract, SceneGraphDraw, SceneGraphPipelineClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct NativeVulkanScenePipelineKey<'a> {
    pub shader: &'a str,
    pub blend: SceneBlendContract,
    pub writes_depth: bool,
    pub tests_depth: bool,
    pub pipeline_class: SceneGraphPipelineClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePipelineBindPlan<'a> {
    pub key: NativeVulkanScenePipelineKey<'a>,
    pub command_order: [&'static str; 1],
}

impl<'a> NativeVulkanScenePipelineKey<'a> {
    pub fn from_draw(draw: &'a SceneGraphDraw) -> Result<Self, String> {
        if draw.material.shader.is_empty() {
            return Err("scene pipeline key requires a non-empty WE shader name".to_owned());
        }
        Ok(Self {
            shader: draw.material.shader.as_str(),
            blend: draw.material.blend,
            writes_depth: draw.material.writes_depth,
            tests_depth: draw.material.tests_depth,
            pipeline_class: draw.pipeline,
        })
    }
}

impl<'a> NativeVulkanScenePipelineBindPlan<'a> {
    pub fn from_key(key: NativeVulkanScenePipelineKey<'a>) -> Self {
        Self {
            key,
            command_order: ["cmd_bind_pipeline"],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_pipeline_bind_command<'a>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    key: NativeVulkanScenePipelineKey<'a>,
    pipeline: vk::Pipeline,
) -> Result<NativeVulkanScenePipelineBindPlan<'a>, String> {
    if pipeline == vk::Pipeline::null() {
        return Err("scene pipeline bind requires a valid vk::Pipeline".to_owned());
    }
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
    }
    Ok(NativeVulkanScenePipelineBindPlan::from_key(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphResourceBinding, SceneMaterialKey,
        SceneObjectId,
    };

    #[test]
    fn pipeline_key_borrows_draw_material_without_shader_clone() {
        let draw = mesh_draw("we/genericimage4");

        let key = NativeVulkanScenePipelineKey::from_draw(&draw).unwrap();

        assert_eq!(key.shader, "we/genericimage4");
        assert_eq!(key.shader.as_ptr(), draw.material.shader.as_ptr());
        assert_eq!(key.blend, SceneBlendContract::TranslucentAlpha);
        assert_eq!(key.pipeline_class, SceneGraphPipelineClass::Mesh);
    }

    #[test]
    fn pipeline_key_rejects_empty_shader() {
        let draw = mesh_draw("");

        let err = NativeVulkanScenePipelineKey::from_draw(&draw)
            .expect_err("empty WE shader name must fail");

        assert!(err.contains("non-empty WE shader name"));
    }

    fn mesh_draw(shader: &str) -> SceneGraphDraw {
        SceneGraphDraw {
            object: SceneObjectId(2),
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: shader.to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(SceneGeometryId(4)),
            puppet: None,
            resources: Vec::<SceneGraphResourceBinding>::new(),
            index_count: 6,
        }
    }
}
