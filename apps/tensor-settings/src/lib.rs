//! Standalone Tensor settings model and product-boundary registry.

mod config;
mod product;

pub use config::{SettingsConfig, SettingsConfigError};
pub use product::{ConfigFormat, ProductEndpoint, ProductKind, ProductRegistry, ReloadRoute};
