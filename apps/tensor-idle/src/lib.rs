//! Independent, deterministic idle policy for TensorDE.

mod config;
mod plan;
mod runtime;

pub use config::{IdleConfig, IdleConfigError, PowerPolicy};
pub use plan::{IdleAction, IdlePlan, IdleStage, PowerSource};
pub use runtime::{IdleMonitorRuntime, IdleRuntimeError, IdleTransition};
