use bevy_ecs::prelude::Component;
use tensor_protocol::SurfacePresentationHint;
use tensor_util::Size;

use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutItem, LayoutLength, Rect, SizeConstraints};
use crate::scene::BackdropRegion;
use crate::scene::{EffectStyle, SurfaceContent};

#[derive(Clone, Copy, Component, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct View {
    pub id: ViewId,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
}

/// Original regular workspace retained while a view family lives in the
/// configured hidden minimize workspace.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct MinimizedFrom(pub WorkspaceId);

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct ViewGeometry(pub Rect);

/// Last valid arranged geometry retained across workspace moves for overview
/// inventory. It is never used as current render geometry.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub(super) struct LastViewGeometry(pub Rect);

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct StackingOrder(pub u64);

#[derive(Clone, Copy, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewEffects(pub EffectStyle);

/// Committed presentation hint for a protocol view.
///
/// This is value-only ECS state. The renderer never receives a Wayland
/// resource and KMS still makes the final capability/policy decision.
#[derive(Clone, Copy, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewPresentationHint(pub SurfacePresentationHint);

/// Protocol-supplied background-effect clip, separate from copyable draw
/// style so ordinary surface draw records never clone region ownership.
#[derive(Clone, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewBackdropRegion(pub Option<BackdropRegion>);

/// Renderable surface values extracted from the protocol boundary.
///
/// The component contains no Wayland resources. A view carries its root,
/// subsurfaces, and popups in protocol draw order; the flat value list keeps
/// that tree independent from the renderer and from Bevy entity identity.
#[derive(Clone, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewContent {
    pub surfaces: Vec<SurfaceContent>,
}

impl ViewContent {
    pub fn new(surfaces: Vec<SurfaceContent>) -> Self {
        Self { surfaces }
    }
}

#[derive(Clone, Copy, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewLayout {
    pub constraints: SizeConstraints,
    pub primary_size: Option<LayoutLength>,
}

impl ViewLayout {
    pub const fn item(self) -> LayoutItem {
        LayoutItem::new(self.constraints, self.primary_size)
    }
}

/// Placement policy independent from a layout engine's output geometry.
///
/// Attached views retain their own stable identity, surface tree, and scene
/// node while their geometry follows an owning view. This is the common model
/// for protocol dialogs and future compositor-owned floating relationships;
/// it avoids turning those windows into either tiled peers or untracked
/// renderer-side overlays.
#[derive(Clone, Copy, Component, Debug, Default, Eq, PartialEq)]
pub enum ViewPlacement {
    #[default]
    Tiled,
    /// Independently positioned by an interactive move or product policy.
    Floating {
        geometry: Rect,
    },
    Attached {
        owner: ViewId,
        preferred_size: Size,
    },
}

impl ViewPlacement {
    pub const fn owner(self) -> Option<ViewId> {
        match self {
            Self::Tiled | Self::Floating { .. } => None,
            Self::Attached { owner, .. } => Some(owner),
        }
    }

    pub const fn is_tiled(self) -> bool {
        matches!(self, Self::Tiled)
    }
}

#[derive(Component, Debug, Default, Eq, PartialEq)]
pub struct Focused;
