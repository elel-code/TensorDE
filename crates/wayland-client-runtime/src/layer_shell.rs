use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bitflags::bitflags;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::{LogicalSize, OutputId, SuggestedSize, SurfaceId};

/// Z-order occupied by a layer surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LayerSurfaceLayer {
    Background,
    Bottom,
    #[default]
    Top,
    Overlay,
}

impl From<LayerSurfaceLayer> for zwlr_layer_shell_v1::Layer {
    fn from(value: LayerSurfaceLayer) -> Self {
        match value {
            LayerSurfaceLayer::Background => Self::Background,
            LayerSurfaceLayer::Bottom => Self::Bottom,
            LayerSurfaceLayer::Top => Self::Top,
            LayerSurfaceLayer::Overlay => Self::Overlay,
        }
    }
}

bitflags! {
    /// Output edges to which a layer surface is anchored.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct LayerAnchor: u8 {
        const TOP = 1 << 0;
        const BOTTOM = 1 << 1;
        const LEFT = 1 << 2;
        const RIGHT = 1 << 3;
    }
}

impl LayerAnchor {
    fn to_wire(self) -> zwlr_layer_surface_v1::Anchor {
        zwlr_layer_surface_v1::Anchor::from_bits_truncate(u32::from(self.bits()))
    }
}

/// A single edge used to disambiguate a positive exclusive zone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LayerEdge {
    Top,
    Bottom,
    Left,
    Right,
}

impl LayerEdge {
    fn anchor(self) -> LayerAnchor {
        match self {
            Self::Top => LayerAnchor::TOP,
            Self::Bottom => LayerAnchor::BOTTOM,
            Self::Left => LayerAnchor::LEFT,
            Self::Right => LayerAnchor::RIGHT,
        }
    }

    fn to_wire(self) -> zwlr_layer_surface_v1::Anchor {
        self.anchor().to_wire()
    }
}

/// Keyboard focus policy for a layer surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LayerKeyboardInteractivity {
    #[default]
    None,
    Exclusive,
    OnDemand,
}

impl From<LayerKeyboardInteractivity> for zwlr_layer_surface_v1::KeyboardInteractivity {
    fn from(value: LayerKeyboardInteractivity) -> Self {
        match value {
            LayerKeyboardInteractivity::None => Self::None,
            LayerKeyboardInteractivity::Exclusive => Self::Exclusive,
            LayerKeyboardInteractivity::OnDemand => Self::OnDemand,
        }
    }
}

