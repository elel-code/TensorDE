use std::path::PathBuf;

use serde::Serialize;

use crate::config::VideoDecoderPolicy;
use crate::core::FitMode;

use super::video_frontend::{NativeVulkanVideoCapsSnapshot, NativeVulkanVideoFrontendSnapshot};
use super::video_import::{NativeVulkanDmabufImportSnapshot, NativeVulkanVideoImportSnapshot};
use super::{
    NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy, NativeVulkanDrmDeviceSnapshot,
    NativeVulkanRenderItem,
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
    pub frontend_provider: &'static str,
    pub frontend_route: &'static str,
    pub frontend_decode_owner: &'static str,
    pub frontend_memory_preference: &'static str,
    pub frontend_sample_queue_policy: &'static str,
    pub frontend_status: &'static str,
    pub handoff_status: &'static str,
    pub texture_import_status: &'static str,
    pub audio_status: &'static str,
    pub audio_output_policy: &'static str,
    pub audio_output_mode: &'static str,
    pub audio_output_status: &'static str,
    pub audio_runtime_status: &'static str,
    pub audio_runtime_provider: &'static str,
    pub audio_runtime_reached_clocked_playback: bool,
    pub audio_runtime_buffer_count: u32,
    pub audio_runtime_output_sink_count: usize,
    pub audio_runtime_loop_seek_count: u32,
    pub audio_runtime_loop_seek_error_count: u32,
    pub audio_runtime_loop_restart_count: u32,
    pub audio_runtime_last_loop_seek_position_ms: Option<u64>,
    pub audio_runtime_clock_serial: u32,
    pub audio_runtime_segment_start_position_ns: Option<u64>,
    pub audio_runtime_segment_elapsed_ns: Option<u64>,
    pub audio_runtime_position_stale_count: u32,
    pub audio_runtime_sample_stale_count: u32,
    pub audio_runtime_master_clock_estimate_ns: Option<u64>,
    pub audio_runtime_sampled_video_frame_count: u32,
    pub audio_runtime_position_query_count: u32,
    pub audio_runtime_position_query_hit_count: u32,
    pub audio_runtime_video_clock_drift_latest_ns: Option<i64>,
    pub audio_runtime_video_master_clock_drift_latest_ns: Option<i64>,
    pub audio_runtime_video_master_clock_drift_abs_max_ns: Option<u64>,
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
    pub(super) audio_provider: &'static str,
    pub(super) reached_clocked_playback: bool,
    pub(super) audio_buffer_count: u32,
    pub(super) audio_output_sink_count: usize,
    pub(super) audio_loop_seek_count: u32,
    pub(super) audio_loop_seek_error_count: u32,
    pub(super) audio_loop_restart_count: u32,
    pub(super) audio_last_loop_seek_position_ms: Option<u64>,
    pub(super) audio_clock_serial: u32,
    pub(super) audio_segment_start_position_ns: Option<u64>,
    pub(super) audio_segment_elapsed_ns: Option<u64>,
    pub(super) audio_position_stale_count: u32,
    pub(super) audio_sample_stale_count: u32,
    pub(super) audio_master_clock_estimate_ns: Option<u64>,
    pub(super) sampled_video_frame_count: u32,
    pub(super) audio_position_query_count: u32,
    pub(super) audio_position_query_hit_count: u32,
    pub(super) audio_video_clock_drift_latest_ns: Option<i64>,
    pub(super) audio_video_master_clock_drift_latest_ns: Option<i64>,
    pub(super) audio_video_master_clock_drift_abs_max_ns: Option<u64>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoAudioRuntimeTelemetry {
    pub(super) fn from_audio_clock_runtime(
        audio_provider: &'static str,
        value: super::audio_clock::NativeVulkanAudioClockRuntimeTelemetry,
    ) -> Self {
        Self {
            audio_provider,
            reached_clocked_playback: value.reached_clocked_playback,
            audio_buffer_count: value.audio_buffer_count,
            audio_output_sink_count: value.audio_output_sink_count,
            audio_loop_seek_count: value.audio_loop_seek_count,
            audio_loop_seek_error_count: value.audio_loop_seek_error_count,
            audio_loop_restart_count: value.audio_loop_restart_count,
            audio_last_loop_seek_position_ms: value.audio_last_loop_seek_position_ms,
            audio_clock_serial: value.audio_clock_serial,
            audio_segment_start_position_ns: value.audio_segment_start_position_ns,
            audio_segment_elapsed_ns: value.audio_segment_elapsed_ns,
            audio_position_stale_count: value.audio_position_stale_count,
            audio_sample_stale_count: value.audio_sample_stale_count,
            audio_master_clock_estimate_ns: value.audio_master_clock_estimate_ns,
            sampled_video_frame_count: value.sampled_video_frame_count,
            audio_position_query_count: value.audio_position_query_count,
            audio_position_query_hit_count: value.audio_position_query_hit_count,
            audio_video_clock_drift_latest_ns: value.audio_video_clock_drift_latest_ns,
            audio_video_master_clock_drift_latest_ns: value
                .audio_video_master_clock_drift_latest_ns,
            audio_video_master_clock_drift_abs_max_ns: value
                .audio_video_master_clock_drift_abs_max_ns,
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
    frontend: Option<NativeVulkanVideoFrontendSnapshot>,
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
    let audio_output_status = match audio_output_mode {
        NativeVulkanAudioOutputMode::ClockOnly => "clock-only-output-ready-for-audio-clock-runtime",
        NativeVulkanAudioOutputMode::Auto => "auto-output-ready-for-audio-clock-runtime",
    };
    let audio_runtime_status = if audio_runtime_last_error.is_some() {
        "audio-runtime-error"
    } else if audio_runtime
        .map(|runtime| runtime.reached_clocked_playback)
        .unwrap_or(false)
    {
        "clocked-playback-active"
    } else if audio_runtime.is_some() {
        "audio-runtime-started-waiting-for-clocked-playback"
    } else {
        audio_output_status
    };
    let audio_runtime_reached_clocked_playback = audio_runtime
        .map(|runtime| runtime.reached_clocked_playback)
        .unwrap_or(false);
    let audio_runtime_provider = audio_runtime
        .map(|runtime| runtime.audio_provider)
        .unwrap_or("gstreamer");
    let audio_runtime_buffer_count = audio_runtime
        .map(|runtime| runtime.audio_buffer_count)
        .unwrap_or(0);
    let audio_runtime_output_sink_count = audio_runtime
        .map(|runtime| runtime.audio_output_sink_count)
        .unwrap_or(0);
    let audio_runtime_loop_seek_count = audio_runtime
        .map(|runtime| runtime.audio_loop_seek_count)
        .unwrap_or(0);
    let audio_runtime_loop_seek_error_count = audio_runtime
        .map(|runtime| runtime.audio_loop_seek_error_count)
        .unwrap_or(0);
    let audio_runtime_loop_restart_count = audio_runtime
        .map(|runtime| runtime.audio_loop_restart_count)
        .unwrap_or(0);
    let audio_runtime_last_loop_seek_position_ms =
        audio_runtime.and_then(|runtime| runtime.audio_last_loop_seek_position_ms);
    let audio_runtime_clock_serial = audio_runtime
        .map(|runtime| runtime.audio_clock_serial)
        .unwrap_or(0);
    let audio_runtime_segment_start_position_ns =
        audio_runtime.and_then(|runtime| runtime.audio_segment_start_position_ns);
    let audio_runtime_segment_elapsed_ns =
        audio_runtime.and_then(|runtime| runtime.audio_segment_elapsed_ns);
    let audio_runtime_position_stale_count = audio_runtime
        .map(|runtime| runtime.audio_position_stale_count)
        .unwrap_or(0);
    let audio_runtime_sample_stale_count = audio_runtime
        .map(|runtime| runtime.audio_sample_stale_count)
        .unwrap_or(0);
    let audio_runtime_master_clock_estimate_ns =
        audio_runtime.and_then(|runtime| runtime.audio_master_clock_estimate_ns);
    let audio_runtime_sampled_video_frame_count = audio_runtime
        .map(|runtime| runtime.sampled_video_frame_count)
        .unwrap_or(0);
    let audio_runtime_position_query_count = audio_runtime
        .map(|runtime| runtime.audio_position_query_count)
        .unwrap_or(0);
    let audio_runtime_position_query_hit_count = audio_runtime
        .map(|runtime| runtime.audio_position_query_hit_count)
        .unwrap_or(0);
    let audio_runtime_video_clock_drift_latest_ns =
        audio_runtime.and_then(|runtime| runtime.audio_video_clock_drift_latest_ns);
    let audio_runtime_video_master_clock_drift_latest_ns =
        audio_runtime.and_then(|runtime| runtime.audio_video_master_clock_drift_latest_ns);
    let audio_runtime_video_master_clock_drift_abs_max_ns =
        audio_runtime.and_then(|runtime| runtime.audio_video_master_clock_drift_abs_max_ns);

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
            frontend
                .as_ref()
                .map(|frontend| frontend.provider.active_frontend_label())
                .unwrap_or("replaceable-frontend-active")
        } else {
            "gstreamer-planned"
        },
        frontend_provider: frontend
            .as_ref()
            .map(|frontend| frontend.provider.as_str())
            .unwrap_or("gstreamer"),
        frontend_route: frontend
            .as_ref()
            .map(|frontend| frontend.route.as_str())
            .unwrap_or("decoded-provider"),
        frontend_decode_owner: frontend
            .as_ref()
            .map(|frontend| frontend.decode_owner.as_str())
            .unwrap_or("gstreamer"),
        frontend_memory_preference: frontend
            .as_ref()
            .map(|frontend| frontend.memory_preference.as_str())
            .unwrap_or("auto"),
        frontend_sample_queue_policy: frontend
            .as_ref()
            .map(|frontend| frontend.sample_queue_policy)
            .unwrap_or("keep-last"),
        frontend_status,
        handoff_status,
        texture_import_status: import
            .as_ref()
            .map(|import| import.texture_import_status)
            .unwrap_or("not-importing-yet"),
        audio_status: if *muted {
            "muted-clock-only-audio-clock-pipeline"
        } else {
            "planned-auto-audio-output-pipeline"
        },
        audio_output_policy: audio_output_policy.as_str(),
        audio_output_mode: audio_output_mode.as_str(),
        audio_output_status,
        audio_runtime_status,
        audio_runtime_provider,
        audio_runtime_reached_clocked_playback,
        audio_runtime_buffer_count,
        audio_runtime_output_sink_count,
        audio_runtime_loop_seek_count,
        audio_runtime_loop_seek_error_count,
        audio_runtime_loop_restart_count,
        audio_runtime_last_loop_seek_position_ms,
        audio_runtime_clock_serial,
        audio_runtime_segment_start_position_ns,
        audio_runtime_segment_elapsed_ns,
        audio_runtime_position_stale_count,
        audio_runtime_sample_stale_count,
        audio_runtime_master_clock_estimate_ns,
        audio_runtime_sampled_video_frame_count,
        audio_runtime_position_query_count,
        audio_runtime_position_query_hit_count,
        audio_runtime_video_clock_drift_latest_ns,
        audio_runtime_video_master_clock_drift_latest_ns,
        audio_runtime_video_master_clock_drift_abs_max_ns,
        audio_runtime_last_error,
        gst_state: frontend
            .as_ref()
            .and_then(|frontend| frontend.provider_state.clone()),
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
    frontend: Option<&NativeVulkanVideoFrontendSnapshot>,
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
    frontend: Option<&NativeVulkanVideoFrontendSnapshot>,
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

#[cfg(test)]
mod tests {
    use super::super::video_frontend::{
        NativeVulkanVideoDecodeOwner, NativeVulkanVideoFrontendMemoryPreference,
        NativeVulkanVideoFrontendProvider, NativeVulkanVideoFrontendRoute,
    };
    use super::super::video_import::{
        NativeVulkanDmabufImportSnapshot, NativeVulkanVideoImportSnapshot,
    };
    use super::super::{DRM_FORMAT_MOD_LINEAR, DRM_FORMAT_NV12, NativeVulkanRenderItem};
    use super::*;
    use crate::config::VideoDecoderPolicy;
    use crate::core::FitMode;
    use std::path::PathBuf;

    #[test]
    fn video_runtime_snapshot_reports_pending_gstreamer_handoff() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: Some(PathBuf::from("/tmp/poster.png")),
            fit: FitMode::Contain,
            loop_playback: true,
            muted: false,
            manifest_max_fps: Some(240),
            target_max_fps: Some(120),
            decoder_policy: VideoDecoderPolicy::HardwareRequired,
            start_offset_ms: 1500,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };

        let snapshot = native_vulkan_video_runtime_snapshot(
            &item,
            None,
            None,
            None,
            None,
            None,
            9,
            Some(1024),
        )
        .expect("video snapshot");

        assert_eq!(snapshot.frontend, "gstreamer-planned");
        assert_eq!(snapshot.frontend_provider, "gstreamer");
        assert_eq!(snapshot.frontend_route, "decoded-provider");
        assert_eq!(snapshot.frontend_decode_owner, "gstreamer");
        assert_eq!(snapshot.frontend_memory_preference, "auto");
        assert_eq!(snapshot.frontend_sample_queue_policy, "keep-last");
        assert_eq!(snapshot.frontend_status, "not-started-poster-placeholder");
        assert_eq!(
            snapshot.handoff_status,
            "pending-appsink-dmabuf-or-gpu-memory-handoff"
        );
        assert_eq!(snapshot.audio_status, "planned-auto-audio-output-pipeline");
        assert_eq!(snapshot.audio_output_policy, "plan");
        assert_eq!(snapshot.audio_output_mode, "auto");
        assert_eq!(
            snapshot.audio_output_status,
            "auto-output-ready-for-audio-clock-runtime"
        );
        assert_eq!(
            snapshot.audio_runtime_status,
            "auto-output-ready-for-audio-clock-runtime"
        );
        assert_eq!(snapshot.audio_runtime_provider, "gstreamer");
        assert!(!snapshot.audio_runtime_reached_clocked_playback);
        assert_eq!(snapshot.audio_runtime_buffer_count, 0);
        assert_eq!(snapshot.audio_runtime_output_sink_count, 0);
        assert_eq!(snapshot.audio_runtime_loop_seek_count, 0);
        assert_eq!(snapshot.audio_runtime_loop_seek_error_count, 0);
        assert_eq!(snapshot.audio_runtime_loop_restart_count, 0);
        assert_eq!(snapshot.audio_runtime_last_loop_seek_position_ms, None);
        assert_eq!(snapshot.audio_runtime_clock_serial, 0);
        assert_eq!(snapshot.audio_runtime_segment_start_position_ns, None);
        assert_eq!(snapshot.audio_runtime_segment_elapsed_ns, None);
        assert_eq!(snapshot.audio_runtime_position_stale_count, 0);
        assert_eq!(snapshot.audio_runtime_sample_stale_count, 0);
        assert_eq!(snapshot.audio_runtime_master_clock_estimate_ns, None);
        assert_eq!(snapshot.audio_runtime_sampled_video_frame_count, 0);
        assert_eq!(snapshot.audio_runtime_position_query_count, 0);
        assert_eq!(snapshot.audio_runtime_position_query_hit_count, 0);
        assert_eq!(snapshot.audio_runtime_video_clock_drift_latest_ns, None);
        assert_eq!(
            snapshot.audio_runtime_video_master_clock_drift_latest_ns,
            None
        );
        assert_eq!(
            snapshot.audio_runtime_video_master_clock_drift_abs_max_ns,
            None
        );
        assert_eq!(snapshot.audio_runtime_last_error, None);
        assert_eq!(snapshot.frames_received, 0);
        assert_eq!(snapshot.frames_imported, 0);
        assert_eq!(snapshot.rendered_placeholder_frames, 9);
        assert_eq!(snapshot.poster_upload_bytes, Some(1024));
        assert_eq!(snapshot.texture_import_status, "not-importing-yet");
        assert_eq!(snapshot.last_import_size, None);
        assert_eq!(snapshot.last_import_memory_path, None);
        assert_eq!(snapshot.last_import_error, None);
        assert_eq!(snapshot.last_import_elapsed_us, None);
        assert_eq!(snapshot.max_import_elapsed_us, None);
        assert_eq!(snapshot.last_dmabuf_import, None);
        assert_eq!(snapshot.memory_route.route, "not-negotiated");
        assert!(!snapshot.memory_route.direct_candidate);
        assert!(!snapshot.memory_route.direct_import_confirmed);
        assert_eq!(snapshot.start_offset_ms, 1500);
        assert_eq!(snapshot.gst_state, None);
        assert_eq!(snapshot.decoder_policy_status, None);
        assert_eq!(snapshot.caps_report_count, 0);
        assert_eq!(snapshot.segment_done_messages, 0);
    }

    #[test]
    fn video_runtime_snapshot_reports_audio_runtime_active() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: false,
            manifest_max_fps: Some(240),
            target_max_fps: Some(240),
            decoder_policy: VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let audio_runtime = NativeVulkanVideoAudioRuntimeTelemetry {
            audio_provider: "gstreamer",
            reached_clocked_playback: true,
            audio_buffer_count: 18,
            audio_output_sink_count: 2,
            audio_loop_seek_count: 2,
            audio_loop_seek_error_count: 0,
            audio_loop_restart_count: 2,
            audio_last_loop_seek_position_ms: Some(1500),
            audio_clock_serial: 3,
            audio_segment_start_position_ns: Some(1_500_000_000),
            audio_segment_elapsed_ns: Some(250_000_000),
            audio_position_stale_count: 1,
            audio_sample_stale_count: 2,
            audio_master_clock_estimate_ns: Some(1_240_000_000),
            sampled_video_frame_count: 14,
            audio_position_query_count: 15,
            audio_position_query_hit_count: 12,
            audio_video_clock_drift_latest_ns: Some(-20_000),
            audio_video_master_clock_drift_latest_ns: Some(15_000),
            audio_video_master_clock_drift_abs_max_ns: Some(40_000),
        };

        let snapshot = native_vulkan_video_runtime_snapshot(
            &item,
            None,
            None,
            Some(audio_runtime),
            None,
            None,
            24,
            None,
        )
        .expect("video snapshot");

        assert_eq!(snapshot.audio_output_policy, "plan");
        assert_eq!(snapshot.audio_output_mode, "auto");
        assert_eq!(snapshot.audio_runtime_status, "clocked-playback-active");
        assert_eq!(snapshot.audio_runtime_provider, "gstreamer");
        assert!(snapshot.audio_runtime_reached_clocked_playback);
        assert_eq!(snapshot.audio_runtime_buffer_count, 18);
        assert_eq!(snapshot.audio_runtime_output_sink_count, 2);
        assert_eq!(snapshot.audio_runtime_loop_seek_count, 2);
        assert_eq!(snapshot.audio_runtime_loop_seek_error_count, 0);
        assert_eq!(snapshot.audio_runtime_loop_restart_count, 2);
        assert_eq!(
            snapshot.audio_runtime_last_loop_seek_position_ms,
            Some(1500)
        );
        assert_eq!(snapshot.audio_runtime_clock_serial, 3);
        assert_eq!(
            snapshot.audio_runtime_segment_start_position_ns,
            Some(1_500_000_000)
        );
        assert_eq!(snapshot.audio_runtime_segment_elapsed_ns, Some(250_000_000));
        assert_eq!(snapshot.audio_runtime_position_stale_count, 1);
        assert_eq!(snapshot.audio_runtime_sample_stale_count, 2);
        assert_eq!(
            snapshot.audio_runtime_master_clock_estimate_ns,
            Some(1_240_000_000)
        );
        assert_eq!(snapshot.audio_runtime_sampled_video_frame_count, 14);
        assert_eq!(snapshot.audio_runtime_position_query_count, 15);
        assert_eq!(snapshot.audio_runtime_position_query_hit_count, 12);
        assert_eq!(
            snapshot.audio_runtime_video_clock_drift_latest_ns,
            Some(-20_000)
        );
        assert_eq!(
            snapshot.audio_runtime_video_master_clock_drift_latest_ns,
            Some(15_000)
        );
        assert_eq!(
            snapshot.audio_runtime_video_master_clock_drift_abs_max_ns,
            Some(40_000)
        );
        assert_eq!(snapshot.audio_runtime_last_error, None);
    }

    #[test]
    fn video_runtime_snapshot_reports_active_appsink_frontend() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: true,
            manifest_max_fps: None,
            target_max_fps: Some(240),
            decoder_policy: VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let frontend = NativeVulkanVideoFrontendSnapshot {
            provider: NativeVulkanVideoFrontendProvider::Gstreamer,
            route: NativeVulkanVideoFrontendRoute::DecodedProvider,
            decode_owner: NativeVulkanVideoDecodeOwner::Gstreamer,
            memory_preference: NativeVulkanVideoFrontendMemoryPreference::Auto,
            sample_queue_policy: "keep-last",
            provider_state: Some("Playing".to_owned()),
            eos_messages: 0,
            segment_done_messages: 1,
            frames_received: 3,
            last_sample_caps: Some("video/x-raw, format=(string)NV12".to_owned()),
            last_sample_format: Some("NV12".to_owned()),
            last_sample_size: Some((3840, 2160)),
            last_sample_pts_ms: Some(8),
            last_sample_duration_ms: Some(4),
            last_sample_pts_delta_ms: Some(4),
            last_sample_memory_types: vec!["CUDAMemory".to_owned()],
            actual_decoders: vec!["nvh264dec".to_owned()],
            decoder_policy_status: Some("Satisfied".to_owned()),
            caps_report_count: 1,
            caps_memory_features: vec!["memory:CUDAMemory".to_owned()],
            caps_reports: vec![NativeVulkanVideoCapsSnapshot {
                element: "appsink0".to_owned(),
                pad: "sink".to_owned(),
                direction: "sink".to_owned(),
                caps: "video/x-raw(memory:CUDAMemory)".to_owned(),
                source: "current".to_owned(),
                memory_features: vec!["memory:CUDAMemory".to_owned()],
            }],
            last_error: None,
        };
        let import = NativeVulkanVideoImportSnapshot {
            texture_import_status: "importing-cuda-vulkan-image-planes",
            frames_imported: 2,
            last_import_size: Some((3840, 2160)),
            last_import_memory_path: Some(
                "CUDAMemory->CUDA->Vulkan external image planes".to_owned(),
            ),
            last_import_error: None,
            last_import_elapsed_us: Some(900),
            max_import_elapsed_us: Some(1200),
            last_dmabuf_import: None,
        };

        let snapshot = native_vulkan_video_runtime_snapshot(
            &item,
            Some(frontend),
            Some(import),
            None,
            None,
            None,
            12,
            None,
        )
        .unwrap();

        assert_eq!(snapshot.frontend, "gstreamer-appsink");
        assert_eq!(snapshot.frontend_provider, "gstreamer");
        assert_eq!(snapshot.frontend_route, "decoded-provider");
        assert_eq!(snapshot.frontend_decode_owner, "gstreamer");
        assert_eq!(snapshot.frontend_memory_preference, "auto");
        assert_eq!(snapshot.frontend_sample_queue_policy, "keep-last");
        assert_eq!(
            snapshot.audio_status,
            "muted-clock-only-audio-clock-pipeline"
        );
        assert_eq!(snapshot.audio_output_policy, "plan");
        assert_eq!(snapshot.audio_output_mode, "clock-only");
        assert_eq!(
            snapshot.audio_output_status,
            "clock-only-output-ready-for-audio-clock-runtime"
        );
        assert_eq!(
            snapshot.audio_runtime_status,
            "clock-only-output-ready-for-audio-clock-runtime"
        );
        assert_eq!(snapshot.audio_runtime_provider, "gstreamer");
        assert_eq!(snapshot.audio_runtime_loop_seek_count, 0);
        assert_eq!(snapshot.audio_runtime_loop_restart_count, 0);
        assert_eq!(snapshot.audio_runtime_position_stale_count, 0);
        assert_eq!(snapshot.audio_runtime_sample_stale_count, 0);
        assert_eq!(snapshot.audio_runtime_clock_serial, 0);
        assert_eq!(snapshot.audio_runtime_position_query_count, 0);
        assert_eq!(snapshot.audio_runtime_master_clock_estimate_ns, None);
        assert_eq!(snapshot.frontend_status, "appsink-receiving-samples");
        assert_eq!(snapshot.handoff_status, "appsink-sample-handoff-active");
        assert_eq!(snapshot.frames_received, 3);
        assert_eq!(snapshot.frames_imported, 2);
        assert_eq!(snapshot.segment_done_messages, 1);
        assert_eq!(snapshot.rendered_placeholder_frames, 10);
        assert_eq!(
            snapshot.texture_import_status,
            "importing-cuda-vulkan-image-planes"
        );
        assert_eq!(snapshot.last_import_size, Some((3840, 2160)));
        assert_eq!(
            snapshot.last_import_memory_path.as_deref(),
            Some("CUDAMemory->CUDA->Vulkan external image planes")
        );
        assert_eq!(snapshot.last_import_elapsed_us, Some(900));
        assert_eq!(snapshot.max_import_elapsed_us, Some(1200));
        assert_eq!(snapshot.last_dmabuf_import, None);
        assert_eq!(snapshot.memory_route.route, "cuda-vulkan-copy");
        assert!(!snapshot.memory_route.direct_candidate);
        assert!(!snapshot.memory_route.direct_import_confirmed);
        assert_eq!(snapshot.memory_route.copy_risk, "gpu-copy-or-sync-risk");
        assert_eq!(snapshot.last_sample_format.as_deref(), Some("NV12"));
        assert_eq!(snapshot.last_sample_pts_ms, Some(8));
        assert_eq!(snapshot.last_sample_duration_ms, Some(4));
        assert_eq!(snapshot.last_sample_pts_delta_ms, Some(4));
        assert_eq!(snapshot.last_sample_memory_types, vec!["CUDAMemory"]);
        assert_eq!(snapshot.actual_decoders, vec!["nvh264dec"]);
        assert_eq!(snapshot.decoder_policy_status.as_deref(), Some("Satisfied"));
        assert_eq!(snapshot.caps_memory_features, vec!["memory:CUDAMemory"]);
    }

    #[test]
    fn video_runtime_snapshot_reports_dmabuf_contract() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: true,
            manifest_max_fps: None,
            target_max_fps: Some(240),
            decoder_policy: VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let contract = NativeVulkanDmabufImportSnapshot {
            source: "GstDmaBufMemory->Vulkan external DRM modifier image planes".to_owned(),
            format: "NV12",
            drm_fourcc: DRM_FORMAT_NV12,
            drm_fourcc_hex: "0x3231564e".to_owned(),
            modifier: DRM_FORMAT_MOD_LINEAR,
            modifier_hex: "0x0000000000000000".to_owned(),
            available_plane_count: 2,
            drm_object_count: 1,
            y_uv_same_fd: true,
            driver_modifier_plane_count: Some(2),
            y_offset: 0,
            y_stride: 3840,
            uv_offset: 8294400,
            uv_stride: 3840,
            image_memory_type_bits: Some(0x0000_0080),
            image_memory_type_bits_hex: Some("0x00000080".to_owned()),
            fd_memory_type_bits: Some(0x0000_00c0),
            fd_memory_type_bits_hex: Some("0x000000c0".to_owned()),
            compatible_memory_type_bits: Some(0x0000_0080),
            compatible_memory_type_bits_hex: Some("0x00000080".to_owned()),
            selected_memory_type_index: Some(7),
            memory_allocation_size: Some(12_451_840),
        };
        let import = NativeVulkanVideoImportSnapshot {
            texture_import_status: "importing-dmabuf-vulkan-image",
            frames_imported: 1,
            last_import_size: Some((3840, 2160)),
            last_import_memory_path: Some(
                "GstDmaBufMemory->Vulkan external DRM modifier image planes".to_owned(),
            ),
            last_import_error: None,
            last_import_elapsed_us: Some(200),
            max_import_elapsed_us: Some(200),
            last_dmabuf_import: Some(contract.clone()),
        };

        let snapshot = native_vulkan_video_runtime_snapshot(
            &item,
            None,
            Some(import),
            None,
            None,
            None,
            1,
            None,
        )
        .expect("video snapshot");

        assert_eq!(snapshot.last_dmabuf_import, Some(contract));
        assert_eq!(snapshot.memory_route.route, "direct-dmabuf-import");
        assert!(snapshot.memory_route.direct_candidate);
        assert!(snapshot.memory_route.direct_import_confirmed);
        assert_eq!(
            snapshot.memory_route.copy_risk,
            "low-confirmed-external-memory-import"
        );
    }

    #[test]
    fn video_runtime_snapshot_separates_dmabuf_caps_from_confirmed_direct_import() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: true,
            manifest_max_fps: None,
            target_max_fps: Some(240),
            decoder_policy: VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let frontend = NativeVulkanVideoFrontendSnapshot {
            provider: NativeVulkanVideoFrontendProvider::Gstreamer,
            route: NativeVulkanVideoFrontendRoute::DecodedProvider,
            decode_owner: NativeVulkanVideoDecodeOwner::Gstreamer,
            memory_preference: NativeVulkanVideoFrontendMemoryPreference::DirectDmabuf,
            sample_queue_policy: "keep-last",
            provider_state: Some("Playing".to_owned()),
            eos_messages: 0,
            segment_done_messages: 0,
            frames_received: 1,
            last_sample_caps: Some("video/x-raw(memory:DMABuf), format=(string)NV12".to_owned()),
            last_sample_format: Some("NV12".to_owned()),
            last_sample_size: Some((1920, 1080)),
            last_sample_pts_ms: Some(0),
            last_sample_duration_ms: Some(16),
            last_sample_pts_delta_ms: None,
            last_sample_memory_types: vec!["DMABuf".to_owned()],
            actual_decoders: vec!["vah264dec".to_owned()],
            decoder_policy_status: Some("Satisfied".to_owned()),
            caps_report_count: 1,
            caps_memory_features: vec!["memory:DMABuf".to_owned()],
            caps_reports: vec![NativeVulkanVideoCapsSnapshot {
                element: "appsink0".to_owned(),
                pad: "sink".to_owned(),
                direction: "sink".to_owned(),
                caps: "video/x-raw(memory:DMABuf)".to_owned(),
                source: "current".to_owned(),
                memory_features: vec!["memory:DMABuf".to_owned()],
            }],
            last_error: None,
        };

        let snapshot = native_vulkan_video_runtime_snapshot(
            &item,
            Some(frontend),
            None,
            None,
            None,
            None,
            1,
            None,
        )
        .unwrap();

        assert_eq!(snapshot.memory_route.route, "dmabuf-caps-pending-import");
        assert_eq!(snapshot.frontend_route, "decoded-provider");
        assert_eq!(snapshot.frontend_decode_owner, "gstreamer");
        assert_eq!(snapshot.frontend_memory_preference, "direct-dmabuf");
        assert_eq!(snapshot.frontend_sample_queue_policy, "keep-last");
        assert!(snapshot.memory_route.direct_candidate);
        assert!(!snapshot.memory_route.direct_import_confirmed);
        assert_eq!(
            snapshot.memory_route.copy_risk,
            "unknown-until-import-contract"
        );
    }
}
