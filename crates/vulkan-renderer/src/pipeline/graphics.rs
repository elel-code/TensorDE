use std::collections::BTreeSet;
use std::ffi::CStr;
use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{PipelineCache, ShaderBindingMap};
use crate::backend::DeviceOwner;
use crate::{Backend, Error, Features, Result, SampleCount, ShaderModule};

mod advanced_blend;
mod machine_code;
mod state;
mod vertex;

pub use advanced_blend::{AdvancedBlendState, BlendOverlap};
pub use machine_code::MachineCodeGraphicsPipelineDescriptor;
pub use state::{
    BlendFactor, BlendOperation, ColorWrites, CullMode, FrontFace, PolygonMode, PrimitiveTopology,
    VertexFormat,
};

#[derive(Clone, Copy, Debug)]
pub struct ProgrammableStage<'a> {
    pub module: &'a ShaderModule,
    pub entry_point: &'a CStr,
    pub bindings: &'a ShaderBindingMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VertexAttribute {
    pub format: VertexFormat,
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
    pub slot: u32,
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
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
    pub operation: BlendOperation,
}

impl BlendComponent {
    pub const REPLACE: Self = Self {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::Zero,
        operation: BlendOperation::Add,
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
            src_factor: BlendFactor::SourceAlpha,
            dst_factor: BlendFactor::OneMinusSourceAlpha,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSourceAlpha,
            operation: BlendOperation::Add,
        },
    };

    pub const PREMULTIPLIED_ALPHA_BLENDING: Self = Self {
        color: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSourceAlpha,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSourceAlpha,
            operation: BlendOperation::Add,
        },
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorTargetState {
    pub format: crate::TextureFormat,
    pub blend: Option<BlendState>,
    pub write_mask: ColorWrites,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimitiveState {
    pub topology: PrimitiveTopology,
    pub primitive_restart_enable: bool,
    pub polygon_mode: PolygonMode,
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
}

impl Default for PrimitiveState {
    fn default() -> Self {
        Self {
            topology: PrimitiveTopology::TriangleList,
            primitive_restart_enable: false,
            polygon_mode: PolygonMode::Fill,
            cull_mode: CullMode::None,
            front_face: FrontFace::CounterClockwise,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MultisampleState {
    pub count: SampleCount,
    pub mask: u64,
    pub alpha_to_coverage_enabled: bool,
}

impl Default for MultisampleState {
    fn default() -> Self {
        Self {
            count: SampleCount::One,
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
    pub advanced_blend: Option<AdvancedBlendState>,
    pub local_read_mapping: Option<&'a crate::RenderingLocalReadMapping>,
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
    color_formats: Vec<Option<crate::TextureFormat>>,
    depth_format: vk::Format,
    stencil_format: vk::Format,
    sample_count: SampleCount,
    vertex_buffer_slots: Vec<u32>,
}

impl GraphicsPipeline {
    pub fn raw(&self) -> vk::Pipeline {
        self.inner.raw
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub fn color_formats(&self) -> &[Option<crate::TextureFormat>] {
        &self.inner.color_formats
    }

    pub fn depth_format(&self) -> vk::Format {
        self.inner.depth_format
    }

    pub fn stencil_format(&self) -> vk::Format {
        self.inner.stencil_format
    }

    pub fn sample_count(&self) -> SampleCount {
        self.inner.sample_count
    }

    pub fn vertex_buffer_slots(&self) -> &[u32] {
        &self.inner.vertex_buffer_slots
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
            .field("vertex_buffer_slots", &self.inner.vertex_buffer_slots)
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
        validate_graphics_pipeline_descriptor(self, descriptor)?;
        create_graphics_pipeline(self.shared_owner(), descriptor)
    }
}

pub(super) fn validate_graphics_pipeline_descriptor(
    backend: &Backend,
    descriptor: &GraphicsPipelineDescriptor<'_>,
) -> Result<()> {
    if !backend.features().contains(Features::DESCRIPTOR_HEAP) {
        return Err(Error::Validation(
            "graphics pipelines require enabled Features::DESCRIPTOR_HEAP".into(),
        ));
    }
    if descriptor.advanced_blend.is_some() && !backend.features().contains(Features::ADVANCED_BLEND)
    {
        return Err(Error::Validation(
            "advanced blend state requires enabled Features::ADVANCED_BLEND".into(),
        ));
    }
    if !backend
        .device_info()
        .properties
        .framebuffer_color_sample_counts
        .contains(descriptor.multisample.count.as_supported_set())
    {
        return Err(Error::Validation(format!(
            "graphics pipeline sample count {:?} is unsupported by the selected Device",
            descriptor.multisample.count
        )));
    }
    let heap_limits = backend.device_info().limits.descriptor_heap;
    descriptor
        .vertex
        .stage
        .bindings
        .validate_for_device(heap_limits)
        .map_err(|error| Error::Validation(format!("vertex shader binding map: {error}")))?;
    descriptor
        .fragment
        .stage
        .bindings
        .validate_for_device(heap_limits)
        .map_err(|error| Error::Validation(format!("fragment shader binding map: {error}")))?;
    validate_descriptor(descriptor, &backend.shared_owner())
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
    if let Some(mapping) = descriptor.local_read_mapping {
        mapping.validate_for_device(
            owner.enabled_features,
            owner.limits.max_color_attachments,
            owner.limits.max_per_stage_descriptor_input_attachments,
        )?;
        if mapping.color_attachment_count() != descriptor.fragment.targets.len() {
            return Err(Error::Validation(
                "graphics pipeline local-read mapping count must match color target count".into(),
            ));
        }
    }
    validate_fixed_state(descriptor)
}

fn validate_fixed_state(descriptor: &GraphicsPipelineDescriptor<'_>) -> Result<()> {
    advanced_blend::validate_advanced_blend(descriptor)?;
    if descriptor.fragment.targets.iter().all(Option::is_none) && descriptor.depth_stencil.is_none()
    {
        return Err(Error::Validation(
            "graphics pipeline must declare an active color or depth/stencil target".into(),
        ));
    }
    if descriptor.fragment.targets.iter().flatten().any(|target| {
        format_has_depth(target.format.to_vk()) || format_has_stencil(target.format.to_vk())
    }) {
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
    let mut buffer_slots = BTreeSet::new();
    for buffer in descriptor.vertex.buffers {
        if !buffer_slots.insert(buffer.slot) {
            return Err(Error::Validation(format!(
                "duplicate vertex-buffer slot {}",
                buffer.slot
            )));
        }
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
    let facts = graphics_pipeline_facts(descriptor);
    let raw = with_graphics_pipeline_create_info(
        descriptor,
        vk::PipelineCreateFlags2::empty(),
        None,
        |info| create_pipeline_with_cache(&owner, descriptor.cache, info),
    )?;
    Ok(GraphicsPipeline {
        inner: Arc::new(GraphicsPipelineInner {
            owner,
            raw,
            label: descriptor.label.map(str::to_owned),
            color_formats: facts.color_formats,
            depth_format: facts.depth_format,
            stencil_format: facts.stencil_format,
            sample_count: facts.sample_count,
            vertex_buffer_slots: facts.vertex_buffer_slots,
        }),
    })
}

pub(super) fn graphics_pipeline_facts(
    descriptor: &GraphicsPipelineDescriptor<'_>,
) -> super::binary::MachineCodeGraphicsFacts {
    super::binary::MachineCodeGraphicsFacts {
        color_formats: descriptor
            .fragment
            .targets
            .iter()
            .map(|target| target.map(|target| target.format))
            .collect(),
        depth_format: descriptor
            .depth_stencil
            .filter(|state| state.depth.is_some())
            .map_or(vk::Format::UNDEFINED, |state| state.format),
        stencil_format: descriptor
            .depth_stencil
            .filter(|state| state.stencil.is_some())
            .map_or(vk::Format::UNDEFINED, |state| state.format),
        sample_count: descriptor.multisample.count,
        vertex_buffer_slots: descriptor
            .vertex
            .buffers
            .iter()
            .map(|buffer| buffer.slot)
            .collect(),
    }
}

pub(super) fn with_graphics_pipeline_create_info<T>(
    descriptor: &GraphicsPipelineDescriptor<'_>,
    additional_flags: vk::PipelineCreateFlags2,
    ready_binaries: Option<&[vk::PipelineBinaryKHR]>,
    use_info: impl FnOnce(vk::GraphicsPipelineCreateInfo) -> Result<T>,
) -> Result<T> {
    let (vertex_bindings, vertex_attributes) =
        vertex::vertex_input_descriptions(descriptor.vertex.buffers);
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&vertex_bindings)
        .vertex_attribute_descriptions(&vertex_attributes)
        .build();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(descriptor.primitive.topology.to_vk())
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
        .polygon_mode(descriptor.primitive.polygon_mode.to_vk())
        .cull_mode(descriptor.primitive.cull_mode.to_vk())
        .front_face(descriptor.primitive.front_face.to_vk())
        .line_width(1.0)
        .build();
    let sample_masks = [
        descriptor.multisample.mask as u32,
        (descriptor.multisample.mask >> 32) as u32,
    ];
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(descriptor.multisample.count.to_vk())
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
    let color_blend_attachments = descriptor
        .fragment
        .targets
        .iter()
        .map(|target| match target {
            Some(target) => color_blend_attachment(*target),
            None => vk::PipelineColorBlendAttachmentState::builder()
                .color_write_mask(vk::ColorComponentFlags::empty())
                .build(),
        })
        .collect::<Vec<_>>();
    let color_formats = descriptor
        .fragment
        .targets
        .iter()
        .map(|target| target.map_or(vk::Format::UNDEFINED, |target| target.format.to_vk()))
        .collect::<Vec<_>>();
    let mut advanced_blend = descriptor.advanced_blend.map(|state| {
        vk::PipelineColorBlendAdvancedStateCreateInfoEXT::builder()
            .src_premultiplied(state.source_premultiplied)
            .dst_premultiplied(state.destination_premultiplied)
            .blend_overlap(state.overlap.to_vk())
            .build()
    });
    let mut color_blend_builder =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(&color_blend_attachments);
    if let Some(advanced_blend) = advanced_blend.as_mut() {
        color_blend_builder = color_blend_builder.push_next(advanced_blend);
    }
    let color_blend = color_blend_builder.build();
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
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT | additional_flags)
        .build();
    let mut attachment_locations = descriptor
        .local_read_mapping
        .map(crate::RenderingLocalReadMapping::attachment_location_info);
    let mut input_attachment_indices = descriptor
        .local_read_mapping
        .map(crate::RenderingLocalReadMapping::input_attachment_index_info);
    let mut binary_info = ready_binaries.map(|binaries| {
        vk::PipelineBinaryInfoKHR::builder()
            .pipeline_binaries(binaries)
            .build()
    });

    descriptor
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
                        if let Some(locations) = attachment_locations.as_mut() {
                            info = info.push_next(locations);
                        }
                        if let Some(indices) = input_attachment_indices.as_mut() {
                            info = info.push_next(indices);
                        }
                        if let Some(binary_info) = binary_info.as_mut() {
                            info = info.push_next(binary_info);
                        }
                        use_info(info.build())
                    },
                )
            },
        )
        .map_err(|error| Error::Validation(error.to_string()))?
        .map_err(|error| Error::Validation(error.to_string()))?
}

fn create_pipeline_with_cache(
    owner: &Arc<DeviceOwner>,
    cache: Option<&PipelineCache>,
    info: vk::GraphicsPipelineCreateInfo,
) -> Result<vk::Pipeline> {
    match cache {
        Some(cache) => {
            cache.with_raw(|cache| create_pipeline_with_device(&owner.device, cache, info))
        }
        None => create_pipeline_with_device(&owner.device, vk::PipelineCache::null(), info),
    }
}

pub(super) fn create_pipeline_with_device(
    device: &vulkanalia::Device,
    cache: vk::PipelineCache,
    info: vk::GraphicsPipelineCreateInfo,
) -> Result<vk::Pipeline> {
    let (mut pipelines, status) = unsafe { device.create_graphics_pipelines(cache, &[info], None) }
        .map_err(|source| Error::vulkan("vkCreateGraphicsPipelines", source))?;
    if status != vk::SuccessCode::SUCCESS || pipelines.len() != 1 {
        for pipeline in pipelines {
            unsafe { device.destroy_pipeline(pipeline, None) };
        }
        return Err(Error::Validation(format!(
            "vkCreateGraphicsPipelines did not return exactly one ready pipeline: status={status:?}"
        )));
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
        .src_color_blend_factor(blend.color.src_factor.to_vk())
        .dst_color_blend_factor(blend.color.dst_factor.to_vk())
        .color_blend_op(blend.color.operation.to_vk())
        .src_alpha_blend_factor(blend.alpha.src_factor.to_vk())
        .dst_alpha_blend_factor(blend.alpha.dst_factor.to_vk())
        .alpha_blend_op(blend.alpha.operation.to_vk())
        .color_write_mask(target.write_mask.to_vk())
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
            BlendFactor::SourceAlpha
        );
        assert_eq!(
            BlendState::PREMULTIPLIED_ALPHA_BLENDING.color.src_factor,
            BlendFactor::One
        );
    }

    #[test]
    fn default_pipeline_state_is_dynamic_triangle_list() {
        assert_eq!(
            PrimitiveState::default().topology,
            PrimitiveTopology::TriangleList
        );
        assert_eq!(MultisampleState::default().count, SampleCount::One);
    }
}
