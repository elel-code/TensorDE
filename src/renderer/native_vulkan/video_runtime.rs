use std::path::PathBuf;

use serde::Serialize;

use crate::config::VideoDecoderPolicy;
use crate::core::FitMode;

use super::{
    NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy, NativeVulkanDmabufImportSnapshot,
    NativeVulkanDrmDeviceSnapshot, NativeVulkanGstVideoFrontendSnapshot, NativeVulkanRenderItem,
    NativeVulkanVideoCapsSnapshot, NativeVulkanVideoImportSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoRuntimeSnapshot {
    pub source: PathBuf,
    pub poster: Option<PathBuf>,
    pub fit: FitMode,
    pub loop_playback: bool,
    pub muted: bool,
    pub manifest_max_fps: Option<u32>,
    pub target_max_fps: Option<u32>,
    pub decoder_policy: VideoDecoderPolicy,
    pub start_offset_ms: u64,
    pub frontend: &'static str,
    pub frontend_status: &'static str,
    pub handoff_status: &'static str,
    pub texture_import_status: &'static str,
    pub audio_status: &'static str,
    pub audio_output_policy: &'static str,
    pub audio_output_mode: &'static str,
    pub audio_output_status: &'static str,
    pub audio_runtime_status: &'static str,
    pub audio_runtime_reached_clocked_playback: bool,
    pub audio_runtime_buffer_count: u32,
    pub audio_runtime_output_sink_count: usize,
    pub audio_runtime_position_query_hit_count: u32,
    pub audio_runtime_last_error: Option<String>,
    pub gst_state: Option<String>,
    pub eos_messages: u64,
    pub segment_done_messages: u64,
    pub frames_received: u64,
    pub frames_imported: u64,
    pub rendered_placeholder_frames: u64,
    pub poster_upload_bytes: Option<u64>,
    pub last_import_size: Option<(u32, u32)>,
    pub last_import_memory_path: Option<String>,
    pub last_import_error: Option<String>,
    pub last_import_elapsed_us: Option<u64>,
    pub max_import_elapsed_us: Option<u64>,
    pub last_dmabuf_import: Option<NativeVulkanDmabufImportSnapshot>,
    pub memory_route: NativeVulkanVideoMemoryRouteSnapshot,
    pub selected_vulkan_drm_device: Option<NativeVulkanDrmDeviceSnapshot>,
    pub last_sample_caps: Option<String>,
    pub last_sample_format: Option<String>,
    pub last_sample_size: Option<(u32, u32)>,
    pub last_sample_pts_ms: Option<u64>,
    pub last_sample_duration_ms: Option<u64>,
    pub last_sample_pts_delta_ms: Option<u64>,
    pub last_sample_memory_types: Vec<String>,
    pub actual_decoders: Vec<String>,
    pub decoder_policy_status: Option<String>,
    pub caps_report_count: usize,
    pub caps_memory_features: Vec<String>,
    pub caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoAudioRuntimeTelemetry {
    pub(super) reached_clocked_playback: bool,
    pub(super) audio_buffer_count: u32,
    pub(super) audio_output_sink_count: usize,
    pub(super) audio_position_query_hit_count: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl From<super::audio_clock::NativeVulkanAudioClockRuntimeTelemetry>
    for NativeVulkanVideoAudioRuntimeTelemetry
{
    fn from(value: super::audio_clock::NativeVulkanAudioClockRuntimeTelemetry) -> Self {
        Self {
            reached_clocked_playback: value.reached_clocked_playback,
            audio_buffer_count: value.audio_buffer_count,
            audio_output_sink_count: value.audio_output_sink_count,
            audio_position_query_hit_count: value.audio_position_query_hit_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoMemoryRouteSnapshot {
    pub route: &'static str,
    pub direct_candidate: bool,
    pub direct_import_confirmed: bool,
    pub copy_risk: &'static str,
    pub notes: Vec<&'static str>,
}

pub(super) fn native_vulkan_video_runtime_snapshot(
    item: &NativeVulkanRenderItem,
    frontend: Option<NativeVulkanGstVideoFrontendSnapshot>,
    import: Option<NativeVulkanVideoImportSnapshot>,
    audio_runtime: Option<NativeVulkanVideoAudioRuntimeTelemetry>,
    audio_runtime_last_error: Option<String>,
    selected_vulkan_drm_device: Option<NativeVulkanDrmDeviceSnapshot>,
    rendered_frames: u64,
    poster_upload_bytes: Option<u64>,
) -> Option<NativeVulkanVideoRuntimeSnapshot> {
    let NativeVulkanRenderItem::Video {
        source,
        poster,
        fit,
        loop_playback,
        muted,
        manifest_max_fps,
        target_max_fps,
        decoder_policy,
        start_offset_ms,
        ..
    } = item
    else {
        return None;
    };

    let frontend_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-receiving-samples",
        Some(_) => "appsink-started-waiting-for-samples",
        None if poster.is_some() => "not-started-poster-placeholder",
        None => "not-started-clear-placeholder",
    };
    let handoff_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-sample-handoff-active",
        Some(_) => "appsink-started-no-sample-yet",
        None => "pending-appsink-dmabuf-or-gpu-memory-handoff",
    };
    let frames_received = frontend
        .as_ref()
        .map(|frontend| frontend.frames_received)
        .unwrap_or(0);
    let frames_imported = import
        .as_ref()
        .map(|import| import.frames_imported)
        .unwrap_or(0);
    let received_placeholder_frames = rendered_frames.saturating_sub(frames_imported);
    let memory_route =
        native_vulkan_video_memory_route_snapshot(frontend.as_ref(), import.as_ref());

    let audio_output_policy = NativeVulkanAudioOutputPolicy::Plan;
    let audio_output_mode = audio_output_policy.resolve(*muted);
    let audio_output_status = if audio_output_mode == NativeVulkanAudioOutputMode::ClockOnly {
        "disabled-by-muted-plan"
    } else {
        "auto-output-ready-for-audio-clock-runtime"
    };
    let audio_runtime_status = if audio_runtime_last_error.is_some() {
        "audio-runtime-error"
    } else if audio_output_mode == NativeVulkanAudioOutputMode::ClockOnly {
        "disabled-by-muted-plan"
    } else if audio_runtime
        .map(|runtime| runtime.reached_clocked_playback)
        .unwrap_or(false)
    {
        "clocked-playback-active"
    } else if audio_runtime.is_some() {
        "audio-runtime-started-waiting-for-clocked-playback"
    } else {
        "auto-output-ready-for-audio-clock-runtime"
    };
    let audio_runtime_reached_clocked_playback = audio_runtime
        .map(|runtime| runtime.reached_clocked_playback)
        .unwrap_or(false);
    let audio_runtime_buffer_count = audio_runtime
        .map(|runtime| runtime.audio_buffer_count)
        .unwrap_or(0);
    let audio_runtime_output_sink_count = audio_runtime
        .map(|runtime| runtime.audio_output_sink_count)
        .unwrap_or(0);
    let audio_runtime_position_query_hit_count = audio_runtime
        .map(|runtime| runtime.audio_position_query_hit_count)
        .unwrap_or(0);

    Some(NativeVulkanVideoRuntimeSnapshot {
        source: source.clone(),
        poster: poster.clone(),
        fit: *fit,
        loop_playback: *loop_playback,
        muted: *muted,
        manifest_max_fps: *manifest_max_fps,
        target_max_fps: *target_max_fps,
        decoder_policy: *decoder_policy,
        start_offset_ms: *start_offset_ms,
        frontend: if frontend.is_some() {
            "gstreamer-appsink"
        } else {
            "gstreamer-planned"
        },
        frontend_status,
        handoff_status,
        texture_import_status: import
            .as_ref()
            .map(|import| import.texture_import_status)
            .unwrap_or("not-importing-yet"),
        audio_status: if *muted {
            "muted-no-audio-pipeline"
        } else {
            "planned-auto-audio-output-pipeline"
        },
        audio_output_policy: audio_output_policy.as_str(),
        audio_output_mode: audio_output_mode.as_str(),
        audio_output_status,
        audio_runtime_status,
        audio_runtime_reached_clocked_playback,
        audio_runtime_buffer_count,
        audio_runtime_output_sink_count,
        audio_runtime_position_query_hit_count,
        audio_runtime_last_error,
        gst_state: frontend
            .as_ref()
            .and_then(|frontend| frontend.gst_state.clone()),
        eos_messages: frontend
            .as_ref()
            .map(|frontend| frontend.eos_messages)
            .unwrap_or(0),
        segment_done_messages: frontend
            .as_ref()
            .map(|frontend| frontend.segment_done_messages)
            .unwrap_or(0),
        frames_received,
        frames_imported,
        rendered_placeholder_frames: received_placeholder_frames,
        poster_upload_bytes,
        last_import_size: import.as_ref().and_then(|import| import.last_import_size),
        last_import_memory_path: import
            .as_ref()
            .and_then(|import| import.last_import_memory_path.clone()),
        last_import_error: import
            .as_ref()
            .and_then(|import| import.last_import_error.clone()),
        last_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.last_import_elapsed_us),
        max_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.max_import_elapsed_us),
        last_dmabuf_import: import
            .as_ref()
            .and_then(|import| import.last_dmabuf_import.clone()),
        memory_route,
        selected_vulkan_drm_device,
        last_sample_caps: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_caps.clone()),
        last_sample_format: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_format.clone()),
        last_sample_size: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_size),
        last_sample_pts_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_ms),
        last_sample_duration_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_duration_ms),
        last_sample_pts_delta_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_delta_ms),
        last_sample_memory_types: frontend
            .as_ref()
            .map(|frontend| frontend.last_sample_memory_types.clone())
            .unwrap_or_default(),
        actual_decoders: frontend
            .as_ref()
            .map(|frontend| frontend.actual_decoders.clone())
            .unwrap_or_default(),
        decoder_policy_status: frontend
            .as_ref()
            .and_then(|frontend| frontend.decoder_policy_status.clone()),
        caps_report_count: frontend
            .as_ref()
            .map(|frontend| frontend.caps_report_count)
            .unwrap_or(0),
        caps_memory_features: frontend
            .as_ref()
            .map(|frontend| frontend.caps_memory_features.clone())
            .unwrap_or_default(),
        caps_reports: frontend
            .as_ref()
            .map(|frontend| frontend.caps_reports.clone())
            .unwrap_or_default(),
        last_error: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_error.clone()),
    })
}

