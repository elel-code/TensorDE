//! Production scene loop using only shared renderer owners and typed commands.

mod snapshot;
#[cfg(feature = "video")]
mod video;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "video")]
use vulkan_renderer::PresentationFrameDependencies;
use vulkan_renderer::{
    CommandEncoderDescriptor, Extent2D, Features, FrameTargetPreference,
    FullscreenSampledSurfaceTerminalDescriptor, OffscreenSamplerTopology, PresentationBootstrap,
    PresentationPathDescriptor, PresentationPathPlan, PresentationRequirements,
    PresentationTransactionDescriptor, SurfaceAcquireStrategy, TerminalAlphaMode,
    TerminalCompositeDescriptor, TerminalSampling, TextureUsages,
};

use crate::engine::scene::RenderingServer;
use crate::engine::scene::semantic_world::SemanticFrameResolver;
use crate::renderer::rendering_device::shared_presentation::tensor_wallpaper_presentation_bootstrap_descriptor;
use crate::renderer::rendering_device::{
    SceneExecutionPlan, scene_execution_plan_from_semantic_frame,
};
use crate::renderer::wayland::WaylandHost;

use super::frame_events::SceneRuntimeEventSources;
use super::gpu_timing::SceneGpuTiming;
use super::pipeline::scene_requires_coherent_advanced_blend;
use super::scene_terminal_program::scene_terminal_program;
use super::shared_scene::SharedSceneGpuResources;
use super::{RenderingDeviceScenePresentOptions, RenderingDeviceScenePresentSnapshot};
#[cfg(feature = "video")]
use video::SharedSceneVideoRuntime;

const WE_MAX_FRAME_DELTA_SECONDS: f32 = 0.25;

#[derive(Default)]
struct SharedFrameStats {
    frames_presented: u64,
    last_present_completed_at: Option<Instant>,
    present_delta_min_micros: Option<u64>,
    present_delta_max_micros: Option<u64>,
    present_delta_over_6250us_count: u64,
    present_delta_over_8334us_count: u64,
    transform_uniform_update_count: u64,
    effect_uniform_update_count: u64,
    skinning_storage_update_count: u64,
    scene_owned_uniform_update_count: u64,
    frame_state_update_total_micros: u64,
    semantic_resolve_total_micros: u64,
    graph_update_total_micros: u64,
    transform_update_total_micros: u64,
    material_update_total_micros: u64,
    skinning_update_total_micros: u64,
    scene_owned_uniform_update_total_micros: u64,
    draw_policy_update_total_micros: u64,
    command_recording_total_micros: u64,
    scene_color_attachment_clear_frame_count: u64,
}

