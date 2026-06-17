//! User configuration and XDG path handling.

pub mod paths;
pub mod settings;

pub use paths::{ApplicationPaths, PathError};
pub use settings::{AdapterConfig, GilderConfig, PerformanceConfig, PowerPolicy, ThrottlePolicy};
