use std::collections::BTreeSet;
use std::ops::Range;

use vulkanalia::{prelude::v1_4::*, vk};

use super::CommandEncoder;
use crate::{
    AcquiredSurfaceTexture, Buffer, DescriptorHeap, Error, ExportedDmaBufImage, GraphicsPipeline,
    ImageView, ImportedDmaBufImage, Result, RetainedExternalImageView,
};

mod validation;

use validation::validate_descriptor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

impl IndexFormat {
    const fn as_vk(self) -> vk::IndexType {
        match self {
            Self::Uint16 => vk::IndexType::UINT16,
            Self::Uint32 => vk::IndexType::UINT32,
        }
    }

    const fn alignment(self) -> u64 {
        match self {
            Self::Uint16 => 2,
            Self::Uint32 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadOp<T> {
    Load,
    Clear(T),
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOp {
    Store,
    Discard,
}

#[derive(Clone, Copy)]
pub struct AttachmentView<'a> {
    owner: &'a std::sync::Arc<crate::backend::DeviceOwner>,
    raw: vk::ImageView,
    format: vk::Format,
    sample_count: vk::SampleCountFlags,
}

impl std::fmt::Debug for AttachmentView<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachmentView")
            .field("raw", &self.raw)
            .field("format", &self.format)
            .field("sample_count", &self.sample_count)
            .finish_non_exhaustive()
    }
}

impl AttachmentView<'_> {
    pub const fn raw(self) -> vk::ImageView {
        self.raw
    }

    pub const fn format(self) -> vk::Format {
        self.format
    }

    pub const fn sample_count(self) -> vk::SampleCountFlags {
        self.sample_count
    }

    fn belongs_to(self, owner: &std::sync::Arc<crate::backend::DeviceOwner>) -> bool {
        std::sync::Arc::ptr_eq(self.owner, owner)
    }
}

impl ImageView {
    /// Borrows this owned view as a validated dynamic-rendering attachment.
    pub fn as_attachment(&self) -> AttachmentView<'_> {
        AttachmentView {
            owner: self.owner(),
            raw: self.raw(),
            format: self.format(),
            sample_count: self.sample_count(),
        }
    }
}

impl ImportedDmaBufImage {
    /// Borrows an imported dma-buf view as a dynamic-rendering attachment.
    /// The image usage and probed modifier capabilities must permit the chosen
    /// attachment role.
    pub fn as_attachment(&self) -> AttachmentView<'_> {
        AttachmentView {
            owner: self.owner(),
            raw: self.view(),
            format: self.format(),
            sample_count: vk::SampleCountFlags::_1,
        }
    }
}

impl ExportedDmaBufImage {
    /// Borrows an exportable dma-buf image as a dynamic-rendering attachment.
    pub fn as_attachment(&self) -> AttachmentView<'_> {
        AttachmentView {
            owner: self.owner(),
            raw: self.view(),
            format: self.format(),
            sample_count: vk::SampleCountFlags::_1,
        }
    }
}

impl RetainedExternalImageView {
    /// Borrows a decoder/host-owned image view as a rendering attachment.
    pub fn as_attachment(&self) -> AttachmentView<'_> {
        AttachmentView {
            owner: self.owner(),
            raw: self.raw_view(),
            format: self.format(),
            sample_count: self.sample_count(),
        }
    }
}

impl AcquiredSurfaceTexture<'_> {
    /// Borrows this swapchain view as a validated dynamic-rendering attachment.
    pub fn as_attachment(&self) -> AttachmentView<'_> {
        AttachmentView {
            owner: self.owner(),
            raw: self.view(),
            format: self.format(),
            sample_count: vk::SampleCountFlags::_1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColorAttachment<'a> {
    pub view: AttachmentView<'a>,
    pub layout: vk::ImageLayout,
    pub resolve_target: Option<AttachmentView<'a>>,
    pub resolve_layout: vk::ImageLayout,
    pub resolve_mode: vk::ResolveModeFlags,
    pub load_op: LoadOp<[f32; 4]>,
    pub store_op: StoreOp,
}

