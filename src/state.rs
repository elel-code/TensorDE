//! Persisted daemon state.

pub mod model;
pub mod store;

pub use model::{AppState, OutputState, WallpaperAssignment};
pub use store::{load_state, save_state, StateStoreError};
