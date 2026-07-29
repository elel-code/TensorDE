//! Retained semantic-frame resolution with ECS-style dynamic dependency propagation.

mod local_time;

use super::{
    ResolveVisitState, ResolvedAttachmentLink, ResolvedObjectState, ResolvedPuppetBoneMatrix,
    ResolvedPuppetBonePalette, ResolvedSemanticFrame, RetainedSceneEventSystem, SceneSemanticWorld,
    SceneSemanticWorldError, identity_matrix, inverse_affine_matrix, multiply_matrix,
    sampled_puppet_bone_local_state,
};
use crate::engine::scene::abi::{ScenePuppetAttachmentRecord, ScenePuppetBoneRecord};
use crate::engine::scene::event::SceneFrameEvents;
use crate::engine::scene::event::{SceneEvent, ScenePointerEventKind};
use crate::engine::scene::semantic_world::resolved_frame::INVALID_RESOLVED_INDEX;
use crate::engine::scene::semantic_world::timeline::SampledPuppetBoneLocalState;
use crate::engine::scene::{
    SceneEventSequence, SceneObjectHandle, SceneScriptDelta, SceneScriptFrameInput,
    SceneScriptPointerClick, SceneScriptRuntime, SceneScriptSubscriptions,
};
use serde_json::{Map, Value};

use super::pointer_parallax::cover_mapped_position;
use local_time::local_time_dirty_events;

const LINUX_INPUT_BUTTON_LEFT: u32 = 0x110;

/// Retains the immutable semantic frame and resolves only time-dependent object state.
#[derive(Debug)]
pub struct SemanticFrameResolver {
    frame: ResolvedSemanticFrame,
    dynamic_entities: Vec<usize>,
    states: Vec<Option<ResolvedObjectState>>,
    visits: Vec<ResolveVisitState>,
    attachment_scratch: Vec<ResolvedAttachmentLink>,
    puppet_topologies: Vec<RetainedPuppetTopology>,
    incremental_enabled: bool,
    retained_puppet_enabled: bool,
    event_system: RetainedSceneEventSystem,
    script_runtime: Option<SceneScriptRuntime>,
    script_deltas: Vec<SceneScriptDelta>,
    script_delta_updates: Vec<SceneScriptDelta>,
    user_property_visibility: Vec<Option<bool>>,
    pointer_clicks: Vec<SceneScriptPointerClick>,
    pressed_click_targets: Vec<(u32, Option<SceneObjectHandle>)>,
    last_pointer_sequence: Option<SceneEventSequence>,
    last_audio_sequence: Option<SceneEventSequence>,
    last_media_sequence: Option<SceneEventSequence>,
    last_local_minute: Option<(i32, u8, u8, u8, u8)>,
    last_local_second: Option<(i32, u8, u8, u8, u8, u8)>,
    last_scene_time_seconds: Option<f32>,
}

#[derive(Debug)]
pub(super) struct RetainedPuppetTopology {
    bones: Vec<RetainedPuppetBone>,
    sampled_local: Vec<SampledPuppetBoneLocalState>,
    animated_world: Vec<[f32; 16]>,
    attachment_world: Vec<[f32; 16]>,
    attachment_object_world: Option<[f32; 16]>,
}

#[derive(Debug, Clone, Copy)]
struct RetainedPuppetBone {
    bone_index: u32,
    parent_slot: Option<usize>,
    inverse_bind: [f32; 16],
}

impl SemanticFrameResolver {
    pub fn from_world(world: &SceneSemanticWorld<'_>) -> Result<Self, SceneSemanticWorldError> {
        Self::from_world_with_user_properties(world, &Map::new())
    }

