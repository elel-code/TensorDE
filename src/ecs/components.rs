use bevy_ecs::prelude::Component;
use tensor_util::Size;

use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutItem, LayoutLength, Rect, SizeConstraints};
use crate::scene::{EffectStyle, SurfaceContent};

#[derive(Clone, Copy, Component, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct View {
    pub id: ViewId,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct ViewGeometry(pub Rect);

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct StackingOrder(pub u64);

#[derive(Clone, Copy, Component, Debug, Default, Eq, PartialEq)]
pub struct ViewEffects(pub EffectStyle);

/// Renderable surface values extracted from the protocol boundary.
///
/// The component contains no Smithay resources. A view carries its root,
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
    Attached {
        owner: ViewId,
        preferred_size: Size,
    },
}

impl ViewPlacement {
    pub const fn owner(self) -> Option<ViewId> {
        match self {
            Self::Tiled => None,
            Self::Attached { owner, .. } => Some(owner),
        }
    }

    pub const fn is_tiled(self) -> bool {
        matches!(self, Self::Tiled)
    }
}

#[derive(Component, Debug, Default, Eq, PartialEq)]
pub struct Focused;
