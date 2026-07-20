//! ECS-like semantic world for scene runtime semantics.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `references/godot/servers/rendering/storage/*`
pub mod components;
mod dynamic_property;
pub mod effect;
pub mod entity;
mod errors;
mod event_system;
pub mod indexes;
pub mod matrix;
mod pointer_parallax;
pub mod resolved_frame;
mod semantic_resolution;
pub mod timeline;
pub mod transform_animation;
pub use components::{
    MaterialBindingComponent, MeshBindingComponent, ParentComponent, ParticleEmitterComponent,
    PuppetBindingComponent, SemanticMeshBinding, SemanticRenderPlanInputs, TransformComponent,
    VisibilityComponent, VisualComponent,
};
use components::{
    material_binding_from_object, parent_from_object, particle_emitter_from_record,
    puppet_binding_from_record, transform_from_object, visibility_from_object, visual_from_object,
};
use dynamic_property::{
    ResolvedParentState, apply_script_transform, multiply_color, script_scalar, script_vector,
};
use effect::object_effect_binding_from_object;
pub use effect::{
    ObjectEffectBindingComponent, ResolvedObjectEffectState, SemanticObjectEffectBinding,
};
pub use entity::{SemanticEntity, SemanticEntityRecord};
pub use errors::SceneSemanticWorldError;
use event_system::RetainedSceneEventSystem;
use indexes::SemanticIndexTable;
use matrix::{identity_matrix, inverse_affine_matrix, multiply_matrix, transform_matrix};
use resolved_frame::{INVALID_RESOLVED_INDEX, ResolvedObjectMeshRange};
pub use resolved_frame::{
    ResolvedAttachmentLink, ResolvedAudioBandMaterialValue, ResolvedObjectState,
    ResolvedPuppetBoneMatrix, ResolvedPuppetBonePalette, ResolvedScriptTextValue,
    ResolvedSemanticFrame,
};
pub use semantic_resolution::SemanticFrameResolver;
use semantic_resolution::{RetainedPuppetTopology, resolve_retained_attachment_anchor};
use timeline::{sampled_object_transform, sampled_puppet_bone_local_state};
pub use transform_animation::TransformAnimationComponent;

use super::abi::*;
use super::storage::SceneStorage;
use super::{SceneScriptDelta, SceneScriptTarget};

#[derive(Debug)]
pub struct SceneSemanticWorld<'a> {
    storage: &'a SceneStorage,
    entities: Vec<SemanticEntityRecord>,
    index: SemanticIndexTable,
    transforms: Vec<Option<TransformComponent>>,
    transform_animations: Vec<Option<TransformAnimationComponent>>,
    parents: Vec<Option<ParentComponent>>,
    visibility: Vec<Option<VisibilityComponent>>,
    visuals: Vec<Option<VisualComponent>>,
    material_bindings: Vec<Option<MaterialBindingComponent>>,
    mesh_components: Vec<Option<MeshBindingComponent>>,
    object_effect_components: Vec<Option<ObjectEffectBindingComponent>>,
    puppet_components: Vec<Option<PuppetBindingComponent>>,
    particle_components: Vec<Option<ParticleEmitterComponent>>,
    mesh_bindings: Vec<SemanticMeshBinding>,
    object_effect_bindings: Vec<SemanticObjectEffectBinding>,
}

