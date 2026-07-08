//! Binary `.gscn` wallpaper-plan entry points.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::core::scene::binary::SceneBinaryChunkKind;
use crate::core::{FitMode, SceneSystems};
use crate::renderer::{RendererPlanError, SceneWallpaperPlan};

use super::dynamic_state::binary_scene_dynamic_state_from_source_path;
use super::facts::{
    binary_scene_names, binary_scene_package_root, binary_scene_puppet_animation_layer_count,
    binary_scene_resources, binary_scene_size, binary_scene_timeline_counts,
};
use super::reader::BinarySceneReader;
use super::render_layers::binary_scene_render_layers;
use super::topology::binary_scene_retained_topology;

pub(in crate::renderer) fn scene_wallpaper_plan_from_gscn_path(
    output_name: String,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    scene_wallpaper_plan_from_gscn_path_with_properties(
        output_name,
        source_path,
        target_max_fps,
        snapshot_time_ms,
        fit_override,
        None,
    )
}

pub(in crate::renderer) fn scene_wallpaper_plan_from_gscn_path_with_properties(
    output_name: String,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let mut reader = BinarySceneReader::open(&source_path)?;
    let names = binary_scene_names(&mut reader)?;
    let package_root = binary_scene_package_root(&source_path);
    let resources = binary_scene_resources(&mut reader, &names, &package_root)?;
    let topology = binary_scene_retained_topology(&mut reader, &resources)?;
    let scene_size = binary_scene_size(&mut reader)?;
    let dynamic_state =
        binary_scene_dynamic_state_from_source_path(&source_path, render_properties)?;
    let layers = binary_scene_render_layers(
        &mut reader,
        &names,
        &resources,
        &topology,
        snapshot_time_ms,
        dynamic_state.as_ref(),
    )?;
    let (timeline_animation_count, timeline_animated_layer_count) =
        binary_scene_timeline_counts(&mut reader)?;
    let puppet_animation_layer_count = binary_scene_puppet_animation_layer_count(&mut reader)?;
    let particle_emitter_count = reader.chunk_count(SceneBinaryChunkKind::ParticleEmitter);
    let scene_systems = SceneSystems {
        particles: if particle_emitter_count > 0 {
            crate::core::SceneSystemStatus::Ready
        } else {
            crate::core::SceneSystemStatus::Absent
        },
        ..Default::default()
    };

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps: None,
        target_max_fps,
        snapshot_time_ms,
        scene_size,
        scene_fit: fit_override.unwrap_or(FitMode::Stretch),
        scene_systems,
        audio_cue_count: 0,
        bound_properties: dynamic_state
            .as_ref()
            .map(|state| state.bound_properties.clone())
            .unwrap_or_default(),
        timeline_animation_count,
        timeline_animated_layer_count,
        puppet_animation_layer_count,
        property_binding_count: dynamic_state
            .as_ref()
            .map(|state| state.property_bindings.len())
            .unwrap_or_default(),
        cursor_parallax_input_ready: false,
        scene_input_properties: dynamic_state
            .as_ref()
            .map(|state| state.properties.clone())
            .unwrap_or_default(),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: reader.chunk_count(SceneBinaryChunkKind::MaterialPass),
        scene_material_graph_resource_count: resources.len(),
        scene_effect_graph_count: reader.chunk_count(SceneBinaryChunkKind::EffectPass),
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        display: None,
        layers,
    })
}
