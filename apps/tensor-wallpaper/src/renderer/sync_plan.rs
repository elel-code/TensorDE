#[cfg(feature = "rendering-device")]
pub mod rendering_device;
#[cfg(feature = "wayland-renderer")]
pub mod wayland;

use crate::config::{CacheConfig, TensorWallpaperConfig, PerformanceConfig, VideoDecoderPolicy};
use crate::core::manifest::{Manifest, Variant};
use crate::core::scene::{
    SceneAudioCueCondition, SceneEffectFbo, SceneEffectUvTransform, SceneLayerCompositeKey,
    SceneMesh, SceneEffectMotion, SceneSystemStatus,
};
use crate::core::{
    FitMode, PackagePath, PlaylistItem, PlaylistPowerCondition, PlaylistSelection, PlaylistWeekday,
    SceneAlphaTextureMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize,
    SceneSystems, SceneTextAlign, SceneTextureRegion, SceneTransform, Transition, WallpaperEntry,
    WallpaperPackage,
};
use crate::desktop::{CompositorKind, DesktopOutput, DesktopSnapshot, PowerState};
use crate::engine::scene::{RenderingServer, SceneEngineRenderPlan, SceneStorage};
use crate::policy::{PerformanceDecision, RenderMode};
use crate::state::{AppState, OutputState, WallpaperAssignment};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque, hash_map::DefaultHasher};
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "sync_plan/resource_and_playlist.rs"]
mod resource_and_playlist;

