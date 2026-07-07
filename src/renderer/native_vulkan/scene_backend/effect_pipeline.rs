//! Scene effect graphics pipeline key and bind command boundary.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_rd/effects/tone_mapper.cpp`
//! - `references/godot/servers/rendering/renderer_rd/effects/copy_effects.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SCENE_WE_MAX_SHADER_TEXTURE_SLOTS, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
    SceneEffectPassGraphMaterialPass, SceneObjectId, we::WeEffectKind,
};

use super::effect_resource_heap::NativeVulkanSceneEffectResourceHeapPassBindPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineKey<'a> {
    pub shader: &'a str,
    pub effect: WeEffectKind,
    pub blend: SceneEffectPassBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    #[serde(skip)]
    pub target_format: vk::Format,
    pub texture_slot_mask: u32,
    pub raster_geometry: NativeVulkanSceneEffectRasterGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectRasterGeometry {
    FullscreenTriangle,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineShaders<'a> {
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineBindPlan<'a> {
    pub key: NativeVulkanSceneEffectPipelineKey<'a>,
    pub command_order: [&'static str; 1],
}

impl<'a> NativeVulkanSceneEffectPipelineKey<'a> {
    pub(in crate::renderer::native_vulkan) fn from_pass_and_resource_heap(
        pass: &'a SceneEffectPassGraphMaterialPass,
        target_format: vk::Format,
        resource_heap: &NativeVulkanSceneEffectResourceHeapPassBindPlan,
    ) -> Result<Self, String> {
        let shader = pass.shader.as_deref().ok_or_else(|| {
            format!(
                "scene effect pass {} for object {:?} requires a WE shader artifact name",
                pass.pass_index, pass.object
            )
        })?;
        if shader.is_empty() {
            return Err(format!(
                "scene effect pass {} for object {:?} has an empty WE shader artifact name",
                pass.pass_index, pass.object
            ));
        }
        if target_format == vk::Format::UNDEFINED {
            return Err(format!(
                "scene effect pass {} for object {:?} requires a defined target format",
                pass.pass_index, pass.object
            ));
        }
        if pass.blend == SceneEffectPassBlend::Unknown {
            return Err(format!(
                "scene effect pass {} for object {:?} has unknown WE blend state",
                pass.pass_index, pass.object
            ));
        }
        if pass.graph_pass_index != resource_heap.effect_pass_index {
            return Err(format!(
                "scene effect pipeline/resource heap pass mismatch for object {:?}: pass {}, heap {}",
                pass.object, pass.graph_pass_index, resource_heap.effect_pass_index
            ));
        }
        if pass.object != resource_heap.object {
            return Err(format!(
                "scene effect pipeline/resource heap object mismatch: pass {:?}, heap {:?}",
                pass.object, resource_heap.object
            ));
        }
        Ok(Self {
            shader,
            effect: pass.effect,
            blend: pass.blend,
            depth_test: pass.depth_test,
            depth_write: pass.depth_write,
            cull_mode: pass.cull_mode,
            target_format,
            texture_slot_mask: effect_resource_heap_texture_slot_mask(
                pass.object,
                pass.pass_index,
                resource_heap,
            )?,
            raster_geometry: NativeVulkanSceneEffectRasterGeometry::FullscreenTriangle,
        })
    }
}

impl<'a> NativeVulkanSceneEffectPipelineBindPlan<'a> {
    pub(in crate::renderer::native_vulkan) fn from_key(
        key: NativeVulkanSceneEffectPipelineKey<'a>,
    ) -> Self {
        Self {
            key,
            command_order: ["cmd_bind_pipeline"],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_pipeline_bind_command<
    'a,
>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    key: NativeVulkanSceneEffectPipelineKey<'a>,
    pipeline: vk::Pipeline,
) -> Result<NativeVulkanSceneEffectPipelineBindPlan<'a>, String> {
    if pipeline == vk::Pipeline::null() {
        return Err(format!(
            "scene effect pipeline bind for shader '{}' requires a valid vk::Pipeline",
            key.shader
        ));
    }
    unsafe {
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
    }
    Ok(NativeVulkanSceneEffectPipelineBindPlan::from_key(key))
}

fn effect_resource_heap_texture_slot_mask(
    object: SceneObjectId,
    pass_index: usize,
    resource_heap: &NativeVulkanSceneEffectResourceHeapPassBindPlan,
) -> Result<u32, String> {
    let mut mask = 0u32;
    for binding in &resource_heap.texture_set.bindings {
        if binding.slot >= SCENE_WE_MAX_SHADER_TEXTURE_SLOTS {
            return Err(format!(
                "scene effect pipeline for pass {pass_index} object {object:?} texture slot {} exceeds WE slot mask width {}",
                binding.slot, SCENE_WE_MAX_SHADER_TEXTURE_SLOTS
            ));
        }
        mask |= 1u32 << binding.slot;
    }
    Ok(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectPassGraphOutput, SceneEffectTextureResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::effect_resource_heap::{
        NativeVulkanSceneEffectResourceHeapPassBindInfo,
        NativeVulkanSceneEffectResourceHeapPassBindPlan, NativeVulkanSceneEffectTextureSetBinding,
        NativeVulkanSceneEffectTextureSetKey,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

    #[test]
    fn effect_pipeline_key_tracks_pass_state_and_texture_slots() {
        let pass = effect_pass(Some("effects/iris"));
        let bind_info = pass_bind_info();
        let bind_plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("effect resource heap bind plan");

        let key = NativeVulkanSceneEffectPipelineKey::from_pass_and_resource_heap(
            &pass,
            vk::Format::R16G16B16A16_SFLOAT,
            &bind_plan,
        )
        .expect("effect pipeline key");

        assert_eq!(key.shader, "effects/iris");
        assert_eq!(key.effect, WeEffectKind::Iris);
        assert_eq!(key.blend, SceneEffectPassBlend::NormalReplace);
        assert_eq!(key.depth_test, SceneDepthTest::Disabled);
        assert_eq!(key.cull_mode, SceneCullMode::None);
        assert_eq!(key.target_format, vk::Format::R16G16B16A16_SFLOAT);
        assert_eq!(key.texture_slot_mask, 0b101);
        assert_eq!(
            key.raster_geometry,
            NativeVulkanSceneEffectRasterGeometry::FullscreenTriangle
        );
    }

    #[test]
    fn effect_pipeline_key_rejects_missing_shader_artifact_name() {
        let pass = effect_pass(None);
        let bind_info = pass_bind_info();
        let bind_plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("effect resource heap bind plan");

        let err = NativeVulkanSceneEffectPipelineKey::from_pass_and_resource_heap(
            &pass,
            vk::Format::R16G16B16A16_SFLOAT,
            &bind_plan,
        )
        .expect_err("missing shader must fail");

        assert!(err.contains("requires a WE shader artifact name"));
    }

    fn effect_pass(shader: Option<&str>) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_pass_index: 2,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 9,
            effect_file: "effects/iris/effect.json".to_owned(),
            effect: WeEffectKind::Iris,
            shader: shader.map(str::to_owned),
            source: Some(SceneEffectPassGraphInputBinding {
                slot: 0,
                image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
                source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(12)),
            }),
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::EffectTarget(0)),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            texture_resources: vec![SceneEffectTextureResourceBinding {
                slot: 2,
                resource: SceneResourceId(13),
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
                    source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                        SceneResourceId(12),
                    ),
                },
                NativeVulkanSceneEffectTextureSetBinding {
                    slot: 2,
                    role: SceneGraphResourceRole::shader_texture(2),
                    source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                        SceneResourceId(13),
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
