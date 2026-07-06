use std::collections::BTreeSet;
#[cfg(feature = "native-vulkan-video")]
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::core::SceneSystemStatus;
use crate::engine::scene::{
    SceneLayerPose, SceneLayerPoseTimeline, retained_layer_pose_timeline_from_frames,
};
use crate::renderer::{
    SceneBinaryLayerGpuPosePayloads, SceneBinaryParticleGpuPayload,
    SceneBinaryParticleGpuVertexPayload, SceneBinaryPuppetGpuPayload,
    SceneBinaryPuppetGpuPosePayload, SceneBinaryPuppetGpuVertexPayload,
    SceneBinaryRetainedGpuPayloads, SceneBinaryRuntimeSampler,
    SceneBinarySampledLayerGpuPosePayload, SceneRenderAudioCue, SceneWallpaperPlan,
};

#[cfg(feature = "native-vulkan-video")]
use super::super::NativeVulkanVulkanaliaSceneVideoOverlayInput;
use super::super::audio::clock::NativeVulkanAudioClockRuntimeSnapshot;
#[cfg(feature = "native-vulkan-video")]
use super::super::audio::clock::{
    NativeVulkanAudioClockProbeOptions, native_vulkan_probe_ffmpeg_audio_clock,
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
    NativeVulkanVulkanaliaSceneParticleGpuPayload,
    NativeVulkanVulkanaliaSceneParticleGpuVertexPayload,
    NativeVulkanVulkanaliaScenePuppetGpuPayload, NativeVulkanVulkanaliaScenePuppetGpuPosePayload,
    NativeVulkanVulkanaliaScenePuppetGpuVertexPayload,
    NativeVulkanVulkanaliaSceneSampledImageDrawStep,
    NativeVulkanVulkanaliaSceneSampledImageLayerPosePayload,
    NativeVulkanVulkanaliaSceneSampledImageLayerPoseTimelinePayload,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadLayerPosePayload,
    NativeVulkanVulkanaliaSceneSolidQuadLayerPoseTimelinePayload,
    NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot,
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator,
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap, run_clear,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_solid_quad_present,
};
#[cfg(feature = "native-vulkan-video")]
use super::super::{
    NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions,
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot,
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions,
    run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present,
};
use super::runtime::{NativeVulkanSceneRuntimeSnapshot, native_vulkan_scene_runtime_snapshot};

const SCENE_RETAINED_LAYER_POSE_SAMPLE_RATE_HZ: u64 = 60;
const SCENE_RETAINED_LAYER_POSE_GPU_STRIDE_BYTES: u64 = 48;

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
        present: NativeVulkanSceneVideoPresentRuntimeSnapshot,
    },
}

#[cfg(feature = "native-vulkan-video")]
pub type NativeVulkanSceneVideoPresentRuntimeSnapshot =
    NativeVulkanFfmpegVulkanHwSceneVideoPresentSnapshot;

#[cfg(not(feature = "native-vulkan-video"))]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum NativeVulkanSceneVideoPresentRuntimeSnapshot {}

