//! Compiled particle runtime records stored by the `PART` chunk.

use serde::{Deserialize, Serialize};

use super::{SceneMaterialHandle, SceneObjectHandle, SceneResourceId, SceneVec3};

#[cfg(test)]
mod gpu_contract_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleChildType {
    BuiltinDefault,
    Static,
    EventFollow,
    EventSpawn,
    EventDeath,
}

impl SceneParticleChildType {
    pub const fn to_u32(self) -> u32 {
        self as u32
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::BuiltinDefault),
            1 => Some(Self::Static),
            2 => Some(Self::EventFollow),
            3 => Some(Self::EventSpawn),
            4 => Some(Self::EventDeath),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleSimulationKind {
    Unsupported,
    FallingLeaves,
    AmbientSparkles,
    FloralOscillation,
    ModuleSprite,
}

impl SceneParticleSimulationKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Unsupported => 0,
            Self::FallingLeaves => 1,
            Self::AmbientSparkles => 2,
            Self::FloralOscillation => 3,
            Self::ModuleSprite => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Unsupported),
            1 => Some(Self::FallingLeaves),
            2 => Some(Self::AmbientSparkles),
            3 => Some(Self::FloralOscillation),
            4 => Some(Self::ModuleSprite),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleEmitterShape {
    Unsupported,
    SphereRandom,
    BoxRandom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleRendererKind {
    Unsupported,
    Sprite,
    SpriteTrail,
}

impl SceneParticleRendererKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Unsupported => 0,
            Self::Sprite => 1,
            Self::SpriteTrail => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Unsupported),
            1 => Some(Self::Sprite),
            2 => Some(Self::SpriteTrail),
            _ => None,
        }
    }
}

impl SceneParticleEmitterShape {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Unsupported => 0,
            Self::SphereRandom => 1,
            Self::BoxRandom => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Unsupported),
            1 => Some(Self::SphereRandom),
            2 => Some(Self::BoxRandom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneParticleModuleMask(pub u32);

impl SceneParticleModuleMask {
    pub const LIFETIME_RANDOM: u32 = 1 << 0;
    pub const SIZE_RANDOM: u32 = 1 << 1;
    pub const VELOCITY_RANDOM: u32 = 1 << 2;
    pub const COLOR_RANDOM: u32 = 1 << 3;
    pub const ALPHA_RANDOM: u32 = 1 << 4;
    pub const ROTATION_RANDOM: u32 = 1 << 5;
    pub const MOVEMENT: u32 = 1 << 6;
    pub const ALPHA_FADE: u32 = 1 << 7;
    pub const OSCILLATE_POSITION: u32 = 1 << 8;
    pub const TURBULENT_VELOCITY_RANDOM: u32 = 1 << 9;
    pub const TURBULENCE: u32 = 1 << 10;
    pub const SIZE_CHANGE: u32 = 1 << 11;
    pub const VORTEX: u32 = 1 << 12;
    pub const SUPPORTED_BITS: u32 = Self::LIFETIME_RANDOM
        | Self::SIZE_RANDOM
        | Self::VELOCITY_RANDOM
        | Self::COLOR_RANDOM
        | Self::ALPHA_RANDOM
        | Self::ROTATION_RANDOM
        | Self::MOVEMENT
        | Self::ALPHA_FADE
        | Self::OSCILLATE_POSITION
        | Self::TURBULENT_VELOCITY_RANDOM
        | Self::TURBULENCE
        | Self::SIZE_CHANGE
        | Self::VORTEX;

    pub const fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleInitializerKind {
    LifetimeRandom = 1,
    SizeRandom = 2,
    VelocityRandom = 3,
    ColorRandom = 4,
    AlphaRandom = 5,
    RotationRandom = 6,
    TurbulentVelocityRandom = 7,
    AngularVelocityRandom = 8,
    InheritInitialValueFromEvent = 9,
}

impl SceneParticleInitializerKind {
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::LifetimeRandom),
            2 => Some(Self::SizeRandom),
            3 => Some(Self::VelocityRandom),
            4 => Some(Self::ColorRandom),
            5 => Some(Self::AlphaRandom),
            6 => Some(Self::RotationRandom),
            7 => Some(Self::TurbulentVelocityRandom),
            8 => Some(Self::AngularVelocityRandom),
            9 => Some(Self::InheritInitialValueFromEvent),
            _ => None,
        }
    }
}

