//! ECS-like semantic world for scene runtime semantics.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`
//! - `references/godot/servers/rendering/storage/*`

pub mod components;
pub mod effect;
pub mod entity;
pub mod indexes;
pub mod matrix;
pub mod resolved_frame;
pub mod timeline;

pub use components::{
    MaterialBindingComponent, MeshBindingComponent, ParentComponent, PuppetBindingComponent,
    SemanticMeshBinding, SemanticRenderPlanInputs, TransformComponent, VisibilityComponent,
};
pub use effect::{
    ObjectEffectBindingComponent, ResolvedObjectEffectState, SemanticObjectEffectBinding,
};
pub use entity::{SemanticEntity, SemanticEntityRecord};
pub use resolved_frame::{
    ResolvedAttachmentLink, ResolvedObjectState, ResolvedPuppetBoneMatrix,
    ResolvedPuppetBonePalette, ResolvedSemanticFrame,
};

use std::fmt;

use components::{
    material_binding_from_object, parent_from_object, puppet_binding_from_record,
    transform_from_object, visibility_from_object,
};
use effect::object_effect_binding_from_object;
use indexes::SemanticIndexTable;
use matrix::{multiply_matrix, transform_matrix};
use resolved_frame::{INVALID_RESOLVED_INDEX, ResolvedObjectMeshRange};
use timeline::sampled_puppet_bone_local_matrix;

use super::abi::*;
use super::storage::SceneStorage;

