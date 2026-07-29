//! Cold-path shader compiler for the TensorDE Vulkan rendering standard.
//!
//! Runtime crates embed the validated SPIR-V artifacts produced here. They do
//! not link Slang, LLVM, or SPIR-V Tools.

mod compiler;
mod contract;
mod error;
mod stage;

pub use compiler::{CompileReport, ShaderCompileRequest, SlangCompiler};
pub use contract::ShaderContract;
pub use error::{Error, Result};
pub use stage::ShaderStage;

/// Exact compiler release used to produce checked-in TensorDE shader assets.
pub const REQUIRED_SLANG_VERSION: &str = "v2026.13.1";

/// SPIR-V capability profile emitted by the standard compiler path.
pub const SPIRV_PROFILE: &str = "spirv_1_5";

/// Vulkan environment used for external SPIR-V validation.
pub const VULKAN_TARGET_ENVIRONMENT: &str = "vulkan1.4";
