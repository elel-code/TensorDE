use crate::core::FitMode;

pub(super) fn native_vulkan_video_fit_push_constants(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> [f32; 4] {
    let (offset, scale) = native_vulkan_video_uv_transform(fit, source_size, surface_size);
    // The embedded fullscreen-triangle vertex shader emits raw uv.y=1 at the
    // screen top and uv.y=0 at the bottom. Video frames follow the normal
    // top-left origin used by FFmpeg/GStreamer, so fold the vertical flip into
    // the existing offset/scale push constants.
    [offset[0], offset[1] + scale[1], scale[0], -scale[1]]
}

fn native_vulkan_video_uv_transform(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> ([f32; 2], [f32; 2]) {
    if matches!(fit, FitMode::Stretch | FitMode::Contain | FitMode::Center) {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    let source_aspect = source_size.0.max(1) as f32 / source_size.1.max(1) as f32;
    let surface_aspect = surface_size.0.max(1) as f32 / surface_size.1.max(1) as f32;
    if source_aspect > surface_aspect {
        let width = (surface_aspect / source_aspect).clamp(0.0, 1.0);
        ([(1.0 - width) * 0.5, 0.0], [width, 1.0])
    } else {
        let height = (source_aspect / surface_aspect).clamp(0.0, 1.0);
        ([0.0, (1.0 - height) * 0.5], [1.0, height])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled_y(push: [f32; 4], raw_shader_y: f32) -> f32 {
        push[1] + raw_shader_y * push[3]
    }

    #[test]
    fn flips_video_sampling_to_top_left_frame_origin() {
        let push =
            native_vulkan_video_fit_push_constants(FitMode::Stretch, (1920, 1080), (1920, 1080));

        assert_eq!(push, [0.0, 1.0, 1.0, -1.0]);
        assert_eq!(sampled_y(push, 1.0), 0.0);
        assert_eq!(sampled_y(push, 0.0), 1.0);
    }

    #[test]
    fn preserves_vertical_cover_crop_while_flipping() {
        let push =
            native_vulkan_video_fit_push_constants(FitMode::Cover, (1440, 1080), (1920, 1080));

        assert_eq!(push[0], 0.0);
        assert_eq!(push[2], 1.0);
        assert!((sampled_y(push, 1.0) - 0.125).abs() < f32::EPSILON);
        assert!((sampled_y(push, 0.0) - 0.875).abs() < f32::EPSILON);
    }
}
