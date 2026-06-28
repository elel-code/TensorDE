#[cfg(feature = "native-vulkan-video")]
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::core::{SceneSystemStatus, SceneTextureRegion};
use crate::renderer::{SceneRenderAudioCue, SceneWallpaperPlan, SceneWallpaperRuntimeSampler};

use super::super::audio::clock::{
    NativeVulkanAudioClockProbeOptions, NativeVulkanAudioClockRuntimeSnapshot,
    native_vulkan_probe_ffmpeg_audio_clock,
};
use super::super::present::render_item::native_vulkan_scene_item;
use super::super::present::render_plan::{
    native_vulkan_clear_color_from_hex, native_vulkan_render_item_clear_color,
};
#[cfg(feature = "native-vulkan-video")]
use super::super::video::direct::{
    NATIVE_VULKAN_AUDIO_OUTPUT_WORKER_STACK_BYTES, native_vulkan_audio_runtime_packet_budget,
};
use super::super::{
    NativeVulkanAudioOutputMode, NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVideoSessionCodec, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, run_clear,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_solid_quad_present,
};
#[cfg(feature = "native-vulkan-video")]
use super::super::{
    NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot, run_vulkanalia_ready_prefix_video,
};
use super::runtime::{NativeVulkanSceneRuntimeSnapshot, native_vulkan_scene_runtime_snapshot};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanSceneAudioCueRuntimeSnapshot {
    pub route: &'static str,
    pub boundary: &'static str,
    pub cue_index: usize,
    pub layer_id: String,
    pub source: std::path::PathBuf,
    pub playback_mode: Option<String>,
    pub start_silent: bool,
    pub runtime: NativeVulkanAudioClockRuntimeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "scene_present_route",
    content = "snapshot",
    rename_all = "kebab-case"
)]
pub enum NativeVulkanScenePresentSnapshot {
    Clear {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        scene_audio: Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>,
        present: NativeVulkanVulkanaliaClearPresentSnapshot,
    },
    SolidQuad {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        scene_audio: Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>,
        present: NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot,
    },
    SampledImage {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        scene_audio: Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>,
        present: NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    },
    #[cfg(feature = "native-vulkan-video")]
    Video {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        scene_audio: Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>,
        present: NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanScenePresentRouteKind {
    Clear,
    SolidQuad,
    SampledImage,
    #[cfg(feature = "native-vulkan-video")]
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoBridgeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub bitstream_extract_max_samples: u32,
    pub ready_prefix_frames: u32,
    pub playback_frames: u32,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
}

fn native_vulkan_scene_dynamic_sampler(
    plan: &SceneWallpaperPlan,
) -> Result<Option<SceneWallpaperRuntimeSampler>, NativeVulkanError> {
    if !native_vulkan_scene_plan_needs_dynamic_sampler(plan) {
        return Ok(None);
    }
    SceneWallpaperRuntimeSampler::from_plan(plan)
        .map_err(|err| NativeVulkanError::Scene(format!("prepare dynamic scene sampler: {err}")))
}

fn native_vulkan_scene_plan_needs_dynamic_sampler(plan: &SceneWallpaperPlan) -> bool {
    let particle_runtime_active = matches!(
        plan.scene_systems.particles,
        SceneSystemStatus::Detected | SceneSystemStatus::Ready
    );
    plan.timeline_animation_count > 0
        || plan.timeline_animated_layer_count > 0
        || particle_runtime_active
        || plan.layers.iter().any(|layer| {
            layer
                .texture_region
                .is_some_and(native_vulkan_scene_texture_region_is_animated)
        })
}

fn native_vulkan_scene_texture_region_is_animated(region: SceneTextureRegion) -> bool {
    region.frame_count > 1 && region.fps.is_some_and(|fps| fps.is_finite() && fps > 0.0)
}

fn native_vulkan_scene_dynamic_solid_geometry(
    plan: &SceneWallpaperPlan,
) -> Result<Option<NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry>, NativeVulkanError> {
    let Some(sampler) = native_vulkan_scene_dynamic_sampler(plan)? else {
        return Ok(None);
    };
    Ok(Some(
        native_vulkan_scene_dynamic_solid_geometry_from_sampler(sampler, plan.snapshot_time_ms),
    ))
}

fn native_vulkan_scene_dynamic_solid_geometry_from_sampler(
    sampler: SceneWallpaperRuntimeSampler,
    base_time_ms: u64,
) -> NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry {
    Box::new(move |elapsed_ms| {
        let sampled_plan = sampler
            .sample_plan(base_time_ms.saturating_add(elapsed_ms))
            .map_err(|err| format!("sample dynamic solid scene: {err}"))?;
        let render_item = native_vulkan_scene_item(&sampled_plan);
        let runtime = native_vulkan_scene_runtime_snapshot(&render_item)
            .ok_or_else(|| "dynamic solid scene runtime snapshot is unavailable".to_owned())?;
        runtime
            .vulkanalia_solid_quad_geometry_input()
            .ok_or_else(|| {
                format!(
                    "dynamic scene is not solid-quad recordable: {}",
                    runtime.draw_pass_backend_status
                )
            })
    })
}

fn native_vulkan_scene_dynamic_sampled_geometry_from_sampler(
    sampler: SceneWallpaperRuntimeSampler,
    base_time_ms: u64,
) -> NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry {
    Box::new(move |elapsed_ms| {
        let sampled_plan = sampler
            .sample_plan(base_time_ms.saturating_add(elapsed_ms))
            .map_err(|err| format!("sample dynamic sampled-image scene: {err}"))?;
        let render_item = native_vulkan_scene_item(&sampled_plan);
        let runtime = native_vulkan_scene_runtime_snapshot(&render_item).ok_or_else(|| {
            "dynamic sampled-image scene runtime snapshot is unavailable".to_owned()
        })?;
        runtime
            .vulkanalia_sampled_image_geometry_input()
            .map(|(_, geometry)| geometry)
            .ok_or_else(|| {
                format!(
                    "dynamic scene is not sampled-image recordable: {}",
                    runtime.draw_pass_backend_status
                )
            })
    })
}

fn native_vulkan_scene_dynamic_mixed_solid_geometry_from_sampler(
    sampler: SceneWallpaperRuntimeSampler,
    base_time_ms: u64,
) -> NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry {
    Box::new(move |elapsed_ms| {
        let sampled_plan = sampler
            .sample_plan(base_time_ms.saturating_add(elapsed_ms))
            .map_err(|err| format!("sample dynamic mixed solid scene: {err}"))?;
        let render_item = native_vulkan_scene_item(&sampled_plan);
        let runtime = native_vulkan_scene_runtime_snapshot(&render_item).ok_or_else(|| {
            "dynamic mixed solid scene runtime snapshot is unavailable".to_owned()
        })?;
        Ok(runtime.vulkanalia_mixed_solid_quad_geometry_input())
    })
}

fn native_vulkan_scene_dynamic_sampled_geometry_pair(
    plan: &SceneWallpaperPlan,
) -> Result<
    (
        Option<NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry>,
        Option<NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry>,
    ),
    NativeVulkanError,
> {
    let Some(sampler) = native_vulkan_scene_dynamic_sampler(plan)? else {
        return Ok((None, None));
    };
    let base_time_ms = plan.snapshot_time_ms;
    Ok((
        Some(
            native_vulkan_scene_dynamic_mixed_solid_geometry_from_sampler(
                sampler.clone(),
                base_time_ms,
            ),
        ),
        Some(native_vulkan_scene_dynamic_sampled_geometry_from_sampler(
            sampler,
            base_time_ms,
        )),
    ))
}

pub fn run_scene(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: SceneWallpaperPlan,
    scene_audio_output_mode: NativeVulkanAudioOutputMode,
    video_bridge: Option<NativeVulkanSceneVideoBridgeOptions>,
) -> Result<NativeVulkanScenePresentSnapshot, NativeVulkanError> {
    #[cfg(not(feature = "native-vulkan-video"))]
    let _ = video_bridge;

    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps = options.target_max_fps.or(plan.target_max_fps);
    options.target_max_fps = target_max_fps;
    let render_item = native_vulkan_scene_item(&plan);
    options.clear_color = native_vulkan_render_item_clear_color(&render_item, options.clear_color);
    let runtime = native_vulkan_scene_runtime_snapshot(&render_item).ok_or_else(|| {
        NativeVulkanError::Scene("scene runtime snapshot is unavailable".to_owned())
    })?;
    if let Some(color) = runtime
        .draw_pass_background_clear_color
        .as_deref()
        .and_then(native_vulkan_clear_color_from_hex)
    {
        options.clear_color = color;
    }
    match native_vulkan_scene_present_route(&runtime)? {
        NativeVulkanScenePresentRouteKind::Clear => {
            let color = runtime
                .draw_pass_fast_clear_color
                .as_deref()
                .and_then(native_vulkan_clear_color_from_hex)
                .ok_or_else(|| {
                    NativeVulkanError::Scene(
                        "scene fast-clear draw plan has no valid #rrggbb color".to_owned(),
                    )
                })?;
            options.clear_color = color;
            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                &plan,
                duration,
                scene_audio_output_mode,
                || run_clear(options, duration),
            )?;
            Ok(NativeVulkanScenePresentSnapshot::Clear {
                runtime,
                scene_audio,
                present,
            })
        }
        NativeVulkanScenePresentRouteKind::SolidQuad => {
            let geometry = runtime
                .vulkanalia_solid_quad_geometry_input()
                .ok_or_else(|| {
                    NativeVulkanError::Scene(format!(
                        "scene draw plan is not solid-quad recordable: {}",
                        runtime.draw_pass_backend_status
                    ))
                })?;
            let dynamic_geometry = native_vulkan_scene_dynamic_solid_geometry(&plan)?;

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                &plan,
                duration,
                scene_audio_output_mode,
                || {
                    run_native_vulkan_vulkanalia_scene_solid_quad_present(
                        NativeVulkanVulkanaliaSceneSolidQuadPresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            duration,
                            target_max_fps,
                            quad_color: options.clear_color,
                            geometry: Some(geometry),
                            dynamic_geometry,
                            scene_size: runtime.scene_size,
                            scene_fit: runtime.scene_fit,
                        },
                    )
                    .map_err(NativeVulkanError::Scene)
                },
            )?;
            Ok(NativeVulkanScenePresentSnapshot::SolidQuad {
                runtime,
                scene_audio,
                present,
            })
        }
        NativeVulkanScenePresentRouteKind::SampledImage => {
            let (source, fit, geometry) = if let Some((source, geometry)) =
                runtime.vulkanalia_sampled_image_geometry_input()
            {
                (source, None, Some(geometry))
            } else if let Some((source, fit)) =
                runtime.vulkanalia_sampled_image_implicit_full_extent_input()
            {
                (source, Some(fit), None)
            } else {
                return Err(NativeVulkanError::Scene(format!(
                    "scene draw plan is not sampled-image recordable: {}",
                    runtime.draw_pass_backend_status
                )));
            };
            let solid_geometry = runtime.vulkanalia_mixed_solid_quad_geometry_input();
            let (dynamic_solid_geometry, dynamic_geometry) =
                native_vulkan_scene_dynamic_sampled_geometry_pair(&plan)?;

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                &plan,
                duration,
                scene_audio_output_mode,
                || {
                    run_native_vulkan_vulkanalia_scene_sampled_image_present(
                        NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            duration,
                            target_max_fps,
                            source,
                            clear_color: options.clear_color,
                            fit,
                            solid_geometry,
                            geometry,
                            dynamic_solid_geometry,
                            dynamic_geometry,
                            scene_size: runtime.scene_size,
                            scene_fit: runtime.scene_fit,
                        },
                    )
                    .map_err(NativeVulkanError::Scene)
                },
            )?;
            Ok(NativeVulkanScenePresentSnapshot::SampledImage {
                runtime,
                scene_audio,
                present,
            })
        }
        #[cfg(feature = "native-vulkan-video")]
        NativeVulkanScenePresentRouteKind::Video => {
            let video_bridge = video_bridge.ok_or_else(|| {
                NativeVulkanError::Scene(
                    "scene video layer requires Vulkan Video bridge options".to_owned(),
                )
            })?;
            let video = runtime
                .draw_ops
                .iter()
                .find(|op| op.kind == "video")
                .ok_or_else(|| {
                    NativeVulkanError::Scene("scene video route has no video draw op".to_owned())
                })?;
            let source = video.source.clone().ok_or_else(|| {
                NativeVulkanError::Scene("scene video route has no video source".to_owned())
            })?;
            let width = native_vulkan_scene_video_extent(video_bridge.width, video.width);
            let height = native_vulkan_scene_video_extent(video_bridge.height, video.height);

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                &plan,
                duration,
                scene_audio_output_mode,
                || {
                    run_vulkanalia_ready_prefix_video(
                        options,
                        video_bridge.codec,
                        source,
                        width,
                        height,
                        video.fit,
                        video_bridge.bitstream_extract_max_samples,
                        video_bridge.ready_prefix_frames,
                        video_bridge.playback_frames,
                        video_bridge.audio_clock_probe_requested,
                        video_bridge.audio_output_mode,
                    )
                },
            )?;
            Ok(NativeVulkanScenePresentSnapshot::Video {
                runtime,
                scene_audio,
                present,
            })
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
type NativeVulkanSceneAudioWorker =
    JoinHandle<Result<NativeVulkanSceneAudioCueRuntimeSnapshot, NativeVulkanError>>;

