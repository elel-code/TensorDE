//! Scene effect resource-set descriptor heap bind command recording.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{SceneEffectPassGraphMaterialPass, SceneObjectId};

use super::key::{NativeVulkanSceneEffectTextureSetKey, effect_pass_texture_set_key};
use super::store::NativeVulkanSceneEffectResourceHeapPassBindInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneEffectResourceHeapPassBindPlan {
    pub(in crate::renderer::native_vulkan) effect_pass_index: usize,
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) resource_set_index: usize,
    pub(in crate::renderer::native_vulkan) texture_set: NativeVulkanSceneEffectTextureSetKey,
    pub(in crate::renderer::native_vulkan) base_resource_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) resource_descriptor_count: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneEffectResourceHeapPassBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_pass_and_bind_info(
        pass: &SceneEffectPassGraphMaterialPass,
        bind_info: &NativeVulkanSceneEffectResourceHeapPassBindInfo,
    ) -> Result<Self, String> {
        let texture_set = effect_pass_texture_set_key(pass)?;
        if pass.graph_pass_index != bind_info.effect_pass_index {
            return Err(format!(
                "scene effect resource heap bind pass-index mismatch for object {:?}: pass {}, heap {}",
                pass.object, pass.graph_pass_index, bind_info.effect_pass_index
            ));
        }
        if pass.object != bind_info.object {
            return Err(format!(
                "scene effect resource heap bind object mismatch: pass {:?}, heap {:?}",
                pass.object, bind_info.object
            ));
        }
        if texture_set != bind_info.texture_set {
            return Err(format!(
                "scene effect resource heap bind texture-set mismatch for pass {} object {:?}: pass {:?}, heap {:?}",
                pass.graph_pass_index, pass.object, texture_set, bind_info.texture_set
            ));
        }
        if texture_set.bindings.len() != bind_info.texture_count {
            return Err(format!(
                "scene effect resource heap bind texture count mismatch for pass {} object {:?}: pass {}, heap {}",
                pass.graph_pass_index,
                pass.object,
                texture_set.bindings.len(),
                bind_info.texture_count
            ));
        }
        Ok(Self {
            effect_pass_index: pass.graph_pass_index,
            object: pass.object,
            resource_set_index: bind_info.resource_set_index,
            texture_set,
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            resource_descriptor_count: bind_info.resource_descriptor_count,
            texture_count: bind_info.texture_count,
            shader_mappings: bind_info.shader_mappings.clone(),
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_resource_heap_pass_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &SceneEffectPassGraphMaterialPass,
    bind_info: NativeVulkanSceneEffectResourceHeapPassBindInfo,
) -> Result<NativeVulkanSceneEffectResourceHeapPassBindPlan, String> {
    let plan =
        NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(pass, &bind_info)?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneCullMode, SceneDepthTest, SceneEffectPassBlend, SceneEffectPassGraphInputBinding,
        SceneEffectPassGraphInputSource, SceneEffectPassGraphOutput,
        SceneEffectTextureResourceBinding, SceneGraphResourceRole, SceneGraphTarget,
        SceneResourceId, we::WeEffectKind,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

    #[test]
    fn effect_resource_heap_pass_bind_plan_tracks_resource_set_identity() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let bind_info = pass_bind_info(2, SceneObjectId(7), texture_set, 11, 3);

        let plan = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect("effect pass resource heap bind plan");

        assert_eq!(plan.effect_pass_index, 2);
        assert_eq!(plan.object, SceneObjectId(7));
        assert_eq!(plan.resource_set_index, 11);
        assert_eq!(plan.base_resource_descriptor_index, 3);
        assert_eq!(plan.resource_descriptor_count, 2);
        assert_eq!(plan.texture_count, 2);
        assert_eq!(
            plan.command_order,
            ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_rejects_wrong_pass_binding() {
        let pass = effect_pass();
        let texture_set = effect_pass_texture_set_key(&pass).expect("effect texture set");
        let bind_info = pass_bind_info(3, SceneObjectId(7), texture_set, 11, 3);

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("wrong pass binding must fail");

        assert!(err.contains("pass-index mismatch"));
    }

    #[test]
    fn effect_resource_heap_pass_bind_plan_rejects_texture_set_mismatch() {
        let pass = effect_pass();
        let bind_info = pass_bind_info(
            2,
            SceneObjectId(7),
            NativeVulkanSceneEffectTextureSetKey {
                bindings: vec![super::super::NativeVulkanSceneEffectTextureSetBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                        SceneResourceId(99),
                    ),
                }],
            },
            11,
            3,
        );

        let err = NativeVulkanSceneEffectResourceHeapPassBindPlan::from_pass_and_bind_info(
            &pass, &bind_info,
        )
        .expect_err("texture set mismatch must fail");

        assert!(err.contains("texture-set mismatch"));
    }

    fn effect_pass() -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
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
            output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::NamedFbo(2)),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            texture_resources: vec![SceneEffectTextureResourceBinding {
                slot: 2,
                resource: SceneResourceId(12),
            }],
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }

    fn pass_bind_info(
        effect_pass_index: usize,
        object: SceneObjectId,
        texture_set: NativeVulkanSceneEffectTextureSetKey,
        resource_set_index: usize,
        base_resource_descriptor_index: usize,
    ) -> NativeVulkanSceneEffectResourceHeapPassBindInfo {
        let texture_count = texture_set.bindings.len();
        let shader_mappings = texture_set
            .bindings
            .iter()
            .enumerate()
            .map(|(ordinal, binding)| {
                format!(
                    "set0.binding{}.g_Texture{} -> effect-resource-set-offset{}",
                    binding.slot, binding.slot, ordinal
                )
            })
            .collect();
        NativeVulkanSceneEffectResourceHeapPassBindInfo {
            effect_pass_index,
            object,
            resource_set_index,
            texture_set,
            base_resource_descriptor_index,
            resource_descriptor_count: texture_count,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }
}
