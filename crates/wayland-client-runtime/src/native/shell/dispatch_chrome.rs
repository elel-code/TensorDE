//! xdg-toplevel-icon and ext-background-effect dispatch for the native shell.

use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1, ext_background_effect_surface_v1,
};
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1, xdg_toplevel_icon_v1,
};

use super::types::NativeShellState;

impl Dispatch<xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
        event: xdg_toplevel_icon_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel_icon_manager_v1::Event::IconSize { size } => {
                if size > 0 && !state.preferred_icon_sizes.contains(&(size as u32)) {
                    state.preferred_icon_sizes.push(size as u32);
                }
            }
            xdg_toplevel_icon_manager_v1::Event::Done => {}
            _ => {}
        }
    }
}

impl Dispatch<xdg_toplevel_icon_v1::XdgToplevelIconV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &xdg_toplevel_icon_v1::XdgToplevelIconV1,
        _: xdg_toplevel_icon_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1, ()>
    for NativeShellState
{
    fn event(
        state: &mut Self,
        _: &ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities {
            flags: WEnum::Value(caps),
        } = event
        {
            let capable =
                caps.contains(ext_background_effect_manager_v1::Capability::Blur);
            let became_capable = capable && !state.background_blur_capable;
            state.background_blur_capable = capable;
            if became_capable {
                // Surfaces may have called set_blur before this event arrived
                // (startup path). Re-apply after dispatch via after_dispatch.
                state.pending_blur_replay = true;
            }
        }
    }
}

impl Dispatch<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1, ()>
    for NativeShellState
{
    fn event(
        _: &mut Self,
        _: &ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
        _: ext_background_effect_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_region::WlRegion, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_region::WlRegion,
        _: wayland_client::protocol::wl_region::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
