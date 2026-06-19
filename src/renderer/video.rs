//! GStreamer video pipeline controller.

use super::{StaticRenderSyncPlan, VideoWallpaperPlan};
use crate::config::VideoDecoderPolicy;
use crate::policy::RenderMode;
use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

const VIDEO_RUNTIME_ELEMENTS: &[&str] = &["playbin", "fakesink", "gtk4paintablesink", "glsinkbin"];
const MUTED_PLAYBIN_FLAGS: &str = "video";
const AUDIBLE_PLAYBIN_FLAGS: &str = "video+audio";
const VIDEO_DIAGNOSTICS_REFRESH_INTERVAL: Duration = Duration::from_millis(500);
const SOFTWARE_DECODER_ELEMENT_NAMES: &[&str] = &[
    "avdec_h264",
    "openh264dec",
    "vp9dec",
    "avdec_vp9",
    "dav1ddec",
    "avdec_av1",
    "av1dec",
];
const HARDWARE_DECODER_ELEMENT_NAMES: &[&str] = &[
    "vah264dec",
    "vaapih264dec",
    "nvh264dec",
    "vdph264dec",
    "vavp9dec",
    "vaapivp9dec",
    "nvvp9dec",
    "vaav1dec",
    "vaapiav1dec",
    "nvav1dec",
];
const DECODER_ELEMENT_NAMES: &[&str] = &[
    "avdec_h264",
    "openh264dec",
    "vah264dec",
    "vaapih264dec",
    "nvh264dec",
    "vdph264dec",
    "vp9dec",
    "avdec_vp9",
    "vavp9dec",
    "vaapivp9dec",
    "nvvp9dec",
    "dav1ddec",
    "avdec_av1",
    "av1dec",
    "vaav1dec",
    "vaapiav1dec",
    "nvav1dec",
];
const DECODER_RANK_BOOST: i32 = 512;
const VIDEO_SINK_DEFAULT_MAX_LATENESS_NS: u64 = 50_000_000;
const VIDEO_SINK_MIN_MAX_LATENESS_NS: u64 = 8_000_000;
const VIDEO_SINK_MAX_MAX_LATENESS_NS: u64 = 50_000_000;
const DMABUF_MEMORY_FEATURE: &str = "memory:DMABuf";
const GPU_MEMORY_FEATURES: &[&str] = &[
    DMABUF_MEMORY_FEATURE,
    "memory:GLMemory",
    "memory:VulkanImage",
    "memory:VaapiSurface",
    "memory:CUDAMemory",
    "memory:NVMM",
];

static DECODER_RANK_STATE: OnceLock<Mutex<DecoderRankState>> = OnceLock::new();

#[derive(Debug, Default)]
struct DecoderRankState {
    original_ranks: BTreeMap<String, gst::Rank>,
}

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
                    || pipeline.decoder_policy != plan.decoder_policy
                    || pipeline.frame_limiter_required
                        != frame_limiter_required(plan.target_max_fps)
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
            .map(|(output_name, pipeline)| {
                let VideoPipelineDiagnostics {
                    actual_decoder_reports: current_decoder_reports,
                    caps_reports,
                    allocation_reports,
                    zero_copy_evidence: _,
                    memory_path: _,
                } = pipeline.diagnostics.snapshot(&pipeline.element);
                let actual_decoder_reports = merge_decoder_reports(
                    current_decoder_reports,
                    pipeline.observed_decoder_reports.values().cloned(),
                );
                let zero_copy_evidence = zero_copy_evidence(&actual_decoder_reports, &caps_reports);
                let memory_path = video_memory_path(&actual_decoder_reports, &caps_reports);
                let retention_report = video_memory_retention_report(
                    &memory_path,
                    &allocation_reports,
                    &pipeline.sink_tuning,
                );
                let frame_limiter_max_fps = pipeline
                    .frame_limiter
                    .as_ref()
                    .and_then(FrameLimiter::target_max_fps);
                VideoPipelineSnapshot {
                    output_name: output_name.clone(),
                    source: pipeline.source.display().to_string(),
                    mode: pipeline.mode,
                    gst_state: pipeline.gst_state.name().to_string(),
                    loop_playback: pipeline.loop_playback,
                    muted: pipeline.muted,
                    target_max_fps: pipeline.target_max_fps,
                    sink_tuning: pipeline.sink_tuning.clone(),
                    frame_limiter_enabled: frame_limiter_max_fps.is_some(),
                    frame_limiter_max_fps,
                    frame_stats: pipeline.frame_stats.clone(),
                    decoder_policy: pipeline.decoder_policy,
                    decoder_policy_status: decoder_policy_status(
                        pipeline.decoder_policy,
                        &actual_decoder_reports,
                    ),
                    start_offset_ms: pipeline.start_offset_ms,
                    position_ms: playback_position_ms(&pipeline.element),
                    duration_ms: playback_duration_ms(&pipeline.element),
                    actual_decoders: actual_decoder_reports
                        .iter()
                        .map(|report| report.element.clone())
                        .collect(),
                    actual_decoder_reports,
                    caps_reports,
                    allocation_reports,
                    zero_copy_evidence,
                    memory_path,
                    retention_report,
                }
            })
            .collect()
    }
}

struct VideoPipeline {
    element: gst::Element,
    frame_limiter: Option<FrameLimiter>,
    frame_limiter_required: bool,
    sink_tuning: VideoSinkTuningReport,
    source: std::path::PathBuf,
    mode: RenderMode,
    gst_state: gst::State,
    loop_playback: bool,
    muted: bool,
    target_max_fps: Option<u32>,
    decoder_policy: VideoDecoderPolicy,
    start_offset_ms: u64,
    frame_stats: VideoFrameStats,
    diagnostics: VideoPipelineDiagnosticsCache,
    observed_decoder_reports: BTreeMap<String, VideoDecoderReport>,
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
            frame_limiter_required: frame_limiter_required(plan.target_max_fps),
            sink_tuning: pipeline.sink_tuning,
            source: plan.source.clone(),
            mode: RenderMode::Paused,
            gst_state: gst::State::Null,
            loop_playback: plan.loop_playback,
            muted: !plan.muted,
            target_max_fps: plan.target_max_fps,
            decoder_policy: plan.decoder_policy,
            start_offset_ms: 0,
            frame_stats: VideoFrameStats::default(),
            diagnostics: VideoPipelineDiagnosticsCache::default(),
            observed_decoder_reports: BTreeMap::new(),
        };
        pipeline.apply_muted(plan.muted);
        Ok(pipeline)
    }

    fn apply_plan(&mut self, plan: &VideoWallpaperPlan) -> Result<(), VideoRendererError> {
        self.loop_playback = plan.loop_playback;
        self.apply_target_max_fps(plan.target_max_fps);
        self.apply_muted(plan.muted);
        self.decoder_policy = plan.decoder_policy;
        self.apply_start_offset(plan.start_offset_ms)?;
        Ok(())
    }

    fn apply_target_max_fps(&mut self, target_max_fps: Option<u32>) {
        if self.target_max_fps == target_max_fps {
            return;
        }
        self.target_max_fps = target_max_fps;
        if let Some(frame_limiter) = &mut self.frame_limiter {
            frame_limiter.apply_target_max_fps(target_max_fps);
            self.sink_tuning = frame_limiter.sink_tuning();
        }
    }

    fn apply_mode(&mut self, mode: RenderMode) -> Result<(), VideoRendererError> {
        let state = gst_state_for_mode(mode);
        if self.mode == mode && self.gst_state == state {
            return Ok(());
        }
        self.mode = mode;
        self.set_state(state)
    }

    fn stop(mut self) -> Result<(), VideoRendererError> {
        self.set_state(gst::State::Null)
    }

    fn poll_bus(&mut self) -> Result<(), VideoRendererError> {
        let Some(bus) = self.element.bus() else {
            return Ok(());
        };
        while let Some(message) = bus.pop() {
            if let Some(report) = decoder_report_from_message(&message) {
                self.observed_decoder_reports
                    .entry(report.element.clone())
                    .or_insert(report);
            }
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
                gst::MessageView::Qos(qos) => {
                    let (processed, dropped) = qos.stats();
                    let (jitter, proportion, _) = qos.values();
                    self.frame_stats.record_qos_values(
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

    fn set_state(&mut self, state: gst::State) -> Result<(), VideoRendererError> {
        if self.gst_state == state {
            return Ok(());
        }
        self.element
            .set_state(state)
            .map_err(|err| VideoRendererError::SetState(err.to_string()))?;
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

    fn apply_start_offset(&mut self, start_offset_ms: u64) -> Result<(), VideoRendererError> {
        if self.start_offset_ms == start_offset_ms {
            return Ok(());
        }
        let position = gst::ClockTime::from_mseconds(start_offset_ms);
        self.element
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position)
            .map_err(|err| VideoRendererError::Seek(err.to_string()))?;
        self.start_offset_ms = start_offset_ms;
        Ok(())
    }
}

fn gst_state_for_mode(mode: RenderMode) -> gst::State {
    match mode {
        RenderMode::Active | RenderMode::Throttled => gst::State::Playing,
        RenderMode::Paused => gst::State::Paused,
    }
}

struct BuiltPipeline {
    element: gst::Element,
    frame_limiter: Option<FrameLimiter>,
    sink_tuning: VideoSinkTuningReport,
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
            sink_tuning: VideoSinkTuningReport::default(),
        });
    }

    let uri = gst::glib::filename_to_uri(&plan.source, None::<&str>)
        .map_err(|err| VideoRendererError::Uri(err.to_string()))?;
    apply_decoder_rank_policy(plan.decoder_policy);
    let video_sink = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .property("enable-last-sample", false)
        .build()
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    let sink_tuning = configure_video_sink_low_memory(&video_sink, plan.target_max_fps);
    let frame_limiter = plan
        .target_max_fps
        .filter(|target_max_fps| *target_max_fps > 0)
        .map(|target_max_fps| FrameLimiter::new(&video_sink, target_max_fps))
        .transpose()?;
    let builder = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .property_from_str("flags", playbin_flags(plan.muted))
        .property("video-sink", &video_sink);
    let element = builder
        .build()
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    Ok(BuiltPipeline {
        element,
        frame_limiter,
        sink_tuning,
    })
}

fn playbin_flags(muted: bool) -> &'static str {
    if muted {
        MUTED_PLAYBIN_FLAGS
    } else {
        AUDIBLE_PLAYBIN_FLAGS
    }
}

fn frame_limiter_required(target_max_fps: Option<u32>) -> bool {
    target_max_fps.is_some_and(|target_max_fps| target_max_fps > 0)
}

pub fn actual_decoder_elements(element: &gst::Element) -> Vec<String> {
    let Ok(bin) = element.clone().downcast::<gst::Bin>() else {
        return Vec::new();
    };
    let mut iterator = bin.iterate_recurse();
    let mut decoders = Vec::new();
    while let Ok(Some(child)) = iterator.next() {
        let Some(factory) = child.factory() else {
            continue;
        };
        let name = factory.name();
        if DECODER_ELEMENT_NAMES.contains(&name.as_str()) {
            decoders.push(name.to_string());
        }
    }
    decoders.sort();
    decoders.dedup();
    decoders
}

pub fn actual_decoder_reports(element: &gst::Element) -> Vec<VideoDecoderReport> {
    actual_decoder_elements(element)
        .into_iter()
        .map(|element| VideoDecoderReport {
            class: decoder_class(&element),
            element,
        })
        .collect()
}