#[derive(Clone, Copy, Debug)]
pub struct DepthAttachment<'a> {
    pub view: AttachmentView<'a>,
    pub layout: vk::ImageLayout,
    pub resolve_target: Option<AttachmentView<'a>>,
    pub resolve_layout: vk::ImageLayout,
    pub resolve_mode: vk::ResolveModeFlags,
    pub load_op: LoadOp<f32>,
    pub store_op: StoreOp,
}

#[derive(Clone, Copy, Debug)]
pub struct StencilAttachment<'a> {
    pub view: AttachmentView<'a>,
    pub layout: vk::ImageLayout,
    pub resolve_target: Option<AttachmentView<'a>>,
    pub resolve_layout: vk::ImageLayout,
    pub resolve_mode: vk::ResolveModeFlags,
    pub load_op: LoadOp<u32>,
    pub store_op: StoreOp,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderingDescriptor<'a> {
    pub label: Option<&'a str>,
    pub render_area: vk::Rect2D,
    pub layer_count: u32,
    pub view_mask: u32,
    pub color_attachments: &'a [Option<ColorAttachment<'a>>],
    pub depth_attachment: Option<DepthAttachment<'a>>,
    pub stencil_attachment: Option<StencilAttachment<'a>>,
}

/// Borrowed dynamic-rendering scope.
///
/// Dropping or explicitly ending this value records `vkCmdEndRendering`.
/// The parent command encoder cannot be finished while the scope is live.
pub struct RenderingEncoder<'encoder> {
    encoder: &'encoder mut CommandEncoder,
    label: Option<String>,
    color_formats: Vec<vk::Format>,
    depth_format: vk::Format,
    stencil_format: vk::Format,
    sample_count: vk::SampleCountFlags,
    pipeline_bound: bool,
    viewport_set: bool,
    scissor_set: bool,
    required_vertex_buffers: u32,
    bound_vertex_buffers: BTreeSet<u32>,
    index_buffer_bound: bool,
    ended: bool,
}

impl std::fmt::Debug for RenderingEncoder<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RenderingEncoder")
            .field("label", &self.label)
            .field("color_formats", &self.color_formats)
            .field("depth_format", &self.depth_format)
            .field("stencil_format", &self.stencil_format)
            .field("sample_count", &self.sample_count)
            .field("pipeline_bound", &self.pipeline_bound)
            .field("viewport_set", &self.viewport_set)
            .field("scissor_set", &self.scissor_set)
            .field("required_vertex_buffers", &self.required_vertex_buffers)
            .finish_non_exhaustive()
    }
}

impl CommandEncoder {
    /// Begins a Vulkan 1.4 dynamic-rendering scope.
    ///
    /// # Safety
    ///
    /// The render graph must have transitioned every attachment to its declared
    /// layout. Views and their images must remain alive until submission finishes.
    pub unsafe fn begin_rendering<'encoder>(
        &'encoder mut self,
        descriptor: &RenderingDescriptor<'_>,
    ) -> Result<RenderingEncoder<'encoder>> {
        let metadata = validate_descriptor(self, descriptor)?;
        let color_attachments = descriptor
            .color_attachments
            .iter()
            .map(|attachment| match attachment {
                Some(attachment) => color_attachment_info(*attachment),
                None => vk::RenderingAttachmentInfo::default(),
            })
            .collect::<Vec<_>>();
        let depth_attachment = descriptor.depth_attachment.map(depth_attachment_info);
        let stencil_attachment = descriptor.stencil_attachment.map(stencil_attachment_info);
        let mut rendering = vk::RenderingInfo::builder()
            .render_area(descriptor.render_area)
            .layer_count(descriptor.layer_count)
            .view_mask(descriptor.view_mask)
            .color_attachments(&color_attachments);
        if let Some(depth_attachment) = depth_attachment.as_ref() {
            rendering = rendering.depth_attachment(depth_attachment);
        }
        if let Some(stencil_attachment) = stencil_attachment.as_ref() {
            rendering = rendering.stencil_attachment(stencil_attachment);
        }
        unsafe {
            self.owner
                .device
                .cmd_begin_rendering(self.raw(), &rendering);
        }
        Ok(RenderingEncoder {
            encoder: self,
            label: descriptor.label.map(str::to_owned),
            color_formats: metadata.color_formats,
            depth_format: metadata.depth_format,
            stencil_format: metadata.stencil_format,
            sample_count: metadata.sample_count,
            pipeline_bound: false,
            viewport_set: false,
            scissor_set: false,
            required_vertex_buffers: 0,
            bound_vertex_buffers: BTreeSet::new(),
            index_buffer_bound: false,
            ended: false,
        })
    }
}

