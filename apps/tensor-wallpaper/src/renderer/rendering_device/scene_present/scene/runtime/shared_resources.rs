//! Shared allocator ownership for mutable per-frame scene data.

use vulkan_renderer::{
    Backend, Buffer, BufferDescriptor, BufferDescriptorBinding, BufferDescriptorKind, BufferState,
    BufferUsages, CommandEncoderDescriptor, DescriptorHeap, DescriptorHeapDescriptor,
    DescriptorHeapKind, DescriptorSlotKind, DynamicExternalImageDescriptorBinding, FrameToken,
    ImageDescriptorBinding, ImageDescriptorKind, ImageView, MemoryAllocator, MemoryLocation, Queue,
    ReservedDescriptorBinding, SamplerBinding, SamplerDescriptor, TextureLayout, UploadBatch,
    UploadBelt,
};

use crate::engine::scene::SceneStorage;

mod cold_upload;
mod descriptor_phase;
mod effect_target;
mod frame_update;
mod particle;
mod texture;

pub(super) use cold_upload::record_cold_upload;
pub(super) use effect_target::{
    SharedSceneEffectTargetDescriptor, SharedSceneEffectTargetResource,
    SharedSceneEffectTargetResources,
};
pub(super) use particle::{SharedSceneParticleResources, particle_frame_state};
use texture::SharedSceneTextureResources;

use super::draw_recording::SceneGpuDrawCommand;
use super::effect_target::SceneEffectTargetImagePlan;
use super::input_attachment_binding::{
    SceneInputAttachmentBindingPlan, SceneInputAttachmentSource,
};
use super::sampled_binding::{
    SceneSampledImageBindingPlan, SceneSampledImageSource, SceneVideoPlane,
};
use super::scene_owned_uniform::SceneOwnedUniformArenaPlan;
use super::{SCENE_DRAW_UNIFORM_BYTES, SCENE_MATERIAL_UNIFORM_BYTES};

pub(super) enum SharedSceneResourceDescriptor<'a> {
    UniformBuffer {
        buffer: &'a Buffer,
        offset: u64,
        size: u64,
    },
    StorageBuffer {
        buffer: &'a Buffer,
        offset: u64,
        size: u64,
    },
    SampledImage {
        view: &'a ImageView,
        layout: TextureLayout,
    },
    InputAttachment {
        view: &'a ImageView,
        layout: TextureLayout,
    },
    ExternalVideoPlane {
        media_instance: u32,
        plane: SceneVideoPlane,
    },
    Reserved {
        kind: DescriptorSlotKind,
    },
}

impl SharedSceneResourceDescriptor<'_> {
    const fn slot_kind(&self) -> DescriptorSlotKind {
        match self {
            Self::UniformBuffer { .. } => DescriptorSlotKind::UniformBuffer,
            Self::StorageBuffer { .. } => DescriptorSlotKind::StorageBuffer,
            Self::SampledImage { .. } => DescriptorSlotKind::SampledImage,
            Self::ExternalVideoPlane { .. } => DescriptorSlotKind::SampledImage,
            Self::InputAttachment { .. } => DescriptorSlotKind::InputAttachment,
            Self::Reserved { kind } => *kind,
        }
    }
}

enum SharedSceneResourceBinding {
    Buffer(BufferDescriptorBinding),
    Image(ImageDescriptorBinding),
    ExternalVideoPlane {
        media_instance: u32,
        plane: SceneVideoPlane,
        binding: DynamicExternalImageDescriptorBinding,
    },
    Reserved(ReservedDescriptorBinding),
}

impl SharedSceneResourceBinding {
    fn shader_heap_index(&self) -> Result<u32, String> {
        match self {
            Self::Buffer(binding) => binding.shader_heap_index(),
            Self::Image(binding) => binding.shader_heap_index(),
            Self::ExternalVideoPlane { binding, .. } => binding.shader_heap_index(),
            Self::Reserved(binding) => binding.shader_heap_index(),
        }
        .map_err(|error| format!("resolve scene resource descriptor index: {error}"))
    }
}

pub(super) struct SharedSceneDescriptorBindings {
    resource_bindings: Vec<SharedSceneResourceBinding>,
    sampler_bindings: Vec<SamplerBinding>,
    pub resource_indices: Vec<u32>,
    pub sampler_indices: Vec<u32>,
}