#[derive(Debug)]
pub struct SceneSemanticWorld<'a> {
    storage: &'a SceneStorage,
    entities: Vec<SemanticEntityRecord>,
    index: SemanticIndexTable,
    transforms: Vec<Option<TransformComponent>>,
    parents: Vec<Option<ParentComponent>>,
    visibility: Vec<Option<VisibilityComponent>>,
    material_bindings: Vec<Option<MaterialBindingComponent>>,
    mesh_components: Vec<Option<MeshBindingComponent>>,
    object_effect_components: Vec<Option<ObjectEffectBindingComponent>>,
    puppet_components: Vec<Option<PuppetBindingComponent>>,
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
            parents: Vec::with_capacity(object_count),
            visibility: Vec::with_capacity(object_count),
            material_bindings: Vec::with_capacity(object_count),
            mesh_components: Vec::with_capacity(object_count),
            object_effect_components: Vec::with_capacity(object_count),
            puppet_components: Vec::with_capacity(object_count),
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

    pub fn parent(&self, object: SceneObjectHandle) -> Option<&ParentComponent> {
        self.component_for_object(object, &self.parents)
    }

    pub fn visibility(&self, object: SceneObjectHandle) -> Option<&VisibilityComponent> {
        self.component_for_object(object, &self.visibility)
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
        let mut states = vec![None; self.entities.len()];
        let mut visits = vec![ResolveVisitState::Unvisited; self.entities.len()];
        let mut attachment_links = Vec::new();

        for entity_index in 0..self.entities.len() {
            self.resolve_entity(
                entity_index,
                &mut visits,
                &mut states,
                &mut attachment_links,
                scene_time_seconds,
            )?;
        }

        let objects = states
            .into_iter()
            .map(|state| state.expect("resolve_entity populates every requested object"))
            .collect::<Vec<_>>();
        let object_effects = self.resolve_object_effects(&objects)?;
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
        self.parents.push(parent_from_object(object));
        self.visibility.push(Some(visibility_from_object(object)));
        self.material_bindings
            .push(material_binding_from_object(object));
        self.mesh_components.push(None);
        self.object_effect_components
            .push(object_effect_binding_from_object(object));
        self.puppet_components.push(None);
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
        let local_matrix = transform_matrix(transform);
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

        let (parent, inherited_visible, world_matrix) = self.resolve_parented_transform(
            object,
            local_matrix,
            visits,
            states,
            attachment_links,
            scene_time_seconds,
        )?;
        let state = ResolvedObjectState {
            entity: entity_record.entity,
            object: entity_record.object,
            object_index: entity_record.object_index,
            parent,
            parent_we_id: object.parent_we_id,
            attachment: object.attachment,
            local_matrix,
            world_matrix,
            self_visible: visibility.visible,
            resolved_visible: visibility.visible && inherited_visible,
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
        scene_time_seconds: f32,
    ) -> Result<(SceneObjectHandle, bool, [f32; 16]), SceneSemanticWorldError> {
        if object.parent_we_id == INVALID_OBJECT_ID {
            if object.attachment.is_some() {
                return Err(SceneSemanticWorldError::AttachmentWithoutParent {
                    object: object.id,
                    attachment: object.attachment,
                });
            }
            return Ok((SceneObjectHandle(INVALID_OBJECT_ID), true, local_matrix));
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
            scene_time_seconds,
        )?;
        let mut parent_anchor = parent_state.world_matrix;

        if object.attachment.is_some() {
            let (link, attachment_anchor) = self.resolve_attachment_link_at(
                object.id,
                &parent_state,
                object.attachment,
                scene_time_seconds,
            );
            if let Some(attachment_anchor) = attachment_anchor {
                parent_anchor = attachment_anchor;
            }
            attachment_links.push(link);
        }

        Ok((
            parent_state.object,
            parent_state.resolved_visible,
            multiply_matrix(&parent_anchor, &local_matrix),
        ))
    }

    fn resolve_attachment_link_at(
        &self,
        child: SceneObjectHandle,
        parent_state: &ResolvedObjectState,
        attachment: SceneStringId,
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
                    self.resolve_puppet_attachment_anchor(
                        parent_state,
                        puppet,
                        record,
                        scene_time_seconds,
                    ),
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
            for bone in self.storage.puppet_bones(puppet) {
                let parent_matrix = if bone.parent_index >= 0 {
                    matrices[local_start..]
                        .iter()
                        .find(|matrix: &&ResolvedPuppetBoneMatrix| {
                            matrix.bone_index == bone.parent_index as u32
                        })
                        .map(|matrix| matrix.matrix)
                } else {
                    None
                };
                let base_matrix = parent_matrix.unwrap_or(object.world_matrix);
                let local_matrix = sampled_puppet_bone_local_matrix(
                    self.storage,
                    object.object,
                    object.puppet_index,
                    bone,
                    scene_time_seconds,
                );
                matrices.push(ResolvedPuppetBoneMatrix {
                    puppet_index: object.puppet_index,
                    bone_index: bone.bone_index,
                    parent_index: bone.parent_index,
                    matrix: multiply_matrix(&base_matrix, &local_matrix),
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
            for binding in bindings {
                let effect_index = binding.effect.0;
                let effect = self.storage.effects().get(effect_index as usize).ok_or(
                    SceneSemanticWorldError::MissingEffectRecord {
                        object: object.object,
                        effect: binding.effect,
                    },
                )?;
                effects.push(ResolvedObjectEffectState {
                    entity: object.entity,
                    object: object.object,
                    object_index: object.object_index,
                    effect: binding.effect,
                    effect_index,
                    instance_id: binding.instance_id,
                    self_visible: binding.visible,
                    object_resolved_visible: object.resolved_visible,
                    resolved_visible: binding.visible && object.resolved_visible,
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
            let local_matrix = sampled_puppet_bone_local_matrix(
                self.storage,
                object,
                puppet_index,
                bone,
                scene_time_seconds,
            );
            let matrix = multiply_matrix(&base_matrix, &local_matrix);
            matrices.push(ResolvedPuppetBoneMatrix {
                puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
                matrix,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneSemanticWorldError {
    TooManyEntities {
        count: usize,
    },
    TooManyMeshBindings {
        count: usize,
    },
    NonCanonicalObjectHandle {
        object_index: usize,
        handle: SceneObjectHandle,
    },
    DuplicateObjectHandle {
        handle: SceneObjectHandle,
    },
    DuplicateWeId {
        we_id: u32,
    },
    ObjectEffectRangeMismatch {
        object: SceneObjectHandle,
        range_index: usize,
        effect_object: SceneObjectHandle,
    },
    ObjectEffectRangeOutOfBounds {
        object: SceneObjectHandle,
        start: u32,
        count: u32,
        len: usize,
    },
    MissingEffectRecord {
        object: SceneObjectHandle,
        effect: SceneEffectHandle,
    },
    MissingObjectForMesh {
        mesh_index: usize,
        object: SceneObjectHandle,
    },
    MissingObjectForPuppet {
        puppet_index: usize,
        object: SceneObjectHandle,
    },
    DuplicatePuppetBinding {
        object: SceneObjectHandle,
    },
    TooManyPuppetBones {
        count: usize,
    },
    MissingPuppetRecord {
        object: SceneObjectHandle,
        puppet_index: u32,
    },
    MissingObjectRecord {
        object: SceneObjectHandle,
        object_index: u32,
    },
    MissingTransform {
        object: SceneObjectHandle,
    },
    MissingVisibility {
        object: SceneObjectHandle,
    },
    MissingParentObject {
        object: SceneObjectHandle,
        parent_we_id: u32,
    },
    AttachmentWithoutParent {
        object: SceneObjectHandle,
        attachment: SceneStringId,
    },
    ParentCycle {
        object: SceneObjectHandle,
    },
}

impl fmt::Display for SceneSemanticWorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEntities { count } => {
                write!(f, "scene semantic world has too many entities: {count}")
            }
            Self::TooManyMeshBindings { count } => write!(
                f,
                "scene semantic world has too many mesh bindings: {count}"
            ),
            Self::NonCanonicalObjectHandle {
                object_index,
                handle,
            } => write!(
                f,
                "scene semantic object at index {object_index} has non-canonical handle {}",
                handle.0
            ),
            Self::DuplicateObjectHandle { handle } => {
                write!(f, "scene semantic object handle {} is duplicated", handle.0)
            }
            Self::DuplicateWeId { we_id } => {
                write!(f, "scene semantic WE object id {we_id} is duplicated")
            }
            Self::ObjectEffectRangeMismatch {
                object,
                range_index,
                effect_object,
            } => write!(
                f,
                "scene semantic object {} effect range item {range_index} points at object {}",
                object.0, effect_object.0
            ),
            Self::ObjectEffectRangeOutOfBounds {
                object,
                start,
                count,
                len,
            } => write!(
                f,
                "scene semantic object {} effect range [{start}, {start}+{count}) exceeds effect binding count {len}",
                object.0
            ),
            Self::MissingEffectRecord { object, effect } => write!(
                f,
                "scene semantic object {} references missing effect record {}",
                object.0, effect.0
            ),
            Self::MissingObjectForMesh { mesh_index, object } => write!(
                f,
                "scene semantic mesh {mesh_index} references missing object {}",
                object.0
            ),
            Self::MissingObjectForPuppet {
                puppet_index,
                object,
            } => write!(
                f,
                "scene semantic puppet {puppet_index} references missing object {}",
                object.0
            ),
            Self::DuplicatePuppetBinding { object } => write!(
                f,
                "scene semantic object {} has more than one puppet binding",
                object.0
            ),
            Self::TooManyPuppetBones { count } => write!(
                f,
                "scene semantic world has too many resolved puppet bones: {count}"
            ),
            Self::MissingPuppetRecord {
                object,
                puppet_index,
            } => write!(
                f,
                "scene semantic object {} references missing puppet record {puppet_index}",
                object.0
            ),
            Self::MissingObjectRecord {
                object,
                object_index,
            } => write!(
                f,
                "scene semantic object {} references missing object record index {object_index}",
                object.0
            ),
            Self::MissingTransform { object } => write!(
                f,
                "scene semantic object {} is missing a transform component",
                object.0
            ),
            Self::MissingVisibility { object } => write!(
                f,
                "scene semantic object {} is missing a visibility component",
                object.0
            ),
            Self::MissingParentObject {
                object,
                parent_we_id,
            } => write!(
                f,
                "scene semantic object {} references missing parent WE id {parent_we_id}",
                object.0
            ),
            Self::AttachmentWithoutParent { object, attachment } => write!(
                f,
                "scene semantic object {} has attachment string {} but no parent",
                object.0, attachment.0
            ),
            Self::ParentCycle { object } => write!(
                f,
                "scene semantic parent transform cycle includes object {}",
                object.0
            ),
        }
    }
}

impl std::error::Error for SceneSemanticWorldError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveVisitState {
    Unvisited,
    Visiting,
    Resolved,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::binary::SceneBinaryDocument;

    #[test]
    fn semantic_world_indexes_components_by_scene_object_handle() {
        let storage = SceneStorage::from_document(semantic_document()).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");

        let image = SceneObjectHandle(0);
        let entity = world.entity_for_object(image).expect("image entity");
        let transform = world.transform(image).expect("image transform");
        let visibility = world.visibility(image).expect("image visibility");
        let material = world.material_binding(image).expect("image material");
        let mesh_bindings = world.object_mesh_bindings(image);

        assert_eq!(entity.index(), 0);
        assert_eq!(world.entity_for_we_id(100), Some(entity));
        assert_eq!(transform.origin.x, 10.0);
        assert_eq!(transform.scale.x, 2.0);
        assert!(visibility.visible);
        assert_eq!(visibility.sort_order, 3);
        assert_eq!(material.material, SceneMaterialHandle(0));
        assert_eq!(mesh_bindings.len(), 2);
        assert_eq!(mesh_bindings[0].mesh_index, 0);
        assert_eq!(mesh_bindings[1].mesh_index, 2);
        assert_eq!(mesh_bindings[0].vertex_count, 4);
    }

    #[test]
    fn semantic_world_exposes_puppet_bones_and_attachments_without_abi_entities() {
        let storage = SceneStorage::from_document(semantic_document()).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
        let puppet_object = SceneObjectHandle(1);

        let parent = world.parent(puppet_object).expect("parent component");
        let puppet = world
            .puppet_binding(puppet_object)
            .expect("puppet component");
        let bones = world.puppet_bones(puppet_object).expect("puppet bones");
        let attachments = world
            .puppet_attachments(puppet_object)
            .expect("puppet attachments");
        let inputs = world.render_plan_inputs();

        assert_eq!(parent.parent_we_id, 100);
        assert_eq!(puppet.bone_count, 1);
        assert_eq!(puppet.attachment_count, 1);
        assert_eq!(bones[0].bone_index, 41);
        assert_eq!(bones[0].parent_index, -1);
        assert_eq!(attachments[0].bone_index, 41);
        assert_eq!(storage.string(attachments[0].name), Some("weapon"));
        assert_eq!(inputs.object_count, 2);
        assert_eq!(inputs.visible_object_count, 2);
        assert_eq!(inputs.mesh_binding_count, 3);
        assert_eq!(inputs.effect_binding_count, 0);
        assert_eq!(inputs.puppet_binding_count, 1);
    }

    #[test]
    fn resolve_frame_carries_object_effect_visibility_and_pass_ranges() {
        let mut document = semantic_document();
        document.effects.push(SceneEffectRecord {
            id: SceneEffectHandle(0),
            resource: SceneResourceId::NONE,
            replacement_key: SceneStringId::NONE,
            pass_start: 0,
            pass_count: 2,
            fbo_start: 0,
            fbo_count: 1,
        });
        document.effect_passes = vec![effect_pass(0), effect_pass(1)];
        document.effect_fbos.push(SceneEffectFboRecord {
            name: SceneStringId::NONE,
            format: SceneStringId::NONE,
            scale: 1.0,
        });
        document.object_effects.push(SceneObjectEffectRecord {
            object: SceneObjectHandle(0),
            effect: SceneEffectHandle(0),
            instance_id: 77,
            visible: true,
        });
        document.object_effects.push(SceneObjectEffectRecord {
            object: SceneObjectHandle(1),
            effect: SceneEffectHandle(0),
            instance_id: 78,
            visible: false,
        });
        document.objects[0].effect_start = 0;
        document.objects[0].effect_count = 1;
        document.objects[1].effect_start = 1;
        document.objects[1].effect_count = 1;

        let storage = SceneStorage::from_document(document).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
        let image_effects = world.object_effect_bindings(SceneObjectHandle(0));
        let frame = world.resolve_frame().expect("resolved frame");

        assert_eq!(image_effects.len(), 1);
        assert_eq!(image_effects[0].instance_id, 77);
        assert_eq!(frame.object_effects.len(), 2);
        assert_eq!(frame.visible_effect_instance_count, 1);
        assert_eq!(frame.visible_effect_pass_count, 2);
        assert_eq!(frame.visible_effect_fbo_count, 1);
        assert!(frame.object_effects[0].resolved_visible);
        assert_eq!(frame.object_effects[0].pass_start, 0);
        assert_eq!(frame.object_effects[0].pass_count, 2);
        assert_eq!(frame.object_effects[0].fbo_start, 0);
        assert_eq!(frame.object_effects[0].fbo_count, 1);
        assert!(!frame.object_effects[1].resolved_visible);
    }

    #[test]
    fn resolve_frame_outputs_parent_attachment_world_transform() {
        let storage = SceneStorage::from_document(attachment_document()).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
        let frame = world.resolve_frame().expect("resolved frame");
        let child = frame.object(SceneObjectHandle(1)).expect("child state");
        let link = frame.attachment_links[0];

        assert_eq!(frame.visible_object_count, 2);
        assert_eq!(frame.visible_mesh_binding_count, 2);
        assert_eq!(frame.visible_effect_instance_count, 0);
        assert_eq!(frame.visible_puppet_binding_count, 1);
        assert_eq!(frame.puppet_bone_palettes.len(), 1);
        assert_eq!(frame.puppet_bone_matrices.len(), 1);
        assert_eq!(frame.visible_puppet_bone_matrix_count, 1);
        assert_eq!(child.parent, SceneObjectHandle(0));
        assert!(child.resolved_visible);
        assert_eq!(child.mesh_binding_start, 1);
        assert_eq!(child.mesh_binding_count, 1);
        assert_eq!(link.parent_puppet_index, 0);
        assert_eq!(link.bone_index, 41);
        assert!(link.resolved);
        assert_eq!(frame.puppet_bone_palettes[0].object, SceneObjectHandle(0));
        assert_eq!(frame.puppet_bone_palettes[0].bone_count, 1);
        assert_eq!(frame.puppet_bone_matrices[0].bone_index, 41);
        assert_close(frame.puppet_bone_matrices[0].matrix[12], 10.0);
        assert_close(frame.puppet_bone_matrices[0].matrix[13], 20.0);
        assert_close(child.world_matrix[12], 17.0);
        assert_close(child.world_matrix[13], 30.0);
    }

    #[test]
    fn resolve_frame_samples_puppet_animation_for_skinning_and_attachment() {
        let mut document = attachment_document();
        document
            .strings
            .extend(["blink".to_owned(), "loop".to_owned()]);
        document
            .object_animation_layers
            .push(SceneObjectAnimationLayerRecord {
                object: SceneObjectHandle(0),
                animation_id: 475,
                layer_index: 0,
                additive: false,
                autosort: false,
            });
        document
            .puppet_animation_transform_samples
            .push(animation_sample([1.0, 2.0, 0.0]));
        document
            .puppet_animation_transform_samples
            .push(animation_sample([30.0, 40.0, 0.0]));
        document
            .puppet_animation_tracks
            .push(ScenePuppetAnimationTrackRecord {
                clip: 0,
                bone_index: 41,
                track_flags: 0,
                sample_start: 0,
                sample_count: 2,
            });
        document
            .puppet_animation_clips
            .push(ScenePuppetAnimationClipRecord {
                puppet: 0,
                clip_id: 475,
                flags: 0,
                name: SceneStringId(2),
                playback: SceneStringId(3),
                fps: 30.0,
                frame_count: 1,
                frame_metadata: 0,
                track_start: 0,
                track_count: 1,
            });
        let storage = SceneStorage::from_document(document).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");

        let frame = world
            .resolve_frame_at(1.0 / 30.0)
            .expect("resolved animated frame");
        let child = frame.object(SceneObjectHandle(1)).expect("child state");

        assert_close(frame.puppet_bone_matrices[0].matrix[12], 40.0);
        assert_close(frame.puppet_bone_matrices[0].matrix[13], 60.0);
        assert_close(child.world_matrix[12], 47.0);
        assert_close(child.world_matrix[13], 70.0);
    }

    #[test]
    fn resolve_frame_rejects_parent_cycles_before_render_planning() {
        let storage = SceneStorage::from_document(parent_cycle_document()).expect("storage");
        let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
        let err = world.resolve_frame().expect_err("parent cycle");

        assert!(matches!(
            err,
            SceneSemanticWorldError::ParentCycle {
                object: SceneObjectHandle(0) | SceneObjectHandle(1)
            }
        ));
    }

    fn semantic_document() -> SceneBinaryDocument {
        let strings = vec!["root-bone".to_owned(), "weapon".to_owned()];
        SceneBinaryDocument {
            strings,
            objects: vec![image_object(), puppet_object()],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 0,
            }],
            meshes: vec![image_mesh(), puppet_mesh(), image_mesh_extra()],
            mesh_vertices: vec![
                SceneMeshVertexRecord {
                    position: SceneVec3::default(),
                    uv: [0.0, 0.0],
                };
                12
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
            puppets: vec![ScenePuppetRecord {
                object: SceneObjectHandle(1),
                resource: SceneResourceId::NONE,
                mesh_start: 1,
                mesh_count: 1,
                bone_start: 0,
                bone_count: 1,
                attachment_start: 0,
                attachment_count: 1,
            }],
            puppet_bones: vec![ScenePuppetBoneRecord {
                puppet: 0,
                bone_index: 41,
                flags: 0,
                parent_index: -1,
                local_matrix: identity_matrix(),
                info: SceneStringId(0),
            }],
            puppet_attachments: vec![ScenePuppetAttachmentRecord {
                puppet: 0,
                bone_index: 41,
                name: SceneStringId(1),
                local_matrix: identity_matrix(),
            }],
            ..SceneBinaryDocument::default()
        }
    }

    fn attachment_document() -> SceneBinaryDocument {
        SceneBinaryDocument {
            strings: vec!["root-bone".to_owned(), "eye".to_owned()],
            objects: vec![parent_puppet_object(), attached_child_object()],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 0,
            }],
            meshes: vec![parent_puppet_mesh(), attached_child_mesh()],
            mesh_vertices: vec![
                SceneMeshVertexRecord {
                    position: SceneVec3::default(),
                    uv: [0.0, 0.0],
                };
                8
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
            puppets: vec![ScenePuppetRecord {
                object: SceneObjectHandle(0),
                resource: SceneResourceId::NONE,
                mesh_start: 0,
                mesh_count: 1,
                bone_start: 0,
                bone_count: 1,
                attachment_start: 0,
                attachment_count: 1,
            }],
            puppet_bones: vec![ScenePuppetBoneRecord {
                puppet: 0,
                bone_index: 41,
                flags: 0,
                parent_index: -1,
                local_matrix: identity_matrix(),
                info: SceneStringId(0),
            }],
            puppet_attachments: vec![ScenePuppetAttachmentRecord {
                puppet: 0,
                bone_index: 41,
                name: SceneStringId(1),
                local_matrix: translated_attachment_matrix(2.0, 3.0),
            }],
            ..SceneBinaryDocument::default()
        }
    }

    fn parent_cycle_document() -> SceneBinaryDocument {
        SceneBinaryDocument {
            objects: vec![
                cycle_object(SceneObjectHandle(0), 100, 200),
                cycle_object(SceneObjectHandle(1), 200, 100),
            ],
            ..SceneBinaryDocument::default()
        }
    }

    fn image_object() -> SceneObjectRecord {
        SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 100,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: 10.0,
                y: 20.0,
                z: 0.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 2.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 3,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }
    }

    fn puppet_object() -> SceneObjectRecord {
        SceneObjectRecord {
            id: SceneObjectHandle(1),
            we_id: 200,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Puppet,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: 100,
            attachment: SceneStringId(1),
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 4,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }
    }

    fn parent_puppet_object() -> SceneObjectRecord {
        SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 937,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Puppet,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: 10.0,
                y: 20.0,
                z: 0.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 1,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }
    }

    fn attached_child_object() -> SceneObjectRecord {
        SceneObjectRecord {
            id: SceneObjectHandle(1),
            we_id: 1336,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: 937,
            attachment: SceneStringId(1),
            origin: SceneVec3 {
                x: 5.0,
                y: 7.0,
                z: 0.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 2,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }
    }

    fn cycle_object(handle: SceneObjectHandle, we_id: u32, parent_we_id: u32) -> SceneObjectRecord {
        SceneObjectRecord {
            id: handle,
            we_id,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        }
    }

    fn image_mesh() -> SceneMeshRecord {
        SceneMeshRecord {
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            width: 64.0,
            height: 32.0,
            bounds_min: SceneVec3 {
                x: -32.0,
                y: -16.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 32.0,
                y: 16.0,
                z: 0.0,
            },
        }
    }

    fn puppet_mesh() -> SceneMeshRecord {
        SceneMeshRecord {
            object: SceneObjectHandle(1),
            material: SceneMaterialHandle(0),
            vertex_start: 4,
            vertex_count: 4,
            index_start: 6,
            index_count: 6,
            width: 128.0,
            height: 96.0,
            bounds_min: SceneVec3 {
                x: -64.0,
                y: -48.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 64.0,
                y: 48.0,
                z: 0.0,
            },
        }
    }

    fn image_mesh_extra() -> SceneMeshRecord {
        SceneMeshRecord {
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
            vertex_start: 8,
            vertex_count: 4,
            index_start: 12,
            index_count: 6,
            width: 16.0,
            height: 16.0,
            bounds_min: SceneVec3 {
                x: -8.0,
                y: -8.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 8.0,
                y: 8.0,
                z: 0.0,
            },
        }
    }

    fn parent_puppet_mesh() -> SceneMeshRecord {
        SceneMeshRecord {
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            width: 64.0,
            height: 64.0,
            bounds_min: SceneVec3 {
                x: -32.0,
                y: -32.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 32.0,
                y: 32.0,
                z: 0.0,
            },
        }
    }

    fn attached_child_mesh() -> SceneMeshRecord {
        SceneMeshRecord {
            object: SceneObjectHandle(1),
            material: SceneMaterialHandle(0),
            vertex_start: 4,
            vertex_count: 4,
            index_start: 6,
            index_count: 6,
            width: 16.0,
            height: 16.0,
            bounds_min: SceneVec3 {
                x: -8.0,
                y: -8.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 8.0,
                y: 8.0,
                z: 0.0,
            },
        }
    }

    fn effect_pass(pass_index: u32) -> SceneEffectPassRecord {
        SceneEffectPassRecord {
            effect: SceneEffectHandle(0),
            pass_index,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            command: SceneStringId::NONE,
            source: SceneStringId::NONE,
            target: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            combo_start: 0,
            combo_count: 0,
        }
    }

    fn identity_matrix() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]
    }

    fn translated_attachment_matrix(x: f32, y: f32) -> [f32; 16] {
        let mut matrix = identity_matrix();
        matrix[12] = x;
        matrix[13] = y;
        matrix
    }

    fn animation_sample(translation: [f32; 3]) -> ScenePuppetAnimationTransformSampleRecord {
        ScenePuppetAnimationTransformSampleRecord {
            translation: SceneVec3 {
                x: translation[0],
                y: translation[1],
                z: translation[2],
            },
            rotation: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be close to {expected}"
        );
    }
}
