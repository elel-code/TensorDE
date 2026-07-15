//! Conservative per-frame scissors for generated object composites.
//!
//! The final `we/objectcomposite` primitive covers the complete target, but its sampled image only
//! contains the graph's object source and finite effect displacement. The authored object rectangle
//! is not a safe proxy for that coverage: puppet skinning and attachment transforms can move mesh
//! vertices beyond it. This module therefore derives coverage from the current semantic frame and
//! falls back to the complete target whenever an effect cannot prove a finite bound.

use crate::engine::scene::{
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw,
    SceneStorage,
};

use super::draw_recording::{SceneGpuDrawCommand, SceneGpuScissor};
use super::draw_uniform::{
    object_projected_pixel_extent, object_uv_to_screen_affine, object_uv_to_screen_linear,
};
use super::flat_rounded_mask_coverage::flat_rounded_mask_uv_bounds;
use super::material_uniform::material_parameter_values;
use super::scene_viewport::scene_cover_clip_transform;

static SCISSOR_DIAGNOSTIC_EMITTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static COMPOSITE_CONSUMER_CULL_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct PixelBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl PixelBounds {
    fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    fn expand(&mut self, x: f32, y: f32) {
        self.min_x -= x;
        self.max_x += x;
        self.min_y -= y;
        self.max_y += y;
    }

    fn scissor(self, output_extent: [u32; 2]) -> Option<SceneGpuScissor> {
        if !self.min_x.is_finite()
            || !self.min_y.is_finite()
            || !self.max_x.is_finite()
            || !self.max_y.is_finite()
        {
            return None;
        }
        let width = output_extent[0] as f32;
        let height = output_extent[1] as f32;
        let min_x = (self.min_x.floor() - 2.0).clamp(0.0, width);
        let min_y = (self.min_y.floor() - 2.0).clamp(0.0, height);
        let max_x = (self.max_x.ceil() + 2.0).clamp(0.0, width);
        let max_y = (self.max_y.ceil() + 2.0).clamp(0.0, height);
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        Some(SceneGpuScissor {
            offset: [min_x as i32, min_y as i32],
            extent: [(max_x - min_x) as u32, (max_y - min_y) as u32],
        })
    }
}

