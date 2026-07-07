use vulkanalia::vk::{self, Handle};

use super::key::binding_shader_mapping;
use super::*;
use crate::engine::scene_engine::{SceneGraphResourceRole, SceneTextureFormat};
use crate::renderer::native_vulkan::scene_backend::effect_descriptors::NativeVulkanSceneEffectTextureDescriptorFramePlan;
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
        descriptor_heap_properties(),
        texture_binding,
        target_binding,
    )
    .expect("effect resource heap plan");

    assert_eq!(plan.pass_count, 2);
    assert_eq!(plan.pass_binding_count, 1);
    assert_eq!(plan.resource_set_count, 1);
    assert_eq!(plan.resource_descriptor_count, 2);
    assert_eq!(plan.sampler_descriptor_count, 2);
    assert_eq!(plan.entries[0].image_handle, 90);
    assert_eq!(plan.entries[1].image_handle, 300);
    assert_eq!(plan.pass_bindings[0].effect_pass_index, 0);
    assert_eq!(plan.pass_bindings[0].base_resource_heap_offset, 0);
    assert_eq!(plan.pass_bindings[0].base_sampler_heap_offset, 0);
    assert_eq!(
        plan.pass_bindings[0].shader_mappings,
        vec![
            "set0.binding0.g_Texture0 -> effect-resource-set-offset0".to_owned(),
            "set0.binding1.g_Texture1 -> effect-resource-set-offset1".to_owned(),
        ]
    );
    assert_eq!(plan.bindings[0].view, vk::ImageView::from_raw(91));
    assert_eq!(plan.bindings[0].sampler, vk::Sampler::from_raw(92));
    assert_eq!(plan.bindings[1].view, vk::ImageView::from_raw(301));
    assert_eq!(plan.bindings[1].sampler, vk::Sampler::from_raw(302));
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
        descriptor_heap_properties(),
        texture_binding,
        target_binding,
    )
    .expect("effect resource heap plan");

    assert_eq!(plan.pass_binding_count, 2);
    assert_eq!(plan.resource_set_count, 1);
    assert_eq!(plan.resource_descriptor_count, 1);
    assert_eq!(plan.pass_bindings[0].resource_set_index, 0);
    assert_eq!(plan.pass_bindings[1].resource_set_index, 0);
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
        descriptor_heap_properties(),
        texture_binding,
        target_binding,
    )
    .expect_err("previous framebuffer has no resolver");

    assert!(err.contains("cannot resolve external sampled source"));
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
