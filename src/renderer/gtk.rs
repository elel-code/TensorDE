//! GTK 4 + layer-shell renderer for wallpaper output surfaces.

#[cfg(feature = "video-renderer")]
use super::VideoWallpaperPlan;
use super::{
    SceneLiteDisplayPlan, SceneLiteWallpaperPlan, SlideshowWallpaperPlan, StaticRenderSyncPlan,
    StaticWallpaperPlan,
};
use crate::core::{FitMode, Transition};
#[cfg(feature = "video-renderer")]
use crate::policy::RenderMode;
#[cfg(feature = "video-renderer")]
use crate::renderer::video::{
    GtkFrameClockPhase, VideoAllocationReport, VideoCapsReport, VideoDecoderPolicyStatus,
    VideoDecoderReport, VideoFrameStats, VideoMemoryPathReport, VideoMemoryRetentionReport,
    VideoPipelineDiagnostics, VideoPipelineDiagnosticsCache, VideoPipelineSnapshot,
    VideoQueueReport, VideoSinkTuningReport, VideoZeroCopyEvidence, apply_decoder_rank_policy,
    configure_video_pipeline_low_memory, configure_video_sink_low_memory, decoder_policy_status,
    decoder_report_from_message, merge_decoder_reports, playback_duration_ms, playback_position_ms,
    video_memory_retention_report,
};
use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
#[cfg(feature = "video-renderer")]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
#[cfg(feature = "video-renderer")]
use std::rc::Rc;
use std::time::{Duration, Instant};

#[cfg(feature = "video-renderer")]
use gst::prelude::*;
#[cfg(feature = "video-renderer")]
use gstreamer as gst;

const GTK_ACTIVE_RUNTIME_TICK_INTERVAL: Duration = Duration::from_millis(50);
const GTK_VIDEO_RUNTIME_TICK_INTERVAL: Duration = Duration::from_millis(250);
const SLIDESHOW_CROSSFADE_DURATION: Duration = Duration::from_millis(600);

#[cfg(feature = "video-renderer")]
const MUTED_PLAYBIN_FLAGS: &str = "video";
#[cfg(feature = "video-renderer")]
const AUDIBLE_PLAYBIN_FLAGS: &str = "video+audio";
#[cfg(feature = "video-renderer")]
const GTK_VIDEO_FRAME_STATS_ENV: &str = "GILDER_GTK_VIDEO_FRAME_STATS";
#[cfg(feature = "video-renderer")]
const GTK_VIDEO_SINK_CHAIN_ENV: &str = "GILDER_GTK_VIDEO_SINK_CHAIN";