    pub fn from_world_with_user_properties(
        world: &SceneSemanticWorld<'_>,
        user_property_overrides: &Map<String, Value>,
    ) -> Result<Self, SceneSemanticWorldError> {
        let user_property_visibility =
            super::user_property::resolved_visibility(world, user_property_overrides)?;
        let frame =
            world.resolve_frame_with_dynamic_values_at(0.0, &[], &user_property_visibility)?;
        let dynamic_entities = dynamic_entity_closure(world);
        let puppet_topologies = retained_puppet_topologies(world)?;
        let entity_count = world.entities.len();
        let event_system = RetainedSceneEventSystem::from_world(world);
        let script_runtime =
            SceneScriptRuntime::from_storage(world.storage(), user_property_overrides)
                .map_err(|error| SceneSemanticWorldError::ScriptRuntime(error.to_string()))?;
        Ok(Self {
            frame,
            dynamic_entities,
            states: Vec::with_capacity(entity_count),
            visits: Vec::with_capacity(entity_count),
            attachment_scratch: Vec::new(),
            puppet_topologies,
            incremental_enabled: std::env::var_os(
                "GILDER_NATIVE_VULKAN_DISABLE_SEMANTIC_DIRTY_RESOLVE",
            )
            .is_none(),
            retained_puppet_enabled: std::env::var_os(
                "GILDER_NATIVE_VULKAN_DISABLE_RETAINED_PUPPET_RESOLVE",
            )
            .is_none(),
            event_system,
            script_runtime,
            script_deltas: Vec::new(),
            script_delta_updates: Vec::new(),
            user_property_visibility,
            pointer_clicks: Vec::new(),
            pressed_click_targets: Vec::new(),
            last_pointer_sequence: None,
            last_audio_sequence: None,
            last_media_sequence: None,
            last_local_minute: None,
            last_local_second: None,
            last_scene_time_seconds: None,
        })
    }

    pub fn resolve_frame_with_events_at(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        scene_time_seconds: f32,
        frame_delta_seconds: f32,
        events: &SceneFrameEvents,
    ) -> Result<&ResolvedSemanticFrame, SceneSemanticWorldError> {
        self.dispatch_scripts(world, scene_time_seconds, events)?;
        self.event_system
            .begin_frame(world, &mut self.frame, frame_delta_seconds, events);
        merge_script_text_deltas(&self.script_deltas, &mut self.frame.script_text_values);
        merge_script_material_deltas(
            &self.script_deltas,
            &mut self.frame.audio_band_material_values,
        );
        if !self.incremental_enabled {
            let audio_values = std::mem::take(&mut self.frame.audio_band_material_values);
            let text_values = std::mem::take(&mut self.frame.script_text_values);
            let media_clock = self.frame.media_clock;
            let video_frame = self.frame.video_frame;
            self.frame = world.resolve_frame_with_dynamic_values_at(
                scene_time_seconds,
                &self.script_deltas,
                &self.user_property_visibility,
            )?;
            self.frame.audio_band_material_values = audio_values;
            self.frame.script_text_values = text_values;
            self.frame.media_clock = media_clock;
            self.frame.video_frame = video_frame;
            self.event_system.finish_frame(&mut self.frame);
            return Ok(&self.frame);
        }

        self.states.clear();
        self.states
            .extend(self.frame.objects.iter().copied().map(Some));
        self.visits.clear();
        self.visits
            .resize(self.frame.objects.len(), ResolveVisitState::Resolved);
        for &entity_index in &self.dynamic_entities {
            self.states[entity_index] = None;
            self.visits[entity_index] = ResolveVisitState::Unvisited;
        }
        self.attachment_scratch.clear();
        if self.retained_puppet_enabled {
            prepare_retained_puppet_samples(world, &mut self.puppet_topologies, scene_time_seconds);
        }
        let mut retained_puppets = self
            .retained_puppet_enabled
            .then_some(self.puppet_topologies.as_mut_slice());
        for &entity_index in &self.dynamic_entities {
            world.resolve_entity(
                entity_index,
                &mut self.visits,
                &mut self.states,
                &mut self.attachment_scratch,
                &mut retained_puppets,
                &self.script_deltas,
                &self.user_property_visibility,
                scene_time_seconds,
            )?;
        }
        for &entity_index in &self.dynamic_entities {
            self.frame.objects[entity_index] = self.states[entity_index]
                .expect("dynamic semantic entity is resolved before frame publication");
        }
        refresh_object_effects(&mut self.frame, &self.script_deltas);

        if self.retained_puppet_enabled {
            resolve_retained_puppets(
                world,
                &self.frame.objects,
                &mut self.puppet_topologies,
                &mut self.frame.puppet_bone_palettes,
                &mut self.frame.puppet_bone_matrices,
            );
        } else {
            let (palettes, matrices) =
                world.resolve_puppet_bone_palettes(&self.frame.objects, scene_time_seconds)?;
            self.frame.puppet_bone_palettes = palettes;
            self.frame.puppet_bone_matrices = matrices;
        }
        self.event_system.finish_frame(&mut self.frame);
        self.frame.refresh_visibility_counts();
        Ok(&self.frame)
    }

    pub fn dynamic_entity_count(&self) -> usize {
        self.dynamic_entities.len()
    }

    pub fn incremental_enabled(&self) -> bool {
        self.incremental_enabled
    }

