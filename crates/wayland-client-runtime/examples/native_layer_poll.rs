//! Protocol-only layer-shell surface with a plain `poll` loop (no Compio).
//!
//! Demonstrates embedding `NativeShell` into an external event loop using only
//! the non-blocking display fd — the path other Wayland projects should use
//! when they already have calloop / tokio / winit / a custom poller.
//!
//! ```sh
//! cargo run -p wayland-client-runtime --example native_layer_poll --no-default-features
//! # or with default features (still uses the protocol path, not Compio waits):
//! cargo run -p wayland-client-runtime --example native_layer_poll
//! ```
//!
//! Creates a bufferless GPU layer (raw-window-handle ready), arms frame
//! callbacks, and exits after configure or a short timeout / ESC.

use std::time::{Duration, Instant};

use rustix::event::{poll, PollFd, PollFlags, Timespec};
use wayland_client_runtime::{
    LayerAnchor, LayerKeyboardInteractivity, LayerSurfaceLayer, LayerSurfaceState, LogicalSize,
    NativeShell, NativeShellEvent, SurfaceRegion,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = match NativeShell::connect_to_env() {
        Ok(s) => s,
        Err(error) => {
            eprintln!("connect failed (no WAYLAND_DISPLAY?): {error}");
            return Ok(());
        }
    };

    if !shell.has_layer_shell() {
        eprintln!("compositor has no zwlr_layer_shell_v1 — nothing to demo");
        return Ok(());
    }

    println!("capabilities: {:?}", shell.capabilities());
    println!("seats: {:?}", shell.seats());
    if let Some(id) = shell.primary_seat_id() {
        println!(
            "primary seat={:?} kb_focus={:?} ptr_focus={:?} serial={:?}",
            id,
            shell.seat_keyboard_focus(id),
            shell.seat_pointer_focus(id),
            shell.seat_last_input_serial(id)
        );
    }

    let state = LayerSurfaceState {
        size: LogicalSize::new(320, 48),
        anchor: LayerAnchor::TOP | LayerAnchor::LEFT | LayerAnchor::RIGHT,
        exclusive_zone: 0,
        exclusive_edge: None,
        margins: Default::default(),
        keyboard_interactivity: LayerKeyboardInteractivity::None,
        layer: LayerSurfaceLayer::Top,
    };
    let surface = shell.create_layer_surface_gpu("wcr-layer-poll", None, state)?;
    // Wallpaper-style: full opaque, empty input (passthrough).
    shell.set_opaque_region(surface, SurfaceRegion::full(320, 48))?;
    shell.set_input_region(surface, SurfaceRegion::Empty)?;
    shell.commit_surface(surface)?;
    shell.request_frame(surface)?;
    let _ = shell.request_presentation_feedback(surface);

    if let Ok(handle) = shell.public_surface_handle(surface) {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        println!(
            "RWH ready: window={:?} display={:?}",
            handle.window_handle().is_ok(),
            handle.display_handle().is_ok()
        );
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut configured = false;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: remaining.subsec_nanos() as i64,
        };
        // BorrowedFd must outlive PollFd.
        let display_fd = shell.display_fd();
        let mut fds = [PollFd::new(&display_fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) => {}
            Err(err) if err == rustix::io::Errno::INTR => continue,
            Err(err) => return Err(std::io::Error::from(err).into()),
        }
        // End PollFd borrow before mutating the shell.
        let _ = (fds, display_fd);

        shell.try_read_and_dispatch()?;
        // Collect first so handlers can re-borrow the shell (same pattern as
        // NativeRuntime::drain_events_into).
        let events: Vec<_> = shell.drain_events().collect();
        for event in events {
            match event {
                NativeShellEvent::LayerConfigure {
                    surface: id,
                    suggested_size,
                    serial,
                } if id == surface => {
                    println!(
                        "layer configure size={suggested_size:?} serial={serial} \
                         logical={:?} buffer={:?} frame_pending={}",
                        shell.logical_size(id),
                        shell.buffer_size(id),
                        shell.is_frame_pending(id)
                    );
                    configured = true;
                    let w = suggested_size.width.unwrap_or(320).max(1);
                    let h = suggested_size.height.unwrap_or(48).max(1);
                    let _ = shell.set_viewport_destination(id, w as i32, h as i32);
                    let _ = shell.set_opaque_region(id, SurfaceRegion::full(w, h));
                    shell.request_frame(id)?;
                    shell.commit_surface(id)?;
                }
                NativeShellEvent::LayerClosed { surface: id } if id == surface => {
                    println!("layer closed");
                    return Ok(());
                }
                NativeShellEvent::Frame { surface: id, time } if id == surface => {
                    println!("frame time={time} pending={}", shell.is_frame_pending(id));
                    if configured {
                        println!("demo complete");
                        let _ = shell.destroy_layer_surface(surface);
                        return Ok(());
                    }
                }
                NativeShellEvent::SeatKeyboardKey { key, pressed, .. } if pressed && key == 1 => {
                    println!("ESC — exit");
                    let _ = shell.destroy_layer_surface(surface);
                    return Ok(());
                }
                NativeShellEvent::SeatAdded { seat, name, .. } => {
                    println!("seat added id={seat} name={name:?}");
                }
                NativeShellEvent::SeatRemoved { seat } => {
                    println!("seat removed id={seat}");
                }
                NativeShellEvent::OutputDone { output } => {
                    if let Some(info) = shell.output_info(output) {
                        println!(
                            "output done id={} name={:?} refresh={:?}",
                            info.id.get(),
                            info.name,
                            info.refresh_hz()
                        );
                    }
                }
                _ => {}
            }
        }
    }

    let _ = shell.destroy_layer_surface(surface);
    println!(
        "exit configured={configured} frame_pending={}",
        shell.is_frame_pending(surface)
    );
    Ok(())
}
