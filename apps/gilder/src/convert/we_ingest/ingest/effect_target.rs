//! WE effect target classification and resolution scaling.

use crate::convert::we_ingest::ir::WeIrImageTargetRole;

pub(super) fn image_target_role(name: &str) -> WeIrImageTargetRole {
    if name.starts_with("fbo_") {
        WeIrImageTargetRole::NamedFbo
    } else if name.starts_with("_tmp_") {
        WeIrImageTargetRole::Temporary
    } else {
        WeIrImageTargetRole::FirstClassEffectTarget
    }
}

pub(super) fn scale_divisor_to_milli(value: f32) -> u32 {
    if value.is_finite() && value > 0.0 {
        (value * 1000.0).round().clamp(1.0, u32::MAX as f32) as u32
    } else {
        1000
    }
}
