use std::ffi::CStr;

use thiserror::Error;
use vulkan_renderer::{
    BlendState, ColorTargetState, ColorWrites, Device as RendererDevice, Error as RendererError,
    FragmentState, GraphicsPipeline, GraphicsPipelineDescriptor, MultisampleState, PrimitiveState,
    ProgrammableStage, ShaderBindingMap, ShaderModuleDescriptor, ShaderModuleError, TextureFormat,
    VertexState,
};

const CLIENT_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/client.vert.spv");
const CLIENT_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/client.frag.spv");
const CURSOR_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/cursor.vert.spv");
const CURSOR_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/cursor.frag.spv");
const FOCUS_RING_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/focus_ring.vert.spv");
const FOCUS_RING_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/focus_ring.frag.spv");
const ENTRY_POINT: &CStr = c"main";

const CLIENT_IMAGE_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-client-image",
    vertex_label: "tensor-client-vertex",
    fragment_label: "tensor-client-fragment",
    vertex: CLIENT_VERTEX_SHADER,
    fragment: CLIENT_FRAGMENT_SHADER,
};
const CURSOR_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-cursor",
    vertex_label: "tensor-cursor-vertex",
    fragment_label: "tensor-cursor-fragment",
    vertex: CURSOR_VERTEX_SHADER,
    fragment: CURSOR_FRAGMENT_SHADER,
};
const FOCUS_RING_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-focus-ring",
    vertex_label: "tensor-focus-ring-vertex",
    fragment_label: "tensor-focus-ring-fragment",
    vertex: FOCUS_RING_VERTEX_SHADER,
    fragment: FOCUS_RING_FRAGMENT_SHADER,
};

#[derive(Clone, Copy)]
struct PipelineProgram {
    label: &'static str,
    vertex_label: &'static str,
    fragment_label: &'static str,
    vertex: &'static [u32],
    fragment: &'static [u32],
}

/// Shared typed pipeline for descriptor-heap client-surface sampling.
///
/// The compositor supplies only its output format. Shader-module ownership,
/// dynamic-rendering state, null-layout descriptor-heap ABI, and pipeline
/// destruction remain in `vulkan-renderer`.
pub(super) struct ClientImagePipeline {
    pipeline: GraphicsPipeline,
}

/// Shared descriptor-heap pipeline for Tensor's analytic fallback cursor.
pub(super) struct CursorPipeline {
    pipeline: GraphicsPipeline,
}

/// Shared descriptor-heap pipeline for Tensor's compositor-owned focus ring.
pub(super) struct FocusRingPipeline {
    pipeline: GraphicsPipeline,
}

/// Validates every SPIR-V module used by the compositor before the first
/// output can be registered. Pipeline creation stays cold and retained; frame
/// recording only selects a previously materialized pipeline by output format.
pub(super) fn validate_shader_modules(
    renderer: &RendererDevice,
) -> Result<(), TensorPipelineError> {
    for program in [CLIENT_IMAGE_PROGRAM, CURSOR_PROGRAM, FOCUS_RING_PROGRAM] {
        validate_program(renderer, program)?;
    }
    Ok(())
}

impl ClientImagePipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, CLIENT_IMAGE_PROGRAM)?,
        })
    }

    pub(super) const fn pipeline(&self) -> &GraphicsPipeline {
        &self.pipeline
    }
}

impl CursorPipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, CURSOR_PROGRAM)?,
        })
    }

    pub(super) const fn pipeline(&self) -> &GraphicsPipeline {
        &self.pipeline
    }
}

impl FocusRingPipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, FOCUS_RING_PROGRAM)?,
        })
    }

    pub(super) const fn pipeline(&self) -> &GraphicsPipeline {
        &self.pipeline
    }
}

fn validate_program(
    renderer: &RendererDevice,
    program: PipelineProgram,
) -> Result<(), TensorPipelineError> {
    let _vertex =
        renderer.create_shader_module(shader_descriptor(program.vertex_label, program.vertex))?;
    let _fragment = renderer
        .create_shader_module(shader_descriptor(program.fragment_label, program.fragment))?;
    Ok(())
}

fn create_pipeline(
    renderer: &RendererDevice,
    target_format: TextureFormat,
    program: PipelineProgram,
) -> Result<GraphicsPipeline, TensorPipelineError> {
    let vertex =
        renderer.create_shader_module(shader_descriptor(program.vertex_label, program.vertex))?;
    let fragment = renderer
        .create_shader_module(shader_descriptor(program.fragment_label, program.fragment))?;
    let bindings = ShaderBindingMap::default();
    let targets = [Some(ColorTargetState {
        format: target_format,
        blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: ColorWrites::ALL,
    })];
    renderer
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some(program.label),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex,
                    entry_point: ENTRY_POINT,
                    bindings: &bindings,
                },
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: FragmentState {
                stage: ProgrammableStage {
                    module: &fragment,
                    entry_point: ENTRY_POINT,
                    bindings: &bindings,
                },
                targets: &targets,
            },
            advanced_blend: None,
            local_read_mapping: None,
            cache: None,
        })
        .map_err(TensorPipelineError::from)
}

fn shader_descriptor(label: &str, spirv: &[u32]) -> ShaderModuleDescriptor {
    ShaderModuleDescriptor {
        label: Some(label.into()),
        spirv: spirv.to_vec(),
    }
}

#[derive(Debug, Error)]
pub(super) enum TensorPipelineError {
    #[error("shared renderer rejected a Tensor shader module: {0}")]
    ShaderModule(#[from] ShaderModuleError),
    #[error("shared renderer failed to create a Tensor graphics pipeline: {0}")]
    Pipeline(#[from] RendererError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_pipeline_shader_descriptors_own_complete_spirv_words() {
        for program in [CLIENT_IMAGE_PROGRAM, CURSOR_PROGRAM, FOCUS_RING_PROGRAM] {
            let vertex = shader_descriptor(program.vertex_label, program.vertex);
            let fragment = shader_descriptor(program.fragment_label, program.fragment);
            assert!(vertex.validate().is_ok());
            assert!(fragment.validate().is_ok());
            assert_eq!(vertex.spirv.as_slice(), program.vertex);
            assert_eq!(fragment.spirv.as_slice(), program.fragment);
        }
    }
}
