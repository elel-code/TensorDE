use std::{collections::BTreeMap, sync::Arc};

use vulkan_renderer::{
    AccessKind, Adapter, BackendProfile, BinarySemaphore, BinarySemaphoreDescriptor,
    ColorAttachment, ColorSpace, CommandEncoderDescriptor, CompiledGraph, CompositeAlphaMode,
    Device, DeviceDescriptor, Extent2D, FrameToken, Instance, InstanceDescriptor, LoadOp, PassId,
    PipelineStages, PowerPreference, PresentMode, PresentStatus, Queue, Rect2D, RenderGraph,
    RenderGraphError, RenderGraphImageState, RenderGraphSyncError, RenderPass, RenderingDescriptor,
    RequestAdapterOptions, ResolveMode, ResourceId, ResourceKind, ResourceState, ResourceUse,
    StoreOp, Surface, SurfaceCapabilities, SurfaceConfiguration, SurfaceConfigurationRequest,
    SurfaceFormat, Swapchain, SwapchainDescriptor, TextureFormat, TextureLayout, TextureUsages,
};
use wayland_client_runtime::{SurfaceHandle, SurfaceId};

use crate::control_center_scene::{ControlCenterInteraction, ControlCenterScene};
use crate::media_osd_scene::{MediaOsdInteraction, MediaOsdScene};
use crate::notification_scene::{NotificationInteraction, NotificationScene};
use crate::overview_scene::{OverviewInteraction, OverviewScene};
use crate::{PanelAppletStore, PanelScene, ShellComponent, SurfaceKey, panel::PanelInteraction};

const FRAME_SLOTS: usize = 3;
const SURFACE_IMAGE: ResourceId = ResourceId(1);
const DRAW_PASS: PassId = PassId(1);
const PRESENT_PASS: PassId = PassId(2);

/// One Vulkan ownership root shared by every Tensor Shell layer surface.
pub(crate) struct ShellPresenter {
    _instance: Instance,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    initial_graph: CompiledGraph,
    retained_graph: CompiledGraph,
    surfaces: BTreeMap<SurfaceId, PresentedSurface>,
}

struct PresentedSurface {
    key: SurfaceKey,
    surface: Surface,
    swapchain: Swapchain,
    frame_slots: Vec<FrameSlot>,
    present_complete: Vec<BinarySemaphore>,
    initialized_images: Vec<bool>,
    next_frame_slot: usize,
    panel_scene: Option<PanelScene>,
    panel_interaction: PanelInteraction,
    panel_applet_revision: u64,
    overview_scene: Option<OverviewScene>,
    overview_interaction: OverviewInteraction,
    notification_scene: Option<NotificationScene>,
    notification_interaction: NotificationInteraction,
    media_osd_scene: Option<MediaOsdScene>,
    media_osd_interaction: MediaOsdInteraction,
    control_center_scene: Option<ControlCenterScene>,
    control_center_interaction: ControlCenterInteraction,
    draws: Vec<crate::panel::PanelDraw>,
}

struct FrameSlot {
    acquire: BinarySemaphore,
    in_flight: Option<FrameToken>,
}

pub(crate) struct RetainedSceneInput<'a, Scene, Interaction> {
    scene: Option<&'a Scene>,
    interaction: Interaction,
}

impl<'a, Scene, Interaction> RetainedSceneInput<'a, Scene, Interaction> {
    pub(crate) const fn new(scene: Option<&'a Scene>, interaction: Interaction) -> Self {
        Self { scene, interaction }
    }
}

pub(crate) struct SurfacePresentation<'a> {
    panel: RetainedSceneInput<'a, PanelScene, PanelInteraction>,
    applets: &'a PanelAppletStore,
    overview: RetainedSceneInput<'a, OverviewScene, OverviewInteraction>,
    notification: RetainedSceneInput<'a, NotificationScene, NotificationInteraction>,
    media_osd: RetainedSceneInput<'a, MediaOsdScene, MediaOsdInteraction>,
    control_center: RetainedSceneInput<'a, ControlCenterScene, ControlCenterInteraction>,
}

