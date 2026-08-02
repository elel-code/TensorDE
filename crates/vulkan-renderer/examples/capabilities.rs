use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "support/noop_compute_spirv.rs"]
mod noop_compute_spirv;
#[path = "support/triangle_spirv.rs"]
mod triangle_spirv;

use vulkan_renderer::{
    AccessKind, BinarySemaphoreDescriptor, BufferDescriptor, BufferDescriptorBinding,
    BufferDescriptorKind, BufferUsages, ColorAttachment, ColorTargetState,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor,
    DescriptorHeapDescriptor, DescriptorHeapKind, DeviceDescriptor, DmaBufExportDescriptor,
    DmaBufImageDescriptor, DmaBufPlaneLayout, Extent3D, ExternalImageViewDescriptor,
    ExternalTimelineSemaphoreDescriptor, Features, FragmentState, GraphicsPipelineDescriptor,
    HeapDescriptorType, ImageDescriptor, ImageDimension, ImageTiling, ImageViewDescriptor,
    Instance, InstanceDescriptor, LoadOp, MemoryAllocatorConfig, MemoryLocation, MultisampleState,
    PassId, PipelineCacheDescriptor, PrimitiveState, ProgrammableStage, Rect2D, RenderGraph,
    RenderGraphImageState, RenderPass, RenderingDescriptor, RequestAdapterOptions, ResolveMode,
    ResourceBinding, ResourceId, ResourceKind, ResourceState, ResourceUse, SampleCount,
    ShaderBindingMap, ShaderModuleDescriptor, StoreOp, TextureFormat, TextureLayout, TextureUsages,
    UploadBeltDescriptor, VertexState, Viewport, vk,
};

struct SubmissionProbe(Arc<AtomicUsize>);

