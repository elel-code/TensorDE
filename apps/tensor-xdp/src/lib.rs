//! TensorDE's dedicated xdg-desktop-portal backend.
//!
//! Portal requests stay in this process. Tensorland exposes only explicit,
//! value-only IPC capabilities and never lends Wayland, ECS, or Vulkan owners.

mod config;
mod service;
mod settings;

pub use config::{AppearanceSettings, ColorScheme, ConfigError, Contrast, TensorXdpConfig};
pub use service::{BUS_NAME, OBJECT_PATH, ServiceError, SettingsService};
pub use settings::{SettingsError, SettingsSnapshot};
