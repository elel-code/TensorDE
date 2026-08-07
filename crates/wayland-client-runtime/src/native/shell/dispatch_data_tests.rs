//! Integration smoke tests for the native shell (live compositor).
//!
//! Included as `dispatch_data::tests` so protocol smoke stays next to data-device
//! dispatch without bloating the production module.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use wayland_client::Proxy;

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

    for _ in 0..32 {
        let _ = shell.try_read_and_dispatch();
        if shell.is_configured(id) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }

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
    for _ in 0..32 {
        let _ = shell.try_read_and_dispatch();
        if shell.is_configured(parent) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    if !shell.is_configured(parent) {
        let _ = shell.destroy_toplevel(parent);
        return;
    }
    let positioner = NativePopupPositioner {
        anchor_rect: crate::geometry::LogicalRect::new(0, 0, 100, 40),
        ..NativePopupPositioner::default()
    };
    let popup = shell
        .create_popup(parent, &positioner, None)
        .expect("create popup");
    assert_eq!(shell.popup_count(), 1);
    let popup_handle = shell.surface_handle(popup).expect("popup renderer handle");
    assert_eq!(popup_handle.kind(), crate::surface::SurfaceKind::Popup);
    let parent_handle = shell.surface_handle(parent).expect("toplevel handle");
    assert_eq!(parent_handle.kind(), crate::surface::SurfaceKind::Toplevel);
    // RWH contracts: Vulkan / wgpu need both display and window handles.
    assert!(popup_handle.window_handle().is_ok());
    assert!(popup_handle.display_handle().is_ok());
    let popup_surface = popup_handle.wl_surface().clone();
    let parent_surface = parent_handle.wl_surface().clone();
    let popup_role = shell.state.popups.get(&popup).unwrap().popup.clone();
    let parent_role = shell.state.toplevels.get(&parent).unwrap().toplevel.clone();
    let _ = shell.dispatch_pending();
    let _ = shell.destroy_popup(popup);
    let _ = shell.destroy_toplevel(parent);
    assert!(popup_surface.is_alive());
    assert!(parent_surface.is_alive());
    assert!(popup_role.is_alive());
    assert!(parent_role.is_alive());
    drop(parent_handle);
    assert!(
        parent_surface.is_alive(),
        "child renderer lease must retain its parent role tree"
    );
    drop(popup_handle);
    assert!(!popup_surface.is_alive());
    assert!(!parent_surface.is_alive());
    assert!(!popup_role.is_alive());
    assert!(!parent_role.is_alive());
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
    assert!(
        shell
            .begin_interactive_resize(id, crate::ResizeEdge::Right)
            .is_err()
    );
    assert!(
        shell
            .show_window_menu(id, crate::geometry::LogicalPosition::new(1, 1))
            .is_err()
    );
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
    // Full CSD path: client mode materializes subsurface frames when
    // subcompositor is available (always on modern compositors).
    assert!(
        shell.has_subcompositor() || !shell.has_xdg_decoration(),
        "subcompositor expected when xdg-decoration is present"
    );
    // After Client preference, either SSD was forced by compositor or CSD
    // frame is tracked.
    let _ = shell.redraw_csd(id);
    let content = crate::TransferContent::text("drag");
    // May fail without input serial — that is fine for this smoke.
    let _ = shell.start_drag_content(id, content);
    let _ = shell.destroy_toplevel(id);
    // Destroy clears CSD frames.
    assert_eq!(shell.csd_frame_count(), 0);
}

