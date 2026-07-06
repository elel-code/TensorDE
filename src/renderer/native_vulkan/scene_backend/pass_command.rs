//! Scene pass command planning for retained mesh draw buffers.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGeometryId, SceneGraphPass, SceneGraphPipelineClass, SceneGraphTarget, SceneObjectId,
};

use super::draw_command::{
    NativeVulkanSceneMeshDrawCommandPlan, native_vulkan_record_scene_mesh_draw_command,
};
use super::pipeline::{
    NativeVulkanScenePipelineBindPlan, NativeVulkanScenePipelineKey,
    native_vulkan_record_scene_pipeline_bind_command,
};
use super::resource_buffers::{
    NativeVulkanSceneMeshDrawBufferRecords, NativeVulkanSceneMeshDrawBuffers,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshPassCommandPlan<'a> {
    pub name: &'a str,
    pub input: Option<SceneGraphTarget>,
    pub output: SceneGraphTarget,
    pub draw_count: usize,
    pub pipeline_bind_count: usize,
    pub indexed_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneMeshPassCommand<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NativeVulkanSceneMeshPassCommand<'a> {
    BeginPass {
        name: &'a str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
    },
    BindPipeline {
        bind: NativeVulkanScenePipelineBindPlan<'a>,
    },
    DrawIndexed {
        object: SceneObjectId,
        geometry: SceneGeometryId,
        draw: NativeVulkanSceneMeshDrawCommandPlan,
    },
    EndPass,
}

