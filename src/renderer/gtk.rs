//! GTK 4 + layer-shell renderer for wallpaper output surfaces.

#[cfg(feature = "video-renderer")]
use super::VideoWallpaperPlan;
use super::{StaticRenderSyncPlan, StaticWallpaperPlan};
use crate::core::FitMode;
#[cfg(feature = "video-renderer")]
use crate::policy::RenderMode;
use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "video-renderer")]
use gst::prelude::*;
#[cfg(feature = "video-renderer")]
use gstreamer as gst;

pub struct GtkStaticRenderer {
    application: gtk::Application,
    windows: BTreeMap<String, RenderedOutput>,
}

struct RenderedOutput {
    #[cfg(feature = "video-renderer")]
    output_name: String,
    window: gtk::ApplicationWindow,
    #[cfg(feature = "video-renderer")]
    surface: gtk::Box,
    provider: Option<gtk::CssProvider>,
    #[cfg(feature = "video-renderer")]
    video: Option<GtkVideoPipeline>,
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
                    output.remove_video();
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
        apply_static_wallpaper(window, plan);
        window.window.present();
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

        match output.set_video(plan, mode) {
            Ok(()) => output.video_error = None,
            Err(err) => output.note_video_error(plan, err),
        }
        output.window.present();
        true
    }

    #[cfg(feature = "video-renderer")]
    pub fn poll_video_buses(&mut self) {
        for output in self.windows.values_mut() {
            if let Some(video) = &mut output.video
                && let Err(err) = video.poll_bus()
            {
                output.note_video_error_for_current_source(err);
                output.remove_video();
            }
        }
    }

    pub fn remove_output(&mut self, output_name: &str) {
        if let Some(mut output) = self.windows.remove(output_name) {
            #[cfg(feature = "video-renderer")]
            output.remove_video();
            if let Some(provider) = output.provider.take() {
                let display = gtk::prelude::WidgetExt::display(&output.window);
                gtk::style_context_remove_provider_for_display(&display, &provider);
            }
            output.window.close();
        }
    }
}

impl RenderedOutput {
    #[cfg(feature = "video-renderer")]
    fn output_name(&self) -> &str {
        &self.output_name
    }

    #[cfg(feature = "video-renderer")]
    fn set_video(
        &mut self,
        plan: &VideoWallpaperPlan,
        mode: RenderMode,
    ) -> Result<(), GtkVideoError> {
        let restart = self
            .video
            .as_ref()
            .map(|video| {
                video.source != plan.source
                    || video.loop_playback != plan.loop_playback
                    || video.muted != plan.muted
            })
            .unwrap_or(true);
        if restart {
            self.remove_video();
            let video = GtkVideoPipeline::new(plan)?;
            self.surface.append(video.widget());
            self.video = Some(video);
        }

        let Some(video) = &mut self.video else {
            return Err(GtkVideoError::MissingPipeline);
        };
        video.apply_plan(plan)?;
        video.apply_mode(mode)?;
        Ok(())
    }

