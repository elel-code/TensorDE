use crate::core::scene::binary::{
    SceneBinaryEffectPassRecord, SceneBinaryEffectUvTransformRecord, SceneBinaryError,
    SceneBinaryLayoutPlan, SceneBinaryMaterialPassRecord,
};

use super::blend::{
    NativeVulkanSceneBinaryEffectPassState, NativeVulkanSceneBinaryMaterialPassState,
    native_vulkan_scene_binary_effect_pass_state, native_vulkan_scene_binary_material_pass_state,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryRecordRange {
    pub(in crate::renderer::native_vulkan::scene) first_record: u32,
    pub(in crate::renderer::native_vulkan::scene) record_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryTextureSlotRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_name: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_name: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_index: u32,
    pub(in crate::renderer::native_vulkan::scene) slot: u32,
    pub(in crate::renderer::native_vulkan::scene) width: u32,
    pub(in crate::renderer::native_vulkan::scene) height: u32,
    pub(in crate::renderer::native_vulkan::scene) role_flags: u16,
    pub(in crate::renderer::native_vulkan::scene) sampler_flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryMaterialRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) shader_name: u32,
    pub(in crate::renderer::native_vulkan::scene) pipeline_key: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slots: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) effect_passes: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) pass_state:
        NativeVulkanSceneBinaryMaterialPassState,
    pub(in crate::renderer::native_vulkan::scene) material_kind: u16,
    pub(in crate::renderer::native_vulkan::scene) descriptor_layout: u16,
    pub(in crate::renderer::native_vulkan::scene) effect_kind_flags: u32,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryEffectRecord {
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_name: u32,
    pub(in crate::renderer::native_vulkan::scene) shader_name: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_index: u32,
    pub(in crate::renderer::native_vulkan::scene) texture_slots: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) effect_uv_transforms:
        NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) parameters: NativeVulkanSceneBinaryRecordRange,
    pub(in crate::renderer::native_vulkan::scene) pass_state:
        NativeVulkanSceneBinaryEffectPassState,
    pub(in crate::renderer::native_vulkan::scene) kind: u16,
    pub(in crate::renderer::native_vulkan::scene) evaluation_boundary: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryEffectUvTransformRecord
{
    pub(in crate::renderer::native_vulkan::scene) owner_name: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_name: u32,
    pub(in crate::renderer::native_vulkan::scene) pass_index: u32,
    pub(in crate::renderer::native_vulkan::scene) source_slot: u32,
    pub(in crate::renderer::native_vulkan::scene) mask_slot: u32,
    pub(in crate::renderer::native_vulkan::scene) input_width: u32,
    pub(in crate::renderer::native_vulkan::scene) input_height: u32,
    pub(in crate::renderer::native_vulkan::scene) mask_width: u32,
    pub(in crate::renderer::native_vulkan::scene) mask_height: u32,
    pub(in crate::renderer::native_vulkan::scene) backing_width: u32,
    pub(in crate::renderer::native_vulkan::scene) backing_height: u32,
    pub(in crate::renderer::native_vulkan::scene) scale_u: f32,
    pub(in crate::renderer::native_vulkan::scene) scale_v: f32,
    pub(in crate::renderer::native_vulkan::scene) offset_u: f32,
    pub(in crate::renderer::native_vulkan::scene) offset_v: f32,
    pub(in crate::renderer::native_vulkan::scene) mapping: u16,
    pub(in crate::renderer::native_vulkan::scene) flags: u16,
}

pub(super) struct NativeVulkanSceneBinaryMaterialRecords {
    pub(super) texture_slots: Vec<NativeVulkanSceneBinaryTextureSlotRecord>,
    pub(super) materials: Vec<NativeVulkanSceneBinaryMaterialRecord>,
    pub(super) effects: Vec<NativeVulkanSceneBinaryEffectRecord>,
    pub(super) effect_uv_transforms: Vec<NativeVulkanSceneBinaryEffectUvTransformRecord>,
}

pub(super) fn native_vulkan_scene_binary_material_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryMaterialRecords, SceneBinaryError> {
    let texture_slots = native_vulkan_scene_binary_texture_slot_records(container, layout)?;
    let effect_uv_transforms =
        native_vulkan_scene_binary_effect_uv_transform_records(container, layout)?;
    let effects = native_vulkan_scene_binary_effect_records(container, layout)?;
    let material_records = layout.material_pass_records(container)?;
    let mut materials = Vec::with_capacity(material_records.len());
    for material in material_records {
        let material = material?;
        materials.push(native_vulkan_scene_binary_material_record(
            container, layout, material,
        )?);
    }

    Ok(NativeVulkanSceneBinaryMaterialRecords {
        texture_slots,
        materials,
        effects,
        effect_uv_transforms,
    })
}