pub use resource_and_playlist::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticWallpaperPlan {
    pub output_name: String,
    pub source: PathBuf,
    pub fit: FitMode,
    pub background: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoWallpaperPlan {
    pub output_name: String,
    pub source: PathBuf,
    pub poster: Option<PathBuf>,
    pub fit: FitMode,
    pub loop_playback: bool,
    pub muted: bool,
    pub manifest_max_fps: Option<u32>,
    pub target_max_fps: Option<u32>,
    #[serde(default)]
    pub decoder_policy: VideoDecoderPolicy,
    pub start_offset_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideshowWallpaperPlan {
    pub output_name: String,
    pub sources: Vec<PathBuf>,
    pub interval_ms: u64,
    pub transition: Transition,
    pub fit: FitMode,
    pub target_max_fps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneWallpaperPlan {
    pub output_name: String,
    pub source: Option<PathBuf>,
    pub manifest_max_fps: Option<u32>,
    pub target_max_fps: Option<u32>,
    pub snapshot_time_ms: u64,
    #[serde(default)]
    pub scene_size: Option<SceneSize>,
    #[serde(default = "default_scene_fit")]
    pub scene_fit: FitMode,
    #[serde(default)]
    pub scene_systems: SceneSystems,
    #[serde(default)]
    pub audio_cue_count: usize,
    #[serde(default)]
    pub bound_properties: Vec<String>,
    #[serde(default)]
    pub timeline_animation_count: usize,
    #[serde(default)]
    pub timeline_animated_layer_count: usize,
    #[serde(default)]
    pub puppet_animation_layer_count: usize,
    #[serde(default)]
    pub property_binding_count: usize,
    #[serde(default)]
    pub cursor_parallax_input_ready: bool,
    #[serde(default)]
    pub scene_input_properties: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_engine: Option<SceneEngineRenderPlan>,
    #[serde(default)]
    pub scene_scenescript_binding_count: usize,
    #[serde(default)]
    pub scene_material_graph_count: usize,
    #[serde(default)]
    pub scene_material_graph_resource_count: usize,
    #[serde(default)]
    pub scene_effect_graph_count: usize,
    #[serde(default)]
    pub scene_mesh_count: usize,
    #[serde(default)]
    pub scene_mesh_vertex_count: usize,
    #[serde(default)]
    pub scene_mesh_index_count: usize,
    #[serde(default)]
    pub scene_audio_response_binding_count: usize,
    #[serde(default)]
    pub unsupported_scene_features: Vec<String>,
    pub display: Option<SceneDisplayPlan>,
    pub layers: Vec<SceneRenderLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SceneDisplayPlan {
    Image {
        source: PathBuf,
        fit: FitMode,
        background: Option<String>,
    },
    Color {
        color: String,
    },
}

fn default_scene_fit() -> FitMode {
    FitMode::Cover
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderTextureSlot {
    pub slot: u32,
    pub source: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderImageEffectPass {
    pub effect_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    pub pass_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binds: BTreeMap<u32, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fbos: Vec<SceneEffectFbo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blending: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depthtest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depthwrite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cullmode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alphawriting: Option<String>,
    #[serde(default)]
    pub texture_slots: Vec<SceneRenderTextureSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_uv_transform: Option<SceneEffectUvTransform>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub combos: BTreeMap<String, i64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub constant_shader_values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderAlphaTextureMode {
    #[default]
    Multiply,
    Inverse,
    Iris,
    Coverage,
}

impl SceneRenderAlphaTextureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Multiply => "multiply",
            Self::Inverse => "inverse",
            Self::Iris => "iris",
            Self::Coverage => "coverage",
        }
    }
}

impl From<SceneAlphaTextureMode> for SceneRenderAlphaTextureMode {
    fn from(mode: SceneAlphaTextureMode) -> Self {
        match mode {
            SceneAlphaTextureMode::Multiply => Self::Multiply,
            SceneAlphaTextureMode::Inverse => Self::Inverse,
            SceneAlphaTextureMode::Iris => Self::Iris,
            SceneAlphaTextureMode::Coverage => Self::Coverage,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderLayer {
    pub id: String,
    pub kind: SceneNodeKind,
    pub source: Option<PathBuf>,
    #[serde(default)]
    pub texture_slots: Vec<SceneRenderTextureSlot>,
    #[serde(default)]
    pub alpha_texture_slot: Option<u32>,
    #[serde(default)]
    pub alpha_texture_mode: SceneRenderAlphaTextureMode,
    #[serde(default)]
    pub image_effect_passes: Vec<SceneRenderImageEffectPass>,
    #[serde(default)]
    pub composite_key: Option<SceneLayerCompositeKey>,
    pub texture_region: Option<SceneTextureRegion>,
    #[serde(default)]
    pub effect_motion: SceneEffectMotion,
    #[serde(default)]
    pub blend_mode: SceneBlendMode,
    #[serde(default)]
    pub audio: Vec<SceneRenderAudioCue>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub mesh: Option<Arc<SceneMesh>>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_source: Option<PathBuf>,
    pub font_weight: Option<String>,
    pub text_align: Option<SceneTextAlign>,
    pub path_data: Option<String>,
    pub path_fill_rule: ScenePathFillRule,
    pub fit: FitMode,
    pub opacity: f64,
    pub transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderAudioCue {
    pub source: PathBuf,
    #[serde(default)]
    pub playback_mode: Option<String>,
    #[serde(default)]
    pub volume: Option<Value>,
    #[serde(default)]
    pub start_silent: bool,
    #[serde(default)]
    pub active_conditions: Vec<SceneAudioCueCondition>,
}

impl SceneWallpaperPlan {
    fn image_sources(&self) -> Vec<&Path> {
        let mut sources = Vec::new();
        if let Some(SceneDisplayPlan::Image { source, .. }) = &self.display {
            sources.push(source.as_path());
        }
        for source in self
            .layers
            .iter()
            .filter(|layer| layer.kind == SceneNodeKind::Image)
            .filter_map(|layer| layer.source.as_deref())
        {
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        sources
    }
}

// Keeping the cold-path plan inline avoids an allocation in every output plan.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WallpaperRenderPlan {
    StaticImage(StaticWallpaperPlan),
    Video(VideoWallpaperPlan),
    Slideshow(SlideshowWallpaperPlan),
    Scene(SceneWallpaperPlan),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticRenderSyncPlan {
    pub plans: Vec<StaticWallpaperPlan>,
    #[serde(default)]
    pub video_plans: Vec<VideoWallpaperPlan>,
    #[serde(default)]
    pub slideshow_plans: Vec<SlideshowWallpaperPlan>,
    #[serde(default)]
    pub scene_plans: Vec<SceneWallpaperPlan>,
    pub removals: Vec<String>,
    pub errors: Vec<StaticRenderPlanFailure>,
    #[serde(default)]
    pub decisions: Vec<StaticRenderOutputDecision>,
    #[serde(default)]
    pub playlist_clock_dependency: PlaylistClockDependency,
    #[serde(default)]
    pub cache: RenderSyncCacheReport,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaylistClockDependency {
    #[default]
    None,
    Minute,
    Weekday,
    MinuteAndWeekday,
}

impl PlaylistClockDependency {
    fn merge(self, other: Self) -> Self {
        match (
            self.uses_minute() || other.uses_minute(),
            self.uses_weekday() || other.uses_weekday(),
        ) {
            (false, false) => Self::None,
            (true, false) => Self::Minute,
            (false, true) => Self::Weekday,
            (true, true) => Self::MinuteAndWeekday,
        }
    }

    fn uses_minute(self) -> bool {
        matches!(self, Self::Minute | Self::MinuteAndWeekday)
    }

    fn uses_weekday(self) -> bool {
        matches!(self, Self::Weekday | Self::MinuteAndWeekday)
    }
}

include!("sync_plan/cache_report.rs");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRenderPlanFailure {
    pub output_name: String,
    pub wallpaper: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRenderOutputDecision {
    pub output_name: String,
    pub action: StaticRenderAction,
    pub performance: PerformanceDecision,
    #[serde(default)]
    pub wallpaper: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StaticRenderAction {
    Render,
    Remove,
    Error,
}

pub fn static_render_sync_plan(
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: impl AsRef<Path>,
) -> StaticRenderSyncPlan {
    static_render_sync_plan_with_performance(
        &PerformanceConfig::default(),
        desktop,
        state,
        cache_dir,
    )
}

pub fn static_render_sync_plan_with_config(
    config: &TensorWallpaperConfig,
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: impl AsRef<Path>,
) -> StaticRenderSyncPlan {
    static_render_sync_plan_with_config_and_adaptive(
        config,
        desktop,
        state,
        cache_dir,
        &crate::adaptive::AdaptiveSnapshot::default(),
    )
}

pub fn static_render_sync_plan_with_config_and_adaptive(
    config: &TensorWallpaperConfig,
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: impl AsRef<Path>,
    adaptive: &crate::adaptive::AdaptiveSnapshot,
) -> StaticRenderSyncPlan {
    static_render_sync_plan_inner(
        &config.performance,
        config.video.decoder,
        config.cache,
        Some(config),
        adaptive,
        desktop,
        state,
        cache_dir.as_ref(),
    )
}

pub fn static_render_sync_plan_with_performance(
    performance_config: &PerformanceConfig,
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: impl AsRef<Path>,
) -> StaticRenderSyncPlan {
    static_render_sync_plan_inner(
        performance_config,
        VideoDecoderPolicy::default(),
        CacheConfig::default(),
        None,
        &crate::adaptive::AdaptiveSnapshot::default(),
        desktop,
        state,
        cache_dir.as_ref(),
    )
}

#[allow(clippy::too_many_arguments)]
fn static_render_sync_plan_inner(
    performance_config: &PerformanceConfig,
    video_decoder_policy: VideoDecoderPolicy,
    cache_config: CacheConfig,
    config: Option<&TensorWallpaperConfig>,
    adaptive: &crate::adaptive::AdaptiveSnapshot,
    desktop: &DesktopSnapshot,
    state: &AppState,
    cache_dir: &Path,
) -> StaticRenderSyncPlan {
    let mut output_names: Vec<String> = desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(state.outputs.keys().cloned())
        .collect();
    if let Some(config) = config {
        output_names.extend(config.outputs.keys().cloned());
    }
    output_names.sort();
    output_names.dedup();

    let mut plans = Vec::new();
    let mut video_plans = Vec::new();
    let mut slideshow_plans = Vec::new();
    let mut scene_plans = Vec::new();
    let mut removals = Vec::new();
    let mut errors = Vec::new();
    let mut decisions = Vec::new();
    let mut playlist_clock_dependency = PlaylistClockDependency::None;
    let mut package_cache = RenderPackageCache::new(
        cache_dir,
        cache_config.package_cache_max_entries,
        cache_config.package_cache_max_retained_unique_resource_bytes,
    );
    let playlist_clock = current_playlist_clock_key();
    for output_name in output_names {
        let desktop_output = desktop.output(&output_name);
        let output_state = state.outputs.get(&output_name).cloned().unwrap_or_default();
        let effective_performance_config = config
            .map(|config| config.performance_for_output(&output_name))
            .unwrap_or_else(|| performance_config.clone());
        let mut performance = crate::policy::decide_performance(
            &effective_performance_config,
            desktop,
            desktop_output,
            &output_state,
        );
        if let Some(config) = config {
            performance = crate::policy::apply_adaptive_policy(
                performance,
                config,
                &output_name,
                desktop_output,
                adaptive,
            );
        }
        let assignment = effective_wallpaper_assignment(config, state, &output_name, &output_state);
        let fit_override = output_fit_override(config, &output_name);

        if performance.mode == RenderMode::Paused {
            removals.push(output_name.clone());
            decisions.push(StaticRenderOutputDecision {
                output_name,
                action: StaticRenderAction::Remove,
                performance,
                wallpaper: assignment
                    .as_ref()
                    .map(|assignment| assignment.path.clone()),
            });
            continue;
        }

        let Some(assignment) = assignment.as_ref() else {
            removals.push(output_name.clone());
            decisions.push(StaticRenderOutputDecision {
                output_name,
                action: StaticRenderAction::Remove,
                performance,
                wallpaper: None,
            });
            continue;
        };
        let render_target = render_target_size(desktop.compositor, desktop_output);
        let package = match package_cache.package(assignment) {
            Ok(package) => package,
            Err(err) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name: output_name.clone(),
                    action: StaticRenderAction::Error,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                errors.push(StaticRenderPlanFailure {
                    output_name,
                    wallpaper: assignment.path.clone(),
                    message: err.to_string(),
                });
                continue;
            }
        };
        playlist_clock_dependency = playlist_clock_dependency
            .merge(playlist_entry_clock_dependency(&package.manifest.entry));
        performance = crate::policy::apply_runtime_policy(
            performance,
            &package.manifest.runtime,
            desktop_output,
        );
        let playlist_context = PlaylistRenderContext {
            desktop,
            output_name: &output_name,
            output: desktop_output,
            local_clock: playlist_clock,
        };
        let dynamic_wallpaper =
            effective_dynamic_wallpaper_entry(&package.manifest.entry, &playlist_context);
        performance = crate::policy::apply_desktop_dynamic_policy(
            performance,
            &effective_performance_config,
            desktop,
            desktop_output,
            dynamic_wallpaper,
        );
        performance = crate::policy::apply_power_dynamic_policy(
            performance,
            &effective_performance_config,
            desktop,
            dynamic_wallpaper,
        );
        if let Some(config) = config {
            performance = crate::policy::apply_adaptive_dynamic_policy(
                performance,
                config,
                &output_name,
                adaptive,
                dynamic_wallpaper,
            );
        }

        if performance.mode == RenderMode::Paused {
            removals.push(output_name.clone());
            decisions.push(StaticRenderOutputDecision {
                output_name,
                action: StaticRenderAction::Remove,
                performance,
                wallpaper: Some(assignment.path.clone()),
            });
            continue;
        }

        let render_entry =
            effective_render_wallpaper_entry(&package.manifest.entry, &playlist_context)
                .unwrap_or(&package.manifest.entry);
        let plan_result = match render_entry {
            WallpaperEntry::StaticImage { .. } => {
                let mut static_image_cache = StaticImageCacheContext {
                    cache_dir,
                    max_entries: cache_config.static_image_cache_max_entries,
                    stats: &mut package_cache.stats,
                    protected_files: &mut package_cache.protected_static_cache_files,
                    ffmpeg: None,
                };
                wallpaper_plan_with_target(
                    &output_name,
                    &package,
                    &performance,
                    video_decoder_policy,
                    fit_override,
                    assignment.variant.as_deref(),
                    render_target,
                    Some(&playlist_context),
                    None,
                    false,
                    Some(&mut static_image_cache),
                )
            }
            WallpaperEntry::Scene { .. } => {
                let render_properties =
                    effective_output_render_properties(state, &output_state, desktop_output);
                let cursor_parallax_input_ready = desktop_output
                    .and_then(|output| output.cursor_parallax)
                    .is_some();
                wallpaper_plan_with_target(
                    &output_name,
                    &package,
                    &performance,
                    video_decoder_policy,
                    fit_override,
                    assignment.variant.as_deref(),
                    render_target,
                    Some(&playlist_context),
                    Some(&render_properties),
                    cursor_parallax_input_ready,
                    None,
                )
            }
            _ => wallpaper_plan_with_target(
                &output_name,
                &package,
                &performance,
                video_decoder_policy,
                fit_override,
                assignment.variant.as_deref(),
                render_target,
                Some(&playlist_context),
                None,
                false,
                None,
            ),
        };

        match plan_result {
            Ok(WallpaperRenderPlan::StaticImage(plan)) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name,
                    action: StaticRenderAction::Render,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                plans.push(plan);
            }
            Ok(WallpaperRenderPlan::Video(plan)) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name,
                    action: StaticRenderAction::Render,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                video_plans.push(plan);
            }
            Ok(WallpaperRenderPlan::Slideshow(plan)) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name,
                    action: StaticRenderAction::Render,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                slideshow_plans.push(plan);
            }
            Ok(WallpaperRenderPlan::Scene(plan)) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name,
                    action: StaticRenderAction::Render,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                scene_plans.push(plan);
            }
            Err(err) => {
                decisions.push(StaticRenderOutputDecision {
                    output_name: output_name.clone(),
                    action: StaticRenderAction::Error,
                    performance,
                    wallpaper: Some(assignment.path.clone()),
                });
                errors.push(StaticRenderPlanFailure {
                    output_name,
                    wallpaper: assignment.path.clone(),
                    message: err.to_string(),
                });
            }
        }
    }

    let mut cache = package_cache.finish(cache_config);
    update_render_sync_resource_footprint(
        &mut cache,
        &plans,
        &video_plans,
        &slideshow_plans,
        &scene_plans,
    );
    StaticRenderSyncPlan {
        plans,
        video_plans,
        slideshow_plans,
        scene_plans,
        removals,
        errors,
        decisions,
        playlist_clock_dependency,
        cache,
    }
}
