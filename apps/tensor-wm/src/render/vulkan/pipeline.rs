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
const MANAGED_CLIENT_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/client_color.frag.spv");
const CURSOR_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/cursor.vert.spv");
const CURSOR_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/cursor.frag.spv");
const FOCUS_RING_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/focus_ring.vert.spv");
const FOCUS_RING_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/focus_ring.frag.spv");
const SHADOW_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/shadow.vert.spv");
const SHADOW_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/shadow.frag.spv");
const BACKDROP_FILTER_VERTEX_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/backdrop_filter.vert.spv");
const BACKDROP_FILTER_FRAGMENT_SHADER: &[u32] =
    vulkan_renderer::include_spirv!("../../../shaders/spirv/backdrop_filter.frag.spv");
const ENTRY_POINT: &CStr = c"main";

const CLIENT_IMAGE_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-client-image",
    vertex_label: "tensor-client-vertex",
    fragment_label: "tensor-client-fragment",
    vertex: CLIENT_VERTEX_SHADER,
    fragment: CLIENT_FRAGMENT_SHADER,
    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
};
const MANAGED_CLIENT_IMAGE_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-managed-client-image",
    vertex_label: "tensor-client-vertex",
    fragment_label: "tensor-managed-client-fragment",
    vertex: CLIENT_VERTEX_SHADER,
    fragment: MANAGED_CLIENT_FRAGMENT_SHADER,
    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
};
const CURSOR_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-cursor",
    vertex_label: "tensor-cursor-vertex",
    fragment_label: "tensor-cursor-fragment",
    vertex: CURSOR_VERTEX_SHADER,
    fragment: CURSOR_FRAGMENT_SHADER,
    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
};
const FOCUS_RING_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-focus-ring",
    vertex_label: "tensor-focus-ring-vertex",
    fragment_label: "tensor-focus-ring-fragment",
    vertex: FOCUS_RING_VERTEX_SHADER,
    fragment: FOCUS_RING_FRAGMENT_SHADER,
    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
};
const SHADOW_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-shadow",
    vertex_label: "tensor-shadow-vertex",
    fragment_label: "tensor-shadow-fragment",
    vertex: SHADOW_VERTEX_SHADER,
    fragment: SHADOW_FRAGMENT_SHADER,
    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
};
const BACKDROP_FILTER_PROGRAM: PipelineProgram = PipelineProgram {
    label: "tensor-backdrop-filter",
    vertex_label: "tensor-backdrop-filter-vertex",
    fragment_label: "tensor-backdrop-filter-fragment",
    vertex: BACKDROP_FILTER_VERTEX_SHADER,
    fragment: BACKDROP_FILTER_FRAGMENT_SHADER,
    blend: None,
};

#[derive(Clone, Copy)]
struct PipelineProgram {
    label: &'static str,
    vertex_label: &'static str,
    fragment_label: &'static str,
    vertex: &'static [u32],
    fragment: &'static [u32],
    blend: Option<BlendState>,
}

/// Shared typed pipeline for descriptor-heap client-surface sampling.
///
/// The compositor supplies only its output format. Shader-module ownership,
/// dynamic-rendering state, null-layout descriptor-heap ABI, and pipeline
/// destruction remain in `vulkan-renderer`.
pub(super) struct ClientImagePipeline {
    pipeline: GraphicsPipeline,
}

/// Parallel client pipeline for non-identity per-surface color transforms.
/// The identity pipeline remains byte-for-byte unchanged and pays no managed
/// color branch or push-data cost.
pub(super) struct ManagedClientImagePipeline {
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

/// Descriptor-free analytic rounded-rectangle shadow pipeline.
pub(super) struct ShadowPipeline {
    pipeline: GraphicsPipeline,
}

/// Retained fixed-cost separable filter shared by horizontal and vertical
/// region-local backdrop passes.
pub(super) struct BackdropFilterPipeline {
    pipeline: GraphicsPipeline,
}

/// Validates every SPIR-V module used by the compositor before the first
/// output can be registered. Pipeline creation stays cold and retained; frame
/// recording only selects a previously materialized pipeline by output format.
pub(super) fn validate_shader_modules(
    renderer: &RendererDevice,
) -> Result<(), TensorPipelineError> {
    for program in tensor_programs() {
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

impl ManagedClientImagePipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, MANAGED_CLIENT_IMAGE_PROGRAM)?,
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

impl ShadowPipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, SHADOW_PROGRAM)?,
        })
    }

    pub(super) const fn pipeline(&self) -> &GraphicsPipeline {
        &self.pipeline
    }
}

impl BackdropFilterPipeline {
    pub(super) fn new(
        renderer: &RendererDevice,
        target_format: TextureFormat,
    ) -> Result<Self, TensorPipelineError> {
        Ok(Self {
            pipeline: create_pipeline(renderer, target_format, BACKDROP_FILTER_PROGRAM)?,
        })
    }

    pub(super) const fn pipeline(&self) -> &GraphicsPipeline {
        &self.pipeline
    }
}

fn tensor_programs() -> [PipelineProgram; 6] {
    [
        CLIENT_IMAGE_PROGRAM,
        MANAGED_CLIENT_IMAGE_PROGRAM,
        CURSOR_PROGRAM,
        FOCUS_RING_PROGRAM,
        SHADOW_PROGRAM,
        BACKDROP_FILTER_PROGRAM,
    ]
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
        blend: program.blend,
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
        for program in tensor_programs() {
            let vertex = shader_descriptor(program.vertex_label, program.vertex);
            let fragment = shader_descriptor(program.fragment_label, program.fragment);
            assert!(vertex.validate().is_ok());
            assert!(fragment.validate().is_ok());
            assert_eq!(vertex.spirv.as_slice(), program.vertex);
            assert_eq!(fragment.spirv.as_slice(), program.fragment);
        }
    }

    #[test]
    fn backdrop_filter_overwrites_its_ping_pong_lane() {
        assert_eq!(BACKDROP_FILTER_PROGRAM.blend, None);
    }
}