impl RenderingEncoder<'_> {
    /// Retains an arbitrary renderer resource through the enclosing command
    /// buffer's eventual submission.
    ///
    /// This is primarily used for resources reached indirectly through
    /// descriptor heaps, because binding a heap cannot infer the image views
    /// or buffers encoded in its descriptor bytes.
    pub fn retain_resource<R>(&mut self, resource: &R)
    where
        R: crate::SubmissionResource + ?Sized,
    {
        self.encoder.retain_resource(resource);
    }

    /// Copies a small shader payload using the descriptor-heap push-data path.
    pub fn push_data(&mut self, offset: u32, data: &[u8]) -> Result<()> {
        self.encoder.push_data(offset, data)
    }

    pub fn set_viewport(&mut self, viewport: vk::Viewport) -> Result<()> {
        if ![
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            viewport.min_depth,
            viewport.max_depth,
        ]
        .into_iter()
        .all(f32::is_finite)
            || viewport.width == 0.0
            || viewport.height == 0.0
            || !(0.0..=1.0).contains(&viewport.min_depth)
            || !(0.0..=1.0).contains(&viewport.max_depth)
            || viewport.min_depth > viewport.max_depth
        {
            return Err(Error::Validation(
                "viewport must be finite, non-empty, and use ordered depth bounds in 0..=1".into(),
            ));
        }
        unsafe {
            self.encoder
                .owner
                .device
                .cmd_set_viewport(self.encoder.raw(), 0, &[viewport]);
        }
        self.viewport_set = true;
        Ok(())
    }

    pub fn set_scissor(&mut self, scissor: vk::Rect2D) -> Result<()> {
        if scissor.extent.width == 0
            || scissor.extent.height == 0
            || i64::from(scissor.offset.x) + i64::from(scissor.extent.width) < 0
            || i64::from(scissor.offset.y) + i64::from(scissor.extent.height) < 0
        {
            return Err(Error::Validation(
                "scissor must be non-empty and its offset plus extent must be non-negative".into(),
            ));
        }
        unsafe {
            self.encoder
                .owner
                .device
                .cmd_set_scissor(self.encoder.raw(), 0, &[scissor]);
        }
        self.scissor_set = true;
        Ok(())
    }

    /// Binds and retains a pipeline compatible with this rendering scope.
    pub fn bind_pipeline(&mut self, pipeline: &GraphicsPipeline) -> Result<()> {
        if !pipeline.belongs_to(&self.encoder.owner) {
            return Err(Error::Validation(
                "graphics pipeline was created by a different Device".into(),
            ));
        }
        if pipeline.color_formats() != self.color_formats
            || pipeline.depth_format() != self.depth_format
            || pipeline.stencil_format() != self.stencil_format
            || pipeline.sample_count() != self.sample_count
        {
            return Err(Error::Validation(
                "graphics pipeline attachment formats or sample count do not match the rendering scope"
                    .into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_bind_pipeline(
                self.encoder.raw(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.raw(),
            );
        }
        self.encoder.retain_resource(pipeline);
        self.pipeline_bound = true;
        self.required_vertex_buffers = pipeline.vertex_buffer_count();
        Ok(())
    }

    /// Binds one vertex buffer for the active or subsequently bound pipeline.
    ///
    /// # Safety
    ///
    /// Its contents must match the pipeline's declared vertex layout. The
    /// encoder retains the buffer until submission finishes.
    pub unsafe fn set_vertex_buffer(
        &mut self,
        slot: u32,
        buffer: &Buffer,
        offset: u64,
    ) -> Result<()> {
        if !buffer.belongs_to(&self.encoder.owner) {
            return Err(Error::Validation(
                "vertex buffer was created by a different Device".into(),
            ));
        }
        if !buffer.usage().contains(vk::BufferUsageFlags::VERTEX_BUFFER) {
            return Err(Error::Validation(
                "vertex buffer is missing VERTEX_BUFFER usage".into(),
            ));
        }
        if offset >= buffer.size() {
            return Err(Error::Validation(
                "vertex buffer offset is outside the buffer".into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_bind_vertex_buffers(
                self.encoder.raw(),
                slot,
                &[buffer.raw()],
                &[offset],
            );
        }
        self.encoder.retain_resource(buffer);
        self.bound_vertex_buffers.insert(slot);
        Ok(())
    }

    /// Binds an index buffer for a following indexed draw.
    ///
    /// # Safety
    ///
    /// The indexed range must contain valid indices for all bound vertex
    /// buffers. The encoder retains the buffer until submission finishes.
    pub unsafe fn set_index_buffer(
        &mut self,
        buffer: &Buffer,
        offset: u64,
        format: IndexFormat,
    ) -> Result<()> {
        if !buffer.belongs_to(&self.encoder.owner) {
            return Err(Error::Validation(
                "index buffer was created by a different Device".into(),
            ));
        }
        if !buffer.usage().contains(vk::BufferUsageFlags::INDEX_BUFFER) {
            return Err(Error::Validation(
                "index buffer is missing INDEX_BUFFER usage".into(),
            ));
        }
        if offset >= buffer.size() || !offset.is_multiple_of(format.alignment()) {
            return Err(Error::Validation(
                "index buffer offset is outside the buffer or incorrectly aligned".into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_bind_index_buffer(
                self.encoder.raw(),
                buffer.raw(),
                offset,
                format.as_vk(),
            );
        }
        self.encoder.retain_resource(buffer);
        self.index_buffer_bound = true;
        Ok(())
    }

    /// Binds a descriptor heap without ending the rendering scope.
    ///
    /// # Safety
    ///
    /// The heap and all referenced resources must remain live and unmodified
    /// until submission completes.
    pub unsafe fn bind_descriptor_heap(&mut self, heap: &DescriptorHeap) -> Result<()> {
        unsafe { self.encoder.bind_descriptor_heap(heap) }
    }

    /// Records a non-indexed draw after pipeline and dynamic-state validation.
    ///
    /// # Safety
    ///
    /// Shader descriptor mappings and all bound descriptor heap contents must
    /// be valid for the specified vertex and instance ranges.
    pub unsafe fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) -> Result<()> {
        self.validate_draw_state()?;
        if vertices.end < vertices.start || instances.end < instances.start {
            return Err(Error::Validation(
                "draw ranges must be ordered start..end ranges".into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_draw(
                self.encoder.raw(),
                vertices.end - vertices.start,
                instances.end - instances.start,
                vertices.start,
                instances.start,
            );
        }
        Ok(())
    }

    /// Records an indexed draw after index, vertex, pipeline, and dynamic-state
    /// validation.
    ///
    /// # Safety
    ///
    /// Bound buffer ranges and shader descriptor mappings must be valid for all
    /// addressed indices and instances.
    pub unsafe fn draw_indexed(
        &mut self,
        indices: Range<u32>,
        base_vertex: i32,
        instances: Range<u32>,
    ) -> Result<()> {
        self.validate_draw_state()?;
        if !self.index_buffer_bound {
            return Err(Error::Validation(
                "indexed draw requires an index buffer".into(),
            ));
        }
        if indices.end < indices.start || instances.end < instances.start {
            return Err(Error::Validation(
                "indexed draw ranges must be ordered start..end ranges".into(),
            ));
        }
        unsafe {
            self.encoder.owner.device.cmd_draw_indexed(
                self.encoder.raw(),
                indices.end - indices.start,
                instances.end - instances.start,
                indices.start,
                base_vertex,
                instances.start,
            );
        }
        Ok(())
    }

    /// Records one or more `VkDrawIndirectCommand` values.
    ///
    /// # Safety
    ///
    /// Indirect commands, bound vertex buffers, descriptor heaps, and shader
    /// resources must remain valid until submission completes.
    pub unsafe fn draw_indirect(
        &mut self,
        buffer: &Buffer,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) -> Result<()> {
        self.validate_draw_state()?;
        validate_indirect_draw(self.encoder, buffer, offset, draw_count, stride, 16)?;
        unsafe {
            self.encoder.owner.device.cmd_draw_indirect(
                self.encoder.raw(),
                buffer.raw(),
                offset,
                draw_count,
                stride,
            )
        };
        Ok(())
    }

    /// Records one or more `VkDrawIndexedIndirectCommand` values.
    ///
    /// # Safety
    ///
    /// The requirements of [`RenderingEncoder::draw_indirect`] apply, and a
    /// compatible index buffer must be bound.
    pub unsafe fn draw_indexed_indirect(
        &mut self,
        buffer: &Buffer,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) -> Result<()> {
        self.validate_draw_state()?;
        if !self.index_buffer_bound {
            return Err(Error::Validation(
                "indexed indirect draw requires a bound index buffer".into(),
            ));
        }
        validate_indirect_draw(self.encoder, buffer, offset, draw_count, stride, 20)?;
        unsafe {
            self.encoder.owner.device.cmd_draw_indexed_indirect(
                self.encoder.raw(),
                buffer.raw(),
                offset,
                draw_count,
                stride,
            )
        };
        Ok(())
    }

    pub fn end(mut self) {
        self.end_inner();
    }

    fn validate_draw_state(&self) -> Result<()> {
        if !self.pipeline_bound {
            return Err(Error::Validation(
                "draw requires a compatible graphics pipeline".into(),
            ));
        }
        if !self.viewport_set || !self.scissor_set {
            return Err(Error::Validation(
                "draw requires viewport and scissor dynamic state".into(),
            ));
        }
        if (0..self.required_vertex_buffers).any(|slot| !self.bound_vertex_buffers.contains(&slot))
        {
            return Err(Error::Validation(
                "draw requires every pipeline vertex-buffer slot to be bound".into(),
            ));
        }
        Ok(())
    }

    fn end_inner(&mut self) {
        if !self.ended {
            unsafe {
                self.encoder
                    .owner
                    .device
                    .cmd_end_rendering(self.encoder.raw());
            }
            self.ended = true;
        }
    }
}

fn validate_indirect_draw(
    encoder: &CommandEncoder,
    buffer: &Buffer,
    offset: u64,
    draw_count: u32,
    stride: u32,
    command_size: u64,
) -> Result<()> {
    if !buffer.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "indirect draw buffer was created by a different Device".into(),
        ));
    }
    if !buffer
        .usage()
        .contains(vk::BufferUsageFlags::INDIRECT_BUFFER)
    {
        return Err(Error::Validation(
            "indirect draw buffer is missing INDIRECT_BUFFER usage".into(),
        ));
    }
    if draw_count == 0 || !offset.is_multiple_of(4) {
        return Err(Error::Validation(
            "indirect draw count must be non-zero and offset must be four-byte aligned".into(),
        ));
    }
    if draw_count > 1 && (u64::from(stride) < command_size || !stride.is_multiple_of(4)) {
        return Err(Error::Validation(
            "multi-draw indirect stride is too small or misaligned".into(),
        ));
    }
    let span = u64::from(draw_count - 1)
        .checked_mul(u64::from(stride))
        .and_then(|value| value.checked_add(command_size))
        .and_then(|value| offset.checked_add(value));
    if span.is_none_or(|end| end > buffer.size()) {
        return Err(Error::Validation(
            "indirect draw commands exceed the buffer".into(),
        ));
    }
    Ok(())
}

impl Drop for RenderingEncoder<'_> {
    fn drop(&mut self) {
        self.end_inner();
    }
}

fn color_attachment_info(attachment: ColorAttachment<'_>) -> vk::RenderingAttachmentInfo {
    attachment_info(
        attachment.view,
        attachment.layout,
        attachment.resolve_target,
        attachment.resolve_layout,
        attachment.resolve_mode,
        load_op(attachment.load_op, |color| vk::ClearValue {
            color: vk::ClearColorValue { float32: color },
        }),
        attachment.store_op,
    )
}

fn depth_attachment_info(attachment: DepthAttachment<'_>) -> vk::RenderingAttachmentInfo {
    attachment_info(
        attachment.view,
        attachment.layout,
        attachment.resolve_target,
        attachment.resolve_layout,
        attachment.resolve_mode,
        load_op(attachment.load_op, |depth| vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 },
        }),
        attachment.store_op,
    )
}

fn stencil_attachment_info(attachment: StencilAttachment<'_>) -> vk::RenderingAttachmentInfo {
    attachment_info(
        attachment.view,
        attachment.layout,
        attachment.resolve_target,
        attachment.resolve_layout,
        attachment.resolve_mode,
        load_op(attachment.load_op, |stencil| vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 0.0,
                stencil,
            },
        }),
        attachment.store_op,
    )
}

