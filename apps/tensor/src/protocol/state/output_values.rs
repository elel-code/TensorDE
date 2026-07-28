//! Output values translated directly to Wayland wire values.

use tensor_protocol::SurfaceTransform;
use tensor_util::OutputScale;
use wayland_server::protocol::wl_output;

pub(super) fn output_integer_scale(scale: OutputScale) -> i32 {
    i32::try_from(scale.units().div_ceil(OutputScale::DENOMINATOR))
        .unwrap_or(i32::MAX)
        .max(1)
}

pub(super) const fn wayland_transform(transform: SurfaceTransform) -> wl_output::Transform {
    match transform {
        SurfaceTransform::Normal => wl_output::Transform::Normal,
        SurfaceTransform::Rotate90 => wl_output::Transform::_90,
        SurfaceTransform::Rotate180 => wl_output::Transform::_180,
        SurfaceTransform::Rotate270 => wl_output::Transform::_270,
        SurfaceTransform::Flipped => wl_output::Transform::Flipped,
        SurfaceTransform::Flipped90 => wl_output::Transform::Flipped90,
        SurfaceTransform::Flipped180 => wl_output::Transform::Flipped180,
        SurfaceTransform::Flipped270 => wl_output::Transform::Flipped270,
    }
}
