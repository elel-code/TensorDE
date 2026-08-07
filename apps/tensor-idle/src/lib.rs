//! Independent, deterministic idle policy for TensorDE.

mod config;
mod plan;
mod power_source;
mod runtime;
mod system_actions;

pub use config::{IdleConfig, IdleConfigError, IdleConfigWatcher, PowerPolicy};
pub use plan::{IdleAction, IdlePlan, IdleStage, PowerSource};
pub use power_source::{PowerSourceService, PowerSourceServiceError, PowerSourceStatus};
pub use runtime::{IdleMonitorRuntime, IdleRuntimeError, IdleTransition};
pub use system_actions::{LogindActionExecutor, SystemActionError, system_actions_required};
