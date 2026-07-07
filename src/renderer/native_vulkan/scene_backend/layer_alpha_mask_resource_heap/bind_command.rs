//! WE layer alpha-mask descriptor heap bind command recording.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene_engine::{SceneObjectId, ScenePuppetId};

use super::key::{NativeVulkanSceneLayerAlphaMaskResourceSetKey, alpha_mask_descriptor_set_key};
use super::store::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan,
    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
    pub(in crate::renderer::native_vulkan) descriptor_set_index: usize,
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) puppet: ScenePuppetId,
    pub(in crate::renderer::native_vulkan) shader: String,
    pub(in crate::renderer::native_vulkan) role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    pub(in crate::renderer::native_vulkan) resource_set_index: usize,
    pub(in crate::renderer::native_vulkan) resource_set:
        NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    pub(in crate::renderer::native_vulkan) base_resource_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) resource_descriptor_count: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_descriptor_set_and_bind_info(
        descriptor_set_index: usize,
        descriptor_set: &NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan,
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    ) -> Result<Self, String> {
        let resource_set = alpha_mask_descriptor_set_key(descriptor_set)?;
        if descriptor_set_index != bind_info.descriptor_set_index {
            return Err(format!(
                "scene layer alpha-mask resource heap bind descriptor-set mismatch: descriptor {}, heap {}",
                descriptor_set_index, bind_info.descriptor_set_index
            ));
        }
        if descriptor_set.object != bind_info.object {
            return Err(format!(
                "scene layer alpha-mask resource heap bind object mismatch: descriptor {:?}, heap {:?}",
                descriptor_set.object, bind_info.object
            ));
        }
        if descriptor_set.puppet != bind_info.puppet {
            return Err(format!(
                "scene layer alpha-mask resource heap bind puppet mismatch for object {:?}: descriptor {:?}, heap {:?}",
                descriptor_set.object, descriptor_set.puppet, bind_info.puppet
            ));
        }
        if descriptor_set.shader != bind_info.shader {
            return Err(format!(
                "scene layer alpha-mask resource heap bind shader mismatch for object {:?}: descriptor {}, heap {}",
                descriptor_set.object, descriptor_set.shader, bind_info.shader
            ));
        }
        if descriptor_set.role != bind_info.role {
            return Err(format!(
                "scene layer alpha-mask resource heap bind role mismatch for object {:?}: descriptor {:?}, heap {:?}",
                descriptor_set.object, descriptor_set.role, bind_info.role
            ));
        }
        if resource_set != bind_info.resource_set {
            return Err(format!(
                "scene layer alpha-mask resource heap bind resource-set mismatch for descriptor set {} object {:?}: descriptor {:?}, heap {:?}",
                descriptor_set_index, descriptor_set.object, resource_set, bind_info.resource_set
            ));
        }
        if resource_set.bindings.len() != bind_info.texture_count {
            return Err(format!(
                "scene layer alpha-mask resource heap bind texture count mismatch for descriptor set {} object {:?}: descriptor {}, heap {}",
                descriptor_set_index,
                descriptor_set.object,
                resource_set.bindings.len(),
                bind_info.texture_count
            ));
        }
        Ok(Self {
            descriptor_set_index,
            object: descriptor_set.object,
            puppet: descriptor_set.puppet,
            shader: descriptor_set.shader.to_owned(),
            role: descriptor_set.role,
            resource_set_index: bind_info.resource_set_index,
            resource_set,
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            resource_descriptor_count: bind_info.resource_descriptor_count,
            texture_count: bind_info.texture_count,
            shader_mappings: bind_info.shader_mappings.clone(),
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_resource_heap_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    descriptor_set_index: usize,
    descriptor_set: &NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan,
    bind_info: NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
) -> Result<NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan, String> {
    let plan =
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_descriptor_set_and_bind_info(
            descriptor_set_index,
            descriptor_set,
            &bind_info,
        )?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &bind_info.resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &bind_info.sampler_bind);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{SceneObjectId, ScenePuppetId, SceneResourceId};
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
        NativeVulkanSceneLayerAlphaMaskDescriptorSource, NativeVulkanSceneLayerAlphaMaskSlotBinding,
    };
    use crate::renderer::native_vulkan::scene_backend::resource_heap::texture_set::scene_shader_texture_mapping;

    #[test]
    fn alpha_mask_resource_heap_bind_plan_tracks_resource_set_identity() {
        let descriptor_set = descriptor_set();
        let resource_set =
            alpha_mask_descriptor_set_key(&descriptor_set).expect("alpha mask resource set");
        let bind_info = bind_info(0, SceneObjectId(77), ScenePuppetId(5), resource_set, 11, 3);

        let plan =
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_descriptor_set_and_bind_info(
                0,
                &descriptor_set,
                &bind_info,
            )
            .expect("alpha mask bind plan");

        assert_eq!(plan.descriptor_set_index, 0);
        assert_eq!(plan.object, SceneObjectId(77));
        assert_eq!(plan.puppet, ScenePuppetId(5));
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
    fn alpha_mask_resource_heap_bind_plan_rejects_resource_set_mismatch() {
        let descriptor_set = descriptor_set();
        let bind_info = bind_info(
            0,
            SceneObjectId(77),
            ScenePuppetId(5),
            NativeVulkanSceneLayerAlphaMaskResourceSetKey {
                shader: "we/clippingmaskimage4".to_owned(),
                bindings: vec![
                    super::super::NativeVulkanSceneLayerAlphaMaskResourceSetBinding {
                        slot: 0,
                        source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                            SceneResourceId(99),
                        ),
                    },
                ],
            },
            11,
            3,
        );

        let err =
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_descriptor_set_and_bind_info(
                0,
                &descriptor_set,
                &bind_info,
            )
            .expect_err("resource set mismatch must fail");

        assert!(err.contains("resource-set mismatch"));
    }

    fn descriptor_set() -> NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan {
        NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan {
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "we/clippingmaskimage4",
            role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                clipping_record_index: 0,
            },
            slot_mask: 0b11,
            optional_morph_slot: Some(5),
            slots: vec![
                NativeVulkanSceneLayerAlphaMaskSlotBinding {
                    slot: 0,
                    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(9),
                    ),
                    shader_mapping: scene_shader_texture_mapping(0),
                },
                NativeVulkanSceneLayerAlphaMaskSlotBinding {
                    slot: 1,
                    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(12),
                    ),
                    shader_mapping: scene_shader_texture_mapping(1),
                },
            ],
        }
    }

    fn bind_info(
        descriptor_set_index: usize,
        object: SceneObjectId,
        puppet: ScenePuppetId,
        resource_set: NativeVulkanSceneLayerAlphaMaskResourceSetKey,
        resource_set_index: usize,
        base_resource_descriptor_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        let texture_count = resource_set.bindings.len();
        let shader_mappings = resource_set
            .bindings
            .iter()
            .enumerate()
            .map(|(ordinal, binding)| {
                format!(
                    "set0.binding{}.g_Texture{} -> alpha-mask-resource-set-offset{}",
                    binding.slot, binding.slot, ordinal
                )
            })
            .collect();
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
            descriptor_set_index,
            object,
            puppet,
            shader: "we/clippingmaskimage4".to_owned(),
            role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                clipping_record_index: 0,
            },
            resource_set_index,
            resource_set,
            base_resource_descriptor_index,
            resource_descriptor_count: texture_count,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }
}
