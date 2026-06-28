//! Rendering plans and native renderer backends.

#[cfg(feature = "native-vulkan-renderer")]
pub mod native_vulkan;
#[cfg(feature = "native-wayland-renderer")]
pub mod native_wayland;
mod scene_display;

use self::scene_display::{
    scene_background_color, scene_direct_display_color, scene_direct_display_image,
    scene_layer_is_snapshot_renderable,
};
use crate::config::{CacheConfig, GilderConfig, PerformanceConfig, VideoDecoderPolicy};
use crate::core::manifest::{Manifest, PropertySpec, Variant};
use crate::core::scene::{SceneEffect, SceneSnapshotLayer};
use crate::core::{
    FitMode, PackagePath, PlaylistItem, PlaylistPowerCondition, PlaylistSelection, PlaylistWeekday,
    SceneAudioCue, SceneDocument, SceneNode, SceneNodeKind, ScenePathFillRule, SceneResource,
    SceneResourceKind, SceneSize, SceneSystemStatus, SceneSystems, SceneTextAlign,
    SceneTextureRegion, SceneTransform, Transition, WallpaperEntry, WallpaperPackage,
};
use crate::desktop::{CompositorKind, DesktopOutput, DesktopSnapshot, PowerState};
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
    pub property_binding_count: usize,
    #[serde(default)]
    pub cursor_parallax_input_ready: bool,
    #[serde(default)]
    pub scene_scenescript_binding_count: usize,
    #[serde(default)]
    pub scene_material_graph_count: usize,
    #[serde(default)]
    pub scene_material_graph_resource_count: usize,
    #[serde(default)]
    pub scene_effect_graph_count: usize,
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
pub struct SceneRenderLayer {
    pub id: String,
    pub kind: SceneNodeKind,
    pub source: Option<PathBuf>,
    pub texture_region: Option<SceneTextureRegion>,
    #[serde(default)]
    pub audio: Vec<SceneRenderAudioCue>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub corner_radius: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ScenePlanSystemMetrics {
    scenescript_binding_count: usize,
    material_graph_count: usize,
    material_graph_resource_count: usize,
    effect_graph_count: usize,
    audio_response_binding_count: usize,
    unsupported_features: Vec<String>,
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

pub fn scene_wallpaper_plan_from_gscene_path(
    output_name: String,
    package_root: &Path,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    scene_wallpaper_plan_from_gscene_path_with_properties(
        output_name,
        package_root,
        source_path,
        target_max_fps,
        snapshot_time_ms,
        fit_override,
        None,
        false,
    )
}

pub fn scene_wallpaper_plan_from_gscene_path_with_properties(
    output_name: String,
    package_root: &Path,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    snapshot_time_ms: u64,
    fit_override: Option<FitMode>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let document = load_scene_document(&source_path)?;
    let snapshot = document.snapshot_at_with_property_resolver(snapshot_time_ms, |property| {
        render_properties
            .and_then(|properties| properties.get(property))
            .and_then(scene_json_property_number)
            .or_else(|| scene_runtime_property_value(&document, snapshot_time_ms, property))
    });
    let layers = scene_render_layers_from_snapshot(package_root, &document, snapshot.layers)?;
    let system_metrics = scene_plan_system_metrics(&document);
    let display = scene_display_plan(
        Some(source_path.as_path()),
        &document,
        &layers,
        fit_override,
        None,
        None,
    );

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps: None,
        target_max_fps,
        snapshot_time_ms: snapshot.time_ms,
        scene_size: document.size,
        scene_fit: fit_override.unwrap_or(FitMode::Cover),
        scene_systems: document.systems.clone(),
        audio_cue_count: layers.iter().map(|layer| layer.audio.len()).sum(),
        bound_properties: scene_bound_properties(&document),
        timeline_animation_count: scene_timeline_animation_count(&document),
        timeline_animated_layer_count: scene_timeline_animated_layer_count(&document),
        property_binding_count: document.property_bindings.len(),
        cursor_parallax_input_ready,
        scene_scenescript_binding_count: system_metrics.scenescript_binding_count,
        scene_material_graph_count: system_metrics.material_graph_count,
        scene_material_graph_resource_count: system_metrics.material_graph_resource_count,
        scene_effect_graph_count: system_metrics.effect_graph_count,
        scene_audio_response_binding_count: system_metrics.audio_response_binding_count,
        unsupported_scene_features: system_metrics.unsupported_features,
        display,
        layers,
    })
}

#[derive(Debug, Clone)]
pub struct SceneWallpaperRuntimeSampler {
    output_name: String,
    package_root: PathBuf,
    source_path: PathBuf,
    target_max_fps: Option<u32>,
    scene_fit: FitMode,
    cursor_parallax_input_ready: bool,
    document: SceneDocument,
    snapshot_layers_scratch: Vec<SceneSnapshotLayer>,
    render_layers_scratch: Vec<SceneRenderLayer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneWallpaperRuntimeFrame {
    pub snapshot_time_ms: u64,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
    pub layers: Vec<SceneRenderLayer>,
}

impl SceneWallpaperRuntimeSampler {
    pub fn from_plan(plan: &SceneWallpaperPlan) -> Result<Option<Self>, RendererPlanError> {
        let Some(source_path) = plan.source.clone() else {
            return Ok(None);
        };
        let document = load_scene_document(&source_path)?;
        Ok(Some(Self {
            output_name: plan.output_name.clone(),
            package_root: scene_default_gscene_package_root(&source_path),
            source_path,
            target_max_fps: plan.target_max_fps,
            scene_fit: plan.scene_fit,
            cursor_parallax_input_ready: plan.cursor_parallax_input_ready,
            document,
            snapshot_layers_scratch: Vec::new(),
            render_layers_scratch: Vec::new(),
        }))
    }

    pub fn sample_frame(
        &self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        let snapshot = self
            .document
            .snapshot_at_with_property_resolver(time_ms, |property| {
                scene_runtime_property_value(&self.document, time_ms, property)
            });
        let layers =
            scene_render_layers_from_snapshot(&self.package_root, &self.document, snapshot.layers)?;
        Ok(SceneWallpaperRuntimeFrame {
            snapshot_time_ms: snapshot.time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers,
        })
    }

    pub fn sample_frame_reusing(
        &mut self,
        time_ms: u64,
    ) -> Result<SceneWallpaperRuntimeFrame, RendererPlanError> {
        self.document.snapshot_layers_at_with_property_resolver(
            time_ms,
            |property| scene_runtime_property_value(&self.document, time_ms, property),
            &mut self.snapshot_layers_scratch,
        );
        scene_render_layers_from_snapshot_into(
            &self.package_root,
            &self.document,
            &mut self.snapshot_layers_scratch,
            &mut self.render_layers_scratch,
        )?;
        Ok(SceneWallpaperRuntimeFrame {
            snapshot_time_ms: time_ms,
            scene_size: self.document.size,
            scene_fit: self.scene_fit,
            layers: std::mem::take(&mut self.render_layers_scratch),
        })
    }

    pub fn recycle_frame(&mut self, mut frame: SceneWallpaperRuntimeFrame) {
        frame.layers.clear();
        self.render_layers_scratch = frame.layers;
    }

    pub fn sample_plan(&self, time_ms: u64) -> Result<SceneWallpaperPlan, RendererPlanError> {
        let frame = self.sample_frame(time_ms)?;
        let system_metrics = scene_plan_system_metrics(&self.document);
        let display = scene_display_plan(
            Some(self.source_path.as_path()),
            &self.document,
            &frame.layers,
            Some(self.scene_fit),
            None,
            None,
        );
        Ok(SceneWallpaperPlan {
            output_name: self.output_name.clone(),
            source: Some(self.source_path.clone()),
            manifest_max_fps: None,
            target_max_fps: self.target_max_fps,
            snapshot_time_ms: frame.snapshot_time_ms,
            scene_size: frame.scene_size,
            scene_fit: frame.scene_fit,
            scene_systems: self.document.systems.clone(),
            audio_cue_count: frame.layers.iter().map(|layer| layer.audio.len()).sum(),
            bound_properties: scene_bound_properties(&self.document),
            timeline_animation_count: scene_timeline_animation_count(&self.document),
            timeline_animated_layer_count: scene_timeline_animated_layer_count(&self.document),
            property_binding_count: self.document.property_bindings.len(),
            cursor_parallax_input_ready: self.cursor_parallax_input_ready,
            scene_scenescript_binding_count: system_metrics.scenescript_binding_count,
            scene_material_graph_count: system_metrics.material_graph_count,
            scene_material_graph_resource_count: system_metrics.material_graph_resource_count,
            scene_effect_graph_count: system_metrics.effect_graph_count,
            scene_audio_response_binding_count: system_metrics.audio_response_binding_count,
            unsupported_scene_features: system_metrics.unsupported_features,
            display,
            layers: frame.layers,
        })
    }
}

fn scene_default_gscene_package_root(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.file_name().and_then(|name| name.to_str()) == Some("assets")
        && let Some(root) = parent.parent()
    {
        return root.to_path_buf();
    }
    parent.to_path_buf()
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
    pub scene_snapshot_cache_entries: usize,
    #[serde(default)]
    pub scene_snapshot_cache_max_entries: usize,
    #[serde(default)]
    pub scene_snapshot_cache_bytes: u64,
    #[serde(default)]
    pub scene_snapshot_cache_max_bytes: u64,
    #[serde(default)]
    pub scene_snapshot_cache_generations: u64,
    #[serde(default)]
    pub scene_snapshot_cache_reuses: u64,
    #[serde(default)]
    pub scene_snapshot_cache_generation_errors: u64,
    #[serde(default)]
    pub scene_snapshot_cache_evictions: u64,
    #[serde(default)]
    pub scene_snapshot_cache_eviction_errors: u64,
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
                    None,
                )
            }
            WallpaperEntry::Scene { .. } => {
                let render_properties =
                    effective_output_render_properties(state, &output_state, desktop_output);
                let cursor_parallax_input_ready = desktop_output
                    .and_then(|output| output.cursor_parallax)
                    .is_some();
                let mut scene_snapshot_cache = SceneSnapshotCacheContext {
                    cache_dir,
                    max_entries: cache_config.static_image_cache_max_entries,
                    stats: &mut package_cache.stats,
                    protected_files: &mut package_cache.protected_scene_snapshot_files,
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
                    Some(&render_properties),
                    cursor_parallax_input_ready,
                    None,
                    Some(&mut scene_snapshot_cache),
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
    scene_snapshot_cache: Option<&mut SceneSnapshotCacheContext<'_>>,
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
        scene_snapshot_cache,
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
    scene_snapshot_cache: Option<&mut SceneSnapshotCacheContext<'_>>,
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
                scene_snapshot_cache,
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
                scene_snapshot_cache,
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
    render_target: Option<RenderTargetSize>,
    render_properties: Option<&BTreeMap<String, Value>>,
    cursor_parallax_input_ready: bool,
    scene_snapshot_cache: Option<&mut SceneSnapshotCacheContext<'_>>,
) -> Result<SceneWallpaperPlan, RendererPlanError> {
    let source_path = source.join_to(&package.root);
    let document = load_scene_document(&source_path)?;
    let snapshot = document.snapshot_at_with_property_resolver(0, |property| {
        scene_property_value(property, render_properties, &package.manifest.properties)
            .or_else(|| scene_runtime_property_value(&document, 0, property))
    });
    let layers = scene_render_layers_from_snapshot(&package.root, &document, snapshot.layers)?;
    let system_metrics = scene_plan_system_metrics(&document);
    let display = scene_display_plan(
        Some(source_path.as_path()),
        &document,
        &layers,
        fit_override,
        render_target,
        scene_snapshot_cache,
    );

    Ok(SceneWallpaperPlan {
        output_name,
        source: Some(source_path),
        manifest_max_fps,
        target_max_fps: effective_max_fps(manifest_max_fps, performance.max_fps),
        snapshot_time_ms: snapshot.time_ms,
        scene_size: document.size,
        scene_fit: fit_override.unwrap_or(FitMode::Cover),
        scene_systems: document.systems.clone(),
        audio_cue_count: layers.iter().map(|layer| layer.audio.len()).sum(),
        bound_properties: scene_bound_properties(&document),
        timeline_animation_count: scene_timeline_animation_count(&document),
        timeline_animated_layer_count: scene_timeline_animated_layer_count(&document),
        property_binding_count: document.property_bindings.len(),
        cursor_parallax_input_ready,
        scene_scenescript_binding_count: system_metrics.scenescript_binding_count,
        scene_material_graph_count: system_metrics.material_graph_count,
        scene_material_graph_resource_count: system_metrics.material_graph_resource_count,
        scene_effect_graph_count: system_metrics.effect_graph_count,
        scene_audio_response_binding_count: system_metrics.audio_response_binding_count,
        unsupported_scene_features: system_metrics.unsupported_features,
        display,
        layers,
    })
}

fn scene_plan_system_metrics(document: &SceneDocument) -> ScenePlanSystemMetrics {
    let mut metrics = ScenePlanSystemMetrics {
        unsupported_features: document
            .unsupported_features
            .iter()
            .map(|feature| feature.feature.clone())
            .collect(),
        ..ScenePlanSystemMetrics::default()
    };
    metrics.scenescript_binding_count =
        scene_scenescript_runtime_binding_count(document, &metrics.unsupported_features);
    metrics.material_graph_count = scene_material_graph_node_count(&document.nodes);
    metrics.material_graph_resource_count = scene_material_graph_resource_count(document);
    metrics.effect_graph_count = scene_effect_graph_node_count(&document.nodes);
    metrics.audio_response_binding_count =
        scene_audio_response_binding_count(document, &metrics.unsupported_features);
    metrics
}

fn scene_scenescript_runtime_binding_count(
    document: &SceneDocument,
    unsupported_features: &[String],
) -> usize {
    if unsupported_features
        .iter()
        .any(|feature| feature.contains("scenescript"))
    {
        return 0;
    }
    document.property_bindings.len()
}

fn scene_material_graph_node_count(nodes: &[crate::core::SceneNode]) -> usize {
    nodes
        .iter()
        .map(|node| {
            let node_count = usize::from(
                node.resource.is_some()
                    && node.provenance.as_ref().is_some_and(|provenance| {
                        provenance.model.as_ref().is_some_and(|model| {
                            model.material_resource.is_some() && !model.texture_resources.is_empty()
                        })
                    }),
            );
            node_count + scene_material_graph_node_count(&node.children)
        })
        .sum()
}

fn scene_material_graph_resource_count(document: &SceneDocument) -> usize {
    document
        .resources
        .iter()
        .filter(|resource| {
            resource.role.as_deref().is_some_and(|role| {
                role == "we-material"
                    || role == "we-material-texture"
                    || role == "we-material-texture-decoded-frame"
                    || role == "we-material-texture-decoded-atlas"
            })
        })
        .count()
}

fn scene_effect_graph_node_count(nodes: &[crate::core::SceneNode]) -> usize {
    nodes
        .iter()
        .map(|node| {
            let node_count = usize::from(
                node.effects
                    .iter()
                    .any(scene_effect_requires_shader_graph_runtime),
            );
            node_count + scene_effect_graph_node_count(&node.children)
        })
        .sum()
}

fn scene_effect_requires_shader_graph_runtime(effect: &SceneEffect) -> bool {
    if effect.resource.is_some() {
        return true;
    }
    matches!(
        effect.runtime.as_deref(),
        Some("wallpaper-engine-effect") | Some("we-effect-runtime")
    )
}

fn scene_audio_response_binding_count(
    document: &SceneDocument,
    unsupported_features: &[String],
) -> usize {
    if unsupported_features
        .iter()
        .any(|feature| feature.contains("audio"))
    {
        return 0;
    }
    document
        .property_bindings
        .iter()
        .filter(|binding| {
            let property = binding.property.to_ascii_lowercase();
            property.contains("audio")
                || property.contains("spectrum")
                || property.contains("bass")
                || property.contains("mid")
                || property.contains("treble")
        })
        .count()
}

fn scene_render_layers_from_snapshot(
    package_root: &Path,
    document: &SceneDocument,
    layers: Vec<SceneSnapshotLayer>,
) -> Result<Vec<SceneRenderLayer>, RendererPlanError> {
    let mut output = Vec::with_capacity(layers.len());
    let mut layers = layers;
    scene_render_layers_from_snapshot_into(package_root, document, &mut layers, &mut output)?;
    Ok(output)
}

fn scene_render_layers_from_snapshot_into(
    package_root: &Path,
    document: &SceneDocument,
    layers: &mut Vec<SceneSnapshotLayer>,
    output: &mut Vec<SceneRenderLayer>,
) -> Result<(), RendererPlanError> {
    output.clear();
    let scene_resource_lookup = document
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<BTreeMap<_, _>>();
    for layer in layers.drain(..) {
        let audio =
            scene_render_audio_cues(package_root, &scene_resource_lookup, &layer.id, layer.audio)?;
        output.push(SceneRenderLayer {
            id: layer.id,
            kind: layer.kind,
            source: layer.source.map(|source| source.join_to(package_root)),
            texture_region: layer.texture_region,
            audio,
            color: layer.color,
            stroke_color: layer.stroke_color,
            stroke_width: layer.stroke_width,
            corner_radius: layer.corner_radius,
            width: layer.width,
            height: layer.height,
            text: layer.text,
            font_size: layer.font_size,
            font_family: layer.font_family,
            font_weight: layer.font_weight,
            text_align: layer.text_align,
            path_data: layer.path_data,
            path_fill_rule: layer.path_fill_rule,
            fit: layer.fit,
            opacity: layer.opacity,
            transform: layer.transform,
        });
    }
    Ok(())
}

fn scene_render_audio_cues(
    package_root: &Path,
    resources: &BTreeMap<&str, &SceneResource>,
    layer_id: &str,
    cues: Vec<SceneAudioCue>,
) -> Result<Vec<SceneRenderAudioCue>, RendererPlanError> {
    cues.into_iter()
        .enumerate()
        .map(|(index, cue)| {
            let source = scene_render_audio_source(package_root, resources, layer_id, index, &cue)?;
            Ok(SceneRenderAudioCue {
                source,
                playback_mode: cue.playback_mode,
                volume: cue.volume,
                start_silent: cue.start_silent.unwrap_or(false),
            })
        })
        .collect()
}

fn scene_render_audio_source(
    package_root: &Path,
    resources: &BTreeMap<&str, &SceneResource>,
    layer_id: &str,
    cue_index: usize,
    cue: &SceneAudioCue,
) -> Result<PathBuf, RendererPlanError> {
    if let Some(resource_id) = cue.resource.as_deref() {
        let resource = resources.get(resource_id).ok_or_else(|| {
            RendererPlanError::PackageLoad(format!(
                "scene layer {layer_id:?} audio cue {cue_index} references missing resource {resource_id:?}"
            ))
        })?;
        if resource.kind != SceneResourceKind::Audio {
            return Err(RendererPlanError::PackageLoad(format!(
                "scene layer {layer_id:?} audio cue {cue_index} references non-audio resource {resource_id:?}"
            )));
        }
        return Ok(resource.source.join_to(package_root));
    }

    let source = cue.source.as_deref().ok_or_else(|| {
        RendererPlanError::PackageLoad(format!(
            "scene layer {layer_id:?} audio cue {cue_index} has no source"
        ))
    })?;
    let package_path = PackagePath::new(source).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "scene layer {layer_id:?} audio cue {cue_index} source is not a package path: {err}"
        ))
    })?;
    Ok(package_path.join_to(package_root))
}

