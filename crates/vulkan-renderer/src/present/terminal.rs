//! Retained fullscreen terminals for explicit offscreen presentation plans.
//!
//! A product supplies its already-compiled native shader program while this
//! module owns the generic SceneColor-like targets, descriptor heaps, pipeline
//! archive use and terminal command recording.  It deliberately accepts only
//! the one sampled-image/one sampler descriptor-heap ABI: broader material
//! composition belongs in a typed render graph before this terminal.

use super::{
    AcquiredSurfaceTexture, OffscreenColorTarget, OffscreenColorTargets,
    OffscreenColorTargetsDescriptor, OffscreenSampledBindings, OffscreenSamplerTopology,
    PresentationPathPlan, PresentationTarget, TerminalSampling,
};
use crate::{
    Backend, ColorAttachment, ColorTargetState, ColorWrites, CommandEncoder, Error, FragmentState,
    GraphicsPipelineDescriptor, LoadOp, MachineCodeGraphicsPipeline,
    MachineCodeGraphicsPipelineDescriptor, MemoryAllocator, MultisampleState,
    PipelineBinaryArchiveCache, PrimitiveState, ProgrammableStage, Rect2D, RenderingDescriptor,
    ResolveMode, SamplerDescriptor, ShaderBindingMap, ShaderModuleDescriptor, StoreOp,
    TextureLayout, TextureState, TextureUsages, VertexState, Viewport,
};

const DESCRIPTOR_PUSH_BYTES: u32 = 8;

/// Package-owned native shader inputs for a fixed fullscreen sampled terminal.
///
/// The program must consume a sampled-image descriptor index at push bytes
/// `0..4` and a sampler descriptor index at `4..8`.  Its fragment output must
/// implement the alpha policy declared by the accompanying
/// [`PresentationPathPlan`].
#[derive(Clone, Copy, Debug)]
pub struct FullscreenSampledSurfaceTerminalProgram<'a> {
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
    pub descriptor_push_bytes: u32,
}

/// Cold creation descriptor for a retained offscreen terminal.
#[derive(Clone, Debug)]
pub struct FullscreenSampledSurfaceTerminalDescriptor<'a> {
    pub label: Option<String>,
    pub plan: &'a PresentationPathPlan,
    /// Extra usage needed by the authored graph, such as `COPY_SOURCE` for a
    /// later scene-color snapshot. Color-attachment and sampled use are
    /// always retained by the shared target contract.
    pub additional_target_usage: TextureUsages,
    /// Explicit descriptor topology for immutable sampled terminal inputs.
    pub sampler_topology: OffscreenSamplerTopology,
    pub program: FullscreenSampledSurfaceTerminalProgram<'a>,
}

impl FullscreenSampledSurfaceTerminalDescriptor<'_> {
    fn validate(&self) -> crate::Result<()> {
        if self.plan.target != PresentationTarget::Offscreen || self.plan.terminal.is_none() {
            return Err(Error::Validation(
                "fullscreen sampled terminal requires an offscreen presentation plan".into(),
            ));
        }
        if self.program.vertex_spirv.is_empty() || self.program.fragment_spirv.is_empty() {
            return Err(Error::Validation(
                "fullscreen sampled terminal requires non-empty vertex and fragment SPIR-V".into(),
            ));
        }
        if self.program.descriptor_push_bytes != DESCRIPTOR_PUSH_BYTES {
            return Err(Error::Validation(format!(
                "fullscreen sampled terminal requires a {DESCRIPTOR_PUSH_BYTES}-byte sampled-image/sampler push ABI, got {}",
                self.program.descriptor_push_bytes
            )));
        }
        Ok(())
    }
}

/// Renderer-owned retained targets, heaps and pipeline for one terminal
/// fullscreen draw per frame slot.
#[derive(Debug)]
pub struct FullscreenSampledSurfaceTerminal {
    label: Option<String>,
    plan: PresentationPathPlan,
    targets: OffscreenColorTargets,
    sampled_bindings: OffscreenSampledBindings,
    pipeline: MachineCodeGraphicsPipeline,
}

