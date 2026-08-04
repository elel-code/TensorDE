//! Scene-level GPU effect-family batching.
//!
//! ECS and convert retain semantic ownership of effect instances. This module only recognizes
//! typed RenderingDevice passes whose inputs are independent of scene-color ordering, then assigns
//! one array layer per instance so the command executor can generate the complete family in one GPU pass.

use serde::{Deserialize, Serialize};

use super::{
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
    SceneRenderingDeviceTargetAllocation,
};
use crate::engine::scene::{
    INVALID_OBJECT_ID, SceneMaterialHandle, SceneRenderPassKind, SceneRenderTargetKind,
    SceneStorage, SceneStringId,
};

const WATERWAVES_UV_FIELD_SHADER: &str = "we/waterwaves-uv-field";
const SCENE_EFFECT_BATCH_ENABLED: bool = true;
const WATERWAVES_BATCH_FIELD_EXTENT_DIVISOR_ENV: &str =
    "TENSOR_WALLPAPER_RENDERING_DEVICE_WATERWAVES_FIELD_DIVISOR";
const DEFAULT_WATERWAVES_BATCH_FIELD_EXTENT_DIVISOR: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderingDeviceEffectBatchFamily {
    WaterWavesUvField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceEffectBatch {
    pub family: SceneRenderingDeviceEffectBatchFamily,
    pub physical_slot: u32,
    pub instance_start: u32,
    pub instance_count: u32,
    pub layer_count: u32,
    pub atlas_columns: u32,
    pub atlas_rows: u32,
    pub field_extent_divisor: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceEffectBatchInstance {
    pub family: SceneRenderingDeviceEffectBatchFamily,
    pub graph_index: u32,
    pub pass_node_index: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub physical_slot: u32,
    pub atlas_tile: u32,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Candidate {
    family: SceneRenderingDeviceEffectBatchFamily,
    graph_index: u32,
    pass_node_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    physical_slot: u32,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
    material: SceneMaterialHandle,
    effect_binding_start: u32,
    effect_binding_count: u32,
}

pub(super) fn build_scene_effect_batches(
    storage: &SceneStorage,
    passes: &[SceneRenderingDevicePassNode],
    allocations: &[SceneRenderingDeviceTargetAllocation],
    draws: &mut [SceneRenderingDeviceMeshDraw],
) -> (
    Vec<SceneRenderingDeviceEffectBatch>,
    Vec<SceneRenderingDeviceEffectBatchInstance>,
) {
    for draw in draws.iter_mut() {
        draw.effect_batch_atlas_tile = INVALID_OBJECT_ID;
        draw.effect_batch_atlas_grid = [0; 2];
    }
    if !SCENE_EFFECT_BATCH_ENABLED {
        return (Vec::new(), Vec::new());
    }
    let candidates = passes
        .iter()
        .enumerate()
        .filter_map(|(pass_node_index, pass)| {
            effect_batch_candidate(storage, pass_node_index, pass, allocations, draws)
        })
        .collect::<Vec<_>>();

    let mut batches = Vec::new();
    let mut instances = Vec::new();
    for candidate in candidates.iter().copied() {
        if batches
            .iter()
            .any(|batch: &SceneRenderingDeviceEffectBatch| {
                batch.family == candidate.family && batch.physical_slot == candidate.physical_slot
            })
        {
            continue;
        }
        let family_candidates = candidates
            .iter()
            .copied()
            .filter(|member| {
                member.family == candidate.family && member.physical_slot == candidate.physical_slot
            })
            .collect::<Vec<_>>();
        if family_candidates.len() < 2
            || physical_slot_has_unbatched_targets(
                candidate.physical_slot,
                allocations,
                &family_candidates,
            )
        {
            continue;
        }
        let instance_start = instances.len() as u32;
        let mut layer_representatives = Vec::<Candidate>::new();
        for member in family_candidates {
            let atlas_tile = layer_representatives
                .iter()
                .position(|representative| {
                    representative.effect_binding_start == member.effect_binding_start
                        && representative.effect_binding_count == member.effect_binding_count
                        && materials_generate_identical_fields(
                            storage,
                            representative.material,
                            member.material,
                        )
                })
                .unwrap_or_else(|| {
                    layer_representatives.push(member);
                    layer_representatives.len() - 1
                });
            let instance = SceneRenderingDeviceEffectBatchInstance {
                family: member.family,
                graph_index: member.graph_index,
                pass_node_index: member.pass_node_index,
                target: member.target,
                target_name: member.target_name,
                physical_slot: member.physical_slot,
                atlas_tile: atlas_tile as u32,
                mesh_draw_start: member.mesh_draw_start,
                mesh_draw_count: member.mesh_draw_count,
            };
            instances.push(instance);
        }
        let layer_count = layer_representatives.len() as u32;
        let [atlas_columns, atlas_rows] = effect_atlas_grid(layer_count);
        for instance in instances.iter().skip(instance_start as usize) {
            for pass in passes.iter().filter(|pass| {
                pass.graph_index == instance.graph_index
                    && (pass.pass_record_index
                        == passes[instance.pass_node_index as usize].pass_record_index
                        || pass.target == SceneRenderTargetKind::SceneColor)
            }) {
                for draw_index in
                    pass.mesh_draw_start..pass.mesh_draw_start.saturating_add(pass.mesh_draw_count)
                {
                    if let Some(draw) = draws.get_mut(draw_index as usize) {
                        draw.effect_batch_atlas_tile = instance.atlas_tile;
                        draw.effect_batch_atlas_grid = [atlas_columns, atlas_rows];
                    }
                }
            }
        }
        batches.push(SceneRenderingDeviceEffectBatch {
            family: candidate.family,
            physical_slot: candidate.physical_slot,
            instance_start,
            instance_count: instances.len() as u32 - instance_start,
            layer_count,
            atlas_columns,
            atlas_rows,
            field_extent_divisor: waterwaves_batch_field_extent_divisor(),
        });
    }
    (batches, instances)
}

fn waterwaves_batch_field_extent_divisor() -> u32 {
    let requested = std::env::var(WATERWAVES_BATCH_FIELD_EXTENT_DIVISOR_ENV).ok();
    parse_waterwaves_batch_field_extent_divisor(requested.as_deref())
}

fn parse_waterwaves_batch_field_extent_divisor(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| matches!(value, 1 | 2 | 4))
        .unwrap_or(DEFAULT_WATERWAVES_BATCH_FIELD_EXTENT_DIVISOR)
}

fn effect_atlas_grid(layer_count: u32) -> [u32; 2] {
    let layer_count = layer_count.max(1);
    let mut columns = 1u32;
    while columns.saturating_mul(columns) < layer_count {
        columns = columns.saturating_add(1);
    }
    [columns, layer_count.div_ceil(columns)]
}

fn effect_batch_candidate(
    storage: &SceneStorage,
    pass_node_index: usize,
    pass: &SceneRenderingDevicePassNode,
    allocations: &[SceneRenderingDeviceTargetAllocation],
    draws: &[SceneRenderingDeviceMeshDraw],
) -> Option<Candidate> {
    if pass.role != SceneRenderPassKind::EffectMaterial
        || pass.target != SceneRenderTargetKind::Temporary
        || pass.mesh_draw_count != 1
    {
        return None;
    }
    let pass_record = storage
        .document()
        .render_passes
        .get(pass.pass_record_index as usize)?;
    let shader = storage.string(pass_record.shader_key)?;
    let family = shader
        .eq_ignore_ascii_case(WATERWAVES_UV_FIELD_SHADER)
        .then_some(SceneRenderingDeviceEffectBatchFamily::WaterWavesUvField)?;
    if draws.get(pass.mesh_draw_start as usize)?.primitive
        != SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    {
        return None;
    }
    let allocation = allocations.iter().find(|allocation| {
        allocation.graph_index == pass.graph_index
            && allocation.target == pass.target
            && allocation.target_name == pass.target_name
    })?;
    Some(Candidate {
        family,
        graph_index: pass.graph_index,
        pass_node_index: pass_node_index as u32,
        target: pass.target,
        target_name: pass.target_name,
        physical_slot: allocation.physical_slot,
        mesh_draw_start: pass.mesh_draw_start,
        mesh_draw_count: pass.mesh_draw_count,
        material: draws.get(pass.mesh_draw_start as usize)?.material,
        effect_binding_start: pass.effect_binding_start,
        effect_binding_count: pass.effect_binding_count,
    })
}

fn materials_generate_identical_fields(
    storage: &SceneStorage,
    left: SceneMaterialHandle,
    right: SceneMaterialHandle,
) -> bool {
    if left == right {
        return true;
    }
    let Some(left_material) = storage.material(left) else {
        return false;
    };
    let Some(right_material) = storage.material(right) else {
        return false;
    };
    let Some(left_pass) = storage.material_passes(left_material).first() else {
        return false;
    };
    let Some(right_pass) = storage.material_passes(right_material).first() else {
        return false;
    };
    if left_pass.shader_key != right_pass.shader_key
        || left_pass.pipeline_blend != right_pass.pipeline_blend
        || left_pass.depth_test != right_pass.depth_test
        || left_pass.depth_write != right_pass.depth_write
        || left_pass.cull_mode != right_pass.cull_mode
    {
        return false;
    }
    let left_constants =
        material_constants(storage, left_pass.constant_start, left_pass.constant_count);
    let right_constants = material_constants(
        storage,
        right_pass.constant_start,
        right_pass.constant_count,
    );
    storage.material_pass_textures(left_pass) == storage.material_pass_textures(right_pass)
        && left_constants == right_constants
}

fn material_constants(
    storage: &SceneStorage,
    start: u32,
    count: u32,
) -> &[crate::engine::scene::SceneMaterialConstantRecord] {
    let start = start as usize;
    let end = start.saturating_add(count as usize);
    storage
        .document()
        .material_constants
        .get(start..end)
        .unwrap_or(&[])
}

fn physical_slot_has_unbatched_targets(
    physical_slot: u32,
    allocations: &[SceneRenderingDeviceTargetAllocation],
    candidates: &[Candidate],
) -> bool {
    allocations
        .iter()
        .filter(|allocation| allocation.physical_slot == physical_slot)
        .any(|allocation| {
            !candidates.iter().any(|candidate| {
                candidate.graph_index == allocation.graph_index
                    && candidate.target == allocation.target
                    && candidate.target_name == allocation.target_name
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waterwaves_field_divisor_defaults_to_four_and_accepts_bounded_diagnostics() {
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(None), 4);
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(Some("1")), 1);
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(Some(" 2 ")), 2);
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(Some("4")), 4);
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(Some("0")), 4);
        assert_eq!(parse_waterwaves_batch_field_extent_divisor(Some("8")), 4);
    }
}