pub(super) fn update_scene_composite_scissors(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    output_extent: [u32; 2],
    commands: &mut [SceneGpuDrawCommand],
) -> Result<(), String> {
    if commands.len() != graph.mesh_draws.len() {
        return Err(format!(
            "scene composite scissor draw count {} does not match command count {}",
            graph.mesh_draws.len(),
            commands.len()
        ));
    }
    for command in commands.iter_mut() {
        command.scissor = None;
    }
    let consumer_cull_enabled = *COMPOSITE_CONSUMER_CULL_ENABLED.get_or_init(|| {
        std::env::var_os("GILDER_NATIVE_VULKAN_DISABLE_COMPOSITE_CONSUMER_CULL").is_none()
    });
    let mut graph_pass_start = 0usize;
    while graph_pass_start < graph.pass_nodes.len() {
        let graph_index = graph.pass_nodes[graph_pass_start].graph_index;
        let graph_pass_end = graph.pass_nodes[graph_pass_start..]
            .iter()
            .position(|pass| pass.graph_index != graph_index)
            .map_or(graph.pass_nodes.len(), |offset| graph_pass_start + offset);
        let graph_passes = &graph.pass_nodes[graph_pass_start..graph_pass_end];
        let has_object_composite = !consumer_cull_enabled
            || graph_passes
                .iter()
                .any(|pass| pass_shader_is(storage, pass, "we/objectcomposite"));
        let has_flat_rounded_composite = graph_passes
            .iter()
            .any(|pass| pass_shader_is(storage, pass, "we/flat-rounded-mask-composite"));
        if consumer_cull_enabled && !has_object_composite && !has_flat_rounded_composite {
            graph_pass_start = graph_pass_end;
            continue;
        }
        let mut bounds = None::<PixelBounds>;
        let mut coverage_is_bounded = true;
        for pass in graph_passes {
            let start = pass.mesh_draw_start as usize;
            let end = start.saturating_add(pass.mesh_draw_count as usize);
            let draws = graph.mesh_draws.get(start..end).unwrap_or(&[]);
            if has_object_composite {
                for draw in draws.iter().filter(|draw| {
                    draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
                }) {
                    let Some(draw_bounds) =
                        object_mesh_pixel_bounds(storage, graph, draw, output_extent)
                    else {
                        coverage_is_bounded = false;
                        continue;
                    };
                    if let Some(bounds) = &mut bounds {
                        bounds.include(draw_bounds);
                    } else {
                        bounds = Some(draw_bounds);
                    }
                }
            }
            if draws.iter().all(|draw| {
                !matches!(
                    draw.primitive,
                    SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
                        | SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
                )
            }) {
                continue;
            }
            let Some(shader) = pass_shader(storage, pass) else {
                coverage_is_bounded = false;
                continue;
            };
            if shader.eq_ignore_ascii_case("we/flat-rounded-mask-composite") {
                    let scissors = draws
                        .iter()
                        .map(|draw| {
                            flat_rounded_mask_pixel_bounds(storage, draw, output_extent)
                                .and_then(|bounds| bounds.scissor(output_extent))
                        })
                        .collect::<Vec<_>>();
                    if std::env::var("GILDER_NATIVE_VULKAN_SCENE_SCISSOR_DEBUG")
                        .ok()
                        .is_some_and(|requested| {
                            requested == "all" || requested == graph_index.to_string()
                        })
                        && !SCISSOR_DIAGNOSTIC_EMITTED
                            .swap(true, std::sync::atomic::Ordering::Relaxed)
                    {
                        eprintln!(
                            "gilder-flat-rounded-mask-scissor: graph={graph_index} scissors={scissors:?}"
                        );
                    }
                    for (command, scissor) in commands
                        .get_mut(start..end)
                        .unwrap_or(&mut [])
                        .iter_mut()
                        .zip(scissors)
                    {
                        command.scissor = scissor;
                    }
            } else if shader.eq_ignore_ascii_case("we/objectcomposite") {
                    let scissor = coverage_is_bounded
                        .then(|| bounds.and_then(|bounds| bounds.scissor(output_extent)))
                        .flatten();
                    if std::env::var("GILDER_NATIVE_VULKAN_SCENE_SCISSOR_DEBUG")
                        .ok()
                        .is_some_and(|requested| {
                            requested == "all" || requested == graph_index.to_string()
                        })
                        && !SCISSOR_DIAGNOSTIC_EMITTED
                            .swap(true, std::sync::atomic::Ordering::Relaxed)
                    {
                        eprintln!(
                            "gilder-scene-composite-scissor: graph={graph_index} bounded={coverage_is_bounded} bounds={bounds:?} scissor={scissor:?}"
                        );
                    }
                    if let Some(scissor) = scissor {
                        for command in commands.get_mut(start..end).unwrap_or(&mut []) {
                            command.scissor = Some(scissor);
                        }
                    }
            } else if shader.eq_ignore_ascii_case("effects/waterwaves") {
                    for draw in draws {
                        let Some([x, y]) = waterwaves_pixel_margin(storage, draw, output_extent)
                        else {
                            coverage_is_bounded = false;
                            continue;
                        };
                        if let Some(bounds) = &mut bounds {
                            bounds.expand(x, y);
                        }
                    }
            } else if !shader.eq_ignore_ascii_case("minimalalpha")
                && !shader.eq_ignore_ascii_case("passthrough")
                && !shader.eq_ignore_ascii_case("effects/opacity")
            {
                coverage_is_bounded = false;
            }
        }
        graph_pass_start = graph_pass_end;
    }
    Ok(())
}