pub(crate) fn decoder_report_from_message(message: &gst::Message) -> Option<VideoDecoderReport> {
    let src = message.src()?;
    let element = src.downcast_ref::<gst::Element>()?;
    decoder_report_for_element(element)
}

fn decoder_report_for_element(element: &gst::Element) -> Option<VideoDecoderReport> {
    let factory = element.factory()?;
    let element = factory.name().to_string();
    DECODER_ELEMENT_NAMES
        .contains(&element.as_str())
        .then(|| VideoDecoderReport {
            class: decoder_class(&element),
            element,
        })
}

pub(crate) fn merge_decoder_reports<I>(
    mut current: Vec<VideoDecoderReport>,
    observed: I,
) -> Vec<VideoDecoderReport>
where
    I: IntoIterator<Item = VideoDecoderReport>,
{
    for report in observed {
        if !current
            .iter()
            .any(|current| current.element == report.element)
        {
            current.push(report);
        }
    }
    current.sort_by(|left, right| left.element.cmp(&right.element));
    current
}

pub fn apply_decoder_rank_policy(policy: VideoDecoderPolicy) {
    let state = DECODER_RANK_STATE.get_or_init(|| Mutex::new(DecoderRankState::default()));
    let Ok(mut state) = state.lock() else {
        return;
    };

    for element in DECODER_ELEMENT_NAMES {
        let Some(factory) = gst::ElementFactory::find(element) else {
            continue;
        };
        let original_rank = *state
            .original_ranks
            .entry((*element).to_owned())
            .or_insert_with(|| factory.rank());
        factory.set_rank(decoder_policy_rank(policy, element, original_rank));
    }
}

fn decoder_policy_rank(
    policy: VideoDecoderPolicy,
    element: &str,
    original_rank: gst::Rank,
) -> gst::Rank {
    let is_hardware = HARDWARE_DECODER_ELEMENT_NAMES.contains(&element);
    let is_software = SOFTWARE_DECODER_ELEMENT_NAMES.contains(&element);

    match policy {
        VideoDecoderPolicy::Auto => original_rank,
        VideoDecoderPolicy::HardwarePreferred if is_hardware => {
            gst::Rank::PRIMARY + DECODER_RANK_BOOST
        }
        VideoDecoderPolicy::HardwarePreferred if is_software => {
            rank_at_least(original_rank, gst::Rank::SECONDARY)
        }
        VideoDecoderPolicy::HardwareRequired if is_hardware => {
            gst::Rank::PRIMARY + DECODER_RANK_BOOST
        }
        VideoDecoderPolicy::HardwareRequired if is_software => gst::Rank::NONE,
        VideoDecoderPolicy::Software if is_hardware => gst::Rank::NONE,
        VideoDecoderPolicy::Software if is_software => gst::Rank::PRIMARY + DECODER_RANK_BOOST,
        _ => original_rank,
    }
}

fn rank_at_least(rank: gst::Rank, minimum: gst::Rank) -> gst::Rank {
    if i32::from(rank) < i32::from(minimum) {
        minimum
    } else {
        rank
    }
}

pub fn decoder_policy_status(
    policy: VideoDecoderPolicy,
    reports: &[VideoDecoderReport],
) -> VideoDecoderPolicyStatus {
    if policy == VideoDecoderPolicy::Auto {
        return VideoDecoderPolicyStatus::NotApplicable;
    }
    if reports.is_empty() {
        return VideoDecoderPolicyStatus::NotObserved;
    }

    let has_hardware = reports
        .iter()
        .any(|report| report.class == VideoDecoderClass::Hardware);
    let has_software = reports
        .iter()
        .any(|report| report.class == VideoDecoderClass::Software);
    let has_unknown = reports
        .iter()
        .any(|report| report.class == VideoDecoderClass::Unknown);

    match policy {
        VideoDecoderPolicy::Auto => VideoDecoderPolicyStatus::NotApplicable,
        VideoDecoderPolicy::HardwarePreferred if has_hardware => {
            VideoDecoderPolicyStatus::Satisfied
        }
        VideoDecoderPolicy::HardwarePreferred if has_unknown => {
            VideoDecoderPolicyStatus::UnknownDecoder
        }
        VideoDecoderPolicy::HardwarePreferred if has_software => {
            VideoDecoderPolicyStatus::SoftwareFallback
        }
        VideoDecoderPolicy::HardwarePreferred => VideoDecoderPolicyStatus::NotObserved,
        VideoDecoderPolicy::HardwareRequired if has_unknown => {
            VideoDecoderPolicyStatus::UnknownDecoder
        }
        VideoDecoderPolicy::HardwareRequired if has_hardware && !has_software => {
            VideoDecoderPolicyStatus::Satisfied
        }
        VideoDecoderPolicy::HardwareRequired => VideoDecoderPolicyStatus::Violated,
        VideoDecoderPolicy::Software if has_unknown => VideoDecoderPolicyStatus::UnknownDecoder,
        VideoDecoderPolicy::Software if has_software && !has_hardware => {
            VideoDecoderPolicyStatus::Satisfied
        }
        VideoDecoderPolicy::Software => VideoDecoderPolicyStatus::Violated,
    }
}