pub(super) fn run(
    mut options: RenderingDeviceScenePresentOptions,
) -> Result<RenderingDeviceScenePresentSnapshot, String> {
    let mut host =
        WaylandHost::connect(options.host.clone()).map_err(|error| error.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|error| error.to_string())?;
    let handles = host.surface_handles().map_err(|error| error.to_string())?;
    let extent = options.surface_extent.unwrap_or(handles.buffer_size);

    let initial_semantic_frame = RenderingServer::new(&options.storage)
        .semantic_world()
        .and_then(|world| {
            world.resolve_frame_with_user_properties_at(0.0, &options.user_property_overrides)
        })
        .map_err(|error| format!("resolve initial scene user properties: {error}"))?;
    let execution_plan = scene_execution_plan_from_semantic_frame(
        &options.storage,
        &initial_semantic_frame,
    );
    crate::renderer::rendering_device::scene::validate_scene_runtime_plan(&execution_plan)
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "video")]
    let video_requirements =
        SharedSceneVideoRuntime::requirements(&execution_plan, &options.video_sources)?;
    #[cfg(not(feature = "video"))]
    let video_requirements = reject_video_without_feature(&execution_plan, &options.video_sources)?;
    let requires_advanced_blend = scene_requires_coherent_advanced_blend(
        &options.storage,
        &execution_plan.rendering_device_graph,
    )?;
    let audio_spectrum_required = super::material_uniform::scene_uses_audio_spectrum(
        &options.storage,
        &execution_plan.rendering_device_graph.mesh_draws,
    );
    let mut event_sources = SceneRuntimeEventSources::new(
        &options.storage,
        options.pointer_replay_normalized,
        audio_spectrum_required,
    );

    let scene_color_4x_msaa = scene_color_4x_msaa_requested()?;
    let mut required_features = Features::RETAINED_SCENE_PRESENTATION;
    if requires_advanced_blend {
        required_features |= Features::ADVANCED_BLEND | Features::ADVANCED_BLEND_COHERENT;
    }
    let optional_features = if scene_color_4x_msaa {
        Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED
    } else {
        Features::empty()
    };
    let mut bootstrap = PresentationBootstrap::create(
        Arc::new(handles.renderer_handle.clone()),
        tensor_wallpaper_presentation_bootstrap_descriptor(
            "tensor-wallpaper-scene",
            extent,
            required_features,
            optional_features,
            if scene_color_4x_msaa {
                vulkan_renderer::SampleCounts::FOUR
            } else {
                vulkan_renderer::SampleCounts::ONE
            },
            video_requirements,
        )?,
    )
    .map_err(|error| format!("create Tensor Wallpaper scene presentation bootstrap: {error}"))?;
    #[cfg(feature = "video")]
    let mut video = SharedSceneVideoRuntime::open(
        bootstrap.video_decode.as_ref(),
        &execution_plan,
        std::mem::take(&mut options.video_sources),
    )?;
    let configuration = bootstrap.swapchain.configuration();
    let target_extent = configuration.extent;
    let frame_slot_count = shared_scene_frame_slot_count()?;
    let plan = presentation_plan(
        target_extent,
        configuration.format,
        frame_slot_count,
        &execution_plan,
    )?;
    let terminal = bootstrap
        .device
        .create_fullscreen_sampled_surface_terminal(
            &bootstrap.allocator,
            &bootstrap.pipeline_binary_cache,
            &FullscreenSampledSurfaceTerminalDescriptor {
                label: Some("tensor-wallpaper-scene-terminal".into()),
                plan: &plan,
                additional_target_usage: TextureUsages::COPY_SOURCE,
                sampler_topology: OffscreenSamplerTopology::PerFrameSlot,
                program: scene_terminal_program(),
            },
        )
        .map_err(|error| format!("create shared scene terminal resources: {error}"))?;
    let mut scene = SharedSceneGpuResources::create(
        &bootstrap.device,
        &bootstrap.allocator,
        &mut bootstrap.upload_belt,
        &bootstrap.queue,
        &options.storage,
        execution_plan,
        configuration.format,
        target_extent,
        terminal.target_views(),
        frame_slot_count,
        scene_color_4x_msaa,
        bootstrap
            .device
            .features()
            .contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED),
        &bootstrap.pipeline_binary_cache,
    )?;
    let mut gpu_timing = SceneGpuTiming::create(
        &bootstrap.device,
        &scene,
        frame_slot_count,
        options.gpu_timing,
    )?;
    let mut transaction = bootstrap
        .device
        .create_presentation_transaction(&PresentationTransactionDescriptor {
            label: Some("tensor-wallpaper-scene-presentation".into()),
            plan: &plan,
            swapchain: &bootstrap.swapchain,
            acquire_timeout_ns: u64::MAX,
        })
        .map_err(|error| format!("create shared scene presentation transaction: {error}"))?;

    let released_resource_payload_bytes = options.storage.release_parsed_resource_payload();
    let released_texture_payload_bytes = options.storage.release_uploaded_texture_payload();
    let (released_mesh_vertex_payload_bytes, released_mesh_index_payload_bytes) =
        options.storage.release_uploaded_mesh_payload();
    let semantic_world = RenderingServer::new(&options.storage)
        .semantic_world()
        .expect("scene semantic world was validated during shared GPU setup");
    let mut semantic_resolver = SemanticFrameResolver::from_world_with_user_properties(
        &semantic_world,
        &options.user_property_overrides,
    )
    .map_err(|error| format!("create retained semantic frame resolver: {error}"))?;

    if std::env::var_os("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_PIPELINE_DEBUG").is_some() {
        eprintln!("tensor-wallpaper-scene-startup: shared-frame-loop-ready");
    }
    let started_at = Instant::now();
    let deadline = options.duration.map(|duration| started_at + duration);
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let fixed_scene_time_seconds = fixed_non_negative_f32("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_FIXED_TIME");
    let fixed_frame_delta_seconds =
        fixed_non_negative_f32("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_FIXED_FRAME_DELTA");
    let mut next_frame = started_at;
    let mut previous_frame_sampled_at = None;
    let mut accumulated_scene_time_seconds = 0.0f32;
    let mut scene_color_initialized = vec![false; frame_slot_count];
    let mut stats = SharedFrameStats::default();

    while deadline.is_none_or(|deadline| Instant::now() < deadline) {
        if !event_sources.pump_platform(&mut host)? {
            break;
        }
        let frame_slot = stats.frames_presented as usize % frame_slot_count;
        let reference_phase = stats.frames_presented as usize % scene.sampled_binding_cycle.len();
        let sampled_at = Instant::now();
        #[cfg(feature = "video")]
        video.advance_to(sampled_at, &mut event_sources)?;
        let raw_delta = previous_frame_sampled_at
            .map(|previous: Instant| {
                fixed_frame_delta_seconds
                    .unwrap_or_else(|| sampled_at.duration_since(previous).as_secs_f32())
            })
            .unwrap_or_else(|| fixed_frame_delta_seconds.unwrap_or(0.0));
        previous_frame_sampled_at = Some(sampled_at);
        let frame_delta_seconds = raw_delta.min(WE_MAX_FRAME_DELTA_SECONDS);
        let scene_time_seconds = fixed_scene_time_seconds.unwrap_or_else(|| {
            accumulated_scene_time_seconds += frame_delta_seconds;
            accumulated_scene_time_seconds
        });
        let sample_time_ns = sampled_at
            .duration_since(started_at)
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let frame_events = event_sources
            .sample_frame_events(sample_time_ns, host.logical_size())
            .clone();
        let initialized = scene_color_initialized[frame_slot];
        let recording_started = Instant::now();
        let mut frame_update = None;
        transaction
            .execute_frame(
                &bootstrap.swapchain,
                frame_slot,
                || {
                    (|| -> Result<[_; 1], String> {
                        let update_started = Instant::now();
                        let update = scene.update_semantic_frame(
                            &options.storage,
                            &semantic_world,
                            &mut semantic_resolver,
                            frame_slot,
                            reference_phase,
                            options.gpu_timing,
                            &frame_events,
                            scene_time_seconds,
                            frame_delta_seconds,
                            [target_extent.width, target_extent.height],
                        )?;
                        stats.frame_state_update_total_micros = stats
                            .frame_state_update_total_micros
                            .saturating_add(super::elapsed_micros_u64(update_started));
                        if let Some(timing) = gpu_timing.as_mut() {
                            timing.collect_slot(frame_slot)?;
                        }
                        let mut encoder = bootstrap
                            .device
                            .create_command_encoder(&CommandEncoderDescriptor {
                                label: Some(format!("tensor-wallpaper-scene-frame-{frame_slot}")),
                            })
                            .map_err(|error| {
                                format!("begin shared scene frame commands: {error}")
                            })?;
                        let timing_frame = gpu_timing
                            .as_ref()
                            .map(|timing| timing.frame(frame_slot))
                            .transpose()?;
                        if let Some(timing) = timing_frame {
                            timing.reset_and_start_frame(&mut encoder)?;
                            timing.start_particle(&mut encoder)?;
                        }
                        scene.record_particle_compute(&mut encoder, frame_slot, reference_phase)?;
                        if let Some(timing) = timing_frame {
                            timing.finish_particle(&mut encoder)?;
                        }
                        let target = terminal
                            .target(frame_slot)
                            .map_err(|error| format!("borrow shared SceneColor target: {error}"))?;
                        #[cfg(feature = "video")]
                        scene.record_graphs_to_scene_color_with_video(
                            &mut encoder,
                            frame_slot,
                            target.image,
                            target.view,
                            target_extent,
                            reference_phase,
                            initialized,
                            options.clear_color,
                            video.media_instances(),
                            video.decoded_frames(),
                            timing_frame,
                        )?;
                        #[cfg(not(feature = "video"))]
                        scene.record_graphs_to_scene_color(
                            &mut encoder,
                            frame_slot,
                            target.image,
                            target.view,
                            target_extent,
                            reference_phase,
                            initialized,
                            options.clear_color,
                            timing_frame,
                        )?;
                        if let Some(timing) = timing_frame {
                            timing.finish_frame(&mut encoder)?;
                        }
                        frame_update = Some(update);
                        encoder
                            .finish()
                            .map(|commands| [commands])
                            .map_err(|error| format!("finish shared scene frame commands: {error}"))
                    })()
                    .map_err(vulkan_renderer::Error::Validation)
                },
                #[cfg(feature = "video")]
                PresentationFrameDependencies::decoded_video(
                    video.decoded_frames(),
                    vulkan_renderer::PresentationDependencyScope::IndependentCommands,
                ),
                |encoder, acquired| {
                    terminal.record_surface(encoder, acquired, frame_slot, [0.0; 4])
                },
            )
            .map_err(|error| format!("execute shared scene frame transaction: {error}"))?;
        if let Some(timing) = gpu_timing.as_mut() {
            timing.mark_submitted(frame_slot)?;
        }
        stats.command_recording_total_micros = stats
            .command_recording_total_micros
            .saturating_add(super::elapsed_micros_u64(recording_started));
        scene_color_initialized[frame_slot] = true;
        accumulate_update(
            &mut stats,
            frame_update.expect("frame transaction recorded an update"),
        );
        observe_present(&mut stats, Instant::now());

        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
    }
    bootstrap
        .device
        .wait_idle()
        .map_err(|error| format!("wait for shared scene shutdown: {error}"))?;
    if let Some(timing) = gpu_timing.as_mut() {
        timing.collect_all()?;
    }
    let gpu_timing_snapshot = gpu_timing
        .as_ref()
        .map(SceneGpuTiming::snapshot)
        .transpose()?;
    let elapsed = started_at.elapsed();
    let wayland_surface = host.snapshot();
    snapshot::build(snapshot::SharedSnapshotInputs {
        options: &options,
        wayland_surface,
        bootstrap: &bootstrap,
        terminal: &terminal,
        scene: &scene,
        semantic_resolver: &semantic_resolver,
        event_sources: &mut event_sources,
        stats: &stats,
        scene_color_4x_msaa,
        elapsed,
        released_resource_payload_bytes,
        released_texture_payload_bytes,
        released_mesh_vertex_payload_bytes,
        released_mesh_index_payload_bytes,
        gpu_timing: gpu_timing_snapshot,
    })
}