fn pass_shader<'a>(
    storage: &'a SceneStorage,
    pass: &crate::engine::scene::SceneRenderingDevicePassNode,
) -> Option<&'a str> {
    let record = storage
        .document()
        .render_passes
        .get(pass.pass_record_index as usize)?;
    storage.string(record.shader_key)?.split("__").next()
}

fn pass_shader_is(
    storage: &SceneStorage,
    pass: &crate::engine::scene::SceneRenderingDevicePassNode,
    expected: &str,
) -> bool {
    pass_shader(storage, pass).is_some_and(|shader| shader.eq_ignore_ascii_case(expected))
}

fn object_mesh_pixel_bounds(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<PixelBounds> {
    let mesh = storage.meshes().get(draw.mesh_index as usize)?;
    let transform =
        scene_cover_clip_transform(storage.project(), output_extent, draw.clip_transform);
    let mut bounds = None::<PixelBounds>;
    for vertex in storage.mesh_vertices(mesh) {
        let local = skinned_vertex_position(graph, draw, vertex)?;
        let clip = multiply_rows(transform, local);
        if !clip.iter().all(|value| value.is_finite()) || clip[3].abs() <= 1.0e-7 {
            return None;
        }
        let pixel = [
            (clip[0] / clip[3] * 0.5 + 0.5) * output_extent[0] as f32,
            (clip[1] / clip[3] * 0.5 + 0.5) * output_extent[1] as f32,
        ];
        let point = PixelBounds {
            min_x: pixel[0],
            min_y: pixel[1],
            max_x: pixel[0],
            max_y: pixel[1],
        };
        if let Some(bounds) = &mut bounds {
            bounds.include(point);
        } else {
            bounds = Some(point);
        }
    }
    bounds
}

pub(super) fn object_mesh_covers_output(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> bool {
    let Some(mesh) = storage.meshes().get(draw.mesh_index as usize) else {
        return false;
    };
    if output_extent.contains(&0)
        || draw.vertex_start != mesh.vertex_start
        || draw.vertex_count != 4
        || mesh.vertex_count != 4
        || draw.index_start != mesh.index_start
        || draw.index_count != 6
        || mesh.index_count != 6
    {
        return false;
    }
    let transform =
        scene_cover_clip_transform(storage.project(), output_extent, draw.clip_transform);
    let mut points = [[0.0; 2]; 4];
    for (point, vertex) in points.iter_mut().zip(storage.mesh_vertices(mesh)) {
        let Some(local) = skinned_vertex_position(graph, draw, vertex) else {
            return false;
        };
        let clip = multiply_rows(transform, local);
        if !clip.iter().all(|value| value.is_finite()) || clip[3] <= 1.0e-7 {
            return false;
        }
        *point = [
            (clip[0] / clip[3] * 0.5 + 0.5) * output_extent[0] as f32,
            (clip[1] / clip[3] * 0.5 + 0.5) * output_extent[1] as f32,
        ];
    }
    rectangle_mesh_covers_pixel_centers(
        points,
        storage.mesh_indices(mesh),
        output_extent,
    )
}

fn rectangle_mesh_covers_pixel_centers(
    points: [[f32; 2]; 4],
    indices: &[u32],
    output_extent: [u32; 2],
) -> bool {
    if indices.len() != 6 || output_extent.contains(&0) {
        return false;
    }
    let min_x = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max);
    if ![min_x, min_y, max_x, max_y]
        .iter()
        .all(|value| value.is_finite())
        || min_x > 0.5
        || min_y > 0.5
        || max_x < output_extent[0] as f32 - 0.5
        || max_y < output_extent[1] as f32 - 0.5
    {
        return false;
    }
    let tolerance = ((max_x - min_x).max(max_y - min_y) * 1.0e-6).max(1.0e-4);
    let mut corner_for_vertex = [u8::MAX; 4];
    for (vertex, point) in points.iter().enumerate() {
        let x = if (point[0] - min_x).abs() <= tolerance {
            0
        } else if (point[0] - max_x).abs() <= tolerance {
            1
        } else {
            return false;
        };
        let y = if (point[1] - min_y).abs() <= tolerance {
            0
        } else if (point[1] - max_y).abs() <= tolerance {
            2
        } else {
            return false;
        };
        corner_for_vertex[vertex] = x | y;
    }
    let mut corners = corner_for_vertex;
    corners.sort_unstable();
    if corners != [0, 1, 2, 3] {
        return false;
    }
    let mut triangles = [[0u8; 3]; 2];
    for (triangle, source) in triangles.iter_mut().zip(indices.chunks_exact(3)) {
        for (corner, index) in triangle.iter_mut().zip(source) {
            let Some(mapped) = corner_for_vertex.get(*index as usize) else {
                return false;
            };
            *corner = *mapped;
        }
        triangle.sort_unstable();
    }
    triangles.sort_unstable();
    matches!(
        triangles,
        [[0, 1, 2], [1, 2, 3]] | [[0, 1, 3], [0, 2, 3]]
    )
}

pub(super) fn object_mesh_pixel_extent(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[u32; 2]> {
    let bounds = object_mesh_pixel_bounds(storage, graph, draw, output_extent)?;
    let width = (bounds.max_x - bounds.min_x).ceil();
    let height = (bounds.max_y - bounds.min_y).ceil();
    (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0).then_some([
        (width as u32).clamp(2, output_extent[0].max(2)),
        (height as u32).clamp(2, output_extent[1].max(2)),
    ])
}

fn skinned_vertex_position(
    graph: &SceneRenderingDeviceGraphPlan,
    draw: &SceneRenderingDeviceMeshDraw,
    vertex: &crate::engine::scene::SceneMeshVertexRecord,
) -> Option<[f32; 4]> {
    let raw = [vertex.position.x, vertex.position.y, vertex.position.z, 1.0];
    if draw.skinning_palette_count == 0 {
        return Some(raw);
    }
    let mut skinned = [0.0; 4];
    let mut total_weight = 0.0;
    for slot in 0..4 {
        let weight = vertex.blend_weights[slot];
        if weight <= 1.0e-7 {
            continue;
        }
        let local_bone = vertex.blend_indices[slot];
        if local_bone >= draw.skinning_palette_count {
            return None;
        }
        let bone = graph
            .puppet_bone_matrices
            .get(draw.skinning_palette_start.saturating_add(local_bone) as usize)?;
        let position = multiply_rows(bone.matrix, raw);
        for lane in 0..4 {
            skinned[lane] += position[lane] * weight;
        }
        total_weight += weight;
    }
    if total_weight <= 1.0e-7 {
        return Some(raw);
    }
    for value in &mut skinned {
        *value /= total_weight;
    }
    Some(skinned)
}

fn waterwaves_pixel_margin(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[f32; 2]> {
    let strength = material_parameter_values(storage, draw.material, &["strength"])
        .first()
        .copied()
        .unwrap_or(0.1);
    if !strength.is_finite() {
        return None;
    }
    let linear = object_uv_to_screen_linear(storage, draw, output_extent)?;
    let displacement = strength.abs() * strength.abs();
    let margin_x = displacement * linear[0][0].hypot(linear[0][1]) * output_extent[0] as f32;
    let margin_y = displacement * linear[1][0].hypot(linear[1][1]) * output_extent[1] as f32;
    (margin_x.is_finite() && margin_y.is_finite())
        .then_some([margin_x.ceil() + 2.0, margin_y.ceil() + 2.0])
}

fn flat_rounded_mask_pixel_bounds(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<PixelBounds> {
    let size_values = material_parameter_values(storage, draw.material, &["Size"]);
    let size = [
        size_values.first().copied().unwrap_or(1.0),
        size_values.get(1).copied().unwrap_or(1.0),
    ];
    let softness = material_parameter_values(storage, draw.material, &["Softness"])
        .first()
        .copied()
        .unwrap_or(0.5);
    let object_pixel_extent = object_projected_pixel_extent(storage, draw, output_extent)?;
    let uv_bounds =
        flat_rounded_mask_uv_bounds(size, softness, object_pixel_extent, output_extent)?;
    let affine = object_uv_to_screen_affine(storage, draw, output_extent)?;
    let mut bounds = None::<PixelBounds>;
    for uv in [
        [uv_bounds.min[0], uv_bounds.min[1]],
        [uv_bounds.max[0], uv_bounds.min[1]],
        [uv_bounds.min[0], uv_bounds.max[1]],
        [uv_bounds.max[0], uv_bounds.max[1]],
    ] {
        let screen_uv = [
            affine[0][0] * uv[0] + affine[0][1] * uv[1] + affine[0][2],
            affine[1][0] * uv[0] + affine[1][1] * uv[1] + affine[1][2],
        ];
        let point = PixelBounds {
            min_x: screen_uv[0] * output_extent[0] as f32,
            min_y: screen_uv[1] * output_extent[1] as f32,
            max_x: screen_uv[0] * output_extent[0] as f32,
            max_y: screen_uv[1] * output_extent[1] as f32,
        };
        if let Some(bounds) = &mut bounds {
            bounds.include(point);
        } else {
            bounds = Some(point);
        }
    }
    bounds
}

fn multiply_rows(matrix: [[f32; 4]; 4], vector: [f32; 4]) -> [f32; 4] {
    matrix.map(|row| {
        row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2] + row[3] * vector[3]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneCompositeBlend,
        SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneObjectHandle, ScenePipelineBlend,
        SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
        SceneRenderingDevicePassNode, SceneStringId,
    };

    #[test]
    fn pixel_bounds_scissor_rounds_outward_and_clamps_to_target() {
        let scissor = PixelBounds {
            min_x: -8.5,
            min_y: 20.25,
            max_x: 90.1,
            max_y: 120.75,
        }
        .scissor([100, 100])
        .expect("visible bounds");

        assert_eq!(scissor.offset, [0, 18]);
        assert_eq!(scissor.extent, [93, 82]);
    }

    #[test]
    fn non_finite_bounds_force_full_target_fallback() {
        assert!(
            PixelBounds {
                min_x: f32::NAN,
                min_y: 0.0,
                max_x: 1.0,
                max_y: 1.0,
            }
            .scissor([100, 100])
            .is_none()
        );
    }

    #[test]
    fn complete_axis_aligned_quad_covers_output_pixel_centers() {
        assert!(rectangle_mesh_covers_pixel_centers(
            [[-10.0, -10.0], [110.0, -10.0], [-10.0, 110.0], [110.0, 110.0]],
            &[0, 1, 3, 0, 3, 2],
            [100, 100],
        ));
    }

    #[test]
    fn incomplete_or_rotated_quad_does_not_prove_full_output_coverage() {
        assert!(!rectangle_mesh_covers_pixel_centers(
            [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0], [100.0, 100.0]],
            &[0, 1, 2, 0, 1, 3],
            [100, 100],
        ));
        assert!(!rectangle_mesh_covers_pixel_centers(
            [[50.0, -50.0], [150.0, 50.0], [-50.0, 50.0], [50.0, 150.0]],
            &[0, 1, 3, 0, 3, 2],
            [100, 100],
        ));
    }

    #[test]
    fn composite_consumer_detection_normalizes_shader_variants() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["we/objectcomposite__TEST_1".to_owned()],
            render_passes: vec![SceneRenderPassRecord {
                id: 0,
                role: SceneRenderPassKind::SceneComposite,
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                scene_blend: SceneCompositeBlend::Alpha,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
            }],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let pass = SceneRenderingDevicePassNode {
            graph_index: 4,
            pass_record_index: 0,
            pass_id: 0,
            role: SceneRenderPassKind::SceneComposite,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            mesh_draw_start: 0,
            mesh_draw_count: 1,
        };

        assert!(pass_shader_is(&storage, &pass, "we/objectcomposite"));
        assert!(!pass_shader_is(
            &storage,
            &pass,
            "we/flat-rounded-mask-composite"
        ));
    }
}
