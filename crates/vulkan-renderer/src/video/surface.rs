//! Retained direct-surface terminals for renderer-owned decoded video frames.
//!
//! The product supplies a package-owned native SPIR-V program. The renderer
//! owns Y/UV descriptor lanes per frame slot, samplers, machine-code pipeline,
//! decode image acquire/release barriers and the final swapchain draw.

use crate::{
    AcquiredSurfaceTexture, Backend, ColorAttachment, ColorTargetState, ColorWrites,
    CommandEncoder, DecodedVideoFrame, DescriptorHeap, DescriptorHeapDescriptor,
    DescriptorHeapKind, DynamicExternalImageDescriptorBinding, Error, FragmentState,
    GraphicsPipelineDescriptor, ImageDescriptorKind, LoadOp, MachineCodeGraphicsPipeline,
    MachineCodeGraphicsPipelineDescriptor, MultisampleState, PipelineBinaryArchiveCache,
    PrimitiveState, ProgrammableStage, Rect2D, RenderingDescriptor, ResolveMode, SamplerBinding,
    SamplerDescriptor, ShaderBindingMap, ShaderModuleDescriptor, StoreOp, TextureFormat,
    TextureLayout, VertexState, Viewport,
};

const DESCRIPTOR_PUSH_BYTES: u32 = 16;

/// Package-owned native shader inputs for a fixed two-plane decoded-video
/// terminal.
///
/// The program must consume Y, UV, Y-sampler and UV-sampler heap indices at
/// contiguous push offsets `0`, `4`, `8` and `12`, respectively.
#[derive(Clone, Copy, Debug)]
pub struct DecodedVideoSurfaceTerminalProgram<'a> {
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
    pub descriptor_push_bytes: u32,
}

/// Cold creation descriptor for retained decoded-video surface presentation.
#[derive(Clone, Debug)]
pub struct DecodedVideoSurfaceTerminalDescriptor<'a> {
    pub label: Option<String>,
    pub surface_format: TextureFormat,
    pub frame_slots: u32,
    pub program: DecodedVideoSurfaceTerminalProgram<'a>,
}

impl DecodedVideoSurfaceTerminalDescriptor<'_> {
    fn validate(&self) -> crate::Result<()> {
        if self.frame_slots == 0 {
            return Err(Error::Validation(
                "decoded-video surface terminal requires at least one frame slot".into(),
            ));
        }
        if self.program.vertex_spirv.is_empty() || self.program.fragment_spirv.is_empty() {
            return Err(Error::Validation(
                "decoded-video surface terminal requires non-empty vertex and fragment SPIR-V"
                    .into(),
            ));
        }
        if self.program.descriptor_push_bytes != DESCRIPTOR_PUSH_BYTES {
            return Err(Error::Validation(format!(
                "decoded-video surface terminal requires a {DESCRIPTOR_PUSH_BYTES}-byte Y/UV descriptor push ABI, got {}",
                self.program.descriptor_push_bytes
            )));
        }
        Ok(())
    }
}

/// Renderer-owned descriptor lanes and direct-surface pipeline for decoded
/// Y/UV frames.
#[derive(Debug)]
pub struct DecodedVideoSurfaceTerminal {
    label: Option<String>,
    resource_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    frames: Vec<DecodedVideoFrameBindings>,
    y_sampler: SamplerBinding,
    uv_sampler: SamplerBinding,
    pipeline: MachineCodeGraphicsPipeline,
    surface_format: TextureFormat,
}

#[derive(Debug)]
struct DecodedVideoFrameBindings {
    y_plane: DynamicExternalImageDescriptorBinding,
    uv_plane: DynamicExternalImageDescriptorBinding,
}

