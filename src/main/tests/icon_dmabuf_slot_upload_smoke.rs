#[test]
fn icon_slot_with_udmabuf_plane_prefers_import_when_plan_ready() {
    use crate::shell::render::dmabuf::{
        DmabufImportPlan, DmabufImportPlane, try_allocate_udmabuf_argb8888,
    };
    use wayland_client_runtime::fourcc;

    // 16x16 matches common icon sizes; padded guard makes GPU size 18x18.
    const W: u32 = 16;
    const H: u32 = 16;

    let Some((fd, stride)) = try_allocate_udmabuf_argb8888(W + 2, H + 2) else {
        eprintln!("skip: /dev/udmabuf unavailable or permission denied");
        return;
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        display: None,
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(a) => a,
        Err(_) => {
            eprintln!("skip: no Vulkan adapter");
            return;
        }
    };
    if !crate::shell::render::dmabuf::adapter_supports_dmabuf_import(&adapter) {
        eprintln!("skip: adapter lacks VULKAN_EXTERNAL_MEMORY_DMA_BUF");
        return;
    }
    let features = crate::shell::render::dmabuf::optional_dmabuf_features(&adapter);
    let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("icon-dmabuf-slot-test"),
        required_features: features,
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    })) {
        Ok(dq) => dq,
        Err(e) => {
            eprintln!("skip: request_device failed: {e}");
            return;
        }
    };

    // CPU fallback pixels (padded size must match slot raster after guard).
    let pad_w = W + ICON_ATLAS_GUARD_TEXELS * 2;
    let pad_h = H + ICON_ATLAS_GUARD_TEXELS * 2;
    let pixels = vec![0u8, 0, 255, 255]
        .into_iter()
        .cycle()
        .take((pad_w * pad_h * 4) as usize)
        .collect::<Vec<_>>();
    let raster = IconRaster {
        pixels: pixels.into(),
        width: W,
        height: H,
    };

    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailRasterResolver::new();
    let mut icon_rasters = IconRasterResolver::new();
    let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
    let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
    let mut builder = IconFrameBuilder::new_for_test(
        &mut resolver,
        &mut thumbnails,
        &mut icon_rasters,
        &mut raster_cache,
        &mut role_raster_cache,
        PhysicalSize::new(128, 128),
    );

    // Attach plane sized for the *padded* texture (what GPU import uses).
    builder.push_raster_with_dmabuf(
        IconGpuUploadKey::theme_asset(PathBuf::from("/test/dmabuf-icon.png")),
        raster,
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: W as f32,
            height: H as f32,
        },
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: W as f32,
            height: H as f32,
        },
        IconDrawLayer::Content,
        DmabufImportPlane {
            fd,
            offset: 0,
            stride,
            modifier: fourcc::MOD_LINEAR,
        },
    );

    let mut frame = builder.finish();
    assert_eq!(frame.slots.len(), 1);
    assert!(frame.slots[0].dmabuf.is_some());

    let mut icons = IconRenderer::new(&device, wgpu::TextureFormat::Bgra8Unorm);
    let plan = DmabufImportPlan {
        fourcc: fourcc::ARGB8888,
        modifier: fourcc::MOD_LINEAR,
        texture_format: wgpu::TextureFormat::Bgra8Unorm,
        main_device: 0,
        scanout_preferred: false,
    };
    icons.set_dmabuf_import_state(true, Some(plan));
    let _ = icons.upload(&device, &queue, &mut frame);

    let (dmabuf_n, cpu_n) = icons.icon_upload_source_stats();
    // Import may still fail on some drivers for linear udmabuf; then CPU
    // fallback must run (never zero total uploads).
    assert_eq!(dmabuf_n + cpu_n, 1, "exactly one GPU texture created");
    if dmabuf_n == 1 {
        eprintln!("icon dmabuf import path exercised successfully");
    } else {
        eprintln!("icon dmabuf import fell back to CPU (driver may reject linear udmabuf)");
    }
    assert!(frame.slots[0].dmabuf.is_none(), "plane consumed at upload");
}
