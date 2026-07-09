#[cfg(feature = "native-vulkan-renderer")]
pub mod native_vulkan;
#[cfg(feature = "native-wayland-renderer")]
pub mod native_wayland;

use crate::config::{CacheConfig, GilderConfig, PerformanceConfig, VideoDecoderPolicy};
use crate::core::manifest::{Manifest, Variant};
use crate::core::scene::{
    SceneAudioCueCondition, SceneEffectFbo, SceneEffectUvTransform, SceneLayerCompositeKey,
    SceneMesh, SceneNativeEffectMotion, SceneSystemStatus,
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
    pub effect_motion: SceneNativeEffectMotion,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSyncCacheReport {
    #[serde(default)]
    pub package_cache_entries: usize,
    #[serde(default)]
    pub package_cache_max_entries: usize,
    #[serde(default)]
    pub package_cache_max_retained_unique_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_hits: u64,
    #[serde(default)]
    pub package_cache_misses: u64,
    #[serde(default)]
    pub package_cache_evictions: u64,
    #[serde(default)]
    pub package_cache_retained_resource_references: usize,
    #[serde(default)]
    pub package_cache_retained_unique_resources: usize,
    #[serde(default)]
    pub package_cache_retained_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_unique_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_preview_resource_references: usize,
    #[serde(default)]
    pub package_cache_retained_unique_preview_resources: usize,
    #[serde(default)]
    pub package_cache_retained_preview_resource_bytes: u64,
    #[serde(default)]
    pub package_cache_retained_unique_preview_resource_bytes: u64,
    #[serde(default)]
    pub archive_cache_entries: usize,
    #[serde(default)]
    pub archive_cache_max_entries: usize,
    #[serde(default)]
    pub archive_cache_reuses: u64,
    #[serde(default)]
    pub archive_cache_extractions: u64,
    #[serde(default)]
    pub archive_cache_evictions: u64,
    #[serde(default)]
    pub archive_cache_eviction_errors: u64,
    #[serde(default)]
    pub static_image_cache_entries: usize,
    #[serde(default)]
    pub static_image_cache_max_entries: usize,
    #[serde(default)]
    pub static_image_cache_bytes: u64,
    #[serde(default)]
    pub static_image_cache_max_bytes: u64,
    #[serde(default)]
    pub static_image_cache_generations: u64,
    #[serde(default)]
    pub static_image_cache_reuses: u64,
    #[serde(default)]
    pub static_image_cache_generation_errors: u64,
    #[serde(default)]
    pub static_image_cache_evictions: u64,
    #[serde(default)]
    pub static_image_cache_eviction_errors: u64,
    #[serde(default)]
    pub planned_video_source_references: usize,
    #[serde(default)]
    pub planned_unique_video_sources: usize,
    #[serde(default)]
    pub planned_duplicate_video_source_references: usize,
    #[serde(default)]
    pub planned_max_video_source_outputs: usize,
    #[serde(default)]
    pub planned_video_source_reference_bytes: u64,
    #[serde(default)]
    pub planned_unique_video_source_bytes: u64,
    #[serde(default)]
    pub planned_static_image_resources: usize,
    #[serde(default)]
    pub planned_video_poster_resources: usize,
    #[serde(default)]
    pub planned_slideshow_image_resources: usize,
    #[serde(default)]
    pub planned_scene_image_resources: usize,
    #[serde(default)]
    pub planned_image_resource_references: usize,
    #[serde(default)]
    pub planned_unique_image_resources: usize,
    #[serde(default)]
    pub planned_static_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_video_poster_resource_bytes: u64,
    #[serde(default)]
    pub planned_slideshow_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_scene_image_resource_bytes: u64,
    #[serde(default)]
    pub planned_image_resource_reference_bytes: u64,
    #[serde(default)]
    pub planned_unique_image_resource_bytes: u64,
}

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
    config: &GilderConfig,
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
    config: &GilderConfig,
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

fn static_render_sync_plan_inner(
    performance_config: &PerformanceConfig,
    video_decoder_policy: VideoDecoderPolicy,
    cache_config: CacheConfig,
    config: Option<&GilderConfig>,
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

fn update_render_sync_resource_footprint(
    report: &mut RenderSyncCacheReport,
    plans: &[StaticWallpaperPlan],
    video_plans: &[VideoWallpaperPlan],
    slideshow_plans: &[SlideshowWallpaperPlan],
    scene_plans: &[SceneWallpaperPlan],
) {
    let video_poster_resources = video_plans
        .iter()
        .filter(|plan| plan.poster.is_some())
        .count();
    let slideshow_image_resources = slideshow_plans
        .iter()
        .map(|plan| plan.sources.len())
        .sum::<usize>();
    let static_image_resources = plans.len();
    let static_image_resource_bytes = plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>();
    let video_poster_resource_bytes = video_plans
        .iter()
        .filter_map(|plan| plan.poster.as_ref())
        .map(|poster| file_size(poster))
        .sum::<u64>();
    let slideshow_image_resource_bytes = slideshow_plans
        .iter()
        .flat_map(|plan| plan.sources.iter())
        .map(|source| file_size(source))
        .sum::<u64>();
    let scene_image_resources = scene_plans
        .iter()
        .map(|plan| plan.image_sources().len())
        .sum::<usize>();
    let scene_image_resource_bytes = scene_plans
        .iter()
        .flat_map(SceneWallpaperPlan::image_sources)
        .map(file_size)
        .sum::<u64>();
    let mut unique_image_resources = BTreeSet::new();
    unique_image_resources.extend(plans.iter().map(|plan| plan.source.clone()));
    unique_image_resources.extend(
        slideshow_plans
            .iter()
            .flat_map(|plan| plan.sources.iter().cloned()),
    );
    unique_image_resources.extend(
        scene_plans
            .iter()
            .flat_map(SceneWallpaperPlan::image_sources)
            .map(Path::to_path_buf),
    );
    let mut video_source_counts = BTreeMap::new();
    for plan in video_plans {
        *video_source_counts
            .entry(plan.source.clone())
            .or_insert(0_usize) += 1;
    }
    let planned_video_source_reference_bytes = video_plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>();
    let planned_unique_video_source_bytes = video_source_counts
        .keys()
        .map(|source| file_size(source))
        .sum::<u64>();

    report.planned_static_image_resources = static_image_resources;
    report.planned_video_poster_resources = video_poster_resources;
    report.planned_slideshow_image_resources = slideshow_image_resources;
    report.planned_scene_image_resources = scene_image_resources;
    report.planned_image_resource_references =
        plans.len() + slideshow_image_resources + scene_image_resources;
    report.planned_unique_image_resources = unique_image_resources.len();
    report.planned_static_image_resource_bytes = static_image_resource_bytes;
    report.planned_video_poster_resource_bytes = video_poster_resource_bytes;
    report.planned_slideshow_image_resource_bytes = slideshow_image_resource_bytes;
    report.planned_scene_image_resource_bytes = scene_image_resource_bytes;
    report.planned_image_resource_reference_bytes = plans
        .iter()
        .map(|plan| file_size(&plan.source))
        .sum::<u64>()
        + slideshow_image_resource_bytes
        + scene_image_resource_bytes;
    report.planned_unique_image_resource_bytes = unique_image_resources
        .iter()
        .map(|source| file_size(source))
        .sum::<u64>();
    report.planned_video_source_references = video_plans.len();
    report.planned_unique_video_sources = video_source_counts.len();
    report.planned_duplicate_video_source_references =
        video_plans.len().saturating_sub(video_source_counts.len());
    report.planned_max_video_source_outputs = video_source_counts
        .values()
        .copied()
        .max()
        .unwrap_or_default();
    report.planned_video_source_reference_bytes = planned_video_source_reference_bytes;
    report.planned_unique_video_source_bytes = planned_unique_video_source_bytes;
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn source_tree_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| source_tree_size(&entry.path()))
        .sum()
}

fn effective_wallpaper_assignment(
    config: Option<&GilderConfig>,
    state: &AppState,
    output_name: &str,
    output_state: &OutputState,
) -> Option<WallpaperAssignment> {
    output_state
        .wallpaper
        .clone()
        .or_else(|| state.default_wallpaper.clone())
        .or_else(|| {
            config
                .and_then(|config| config.outputs.get(output_name))
                .and_then(|output| output.wallpaper.as_ref())
                .map(|path| config_wallpaper_assignment(path))
        })
        .or_else(|| {
            config
                .and_then(|config| config.default_wallpaper.as_ref())
                .map(|path| config_wallpaper_assignment(path))
        })
}

fn config_wallpaper_assignment(path: &str) -> WallpaperAssignment {
    WallpaperAssignment {
        path: path.to_owned(),
        variant: None,
    }
}

fn output_fit_override(config: Option<&GilderConfig>, output_name: &str) -> Option<FitMode> {
    config
        .and_then(|config| config.outputs.get(output_name))
        .and_then(|output| output.fit)
}

fn effective_output_render_properties(
    state: &AppState,
    output_state: &OutputState,
    output: Option<&DesktopOutput>,
) -> BTreeMap<String, Value> {
    let mut properties = state.properties.clone();
    properties.extend(output_state.properties.clone());
    if let Some(parallax) = output.and_then(|output| output.cursor_parallax) {
        properties.insert("scene.parallax.x".to_owned(), Value::from(parallax.x));
        properties.insert("scene.parallax.y".to_owned(), Value::from(parallax.y));
    }
    properties
}

#[derive(Debug, Clone, Copy)]
struct PlaylistRenderContext<'a> {
    desktop: &'a DesktopSnapshot,
    output_name: &'a str,
    output: Option<&'a DesktopOutput>,
    local_clock: PlaylistClockKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistClockKey {
    pub local_minute_of_day: u16,
    pub local_weekday: PlaylistWeekday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistClockCacheKey {
    pub local_minute_of_day: Option<u16>,
    pub local_weekday: Option<PlaylistWeekday>,
}

fn select_playlist_item<'a>(
    items: &'a [PlaylistItem],
    selection: PlaylistSelection,
    context: Option<&PlaylistRenderContext<'_>>,
) -> Option<&'a PlaylistItem> {
    match selection {
        PlaylistSelection::FirstMatch => items
            .iter()
            .find(|item| playlist_item_matches(item, context)),
        PlaylistSelection::WeightedRandom => select_weighted_playlist_item(items, context),
    }
}

