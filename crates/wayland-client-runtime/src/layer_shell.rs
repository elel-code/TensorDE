//! wlr-layer-shell public types. 

use bitflags::bitflags;
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
    pub(crate) fn to_wire(self) -> zwlr_layer_surface_v1::Anchor {
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
    pub(crate) fn anchor(self) -> LayerAnchor {
        match self {
            Self::Top => LayerAnchor::TOP,
            Self::Bottom => LayerAnchor::BOTTOM,
            Self::Left => LayerAnchor::LEFT,
            Self::Right => LayerAnchor::RIGHT,
        }
    }

    pub(crate) fn to_wire(self) -> zwlr_layer_surface_v1::Anchor {
        // Exclusive-edge is a single bit; map via the anchor helpers.
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

/// Validate layer state against protocol rules and the bound layer-shell version.
pub(crate) fn validate_layer_state(
    state: &LayerSurfaceState,
    namespace: Option<&str>,
    version: u32,
) -> Result<(), LayerSurfaceError> {
    if namespace.is_some_and(|ns| ns.contains('\0')) {
        return Err(LayerSurfaceError::NamespaceContainsNul);
    }
    if state.size.width == 0
        && !(state.anchor.contains(LayerAnchor::LEFT) && state.anchor.contains(LayerAnchor::RIGHT))
    {
        return Err(LayerSurfaceError::UnconstrainedWidthWithoutAnchors);
    }
    if state.size.height == 0
        && !(state.anchor.contains(LayerAnchor::TOP) && state.anchor.contains(LayerAnchor::BOTTOM))
    {
        return Err(LayerSurfaceError::UnconstrainedHeightWithoutAnchors);
    }
    if state.exclusive_zone < -1 {
        return Err(LayerSurfaceError::InvalidExclusiveZone);
    }
    if let Some(edge) = state.exclusive_edge {
        if version < 5 {
            return Err(LayerSurfaceError::ExclusiveEdgeUnsupported);
        }
        if !state.anchor.contains(edge.anchor()) {
            return Err(LayerSurfaceError::ExclusiveEdgeNotAnchored);
        }
    }
    if matches!(
        state.keyboard_interactivity,
        LayerKeyboardInteractivity::OnDemand
    ) && version < 4
    {
        return Err(LayerSurfaceError::OnDemandKeyboardUnsupported);
    }
    // set_layer is version 2+; creation always sets the initial layer via
    // get_layer_surface, so this only matters for dynamic updates (caller
    // checks DynamicLayerUnsupported separately when layer changes).
    let _ = version;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_zero_width_without_horizontal_anchors() {
        let state = LayerSurfaceState {
            size: LogicalSize::new(0, 32),
            anchor: LayerAnchor::TOP,
            ..Default::default()
        };
        assert!(matches!(
            validate_layer_state(&state, Some("ok"), 5),
            Err(LayerSurfaceError::UnconstrainedWidthWithoutAnchors)
        ));
    }

    #[test]
    fn validate_exclusive_edge_requires_v5_and_matching_anchor() {
        let state = LayerSurfaceState {
            exclusive_edge: Some(LayerEdge::Top),
            anchor: LayerAnchor::BOTTOM,
            ..Default::default()
        };
        assert!(matches!(
            validate_layer_state(&state, None, 4),
            Err(LayerSurfaceError::ExclusiveEdgeUnsupported)
        ));
        assert!(matches!(
            validate_layer_state(&state, None, 5),
            Err(LayerSurfaceError::ExclusiveEdgeNotAnchored)
        ));
        let ok = LayerSurfaceState {
            exclusive_edge: Some(LayerEdge::Top),
            anchor: LayerAnchor::TOP,
            ..Default::default()
        };
        assert!(validate_layer_state(&ok, None, 5).is_ok());
    }
}


