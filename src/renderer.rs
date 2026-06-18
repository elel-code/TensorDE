//! Rendering plans and optional GTK/layer-shell renderer.

#[cfg(feature = "gtk-renderer")]
pub mod gtk;
#[cfg(feature = "video-renderer")]
pub mod video;

use crate::config::{CacheConfig, GilderConfig, PerformanceConfig, VideoDecoderPolicy};
use crate::core::manifest::Variant;
use crate::core::{FitMode, PackagePath, Transition, WallpaperEntry, WallpaperPackage};
use crate::desktop::{CompositorKind, DesktopOutput, DesktopSnapshot};
use crate::policy::{PerformanceDecision, RenderMode};
use crate::state::{AppState, OutputState, WallpaperAssignment};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque, hash_map::DefaultHasher};
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WallpaperRenderPlan {
    StaticImage(StaticWallpaperPlan),
    Video(VideoWallpaperPlan),
    Slideshow(SlideshowWallpaperPlan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticRenderSyncPlan {
    pub plans: Vec<StaticWallpaperPlan>,
    #[serde(default)]
    pub video_plans: Vec<VideoWallpaperPlan>,
    #[serde(default)]
    pub slideshow_plans: Vec<SlideshowWallpaperPlan>,
    pub removals: Vec<String>,
    pub errors: Vec<StaticRenderPlanFailure>,
    #[serde(default)]
    pub decisions: Vec<StaticRenderOutputDecision>,
    #[serde(default)]
    pub cache: RenderSyncCacheReport,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderSyncCacheReport {
    #[serde(default)]
    pub package_cache_entries: usize,
    #[serde(default)]
    pub package_cache_max_entries: usize,
    #[serde(default)]
    pub package_cache_hits: u64,
    #[serde(default)]
    pub package_cache_misses: u64,
    #[serde(default)]
    pub package_cache_evictions: u64,
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
    pub planned_static_image_resources: usize,
    #[serde(default)]
    pub planned_video_poster_resources: usize,
    #[serde(default)]
    pub planned_slideshow_image_resources: usize,
    #[serde(default)]
    pub planned_image_resource_references: usize,
    #[serde(default)]
    pub planned_unique_image_resources: usize,
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
    let mut removals = Vec::new();
    let mut errors = Vec::new();
    let mut decisions = Vec::new();
    let mut package_cache =
        RenderPackageCache::new(cache_dir, cache_config.package_cache_max_entries);
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
        performance = crate::policy::apply_runtime_policy(
            performance,
            &package.manifest.runtime,
            desktop_output,
        );
        let dynamic_wallpaper = dynamic_wallpaper_entry(&package.manifest.entry);
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

        match wallpaper_plan_with_target(
            &output_name,
            &package,
            &performance,
            video_decoder_policy,
            fit_override,
            assignment.variant.as_deref(),
            render_target,
        ) {
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
                if let Some(poster_plan) = video_poster_plan(&plan) {
                    plans.push(poster_plan);
                }
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
    update_render_sync_resource_footprint(&mut cache, &plans, &video_plans, &slideshow_plans);
    StaticRenderSyncPlan {
        plans,
        video_plans,
        slideshow_plans,
        removals,
        errors,
        decisions,
        cache,
    }
}

fn update_render_sync_resource_footprint(
    report: &mut RenderSyncCacheReport,
    plans: &[StaticWallpaperPlan],
    video_plans: &[VideoWallpaperPlan],
    slideshow_plans: &[SlideshowWallpaperPlan],
) {
    let video_poster_resources = video_plans
        .iter()
        .filter(|plan| plan.poster.is_some())
        .count();
    let slideshow_image_resources = slideshow_plans
        .iter()
        .map(|plan| plan.sources.len())
        .sum::<usize>();
    let static_image_resources = plans
        .iter()
        .filter(|plan| !is_video_poster_resource(plan, video_plans))
        .count();
    let mut unique_image_resources = BTreeSet::new();
    unique_image_resources.extend(plans.iter().map(|plan| plan.source.clone()));
    unique_image_resources.extend(
        slideshow_plans
            .iter()
            .flat_map(|plan| plan.sources.iter().cloned()),
    );

    report.planned_static_image_resources = static_image_resources;
    report.planned_video_poster_resources = video_poster_resources;
    report.planned_slideshow_image_resources = slideshow_image_resources;
    report.planned_image_resource_references = plans.len() + slideshow_image_resources;
    report.planned_unique_image_resources = unique_image_resources.len();
}

fn is_video_poster_resource(
    plan: &StaticWallpaperPlan,
    video_plans: &[VideoWallpaperPlan],
) -> bool {
    video_plans.iter().any(|video| {
        video.output_name == plan.output_name
            && video
                .poster
                .as_ref()
                .is_some_and(|poster| poster == &plan.source)
    })
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

fn dynamic_wallpaper_entry(entry: &WallpaperEntry) -> bool {
    matches!(
        entry,
        WallpaperEntry::Video { .. } | WallpaperEntry::Slideshow { .. }
    )
}

fn video_poster_plan(plan: &VideoWallpaperPlan) -> Option<StaticWallpaperPlan> {
    Some(StaticWallpaperPlan {
        output_name: plan.output_name.clone(),
        source: plan.poster.clone()?,
        fit: plan.fit,
        background: Some("#000000".to_owned()),
    })
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
) -> Result<WallpaperRenderPlan, RendererPlanError> {
    let output_name = output_name.into();
    let explicit_variant_source = explicit_variant_source(package, variant_id)?;
    match &package.manifest.entry {
        WallpaperEntry::StaticImage {
            source,
            fit,
            background,
            ..
        } => Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
            output_name,
            source: selected_variant_source(package, explicit_variant_source, render_target)
                .unwrap_or(source)
                .join_to(&package.root),
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
                output_name,
                source: selected_variant_source(package, explicit_variant_source, render_target)
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
            output_name,
            sources: sources
                .iter()
                .map(|source| source.join_to(&package.root))
                .collect(),
            interval_ms: *interval_ms,
            transition: *transition,
            fit: effective_fit(*fit, fit_override),
            target_max_fps: performance.max_fps,
        })),
        WallpaperEntry::SceneLite { fallback, .. } => {
            let Some(fallback) = fallback else {
                return Err(RendererPlanError::UnsupportedEntry(
                    package.manifest.entry.kind().as_str(),
                ));
            };
            Ok(WallpaperRenderPlan::StaticImage(StaticWallpaperPlan {
                output_name,
                source: fallback.join_to(&package.root),
                fit: effective_fit(FitMode::Cover, fit_override),
                background: Some("#000000".to_owned()),
            }))
        }
        other => Err(RendererPlanError::UnsupportedEntry(other.kind().as_str())),
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    packages: BTreeMap<String, Result<WallpaperPackage, RendererPlanError>>,
    package_order: VecDeque<String>,
    protected_archive_dirs: BTreeSet<PathBuf>,
    stats: RenderSyncCacheReport,
}

impl<'a> RenderPackageCache<'a> {
    fn new(cache_dir: &'a Path, max_entries: usize) -> Self {
        Self {
            cache_dir,
            max_entries,
            packages: BTreeMap::new(),
            package_order: VecDeque::new(),
            protected_archive_dirs: BTreeSet::new(),
            stats: RenderSyncCacheReport::default(),
        }
    }

    fn package(
        &mut self,
        assignment: &WallpaperAssignment,
    ) -> Result<WallpaperPackage, RendererPlanError> {
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
        );
        if self.max_entries > 0 {
            self.prune_for_insert();
            self.packages
                .insert(assignment.path.clone(), package.clone());
            self.package_order.push_back(assignment.path.clone());
        }
        package
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

    fn finish(mut self, cache_config: CacheConfig) -> RenderSyncCacheReport {
        let prune = prune_render_cache(
            self.cache_dir,
            cache_config.render_cache_max_entries,
            &self.protected_archive_dirs,
        );
        self.stats.package_cache_entries = self.packages.len();
        self.stats.package_cache_max_entries = cache_config.package_cache_max_entries;
        self.stats.archive_cache_entries = prune.entries_after;
        self.stats.archive_cache_max_entries = cache_config.render_cache_max_entries;
        self.stats.archive_cache_evictions = prune.evictions;
        self.stats.archive_cache_eviction_errors = prune.errors;
        self.stats
    }
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
        evictions,
        errors,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderCacheEntry {
    path: PathBuf,
    last_used: SystemTime,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        GilderConfig, OutputConfig, OutputPerformanceConfig, PerformanceConfig, PowerPolicy,
        VideoDecoderPolicy,
    };
    use crate::core::pack_gwp;
    use crate::desktop::{DesktopOutput, PowerState};
    use crate::policy::{DecisionReason, RenderMode};
    use crate::state::{OutputState, WallpaperAssignment};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

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

        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.video_plans.len(), 1);
        assert!(sync.removals.is_empty());
        assert!(sync.errors.is_empty());
        let poster_plan = &sync.plans[0];
        assert_eq!(poster_plan.output_name, "eDP-1");
        assert!(poster_plan.source.ends_with("previews/poster.jpg"));
        assert_eq!(poster_plan.fit, FitMode::Contain);
        assert_eq!(poster_plan.background.as_deref(), Some("#000000"));
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

        assert_eq!(sync.plans.len(), 1);
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.plans[0].fit, FitMode::Stretch);
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
        assert_eq!(sync.plans.len(), 1);
        assert!(
            sync.video_plans[0]
                .poster
                .as_ref()
                .unwrap()
                .ends_with("previews/poster.jpg")
        );
        assert!(sync.plans[0].source.ends_with("previews/poster.jpg"));
    }

    #[test]
    fn scene_lite_fallback_builds_static_plan() {
        let test_dir = TestDir::new("gilder-scene-lite-plan");
        let package_dir = test_dir.path.join("scene-demo.gwpdir");
        write_minimal_scene_lite_gwpdir(&package_dir);
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
        assert!(sync.errors.is_empty());
        assert!(sync.plans[0].source.ends_with("previews/poster.svg"));
        assert_eq!(sync.plans[0].fit, FitMode::Cover);
        assert_eq!(sync.plans[0].background.as_deref(), Some("#000000"));
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
        write_minimal_static_variant_gwpdir(&static_package);
        write_minimal_video_gwpdir(&video_package);
        write_minimal_slideshow_gwpdir(&slideshow_package);
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
        let desktop = DesktopSnapshot {
            outputs: vec![
                DesktopOutput::virtual_output("eDP-1"),
                DesktopOutput::virtual_output("HDMI-A-1"),
                DesktopOutput::virtual_output("DP-1"),
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
        assert_eq!(sync.video_plans.len(), 1);
        assert_eq!(sync.slideshow_plans.len(), 1);
        assert_eq!(sync.cache.planned_static_image_resources, 1);
        assert_eq!(sync.cache.planned_video_poster_resources, 1);
        assert_eq!(sync.cache.planned_slideshow_image_resources, 2);
        assert_eq!(sync.cache.planned_image_resource_references, 4);
        assert_eq!(sync.cache.planned_unique_image_resources, 4);
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
        let mut cache = RenderPackageCache::new(&test_dir.path, 16);

        let first_id = cache.package(&assignment).unwrap().manifest.id.clone();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        let second_id = cache.package(&assignment).unwrap().manifest.id.clone();

        assert_eq!(first_id, "org.example.static-variant");
        assert_eq!(second_id, first_id);
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
        let mut cache = RenderPackageCache::new(&test_dir.path, 1);

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
        let mut cache = RenderPackageCache::new(&test_dir.path, 0);

        cache.package(&assignment).unwrap();
        fs::remove_file(package_dir.join(crate::core::MANIFEST_FILE)).unwrap();
        assert!(cache.package(&assignment).is_err());

        assert!(cache.packages.is_empty());
        assert_eq!(cache.stats.package_cache_hits, 0);
        assert_eq!(cache.stats.package_cache_misses, 2);
        assert_eq!(cache.stats.package_cache_evictions, 0);
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

    fn write_minimal_scene_lite_gwpdir(path: &Path) {
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::create_dir_all(path.join("previews")).unwrap();
        fs::write(path.join("assets/scene.json"), b"{\"layers\":[]}").unwrap();
        fs::write(path.join("previews/poster.svg"), b"<svg/>").unwrap();
        let manifest = json!({
            "format": crate::core::FORMAT_NAME,
            "format_version": crate::core::FORMAT_VERSION,
            "id": "org.example.scene-demo",
            "version": "1.0.0",
            "title": "Scene Demo",
            "kind": "scene-lite",
            "preview": {
                "poster": "previews/poster.svg"
            },
            "entry": {
                "type": "scene-lite",
                "source": "assets/scene.json",
                "fallback": "previews/poster.svg",
                "max_fps": 60
            }
        });
        fs::write(
            path.join(crate::core::MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
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

    fn set_runtime_pause_when_unfocused(path: &Path) {
        let manifest_path = path.join(crate::core::MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["runtime"] = json!({
            "pause_when_unfocused": true
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

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
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
