//! Tensor-owned `wp_linux_drm_syncobj_v1` wire adapter.
//!
//! The protocol validation and surface-hook lifecycle are adapted from
//! Smithay's `wayland::drm_syncobj` implementation. See
//! `LICENSES/Smithay-MIT.txt`.

use std::{
    cell::RefCell,
    os::fd::AsFd,
    sync::{Arc, Weak},
};

use smithay::wayland::{
    Dispatch2, GlobalDispatch2,
    compositor::{self, BufferAssignment, Cacheable, HookId, SurfaceAttributes, with_states},
    dmabuf::get_dmabuf,
};
use tracing::warn;
use wayland_protocols::wp::linux_drm_syncobj::v1::server::{
    wp_linux_drm_syncobj_manager_v1::{self, WpLinuxDrmSyncobjManagerV1},
    wp_linux_drm_syncobj_surface_v1::{self, WpLinuxDrmSyncobjSurfaceV1},
    wp_linux_drm_syncobj_timeline_v1::{self, WpLinuxDrmSyncobjTimelineV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak as WlWeak,
    backend::GlobalId, protocol::wl_surface::WlSurface,
};

use crate::{backend::DrmDeviceFd, protocol::state::RuntimeState};

mod point;

pub(crate) use point::DrmSyncPoint;
use point::{DrmTimeline, DrmTimelineInner};

pub(super) fn supports_syncobj_eventfd(device: &DrmDeviceFd) -> bool {
    match drm_ffi::syncobj::eventfd(device.as_fd(), 0, 0, device.as_fd(), false) {
        Ok(_) => unreachable!("zero syncobj handle unexpectedly accepted"),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

pub(in crate::protocol) trait DrmSyncobjHandler {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState>;
}

pub(super) struct DrmSyncobjGlobalData {
    filter: Box<dyn for<'client> Fn(&'client Client) -> bool + Send + Sync>,
}

#[derive(Debug, Default)]
pub(in crate::protocol) struct DrmSyncobjCachedState {
    pub(in crate::protocol) acquire_point: Option<DrmSyncPoint>,
    pub(in crate::protocol) release_point: Option<DrmSyncPoint>,
}

impl Cacheable for DrmSyncobjCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        Self {
            acquire_point: self.acquire_point.take(),
            release_point: self.release_point.take(),
        }
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        if self.acquire_point.is_some() && self.release_point.is_some() {
            if let Some(release) = &current.release_point
                && let Err(error) = release.signal()
            {
                tracing::error!(%error, "failed to signal superseded syncobj release point");
            }
            current.acquire_point = self.acquire_point;
            current.release_point = self.release_point;
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmSyncobjState {
    _global: GlobalId,
    import_device: Option<DrmDeviceFd>,
    known_timelines: Vec<Weak<DrmTimelineInner>>,
}

impl DrmSyncobjState {
    fn new<D>(display: &DisplayHandle, import_device: DrmDeviceFd) -> Self
    where
        D: GlobalDispatch<WpLinuxDrmSyncobjManagerV1, DrmSyncobjGlobalData> + 'static,
    {
        let global = display.create_global::<D, WpLinuxDrmSyncobjManagerV1, _>(
            1,
            DrmSyncobjGlobalData {
                filter: Box::new(|_| true),
            },
        );
        Self {
            _global: global,
            import_device: Some(import_device),
            known_timelines: Vec::new(),
        }
    }

    fn close_device(&mut self) {
        self.import_device = None;
    }

    fn update_device(&mut self, import_device: DrmDeviceFd) {
        self.known_timelines.retain(|timeline| {
            let Some(timeline) = timeline.upgrade() else {
                return false;
            };
            if let Err(error) = timeline.update_device(&import_device) {
                warn!(%error, "failed to move syncobj timeline to replacement DRM device");
            }
            true
        });
        self.import_device = Some(import_device);
    }
}

impl Drop for DrmSyncobjState {
    fn drop(&mut self) {
        for timeline in self.known_timelines.iter().filter_map(Weak::upgrade) {
            timeline.invalidate();
        }
    }
}

impl<D> GlobalDispatch2<WpLinuxDrmSyncobjManagerV1, D> for DrmSyncobjGlobalData
where
    D: Dispatch<WpLinuxDrmSyncobjManagerV1, DrmSyncobjManagerData>,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpLinuxDrmSyncobjManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, DrmSyncobjManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

fn commit_hook<D: DrmSyncobjHandler>(
    _state: &mut D,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    compositor::with_states(surface, |states| {
        let mut attributes = states.cached_state.get::<SurfaceAttributes>();
        let new_buffer =
            attributes
                .pending()
                .buffer
                .as_ref()
                .and_then(|assignment| match assignment {
                    BufferAssignment::NewBuffer(buffer) => Some(buffer),
                    _ => None,
                });
        let Some(surface_data) = states
            .data_map
            .get::<RefCell<Option<WpLinuxDrmSyncobjSurfaceV1>>>()
        else {
            return;
        };
        let surface_data = surface_data.borrow();
        let Some(syncobj_surface) = surface_data.as_ref() else {
            return;
        };
        let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
        let pending = cached.pending();
        match (
            pending.acquire_point.as_ref(),
            pending.release_point.as_ref(),
            new_buffer,
        ) {
            (Some(_), _, None) => syncobj_surface.post_error(
                wp_linux_drm_syncobj_surface_v1::Error::NoBuffer,
                "acquire point without buffer".to_owned(),
            ),
            (Some(_), None, Some(_)) => syncobj_surface.post_error(
                wp_linux_drm_syncobj_surface_v1::Error::NoReleasePoint,
                "acquire point without release point".to_owned(),
            ),
            (None, Some(_), _) => syncobj_surface.post_error(
                wp_linux_drm_syncobj_surface_v1::Error::NoAcquirePoint,
                "release point without acquire point".to_owned(),
            ),
            (Some(acquire), Some(release), Some(buffer)) => {
                if acquire.timeline == release.timeline && release.point <= acquire.point {
                    syncobj_surface.post_error(
                        wp_linux_drm_syncobj_surface_v1::Error::ConflictingPoints,
                        format!(
                            "release point {} is not greater than acquire point {}",
                            release.point, acquire.point
                        ),
                    );
                }
                if get_dmabuf(buffer).is_err() {
                    syncobj_surface.post_error(
                        wp_linux_drm_syncobj_surface_v1::Error::UnsupportedBuffer,
                        "sync points require a dma-buf buffer".to_owned(),
                    );
                }
            }
            (None, None, _) => {}
        }
    });
}

fn destruction_hook<D: DrmSyncobjHandler>(_state: &mut D, surface: &WlSurface) {
    compositor::with_states(surface, |states| {
        let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
        if let Some(release) = cached.pending().release_point.as_ref()
            && let Err(error) = release.signal()
        {
            tracing::error!(%error, "failed to signal destroyed-surface pending release point");
        }
        if let Some(release) = cached.current().release_point.as_ref()
            && let Err(error) = release.signal()
        {
            tracing::error!(%error, "failed to signal destroyed-surface current release point");
        }
    });
}

#[derive(Debug)]
pub(in crate::protocol) struct DrmSyncobjManagerData;

impl<D> Dispatch2<WpLinuxDrmSyncobjManagerV1, D> for DrmSyncobjManagerData
where
    D: Dispatch<WpLinuxDrmSyncobjSurfaceV1, DrmSyncobjSurfaceData>,
    D: Dispatch<WpLinuxDrmSyncobjTimelineV1, DrmSyncobjTimelineData>,
    D: DrmSyncobjHandler,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        manager: &WpLinuxDrmSyncobjManagerV1,
        request: wp_linux_drm_syncobj_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_linux_drm_syncobj_manager_v1::Request::GetSurface { id, surface } => {
                let exists = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<RefCell<Option<WpLinuxDrmSyncobjSurfaceV1>>>()
                        .is_some_and(|surface| surface.borrow().is_some())
                });
                if exists {
                    manager.post_error(
                        wp_linux_drm_syncobj_manager_v1::Error::SurfaceExists,
                        "surface already has a syncobj protocol object".to_owned(),
                    );
                    return;
                }
                let commit_hook_id = compositor::add_pre_commit_hook::<D, _>(&surface, commit_hook);
                let destruction_hook_id =
                    compositor::add_destruction_hook::<D, _>(&surface, destruction_hook);
                let syncobj_surface = data_init.init(
                    id,
                    DrmSyncobjSurfaceData {
                        surface: surface.downgrade(),
                        commit_hook_id,
                        destruction_hook_id,
                    },
                );
                with_states(&surface, |states| {
                    states
                        .data_map
                        .insert_if_missing(|| RefCell::new(Some(syncobj_surface)));
                });
            }
            wp_linux_drm_syncobj_manager_v1::Request::ImportTimeline { id, fd } => {
                let Some(syncobj_state) = state.drm_syncobj_state() else {
                    manager.post_error(
                        wp_linux_drm_syncobj_manager_v1::Error::InvalidTimeline,
                        "syncobj global is unavailable".to_owned(),
                    );
                    return;
                };
                let Some(device) = syncobj_state.import_device.clone() else {
                    manager.post_error(
                        wp_linux_drm_syncobj_manager_v1::Error::InvalidTimeline,
                        "failed to import syncobj timeline without an active DRM device".to_owned(),
                    );
                    return;
                };
                match DrmTimeline::new(&device, fd) {
                    Ok(timeline) => {
                        syncobj_state
                            .known_timelines
                            .push(Arc::downgrade(&timeline.0));
                        data_init.init(id, DrmSyncobjTimelineData { timeline });
                    }
                    Err(error) => manager.post_error(
                        wp_linux_drm_syncobj_manager_v1::Error::InvalidTimeline,
                        format!("failed to import syncobj timeline: {error}"),
                    ),
                }
            }
            wp_linux_drm_syncobj_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub(super) struct DrmSyncobjSurfaceData {
    surface: WlWeak<WlSurface>,
    commit_hook_id: HookId,
    destruction_hook_id: HookId,
}

impl<D> Dispatch2<WpLinuxDrmSyncobjSurfaceV1, D> for DrmSyncobjSurfaceData
where
    D: DrmSyncobjHandler,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        resource: &WpLinuxDrmSyncobjSurfaceV1,
        request: wp_linux_drm_syncobj_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_linux_drm_syncobj_surface_v1::Request::Destroy => {
                if let Ok(surface) = self.surface.upgrade() {
                    compositor::remove_pre_commit_hook(&surface, &self.commit_hook_id);
                    compositor::remove_destruction_hook(&surface, &self.destruction_hook_id);
                    with_states(&surface, |states| {
                        *states
                            .data_map
                            .get::<RefCell<Option<WpLinuxDrmSyncobjSurfaceV1>>>()
                            .expect("syncobj surface data was installed")
                            .borrow_mut() = None;
                        let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
                        cached.pending().acquire_point = None;
                        if let Some(release) = cached.pending().release_point.take()
                            && let Err(error) = release.signal()
                        {
                            tracing::error!(%error, "failed to signal abandoned release point");
                        }
                    });
                }
            }
            wp_linux_drm_syncobj_surface_v1::Request::SetAcquirePoint {
                timeline,
                point_hi,
                point_lo,
            } => self.set_point(resource, timeline, point_hi, point_lo, true),
            wp_linux_drm_syncobj_surface_v1::Request::SetReleasePoint {
                timeline,
                point_hi,
                point_lo,
            } => self.set_point(resource, timeline, point_hi, point_lo, false),
            _ => unreachable!(),
        }
    }
}

