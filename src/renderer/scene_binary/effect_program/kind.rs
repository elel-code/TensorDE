//! WE effect kind classification.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`

use crate::engine::scene_engine::we::WeEffectKind;

pub(super) fn gscn_we_effect_kind(kind: u16, effect_file: &str) -> WeEffectKind {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    match kind {
        1 => WeEffectKind::Opacity,
        2 => WeEffectKind::Iris,
        3 => WeEffectKind::WaterRipple,
        4 => WeEffectKind::WaterWaves,
        5 => WeEffectKind::WaterFlow,
        7 if file.contains("foliagesway")
            || file.contains("foliage_sway")
            || file.contains("auto_sway")
            || file.contains("autosway") =>
        {
            WeEffectKind::FoliageSway
        }
        _ if file.contains("opacity") => WeEffectKind::Opacity,
        _ if file.contains("iris") => WeEffectKind::Iris,
        _ if file.contains("waterripple") || file.contains("water_ripple") => {
            WeEffectKind::WaterRipple
        }
        _ if file.contains("waterwaves") || file.contains("water_waves") => {
            WeEffectKind::WaterWaves
        }
        _ if file.contains("waterflow") || file.contains("water_flow") => WeEffectKind::WaterFlow,
        _ if file.contains("foliagesway")
            || file.contains("foliage_sway")
            || file.contains("auto_sway")
            || file.contains("autosway") =>
        {
            WeEffectKind::FoliageSway
        }
        _ if file.contains("scroll") => WeEffectKind::Scroll,
        _ if file.contains("skew") => WeEffectKind::Skew,
        _ if file.contains("tint") => WeEffectKind::Tint,
        _ => WeEffectKind::Unknown,
    }
}
