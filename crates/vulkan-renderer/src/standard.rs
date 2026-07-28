//! Strict, descriptor-based public object model.
//!
//! This follows WebGPU's separation of capability discovery from device
//! enablement: `Adapter::features` and `Adapter::limits` report support, while
//! `Adapter::request_device` validates and enables only requested capabilities.

use std::fmt;
use std::sync::Arc;

use vulkanalia::vk::{self, KhrSurfaceExtensionInstanceCommands};

use crate::backend::{
    Backend, BackendConfig, Candidate, DeviceInfo, DevicePreference, InstanceOwner, Queue,
    append_feature_extensions, create_instance, extension_union, load_entry, probe_devices,
    select_device,
};
use crate::{BackendProfile, Error, Features, Limits, Result, SurfacePresentCapabilities};

/// Describes loader/instance creation and the capability profile shared by all
/// adapters requested from the instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstanceDescriptor {
    /// Minimum Vulkan/profile contract used during adapter validation.
    pub profile: BackendProfile,
    /// Window-system instance extensions required by the embedding project.
    pub extra_instance_extensions: Vec<String>,
}

impl Default for InstanceDescriptor {
    fn default() -> Self {
        Self {
            profile: BackendProfile::Vulkan14,
            extra_instance_extensions: Vec::new(),
        }
    }
}

/// GPU class preference used only for ranking compatible adapters.
///
/// It never weakens required features, limits, API version, or extension
/// validation.
pub type PowerPreference = DevicePreference;

/// Adapter-selection constraints, corresponding to WebGPU's
/// `RequestAdapterOptions`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestAdapterOptions {
    /// Preferred GPU class after all hard capability gates pass.
    pub power_preference: PowerPreference,
    /// Restrict selection to Vulkan CPU devices for deterministic fallback
    /// testing. No software fallback is silently selected otherwise.
    pub force_fallback_adapter: bool,
}

impl Default for RequestAdapterOptions {
    fn default() -> Self {
        Self {
            power_preference: PowerPreference::Discrete,
            force_fallback_adapter: false,
        }
    }
}

/// Required capabilities for logical-device creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor {
    /// Diagnostic label retained by the embedding layer.
    pub label: Option<String>,
    /// Exact feature set exposed by the returned `Device`.
    pub required_features: Features,
    /// Minimum limits. Every nonzero field is validated against the adapter.
    pub required_limits: Limits,
    /// Additional Vulkan extensions owned by higher-level feature modules.
    pub required_extensions: Vec<String>,
}

impl Default for DeviceDescriptor {
    fn default() -> Self {
        Self {
            label: None,
            required_features: Features::VULKAN14_RENDERER_BASELINE,
            required_limits: Limits::downlevel_defaults(),
            required_extensions: Vec::new(),
        }
    }
}

/// Vulkan loader and instance owner. Dropping clones only destroys the Vulkan
/// instance after all adapters/devices/queues have released it.
#[derive(Clone)]
pub struct Instance {
    descriptor: InstanceDescriptor,
    owner: Arc<InstanceOwner>,
}

impl fmt::Debug for Instance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Instance")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl Instance {
    /// Loads Vulkan and creates an instance with the descriptor's exact API and
    /// extension requirements.
    pub fn new(descriptor: InstanceDescriptor) -> Result<Self> {
        let entry = load_entry()?;
        let found = entry
            .version()
            .map_err(|source| Error::vulkan("vkEnumerateInstanceVersion", source))?;
        let required = descriptor.profile.required_api_version();
        if found < required {
            return Err(Error::LoaderVersion { required, found });
        }
        let extensions = extension_union(
            descriptor.profile.required_instance_extensions(),
            &descriptor.extra_instance_extensions,
        );
        let owner = create_instance(entry, required, &extensions)?;
        Ok(Self { descriptor, owner })
    }

    /// Returns every Vulkan physical device without treating unsupported
    /// devices as compatible. Callers inspect features/limits before requesting
    /// a logical device.
    pub fn enumerate_adapters(&self) -> Result<Vec<Adapter>> {
        Ok(probe_devices(&self.owner.instance)?
            .into_iter()
            .map(|candidate| Adapter {
                descriptor: self.descriptor.clone(),
                owner: Arc::clone(&self.owner),
                candidate,
            })
            .collect())
    }

    /// Selects the highest-ranked adapter satisfying the instance profile and
    /// the Vulkan 1.4 renderer baseline.
    pub fn request_adapter(&self, options: RequestAdapterOptions) -> Result<Adapter> {
        let mut candidates = probe_devices(&self.owner.instance)?;
        if options.force_fallback_adapter {
            candidates
                .retain(|candidate| candidate.info.device_type == vk::PhysicalDeviceType::CPU);
        }
        if candidates.is_empty() {
            return Err(Error::NoPhysicalDevice);
        }
        let mut required_features = Features::VULKAN14_RENDERER_BASELINE;
        if self.descriptor.profile == BackendProfile::Roadmap2026 {
            required_features |= Features::FIFO_LATEST_READY;
        }
        let mut feature_extensions = Vec::new();
        append_feature_extensions(required_features, &mut feature_extensions);
        let extensions = extension_union(
            self.descriptor.profile.required_device_extensions(),
            &feature_extensions,
        );
        let (candidate, rejections) = select_device(
            candidates,
            self.descriptor.profile,
            options.power_preference,
            &extensions,
            required_features,
            Limits::downlevel_defaults(),
        );
        Ok(Adapter {
            descriptor: self.descriptor.clone(),
            owner: Arc::clone(&self.owner),
            candidate: candidate.ok_or(Error::NoCompatibleDevice(rejections))?,
        })
    }
}

