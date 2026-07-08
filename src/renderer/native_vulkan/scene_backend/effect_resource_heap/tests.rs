use vulkanalia::vk::{self, Handle};

use super::key::binding_shader_mapping;
use super::*;
use crate::engine::scene_engine::{
    SceneEffectUniformFramePlan, SceneGraphResourceRole, SceneIrisEffectUniformRecord,
    SceneTextureFormat,
};
use crate::renderer::native_vulkan::scene_backend::effect_descriptors::NativeVulkanSceneEffectTextureDescriptorFramePlan;
use crate::renderer::native_vulkan::scene_backend::effect_uniforms::{
    NativeVulkanSceneEffectUniformGpuBufferBinding, NativeVulkanSceneEffectUniformKey,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorFormat, NativeVulkanSceneTextureDescriptorVkFormat,
};

#[test]
fn effect_resource_heap_plan_packs_sampled_image_sets() {
    let descriptors = NativeVulkanSceneEffectTextureDescriptorFramePlan {
        pass_count: 2,
        binding_count: 2,
        bindings: vec![
            descriptor(
                0,
                0,
                NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(9)),
                1024,
                512,
                NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                    SceneTextureFormat::R8G8B8A8Unorm,
                ),
                10,
            ),
            descriptor(
                0,
                1,
                NativeVulkanSceneTextureDescriptorSource::GraphTarget(SceneGraphTarget::NamedFbo(
                    3,
                )),
                1920,
                1080,
                NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                    NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat,
                ),
                1,
            ),
        ],
        descriptor_model: "VK_EXT_descriptor_heap",
        command_order: [
            "resolve_effect_source_texture_descriptors",
            "resolve_effect_named_fbo_texture_descriptors",
            "resolve_effect_previous_scene_texture_descriptors",
            "bind_descriptor_heap_texture_mapping",
        ],
    };

    let plan = NativeVulkanSceneEffectResourceHeapFramePlan::from_descriptors(
        &descriptors,
        &SceneEffectUniformFramePlan::empty(),
        descriptor_heap_properties(),
        effect_uniform_binding,
        texture_binding,
        target_binding,
    )
    .expect("effect resource heap plan");

    assert_eq!(plan.pass_count, 2);
    assert_eq!(plan.pass_binding_count, 1);
    assert_eq!(plan.heap_slice_count, 1);
    assert_eq!(plan.resource_descriptor_count, 2);
    assert_eq!(plan.sampler_descriptor_count, 2);
    assert!(matches!(
        plan.entries[0].role,
        NativeVulkanSceneEffectResourceHeapEntryRole::WeSampledTexture {
            image_handle: 90,
            ..
        }
    ));
    assert!(matches!(
        plan.entries[1].role,
        NativeVulkanSceneEffectResourceHeapEntryRole::WeSampledTexture {
            image_handle: 300,
            ..
        }
    ));
    assert_eq!(plan.pass_bindings[0].effect_pass_index, 0);
    assert_eq!(plan.pass_bindings[0].effect_uniform, None);
    assert_eq!(plan.pass_bindings[0].base_resource_heap_offset, 0);
    assert_eq!(plan.pass_bindings[0].base_sampler_heap_offset, Some(0));
    assert_eq!(
        plan.pass_bindings[0].shader_mappings,
        vec![
            "we.texture_slot0.g_Texture0 -> effect-heap-slice-offset0".to_owned(),
            "we.texture_slot1.g_Texture1 -> effect-heap-slice-offset1".to_owned(),
        ]
    );
    assert!(matches!(
        plan.bindings[0],
        NativeVulkanSceneEffectResourceHeapDescriptorBinding::SampledImage {
            view,
            sampler,
            ..
        } if view == vk::ImageView::from_raw(91) && sampler == vk::Sampler::from_raw(92)
    ));
    assert!(matches!(
        plan.bindings[1],
        NativeVulkanSceneEffectResourceHeapDescriptorBinding::SampledImage {
            view,
            sampler,
            ..
        } if view == vk::ImageView::from_raw(301) && sampler == vk::Sampler::from_raw(302)
    ));
}

