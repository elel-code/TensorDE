//! Packed resolved semantic frame produced before RenderingServer planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`

use super::super::abi::*;
use super::components::MeshBindingComponent;
use super::entity::SemanticEntity;

pub const INVALID_RESOLVED_INDEX: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSemanticFrame {
    pub objects: Vec<ResolvedObjectState>,
    pub attachment_links: Vec<ResolvedAttachmentLink>,
    pub visible_object_count: usize,
    pub visible_mesh_binding_count: usize,
    pub visible_puppet_binding_count: usize,
}

impl ResolvedSemanticFrame {
    pub fn from_objects(
        objects: Vec<ResolvedObjectState>,
        attachment_links: Vec<ResolvedAttachmentLink>,
    ) -> Self {
        let visible_object_count = objects
            .iter()
            .filter(|object| object.resolved_visible)
            .count();
        let visible_mesh_binding_count = objects
            .iter()
            .filter(|object| object.resolved_visible)
            .map(|object| object.mesh_binding_count as usize)
            .sum();
        let visible_puppet_binding_count = objects
            .iter()
            .filter(|object| {
                object.resolved_visible && object.puppet_index != INVALID_RESOLVED_INDEX
            })
            .count();
        Self {
            objects,
            attachment_links,
            visible_object_count,
            visible_mesh_binding_count,
            visible_puppet_binding_count,
        }
    }

    pub fn object(&self, object: SceneObjectHandle) -> Option<&ResolvedObjectState> {
        self.objects.iter().find(|state| state.object == object)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedObjectState {
    pub entity: SemanticEntity,
    pub object: SceneObjectHandle,
    pub object_index: u32,
    pub parent: SceneObjectHandle,
    pub parent_we_id: u32,
    pub attachment: SceneStringId,
    pub local_matrix: [f32; 16],
    pub world_matrix: [f32; 16],
    pub self_visible: bool,
    pub resolved_visible: bool,
    pub sort_order: i32,
    pub mesh_binding_start: u32,
    pub mesh_binding_count: u32,
    pub puppet_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAttachmentLink {
    pub child: SceneObjectHandle,
    pub parent: SceneObjectHandle,
    pub attachment: SceneStringId,
    pub parent_puppet_index: u32,
    pub bone_index: u32,
    pub resolved: bool,
}

impl ResolvedAttachmentLink {
    pub fn unresolved(
        child: SceneObjectHandle,
        parent: SceneObjectHandle,
        attachment: SceneStringId,
    ) -> Self {
        Self {
            child,
            parent,
            attachment,
            parent_puppet_index: INVALID_RESOLVED_INDEX,
            bone_index: INVALID_RESOLVED_INDEX,
            resolved: false,
        }
    }

    pub fn with_parent_puppet(
        child: SceneObjectHandle,
        parent: SceneObjectHandle,
        attachment: SceneStringId,
        parent_puppet_index: u32,
    ) -> Self {
        Self {
            parent_puppet_index,
            ..Self::unresolved(child, parent, attachment)
        }
    }

    pub fn resolved(
        child: SceneObjectHandle,
        parent: SceneObjectHandle,
        attachment: SceneStringId,
        parent_puppet_index: u32,
        bone_index: u32,
    ) -> Self {
        Self {
            child,
            parent,
            attachment,
            parent_puppet_index,
            bone_index,
            resolved: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResolvedObjectMeshRange {
    pub binding_start: u32,
    pub binding_count: u32,
}

impl ResolvedObjectMeshRange {
    pub fn from_component(component: &MeshBindingComponent) -> Self {
        Self {
            binding_start: component.binding_start,
            binding_count: component.binding_count,
        }
    }
}
