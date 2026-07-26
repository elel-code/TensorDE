//! Protocol-only xdg-toplevel with a plain `poll` loop (no Compio, no layer-shell).
//!
//! Minimal embedding demo for other Wayland clients: non-blocking display fd,
//! `poll(2)`, bufferless GPU toplevel + raw-window-handle, frame pacing.
//!
//! ```sh
//! cargo run -p wayland-client-runtime --example native_toplevel_poll --no-default-features
//! cargo run -p wayland-client-runtime --example native_toplevel_poll
//! ```

use std::time::{Duration, Instant};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use wayland_client_runtime::{NativeShell, NativeShellEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = match NativeShell::connect_to_env() {
        Ok(s) => s,
        Err(error) => {
            eprintln!("connect failed (no WAYLAND_DISPLAY?): {error}");
            return Ok(());
        }
    };

    println!(
        "caps: seat_count={} presentation={} dmabuf={}",
        shell.capabilities().seat_count,
        shell.capabilities().presentation,
        shell.capabilities().linux_dmabuf
    );
    for seat in shell.seats() {
        println!(
            "  seat {:?} data_device={} primary={}",
            seat.id,
            shell.seat_has_data_device(seat.id),
            shell.seat_has_primary_device(seat.id)
        );
    }

    let surface = shell.create_toplevel_gpu(
        "native-toplevel-poll",
        "dev.example.NativeToplevelPoll",
        480,
        320,
    )?;
    shell.request_frame(surface)?;
    let _ = shell.request_presentation_feedback(surface);

    if let Ok(handle) = shell.public_surface_handle(surface) {
        println!(
            "RWH window={} display={} kind={:?}",
            handle.window_handle().is_ok(),
            handle.display_handle().is_ok(),
            shell.surface_kind(surface)
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
        let display_fd = shell.display_fd();
        let mut fds = [PollFd::new(&display_fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) => {}
            Err(err) if err == rustix::io::Errno::INTR => continue,
            Err(err) => return Err(std::io::Error::from(err).into()),
        }
        let _ = (fds, display_fd);

        shell.try_read_and_dispatch()?;
        let events: Vec<_> = shell.drain_events().collect();
        for event in events {
            match event {
                NativeShellEvent::ToplevelConfigure {
                    surface: id,
                    suggested_size,
                    state,
                    serial,
                } if id == surface => {
                    println!(
                        "configure size={suggested_size:?} state={state:?} serial={serial} \
                         logical={:?} buffer={:?} frame_pending={}",
                        shell.logical_size(id),
                        shell.buffer_size(id),
                        shell.is_frame_pending(id)
                    );
                    configured = true;
                    shell.request_frame(id)?;
                    shell.commit_surface(id)?;
                }
                NativeShellEvent::ToplevelClose { surface: id } if id == surface => {
                    println!("close");
                    return Ok(());
                }
                NativeShellEvent::Frame { surface: id, time } if id == surface => {
                    println!("frame time={time}");
                    if configured {
                        println!("demo complete");
                        let _ = shell.destroy_toplevel(surface);
                        return Ok(());
                    }
                }
                NativeShellEvent::SeatKeyboardKey {
                    key,
                    pressed: true,
                    seat,
                    ..
                } if key == 1 => {
                    println!("ESC seat={seat:?}");
                    let _ = shell.destroy_toplevel(surface);
                    return Ok(());
                }
                NativeShellEvent::PointerEnter {
                    surface: id,
                    x,
                    y,
                    seat,
                } => {
                    println!("pointer enter {id:?} @ ({x:.1},{y:.1}) seat={seat:?}");
                }
                NativeShellEvent::SeatAdded { seat, name, .. } => {
                    println!("seat added {seat} name={name:?}");
                }
                _ => {}
            }
        }
    }

    let _ = shell.destroy_toplevel(surface);
    println!("exit configured={configured}");
    Ok(())
}
