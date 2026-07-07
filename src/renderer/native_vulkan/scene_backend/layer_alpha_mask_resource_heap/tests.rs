use vulkanalia::vk::{self, Handle};

use super::key::binding_shader_mapping;
use super::*;
use crate::engine::scene_engine::{
    SceneGraphTarget, SceneObjectId, ScenePuppetId, SceneResourceId,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    NativeVulkanSceneLayerAlphaMaskDescriptorSource, NativeVulkanSceneLayerAlphaMaskSlotBinding,
};

#[test]
fn alpha_mask_resource_heap_plan_packs_clipping_and_generated_target_sets() {
    let descriptors = NativeVulkanSceneLayerAlphaMaskDescriptorPlan {
        tokenized_layer_count: 1,
        descriptor_set_count: 2,
        clippingmaskimage4_descriptor_set_count: 1,
        generated_clippingtarget_descriptor_set_count: 1,
        resident_texture_descriptor_count: 3,
        graph_target_descriptor_count: 1,
        entries: vec![clipping_descriptor_set(), generated_target_descriptor_set()],
        command_order: [
            "resolve_tokenized_layer_object_source_texture",
            "resolve_puppet_clipping_record_mask_texture",
            "bind_clippingmaskimage4_slots_0_1",
            "preserve_clippingmaskimage4_optional_morph_slot_5",
            "bind_generated_clippingtarget_slots_0_8",
            "keep_alpha_mask_descriptors_separate_from_genericimage4_material_heap",
        ],
    };

    let plan = NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan::from_descriptors(
        &descriptors,
        descriptor_heap_properties(),
        texture_binding,
        target_binding,
    )
    .expect("alpha-mask resource heap plan");

    assert_eq!(plan.descriptor_set_count, 2);
    assert_eq!(plan.resource_set_count, 2);
    assert_eq!(plan.resource_descriptor_count, 4);
    assert_eq!(plan.sampler_descriptor_count, 4);
    assert_eq!(plan.entries[0].shader, "we/clippingmaskimage4");
    assert_eq!(plan.entries[0].slot, 0);
    assert_eq!(plan.entries[0].image_handle, 90);
    assert_eq!(plan.entries[1].slot, 1);
    assert_eq!(plan.entries[1].image_handle, 120);
    assert_eq!(plan.entries[3].slot, 8);
    assert_eq!(plan.entries[3].image_handle, 600);
    assert_eq!(plan.descriptor_set_bindings[0].descriptor_set_index, 0);
    assert_eq!(plan.descriptor_set_bindings[0].resource_set_index, 0);
    assert_eq!(plan.descriptor_set_bindings[1].descriptor_set_index, 1);
    assert_eq!(plan.descriptor_set_bindings[1].resource_set_index, 1);
    assert_eq!(
        plan.descriptor_set_bindings[1].shader_mappings,
        vec![
            "set0.binding0.g_Texture0 -> alpha-mask-resource-set-offset0".to_owned(),
            "set0.binding8.g_Texture8 -> alpha-mask-resource-set-offset1".to_owned(),
        ]
    );
    assert_eq!(plan.bindings[3].view, vk::ImageView::from_raw(601));
    assert_eq!(plan.bindings[3].sampler, vk::Sampler::from_raw(602));
}

#[test]
fn alpha_mask_resource_heap_plan_dedupes_identical_descriptor_sets() {
    let mut second = clipping_descriptor_set();
    second.role = NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
        clipping_record_index: 1,
    };
    let descriptors = NativeVulkanSceneLayerAlphaMaskDescriptorPlan {
        tokenized_layer_count: 1,
        descriptor_set_count: 2,
        clippingmaskimage4_descriptor_set_count: 2,
        generated_clippingtarget_descriptor_set_count: 0,
        resident_texture_descriptor_count: 4,
        graph_target_descriptor_count: 0,
        entries: vec![clipping_descriptor_set(), second],
        command_order: [
            "resolve_tokenized_layer_object_source_texture",
            "resolve_puppet_clipping_record_mask_texture",
            "bind_clippingmaskimage4_slots_0_1",
            "preserve_clippingmaskimage4_optional_morph_slot_5",
            "bind_generated_clippingtarget_slots_0_8",
            "keep_alpha_mask_descriptors_separate_from_genericimage4_material_heap",
        ],
    };

    let plan = NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan::from_descriptors(
        &descriptors,
        descriptor_heap_properties(),
        texture_binding,
        target_binding,
    )
    .expect("alpha-mask resource heap plan");

    assert_eq!(plan.resource_set_count, 1);
    assert_eq!(plan.resource_descriptor_count, 2);
    assert_eq!(plan.descriptor_set_bindings[0].resource_set_index, 0);
    assert_eq!(plan.descriptor_set_bindings[1].resource_set_index, 0);
    assert_ne!(
        plan.descriptor_set_bindings[0].role,
        plan.descriptor_set_bindings[1].role
    );
}

