use std::collections::BTreeSet;
use std::ffi::CStr;
use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{PipelineCache, ShaderBindingMap};
use crate::backend::DeviceOwner;
use crate::{Backend, Error, Features, Result, ShaderModule};

#[derive(Clone, Copy, Debug)]
pub struct ProgrammableStage<'a> {
    pub module: &'a ShaderModule,
    pub entry_point: &'a CStr,
    pub bindings: &'a ShaderBindingMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VertexAttribute {
    pub format: vk::Format,
    pub offset: u64,
    pub shader_location: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VertexStepMode {
    #[default]
    Vertex,
    Instance,
}

#[derive(Clone, Copy, Debug)]
pub struct VertexBufferLayout<'a> {
    pub array_stride: u64,
    pub step_mode: VertexStepMode,
    pub attributes: &'a [VertexAttribute],
}

#[derive(Clone, Copy, Debug)]
pub struct VertexState<'a> {
    pub stage: ProgrammableStage<'a>,
    pub buffers: &'a [VertexBufferLayout<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct FragmentState<'a> {
    pub stage: ProgrammableStage<'a>,
    pub targets: &'a [Option<ColorTargetState>],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlendComponent {
    pub src_factor: vk::BlendFactor,
    pub dst_factor: vk::BlendFactor,
    pub operation: vk::BlendOp,
}