fn select_weighted_playlist_item<'a>(
    items: &'a [PlaylistItem],
    context: Option<&PlaylistRenderContext<'_>>,
) -> Option<&'a PlaylistItem> {
    let candidates = items
        .iter()
        .filter(|item| playlist_item_matches(item, context))
        .collect::<Vec<_>>();
    let total_weight = candidates
        .iter()
        .map(|item| u64::from(item.weight))
        .sum::<u64>();
    if total_weight == 0 {
        return None;
    }

    let mut selected_weight = playlist_weighted_selection_seed(&candidates, context) % total_weight;
    for item in candidates {
        let item_weight = u64::from(item.weight);
        if selected_weight < item_weight {
            return Some(item);
        }
        selected_weight -= item_weight;
    }
    None
}

fn playlist_weighted_selection_seed(
    candidates: &[&PlaylistItem],
    context: Option<&PlaylistRenderContext<'_>>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    "gilder-playlist-weighted-random-v1".hash(&mut hasher);
    if let Some(context) = context {
        context.output_name.hash(&mut hasher);
        context.local_clock.local_minute_of_day.hash(&mut hasher);
        playlist_weekday_seed(context.local_clock.local_weekday).hash(&mut hasher);
        playlist_power_seed(context.desktop.power).hash(&mut hasher);
        context.desktop.session_active.hash(&mut hasher);
        context.desktop.session_locked.hash(&mut hasher);
        if let Some(output) = context.output {
            output.name.hash(&mut hasher);
            output.focused.hash(&mut hasher);
            output.visible.hash(&mut hasher);
            output.has_fullscreen.hash(&mut hasher);
        }
    }
    for item in candidates {
        item.id.hash(&mut hasher);
        item.weight.hash(&mut hasher);
    }
    hasher.finish()
}

