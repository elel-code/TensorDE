//! Conservative sparse scissors for transparent image-waterwaves composites
//! and identity-sampled alpha-blended object images.
//!
//! Scene-color composites use projected object UV → surface scissors. Multipass
//! `image-local-*` sources/fields instead use authored-texture UV identity on the
//! local target extent (`gl_Position = vec4(a_TexCoord * 2 - 1, ...)`), so
//! coverage must never reuse surface affine there.
//!
//! Identity object images (`we/genericimage4` under standard alpha blend) sample
//! slot-0 UV without displacement; zero-alpha fragments contribute nothing to
//! the destination, so sparse coverage is stream-safe with only a bilinear
//! filter guard.

use crate::engine::scene::{
    SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE, SceneCompositeBlend, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode, SceneStorage,
};

use super::draw_recording::SceneGpuScissor;
use super::draw_uniform::object_uv_to_screen_affine;
use super::material_uniform::material_parameter_values;

const WATERWAVES_DISPLACEMENT_SHADERS: &[&str] = &[
    "we/waterwaves-uv-field",
    "we/image-waterwaves-direct",
    "we/image-waterwaves-multiply-direct",
    "we/puppet-waterwaves-direct",
    "we/effect-waterwaves-direct",
];
const SPARSE_COMPOSITE_SHADERS: &[&str] = &[
    "we/image-waterwaves-composite",
    "we/image-waterwaves-multiply-composite",
    "we/image-waterwaves-direct",
    "we/image-waterwaves-multiply-direct",
];
/// Identity UV sample + standard alpha blend: no authored displacement.
const IDENTITY_ALPHA_OBJECT_SHADERS: &[&str] = &["we/genericimage4"];
/// Final-program object draws with UV displacement bounded by strength².
/// Only exact keys under standard alpha blend (modulate / multiply excluded).
const FINAL_DISPLACED_ALPHA_OBJECT_SHADERS: &[&str] = &[
    "we/image-waterwaves-final",
    "we/image-waterripple-final",
];
/// Source assembly on local RT: UV covers the full target, so sparse coverage is
/// identity-mapped in authored-texture domain.
const LOCAL_SPARSE_SOURCE_SHADERS: &[&str] = &[
    "we/image-effect-source",
    "we/puppet-effect-source",
];
/// Field / multipass waterwaves stages that write a local RT while sampling the
/// sparse source. Fullscreen triangles are eligible only on local targets.
const LOCAL_SPARSE_FIELD_SHADERS: &[&str] = &[
    "we/effect-waterwaves-direct",
    "we/waterwaves-uv-field",
];
const MAX_WATERWAVES_STAGES: usize = 9;
const WATERWAVES_FILTER_GUARD_CELLS: usize = 2;
const LOCAL_IDENTITY_AFFINE: [[f32; 3]; 2] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