fn scene_bound_properties(document: &SceneDocument) -> Vec<String> {
    let mut properties = BTreeSet::new();
    properties.extend(
        document
            .property_bindings
            .iter()
            .map(|binding| binding.property.clone()),
    );
    properties.into_iter().collect()
}

fn scene_timeline_animation_count(document: &SceneDocument) -> usize {
    document
        .timelines
        .iter()
        .map(|timeline| timeline.channels.len())
        .sum()
}

fn scene_timeline_animated_layer_count(document: &SceneDocument) -> usize {
    document
        .timelines
        .iter()
        .filter_map(|timeline| timeline.target_node.as_deref())
        .collect::<BTreeSet<_>>()
        .len()
}

fn scene_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: &BTreeMap<String, PropertySpec>,
) -> Option<f64> {
    render_properties
        .and_then(|properties| properties.get(property))
        .and_then(scene_json_property_number)
        .or_else(|| {
            manifest_properties
                .get(property)
                .and_then(scene_manifest_property_default_number)
        })
}

fn scene_runtime_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    scene_controller_property_value(document, time_ms, property)
        .or_else(|| scene_audio_response_property_value(document, time_ms, property))
}

fn scene_controller_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    let property = property.trim();
    if !property.starts_with("scene.controller.") {
        return None;
    }
    document
        .nodes
        .iter()
        .find_map(|node| scene_node_controller_property_value(node, time_ms, property))
}

fn scene_node_controller_property_value(
    node: &SceneNode,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if let Some(controller) = node.properties.get("controller").and_then(Value::as_object)
        && controller
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime == "native")
        && controller
            .get("property")
            .and_then(Value::as_str)
            .is_some_and(|controller_property| controller_property.trim() == property)
    {
        match controller.get("kind").and_then(Value::as_str)? {
            "idle-video-switch" => {
                let inactive_sec = controller
                    .get("mouse_inactive_sec")
                    .and_then(scene_controller_config_number)?;
                let inactive_ms = (inactive_sec.max(0.0) * 1000.0).round();
                return Some(if time_ms as f64 >= inactive_ms {
                    1.0
                } else {
                    0.0
                });
            }
            "click-video-switch" => {
                return Some(0.0);
            }
            _ => return None,
        }
    }
    node.children
        .iter()
        .find_map(|child| scene_node_controller_property_value(child, time_ms, property))
}

fn scene_audio_response_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if document.systems.audio_response != SceneSystemStatus::Ready {
        return None;
    }
    let property = property
        .trim()
        .replace(['-', ' ', '/'], "_")
        .to_ascii_lowercase();
    if property.is_empty() {
        return None;
    }
    let band = if property == "audio"
        || property == "audio_level"
        || property == "audio_response"
        || property.ends_with("_audio")
    {
        "full"
    } else if property.contains("bass") || property.contains("low") {
        "bass"
    } else if property.contains("mid") || property.contains("vocal") {
        "mid"
    } else if property.contains("treble") || property.contains("high") {
        "treble"
    } else if property.contains("spectrum") || property.contains("frequency") {
        "spectrum"
    } else {
        return None;
    };
    let seconds = time_ms as f64 / 1000.0;
    let (frequency, phase, floor, gain) = match band {
        "bass" => (1.25, 0.0, 0.12, 0.88),
        "mid" => (2.5, 0.7, 0.08, 0.78),
        "treble" => (5.0, 1.3, 0.04, 0.72),
        "spectrum" => (
            3.5,
            scene_audio_response_spectrum_phase(&property),
            0.05,
            0.8,
        ),
        _ => (1.75, 0.35, 0.1, 0.82),
    };
    let wave = (seconds.mul_add(frequency * std::f64::consts::TAU, phase)).sin() * 0.5 + 0.5;
    Some((floor + wave.powf(1.35) * gain).clamp(0.0, 1.0))
}

fn scene_audio_response_spectrum_phase(property: &str) -> f64 {
    let bin = property
        .rsplit('_')
        .find_map(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    f64::from(bin % 32) * 0.196_349_540_849_362_07
}

fn scene_controller_config_number(value: &Value) -> Option<f64> {
    scene_json_property_number(value.get("value").unwrap_or(value))
}

fn scene_json_property_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

fn scene_manifest_property_default_number(property: &PropertySpec) -> Option<f64> {
    let number = match property {
        PropertySpec::Bool { default } => {
            if (*default)? {
                1.0
            } else {
                0.0
            }
        }
        PropertySpec::Number { default } | PropertySpec::Range { default, .. } => (*default)?,
        PropertySpec::Choice { .. }
        | PropertySpec::Color { .. }
        | PropertySpec::Text { .. }
        | PropertySpec::File { .. } => return None,
    };
    number.is_finite().then_some(number)
}

fn load_scene_document(path: &Path) -> Result<SceneDocument, RendererPlanError> {
    let contents = fs::read_to_string(path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to read scene document {}: {err}",
            path.display()
        ))
    })?;
    let document: SceneDocument = serde_json::from_str(&contents).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to parse scene document {}: {err}",
            path.display()
        ))
    })?;
    document.validate().map_err(|err| {
        RendererPlanError::PackageLoad(format!("invalid scene document {}: {err}", path.display()))
    })?;
    Ok(document)
}

fn scene_display_plan(
    source_path: Option<&Path>,
    document: &SceneDocument,
    layers: &[SceneRenderLayer],
    fit_override: Option<FitMode>,
    render_target: Option<RenderTargetSize>,
    scene_snapshot_cache: Option<&mut SceneSnapshotCacheContext<'_>>,
) -> Option<SceneDisplayPlan> {
    if let Some(color) =
        scene_direct_display_color(layers, scene_snapshot_size(document, render_target))
    {
        return Some(SceneDisplayPlan::Color { color });
    }
    if let Some(image) = scene_direct_display_image(layers, fit_override) {
        return Some(image);
    }
    if let Some(snapshot) = scene_snapshot_display(
        source_path,
        document,
        layers,
        render_target,
        scene_snapshot_cache,
    ) {
        return Some(snapshot);
    }
    if let Some(layer) = layers.iter().find(|layer| layer.source.is_some()) {
        return Some(SceneDisplayPlan::Image {
            source: layer.source.clone()?,
            fit: effective_fit(layer.fit, fit_override),
            background: scene_background_color(layers).or_else(|| Some("#000000".to_owned())),
        });
    }
    scene_background_color(layers).map(|color| SceneDisplayPlan::Color { color })
}

fn scene_snapshot_display(
    source_path: Option<&Path>,
    document: &SceneDocument,
    layers: &[SceneRenderLayer],
    render_target: Option<RenderTargetSize>,
    scene_snapshot_cache: Option<&mut SceneSnapshotCacheContext<'_>>,
) -> Option<SceneDisplayPlan> {
    if !layers.iter().any(scene_layer_is_snapshot_renderable) {
        return None;
    }
    let context = scene_snapshot_cache?;
    let size = scene_snapshot_size(document, render_target);
    let source = cached_scene_snapshot(source_path, document, layers, size, context)?;
    Some(SceneDisplayPlan::Image {
        source,
        fit: FitMode::Cover,
        background: scene_background_color(layers).or_else(|| Some("#000000".to_owned())),
    })
}

fn scene_snapshot_size(
    document: &SceneDocument,
    render_target: Option<RenderTargetSize>,
) -> RenderTargetSize {
    render_target
        .or_else(|| {
            document.size.map(|size| RenderTargetSize {
                width: size.width,
                height: size.height,
            })
        })
        .unwrap_or(RenderTargetSize {
            width: 1920,
            height: 1080,
        })
}

fn cached_scene_snapshot(
    source_path: Option<&Path>,
    document: &SceneDocument,
    layers: &[SceneRenderLayer],
    size: RenderTargetSize,
    context: &mut SceneSnapshotCacheContext<'_>,
) -> Option<PathBuf> {
    if context.max_entries == 0 {
        return None;
    }
    let cache_path =
        scene_snapshot_cache_path(context.cache_dir, source_path, document, layers, size);
    if is_nonempty_file(&cache_path) {
        context.stats.scene_snapshot_cache_reuses += 1;
        context.protected_files.insert(cache_path.clone());
        mark_scene_snapshot_cache_used(&cache_path);
        return Some(cache_path);
    }

    let svg = scene_snapshot_svg(layers, size);
    if write_scene_snapshot_svg(&cache_path, &svg).is_ok() {
        context.stats.scene_snapshot_cache_generations += 1;
        context.protected_files.insert(cache_path.clone());
        mark_scene_snapshot_cache_used(&cache_path);
        Some(cache_path)
    } else {
        context.stats.scene_snapshot_cache_generation_errors += 1;
        let _ = fs::remove_file(&cache_path);
        let _ = fs::remove_file(scene_snapshot_cache_used_marker(&cache_path));
        None
    }
}

fn scene_snapshot_cache_path(
    cache_dir: &Path,
    source_path: Option<&Path>,
    document: &SceneDocument,
    layers: &[SceneRenderLayer],
    size: RenderTargetSize,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source_path.hash(&mut hasher);
    size.hash(&mut hasher);
    if let Ok(serialized) = serde_json::to_string(document) {
        serialized.hash(&mut hasher);
    }
    if let Ok(serialized) = serde_json::to_string(layers) {
        serialized.hash(&mut hasher);
    }
    if let Some(source_path) = source_path
        && let Ok(metadata) = fs::metadata(source_path)
    {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
        {
            duration.as_secs().hash(&mut hasher);
            duration.subsec_nanos().hash(&mut hasher);
        }
    }

    cache_dir.join("scene-cache").join(format!(
        "{}x{}-{:016x}.svg",
        size.width,
        size.height,
        hasher.finish()
    ))
}

fn write_scene_snapshot_svg(path: &Path, svg: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create scene snapshot cache directory: {err}"))?;
    }
    let temporary_path = path.with_extension("svg.tmp");
    let _ = fs::remove_file(&temporary_path);
    fs::write(&temporary_path, svg)
        .map_err(|err| format!("failed to write scene snapshot: {err}"))?;
    fs::rename(&temporary_path, path)
        .map_err(|err| format!("failed to move scene snapshot into place: {err}"))?;
    Ok(())
}

fn scene_snapshot_svg(layers: &[SceneRenderLayer], size: RenderTargetSize) -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#,
        width = size.width,
        height = size.height,
    );
    for layer in layers.iter().filter(|layer| layer.opacity > 0.0) {
        match layer.kind {
            SceneNodeKind::Image => {
                let Some(source) = &layer.source else {
                    continue;
                };
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><image href="{href}" xlink:href="{href}" x="0" y="0" width="{width}" height="{height}" preserveAspectRatio="{aspect}"/></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    href = xml_attr(&file_uri_for_path(source)),
                    width = size.width,
                    height = size.height,
                    aspect = scene_svg_aspect(layer.fit),
                ));
            }
            SceneNodeKind::Color => {
                let Some(color) = &layer.color else {
                    continue;
                };
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><rect x="0" y="0" width="{width}" height="{height}" fill="{color}"/></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    width = size.width,
                    height = size.height,
                    color = xml_attr(color),
                ));
            }
            SceneNodeKind::Rectangle | SceneNodeKind::AudioResponse => {
                let fill = layer.color.as_deref().unwrap_or("none");
                let width = layer.width.unwrap_or(f64::from(size.width));
                let height = layer.height.unwrap_or(f64::from(size.height));
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><rect x="0" y="0" width="{width}" height="{height}" rx="{radius}" ry="{radius}" fill="{color}"{stroke}/></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    width = svg_number(width),
                    height = svg_number(height),
                    radius = svg_number(layer.corner_radius.unwrap_or(0.0)),
                    color = xml_attr(fill),
                    stroke = scene_svg_stroke(layer),
                ));
            }
            SceneNodeKind::Ellipse => {
                let fill = layer.color.as_deref().unwrap_or("none");
                let width = layer.width.unwrap_or(f64::from(size.width));
                let height = layer.height.unwrap_or(f64::from(size.height));
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="{color}"{stroke}/></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    cx = svg_number(width / 2.0),
                    cy = svg_number(height / 2.0),
                    rx = svg_number(width / 2.0),
                    ry = svg_number(height / 2.0),
                    color = xml_attr(fill),
                    stroke = scene_svg_stroke(layer),
                ));
            }
            SceneNodeKind::Text => {
                let (Some(text), Some(color)) = (&layer.text, &layer.color) else {
                    continue;
                };
                let font_size = layer.font_size.unwrap_or(32.0);
                let width = layer.width.unwrap_or(f64::from(size.width));
                let align = layer.text_align.unwrap_or_default();
                let (x, anchor) = scene_text_anchor(align, width);
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><text x="{x}" y="{y}" fill="{color}" font-size="{font_size}" text-anchor="{anchor}"{font_family}{font_weight}>{text}</text></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    x = svg_number(x),
                    y = svg_number(font_size),
                    color = xml_attr(color),
                    font_size = svg_number(font_size),
                    anchor = anchor,
                    font_family = scene_svg_optional_attr("font-family", layer.font_family.as_deref()),
                    font_weight = scene_svg_optional_attr("font-weight", layer.font_weight.as_deref()),
                    text = xml_text(text),
                ));
            }
            SceneNodeKind::Path => {
                let Some(path_data) = &layer.path_data else {
                    continue;
                };
                let fill = layer.color.as_deref().unwrap_or("none");
                svg.push_str(&format!(
                    r#"<g opacity="{opacity}" transform="{transform}"><path d="{path}" fill="{fill}"{stroke}/></g>"#,
                    opacity = svg_number(layer.opacity),
                    transform = xml_attr(&scene_svg_transform(layer.transform, size)),
                    path = xml_attr(path_data),
                    fill = xml_attr(fill),
                    stroke = scene_svg_stroke(layer),
                ));
            }
            SceneNodeKind::Video
            | SceneNodeKind::Audio
            | SceneNodeKind::Group
            | SceneNodeKind::Shader
            | SceneNodeKind::ParticleEmitter
            | SceneNodeKind::Script
            | SceneNodeKind::Unknown => {}
        }
    }
    svg.push_str("</svg>");
    svg
}

