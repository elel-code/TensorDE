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

use super::key::{
    NativeVulkanSceneLayerAlphaMaskResourceSetKey, alpha_mask_texture_bind_resource_set,
};
use super::store::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskTextureBindPlan, NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
    pub(in crate::renderer::native_vulkan) heap_bind_index: usize,
    pub(in crate::renderer::native_vulkan) object: SceneObjectId,
    pub(in crate::renderer::native_vulkan) puppet: ScenePuppetId,
    pub(in crate::renderer::native_vulkan) shader: String,
    pub(in crate::renderer::native_vulkan) role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    pub(in crate::renderer::native_vulkan) resource_set_index: usize,
    pub(in crate::renderer::native_vulkan) resource_set:
        NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    pub(in crate::renderer::native_vulkan) base_resource_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) base_sampler_descriptor_index: usize,
    pub(in crate::renderer::native_vulkan) resource_descriptor_count: usize,
    pub(in crate::renderer::native_vulkan) texture_count: usize,
    pub(in crate::renderer::native_vulkan) shader_mappings: Vec<String>,
    pub(in crate::renderer::native_vulkan) command_order: [&'static str; 2],
}

impl NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
    pub(in crate::renderer::native_vulkan) fn from_bind_info(
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    ) -> Self {
        Self {
            heap_bind_index: bind_info.heap_bind_index,
            object: bind_info.object,
            puppet: bind_info.puppet,
            shader: bind_info.shader.clone(),
            role: bind_info.role,
            resource_set_index: bind_info.resource_set_index,
            resource_set: bind_info.resource_set.clone(),
            base_resource_descriptor_index: bind_info.base_resource_descriptor_index,
            base_sampler_descriptor_index: bind_info.base_sampler_descriptor_index,
            resource_descriptor_count: bind_info.resource_descriptor_count,
            texture_count: bind_info.texture_count,
            shader_mappings: bind_info.shader_mappings.clone(),
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        }
    }

    pub(in crate::renderer::native_vulkan) fn from_texture_bind_and_bind_info(
        heap_bind_index: usize,
        texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    ) -> Result<Self, String> {
        let resource_set = alpha_mask_texture_bind_resource_set(texture_bind)?;
        if heap_bind_index != bind_info.heap_bind_index {
            return Err(format!(
                "scene layer alpha-mask resource heap bind heap-bind mismatch: descriptor {}, heap {}",
                heap_bind_index, bind_info.heap_bind_index
            ));
        }
        if texture_bind.object != bind_info.object {
            return Err(format!(
                "scene layer alpha-mask resource heap bind object mismatch: descriptor {:?}, heap {:?}",
                texture_bind.object, bind_info.object
            ));
        }
        if texture_bind.puppet != bind_info.puppet {
            return Err(format!(
                "scene layer alpha-mask resource heap bind puppet mismatch for object {:?}: descriptor {:?}, heap {:?}",
                texture_bind.object, texture_bind.puppet, bind_info.puppet
            ));
        }
        if texture_bind.shader != bind_info.shader {
            return Err(format!(
                "scene layer alpha-mask resource heap bind shader mismatch for object {:?}: descriptor {}, heap {}",
                texture_bind.object, texture_bind.shader, bind_info.shader
            ));
        }
        if texture_bind.role != bind_info.role {
            return Err(format!(
                "scene layer alpha-mask resource heap bind role mismatch for object {:?}: descriptor {:?}, heap {:?}",
                texture_bind.object, texture_bind.role, bind_info.role
            ));
        }
        if resource_set != bind_info.resource_set {
            return Err(format!(
                "scene layer alpha-mask resource heap bind resource-set mismatch for heap bind {} object {:?}: descriptor {:?}, heap {:?}",
                heap_bind_index, texture_bind.object, resource_set, bind_info.resource_set
            ));
        }
        if resource_set.bindings.len() != bind_info.texture_count {
            return Err(format!(
                "scene layer alpha-mask resource heap bind texture count mismatch for heap bind {} object {:?}: descriptor {}, heap {}",
                heap_bind_index,
                texture_bind.object,
                resource_set.bindings.len(),
                bind_info.texture_count
            ));
        }
        Ok(Self::from_bind_info(bind_info))
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_resource_heap_bind_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    heap_bind_index: usize,
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
    bind_info: NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
) -> Result<NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan, String> {
    let plan =
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_texture_bind_and_bind_info(
            heap_bind_index,
            texture_bind,
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
        let texture_bind = texture_bind();
        let resource_set =
            alpha_mask_texture_bind_resource_set(&texture_bind).expect("alpha mask resource set");
        let bind_info = bind_info(
            0,
            SceneObjectId(77),
            ScenePuppetId(5),
            resource_set,
            11,
            3,
            5,
        );

        let plan =
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_texture_bind_and_bind_info(
                0,
                &texture_bind,
                &bind_info,
            )
            .expect("alpha mask bind plan");

        assert_eq!(plan.heap_bind_index, 0);
        assert_eq!(plan.object, SceneObjectId(77));
        assert_eq!(plan.puppet, ScenePuppetId(5));
        assert_eq!(plan.resource_set_index, 11);
        assert_eq!(plan.base_resource_descriptor_index, 3);
        assert_eq!(plan.base_sampler_descriptor_index, 5);
        assert_eq!(plan.resource_descriptor_count, 2);
        assert_eq!(plan.texture_count, 2);
        assert_eq!(
            plan.command_order,
            ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"]
        );
    }

    #[test]
    fn alpha_mask_resource_heap_bind_plan_rejects_resource_set_mismatch() {
        let texture_bind = texture_bind();
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
            5,
        );

        let err =
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_texture_bind_and_bind_info(
                0,
                &texture_bind,
                &bind_info,
            )
            .expect_err("resource set mismatch must fail");

        assert!(err.contains("resource-set mismatch"));
    }

    fn texture_bind() -> NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
        NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "we/clippingmaskimage4",
            role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
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
        heap_bind_index: usize,
        object: SceneObjectId,
        puppet: ScenePuppetId,
        resource_set: NativeVulkanSceneLayerAlphaMaskResourceSetKey,
        resource_set_index: usize,
        base_resource_descriptor_index: usize,
        base_sampler_descriptor_index: usize,
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
            heap_bind_index,
            object,
            puppet,
            shader: "we/clippingmaskimage4".to_owned(),
            role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index: 0,
            },
            resource_set_index,
            resource_set,
            base_resource_descriptor_index,
            base_sampler_descriptor_index,
            resource_descriptor_count: texture_count,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }
}
