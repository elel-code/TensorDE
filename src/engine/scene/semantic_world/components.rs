//! Semantic ECS-like components for scene runtime state.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`

use super::super::abi::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformComponent {
    pub origin: SceneVec3,
    pub angles: SceneVec3,
    pub scale: SceneVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentComponent {
    pub parent_we_id: u32,
    pub attachment: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityComponent {
    pub visible: bool,
    pub color_blend_mode: i32,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialBindingComponent {
    pub material: SceneMaterialHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshBindingComponent {
    pub binding_start: u32,
    pub binding_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMeshBinding {
    pub mesh_index: u32,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
}

impl SemanticMeshBinding {
    pub fn from_mesh(mesh_index: u32, mesh: &SceneMeshRecord) -> Self {
        Self {
            mesh_index,
            object: mesh.object,
            material: mesh.material,
            vertex_start: mesh.vertex_start,
            vertex_count: mesh.vertex_count,
            index_start: mesh.index_start,
            index_count: mesh.index_count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PuppetBindingComponent {
    pub puppet_index: u32,
    pub resource: SceneResourceId,
    pub mesh_start: u32,
    pub mesh_count: u32,
    pub bone_start: u32,
    pub bone_count: u32,
    pub attachment_start: u32,
    pub attachment_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticRenderPlanInputs {
    pub object_count: usize,
    pub visible_object_count: usize,
    pub mesh_binding_count: usize,
    pub effect_binding_count: usize,
    pub puppet_binding_count: usize,
}

pub fn transform_from_object(object: &SceneObjectRecord) -> TransformComponent {
    TransformComponent {
        origin: object.origin,
        angles: object.angles,
        scale: object.scale,
    }
}

pub fn parent_from_object(object: &SceneObjectRecord) -> Option<ParentComponent> {
    if object.parent_we_id == INVALID_OBJECT_ID && !object.attachment.is_some() {
        return None;
    }
    Some(ParentComponent {
        parent_we_id: object.parent_we_id,
        attachment: object.attachment,
    })
}

pub fn visibility_from_object(object: &SceneObjectRecord) -> VisibilityComponent {
    VisibilityComponent {
        visible: object.visible,
        color_blend_mode: object.color_blend_mode,
        sort_order: object.sort_order,
    }
}

pub fn material_binding_from_object(
    object: &SceneObjectRecord,
) -> Option<MaterialBindingComponent> {
    if object.material.0 == INVALID_MATERIAL_ID {
        return None;
    }
    Some(MaterialBindingComponent {
        material: object.material,
    })
}

pub fn puppet_binding_from_record(
    puppet_index: u32,
    puppet: &ScenePuppetRecord,
) -> PuppetBindingComponent {
    PuppetBindingComponent {
        puppet_index,
        resource: puppet.resource,
        mesh_start: puppet.mesh_start,
        mesh_count: puppet.mesh_count,
        bone_start: puppet.bone_start,
        bone_count: puppet.bone_count,
        attachment_start: puppet.attachment_start,
        attachment_count: puppet.attachment_count,
    }
}
