//! Standalone Tensor settings model and product-boundary registry.

mod config;
mod document;
mod product;
mod schema;
mod surface;

pub use config::{SettingsConfig, SettingsConfigError};
pub use document::{
    ConfigDocument, ConfigDocumentError, ConfigDocumentState, MAX_CONFIG_DRAFT_BYTES,
    SaveAndReloadOutcome, SaveConfirmation, SaveOutcome, SettingsWorkspace,
};
pub use product::{ConfigFormat, ProductEndpoint, ProductKind, ProductRegistry, ReloadRoute};
pub use schema::{
    ConfigDiagnostic, ConfigPreview, FilesConfigPreview, GreeterConfigPreview, IdleConfigPreview,
    LauncherConfigPreview, PowerPolicyPreview, ShellLayoutPreview, XdpColorScheme,
    XdpConfigPreview, XdpContrast, validate_product_config,
};
pub use surface::{SettingsSurface, SettingsSurfaceError};
