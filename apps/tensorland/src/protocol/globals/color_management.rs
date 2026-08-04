//! Completion-gated `wp_color_management_v1` v3 ownership.
//!
//! The wire implementation is test-bindable while production advertising
//! remains gated on output HDR encoding and KMS metadata ownership.

mod image;
mod manager;
mod params;
mod surface;

use std::{cell::RefCell, collections::HashMap};

use tensor_protocol::{ImageDescription, ImageDescriptionId, RenderIntent};
use wayland_protocols::wp::color_management::v1::server::{
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_color_manager_v1::WpColorManagerV1, wp_image_description_info_v1::WpImageDescriptionInfoV1,
};
use wayland_server::{
    DisplayHandle, Resource,
    backend::{GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::state::RuntimeState;

pub(super) use image::{ImageDescriptionData, ImageDescriptionReferenceData};
pub(super) use manager::ColorManagerGlobalData;
pub(super) use params::ParametricCreatorData;
pub(super) use surface::{
    ColorManagementOutputData, ColorManagementSurfaceData, SurfaceFeedbackData,
};

const DEFAULT_IMAGE_DESCRIPTION_ID: u64 = 1;

pub(crate) struct ColorManagementProtocol {
    global: Option<GlobalId>,
    surfaces: RefCell<HashMap<ObjectId, SurfaceDescriptionState>>,
    next_description_id: RefCell<u64>,
    default_description: ImageDescription,
    pending_information_done: RefCell<Vec<WpImageDescriptionInfoV1>>,
}

impl ColorManagementProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let default_id = ImageDescriptionId::new(DEFAULT_IMAGE_DESCRIPTION_ID)
            .expect("the fixed image-description identity is non-zero");
        Self {
            // The protocol is intentionally available to wire tests without
            // claiming production HDR/output completion.
            global: cfg!(test).then(|| {
                display
                    .create_global::<RuntimeState, WpColorManagerV1, _>(3, ColorManagerGlobalData)
            }),
            surfaces: RefCell::new(HashMap::new()),
            next_description_id: RefCell::new(DEFAULT_IMAGE_DESCRIPTION_ID + 1),
            default_description: ImageDescription::srgb(default_id),
            pending_information_done: RefCell::new(Vec::new()),
        }
    }

    pub(crate) const fn advertised(&self) -> bool {
        self.global.is_some()
    }

    pub(super) const fn default_description(&self) -> ImageDescription {
        self.default_description
    }

    pub(super) fn allocate_description_id(&self) -> ImageDescriptionId {
        let mut next = self.next_description_id.borrow_mut();
        let value = *next;
        *next = next
            .checked_add(1)
            .expect("wp_color_management_v1 description identity exhausted");
        ImageDescriptionId::new(value).expect("description identity allocator skipped zero")
    }

    pub(super) fn attach_surface(
        &self,
        surface: &WlSurface,
        resource: &WpColorManagementSurfaceV1,
    ) -> AttachResult {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        if state.resource.is_some() {
            return AttachResult::AlreadyExists;
        }
        state.resource = Some(resource.clone());
        let install_hooks = !state.hooks_installed;
        state.hooks_installed = true;
        AttachResult::Attached { install_hooks }
    }

    pub(super) fn detach_surface(
        &self,
        surface: &WlSurface,
        resource: &WpColorManagementSurfaceV1,
    ) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(state) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if state.resource.as_ref().map(Resource::id).as_ref() != Some(&resource.id()) {
            return;
        }
        state.resource = None;
        state.pending = Some(None);
    }

    pub(super) fn set_pending(
        &self,
        surface: &WlSurface,
        description: Option<(ImageDescription, RenderIntent)>,
    ) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending = Some(description);
    }

    pub(super) fn commit_surface(
        &self,
        surface: &WlSurface,
    ) -> Option<Option<(ImageDescription, RenderIntent)>> {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.get_mut(&surface.id())?;
        if let Some(pending) = state.pending.take() {
            state.current = pending;
        }
        Some(state.current)
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    pub(super) fn defer_information_done(&self, info: WpImageDescriptionInfoV1) {
        self.pending_information_done.borrow_mut().push(info);
    }

    pub(crate) fn flush_information_done(&self) {
        for info in self.pending_information_done.borrow_mut().drain(..) {
            if info.is_alive() {
                info.done();
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn first_committed_description(&self) -> Option<(ImageDescription, RenderIntent)> {
        self.surfaces
            .borrow()
            .values()
            .next()
            .and_then(|state| state.current)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AttachResult {
    AlreadyExists,
    Attached { install_hooks: bool },
}

#[derive(Debug, Default)]
struct SurfaceDescriptionState {
    resource: Option<WpColorManagementSurfaceV1>,
    pending: Option<Option<(ImageDescription, RenderIntent)>>,
    current: Option<(ImageDescription, RenderIntent)>,
    hooks_installed: bool,
}