pub fn native_vulkan_scene_runtime_snapshot_from_plan(
    plan: &SceneWallpaperPlan,
) -> Result<NativeVulkanSceneRuntimeSnapshot, NativeVulkanError> {
    let render_item = native_vulkan_scene_item(plan);
    native_vulkan_scene_runtime_snapshot(&render_item)
        .ok_or_else(|| NativeVulkanError::Scene("scene runtime snapshot is unavailable".to_owned()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanScenePresentRouteKind {
    Clear,
    SolidQuad,
    SampledImage,
    #[cfg(feature = "native-vulkan-video")]
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoPresentSourceOptions {
    pub source: std::path::PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub playback_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoPresentOptions {
    pub sources: Vec<NativeVulkanSceneVideoPresentSourceOptions>,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
}

fn native_vulkan_scene_binary_runtime_sampler(
    plan: &SceneWallpaperPlan,
    context: &'static str,
) -> Result<Option<SceneBinaryRuntimeSampler>, NativeVulkanError> {
    if !native_vulkan_scene_plan_uses_binary_scene(plan) {
        return Ok(None);
    }
    SceneBinaryRuntimeSampler::from_plan(plan)
        .map_err(|err| NativeVulkanError::Scene(format!("{context}: {err}")))
}

fn native_vulkan_scene_retained_binary_runtime_sampler(
    plan: &SceneWallpaperPlan,
    include_sampled_geometry: bool,
    include_solid_geometry: bool,
) -> Result<Option<SceneBinaryRuntimeSampler>, NativeVulkanError> {
    if !native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(
        plan,
        include_sampled_geometry,
        include_solid_geometry,
    ) {
        return Ok(None);
    }
    native_vulkan_scene_binary_runtime_sampler(
        plan,
        "prepare retained binary scene runtime sampler",
    )
}

fn native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(
    plan: &SceneWallpaperPlan,
    include_sampled_geometry: bool,
    include_solid_geometry: bool,
) -> bool {
    native_vulkan_scene_plan_uses_binary_scene(plan)
        && ((include_solid_geometry
            && native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(plan))
            || (include_sampled_geometry
                && (native_vulkan_scene_plan_needs_binary_sampled_layer_pose_sampler(plan)
                    || matches!(
                        plan.scene_systems.particles,
                        SceneSystemStatus::Detected | SceneSystemStatus::Ready
                    ))))
}

fn native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(plan: &SceneWallpaperPlan) -> bool {
    plan.timeline_animation_count > 0
        || plan.timeline_animated_layer_count > 0
        || plan.property_binding_count > 0
}

fn native_vulkan_scene_plan_needs_binary_sampled_layer_pose_sampler(
    plan: &SceneWallpaperPlan,
) -> bool {
    plan.timeline_animation_count > 0
        || plan.timeline_animated_layer_count > 0
        || plan.puppet_animation_layer_count > 0
        || plan.property_binding_count > 0
}

fn native_vulkan_scene_plan_uses_binary_scene(plan: &SceneWallpaperPlan) -> bool {
    plan.source
        .as_deref()
        .and_then(std::path::Path::extension)
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gscn"))
}

#[derive(Default)]
struct NativeVulkanSceneRetainedLayerGpuPosePayloads {
    sampled_layer_poses: Vec<NativeVulkanVulkanaliaSceneSampledImageLayerPosePayload>,
    sampled_layer_pose_timeline:
        Option<NativeVulkanVulkanaliaSceneSampledImageLayerPoseTimelinePayload>,
    solid_layer_poses: Vec<NativeVulkanVulkanaliaSceneSolidQuadLayerPosePayload>,
    solid_layer_pose_timeline: Option<NativeVulkanVulkanaliaSceneSolidQuadLayerPoseTimelinePayload>,
}

fn native_vulkan_scene_apply_retained_layer_pose_telemetry(
    runtime: &mut NativeVulkanSceneRuntimeSnapshot,
    retained_layer_poses: &NativeVulkanSceneRetainedLayerGpuPosePayloads,
) {
    let sampled_timeline = retained_layer_poses.sampled_layer_pose_timeline.as_ref();
    let solid_timeline = retained_layer_poses.solid_layer_pose_timeline.as_ref();
    let timeline_bytes = sampled_timeline
        .map(|timeline| {
            timeline
                .poses
                .len()
                .saturating_mul(SCENE_RETAINED_LAYER_POSE_GPU_STRIDE_BYTES as usize)
        })
        .unwrap_or(0)
        .saturating_add(
            solid_timeline
                .map(|timeline| {
                    timeline
                        .poses
                        .len()
                        .saturating_mul(SCENE_RETAINED_LAYER_POSE_GPU_STRIDE_BYTES as usize)
                })
                .unwrap_or(0),
        );
    let timeline_layers = sampled_timeline
        .map(|timeline| timeline.layer_indices.len())
        .unwrap_or(0)
        .saturating_add(
            solid_timeline
                .map(|timeline| timeline.layer_indices.len())
                .unwrap_or(0),
        );
    let timeline_frames = sampled_timeline
        .map(|timeline| timeline.frame_count)
        .into_iter()
        .chain(solid_timeline.map(|timeline| timeline.frame_count))
        .max()
        .unwrap_or(0);
    runtime.engine_telemetry.retained_layer_pose_timeline_bytes =
        timeline_bytes.min(u64::MAX as usize) as u64;
    runtime.engine_telemetry.retained_layer_pose_timeline_layers =
        timeline_layers.min(u32::MAX as usize) as u32;
    runtime.engine_telemetry.retained_layer_pose_timeline_frames = timeline_frames;
    if timeline_bytes > 0 {
        runtime.engine_telemetry.retained_layer_pose_timeline_model =
            "engine-packed-layer-major-pose-timeline";
    }
}

fn native_vulkan_scene_retained_layer_gpu_pose_payloads_from_sampler(
    plan: &SceneWallpaperPlan,
    duration: Duration,
    sampler: Option<&mut SceneBinaryRuntimeSampler>,
    include_sampled_geometry: bool,
    include_solid_geometry: bool,
) -> Result<NativeVulkanSceneRetainedLayerGpuPosePayloads, NativeVulkanError> {
    let wants_sampled = include_sampled_geometry
        && native_vulkan_scene_plan_uses_binary_scene(plan)
        && native_vulkan_scene_plan_needs_binary_sampled_layer_pose_sampler(plan);
    let wants_solid = include_solid_geometry
        && native_vulkan_scene_plan_uses_binary_scene(plan)
        && native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(plan);
    if !wants_sampled && !wants_solid {
        return Ok(NativeVulkanSceneRetainedLayerGpuPosePayloads::default());
    }
    let Some(sampler) = sampler else {
        return Ok(NativeVulkanSceneRetainedLayerGpuPosePayloads::default());
    };
    let base_time_ms = plan.snapshot_time_ms;
    let duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
    let frame_count = duration_ms
        .saturating_mul(SCENE_RETAINED_LAYER_POSE_SAMPLE_RATE_HZ)
        .saturating_add(999)
        / 1_000
        + 2;
    let retained_frame_capacity = frame_count.min(usize::MAX as u64) as usize;
    let mut sampled_frames = Vec::<Vec<SceneLayerPose>>::with_capacity(if wants_sampled {
        retained_frame_capacity
    } else {
        0
    });
    let mut solid_frames = Vec::<Vec<SceneLayerPose>>::with_capacity(if wants_solid {
        retained_frame_capacity
    } else {
        0
    });
    for frame_index in 0..frame_count {
        let sample_time_ms = base_time_ms.saturating_add(
            frame_index.saturating_mul(1_000) / SCENE_RETAINED_LAYER_POSE_SAMPLE_RATE_HZ,
        );
        let SceneBinaryLayerGpuPosePayloads {
            sampled_layer_poses,
            solid_layer_poses,
        } = sampler
            .layer_gpu_pose_payloads(sample_time_ms)
            .map_err(|err| {
                NativeVulkanError::Scene(format!(
                    "build retained binary layer GPU pose timelines: {err}"
                ))
            })?;
        if wants_sampled {
            sampled_frames.push(
                sampled_layer_poses
                    .into_iter()
                    .map(native_vulkan_scene_engine_layer_pose)
                    .collect(),
            );
        }
        if wants_solid {
            solid_frames.push(
                solid_layer_poses
                    .into_iter()
                    .map(native_vulkan_scene_engine_layer_pose)
                    .collect(),
            );
        }
    }
    let frame_rate = SCENE_RETAINED_LAYER_POSE_SAMPLE_RATE_HZ.min(u64::from(u32::MAX)) as u32;
    let (sampled_layer_poses, sampled_layer_pose_timeline) = if wants_sampled {
        let (base_poses, timeline) =
            retained_layer_pose_timeline_from_frames(frame_rate, sampled_frames).map_err(
                |err| {
                    NativeVulkanError::Scene(format!(
                        "build retained sampled-layer engine pose timeline: {err}"
                    ))
                },
            )?;
        (
            base_poses
                .into_iter()
                .map(native_vulkan_scene_vulkanalia_sampled_layer_gpu_pose)
                .collect(),
            timeline.map(native_vulkan_scene_vulkanalia_sampled_layer_pose_timeline),
        )
    } else {
        (Vec::new(), None)
    };
    let (solid_layer_poses, solid_layer_pose_timeline) = if wants_solid {
        let (base_poses, timeline) =
            retained_layer_pose_timeline_from_frames(frame_rate, solid_frames).map_err(|err| {
                NativeVulkanError::Scene(format!(
                    "build retained solid-layer engine pose timeline: {err}"
                ))
            })?;
        (
            base_poses
                .into_iter()
                .map(native_vulkan_scene_vulkanalia_solid_layer_gpu_pose)
                .collect(),
            timeline.map(native_vulkan_scene_vulkanalia_solid_layer_pose_timeline),
        )
    } else {
        (Vec::new(), None)
    };
    Ok(NativeVulkanSceneRetainedLayerGpuPosePayloads {
        sampled_layer_poses,
        sampled_layer_pose_timeline,
        solid_layer_poses,
        solid_layer_pose_timeline,
    })
}

fn native_vulkan_scene_retained_gpu_payloads_from_sampler(
    plan: &SceneWallpaperPlan,
    sampler: Option<&mut SceneBinaryRuntimeSampler>,
) -> Result<
    (
        Vec<NativeVulkanVulkanaliaScenePuppetGpuPayload>,
        Vec<NativeVulkanVulkanaliaScenePuppetGpuPosePayload>,
        Vec<NativeVulkanVulkanaliaSceneParticleGpuPayload>,
    ),
    NativeVulkanError,
> {
    let wants_particles = matches!(
        plan.scene_systems.particles,
        SceneSystemStatus::Detected | SceneSystemStatus::Ready
    );
    if !native_vulkan_scene_plan_uses_binary_scene(plan)
        || (plan.puppet_animation_layer_count == 0 && !wants_particles)
    {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let Some(sampler) = sampler else {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    };
    let SceneBinaryRetainedGpuPayloads {
        puppet_payloads,
        puppet_poses,
        particle_payloads,
    } = sampler
        .retained_gpu_payloads(plan.snapshot_time_ms)
        .map_err(|err| {
            NativeVulkanError::Scene(format!("build retained binary GPU payloads: {err}"))
        })?;
    Ok((
        puppet_payloads
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_puppet_gpu_payload)
            .collect(),
        puppet_poses
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_puppet_gpu_pose_payload)
            .collect(),
        particle_payloads
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_particle_gpu_payload)
            .collect(),
    ))
}

fn native_vulkan_scene_gpu_only_puppet_layers(
    payloads: &[NativeVulkanVulkanaliaScenePuppetGpuPayload],
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
) -> BTreeSet<usize> {
    payloads
        .iter()
        .filter(|payload| {
            draw_steps.iter().any(|step| {
                step.layer_index == payload.layer_index
                    && step.vertex_count == payload.vertices.len().min(u32::MAX as usize) as u32
                    && step.index_count == payload.indices.len().min(u32::MAX as usize) as u32
            })
        })
        .map(|payload| payload.layer_index)
        .collect()
}

fn native_vulkan_scene_vulkanalia_puppet_gpu_payload(
    payload: SceneBinaryPuppetGpuPayload,
) -> NativeVulkanVulkanaliaScenePuppetGpuPayload {
    NativeVulkanVulkanaliaScenePuppetGpuPayload {
        layer_index: payload.layer_index,
        layer_id: payload.layer_id,
        geometry_index: payload.geometry_index,
        puppet_index: payload.puppet_index,
        vertices: payload
            .vertices
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_puppet_gpu_vertex_payload)
            .collect(),
        indices: payload.indices,
        bone_count: payload.bone_count,
    }
}

fn native_vulkan_scene_vulkanalia_puppet_gpu_vertex_payload(
    vertex: SceneBinaryPuppetGpuVertexPayload,
) -> NativeVulkanVulkanaliaScenePuppetGpuVertexPayload {
    NativeVulkanVulkanaliaScenePuppetGpuVertexPayload {
        position: vertex.position,
        uv: vertex.uv,
        opacity: vertex.opacity,
        bone_indices: vertex.bone_indices,
        bone_weights: vertex.bone_weights,
    }
}

fn native_vulkan_scene_vulkanalia_particle_gpu_payload(
    payload: SceneBinaryParticleGpuPayload,
) -> NativeVulkanVulkanaliaSceneParticleGpuPayload {
    NativeVulkanVulkanaliaSceneParticleGpuPayload {
        layer_index: payload.layer_index,
        layer_id: payload.layer_id,
        particle_index: payload.particle_index,
        position_transform_x: payload.position_transform_x,
        position_transform_y: payload.position_transform_y,
        layer_opacity: payload.layer_opacity,
        tint: payload.tint,
        loop_playback: payload.loop_playback,
        fade: payload.fade,
        vertices: payload
            .vertices
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_particle_gpu_vertex_payload)
            .collect(),
        indices: payload.indices,
    }
}

fn native_vulkan_scene_vulkanalia_particle_gpu_vertex_payload(
    vertex: SceneBinaryParticleGpuVertexPayload,
) -> NativeVulkanVulkanaliaSceneParticleGpuVertexPayload {
    NativeVulkanVulkanaliaSceneParticleGpuVertexPayload {
        corner: vertex.corner,
        uv: vertex.uv,
        spawn: vertex.spawn,
        velocity: vertex.velocity,
        constants: vertex.constants,
    }
}

fn native_vulkan_scene_vulkanalia_puppet_gpu_pose_payload(
    pose: SceneBinaryPuppetGpuPosePayload,
) -> NativeVulkanVulkanaliaScenePuppetGpuPosePayload {
    NativeVulkanVulkanaliaScenePuppetGpuPosePayload {
        layer_index: pose.layer_index,
        layer_id: pose.layer_id,
        puppet_index: pose.puppet_index,
        position_transform_x: pose.position_transform_x,
        position_transform_y: pose.position_transform_y,
        layer_opacity: pose.layer_opacity,
        layer_extent: pose.layer_extent,
        layer_anchor: pose.layer_anchor,
        pose_frame_count: pose.pose_frame_count,
        pose_frame_rate: pose.pose_frame_rate,
        pose_looping: pose.pose_looping,
        pose_frame_bone_count: pose.pose_frame_bone_count,
        skin_matrices: pose.skin_matrices,
        bone_opacities: pose.bone_opacities,
    }
}

fn native_vulkan_scene_engine_layer_pose(
    pose: SceneBinarySampledLayerGpuPosePayload,
) -> SceneLayerPose {
    SceneLayerPose {
        layer_index: pose.layer_index,
        position_transform_x: pose.position_transform_x,
        position_transform_y: pose.position_transform_y,
        layer_opacity: pose.layer_opacity,
    }
}

fn native_vulkan_scene_vulkanalia_sampled_layer_gpu_pose(
    pose: SceneLayerPose,
) -> NativeVulkanVulkanaliaSceneSampledImageLayerPosePayload {
    NativeVulkanVulkanaliaSceneSampledImageLayerPosePayload {
        layer_index: pose.layer_index,
        position_transform_x: pose.position_transform_x,
        position_transform_y: pose.position_transform_y,
        layer_opacity: pose.layer_opacity,
    }
}

fn native_vulkan_scene_vulkanalia_solid_layer_gpu_pose(
    pose: SceneLayerPose,
) -> NativeVulkanVulkanaliaSceneSolidQuadLayerPosePayload {
    NativeVulkanVulkanaliaSceneSolidQuadLayerPosePayload {
        layer_index: pose.layer_index,
        position_transform_x: pose.position_transform_x,
        position_transform_y: pose.position_transform_y,
        layer_opacity: pose.layer_opacity,
    }
}

fn native_vulkan_scene_vulkanalia_sampled_layer_pose_timeline(
    timeline: SceneLayerPoseTimeline,
) -> NativeVulkanVulkanaliaSceneSampledImageLayerPoseTimelinePayload {
    NativeVulkanVulkanaliaSceneSampledImageLayerPoseTimelinePayload {
        frame_rate: timeline.frame_rate,
        frame_count: timeline.frame_count,
        layer_indices: timeline.layer_indices,
        poses: timeline
            .poses
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_sampled_layer_gpu_pose)
            .collect(),
    }
}