/// Authored initializer order packed into two exactly representable 16-bit
/// lanes. Each initializer occupies one four-bit nibble.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneParticleInitializerOrder {
    count: u8,
    packed_low: u16,
    packed_high: u16,
}

impl SceneParticleInitializerOrder {
    pub const MAX_COUNT: usize = 8;

    pub fn from_kinds(kinds: &[SceneParticleInitializerKind]) -> Option<Self> {
        if kinds.len() > Self::MAX_COUNT {
            return None;
        }
        let mut packed_low = 0_u16;
        let mut packed_high = 0_u16;
        for (index, kind) in kinds.iter().copied().enumerate() {
            let shift = (index % 4) * 4;
            if index < 4 {
                packed_low |= (kind as u16) << shift;
            } else {
                packed_high |= (kind as u16) << shift;
            }
        }
        Some(Self {
            count: kinds.len() as u8,
            packed_low,
            packed_high,
        })
    }

    pub fn from_packed(count: u32, packed_low: u32, packed_high: u32) -> Option<Self> {
        if count > Self::MAX_COUNT as u32
            || packed_low > u16::MAX as u32
            || packed_high > u16::MAX as u32
        {
            return None;
        }
        let order = Self {
            count: count as u8,
            packed_low: packed_low as u16,
            packed_high: packed_high as u16,
        };
        for index in 0..Self::MAX_COUNT {
            match (index < count as usize, order.raw_kind(index)) {
                (true, 1..=9) | (false, 0) => {}
                _ => return None,
            }
        }
        Some(order)
    }

    pub const fn count(self) -> u8 {
        self.count
    }

    pub const fn packed_low(self) -> u16 {
        self.packed_low
    }

    pub const fn packed_high(self) -> u16 {
        self.packed_high
    }

    pub fn kind_at(self, index: usize) -> Option<SceneParticleInitializerKind> {
        if index >= self.count as usize {
            return None;
        }
        SceneParticleInitializerKind::from_u32(self.raw_kind(index))
    }