#[test]
fn effect_resource_heap_plan_packs_iris_uniform_before_textures() {
    let descriptors = NativeVulkanSceneEffectTextureDescriptorFramePlan {
        pass_count: 1,
        binding_count: 2,
        bindings: vec![
            descriptor(
                0,
                0,
                NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(9)),
                1024,
                512,
                NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                    SceneTextureFormat::R8G8B8A8Unorm,
                ),
                10,
            ),
            descriptor(
                0,
                1,
                NativeVulkanSceneTextureDescriptorSource::GraphTarget(SceneGraphTarget::NamedFbo(
                    3,
                )),
                1920,
                1080,
                NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                    NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat,
                ),
                1,
            ),
        ],
        descriptor_model: "VK_EXT_descriptor_heap",
        command_order: [
            "resolve_effect_source_texture_descriptors",
            "resolve_effect_named_fbo_texture_descriptors",
            "resolve_effect_previous_scene_texture_descriptors",
            "bind_descriptor_heap_texture_mapping",
        ],
    };
    let uniform_plan = SceneEffectUniformFramePlan {
        effect_pass_count: 1,
        iris_record_count: 1,
        iris_records: vec![iris_uniform_record()],
        command_order: SceneEffectUniformFramePlan::empty().command_order,
    };

    let plan = NativeVulkanSceneEffectResourceHeapFramePlan::from_descriptors(
        &descriptors,
        &uniform_plan,
        descriptor_heap_properties(),
        effect_uniform_binding,
        texture_binding,
        target_binding,
    )
    .expect("effect resource heap plan");

    assert_eq!(plan.resource_descriptor_count, 3);
    assert_eq!(plan.sampler_descriptor_count, 2);
    assert_eq!(
        plan.pass_bindings[0].effect_uniform,
        Some(iris_uniform_key())
    );
    assert_eq!(plan.pass_bindings[0].resource_descriptor_count, 3);
    assert_eq!(plan.pass_bindings[0].texture_count, 2);
    assert_eq!(
        plan.pass_bindings[0].shader_mappings,
        vec![
            "WE effect uniform payload -> effect-heap-slice-offset0".to_owned(),
            "we.texture_slot0.g_Texture0 -> effect-heap-slice-offset1".to_owned(),
            "we.texture_slot1.g_Texture1 -> effect-heap-slice-offset2".to_owned(),
        ]
    );
    assert!(matches!(
        plan.entries[0].role,
        NativeVulkanSceneEffectResourceHeapEntryRole::WeEffectUniformPayload {
            buffer_handle: 0x4200,
            device_address: 0x4280,
            bytes: 64,
            payload_hash: 0x1234,
            ..
        }
    ));
    assert_eq!(plan.entries[0].sampler_descriptor_index, None);
    assert_eq!(plan.entries[1].sampler_descriptor_index, Some(0));
    assert_eq!(plan.entries[2].sampler_descriptor_index, Some(1));
    assert!(matches!(
        plan.bindings[0],
        NativeVulkanSceneEffectResourceHeapDescriptorBinding::UniformBuffer {
            device_address: 0x4280,
            bytes: 64,
            ..
        }
    ));
}

#[test]
fn effect_resource_heap_plan_dedupes_identical_texture_sets() {
    let descriptors = NativeVulkanSceneEffectTextureDescriptorFramePlan {
        pass_count: 2,
        binding_count: 2,
        bindings: vec![
            descriptor(
                0,
                0,
                NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(9)),
                1024,
                512,
                NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                    SceneTextureFormat::R8G8B8A8Unorm,
                ),
                10,
            ),
            descriptor(
                1,
                0,
                NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(9)),
                1024,
                512,
                NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                    SceneTextureFormat::R8G8B8A8Unorm,
                ),
                10,
            ),
        ],
        descriptor_model: "VK_EXT_descriptor_heap",
        command_order: [
            "resolve_effect_source_texture_descriptors",
            "resolve_effect_named_fbo_texture_descriptors",
            "resolve_effect_previous_scene_texture_descriptors",
            "bind_descriptor_heap_texture_mapping",
        ],
    };

    let plan = NativeVulkanSceneEffectResourceHeapFramePlan::from_descriptors(
        &descriptors,
        &SceneEffectUniformFramePlan::empty(),
        descriptor_heap_properties(),
        effect_uniform_binding,
        texture_binding,
        target_binding,
    )
    .expect("effect resource heap plan");

    assert_eq!(plan.pass_binding_count, 2);
    assert_eq!(plan.heap_slice_count, 1);
    assert_eq!(plan.resource_descriptor_count, 1);
    assert_eq!(plan.pass_bindings[0].heap_slice_index, 0);
    assert_eq!(plan.pass_bindings[1].heap_slice_index, 0);
}