impl DrmSyncobjSurfaceData {
    fn set_point(
        &self,
        resource: &WpLinuxDrmSyncobjSurfaceV1,
        timeline: WpLinuxDrmSyncobjTimelineV1,
        point_hi: u32,
        point_lo: u32,
        acquire: bool,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            resource.post_error(
                wp_linux_drm_syncobj_surface_v1::Error::NoSurface,
                "cannot set a sync point on a destroyed surface".to_owned(),
            );
            return;
        };
        let point = DrmSyncPoint {
            timeline: timeline
                .data::<DrmSyncobjTimelineData>()
                .expect("timeline has Tensor syncobj data")
                .timeline
                .clone(),
            point: (u64::from(point_hi) << 32) | u64::from(point_lo),
        };
        with_states(&surface, |states| {
            let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
            if acquire {
                cached.pending().acquire_point = Some(point);
            } else {
                cached.pending().release_point = Some(point);
            }
        });
    }
}

#[derive(Debug)]
pub(super) struct DrmSyncobjTimelineData {
    timeline: DrmTimeline,
}

impl<D: DrmSyncobjHandler> Dispatch2<WpLinuxDrmSyncobjTimelineV1, D> for DrmSyncobjTimelineData {
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &WpLinuxDrmSyncobjTimelineV1,
        request: wp_linux_drm_syncobj_timeline_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_linux_drm_syncobj_timeline_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut D,
        _client: wayland_server::backend::ClientId,
        _resource: &WpLinuxDrmSyncobjTimelineV1,
    ) {
        if let Some(state) = state.drm_syncobj_state() {
            state.known_timelines.retain(|timeline| {
                timeline
                    .upgrade()
                    .is_some_and(|timeline| !Arc::ptr_eq(&timeline, &self.timeline.0))
            });
        }
    }
}

