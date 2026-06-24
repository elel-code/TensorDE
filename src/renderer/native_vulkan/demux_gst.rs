//! GStreamer packet frontend for the native Vulkan demux boundary.
//!
//! This module is intentionally a provider: it turns GStreamer appsink samples
//! into codec access units, then hands them to the frontend-agnostic packet
//! queue in `demux.rs`.

use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, Instant};

use gst::prelude::*;
use gstreamer as gst;

use super::NativeVulkanError;
use super::demux::{
    NativeVulkanStreamingAccessUnit, NativeVulkanStreamingPacketFrontend,
    NativeVulkanStreamingPacketQueue, native_vulkan_start_streaming_packet_queue_from_frontend,
};

pub(super) trait NativeVulkanGstStreamingAccessUnit:
    NativeVulkanStreamingAccessUnit
{
    fn pipeline(source: &Path) -> Result<gst::Pipeline, NativeVulkanError>;
    fn sink_name() -> &'static str;
    fn from_sample(sample: &gst::Sample) -> Result<Self, NativeVulkanError>;
}

pub(super) fn native_vulkan_start_gst_streaming_packet_queue<
    A: NativeVulkanGstStreamingAccessUnit + 'static,
>(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    let frontend = NativeVulkanGstStreamingPacketFrontend::<A>::new(source)?;
    native_vulkan_start_streaming_packet_queue_from_frontend(Box::new(frontend), capacity)
}

pub(super) fn native_vulkan_run_gst_bitstream_pipeline<T>(
    source: &Path,
    codec_label: &'static str,
    sink_name: &'static str,
    pipeline: impl FnOnce(&Path) -> Result<gst::Pipeline, NativeVulkanError>,
    collect: impl FnOnce(&gst::Element, &gst::Bus) -> Result<T, NativeVulkanError>,
) -> Result<T, NativeVulkanError> {
    gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    let pipeline = pipeline(source)?;
    let sink = pipeline.by_name(sink_name).ok_or_else(|| {
        NativeVulkanError::Video(format!("{codec_label} bitstream appsink not found"))
    })?;
    let bus = pipeline.bus().ok_or_else(|| {
        NativeVulkanError::Video(format!("{codec_label} bitstream pipeline has no bus"))
    })?;

    let result = (|| -> Result<T, NativeVulkanError> {
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        collect(&sink, &bus)
    })();

    let _ = pipeline.set_state(gst::State::Null);
    let _ = pipeline.state(gst::ClockTime::from_mseconds(500));
    result
}

pub(super) fn native_vulkan_h264_bitstream_pipeline(
    source: &Path,
) -> Result<gst::Pipeline, NativeVulkanError> {
    native_vulkan_gst_bitstream_pipeline(
        source,
        NativeVulkanGstBitstreamPipelineSpec {
            codec_label: "H.264",
            demux_element_name: native_vulkan_qtdemux_element_name,
            parser_element_name: "h264parse",
            parser_config_interval: Some(-1),
            caps: "video/x-h264,stream-format=byte-stream,alignment=au",
            sink_name: "gilder-native-vulkan-h264-bitstream-appsink",
            pad_media_type: "video/x-h264",
        },
    )
}

pub(super) fn native_vulkan_h265_bitstream_pipeline(
    source: &Path,
) -> Result<gst::Pipeline, NativeVulkanError> {
    native_vulkan_gst_bitstream_pipeline(
        source,
        NativeVulkanGstBitstreamPipelineSpec {
            codec_label: "H.265",
            demux_element_name: native_vulkan_qtdemux_element_name,
            parser_element_name: "h265parse",
            parser_config_interval: Some(-1),
            caps: "video/x-h265,stream-format=byte-stream,alignment=au",
            sink_name: "gilder-native-vulkan-h265-bitstream-appsink",
            pad_media_type: "video/x-h265",
        },
    )
}

pub(super) fn native_vulkan_av1_bitstream_pipeline(
    source: &Path,
) -> Result<gst::Pipeline, NativeVulkanError> {
    native_vulkan_gst_bitstream_pipeline(
        source,
        NativeVulkanGstBitstreamPipelineSpec {
            codec_label: "AV1",
            demux_element_name: native_vulkan_av1_demux_element_name,
            parser_element_name: "av1parse",
            parser_config_interval: None,
            caps: "video/x-av1,stream-format=obu-stream,alignment=frame",
            sink_name: "gilder-native-vulkan-av1-bitstream-appsink",
            pad_media_type: "video/x-av1",
        },
    )
}

#[derive(Clone, Copy)]
struct NativeVulkanGstBitstreamPipelineSpec {
    codec_label: &'static str,
    demux_element_name: fn(&Path) -> &'static str,
    parser_element_name: &'static str,
    parser_config_interval: Option<i32>,
    caps: &'static str,
    sink_name: &'static str,
    pad_media_type: &'static str,
}