fn native_vulkan_video_memory_route_snapshot(
    frontend: Option<&NativeVulkanGstVideoFrontendSnapshot>,
    import: Option<&NativeVulkanVideoImportSnapshot>,
) -> NativeVulkanVideoMemoryRouteSnapshot {
    let import_path = import.and_then(|import| import.last_import_memory_path.as_deref());
    let has_dmabuf_contract = import
        .and_then(|import| import.last_dmabuf_import.as_ref())
        .is_some();
    let has_import_error = import
        .and_then(|import| import.last_import_error.as_ref())
        .is_some();
    let has_dmabuf_signal = native_vulkan_video_frontend_has_memory(frontend, "DMABuf")
        || native_vulkan_video_frontend_has_memory(frontend, "DmaBuf")
        || native_vulkan_video_frontend_has_memory(frontend, "dmabuf");
    let has_va_signal = native_vulkan_video_frontend_has_memory(frontend, "VAMemory");
    let has_cuda_signal = native_vulkan_video_frontend_has_memory(frontend, "CUDAMemory");
    let has_gl_signal = native_vulkan_video_frontend_has_memory(frontend, "GLMemory");
    let has_sample = frontend
        .map(|frontend| frontend.frames_received > 0)
        .unwrap_or(false);

    if has_dmabuf_contract {
        let route = match import_path {
            Some(path) if path.contains("GstVAMemory") => "direct-va-drm-prime-import",
            _ => "direct-dmabuf-import",
        };
        return NativeVulkanVideoMemoryRouteSnapshot {
            route,
            direct_candidate: true,
            direct_import_confirmed: true,
            copy_risk: "low-confirmed-external-memory-import",
            notes: vec![
                "DRM format/modifier/plane layout was checked against the target Vulkan device",
            ],
        };
    }

    if let Some(path) = import_path {
        if path.contains("CUDAMemory") {
            return NativeVulkanVideoMemoryRouteSnapshot {
                route: "cuda-vulkan-copy",
                direct_candidate: false,
                direct_import_confirmed: false,
                copy_risk: "gpu-copy-or-sync-risk",
                notes: vec![
                    "CUDAMemory is GPU memory but not a confirmed DMABUF/Vulkan direct import",
                    "Sunshine-style routing treats this separately from direct DMABUF",
                ],
            };
        }
        if path.contains("GstDmaBufMemory") || path.contains("GstVAMemory") {
            return NativeVulkanVideoMemoryRouteSnapshot {
                route: "dmabuf-import-unverified",
                direct_candidate: true,
                direct_import_confirmed: false,
                copy_risk: "unknown-until-import-contract",
                notes: vec![
                    "import path names DMABUF/DRM PRIME but no contract snapshot was recorded",
                ],
            };
        }
    }

    if has_dmabuf_signal || has_va_signal {
        let route = if has_va_signal {
            "va-memory-pending-drm-prime-export"
        } else {
            "dmabuf-caps-pending-import"
        };
        let mut notes = vec![
            "caps or sample memory advertise a direct-memory candidate",
            "zero-copy is not proven until the Vulkan importer records a DMABUF contract",
        ];
        if has_import_error {
            notes.push("last importer attempt failed");
        }
        return NativeVulkanVideoMemoryRouteSnapshot {
            route,
            direct_candidate: true,
            direct_import_confirmed: false,
            copy_risk: "unknown-until-import-contract",
            notes,
        };
    }

    if has_cuda_signal {
        return NativeVulkanVideoMemoryRouteSnapshot {
            route: "cuda-memory-pending-import",
            direct_candidate: false,
            direct_import_confirmed: false,
            copy_risk: "gpu-copy-or-sync-risk",
            notes: vec!["CUDAMemory is vendor GPU memory, not a portable DMABUF contract"],
        };
    }

    if has_gl_signal {
        return NativeVulkanVideoMemoryRouteSnapshot {
            route: "gl-memory-intermediate",
            direct_candidate: false,
            direct_import_confirmed: false,
            copy_risk: "gpu-copy-or-export-risk",
            notes: vec!["GLMemory may still need EGL/export conversion before Vulkan sampling"],
        };
    }

    if has_sample {
        return NativeVulkanVideoMemoryRouteSnapshot {
            route: "system-memory-or-unsupported",
            direct_candidate: false,
            direct_import_confirmed: false,
            copy_risk: "high-cpu-copy-risk",
            notes: vec!["appsink is receiving samples without a supported GPU memory signal"],
        };
    }

    NativeVulkanVideoMemoryRouteSnapshot {
        route: "not-negotiated",
        direct_candidate: false,
        direct_import_confirmed: false,
        copy_risk: "unknown",
        notes: Vec::new(),
    }
}

fn native_vulkan_video_frontend_has_memory(
    frontend: Option<&NativeVulkanGstVideoFrontendSnapshot>,
    needle: &str,
) -> bool {
    let Some(frontend) = frontend else {
        return false;
    };
    frontend
        .last_sample_memory_types
        .iter()
        .any(|memory| memory.contains(needle))
        || frontend
            .caps_memory_features
            .iter()
            .any(|feature| feature.contains(needle))
        || frontend.caps_reports.iter().any(|report| {
            report.caps.contains(needle)
                || report
                    .memory_features
                    .iter()
                    .any(|feature| feature.contains(needle))
        })
}
