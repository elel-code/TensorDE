//! Mapping from Wallpaper Engine's logical scene canvas to the physical output viewport.

use crate::engine::scene::SceneProjectRecord;

/// Selects the live surface extent independently from the authored scene canvas.
///
/// The authored extent remains an input to projection and intermediate render-target sizing; it
/// must not replace the compositor-configured Wayland buffer extent of the final surface.
pub(super) fn scene_surface_extent(
    explicit_surface_extent: Option<(u32, u32)>,
    wayland_buffer_extent: (u32, u32),
) -> (u32, u32) {
    explicit_surface_extent.unwrap_or(wayland_buffer_extent)
}

/// Applies an aspect-preserving `cover` mapping to a logical-scene clip matrix.
///
/// The scene graph produces clip coordinates for the complete logical canvas. A Vulkan viewport
/// maps those coordinates over its complete physical extent, which would stretch a scene whenever
/// the two aspect ratios differ. Wallpaper scenes instead retain their authored pixel aspect and
/// crop the overflowing axis around the canvas centre.
pub(super) fn scene_cover_clip_transform(
    project: &SceneProjectRecord,
    output_extent: [u32; 2],
    mut clip_transform: [[f32; 4]; 4],
) -> [[f32; 4]; 4] {
    let logical_width = project.logical_width.max(1) as f32;
    let logical_height = project.logical_height.max(1) as f32;
    let output_width = output_extent[0].max(1) as f32;
    let output_height = output_extent[1].max(1) as f32;
    let logical_aspect = logical_width / logical_height;
    let output_aspect = output_width / output_height;

    let [clip_scale_x, clip_scale_y] = if logical_aspect > output_aspect {
        [logical_aspect / output_aspect, 1.0]
    } else {
        [1.0, output_aspect / logical_aspect]
    };
    for value in &mut clip_transform[0] {
        *value *= clip_scale_x;
    }
    for value in &mut clip_transform[1] {
        *value *= clip_scale_y;
    }
    clip_transform
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneBinaryDocument;

    #[test]
    fn cover_crops_wide_scene_horizontally_on_taller_output() {
        let mut project = SceneBinaryDocument::default().project;
        project.logical_width = 3840;
        project.logical_height = 2160;
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        let transform = scene_cover_clip_transform(&project, [2560, 1600], identity);

        assert!((transform[0][0] - 10.0 / 9.0).abs() <= 1.0e-6);
        assert_eq!(transform[1][1], 1.0);
    }

    #[test]
    fn cover_crops_tall_scene_vertically_on_wider_output() {
        let mut project = SceneBinaryDocument::default().project;
        project.logical_width = 1920;
        project.logical_height = 1080;

        let transform = scene_cover_clip_transform(
            &project,
            [2560, 1080],
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        );

        assert_eq!(transform[0][0], 1.0);
        assert!((transform[1][1] - 4.0 / 3.0).abs() <= 1.0e-6);
    }
}