fn playlist_power_seed(power: PowerState) -> u8 {
    match power {
        PowerState::Unknown => 0,
        PowerState::Ac => 1,
        PowerState::Battery => 2,
    }
}

fn playlist_weekday_seed(weekday: PlaylistWeekday) -> u8 {
    match weekday {
        PlaylistWeekday::Monday => 1,
        PlaylistWeekday::Tuesday => 2,
        PlaylistWeekday::Wednesday => 3,
        PlaylistWeekday::Thursday => 4,
        PlaylistWeekday::Friday => 5,
        PlaylistWeekday::Saturday => 6,
        PlaylistWeekday::Sunday => 7,
    }
}

fn playlist_item_matches(item: &PlaylistItem, context: Option<&PlaylistRenderContext<'_>>) -> bool {
    let conditions = &item.conditions;
    if !conditions.outputs.is_empty() {
        let Some(context) = context else {
            return false;
        };
        if !conditions
            .outputs
            .iter()
            .any(|output| output == context.output_name)
        {
            return false;
        }
    }
    if let Some(power) = conditions.power {
        let Some(context) = context else {
            return false;
        };
        if !playlist_power_matches(power, context.desktop.power) {
            return false;
        }
    }
    if let Some(local_time) = &conditions.local_time {
        let Some(context) = context else {
            return false;
        };
        if !local_time.contains_minute_of_day(context.local_clock.local_minute_of_day) {
            return false;
        }
    }
    if !conditions.weekdays.is_empty() {
        let Some(context) = context else {
            return false;
        };
        if !conditions
            .weekdays
            .contains(&context.local_clock.local_weekday)
        {
            return false;
        }
    }
    if let Some(expected) = conditions.focused {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.focused != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.visible {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.visible != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.fullscreen {
        let Some(output) = context.and_then(|context| context.output) else {
            return false;
        };
        if output.has_fullscreen != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.session_active {
        let Some(context) = context else {
            return false;
        };
        if context.desktop.session_active != expected {
            return false;
        }
    }
    if let Some(expected) = conditions.session_locked {
        let Some(context) = context else {
            return false;
        };
        if context.desktop.session_locked != expected {
            return false;
        }
    }
    true
}

fn playlist_power_matches(condition: PlaylistPowerCondition, power: PowerState) -> bool {
    matches!(
        (condition, power),
        (PlaylistPowerCondition::Unknown, PowerState::Unknown)
            | (PlaylistPowerCondition::Ac, PowerState::Ac)
            | (PlaylistPowerCondition::Battery, PowerState::Battery)
    )
}

pub fn current_playlist_clock_key() -> PlaylistClockKey {
    let now = jiff::Zoned::now();
    PlaylistClockKey {
        local_minute_of_day: playlist_local_time_override()
            .unwrap_or_else(|| zoned_minute_of_day(now.clone())),
        local_weekday: playlist_local_weekday_override().unwrap_or_else(|| zoned_weekday(now)),
    }
}

pub fn current_playlist_clock_cache_key(
    dependency: PlaylistClockDependency,
) -> Option<PlaylistClockCacheKey> {
    playlist_clock_cache_key(dependency, current_playlist_clock_key())
}

fn playlist_clock_cache_key(
    dependency: PlaylistClockDependency,
    clock: PlaylistClockKey,
) -> Option<PlaylistClockCacheKey> {
    if dependency == PlaylistClockDependency::None {
        return None;
    }
    Some(PlaylistClockCacheKey {
        local_minute_of_day: dependency
            .uses_minute()
            .then_some(clock.local_minute_of_day),
        local_weekday: dependency.uses_weekday().then_some(clock.local_weekday),
    })
}

fn playlist_local_time_override() -> Option<u16> {
    std::env::var("GILDER_PLAYLIST_LOCAL_TIME")
        .ok()
        .as_deref()
        .and_then(crate::core::manifest::parse_playlist_local_time_minute)
}

fn playlist_local_weekday_override() -> Option<PlaylistWeekday> {
    std::env::var("GILDER_PLAYLIST_LOCAL_WEEKDAY")
        .ok()
        .as_deref()
        .and_then(parse_playlist_weekday_name)
}

fn parse_playlist_weekday_name(value: &str) -> Option<PlaylistWeekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" | "mon" => Some(PlaylistWeekday::Monday),
        "tuesday" | "tue" => Some(PlaylistWeekday::Tuesday),
        "wednesday" | "wed" => Some(PlaylistWeekday::Wednesday),
        "thursday" | "thu" => Some(PlaylistWeekday::Thursday),
        "friday" | "fri" => Some(PlaylistWeekday::Friday),
        "saturday" | "sat" => Some(PlaylistWeekday::Saturday),
        "sunday" | "sun" => Some(PlaylistWeekday::Sunday),
        _ => None,
    }
}

fn zoned_minute_of_day(now: jiff::Zoned) -> u16 {
    let hour = u16::try_from(now.hour()).unwrap_or(0);
    let minute = u16::try_from(now.minute()).unwrap_or(0);
    hour * 60 + minute
}

fn zoned_weekday(now: jiff::Zoned) -> PlaylistWeekday {
    gregorian_weekday(now.year().into(), now.month().into(), now.day().into())
}

fn gregorian_weekday(year: i32, month: i32, day: i32) -> PlaylistWeekday {
    const MONTH_OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut adjusted_year = year;
    if month < 3 {
        adjusted_year -= 1;
    }
    let sunday_zero = (adjusted_year + adjusted_year / 4 - adjusted_year / 100
        + adjusted_year / 400
        + MONTH_OFFSETS[(month.clamp(1, 12) - 1) as usize]
        + day)
        .rem_euclid(7);
    match sunday_zero {
        0 => PlaylistWeekday::Sunday,
        1 => PlaylistWeekday::Monday,
        2 => PlaylistWeekday::Tuesday,
        3 => PlaylistWeekday::Wednesday,
        4 => PlaylistWeekday::Thursday,
        5 => PlaylistWeekday::Friday,
        _ => PlaylistWeekday::Saturday,
    }
}

fn effective_dynamic_wallpaper_entry(
    entry: &WallpaperEntry,
    playlist_context: &PlaylistRenderContext<'_>,
) -> bool {
    effective_render_wallpaper_entry(entry, playlist_context)
        .map(dynamic_wallpaper_entry)
        .unwrap_or(false)
}

fn effective_render_wallpaper_entry<'a>(
    entry: &'a WallpaperEntry,
    playlist_context: &PlaylistRenderContext<'_>,
) -> Option<&'a WallpaperEntry> {
    match entry {
        WallpaperEntry::Playlist { items, selection } => {
            select_playlist_item(items, *selection, Some(playlist_context))
                .map(|item| item.entry.as_ref())
        }
        _ => Some(entry),
    }
}

