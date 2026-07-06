//! Backend-independent scene engine contracts.
//!
//! This module is the migration boundary described in
//! `docs/gilder-scene-engine-architecture.md`: typed scene/runtime data lives
//! here, while renderer backends execute the resulting resource, graph, and
//! update plans.

pub mod frame;
pub mod render_graph;
pub mod resources;
pub mod scene;
pub mod scene_engine;
pub mod telemetry;
