use std::sync::Arc;

use vulkan_renderer::{
    Adapter, BackendProfile, BinarySemaphore, BinarySemaphoreDescriptor, ColorAttachment,
    CommandEncoderDescriptor, Device, DeviceDescriptor, FrameToken, Instance, InstanceDescriptor,
    LoadOp, MemoryAllocatorConfig, PipelineCache, PipelineCacheDescriptor, PowerPreference,
    PresentMode, PresentStatus, Queue, RenderingDescriptor, RequestAdapterOptions, StoreOp,
    Surface, SurfaceConfiguration, SurfaceConfigurationRequest, Swapchain, SwapchainDescriptor,
    UploadBelt, UploadBeltDescriptor, vk,
};

use crate::ViewRect;
use crate::vulkan_frame::{FrameVertexBuffer, compile_frame_barriers};
use crate::vulkan_rect::{
    NativeFrameLayerRefs, VulkanRectInstance, VulkanRectRenderer, VulkanRectStream,
};
use crate::windowing::{ActiveEventLoop, PhysicalSize, Window};

const FRAME_SLOTS: usize = 3;

struct FrameSlot {
    acquire: BinarySemaphore,
    in_flight: Option<FrameToken>,
}

/// Native Vulkan migration path. It deliberately owns a complete logical
/// device and swapchain so no frame mixes wgpu and vulkan-renderer resources.
pub(crate) struct VulkanState {
    _instance: Instance,
    adapter: Adapter,
    surface: Surface,
    device: Device,
    queue: Queue,
    swapchain: Swapchain,
    pipeline_cache: PipelineCache,
    rect_renderer: VulkanRectRenderer,
    base_rect_stream: VulkanRectStream,
    overlay_rect_stream: VulkanRectStream,
    upload_belt: UploadBelt,
    frame_slots: Vec<FrameSlot>,
    present_complete: Vec<BinarySemaphore>,
    initialized_images: Vec<bool>,
    next_frame_slot: usize,
    frame_count: u64,
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
            })
            .map_err(|error| format!("request Vulkan adapter: {error}"))?;
        let (device, queue) = adapter
            .request_device(DeviceDescriptor {
                label: Some("fika-vulkan-device".into()),
                ..DeviceDescriptor::default()
            })
            .map_err(|error| format!("request Vulkan device: {error}"))?;
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
                    label: Some("fika-vulkan-swapchain"),
                    configuration,
                    old_swapchain: None,
                },
            )
            .map_err(|error| format!("create Vulkan swapchain: {error}"))?;
        let allocator = device
            .create_memory_allocator(MemoryAllocatorConfig {
                device_block_size: 8 * 1024 * 1024,
                image_block_size: 32 * 1024 * 1024,
                upload_block_size: 4 * 1024 * 1024,
                readback_block_size: 4 * 1024 * 1024,
                dedicated_threshold: 16 * 1024 * 1024,
            })
            .map_err(|error| format!("create Fika Vulkan allocator: {error}"))?;
        let pipeline_cache = device
            .create_pipeline_cache(&PipelineCacheDescriptor {
                label: Some("fika-vulkan-pipeline-cache".into()),
                initial_data: Vec::new(),
            })
            .map_err(|error| format!("create Fika Vulkan pipeline cache: {error}"))?;
        let rect_renderer =
            VulkanRectRenderer::new(&device, &pipeline_cache, swapchain.configuration().format)?;
        let base_rect_stream =
            rect_renderer.create_stream(&allocator, "fika-vulkan-base-analytic-rects")?;
        let overlay_rect_stream =
            rect_renderer.create_stream(&allocator, "fika-vulkan-overlay-analytic-rects")?;
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
            .map_err(|error| format!("create Fika Vulkan upload belt: {error}"))?;
        let frame_slots = (0..FRAME_SLOTS)
            .map(|index| {
                device
                    .create_binary_semaphore(&BinarySemaphoreDescriptor {
                        label: Some(format!("fika-vulkan-acquire-{index}")),
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
            rect_renderer,
            base_rect_stream,
            overlay_rect_stream,
            upload_belt,
            frame_slots,
            present_complete,
            initialized_images,
            next_frame_slot: 0,
            frame_count: 0,
        })
    }

    pub(crate) fn size(&self) -> PhysicalSize<u32> {
        let extent = self.swapchain.configuration().extent;
        PhysicalSize::new(extent.width, extent.height)
    }

    pub(crate) const fn frame_count(&self) -> u64 {
        self.frame_count
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

    /// Records analytic rectangle layers in one dynamic-rendering scope and
    /// one acquire/submit/present transaction. Fika's native path submits no
    /// CPU-generated vertex stream here.
    pub(crate) fn present_layers(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: &Window,
        clear: [f32; 4],
        layers: NativeFrameLayerRefs<'_>,
    ) -> Result<(), String> {
        let slot_index = self.next_frame_slot;
        self.next_frame_slot = (self.next_frame_slot + 1) % self.frame_slots.len();
        let slot = &mut self.frame_slots[slot_index];
        if let Some(frame) = slot.in_flight.take() {
            self.queue
                .wait_for(frame, u64::MAX)
                .map_err(|error| format!("wait Vulkan frame slot: {error}"))?;
        }

        let acquired = unsafe { self.swapchain.acquire_next_image(u64::MAX, &slot.acquire) }
            .map_err(|error| format!("acquire Vulkan swapchain image: {error}"))?;
        let image_index = acquired.index() as usize;
        let acquire_status = acquired.status();
        let queue_family = self.device.device_info().queues.graphics;
        let mut uploads = self
            .upload_belt
            .begin(
                &self.queue,
                &CommandEncoderDescriptor {
                    label: Some("fika-vulkan-frame".into()),
                },
            )
            .map_err(|error| format!("begin Vulkan frame uploads: {error}"))?;
        let base_rect_upload = self
            .base_rect_stream
            .upload(&mut uploads, layers.base_rects)?;
        let overlay_rect_upload = self
            .overlay_rect_stream
            .upload(&mut uploads, layers.overlay_rects)?;
        let mut vertex_buffers = Vec::with_capacity(2);
        if let Some(buffer) = self.base_rect_stream.vertex_buffer() {
            vertex_buffers.push(FrameVertexBuffer {
                buffer,
                uploaded: base_rect_upload.bytes != 0,
            });
        }
        if let Some(buffer) = self.overlay_rect_stream.vertex_buffer() {
            vertex_buffers.push(FrameVertexBuffer {
                buffer,
                uploaded: overlay_rect_upload.bytes != 0,
            });
        }
        let barriers = compile_frame_barriers(
            acquired.image(),
            self.initialized_images[image_index],
            &vertex_buffers,
            queue_family,
        )?;
        let encoder = uploads.encoder_mut();
        unsafe { encoder.pipeline_barrier(&barriers.before_render) };
        let color_attachments = [Some(ColorAttachment {
            view: acquired.as_attachment(),
            layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
            resolve_target: None,
            resolve_layout: vk::ImageLayout::UNDEFINED,
            resolve_mode: vk::ResolveModeFlags::NONE,
            load_op: LoadOp::Clear(clear),
            store_op: StoreOp::Store,
        })];
        let rendering = RenderingDescriptor {
            label: Some("fika-vulkan-rendering"),
            render_area: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: acquired.extent(),
            },
            layer_count: 1,
            view_mask: 0,
            color_attachments: &color_attachments,
            depth_attachment: None,
            stencil_attachment: None,
        };
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&rendering)
                .map_err(|error| format!("begin Vulkan rendering: {error}"))?;
            rendering
                .set_viewport(vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: acquired.extent().width as f32,
                    height: acquired.extent().height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                })
                .map_err(|error| format!("set Vulkan viewport: {error}"))?;
            rendering
                .set_scissor(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: acquired.extent(),
                })
                .map_err(|error| format!("set Vulkan scissor: {error}"))?;
            self.rect_renderer
                .draw(&mut rendering, &self.base_rect_stream)?;
            self.rect_renderer
                .draw(&mut rendering, &self.overlay_rect_stream)?;
            rendering.end();
            encoder.pipeline_barrier(&barriers.before_present);
        }
        let acquire_wait = slot
            .acquire
            .wait(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .map_err(|error| format!("create Vulkan acquire wait: {error}"))?;
        let present_complete = &self.present_complete[image_index];
        let frame = unsafe {
            uploads.submit_with_binary_signals(&self.queue, &[acquire_wait], &[present_complete])
        }
        .map_err(|error| format!("submit Vulkan frame: {error}"))?;
        slot.in_flight = Some(frame);
        self.initialized_images[image_index] = true;
        event_loop.pre_present_notify(window.id());
        let present_status = unsafe { acquired.present(&self.queue, &[present_complete]) }
            .map_err(|error| format!("present Vulkan clear frame: {error}"))?;
        self.frame_count = self.frame_count.saturating_add(1);

        if acquire_status == PresentStatus::Suboptimal
            || present_status == PresentStatus::Suboptimal
        {
            self.reconfigure(window.surface_size())?;
        }
        Ok(())
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
                    label: Some("fika-vulkan-swapchain"),
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
        let present_complete = create_present_semaphores(&self.device, replacement.image_count())?;
        self.swapchain = replacement;
        self.present_complete = present_complete;
        self.initialized_images = vec![false; self.swapchain.image_count()];
        for slot in &mut self.frame_slots {
            slot.in_flight = None;
        }
        Ok(())
    }

    pub(crate) fn run_probe(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
    ) -> Result<(), String> {
        let mut renderer = Self::new(Arc::clone(&window))?;
        let size = renderer.size();
        let screen = ViewRect {
            x: 0.0,
            y: 0.0,
            width: size.width.max(1) as f32,
            height: size.height.max(1) as f32,
        };
        let probe = VulkanRectInstance::fill(
            ViewRect {
                x: screen.width * 0.175,
                y: screen.height * 0.275,
                width: screen.width * 0.65,
                height: screen.height * 0.45,
            },
            screen,
            12.0,
            [0.15, 0.48, 0.92, 0.92],
            size,
        )
        .ok_or_else(|| "create Vulkan analytic probe rectangle".to_string())?;
        let probes = [probe];
        renderer.present_layers(
            event_loop,
            &window,
            [0.035, 0.045, 0.065, 1.0],
            NativeFrameLayerRefs {
                base_rects: &probes,
                overlay_rects: &[],
            },
        )?;
        renderer
            .queue
            .wait_idle()
            .map_err(|error| format!("finish Vulkan migration probe: {error}"))
    }
}