fn playlist_entry_clock_dependency(entry: &WallpaperEntry) -> PlaylistClockDependency {
    let WallpaperEntry::Playlist { items, selection } = entry else {
        return PlaylistClockDependency::None;
    };
    let mut dependency = if *selection == PlaylistSelection::WeightedRandom {
        PlaylistClockDependency::MinuteAndWeekday
    } else {
        PlaylistClockDependency::None
    };
    for item in items {
        if item.conditions.local_time.is_some() {
            dependency = dependency.merge(PlaylistClockDependency::Minute);
        }
        if !item.conditions.weekdays.is_empty() {
            dependency = dependency.merge(PlaylistClockDependency::Weekday);
        }
    }
    dependency
}

fn dynamic_wallpaper_entry(entry: &WallpaperEntry) -> bool {
    match entry {
        WallpaperEntry::Video { .. }
        | WallpaperEntry::Slideshow { .. }
        | WallpaperEntry::Web { .. }
        | WallpaperEntry::Shader { .. }
        | WallpaperEntry::Scene { .. } => true,
        WallpaperEntry::StaticImage { .. } => false,
        WallpaperEntry::Playlist { items, .. } => items
            .iter()
            .any(|item| dynamic_wallpaper_entry(item.entry.as_ref())),
    }
}