fn scene_text_anchor(align: SceneTextAlign, width: f64) -> (f64, &'static str) {
    match align {
        SceneTextAlign::Start => (0.0, "start"),
        SceneTextAlign::Middle => (width / 2.0, "middle"),
        SceneTextAlign::End => (width, "end"),
    }
}

fn scene_svg_optional_attr(name: &str, value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if value.is_empty() {
        return String::new();
    }
    format!(r#" {name}="{}""#, xml_attr(value))
}

fn scene_svg_stroke(layer: &SceneRenderLayer) -> String {
    let Some(color) = layer.stroke_color.as_deref() else {
        return String::new();
    };
    let width = layer.stroke_width.unwrap_or(1.0);
    if color.is_empty() || !width.is_finite() || width <= 0.0 {
        return String::new();
    }
    format!(
        r#" stroke="{}" stroke-width="{}""#,
        xml_attr(color),
        svg_number(width)
    )
}

fn scene_svg_transform(transform: SceneTransform, size: RenderTargetSize) -> String {
    let anchor_x = transform.anchor_x * f64::from(size.width);
    let anchor_y = transform.anchor_y * f64::from(size.height);
    format!(
        "translate({} {}) translate({} {}) rotate({}) scale({} {}) translate({} {})",
        svg_number(transform.x),
        svg_number(transform.y),
        svg_number(anchor_x),
        svg_number(anchor_y),
        svg_number(transform.rotation_deg),
        svg_number(transform.scale_x),
        svg_number(transform.scale_y),
        svg_number(-anchor_x),
        svg_number(-anchor_y)
    )
}

fn scene_svg_aspect(fit: FitMode) -> &'static str {
    match fit {
        FitMode::Cover => "xMidYMid slice",
        FitMode::Contain | FitMode::Center | FitMode::Tile => "xMidYMid meet",
        FitMode::Stretch => "none",
    }
}

fn file_uri_for_path(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    format!(
        "file://{}",
        percent_encode_file_uri_path(&absolute.to_string_lossy())
    )
}

fn percent_encode_file_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for &byte in path.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte))
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn svg_number(value: f64) -> String {
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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

struct SceneSnapshotCacheContext<'a> {
    cache_dir: &'a Path,
    max_entries: usize,
    stats: &'a mut RenderSyncCacheReport,
    protected_files: &'a mut BTreeSet<PathBuf>,
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

fn cached_static_image_variant(
    source_path: &Path,
    fit: FitMode,
    render_target: Option<RenderTargetSize>,
    source_dimensions: Option<RenderTargetSize>,
    context: &mut StaticImageCacheContext<'_>,
) -> Option<PathBuf> {
    if context.max_entries == 0 || !is_runtime_cacheable_static_image(source_path) {
        return None;
    }
    let render_target = render_target?;
    let source_dimensions = source_dimensions?;
    if !should_generate_static_image_cache_variant(source_dimensions, render_target, fit) {
        return None;
    }

    let cache_path = static_image_cache_path(
        context.cache_dir,
        source_path,
        source_dimensions,
        render_target,
        fit,
    );
    if is_nonempty_file(&cache_path) {
        context.stats.static_image_cache_reuses += 1;
        context.protected_files.insert(cache_path.clone());
        mark_static_image_cache_used(&cache_path);
        return Some(cache_path);
    }

    if generate_static_image_cache_variant(
        context.ffmpeg,
        source_path,
        &cache_path,
        render_target,
        fit,
    )
    .is_ok()
    {
        context.stats.static_image_cache_generations += 1;
        context.protected_files.insert(cache_path.clone());
        mark_static_image_cache_used(&cache_path);
        Some(cache_path)
    } else {
        context.stats.static_image_cache_generation_errors += 1;
        let _ = fs::remove_file(&cache_path);
        let _ = fs::remove_file(static_image_cache_used_marker(&cache_path));
        None
    }
}

fn should_generate_static_image_cache_variant(
    source: RenderTargetSize,
    target: RenderTargetSize,
    fit: FitMode,
) -> bool {
    let Some(cache_target) = static_image_cache_target_size(source, target, fit) else {
        return false;
    };
    source.area() >= cache_target.area().saturating_mul(2)
}

fn static_image_cache_target_size(
    source: RenderTargetSize,
    target: RenderTargetSize,
    fit: FitMode,
) -> Option<RenderTargetSize> {
    match fit {
        FitMode::Cover => source.covers(target).then_some(target),
        FitMode::Contain => contain_downscaled_size(source, target),
        FitMode::Stretch => Some(target),
        FitMode::Tile | FitMode::Center => None,
    }
}

fn contain_downscaled_size(
    source: RenderTargetSize,
    target: RenderTargetSize,
) -> Option<RenderTargetSize> {
    if source.width == 0 || source.height == 0 || target.width == 0 || target.height == 0 {
        return None;
    }

    let source_width = u64::from(source.width);
    let source_height = u64::from(source.height);
    let target_width = u64::from(target.width);
    let target_height = u64::from(target.height);

    let (scale_num, scale_den) = if target_width.saturating_mul(source_height)
        <= target_height.saturating_mul(source_width)
    {
        (target_width, source_width)
    } else {
        (target_height, source_height)
    };
    if scale_num >= scale_den {
        return None;
    }

    let width = ((source_width.saturating_mul(scale_num)) / scale_den)
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    let height = ((source_height.saturating_mul(scale_num)) / scale_den)
        .max(1)
        .min(u64::from(u32::MAX)) as u32;
    Some(RenderTargetSize { width, height })
}

fn static_image_cache_path(
    cache_dir: &Path,
    source_path: &Path,
    source_dimensions: RenderTargetSize,
    render_target: RenderTargetSize,
    fit: FitMode,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source_path.hash(&mut hasher);
    source_dimensions.hash(&mut hasher);
    render_target.hash(&mut hasher);
    fit_cache_name(fit).hash(&mut hasher);
    if let Ok(metadata) = fs::metadata(source_path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
        {
            duration.as_secs().hash(&mut hasher);
            duration.subsec_nanos().hash(&mut hasher);
        }
    }

    cache_dir.join("static-image-cache").join(format!(
        "{}-{}x{}-{}.png",
        fit_cache_name(fit),
        render_target.width,
        render_target.height,
        hasher.finish()
    ))
}

fn fit_cache_name(fit: FitMode) -> &'static str {
    match fit {
        FitMode::Cover => "cover",
        FitMode::Contain => "contain",
        FitMode::Stretch => "stretch",
        FitMode::Tile => "tile",
        FitMode::Center => "center",
    }
}

fn is_runtime_cacheable_static_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif" | "bmp" | "jpeg" | "jpg" | "png" | "webp"
            )
        })
        .unwrap_or(false)
}

fn is_nonempty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn generate_static_image_cache_variant(
    ffmpeg: Option<&Path>,
    source_path: &Path,
    output_path: &Path,
    target: RenderTargetSize,
    fit: FitMode,
) -> Result<(), String> {
    let Some(filter) = static_image_cache_filter(target, fit) else {
        return Err("fit mode is not runtime-cacheable".to_owned());
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create static image cache directory: {err}"))?;
    }
    let temporary_path = output_path.with_extension("png.tmp");
    let _ = fs::remove_file(&temporary_path);

    let executable = ffmpeg.unwrap_or_else(|| Path::new("ffmpeg"));
    let output = Command::new(executable)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source_path)
        .args(["-frames:v", "1", "-an", "-sn", "-vf", &filter])
        .arg(&temporary_path)
        .output()
        .map_err(|err| format!("failed to run {}: {err}", executable.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let reason = if stderr.is_empty() {
            output.status.to_string()
        } else {
            format!("{}: {stderr}", output.status)
        };
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("{} failed: {reason}", executable.display()));
    }
    if !is_nonempty_file(&temporary_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "{} created an empty static image cache file at {}",
            executable.display(),
            temporary_path.display()
        ));
    }

    fs::rename(&temporary_path, output_path)
        .map_err(|err| format!("failed to move static image cache file into place: {err}"))?;
    Ok(())
}

fn static_image_cache_filter(target: RenderTargetSize, fit: FitMode) -> Option<String> {
    match fit {
        FitMode::Cover => Some(format!(
            "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}",
            target.width, target.height, target.width, target.height
        )),
        FitMode::Contain => Some(format!(
            "scale={}:{}:force_original_aspect_ratio=decrease",
            target.width, target.height
        )),
        FitMode::Stretch => Some(format!("scale={}:{}", target.width, target.height)),
        FitMode::Tile | FitMode::Center => None,
    }
}

fn automatic_variant_source(
    package: &WallpaperPackage,
    render_target: Option<RenderTargetSize>,
) -> Option<&PackagePath> {
    let render_target = render_target?;
    let target_area = render_target.area();
    package
        .manifest
        .variants
        .iter()
        .filter_map(|variant| variant_dimensions(variant).map(|dimensions| (variant, dimensions)))
        .filter(|(_, dimensions)| dimensions.covers(render_target))
        .min_by_key(|(_, dimensions)| {
            (
                dimensions.area().saturating_sub(target_area),
                dimensions.aspect_delta(render_target),
            )
        })
        .map(|(variant, _)| &variant.source)
}

fn variant_dimensions(variant: &Variant) -> Option<RenderTargetSize> {
    Some(RenderTargetSize {
        width: variant.width?,
        height: variant.height?,
    })
}

fn render_target_size(
    compositor: Option<CompositorKind>,
    output: Option<&DesktopOutput>,
) -> Option<RenderTargetSize> {
    let output = output?;
    let width = output.width?;
    let height = output.height?;
    if matches!(compositor, Some(CompositorKind::Hyprland)) {
        return Some(RenderTargetSize { width, height });
    }

    let scale = if output.scale.is_finite() && output.scale > 0.0 {
        output.scale
    } else {
        1.0
    };
    Some(RenderTargetSize {
        width: scaled_dimension(width, scale),
        height: scaled_dimension(height, scale),
    })
}

