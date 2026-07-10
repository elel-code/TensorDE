//! Per-draw sampled-image lowering for scene descriptor heaps.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation, SceneResourceId,
    SceneStringId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum SceneSampledImageSource {
    FallbackWhite,
    SceneTexture { resource: SceneResourceId },
    EffectTarget { physical_slot: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneSampledImageBindingPlan {
    pub sampled_slot_count: usize,
    pub sources: Vec<SceneSampledImageSource>,
    pub initial_reference_physical_slots: Vec<u32>,
    pub fallback_descriptor_count: usize,
    pub scene_texture_descriptor_count: usize,
    pub effect_target_descriptor_count: usize,
}

impl SceneSampledImageBindingPlan {
    pub fn source(
        &self,
        draw_index: usize,
        sampled_index: usize,
    ) -> Option<SceneSampledImageSource> {
        draw_index
            .checked_mul(self.sampled_slot_count)
            .and_then(|base| base.checked_add(sampled_index))
            .and_then(|index| self.sources.get(index))
            .copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogicalTargetReference {
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    physical_slot: u32,
}

pub(in crate::renderer::native_vulkan) fn scene_sampled_image_binding_plan(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
) -> Result<SceneSampledImageBindingPlan, String> {
    scene_sampled_image_binding_cycle(graph, sampled_slots)?
        .into_iter()
        .next()
        .ok_or_else(|| "scene sampled binding cycle is empty".to_owned())
}

pub(in crate::renderer::native_vulkan) fn scene_sampled_image_binding_cycle(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
) -> Result<Vec<SceneSampledImageBindingPlan>, String> {
    let initial_references = logical_target_references(&graph.target_allocations);
    let mut references = initial_references.clone();
    let mut cycle = Vec::new();
    loop {
        if cycle.len() >= 1_024 {
            return Err("scene sampled target reference cycle exceeds 1024 frames".to_owned());
        }
        if cycle.iter().any(|plan: &SceneSampledImageBindingPlan| {
            plan.initial_reference_physical_slots
                == reference_physical_slots(&references)
        }) {
            return Err(
                "scene sampled target references entered a non-origin permutation cycle"
                    .to_owned(),
            );
        }
        cycle.push(scene_sampled_image_binding_plan_for_references(
            graph,
            sampled_slots,
            &mut references,
        )?);
        if references == initial_references {
            break;
        }
    }
    Ok(cycle)
}

fn scene_sampled_image_binding_plan_for_references(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    references: &mut [LogicalTargetReference],
) -> Result<SceneSampledImageBindingPlan, String> {
    let source_count = graph.mesh_draws.len().saturating_mul(sampled_slots.len());
    let mut sources = vec![SceneSampledImageSource::FallbackWhite; source_count];
    let initial_reference_physical_slots = reference_physical_slots(references);
    lower_material_sampled_bindings(graph, sampled_slots, &mut sources)?;

    for (pass_node_index, pass) in graph.pass_nodes.iter().enumerate() {
        let pass_bindings = graph
            .sampled_bindings
            .iter()
            .filter(|binding| binding.pass_node_index == pass_node_index as u32)
            .collect::<Vec<_>>();
        if pass.role == SceneRenderPassKind::SwapTargetReferences {
            apply_swap_reference(
                pass.graph_index,
                pass.target,
                pass.target_name,
                &pass_bindings,
                references,
            )?;
            continue;
        }
        lower_pass_sampled_bindings(
            pass.mesh_draw_start,
            pass.mesh_draw_count,
            &pass_bindings,
            sampled_slots,
            references,
            &mut sources,
        )?;
    }

    let effect_target_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::EffectTarget { .. }))
        .count();
    let scene_texture_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::SceneTexture { .. }))
        .count();
    Ok(SceneSampledImageBindingPlan {
        sampled_slot_count: sampled_slots.len(),
        initial_reference_physical_slots,
        fallback_descriptor_count: sources
            .len()
            .saturating_sub(effect_target_descriptor_count)
            .saturating_sub(scene_texture_descriptor_count),
        scene_texture_descriptor_count,
        effect_target_descriptor_count,
        sources,
    })
}

