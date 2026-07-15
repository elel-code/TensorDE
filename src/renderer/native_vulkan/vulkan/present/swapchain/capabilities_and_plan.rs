pub(in crate::renderer::native_vulkan::vulkan) fn query_vulkanalia_present_feature_selection(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    _surface_maintenance1_enabled: bool,
) -> NativeVulkanVulkanaliaPresentFeatureSelection {
    let (mut core_features, vulkan_1_4_properties, descriptor_heap_properties) =
        native_vulkan_vulkanalia_core_feature_snapshot(instance, physical_device);
    if !extension_available(device_extensions, DESCRIPTOR_HEAP_EXTENSION_NAME) {
        core_features.descriptor_heap = false;
        core_features.descriptor_heap_capture_replay = false;
    }
    let synchronization2_enabled = core_features.synchronization2;
    let dynamic_rendering_enabled = core_features.dynamic_rendering;
    let present_id2_enabled = extension_available(device_extensions, PRESENT_ID2_EXTENSION_NAME)
        && query_present_id2_feature(instance, physical_device);
    let present_wait2_enabled = present_id2_enabled
        && extension_available(device_extensions, PRESENT_WAIT2_EXTENSION_NAME)
        && query_present_wait2_feature(instance, physical_device);
    let swapchain_maintenance1_enabled =
        extension_available(device_extensions, SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME)
            && query_swapchain_maintenance1_feature(instance, physical_device);
    let present_mode_fifo_latest_ready_enabled =
        extension_available(
            device_extensions,
            PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
        ) && query_present_mode_fifo_latest_ready_feature(instance, physical_device);
    let blend_operation_advanced_enabled =
        extension_available(device_extensions, BLEND_OPERATION_ADVANCED_EXTENSION_NAME);
    let blend_operation_advanced_coherent_operations = blend_operation_advanced_enabled
        && query_blend_operation_advanced_coherent_operations(instance, physical_device);
    let scene_color_msaa_request = std::env::var("GILDER_NATIVE_VULKAN_SCENE_MSAA").ok();
    let scene_color_4x_msaa_enabled =
        scene_color_4x_msaa_requested(scene_color_msaa_request.as_deref())
        && unsafe { instance.get_physical_device_properties(physical_device) }
            .limits
            .framebuffer_color_sample_counts
            .contains(vk::SampleCountFlags::_4);
    let multisampled_render_to_single_sampled_enabled = scene_color_4x_msaa_enabled
        && extension_available(
            device_extensions,
            MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED_EXTENSION_NAME,
        )
        && query_multisampled_render_to_single_sampled_feature(instance, physical_device);
    let maintenance7_enabled = extension_available(device_extensions, MAINTENANCE7_EXTENSION_NAME)
        && query_maintenance7_feature(instance, physical_device);
    let maintenance8_enabled = extension_available(device_extensions, MAINTENANCE8_EXTENSION_NAME)
        && query_maintenance8_feature(instance, physical_device);
    let maintenance9_enabled = extension_available(device_extensions, MAINTENANCE9_EXTENSION_NAME)
        && query_maintenance9_feature(instance, physical_device);
    let maintenance10_enabled =
        extension_available(device_extensions, MAINTENANCE10_EXTENSION_NAME)
            && query_maintenance10_feature(instance, physical_device);

    NativeVulkanVulkanaliaPresentFeatureSelection {
        core_features,
        vulkan_1_4_properties,
        descriptor_heap_properties,
        synchronization2_enabled,
        dynamic_rendering_enabled,
        present_id2_enabled,
        present_wait2_enabled,
        swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_enabled,
        blend_operation_advanced_enabled,
        blend_operation_advanced_coherent_operations,
        multisampled_render_to_single_sampled_enabled,
        scene_color_4x_msaa_enabled,
        maintenance7_enabled,
        maintenance8_enabled,
        maintenance9_enabled,
        maintenance10_enabled,
    }
}

fn scene_color_4x_msaa_requested(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "4" | "4x"))
}

