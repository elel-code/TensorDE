//! Scene effect material pass command planning for retained Vulkan resources.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/renderer_rd/effects/tone_mapper.cpp`
//! - `references/godot/servers/rendering/renderer_rd/effects/copy_effects.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneGraphTarget, SceneObjectId,
};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineBindPlan, NativeVulkanSceneEffectPipelineKey,
    native_vulkan_record_scene_effect_pipeline_bind_command,
};
use super::effect_resource_heap::{
    NativeVulkanSceneEffectResourceHeapPassBindInfo,
    NativeVulkanSceneEffectResourceHeapPassBindPlan,
    native_vulkan_record_scene_effect_resource_heap_pass_bind_command,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPassCommandPlan<'a> {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub shader: &'a str,
    pub output: SceneGraphTarget,
    pub texture_count: usize,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub fullscreen_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneEffectPassCommand<'a>>,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectPassCommand<'a> {
    BeginPass {
        effect_pass_index: usize,
        object: SceneObjectId,
        shader: &'a str,
        output: SceneGraphTarget,
    },
    BindPipeline {
        bind: NativeVulkanSceneEffectPipelineBindPlan<'a>,
    },
    BindResourceHeap {
        bind: NativeVulkanSceneEffectResourceHeapPassBindPlan,
    },
    DrawFullscreenTriangle {
        draw: NativeVulkanSceneEffectFullscreenDrawPlan,
    },
    EndPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectFullscreenDrawPlan {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub vertex_count: u32,
    pub instance_count: u32,
    pub command_order: [&'static str; 1],
}

impl<'a> NativeVulkanSceneEffectPassCommandPlan<'a> {
    pub(in crate::renderer::native_vulkan) fn from_pass_bindings(
        pass: &'a SceneEffectPassGraphMaterialPass,
        target_format: vk::Format,
        bind_info: &NativeVulkanSceneEffectResourceHeapPassBindInfo,
    ) -> Result<Self, String> {
        let output = effect_pass_render_target(pass)?;
        let bind = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            pass, bind_info,
        )?;
        let pipeline_key = NativeVulkanSceneEffectPipelineKey::from_pass_and_resource_heap(
            pass,
            target_format,
            &bind,
        )?;
        let pipeline_bind = NativeVulkanSceneEffectPipelineBindPlan::from_key(pipeline_key);
        let draw = NativeVulkanSceneEffectFullscreenDrawPlan::from_pass(pass);
        Ok(Self::from_parts(
            pass,
            output,
            bind.texture_count,
            pipeline_bind,
            bind,
            draw,
        ))
    }

    fn from_parts(
        pass: &'a SceneEffectPassGraphMaterialPass,
        output: SceneGraphTarget,
        texture_count: usize,
        pipeline_bind: NativeVulkanSceneEffectPipelineBindPlan<'a>,
        resource_bind: NativeVulkanSceneEffectResourceHeapPassBindPlan,
        draw: NativeVulkanSceneEffectFullscreenDrawPlan,
    ) -> Self {
        let shader = pipeline_bind.key.shader;
        Self {
            effect_pass_index: pass.graph_pass_index,
            object: pass.object,
            shader,
            output,
            texture_count,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            fullscreen_draw_count: 1,
            commands: vec![
                NativeVulkanSceneEffectPassCommand::BeginPass {
                    effect_pass_index: pass.graph_pass_index,
                    object: pass.object,
                    shader,
                    output,
                },
                NativeVulkanSceneEffectPassCommand::BindPipeline {
                    bind: pipeline_bind,
                },
                NativeVulkanSceneEffectPassCommand::BindResourceHeap {
                    bind: resource_bind,
                },
                NativeVulkanSceneEffectPassCommand::DrawFullscreenTriangle { draw },
                NativeVulkanSceneEffectPassCommand::EndPass,
            ],
            command_order: [
                "resolve_effect_pass_render_target",
                "cmd_bind_pipeline",
                "cmd_bind_resource_heap_ext",
                "cmd_bind_sampler_heap_ext",
                "cmd_draw_fullscreen_triangle",
            ],
        }
    }
}

impl NativeVulkanSceneEffectFullscreenDrawPlan {
    fn from_pass(pass: &SceneEffectPassGraphMaterialPass) -> Self {
        Self {
            effect_pass_index: pass.graph_pass_index,
            object: pass.object,
            vertex_count: 3,
            instance_count: 1,
            command_order: ["cmd_draw_fullscreen_triangle"],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_material_pass_commands<
    'a,
    PipelineForKey,
>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &'a SceneEffectPassGraphMaterialPass,
    target_format: vk::Format,
    bind_info: NativeVulkanSceneEffectResourceHeapPassBindInfo,
    mut pipeline_for_key: PipelineForKey,
) -> Result<NativeVulkanSceneEffectPassCommandPlan<'a>, String>
where
    PipelineForKey: FnMut(NativeVulkanSceneEffectPipelineKey<'a>) -> Result<vk::Pipeline, String>,
{
    let output = effect_pass_render_target(pass)?;
    let validated_bind =
        NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(pass, &bind_info)?;
    let pipeline_key = NativeVulkanSceneEffectPipelineKey::from_pass_and_resource_heap(
        pass,
        target_format,
        &validated_bind,
    )?;
    let pipeline = pipeline_for_key(pipeline_key)?;
    let pipeline_bind = native_vulkan_record_scene_effect_pipeline_bind_command(
        device,
        command_buffer,
        pipeline_key,
        pipeline,
    )?;
    let resource_bind = native_vulkan_record_scene_effect_resource_heap_pass_bind_command(
        device,
        command_buffer,
        pass,
        bind_info,
    )?;
    let draw =
        native_vulkan_record_scene_effect_fullscreen_draw_command(device, command_buffer, pass)?;
    Ok(NativeVulkanSceneEffectPassCommandPlan::from_parts(
        pass,
        output,
        resource_bind.texture_count,
        pipeline_bind,
        resource_bind,
        draw,
    ))
}

fn native_vulkan_record_scene_effect_fullscreen_draw_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &SceneEffectPassGraphMaterialPass,
) -> Result<NativeVulkanSceneEffectFullscreenDrawPlan, String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err(format!(
            "scene effect pass {} for object {:?} requires a valid command buffer",
            pass.pass_index, pass.object
        ));
    }
    let draw = NativeVulkanSceneEffectFullscreenDrawPlan::from_pass(pass);
    unsafe {
        device.cmd_draw(command_buffer, draw.vertex_count, draw.instance_count, 0, 0);
    }
    Ok(draw)
}

fn effect_pass_render_target(
    pass: &SceneEffectPassGraphMaterialPass,
) -> Result<SceneGraphTarget, String> {
    match pass.output {
        SceneEffectPassGraphOutput::GraphTarget(target) => Ok(target),
        SceneEffectPassGraphOutput::ObjectFinal(object) => {
            if object != pass.object {
                return Err(format!(
                    "scene effect pass {} for object {:?} has mismatched ObjectFinal({object:?}) output",
                    pass.pass_index, pass.object
                ));
            }
            Ok(SceneGraphTarget::ObjectFinal(object))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectTextureResourceBinding, SceneGraphResourceRole, SceneResourceId,
        we::WeEffectKind,
    };
    use crate::renderer::native_vulkan::scene_backend::effect_resource_heap::{
        NativeVulkanSceneEffectTextureSetBinding, NativeVulkanSceneEffectTextureSetKey,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

    #[test]
    fn effect_pass_plan_binds_pipeline_heap_then_fullscreen_triangle() {
        let pass = effect_pass(SceneEffectPassGraphOutput::GraphTarget(
            SceneGraphTarget::NamedFbo(2),
        ));
        let bind_info = pass_bind_info();

        let plan = NativeVulkanSceneEffectPassCommandPlan::from_pass_bindings(
            &pass,
            vk::Format::R16G16B16A16_SFLOAT,
            &bind_info,
        )
        .expect("effect pass command plan");

        assert_eq!(plan.effect_pass_index, 2);
        assert_eq!(plan.object, SceneObjectId(7));
        assert_eq!(plan.shader, "effects/fluidsimulation_vorticity");
        assert_eq!(plan.output, SceneGraphTarget::NamedFbo(2));
        assert_eq!(plan.texture_count, 2);
        assert_eq!(plan.pipeline_bind_count, 1);
        assert_eq!(plan.resource_heap_bind_count, 1);
        assert_eq!(plan.fullscreen_draw_count, 1);
        assert_eq!(
            plan.command_order,
            [
                "resolve_effect_pass_render_target",
                "cmd_bind_pipeline",
                "cmd_bind_resource_heap_ext",
                "cmd_bind_sampler_heap_ext",
                "cmd_draw_fullscreen_triangle"
            ]
        );
        assert!(matches!(
            plan.commands.as_slice(),
            [
                NativeVulkanSceneEffectPassCommand::BeginPass { .. },
                NativeVulkanSceneEffectPassCommand::BindPipeline { .. },
                NativeVulkanSceneEffectPassCommand::BindResourceHeap { .. },
                NativeVulkanSceneEffectPassCommand::DrawFullscreenTriangle {
                    draw: NativeVulkanSceneEffectFullscreenDrawPlan {
                        vertex_count: 3,
                        instance_count: 1,
                        ..
                    }
                },
                NativeVulkanSceneEffectPassCommand::EndPass,
            ]
        ));
    }

    #[test]
    fn effect_pass_plan_resolves_object_final_to_retained_target() {
        let pass = effect_pass(SceneEffectPassGraphOutput::ObjectFinal(SceneObjectId(7)));
        let bind_info = pass_bind_info();

        let plan = NativeVulkanSceneEffectPassCommandPlan::from_pass_bindings(
            &pass,
            vk::Format::R16G16B16A16_SFLOAT,
            &bind_info,
        )
        .expect("object final pass command");

        assert_eq!(plan.output, SceneGraphTarget::ObjectFinal(SceneObjectId(7)));
    }

    fn effect_pass(output: SceneEffectPassGraphOutput) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 2,
            graph_pass_index: 2,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 9,
            effect_file: "effects/fluidsimulation/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some("effects/fluidsimulation_vorticity".to_owned()),
            source: Some(SceneEffectPassGraphInputBinding {
                slot: 0,
                image: crate::engine::scene_engine::SceneEffectImageRef::NamedFbo(
                    "_rt_SmokeVelocity1".to_owned(),
                ),
                source: SceneEffectPassGraphInputSource::GraphTarget(SceneGraphTarget::NamedFbo(1)),
            }),
            input_bindings: Vec::new(),
            output,
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: vec![SceneEffectTextureResourceBinding {
                slot: 2,
                resource: SceneResourceId(12),
            }],
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }

    fn pass_bind_info() -> NativeVulkanSceneEffectResourceHeapPassBindInfo {
        let texture_set = NativeVulkanSceneEffectTextureSetKey {
            bindings: vec![
                NativeVulkanSceneEffectTextureSetBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                        SceneGraphTarget::NamedFbo(1),
                    ),
                },
                NativeVulkanSceneEffectTextureSetBinding {
                    slot: 2,
                    role: SceneGraphResourceRole::shader_texture(2),
                    source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                        SceneResourceId(12),
                    ),
                },
            ],
        };
        NativeVulkanSceneEffectResourceHeapPassBindInfo {
            effect_pass_index: 2,
            object: SceneObjectId(7),
            resource_set_index: 11,
            texture_set,
            base_resource_descriptor_index: 4,
            resource_descriptor_count: 2,
            texture_count: 2,
            shader_mappings: Vec::new(),
            resource_bind: vk::BindHeapInfoEXT::default(),
            sampler_bind: vk::BindHeapInfoEXT::default(),
        }
    }
}
