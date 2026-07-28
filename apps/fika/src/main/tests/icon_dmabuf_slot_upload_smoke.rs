#[test]
fn icon_slot_with_udmabuf_plane_prefers_import_when_plan_ready() {
    use crate::ui::render::dmabuf::{
        DmabufImportPlan, DmabufImportPlane, try_allocate_udmabuf_argb8888,
    };
    use wayland_client_runtime::fourcc;

    const W: u32 = 16;
    const H: u32 = 16;

    let Some((fd, stride)) = try_allocate_udmabuf_argb8888(W, H) else {
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
    let adapter = match futures_lite::future::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
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
    if !crate::ui::render::dmabuf::adapter_supports_dmabuf_import(&adapter) {
        eprintln!("skip: adapter lacks VULKAN_EXTERNAL_MEMORY_DMA_BUF");
        return;
    }
    let features = crate::ui::render::dmabuf::optional_dmabuf_features(&adapter);
    let (device, queue) = match futures_lite::future::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
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

    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let mut builder = IconFrameBuilder::new_for_test(
        &mut resolver,
        &mut thumbnails,
        PhysicalSize::new(128, 128),
    );

    builder.push_dmabuf_draw(IconDmabufDraw {
        identity: IconGpuUploadKey::theme_asset(PathBuf::from("/test/dmabuf-icon.png")),
        width: W,
        height: H,
        content_hash: 1,
        rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: W as f32,
            height: H as f32,
        },
        screen: ViewRect {
            x: 0.0,
            y: 0.0,
            width: W as f32,
            height: H as f32,
        },
        layer: IconDrawLayer::Content,
        plane: DmabufImportPlane {
            fd,
            offset: 0,
            stride,
            modifier: fourcc::MOD_LINEAR,
        },
    });

    let mut frame = builder.finish();
    assert_eq!(frame.slots.len(), 1);
    assert!(frame.slots[0].dmabuf.is_some());

    let mut icons = IconRenderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
    let plan = DmabufImportPlan {
        fourcc: fourcc::ARGB8888,
        modifier: fourcc::MOD_LINEAR,
        main_device: 0,
        scanout_preferred: false,
    };
    icons.set_dmabuf_import_state(true, Some(plan));
    let _ = icons.upload(&device, &queue, &mut frame);

    let dmabuf_n = icons.icon_dmabuf_import_count();
    if dmabuf_n == 1 {
        eprintln!("icon dmabuf import path exercised successfully");
    } else {
        eprintln!("skip: driver rejected linear udmabuf without CPU fallback");
    }
    assert!(frame.slots[0].dmabuf.is_none(), "plane consumed at upload");

    match crate::ui::render::dmabuf::create_exportable_dmabuf_texture(
        &device,
        plan,
        32,
        32,
        Some("gbm-wgpu-export-smoke"),
    ) {
        Ok(exported) => {
            assert_eq!(exported.texture.width(), 32);
            assert_eq!(exported.texture.height(), 32);
            assert!(exported.plane.stride >= 32 * 4);
            eprintln!("GBM allocation -> wgpu 30 dma-buf import exercised successfully");
        }
        Err(error) => eprintln!("skip: GBM/wgpu import unavailable: {error}"),
    }
}

