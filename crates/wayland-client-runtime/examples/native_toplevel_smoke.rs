//! Smoke: SCTK-free native shell + Compio pump.
//!
//! ```sh
//! cargo run -p wayland-client-runtime --example native_toplevel_smoke
//! ```

use std::time::{Duration, Instant};

use wayland_client_runtime::{NativeShell, NativeShellEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shell = NativeShell::connect_to_env()?;
    let surface = shell.create_toplevel("native-toplevel-smoke", "dev.fika.NativeToplevelSmoke")?;
    println!("created surface {surface:?}");

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