#[allow(clippy::too_many_arguments)]
fn attachment_info(
    view: AttachmentView<'_>,
    layout: vk::ImageLayout,
    resolve_target: Option<AttachmentView<'_>>,
    resolve_layout: vk::ImageLayout,
    resolve_mode: vk::ResolveModeFlags,
    load: (vk::AttachmentLoadOp, vk::ClearValue),
    store: StoreOp,
) -> vk::RenderingAttachmentInfo {
    vk::RenderingAttachmentInfo::builder()
        .image_view(view.raw())
        .image_layout(layout)
        .resolve_mode(resolve_mode)
        .resolve_image_view(resolve_target.map_or(vk::ImageView::null(), AttachmentView::raw))
        .resolve_image_layout(resolve_layout)
        .load_op(load.0)
        .store_op(match store {
            StoreOp::Store => vk::AttachmentStoreOp::STORE,
            StoreOp::Discard => vk::AttachmentStoreOp::DONT_CARE,
        })
        .clear_value(load.1)
        .build()
}

fn load_op<T>(
    operation: LoadOp<T>,
    clear_value: impl FnOnce(T) -> vk::ClearValue,
) -> (vk::AttachmentLoadOp, vk::ClearValue) {
    match operation {
        LoadOp::Load => (vk::AttachmentLoadOp::LOAD, vk::ClearValue::default()),
        LoadOp::Clear(value) => (vk::AttachmentLoadOp::CLEAR, clear_value(value)),
        LoadOp::Discard => (vk::AttachmentLoadOp::DONT_CARE, vk::ClearValue::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_load_and_store_ops_map_without_implicit_preservation() {
        assert_eq!(
            load_op(LoadOp::Clear(0.5), |depth| vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 }
            })
            .0,
            vk::AttachmentLoadOp::CLEAR
        );
        assert_eq!(
            match StoreOp::Discard {
                StoreOp::Store => vk::AttachmentStoreOp::STORE,
                StoreOp::Discard => vk::AttachmentStoreOp::DONT_CARE,
            },
            vk::AttachmentStoreOp::DONT_CARE
        );
    }
}
