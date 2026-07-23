#![allow(unsafe_code)]

use std::slice;

use thiserror::Error;
use vulkanalia::vk::{DeviceV1_0, Handle, HasBuilder};
use vulkanalia::{Device, vk};

const VERTEX_SHADER: &[u32] =
    vulkanalia::include_shader_code!(concat!(env!("OUT_DIR"), "/tensor_client.vert.spv"));
const FRAGMENT_SHADER: &[u32] =
    vulkanalia::include_shader_code!(concat!(env!("OUT_DIR"), "/tensor_client.frag.spv"));

/// The first real scene pipeline. It deliberately has no descriptor-set
/// layout: `VK_EXT_descriptor_heap` maps the sampled-image declaration at
/// pipeline creation and a per-draw push index selects the frame descriptor.
/// `resource_heap_base` is the byte offset immediately after the implementation
/// reserved range; push indices are relative to that base so descriptor sizes
/// need not divide the reserved range itself.
pub(super) struct ClientImagePipeline {
    pipeline: vk::Pipeline,
}

impl ClientImagePipeline {
    pub(super) fn new(
        device: &Device,
        target_format: vk::Format,
        descriptor_stride: u64,
        resource_heap_base: u64,
    ) -> Result<Self, ClientPipelineError> {
        let descriptor_stride = u32::try_from(descriptor_stride)
            .map_err(|_| ClientPipelineError::DescriptorStrideTooLarge(descriptor_stride))?;
        let resource_heap_base = u32::try_from(resource_heap_base)
            .map_err(|_| ClientPipelineError::HeapOffsetTooLarge(resource_heap_base))?;

        let vertex_info = vk::ShaderModuleCreateInfo::builder()
            .code(VERTEX_SHADER)
            .build();
        let vertex_module = unsafe { device.create_shader_module(&vertex_info, None) }
            .map_err(ClientPipelineError::CreateVertexModule)?;
        let fragment_info = vk::ShaderModuleCreateInfo::builder()
            .code(FRAGMENT_SHADER)
            .build();
        let fragment_module = match unsafe { device.create_shader_module(&fragment_info, None) } {
            Ok(module) => module,
            Err(error) => {
                unsafe { device.destroy_shader_module(vertex_module, None) };
                return Err(ClientPipelineError::CreateFragmentModule(error));
            }
        };

        let sampler = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(1.0)
            .build();
        let source = vk::DescriptorMappingSourcePushIndexEXT::builder()
            .heap_offset(resource_heap_base)
            .push_offset(0)
            .heap_index_stride(descriptor_stride)
            .heap_array_stride(descriptor_stride)
            .embedded_sampler(&sampler)
            .build();
        let mapping = vk::DescriptorSetAndBindingMappingEXT::builder()
            .descriptor_set(0)
            .first_binding(0)
            .binding_count(1)
            .resource_mask(vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE)
            .source(vk::DescriptorMappingSourceEXT::HEAP_WITH_PUSH_INDEX)
            .source_data(vk::DescriptorMappingSourceDataEXT { push_index: source })
            .build();
        let mut mapping_info = vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
            .mappings(slice::from_ref(&mapping))
            .build();

        let entry = b"main\0";
        let vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(entry)
            .build();
        let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(entry)
            .build();
        fragment_stage.next =
            (&mut mapping_info as *mut vk::ShaderDescriptorSetAndBindingMappingInfoEXT).cast();
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
            .src_color_blend_factor(vk::BlendFactor::ONE)
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
        let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
            .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
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
            .layout(vk::PipelineLayout::null())
            .render_pass(vk::RenderPass::null())
            .subpass(0)
            .push_next(&mut rendering)
            .push_next(&mut flags)
            .build();
        let result = unsafe {
            device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                slice::from_ref(&pipeline_info),
                None,
            )
        };
        unsafe {
            device.destroy_shader_module(vertex_module, None);
            device.destroy_shader_module(fragment_module, None);
        }
        let (pipelines, _) = result.map_err(ClientPipelineError::CreatePipeline)?;
        let pipeline = pipelines
            .first()
            .copied()
            .ok_or(ClientPipelineError::NoPipeline)?;
        Ok(Self { pipeline })
    }

    pub(super) const fn handle(&self) -> vk::Pipeline {
        self.pipeline
    }

    pub(super) unsafe fn destroy(self, device: &Device) {
        unsafe { device.destroy_pipeline(self.pipeline, None) };
    }
}

#[derive(Debug, Error)]
pub(super) enum ClientPipelineError {
    #[error("descriptor stride {0} does not fit descriptor-heap mapping fields")]
    DescriptorStrideTooLarge(u64),
    #[error("resource heap base offset {0} does not fit descriptor-heap mapping fields")]
    HeapOffsetTooLarge(u64),
    #[error("failed to create the client vertex shader module: {0:?}")]
    CreateVertexModule(vk::ErrorCode),
    #[error("failed to create the client fragment shader module: {0:?}")]
    CreateFragmentModule(vk::ErrorCode),
    #[error("failed to create the client graphics pipeline: {0:?}")]
    CreatePipeline(vk::ErrorCode),
    #[error("Vulkan returned no client graphics pipeline")]
    NoPipeline,
}