#[test]
fn alpha_mask_resource_heap_plan_rejects_non_r8_full_mask_target() {
    let descriptors = NativeVulkanSceneLayerAlphaMaskDescriptorPlan {
        tokenized_layer_count: 1,
        descriptor_set_count: 1,
        clippingmaskimage4_descriptor_set_count: 0,
        generated_clippingtarget_descriptor_set_count: 1,
        resident_texture_descriptor_count: 1,
        graph_target_descriptor_count: 1,
        entries: vec![generated_target_descriptor_set()],
        command_order: [
            "resolve_tokenized_layer_object_source_texture",
            "resolve_puppet_clipping_record_mask_texture",
            "bind_clippingmaskimage4_slots_0_1",
            "preserve_clippingmaskimage4_optional_morph_slot_5",
            "bind_generated_clippingtarget_slots_0_8",
            "keep_alpha_mask_descriptors_separate_from_genericimage4_material_heap",
        ],
    };

    let err = NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan::from_descriptors(
        &descriptors,
        descriptor_heap_properties(),
        texture_binding,
        non_r8_target_binding,
    )
    .expect_err("full alpha mask target must be R8");

    assert!(err.contains("must be R8_UNORM"));
}

fn clipping_descriptor_set() -> NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan {
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
            slot(
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            slot(
                1,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    12,
                )),
            ),
        ],
    }
}

fn generated_target_descriptor_set() -> NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan {
    NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan {
        object: SceneObjectId(77),
        puppet: ScenePuppetId(5),
        shader: "we/genericimage4",
        role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget,
        slot_mask: (1 << 0) | (1 << 8),
        optional_morph_slot: None,
        slots: vec![
            slot(
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            slot(
                8,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMask,
                ),
            ),
        ],
    }
}

fn slot(
    slot: u32,
    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
) -> NativeVulkanSceneLayerAlphaMaskSlotBinding {
    NativeVulkanSceneLayerAlphaMaskSlotBinding {
        slot,
        source,
        shader_mapping: binding_shader_mapping(slot),
    }
}

fn texture_binding(
    resource: SceneResourceId,
) -> Result<NativeVulkanSceneTextureImageBinding, String> {
    let image = match resource {
        SceneResourceId(9) => 90,
        SceneResourceId(12) => 120,
        _ => return Err(format!("unexpected texture {resource:?}")),
    };
    let format = if resource == SceneResourceId(12) {
        vk::Format::R8_UNORM
    } else {
        vk::Format::R8G8B8A8_UNORM
    };
    Ok(NativeVulkanSceneTextureImageBinding {
        resource,
        image: vk::Image::from_raw(image),
        view: vk::ImageView::from_raw(image + 1),
        sampler: vk::Sampler::from_raw(image + 2),
        format,
        width: 1024,
        height: 512,
        mip_count: 1,
    })
}

fn target_binding(
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
    if target != SceneGraphTarget::FullAlphaMask {
        return Err(format!("unexpected target {target:?}"));
    }
    Ok(NativeVulkanSceneOffscreenTargetBinding {
        target,
        image: vk::Image::from_raw(600),
        view: vk::ImageView::from_raw(601),
        sampler: vk::Sampler::from_raw(602),
        format: vk::Format::R8_UNORM,
        width: 1920,
        height: 1080,
        current_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    })
}

fn non_r8_target_binding(
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneOffscreenTargetBinding, String> {
    let mut binding = target_binding(target)?;
    binding.format = vk::Format::R8G8B8A8_UNORM;
    Ok(binding)
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
