pub(in crate::renderer::native_vulkan::vulkan) mod codec;
pub(in crate::renderer::native_vulkan::vulkan) mod device;
pub(in crate::renderer::native_vulkan::vulkan) mod format_probe;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) mod media_runtime;
pub(in crate::renderer::native_vulkan::vulkan) mod present_device;
pub(in crate::renderer::native_vulkan::vulkan) mod present_handoff;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) mod present_runtime;
pub(in crate::renderer::native_vulkan::vulkan) mod profile_gate;
pub(in crate::renderer::native_vulkan::vulkan) mod profile_info;
pub(in crate::renderer::native_vulkan::vulkan) mod profile_labels;
pub(in crate::renderer::native_vulkan::vulkan) mod profile_probe;
pub(in crate::renderer::native_vulkan::vulkan) mod session;
pub(in crate::renderer::native_vulkan::vulkan) mod session_images;
pub(in crate::renderer::native_vulkan::vulkan) mod surface_host;

use self::device as video_device;
use self::format_probe as video_format_probe;
#[cfg(feature = "native-vulkan-video")]
use self::media_runtime as video_media_runtime;
#[cfg(feature = "native-vulkan-video")]
use self::present_device as video_present_device;
#[cfg(feature = "native-vulkan-video")]
use self::present_handoff as video_present_handoff;
use self::profile_gate as video_profile_gate;
use self::profile_info as video_profile_info;
use self::profile_labels as video_profile_labels;
use self::session as video_session;
use self::surface_host as video_surface_host;
use super::core::{features, instance, memory, queue_probe, roadmap_2026};
#[cfg(feature = "native-vulkan-video")]
use super::present::render as render_present;
use super::present::swapchain;