impl SharedSceneDescriptorBindings {
    pub(super) fn create(
        resource_heap: &DescriptorHeap,
        sampler_heap: Option<&DescriptorHeap>,
        resources: &[SharedSceneResourceDescriptor<'_>],
        samplers: &[SamplerDescriptor],
    ) -> Result<Self, String> {
        let mut resource_bindings = Vec::with_capacity(resources.len());
        let mut resource_indices = Vec::with_capacity(resources.len());
        for (position, source) in resources.iter().enumerate() {
            let binding = match source {
                SharedSceneResourceDescriptor::UniformBuffer {
                    buffer,
                    offset,
                    size,
                } => SharedSceneResourceBinding::Buffer(
                    BufferDescriptorBinding::new(
                        resource_heap,
                        buffer,
                        BufferDescriptorKind::Uniform,
                        *offset,
                        *size,
                    )
                    .map_err(|error| {
                        format!("write scene uniform descriptor {position}: {error}")
                    })?,
                ),
                SharedSceneResourceDescriptor::StorageBuffer {
                    buffer,
                    offset,
                    size,
                } => SharedSceneResourceBinding::Buffer(
                    BufferDescriptorBinding::new(
                        resource_heap,
                        buffer,
                        BufferDescriptorKind::Storage,
                        *offset,
                        *size,
                    )
                    .map_err(|error| {
                        format!("write scene storage descriptor {position}: {error}")
                    })?,
                ),
                SharedSceneResourceDescriptor::SampledImage { view, layout } => {
                    SharedSceneResourceBinding::Image(
                        ImageDescriptorBinding::new(
                            resource_heap,
                            view,
                            ImageDescriptorKind::Sampled,
                            *layout,
                        )
                        .map_err(|error| {
                            format!("write scene sampled-image descriptor {position}: {error}")
                        })?,
                    )
                }
                SharedSceneResourceDescriptor::InputAttachment { view, layout } => {
                    SharedSceneResourceBinding::Image(
                        ImageDescriptorBinding::new(
                            resource_heap,
                            view,
                            ImageDescriptorKind::InputAttachment,
                            *layout,
                        )
                        .map_err(|error| {
                            format!("write scene input-attachment descriptor {position}: {error}")
                        })?,
                    )
                }
                SharedSceneResourceDescriptor::ExternalVideoPlane {
                    media_instance,
                    plane,
                } => SharedSceneResourceBinding::ExternalVideoPlane {
                    media_instance: *media_instance,
                    plane: *plane,
                    binding: DynamicExternalImageDescriptorBinding::reserve(
                        resource_heap,
                        ImageDescriptorKind::Sampled,
                    )
                    .map_err(|error| {
                        format!(
                            "reserve scene video media instance {media_instance} plane {plane:?} descriptor {position}: {error}"
                        )
                    })?,
                },
                SharedSceneResourceDescriptor::Reserved { kind } => {
                    SharedSceneResourceBinding::Reserved(
                        ReservedDescriptorBinding::new(resource_heap, *kind).map_err(|error| {
                            format!("reserve scene resource descriptor {position}: {error}")
                        })?,
                    )
                }
            };
            let index = binding.shader_heap_index()?;
            validate_dense_index(index, position, "resource")?;
            resource_bindings.push(binding);
            resource_indices.push(index);
        }

        if samplers.is_empty() != sampler_heap.is_none() {
            return Err(
                "scene sampler descriptors and sampler heap presence must match exactly".into(),
            );
        }
        let mut sampler_bindings = Vec::with_capacity(samplers.len());
        let mut sampler_indices = Vec::with_capacity(samplers.len());
        if let Some(heap) = sampler_heap {
            for (position, descriptor) in samplers.iter().copied().enumerate() {
                let binding = SamplerBinding::new(heap, descriptor).map_err(|error| {
                    format!("write scene sampler descriptor {position}: {error}")
                })?;
                let index = binding
                    .shader_heap_index()
                    .map_err(|error| format!("resolve scene sampler descriptor index: {error}"))?;
                validate_dense_index(index, position, "sampler")?;
                sampler_bindings.push(binding);
                sampler_indices.push(index);
            }
        }
        Ok(Self {
            resource_bindings,
            sampler_bindings,
            resource_indices,
            sampler_indices,
        })
    }

    pub(super) fn resource_binding_count(&self) -> usize {
        self.resource_bindings.len()
    }

    pub(super) fn sampler_binding_count(&self) -> usize {
        self.sampler_bindings.len()
    }

    fn validate_external_video_bound(&self) -> Result<(), String> {
        self.resource_bindings.iter().try_for_each(|resource| {
            let SharedSceneResourceBinding::ExternalVideoPlane {
                media_instance,
                plane,
                binding,
            } = resource
            else {
                return Ok(());
            };
            binding.is_bound().then_some(()).ok_or_else(|| {
                format!(
                    "scene video media instance {media_instance} plane {plane:?} descriptor is unbound"
                )
            })
        })
    }

    #[cfg(feature = "video")]
    fn bind_decoded_video_frame(
        &mut self,
        resource_heap: &DescriptorHeap,
        media_instance: u32,
        frame: &vulkan_renderer::DecodedVideoFrame,
    ) -> Result<(), String> {
        let planes = frame.planes();
        let mut bound = [0usize; 2];
        for resource in &mut self.resource_bindings {
            let SharedSceneResourceBinding::ExternalVideoPlane {
                media_instance: candidate,
                plane,
                binding,
            } = resource
            else {
                continue;
            };
            if *candidate != media_instance {
                continue;
            }
            let (plane_index, image) = match plane {
                SceneVideoPlane::Y => (0, planes.y.clone()),
                SceneVideoPlane::Uv => (1, planes.uv.clone()),
            };
            binding
                .bind(resource_heap, image, TextureLayout::ShaderReadOnly)
                .map_err(|error| {
                    format!(
                        "bind scene video media instance {media_instance} plane {plane:?}: {error}"
                    )
                })?;
            bound[plane_index] += 1;
        }
        if bound[0] == 0 || bound[1] == 0 || bound[0] != bound[1] {
            return Err(format!(
                "scene video media instance {media_instance} has mismatched Y/UV descriptor lanes ({}, {})",
                bound[0], bound[1]
            ));
        }
        Ok(())
    }
}

fn validate_dense_index(index: u32, position: usize, role: &str) -> Result<(), String> {
    let expected = u32::try_from(position)
        .map_err(|_| format!("scene {role} descriptor position exceeds u32"))?;
    if index != expected {
        return Err(format!(
            "scene {role} descriptor {position} resolved to sparse heap element {index}"
        ));
    }
    Ok(())
}

pub(super) struct SharedSceneFramePayloads<'a> {
    pub transform: &'a [u8],
    pub video_vertex: Option<&'a [u8]>,
    pub material: Option<&'a [u8]>,
    pub skinning: Option<&'a [u8]>,
    pub scene_owned_uniform: Option<&'a [u8]>,
}