pub struct GtkStaticRenderer {
    application: gtk::Application,
    windows: BTreeMap<String, RenderedOutput>,
    #[cfg(feature = "video-renderer")]
    video_runtimes: GtkVideoRuntimePool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GtkRendererResourceSnapshot {
    pub output_windows: usize,
    pub static_surfaces: usize,
    pub static_picture_surfaces: usize,
    pub static_css_surfaces: usize,
    pub static_color_surfaces: usize,
    pub slideshow_surfaces: usize,
    pub video_surfaces: usize,
    pub static_surface_resource_references: usize,
    pub static_surface_resource_bytes: u64,
    pub static_surface_unique_resources: usize,
    pub static_surface_unique_resource_bytes: u64,
    pub static_surface_estimated_decoded_bytes: u64,
    pub slideshow_resource_references: usize,
    pub slideshow_resource_bytes: u64,
    pub slideshow_unique_resources: usize,
    pub slideshow_unique_resource_bytes: u64,
    pub video_shared_runtimes: usize,
    pub video_pipeline_source_references: usize,
    pub video_pipeline_source_reference_bytes: u64,
    pub video_pipeline_unique_sources: usize,
    pub video_pipeline_unique_source_bytes: u64,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtkVideoFrameStatsSnapshot {
    pub output_name: String,
    pub frame_stats: VideoFrameStats,
}

struct RenderedOutput {
    output_name: String,
    window: gtk::ApplicationWindow,
    surface: gtk::Box,
    background_provider: Option<gtk::CssProvider>,
    static_surface: Option<RenderedStaticSurface>,
    static_plan: Option<StaticWallpaperPlan>,
    slideshow: Option<RenderedSlideshow>,
    scene_lite_plan: Option<SceneLiteWallpaperPlan>,
    #[cfg(feature = "video-renderer")]
    video: Option<GtkVideoAttachment>,
    #[cfg(feature = "video-renderer")]
    video_error: Option<VideoErrorState>,
}

enum RenderedStaticSurface {
    Picture {
        widget: gtk::Picture,
        source: PathBuf,
    },
    Crossfade {
        stack: gtk::Stack,
        current: gtk::Picture,
        previous: Option<gtk::Picture>,
        source: PathBuf,
        fit: FitMode,
    },
    CssImage {
        source: PathBuf,
    },
    Color,
}

impl GtkStaticRenderer {
    pub fn new(application_id: &str) -> Self {
        let application = gtk::Application::builder()
            .application_id(application_id)
            .build();
        Self {
            application,
            windows: BTreeMap::new(),
            #[cfg(feature = "video-renderer")]
            video_runtimes: GtkVideoRuntimePool::default(),
        }
    }

    pub fn application(&self) -> &gtk::Application {
        &self.application
    }

    pub fn sync_static_render_plan(&mut self, sync: &StaticRenderSyncPlan) {
        let mut desired_outputs = BTreeSet::new();

        for output_name in sync
            .removals
            .iter()
            .chain(sync.errors.iter().map(|failure| &failure.output_name))
        {
            self.remove_output(output_name);
        }

        for plan in &sync.plans {
            #[cfg(feature = "video-renderer")]
            if static_plan_is_video_poster_fallback(plan, &sync.video_plans) {
                continue;
            }
            if self.set_static_wallpaper(plan) {
                desired_outputs.insert(plan.output_name.clone());
            }
        }

        let mut desired_slideshow_outputs = BTreeSet::new();
        for plan in &sync.slideshow_plans {
            if self.set_slideshow_wallpaper(plan) {
                desired_outputs.insert(plan.output_name.clone());
                desired_slideshow_outputs.insert(plan.output_name.clone());
            }
        }
        for output in self.windows.values_mut() {
            if !desired_slideshow_outputs.contains(output.output_name()) {
                output.remove_slideshow();
            }
        }

        for plan in &sync.scene_lite_plans {
            if self.set_scene_lite_wallpaper(plan) {
                desired_outputs.insert(plan.output_name.clone());
            }
        }

        #[cfg(feature = "video-renderer")]
        {
            let mut desired_video_outputs = BTreeSet::new();
            for plan in &sync.video_plans {
                let mode = sync
                    .decisions
                    .iter()
                    .find(|decision| decision.output_name == plan.output_name)
                    .map(|decision| decision.performance.mode)
                    .unwrap_or(RenderMode::Active);
                if self.set_video_wallpaper(plan, mode) {
                    desired_outputs.insert(plan.output_name.clone());
                    desired_video_outputs.insert(plan.output_name.clone());
                }
            }

            for output in self.windows.values_mut() {
                if !desired_video_outputs.contains(output.output_name()) {
                    output.remove_video(&mut self.video_runtimes);
                }
            }
        }

        let stale_outputs = self
            .windows
            .keys()
            .filter(|output_name| !desired_outputs.contains(*output_name))
            .cloned()
            .collect::<Vec<_>>();
        for output_name in stale_outputs {
            self.remove_output(&output_name);
        }
    }

    pub fn set_static_wallpaper(&mut self, plan: &StaticWallpaperPlan) -> bool {
        let Some(monitor) = monitor_for_output(&plan.output_name) else {
            self.remove_output(&plan.output_name);
            return false;
        };
        let window = self
            .windows
            .entry(plan.output_name.clone())
            .or_insert_with(|| {
                build_background_output(&self.application, &plan.output_name, &monitor)
            });
        window.window.set_monitor(Some(&monitor));
        window.remove_slideshow();
        window.scene_lite_plan = None;
        if window.static_surface.is_none()
            || static_plan_needs_update(window.static_plan.as_ref(), plan)
        {
            apply_static_wallpaper(window, plan);
            window.static_plan = Some(plan.clone());
        }
        if !window.window.is_visible() {
            window.window.present();
        }
        true
    }

    pub fn set_slideshow_wallpaper(&mut self, plan: &SlideshowWallpaperPlan) -> bool {
        let Some(monitor) = monitor_for_output(&plan.output_name) else {
            self.remove_output(&plan.output_name);
            return false;
        };
        let output = self
            .windows
            .entry(plan.output_name.clone())
            .or_insert_with(|| {
                build_background_output(&self.application, &plan.output_name, &monitor)
            });
        output.window.set_monitor(Some(&monitor));
        #[cfg(feature = "video-renderer")]
        output.remove_video(&mut self.video_runtimes);
        output.scene_lite_plan = None;
        output.set_slideshow(plan);
        if !output.window.is_visible() {
            output.window.present();
        }
        true
    }

    pub fn tick_slideshows(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = false;
        for output in self.windows.values_mut() {
            changed |= output.tick_slideshow(now);
        }
        changed
    }

    pub fn next_runtime_tick_interval(&self) -> Option<Duration> {
        let now = Instant::now();
        let next_slideshow_delay = self
            .windows
            .values()
            .filter_map(|output| output.next_slideshow_tick_delay(now))
            .min();
        runtime_tick_interval(self.has_video_runtimes(), next_slideshow_delay)
    }

    pub fn set_scene_lite_wallpaper(&mut self, plan: &SceneLiteWallpaperPlan) -> bool {
        let Some(monitor) = monitor_for_output(&plan.output_name) else {
            self.remove_output(&plan.output_name);
            return false;
        };
        let output = self
            .windows
            .entry(plan.output_name.clone())
            .or_insert_with(|| {
                build_background_output(&self.application, &plan.output_name, &monitor)
            });
        output.window.set_monitor(Some(&monitor));
        output.remove_slideshow();
        #[cfg(feature = "video-renderer")]
        output.remove_video(&mut self.video_runtimes);
        output.set_scene_lite(plan);
        if !output.window.is_visible() {
            output.window.present();
        }
        true
    }

    #[cfg(feature = "video-renderer")]
    pub fn set_video_wallpaper(&mut self, plan: &VideoWallpaperPlan, mode: RenderMode) -> bool {
        let Some(monitor) = monitor_for_output(&plan.output_name) else {
            self.remove_output(&plan.output_name);
            return false;
        };
        let output = self
            .windows
            .entry(plan.output_name.clone())
            .or_insert_with(|| {
                build_background_output(&self.application, &plan.output_name, &monitor)
            });
        output.window.set_monitor(Some(&monitor));
        output.remove_slideshow();
        output.scene_lite_plan = None;

        match output.set_video(plan, mode, &mut self.video_runtimes) {
            Ok(()) => {
                output.video_error = None;
                output.release_static_surface();
            }
            Err(err) => {
                output.note_video_error(plan, err);
                output.apply_video_poster_fallback(plan);
            }
        }
        output.window.present();
        true
    }

    #[cfg(feature = "video-renderer")]
    pub fn poll_video_buses(&mut self) -> bool {
        let (observed_decoder_changed, errors) = self.video_runtimes.poll_buses();
        let had_errors = !errors.is_empty();
        for (key, err) in errors {
            let output_names = self
                .windows
                .iter()
                .filter_map(|(output_name, output)| {
                    output
                        .video
                        .as_ref()
                        .filter(|video| video.key == key)
                        .map(|_| output_name.clone())
                })
                .collect::<Vec<_>>();
            for output_name in output_names {
                if let Some(output) = self.windows.get_mut(&output_name) {
                    let fallback = output.video.as_ref().and_then(|video| {
                        video.poster.as_ref().map(|poster| StaticWallpaperPlan {
                            output_name: output.output_name.clone(),
                            source: poster.clone(),
                            fit: video.fit,
                            background: Some("#000000".to_owned()),
                        })
                    });
                    output.note_video_error_for_current_source(err.clone());
                    output.remove_video(&mut self.video_runtimes);
                    if let Some(fallback) = fallback {
                        output.apply_static_fallback(&fallback);
                    } else {
                        output.restore_static_surface();
                    }
                }
            }
        }
        observed_decoder_changed || had_errors
    }

    pub fn resource_snapshot(&self) -> GtkRendererResourceSnapshot {
        let footprint = renderer_surface_resource_footprint(self.windows.values().map(|output| {
            RendererSurfaceResourceSources {
                static_surface_source: output.static_surface_source(),
                slideshow_sources: output
                    .slideshow
                    .as_ref()
                    .map(|slideshow| slideshow.plan.sources.as_slice()),
                video_pipeline_source: output.video_pipeline_source(),
            }
        }));

        GtkRendererResourceSnapshot {
            output_windows: self.windows.len(),
            static_surfaces: self
                .windows
                .values()
                .filter(|output| output.static_surface.is_some())
                .count(),
            static_picture_surfaces: self
                .windows
                .values()
                .filter(|output| output.static_surface_is_picture())
                .count(),
            static_css_surfaces: self
                .windows
                .values()
                .filter(|output| output.static_surface_is_css_image())
                .count(),
            static_color_surfaces: self
                .windows
                .values()
                .filter(|output| output.static_surface_is_color())
                .count(),
            slideshow_surfaces: self
                .windows
                .values()
                .filter(|output| output.slideshow.is_some())
                .count(),
            video_surfaces: self
                .windows
                .values()
                .filter(|output| output.has_video_surface())
                .count(),
            static_surface_resource_references: footprint.static_surface_resource_references,
            static_surface_resource_bytes: footprint.static_surface_resource_bytes,
            static_surface_unique_resources: footprint.static_surface_unique_resources,
            static_surface_unique_resource_bytes: footprint.static_surface_unique_resource_bytes,
            static_surface_estimated_decoded_bytes: self
                .windows
                .values()
                .map(RenderedOutput::static_surface_estimated_decoded_bytes)
                .sum(),
            slideshow_resource_references: footprint.slideshow_resource_references,
            slideshow_resource_bytes: footprint.slideshow_resource_bytes,
            slideshow_unique_resources: footprint.slideshow_unique_resources,
            slideshow_unique_resource_bytes: footprint.slideshow_unique_resource_bytes,
            video_shared_runtimes: self.video_shared_runtime_count(),
            video_pipeline_source_references: footprint.video_pipeline_source_references,
            video_pipeline_source_reference_bytes: footprint.video_pipeline_source_reference_bytes,
            video_pipeline_unique_sources: footprint.video_pipeline_unique_sources,
            video_pipeline_unique_source_bytes: footprint.video_pipeline_unique_source_bytes,
        }
    }

    #[cfg(feature = "video-renderer")]
    pub fn snapshot(&self) -> Vec<VideoPipelineSnapshot> {
        let mut runtime_snapshots = BTreeMap::new();
        self.windows
            .iter()
            .filter_map(|(output_name, output)| {
                output.video.as_ref().and_then(|video| {
                    self.video_runtimes.snapshot_for_attachment(
                        output_name,
                        video,
                        &mut runtime_snapshots,
                    )
                })
            })
            .collect()
    }

    #[cfg(feature = "video-renderer")]
    pub fn video_frame_stats_snapshot(&self) -> Vec<GtkVideoFrameStatsSnapshot> {
        self.windows
            .iter()
            .filter_map(|(output_name, output)| {
                output.video.as_ref().and_then(|video| {
                    self.video_runtimes
                        .frame_stats_for_attachment(video)
                        .map(|frame_stats| GtkVideoFrameStatsSnapshot {
                            output_name: output_name.clone(),
                            frame_stats,
                        })
                })
            })
            .collect()
    }

    pub fn remove_output(&mut self, output_name: &str) {
        if let Some(mut output) = self.windows.remove(output_name) {
            #[cfg(feature = "video-renderer")]
            output.remove_video(&mut self.video_runtimes);
            output.release_static_surface();
            output.static_plan = None;
            output.remove_slideshow();
            output.scene_lite_plan = None;
            output.window.close();
        }
    }
}

#[cfg(feature = "video-renderer")]
impl GtkStaticRenderer {
    fn video_shared_runtime_count(&self) -> usize {
        self.video_runtimes.len()
    }

    pub fn has_video_runtimes(&self) -> bool {
        self.video_runtimes.len() > 0
    }
}

#[cfg(not(feature = "video-renderer"))]
impl GtkStaticRenderer {
    fn video_shared_runtime_count(&self) -> usize {
        0
    }

    pub fn has_video_runtimes(&self) -> bool {
        false
    }
}

fn clamp_runtime_tick_interval(delay: Duration) -> Duration {
    delay.max(GTK_ACTIVE_RUNTIME_TICK_INTERVAL)
}

fn runtime_tick_interval(
    has_video_runtimes: bool,
    next_slideshow_delay: Option<Duration>,
) -> Option<Duration> {
    let slideshow_interval = next_slideshow_delay.map(clamp_runtime_tick_interval);
    Some(match (has_video_runtimes, slideshow_interval) {
        (true, Some(slideshow_interval)) => slideshow_interval.min(GTK_VIDEO_RUNTIME_TICK_INTERVAL),
        (true, None) => GTK_VIDEO_RUNTIME_TICK_INTERVAL,
        (false, Some(slideshow_interval)) => slideshow_interval,
        (false, None) => return None,
    })
}

impl RenderedOutput {
    fn output_name(&self) -> &str {
        &self.output_name
    }

    fn set_slideshow(&mut self, plan: &SlideshowWallpaperPlan) {
        let needs_update = self
            .slideshow
            .as_ref()
            .map(|slideshow| slideshow.plan != *plan)
            .unwrap_or(true);
        if needs_update {
            let slideshow = RenderedSlideshow {
                plan: plan.clone(),
                index: 0,
                next_frame_at: Instant::now() + Duration::from_millis(plan.interval_ms),
                transition_cleanup_at: None,
            };
            self.slideshow = Some(slideshow);
            self.apply_slideshow_frame(false);
        }
    }

    fn tick_slideshow(&mut self, now: Instant) -> bool {
        let mut changed = self.cleanup_slideshow_transition(now);
        let Some(slideshow) = &mut self.slideshow else {
            return false;
        };
        if slideshow.plan.sources.len() < 2 || now < slideshow.next_frame_at {
            return changed;
        }
        slideshow.index = (slideshow.index + 1) % slideshow.plan.sources.len();
        slideshow.next_frame_at = now + Duration::from_millis(slideshow.plan.interval_ms);
        self.apply_slideshow_frame(true);
        changed = true;
        changed
    }

    fn next_slideshow_tick_delay(&self, now: Instant) -> Option<Duration> {
        let slideshow = self.slideshow.as_ref()?;
        if slideshow.plan.sources.len() < 2 {
            return None;
        }
        let frame_delay = slideshow.next_frame_at.saturating_duration_since(now);
        let cleanup_delay = slideshow
            .transition_cleanup_at
            .map(|cleanup_at| cleanup_at.saturating_duration_since(now));
        Some(
            cleanup_delay
                .map(|delay| delay.min(frame_delay))
                .unwrap_or(frame_delay),
        )
    }

    fn apply_slideshow_frame(&mut self, animate_transition: bool) {
        let Some((output_name, source, fit, transition)) =
            self.slideshow.as_ref().and_then(|slideshow| {
                slideshow.plan.sources.get(slideshow.index).map(|source| {
                    (
                        slideshow.plan.output_name.clone(),
                        source.clone(),
                        slideshow.plan.fit,
                        slideshow.plan.transition,
                    )
                })
            })
        else {
            return;
        };
        let static_plan = StaticWallpaperPlan {
            output_name,
            source,
            fit,
            background: Some("#000000".to_owned()),
        };
        let cleanup_at = if slideshow_uses_crossfade(transition, fit) {
            apply_slideshow_crossfade_wallpaper(self, &static_plan, animate_transition)
        } else {
            apply_static_wallpaper(self, &static_plan);
            None
        };
        if let Some(slideshow) = &mut self.slideshow {
            slideshow.transition_cleanup_at = cleanup_at;
        }
        self.static_plan = None;
    }

    fn remove_slideshow(&mut self) {
        self.slideshow = None;
    }

    fn cleanup_slideshow_transition(&mut self, now: Instant) -> bool {
        let cleanup_due = self
            .slideshow
            .as_ref()
            .and_then(|slideshow| slideshow.transition_cleanup_at)
            .is_some_and(|cleanup_at| now >= cleanup_at);
        if !cleanup_due {
            return false;
        }
        if let Some(slideshow) = &mut self.slideshow {
            slideshow.transition_cleanup_at = None;
        }
        clear_crossfade_previous(&mut self.static_surface)
    }

    fn set_scene_lite(&mut self, plan: &SceneLiteWallpaperPlan) {
        if self.scene_lite_plan.as_ref() == Some(plan) {
            return;
        }
        match &plan.display {
            Some(SceneLiteDisplayPlan::Image {
                source,
                fit,
                background,
            }) => {
                let static_plan = StaticWallpaperPlan {
                    output_name: plan.output_name.clone(),
                    source: source.clone(),
                    fit: *fit,
                    background: background.clone(),
                };
                apply_static_wallpaper(self, &static_plan);
            }
            Some(SceneLiteDisplayPlan::Color { color }) => {
                apply_color_wallpaper(self, &plan.output_name, color);
            }
            None => {
                apply_color_wallpaper(self, &plan.output_name, "#000000");
            }
        }
        self.static_plan = None;
        self.scene_lite_plan = Some(plan.clone());
    }

    fn release_static_surface(&mut self) {
        match self.static_surface.take() {
            Some(RenderedStaticSurface::Picture { widget, .. }) => {
                self.surface.remove(&widget);
            }
            Some(RenderedStaticSurface::Crossfade { stack, .. }) => {
                self.surface.remove(&stack);
            }
            Some(RenderedStaticSurface::CssImage { .. } | RenderedStaticSurface::Color) | None => {}
        }
        self.static_surface = None;
        self.clear_background_provider();
    }

    fn clear_background_provider(&mut self) {
        let Some(provider) = self.background_provider.take() else {
            return;
        };
        let display = gtk::prelude::WidgetExt::display(&self.window);
        gtk::style_context_remove_provider_for_display(&display, &provider);
    }

    #[cfg(feature = "video-renderer")]
    fn restore_static_surface(&mut self) {
        if self.static_surface.is_some() {
            return;
        }
        if let Some(plan) = self.static_plan.clone() {
            apply_static_wallpaper(self, &plan);
        }
    }

    #[cfg(feature = "video-renderer")]
    fn apply_video_poster_fallback(&mut self, plan: &VideoWallpaperPlan) {
        let Some(poster) = &plan.poster else {
            self.restore_static_surface();
            return;
        };
        let fallback = StaticWallpaperPlan {
            output_name: plan.output_name.clone(),
            source: poster.clone(),
            fit: plan.fit,
            background: Some("#000000".to_owned()),
        };
        self.apply_static_fallback(&fallback);
    }

    #[cfg(feature = "video-renderer")]
    fn apply_static_fallback(&mut self, plan: &StaticWallpaperPlan) {
        if self.static_surface.is_none()
            || static_plan_needs_update(self.static_plan.as_ref(), plan)
        {
            apply_static_wallpaper(self, plan);
            self.static_plan = Some(plan.clone());
        }
    }

    fn static_surface_source(&self) -> Option<&Path> {
        self.static_surface.as_ref()?;
        if let Some(slideshow) = &self.slideshow {
            return slideshow
                .plan
                .sources
                .get(slideshow.index)
                .map(|source| source.as_path());
        }
        if let Some(scene_lite) = &self.scene_lite_plan
            && let Some(SceneLiteDisplayPlan::Image { source, .. }) = &scene_lite.display
        {
            return Some(source.as_path());
        }
        match self.static_surface.as_ref()? {
            RenderedStaticSurface::Picture { source, .. }
            | RenderedStaticSurface::Crossfade { source, .. }
            | RenderedStaticSurface::CssImage { source } => Some(source.as_path()),
            RenderedStaticSurface::Color => None,
        }
    }

    fn static_surface_is_picture(&self) -> bool {
        matches!(
            self.static_surface.as_ref(),
            Some(RenderedStaticSurface::Picture { .. } | RenderedStaticSurface::Crossfade { .. })
        )
    }

    fn static_surface_is_css_image(&self) -> bool {
        matches!(
            self.static_surface.as_ref(),
            Some(RenderedStaticSurface::CssImage { .. })
        )
    }

    fn static_surface_is_color(&self) -> bool {
        matches!(
            self.static_surface.as_ref(),
            Some(RenderedStaticSurface::Color)
        )
    }

    fn static_surface_estimated_decoded_bytes(&self) -> u64 {
        self.static_surface
            .as_ref()
            .map(RenderedStaticSurface::estimated_decoded_bytes)
            .unwrap_or(0)
    }

    #[cfg(feature = "video-renderer")]
    fn video_pipeline_source(&self) -> Option<&Path> {
        self.video.as_ref().map(|video| video.source.as_path())
    }

    #[cfg(not(feature = "video-renderer"))]
    fn video_pipeline_source(&self) -> Option<&Path> {
        None
    }

    #[cfg(feature = "video-renderer")]
    fn has_video_surface(&self) -> bool {
        self.video.is_some()
    }

    #[cfg(not(feature = "video-renderer"))]
    fn has_video_surface(&self) -> bool {
        false
    }

    #[cfg(feature = "video-renderer")]
    fn set_video(
        &mut self,
        plan: &VideoWallpaperPlan,
        mode: RenderMode,
        runtimes: &mut GtkVideoRuntimePool,
    ) -> Result<(), GtkVideoError> {
        let output_name = self.output_name.clone();
        let key = GtkVideoRuntimeKey::from_plan(plan);
        let restart = self.video.as_ref().is_none_or(|video| video.key != key);
        if restart {
            self.remove_video(runtimes);
            let video = runtimes.attach_output(&output_name, plan, mode)?;
            self.surface.append(video.widget());
            self.video = Some(video);
        }

        let Some(video) = &mut self.video else {
            return Err(GtkVideoError::MissingPipeline);
        };
        video.apply_plan(plan);
        video.mode = mode;
        runtimes.apply_output_mode(&key, &output_name, mode)?;
        Ok(())
    }

    #[cfg(feature = "video-renderer")]
    fn remove_video(&mut self, runtimes: &mut GtkVideoRuntimePool) {
        if let Some(video) = self.video.take() {
            self.surface.remove(video.widget());
            runtimes.detach_output(&video.key, &video.output_name);
        }
    }

    #[cfg(feature = "video-renderer")]
    fn note_video_error(&mut self, plan: &VideoWallpaperPlan, err: GtkVideoError) {
        let error = VideoErrorState {
            source: plan.source.clone(),
            message: err.to_string(),
        };
        if self.video_error.as_ref() != Some(&error) {
            eprintln!(
                "gilderd: video surface renderer unavailable for {}: {}",
                plan.output_name, error.message
            );
        }
        self.video_error = Some(error);
    }

    #[cfg(feature = "video-renderer")]
    fn note_video_error_for_current_source(&mut self, err: GtkVideoError) {
        let source = self
            .video
            .as_ref()
            .map(|video| video.source.clone())
            .unwrap_or_default();
        let error = VideoErrorState {
            source,
            message: err.to_string(),
        };
        if self.video_error.as_ref() != Some(&error) {
            eprintln!(
                "gilderd: video surface renderer pipeline error for {}: {}",
                self.output_name(),
                error.message
            );
        }
        self.video_error = Some(error);
    }
}

impl RenderedStaticSurface {
    fn estimated_decoded_bytes(&self) -> u64 {
        match self {
            Self::Picture { widget, .. } => widget
                .paintable()
                .as_ref()
                .map(paintable_estimated_decoded_bytes)
                .unwrap_or(0),
            Self::Crossfade {
                current, previous, ..
            } => {
                let current_bytes = current
                    .paintable()
                    .as_ref()
                    .map(paintable_estimated_decoded_bytes)
                    .unwrap_or(0);
                let previous_bytes = previous
                    .as_ref()
                    .and_then(gtk::Picture::paintable)
                    .as_ref()
                    .map(paintable_estimated_decoded_bytes)
                    .unwrap_or(0);
                current_bytes.saturating_add(previous_bytes)
            }
            Self::CssImage { .. } | Self::Color => 0,
        }
    }
}

pub fn can_read_gdk_desktop_outputs() -> bool {
    gtk::is_initialized_main_thread() && gdk::Display::default().is_some()
}

pub fn gdk_desktop_outputs() -> Vec<crate::desktop::DesktopOutput> {
    if !can_read_gdk_desktop_outputs() {
        return Vec::new();
    }
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    let mut outputs = Vec::new();
    for index in 0..monitors.n_items() {
        let Some(item) = monitors.item(index) else {
            continue;
        };
        let Ok(monitor) = item.downcast::<gdk::Monitor>() else {
            continue;
        };
        let geometry = monitor.geometry();
        let name = monitor_output_name(&monitor, index);
        outputs.push(crate::desktop::DesktopOutput {
            name,
            make: monitor.manufacturer().map(|value| value.to_string()),
            model: monitor.model().map(|value| value.to_string()),
            width: u32::try_from(geometry.width()).ok(),
            height: u32::try_from(geometry.height()).ok(),
            scale: monitor.scale_factor() as f32,
            focused: index == 0,
            visible: true,
            has_fullscreen: false,
            active_workspace: None,
        });
    }
    outputs
}

fn build_background_output(
    application: &gtk::Application,
    output_name: &str,
    monitor: &gdk::Monitor,
) -> RenderedOutput {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .decorated(false)
        .resizable(false)
        .focusable(false)
        .title(format!("Gilder Wallpaper {output_name}"))
        .build();
    window.init_layer_shell();
    window.set_namespace(Some("gilder-wallpaper"));
    window.set_layer(Layer::Background);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }

    let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
    surface.set_hexpand(true);
    surface.set_vexpand(true);
    surface.set_widget_name(&css_widget_name(output_name));
    window.set_child(Some(&surface));
    RenderedOutput {
        output_name: output_name.to_owned(),
        window,
        surface,
        background_provider: None,
        static_surface: None,
        static_plan: None,
        slideshow: None,
        scene_lite_plan: None,
        #[cfg(feature = "video-renderer")]
        video: None,
        #[cfg(feature = "video-renderer")]
        video_error: None,
    }
}

fn apply_static_wallpaper(output: &mut RenderedOutput, plan: &StaticWallpaperPlan) {
    output.release_static_surface();
    if use_picture_static_surface(plan.fit) {
        apply_wallpaper_css(
            output,
            &color_wallpaper_css(
                &plan.output_name,
                plan.background.as_deref().unwrap_or("#000000"),
            ),
        );
        let picture = wallpaper_picture(&plan.source, plan.fit);
        output.surface.append(&picture);
        output.static_surface = Some(RenderedStaticSurface::Picture {
            widget: picture,
            source: plan.source.clone(),
        });
    } else {
        apply_wallpaper_css(output, &static_wallpaper_css(plan));
        output.static_surface = Some(RenderedStaticSurface::CssImage {
            source: plan.source.clone(),
        });
    }
}

fn apply_slideshow_crossfade_wallpaper(
    output: &mut RenderedOutput,
    plan: &StaticWallpaperPlan,
    animate_transition: bool,
) -> Option<Instant> {
    apply_wallpaper_css(
        output,
        &color_wallpaper_css(
            &plan.output_name,
            plan.background.as_deref().unwrap_or("#000000"),
        ),
    );

    match output.static_surface.as_mut() {
        Some(RenderedStaticSurface::Crossfade {
            stack,
            current,
            previous,
            source,
            fit,
        }) => {
            if *source == plan.source {
                current.set_content_fit(content_fit_for_fit(plan.fit));
                *fit = plan.fit;
                if let Some(previous_picture) = previous.take() {
                    stack.remove(&previous_picture);
                }
                return None;
            }
            if let Some(previous_picture) = previous.take() {
                stack.remove(&previous_picture);
            }
            let next = wallpaper_picture(&plan.source, plan.fit);
            stack.add_child(&next);
            stack.set_transition_type(if animate_transition {
                gtk::StackTransitionType::Crossfade
            } else {
                gtk::StackTransitionType::None
            });
            stack.set_transition_duration(SLIDESHOW_CROSSFADE_DURATION.as_millis() as u32);
            stack.set_visible_child(&next);
            let old_current = std::mem::replace(current, next);
            *previous = Some(old_current);
            *source = plan.source.clone();
            *fit = plan.fit;
            animate_transition.then(|| Instant::now() + SLIDESHOW_CROSSFADE_DURATION)
        }
        _ => {
            output.release_static_surface();
            let stack = gtk::Stack::new();
            stack.set_hexpand(true);
            stack.set_vexpand(true);
            stack.set_transition_type(gtk::StackTransitionType::None);
            stack.set_transition_duration(SLIDESHOW_CROSSFADE_DURATION.as_millis() as u32);
            let current = wallpaper_picture(&plan.source, plan.fit);
            stack.add_child(&current);
            stack.set_visible_child(&current);
            output.surface.append(&stack);
            output.static_surface = Some(RenderedStaticSurface::Crossfade {
                stack,
                current,
                previous: None,
                source: plan.source.clone(),
                fit: plan.fit,
            });
            None
        }
    }
}

fn wallpaper_picture(source: &Path, fit: FitMode) -> gtk::Picture {
    let file = gio::File::for_path(source);
    let picture = gtk::Picture::for_file(&file);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_can_shrink(false);
    picture.set_content_fit(content_fit_for_fit(fit));
    picture
}

fn clear_crossfade_previous(surface: &mut Option<RenderedStaticSurface>) -> bool {
    let Some(RenderedStaticSurface::Crossfade {
        stack, previous, ..
    }) = surface.as_mut()
    else {
        return false;
    };
    let Some(previous_picture) = previous.take() else {
        return false;
    };
    stack.remove(&previous_picture);
    true
}

fn slideshow_uses_crossfade(transition: Transition, fit: FitMode) -> bool {
    transition == Transition::Crossfade && use_picture_static_surface(fit)
}

fn apply_color_wallpaper(output: &mut RenderedOutput, output_name: &str, color: &str) {
    output.release_static_surface();
    apply_wallpaper_css(output, &color_wallpaper_css(output_name, color));
    output.static_surface = Some(RenderedStaticSurface::Color);
}

fn apply_wallpaper_css(output: &mut RenderedOutput, css: &str) {
    let display = gtk::prelude::WidgetExt::display(&output.window);
    output.clear_background_provider();
    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    output.background_provider = Some(provider);
}

fn static_plan_needs_update(
    previous: Option<&StaticWallpaperPlan>,
    next: &StaticWallpaperPlan,
) -> bool {
    previous != Some(next)
}

#[cfg(feature = "video-renderer")]
fn static_plan_is_video_poster_fallback(
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RendererSurfaceResourceFootprint {
    static_surface_resource_references: usize,
    static_surface_resource_bytes: u64,
    static_surface_unique_resources: usize,
    static_surface_unique_resource_bytes: u64,
    slideshow_resource_references: usize,
    slideshow_resource_bytes: u64,
    slideshow_unique_resources: usize,
    slideshow_unique_resource_bytes: u64,
    video_pipeline_source_references: usize,
    video_pipeline_source_reference_bytes: u64,
    video_pipeline_unique_sources: usize,
    video_pipeline_unique_source_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct RendererSurfaceResourceSources<'a> {
    static_surface_source: Option<&'a Path>,
    slideshow_sources: Option<&'a [PathBuf]>,
    video_pipeline_source: Option<&'a Path>,
}

fn renderer_surface_resource_footprint<'a>(
    outputs: impl IntoIterator<Item = RendererSurfaceResourceSources<'a>>,
) -> RendererSurfaceResourceFootprint {
    let mut footprint = RendererSurfaceResourceFootprint::default();
    let mut source_sizes = BTreeMap::new();
    let mut static_unique_sources = BTreeSet::new();
    let mut slideshow_unique_sources = BTreeSet::new();
    let mut video_unique_sources = BTreeSet::new();
    for output in outputs {
        if let Some(source) = output.static_surface_source {
            footprint.static_surface_resource_references += 1;
            footprint.static_surface_resource_bytes +=
                cached_source_file_size(&mut source_sizes, source);
            static_unique_sources.insert(source.to_path_buf());
        }
        if let Some(sources) = output.slideshow_sources {
            footprint.slideshow_resource_references += sources.len();
            footprint.slideshow_resource_bytes += sources
                .iter()
                .map(|source| cached_source_file_size(&mut source_sizes, source))
                .sum::<u64>();
            slideshow_unique_sources.extend(sources.iter().cloned());
        }
        if let Some(source) = output.video_pipeline_source {
            footprint.video_pipeline_source_references += 1;
            footprint.video_pipeline_source_reference_bytes +=
                cached_source_file_size(&mut source_sizes, source);
            video_unique_sources.insert(source.to_path_buf());
        }
    }
    footprint.static_surface_unique_resources = static_unique_sources.len();
    footprint.static_surface_unique_resource_bytes = static_unique_sources
        .iter()
        .map(|source| cached_source_file_size(&mut source_sizes, source))
        .sum();
    footprint.slideshow_unique_resources = slideshow_unique_sources.len();
    footprint.slideshow_unique_resource_bytes = slideshow_unique_sources
        .iter()
        .map(|source| cached_source_file_size(&mut source_sizes, source))
        .sum();
    footprint.video_pipeline_unique_sources = video_unique_sources.len();
    footprint.video_pipeline_unique_source_bytes = video_unique_sources
        .iter()
        .map(|source| cached_source_file_size(&mut source_sizes, source))
        .sum();
    footprint
}

fn cached_source_file_size(cache: &mut BTreeMap<PathBuf, u64>, path: &Path) -> u64 {
    if let Some(size) = cache.get(path) {
        return *size;
    }
    let size = source_file_size(path);
    cache.insert(path.to_path_buf(), size);
    size
}

fn source_file_size(path: &Path) -> u64 {
    fs_metadata_file_len(path).unwrap_or(0)
}

fn fs_metadata_file_len(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then_some(metadata.len())
}

fn monitor_for_output(output_name: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    for index in 0..monitors.n_items() {
        let Some(item) = monitors.item(index) else {
            continue;
        };
        let Ok(monitor) = item.downcast::<gdk::Monitor>() else {
            continue;
        };
        if monitor_output_name(&monitor, index) == output_name {
            return Some(monitor);
        }
    }
    None
}

fn monitor_output_name(monitor: &gdk::Monitor, index: u32) -> String {
    monitor
        .connector()
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("gdk-monitor-{index}"))
}

