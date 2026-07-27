//! Print capabilities and seat/output inventory from the native stack.
//!
//! Works with the Compio facade (default features):
//! ```sh
//! cargo run -p wayland-client-runtime --example native_capabilities
//! ```
//!
//! Protocol-only (no Compio):
//! ```sh
//! cargo run -p wayland-client-runtime --example native_capabilities --no-default-features
//! ```

#[cfg(feature = "compio")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use wayland_client_runtime::NativeRuntime;

    let runtime = match NativeRuntime::connect() {
        Ok(rt) => rt,
        Err(error) => {
            println!("native runtime unavailable: {error}");
            return Ok(());
        }
    };
    let c = runtime.capabilities();
    println!(
        "protocols: dialog={} activation={} icon={} layer={} text_input={} \
         constraints={} relative={} gestures={} hold={} blur={} \
         fractional={} cursor_shape={} presentation={} dmabuf={} idle={} \
         idle_notify={} foreign={}",
        c.xdg_dialog_v1,
        c.xdg_activation_v1,
        c.xdg_toplevel_icon_v1,
        c.layer_shell_v1,
        c.text_input_v3,
        c.pointer_constraints_v1,
        c.relative_pointer_v1,
        c.pointer_gestures_v1,
        c.pointer_gesture_hold_v1,
        c.ext_background_effect,
        c.fractional_scale,
        c.cursor_shape,
        c.presentation,
        c.linux_dmabuf,
        c.idle_inhibit,
        c.idle_notify,
        c.xdg_foreign,
    );
    println!("seats ({}):", runtime.seat_count());
    for seat in runtime.seats() {
        println!(
            "  id={} name={:?} kb={} ptr={} touch={}",
            seat.id.get(),
            seat.name,
            seat.has_keyboard,
            seat.has_pointer,
            seat.has_touch
        );
    }
    let mut outputs = Vec::new();
    runtime.outputs_into(&mut outputs);
    println!("outputs ({}):", outputs.len());
    for out in &outputs {
        println!(
            "  id={} name={:?} scale={} refresh={:?}Hz size={:?}",
            out.id.get(),
            out.name,
            out.scale_factor,
            out.refresh_hz(),
            out.logical_size
        );
    }
    println!(
        "preferred_toplevel_icon_sizes={:?}",
        runtime.preferred_toplevel_icon_sizes()
    );
    Ok(())
}

#[cfg(not(feature = "compio"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use wayland_client_runtime::NativeShell;

    let shell = match NativeShell::connect_to_env() {
        Ok(s) => s,
        Err(error) => {
            println!("native shell unavailable: {error}");
            return Ok(());
        }
    };
    let c = shell.capabilities();
    println!(
        "protocol-only: seat={} seat_count={} outputs={} layer={} presentation={} dmabuf={}",
        c.seat,
        c.seat_count,
        c.output_count,
        c.layer_shell,
        c.presentation,
        c.linux_dmabuf
    );
    println!("seats ({}): {:?}", shell.seat_count(), shell.seats());
    let mut outs = Vec::new();
    shell.outputs_into(&mut outs);
    println!("outputs: {outs:?}");
    Ok(())
}
