//! Smoke: SCTK-free native shell + Compio pump.
//!
//! ```sh
//! cargo run -p wayland-client-runtime --example native_toplevel_smoke
//! ```

use std::time::{Duration, Instant};

use wayland_client_runtime::{NativeShell, NativeShellEvent};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = NativeShell::connect_to_env()?;
    println!("capabilities: {:?}", shell.capabilities());
    let surface = shell.create_toplevel("native-toplevel-smoke", "dev.fika.NativeToplevelSmoke")?;
    println!("created surface {surface:?}");
    let _ = shell.request_frame(surface);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut configured = false;

    compio::runtime::Runtime::new()?.block_on(async {
        while Instant::now() < deadline {
            let _ = shell.pump_once().await;
            for event in shell.drain_events() {
                match event {
                    NativeShellEvent::ToplevelConfigure {
                        surface: id,
                        suggested_size,
                    } => {
                        println!("configure {id:?} size={suggested_size:?}");
                        if id == surface {
                            configured = true;
                        }
                    }
                    NativeShellEvent::ToplevelClose { surface: id } => {
                        println!("close {id:?}");
                        return;
                    }
                    NativeShellEvent::SeatKeyboardKey { key, pressed } => {
                        println!("key key={key} pressed={pressed}");
                        if pressed && key == 1 {
                            // ESC
                            return;
                        }
                    }
                    NativeShellEvent::ScaleFactorChanged { surface: id, factor } => {
                        println!("scale {id:?} factor={factor:.3}");
                    }
                    NativeShellEvent::PointerEnter { surface: id, x, y } => {
                        println!("pointer enter {id:?} @ ({x:.1},{y:.1})");
                        if shell.has_cursor_shape() {
                            let _ = shell.set_cursor_shape(Shape::Default);
                        }
                    }
                    NativeShellEvent::PointerMotion { surface: id, x, y } => {
                        println!("pointer motion {id:?} @ ({x:.1},{y:.1})");
                    }
                    NativeShellEvent::SeatKeyboardEnter { surface: id } => {
                        println!("keyboard enter {id:?}");
                    }
                    NativeShellEvent::SeatKeyboardLeave { surface: id } => {
                        println!("keyboard leave {id:?}");
                    }
                    NativeShellEvent::SeatModifiers {
                        mods_depressed, ..
                    } => {
                        println!("modifiers depressed={mods_depressed:#x}");
                    }
                    NativeShellEvent::PointerLeave { surface: id } => {
                        println!("pointer leave {id:?}");
                    }
                    NativeShellEvent::PointerButton {
                        button,
                        pressed,
                        ..
                    } => {
                        println!("pointer button={button} pressed={pressed}");
                    }
                    NativeShellEvent::PointerAxis {
                        horizontal,
                        vertical,
                        ..
                    } => {
                        println!("pointer axis h={horizontal:.2} v={vertical:.2}");
                    }
                }
            }
            if configured {
                // Keep the window mapped briefly so manual smoke is visible.
                compio::runtime::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });

    if configured {
        println!("native toplevel configured successfully");
    } else {
        println!("timed out waiting for configure (compositor may be headless)");
    }
    Ok(())
}
