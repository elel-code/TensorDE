use std::path::PathBuf;

use crate::config::VideoDecoderPolicy;
use crate::renderer::video::{
    actual_decoder_reports, apply_decoder_rank_policy, decoder_policy_status, video_caps_reports,
};
use gst::prelude::*;
use gstreamer as gst;

use super::video_frontend::{
    NativeVulkanVideoCapsSnapshot, NativeVulkanVideoDecodeOwner,
    NativeVulkanVideoFrontendMemoryPreference, NativeVulkanVideoFrontendProvider,
    NativeVulkanVideoFrontendRoute, NativeVulkanVideoFrontendSnapshot,
};
use super::video_memory_gst::native_vulkan_gst_memory_types;
use super::{NativeVulkanError, NativeVulkanRenderItem, native_vulkan_clock_time_ms};

pub(super) struct NativeVulkanGstVideoFrontend {
    pipeline: gst::Element,
    sink: gst::Element,
    bus: gst::Bus,
    loop_playback: bool,
    loop_start_position_ms: u64,
    decoder_policy: VideoDecoderPolicy,
    eos_messages: u64,
    segment_done_messages: u64,
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_pts_ms: Option<u64>,
    last_sample_duration_ms: Option<u64>,
    last_sample_pts_delta_ms: Option<u64>,
    last_sample_memory_types: Vec<String>,
    latest_sample: Option<gst::Sample>,
    last_error: Option<String>,
}