impl Backend {
    /// Creates retained per-slot descriptor state and a direct-surface Y/UV
    /// terminal. Call [`DecodedVideoSurfaceTerminal::record_surface`] only in
    /// a transaction surface callback after that frame slot has retired.
    pub fn create_decoded_video_surface_terminal(
        &self,
        archive_cache: &PipelineBinaryArchiveCache,
        descriptor: &DecodedVideoSurfaceTerminalDescriptor<'_>,
    ) -> crate::Result<DecodedVideoSurfaceTerminal> {
        descriptor.validate()?;
        let resource_count = u64::from(descriptor.frame_slots)
            .checked_mul(2)
            .ok_or_else(|| {
                Error::Validation("decoded-video descriptor count overflows u64".into())
            })?;
        let resource_heap = create_heap(
            self,
            DescriptorHeapKind::Resource,
            descriptor.label.as_deref(),
            "resource",
            resource_count,
        )?;
        let sampler_heap = create_heap(
            self,
            DescriptorHeapKind::Sampler,
            descriptor.label.as_deref(),
            "sampler",
            2,
        )?;
        let mut frames = Vec::with_capacity(descriptor.frame_slots as usize);
        for frame_slot in 0..descriptor.frame_slots as usize {
            let y_plane = DynamicExternalImageDescriptorBinding::reserve(
                &resource_heap,
                ImageDescriptorKind::Sampled,
            )?;
            let uv_plane = DynamicExternalImageDescriptorBinding::reserve(
                &resource_heap,
                ImageDescriptorKind::Sampled,
            )?;
            let [expected_y, expected_uv] = frame_slot_descriptor_indices(frame_slot)?;
            validate_descriptor_index(&y_plane, expected_y, frame_slot, "Y")?;
            validate_descriptor_index(&uv_plane, expected_uv, frame_slot, "UV")?;
            frames.push(DecodedVideoFrameBindings { y_plane, uv_plane });
        }
        let y_sampler = SamplerBinding::new(&sampler_heap, SamplerDescriptor::linear_clamp())?;
        let uv_sampler = SamplerBinding::new(&sampler_heap, SamplerDescriptor::linear_clamp())?;
        validate_sampler_index(&y_sampler, 0, "Y")?;
        validate_sampler_index(&uv_sampler, 1, "UV")?;
        let pipeline = create_pipeline(self, archive_cache, descriptor)?;
        Ok(DecodedVideoSurfaceTerminal {
            label: descriptor.label.clone(),
            resource_heap,
            sampler_heap,
            frames,
            y_sampler,
            uv_sampler,
            pipeline,
            surface_format: descriptor.surface_format,
        })
    }
}

impl DecodedVideoSurfaceTerminal {
    pub fn frame_slot_count(&self) -> usize {
        self.frames.len()
    }

    /// Rewrites the current slot's opaque Y/UV descriptor lanes, records the
    /// decoder ownership acquire/release pair and draws straight into the
    /// acquired swapchain image. The caller's presentation transaction owns
    /// the corresponding decode timeline wait and AVFrame lease.
    pub fn record_surface(
        &mut self,
        encoder: &mut CommandEncoder,
        acquired: &AcquiredSurfaceTexture<'_>,
        frame_slot: usize,
        frame: &DecodedVideoFrame,
        clear: [f32; 4],
    ) -> crate::Result<()> {
        if acquired.format() != self.surface_format {
            return Err(Error::Validation(format!(
                "decoded-video surface format {:?} differs from terminal pipeline {:?}",
                acquired.format(),
                self.surface_format
            )));
        }
        let planes = frame.planes();
        let bindings = self.frames.get_mut(frame_slot).ok_or_else(|| {
            Error::Validation(format!("decoded-video frame slot {frame_slot} is missing"))
        })?;
        bindings.y_plane.bind(
            &self.resource_heap,
            planes.y.clone(),
            TextureLayout::ShaderReadOnly,
        )?;
        bindings.uv_plane.bind(
            &self.resource_heap,
            planes.uv.clone(),
            TextureLayout::ShaderReadOnly,
        )?;
        encoder.begin_decoded_video_sampling(frame)?;
        unsafe {
            encoder.bind_descriptor_heap(&self.resource_heap)?;
            encoder.bind_descriptor_heap(&self.sampler_heap)?;
        }
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
        let push = descriptor_push_data(bindings, &self.y_sampler, &self.uv_sampler)?;
        unsafe {
            let mut rendering = encoder.begin_rendering(&rendering_descriptor)?;
            rendering.bind_machine_code_pipeline(&self.pipeline)?;
            rendering.retain_resource(planes.y);
            rendering.retain_resource(planes.uv);
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
            rendering.draw(0..3, 0..frame.array_layers())?;
        }
        encoder.end_decoded_video_sampling(frame)
    }
}