#[test]
fn native_shell_full_csd_lifecycle() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    if !shell.has_subcompositor() {
        return;
    }
    let id = shell
        .create_toplevel_sized(
            "csd-full",
            "dev.fika.CsdFull",
            400,
            300,
            [0xff, 0x33, 0x66, 0x99],
        )
        .expect("toplevel");
    shell
        .set_decorations(id, crate::DecorationPreference::Client)
        .expect("client decorations");
    // Pump so decoration.configure (if any) and xdg configure land.
    for _ in 0..48 {
        let _ = shell.try_read_and_dispatch();
        if shell.is_configured(id) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    shell.set_title(id, "CSD Title Updated").expect("title");
    // If compositor forced SSD, frame may be absent; otherwise present.
    if shell.csd_frame_count() > 0 {
        assert!(shell.csd_frame_count() >= 1);
        shell.redraw_all_csd().expect("redraw");
    }
    // None preference hides chrome.
    shell
        .set_decorations(id, crate::DecorationPreference::None)
        .expect("none");
    let _ = shell.destroy_toplevel(id);
    assert_eq!(shell.csd_frame_count(), 0);
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
        shell.set_toplevel_icon(id, Some(icon)).expect("set icon");
        shell.set_toplevel_icon(id, None).expect("clear icon");
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
    // Always accept enable: if capability is not yet known, the request is
    // remembered and applied when Capabilities arrives.
    shell
        .set_blur(
            id,
            crate::BlurState::Enabled(crate::BlurRegion::EntireSurface),
        )
        .expect("enable blur (or queue pending)");
    if shell.has_background_blur() {
        shell
            .set_blur(id, crate::BlurState::Disabled)
            .expect("disable blur");
    }
    let _ = shell.destroy_toplevel(id);
}

#[test]
fn native_shell_text_input_applies_cursor_rectangle_when_present() {
    use crate::geometry::LogicalRect;
    use crate::text_input::{
        TextInputContentHint, TextInputContentPurpose, TextInputContentType, TextInputState,
        TextInputSurroundingText,
    };

    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    if !shell.has_text_input() {
        return;
    }
    let id = shell
        .create_toplevel_gpu("ime", "dev.fika.Ime", 400, 300)
        .expect("toplevel");
    let surrounding = TextInputSurroundingText::new("hello", 5, 5).expect("surrounding");
    let state = TextInputState::new()
        .with_surrounding_text(surrounding)
        .with_content_type(TextInputContentType {
            hints: TextInputContentHint::COMPLETION,
            purpose: TextInputContentPurpose::Normal,
        })
        .with_cursor_rectangle(LogicalRect::new(120, 80, 2, 18))
        .expect("cursor rect");
    // Must not hard-code (0,0); applying real rect must succeed.
    shell
        .set_text_input_state(id, &state)
        .expect("set text input state with cursor");
    shell.disable_text_input().expect("disable");
    let _ = shell.destroy_toplevel(id);
}

#[test]
fn native_shell_primary_selection_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    if !shell.has_primary_selection() {
        return;
    }
    assert!(shell.capabilities().primary_selection);
    // Dual-write path: set_selection also arms primary when available.
    // May fail without input serial on a fresh seat — that is OK.
    let _ = shell.set_selection_text("primary-smoke");
    let id = shell
        .create_toplevel_gpu("ps", "dev.fika.Primary", 100, 100)
        .expect("toplevel");
    let _ = shell.destroy_toplevel(id);
}

