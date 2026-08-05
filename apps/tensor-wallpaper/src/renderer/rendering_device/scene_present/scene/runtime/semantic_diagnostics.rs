//! Explicit cold diagnostic projection of the retained semantic and draw state.

use serde::Serialize;

use super::SemanticFrameResolver;
use super::descriptor_layout::ScenePipelineDescriptorLayout;
use super::draw_recording::{SceneGpuDrawCommand, SceneGpuScissor};
use super::material_uniform::{SCENE_MATERIAL_UNIFORM_BYTES, resolved_standard_material_color};
use super::sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, SceneVideoPlane,
};
use super::scene_owned_uniform::SceneOwnedUniformArenaPlan;
use crate::engine::scene::{
    INVALID_OBJECT_ID, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneScriptTarget, SceneStorage, SceneTextureFormat, SceneTextureSamplerAddressMode,
    SceneTextureSamplerFilter,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneSemanticDiagnosticsSnapshot {
    pub descriptor_heap: RenderingDeviceSceneDescriptorHeapSnapshot,
    pub retained_script_deltas: Vec<RenderingDeviceSceneScriptDeltaSnapshot>,
    pub resolved_objects: Vec<RenderingDeviceSceneResolvedObjectSnapshot>,
    pub puppet_bone_palettes: Vec<RenderingDeviceScenePuppetBonePaletteSnapshot>,
    /// Typed sampled-image source and sampler evidence for every retained
    /// descriptor phase. This is diagnostic-only and exposes the same lane
    /// identity that command recording pushes through `vkCmdPushDataEXT`.
    pub sampled_binding_phases: Vec<RenderingDeviceSceneSampledBindingPhaseSnapshot>,
    pub draws: Vec<RenderingDeviceSceneDrawActivationSnapshot>,
    pub enabled_draw_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneDescriptorHeapSnapshot {
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub reference_phase_count: usize,
    /// Absolute sampled-image slot order used by every per-draw descriptor lane.
    ///
    /// This turns a recorded descriptor-push index back into an authored shader
    /// register without relying on API-dump heap-range heuristics.
    pub sampled_slots: Vec<u32>,
    /// Absolute input-attachment slot order, kept separate from sampled images.
    pub input_attachment_slots: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneScriptDeltaSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub target: SceneScriptTarget,
    pub selector: u32,
    pub numeric: [f32; 4],
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneResolvedObjectSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub parent_handle: Option<u32>,
    pub parent_id: Option<u32>,
    pub authored_visible: bool,
    pub self_visible: bool,
    pub resolved_visible: bool,
    pub authored_origin: [f32; 3],
    pub authored_angles: [f32; 3],
    pub authored_scale: [f32; 3],
    pub authored_alpha: f32,
    pub self_alpha: f32,
    pub resolved_alpha: f32,
    pub local_matrix: [f32; 16],
    pub world_matrix: [f32; 16],
    pub render_world_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceScenePuppetBonePaletteSnapshot {
    pub object_handle: u32,
    pub object_id: u32,
    pub object_name: String,
    pub puppet_index: u32,
    pub resolved_visible: bool,
    pub bones: Vec<RenderingDeviceScenePuppetBoneMatrixSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RenderingDeviceScenePuppetBoneMatrixSnapshot {
    pub bone_index: u32,
    pub parent_index: i32,
    pub matrix: [[f32; 4]; 4],
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneDrawActivationSnapshot {
    pub draw_index: u32,
    pub object_handle: Option<u32>,
    pub shader_key: String,
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub enabled: bool,
    pub pipeline_index: u32,
    pub resource_descriptor_base: usize,
    pub material_resource_descriptor: Option<usize>,
    pub skinning_resource_descriptor: Option<usize>,
    pub particle_resource_descriptor: Option<usize>,
    pub scene_owned_uniform_descriptor_base: usize,
    pub sampled_resource_descriptor_base: usize,
    pub input_attachment_resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub vertex_buffer_byte_offset: Option<u64>,
    pub descriptor_push_words: Vec<u32>,
    pub material_uniform_words: Vec<u32>,
    pub scene_owned_uniform_words: Vec<Vec<u32>>,
    pub standard_material_color: Option<[f32; 4]>,
    pub scissor: Option<RenderingDeviceSceneScissorSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RenderingDeviceSceneScissorSnapshot {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneSampledBindingPhaseSnapshot {
    pub reference_phase: usize,
    pub initial_reference_physical_slots: Vec<u32>,
    pub bindings: Vec<RenderingDeviceSceneSampledBindingSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSceneSampledBindingSnapshot {
    pub draw_index: u32,
    pub slot: u32,
    pub resource_descriptor_index: usize,
    pub sampler_descriptor_index: usize,
    pub source: RenderingDeviceSceneSampledImageSourceSnapshot,
    pub sampler: RenderingDeviceSceneSamplerSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RenderingDeviceSceneSampledImageSourceSnapshot {
    FallbackWhite,
    SceneTexture {
        resource: u32,
        path: String,
        format: SceneTextureFormat,
        logical_extent: [u32; 2],
        storage_extent: [u32; 2],
    },
    SceneColorSnapshot,
    EffectTarget {
        physical_slot: u32,
        batch_atlas_tile: u32,
    },
    VideoFramePlane {
        media_instance: u32,
        plane: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RenderingDeviceSceneSamplerSnapshot {
    AuthoredTexture {
        filter: SceneTextureSamplerFilter,
        address_mode: SceneTextureSamplerAddressMode,
    },
    LinearClamp,
}

pub(super) fn scene_semantic_diagnostics_snapshot(
    storage: &SceneStorage,
    resolver: &SemanticFrameResolver,
    graph: &SceneRenderingDeviceGraphPlan,
    draws: &[SceneGpuDrawCommand],
    descriptor_layout: &ScenePipelineDescriptorLayout,
    sampled_binding_cycle: &[SceneSampledImageBindingPlan],
    material_uniform_payload: Option<&[u8]>,
    scene_owned_uniform_plan: &SceneOwnedUniformArenaPlan,
    scene_owned_uniform_payload: &[u8],
    descriptor_heap: RenderingDeviceSceneDescriptorHeapSnapshot,
) -> Result<RenderingDeviceSceneSemanticDiagnosticsSnapshot, String> {
    if graph.mesh_draws.len() != draws.len() {
        return Err(format!(
            "scene diagnostic graph draw count {} does not match command count {}",
            graph.mesh_draws.len(),
            draws.len()
        ));
    }
    let sampled_binding_phases =
        sampled_binding_phase_snapshots(storage, descriptor_layout, sampled_binding_cycle, draws)?;
    let retained_script_deltas = resolver
        .retained_script_deltas()
        .iter()
        .map(|delta| {
            let (object_id, object_name) = object_identity(storage, delta.object.0)?;
            Ok(RenderingDeviceSceneScriptDeltaSnapshot {
                object_handle: delta.object.0,
                object_id,
                object_name,
                target: delta.target,
                selector: delta.selector,
                numeric: delta.numeric,
                text: delta.text.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let resolved_objects = resolver
        .resolved_frame()
        .objects
        .iter()
        .map(|resolved| {
            let object = storage
                .objects()
                .get(resolved.object.0 as usize)
                .ok_or_else(|| {
                    format!(
                        "resolved object {} is outside scene storage",
                        resolved.object.0
                    )
                })?;
            if object.id != resolved.object {
                return Err(format!(
                    "resolved object handle {} does not match storage handle {}",
                    resolved.object.0, object.id.0
                ));
            }
            let (_, object_name) = object_identity(storage, resolved.object.0)?;
            let parent = (resolved.parent.0 != INVALID_OBJECT_ID).then_some(resolved.parent.0);
            let parent_id = parent
                .map(|handle| object_identity(storage, handle).map(|identity| identity.0))
                .transpose()?;
            Ok(RenderingDeviceSceneResolvedObjectSnapshot {
                object_handle: resolved.object.0,
                object_id: object.we_id,
                object_name,
                parent_handle: parent,
                parent_id,
                authored_visible: object.visible,
                self_visible: resolved.self_visible,
                resolved_visible: resolved.resolved_visible,
                authored_origin: [object.origin.x, object.origin.y, object.origin.z],
                authored_angles: [object.angles.x, object.angles.y, object.angles.z],
                authored_scale: [object.scale.x, object.scale.y, object.scale.z],
                authored_alpha: object.alpha,
                self_alpha: resolved.self_alpha,
                resolved_alpha: resolved.resolved_alpha,
                local_matrix: resolved.local_matrix,
                world_matrix: resolved.world_matrix,
                render_world_matrix: resolved.render_world_matrix,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let puppet_bone_palettes = graph
        .puppet_bone_palettes
        .iter()
        .map(|palette| {
            let (object_id, object_name) = object_identity(storage, palette.object.0)?;
            let start = usize::try_from(palette.bone_matrix_start)
                .map_err(|_| "puppet bone palette start exceeds usize".to_owned())?;
            let count = usize::try_from(palette.bone_matrix_count)
                .map_err(|_| "puppet bone palette count exceeds usize".to_owned())?;
            let end = start
                .checked_add(count)
                .ok_or_else(|| "puppet bone palette range overflows usize".to_owned())?;
            let matrices = graph.puppet_bone_matrices.get(start..end).ok_or_else(|| {
                format!(
                    "puppet {} palette range {start}..{end} exceeds {} matrices",
                    palette.puppet_index,
                    graph.puppet_bone_matrices.len()
                )
            })?;
            let bones = matrices
                .iter()
                .map(|matrix| {
                    if matrix.puppet_index != palette.puppet_index {
                        return Err(format!(
                            "puppet {} palette contains matrix for puppet {}",
                            palette.puppet_index, matrix.puppet_index
                        ));
                    }
                    Ok(RenderingDeviceScenePuppetBoneMatrixSnapshot {
                        bone_index: matrix.bone_index,
                        parent_index: matrix.parent_index,
                        matrix: matrix.matrix,
                        alpha: matrix.alpha,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(RenderingDeviceScenePuppetBonePaletteSnapshot {
                object_handle: palette.object.0,
                object_id,
                object_name,
                puppet_index: palette.puppet_index,
                resolved_visible: palette.resolved_visible,
                bones,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let draws = graph
        .mesh_draws
        .iter()
        .zip(draws)
        .enumerate()
        .map(|(draw_index, (planned, command))| {
            let shader_key = storage
                .string(planned.shader_key)
                .ok_or_else(|| format!("scene draw {draw_index} has no shader key"))?;
            Ok(RenderingDeviceSceneDrawActivationSnapshot {
                draw_index: u32::try_from(draw_index)
                    .map_err(|_| "scene draw diagnostic index exceeds u32".to_owned())?,
                object_handle: (planned.object.0 != INVALID_OBJECT_ID).then_some(planned.object.0),
                shader_key: shader_key.to_owned(),
                primitive: command.primitive,
                enabled: command.enabled,
                pipeline_index: command.pipeline_index,
                resource_descriptor_base: command.resource_descriptor_base,
                material_resource_descriptor: command.material_resource_descriptor,
                skinning_resource_descriptor: command.skinning_resource_descriptor,
                particle_resource_descriptor: command.particle_resource_descriptor,
                scene_owned_uniform_descriptor_base: command.scene_owned_uniform_descriptor_base,
                sampled_resource_descriptor_base: command.sampled_resource_descriptor_base,
                input_attachment_resource_descriptor_base: command
                    .input_attachment_resource_descriptor_base,
                sampler_descriptor_base: command.sampler_descriptor_base,
                vertex_buffer_byte_offset: command.vertex_buffer_byte_offset,
                descriptor_push_words: command
                    .active_descriptor_push()
                    .map(|push| {
                        push.bytes()
                            .chunks_exact(4)
                            .map(|word| {
                                u32::from_le_bytes(word.try_into().expect("four-byte push word"))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                material_uniform_words: material_uniform_payload
                    .map(|payload| material_uniform_words_for_draw(draw_index, payload))
                    .transpose()?
                    .unwrap_or_default(),
                scene_owned_uniform_words: scene_owned_uniform_plan
                    .payload_slices_for_draw(draw_index, scene_owned_uniform_payload)?
                    .into_iter()
                    .map(|slice| {
                        slice
                            .chunks_exact(4)
                            .map(|word| {
                                u32::from_le_bytes(word.try_into().expect("four-byte uniform word"))
                            })
                            .collect()
                    })
                    .collect(),
                standard_material_color: resolved_standard_material_color(storage, planned),
                scissor: command.scissor.map(scissor_snapshot),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enabled_draw_count = draws.iter().filter(|draw| draw.enabled).count();
    Ok(RenderingDeviceSceneSemanticDiagnosticsSnapshot {
        descriptor_heap,
        retained_script_deltas,
        resolved_objects,
        puppet_bone_palettes,
        sampled_binding_phases,
        draws,
        enabled_draw_count,
    })
}

fn sampled_binding_phase_snapshots(
    storage: &SceneStorage,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    sampled_binding_cycle: &[SceneSampledImageBindingPlan],
    draws: &[SceneGpuDrawCommand],
) -> Result<Vec<RenderingDeviceSceneSampledBindingPhaseSnapshot>, String> {
    sampled_binding_cycle
        .iter()
        .enumerate()
        .map(|(reference_phase, plan)| {
            if plan.sampled_slot_count != descriptor_layout.sampled_slots.len() {
                return Err(format!(
                    "scene diagnostic sampled binding phase {reference_phase} has {} slots, descriptor layout has {}",
                    plan.sampled_slot_count,
                    descriptor_layout.sampled_slots.len()
                ));
            }
            let mut bindings = Vec::with_capacity(
                draws
                    .len()
                    .saturating_mul(descriptor_layout.sampled_slots.len()),
            );
            for (draw_index, draw) in draws.iter().enumerate() {
                for (sampled_index, slot) in descriptor_layout.sampled_slots.iter().copied().enumerate() {
                    let source = plan.source(draw_index, sampled_index).ok_or_else(|| {
                        format!(
                            "scene diagnostic sampled binding phase {reference_phase} is missing draw {draw_index} slot {slot}"
                        )
                    })?;
                    let (source, sampler) = sampled_source_snapshot(storage, source)?;
                    bindings.push(RenderingDeviceSceneSampledBindingSnapshot {
                        draw_index: u32::try_from(draw_index).map_err(|_| {
                            "scene diagnostic sampled binding draw index exceeds u32".to_owned()
                        })?,
                        slot,
                        resource_descriptor_index: draw
                            .sampled_resource_descriptor_base
                            .checked_add(sampled_index)
                            .ok_or_else(|| {
                                "scene diagnostic sampled resource descriptor index overflows"
                                    .to_owned()
                            })?,
                        sampler_descriptor_index: draw
                            .sampler_descriptor_base
                            .checked_add(sampled_index)
                            .ok_or_else(|| {
                                "scene diagnostic sampled sampler descriptor index overflows"
                                    .to_owned()
                            })?,
                        source,
                        sampler,
                    });
                }
            }
            Ok(RenderingDeviceSceneSampledBindingPhaseSnapshot {
                reference_phase,
                initial_reference_physical_slots: plan.initial_reference_physical_slots.clone(),
                bindings,
            })
        })
        .collect()
}

fn sampled_source_snapshot(
    storage: &SceneStorage,
    source: SceneSampledImageSource,
) -> Result<
    (
        RenderingDeviceSceneSampledImageSourceSnapshot,
        RenderingDeviceSceneSamplerSnapshot,
    ),
    String,
> {
    match source {
        SceneSampledImageSource::FallbackWhite => Ok((
            RenderingDeviceSceneSampledImageSourceSnapshot::FallbackWhite,
            RenderingDeviceSceneSamplerSnapshot::LinearClamp,
        )),
        SceneSampledImageSource::SceneTexture { resource } => {
            let resource_record = storage.resource(resource).ok_or_else(|| {
                format!(
                    "scene diagnostic sampled texture resource {} is missing from storage",
                    resource.0
                )
            })?;
            let path = storage.string(resource_record.path).ok_or_else(|| {
                format!(
                    "scene diagnostic sampled texture resource {} has an invalid path",
                    resource.0
                )
            })?;
            let texture = storage.texture(resource).ok_or_else(|| {
                format!(
                    "scene diagnostic sampled texture resource {} has no texture record",
                    resource.0
                )
            })?;
            Ok((
                RenderingDeviceSceneSampledImageSourceSnapshot::SceneTexture {
                    resource: resource.0,
                    path: path.to_owned(),
                    format: texture.format,
                    logical_extent: [texture.width, texture.height],
                    storage_extent: [texture.storage_width, texture.storage_height],
                },
                RenderingDeviceSceneSamplerSnapshot::AuthoredTexture {
                    filter: texture.sampler_filter,
                    address_mode: texture.sampler_address_mode,
                },
            ))
        }
        SceneSampledImageSource::SceneColorSnapshot => Ok((
            RenderingDeviceSceneSampledImageSourceSnapshot::SceneColorSnapshot,
            RenderingDeviceSceneSamplerSnapshot::LinearClamp,
        )),
        SceneSampledImageSource::EffectTarget {
            physical_slot,
            batch_atlas_tile,
        } => Ok((
            RenderingDeviceSceneSampledImageSourceSnapshot::EffectTarget {
                physical_slot,
                batch_atlas_tile,
            },
            RenderingDeviceSceneSamplerSnapshot::LinearClamp,
        )),
        SceneSampledImageSource::VideoFramePlane {
            media_instance,
            plane,
        } => Ok((
            RenderingDeviceSceneSampledImageSourceSnapshot::VideoFramePlane {
                media_instance,
                plane: match plane {
                    SceneVideoPlane::Y => "y",
                    SceneVideoPlane::Uv => "uv",
                },
            },
            RenderingDeviceSceneSamplerSnapshot::LinearClamp,
        )),
    }
}

fn material_uniform_words_for_draw(draw_index: usize, payload: &[u8]) -> Result<Vec<u32>, String> {
    let stride = usize::try_from(SCENE_MATERIAL_UNIFORM_BYTES)
        .map_err(|_| "scene material uniform stride exceeds usize".to_owned())?;
    let start = draw_index
        .checked_mul(stride)
        .ok_or_else(|| "scene material uniform draw offset overflows usize".to_owned())?;
    let end = start
        .checked_add(stride)
        .ok_or_else(|| "scene material uniform draw range overflows usize".to_owned())?;
    let bytes = payload.get(start..end).ok_or_else(|| {
        format!(
            "scene material uniform draw {draw_index} range {start}..{end} exceeds {} bytes",
            payload.len()
        )
    })?;
    Ok(bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("four-byte material uniform word")))
        .collect())
}

fn object_identity(storage: &SceneStorage, handle: u32) -> Result<(u32, String), String> {
    let object = storage
        .objects()
        .get(handle as usize)
        .filter(|object| object.id.0 == handle)
        .ok_or_else(|| format!("scene diagnostic references missing object {handle}"))?;
    let name = if object.name.is_some() {
        storage
            .string(object.name)
            .ok_or_else(|| format!("scene object {handle} has an invalid name string"))?
    } else {
        ""
    };
    Ok((object.we_id, name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_heap_snapshot_keeps_explicit_resource_lane_order() {
        let snapshot = RenderingDeviceSceneDescriptorHeapSnapshot {
            resource_descriptor_count: 7,
            sampler_descriptor_count: 3,
            reference_phase_count: 2,
            sampled_slots: vec![0, 3, 7],
            input_attachment_slots: vec![1, 4],
        };

        let json = serde_json::to_value(snapshot).expect("serialize descriptor diagnostics");

        assert_eq!(json["sampled_slots"], serde_json::json!([0, 3, 7]));
        assert_eq!(json["input_attachment_slots"], serde_json::json!([1, 4]));
    }

    #[test]
    fn sampled_binding_snapshot_ties_phase_sources_to_dense_heap_lanes() {
        let storage =
            SceneStorage::from_document(crate::engine::scene::SceneBinaryDocument::default())
                .expect("diagnostic storage");
        let layout = ScenePipelineDescriptorLayout {
            sampled_slots: vec![0, 3],
            input_attachment_slots: Vec::new(),
            material_uniform_enabled: false,
            skinning_storage_enabled: false,
            particle_storage_enabled: false,
            scene_owned_uniform_count: 0,
        };
        let plan = SceneSampledImageBindingPlan {
            sampled_slot_count: 2,
            sources: vec![
                SceneSampledImageSource::SceneColorSnapshot,
                SceneSampledImageSource::EffectTarget {
                    physical_slot: 9,
                    batch_atlas_tile: 2,
                },
            ],
            initial_reference_physical_slots: vec![9],
            fallback_descriptor_count: 0,
            scene_texture_descriptor_count: 0,
            scene_color_snapshot_descriptor_count: 1,
            effect_target_descriptor_count: 1,
            video_frame_descriptor_count: 0,
        };
        let draw = SceneGpuDrawCommand {
            enabled: true,
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            pipeline_index: 0,
            authored_pipeline_index: 0,
            disabled_pipeline_index: None,
            first_index: 0,
            index_count: 0,
            vertex_offset: 0,
            vertex_buffer_byte_offset: None,
            vertex_count: 3,
            instance_count: 1,
            instance_capacity: 1,
            first_instance: 0,
            dynamic_text: false,
            video_media_instance: None,
            video_vertex_byte_offset: None,
            particle_indirect_index: None,
            resource_descriptor_base: 0,
            material_resource_descriptor: None,
            skinning_resource_descriptor: None,
            particle_resource_descriptor: None,
            scene_owned_uniform_descriptor_base: 0,
            sampled_resource_descriptor_base: 17,
            input_attachment_resource_descriptor_base: 19,
            sampler_descriptor_base: 23,
            descriptor_push: None,
            disabled_descriptor_push: None,
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: None,
        };

        let phases = sampled_binding_phase_snapshots(&storage, &layout, &[plan], &[draw])
            .expect("sampled binding diagnostics");

        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].reference_phase, 0);
        assert_eq!(phases[0].bindings[0].slot, 0);
        assert_eq!(phases[0].bindings[0].resource_descriptor_index, 17);
        assert_eq!(phases[0].bindings[0].sampler_descriptor_index, 23);
        assert_eq!(phases[0].bindings[1].slot, 3);
        assert_eq!(phases[0].bindings[1].resource_descriptor_index, 18);
        assert_eq!(phases[0].bindings[1].sampler_descriptor_index, 24);
        assert!(matches!(
            phases[0].bindings[1].source,
            RenderingDeviceSceneSampledImageSourceSnapshot::EffectTarget {
                physical_slot: 9,
                batch_atlas_tile: 2,
            }
        ));
        assert!(matches!(
            phases[0].bindings[1].sampler,
            RenderingDeviceSceneSamplerSnapshot::LinearClamp
        ));
    }
}

fn scissor_snapshot(scissor: SceneGpuScissor) -> RenderingDeviceSceneScissorSnapshot {
    RenderingDeviceSceneScissorSnapshot {
        offset: scissor.offset,
        extent: scissor.extent,
    }
}
