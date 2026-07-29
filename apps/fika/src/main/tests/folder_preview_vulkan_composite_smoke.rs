#[test]
fn folder_preview_slot_composites_on_native_vulkan() {
    use std::sync::Arc;

    use vulkan_renderer::{
        CommandEncoderDescriptor, DeviceDescriptor, Instance, InstanceDescriptor,
        MemoryAllocatorConfig, PipelineCacheDescriptor, RequestAdapterOptions,
        UploadBeltDescriptor, vk,
    };

    let _gpu = GPU_TEST_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let instance = match Instance::new(InstanceDescriptor::default()) {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("skip: no Vulkan instance: {error}");
            return;
        }
    };
    let adapter = match instance.request_adapter(RequestAdapterOptions::default()) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("skip: no Vulkan 1.4 adapter: {error}");
            return;
        }
    };
    let (device, queue) = match adapter.request_device(DeviceDescriptor {
        label: Some("fika-folder-preview-smoke".into()),
        ..DeviceDescriptor::default()
    }) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("skip: request_device failed: {error}");
            return;
        }
    };
    let allocator = device
        .create_memory_allocator(MemoryAllocatorConfig {
            device_block_size: 8 * 1024 * 1024,
            image_block_size: 8 * 1024 * 1024,
            upload_block_size: 4 * 1024 * 1024,
            readback_block_size: 4 * 1024 * 1024,
            dedicated_threshold: 16 * 1024 * 1024,
        })
        .unwrap();
    let pipeline_cache = device
        .create_pipeline_cache(&PipelineCacheDescriptor {
            label: Some("fika-folder-preview-smoke-cache".into()),
            initial_data: Vec::new(),
        })
        .unwrap();
    let mut renderer = crate::vulkan_icon::VulkanIconRenderer::new(
        &device,
        &allocator,
        &pipeline_cache,
        vk::Format::B8G8R8A8_UNORM,
    )
    .unwrap();
    // Validate the remaining Slang pipelines against the real driver too.
    crate::vulkan_text::VulkanTextRenderer::new(
        &device,
        &allocator,
        &pipeline_cache,
        vk::Format::B8G8R8A8_UNORM,
    )
    .unwrap();
    crate::vulkan_rect::VulkanRectRenderer::new(
        &device,
        &pipeline_cache,
        vk::Format::B8G8R8A8_UNORM,
    )
    .unwrap();
    let mut belt = device
        .create_upload_belt(
            &allocator,
            UploadBeltDescriptor {
                chunk_size: 1024 * 1024,
                ..UploadBeltDescriptor::default()
            },
        )
        .unwrap();

    let unique = format!(
        "fika-folder-preview-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&root).unwrap();
    let bitmap_child = root.join("photo.png");
    image::RgbaImage::from_pixel(24, 16, image::Rgba([200, 40, 40, 255]))
        .save(&bitmap_child)
        .unwrap();
    let svg_child = root.join("vector.svg");
    std::fs::write(
        &svg_child,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <rect x="2" y="2" width="16" height="16" fill="#3060c0"/>
        </svg>"##,
    )
    .unwrap();

    const SIDE: u32 = 64;
    let mut frame = IconFrame {
        slots: vec![IconGpuSlot {
            identity: IconGpuUploadKey::content(root.clone(), 1),
            width: SIDE,
            height: SIDE,
            content_width: SIDE,
            content_height: SIDE,
            content_hash: 7,
            rounding: None,
            source: Some(IconGpuSource::FolderPreview {
                children: Arc::from(vec![bitmap_child, svg_child]),
                size_px: SIDE as u16,
                seed: 11,
            }),
            dmabuf: None,
        }],
        content_batches: Vec::new(),
        overlay_batches: Vec::new(),
        content_vertices: Vec::new(),
        overlay_vertices: Vec::new(),
        stats: IconFrameStats::default(),
    };

    let mut uploads = belt
        .begin(
            &queue,
            &CommandEncoderDescriptor {
                label: Some("fika-folder-preview-smoke".into()),
            },
        )
        .unwrap();
    renderer
        .upload(&device, &allocator, &mut uploads, &mut frame, None)
        .unwrap();
    let token = uploads.submit(&queue, &[]).unwrap();
    queue.wait_for(token, u64::MAX).unwrap();

    assert_eq!(frame.stats.atlas_uploads, 1);
    let resident = renderer.resident_index();
    let entry = resident
        .entries
        .get(&IconGpuUploadKey::content(root.clone(), 1))
        .expect("folder preview should be resident after upload");
    assert_eq!((entry.width, entry.height), (SIDE, SIDE));

    // A second frame retires the child descriptors against the first
    // submission and reuses the resident preview without re-uploading.
    let mut uploads = belt
        .begin(
            &queue,
            &CommandEncoderDescriptor {
                label: Some("fika-folder-preview-smoke-2".into()),
            },
        )
        .unwrap();
    renderer
        .upload(&device, &allocator, &mut uploads, &mut frame, Some(token))
        .unwrap();
    let token = uploads.submit(&queue, &[]).unwrap();
    queue.wait_for(token, u64::MAX).unwrap();
    assert_eq!(frame.stats.atlas_uploads, 0);
    assert_eq!(frame.stats.atlas_upload_skips, 1);

    let _ = std::fs::remove_dir_all(&root);
}
