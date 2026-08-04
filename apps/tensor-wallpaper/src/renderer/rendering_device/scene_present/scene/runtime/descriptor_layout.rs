//! Explicit per-draw descriptor layout for scene image resources.
//!
//! Sampled images and dynamic-rendering input attachments occupy different
//! resource lanes.  They may share a logical Wallpaper Engine slot number,
//! but they never share a descriptor or sampler offset.

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use crate::engine::scene::{
    SceneRenderEffectVisibilityPolicy, SceneRenderingDeviceGraphPlan, SceneShaderStage,
    SceneStorage, SceneStringId,
};
use crate::renderer::rendering_device::scene::rendering_device_scene_shader_for_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct ScenePipelineDescriptorLayout {
    pub sampled_slots: Vec<u32>,
    pub input_attachment_slots: Vec<u32>,
    pub material_uniform_enabled: bool,
    pub skinning_storage_enabled: bool,
    pub particle_storage_enabled: bool,
    pub scene_owned_uniform_count: usize,
}

impl ScenePipelineDescriptorLayout {
    pub(in crate::renderer::rendering_device) fn transform_resource_offset(&self) -> usize {
        0
    }

    pub(in crate::renderer::rendering_device) fn material_resource_offset(&self) -> Option<usize> {
        self.material_uniform_enabled.then_some(1)
    }

    pub(in crate::renderer::rendering_device) fn skinning_resource_offset(&self) -> Option<usize> {
        self.skinning_storage_enabled
            .then_some(1 + usize::from(self.material_uniform_enabled))
    }

    pub(in crate::renderer::rendering_device) fn particle_resource_offset(&self) -> Option<usize> {
        self.particle_storage_enabled.then_some(
            1 + usize::from(self.material_uniform_enabled)
                + usize::from(self.skinning_storage_enabled),
        )
    }

    pub(in crate::renderer::rendering_device) fn sampled_resource_offset(&self) -> usize {
        self.scene_owned_uniform_resource_offset() + self.scene_owned_uniform_count
    }

    pub(in crate::renderer::rendering_device) fn scene_owned_uniform_resource_offset(&self) -> usize {
        1 + usize::from(self.material_uniform_enabled)
            + usize::from(self.skinning_storage_enabled)
            + usize::from(self.particle_storage_enabled)
    }

    pub(in crate::renderer::rendering_device) fn input_attachment_resource_offset(&self) -> usize {
        self.sampled_resource_offset() + self.sampled_slots.len()
    }

    pub(in crate::renderer::rendering_device) fn per_draw_resource_count(&self) -> usize {
        self.input_attachment_resource_offset() + self.input_attachment_slots.len()
    }

    pub(in crate::renderer::rendering_device) fn sampler_count_per_draw(&self) -> usize {
        self.sampled_slots.len()
    }

    pub(in crate::renderer::rendering_device) fn sampled_resource_index(
        &self,
        slot: u32,
    ) -> Option<usize> {
        self.sampled_slots
            .iter()
            .position(|candidate| *candidate == slot)
            .map(|index| self.sampled_resource_offset() + index)
    }

    pub(in crate::renderer::rendering_device) fn input_attachment_resource_index(
        &self,
        slot: u32,
    ) -> Option<usize> {
        self.input_attachment_slots
            .iter()
            .position(|candidate| *candidate == slot)
            .map(|index| self.input_attachment_resource_offset() + index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct ScenePipelineShaderDescriptorAccess {
    pub sampled_slots: Vec<u32>,
    pub input_attachment_slots: Vec<u32>,
}

pub(in crate::renderer::rendering_device) fn scene_pipeline_descriptor_layout(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<ScenePipelineDescriptorLayout, String> {
    let mut shader_ids = BTreeSet::new();
    for pass in graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
    {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        if pass_record.shader_key == SceneStringId::NONE {
            return Err("scene drawable pass has no shader key".to_owned());
        }
        shader_ids.insert(pass_record.shader_key);
    }

    let mut texture_slot_mask = 0u32;
    let mut input_attachment_slot_mask = 0u32;
    let mut material_uniform_enabled = false;
    let mut scene_owned_uniform_count = 0usize;
    for shader_id in shader_ids {
        let shader_key = storage
            .string(shader_id)
            .ok_or_else(|| "scene drawable pass has no shader key".to_owned())?;
        let contract = storage
            .shader_contracts()
            .iter()
            .find(|contract| contract.shader_key == shader_id)
            .ok_or_else(|| format!("scene shader {shader_key:?} has no shader contract"))?;
        texture_slot_mask |= contract.texture_slot_mask;
        input_attachment_slot_mask |= contract.input_attachment_slot_mask;
        let vertex = storage.shader_program(shader_id, SceneShaderStage::Vertex);
        let fragment = storage.shader_program(shader_id, SceneShaderStage::Fragment);
        match (vertex, fragment) {
            (Some(vertex), Some(fragment)) => {
                scene_owned_uniform_count = scene_owned_uniform_count.max(
                    vertex.uniform_buffer_count as usize + fragment.uniform_buffer_count as usize,
                );
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(format!(
                    "scene-owned graphics program {shader_key:?} has an incomplete stage pair"
                ));
            }
            (None, None) => {
                let shader = rendering_device_scene_shader_for_key(shader_key).ok_or_else(|| {
                    format!(
                        "engine-owned scene shader {shader_key:?} is not built into the catalog"
                    )
                })?;
                material_uniform_enabled |= shader.parameter_layout.uses_material_uniform();
            }
        }
    }
    if graph
        .pass_nodes
        .iter()
        .any(|pass| pass.effect_visibility_policy == SceneRenderEffectVisibilityPolicy::Passthrough)
    {
        texture_slot_mask |= 1;
    }
    Ok(ScenePipelineDescriptorLayout {
        sampled_slots: slots_from_mask(texture_slot_mask),
        input_attachment_slots: slots_from_mask(input_attachment_slot_mask),
        material_uniform_enabled,
        skinning_storage_enabled: !graph.puppet_bone_matrices.is_empty(),
        particle_storage_enabled: !graph.particle_gpu_emitters.is_empty(),
        scene_owned_uniform_count,
    })
}

pub(in crate::renderer::rendering_device) fn scene_pipeline_shader_descriptor_access(
    storage: &SceneStorage,
    shader_id: SceneStringId,
) -> Result<ScenePipelineShaderDescriptorAccess, String> {
    let shader_key = storage
        .string(shader_id)
        .ok_or_else(|| "scene shader descriptor access has no shader key".to_owned())?;
    let contract = storage
        .shader_contracts()
        .iter()
        .find(|contract| contract.shader_key == shader_id)
        .ok_or_else(|| format!("scene shader {shader_key:?} has no shader contract"))?;
    Ok(ScenePipelineShaderDescriptorAccess {
        sampled_slots: slots_from_mask(contract.texture_slot_mask),
        input_attachment_slots: slots_from_mask(contract.input_attachment_slot_mask),
    })
}

pub(in crate::renderer::rendering_device) fn scene_passthrough_descriptor_access()
-> ScenePipelineShaderDescriptorAccess {
    ScenePipelineShaderDescriptorAccess {
        sampled_slots: vec![0],
        input_attachment_slots: Vec::new(),
    }
}

fn slots_from_mask(mask: u32) -> Vec<u32> {
    (0..32).filter(|slot| mask & (1u32 << slot) != 0).collect()
}
