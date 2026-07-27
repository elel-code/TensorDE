//! Tensor-owned `wp_presentation` wire state.
//!
//! The cached surface state still uses Smithay's compositor cache boundary,
//! but the presentation callback itself only knows about a Tensor output and
//! its Wayland resources. This keeps Smithay's `Output` out of the frame path.

use std::time::Duration;

use smithay::wayland::compositor::{Cacheable, with_states};
use wayland_protocols::wp::presentation_time::server::{wp_presentation, wp_presentation_feedback};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, Weak, backend::GlobalId,
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::output::Output,
    state::RuntimeState,
};

pub(crate) struct PresentationProtocol {
    _global: GlobalId,
}

impl PresentationProtocol {
    pub(crate) fn new(display: &DisplayHandle, clock_id: u32) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, wp_presentation::WpPresentation, _>(
                2,
                PresentationGlobalData { clock_id },
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::protocol) struct PresentationGlobalData {
    clock_id: u32,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::protocol) struct PresentationData {
    clock_id: u32,
}

#[derive(Debug)]
pub(in crate::protocol) struct PresentationFeedbackGlobalData;

/// Cached presentation callbacks attached to one surface commit.
#[derive(Debug, Default)]
pub(crate) struct PresentationFeedbackCachedState {
    pub(crate) callbacks: Vec<PresentationFeedbackCallback>,
}

#[derive(Debug)]
pub(crate) struct PresentationFeedbackCallback {
    surface: Weak<WlSurface>,
    clock_id: u32,
    callback: wp_presentation_feedback::WpPresentationFeedback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Refresh {
    Unknown,
    Fixed(Duration),
}

impl PresentationFeedbackCallback {
    pub(crate) fn clock_id(&self) -> u32 {
        self.clock_id
    }

    pub(crate) fn presented(
        self,
        output: &Output,
        time: Duration,
        refresh: Refresh,
        sequence: u64,
        flags: wp_presentation_feedback::Kind,
    ) {
        let Some(surface) = self.surface.upgrade().ok() else {
            self.discarded();
            return;
        };
        let Some(client) = surface.client() else {
            return;
        };
        output.for_each_client_resource(&client, |wl_output| {
            self.callback.sync_output(wl_output);
        });
        let refresh = match refresh {
            Refresh::Fixed(duration) => duration,
            _ => Duration::ZERO,
        };
        let tv_sec_hi = (time.as_secs() >> 32) as u32;
        let tv_sec_lo = (time.as_secs() & 0xffff_ffff) as u32;
        let tv_nsec = time.subsec_nanos();
        let refresh = refresh.as_nanos() as u32;
        let seq_hi = (sequence >> 32) as u32;
        let seq_lo = sequence as u32;
        self.callback.presented(
            tv_sec_hi, tv_sec_lo, tv_nsec, refresh, seq_hi, seq_lo, flags,
        );
    }

    pub(crate) fn discarded(self) {
        self.callback.discarded();
    }
}

impl Cacheable for PresentationFeedbackCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        Self {
            callbacks: std::mem::take(&mut self.callbacks),
        }
    }

    fn merge_into(mut self, into: &mut Self, _display: &DisplayHandle) {
        if self.callbacks.is_empty() {
            return;
        }
        for callback in std::mem::replace(&mut into.callbacks, std::mem::take(&mut self.callbacks))
        {
            callback.discarded();
        }
    }
}

impl Drop for PresentationFeedbackCachedState {
    fn drop(&mut self) {
        for callback in self.callbacks.drain(..) {
            callback.discarded();
        }
    }
}

impl<D> GlobalDispatchDelegate<wp_presentation::WpPresentation, D> for PresentationGlobalData
where
    D: Dispatch<wp_presentation::WpPresentation, PresentationData>
        + Dispatch<wp_presentation_feedback::WpPresentationFeedback, PresentationFeedbackGlobalData>
        + 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<wp_presentation::WpPresentation>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let interface = data_init.init(
            resource,
            PresentationData {
                clock_id: self.clock_id,
            },
        );
        interface.clock_id(self.clock_id);
    }
}

impl<D> DispatchDelegate<wp_presentation::WpPresentation, D> for PresentationData
where
    D: Dispatch<wp_presentation_feedback::WpPresentationFeedback, PresentationFeedbackGlobalData>
        + 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &wp_presentation::WpPresentation,
        request: wp_presentation::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_presentation::Request::Feedback { surface, callback } => {
                let callback = data_init.init(callback, PresentationFeedbackGlobalData);
                with_states(&surface, |states| {
                    states
                        .cached_state
                        .get::<PresentationFeedbackCachedState>()
                        .pending()
                        .callbacks
                        .push(PresentationFeedbackCallback {
                            surface: surface.downgrade(),
                            clock_id: self.clock_id,
                            callback,
                        });
                });
            }
            wp_presentation::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<wp_presentation_feedback::WpPresentationFeedback, D>
    for PresentationFeedbackGlobalData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &wp_presentation_feedback::WpPresentationFeedback,
        _request: wp_presentation_feedback::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
    }
}

delegate_global_dispatch!(
    RuntimeState,
    wp_presentation::WpPresentation,
    PresentationGlobalData
);
delegate_dispatch!(
    RuntimeState,
    wp_presentation::WpPresentation,
    PresentationData
);
delegate_dispatch!(
    RuntimeState,
    wp_presentation_feedback::WpPresentationFeedback,
    PresentationFeedbackGlobalData
);
