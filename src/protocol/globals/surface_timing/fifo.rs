use smithay::wayland::compositor::{
    self, Barrier, Cacheable, add_blocker, is_sync_subsurface, with_states,
};
use wayland_protocols::wp::fifo::v1::server::{
    wp_fifo_manager_v1::{self, WpFifoManagerV1},
    wp_fifo_v1::{self, WpFifoV1},
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

use super::AttachResult;

#[derive(Clone, Copy, Debug, Default)]
struct FifoCommitCachedState {
    set_barrier: bool,
    wait_barrier: bool,
}

impl Cacheable for FifoCommitCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        std::mem::take(self)
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        *current = self;
    }
}

#[derive(Debug, Default)]
pub(super) struct FifoBarrierCachedState {
    pub(super) barrier: Option<Barrier>,
}

impl Cacheable for FifoBarrierCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        Self {
            barrier: self.barrier.clone(),
        }
    }

    fn merge_into(mut self, current: &mut Self, _display: &DisplayHandle) {
        let Some(barrier) = self.barrier.take() else {
            return;
        };
        if current.barrier.as_ref() == Some(&barrier) || barrier.is_signaled() {
            return;
        }
        if let Some(previous) = current.barrier.replace(barrier) {
            previous.signal();
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct FifoGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct FifoManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct FifoData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpFifoManagerV1, RuntimeState> for FifoGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpFifoManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, FifoManagerData);
    }
}

impl DispatchDelegate<WpFifoManagerV1, RuntimeState> for FifoManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpFifoManagerV1,
        request: wp_fifo_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_fifo_manager_v1::Request::Destroy => {}
            wp_fifo_manager_v1::Request::GetFifo { id, surface } => {
                let fifo = data_init.init(
                    id,
                    FifoData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .surface_timing
                    .attach_fifo(&surface, &fifo)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_fifo_manager_v1::Error::AlreadyExists,
                        "the surface already has a FIFO object",
                    ),
                    AttachResult::Attached { install_hooks } => {
                        if install_hooks {
                            compositor::add_pre_commit_hook::<RuntimeState, _>(
                                &surface,
                                fifo_pre_commit,
                            );
                            compositor::add_post_commit_hook::<RuntimeState, _>(
                                &surface,
                                fifo_post_commit,
                            );
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpFifoV1, RuntimeState> for FifoData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpFifoV1,
        request: wp_fifo_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_fifo_v1::Request::SetBarrier | wp_fifo_v1::Request::WaitBarrier => {
                let Ok(surface) = self.surface.upgrade() else {
                    resource.post_error(
                        wp_fifo_v1::Error::SurfaceDestroyed,
                        "the associated wl_surface was destroyed",
                    );
                    return;
                };
                with_states(&surface, |states| {
                    let mut cached = states.cached_state.get::<FifoCommitCachedState>();
                    let pending = cached.pending();
                    match request {
                        wp_fifo_v1::Request::SetBarrier => pending.set_barrier = true,
                        wp_fifo_v1::Request::WaitBarrier => pending.wait_barrier = true,
                        _ => unreachable!(),
                    }
                });
            }
            wp_fifo_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &WpFifoV1) {
        self.detach(state, resource);
    }
}

impl FifoData {
    fn detach(&self, state: &RuntimeState, resource: &WpFifoV1) {
        if let Ok(surface) = self.surface.upgrade() {
            state
                .protocol_globals
                .surface_timing
                .detach_fifo(&surface, resource);
        }
    }
}

fn fifo_pre_commit(_state: &mut RuntimeState, _display: &DisplayHandle, surface: &WlSurface) {
    let wait_barrier = with_states(surface, |states| {
        let request = *states.cached_state.get::<FifoCommitCachedState>().pending();
        let wait_barrier = request
            .wait_barrier
            .then(|| {
                states
                    .cached_state
                    .get::<FifoBarrierCachedState>()
                    .pending()
                    .barrier
                    .take()
            })
            .flatten();
        if request.set_barrier {
            states
                .cached_state
                .get::<FifoBarrierCachedState>()
                .pending()
                .barrier = Some(Barrier::new(false));
        }
        wait_barrier
    });
    if let Some(barrier) = wait_barrier
        && !barrier.is_signaled()
        && !is_sync_subsurface(surface)
    {
        add_blocker(surface, barrier);
    }
}

fn fifo_post_commit(state: &mut RuntimeState, _display: &DisplayHandle, surface: &WlSurface) {
    let activation = state.protocol_globals.surface_timing.activate_fifo(surface);
    if !activation.released.is_empty() {
        state.release_surface_barriers(activation.released);
    }
}

delegate_global_dispatch!(RuntimeState, WpFifoManagerV1, FifoGlobalData);
delegate_dispatch!(RuntimeState, WpFifoManagerV1, FifoManagerData);
delegate_dispatch!(RuntimeState, WpFifoV1, FifoData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_releases_previous_barrier() {
        let display = wayland_server::Display::<RuntimeState>::new().unwrap();
        let handle = display.handle();
        let first = Barrier::new(false);
        let second = Barrier::new(false);
        let mut current = FifoBarrierCachedState {
            barrier: Some(first.clone()),
        };
        FifoBarrierCachedState {
            barrier: Some(second.clone()),
        }
        .merge_into(&mut current, &handle);
        assert!(first.is_signaled());
        assert!(!second.is_signaled());
        assert_eq!(current.barrier, Some(second));
    }
}
