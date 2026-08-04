//! Tensor-owned double-buffered surface metadata protocols.

use std::{cell::RefCell, collections::HashMap};

use super::background_effect::BackgroundRegion;
use super::compositor;
use tensor_protocol::{SurfaceAlpha, SurfaceContentType};
use wayland_protocols::{
    ext::background_effect::v1::server::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
    wp::{
        alpha_modifier::v1::server::{
            wp_alpha_modifier_surface_v1::{self, WpAlphaModifierSurfaceV1},
            wp_alpha_modifier_v1::{self, WpAlphaModifierV1},
        },
        content_type::v1::server::{
            wp_content_type_manager_v1::{self, WpContentTypeManagerV1},
            wp_content_type_v1::{self, WpContentTypeV1},
        },
    },
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{RuntimeState, apply_surface_alpha},
};

pub(crate) struct SurfaceMetadataProtocol {
    _content_type_global: GlobalId,
    _alpha_modifier_global: GlobalId,
    surfaces: RefCell<HashMap<ObjectId, SurfaceMetadataState>>,
}

impl SurfaceMetadataProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let content_type_global = display
            .create_global::<RuntimeState, WpContentTypeManagerV1, _>(1, ContentTypeGlobalData);
        let alpha_modifier_global =
            display.create_global::<RuntimeState, WpAlphaModifierV1, _>(1, AlphaModifierGlobalData);
        Self {
            _content_type_global: content_type_global,
            _alpha_modifier_global: alpha_modifier_global,
            surfaces: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    pub(crate) fn committed_background_region(
        &self,
        surface: &WlSurface,
    ) -> Option<BackgroundRegion> {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .and_then(|state| state.current.background_region.clone())
    }

    #[cfg(test)]
    pub(crate) fn committed_content_type(&self, surface: &WlSurface) -> SurfaceContentType {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .map_or(SurfaceContentType::None, |state| state.current.content_type)
    }

    pub(super) fn has_background(&self, surface: &WlSurface) -> bool {
        self.has_resource(surface, ResourceKind::Background)
    }

    pub(super) fn attach_background(
        &self,
        surface: &WlSurface,
        resource: &ExtBackgroundEffectSurfaceV1,
    ) -> AttachResult {
        self.attach(surface, ResourceKind::Background, resource.id())
    }

    pub(super) fn set_pending_background(
        &self,
        surface: &WlSurface,
        region: Option<BackgroundRegion>,
    ) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .background_region = Some(region);
    }

    pub(super) fn detach_background(
        &self,
        surface: &WlSurface,
        resource: &ExtBackgroundEffectSurfaceV1,
    ) {
        self.detach(surface, ResourceKind::Background, &resource.id());
    }

    fn attach_content(&self, surface: &WlSurface, resource: &WpContentTypeV1) -> AttachResult {
        self.attach(surface, ResourceKind::ContentType, resource.id())
    }

    fn has_content(&self, surface: &WlSurface) -> bool {
        self.has_resource(surface, ResourceKind::ContentType)
    }

    fn set_pending_content(&self, surface: &WlSurface, content_type: SurfaceContentType) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .content_type = Some(content_type);
    }

    fn detach_content(&self, surface: &WlSurface, resource: &WpContentTypeV1) {
        self.detach(surface, ResourceKind::ContentType, &resource.id());
    }

    fn attach_alpha(
        &self,
        surface: &WlSurface,
        resource: &WpAlphaModifierSurfaceV1,
    ) -> AttachResult {
        self.attach(surface, ResourceKind::Alpha, resource.id())
    }

    fn has_alpha(&self, surface: &WlSurface) -> bool {
        self.has_resource(surface, ResourceKind::Alpha)
    }

    fn set_pending_alpha(&self, surface: &WlSurface, alpha: SurfaceAlpha) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .alpha = Some(alpha);
    }

    fn detach_alpha(&self, surface: &WlSurface, resource: &WpAlphaModifierSurfaceV1) {
        self.detach(surface, ResourceKind::Alpha, &resource.id());
    }

    fn attach(&self, surface: &WlSurface, kind: ResourceKind, resource: ObjectId) -> AttachResult {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        if kind.resource(state).is_some() {
            return AttachResult::AlreadyExists;
        }
        *kind.resource_mut(state) = Some(resource);
        let install_hook = !state.commit_hook_installed;
        state.commit_hook_installed = true;
        AttachResult::Attached { install_hook }
    }

    fn has_resource(&self, surface: &WlSurface, kind: ResourceKind) -> bool {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .is_some_and(|state| kind.resource(state).is_some())
    }

    fn detach(&self, surface: &WlSurface, kind: ResourceKind, resource: &ObjectId) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(state) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if kind.resource(state).as_ref() != Some(resource) {
            return;
        }
        *kind.resource_mut(state) = None;
        match kind {
            ResourceKind::ContentType => {
                state.pending.content_type = Some(SurfaceContentType::None);
            }
            ResourceKind::Alpha => state.pending.alpha = Some(SurfaceAlpha::OPAQUE),
            ResourceKind::Background => state.pending.background_region = Some(None),
        }
    }

    fn commit_surface(&self, surface: &WlSurface) -> Option<SurfaceAlpha> {
        self.surfaces.borrow_mut().get_mut(&surface.id())?.commit()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AttachResult {
    AlreadyExists,
    Attached { install_hook: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResourceKind {
    ContentType,
    Alpha,
    Background,
}

impl ResourceKind {
    fn resource(self, state: &SurfaceMetadataState) -> &Option<ObjectId> {
        match self {
            Self::ContentType => &state.content_resource,
            Self::Alpha => &state.alpha_resource,
            Self::Background => &state.background_resource,
        }
    }

    fn resource_mut(self, state: &mut SurfaceMetadataState) -> &mut Option<ObjectId> {
        match self {
            Self::ContentType => &mut state.content_resource,
            Self::Alpha => &mut state.alpha_resource,
            Self::Background => &mut state.background_resource,
        }
    }
}

#[derive(Debug, Default)]
struct SurfaceMetadataState {
    content_resource: Option<ObjectId>,
    alpha_resource: Option<ObjectId>,
    background_resource: Option<ObjectId>,
    pending: PendingMetadata,
    current: CommittedMetadata,
    commit_hook_installed: bool,
}

impl SurfaceMetadataState {
    fn commit(&mut self) -> Option<SurfaceAlpha> {
        let previous_alpha = self.current.alpha;
        if let Some(content_type) = self.pending.content_type.take() {
            self.current.content_type = content_type;
        }
        if let Some(alpha) = self.pending.alpha.take() {
            self.current.alpha = alpha;
        }
        if let Some(region) = self.pending.background_region.take() {
            self.current.background_region = region;
        }
        (self.current.alpha != previous_alpha).then_some(self.current.alpha)
    }
}

#[derive(Debug, Default)]
struct PendingMetadata {
    content_type: Option<SurfaceContentType>,
    alpha: Option<SurfaceAlpha>,
    background_region: Option<Option<BackgroundRegion>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CommittedMetadata {
    content_type: SurfaceContentType,
    alpha: SurfaceAlpha,
    background_region: Option<BackgroundRegion>,
}

#[derive(Debug)]
pub(in crate::protocol) struct ContentTypeGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ContentTypeManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct ContentTypeData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpContentTypeManagerV1, RuntimeState> for ContentTypeGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpContentTypeManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ContentTypeManagerData);
    }
}

impl DispatchDelegate<WpContentTypeManagerV1, RuntimeState> for ContentTypeManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpContentTypeManagerV1,
        request: wp_content_type_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_content_type_manager_v1::Request::Destroy => {}
            wp_content_type_manager_v1::Request::GetSurfaceContentType { id, surface } => {
                if state
                    .protocol_globals
                    .surface_metadata
                    .has_content(&surface)
                {
                    manager.post_error(
                        wp_content_type_manager_v1::Error::AlreadyConstructed,
                        "the surface already has a content-type object",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    ContentTypeData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .surface_metadata
                    .attach_content(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_content_type_manager_v1::Error::AlreadyConstructed,
                        "the surface already has a content-type object",
                    ),
                    AttachResult::Attached { install_hook } => {
                        install_metadata_hook(install_hook, &surface);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpContentTypeV1, RuntimeState> for ContentTypeData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpContentTypeV1,
        request: wp_content_type_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_content_type_v1::Request::SetContentType { content_type } => {
                let wayland_server::WEnum::Value(content_type) = content_type else {
                    return;
                };
                let Ok(surface) = self.surface.upgrade() else {
                    return;
                };
                state
                    .protocol_globals
                    .surface_metadata
                    .set_pending_content(&surface, tensor_content_type(content_type));
            }
            wp_content_type_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, resource: &WpContentTypeV1) {
        self.detach(state, resource);
    }
}

impl ContentTypeData {
    fn detach(&self, state: &RuntimeState, resource: &WpContentTypeV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .surface_metadata
            .detach_content(&surface, resource);
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct AlphaModifierGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct AlphaModifierManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct AlphaModifierData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpAlphaModifierV1, RuntimeState> for AlphaModifierGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpAlphaModifierV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, AlphaModifierManagerData);
    }
}

impl DispatchDelegate<WpAlphaModifierV1, RuntimeState> for AlphaModifierManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpAlphaModifierV1,
        request: wp_alpha_modifier_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_alpha_modifier_v1::Request::Destroy => {}
            wp_alpha_modifier_v1::Request::GetSurface { id, surface } => {
                if state.protocol_globals.surface_metadata.has_alpha(&surface) {
                    manager.post_error(
                        wp_alpha_modifier_v1::Error::AlreadyConstructed,
                        "the surface already has an alpha-modifier object",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    AlphaModifierData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .surface_metadata
                    .attach_alpha(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_alpha_modifier_v1::Error::AlreadyConstructed,
                        "the surface already has an alpha-modifier object",
                    ),
                    AttachResult::Attached { install_hook } => {
                        install_metadata_hook(install_hook, &surface);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpAlphaModifierSurfaceV1, RuntimeState> for AlphaModifierData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpAlphaModifierSurfaceV1,
        request: wp_alpha_modifier_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_alpha_modifier_surface_v1::Request::SetMultiplier { factor } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                state
                    .protocol_globals
                    .surface_metadata
                    .set_pending_alpha(&surface, SurfaceAlpha::from_raw(factor));
            }
            wp_alpha_modifier_surface_v1::Request::Destroy => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                state
                    .protocol_globals
                    .surface_metadata
                    .detach_alpha(&surface, resource);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &WpAlphaModifierSurfaceV1,
    ) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .surface_metadata
            .detach_alpha(&surface, resource);
    }
}

impl AlphaModifierData {
    fn surface(&self, resource: &WpAlphaModifierSurfaceV1) -> Option<WlSurface> {
        match self.surface.upgrade() {
            Ok(surface) => Some(surface),
            Err(_) => {
                resource.post_error(
                    wp_alpha_modifier_surface_v1::Error::NoSurface,
                    "the associated wl_surface was destroyed",
                );
                None
            }
        }
    }
}

fn tensor_content_type(content_type: wp_content_type_v1::Type) -> SurfaceContentType {
    match content_type {
        wp_content_type_v1::Type::None => SurfaceContentType::None,
        wp_content_type_v1::Type::Photo => SurfaceContentType::Photo,
        wp_content_type_v1::Type::Video => SurfaceContentType::Video,
        wp_content_type_v1::Type::Game => SurfaceContentType::Game,
        _ => SurfaceContentType::None,
    }
}

pub(super) fn install_metadata_hook(install: bool, surface: &WlSurface) {
    if install {
        compositor::add_post_commit_hook::<RuntimeState, _>(surface, metadata_post_commit);
    }
}

fn metadata_post_commit(state: &mut RuntimeState, _display: &DisplayHandle, surface: &WlSurface) {
    let Some(alpha) = state
        .protocol_globals
        .surface_metadata
        .commit_surface(surface)
    else {
        return;
    };
    apply_surface_alpha(surface, alpha);
}

delegate_global_dispatch!(RuntimeState, WpContentTypeManagerV1, ContentTypeGlobalData);
delegate_dispatch!(RuntimeState, WpContentTypeManagerV1, ContentTypeManagerData);
delegate_dispatch!(RuntimeState, WpContentTypeV1, ContentTypeData);
delegate_global_dispatch!(RuntimeState, WpAlphaModifierV1, AlphaModifierGlobalData);
delegate_dispatch!(RuntimeState, WpAlphaModifierV1, AlphaModifierManagerData);
delegate_dispatch!(RuntimeState, WpAlphaModifierSurfaceV1, AlphaModifierData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_double_buffered_and_preserves_full_alpha_width() {
        let mut state = SurfaceMetadataState::default();
        state.pending.alpha = Some(SurfaceAlpha::from_raw(0x1234_5678));
        state.pending.content_type = Some(SurfaceContentType::Video);
        assert_eq!(state.current.alpha, SurfaceAlpha::OPAQUE);
        assert_eq!(state.current.content_type, SurfaceContentType::None);

        assert_eq!(state.commit(), Some(SurfaceAlpha::from_raw(0x1234_5678)));
        assert_eq!(state.current.content_type, SurfaceContentType::Video);
        assert_eq!(state.commit(), None);
    }

    #[test]
    fn content_hint_commit_does_not_publish_a_render_update() {
        let mut state = SurfaceMetadataState::default();
        state.pending.content_type = Some(SurfaceContentType::Game);

        assert_eq!(state.commit(), None);
        assert_eq!(state.current.content_type, SurfaceContentType::Game);
    }
}