pub(super) struct SharedSceneMeshResources {
    pub vertex: Buffer,
    pub index: Buffer,
}

pub(super) struct SharedSceneColdResourceInputs<'a> {
    pub vertex_payload: &'a [u8],
    pub index_payload: &'a [u8],
    pub storage: &'a SceneStorage,
    pub graph: &'a crate::engine::scene::SceneRenderingDeviceGraphPlan,
    pub sampled_binding_cycle: &'a [SceneSampledImageBindingPlan],
    pub effect_targets: &'a [SceneEffectTargetImagePlan],
}

pub(super) struct SharedSceneColdResources {
    pub mesh: SharedSceneMeshResources,
    pub textures: SharedSceneTextureResources,
    pub effect_targets: SharedSceneEffectTargetResources,
    pub particles: Option<SharedSceneParticleResources>,
    pub upload_frame: FrameToken,
}

impl SharedSceneColdResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        upload_belt: &mut UploadBelt,
        queue: &Queue,
        inputs: SharedSceneColdResourceInputs<'_>,
    ) -> Result<Self, String> {
        let mut uploads = upload_belt
            .begin(
                queue,
                &CommandEncoderDescriptor {
                    label: Some("tensor-wallpaper-scene-cold-resources".into()),
                },
            )
            .map_err(|error| format!("begin scene cold resource upload: {error}"))?;
        let mesh = SharedSceneMeshResources::create(
            allocator,
            &mut uploads,
            queue,
            inputs.vertex_payload,
            inputs.index_payload,
        )?;
        let textures = SharedSceneTextureResources::create(
            allocator,
            &mut uploads,
            queue,
            inputs.storage,
            inputs.sampled_binding_cycle,
        )?;
        let effect_target_descriptors = inputs
            .effect_targets
            .iter()
            .map(SharedSceneEffectTargetDescriptor::from_plan)
            .collect::<Vec<_>>();
        let effect_targets = SharedSceneEffectTargetResources::create(
            allocator,
            &mut uploads,
            &effect_target_descriptors,
        )?;
        let particles = SharedSceneParticleResources::create(
            allocator,
            &mut uploads,
            queue,
            inputs.storage,
            inputs.graph,
        )?;
        let upload_frame = uploads
            .submit(queue, &[])
            .map_err(|error| format!("submit scene cold resources: {error}"))?;
        Ok(Self {
            mesh,
            textures,
            effect_targets,
            particles,
            upload_frame,
        })
    }

    pub(super) fn allocation_bytes(&self) -> u64 {
        self.mesh
            .vertex
            .allocation_size()
            .saturating_add(self.mesh.index.allocation_size())
            .saturating_add(self.textures.allocation_bytes())
            .saturating_add(self.effect_targets.allocation_bytes())
            .saturating_add(
                self.particles
                    .as_ref()
                    .map_or(0, SharedSceneParticleResources::allocation_bytes),
            )
    }
}

