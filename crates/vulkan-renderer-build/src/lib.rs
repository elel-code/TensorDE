//! Cold-path shader compiler for the TensorDE Vulkan rendering standard.
//!
//! Runtime crates embed the validated SPIR-V artifacts produced here. They do
//! not link Slang, LLVM, or SPIR-V Tools.

mod compiler;
mod contract;
mod error;
mod heap_lowering;
mod input_attachment;
mod reflection;
mod stage;
mod transpile;

pub use compiler::{CompileReport, ShaderCompileRequest, SlangCompiler};
pub use contract::ShaderContract;
pub use error::{Error, Result};
pub use heap_lowering::{
    DescriptorHeapBinding, DescriptorHeapBindingKind, DescriptorHeapSlang,
    lower_slang_bindings_to_descriptor_heap, lower_slang_bindings_to_descriptor_heap_at_offset,
    lower_slang_input_attachment_to_descriptor_heap_at_offset,
};
pub use reflection::{
    ShaderInterface, ShaderIoDirection, ShaderScalarType, ShaderStageIo, ShaderUniformBuffer,
    ShaderUniformMember, reflect_shader_interface,
};
pub use stage::ShaderStage;
pub use transpile::{GlslToSlangRequest, SlangSourceReport};

/// Exact compiler release used to produce checked-in TensorDE shader assets.
pub const REQUIRED_SLANG_VERSION: &str = "2026.14";

/// SPIR-V capability profile emitted by the standard compiler path.
pub const SPIRV_PROFILE: &str = "spirv_1_5";

/// Vulkan environment used for external SPIR-V validation.
pub const VULKAN_TARGET_ENVIRONMENT: &str = "vulkan1.4";
