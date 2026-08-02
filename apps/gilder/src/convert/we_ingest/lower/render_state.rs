//! Typed render-graph state lowering into the current scene binary ABI.

use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    ColorWriteMask, CullMode, DepthTestMode, PipelineBlendMode, RenderGraphActivationPolicy,
    RenderPassDrawPrimitive, RenderPassEffectVisibilityPolicy, RenderPassRole, RenderTargetRole,
};
use crate::engine::scene::{
    SceneColorWriteMask, SceneCompositeBlend, SceneCullMode, SceneDepthTest, ScenePipelineBlend,
    SceneRenderEffectVisibilityPolicy, SceneRenderGraphActivationPolicy,
    SceneRenderPassDrawPrimitive, SceneRenderPassKind, SceneRenderTargetKind,
};

pub(super) fn lower_render_graph_activation_policy(
    policy: RenderGraphActivationPolicy,
) -> SceneRenderGraphActivationPolicy {
    match policy {
        RenderGraphActivationPolicy::Always => SceneRenderGraphActivationPolicy::Always,
        RenderGraphActivationPolicy::AnyEffectVisible => {
            SceneRenderGraphActivationPolicy::AnyEffectVisible
        }
    }
}

pub(super) fn lower_pass_role(role: RenderPassRole) -> SceneRenderPassKind {
    match role {
        RenderPassRole::Clear => SceneRenderPassKind::Clear,
        RenderPassRole::BaseMaterial => SceneRenderPassKind::BaseMaterial,
        RenderPassRole::ObjectLocalSource => SceneRenderPassKind::ObjectLocalSource,
        RenderPassRole::EffectMaterial => SceneRenderPassKind::EffectMaterial,
        RenderPassRole::ColorBlendPassthrough => SceneRenderPassKind::ColorBlendPassthrough,
        RenderPassRole::CopyTarget => SceneRenderPassKind::CopyTarget,
        RenderPassRole::SwapTargetReferences => SceneRenderPassKind::SwapTargetReferences,
        RenderPassRole::VideoSample => SceneRenderPassKind::VideoSample,
        RenderPassRole::Particle => SceneRenderPassKind::Particle,
        RenderPassRole::TextPath => SceneRenderPassKind::TextPath,
        RenderPassRole::SceneComposite => SceneRenderPassKind::SceneComposite,
        RenderPassRole::MeshVisiblePrefix => SceneRenderPassKind::MeshVisiblePrefix,
        RenderPassRole::MeshClippingMask => SceneRenderPassKind::MeshClippingMask,
        RenderPassRole::MeshClippedTarget => SceneRenderPassKind::MeshClippedTarget,
        RenderPassRole::MeshVisibleRemainder => SceneRenderPassKind::MeshVisibleRemainder,
        RenderPassRole::DebugEvidence => SceneRenderPassKind::DebugEvidence,
        RenderPassRole::Unsupported => SceneRenderPassKind::Unsupported,
    }
}

pub(super) fn lower_pass_draw_primitive(
    primitive: RenderPassDrawPrimitive,
) -> SceneRenderPassDrawPrimitive {
    match primitive {
        RenderPassDrawPrimitive::None => SceneRenderPassDrawPrimitive::None,
        RenderPassDrawPrimitive::ObjectMesh => SceneRenderPassDrawPrimitive::ObjectMesh,
        RenderPassDrawPrimitive::ObjectCompositeMesh => {
            SceneRenderPassDrawPrimitive::ObjectCompositeMesh
        }
        RenderPassDrawPrimitive::FullscreenTriangle => {
            SceneRenderPassDrawPrimitive::FullscreenTriangle
        }
        RenderPassDrawPrimitive::ObjectUvSupportQuad => {
            SceneRenderPassDrawPrimitive::ObjectUvSupportQuad
        }
        RenderPassDrawPrimitive::ParticleBillboard => {
            SceneRenderPassDrawPrimitive::ParticleBillboard
        }
    }
}