/// Physical-device capability snapshot. It owns no logical device state.
#[derive(Clone)]
pub struct Adapter {
    descriptor: InstanceDescriptor,
    owner: Arc<InstanceOwner>,
    candidate: Candidate,
}

impl fmt::Debug for Adapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Adapter")
            .field("info", &self.candidate.info)
            .finish_non_exhaustive()
    }
}

impl Adapter {
    /// Immutable physical-device identity and raw capability snapshot.
    pub const fn info(&self) -> &DeviceInfo {
        &self.candidate.info
    }

    /// Features supported by the adapter, not features enabled on a device.
    pub const fn features(&self) -> Features {
        self.candidate.info.supported_features
    }

    /// Maximum adapter limits used for device-request validation.
    pub const fn limits(&self) -> Limits {
        self.candidate.info.limits
    }

    /// Queries the present modes of one concrete surface. The surface must
    /// belong to this adapter's Vulkan instance and remain valid for the call.
    ///
    /// # Safety
    ///
    /// `surface` must be a live `VkSurfaceKHR` created from `self`'s instance.
    pub unsafe fn surface_present_capabilities(
        &self,
        surface: vk::SurfaceKHR,
    ) -> Result<SurfacePresentCapabilities> {
        let modes = unsafe {
            self.owner
                .instance
                .get_physical_device_surface_present_modes_khr(self.candidate.handle, surface)
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfacePresentModesKHR", source))?;
        Ok(SurfacePresentCapabilities::from_vk(&modes))
    }

    /// Validates the full descriptor and returns independently owned device and
    /// queue handles. No feature is silently downgraded.
    pub fn request_device(&self, descriptor: DeviceDescriptor) -> Result<(Device, Queue)> {
        let mut required_features = descriptor.required_features;
        if self.descriptor.profile == BackendProfile::Roadmap2026 {
            required_features |= Features::FIFO_LATEST_READY;
        }
        let mut feature_extensions = descriptor.required_extensions.clone();
        append_feature_extensions(required_features, &mut feature_extensions);
        let extensions = extension_union(
            self.descriptor.profile.required_device_extensions(),
            &feature_extensions,
        );
        let (candidate, rejections) = select_device(
            vec![self.candidate.clone()],
            self.descriptor.profile,
            DevicePreference::Any,
            &extensions,
            required_features,
            descriptor.required_limits,
        );
        let candidate = candidate.ok_or(Error::NoCompatibleDevice(rejections))?;
        let config = BackendConfig {
            label: descriptor.label,
            profile: self.descriptor.profile,
            device_preference: DevicePreference::Any,
            extra_instance_extensions: self.descriptor.extra_instance_extensions.clone(),
            extra_device_extensions: descriptor.required_extensions,
            required_features,
            required_limits: descriptor.required_limits,
        };
        let device =
            Backend::from_selected(config, Arc::clone(&self.owner), candidate, &extensions)?;
        let queue = device.queue();
        Ok((device, queue))
    }
}

/// Logical Vulkan device created by `Adapter::request_device`.
pub type Device = Backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_heap_and_fifo_features_map_to_their_device_extensions() {
        let mut extensions = Vec::new();
        append_feature_extensions(
            Features::DESCRIPTOR_HEAP | Features::FIFO_LATEST_READY,
            &mut extensions,
        );
        extensions.sort();
        assert_eq!(
            extensions,
            vec![
                "VK_EXT_descriptor_heap".to_owned(),
                "VK_KHR_present_mode_fifo_latest_ready".to_owned(),
            ]
        );
    }

    #[test]
    fn device_descriptor_never_enables_optional_features_by_default() {
        let descriptor = DeviceDescriptor::default();
        assert!(
            descriptor
                .required_features
                .contains(Features::VULKAN14_RENDERER_BASELINE)
        );
        assert!(
            !descriptor
                .required_features
                .contains(Features::DESCRIPTOR_HEAP)
        );
        assert!(
            !descriptor
                .required_features
                .contains(Features::FIFO_LATEST_READY)
        );
    }
}
