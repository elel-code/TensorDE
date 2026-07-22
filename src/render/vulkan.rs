#![allow(unsafe_code)]

use thiserror::Error;
use tracing::{debug, info};
use vulkanalia::{
    Device, Entry, Instance, Version,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
};

use super::{DeviceCandidate, DeviceSelectionError, DrmDeviceIdentity, DrmNodeId, RendererTarget};

pub(crate) struct VulkanRenderer {
    _owner: VulkanOwner,
    target: RendererTarget,
    selected: SelectedDevice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedDevice {
    pub(crate) name: String,
    pub(crate) api_version: Version,
    pub(crate) device_type: vk::PhysicalDeviceType,
    pub(crate) graphics_queue_family: u32,
    pub(crate) primary_node: DrmNodeId,
    pub(crate) render_node: DrmNodeId,
}

impl VulkanRenderer {
    pub(crate) fn new(target: RendererTarget) -> Result<Self, RendererError> {
        let entry = load_entry()?;
        let loader_version = entry.version().map_err(RendererError::LoaderVersion)?;
        if loader_version < target.api_version {
            return Err(RendererError::UnsupportedLoaderVersion {
                required: target.api_version,
                found: loader_version,
            });
        }

        let application = vk::ApplicationInfo::builder()
            .application_name(b"tensor-compositor\0")
            .engine_name(b"tensor-renderer\0")
            .api_version(target.api_version.into());
        let instance_info = vk::InstanceCreateInfo::builder().application_info(&application);
        let instance = unsafe { entry.create_instance(&instance_info, None) }
            .map_err(RendererError::CreateInstance)?;
        let instance = InstanceOwner { entry, instance };

        let probed = probe_devices(&instance.instance)?;
        for device in &probed {
            debug!(
                ordinal = device.candidate.ordinal,
                name = device.candidate.name,
                api = %device.candidate.api_version,
                device_type = ?device.candidate.device_type,
                descriptor_heap = device.candidate.descriptor_heap_supported,
                graphics_queue_family = ?device.candidate.graphics_queue_family,
                "Vulkan physical device probed"
            );
        }
        let selected = target
            .device
            .select(probed.iter().map(|device| &device.candidate))?;
        let selected = &probed[selected.ordinal];
        let graphics_queue_family = selected
            .candidate
            .graphics_queue_family
            .ok_or(DeviceSelectionError::MissingGraphicsQueue)?;
        let device = create_device(&instance.instance, selected.handle, graphics_queue_family)?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };
        let (primary_node, render_node) = selected
            .candidate
            .drm
            .and_then(DrmDeviceIdentity::node_pair)
            .ok_or(DeviceSelectionError::MissingDrmNodePair)?;
        let selected_info = SelectedDevice {
            name: selected.candidate.name.clone(),
            api_version: selected.candidate.api_version,
            device_type: selected.candidate.device_type,
            graphics_queue_family,
            primary_node,
            render_node,
        };
        info!(
            name = selected_info.name,
            api = %selected_info.api_version,
            device_type = ?selected_info.device_type,
            graphics_queue_family,
            primary_node = %selected_info.primary_node,
            render_node = %selected_info.render_node,
            descriptor_heap = true,
            "Vulkanalia renderer device initialized"
        );

        Ok(Self {
            _owner: VulkanOwner {
                device,
                instance,
                _physical_device: selected.handle,
                _graphics_queue: graphics_queue,
            },
            target,
            selected: selected_info,
        })
    }

    pub(crate) const fn target(&self) -> RendererTarget {
        self.target
    }

    pub(crate) fn selected(&self) -> &SelectedDevice {
        &self.selected
    }
}

struct ProbedDevice {
    handle: vk::PhysicalDevice,
    candidate: DeviceCandidate,
}

struct InstanceOwner {
    entry: Entry,
    instance: Instance,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}

struct VulkanOwner {
    device: Device,
    instance: InstanceOwner,
    _physical_device: vk::PhysicalDevice,
    _graphics_queue: vk::Queue,
}

impl Drop for VulkanOwner {
    fn drop(&mut self) {
        unsafe { self.device.destroy_device(None) };
        let _ = &self.instance;
    }
}

