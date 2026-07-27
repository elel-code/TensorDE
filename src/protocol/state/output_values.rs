//! Transitional mapping from Tensor output values into Smithay surface state.

use tensor_protocol::SurfaceTransform;
use tensor_util::OutputScale;

pub(super) fn output_integer_scale(scale: OutputScale) -> i32 {
    i32::try_from(scale.units().div_ceil(OutputScale::DENOMINATOR))
        .unwrap_or(i32::MAX)
        .max(1)
}

pub(super) fn smithay_transform(transform: SurfaceTransform) -> smithay::utils::Transform {
    match transform {
        SurfaceTransform::Normal => smithay::utils::Transform::Normal,
        SurfaceTransform::Rotate90 => smithay::utils::Transform::_90,
        SurfaceTransform::Rotate180 => smithay::utils::Transform::_180,
        SurfaceTransform::Rotate270 => smithay::utils::Transform::_270,
        SurfaceTransform::Flipped => smithay::utils::Transform::Flipped,
        SurfaceTransform::Flipped90 => smithay::utils::Transform::Flipped90,
        SurfaceTransform::Flipped180 => smithay::utils::Transform::Flipped180,
        SurfaceTransform::Flipped270 => smithay::utils::Transform::Flipped270,
    }
}
