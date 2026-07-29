use std::path::PathBuf;

use serde::Serialize;

use crate::config::VideoDecoderPolicy;
use crate::core::{
    FitMode, SceneNodeKind, SceneSize, SceneSystems, SceneTextureRegion, SceneTransform, Transition,
};
use crate::engine::scene::SceneEngineRenderPlan;
use crate::renderer::{
    SceneDisplayPlan, SceneRenderLayer, SceneWallpaperPlan, SlideshowWallpaperPlan,
    StaticRenderSyncPlan, StaticWallpaperPlan, VideoWallpaperPlan,
};

use super::super::NativeVulkanWallpaperType;

const NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS: &str =
    "static-image-lowered-to-scene-sampled-image-layer";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NativeVulkanRenderItem {
    Clear {
        output_name: String,
    },
    Video {
        output_name: String,
        source: PathBuf,
        poster: Option<PathBuf>,
        fit: FitMode,
        loop_playback: bool,
        muted: bool,
        manifest_max_fps: Option<u32>,
        target_max_fps: Option<u32>,
        decoder_policy: VideoDecoderPolicy,
        start_offset_ms: u64,
        renderer_status: &'static str,
    },
    Slideshow {
        output_name: String,
        sources: Vec<PathBuf>,
        interval_ms: u64,
        transition: Transition,
        fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
    Scene {
        output_name: String,
        scene_source: Option<PathBuf>,
        display: Option<SceneDisplayPlan>,
        display_image: Option<PathBuf>,
        display_color: Option<String>,
        manifest_max_fps: Option<u32>,
        layer_count: usize,
        layers: Vec<SceneRenderLayer>,
        scene_systems: SceneSystems,
        audio_cue_count: usize,
        bound_properties: Vec<String>,
        timeline_animation_count: usize,
        timeline_animated_layer_count: usize,
        puppet_animation_layer_count: usize,
        property_binding_count: usize,
        cursor_parallax_input_ready: bool,
        dynamic_topology_required: bool,
        scene_engine: Option<SceneEngineRenderPlan>,
        scene_scenescript_binding_count: usize,
        scene_material_graph_count: usize,
        scene_material_graph_resource_count: usize,
        scene_effect_graph_count: usize,
        scene_mesh_count: usize,
        scene_mesh_vertex_count: usize,
        scene_mesh_index_count: usize,
        scene_audio_response_binding_count: usize,
        unsupported_scene_features: Vec<String>,
        snapshot_time_ms: u64,
        scene_size: Option<SceneSize>,
        scene_fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
}

impl NativeVulkanRenderItem {
    pub fn wallpaper_type(&self) -> NativeVulkanWallpaperType {
        match self {
            Self::Clear { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::Video { .. } => NativeVulkanWallpaperType::Video,
            Self::Slideshow { .. } => NativeVulkanWallpaperType::Playlist,
            Self::Scene { .. } => NativeVulkanWallpaperType::Scene,
        }
    }
}

pub fn render_items_from_sync_plan(plan: &StaticRenderSyncPlan) -> Vec<NativeVulkanRenderItem> {
    plan.plans
        .iter()
        .map(native_vulkan_static_scene_item)
        .chain(plan.video_plans.iter().map(native_vulkan_video_item))
        .chain(
            plan.slideshow_plans
                .iter()
                .map(native_vulkan_slideshow_item),
        )
        .chain(plan.scene_plans.iter().map(native_vulkan_scene_item))
        .collect()
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_static_scene_item(
    plan: &StaticWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Scene {
        output_name: plan.output_name.clone(),
        scene_source: None,
        display: Some(SceneDisplayPlan::Image {
            source: plan.source.clone(),
            fit: plan.fit,
            background: plan.background.clone(),
        }),
        display_image: Some(plan.source.clone()),
        display_color: None,
        manifest_max_fps: None,
        layer_count: 1,
        layers: vec![SceneRenderLayer {
            id: "static-image".to_owned(),
            kind: SceneNodeKind::Image,
            source: Some(plan.source.clone()),
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
            path_fill_rule: crate::core::ScenePathFillRule::default(),
            fit: plan.fit,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }],
        scene_systems: SceneSystems::default(),
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count: 0,
        timeline_animated_layer_count: 0,
        puppet_animation_layer_count: 0,
        property_binding_count: 0,
        cursor_parallax_input_ready: false,
        dynamic_topology_required: false,
        scene_engine: None,
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: 0,
        scene_material_graph_resource_count: 0,
        scene_effect_graph_count: 0,
        scene_mesh_count: 0,
        scene_mesh_vertex_count: 0,
        scene_mesh_index_count: 0,
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        snapshot_time_ms: 0,
        scene_size: None,
        scene_fit: plan.fit,
        target_max_fps: None,
        renderer_status: NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS,
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_video_item(
    plan: &VideoWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Video {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        poster: plan.poster.clone(),
        fit: plan.fit,
        loop_playback: plan.loop_playback,
        muted: plan.muted,
        manifest_max_fps: plan.manifest_max_fps,
        target_max_fps: plan.target_max_fps,
        decoder_policy: plan.decoder_policy,
        start_offset_ms: plan.start_offset_ms,
        renderer_status: "vulkan-lifecycle-video-placeholder",
    }
}

fn native_vulkan_slideshow_item(plan: &SlideshowWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Slideshow {
        output_name: plan.output_name.clone(),
        sources: plan.sources.clone(),
        interval_ms: plan.interval_ms,
        transition: plan.transition,
        fit: plan.fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-slideshow-static-texture-sequence",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        INVALID_MATERIAL_ID, INVALID_OBJECT_ID, RendererSceneRenderPlan, SceneMaterialHandle,
        SceneObjectHandle, SceneRenderPassKind, SceneRenderTargetKind,
        SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw,
        SceneRenderingDevicePassNode,
    };

    #[test]
    fn static_wallpaper_lowers_to_single_image_scene_layer() {
        let plan = StaticWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/static.png"),
            fit: FitMode::Contain,
            background: Some("#010203".to_owned()),
        };

        let item = native_vulkan_static_scene_item(&plan);

        let NativeVulkanRenderItem::Scene {
            output_name,
            scene_source,
            display,
            display_image,
            layer_count,
            layers,
            bound_properties,
            renderer_status,
            ..
        } = item
        else {
            panic!("static image should lower to a scene render item");
        };
        assert_eq!(output_name, "HDMI-A-1");
        assert_eq!(scene_source, None);
        assert_eq!(display_image, Some(PathBuf::from("/tmp/static.png")));
        assert_eq!(
            display,
            Some(SceneDisplayPlan::Image {
                source: PathBuf::from("/tmp/static.png"),
                fit: FitMode::Contain,
                background: Some("#010203".to_owned()),
            })
        );
        assert_eq!(layer_count, 1);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].kind, SceneNodeKind::Image);
        assert_eq!(layers[0].source, Some(PathBuf::from("/tmp/static.png")));
        assert_eq!(layers[0].fit, FitMode::Contain);
        assert!(bound_properties.is_empty());
        assert_eq!(renderer_status, NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS);
    }

    #[test]
    fn scene_engine_binary_item_reports_rendering_device_graph_status() {
        let plan = SceneWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: Some(PathBuf::from("/tmp/scene.gscene")),
            manifest_max_fps: Some(60),
            target_max_fps: Some(30),
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
            scene_engine: Some(SceneEngineRenderPlan {
                renderer_scene_render: RendererSceneRenderPlan {
                    object_count: 1,
                    visible_object_count: 1,
                    resource_count: 1,
                    texture_count: 0,
                    material_count: 1,
                    mesh_count: 1,
                    visible_mesh_binding_count: 1,
                    mesh_vertex_count: 4,
                    mesh_index_count: 6,
                    puppet_binding_count: 0,
                    visible_puppet_binding_count: 0,
                    puppet_bone_palette_count: 0,
                    puppet_bone_matrix_count: 0,
                    visible_puppet_bone_matrix_count: 0,
                    attachment_link_count: 0,
                    effect_count: 0,
                    visible_effect_instance_count: 0,
                    visible_effect_pass_count: 0,
                    visible_effect_fbo_count: 0,
                    render_graph_count: 1,
                    render_pass_count: 1,
                    render_binding_count: 0,
                    image_target_count: 0,
                    shader_contract_count: 1,
                    resource_payload_bytes: 2,
                    descriptor_heap_required: true,
                    descriptor_heap_resource_count: 1,
                    descriptor_heap_sampled_image_count: 0,
                    descriptor_heap_uniform_buffer_count: 1,
                    descriptor_heap_input_attachment_count: 0,
                    descriptor_heap_storage_buffer_count: 0,
                    descriptor_heap_sampler_count: 0,
                    fifo_latest_ready_present_required: true,
                },
                rendering_device_graph: SceneRenderingDeviceGraphPlan {
                    pass_nodes: vec![SceneRenderingDevicePassNode {
                        graph_index: 0,
                        graph_activation_policy:
                            crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
                        pass_record_index: 0,
                        pass_id: 0,
                        role: SceneRenderPassKind::BaseMaterial,
                        target: SceneRenderTargetKind::SceneColor,
                        target_name: crate::engine::scene::SceneStringId::NONE,
                        binding_start: 0,
                        binding_count: 0,
                        effect_binding_start: u32::MAX,
                        effect_binding_count: 0,
                        effect_visibility_policy:
                            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                        mesh_draw_start: 0,
                        mesh_draw_count: 1,
                    }],
                    target_allocations: Vec::new(),
                    effect_batches: Vec::new(),
                    effect_batch_instances: Vec::new(),
                    sampled_bindings: Vec::new(),
                    material_sampled_bindings: Vec::new(),
                    mesh_draws: vec![SceneRenderingDeviceMeshDraw {
                        primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
                        shader_key: crate::engine::scene::SceneStringId::NONE,
                        mesh_index: 0,
                        resolved_object_index: 0,
                        render_world_matrix: identity_clip_transform(),
                        clip_transform: identity_clip_transform(),
                        authored_source_extent: [0.0; 2],
                        skinning_palette_start: INVALID_OBJECT_ID,
                        skinning_palette_count: 0,
                        resolved_color: crate::engine::scene::SceneVec3 {
                            x: 1.0,
                            y: 1.0,
                            z: 1.0,
                        },
                        resolved_alpha: 1.0,
                        apply_resolved_visual: true,
                        effect_batch_atlas_tile: u32::MAX,
                        effect_batch_atlas_grid: [0; 2],
                        effect_binding_start: u32::MAX,
                        effect_binding_count: 0,
                        effect_visibility_policy:
                            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                        resolved_effect_visibility_mask: 0,
                        object: SceneObjectHandle(0),
                        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                        vertex_start: 0,
                        vertex_count: 4,
                        index_start: 0,
                        index_count: 6,
                        instance_count: 1,
                    }],
                    puppet_bone_palettes: Vec::new(),
                    puppet_bone_matrices: Vec::new(),
                    particle_gpu_emitters: Vec::new(),
                    resolved_object_count: 1,
                    resolved_visible_object_count: 1,
                    resolved_attachment_link_count: 0,
                    resolved_visible_effect_instance_count: 0,
                    resolved_visible_effect_pass_count: 0,
                    resolved_visible_effect_fbo_count: 0,
                    descriptor_heap_required: true,
                    descriptor_heap_resource_count: 1,
                    descriptor_heap_sampled_image_count: 0,
                    descriptor_heap_uniform_buffer_count: 1,
                    descriptor_heap_storage_buffer_count: 0,
                    descriptor_heap_sampler_count: 0,
                    graph_physical_target_count: 0,
                    graph_aliased_target_count: 0,
                    fifo_latest_ready_present_required: true,
                },
            }),
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 1,
            scene_material_graph_resource_count: 1,
            scene_effect_graph_count: 1,
            scene_mesh_count: 1,
            scene_mesh_vertex_count: 4,
            scene_mesh_index_count: 6,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            display: None,
            layers: Vec::new(),
        };

        let item = native_vulkan_scene_item(&plan);

        let NativeVulkanRenderItem::Scene {
            renderer_status,
            scene_mesh_count,
            scene_mesh_vertex_count,
            scene_mesh_index_count,
            scene_engine,
            ..
        } = item
        else {
            panic!("scene plan should lower to a scene render item");
        };
        assert_eq!(
            renderer_status,
            "scene-engine-binary-ready-for-rendering-device-graph"
        );
        assert_eq!(scene_mesh_count, 1);
        assert_eq!(scene_mesh_vertex_count, 4);
        assert_eq!(scene_mesh_index_count, 6);
        let scene_engine = scene_engine.expect("scene engine render plan");
        assert_eq!(scene_engine.renderer_scene_render.mesh_count, 1);
        assert_eq!(
            scene_engine
                .renderer_scene_render
                .descriptor_heap_resource_count,
            1
        );
        assert!(
            scene_engine
                .renderer_scene_render
                .fifo_latest_ready_present_required
        );
        assert_eq!(scene_engine.rendering_device_graph.pass_nodes.len(), 1);
        assert_eq!(scene_engine.rendering_device_graph.mesh_draws.len(), 1);
        assert_eq!(
            scene_engine.rendering_device_graph.mesh_draws[0].index_count,
            6
        );
    }

    fn identity_clip_transform() -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_item(
    plan: &SceneWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Scene {
        output_name: plan.output_name.clone(),
        scene_source: plan.source.clone(),
        display: plan.display.clone(),
        display_image: match &plan.display {
            Some(SceneDisplayPlan::Image { source, .. }) => Some(source.clone()),
            Some(SceneDisplayPlan::Color { .. }) | None => None,
        },
        display_color: match &plan.display {
            Some(SceneDisplayPlan::Color { color }) => Some(color.clone()),
            Some(SceneDisplayPlan::Image { .. }) | None => None,
        },
        manifest_max_fps: plan.manifest_max_fps,
        layer_count: plan.layers.len(),
        layers: plan.layers.clone(),
        scene_systems: plan.scene_systems.clone(),
        audio_cue_count: plan.audio_cue_count,
        bound_properties: plan.bound_properties.clone(),
        timeline_animation_count: plan.timeline_animation_count,
        timeline_animated_layer_count: plan.timeline_animated_layer_count,
        puppet_animation_layer_count: plan.puppet_animation_layer_count,
        property_binding_count: plan.property_binding_count,
        cursor_parallax_input_ready: plan.cursor_parallax_input_ready,
        dynamic_topology_required: native_vulkan_scene_plan_requires_dynamic_topology(plan),
        scene_engine: plan.scene_engine.clone(),
        scene_scenescript_binding_count: plan.scene_scenescript_binding_count,
        scene_material_graph_count: plan.scene_material_graph_count,
        scene_material_graph_resource_count: plan.scene_material_graph_resource_count,
        scene_effect_graph_count: plan.scene_effect_graph_count,
        scene_mesh_count: plan.scene_mesh_count,
        scene_mesh_vertex_count: plan.scene_mesh_vertex_count,
        scene_mesh_index_count: plan.scene_mesh_index_count,
        scene_audio_response_binding_count: plan.scene_audio_response_binding_count,
        unsupported_scene_features: plan.unsupported_scene_features.clone(),
        snapshot_time_ms: plan.snapshot_time_ms,
        scene_size: plan.scene_size,
        scene_fit: plan.scene_fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: native_vulkan_scene_renderer_status(plan),
    }
}

fn native_vulkan_scene_renderer_status(plan: &SceneWallpaperPlan) -> &'static str {
    if plan.layers.is_empty()
        && plan
            .source
            .as_ref()
            .and_then(|source| source.extension())
            .is_some_and(|extension| extension == "gscene")
    {
        "scene-engine-binary-ready-for-rendering-device-graph"
    } else {
        "deterministic-scene-snapshot-ready-for-vulkan-passes"
    }
}

fn native_vulkan_scene_plan_requires_dynamic_topology(plan: &SceneWallpaperPlan) -> bool {
    let particle_runtime_active = matches!(
        plan.scene_systems.particles,
        crate::core::scene::SceneSystemStatus::Detected
            | crate::core::scene::SceneSystemStatus::Ready
    );
    let native_effect_runtime_active = matches!(
        plan.scene_systems.shader_material_graph,
        crate::core::scene::SceneSystemStatus::Detected
            | crate::core::scene::SceneSystemStatus::Ready
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
