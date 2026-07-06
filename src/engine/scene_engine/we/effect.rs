//! WE effect family semantics.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/index.md`
//! - `reverse-engineered/shaders/effects/waterwaves.frag`
//! - `reverse-engineered/shaders/effects/waterripple.frag`
//! - `reverse-engineered/shaders/effects/waterflow.frag`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum WeEffectKind {
    Opacity,
    Iris,
    WaterWaves,
    WaterRipple,
    WaterFlow,
    FoliageSway,
    Scroll,
    Skew,
    Tint,
    PassthroughBlend,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WeEffectOutputContract {
    SourcePreserving,
    AlphaModifying,
    ColorBlend,
    Replacement,
}

impl WeEffectKind {
    pub fn output_contract(self) -> WeEffectOutputContract {
        match self {
            Self::Iris | Self::WaterWaves | Self::WaterRipple | Self::WaterFlow | Self::Scroll => {
                WeEffectOutputContract::SourcePreserving
            }
            Self::Opacity => WeEffectOutputContract::AlphaModifying,
            Self::Tint | Self::PassthroughBlend => WeEffectOutputContract::ColorBlend,
            Self::FoliageSway | Self::Skew | Self::Unknown => WeEffectOutputContract::Replacement,
        }
    }
}