#[test]
fn effect_resource_heap_plan_rejects_previous_framebuffer_without_resolver() {
    let descriptors = NativeVulkanSceneEffectTextureDescriptorFramePlan {
        pass_count: 1,
        binding_count: 1,
        bindings: vec![descriptor(
            0,
            0,
            NativeVulkanSceneTextureDescriptorSource::PreviousFramebuffer {
                object: SceneObjectId(7),
                effect_pass_index: 0,
            },
            3840,
            2160,
            NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
            ),
            1,
        )],
        descriptor_model: "VK_EXT_descriptor_heap",
        command_order: [
            "resolve_effect_source_texture_descriptors",
            "resolve_effect_named_fbo_texture_descriptors",
            "resolve_effect_previous_scene_texture_descriptors",
            "bind_descriptor_heap_texture_mapping",
        ],
    };

    let err = NativeVulkanSceneEffectResourceHeapFramePlan::from_descriptors(
        &descriptors,
        &SceneEffectUniformFramePlan::empty(),
        descriptor_heap_properties(),
        effect_uniform_binding,
        texture_binding,
        target_binding,
    )
    .expect_err("previous framebuffer has no resolver");

    assert!(err.contains("cannot resolve external sampled source"));
}

fn iris_uniform_record() -> SceneIrisEffectUniformRecord {
    SceneIrisEffectUniformRecord {
        record_index: 0,
        effect_pass_index: 0,
        object: SceneObjectId(7),
        pass_index: 0,
        shader: "effects/iris".to_owned(),
        time_seconds: 1.0,
        texture_slot_mask: 0b11,
        texture_resolution_slots: vec![1],
        scale: [1.0, 1.0],
        speed: 1.0,
        rough: 0.2,
        noise_amount: 0.5,
        phase_offset: 0.0,
        eye_color: [1.0, 1.0, 1.0],
        mask_combo: 1,
        background_combo: 0,
    }
}

fn iris_uniform_key() -> NativeVulkanSceneEffectUniformKey {
    NativeVulkanSceneEffectUniformKey {
        effect_pass_index: 0,
        object: SceneObjectId(7),
        shader: "effects/iris".to_owned(),
    }
}

fn effect_uniform_binding(
    key: &NativeVulkanSceneEffectUniformKey,
) -> Result<NativeVulkanSceneEffectUniformGpuBufferBinding, String> {
    if key != &iris_uniform_key() {
        return Err(format!("unexpected uniform key {key:?}"));
    }
    Ok(NativeVulkanSceneEffectUniformGpuBufferBinding {
        key: key.clone(),
        buffer: vk::Buffer::from_raw(0x4200),
        device_address: 0x4280,
        record_index: 0,
        bytes: 64,
        payload_hash: 0x1234,
    })
}

fn descriptor(
    effect_pass_index: usize,
    slot: u32,
    source: NativeVulkanSceneTextureDescriptorSource,
    width: u32,
    height: u32,
    format: NativeVulkanSceneTextureDescriptorFormat,
    mip_count: u32,
) -> NativeVulkanSceneEffectTextureDescriptorBinding {
    NativeVulkanSceneEffectTextureDescriptorBinding {
        effect_pass_index,
        object: SceneObjectId(7),
        slot,
        role: SceneGraphResourceRole::shader_texture(slot),
        source,
        width,
        height,
        format,
        mip_count,
        payload_bytes: None,
        shader_mapping: binding_shader_mapping(slot),
    }
}

fn texture_binding(
    resource: SceneResourceId,
) -> Result<NativeVulkanSceneTextureImageBinding, String> {
    if resource != SceneResourceId(9) {
        return Err(format!("unexpected texture {resource:?}"));
    }
    Ok(NativeVulkanSceneTextureImageBinding {
        resource,
        image: vk::Image::from_raw(90),
        view: vk::ImageView::from_raw(91),
        sampler: vk::Sampler::from_raw(92),
        format: vk::Format::R8G8B8A8_UNORM,
        width: 1024,
        height: 512,
        mip_count: 10,
    })
}

fn target_binding(
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
    if target != SceneGraphTarget::NamedFbo(3) {
        return Err(format!("unexpected target {target:?}"));
    }
    Ok(NativeVulkanSceneOffscreenTargetBinding {
        target,
        image: vk::Image::from_raw(300),
        view: vk::ImageView::from_raw(301),
        sampler: vk::Sampler::from_raw(302),
        format: vk::Format::R16G16_SFLOAT,
        width: 1920,
        height: 1080,
        current_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    })
}

fn descriptor_heap_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        resource_heap_alignment: 64,
        sampler_heap_alignment: 32,
        max_resource_heap_size: 4096,
        min_resource_heap_reserved_range: 96,
        max_sampler_heap_size: 4096,
        min_sampler_heap_reserved_range: 48,
        image_descriptor_size: 24,
        image_descriptor_alignment: 32,
        buffer_descriptor_size: 16,
        buffer_descriptor_alignment: 16,
        sampler_descriptor_size: 12,
        sampler_descriptor_alignment: 16,
        ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
    }
}