impl BlendComponent {
    pub const REPLACE: Self = Self {
        src_factor: vk::BlendFactor::ONE,
        dst_factor: vk::BlendFactor::ZERO,
        operation: vk::BlendOp::ADD,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

impl BlendState {
    pub const ALPHA_BLENDING: Self = Self {
        color: BlendComponent {
            src_factor: vk::BlendFactor::SRC_ALPHA,
            dst_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            operation: vk::BlendOp::ADD,
        },
        alpha: BlendComponent {
            src_factor: vk::BlendFactor::ONE,
            dst_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            operation: vk::BlendOp::ADD,
        },
    };

    pub const PREMULTIPLIED_ALPHA_BLENDING: Self = Self {
        color: BlendComponent {
            src_factor: vk::BlendFactor::ONE,
            dst_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            operation: vk::BlendOp::ADD,
        },
        alpha: BlendComponent {
            src_factor: vk::BlendFactor::ONE,
            dst_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            operation: vk::BlendOp::ADD,
        },
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorTargetState {
    pub format: vk::Format,
    pub blend: Option<BlendState>,
    pub write_mask: vk::ColorComponentFlags,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimitiveState {
    pub topology: vk::PrimitiveTopology,
    pub primitive_restart_enable: bool,
    pub polygon_mode: vk::PolygonMode,
    pub cull_mode: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
}

impl Default for PrimitiveState {
    fn default() -> Self {
        Self {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            primitive_restart_enable: false,
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::NONE,
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MultisampleState {
    pub count: vk::SampleCountFlags,
    pub mask: u64,
    pub alpha_to_coverage_enabled: bool,
}

impl Default for MultisampleState {
    fn default() -> Self {
        Self {
            count: vk::SampleCountFlags::_1,
            mask: u64::MAX,
            alpha_to_coverage_enabled: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepthState {
    pub write_enabled: bool,
    pub compare: vk::CompareOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StencilState {
    pub front: vk::StencilOpState,
    pub back: vk::StencilOpState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DepthBiasState {
    pub constant_factor: f32,
    pub clamp: f32,
    pub slope_factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthStencilState {
    pub format: vk::Format,
    pub depth: Option<DepthState>,
    pub stencil: Option<StencilState>,
    pub bias: DepthBiasState,
}

#[derive(Clone, Copy, Debug)]
pub struct GraphicsPipelineDescriptor<'a> {
    pub label: Option<&'a str>,
    pub vertex: VertexState<'a>,
    pub primitive: PrimitiveState,
    pub depth_stencil: Option<DepthStencilState>,
    pub multisample: MultisampleState,
    pub fragment: FragmentState<'a>,
    pub cache: Option<&'a PipelineCache>,
}

#[derive(Clone)]
pub struct GraphicsPipeline {
    inner: Arc<GraphicsPipelineInner>,
}

struct GraphicsPipelineInner {
    owner: Arc<DeviceOwner>,
    raw: vk::Pipeline,
    label: Option<String>,
    color_formats: Vec<vk::Format>,
    depth_format: vk::Format,
    stencil_format: vk::Format,
    sample_count: vk::SampleCountFlags,
    vertex_buffer_count: u32,
}

impl GraphicsPipeline {
    pub fn raw(&self) -> vk::Pipeline {
        self.inner.raw
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub fn color_formats(&self) -> &[vk::Format] {
        &self.inner.color_formats
    }

    pub fn depth_format(&self) -> vk::Format {
        self.inner.depth_format
    }

    pub fn stencil_format(&self) -> vk::Format {
        self.inner.stencil_format
    }

    pub fn sample_count(&self) -> vk::SampleCountFlags {
        self.inner.sample_count
    }

    pub fn vertex_buffer_count(&self) -> u32 {
        self.inner.vertex_buffer_count
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }
}

impl fmt::Debug for GraphicsPipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GraphicsPipeline")
            .field("raw", &self.inner.raw)
            .field("label", &self.inner.label)
            .field("color_formats", &self.inner.color_formats)
            .field("depth_format", &self.inner.depth_format)
            .field("stencil_format", &self.inner.stencil_format)
            .field("sample_count", &self.inner.sample_count)
            .field("vertex_buffer_count", &self.inner.vertex_buffer_count)
            .finish_non_exhaustive()
    }
}

impl crate::SubmissionResource for GraphicsPipeline {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

impl Drop for GraphicsPipelineInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_pipeline(self.raw, None) };
    }
}

impl Backend {
    /// Creates a dynamic-rendering graphics pipeline whose only shader binding
    /// model is `VK_EXT_descriptor_heap`.
    pub fn create_graphics_pipeline(
        &self,
        descriptor: &GraphicsPipelineDescriptor<'_>,
    ) -> Result<GraphicsPipeline> {
        if !self.features().contains(Features::DESCRIPTOR_HEAP) {
            return Err(Error::Validation(
                "graphics pipelines require enabled Features::DESCRIPTOR_HEAP".into(),
            ));
        }
        validate_descriptor(descriptor, &self.shared_owner())?;
        create_graphics_pipeline(self.shared_owner(), descriptor)
    }
}

fn validate_descriptor(
    descriptor: &GraphicsPipelineDescriptor<'_>,
    owner: &Arc<DeviceOwner>,
) -> Result<()> {
    if !descriptor.vertex.stage.module.belongs_to(owner) {
        return Err(Error::Validation(
            "vertex shader module was created by a different Device".into(),
        ));
    }
    if !descriptor.fragment.stage.module.belongs_to(owner) {
        return Err(Error::Validation(
            "fragment shader module was created by a different Device".into(),
        ));
    }
    if let Some(cache) = descriptor.cache
        && !cache.belongs_to(owner)
    {
        return Err(Error::Validation(
            "pipeline cache was created by a different Device".into(),
        ));
    }
    validate_fixed_state(descriptor)
}

fn validate_fixed_state(descriptor: &GraphicsPipelineDescriptor<'_>) -> Result<()> {
    if descriptor.multisample.count.is_empty()
        || descriptor.multisample.count.bits().count_ones() != 1
    {
        return Err(Error::Validation(
            "multisample count must contain exactly one sample-count bit".into(),
        ));
    }
    if descriptor.fragment.targets.iter().all(Option::is_none) && descriptor.depth_stencil.is_none()
    {
        return Err(Error::Validation(
            "graphics pipeline must declare an active color or depth/stencil target".into(),
        ));
    }
    if descriptor
        .fragment
        .targets
        .iter()
        .flatten()
        .any(|target| target.format == vk::Format::UNDEFINED)
    {
        return Err(Error::Validation(
            "active color target format must not be UNDEFINED".into(),
        ));
    }
    if descriptor
        .fragment
        .targets
        .iter()
        .flatten()
        .any(|target| format_has_depth(target.format) || format_has_stencil(target.format))
    {
        return Err(Error::Validation(
            "color target format must not contain depth or stencil aspects".into(),
        ));
    }
    if let Some(depth_stencil) = descriptor.depth_stencil {
        if depth_stencil.format == vk::Format::UNDEFINED {
            return Err(Error::Validation(
                "depth/stencil format must not be UNDEFINED".into(),
            ));
        }
        if depth_stencil.depth.is_none() && depth_stencil.stencil.is_none() {
            return Err(Error::Validation(
                "depth/stencil state must enable at least one aspect".into(),
            ));
        }
        if depth_stencil.depth.is_some() && !format_has_depth(depth_stencil.format) {
            return Err(Error::Validation(
                "depth state requires a format with a depth aspect".into(),
            ));
        }
        if depth_stencil.stencil.is_some() && !format_has_stencil(depth_stencil.format) {
            return Err(Error::Validation(
                "stencil state requires a format with a stencil aspect".into(),
            ));
        }
    }

    let mut locations = BTreeSet::new();
    for buffer in descriptor.vertex.buffers {
        if buffer.array_stride > u32::MAX as u64 {
            return Err(Error::Validation(
                "vertex buffer array_stride exceeds Vulkan's u32 range".into(),
            ));
        }
        for attribute in buffer.attributes {
            if attribute.offset > u32::MAX as u64 {
                return Err(Error::Validation(
                    "vertex attribute offset exceeds Vulkan's u32 range".into(),
                ));
            }
            if !locations.insert(attribute.shader_location) {
                return Err(Error::Validation(format!(
                    "duplicate vertex shader location {}",
                    attribute.shader_location
                )));
            }
        }
    }
    Ok(())
}

fn create_graphics_pipeline(
    owner: Arc<DeviceOwner>,
    descriptor: &GraphicsPipelineDescriptor<'_>,
) -> Result<GraphicsPipeline> {
    let vertex_bindings = descriptor
        .vertex
        .buffers
        .iter()
        .enumerate()
        .map(|(binding, buffer)| {
            vk::VertexInputBindingDescription::builder()
                .binding(binding as u32)
                .stride(buffer.array_stride as u32)
                .input_rate(match buffer.step_mode {
                    VertexStepMode::Vertex => vk::VertexInputRate::VERTEX,
                    VertexStepMode::Instance => vk::VertexInputRate::INSTANCE,
                })
                .build()
        })
        .collect::<Vec<_>>();
    let vertex_attributes = descriptor
        .vertex
        .buffers
        .iter()
        .enumerate()
        .flat_map(|(binding, buffer)| {
            buffer.attributes.iter().map(move |attribute| {
                vk::VertexInputAttributeDescription::builder()
                    .location(attribute.shader_location)
                    .binding(binding as u32)
                    .format(attribute.format)
                    .offset(attribute.offset as u32)
                    .build()
            })
        })
        .collect::<Vec<_>>();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&vertex_bindings)
        .vertex_attribute_descriptions(&vertex_attributes)
        .build();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(descriptor.primitive.topology)
        .primitive_restart_enable(descriptor.primitive.primitive_restart_enable)
        .build();
    let viewport = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1)
        .build();
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .depth_bias_enable(
            descriptor
                .depth_stencil
                .is_some_and(|state| state.bias != DepthBiasState::default()),
        )
        .depth_bias_constant_factor(
            descriptor
                .depth_stencil
                .map_or(0.0, |state| state.bias.constant_factor),
        )
        .depth_bias_clamp(
            descriptor
                .depth_stencil
                .map_or(0.0, |state| state.bias.clamp),
        )
        .depth_bias_slope_factor(
            descriptor
                .depth_stencil
                .map_or(0.0, |state| state.bias.slope_factor),
        )
        .polygon_mode(descriptor.primitive.polygon_mode)
        .cull_mode(descriptor.primitive.cull_mode)
        .front_face(descriptor.primitive.front_face)
        .line_width(1.0)
        .build();
    let sample_masks = [
        descriptor.multisample.mask as u32,
        (descriptor.multisample.mask >> 32) as u32,
    ];
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(descriptor.multisample.count)
        .sample_mask(&sample_masks)
        .alpha_to_coverage_enable(descriptor.multisample.alpha_to_coverage_enabled)
        .build();
    let depth_stencil = descriptor.depth_stencil.map(|state| {
        let depth = state.depth.unwrap_or(DepthState {
            write_enabled: false,
            compare: vk::CompareOp::ALWAYS,
        });
        let stencil = state.stencil.unwrap_or(StencilState {
            front: vk::StencilOpState::default(),
            back: vk::StencilOpState::default(),
        });
        vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(state.depth.is_some())
            .depth_write_enable(depth.write_enabled)
            .depth_compare_op(depth.compare)
            .stencil_test_enable(state.stencil.is_some())
            .front(stencil.front)
            .back(stencil.back)
            .build()
    });
    let (color_formats, color_blend_attachments) = descriptor
        .fragment
        .targets
        .iter()
        .map(|target| match target {
            Some(target) => (target.format, color_blend_attachment(*target)),
            None => (
                vk::Format::UNDEFINED,
                vk::PipelineColorBlendAttachmentState::builder()
                    .color_write_mask(vk::ColorComponentFlags::empty())
                    .build(),
            ),
        })
        .unzip::<_, _, Vec<_>, Vec<_>>();
    let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
        .attachments(&color_blend_attachments)
        .build();
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&dynamic_states)
        .build();
    let depth_format = descriptor
        .depth_stencil
        .filter(|state| state.depth.is_some())
        .map_or(vk::Format::UNDEFINED, |state| state.format);
    let stencil_format = descriptor
        .depth_stencil
        .filter(|state| state.stencil.is_some())
        .map_or(vk::Format::UNDEFINED, |state| state.format);
    let mut rendering = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(&color_formats)
        .depth_attachment_format(depth_format)
        .stencil_attachment_format(stencil_format)
        .build();
    let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();

    let raw = descriptor
        .vertex
        .stage
        .bindings
        .with_stage_create_info(
            vk::ShaderStageFlags::VERTEX,
            descriptor.vertex.stage.module,
            descriptor.vertex.stage.entry_point,
            |vertex_stage| {
                descriptor.fragment.stage.bindings.with_stage_create_info(
                    vk::ShaderStageFlags::FRAGMENT,
                    descriptor.fragment.stage.module,
                    descriptor.fragment.stage.entry_point,
                    |fragment_stage| {
                        let stages = [*vertex_stage, *fragment_stage];
                        let mut info = vk::GraphicsPipelineCreateInfo::builder()
                            .stages(&stages)
                            .vertex_input_state(&vertex_input)
                            .input_assembly_state(&input_assembly)
                            .viewport_state(&viewport)
                            .rasterization_state(&rasterization)
                            .multisample_state(&multisample)
                            .color_blend_state(&color_blend)
                            .dynamic_state(&dynamic)
                            .layout(vk::PipelineLayout::null())
                            .render_pass(vk::RenderPass::null())
                            .subpass(0)
                            .push_next(&mut rendering)
                            .push_next(&mut flags);
                        if let Some(depth_stencil) = depth_stencil.as_ref() {
                            info = info.depth_stencil_state(depth_stencil);
                        }
                        create_pipeline_with_cache(&owner, descriptor.cache, info.build())
                    },
                )
            },
        )
        .map_err(|error| Error::Validation(error.to_string()))?
        .map_err(|error| Error::Validation(error.to_string()))??;
    Ok(GraphicsPipeline {
        inner: Arc::new(GraphicsPipelineInner {
            owner,
            raw,
            label: descriptor.label.map(str::to_owned),
            color_formats,
            depth_format,
            stencil_format,
            sample_count: descriptor.multisample.count,
            vertex_buffer_count: descriptor.vertex.buffers.len() as u32,
        }),
    })
}

fn create_pipeline_with_cache(
    owner: &Arc<DeviceOwner>,
    cache: Option<&PipelineCache>,
    info: vk::GraphicsPipelineCreateInfo,
) -> Result<vk::Pipeline> {
    let create = |cache| unsafe { owner.device.create_graphics_pipelines(cache, &[info], None) };
    let (mut pipelines, _) = match cache {
        Some(cache) => cache.with_raw(create),
        None => create(vk::PipelineCache::null()),
    }
    .map_err(|source| Error::vulkan("vkCreateGraphicsPipelines", source))?;
    if pipelines.len() != 1 {
        for pipeline in pipelines {
            unsafe { owner.device.destroy_pipeline(pipeline, None) };
        }
        return Err(Error::Validation(
            "vkCreateGraphicsPipelines returned an unexpected pipeline count".into(),
        ));
    }
    Ok(pipelines.remove(0))
}

fn color_blend_attachment(target: ColorTargetState) -> vk::PipelineColorBlendAttachmentState {
    let blend = target.blend.unwrap_or(BlendState {
        color: BlendComponent::REPLACE,
        alpha: BlendComponent::REPLACE,
    });
    vk::PipelineColorBlendAttachmentState::builder()
        .blend_enable(target.blend.is_some())
        .src_color_blend_factor(blend.color.src_factor)
        .dst_color_blend_factor(blend.color.dst_factor)
        .color_blend_op(blend.color.operation)
        .src_alpha_blend_factor(blend.alpha.src_factor)
        .dst_alpha_blend_factor(blend.alpha.dst_factor)
        .alpha_blend_op(blend.alpha.operation)
        .color_write_mask(target.write_mask)
        .build()
}

pub(crate) fn format_has_depth(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::D16_UNORM
            | vk::Format::X8_D24_UNORM_PACK32
            | vk::Format::D32_SFLOAT
            | vk::Format::D16_UNORM_S8_UINT
            | vk::Format::D24_UNORM_S8_UINT
            | vk::Format::D32_SFLOAT_S8_UINT
    )
}

pub(crate) fn format_has_stencil(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::S8_UINT
            | vk::Format::D16_UNORM_S8_UINT
            | vk::Format::D24_UNORM_S8_UINT
            | vk::Format::D32_SFLOAT_S8_UINT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_presets_match_straight_and_premultiplied_alpha_contracts() {
        assert_eq!(
            BlendState::ALPHA_BLENDING.color.src_factor,
            vk::BlendFactor::SRC_ALPHA
        );
        assert_eq!(
            BlendState::PREMULTIPLIED_ALPHA_BLENDING.color.src_factor,
            vk::BlendFactor::ONE
        );
    }

    #[test]
    fn default_pipeline_state_is_dynamic_triangle_list() {
        assert_eq!(
            PrimitiveState::default().topology,
            vk::PrimitiveTopology::TRIANGLE_LIST
        );
        assert_eq!(MultisampleState::default().count, vk::SampleCountFlags::_1);
    }
}
