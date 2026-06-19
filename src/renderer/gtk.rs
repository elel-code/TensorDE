//! GTK 4 + layer-shell renderer for wallpaper output surfaces.

#[cfg(feature = "video-renderer")]
use super::VideoWallpaperPlan;
use super::{
    SceneLiteDisplayPlan, SceneLiteWallpaperPlan, SlideshowWallpaperPlan, StaticRenderSyncPlan,
    StaticWallpaperPlan,
};
use crate::core::FitMode;
#[cfg(feature = "video-renderer")]
use crate::policy::RenderMode;
#[cfg(feature = "video-renderer")]
use crate::renderer::video::{
    GtkFrameClockPhase, VideoFrameStats, VideoPipelineDiagnostics, VideoPipelineDiagnosticsCache,
    VideoPipelineSnapshot, apply_decoder_rank_policy, decoder_policy_status, playback_duration_ms,
    playback_position_ms,
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

#[cfg(feature = "video-renderer")]
const MUTED_PLAYBIN_FLAGS: &str = "video";
#[cfg(feature = "video-renderer")]
const AUDIBLE_PLAYBIN_FLAGS: &str = "video+audio";

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
    pub slideshow_surfaces: usize,
    pub video_surfaces: usize,
    pub static_surface_resource_references: usize,
    pub static_surface_resource_bytes: u64,
    pub static_surface_unique_resources: usize,
    pub static_surface_unique_resource_bytes: u64,
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

struct RenderedOutput {
    output_name: String,
    window: gtk::ApplicationWindow,
    #[cfg(feature = "video-renderer")]
    surface: gtk::Box,
    provider: Option<gtk::CssProvider>,
    static_plan: Option<StaticWallpaperPlan>,
    slideshow: Option<RenderedSlideshow>,
    scene_lite_plan: Option<SceneLiteWallpaperPlan>,
    #[cfg(feature = "video-renderer")]
    video: Option<GtkVideoAttachment>,
    #[cfg(feature = "video-renderer")]
    video_error: Option<VideoErrorState>,
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
        if window.provider.is_none() || static_plan_needs_update(window.static_plan.as_ref(), plan)
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
            Err(err) => output.note_video_error(plan, err),
        }
        output.window.present();
        true
    }

    #[cfg(feature = "video-renderer")]
    pub fn poll_video_buses(&mut self) -> bool {
        let had_runtimes = self.video_runtimes.len() > 0;
        let errors = self.video_runtimes.poll_buses();
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
                    output.note_video_error_for_current_source(err.clone());
                    output.remove_video(&mut self.video_runtimes);
                    output.restore_static_surface();
                }
            }
        }
        had_runtimes || had_errors
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
                .filter(|output| output.provider.is_some())
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
        self.windows
            .iter()
            .filter_map(|(output_name, output)| {
                output.video.as_ref().and_then(|video| {
                    self.video_runtimes
                        .snapshot_for_attachment(output_name, video)
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
}

#[cfg(not(feature = "video-renderer"))]
impl GtkStaticRenderer {
    fn video_shared_runtime_count(&self) -> usize {
        0
    }
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
            };
            self.slideshow = Some(slideshow);
            self.apply_slideshow_frame();
        }
    }

    fn tick_slideshow(&mut self, now: Instant) -> bool {
        let Some(slideshow) = &mut self.slideshow else {
            return false;
        };
        if slideshow.plan.sources.len() < 2 || now < slideshow.next_frame_at {
            return false;
        }
        slideshow.index = (slideshow.index + 1) % slideshow.plan.sources.len();
        slideshow.next_frame_at = now + Duration::from_millis(slideshow.plan.interval_ms);
        self.apply_slideshow_frame();
        true
    }

    fn apply_slideshow_frame(&mut self) {
        let Some(slideshow) = &self.slideshow else {
            return;
        };
        let Some(source) = slideshow.plan.sources.get(slideshow.index) else {
            return;
        };
        let static_plan = StaticWallpaperPlan {
            output_name: slideshow.plan.output_name.clone(),
            source: source.clone(),
            fit: slideshow.plan.fit,
            background: Some("#000000".to_owned()),
        };
        apply_static_wallpaper(self, &static_plan);
        self.static_plan = None;
    }

    fn remove_slideshow(&mut self) {
        self.slideshow = None;
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
        let Some(provider) = self.provider.take() else {
            return;
        };
        let display = gtk::prelude::WidgetExt::display(&self.window);
        gtk::style_context_remove_provider_for_display(&display, &provider);
    }

    #[cfg(feature = "video-renderer")]
    fn restore_static_surface(&mut self) {
        if self.provider.is_some() {
            return;
        }
        if let Some(plan) = self.static_plan.clone() {
            apply_static_wallpaper(self, &plan);
        }
    }

    fn static_surface_source(&self) -> Option<&Path> {
        self.provider.as_ref()?;
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
        self.static_plan.as_ref().map(|plan| plan.source.as_path())
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
        #[cfg(feature = "video-renderer")]
        surface,
        provider: None,
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
    apply_wallpaper_css(output, &static_wallpaper_css(plan));
}

fn apply_color_wallpaper(output: &mut RenderedOutput, output_name: &str, color: &str) {
    apply_wallpaper_css(output, &color_wallpaper_css(output_name, color));
}

fn apply_wallpaper_css(output: &mut RenderedOutput, css: &str) {
    let display = gtk::prelude::WidgetExt::display(&output.window);
    if let Some(provider) = output.provider.take() {
        gtk::style_context_remove_provider_for_display(&display, &provider);
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_data(css);
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    output.provider = Some(provider);
}

fn static_plan_needs_update(
    previous: Option<&StaticWallpaperPlan>,
    next: &StaticWallpaperPlan,
) -> bool {
    previous != Some(next)
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
    let mut static_unique_sources = BTreeSet::new();
    let mut slideshow_unique_sources = BTreeSet::new();
    let mut video_unique_sources = BTreeSet::new();
    for output in outputs {
        if let Some(source) = output.static_surface_source {
            footprint.static_surface_resource_references += 1;
            footprint.static_surface_resource_bytes += source_file_size(source);
            static_unique_sources.insert(source.to_path_buf());
        }
        if let Some(sources) = output.slideshow_sources {
            footprint.slideshow_resource_references += sources.len();
            footprint.slideshow_resource_bytes += sources
                .iter()
                .map(|source| source_file_size(source))
                .sum::<u64>();
            slideshow_unique_sources.extend(sources.iter().cloned());
        }
        if let Some(source) = output.video_pipeline_source {
            footprint.video_pipeline_source_references += 1;
            footprint.video_pipeline_source_reference_bytes += source_file_size(source);
            video_unique_sources.insert(source.to_path_buf());
        }
    }
    footprint.static_surface_unique_resources = static_unique_sources.len();
    footprint.static_surface_unique_resource_bytes = static_unique_sources
        .iter()
        .map(|source| source_file_size(source))
        .sum();
    footprint.slideshow_unique_resources = slideshow_unique_sources.len();
    footprint.slideshow_unique_resource_bytes = slideshow_unique_sources
        .iter()
        .map(|source| source_file_size(source))
        .sum();
    footprint.video_pipeline_unique_sources = video_unique_sources.len();
    footprint.video_pipeline_unique_source_bytes = video_unique_sources
        .iter()
        .map(|source| source_file_size(source))
        .sum();
    footprint
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
        let attachment = runtime.attach_output(output_name, plan.fit, mode);
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

    fn poll_buses(&mut self) -> Vec<(GtkVideoRuntimeKey, GtkVideoError)> {
        self.runtimes
            .iter()
            .filter_map(|(key, runtime)| {
                runtime
                    .borrow_mut()
                    .poll_bus()
                    .err()
                    .map(|err| (key.clone(), err))
            })
            .collect()
    }

    fn snapshot_for_attachment(
        &self,
        output_name: &str,
        attachment: &GtkVideoAttachment,
    ) -> Option<VideoPipelineSnapshot> {
        let runtime = self.runtimes.get(&attachment.key)?;
        let runtime = runtime.borrow();
        Some(runtime.snapshot_for_attachment(output_name, attachment))
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
    mode: RenderMode,
    fit: FitMode,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
    _frame_clock_observer: GtkFrameClockObserver,
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
    }
}

#[cfg(feature = "video-renderer")]
struct GtkSharedVideoRuntime {
    element: gst::Element,
    paintable: gdk::Paintable,
    frame_limiter: Option<GtkFrameLimiter>,
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
        };
        runtime.apply_muted(plan.muted);
        runtime.apply_start_offset(plan.start_offset_ms)?;
        Ok(runtime)
    }

    fn attach_output(
        &mut self,
        output_name: &str,
        fit: FitMode,
        mode: RenderMode,
    ) -> GtkVideoAttachment {
        let picture = gtk::Picture::for_paintable(&self.paintable);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_can_shrink(false);
        picture.set_content_fit(content_fit_for_fit(fit));
        let frame_stats = Rc::new(RefCell::new(VideoFrameStats::default()));
        let frame_clock_observer = install_frame_clock_stats(&picture, Rc::clone(&frame_stats));
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

    fn poll_bus(&mut self) -> Result<(), GtkVideoError> {
        let Some(bus) = self.element.bus() else {
            return Ok(());
        };
        while let Some(message) = bus.pop() {
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
        Ok(())
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

    fn snapshot_for_attachment(
        &self,
        output_name: &str,
        attachment: &GtkVideoAttachment,
    ) -> VideoPipelineSnapshot {
        let VideoPipelineDiagnostics {
            actual_decoder_reports,
            caps_reports,
            allocation_reports,
            zero_copy_evidence,
            memory_path,
        } = self.diagnostics.snapshot(&self.element);
        let frame_limiter_max_fps = self
            .frame_limiter
            .as_ref()
            .and_then(GtkFrameLimiter::target_max_fps);
        VideoPipelineSnapshot {
            output_name: output_name.to_owned(),
            source: self.source.to_string_lossy().into_owned(),
            mode: attachment.mode,
            gst_state: self.gst_state.name().to_string(),
            loop_playback: self.loop_playback,
            muted: self.muted,
            target_max_fps: self.target_max_fps,
            frame_limiter_enabled: frame_limiter_max_fps.is_some(),
            frame_limiter_max_fps,
            frame_stats: merge_video_frame_stats(
                self.frame_stats.borrow().clone(),
                attachment.frame_stats.borrow().clone(),
            ),
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
            zero_copy_evidence,
            memory_path,
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
}

#[cfg(feature = "video-renderer")]
fn build_gtk_video_pipeline(
    plan: &VideoWallpaperPlan,
) -> Result<BuiltGtkVideoPipeline, GtkVideoError> {
    let uri = gst::glib::filename_to_uri(&plan.source, None::<&str>)
        .map_err(|err| GtkVideoError::Uri(err.to_string()))?;
    apply_decoder_rank_policy(plan.decoder_policy);
    let video_sink = gst::ElementFactory::make("gtk4paintablesink")
        .property("sync", true)
        .property("enable-last-sample", false)
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    let frame_limiter = plan
        .target_max_fps
        .filter(|target_max_fps| *target_max_fps > 0)
        .map(|target_max_fps| GtkFrameLimiter::new(&video_sink, target_max_fps))
        .transpose()?;
    let paintable = video_sink.property::<gdk::Paintable>("paintable");

    let builder = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .property_from_str("flags", playbin_flags(plan.muted))
        .property("video-sink", &video_sink);
    let element = builder
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;

    Ok(BuiltGtkVideoPipeline {
        element,
        paintable,
        frame_limiter,
    })
}

#[cfg(feature = "video-renderer")]
fn install_frame_clock_stats(
    picture: &gtk::Picture,
    frame_stats: Rc<RefCell<VideoFrameStats>>,
) -> GtkFrameClockObserver {
    let frame_clock_handlers = Rc::new(RefCell::new(None));
    let realize_stats = Rc::clone(&frame_stats);
    let realize_frame_clock_handlers = Rc::clone(&frame_clock_handlers);
    let realize_handler = picture.connect_realize(move |picture| {
        attach_frame_clock_stats(
            picture,
            Rc::clone(&realize_stats),
            Rc::clone(&realize_frame_clock_handlers),
        );
    });
    attach_frame_clock_stats(picture, frame_stats, Rc::clone(&frame_clock_handlers));
    GtkFrameClockObserver {
        picture: picture.clone(),
        realize_handler: Some(realize_handler),
        frame_clock_handlers,
    }
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
) {
    if frame_clock_handlers.borrow().is_some() {
        return;
    }
    let Some(clock) = picture.frame_clock() else {
        return;
    };
    let observed_clock = clock.clone();
    let mut handlers = Vec::new();
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
    let handler = clock.connect_after_paint(move |clock| {
        let frame_time_us = clock.frame_time();
        let (refresh_interval_us, predicted_presentation_time_us) =
            clock.refresh_info(frame_time_us);
        let frame_counter = clock.frame_counter();
        {
            let mut frame_stats = frame_stats.borrow_mut();
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

#[cfg(feature = "video-renderer")]
fn content_fit_for_fit(fit: FitMode) -> gtk::ContentFit {
    match fit {
        FitMode::Cover => gtk::ContentFit::Cover,
        FitMode::Contain | FitMode::Tile => gtk::ContentFit::Contain,
        FitMode::Stretch => gtk::ContentFit::Fill,
        FitMode::Center => gtk::ContentFit::ScaleDown,
    }
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

    #[cfg(feature = "video-renderer")]
    #[test]
    fn maps_video_fit_modes_to_gtk_content_fit() {
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
