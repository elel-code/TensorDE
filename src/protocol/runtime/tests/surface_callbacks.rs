use std::time::Duration;

use crate::protocol::globals::compositor::{SurfaceAttributes, with_states};
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;

use crate::protocol::{
    globals::{output::Output, presentation::PresentationFeedbackCachedState},
    state::{OutputPresentationFeedback, PopupManager, ProtocolWindow},
};

pub(super) fn assert_submission_filtering(
    window: &ProtocolWindow,
    popups: &PopupManager,
    output: &Output,
) {
    let root = window.wl_surface().unwrap().into_owned();
    let callback_counts = || {
        with_states(&root, |states| {
            (
                states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .frame_callbacks
                    .len(),
                states
                    .cached_state
                    .get::<PresentationFeedbackCachedState>()
                    .current()
                    .callbacks
                    .len(),
            )
        })
    };
    assert_eq!(callback_counts(), (1, 1));

    window.send_frame(popups, Duration::ZERO, &mut |_, _| false);
    let mut skipped_feedback = OutputPresentationFeedback::new(output);
    window.take_presentation_feedback(
        popups,
        &mut skipped_feedback,
        &mut |_, _| false,
        &mut |_, _| wp_presentation_feedback::Kind::empty(),
    );
    drop(skipped_feedback);
    assert_eq!(callback_counts(), (1, 1));

    window.send_frame(popups, Duration::ZERO, &mut |_, _| true);
    let mut feedback = OutputPresentationFeedback::new(output);
    window.take_presentation_feedback(popups, &mut feedback, &mut |_, _| true, &mut |_, _| {
        wp_presentation_feedback::Kind::empty()
    });
    assert_eq!(callback_counts(), (0, 0));
    drop(feedback);
}
