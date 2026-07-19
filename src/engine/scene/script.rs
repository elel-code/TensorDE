//! Sandboxed SceneScript execution between frame events and semantic ECS.
//!
//! One scene owns one QuickJS runtime and context. Modules are compiled once at load time and
//! publish compact deltas; renderer backends never access JavaScript values.

mod runtime;
pub(crate) mod standard_library;

pub use runtime::{
    SceneScriptDelta, SceneScriptError, SceneScriptFrameInput, SceneScriptProgram,
    SceneScriptRuntime,
};
