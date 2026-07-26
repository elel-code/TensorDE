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
            let events: Vec<_> = shell.drain_events().collect();
            for event in events {
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
                    NativeShellEvent::SeatKeyboardKey {
                        key,
                        pressed,
                        keysym,
                        text,
                    } => {
                        println!(
                            "key key={key} pressed={pressed} keysym={keysym:#x} text={text:?}"
                        );
                        if pressed && key == 1 {
                            // ESC
                            return;
                        }
                    }
                    NativeShellEvent::Selection { mimes } => {
                        println!("selection mimes={mimes:?}");
                    }
                    NativeShellEvent::SelectionCancelled => {
                        println!("selection cancelled");
                    }
                    NativeShellEvent::PopupConfigure {
                        surface: id,
                        x,
                        y,
                        width,
                        height,
                    } => {
                        println!(
                            "popup configure {id:?} @ ({x},{y}) {width}x{height}"
                        );
                    }
                    NativeShellEvent::PopupDone { surface: id } => {
                        println!("popup done {id:?}");
                    }
                    NativeShellEvent::DndEnter {
                        offer,
                        surface: id,
                        x,
                        y,
                        mimes,
                    } => {
                        println!(
                            "dnd enter offer={offer} {id:?} @ ({x:.1},{y:.1}) mimes={mimes:?}"
                        );
                    }
                    NativeShellEvent::DndLeave { offer, surface } => {
                        println!("dnd leave offer={offer} surface={surface:?}");
                    }
                    NativeShellEvent::DndMotion { offer, x, y } => {
                        println!("dnd motion offer={offer} @ ({x:.1},{y:.1})");
                    }
                    NativeShellEvent::DndDrop { offer } => {
                        println!("dnd drop offer={offer}");
                    }
                    NativeShellEvent::DndFinished { source, cancelled } => {
                        println!("dnd finished source={source} cancelled={cancelled}");
                    }
                    NativeShellEvent::TextInputEnter { surface: id } => {
                        println!("text_input enter {id:?}");
                    }
                    NativeShellEvent::TextInputLeave { surface: id } => {
                        println!("text_input leave {id:?}");
                    }
                    NativeShellEvent::TextInputDone {
                        surface: id,
                        serial,
                        commit,
                        preedit,
                        delete_before,
                        delete_after,
                    } => {
                        println!(
                            "text_input done {id:?} serial={serial} commit={commit:?} preedit={preedit:?} del={delete_before}/{delete_after}"
                        );
                    }
                    NativeShellEvent::LayerConfigure {
                        surface: id,
                        suggested_size,
                        serial,
                    } => {
                        println!(
                            "layer configure {id:?} size={suggested_size:?} serial={serial}"
                        );
                    }
                    NativeShellEvent::LayerClosed { surface: id } => {
                        println!("layer closed {id:?}");
                    }
                    NativeShellEvent::ActivationToken { surface: id, token } => {
                        println!("activation token {id:?} token={token}");
                    }
                    NativeShellEvent::GestureSwipeBegin {
                        surface: id,
                        fingers,
                        time,
                    } => println!("swipe begin {id:?} fingers={fingers} time={time}"),
                    NativeShellEvent::GestureSwipeUpdate { dx, dy, time } => {
                        println!("swipe update dx={dx:.2} dy={dy:.2} time={time}");
                    }
                    NativeShellEvent::GestureSwipeEnd { cancelled, time } => {
                        println!("swipe end cancelled={cancelled} time={time}");
                    }
                    NativeShellEvent::GesturePinchBegin {
                        surface: id,
                        fingers,
                        time,
                    } => println!("pinch begin {id:?} fingers={fingers} time={time}"),
                    NativeShellEvent::GesturePinchUpdate {
                        dx,
                        dy,
                        scale,
                        rotation,
                        time,
                    } => println!(
                        "pinch update dx={dx:.2} dy={dy:.2} scale={scale:.3} rot={rotation:.1} time={time}"
                    ),
                    NativeShellEvent::GesturePinchEnd { cancelled, time } => {
                        println!("pinch end cancelled={cancelled} time={time}");
                    }
                    NativeShellEvent::GestureHoldBegin {
                        surface: id,
                        fingers,
                        time,
                    } => println!("hold begin {id:?} fingers={fingers} time={time}"),
                    NativeShellEvent::GestureHoldEnd { cancelled, time } => {
                        println!("hold end cancelled={cancelled} time={time}");
                    }
                    NativeShellEvent::RelativePointer {
                        utime,
                        dx,
                        dy,
                        dx_unaccel,
                        dy_unaccel,
                    } => {
                        println!(
                            "relative motion utime={utime} dx={dx:.3} dy={dy:.3} unaccel=({dx_unaccel:.3},{dy_unaccel:.3})"
                        );
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
                        horizontal_value120,
                        vertical_value120,
                        ..
                    } => {
                        println!(
                            "pointer axis h={horizontal:.2} v={vertical:.2} v120=({horizontal_value120},{vertical_value120})"
                        );
                    }
                    NativeShellEvent::Frame { surface: id, time } => {
                        println!("frame {id:?} time={time}");
                        let _ = shell.request_frame(id);
                    }
                    NativeShellEvent::TouchDown {
                        surface: id,
                        id: finger,
                        x,
                        y,
                    } => {
                        println!("touch down {id:?} finger={finger} @ ({x:.1},{y:.1})");
                    }
                    NativeShellEvent::TouchUp { id: finger } => {
                        println!("touch up finger={finger}");
                    }
                    NativeShellEvent::TouchMotion { id: finger, x, y } => {
                        println!("touch motion finger={finger} @ ({x:.1},{y:.1})");
                    }
                    NativeShellEvent::TouchFrame => {
                        println!("touch frame");
                    }
                    NativeShellEvent::TouchCancel => {
                        println!("touch cancel");
                    }
                    NativeShellEvent::OutputGeometry {
                        output,
                        make,
                        model,
                        ..
                    } => {
                        println!("output {output} geometry make={make:?} model={model:?}");
                    }
                    NativeShellEvent::OutputMode {
                        output,
                        width,
                        height,
                        current,
                        ..
                    } => {
                        println!(
                            "output {output} mode {width}x{height} current={current}"
                        );
                    }
                    NativeShellEvent::OutputScale { output, factor } => {
                        println!("output {output} scale={factor}");
                    }
                    NativeShellEvent::OutputDone { output } => {
                        println!("output {output} done");
                    }
                    NativeShellEvent::SurfaceOutputEnter { surface: id, output } => {
                        println!("surface {id:?} enter output {output}");
                    }
                    NativeShellEvent::SurfaceOutputLeave { surface: id, output } => {
                        println!("surface {id:?} leave output {output}");
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