pub fn static_wallpaper_plan_for_assignment(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
) -> Result<StaticWallpaperPlan, RendererPlanError> {
    let package = load_assigned_package(assignment, cache_dir.as_ref())?;
    let output_state = OutputState {
        wallpaper: Some(assignment.clone()),
        ..OutputState::default()
    };
    static_wallpaper_plan(output_name, &package, &output_state)?
        .ok_or(RendererPlanError::MissingAssignment)
}

pub fn wallpaper_plan_for_assignment(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    wallpaper_plan_for_assignment_with_target(
        output_name,
        assignment,
        cache_dir,
        performance,
        VideoDecoderPolicy::default(),
        fit_override,
        None,
    )
}

fn wallpaper_plan_for_assignment_with_target(
    output_name: impl Into<String>,
    assignment: &WallpaperAssignment,
    cache_dir: impl AsRef<Path>,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    render_target: Option<RenderTargetSize>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let package = load_assigned_package(assignment, cache_dir.as_ref())?;
    wallpaper_plan_with_target(
        output_name,
        &package,
        performance,
        video_decoder_policy,
        fit_override,
        assignment.variant.as_deref(),
        render_target,
        None,
        None,
        false,
        None,
    )
}

pub fn wallpaper_plan(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
    variant_id: Option<&str>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    wallpaper_plan_with_target(
        output_name,
        package,
        performance,
        VideoDecoderPolicy::default(),
        fit_override,
        variant_id,
        None,
        None,
        None,
        false,
        None,
    )
}

