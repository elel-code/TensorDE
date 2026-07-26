//! Data device (clipboard + drag-and-drop) dispatch for the native shell.

use std::io::Write;

use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for NativeShellState {
    // Opcode 0 = data_offer creates a new wl_data_offer child object.
    event_created_child!(NativeShellState, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);

    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                let offer_id = id.id().protocol_id();
                state.offer_mimes.entry(offer_id).or_default();
            }
            wl_data_device::Event::Selection { id } => {
                if let Some(old) = state.incoming_offer.take() {
                    let old_id = old.id().protocol_id();
                    state.offer_mimes.remove(&old_id);
                    old.destroy();
                }
                state.incoming_mimes.clear();
                if let Some(offer) = id {
                    let offer_id = offer.id().protocol_id();
                    let mimes = state.offer_mimes.remove(&offer_id).unwrap_or_default();
                    state.incoming_mimes = mimes.clone();
                    state.incoming_offer = Some(offer);
                    state.push(NativeShellEvent::Selection { mimes });
                } else {
                    state.push(NativeShellEvent::Selection { mimes: Vec::new() });
                }
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                state.dnd_serial = Some(serial);
                if let Some(old) = state.dnd_offer.take() {
                    old.destroy();
                }
                state.dnd_mimes.clear();
                state.dnd_offer_id = None;
                let surface_id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.dnd_focus = surface_id;
                if let Some(offer) = id {
                    let offer_obj = offer.id().protocol_id();
                    let mimes = state
                        .offer_mimes
                        .get(&offer_obj)
                        .cloned()
                        .unwrap_or_default();
                    state.dnd_mimes = mimes.clone();
                    if let Some(mime) = mimes.first() {
                        offer.accept(serial, Some(mime.clone()));
                    }
                    offer.set_actions(
                        wayland_client::protocol::wl_data_device_manager::DndAction::Copy
                            | wayland_client::protocol::wl_data_device_manager::DndAction::Move,
                        wayland_client::protocol::wl_data_device_manager::DndAction::Copy,
                    );
                    let public_id = state.alloc_transfer_id();
                    state.dnd_offer_id = Some(public_id);
                    state.dnd_offer = Some(offer);
                    if let Some(surface) = surface_id {
                        state.push(NativeShellEvent::DndEnter {
                            offer: public_id,
                            surface,
                            x,
                            y,
                            mimes,
                        });
                    }
                }
            }
            wl_data_device::Event::Leave => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                let surface = state.dnd_focus;
                if let Some(old) = state.dnd_offer.take() {
                    let old_id = old.id().protocol_id();
                    state.offer_mimes.remove(&old_id);
                    old.destroy();
                }
                state.dnd_mimes.clear();
                state.dnd_focus = None;
                state.dnd_serial = None;
                state.dnd_offer_id = None;
                state.push(NativeShellEvent::DndLeave { offer, surface });
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                state.push(NativeShellEvent::DndMotion { offer, x, y });
            }
            wl_data_device::Event::Drop => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                state.push(NativeShellEvent::DndDrop { offer });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                let offer_id = offer.id().protocol_id();
                let mimes = state.offer_mimes.entry(offer_id).or_default();
                if !mimes.iter().any(|m| m == &mime_type) {
                    mimes.push(mime_type.clone());
                }
                if state
                    .incoming_offer
                    .as_ref()
                    .is_some_and(|o| o.id() == offer.id())
                    && !state.incoming_mimes.iter().any(|m| m == &mime_type)
                {
                    state.incoming_mimes.push(mime_type.clone());
                }
                if state
                    .dnd_offer
                    .as_ref()
                    .is_some_and(|o| o.id() == offer.id())
                    && !state.dnd_mimes.iter().any(|m| m == &mime_type)
                {
                    state.dnd_mimes.push(mime_type);
                }
            }
            wl_data_offer::Event::SourceActions { .. }
            | wl_data_offer::Event::Action { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let bytes = if state
                    .selection_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state
                        .selection_content
                        .as_ref()
                        .and_then(|c| c.bytes_for_mime(&mime_type))
                } else if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state
                        .dnd_source_content
                        .as_ref()
                        .and_then(|c| c.bytes_for_mime(&mime_type))
                } else {
                    None
                };
                if let Some(bytes) = bytes {
                    let mut file = std::fs::File::from(fd);
                    let _ = file.write_all(&bytes);
                }
            }
            wl_data_source::Event::Cancelled => {
                if state
                    .selection_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state.selection_source = None;
                    state.selection_content = None;
                    state.push(NativeShellEvent::SelectionCancelled);
                }
                if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    let source_id = state.dnd_source_id.unwrap_or(0);
                    state.dnd_source = None;
                    state.dnd_source_id = None;
                    state.dnd_source_content = None;
                    state.dnd_icon = None;
                    state.push(NativeShellEvent::DndFinished {
                        source: source_id,
                        cancelled: true,
                    });
                }
            }
            wl_data_source::Event::DndFinished => {
                if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    let source_id = state.dnd_source_id.unwrap_or(0);
                    state.dnd_source = None;
                    state.dnd_source_id = None;
                    state.dnd_source_content = None;
                    state.dnd_icon = None;
                    state.push(NativeShellEvent::DndFinished {
                        source: source_id,
                        cancelled: false,
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::shell::{NativePopupPositioner, NativeShell};

    #[test]
    fn native_shell_creates_toplevel_when_compositor_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel("fika-native-smoke", "dev.fika.NativeSmoke")
            .expect("create toplevel");
        assert_eq!(shell.toplevel_count(), 1);

        compio::runtime::Runtime::new()
            .expect("compio")
            .block_on(async {
                for _ in 0..32 {
                    let _ = shell.pump_once().await;
                    if shell.is_configured(id) {
                        break;
                    }
                }
            });

        let mut events = Vec::new();
        shell.drain_events_into(&mut events);
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_creates_popup_when_compositor_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let parent = shell
            .create_toplevel("fika-native-popup-parent", "dev.fika.NativePopup")
            .expect("create toplevel");
        compio::runtime::Runtime::new()
            .expect("compio")
            .block_on(async {
                for _ in 0..32 {
                    let _ = shell.pump_once().await;
                    if shell.is_configured(parent) {
                        break;
                    }
                }
            });
        if !shell.is_configured(parent) {
            let _ = shell.destroy_toplevel(parent);
            return;
        }
        let mut positioner = NativePopupPositioner::default();
        positioner.anchor_rect = crate::geometry::LogicalRect::new(0, 0, 100, 40);
        let popup = shell
            .create_popup(parent, &positioner, false)
            .expect("create popup");
        assert_eq!(shell.popup_count(), 1);
        let _ = shell.dispatch_pending();
        let _ = shell.destroy_popup(popup);
        let _ = shell.destroy_toplevel(parent);
    }

    #[test]
    fn native_shell_interactive_apis_need_serial() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel_gpu("move", "dev.fika.Move", 200, 200)
            .expect("toplevel");
        // Without input serial, interactive requests fail cleanly.
        assert!(shell.begin_interactive_move(id).is_err());
        assert!(shell
            .begin_interactive_resize(id, crate::ResizeEdge::Right)
            .is_err());
        assert!(shell
            .show_window_menu(id, crate::geometry::LogicalPosition::new(1, 1))
            .is_err());
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_pointer_constraints_api_when_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel_gpu("cap", "dev.fika.Capture", 200, 200)
            .expect("toplevel");
        if shell.has_pointer_constraints() {
            shell
                .set_pointer_constraint(id, crate::PointerConstraint::Confined)
                .expect("confine");
            shell
                .set_pointer_constraint(id, crate::PointerConstraint::Locked)
                .expect("lock");
            shell
                .set_pointer_constraint(id, crate::PointerConstraint::None)
                .expect("clear");
        } else {
            let err = shell
                .set_pointer_constraint(id, crate::PointerConstraint::Locked)
                .expect_err("constraints missing");
            let _ = err;
        }
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_set_decorations_and_dnd_icon_smoke() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel_gpu("deco", "dev.fika.Deco", 200, 200)
            .expect("toplevel");
        let _ = shell.set_decorations(id, crate::DecorationPreference::Server);
        let _ = shell.set_decorations(id, crate::DecorationPreference::Client);
        // DnD without serial is expected to fail; with a synthetic path we only
        // verify the icon helper builds when a serial exists after input.
        let icon = crate::DndIcon::new(
            vec![0u8; 4 * 16 * 16],
            16,
            16,
            1,
            crate::geometry::LogicalPosition::new(0, 0),
        )
        .expect("icon");
        let content = crate::TransferContent::text("drag");
        // May fail without input serial — that is fine for this smoke.
        let _ = shell.start_drag_content_with_icon(id, content, Some(icon));
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_set_icon_name_when_global_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel_gpu("icon", "dev.fika.Icon", 200, 200)
            .expect("toplevel");
        if shell.has_toplevel_icon() {
            let icon = crate::ToplevelIcon::from_name("fika").expect("icon name");
            shell
                .set_toplevel_icon(id, Some(icon))
                .expect("set icon");
            shell
                .set_toplevel_icon(id, None)
                .expect("clear icon");
        }
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_set_blur_when_capable() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel_gpu("blur", "dev.fika.Blur", 200, 200)
            .expect("toplevel");
        // Drain capability events.
        let _ = shell.dispatch_pending();
        if shell.has_background_blur() {
            shell
                .set_blur(
                    id,
                    crate::BlurState::Enabled(crate::BlurRegion::EntireSurface),
                )
                .expect("enable blur");
            shell
                .set_blur(id, crate::BlurState::Disabled)
                .expect("disable blur");
        } else {
            let err = shell
                .set_blur(
                    id,
                    crate::BlurState::Enabled(crate::BlurRegion::EntireSurface),
                )
                .expect_err("blur should fail without capability");
            let _ = err;
        }
        let _ = shell.destroy_toplevel(id);
    }

    #[test]
    fn native_shell_creates_parented_dialog_when_compositor_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let parent = shell
            .create_toplevel_gpu("parent", "dev.fika.DialogParent", 400, 300)
            .expect("parent");
        let dialog = shell
            .create_dialog_gpu(parent, "dialog", "dev.fika.DialogChild", 320, 240, true)
            .expect("dialog");
        assert_eq!(shell.toplevel_count(), 2);
        let _ = shell.set_min_size(
            dialog,
            Some(crate::geometry::LogicalSize::new(200, 100)),
        );
        let _ = shell.set_max_size(
            dialog,
            Some(crate::geometry::LogicalSize::new(800, 600)),
        );
        let _ = shell.set_window_geometry(
            dialog,
            crate::geometry::LogicalPosition::new(0, 0),
            crate::geometry::LogicalSize::new(320, 240),
        );
        let _ = shell.dispatch_pending();
        let _ = shell.destroy_toplevel(dialog);
        let _ = shell.destroy_toplevel(parent);
    }
}