impl NativeVulkanGstVideoFrontend {
    pub(super) fn new(item: &NativeVulkanRenderItem) -> Result<Self, NativeVulkanError> {
        let NativeVulkanRenderItem::Video {
            source,
            loop_playback,
            decoder_policy,
            start_offset_ms,
            ..
        } = item
        else {
            return Err(NativeVulkanError::Video(
                "GStreamer frontend requires a video render item".to_owned(),
            ));
        };

        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        apply_decoder_rank_policy(*decoder_policy);
        native_vulkan_apply_memory_path_decoder_policy();
        let pipeline = native_vulkan_gst_video_pipeline(source)?;
        let sink = pipeline
            .by_name("gilder-native-vulkan-video-appsink")
            .ok_or_else(|| NativeVulkanError::Video("video appsink not found".to_owned()))?;
        let bus = pipeline
            .bus()
            .ok_or_else(|| NativeVulkanError::Video("video pipeline has no bus".to_owned()))?;
        pipeline
            .set_state(gst::State::Paused)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));
        if *loop_playback {
            native_vulkan_gst_seek_loop_segment(pipeline.upcast_ref(), *start_offset_ms)?;
        } else if *start_offset_ms > 0 {
            native_vulkan_gst_seek_once(pipeline.upcast_ref(), *start_offset_ms)?;
        }
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

        Ok(Self {
            pipeline: pipeline.upcast::<gst::Element>(),
            sink,
            bus,
            loop_playback: *loop_playback,
            loop_start_position_ms: *start_offset_ms,
            decoder_policy: *decoder_policy,
            eos_messages: 0,
            segment_done_messages: 0,
            frames_received: 0,
            last_sample_caps: None,
            last_sample_format: None,
            last_sample_size: None,
            last_sample_pts_ms: None,
            last_sample_duration_ms: None,
            last_sample_pts_delta_ms: None,
            last_sample_memory_types: Vec::new(),
            latest_sample: None,
            last_error: None,
        })
    }

    pub(super) fn poll(&mut self) -> Result<(), NativeVulkanError> {
        self.poll_bus()?;
        self.pull_available_samples();
        Ok(())
    }

    fn poll_bus(&mut self) -> Result<(), NativeVulkanError> {
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    if let Some(position_ms) = native_vulkan_gst_video_loop_seek_position_ms(
                        self.loop_playback,
                        self.loop_start_position_ms,
                    ) {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, position_ms)?;
                    } else {
                        self.pipeline
                            .set_state(gst::State::Paused)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
                    }
                }
                gst::MessageView::SegmentDone(_) => {
                    self.segment_done_messages = self.segment_done_messages.saturating_add(1);
                    if let Some(position_ms) = native_vulkan_gst_video_loop_seek_position_ms(
                        self.loop_playback,
                        self.loop_start_position_ms,
                    ) {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, position_ms)?;
                    }
                }
                gst::MessageView::Error(err) => {
                    let mut message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
                    if let Some(debug) = err.debug() {
                        message.push_str(": ");
                        message.push_str(&debug);
                    }
                    self.last_error = Some(message.clone());
                    return Err(NativeVulkanError::Video(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn pull_available_samples(&mut self) {
        let sample = self
            .sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
        let Some(sample) = sample else {
            return;
        };
        self.record_sample(&sample);
        self.latest_sample = Some(sample);
    }

    fn record_sample(&mut self, sample: &gst::Sample) {
        self.frames_received = self.frames_received.saturating_add(1);
        self.last_sample_caps = sample.caps().map(|caps| caps.to_string());
        if let Some(caps) = sample.caps()
            && let Some(structure) = caps.structure(0)
        {
            self.last_sample_format = structure.get::<String>("format").ok();
            let width = structure.get::<i32>("width").ok();
            let height = structure.get::<i32>("height").ok();
            self.last_sample_size = width.zip(height).and_then(|(width, height)| {
                Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
            });
        }
        self.last_sample_memory_types = sample
            .buffer()
            .map(|buffer| {
                let pts_ms = native_vulkan_clock_time_ms(buffer.pts());
                self.last_sample_pts_delta_ms = self
                    .last_sample_pts_ms
                    .zip(pts_ms)
                    .and_then(|(previous, current)| current.checked_sub(previous));
                self.last_sample_pts_ms = pts_ms;
                self.last_sample_duration_ms = native_vulkan_clock_time_ms(buffer.duration());
                native_vulkan_gst_memory_types(buffer)
            })
            .unwrap_or_else(|| {
                self.last_sample_pts_ms = None;
                self.last_sample_duration_ms = None;
                self.last_sample_pts_delta_ms = None;
                Vec::new()
            });
        self.last_error = None;
    }

    pub(super) fn take_latest_sample(&mut self) -> Option<gst::Sample> {
        self.latest_sample.take()
    }

    pub(super) fn segment_done_messages(&self) -> u64 {
        self.segment_done_messages
    }

    pub(super) fn loop_start_position_ms(&self) -> u64 {
        self.loop_start_position_ms
    }

    pub(super) fn snapshot(&self) -> NativeVulkanVideoFrontendSnapshot {
        let provider_state = Some(
            self.pipeline
                .state(gst::ClockTime::ZERO)
                .1
                .name()
                .to_string(),
        );
        let decoder_reports = actual_decoder_reports(&self.pipeline);
        let actual_decoders = decoder_reports
            .iter()
            .map(|report| report.element.clone())
            .collect::<Vec<_>>();
        let decoder_policy_status = Some(format!(
            "{:?}",
            decoder_policy_status(self.decoder_policy, &decoder_reports)
        ));
        let caps_reports = video_caps_reports(&self.pipeline);
        let mut caps_memory_features = caps_reports
            .iter()
            .flat_map(|report| report.memory_features.iter().cloned())
            .collect::<Vec<_>>();
        caps_memory_features.sort();
        caps_memory_features.dedup();
        let caps_report_count = caps_reports.len();
        let caps_reports = caps_reports
            .into_iter()
            .map(|report| NativeVulkanVideoCapsSnapshot {
                element: report.element,
                pad: report.pad,
                direction: report.direction,
                caps: report.caps,
                source: report.source,
                memory_features: report.memory_features,
            })
            .collect();

        NativeVulkanVideoFrontendSnapshot {
            provider: NativeVulkanVideoFrontendProvider::Gstreamer,
            route: NativeVulkanVideoFrontendRoute::DecodedProvider,
            decode_owner: NativeVulkanVideoDecodeOwner::Gstreamer,
            memory_preference: native_vulkan_gst_memory_preference(),
            sample_queue_policy: native_vulkan_gst_sample_queue_policy().as_str(),
            provider_state,
            eos_messages: self.eos_messages,
            segment_done_messages: self.segment_done_messages,
            frames_received: self.frames_received,
            last_sample_caps: self.last_sample_caps.clone(),
            last_sample_format: self.last_sample_format.clone(),
            last_sample_size: self.last_sample_size,
            last_sample_pts_ms: self.last_sample_pts_ms,
            last_sample_duration_ms: self.last_sample_duration_ms,
            last_sample_pts_delta_ms: self.last_sample_pts_delta_ms,
            last_sample_memory_types: self.last_sample_memory_types.clone(),
            actual_decoders,
            decoder_policy_status,
            caps_report_count,
            caps_memory_features,
            caps_reports,
            last_error: self.last_error.clone(),
        }
    }
}

impl Drop for NativeVulkanGstVideoFrontend {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn native_vulkan_gst_video_loop_seek_position_ms(
    loop_playback: bool,
    loop_start_position_ms: u64,
) -> Option<u64> {
    loop_playback.then_some(loop_start_position_ms)
}

fn native_vulkan_gst_video_pipeline(source: &PathBuf) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let decodebin = native_vulkan_gst_element("decodebin")?;
    if let Ok(decodebin_bin) = decodebin.clone().dynamic_cast::<gst::Bin>() {
        decodebin_bin.connect_element_added(|_, element| {
            native_vulkan_configure_decoder_low_memory(element);
        });
    }
    let queue = native_vulkan_gst_element("queue")?;
    native_vulkan_configure_queue(&queue);
    let sink = native_vulkan_gst_element("appsink")?;
    sink.set_property("name", "gilder-native-vulkan-video-appsink");
    native_vulkan_configure_appsink(&sink);

    pipeline
        .add_many([&filesrc, &decodebin, &queue, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&decodebin)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    queue
        .link(&sink)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeVulkanError::Video("queue has no sink pad".to_owned()))?;
    decodebin.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let is_video = caps
            .structure(0)
            .map(|structure| structure.name().starts_with("video/"))
            .unwrap_or(false);
        if is_video {
            let _ = pad.link(&queue_sink);
        }
    });

    Ok(pipeline)
}

