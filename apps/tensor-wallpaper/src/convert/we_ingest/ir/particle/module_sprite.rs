use super::*;

impl WeIrParticleSystem {
    pub fn module_sprite_profile(&self) -> Option<WeIrModuleSpriteProfile> {
        if !matches!(
            self.renderers.as_slice(),
            [WeIrParticleRenderer::Sprite { .. } | WeIrParticleRenderer::SpriteTrail { .. }]
        ) || self.emitters.len() != 1
            || self.initializers.iter().any(|item| {
                !matches!(
                    item,
                    WeIrParticleInitializer::LifetimeRandom { .. }
                        | WeIrParticleInitializer::SizeRandom { .. }
                        | WeIrParticleInitializer::VelocityRandom { .. }
                        | WeIrParticleInitializer::ColorRandom { .. }
                        | WeIrParticleInitializer::AlphaRandom { .. }
                        | WeIrParticleInitializer::RotationRandom { .. }
                        | WeIrParticleInitializer::TurbulentVelocityRandom { .. }
                )
            })
            || self.operators.iter().any(|item| {
                !matches!(
                    item,
                    WeIrParticleOperator::Movement { .. }
                        | WeIrParticleOperator::AlphaFade { .. }
                        | WeIrParticleOperator::OscillatePosition { .. }
                        | WeIrParticleOperator::Turbulence { .. }
                        | WeIrParticleOperator::SizeChange { .. }
                        | WeIrParticleOperator::Vortex { .. }
                )
            })
        {
            return None;
        }

        let (
            emitter_shape,
            rate,
            origin,
            directions,
            distance_min,
            distance_max,
            emitter_speed_min,
            emitter_speed_max,
        ) = match self.emitters[0] {
            WeIrParticleEmitter::SphereRandom {
                rate,
                instantaneous: 0,
                origin,
                directions,
                distance_min,
                distance_max,
                speed_min,
                speed_max,
                ..
            } => (
                SceneParticleEmitterShape::SphereRandom,
                rate,
                origin,
                directions,
                distance_min,
                distance_max,
                speed_min,
                speed_max,
            ),
            WeIrParticleEmitter::BoxRandom {
                rate,
                origin,
                directions,
                distance_min,
                distance_max,
                ..
            } => (
                SceneParticleEmitterShape::BoxRandom,
                rate,
                origin,
                directions,
                distance_min,
                distance_max,
                0.0,
                0.0,
            ),
            _ => return None,
        };
        if rate <= 0.0 {
            return None;
        }
        let (renderer, renderer_flags, trail_length, trail_min_length, trail_max_length) =
            match self.renderers[0] {
                WeIrParticleRenderer::Sprite { flags, .. } => {
                    (SceneParticleRendererKind::Sprite, flags, 0.0, 0.0, 0.0)
                }
                WeIrParticleRenderer::SpriteTrail {
                    flags,
                    length,
                    min_length,
                    max_length,
                    ..
                } => (
                    SceneParticleRendererKind::SpriteTrail,
                    flags,
                    length,
                    min_length,
                    max_length,
                ),
                _ => return None,
            };

        let lifetime = exactly_one_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::LifetimeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let size = exactly_one_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::SizeRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let velocity = optional_one_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::VelocityRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let color = optional_one_vec3_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::ColorRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let alpha = optional_one_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::AlphaRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let rotation = optional_one_scalar_range(&self.initializers, |item| match item {
            WeIrParticleInitializer::RotationRandom { min, max, .. } => Some((*min, *max)),
            _ => None,
        })?;
        let turbulent_velocity = optional_one_initializer(&self.initializers, |item| match item {
            WeIrParticleInitializer::TurbulentVelocityRandom {
                offset,
                scale,
                speed_min,
                speed_max,
                phase_min,
                phase_max,
                time_scale,
                right,
                forward,
                ..
            } => Some((
                *offset,
                *scale,
                *speed_min,
                *speed_max,
                *phase_min,
                *phase_max,
                *time_scale,
                *right,
                *forward,
            )),
            _ => None,
        })?;
        let movement = optional_one_operator(&self.operators, |item| match item {
            WeIrParticleOperator::Movement { gravity, .. } => Some(*gravity),
            _ => None,
        })?;
        let fade = optional_one_operator(&self.operators, |item| match item {
            WeIrParticleOperator::AlphaFade {
                fade_in_time,
                fade_out_time,
                ..
            } => Some((*fade_in_time, *fade_out_time)),
            _ => None,
        })?;
        let position = optional_one_operator(&self.operators, |item| match item {
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
        let turbulence = optional_one_operator(&self.operators, |item| match item {
            WeIrParticleOperator::Turbulence {
                mask,
                phase_min,
                phase_max,
                scale,
                speed_min,
                speed_max,
                time_scale,
                ..
            } => Some((
                *mask,
                *phase_min,
                *phase_max,
                *scale,
                *speed_min,
                *speed_max,
                *time_scale,
            )),
            _ => None,
        })?;
        let size_change = optional_one_operator(&self.operators, |item| match item {
            WeIrParticleOperator::SizeChange {
                start_time,
                start_value,
                end_value,
                ..
            } => Some((*start_time, *start_value, *end_value)),
            _ => None,
        })?;
        let vortex = optional_one_operator(&self.operators, |item| match item {
            WeIrParticleOperator::Vortex {
                axis,
                distance_inner,
                distance_outer,
                speed_inner,
                speed_outer,
                ..
            } => Some((
                *axis,
                *distance_inner,
                *distance_outer,
                *speed_inner,
                *speed_outer,
            )),
            _ => None,
        })?;
        let initializer_kinds = self
            .initializers
            .iter()
            .map(WeIrParticleInitializer::runtime_kind)
            .collect::<Option<Vec<_>>>()?;
        let initializer_order = SceneParticleInitializerOrder::from_kinds(&initializer_kinds)?;

        let mut mask = SceneParticleModuleMask(
            SceneParticleModuleMask::LIFETIME_RANDOM | SceneParticleModuleMask::SIZE_RANDOM,
        );
        for (present, bit) in [
            (velocity.is_some(), SceneParticleModuleMask::VELOCITY_RANDOM),
            (color.is_some(), SceneParticleModuleMask::COLOR_RANDOM),
            (alpha.is_some(), SceneParticleModuleMask::ALPHA_RANDOM),
            (rotation.is_some(), SceneParticleModuleMask::ROTATION_RANDOM),
            (movement.is_some(), SceneParticleModuleMask::MOVEMENT),
            (fade.is_some(), SceneParticleModuleMask::ALPHA_FADE),
            (
                position.is_some(),
                SceneParticleModuleMask::OSCILLATE_POSITION,
            ),
            (
                turbulent_velocity.is_some(),
                SceneParticleModuleMask::TURBULENT_VELOCITY_RANDOM,
            ),
            (turbulence.is_some(), SceneParticleModuleMask::TURBULENCE),
            (size_change.is_some(), SceneParticleModuleMask::SIZE_CHANGE),
            (vortex.is_some(), SceneParticleModuleMask::VORTEX),
        ] {
            if present {
                mask.0 |= bit;
            }
        }
        let zero = SceneVec3::default();
        let one = SceneVec3::ONE;
        let position = position.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, one));
        let turbulent_velocity = turbulent_velocity.unwrap_or((
            0.0,
            1.0,
            0.5,
            1.0,
            0.0,
            0.0,
            1.0,
            SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            SceneVec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ));
        let turbulence = turbulence.unwrap_or((one, 0.0, 0.0, 1.0, 1.0, 2.0, 1.0));
        let size_change = size_change.unwrap_or((0.0, 1.0, 1.0));
        let vortex = vortex.unwrap_or((
            SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            0.0,
            0.0,
            0.0,
            0.0,
        ));
        Some(WeIrModuleSpriteProfile {
            emitter_shape,
            module_mask: mask,
            initializer_order,
            renderer,
            renderer_flags,
            rate,
            emitter_origin: origin,
            emitter_directions: directions,
            distance_min,
            distance_max,
            emitter_speed_min,
            emitter_speed_max,
            lifetime_min: lifetime.0,
            lifetime_max: lifetime.1,
            size_min: size.0,
            size_max: size.1,
            velocity_min: velocity.unwrap_or((zero, zero)).0,
            velocity_max: velocity.unwrap_or((zero, zero)).1,
            color_min: color.unwrap_or((one, one)).0,
            color_max: color.unwrap_or((one, one)).1,
            alpha_min: alpha.unwrap_or((1.0, 1.0)).0,
            alpha_max: alpha.unwrap_or((1.0, 1.0)).1,
            rotation_min: rotation.unwrap_or((0.0, 0.0)).0,
            rotation_max: rotation.unwrap_or((0.0, 0.0)).1,
            turbulence_offset: turbulent_velocity.0,
            turbulence_scale: turbulent_velocity.1,
            turbulence_speed_min: turbulent_velocity.2,
            turbulence_speed_max: turbulent_velocity.3,
            turbulent_velocity_phase_min: turbulent_velocity.4,
            turbulent_velocity_phase_max: turbulent_velocity.5,
            turbulent_velocity_time_scale: turbulent_velocity.6,
            turbulent_velocity_right: turbulent_velocity.7,
            turbulent_velocity_forward: turbulent_velocity.8,
            turbulence_operator_scale: turbulence.3,
            turbulence_operator_speed_min: turbulence.4,
            turbulence_operator_speed_max: turbulence.5,
            turbulence_operator_phase_min: turbulence.1,
            turbulence_phase_max: turbulence.2,
            turbulence_operator_time_scale: turbulence.6,
            turbulence_mask: turbulence.0,
            gravity: movement.unwrap_or(zero),
            fade_in_time: fade.unwrap_or((0.0, 1.0)).0,
            fade_out_time: fade.unwrap_or((0.0, 1.0)).1,
            position_frequency_min: position.0,
            position_frequency_max: position.1,
            position_phase_min: position.2,
            position_phase_max: position.3,
            position_scale_min: position.4,
            position_scale_max: position.5,
            position_mask: position.6,
            size_change_start_time: size_change.0,
            size_change_start_value: size_change.1,
            size_change_end_value: size_change.2,
            vortex_axis: vortex.0,
            vortex_distance_inner: vortex.1,
            vortex_distance_outer: vortex.2,
            vortex_speed_inner: vortex.3,
            vortex_speed_outer: vortex.4,
            trail_length,
            trail_min_length,
            trail_max_length,
        })
    }
}
