//! Analytic non-zero coverage for the typed flat rounded-mask composite.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FlatRoundedMaskUvBounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

pub(super) fn flat_rounded_mask_uv_bounds(
    size: [f32; 2],
    softness: f32,
    object_pixel_extent: [f32; 2],
    output_extent: [u32; 2],
) -> Option<FlatRoundedMaskUvBounds> {
    if !size
        .into_iter()
        .chain([softness])
        .chain(object_pixel_extent)
        .all(f32::is_finite)
        || size.iter().any(|value| *value <= 0.0)
        || softness < 0.0
        || object_pixel_extent.iter().any(|value| *value <= 0.0)
    {
        return None;
    }
    let [width_pixels, height_pixels] = object_pixel_extent;
    let aspect = [
        (width_pixels / height_pixels).max(1.0),
        (height_pixels / width_pixels).max(1.0),
    ];
    let edge_softness = softness / output_extent[0].max(output_extent[1]).max(1) as f32 * 2.0;
    let half_extent = [
        size[0] * 0.5 + edge_softness / aspect[0],
        size[1] * 0.5 + edge_softness / aspect[1],
    ];
    Some(FlatRoundedMaskUvBounds {
        min: [0.5 - half_extent[0], 0.5 - half_extent[1]],
        max: [0.5 + half_extent[0], 0.5 + half_extent[1]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_include_only_the_soft_sdf_support() {
        let bounds = flat_rounded_mask_uv_bounds([0.9, 0.9], 2.0, [550.0, 3300.0], [2561, 1601])
            .expect("bounded rounded mask");

        let edge = 2.0 / 2561.0 * 2.0;
        assert!((bounds.min[0] - (0.05 - edge)).abs() <= 1.0e-6);
        assert!((bounds.max[0] - (0.95 + edge)).abs() <= 1.0e-6);
        assert!((bounds.min[1] - (0.05 - edge / 6.0)).abs() <= 1.0e-6);
        assert!((bounds.max[1] - (0.95 + edge / 6.0)).abs() <= 1.0e-6);
    }

    #[test]
    fn invalid_parameters_require_an_unbounded_fallback() {
        assert!(
            flat_rounded_mask_uv_bounds([0.9, 0.9], -1.0, [550.0, 3300.0], [2561, 1601],).is_none()
        );
    }
}
