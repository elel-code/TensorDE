//! Renderer-owned particle state, indirect commands, and frame-time storage.

use std::mem::{size_of, size_of_val};

use super::*;
use crate::engine::scene::{
    SceneParticleGpuEmitterState, SceneParticleIndirectDraw, SceneRenderingDeviceGraphPlan,
};

pub(in super::super) struct SharedSceneParticleResources {
    pub state: Buffer,
    pub indirect: Buffer,
    pub frame_time: Buffer,
    pub emitter_count: u32,
    pub total_capacity: u64,
}

impl SharedSceneParticleResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        storage: &SceneStorage,
        graph: &SceneRenderingDeviceGraphPlan,
    ) -> Result<Option<Self>, String> {
        if graph.particle_gpu_emitters.is_empty() {
            return Ok(None);
        }
        let states = graph
            .particle_gpu_emitters
            .iter()
            .map(|plan| {
                let particle = storage
                    .particles()
                    .get(plan.particle_index as usize)
                    .ok_or_else(|| {
                        format!("particle plan {} has no scene record", plan.particle_index)
                    })?;
                Ok(SceneParticleGpuEmitterState::from_record(
                    particle,
                    plan.capacity,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let indirect_commands = graph
            .particle_gpu_emitters
            .iter()
            .map(|_| SceneParticleIndirectDraw::BILLBOARD)
            .collect::<Vec<_>>();
        let state_payload = typed_bytes(&states);
        let indirect_payload = typed_bytes(&indirect_commands);
        let state = create_device_buffer(
            allocator,
            "gilder-scene-particle-state",
            state_payload.len(),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let indirect = create_device_buffer(
            allocator,
            "gilder-scene-particle-indirect",
            indirect_payload.len(),
            BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let frame_time = create_device_buffer(
            allocator,
            "gilder-scene-particle-frame-time",
            size_of::<f32>(),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        unsafe {
            uploads
                .write_buffer(&state, 0, state_payload)
                .map_err(|error| format!("upload shared particle state: {error}"))?;
            uploads
                .write_buffer(&indirect, 0, indirect_payload)
                .map_err(|error| format!("upload shared particle indirect commands: {error}"))?;
            uploads
                .write_buffer(&frame_time, 0, &0.0f32.to_ne_bytes())
                .map_err(|error| format!("upload shared particle frame time: {error}"))?;
        }
        for (buffer, target) in [
            (&state, BufferState::ComputeStorageReadWrite),
            (&indirect, BufferState::IndirectRead),
            (&frame_time, BufferState::ComputeStorageReadWrite),
        ] {
            uploads
                .encoder_mut()
                .transition_buffer(buffer, BufferState::TransferDestination, target)
                .map_err(|error| format!("transition shared particle resource: {error}"))?;
        }
        Ok(Some(Self {
            state,
            indirect,
            frame_time,
            emitter_count: states.len() as u32,
            total_capacity: graph
                .particle_gpu_emitters
                .iter()
                .map(|plan| u64::from(plan.capacity))
                .sum(),
        }))
    }

    pub(super) fn allocation_bytes(&self) -> u64 {
        self.state
            .allocation_size()
            .saturating_add(self.indirect.allocation_size())
            .saturating_add(self.frame_time.allocation_size())
    }
}

fn typed_bytes<T>(values: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indirect_payload_preserves_one_native_command_per_emitter() {
        let commands = [
            SceneParticleIndirectDraw::BILLBOARD,
            SceneParticleIndirectDraw::with_instance_count(9),
        ];
        assert_eq!(typed_bytes(&commands).len(), 32);
    }
}
