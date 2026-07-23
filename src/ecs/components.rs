use bevy_ecs::prelude::Component;

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
/// The component contains no Smithay resources.  A view may eventually carry
/// a root surface plus subsurfaces; keeping a flat value list here makes that
/// extension independent from the renderer and from Bevy entity identity.
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

#[derive(Component, Debug, Default, Eq, PartialEq)]
pub struct Focused;
