//! Backend-independent render graph contracts.
//!
//! This module is intentionally split by graph concern so the engine layer can
//! grow toward a Godot-style RenderingDeviceGraph boundary without continuing
//! to concentrate WE semantics inside the Vulkan draw-pass code.

pub mod allocation;
pub mod binding;
pub mod graph;
pub mod pass;
pub mod resource;
pub mod state;
pub mod target;
pub mod we_image;

pub use allocation::{RenderGraphTargetAllocation, RenderGraphTargetAllocationPlan};
pub use binding::TextureBindingRole;
pub use graph::{RenderGraph, UnsupportedGraphBoundary};
pub use pass::{RenderPassNode, RenderPassRole};
pub use resource::{
    RenderGraphBarrier, RenderGraphResourceAccess, RenderGraphResourceUsage, RenderGraphResourceUse,
};
pub use state::{CullMode, DepthTestMode, PassState, PipelineBlendMode, ShaderBlendMode};
pub use target::RenderTargetRole;
pub use we_image::{
    WeEffectPassContract, WeImageGraphContract, we_effect_pass_node, we_image_graph,
};
