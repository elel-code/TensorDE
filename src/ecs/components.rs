use bevy_ecs::prelude::Component;

use crate::layout::Rect;

#[derive(Clone, Copy, Component, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct View {
    pub id: u64,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct Workspace {
    pub id: u32,
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub struct ViewGeometry(pub Rect);
