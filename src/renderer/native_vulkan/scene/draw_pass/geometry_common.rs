use super::*;

pub(super) fn native_vulkan_scene_quad_positions(
    width: f64,
    height: f64,
    transform: SceneTransform,
) -> Option<[[f32; 2]; 4]> {
    let left = -transform.anchor_x * width;
    let top = -transform.anchor_y * height;
    let right = left + width;
    let bottom = top + height;
    let rotation = transform.rotation_deg.to_radians();
    let cos = rotation.cos();
    let sin = rotation.sin();
    let points = [(left, top), (right, top), (left, bottom), (right, bottom)];
    let mut positions = [[0.0, 0.0]; 4];
    for (position, (x, y)) in positions.iter_mut().zip(points) {
        *position = native_vulkan_scene_transform_point_with_rotation(x, y, transform, cos, sin)?;
    }
    Some(positions)
}

pub(super) fn native_vulkan_scene_transform_point(
    x: f64,
    y: f64,
    transform: SceneTransform,
) -> Option<[f32; 2]> {
    let rotation = transform.rotation_deg.to_radians();
    native_vulkan_scene_transform_point_with_rotation(
        x,
        y,
        transform,
        rotation.cos(),
        rotation.sin(),
    )
}

pub(super) fn native_vulkan_scene_transform_point_with_rotation(
    x: f64,
    y: f64,
    transform: SceneTransform,
    cos: f64,
    sin: f64,
) -> Option<[f32; 2]> {
    let scaled_x = x * transform.scale_x;
    let scaled_y = y * transform.scale_y;
    let scene_x = scaled_x.mul_add(cos, -scaled_y * sin) + transform.x;
    let scene_y = scaled_x.mul_add(sin, scaled_y * cos) + transform.y;
    if !scene_x.is_finite() || !scene_y.is_finite() {
        return None;
    }
    Some([scene_x as f32, scene_y as f32])
}

pub(super) fn native_vulkan_scene_solid_vertex_buffer_bytes(vertex_count: usize) -> u64 {
    (vertex_count as u64).saturating_mul(SCENE_FULL_SOLID_QUAD_VERTEX_BYTES)
}

pub(super) fn native_vulkan_scene_solid_index_buffer_bytes(index_count: usize) -> u64 {
    (index_count as u64).saturating_mul(SCENE_FULL_SOLID_QUAD_INDEX_BYTES)
}

pub(super) fn native_vulkan_scene_sampled_image_vertex_buffer_bytes(vertex_count: usize) -> u64 {
    (vertex_count as u64).saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_BYTES)
}

pub(super) fn native_vulkan_scene_sampled_image_index_buffer_bytes(index_count: usize) -> u64 {
    (index_count as u64).saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_BYTES)
}