impl Backend {
    /// Creates an allocator-backed terminal for an explicit offscreen plan.
    ///
    /// This remains independent from [`super::PresentationTransaction`]: a
    /// product can select before-frame or late-acquire scheduling without
    /// changing target, descriptor, or pipeline ownership.
    pub fn create_fullscreen_sampled_surface_terminal(
        &self,
        allocator: &MemoryAllocator,
        archive_cache: &PipelineBinaryArchiveCache,
        descriptor: &FullscreenSampledSurfaceTerminalDescriptor<'_>,
    ) -> crate::Result<FullscreenSampledSurfaceTerminal> {
        descriptor.validate()?;
        let mut target_descriptor =
            OffscreenColorTargetsDescriptor::from_plan(descriptor.label.clone(), descriptor.plan)?;
        target_descriptor.additional_usage = descriptor.additional_target_usage;
        let targets = allocator.create_offscreen_color_targets(&target_descriptor)?;
        let terminal = descriptor
            .plan
            .terminal
            .expect("validated offscreen plan has a terminal descriptor");
        let sampled_bindings = self.create_offscreen_sampled_bindings_with_topology(
            &targets,
            terminal_sampler(terminal.sampling),
            descriptor.sampler_topology,
        )?;
        let pipeline = create_pipeline(self, archive_cache, descriptor)?;
        Ok(FullscreenSampledSurfaceTerminal {
            label: descriptor.label.clone(),
            plan: descriptor.plan.clone(),
            targets,
            sampled_bindings,
            pipeline,
        })
    }
}

impl FullscreenSampledSurfaceTerminal {
    pub const fn plan(&self) -> &PresentationPathPlan {
        &self.plan
    }

    /// Borrows one renderer-owned target for graph recording in `frame_slot`.
    pub fn target(&self, frame_slot: usize) -> crate::Result<OffscreenColorTarget<'_>> {
        self.targets.target(frame_slot)
    }

    /// Borrows all target views in stable frame-slot order for cold graph
    /// setup. The renderer keeps their images, descriptor heaps and pipeline
    /// alive for the terminal's full lifetime.
    pub fn target_views(&self) -> &[crate::ImageView] {
        self.targets.views()
    }

    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    pub fn target_allocation_size(&self) -> u64 {
        self.targets.allocation_size()
    }

    /// Records the terminal draw into an acquired surface image.
    ///
    /// The supplied source target must have completed its authored graph in
    /// `ColorAttachmentWrite`; this function transitions it once to sampled
    /// read, keeps the slot's descriptor image immutable, and writes the final
    /// surface image directly as the dynamic-rendering color attachment.
    pub fn record_surface(
        &self,
        encoder: &mut CommandEncoder,
        acquired: &AcquiredSurfaceTexture<'_>,
        frame_slot: usize,
        clear: [f32; 4],
    ) -> crate::Result<()> {
        if acquired.extent() != self.plan.surface_extent {
            return Err(Error::Validation(format!(
                "terminal surface extent {:?} differs from planned {:?}",
                acquired.extent(),
                self.plan.surface_extent
            )));
        }
        if acquired.format() != self.plan.surface_format {
            return Err(Error::Validation(format!(
                "terminal surface format {:?} differs from planned {:?}",
                acquired.format(),
                self.plan.surface_format
            )));
        }
        let target = self.targets.target(frame_slot)?;
        let indices = self.sampled_bindings.indices(frame_slot)?;
        encoder.transition_image(
            target.image,
            TextureState::ColorAttachmentWrite,
            TextureState::FragmentSampledRead,
        )?;
        let extent = acquired.extent();
        let color_attachments = [Some(ColorAttachment {
            view: acquired.as_attachment(),
            layout: TextureLayout::ColorAttachment,
            resolve_target: None,
            resolve_layout: TextureLayout::Undefined,
            resolve_mode: ResolveMode::None,
            load_op: LoadOp::Clear(clear),
            store_op: StoreOp::Store,
        })];
        let rendering_descriptor = RenderingDescriptor {
            label: self.label.as_deref(),
            render_area: Rect2D::new(0, 0, extent.width, extent.height),
            layer_count: 1,
            view_mask: 0,
            color_attachments: &color_attachments,
            depth_attachment: None,
            stencil_attachment: None,
            multisampled_render_to_single_sampled: None,
        };
        let push = sampled_indices_push_data(indices);
        unsafe {
            let mut rendering = encoder.begin_rendering(&rendering_descriptor)?;
            rendering.bind_machine_code_pipeline(&self.pipeline)?;
            rendering.bind_descriptor_heap(self.sampled_bindings.resource_heap())?;
            rendering.bind_descriptor_heap(self.sampled_bindings.sampler_heap())?;
            rendering.retain_resource(target.view);
            rendering.push_data(0, &push)?;
            rendering.set_viewport(Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            })?;
            rendering.set_scissor(Rect2D::new(0, 0, extent.width, extent.height))?;
            rendering.draw(0..3, 0..1)?;
        }
        Ok(())
    }
}

fn terminal_sampler(sampling: TerminalSampling) -> SamplerDescriptor {
    match sampling {
        TerminalSampling::Linear => SamplerDescriptor::linear_clamp(),
        TerminalSampling::Nearest => SamplerDescriptor {
            mag_filter: crate::SamplerFilterMode::Nearest,
            min_filter: crate::SamplerFilterMode::Nearest,
            mipmap_filter: crate::SamplerFilterMode::Nearest,
            ..SamplerDescriptor::linear_clamp()
        },
    }
}