#[test]
fn gpu_icon_sources_and_preview_composite_without_readback() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        display: None,
    });
    let adapter = match futures_lite::future::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(adapter) => adapter,
        Err(_) => {
            eprintln!("skip: no Vulkan adapter");
            return;
        }
    };
    let (device, queue) = match futures_lite::future::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("gpu-icon-source-smoke"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    })) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("skip: request_device failed: {error}");
            return;
        }
    };
    let Some(mut renderer) = GpuIconSourceRenderer::new(&device, &queue) else {
        eprintln!("skip: GPU icon renderer unavailable");
        return;
    };

    let root = std::env::temp_dir().join(format!(
        "fika-gpu-source-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let svg = root.join("source.svg");
    fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24"><rect width="32" height="24" rx="4" fill="#47a3ff"/></svg>"##,
    )
    .unwrap();
    let png = root.join("source.png");
    image::RgbaImage::from_pixel(24, 32, image::Rgba([255, 96, 32, 255]))
        .save(&png)
        .unwrap();

    let svg_source = IconGpuSource::file(svg.clone(), 64);
    let target = create_icon_texture(&device, 64, 64);
    assert!(renderer.render(&device, &queue, &target, &svg_source));

    let themed_svg = root.join("themed-use-gradient.svg");
    fs::write(
        &themed_svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 64 64">
          <defs>
            <style>.ColorScheme-Accent { color: #3daee9; }</style>
            <linearGradient id="fade" x1="4" y1="4" x2="60" y2="60" gradientUnits="userSpaceOnUse">
              <stop stop-color="#ffffff"/><stop offset="1" stop-color="#000000" stop-opacity=".25"/>
            </linearGradient>
            <path id="page" d="M4 4h42l14 14v42H4z"/>
          </defs>
          <use xlink:href="#page" fill="currentColor" class="ColorScheme-Accent"/>
          <use href="#page" fill="url(#fade)" opacity=".35"/>
        </svg>"##,
    )
    .unwrap();
    let themed_target = create_icon_texture(&device, 64, 64);
    assert!(renderer.render(
        &device,
        &queue,
        &themed_target,
        &IconGpuSource::file(themed_svg, 64),
    ));
    for theme_svg in [
        "/usr/share/icons/breeze/places/64/folder.svg",
        "/usr/share/icons/breeze/mimetypes/64/text-x-generic.svg",
        "/usr/share/icons/breeze/mimetypes/64/image-x-generic.svg",
        "/usr/share/icons/Papirus/64x64/places/folder.svg",
        "/usr/share/icons/Papirus/64x64/mimetypes/text-x-generic.svg",
        "/usr/share/icons/Papirus/64x64/mimetypes/image-x-generic.svg",
    ] {
        let path = PathBuf::from(theme_svg);
        if path.is_file() {
            let target = create_icon_texture(&device, 64, 64);
            assert!(
                renderer.render(&device, &queue, &target, &IconGpuSource::file(path, 64)),
                "wgpu SVG renderer rejected {theme_svg}",
            );
        }
    }

    let folder_source = IconGpuSource::FolderPreview {
        children: vec![svg.clone(), png.clone()].into(),
        size_px: 64,
        seed: 7,
    };
    let folder_target = create_icon_texture(&device, 64, 64);
    assert!(renderer.render(&device, &queue, &folder_target, &folder_source));

    let preview_target = create_icon_texture(&device, 144, 80);
    let mut text_renderer = TextRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let preview_label = rasterize_gpu_drag_preview_label(
        &mut text_renderer,
        ViewRect {
            x: 8.0,
            y: 64.0,
            width: 120.0,
            height: 14.0,
        },
        "2 items",
        [240, 244, 255, 255],
    );
    let preview = GpuDragPreview {
        width: 144,
        height: 80,
        background: Some((
            ViewRect {
                x: 2.0,
                y: 2.0,
                width: 140.0,
                height: 76.0,
            },
            10.0,
            [25, 32, 48, 220],
        )),
        draws: vec![
            GpuDragPreviewDraw {
                source: svg_source,
                rect: ViewRect {
                    x: 8.0,
                    y: 8.0,
                    width: 56.0,
                    height: 56.0,
                },
            },
            GpuDragPreviewDraw {
                source: folder_source,
                rect: ViewRect {
                    x: 72.0,
                    y: 8.0,
                    width: 56.0,
                    height: 56.0,
                },
            },
        ],
        label: preview_label,
    };
    assert!(renderer.render_drag_preview(&device, &queue, &preview_target, &preview));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(2)),
        })
        .unwrap();
    drop(renderer);
    let _ = fs::remove_dir_all(root);
}
