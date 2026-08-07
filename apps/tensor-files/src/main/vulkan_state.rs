use std::sync::Arc;

use vulkan_renderer::{
    AccessKind, Adapter, BackendProfile, BarrierBatch, BinarySemaphore, BinarySemaphoreDescriptor,
    ColorAttachment, CommandEncoderDescriptor, CompiledGraph, Device, DeviceDescriptor,
    DmaBufExportDescriptor, ExportedDmaBufImage, Extent2D, Features, ForeignImageState, FrameToken,
    Instance, InstanceDescriptor, LoadOp, MemoryAllocator, MemoryAllocatorConfig, PassId,
    PipelineCache, PipelineCacheDescriptor, PipelineStages, PowerPreference, PresentStatus, Queue,
    Rect2D, RenderGraph, RenderGraphImageState, RenderPass, RenderingDescriptor,
    RequestAdapterOptions, ResolveMode, ResourceId, ResourceKind, ResourceState, ResourceUse,
    StoreOp, Surface, Swapchain, SwapchainDescriptor, TextureFormat, TextureLayout, TextureUsages,
    UploadBelt, UploadBeltDescriptor, Viewport,
};

use crate::IconFrame;
use crate::TextFrame;
use crate::ui::render::quad::QuadVertex;
use crate::vulkan_color::{VulkanColorRenderer, VulkanColorStream};
use crate::vulkan_frame::{
    DirectFramePlanCache, FrameBarrierCache, FrameVertexBuffer, TensorFilesFrameSemantics,
};
use crate::vulkan_icon::VulkanIconRenderer;
use crate::vulkan_rect::{NativeFrameLayerRefs, VulkanRectRenderer, VulkanRectStream};
use crate::vulkan_text::VulkanTextRenderer;
use crate::windowing::{ActiveEventLoop, PhysicalSize, Window};

#[path = "vulkan_state/surface.rs"]
mod surface;
use surface::{choose_surface_configuration, create_present_semaphores};

const FRAME_SLOTS: usize = 3;
const EXPORTED_IMAGE: ResourceId = ResourceId(1);
const EXPORT_DRAW: PassId = PassId(1);
const EXPORT_RELEASE: PassId = PassId(2);

struct FrameSlot {
    acquire: BinarySemaphore,
    in_flight: Option<FrameToken>,
}

pub(crate) struct PresentLayers<'a> {
    pub(crate) clear: [f32; 4],
    pub(crate) colors: &'a [QuadVertex],
    pub(crate) layers: NativeFrameLayerRefs<'a>,
    pub(crate) icons: &'a mut IconFrame,
    pub(crate) text: &'a mut TextFrame,
    pub(crate) colors_are_overlay: bool,
    pub(crate) semantics: TensorFilesFrameSemantics,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VulkanPresentOutcome {
    Presented,
    RetryRequired,
}

/// Persistent native Vulkan renderer. It owns one complete logical device and
/// swapchain; its frames contain only vulkan-renderer resources.
pub(crate) struct VulkanState {
    _instance: Instance,
    adapter: Adapter,
    surface: Surface,
    device: Device,
    queue: Queue,
    swapchain: Swapchain,
    pipeline_cache: PipelineCache,
    allocator: MemoryAllocator,
    color_renderer: VulkanColorRenderer,
    rect_renderer: VulkanRectRenderer,
    icon_renderer: VulkanIconRenderer,
    text_renderer: VulkanTextRenderer,
    base_rect_stream: VulkanRectStream,
    overlay_rect_stream: VulkanRectStream,
    color_stream: VulkanColorStream,
    upload_belt: UploadBelt,
    frame_encoder: CommandEncoderDescriptor,
    export_encoder: CommandEncoderDescriptor,
    frame_plan: DirectFramePlanCache,
    frame_barriers: FrameBarrierCache,
    export_release_graph: CompiledGraph,
    export_release_barrier: BarrierBatch,
    frame_slots: Vec<FrameSlot>,
    present_complete: Vec<BinarySemaphore>,
    initialized_images: Vec<bool>,
    next_frame_slot: usize,
    frame_count: u64,
    last_submission: Option<FrameToken>,
}