impl Drop for SubmissionProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let instance = Instance::new(InstanceDescriptor::default())?;
    for adapter in instance.enumerate_adapters()? {
        let info = adapter.info();
        println!(
            "adapter={} api={} features={:?} descriptor_heap_usable={} roadmap_2026={}",
            info.name,
            info.api_version,
            adapter.features(),
            info.limits.descriptor_heap.is_usable(),
            info.roadmap_2026_ready,
        );
    }

    let adapter = instance.request_adapter(RequestAdapterOptions::default())?;
    let external_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
    let external_capability = if adapter
        .features()
        .contains(Features::EXTERNAL_MEMORY_DMA_BUF)
    {
        adapter
            .drm_format_modifier_capabilities(vk::Format::B8G8R8A8_UNORM, external_usage)?
            .into_iter()
            .filter(|capability| capability.exportable && capability.importable)
            .max_by_key(|capability| capability.plane_count)
    } else {
        None
    };
    let mut device_descriptor = DeviceDescriptor::default();
    if external_capability.is_some() {
        device_descriptor.required_features |= Features::EXTERNAL_MEMORY_DMA_BUF;
    }
    let sync_fd_supported = adapter
        .features()
        .contains(Features::EXTERNAL_SEMAPHORE_SYNC_FD);
    if sync_fd_supported {
        device_descriptor.required_features |= Features::EXTERNAL_SEMAPHORE_SYNC_FD;
    }
    let (device, queue) = adapter.request_device(device_descriptor)?;
    let retained_external_timeline = unsafe {
        device.retain_external_timeline_semaphore(
            &ExternalTimelineSemaphoreDescriptor {
                label: Some("capability-retained-external-timeline".into()),
                semaphore: queue.timeline_semaphore(),
            },
            Arc::new(queue.clone()),
        )?
    };
    assert!(device.features().contains(Features::DESCRIPTOR_HEAP));
    assert!(device.features().contains(Features::FIFO_LATEST_READY));
    let exported_dma_buf = external_capability
        .map(|capability| {
            device.create_exportable_dma_buf_image(&DmaBufExportDescriptor {
                label: Some("capability-exported-dma-buf".into()),
                format: vk::Format::B8G8R8A8_UNORM,
                extent: vk::Extent2D {
                    width: 64,
                    height: 64,
                },
                modifiers: vec![capability.modifier],
                usage: external_usage,
                components: vk::ComponentMapping::default(),
            })
        })
        .transpose()?;
    if let Some(image) = &exported_dma_buf {
        let fd = image.try_clone_fd()?;
        println!(
            "dma_buf_modifier={:#x} planes={} fd={:?}",
            image.modifier(),
            image.planes().len(),
            fd
        );
    }
    let imported_dma_buf = if let Some(image) = &exported_dma_buf {
        let fd = image.try_clone_fd()?;
        Some(
            device.import_dma_buf_image(
                &DmaBufImageDescriptor {
                    label: Some("capability-imported-dma-buf".into()),
                    format: image.format(),
                    extent: image.extent(),
                    modifier: image.modifier(),
                    planes: image
                        .planes()
                        .iter()
                        .map(|plane| DmaBufPlaneLayout {
                            offset: plane.offset,
                            row_pitch: plane.row_pitch,
                        })
                        .collect(),
                    usage: external_usage,
                    components: vk::ComponentMapping::default(),
                },
                fd,
            )?,
        )
    } else {
        None
    };
    if let Some(image) = &imported_dma_buf {
        println!(
            "imported_dma_buf={:?} modifier={:#x}",
            image.raw(),
            image.modifier()
        );
    }
    let sync_fd_signal = sync_fd_supported
        .then(|| {
            device.create_exportable_sync_fd_semaphore(&BinarySemaphoreDescriptor {
                label: Some("capability-sync-fd-signal".into()),
            })
        })
        .transpose()?;
    let resource_heap = device.create_descriptor_heap(&DescriptorHeapDescriptor {
        label: Some("capability-resource-heap".into()),
        kind: DescriptorHeapKind::Resource,
        descriptor_capacity: 4096,
        embedded_samplers: false,
    })?;
    let allocator = device.create_memory_allocator(MemoryAllocatorConfig::default())?;
    let uniform_buffer = allocator.create_buffer(&BufferDescriptor {
        label: Some("capability-uniform".into()),
        size: 256,
        usage: BufferUsages::UNIFORM | BufferUsages::SHADER_DEVICE_ADDRESS,
        memory: MemoryLocation::Upload,
    })?;
    unsafe {
        uniform_buffer.write(0, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])?;
    }
    let uniform = BufferDescriptorBinding::new(
        &resource_heap,
        &uniform_buffer,
        BufferDescriptorKind::Uniform,
        0,
        uniform_buffer.size(),
    )?;
    let image = allocator.create_image(&ImageDescriptor {
        label: Some("capability-image".into()),
        dimension: ImageDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        extent: Extent3D::new(64, 64, 1),
        mip_levels: 1,
        array_layers: 1,
        samples: SampleCount::One,
        tiling: ImageTiling::Optimal,
        usage: TextureUsages::SAMPLED | TextureUsages::STORAGE | TextureUsages::COPY_DESTINATION,
        memory: MemoryLocation::Device,
    })?;
    let subresources = image.full_subresource_range(vk::ImageAspectFlags::COLOR);
    let image_view = image.create_view(&ImageViewDescriptor {
        label: Some("capability-image-view".into()),
        view_type: vk::ImageViewType::_2D,
        format: image.format(),
        components: vk::ComponentMapping::default(),
        subresource_range: subresources,
    })?;
    let retained_external_image_source = unsafe {
        device.retain_external_image(
            &ExternalImageViewDescriptor {
                label: Some("capability-retained-external-image".into()),
                image: image.raw(),
                view_type: vk::ImageViewType::_2D,
                format: image.format(),
                extent: image.extent(),
                mip_levels: image.mip_levels(),
                array_layers: image.array_layers(),
                samples: image.sample_count(),
                usage: image.usage(),
                view_usage: None,
                components: vk::ComponentMapping::default(),
                subresource_range: subresources,
            },
            Arc::new(image.clone()),
        )?
    };
    let retained_external_view = retained_external_image_source.create_view()?;
    assert_eq!(retained_external_view.format(), image.format());
    let render_target = allocator.create_image(&ImageDescriptor {
        label: Some("capability-render-target".into()),
        dimension: ImageDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        extent: Extent3D::new(64, 64, 1),
        mip_levels: 1,
        array_layers: 1,
        samples: SampleCount::One,
        tiling: ImageTiling::Optimal,
        usage: TextureUsages::COLOR_ATTACHMENT | TextureUsages::COPY_SOURCE,
        memory: MemoryLocation::Device,
    })?;
    let render_subresources = render_target.full_subresource_range(vk::ImageAspectFlags::COLOR);
    let render_target_view = render_target.create_view(&ImageViewDescriptor {
        label: Some("capability-render-target-view".into()),
        view_type: vk::ImageViewType::_2D,
        format: render_target.format(),
        components: vk::ComponentMapping::default(),
        subresource_range: render_subresources,
    })?;
    let sampled_image = resource_heap.allocate(HeapDescriptorType::SampledImage)?;
    unsafe {
        resource_heap.write_image(
            &sampled_image,
            HeapDescriptorType::SampledImage,
            &image_view.create_info(),
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )?;
    }
    let storage_image = resource_heap.allocate(HeapDescriptorType::StorageImage)?;
    unsafe {
        resource_heap.write_image(
            &storage_image,
            HeapDescriptorType::StorageImage,
            &image_view.create_info(),
            vk::ImageLayout::GENERAL,
        )?;
    }
    let retained_external_image = resource_heap.allocate(HeapDescriptorType::SampledImage)?;
    unsafe {
        resource_heap.write_image(
            &retained_external_image,
            HeapDescriptorType::SampledImage,
            &retained_external_image_source.view_create_info(),
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )?;
    }
    let sampler_heap = device.create_descriptor_heap(&DescriptorHeapDescriptor {
        label: Some("capability-sampler-heap".into()),
        kind: DescriptorHeapKind::Sampler,
        descriptor_capacity: 4096,
        embedded_samplers: false,
    })?;
    let sampler = sampler_heap.allocate(HeapDescriptorType::Sampler)?;
    unsafe {
        sampler_heap.write_sampler(&sampler, &vk::SamplerCreateInfo::default())?;
    }
    let vertex_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("capability-triangle-vertex".into()),
        spirv: triangle_spirv::VERTEX.to_vec(),
    })?;
    let fragment_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("capability-triangle-fragment".into()),
        spirv: triangle_spirv::FRAGMENT.to_vec(),
    })?;
    let empty_bindings = ShaderBindingMap::default();
    let color_targets = [Some(ColorTargetState {
        format: render_target.format(),
        blend: None,
        write_mask: vulkan_renderer::ColorWrites::ALL,
    })];
    let pipeline_cache = device.create_pipeline_cache(&PipelineCacheDescriptor::default())?;
    let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDescriptor {
        label: Some("capability-triangle-pipeline"),
        vertex: VertexState {
            stage: ProgrammableStage {
                module: &vertex_shader,
                entry_point: c"main",
                bindings: &empty_bindings,
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
                bindings: &empty_bindings,
            },
            targets: &color_targets,
        },
        advanced_blend: None,
        local_read_mapping: None,
        cache: Some(&pipeline_cache),
    })?;
    let compute_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("capability-noop-compute".into()),
        spirv: noop_compute_spirv::COMPUTE.to_vec(),
    })?;
    let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("capability-noop-compute-pipeline"),
        stage: ProgrammableStage {
            module: &compute_shader,
            entry_point: c"main",
            bindings: &empty_bindings,
        },
        cache: Some(&pipeline_cache),
    })?;
    let upload_target = allocator.create_buffer(&BufferDescriptor {
        label: Some("capability-upload-target".into()),
        size: 16,
        usage: BufferUsages::COPY_DESTINATION | BufferUsages::VERTEX,
        memory: MemoryLocation::Device,
    })?;
    let mut upload_belt = device.create_upload_belt(&allocator, UploadBeltDescriptor::default())?;
    let mut uploads = upload_belt.begin(
        &queue,
        &CommandEncoderDescriptor {
            label: Some("capability-smoke".into()),
        },
    )?;
    unsafe {
        uploads.write_buffer(
            &upload_target,
            0,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        )?;
    }
    let encoder = uploads.encoder_mut();
    encoder.retain_resource(&retained_external_image_source);
    encoder.retain_resource(&retained_external_timeline);
    let mut retained_resource_leases = 2;
    if let Some(imported) = &imported_dma_buf {
        encoder.retain_resource(imported);
        retained_resource_leases += 1;
    }
    unsafe {
        let mut compute = encoder.begin_compute(&ComputePassDescriptor {
            label: Some("capability-noop-compute-pass"),
        });
        compute.push_data(0, &[0; 8])?;
        compute.bind_pipeline(&compute_pipeline)?;
        compute.dispatch(1, 1, 1)?;
    }
    let image_resource = ResourceId(1);
    let image_pass = PassId(1);
    let render_resource = ResourceId(2);
    let render_pass = PassId(2);
    let queue_family = device.device_info().queues.graphics;
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        image_resource,
        ResourceKind::Image,
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
    );
    graph.add_pass(RenderPass {
        id: image_pass,
        label: "sample-image".into(),
        depends_on: vec![],
        resources: vec![ResourceUse {
            resource: image_resource,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, queue_family),
        }],
    });
    graph.set_initial_state(
        render_resource,
        ResourceKind::Image,
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
    );
    graph.add_pass(RenderPass {
        id: render_pass,
        label: "draw-triangle".into(),
        depends_on: vec![],
        resources: vec![ResourceUse {
            resource: render_resource,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(RenderGraphImageState::ColorAttachmentWrite, queue_family),
        }],
    });
    let graph = graph.compile()?;
    let bindings = BTreeMap::from([
        (image_resource, ResourceBinding::whole_color_image(&image)),
        (
            render_resource,
            ResourceBinding::whole_color_image(&render_target),
        ),
    ]);
    let barriers = graph.barrier_batch_before(image_pass, &bindings)?;
    let render_barriers = graph.barrier_batch_before(render_pass, &bindings)?;
    unsafe {
        encoder.pipeline_barrier(&barriers);
        encoder.pipeline_barrier(&render_barriers);
        encoder.bind_descriptor_heap(&resource_heap)?;
        encoder.bind_descriptor_heap(&sampler_heap)?;
    }
    let color_attachment = [Some(ColorAttachment {
        view: render_target_view.as_attachment(),
        layout: TextureLayout::ColorAttachment,
        resolve_target: None,
        resolve_layout: TextureLayout::Undefined,
        resolve_mode: ResolveMode::None,
        load_op: LoadOp::Clear([0.0, 0.0, 0.0, 1.0]),
        store_op: StoreOp::Store,
    })];
    let rendering = RenderingDescriptor {
        label: Some("capability-triangle-rendering"),
        render_area: Rect2D::new(0, 0, 64, 64),
        layer_count: 1,
        view_mask: 0,
        color_attachments: &color_attachment,
        depth_attachment: None,
        stencil_attachment: None,
        multisampled_render_to_single_sampled: None,
    };
    unsafe {
        let mut rendering = encoder.begin_rendering(&rendering)?;
        rendering.set_viewport(Viewport {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
            min_depth: 0.0,
            max_depth: 1.0,
        })?;
        rendering.set_scissor(Rect2D::new(0, 0, 64, 64))?;
        rendering.bind_pipeline(&pipeline)?;
        rendering.draw(0..3, 0..1)?;
        rendering.end();
    }
    let lease_dropped = Arc::new(AtomicUsize::new(0));
    uploads
        .encoder_mut()
        .retain(Arc::new(SubmissionProbe(Arc::clone(&lease_dropped))));
    let submission = if let Some(signal) = &sync_fd_signal {
        unsafe { uploads.submit_with_binary_signals(&queue, &[], &[signal])? }
    } else {
        uploads.submit(&queue, &[])?
    };
    assert_eq!(lease_dropped.load(Ordering::Relaxed), 0);
    assert_eq!(
        queue.pending_submission_leases(),
        retained_resource_leases + 3
    );
    uniform.retire(&resource_heap, submission)?;
    resource_heap.retire(sampled_image, submission)?;
    resource_heap.retire(storage_image, submission)?;
    resource_heap.retire(retained_external_image, submission)?;
    sampler_heap.retire(sampler, submission)?;
    queue.wait_for(submission, u64::MAX)?;
    assert_eq!(lease_dropped.load(Ordering::Relaxed), 1);
    assert_eq!(queue.pending_submission_leases(), 0);
    let retained_wait = retained_external_timeline
        .wait(submission.value(), vk::PipelineStageFlags2::ALL_COMMANDS)?;
    assert_eq!(retained_wait.value, submission.value());
    let imported_sync_fd = if let Some(signal) = &sync_fd_signal {
        let fd = unsafe { signal.export_sync_fd()? };
        let imported = device.import_sync_fd_semaphore(
            &BinarySemaphoreDescriptor {
                label: Some("capability-imported-sync-fd".into()),
            },
            fd,
        )?;
        println!("sync_fd_imported={:?}", imported.raw());
        Some(imported)
    } else {
        None
    };
    assert_eq!(resource_heap.reclaim(submission.value()), 4);
    assert_eq!(sampler_heap.reclaim(submission.value()), 1);
    println!(
        "selected={} queue={:?} enabled={:?} completed={}",
        device.device_info().name,
        queue.raw(),
        device.features(),
        queue.completed_timeline()?,
    );
    drop(imported_sync_fd);
    Ok(())
}
