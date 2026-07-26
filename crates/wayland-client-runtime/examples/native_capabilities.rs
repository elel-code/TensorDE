//! Print capabilities from the SCTK-free [`NativeRuntime`] facade.
//!
//! ```sh
//! cargo run -p wayland-client-runtime --example native_capabilities
//! ```

use wayland_client_runtime::NativeRuntime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = match NativeRuntime::connect() {
        Ok(rt) => rt,
        Err(error) => {
            println!("native runtime unavailable: {error}");
            return Ok(());
        }
    };
    let c = runtime.capabilities();
    println!(
        "native: xdg_dialog_v1={} xdg_activation_v1={} xdg_toplevel_icon_v1={} layer_shell_v1={} text_input_v3={} pointer_constraints_v1={} relative_pointer_v1={} pointer_gestures_v1={} pointer_gesture_hold_v1={} ext_background_effect={} fractional_scale={} cursor_shape={}",
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
        c.cursor_shape
    );
    println!(
        "preferred_toplevel_icon_sizes={:?}",
        runtime.preferred_toplevel_icon_sizes()
    );
    Ok(())
}