pub(super) fn scene_alpha_coverage_scissors(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    output_extent: [u32; 2],
) -> Vec<Vec<SceneGpuScissor>> {
    let mut draw_scissors = vec![Vec::new(); graph.mesh_draws.len()];
    if std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("off"))
    {
        return draw_scissors;
    }
    for pass in &graph.pass_nodes {
        let Some(pass_record) = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
        else {
            continue;
        };
        let Some(shader) = storage.string(pass_record.shader_key) else {
            continue;
        };
        let local_target = pass_is_local_effect_target(pass.target);
        let sparse_kind = sparse_coverage_kind(shader, local_target, pass_record.scene_blend);
        let Some(sparse_kind) = sparse_kind else {
            continue;
        };
        let coverage_extent = match sparse_kind {
            SparseCoverageKind::LocalSource | SparseCoverageKind::LocalField => {
                local_coverage_extent(graph, pass)
            }
            SparseCoverageKind::SceneComposite
            | SparseCoverageKind::IdentityObject
            | SparseCoverageKind::FinalDisplacedObject => Some(output_extent),
        };
        let Some(coverage_extent) = coverage_extent else {
            continue;
        };
        let start = pass.mesh_draw_start as usize;
        let end = start.saturating_add(pass.mesh_draw_count as usize);
        for (draw_index, draw) in graph
            .mesh_draws
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            if !draw_accepts_sparse_coverage(draw, sparse_kind) {
                continue;
            }
            let Some(texture) = source_texture_for_sparse_draw(storage, graph, pass, draw) else {
                continue;
            };
            let displacement = match sparse_kind {
                SparseCoverageKind::LocalSource | SparseCoverageKind::IdentityObject => {
                    [WATERWAVES_FILTER_GUARD_CELLS; 2]
                }
                SparseCoverageKind::LocalField | SparseCoverageKind::SceneComposite => {
                    graph_waterwaves_displacement_cells(storage, graph, pass.graph_index)
                }
                SparseCoverageKind::FinalDisplacedObject => {
                    final_program_displacement_cells(storage, draw, shader)
                }
            };
            let coverage = dilate_coverage(
                texture.alpha_coverage_rows,
                displacement[0],
                displacement[1],
            );
            if coverage.iter().all(|row| *row == u32::MAX) {
                continue;
            }
            let affine = match sparse_kind {
                SparseCoverageKind::LocalSource | SparseCoverageKind::LocalField => {
                    LOCAL_IDENTITY_AFFINE
                }
                SparseCoverageKind::SceneComposite
                | SparseCoverageKind::IdentityObject
                | SparseCoverageKind::FinalDisplacedObject => {
                    let Some(affine) = object_uv_to_screen_affine(storage, draw, coverage_extent)
                    else {
                        continue;
                    };
                    if !axis_aligned(affine) {
                        continue;
                    }
                    affine
                }
            };
            let scissors = coverage_scissors(coverage, affine, coverage_extent);
            if std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_DEBUG")
                .ok()
                .is_some_and(|value| value == pass.graph_index.to_string())
            {
                eprintln!(
                    "gilder-alpha-coverage: graph={} draw={} kind={:?} target={:?} texture={} path={:?} size={}x{} storage={}x{} coverage_extent={:?} displacement={:?} affine={:?} coverage={:08x?} scissors={} pixels={} overlaps={}",
                    pass.graph_index,
                    start + draw_index,
                    sparse_kind,
                    pass.target,
                    texture.resource.0,
                    storage
                        .resource(texture.resource)
                        .and_then(|resource| storage.string(resource.path)),
                    texture.width,
                    texture.height,
                    texture.storage_width,
                    texture.storage_height,
                    coverage_extent,
                    displacement,
                    affine,
                    coverage,
                    scissors.len(),
                    scissors
                        .iter()
                        .map(|scissor| u64::from(scissor.extent[0]) * u64::from(scissor.extent[1]))
                        .sum::<u64>(),
                    overlapping_scissor_pairs(&scissors),
                );
            }
            draw_scissors[start + draw_index] = scissors;
        }
    }
    draw_scissors
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SparseCoverageKind {
    /// Scene-color waterwaves composite / direct mesh path.
    SceneComposite,
    /// Scene-color identity UV object image under standard alpha blend.
    IdentityObject,
    /// Final waterwaves/waterripple object draw under standard alpha blend.
    FinalDisplacedObject,
    /// Local RT source assembly (`image-effect-source` family).
    LocalSource,
    /// Local RT field / multipass waterwaves write.
    LocalField,
}

fn sparse_coverage_kind(
    shader: &str,
    local_target: bool,
    scene_blend: SceneCompositeBlend,
) -> Option<SparseCoverageKind> {
    if local_target {
        if matches_shader_or_stage_variant(shader, LOCAL_SPARSE_SOURCE_SHADERS)
            || LOCAL_SPARSE_SOURCE_SHADERS
                .iter()
                .any(|base| shader.eq_ignore_ascii_case(base))
        {
            return Some(SparseCoverageKind::LocalSource);
        }
        if matches_shader_or_stage_variant(shader, LOCAL_SPARSE_FIELD_SHADERS) {
            return Some(SparseCoverageKind::LocalField);
        }
        return None;
    }
    if matches_shader_or_stage_variant(shader, SPARSE_COMPOSITE_SHADERS) {
        return Some(SparseCoverageKind::SceneComposite);
    }
    // Exact keys only. Multiply / modulate / screen blends can change the
    // destination under zero source alpha and are excluded.
    // Opt-out for formal A/B: GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_IDENTITY=off.
    let identity_enabled = !std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_IDENTITY")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("off"));
    if identity_enabled
        && scene_blend == SceneCompositeBlend::Alpha
        && IDENTITY_ALPHA_OBJECT_SHADERS
            .iter()
            .any(|base| shader.eq_ignore_ascii_case(base))
    {
        return Some(SparseCoverageKind::IdentityObject);
    }
    if identity_enabled
        && scene_blend == SceneCompositeBlend::Alpha
        && FINAL_DISPLACED_ALPHA_OBJECT_SHADERS
            .iter()
            .any(|base| shader.eq_ignore_ascii_case(base))
    {
        return Some(SparseCoverageKind::FinalDisplacedObject);
    }
    None
}

