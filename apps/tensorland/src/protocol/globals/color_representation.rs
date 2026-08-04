//! `wp_color_representation_v1` surface metadata and commit validation.

use std::{cell::RefCell, collections::HashMap};

use tensor_protocol::{
    ChromaLocation, ColorAlphaMode, ColorRange, ColorRepresentation, MatrixCoefficients,
};
use wayland_protocols::wp::color_representation::v1::server::{
    wp_color_representation_manager_v1::{self, WpColorRepresentationManagerV1},
    wp_color_representation_surface_v1::{self, WpColorRepresentationSurfaceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::compositor,
    state::{RuntimeState, apply_surface_representation, pending_surface_fourcc},
};

pub(crate) struct ColorRepresentationProtocol {
    global: Option<GlobalId>,
    surfaces: RefCell<HashMap<ObjectId, SurfaceRepresentationState>>,
}

impl ColorRepresentationProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            // Keep real wire coverage available while the global remains
            // product-gated on shader execution and KMS color ownership.
            global: cfg!(test).then(|| {
                display.create_global::<RuntimeState, WpColorRepresentationManagerV1, _>(
                    1,
                    ColorRepresentationGlobalData,
                )
            }),
            surfaces: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) const fn advertised(&self) -> bool {
        self.global.is_some()
    }

    fn attach(
        &self,
        surface: &WlSurface,
        resource: &WpColorRepresentationSurfaceV1,
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

    fn detach(&self, surface: &WlSurface, resource: &WpColorRepresentationSurfaceV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(state) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if state.resource.as_ref().map(Resource::id).as_ref() != Some(&resource.id()) {
            return;
        }
        state.resource = None;
        state.pending = PendingRepresentation::unset_all();
    }

    fn set_alpha(&self, surface: &WlSurface, alpha_mode: ColorAlphaMode) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .alpha_mode = Some(alpha_mode);
    }

    fn set_coefficients(
        &self,
        surface: &WlSurface,
        coefficients: MatrixCoefficients,
        range: ColorRange,
    ) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .coefficients_and_range = Some(Some((coefficients, range)));
    }

    fn set_chroma_location(&self, surface: &WlSurface, location: ChromaLocation) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending
            .chroma_location = Some(Some(location));
    }

    fn pending_representation(&self, surface: &WlSurface) -> Option<ColorRepresentation> {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .map(SurfaceRepresentationState::next)
    }

    fn commit_surface(&self, surface: &WlSurface) -> Option<ColorRepresentation> {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.get_mut(&surface.id())?;
        state.commit();
        Some(state.current)
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    #[cfg(test)]
    pub(crate) fn first_committed(&self) -> ColorRepresentation {
        self.surfaces
            .borrow()
            .values()
            .next()
            .map_or(ColorRepresentation::default(), |state| state.current)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachResult {
    AlreadyExists,
    Attached { install_hooks: bool },
}

#[derive(Debug, Default)]
struct SurfaceRepresentationState {
    resource: Option<WpColorRepresentationSurfaceV1>,
    pending: PendingRepresentation,
    current: ColorRepresentation,
    hooks_installed: bool,
}

impl SurfaceRepresentationState {
    fn next(&self) -> ColorRepresentation {
        ColorRepresentation {
            alpha_mode: self.pending.alpha_mode.unwrap_or(self.current.alpha_mode),
            coefficients_and_range: self
                .pending
                .coefficients_and_range
                .unwrap_or(self.current.coefficients_and_range),
            chroma_location: self
                .pending
                .chroma_location
                .unwrap_or(self.current.chroma_location),
        }
    }

    fn commit(&mut self) {
        self.current = self.next();
        self.pending = PendingRepresentation::default();
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PendingRepresentation {
    alpha_mode: Option<ColorAlphaMode>,
    coefficients_and_range: Option<Option<(MatrixCoefficients, ColorRange)>>,
    chroma_location: Option<Option<ChromaLocation>>,
}

impl PendingRepresentation {
    const fn unset_all() -> Self {
        Self {
            alpha_mode: Some(ColorAlphaMode::PremultipliedElectrical),
            coefficients_and_range: Some(None),
            chroma_location: Some(None),
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ColorRepresentationGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ColorRepresentationManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct ColorRepresentationSurfaceData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpColorRepresentationManagerV1, RuntimeState>
    for ColorRepresentationGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpColorRepresentationManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let manager = data_init.init(resource, ColorRepresentationManagerData);
        manager.supported_alpha_mode(
            wp_color_representation_surface_v1::AlphaMode::PremultipliedElectrical,
        );
        manager.supported_alpha_mode(
            wp_color_representation_surface_v1::AlphaMode::PremultipliedOptical,
        );
        manager.supported_alpha_mode(wp_color_representation_surface_v1::AlphaMode::Straight);
        for range in [
            wp_color_representation_surface_v1::Range::Full,
            wp_color_representation_surface_v1::Range::Limited,
        ] {
            manager.supported_coefficients_and_ranges(
                wp_color_representation_surface_v1::Coefficients::Identity,
                range,
            );
        }
        manager.done();
    }
}

impl DispatchDelegate<WpColorRepresentationManagerV1, RuntimeState>
    for ColorRepresentationManagerData
{
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpColorRepresentationManagerV1,
        request: wp_color_representation_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_representation_manager_v1::Request::Destroy => {}
            wp_color_representation_manager_v1::Request::GetSurface { id, surface } => {
                let resource = data_init.init(
                    id,
                    ColorRepresentationSurfaceData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .color_representation
                    .attach(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_color_representation_manager_v1::Error::SurfaceExists,
                        "the surface already has a color-representation object",
                    ),
                    AttachResult::Attached { install_hooks } => {
                        if install_hooks {
                            compositor::add_pre_commit_hook::<RuntimeState, _>(
                                &surface,
                                representation_pre_commit,
                            );
                            compositor::add_post_commit_hook::<RuntimeState, _>(
                                &surface,
                                representation_post_commit,
                            );
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpColorRepresentationSurfaceV1, RuntimeState>
    for ColorRepresentationSurfaceData
{
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpColorRepresentationSurfaceV1,
        request: wp_color_representation_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_representation_surface_v1::Request::Destroy => self.detach(state, resource),
            wp_color_representation_surface_v1::Request::SetAlphaMode { alpha_mode } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                let Some(alpha_mode) = decode_alpha(alpha_mode) else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::AlphaMode,
                        "unsupported alpha mode",
                    );
                    return;
                };
                state
                    .protocol_globals
                    .color_representation
                    .set_alpha(&surface, alpha_mode);
            }
            wp_color_representation_surface_v1::Request::SetCoefficientsAndRange {
                coefficients,
                range,
            } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                let Some(coefficients) = decode_coefficients(coefficients) else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "unsupported matrix coefficients",
                    );
                    return;
                };
                let Some(range) = decode_range(range) else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "unsupported coefficients/range combination",
                    );
                    return;
                };
                if coefficients != MatrixCoefficients::Identity {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::Coefficients,
                        "Tensorland currently advertises RGB identity coefficients only",
                    );
                    return;
                }
                state
                    .protocol_globals
                    .color_representation
                    .set_coefficients(&surface, coefficients, range);
            }
            wp_color_representation_surface_v1::Request::SetChromaLocation { chroma_location } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                let Some(location) = decode_chroma_location(chroma_location) else {
                    resource.post_error(
                        wp_color_representation_surface_v1::Error::ChromaLocation,
                        "invalid chroma location",
                    );
                    return;
                };
                state
                    .protocol_globals
                    .color_representation
                    .set_chroma_location(&surface, location);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &WpColorRepresentationSurfaceV1,
    ) {
        self.detach(state, resource);
    }
}