fn lower_material_sampled_bindings(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    sources: &mut [SceneSampledImageSource],
) -> Result<(), String> {
    for binding in &graph.material_sampled_bindings {
        let Some(sampled_index) = sampled_slots.iter().position(|slot| *slot == binding.slot) else {
            continue;
        };
        let source_index = binding.draw_index as usize * sampled_slots.len() + sampled_index;
        let source = sources.get_mut(source_index).ok_or_else(|| {
            format!(
                "scene material texture references missing draw {} sampled slot {}",
                binding.draw_index, binding.slot
            )
        })?;
        if !matches!(source, SceneSampledImageSource::FallbackWhite) {
            return Err(format!(
                "scene draw {} has duplicate material texture binding for sampled slot {}",
                binding.draw_index, binding.slot
            ));
        }
        *source = SceneSampledImageSource::SceneTexture {
            resource: binding.resource,
        };
    }
    Ok(())
}

fn reference_physical_slots(references: &[LogicalTargetReference]) -> Vec<u32> {
    references
        .iter()
        .map(|reference| reference.physical_slot)
        .collect()
}

fn lower_pass_sampled_bindings(
    draw_start: u32,
    draw_count: u32,
    bindings: &[&SceneRenderingDeviceSampledBinding],
    sampled_slots: &[u32],
    references: &[LogicalTargetReference],
    sources: &mut [SceneSampledImageSource],
) -> Result<(), String> {
    for binding in bindings {
        let Some((graph_index, target, target_name)) = binding.logical_target() else {
            continue;
        };
        let sampled_index = sampled_slots
            .iter()
            .position(|slot| *slot == binding.slot)
            .ok_or_else(|| {
                format!(
                    "scene graph target binding slot {} is absent from drawable shader contracts",
                    binding.slot
                )
            })?;
        let physical_slot = reference_physical_slot(
            references,
            graph_index,
            target,
            target_name,
        )
        .ok_or_else(|| {
            format!(
                "scene graph target binding {:?}:{:?} has no physical allocation",
                target, target_name
            )
        })?;
        for draw_index in draw_start..draw_start.saturating_add(draw_count) {
            let source_index = draw_index as usize * sampled_slots.len() + sampled_index;
            let source = sources.get_mut(source_index).ok_or_else(|| {
                format!(
                    "scene graph target binding references missing draw {draw_index} sampled slot {}",
                    binding.slot
                )
            })?;
            if matches!(source, SceneSampledImageSource::EffectTarget { .. }) {
                return Err(format!(
                    "scene draw {draw_index} has duplicate graph target binding for sampled slot {}",
                    binding.slot
                ));
            }
            *source = SceneSampledImageSource::EffectTarget { physical_slot };
        }
    }
    Ok(())
}

fn apply_swap_reference(
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    bindings: &[&SceneRenderingDeviceSampledBinding],
    references: &mut [LogicalTargetReference],
) -> Result<(), String> {
    let (source_graph_index, source_target, source_name) = bindings
        .iter()
        .find_map(|binding| binding.logical_target())
        .ok_or_else(|| "scene effect swap pass has no logical source target binding".to_owned())?;
    let source_index = reference_index(
        references,
        source_graph_index,
        source_target,
        source_name,
    )
    .ok_or_else(|| {
        "scene effect swap source target has no physical allocation".to_owned()
    })?;
    let target_index = reference_index(references, graph_index, target, target_name).ok_or_else(|| {
        "scene effect swap destination target has no physical allocation".to_owned()
    })?;
    let source_physical_slot = references[source_index].physical_slot;
    references[source_index].physical_slot = references[target_index].physical_slot;
    references[target_index].physical_slot = source_physical_slot;
    Ok(())
}

fn logical_target_references(
    allocations: &[SceneRenderingDeviceTargetAllocation],
) -> Vec<LogicalTargetReference> {
    allocations
        .iter()
        .map(|allocation| LogicalTargetReference {
            graph_index: allocation.graph_index,
            target: allocation.target,
            target_name: allocation.target_name,
            physical_slot: allocation.physical_slot,
        })
        .collect()
}