impl<'a> SceneSemanticWorld<'a> {
    pub fn from_storage(storage: &'a SceneStorage) -> Result<Self, SceneSemanticWorldError> {
        let object_count = storage.objects().len();
        let mut world = Self {
            storage,
            entities: Vec::with_capacity(object_count),
            index: SemanticIndexTable::default(),
            transforms: Vec::with_capacity(object_count),
            transform_animations: Vec::with_capacity(object_count),
            parents: Vec::with_capacity(object_count),
            visibility: Vec::with_capacity(object_count),
            visuals: Vec::with_capacity(object_count),
            material_bindings: Vec::with_capacity(object_count),
            mesh_components: Vec::with_capacity(object_count),
            object_effect_components: Vec::with_capacity(object_count),
            puppet_components: Vec::with_capacity(object_count),
            particle_components: Vec::with_capacity(object_count),
            mesh_bindings: Vec::with_capacity(storage.meshes().len()),
            object_effect_bindings: storage
                .object_effects()
                .iter()
                .map(SemanticObjectEffectBinding::from_record)
                .collect(),
        };

        for (object_index, object) in storage.objects().iter().enumerate() {
            world.push_object(object_index, object)?;
        }
        world.validate_object_effect_ranges()?;
        world.validate_mesh_objects()?;
        for entity_index in 0..world.entities.len() {
            let object = world.entities[entity_index].object;
            world.push_mesh_bindings_for_object(entity_index, object)?;
        }
        for (puppet_index, puppet) in storage.puppets().iter().enumerate() {
            world.push_puppet_binding(puppet_index, puppet)?;
        }
        for (particle_index, particle) in storage.particles().iter().enumerate() {
            if let Some(entity) = world.index.entity_for_object(particle.object) {
                world.particle_components[entity.index()] = Some(particle_emitter_from_record(
                    particle_index as u32,
                    particle,
                ));
            }
        }

        Ok(world)
    }

    pub fn storage(&self) -> &'a SceneStorage {
        self.storage
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn entities(&self) -> &[SemanticEntityRecord] {
        &self.entities
    }

    pub fn entity_for_object(&self, object: SceneObjectHandle) -> Option<SemanticEntity> {
        self.index.entity_for_object(object)
    }

    pub fn entity_for_we_id(&self, we_id: u32) -> Option<SemanticEntity> {
        self.index.entity_for_we_id(we_id)
    }

    pub fn entity_record(&self, entity: SemanticEntity) -> Option<&SemanticEntityRecord> {
        self.entities.get(entity.index())
    }

