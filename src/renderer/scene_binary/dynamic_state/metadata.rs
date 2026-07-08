//! Runtime metadata ingress for binary scene dynamic state.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::renderer::RendererPlanError;

use super::BinarySceneDynamicState;

#[derive(Debug, Deserialize)]
pub(super) struct BinarySceneRuntimeMetadata {
    #[allow(dead_code)]
    pub(super) version: Option<u32>,
    #[serde(default)]
    pub(super) properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub(super) nodes: Vec<BinarySceneRuntimeMetadataNode>,
    #[serde(default)]
    pub(super) property_bindings: Vec<BinarySceneRuntimeMetadataPropertyBinding>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BinarySceneRuntimeMetadataNode {
    pub(super) id: String,
    #[serde(default = "binary_scene_runtime_metadata_default_visible")]
    pub(super) visible: bool,
    #[serde(default)]
    pub(super) visibility_condition: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(in crate::renderer::scene_binary::dynamic_state) struct BinarySceneRuntimeMetadataPropertyBinding
{
    pub(super) property: String,
    #[serde(default)]
    pub(super) target_node: Option<String>,
    pub(super) target: String,
    #[serde(default = "binary_scene_runtime_metadata_default_scale")]
    pub(super) scale: f64,
    #[serde(default)]
    pub(super) offset: f64,
}

pub(in crate::renderer::scene_binary) fn binary_scene_dynamic_state_from_source_path(
    source_path: &Path,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<Option<BinarySceneDynamicState>, RendererPlanError> {
    let metadata_path = binary_scene_runtime_metadata_path(source_path);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&metadata_path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to read binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    let metadata: BinarySceneRuntimeMetadata = serde_json::from_slice(&bytes).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to parse binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    Ok(Some(BinarySceneDynamicState::from_metadata(
        metadata,
        render_properties,
    )))
}

fn binary_scene_runtime_metadata_default_visible() -> bool {
    true
}

fn binary_scene_runtime_metadata_default_scale() -> f64 {
    1.0
}

fn binary_scene_runtime_metadata_path(source_path: &Path) -> PathBuf {
    let mut path = source_path.as_os_str().to_os_string();
    path.push(".runtime.json");
    PathBuf::from(path)
}
