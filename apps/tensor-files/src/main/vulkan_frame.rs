use vulkan_renderer::{
    AccessKind, BarrierBatch, Buffer, CompiledGraph, Extent2D, FrameTargetPreference, PassId,
    PresentationPathDescriptor, PresentationPathPlan, PresentationRequirements, RenderGraph,
    RenderGraphImageState, RenderPass, ResourceBinding, ResourceId, ResourceKind, ResourceState,
    ResourceUse, SurfaceAcquireStrategy, TerminalAlphaMode, TerminalCompositeDescriptor,
    TerminalSampling, TextureFormat, TextureUsages,
};

const RENDER_PASS: PassId = PassId(2);
const PRESENT_PASS: PassId = PassId(3);
const SURFACE_IMAGE: ResourceId = ResourceId(1);
const FIRST_VERTEX_BUFFER: u64 = 2;
const FIRST_UPLOAD_PASS: u32 = 10;

pub(crate) const FRAME_VERTEX_STREAM_COUNT: usize = 5;
const FRAME_GRAPH_VARIANTS: usize = 1 << (FRAME_VERTEX_STREAM_COUNT + 1);

#[derive(Clone, Copy)]
pub(crate) struct FrameVertexBuffer<'a> {
    pub(crate) buffer: &'a Buffer,
    pub(crate) uploaded: bool,
}

pub(crate) type FrameVertexBuffers<'a> = [FrameVertexBuffer<'a>; FRAME_VERTEX_STREAM_COUNT];

fn frame_graph_cache_key(image_initialized: bool, upload_mask: u8) -> usize {
    usize::from(upload_mask & ((1 << FRAME_VERTEX_STREAM_COUNT) - 1))
        | (usize::from(image_initialized) << FRAME_VERTEX_STREAM_COUNT)
}

fn frame_vertex_upload_mask(vertex_buffers: &FrameVertexBuffers<'_>) -> u8 {
    vertex_buffers
        .iter()
        .enumerate()
        .fold(0, |mask, (index, buffer)| {
            mask | (u8::from(buffer.uploaded) << index)
        })
}

/// Precompiled synchronization variants for one fixed set of retained streams.
///
/// The image initialization bit and five upload bits are the only frame facts
/// that affect this graph. Keeping all variants resident moves graph parsing,
/// dependency ordering, and barrier allocation out of the present path.
pub(crate) struct FrameBarrierCache {
    graphs: Vec<CompiledGraph>,
    before_render: BarrierBatch,
    before_present: BarrierBatch,
}

impl FrameBarrierCache {
    pub(crate) fn new(queue_family: u32) -> Result<Self, String> {
        let graphs = (0..FRAME_GRAPH_VARIANTS)
            .map(|key| {
                compile_frame_graph(
                    key & ((1 << FRAME_VERTEX_STREAM_COUNT) - 1),
                    key & (1 << FRAME_VERTEX_STREAM_COUNT) != 0,
                    queue_family,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            graphs,
            before_render: BarrierBatch::with_capacity(FRAME_VERTEX_STREAM_COUNT, 1),
            before_present: BarrierBatch::with_capacity(0, 1),
        })
    }

    pub(crate) fn resolve(
        &mut self,
        surface_binding: ResourceBinding,
        image_initialized: bool,
        vertex_buffers: &FrameVertexBuffers<'_>,
    ) -> Result<(&BarrierBatch, &BarrierBatch), String> {
        let upload_mask = frame_vertex_upload_mask(vertex_buffers);
        let key = frame_graph_cache_key(image_initialized, upload_mask);
        let graph = &self.graphs[key];
        let mut bindings = [(SURFACE_IMAGE, surface_binding); FRAME_VERTEX_STREAM_COUNT + 1];
        for (index, vertex_buffer) in vertex_buffers.iter().enumerate() {
            bindings[index + 1] = (
                ResourceId(FIRST_VERTEX_BUFFER + index as u64),
                ResourceBinding::whole_buffer(vertex_buffer.buffer),
            );
        }
        graph
            .fill_barrier_batch_before_from_slice(RENDER_PASS, &bindings, &mut self.before_render)
            .map_err(|error| format!("resolve Vulkan render barriers: {error}"))?;
        graph
            .fill_barrier_batch_before_from_slice(PRESENT_PASS, &bindings, &mut self.before_present)
            .map_err(|error| format!("resolve Vulkan present barrier: {error}"))?;
        Ok((&self.before_render, &self.before_present))
    }
}

/// Product-level frame facts. These are semantics, not a requested topology.
///
/// Analytic chrome/shadows and already-retained icon/preview textures do not
/// read Tensor Files's scene color, so the ordinary main/dialog frame keeps the
/// default facts and compiles to one direct dynamic-rendering pass. A future
/// background filter contributes a local scene-color dependency. History,
/// whole-frame consumers, async compute, or terminal transforms remain global
/// facts because they can require a retained terminal image.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TensorFilesFrameSemantics {
    pub(crate) local_scene_color_dependencies: u32,
    pub(crate) has_history: bool,
    pub(crate) has_external_consumer: bool,
    pub(crate) uses_async_compute: bool,
    pub(crate) requires_terminal_transform: bool,
}