impl<'a> SurfacePresentation<'a> {
    pub(crate) const fn new(
        panel: RetainedSceneInput<'a, PanelScene, PanelInteraction>,
        applets: &'a PanelAppletStore,
        overview: RetainedSceneInput<'a, OverviewScene, OverviewInteraction>,
        notification: RetainedSceneInput<'a, NotificationScene, NotificationInteraction>,
        media_osd: RetainedSceneInput<'a, MediaOsdScene, MediaOsdInteraction>,
        control_center: RetainedSceneInput<'a, ControlCenterScene, ControlCenterInteraction>,
    ) -> Self {
        Self {
            panel,
            applets,
            overview,
            notification,
            media_osd,
            control_center,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PresentOutcome {
    Presented,
    PresentedSuboptimally,
    SurfaceOutOfDate,
}

impl ShellPresenter {
    pub(crate) fn new(
        surface_id: SurfaceId,
        key: SurfaceKey,
        host: Arc<SurfaceHandle>,
        extent: Extent2D,
    ) -> Result<Self, ShellPresentError> {
        require_extent(surface_id, extent)?;
        let descriptor = InstanceDescriptor::for_window(BackendProfile::Roadmap2026, host.as_ref())
            .map_err(|source| gpu_error("build Tensor Shell Vulkan instance descriptor", source))?;
        let instance = Instance::new(descriptor)
            .map_err(|source| gpu_error("create Tensor Shell Vulkan instance", source))?;
        let surface = instance
            .create_surface(host)
            .map_err(|source| gpu_error("create initial Tensor Shell Vulkan surface", source))?;
        let adapter = instance
            .request_adapter(RequestAdapterOptions {
                power_preference: PowerPreference::Discrete,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                selector: None,
            })
            .map_err(|source| gpu_error("select Tensor Shell presentation adapter", source))?;
        let (device, queue) = adapter
            .request_device(DeviceDescriptor {
                label: Some("tensor-shell-vulkan-device".into()),
                ..DeviceDescriptor::default()
            })
            .map_err(|source| gpu_error("create Tensor Shell Vulkan device", source))?;
        let queue_family = device.device_info().queues.graphics;
        let initial_graph = compile_present_graph(queue_family, false)?;
        let retained_graph = compile_present_graph(queue_family, true)?;
        let presented = PresentedSurface::new(surface_id, key, surface, extent, &adapter, &device)?;
        Ok(Self {
            _instance: instance,
            adapter,
            device,
            queue,
            initial_graph,
            retained_graph,
            surfaces: BTreeMap::from([(surface_id, presented)]),
        })
    }

    pub(crate) fn ensure_surface(
        &mut self,
        surface_id: SurfaceId,
        key: SurfaceKey,
        host: Arc<SurfaceHandle>,
        extent: Extent2D,
    ) -> Result<(), ShellPresentError> {
        require_extent(surface_id, extent)?;
        if let Some(surface) = self.surfaces.get(&surface_id) {
            if surface.key != key {
                return Err(ShellPresentError::SurfaceIdentityChanged {
                    surface: surface_id,
                    previous: surface.key,
                    current: key,
                });
            }
            if surface.swapchain.configuration().extent == extent {
                return Ok(());
            }
            return self.reconfigure(surface_id, extent);
        }
        let surface = self
            ._instance
            .create_surface(host)
            .map_err(|source| gpu_error("create Tensor Shell Vulkan surface", source))?;
        let presented = PresentedSurface::new(
            surface_id,
            key,
            surface,
            extent,
            &self.adapter,
            &self.device,
        )?;
        self.surfaces.insert(surface_id, presented);
        Ok(())
    }

    pub(crate) fn present(
        &mut self,
        surface_id: SurfaceId,
        extent: Extent2D,
        presentation: SurfacePresentation<'_>,
    ) -> Result<(), ShellPresentError> {
        for attempt in 0..2 {
            let surface = self
                .surfaces
                .get_mut(&surface_id)
                .ok_or(ShellPresentError::UnknownSurface(surface_id))?;
            let outcome = surface.present(
                &self.device,
                &self.queue,
                &self.initial_graph,
                &self.retained_graph,
                &presentation,
            )?;
            match outcome {
                PresentOutcome::Presented => return Ok(()),
                PresentOutcome::PresentedSuboptimally => {
                    self.reconfigure(surface_id, extent)?;
                    return Ok(());
                }
                PresentOutcome::SurfaceOutOfDate if attempt == 0 => {
                    self.reconfigure(surface_id, extent)?;
                }
                PresentOutcome::SurfaceOutOfDate => {
                    return Err(ShellPresentError::RepeatedSurfaceOutOfDate(surface_id));
                }
            }
        }
        unreachable!("the bounded present retry loop always returns")
    }

    pub(crate) fn remove_surface(
        &mut self,
        surface_id: SurfaceId,
    ) -> Result<(), ShellPresentError> {
        if self.surfaces.contains_key(&surface_id) {
            self.queue.wait_idle().map_err(|source| {
                gpu_error("idle Tensor Shell queue before surface removal", source)
            })?;
            self.surfaces.remove(&surface_id);
        }
        Ok(())
    }

    fn reconfigure(
        &mut self,
        surface_id: SurfaceId,
        extent: Extent2D,
    ) -> Result<(), ShellPresentError> {
        require_extent(surface_id, extent)?;
        self.queue.wait_idle().map_err(|source| {
            gpu_error(
                "idle Tensor Shell queue before swapchain replacement",
                source,
            )
        })?;
        let surface = self
            .surfaces
            .get_mut(&surface_id)
            .ok_or(ShellPresentError::UnknownSurface(surface_id))?;
        surface.reconfigure(surface_id, extent, &self.adapter, &self.device)
    }
}

impl PresentedSurface {
    fn new(
        surface_id: SurfaceId,
        key: SurfaceKey,
        surface: Surface,
        extent: Extent2D,
        adapter: &Adapter,
        device: &Device,
    ) -> Result<Self, ShellPresentError> {
        let configuration = choose_surface_configuration(adapter, &surface, device, extent)?;
        let swapchain = device
            .create_swapchain(
                &surface,
                &SwapchainDescriptor {
                    label: Some("tensor-shell-swapchain"),
                    configuration,
                    old_swapchain: None,
                },
            )
            .map_err(|source| gpu_error("create Tensor Shell swapchain", source))?;
        let frame_slots = create_frame_slots(device, surface_id)?;
        let present_complete =
            create_present_semaphores(device, surface_id, swapchain.image_count())?;
        let initialized_images = vec![false; swapchain.image_count()];
        Ok(Self {
            key,
            surface,
            swapchain,
            frame_slots,
            present_complete,
            initialized_images,
            next_frame_slot: 0,
            panel_scene: None,
            panel_interaction: PanelInteraction::default(),
            panel_applet_revision: 0,
            overview_scene: None,
            overview_interaction: OverviewInteraction::default(),
            notification_scene: None,
            notification_interaction: NotificationInteraction::default(),
            media_osd_scene: None,
            media_osd_interaction: MediaOsdInteraction::default(),
            control_center_scene: None,
            control_center_interaction: ControlCenterInteraction::default(),
            draws: Vec::with_capacity(64),
        })
    }

    fn present(
        &mut self,
        device: &Device,
        queue: &Queue,
        initial_graph: &CompiledGraph,
        retained_graph: &CompiledGraph,
        presentation: &SurfacePresentation<'_>,
    ) -> Result<PresentOutcome, ShellPresentError> {
        self.update_draws(presentation);
        let slot_index = self.next_frame_slot;
        self.next_frame_slot = (self.next_frame_slot + 1) % self.frame_slots.len();
        if let Some(frame) = self.frame_slots[slot_index].in_flight.take() {
            queue
                .wait_for(frame, u64::MAX)
                .map_err(|source| gpu_error("wait for Tensor Shell frame slot", source))?;
        }
        let acquired = match unsafe {
            self.swapchain
                .acquire_next_image(u64::MAX, &self.frame_slots[slot_index].acquire)
        } {
            Ok(acquired) => acquired,
            Err(error) if error.is_surface_out_of_date() => {
                return Ok(PresentOutcome::SurfaceOutOfDate);
            }
            Err(source) => return Err(gpu_error("acquire Tensor Shell swapchain image", source)),
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
                label: Some("tensor-shell-clear-frame".into()),
            })
            .map_err(|source| gpu_error("create Tensor Shell command encoder", source))?;
        unsafe { encoder.pipeline_barrier(&before_draw) };
        let attachments = [Some(ColorAttachment {
            view: acquired.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear(component_clear_color(self.key.component)),
            store_op: StoreOp::Store,
        })];
        let rendering = RenderingDescriptor {
            label: Some("tensor-shell-chrome-clear"),
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
                .map_err(|source| gpu_error("begin Tensor Shell dynamic rendering", source))?;
            for draw in &self.draws {
                rendering
                    .clear_color_attachment(0, draw.color, &[draw.rect])
                    .map_err(|source| {
                        gpu_error("draw retained Tensor Shell surface item", source)
                    })?;
            }
            rendering.end();
            encoder.pipeline_barrier(&before_present);
        }
        let command = encoder
            .finish()
            .map_err(|source| gpu_error("finish Tensor Shell command buffer", source))?;
        let acquire_wait = self.frame_slots[slot_index]
            .acquire
            .wait(PipelineStages::COLOR_ATTACHMENT_OUTPUT)
            .map_err(|source| gpu_error("build Tensor Shell acquire wait", source))?;
        let present_complete = &self.present_complete[image_index];
        let frame = unsafe {
            queue.submit_with_binary_signals([command], &[acquire_wait], &[present_complete])
        }
        .map_err(|source| gpu_error("submit Tensor Shell frame", source))?;
        self.frame_slots[slot_index].in_flight = Some(frame);
        self.initialized_images[image_index] = true;
        let acquire_status = acquired.status();
        let present_status = match unsafe { acquired.present(queue, &[present_complete]) } {
            Ok(status) => status,
            Err(error) if error.is_surface_out_of_date() => {
                return Ok(PresentOutcome::SurfaceOutOfDate);
            }
            Err(source) => return Err(gpu_error("present Tensor Shell frame", source)),
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
    ) -> Result<(), ShellPresentError> {
        let configuration = choose_surface_configuration(adapter, &self.surface, device, extent)?;
        let replacement = device
            .create_swapchain(
                &self.surface,
                &SwapchainDescriptor {
                    label: Some("tensor-shell-swapchain"),
                    configuration,
                    old_swapchain: Some(&self.swapchain),
                },
            )
            .map_err(|source| gpu_error("replace Tensor Shell swapchain", source))?;
        let present_complete =
            create_present_semaphores(device, surface_id, replacement.image_count())?;
        self.swapchain = replacement;
        self.present_complete = present_complete;
        self.initialized_images = vec![false; self.swapchain.image_count()];
        self.panel_scene = None;
        self.panel_applet_revision = 0;
        self.overview_scene = None;
        self.notification_scene = None;
        self.media_osd_scene = None;
        self.control_center_scene = None;
        self.draws.clear();
        for slot in &mut self.frame_slots {
            slot.in_flight = None;
        }
        Ok(())
    }

    fn update_draws(&mut self, presentation: &SurfacePresentation<'_>) {
        if self.panel_scene.as_ref() == presentation.panel.scene
            && self.panel_interaction == presentation.panel.interaction
            && self.panel_applet_revision == presentation.applets.revision()
            && self.overview_scene.as_ref() == presentation.overview.scene
            && self.overview_interaction == presentation.overview.interaction
            && self.notification_scene.as_ref() == presentation.notification.scene
            && self.notification_interaction == presentation.notification.interaction
            && self.media_osd_scene.as_ref() == presentation.media_osd.scene
            && self.media_osd_interaction == presentation.media_osd.interaction
            && self.control_center_scene.as_ref() == presentation.control_center.scene
            && self.control_center_interaction == presentation.control_center.interaction
        {
            return;
        }
        self.panel_scene = presentation.panel.scene.cloned();
        self.panel_interaction = presentation.panel.interaction;
        self.panel_applet_revision = presentation.applets.revision();
        self.overview_scene = presentation.overview.scene.cloned();
        self.overview_interaction = presentation.overview.interaction;
        self.notification_scene = presentation.notification.scene.cloned();
        self.notification_interaction = presentation.notification.interaction;
        self.media_osd_scene = presentation.media_osd.scene.cloned();
        self.media_osd_interaction = presentation.media_osd.interaction;
        self.control_center_scene = presentation.control_center.scene.cloned();
        self.control_center_interaction = presentation.control_center.interaction;
        self.draws = presentation
            .panel
            .scene
            .map(|scene| {
                scene.physical_draws(
                    self.swapchain.configuration().extent,
                    presentation.panel.interaction,
                    presentation.applets,
                )
            })
            .or_else(|| {
                presentation.overview.scene.map(|scene| {
                    scene.physical_draws(
                        self.swapchain.configuration().extent,
                        presentation.overview.interaction,
                    )
                })
            })
            .or_else(|| {
                presentation.notification.scene.map(|scene| {
                    scene.physical_draws(
                        self.swapchain.configuration().extent,
                        presentation.notification.interaction,
                    )
                })
            })
            .or_else(|| {
                presentation.media_osd.scene.map(|scene| {
                    scene.physical_draws(
                        self.swapchain.configuration().extent,
                        presentation.media_osd.interaction,
                    )
                })
            })
            .or_else(|| {
                presentation.control_center.scene.map(|scene| {
                    scene.physical_draws(
                        self.swapchain.configuration().extent,
                        presentation.control_center.interaction,
                    )
                })
            })
            .unwrap_or_default();
    }
}

fn choose_surface_configuration(
    adapter: &Adapter,
    surface: &Surface,
    device: &Device,
    requested_extent: Extent2D,
) -> Result<SurfaceConfiguration, ShellPresentError> {
    let capabilities = adapter
        .surface_capabilities(surface)
        .map_err(|source| gpu_error("query Tensor Shell surface capabilities", source))?;
    let configuration = choose_configuration(&capabilities, device.features(), requested_extent)
        .map_err(|source| gpu_error("choose Tensor Shell surface configuration", source))?;
    if configuration.extent != requested_extent {
        return Err(ShellPresentError::SurfaceExtentMismatch {
            requested: requested_extent,
            selected: configuration.extent,
        });
    }
    Ok(configuration)
}

fn choose_configuration(
    capabilities: &SurfaceCapabilities,
    features: vulkan_renderer::Features,
    requested_extent: Extent2D,
) -> vulkan_renderer::Result<SurfaceConfiguration> {
    let formats = [
        SurfaceFormat::new(TextureFormat::Bgra8Unorm, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Bgra8Srgb, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Rgba8Unorm, ColorSpace::SrgbNonlinear),
    ];
    SurfaceConfiguration::choose(
        capabilities,
        features,
        SurfaceConfigurationRequest {
            width: requested_extent.width,
            height: requested_extent.height,
            usage: TextureUsages::COLOR_ATTACHMENT,
            formats: &formats,
            present_modes: &[PresentMode::FifoLatestReady],
            composite_alpha: &[CompositeAlphaMode::PreMultiplied],
            pre_transforms: &[capabilities.current_transform],
            desired_image_count: preferred_image_count(
                capabilities.min_image_count,
                capabilities.max_image_count,
            ),
        },
    )
}

fn create_frame_slots(
    device: &Device,
    surface: SurfaceId,
) -> Result<Vec<FrameSlot>, ShellPresentError> {
    (0..FRAME_SLOTS)
        .map(|index| {
            device
                .create_binary_semaphore(&BinarySemaphoreDescriptor {
                    label: Some(format!("tensor-shell-{surface:?}-acquire-{index}")),
                })
                .map(|acquire| FrameSlot {
                    acquire,
                    in_flight: None,
                })
                .map_err(|source| gpu_error("create Tensor Shell acquire semaphore", source))
        })
        .collect()
}

fn create_present_semaphores(
    device: &Device,
    surface: SurfaceId,
    image_count: usize,
) -> Result<Vec<BinarySemaphore>, ShellPresentError> {
    (0..image_count)
        .map(|index| {
            device
                .create_binary_semaphore(&BinarySemaphoreDescriptor {
                    label: Some(format!("tensor-shell-{surface:?}-present-{index}")),
                })
                .map_err(|source| gpu_error("create Tensor Shell present semaphore", source))
        })
        .collect()
}

fn compile_present_graph(
    queue_family: u32,
    initialized: bool,
) -> Result<CompiledGraph, ShellPresentError> {
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
        label: "tensor-shell-clear".into(),
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
        label: "tensor-shell-present".into(),
        depends_on: vec![DRAW_PASS],
        resources: vec![ResourceUse {
            resource: SURFACE_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::present(queue_family),
        }],
    });
    graph.compile().map_err(ShellPresentError::CompileGraph)
}

fn preferred_image_count(minimum: u32, maximum: Option<u32>) -> u32 {
    match maximum {
        Some(maximum) => minimum.saturating_add(1).min(maximum),
        None => minimum.saturating_add(1),
    }
}

const fn component_clear_color(component: ShellComponent) -> [f32; 4] {
    match component {
        ShellComponent::Panel => [0.055, 0.063, 0.075, 0.98],
        ShellComponent::NotificationCenter => [0.065, 0.073, 0.086, 0.98],
        ShellComponent::NotificationPopups => [0.075, 0.082, 0.095, 0.96],
        ShellComponent::Osd => [0.08, 0.087, 0.1, 0.96],
        ShellComponent::ControlCenter => [0.065, 0.073, 0.086, 0.98],
        ShellComponent::Overview => [0.025, 0.03, 0.04, 0.94],
        ShellComponent::LockScreen => [0.018, 0.022, 0.03, 1.0],
    }
}

fn require_extent(surface: SurfaceId, extent: Extent2D) -> Result<(), ShellPresentError> {
    if extent.is_empty() {
        Err(ShellPresentError::EmptySurfaceExtent(surface))
    } else {
        Ok(())
    }
}

fn gpu_error(operation: &'static str, source: vulkan_renderer::Error) -> ShellPresentError {
    ShellPresentError::Renderer { operation, source }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellPresentError {
    #[error("Tensor Shell surface {0:?} has no non-empty physical buffer extent")]
    EmptySurfaceExtent(SurfaceId),
    #[error("Tensor Shell presenter does not own surface {0:?}")]
    UnknownSurface(SurfaceId),
    #[error("Tensor Shell surface {surface:?} changed identity from {previous:?} to {current:?}")]
    SurfaceIdentityChanged {
        surface: SurfaceId,
        previous: SurfaceKey,
        current: SurfaceKey,
    },
    #[error(
        "Tensor Shell requested surface extent {requested:?}, but Vulkan selected {selected:?}"
    )]
    SurfaceExtentMismatch {
        requested: Extent2D,
        selected: Extent2D,
    },
    #[error("Tensor Shell surface {0:?} remained out of date after swapchain replacement")]
    RepeatedSurfaceOutOfDate(SurfaceId),
    #[error("{operation}: {source}")]
    Renderer {
        operation: &'static str,
        #[source]
        source: vulkan_renderer::Error,
    },
    #[error("compile retained Tensor Shell presentation graph: {0}")]
    CompileGraph(#[source] RenderGraphError),
    #[error("resolve retained Tensor Shell presentation barriers: {0}")]
    ResolveGraph(#[from] RenderGraphSyncError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_graph_has_cold_and_retained_layout_transitions() {
        for initialized in [false, true] {
            let graph = compile_present_graph(3, initialized).unwrap();
            assert_eq!(graph.ordered_passes, vec![DRAW_PASS, PRESENT_PASS]);
            assert_eq!(graph.barriers.len(), 2);
            assert_eq!(graph.barriers[0].before, None);
            assert_eq!(graph.barriers[0].after, DRAW_PASS);
            assert_eq!(graph.barriers[1].before, Some(DRAW_PASS));
            assert_eq!(graph.barriers[1].after, PRESENT_PASS);
        }
    }

    #[test]
    fn image_count_retains_one_frame_when_capabilities_allow_it() {
        assert_eq!(preferred_image_count(2, None), 3);
        assert_eq!(preferred_image_count(2, Some(4)), 3);
        assert_eq!(preferred_image_count(2, Some(2)), 2);
        assert_eq!(preferred_image_count(u32::MAX, None), u32::MAX);
    }

    #[test]
    fn every_shell_component_has_visible_nonzero_chrome() {
        for component in [
            ShellComponent::Panel,
            ShellComponent::NotificationCenter,
            ShellComponent::NotificationPopups,
            ShellComponent::Osd,
            ShellComponent::ControlCenter,
            ShellComponent::Overview,
            ShellComponent::LockScreen,
        ] {
            let color = component_clear_color(component);
            assert!(color[3] > 0.0);
            assert!(color[..3].iter().any(|channel| *channel > 0.0));
        }
    }
}
