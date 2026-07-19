//! Typed Wallpaper Engine particle-system ingest records.
//!
//! The converter retains authored emitter/initializer/operator/renderer semantics here. Runtime
//! specialization happens only when a complete, known module profile matches.

use serde::{Deserialize, Serialize};

use crate::engine::scene::abi::SceneVec3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrParticleSystem {
    pub object: u32,
    pub resource: u32,
    pub material: u32,
    pub flags: u32,
    pub max_count: u32,
    pub sequence_multiplier: f32,
    pub start_time: f32,
    pub instance_time_scale: f32,
    pub control_points: Vec<WeIrParticleControlPoint>,
    pub emitters: Vec<WeIrParticleEmitter>,
    pub initializers: Vec<WeIrParticleInitializer>,
    pub operators: Vec<WeIrParticleOperator>,
    pub renderers: Vec<WeIrParticleRenderer>,
    pub children: Vec<WeIrParticleChild>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeIrParticleControlPoint {
    pub id: u32,
    pub origin: SceneVec3,
    pub angles: SceneVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WeIrParticleEmitter {
    SphereRandom {
        id: u32,
        rate: f32,
        instantaneous: u32,
        origin: SceneVec3,
        directions: SceneVec3,
        distance_min: SceneVec3,
        distance_max: SceneVec3,
    },
    BoxRandom {
        id: u32,
        rate: f32,
        origin: SceneVec3,
        directions: SceneVec3,
        distance_min: SceneVec3,
        distance_max: SceneVec3,
    },
    Unsupported {
        id: u32,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WeIrParticleInitializer {
    LifetimeRandom {
        id: u32,
        min: f32,
        max: f32,
    },
    SizeRandom {
        id: u32,
        min: f32,
        max: f32,
    },
    VelocityRandom {
        id: u32,
        min: SceneVec3,
        max: SceneVec3,
    },
    ColorRandom {
        id: u32,
        min: SceneVec3,
        max: SceneVec3,
    },
    AlphaRandom {
        id: u32,
        min: f32,
        max: f32,
    },
    RotationRandom {
        id: u32,
        min: f32,
        max: f32,
    },
    TurbulentVelocityRandom {
        id: u32,
        offset: f32,
        scale: f32,
        speed_min: f32,
        speed_max: f32,
    },
    AngularVelocityRandom {
        id: u32,
        min: SceneVec3,
        max: SceneVec3,
    },
    InheritInitialValueFromEvent {
        id: u32,
        input: String,
    },
    Unsupported {
        id: u32,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WeIrParticleOperator {
    Movement {
        id: u32,
        gravity: SceneVec3,
    },
    AlphaFade {
        id: u32,
        fade_in_time: f32,
        fade_out_time: f32,
    },
    AngularMovement {
        id: u32,
    },
    OscillateAlpha {
        id: u32,
        frequency_min: f32,
        frequency_max: f32,
        phase_min: f32,
        phase_max: f32,
        scale_min: f32,
        scale_max: f32,
    },
    OscillatePosition {
        id: u32,
        frequency_min: f32,
        frequency_max: f32,
        phase_min: f32,
        phase_max: f32,
        scale_min: f32,
        scale_max: f32,
        mask: SceneVec3,
    },
    OscillateSize {
        id: u32,
        frequency_min: f32,
        frequency_max: f32,
        phase_min: f32,
        phase_max: f32,
        scale_min: f32,
        scale_max: f32,
    },
    MaintainDistanceToControlPoint {
        id: u32,
        control_point: u32,
        distance: f32,
        variable_strength: f32,
    },
    ControlPointAttract {
        id: u32,
        control_point: u32,
        origin: SceneVec3,
        scale: f32,
        threshold: f32,
    },
    Unsupported {
        id: u32,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WeIrParticleRenderer {
    Sprite { id: u32, flags: u32 },
    Unsupported { id: u32, name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrParticleChild {
    pub id: u32,
    pub particle: String,
    pub event: WeIrParticleChildEvent,
    pub max_count: u32,
    pub probability: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrParticleChildEvent {
    Birth,
    Death,
    Collision,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeIrFallingLeavesProfile {
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeIrAmbientSparklesProfile {
    pub rate: f32,
    pub emitter_origin: SceneVec3,
    pub emitter_directions: SceneVec3,
    pub distance_min: SceneVec3,
    pub distance_max: SceneVec3,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub color_min: SceneVec3,
    pub color_max: SceneVec3,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeIrFloralOscillationProfile {
    pub rate: f32,
    pub emitter_origin: SceneVec3,
    pub emitter_directions: SceneVec3,
    pub distance_min: SceneVec3,
    pub distance_max: SceneVec3,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub rotation_min: f32,
    pub rotation_max: f32,
    pub position_frequency_min: f32,
    pub position_frequency_max: f32,
    pub position_phase_min: f32,
    pub position_phase_max: f32,
    pub position_scale_min: f32,
    pub position_scale_max: f32,
    pub position_mask: SceneVec3,
    pub size_frequency_min: f32,
    pub size_frequency_max: f32,
    pub size_phase_min: f32,
    pub size_phase_max: f32,
    pub size_scale_min: f32,
    pub size_scale_max: f32,
}

impl WeIrParticleSystem {
    pub fn falling_leaves_profile(&self) -> Option<WeIrFallingLeavesProfile> {
        if !self.children.is_empty()
            || !matches!(
                self.renderers.as_slice(),
                [WeIrParticleRenderer::Sprite { .. }]
            )
            || self.emitters.len() != 1
        {
            return None;
        }
        if self.initializers.len() != 7
            || self.operators.len() != 3
            || !self.initializers.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleInitializer::LifetimeRandom { .. }
                        | WeIrParticleInitializer::SizeRandom { .. }
                        | WeIrParticleInitializer::VelocityRandom { .. }
                        | WeIrParticleInitializer::ColorRandom { .. }
                        | WeIrParticleInitializer::RotationRandom { .. }
                        | WeIrParticleInitializer::TurbulentVelocityRandom { .. }
                        | WeIrParticleInitializer::AngularVelocityRandom { .. }
                )
            })
            || !self.operators.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleOperator::Movement { .. }
                        | WeIrParticleOperator::AlphaFade { .. }
                        | WeIrParticleOperator::AngularMovement { .. }
                )
            })
        {
            return None;
        }
        let WeIrParticleEmitter::SphereRandom {
            rate,
            instantaneous,
            origin,
            directions,
            distance_min,
            distance_max,
            ..
        } = self.emitters[0]
        else {
            return None;
        };
        if instantaneous != 0 || rate <= 0.0 {
            return None;
        }
        let lifetime = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::LifetimeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let size = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::SizeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let velocity = find_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::VelocityRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let color = find_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::ColorRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let rotation = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::RotationRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let turbulence = self.initializers.iter().find_map(|item| match item {
            WeIrParticleInitializer::TurbulentVelocityRandom {
                offset,
                scale,
                speed_min,
                speed_max,
                ..
            } => Some((*offset, *scale, *speed_min, *speed_max)),
            _ => None,
        })?;
        let angular_velocity = find_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::AngularVelocityRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let gravity = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::Movement { gravity, .. } => Some(*gravity),
            _ => None,
        })?;
        let fade = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::AlphaFade {
                fade_in_time,
                fade_out_time,
                ..
            } => Some((*fade_in_time, *fade_out_time)),
            _ => None,
        })?;
        if !self
            .operators
            .iter()
            .any(|item| matches!(item, WeIrParticleOperator::AngularMovement { .. }))
        {
            return None;
        }
        Some(WeIrFallingLeavesProfile {
            rate,
            emitter_origin: origin,
            emitter_directions: directions,
            distance_min,
            distance_max,
            lifetime_min: lifetime.0,
            lifetime_max: lifetime.1,
            size_min: size.0,
            size_max: size.1,
            velocity_min: velocity.0,
            velocity_max: velocity.1,
            color_min: color.0,
            color_max: color.1,
            rotation_min: rotation.0,
            rotation_max: rotation.1,
            turbulence_offset: turbulence.0,
            turbulence_scale: turbulence.1,
            turbulence_speed_min: turbulence.2,
            turbulence_speed_max: turbulence.3,
            angular_velocity_min: angular_velocity.0,
            angular_velocity_max: angular_velocity.1,
            gravity,
            fade_in_time: fade.0,
            fade_out_time: fade.1,
        })
    }

    pub fn ambient_sparkles_profile(&self) -> Option<WeIrAmbientSparklesProfile> {
        if !self.children.is_empty()
            || !matches!(
                self.renderers.as_slice(),
                [WeIrParticleRenderer::Sprite { .. }]
            )
            || self.initializers.len() != 3
            || self.operators.len() != 3
            || !self.initializers.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleInitializer::LifetimeRandom { .. }
                        | WeIrParticleInitializer::SizeRandom { .. }
                        | WeIrParticleInitializer::ColorRandom { .. }
                )
            })
            || !self.operators.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleOperator::Movement { .. }
                        | WeIrParticleOperator::AlphaFade { .. }
                        | WeIrParticleOperator::OscillateAlpha { .. }
                )
            })
        {
            return None;
        }
        let [
            WeIrParticleEmitter::BoxRandom {
                rate,
                origin,
                directions,
                distance_min,
                distance_max,
                ..
            },
        ] = self.emitters.as_slice()
        else {
            return None;
        };
        if *rate <= 0.0 {
            return None;
        }
        let lifetime = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::LifetimeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let size = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::SizeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let color = find_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::ColorRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let gravity = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::Movement { gravity, .. } => Some(*gravity),
            _ => None,
        })?;
        let fade = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::AlphaFade {
                fade_in_time,
                fade_out_time,
                ..
            } => Some((*fade_in_time, *fade_out_time)),
            _ => None,
        })?;
        let oscillation = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::OscillateAlpha {
                frequency_min,
                frequency_max,
                phase_min,
                phase_max,
                scale_min,
                scale_max,
                ..
            } => Some((
                *frequency_min,
                *frequency_max,
                *phase_min,
                *phase_max,
                *scale_min,
                *scale_max,
            )),
            _ => None,
        })?;
        Some(WeIrAmbientSparklesProfile {
            rate: *rate,
            emitter_origin: *origin,
            emitter_directions: *directions,
            distance_min: *distance_min,
            distance_max: *distance_max,
            lifetime_min: lifetime.0,
            lifetime_max: lifetime.1,
            size_min: size.0,
            size_max: size.1,
            color_min: color.0,
            color_max: color.1,
            gravity,
            fade_in_time: fade.0,
            fade_out_time: fade.1,
            oscillation_frequency_min: oscillation.0,
            oscillation_frequency_max: oscillation.1,
            oscillation_phase_min: oscillation.2,
            oscillation_phase_max: oscillation.3,
            oscillation_scale_min: oscillation.4,
            oscillation_scale_max: oscillation.5,
        })
    }

    pub fn floral_oscillation_profile(&self) -> Option<WeIrFloralOscillationProfile> {
        if !self.children.is_empty()
            || !matches!(
                self.renderers.as_slice(),
                [WeIrParticleRenderer::Sprite { .. }]
            )
            || self.initializers.len() != 3
            || self.operators.len() != 2
            || !self.initializers.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleInitializer::LifetimeRandom { .. }
                        | WeIrParticleInitializer::SizeRandom { .. }
                        | WeIrParticleInitializer::RotationRandom { .. }
                )
            })
            || !self.operators.iter().all(|item| {
                matches!(
                    item,
                    WeIrParticleOperator::OscillatePosition { .. }
                        | WeIrParticleOperator::OscillateSize { .. }
                )
            })
        {
            return None;
        }
        let [
            WeIrParticleEmitter::BoxRandom {
                rate,
                origin,
                directions,
                distance_min,
                distance_max,
                ..
            },
        ] = self.emitters.as_slice()
        else {
            return None;
        };
        if *rate <= 0.0 {
            return None;
        }
        let lifetime = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::LifetimeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let size = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::SizeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let rotation = find_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::RotationRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let position = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::OscillatePosition {
                frequency_min,
                frequency_max,
                phase_min,
                phase_max,
                scale_min,
                scale_max,
                mask,
                ..
            } => Some((
                *frequency_min,
                *frequency_max,
                *phase_min,
                *phase_max,
                *scale_min,
                *scale_max,
                *mask,
            )),
            _ => None,
        })?;
        let size_oscillation = self.operators.iter().find_map(|item| match item {
            WeIrParticleOperator::OscillateSize {
                frequency_min,
                frequency_max,
                phase_min,
                phase_max,
                scale_min,
                scale_max,
                ..
            } => Some((
                *frequency_min,
                *frequency_max,
                *phase_min,
                *phase_max,
                *scale_min,
                *scale_max,
            )),
            _ => None,
        })?;
        Some(WeIrFloralOscillationProfile {
            rate: *rate,
            emitter_origin: *origin,
            emitter_directions: *directions,
            distance_min: *distance_min,
            distance_max: *distance_max,
            lifetime_min: lifetime.0,
            lifetime_max: lifetime.1,
            size_min: size.0,
            size_max: size.1,
            rotation_min: rotation.0,
            rotation_max: rotation.1,
            position_frequency_min: position.0,
            position_frequency_max: position.1,
            position_phase_min: position.2,
            position_phase_max: position.3,
            position_scale_min: position.4,
            position_scale_max: position.5,
            position_mask: position.6,
            size_frequency_min: size_oscillation.0,
            size_frequency_max: size_oscillation.1,
            size_phase_min: size_oscillation.2,
            size_phase_max: size_oscillation.3,
            size_scale_min: size_oscillation.4,
            size_scale_max: size_oscillation.5,
        })
    }
}

fn find_scalar_range(
    items: &[WeIrParticleInitializer],
    find: impl Fn(&WeIrParticleInitializer) -> Option<(f32, f32)>,
) -> Option<(f32, f32)> {
    items.iter().find_map(find)
}

fn find_vec3_range(
    items: &[WeIrParticleInitializer],
    find: impl Fn(&WeIrParticleInitializer) -> Option<(SceneVec3, SceneVec3)>,
) -> Option<(SceneVec3, SceneVec3)> {
    items.iter().find_map(find)
}