#[test]
fn native_shell_layer_surface_gpu_and_outputs_refresh() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    // Outputs should carry refresh when the compositor advertised a current mode.
    for out in shell.outputs() {
        if let Some(mhz) = out.refresh_mhz {
            assert!(mhz > 0, "refresh_mhz should be positive when set");
            assert!(out.refresh_hz().unwrap_or(0.0) > 0.0);
        }
    }
    if !shell.has_layer_shell() {
        return;
    }
    use crate::layer_shell::{LayerAnchor, LayerSurfaceLayer, LayerSurfaceState};
    let state = LayerSurfaceState {
        size: crate::LogicalSize::new(320, 200),
        anchor: LayerAnchor::TOP | LayerAnchor::LEFT,
        exclusive_zone: 0,
        exclusive_edge: None,
        margins: Default::default(),
        keyboard_interactivity: Default::default(),
        layer: LayerSurfaceLayer::Background,
    };
    let id = shell
        .create_layer_surface_gpu("fika-gpu-layer", None, state)
        .expect("gpu layer");
    // Bufferless GPU layer: scale starts at 1; fractional may update later.
    assert_eq!(shell.scale_factor(id), Some(1.0));
    // Viewporter destination is safe to set for fixed-size layers.
    shell
        .set_viewport_destination(id, 320, 200)
        .expect("layer viewport");
    // Wallpaper-style regions: full opaque + empty input (passthrough).
    shell
        .set_opaque_region(id, crate::SurfaceRegion::full(320, 200))
        .expect("opaque region");
    shell
        .set_input_region(id, crate::SurfaceRegion::Empty)
        .expect("input passthrough");
    shell.commit_surface(id).expect("commit regions");
    // Output name lookup is best-effort (needs wl_output v4 name event).
    for out in shell.outputs() {
        if let Some(ref name) = out.name {
            let found = shell.find_output_by_name(name);
            assert_eq!(found.as_ref().map(|o| o.id), Some(out.id));
        }
    }
    let handle = shell.public_surface_handle(id).expect("handle");
    assert!(
        handle.window_handle().is_ok() && handle.display_handle().is_ok(),
        "GPU layer must export RWH for Vulkan WSI"
    );
    // Frame + presentation arm on layers (Tensor Wallpaper present pacing).
    shell.request_frame(id).expect("layer frame");
    assert!(shell.is_frame_pending(id));
    // Coalesce: second arm is a no-op while pending.
    shell.request_frame(id).expect("layer frame coalesced");
    assert!(shell.is_frame_pending(id));
    shell
        .request_presentation_feedback(id)
        .expect("layer presentation");
    assert!(shell.is_presentation_pending(id) || !shell.has_presentation());
    shell
        .request_presentation_feedback(id)
        .expect("presentation coalesced");
    assert_eq!(
        shell.logical_size(id),
        Some(crate::LogicalSize::new(320, 200))
    );
    assert!(shell.buffer_size(id).is_some());
    assert_eq!(
        shell.surface_kind(id),
        Some(crate::surface::SurfaceKind::Layer)
    );
    // Multi-seat: at least the primary seat is registered when a seat exists.
    if shell.seat().is_some() {
        assert!(shell.seat_count() >= 1);
        assert!(!shell.seats().is_empty());
        if let Some(sid) = shell.primary_seat_id() {
            // Focus/serial queries are safe with no input yet.
            let _ = shell.seat_keyboard_focus(sid);
            let _ = shell.seat_pointer_focus(sid);
            let _ = shell.seat_last_input_serial(sid);
            let _ = shell.seat_input_serial(sid, crate::InputSerialSource::PointerPress);
            // Transfer devices should be bound per seat at connect.
            assert!(
                shell.seat_has_data_device(sid) || !shell.capabilities().data_device,
                "primary seat should have data_device when manager is present"
            );
        }
    }
    // outputs_into reuses capacity.
    let mut outs = Vec::with_capacity(4);
    shell.outputs_into(&mut outs);
    let cap = outs.capacity();
    shell.outputs_into(&mut outs);
    assert!(outs.capacity() >= cap);
    let _ = shell.destroy_layer_surface(id);
    assert!(shell.scale_factor(id).is_none());
    assert!(!shell.is_frame_pending(id));
    assert!(!shell.is_presentation_pending(id));
}

#[test]
fn native_shell_linux_dmabuf_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    if !shell.has_linux_dmabuf() {
        return;
    }
    assert!(shell.capabilities().linux_dmabuf);
    let ver = shell.linux_dmabuf_version().expect("version");
    assert!(ver >= 3);
    // v4+: request default feedback (events may arrive after more dispatch).
    if ver >= 4 {
        shell
            .request_dmabuf_default_feedback()
            .expect("default feedback");
        let _ = shell.dispatch_pending();
        // Feedback is optional timing-wise; just ensure no panic.
        let _ = shell.dmabuf_default_feedback();
    }
    // v3: modifiers may be populated after roundtrip; presence is enough.
    let _ = shell.dmabuf_modifiers();
    let id = shell
        .create_toplevel_gpu("dmabuf", "dev.fika.Dmabuf", 100, 100)
        .expect("toplevel");
    if ver >= 4 {
        let _ = shell.request_dmabuf_surface_feedback(id);
        let _ = shell.dispatch_pending();
    }
    // Invalid params must fail client-side without contacting the compositor.
    let bad = crate::dmabuf::DmabufBufferParams::new(0, 0, 0x34325241);
    assert!(shell.create_dmabuf_buffer(bad).is_err());
    let _ = shell.destroy_toplevel(id);
}