fn wallpaper_plan_with_target(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    variant_id: Option<&str>,
    render_target: Option<RenderTargetSize>,
    playlist_context: Option<&PlaylistRenderContext<'_>>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let output_name = output_name.into();
    let explicit_variant_source = explicit_variant_source(package, variant_id)?;
    wallpaper_entry_plan_with_target(
        &output_name,
        package,
        &package.manifest.entry,
        performance,
        video_decoder_policy,
        fit_override,
        explicit_variant_source,
        true,
        render_target,
        playlist_context,
        render_properties,
        cursor_parallax_input_ready,
        static_image_cache,
    )
}

fn wallpaper_entry_plan_with_target(
    output_name: &str,
    package: &WallpaperPackage,
    entry: &WallpaperEntry,
    performance: &PerformanceDecision,
    video_decoder_policy: VideoDecoderPolicy,
    fit_override: Option<FitMode>,
    explicit_variant_source: Option<&PackagePath>,
    allow_automatic_variants: bool,
    render_target: Option<RenderTargetSize>,
    playlist_context: Option<&PlaylistRenderContext<'_>>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let variant_render_target = allow_automatic_variants.then_some(render_target).flatten();
    match entry {
        WallpaperEntry::StaticImage {
            source,
            fit,
            background,
            width,
            height,
            ..
        } => Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
            output_name: output_name.to_owned(),
            source: static_image_source_path(
                package,
                source,
                effective_fit(*fit, fit_override),
                explicit_variant_source,
                variant_render_target,
                source_dimensions(*width, *height),
                static_image_cache,
            ),
            fit: effective_fit(*fit, fit_override),
            background: background.clone(),
        })),
        WallpaperEntry::Video {
            source,
            poster,
            loop_playback,
            muted,
            fit,
            max_fps,
            start_offset_ms,
        } => {
            let poster = poster
                .as_ref()
                .or(package.manifest.preview.poster.as_ref())
                .map(|poster| poster.join_to(&package.root));
            Ok(WallpaperRenderPlan::Video(VideoWallpaperPlan {
                output_name: output_name.to_owned(),
                source: selected_variant_source(
                    package,
                    explicit_variant_source,
                    variant_render_target,
                )
                .unwrap_or(source)
                .join_to(&package.root),
                poster,
                fit: effective_fit(*fit, fit_override),
                loop_playback: *loop_playback,
                muted: effective_muted(*muted, package.manifest.runtime.allow_audio),
                manifest_max_fps: *max_fps,
                target_max_fps: effective_max_fps(*max_fps, performance.max_fps),
                decoder_policy: video_decoder_policy,
                start_offset_ms: *start_offset_ms,
            }))
        }
        WallpaperEntry::Slideshow {
            sources,
            interval_ms,
            transition,
            fit,
        } => Ok(WallpaperRenderPlan::Slideshow(SlideshowWallpaperPlan {
            output_name: output_name.to_owned(),
            sources: sources
                .iter()
                .map(|source| source.join_to(&package.root))
                .collect(),
            interval_ms: *interval_ms,
            transition: *transition,
            fit: effective_fit(*fit, fit_override),
            target_max_fps: performance.max_fps,
        })),
        WallpaperEntry::Web { fallback, .. } => {
            let Some(fallback) = fallback else {
                return Err(RendererPlanError::UnsupportedEntry(entry.kind().as_str()));
            };
            Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
                output_name: output_name.to_owned(),
                source: fallback.join_to(&package.root),
                fit: effective_fit(FitMode::Cover, fit_override),
                background: Some("#000000".to_owned()),
            }))
        }
        WallpaperEntry::Shader { fallback, .. } => {
            let Some(fallback) = fallback else {
                return Err(RendererPlanError::UnsupportedEntry(entry.kind().as_str()));
            };
            Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
                output_name: output_name.to_owned(),
                source: fallback.join_to(&package.root),
                fit: effective_fit(FitMode::Cover, fit_override),
                background: Some("#000000".to_owned()),
            }))
        }
        WallpaperEntry::Scene { source, max_fps } => {
            Ok(WallpaperRenderPlan::Scene(scene_wallpaper_plan(
                output_name.to_owned(),
                package,
                source,
                *max_fps,
                performance,
                fit_override,
                render_target,
                render_properties,
                cursor_parallax_input_ready,
            )?))
        }
        WallpaperEntry::Playlist { items, selection } => {
            let item = select_playlist_item(items, *selection, playlist_context)
                .ok_or(RendererPlanError::PlaylistNoMatch)?;
            wallpaper_entry_plan_with_target(
                output_name,
                package,
                item.entry.as_ref(),
                performance,
                video_decoder_policy,
                fit_override,
                None,
                false,
                render_target,
                playlist_context,
                render_properties,
                cursor_parallax_input_ready,
                static_image_cache,
            )
        }
    }
}

