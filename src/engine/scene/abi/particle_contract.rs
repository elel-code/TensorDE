//! Compiled particle runtime records stored by the `PART` chunk.

use serde::{Deserialize, Serialize};

use super::{SceneMaterialHandle, SceneObjectHandle, SceneResourceId, SceneVec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleSimulationKind {
    Unsupported,
    FallingLeaves,
    AmbientSparkles,
    FloralOscillation,
}

impl SceneParticleSimulationKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Unsupported => 0,
            Self::FallingLeaves => 1,
            Self::AmbientSparkles => 2,
            Self::FloralOscillation => 3,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Unsupported),
            1 => Some(Self::FallingLeaves),
            2 => Some(Self::AmbientSparkles),
            3 => Some(Self::FloralOscillation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneParticleSystemRecord {
    pub object: SceneObjectHandle,
    pub resource: SceneResourceId,
    pub material: SceneMaterialHandle,
    pub simulation: SceneParticleSimulationKind,
    pub flags: u32,
    pub max_count: u32,
    pub sequence_multiplier: f32,
    pub start_time: f32,
    pub rate: f32,
    pub emitter_origin: SceneVec3,
    pub emitter_directions: SceneVec3,
    pub distance_min: SceneVec3,
    pub distance_max: SceneVec3,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub velocity_min: SceneVec3,
    pub velocity_max: SceneVec3,
    pub color_min: SceneVec3,
    pub color_max: SceneVec3,
    pub rotation_min: f32,
    pub rotation_max: f32,
    pub turbulence_offset: f32,
    pub turbulence_scale: f32,
    pub turbulence_speed_min: f32,
    pub turbulence_speed_max: f32,
    pub angular_velocity_min: SceneVec3,
    pub angular_velocity_max: SceneVec3,
    pub gravity: SceneVec3,
    pub fade_in_time: f32,
    pub fade_out_time: f32,
    pub oscillation_frequency_min: f32,
    pub oscillation_frequency_max: f32,
    pub oscillation_phase_min: f32,
    pub oscillation_phase_max: f32,
    pub oscillation_scale_min: f32,
    pub oscillation_scale_max: f32,
    pub position_oscillation_frequency_min: f32,
    pub position_oscillation_frequency_max: f32,
    pub position_oscillation_phase_min: f32,
    pub position_oscillation_phase_max: f32,
    pub position_oscillation_scale_min: f32,
    pub position_oscillation_scale_max: f32,
    pub position_oscillation_mask: SceneVec3,
    pub size_oscillation_frequency_min: f32,
    pub size_oscillation_frequency_max: f32,
    pub size_oscillation_phase_min: f32,
    pub size_oscillation_phase_max: f32,
    pub size_oscillation_scale_min: f32,
    pub size_oscillation_scale_max: f32,
}

impl SceneParticleSystemRecord {
    pub const fn unsupported(
        object: SceneObjectHandle,
        resource: SceneResourceId,
        material: SceneMaterialHandle,
        flags: u32,
        max_count: u32,
        sequence_multiplier: f32,
        start_time: f32,
    ) -> Self {
        Self {
            object,
            resource,
            material,
            simulation: SceneParticleSimulationKind::Unsupported,
            flags,
            max_count,
            sequence_multiplier,
            start_time,
            rate: 0.0,
            emitter_origin: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            emitter_directions: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            distance_min: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            distance_max: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            lifetime_min: 0.0,
            lifetime_max: 0.0,
            size_min: 0.0,
            size_max: 0.0,
            velocity_min: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            velocity_max: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            color_min: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            color_max: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            rotation_min: 0.0,
            rotation_max: 0.0,
            turbulence_offset: 0.0,
            turbulence_scale: 0.0,
            turbulence_speed_min: 0.0,
            turbulence_speed_max: 0.0,
            angular_velocity_min: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            angular_velocity_max: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            gravity: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            fade_in_time: 0.0,
            fade_out_time: 1.0,
            oscillation_frequency_min: 0.0,
            oscillation_frequency_max: 0.0,
            oscillation_phase_min: 0.0,
            oscillation_phase_max: std::f32::consts::TAU,
            oscillation_scale_min: 1.0,
            oscillation_scale_max: 1.0,
            position_oscillation_frequency_min: 0.0,
            position_oscillation_frequency_max: 0.0,
            position_oscillation_phase_min: 0.0,
            position_oscillation_phase_max: std::f32::consts::TAU,
            position_oscillation_scale_min: 0.0,
            position_oscillation_scale_max: 0.0,
            position_oscillation_mask: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            },
            size_oscillation_frequency_min: 0.0,
            size_oscillation_frequency_max: 0.0,
            size_oscillation_phase_min: 0.0,
            size_oscillation_phase_max: std::f32::consts::TAU,
            size_oscillation_scale_min: 1.0,
            size_oscillation_scale_max: 1.0,
        }
    }
}