fn pass_is_local_effect_target(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain | SceneRenderTargetKind::ImageLocalSub
    )
}

fn local_coverage_extent(
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
) -> Option<[u32; 2]> {
    if let Some(allocation) = graph.target_allocations.iter().find(|allocation| {
        allocation.graph_index == pass.graph_index
            && allocation.target == pass.target
            && allocation.target_name == pass.target_name
            && allocation.width != 0
            && allocation.height != 0
    }) {
        return Some([allocation.width, allocation.height]);
    }
    let start = pass.mesh_draw_start as usize;
    let end = start.saturating_add(pass.mesh_draw_count as usize);
    graph
        .mesh_draws
        .get(start..end)
        .into_iter()
        .flatten()
        .find_map(|draw| {
            let width = draw.authored_source_extent[0].round();
            let height = draw.authored_source_extent[1].round();
            (width.is_finite()
                && height.is_finite()
                && width >= 1.0
                && height >= 1.0
                && width <= u32::MAX as f32
                && height <= u32::MAX as f32)
                .then_some([width as u32, height as u32])
        })
}

fn draw_accepts_sparse_coverage(
    draw: &SceneRenderingDeviceMeshDraw,
    kind: SparseCoverageKind,
) -> bool {
    if draw.skinning_palette_count != 0 {
        return false;
    }
    match kind {
        SparseCoverageKind::SceneComposite
        | SparseCoverageKind::IdentityObject
        | SparseCoverageKind::FinalDisplacedObject
        | SparseCoverageKind::LocalSource => {
            draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
        }
        SparseCoverageKind::LocalField => matches!(
            draw.primitive,
            SceneRenderingDeviceDrawPrimitive::ObjectMesh
                | SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        ),
    }
}

fn source_texture_for_sparse_draw<'a>(
    storage: &'a SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
    draw: &SceneRenderingDeviceMeshDraw,
) -> Option<&'a crate::engine::scene::SceneTextureRecord> {
    if let Some(texture) = source_texture(storage, draw.material) {
        return Some(texture);
    }
    // Fullscreen field stages often bind only previous-target / mask slots; reuse
    // the graph's object-mesh source material (image-effect-source) coverage.
    graph
        .pass_nodes
        .iter()
        .filter(|candidate| {
            candidate.graph_index == pass.graph_index
                && candidate.mesh_draw_count != 0
                && matches!(
                    candidate.target,
                    SceneRenderTargetKind::ImageLocalMain | SceneRenderTargetKind::ImageLocalSub
                )
        })
        .flat_map(|candidate| {
            let start = candidate.mesh_draw_start as usize;
            let end = start.saturating_add(candidate.mesh_draw_count as usize);
            graph.mesh_draws.get(start..end).into_iter().flatten()
        })
        .find(|candidate| {
            candidate.primitive == SceneRenderingDeviceDrawPrimitive::ObjectMesh
                && candidate.skinning_palette_count == 0
        })
        .and_then(|candidate| source_texture(storage, candidate.material))
}

fn source_texture<'a>(
    storage: &'a SceneStorage,
    material: crate::engine::scene::SceneMaterialHandle,
) -> Option<&'a crate::engine::scene::SceneTextureRecord> {
    let material = storage.material(material)?;
    storage
        .material_passes(material)
        .iter()
        .flat_map(|pass| storage.material_pass_textures(pass))
        .find(|binding| binding.slot == 0)
        .and_then(|binding| storage.texture(binding.resource))
}