fn presentation_plan(
    extent: Extent2D,
    format: vulkan_renderer::TextureFormat,
    frame_slots: usize,
    execution_plan: &SceneExecutionPlan,
) -> Result<PresentationPathPlan, String> {
    let physical_pass_count = execution_plan
        .rendering_device_graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
        .count()
        .max(1);
    PresentationPathPlan::compile(
        PresentationPathDescriptor {
            target: FrameTargetPreference::Offscreen,
            acquire: SurfaceAcquireStrategy::BeforeFrame,
            terminal: TerminalCompositeDescriptor {
                sampling: TerminalSampling::Linear,
                alpha: TerminalAlphaMode::Opaque,
            },
        },
        PresentationRequirements {
            surface_extent: extent,
            target_extent: extent,
            surface_format: format,
            target_format: format,
            frame_slots: u32::try_from(frame_slots)
                .map_err(|_| "scene frame-slot count exceeds u32")?,
            physical_pass_count: u32::try_from(physical_pass_count)
                .map_err(|_| "scene physical pass count exceeds u32")?,
            sampled_after_write: true,
            has_history: false,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: true,
        },
    )
    .map_err(|error| format!("compile shared scene presentation path: {error}"))
}

fn shared_scene_frame_slot_count() -> Result<usize, String> {
    const MAX: usize = 3;
    let count = std::env::var("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_FRAME_SLOT_COUNT")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid scene frame-slot count {value:?}"))
        })
        .transpose()?
        .unwrap_or(1);
    if !(1..=MAX).contains(&count) {
        return Err(format!(
            "scene frame-slot count must be in 1..={MAX}, got {count}"
        ));
    }
    Ok(count)
}