impl VulkanState {
    pub(crate) fn new(window: Arc<Window>) -> Result<Self, String> {
        let host = window.surface_handle();
        let descriptor = InstanceDescriptor::for_window(BackendProfile::Vulkan14, &host)
            .map_err(|error| format!("Vulkan instance descriptor: {error}"))?;
        let instance = Instance::new(descriptor)
            .map_err(|error| format!("create Vulkan instance: {error}"))?;
        let surface = instance
            .create_surface(Arc::new(host))
            .map_err(|error| format!("create Vulkan Wayland surface: {error}"))?;
        let adapter = instance
            .request_adapter(RequestAdapterOptions {
                power_preference: PowerPreference::Discrete,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                selector: None,
            })
            .map_err(|error| format!("request Vulkan adapter: {error}"))?;
        let mut required_features = DeviceDescriptor::default().required_features;
        if adapter
            .features()
            .contains(Features::EXTERNAL_MEMORY_DMA_BUF)
        {
            required_features |= Features::EXTERNAL_MEMORY_DMA_BUF;
        }
        let (device, queue) = adapter
            .request_device(DeviceDescriptor {
                label: Some("tensor-files-vulkan-device".into()),
                required_features,
                ..DeviceDescriptor::default()
            })
            .map_err(|error| format!("request Vulkan device: {error}"))?;
        let allocator = device
            .create_memory_allocator(MemoryAllocatorConfig {
                device_block_size: 8 * 1024 * 1024,
                image_block_size: 32 * 1024 * 1024,
                upload_block_size: 4 * 1024 * 1024,
                readback_block_size: 4 * 1024 * 1024,
                dedicated_threshold: 16 * 1024 * 1024,
            })
            .map_err(|error| format!("create Tensor Files Vulkan allocator: {error}"))?;
        Self::from_shared_device(window, instance, adapter, surface, device, queue, allocator)
    }

    pub(crate) fn new_shared(window: Arc<Window>, shared: &Self) -> Result<Self, String> {
        let host = window.surface_handle();
        let instance = shared._instance.clone();
        let surface = instance
            .create_surface(Arc::new(host))
            .map_err(|error| format!("create shared Vulkan Wayland surface: {error}"))?;
        Self::from_shared_device(
            window,
            instance,
            shared.adapter.clone(),
            surface,
            shared.device.clone(),
            shared.queue.clone(),
            shared.allocator.clone(),
        )
    }