fn native_vulkan_gst_element(name: &str) -> Result<gst::Element, NativeVulkanError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeVulkanError::Video(format!("create {name}: {err}")))
}

fn native_vulkan_gst_seek_once(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_mseconds(start_offset_ms),
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

fn native_vulkan_gst_seek_loop_segment(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek(
            1.0,
            gst::SeekFlags::FLUSH | gst::SeekFlags::SEGMENT | gst::SeekFlags::KEY_UNIT,
            gst::SeekType::Set,
            gst::ClockTime::from_mseconds(start_offset_ms),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

fn native_vulkan_configure_decoder_low_memory(decoder: &gst::Element) {
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("max-display-delay").is_some() {
        decoder.set_property("max-display-delay", 0i32);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property(
            "num-output-surfaces",
            native_vulkan_gst_nvdec_output_surfaces(),
        );
    }
}

fn native_vulkan_configure_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_vulkan_gst_video_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
}

fn native_vulkan_gst_nvdec_output_surfaces() -> u32 {
    std::env::var("GILDER_VULKAN_GST_NVDEC_OUTPUT_SURFACES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .map(|value| value.clamp(1, 64))
        .unwrap_or(1)
}

fn native_vulkan_configure_appsink(sink: &gst::Element) {
    let sample_queue_policy = native_vulkan_gst_sample_queue_policy();
    if let Some(caps) = native_vulkan_gst_forced_sink_caps() {
        sink.set_property("caps", &caps);
    }
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", true);
    }
    if sink.find_property("async").is_some() {
        sink.set_property("async", false);
    }
    if sink.find_property("emit-signals").is_some() {
        sink.set_property("emit-signals", false);
    }
    if sink.find_property("enable-last-sample").is_some() {
        sink.set_property("enable-last-sample", false);
    }
    if sink.find_property("wait-on-eos").is_some() {
        sink.set_property("wait-on-eos", false);
    }
    if sink.find_property("max-buffers").is_some() {
        sink.set_property("max-buffers", native_vulkan_gst_video_queue_frames());
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", sample_queue_policy.drops_old_buffers());
    }
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    if sink.find_property("processing-deadline").is_some() {
        sink.set_property("processing-deadline", 0u64);
    }
    if sink.find_property("render-delay").is_some() {
        sink.set_property("render-delay", 0u64);
    }
}

fn native_vulkan_gst_video_queue_frames() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanGstSampleQueuePolicy {
    KeepLast,
    Backpressure,
}

impl NativeVulkanGstSampleQueuePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::KeepLast => "keep-last",
            Self::Backpressure => "backpressure",
        }
    }

    fn drops_old_buffers(self) -> bool {
        matches!(self, Self::KeepLast)
    }
}