fn native_vulkan_scene_vulkanalia_solid_layer_pose_timeline(
    timeline: SceneLayerPoseTimeline,
) -> NativeVulkanVulkanaliaSceneSolidQuadLayerPoseTimelinePayload {
    NativeVulkanVulkanaliaSceneSolidQuadLayerPoseTimelinePayload {
        frame_rate: timeline.frame_rate,
        frame_count: timeline.frame_count,
        layer_indices: timeline.layer_indices,
        poses: timeline
            .poses
            .into_iter()
            .map(native_vulkan_scene_vulkanalia_solid_layer_gpu_pose)
            .collect(),
    }
}

pub fn run_scene(
    mut options: NativeVulkanOptions,
    duration: Duration,
    mut plan: SceneWallpaperPlan,
    scene_audio_output_mode: NativeVulkanAudioOutputMode,
    scene_video: Option<NativeVulkanSceneVideoPresentOptions>,
) -> Result<NativeVulkanScenePresentSnapshot, NativeVulkanError> {
    #[cfg(not(feature = "native-vulkan-video"))]
    let _ = scene_video;

    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator();

    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps =
        native_vulkan_scene_effective_target_max_fps(options.target_max_fps, plan.target_max_fps);
    options.target_max_fps = target_max_fps;
    let render_item = native_vulkan_scene_item(&plan);
    let scene_audio_cues = native_vulkan_scene_active_audio_cues(&plan);
    let scene_audio_target_max_fps = plan.target_max_fps;
    options.clear_color = native_vulkan_render_item_clear_color(&render_item, options.clear_color);
    let mut runtime = native_vulkan_scene_runtime_snapshot(&render_item).ok_or_else(|| {
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
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_scene_release_plan_cpu_payloads_for_present(&mut plan);
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                scene_audio_cues,
                scene_audio_target_max_fps,
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
            let mut geometry = runtime
                .take_vulkanalia_solid_quad_geometry_input()
                .ok_or_else(|| {
                    NativeVulkanError::Scene(format!(
                        "scene draw plan is not solid-quad recordable: {}",
                        runtime.draw_pass_backend_status
                    ))
                })?;
            let mut binary_sampler =
                native_vulkan_scene_retained_binary_runtime_sampler(&plan, false, true)?;
            let retained_layer_poses =
                native_vulkan_scene_retained_layer_gpu_pose_payloads_from_sampler(
                    &plan,
                    duration,
                    binary_sampler.as_mut(),
                    false,
                    true,
                )?;
            native_vulkan_scene_apply_retained_layer_pose_telemetry(
                &mut runtime,
                &retained_layer_poses,
            );
            geometry.solid_layer_poses = retained_layer_poses.solid_layer_poses;
            geometry.solid_layer_pose_timeline = retained_layer_poses.solid_layer_pose_timeline;
            drop(binary_sampler);
            let scene_time_origin_ms = plan.snapshot_time_ms;
            let scene_size = runtime.scene_size;
            let scene_fit = runtime.scene_fit;
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_scene_release_plan_cpu_payloads_for_present(&mut plan);
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                scene_audio_cues,
                scene_audio_target_max_fps,
                duration,
                scene_audio_output_mode,
                || {
                    run_native_vulkan_vulkanalia_scene_solid_quad_present(
                        NativeVulkanVulkanaliaSceneSolidQuadPresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            duration,
                            scene_time_origin_ms,
                            target_max_fps,
                            quad_color: options.clear_color,
                            geometry: Some(geometry),
                            scene_size,
                            scene_fit,
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
            let (source, fit, mut geometry) = if let Some((source, geometry)) =
                runtime.take_vulkanalia_sampled_image_geometry_input()
            {
                (source, None, Some(geometry))
            } else if let Some((source, fit)) =
                runtime.take_vulkanalia_sampled_image_implicit_full_extent_input()
            {
                (source, Some(fit), None)
            } else {
                return Err(NativeVulkanError::Scene(format!(
                    "scene draw plan is not sampled-image recordable: {}",
                    runtime.draw_pass_backend_status
                )));
            };
            let mut solid_geometry = runtime.take_vulkanalia_mixed_solid_quad_geometry_input();
            let mut binary_sampler = native_vulkan_scene_retained_binary_runtime_sampler(
                &plan,
                geometry.is_some(),
                solid_geometry.is_some(),
            )?;
            let (
                mut retained_puppet_gpu_payloads,
                mut retained_puppet_gpu_poses,
                retained_particle_gpu_payloads,
            ) = native_vulkan_scene_retained_gpu_payloads_from_sampler(
                &plan,
                binary_sampler.as_mut(),
            )?;
            let retained_layer_poses =
                native_vulkan_scene_retained_layer_gpu_pose_payloads_from_sampler(
                    &plan,
                    duration,
                    binary_sampler.as_mut(),
                    geometry.is_some(),
                    solid_geometry.is_some(),
                )?;
            native_vulkan_scene_apply_retained_layer_pose_telemetry(
                &mut runtime,
                &retained_layer_poses,
            );
            let gpu_only_puppet_layers = geometry
                .as_ref()
                .map(|geometry| {
                    native_vulkan_scene_gpu_only_puppet_layers(
                        &retained_puppet_gpu_payloads,
                        &geometry.draw_steps,
                    )
                })
                .unwrap_or_default();
            retained_puppet_gpu_payloads
                .retain(|payload| gpu_only_puppet_layers.contains(&payload.layer_index));
            retained_puppet_gpu_poses
                .retain(|pose| gpu_only_puppet_layers.contains(&pose.layer_index));
            if let Some(geometry) = geometry.as_mut() {
                geometry.puppet_gpu_payloads = retained_puppet_gpu_payloads;
                geometry.puppet_gpu_poses = retained_puppet_gpu_poses;
                geometry.particle_gpu_payloads = retained_particle_gpu_payloads;
                geometry.sampled_layer_poses = retained_layer_poses.sampled_layer_poses;
                geometry.sampled_layer_pose_timeline =
                    retained_layer_poses.sampled_layer_pose_timeline;
            }
            if let Some(geometry) = solid_geometry.as_mut() {
                geometry.solid_layer_poses = retained_layer_poses.solid_layer_poses;
                geometry.solid_layer_pose_timeline = retained_layer_poses.solid_layer_pose_timeline;
            }
            drop(binary_sampler);
            let scene_time_origin_ms = plan.snapshot_time_ms;
            let scene_size = runtime.scene_size;
            let scene_fit = runtime.scene_fit;
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_scene_release_plan_cpu_payloads_for_present(&mut plan);
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                scene_audio_cues,
                scene_audio_target_max_fps,
                duration,
                scene_audio_output_mode,
                || {
                    run_native_vulkan_vulkanalia_scene_sampled_image_present(
                        NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            duration,
                            scene_time_origin_ms,
                            target_max_fps,
                            source,
                            clear_color: options.clear_color,
                            fit,
                            solid_geometry,
                            geometry,
                            scene_size,
                            scene_fit,
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
            let scene_video = scene_video.ok_or_else(|| {
                NativeVulkanError::Scene(
                    "scene video layer requires FFmpeg Vulkan scene-video options".to_owned(),
                )
            })?;
            let video_geometry = runtime.take_vulkanalia_video_layer_geometry_input();
            let scene_video_sources = scene_video.sources.clone();
            if scene_video_sources.is_empty() {
                return Err(NativeVulkanError::Scene(
                    "scene video present requires at least one source".to_owned(),
                ));
            }
            let mut overlay_source = None;
            let mut overlay_fit = None;
            let mut overlay_geometry = None;
            if let Some((source, geometry)) = runtime.take_vulkanalia_sampled_image_geometry_input()
            {
                overlay_source = Some(source);
                overlay_geometry = Some(geometry);
            } else if let Some((source, fit)) =
                runtime.take_vulkanalia_sampled_image_implicit_full_extent_input()
            {
                overlay_source = Some(source);
                overlay_fit = Some(fit);
            }
            let mut solid_geometry = runtime.take_vulkanalia_mixed_solid_quad_geometry_input();
            let mut binary_sampler = native_vulkan_scene_retained_binary_runtime_sampler(
                &plan,
                overlay_geometry.is_some(),
                solid_geometry.is_some(),
            )?;
            let retained_layer_poses =
                native_vulkan_scene_retained_layer_gpu_pose_payloads_from_sampler(
                    &plan,
                    duration,
                    binary_sampler.as_mut(),
                    overlay_geometry.is_some(),
                    solid_geometry.is_some(),
                )?;
            native_vulkan_scene_apply_retained_layer_pose_telemetry(
                &mut runtime,
                &retained_layer_poses,
            );
            if overlay_source.is_some() || overlay_geometry.is_some() {
                let (
                    mut retained_puppet_gpu_payloads,
                    mut retained_puppet_gpu_poses,
                    retained_particle_gpu_payloads,
                ) = native_vulkan_scene_retained_gpu_payloads_from_sampler(
                    &plan,
                    binary_sampler.as_mut(),
                )?;
                let gpu_only_puppet_layers = overlay_geometry
                    .as_ref()
                    .map(|geometry| {
                        native_vulkan_scene_gpu_only_puppet_layers(
                            &retained_puppet_gpu_payloads,
                            &geometry.draw_steps,
                        )
                    })
                    .unwrap_or_default();
                retained_puppet_gpu_payloads
                    .retain(|payload| gpu_only_puppet_layers.contains(&payload.layer_index));
                retained_puppet_gpu_poses
                    .retain(|pose| gpu_only_puppet_layers.contains(&pose.layer_index));
                if let Some(geometry) = overlay_geometry.as_mut() {
                    geometry.puppet_gpu_payloads = retained_puppet_gpu_payloads;
                    geometry.puppet_gpu_poses = retained_puppet_gpu_poses;
                    geometry.particle_gpu_payloads = retained_particle_gpu_payloads;
                    geometry.sampled_layer_poses = retained_layer_poses.sampled_layer_poses;
                    geometry.sampled_layer_pose_timeline =
                        retained_layer_poses.sampled_layer_pose_timeline;
                }
            }
            if let Some(geometry) = solid_geometry.as_mut() {
                geometry.solid_layer_poses = retained_layer_poses.solid_layer_poses;
                geometry.solid_layer_pose_timeline = retained_layer_poses.solid_layer_pose_timeline;
            }
            drop(binary_sampler);
            let scene_time_origin_ms = plan.snapshot_time_ms;
            let scene_video_overlay = (overlay_source.is_some()
                || overlay_geometry.is_some()
                || solid_geometry.is_some()
                || video_geometry.is_some())
            .then_some(NativeVulkanVulkanaliaSceneVideoOverlayInput {
                video_geometry,
                source: overlay_source,
                clear_color: options.clear_color,
                fit: overlay_fit,
                scene_time_origin_ms,
                solid_geometry,
                geometry: overlay_geometry,
                scene_size: runtime.scene_size,
                scene_fit: runtime.scene_fit,
            });
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_scene_release_plan_cpu_payloads_for_present(&mut plan);
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                scene_audio_cues,
                scene_audio_target_max_fps,
                duration,
                scene_audio_output_mode,
                || {
                    let sources = scene_video_sources
                        .into_iter()
                        .map(
                            |source| NativeVulkanFfmpegVulkanHwSceneVideoPresentSourceOptions {
                                source: source.source,
                                codec: source.codec,
                                playback_frame_count: source.playback_frames,
                            },
                        )
                        .collect();
                    run_native_vulkan_ffmpeg_vulkan_hw_scene_video_present(
                        NativeVulkanFfmpegVulkanHwSceneVideoPresentOptions {
                            host: options.host,
                            wait_configure_roundtrips: options.wait_configure_roundtrips,
                            target_max_fps: options.target_max_fps,
                            audio_clock_probe_requested: scene_video.audio_clock_probe_requested,
                            audio_output_mode: scene_video.audio_output_mode,
                            clear_color: options.clear_color,
                            sources,
                            scene_video_overlay,
                        },
                    )
                    .map_err(NativeVulkanError::Video)
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

fn native_vulkan_scene_release_plan_cpu_payloads_for_present(plan: &mut SceneWallpaperPlan) {
    plan.layers = Vec::new();
    plan.bound_properties = Vec::new();
    plan.scene_input_properties = Default::default();
    plan.unsupported_scene_features = Vec::new();
}

fn native_vulkan_scene_effective_target_max_fps(
    options_target_max_fps: Option<u32>,
    plan_target_max_fps: Option<u32>,
) -> Option<u32> {
    const NATIVE_VULKAN_OPTIONS_DEFAULT_TARGET_FPS: u32 = 240;
    plan_target_max_fps.or_else(|| {
        options_target_max_fps.filter(|fps| *fps != NATIVE_VULKAN_OPTIONS_DEFAULT_TARGET_FPS)
    })
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
    audio_cues: Vec<NativeVulkanSceneAudioCuePlayback>,
    target_max_fps: Option<u32>,
    duration: Duration,
    output_mode: NativeVulkanAudioOutputMode,
    present: impl FnOnce() -> Result<T, NativeVulkanError>,
) -> Result<(T, Vec<NativeVulkanSceneAudioCueRuntimeSnapshot>), NativeVulkanError> {
    let audio_workers =
        native_vulkan_scene_start_audio_workers(audio_cues, target_max_fps, duration, output_mode)?;
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
    audio_cues: Vec<NativeVulkanSceneAudioCuePlayback>,
    target_max_fps: Option<u32>,
    duration: Duration,
    output_mode: NativeVulkanAudioOutputMode,
) -> Result<Vec<NativeVulkanSceneAudioWorker>, NativeVulkanError> {
    audio_cues
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
                native_vulkan_scene_audio_playback_frame_count(duration, target_max_fps);
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
    audio_cues: Vec<NativeVulkanSceneAudioCuePlayback>,
    _target_max_fps: Option<u32>,
    _duration: Duration,
    _output_mode: NativeVulkanAudioOutputMode,
) -> Result<Vec<NativeVulkanSceneAudioWorker>, NativeVulkanError> {
    if audio_cues.is_empty() {
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
        let unsupported_layers = runtime
            .unsupported_layers
            .iter()
            .map(|layer| format!("{}:{}", layer.layer_id, layer.reason))
            .collect::<Vec<_>>()
            .join(",");
        return Err(NativeVulkanError::Scene(format!(
            "scene draw plan is not presentable by the native Vulkan scene backend: {}; draw_ops={}, unsupported_layers={}, unsupported_layer_details=[{}], clear_background_ops={}, sampled_image_ops={}, sampled_image_steps={}, sampled_image_recording_ready={}, sampled_image_implicit_full_extent_ready={}, quad_steps={}",
            runtime.draw_pass_backend_status,
            runtime.draw_op_count,
            runtime.unsupported_layer_count,
            unsupported_layers,
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
        | "multi-video-layer-vulkan-video-scene-bridge-ready"
        | "clear-background-video-layer-vulkan-video-scene-bridge-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::Video)
        }
        status => Err(NativeVulkanError::Scene(format!(
            "scene draw plan has no native Vulkan present route: {status}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        FitMode, SceneNodeKind, ScenePathFillRule, SceneSystems, SceneTextureRegion, SceneTransform,
    };
    use crate::renderer::{SceneDisplayPlan, SceneRenderAudioCue, SceneRenderLayer};
    use std::path::PathBuf;

    fn layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: Default::default(),
            blend_mode: Default::default(),
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::default(),
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }

    #[test]
    fn scene_default_options_target_fps_does_not_cap_runtime() {
        assert_eq!(
            native_vulkan_scene_effective_target_max_fps(Some(240), None),
            None
        );
        assert_eq!(
            native_vulkan_scene_effective_target_max_fps(Some(120), None),
            Some(120)
        );
        assert_eq!(
            native_vulkan_scene_effective_target_max_fps(Some(240), Some(240)),
            Some(240)
        );
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
            puppet_animation_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            scene_input_properties: Default::default(),
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
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
    fn binary_scene_plan_uses_solid_dynamic_sampler_only_for_solid_affecting_animation() {
        let mut image = layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.gtex"));
        image.width = Some(640.0);
        image.height = Some(360.0);
        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));
        plan.timeline_animation_count = 1;
        plan.timeline_animated_layer_count = 1;
        plan.puppet_animation_layer_count = 1;

        assert!(native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(
            &plan
        ));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn scene_main_present_route_selects_video_for_mixed_video_scene() {
        let mut video = layer("cinematic", SceneNodeKind::Video);
        video.source = Some(PathBuf::from("/tmp/scene-video.mp4"));
        let mut overlay = layer("overlay", SceneNodeKind::Image);
        overlay.source = Some(PathBuf::from("/tmp/overlay.gtex"));
        overlay.width = Some(256.0);
        overlay.height = Some(256.0);
        let mut panel = layer("panel", SceneNodeKind::Rectangle);
        panel.color = Some("#102030".to_owned());
        panel.width = Some(320.0);
        panel.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![video, overlay, panel]).unwrap(),
            NativeVulkanScenePresentRouteKind::Video
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn scene_main_present_route_selects_video_for_multi_video_scene() {
        let mut h264 = layer("h264-layer", SceneNodeKind::Video);
        h264.source = Some(PathBuf::from("/tmp/h264.mp4"));
        h264.width = Some(640.0);
        h264.height = Some(360.0);
        let mut h265 = layer("h265-layer", SceneNodeKind::Video);
        h265.source = Some(PathBuf::from("/tmp/h265.mp4"));
        h265.width = Some(640.0);
        h265.height = Some(360.0);
        h265.transform.x = 640.0;
        let mut av1 = layer("av1-layer", SceneNodeKind::Video);
        av1.source = Some(PathBuf::from("/tmp/av1.webm"));
        av1.width = Some(640.0);
        av1.height = Some(360.0);
        av1.transform.x = 1280.0;

        assert_eq!(
            route_for_layers(vec![h264, h265, av1]).unwrap(),
            NativeVulkanScenePresentRouteKind::Video
        );
    }

    #[test]
    fn animated_texture_regions_do_not_enable_binary_solid_sampling() {
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

        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));

        assert!(!native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(&plan));
    }

    #[test]
    fn puppet_animation_layers_do_not_enable_binary_solid_sampling() {
        let mut image = layer("puppet", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/puppet.gtex"));

        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));
        plan.puppet_animation_layer_count = 1;

        assert!(!native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(&plan));
    }

    #[test]
    fn particle_emitters_do_not_enable_binary_solid_sampling() {
        let mut image = layer("particle", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/particle.gtex"));

        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));
        plan.scene_systems.particles = SceneSystemStatus::Ready;

        assert!(!native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(&plan));
    }

    #[test]
    fn retained_binary_sampler_is_prepared_only_for_retained_payloads() {
        let mut image = layer("static-image", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/static.gtex"));
        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));

        assert!(
            !native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, true, false)
        );
        assert!(
            !native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, false, true)
        );

        plan.scene_systems.particles = SceneSystemStatus::Ready;
        assert!(native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, true, false));
        assert!(
            !native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, false, true)
        );

        plan.timeline_animation_count = 1;
        assert!(native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, false, true));
    }

    #[test]
    fn property_bindings_prepare_retained_binary_pose_sampler() {
        let mut image = layer("bound-image", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/bound.gtex"));
        let mut plan = plan(vec![image]);
        plan.source = Some(PathBuf::from("/tmp/scene.gscn"));
        plan.property_binding_count = 1;

        assert!(native_vulkan_scene_plan_needs_binary_sampled_layer_pose_sampler(&plan));
        assert!(native_vulkan_scene_plan_needs_binary_solid_dynamic_sampler(
            &plan
        ));
        assert!(native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, true, false));
        assert!(native_vulkan_scene_plan_needs_retained_binary_runtime_sampler(&plan, false, true));
    }

    #[test]
    fn scene_audio_runtime_uses_only_active_cues() {
        let mut image = layer("speaker", SceneNodeKind::Image);
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/theme.ogg"),
            playback_mode: Some("loop".to_owned()),
            volume: None,
            start_silent: false,
            active_conditions: Vec::new(),
        });
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/response.ogg"),
            playback_mode: None,
            volume: None,
            start_silent: true,
            active_conditions: Vec::new(),
        });
        let plan = plan(vec![image]);

        let active = native_vulkan_scene_active_audio_cues(&plan);

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].layer_id, "speaker");
        assert_eq!(active[0].cue.source, PathBuf::from("/tmp/theme.ogg"));
        #[cfg(feature = "native-vulkan-video")]
        assert!(native_vulkan_scene_audio_loop_on_eos(&active[0].cue));
    }

    #[test]
    fn present_release_drops_layer_cpu_payload_after_compact_audio_plan() {
        let mut image = layer("speaker", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.gtex"));
        image.audio.push(SceneRenderAudioCue {
            source: PathBuf::from("/tmp/theme.ogg"),
            playback_mode: Some("loop".to_owned()),
            volume: None,
            start_silent: false,
            active_conditions: Vec::new(),
        });
        let mut plan = plan(vec![image]);
        plan.bound_properties.push("exposure".to_owned());
        plan.scene_input_properties
            .insert("exposure".to_owned(), serde_json::json!(1.0));
        plan.unsupported_scene_features
            .push("debug-only".to_owned());

        let active = native_vulkan_scene_active_audio_cues(&plan);
        native_vulkan_scene_release_plan_cpu_payloads_for_present(&mut plan);

        assert!(plan.layers.is_empty());
        assert!(plan.bound_properties.is_empty());
        assert!(plan.scene_input_properties.is_empty());
        assert!(plan.unsupported_scene_features.is_empty());
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].layer_id, "speaker");
        assert_eq!(active[0].cue.source, PathBuf::from("/tmp/theme.ogg"));
    }
}
