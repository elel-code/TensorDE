#![allow(unsafe_code)]

use std::{mem, slice};

use thiserror::Error;
use vulkanalia::vk::{DeviceV1_0, Handle, HasBuilder};
use vulkanalia::{Device, vk};

const VERTEX_SHADER: &[u32] =
    vulkanalia::include_shader_code!(concat!(env!("OUT_DIR"), "/tensor_solid.vert.spv"));
const FRAGMENT_SHADER: &[u32] =
    vulkanalia::include_shader_code!(concat!(env!("OUT_DIR"), "/tensor_solid.frag.spv"));

/// Descriptor-free pipeline for compositor-owned solid geometry such as the
/// active-view focus ring. It does not form a descriptor-set fallback: client
/// and output image sampling still exclusively uses `VK_EXT_descriptor_heap`.
pub(crate) struct SolidPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl SolidPipeline {
    pub(crate) fn new(
        device: &Device,
        target_format: vk::Format,
    ) -> Result<Self, SolidPipelineError> {
        let (vertex_module, fragment_module) = create_shader_modules(device)?;
        let ranges = [vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(32)
            .build()];
        let layout_info = vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&ranges);
        let layout = match unsafe { device.create_pipeline_layout(&layout_info, None) } {
            Ok(layout) => layout,
            Err(error) => {
                unsafe {
                    device.destroy_shader_module(fragment_module, None);
                    device.destroy_shader_module(vertex_module, None);
                }
                return Err(SolidPipelineError::CreateLayout(error));
            }
        };

        let entry = b"main\0";
        let vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(entry)
            .build();
        let fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(entry)
            .build();
        let stages = [vertex_stage, fragment_stage];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder().build();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .build();
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(1)
            .scissor_count(1)
            .build();
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
            .dynamic_states(&dynamic_states)
            .build();
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0)
            .build();
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(vk::SampleCountFlags::_1)
            .build();
        let color_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .build();
        let color_attachments = [color_attachment];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .attachments(&color_attachments)
            .build();
        let formats = [target_format];
        let mut rendering = vk::PipelineRenderingCreateInfo::builder()
            .color_attachment_formats(&formats)
            .build();
        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .render_pass(vk::RenderPass::null())
            .subpass(0)
            .push_next(&mut rendering)
            .build();
        let result = unsafe {
            device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                slice::from_ref(&pipeline_info),
                None,
            )
        };
        unsafe {
            device.destroy_shader_module(fragment_module, None);
            device.destroy_shader_module(vertex_module, None);
        }
        let (pipelines, _) = match result {
            Ok(result) => result,
            Err(error) => {
                unsafe { device.destroy_pipeline_layout(layout, None) };
                return Err(SolidPipelineError::CreatePipeline(error));
            }
        };
        let Some(pipeline) = pipelines.first().copied() else {
            unsafe { device.destroy_pipeline_layout(layout, None) };
            return Err(SolidPipelineError::NoPipeline);
        };
        Ok(Self { pipeline, layout })
    }

    pub(crate) const fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub(crate) const fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    pub(crate) unsafe fn destroy(self, device: &Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn create_shader_modules(
    device: &Device,
) -> Result<(vk::ShaderModule, vk::ShaderModule), SolidPipelineError> {
    let vertex_info = shader_module_info(VERTEX_SHADER);
    let vertex_module = unsafe { device.create_shader_module(&vertex_info, None) }
        .map_err(SolidPipelineError::CreateVertexModule)?;
    let fragment_info = shader_module_info(FRAGMENT_SHADER);
    match unsafe { device.create_shader_module(&fragment_info, None) } {
        Ok(fragment_module) => Ok((vertex_module, fragment_module)),
        Err(error) => {
            unsafe { device.destroy_shader_module(vertex_module, None) };
            Err(SolidPipelineError::CreateFragmentModule(error))
        }
    }
}

fn shader_module_info(code: &[u32]) -> vk::ShaderModuleCreateInfo {
    vk::ShaderModuleCreateInfo::builder()
        .code_size(mem::size_of_val(code))
        .code(code)
        .build()
}

#[derive(Debug, Error)]
pub(crate) enum SolidPipelineError {
    #[error("failed to create the solid vertex shader module: {0:?}")]
    CreateVertexModule(vk::ErrorCode),
    #[error("failed to create the solid fragment shader module: {0:?}")]
    CreateFragmentModule(vk::ErrorCode),
    #[error("failed to create the solid push-constant layout: {0:?}")]
    CreateLayout(vk::ErrorCode),
    #[error("failed to create the solid graphics pipeline: {0:?}")]
    CreatePipeline(vk::ErrorCode),
    #[error("Vulkan returned no solid graphics pipeline")]
    NoPipeline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_shader_modules_have_complete_word_lengths() {
        assert_eq!(
            shader_module_info(VERTEX_SHADER).code_size,
            mem::size_of_val(VERTEX_SHADER)
        );
        assert_eq!(
            shader_module_info(FRAGMENT_SHADER).code_size,
            mem::size_of_val(FRAGMENT_SHADER)
        );
    }
}