fn native_vulkan_gst_bitstream_pipeline(
    source: &Path,
    spec: NativeVulkanGstBitstreamPipelineSpec,
) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_gst_bitstream_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_vulkan_gst_bitstream_element((spec.demux_element_name)(source))?;
    let queue = native_vulkan_gst_bitstream_element("queue")?;
    native_vulkan_configure_bitstream_queue(&queue);
    let parser = native_vulkan_gst_bitstream_element(spec.parser_element_name)?;
    if let Some(config_interval) = spec.parser_config_interval
        && parser.find_property("config-interval").is_some()
    {
        parser.set_property("config-interval", config_interval);
    }
    if parser.find_property("disable-passthrough").is_some() {
        parser.set_property("disable-passthrough", true);
    }
    let capsfilter = native_vulkan_gst_bitstream_element("capsfilter")?;
    let caps = spec
        .caps
        .parse::<gst::Caps>()
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);
    let sink = native_vulkan_gst_bitstream_element("appsink")?;
    sink.set_property("name", spec.sink_name);
    native_vulkan_configure_bitstream_appsink(&sink);

    pipeline
        .add_many([&filesrc, &demux, &queue, &parser, &capsfilter, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &parser, &capsfilter, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue.static_pad("sink").ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{} bitstream queue has no sink pad",
            spec.codec_label
        ))
    })?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_vulkan_gst_pad_media_type_is(pad, spec.pad_media_type)
        {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline)
}

struct NativeVulkanGstStreamingPacketFrontend<A: NativeVulkanGstStreamingAccessUnit> {
    pipeline: gst::Pipeline,
    sink: gst::Element,
    bus: gst::Bus,
    eos_count: u32,
    loop_count: u32,
    _access_unit: PhantomData<A>,
}

impl<A: NativeVulkanGstStreamingAccessUnit> NativeVulkanGstStreamingPacketFrontend<A> {
    fn new(source: &Path) -> Result<Self, NativeVulkanError> {
        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let pipeline = A::pipeline(source)?;
        let sink = pipeline.by_name(A::sink_name()).ok_or_else(|| {
            NativeVulkanError::Video(format!("{} bitstream appsink not found", A::CODEC_LABEL))
        })?;
        let bus = pipeline.bus().ok_or_else(|| {
            NativeVulkanError::Video(format!("{} bitstream pipeline has no bus", A::CODEC_LABEL))
        })?;

        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

        Ok(Self {
            pipeline,
            sink,
            bus,
            eos_count: 0,
            loop_count: 0,
            _access_unit: PhantomData,
        })
    }
}

impl<A: NativeVulkanGstStreamingAccessUnit> Drop for NativeVulkanGstStreamingPacketFrontend<A> {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
    }
}

impl<A: NativeVulkanGstStreamingAccessUnit> NativeVulkanStreamingPacketFrontend<A>
    for NativeVulkanGstStreamingPacketFrontend<A>
{
    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        native_vulkan_pull_gst_streaming_access_unit::<A>(
            &self.pipeline,
            &self.sink,
            &self.bus,
            loop_on_eos,
            &mut self.eos_count,
            &mut self.loop_count,
        )
    }

    fn eos_count(&self) -> u32 {
        self.eos_count
    }

    fn loop_count(&self) -> u32 {
        self.loop_count
    }
}

fn native_vulkan_pull_gst_streaming_access_unit<A: NativeVulkanGstStreamingAccessUnit>(
    pipeline: &gst::Pipeline,
    sink: &gst::Element,
    bus: &gst::Bus,
    loop_on_eos: bool,
    eos_count: &mut u32,
    loop_count: &mut u32,
) -> Result<Option<A>, NativeVulkanError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let mut saw_bus_eos = false;
        while let Some(message) = bus.pop() {
            match message.view() {
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
                    return Err(NativeVulkanError::Video(message));
                }
                gst::MessageView::Eos(_) => {
                    saw_bus_eos = true;
                }
                _ => {}
            }
        }

        let timeout_ns = 50_000_000u64;
        let sample = sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]);
        if let Some(sample) = sample {
            return A::from_sample(&sample).map(Some);
        }
        if saw_bus_eos || native_vulkan_streaming_sink_is_eos(sink) {
            *eos_count = eos_count.saturating_add(1);
            if !loop_on_eos {
                return Ok(None);
            }
            pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    gst::ClockTime::ZERO,
                )
                .map_err(|err| {
                    NativeVulkanError::Video(format!(
                        "seek {} streaming packet queue to start: {err}",
                        A::CODEC_LABEL
                    ))
                })?;
            *loop_count = loop_count.saturating_add(1);
        }
    }

    Err(NativeVulkanError::Video(format!(
        "{} streaming packet queue timed out waiting for an AU",
        A::CODEC_LABEL
    )))
}

fn native_vulkan_streaming_sink_is_eos(sink: &gst::Element) -> bool {
    sink.find_property("eos").is_some() && sink.property::<bool>("eos")
}

fn native_vulkan_av1_demux_element_name(source: &Path) -> &'static str {
    match source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("webm") | Some("mkv") | Some("mka") => "matroskademux",
        _ => "qtdemux",
    }
}

fn native_vulkan_qtdemux_element_name(_: &Path) -> &'static str {
    "qtdemux"
}

fn native_vulkan_gst_bitstream_element(name: &str) -> Result<gst::Element, NativeVulkanError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeVulkanError::Video(format!("create {name}: {err}")))
}

fn native_vulkan_configure_bitstream_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 1u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
}

fn native_vulkan_configure_bitstream_appsink(sink: &gst::Element) {
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", false);
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
        sink.set_property("max-buffers", 1u32);
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", false);
    }
}

fn native_vulkan_gst_pad_media_type_is(pad: &gst::Pad, expected: &str) -> bool {
    pad.current_caps()
        .or_else(|| Some(pad.query_caps(None)))
        .and_then(|caps| {
            caps.structure(0)
                .map(|structure| structure.name() == expected)
        })
        .unwrap_or(false)
}
