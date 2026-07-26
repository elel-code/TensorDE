use calloop::EventLoop;
use smithay::{
    input::keyboard::keysyms,
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_server::Display,
    utils::{Logical, Point, Rectangle},
};

use super::pointer_geometry::{
    constrain_pointer_location, replace_non_finite_pointer_location,
    sanitize_relative_pointer_delta, virtual_terminal_for_keysym,
};
use crate::layout::{LayoutEngine, LayoutKind};
use crate::protocol::state::RuntimeState;

#[test]
fn virtual_terminal_recovery_keys_are_complete_and_bounded() {
    for vt in 1..=12 {
        let keysym = keysyms::KEY_XF86Switch_VT_1 + (vt - 1);
        assert_eq!(virtual_terminal_for_keysym(keysym), Some(vt as i32));
    }
    assert_eq!(
        virtual_terminal_for_keysym(keysyms::KEY_XF86Switch_VT_1 - 1),
        None
    );
    assert_eq!(
        virtual_terminal_for_keysym(keysyms::KEY_XF86Switch_VT_12 + 1),
        None
    );
}

#[test]
fn pointer_location_stays_inside_the_logical_output_edges() {
    let bounds = Rectangle::<i32, Logical>::new((-20, 40).into(), (100, 80).into());

    assert_eq!(
        constrain_pointer_location((-120.0, 999.0).into(), bounds),
        Point::from((-20.0, 119.0))
    );
}

#[test]
fn pointer_location_handles_non_finite_input_without_protocol_escape() {
    let bounds = Rectangle::<i32, Logical>::new((10, 20).into(), (4, 6).into());

    assert_eq!(
        constrain_pointer_location((f64::INFINITY, f64::NAN).into(), bounds),
        Point::from((13.0, 20.0))
    );
}

#[test]
fn relative_pointer_delta_ignores_non_finite_axes() {
    assert_eq!(
        sanitize_relative_pointer_delta((f64::NAN, f64::INFINITY).into()),
        Point::from((0.0, 0.0))
    );
}

#[test]
fn absolute_pointer_location_retains_valid_axes_when_one_axis_is_invalid() {
    assert_eq!(
        replace_non_finite_pointer_location((f64::NAN, 95.0).into(), Point::from((30.0, 40.0)),),
        Point::from((30.0, 95.0))
    );
}

#[test]
fn relative_pointer_crosses_neighboring_outputs_but_not_a_gap() {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        EventLoop::<RuntimeState>::try_new().unwrap().handle(),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        crate::scene::SceneAppearance::default(),
    );
    map_output(&mut state, "left", (0, 0), (100, 100));
    map_output(&mut state, "right", (200, 0), (100, 100));

    assert_eq!(
        state.relative_pointer_location((90.0, 40.0).into(), (30.0, 0.0).into()),
        Some(Point::from((99.0, 40.0))),
        "a gap is clipped to the output that already contains the pointer"
    );
    assert_eq!(
        state.relative_pointer_location((90.0, 40.0).into(), (120.0, 0.0).into()),
        Some(Point::from((210.0, 40.0))),
        "a direct crossing remains possible"
    );
}

fn map_output(state: &mut RuntimeState, name: &str, location: (i32, i32), size: (i32, i32)) {
    let output = Output::new(
        name.to_owned(),
        PhysicalProperties {
            size: (600, 340).into(),
            subpixel: Subpixel::HorizontalRgb,
            make: "Tensor test".to_owned(),
            model: name.to_owned(),
            serial_number: name.to_owned(),
        },
    );
    output.change_current_state(
        Some(Mode {
            size: size.into(),
            refresh: 60_000,
        }),
        None,
        Some(Scale::Integer(1)),
        Some(location.into()),
    );
    state.space.map_output(&output, location);
}
