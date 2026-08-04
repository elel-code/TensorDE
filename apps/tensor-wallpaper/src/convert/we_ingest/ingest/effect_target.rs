//! WE effect target classification and resolution scaling.

use crate::convert::we_ingest::ir::WeIrImageTargetRole;

pub(super) fn image_target_role(name: &str) -> WeIrImageTargetRole {
    if name.starts_with("_tmp_") {
        WeIrImageTargetRole::Temporary
    } else if name.starts_with("_rt_") {
        WeIrImageTargetRole::FirstClassEffectTarget
    } else {
        // Effect-declared FBO names are local named targets. WE's engine-owned first-class
        // targets use the `_rt_` namespace; authored effects are not required to prefix their
        // private FBOs with the literal `fbo_` (blur pyramids commonly use names such as
        // `blur_start_4`). Classifying every other name as first-class loses the authored scale
        // when a pass later addresses the same resource through NamedFbo.
        WeIrImageTargetRole::NamedFbo
    }
}

pub(super) fn scale_divisor_to_milli(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        (value * 1000.0).round().clamp(1.0, u32::MAX as f32) as u32
    } else {
        1000
    }
}