/// Owns the optional syncobj global and follows the Vulkan-selected DRM device.
#[derive(Debug)]
pub(crate) struct DrmSyncobjProtocol {
    pub(crate) state: Option<DrmSyncobjState>,
    device: Option<DrmDeviceFd>,
    active: bool,
}

impl DrmSyncobjProtocol {
    pub(crate) fn new() -> Self {
        Self {
            state: None,
            device: None,
            active: false,
        }
    }

    pub(crate) fn update(&mut self, display: &DisplayHandle, device: Option<DrmDeviceFd>) {
        let Some(device) = device else {
            self.close_device();
            return;
        };
        if !supports_syncobj_eventfd(&device) {
            self.close_device();
            return;
        }
        if self.device.as_ref() == Some(&device) {
            self.active = true;
            return;
        }
        if let Some(state) = self.state.as_mut() {
            state.update_device(device.clone());
        } else {
            self.state = Some(DrmSyncobjState::new::<RuntimeState>(
                display,
                device.clone(),
            ));
        }
        self.device = Some(device);
        self.active = true;
    }

    pub(crate) fn close_device(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.close_device();
        }
        self.device = None;
        self.active = false;
    }

    pub(crate) fn advertised(&self) -> bool {
        self.state.is_some()
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }
}

impl Default for DrmSyncobjProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_starts_unadvertised_and_inactive() {
        let protocol = DrmSyncobjProtocol::new();
        assert!(!protocol.advertised());
        assert!(!protocol.active());
    }
}
