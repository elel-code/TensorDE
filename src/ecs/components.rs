use bevy_ecs::prelude::Component;

use super::{ViewId, WorkspaceId};
use crate::layout::Rect;

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

#[derive(Component, Debug, Default, Eq, PartialEq)]
pub struct Focused;