fn scene_wallpaper_plan(
    output_name: String,
    package: &WallpaperPackage,
    source: &PackagePath,
    manifest_max_fps: Option<u32>,
    performance: &PerformanceDecision,
    fit_override: Option<FitMode>,
    _render_target: Option<RenderTargetSize>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let source_path = source.join_to(&package.root);
    let file = fs::File::open(&source_path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to open scene engine binary {}: {err}",
            source_path.display()
        ))
    })?;
    let storage = SceneStorage::from_binary_reader(file).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to load scene engine binary {}: {err}",
            source_path.display()
        ))
    })?;
    let scene_engine = RenderingServer::new(&storage).scene_engine_render_plan();
    let render_plan = scene_engine.renderer_scene_render;
    let scene_size = if storage.project().logical_width > 0 && storage.project().logical_height > 0
    {
        Some(SceneSize {
            width: storage.project().logical_width,
            height: storage.project().logical_height,
        })
    } else {
        None
    };
    let mut scene_systems = SceneSystems::default();
    if render_plan.render_graph_count > 0 || render_plan.shader_contract_count > 0 {
        scene_systems.shader_material_graph = SceneSystemStatus::Ready;
    }

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps,
        target_max_fps: effective_max_fps(manifest_max_fps, performance.max_fps),
        snapshot_time_ms: 0,
        scene_size,
        scene_fit: effective_fit(FitMode::Cover, fit_override),
        scene_systems,
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count: 0,
        timeline_animated_layer_count: 0,
        puppet_animation_layer_count: 0,
        property_binding_count: 0,
        cursor_parallax_input_ready,
        scene_input_properties: render_properties.cloned().unwrap_or_default(),
        scene_engine: Some(scene_engine),
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: render_plan.material_count,
        scene_material_graph_resource_count: render_plan.resource_count,
        scene_effect_graph_count: render_plan.render_graph_count,
        scene_mesh_count: render_plan.mesh_count,
        scene_mesh_vertex_count: render_plan.mesh_vertex_count,
        scene_mesh_index_count: render_plan.mesh_index_count,
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: scene_engine_unsupported_features(&storage),
        display: None,
        layers: Vec::new(),
    })
}