fn scaled_dimension(value: u32, scale: f32) -> u32 {
    ((f64::from(value) * f64::from(scale))
        .round()
        .clamp(1.0, f64::from(u32::MAX))) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RenderTargetSize {
    width: u32,
    height: u32,
}

impl RenderTargetSize {
    fn covers(self, target: Self) -> bool {
        self.width >= target.width && self.height >= target.height
    }

    fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn aspect_delta(self, target: Self) -> u64 {
        let left = u64::from(self.width) * u64::from(target.height);
        let right = u64::from(target.width) * u64::from(self.height);
        left.abs_diff(right)
    }
}

pub fn static_wallpaper_plan(
    output_name: impl Into<String>,
    package: &WallpaperPackage,
    output_state: &OutputState,
) -> Result<Option<StaticWallpaperPlan>, RendererPlanError> {
    let Some(assignment) = &output_state.wallpaper else {
        return Ok(None);
    };
    let WallpaperEntry::StaticImage {
        source,
        fit,
        background,
        ..
    } = &package.manifest.entry
    else {
        return Err(RendererPlanError::UnsupportedEntry(
            package.manifest.entry.kind().as_str(),
        ));
    };
    let variant_source = explicit_variant_source(package, assignment.variant.as_deref())?;

    Ok(Some(StaticWallpaperPlan {
        output_name: output_name.into(),
        source: variant_source.unwrap_or(source).join_to(&package.root),
        fit: *fit,
        background: background.clone(),
    }))
}

fn effective_max_fps(manifest_max_fps: Option<u32>, policy_max_fps: Option<u32>) -> Option<u32> {
    match (manifest_max_fps, policy_max_fps) {
        (Some(manifest), Some(policy)) => Some(manifest.min(policy)),
        (Some(manifest), None) => Some(manifest),
        (None, Some(policy)) => Some(policy),
        (None, None) => None,
    }
}

fn effective_muted(entry_muted: bool, runtime_allow_audio: bool) -> bool {
    entry_muted || !runtime_allow_audio
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RendererPlanError {
    UnsupportedEntry(&'static str),
    MissingAssignment,
    MissingVariant(String),
    PlaylistNoMatch,
    PackageLoad(String),
}

impl fmt::Display for RendererPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEntry(kind) => write!(f, "{kind} entries are not supported here"),
            Self::MissingAssignment => f.write_str("wallpaper assignment is missing"),
            Self::MissingVariant(variant) => {
                write!(f, "wallpaper variant {variant:?} was not found")
            }
            Self::PlaylistNoMatch => f.write_str("playlist did not match any item"),
            Self::PackageLoad(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RendererPlanError {}

fn load_assigned_package(
    assignment: &WallpaperAssignment,
    cache_dir: &Path,
) -> Result<WallpaperPackage, RendererPlanError> {
    let mut stats = RenderSyncCacheReport::default();
    let mut protected_archive_dirs = BTreeSet::new();
    load_assigned_package_tracked(
        assignment,
        cache_dir,
        &mut stats,
        &mut protected_archive_dirs,
    )
}

fn load_assigned_package_tracked(
    assignment: &WallpaperAssignment,
    cache_dir: &Path,
    stats: &mut RenderSyncCacheReport,
    protected_archive_dirs: &mut BTreeSet<PathBuf>,
) -> Result<WallpaperPackage, RendererPlanError> {
    let path = Path::new(&assignment.path);
    if path.is_dir() || path.extension().and_then(|extension| extension.to_str()) == Some("gwpdir")
    {
        return crate::core::load_gwpdir(path)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()));
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("gwp") {
        let extract_dir = archive_extract_dir(cache_dir, path);
        protected_archive_dirs.insert(extract_dir.clone());
        if extract_dir.join(crate::core::MANIFEST_FILE).exists()
            || extract_dir.join(crate::core::MANIFEST_TOML_FILE).exists()
        {
            stats.archive_cache_reuses += 1;
            let package = crate::core::load_gwpdir(&extract_dir)
                .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
            mark_archive_cache_used(&extract_dir);
            return Ok(package);
        }
        stats.archive_cache_extractions += 1;
        fs::create_dir_all(
            extract_dir
                .parent()
                .ok_or_else(|| RendererPlanError::PackageLoad("invalid cache path".to_owned()))?,
        )
        .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
        let package = crate::core::load_gwp(path, &extract_dir)
            .map_err(|err| RendererPlanError::PackageLoad(err.to_string()))?;
        mark_archive_cache_used(&extract_dir);
        Ok(package)
    } else {
        Err(RendererPlanError::PackageLoad(format!(
            "unsupported wallpaper path {}",
            path.display()
        )))
    }
}

struct RenderPackageCache<'a> {
    cache_dir: &'a Path,
    max_entries: usize,
    max_retained_unique_resource_bytes: u64,
    packages: BTreeMap<String, Result<Rc<WallpaperPackage>, RendererPlanError>>,
    package_order: VecDeque<String>,
    protected_archive_dirs: BTreeSet<PathBuf>,
    protected_static_cache_files: BTreeSet<PathBuf>,
    protected_scene_snapshot_files: BTreeSet<PathBuf>,
    stats: RenderSyncCacheReport,
}

impl<'a> RenderPackageCache<'a> {
    fn new(
        cache_dir: &'a Path,
        max_entries: usize,
        max_retained_unique_resource_bytes: u64,
    ) -> Self {
        Self {
            cache_dir,
            max_entries,
            max_retained_unique_resource_bytes,
            packages: BTreeMap::new(),
            package_order: VecDeque::new(),
            protected_archive_dirs: BTreeSet::new(),
            protected_static_cache_files: BTreeSet::new(),
            protected_scene_snapshot_files: BTreeSet::new(),
            stats: RenderSyncCacheReport::default(),
        }
    }

    fn package(
        &mut self,
        assignment: &WallpaperAssignment,
    ) -> Result<Rc<WallpaperPackage>, RendererPlanError> {
        if let Some(package) = self.packages.get(&assignment.path) {
            self.stats.package_cache_hits += 1;
            return package.clone();
        }

        self.stats.package_cache_misses += 1;
        let package = load_assigned_package_tracked(
            assignment,
            self.cache_dir,
            &mut self.stats,
            &mut self.protected_archive_dirs,
        )
        .map(Rc::new);
        if self.should_retain_packages() {
            self.prune_for_insert();
            self.packages
                .insert(assignment.path.clone(), package.clone());
            self.package_order.push_back(assignment.path.clone());
            self.prune_to_resource_limit();
        }
        package
    }

    fn should_retain_packages(&self) -> bool {
        self.max_entries > 0 && self.max_retained_unique_resource_bytes > 0
    }

    fn prune_for_insert(&mut self) {
        while self.packages.len() >= self.max_entries {
            let Some(key) = self.package_order.pop_front() else {
                break;
            };
            if self.packages.remove(&key).is_some() {
                self.stats.package_cache_evictions += 1;
            }
        }
    }

    fn prune_to_resource_limit(&mut self) {
        self.update_retained_resource_footprint();
        while self.stats.package_cache_retained_unique_resource_bytes
            > self.max_retained_unique_resource_bytes
        {
            let Some(key) = self.package_order.pop_front() else {
                break;
            };
            if self.packages.remove(&key).is_some() {
                self.stats.package_cache_evictions += 1;
                self.update_retained_resource_footprint();
            }
        }
    }

    fn finish(mut self, cache_config: CacheConfig) -> RenderSyncCacheReport {
        self.update_retained_resource_footprint();
        let prune = prune_render_cache(
            self.cache_dir,
            cache_config.render_cache_max_entries,
            &self.protected_archive_dirs,
        );
        let static_image_prune = prune_static_image_cache(
            self.cache_dir,
            cache_config.static_image_cache_max_entries,
            cache_config.static_image_cache_max_bytes,
            &self.protected_static_cache_files,
        );
        let scene_snapshot_prune = prune_scene_snapshot_cache(
            self.cache_dir,
            cache_config.static_image_cache_max_entries,
            cache_config.static_image_cache_max_bytes,
            &self.protected_scene_snapshot_files,
        );
        self.stats.package_cache_entries = self.packages.len();
        self.stats.package_cache_max_entries = cache_config.package_cache_max_entries;
        self.stats.package_cache_max_retained_unique_resource_bytes =
            cache_config.package_cache_max_retained_unique_resource_bytes;
        self.stats.archive_cache_entries = prune.entries_after;
        self.stats.archive_cache_max_entries = cache_config.render_cache_max_entries;
        self.stats.archive_cache_evictions = prune.evictions;
        self.stats.archive_cache_eviction_errors = prune.errors;
        self.stats.static_image_cache_entries = static_image_prune.entries_after;
        self.stats.static_image_cache_max_entries = cache_config.static_image_cache_max_entries;
        self.stats.static_image_cache_bytes = static_image_prune.bytes_after;
        self.stats.static_image_cache_max_bytes = cache_config.static_image_cache_max_bytes;
        self.stats.static_image_cache_evictions = static_image_prune.evictions;
        self.stats.static_image_cache_eviction_errors = static_image_prune.errors;
        self.stats.scene_snapshot_cache_entries = scene_snapshot_prune.entries_after;
        self.stats.scene_snapshot_cache_max_entries = cache_config.static_image_cache_max_entries;
        self.stats.scene_snapshot_cache_bytes = scene_snapshot_prune.bytes_after;
        self.stats.scene_snapshot_cache_max_bytes = cache_config.static_image_cache_max_bytes;
        self.stats.scene_snapshot_cache_evictions = scene_snapshot_prune.evictions;
        self.stats.scene_snapshot_cache_eviction_errors = scene_snapshot_prune.errors;
        self.stats
    }

    fn update_retained_resource_footprint(&mut self) {
        let mut resource_references = 0;
        let mut resource_reference_bytes = 0;
        let mut unique_resources = BTreeSet::new();
        let mut preview_resource_references = 0;
        let mut preview_resource_reference_bytes = 0;
        let mut unique_preview_resources = BTreeSet::new();

        for package in self
            .packages
            .values()
            .filter_map(|package| package.as_ref().ok())
        {
            for package_path in manifest_preview_paths(&package.manifest) {
                let path = package_path.join_to(&package.root);
                preview_resource_references += 1;
                preview_resource_reference_bytes += source_tree_size(&path);
                unique_preview_resources.insert(path);
            }
            for package_path in package_resource_paths(package) {
                let path = package_path.join_to(&package.root);
                resource_references += 1;
                resource_reference_bytes += source_tree_size(&path);
                unique_resources.insert(path);
            }
        }

        self.stats.package_cache_retained_resource_references = resource_references;
        self.stats.package_cache_retained_unique_resources = unique_resources.len();
        self.stats.package_cache_retained_resource_bytes = resource_reference_bytes;
        self.stats.package_cache_retained_unique_resource_bytes = unique_resources
            .iter()
            .map(|path| source_tree_size(path))
            .sum();
        self.stats
            .package_cache_retained_preview_resource_references = preview_resource_references;
        self.stats.package_cache_retained_unique_preview_resources = unique_preview_resources.len();
        self.stats.package_cache_retained_preview_resource_bytes = preview_resource_reference_bytes;
        self.stats
            .package_cache_retained_unique_preview_resource_bytes = unique_preview_resources
            .iter()
            .map(|path| source_tree_size(path))
            .sum();
    }
}

fn manifest_preview_paths(manifest: &Manifest) -> Vec<PackagePath> {
    let mut paths = Vec::new();
    if let Some(path) = &manifest.preview.thumbnail {
        paths.push(path.clone());
    }
    if let Some(path) = &manifest.preview.poster {
        paths.push(path.clone());
    }
    paths
}

fn package_resource_paths(package: &WallpaperPackage) -> Vec<PackagePath> {
    let Ok(mut paths) = package.manifest.referenced_paths() else {
        return Vec::new();
    };
    if let WallpaperEntry::Scene { source, .. } = &package.manifest.entry {
        let path = source.join_to(&package.root);
        if let Ok(contents) = fs::read_to_string(&path)
            && let Ok(document) = serde_json::from_str::<SceneDocument>(&contents)
            && document.validate().is_ok()
        {
            paths.extend(document.referenced_paths());
        }
    }
    paths
}

fn archive_extract_dir(cache_dir: &Path, archive_path: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    archive_path.hash(&mut hasher);
    let file_name = archive_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("wallpaper")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    cache_dir
        .join("render-cache")
        .join(format!("{}-{:016x}.gwpdir", file_name, hasher.finish()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RenderCachePruneReport {
    entries_after: usize,
    bytes_after: u64,
    evictions: u64,
    errors: u64,
}

fn prune_render_cache(
    cache_dir: &Path,
    max_entries: usize,
    protected_archive_dirs: &BTreeSet<PathBuf>,
) -> RenderCachePruneReport {
    let render_cache_dir = cache_dir.join("render-cache");
    let Ok(mut entries) = render_cache_entries(&render_cache_dir) else {
        return RenderCachePruneReport::default();
    };
    let entries_before = entries.len();
    let remove_count = entries_before.saturating_sub(max_entries);
    if remove_count == 0 {
        return RenderCachePruneReport {
            entries_after: entries_before,
            bytes_after: 0,
            evictions: 0,
            errors: 0,
        };
    }

    entries.sort_by_key(|entry| (entry.last_used, entry.path.clone()));
    let mut evictions = 0;
    let mut errors = 0;
    for entry in entries
        .iter()
        .filter(|entry| !protected_archive_dirs.contains(&entry.path))
        .take(remove_count)
    {
        match fs::remove_dir_all(&entry.path) {
            Ok(()) => evictions += 1,
            Err(_) => errors += 1,
        }
    }

    let entries_after = render_cache_entries(&render_cache_dir)
        .map(|entries| entries.len())
        .unwrap_or_else(|_| entries_before.saturating_sub(evictions as usize));
    RenderCachePruneReport {
        entries_after,
        bytes_after: 0,
        evictions,
        errors,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderCacheEntry {
    path: PathBuf,
    last_used: SystemTime,
    size_bytes: u64,
}

fn render_cache_entries(render_cache_dir: &Path) -> Result<Vec<RenderCacheEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = match fs::read_dir(render_cache_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(err) => return Err(err),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !is_archive_cache_dir(&path, &entry.file_type()?) {
            continue;
        }
        entries.push(RenderCacheEntry {
            last_used: archive_cache_last_used(&path),
            path,
            size_bytes: 0,
        });
    }
    Ok(entries)
}

fn is_archive_cache_dir(path: &Path, file_type: &fs::FileType) -> bool {
    file_type.is_dir()
        && path.extension().and_then(|extension| extension.to_str()) == Some("gwpdir")
}

fn archive_cache_last_used(path: &Path) -> SystemTime {
    fs::metadata(path.join(".gilder-cache-used"))
        .or_else(|_| fs::metadata(path))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn mark_archive_cache_used(extract_dir: &Path) {
    let _ = fs::write(extract_dir.join(".gilder-cache-used"), b"");
}

fn prune_static_image_cache(
    cache_dir: &Path,
    max_entries: usize,
    max_bytes: u64,
    protected_files: &BTreeSet<PathBuf>,
) -> RenderCachePruneReport {
    let static_cache_dir = cache_dir.join("static-image-cache");
    let Ok(mut entries) = static_image_cache_entries(&static_cache_dir) else {
        return RenderCachePruneReport::default();
    };
    entries.sort_by_key(|entry| (entry.last_used, entry.path.clone()));
    let mut evictions = 0;
    let mut errors = 0;
    let mut retained_entries = entries.len();
    let mut retained_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
    let mut removable_index = 0;
    while retained_entries > max_entries || (max_bytes > 0 && retained_bytes > max_bytes) {
        let Some(entry) = entries
            .iter()
            .skip(removable_index)
            .find(|entry| !protected_files.contains(&entry.path))
        else {
            break;
        };
        removable_index = entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
            .map(|index| index + 1)
            .unwrap_or(entries.len());
        let marker = static_image_cache_used_marker(&entry.path);
        match fs::remove_file(&entry.path) {
            Ok(()) => {
                evictions += 1;
                retained_entries = retained_entries.saturating_sub(1);
                retained_bytes = retained_bytes.saturating_sub(entry.size_bytes);
                let _ = fs::remove_file(marker);
            }
            Err(_) => errors += 1,
        }
    }

    let (entries_after, bytes_after) = static_image_cache_entries(&static_cache_dir)
        .map(|entries| {
            (
                entries.len(),
                entries.iter().map(|entry| entry.size_bytes).sum::<u64>(),
            )
        })
        .unwrap_or((retained_entries, retained_bytes));
    RenderCachePruneReport {
        entries_after,
        bytes_after,
        evictions,
        errors,
    }
}

fn static_image_cache_entries(
    static_cache_dir: &Path,
) -> Result<Vec<RenderCacheEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = match fs::read_dir(static_cache_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(err) => return Err(err),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !is_static_image_cache_file(&path, &entry.file_type()?) {
            continue;
        }
        entries.push(RenderCacheEntry {
            last_used: static_image_cache_last_used(&path),
            size_bytes: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            path,
        });
    }
    Ok(entries)
}

fn is_static_image_cache_file(path: &Path, file_type: &fs::FileType) -> bool {
    file_type.is_file() && path.extension().and_then(|extension| extension.to_str()) == Some("png")
}

fn static_image_cache_used_marker(path: &Path) -> PathBuf {
    path.with_extension("png.used")
}

fn static_image_cache_last_used(path: &Path) -> SystemTime {
    fs::metadata(static_image_cache_used_marker(path))
        .or_else(|_| fs::metadata(path))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn mark_static_image_cache_used(path: &Path) {
    let _ = fs::write(static_image_cache_used_marker(path), b"");
}

fn prune_scene_snapshot_cache(
    cache_dir: &Path,
    max_entries: usize,
    max_bytes: u64,
    protected_files: &BTreeSet<PathBuf>,
) -> RenderCachePruneReport {
    let snapshot_cache_dir = cache_dir.join("scene-cache");
    let Ok(mut entries) = scene_snapshot_cache_entries(&snapshot_cache_dir) else {
        return RenderCachePruneReport::default();
    };
    entries.sort_by_key(|entry| (entry.last_used, entry.path.clone()));
    let mut evictions = 0;
    let mut errors = 0;
    let mut retained_entries = entries.len();
    let mut retained_bytes = entries.iter().map(|entry| entry.size_bytes).sum::<u64>();
    let mut removable_index = 0;
    while retained_entries > max_entries || (max_bytes > 0 && retained_bytes > max_bytes) {
        let Some(entry) = entries
            .iter()
            .skip(removable_index)
            .find(|entry| !protected_files.contains(&entry.path))
        else {
            break;
        };
        removable_index = entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
            .map(|index| index + 1)
            .unwrap_or(entries.len());
        let marker = scene_snapshot_cache_used_marker(&entry.path);
        match fs::remove_file(&entry.path) {
            Ok(()) => {
                evictions += 1;
                retained_entries = retained_entries.saturating_sub(1);
                retained_bytes = retained_bytes.saturating_sub(entry.size_bytes);
                let _ = fs::remove_file(marker);
            }
            Err(_) => errors += 1,
        }
    }

    let (entries_after, bytes_after) = scene_snapshot_cache_entries(&snapshot_cache_dir)
        .map(|entries| {
            (
                entries.len(),
                entries.iter().map(|entry| entry.size_bytes).sum::<u64>(),
            )
        })
        .unwrap_or((retained_entries, retained_bytes));
    RenderCachePruneReport {
        entries_after,
        bytes_after,
        evictions,
        errors,
    }
}

fn scene_snapshot_cache_entries(
    snapshot_cache_dir: &Path,
) -> Result<Vec<RenderCacheEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = match fs::read_dir(snapshot_cache_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
        Err(err) => return Err(err),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !is_scene_snapshot_cache_file(&path, &entry.file_type()?) {
            continue;
        }
        entries.push(RenderCacheEntry {
            last_used: scene_snapshot_cache_last_used(&path),
            size_bytes: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            path,
        });
    }
    Ok(entries)
}

fn is_scene_snapshot_cache_file(path: &Path, file_type: &fs::FileType) -> bool {
    file_type.is_file() && path.extension().and_then(|extension| extension.to_str()) == Some("svg")
}

fn scene_snapshot_cache_used_marker(path: &Path) -> PathBuf {
    path.with_extension("svg.used")
}

fn scene_snapshot_cache_last_used(path: &Path) -> SystemTime {
    fs::metadata(scene_snapshot_cache_used_marker(path))
        .or_else(|_| fs::metadata(path))
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH)
}

fn mark_scene_snapshot_cache_used(path: &Path) {
    let _ = fs::write(scene_snapshot_cache_used_marker(path), b"");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DynamicPausePolicy, GilderConfig, OutputConfig, OutputPerformanceConfig, PerformanceConfig,
        PowerPolicy, VideoDecoderPolicy,
    };
    use crate::core::{SceneSystemStatus, pack_gwp};
    use crate::desktop::{DesktopCursorParallax, DesktopOutput, PowerState};
    use crate::policy::{DecisionReason, PerformanceDecision, RenderMode};
    use crate::state::{OutputState, WallpaperAssignment};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn playlist_test_clock(
        local_minute_of_day: u16,
        local_weekday: PlaylistWeekday,
    ) -> PlaylistClockKey {
        PlaylistClockKey {
            local_minute_of_day,
            local_weekday,
        }
    }

    #[test]
    fn builds_static_wallpaper_plan_from_package() {
        let package = crate::core::load_gwpdir("examples/wallpapers/static-demo.gwpdir").unwrap();
        let output_state = OutputState {
            wallpaper: Some(WallpaperAssignment {
                path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
                variant: None,
            }),
            ..OutputState::default()
        };

        let plan = static_wallpaper_plan("eDP-1", &package, &output_state)
            .unwrap()
            .unwrap();
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.fit, FitMode::Cover);
        assert_eq!(plan.background.as_deref(), Some("#101418"));
        assert!(plan.source.ends_with("assets/wallpaper.svg"));
    }

    #[test]
    fn builds_slideshow_plan_from_example_package() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/slideshow-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert!(sync.errors.is_empty());
        let plan = &sync.slideshow_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.sources.len(), 2);
        assert!(plan.sources[0].ends_with("assets/slide-a.svg"));
        assert!(plan.sources[1].ends_with("assets/slide-b.svg"));
        assert_eq!(plan.interval_ms, 3_000);
        assert_eq!(plan.fit, FitMode::Cover);
    }

    #[test]
    fn playlist_selects_wallpaper_from_power_condition() {
        let test_dir = TestDir::new("gilder-playlist-power-plan");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_minimal_playlist_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });

        let battery_sync = static_render_sync_plan(
            &DesktopSnapshot {
                power: PowerState::Battery,
                outputs: vec![DesktopOutput::virtual_output("eDP-1")],
                ..DesktopSnapshot::default()
            },
            &state,
            test_dir.path.join("cache"),
        );

        assert!(battery_sync.errors.is_empty());
        assert_eq!(battery_sync.plans.len(), 1);
        assert!(battery_sync.video_plans.is_empty());
        assert!(battery_sync.plans[0].source.ends_with("assets/battery.svg"));

        let ac_sync = static_render_sync_plan(
            &DesktopSnapshot {
                power: PowerState::Ac,
                outputs: vec![DesktopOutput::virtual_output("eDP-1")],
                ..DesktopSnapshot::default()
            },
            &state,
            test_dir.path.join("cache"),
        );

        assert!(ac_sync.errors.is_empty());
        assert!(ac_sync.plans.is_empty());
        assert_eq!(ac_sync.video_plans.len(), 1);
        assert!(ac_sync.video_plans[0].source.ends_with("assets/loop.webm"));
    }

    #[test]
    fn playlist_selects_wallpaper_from_local_time_condition() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "day",
                    "conditions": {
                        "local_time": {
                            "start": "08:00",
                            "end": "18:00"
                        }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/day.svg"
                    }
                },
                {
                    "id": "night",
                    "conditions": {
                        "local_time": {
                            "start": "18:00",
                            "end": "08:00"
                        }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/night.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, .. } = &entry else {
            panic!("expected playlist entry");
        };
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");

        let day_context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(10 * 60 + 30, PlaylistWeekday::Monday),
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&day_context))
                .map(|item| item.id.as_str()),
            Some("day")
        );

        let night_context = PlaylistRenderContext {
            local_clock: playlist_test_clock(22 * 60 + 30, PlaylistWeekday::Monday),
            ..day_context
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&night_context))
                .map(|item| item.id.as_str()),
            Some("night")
        );
    }

    #[test]
    fn playlist_selects_wallpaper_from_weekday_condition() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "workday",
                    "conditions": {
                        "weekdays": ["monday", "tuesday", "wednesday", "thursday", "friday"]
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/workday.svg"
                    }
                },
                {
                    "id": "weekend",
                    "conditions": {
                        "weekdays": ["sat", "sun"]
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weekend.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, .. } = &entry else {
            panic!("expected playlist entry");
        };
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");
        let monday_context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(10 * 60, PlaylistWeekday::Monday),
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&monday_context))
                .map(|item| item.id.as_str()),
            Some("workday")
        );

        let sunday_context = PlaylistRenderContext {
            local_clock: playlist_test_clock(10 * 60, PlaylistWeekday::Sunday),
            ..monday_context
        };
        assert_eq!(
            select_playlist_item(items, PlaylistSelection::FirstMatch, Some(&sunday_context))
                .map(|item| item.id.as_str()),
            Some("weekend")
        );
    }

    #[test]
    fn computes_gregorian_weekdays_for_playlist_clock() {
        assert_eq!(gregorian_weekday(2026, 6, 19), PlaylistWeekday::Friday);
        assert_eq!(gregorian_weekday(2024, 2, 29), PlaylistWeekday::Thursday);
        assert_eq!(gregorian_weekday(1970, 1, 1), PlaylistWeekday::Thursday);
    }

    #[test]
    fn playlist_weighted_random_selection_is_stable_and_weighted() {
        let entry: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
                {
                    "id": "rare",
                    "weight": 1,
                    "entry": {
                        "type": "static-image",
                        "source": "assets/rare.svg"
                    }
                },
                {
                    "id": "common",
                    "weight": 9,
                    "entry": {
                        "type": "static-image",
                        "source": "assets/common.svg"
                    }
                }
            ]
        }))
        .unwrap();
        let WallpaperEntry::Playlist { items, selection } = &entry else {
            panic!("expected playlist entry");
        };
        assert_eq!(*selection, PlaylistSelection::WeightedRandom);

        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let output = desktop.output("eDP-1");
        let context = PlaylistRenderContext {
            desktop: &desktop,
            output_name: "eDP-1",
            output,
            local_clock: playlist_test_clock(11 * 60 + 7, PlaylistWeekday::Monday),
        };
        let first =
            select_playlist_item(items, *selection, Some(&context)).map(|item| item.id.as_str());
        let second =
            select_playlist_item(items, *selection, Some(&context)).map(|item| item.id.as_str());
        assert_eq!(first, second);

        let mut rare_count = 0;
        let mut common_count = 0;
        for local_minute_of_day in 0..(24 * 60) {
            let context = PlaylistRenderContext {
                local_clock: playlist_test_clock(local_minute_of_day, PlaylistWeekday::Monday),
                ..context
            };
            match select_playlist_item(items, *selection, Some(&context))
                .map(|item| item.id.as_str())
            {
                Some("rare") => rare_count += 1,
                Some("common") => common_count += 1,
                other => panic!("unexpected weighted playlist item {other:?}"),
            }
        }

        assert!(common_count > rare_count * 3);
    }

    #[test]
    fn playlist_clock_dependency_tracks_time_sensitive_selection() {
        let power_only: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "battery",
                    "conditions": { "power": "battery" },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/battery.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&power_only),
            PlaylistClockDependency::None
        );

        let local_time: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "day",
                    "conditions": {
                        "local_time": { "start": "08:00", "end": "18:00" }
                    },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/day.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&local_time),
            PlaylistClockDependency::Minute
        );

        let weekdays: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "items": [
                {
                    "id": "weekday",
                    "conditions": { "weekdays": ["monday"] },
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weekday.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&weekdays),
            PlaylistClockDependency::Weekday
        );

        let weighted: WallpaperEntry = serde_json::from_value(json!({
            "type": "playlist",
            "selection": "weighted-random",
            "items": [
                {
                    "id": "weighted",
                    "entry": {
                        "type": "static-image",
                        "source": "assets/weighted.svg"
                    }
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            playlist_entry_clock_dependency(&weighted),
            PlaylistClockDependency::MinuteAndWeekday
        );
    }

    #[test]
    fn playlist_clock_cache_key_uses_only_required_fields() {
        let clock = playlist_test_clock(11 * 60 + 7, PlaylistWeekday::Friday);

        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::None, clock),
            None
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::Minute, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: Some(11 * 60 + 7),
                local_weekday: None,
            })
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::Weekday, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: None,
                local_weekday: Some(PlaylistWeekday::Friday),
            })
        );
        assert_eq!(
            playlist_clock_cache_key(PlaylistClockDependency::MinuteAndWeekday, clock),
            Some(PlaylistClockCacheKey {
                local_minute_of_day: Some(11 * 60 + 7),
                local_weekday: Some(PlaylistWeekday::Friday),
            })
        );
    }

    #[test]
    fn playlist_static_selection_survives_battery_pause_dynamic_policy() {
        let test_dir = TestDir::new("gilder-playlist-battery-static");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_minimal_playlist_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/battery.svg"));
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
    }

    #[test]
    fn playlist_no_match_reports_error_under_pause_dynamic_policy() {
        let test_dir = TestDir::new("gilder-playlist-no-match");
        let package_dir = test_dir.path.join("playlist-demo.gwpdir");
        write_playlist_no_match_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.removals.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(sync.errors[0].message, "playlist did not match any item");
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn static_wallpaper_plan_uses_requested_variant_source() {
        let test_dir = TestDir::new("gilder-static-variant-plan");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("wide".to_owned()),
        };

        let plan =
            static_wallpaper_plan_for_assignment("eDP-1", &assignment, test_dir.path.join("cache"))
                .unwrap();

        assert!(plan.source.ends_with("assets/wide.svg"));
    }

    #[test]
    fn missing_requested_variant_reports_error() {
        let test_dir = TestDir::new("gilder-missing-variant-plan");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("missing".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(
            sync.errors[0].message,
            "wallpaper variant \"missing\" was not found"
        );
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn auto_selects_smallest_variant_covering_scaled_output() {
        let test_dir = TestDir::new("gilder-auto-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(960),
                height: Some(540),
                scale: 2.0,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/hd.svg"));
    }

    #[test]
    fn explicit_variant_overrides_automatic_variant_selection() {
        let test_dir = TestDir::new("gilder-explicit-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("uhd".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(1920),
                height: Some(1080),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/uhd.svg"));
    }

    #[test]
    fn automatic_variant_keeps_entry_source_when_no_variant_covers_output() {
        let test_dir = TestDir::new("gilder-no-cover-static-variant-plan");
        let package_dir = test_dir.path.join("static-auto-variant.gwpdir");
        write_static_auto_variant_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(5000),
                height: Some(3000),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("assets/wallpaper.svg"));
    }

    #[test]
    fn runtime_static_image_cache_generates_and_reuses_downscaled_source() {
        let test_dir = TestDir::new("gilder-static-runtime-cache");
        let package_dir = test_dir.path.join("static-large.gwpdir");
        let cache_dir = test_dir.path.join("cache");
        let ffmpeg = test_dir.path.join("ffmpeg");
        write_static_large_gwpdir(&package_dir);
        write_executable_script(
            &ffmpeg,
            r#"#!/bin/sh
out=""
for arg in "$@"; do
  out="$arg"
done
printf 'cached-static' > "$out"
exit 0
"#,
        );
        let package = crate::core::load_gwpdir(&package_dir).unwrap();
        let performance = active_performance_decision();
        let mut stats = RenderSyncCacheReport::default();
        let mut protected = BTreeSet::new();

        let first_source = {
            let mut context = StaticImageCacheContext {
                cache_dir: &cache_dir,
                max_entries: 8,
                stats: &mut stats,
                protected_files: &mut protected,
                ffmpeg: Some(&ffmpeg),
            };
            let plan = wallpaper_plan_with_target(
                "eDP-1",
                &package,
                &performance,
                VideoDecoderPolicy::default(),
                None,
                None,
                Some(RenderTargetSize {
                    width: 1920,
                    height: 1080,
                }),
                None,
                None,
                false,
                Some(&mut context),
                None,
            )
            .unwrap();
            match plan {
                WallpaperRenderPlan::StaticImage(plan) => plan.source,
                _ => panic!("expected static image plan"),
            }
        };

        assert!(first_source.starts_with(cache_dir.join("static-image-cache")));
        assert_eq!(fs::read(&first_source).unwrap(), b"cached-static");
        assert_eq!(stats.static_image_cache_generations, 1);
        assert_eq!(stats.static_image_cache_reuses, 0);
        assert_eq!(stats.static_image_cache_generation_errors, 0);

        let second_source = {
            let mut context = StaticImageCacheContext {
                cache_dir: &cache_dir,
                max_entries: 8,
                stats: &mut stats,
                protected_files: &mut protected,
                ffmpeg: Some(&ffmpeg),
            };
            let plan = wallpaper_plan_with_target(
                "eDP-1",
                &package,
                &performance,
                VideoDecoderPolicy::default(),
                None,
                None,
                Some(RenderTargetSize {
                    width: 1920,
                    height: 1080,
                }),
                None,
                None,
                false,
                Some(&mut context),
                None,
            )
            .unwrap();
            match plan {
                WallpaperRenderPlan::StaticImage(plan) => plan.source,
                _ => panic!("expected static image plan"),
            }
        };

        assert_eq!(second_source, first_source);
        assert_eq!(stats.static_image_cache_generations, 1);
        assert_eq!(stats.static_image_cache_reuses, 1);
        assert!(protected.contains(&first_source));
    }

    #[test]
    fn static_image_cache_accepts_tall_contain_sources() {
        assert!(should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 1200,
                height: 8000,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Contain,
        ));
    }

    #[test]
    fn static_image_cache_does_not_upscale_small_contain_sources() {
        assert!(!should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 800,
                height: 600,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Contain,
        ));
    }

    #[test]
    fn static_image_cache_accepts_stretch_when_area_shrinks() {
        assert!(should_generate_static_image_cache_variant(
            RenderTargetSize {
                width: 9000,
                height: 500,
            },
            RenderTargetSize {
                width: 1920,
                height: 1080,
            },
            FitMode::Stretch,
        ));
    }

    #[test]
    fn skips_output_without_wallpaper_assignment() {
        let package = crate::core::load_gwpdir("examples/wallpapers/static-demo.gwpdir").unwrap();
        let plan = static_wallpaper_plan("eDP-1", &package, &OutputState::default()).unwrap();
        assert_eq!(plan, None);
    }

    #[test]
    fn builds_sync_plan_for_default_and_per_output_wallpapers() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        state.outputs.insert(
            "DP-1".to_owned(),
            OutputState {
                wallpaper: Some(WallpaperAssignment {
                    path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
                    variant: None,
                }),
                ..OutputState::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![crate::desktop::DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, std::env::temp_dir());
        assert_eq!(sync.plans.len(), 2);
        assert!(sync.errors.is_empty());
        assert!(sync.plans.iter().any(|plan| plan.output_name == "eDP-1"));
        assert!(sync.plans.iter().any(|plan| plan.output_name == "DP-1"));
        assert_eq!(sync.decisions.len(), 2);
        assert!(
            sync.decisions
                .iter()
                .all(|decision| decision.action == StaticRenderAction::Render)
        );
        assert!(sync.video_plans.is_empty());
    }

    #[test]
    fn config_default_wallpaper_builds_plan_for_desktop_output() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "eDP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn adaptive_snapshot_throttles_render_sync_decision() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.throttle_max_fps = 15;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_unfocused_removes_unfocused_output_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseUnfocused;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };
        let adaptive = crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Adaptive
        );
    }

    #[test]
    fn adaptive_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.adaptive.enabled = true;
        config.adaptive.action = crate::config::AdaptiveAction::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };
        let adaptive = adaptive_cpu_pressure_snapshot();

        let sync = static_render_sync_plan_with_config_and_adaptive(
            &config,
            &desktop,
            &state,
            std::env::temp_dir(),
            &adaptive,
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn battery_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Battery
        );
    }

    #[test]
    fn battery_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.battery = PowerPolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            power: PowerState::Battery,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn hidden_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::OutputHidden
        );
    }

    #[test]
    fn hidden_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn session_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_active: false,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::SessionInactive
        );
    }

    #[test]
    fn session_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.session = DynamicPausePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            session_locked: true,
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn unfocused_pause_dynamic_removes_slideshow_from_render_plan() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn unfocused_pause_dynamic_keeps_static_wallpaper_renderable() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.performance.unfocused = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn config_output_wallpaper_adds_named_output_without_state() {
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "DP-1".to_owned(),
            OutputConfig {
                wallpaper: Some("examples/wallpapers/static-demo.gwpdir".to_owned()),
                ..OutputConfig::default()
            },
        );
        let state = AppState::default();
        let desktop = DesktopSnapshot::default();

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans[0].output_name, "DP-1");
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn persisted_state_wallpaper_overrides_config_default() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("missing-config-default.gwpdir".to_owned());
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("examples/wallpapers/static-demo.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_policy_removes_output_without_loading_wallpaper() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "missing-wallpaper.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert!(sync.plans.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some("missing-wallpaper.gwpdir")
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_removes_slideshow_after_manifest_load() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/slideshow-demo.gwpdir".to_owned());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync =
            static_render_sync_plan_with_config(&config, &desktop, &state, std::env::temp_dir());

        assert!(sync.plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Fullscreen
        );
    }

    #[test]
    fn fullscreen_pause_dynamic_keeps_static_wallpaper_renderable() {
        let test_dir = TestDir::new("gilder-fullscreen-pause-dynamic-static");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_continue_when_fullscreen(&package_dir);
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(package_dir.display().to_string());
        config.performance.fullscreen = crate::config::ThrottlePolicy::PauseDynamic;
        let state = AppState::default();
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                has_fullscreen: true,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.removals.is_empty());
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Active);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Interactive
        );
    }

    #[test]
    fn throttled_policy_keeps_static_plan_with_decision() {
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: "examples/wallpapers/static-demo.gwpdir".to_owned(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &PerformanceConfig::default(),
            &desktop,
            &state,
            std::env::temp_dir(),
        );
        assert_eq!(sync.plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.decisions.len(), 1);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn manifest_runtime_policy_can_pause_unfocused_output() {
        let test_dir = TestDir::new("gilder-runtime-unfocused-pause");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        set_runtime_pause_when_unfocused(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, ["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn builds_video_sync_plan_with_effective_fps() {
        let test_dir = TestDir::new("gilder-video-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = PerformanceConfig::default();
        config.background_max_fps = 15;
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        let plan = &sync.video_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/loop.webm"));
        assert!(
            plan.poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
        assert_eq!(plan.fit, FitMode::Contain);
        assert!(!plan.loop_playback);
        assert!(plan.muted);
        assert_eq!(plan.manifest_max_fps, Some(60));
        assert_eq!(plan.target_max_fps, Some(15));
        assert_eq!(plan.start_offset_ms, 1200);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(15));
    }

    #[test]
    fn video_plan_keeps_audio_unmuted_when_runtime_allows_audio() {
        let test_dir = TestDir::new("gilder-video-runtime-audio");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        set_runtime_allow_audio(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(!sync.video_plans[0].muted);
    }

    #[test]
    fn output_performance_override_sets_video_target_fps() {
        let test_dir = TestDir::new("gilder-output-performance-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.video.decoder = VideoDecoderPolicy::Software;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                performance: OutputPerformanceConfig {
                    background_max_fps: Some(12),
                    ..OutputPerformanceConfig::default()
                },
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].target_max_fps, Some(12));
        assert_eq!(
            sync.video_plans[0].decoder_policy,
            VideoDecoderPolicy::Software
        );
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
        assert_eq!(sync.decisions[0].performance.max_fps, Some(12));
        assert_eq!(
            sync.decisions[0].performance.reason,
            DecisionReason::Unfocused
        );
    }

    #[test]
    fn output_fit_override_sets_video_and_poster_fit() {
        let test_dir = TestDir::new("gilder-output-fit-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                fit: Some(FitMode::Stretch),
                ..OutputConfig::default()
            },
        );
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.video_plans[0].fit, FitMode::Stretch);
    }

    #[test]
    fn video_plan_uses_requested_variant_source() {
        let test_dir = TestDir::new("gilder-video-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: Some("mobile".to_owned()),
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
        assert_eq!(
            sync.decisions[0].wallpaper.as_deref(),
            Some(package_dir.display().to_string().as_str())
        );
    }

    #[test]
    fn video_plan_auto_selects_portrait_variant_source() {
        let test_dir = TestDir::new("gilder-video-auto-variant-plan");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                width: Some(1080),
                height: Some(1920),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.errors.is_empty());
        assert!(
            sync.video_plans[0]
                .source
                .ends_with("assets/loop-mobile.webm")
        );
    }

    #[test]
    fn video_plan_uses_preview_poster_when_entry_poster_is_missing() {
        let test_dir = TestDir::new("gilder-video-preview-poster");
        let package_dir = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&package_dir);
        remove_entry_poster(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.plans.is_empty());
        assert!(
            sync.video_plans[0]
                .poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
    }

    #[test]
    fn scene_document_builds_native_scene_plan() {
        let test_dir = TestDir::new("gilder-scene-plan");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        assert!(sync.errors.is_empty());
        let plan = &sync.scene_plans[0];
        assert!(
            plan.source
                .as_ref()
                .unwrap()
                .ends_with("assets/scene.gscene.json")
        );
        assert_eq!(plan.layers.len(), 1);
        assert!(
            plan.layers[0]
                .source
                .as_ref()
                .unwrap()
                .ends_with("assets/background.svg")
        );
        assert!(matches!(
            &plan.display,
            Some(SceneDisplayPlan::Image { source, fit, background })
                if source.ends_with("assets/background.svg")
                    && *fit == FitMode::Cover
                    && background.as_deref() == Some("#000000")
        ));
        assert_eq!(sync.cache.scene_snapshot_cache_entries, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_generations, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_reuses, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_bytes, 0);
        assert_eq!(sync.cache.planned_scene_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);

        let sync_again = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync_again.cache.scene_snapshot_cache_generations, 0);
        assert_eq!(sync_again.cache.scene_snapshot_cache_reuses, 0);
    }

    #[test]
    fn scene_plan_resolves_audio_cue_resources_to_package_paths() {
        let test_dir = TestDir::new("gilder-scene-audio-plan");
        let package_dir = test_dir.path.join("scene-audio.gwpdir");
        write_scene_audio_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.audio_cue_count, 1);
        assert_eq!(plan.layers[0].audio.len(), 1);
        let cue = &plan.layers[0].audio[0];
        assert!(cue.source.ends_with("assets/audio/theme.ogg"));
        assert_eq!(cue.playback_mode.as_deref(), Some("loop"));
        assert_eq!(cue.volume, Some(json!(0.75)));
        assert!(!cue.start_silent);
    }

    #[test]
    fn scene_color_layer_uses_direct_display_without_snapshot() {
        let test_dir = TestDir::new("gilder-scene-color-plan");
        let package_dir = test_dir.path.join("scene-color.gwpdir");
        write_minimal_scene_color_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert!(matches!(
            &plan.display,
            Some(SceneDisplayPlan::Color { color }) if color == "#203040"
        ));
        assert_eq!(sync.cache.scene_snapshot_cache_generations, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_reuses, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_entries, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_bytes, 0);
        assert_eq!(sync.cache.planned_scene_image_resources, 0);
        assert_eq!(sync.cache.planned_image_resource_references, 0);
    }

    #[test]
    fn scene_full_rectangle_layer_uses_direct_display_without_snapshot() {
        let test_dir = TestDir::new("gilder-scene-full-rect-plan");
        let package_dir = test_dir.path.join("scene-full-rect.gwpdir");
        write_minimal_scene_full_rect_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Rectangle);
        assert!(matches!(
            &plan.display,
            Some(SceneDisplayPlan::Color { color }) if color == "#304050"
        ));
        assert_eq!(sync.cache.scene_snapshot_cache_generations, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_entries, 0);
        assert_eq!(sync.cache.planned_scene_image_resources, 0);
        assert_eq!(sync.cache.planned_image_resource_references, 0);
    }

    #[test]
    fn scene_single_image_layer_uses_direct_display_without_snapshot() {
        let test_dir = TestDir::new("gilder-scene-image-plan");
        let package_dir = test_dir.path.join("scene-image.gwpdir");
        write_minimal_scene_image_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Image);
        assert!(matches!(
            &plan.display,
            Some(SceneDisplayPlan::Image { source, fit, background })
                if source.ends_with("assets/image.png")
                    && *fit == FitMode::Contain
                    && background.as_deref() == Some("#000000")
        ));
        assert_eq!(sync.cache.scene_snapshot_cache_generations, 0);
        assert_eq!(sync.cache.scene_snapshot_cache_entries, 0);
        assert_eq!(sync.cache.planned_scene_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn scene_shape_layers_build_snapshot() {
        let test_dir = TestDir::new("gilder-scene-shape-plan");
        let package_dir = test_dir.path.join("scene-shapes.gwpdir");
        write_minimal_scene_shape_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.layers.len(), 3);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Rectangle);
        assert_eq!(plan.layers[0].stroke_color.as_deref(), Some("#ffffff"));
        assert_eq!(plan.layers[1].kind, SceneNodeKind::Ellipse);
        assert_eq!(plan.layers[2].kind, SceneNodeKind::Rectangle);
        assert_eq!(plan.layers[2].color, None);
        assert_eq!(plan.layers[2].stroke_color.as_deref(), Some("#ffcc00"));
        let display_source = scene_display_source(plan);
        let snapshot = fs::read_to_string(display_source).unwrap();
        assert!(snapshot.contains("<rect"));
        assert!(snapshot.contains(r##"rx="16""##));
        assert!(snapshot.contains(r##"stroke="#ffffff""##));
        assert!(snapshot.contains("<ellipse"));
        assert!(snapshot.contains(r##"fill="#80ffaa""##));
        assert!(snapshot.contains(r##"stroke="#ffcc00""##));
        assert!(snapshot.contains(r##"fill="none""##));
        assert_eq!(sync.cache.scene_snapshot_cache_generations, 1);
    }

    #[test]
    fn scene_text_layer_builds_snapshot() {
        let test_dir = TestDir::new("gilder-scene-text-plan");
        let package_dir = test_dir.path.join("scene-text.gwpdir");
        write_minimal_scene_text_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Text);
        assert_eq!(plan.layers[0].text_align, Some(SceneTextAlign::Middle));
        let display_source = scene_display_source(plan);
        let snapshot = fs::read_to_string(display_source).unwrap();
        assert!(snapshot.contains("<text"));
        assert!(snapshot.contains(r##"font-family="Inter""##));
        assert!(snapshot.contains(r##"font-weight="700""##));
        assert!(snapshot.contains(r##"text-anchor="middle""##));
        assert!(snapshot.contains("Gilder &amp; Wayland"));
    }

    #[test]
    fn scene_path_layer_builds_snapshot() {
        let test_dir = TestDir::new("gilder-scene-path-plan");
        let package_dir = test_dir.path.join("scene-path.gwpdir");
        write_minimal_scene_path_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Path);
        assert_eq!(plan.layers[0].stroke_color.as_deref(), Some("#80ffaa"));
        let display_source = scene_display_source(plan);
        let snapshot = fs::read_to_string(display_source).unwrap();
        assert!(snapshot.contains("<path"));
        assert!(snapshot.contains(r##"fill="none""##));
        assert!(snapshot.contains(r##"stroke="#80ffaa""##));
        assert!(snapshot.contains("M 0 80 C 120 20 240 140 360 80"));
    }

    #[test]
    fn scene_property_binding_applies_effective_output_property() {
        let test_dir = TestDir::new("gilder-scene-property-binding");
        let package_dir = test_dir.path.join("scene-property.gwpdir");
        let cache_dir = test_dir.path.join("cache");
        write_scene_property_binding_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let default_sync = static_render_sync_plan(&desktop, &state, &cache_dir);

        assert!(default_sync.errors.is_empty());
        assert_eq!(default_sync.scene_plans.len(), 1);
        let default_plan = &default_sync.scene_plans[0];
        assert_eq!(default_plan.bound_properties, vec!["scene_opacity"]);
        assert_eq!(default_plan.property_binding_count, 1);
        assert_eq!(default_plan.timeline_animation_count, 0);
        assert_eq!(default_plan.timeline_animated_layer_count, 0);
        assert!((default_plan.layers[0].opacity - 0.6).abs() < f64::EPSILON);
        let default_snapshot = scene_display_source(default_plan);
        assert!(
            fs::read_to_string(default_snapshot)
                .unwrap()
                .contains(r#"opacity="0.6""#)
        );

        state.set_property(None, "scene_opacity", json!(0.75));
        state.set_property(Some("eDP-1"), "scene_opacity", json!(0.25));
        let override_sync = static_render_sync_plan(&desktop, &state, &cache_dir);

        assert!(override_sync.errors.is_empty());
        assert_eq!(override_sync.scene_plans.len(), 1);
        let override_plan = &override_sync.scene_plans[0];
        assert_eq!(override_plan.bound_properties, vec!["scene_opacity"]);
        assert_eq!(override_plan.property_binding_count, 1);
        assert!((override_plan.layers[0].opacity - 0.25).abs() < f64::EPSILON);
        let override_snapshot = scene_display_source(override_plan);
        assert_ne!(override_snapshot, default_snapshot);
        assert!(
            fs::read_to_string(override_snapshot)
                .unwrap()
                .contains(r#"opacity="0.25""#)
        );
    }

    #[test]
    fn scene_audio_response_ready_properties_drive_geometry_fields() {
        let document: SceneDocument = serde_json::from_value(json!({
            "systems": { "audio_response": "ready" },
            "nodes": [
                {
                    "id": "bass-bar",
                    "type": "audio-response",
                    "color": "#44ccff",
                    "width": 20,
                    "height": 4
                }
            ],
            "property_bindings": [
                {
                    "property": "audio.bass",
                    "target_node": "bass-bar",
                    "target": "width",
                    "scale": 120,
                    "offset": 12
                }
            ]
        }))
        .unwrap();

        document.validate().unwrap();
        let first = document.snapshot_at_with_property_resolver(0, |property| {
            scene_audio_response_property_value(&document, 0, property)
        });
        let later = document.snapshot_at_with_property_resolver(250, |property| {
            scene_audio_response_property_value(&document, 250, property)
        });

        assert_eq!(first.layers[0].kind, SceneNodeKind::AudioResponse);
        assert_eq!(first.layers[0].color.as_deref(), Some("#44ccff"));
        assert_ne!(first.layers[0].width, later.layers[0].width);
        assert!(first.layers[0].width.unwrap() >= 12.0);
        assert!(later.layers[0].width.unwrap() >= 12.0);
    }

    #[test]
    fn scene_cursor_parallax_from_desktop_output_offsets_scene_transform() {
        let test_dir = TestDir::new("gilder-scene-cursor-parallax");
        let package_dir = test_dir.path.join("scene-parallax.gwpdir");
        let cache_dir = test_dir.path.join("cache");
        write_scene_parallax_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        state.set_property(None, "scene.parallax.x", json!(-1.0));
        state.set_property(Some("eDP-1"), "scene.parallax.y", json!(1.0));
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                cursor_parallax: Some(DesktopCursorParallax { x: 0.4, y: -0.2 }),
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, &cache_dir);

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert!(plan.cursor_parallax_input_ready);
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Rectangle);
        assert!((plan.layers[0].transform.x - 5.0).abs() < f64::EPSILON);
        assert!((plan.layers[0].transform.y - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scene_timeline_animation_metadata_reaches_plan() {
        let test_dir = TestDir::new("gilder-scene-animation-plan");
        let package_dir = test_dir.path.join("scene-animation.gwpdir");
        write_scene_animation_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.errors.is_empty());
        assert_eq!(sync.scene_plans.len(), 1);
        let plan = &sync.scene_plans[0];
        assert_eq!(plan.timeline_animation_count, 2);
        assert_eq!(plan.timeline_animated_layer_count, 1);
        assert_eq!(plan.property_binding_count, 0);
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0].kind, SceneNodeKind::Rectangle);
        assert!((plan.layers[0].transform.x - 10.0).abs() < f64::EPSILON);
        assert!((plan.layers[0].opacity - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn scene_runtime_sampler_resamples_timeline_layers_from_gscene_source() {
        let test_dir = TestDir::new("gilder-scene-runtime-sampler");
        let package_dir = test_dir.path.join("scene-animation.gwpdir");
        write_scene_animation_gwpdir(&package_dir);
        let plan = scene_wallpaper_plan_from_gscene_path(
            "eDP-1".to_owned(),
            &package_dir,
            package_dir.join("assets/scene.gscene.json"),
            Some(60),
            0,
            Some(FitMode::Cover),
        )
        .unwrap();
        let sampler = SceneWallpaperRuntimeSampler::from_plan(&plan)
            .unwrap()
            .expect("runtime sampler");

        let sampled = sampler.sample_plan(500).unwrap();

        assert_eq!(sampled.timeline_animation_count, 2);
        assert_eq!(sampled.timeline_animated_layer_count, 1);
        assert_eq!(sampled.layers.len(), 1);
        assert_eq!(sampled.layers[0].kind, SceneNodeKind::Rectangle);
        assert!((sampled.layers[0].transform.x - 60.0).abs() < f64::EPSILON);
        assert!((sampled.layers[0].opacity - 0.65).abs() < f64::EPSILON);
    }

    #[test]
    fn scene_runtime_sampler_resamples_native_idle_controller_bindings() {
        let test_dir = TestDir::new("gilder-scene-controller-sampler");
        let package_dir = test_dir.path.join("scene-controller.gwpdir");
        write_scene_controller_gwpdir(&package_dir);
        let plan = scene_wallpaper_plan_from_gscene_path(
            "eDP-1".to_owned(),
            &package_dir,
            package_dir.join("assets/scene.gscene.json"),
            Some(60),
            0,
            Some(FitMode::Cover),
        )
        .unwrap();
        let mut sampler = SceneWallpaperRuntimeSampler::from_plan(&plan)
            .unwrap()
            .expect("runtime sampler");

        let first = sampler.sample_plan(0).unwrap();
        let later = sampler.sample_plan(600).unwrap();
        let reused = sampler.sample_frame_reusing(700).unwrap();
        let nonzero_snapshot_plan = scene_wallpaper_plan_from_gscene_path(
            "eDP-1".to_owned(),
            &package_dir,
            package_dir.join("assets/scene.gscene.json"),
            Some(60),
            600,
            Some(FitMode::Cover),
        )
        .unwrap();

        assert_eq!(
            first
                .layers
                .iter()
                .find(|layer| layer.id == "idle-target")
                .unwrap()
                .kind,
            SceneNodeKind::Video
        );
        assert!(
            first
                .layers
                .iter()
                .find(|layer| layer.id == "idle-target")
                .unwrap()
                .opacity
                .abs()
                < f64::EPSILON
        );
        assert!(
            (later
                .layers
                .iter()
                .find(|layer| layer.id == "idle-target")
                .unwrap()
                .opacity
                - 1.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (reused
                .layers
                .iter()
                .find(|layer| layer.id == "idle-target")
                .unwrap()
                .opacity
                - 1.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (nonzero_snapshot_plan
                .layers
                .iter()
                .find(|layer| layer.id == "idle-target")
                .unwrap()
                .opacity
                - 1.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            later
                .layers
                .iter()
                .find(|layer| layer.id == "click-target")
                .unwrap()
                .opacity
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn scene_runtime_sampler_resamples_particle_layers_from_gscene_source() {
        let test_dir = TestDir::new("gilder-scene-particle-sampler");
        let package_dir = test_dir.path.join("scene-particles.gwpdir");
        write_scene_particle_gwpdir(&package_dir);
        let plan = scene_wallpaper_plan_from_gscene_path(
            "eDP-1".to_owned(),
            &package_dir,
            package_dir.join("assets/scene.gscene.json"),
            Some(60),
            0,
            Some(FitMode::Cover),
        )
        .unwrap();
        let sampler = SceneWallpaperRuntimeSampler::from_plan(&plan)
            .unwrap()
            .expect("runtime sampler");

        let first = sampler.sample_plan(0).unwrap();
        let later = sampler.sample_plan(500).unwrap();

        assert_eq!(first.scene_systems.particles, SceneSystemStatus::Ready);
        assert_eq!(first.layers.len(), 4);
        assert_eq!(later.layers.len(), 4);
        assert!(
            first
                .layers
                .iter()
                .all(|layer| layer.kind == SceneNodeKind::Rectangle)
        );
        assert_ne!(first.layers[0].transform, later.layers[0].transform);
    }

    #[test]
    fn web_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-web-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn web_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("gilder-web-without-fallback-plan");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        remove_entry_fallback(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(sync.errors[0].message, "web entries are not supported here");
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn shader_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-shader-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert_eq!(sync.plans.len(), 1);
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_image_resource_references, 1);
    }

    #[test]
    fn shader_without_fallback_reports_unsupported_entry() {
        let test_dir = TestDir::new("gilder-shader-without-fallback-plan");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        remove_entry_fallback(&package_dir);
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan(&desktop, &state, test_dir.path.join("cache"));

        assert!(sync.plans.is_empty());
        assert_eq!(sync.errors.len(), 1);
        assert_eq!(
            sync.errors[0].message,
            "shader entries are not supported here"
        );
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Error);
    }

    #[test]
    fn pause_dynamic_releases_shader_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-shader-pause-dynamic");
        let package_dir = test_dir.path.join("shader-demo.gwpdir");
        write_minimal_shader_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }

    #[test]
    fn pause_dynamic_releases_web_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-web-pause-dynamic");
        let package_dir = test_dir.path.join("web-demo.gwpdir");
        write_minimal_web_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }

    #[test]
    fn pause_dynamic_releases_scene_wallpaper_after_manifest_load() {
        let test_dir = TestDir::new("gilder-scene-pause-dynamic");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_gwpdir(&package_dir);
        let mut config = GilderConfig::default();
        config.performance.hidden = DynamicPausePolicy::PauseDynamic;
        config.default_wallpaper = Some(package_dir.display().to_string());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                visible: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert!(sync.slideshow_plans.is_empty());
        assert!(sync.scene_plans.is_empty());
        assert!(sync.errors.is_empty());
        assert_eq!(sync.removals, vec!["eDP-1"]);
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Remove);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Paused);
        assert_eq!(
            sync.decisions[0].performance.reason,
            crate::policy::DecisionReason::OutputHidden
        );
    }

    #[test]
    fn builds_slideshow_sync_plan_with_effective_fps() {
        let test_dir = TestDir::new("gilder-slideshow-plan");
        let package_dir = test_dir.path.join("slideshow-demo.gwpdir");
        write_minimal_slideshow_gwpdir(&package_dir);
        let mut config = PerformanceConfig::default();
        config.background_max_fps = 10;
        let mut state = AppState::default();
        state.default_wallpaper = Some(WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        });
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput {
                focused: false,
                ..DesktopOutput::virtual_output("eDP-1")
            }],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_performance(
            &config,
            &desktop,
            &state,
            test_dir.path.join("cache"),
        );

        assert!(sync.plans.is_empty());
        assert!(sync.video_plans.is_empty());
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert!(sync.errors.is_empty());
        let plan = &sync.slideshow_plans[0];
        assert_eq!(plan.output_name, "eDP-1");
        assert_eq!(plan.sources.len(), 2);
        assert!(plan.sources[0].ends_with("assets/a.svg"));
        assert!(plan.sources[1].ends_with("assets/b.svg"));
        assert_eq!(plan.interval_ms, 1_500);
        assert_eq!(plan.transition, Transition::Crossfade);
        assert_eq!(plan.fit, FitMode::Contain);
        assert_eq!(plan.target_max_fps, Some(10));
        assert_eq!(sync.decisions[0].action, StaticRenderAction::Render);
        assert_eq!(sync.decisions[0].performance.mode, RenderMode::Throttled);
    }

    #[test]
    fn render_sync_reports_planned_image_resource_footprint() {
        let test_dir = TestDir::new("gilder-render-resource-footprint");
        let static_package = test_dir.path.join("static-demo.gwpdir");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        let slideshow_package = test_dir.path.join("slideshow-demo.gwpdir");
        let scene_package = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_static_variant_gwpdir(&static_package);
        write_minimal_video_gwpdir(&video_package);
        write_minimal_slideshow_gwpdir(&slideshow_package);
        write_minimal_scene_gwpdir(&scene_package);
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(static_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "HDMI-A-1".to_owned(),
            OutputConfig {
                wallpaper: Some(video_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "DP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(slideshow_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "DP-2".to_owned(),
            OutputConfig {
                wallpaper: Some(scene_package.display().to_string()),
                ..OutputConfig::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
                DesktopOutput::virtual_output("DP-1"),
                DesktopOutput::virtual_output("DP-2"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert_eq!(sync.scene_plans.len(), 1);
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_video_poster_resources, 1);
        assert_eq!(sync.cache.planned_slideshow_image_resources, 2);
        assert_eq!(sync.cache.planned_scene_image_resources, 1);
        let expected_image_resource_count = 4;
        assert_eq!(
            sync.cache.planned_image_resource_references,
            expected_image_resource_count
        );
        assert_eq!(
            sync.cache.planned_unique_image_resources,
            expected_image_resource_count
        );
        let static_bytes = fs::metadata(static_package.join("assets/wallpaper.svg"))
            .unwrap()
            .len();
        let poster_bytes = fs::metadata(video_package.join("previews/poster.jpg"))
            .unwrap()
            .len();
        let slideshow_bytes = fs::metadata(slideshow_package.join("assets/a.svg"))
            .unwrap()
            .len()
            + fs::metadata(slideshow_package.join("assets/b.svg"))
                .unwrap()
                .len();
        assert!(matches!(
            &sync.scene_plans[0].display,
            Some(SceneDisplayPlan::Image { source, .. })
                if source.ends_with("assets/background.svg")
        ));
        let scene_bytes = fs::metadata(scene_package.join("assets/background.svg"))
            .unwrap()
            .len();
        assert_eq!(sync.cache.planned_static_image_resource_bytes, static_bytes);
        assert_eq!(sync.cache.planned_video_poster_resource_bytes, poster_bytes);
        assert_eq!(
            sync.cache.planned_slideshow_image_resource_bytes,
            slideshow_bytes
        );
        assert_eq!(sync.cache.planned_scene_image_resource_bytes, scene_bytes);
        let expected_image_resource_bytes = static_bytes + slideshow_bytes + scene_bytes;
        assert_eq!(
            sync.cache.planned_image_resource_reference_bytes,
            expected_image_resource_bytes
        );
        assert_eq!(
            sync.cache.planned_unique_image_resource_bytes,
            expected_image_resource_bytes
        );
    }

    #[test]
    fn render_sync_reports_duplicate_video_source_candidates() {
        let test_dir = TestDir::new("gilder-video-source-sharing");
        let video_package = test_dir.path.join("video-demo.gwpdir");
        write_minimal_video_gwpdir(&video_package);
        let mut config = GilderConfig::default();
        for output_name in ["eDP-1", "HDMI-A-1"] {
            config.outputs.insert(
                output_name.to_owned(),
                OutputConfig {
                    wallpaper: Some(video_package.display().to_string()),
                    ..OutputConfig::default()
                },
            );
        }
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.video_plans.len(), 2);
        assert_eq!(sync.cache.planned_video_source_references, 2);
        assert_eq!(sync.cache.planned_unique_video_sources, 1);
        assert_eq!(sync.cache.planned_duplicate_video_source_references, 1);
        assert_eq!(sync.cache.planned_max_video_source_outputs, 2);
        let video_bytes = fs::metadata(video_package.join("assets/loop.webm"))
            .unwrap()
            .len();
        assert_eq!(
            sync.cache.planned_video_source_reference_bytes,
            video_bytes * 2
        );
        assert_eq!(sync.cache.planned_unique_video_source_bytes, video_bytes);
    }

    #[test]
    fn builds_plan_from_gwp_archive() {
        let test_dir = TestDir::new("gilder-render-archive");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache = test_dir.path.join("cache");
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let assignment = WallpaperAssignment {
            path: archive.display().to_string(),
            variant: None,
        };

        let plan = static_wallpaper_plan_for_assignment("eDP-1", &assignment, &cache).unwrap();
        assert_eq!(plan.output_name, "eDP-1");
        assert!(plan.source.ends_with("assets/wallpaper.svg"));
        assert!(cache.join("render-cache").exists());
    }

    #[test]
    fn render_package_cache_reuses_loaded_package() {
        let test_dir = TestDir::new("gilder-render-package-cache");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 16, u64::MAX);

        let first = cache.package(&assignment).unwrap();
        let first_id = first.manifest.id.clone();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        let second = cache.package(&assignment).unwrap();
        let second_id = second.manifest.id.clone();

        assert_eq!(first_id, "org.example.static-variant");
        assert_eq!(second_id, first_id);
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(cache.packages.len(), 1);
        assert_eq!(cache.stats.package_cache_misses, 1);
        assert_eq!(cache.stats.package_cache_hits, 1);
    }

    #[test]
    fn render_package_cache_evicts_old_entries_at_limit() {
        let test_dir = TestDir::new("gilder-render-package-cache-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let assignment_a = WallpaperAssignment {
            path: package_a.display().to_string(),
            variant: None,
        };
        let assignment_b = WallpaperAssignment {
            path: package_b.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 1, u64::MAX);

        cache.package(&assignment_a).unwrap();
        cache.package(&assignment_b).unwrap();
        fs::remove_file(package_a.join(crate::core::MANIFEST_FILE)).unwrap();
        let err = cache.package(&assignment_a).unwrap_err();

        assert!(err.to_string().contains("manifest"));
        assert!(
            err.to_string().contains(
                &package_a
                    .join(crate::core::MANIFEST_FILE)
                    .display()
                    .to_string()
            )
        );
        assert_eq!(cache.packages.len(), 1);
        assert_eq!(cache.stats.package_cache_hits, 0);
        assert_eq!(cache.stats.package_cache_misses, 3);
        assert_eq!(cache.stats.package_cache_evictions, 2);
    }

    #[test]
    fn zero_package_cache_limit_disables_package_retention() {
        let test_dir = TestDir::new("gilder-render-package-cache-zero-limit");
        let package_dir = test_dir.path.join("static-variant.gwpdir");
        write_minimal_static_variant_gwpdir(&package_dir);
        let assignment = WallpaperAssignment {
            path: package_dir.display().to_string(),
            variant: None,
        };
        let mut cache = RenderPackageCache::new(&test_dir.path, 0, u64::MAX);

        cache.package(&assignment).unwrap();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        assert!(cache.package(&assignment).is_err());

        assert!(cache.packages.is_empty());
        assert_eq!(cache.stats.package_cache_hits, 0);
        assert_eq!(cache.stats.package_cache_misses, 2);
        assert_eq!(cache.stats.package_cache_evictions, 0);
    }

    #[test]
    fn render_package_cache_evicts_old_entries_at_retained_resource_byte_limit() {
        let test_dir = TestDir::new("gilder-render-package-cache-byte-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let assignment_a = WallpaperAssignment {
            path: package_a.display().to_string(),
            variant: None,
        };
        let assignment_b = WallpaperAssignment {
            path: package_b.display().to_string(),
            variant: None,
        };
        let package_resource_bytes = source_tree_size(&package_a.join("assets/wallpaper.svg"))
            + source_tree_size(&package_a.join("assets/wide.svg"));
        let mut cache = RenderPackageCache::new(&test_dir.path, 16, package_resource_bytes);

        cache.package(&assignment_a).unwrap();
        cache.package(&assignment_b).unwrap();
        cache.update_retained_resource_footprint();

        assert_eq!(cache.packages.len(), 1);
        assert!(cache.packages.contains_key(&assignment_b.path));
        assert_eq!(cache.stats.package_cache_evictions, 1);
        assert_eq!(
            cache.stats.package_cache_retained_unique_resource_bytes,
            package_resource_bytes
        );
    }

    #[test]
    fn render_sync_reports_package_cache_retained_resource_footprint() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            std::env::temp_dir(),
        );

        let retained_bytes = [
            "previews/thumbnail.svg",
            "previews/poster.svg",
            "assets/wallpaper.svg",
        ]
        .iter()
        .map(|path| {
            fs::metadata(Path::new("examples/wallpapers/static-demo.gwpdir").join(path))
                .unwrap()
                .len()
        })
        .sum::<u64>();
        let retained_preview_bytes = ["previews/thumbnail.svg", "previews/poster.svg"]
            .iter()
            .map(|path| {
                fs::metadata(Path::new("examples/wallpapers/static-demo.gwpdir").join(path))
                    .unwrap()
                    .len()
            })
            .sum::<u64>();
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_retained_resource_references, 3);
        assert_eq!(sync.cache.package_cache_retained_unique_resources, 3);
        assert_eq!(
            sync.cache.package_cache_retained_resource_bytes,
            retained_bytes
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_resource_bytes,
            retained_bytes
        );
        assert_eq!(
            sync.cache
                .package_cache_retained_preview_resource_references,
            2
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_preview_resources,
            2
        );
        assert_eq!(
            sync.cache.package_cache_retained_preview_resource_bytes,
            retained_preview_bytes
        );
        assert_eq!(
            sync.cache
                .package_cache_retained_unique_preview_resource_bytes,
            retained_preview_bytes
        );
    }

    #[test]
    fn zero_package_cache_limit_reports_no_retained_resource_footprint() {
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some("examples/wallpapers/static-demo.gwpdir".to_owned());
        config.cache.package_cache_max_entries = 0;
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            std::env::temp_dir(),
        );

        assert_eq!(sync.cache.package_cache_entries, 0);
        assert_eq!(sync.cache.package_cache_retained_resource_references, 0);
        assert_eq!(sync.cache.package_cache_retained_unique_resources, 0);
        assert_eq!(sync.cache.package_cache_retained_resource_bytes, 0);
        assert_eq!(sync.cache.package_cache_retained_unique_resource_bytes, 0);
        assert_eq!(
            sync.cache
                .package_cache_retained_preview_resource_references,
            0
        );
        assert_eq!(
            sync.cache.package_cache_retained_unique_preview_resources,
            0
        );
        assert_eq!(sync.cache.package_cache_retained_preview_resource_bytes, 0);
        assert_eq!(
            sync.cache
                .package_cache_retained_unique_preview_resource_bytes,
            0
        );
    }

    #[test]
    fn prunes_unprotected_archive_cache_entries() {
        let test_dir = TestDir::new("gilder-render-cache-prune");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old = render_cache_dir.join("a-old.gwpdir");
        let current = render_cache_dir.join("z-current.gwpdir");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&current).unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_render_cache(&cache_dir, 1, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn zero_archive_cache_limit_keeps_only_protected_entries() {
        let test_dir = TestDir::new("gilder-render-cache-zero-limit");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old_a = render_cache_dir.join("a-old.gwpdir");
        let old_b = render_cache_dir.join("b-old.gwpdir");
        let current = render_cache_dir.join("z-current.gwpdir");
        fs::create_dir_all(&old_a).unwrap();
        fs::create_dir_all(&old_b).unwrap();
        fs::create_dir_all(&current).unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_render_cache(&cache_dir, 0, &protected);

        assert_eq!(report.evictions, 2);
        assert_eq!(report.entries_after, 1);
        assert!(!old_a.exists());
        assert!(!old_b.exists());
        assert!(current.exists());
    }

    #[test]
    fn prunes_static_image_cache_entries_by_total_bytes() {
        let test_dir = TestDir::new("gilder-static-cache-byte-limit");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let protected = BTreeSet::new();

        let report = prune_static_image_cache(&cache_dir, 32, 5, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert_eq!(report.bytes_after, 5);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn static_image_cache_byte_limit_keeps_protected_files() {
        let test_dir = TestDir::new("gilder-static-cache-byte-limit-protected");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let mut protected = BTreeSet::new();
        protected.insert(current.clone());

        let report = prune_static_image_cache(&cache_dir, 32, 1, &protected);

        assert_eq!(report.evictions, 1);
        assert_eq!(report.errors, 0);
        assert_eq!(report.entries_after, 1);
        assert_eq!(report.bytes_after, 5);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn render_sync_reports_static_image_cache_bytes_after_prune() {
        let test_dir = TestDir::new("gilder-render-sync-static-cache-byte-limit");
        let cache_dir = test_dir.path.join("cache");
        let static_cache_dir = cache_dir.join("static-image-cache");
        fs::create_dir_all(&static_cache_dir).unwrap();
        let old = static_cache_dir.join("a-old.png");
        let current = static_cache_dir.join("b-current.png");
        fs::write(&old, b"12345").unwrap();
        fs::write(&current, b"67890").unwrap();
        let mut config = GilderConfig::default();
        config.cache.static_image_cache_max_bytes = 5;

        let sync = static_render_sync_plan_with_config(
            &config,
            &DesktopSnapshot::default(),
            &AppState::default(),
            &cache_dir,
        );

        assert_eq!(sync.cache.static_image_cache_entries, 1);
        assert_eq!(sync.cache.static_image_cache_bytes, 5);
        assert_eq!(sync.cache.static_image_cache_max_bytes, 5);
        assert_eq!(sync.cache.static_image_cache_evictions, 1);
        assert!(!old.exists());
        assert!(current.exists());
    }

    #[test]
    fn render_sync_prunes_stale_archive_cache_and_reports_stats() {
        let test_dir = TestDir::new("gilder-render-sync-cache-prune");
        let archive = test_dir.path.join("static-demo.gwp");
        let cache_dir = test_dir.path.join("cache");
        let render_cache_dir = cache_dir.join("render-cache");
        let old_a = render_cache_dir.join("a-old.gwpdir");
        let old_b = render_cache_dir.join("b-old.gwpdir");
        fs::create_dir_all(&old_a).unwrap();
        fs::create_dir_all(&old_b).unwrap();
        pack_gwp("examples/wallpapers/static-demo.gwpdir", &archive).unwrap();
        let mut config = GilderConfig::default();
        config.default_wallpaper = Some(archive.display().to_string());
        config.cache.render_cache_max_entries = 1;
        let desktop = DesktopSnapshot {
            outputs: vec![DesktopOutput::virtual_output("eDP-1")],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            &cache_dir,
        );

        let extract_dir = archive_extract_dir(&cache_dir, &archive);
        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_misses, 1);
        assert_eq!(sync.cache.archive_cache_extractions, 1);
        assert_eq!(sync.cache.archive_cache_evictions, 2);
        assert_eq!(sync.cache.archive_cache_entries, 1);
        assert_eq!(sync.cache.archive_cache_max_entries, 1);
        assert!(!old_a.exists());
        assert!(!old_b.exists());
        assert!(extract_dir.exists());
    }

    #[test]
    fn render_sync_reports_package_cache_limit_and_evictions() {
        let test_dir = TestDir::new("gilder-render-sync-package-cache-limit");
        let package_a = test_dir.path.join("a.gwpdir");
        let package_b = test_dir.path.join("b.gwpdir");
        write_minimal_static_variant_gwpdir(&package_a);
        write_minimal_static_variant_gwpdir(&package_b);
        let mut config = GilderConfig::default();
        config.cache.package_cache_max_entries = 1;
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_a.display().to_string()),
                ..OutputConfig::default()
            },
        );
        config.outputs.insert(
            "HDMI-A-1".to_owned(),
            OutputConfig {
                wallpaper: Some(package_b.display().to_string()),
                ..OutputConfig::default()
            },
        );
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
            ],
            ..DesktopSnapshot::default()
        };

        let sync = static_render_sync_plan_with_config(
            &config,
            &desktop,
            &AppState::default(),
            test_dir.path.join("cache"),
        );

        assert!(sync.errors.is_empty());
        assert_eq!(sync.plans.len(), 2);
        assert_eq!(sync.cache.package_cache_entries, 1);
        assert_eq!(sync.cache.package_cache_max_entries, 1);
        assert_eq!(sync.cache.package_cache_misses, 2);
        assert_eq!(sync.cache.package_cache_evictions, 1);
    }

    fn adaptive_cpu_pressure_snapshot() -> crate::adaptive::AdaptiveSnapshot {
        crate::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![crate::adaptive::AdaptiveTrigger {
                metric: crate::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..crate::adaptive::AdaptiveSnapshot::default()
        }
    }

    fn write_minimal_video_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        fs::write(path.join("assets/loop-mobile.webm"), b"not a real video").unwrap();
        fs::write(path.join("previews/poster.jpg"), b"not a real image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.video-demo",
            "version": "1.0.0",
            "title": "Video Demo",
            "kind": "video",
            "preview": {
                "poster": "previews/poster.jpg"
            },
            "entry": {
                "type": "video",
                "source": "assets/loop.webm",
                "poster": "previews/poster.jpg",
                "loop": false,
                "muted": false,
                "fit": "contain",
                "max_fps": 60,
                "start_offset_ms": 1200
            },
            "variants": [
                {
                    "id": "mobile",
                    "source": "assets/loop-mobile.webm",
                    "width": 1080,
                    "height": 1920
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_static_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/wide.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-variant",
            "version": "1.0.0",
            "title": "Static Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "wide",
                    "source": "assets/wide.svg",
                    "width": 2560,
                    "height": 1080
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_slideshow_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/a.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/b.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.slideshow-demo",
            "version": "1.0.0",
            "title": "Slideshow Demo",
            "kind": "slideshow",
            "entry": {
                "type": "slideshow",
                "sources": ["assets/a.svg", "assets/b.svg"],
                "interval_ms": 1500,
                "transition": "crossfade",
                "fit": "contain"
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_playlist_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/battery.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-demo",
            "version": "1.0.0",
            "title": "Playlist Demo",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "battery-static",
                        "conditions": {
                            "power": "battery"
                        },
                        "entry": {
                            "type": "static-image",
                            "source": "assets/battery.svg",
                            "fit": "cover"
                        }
                    },
                    {
                        "id": "default-video",
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover",
                            "max_fps": 60
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_playlist_no_match_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/loop.webm"), b"not a real video").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.playlist-no-match",
            "version": "1.0.0",
            "title": "Playlist No Match",
            "kind": "playlist",
            "entry": {
                "type": "playlist",
                "items": [
                    {
                        "id": "dp-only-video",
                        "conditions": {
                            "outputs": ["DP-1"]
                        },
                        "entry": {
                            "type": "video",
                            "source": "assets/loop.webm",
                            "loop": true,
                            "muted": true,
                            "fit": "cover"
                        }
                    }
                ]
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_auto_variant_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/small.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/hd.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/uhd.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-auto-variant",
            "version": "1.0.0",
            "title": "Static Auto Variant Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg",
                "fit": "cover"
            },
            "variants": [
                {
                    "id": "small",
                    "source": "assets/small.svg",
                    "width": 1280,
                    "height": 720
                },
                {
                    "id": "hd",
                    "source": "assets/hd.svg",
                    "width": 1920,
                    "height": 1080
                },
                {
                    "id": "uhd",
                    "source": "assets/uhd.svg",
                    "width": 3840,
                    "height": 2160
                }
            ]
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_static_large_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/wallpaper.png"), b"original-large-image").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.static-large",
            "version": "1.0.0",
            "title": "Static Large Demo",
            "kind": "static-image",
            "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.png",
                "fit": "cover",
                "width": 7680,
                "height": 4320
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_web_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets/web")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("assets/web/index.html"),
            b"<main>web wallpaper</main>",
        )
        .unwrap();
        fs::write(
            path.join("assets/web/gilder-bridge.js"),
            b"window.gilder = {};",
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.web-demo",
            "version": "1.0.0",
            "title": "Web Demo",
            "kind": "web",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "web",
                "root": "assets/web",
                "index": "index.html",
                "fallback": "previews/poster.svg",
                "max_fps": 30
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_shader_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("shaders")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("shaders/main.frag"),
            br##"
uniform float u_time;
uniform vec2 u_resolution;
uniform float u_intensity;
void main() {}
"##,
        )
        .unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.shader-demo",
            "version": "1.0.0",
            "title": "Shader Demo",
            "kind": "shader",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "shader",
                "source": "shaders/main.frag",
                "fallback": "previews/poster.svg",
                "language": "glsl",
                "max_fps": 60,
                "uniforms": [
                    { "name": "u_time", "source": "time" },
                    { "name": "u_resolution", "source": "resolution" },
                    { "name": "u_intensity", "source": "property", "property": "intensity" }
                ]
            },
            "properties": {
                "intensity": {
                    "type": "range",
                    "min": 0.0,
                    "max": 1.0,
                    "default": 0.5
                }
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "resources": [
                {
                  "id": "background-resource",
                  "type": "image",
                  "source": "assets/background.svg"
                }
              ],
              "nodes": [
                {
                  "id": "background",
                  "type": "image",
                  "resource": "background-resource",
                  "fit": "cover"
                }
              ]
            }"##,
        )
        .unwrap();
        fs::write(path.join("assets/background.svg"), b"<svg/>").unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-demo",
            "version": "1.0.0",
            "title": "Scene Demo",
            "kind": "scene",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_audio_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets/audio")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "resources": [
                {
                  "id": "background-resource",
                  "type": "image",
                  "source": "assets/background.svg"
                },
                {
                  "id": "theme-audio",
                  "type": "audio",
                  "source": "assets/audio/theme.ogg"
                }
              ],
              "nodes": [
                {
                  "id": "background",
                  "type": "image",
                  "resource": "background-resource",
                  "audio": [
                    {
                      "resource": "theme-audio",
                      "source": "sounds/theme.ogg",
                      "playback_mode": "loop",
                      "volume": 0.75,
                      "start_silent": false
                    }
                  ]
                }
              ]
            }"##,
        )
        .unwrap();
        fs::write(path.join("assets/background.svg"), b"<svg/>").unwrap();
        fs::write(path.join("assets/audio/theme.ogg"), b"not real ogg").unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-audio",
            "version": "1.0.0",
            "title": "Scene Audio",
            "kind": "scene",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_color_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "nodes": [
                {
                  "id": "background",
                  "type": "color",
                  "color": "#203040"
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-color",
            "version": "1.0.0",
            "title": "Scene Color",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_full_rect_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 1280, "height": 720 },
              "nodes": [
                {
                  "id": "background",
                  "type": "rectangle",
                  "color": "#304050",
                  "width": 1280,
                  "height": 720
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-full-rect",
            "version": "1.0.0",
            "title": "Scene Full Rect",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_image_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/image.png"), b"image-bytes").unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 1280, "height": 720 },
              "resources": [
                {
                  "id": "image-resource",
                  "type": "image",
                  "source": "assets/image.png"
                }
              ],
              "nodes": [
                {
                  "id": "image",
                  "type": "image",
                  "resource": "image-resource",
                  "fit": "contain"
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-image",
            "version": "1.0.0",
            "title": "Scene Image",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_shape_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 1280, "height": 720 },
              "nodes": [
                {
                  "id": "panel",
                  "type": "rectangle",
                  "color": "#102030",
                  "stroke_color": "#ffffff",
                  "stroke_width": 2,
                  "corner_radius": 16,
                  "width": 640,
                  "height": 360,
                  "transform": { "x": 100, "y": 80 }
                },
                {
                  "id": "glow",
                  "type": "ellipse",
                  "color": "#80ffaa",
                  "width": 240,
                  "height": 160,
                  "opacity": 0.5,
                  "transform": { "x": 420, "y": 260 }
                },
                {
                  "id": "outline",
                  "type": "rectangle",
                  "stroke_color": "#ffcc00",
                  "stroke_width": 4,
                  "width": 128,
                  "height": 72,
                  "transform": { "x": 760, "y": 260 }
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-shapes",
            "version": "1.0.0",
            "title": "Scene Shapes",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_text_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 1280, "height": 720 },
              "nodes": [
                {
                  "id": "title",
                  "type": "text",
                  "text": "Gilder & Wayland",
                  "color": "#f0f4ff",
                  "font_size": 48,
                  "font_family": "Inter",
                  "font_weight": "700",
                  "text_align": "middle",
                  "width": 1280,
                  "transform": { "x": 0, "y": 96 }
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-text",
            "version": "1.0.0",
            "title": "Scene Text",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_minimal_scene_path_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 1280, "height": 720 },
              "nodes": [
                {
                  "id": "wave",
                  "type": "path",
                  "path": "M 0 80 C 120 20 240 140 360 80",
                  "stroke_color": "#80ffaa",
                  "stroke_width": 4,
                  "transform": { "x": 200, "y": 160 }
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-path",
            "version": "1.0.0",
            "title": "Scene Path",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_property_binding_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "nodes": [
                {
                  "id": "background",
                  "type": "color",
                  "color": "#203040"
                }
              ],
              "property_bindings": [
                {
                  "property": "scene_opacity",
                  "target": "opacity",
                  "target_node": "background"
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-property",
            "version": "1.0.0",
            "title": "Scene Property",
            "kind": "scene",
            "properties": {
                "scene_opacity": {
                    "type": "range",
                    "min": 0.0,
                    "max": 1.0,
                    "default": 0.6
                }
            },
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_parallax_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "render": {
                "parallax": { "amount": 10 }
              },
              "nodes": [
                {
                  "id": "near-panel",
                  "type": "rectangle",
                  "color": "#203040",
                  "width": 320,
                  "height": 180,
                  "transform": { "x": 3, "y": 4 },
                  "parallax_depth": 0.5
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-parallax",
            "version": "1.0.0",
            "title": "Scene Parallax",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_animation_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "nodes": [
                {
                  "id": "moving-panel",
                  "type": "rectangle",
                  "color": "#203040",
                  "width": 320,
                  "height": 180
                }
              ],
              "timelines": [
                {
                  "id": "moving-panel-timeline",
                  "target_node": "moving-panel",
                  "channels": [
                    {
                      "property": "x",
                      "keyframes": [
                        { "time_ms": 0, "value": 10 },
                        { "time_ms": 1000, "value": 110 }
                      ]
                    },
                    {
                      "property": "opacity",
                      "keyframes": [
                        { "time_ms": 0, "value": 0.4 },
                        { "time_ms": 1000, "value": 0.9 }
                      ]
                    }
                  ]
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-animation",
            "version": "1.0.0",
            "title": "Scene Animation",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_controller_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "resources": [
                { "id": "idle-video", "type": "video", "source": "assets/idle.mp4" },
                { "id": "click-video", "type": "video", "source": "assets/click.mp4" }
              ],
              "nodes": [
                {
                  "id": "idle-controller",
                  "type": "script",
                  "properties": {
                    "controller": {
                      "runtime": "native",
                      "kind": "idle-video-switch",
                      "property": "scene.controller.idle-controller.active",
                      "mouse_inactive_sec": { "value": 0.5 },
                      "target_node": "idle-target",
                      "target_type": "video"
                    }
                  }
                },
                {
                  "id": "idle-target",
                  "type": "video",
                  "resource": "idle-video",
                  "opacity": 0
                },
                {
                  "id": "click-controller",
                  "type": "script",
                  "properties": {
                    "controller": {
                      "runtime": "native",
                      "kind": "click-video-switch",
                      "property": "scene.controller.click-controller.active",
                      "target_node": "click-target",
                      "target_type": "video"
                    }
                  }
                },
                {
                  "id": "click-target",
                  "type": "video",
                  "resource": "click-video",
                  "opacity": 0
                }
              ],
              "property_bindings": [
                {
                  "property": "scene.controller.idle-controller.active",
                  "target": "opacity",
                  "target_node": "idle-target"
                },
                {
                  "property": "scene.controller.click-controller.active",
                  "target": "opacity",
                  "target_node": "click-target"
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-controller",
            "version": "1.0.0",
            "title": "Scene Controller",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn write_scene_particle_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(
            path.join("assets/scene.gscene.json"),
            br##"{
              "size": { "width": 640, "height": 360 },
              "systems": { "particles": "ready" },
              "nodes": [
                {
                  "id": "sparks",
                  "type": "particle-emitter",
                  "transform": { "x": 320, "y": 180 },
                  "properties": {
                    "particle": {
                      "count": 4,
                      "seed": 7,
                      "lifetime_ms": 1000,
                      "size": 8,
                      "speed": 20,
                      "spread_deg": 45,
                      "fade": false,
                      "color": "#ffaa00"
                    }
                  }
                }
              ]
            }"##,
        )
        .unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-particles",
            "version": "1.0.0",
            "title": "Scene Particles",
            "kind": "scene",
            "entry": {
                "type": "scene",
                "source": "assets/scene.gscene.json",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn scene_display_source(plan: &SceneWallpaperPlan) -> &Path {
        match &plan.display {
            Some(SceneDisplayPlan::Image { source, .. }) => source,
            _ => panic!("expected scene image display"),
        }
    }

    fn remove_entry_poster(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("poster");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn remove_entry_fallback(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .get_mut("entry")
            .and_then(|entry| entry.as_object_mut())
            .unwrap()
            .remove("fallback");
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_pause_when_unfocused(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_unfocused": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_continue_when_fullscreen(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_fullscreen": false
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn set_runtime_allow_audio(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "allow_audio": true
        });
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    fn active_performance_decision() -> PerformanceDecision {
        PerformanceDecision {
            mode: RenderMode::Active,
            max_fps: Some(60),
            reason: DecisionReason::Interactive,
        }
    }

    fn write_executable_script(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let pid = std::process::id();
            let sequence = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{pid}-{sequence}-{nanos}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
