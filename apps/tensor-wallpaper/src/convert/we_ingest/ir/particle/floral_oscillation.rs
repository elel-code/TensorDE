use super::*;

impl WeIrParticleSystem {
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
