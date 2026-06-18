//! User configuration and XDG path handling.

pub mod paths;
pub mod settings;

pub use paths::{ApplicationPaths, PathError};
pub use settings::{
    AdapterConfig, GilderConfig, OutputConfig, OutputPerformanceConfig, PerformanceConfig,
    PowerPolicy, ThrottlePolicy, VideoConfig, VideoDecoderPolicy,
};
