//! Device ranking unit tests (Smithay-free).

use super::*;

#[test]
fn major_minor_match_linux_dev_t_encoding() {
    // major 226 minor 0 → typical /dev/dri/card0 style rdev
    let card0 = 226u64 << 8;
    assert_eq!(super::major_dev(card0), 226);
    assert_eq!(super::minor_dev(card0), 0);
    let card1 = (226u64 << 8) | 1;
    assert_eq!(super::major_dev(card1), 226);
    assert_eq!(super::minor_dev(card1), 1);
}

fn required_interop() -> NativeInteropCapabilities {
    NativeInteropCapabilities {
        external_memory_fd: true,
        dma_buf_memory: true,
        drm_format_modifier: true,
        foreign_queue_family: true,
        external_semaphore_fd: true,
        sync_fd_semaphore: true,
    }
}

fn candidate(ordinal: usize, device_type: DeviceType, heap: bool) -> DeviceCandidate {
    DeviceCandidate {
        ordinal,
        name: format!("device-{ordinal}"),
        device_type,
        api_version: ROADMAP_2026_API_VERSION,
        descriptor_heap_supported: heap,
        descriptor_heap: DescriptorHeapProperties {
            sampler_heap_alignment: 32,
            resource_heap_alignment: 32,
            max_sampler_heap_size: 4096,
            max_resource_heap_size: 16 * 1024 * 1024,
            min_sampler_heap_reserved_range: 0,
            min_sampler_heap_reserved_range_with_embedded: 32,
            min_resource_heap_reserved_range: 0,
            sampler_descriptor_size: 32,
            buffer_descriptor_alignment: 32,
            image_descriptor_size: 32,
            sampler_descriptor_alignment: 32,
            image_descriptor_alignment: 32,
            max_push_data_size: 128,
            max_descriptor_heap_embedded_samplers: 8,
        },
        buffer_device_address_supported: true,
        timeline_semaphore_supported: true,
        dynamic_rendering_supported: true,
        maintenance5_supported: true,
        graphics_queue_family: Some(0),
        drm: Some(DrmDeviceIdentity::new(
            Some(DrmNodeId::new(226, ordinal as u32)),
            Some(DrmNodeId::new(226, 128 + ordinal as u32)),
        )),
        interop: required_interop(),
        native_output_format_count: 1,
    }
}

#[test]
fn default_prefers_discrete_gpu_with_heap() {
    let candidates = [
        candidate(0, DeviceType::Cpu, true),
        candidate(1, DeviceType::Discrete, true),
        candidate(2, DeviceType::Integrated, true),
    ];

    assert_eq!(
        DeviceSelector::new(GpuPreference::Discrete)
            .select(&candidates)
            .unwrap()
            .ordinal,
        1
    );
}

#[test]
fn unsupported_heap_devices_are_never_selected() {
    let candidates = [
        candidate(0, DeviceType::Discrete, false),
        candidate(1, DeviceType::Cpu, false),
    ];

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select(&candidates),
        Err(DeviceSelectionError::MissingDescriptorHeap)
    ));
}

#[test]
fn timeline_semaphores_are_required_for_native_frame_scheduling() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.timeline_semaphore_supported = false;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::MissingTimelineSemaphore)
    ));
}

#[test]
fn buffer_device_address_is_required_for_descriptor_heap_binding() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.buffer_device_address_supported = false;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::MissingBufferDeviceAddress)
    ));
}

#[test]
fn maintenance5_is_required_for_descriptor_heap_pipelines() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.maintenance5_supported = false;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::MissingMaintenance5)
    ));
}

#[test]
fn dynamic_rendering_is_required_for_client_image_pipelines() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.dynamic_rendering_supported = false;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::MissingDynamicRendering)
    ));
}

#[test]
fn unusable_descriptor_heap_limits_are_rejected() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.descriptor_heap.max_resource_heap_size = 0;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
    ));
}

#[test]
fn descriptor_heap_draw_push_and_embedded_sampler_limits_are_required() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.descriptor_heap.max_push_data_size = 32;
    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
    ));

    candidate.descriptor_heap.max_push_data_size = 128;
    candidate
        .descriptor_heap
        .max_descriptor_heap_embedded_samplers = 0;
    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
    ));

    candidate
        .descriptor_heap
        .max_descriptor_heap_embedded_samplers = 8;
    candidate
        .descriptor_heap
        .min_sampler_heap_reserved_range_with_embedded = 33;
    candidate.descriptor_heap.max_sampler_heap_size = 40;
    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
    ));
}

