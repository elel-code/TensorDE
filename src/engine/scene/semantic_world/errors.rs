use std::fmt;

use super::*;

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
    NonInvertiblePuppetBindMatrix {
        object: SceneObjectHandle,
        bone_index: u32,
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
    MissingVisual {
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
    ScriptRuntime(String),
    UserProperty(String),
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
            Self::NonInvertiblePuppetBindMatrix { object, bone_index } => write!(
                f,
                "scene semantic puppet object {} bone {bone_index} has a non-invertible bind matrix",
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
            Self::MissingVisual { object } => write!(
                f,
                "scene semantic object {} is missing a visual component",
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
            Self::ScriptRuntime(message) => write!(f, "scene script runtime failed: {message}"),
            Self::UserProperty(message) => write!(f, "scene user property failed: {message}"),
        }
    }
}

impl std::error::Error for SceneSemanticWorldError {}