    fn from_shared_device(
        window: Arc<Window>,
        instance: Instance,
        adapter: Adapter,
        surface: Surface,
        device: Device,
        queue: Queue,
        allocator: MemoryAllocator,
    ) -> Result<Self, String> {
        let configuration = choose_surface_configuration(
            &adapter,
            &surface,
            device.features(),
            window.surface_size(),
        )?;
        let swapchain = device
            .create_swapchain(
                &surface,
                &SwapchainDescriptor {
                    label: Some("tensor-files-vulkan-swapchain"),
                    configuration,
                    old_swapchain: None,
                },
            )
            .map_err(|error| format!("create Vulkan swapchain: {error}"))?;
        let pipeline_cache = device
            .create_pipeline_cache(&PipelineCacheDescriptor {
                label: Some("tensor-files-vulkan-pipeline-cache".into()),
                initial_data: Vec::new(),
            })
            .map_err(|error| format!("create Tensor Files Vulkan pipeline cache: {error}"))?;
        let color_renderer =
            VulkanColorRenderer::new(&device, &pipeline_cache, swapchain.configuration().format)?;
        let rect_renderer =
            VulkanRectRenderer::new(&device, &pipeline_cache, swapchain.configuration().format)?;
        let icon_renderer = VulkanIconRenderer::new(
            &device,
            &allocator,
            &pipeline_cache,
            swapchain.configuration().format,
        )?;
        let text_renderer = VulkanTextRenderer::new(
            &device,
            &allocator,
            &pipeline_cache,
            swapchain.configuration().format,
        )?;
        let base_rect_stream =
            rect_renderer.create_stream(&allocator, "tensor-files-vulkan-base-analytic-rects")?;
        let overlay_rect_stream = rect_renderer
            .create_stream(&allocator, "tensor-files-vulkan-overlay-analytic-rects")?;
        let color_stream =
            color_renderer.create_stream(&allocator, "tensor-files-vulkan-color-vertices")?;
        let queue_family = device.device_info().queues.graphics;
        let frame_barriers = FrameBarrierCache::new(queue_family)?;
        let export_release_graph = compile_export_release_graph(queue_family)?;
        let upload_belt = device
            .create_upload_belt(
                &allocator,
                UploadBeltDescriptor {
                    chunk_size: 1024 * 1024,
                    max_chunks: 4,
                    max_bytes: 8 * 1024 * 1024,
                    offset_alignment: 256,
                },
            )
            .map_err(|error| format!("create Tensor Files Vulkan upload belt: {error}"))?;
        let frame_slots = (0..FRAME_SLOTS)
            .map(|index| {
                device
                    .create_binary_semaphore(&BinarySemaphoreDescriptor {
                        label: Some(format!("tensor-files-vulkan-acquire-{index}")),
                    })
                    .map(|acquire| FrameSlot {
                        acquire,
                        in_flight: None,
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("create Vulkan acquire semaphores: {error}"))?;
        let present_complete = create_present_semaphores(&device, swapchain.image_count())?;
        let initialized_images = vec![false; swapchain.image_count()];

        Ok(Self {
            _instance: instance,
            adapter,
            surface,
            device,
            queue,
            swapchain,
            pipeline_cache,
            allocator,
            color_renderer,
            rect_renderer,
            icon_renderer,
            text_renderer,
            base_rect_stream,
            overlay_rect_stream,
            color_stream,
            upload_belt,
            frame_encoder: CommandEncoderDescriptor {
                label: Some("tensor-files-vulkan-frame".into()),
            },
            export_encoder: CommandEncoderDescriptor {
                label: Some("tensor-files-vulkan-dnd-preview".into()),
            },
            frame_plan: DirectFramePlanCache::default(),
            frame_barriers,
            export_release_graph,
            export_release_barrier: BarrierBatch::with_capacity(0, 1),
            frame_slots,
            present_complete,
            initialized_images,
            next_frame_slot: 0,
            frame_count: 0,
            last_submission: None,
        })
    }

    pub(crate) fn size(&self) -> PhysicalSize<u32> {
        let extent = self.swapchain.configuration().extent;
        PhysicalSize::new(extent.width, extent.height)
    }

    pub(crate) const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub(crate) fn icon_resident_index(&self) -> crate::IconGpuResidentIndex {
        self.icon_renderer.resident_index()
    }

    pub(crate) fn icon_resident_lookup(&self) -> &dyn crate::IconGpuResidentLookup {
        &self.icon_renderer
    }

    pub(crate) fn external_memory_dma_buf_supported(&self) -> bool {
        self.device
            .features()
            .contains(Features::EXTERNAL_MEMORY_DMA_BUF)
    }

    pub(crate) fn exportable_dmabuf_formats(
        &self,
    ) -> Result<Vec<wayland_client_runtime::DmabufFormat>, String> {
        crate::ui::render::dmabuf::vulkan_exportable_formats(&self.device)
    }

    pub(crate) fn render_exported_layers(
        &mut self,
        plan: crate::ui::render::dmabuf::DmabufExportPlan,
        extent: Extent2D,
        colors: &[QuadVertex],
        layers: NativeFrameLayerRefs<'_>,
        icons: &mut IconFrame,
        text: &mut TextFrame,
    ) -> Result<ExportedDmaBufImage, String> {
        let (format, components) = crate::ui::render::dmabuf::vulkan_format_for_fourcc(plan.fourcc)
            .ok_or_else(|| format!("unsupported exported dma-buf fourcc 0x{:08x}", plan.fourcc))?;
        let exported = self
            .device
            .create_exportable_dma_buf_image(&DmaBufExportDescriptor {
                label: Some("tensor-files-vulkan-dnd-preview".into()),
                format,
                extent,
                modifiers: vec![plan.modifier],
                usage: TextureUsages::COLOR_ATTACHMENT,
                components,
            })
            .map_err(|error| format!("create Vulkan drag-preview dma-buf: {error}"))?;
        self.set_render_format(format)?;
        let mut uploads = self
            .upload_belt
            .begin(&self.queue, &self.export_encoder)
            .map_err(|error| format!("begin Vulkan drag-preview uploads: {error}"))?;
        let color_vertex_uploaded = self.color_stream.upload(&mut uploads, colors)?;
        let base_rect_upload = self
            .base_rect_stream
            .upload(&mut uploads, layers.base_rects)?;
        let overlay_rect_upload = self
            .overlay_rect_stream
            .upload(&mut uploads, layers.overlay_rects)?;
        let text_vertex_uploaded =
            self.text_renderer
                .upload(&self.allocator, &mut uploads, text, self.last_submission)?;
        let icon_vertex_uploaded = self.icon_renderer.upload(
            &self.device,
            &self.allocator,
            &mut uploads,
            icons,
            self.last_submission,
        )?;
        let vertex_buffers = [
            FrameVertexBuffer {
                buffer: self.color_stream.buffer(),
                uploaded: color_vertex_uploaded,
            },
            FrameVertexBuffer {
                buffer: self.base_rect_stream.buffer(),
                uploaded: base_rect_upload.bytes != 0,
            },
            FrameVertexBuffer {
                buffer: self.overlay_rect_stream.buffer(),
                uploaded: overlay_rect_upload.bytes != 0,
            },
            FrameVertexBuffer {
                buffer: self.text_renderer.buffer(),
                uploaded: text_vertex_uploaded,
            },
            FrameVertexBuffer {
                buffer: self.icon_renderer.buffer(),
                uploaded: icon_vertex_uploaded,
            },
        ];
        let (before_render, _) =
            self.frame_barriers
                .resolve(exported.resource_binding(), false, &vertex_buffers)?;
        let release_bindings = [(EXPORTED_IMAGE, exported.resource_binding())];
        self.export_release_graph
            .fill_barrier_batch_before_from_slice(
                EXPORT_RELEASE,
                &release_bindings,
                &mut self.export_release_barrier,
            )
            .map_err(|error| format!("resolve Vulkan drag-preview release barrier: {error}"))?;
        let encoder = uploads.encoder_mut();
        unsafe { encoder.pipeline_barrier(before_render) };
        encoder.retain_resource(&exported);
        let color_attachments = [Some(ColorAttachment {
            view: exported.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear([0.0, 0.0, 0.0, 0.0]),
            store_op: StoreOp::Store,
        })];
        let descriptor = RenderingDescriptor {
            label: Some("tensor-files-vulkan-dnd-preview-rendering"),
            render_area: Rect2D::new(0, 0, extent.width, extent.height),
            layer_count: 1,
            view_mask: 0,
            color_attachments: &color_attachments,
            depth_attachment: None,
            stencil_attachment: None,
            multisampled_render_to_single_sampled: None,
        };
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&descriptor)
                .map_err(|error| format!("begin Vulkan drag-preview rendering: {error}"))?;
            rendering
                .set_viewport(Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                })
                .map_err(|error| format!("set Vulkan drag-preview viewport: {error}"))?;
            rendering
                .set_scissor(Rect2D::new(0, 0, extent.width, extent.height))
                .map_err(|error| format!("set Vulkan drag-preview scissor: {error}"))?;
            self.rect_renderer
                .draw(&mut rendering, &self.base_rect_stream)?;
            self.color_renderer
                .draw(&mut rendering, &self.color_stream)?;
            self.icon_renderer.draw_content(&mut rendering)?;
            self.text_renderer.draw(&mut rendering)?;
            self.rect_renderer
                .draw(&mut rendering, &self.overlay_rect_stream)?;
            self.icon_renderer.draw_overlay(&mut rendering)?;
            rendering.end();
            encoder.pipeline_barrier(&self.export_release_barrier);
        }
        let frame = uploads
            .submit(&self.queue, &[])
            .map_err(|error| format!("submit Vulkan drag-preview frame: {error}"))?;
        self.queue
            .wait_for(frame, u64::MAX)
            .map_err(|error| format!("wait Vulkan drag-preview frame: {error}"))?;
        self.last_submission = Some(frame);
        self.set_render_format(self.swapchain.configuration().format)?;
        Ok(exported)
    }

