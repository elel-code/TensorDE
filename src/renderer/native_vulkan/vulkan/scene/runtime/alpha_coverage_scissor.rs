//! Conservative sparse scissors for transparent image-waterwaves composites.

use crate::engine::scene::{
    SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceGraphPlan, SceneStorage,
};

use super::draw_recording::SceneGpuScissor;
use super::draw_uniform::{object_uv_to_screen_affine, object_uv_to_screen_linear};
use super::material_uniform::material_parameter_values;

const WATERWAVES_FIELD_SHADER: &str = "we/waterwaves-uv-field";
const SPARSE_COMPOSITE_SHADERS: &[&str] = &[
    "we/image-waterwaves-composite",
    "we/image-waterwaves-multiply-composite",
];
const MAX_WATERWAVES_STAGES: usize = 7;

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
        if !SPARSE_COMPOSITE_SHADERS
            .iter()
            .any(|candidate| shader.eq_ignore_ascii_case(candidate))
        {
            continue;
        }
        let displacement = graph_waterwaves_displacement_cells(
            storage,
            graph,
            pass.graph_index,
            output_extent,
        );
        let start = pass.mesh_draw_start as usize;
        let end = start.saturating_add(pass.mesh_draw_count as usize);
        for (draw_index, draw) in graph
            .mesh_draws
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            if draw.primitive != SceneRenderingDeviceDrawPrimitive::ObjectMesh
                || draw.skinning_palette_count != 0
            {
                continue;
            }
            let Some(texture) = source_texture(storage, draw.material) else {
                continue;
            };
            let coverage = dilate_coverage(
                texture.alpha_coverage_rows,
                displacement[0],
                displacement[1],
            );
            if coverage.iter().all(|row| *row == u32::MAX) {
                continue;
            }
            let Some(affine) = object_uv_to_screen_affine(storage, draw, output_extent) else {
                continue;
            };
            if !axis_aligned(affine) {
                continue;
            }
            let scissors = coverage_scissors(coverage, affine, output_extent);
            if std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_DEBUG")
                .ok()
                .is_some_and(|value| value == pass.graph_index.to_string())
            {
                eprintln!(
                    "gilder-alpha-coverage: graph={} draw={} texture={} displacement={:?} affine={:?} coverage={:08x?} scissors={} pixels={} overlaps={}",
                    pass.graph_index,
                    start + draw_index,
                    texture.resource.0,
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
    output_extent: [u32; 2],
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
            .is_some_and(|shader| shader.eq_ignore_ascii_case(WATERWAVES_FIELD_SHADER))
    }) else {
        return [SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE; 2];
    };
    let Some(field_draw) = graph.mesh_draws.get(field_pass.mesh_draw_start as usize) else {
        return [SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE; 2];
    };
    let stage_count = material_parameter_values(
        storage,
        field_draw.material,
        &["waterwaves.stage_count"],
    )
    .first()
    .copied()
    .unwrap_or(0.0)
    .round()
    .clamp(0.0, MAX_WATERWAVES_STAGES as f32) as usize;
    let amplitude = (0..stage_count)
        .map(|stage| {
            let name = format!("waterwaves.{stage}.strength");
            let strength = material_parameter_values(
                storage,
                field_draw.material,
                &[name.as_str()],
            )
            .first()
            .copied()
            .unwrap_or(0.1)
            .abs();
            strength * strength
        })
        .sum::<f32>();
    let Some(linear) = object_uv_to_screen_linear(storage, field_draw, output_extent) else {
        return [SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE; 2];
    };
    let x = amplitude * linear[0][0].hypot(linear[0][1]);
    let y = amplitude * linear[1][0].hypot(linear[1][1]);
    let safety_cells = std::env::var("GILDER_NATIVE_VULKAN_SCENE_ALPHA_COVERAGE_PADDING")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .min(SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE);
    [
        (x * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32).ceil() as usize + safety_cells,
        (y * SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE as f32).ceil() as usize + safety_cells,
    ]
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

fn axis_aligned(affine: [[f32; 3]; 2]) -> bool {
    affine.iter().flatten().all(|value| value.is_finite())
        && affine[0][1].abs() <= 1.0e-6
        && affine[1][0].abs() <= 1.0e-6
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

fn coverage_rectangles(
    mut rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
) -> Vec<[usize; 4]> {
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
            while row_end < SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE
                && rows[row_end] & mask == mask
            {
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

        let scissors = coverage_scissors(
            rows,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [320, 160],
        );

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
    fn coverage_rectangles_merge_shared_vertical_runs() {
        let mut rows = [0u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE];
        rows[3] = 0b11110;
        rows[4] = 0b11110;
        rows[5] = 0b01110;

        let rectangles = coverage_rectangles(rows);

        assert_eq!(rectangles, vec![[1, 3, 5, 5], [1, 5, 4, 6]]);
    }
}
