//! Shared strict Vulkan presentation for ordinary Wayland application surfaces.
//!
//! Product crates retain their own scene and interaction models. This module
//! owns only the expensive presentation lifetime: one Vulkan instance/device,
//! per-surface swapchains, bounded frame slots, and the cold/retained image
//! layout graph. There is deliberately no SHM or CPU presentation path.

use std::{collections::BTreeMap, sync::Arc};

use vulkan_renderer::{
    AccessKind, Adapter, BackendProfile, BinarySemaphore, BinarySemaphoreDescriptor,
    ColorAttachment, ColorSpace, CommandEncoderDescriptor, CompiledGraph, CompositeAlphaMode,
    Device, DeviceDescriptor, Extent2D, FrameToken, Instance, InstanceDescriptor, LoadOp, PassId,
    PipelineStages, PowerPreference, PresentMode, PresentStatus, Queue, Rect2D, RenderGraph,
    RenderGraphError, RenderGraphImageState, RenderPass, RenderingDescriptor,
    RequestAdapterOptions, ResolveMode, ResourceId, ResourceKind, ResourceState, ResourceUse,
    StoreOp, Surface, SurfaceConfiguration, SurfaceConfigurationRequest, SurfaceFormat, Swapchain,
    SwapchainDescriptor, TextureFormat, TextureLayout, TextureUsages,
};
use wayland_client_runtime::{SurfaceHandle, SurfaceId};

const FRAME_SLOTS: usize = 3;
const SURFACE_IMAGE: ResourceId = ResourceId(1);
const DRAW_PASS: PassId = PassId(1);
const PRESENT_PASS: PassId = PassId(2);

/// One retained rectangle in physical surface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorRect {
    pub rect: Rect2D,
    pub color: [f32; 4],
}

/// Product-owned content submitted to [`SurfacePresenter::present`].
#[derive(Clone, Copy, Debug)]
pub struct SurfaceFrame<'a> {
    pub clear: [f32; 4],
    pub rectangles: &'a [ColorRect],
}

/// Vulkan ownership root shared by one ordinary application process.
pub struct SurfacePresenter {
    instance: Instance,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    initial_graph: CompiledGraph,
    retained_graph: CompiledGraph,
    surfaces: BTreeMap<SurfaceId, PresentedSurface>,
}

struct PresentedSurface {
    label: String,
    surface: Surface,
    swapchain: Swapchain,
    frame_slots: Vec<FrameSlot>,
    present_complete: Vec<BinarySemaphore>,
    initialized_images: Vec<bool>,
    next_frame_slot: usize,
}

struct FrameSlot {
    acquire: BinarySemaphore,
    in_flight: Option<FrameToken>,
}

