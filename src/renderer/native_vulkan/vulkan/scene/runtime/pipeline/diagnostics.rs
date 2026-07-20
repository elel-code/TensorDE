//! Opt-in diagnostics for resolved scene pipeline state.

use vulkanalia::vk;

use crate::engine::scene::{SceneRenderingDeviceGraphPlan, SceneStorage};

use super::{
    SceneEffectTargetImagePlan, pass_pipeline_samples, pass_target_format, scene_gpu_blend,
};

pub(in crate::renderer::native_vulkan) fn emit_scene_pipeline_diagnostics_if_requested(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    pipeline_indices: &[u32],
    scene_color_msaa_enabled: bool,
) -> Result<(), String> {
    const ENV: &str = "GILDER_NATIVE_VULKAN_SCENE_PIPELINE_DEBUG";
    let Ok(requested) = std::env::var(ENV) else {
        return Ok(());
    };
    let requested = requested.trim();
    let graph_filter =
        if requested.eq_ignore_ascii_case("all") || requested == "1" {
            None
        } else {
            Some(requested.parse::<u32>().map_err(|_| {
                format!("{ENV} must be a graph index, 1, or all; got {requested:?}")
            })?)
        };
    for pass in graph.pass_nodes.iter().filter(|pass| {
        pass.mesh_draw_count != 0
            && graph_filter.is_none_or(|graph_index| pass.graph_index == graph_index)
    }) {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        let blend = scene_gpu_blend(storage, pass_record, pass.target);
        let target_format = pass_target_format(graph, pass, swapchain_format, effect_target_plans)?;
        let draw_start = pass.mesh_draw_start as usize;
        let draw_end = draw_start.saturating_add(pass.mesh_draw_count as usize);
        let draw_pipeline_indices =
            pipeline_indices.get(draw_start..draw_end).ok_or_else(|| {
                format!(
                    "scene graph {} pass {} draw range {}..{} exceeds pipeline index plan",
                    pass.graph_index, pass.pass_id, draw_start, draw_end
                )
            })?;
        let pipeline_index = draw_pipeline_indices.first().copied().unwrap_or_default();
        if draw_pipeline_indices
            .iter()
            .any(|candidate| *candidate != pipeline_index)
        {
            return Err(format!(
                "scene graph {} pass {} resolves to multiple pipeline indices",
                pass.graph_index, pass.pass_id
            ));
        }
        let material_pass = storage.material(pass_record.material).and_then(|material| {
            let passes = storage.material_passes(material);
            passes
                .iter()
                .find(|material_pass| material_pass.shader_key == pass_record.shader_key)
                .or_else(|| passes.first())
        });
        let material_textures = material_pass
            .map(|material_pass| storage.material_pass_textures(material_pass))
            .unwrap_or_default()
            .iter()
            .map(|binding| {
                storage.texture(binding.resource).map_or_else(
                    || {
                        format!(
                            "slot{}=resource{}:<missing>",
                            binding.slot, binding.resource.0
                        )
                    },
                    |texture| {
                        format!(
                            "slot{}=resource{}:{}x{}/storage{}x{}",
                            binding.slot,
                            binding.resource.0,
                            texture.width,
                            texture.height,
                            texture.storage_width,
                            texture.storage_height,
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let material_constants = material_pass
            .and_then(|material_pass| {
                let start = material_pass.constant_start as usize;
                let end = start.saturating_add(material_pass.constant_count as usize);
                storage.document().material_constants.get(start..end)
            })
            .unwrap_or_default()
            .iter()
            .map(|constant| {
                format!(
                    "{}={}",
                    storage.string(constant.name).unwrap_or("<missing>"),
                    storage.string(constant.value_json).unwrap_or("<missing>"),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let draw_objects = graph.mesh_draws[draw_start..draw_end]
            .iter()
            .map(|draw| {
                let name = storage
                    .objects()
                    .get(draw.object.0 as usize)
                    .and_then(|object| storage.string(object.name))
                    .unwrap_or("<unnamed>");
                format!("{}:{name}", draw.object.0)
            })
            .collect::<Vec<_>>()
            .join(",");
        eprintln!(
            "gilder-scene-pipeline: graph={} pass={} record={} draws={}..{} objects=[{}] target={:?}:{:?} shader={:?} pipeline_blend={:?} scene_blend={:?} resolved_blend={} cull_mode={:?} color_write_mask={:?} target_format={:?} samples={:?} pipeline_index={} material_textures=[{}] material_constants=[{}]",
            pass.graph_index,
            pass.pass_id,
            pass.pass_record_index,
            draw_start,
            draw_end,
            draw_objects,
            pass.target,
            pass.target_name,
            storage
                .string(pass_record.shader_key)
                .unwrap_or("<missing>"),
            pass_record.pipeline_blend,
            pass_record.scene_blend,
            blend.label(),
            pass_record.cull_mode,
            pass_record.color_write_mask,
            target_format,
            pass_pipeline_samples(pass.target, scene_color_msaa_enabled).rasterization_samples(),
            pipeline_index,
            material_textures,
            material_constants,
        );
    }
    Ok(())
}