fn graph_waterwaves_displacement_cells(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    graph_index: u32,
) -> [usize; 2] {
    let Some(field_pass) = graph.pass_nodes.iter().find(|pass| {
        if pass.graph_index != graph_index || pass.mesh_draw_count == 0 {
            return false;
        }
        storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .and_then(|record| storage.string(record.shader_key))
            .is_some_and(|shader| {
                matches_shader_or_stage_variant(shader, WATERWAVES_DISPLACEMENT_SHADERS)
            })
    }) else {
        return [SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE; 2];
    };
    let Some(field_draw) = graph.mesh_draws.get(field_pass.mesh_draw_start as usize) else {
        return [SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE; 2];
    };
    let stage_count =
        material_parameter_values(storage, field_draw.material, &["waterwaves.stage_count"])
            .first()
            .copied()
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, MAX_WATERWAVES_STAGES as f32) as usize;
    let amplitude = (0..stage_count).fold([0.0f32; 2], |mut amplitude, stage| {
        let name = format!("waterwaves.{stage}.strength");
        let strength = material_parameter_values(storage, field_draw.material, &[name.as_str()])
            .first()
            .copied()
            .unwrap_or(0.1)
            .abs();
        let name = format!("waterwaves.{stage}.direction");
        let direction = material_parameter_values(storage, field_draw.material, &[name.as_str()])
            .first()
            .copied()
            .unwrap_or(0.0);
        let displacement = waterwaves_stage_displacement(strength, direction);
        amplitude[0] += displacement[0];
        amplitude[1] += displacement[1];
        amplitude
    });
    let safety_cells = std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_PADDING")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(WATERWAVES_FILTER_GUARD_CELLS)
        .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    [
        (amplitude[0] * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32).ceil() as usize
            + safety_cells,
        (amplitude[1] * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32).ceil() as usize
            + safety_cells,
    ]
}

fn waterwaves_stage_displacement(strength: f32, direction: f32) -> [f32; 2] {
    let displacement = strength.abs() * strength.abs();
    [
        direction.cos().abs() * displacement,
        direction.sin().abs() * displacement,
    ]
}

/// Final-program UV displacement is bounded by `strength²` in texture UV
/// (waterwaves / waterripple sample offset). Expand coverage isotropically.
fn final_program_displacement_cells(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    shader: &str,
) -> [usize; 2] {
    let strength = if shader.eq_ignore_ascii_case("we/image-waterwaves-final") {
        material_parameter_values(storage, draw.material, &["effect.strength"])
            .first()
            .copied()
            .unwrap_or(0.1)
            .abs()
    } else {
        material_parameter_values(
            storage,
            draw.material,
            &["effect.ripplestrength", "effect.strength"],
        )
        .first()
        .copied()
        .unwrap_or(0.1)
        .abs()
    };
    let max_uv = strength * strength;
    let safety_cells = std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_PADDING")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(WATERWAVES_FILTER_GUARD_CELLS)
        .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    let cells = (max_uv * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32)
        .ceil()
        .max(0.0) as usize
        + safety_cells;
    let cells = cells.min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    [cells, cells]
}

fn axis_aligned(affine: [[f32; 3]; 2]) -> bool {
    affine.iter().flatten().all(|value| value.is_finite())
        && affine[0][1].abs() <= 1.0e-6
        && affine[1][0].abs() <= 1.0e-6
}

fn matches_shader_or_stage_variant(shader: &str, bases: &[&str]) -> bool {
    bases.iter().any(|base| {
        shader.eq_ignore_ascii_case(base)
            || shader
                .get(..base.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(base))
                && shader
                    .get(base.len()..)
                    .is_some_and(|suffix| suffix.starts_with("__STAGES_"))
    })
}

fn dilate_coverage(
    rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    horizontal_cells: usize,
    vertical_cells: usize,
) -> [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE] {
    let horizontal_cells = horizontal_cells.min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    let vertical_cells = vertical_cells.min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    let mut expanded = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
    for (row, bits) in rows.into_iter().enumerate() {
        let mut horizontal = bits;
        for shift in 1..=horizontal_cells {
            horizontal |= bits.wrapping_shl(shift as u32) | bits.wrapping_shr(shift as u32);
        }
        let start = row.saturating_sub(vertical_cells);
        let end = row
            .saturating_add(vertical_cells)
            .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE - 1);
        for target in expanded.iter_mut().take(end + 1).skip(start) {
            *target |= horizontal;
        }
    }
    expanded
}

fn coverage_scissors(
    rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    affine: [[f32; 3]; 2],
    output_extent: [u32; 2],
) -> Vec<SceneGpuScissor> {
    let x_boundaries = pixel_boundaries(affine[0][0], affine[0][2], output_extent[0]);
    let y_boundaries = pixel_boundaries(affine[1][1], affine[1][2], output_extent[1]);
    let mut scissors = Vec::new();
    for [column_start, row_start, column_end, row_end] in coverage_rectangles(rows) {
        let y0 = y_boundaries[row_start].min(y_boundaries[row_end]);
        let y1 = y_boundaries[row_start].max(y_boundaries[row_end]);
        if y1 <= y0 {
            continue;
        }
        let x0 = x_boundaries[column_start].min(x_boundaries[column_end]);
        let x1 = x_boundaries[column_start].max(x_boundaries[column_end]);
        if x1 > x0 {
            scissors.push(SceneGpuScissor {
                offset: [x0 as i32, y0 as i32],
                extent: [x1 - x0, y1 - y0],
            });
        }
    }
    scissors
}