#[test]
fn native_shell_idle_inhibit_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    if !shell.has_idle_inhibit() {
        return;
    }
    assert!(shell.capabilities().idle_inhibit);
    let id = shell
        .create_toplevel_gpu("idle", "dev.fika.IdleInhibit", 100, 100)
        .expect("toplevel");
    // Fullscreen path best-effort arms inhibit; explicit API must work too.
    shell.set_idle_inhibit(id, true).expect("inhibit on");
    shell
        .set_idle_inhibit(id, true)
        .expect("inhibit idempotent");
    shell.set_idle_inhibit(id, false).expect("inhibit off");
    let _ = shell.set_fullscreen(id, true);
    let _ = shell.set_fullscreen(id, false);
    let _ = shell.destroy_toplevel(id);
}

#[test]
fn native_shell_idle_notify_and_foreign_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    let caps = shell.capabilities();
    if caps.idle_notify {
        assert!(shell.has_idle_notify());
        let kind = if caps.idle_notify_input {
            crate::IdleNotifyKind::InputOnly
        } else {
            crate::IdleNotifyKind::WithInhibitors
        };
        let id = shell
            .create_idle_notification(60_000, None, kind)
            .expect("idle notification");
        shell
            .destroy_idle_notification(id)
            .expect("destroy idle notification");
    }
    if caps.xdg_foreign {
        assert!(shell.has_xdg_foreign());
        let id = shell
            .create_toplevel_gpu("foreign", "dev.fika.Foreign", 120, 120)
            .expect("toplevel");
        shell.export_toplevel(id).expect("export");
        // Handle arrives asynchronously; unexport must still be safe.
        shell.unexport_toplevel(id).expect("unexport");
        let _ = shell.destroy_toplevel(id);
    }
}

#[test]
fn native_shell_output_power_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    if !shell.capabilities().output_power {
        return;
    }
    let Some(output) = shell.outputs().first().map(|info| info.id) else {
        return;
    };

    assert!(shell.has_output_power());
    shell
        .create_output_power(output)
        .expect("create output power control");
    assert!(shell.create_output_power(output).is_err());
    shell
        .destroy_output_power(output)
        .expect("destroy output power control");
}

#[test]
fn native_shell_relative_pointer_enable_is_multiseat_safe() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let _ = shell.dispatch_pending();
    if !shell.capabilities().relative_pointer {
        return;
    }
    // Enabling before/after seats exist must not error; streams bind per seat.
    shell.enable_relative_pointer().expect("enable relative");
    shell.enable_relative_pointer().expect("enable idempotent");
    let _ = shell.dispatch_pending();
    shell.disable_relative_pointer().expect("disable relative");
}

#[test]
fn native_shell_presentation_feedback_api_when_present() {
    let Ok(mut shell) = NativeShell::connect_to_env() else {
        return;
    };
    let id = shell
        .create_toplevel_gpu("pres", "dev.fika.Presentation", 200, 200)
        .expect("toplevel");
    let _ = shell.dispatch_pending();
    // Always safe: no-op without the global.
    shell
        .request_presentation_feedback(id)
        .expect("presentation feedback arm");
    if shell.has_presentation() {
        assert!(shell.capabilities().presentation);
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
    let _ = shell.set_min_size(dialog, Some(crate::geometry::LogicalSize::new(200, 100)));
    let _ = shell.set_max_size(dialog, Some(crate::geometry::LogicalSize::new(800, 600)));
    let _ = shell.set_window_geometry(
        dialog,
        crate::geometry::LogicalPosition::new(0, 0),
        crate::geometry::LogicalSize::new(320, 240),
    );
    let _ = shell.dispatch_pending();
    let _ = shell.destroy_toplevel(dialog);
    let _ = shell.destroy_toplevel(parent);
}