pub(in crate::renderer::native_vulkan::vulkan) fn vulkanalia_surface_maintenance1_enabled(
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
) -> bool {
    vulkan
        .extension_selection
        .enabled_instance_extensions
        .contains(&SURFACE_MAINTENANCE1_EXTENSION_NAME)
}

pub(in crate::renderer::native_vulkan::vulkan) fn enabled_present_device_extensions(
    feature_selection: &NativeVulkanVulkanaliaPresentFeatureSelection,
) -> Vec<&'static str> {
    let mut extensions = vec!["VK_KHR_swapchain"];
    if feature_selection.present_id2_enabled {
        extensions.push(PRESENT_ID2_EXTENSION_NAME);
    }
    if feature_selection.present_wait2_enabled {
        extensions.push(PRESENT_WAIT2_EXTENSION_NAME);
    }
    if feature_selection.swapchain_maintenance1_enabled {
        extensions.push(SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME);
    }
    if feature_selection.present_mode_fifo_latest_ready_enabled {
        extensions.push(PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME);
    }
    if feature_selection.blend_operation_advanced_enabled {
        extensions.push(BLEND_OPERATION_ADVANCED_EXTENSION_NAME);
    }
    if feature_selection.multisampled_render_to_single_sampled_enabled {
        extensions.push(MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED_EXTENSION_NAME);
    }
    if feature_selection.core_features.descriptor_heap {
        extensions.push(DESCRIPTOR_HEAP_EXTENSION_NAME);
    }
    if feature_selection.maintenance7_enabled {
        extensions.push(MAINTENANCE7_EXTENSION_NAME);
    }
    if feature_selection.maintenance8_enabled {
        extensions.push(MAINTENANCE8_EXTENSION_NAME);
    }
    if feature_selection.maintenance9_enabled {
        extensions.push(MAINTENANCE9_EXTENSION_NAME);
    }
    if feature_selection.maintenance10_enabled {
        extensions.push(MAINTENANCE10_EXTENSION_NAME);
    }
    extensions
}

pub(in crate::renderer::native_vulkan::vulkan) fn vulkanalia_surface_capabilities2_enabled(
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
) -> bool {
    vulkan
        .extension_selection
        .enabled_instance_extensions
        .contains(&GET_SURFACE_CAPABILITIES2_EXTENSION_NAME)
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_vulkanalia_swapchain_plan(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    buffer_size: (u32, u32),
    surface_capabilities2_enabled: bool,
    feature_selection: &NativeVulkanVulkanaliaPresentFeatureSelection,
) -> Result<NativeVulkanVulkanaliaSwapchainPlan, String> {
    let capabilities =
        unsafe { instance.get_physical_device_surface_capabilities_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceCapabilitiesKHR(vulkanalia): {err:?}")
            })?;
    let present_timing_capabilities = query_surface_present_timing_capabilities(
        instance,
        physical_device,
        surface,
        surface_capabilities2_enabled,
    )?;
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err("Vulkanalia swapchain surface does not support TRANSFER_DST".to_owned());
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_SRC)
    {
        return Err("Vulkanalia swapchain surface does not support TRANSFER_SRC".to_owned());
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err("Vulkanalia swapchain surface does not support COLOR_ATTACHMENT".to_owned());
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::SAMPLED)
    {
        return Err("Vulkanalia swapchain surface does not support SAMPLED".to_owned());
    }
    let formats =
        unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface) }
            .map_err(|err| format!("vkGetPhysicalDeviceSurfaceFormatsKHR(vulkanalia): {err:?}"))?;
    let format = choose_surface_format(&formats)?;
    let present_modes =
        unsafe { instance.get_physical_device_surface_present_modes_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfacePresentModesKHR(vulkanalia): {err:?}")
            })?;
    let present_mode = choose_present_mode(
        &present_modes,
        feature_selection.present_mode_fifo_latest_ready_enabled,
    )?;
    let (extent, extent_selection) = choose_swapchain_extent(&capabilities, buffer_size)?;
    let image_count = swapchain_image_count(&capabilities);
    let composite_alpha = choose_composite_alpha(capabilities.supported_composite_alpha);
    let present_id2_enabled =
        feature_selection.present_id2_enabled && present_timing_capabilities.present_id2_supported;
    let present_wait2_enabled = feature_selection.present_wait2_enabled
        && present_id2_enabled
        && present_timing_capabilities.present_wait2_supported;
    if !present_id2_enabled {
        return Err(
            "Vulkanalia swapchain requires VK_KHR_present_id2 feature and surface support"
                .to_owned(),
        );
    }
    if !present_wait2_enabled {
        return Err(
            "Vulkanalia swapchain requires VK_KHR_present_wait2 feature and surface support"
                .to_owned(),
        );
    }
    let create_flags = swapchain_create_flags(present_id2_enabled, present_wait2_enabled);
    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .flags(create_flags)
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(
            vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
        )
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true)
        .build();

    Ok(NativeVulkanVulkanaliaSwapchainPlan {
        create_info,
        format,
        present_mode,
        extent,
        extent_selection,
        image_count,
        composite_alpha,
        create_flags,
        surface_present_id2_supported: present_timing_capabilities.present_id2_supported,
        surface_present_wait2_supported: present_timing_capabilities.present_wait2_supported,
        present_id2_enabled,
        present_wait2_enabled,
    })
}

