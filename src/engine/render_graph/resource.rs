use serde::{Deserialize, Serialize};

use super::binding::TextureBindingRole;
use super::target::RenderTargetRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderGraphResourceAccess {
    Read,
    Write,
    ReadWrite,
}

impl RenderGraphResourceAccess {
    pub(super) fn conflicts_after(self, previous: Self) -> bool {
        !matches!((previous, self), (Self::Read, Self::Read))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderGraphResourceUsage {
    TextureSample,
    UniformRead,
    StorageBufferRead,
    StorageBufferReadWrite,
    AttachmentColorReadWrite,
    ExternalVideoSample,
    Present,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphResourceUse {
    pub pass_id: u32,
    pub resource_key: String,
    pub access: RenderGraphResourceAccess,
    pub usage: RenderGraphResourceUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphBarrier {
    pub resource_key: String,
    pub before_pass_id: u32,
    pub after_pass_id: u32,
    pub previous_access: RenderGraphResourceAccess,
    pub next_access: RenderGraphResourceAccess,
    pub previous_usage: RenderGraphResourceUsage,
    pub next_usage: RenderGraphResourceUsage,
}

pub(super) fn render_target_resource_key(role: RenderTargetRole, name: Option<&str>) -> String {
    match (role, name) {
        (RenderTargetRole::NamedFbo, Some(name)) => format!("target:named-fbo:{name}"),
        (RenderTargetRole::FirstClassEffectTarget, Some(name)) => {
            format!("target:first-class-effect:{name}")
        }
        (role, Some(name)) => format!("target:{role:?}:{name}"),
        (role, None) => format!("target:{role:?}"),
    }
}

pub(super) fn texture_binding_resource_key(
    object_index: Option<usize>,
    binding: &TextureBindingRole,
) -> String {
    match binding {
        TextureBindingRole::SourceTexture => {
            format!("texture:source:{}", object_index.unwrap_or(usize::MAX))
        }
        TextureBindingRole::TextureSlot { slot } => {
            format!("texture-slot:{}:{slot}", object_index.unwrap_or(usize::MAX))
        }
        TextureBindingRole::AlphaTextureSlot { slot } => {
            format!(
                "alpha-texture-slot:{}:{slot}",
                object_index.unwrap_or(usize::MAX)
            )
        }
        TextureBindingRole::PreviousGraphTarget => "target:previous-graph-target".to_owned(),
        TextureBindingRole::GraphTarget { role, name } => {
            render_target_resource_key(*role, name.as_deref())
        }
        TextureBindingRole::NamedFboBind { name } => format!("target:named-fbo:{name}"),
        TextureBindingRole::EffectTarget { name } => format!("target:first-class-effect:{name}"),
        TextureBindingRole::VideoFrame { media_instance } => {
            format!("external-video-frame:{media_instance}")
        }
        TextureBindingRole::AudioUniform => "uniform:audio".to_owned(),
        TextureBindingRole::SystemUniform => "uniform:system".to_owned(),
        TextureBindingRole::PassConstant { name } => format!("uniform:pass-constant:{name}"),
    }
}

pub(super) fn texture_binding_resource_usage(
    binding: &TextureBindingRole,
) -> RenderGraphResourceUsage {
    match binding {
        TextureBindingRole::AudioUniform
        | TextureBindingRole::SystemUniform
        | TextureBindingRole::PassConstant { .. } => RenderGraphResourceUsage::UniformRead,
        TextureBindingRole::VideoFrame { .. } => RenderGraphResourceUsage::ExternalVideoSample,
        _ => RenderGraphResourceUsage::TextureSample,
    }
}
