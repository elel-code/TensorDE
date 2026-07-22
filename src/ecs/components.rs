use bevy_ecs::prelude::Component;

use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutItem, LayoutLength, Rect, SizeConstraints};

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
