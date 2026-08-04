//! Particle compute pipeline backed by global descriptor-heap storage bindings.

use vulkan_renderer::{
    Backend, ComputePipelineDescriptor, MachineCodeComputePipeline,
    MachineCodeComputePipelineDescriptor, PipelineBinaryArchiveCache, ProgrammableStage,
    ShaderBindingMap, ShaderModuleDescriptor,
};

use crate::engine::scene::SceneRenderingDeviceGraphPlan;
use crate::renderer::rendering_device::scene::{
    BuiltinSceneDescriptorBindingKind, rendering_device_particle_compute_shader,
};
const PARTICLE_DESCRIPTOR_COUNT: usize = 5;
const PARTICLE_PUSH_BYTES: usize = PARTICLE_DESCRIPTOR_COUNT * 4;

pub(in crate::renderer::rendering_device) struct SceneParticleComputePipeline {
    pipeline: MachineCodeComputePipeline,
    descriptor_push: [u8; PARTICLE_PUSH_BYTES],
    machine_code_binary_count: usize,
    machine_code_bytes: usize,
    archive_reused: bool,
}

impl SceneParticleComputePipeline {
    pub(in crate::renderer::rendering_device) fn pipeline(&self) -> &MachineCodeComputePipeline {
        &self.pipeline
    }

    pub(in crate::renderer::rendering_device) fn descriptor_push(&self) -> &[u8] {
        &self.descriptor_push
    }

    pub(super) fn machine_code_metrics(&self) -> (usize, usize, bool) {
        (
            self.machine_code_binary_count,
            self.machine_code_bytes,
            self.archive_reused,
        )
    }
}

pub(super) fn create_optional_particle_compute_pipeline(
    device: &Backend,
    graph: &SceneRenderingDeviceGraphPlan,
    resource_descriptor_kinds: &[vulkan_renderer::DescriptorSlotKind],
    descriptor_base: Option<usize>,
    pipeline_binary_cache: &PipelineBinaryArchiveCache,
) -> Result<Option<SceneParticleComputePipeline>, String> {
    if graph.particle_gpu_emitters.is_empty() {
        if descriptor_base.is_some() {
            return Err("particle descriptor base exists without GPU emitters".to_owned());
        }
        return Ok(None);
    }
    let descriptor_base = descriptor_base
        .ok_or_else(|| "particle compute requires a retained descriptor base".to_owned())?;
    let descriptor_push = particle_descriptor_push(resource_descriptor_kinds, descriptor_base)?;
    create_particle_compute_pipeline(device, pipeline_binary_cache).map(|prepared| {
        let machine_code_binary_count = prepared.archive().binaries.len();
        let machine_code_bytes = prepared
            .archive()
            .binaries
            .iter()
            .map(|binary| binary.data.len())
            .sum();
        let archive_reused = prepared.archive_reused();
        Some(SceneParticleComputePipeline {
            pipeline: prepared,
            descriptor_push,
            machine_code_binary_count,
            machine_code_bytes,
            archive_reused,
        })
    })
}

pub(super) fn destroy_optional_particle_compute_pipeline(
    pipeline: Option<SceneParticleComputePipeline>,
) {
    drop(pipeline);
}

fn create_particle_compute_pipeline(
    device: &Backend,
    pipeline_binary_cache: &PipelineBinaryArchiveCache,
) -> Result<MachineCodeComputePipeline, String> {
    let shader = rendering_device_particle_compute_shader();
    let module = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("tensor-wallpaper-particle-compute".into()),
            spirv: shader.spirv.to_vec(),
        })
        .map_err(|error| format!("create shared particle compute shader module: {error}"))?;
    let bindings = ShaderBindingMap::default();
    device
        .create_machine_code_compute_pipeline(&MachineCodeComputePipelineDescriptor {
            pipeline: ComputePipelineDescriptor {
                label: Some("tensor-wallpaper-particle-compute"),
                stage: ProgrammableStage {
                    module: &module,
                    entry_point: c"main",
                    bindings: &bindings,
                },
                cache: None,
            },
            archive_cache: pipeline_binary_cache,
        })
        .map_err(|error| format!("create shared particle compute pipeline: {error}"))
}

fn particle_descriptor_push(
    resource_descriptor_kinds: &[vulkan_renderer::DescriptorSlotKind],
    descriptor_base: usize,
) -> Result<[u8; PARTICLE_PUSH_BYTES], String> {
    let shader = rendering_device_particle_compute_shader();
    if shader.push_constant_bytes as usize != PARTICLE_PUSH_BYTES
        || shader.bindings.len() != PARTICLE_DESCRIPTOR_COUNT
    {
        return Err(format!(
            "particle compute shader requires {} push bytes and {} bindings, found {} and {}",
            PARTICLE_PUSH_BYTES,
            PARTICLE_DESCRIPTOR_COUNT,
            shader.push_constant_bytes,
            shader.bindings.len()
        ));
    }
    let mut bytes = [0; PARTICLE_PUSH_BYTES];
    for binding in shader.bindings {
        if binding.kind != BuiltinSceneDescriptorBindingKind::StorageBuffer
            || binding.register as usize >= PARTICLE_DESCRIPTOR_COUNT
            || binding.push_offset != binding.register * 4
        {
            return Err(format!(
                "particle compute binding {:?} register {} has invalid descriptor push offset {}",
                binding.kind, binding.register, binding.push_offset
            ));
        }
        let descriptor = descriptor_base + binding.register as usize;
        let kind = resource_descriptor_kinds
            .get(descriptor)
            .copied()
            .ok_or_else(|| format!("particle compute descriptor {descriptor} is missing"))?;
        if kind != vulkan_renderer::DescriptorSlotKind::StorageBuffer {
            return Err(format!(
                "particle compute descriptor {descriptor} has kind {kind:?}, expected StorageBuffer"
            ));
        }
        let element = u32::try_from(descriptor)
            .map_err(|_| "particle descriptor heap index exceeds u32".to_owned())?;
        let push_offset = binding.push_offset as usize;
        bytes[push_offset..push_offset + 4].copy_from_slice(&element.to_le_bytes());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_push_words_address_five_retained_particle_descriptors() {
        let kinds = [
            vulkan_renderer::DescriptorSlotKind::UniformBuffer,
            vulkan_renderer::DescriptorSlotKind::StorageBuffer,
            vulkan_renderer::DescriptorSlotKind::StorageBuffer,
            vulkan_renderer::DescriptorSlotKind::StorageBuffer,
            vulkan_renderer::DescriptorSlotKind::StorageBuffer,
            vulkan_renderer::DescriptorSlotKind::StorageBuffer,
        ];
        let bytes = particle_descriptor_push(&kinds, 1).unwrap();
        let words = bytes
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words, vec![1, 2, 3, 4, 5]);
    }
}
