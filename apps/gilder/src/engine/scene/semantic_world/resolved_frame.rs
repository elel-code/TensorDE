//! Packed resolved semantic frame produced before RenderingServer planning.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/scene-format.md`
//! - `reverse-engineered/gilder/docs/exe/scene-and-object.md`
//! - `reverse-engineered/gilder/docs/exe/model-and-animation.md`

use super::super::abi::*;
use super::super::event::{SceneMediaClockState, SceneVideoState};
use super::components::MeshBindingComponent;
use super::effect::ResolvedObjectEffectState;
use super::entity::SemanticEntity;

pub const INVALID_RESOLVED_INDEX: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSemanticFrame {
    pub objects: Vec<ResolvedObjectState>,
    pub object_effects: Vec<ResolvedObjectEffectState>,
    pub attachment_links: Vec<ResolvedAttachmentLink>,
    pub puppet_bone_palettes: Vec<ResolvedPuppetBonePalette>,
    pub puppet_bone_matrices: Vec<ResolvedPuppetBoneMatrix>,
    pub audio_band_material_values: Vec<ResolvedAudioBandMaterialValue>,
    pub material_scalar_values: Vec<ResolvedMaterialScalarValue>,
    pub script_text_values: Vec<ResolvedScriptTextValue>,
    pub media_clock: Option<SceneMediaClockState>,
    pub video_frame: Option<SceneVideoState>,
    pub visible_object_count: usize,
    pub visible_mesh_binding_count: usize,
    pub visible_effect_instance_count: usize,
    pub visible_effect_pass_count: usize,
    pub visible_effect_fbo_count: usize,
    pub visible_puppet_binding_count: usize,
    pub visible_puppet_bone_matrix_count: usize,
}

impl ResolvedSemanticFrame {
    pub fn from_resolved_parts(
        objects: Vec<ResolvedObjectState>,
        object_effects: Vec<ResolvedObjectEffectState>,
        attachment_links: Vec<ResolvedAttachmentLink>,
        puppet_bone_palettes: Vec<ResolvedPuppetBonePalette>,
        puppet_bone_matrices: Vec<ResolvedPuppetBoneMatrix>,
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
        let visible_effect_instance_count = object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .count();
        let visible_effect_pass_count = object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .map(|effect| effect.pass_count as usize)
            .sum();
        let visible_effect_fbo_count = object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .map(|effect| effect.fbo_count as usize)
            .sum();
        let visible_puppet_binding_count = objects
            .iter()
            .filter(|object| {
                object.resolved_visible && object.puppet_index != INVALID_RESOLVED_INDEX
            })
            .count();
        let visible_puppet_bone_matrix_count = puppet_bone_palettes
            .iter()
            .filter(|palette| palette.resolved_visible)
            .map(|palette| palette.bone_count as usize)
            .sum();
        Self {
            objects,
            object_effects,
            attachment_links,
            puppet_bone_palettes,
            puppet_bone_matrices,
            audio_band_material_values: Vec::new(),
            material_scalar_values: Vec::new(),
            script_text_values: Vec::new(),
            media_clock: None,
            video_frame: None,
            visible_object_count,
            visible_mesh_binding_count,
            visible_effect_instance_count,
            visible_effect_pass_count,
            visible_effect_fbo_count,
            visible_puppet_binding_count,
            visible_puppet_bone_matrix_count,
        }
    }

    pub fn object(&self, object: SceneObjectHandle) -> Option<&ResolvedObjectState> {
        self.objects
            .get(object.0 as usize)
            .filter(|state| state.object == object)
    }

    pub fn object_effect(&self, binding_index: u32) -> Option<&ResolvedObjectEffectState> {
        self.object_effects
            .get(binding_index as usize)
            .filter(|effect| effect.binding_index == binding_index)
    }

    pub fn refresh_visibility_counts(&mut self) {
        self.visible_object_count = self
            .objects
            .iter()
            .filter(|object| object.resolved_visible)
            .count();
        self.visible_mesh_binding_count = self
            .objects
            .iter()
            .filter(|object| object.resolved_visible)
            .map(|object| object.mesh_binding_count as usize)
            .sum();
        self.visible_effect_instance_count = self
            .object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .count();
        self.visible_effect_pass_count = self
            .object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .map(|effect| effect.pass_count as usize)
            .sum();
        self.visible_effect_fbo_count = self
            .object_effects
            .iter()
            .filter(|effect| effect.resolved_visible)
            .map(|effect| effect.fbo_count as usize)
            .sum();
        self.visible_puppet_binding_count = self
            .objects
            .iter()
            .filter(|object| {
                object.resolved_visible && object.puppet_index != INVALID_RESOLVED_INDEX
            })
            .count();
        self.visible_puppet_bone_matrix_count = self
            .puppet_bone_palettes
            .iter()
            .filter(|palette| palette.resolved_visible)
            .map(|palette| palette.bone_count as usize)
            .sum();
    }

    pub fn audio_material_value(
        &self,
        object: SceneObjectHandle,
        target: SceneAudioBandMaterialTarget,
    ) -> Option<f32> {
        self.audio_band_material_values
            .iter()
            .find(|value| value.object == object && value.target == target)
            .map(|value| value.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedMaterialScalarValue {
    pub object: SceneObjectHandle,
    pub constant_index: u32,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScriptTextValue {
    pub object: SceneObjectHandle,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedAudioBandMaterialValue {
    pub object: SceneObjectHandle,
    pub target: SceneAudioBandMaterialTarget,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPuppetBonePalette {
    pub object: SceneObjectHandle,
    pub puppet_index: u32,
    pub bone_start: u32,
    pub bone_count: u32,
    pub resolved_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPuppetBoneMatrix {
    pub puppet_index: u32,
    pub bone_index: u32,
    pub parent_index: i32,
    pub matrix: [f32; 16],
    pub alpha: f32,
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
    pub render_world_matrix: [f32; 16],
    pub self_visible: bool,
    pub resolved_visible: bool,
    pub self_color: SceneVec3,
    pub resolved_color: SceneVec3,
    pub self_alpha: f32,
    pub resolved_alpha: f32,
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