impl SharedSceneMeshResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        queue: &Queue,
        vertex_payload: &[u8],
        index_payload: &[u8],
    ) -> Result<Self, String> {
        if vertex_payload.is_empty() || index_payload.is_empty() {
            return Err("scene mesh vertex and index payloads must be non-empty".into());
        }
        let vertex = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-mesh-vertices",
            vertex_payload.len(),
            BufferUsages::VERTEX | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let index = create_device_buffer(
            allocator,
            "tensor-wallpaper-scene-mesh-indices",
            index_payload.len(),
            BufferUsages::INDEX | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&vertex, 0, vertex_payload)
        })
        .map_err(|error| format!("upload scene mesh vertices: {error}"))?;
        record_cold_upload(uploads, queue, |uploads| unsafe {
            uploads.write_buffer(&index, 0, index_payload)
        })
        .map_err(|error| format!("upload scene mesh indices: {error}"))?;
        uploads
            .encoder_mut()
            .transition_buffer(
                &vertex,
                BufferState::TransferDestination,
                BufferState::VertexRead,
            )
            .map_err(|error| format!("transition scene mesh vertices: {error}"))?;
        uploads
            .encoder_mut()
            .transition_buffer(
                &index,
                BufferState::TransferDestination,
                BufferState::IndexRead,
            )
            .map_err(|error| format!("transition scene mesh indices: {error}"))?;
        Ok(Self { vertex, index })
    }
}

pub(super) struct SharedSceneFrameResources {
    pub transform: Buffer,
    pub video_vertex: Option<Buffer>,
    pub material: Option<Buffer>,
    pub skinning: Option<Buffer>,
    pub scene_owned_uniform: Option<Buffer>,
    pub descriptor_phases: Vec<SharedSceneDescriptorPhaseResources>,
    resource_descriptor_count: usize,
    sampler_descriptor_count: usize,
}

pub(super) struct SharedSceneDescriptorPhaseResources {
    pub resource_heap: DescriptorHeap,
    pub sampler_heap: Option<DescriptorHeap>,
    pub reference_phase: usize,
    descriptor_bindings: Option<SharedSceneDescriptorBindings>,
}

pub(super) struct SharedSceneDescriptorInputs<'a> {
    pub slot_kinds: &'a [DescriptorSlotKind],
    pub draw_commands: &'a [SceneGpuDrawCommand],
    pub scene_owned_uniform_plan: &'a SceneOwnedUniformArenaPlan,
    pub sampled_binding_plan: &'a SceneSampledImageBindingPlan,
    pub input_attachment_binding_plan: &'a SceneInputAttachmentBindingPlan,
    pub cold: &'a SharedSceneColdResources,
    pub scene_color: Option<&'a ImageView>,
    pub particle_global_descriptor_base: Option<usize>,
}

