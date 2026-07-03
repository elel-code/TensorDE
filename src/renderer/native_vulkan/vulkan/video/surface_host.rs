use serde::Serialize;

use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoSurfaceHostSnapshot {
    pub binding: &'static str,
    pub platform_backend: &'static str,
    pub event_loop_backend: &'static str,
    pub surface_handle_model: &'static str,
    pub wait_configure_roundtrips: usize,
    pub requested_output_name: Option<String>,
    pub logical_size: (u32, u32),
    pub buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
    pub cross_platform_boundary: &'static str,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVideoSurfaceHost {
    _host: NativeWaylandHost,
    handles: NativeWaylandSurfaceHandles,
    snapshot: NativeVulkanVideoSurfaceHostSnapshot,
}

impl NativeVulkanVideoSurfaceHost {
    pub(in crate::renderer::native_vulkan::vulkan) fn connect_wayland(
        options: NativeWaylandHostOptions,
        wait_configure_roundtrips: usize,
    ) -> Result<Self, String> {
        let requested_output_name = options.output_name.clone();
        let mut host = NativeWaylandHost::connect(options).map_err(|err| err.to_string())?;
        host.wait_until_configured(wait_configure_roundtrips)
            .map_err(|err| err.to_string())?;
        let handles = host.surface_handles().map_err(|err| err.to_string())?;
        Ok(Self {
            _host: host,
            handles,
            snapshot: NativeVulkanVideoSurfaceHostSnapshot {
                binding: "native-vulkan-video-surface-host",
                platform_backend: "wayland-layer-shell",
                event_loop_backend: "smithay-client-toolkit-event-queue",
                surface_handle_model: "raw-wayland-display-and-surface-handles",
                wait_configure_roundtrips,
                requested_output_name,
                logical_size: handles.logical_size,
                buffer_size: handles.buffer_size,
                dmabuf_main_device: handles.dmabuf_main_device,
                cross_platform_boundary: "video decode and decoded-image present depend on surface handles, not direct event-loop ownership",
            },
        })
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn handles(
        &self,
    ) -> NativeWaylandSurfaceHandles {
        self.handles
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn snapshot(
        &self,
    ) -> &NativeVulkanVideoSurfaceHostSnapshot {
        &self.snapshot
    }
}
