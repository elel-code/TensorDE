use std::any::Any;
use std::fmt;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use vulkanalia::vk::{
    self, HasBuilder, KhrGetSurfaceCapabilities2ExtensionInstanceCommands,
    KhrSurfaceExtensionInstanceCommands, KhrWaylandSurfaceExtensionInstanceCommands,
};

use crate::backend::InstanceOwner;
use crate::{
    CompositeAlphaModes, Error, Extent2D, Features, Instance, Result, SurfaceFormat,
    SurfaceTransform, SurfaceTransforms, TextureUsages,
};

mod bootstrap;
mod intermediate;
mod offscreen;
mod swapchain;
mod terminal;
mod transaction;

pub use bootstrap::{
    PresentationAdapterRequest, PresentationBootstrap, PresentationBootstrapDescriptor,
    PresentationExtentPolicy, PresentationImageCount, PresentationSurfaceConfigurationDescriptor,
};
pub use intermediate::{
    AcquiredRetainedColorTarget, AcquiredRetainedColorTargets, RetainedColorTargetPool,
    RetainedColorTargetPoolDescriptor, RetainedColorTargetRequest, RetainedColorTargetReservation,
};
pub use offscreen::{
    DirectSurfaceBlocker, FrameTargetPreference, OffscreenColorTarget, OffscreenColorTargets,
    OffscreenColorTargetsDescriptor, OffscreenSampledBindings, OffscreenSamplerTopology,
    PresentationPathDescriptor, PresentationPathPlan, PresentationRequirements, PresentationTarget,
    SurfaceAcquireStrategy, TerminalAlphaMode, TerminalCompositeDescriptor, TerminalSampling,
};
pub use swapchain::{
    AcquiredSurfaceTexture, PresentStatus, SurfaceConfiguration, SurfaceConfigurationRequest,
    Swapchain, SwapchainDescriptor,
};
pub use terminal::{
    FullscreenSampledSurfaceTerminal, FullscreenSampledSurfaceTerminalDescriptor,
    FullscreenSampledSurfaceTerminalProgram,
};
pub use transaction::{
    PresentTransactionOutcome, PresentationTransaction, PresentationTransactionDescriptor,
    PresentationTransactionPhase, PresentationTransactionSchedule, PresentationTransactionStep,
};
#[cfg(feature = "ffmpeg-vulkan-decode")]
pub use transaction::{PresentationDependencyScope, PresentationFrameDependencies};

/// Vulkan presentation surface retaining the host window/display lease and
/// instance lifetime.
#[derive(Clone)]
pub struct Surface {
    inner: Arc<SurfaceInner>,
}

struct SurfaceInner {
    owner: Arc<InstanceOwner>,
    raw: vk::SurfaceKHR,
    _host: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for Surface {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Surface")
            .field("raw", &self.inner.raw)
            .finish_non_exhaustive()
    }
}

impl Surface {
    pub fn raw(&self) -> vk::SurfaceKHR {
        self.inner.raw
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<InstanceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }
}

impl Drop for SurfaceInner {
    fn drop(&mut self) {
        unsafe {
            self.owner.instance.destroy_surface_khr(self.raw, None);
        }
    }
}

impl Instance {
    /// Creates a Vulkan surface and retains `host` until the last surface or
    /// swapchain owner is dropped.
    pub fn create_surface<T>(&self, host: Arc<T>) -> Result<Surface>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static,
    {
        let display = host
            .display_handle()
            .map_err(|error| Error::Validation(format!("obtain display handle: {error}")))?
            .as_raw();
        let window = host
            .window_handle()
            .map_err(|error| Error::Validation(format!("obtain window handle: {error}")))?
            .as_raw();
        let raw = match (display, window) {
            (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => {
                let create = vk::WaylandSurfaceCreateInfoKHR::builder()
                    .display(display.display.as_ptr())
                    .surface(window.surface.as_ptr());
                unsafe {
                    self.shared_owner()
                        .instance
                        .create_wayland_surface_khr(&create, None)
                }
                .map_err(|source| Error::vulkan("vkCreateWaylandSurfaceKHR", source))?
            }
            _ => {
                return Err(Error::Validation(
                    "display and window handles must both be Wayland handles".into(),
                ));
            }
        };
        Ok(Surface {
            inner: Arc::new(SurfaceInner {
                owner: self.shared_owner(),
                raw,
                _host: host,
            }),
        })
    }
}

/// Complete capabilities for one adapter/surface pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceCapabilities {
    pub present_supported: bool,
    pub min_image_count: u32,
    pub max_image_count: Option<u32>,
    pub current_extent: Option<Extent2D>,
    pub min_image_extent: Extent2D,
    pub max_image_extent: Extent2D,
    pub max_image_array_layers: u32,
    pub supported_transforms: SurfaceTransforms,
    pub current_transform: SurfaceTransform,
    pub supported_composite_alpha: CompositeAlphaModes,
    pub supported_usage: TextureUsages,
    pub formats: Vec<SurfaceFormat>,
    pub present_modes: SurfacePresentCapabilities,
    pub present_id2_supported: bool,
    pub present_wait2_supported: bool,
}