impl<'a> NativeVulkanSceneMeshPassCommandPlan<'a> {
    pub fn from_record_bindings<F>(
        pass: &'a SceneGraphPass,
        mut mesh_records: F,
    ) -> Result<Self, String>
    where
        F: FnMut(SceneGeometryId) -> Result<NativeVulkanSceneMeshDrawBufferRecords, String>,
    {
        let mut commands = Vec::with_capacity(pass.draws.len().saturating_mul(2) + 2);
        commands.push(NativeVulkanSceneMeshPassCommand::BeginPass {
            name: pass.name.as_str(),
            input: pass.input,
            output: pass.output,
        });

        let mut last_pipeline = None;
        let mut pipeline_bind_count = 0usize;
        let mut indexed_draw_count = 0usize;

        for draw in &pass.draws {
            if draw.pipeline != SceneGraphPipelineClass::Mesh {
                return Err(format!(
                    "scene mesh pass '{}' requires Mesh pipeline, got {:?} for object {:?}",
                    pass.name, draw.pipeline, draw.object
                ));
            }

            let key = NativeVulkanScenePipelineKey::from_draw(draw)?;
            if last_pipeline != Some(key) {
                commands.push(NativeVulkanSceneMeshPassCommand::BindPipeline {
                    bind: NativeVulkanScenePipelineBindPlan::from_key(key),
                });
                last_pipeline = Some(key);
                pipeline_bind_count += 1;
            }

            let geometry = draw.geometry.ok_or_else(|| {
                format!(
                    "scene mesh pass '{}' draw requires geometry handle",
                    pass.name
                )
            })?;
            let records = mesh_records(geometry)?;
            let draw_plan =
                NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(draw, &records)?;
            commands.push(NativeVulkanSceneMeshPassCommand::DrawIndexed {
                object: draw.object,
                geometry: draw_plan.geometry,
                draw: draw_plan,
            });
            indexed_draw_count += 1;
        }

        commands.push(NativeVulkanSceneMeshPassCommand::EndPass);

        Ok(Self {
            name: pass.name.as_str(),
            input: pass.input,
            output: pass.output,
            draw_count: pass.draws.len(),
            pipeline_bind_count,
            indexed_draw_count,
            commands,
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_pass_draw_commands<
    'a,
    PipelineForKey,
    MeshBuffersForGeometry,
>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &'a SceneGraphPass,
    mut pipeline_for_key: PipelineForKey,
    mut mesh_buffers: MeshBuffersForGeometry,
) -> Result<NativeVulkanSceneMeshPassCommandPlan<'a>, String>
where
    PipelineForKey: FnMut(NativeVulkanScenePipelineKey<'a>) -> Result<vk::Pipeline, String>,
    MeshBuffersForGeometry:
        FnMut(SceneGeometryId) -> Result<NativeVulkanSceneMeshDrawBuffers, String>,
{
    let mut commands = Vec::with_capacity(pass.draws.len().saturating_mul(2) + 2);
    commands.push(NativeVulkanSceneMeshPassCommand::BeginPass {
        name: pass.name.as_str(),
        input: pass.input,
        output: pass.output,
    });

    let mut last_pipeline = None;
    let mut pipeline_bind_count = 0usize;
    let mut indexed_draw_count = 0usize;

    for draw in &pass.draws {
        if draw.pipeline != SceneGraphPipelineClass::Mesh {
            return Err(format!(
                "scene mesh pass '{}' requires Mesh pipeline, got {:?} for object {:?}",
                pass.name, draw.pipeline, draw.object
            ));
        }

        let key = NativeVulkanScenePipelineKey::from_draw(draw)?;
        if last_pipeline != Some(key) {
            let pipeline = pipeline_for_key(key)?;
            let bind = native_vulkan_record_scene_pipeline_bind_command(
                device,
                command_buffer,
                key,
                pipeline,
            )?;
            commands.push(NativeVulkanSceneMeshPassCommand::BindPipeline { bind });
            last_pipeline = Some(key);
            pipeline_bind_count += 1;
        }

        let geometry = draw.geometry.ok_or_else(|| {
            format!(
                "scene mesh pass '{}' draw requires geometry handle",
                pass.name
            )
        })?;
        let buffers = mesh_buffers(geometry)?;
        let draw_plan =
            native_vulkan_record_scene_mesh_draw_command(device, command_buffer, draw, buffers)?;
        commands.push(NativeVulkanSceneMeshPassCommand::DrawIndexed {
            object: draw.object,
            geometry: draw_plan.geometry,
            draw: draw_plan,
        });
        indexed_draw_count += 1;
    }

    commands.push(NativeVulkanSceneMeshPassCommand::EndPass);

    Ok(NativeVulkanSceneMeshPassCommandPlan {
        name: pass.name.as_str(),
        input: pass.input,
        output: pass.output,
        draw_count: pass.draws.len(),
        pipeline_bind_count,
        indexed_draw_count,
        commands,
    })
}

#[cfg(test)]
mod tests {
    use super::super::resource_buffers::NativeVulkanSceneGpuBufferKey;
    use super::super::resource_buffers::NativeVulkanSceneGpuBufferRecordBinding;
    use super::super::resource_storage::{
        NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
    };
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGraphDraw, SceneGraphResourceBinding, SceneMaterialKey,
    };

    #[test]
    fn mesh_pass_plan_binds_same_pipeline_once_for_consecutive_draws() {
        let pass = mesh_pass(vec![
            mesh_draw(SceneObjectId(1), SceneGeometryId(4), "we/genericimage4"),
            mesh_draw(SceneObjectId(2), SceneGeometryId(5), "we/genericimage4"),
        ]);

        let plan = NativeVulkanSceneMeshPassCommandPlan::from_record_bindings(&pass, mesh_records)
            .expect("mesh pass plan");

        assert_eq!(plan.draw_count, 2);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.indexed_draw_count, 2);
        assert!(matches!(
            plan.commands.as_slice(),
            [
                NativeVulkanSceneMeshPassCommand::BeginPass { .. },
                NativeVulkanSceneMeshPassCommand::BindPipeline { .. },
                NativeVulkanSceneMeshPassCommand::DrawIndexed {
                    object: SceneObjectId(1),
                    geometry: SceneGeometryId(4),
                    ..
                },
                NativeVulkanSceneMeshPassCommand::DrawIndexed {
                    object: SceneObjectId(2),
                    geometry: SceneGeometryId(5),
                    ..
                },
                NativeVulkanSceneMeshPassCommand::EndPass,
            ]
        ));
    }

    #[test]
    fn mesh_pass_plan_rebinds_when_pipeline_key_changes() {
        let pass = mesh_pass(vec![
            mesh_draw(SceneObjectId(1), SceneGeometryId(4), "we/genericimage4"),
            mesh_draw(SceneObjectId(2), SceneGeometryId(5), "we/additive"),
        ]);

        let plan = NativeVulkanSceneMeshPassCommandPlan::from_record_bindings(&pass, mesh_records)
            .expect("mesh pass plan");

        assert_eq!(plan.pipeline_bind_count, 2);
    }

    #[test]
    fn mesh_pass_plan_rejects_non_mesh_draws() {
        let mut draw = mesh_draw(SceneObjectId(1), SceneGeometryId(4), "we/genericimage4");
        draw.pipeline = SceneGraphPipelineClass::PuppetSkinning;
        let pass = mesh_pass(vec![draw]);

        let err = NativeVulkanSceneMeshPassCommandPlan::from_record_bindings(&pass, mesh_records)
            .expect_err("non-mesh draw must fail");

        assert!(err.contains("requires Mesh pipeline"));
    }

    #[test]
    fn mesh_pass_plan_propagates_missing_buffer_records() {
        let pass = mesh_pass(vec![mesh_draw(
            SceneObjectId(1),
            SceneGeometryId(4),
            "we/genericimage4",
        )]);

        let err = NativeVulkanSceneMeshPassCommandPlan::from_record_bindings(&pass, |_| {
            Err("missing retained scene GPU buffer record".to_owned())
        })
        .expect_err("missing records must fail");

        assert!(err.contains("missing retained scene GPU buffer record"));
    }

    fn mesh_pass(draws: Vec<SceneGraphDraw>) -> SceneGraphPass {
        SceneGraphPass {
            name: "mesh-main".to_owned(),
            input: None,
            output: SceneGraphTarget::Swapchain,
            draws,
        }
    }

    fn mesh_draw(object: SceneObjectId, geometry: SceneGeometryId, shader: &str) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: shader.to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(geometry),
            puppet: None,
            resources: Vec::<SceneGraphResourceBinding>::new(),
            index_count: 6,
        }
    }

    fn mesh_records(
        geometry: SceneGeometryId,
    ) -> Result<NativeVulkanSceneMeshDrawBufferRecords, String> {
        Ok(NativeVulkanSceneMeshDrawBufferRecords {
            geometry,
            vertex: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                    role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                },
                bytes: 120,
                payload_hash: u64::from(geometry.0) + 11,
            },
            index: NativeVulkanSceneGpuBufferRecordBinding {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                    role: NativeVulkanSceneGpuBufferRole::MeshIndex,
                },
                bytes: 24,
                payload_hash: u64::from(geometry.0) + 12,
            },
        })
    }
}
