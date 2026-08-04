//! Shared-renderer graphics-pipeline lowering for scene draws.

use std::ffi::CStr;

use vulkan_renderer::{
    AdvancedBlendState, Backend, BlendComponent, BlendFactor, BlendOperation, BlendState,
    ColorTargetState, ColorWrites, CullMode, FragmentState, FrontFace, GraphicsPipelineDescriptor,
    MachineCodeGraphicsPipeline, MachineCodeGraphicsPipelineDescriptor, MultisampleState,
    PipelineBinaryArchiveCache, PolygonMode, PrimitiveState, PrimitiveTopology, ProgrammableStage,
    SampleCount, ShaderBindingMap, ShaderModule, TextureFormat, VertexAttribute,
    VertexBufferLayout, VertexState, VertexStepMode,
};

use super::super::local_read::SceneLocalReadPipelineMetadata;
use super::{
    SceneColorWriteMask, SceneCullMode, SceneGpuBlend, ScenePipelineSamples,
    SceneVertexAttributePlan,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn create_graphics_pipeline(
    device: &Backend,
    target_format: TextureFormat,
    vertex_module: &ShaderModule,
    fragment_module: &ShaderModule,
    vertex_entry_point: &CStr,
    fragment_entry_point: &CStr,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vulkan_renderer::BlendOverlap,
    samples: ScenePipelineSamples,
    topology: PrimitiveTopology,
    default_mesh_vertex_input: bool,
    dynamic_text: bool,
    scene_owned_attributes: Option<&[SceneVertexAttributePlan]>,
    local_read_metadata: Option<&SceneLocalReadPipelineMetadata<'_>>,
    pipeline_binary_cache: &PipelineBinaryArchiveCache,
) -> Result<MachineCodeGraphicsPipeline, String> {
    if local_read_metadata.is_some() && blend.requires_advanced_operation() {
        return Err(
            "scene local-read pipeline does not have a proven advanced-blend attachment contract"
                .to_owned(),
        );
    }
    if local_read_metadata.is_some() && samples != ScenePipelineSamples::Single {
        return Err(
            "scene local-read pipeline does not have a proven multisampled attachment contract"
                .to_owned(),
        );
    }
    if dynamic_text && scene_owned_attributes.is_some() {
        return Err(
            "scene pipeline cannot combine dynamic-text and scene-owned vertex layouts".to_owned(),
        );
    }

    let attributes = vertex_attributes(
        default_mesh_vertex_input,
        dynamic_text,
        scene_owned_attributes,
    );
    let buffers = (!attributes.is_empty())
        .then_some(VertexBufferLayout {
            slot: if dynamic_text { 1 } else { 0 },
            array_stride: if dynamic_text {
                super::super::dynamic_text::DYNAMIC_TEXT_INSTANCE_STRIDE as u64
            } else {
                u64::from(super::super::SCENE_MESH_VERTEX_STRIDE_BYTES)
            },
            step_mode: if dynamic_text {
                VertexStepMode::Instance
            } else {
                VertexStepMode::Vertex
            },
            attributes: &attributes,
        })
        .into_iter()
        .collect::<Vec<_>>();
    let shader_bindings = ShaderBindingMap::default();
    let active_target = scene_color_target(blend, color_write_mask, target_format);
    let target_formats = local_read_metadata
        .map(SceneLocalReadPipelineMetadata::color_attachment_formats)
        .unwrap_or(std::slice::from_ref(&target_format));
    let active_attachments = local_read_metadata
        .map(SceneLocalReadPipelineMetadata::active_color_attachments)
        .unwrap_or_else(|| vec![true]);
    let targets = target_formats
        .iter()
        .zip(active_attachments)
        .map(|(format, active)| {
            active.then_some(ColorTargetState {
                format: *format,
                ..active_target
            })
        })
        .collect::<Vec<_>>();
    let local_read_mapping = local_read_metadata
        .map(|metadata| metadata.shared_mapping(device))
        .transpose()?;
    let advanced_blend = blend
        .requires_advanced_operation()
        .then_some(AdvancedBlendState {
            source_premultiplied: advanced_source_premultiplied,
            destination_premultiplied: false,
            overlap: advanced_blend_overlap,
        });
    device
        .create_machine_code_graphics_pipeline(&MachineCodeGraphicsPipelineDescriptor {
            pipeline: GraphicsPipelineDescriptor {
                label: Some("tensor-wallpaper-scene-graphics"),
                vertex: VertexState {
                    stage: ProgrammableStage {
                        module: vertex_module,
                        entry_point: vertex_entry_point,
                        bindings: &shader_bindings,
                    },
                    buffers: &buffers,
                },
                primitive: PrimitiveState {
                    topology,
                    primitive_restart_enable: false,
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: scene_cull_mode(cull_mode),
                    front_face: FrontFace::CounterClockwise,
                },
                depth_stencil: None,
                multisample: MultisampleState {
                    count: match samples {
                        ScenePipelineSamples::Single => SampleCount::One,
                        ScenePipelineSamples::SceneColor4x => SampleCount::Four,
                    },
                    mask: u64::MAX,
                    alpha_to_coverage_enabled: blend == SceneGpuBlend::AlphaToCoverage,
                },
                fragment: FragmentState {
                    stage: ProgrammableStage {
                        module: fragment_module,
                        entry_point: fragment_entry_point,
                        bindings: &shader_bindings,
                    },
                    targets: &targets,
                },
                advanced_blend,
                local_read_mapping: local_read_mapping.as_ref(),
                cache: None,
            },
            archive_cache: pipeline_binary_cache,
        })
        .map_err(|error| format!("create shared scene graphics pipeline: {error}"))
}

fn vertex_attributes(
    default_mesh_vertex_input: bool,
    dynamic_text: bool,
    scene_owned: Option<&[SceneVertexAttributePlan]>,
) -> Vec<VertexAttribute> {
    if dynamic_text {
        return vec![
            VertexAttribute {
                format: vulkan_renderer::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 5,
            },
            VertexAttribute {
                format: vulkan_renderer::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 6,
            },
        ];
    }
    if let Some(attributes) = scene_owned {
        return attributes
            .iter()
            .map(|attribute| VertexAttribute {
                format: attribute.format,
                offset: u64::from(attribute.offset),
                shader_location: attribute.location,
            })
            .collect();
    }
    if !default_mesh_vertex_input {
        return Vec::new();
    }
    vec![
        vertex_attribute(0, vulkan_renderer::VertexFormat::Float32x2, 0),
        vertex_attribute(1, vulkan_renderer::VertexFormat::Float32x2, 8),
        vertex_attribute(2, vulkan_renderer::VertexFormat::Float32, 16),
        vertex_attribute(3, vulkan_renderer::VertexFormat::Uint32x4, 20),
        vertex_attribute(4, vulkan_renderer::VertexFormat::Float32x4, 36),
    ]
}

const fn vertex_attribute(
    shader_location: u32,
    format: vulkan_renderer::VertexFormat,
    offset: u64,
) -> VertexAttribute {
    VertexAttribute {
        format,
        offset,
        shader_location,
    }
}

pub(super) const fn scene_cull_mode(cull_mode: SceneCullMode) -> CullMode {
    match cull_mode {
        SceneCullMode::None => CullMode::None,
        SceneCullMode::Normal => CullMode::Back,
    }
}

pub(super) fn scene_color_target(
    blend: SceneGpuBlend,
    write_mask: SceneColorWriteMask,
    format: TextureFormat,
) -> ColorTargetState {
    let blend = match blend {
        SceneGpuBlend::Replace | SceneGpuBlend::AlphaToCoverage => None,
        SceneGpuBlend::Alpha => Some(BlendState::ALPHA_BLENDING),
        SceneGpuBlend::Additive => Some(blend_state(
            BlendFactor::SourceAlpha,
            BlendFactor::One,
            BlendOperation::Add,
            BlendFactor::One,
            BlendFactor::One,
            BlendOperation::Add,
        )),
        SceneGpuBlend::Multiply => Some(advanced_blend_state(BlendOperation::Multiply)),
        SceneGpuBlend::MultiplyPremultiplied => Some(blend_state(
            BlendFactor::DestinationColor,
            BlendFactor::OneMinusSourceAlpha,
            BlendOperation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusSourceAlpha,
            BlendOperation::Add,
        )),
        SceneGpuBlend::Screen => Some(advanced_blend_state(BlendOperation::Screen)),
        SceneGpuBlend::ScreenPremultiplied => Some(blend_state(
            BlendFactor::One,
            BlendFactor::OneMinusSourceColor,
            BlendOperation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusSourceAlpha,
            BlendOperation::Add,
        )),
        SceneGpuBlend::Maximum => Some(blend_state(
            BlendFactor::One,
            BlendFactor::One,
            BlendOperation::Maximum,
            BlendFactor::One,
            BlendFactor::One,
            BlendOperation::Maximum,
        )),
        SceneGpuBlend::Modulate => Some(blend_state(
            BlendFactor::DestinationColor,
            BlendFactor::One,
            BlendOperation::Add,
            BlendFactor::Zero,
            BlendFactor::One,
            BlendOperation::Add,
        )),
        SceneGpuBlend::HslColor => Some(advanced_blend_state(BlendOperation::HslColor)),
    };
    ColorTargetState {
        format,
        blend,
        write_mask: match write_mask {
            SceneColorWriteMask::Rgb => ColorWrites::RGB,
            SceneColorWriteMask::Rgba => ColorWrites::ALL,
        },
    }
}

const fn advanced_blend_state(operation: BlendOperation) -> BlendState {
    blend_state(
        BlendFactor::One,
        BlendFactor::Zero,
        operation,
        BlendFactor::One,
        BlendFactor::Zero,
        operation,
    )
}

const fn blend_state(
    color_source: BlendFactor,
    color_destination: BlendFactor,
    color_operation: BlendOperation,
    alpha_source: BlendFactor,
    alpha_destination: BlendFactor,
    alpha_operation: BlendOperation,
) -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: color_source,
            dst_factor: color_destination,
            operation: color_operation,
        },
        alpha: BlendComponent {
            src_factor: alpha_source,
            dst_factor: alpha_destination,
            operation: alpha_operation,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::vertex_attributes;

    #[test]
    fn builtin_fullscreen_pipeline_declares_no_vertex_buffer_slot() {
        assert!(vertex_attributes(false, false, None).is_empty());
        assert_eq!(vertex_attributes(true, false, None).len(), 5);
        assert_eq!(vertex_attributes(false, true, None).len(), 2);
    }
}
