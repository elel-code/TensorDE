use crate::core::scene::{SceneMesh, SceneNativeEffectMotion};

use super::NativeVulkanSceneEffectKind;

const GRID_MIN_SEGMENTS: usize = 12;
const GRID_MAX_SEGMENTS: usize = 20;
const MESH_MAX_SUBDIVISION: usize = 4;
const MESH_MAX_VERTICES: usize = 4096;
const MESH_SAMPLES_PER_PERIOD: f64 = 4.0;

pub(super) fn classify(normalized_effect_file: &str) -> Option<NativeVulkanSceneEffectKind> {
    if normalized_effect_file.contains("sway") || normalized_effect_file.contains("shake") {
        Some(NativeVulkanSceneEffectKind::SwayShake)
    } else if normalized_effect_file.contains("flutter") {
        Some(NativeVulkanSceneEffectKind::Flutter)
    } else if normalized_effect_file.contains("drift") {
        Some(NativeVulkanSceneEffectKind::Drift)
    } else {
        None
    }
}

pub(in crate::renderer::native_vulkan::scene::draw_pass) fn grid_segments(
    width: f64,
    height: f64,
    motion: SceneNativeEffectMotion,
) -> usize {
    let motion_frequency = max_frequency(motion);
    if motion_frequency <= f64::EPSILON {
        return GRID_MIN_SEGMENTS;
    }
    let motion_amplitude = max_amplitude(motion);
    if motion_amplitude < 0.75 {
        return GRID_MIN_SEGMENTS;
    }
    let max_extent = width.abs().max(height.abs());
    if !max_extent.is_finite() || max_extent <= f64::EPSILON {
        return GRID_MIN_SEGMENTS;
    }
    let samples_per_period = MESH_SAMPLES_PER_PERIOD * (motion_amplitude / 4.0).clamp(0.5, 1.0);
    let target_edge =
        (std::f64::consts::TAU / motion_frequency / samples_per_period).clamp(12.0, 128.0);
    let max_segments = if motion_amplitude < 2.0 {
        16
    } else {
        GRID_MAX_SEGMENTS
    };
    (max_extent / target_edge)
        .ceil()
        .clamp(GRID_MIN_SEGMENTS as f64, max_segments as f64) as usize
}

pub(in crate::renderer::native_vulkan::scene::draw_pass) fn mesh_subdivision(
    width: f64,
    height: f64,
    motion: SceneNativeEffectMotion,
    mesh: &SceneMesh,
) -> Option<usize> {
    if !motion.is_active() {
        return Some(1);
    }
    let triangle_count = mesh.indices.len() / 3;
    if triangle_count == 0 {
        return Some(1);
    }
    if mesh.indices.len() % 3 != 0 {
        return None;
    }
    let max_edge = mesh_max_triangle_edge(mesh)?;
    if max_edge <= f64::EPSILON {
        return Some(1);
    }
    let motion_frequency = max_frequency(motion);
    let motion_amplitude = max_amplitude(motion);
    let samples_per_period = MESH_SAMPLES_PER_PERIOD * (motion_amplitude / 4.0).clamp(0.5, 1.0);
    let target_edge = if motion_frequency > f64::EPSILON {
        (std::f64::consts::TAU / motion_frequency / samples_per_period).clamp(24.0, 128.0)
    } else {
        (width.abs().min(height.abs()) / 8.0).clamp(48.0, 128.0)
    };
    let mut subdivision = (max_edge / target_edge)
        .ceil()
        .clamp(1.0, MESH_MAX_SUBDIVISION as f64) as usize;
    while subdivision > 1 {
        let vertices_per_triangle = subdivided_triangle_vertex_count(subdivision)?;
        if triangle_count.saturating_mul(vertices_per_triangle) <= MESH_MAX_VERTICES {
            break;
        }
        subdivision -= 1;
    }
    Some(subdivision.max(1))
}

pub(in crate::renderer::native_vulkan::scene::draw_pass) fn subdivided_triangle_vertex_count(
    subdivision: usize,
) -> Option<usize> {
    subdivision
        .checked_add(1)?
        .checked_mul(subdivision.checked_add(2)?)?
        .checked_div(2)
}