fn native_vulkan_scene_binary_texture_slot_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryTextureSlotRecord>, SceneBinaryError> {
    let texture_slot_records = layout.texture_slot_records(container)?;
    let mut texture_slots = Vec::with_capacity(texture_slot_records.len());
    for texture_slot in texture_slot_records {
        let texture_slot = texture_slot?;
        texture_slots.push(NativeVulkanSceneBinaryTextureSlotRecord {
            owner_name: texture_slot.owner_name,
            pass_name: texture_slot.pass_name,
            texture_name: texture_slot.texture_name,
            resource_index: texture_slot.resource_index,
            slot: texture_slot.slot,
            width: texture_slot.width,
            height: texture_slot.height,
            role_flags: texture_slot.role_flags,
            sampler_flags: texture_slot.sampler_flags,
        });
    }
    Ok(texture_slots)
}

fn native_vulkan_scene_binary_material_record(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
    material: SceneBinaryMaterialPassRecord,
) -> Result<NativeVulkanSceneBinaryMaterialRecord, SceneBinaryError> {
    let texture_slots = material_record_range(
        material.first_texture_slot,
        layout.material_texture_slot_records(container, material)?,
    );
    let effect_passes = material_record_range(
        material.first_effect_pass,
        layout.material_effect_pass_records(container, material)?,
    );

    Ok(NativeVulkanSceneBinaryMaterialRecord {
        owner_name: material.owner_name,
        shader_name: material.shader_name,
        pipeline_key: material.pipeline_key,
        texture_slots,
        effect_passes,
        pass_state: native_vulkan_scene_binary_material_pass_state(material),
        material_kind: material.material_kind,
        descriptor_layout: material.descriptor_layout,
        effect_kind_flags: material.effect_kind_flags,
        flags: material.flags,
    })
}

fn native_vulkan_scene_binary_effect_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryEffectRecord>, SceneBinaryError> {
    let effect_pass_records = layout.effect_pass_records(container)?;
    let mut effects = Vec::with_capacity(effect_pass_records.len());
    for effect_pass in effect_pass_records {
        let effect_pass = effect_pass?;
        effects.push(native_vulkan_scene_binary_effect_record(
            container,
            layout,
            effect_pass,
        )?);
    }
    Ok(effects)
}

fn native_vulkan_scene_binary_effect_uv_transform_records(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<Vec<NativeVulkanSceneBinaryEffectUvTransformRecord>, SceneBinaryError> {
    let records = layout.effect_uv_transform_records(container)?;
    let mut transforms = Vec::with_capacity(records.len());
    for record in records {
        transforms.push(native_vulkan_scene_binary_effect_uv_transform_record(
            record?,
        ));
    }
    Ok(transforms)
}

fn native_vulkan_scene_binary_effect_uv_transform_record(
    record: SceneBinaryEffectUvTransformRecord,
) -> NativeVulkanSceneBinaryEffectUvTransformRecord {
    NativeVulkanSceneBinaryEffectUvTransformRecord {
        owner_name: record.owner_name,
        effect_name: record.effect_name,
        pass_index: record.pass_index,
        source_slot: record.source_slot,
        mask_slot: record.mask_slot,
        input_width: record.input_width,
        input_height: record.input_height,
        mask_width: record.mask_width,
        mask_height: record.mask_height,
        backing_width: record.backing_width,
        backing_height: record.backing_height,
        scale_u: record.scale_u,
        scale_v: record.scale_v,
        offset_u: record.offset_u,
        offset_v: record.offset_v,
        mapping: record.mapping,
        flags: record.flags,
    }
}

fn native_vulkan_scene_binary_effect_record(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
    effect_pass: SceneBinaryEffectPassRecord,
) -> Result<NativeVulkanSceneBinaryEffectRecord, SceneBinaryError> {
    let texture_slots = material_record_range(
        effect_pass.first_texture_slot,
        layout.effect_texture_slot_records(container, effect_pass)?,
    );
    let effect_uv_transforms = material_record_range(
        effect_pass.first_effect_uv_transform,
        layout.effect_uv_transform_record_range(container, effect_pass)?,
    );
    let parameters = material_record_range(
        effect_pass.first_parameter,
        layout.effect_parameter_record_range(container, effect_pass)?,
    );

    Ok(NativeVulkanSceneBinaryEffectRecord {
        owner_name: effect_pass.owner_name,
        effect_name: effect_pass.effect_name,
        shader_name: effect_pass.shader_name,
        pass_index: effect_pass.pass_index,
        texture_slots,
        effect_uv_transforms,
        parameters,
        pass_state: native_vulkan_scene_binary_effect_pass_state(effect_pass),
        kind: effect_pass.kind,
        evaluation_boundary: effect_pass.evaluation_boundary,
        flags: effect_pass.flags,
    })
}

fn material_record_range<T>(
    first_record: u32,
    records: impl ExactSizeIterator<Item = Result<T, SceneBinaryError>>,
) -> NativeVulkanSceneBinaryRecordRange {
    NativeVulkanSceneBinaryRecordRange {
        first_record,
        record_count: records.len().min(u32::MAX as usize) as u32,
    }
}
