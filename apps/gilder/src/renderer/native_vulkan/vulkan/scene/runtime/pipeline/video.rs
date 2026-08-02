//! Exact native-Slang scene-video graphics pipeline.

use vulkan_renderer::{
    Backend, BlendState, ColorTargetState, ColorWrites, FragmentState, GraphicsPipelineDescriptor,
    MachineCodeGraphicsPipeline, MachineCodeGraphicsPipelineDescriptor, MultisampleState,
    PipelineBinaryArchiveCache, PrimitiveState, ProgrammableStage, ShaderBindingMap,
    ShaderModuleDescriptor, TextureFormat, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
};

use crate::engine::scene::{
    SceneRenderBindingKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess,
};

include!(concat!(env!("OUT_DIR"), "/gilder_scene_video_shaders.rs"));

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SceneVideoPipeline {
    pub pipeline: MachineCodeGraphicsPipeline,
}

pub(super) fn create_optional(
    device: &Backend,
    graph: &SceneRenderingDeviceGraphPlan,
    target_format: TextureFormat,
    scene_color_msaa_enabled: bool,
    archive_cache: &PipelineBinaryArchiveCache,
) -> Result<Option<SceneVideoPipeline>, String> {
    let mut required = false;
    for binding in graph
        .sampled_bindings
        .iter()
        .filter(|binding| binding.kind == SceneRenderBindingKind::VideoFrame)
    {
        required = true;
        if binding.access != SceneRenderingDeviceImageAccess::SampledImage {
            return Err("scene-video pipeline requires sampled-image access".into());
        }
        let pass = graph
            .pass_nodes
            .get(binding.pass_node_index as usize)
            .ok_or_else(|| {
                format!(
                    "scene-video media instance {} references missing pass {}",
                    binding.slot, binding.pass_node_index
                )
            })?;
        if !matches!(
            pass.target,
            SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
        ) {
            return Err(format!(
                "scene-video media instance {} targets unsupported {:?}",
                binding.slot, pass.target
            ));
        }
        if (pass.mesh_draw_start, pass.mesh_draw_count)
            != (binding.mesh_draw_start, binding.mesh_draw_count)
        {
            return Err(format!(
                "scene-video media instance {} binding range differs from pass {} draw range",
                binding.slot, binding.pass_node_index
            ));
        }
    }
    if !required {
        return Ok(None);
    }
    if scene_color_msaa_enabled {
        return Err("scene-video exact pipeline does not support multisampled SceneColor".into());
    }
    create(device, target_format, archive_cache).map(Some)
}

impl SceneVideoPipeline {
    pub(super) fn machine_code_metrics(&self) -> (usize, usize, bool) {
        (
            self.pipeline.archive().binaries.len(),
            self.pipeline
                .archive()
                .binaries
                .iter()
                .map(|binary| binary.data.len())
                .sum(),
            self.pipeline.archive_reused(),
        )
    }
}

pub(super) fn create(
    device: &Backend,
    target_format: TextureFormat,
    archive_cache: &PipelineBinaryArchiveCache,
) -> Result<SceneVideoPipeline, String> {
    if SCENE_VIDEO_LAYER_PUSH_BYTES != 24 {
        return Err(format!(
            "scene-video shader push ABI is {} bytes instead of 24",
            SCENE_VIDEO_LAYER_PUSH_BYTES
        ));
    }
    let vertex = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("gilder-scene-video-vertex".into()),
            spirv: SCENE_VIDEO_LAYER_VERTEX_SPIRV.to_vec(),
        })
        .map_err(|error| format!("create scene-video vertex shader: {error}"))?;
    let fragment = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("gilder-scene-video-fragment".into()),
            spirv: SCENE_VIDEO_LAYER_FRAGMENT_SPIRV.to_vec(),
        })
        .map_err(|error| format!("create scene-video fragment shader: {error}"))?;
    let attributes = [
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
        VertexAttribute {
            format: VertexFormat::Float32,
            offset: 16,
            shader_location: 2,
        },
    ];
    let buffers = [VertexBufferLayout {
        slot: 0,
        array_stride: u64::from(super::super::SCENE_VIDEO_VERTEX_STRIDE_BYTES),
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let bindings = ShaderBindingMap::default();
    let targets = [Some(ColorTargetState {
        format: target_format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device
        .create_machine_code_graphics_pipeline(&MachineCodeGraphicsPipelineDescriptor {
            pipeline: GraphicsPipelineDescriptor {
                label: Some("gilder-scene-video-layer"),
                vertex: VertexState {
                    stage: ProgrammableStage {
                        module: &vertex,
                        entry_point: c"main",
                        bindings: &bindings,
                    },
                    buffers: &buffers,
                },
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                fragment: FragmentState {
                    stage: ProgrammableStage {
                        module: &fragment,
                        entry_point: c"main",
                        bindings: &bindings,
                    },
                    targets: &targets,
                },
                advanced_blend: None,
                local_read_mapping: None,
                cache: None,
            },
            archive_cache,
        })
        .map_err(|error| format!("create scene-video machine-code pipeline: {error}"))?;
    Ok(SceneVideoPipeline { pipeline })
}
