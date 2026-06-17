//! GStreamer video pipeline controller.

use super::{StaticRenderSyncPlan, VideoWallpaperPlan};
use crate::policy::RenderMode;
use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub struct GstVideoRenderer {
    pipelines: BTreeMap<String, VideoPipeline>,
    #[cfg(test)]
    test_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoRuntimeCapabilities {
    pub gstreamer_initialized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_error: Option<String>,
    pub elements: Vec<GstElementCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GstElementCapability {
    pub name: String,
    pub available: bool,
}

pub fn runtime_capabilities() -> VideoRuntimeCapabilities {
    match gst::init() {
        Ok(()) => VideoRuntimeCapabilities {
            gstreamer_initialized: true,
            init_error: None,
            elements: VIDEO_RUNTIME_ELEMENTS
                .iter()
                .map(|element| GstElementCapability {
                    name: (*element).to_owned(),
                    available: gst::ElementFactory::find(element).is_some(),
                })
                .collect(),
        },
        Err(err) => VideoRuntimeCapabilities {
            gstreamer_initialized: false,
            init_error: Some(err.to_string()),
            elements: VIDEO_RUNTIME_ELEMENTS
                .iter()
                .map(|element| GstElementCapability {
                    name: (*element).to_owned(),
                    available: false,
                })
                .collect(),
        },
    }
}

const VIDEO_RUNTIME_ELEMENTS: &[&str] = &[
    "playbin",
    "fakesink",
    "videorate",
    "capsfilter",
    "gtk4paintablesink",
];

impl GstVideoRenderer {
    pub fn new() -> Result<Self, VideoRendererError> {
        gst::init().map_err(|err| VideoRendererError::Init(err.to_string()))?;
        Ok(Self {
            pipelines: BTreeMap::new(),
            #[cfg(test)]
            test_source: false,
        })
    }

    #[cfg(test)]
    fn new_with_test_source() -> Result<Self, VideoRendererError> {
        gst::init().map_err(|err| VideoRendererError::Init(err.to_string()))?;
        Ok(Self {
            pipelines: BTreeMap::new(),
            test_source: true,
        })
    }

    pub fn sync_render_plan(
        &mut self,
        sync: &StaticRenderSyncPlan,
    ) -> Result<(), VideoRendererError> {
        for output_name in sync
            .removals
            .iter()
            .chain(sync.errors.iter().map(|failure| &failure.output_name))
        {
            self.remove_output(output_name)?;
        }

        let mut desired_outputs = BTreeSet::new();
        for plan in &sync.video_plans {
            desired_outputs.insert(plan.output_name.clone());
            let mode = sync
                .decisions
                .iter()
                .find(|decision| decision.output_name == plan.output_name)
                .map(|decision| decision.performance.mode)
                .unwrap_or(RenderMode::Active);
            self.set_video_wallpaper(plan, mode)?;
        }

        let stale_outputs = self
            .pipelines
            .keys()
            .filter(|output_name| !desired_outputs.contains(*output_name))
            .cloned()
            .collect::<Vec<_>>();
        for output_name in stale_outputs {
            self.remove_output(&output_name)?;
        }

        Ok(())
    }

    pub fn set_video_wallpaper(
        &mut self,
        plan: &VideoWallpaperPlan,
        mode: RenderMode,
    ) -> Result<(), VideoRendererError> {
        let restart = self
            .pipelines
            .get(&plan.output_name)
            .map(|pipeline| {
                pipeline.source != plan.source
                    || pipeline.loop_playback != plan.loop_playback
                    || pipeline.muted != plan.muted
            })
            .unwrap_or(true);
        if restart {
            self.remove_output(&plan.output_name)?;
            let pipeline = VideoPipeline::new(
                plan,
                #[cfg(test)]
                self.test_source,
            )?;
            self.pipelines.insert(plan.output_name.clone(), pipeline);
        }

        let pipeline = self
            .pipelines
            .get_mut(&plan.output_name)
            .ok_or_else(|| VideoRendererError::MissingPipeline(plan.output_name.clone()))?;
        pipeline.apply_plan(plan)?;
        pipeline.apply_mode(mode)?;
        Ok(())
    }

    pub fn remove_output(&mut self, output_name: &str) -> Result<(), VideoRendererError> {
        if let Some(pipeline) = self.pipelines.remove(output_name) {
            pipeline.stop()?;
        }
        Ok(())
    }

    pub fn poll_bus(&mut self) -> Result<(), VideoRendererError> {
        for pipeline in self.pipelines.values_mut() {
            pipeline.poll_bus()?;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<VideoPipelineSnapshot> {
        self.pipelines
            .iter()
            .map(|(output_name, pipeline)| VideoPipelineSnapshot {
                output_name: output_name.clone(),
                source: pipeline.source.display().to_string(),
                mode: pipeline.mode,
                loop_playback: pipeline.loop_playback,
                muted: pipeline.muted,
                target_max_fps: pipeline.target_max_fps,
            })
            .collect()
    }
}

struct VideoPipeline {
    element: gst::Element,
    frame_limiter: Option<FrameLimiter>,
    source: std::path::PathBuf,
    mode: RenderMode,
    loop_playback: bool,
    muted: bool,
    target_max_fps: Option<u32>,
}

impl VideoPipeline {
    fn new(
        plan: &VideoWallpaperPlan,
        #[cfg(test)] test_source: bool,
    ) -> Result<Self, VideoRendererError> {
        let pipeline = build_pipeline(
            plan,
            #[cfg(test)]
            test_source,
        )?;
        let mut pipeline = Self {
            element: pipeline.element,
            frame_limiter: pipeline.frame_limiter,
            source: plan.source.clone(),
            mode: RenderMode::Paused,
            loop_playback: plan.loop_playback,
            muted: plan.muted,
            target_max_fps: plan.target_max_fps,
        };
        pipeline.apply_muted(plan.muted);
        Ok(pipeline)
    }

    fn apply_plan(&mut self, plan: &VideoWallpaperPlan) -> Result<(), VideoRendererError> {
        self.loop_playback = plan.loop_playback;
        self.apply_target_max_fps(plan.target_max_fps);
        self.apply_muted(plan.muted);
        if plan.start_offset_ms > 0 {
            let position = gst::ClockTime::from_mseconds(plan.start_offset_ms);
            self.element
                .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
                .map_err(|err| VideoRendererError::Seek(err.to_string()))?;
        }
        Ok(())
    }

    fn apply_target_max_fps(&mut self, target_max_fps: Option<u32>) {
        self.target_max_fps = target_max_fps;
        if let Some(frame_limiter) = &self.frame_limiter {
            frame_limiter.apply_target_max_fps(target_max_fps);
        }
    }

    fn apply_mode(&mut self, mode: RenderMode) -> Result<(), VideoRendererError> {
        self.mode = mode;
        match mode {
            RenderMode::Active | RenderMode::Throttled => self.set_state(gst::State::Playing),
            RenderMode::Paused => self.set_state(gst::State::Paused),
        }
    }

    fn stop(self) -> Result<(), VideoRendererError> {
        self.set_state(gst::State::Null)
    }

    fn poll_bus(&mut self) -> Result<(), VideoRendererError> {
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
                            .map_err(|err| VideoRendererError::Seek(err.to_string()))?;
                        if self.mode != RenderMode::Paused {
                            self.set_state(gst::State::Playing)?;
                        }
                    } else {
                        self.set_state(gst::State::Paused)?;
                    }
                }
                gst::MessageView::Error(err) => {
                    return Err(VideoRendererError::Pipeline(format!(
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

    fn set_state(&self, state: gst::State) -> Result<(), VideoRendererError> {
        self.element
            .set_state(state)
            .map(|_| ())
            .map_err(|err| VideoRendererError::SetState(err.to_string()))
    }

    fn apply_muted(&mut self, muted: bool) {
        self.muted = muted;
        if self.element.find_property("mute").is_some() {
            self.element.set_property("mute", muted);
        }
    }
}

struct BuiltPipeline {
    element: gst::Element,
    frame_limiter: Option<FrameLimiter>,
}

fn build_pipeline(
    plan: &VideoWallpaperPlan,
    #[cfg(test)] test_source: bool,
) -> Result<BuiltPipeline, VideoRendererError> {
    #[cfg(test)]
    if test_source {
        let element = gst::parse::launch("videotestsrc is-live=true ! fakesink sync=false")
            .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?
            .downcast::<gst::Element>()
            .map_err(|_| {
                VideoRendererError::BuildElement("test pipeline is not an element".to_owned())
            })?;
        return Ok(BuiltPipeline {
            element,
            frame_limiter: None,
        });
    }

    let uri = gst::glib::filename_to_uri(&plan.source, None::<&str>)
        .map_err(|err| VideoRendererError::Uri(err.to_string()))?;
    let frame_limiter = Some(FrameLimiter::new(plan.target_max_fps)?);
    let video_sink = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .build()
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    let audio_sink = if plan.muted {
        Some(
            gst::ElementFactory::make("fakesink")
                .property("sync", false)
                .build()
                .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?,
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
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    Ok(BuiltPipeline {
        element,
        frame_limiter,
    })
}

struct FrameLimiter {
    element: gst::Element,
    capsfilter: gst::Element,
}

impl FrameLimiter {
    fn new(target_max_fps: Option<u32>) -> Result<Self, VideoRendererError> {
        let bin = gst::Bin::new();
        let videorate = gst::ElementFactory::make("videorate")
            .build()
            .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property("caps", caps_for_target_max_fps(target_max_fps))
            .build()
            .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
        bin.add_many([&videorate, &capsfilter])
            .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
        gst::Element::link_many([&videorate, &capsfilter])
            .map_err(|err| VideoRendererError::LinkElement(err.to_string()))?;

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

    #[cfg(test)]
    fn target_max_fps(&self) -> Option<u32> {
        target_max_fps_from_caps(&self.capsfilter.property::<gst::Caps>("caps"))
    }
}

fn add_ghost_pad(
    bin: &gst::Bin,
    element: &gst::Element,
    pad_name: &str,
) -> Result<(), VideoRendererError> {
    let pad = element
        .static_pad(pad_name)
        .ok_or_else(|| VideoRendererError::MissingPad(pad_name.to_owned()))?;
    let ghost_pad = gst::GhostPad::with_target(&pad)
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    ghost_pad
        .set_active(true)
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    bin.add_pad(&ghost_pad)
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))
}

fn caps_for_target_max_fps(target_max_fps: Option<u32>) -> gst::Caps {
    match target_max_fps {
        Some(max_fps) => gst::Caps::builder("video/x-raw")
            .field("framerate", gst::Fraction::new(max_fps as i32, 1))
            .build(),
        None => gst::Caps::new_any(),
    }
}

#[cfg(test)]
fn target_max_fps_from_caps(caps: &gst::Caps) -> Option<u32> {
    let structure = caps.structure(0)?;
    let framerate = structure.get::<gst::Fraction>("framerate").ok()?;
    u32::try_from(framerate.numer()).ok()
}

impl Drop for VideoPipeline {
    fn drop(&mut self) {
        let _ = self.element.set_state(gst::State::Null);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoPipelineSnapshot {
    pub output_name: String,
    pub source: String,
    pub mode: RenderMode,
    pub loop_playback: bool,
    pub muted: bool,
    pub target_max_fps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoRendererError {
    Init(String),
    Uri(String),
    BuildElement(String),
    LinkElement(String),
    MissingPad(String),
    MissingPipeline(String),
    SetState(String),
    Seek(String),
    Pipeline(String),
}

impl fmt::Display for VideoRendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(message) => write!(f, "failed to initialize GStreamer: {message}"),
            Self::Uri(message) => write!(f, "failed to convert path to URI: {message}"),
            Self::BuildElement(message) => {
                write!(f, "failed to build GStreamer element: {message}")
            }
            Self::LinkElement(message) => write!(f, "failed to link GStreamer elements: {message}"),
            Self::MissingPad(pad) => write!(f, "GStreamer element is missing {pad} pad"),
            Self::MissingPipeline(output) => {
                write!(f, "video pipeline for output {output} is missing")
            }
            Self::SetState(message) => {
                write!(f, "failed to set GStreamer pipeline state: {message}")
            }
            Self::Seek(message) => write!(f, "failed to seek GStreamer pipeline: {message}"),
            Self::Pipeline(message) => write!(f, "GStreamer pipeline error: {message}"),
        }
    }
}

impl std::error::Error for VideoRendererError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FitMode;
    use crate::policy::PerformanceDecision;
    use crate::renderer::{
        StaticRenderAction, StaticRenderOutputDecision, StaticRenderSyncPlan, VideoWallpaperPlan,
    };
    use std::path::PathBuf;

    #[test]
    fn syncs_video_pipeline_snapshots() {
        let mut renderer = GstVideoRenderer::new_with_test_source().unwrap();
        let plan = video_plan("eDP-1", true, true);
        let sync = StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: vec![plan],
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: vec![decision("eDP-1", RenderMode::Throttled)],
        };

        renderer.sync_render_plan(&sync).unwrap();
        let snapshot = renderer.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].output_name, "eDP-1");
        assert_eq!(snapshot[0].mode, RenderMode::Throttled);
        assert!(snapshot[0].loop_playback);
        assert!(snapshot[0].muted);
        assert_eq!(snapshot[0].target_max_fps, Some(24));

        let sync = StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: Vec::new(),
            removals: vec!["eDP-1".to_owned()],
            errors: Vec::new(),
            decisions: Vec::new(),
        };
        renderer.sync_render_plan(&sync).unwrap();
        assert!(renderer.snapshot().is_empty());
    }

    #[test]
    fn builds_and_updates_frame_limiter_caps() {
        gst::init().unwrap();
        let mut plan = video_plan("eDP-1", true, true);
        let mut pipeline = VideoPipeline::new(&plan, false).unwrap();
        assert_eq!(
            pipeline
                .frame_limiter
                .as_ref()
                .and_then(FrameLimiter::target_max_fps),
            Some(24)
        );

        plan.target_max_fps = Some(12);
        pipeline.apply_plan(&plan).unwrap();
        assert_eq!(pipeline.target_max_fps, Some(12));
        assert_eq!(
            pipeline
                .frame_limiter
                .as_ref()
                .and_then(FrameLimiter::target_max_fps),
            Some(12)
        );
    }

    #[test]
    fn leaves_frame_limiter_unrestricted_without_target_fps() {
        gst::init().unwrap();
        let mut plan = video_plan("eDP-1", true, true);
        plan.target_max_fps = None;
        let pipeline = VideoPipeline::new(&plan, false).unwrap();
        assert_eq!(
            pipeline
                .frame_limiter
                .as_ref()
                .and_then(FrameLimiter::target_max_fps),
            None
        );
    }

    #[test]
    fn runtime_capabilities_report_expected_elements() {
        let capabilities = runtime_capabilities();
        let element_names = capabilities
            .elements
            .iter()
            .map(|element| element.name.as_str())
            .collect::<Vec<_>>();

        for expected in [
            "playbin",
            "fakesink",
            "videorate",
            "capsfilter",
            "gtk4paintablesink",
        ] {
            assert!(element_names.contains(&expected));
        }
    }

    fn video_plan(output_name: &str, loop_playback: bool, muted: bool) -> VideoWallpaperPlan {
        VideoWallpaperPlan {
            output_name: output_name.to_owned(),
            source: PathBuf::from("/tmp/gilder-test-video.webm"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback,
            muted,
            manifest_max_fps: Some(60),
            target_max_fps: Some(24),
            start_offset_ms: 0,
        }
    }

    fn decision(output_name: &str, mode: RenderMode) -> StaticRenderOutputDecision {
        StaticRenderOutputDecision {
            output_name: output_name.to_owned(),
            action: StaticRenderAction::Render,
            performance: PerformanceDecision {
                mode,
                max_fps: Some(24),
                reason: crate::policy::DecisionReason::Unfocused,
            },
            wallpaper: Some("/tmp/gilder-test-video.gwpdir".to_owned()),
        }
    }
}