fn static_wallpaper_css(plan: &StaticWallpaperPlan) -> String {
    let file = gio::File::for_path(&plan.source);
    let uri = file.uri();
    let background = plan.background.as_deref().unwrap_or("#000000");
    let mode = css_background_mode(plan.fit);
    format!(
        "#{widget} {{
            background-color: {background};
            background-image: url(\"{uri}\");
            background-position: {position};
            background-repeat: {repeat};
            background-size: {size};
        }}",
        widget = css_widget_name(&plan.output_name),
        position = mode.position,
        repeat = mode.repeat,
        size = mode.size,
    )
}

fn color_wallpaper_css(output_name: &str, color: &str) -> String {
    format!(
        "#{widget} {{
            background-color: {color};
            background-image: none;
        }}",
        widget = css_widget_name(output_name),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CssBackgroundMode {
    position: &'static str,
    repeat: &'static str,
    size: &'static str,
}

fn css_background_mode(fit: FitMode) -> CssBackgroundMode {
    match fit {
        FitMode::Cover => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "cover",
        },
        FitMode::Contain => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "contain",
        },
        FitMode::Stretch => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "100% 100%",
        },
        FitMode::Tile => CssBackgroundMode {
            position: "top left",
            repeat: "repeat",
            size: "auto",
        },
        FitMode::Center => CssBackgroundMode {
            position: "center",
            repeat: "no-repeat",
            size: "auto",
        },
    }
}