impl ColorRepresentationSurfaceData {
    fn surface(&self, resource: &WpColorRepresentationSurfaceV1) -> Option<WlSurface> {
        match self.surface.upgrade() {
            Ok(surface) => Some(surface),
            Err(_) => {
                resource.post_error(
                    wp_color_representation_surface_v1::Error::Inert,
                    "the associated wl_surface was destroyed",
                );
                None
            }
        }
    }

    fn detach(&self, state: &RuntimeState, resource: &WpColorRepresentationSurfaceV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .color_representation
            .detach(&surface, resource);
    }
}

fn representation_pre_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let Some(representation) = state
        .protocol_globals
        .color_representation
        .pending_representation(surface)
    else {
        return;
    };
    let Some(format) = pending_surface_fourcc(surface) else {
        return;
    };
    let compatible = match format {
        tensor_host::Fourcc::XRGB8888
        | tensor_host::Fourcc::ARGB8888
        | tensor_host::Fourcc::XBGR8888
        | tensor_host::Fourcc::ABGR8888
        | tensor_host::Fourcc::XRGB2101010
        | tensor_host::Fourcc::ARGB2101010
        | tensor_host::Fourcc::XBGR2101010
        | tensor_host::Fourcc::ABGR2101010 => {
            representation
                .coefficients_and_range
                .is_none_or(|(coefficients, _)| coefficients == MatrixCoefficients::Identity)
                && representation.chroma_location.is_none()
        }
        tensor_host::Fourcc::NV12 => representation
            .coefficients_and_range
            .is_some_and(|(coefficients, _)| coefficients != MatrixCoefficients::Identity),
        _ => false,
    };
    if compatible {
        return;
    }
    let resource = {
        state
            .protocol_globals
            .color_representation
            .surfaces
            .borrow()
            .get(&surface.id())
            .and_then(|surface| surface.resource.clone())
    };
    if let Some(resource) = resource {
        resource.post_error(
            wp_color_representation_surface_v1::Error::PixelFormat,
            "committed buffer format is incompatible with its color representation",
        );
    }
}