impl SharedSceneFrameResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        frame_slot: usize,
        payloads: SharedSceneFramePayloads<'_>,
        resource_descriptor_count_u64: u64,
        sampler_descriptor_count_u64: u64,
    ) -> Result<Self, String> {
        let resource_descriptor_count = usize::try_from(resource_descriptor_count_u64)
            .map_err(|_| "scene resource descriptor count exceeds host address space")?;
        let sampler_descriptor_count = usize::try_from(sampler_descriptor_count_u64)
            .map_err(|_| "scene sampler descriptor count exceeds host address space")?;
        let transform = create_upload_buffer(
            allocator,
            format!("tensor-wallpaper-scene-transform-slot-{frame_slot}"),
            payloads.transform,
            BufferUsages::UNIFORM | BufferUsages::VERTEX | BufferUsages::SHADER_DEVICE_ADDRESS,
        )?;
        let video_vertex = payloads
            .video_vertex
            .map(|payload| {
                create_upload_buffer(
                    allocator,
                    format!("tensor-wallpaper-scene-video-vertex-slot-{frame_slot}"),
                    payload,
                    BufferUsages::VERTEX,
                )
            })
            .transpose()?;
        let material = payloads
            .material
            .map(|payload| {
                create_upload_buffer(
                    allocator,
                    format!("tensor-wallpaper-scene-material-slot-{frame_slot}"),
                    payload,
                    BufferUsages::UNIFORM | BufferUsages::SHADER_DEVICE_ADDRESS,
                )
            })
            .transpose()?;
        let skinning = payloads
            .skinning
            .map(|payload| {
                create_upload_buffer(
                    allocator,
                    format!("tensor-wallpaper-scene-skinning-slot-{frame_slot}"),
                    payload,
                    BufferUsages::STORAGE | BufferUsages::SHADER_DEVICE_ADDRESS,
                )
            })
            .transpose()?;
        let scene_owned_uniform = payloads
            .scene_owned_uniform
            .map(|payload| {
                create_upload_buffer(
                    allocator,
                    format!("tensor-wallpaper-scene-owned-uniform-slot-{frame_slot}"),
                    payload,
                    BufferUsages::UNIFORM | BufferUsages::SHADER_DEVICE_ADDRESS,
                )
            })
            .transpose()?;
        if resource_descriptor_count == 0 {
            return Err("scene frame requires a non-empty resource descriptor heap".into());
        }
        Ok(Self {
            transform,
            video_vertex,
            material,
            skinning,
            scene_owned_uniform,
            descriptor_phases: Vec::new(),
            resource_descriptor_count,
            sampler_descriptor_count,
        })
    }
}