/// GPU-facing profile selected after the semantic particle record is resolved.
/// The value is intentionally separate from `SceneParticleSimulationKind`: the
/// latter is a WE/content concept, while this one is a shader/compute contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum SceneParticleGpuProfile {
    AnalyticBillboard = 0,
    RetainedState = 1,
}

/// std430-compatible per-emitter state. Every vector is a full vec4 slot so the
/// same layout can be consumed by Vulkan GLSL and Rust without implicit padding.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct SceneParticleGpuEmitterState {
    pub time_rate_start_capacity: [f32; 4],
    pub lifetime_min_max_profile_flags: [f32; 4],
    pub emitter_origin: [f32; 4],
    pub emitter_directions: [f32; 4],
    pub velocity_min: [f32; 4],
    pub velocity_max: [f32; 4],
    pub gravity: [f32; 4],
    pub size_min_max_fade: [f32; 4],
}

impl SceneParticleGpuEmitterState {
    pub fn from_record(
        particle: &SceneParticleSystemRecord,
        scene_time_seconds: f32,
        capacity: u32,
    ) -> Self {
        Self {
            time_rate_start_capacity: [
                scene_time_seconds,
                particle.rate,
                particle.start_time,
                capacity as f32,
            ],
            lifetime_min_max_profile_flags: [
                particle.lifetime_min,
                particle.lifetime_max,
                particle.simulation.to_u32() as f32,
                particle.flags as f32,
            ],
            emitter_origin: [
                particle.emitter_origin.x,
                particle.emitter_origin.y,
                particle.emitter_origin.z,
                0.0,
            ],
            emitter_directions: [
                particle.emitter_directions.x,
                particle.emitter_directions.y,
                particle.emitter_directions.z,
                0.0,
            ],
            velocity_min: [
                particle.velocity_min.x,
                particle.velocity_min.y,
                particle.velocity_min.z,
                0.0,
            ],
            velocity_max: [
                particle.velocity_max.x,
                particle.velocity_max.y,
                particle.velocity_max.z,
                0.0,
            ],
            gravity: [
                particle.gravity.x,
                particle.gravity.y,
                particle.gravity.z,
                0.0,
            ],
            size_min_max_fade: [
                particle.size_min,
                particle.size_max,
                particle.fade_in_time,
                particle.fade_out_time,
            ],
        }
    }
}

/// Vulkan `VkDrawIndirectCommand` layout. This is kept in the scene ABI so the
/// graph can describe an indirect draw without depending on Vulkan types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SceneParticleIndirectDraw {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneParticleGpuEmitterPlan {
    pub object: SceneObjectHandle,
    pub particle_index: u32,
    pub profile: SceneParticleGpuProfile,
    pub state_index: u32,
    pub capacity: u32,
    pub indirect_draw_index: u32,
}

impl SceneParticleIndirectDraw {
    pub const BILLBOARD: Self = Self {
        vertex_count: 4,
        instance_count: 0,
        first_vertex: 0,
        first_instance: 0,
    };

    pub const fn with_instance_count(instance_count: u32) -> Self {
        Self {
            instance_count,
            ..Self::BILLBOARD
        }
    }
}

#[cfg(test)]
mod gpu_contract_tests {
    use super::*;

    #[test]
    fn gpu_emitter_state_uses_vec4_slots_without_hidden_padding() {
        assert_eq!(std::mem::size_of::<SceneParticleGpuEmitterState>(), 128);
        assert_eq!(std::mem::align_of::<SceneParticleGpuEmitterState>(), 4);
    }

    #[test]
    fn indirect_billboard_command_matches_vulkan_layout() {
        assert_eq!(std::mem::size_of::<SceneParticleIndirectDraw>(), 16);
        assert_eq!(
            SceneParticleIndirectDraw::with_instance_count(37).instance_count,
            37
        );
    }

    #[test]
    fn gpu_state_preserves_semantic_particle_values() {
        let mut particle = SceneParticleSystemRecord::unsupported(
            SceneObjectHandle(0),
            SceneResourceId(0),
            SceneMaterialHandle(0),
            9,
            100,
            1.0,
            2.0,
        );
        particle.rate = 12.0;
        particle.gravity = SceneVec3 {
            x: 1.0,
            y: -2.0,
            z: 3.0,
        };
        let state = SceneParticleGpuEmitterState::from_record(&particle, 4.0, 80);
        assert_eq!(state.time_rate_start_capacity, [4.0, 12.0, 2.0, 80.0]);
        assert_eq!(state.gravity, [1.0, -2.0, 3.0, 0.0]);
    }
}