    pub fn object_record(&self, object: SceneObjectHandle) -> Option<&'a SceneObjectRecord> {
        let entity = self.entity_for_object(object)?;
        let record = self.entity_record(entity)?;
        self.storage.objects().get(record.object_index as usize)
    }

    pub fn transform(&self, object: SceneObjectHandle) -> Option<&TransformComponent> {
        self.component_for_object(object, &self.transforms)
    }

    pub fn transform_animation(
        &self,
        object: SceneObjectHandle,
    ) -> Option<&TransformAnimationComponent> {
        self.component_for_object(object, &self.transform_animations)
    }

    pub fn parent(&self, object: SceneObjectHandle) -> Option<&ParentComponent> {
        self.component_for_object(object, &self.parents)
    }

    pub fn visibility(&self, object: SceneObjectHandle) -> Option<&VisibilityComponent> {
        self.component_for_object(object, &self.visibility)
    }

    pub fn visual(&self, object: SceneObjectHandle) -> Option<&VisualComponent> {
        self.component_for_object(object, &self.visuals)
    }

    pub fn material_binding(&self, object: SceneObjectHandle) -> Option<&MaterialBindingComponent> {
        self.component_for_object(object, &self.material_bindings)
    }

    pub fn mesh_component(&self, object: SceneObjectHandle) -> Option<&MeshBindingComponent> {
        self.component_for_object(object, &self.mesh_components)
    }

    pub fn object_mesh_bindings(&self, object: SceneObjectHandle) -> &[SemanticMeshBinding] {
        let Some(component) = self.mesh_component(object) else {
            return &[];
        };
        let start = component.binding_start as usize;
        let end = start + component.binding_count as usize;
        self.mesh_bindings.get(start..end).unwrap_or(&[])
    }

    pub fn object_effect_component(
        &self,
        object: SceneObjectHandle,
    ) -> Option<&ObjectEffectBindingComponent> {
        self.component_for_object(object, &self.object_effect_components)
    }

    pub fn object_effect_bindings(
        &self,
        object: SceneObjectHandle,
    ) -> &[SemanticObjectEffectBinding] {
        let Some(component) = self.object_effect_component(object) else {
            return &[];
        };
        let start = component.binding_start as usize;
        let end = start + component.binding_count as usize;
        self.object_effect_bindings.get(start..end).unwrap_or(&[])
    }

    pub fn puppet_binding(&self, object: SceneObjectHandle) -> Option<&PuppetBindingComponent> {
        self.component_for_object(object, &self.puppet_components)
    }

    pub fn particle_emitter(&self, object: SceneObjectHandle) -> Option<&ParticleEmitterComponent> {
        self.component_for_object(object, &self.particle_components)
    }

    pub fn puppet_bones(&self, object: SceneObjectHandle) -> Option<&'a [ScenePuppetBoneRecord]> {
        let puppet = self.puppet_record(object)?;
        Some(self.storage.puppet_bones(puppet))
    }

    pub fn puppet_attachments(
        &self,
        object: SceneObjectHandle,
    ) -> Option<&'a [ScenePuppetAttachmentRecord]> {
        let puppet = self.puppet_record(object)?;
        Some(self.storage.puppet_attachments(puppet))
    }

    pub fn render_plan_inputs(&self) -> SemanticRenderPlanInputs {
        SemanticRenderPlanInputs {
            object_count: self.entities.len(),
            visible_object_count: self
                .visibility
                .iter()
                .flatten()
                .filter(|component| component.visible)
                .count(),
            mesh_binding_count: self.mesh_bindings.len(),
            effect_binding_count: self.object_effect_bindings.len(),
            puppet_binding_count: self
                .puppet_components
                .iter()
                .filter(|component| component.is_some())
                .count(),
        }
    }

    pub fn resolve_frame(&self) -> Result<ResolvedSemanticFrame, SceneSemanticWorldError> {
        self.resolve_frame_at(0.0)
    }

    pub fn resolve_frame_at(
        &self,
        scene_time_seconds: f32,
    ) -> Result<ResolvedSemanticFrame, SceneSemanticWorldError> {
        self.resolve_frame_with_dynamic_values_at(scene_time_seconds, &[])
    }

    fn resolve_frame_with_dynamic_values_at(
        &self,
        scene_time_seconds: f32,
        script_deltas: &[SceneScriptDelta],
    ) -> Result<ResolvedSemanticFrame, SceneSemanticWorldError> {
        let mut states = vec![None; self.entities.len()];
        let mut visits = vec![ResolveVisitState::Unvisited; self.entities.len()];
        let mut attachment_links = Vec::new();
        let mut retained_puppets = None;

        for entity_index in 0..self.entities.len() {
            self.resolve_entity(
                entity_index,
                &mut visits,
                &mut states,
                &mut attachment_links,
                &mut retained_puppets,
                script_deltas,
                scene_time_seconds,
            )?;
        }

        let objects = states
            .into_iter()
            .map(|state| state.expect("resolve_entity populates every requested object"))
            .collect::<Vec<_>>();
        let object_effects = self.resolve_object_effects(&objects, script_deltas)?;
        let (puppet_bone_palettes, puppet_bone_matrices) =
            self.resolve_puppet_bone_palettes(&objects, scene_time_seconds)?;
        Ok(ResolvedSemanticFrame::from_resolved_parts(
            objects,
            object_effects,
            attachment_links,
            puppet_bone_palettes,
            puppet_bone_matrices,
        ))
    }

    fn push_object(
        &mut self,
        object_index: usize,
        object: &SceneObjectRecord,
    ) -> Result<(), SceneSemanticWorldError> {
        if object.id.0 as usize != object_index {
            return Err(SceneSemanticWorldError::NonCanonicalObjectHandle {
                object_index,
                handle: object.id,
            });
        }
        let entity_raw = u32::try_from(self.entities.len()).map_err(|_| {
            SceneSemanticWorldError::TooManyEntities {
                count: self.entities.len(),
            }
        })?;
        let entity = SemanticEntity::from_raw(entity_raw);
        if self.index.insert_object(object.id, entity).is_some() {
            return Err(SceneSemanticWorldError::DuplicateObjectHandle { handle: object.id });
        }
        if object.we_id != INVALID_OBJECT_ID
            && self.index.insert_we_id(object.we_id, entity).is_some()
        {
            return Err(SceneSemanticWorldError::DuplicateWeId {
                we_id: object.we_id,
            });
        }

        self.entities.push(SemanticEntityRecord {
            entity,
            object: object.id,
            object_index: object_index as u32,
        });
        self.transforms.push(Some(transform_from_object(object)));
        self.transform_animations
            .push(TransformAnimationComponent::from_storage(
                self.storage,
                object.id,
            ));
        self.parents.push(parent_from_object(object));
        self.visibility.push(Some(visibility_from_object(object)));
        self.visuals.push(Some(visual_from_object(object)));
        self.material_bindings
            .push(material_binding_from_object(object));
        self.mesh_components.push(None);
        self.object_effect_components
            .push(object_effect_binding_from_object(object));
        self.puppet_components.push(None);
        self.particle_components.push(None);
        Ok(())
    }

    fn validate_object_effect_ranges(&self) -> Result<(), SceneSemanticWorldError> {
        for object in self.storage.objects() {
            for (range_index, effect) in self
                .storage
                .object_effects_for_object(object)
                .iter()
                .enumerate()
            {
                if effect.object != object.id {
                    return Err(SceneSemanticWorldError::ObjectEffectRangeMismatch {
                        object: object.id,
                        range_index,
                        effect_object: effect.object,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_mesh_objects(&self) -> Result<(), SceneSemanticWorldError> {
        for (mesh_index, mesh) in self.storage.meshes().iter().enumerate() {
            if self.index.entity_for_object(mesh.object).is_none() {
                return Err(SceneSemanticWorldError::MissingObjectForMesh {
                    mesh_index,
                    object: mesh.object,
                });
            }
        }
        Ok(())
    }

    fn push_mesh_bindings_for_object(
        &mut self,
        slot: usize,
        object: SceneObjectHandle,
    ) -> Result<(), SceneSemanticWorldError> {
        let binding_start = u32::try_from(self.mesh_bindings.len()).map_err(|_| {
            SceneSemanticWorldError::TooManyMeshBindings {
                count: self.mesh_bindings.len(),
            }
        })?;
        let meshes = self.storage.meshes();
        for (mesh_index, mesh) in meshes.iter().enumerate() {
            if mesh.object != object {
                continue;
            }
            let mesh_index = u32::try_from(mesh_index).map_err(|_| {
                SceneSemanticWorldError::TooManyMeshBindings {
                    count: meshes.len(),
                }
            })?;
            self.mesh_bindings
                .push(SemanticMeshBinding::from_mesh(mesh_index, mesh));
        }
        let binding_count = u32::try_from(self.mesh_bindings.len() - binding_start as usize)
            .map_err(|_| SceneSemanticWorldError::TooManyMeshBindings {
                count: self.mesh_bindings.len(),
            })?;
        if binding_count != 0 {
            self.mesh_components[slot] = Some(MeshBindingComponent {
                binding_start,
                binding_count,
            });
        }
        Ok(())
    }

    fn push_puppet_binding(
        &mut self,
        puppet_index: usize,
        puppet: &ScenePuppetRecord,
    ) -> Result<(), SceneSemanticWorldError> {
        let entity = self.index.entity_for_object(puppet.object).ok_or(
            SceneSemanticWorldError::MissingObjectForPuppet {
                puppet_index,
                object: puppet.object,
            },
        )?;
        let slot = entity.index();
        if self.puppet_components[slot].is_some() {
            return Err(SceneSemanticWorldError::DuplicatePuppetBinding {
                object: puppet.object,
            });
        }
        self.puppet_components[slot] =
            Some(puppet_binding_from_record(puppet_index as u32, puppet));
        Ok(())
    }

    fn resolve_entity(
        &self,
        entity_index: usize,
        visits: &mut [ResolveVisitState],
        states: &mut [Option<ResolvedObjectState>],
        attachment_links: &mut Vec<ResolvedAttachmentLink>,
        retained_puppets: &mut Option<&mut [RetainedPuppetTopology]>,
        script_deltas: &[SceneScriptDelta],
        scene_time_seconds: f32,
    ) -> Result<ResolvedObjectState, SceneSemanticWorldError> {
        match visits[entity_index] {
            ResolveVisitState::Resolved => {
                return Ok(states[entity_index]
                    .expect("resolved semantic frame state is populated before marking resolved"));
            }
            ResolveVisitState::Visiting => {
                return Err(SceneSemanticWorldError::ParentCycle {
                    object: self.entities[entity_index].object,
                });
            }
            ResolveVisitState::Unvisited => {}
        }
        visits[entity_index] = ResolveVisitState::Visiting;

        let entity_record = self.entities[entity_index];
        let object = self
            .storage
            .objects()
            .get(entity_record.object_index as usize)
            .ok_or(SceneSemanticWorldError::MissingObjectRecord {
                object: entity_record.object,
                object_index: entity_record.object_index,
            })?;
        let transform = self
            .transforms
            .get(entity_index)
            .and_then(Option::as_ref)
            .ok_or(SceneSemanticWorldError::MissingTransform {
                object: entity_record.object,
            })?;
        let visibility = self
            .visibility
            .get(entity_index)
            .and_then(Option::as_ref)
            .ok_or(SceneSemanticWorldError::MissingVisibility {
                object: entity_record.object,
            })?;
        let visual = self
            .visuals
            .get(entity_index)
            .and_then(Option::as_ref)
            .ok_or(SceneSemanticWorldError::MissingVisual {
                object: entity_record.object,
            })?;
        let mut sampled_transform = sampled_object_transform(
            self.storage,
            transform,
            self.transform_animations
                .get(entity_index)
                .and_then(Option::as_ref),
            scene_time_seconds,
        );
        apply_script_transform(script_deltas, object.id, &mut sampled_transform);
        let local_matrix = transform_matrix(&sampled_transform);
        let mesh_range = self
            .mesh_components
            .get(entity_index)
            .and_then(Option::as_ref)
            .map(ResolvedObjectMeshRange::from_component)
            .unwrap_or_default();
        let puppet_index = self
            .puppet_components
            .get(entity_index)
            .and_then(Option::as_ref)
            .map(|puppet| puppet.puppet_index)
            .unwrap_or(INVALID_RESOLVED_INDEX);

        let parent_state = self.resolve_parented_transform(
            object,
            local_matrix,
            visits,
            states,
            attachment_links,
            retained_puppets,
            script_deltas,
            scene_time_seconds,
        )?;
        let self_visible = script_scalar(script_deltas, object.id, SceneScriptTarget::Visible)
            .map(|value| value != 0.0)
            .unwrap_or(visibility.visible);
        let self_color = script_vector(script_deltas, object.id, SceneScriptTarget::Color)
            .unwrap_or(visual.color);
        let self_alpha = script_scalar(script_deltas, object.id, SceneScriptTarget::Alpha)
            .unwrap_or(visual.alpha)
            .clamp(0.0, 1.0);
        let state = ResolvedObjectState {
            entity: entity_record.entity,
            object: entity_record.object,
            object_index: entity_record.object_index,
            parent: parent_state.parent,
            parent_we_id: object.parent_we_id,
            attachment: object.attachment,
            local_matrix,
            world_matrix: parent_state.world_matrix,
            render_world_matrix: parent_state.world_matrix,
            self_visible,
            resolved_visible: self_visible && parent_state.inherited_visible,
            self_color,
            resolved_color: multiply_color(self_color, parent_state.inherited_color),
            self_alpha,
            resolved_alpha: (self_alpha * parent_state.inherited_alpha).clamp(0.0, 1.0),
            sort_order: visibility.sort_order,
            mesh_binding_start: mesh_range.binding_start,
            mesh_binding_count: mesh_range.binding_count,
            puppet_index,
        };
        states[entity_index] = Some(state);
        visits[entity_index] = ResolveVisitState::Resolved;
        Ok(state)
    }

    fn resolve_parented_transform(
        &self,
        object: &SceneObjectRecord,
        local_matrix: [f32; 16],
        visits: &mut [ResolveVisitState],
        states: &mut [Option<ResolvedObjectState>],
        attachment_links: &mut Vec<ResolvedAttachmentLink>,
        retained_puppets: &mut Option<&mut [RetainedPuppetTopology]>,
        script_deltas: &[SceneScriptDelta],
        scene_time_seconds: f32,
    ) -> Result<ResolvedParentState, SceneSemanticWorldError> {
        if object.parent_we_id == INVALID_OBJECT_ID {
            if object.attachment.is_some() {
                return Err(SceneSemanticWorldError::AttachmentWithoutParent {
                    object: object.id,
                    attachment: object.attachment,
                });
            }
            return Ok(ResolvedParentState {
                parent: SceneObjectHandle(INVALID_OBJECT_ID),
                inherited_visible: true,
                inherited_color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                inherited_alpha: 1.0,
                world_matrix: local_matrix,
            });
        }

        let parent_entity = self.index.entity_for_we_id(object.parent_we_id).ok_or(
            SceneSemanticWorldError::MissingParentObject {
                object: object.id,
                parent_we_id: object.parent_we_id,
            },
        )?;
        let parent_state = self.resolve_entity(
            parent_entity.index(),
            visits,
            states,
            attachment_links,
            retained_puppets,
            script_deltas,
            scene_time_seconds,
        )?;
        let mut parent_anchor = parent_state.world_matrix;

        if object.attachment.is_some() {
            let (link, attachment_anchor) = self.resolve_attachment_link_at(
                object.id,
                &parent_state,
                object.attachment,
                retained_puppets,
                scene_time_seconds,
            );
            if let Some(attachment_anchor) = attachment_anchor {
                parent_anchor = attachment_anchor;
            }
            attachment_links.push(link);
        }

        Ok(ResolvedParentState {
            parent: parent_state.object,
            inherited_visible: parent_state.resolved_visible,
            inherited_color: parent_state.resolved_color,
            inherited_alpha: parent_state.resolved_alpha,
            world_matrix: multiply_matrix(&parent_anchor, &local_matrix),
        })
    }

    fn resolve_attachment_link_at(
        &self,
        child: SceneObjectHandle,
        parent_state: &ResolvedObjectState,
        attachment: SceneStringId,
        retained_puppets: &mut Option<&mut [RetainedPuppetTopology]>,
        scene_time_seconds: f32,
    ) -> (ResolvedAttachmentLink, Option<[f32; 16]>) {
        let parent = parent_state.object;
        let Some(parent_puppet) = self.puppet_binding(parent) else {
            return (
                ResolvedAttachmentLink::unresolved(child, parent, attachment),
                None,
            );
        };
        let Some(puppet) = self
            .storage
            .puppets()
            .get(parent_puppet.puppet_index as usize)
        else {
            return (
                ResolvedAttachmentLink::unresolved(child, parent, attachment),
                None,
            );
        };
        let requested = self.storage.string(attachment);
        for record in self.storage.puppet_attachments(puppet) {
            let matches_name = record.name == attachment
                || requested
                    .zip(self.storage.string(record.name))
                    .is_some_and(|(requested, candidate)| requested == candidate);
            if matches_name {
                return (
                    ResolvedAttachmentLink::resolved(
                        child,
                        parent,
                        attachment,
                        parent_puppet.puppet_index,
                        record.bone_index,
                    ),
                    retained_puppets
                        .as_deref_mut()
                        .and_then(|topologies| {
                            resolve_retained_attachment_anchor(topologies, parent_state, record)
                        })
                        .or_else(|| {
                            self.resolve_puppet_attachment_anchor(
                                parent_state,
                                puppet,
                                record,
                                scene_time_seconds,
                            )
                        }),
                );
            }
        }
        (
            ResolvedAttachmentLink::with_parent_puppet(
                child,
                parent,
                attachment,
                parent_puppet.puppet_index,
            ),
            None,
        )
    }

    fn resolve_puppet_bone_palettes(
        &self,
        objects: &[ResolvedObjectState],
        scene_time_seconds: f32,
    ) -> Result<
        (
            Vec<ResolvedPuppetBonePalette>,
            Vec<ResolvedPuppetBoneMatrix>,
        ),
        SceneSemanticWorldError,
    > {
        let mut palettes = Vec::new();
        let mut matrices = Vec::new();
        for object in objects {
            if object.puppet_index == INVALID_RESOLVED_INDEX {
                continue;
            }
            let puppet = self
                .storage
                .puppets()
                .get(object.puppet_index as usize)
                .ok_or(SceneSemanticWorldError::MissingPuppetRecord {
                    object: object.object,
                    puppet_index: object.puppet_index,
                })?;
            let bone_start = u32::try_from(matrices.len()).map_err(|_| {
                SceneSemanticWorldError::TooManyPuppetBones {
                    count: matrices.len(),
                }
            })?;
            let local_start = matrices.len();
            let mut bind_world = Vec::<(u32, [f32; 16])>::new();
            let mut animated_world = Vec::<(u32, [f32; 16])>::new();
            for bone in self.storage.puppet_bones(puppet) {
                let bind_parent = if bone.parent_index >= 0 {
                    bind_world
                        .iter()
                        .find(|(index, _)| *index == bone.parent_index as u32)
                        .map(|(_, matrix)| *matrix)
                } else {
                    None
                };
                let animated_parent = if bone.parent_index >= 0 {
                    animated_world
                        .iter()
                        .find(|(index, _)| *index == bone.parent_index as u32)
                        .map(|(_, matrix)| *matrix)
                } else {
                    None
                };
                let bind_matrix = multiply_matrix(
                    &bind_parent.unwrap_or_else(identity_matrix),
                    &bone.local_bind_matrix,
                );
                let animated_local = sampled_puppet_bone_local_state(
                    self.storage,
                    object.object,
                    object.puppet_index,
                    bone,
                    scene_time_seconds,
                );
                let animated_matrix = multiply_matrix(
                    &animated_parent.unwrap_or_else(identity_matrix),
                    &animated_local.matrix,
                );
                let inverse_bind = inverse_affine_matrix(&bind_matrix).ok_or(
                    SceneSemanticWorldError::NonInvertiblePuppetBindMatrix {
                        object: object.object,
                        bone_index: bone.bone_index,
                    },
                )?;
                bind_world.push((bone.bone_index, bind_matrix));
                animated_world.push((bone.bone_index, animated_matrix));
                matrices.push(ResolvedPuppetBoneMatrix {
                    puppet_index: object.puppet_index,
                    bone_index: bone.bone_index,
                    parent_index: bone.parent_index,
                    matrix: multiply_matrix(&animated_matrix, &inverse_bind),
                    alpha: animated_local.alpha,
                });
            }
            let bone_count = u32::try_from(matrices.len() - local_start).map_err(|_| {
                SceneSemanticWorldError::TooManyPuppetBones {
                    count: matrices.len(),
                }
            })?;
            palettes.push(ResolvedPuppetBonePalette {
                object: object.object,
                puppet_index: object.puppet_index,
                bone_start,
                bone_count,
                resolved_visible: object.resolved_visible,
            });
        }
        Ok((palettes, matrices))
    }

    fn resolve_object_effects(
        &self,
        objects: &[ResolvedObjectState],
        script_deltas: &[SceneScriptDelta],
    ) -> Result<Vec<ResolvedObjectEffectState>, SceneSemanticWorldError> {
        let mut effects = Vec::new();
        for object in objects {
            let Some(component) = self
                .object_effect_components
                .get(object.entity.index())
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let start = component.binding_start as usize;
            let end = start.saturating_add(component.binding_count as usize);
            let Some(bindings) = self.object_effect_bindings.get(start..end) else {
                return Err(SceneSemanticWorldError::ObjectEffectRangeOutOfBounds {
                    object: object.object,
                    start: component.binding_start,
                    count: component.binding_count,
                    len: self.object_effect_bindings.len(),
                });
            };
            for (local_index, binding) in bindings.iter().enumerate() {
                let binding_index = component.binding_start + local_index as u32;
                let effect_index = binding.effect.0;
                let effect = self.storage.effects().get(effect_index as usize).ok_or(
                    SceneSemanticWorldError::MissingEffectRecord {
                        object: object.object,
                        effect: binding.effect,
                    },
                )?;
                let self_visible = script_deltas
                    .iter()
                    .rev()
                    .find(|delta| {
                        delta.target == SceneScriptTarget::EffectVisible
                            && delta.object == binding.object
                            && delta.selector == binding_index
                    })
                    .map(|delta| delta.numeric[0] != 0.0)
                    .unwrap_or(binding.visible);
                effects.push(ResolvedObjectEffectState {
                    binding_index,
                    entity: object.entity,
                    object: object.object,
                    object_index: object.object_index,
                    effect: binding.effect,
                    effect_index,
                    instance_id: binding.instance_id,
                    self_visible,
                    object_resolved_visible: object.resolved_visible,
                    resolved_visible: self_visible && object.resolved_visible,
                    pass_start: effect.pass_start,
                    pass_count: effect.pass_count,
                    fbo_start: effect.fbo_start,
                    fbo_count: effect.fbo_count,
                });
            }
        }
        Ok(effects)
    }

    fn resolve_puppet_attachment_anchor(
        &self,
        parent_state: &ResolvedObjectState,
        puppet: &ScenePuppetRecord,
        attachment: &ScenePuppetAttachmentRecord,
        scene_time_seconds: f32,
    ) -> Option<[f32; 16]> {
        self.resolve_puppet_bone_world_matrix(
            parent_state.object,
            parent_state.world_matrix,
            parent_state.puppet_index,
            puppet,
            attachment.bone_index,
            scene_time_seconds,
        )
        .map(|bone_matrix| multiply_matrix(&bone_matrix, &attachment.local_matrix))
        .or_else(|| {
            Some(multiply_matrix(
                &parent_state.world_matrix,
                &attachment.local_matrix,
            ))
        })
    }

    fn resolve_puppet_bone_world_matrix(
        &self,
        object: SceneObjectHandle,
        object_world_matrix: [f32; 16],
        puppet_index: u32,
        puppet: &ScenePuppetRecord,
        bone_index: u32,
        scene_time_seconds: f32,
    ) -> Option<[f32; 16]> {
        let bones = self.storage.puppet_bones(puppet);
        let mut matrices = Vec::<ResolvedPuppetBoneMatrix>::with_capacity(bones.len());
        for bone in bones {
            let parent_matrix = if bone.parent_index >= 0 {
                matrices
                    .iter()
                    .find(|matrix| matrix.bone_index == bone.parent_index as u32)
                    .map(|matrix| matrix.matrix)
            } else {
                None
            };
            let base_matrix = parent_matrix.unwrap_or(object_world_matrix);
            let local_state = sampled_puppet_bone_local_state(
                self.storage,
                object,
                puppet_index,
                bone,
                scene_time_seconds,
            );
            let matrix = multiply_matrix(&base_matrix, &local_state.matrix);
            matrices.push(ResolvedPuppetBoneMatrix {
                puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
                matrix,
                alpha: local_state.alpha,
            });
            if bone.bone_index == bone_index {
                return Some(matrix);
            }
        }
        None
    }

    fn component_for_object<'components, T>(
        &self,
        object: SceneObjectHandle,
        components: &'components [Option<T>],
    ) -> Option<&'components T> {
        let entity = self.entity_for_object(object)?;
        components.get(entity.index())?.as_ref()
    }

    fn puppet_record(&self, object: SceneObjectHandle) -> Option<&'a ScenePuppetRecord> {
        let binding = self.puppet_binding(object)?;
        self.storage.puppets().get(binding.puppet_index as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveVisitState {
    Unvisited,
    Visiting,
    Resolved,
}

#[cfg(test)]
mod tests;