fn create_pipeline(
    device: &Backend,
    archive_cache: &PipelineBinaryArchiveCache,
    descriptor: &FullscreenSampledSurfaceTerminalDescriptor<'_>,
) -> crate::Result<MachineCodeGraphicsPipeline> {
    let vertex = device
        .create_shader_module(ShaderModuleDescriptor {
            label: descriptor
                .label
                .as_deref()
                .map(|label| format!("{label}-vertex")),
            spirv: descriptor.program.vertex_spirv.to_vec(),
        })
        .map_err(|error| {
            Error::Validation(format!("create terminal vertex shader module: {error}"))
        })?;
    let fragment = device
        .create_shader_module(ShaderModuleDescriptor {
            label: descriptor
                .label
                .as_deref()
                .map(|label| format!("{label}-fragment")),
            spirv: descriptor.program.fragment_spirv.to_vec(),
        })
        .map_err(|error| {
            Error::Validation(format!("create terminal fragment shader module: {error}"))
        })?;
    let bindings = ShaderBindingMap::default();
    let targets = [Some(ColorTargetState {
        format: descriptor.plan.surface_format,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    device.create_machine_code_graphics_pipeline(&MachineCodeGraphicsPipelineDescriptor {
        pipeline: GraphicsPipelineDescriptor {
            label: descriptor.label.as_deref(),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex,
                    entry_point: c"main",
                    bindings: &bindings,
                },
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: FragmentState {
                stage: ProgrammableStage {
                    module: &fragment,
                    entry_point: c"main",
                    bindings: &bindings,
                },
                targets: &targets,
            },
            advanced_blend: None,
            local_read_mapping: None,
            cache: None,
        },
        archive_cache,
    })
}

fn sampled_indices_push_data(indices: crate::SampledTextureHeapIndices) -> [u8; 8] {
    let mut push = [0; DESCRIPTOR_PUSH_BYTES as usize];
    push[..4].copy_from_slice(&indices.image.to_ne_bytes());
    push[4..].copy_from_slice(&indices.sampler.to_ne_bytes());
    push
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Extent2D, FrameTargetPreference, PresentationPathDescriptor, PresentationRequirements,
        TerminalAlphaMode, TerminalCompositeDescriptor, TextureFormat,
    };

    fn offscreen_plan() -> PresentationPathPlan {
        PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::Offscreen,
                acquire: super::super::SurfaceAcquireStrategy::BeforeFrame,
                terminal: TerminalCompositeDescriptor {
                    sampling: TerminalSampling::Linear,
                    alpha: TerminalAlphaMode::Opaque,
                },
            },
            PresentationRequirements {
                surface_extent: Extent2D::new(3840, 2160),
                target_extent: Extent2D::new(3840, 2160),
                surface_format: TextureFormat::Bgra8Unorm,
                target_format: TextureFormat::Bgra8Unorm,
                frame_slots: 2,
                physical_pass_count: 2,
                sampled_after_write: true,
                has_history: false,
                has_external_consumer: false,
                uses_async_compute: false,
                requires_terminal_transform: true,
            },
        )
        .unwrap()
    }

    fn descriptor(plan: &PresentationPathPlan) -> FullscreenSampledSurfaceTerminalDescriptor<'_> {
        FullscreenSampledSurfaceTerminalDescriptor {
            label: Some("terminal".into()),
            plan,
            additional_target_usage: TextureUsages::COPY_SOURCE,
            sampler_topology: OffscreenSamplerTopology::PerFrameSlot,
            program: FullscreenSampledSurfaceTerminalProgram {
                vertex_spirv: &[1],
                fragment_spirv: &[2],
                descriptor_push_bytes: DESCRIPTOR_PUSH_BYTES,
            },
        }
    }

    #[test]
    fn terminal_descriptor_requires_exact_descriptor_heap_push_abi() {
        let plan = offscreen_plan();
        let mut descriptor = descriptor(&plan);
        descriptor.program.descriptor_push_bytes = 12;

        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn terminal_descriptor_rejects_direct_surface_plan() {
        let mut plan = offscreen_plan();
        plan.target = PresentationTarget::DirectSurface;
        plan.terminal = None;

        assert!(descriptor(&plan).validate().is_err());
    }

    #[test]
    fn sampled_index_push_data_preserves_image_then_sampler_order() {
        assert_eq!(
            sampled_indices_push_data(crate::SampledTextureHeapIndices {
                image: 9,
                sampler: 4,
            }),
            [9, 0, 0, 0, 4, 0, 0, 0]
        );
    }

    #[test]
    fn terminal_sampler_keeps_nearest_policy_explicit() {
        let sampler = terminal_sampler(TerminalSampling::Nearest);
        assert_eq!(sampler.mag_filter, crate::SamplerFilterMode::Nearest);
        assert_eq!(sampler.min_filter, crate::SamplerFilterMode::Nearest);
    }
}