impl SurfaceCapabilities {
    pub(crate) fn query(
        owner: &Arc<InstanceOwner>,
        physical_device: vk::PhysicalDevice,
        graphics_queue_family: u32,
        surface: &Surface,
    ) -> Result<Self> {
        if !surface.belongs_to(owner) {
            return Err(Error::Validation(
                "surface was created by a different Instance".into(),
            ));
        }
        let present_supported = unsafe {
            owner.instance.get_physical_device_surface_support_khr(
                physical_device,
                graphics_queue_family,
                surface.raw(),
            )
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfaceSupportKHR", source))?;
        let raw = unsafe {
            owner
                .instance
                .get_physical_device_surface_capabilities_khr(physical_device, surface.raw())
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfaceCapabilitiesKHR", source))?;
        let raw_formats = unsafe {
            owner
                .instance
                .get_physical_device_surface_formats_khr(physical_device, surface.raw())
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfaceFormatsKHR", source))?;
        let modes = unsafe {
            owner
                .instance
                .get_physical_device_surface_present_modes_khr(physical_device, surface.raw())
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfacePresentModesKHR", source))?;
        let surface_info = vk::PhysicalDeviceSurfaceInfo2KHR::builder()
            .surface(surface.raw())
            .build();
        let mut present_id2 = vk::SurfaceCapabilitiesPresentId2KHR::default();
        let mut present_wait2 = vk::SurfaceCapabilitiesPresentWait2KHR::default();
        let mut capabilities2 = vk::SurfaceCapabilities2KHR::builder()
            .push_next(&mut present_id2)
            .push_next(&mut present_wait2)
            .build();
        unsafe {
            owner
                .instance
                .get_physical_device_surface_capabilities2_khr(
                    physical_device,
                    &surface_info,
                    &mut capabilities2,
                )
        }
        .map_err(|source| Error::vulkan("vkGetPhysicalDeviceSurfaceCapabilities2KHR", source))?;
        let formats = raw_formats
            .into_iter()
            .filter_map(SurfaceFormat::from_vk)
            .collect::<Vec<_>>();
        if formats.is_empty() {
            return Err(Error::Validation(
                "surface exposes no renderer-supported typed format/color-space pair".into(),
            ));
        }
        let current_transform =
            SurfaceTransform::from_vk(raw.current_transform).ok_or_else(|| {
                Error::Validation("surface reports an unsupported current transform".into())
            })?;
        Ok(Self {
            present_supported,
            min_image_count: raw.min_image_count,
            max_image_count: (raw.max_image_count != 0).then_some(raw.max_image_count),
            current_extent: (raw.current_extent.width != u32::MAX).then_some(Extent2D::new(
                raw.current_extent.width,
                raw.current_extent.height,
            )),
            min_image_extent: Extent2D::new(
                raw.min_image_extent.width,
                raw.min_image_extent.height,
            ),
            max_image_extent: Extent2D::new(
                raw.max_image_extent.width,
                raw.max_image_extent.height,
            ),
            max_image_array_layers: raw.max_image_array_layers,
            supported_transforms: SurfaceTransforms::from_vk(raw.supported_transforms),
            current_transform,
            supported_composite_alpha: CompositeAlphaModes::from_vk(raw.supported_composite_alpha),
            supported_usage: TextureUsages::from_vk(raw.supported_usage_flags),
            formats,
            present_modes: SurfacePresentCapabilities::from_vk(&modes),
            present_id2_supported: present_id2.present_id2_supported != 0,
            present_wait2_supported: present_wait2.present_wait2_supported != 0,
        })
    }
}

/// Backend-neutral present modes with a one-to-one Vulkan mapping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PresentMode {
    Immediate,
    Mailbox,
    Fifo,
    FifoRelaxed,
    /// `VK_PRESENT_MODE_FIFO_LATEST_READY`. This mode is usable only when
    /// the device extension and feature were enabled *and* the target surface
    /// advertises the mode.
    FifoLatestReady,
}

impl PresentMode {
    pub const fn as_vk(self) -> vk::PresentModeKHR {
        match self {
            Self::Immediate => vk::PresentModeKHR::IMMEDIATE,
            Self::Mailbox => vk::PresentModeKHR::MAILBOX,
            Self::Fifo => vk::PresentModeKHR::FIFO,
            Self::FifoRelaxed => vk::PresentModeKHR::FIFO_RELAXED,
            Self::FifoLatestReady => vk::PresentModeKHR::FIFO_LATEST_READY,
        }
    }

    fn from_vk(mode: vk::PresentModeKHR) -> Option<Self> {
        match mode {
            vk::PresentModeKHR::IMMEDIATE => Some(Self::Immediate),
            vk::PresentModeKHR::MAILBOX => Some(Self::Mailbox),
            vk::PresentModeKHR::FIFO => Some(Self::Fifo),
            vk::PresentModeKHR::FIFO_RELAXED => Some(Self::FifoRelaxed),
            vk::PresentModeKHR::FIFO_LATEST_READY => Some(Self::FifoLatestReady),
            _ => None,
        }
    }
}

/// Present modes reported for one concrete Vulkan surface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SurfacePresentCapabilities {
    modes: Vec<PresentMode>,
}

impl SurfacePresentCapabilities {
    /// Converts `vkGetPhysicalDeviceSurfacePresentModesKHR` output without
    /// synthesizing modes the surface did not report.
    pub fn from_vk(modes: &[vk::PresentModeKHR]) -> Self {
        let mut modes = modes
            .iter()
            .copied()
            .filter_map(PresentMode::from_vk)
            .collect::<Vec<_>>();
        modes.sort_by_key(|mode| *mode as u8);
        modes.dedup();
        Self { modes }
    }

    pub fn modes(&self) -> &[PresentMode] {
        &self.modes
    }

    /// Checks both surface support and the device feature gate. In particular,
    /// extension availability alone never makes FIFO latest-ready usable.
    pub fn supports(&self, mode: PresentMode, enabled_features: Features) -> bool {
        self.modes.contains(&mode)
            && (mode != PresentMode::FifoLatestReady
                || enabled_features.contains(Features::FIFO_LATEST_READY))
    }

    /// Returns the first fully usable preference. There is no implicit
    /// downgrade; callers include `Fifo` explicitly when it is acceptable.
    pub fn choose(
        &self,
        preferences: &[PresentMode],
        enabled_features: Features,
    ) -> Option<PresentMode> {
        preferences
            .iter()
            .copied()
            .find(|mode| self.supports(*mode, enabled_features))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_latest_ready_requires_device_feature_and_surface_mode() {
        let surface = SurfacePresentCapabilities::from_vk(&[
            vk::PresentModeKHR::FIFO,
            vk::PresentModeKHR::FIFO_LATEST_READY,
        ]);
        assert!(!surface.supports(PresentMode::FifoLatestReady, Features::empty()));
        assert!(surface.supports(PresentMode::FifoLatestReady, Features::FIFO_LATEST_READY));

        let fifo_only = SurfacePresentCapabilities::from_vk(&[vk::PresentModeKHR::FIFO]);
        assert!(!fifo_only.supports(PresentMode::FifoLatestReady, Features::FIFO_LATEST_READY));
    }

    #[test]
    fn selection_does_not_hide_a_present_mode_fallback() {
        let surface = SurfacePresentCapabilities::from_vk(&[vk::PresentModeKHR::FIFO]);
        assert_eq!(
            surface.choose(
                &[PresentMode::FifoLatestReady, PresentMode::Fifo],
                Features::FIFO_LATEST_READY,
            ),
            Some(PresentMode::Fifo)
        );
    }
}