/// Surface-local distances from the corresponding anchored output edges.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct LayerMargins {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl LayerMargins {
    pub const fn new(top: i32, right: i32, bottom: i32, left: i32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Complete double-buffered state of a layer surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LayerSurfaceState {
    /// A zero axis asks the compositor to choose it and requires anchors on
    /// both opposing edges of that axis.
    pub size: LogicalSize,
    pub anchor: LayerAnchor,
    /// `-1` ignores other exclusive zones, `0` avoids them, and positive
    /// values reserve that many surface-local units.
    pub exclusive_zone: i32,
    /// v5 edge disambiguation for surfaces anchored to a corner.
    pub exclusive_edge: Option<LayerEdge>,
    pub margins: LayerMargins,
    pub keyboard_interactivity: LayerKeyboardInteractivity,
    pub layer: LayerSurfaceLayer,
}

impl Default for LayerSurfaceState {
    fn default() -> Self {
        Self {
            size: LogicalSize::new(1, 1),
            anchor: LayerAnchor::empty(),
            exclusive_zone: 0,
            exclusive_edge: None,
            margins: LayerMargins::default(),
            keyboard_interactivity: LayerKeyboardInteractivity::None,
            layer: LayerSurfaceLayer::Top,
        }
    }
}

/// Immutable creation attributes and initial double-buffered state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LayerSurfaceAttributes {
    /// Compositor-facing purpose used for layer ordering policy.
    pub namespace: String,
    /// `None` lets the compositor choose the output.
    pub output: Option<OutputId>,
    pub state: LayerSurfaceState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayerSurfaceEvent {
    Configure {
        surface: SurfaceId,
        suggested_size: SuggestedSize,
        serial: u32,
    },
    Closed {
        surface: SurfaceId,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LayerSurfaceError {
    #[error("layer namespace contains a NUL byte")]
    NamespaceContainsNul,
    #[error("a compositor-selected width requires both left and right anchors")]
    UnconstrainedWidthWithoutAnchors,
    #[error("a compositor-selected height requires both top and bottom anchors")]
    UnconstrainedHeightWithoutAnchors,
    #[error("exclusive zone must be -1, zero, or positive")]
    InvalidExclusiveZone,
    #[error("exclusive edge must also be present in the surface anchors")]
    ExclusiveEdgeNotAnchored,
    #[error("the compositor's layer-shell version does not support changing layers")]
    DynamicLayerUnsupported,
    #[error("the compositor's layer-shell version does not support on-demand keyboard focus")]
    OnDemandKeyboardUnsupported,
    #[error("the compositor's layer-shell version does not support exclusive-edge v5")]
    ExclusiveEdgeUnsupported,
    #[error("the layer surface was closed by the compositor")]
    Closed,
}

pub(crate) struct LayerShellManager {
    proxy: zwlr_layer_shell_v1::ZwlrLayerShellV1,
}

impl LayerShellManager {
    pub(crate) fn bind<D>(globals: &GlobalList, qh: &QueueHandle<D>) -> Result<Self, BindError>
    where
        D: Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> + 'static,
    {
        let proxy = globals.bind(qh, 1..=5, ())?;
        Ok(Self { proxy })
    }

    pub(crate) fn version(&self) -> u32 {
        self.proxy.version()
    }

    pub(crate) fn create_surface<D>(
        &self,
        qh: &QueueHandle<D>,
        surface: wl_surface::WlSurface,
        output: Option<&wl_output::WlOutput>,
        attributes: &LayerSurfaceAttributes,
    ) -> Result<LayerProtocolSurface, LayerSurfaceError>
    where
        D: Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, LayerSurfaceData> + 'static,
    {
        validate_attributes(attributes, self.version())?;
        let role = self.proxy.get_layer_surface(
            &surface,
            output,
            attributes.state.layer.into(),
            attributes.namespace.clone(),
            qh,
            LayerSurfaceData {
                wl_surface: surface.clone(),
            },
        );
        let protocol = LayerProtocolSurface {
            wl_surface: surface,
            role,
            state: Mutex::new(wire_default_state(attributes.state.layer)),
            closed: AtomicBool::new(false),
        };
        protocol.apply_state(attributes.state)?;
        Ok(protocol)
    }
}

impl Drop for LayerShellManager {
    fn drop(&mut self) {
        if self.proxy.is_alive() && self.proxy.version() >= 3 {
            self.proxy.destroy();
        }
    }
}

pub(crate) struct LayerProtocolSurface {
    wl_surface: wl_surface::WlSurface,
    role: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    state: Mutex<LayerSurfaceState>,
    closed: AtomicBool,
}

impl LayerProtocolSurface {
    pub(crate) fn wl_surface(&self) -> &wl_surface::WlSurface {
        &self.wl_surface
    }

    pub(crate) fn role(&self) -> &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 {
        &self.role
    }

    pub(crate) fn state(&self) -> LayerSurfaceState {
        *self
            .state
            .lock()
            .expect("layer surface state mutex poisoned")
    }

    pub(crate) fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub(crate) fn apply_state(&self, state: LayerSurfaceState) -> Result<(), LayerSurfaceError> {
        validate_state(&state)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(LayerSurfaceError::Closed);
        }
        let mut current = self
            .state
            .lock()
            .expect("layer surface state mutex poisoned");
        if *current == state {
            return Ok(());
        }
        if current.layer != state.layer && self.role.version() < 2 {
            return Err(LayerSurfaceError::DynamicLayerUnsupported);
        }
        if current.keyboard_interactivity != state.keyboard_interactivity
            && state.keyboard_interactivity == LayerKeyboardInteractivity::OnDemand
            && self.role.version() < 4
        {
            return Err(LayerSurfaceError::OnDemandKeyboardUnsupported);
        }
        if current.exclusive_edge != state.exclusive_edge && self.role.version() < 5 {
            return Err(LayerSurfaceError::ExclusiveEdgeUnsupported);
        }

        if current.size != state.size {
            self.role.set_size(state.size.width, state.size.height);
        }
        if current.anchor != state.anchor {
            self.role.set_anchor(state.anchor.to_wire());
        }
        if current.exclusive_zone != state.exclusive_zone {
            self.role.set_exclusive_zone(state.exclusive_zone);
        }
        if current.exclusive_edge != state.exclusive_edge {
            self.role.set_exclusive_edge(
                state
                    .exclusive_edge
                    .map(LayerEdge::to_wire)
                    .unwrap_or_else(zwlr_layer_surface_v1::Anchor::empty),
            );
        }
        if current.margins != state.margins {
            self.role.set_margin(
                state.margins.top,
                state.margins.right,
                state.margins.bottom,
                state.margins.left,
            );
        }
        if current.keyboard_interactivity != state.keyboard_interactivity {
            self.role
                .set_keyboard_interactivity(state.keyboard_interactivity.into());
        }
        if current.layer != state.layer {
            self.role.set_layer(state.layer.into());
        }
        *current = state;
        Ok(())
    }
}

impl Drop for LayerProtocolSurface {
    fn drop(&mut self) {
        if self.role.is_alive() {
            self.role.destroy();
        }
        if self.wl_surface.is_alive() {
            self.wl_surface.destroy();
        }
    }
}

#[derive(Debug)]
pub(crate) struct LayerSurfaceData {
    wl_surface: wl_surface::WlSurface,
}

impl LayerSurfaceData {
    pub(crate) fn wl_surface(&self) -> &wl_surface::WlSurface {
        &self.wl_surface
    }
}

pub(crate) enum LayerProtocolEvent {
    Configure {
        suggested_size: SuggestedSize,
        serial: u32,
    },
    Closed,
}

pub(crate) fn handle_layer_event(
    role: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    event: zwlr_layer_surface_v1::Event,
) -> Option<LayerProtocolEvent> {
    match event {
        zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } => {
            role.ack_configure(serial);
            Some(LayerProtocolEvent::Configure {
                suggested_size: SuggestedSize::new(
                    (width != 0).then_some(width),
                    (height != 0).then_some(height),
                ),
                serial,
            })
        }
        zwlr_layer_surface_v1::Event::Closed => Some(LayerProtocolEvent::Closed),
        _ => None,
    }
}

fn validate_attributes(
    attributes: &LayerSurfaceAttributes,
    version: u32,
) -> Result<(), LayerSurfaceError> {
    if attributes.namespace.contains('\0') {
        return Err(LayerSurfaceError::NamespaceContainsNul);
    }
    validate_state(&attributes.state)?;
    if attributes.state.keyboard_interactivity == LayerKeyboardInteractivity::OnDemand
        && version < 4
    {
        return Err(LayerSurfaceError::OnDemandKeyboardUnsupported);
    }
    if attributes.state.exclusive_edge.is_some() && version < 5 {
        return Err(LayerSurfaceError::ExclusiveEdgeUnsupported);
    }
    Ok(())
}

fn validate_state(state: &LayerSurfaceState) -> Result<(), LayerSurfaceError> {
    if state.size.width == 0
        && !state
            .anchor
            .contains(LayerAnchor::LEFT | LayerAnchor::RIGHT)
    {
        return Err(LayerSurfaceError::UnconstrainedWidthWithoutAnchors);
    }
    if state.size.height == 0
        && !state
            .anchor
            .contains(LayerAnchor::TOP | LayerAnchor::BOTTOM)
    {
        return Err(LayerSurfaceError::UnconstrainedHeightWithoutAnchors);
    }
    if state.exclusive_zone < -1 {
        return Err(LayerSurfaceError::InvalidExclusiveZone);
    }
    if let Some(edge) = state.exclusive_edge
        && !state.anchor.contains(edge.anchor())
    {
        return Err(LayerSurfaceError::ExclusiveEdgeNotAnchored);
    }
    Ok(())
}

fn wire_default_state(layer: LayerSurfaceLayer) -> LayerSurfaceState {
    LayerSurfaceState {
        size: LogicalSize::default(),
        layer,
        ..LayerSurfaceState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_default_is_valid_and_requests_a_concrete_size() {
        let state = LayerSurfaceState::default();
        assert_eq!(state.size, LogicalSize::new(1, 1));
        assert!(validate_state(&state).is_ok());
    }

    #[test]
    fn compositor_selected_axes_require_opposing_anchors() {
        let mut state = LayerSurfaceState {
            size: LogicalSize::new(0, 24),
            anchor: LayerAnchor::LEFT,
            ..Default::default()
        };
        assert!(matches!(
            validate_state(&state),
            Err(LayerSurfaceError::UnconstrainedWidthWithoutAnchors)
        ));

        state.anchor = LayerAnchor::LEFT | LayerAnchor::RIGHT | LayerAnchor::TOP;
        assert!(validate_state(&state).is_ok());
    }

    #[test]
    fn exclusive_edge_must_be_one_of_the_surface_anchors() {
        let state = LayerSurfaceState {
            anchor: LayerAnchor::TOP,
            exclusive_zone: 32,
            exclusive_edge: Some(LayerEdge::Left),
            ..Default::default()
        };
        assert!(matches!(
            validate_state(&state),
            Err(LayerSurfaceError::ExclusiveEdgeNotAnchored)
        ));
    }

    #[test]
    fn namespaces_and_exclusive_zone_are_validated_before_proxy_creation() {
        let attributes = LayerSurfaceAttributes {
            namespace: "bad\0namespace".into(),
            ..Default::default()
        };
        assert!(matches!(
            validate_attributes(&attributes, 5),
            Err(LayerSurfaceError::NamespaceContainsNul)
        ));

        let state = LayerSurfaceState {
            exclusive_zone: -2,
            ..Default::default()
        };
        assert!(matches!(
            validate_state(&state),
            Err(LayerSurfaceError::InvalidExclusiveZone)
        ));
    }

    #[test]
    fn exclusive_edge_requires_protocol_v5() {
        let attributes = LayerSurfaceAttributes {
            state: LayerSurfaceState {
                anchor: LayerAnchor::TOP,
                exclusive_edge: Some(LayerEdge::Top),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            validate_attributes(&attributes, 4),
            Err(LayerSurfaceError::ExclusiveEdgeUnsupported)
        ));
        assert!(validate_attributes(&attributes, 5).is_ok());
    }

    #[test]
    fn on_demand_keyboard_focus_requires_protocol_v4() {
        let attributes = LayerSurfaceAttributes {
            state: LayerSurfaceState {
                keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            validate_attributes(&attributes, 3),
            Err(LayerSurfaceError::OnDemandKeyboardUnsupported)
        ));
        assert!(validate_attributes(&attributes, 4).is_ok());
    }
}