fn native_vulkan_gst_sample_queue_policy() -> NativeVulkanGstSampleQueuePolicy {
    native_vulkan_gst_sample_queue_policy_from_value(
        std::env::var("GILDER_VULKAN_GST_SAMPLE_QUEUE_POLICY").ok(),
    )
}

fn native_vulkan_gst_sample_queue_policy_from_value(
    value: Option<String>,
) -> NativeVulkanGstSampleQueuePolicy {
    match value.as_deref() {
        Some("backpressure" | "bounded" | "blocking") => {
            NativeVulkanGstSampleQueuePolicy::Backpressure
        }
        _ => NativeVulkanGstSampleQueuePolicy::KeepLast,
    }
}

fn native_vulkan_gst_forced_sink_caps() -> Option<gst::Caps> {
    if !native_vulkan_gst_prefers_dmabuf() {
        return None;
    }
    Some(
        gst::Caps::builder_full()
            .structure_with_features(
                gst::Structure::builder("video/x-raw")
                    .field("format", "NV12")
                    .build(),
                gst::CapsFeatures::new(["memory:VAMemory"]),
            )
            .structure_with_features(
                gst::Structure::builder("video/x-raw")
                    .field("format", "P010_10LE")
                    .build(),
                gst::CapsFeatures::new(["memory:VAMemory"]),
            )
            .structure_with_features(
                gst::Structure::builder("video/x-raw")
                    .field("format", "DMA_DRM")
                    .build(),
                gst::CapsFeatures::new(["memory:DMABuf"]),
            )
            .build(),
    )
}

fn native_vulkan_apply_memory_path_decoder_policy() {
    if !native_vulkan_gst_prefers_dmabuf() {
        return;
    }
    for element in [
        "vah264dec",
        "vah265dec",
        "vavp8dec",
        "vavp9dec",
        "vaav1dec",
        "nvh264dec",
        "nvh265dec",
        "nvvp8dec",
        "nvvp9dec",
        "nvav1dec",
        "avdec_h264",
        "openh264dec",
        "vp9dec",
        "avdec_vp9",
        "dav1ddec",
        "avdec_av1",
        "av1dec",
    ] {
        let Some(factory) = gst::ElementFactory::find(element) else {
            continue;
        };
        if element.starts_with("va") {
            factory.set_rank(gst::Rank::PRIMARY + 2048);
        } else {
            factory.set_rank(gst::Rank::NONE);
        }
    }
}

fn native_vulkan_gst_prefers_dmabuf() -> bool {
    std::env::var("GILDER_VULKAN_GST_MEMORY_PATH")
        .map(|memory_path| {
            matches!(
                memory_path.as_str(),
                "dmabuf" | "DMABuf" | "gst-dmabuf" | "direct-dmabuf"
            )
        })
        .unwrap_or(false)
}

fn native_vulkan_gst_memory_preference() -> NativeVulkanVideoFrontendMemoryPreference {
    if native_vulkan_gst_prefers_dmabuf() {
        NativeVulkanVideoFrontendMemoryPreference::DirectDmabuf
    } else {
        NativeVulkanVideoFrontendMemoryPreference::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gst_sample_queue_policy_defaults_to_keep_last() {
        assert_eq!(
            native_vulkan_gst_sample_queue_policy_from_value(None),
            NativeVulkanGstSampleQueuePolicy::KeepLast
        );
        assert_eq!(
            native_vulkan_gst_sample_queue_policy_from_value(Some("keep-last".to_owned())),
            NativeVulkanGstSampleQueuePolicy::KeepLast
        );
    }

    #[test]
    fn gst_sample_queue_policy_allows_backpressure_for_diagnostics() {
        assert_eq!(
            native_vulkan_gst_sample_queue_policy_from_value(Some("backpressure".to_owned())),
            NativeVulkanGstSampleQueuePolicy::Backpressure
        );
        assert_eq!(
            native_vulkan_gst_sample_queue_policy_from_value(Some("blocking".to_owned())),
            NativeVulkanGstSampleQueuePolicy::Backpressure
        );
    }

    #[test]
    fn loop_seek_reuses_video_start_offset() {
        assert_eq!(
            native_vulkan_gst_video_loop_seek_position_ms(true, 12_345),
            Some(12_345)
        );
    }

    #[test]
    fn non_loop_video_does_not_request_loop_seek() {
        assert_eq!(
            native_vulkan_gst_video_loop_seek_position_ms(false, 12_345),
            None
        );
    }
}
