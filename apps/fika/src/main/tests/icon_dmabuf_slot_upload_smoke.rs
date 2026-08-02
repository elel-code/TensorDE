#[test]
fn icon_slot_imports_udmabuf_into_native_vulkan_resident_cache() {
    use crate::ui::render::dmabuf::{DmabufImportPlane, try_allocate_udmabuf_argb8888};
    use vulkan_renderer::{
        CommandEncoderDescriptor, DeviceDescriptor, Features, Instance, InstanceDescriptor,
        MemoryAllocatorConfig, PipelineCacheDescriptor, RequestAdapterOptions,
        TextureFormat, UploadBeltDescriptor,
    };
    use wayland_client_runtime::fourcc;

    const W: u32 = 16;
    const H: u32 = 16;

    let _gpu = GPU_TEST_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some((fd, stride)) = try_allocate_udmabuf_argb8888(W, H) else {
        eprintln!("skip: /dev/udmabuf unavailable or permission denied");
        return;
    };
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
    if !adapter
        .features()
        .contains(Features::EXTERNAL_MEMORY_DMA_BUF)
    {
        eprintln!("skip: adapter lacks native Vulkan dma-buf import");
        return;
    }
    let (device, queue) = match adapter.request_device(DeviceDescriptor {
        label: Some("fika-native-dmabuf-icon-smoke".into()),
        required_features: DeviceDescriptor::default().required_features
            | Features::EXTERNAL_MEMORY_DMA_BUF,
        ..DeviceDescriptor::default()
    }) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("skip: native Vulkan device request failed: {error}");
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
            label: Some("fika-native-dmabuf-icon-cache".into()),
            initial_data: Vec::new(),
        })
        .unwrap();
    let mut renderer = crate::vulkan_icon::VulkanIconRenderer::new(
        &device,
        &allocator,
        &pipeline_cache,
        TextureFormat::Bgra8Unorm,
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
    let identity = IconGpuUploadKey::theme_asset(
        PathBuf::from("/test/native-dmabuf-icon"),
        W.max(H) as u16,
    );
    let mut frame = IconFrame {
        slots: vec![IconGpuSlot {
            identity: identity.clone(),
            width: W,
            height: H,
            content_width: W,
            content_height: H,
            content_hash: 1,
            rounding: None,
            source: None,
            dmabuf: Some(IconDmabufSource {
                fourcc: fourcc::ARGB8888,
                plane: DmabufImportPlane {
                    fd,
                    offset: 0,
                    stride,
                    modifier: fourcc::MOD_LINEAR,
                },
            }),
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
                label: Some("fika-native-dmabuf-icon-upload".into()),
            },
        )
        .unwrap();
    if let Err(error) = renderer.upload(&device, &allocator, &mut uploads, &mut frame, None) {
        eprintln!("skip: driver rejected native linear udmabuf import: {error}");
        return;
    }
    let token = uploads.submit(&queue, &[]).unwrap();
    queue.wait_for(token, u64::MAX).unwrap();

    assert_eq!(frame.stats.atlas_uploads, 1);
    assert!(renderer.resident_index().entries.contains_key(&identity));
    assert!(frame.slots[0].dmabuf.is_none());
}
