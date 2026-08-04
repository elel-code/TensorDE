//! Read-only queries over a completed RenderingDevice graph plan.

use super::*;

impl SceneRenderingDeviceGraphPlan {
    pub fn fullscreen_utility_draw_count(&self) -> usize {
        self.mesh_draws
            .iter()
            .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
            .count()
    }

    pub fn uses_fullscreen_utility_primitive(&self) -> bool {
        self.fullscreen_utility_draw_count() != 0
    }

    pub fn scene_owned_utility_quad_draw_count(&self, storage: &SceneStorage) -> usize {
        self.mesh_draws
            .iter()
            .filter(|draw| {
                draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
                    && storage
                        .shader_program(draw.shader_key, SceneShaderStage::Vertex)
                        .is_some()
            })
            .count()
    }

    pub fn bound_utility_vertex_count(&self, storage: &SceneStorage) -> usize {
        self.fullscreen_utility_draw_count()
            .saturating_mul(3)
            .saturating_add(
                self.scene_owned_utility_quad_draw_count(storage)
                    .saturating_mul(6),
            )
    }

    pub fn effect_batch_atlas_tile(
        &self,
        graph_index: u32,
        target: SceneRenderTargetKind,
        target_name: SceneStringId,
    ) -> Option<u32> {
        self.effect_batch_instances
            .iter()
            .find(|instance| {
                instance.graph_index == graph_index
                    && instance.target == target
                    && instance.target_name == target_name
            })
            .map(|instance| instance.atlas_tile)
    }

    pub fn effect_batch_field_count(&self, physical_slot: u32) -> u32 {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or(1, |batch| batch.layer_count.max(1))
    }

    pub fn effect_batch_atlas_grid(&self, physical_slot: u32) -> [u32; 2] {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or([1, 1], |batch| {
                [batch.atlas_columns.max(1), batch.atlas_rows.max(1)]
            })
    }

    pub fn effect_batch_field_extent_divisor(&self, physical_slot: u32) -> u32 {
        self.effect_batches
            .iter()
            .find(|batch| batch.physical_slot == physical_slot)
            .map_or(1, |batch| batch.field_extent_divisor.max(1))
    }
}