pub(super) fn lower_effect_visibility_policy(
    policy: RenderPassEffectVisibilityPolicy,
) -> SceneRenderEffectVisibilityPolicy {
    match policy {
        RenderPassEffectVisibilityPolicy::None => SceneRenderEffectVisibilityPolicy::None,
        RenderPassEffectVisibilityPolicy::Passthrough => {
            SceneRenderEffectVisibilityPolicy::Passthrough
        }
        RenderPassEffectVisibilityPolicy::WaterWavesStages => {
            SceneRenderEffectVisibilityPolicy::WaterWavesStages
        }
        RenderPassEffectVisibilityPolicy::FlatRoundedMask => {
            SceneRenderEffectVisibilityPolicy::FlatRoundedMask
        }
        RenderPassEffectVisibilityPolicy::MaterialStages => {
            SceneRenderEffectVisibilityPolicy::MaterialStages
        }
    }
}

pub(super) fn lower_render_target(target: RenderTargetRole) -> SceneRenderTargetKind {
    match target {
        RenderTargetRole::SceneColor => SceneRenderTargetKind::SceneColor,
        RenderTargetRole::Swapchain => SceneRenderTargetKind::Swapchain,
        RenderTargetRole::ImageLocalMain => SceneRenderTargetKind::ImageLocalMain,
        RenderTargetRole::ImageLocalSub => SceneRenderTargetKind::ImageLocalSub,
        RenderTargetRole::NamedFbo => SceneRenderTargetKind::NamedFbo,
        RenderTargetRole::FirstClassEffectTarget => SceneRenderTargetKind::FirstClassEffectTarget,
        RenderTargetRole::VideoExternalImage => SceneRenderTargetKind::VideoExternalImage,
        RenderTargetRole::Temporary => SceneRenderTargetKind::Temporary,
    }
}

pub(super) fn lower_pipeline_blend(blend: PipelineBlendMode) -> ScenePipelineBlend {
    match blend {
        PipelineBlendMode::Normal => ScenePipelineBlend::Normal,
        PipelineBlendMode::Translucent => ScenePipelineBlend::Translucent,
        PipelineBlendMode::Additive => ScenePipelineBlend::Additive,
        PipelineBlendMode::Disabled => ScenePipelineBlend::Disabled,
        PipelineBlendMode::AlphaToCoverage => ScenePipelineBlend::AlphaToCoverage,
    }
}

pub(super) fn lower_scene_blend(blend: SceneBlendMode) -> SceneCompositeBlend {
    match blend {
        SceneBlendMode::Alpha => SceneCompositeBlend::Alpha,
        SceneBlendMode::Normal => SceneCompositeBlend::Normal,
        SceneBlendMode::Additive => SceneCompositeBlend::Additive,
        SceneBlendMode::Multiply => SceneCompositeBlend::Multiply,
        SceneBlendMode::Screen => SceneCompositeBlend::Screen,
        SceneBlendMode::Max => SceneCompositeBlend::Max,
        SceneBlendMode::Modulate => SceneCompositeBlend::Modulate,
        SceneBlendMode::HslColor => SceneCompositeBlend::HslColor,
        SceneBlendMode::AlphaToCoverage => SceneCompositeBlend::AlphaToCoverage,
    }
}

pub(super) fn lower_depth_test(depth: DepthTestMode) -> SceneDepthTest {
    match depth {
        DepthTestMode::Disabled => SceneDepthTest::Disabled,
        DepthTestMode::Less
        | DepthTestMode::LessEqual
        | DepthTestMode::Equal
        | DepthTestMode::NotEqual
        | DepthTestMode::Greater
        | DepthTestMode::Never => SceneDepthTest::Enabled,
    }
}

pub(super) fn lower_cull_mode(cull: CullMode) -> SceneCullMode {
    match cull {
        CullMode::None => SceneCullMode::None,
        CullMode::Front | CullMode::Back => SceneCullMode::Normal,
    }
}

pub(super) fn lower_color_write_mask(mask: ColorWriteMask) -> SceneColorWriteMask {
    match mask {
        ColorWriteMask::Rgb => SceneColorWriteMask::Rgb,
        ColorWriteMask::Rgba => SceneColorWriteMask::Rgba,
    }
}