fn coverage_rectangles(mut rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE]) -> Vec<[usize; 4]> {
    let mut rectangles = Vec::new();
    for row in 0..SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE {
        let mut column = 0usize;
        while column < SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE {
            if rows[row] & (1u32 << column) == 0 {
                column += 1;
                continue;
            }
            let column_start = column;
            while column < SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE
                && rows[row] & (1u32 << column) != 0
            {
                column += 1;
            }
            let column_end = column;
            let width = column_end - column_start;
            let mask = if width == SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE {
                u32::MAX
            } else {
                ((1u32 << width) - 1) << column_start
            };
            let mut row_end = row + 1;
            while row_end < SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE && rows[row_end] & mask == mask {
                row_end += 1;
            }
            for covered_row in &mut rows[row..row_end] {
                *covered_row &= !mask;
            }
            rectangles.push([column_start, row, column_end, row_end]);
        }
    }
    rectangles
}

fn pixel_boundaries(
    scale: f32,
    translation: f32,
    output_size: u32,
) -> [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE + 1] {
    std::array::from_fn(|index| {
        let uv = index as f32 / SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32;
        let pixel = (scale * uv + translation) * output_size as f32;
        (pixel - 0.5).ceil().clamp(0.0, output_size as f32) as u32
    })
}

fn overlapping_scissor_pairs(scissors: &[SceneGpuScissor]) -> usize {
    let mut overlaps = 0;
    for (index, left) in scissors.iter().enumerate() {
        for right in &scissors[index + 1..] {
            let separated = left.offset[0] + left.extent[0] as i32 <= right.offset[0]
                || right.offset[0] + right.extent[0] as i32 <= left.offset[0]
                || left.offset[1] + left.extent[1] as i32 <= right.offset[1]
                || right.offset[1] + right.extent[1] as i32 <= left.offset[1];
            overlaps += usize::from(!separated);
        }
    }
    overlaps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_scissors_partition_horizontal_runs() {
        let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        rows[4] = 0b1110;

        let scissors = coverage_scissors(rows, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], [320, 160]);

        assert_eq!(scissors.len(), 1);
        assert_eq!(scissors[0].offset, [10, 20]);
        assert_eq!(scissors[0].extent, [30, 5]);
    }

    #[test]
    fn displacement_dilation_expands_both_axes() {
        let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        rows[10] = 1 << 10;

        let expanded = dilate_coverage(rows, 2, 1);

        assert_eq!(expanded[9] & 0b1_1111 << 8, 0b1_1111 << 8);
        assert_eq!(expanded[10] & 0b1_1111 << 8, 0b1_1111 << 8);
        assert_eq!(expanded[11] & 0b1_1111 << 8, 0b1_1111 << 8);
    }

    #[test]
    fn waterwaves_displacement_stays_in_texture_uv_space() {
        let horizontal = waterwaves_stage_displacement(0.08, 0.0);
        let vertical = waterwaves_stage_displacement(0.08, std::f32::consts::FRAC_PI_2);

        assert!((horizontal[0] - 0.0064).abs() < 0.000_001);
        assert!(horizontal[1] < 0.000_001);
        assert!(vertical[0] < 0.000_001);
        assert!((vertical[1] - 0.0064).abs() < 0.000_001);
        assert_eq!(WATERWAVES_FILTER_GUARD_CELLS, 2);
    }

    #[test]
    fn coverage_rectangles_merge_shared_vertical_runs() {
        let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        rows[3] = 0b11110;
        rows[4] = 0b11110;
        rows[5] = 0b01110;

        let rectangles = coverage_rectangles(rows);

        assert_eq!(rectangles, vec![[1, 3, 5, 5], [1, 5, 4, 6]]);
    }

    #[test]
    fn stage_specialized_direct_shader_keeps_sparse_coverage() {
        assert!(matches_shader_or_stage_variant(
            "we/image-waterwaves-direct__STAGES_7",
            SPARSE_COMPOSITE_SHADERS,
        ));
        assert!(!matches_shader_or_stage_variant(
            "we/image-waterwaves-direct__UNKNOWN_7",
            SPARSE_COMPOSITE_SHADERS,
        ));
    }

    #[test]
    fn local_target_sparse_kinds_are_separated_from_scene_composite() {
        assert_eq!(
            sparse_coverage_kind("we/image-effect-source", true, SceneCompositeBlend::Alpha),
            Some(SparseCoverageKind::LocalSource)
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/effect-waterwaves-direct__STAGES_2",
                true,
                SceneCompositeBlend::Alpha
            ),
            Some(SparseCoverageKind::LocalField)
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterwaves-direct__STAGES_2",
                false,
                SceneCompositeBlend::Alpha
            ),
            Some(SparseCoverageKind::SceneComposite)
        );
        // Scene-color multipass composite stays on the surface path only.
        assert_eq!(
            sparse_coverage_kind(
                "we/image-effect-composite",
                false,
                SceneCompositeBlend::Alpha
            ),
            None
        );
        assert_eq!(
            sparse_coverage_kind("we/image-effect-source", false, SceneCompositeBlend::Alpha),
            None
        );
        // Must not apply surface composite scissors on local targets.
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterwaves-direct__STAGES_2",
                true,
                SceneCompositeBlend::Alpha
            ),
            None
        );
    }

    #[test]
    fn identity_genericimage4_accepts_alpha_blend_only() {
        assert_eq!(
            sparse_coverage_kind("we/genericimage4", false, SceneCompositeBlend::Alpha),
            Some(SparseCoverageKind::IdentityObject)
        );
        assert_eq!(
            sparse_coverage_kind("we/genericimage4", false, SceneCompositeBlend::Modulate),
            None
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/genericimage4-multiply-composite",
                false,
                SceneCompositeBlend::Alpha
            ),
            None
        );
        assert_eq!(
            sparse_coverage_kind("we/genericimage4", true, SceneCompositeBlend::Alpha),
            None
        );
    }

    #[test]
    fn final_displaced_alpha_object_accepts_waterwaves_and_ripple() {
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterwaves-final",
                false,
                SceneCompositeBlend::Alpha
            ),
            Some(SparseCoverageKind::FinalDisplacedObject)
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterripple-final",
                false,
                SceneCompositeBlend::Alpha
            ),
            Some(SparseCoverageKind::FinalDisplacedObject)
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterripple-modulate-final",
                false,
                SceneCompositeBlend::Modulate
            ),
            None
        );
        assert_eq!(
            sparse_coverage_kind(
                "we/image-waterripple-modulate-final",
                false,
                SceneCompositeBlend::Alpha
            ),
            None
        );
    }

    #[test]
    fn final_program_displacement_uses_strength_squared() {
        // strength 0.1 → 0.01 UV → ceil(0.01*32)+2 = 1+2 = 3
        let cells = {
            let max_uv = 0.1f32 * 0.1;
            let cells = (max_uv * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32).ceil() as usize
                + WATERWAVES_FILTER_GUARD_CELLS;
            [cells, cells]
        };
        assert_eq!(cells, [3, 3]);
    }

    #[test]
    fn local_identity_coverage_uses_target_extent_not_surface() {
        let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        // Cover the full UV domain so scissors become the whole local target.
        rows.fill(u32::MAX);
        let local = [2318u32, 1794u32];
        let scissors = coverage_scissors(rows, LOCAL_IDENTITY_AFFINE, local);
        assert_eq!(scissors.len(), 1);
        assert_eq!(scissors[0].offset, [0, 0]);
        assert_eq!(scissors[0].extent, local);

        // Sparse run still maps into local pixel space, not 4K surface space.
        rows.fill(0);
        rows[0] = 0b1;
        let sparse = coverage_scissors(rows, LOCAL_IDENTITY_AFFINE, local);
        assert_eq!(sparse.len(), 1);
        assert_eq!(sparse[0].offset, [0, 0]);
        assert!(sparse[0].extent[0] < local[0]);
        assert!(sparse[0].extent[1] < local[1]);
        assert!(sparse[0].extent[0] > 0);
        assert!(sparse[0].extent[1] > 0);
    }

    #[test]
    fn displacement_list_includes_multipass_effect_waterwaves_direct() {
        assert!(matches_shader_or_stage_variant(
            "we/effect-waterwaves-direct__STAGES_2",
            WATERWAVES_DISPLACEMENT_SHADERS,
        ));
    }
}