fn load_entry() -> Result<Entry, RendererError> {
    // Vulkanalia deliberately exposes loader and dispatch construction as unsafe. The library
    // path is its platform constant and both owners outlive every command loaded from it.
    let loader = unsafe { LibloadingLoader::new(LIBRARY) }
        .map_err(|error| RendererError::LoadLibrary(error.to_string()))?;
    unsafe { Entry::new(loader) }.map_err(|error| RendererError::LoadEntry(error.to_string()))
}

fn probe_devices(instance: &Instance) -> Result<Vec<ProbedDevice>, RendererError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(RendererError::EnumerateDevices)?;
    let mut candidates = Vec::with_capacity(physical_devices.len());
    for (ordinal, handle) in physical_devices.into_iter().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(handle) };
        let extensions = unsafe { instance.enumerate_device_extension_properties(handle, None) }
            .map_err(RendererError::EnumerateExtensions)?;
        let has_heap_extension = extensions
            .iter()
            .any(|extension| extension.extension_name == vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name);
        let descriptor_heap_supported =
            has_heap_extension && descriptor_heap_feature(instance, handle);
        let has_drm_extension = extensions.iter().any(|extension| {
            extension.extension_name == vk::EXT_PHYSICAL_DEVICE_DRM_EXTENSION.name
        });
        let drm = has_drm_extension
            .then(|| drm_device_identity(instance, handle))
            .flatten();
        let graphics_queue_family = unsafe {
            instance
                .get_physical_device_queue_family_properties(handle)
                .iter()
                .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .map(|index| index as u32)
        };
        candidates.push(ProbedDevice {
            handle,
            candidate: DeviceCandidate {
                ordinal,
                name: properties.device_name.to_string_lossy().into_owned(),
                device_type: properties.device_type,
                api_version: properties.api_version.into(),
                descriptor_heap_supported,
                graphics_queue_family,
                drm,
            },
        });
    }
    Ok(candidates)
}

fn drm_device_identity(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Option<DrmDeviceIdentity> {
    let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut drm);
    unsafe { instance.get_physical_device_properties2(device, &mut properties) };

    let primary = drm_node(drm.has_primary, drm.primary_major, drm.primary_minor);
    let render = drm_node(drm.has_render, drm.render_major, drm.render_minor);
    (primary.is_some() || render.is_some()).then(|| DrmDeviceIdentity::new(primary, render))
}

fn drm_node(present: vk::Bool32, major: i64, minor: i64) -> Option<DrmNodeId> {
    if present == 0 {
        return None;
    }
    Some(DrmNodeId::new(
        u32::try_from(major).ok()?,
        u32::try_from(minor).ok()?,
    ))
}

fn descriptor_heap_feature(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut descriptor_heap);
    unsafe { instance.get_physical_device_features2(device, &mut features) };
    descriptor_heap.descriptor_heap != 0
}

fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_queue_family: u32,
) -> Result<Device, RendererError> {
    let priorities = [1.0];
    let queue = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(&priorities);
    let queues = [queue];
    let descriptor_heap_name = vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name;
    let extensions = [descriptor_heap_name.as_cstr().as_ptr()];
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(true)
        .build();
    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut descriptor_heap);
    unsafe { instance.create_device(physical_device, &info, None) }
        .map_err(RendererError::CreateDevice)
}

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("failed to load Vulkan library {LIBRARY}: {0}")]
    LoadLibrary(String),
    #[error("failed to load the Vulkan entry points: {0}")]
    LoadEntry(String),
    #[error("failed to query the Vulkan loader version: {0:?}")]
    LoaderVersion(vk::ErrorCode),
    #[error("Vulkan {required} is required but the loader exposes {found}")]
    UnsupportedLoaderVersion { required: Version, found: Version },
    #[error("failed to create the Vulkan instance: {0:?}")]
    CreateInstance(vk::ErrorCode),
    #[error("failed to enumerate Vulkan physical devices: {0:?}")]
    EnumerateDevices(vk::ErrorCode),
    #[error("failed to enumerate Vulkan device extensions: {0:?}")]
    EnumerateExtensions(vk::ErrorCode),
    #[error(transparent)]
    Selection(#[from] DeviceSelectionError),
    #[error("failed to create the Vulkan descriptor-heap device: {0:?}")]
    CreateDevice(vk::ErrorCode),
}