fn reference_physical_slot(
    references: &[LogicalTargetReference],
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<u32> {
    reference_index(references, graph_index, target, target_name)
        .map(|index| references[index].physical_slot)
}

fn reference_index(
    references: &[LogicalTargetReference],
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<usize> {
    references.iter().position(|reference| {
        reference.graph_index == graph_index
            && reference.target == target
            && reference.target_name == target_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneRenderBindingKind, SceneRenderingDeviceGraphPlan,
        SceneRenderingDeviceMaterialSampledBinding, SceneRenderingDevicePassNode,
        SceneRenderingDeviceSampledBinding,
    };

    #[test]
    fn sampled_binding_plan_preserves_nonzero_slots_and_swap_rewrites() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                pass_node(0, SceneRenderPassKind::EffectMaterial, SceneStringId(0), 0, 1),
                pass_node(
                    1,
                    SceneRenderPassKind::SwapTargetReferences,
                    SceneStringId(1),
                    1,
                    0,
                ),
                pass_node(2, SceneRenderPassKind::EffectMaterial, SceneStringId(2), 1, 1),
            ],
            target_allocations: vec![
                allocation(SceneStringId(0), 0),
                allocation(SceneStringId(1), 1),
                allocation(SceneStringId(2), 2),
            ],
            sampled_bindings: vec![
                sampled_binding(0, 2, SceneStringId(1), 0, 1),
                sampled_binding(1, 0, SceneStringId(0), 1, 0),
                sampled_binding(2, 2, SceneStringId(0), 1, 1),
            ],
            material_sampled_bindings: vec![
                SceneRenderingDeviceMaterialSampledBinding {
                    draw_index: 0,
                    slot: 0,
                    resource: SceneResourceId(7),
                },
                SceneRenderingDeviceMaterialSampledBinding {
                    draw_index: 1,
                    slot: 2,
                    resource: SceneResourceId(8),
                },
            ],
            mesh_draws: vec![draw(), draw()],
            ..empty_graph_plan()
        };

        let plan = scene_sampled_image_binding_plan(&graph, &[0, 2]).expect("binding plan");
        let cycle = scene_sampled_image_binding_cycle(&graph, &[0, 2]).expect("binding cycle");

        assert_eq!(plan.effect_target_descriptor_count, 2);
        assert_eq!(plan.scene_texture_descriptor_count, 1);
        assert_eq!(plan.fallback_descriptor_count, 1);
        assert_eq!(cycle.len(), 2);
        assert_eq!(cycle[0].initial_reference_physical_slots, vec![0, 1, 2]);
        assert_eq!(cycle[1].initial_reference_physical_slots, vec![1, 0, 2]);
        assert_eq!(
            plan.source(0, 0),
            Some(SceneSampledImageSource::SceneTexture {
                resource: SceneResourceId(7)
            })
        );
        assert_eq!(
            plan.source(0, 1),
            Some(SceneSampledImageSource::EffectTarget { physical_slot: 1 })
        );
        assert_eq!(
            plan.source(1, 1),
            Some(SceneSampledImageSource::EffectTarget { physical_slot: 1 })
        );
        assert_eq!(
            cycle[1].source(1, 1),
            Some(SceneSampledImageSource::EffectTarget { physical_slot: 0 })
        );
    }

    fn pass_node(
        pass_record_index: u32,
        role: SceneRenderPassKind,
        target_name: SceneStringId,
        mesh_draw_start: u32,
        mesh_draw_count: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            pass_record_index,
            pass_id: pass_record_index,
            role,
            target: SceneRenderTargetKind::NamedFbo,
            target_name,
            binding_start: 0,
            binding_count: 0,
            mesh_draw_start,
            mesh_draw_count,
        }
    }

    fn allocation(
        target_name: SceneStringId,
        physical_slot: u32,
    ) -> SceneRenderingDeviceTargetAllocation {
        SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name,
            first_write_pass_id: 0,
            last_use_pass_id: 2,
            physical_slot,
        }
    }

    fn sampled_binding(
        pass_node_index: u32,
        slot: u32,
        target_name: SceneStringId,
        mesh_draw_start: u32,
        mesh_draw_count: u32,
    ) -> SceneRenderingDeviceSampledBinding {
        SceneRenderingDeviceSampledBinding {
            pass_node_index,
            graph_index: 0,
            mesh_draw_start,
            mesh_draw_count,
            kind: SceneRenderBindingKind::NamedFboBind,
            slot,
            target: SceneRenderTargetKind::NamedFbo,
            target_name,
        }
    }

    fn draw() -> crate::engine::scene::SceneRenderingDeviceMeshDraw {
        crate::engine::scene::SceneRenderingDeviceMeshDraw {
            primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
            resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
            clip_transform: [[0.0; 4]; 4],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            object: crate::engine::scene::SceneObjectHandle(
                crate::engine::scene::INVALID_OBJECT_ID,
            ),
            material: crate::engine::scene::SceneMaterialHandle(
                crate::engine::scene::INVALID_MATERIAL_ID,
            ),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
        }
    }

    fn empty_graph_plan() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }
}