#[test]
fn older_vulkan_devices_are_rejected_before_ranking() {
    let mut discrete = candidate(0, DeviceType::Discrete, true);
    discrete.api_version = ApiVersion::V1_3_0;
    let integrated = candidate(1, DeviceType::Integrated, true);
    let candidates = [discrete, integrated];

    assert_eq!(
        DeviceSelector::new(GpuPreference::Discrete)
            .select(&candidates)
            .unwrap()
            .ordinal,
        1
    );
}

#[test]
fn reports_when_all_descriptor_heap_devices_are_too_old() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.api_version = ApiVersion::V1_3_0;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Discrete).select([&candidate]),
        Err(DeviceSelectionError::VulkanTooOld)
    ));
}

#[test]
fn graphics_queue_is_a_required_renderer_capability() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.graphics_queue_family = None;

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Any).select([&candidate]),
        Err(DeviceSelectionError::MissingGraphicsQueue)
    ));
}

#[test]
fn configured_drm_node_overrides_gpu_type_ranking() {
    let discrete = candidate(0, DeviceType::Discrete, true);
    let integrated = candidate(1, DeviceType::Integrated, true);
    let requested = integrated.drm.unwrap().render.unwrap();

    let selected = DeviceSelector::new(GpuPreference::Discrete)
        .with_drm_node(Some(requested))
        .select([&discrete, &integrated])
        .unwrap();

    assert_eq!(selected.ordinal, integrated.ordinal);
}

#[test]
fn rejects_a_drm_node_without_a_vulkan_device() {
    let candidate = candidate(0, DeviceType::Discrete, true);
    let requested = DrmNodeId::new(226, 191);

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Discrete)
            .with_drm_node(Some(requested))
            .select([&candidate]),
        Err(DeviceSelectionError::DrmNodeNotFound(node)) if node == requested
    ));
}

#[test]
fn drm_primary_and_render_nodes_are_both_required() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.drm = Some(DrmDeviceIdentity::new(Some(DrmNodeId::new(226, 0)), None));

    assert!(matches!(
        DeviceSelector::new(GpuPreference::Discrete).select([&candidate]),
        Err(DeviceSelectionError::MissingDrmNodePair)
    ));
}

#[test]
fn configured_node_path_must_be_a_character_device() {
    assert!(matches!(
        DrmNodeId::from_path(Path::new("Cargo.toml")),
        Err(DrmNodeError::NotCharacterDevice(_))
    ));
}

#[test]
fn missing_configured_node_is_reported_at_selection_boundary() {
    assert!(matches!(
        DrmNodeId::from_path(Path::new("/definitely/missing/tensor-drm-node")),
        Err(DrmNodeError::Read { .. })
    ));
}

#[test]
fn reports_the_first_missing_native_interop_capability() {
    let required = required_interop();
    let cases = [
        (
            NativeInteropCapabilities {
                external_memory_fd: false,
                ..required
            },
            DeviceSelectionError::MissingExternalMemoryFd,
        ),
        (
            NativeInteropCapabilities {
                dma_buf_memory: false,
                ..required
            },
            DeviceSelectionError::MissingDmaBufMemory,
        ),
        (
            NativeInteropCapabilities {
                drm_format_modifier: false,
                ..required
            },
            DeviceSelectionError::MissingDrmFormatModifier,
        ),
        (
            NativeInteropCapabilities {
                foreign_queue_family: false,
                ..required
            },
            DeviceSelectionError::MissingForeignQueueFamily,
        ),
        (
            NativeInteropCapabilities {
                external_semaphore_fd: false,
                ..required
            },
            DeviceSelectionError::MissingExternalSemaphoreFd,
        ),
        (
            NativeInteropCapabilities {
                sync_fd_semaphore: false,
                ..required
            },
            DeviceSelectionError::MissingSyncFdSemaphore,
        ),
    ];

    for (interop, expected) in cases {
        let mut candidate = candidate(0, DeviceType::Discrete, true);
        candidate.interop = interop;
        let error = DeviceSelector::new(GpuPreference::Discrete)
            .select([&candidate])
            .unwrap_err();
        assert_eq!(error, expected);
    }
}

#[test]
fn device_without_an_exportable_output_format_is_not_selected() {
    let mut candidate = candidate(0, DeviceType::Discrete, true);
    candidate.native_output_format_count = 0;

    assert_eq!(
        DeviceSelector::new(GpuPreference::Discrete)
            .select([&candidate])
            .unwrap_err(),
        DeviceSelectionError::MissingNativeOutputFormat
    );
}
