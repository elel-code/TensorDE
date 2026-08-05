//! Product-owned catalog and query model for the Tensor launcher.
//!
//! Desktop discovery is a cold-path operation. [`LauncherCatalog`] retains
//! normalized search text so [`LauncherCatalog::query_into`] only allocates the
//! normalized query and a caller-controlled, bounded result vector.

mod catalog;
mod config;
mod query;

pub use catalog::{CatalogDiagnostic, DesktopEntry, LauncherCatalog, LauncherCatalogError};
pub use config::{
    LauncherConfig, LauncherConfigError, MAX_CATALOG_DIAGNOSTICS, MAX_CATALOG_ENTRIES,
    MAX_QUERY_RESULTS, SystemdMode,
};
pub use query::SearchResult;