impl SurfacePresenter {
    /// Create the process presentation root and its first surface.
    pub fn new(
        surface_id: SurfaceId,
        host: Arc<SurfaceHandle>,
        extent: Extent2D,
        label: impl Into<String>,
    ) -> Result<Self, SurfacePresenterError> {
        require_extent(surface_id, extent)?;
        let label = label.into();
        let descriptor = InstanceDescriptor::for_window(BackendProfile::Roadmap2026, host.as_ref())
            .map_err(|source| renderer_error("build Vulkan instance descriptor", source))?;
        let instance = Instance::new(descriptor)
            .map_err(|source| renderer_error("create Vulkan instance", source))?;
        let surface = instance
            .create_surface(host)
            .map_err(|source| renderer_error("create Vulkan surface", source))?;
        let adapter = instance
            .request_adapter(RequestAdapterOptions {
                power_preference: PowerPreference::Discrete,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                selector: None,
            })
            .map_err(|source| renderer_error("select Vulkan presentation adapter", source))?;
        let (device, queue) = adapter
            .request_device(DeviceDescriptor {
                label: Some(label.clone()),
                ..DeviceDescriptor::default()
            })
            .map_err(|source| renderer_error("create Vulkan presentation device", source))?;
        let queue_family = device.device_info().queues.graphics;
        let initial_graph = compile_graph(queue_family, false)?;
        let retained_graph = compile_graph(queue_family, true)?;
        let presented =
            PresentedSurface::new(surface_id, label, surface, extent, &adapter, &device)?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            initial_graph,
            retained_graph,
            surfaces: BTreeMap::from([(surface_id, presented)]),
        })
    }

    /// Add or resize a surface while retaining the device and graph lifetime.
    pub fn ensure_surface(
        &mut self,
        surface_id: SurfaceId,
        host: Arc<SurfaceHandle>,
        extent: Extent2D,
        label: impl Into<String>,
    ) -> Result<(), SurfacePresenterError> {
        require_extent(surface_id, extent)?;
        if let Some(surface) = self.surfaces.get(&surface_id) {
            if surface.swapchain.configuration().extent == extent {
                return Ok(());
            }
            return self.reconfigure(surface_id, extent);
        }
        let surface = self
            .instance
            .create_surface(host)
            .map_err(|source| renderer_error("create Vulkan surface", source))?;
        let presented = PresentedSurface::new(
            surface_id,
            label.into(),
            surface,
            extent,
            &self.adapter,
            &self.device,
        )?;
        self.surfaces.insert(surface_id, presented);
        Ok(())
    }

    /// Record and submit one retained frame, retrying a single out-of-date
    /// acquire after rebuilding the affected swapchain.
    pub fn present(
        &mut self,
        surface_id: SurfaceId,
        extent: Extent2D,
        frame: SurfaceFrame<'_>,
    ) -> Result<(), SurfacePresenterError> {
        for attempt in 0..2 {
            let surface = self
                .surfaces
                .get_mut(&surface_id)
                .ok_or(SurfacePresenterError::UnknownSurface(surface_id))?;
            match surface.present(
                &self.device,
                &self.queue,
                &self.initial_graph,
                &self.retained_graph,
                frame,
            )? {
                PresentOutcome::Presented => return Ok(()),
                PresentOutcome::PresentedSuboptimally => {
                    self.reconfigure(surface_id, extent)?;
                    return Ok(());
                }
                PresentOutcome::SurfaceOutOfDate if attempt == 0 => {
                    self.reconfigure(surface_id, extent)?;
                }
                PresentOutcome::SurfaceOutOfDate => {
                    return Err(SurfacePresenterError::RepeatedSurfaceOutOfDate(surface_id));
                }
            }
        }
        unreachable!("bounded present retry always returns")
    }

    pub fn remove_surface(&mut self, surface_id: SurfaceId) -> Result<(), SurfacePresenterError> {
        if self.surfaces.remove(&surface_id).is_some() {
            self.queue
                .wait_idle()
                .map_err(|source| renderer_error("idle queue before surface removal", source))?;
        }
        Ok(())
    }

    fn reconfigure(
        &mut self,
        surface_id: SurfaceId,
        extent: Extent2D,
    ) -> Result<(), SurfacePresenterError> {
        require_extent(surface_id, extent)?;
        self.queue
            .wait_idle()
            .map_err(|source| renderer_error("idle queue before swapchain replacement", source))?;
        let surface = self
            .surfaces
            .get_mut(&surface_id)
            .ok_or(SurfacePresenterError::UnknownSurface(surface_id))?;
        surface.reconfigure(surface_id, extent, &self.adapter, &self.device)
    }
}

impl PresentedSurface {
    fn new(
        surface_id: SurfaceId,
        label: String,
        surface: Surface,
        extent: Extent2D,
        adapter: &Adapter,
        device: &Device,
    ) -> Result<Self, SurfacePresenterError> {
        let configuration = choose_configuration(adapter, &surface, device, extent)?;
        let swapchain = device
            .create_swapchain(
                &surface,
                &SwapchainDescriptor {
                    label: Some(&label),
                    configuration,
                    old_swapchain: None,
                },
            )
            .map_err(|source| renderer_error("create Vulkan swapchain", source))?;
        let frame_slots = create_frame_slots(device, surface_id)?;
        let present_complete =
            create_present_semaphores(device, surface_id, swapchain.image_count())?;
        let initialized_images = vec![false; swapchain.image_count()];
        Ok(Self {
            label,
            surface,
            swapchain,
            frame_slots,
            present_complete,
            initialized_images,
            next_frame_slot: 0,
        })
    }

