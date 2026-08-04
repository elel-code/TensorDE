//! Renderer-owned particle state, indirect commands, and frame-time storage.

use std::mem::size_of_val;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::engine::scene::{
    SceneParticleGpuEmitterState, SceneParticleGpuParticleState, SceneParticleGpuProfile,
    SceneParticleIndirectDraw, SceneRenderingDeviceGraphPlan,
};

pub(in super::super) struct SharedSceneParticleResources {
    pub state: Buffer,
    pub indirect: Buffer,
    pub frame_time: Buffer,
    pub simulation: Buffer,
    pub random: Buffer,
    pub emitter_count: u32,
    pub max_capacity: u32,
    pub total_capacity: u64,
}

impl SharedSceneParticleResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        queue: &Queue,
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
                    plan.particle_state_offset,
                    plan.profile,
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
        let total_capacity = graph
            .particle_gpu_emitters
            .iter()
            .map(|plan| u64::from(plan.capacity))
            .sum::<u64>();
        let simulation_states = vec![
            SceneParticleGpuParticleState::default();
            usize::try_from(total_capacity.max(1)).map_err(
                |_| "particle state capacity exceeds host address space"
            )?
        ];
        let simulation_payload = typed_bytes(&simulation_states);
        let random_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("particle random seed clock is before Unix epoch: {error}"))?
            .as_millis() as u32;
        let random_states = mt19937_state_payload(states.len(), random_seed);
        let random_payload = typed_bytes(&random_states);
        let state = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-particle-state",
            state_payload.len(),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let indirect = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-particle-indirect",
            indirect_payload.len(),
            BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let frame_state = particle_frame_state(&graph, 0.0, 0.0)?;
        let frame_time = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-particle-frame-time",
            size_of_val(frame_state.as_slice()),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let simulation = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-particle-simulation",
            simulation_payload.len(),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let random = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-particle-random",
            random_payload.len(),
            BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&state, 0, state_payload)
        })
        .map_err(|error| format!("upload shared particle state: {error}"))?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&indirect, 0, indirect_payload)
        })
        .map_err(|error| format!("upload shared particle indirect commands: {error}"))?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&frame_time, 0, typed_bytes(&frame_state))
        })
        .map_err(|error| format!("upload shared particle frame time: {error}"))?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&simulation, 0, simulation_payload)
        })
        .map_err(|error| format!("upload shared particle simulation state: {error}"))?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&random, 0, random_payload)
        })
        .map_err(|error| format!("upload shared particle random state: {error}"))?;
        for (buffer, target) in [
            (&state, BufferState::ComputeStorageReadWrite),
            (&indirect, BufferState::IndirectRead),
            (&frame_time, BufferState::ComputeStorageReadWrite),
            (&simulation, BufferState::StorageReadWrite),
            (&random, BufferState::StorageReadWrite),
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
            simulation,
            random,
            emitter_count: states.len() as u32,
            max_capacity: graph
                .particle_gpu_emitters
                .iter()
                .map(|plan| plan.capacity)
                .max()
                .unwrap_or(0),
            total_capacity,
        }))
    }

    pub(super) fn allocation_bytes(&self) -> u64 {
        self.state
            .allocation_size()
            .saturating_add(self.indirect.allocation_size())
            .saturating_add(self.frame_time.allocation_size())
            .saturating_add(self.simulation.allocation_size())
            .saturating_add(self.random.allocation_size())
    }
}

pub(in super::super) fn particle_frame_state(
    graph: &SceneRenderingDeviceGraphPlan,
    scene_time_seconds: f32,
    frame_delta_seconds: f32,
) -> Result<Vec<f32>, String> {
    let mut state = Vec::with_capacity(2 + graph.particle_gpu_emitters.len());
    state.extend([scene_time_seconds, frame_delta_seconds]);
    for emitter in &graph.particle_gpu_emitters {
        if emitter.profile != SceneParticleGpuProfile::RetainedState {
            state.push(1.0);
            continue;
        }
        let draw = graph
            .mesh_draws
            .iter()
            .find(|draw| draw.particle_index == emitter.particle_index)
            .ok_or_else(|| {
                format!(
                    "retained particle {} has no rendering-device draw",
                    emitter.particle_index
                )
            })?;
        state.push(particle_transform_scale(
            emitter.particle_index,
            &draw.render_world_matrix,
        )?);
    }
    Ok(state)
}

fn particle_transform_scale(
    particle_index: u32,
    render_world_matrix: &[[f32; 4]; 4],
) -> Result<f32, String> {
    let transform_scale = render_world_matrix[0][0];
    if transform_scale.is_finite() && transform_scale >= 0.0 {
        Ok(transform_scale)
    } else {
        Err(format!(
            "retained particle {particle_index} has invalid transform scale {transform_scale}"
        ))
    }
}

fn typed_bytes<T>(values: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) }
}

const MT19937_WORD_COUNT: usize = 624;
const PARTICLE_RANDOM_STATE_WORD_COUNT: usize = MT19937_WORD_COUNT + 4;

fn mt19937_state_payload(emitter_count: usize, seed: u32) -> Vec<u32> {
    let mut payload = Vec::with_capacity(emitter_count * PARTICLE_RANDOM_STATE_WORD_COUNT);
    for _ in 0..emitter_count {
        let start = payload.len();
        payload.push(seed);
        for index in 1..MT19937_WORD_COUNT {
            let previous = payload[start + index - 1];
            payload.push(
                1_812_433_253_u32
                    .wrapping_mul(previous ^ (previous >> 30))
                    .wrapping_add(index as u32),
            );
        }
        payload.extend_from_slice(&[MT19937_WORD_COUNT as u32, 0, f32::NAN.to_bits(), seed]);
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indirect_payload_preserves_one_indirect_command_per_emitter() {
        let commands = [
            SceneParticleIndirectDraw::BILLBOARD,
            SceneParticleIndirectDraw::with_instance_count(9),
        ];
        assert_eq!(typed_bytes(&commands).len(), 32);
    }

    #[test]
    fn particle_random_payload_preserves_independent_mt19937_state_per_emitter() {
        let payload = mt19937_state_payload(2, 5489);
        assert_eq!(payload.len(), 2 * PARTICLE_RANDOM_STATE_WORD_COUNT);
        assert_eq!(payload[0], 5489);
        assert_eq!(payload[1], 1_301_868_182);
        assert_eq!(payload[MT19937_WORD_COUNT], MT19937_WORD_COUNT as u32);
        assert_eq!(payload[MT19937_WORD_COUNT + 1], 0);
        assert!(f32::from_bits(payload[MT19937_WORD_COUNT + 2]).is_nan());
        assert_eq!(payload[PARTICLE_RANDOM_STATE_WORD_COUNT], 5489);
    }

    #[test]
    fn turbulence_scale_uses_the_first_world_matrix_coefficient() {
        let matrix = [
            [0.75, -0.5, 0.0, 120.0],
            [0.5, 0.75, 0.0, 80.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert_eq!(particle_transform_scale(3, &matrix), Ok(0.75));
    }
}
