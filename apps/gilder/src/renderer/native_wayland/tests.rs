use super::host::NativeWaylandConfigureReadiness;
use super::*;

#[test]
fn reports_experimental_native_capabilities() {
    let capabilities = capabilities();
    assert!(capabilities.built);
    assert!(capabilities.experimental);
    assert!(capabilities.owns_wlr_layer_shell_surface);
    assert!(capabilities.exports_raw_wayland_handles);
    assert!(!capabilities.native_video_overlay);
    assert!(capabilities.probes_linux_dmabuf_protocol);
    assert!(!capabilities.native_dmabuf_buffer_attach);
    assert!(!capabilities.consumes_render_sync);
}

#[test]
fn fractional_scale_rounding_can_match_physical_mode_for_floor_policy() {
    assert_eq!(
        native_scaled_buffer_dimension(1707, 180, 120, NativeWaylandFractionalScaleRounding::Ceil),
        2561
    );
    assert_eq!(
        native_scaled_buffer_dimension(
            1707,
            180,
            120,
            NativeWaylandFractionalScaleRounding::Nearest
        ),
        2561
    );
    assert_eq!(
        native_scaled_buffer_dimension(1707, 180, 120, NativeWaylandFractionalScaleRounding::Floor),
        2560
    );
    assert_eq!(
        native_scaled_buffer_dimension(1067, 180, 120, NativeWaylandFractionalScaleRounding::Floor),
        1600
    );
}

#[test]
fn logical_configure_does_not_become_ready_before_fractional_scale() {
    let readiness = NativeWaylandConfigureReadiness::new(true);

    assert!(!readiness.is_ready(false));
    assert!(!readiness.is_ready(true));
}

#[test]
fn explicit_one_x_preferred_scale_becomes_ready() {
    let mut readiness = NativeWaylandConfigureReadiness::new(true);

    readiness.record_preferred_scale();

    assert!(readiness.is_ready(true));
}

#[test]
fn logical_configure_is_enough_without_fractional_scale_protocol() {
    let readiness = NativeWaylandConfigureReadiness::new(false);

    assert!(readiness.is_ready(true));
}
