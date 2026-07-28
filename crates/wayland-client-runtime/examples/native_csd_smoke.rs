//! Live smoke: native shell with full client-side decorations.
//!
//! Run under a Wayland compositor:
//! ```text
//! FIKA_WAYLAND_BACKEND=native cargo run -p wayland-client-runtime --example native_csd_smoke
//! ```
//!
//! Creates a solid-color toplevel, requests Client decorations, and pumps
//! events so the CSD titlebar/borders receive pointer input (move/resize/
//! min/max/close).

use wayland_client_runtime::{DecorationPreference, NativeShell};

fn main() {
    let mut shell = match NativeShell::connect_to_env() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("no wayland display: {e}");
            return;
        }
    };

    let caps = shell.capabilities();
    println!(
        "caps: xdg_decoration={} subcompositor={} cursor_shape={}",
        caps.xdg_decoration, caps.subcompositor, caps.cursor_shape
    );

    let id = shell
        .create_toplevel_sized(
            "Fika Native CSD",
            "dev.fika.NativeCsd",
            640,
            480,
            [0xff, 0x2a, 0x4a, 0x6e],
        )
        .expect("toplevel");
    shell
        .set_decorations(id, DecorationPreference::Client)
        .expect("csd");

    println!("surface {id:?} csd_frames={}", shell.csd_frame_count());

    compio::runtime::Runtime::new()
        .expect("compio")
        .block_on(async {
            let mut frames = 0u32;
            loop {
                let _ = shell.pump_once().await;
                let mut events = Vec::new();
                shell.drain_events_into(&mut events);
                for ev in events {
                    match &ev {
                        wayland_client_runtime::NativeShellEvent::ToplevelClose { .. } => {
                            println!("close requested");
                            let _ = shell.destroy_toplevel(id);
                            return;
                        }
                        wayland_client_runtime::NativeShellEvent::ToplevelConfigure {
                            suggested_size,
                            state,
                            ..
                        } => {
                            println!("configure size={suggested_size:?} state={state:?}");
                            let _ = shell.redraw_csd(id);
                        }
                        wayland_client_runtime::NativeShellEvent::Frame { time, .. } => {
                            frames += 1;
                            if frames.is_multiple_of(60) {
                                println!("frame time={time} csd={}", shell.csd_frame_count());
                            }
                            let _ = shell.request_frame(id);
                        }
                        _ => {}
                    }
                }
                if shell.is_configured(id) && frames == 0 {
                    let _ = shell.request_frame(id);
                    frames = 1;
                }
                // Exit after a short idle smoke window if no interactive close.
                if frames > 300 {
                    println!("smoke complete (timeout frames)");
                    let _ = shell.destroy_toplevel(id);
                    return;
                }
            }
        });
}
