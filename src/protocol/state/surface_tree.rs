//! Allocation-free surface-tree protocol helpers.
//!
//! Presentation feedback ownership follows Smithay's implementation, while
//! Tensor specializes frame selection to the submitted-surface predicate. See
//! `LICENSES/Smithay-MIT.txt` for the upstream license notice.

use std::time::Duration;

use smithay::{
    utils::{NonNegativeClockSource, Time},
    wayland::compositor::{
        SurfaceAttributes, SurfaceData, TraversalAction, with_surface_tree_downward,
    },
};
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use wayland_server::protocol::wl_surface::WlSurface;

use crate::protocol::globals::{
    output::{Output, WeakOutput},
    presentation::{PresentationFeedbackCachedState, PresentationFeedbackCallback, Refresh},
};

pub(super) fn for_each_surface_tree<F>(surface: &WlSurface, processor: &mut F)
where
    F: FnMut(&WlSurface, &SurfaceData),
{
    with_surface_tree_downward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, states, _| processor(surface, states),
        |_, _, _| true,
    );
}

/// Drain frame callbacks only for surfaces accepted into this submitted frame.
pub(super) fn send_frame_callbacks_surface_tree<F>(
    surface: &WlSurface,
    time: Duration,
    is_submitted: &mut F,
) where
    F: FnMut(&WlSurface, &SurfaceData) -> bool,
{
    with_surface_tree_downward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, states, _| {
            if !is_submitted(surface, states) {
                return;
            }
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time.as_millis() as u32);
            }
        },
        |_, _, _| true,
    );
}

#[derive(Debug)]
struct SurfacePresentationFeedback {
    callbacks: Vec<PresentationFeedbackCallback>,
    flags: wp_presentation_feedback::Kind,
}

impl SurfacePresentationFeedback {
    fn from_states(states: &SurfaceData, flags: wp_presentation_feedback::Kind) -> Option<Self> {
        let mut cached = states.cached_state.get::<PresentationFeedbackCachedState>();
        let callbacks = &mut cached.current().callbacks;
        if callbacks.is_empty() {
            return None;
        }
        Some(Self {
            callbacks: std::mem::take(callbacks),
            flags,
        })
    }

    fn presented(
        &mut self,
        output: &Output,
        clock_id: u32,
        time: Duration,
        refresh: Refresh,
        sequence: u64,
        flags: wp_presentation_feedback::Kind,
    ) {
        for callback in self.callbacks.drain(..) {
            if callback.clock_id() == clock_id {
                callback.presented(output, time, refresh, sequence, flags | self.flags);
            } else {
                callback.discarded();
            }
        }
    }

    fn discard(&mut self) {
        for callback in self.callbacks.drain(..) {
            callback.discarded();
        }
    }
}

impl Drop for SurfacePresentationFeedback {
    fn drop(&mut self) {
        self.discard();
    }
}

/// Presentation callbacks captured for one output and one submitted frame.
#[derive(Debug)]
pub(crate) struct OutputPresentationFeedback {
    output: WeakOutput,
    callbacks: Vec<SurfacePresentationFeedback>,
}

impl OutputPresentationFeedback {
    pub(crate) fn new(output: &Output) -> Self {
        Self {
            output: output.downgrade(),
            callbacks: Vec::new(),
        }
    }

    pub(super) fn presented<T, Kind>(
        &mut self,
        time: T,
        refresh: Refresh,
        sequence: u64,
        flags: wp_presentation_feedback::Kind,
    ) where
        T: Into<Time<Kind>>,
        Kind: NonNegativeClockSource,
    {
        let time = Duration::from(time.into());
        let clock_id = Kind::ID as u32;
        if let Some(output) = self.output.upgrade() {
            for feedback in &mut self.callbacks {
                feedback.presented(&output, clock_id, time, refresh, sequence, flags);
            }
            self.callbacks.clear();
        } else {
            self.discard();
        }
    }

    fn discard(&mut self) {
        for feedback in &mut self.callbacks {
            feedback.discard();
        }
        self.callbacks.clear();
    }
}

pub(super) fn take_presentation_feedback_surface_tree<F1, F2>(
    surface: &WlSurface,
    output_feedback: &mut OutputPresentationFeedback,
    is_submitted: &mut F1,
    feedback_flags: &mut F2,
) where
    F1: FnMut(&WlSurface, &SurfaceData) -> bool,
    F2: FnMut(&WlSurface, &SurfaceData) -> wp_presentation_feedback::Kind,
{
    with_surface_tree_downward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, states, _| {
            if !is_submitted(surface, states) {
                return;
            }
            let flags = feedback_flags(surface, states);
            if let Some(feedback) = SurfacePresentationFeedback::from_states(states, flags) {
                output_feedback.callbacks.push(feedback);
            }
        },
        |_, _, _| true,
    );
}