    fn set_render_format(&mut self, format: TextureFormat) -> Result<(), String> {
        self.color_renderer
            .set_format(&self.device, &self.pipeline_cache, format)?;
        self.rect_renderer
            .set_format(&self.device, &self.pipeline_cache, format)?;
        self.text_renderer
            .set_format(&self.device, &self.pipeline_cache, format)?;
        self.icon_renderer
            .set_format(&self.device, &self.pipeline_cache, format)
    }

    /// Replaces the swapchain only when its configured extent differs from
    /// the window's current physical extent.
    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        let current = self.size();
        let requested = PhysicalSize::new(size.width.max(1), size.height.max(1));
        if current != requested {
            self.reconfigure(requested)?;
        }
        Ok(())
    }

    /// Waits only at an application shutdown boundary.
    pub(crate) fn wait_idle(&self, label: &str) -> Result<(), String> {
        self.queue
            .wait_idle()
            .map_err(|error| format!("idle Vulkan device for {label}: {error}"))
    }

    /// Records analytic chrome, resident icons, and R8-atlas text in one
    /// dynamic-rendering scope and one acquire/submit/present transaction.
    pub(crate) fn present_layers(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: &Window,
        frame: PresentLayers<'_>,
    ) -> Result<VulkanPresentOutcome, String> {
        let PresentLayers {
            clear,
            colors,
            layers,
            icons,
            text,
            colors_are_overlay,
            semantics,
        } = frame;
        self.set_render_format(self.swapchain.configuration().format)?;
        self.frame_plan.require(
            self.swapchain.configuration().extent,
            self.swapchain.configuration().format,
            self.swapchain.configuration().usage,
            semantics,
        )?;
        let slot_index = self.next_frame_slot;
        self.next_frame_slot = (self.next_frame_slot + 1) % self.frame_slots.len();
        if let Some(frame) = self.frame_slots[slot_index].in_flight.take() {
            self.queue
                .wait_for(frame, u64::MAX)
                .map_err(|error| format!("wait Vulkan frame slot: {error}"))?;
        }
        let completed = self
            .queue
            .completed_timeline()
            .map_err(|error| format!("query completed Vulkan text frames: {error}"))?;
        self.text_renderer.reclaim(completed);
        self.icon_renderer.reclaim(completed);

        let acquired = match unsafe {
            self.swapchain
                .acquire_next_image(u64::MAX, &self.frame_slots[slot_index].acquire)
        } {
            Ok(acquired) => acquired,
            Err(error) if error.is_surface_out_of_date() => {
                self.reconfigure(window.surface_size())?;
                unsafe {
                    self.swapchain
                        .acquire_next_image(u64::MAX, &self.frame_slots[slot_index].acquire)
                }
                .map_err(|retry| {
                    format!("acquire Vulkan swapchain image after replacement: {retry}")
                })?
            }
            Err(error) => {
                return Err(format!("acquire Vulkan swapchain image: {error}"));
            }
        };
        let image_index = acquired.index() as usize;
        let acquire_status = acquired.status();
        let mut uploads = self
            .upload_belt
            .begin(&self.queue, &self.frame_encoder)
            .map_err(|error| format!("begin Vulkan frame uploads: {error}"))?;
        let base_rect_upload = self
            .base_rect_stream
            .upload(&mut uploads, layers.base_rects)?;
        let overlay_rect_upload = self
            .overlay_rect_stream
            .upload(&mut uploads, layers.overlay_rects)?;
        let color_vertex_uploaded = self.color_stream.upload(&mut uploads, colors)?;
        let text_vertex_uploaded =
            self.text_renderer
                .upload(&self.allocator, &mut uploads, text, self.last_submission)?;
        let icon_vertex_uploaded = self.icon_renderer.upload(
            &self.device,
            &self.allocator,
            &mut uploads,
            icons,
            self.last_submission,
        )?;
        let vertex_buffers = [
            FrameVertexBuffer {
                buffer: self.color_stream.buffer(),
                uploaded: color_vertex_uploaded,
            },
            FrameVertexBuffer {
                buffer: self.base_rect_stream.buffer(),
                uploaded: base_rect_upload.bytes != 0,
            },
            FrameVertexBuffer {
                buffer: self.overlay_rect_stream.buffer(),
                uploaded: overlay_rect_upload.bytes != 0,
            },
            FrameVertexBuffer {
                buffer: self.text_renderer.buffer(),
                uploaded: text_vertex_uploaded,
            },
            FrameVertexBuffer {
                buffer: self.icon_renderer.buffer(),
                uploaded: icon_vertex_uploaded,
            },
        ];
        let (before_render, before_present) = self.frame_barriers.resolve(
            acquired.resource_binding(),
            self.initialized_images[image_index],
            &vertex_buffers,
        )?;
        let encoder = uploads.encoder_mut();
        unsafe { encoder.pipeline_barrier(before_render) };
        let color_attachments = [Some(ColorAttachment {
            view: acquired.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear(clear),
            store_op: StoreOp::Store,
        })];
        let rendering = RenderingDescriptor {
            label: Some("tensor-files-vulkan-rendering"),
            render_area: Rect2D::new(0, 0, acquired.extent().width, acquired.extent().height),
            layer_count: 1,
            view_mask: 0,
            color_attachments: &color_attachments,
            depth_attachment: None,
            stencil_attachment: None,
            multisampled_render_to_single_sampled: None,
        };
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&rendering)
                .map_err(|error| format!("begin Vulkan rendering: {error}"))?;
            rendering
                .set_viewport(Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: acquired.extent().width as f32,
                    height: acquired.extent().height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                })
                .map_err(|error| format!("set Vulkan viewport: {error}"))?;
            rendering
                .set_scissor(Rect2D::new(
                    0,
                    0,
                    acquired.extent().width,
                    acquired.extent().height,
                ))
                .map_err(|error| format!("set Vulkan scissor: {error}"))?;
            self.rect_renderer
                .draw(&mut rendering, &self.base_rect_stream)?;
            if !colors_are_overlay {
                self.color_renderer
                    .draw(&mut rendering, &self.color_stream)?;
            }
            self.icon_renderer.draw_content(&mut rendering)?;
            self.text_renderer.draw_content(&mut rendering)?;
            if colors_are_overlay {
                self.color_renderer
                    .draw(&mut rendering, &self.color_stream)?;
            }
            self.rect_renderer
                .draw(&mut rendering, &self.overlay_rect_stream)?;
            self.text_renderer.draw_overlay(&mut rendering)?;
            self.icon_renderer.draw_overlay(&mut rendering)?;
            rendering.end();
            encoder.pipeline_barrier(before_present);
        }
        let acquire_wait = self.frame_slots[slot_index]
            .acquire
            .wait(PipelineStages::COLOR_ATTACHMENT_OUTPUT)
            .map_err(|error| format!("create Vulkan acquire wait: {error}"))?;
        let present_complete = &self.present_complete[image_index];
        let frame = unsafe {
            uploads.submit_with_binary_signals(&self.queue, &[acquire_wait], &[present_complete])
        }
        .map_err(|error| format!("submit Vulkan frame: {error}"))?;
        self.frame_slots[slot_index].in_flight = Some(frame);
        self.last_submission = Some(frame);
        self.initialized_images[image_index] = true;
        event_loop.pre_present_notify(window.id());
        let present_status = match unsafe { acquired.present(&self.queue, &[present_complete]) } {
            Ok(status) => status,
            Err(error) if error.is_surface_out_of_date() => {
                self.reconfigure(window.surface_size())?;
                return Ok(VulkanPresentOutcome::RetryRequired);
            }
            Err(error) => return Err(format!("present Vulkan frame: {error}")),
        };
        self.frame_count = self.frame_count.saturating_add(1);

        if acquire_status == PresentStatus::Suboptimal
            || present_status == PresentStatus::Suboptimal
        {
            self.reconfigure(window.surface_size())?;
        }
        Ok(VulkanPresentOutcome::Presented)
    }

    fn reconfigure(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.queue
            .wait_idle()
            .map_err(|error| format!("idle Vulkan device for swapchain replacement: {error}"))?;
        let configuration = choose_surface_configuration(
            &self.adapter,
            &self.surface,
            self.device.features(),
            size,
        )?;
        let replacement = self
            .device
            .create_swapchain(
                &self.surface,
                &SwapchainDescriptor {
                    label: Some("tensor-files-vulkan-swapchain"),
                    configuration,
                    old_swapchain: Some(&self.swapchain),
                },
            )
            .map_err(|error| format!("replace Vulkan swapchain: {error}"))?;
        self.rect_renderer.set_format(
            &self.device,
            &self.pipeline_cache,
            replacement.configuration().format,
        )?;
        self.color_renderer.set_format(
            &self.device,
            &self.pipeline_cache,
            replacement.configuration().format,
        )?;
        self.text_renderer.set_format(
            &self.device,
            &self.pipeline_cache,
            replacement.configuration().format,
        )?;
        self.icon_renderer.set_format(
            &self.device,
            &self.pipeline_cache,
            replacement.configuration().format,
        )?;
        let present_complete = create_present_semaphores(&self.device, replacement.image_count())?;
        self.swapchain = replacement;
        self.present_complete = present_complete;
        self.initialized_images = vec![false; self.swapchain.image_count()];
        for slot in &mut self.frame_slots {
            slot.in_flight = None;
        }
        Ok(())
    }
}

fn compile_export_release_graph(
    queue_family: u32,
) -> Result<vulkan_renderer::CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        EXPORTED_IMAGE,
        ResourceKind::Image,
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
    );
    graph.add_pass(RenderPass {
        id: EXPORT_DRAW,
        label: "tensor-files-vulkan-export-draw".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: EXPORTED_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::color_attachment_write(queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: EXPORT_RELEASE,
        label: "tensor-files-vulkan-export-release".into(),
        depends_on: vec![EXPORT_DRAW],
        resources: vec![ResourceUse {
            resource: EXPORTED_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::foreign_image(ForeignImageState::General),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan dma-buf export graph: {error}"))
}