pub(crate) fn configure_video_sink_low_memory(
    sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> VideoSinkTuningReport {
    set_optional_bool_property(sink, "async", false);
    set_optional_bool_property(sink, "enable-last-sample", false);
    set_optional_bool_property(sink, "qos", true);
    set_optional_i64_property(
        sink,
        "max-lateness",
        video_sink_max_lateness_ns(target_max_fps),
    );
    set_optional_u64_property(sink, "render-delay", 0);
    set_optional_u64_property(sink, "processing-deadline", 0);
    set_optional_bool_property(sink, "show-preroll-frame", false);
    set_optional_enum_property_from_str(sink, "reconfigure-on-window-resize", "never");
    video_sink_tuning_report(sink)
}

fn update_video_sink_max_lateness(
    sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> VideoSinkTuningReport {
    set_optional_i64_property(
        sink,
        "max-lateness",
        video_sink_max_lateness_ns(target_max_fps),
    );
    video_sink_tuning_report(sink)
}

fn video_sink_max_lateness_ns(target_max_fps: Option<u32>) -> i64 {
    let max_lateness_ns = target_max_fps
        .filter(|target_max_fps| *target_max_fps > 0)
        .map(|target_max_fps| 1_000_000_000_u64 / u64::from(target_max_fps))
        .unwrap_or(VIDEO_SINK_DEFAULT_MAX_LATENESS_NS)
        .clamp(
            VIDEO_SINK_MIN_MAX_LATENESS_NS,
            VIDEO_SINK_MAX_MAX_LATENESS_NS,
        );
    i64::try_from(max_lateness_ns).unwrap_or(i64::MAX)
}

fn set_optional_bool_property(element: &gst::Element, name: &str, value: bool) {
    if element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

fn set_optional_i64_property(element: &gst::Element, name: &str, value: i64) {
    if element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

fn set_optional_u64_property(element: &gst::Element, name: &str, value: u64) {
    if element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

fn set_optional_enum_property_from_str(element: &gst::Element, name: &str, value: &str) {
    if element.find_property(name).is_some() {
        element.set_property_from_str(name, value);
    }
}

fn optional_bool_property(element: &gst::Element, name: &str) -> Option<bool> {
    element
        .find_property(name)
        .is_some()
        .then(|| element.property::<bool>(name))
}

fn optional_i64_property(element: &gst::Element, name: &str) -> Option<i64> {
    element
        .find_property(name)
        .is_some()
        .then(|| element.property::<i64>(name))
}

fn optional_u64_property(element: &gst::Element, name: &str) -> Option<u64> {
    element
        .find_property(name)
        .is_some()
        .then(|| element.property::<u64>(name))
}

fn video_sink_tuning_report(sink: &gst::Element) -> VideoSinkTuningReport {
    VideoSinkTuningReport {
        sink_element: Some(
            sink.factory()
                .map(|factory| factory.name().to_string())
                .unwrap_or_else(|| sink.name().to_string()),
        ),
        async_enabled: optional_bool_property(sink, "async"),
        last_sample_enabled: optional_bool_property(sink, "enable-last-sample"),
        qos_enabled: optional_bool_property(sink, "qos"),
        max_lateness_ns: optional_i64_property(sink, "max-lateness"),
        render_delay_ns: optional_u64_property(sink, "render-delay"),
        processing_deadline_ns: optional_u64_property(sink, "processing-deadline"),
        preroll_frame_enabled: optional_bool_property(sink, "show-preroll-frame"),
    }
}

pub fn video_caps_reports(element: &gst::Element) -> Vec<VideoCapsReport> {
    let mut reports = Vec::new();
    push_element_caps_reports(element, &mut reports);

    if let Ok(bin) = element.clone().downcast::<gst::Bin>() {
        let mut iterator = bin.iterate_recurse();
        while let Ok(Some(child)) = iterator.next() {
            push_element_caps_reports(&child, &mut reports);
        }
    }

    reports.sort_by(|left, right| {
        (
            left.element.as_str(),
            left.pad.as_str(),
            left.direction.as_str(),
            left.caps.as_str(),
        )
            .cmp(&(
                right.element.as_str(),
                right.pad.as_str(),
                right.direction.as_str(),
                right.caps.as_str(),
            ))
    });
    reports.dedup();
    reports
}

pub fn video_allocation_reports(element: &gst::Element) -> Vec<VideoAllocationReport> {
    let mut reports = Vec::new();
    push_element_allocation_reports(element, &mut reports);

    if let Ok(bin) = element.clone().downcast::<gst::Bin>() {
        let mut iterator = bin.iterate_recurse();
        while let Ok(Some(child)) = iterator.next() {
            push_element_allocation_reports(&child, &mut reports);
        }
    }

    reports.sort_by(|left, right| {
        (
            left.element.as_str(),
            left.pad.as_str(),
            left.direction.as_str(),
            left.query_scope.as_str(),
            left.caps.as_str(),
        )
            .cmp(&(
                right.element.as_str(),
                right.pad.as_str(),
                right.direction.as_str(),
                right.query_scope.as_str(),
                right.caps.as_str(),
            ))
    });
    reports.dedup();
    reports
}

#[derive(Debug)]
pub(crate) struct VideoPipelineDiagnosticsCache {
    refresh_interval: Duration,
    state: RefCell<Option<CachedVideoPipelineDiagnostics>>,
}

impl Default for VideoPipelineDiagnosticsCache {
    fn default() -> Self {
        Self {
            refresh_interval: VIDEO_DIAGNOSTICS_REFRESH_INTERVAL,
            state: RefCell::new(None),
        }
    }
}

impl VideoPipelineDiagnosticsCache {
    #[cfg(test)]
    fn with_refresh_interval(refresh_interval: Duration) -> Self {
        Self {
            refresh_interval,
            state: RefCell::new(None),
        }
    }

    pub(crate) fn snapshot(&self, element: &gst::Element) -> VideoPipelineDiagnostics {
        let now = Instant::now();
        if let Some(cached) = self.state.borrow().as_ref()
            && now.duration_since(cached.sampled_at) < self.refresh_interval
        {
            return cached.diagnostics.clone();
        }

        let diagnostics = collect_video_pipeline_diagnostics(element);
        *self.state.borrow_mut() = Some(CachedVideoPipelineDiagnostics {
            sampled_at: now,
            diagnostics: diagnostics.clone(),
        });
        diagnostics
    }

    pub(crate) fn invalidate(&self) {
        *self.state.borrow_mut() = None;
    }
}

#[derive(Debug, Clone)]
struct CachedVideoPipelineDiagnostics {
    sampled_at: Instant,
    diagnostics: VideoPipelineDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VideoPipelineDiagnostics {
    pub(crate) actual_decoder_reports: Vec<VideoDecoderReport>,
    pub(crate) caps_reports: Vec<VideoCapsReport>,
    pub(crate) allocation_reports: Vec<VideoAllocationReport>,
    pub(crate) zero_copy_evidence: VideoZeroCopyEvidence,
    pub(crate) memory_path: VideoMemoryPathReport,
}

fn collect_video_pipeline_diagnostics(element: &gst::Element) -> VideoPipelineDiagnostics {
    let actual_decoder_reports = actual_decoder_reports(element);
    let caps_reports = video_caps_reports(element);
    let allocation_reports = video_allocation_reports(element);
    let zero_copy_evidence = zero_copy_evidence(&actual_decoder_reports, &caps_reports);
    let memory_path = video_memory_path(&actual_decoder_reports, &caps_reports);
    VideoPipelineDiagnostics {
        actual_decoder_reports,
        caps_reports,
        allocation_reports,
        zero_copy_evidence,
        memory_path,
    }
}

pub fn zero_copy_evidence(
    decoder_reports: &[VideoDecoderReport],
    caps_reports: &[VideoCapsReport],
) -> VideoZeroCopyEvidence {
    let decoder_classes = decoder_report_classes(decoder_reports);
    let memory_features = caps_report_memory_features(caps_reports, false);
    let sink_memory_features = caps_report_memory_features(caps_reports, true);
    let has_hardware_decoder = decoder_classes.contains(&VideoDecoderClass::Hardware);
    let has_software_decoder = decoder_classes.contains(&VideoDecoderClass::Software);
    let has_dmabuf_caps = memory_features
        .iter()
        .any(|feature| is_dmabuf_memory_feature(feature));
    let has_sink_dmabuf_caps = sink_memory_features
        .iter()
        .any(|feature| is_dmabuf_memory_feature(feature));
    let has_gpu_memory_caps = memory_features
        .iter()
        .any(|feature| is_gpu_memory_feature(feature));
    let has_sink_gpu_memory_caps = sink_memory_features
        .iter()
        .any(|feature| is_gpu_memory_feature(feature));

    let level = if has_sink_dmabuf_caps {
        VideoZeroCopyEvidenceLevel::SinkDmabufCaps
    } else if has_sink_gpu_memory_caps {
        VideoZeroCopyEvidenceLevel::SinkGpuMemoryCaps
    } else if has_dmabuf_caps {
        VideoZeroCopyEvidenceLevel::DmabufCaps
    } else if has_gpu_memory_caps {
        VideoZeroCopyEvidenceLevel::GpuMemoryCaps
    } else if has_hardware_decoder {
        VideoZeroCopyEvidenceLevel::HardwareDecode
    } else if has_software_decoder {
        VideoZeroCopyEvidenceLevel::SoftwareDecode
    } else {
        VideoZeroCopyEvidenceLevel::Missing
    };

    VideoZeroCopyEvidence {
        level,
        decoder_classes,
        memory_features,
        sink_memory_features,
        notes: zero_copy_evidence_notes(level),
    }
}

pub fn video_memory_path(
    decoder_reports: &[VideoDecoderReport],
    caps_reports: &[VideoCapsReport],
) -> VideoMemoryPathReport {
    let decoder_classes = decoder_report_classes(decoder_reports);
    let has_hardware_decoder = decoder_classes.contains(&VideoDecoderClass::Hardware);
    let has_software_decoder = decoder_classes.contains(&VideoDecoderClass::Software);
    let segments = video_memory_path_segments(caps_reports);
    let has_cpu_raw_caps = segments
        .iter()
        .any(|segment| segment.memory_class == VideoMemoryClass::SystemMemory);
    let has_decoder_gpu_caps = caps_reports.iter().any(|report| {
        report.direction == "src"
            && report
                .memory_features
                .iter()
                .any(|feature| is_gpu_memory_feature(feature))
    });
    let has_decoder_dmabuf_caps = caps_reports.iter().any(|report| {
        report.direction == "src"
            && report
                .memory_features
                .iter()
                .any(|feature| is_dmabuf_memory_feature(feature))
    });
    let has_sink_gpu_caps = caps_reports.iter().any(|report| {
        report.direction == "sink"
            && report
                .memory_features
                .iter()
                .any(|feature| is_gpu_memory_feature(feature))
    });
    let has_sink_dmabuf_caps = caps_reports.iter().any(|report| {
        report.direction == "sink"
            && report
                .memory_features
                .iter()
                .any(|feature| is_dmabuf_memory_feature(feature))
    });

    let level = if has_sink_dmabuf_caps {
        VideoMemoryPathLevel::SinkDmabuf
    } else if has_sink_gpu_caps {
        VideoMemoryPathLevel::SinkGpuMemory
    } else if has_decoder_dmabuf_caps {
        VideoMemoryPathLevel::DecoderDmabuf
    } else if has_decoder_gpu_caps {
        VideoMemoryPathLevel::DecoderGpuMemory
    } else if has_hardware_decoder && has_cpu_raw_caps {
        VideoMemoryPathLevel::HardwareDecodeCpuRaw
    } else if has_software_decoder && has_cpu_raw_caps {
        VideoMemoryPathLevel::SoftwareDecodeCpuRaw
    } else if has_cpu_raw_caps {
        VideoMemoryPathLevel::CpuRawCaps
    } else if has_hardware_decoder {
        VideoMemoryPathLevel::HardwareDecodeNoCaps
    } else if has_software_decoder {
        VideoMemoryPathLevel::SoftwareDecodeNoCaps
    } else {
        VideoMemoryPathLevel::Unknown
    };

    VideoMemoryPathReport {
        level,
        segments,
        notes: video_memory_path_notes(level),
    }
}

pub fn video_memory_retention_report(
    memory_path: &VideoMemoryPathReport,
    allocation_reports: &[VideoAllocationReport],
    sink_tuning: &VideoSinkTuningReport,
) -> VideoMemoryRetentionReport {
    let pool_stats = video_allocation_pool_stats(allocation_reports);
    let sink_frame_retention = sink_frame_retention(sink_tuning);
    let has_retained_sink_frame = matches!(
        sink_frame_retention,
        VideoSinkFrameRetention::LastSample
            | VideoSinkFrameRetention::PrerollFrame
            | VideoSinkFrameRetention::LastSampleAndPrerollFrame
    );
    let has_cpu_raw_path = matches!(
        memory_path.level,
        VideoMemoryPathLevel::CpuRawCaps
            | VideoMemoryPathLevel::SoftwareDecodeCpuRaw
            | VideoMemoryPathLevel::HardwareDecodeCpuRaw
    );
    let has_decoder_only_gpu_path = matches!(
        memory_path.level,
        VideoMemoryPathLevel::DecoderGpuMemory | VideoMemoryPathLevel::DecoderDmabuf
    );
    let has_pool_capacity = pool_stats.estimated_min_pool_bytes > 0;
    let has_unbounded_pool = pool_stats.estimated_max_pool_bytes.is_none();

    let level =
        if has_retained_sink_frame || has_cpu_raw_path || pool_stats.system_memory_pool_reports > 0
        {
            VideoMemoryRetentionLevel::High
        } else if has_decoder_only_gpu_path || has_pool_capacity || has_unbounded_pool {
            VideoMemoryRetentionLevel::Medium
        } else if memory_path.level == VideoMemoryPathLevel::Unknown
            && pool_stats.pool_reports == 0
            && sink_frame_retention == VideoSinkFrameRetention::Unknown
        {
            VideoMemoryRetentionLevel::Unknown
        } else {
            VideoMemoryRetentionLevel::Low
        };

    VideoMemoryRetentionReport {
        level,
        estimated_min_pool_bytes: pool_stats.estimated_min_pool_bytes,
        estimated_max_pool_bytes: pool_stats.estimated_max_pool_bytes,
        pool_reports: pool_stats.pool_reports,
        system_memory_pool_reports: pool_stats.system_memory_pool_reports,
        gpu_memory_pool_reports: pool_stats.gpu_memory_pool_reports,
        dmabuf_pool_reports: pool_stats.dmabuf_pool_reports,
        other_memory_pool_reports: pool_stats.other_memory_pool_reports,
        sink_frame_retention,
        notes: video_memory_retention_notes(level, memory_path, sink_frame_retention, &pool_stats),
    }
}

#[derive(Debug, Default)]
struct VideoAllocationPoolStats {
    estimated_min_pool_bytes: u64,
    estimated_max_pool_bytes: Option<u64>,
    pool_reports: usize,
    system_memory_pool_reports: usize,
    gpu_memory_pool_reports: usize,
    dmabuf_pool_reports: usize,
    other_memory_pool_reports: usize,
}

fn video_allocation_pool_stats(
    allocation_reports: &[VideoAllocationReport],
) -> VideoAllocationPoolStats {
    let mut stats = VideoAllocationPoolStats {
        estimated_max_pool_bytes: Some(0),
        ..VideoAllocationPoolStats::default()
    };

    for report in allocation_reports {
        if report.pools.is_empty() {
            continue;
        }

        let report_memory_class = allocation_report_memory_class(report);
        for pool in &report.pools {
            stats.pool_reports += 1;
            match allocation_pool_memory_class(pool).unwrap_or(report_memory_class) {
                VideoMemoryClass::SystemMemory => stats.system_memory_pool_reports += 1,
                VideoMemoryClass::GpuMemory => stats.gpu_memory_pool_reports += 1,
                VideoMemoryClass::Dmabuf => stats.dmabuf_pool_reports += 1,
                VideoMemoryClass::OtherMemoryFeature => stats.other_memory_pool_reports += 1,
            }
            stats.estimated_min_pool_bytes = stats
                .estimated_min_pool_bytes
                .saturating_add(pool_buffer_bytes(pool.size, pool.min_buffers));
            if pool.max_buffers == 0 {
                stats.estimated_max_pool_bytes = None;
            } else if let Some(max_bytes) = stats.estimated_max_pool_bytes.as_mut() {
                *max_bytes =
                    max_bytes.saturating_add(pool_buffer_bytes(pool.size, pool.max_buffers));
            }
        }
    }

    stats
}

fn allocation_pool_memory_class(pool: &VideoAllocationPoolReport) -> Option<VideoMemoryClass> {
    if contains_dmabuf_memory_hint(&pool.pool) {
        Some(VideoMemoryClass::Dmabuf)
    } else if contains_gpu_memory_hint(&pool.pool) {
        Some(VideoMemoryClass::GpuMemory)
    } else {
        None
    }
}

fn allocation_report_memory_class(report: &VideoAllocationReport) -> VideoMemoryClass {
    if contains_dmabuf_memory_hint(&report.caps)
        || report
            .params
            .iter()
            .any(|param| contains_dmabuf_memory_hint(&param.allocator))
    {
        return VideoMemoryClass::Dmabuf;
    }
    if contains_gpu_memory_hint(&report.caps)
        || report
            .params
            .iter()
            .any(|param| contains_gpu_memory_hint(&param.allocator))
    {
        return VideoMemoryClass::GpuMemory;
    }
    if report.caps.contains("video/") {
        VideoMemoryClass::SystemMemory
    } else {
        VideoMemoryClass::OtherMemoryFeature
    }
}

fn contains_dmabuf_memory_hint(value: &str) -> bool {
    value.contains(DMABUF_MEMORY_FEATURE) || value.to_ascii_lowercase().contains("dmabuf")
}

fn contains_gpu_memory_hint(value: &str) -> bool {
    GPU_MEMORY_FEATURES
        .iter()
        .any(|feature| value.contains(feature))
        || {
            let lower = value.to_ascii_lowercase();
            lower.contains("glmemory")
                || lower.contains("glbuffer")
                || lower.contains("vulkan")
                || lower.contains("vaapi")
                || lower.contains("cuda")
                || lower.contains("nvmm")
        }
}

fn pool_buffer_bytes(size: u32, buffers: u32) -> u64 {
    u64::from(size).saturating_mul(u64::from(buffers))
}

fn sink_frame_retention(sink_tuning: &VideoSinkTuningReport) -> VideoSinkFrameRetention {
    let last_sample = sink_tuning.last_sample_enabled;
    let preroll_frame = sink_tuning.preroll_frame_enabled;
    match (last_sample, preroll_frame) {
        (Some(true), Some(true)) => VideoSinkFrameRetention::LastSampleAndPrerollFrame,
        (Some(true), _) => VideoSinkFrameRetention::LastSample,
        (_, Some(true)) => VideoSinkFrameRetention::PrerollFrame,
        (Some(false), _) | (_, Some(false)) => VideoSinkFrameRetention::Disabled,
        _ => VideoSinkFrameRetention::Unknown,
    }
}

fn video_memory_retention_notes(
    level: VideoMemoryRetentionLevel,
    memory_path: &VideoMemoryPathReport,
    sink_frame_retention: VideoSinkFrameRetention,
    pool_stats: &VideoAllocationPoolStats,
) -> Vec<String> {
    let mut notes = Vec::new();

    match sink_frame_retention {
        VideoSinkFrameRetention::LastSample => {
            notes.push("sink last-sample can retain the most recent frame".to_owned());
        }
        VideoSinkFrameRetention::PrerollFrame => {
            notes.push("sink preroll frame can retain a decoded frame while paused".to_owned());
        }
        VideoSinkFrameRetention::LastSampleAndPrerollFrame => {
            notes.push("sink last-sample and preroll frame retention are enabled".to_owned());
        }
        VideoSinkFrameRetention::Disabled => {
            notes.push("sink last-sample and preroll frame retention are disabled".to_owned());
        }
        VideoSinkFrameRetention::Unknown => {
            notes.push("sink frame retention properties were not observed".to_owned());
        }
    }

    if matches!(
        memory_path.level,
        VideoMemoryPathLevel::CpuRawCaps
            | VideoMemoryPathLevel::SoftwareDecodeCpuRaw
            | VideoMemoryPathLevel::HardwareDecodeCpuRaw
    ) {
        notes.push("system-memory raw video caps can retain CPU-side frames".to_owned());
    }
    if matches!(
        memory_path.level,
        VideoMemoryPathLevel::DecoderGpuMemory | VideoMemoryPathLevel::DecoderDmabuf
    ) {
        notes.push(
            "GPU/DMABuf caps are only observed before the sink; an implicit copy is still possible"
                .to_owned(),
        );
    }
    if pool_stats.estimated_min_pool_bytes > 0 {
        notes.push(format!(
            "allocation pools report at least {} bytes of minimum buffer capacity",
            pool_stats.estimated_min_pool_bytes
        ));
    }
    if pool_stats.estimated_max_pool_bytes.is_none() {
        notes.push(
            "at least one allocation pool reports an unbounded or unknown max_buffers value"
                .to_owned(),
        );
    }
    if pool_stats.system_memory_pool_reports > 0 {
        notes.push("allocation query reports system-memory video buffer pools".to_owned());
    }
    if notes.is_empty() {
        notes.push(
            match level {
                VideoMemoryRetentionLevel::Unknown => {
                    "no negotiated caps, allocation pools, or sink retention properties observed"
                }
                VideoMemoryRetentionLevel::Low => {
                    "no CPU-side frame retention risk observed in current runtime evidence"
                }
                VideoMemoryRetentionLevel::Medium => {
                    "buffer-pool or decoder-side GPU evidence needs real PSS/USS correlation"
                }
                VideoMemoryRetentionLevel::High => {
                    "CPU-side frame retention risk observed in current runtime evidence"
                }
            }
            .to_owned(),
        );
    }

    notes
}

fn video_memory_path_segments(caps_reports: &[VideoCapsReport]) -> Vec<VideoMemoryPathSegment> {
    caps_reports
        .iter()
        .flat_map(|report| {
            report
                .structures
                .iter()
                .filter(|structure| structure.media_type.starts_with("video/"))
                .map(|structure| VideoMemoryPathSegment {
                    element: report.element.clone(),
                    pad: report.pad.clone(),
                    direction: report.direction.clone(),
                    media_type: structure.media_type.clone(),
                    memory_features: structure
                        .features
                        .iter()
                        .filter(|feature| feature.starts_with("memory:"))
                        .cloned()
                        .collect(),
                    memory_class: memory_features_class(&structure.features),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn decoder_report_classes(reports: &[VideoDecoderReport]) -> Vec<VideoDecoderClass> {
    let mut classes = Vec::new();
    for class in [
        VideoDecoderClass::Hardware,
        VideoDecoderClass::Software,
        VideoDecoderClass::Unknown,
    ] {
        if reports.iter().any(|report| report.class == class) {
            classes.push(class);
        }
    }
    classes
}

fn caps_report_memory_features(caps_reports: &[VideoCapsReport], sink_only: bool) -> Vec<String> {
    let mut features = caps_reports
        .iter()
        .filter(|report| !sink_only || report.direction == "sink")
        .flat_map(|report| report.memory_features.iter().cloned())
        .collect::<Vec<_>>();
    features.sort();
    features.dedup();
    features
}

fn is_dmabuf_memory_feature(feature: &str) -> bool {
    feature == DMABUF_MEMORY_FEATURE
}

fn is_gpu_memory_feature(feature: &str) -> bool {
    GPU_MEMORY_FEATURES.contains(&feature)
}

fn memory_features_class(features: &[String]) -> VideoMemoryClass {
    if features
        .iter()
        .any(|feature| is_dmabuf_memory_feature(feature))
    {
        VideoMemoryClass::Dmabuf
    } else if features
        .iter()
        .any(|feature| is_gpu_memory_feature(feature))
    {
        VideoMemoryClass::GpuMemory
    } else if features
        .iter()
        .any(|feature| feature.starts_with("memory:"))
    {
        VideoMemoryClass::OtherMemoryFeature
    } else {
        VideoMemoryClass::SystemMemory
    }
}

fn zero_copy_evidence_notes(level: VideoZeroCopyEvidenceLevel) -> Vec<String> {
    let note = match level {
        VideoZeroCopyEvidenceLevel::Missing => "no decoder or GPU memory caps observed yet",
        VideoZeroCopyEvidenceLevel::SoftwareDecode => {
            "software decoder observed without GPU memory caps"
        }
        VideoZeroCopyEvidenceLevel::HardwareDecode => {
            "hardware decoder observed without GPU memory caps"
        }
        VideoZeroCopyEvidenceLevel::GpuMemoryCaps => "GPU memory caps observed before the sink",
        VideoZeroCopyEvidenceLevel::DmabufCaps => "DMABuf caps observed before the sink",
        VideoZeroCopyEvidenceLevel::SinkGpuMemoryCaps => "sink-side GPU memory caps observed",
        VideoZeroCopyEvidenceLevel::SinkDmabufCaps => "sink-side DMABuf caps observed",
    };
    vec![note.to_owned()]
}

fn video_memory_path_notes(level: VideoMemoryPathLevel) -> Vec<String> {
    let note = match level {
        VideoMemoryPathLevel::Unknown => "no negotiated video caps observed yet",
        VideoMemoryPathLevel::CpuRawCaps => "system-memory video caps observed",
        VideoMemoryPathLevel::SoftwareDecodeNoCaps => {
            "software decoder observed before video caps negotiation"
        }
        VideoMemoryPathLevel::SoftwareDecodeCpuRaw => {
            "software decoder with system-memory raw video caps observed"
        }
        VideoMemoryPathLevel::HardwareDecodeNoCaps => {
            "hardware decoder observed before video caps negotiation"
        }
        VideoMemoryPathLevel::HardwareDecodeCpuRaw => {
            "hardware decoder observed but negotiated caps expose system-memory raw frames"
        }
        VideoMemoryPathLevel::DecoderGpuMemory => {
            "GPU memory caps observed before the sink; sink-side GPU memory caps are not observed"
        }
        VideoMemoryPathLevel::DecoderDmabuf => {
            "DMABuf caps observed before the sink; sink-side DMABuf caps are not observed"
        }
        VideoMemoryPathLevel::SinkGpuMemory => "sink-side GPU memory caps observed",
        VideoMemoryPathLevel::SinkDmabuf => "sink-side DMABuf caps observed",
    };
    vec![note.to_owned()]
}

fn push_element_caps_reports(element: &gst::Element, reports: &mut Vec<VideoCapsReport>) {
    for pad in element.pads() {
        let Some(caps) = pad.current_caps() else {
            continue;
        };
        let structures = caps_structure_reports(&caps);
        if !caps_report_is_relevant(&structures) {
            continue;
        }
        reports.push(VideoCapsReport {
            element: element.name().to_string(),
            pad: pad.name().to_string(),
            direction: pad_direction_name(pad.direction()).to_owned(),
            caps: caps.to_string(),
            memory_features: caps_memory_features(&structures),
            structures,
        });
    }
}

fn push_element_allocation_reports(
    element: &gst::Element,
    reports: &mut Vec<VideoAllocationReport>,
) {
    for pad in element.pads() {
        if pad.direction() != gst::PadDirection::Src {
            continue;
        }
        let Some(caps) = pad.current_caps() else {
            continue;
        };
        let structures = caps_structure_reports(&caps);
        if !caps_report_is_relevant(&structures) {
            continue;
        }
        let Some(mut report) = query_pad_allocation(&pad, &caps) else {
            continue;
        };
        report.element = element.name().to_string();
        report.pad = pad.name().to_string();
        report.direction = pad_direction_name(pad.direction()).to_owned();
        report.query_scope = "peer".to_owned();
        reports.push(report);
    }
}

fn query_pad_allocation(pad: &gst::Pad, caps: &gst::Caps) -> Option<VideoAllocationReport> {
    let mut query = gst::query::Allocation::new(Some(caps), true);
    if !pad.peer_query(query.query_mut()) {
        return None;
    }
    let (query_caps, need_pool) = query.get_owned();
    Some(VideoAllocationReport {
        element: String::new(),
        pad: String::new(),
        direction: String::new(),
        query_scope: String::new(),
        caps: query_caps
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| caps.to_string()),
        need_pool,
        pools: query
            .allocation_pools()
            .map(
                |(pool, size, min_buffers, max_buffers)| VideoAllocationPoolReport {
                    pool: pool
                        .as_ref()
                        .map(|pool| pool.name().to_string())
                        .unwrap_or_else(|| "none".to_owned()),
                    size,
                    min_buffers,
                    max_buffers,
                },
            )
            .collect(),
        params: query
            .allocation_params()
            .map(|(allocator, params)| VideoAllocationParamReport {
                allocator: allocator
                    .as_ref()
                    .map(|allocator| allocator.name().to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                flags: format!("{:?}", params.flags()),
                align: params.align() as u64,
                prefix: params.prefix() as u64,
                padding: params.padding() as u64,
            })
            .collect(),
        metas: query
            .allocation_metas()
            .map(|(api, structure)| {
                structure
                    .map(ToString::to_string)
                    .unwrap_or_else(|| api.name().to_owned())
            })
            .collect(),
    })
}

fn caps_structure_reports(caps: &gst::Caps) -> Vec<VideoCapsStructureReport> {
    if caps.is_any() {
        return vec![VideoCapsStructureReport {
            media_type: "ANY".to_owned(),
            features: vec!["ANY".to_owned()],
        }];
    }
    if caps.is_empty() {
        return Vec::new();
    }

    (0..caps.size())
        .filter_map(|index| {
            let structure = caps.structure(index)?;
            let features = caps.features(index)?;
            Some(VideoCapsStructureReport {
                media_type: structure.name().to_string(),
                features: caps_feature_strings(features),
            })
        })
        .collect()
}

fn caps_feature_strings(features: &gst::CapsFeaturesRef) -> Vec<String> {
    if features.is_any() {
        return vec!["ANY".to_owned()];
    }

    (0..features.size())
        .filter_map(|index| features.nth(index).map(ToString::to_string))
        .collect()
}

fn caps_memory_features(structures: &[VideoCapsStructureReport]) -> Vec<String> {
    let mut features = structures
        .iter()
        .flat_map(|structure| structure.features.iter())
        .filter(|feature| feature.starts_with("memory:"))
        .cloned()
        .collect::<Vec<_>>();
    features.sort();
    features.dedup();
    features
}

fn caps_report_is_relevant(structures: &[VideoCapsStructureReport]) -> bool {
    structures.iter().any(|structure| {
        structure.media_type.starts_with("video/")
            || structure
                .features
                .iter()
                .any(|feature| feature.starts_with("memory:"))
    })
}

fn pad_direction_name(direction: gst::PadDirection) -> &'static str {
    match direction {
        gst::PadDirection::Src => "src",
        gst::PadDirection::Sink => "sink",
        gst::PadDirection::Unknown => "unknown",
    }
}

fn decoder_class(element: &str) -> VideoDecoderClass {
    if HARDWARE_DECODER_ELEMENT_NAMES.contains(&element) {
        VideoDecoderClass::Hardware
    } else if SOFTWARE_DECODER_ELEMENT_NAMES.contains(&element) {
        VideoDecoderClass::Software
    } else {
        VideoDecoderClass::Unknown
    }
}

struct FrameLimiter {
    sink: gst::Element,
    target_max_fps: u32,
}

impl FrameLimiter {
    fn new(sink: &gst::Element, target_max_fps: u32) -> Result<Self, VideoRendererError> {
        if sink.find_property("throttle-time").is_none() {
            return Err(VideoRendererError::BuildElement(format!(
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

    fn apply_target_max_fps(&mut self, target_max_fps: Option<u32>) {
        self.target_max_fps = target_max_fps
            .filter(|target_max_fps| *target_max_fps > 0)
            .unwrap_or(0);
        self.apply_sink_throttle();
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
        update_video_sink_max_lateness(&self.sink, self.target_max_fps());
    }

    fn sink_tuning(&self) -> VideoSinkTuningReport {
        video_sink_tuning_report(&self.sink)
    }
}

fn frame_throttle_time_ns(target_max_fps: u32) -> u64 {
    if target_max_fps == 0 {
        0
    } else {
        1_000_000_000_u64 / u64::from(target_max_fps)
    }
}

pub fn playback_position_ms(element: &gst::Element) -> Option<u64> {
    element
        .query_position::<gst::ClockTime>()
        .map(|position| position.mseconds())
}

pub fn playback_duration_ms(element: &gst::Element) -> Option<u64> {
    element
        .query_duration::<gst::ClockTime>()
        .map(|duration| duration.mseconds())
}

impl Drop for VideoPipeline {
    fn drop(&mut self) {
        let _ = self.element.set_state(gst::State::Null);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoPipelineSnapshot {
    pub output_name: String,
    pub source: String,
    pub mode: RenderMode,
    pub gst_state: String,
    pub loop_playback: bool,
    pub muted: bool,
    pub target_max_fps: Option<u32>,
    pub sink_tuning: VideoSinkTuningReport,
    pub frame_limiter_enabled: bool,
    pub frame_limiter_max_fps: Option<u32>,
    pub frame_stats: VideoFrameStats,
    pub decoder_policy: VideoDecoderPolicy,
    pub decoder_policy_status: VideoDecoderPolicyStatus,
    pub start_offset_ms: u64,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub actual_decoders: Vec<String>,
    pub actual_decoder_reports: Vec<VideoDecoderReport>,
    pub caps_reports: Vec<VideoCapsReport>,
    pub allocation_reports: Vec<VideoAllocationReport>,
    pub zero_copy_evidence: VideoZeroCopyEvidence,
    pub memory_path: VideoMemoryPathReport,
    pub retention_report: VideoMemoryRetentionReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VideoSinkTuningReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_element: Option<String>,
    pub async_enabled: Option<bool>,
    pub last_sample_enabled: Option<bool>,
    pub qos_enabled: Option<bool>,
    pub max_lateness_ns: Option<i64>,
    pub render_delay_ns: Option<u64>,
    pub processing_deadline_ns: Option<u64>,
    pub preroll_frame_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VideoFrameStats {
    pub qos_messages: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_stats_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_processed_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_dropped_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_jitter_ns_latest: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_jitter_ns_abs_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qos_proportion_x1000_latest: Option<u32>,
    pub gtk_frame_clock_ticks: u64,
    pub gtk_frame_clock_before_paint_ticks: u64,
    pub gtk_frame_clock_update_ticks: u64,
    pub gtk_frame_clock_layout_ticks: u64,
    pub gtk_frame_clock_paint_ticks: u64,
    pub gtk_frame_clock_after_paint_ticks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_counter_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_time_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_interval_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_interval_us_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_fps_x1000_latest: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_refresh_interval_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_clock_predicted_presentation_time_us_latest: Option<u64>,
    pub gtk_frame_timings_observed: u64,
    pub gtk_frame_timings_complete: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_counter_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_complete_counter_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_frame_time_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_predicted_presentation_time_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_presentation_time_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_presentation_interval_us_latest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_presentation_interval_us_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gtk_frame_timings_refresh_interval_us_latest: Option<u64>,
}

#[cfg_attr(not(feature = "gtk-renderer"), allow(dead_code))]
pub(crate) enum GtkFrameClockPhase {
    BeforePaint,
    Update,
    Layout,
    Paint,
}

impl VideoFrameStats {
    pub(crate) fn record_qos_values(
        &mut self,
        stats_format: String,
        processed: i64,
        dropped: i64,
        jitter_ns: i64,
        proportion: f64,
    ) {
        self.qos_messages = self.qos_messages.saturating_add(1);
        if !stats_format.is_empty() {
            self.qos_stats_format = Some(stats_format);
        }
        update_max_u64(&mut self.qos_processed_max, non_negative_u64(processed));
        update_max_u64(&mut self.qos_dropped_max, non_negative_u64(dropped));
        self.qos_jitter_ns_latest = Some(jitter_ns);
        update_max_u64(
            &mut self.qos_jitter_ns_abs_max,
            Some(jitter_ns.unsigned_abs()),
        );
        if proportion.is_finite() && proportion >= 0.0 {
            let scaled = (proportion * 1000.0).round();
            if scaled <= f64::from(u32::MAX) {
                self.qos_proportion_x1000_latest = Some(scaled as u32);
            }
        }
    }

    #[cfg_attr(not(feature = "gtk-renderer"), allow(dead_code))]
    pub(crate) fn record_gtk_frame_clock_tick_minimal(
        &mut self,
        frame_counter: i64,
        frame_time_us: i64,
    ) {
        self.gtk_frame_clock_ticks = self.gtk_frame_clock_ticks.saturating_add(1);
        self.gtk_frame_clock_after_paint_ticks =
            self.gtk_frame_clock_after_paint_ticks.saturating_add(1);
        self.gtk_frame_clock_counter_latest = non_negative_u64(frame_counter);
        if let Some(frame_time_us) = non_negative_u64(frame_time_us) {
            if let Some(previous_frame_time_us) = self.gtk_frame_clock_time_us_latest
                && frame_time_us >= previous_frame_time_us
            {
                let interval = frame_time_us - previous_frame_time_us;
                self.gtk_frame_clock_interval_us_latest = Some(interval);
                update_max_u64(&mut self.gtk_frame_clock_interval_us_max, Some(interval));
            }
            self.gtk_frame_clock_time_us_latest = Some(frame_time_us);
        }
    }

    #[cfg_attr(not(feature = "gtk-renderer"), allow(dead_code))]
    pub(crate) fn record_gtk_frame_clock_tick(
        &mut self,
        frame_counter: i64,
        frame_time_us: i64,
        fps: f64,
        refresh_interval_us: i64,
        predicted_presentation_time_us: i64,
    ) {
        self.record_gtk_frame_clock_tick_minimal(frame_counter, frame_time_us);
        if fps.is_finite() && fps >= 0.0 {
            let scaled = (fps * 1000.0).round();
            if scaled <= f64::from(u32::MAX) {
                self.gtk_frame_clock_fps_x1000_latest = Some(scaled as u32);
            }
        }
        self.gtk_frame_clock_refresh_interval_us_latest = non_negative_u64(refresh_interval_us);
        self.gtk_frame_clock_predicted_presentation_time_us_latest =
            non_negative_u64(predicted_presentation_time_us);
    }

    #[cfg_attr(not(feature = "gtk-renderer"), allow(dead_code))]
    pub(crate) fn record_gtk_frame_clock_phase(&mut self, phase: GtkFrameClockPhase) {
        match phase {
            GtkFrameClockPhase::BeforePaint => {
                self.gtk_frame_clock_before_paint_ticks =
                    self.gtk_frame_clock_before_paint_ticks.saturating_add(1);
            }
            GtkFrameClockPhase::Update => {
                self.gtk_frame_clock_update_ticks =
                    self.gtk_frame_clock_update_ticks.saturating_add(1);
            }
            GtkFrameClockPhase::Layout => {
                self.gtk_frame_clock_layout_ticks =
                    self.gtk_frame_clock_layout_ticks.saturating_add(1);
            }
            GtkFrameClockPhase::Paint => {
                self.gtk_frame_clock_paint_ticks =
                    self.gtk_frame_clock_paint_ticks.saturating_add(1);
            }
        }
    }

    #[cfg_attr(not(feature = "gtk-renderer"), allow(dead_code))]
    pub(crate) fn record_gtk_frame_timing(
        &mut self,
        frame_counter: i64,
        complete: bool,
        frame_time_us: i64,
        predicted_presentation_time_us: i64,
        presentation_time_us: i64,
        refresh_interval_us: i64,
    ) {
        let Some(frame_counter) = non_negative_u64(frame_counter) else {
            return;
        };

        let is_new_observed_frame = self
            .gtk_frame_timings_counter_latest
            .is_none_or(|counter| frame_counter > counter);
        if is_new_observed_frame {
            self.gtk_frame_timings_observed = self.gtk_frame_timings_observed.saturating_add(1);
            self.gtk_frame_timings_counter_latest = Some(frame_counter);
            self.gtk_frame_timings_frame_time_us_latest = non_negative_u64(frame_time_us);
            self.gtk_frame_timings_predicted_presentation_time_us_latest =
                non_negative_u64(predicted_presentation_time_us);
            self.gtk_frame_timings_refresh_interval_us_latest =
                non_negative_u64(refresh_interval_us);
        }

        if !complete
            || self
                .gtk_frame_timings_complete_counter_latest
                .is_some_and(|counter| frame_counter <= counter)
        {
            return;
        }

        self.gtk_frame_timings_complete = self.gtk_frame_timings_complete.saturating_add(1);
        self.gtk_frame_timings_complete_counter_latest = Some(frame_counter);
        if let Some(presentation_time_us) = non_negative_u64(presentation_time_us) {
            if let Some(previous_presentation_time_us) =
                self.gtk_frame_timings_presentation_time_us_latest
                && presentation_time_us >= previous_presentation_time_us
            {
                let interval = presentation_time_us - previous_presentation_time_us;
                self.gtk_frame_timings_presentation_interval_us_latest = Some(interval);
                update_max_u64(
                    &mut self.gtk_frame_timings_presentation_interval_us_max,
                    Some(interval),
                );
            }
            self.gtk_frame_timings_presentation_time_us_latest = Some(presentation_time_us);
        }
    }
}

fn non_negative_u64(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

fn update_max_u64(slot: &mut Option<u64>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    *slot = Some(slot.map_or(value, |current| current.max(value)));
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoDecoderReport {
    pub element: String,
    pub class: VideoDecoderClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoDecoderPolicyStatus {
    NotApplicable,
    NotObserved,
    Satisfied,
    SoftwareFallback,
    Violated,
    UnknownDecoder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoCapsReport {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub caps: String,
    pub memory_features: Vec<String>,
    pub structures: Vec<VideoCapsStructureReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoCapsStructureReport {
    pub media_type: String,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoAllocationReport {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub query_scope: String,
    pub caps: String,
    pub need_pool: bool,
    pub pools: Vec<VideoAllocationPoolReport>,
    pub params: Vec<VideoAllocationParamReport>,
    pub metas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoAllocationPoolReport {
    pub pool: String,
    pub size: u32,
    pub min_buffers: u32,
    pub max_buffers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoAllocationParamReport {
    pub allocator: String,
    pub flags: String,
    pub align: u64,
    pub prefix: u64,
    pub padding: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoZeroCopyEvidence {
    pub level: VideoZeroCopyEvidenceLevel,
    pub decoder_classes: Vec<VideoDecoderClass>,
    pub memory_features: Vec<String>,
    pub sink_memory_features: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoMemoryPathReport {
    pub level: VideoMemoryPathLevel,
    pub segments: Vec<VideoMemoryPathSegment>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoMemoryPathSegment {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub media_type: String,
    pub memory_features: Vec<String>,
    pub memory_class: VideoMemoryClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoMemoryRetentionReport {
    pub level: VideoMemoryRetentionLevel,
    pub estimated_min_pool_bytes: u64,
    pub estimated_max_pool_bytes: Option<u64>,
    pub pool_reports: usize,
    pub system_memory_pool_reports: usize,
    pub gpu_memory_pool_reports: usize,
    pub dmabuf_pool_reports: usize,
    pub other_memory_pool_reports: usize,
    pub sink_frame_retention: VideoSinkFrameRetention,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoMemoryPathLevel {
    Unknown,
    CpuRawCaps,
    SoftwareDecodeNoCaps,
    SoftwareDecodeCpuRaw,
    HardwareDecodeNoCaps,
    HardwareDecodeCpuRaw,
    DecoderGpuMemory,
    DecoderDmabuf,
    SinkGpuMemory,
    SinkDmabuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoMemoryClass {
    SystemMemory,
    GpuMemory,
    Dmabuf,
    OtherMemoryFeature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoMemoryRetentionLevel {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoSinkFrameRetention {
    Unknown,
    Disabled,
    LastSample,
    PrerollFrame,
    LastSampleAndPrerollFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoZeroCopyEvidenceLevel {
    Missing,
    SoftwareDecode,
    HardwareDecode,
    GpuMemoryCaps,
    DmabufCaps,
    SinkGpuMemoryCaps,
    SinkDmabufCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoDecoderClass {
    Hardware,
    Software,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoRendererError {
    Init(String),
    Uri(String),
    BuildElement(String),
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
        PlaylistClockDependency, StaticRenderAction, StaticRenderOutputDecision,
        StaticRenderSyncPlan, VideoWallpaperPlan,
    };
    use std::path::PathBuf;

    #[test]
    fn syncs_video_pipeline_snapshots() {
        let mut renderer = GstVideoRenderer::new_with_test_source().unwrap();
        let plan = video_plan("eDP-1", true, true);
        let sync = StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: vec![plan],
            slideshow_plans: Vec::new(),
            scene_lite_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: vec![decision("eDP-1", RenderMode::Throttled)],
            playlist_clock_dependency: PlaylistClockDependency::None,
            cache: Default::default(),
        };

        renderer.sync_render_plan(&sync).unwrap();
        let snapshot = renderer.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].output_name, "eDP-1");
        assert_eq!(snapshot[0].mode, RenderMode::Throttled);
        assert!(snapshot[0].loop_playback);
        assert!(snapshot[0].muted);
        assert_eq!(snapshot[0].target_max_fps, Some(24));
        assert!(!snapshot[0].frame_limiter_enabled);
        assert_eq!(snapshot[0].frame_limiter_max_fps, None);
        assert_eq!(
            snapshot[0].decoder_policy,
            crate::config::VideoDecoderPolicy::Auto
        );
        assert_eq!(
            snapshot[0].decoder_policy_status,
            VideoDecoderPolicyStatus::NotApplicable
        );
        assert_eq!(snapshot[0].start_offset_ms, 0);
        assert!(snapshot[0].actual_decoders.is_empty());
        assert!(snapshot[0].actual_decoder_reports.is_empty());

        let sync = StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: Vec::new(),
            slideshow_plans: Vec::new(),
            scene_lite_plans: Vec::new(),
            removals: vec!["eDP-1".to_owned()],
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: PlaylistClockDependency::None,
            cache: Default::default(),
        };
        renderer.sync_render_plan(&sync).unwrap();
        assert!(renderer.snapshot().is_empty());
    }

    #[test]
    fn builds_and_updates_sink_frame_limiter() {
        gst::init().unwrap();
        let mut plan = video_plan("eDP-1", true, true);
        let mut pipeline = VideoPipeline::new(&plan, false).unwrap();
        let limiter = pipeline.frame_limiter.as_ref().unwrap();
        assert_eq!(limiter.target_max_fps(), Some(24));
        assert_eq!(limiter.throttle_time_ns(), 41_666_666);
        assert_eq!(
            pipeline.sink_tuning.sink_element.as_deref(),
            Some("fakesink")
        );
        assert_eq!(pipeline.sink_tuning.async_enabled, Some(false));
        assert_eq!(pipeline.sink_tuning.last_sample_enabled, Some(false));
        assert_eq!(pipeline.sink_tuning.qos_enabled, Some(true));
        assert_eq!(pipeline.sink_tuning.max_lateness_ns, Some(41_666_666));
        assert_eq!(pipeline.sink_tuning.render_delay_ns, Some(0));
        if pipeline.sink_tuning.processing_deadline_ns.is_some() {
            assert_eq!(pipeline.sink_tuning.processing_deadline_ns, Some(0));
        }

        plan.target_max_fps = Some(12);
        pipeline.apply_plan(&plan).unwrap();
        assert_eq!(pipeline.target_max_fps, Some(12));
        let limiter = pipeline.frame_limiter.as_ref().unwrap();
        assert_eq!(limiter.target_max_fps(), Some(12));
        assert_eq!(limiter.throttle_time_ns(), 83_333_333);
        assert_eq!(pipeline.sink_tuning.max_lateness_ns, Some(50_000_000));

        plan.target_max_fps = None;
        pipeline.apply_plan(&plan).unwrap();
        assert_eq!(pipeline.target_max_fps, None);
        let limiter = pipeline.frame_limiter.as_ref().unwrap();
        assert_eq!(limiter.target_max_fps(), None);
        assert_eq!(limiter.throttle_time_ns(), 0);
        assert_eq!(pipeline.sink_tuning.max_lateness_ns, Some(50_000_000));

        plan.target_max_fps = Some(0);
        pipeline.apply_plan(&plan).unwrap();
        assert_eq!(pipeline.target_max_fps, Some(0));
        let limiter = pipeline.frame_limiter.as_ref().unwrap();
        assert_eq!(limiter.target_max_fps(), None);
        assert_eq!(limiter.throttle_time_ns(), 0);
        assert_eq!(pipeline.sink_tuning.max_lateness_ns, Some(50_000_000));
    }

    #[test]
    fn omits_frame_limiter_without_target_fps() {
        gst::init().unwrap();
        let mut plan = video_plan("eDP-1", true, true);
        plan.target_max_fps = None;
        let pipeline = VideoPipeline::new(&plan, false).unwrap();
        assert!(pipeline.frame_limiter.is_none());
        assert_eq!(
            pipeline.sink_tuning.sink_element.as_deref(),
            Some("fakesink")
        );
        assert_eq!(pipeline.sink_tuning.async_enabled, Some(false));
        assert_eq!(pipeline.sink_tuning.last_sample_enabled, Some(false));
        assert_eq!(pipeline.sink_tuning.qos_enabled, Some(true));
        assert_eq!(pipeline.sink_tuning.max_lateness_ns, Some(50_000_000));
        assert_eq!(pipeline.sink_tuning.render_delay_ns, Some(0));
        assert_eq!(
            pipeline
                .frame_limiter
                .as_ref()
                .and_then(FrameLimiter::target_max_fps),
            None
        );
    }

    #[test]
    fn bounds_video_sink_max_lateness_from_target_fps() {
        assert_eq!(video_sink_max_lateness_ns(None), 50_000_000);
        assert_eq!(video_sink_max_lateness_ns(Some(0)), 50_000_000);
        assert_eq!(video_sink_max_lateness_ns(Some(24)), 41_666_666);
        assert_eq!(video_sink_max_lateness_ns(Some(12)), 50_000_000);
        assert_eq!(video_sink_max_lateness_ns(Some(240)), 8_000_000);
    }

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

    #[test]
    fn maps_render_modes_to_gstreamer_states() {
        assert_eq!(gst_state_for_mode(RenderMode::Active), gst::State::Playing);
        assert_eq!(
            gst_state_for_mode(RenderMode::Throttled),
            gst::State::Playing
        );
        assert_eq!(gst_state_for_mode(RenderMode::Paused), gst::State::Paused);
    }

    #[test]
    fn classifies_known_decoder_elements() {
        assert_eq!(decoder_class("dav1ddec"), VideoDecoderClass::Software);
        assert_eq!(decoder_class("vaav1dec"), VideoDecoderClass::Hardware);
        assert_eq!(decoder_class("customdec"), VideoDecoderClass::Unknown);
    }

    #[test]
    fn merges_transient_observed_decoder_reports() {
        let current = Vec::new();
        let observed = vec![
            VideoDecoderReport {
                element: "nvh264dec".to_owned(),
                class: VideoDecoderClass::Hardware,
            },
            VideoDecoderReport {
                element: "nvh264dec".to_owned(),
                class: VideoDecoderClass::Hardware,
            },
        ];

        let reports = merge_decoder_reports(current, observed);

        assert_eq!(
            reports,
            vec![VideoDecoderReport {
                element: "nvh264dec".to_owned(),
                class: VideoDecoderClass::Hardware,
            },]
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwareRequired, &reports),
            VideoDecoderPolicyStatus::Satisfied
        );
    }

    #[test]
    fn decoder_policy_rank_rules_select_expected_decoder_class() {
        assert_eq!(
            decoder_policy_rank(VideoDecoderPolicy::Auto, "dav1ddec", gst::Rank::MARGINAL),
            gst::Rank::MARGINAL
        );
        assert_eq!(
            decoder_policy_rank(
                VideoDecoderPolicy::HardwarePreferred,
                "vaav1dec",
                gst::Rank::NONE
            ),
            gst::Rank::PRIMARY + DECODER_RANK_BOOST
        );
        assert_eq!(
            decoder_policy_rank(
                VideoDecoderPolicy::HardwarePreferred,
                "dav1ddec",
                gst::Rank::NONE
            ),
            gst::Rank::SECONDARY
        );
        assert_eq!(
            decoder_policy_rank(
                VideoDecoderPolicy::HardwareRequired,
                "dav1ddec",
                gst::Rank::PRIMARY
            ),
            gst::Rank::NONE
        );
        assert_eq!(
            decoder_policy_rank(VideoDecoderPolicy::Software, "vaav1dec", gst::Rank::PRIMARY),
            gst::Rank::NONE
        );
        assert_eq!(
            decoder_policy_rank(VideoDecoderPolicy::Software, "dav1ddec", gst::Rank::NONE),
            gst::Rank::PRIMARY + DECODER_RANK_BOOST
        );
    }

    #[test]
    fn reports_decoder_policy_status_from_observed_decoders() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let software = VideoDecoderReport {
            element: "dav1ddec".to_owned(),
            class: VideoDecoderClass::Software,
        };
        let unknown = VideoDecoderReport {
            element: "customdec".to_owned(),
            class: VideoDecoderClass::Unknown,
        };

        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::Auto, &[]),
            VideoDecoderPolicyStatus::NotApplicable
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwareRequired, &[]),
            VideoDecoderPolicyStatus::NotObserved
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwareRequired, &[hardware.clone()]),
            VideoDecoderPolicyStatus::Satisfied
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwareRequired, &[software.clone()]),
            VideoDecoderPolicyStatus::Violated
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwarePreferred, &[software.clone()]),
            VideoDecoderPolicyStatus::SoftwareFallback
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::Software, &[hardware.clone()]),
            VideoDecoderPolicyStatus::Violated
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::Software, &[software]),
            VideoDecoderPolicyStatus::Satisfied
        );
        assert_eq!(
            decoder_policy_status(VideoDecoderPolicy::HardwarePreferred, &[unknown]),
            VideoDecoderPolicyStatus::UnknownDecoder
        );
    }

    #[test]
    fn classifies_zero_copy_evidence_from_decoder_and_caps() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let software = VideoDecoderReport {
            element: "dav1ddec".to_owned(),
            class: VideoDecoderClass::Software,
        };

        assert_eq!(
            zero_copy_evidence(&[], &[]).level,
            VideoZeroCopyEvidenceLevel::Missing
        );
        assert_eq!(
            zero_copy_evidence(std::slice::from_ref(&software), &[]).level,
            VideoZeroCopyEvidenceLevel::SoftwareDecode
        );
        assert_eq!(
            zero_copy_evidence(std::slice::from_ref(&hardware), &[]).level,
            VideoZeroCopyEvidenceLevel::HardwareDecode
        );
        assert_eq!(
            zero_copy_evidence(
                std::slice::from_ref(&software),
                &[caps_report("decoder", "src", "src", "memory:GLMemory")]
            )
            .level,
            VideoZeroCopyEvidenceLevel::GpuMemoryCaps
        );
        assert_eq!(
            zero_copy_evidence(
                std::slice::from_ref(&hardware),
                &[caps_report("decoder", "src", "src", "memory:DMABuf")]
            )
            .level,
            VideoZeroCopyEvidenceLevel::DmabufCaps
        );
        assert_eq!(
            zero_copy_evidence(
                std::slice::from_ref(&software),
                &[caps_report(
                    "gtk4paintablesink0",
                    "sink",
                    "sink",
                    "memory:GLMemory"
                )]
            )
            .level,
            VideoZeroCopyEvidenceLevel::SinkGpuMemoryCaps
        );
        assert_eq!(
            zero_copy_evidence(
                &[hardware],
                &[caps_report(
                    "gtk4paintablesink0",
                    "sink",
                    "sink",
                    "memory:DMABuf"
                )]
            )
            .level,
            VideoZeroCopyEvidenceLevel::SinkDmabufCaps
        );
    }

    #[test]
    fn classifies_video_memory_path_from_decoder_and_caps() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let software = VideoDecoderReport {
            element: "dav1ddec".to_owned(),
            class: VideoDecoderClass::Software,
        };
        let cpu_raw = caps_report("videoconvert0", "src", "src", "");

        assert_eq!(
            video_memory_path(&[], &[]).level,
            VideoMemoryPathLevel::Unknown
        );
        assert_eq!(
            video_memory_path(std::slice::from_ref(&software), &[]).level,
            VideoMemoryPathLevel::SoftwareDecodeNoCaps
        );
        assert_eq!(
            video_memory_path(&[], std::slice::from_ref(&cpu_raw)).level,
            VideoMemoryPathLevel::CpuRawCaps
        );
        assert_eq!(
            video_memory_path(
                std::slice::from_ref(&software),
                std::slice::from_ref(&cpu_raw)
            )
            .level,
            VideoMemoryPathLevel::SoftwareDecodeCpuRaw
        );
        assert_eq!(
            video_memory_path(
                std::slice::from_ref(&hardware),
                std::slice::from_ref(&cpu_raw)
            )
            .level,
            VideoMemoryPathLevel::HardwareDecodeCpuRaw
        );
        assert_eq!(
            video_memory_path(
                std::slice::from_ref(&hardware),
                &[caps_report("vaav1dec0", "src", "src", "memory:GLMemory")]
            )
            .level,
            VideoMemoryPathLevel::DecoderGpuMemory
        );
        assert_eq!(
            video_memory_path(
                std::slice::from_ref(&hardware),
                &[caps_report("vaav1dec0", "src", "src", "memory:DMABuf")]
            )
            .level,
            VideoMemoryPathLevel::DecoderDmabuf
        );
        assert_eq!(
            video_memory_path(
                std::slice::from_ref(&software),
                &[caps_report(
                    "gtk4paintablesink0",
                    "sink",
                    "sink",
                    "memory:GLMemory"
                )]
            )
            .level,
            VideoMemoryPathLevel::SinkGpuMemory
        );
        assert_eq!(
            video_memory_path(
                &[hardware],
                &[caps_report(
                    "gtk4paintablesink0",
                    "sink",
                    "sink",
                    "memory:DMABuf"
                )]
            )
            .level,
            VideoMemoryPathLevel::SinkDmabuf
        );
    }

    #[test]
    fn reports_low_video_memory_retention_for_sink_dmabuf_without_frame_retention() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let memory_path = video_memory_path(
            &[hardware],
            &[caps_report(
                "gtk4paintablesink0",
                "sink",
                "sink",
                "memory:DMABuf",
            )],
        );
        let sink_tuning = VideoSinkTuningReport {
            last_sample_enabled: Some(false),
            preroll_frame_enabled: Some(false),
            ..VideoSinkTuningReport::default()
        };

        let report = video_memory_retention_report(&memory_path, &[], &sink_tuning);

        assert_eq!(report.level, VideoMemoryRetentionLevel::Low);
        assert_eq!(
            report.sink_frame_retention,
            VideoSinkFrameRetention::Disabled
        );
        assert_eq!(report.estimated_min_pool_bytes, 0);
        assert_eq!(report.estimated_max_pool_bytes, Some(0));

        let partial_sink_tuning = VideoSinkTuningReport {
            last_sample_enabled: Some(false),
            ..VideoSinkTuningReport::default()
        };
        let report = video_memory_retention_report(&memory_path, &[], &partial_sink_tuning);
        assert_eq!(
            report.sink_frame_retention,
            VideoSinkFrameRetention::Disabled
        );
    }

    #[test]
    fn reports_high_video_memory_retention_for_cpu_raw_pools_or_retained_sink_frame() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let memory_path = video_memory_path(
            &[hardware],
            &[caps_report("videoconvert0", "src", "src", "")],
        );
        let allocation = allocation_report(
            "videoconvert0",
            "src",
            "video/x-raw",
            "GstVideoBufferPool",
            4096,
            2,
            4,
            "systemmemoryallocator0",
        );
        let sink_tuning = VideoSinkTuningReport {
            last_sample_enabled: Some(true),
            preroll_frame_enabled: Some(false),
            ..VideoSinkTuningReport::default()
        };

        let report = video_memory_retention_report(&memory_path, &[allocation], &sink_tuning);

        assert_eq!(report.level, VideoMemoryRetentionLevel::High);
        assert_eq!(
            report.sink_frame_retention,
            VideoSinkFrameRetention::LastSample
        );
        assert_eq!(report.estimated_min_pool_bytes, 8192);
        assert_eq!(report.estimated_max_pool_bytes, Some(16384));
        assert_eq!(report.system_memory_pool_reports, 1);
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("system-memory raw video caps"))
        );
    }

    #[test]
    fn reports_medium_video_memory_retention_for_decoder_only_gpu_path() {
        let hardware = VideoDecoderReport {
            element: "vaav1dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let memory_path = video_memory_path(
            &[hardware],
            &[caps_report("vaav1dec0", "src", "src", "memory:GLMemory")],
        );
        let allocation = allocation_report(
            "vaav1dec0",
            "src",
            "video/x-raw(memory:GLMemory)",
            "GstGLBufferPool",
            4096,
            2,
            0,
            "glmemoryallocator0",
        );
        let sink_tuning = VideoSinkTuningReport {
            last_sample_enabled: Some(false),
            preroll_frame_enabled: Some(false),
            ..VideoSinkTuningReport::default()
        };

        let report = video_memory_retention_report(&memory_path, &[allocation], &sink_tuning);

        assert_eq!(report.level, VideoMemoryRetentionLevel::Medium);
        assert_eq!(report.estimated_min_pool_bytes, 8192);
        assert_eq!(report.estimated_max_pool_bytes, None);
        assert_eq!(report.gpu_memory_pool_reports, 1);
    }

    #[test]
    fn classifies_retention_pools_from_gl_and_dmabuf_pool_names() {
        let hardware = VideoDecoderReport {
            element: "nvh264dec".to_owned(),
            class: VideoDecoderClass::Hardware,
        };
        let memory_path = video_memory_path(
            &[hardware],
            &[caps_report(
                "gtk4paintablesink0",
                "sink",
                "sink",
                "memory:GLMemory",
            )],
        );
        let allocation = VideoAllocationReport {
            element: "nvh264dec0".to_owned(),
            pad: "src".to_owned(),
            direction: "src".to_owned(),
            query_scope: "peer".to_owned(),
            caps: "video/x-raw, format=(string)NV12".to_owned(),
            need_pool: true,
            pools: vec![
                VideoAllocationPoolReport {
                    pool: "videodmabufpool12".to_owned(),
                    size: 13_824,
                    min_buffers: 1,
                    max_buffers: 0,
                },
                VideoAllocationPoolReport {
                    pool: "glbufferpool13".to_owned(),
                    size: 13_824,
                    min_buffers: 1,
                    max_buffers: 0,
                },
            ],
            params: vec![VideoAllocationParamReport {
                allocator: "none".to_owned(),
                flags: "MemoryFlags(0x0)".to_owned(),
                align: 0,
                prefix: 0,
                padding: 0,
            }],
            metas: Vec::new(),
        };
        let sink_tuning = VideoSinkTuningReport {
            last_sample_enabled: Some(false),
            preroll_frame_enabled: Some(false),
            ..VideoSinkTuningReport::default()
        };

        let report = video_memory_retention_report(&memory_path, &[allocation], &sink_tuning);

        assert_eq!(report.level, VideoMemoryRetentionLevel::Medium);
        assert_eq!(report.pool_reports, 2);
        assert_eq!(report.system_memory_pool_reports, 0);
        assert_eq!(report.dmabuf_pool_reports, 1);
        assert_eq!(report.gpu_memory_pool_reports, 1);
        assert_eq!(report.estimated_min_pool_bytes, 27_648);
        assert_eq!(report.estimated_max_pool_bytes, None);
    }

    fn caps_report(
        element: &str,
        pad: &str,
        direction: &str,
        memory_feature: &str,
    ) -> VideoCapsReport {
        let memory_features = if memory_feature.is_empty() {
            Vec::new()
        } else {
            vec![memory_feature.to_owned()]
        };
        let caps = if memory_feature.is_empty() {
            "video/x-raw".to_owned()
        } else {
            format!("video/x-raw({memory_feature})")
        };
        VideoCapsReport {
            element: element.to_owned(),
            pad: pad.to_owned(),
            direction: direction.to_owned(),
            caps,
            memory_features: memory_features.clone(),
            structures: vec![VideoCapsStructureReport {
                media_type: "video/x-raw".to_owned(),
                features: memory_features,
            }],
        }
    }

    fn allocation_report(
        element: &str,
        pad: &str,
        caps: &str,
        pool: &str,
        size: u32,
        min_buffers: u32,
        max_buffers: u32,
        allocator: &str,
    ) -> VideoAllocationReport {
        VideoAllocationReport {
            element: element.to_owned(),
            pad: pad.to_owned(),
            direction: "src".to_owned(),
            query_scope: "peer".to_owned(),
            caps: caps.to_owned(),
            need_pool: true,
            pools: vec![VideoAllocationPoolReport {
                pool: pool.to_owned(),
                size,
                min_buffers,
                max_buffers,
            }],
            params: vec![VideoAllocationParamReport {
                allocator: allocator.to_owned(),
                flags: "MemoryFlags(0x0)".to_owned(),
                align: 0,
                prefix: 0,
                padding: 0,
            }],
            metas: Vec::new(),
        }
    }

    #[test]
    fn accumulates_qos_frame_stats() {
        let mut stats = VideoFrameStats::default();

        stats.record_qos_values("buffers".to_owned(), 10, 2, -7_000, 0.995);
        stats.record_qos_values("buffers".to_owned(), 15, 1, 2_000, 1.25);

        assert_eq!(stats.qos_messages, 2);
        assert_eq!(stats.qos_stats_format.as_deref(), Some("buffers"));
        assert_eq!(stats.qos_processed_max, Some(15));
        assert_eq!(stats.qos_dropped_max, Some(2));
        assert_eq!(stats.qos_jitter_ns_latest, Some(2_000));
        assert_eq!(stats.qos_jitter_ns_abs_max, Some(7_000));
        assert_eq!(stats.qos_proportion_x1000_latest, Some(1_250));
    }

    #[test]
    fn accumulates_gtk_frame_clock_stats() {
        let mut stats = VideoFrameStats::default();

        stats.record_gtk_frame_clock_tick(10, 1_000, 59.94, 16_667, 17_667);
        stats.record_gtk_frame_clock_phase(GtkFrameClockPhase::BeforePaint);
        stats.record_gtk_frame_clock_phase(GtkFrameClockPhase::Update);
        stats.record_gtk_frame_clock_phase(GtkFrameClockPhase::Layout);
        stats.record_gtk_frame_clock_phase(GtkFrameClockPhase::Paint);
        stats.record_gtk_frame_clock_tick(11, 17_700, 60.0, 16_667, 34_367);
        stats.record_gtk_frame_clock_tick(12, 34_300, 60.0, 16_667, 50_967);

        assert_eq!(stats.gtk_frame_clock_ticks, 3);
        assert_eq!(stats.gtk_frame_clock_before_paint_ticks, 1);
        assert_eq!(stats.gtk_frame_clock_update_ticks, 1);
        assert_eq!(stats.gtk_frame_clock_layout_ticks, 1);
        assert_eq!(stats.gtk_frame_clock_paint_ticks, 1);
        assert_eq!(stats.gtk_frame_clock_after_paint_ticks, 3);
        assert_eq!(stats.gtk_frame_clock_counter_latest, Some(12));
        assert_eq!(stats.gtk_frame_clock_time_us_latest, Some(34_300));
        assert_eq!(stats.gtk_frame_clock_interval_us_latest, Some(16_600));
        assert_eq!(stats.gtk_frame_clock_interval_us_max, Some(16_700));
        assert_eq!(stats.gtk_frame_clock_fps_x1000_latest, Some(60_000));
        assert_eq!(
            stats.gtk_frame_clock_refresh_interval_us_latest,
            Some(16_667)
        );
        assert_eq!(
            stats.gtk_frame_clock_predicted_presentation_time_us_latest,
            Some(50_967)
        );
    }

    #[test]
    fn accumulates_minimal_gtk_frame_clock_stats_without_expensive_fields() {
        let mut stats = VideoFrameStats::default();

        stats.record_gtk_frame_clock_tick_minimal(10, 1_000);
        stats.record_gtk_frame_clock_tick_minimal(11, 17_700);

        assert_eq!(stats.gtk_frame_clock_ticks, 2);
        assert_eq!(stats.gtk_frame_clock_after_paint_ticks, 2);
        assert_eq!(stats.gtk_frame_clock_counter_latest, Some(11));
        assert_eq!(stats.gtk_frame_clock_time_us_latest, Some(17_700));
        assert_eq!(stats.gtk_frame_clock_interval_us_latest, Some(16_700));
        assert_eq!(stats.gtk_frame_clock_interval_us_max, Some(16_700));
        assert_eq!(stats.gtk_frame_clock_fps_x1000_latest, None);
        assert_eq!(stats.gtk_frame_clock_refresh_interval_us_latest, None);
        assert_eq!(
            stats.gtk_frame_clock_predicted_presentation_time_us_latest,
            None
        );
    }

    #[test]
    fn accumulates_gtk_frame_timing_stats() {
        let mut stats = VideoFrameStats::default();

        stats.record_gtk_frame_timing(10, false, 1_000, 17_667, -1, 16_667);
        stats.record_gtk_frame_timing(10, true, 1_000, 17_667, 17_700, 16_667);
        stats.record_gtk_frame_timing(10, true, 1_000, 17_667, 17_700, 16_667);
        stats.record_gtk_frame_timing(11, true, 17_700, 34_367, 34_400, 16_667);

        assert_eq!(stats.gtk_frame_timings_observed, 2);
        assert_eq!(stats.gtk_frame_timings_complete, 2);
        assert_eq!(stats.gtk_frame_timings_counter_latest, Some(11));
        assert_eq!(stats.gtk_frame_timings_complete_counter_latest, Some(11));
        assert_eq!(stats.gtk_frame_timings_frame_time_us_latest, Some(17_700));
        assert_eq!(
            stats.gtk_frame_timings_predicted_presentation_time_us_latest,
            Some(34_367)
        );
        assert_eq!(
            stats.gtk_frame_timings_presentation_time_us_latest,
            Some(34_400)
        );
        assert_eq!(
            stats.gtk_frame_timings_presentation_interval_us_latest,
            Some(16_700)
        );
        assert_eq!(
            stats.gtk_frame_timings_presentation_interval_us_max,
            Some(16_700)
        );
        assert_eq!(
            stats.gtk_frame_timings_refresh_interval_us_latest,
            Some(16_667)
        );
    }

    #[test]
    fn reports_current_video_caps_memory_features() {
        gst::init().unwrap();
        let source = gst::ElementFactory::make("videotestsrc").build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").build().unwrap();
        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&source, &sink]).unwrap();
        source.link(&sink).unwrap();
        pipeline.set_state(gst::State::Paused).unwrap();
        let _ = pipeline.state(gst::ClockTime::from_seconds(3));

        let reports = video_caps_reports(&pipeline.clone().upcast::<gst::Element>());
        pipeline.set_state(gst::State::Null).unwrap();

        assert!(reports.iter().any(|report| {
            report
                .structures
                .iter()
                .any(|structure| structure.media_type.starts_with("video/"))
        }));
    }

    #[test]
    fn caches_video_pipeline_diagnostics_until_invalidated() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let element = pipeline.clone().upcast::<gst::Element>();
        let cache = VideoPipelineDiagnosticsCache::with_refresh_interval(Duration::from_secs(60));

        let first = cache.snapshot(&element);
        let first_sampled_at = cache.state.borrow().as_ref().unwrap().sampled_at;
        let second = cache.snapshot(&element);
        let second_sampled_at = cache.state.borrow().as_ref().unwrap().sampled_at;

        assert_eq!(first, second);
        assert_eq!(first_sampled_at, second_sampled_at);

        cache.invalidate();
        assert!(cache.state.borrow().is_none());
        assert_eq!(cache.snapshot(&element), first);
        assert!(cache.state.borrow().is_some());
    }

    #[test]
    fn extracts_caps_memory_features() {
        gst::init().unwrap();
        let caps = gst::Caps::builder_full_with_features(gst::CapsFeatures::new(["memory:DMABuf"]))
            .structure(gst::Structure::builder("video/x-raw").build())
            .build();
        let structures = caps_structure_reports(&caps);

        assert_eq!(caps_memory_features(&structures), vec!["memory:DMABuf"]);
        assert!(caps_report_is_relevant(&structures));
    }

    #[test]
    fn runtime_capabilities_report_expected_elements() {
        let capabilities = runtime_capabilities();
        let element_names = capabilities
            .elements
            .iter()
            .map(|element| element.name.as_str())
            .collect::<Vec<_>>();

        for expected in ["playbin", "fakesink", "gtk4paintablesink", "glsinkbin"] {
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
            decoder_policy: crate::config::VideoDecoderPolicy::Auto,
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