    fn present(
        &mut self,
        device: &Device,
        queue: &Queue,
        initial_graph: &CompiledGraph,
        retained_graph: &CompiledGraph,
        frame: SurfaceFrame<'_>,
    ) -> Result<PresentOutcome, SurfacePresenterError> {
        let slot_index = self.next_frame_slot;
        self.next_frame_slot = (slot_index + 1) % self.frame_slots.len();
        if let Some(in_flight) = self.frame_slots[slot_index].in_flight.take() {
            queue
                .wait_for(in_flight, u64::MAX)
                .map_err(|source| renderer_error("wait for application frame slot", source))?;
        }
        let acquired = match unsafe {
            self.swapchain
                .acquire_next_image(u64::MAX, &self.frame_slots[slot_index].acquire)
        } {
            Ok(acquired) => acquired,
            Err(error) if error.is_surface_out_of_date() => {
                return Ok(PresentOutcome::SurfaceOutOfDate);
            }
            Err(source) => {
                return Err(renderer_error(
                    "acquire application swapchain image",
                    source,
                ));
            }
        };
        let image_index = acquired.index() as usize;
        let graph = if self.initialized_images[image_index] {
            retained_graph
        } else {
            initial_graph
        };
        let bindings = BTreeMap::from([(SURFACE_IMAGE, acquired.resource_binding())]);
        let before_draw = graph.barrier_batch_before(DRAW_PASS, &bindings)?;
        let before_present = graph.barrier_batch_before(PRESENT_PASS, &bindings)?;
        let mut encoder = device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some(self.label.clone().into()),
            })
            .map_err(|source| renderer_error("create application command encoder", source))?;
        unsafe { encoder.pipeline_barrier(&before_draw) };
        let attachments = [Some(ColorAttachment {
            view: acquired.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear(frame.clear),
            store_op: StoreOp::Store,
        })];
        let rendering = RenderingDescriptor {
            label: Some(&self.label),
            render_area: Rect2D::new(0, 0, acquired.extent().width, acquired.extent().height),
            layer_count: 1,
            view_mask: 0,
            color_attachments: &attachments,
            depth_attachment: None,
            stencil_attachment: None,
            multisampled_render_to_single_sampled: None,
        };
        unsafe {
            let mut rendering = encoder
                .begin_rendering(&rendering)
                .map_err(|source| renderer_error("begin application dynamic rendering", source))?;
            for draw in frame.rectangles {
                if draw.rect.extent.width != 0 && draw.rect.extent.height != 0 {
                    rendering
                        .clear_color_attachment(0, draw.color, &[draw.rect])
                        .map_err(|source| renderer_error("draw application color rect", source))?;
                }
            }
            rendering.end();
            encoder.pipeline_barrier(&before_present);
        }
        let command = encoder
            .finish()
            .map_err(|source| renderer_error("finish application command buffer", source))?;
        let acquire_wait = self.frame_slots[slot_index]
            .acquire
            .wait(PipelineStages::COLOR_ATTACHMENT_OUTPUT)
            .map_err(|source| renderer_error("build application acquire wait", source))?;
        let present_complete = &self.present_complete[image_index];
        let in_flight = unsafe {
            queue.submit_with_binary_signals([command], &[acquire_wait], &[present_complete])
        }
        .map_err(|source| renderer_error("submit application frame", source))?;
        self.frame_slots[slot_index].in_flight = Some(in_flight);
        self.initialized_images[image_index] = true;
        let acquire_status = acquired.status();
        let present_status = match unsafe { acquired.present(queue, &[present_complete]) } {
            Ok(status) => status,
            Err(error) if error.is_surface_out_of_date() => {
                return Ok(PresentOutcome::SurfaceOutOfDate);
            }
            Err(source) => return Err(renderer_error("present application frame", source)),
        };
        if acquire_status == PresentStatus::Suboptimal
            || present_status == PresentStatus::Suboptimal
        {
            Ok(PresentOutcome::PresentedSuboptimally)
        } else {
            Ok(PresentOutcome::Presented)
        }
    }

    fn reconfigure(
        &mut self,
        surface_id: SurfaceId,
        extent: Extent2D,
        adapter: &Adapter,
        device: &Device,
    ) -> Result<(), SurfacePresenterError> {
        let configuration = choose_configuration(adapter, &self.surface, device, extent)?;
        let swapchain = device
            .create_swapchain(
                &self.surface,
                &SwapchainDescriptor {
                    label: Some(&self.label),
                    configuration,
                    old_swapchain: Some(&self.swapchain),
                },
            )
            .map_err(|source| renderer_error("replace application swapchain", source))?;
        self.present_complete =
            create_present_semaphores(device, surface_id, swapchain.image_count())?;
        self.initialized_images = vec![false; swapchain.image_count()];
        self.swapchain = swapchain;
        for slot in &mut self.frame_slots {
            slot.in_flight = None;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentOutcome {
    Presented,
    PresentedSuboptimally,
    SurfaceOutOfDate,
}

fn choose_configuration(
    adapter: &Adapter,
    surface: &Surface,
    device: &Device,
    requested_extent: Extent2D,
) -> Result<SurfaceConfiguration, SurfacePresenterError> {
    let capabilities = adapter
        .surface_capabilities(surface)
        .map_err(|source| renderer_error("query application surface capabilities", source))?;
    let formats = [
        SurfaceFormat::new(TextureFormat::Bgra8Unorm, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Bgra8Srgb, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Rgba8Unorm, ColorSpace::SrgbNonlinear),
    ];
    let configuration = SurfaceConfiguration::choose(
        &capabilities,
        device.features(),
        SurfaceConfigurationRequest {
            width: requested_extent.width,
            height: requested_extent.height,
            usage: TextureUsages::COLOR_ATTACHMENT,
            formats: &formats,
            present_modes: &[PresentMode::FifoLatestReady],
            composite_alpha: &[CompositeAlphaMode::PreMultiplied],
            pre_transforms: &[capabilities.current_transform],
            desired_image_count: capabilities
                .min_image_count
                .saturating_add(1)
                .min(capabilities.max_image_count.unwrap_or(u32::MAX)),
        },
    )
    .map_err(|source| renderer_error("choose application surface configuration", source))?;
    if configuration.extent != requested_extent {
        return Err(SurfacePresenterError::SurfaceExtentMismatch {
            requested: requested_extent,
            selected: configuration.extent,
        });
    }
    Ok(configuration)
}

fn create_frame_slots(
    device: &Device,
    surface: SurfaceId,
) -> Result<Vec<FrameSlot>, SurfacePresenterError> {
    (0..FRAME_SLOTS)
        .map(|index| {
            device
                .create_binary_semaphore(&BinarySemaphoreDescriptor {
                    label: Some(format!("application-{surface:?}-acquire-{index}")),
                })
                .map(|acquire| FrameSlot {
                    acquire,
                    in_flight: None,
                })
                .map_err(|source| renderer_error("create application acquire semaphore", source))
        })
        .collect()
}

fn create_present_semaphores(
    device: &Device,
    surface: SurfaceId,
    image_count: usize,
) -> Result<Vec<BinarySemaphore>, SurfacePresenterError> {
    (0..image_count)
        .map(|index| {
            device
                .create_binary_semaphore(&BinarySemaphoreDescriptor {
                    label: Some(format!("application-{surface:?}-present-{index}")),
                })
                .map_err(|source| renderer_error("create application present semaphore", source))
        })
        .collect()
}

fn compile_graph(
    queue_family: u32,
    initialized: bool,
) -> Result<CompiledGraph, SurfacePresenterError> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        SURFACE_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            if initialized {
                RenderGraphImageState::Present
            } else {
                RenderGraphImageState::Undefined
            },
            queue_family,
        ),
    );
    graph.add_pass(RenderPass {
        id: DRAW_PASS,
        label: "application-draw".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: SURFACE_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::color_attachment_write(queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: PRESENT_PASS,
        label: "application-present".into(),
        depends_on: vec![DRAW_PASS],
        resources: vec![ResourceUse {
            resource: SURFACE_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::present(queue_family),
        }],
    });
    graph.compile().map_err(SurfacePresenterError::CompileGraph)
}

