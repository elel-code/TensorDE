use super::*;

const PREVIEW_IMAGE: ResourceId = ResourceId(4);
const PREVIEW_RENDER: PassId = PassId(20);
const PREVIEW_SAMPLE: PassId = PassId(21);

pub(super) fn is_folder_preview_source(slot: &IconGpuSlot) -> bool {
    matches!(
        slot.source.as_ref(),
        Some(IconGpuSource::FolderPreview { .. })
    )
}

/// One composite quad; the full push-data record consumed by
/// `fika_preview_composite.slang` (std430 push-constant block).
///
/// The leading fields are direct descriptor-heap element indices for the
/// child texture and shared sampler, so one push covers bindings and
/// constants.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PreviewCompositeParams {
    pub(super) image_index: u32,
    pub(super) sampler_index: u32,
    pub(super) radius: f32,
    pub(super) mode: f32,
    pub(super) rect: [f32; 4],
    pub(super) color: [f32; 4],
    pub(super) canvas: [f32; 2],
    pub(super) angle: f32,
    pub(super) inset: f32,
    pub(super) blur: f32,
    pub(super) opacity: f32,
    /// Slang rounds the push block size up to its 16-byte alignment.
    pub(super) _pad: [f32; 2],
}

/// Composites pre-rendered child thumbnails into one folder-preview texture.
pub(super) struct FolderPreviewCompositor {
    pipeline: GraphicsPipeline,
    graph: CompiledGraph,
}

impl FolderPreviewCompositor {
    pub(super) fn new(
        device: &Device,
        cache: &PipelineCache,
        queue_family: u32,
    ) -> Result<Self, String> {
        Ok(Self {
            pipeline: create_pipeline(device, cache)?,
            graph: compile_graph(queue_family)?,
        })
    }

    /// Records the composite pass into `target`, leaving it sampleable.
    ///
    /// Child views must already be in `SHADER_READ_ONLY_OPTIMAL`; the encoder
    /// retains them until the submission retires.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn composite(
        &self,
        uploads: &mut UploadBatch<'_>,
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
        target: &Image,
        target_view: &ImageView,
        side: u32,
        draws: &[PreviewCompositeParams],
        child_views: &[ImageView],
    ) -> Result<(), String> {
        let bindings =
            BTreeMap::from([(PREVIEW_IMAGE, ResourceBinding::whole_color_image(target))]);
        let before_render = self
            .graph
            .barrier_batch_before(PREVIEW_RENDER, &bindings)
            .map_err(|error| format!("resolve Vulkan preview render barrier: {error}"))?;
        let before_sample = self
            .graph
            .barrier_batch_before(PREVIEW_SAMPLE, &bindings)
            .map_err(|error| format!("resolve Vulkan preview sample barrier: {error}"))?;
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_render) };
        let color_attachments = [Some(ColorAttachment {
            view: target_view.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear([0.0; 4]),
            store_op: StoreOp::Store,
        })];
        let descriptor = RenderingDescriptor {
            label: Some("fika-vulkan-folder-preview-render"),
            render_area: Rect2D::new(0, 0, side, side),
            layer_count: 1,
            view_mask: 0,
            color_attachments: &color_attachments,
            depth_attachment: None,
            stencil_attachment: None,
            multisampled_render_to_single_sampled: None,
        };
        let mut rendering = unsafe { uploads.encoder_mut().begin_rendering(&descriptor) }
            .map_err(|error| format!("begin Vulkan folder-preview rendering: {error}"))?;
        rendering
            .set_viewport(Viewport {
                x: 0.0,
                y: 0.0,
                width: side as f32,
                height: side as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })
            .map_err(|error| format!("set Vulkan folder-preview viewport: {error}"))?;
        rendering
            .set_scissor(descriptor.render_area)
            .map_err(|error| format!("set Vulkan folder-preview scissor: {error}"))?;
        rendering
            .bind_pipeline(&self.pipeline)
            .map_err(|error| format!("bind Vulkan folder-preview pipeline: {error}"))?;
        rendering.retain_resource(target_view);
        for view in child_views {
            rendering.retain_resource(view);
        }
        unsafe {
            rendering
                .bind_descriptor_heap(resource_heap)
                .map_err(|error| format!("bind Vulkan folder-preview resource heap: {error}"))?;
            rendering
                .bind_descriptor_heap(sampler_heap)
                .map_err(|error| format!("bind Vulkan folder-preview sampler heap: {error}"))?;
            for draw in draws {
                rendering
                    .push_data(0, bytemuck::bytes_of(draw))
                    .map_err(|error| format!("push Vulkan folder-preview params: {error}"))?;
                rendering
                    .draw(0..6, 0..1)
                    .map_err(|error| format!("draw Vulkan folder-preview quad: {error}"))?;
            }
        }
        rendering.end();
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_sample) };
        Ok(())
    }
}

fn create_pipeline(device: &Device, cache: &PipelineCache) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-preview-composite-vertex".into()),
            spirv: vulkan_icon_spirv::PREVIEW_COMPOSITE_VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan preview composite vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-preview-composite-fragment".into()),
            spirv: vulkan_icon_spirv::PREVIEW_COMPOSITE_FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan preview composite fragment shader: {error}"))?;
    let vertex_bindings = ShaderBindingMap::default();
    // The Slang fragment shader selects its texture and sampler through
    // direct descriptor-heap indices in push data, so no binding map exists.
    let fragment_bindings = ShaderBindingMap::default();
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: vulkan_renderer::ColorWrites::ALL,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("fika-vulkan-preview-composite-pipeline"),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex_shader,
                    entry_point: c"main",
                    bindings: &vertex_bindings,
                },
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: FragmentState {
                stage: ProgrammableStage {
                    module: &fragment_shader,
                    entry_point: c"main",
                    bindings: &fragment_bindings,
                },
                targets: &targets,
            },
            advanced_blend: None,
            local_read_mapping: None,
            cache: Some(cache),
        })
        .map_err(|error| format!("create Vulkan preview composite pipeline: {error}"))
}

fn compile_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        PREVIEW_IMAGE,
        ResourceKind::Image,
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
    );
    graph.add_pass(RenderPass {
        id: PREVIEW_RENDER,
        label: "fika-vulkan-preview-render".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: PREVIEW_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::color_attachment_write(queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: PREVIEW_SAMPLE,
        label: "fika-vulkan-preview-sample".into(),
        depends_on: vec![PREVIEW_RENDER],
        resources: vec![ResourceUse {
            resource: PREVIEW_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, queue_family),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan folder-preview graph: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_params_match_std430_push_block_layout() {
        assert_eq!(std::mem::size_of::<PreviewCompositeParams>(), 80);
        assert_eq!(std::mem::offset_of!(PreviewCompositeParams, rect), 16);
        assert_eq!(std::mem::offset_of!(PreviewCompositeParams, color), 32);
        assert_eq!(std::mem::offset_of!(PreviewCompositeParams, canvas), 48);
        assert_eq!(std::mem::offset_of!(PreviewCompositeParams, opacity), 68);
    }

    #[test]
    fn preview_graph_renders_then_samples() {
        let graph = compile_graph(3).unwrap();
        assert_eq!(graph.ordered_passes, [PREVIEW_RENDER, PREVIEW_SAMPLE]);
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == PREVIEW_SAMPLE
                && barrier.destination.image_state()
                    == Some(RenderGraphImageState::FragmentSampledRead)
        }));
    }
}
