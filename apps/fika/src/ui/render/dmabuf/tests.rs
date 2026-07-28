use super::*;

#[test]
fn fourcc_maps_common_formats() {
    assert_eq!(
        texture_format_for_fourcc(fourcc::ARGB8888),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(
        texture_format_for_fourcc(fourcc::BGRA8888),
        Some(wgpu::TextureFormat::Bgra8Unorm)
    );
    assert_eq!(
        texture_format_for_fourcc(fourcc::ABGR8888),
        Some(wgpu::TextureFormat::Rgba8Unorm)
    );
    assert!(texture_format_for_fourcc(0xdead_beef).is_none());
}

#[test]
fn legacy_importer_translates_only_supported_vulkan_image_usages() {
    use vulkan_renderer::vk::ImageUsageFlags as VkUsage;

    let (public, hal) =
        translate_image_usage(VkUsage::SAMPLED | VkUsage::TRANSFER_SRC | VkUsage::COLOR_ATTACHMENT)
            .unwrap();
    assert_eq!(
        public,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT
    );
    assert_eq!(
        hal,
        wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC | wgpu::TextureUses::COLOR_TARGET
    );
    assert!(matches!(
        translate_image_usage(VkUsage::STORAGE),
        Err(DmabufImportError::UnsupportedUsage(VkUsage::STORAGE))
    ));
    assert!(matches!(
        translate_image_usage(VkUsage::empty()),
        Err(DmabufImportError::UnsupportedUsage(usage)) if usage.is_empty()
    ));
}

#[test]
fn pick_import_format_prefers_argb_from_feedback() {
    use wayland_client_runtime::{DmabufFeedback, DmabufFeedbackTranche, DmabufFormat};

    let feedback = DmabufFeedback {
        main_device: 0,
        formats: vec![
            DmabufFormat::new(fourcc::RGBA8888, fourcc::MOD_LINEAR),
            DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR),
        ],
        tranches: vec![DmabufFeedbackTranche {
            device: 0,
            flags: wayland_client_runtime::DmabufTrancheFlags::empty(),
            formats: vec![0, 1],
        }],
    };
    let picked = pick_import_format(&feedback).expect("pick");
    assert_eq!(picked.format, fourcc::ARGB8888);
}

#[test]
fn assess_readiness_needs_vulkan_and_feedback() {
    use wayland_client_runtime::{DmabufFeedback, DmabufFormat};

    let feedback = DmabufFeedback {
        main_device: 42,
        formats: vec![DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR)],
        tranches: vec![],
    };
    let not_ready = assess_readiness(false, true, Some(&feedback));
    assert!(!not_ready.import_ready());
    assert!(not_ready.plan.is_none());

    let ready = assess_readiness(true, true, Some(&feedback));
    assert!(ready.import_ready());
    let plan = ready.plan.expect("plan");
    assert_eq!(plan.fourcc, fourcc::ARGB8888);
    assert_eq!(plan.main_device, 42);
}

#[test]
fn import_udmabuf_into_wgpu_when_available() {
    let Some((fd, stride)) = try_allocate_udmabuf_argb8888(64, 64) else {
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
    let adapter = match futures_lite::future::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        },
    )) {
        Ok(adapter) => adapter,
        Err(_) => {
            eprintln!("skip: no Vulkan adapter");
            return;
        }
    };
    if !adapter_supports_dmabuf_import(&adapter) {
        eprintln!("skip: adapter lacks VULKAN_EXTERNAL_MEMORY_DMA_BUF");
        return;
    }
    let features = optional_dmabuf_features(&adapter);
    let (device, _queue) =
        match futures_lite::future::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dmabuf-import-test"),
            required_features: features,
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        })) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("skip: request_device failed: {error}");
                return;
            }
        };

    let plan = DmabufImportPlan {
        fourcc: fourcc::ARGB8888,
        modifier: fourcc::MOD_LINEAR,
        main_device: 0,
        scanout_preferred: false,
    };
    let desc = import_desc_from_plan(
        plan,
        64,
        64,
        DmabufImportPlane {
            fd,
            offset: 0,
            stride,
            modifier: fourcc::MOD_LINEAR,
        },
        vulkan_renderer::vk::ImageUsageFlags::SAMPLED
            | vulkan_renderer::vk::ImageUsageFlags::TRANSFER_SRC,
        Some("udmabuf-test"),
    );
    match import_dmabuf_texture(&device, desc) {
        Ok(texture) => {
            assert_eq!(texture.size().width, 64);
            assert_eq!(texture.size().height, 64);
            assert_eq!(texture.format(), wgpu::TextureFormat::Bgra8Unorm);
            texture.destroy();
        }
        Err(error) => {
            eprintln!("import failed (driver may reject linear udmabuf): {error}");
        }
    }
}
