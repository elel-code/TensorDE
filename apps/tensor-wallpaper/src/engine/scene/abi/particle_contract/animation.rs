//! Typed particle texture-sequence playback modes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneParticleAnimationMode {
    InterpolatedSequence,
    RandomFrame,
}

impl SceneParticleAnimationMode {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::InterpolatedSequence => 0,
            Self::RandomFrame => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::InterpolatedSequence),
            1 => Some(Self::RandomFrame),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SceneParticleAnimationMode;

    #[test]
    fn particle_animation_mode_binary_discriminants_are_strict() {
        assert_eq!(
            SceneParticleAnimationMode::from_u32(0),
            Some(SceneParticleAnimationMode::InterpolatedSequence)
        );
        assert_eq!(
            SceneParticleAnimationMode::from_u32(1),
            Some(SceneParticleAnimationMode::RandomFrame)
        );
        assert_eq!(SceneParticleAnimationMode::from_u32(2), None);
        assert_eq!(SceneParticleAnimationMode::from_u32(u32::MAX), None);
    }
}