fn create_heap(
    device: &Backend,
    kind: DescriptorHeapKind,
    label: Option<&str>,
    role: &str,
    descriptor_count: u64,
) -> crate::Result<DescriptorHeap> {
    let capacity = device.descriptor_heap_capacity_bytes(kind, descriptor_count)?;
    device.create_descriptor_heap(&DescriptorHeapDescriptor {
        label: label.map(|label| format!("{label}-{role}-heap")),
        kind,
        descriptor_capacity: capacity,
        embedded_samplers: false,
    })
}

fn validate_descriptor_index(
    binding: &DynamicExternalImageDescriptorBinding,
    expected: u32,
    frame_slot: usize,
    plane: &str,
) -> crate::Result<()> {
    let index = binding.shader_heap_index()?;
    if index != expected {
        return Err(Error::Validation(format!(
            "decoded-video frame slot {frame_slot} {plane} descriptor is sparse at index {index}, expected {expected}"
        )));
    }
    Ok(())
}

fn frame_slot_descriptor_indices(frame_slot: usize) -> crate::Result<[u32; 2]> {
    let y = u32::try_from(frame_slot)
        .ok()
        .and_then(|slot| slot.checked_mul(2))
        .ok_or_else(|| Error::Validation("decoded-video Y descriptor index exceeds u32".into()))?;
    let uv = y
        .checked_add(1)
        .ok_or_else(|| Error::Validation("decoded-video UV descriptor index exceeds u32".into()))?;
    Ok([y, uv])
}

fn validate_sampler_index(
    binding: &SamplerBinding,
    expected: u32,
    plane: &str,
) -> crate::Result<()> {
    let index = binding.shader_heap_index()?;
    if index != expected {
        return Err(Error::Validation(format!(
            "decoded-video {plane} sampler is sparse at index {index}, expected {expected}"
        )));
    }
    Ok(())
}

fn create_pipeline(
    device: &Backend,
    archive_cache: &PipelineBinaryArchiveCache,
    descriptor: &DecodedVideoSurfaceTerminalDescriptor<'_>,
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
            Error::Validation(format!("create decoded-video vertex shader: {error}"))
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
            Error::Validation(format!("create decoded-video fragment shader: {error}"))
        })?;
    let bindings = ShaderBindingMap::default();
    let targets = [Some(ColorTargetState {
        format: descriptor.surface_format,
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

fn descriptor_push_data(
    bindings: &DecodedVideoFrameBindings,
    y_sampler: &SamplerBinding,
    uv_sampler: &SamplerBinding,
) -> crate::Result<[u8; DESCRIPTOR_PUSH_BYTES as usize]> {
    let indices = [
        bindings.y_plane.shader_heap_index()?,
        bindings.uv_plane.shader_heap_index()?,
        y_sampler.shader_heap_index()?,
        uv_sampler.shader_heap_index()?,
    ];
    let mut push = [0; DESCRIPTOR_PUSH_BYTES as usize];
    for (position, index) in indices.into_iter().enumerate() {
        push[position * 4..position * 4 + 4].copy_from_slice(&index.to_ne_bytes());
    }
    Ok(push)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> DecodedVideoSurfaceTerminalDescriptor<'static> {
        DecodedVideoSurfaceTerminalDescriptor {
            label: Some("video".into()),
            surface_format: TextureFormat::Bgra8Unorm,
            frame_slots: 2,
            program: DecodedVideoSurfaceTerminalProgram {
                vertex_spirv: &[1],
                fragment_spirv: &[2],
                descriptor_push_bytes: DESCRIPTOR_PUSH_BYTES,
            },
        }
    }

    #[test]
    fn terminal_descriptor_rejects_zero_frame_slots() {
        let mut descriptor = descriptor();
        descriptor.frame_slots = 0;

        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn terminal_descriptor_requires_four_descriptor_indices() {
        let mut descriptor = descriptor();
        descriptor.program.descriptor_push_bytes = 12;

        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn frame_slot_indices_are_exact_dense_y_uv_pairs() {
        assert_eq!(frame_slot_descriptor_indices(0).unwrap(), [0, 1]);
        assert_eq!(frame_slot_descriptor_indices(2).unwrap(), [4, 5]);
    }
}