fn surface_snapshot_from_plan(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    _swapchain_plan: &NativeVulkanVulkanaliaSwapchainPlan,
) -> Result<NativeVulkanVulkanaliaSurfaceSnapshot, String> {
    let capabilities =
        unsafe { instance.get_physical_device_surface_capabilities_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceCapabilitiesKHR(vulkanalia snapshot): {err:?}")
            })?;
    let formats =
        unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceFormatsKHR(vulkanalia snapshot): {err:?}")
            })?;
    let present_modes =
        unsafe { instance.get_physical_device_surface_present_modes_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfacePresentModesKHR(vulkanalia snapshot): {err:?}")
            })?;

    Ok(NativeVulkanVulkanaliaSurfaceSnapshot {
        capabilities: NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot {
            min_image_count: capabilities.min_image_count,
            max_image_count: capabilities.max_image_count,
            current_extent: extent_tuple(capabilities.current_extent),
            min_image_extent: (
                capabilities.min_image_extent.width,
                capabilities.min_image_extent.height,
            ),
            max_image_extent: (
                capabilities.max_image_extent.width,
                capabilities.max_image_extent.height,
            ),
            supports_transfer_src: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::TRANSFER_SRC),
            supports_transfer_dst: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::TRANSFER_DST),
            supports_color_attachment: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT),
            supports_sampled: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::SAMPLED),
            present_id2_supported: _swapchain_plan.surface_present_id2_supported,
            present_wait2_supported: _swapchain_plan.surface_present_wait2_supported,
        },
        surface_format_count: formats.len(),
        surface_formats: formats
            .into_iter()
            .map(|format| NativeVulkanVulkanaliaSurfaceFormatSnapshot {
                format: format!("{:?}", format.format),
                color_space: format!("{:?}", format.color_space),
            })
            .collect(),
        present_mode_count: present_modes.len(),
        present_modes: present_modes.into_iter().map(present_mode_label).collect(),
    })
}

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> Result<vk::SurfaceFormatKHR, String> {
    formats
        .iter()
        .copied()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_UNORM
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| {
            formats.iter().copied().find(|format| {
                format.format == vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
        })
        .or_else(|| formats.first().copied())
        .ok_or_else(|| "Vulkanalia surface reported no surface formats".to_owned())
}

fn choose_present_mode(
    present_modes: &[vk::PresentModeKHR],
    present_mode_fifo_latest_ready_enabled: bool,
) -> Result<vk::PresentModeKHR, String> {
    if !present_mode_fifo_latest_ready_enabled {
        return Err(
            "Vulkanalia present requires VK_KHR_present_mode_fifo_latest_ready; mailbox/immediate fallback is forbidden"
                .to_owned(),
        );
    }
    if present_mode_fifo_latest_ready_enabled
        && present_modes.contains(&vk::PresentModeKHR::FIFO_LATEST_READY)
    {
        return Ok(vk::PresentModeKHR::FIFO_LATEST_READY);
    }
    Err(
        "Vulkanalia present requires VK_PRESENT_MODE_FIFO_LATEST_READY_KHR; FIFO/mailbox/immediate fallback is forbidden"
            .to_owned(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SurfacePresentTimingCapabilities {
    present_id2_supported: bool,
    present_wait2_supported: bool,
}

fn query_surface_present_timing_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_capabilities2_enabled: bool,
) -> Result<SurfacePresentTimingCapabilities, String> {
    if !surface_capabilities2_enabled {
        return Ok(SurfacePresentTimingCapabilities {
            present_id2_supported: false,
            present_wait2_supported: false,
        });
    }

    let surface_info = vk::PhysicalDeviceSurfaceInfo2KHR::builder()
        .surface(surface)
        .build();
    let mut present_id2 = vk::SurfaceCapabilitiesPresentId2KHR::default();
    let mut present_wait2 = vk::SurfaceCapabilitiesPresentWait2KHR::default();
    let mut capabilities2 = vk::SurfaceCapabilities2KHR::builder()
        .push_next(&mut present_id2)
        .push_next(&mut present_wait2)
        .build();
    unsafe {
        instance.get_physical_device_surface_capabilities2_khr(
            physical_device,
            &surface_info,
            &mut capabilities2,
        )
    }
    .map_err(|err| {
        format!("vkGetPhysicalDeviceSurfaceCapabilities2KHR(vulkanalia present timing): {err:?}")
    })?;

    Ok(SurfacePresentTimingCapabilities {
        present_id2_supported: present_id2.present_id2_supported != 0,
        present_wait2_supported: present_wait2.present_wait2_supported != 0,
    })
}

fn swapchain_create_flags(
    present_id2_enabled: bool,
    present_wait2_enabled: bool,
) -> vk::SwapchainCreateFlagsKHR {
    let mut flags = vk::SwapchainCreateFlagsKHR::empty();
    if present_id2_enabled {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_ID_2;
    }
    if present_wait2_enabled {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2;
    }
    flags
}

fn choose_swapchain_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    buffer_size: (u32, u32),
) -> Result<
    (
        vk::Extent2D,
        NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot,
    ),
    String,
> {
    let surface_current_extent = extent_tuple(capabilities.current_extent);
    let surface_min_image_extent = (
        capabilities.min_image_extent.width,
        capabilities.min_image_extent.height,
    );
    let surface_max_image_extent = (
        capabilities.max_image_extent.width,
        capabilities.max_image_extent.height,
    );
    if let Some((width, height)) = surface_current_extent {
        return Ok((
            vk::Extent2D { width, height },
            NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot {
                source: "surface-current-extent",
                requested_wayland_buffer_size: buffer_size,
                surface_current_extent,
                surface_min_image_extent,
                surface_max_image_extent,
            },
        ));
    }
    let width = buffer_size.0.clamp(
        capabilities.min_image_extent.width,
        capabilities.max_image_extent.width,
    );
    let height = buffer_size.1.clamp(
        capabilities.min_image_extent.height,
        capabilities.max_image_extent.height,
    );
    if width == 0 || height == 0 {
        return Err("Vulkanalia swapchain extent resolved to zero".to_owned());
    }
    Ok((
        vk::Extent2D { width, height },
        NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot {
            source: "wayland-buffer-size-clamped-to-surface-capabilities",
            requested_wayland_buffer_size: buffer_size,
            surface_current_extent,
            surface_min_image_extent,
            surface_max_image_extent,
        },
    ))
}

fn swapchain_image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let required = capabilities.min_image_count.max(1).saturating_add(2);
    if capabilities.max_image_count > 0 {
        required.min(capabilities.max_image_count)
    } else {
        required
    }
}

fn choose_composite_alpha(flags: vk::CompositeAlphaFlagsKHR) -> vk::CompositeAlphaFlagsKHR {
    // WE's DirectComposition handoff uses DXGI_ALPHA_MODE_PREMULTIPLIED.
    // Reference: reverse-engineered/docs/exe/d3d11-context-calls.md.
    [
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
    ]
    .into_iter()
    .find(|flag| flags.contains(*flag))
    .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE)
}

