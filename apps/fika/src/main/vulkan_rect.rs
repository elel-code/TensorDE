use bytemuck::{Pod, Zeroable};
use vulkan_renderer::{
    BlendState, Buffer, ColorTargetState, Device, DynamicBuffer, DynamicBufferDescriptor,
    FragmentState, GraphicsPipeline, GraphicsPipelineDescriptor, MemoryAllocator, MultisampleState,
    PipelineCache, PrimitiveState, ProgrammableStage, RenderingEncoder, ShaderBindingMap,
    ShaderModuleDescriptor, UploadBatch, VertexAttribute, VertexBufferLayout, VertexState,
    VertexStepMode, vk,
};

use crate::ViewRect;
use crate::ui::render::coordinates::rect_to_vulkan_ndc;
use crate::windowing::PhysicalSize;

use super::vulkan_rect_spirv;

const INITIAL_INSTANCE_CAPACITY: u64 = std::mem::size_of::<VulkanRectInstance>() as u64;

/// One analytic, screen-space rectangle. The Vulkan fragment shader evaluates
/// fill, rounded corners, clipping, and outlines, so Fika does not tessellate
/// curved chrome into CPU-generated strips.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct VulkanRectInstance {
    /// `[left, top, right, bottom]` in Vulkan NDC.
    rect: [f32; 4],
    /// Rectangular clip in the same coordinate system as `rect`.
    clip: [f32; 4],
    color: [f32; 4],
    /// `[radius_x, radius_y, stroke_x, stroke_y]` in NDC.
    style: [f32; 4],
}

impl VulkanRectInstance {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn color(self) -> [f32; 4] {
        self.color
    }

    pub(crate) fn fill(
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        color: [f32; 4],
        size: PhysicalSize<u32>,
    ) -> Option<Self> {
        (rect.width > 0.0 && rect.height > 0.0 && color[3] > 0.0)
            .then(|| Self::new(rect, clip, radius, 0.0, color, size))
    }

    pub(crate) fn outline(
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        stroke_width: f32,
        color: [f32; 4],
        size: PhysicalSize<u32>,
    ) -> Option<Self> {
        (rect.width > 0.0 && rect.height > 0.0 && color[3] > 0.0 && stroke_width > 0.0)
            .then(|| Self::new(rect, clip, radius, stroke_width, color, size))
    }

    fn new(
        rect: ViewRect,
        clip: ViewRect,
        radius: f32,
        stroke_width: f32,
        color: [f32; 4],
        size: PhysicalSize<u32>,
    ) -> Self {
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let radius = radius
            .max(0.0)
            .min(rect.width.max(0.0) * 0.5)
            .min(rect.height.max(0.0) * 0.5);
        let stroke_width = stroke_width
            .max(0.0)
            .min(rect.width.max(0.0) * 0.5)
            .min(rect.height.max(0.0) * 0.5);
        Self {
            rect: rect_to_vulkan_ndc(rect, size),
            clip: rect_to_vulkan_ndc(clip, size),
            color,
            style: [
                radius * 2.0 / width,
                radius * 2.0 / height,
                stroke_width * 2.0 / width,
                stroke_width * 2.0 / height,
            ],
        }
    }
}

/// Native frame layers use analytic instances for Fika's texture-free chrome.
/// Each layer remains separate so a frame can retain its paint ordering without
/// concatenating CPU geometry before submission.
#[derive(Default)]
pub(crate) struct NativeFrameLayers {
    pub(crate) base_rects: Vec<VulkanRectInstance>,
    pub(crate) overlay_rects: Vec<VulkanRectInstance>,
}

/// Borrowed counterpart of [`NativeFrameLayers`] for a single Vulkan submit.
#[derive(Clone, Copy)]
pub(crate) struct NativeFrameLayerRefs<'a> {
    pub(crate) base_rects: &'a [VulkanRectInstance],
    pub(crate) overlay_rects: &'a [VulkanRectInstance],
}

impl NativeFrameLayers {
    pub(crate) fn with_capacities(base_rects: usize, overlay_rects: usize) -> Self {
        Self {
            base_rects: Vec::with_capacity(base_rects),
            overlay_rects: Vec::with_capacity(overlay_rects),
        }
    }

    pub(crate) fn as_refs(&self) -> NativeFrameLayerRefs<'_> {
        NativeFrameLayerRefs {
            base_rects: &self.base_rects,
            overlay_rects: &self.overlay_rects,
        }
    }
}

pub(crate) struct VulkanRectRenderer {
    pipeline: GraphicsPipeline,
    format: vk::Format,
}

pub(crate) struct VulkanRectStream {
    instance_buffer: DynamicBuffer,
    instance_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RectUploadStats {
    pub(crate) bytes: usize,
    pub(crate) reallocated: bool,
}

impl VulkanRectRenderer {
    pub(crate) fn new(
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: vk::Format,
    ) -> Result<Self, String> {
        Ok(Self {
            pipeline: create_pipeline(device, pipeline_cache, format)?,
            format,
        })
    }