pub(in crate::renderer::native_vulkan::scene::draw_pass) fn append_subdivided_mesh_indices(
    first_vertex: u32,
    triangle_count: usize,
    subdivision: usize,
    indices: &mut Vec<u32>,
) -> Option<u32> {
    if subdivision == 0 {
        return None;
    }
    let vertices_per_triangle = subdivided_triangle_vertex_count(subdivision)?;
    let index_count = triangle_count
        .checked_mul(subdivision)?
        .checked_mul(subdivision)?
        .checked_mul(3)?;
    indices.reserve(index_count);
    for triangle_index in 0..triangle_count {
        let triangle_base = first_vertex.checked_add(
            triangle_index
                .checked_mul(vertices_per_triangle)?
                .min(u32::MAX as usize) as u32,
        )?;
        for row in 0..subdivision {
            for column in 0..subdivision - row {
                let top = triangle_base.checked_add(subdivided_triangle_vertex_offset(
                    subdivision,
                    row,
                    column,
                )? as u32)?;
                let left = triangle_base.checked_add(subdivided_triangle_vertex_offset(
                    subdivision,
                    row + 1,
                    column,
                )? as u32)?;
                let right = triangle_base.checked_add(subdivided_triangle_vertex_offset(
                    subdivision,
                    row,
                    column + 1,
                )? as u32)?;
                indices.extend_from_slice(&[top, left, right]);
                if column < subdivision - row - 1 {
                    let lower_right = triangle_base.checked_add(
                        subdivided_triangle_vertex_offset(subdivision, row + 1, column + 1)? as u32,
                    )?;
                    indices.extend_from_slice(&[left, lower_right, right]);
                }
            }
        }
    }
    Some(index_count.min(u32::MAX as usize) as u32)
}

pub(in crate::renderer::native_vulkan::scene::draw_pass) fn apply(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    motion: SceneNativeEffectMotion,
) -> (f64, f64) {
    if !motion.is_active() {
        return (x, y);
    }
    let (origin_dx, origin_dy) = delta(0.0, 0.0, width, height, motion);
    let (dx, dy) = delta(x, y, width, height, motion);
    (x + dx - origin_dx, y + dy - origin_dy)
}

fn max_frequency(motion: SceneNativeEffectMotion) -> f64 {
    let mut frequency: f64 = 0.0;
    if motion.wave_count > 0 {
        frequency = frequency.max(motion.wave_spatial_frequency.abs());
    }
    if motion.wave2_count > 0 {
        frequency = frequency.max(motion.wave2_spatial_frequency.abs());
    }
    if motion.sway_count > 0 {
        frequency = frequency.max(motion.sway_spatial_frequency.abs());
    }
    frequency
}

fn max_amplitude(motion: SceneNativeEffectMotion) -> f64 {
    let mut amplitude: f64 = 0.0;
    if motion.wave_count > 0 {
        amplitude = amplitude.max(motion.wave_x.hypot(motion.wave_y));
    }
    if motion.wave2_count > 0 {
        amplitude = amplitude.max(motion.wave2_x.hypot(motion.wave2_y));
    }
    if motion.sway_count > 0 {
        amplitude = amplitude.max(motion.sway_amplitude.abs());
    }
    amplitude
}

fn mesh_max_triangle_edge(mesh: &SceneMesh) -> Option<f64> {
    let mut max_edge: f64 = 0.0;
    for triangle in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices.get(usize::try_from(triangle[0]).ok()?)?;
        let b = mesh.vertices.get(usize::try_from(triangle[1]).ok()?)?;
        let c = mesh.vertices.get(usize::try_from(triangle[2]).ok()?)?;
        if !a.x.is_finite()
            || !a.y.is_finite()
            || !b.x.is_finite()
            || !b.y.is_finite()
            || !c.x.is_finite()
            || !c.y.is_finite()
        {
            return None;
        }
        max_edge = max_edge
            .max(distance(a.x, a.y, b.x, b.y))
            .max(distance(b.x, b.y, c.x, c.y))
            .max(distance(c.x, c.y, a.x, a.y));
    }
    Some(max_edge)
}

fn distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    (ax - bx).hypot(ay - by)
}

fn subdivided_triangle_vertex_offset(
    subdivision: usize,
    row: usize,
    column: usize,
) -> Option<usize> {
    if row > subdivision || column > subdivision.saturating_sub(row) {
        return None;
    }
    row.checked_mul(subdivision.checked_add(1)?)?
        .checked_sub(row.checked_mul(row.saturating_sub(1))?.checked_div(2)?)?
        .checked_add(column)
}

