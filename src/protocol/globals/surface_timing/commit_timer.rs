use smithay::wayland::compositor::{self, add_blocker};
use wayland_protocols::wp::commit_timing::v1::server::{
    wp_commit_timer_v1::{self, WpCommitTimerV1},
    wp_commit_timing_manager_v1::{self, WpCommitTimingManagerV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak, backend::ClientId,
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use super::{AttachResult, Deadline};

#[derive(Debug)]
pub(in crate::protocol) struct CommitTimingGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct CommitTimingManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct CommitTimerData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpCommitTimingManagerV1, RuntimeState> for CommitTimingGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpCommitTimingManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, CommitTimingManagerData);
    }
}

impl DispatchDelegate<WpCommitTimingManagerV1, RuntimeState> for CommitTimingManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpCommitTimingManagerV1,
        request: wp_commit_timing_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_commit_timing_manager_v1::Request::Destroy => {}
            wp_commit_timing_manager_v1::Request::GetTimer { id, surface } => {
                let timer = data_init.init(
                    id,
                    CommitTimerData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .surface_timing
                    .attach_commit_timer(&surface, &timer)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_commit_timing_manager_v1::Error::CommitTimerExists,
                        "the surface already has a commit-timer object",
                    ),
                    AttachResult::Attached { install_hooks } => {
                        if install_hooks {
                            compositor::add_pre_commit_hook::<RuntimeState, _>(
                                &surface,
                                commit_timer_pre_commit,
                            );
                            #[cfg(test)]
                            compositor::add_post_commit_hook::<RuntimeState, _>(
                                &surface,
                                commit_timer_post_commit,
                            );
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpCommitTimerV1, RuntimeState> for CommitTimerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpCommitTimerV1,
        request: wp_commit_timer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_commit_timer_v1::Request::SetTimestamp {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => {
                let Ok(surface) = self.surface.upgrade() else {
                    resource.post_error(
                        wp_commit_timer_v1::Error::SurfaceDestroyed,
                        "the associated wl_surface was destroyed",
                    );
                    return;
                };
                let Some(deadline) = Deadline::from_wire(tv_sec_hi, tv_sec_lo, tv_nsec) else {
                    resource.post_error(
                        wp_commit_timer_v1::Error::InvalidTimestamp,
                        "tv_nsec must be less than one billion",
                    );
                    return;
                };
                if !state
                    .protocol_globals
                    .surface_timing
                    .set_pending_timestamp(&surface, deadline)
                {
                    resource.post_error(
                        wp_commit_timer_v1::Error::TimestampExists,
                        "the surface already has a timestamp for its next commit",
                    );
                }
            }
            wp_commit_timer_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &WpCommitTimerV1) {
        self.detach(state, resource);
    }
}

impl CommitTimerData {
    fn detach(&self, state: &RuntimeState, resource: &WpCommitTimerV1) {
        if let Ok(surface) = self.surface.upgrade() {
            state
                .protocol_globals
                .surface_timing
                .detach_commit_timer(&surface, resource);
        }
    }
}

fn commit_timer_pre_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let Some(deadline) = state
        .protocol_globals
        .surface_timing
        .take_pending_timestamp(surface)
    else {
        return;
    };
    let registration = state
        .protocol_globals
        .surface_timing
        .register_deadline(surface, deadline);
    if !registration.released.is_empty() {
        state.release_surface_barriers(registration.released);
    }
    if !registration.blocker.is_signaled() {
        add_blocker(surface, registration.blocker);
    }
}

#[cfg(test)]
fn commit_timer_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    state
        .protocol_globals
        .surface_timing
        .note_applied_timed_commit(surface);
}

delegate_global_dispatch!(
    RuntimeState,
    WpCommitTimingManagerV1,
    CommitTimingGlobalData
);
delegate_dispatch!(
    RuntimeState,
    WpCommitTimingManagerV1,
    CommitTimingManagerData
);
delegate_dispatch!(RuntimeState, WpCommitTimerV1, CommitTimerData);