fn query_present_id2_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDevicePresentId2FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_id2 != 0
}

fn query_present_wait2_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDevicePresentWait2FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_wait2 != 0
}

fn query_swapchain_maintenance1_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.swapchain_maintenance1 != 0
}

fn query_present_mode_fifo_latest_ready_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_mode_fifo_latest_ready != 0
}

fn query_blend_operation_advanced_coherent_operations(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceBlendOperationAdvancedFeaturesEXT::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.advanced_blend_coherent_operations != 0
}

fn query_multisampled_render_to_single_sampled_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceMultisampledRenderToSingleSampledFeaturesEXT::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.multisampled_render_to_single_sampled != 0
}

fn query_maintenance7_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDeviceMaintenance7FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.maintenance7 != 0
}

fn query_maintenance8_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDeviceMaintenance8FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.maintenance8 != 0
}

fn query_maintenance9_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDeviceMaintenance9FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.maintenance9 != 0
}

fn query_maintenance10_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDeviceMaintenance10FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.maintenance10 != 0
}

fn extension_available(available: &[String], extension: &str) -> bool {
    available.iter().any(|available| available == extension)
}

fn physical_device_name(properties: vk::PhysicalDeviceProperties) -> String {
    unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub(in crate::renderer::native_vulkan::vulkan) fn queue_flag_labels(
    flags: vk::QueueFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::QueueFlags::GRAPHICS) {
        labels.push("graphics");
    }
    if flags.contains(vk::QueueFlags::COMPUTE) {
        labels.push("compute");
    }
    if flags.contains(vk::QueueFlags::TRANSFER) {
        labels.push("transfer");
    }
    if flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
        labels.push("video-decode");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn present_mode_label(
    mode: vk::PresentModeKHR,
) -> &'static str {
    match mode {
        vk::PresentModeKHR::IMMEDIATE => "immediate",
        vk::PresentModeKHR::MAILBOX => "mailbox",
        vk::PresentModeKHR::FIFO => "fifo",
        vk::PresentModeKHR::FIFO_RELAXED => "fifo-relaxed",
        vk::PresentModeKHR::FIFO_LATEST_READY => "fifo-latest-ready",
        vk::PresentModeKHR::SHARED_DEMAND_REFRESH => "shared-demand-refresh",
        vk::PresentModeKHR::SHARED_CONTINUOUS_REFRESH => "shared-continuous-refresh",
        _ => "unknown",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn composite_alpha_label(
    flags: vk::CompositeAlphaFlagsKHR,
) -> &'static str {
    if flags == vk::CompositeAlphaFlagsKHR::OPAQUE {
        "opaque"
    } else if flags == vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED {
        "pre-multiplied"
    } else if flags == vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED {
        "post-multiplied"
    } else if flags == vk::CompositeAlphaFlagsKHR::INHERIT {
        "inherit"
    } else {
        "unknown"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn swapchain_create_flag_labels(
    flags: vk::SwapchainCreateFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::SwapchainCreateFlagsKHR::PRESENT_ID_2) {
        labels.push("present-id2");
    }
    if flags.contains(vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2) {
        labels.push("present-wait2");
    }
    labels
}

fn extent_tuple(extent: vk::Extent2D) -> Option<(u32, u32)> {
    if extent.width == u32::MAX || extent.height == u32::MAX {
        None
    } else {
        Some((extent.width, extent.height))
    }
}

#[cfg(test)]
mod tests;