fn require_extent(surface: SurfaceId, extent: Extent2D) -> Result<(), SurfacePresenterError> {
    if extent.is_empty() {
        Err(SurfacePresenterError::EmptySurfaceExtent(surface))
    } else {
        Ok(())
    }
}

fn renderer_error(
    operation: &'static str,
    source: vulkan_renderer::Error,
) -> SurfacePresenterError {
    SurfacePresenterError::Renderer { operation, source }
}

#[derive(Debug, thiserror::Error)]
pub enum SurfacePresenterError {
    #[error("surface {0:?} has no non-empty physical extent")]
    EmptySurfaceExtent(SurfaceId),
    #[error("surface presenter does not own {0:?}")]
    UnknownSurface(SurfaceId),
    #[error("surface extent changed from {requested:?} to {selected:?}")]
    SurfaceExtentMismatch {
        requested: Extent2D,
        selected: Extent2D,
    },
    #[error("surface {0:?} remained out of date after swapchain replacement")]
    RepeatedSurfaceOutOfDate(SurfaceId),
    #[error("{operation}: {source}")]
    Renderer {
        operation: &'static str,
        #[source]
        source: vulkan_renderer::Error,
    },
    #[error("compile application presentation graph: {0}")]
    CompileGraph(#[source] RenderGraphError),
    #[error("resolve application presentation barriers: {0}")]
    GraphSync(#[from] vulkan_renderer::RenderGraphSyncError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rects_are_value_only() {
        let frame = SurfaceFrame {
            clear: [0.1, 0.2, 0.3, 1.0],
            rectangles: &[ColorRect {
                rect: Rect2D::new(1, 2, 3, 4),
                color: [1.0; 4],
            }],
        };
        assert_eq!(frame.rectangles[0].rect.extent.width, 3);
    }
}
