use std::sync::{Arc, Mutex};
#[cfg(feature = "native-vulkan-video")]
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::core::{SceneSystemStatus, SceneTextureRegion};
use crate::renderer::{
    SceneRenderAudioCue, SceneWallpaperPlan, SceneWallpaperRuntimeSampledImageFrame,
    SceneWallpaperRuntimeSampler,
};

#[cfg(feature = "native-vulkan-video")]
use super::super::NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot;
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
    run_vulkanalia_ready_prefix_video_sources_with_scene_overlay,
};
use super::super::{
    NativeVulkanAudioOutputMode, NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVideoSessionCodec, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry,
    NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry,
    NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
    NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot,
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator,
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap, run_clear,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_solid_quad_present,
};
use super::runtime::{
    NativeVulkanSceneRuntimeSnapshot, native_vulkan_scene_runtime_snapshot,
    native_vulkan_scene_sampled_vertex_input_from_sampled_layers,
    native_vulkan_scene_solid_quad_geometry_input_from_layers,
    native_vulkan_scene_solid_quad_geometry_input_from_snapshot_layers,
};

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
    NativeVulkanVulkanaliaMultiStreamingVideoPresentDecodeSnapshot;

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

struct NativeVulkanSceneDynamicGeometryFrame {
    elapsed_ms: u64,
    solid_geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    sampled_geometry: Option<NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
}

struct NativeVulkanSceneDynamicGeometryCache {
    sampler: SceneWallpaperRuntimeSampler,
    base_time_ms: u64,
    include_solid_geometry: bool,
    cached: Option<NativeVulkanSceneDynamicGeometryFrame>,
}

impl NativeVulkanSceneDynamicGeometryCache {
    fn new(
        sampler: SceneWallpaperRuntimeSampler,
        base_time_ms: u64,
        include_solid_geometry: bool,
    ) -> Self {
        Self {
            sampler,
            base_time_ms,
            include_solid_geometry,
            cached: None,
        }
    }

    fn sampled_geometry(
        &mut self,
        elapsed_ms: u64,
    ) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
        self.ensure_frame(elapsed_ms)?;
        if self
            .cached
            .as_ref()
            .is_none_or(|frame| frame.sampled_geometry.is_none())
        {
            self.refresh_frame(elapsed_ms)?;
        }
        let geometry = self
            .cached
            .as_mut()
            .and_then(|frame| frame.sampled_geometry.take())
            .ok_or_else(|| {
                "dynamic scene geometry cache did not retain sampled geometry".to_owned()
            })?;
        self.drop_consumed_frame();
        Ok(geometry)
    }

    fn solid_geometry(
        &mut self,
        elapsed_ms: u64,
    ) -> Result<Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>, String> {
        self.ensure_frame(elapsed_ms)?;
        let geometry = self
            .cached
            .as_mut()
            .and_then(|frame| frame.solid_geometry.take());
        self.drop_consumed_frame();
        Ok(geometry)
    }

    fn ensure_frame(&mut self, elapsed_ms: u64) -> Result<(), String> {
        if self
            .cached
            .as_ref()
            .is_none_or(|frame| frame.elapsed_ms != elapsed_ms)
        {
            self.refresh_frame(elapsed_ms)?;
        }
        Ok(())
    }

    fn refresh_frame(&mut self, elapsed_ms: u64) -> Result<(), String> {
        let sample_time_ms = self.base_time_ms.saturating_add(elapsed_ms);
        let sampled_frame = self
            .sampler
            .sample_sampled_image_frame_reusing(sample_time_ms)
            .map_err(|err| format!("sample dynamic sampled image frame: {err}"))?;
        let sampled_geometry =
            native_vulkan_scene_sampled_geometry_input_from_runtime_sampled_image_frame(
                &sampled_frame,
            );
        self.sampler.recycle_sampled_image_frame(sampled_frame);
        let sampled_geometry = sampled_geometry?;
        let solid_geometry = if self.include_solid_geometry {
            let frame = self
                .sampler
                .sample_solid_snapshot_frame_reusing(sample_time_ms)
                .map_err(|err| format!("sample dynamic solid scene snapshot frame: {err}"))?;
            let geometry =
                native_vulkan_scene_solid_quad_geometry_input_from_snapshot_layers(&frame.layers);
            self.sampler.recycle_snapshot_frame(frame);
            Some(geometry?)
        } else {
            None
        };
        self.cached = Some(NativeVulkanSceneDynamicGeometryFrame {
            elapsed_ms,
            solid_geometry,
            sampled_geometry: Some(sampled_geometry),
        });
        native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
        Ok(())
    }

    fn drop_consumed_frame(&mut self) {
        if self
            .cached
            .as_ref()
            .is_some_and(|frame| frame.sampled_geometry.is_none() && frame.solid_geometry.is_none())
        {
            self.cached = None;
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
        }
    }
}