fn scene_engine_unsupported_features(storage: &SceneStorage) -> Vec<String> {
    storage
        .document()
        .unsupported
        .iter()
        .map(|unsupported| {
            let feature = storage
                .string(unsupported.feature)
                .unwrap_or("<invalid-feature>");
            let containment = storage
                .string(unsupported.containment)
                .unwrap_or("<invalid-containment>");
            format!(
                "scene-engine:{}:object={}:pass={}:{}",
                feature, unsupported.object.0, unsupported.pass_index, containment
            )
        })
        .collect()
}

fn effective_fit(manifest_fit: FitMode, output_fit: Option<FitMode>) -> FitMode {
    output_fit.unwrap_or(manifest_fit)
}

fn explicit_variant_source<'a>(
    package: &'a WallpaperPackage,
    variant_id: Option<&str>,
) -> Result<Option<&'a PackagePath>, RendererPlanError> {
    let Some(variant_id) = variant_id else {
        return Ok(None);
    };
    package
        .manifest
        .variants
        .iter()
        .find(|variant| variant.id == variant_id)
        .map(|variant| Some(&variant.source))
        .ok_or_else(|| RendererPlanError::MissingVariant(variant_id.to_owned()))
}

fn selected_variant_source<'a>(
    package: &'a WallpaperPackage,
    explicit_source: Option<&'a PackagePath>,
    render_target: Option<RenderTargetSize>,
) -> Option<&'a PackagePath> {
    explicit_source.or_else(|| automatic_variant_source(package, render_target))
}

struct StaticImageCacheContext<'a> {
    cache_dir: &'a Path,
    max_entries: usize,
    stats: &'a mut RenderSyncCacheReport,
    protected_files: &'a mut BTreeSet<PathBuf>,
    ffmpeg: Option<&'a Path>,
}

fn static_image_source_path(
    package: &WallpaperPackage,
    source: &PackagePath,
    fit: FitMode,
    explicit_source: Option<&PackagePath>,
    render_target: Option<RenderTargetSize>,
    source_dimensions: Option<RenderTargetSize>,
    static_image_cache: Option<&mut StaticImageCacheContext<'_>>,
) -> PathBuf {
    if let Some(selected) = selected_variant_source(package, explicit_source, render_target) {
        return selected.join_to(&package.root);
    }

    let source_path = source.join_to(&package.root);
    if explicit_source.is_some() {
        return source_path;
    }

    let Some(cache) = static_image_cache else {
        return source_path;
    };
    cached_static_image_variant(&source_path, fit, render_target, source_dimensions, cache)
        .unwrap_or(source_path)
}

fn source_dimensions(width: Option<u32>, height: Option<u32>) -> Option<RenderTargetSize> {
    Some(RenderTargetSize {
        width: width?,
        height: height?,
    })
}
