//! Product-owned catalog and query model for the Tensor launcher.
//!
//! Desktop discovery is a cold-path operation. [`LauncherCatalog`] retains
//! normalized search text so [`LauncherCatalog::query_into`] only allocates the
//! normalized query and a caller-controlled, bounded result vector.

mod catalog;
mod config;
mod launch;
mod query;
mod session;
mod surface;

pub use catalog::{
    CatalogDiagnostic, DesktopEntry, LauncherCatalog, LauncherCatalogError, LauncherCatalogWatcher,
};
pub use config::{
    LauncherConfig, LauncherConfigError, MAX_CATALOG_DIAGNOSTICS, MAX_CATALOG_ENTRIES,
    MAX_QUERY_RESULTS,
};
pub use launch::{LaunchError, LaunchPlan, LauncherClient};
pub use query::SearchResult;
pub use session::{LauncherSession, LauncherSessionError, MAX_LAUNCHER_QUERY_BYTES};
pub use surface::{LauncherSurface, LauncherSurfaceError};