fn representation_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let Some(color) = state
        .protocol_globals
        .color_representation
        .commit_surface(surface)
    else {
        return;
    };
    apply_surface_representation(surface, color);
}

fn decode_alpha(
    value: WEnum<wp_color_representation_surface_v1::AlphaMode>,
) -> Option<ColorAlphaMode> {
    match value {
        WEnum::Value(wp_color_representation_surface_v1::AlphaMode::PremultipliedElectrical) => {
            Some(ColorAlphaMode::PremultipliedElectrical)
        }
        WEnum::Value(wp_color_representation_surface_v1::AlphaMode::PremultipliedOptical) => {
            Some(ColorAlphaMode::PremultipliedOptical)
        }
        WEnum::Value(wp_color_representation_surface_v1::AlphaMode::Straight) => {
            Some(ColorAlphaMode::Straight)
        }
        WEnum::Value(_) | WEnum::Unknown(_) => None,
    }
}

fn decode_coefficients(
    value: WEnum<wp_color_representation_surface_v1::Coefficients>,
) -> Option<MatrixCoefficients> {
    use wp_color_representation_surface_v1::Coefficients as Wire;
    match value {
        WEnum::Value(Wire::Identity) => Some(MatrixCoefficients::Identity),
        WEnum::Value(Wire::Bt709) => Some(MatrixCoefficients::Bt709),
        WEnum::Value(Wire::Fcc) => Some(MatrixCoefficients::Fcc),
        WEnum::Value(Wire::Bt601) => Some(MatrixCoefficients::Bt601),
        WEnum::Value(Wire::Smpte240) => Some(MatrixCoefficients::Smpte240),
        WEnum::Value(Wire::Bt2020) => Some(MatrixCoefficients::Bt2020),
        WEnum::Value(Wire::Bt2020Cl) => Some(MatrixCoefficients::Bt2020ConstantLuminance),
        WEnum::Value(Wire::Ictcp) => Some(MatrixCoefficients::Ictcp),
        WEnum::Value(_) | WEnum::Unknown(_) => None,
    }
}

fn decode_range(value: WEnum<wp_color_representation_surface_v1::Range>) -> Option<ColorRange> {
    match value {
        WEnum::Value(wp_color_representation_surface_v1::Range::Full) => Some(ColorRange::Full),
        WEnum::Value(wp_color_representation_surface_v1::Range::Limited) => {
            Some(ColorRange::Limited)
        }
        WEnum::Value(_) | WEnum::Unknown(_) => None,
    }
}

fn decode_chroma_location(
    value: WEnum<wp_color_representation_surface_v1::ChromaLocation>,
) -> Option<ChromaLocation> {
    use wp_color_representation_surface_v1::ChromaLocation as Wire;
    match value {
        WEnum::Value(Wire::Type0) => Some(ChromaLocation::Type0),
        WEnum::Value(Wire::Type1) => Some(ChromaLocation::Type1),
        WEnum::Value(Wire::Type2) => Some(ChromaLocation::Type2),
        WEnum::Value(Wire::Type3) => Some(ChromaLocation::Type3),
        WEnum::Value(Wire::Type4) => Some(ChromaLocation::Type4),
        WEnum::Value(Wire::Type5) => Some(ChromaLocation::Type5),
        WEnum::Value(_) | WEnum::Unknown(_) => None,
    }
}

delegate_global_dispatch!(
    RuntimeState,
    WpColorRepresentationManagerV1,
    ColorRepresentationGlobalData
);
delegate_dispatch!(
    RuntimeState,
    WpColorRepresentationManagerV1,
    ColorRepresentationManagerData
);
delegate_dispatch!(
    RuntimeState,
    WpColorRepresentationSurfaceV1,
    ColorRepresentationSurfaceData
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representation_is_double_buffered_and_destroy_unsets_every_field() {
        let mut state = SurfaceRepresentationState::default();
        state.pending.alpha_mode = Some(ColorAlphaMode::Straight);
        state.pending.coefficients_and_range =
            Some(Some((MatrixCoefficients::Identity, ColorRange::Limited)));
        assert_eq!(state.current, ColorRepresentation::default());
        state.commit();
        assert_eq!(state.current.alpha_mode, ColorAlphaMode::Straight);
        assert_eq!(
            state.current.coefficients_and_range,
            Some((MatrixCoefficients::Identity, ColorRange::Limited))
        );

        state.pending = PendingRepresentation::unset_all();
        state.commit();
        assert_eq!(state.current, ColorRepresentation::default());
    }

    #[test]
    fn unknown_wire_enums_never_lower_to_product_state() {
        assert_eq!(decode_alpha(WEnum::Unknown(99)), None);
        assert_eq!(decode_coefficients(WEnum::Unknown(99)), None);
        assert_eq!(decode_range(WEnum::Unknown(99)), None);
        assert_eq!(decode_chroma_location(WEnum::Unknown(99)), None);
    }
}