    pub(crate) fn create_stream(
        &self,
        allocator: &MemoryAllocator,
        label: &str,
    ) -> Result<VulkanRectStream, String> {
        let instance_buffer = DynamicBuffer::new(
            allocator,
            DynamicBufferDescriptor {
                label: Some(label.into()),
                initial_capacity: INITIAL_INSTANCE_CAPACITY,
                usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            },
        )
        .map_err(|error| format!("create Vulkan analytic-rect instance buffer: {error}"))?;
        Ok(VulkanRectStream {
            instance_buffer,
            instance_count: 0,
        })
    }

    pub(crate) fn set_format(
        &mut self,
        device: &Device,
        pipeline_cache: &PipelineCache,
        format: vk::Format,
    ) -> Result<(), String> {
        if self.format != format {
            self.pipeline = create_pipeline(device, pipeline_cache, format)?;
            self.format = format;
        }
        Ok(())
    }

    pub(crate) fn draw(
        &self,
        rendering: &mut RenderingEncoder<'_>,
        stream: &VulkanRectStream,
    ) -> Result<(), String> {
        if stream.instance_count == 0 {
            return Ok(());
        }
        rendering
            .bind_pipeline(&self.pipeline)
            .map_err(|error| format!("bind Vulkan analytic-rect pipeline: {error}"))?;
        unsafe { rendering.set_vertex_buffer(0, stream.instance_buffer.buffer(), 0) }
            .map_err(|error| format!("bind Vulkan analytic-rect instance buffer: {error}"))?;
        unsafe { rendering.draw(0..6, 0..stream.instance_count as u32) }
            .map_err(|error| format!("draw Vulkan analytic rectangles: {error}"))
    }
}

impl VulkanRectStream {
    pub(crate) fn upload(
        &mut self,
        uploads: &mut UploadBatch<'_>,
        instances: &[VulkanRectInstance],
    ) -> Result<RectUploadStats, String> {
        self.instance_count = instances.len();
        let bytes = bytemuck::cast_slice(instances);
        let upload = self
            .instance_buffer
            .upload(uploads, bytes)
            .map_err(|error| format!("upload Vulkan analytic-rect instances: {error}"))?;
        Ok(RectUploadStats {
            bytes: upload.bytes_written as usize,
            reallocated: upload.reallocated,
        })
    }

    pub(crate) fn vertex_buffer(&self) -> Option<&Buffer> {
        (self.instance_count != 0).then_some(self.instance_buffer.buffer())
    }
}

fn create_pipeline(
    device: &Device,
    pipeline_cache: &PipelineCache,
    format: vk::Format,
) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-analytic-rect-vertex".into()),
            spirv: vulkan_rect_spirv::VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan analytic-rect vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-analytic-rect-fragment".into()),
            spirv: vulkan_rect_spirv::FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan analytic-rect fragment shader: {error}"))?;
    let bindings = ShaderBindingMap::default();
    let attributes = [
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 16,
            shader_location: 1,
        },
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 32,
            shader_location: 2,
        },
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 48,
            shader_location: 3,
        },
    ];
    let buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<VulkanRectInstance>() as u64,
        step_mode: VertexStepMode::Instance,
        attributes: &attributes,
    }];
    let targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::ALPHA_BLENDING),
        write_mask: vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("fika-vulkan-analytic-rect-pipeline"),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex_shader,
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
                    module: &fragment_shader,
                    entry_point: c"main",
                    bindings: &bindings,
                },
                targets: &targets,
            },
            cache: Some(pipeline_cache),
        })
        .map_err(|error| format!("create Vulkan analytic-rect pipeline: {error}"))
}

#[cfg(test)]
mod tests {
    use super::VulkanRectInstance;
    use crate::ViewRect;
    use crate::windowing::PhysicalSize;

    #[test]
    fn analytic_rect_instances_use_one_cache_line_and_preserve_physical_roundness() {
        let instance = VulkanRectInstance::fill(
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 200.0,
                height: 100.0,
            },
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 400.0,
            },
            20.0,
            [0.1, 0.2, 0.3, 0.4],
            PhysicalSize::new(800, 400),
        )
        .unwrap();

        assert_eq!(std::mem::size_of::<VulkanRectInstance>(), 64);
        assert_eq!(instance.rect, [-0.75, -0.75, -0.25, -0.25]);
        assert_eq!(instance.style, [0.05, 0.1, 0.0, 0.0]);
        assert_eq!(instance.color(), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn transparent_fills_and_zero_width_outlines_do_not_allocate_instances() {
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let size = PhysicalSize::new(10, 10);
        assert!(VulkanRectInstance::fill(rect, rect, 0.0, [0.0; 4], size).is_none());
        assert!(VulkanRectInstance::outline(rect, rect, 2.0, 0.0, [1.0; 4], size).is_none());
        assert!(
            VulkanRectInstance::fill(ViewRect { width: 0.0, ..rect }, rect, 0.0, [1.0; 4], size,)
                .is_none()
        );
    }
}
