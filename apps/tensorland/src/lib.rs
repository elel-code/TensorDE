#![deny(unsafe_code)]

mod backend;
mod compositor;
mod config;
pub use config::{
    Config, ConfigDiagnostic, ConfigDiagnosticCategory, ConfigDiagnosticMetadata, ConfigError,
    ConfigReloadFailure, ConfigReloadResult, ConfigTransaction, MAX_DIAGNOSTIC_PATH_BYTES,
    MAX_DIAGNOSTIC_SUMMARY_BYTES, MAX_VALIDATION_COMMAND_BYTES,
};
pub mod ecs;
pub mod ipc;
pub mod layout;
pub mod overview;
mod protocol;
mod render;
pub mod scene;
pub mod service;
mod signals;
pub mod spawn;
pub mod startup;
mod xwayland;