    const fn raw_kind(self, index: usize) -> u32 {
        let packed = if index < 4 {
            self.packed_low
        } else {
            self.packed_high
        };
        ((packed >> ((index % 4) * 4)) & 0xf) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneParticleSystemRecord {
    pub object: SceneObjectHandle,
    pub resource: SceneResourceId,
    pub material: SceneMaterialHandle,
    pub parent_particle_index: u32,
    pub child_type: SceneParticleChildType,
    pub child_probability: f32,
    pub child_max_count: u32,
    pub simulation: SceneParticleSimulationKind,
    pub emitter_shape: SceneParticleEmitterShape,
    pub renderer: SceneParticleRendererKind,
    pub module_mask: SceneParticleModuleMask,
    pub initializer_order: SceneParticleInitializerOrder,
    pub flags: u32,
    pub max_count: u32,
    pub sequence_multiplier: f32,
    pub start_time: f32,
    pub instance_time_scale: f32,
    pub instance_count_scale: f32,
    pub rate: f32,
    pub emitter_origin: SceneVec3,
    pub emitter_directions: SceneVec3,
    pub distance_min: SceneVec3,
    pub distance_max: SceneVec3,
    pub emitter_speed_min: f32,
    pub emitter_speed_max: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub velocity_min: SceneVec3,
    pub velocity_max: SceneVec3,
    pub color_min: SceneVec3,
    pub color_max: SceneVec3,
    pub alpha_min: f32,
    pub alpha_max: f32,
    pub rotation_min: f32,
    pub rotation_max: f32,
    pub turbulence_offset: f32,
    pub turbulence_scale: f32,
    pub turbulence_speed_min: f32,
    pub turbulence_speed_max: f32,
    pub turbulent_velocity_phase_min: f32,
    pub turbulent_velocity_phase_max: f32,
    pub turbulent_velocity_time_scale: f32,
    pub turbulent_velocity_right: SceneVec3,
    pub turbulent_velocity_forward: SceneVec3,
    pub turbulence_operator_scale: f32,
    pub turbulence_operator_speed_min: f32,
    pub turbulence_operator_speed_max: f32,
    pub turbulence_operator_phase_min: f32,
    pub turbulence_phase_max: f32,
    pub turbulence_operator_time_scale: f32,
    pub turbulence_mask: SceneVec3,
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
    pub size_change_start_time: f32,
    pub size_change_start_value: f32,
    pub size_change_end_value: f32,
    pub vortex_axis: SceneVec3,
    pub vortex_distance_inner: f32,
    pub vortex_distance_outer: f32,
    pub vortex_speed_inner: f32,
    pub vortex_speed_outer: f32,
    pub trail_length: f32,
    pub trail_min_length: f32,
    pub trail_max_length: f32,
    pub renderer_flags: u32,
}

impl SceneParticleSystemRecord {
    pub fn procedural_instance_capacity(&self) -> u32 {
        let emitted_during_longest_lifetime = (self.rate * self.lifetime_max)
            .ceil()
            .clamp(0.0, u32::MAX as f32) as u32;
        self.max_count.min(emitted_during_longest_lifetime)
    }

    pub fn gpu_profile(&self) -> SceneParticleGpuProfile {
        match self.simulation {
            SceneParticleSimulationKind::ModuleSprite
                if self
                    .module_mask
                    .contains(SceneParticleModuleMask::TURBULENCE) =>
            {
                SceneParticleGpuProfile::RetainedState
            }
            SceneParticleSimulationKind::FallingLeaves
            | SceneParticleSimulationKind::AmbientSparkles
            | SceneParticleSimulationKind::FloralOscillation
            | SceneParticleSimulationKind::ModuleSprite => {
                SceneParticleGpuProfile::AnalyticBillboard
            }
            SceneParticleSimulationKind::Unsupported => SceneParticleGpuProfile::RetainedState,
        }
    }

    // The constructor mirrors the compact particle ABI fields one-for-one.
    #[allow(clippy::too_many_arguments)]
    pub const fn unsupported(
        object: SceneObjectHandle,
        resource: SceneResourceId,
        material: SceneMaterialHandle,
        flags: u32,
        max_count: u32,
        sequence_multiplier: f32,
        start_time: f32,
        instance_time_scale: f32,
    ) -> Self {
        Self {
            object,
            resource,
            material,
            parent_particle_index: super::INVALID_PARTICLE_INDEX,
            child_type: SceneParticleChildType::BuiltinDefault,
            child_probability: 1.0,
            child_max_count: 10,
            simulation: SceneParticleSimulationKind::Unsupported,
            emitter_shape: SceneParticleEmitterShape::Unsupported,
            renderer: SceneParticleRendererKind::Unsupported,
            module_mask: SceneParticleModuleMask(0),
            initializer_order: SceneParticleInitializerOrder {
                count: 0,
                packed_low: 0,
                packed_high: 0,
            },
            flags,
            max_count,
            sequence_multiplier,
            start_time,
            instance_time_scale,
            instance_count_scale: 1.0,
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
            emitter_speed_min: 0.0,
            emitter_speed_max: 0.0,
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
            alpha_min: 1.0,
            alpha_max: 1.0,
            rotation_min: 0.0,
            rotation_max: 0.0,
            turbulence_offset: 0.0,
            turbulence_scale: 0.0,
            turbulence_speed_min: 0.0,
            turbulence_speed_max: 0.0,
            turbulent_velocity_phase_min: 0.0,
            turbulent_velocity_phase_max: 0.0,
            turbulent_velocity_time_scale: 1.0,
            turbulent_velocity_right: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            turbulent_velocity_forward: SceneVec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            turbulence_operator_scale: 0.0,
            turbulence_operator_speed_min: 0.0,
            turbulence_operator_speed_max: 0.0,
            turbulence_operator_phase_min: 0.0,
            turbulence_phase_max: 0.0,
            turbulence_operator_time_scale: 1.0,
            turbulence_mask: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
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
            size_change_start_time: 0.0,
            size_change_start_value: 1.0,
            size_change_end_value: 1.0,
            vortex_axis: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            vortex_distance_inner: 0.0,
            vortex_distance_outer: 0.0,
            vortex_speed_inner: 0.0,
            vortex_speed_outer: 0.0,
            trail_length: 0.0,
            trail_min_length: 0.0,
            trail_max_length: 0.0,
            renderer_flags: 0,
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
    pub time_scale_rate_start_capacity: [f32; 4],
    pub lifetime_min_max_profile_flags: [f32; 4],
    pub emitter_origin: [f32; 4],
    pub emitter_directions: [f32; 4],
    pub distance_min: [f32; 4],
    pub distance_max: [f32; 4],
    pub velocity_min: [f32; 4],
    pub velocity_max: [f32; 4],
    pub gravity: [f32; 4],
    pub size_min_max_fade: [f32; 4],
    pub turbulent_velocity: [f32; 4],
    pub turbulent_velocity_phase_time: [f32; 4],
    pub turbulent_velocity_right: [f32; 4],
    pub turbulent_velocity_forward: [f32; 4],
    pub turbulence_mask_scale: [f32; 4],
    pub turbulence_speed_phase_time: [f32; 4],
    pub color_min_alpha: [f32; 4],
    pub color_max_alpha: [f32; 4],
    pub rotation_min_max: [f32; 4],
}

impl SceneParticleGpuEmitterState {
    pub fn from_record(
        particle: &SceneParticleSystemRecord,
        capacity: u32,
        particle_state_offset: u32,
        profile: SceneParticleGpuProfile,
    ) -> Self {
        Self {
            time_scale_rate_start_capacity: [
                particle.instance_time_scale,
                particle.rate,
                particle.start_time,
                capacity as f32,
            ],
            lifetime_min_max_profile_flags: [
                particle.lifetime_min,
                particle.lifetime_max,
                profile as u32 as f32,
                particle.flags as f32,
            ],
            emitter_origin: [
                particle.emitter_origin.x,
                particle.emitter_origin.y,
                particle.emitter_origin.z,
                particle_state_offset as f32,
            ],
            emitter_directions: [
                particle.emitter_directions.x,
                particle.emitter_directions.y,
                particle.emitter_directions.z,
                particle.emitter_shape.to_u32() as f32,
            ],
            distance_min: [
                particle.distance_min.x,
                particle.distance_min.y,
                particle.distance_min.z,
                particle.module_mask.0 as f32,
            ],
            distance_max: [
                particle.distance_max.x,
                particle.distance_max.y,
                particle.distance_max.z,
                particle.initializer_order.count() as f32,
            ],
            velocity_min: [
                particle.velocity_min.x,
                particle.velocity_min.y,
                particle.velocity_min.z,
                particle.initializer_order.packed_low() as f32,
            ],
            velocity_max: [
                particle.velocity_max.x,
                particle.velocity_max.y,
                particle.velocity_max.z,
                particle.initializer_order.packed_high() as f32,
            ],
            gravity: [
                particle.gravity.x,
                particle.gravity.y,
                particle.gravity.z,
                particle.emitter_speed_min,
            ],
            size_min_max_fade: [
                particle.size_min,
                particle.size_max,
                particle.fade_in_time,
                particle.fade_out_time,
            ],
            turbulent_velocity: [
                particle.turbulence_offset,
                particle.turbulence_scale,
                particle.turbulence_speed_min,
                particle.turbulence_speed_max,
            ],
            turbulent_velocity_phase_time: [
                particle.turbulent_velocity_phase_min,
                particle.turbulent_velocity_phase_max,
                particle.turbulent_velocity_time_scale,
                particle.emitter_speed_max,
            ],
            turbulent_velocity_right: [
                particle.turbulent_velocity_right.x,
                particle.turbulent_velocity_right.y,
                particle.turbulent_velocity_right.z,
                0.0,
            ],
            turbulent_velocity_forward: [
                particle.turbulent_velocity_forward.x,
                particle.turbulent_velocity_forward.y,
                particle.turbulent_velocity_forward.z,
                0.0,
            ],
            turbulence_mask_scale: [
                particle.turbulence_mask.x,
                particle.turbulence_mask.y,
                particle.turbulence_mask.z,
                particle.turbulence_operator_scale,
            ],
            turbulence_speed_phase_time: [
                particle.turbulence_operator_speed_min,
                particle.turbulence_operator_speed_max,
                particle.turbulence_phase_max - particle.turbulence_operator_phase_min,
                particle.turbulence_operator_time_scale,
            ],
            color_min_alpha: [
                particle.color_min.x,
                particle.color_min.y,
                particle.color_min.z,
                particle.alpha_min,
            ],
            color_max_alpha: [
                particle.color_max.x,
                particle.color_max.y,
                particle.color_max.z,
                particle.alpha_max,
            ],
            rotation_min_max: [particle.rotation_min, particle.rotation_max, 0.0, 0.0],
        }
    }
}

/// Retained state for one retained particle slot. The cycle marker is stored as
/// `cycle + 1` so a zero-filled device buffer is unambiguously uninitialized.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct SceneParticleGpuParticleState {
    pub position_birth: [f32; 4],
    pub velocity_stable: [f32; 4],
    pub lifetime_size_alpha_rotation: [f32; 4],
    pub color_last_time: [f32; 4],
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
    pub particle_state_offset: u32,
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
