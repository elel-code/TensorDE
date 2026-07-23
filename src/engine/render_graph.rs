//! Backend-independent render graph contracts.
//!
//! This module is intentionally split by graph concern so the engine layer can
//! grow toward a Godot-style RenderingDeviceGraph boundary without continuing
//! to concentrate WE semantics inside the Vulkan draw-pass code.

pub mod allocation;
pub mod binding;
pub mod execution;
pub mod graph;
pub mod pass;
pub mod resource;
pub mod run;
pub mod state;
pub mod target;
pub mod we_image;

pub use allocation::{RenderGraphTargetAllocation, RenderGraphTargetAllocationPlan};
pub use binding::TextureBindingRole;
pub use execution::{
    RenderGraphExecutionLevel, RenderGraphExecutionPass, RenderGraphExecutionPlan,
};
pub use graph::{RenderGraph, RenderGraphActivationPolicy, UnsupportedGraphBoundary};
pub use pass::{
    RenderPassEffectVisibility, RenderPassEffectVisibilityPolicy, RenderPassNode, RenderPassRole,
};
pub use resource::{
    RenderGraphBarrier, RenderGraphResourceAccess, RenderGraphResourceUsage, RenderGraphResourceUse,
};
pub use run::{
    RenderGraphRunPlan, RenderGraphRunPlanCandidate, RenderGraphTargetRun, RenderGraphTargetRunPass,
};
pub use state::{
    ColorWriteMask, CullMode, DepthTestMode, PassState, PipelineBlendMode, ShaderBlendMode,
};
pub use target::{RenderTargetRole, RenderTargetSpec};
pub use we_image::{
    WeEffectPassContract, WeFinalEffectMaterial, WeFinalEffectPrepass, WeFoliageRippleMaterial,
    WeFramebufferSnapshotContract, WeFramebufferSnapshotUsage, WeImageGraphContract,
    WeRippleFlowMaterialIndices, WeWaterWavesDirectMaterial, we_effect_pass_node,
    we_effect_passes_form_foliage_ripple_chain, we_effect_passes_form_ripple_flow_chain,
    we_effect_passes_form_waterwaves_displacement_chain, we_image_graph,
    we_image_graph_requires_generated_scene_snapshot,
};