    pub fn retained_puppet_enabled(&self) -> bool {
        self.incremental_enabled && self.retained_puppet_enabled
    }

    pub fn resolved_frame(&self) -> &ResolvedSemanticFrame {
        &self.frame
    }

    pub fn retained_script_deltas(&self) -> &[SceneScriptDelta] {
        &self.script_deltas
    }

    pub fn script_memory_snapshot(
        &self,
    ) -> Option<crate::engine::scene::SceneScriptMemorySnapshot> {
        self.script_runtime
            .as_ref()
            .map(SceneScriptRuntime::memory_snapshot)
    }

    fn dispatch_scripts(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        scene_time_seconds: f32,
        events: &SceneFrameEvents,
    ) -> Result<(), SceneSemanticWorldError> {
        self.collect_pointer_clicks(world, events);
        let Some(runtime) = self.script_runtime.as_ref() else {
            return Ok(());
        };
        let mut dirty = SceneScriptSubscriptions::FRAME;
        if Some(events.pointer.sequence) != self.last_pointer_sequence {
            dirty = dirty.union(SceneScriptSubscriptions::POINTER);
            self.last_pointer_sequence = Some(events.pointer.sequence);
        }
        if !self.pointer_clicks.is_empty() {
            dirty = dirty.union(SceneScriptSubscriptions::POINTER_CLICK);
        }
        if Some(events.audio.sequence) != self.last_audio_sequence {
            dirty = dirty.union(SceneScriptSubscriptions::AUDIO);
            self.last_audio_sequence = Some(events.audio.sequence);
        }
        let media_sequence = events.media.map(|media| media.sequence);
        if media_sequence != self.last_media_sequence {
            dirty = dirty.union(SceneScriptSubscriptions::MEDIA);
            self.last_media_sequence = media_sequence;
        }
        if let Some(local_time) = events.local_time {
            dirty = dirty.union(local_time_dirty_events(
                local_time,
                &mut self.last_local_minute,
                &mut self.last_local_second,
            ));
        }
        let pointer = events
            .pointer
            .normalized_position_top_left()
            .unwrap_or([0.5; 2]);
        let zero_spectrum = [0.0; 32];
        let spectrum = events.coherent_audio_spectrum().unwrap_or(&zero_spectrum);
        let frame_time_seconds = self
            .last_scene_time_seconds
            .map(|previous| (scene_time_seconds - previous).max(0.0))
            .unwrap_or(0.0);
        self.last_scene_time_seconds = Some(scene_time_seconds);
        runtime
            .dispatch_into(
                SceneScriptFrameInput {
                    scene_time_seconds: f64::from(scene_time_seconds),
                    frame_time_seconds: f64::from(frame_time_seconds),
                    dirty_events: dirty,
                    pointer,
                    pointer_clicks: &self.pointer_clicks,
                    audio_spectrum32: spectrum,
                    media: events.media,
                },
                &mut self.script_delta_updates,
            )
            .map_err(|error| SceneSemanticWorldError::ScriptRuntime(error.to_string()))?;
        merge_retained_script_deltas(&self.script_delta_updates, &mut self.script_deltas);
        Ok(())
    }