fn choose_surface_configuration(
    adapter: &Adapter,
    surface: &Surface,
    features: vulkan_renderer::Features,
    size: PhysicalSize<u32>,
) -> Result<SurfaceConfiguration, String> {
    let capabilities = adapter
        .surface_capabilities(surface)
        .map_err(|error| format!("query Vulkan surface capabilities: {error}"))?;
    let desired_image_count =
        preferred_image_count(capabilities.min_image_count, capabilities.max_image_count);
    let formats = [
        vk::SurfaceFormatKHR {
            format: vk::Format::B8G8R8A8_UNORM,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        },
        vk::SurfaceFormatKHR {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        },
        vk::SurfaceFormatKHR {
            format: vk::Format::R8G8B8A8_UNORM,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        },
    ];
    SurfaceConfiguration::choose(
        &capabilities,
        features,
        SurfaceConfigurationRequest {
            width: size.width.max(1),
            height: size.height.max(1),
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            formats: &formats,
            present_modes: &[PresentMode::FifoLatestReady, PresentMode::Fifo],
            composite_alpha: &[
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::INHERIT,
            ],
            desired_image_count,
        },
    )
    .map_err(|error| format!("choose Vulkan surface configuration: {error}"))
}

fn create_present_semaphores(
    device: &Device,
    image_count: usize,
) -> Result<Vec<BinarySemaphore>, String> {
    (0..image_count)
        .map(|index| {
            device.create_binary_semaphore(&BinarySemaphoreDescriptor {
                label: Some(format!("fika-vulkan-present-{index}")),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("create Vulkan present semaphores: {error}"))
}

fn preferred_image_count(minimum: u32, maximum: Option<u32>) -> u32 {
    maximum.map_or_else(
        || minimum.saturating_add(1),
        |maximum| minimum.saturating_add(1).min(maximum),
    )
}

#[cfg(test)]
mod tests {
    use super::preferred_image_count;

    #[test]
    fn image_count_prefers_one_image_beyond_the_surface_minimum() {
        assert_eq!(preferred_image_count(2, None), 3);
        assert_eq!(preferred_image_count(2, Some(4)), 3);
        assert_eq!(preferred_image_count(2, Some(2)), 2);
        assert_eq!(preferred_image_count(u32::MAX, None), u32::MAX);
    }
}
