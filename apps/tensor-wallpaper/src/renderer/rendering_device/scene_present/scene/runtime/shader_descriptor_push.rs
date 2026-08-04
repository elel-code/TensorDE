//! Generic descriptor-heap push words for scene-owned shader stages.

use crate::engine::scene::SceneShaderBindingKind;

use super::shader_program::SceneOwnedStageResourcePlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneOwnedDescriptorHeapIndex {
    pub kind: SceneShaderBindingKind,
    pub register: u32,
    pub element_index: u32,
}

pub(super) fn write_scene_owned_descriptor_push(
    plan: &SceneOwnedStageResourcePlan<'_>,
    indices: &[SceneOwnedDescriptorHeapIndex],
    output: &mut [u8],
) -> Result<(), String> {
    if output.len() < plan.push_constant_bytes as usize {
        return Err(format!(
            "scene-owned {:?} push ABI requires at least {} bytes, received {}",
            plan.stage,
            plan.push_constant_bytes,
            output.len()
        ));
    }
    if indices.len() != plan.bindings.len() {
        return Err(format!(
            "scene-owned {:?} push ABI requires {} descriptor indices, received {}",
            plan.stage,
            plan.bindings.len(),
            indices.len()
        ));
    }
    for binding in &plan.bindings {
        let mut matches = indices
            .iter()
            .filter(|index| index.kind == binding.kind && index.register == binding.register);
        let index = matches.next().ok_or_else(|| {
            format!(
                "scene-owned {:?} push ABI is missing {:?} register {}",
                plan.stage, binding.kind, binding.register
            )
        })?;
        if matches.next().is_some() {
            return Err(format!(
                "scene-owned {:?} push ABI repeats {:?} register {}",
                plan.stage, binding.kind, binding.register
            ));
        }
        let start = binding.push_offset as usize;
        let end = start
            .checked_add(size_of::<u32>())
            .ok_or_else(|| "scene-owned descriptor push offset overflows".to_owned())?;
        let destination = output.get_mut(start..end).ok_or_else(|| {
            format!(
                "scene-owned {:?} descriptor push offset {} is outside its ABI",
                plan.stage, binding.push_offset
            )
        })?;
        destination.copy_from_slice(&index.element_index.to_le_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneShaderBindingKind, SceneShaderStage};
    use crate::renderer::rendering_device::scene_present::scene::runtime::shader_program::{
        SceneOwnedDescriptorBindingPlan, SceneOwnedStageResourcePlan,
    };

    #[test]
    fn rounded_mask_fragment_push_follows_typed_offsets() {
        let plan = rounded_mask_fragment_plan();
        let indices = [
            index(SceneShaderBindingKind::SampledImage, 0, 7),
            index(SceneShaderBindingKind::Sampler, 0, 3),
            index(SceneShaderBindingKind::UniformBuffer, 0, 11),
        ];
        let mut bytes = [0u8; 16];

        write_scene_owned_descriptor_push(&plan, &indices, &mut bytes).expect("typed push");

        assert_eq!(word(&bytes, 4), 7);
        assert_eq!(word(&bytes, 8), 3);
        assert_eq!(word(&bytes, 12), 11);
    }

    #[test]
    fn missing_or_duplicate_binding_index_fails_strictly() {
        let plan = rounded_mask_fragment_plan();
        let missing = [
            index(SceneShaderBindingKind::SampledImage, 0, 7),
            index(SceneShaderBindingKind::Sampler, 0, 3),
        ];
        assert!(write_scene_owned_descriptor_push(&plan, &missing, &mut [0u8; 16]).is_err());
        let duplicate = [
            index(SceneShaderBindingKind::SampledImage, 0, 7),
            index(SceneShaderBindingKind::Sampler, 0, 3),
            index(SceneShaderBindingKind::Sampler, 0, 4),
        ];
        assert!(write_scene_owned_descriptor_push(&plan, &duplicate, &mut [0u8; 16]).is_err());
    }

    fn rounded_mask_fragment_plan() -> SceneOwnedStageResourcePlan<'static> {
        SceneOwnedStageResourcePlan {
            stage: SceneShaderStage::Fragment,
            push_constant_bytes: 16,
            bindings: vec![
                binding(SceneShaderBindingKind::SampledImage, 0, 4),
                binding(SceneShaderBindingKind::Sampler, 0, 8),
                binding(SceneShaderBindingKind::UniformBuffer, 0, 12),
            ],
            uniform_buffers: Vec::new(),
        }
    }

    fn binding(
        kind: SceneShaderBindingKind,
        register: u32,
        push_offset: u32,
    ) -> SceneOwnedDescriptorBindingPlan {
        SceneOwnedDescriptorBindingPlan {
            kind,
            register,
            descriptor_count: 1,
            push_offset,
        }
    }

    fn index(
        kind: SceneShaderBindingKind,
        register: u32,
        element_index: u32,
    ) -> SceneOwnedDescriptorHeapIndex {
        SceneOwnedDescriptorHeapIndex {
            kind,
            register,
            element_index,
        }
    }

    fn word(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("push word"))
    }
}