#[cfg(not(feature = "native-vulkan-video"))]
struct NativeVulkanSceneAudioWorker;

#[derive(Debug, Clone, PartialEq)]
struct NativeVulkanSceneAudioCuePlayback {
    cue_index: usize,
    layer_id: String,
    cue: SceneRenderAudioCue,
}

fn native_vulkan_scene_present_with_audio<T>(
    plan: &SceneWallpaperPlan,
    duration: Duration,
    output_mode: NativeVulkanAudioOutputMode,
    present: impl FnOnce() -> Result<T, NativeVulkanError>,
) -> Result<(T, Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>), NativeVulkanError> {
    let audio_workers = native_vulkan_scene_start_audio_workers(plan, duration, output_mode)?;
    let present_result = present();
    let audio_result = native_vulkan_scene_join_audio_workers(audio_workers);
    match (present_result, audio_result) {
        (Ok(present), Ok(audio)) => Ok((present, audio)),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_start_audio_workers(
    plan: &SceneWallpaperPlan,
    duration: Duration,
    output_mode: NativeVulkanAudioOutputMode,
) -> Result<Vec<NativeVulkanSceneAudioWorker>, NativeVulkanError> {
    native_vulkan_scene_active_audio_cues(plan)
        .into_iter()
        .map(|playback| {
            if !playback.cue.source.is_file() {
                return Err(NativeVulkanError::Scene(format!(
                    "scene audio cue source does not exist: {}",
                    playback.cue.source.display()
                )));
            }
            let target_playback_clock_ns = Some(native_vulkan_scene_duration_ns(duration).max(1));
            let playback_frame_count =
                native_vulkan_scene_audio_playback_frame_count(duration, plan.target_max_fps);
            let packets_to_probe =
                native_vulkan_audio_runtime_packet_budget(duration, playback_frame_count);
            thread::Builder::new()
                .name(format!("gilder-scene-audio-{}", playback.cue_index))
                .stack_size(NATIVE_VULKAN_AUDIO_OUTPUT_WORKER_STACK_BYTES)
                .spawn(move || {
                    let mut options =
                        NativeVulkanAudioClockProbeOptions::clock_only(playback.cue.source.clone());
                    options.output_mode = output_mode;
                    options.queue_capacity =
                        super::super::audio::clock::NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS;
                    options.packets_to_probe = packets_to_probe;
                    options.loop_on_eos = native_vulkan_scene_audio_loop_on_eos(&playback.cue);
                    options.target_playback_clock_ns = target_playback_clock_ns;
                    let runtime = native_vulkan_probe_ffmpeg_audio_clock(options)?;
                    native_vulkan_scene_audio_validate_runtime(&playback, output_mode, &runtime)?;
                    Ok(NativeVulkanSceneAudioCueRuntimeSnapshot {
                        route: "native-vulkan-scene-audio-cue-runtime",
                        boundary: "gscene audio cue -> FFmpeg audio decode -> PipeWire-only output",
                        cue_index: playback.cue_index,
                        layer_id: playback.layer_id,
                        source: playback.cue.source,
                        playback_mode: playback.cue.playback_mode,
                        start_silent: playback.cue.start_silent,
                        runtime,
                    })
                })
                .map_err(|err| {
                    NativeVulkanError::Scene(format!(
                        "spawn PipeWire scene audio output worker: {err}"
                    ))
                })
        })
        .collect()
}

#[cfg(not(feature = "native-vulkan-video"))]
fn native_vulkan_scene_start_audio_workers(
    plan: &SceneWallpaperPlan,
    _duration: Duration,
    _output_mode: NativeVulkanAudioOutputMode,
) -> Result<Vec<NativeVulkanSceneAudioWorker>, NativeVulkanError> {
    if native_vulkan_scene_active_audio_cues(plan).is_empty() {
        Ok(Vec::new())
    } else {
        Err(NativeVulkanError::Scene(
            "scene audio cues require native-vulkan-video FFmpeg/PipeWire runtime".to_owned(),
        ))
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_join_audio_workers(
    workers: Vec<NativeVulkanSceneAudioWorker>,
) -> Result<Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>, NativeVulkanError> {
    workers
        .into_iter()
        .map(|worker| match worker.join() {
            Ok(result) => result,
            Err(_) => Err(NativeVulkanError::Scene(
                "scene audio output worker panicked".to_owned(),
            )),
        })
        .collect()
}

#[cfg(not(feature = "native-vulkan-video"))]
fn native_vulkan_scene_join_audio_workers(
    workers: Vec<NativeVulkanSceneAudioWorker>,
) -> Result<Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>, NativeVulkanError> {
    let _ = workers;
    Ok(Vec::new())
}

fn native_vulkan_scene_active_audio_cues(
    plan: &SceneWallpaperPlan,
) -> Vec<NativeVulkanSceneAudioCuePlayback> {
    plan.layers
        .iter()
        .flat_map(|layer| {
            layer
                .audio
                .iter()
                .enumerate()
                .filter(|(_, cue)| !cue.start_silent)
                .map(|(cue_index, cue)| NativeVulkanSceneAudioCuePlayback {
                    cue_index,
                    layer_id: layer.id.clone(),
                    cue: cue.clone(),
                })
        })
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_audio_validate_runtime(
    playback: &NativeVulkanSceneAudioCuePlayback,
    output_mode: NativeVulkanAudioOutputMode,
    runtime: &NativeVulkanAudioClockRuntimeSnapshot,
) -> Result<(), NativeVulkanError> {
    if !runtime.audio_stream_found {
        return Err(NativeVulkanError::Scene(format!(
            "scene audio cue {:?} did not open an FFmpeg audio stream: {}",
            playback.cue.source,
            runtime
                .audio_stream_error
                .as_deref()
                .unwrap_or("missing audio stream")
        )));
    }
    if native_vulkan_scene_audio_loop_on_eos(&playback.cue) && !runtime.playback_target_reached {
        return Err(NativeVulkanError::Scene(format!(
            "scene audio cue {:?} did not cover requested playback duration",
            playback.cue.source
        )));
    }
    match output_mode {
        NativeVulkanAudioOutputMode::Auto => {
            if runtime.audible_output_started
                && runtime.audio_output_backend == "pipewire-s16le"
                && runtime.audio_output_xrun_count == 0
                && runtime.audio_output_stream_ready
            {
                Ok(())
            } else {
                Err(NativeVulkanError::Scene(format!(
                    "scene audio cue {:?} did not start clean PipeWire output",
                    playback.cue.source
                )))
            }
        }
        NativeVulkanAudioOutputMode::ClockOnly => Ok(()),
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_audio_loop_on_eos(cue: &SceneRenderAudioCue) -> bool {
    cue.playback_mode.as_deref() == Some("loop")
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_scene_audio_playback_frame_count(
    duration: Duration,
    target_max_fps: Option<u32>,
) -> u32 {
    let fps = u128::from(target_max_fps.unwrap_or(60).max(1));
    let frames = duration
        .as_nanos()
        .saturating_mul(fps)
        .saturating_add(999_999_999)
        / 1_000_000_000;
    u32::try_from(frames.min(u128::from(u32::MAX)))
        .unwrap_or(u32::MAX)
        .max(1)
}

fn native_vulkan_scene_present_route(
    runtime: &NativeVulkanSceneRuntimeSnapshot,
) -> Result<NativeVulkanScenePresentRouteKind, NativeVulkanError> {
    if !runtime.draw_pass_backend_ready {
        return Err(NativeVulkanError::Scene(format!(
            "scene draw plan is not presentable by the native Vulkan scene backend: {}; draw_ops={}, unsupported_layers={}, clear_background_ops={}, sampled_image_ops={}, sampled_image_steps={}, sampled_image_recording_ready={}, sampled_image_implicit_full_extent_ready={}, quad_steps={}",
            runtime.draw_pass_backend_status,
            runtime.draw_op_count,
            runtime.unsupported_layer_count,
            runtime.draw_pass_clear_background_op_count,
            runtime.draw_pass_sampled_image_op_count,
            runtime.draw_pass_sampled_image_recording_step_count,
            runtime.draw_pass_sampled_image_recording_ready,
            runtime.draw_pass_sampled_image_implicit_full_extent_ready,
            runtime.draw_pass_quad_recording_step_count
        )));
    }

    match runtime.draw_pass_backend_status {
        "fast-clear-color-ready" => Ok(NativeVulkanScenePresentRouteKind::Clear),
        "solid-quad-recording-ready" | "clear-background-solid-quad-recording-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::SolidQuad)
        }
        "sampled-image-recording-ready"
        | "clear-background-sampled-image-recording-ready"
        | "sampled-image-implicit-full-extent-ready"
        | "clear-background-sampled-image-implicit-full-extent-ready"
        | "mixed-quad-sampled-image-implicit-full-extent-ready"
        | "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready"
        | "clear-background-mixed-quad-sampled-image-recording-ready"
        | "mixed-quad-sampled-image-recording-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::SampledImage)
        }
        #[cfg(feature = "native-vulkan-video")]
        "video-layer-vulkan-video-scene-bridge-ready"
        | "clear-background-video-layer-vulkan-video-scene-bridge-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::Video)
        }
        status => Err(NativeVulkanError::Scene(format!(
            "scene draw plan has no native Vulkan present route: {status}"
        ))),
    }
}

fn native_vulkan_scene_video_extent(option_extent: u32, layer_extent: Option<f64>) -> u32 {
    if option_extent > 0 {
        return option_extent;
    }
    layer_extent
        .filter(|extent| extent.is_finite() && *extent > 0.0)
        .map(|extent| extent.round().clamp(1.0, f64::from(u32::MAX)) as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{FitMode, SceneNodeKind, SceneSystems, SceneTextureRegion, SceneTransform};
    use crate::renderer::{SceneDisplayPlan, SceneRenderAudioCue, SceneRenderLayer};
    use std::path::PathBuf;

    fn layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_region: None,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            text: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }

    fn plan(layers: Vec<SceneRenderLayer>) -> SceneWallpaperPlan {
        SceneWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: None,
            manifest_max_fps: None,
            target_max_fps: Some(60),
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            display: Some(SceneDisplayPlan::Color {
                color: "#000000".to_owned(),
            }),
            layers,
        }
    }

    fn route_for_layers(
        layers: Vec<SceneRenderLayer>,
    ) -> Result<NativeVulkanScenePresentRouteKind, NativeVulkanError> {
        let render_item = native_vulkan_scene_item(&plan(layers));
        let runtime = native_vulkan_scene_runtime_snapshot(&render_item).expect("runtime snapshot");
        native_vulkan_scene_present_route(&runtime)
    }

    #[test]
    fn scene_main_present_route_selects_fast_clear() {
        let mut color = layer("background", SceneNodeKind::Color);
        color.color = Some("#102030".to_owned());

        assert_eq!(
            route_for_layers(vec![color]).unwrap(),
            NativeVulkanScenePresentRouteKind::Clear
        );
    }

    #[test]
    fn scene_main_present_route_selects_solid_quad() {
        let mut rectangle = layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle]).unwrap(),
            NativeVulkanScenePresentRouteKind::SolidQuad
        );
    }

    #[test]
    fn scene_main_present_route_selects_sampled_image_for_image_and_mixed_scenes() {
        let mut image = layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));

        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        image.width = Some(640.0);
        image.height = Some(360.0);
        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        let mut rectangle = layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#203040".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle, image]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        let mut background = layer("background", SceneNodeKind::Image);
        background.source = Some(PathBuf::from("/tmp/background.png"));
        let mut overlay = layer("overlay", SceneNodeKind::Rectangle);
        overlay.color = Some("#ffffff".to_owned());
        overlay.width = Some(64.0);
        overlay.height = Some(64.0);

        assert_eq!(
            route_for_layers(vec![background, overlay]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );
    }

    #[test]
    fn animated_texture_regions_enable_dynamic_scene_sampling() {
        let mut image = layer("atlas", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/atlas.gtex"));
        image.texture_region = Some(SceneTextureRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.25,
            v_max: 0.25,
            frame_index: 0,
            frame_count: 12,
            columns: 4,
            rows: 3,
            fps: Some(12.0),
            loop_playback: true,
        });

        let plan = plan(vec![image]);

        assert!(native_vulkan_scene_plan_needs_dynamic_sampler(&plan));
    }

    #[test]
    fn scene_audio_runtime_uses_only_active_cues() {
        let mut image = layer("speaker", SceneNodeKind::Image);
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/theme.ogg"),
            playback_mode: Some("loop".to_owned()),
            volume: None,
            start_silent: false,
        });
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/response.ogg"),
            playback_mode: None,
            volume: None,
            start_silent: true,
        });
        let plan = plan(vec![image]);

        let active = native_vulkan_scene_active_audio_cues(&plan);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].layer_id, "speaker");
        assert_eq!(active[0].cue.source, PathBuf::from("/tmp/theme.ogg"));
        #[cfg(feature = "native-vulkan-video")]
        assert!(native_vulkan_scene_audio_loop_on_eos(&active[0].cue));
    }
}