fn scene_color_4x_msaa_requested() -> Result<bool, String> {
    let Some(value) = std::env::var_os("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_MSAA") else {
        return Ok(false);
    };
    let value = value
        .into_string()
        .map_err(|_| "TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_MSAA must contain valid UTF-8".to_owned())?;
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "1" | "1x" => Ok(false),
        "4" | "4x" => Ok(true),
        _ => Err(format!(
            "TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_MSAA must be 1, 1x, 4, or 4x; got {value:?}"
        )),
    }
}

fn fixed_non_negative_f32(name: &str) -> Option<f32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(not(feature = "video"))]
fn reject_video_without_feature(
    plan: &SceneExecutionPlan,
    sources: &[crate::renderer::rendering_device::scene::RenderingDeviceSceneVideoSource],
) -> Result<Option<vulkan_renderer::VideoDecodeRequirements>, String> {
    let requires_video = plan
        .rendering_device_graph
        .sampled_bindings
        .iter()
        .any(|binding| binding.kind == crate::engine::scene::SceneRenderBindingKind::VideoFrame);
    if !requires_video && sources.is_empty() {
        Ok(None)
    } else {
        Err("scene VideoFrame sources require the video feature".into())
    }
}

fn accumulate_update(
    stats: &mut SharedFrameStats,
    update: super::frame_state::SceneFrameBufferUpdate,
) {
    stats.scene_color_attachment_clear_frame_count = stats
        .scene_color_attachment_clear_frame_count
        .saturating_add(u64::from(update.scene_color_attachment_clear.is_some()));
    stats.transform_uniform_update_count = stats
        .transform_uniform_update_count
        .saturating_add(u64::from(update.transform_uniform_updated));
    stats.effect_uniform_update_count = stats
        .effect_uniform_update_count
        .saturating_add(u64::from(update.material_uniform_updated));
    stats.skinning_storage_update_count = stats
        .skinning_storage_update_count
        .saturating_add(u64::from(update.skinning_storage_updated));
    stats.scene_owned_uniform_update_count = stats
        .scene_owned_uniform_update_count
        .saturating_add(u64::from(update.scene_owned_uniform_updated));
    stats.semantic_resolve_total_micros = stats
        .semantic_resolve_total_micros
        .saturating_add(update.cpu_timing.semantic_resolve_micros);
    stats.graph_update_total_micros = stats
        .graph_update_total_micros
        .saturating_add(update.cpu_timing.graph_update_micros);
    stats.transform_update_total_micros = stats
        .transform_update_total_micros
        .saturating_add(update.cpu_timing.transform_update_micros);
    stats.material_update_total_micros = stats
        .material_update_total_micros
        .saturating_add(update.cpu_timing.material_update_micros);
    stats.skinning_update_total_micros = stats
        .skinning_update_total_micros
        .saturating_add(update.cpu_timing.skinning_update_micros);
    stats.scene_owned_uniform_update_total_micros = stats
        .scene_owned_uniform_update_total_micros
        .saturating_add(update.cpu_timing.scene_owned_uniform_update_micros);
    stats.draw_policy_update_total_micros = stats
        .draw_policy_update_total_micros
        .saturating_add(update.cpu_timing.draw_policy_update_micros);
}

fn observe_present(stats: &mut SharedFrameStats, completed_at: Instant) {
    if let Some(previous) = stats.last_present_completed_at {
        let delta = completed_at
            .duration_since(previous)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        stats.present_delta_min_micros = Some(
            stats
                .present_delta_min_micros
                .map_or(delta, |value| value.min(delta)),
        );
        stats.present_delta_max_micros = Some(
            stats
                .present_delta_max_micros
                .map_or(delta, |value| value.max(delta)),
        );
        stats.present_delta_over_6250us_count = stats
            .present_delta_over_6250us_count
            .saturating_add(u64::from(delta > 6_250));
        stats.present_delta_over_8334us_count = stats
            .present_delta_over_8334us_count
            .saturating_add(u64::from(delta > 8_334));
    }
    stats.last_present_completed_at = Some(completed_at);
    stats.frames_presented = stats.frames_presented.saturating_add(1);
}