fn css_widget_name(output_name: &str) -> String {
    let mut name = String::from("gilder-wallpaper-");
    for character in output_name.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            name.push(character);
        } else {
            name.push('-');
        }
    }
    name
}

fn paintable_estimated_decoded_bytes(paintable: &gdk::Paintable) -> u64 {
    estimated_rgba_decoded_bytes(paintable.intrinsic_width(), paintable.intrinsic_height())
}

fn estimated_rgba_decoded_bytes(width: i32, height: i32) -> u64 {
    if width <= 0 || height <= 0 {
        return 0;
    }
    (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(4)
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct VideoErrorState {
    source: std::path::PathBuf,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedSlideshow {
    plan: SlideshowWallpaperPlan,
    index: usize,
    next_frame_at: Instant,
    transition_cleanup_at: Option<Instant>,
}

#[cfg(feature = "video-renderer")]
#[derive(Default)]
struct GtkVideoRuntimePool {
    runtimes: BTreeMap<GtkVideoRuntimeKey, Rc<RefCell<GtkSharedVideoRuntime>>>,
}

#[cfg(feature = "video-renderer")]
impl GtkVideoRuntimePool {
    fn len(&self) -> usize {
        self.runtimes.len()
    }

    fn attach_output(
        &mut self,
        output_name: &str,
        plan: &VideoWallpaperPlan,
        mode: RenderMode,
    ) -> Result<GtkVideoAttachment, GtkVideoError> {
        let key = GtkVideoRuntimeKey::from_plan(plan);
        if !self.runtimes.contains_key(&key) {
            let runtime = GtkSharedVideoRuntime::new(plan)?;
            self.runtimes
                .insert(key.clone(), Rc::new(RefCell::new(runtime)));
        }
        let runtime = self
            .runtimes
            .get(&key)
            .ok_or(GtkVideoError::MissingPipeline)?;
        let mut runtime = runtime.borrow_mut();
        let attachment = runtime.attach_output(output_name, plan.fit, plan.poster.clone(), mode);
        runtime.apply_output_mode(output_name, mode)?;
        Ok(attachment)
    }

    fn apply_output_mode(
        &mut self,
        key: &GtkVideoRuntimeKey,
        output_name: &str,
        mode: RenderMode,
    ) -> Result<(), GtkVideoError> {
        let runtime = self
            .runtimes
            .get(key)
            .ok_or(GtkVideoError::MissingPipeline)?;
        runtime.borrow_mut().apply_output_mode(output_name, mode)
    }

    fn detach_output(&mut self, key: &GtkVideoRuntimeKey, output_name: &str) {
        let remove_runtime = self
            .runtimes
            .get(key)
            .map(|runtime| {
                let mut runtime = runtime.borrow_mut();
                runtime.detach_output(output_name);
                runtime.is_unused()
            })
            .unwrap_or(false);
        if remove_runtime {
            self.runtimes.remove(key);
        }
    }

    fn poll_buses(&mut self) -> (bool, Vec<(GtkVideoRuntimeKey, GtkVideoError)>) {
        let mut observed_decoder_changed = false;
        let mut errors = Vec::new();
        for (key, runtime) in &self.runtimes {
            match runtime.borrow_mut().poll_bus() {
                Ok(changed) => observed_decoder_changed |= changed,
                Err(err) => errors.push((key.clone(), err)),
            }
        }
        (observed_decoder_changed, errors)
    }

    fn snapshot_for_attachment(
        &self,
        output_name: &str,
        attachment: &GtkVideoAttachment,
        snapshots: &mut BTreeMap<GtkVideoRuntimeKey, GtkSharedVideoRuntimeSnapshot>,
    ) -> Option<VideoPipelineSnapshot> {
        if !snapshots.contains_key(&attachment.key) {
            let runtime = self.runtimes.get(&attachment.key)?;
            let snapshot = runtime.borrow().snapshot();
            snapshots.insert(attachment.key.clone(), snapshot);
        }
        snapshots
            .get(&attachment.key)
            .map(|snapshot| snapshot.for_attachment(output_name, attachment))
    }

    fn frame_stats_for_attachment(
        &self,
        attachment: &GtkVideoAttachment,
    ) -> Option<VideoFrameStats> {
        let runtime = self.runtimes.get(&attachment.key)?;
        let runtime = runtime.borrow();
        Some(merge_video_frame_stats(
            runtime.frame_stats.borrow().clone(),
            attachment.frame_stats.borrow().clone(),
        ))
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GtkVideoRuntimeKey {
    source: PathBuf,
    loop_playback: bool,
    muted: bool,
    decoder_policy: u8,
    start_offset_ms: u64,
    target_max_fps: Option<u32>,
}

#[cfg(feature = "video-renderer")]
impl GtkVideoRuntimeKey {
    fn from_plan(plan: &VideoWallpaperPlan) -> Self {
        Self {
            source: plan.source.clone(),
            loop_playback: plan.loop_playback,
            muted: plan.muted,
            decoder_policy: decoder_policy_key(plan.decoder_policy),
            start_offset_ms: plan.start_offset_ms,
            target_max_fps: plan.target_max_fps,
        }
    }
}

#[cfg(feature = "video-renderer")]
fn decoder_policy_key(policy: crate::config::VideoDecoderPolicy) -> u8 {
    match policy {
        crate::config::VideoDecoderPolicy::Auto => 0,
        crate::config::VideoDecoderPolicy::HardwarePreferred => 1,
        crate::config::VideoDecoderPolicy::HardwareRequired => 2,
        crate::config::VideoDecoderPolicy::Software => 3,
    }
}

#[cfg(feature = "video-renderer")]
struct GtkVideoAttachment {
    key: GtkVideoRuntimeKey,
    output_name: String,
    picture: gtk::Picture,
    source: PathBuf,
    poster: Option<PathBuf>,
    mode: RenderMode,
    fit: FitMode,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
    _frame_clock_observer: Option<GtkFrameClockObserver>,
}

#[cfg(feature = "video-renderer")]
impl GtkVideoAttachment {
    fn widget(&self) -> &gtk::Picture {
        &self.picture
    }

    fn apply_plan(&mut self, plan: &VideoWallpaperPlan) {
        if self.fit != plan.fit {
            self.picture.set_content_fit(content_fit_for_fit(plan.fit));
            self.fit = plan.fit;
        }
        self.poster = plan.poster.clone();
    }
}

#[cfg(feature = "video-renderer")]
struct GtkSharedVideoRuntime {
    element: gst::Element,
    paintable: gdk::Paintable,
    frame_limiter: Option<GtkFrameLimiter>,
    sink_tuning: VideoSinkTuningReport,
    source: PathBuf,
    output_modes: BTreeMap<String, RenderMode>,
    gst_state: gst::State,
    loop_playback: bool,
    muted: bool,
    target_max_fps: Option<u32>,
    decoder_policy: crate::config::VideoDecoderPolicy,
    start_offset_ms: u64,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
    diagnostics: VideoPipelineDiagnosticsCache,
    observed_decoder_reports: BTreeMap<String, crate::renderer::video::VideoDecoderReport>,
}

#[cfg(feature = "video-renderer")]
impl GtkSharedVideoRuntime {
    fn new(plan: &VideoWallpaperPlan) -> Result<Self, GtkVideoError> {
        gst::init().map_err(|err| GtkVideoError::Init(err.to_string()))?;
        let built = build_gtk_video_pipeline(plan)?;
        let mut runtime = Self {
            element: built.element,
            paintable: built.paintable,
            frame_limiter: built.frame_limiter,
            sink_tuning: built.sink_tuning,
            source: plan.source.clone(),
            output_modes: BTreeMap::new(),
            gst_state: gst::State::Null,
            loop_playback: plan.loop_playback,
            muted: !plan.muted,
            target_max_fps: plan.target_max_fps,
            decoder_policy: plan.decoder_policy,
            start_offset_ms: 0,
            frame_stats: Rc::new(RefCell::new(VideoFrameStats::default())),
            diagnostics: VideoPipelineDiagnosticsCache::default(),
            observed_decoder_reports: BTreeMap::new(),
        };
        runtime.apply_muted(plan.muted);
        runtime.apply_start_offset(plan.start_offset_ms)?;
        Ok(runtime)
    }

    fn attach_output(
        &mut self,
        output_name: &str,
        fit: FitMode,
        poster: Option<PathBuf>,
        mode: RenderMode,
    ) -> GtkVideoAttachment {
        let picture = gtk::Picture::for_paintable(&self.paintable);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_can_shrink(false);
        picture.set_content_fit(content_fit_for_fit(fit));
        let frame_stats = Rc::new(RefCell::new(VideoFrameStats::default()));
        let frame_clock_observer = install_frame_clock_stats(
            &picture,
            Rc::clone(&frame_stats),
            gtk_frame_clock_stats_mode(),
        );
        self.output_modes.insert(output_name.to_owned(), mode);
        GtkVideoAttachment {
            key: GtkVideoRuntimeKey {
                source: self.source.clone(),
                loop_playback: self.loop_playback,
                muted: self.muted,
                decoder_policy: decoder_policy_key(self.decoder_policy),
                start_offset_ms: self.start_offset_ms,
                target_max_fps: self.target_max_fps,
            },
            output_name: output_name.to_owned(),
            picture,
            source: self.source.clone(),
            poster,
            mode,
            fit,
            frame_stats,
            _frame_clock_observer: frame_clock_observer,
        }
    }

    fn detach_output(&mut self, output_name: &str) {
        self.output_modes.remove(output_name);
        let _ = self.apply_aggregate_state();
    }

    fn is_unused(&self) -> bool {
        self.output_modes.is_empty()
    }

    fn apply_output_mode(
        &mut self,
        output_name: &str,
        mode: RenderMode,
    ) -> Result<(), GtkVideoError> {
        self.output_modes.insert(output_name.to_owned(), mode);
        self.apply_aggregate_state()
    }

    fn apply_aggregate_state(&mut self) -> Result<(), GtkVideoError> {
        let mode = if self
            .output_modes
            .values()
            .any(|mode| *mode != RenderMode::Paused)
        {
            RenderMode::Active
        } else {
            RenderMode::Paused
        };
        self.set_state(gst_state_for_mode(mode))
    }

    fn poll_bus(&mut self) -> Result<bool, GtkVideoError> {
        let mut observed_decoder_changed = false;
        let Some(bus) = self.element.bus() else {
            return Ok(false);
        };
        while let Some(message) = bus.pop() {
            if let Some(report) = decoder_report_from_message(&message) {
                if !self.observed_decoder_reports.contains_key(&report.element) {
                    self.observed_decoder_reports
                        .insert(report.element.clone(), report);
                    observed_decoder_changed = true;
                }
            }
            match message.view() {
                gst::MessageView::Eos(_) => {
                    if self.loop_playback {
                        self.element
                            .seek_simple(
                                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                gst::ClockTime::ZERO,
                            )
                            .map_err(|err| GtkVideoError::Seek(err.to_string()))?;
                        if self
                            .output_modes
                            .values()
                            .any(|mode| *mode != RenderMode::Paused)
                        {
                            self.set_state(gst::State::Playing)?;
                        }
                    } else {
                        self.set_state(gst::State::Paused)?;
                    }
                }
                gst::MessageView::Error(err) => {
                    return Err(GtkVideoError::Pipeline(format!(
                        "{}: {}",
                        err.error(),
                        err.debug().unwrap_or_default()
                    )));
                }
                gst::MessageView::Qos(qos) => {
                    let (processed, dropped) = qos.stats();
                    let (jitter, proportion, _) = qos.values();
                    self.frame_stats.borrow_mut().record_qos_values(
                        processed.format().to_string(),
                        processed.value(),
                        dropped.value(),
                        jitter,
                        proportion,
                    );
                }
                _ => {}
            }
        }
        Ok(observed_decoder_changed)
    }

    fn set_state(&mut self, state: gst::State) -> Result<(), GtkVideoError> {
        if self.gst_state == state {
            return Ok(());
        }
        self.element
            .set_state(state)
            .map_err(|err| GtkVideoError::SetState(err.to_string()))?;
        self.gst_state = state;
        self.diagnostics.invalidate();
        Ok(())
    }

    fn apply_muted(&mut self, muted: bool) {
        if self.muted == muted {
            return;
        }
        self.muted = muted;
        if self.element.find_property("mute").is_some() {
            self.element.set_property("mute", muted);
        }
    }

    fn apply_start_offset(&mut self, start_offset_ms: u64) -> Result<(), GtkVideoError> {
        if self.start_offset_ms == start_offset_ms {
            return Ok(());
        }
        self.element
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(start_offset_ms),
            )
            .map_err(|err| GtkVideoError::Seek(err.to_string()))?;
        self.start_offset_ms = start_offset_ms;
        Ok(())
    }

    fn snapshot(&self) -> GtkSharedVideoRuntimeSnapshot {
        let VideoPipelineDiagnostics {
            actual_decoder_reports: current_decoder_reports,
            caps_reports,
            allocation_reports,
            queue_reports,
            zero_copy_evidence: _,
            memory_path: _,
        } = self.diagnostics.snapshot(&self.element);
        let actual_decoder_reports = merge_decoder_reports(
            current_decoder_reports,
            self.observed_decoder_reports.values().cloned(),
        );
        let zero_copy_evidence =
            crate::renderer::video::zero_copy_evidence(&actual_decoder_reports, &caps_reports);
        let memory_path =
            crate::renderer::video::video_memory_path(&actual_decoder_reports, &caps_reports);
        let retention_report =
            video_memory_retention_report(&memory_path, &allocation_reports, &self.sink_tuning);
        let frame_limiter_max_fps = self
            .frame_limiter
            .as_ref()
            .and_then(GtkFrameLimiter::target_max_fps);
        GtkSharedVideoRuntimeSnapshot {
            source: self.source.to_string_lossy().into_owned(),
            gst_state: self.gst_state.name().to_string(),
            loop_playback: self.loop_playback,
            muted: self.muted,
            target_max_fps: self.target_max_fps,
            sink_tuning: self.sink_tuning.clone(),
            frame_limiter_enabled: frame_limiter_max_fps.is_some(),
            frame_limiter_max_fps,
            frame_stats: self.frame_stats.borrow().clone(),
            decoder_policy: self.decoder_policy,
            decoder_policy_status: decoder_policy_status(
                self.decoder_policy,
                &actual_decoder_reports,
            ),
            start_offset_ms: self.start_offset_ms,
            position_ms: playback_position_ms(&self.element),
            duration_ms: playback_duration_ms(&self.element),
            actual_decoders: actual_decoder_reports
                .iter()
                .map(|report| report.element.clone())
                .collect(),
            actual_decoder_reports,
            caps_reports,
            allocation_reports,
            queue_reports,
            zero_copy_evidence,
            memory_path,
            retention_report,
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Clone)]
struct GtkSharedVideoRuntimeSnapshot {
    source: String,
    gst_state: String,
    loop_playback: bool,
    muted: bool,
    target_max_fps: Option<u32>,
    sink_tuning: VideoSinkTuningReport,
    frame_limiter_enabled: bool,
    frame_limiter_max_fps: Option<u32>,
    frame_stats: VideoFrameStats,
    decoder_policy: crate::config::VideoDecoderPolicy,
    decoder_policy_status: VideoDecoderPolicyStatus,
    start_offset_ms: u64,
    position_ms: Option<u64>,
    duration_ms: Option<u64>,
    actual_decoders: Vec<String>,
    actual_decoder_reports: Vec<VideoDecoderReport>,
    caps_reports: Vec<VideoCapsReport>,
    allocation_reports: Vec<VideoAllocationReport>,
    queue_reports: Vec<VideoQueueReport>,
    zero_copy_evidence: VideoZeroCopyEvidence,
    memory_path: VideoMemoryPathReport,
    retention_report: VideoMemoryRetentionReport,
}

#[cfg(feature = "video-renderer")]
impl GtkSharedVideoRuntimeSnapshot {
    fn for_attachment(
        &self,
        output_name: &str,
        attachment: &GtkVideoAttachment,
    ) -> VideoPipelineSnapshot {
        VideoPipelineSnapshot {
            output_name: output_name.to_owned(),
            source: self.source.clone(),
            mode: attachment.mode,
            gst_state: self.gst_state.clone(),
            loop_playback: self.loop_playback,
            muted: self.muted,
            target_max_fps: self.target_max_fps,
            sink_tuning: self.sink_tuning.clone(),
            frame_limiter_enabled: self.frame_limiter_enabled,
            frame_limiter_max_fps: self.frame_limiter_max_fps,
            frame_stats: merge_video_frame_stats(
                self.frame_stats.clone(),
                attachment.frame_stats.borrow().clone(),
            ),
            decoder_policy: self.decoder_policy,
            decoder_policy_status: self.decoder_policy_status,
            start_offset_ms: self.start_offset_ms,
            position_ms: self.position_ms,
            duration_ms: self.duration_ms,
            actual_decoders: self.actual_decoders.clone(),
            actual_decoder_reports: self.actual_decoder_reports.clone(),
            caps_reports: self.caps_reports.clone(),
            allocation_reports: self.allocation_reports.clone(),
            queue_reports: self.queue_reports.clone(),
            zero_copy_evidence: self.zero_copy_evidence.clone(),
            memory_path: self.memory_path.clone(),
            retention_report: self.retention_report.clone(),
        }
    }
}

#[cfg(feature = "video-renderer")]
fn merge_video_frame_stats(
    mut pipeline_stats: VideoFrameStats,
    output_stats: VideoFrameStats,
) -> VideoFrameStats {
    pipeline_stats.gtk_frame_clock_ticks = output_stats.gtk_frame_clock_ticks;
    pipeline_stats.gtk_frame_clock_before_paint_ticks =
        output_stats.gtk_frame_clock_before_paint_ticks;
    pipeline_stats.gtk_frame_clock_update_ticks = output_stats.gtk_frame_clock_update_ticks;
    pipeline_stats.gtk_frame_clock_layout_ticks = output_stats.gtk_frame_clock_layout_ticks;
    pipeline_stats.gtk_frame_clock_paint_ticks = output_stats.gtk_frame_clock_paint_ticks;
    pipeline_stats.gtk_frame_clock_after_paint_ticks =
        output_stats.gtk_frame_clock_after_paint_ticks;
    pipeline_stats.gtk_frame_clock_counter_latest = output_stats.gtk_frame_clock_counter_latest;
    pipeline_stats.gtk_frame_clock_time_us_latest = output_stats.gtk_frame_clock_time_us_latest;
    pipeline_stats.gtk_frame_clock_interval_us_latest =
        output_stats.gtk_frame_clock_interval_us_latest;
    pipeline_stats.gtk_frame_clock_interval_us_max = output_stats.gtk_frame_clock_interval_us_max;
    pipeline_stats.gtk_frame_clock_fps_x1000_latest = output_stats.gtk_frame_clock_fps_x1000_latest;
    pipeline_stats.gtk_frame_clock_refresh_interval_us_latest =
        output_stats.gtk_frame_clock_refresh_interval_us_latest;
    pipeline_stats.gtk_frame_clock_predicted_presentation_time_us_latest =
        output_stats.gtk_frame_clock_predicted_presentation_time_us_latest;
    pipeline_stats.gtk_frame_timings_observed = output_stats.gtk_frame_timings_observed;
    pipeline_stats.gtk_frame_timings_complete = output_stats.gtk_frame_timings_complete;
    pipeline_stats.gtk_frame_timings_counter_latest = output_stats.gtk_frame_timings_counter_latest;
    pipeline_stats.gtk_frame_timings_complete_counter_latest =
        output_stats.gtk_frame_timings_complete_counter_latest;
    pipeline_stats.gtk_frame_timings_frame_time_us_latest =
        output_stats.gtk_frame_timings_frame_time_us_latest;
    pipeline_stats.gtk_frame_timings_predicted_presentation_time_us_latest =
        output_stats.gtk_frame_timings_predicted_presentation_time_us_latest;
    pipeline_stats.gtk_frame_timings_presentation_time_us_latest =
        output_stats.gtk_frame_timings_presentation_time_us_latest;
    pipeline_stats.gtk_frame_timings_presentation_interval_us_latest =
        output_stats.gtk_frame_timings_presentation_interval_us_latest;
    pipeline_stats.gtk_frame_timings_presentation_interval_us_max =
        output_stats.gtk_frame_timings_presentation_interval_us_max;
    pipeline_stats.gtk_frame_timings_refresh_interval_us_latest =
        output_stats.gtk_frame_timings_refresh_interval_us_latest;
    pipeline_stats
}

#[cfg(feature = "video-renderer")]
fn gst_state_for_mode(mode: RenderMode) -> gst::State {
    match mode {
        RenderMode::Active | RenderMode::Throttled => gst::State::Playing,
        RenderMode::Paused => gst::State::Paused,
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for GtkSharedVideoRuntime {
    fn drop(&mut self) {
        let _ = self.element.set_state(gst::State::Null);
    }
}

#[cfg(feature = "video-renderer")]
struct BuiltGtkVideoPipeline {
    element: gst::Element,
    paintable: gdk::Paintable,
    frame_limiter: Option<GtkFrameLimiter>,
    sink_tuning: VideoSinkTuningReport,
}

#[cfg(feature = "video-renderer")]
fn build_gtk_video_pipeline(
    plan: &VideoWallpaperPlan,
) -> Result<BuiltGtkVideoPipeline, GtkVideoError> {
    let uri = gst::glib::filename_to_uri(&plan.source, None::<&str>)
        .map_err(|err| GtkVideoError::Uri(err.to_string()))?;
    apply_decoder_rank_policy(plan.decoder_policy);
    let gtk_sink = gst::ElementFactory::make("gtk4paintablesink")
        .property("sync", true)
        .property("enable-last-sample", false)
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    configure_video_sink_low_memory(&gtk_sink, plan.target_max_fps);
    let paintable = gtk_sink.property::<gdk::Paintable>("paintable");
    let (video_sink, sink_tuning) = gtk_video_sink_chain(&gtk_sink, plan.target_max_fps);
    let frame_limiter = plan
        .target_max_fps
        .filter(|target_max_fps| *target_max_fps > 0)
        .map(|target_max_fps| GtkFrameLimiter::new(&video_sink, target_max_fps))
        .transpose()?;

    let builder = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .property_from_str("flags", playbin_flags(plan.muted))
        .property("video-sink", &video_sink);
    let element = builder
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    configure_video_pipeline_low_memory(&element);

    Ok(BuiltGtkVideoPipeline {
        element,
        paintable,
        frame_limiter,
        sink_tuning,
    })
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkFrameClockStatsMode {
    Off,
    Lightweight,
    Full,
}

#[cfg(feature = "video-renderer")]
fn gtk_frame_clock_stats_mode() -> GtkFrameClockStatsMode {
    std::env::var(GTK_VIDEO_FRAME_STATS_ENV)
        .ok()
        .as_deref()
        .map(parse_gtk_frame_clock_stats_mode)
        .unwrap_or(GtkFrameClockStatsMode::Lightweight)
}

#[cfg(feature = "video-renderer")]
fn parse_gtk_frame_clock_stats_mode(value: &str) -> GtkFrameClockStatsMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "off" | "none" | "disabled" => GtkFrameClockStatsMode::Off,
        "full" | "phase" | "phases" | "timing" | "timings" | "detailed" => {
            GtkFrameClockStatsMode::Full
        }
        _ => GtkFrameClockStatsMode::Lightweight,
    }
}

#[cfg(feature = "video-renderer")]
fn install_frame_clock_stats(
    picture: &gtk::Picture,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
    mode: GtkFrameClockStatsMode,
) -> Option<GtkFrameClockObserver> {
    if mode == GtkFrameClockStatsMode::Off {
        return None;
    }
    let frame_clock_handlers = Rc::new(RefCell::new(None));
    let realize_stats = Rc::clone(&frame_stats);
    let realize_frame_clock_handlers = Rc::clone(&frame_clock_handlers);
    let realize_handler = picture.connect_realize(move |picture| {
        attach_frame_clock_stats(
            picture,
            Rc::clone(&realize_stats),
            Rc::clone(&realize_frame_clock_handlers),
            mode,
        );
    });
    attach_frame_clock_stats(picture, frame_stats, Rc::clone(&frame_clock_handlers), mode);
    Some(GtkFrameClockObserver {
        picture: picture.clone(),
        realize_handler: Some(realize_handler),
        frame_clock_handlers,
    })
}

#[cfg(feature = "video-renderer")]
struct GtkFrameClockObserver {
    picture: gtk::Picture,
    realize_handler: Option<gtk::glib::SignalHandlerId>,
    frame_clock_handlers: Rc<RefCell<Option<GtkFrameClockSignalHandlers>>>,
}

#[cfg(feature = "video-renderer")]
struct GtkFrameClockSignalHandlers {
    clock: gdk::FrameClock,
    handlers: Vec<gtk::glib::SignalHandlerId>,
}

#[cfg(feature = "video-renderer")]
impl Drop for GtkFrameClockObserver {
    fn drop(&mut self) {
        if let Some(handlers) = self.frame_clock_handlers.borrow_mut().take() {
            for handler in handlers.handlers {
                handlers.clock.disconnect(handler);
            }
        }
        if let Some(handler) = self.realize_handler.take() {
            self.picture.disconnect(handler);
        }
    }
}

#[cfg(feature = "video-renderer")]
fn attach_frame_clock_stats(
    picture: &gtk::Picture,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
    frame_clock_handlers: Rc<RefCell<Option<GtkFrameClockSignalHandlers>>>,
    mode: GtkFrameClockStatsMode,
) {
    if frame_clock_handlers.borrow().is_some() {
        return;
    }
    let Some(clock) = picture.frame_clock() else {
        return;
    };
    let observed_clock = clock.clone();
    let mut handlers = Vec::new();
    if mode == GtkFrameClockStatsMode::Full {
        let before_paint_stats = Rc::clone(&frame_stats);
        handlers.push(clock.connect_before_paint(move |_| {
            before_paint_stats
                .borrow_mut()
                .record_gtk_frame_clock_phase(GtkFrameClockPhase::BeforePaint);
        }));
        let update_stats = Rc::clone(&frame_stats);
        handlers.push(clock.connect_update(move |_| {
            update_stats
                .borrow_mut()
                .record_gtk_frame_clock_phase(GtkFrameClockPhase::Update);
        }));
        let layout_stats = Rc::clone(&frame_stats);
        handlers.push(clock.connect_layout(move |_| {
            layout_stats
                .borrow_mut()
                .record_gtk_frame_clock_phase(GtkFrameClockPhase::Layout);
        }));
        let paint_stats = Rc::clone(&frame_stats);
        handlers.push(clock.connect_paint(move |_| {
            paint_stats
                .borrow_mut()
                .record_gtk_frame_clock_phase(GtkFrameClockPhase::Paint);
        }));
    }
    let handler = clock.connect_after_paint(move |clock| {
        let frame_time_us = clock.frame_time();
        let frame_counter = clock.frame_counter();
        let mut frame_stats = frame_stats.borrow_mut();
        if mode == GtkFrameClockStatsMode::Full {
            let (refresh_interval_us, predicted_presentation_time_us) =
                clock.refresh_info(frame_time_us);
            frame_stats.record_gtk_frame_clock_tick(
                frame_counter,
                frame_time_us,
                clock.fps(),
                refresh_interval_us,
                predicted_presentation_time_us,
            );
            if let Some(timings) = clock.current_timings() {
                record_gtk_frame_timing(&mut frame_stats, &timings);
            }
            let previous_frame_counter = frame_counter.saturating_sub(1);
            if previous_frame_counter >= clock.history_start()
                && let Some(timings) = clock.timings(previous_frame_counter)
            {
                record_gtk_frame_timing(&mut frame_stats, &timings);
            }
        } else {
            frame_stats.record_gtk_frame_clock_tick_minimal(frame_counter, frame_time_us);
        }
    });
    handlers.push(handler);
    *frame_clock_handlers.borrow_mut() = Some(GtkFrameClockSignalHandlers {
        clock: observed_clock,
        handlers,
    });
}

#[cfg(feature = "video-renderer")]
fn record_gtk_frame_timing(frame_stats: &mut VideoFrameStats, timings: &gdk::FrameTimings) {
    frame_stats.record_gtk_frame_timing(
        timings.frame_counter(),
        timings.is_complete(),
        timings.frame_time(),
        timings.predicted_presentation_time(),
        timings.presentation_time(),
        timings.refresh_interval(),
    );
}

#[cfg(feature = "video-renderer")]
fn gtk_video_sink_chain(
    gtk_sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> (gst::Element, VideoSinkTuningReport) {
    match gtk_video_sink_chain_mode() {
        GtkVideoSinkChainMode::DirectGtk => direct_gtk_video_sink(gtk_sink, target_max_fps),
        GtkVideoSinkChainMode::GlSinkBin => gl_wrapped_gtk_video_sink(gtk_sink, target_max_fps)
            .unwrap_or_else(|| direct_gtk_video_sink(gtk_sink, target_max_fps)),
        GtkVideoSinkChainMode::Auto => gl_wrapped_gtk_video_sink(gtk_sink, target_max_fps)
            .unwrap_or_else(|| direct_gtk_video_sink(gtk_sink, target_max_fps)),
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkVideoSinkChainMode {
    Auto,
    DirectGtk,
    GlSinkBin,
}

#[cfg(feature = "video-renderer")]
fn gtk_video_sink_chain_mode() -> GtkVideoSinkChainMode {
    std::env::var(GTK_VIDEO_SINK_CHAIN_ENV)
        .ok()
        .as_deref()
        .map(parse_gtk_video_sink_chain_mode)
        .unwrap_or(GtkVideoSinkChainMode::Auto)
}

#[cfg(feature = "video-renderer")]
fn parse_gtk_video_sink_chain_mode(value: &str) -> GtkVideoSinkChainMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "gtk" | "gtk4" | "direct" | "direct-gtk" | "gtk4paintablesink" => {
            GtkVideoSinkChainMode::DirectGtk
        }
        "gl" | "glsinkbin" | "glsinkbin+gtk4" | "glsinkbin+gtk4paintablesink" => {
            GtkVideoSinkChainMode::GlSinkBin
        }
        _ => GtkVideoSinkChainMode::Auto,
    }
}

#[cfg(feature = "video-renderer")]
fn direct_gtk_video_sink(
    gtk_sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> (gst::Element, VideoSinkTuningReport) {
    let mut tuning = configure_video_sink_low_memory(gtk_sink, target_max_fps);
    tuning.sink_element = Some("gtk4paintablesink".to_owned());
    (gtk_sink.clone(), tuning)
}

#[cfg(feature = "video-renderer")]
fn gl_wrapped_gtk_video_sink(
    gtk_sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> Option<(gst::Element, VideoSinkTuningReport)> {
    gst::ElementFactory::find("glsinkbin")?;
    let sink = gst::ElementFactory::make("glsinkbin")
        .property("sink", gtk_sink)
        .property("sync", true)
        .property("enable-last-sample", false)
        .build()
        .ok()?;
    let mut tuning = configure_video_sink_low_memory(&sink, target_max_fps);
    tuning.sink_element = Some("glsinkbin+gtk4paintablesink".to_owned());
    Some((sink, tuning))
}

#[cfg(feature = "video-renderer")]
fn playbin_flags(muted: bool) -> &'static str {
    if muted {
        MUTED_PLAYBIN_FLAGS
    } else {
        AUDIBLE_PLAYBIN_FLAGS
    }
}

#[cfg(feature = "video-renderer")]
struct GtkFrameLimiter {
    sink: gst::Element,
    target_max_fps: u32,
}

#[cfg(feature = "video-renderer")]
impl GtkFrameLimiter {
    fn new(sink: &gst::Element, target_max_fps: u32) -> Result<Self, GtkVideoError> {
        if sink.find_property("throttle-time").is_none() {
            return Err(GtkVideoError::BuildElement(format!(
                "{} does not support throttle-time",
                sink.name()
            )));
        }
        let limiter = Self {
            sink: sink.clone(),
            target_max_fps,
        };
        limiter.apply_sink_throttle();
        Ok(limiter)
    }

    fn target_max_fps(&self) -> Option<u32> {
        (self.target_max_fps > 0).then_some(self.target_max_fps)
    }

    fn throttle_time_ns(&self) -> u64 {
        frame_throttle_time_ns(self.target_max_fps)
    }

    fn apply_sink_throttle(&self) {
        self.sink
            .set_property("throttle-time", self.throttle_time_ns());
    }
}

#[cfg(feature = "video-renderer")]
fn frame_throttle_time_ns(target_max_fps: u32) -> u64 {
    if target_max_fps == 0 {
        0
    } else {
        1_000_000_000_u64 / u64::from(target_max_fps)
    }
}

fn content_fit_for_fit(fit: FitMode) -> gtk::ContentFit {
    match fit {
        FitMode::Cover => gtk::ContentFit::Cover,
        FitMode::Contain | FitMode::Tile => gtk::ContentFit::Contain,
        FitMode::Stretch => gtk::ContentFit::Fill,
        FitMode::Center => gtk::ContentFit::ScaleDown,
    }
}

fn use_picture_static_surface(fit: FitMode) -> bool {
    fit != FitMode::Tile
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum GtkVideoError {
    Init(String),
    Uri(String),
    BuildElement(String),
    MissingPipeline,
    SetState(String),
    Seek(String),
    Pipeline(String),
}

#[cfg(feature = "video-renderer")]
impl std::fmt::Display for GtkVideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init(message) => write!(f, "failed to initialize GStreamer: {message}"),
            Self::Uri(message) => write!(f, "failed to convert path to URI: {message}"),
            Self::BuildElement(message) => {
                write!(f, "failed to build GStreamer element: {message}")
            }
            Self::MissingPipeline => f.write_str("GTK video pipeline is missing"),
            Self::SetState(message) => {
                write!(f, "failed to set GStreamer pipeline state: {message}")
            }
            Self::Seek(message) => write!(f, "failed to seek GStreamer pipeline: {message}"),
            Self::Pipeline(message) => write!(f, "GStreamer pipeline error: {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn maps_fit_modes_to_css_background_modes() {
        assert_eq!(css_background_mode(FitMode::Cover).size, "cover");
        assert_eq!(css_background_mode(FitMode::Contain).size, "contain");
        assert_eq!(css_background_mode(FitMode::Stretch).size, "100% 100%");
        assert_eq!(css_background_mode(FitMode::Tile).repeat, "repeat");
        assert_eq!(css_background_mode(FitMode::Center).position, "center");
    }

    #[test]
    fn sanitizes_output_names_for_css_ids() {
        assert_eq!(
            css_widget_name("HDMI-A-1 workspace"),
            "gilder-wallpaper-HDMI-A-1-workspace"
        );
    }

    #[test]
    fn detects_unchanged_static_wallpaper_plans() {
        let plan = StaticWallpaperPlan {
            output_name: "eDP-1".to_owned(),
            source: std::path::PathBuf::from("/wallpapers/current.png"),
            fit: FitMode::Cover,
            background: Some("#101010".to_owned()),
        };
        assert!(static_plan_needs_update(None, &plan));
        assert!(!static_plan_needs_update(Some(&plan), &plan));

        let changed_fit = StaticWallpaperPlan {
            fit: FitMode::Contain,
            ..plan.clone()
        };
        assert!(static_plan_needs_update(Some(&plan), &changed_fit));

        let changed_background = StaticWallpaperPlan {
            background: Some("#202020".to_owned()),
            ..plan.clone()
        };
        assert!(static_plan_needs_update(Some(&plan), &changed_background));
    }

    #[test]
    fn renderer_surface_resource_footprint_counts_static_and_slideshow_sources() {
        let test_dir = TestDir::new("gilder-gtk-resource-footprint");
        test_dir.write_file("static.png", b"static");
        test_dir.write_file("slide-a.png", b"a");
        test_dir.write_file("slide-b.png", b"bbbb");
        test_dir.write_file("loop.webm", b"video");

        let static_source = test_dir.path().join("static.png");
        let slide_a = test_dir.path().join("slide-a.png");
        let slide_b = test_dir.path().join("slide-b.png");
        let video_source = test_dir.path().join("loop.webm");
        let slideshow_sources = vec![slide_a.clone(), slide_b.clone(), slide_a.clone()];

        let footprint = renderer_surface_resource_footprint([
            RendererSurfaceResourceSources {
                static_surface_source: Some(static_source.as_path()),
                slideshow_sources: None,
                video_pipeline_source: Some(video_source.as_path()),
            },
            RendererSurfaceResourceSources {
                static_surface_source: Some(slide_b.as_path()),
                slideshow_sources: Some(slideshow_sources.as_slice()),
                video_pipeline_source: Some(video_source.as_path()),
            },
            RendererSurfaceResourceSources {
                static_surface_source: Some(static_source.as_path()),
                slideshow_sources: None,
                video_pipeline_source: None,
            },
        ]);

        assert_eq!(footprint.static_surface_resource_references, 3);
        assert_eq!(footprint.static_surface_resource_bytes, 16);
        assert_eq!(footprint.static_surface_unique_resources, 2);
        assert_eq!(footprint.static_surface_unique_resource_bytes, 10);
        assert_eq!(footprint.slideshow_resource_references, 3);
        assert_eq!(footprint.slideshow_resource_bytes, 6);
        assert_eq!(footprint.slideshow_unique_resources, 2);
        assert_eq!(footprint.slideshow_unique_resource_bytes, 5);
        assert_eq!(footprint.video_pipeline_source_references, 2);
        assert_eq!(footprint.video_pipeline_source_reference_bytes, 10);
        assert_eq!(footprint.video_pipeline_unique_sources, 1);
        assert_eq!(footprint.video_pipeline_unique_source_bytes, 5);
    }

    #[test]
    fn renderer_surface_resource_footprint_counts_missing_sources_as_zero_bytes() {
        let test_dir = TestDir::new("gilder-gtk-resource-footprint-missing");
        let missing_static = test_dir.path().join("missing-static.png");
        let missing_slide = test_dir.path().join("missing-slide.png");
        let missing_video = test_dir.path().join("missing-video.webm");
        let slideshow_sources = vec![missing_slide];

        let footprint = renderer_surface_resource_footprint([RendererSurfaceResourceSources {
            static_surface_source: Some(missing_static.as_path()),
            slideshow_sources: Some(slideshow_sources.as_slice()),
            video_pipeline_source: Some(missing_video.as_path()),
        }]);

        assert_eq!(footprint.static_surface_resource_references, 1);
        assert_eq!(footprint.static_surface_resource_bytes, 0);
        assert_eq!(footprint.static_surface_unique_resources, 1);
        assert_eq!(footprint.static_surface_unique_resource_bytes, 0);
        assert_eq!(footprint.slideshow_resource_references, 1);
        assert_eq!(footprint.slideshow_resource_bytes, 0);
        assert_eq!(footprint.slideshow_unique_resources, 1);
        assert_eq!(footprint.slideshow_unique_resource_bytes, 0);
        assert_eq!(footprint.video_pipeline_source_references, 1);
        assert_eq!(footprint.video_pipeline_source_reference_bytes, 0);
        assert_eq!(footprint.video_pipeline_unique_sources, 1);
        assert_eq!(footprint.video_pipeline_unique_source_bytes, 0);
    }

    #[test]
    fn maps_fit_modes_to_gtk_content_fit() {
        assert_eq!(content_fit_for_fit(FitMode::Cover), gtk::ContentFit::Cover);
        assert_eq!(
            content_fit_for_fit(FitMode::Contain),
            gtk::ContentFit::Contain
        );
        assert_eq!(content_fit_for_fit(FitMode::Stretch), gtk::ContentFit::Fill);
        assert_eq!(
            content_fit_for_fit(FitMode::Center),
            gtk::ContentFit::ScaleDown
        );
        assert_eq!(content_fit_for_fit(FitMode::Tile), gtk::ContentFit::Contain);
    }

    #[test]
    fn slideshow_crossfade_uses_picture_surface_modes_only() {
        assert!(slideshow_uses_crossfade(
            Transition::Crossfade,
            FitMode::Cover
        ));
        assert!(slideshow_uses_crossfade(
            Transition::Crossfade,
            FitMode::Contain
        ));
        assert!(slideshow_uses_crossfade(
            Transition::Crossfade,
            FitMode::Stretch
        ));
        assert!(slideshow_uses_crossfade(
            Transition::Crossfade,
            FitMode::Center
        ));
        assert!(!slideshow_uses_crossfade(
            Transition::Crossfade,
            FitMode::Tile
        ));
        assert!(!slideshow_uses_crossfade(Transition::None, FitMode::Cover));
    }

    #[test]
    fn uses_picture_static_surface_except_tile_fallback() {
        assert!(use_picture_static_surface(FitMode::Cover));
        assert!(use_picture_static_surface(FitMode::Contain));
        assert!(use_picture_static_surface(FitMode::Stretch));
        assert!(use_picture_static_surface(FitMode::Center));
        assert!(!use_picture_static_surface(FitMode::Tile));
    }

    #[test]
    fn estimates_rgba_decoded_bytes_from_intrinsic_size() {
        assert_eq!(estimated_rgba_decoded_bytes(1920, 1080), 8_294_400);
        assert_eq!(estimated_rgba_decoded_bytes(0, 1080), 0);
        assert_eq!(estimated_rgba_decoded_bytes(-1, 1080), 0);
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn muted_video_playbin_flags_disable_audio_streams() {
        assert_eq!(playbin_flags(true), MUTED_PLAYBIN_FLAGS);
        assert_eq!(playbin_flags(true), "video");
        assert!(!playbin_flags(true).contains("audio"));
        assert_eq!(playbin_flags(false), "video+audio");
        assert!(playbin_flags(false).contains("audio"));
        assert!(!playbin_flags(false).contains("deinterlace"));
        assert!(!playbin_flags(false).contains("soft-colorbalance"));
        assert!(!playbin_flags(false).contains("soft-volume"));
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn gtk_video_runtime_key_shares_compatible_outputs() {
        let source = PathBuf::from("/tmp/gilder-shared-video.webm");
        let first = video_plan_for_key("eDP-1", source.clone(), FitMode::Cover, Some(30));
        let second = video_plan_for_key("HDMI-A-1", source.clone(), FitMode::Contain, Some(30));
        let different_fps = video_plan_for_key("DP-1", source.clone(), FitMode::Cover, Some(24));
        let different_source = video_plan_for_key(
            "eDP-1",
            PathBuf::from("/tmp/other.webm"),
            FitMode::Cover,
            Some(30),
        );

        assert_eq!(
            GtkVideoRuntimeKey::from_plan(&first),
            GtkVideoRuntimeKey::from_plan(&second)
        );
        assert_ne!(
            GtkVideoRuntimeKey::from_plan(&first),
            GtkVideoRuntimeKey::from_plan(&different_fps)
        );
        assert_ne!(
            GtkVideoRuntimeKey::from_plan(&first),
            GtkVideoRuntimeKey::from_plan(&different_source)
        );
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn identifies_video_poster_fallback_static_plan() {
        let poster = PathBuf::from("/tmp/gilder-poster.jpg");
        let mut video = video_plan_for_key(
            "eDP-1",
            PathBuf::from("/tmp/gilder-video.webm"),
            FitMode::Cover,
            Some(30),
        );
        video.poster = Some(poster.clone());

        let poster_plan = StaticWallpaperPlan {
            output_name: "eDP-1".to_owned(),
            source: poster.clone(),
            fit: FitMode::Cover,
            background: Some("#000000".to_owned()),
        };
        let real_static_plan = StaticWallpaperPlan {
            output_name: "eDP-1".to_owned(),
            source: PathBuf::from("/tmp/gilder-static.jpg"),
            fit: FitMode::Cover,
            background: Some("#000000".to_owned()),
        };
        let other_output_poster = StaticWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: poster,
            fit: FitMode::Cover,
            background: Some("#000000".to_owned()),
        };

        assert!(static_plan_is_video_poster_fallback(
            &poster_plan,
            &[video.clone()]
        ));
        assert!(!static_plan_is_video_poster_fallback(
            &real_static_plan,
            &[video.clone()]
        ));
        assert!(!static_plan_is_video_poster_fallback(
            &other_output_poster,
            &[video]
        ));
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn video_frame_stats_merge_keeps_pipeline_qos_and_output_frame_clock() {
        let mut pipeline = VideoFrameStats::default();
        pipeline.record_qos_values("buffers".to_owned(), 120, 3, -7000, 0.95);

        let mut output = VideoFrameStats::default();
        output.record_gtk_frame_clock_phase(GtkFrameClockPhase::BeforePaint);
        output.record_gtk_frame_clock_phase(GtkFrameClockPhase::Update);
        output.record_gtk_frame_clock_tick(5, 100_000, 60.0, 16_667, 116_667);

        let merged = merge_video_frame_stats(pipeline, output);

        assert_eq!(merged.qos_messages, 1);
        assert_eq!(merged.qos_processed_max, Some(120));
        assert_eq!(merged.qos_dropped_max, Some(3));
        assert_eq!(merged.gtk_frame_clock_ticks, 1);
        assert_eq!(merged.gtk_frame_clock_before_paint_ticks, 1);
        assert_eq!(merged.gtk_frame_clock_update_ticks, 1);
        assert_eq!(merged.gtk_frame_clock_counter_latest, Some(5));
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn gtk_frame_clock_stats_mode_defaults_to_lightweight_for_unknown_values() {
        assert_eq!(
            parse_gtk_frame_clock_stats_mode(""),
            GtkFrameClockStatsMode::Lightweight
        );
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("lightweight"),
            GtkFrameClockStatsMode::Lightweight
        );
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("unexpected"),
            GtkFrameClockStatsMode::Lightweight
        );
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn gtk_frame_clock_stats_mode_parses_full_and_off_values() {
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("full"),
            GtkFrameClockStatsMode::Full
        );
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("timings"),
            GtkFrameClockStatsMode::Full
        );
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("off"),
            GtkFrameClockStatsMode::Off
        );
        assert_eq!(
            parse_gtk_frame_clock_stats_mode("0"),
            GtkFrameClockStatsMode::Off
        );
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn gtk_video_sink_chain_mode_parses_direct_gl_and_auto_values() {
        assert_eq!(
            parse_gtk_video_sink_chain_mode("gtk4"),
            GtkVideoSinkChainMode::DirectGtk
        );
        assert_eq!(
            parse_gtk_video_sink_chain_mode("direct"),
            GtkVideoSinkChainMode::DirectGtk
        );
        assert_eq!(
            parse_gtk_video_sink_chain_mode("glsinkbin"),
            GtkVideoSinkChainMode::GlSinkBin
        );
        assert_eq!(
            parse_gtk_video_sink_chain_mode("unexpected"),
            GtkVideoSinkChainMode::Auto
        );
    }

    #[test]
    fn clamps_runtime_tick_interval_to_active_floor() {
        assert_eq!(
            clamp_runtime_tick_interval(Duration::from_millis(1)),
            GTK_ACTIVE_RUNTIME_TICK_INTERVAL
        );
        assert_eq!(
            clamp_runtime_tick_interval(Duration::from_millis(100)),
            Duration::from_millis(100)
        );
        assert_eq!(
            clamp_runtime_tick_interval(Duration::from_secs(10)),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn runtime_tick_interval_uses_video_cadence_for_video_only() {
        assert_eq!(
            runtime_tick_interval(true, None),
            Some(GTK_VIDEO_RUNTIME_TICK_INTERVAL)
        );
    }

    #[test]
    fn runtime_tick_interval_keeps_short_slideshow_ticks_with_video() {
        assert_eq!(
            runtime_tick_interval(true, Some(Duration::from_millis(1))),
            Some(GTK_ACTIVE_RUNTIME_TICK_INTERVAL)
        );
        assert_eq!(
            runtime_tick_interval(true, Some(Duration::from_millis(120))),
            Some(Duration::from_millis(120))
        );
    }

    #[test]
    fn runtime_tick_interval_disables_tick_without_runtime_work() {
        assert_eq!(runtime_tick_interval(false, None), None);
    }

    #[cfg(feature = "video-renderer")]
    fn video_plan_for_key(
        output_name: &str,
        source: PathBuf,
        fit: FitMode,
        target_max_fps: Option<u32>,
    ) -> VideoWallpaperPlan {
        VideoWallpaperPlan {
            output_name: output_name.to_owned(),
            source,
            poster: None,
            loop_playback: true,
            muted: true,
            fit,
            manifest_max_fps: None,
            start_offset_ms: 0,
            target_max_fps,
            decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
        }
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
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_file(&self, relative_path: &str, contents: &[u8]) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
