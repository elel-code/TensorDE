//! Compiled particle runtime records stored by the `PART` chunk.

use serde::{Deserialize, Serialize};

use super::{SceneMaterialHandle, SceneObjectHandle, SceneResourceId, SceneVec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleSimulationKind {
    Unsupported,
    FallingLeaves,
    AmbientSparkles,
}

impl SceneParticleSimulationKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Unsupported => 0,
            Self::FallingLeaves => 1,
            Self::AmbientSparkles => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Unsupported),
            1 => Some(Self::FallingLeaves),
            2 => Some(Self::AmbientSparkles),
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
        }
    }
}
