use crate::engine::scene::SceneRenderingDeviceMeshDraw;

pub(super) fn draw_source_aspect_ratio(
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> f32 {
    let [authored_width, authored_height] = draw.authored_source_extent;
    if authored_width.is_finite()
        && authored_height.is_finite()
        && authored_width > 0.0
        && authored_height > 0.0
    {
        authored_width / authored_height
    } else {
        output_extent[0].max(1) as f32 / output_extent[1].max(1) as f32
    }
}