fn native_vulkan_scene_sampled_geometry_input_from_runtime_sampled_image_frame(
    frame: &SceneWallpaperRuntimeSampledImageFrame,
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    native_vulkan_scene_sampled_vertex_input_from_sampled_layers(&frame.layers)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoBridgeSourceOptions {
    pub source: std::path::PathBuf,
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub bitstream_extract_max_samples: u32,
    pub ready_prefix_frames: u32,
    pub playback_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoBridgeOptions {
    pub sources: Vec<NativeVulkanSceneVideoBridgeSourceOptions>,
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
    let native_effect_runtime_active = matches!(
        plan.scene_systems.shader_material_graph,
        SceneSystemStatus::Detected | SceneSystemStatus::Ready
    );
    plan.timeline_animation_count > 0
        || plan.timeline_animated_layer_count > 0
        || plan.puppet_animation_layer_count > 0
        || particle_runtime_active
        || native_effect_runtime_active
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
    let sampler = Arc::new(Mutex::new(sampler));
    Box::new(move |elapsed_ms| {
        let result = (|| {
            let mut sampler = sampler
                .lock()
                .map_err(|_| "dynamic solid scene sampler is poisoned".to_owned())?;
            let frame = sampler
                .sample_frame_reusing(base_time_ms.saturating_add(elapsed_ms))
                .map_err(|err| format!("sample dynamic solid scene: {err}"))?;
            let geometry = native_vulkan_scene_solid_quad_geometry_input_from_layers(
                frame.snapshot_time_ms,
                frame.scene_size,
                frame.scene_fit,
                &frame.layers,
            );
            sampler.recycle_frame(frame);
            geometry
        })();
        native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
        result
    })
}

fn native_vulkan_scene_dynamic_mixed_solid_geometry_from_sampler(
    cache: Arc<Mutex<NativeVulkanSceneDynamicGeometryCache>>,
) -> NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry {
    Box::new(move |elapsed_ms| {
        cache
            .lock()
            .map_err(|_| "dynamic mixed scene geometry cache is poisoned".to_owned())?
            .solid_geometry(elapsed_ms)
    })
}

fn native_vulkan_scene_dynamic_sampled_geometry_from_cache(
    cache: Arc<Mutex<NativeVulkanSceneDynamicGeometryCache>>,
) -> NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry {
    Box::new(move |elapsed_ms| {
        cache
            .lock()
            .map_err(|_| "dynamic sampled scene geometry cache is poisoned".to_owned())?
            .sampled_geometry(elapsed_ms)
    })
}

fn native_vulkan_scene_dynamic_sampled_geometry_pair(
    plan: &SceneWallpaperPlan,
    include_solid_geometry: bool,
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
    let include_solid_geometry =
        include_solid_geometry && sampler.dynamic_solid_geometry_required();
    let cache = Arc::new(Mutex::new(NativeVulkanSceneDynamicGeometryCache::new(
        sampler,
        base_time_ms,
        include_solid_geometry,
    )));
    Ok((
        include_solid_geometry.then(|| {
            native_vulkan_scene_dynamic_mixed_solid_geometry_from_sampler(Arc::clone(&cache))
        }),
        Some(native_vulkan_scene_dynamic_sampled_geometry_from_cache(
            cache,
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

    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator();

    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps =
        native_vulkan_scene_effective_target_max_fps(options.target_max_fps, plan.target_max_fps);
    options.target_max_fps = target_max_fps;
    let render_item = native_vulkan_scene_item(&plan);
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
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
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
                .take_vulkanalia_solid_quad_geometry_input()
                .ok_or_else(|| {
                    NativeVulkanError::Scene(format!(
                        "scene draw plan is not solid-quad recordable: {}",
                        runtime.draw_pass_backend_status
                    ))
                })?;
            let dynamic_geometry = native_vulkan_scene_dynamic_solid_geometry(&plan)?;
            let scene_size = runtime.scene_size;
            let scene_fit = runtime.scene_fit;
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

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
            let (source, fit, geometry) = if let Some((source, geometry)) =
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
            let solid_geometry = runtime.take_vulkanalia_mixed_solid_quad_geometry_input();
            let (dynamic_solid_geometry, dynamic_geometry) =
                native_vulkan_scene_dynamic_sampled_geometry_pair(&plan, solid_geometry.is_some())?;
            let scene_size = runtime.scene_size;
            let scene_fit = runtime.scene_fit;
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

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
            let video_bridge = video_bridge.ok_or_else(|| {
                NativeVulkanError::Scene(
                    "scene video layer requires Vulkan Video bridge options".to_owned(),
                )
            })?;
            let video_geometry = runtime.take_vulkanalia_video_layer_geometry_input();
            let video_bridge_sources = video_bridge.sources.clone();
            if video_bridge_sources.is_empty() {
                return Err(NativeVulkanError::Scene(
                    "scene video bridge requires at least one source".to_owned(),
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
            let solid_geometry = runtime.take_vulkanalia_mixed_solid_quad_geometry_input();
            let (dynamic_solid_geometry, dynamic_geometry) = if overlay_source.is_some()
                || overlay_geometry.is_some()
            {
                native_vulkan_scene_dynamic_sampled_geometry_pair(&plan, solid_geometry.is_some())?
            } else if solid_geometry.is_some() {
                let dynamic_solid =
                    native_vulkan_scene_dynamic_solid_geometry(&plan)?.map(|dynamic_geometry| {
                        Box::new(move |elapsed_ms| dynamic_geometry(elapsed_ms).map(Some))
                            as NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry
                    });
                (dynamic_solid, None)
            } else {
                (None, None)
            };
            let scene_video_overlay = (overlay_source.is_some()
                || overlay_geometry.is_some()
                || solid_geometry.is_some()
                || video_geometry.is_some())
            .then_some(NativeVulkanVulkanaliaSceneVideoOverlayInput {
                video_geometry,
                source: overlay_source,
                clear_color: options.clear_color,
                fit: overlay_fit,
                solid_geometry,
                geometry: overlay_geometry,
                dynamic_solid_geometry,
                dynamic_geometry,
                scene_size: runtime.scene_size,
                scene_fit: runtime.scene_fit,
            });
            runtime.release_cpu_draw_payloads_for_present();
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

            let (present, scene_audio) = native_vulkan_scene_present_with_audio(
                &plan,
                duration,
                scene_audio_output_mode,
                || {
                    run_vulkanalia_ready_prefix_video_sources_with_scene_overlay(
                        options,
                        video_bridge_sources,
                        video_bridge.audio_clock_probe_requested,
                        video_bridge.audio_output_mode,
                        scene_video_overlay,
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
    fn puppet_animation_layers_enable_dynamic_scene_sampling() {
        let mut image = layer("puppet", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/puppet.gtex"));

        let mut plan = plan(vec![image]);
        plan.puppet_animation_layer_count = 1;

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
}
