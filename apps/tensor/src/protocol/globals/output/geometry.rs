use tensor_host::SubpixelLayout;
use tensor_protocol::SurfaceTransform;
use tensor_util::OutputScale;
use wayland_server::protocol::wl_output;

pub(super) fn integer_scale(scale: OutputScale) -> i32 {
    i32::try_from(scale.units().div_ceil(OutputScale::DENOMINATOR))
        .unwrap_or(i32::MAX)
        .max(1)
}

pub(super) fn logical_length_round(value: i32, scale: OutputScale) -> i32 {
    (f64::from(value) / scale.as_f64()).round() as i32
}

pub(crate) fn transformed_dimensions(
    width: i32,
    height: i32,
    transform: SurfaceTransform,
) -> (i32, i32) {
    match transform {
        SurfaceTransform::Rotate90
        | SurfaceTransform::Rotate270
        | SurfaceTransform::Flipped90
        | SurfaceTransform::Flipped270 => (height, width),
        _ => (width, height),
    }
}

pub(super) fn wl_subpixel(subpixel: SubpixelLayout) -> wl_output::Subpixel {
    match subpixel {
        SubpixelLayout::Unknown => wl_output::Subpixel::Unknown,
        SubpixelLayout::None => wl_output::Subpixel::None,
        SubpixelLayout::HorizontalRgb => wl_output::Subpixel::HorizontalRgb,
        SubpixelLayout::HorizontalBgr => wl_output::Subpixel::HorizontalBgr,
        SubpixelLayout::VerticalRgb => wl_output::Subpixel::VerticalRgb,
        SubpixelLayout::VerticalBgr => wl_output::Subpixel::VerticalBgr,
    }
}

pub(super) fn wl_transform(transform: SurfaceTransform) -> wl_output::Transform {
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

pub(super) const fn subpixel_code(subpixel: SubpixelLayout) -> u32 {
    match subpixel {
        SubpixelLayout::Unknown => 0,
        SubpixelLayout::None => 1,
        SubpixelLayout::HorizontalRgb => 2,
        SubpixelLayout::HorizontalBgr => 3,
        SubpixelLayout::VerticalRgb => 4,
        SubpixelLayout::VerticalBgr => 5,
    }
}

pub(super) const fn subpixel_from_code(code: u32) -> SubpixelLayout {
    match code {
        1 => SubpixelLayout::None,
        2 => SubpixelLayout::HorizontalRgb,
        3 => SubpixelLayout::HorizontalBgr,
        4 => SubpixelLayout::VerticalRgb,
        5 => SubpixelLayout::VerticalBgr,
        _ => SubpixelLayout::Unknown,
    }
}

pub(super) const fn transform_code(transform: SurfaceTransform) -> u32 {
    match transform {
        SurfaceTransform::Normal => 0,
        SurfaceTransform::Rotate90 => 1,
        SurfaceTransform::Rotate180 => 2,
        SurfaceTransform::Rotate270 => 3,
        SurfaceTransform::Flipped => 4,
        SurfaceTransform::Flipped90 => 5,
        SurfaceTransform::Flipped180 => 6,
        SurfaceTransform::Flipped270 => 7,
    }
}

pub(super) const fn transform_from_code(code: u32) -> SurfaceTransform {
    match code {
        1 => SurfaceTransform::Rotate90,
        2 => SurfaceTransform::Rotate180,
        3 => SurfaceTransform::Rotate270,
        4 => SurfaceTransform::Flipped,
        5 => SurfaceTransform::Flipped90,
        6 => SurfaceTransform::Flipped180,
        7 => SurfaceTransform::Flipped270,
        _ => SurfaceTransform::Normal,
    }
}