impl TensorFilesFrameSemantics {
    pub(crate) const fn direct_ui() -> Self {
        Self {
            local_scene_color_dependencies: 0,
            has_history: false,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectFramePlanKey {
    extent: Extent2D,
    format: TextureFormat,
    surface_usage: TextureUsages,
    semantics: TensorFilesFrameSemantics,
}

/// Retains the successful direct-surface validation for stable frame facts.
#[derive(Default)]
pub(crate) struct DirectFramePlanCache {
    validated: Option<DirectFramePlanKey>,
}

impl DirectFramePlanCache {
    pub(crate) fn require(
        &mut self,
        extent: Extent2D,
        format: TextureFormat,
        surface_usage: TextureUsages,
        semantics: TensorFilesFrameSemantics,
    ) -> Result<bool, String> {
        let key = DirectFramePlanKey {
            extent,
            format,
            surface_usage,
            semantics,
        };
        if self.validated == Some(key) {
            return Ok(false);
        }
        require_direct_frame_plan(extent, format, surface_usage, semantics)?;
        self.validated = Some(key);
        Ok(true)
    }
}

/// Product-side composition selected from Tensor Files semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TensorFilesCompositionPath {
    DirectSinglePass,
    DirectWithLocalPasses { dependencies: u32 },
    OffscreenMultiPass,
}

/// Keeps product-local pass selection separate from the shared presentation
/// target decision. Local passes are legal on a direct surface only when its
/// selected swapchain usage permits a bounded source copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TensorFilesFramePlan {
    pub(crate) presentation: PresentationPathPlan,
    pub(crate) composition: TensorFilesCompositionPath,
}

pub(crate) fn compile_frame_plan(
    extent: Extent2D,
    format: TextureFormat,
    surface_usage: TextureUsages,
    semantics: TensorFilesFrameSemantics,
) -> Result<TensorFilesFramePlan, String> {
    compile_frame_plan_with_preference(
        extent,
        format,
        surface_usage,
        semantics,
        FrameTargetPreference::Automatic,
    )
}

pub(crate) fn compile_frame_plan_with_preference(
    extent: Extent2D,
    format: TextureFormat,
    surface_usage: TextureUsages,
    semantics: TensorFilesFrameSemantics,
    target: FrameTargetPreference,
) -> Result<TensorFilesFramePlan, String> {
    let local_dependencies_require_offscreen = semantics.local_scene_color_dependencies > 0
        && !surface_usage.contains(TextureUsages::COPY_SOURCE);
    let presentation = PresentationPathPlan::compile(
        PresentationPathDescriptor {
            target,
            acquire: SurfaceAcquireStrategy::BeforeFrame,
            terminal: TerminalCompositeDescriptor {
                sampling: TerminalSampling::Nearest,
                alpha: TerminalAlphaMode::Preserve,
            },
        },
        PresentationRequirements {
            surface_extent: extent,
            target_extent: extent,
            surface_format: format,
            target_format: format,
            frame_slots: 3,
            // A surface-local copy/filter/composite sequence does not make the
            // terminal image multi-pass. Only lower it as a global blocker
            // when this surface cannot be a transfer source.
            physical_pass_count: 1 + u32::from(local_dependencies_require_offscreen),
            sampled_after_write: local_dependencies_require_offscreen,
            has_history: semantics.has_history,
            has_external_consumer: semantics.has_external_consumer,
            uses_async_compute: semantics.uses_async_compute,
            requires_terminal_transform: semantics.requires_terminal_transform,
        },
    )
    .map_err(|error| format!("compile Tensor Files presentation path: {error}"))?;
    let composition = match presentation.target {
        vulkan_renderer::PresentationTarget::Offscreen => {
            TensorFilesCompositionPath::OffscreenMultiPass
        }
        vulkan_renderer::PresentationTarget::DirectSurface
            if semantics.local_scene_color_dependencies > 0 =>
        {
            TensorFilesCompositionPath::DirectWithLocalPasses {
                dependencies: semantics.local_scene_color_dependencies,
            }
        }
        vulkan_renderer::PresentationTarget::DirectSurface => {
            TensorFilesCompositionPath::DirectSinglePass
        }
    };
    Ok(TensorFilesFramePlan {
        presentation,
        composition,
    })
}

pub(crate) fn require_direct_frame_plan(
    extent: Extent2D,
    format: TextureFormat,
    surface_usage: TextureUsages,
    semantics: TensorFilesFrameSemantics,
) -> Result<TensorFilesFramePlan, String> {
    let plan = compile_frame_plan(extent, format, surface_usage, semantics)?;
    if plan.presentation.target == vulkan_renderer::PresentationTarget::DirectSurface {
        return Ok(plan);
    }
    Err(format!(
        "Tensor Files frame semantics require the shared offscreen path ({:?})",
        plan.presentation.direct_surface_blockers
    ))
}

fn compile_frame_graph(
    upload_mask: usize,
    image_initialized: bool,
    queue_family: u32,
) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        SURFACE_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            if image_initialized {
                RenderGraphImageState::Present
            } else {
                RenderGraphImageState::Undefined
            },
            queue_family,
        ),
    );
    let mut render_dependencies = Vec::with_capacity(FRAME_VERTEX_STREAM_COUNT);
    let mut render_resources = vec![ResourceUse {
        resource: SURFACE_IMAGE,
        kind: ResourceKind::Image,
        access: AccessKind::Write,
        state: ResourceState::color_attachment_write(queue_family),
    }];
    for index in 0..FRAME_VERTEX_STREAM_COUNT {
        let index = u32::try_from(index)
            .map_err(|_| "Vulkan frame has too many vertex buffer streams".to_string())?;
        let resource = ResourceId(FIRST_VERTEX_BUFFER + u64::from(index));
        // Dynamic buffers return to vertex-read state at the end of every
        // frame. Declaring that carried state emits the required read-to-write
        // dependency before an in-place upload; a freshly reallocated buffer
        // merely receives the same harmless execution dependency.
        graph.set_initial_state(
            resource,
            ResourceKind::Buffer,
            ResourceState::vertex_buffer(queue_family),
        );
        if upload_mask & (1 << index) != 0 {
            let upload_pass = PassId(
                FIRST_UPLOAD_PASS
                    .checked_add(index)
                    .ok_or_else(|| "Vulkan frame upload pass ID overflows".to_string())?,
            );
            graph.add_pass(RenderPass {
                id: upload_pass,
                label: format!("tensor-files-vulkan-vertex-upload-{index}"),
                depends_on: Vec::new(),
                resources: vec![ResourceUse {
                    resource,
                    kind: ResourceKind::Buffer,
                    access: AccessKind::Write,
                    state: ResourceState::buffer_copy_destination(queue_family),
                }],
            });
            render_dependencies.push(upload_pass);
        }
        render_resources.push(ResourceUse {
            resource,
            kind: ResourceKind::Buffer,
            access: AccessKind::Read,
            state: ResourceState::vertex_buffer(queue_family),
        });
    }
    graph.add_pass(RenderPass {
        id: RENDER_PASS,
        label: "tensor-files-vulkan-render".into(),
        depends_on: render_dependencies,
        resources: render_resources,
    });
    graph.add_pass(RenderPass {
        id: PRESENT_PASS,
        label: "tensor-files-vulkan-present".into(),
        depends_on: vec![RENDER_PASS],
        resources: vec![ResourceUse {
            resource: SURFACE_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::present(queue_family),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan frame graph: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkan_renderer::{DirectSurfaceBlocker, PresentationTarget};

    const EXTENT: Extent2D = Extent2D::new(1920, 1080);

    #[test]
    fn frame_barrier_cache_precompiles_the_bounded_state_space() {
        let cache = FrameBarrierCache::new(3).unwrap();
        assert_eq!(cache.graphs.len(), FRAME_GRAPH_VARIANTS);
        assert_eq!(frame_graph_cache_key(false, 0), 0);
        assert_eq!(
            frame_graph_cache_key(true, u8::MAX),
            FRAME_GRAPH_VARIANTS - 1
        );

        let warm = &cache.graphs[frame_graph_cache_key(true, 0)];
        assert_eq!(
            warm.barriers
                .iter()
                .filter(|barrier| barrier.after == RENDER_PASS)
                .count(),
            1
        );
        let all_uploaded = &cache.graphs[frame_graph_cache_key(true, 0b11111)];
        assert_eq!(
            all_uploaded
                .barriers
                .iter()
                .filter(|barrier| barrier.after == RENDER_PASS)
                .count(),
            FRAME_VERTEX_STREAM_COUNT + 1
        );
    }

    #[test]
    fn frame_upload_mask_keeps_stream_order_in_the_cache_key() {
        assert_eq!(frame_graph_cache_key(false, 0b00001), 1);
        assert_eq!(frame_graph_cache_key(false, 0b10000), 16);
        assert_eq!(frame_graph_cache_key(true, 0b00101), 37);
    }

    #[test]
    fn ordinary_ui_semantics_compile_to_direct_single_pass() {
        let plan = compile_frame_plan(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT,
            TensorFilesFrameSemantics::direct_ui(),
        )
        .unwrap();
        assert_eq!(plan.presentation.target, PresentationTarget::DirectSurface);
        assert_eq!(
            plan.composition,
            TensorFilesCompositionPath::DirectSinglePass
        );
        assert!(plan.presentation.direct_surface_blockers.is_empty());
    }

    #[test]
    fn direct_frame_plan_cache_revalidates_only_changed_facts() {
        let mut cache = DirectFramePlanCache::default();
        assert!(
            cache
                .require(
                    EXTENT,
                    TextureFormat::Bgra8Srgb,
                    TextureUsages::COLOR_ATTACHMENT,
                    TensorFilesFrameSemantics::direct_ui(),
                )
                .unwrap()
        );
        assert!(
            !cache
                .require(
                    EXTENT,
                    TextureFormat::Bgra8Srgb,
                    TextureUsages::COLOR_ATTACHMENT,
                    TensorFilesFrameSemantics::direct_ui(),
                )
                .unwrap()
        );
        assert!(
            cache
                .require(
                    EXTENT,
                    TextureFormat::Bgra8Srgb,
                    TextureUsages::COLOR_ATTACHMENT,
                    TensorFilesFrameSemantics {
                        has_external_consumer: true,
                        ..TensorFilesFrameSemantics::direct_ui()
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn local_scene_color_dependency_stays_direct_when_surface_is_copyable() {
        let plan = compile_frame_plan(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
            TensorFilesFrameSemantics {
                local_scene_color_dependencies: 2,
                ..TensorFilesFrameSemantics::direct_ui()
            },
        )
        .unwrap();
        assert_eq!(plan.presentation.target, PresentationTarget::DirectSurface);
        assert_eq!(
            plan.composition,
            TensorFilesCompositionPath::DirectWithLocalPasses { dependencies: 2 }
        );
        assert!(plan.presentation.direct_surface_blockers.is_empty());
    }

    #[test]
    fn local_dependency_uses_offscreen_when_surface_cannot_be_copied() {
        let plan = compile_frame_plan(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT,
            TensorFilesFrameSemantics {
                local_scene_color_dependencies: 1,
                ..TensorFilesFrameSemantics::direct_ui()
            },
        )
        .unwrap();
        assert_eq!(plan.presentation.target, PresentationTarget::Offscreen);
        assert_eq!(
            plan.composition,
            TensorFilesCompositionPath::OffscreenMultiPass
        );
        assert_eq!(
            plan.presentation.direct_surface_blockers,
            [
                DirectSurfaceBlocker::MultiplePhysicalPasses,
                DirectSurfaceBlocker::SampledAfterWrite,
            ]
        );
    }

    #[test]
    fn external_consumers_and_history_are_semantic_offscreen_blockers() {
        let plan = compile_frame_plan(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
            TensorFilesFrameSemantics {
                has_history: true,
                has_external_consumer: true,
                ..TensorFilesFrameSemantics::direct_ui()
            },
        )
        .unwrap();
        assert_eq!(plan.presentation.target, PresentationTarget::Offscreen);
        assert_eq!(
            plan.composition,
            TensorFilesCompositionPath::OffscreenMultiPass
        );
        assert!(
            plan.presentation
                .direct_surface_blockers
                .contains(&DirectSurfaceBlocker::History)
        );
        assert!(
            plan.presentation
                .direct_surface_blockers
                .contains(&DirectSurfaceBlocker::ExternalConsumer)
        );
    }

    #[test]
    fn product_can_force_offscreen_without_changing_scene_semantics() {
        let plan = compile_frame_plan_with_preference(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT,
            TensorFilesFrameSemantics::direct_ui(),
            FrameTargetPreference::Offscreen,
        )
        .unwrap();
        assert_eq!(plan.presentation.target, PresentationTarget::Offscreen);
        assert_eq!(
            plan.composition,
            TensorFilesCompositionPath::OffscreenMultiPass
        );
        assert!(plan.presentation.direct_surface_blockers.is_empty());
    }

    #[test]
    fn product_cannot_force_direct_when_local_copy_is_unsupported() {
        let error = compile_frame_plan_with_preference(
            EXTENT,
            TextureFormat::Bgra8Srgb,
            TextureUsages::COLOR_ATTACHMENT,
            TensorFilesFrameSemantics {
                local_scene_color_dependencies: 1,
                ..TensorFilesFrameSemantics::direct_ui()
            },
            FrameTargetPreference::DirectSurface,
        )
        .unwrap_err();
        assert!(error.contains("MultiplePhysicalPasses"));
        assert!(error.contains("SampledAfterWrite"));
    }
}
