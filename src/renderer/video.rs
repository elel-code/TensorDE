//! GStreamer video pipeline controller.

use super::{StaticRenderSyncPlan, VideoWallpaperPlan};
use crate::config::VideoDecoderPolicy;
use crate::policy::RenderMode;
use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Mutex, OnceLock};

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
const MUTED_PLAYBIN_FLAGS: &str = "video+deinterlace+soft-colorbalance";
const AUDIBLE_PLAYBIN_FLAGS: &str = "video+audio+soft-volume+deinterlace+soft-colorbalance";
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
                let actual_decoder_reports = actual_decoder_reports(&pipeline.element);
                let caps_reports = video_caps_reports(&pipeline.element);
                let zero_copy_evidence = zero_copy_evidence(&actual_decoder_reports, &caps_reports);
                VideoPipelineSnapshot {
                    output_name: output_name.clone(),
                    source: pipeline.source.display().to_string(),
                    mode: pipeline.mode,
                    gst_state: pipeline.gst_state.name().to_string(),
                    loop_playback: pipeline.loop_playback,
                    muted: pipeline.muted,
                    target_max_fps: pipeline.target_max_fps,
                    frame_limiter_enabled: pipeline.frame_limiter.is_some(),
                    frame_limiter_max_fps: pipeline
                        .frame_limiter
                        .as_ref()
                        .and_then(FrameLimiter::target_max_fps),
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
                    zero_copy_evidence,
                }
            })
            .collect()
    }
}

struct VideoPipeline {
    element: gst::Element,
    frame_limiter: Option<FrameLimiter>,
    source: std::path::PathBuf,
    mode: RenderMode,
    gst_state: gst::State,
    loop_playback: bool,
    muted: bool,
    target_max_fps: Option<u32>,
    decoder_policy: VideoDecoderPolicy,
    start_offset_ms: u64,
    frame_stats: VideoFrameStats,
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
            gst_state: gst::State::Null,
            loop_playback: plan.loop_playback,
            muted: !plan.muted,
            target_max_fps: plan.target_max_fps,
            decoder_policy: plan.decoder_policy,
            start_offset_ms: 0,
            frame_stats: VideoFrameStats::default(),
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
        if let Some(frame_limiter) = &self.frame_limiter {
            frame_limiter.apply_target_max_fps(target_max_fps);
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
    apply_decoder_rank_policy(plan.decoder_policy);
    let frame_limiter = Some(FrameLimiter::new(plan.target_max_fps)?);
    let video_sink = gst::ElementFactory::make("fakesink")
        .property("sync", true)
        .build()
        .map_err(|err| VideoRendererError::BuildElement(err.to_string()))?;
    let mut builder = gst::ElementFactory::make("playbin")
        .property("uri", uri.as_str())
        .property_from_str("flags", playbin_flags(plan.muted))
        .property("video-sink", &video_sink);
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

fn playbin_flags(muted: bool) -> &'static str {
    if muted {
        MUTED_PLAYBIN_FLAGS
    } else {
        AUDIBLE_PLAYBIN_FLAGS
    }
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

pub(crate) fn target_max_fps_from_caps(caps: &gst::Caps) -> Option<u32> {
    let structure = caps.structure(0)?;
    let framerate = structure.get::<gst::Fraction>("framerate").ok()?;
    u32::try_from(framerate.numer()).ok()
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
    pub zero_copy_evidence: VideoZeroCopyEvidence,
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
    pub(crate) fn record_gtk_frame_clock_tick(
        &mut self,
        frame_counter: i64,
        frame_time_us: i64,
        fps: f64,
        refresh_interval_us: i64,
        predicted_presentation_time_us: i64,
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
pub struct VideoZeroCopyEvidence {
    pub level: VideoZeroCopyEvidenceLevel,
    pub decoder_classes: Vec<VideoDecoderClass>,
    pub memory_features: Vec<String>,
    pub sink_memory_features: Vec<String>,
    pub notes: Vec<String>,
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
            slideshow_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: vec![decision("eDP-1", RenderMode::Throttled)],
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
            removals: vec!["eDP-1".to_owned()],
            errors: Vec::new(),
            decisions: Vec::new(),
            cache: Default::default(),
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
    fn muted_video_playbin_flags_disable_audio_streams() {
        assert_eq!(playbin_flags(true), MUTED_PLAYBIN_FLAGS);
        assert!(!playbin_flags(true).contains("audio"));
        assert!(playbin_flags(false).contains("audio"));
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

    fn caps_report(
        element: &str,
        pad: &str,
        direction: &str,
        memory_feature: &str,
    ) -> VideoCapsReport {
        VideoCapsReport {
            element: element.to_owned(),
            pad: pad.to_owned(),
            direction: direction.to_owned(),
            caps: format!("video/x-raw({memory_feature})"),
            memory_features: vec![memory_feature.to_owned()],
            structures: vec![VideoCapsStructureReport {
                media_type: "video/x-raw".to_owned(),
                features: vec![memory_feature.to_owned()],
            }],
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