    fn collect_pointer_clicks(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        events: &SceneFrameEvents,
    ) {
        self.pointer_clicks.clear();
        for sequenced in &events.ordered {
            let SceneEvent::Pointer(event) = &sequenced.event else {
                continue;
            };
            match event.kind {
                ScenePointerEventKind::Leave { .. } => self.pressed_click_targets.clear(),
                ScenePointerEventKind::Button {
                    button, pressed, ..
                } if button == LINUX_INPUT_BUTTON_LEFT => {
                    let target = hit_test_pointer_script_object(
                        world,
                        &self.frame,
                        event.position,
                        event.surface_size,
                    );
                    if pressed {
                        set_pressed_click_target(&mut self.pressed_click_targets, button, target);
                        continue;
                    }
                    let pressed_target =
                        take_pressed_click_target(&mut self.pressed_click_targets, button);
                    if let Some(object) = target.filter(|target| Some(*target) == pressed_target) {
                        let pointer = event_scene_normalized_position(
                            world,
                            event.position,
                            event.surface_size,
                        )
                        .unwrap_or([0.5; 2]);
                        self.pointer_clicks.push(SceneScriptPointerClick {
                            object,
                            button,
                            pointer,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

fn merge_retained_script_deltas(
    updates: &[SceneScriptDelta],
    retained: &mut Vec<SceneScriptDelta>,
) {
    for update in updates {
        if let Some(current) = retained.iter_mut().find(|current| {
            current.object == update.object
                && current.target == update.target
                && current.selector == update.selector
        }) {
            current.numeric = update.numeric;
            current.text.clone_from(&update.text);
        } else {
            retained.push(update.clone());
        }
    }
}

fn set_pressed_click_target(
    targets: &mut Vec<(u32, Option<SceneObjectHandle>)>,
    button: u32,
    target: Option<SceneObjectHandle>,
) {
    if let Some(entry) = targets.iter_mut().find(|entry| entry.0 == button) {
        entry.1 = target;
    } else {
        targets.push((button, target));
    }
}

fn take_pressed_click_target(
    targets: &mut Vec<(u32, Option<SceneObjectHandle>)>,
    button: u32,
) -> Option<SceneObjectHandle> {
    let index = targets.iter().position(|entry| entry.0 == button)?;
    targets.swap_remove(index).1
}

fn hit_test_pointer_script_object(
    world: &SceneSemanticWorld<'_>,
    frame: &ResolvedSemanticFrame,
    position: [f64; 2],
    surface_size: [u32; 2],
) -> Option<SceneObjectHandle> {
    let normalized = event_scene_normalized_position(world, position, surface_size)?;
    let project = world.storage.project();
    let scene_point = [
        normalized[0] * project.logical_width.max(1) as f32,
        normalized[1] * project.logical_height.max(1) as f32,
    ];
    frame
        .objects
        .iter()
        .filter(|object| {
            object.resolved_visible && object_handles_pointer_click(world, object.object)
        })
        .filter(|object| object_mesh_contains_scene_point(world, object, scene_point))
        .max_by_key(|object| (object.sort_order, object.object_index))
        .map(|object| object.object)
}

fn event_scene_normalized_position(
    world: &SceneSemanticWorld<'_>,
    position: [f64; 2],
    surface_size: [u32; 2],
) -> Option<[f32; 2]> {
    let [width, height] = surface_size;
    if width == 0
        || height == 0
        || position[0] < 0.0
        || position[1] < 0.0
        || position[0] > f64::from(width)
        || position[1] > f64::from(height)
    {
        return None;
    }
    let normalized = [
        (position[0] / f64::from(width)) as f32,
        (position[1] / f64::from(height)) as f32,
    ];
    let project = world.storage.project();
    Some(cover_mapped_position(
        normalized,
        [project.logical_width, project.logical_height],
        surface_size,
    ))
}

fn object_handles_pointer_click(world: &SceneSemanticWorld<'_>, object: SceneObjectHandle) -> bool {
    world.storage.script_programs().iter().any(|program| {
        program.object == object
            && program
                .subscriptions
                .contains(SceneScriptSubscriptions::POINTER_CLICK)
    })
}

fn object_mesh_contains_scene_point(
    world: &SceneSemanticWorld<'_>,
    object: &ResolvedObjectState,
    scene_point: [f32; 2],
) -> bool {
    let Some(inverse) = inverse_affine_matrix(&object.render_world_matrix) else {
        return false;
    };
    let near = transform_point(&inverse, [scene_point[0], scene_point[1], -1.0]);
    let far = transform_point(&inverse, [scene_point[0], scene_point[1], 1.0]);
    let direction = [far[0] - near[0], far[1] - near[1], far[2] - near[2]];
    if direction[2].abs() <= 1.0e-8 {
        return false;
    }
    let distance = -near[2] / direction[2];
    let local = [
        near[0] + direction[0] * distance,
        near[1] + direction[1] * distance,
    ];
    world.storage.meshes().iter().any(|mesh| {
        mesh.object == object.object
            && local[0] >= mesh.bounds_min.x
            && local[0] <= mesh.bounds_max.x
            && local[1] >= mesh.bounds_min.y
            && local[1] <= mesh.bounds_max.y
    })
}

fn transform_point(matrix: &[f32; 16], point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
        matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
        matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
    ]
}

fn refresh_object_effects(frame: &mut ResolvedSemanticFrame, deltas: &[SceneScriptDelta]) {
    for effect in &mut frame.object_effects {
        if let Some(delta) = deltas.iter().rev().find(|delta| {
            delta.target == crate::engine::scene::SceneScriptTarget::EffectVisible
                && delta.object == effect.object
                && delta.selector == effect.binding_index
        }) {
            effect.self_visible = delta.numeric[0] != 0.0;
        }
        effect.object_resolved_visible = frame
            .objects
            .get(effect.object_index as usize)
            .is_some_and(|object| object.object == effect.object && object.resolved_visible);
        effect.resolved_visible = effect.self_visible && effect.object_resolved_visible;
    }
}

fn merge_script_text_deltas(
    deltas: &[SceneScriptDelta],
    values: &mut Vec<crate::engine::scene::semantic_world::ResolvedScriptTextValue>,
) {
    for delta in deltas {
        let Some(text) = delta.text.as_ref() else {
            continue;
        };
        if let Some(value) = values.iter_mut().find(|value| value.object == delta.object) {
            value.text.clone_from(text);
        } else {
            values.push(
                crate::engine::scene::semantic_world::ResolvedScriptTextValue {
                    object: delta.object,
                    text: text.clone(),
                },
            );
        }
    }
}

fn merge_script_material_deltas(
    deltas: &[SceneScriptDelta],
    values: &mut Vec<crate::engine::scene::semantic_world::ResolvedAudioBandMaterialValue>,
) {
    for delta in deltas {
        let target = match delta.target {
            crate::engine::scene::SceneScriptTarget::TechCircleSectorWidth => {
                crate::engine::scene::SceneAudioBandMaterialTarget::TechCircleSectorWidth
            }
            _ => continue,
        };
        if let Some(value) = values
            .iter_mut()
            .find(|value| value.object == delta.object && value.target == target)
        {
            value.value = delta.numeric[0];
        } else {
            values.push(
                crate::engine::scene::semantic_world::ResolvedAudioBandMaterialValue {
                    object: delta.object,
                    target,
                    value: delta.numeric[0],
                },
            );
        }
    }
}

fn retained_puppet_topologies(
    world: &SceneSemanticWorld<'_>,
) -> Result<Vec<RetainedPuppetTopology>, SceneSemanticWorldError> {
    world
        .storage
        .puppets()
        .iter()
        .map(|puppet| {
            let source_bones = world.storage.puppet_bones(puppet);
            let mut bind_world = Vec::with_capacity(source_bones.len());
            let mut bones = Vec::with_capacity(source_bones.len());
            for bone in source_bones {
                let parent_slot = parent_bone_slot(source_bones, bones.len(), bone);
                let parent_bind = parent_slot
                    .map(|slot| bind_world[slot])
                    .unwrap_or_else(identity_matrix);
                let bind_matrix = multiply_matrix(&parent_bind, &bone.local_bind_matrix);
                let inverse_bind = inverse_affine_matrix(&bind_matrix).ok_or(
                    SceneSemanticWorldError::NonInvertiblePuppetBindMatrix {
                        object: puppet.object,
                        bone_index: bone.bone_index,
                    },
                )?;
                bind_world.push(bind_matrix);
                bones.push(RetainedPuppetBone {
                    bone_index: bone.bone_index,
                    parent_slot,
                    inverse_bind,
                });
            }
            Ok(RetainedPuppetTopology {
                sampled_local: Vec::with_capacity(bones.len()),
                animated_world: Vec::with_capacity(bones.len()),
                attachment_world: Vec::with_capacity(bones.len()),
                attachment_object_world: None,
                bones,
            })
        })
        .collect()
}

fn prepare_retained_puppet_samples(
    world: &SceneSemanticWorld<'_>,
    topologies: &mut [RetainedPuppetTopology],
    scene_time_seconds: f32,
) {
    for (puppet_index, (puppet, topology)) in
        world.storage.puppets().iter().zip(topologies).enumerate()
    {
        topology.sampled_local.clear();
        topology.attachment_world.clear();
        topology.attachment_object_world = None;
        topology
            .sampled_local
            .extend(world.storage.puppet_bones(puppet).iter().map(|bone| {
                sampled_puppet_bone_local_state(
                    world.storage,
                    puppet.object,
                    puppet_index as u32,
                    bone,
                    scene_time_seconds,
                )
            }));
    }
}

pub(super) fn resolve_retained_attachment_anchor(
    topologies: &mut [RetainedPuppetTopology],
    parent_state: &ResolvedObjectState,
    attachment: &ScenePuppetAttachmentRecord,
) -> Option<[f32; 16]> {
    let topology = topologies.get_mut(parent_state.puppet_index as usize)?;
    if topology.attachment_object_world != Some(parent_state.world_matrix) {
        topology.attachment_world.clear();
        for (retained, sampled) in topology.bones.iter().zip(&topology.sampled_local) {
            let parent = retained
                .parent_slot
                .map(|slot| topology.attachment_world[slot])
                .unwrap_or(parent_state.world_matrix);
            topology
                .attachment_world
                .push(multiply_matrix(&parent, &sampled.matrix));
        }
        topology.attachment_object_world = Some(parent_state.world_matrix);
    }
    let bone_slot = world_bone_slot(topology, attachment.bone_index)?;
    Some(multiply_matrix(
        &topology.attachment_world[bone_slot],
        &attachment.local_matrix,
    ))
}

fn world_bone_slot(topology: &RetainedPuppetTopology, bone_index: u32) -> Option<usize> {
    topology
        .bones
        .iter()
        .position(|bone| bone.bone_index == bone_index)
}

fn parent_bone_slot(
    bones: &[ScenePuppetBoneRecord],
    child_slot: usize,
    child: &ScenePuppetBoneRecord,
) -> Option<usize> {
    (child.parent_index >= 0)
        .then(|| {
            bones[..child_slot]
                .iter()
                .position(|bone| bone.bone_index == child.parent_index as u32)
        })
        .flatten()
}

fn resolve_retained_puppets(
    world: &SceneSemanticWorld<'_>,
    objects: &[ResolvedObjectState],
    topologies: &mut [RetainedPuppetTopology],
    palettes: &mut Vec<ResolvedPuppetBonePalette>,
    matrices: &mut Vec<ResolvedPuppetBoneMatrix>,
) {
    palettes.clear();
    matrices.clear();
    for object in objects {
        if object.puppet_index == INVALID_RESOLVED_INDEX {
            continue;
        }
        let puppet_index = object.puppet_index as usize;
        let puppet = &world.storage.puppets()[puppet_index];
        let source_bones = world.storage.puppet_bones(puppet);
        let topology = &mut topologies[puppet_index];
        topology.animated_world.clear();
        let bone_start = matrices.len() as u32;
        for (bone_slot, (bone, retained)) in source_bones.iter().zip(&topology.bones).enumerate() {
            let animated_parent = retained
                .parent_slot
                .map(|parent_slot| topology.animated_world[parent_slot])
                .unwrap_or_else(identity_matrix);
            let animated_local = topology.sampled_local[bone_slot];
            let animated_matrix = multiply_matrix(&animated_parent, &animated_local.matrix);
            topology.animated_world.push(animated_matrix);
            matrices.push(ResolvedPuppetBoneMatrix {
                puppet_index: object.puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
                matrix: multiply_matrix(&animated_matrix, &retained.inverse_bind),
                alpha: animated_local.alpha,
            });
            debug_assert_eq!(topology.animated_world.len(), bone_slot + 1);
        }
        palettes.push(ResolvedPuppetBonePalette {
            object: object.object,
            puppet_index: object.puppet_index,
            bone_start,
            bone_count: source_bones.len() as u32,
            resolved_visible: object.resolved_visible,
        });
    }
}

fn dynamic_entity_closure(world: &SceneSemanticWorld<'_>) -> Vec<usize> {
    let mut dynamic = world
        .transform_animations
        .iter()
        .map(Option::is_some)
        .collect::<Vec<_>>();

    for program in world.storage.script_programs() {
        if let Some(entity) = world.index.entity_for_object(program.object) {
            dynamic[entity.index()] = true;
        }
    }

    for binding in world.user_property_bindings() {
        if let Some(entity) = world.index.entity_for_object(binding.object) {
            dynamic[entity.index()] = true;
        }
    }

    // Attachment anchors may be driven by puppet animation. Treat every attachment as dynamic;
    // unresolved/non-puppet attachments are rare and this keeps future puppet binding changes safe.
    for (entity_index, parent) in world.parents.iter().enumerate() {
        if parent.is_some_and(|parent| parent.attachment.is_some()) {
            dynamic[entity_index] = true;
        }
    }

    // A dynamic world transform dirties every descendant, matching Godot's transform propagation.
    loop {
        let mut changed = false;
        for (entity_index, parent) in world.parents.iter().enumerate() {
            let Some(parent) = parent else {
                continue;
            };
            let parent_is_dynamic = world
                .index
                .entity_for_we_id(parent.parent_we_id)
                .is_some_and(|parent| dynamic[parent.index()]);
            if parent_is_dynamic && !dynamic[entity_index] {
                dynamic[entity_index] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    dynamic
        .into_iter()
        .enumerate()
        .filter_map(|(entity_index, dynamic)| dynamic.then_some(entity_index))
        .collect()
}