    #[cfg(feature = "video-renderer")]
    fn remove_video(&mut self) {
        if let Some(video) = self.video.take() {
            self.surface.remove(video.widget());
            video.stop();
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
        #[cfg(feature = "video-renderer")]
        output_name: output_name.to_owned(),
        window,
        #[cfg(feature = "video-renderer")]
        surface,
        provider: None,
        #[cfg(feature = "video-renderer")]
        video: None,
        #[cfg(feature = "video-renderer")]
        video_error: None,
    }
}

fn apply_static_wallpaper(output: &mut RenderedOutput, plan: &StaticWallpaperPlan) {
    let display = gtk::prelude::WidgetExt::display(&output.window);
    if let Some(provider) = output.provider.take() {
        gtk::style_context_remove_provider_for_display(&display, &provider);
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&static_wallpaper_css(plan));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    output.provider = Some(provider);
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

#[cfg(feature = "video-renderer")]
struct GtkVideoPipeline {
    element: gst::Element,
    picture: gtk::Picture,
    frame_limiter: Option<GtkFrameLimiter>,
    source: std::path::PathBuf,
    mode: RenderMode,
    gst_state: gst::State,
    loop_playback: bool,
    muted: bool,
    fit: FitMode,
    target_max_fps: Option<u32>,
    start_offset_ms: u64,
}

#[cfg(feature = "video-renderer")]
impl GtkVideoPipeline {
    fn new(plan: &VideoWallpaperPlan) -> Result<Self, GtkVideoError> {
        gst::init().map_err(|err| GtkVideoError::Init(err.to_string()))?;
        let built = build_gtk_video_pipeline(plan)?;
        let mut pipeline = Self {
            element: built.element,
            picture: built.picture,
            frame_limiter: built.frame_limiter,
            source: plan.source.clone(),
            mode: RenderMode::Paused,
            gst_state: gst::State::Null,
            loop_playback: plan.loop_playback,
            muted: !plan.muted,
            fit: plan.fit,
            target_max_fps: plan.target_max_fps,
            start_offset_ms: 0,
        };
        pipeline.apply_muted(plan.muted);
        Ok(pipeline)
    }

    fn widget(&self) -> &gtk::Picture {
        &self.picture
    }

    fn apply_plan(&mut self, plan: &VideoWallpaperPlan) -> Result<(), GtkVideoError> {
        self.loop_playback = plan.loop_playback;
        self.apply_target_max_fps(plan.target_max_fps);
        self.apply_muted(plan.muted);
        self.apply_fit(plan.fit);
        self.apply_start_offset(plan.start_offset_ms)?;
        Ok(())
    }

    fn apply_target_max_fps(&mut self, target_max_fps: Option<u32>) {
        if self.target_max_fps == target_max_fps {
            return;
        }
        self.target_max_fps = target_max_fps;
        if let Some(frame_limiter) = &self.frame_limiter {
            frame_limiter.apply_target_max_fps(target_max_fps);
        }
    }

    fn apply_mode(&mut self, mode: RenderMode) -> Result<(), GtkVideoError> {
        let state = gst_state_for_mode(mode);
        if self.mode == mode && self.gst_state == state {
            return Ok(());
        }
        self.mode = mode;
        self.set_state(state)
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
                        if self.mode != RenderMode::Paused {
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
                _ => {}
            }
        }
        Ok(())
    }

    fn stop(mut self) {
        let _ = self.set_state(gst::State::Null);
    }

    fn set_state(&mut self, state: gst::State) -> Result<(), GtkVideoError> {
        if self.gst_state == state {
            return Ok(());
        }
        self.element
            .set_state(state)
            .map_err(|err| GtkVideoError::SetState(err.to_string()))?;
        self.gst_state = state;
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

    fn apply_fit(&mut self, fit: FitMode) {
        if self.fit == fit {
            return;
        }
        self.picture.set_content_fit(content_fit_for_fit(fit));
        self.fit = fit;
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
}

#[cfg(feature = "video-renderer")]
fn gst_state_for_mode(mode: RenderMode) -> gst::State {
    match mode {
        RenderMode::Active | RenderMode::Throttled => gst::State::Playing,
        RenderMode::Paused => gst::State::Paused,
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for GtkVideoPipeline {
    fn drop(&mut self) {
        let _ = self.element.set_state(gst::State::Null);
    }
}

#[cfg(feature = "video-renderer")]
struct BuiltGtkVideoPipeline {
    element: gst::Element,
    picture: gtk::Picture,
    frame_limiter: Option<GtkFrameLimiter>,
}

#[cfg(feature = "video-renderer")]
fn build_gtk_video_pipeline(
    plan: &VideoWallpaperPlan,
) -> Result<BuiltGtkVideoPipeline, GtkVideoError> {
    let uri = gst::glib::filename_to_uri(&plan.source, None::<&str>)
        .map_err(|err| GtkVideoError::Uri(err.to_string()))?;
    let frame_limiter = Some(GtkFrameLimiter::new(plan.target_max_fps)?);
    let video_sink = gst::ElementFactory::make("gtk4paintablesink")
        .property("sync", true)
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    let paintable = video_sink.property::<gdk::Paintable>("paintable");
    let picture = gtk::Picture::for_paintable(&paintable);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_can_shrink(false);
    picture.set_content_fit(content_fit_for_fit(plan.fit));

    let audio_sink = if plan.muted {
        Some(
            gst::ElementFactory::make("fakesink")
                .property("sync", false)
                .build()
                .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?,
        )
    } else {
        None
    };
    let mut builder = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .property("video-sink", &video_sink);
    if let Some(audio_sink) = &audio_sink {
        builder = builder.property("audio-sink", audio_sink);
    }
    if let Some(frame_limiter) = &frame_limiter {
        builder = builder.property("video-filter", frame_limiter.element());
    }
    let element = builder
        .build()
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;

    Ok(BuiltGtkVideoPipeline {
        element,
        picture,
        frame_limiter,
    })
}

#[cfg(feature = "video-renderer")]
struct GtkFrameLimiter {
    element: gst::Element,
    capsfilter: gst::Element,
}

#[cfg(feature = "video-renderer")]
impl GtkFrameLimiter {
    fn new(target_max_fps: Option<u32>) -> Result<Self, GtkVideoError> {
        let bin = gst::Bin::new();
        let videorate = gst::ElementFactory::make("videorate")
            .build()
            .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", caps_for_target_max_fps(target_max_fps))
            .build()
            .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
        bin.add_many([&videorate, &capsfilter])
            .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
        gst::Element::link_many([&videorate, &capsfilter])
            .map_err(|err| GtkVideoError::LinkElement(err.to_string()))?;
        add_ghost_pad(&bin, &videorate, "sink")?;
        add_ghost_pad(&bin, &capsfilter, "src")?;
        Ok(Self {
            element: bin.upcast(),
            capsfilter,
        })
    }

    fn element(&self) -> &gst::Element {
        &self.element
    }

    fn apply_target_max_fps(&self, target_max_fps: Option<u32>) {
        self.capsfilter
            .set_property("caps", caps_for_target_max_fps(target_max_fps));
    }
}

#[cfg(feature = "video-renderer")]
fn add_ghost_pad(
    bin: &gst::Bin,
    element: &gst::Element,
    pad_name: &str,
) -> Result<(), GtkVideoError> {
    let pad = element
        .static_pad(pad_name)
        .ok_or_else(|| GtkVideoError::MissingPad(pad_name.to_owned()))?;
    let ghost_pad = gst::GhostPad::with_target(&pad)
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    ghost_pad
        .set_active(true)
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))?;
    bin.add_pad(&ghost_pad)
        .map_err(|err| GtkVideoError::BuildElement(err.to_string()))
}

#[cfg(feature = "video-renderer")]
fn caps_for_target_max_fps(target_max_fps: Option<u32>) -> gst::Caps {
    match target_max_fps {
        Some(max_fps) => gst::Caps::builder("video/x-raw")
            .field("framerate", gst::Fraction::new(max_fps as i32, 1))
            .build(),
        None => gst::Caps::new_any(),
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
    LinkElement(String),
    MissingPad(String),
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
            Self::LinkElement(message) => write!(f, "failed to link GStreamer elements: {message}"),
            Self::MissingPad(pad) => write!(f, "GStreamer element is missing {pad} pad"),
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
}