fn assign_resource<'a>(
    resources: &mut [Option<SharedSceneResourceDescriptor<'a>>],
    index: usize,
    descriptor: SharedSceneResourceDescriptor<'a>,
    draw_index: usize,
) -> Result<(), String> {
    let slot = resources.get_mut(index).ok_or_else(|| {
        format!("scene draw {draw_index} resource descriptor {index} exceeds the retained heap")
    })?;
    if slot.is_some() {
        return Err(format!(
            "scene draw {draw_index} resource descriptor {index} is assigned more than once"
        ));
    }
    *slot = Some(descriptor);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_sampled_descriptors<'a>(
    resources: &mut [Option<SharedSceneResourceDescriptor<'a>>],
    samplers: &mut [Option<SamplerDescriptor>],
    draw_index: usize,
    draw: &SceneGpuDrawCommand,
    plan: &SceneSampledImageBindingPlan,
    cold: &'a SharedSceneColdResources,
    scene_color: Option<&'a ImageView>,
) -> Result<(), String> {
    for sampled_index in 0..plan.sampled_slot_count {
        let source = plan.source(draw_index, sampled_index).ok_or_else(|| {
            format!(
                "scene draw {draw_index} sampled descriptor {sampled_index} has no binding plan"
            )
        })?;
        let (view, sampler) = match source {
            SceneSampledImageSource::FallbackWhite => {
                let texture = cold.textures.white_fallback.as_ref().ok_or_else(|| {
                    "scene fallback sampled binding has no fallback texture".to_owned()
                })?;
                (&texture.view, texture.sampler)
            }
            SceneSampledImageSource::SceneTexture { resource } => {
                let texture = cold.textures.texture(resource).ok_or_else(|| {
                    format!(
                        "scene sampled texture resource {} has no GPU image",
                        resource.0
                    )
                })?;
                (&texture.view, texture.sampler)
            }
            SceneSampledImageSource::SceneColorSnapshot => (
                scene_color.ok_or_else(|| {
                    "scene color snapshot descriptor has no frame-slot image view".to_owned()
                })?,
                SamplerDescriptor::linear_clamp(),
            ),
            SceneSampledImageSource::EffectTarget { physical_slot, .. } => {
                let target = cold.effect_targets.target(physical_slot).ok_or_else(|| {
                    format!(
                        "scene sampled effect target physical slot {physical_slot} has no image"
                    )
                })?;
                (&target.view, SamplerDescriptor::linear_clamp())
            }
            SceneSampledImageSource::VideoFramePlane {
                media_instance,
                plane,
            } => {
                assign_resource(
                    resources,
                    draw.sampled_resource_descriptor_base + sampled_index,
                    SharedSceneResourceDescriptor::ExternalVideoPlane {
                        media_instance,
                        plane,
                    },
                    draw_index,
                )?;
                assign_sampler(
                    samplers,
                    draw.sampler_descriptor_base + sampled_index,
                    SamplerDescriptor::linear_clamp(),
                    draw_index,
                )?;
                continue;
            }
        };
        assign_resource(
            resources,
            draw.sampled_resource_descriptor_base + sampled_index,
            SharedSceneResourceDescriptor::SampledImage {
                view,
                layout: TextureLayout::ShaderReadOnly,
            },
            draw_index,
        )?;
        assign_sampler(
            samplers,
            draw.sampler_descriptor_base + sampled_index,
            sampler,
            draw_index,
        )?;
    }
    Ok(())
}

fn assign_sampler(
    samplers: &mut [Option<SamplerDescriptor>],
    index: usize,
    sampler: SamplerDescriptor,
    draw_index: usize,
) -> Result<(), String> {
    let slot = samplers.get_mut(index).ok_or_else(|| {
        format!("scene draw {draw_index} sampler descriptor {index} exceeds the retained heap")
    })?;
    if slot.replace(sampler).is_some() {
        return Err(format!(
            "scene draw {draw_index} sampler descriptor {index} is assigned more than once"
        ));
    }
    Ok(())
}

fn lower_input_attachment_descriptors<'a>(
    resources: &mut [Option<SharedSceneResourceDescriptor<'a>>],
    draw_index: usize,
    draw: &SceneGpuDrawCommand,
    plan: &SceneInputAttachmentBindingPlan,
    cold: &'a SharedSceneColdResources,
) -> Result<(), String> {
    for input_index in 0..plan.input_attachment_slot_count {
        let Some(SceneInputAttachmentSource::EffectTarget {
            physical_slot,
            batch_atlas_tile,
        }) = plan.source(draw_index, input_index)
        else {
            continue;
        };
        if batch_atlas_tile != 0 {
            return Err(format!(
                "scene draw {draw_index} input attachment {input_index} has unsupported atlas tile {batch_atlas_tile}"
            ));
        }
        let target = cold.effect_targets.target(physical_slot).ok_or_else(|| {
            format!("scene input-attachment physical slot {physical_slot} has no image")
        })?;
        assign_resource(
            resources,
            draw.input_attachment_resource_descriptor_base + input_index,
            SharedSceneResourceDescriptor::InputAttachment {
                view: &target.view,
                layout: TextureLayout::RenderingLocalRead,
            },
            draw_index,
        )?;
    }
    Ok(())
}

fn create_upload_buffer(
    allocator: &MemoryAllocator,
    label: String,
    payload: &[u8],
    usage: BufferUsages,
) -> Result<Buffer, String> {
    if payload.is_empty() {
        return Err(format!("scene upload buffer {label:?} cannot be empty"));
    }
    let buffer = allocator
        .create_buffer(&BufferDescriptor {
            label: Some(label.clone()),
            size: payload.len() as u64,
            usage,
            memory: MemoryLocation::Upload,
        })
        .map_err(|error| format!("create scene upload buffer {label:?}: {error}"))?;
    unsafe { buffer.write(0, payload) }
        .map_err(|error| format!("initialize scene upload buffer {label:?}: {error}"))?;
    Ok(buffer)
}

fn create_device_buffer(
    allocator: &MemoryAllocator,
    label: &str,
    byte_count: usize,
    usage: BufferUsages,
) -> Result<Buffer, String> {
    let size = u64::try_from(byte_count)
        .map_err(|_| format!("scene device buffer {label:?} size exceeds u64"))?;
    allocator
        .create_buffer(&BufferDescriptor {
            label: Some(label.into()),
            size,
            usage: usage | BufferUsages::COPY_DESTINATION,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create scene device buffer {label:?}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::validate_dense_index;

    #[test]
    fn direct_heap_indices_must_match_exact_plan_positions() {
        assert!(validate_dense_index(0, 0, "resource").is_ok());
        assert!(validate_dense_index(17, 17, "sampler").is_ok());
        assert!(validate_dense_index(2, 1, "resource").is_err());
    }
}