fn delta(x: f64, y: f64, width: f64, height: f64, motion: SceneNativeEffectMotion) -> (f64, f64) {
    let mut x = x;
    let mut y = y;
    let original_x = x;
    let original_y = y;
    if motion.wave_count > 0 {
        let (dx, dy) = wave_delta(
            x,
            y,
            motion.wave_x,
            motion.wave_y,
            motion.wave_direction_x,
            motion.wave_direction_y,
            motion.wave_spatial_frequency,
            motion.wave_phase,
        );
        x += dx;
        y += dy;
    }
    if motion.wave2_count > 0 {
        let (dx, dy) = wave_delta(
            x,
            y,
            motion.wave2_x,
            motion.wave2_y,
            motion.wave2_direction_x,
            motion.wave2_direction_y,
            motion.wave2_spatial_frequency,
            motion.wave2_phase,
        );
        x += dx;
        y += dy;
    }
    if motion.sway_amplitude.abs() > f64::EPSILON {
        let vertical = if height.abs() > f64::EPSILON {
            ((y / height) + 0.5).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let horizontal = if width.abs() > f64::EPSILON {
            (x / width).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let direction_length = motion
            .sway_direction_x
            .hypot(motion.sway_direction_y)
            .max(f64::EPSILON);
        let direction_x = if direction_length > f64::EPSILON {
            motion.sway_direction_x / direction_length
        } else {
            1.0
        };
        let direction_y = if direction_length > f64::EPSILON {
            motion.sway_direction_y / direction_length
        } else {
            0.0
        };
        let tip_weight = vertical.powf(motion.sway_power.max(1.0));
        let sway = fast_sin(y * motion.sway_spatial_frequency + motion.sway_phase)
            * motion.sway_amplitude
            * tip_weight;
        x += direction_x * sway;
        y += direction_y * sway + sway * horizontal * 0.12;
    }
    (x - original_x, y - original_y)
}

#[allow(clippy::too_many_arguments)]
fn wave_delta(
    x: f64,
    y: f64,
    wave_x: f64,
    wave_y: f64,
    direction_x: f64,
    direction_y: f64,
    spatial_frequency: f64,
    phase: f64,
) -> (f64, f64) {
    let wave = fast_sin(
        x.mul_add(
            direction_x * spatial_frequency,
            y * direction_y * spatial_frequency,
        ) + phase,
    );
    (wave_x * wave, wave_y * wave)
}

fn fast_sin(value: f64) -> f64 {
    let value =
        (value + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    let sine = 1.273_239_544_735_162_8 * value - 0.405_284_734_569_351_1 * value * value.abs();
    0.225 * (sine * sine.abs() - sine) + sine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::SceneMeshVertex;

    #[test]
    fn grid_segments_keep_motion_tessellation_bounded() {
        let motion = SceneNativeEffectMotion {
            wave_x: 2.0,
            wave_y: 1.0,
            wave_direction_x: 1.0,
            wave_spatial_frequency: 0.1,
            wave_phase: 0.5,
            wave_count: 1,
            ..Default::default()
        };

        assert_eq!(grid_segments(120.0, 60.0, motion), GRID_MIN_SEGMENTS);
        assert_eq!(grid_segments(4096.0, 4096.0, motion), GRID_MAX_SEGMENTS);
    }

    #[test]
    fn mesh_subdivision_respects_vertex_budget() {
        let mesh = SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -512.0,
                    y: -512.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 512.0,
                    y: -512.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -512.0,
                    y: 512.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        };
        let motion = SceneNativeEffectMotion {
            sway_amplitude: 8.0,
            sway_spatial_frequency: 0.02,
            sway_count: 1,
            ..Default::default()
        };

        assert_eq!(mesh_subdivision(1024.0, 1024.0, motion, &mesh), Some(4));
    }

    #[test]
    fn apply_keeps_motion_origin_stable() {
        let motion = SceneNativeEffectMotion {
            wave_x: 6.0,
            wave_y: 3.0,
            wave_direction_x: 1.0,
            wave_spatial_frequency: 0.05,
            wave_phase: 0.25,
            wave_count: 1,
            ..Default::default()
        };

        assert_eq!(apply(0.0, 0.0, 100.0, 100.0, motion), (0.0, 0.0));
        assert_ne!(apply(50.0, 50.0, 100.0, 100.0, motion), (50.0, 50.0));
    }
}
