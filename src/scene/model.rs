use crate::{
    ecs::{ViewId, WorkspaceId},
    layout::{LayoutPlacement, Rect},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitFraction(u16);

impl UnitFraction {
    pub const TRANSPARENT: Self = Self(0);
    pub const OPAQUE: Self = Self(u16::MAX);

    pub const fn from_raw(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub fn as_f32(self) -> f32 {
        f32::from(self.0) / f32::from(u16::MAX)
    }
}

impl Default for UnitFraction {
    fn default() -> Self {
        Self::OPAQUE
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinearRgba16 {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
    pub alpha: u16,
}

impl LinearRgba16 {
    pub const fn new(red: u16, green: u16, blue: u16, alpha: u16) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowStyle {
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur_radius: u32,
    pub spread: u32,
    pub color: LinearRgba16,
}

impl ShadowStyle {
    pub fn bounds(self, geometry: Rect) -> Option<Rect> {
        (self.color.alpha > 0).then(|| {
            geometry
                .translated(self.offset_x, self.offset_y)
                .inflated(self.blur_radius.saturating_add(self.spread))
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackdropBlur {
    pub radius: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectStyle {
    pub opacity: UnitFraction,
    pub corner_radius: u32,
    pub shadow: Option<ShadowStyle>,
    pub backdrop_blur: Option<BackdropBlur>,
}

impl EffectStyle {
    pub(crate) fn resolved_for(self, geometry: Rect) -> Self {
        let maximum_radius = geometry.width.min(geometry.height) / 2;
        Self {
            corner_radius: self.corner_radius.min(maximum_radius),
            ..self
        }
    }
}

impl Default for EffectStyle {
    fn default() -> Self {
        Self {
            opacity: UnitFraction::OPAQUE,
            corner_radius: 0,
            shadow: None,
            backdrop_blur: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneNode {
    pub view_id: ViewId,
    pub stacking_order: u64,
    pub placement: LayoutPlacement,
    pub effects: EffectStyle,
}

impl SceneNode {
    pub fn new(
        view_id: ViewId,
        stacking_order: u64,
        placement: LayoutPlacement,
        effects: EffectStyle,
    ) -> Self {
        Self {
            view_id,
            stacking_order,
            placement,
            effects: effects.resolved_for(placement.geometry),
        }
    }

    pub fn visual_bounds(self, viewport: Rect) -> Option<Rect> {
        if self.effects.opacity == UnitFraction::TRANSPARENT {
            return None;
        }
        let mut bounds = self.placement.geometry;
        if let Some(shadow) = self.effects.shadow.and_then(|shadow| shadow.bounds(bounds)) {
            bounds = bounds.union(shadow);
        }
        bounds.intersection(viewport)
    }

    pub const fn samples_background(self) -> bool {
        self.effects.backdrop_blur.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneSnapshot {
    pub workspace_id: WorkspaceId,
    pub viewport: Rect,
    nodes: Vec<SceneNode>,
    draw_order: Vec<u32>,
}

impl SceneSnapshot {
    pub fn new(workspace_id: WorkspaceId, viewport: Rect, mut nodes: Vec<SceneNode>) -> Self {
        nodes.sort_unstable_by_key(|node| node.view_id);
        let mut draw_order = (0..nodes.len())
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect::<Vec<_>>();
        draw_order.sort_unstable_by_key(|index| {
            let node = nodes[*index as usize];
            (node.stacking_order, node.view_id)
        });
        Self {
            workspace_id,
            viewport,
            nodes,
            draw_order,
        }
    }

    pub fn nodes(&self) -> &[SceneNode] {
        &self.nodes
    }

    pub fn draw_order(&self) -> impl ExactSizeIterator<Item = &SceneNode> {
        self.draw_order
            .iter()
            .map(|index| &self.nodes[*index as usize])
    }

    pub fn damage_since(&self, previous: Option<&Self>) -> super::DamageSet {
        super::damage::between(previous, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(value: u64) -> ViewId {
        ViewId::new(value)
    }

    #[test]
    fn stable_nodes_and_draw_order_are_independent() {
        let placement = LayoutPlacement {
            geometry: Rect::new(0, 0, 100, 100),
            visible: Some(Rect::new(0, 0, 100, 100)),
        };
        let snapshot = SceneSnapshot::new(
            WorkspaceId::new(1),
            Rect::new(0, 0, 100, 100),
            vec![
                SceneNode::new(view(2), 1, placement, EffectStyle::default()),
                SceneNode::new(view(1), 2, placement, EffectStyle::default()),
            ],
        );

        assert_eq!(
            snapshot
                .nodes()
                .iter()
                .map(|node| node.view_id)
                .collect::<Vec<_>>(),
            [view(1), view(2)]
        );
        assert_eq!(
            snapshot
                .draw_order()
                .map(|node| node.view_id)
                .collect::<Vec<_>>(),
            [view(2), view(1)]
        );
    }

    #[test]
    fn visual_bounds_include_shadow_and_clamp_corner_radius() {
        let node = SceneNode::new(
            view(1),
            0,
            LayoutPlacement {
                geometry: Rect::new(20, 20, 40, 20),
                visible: Some(Rect::new(20, 20, 40, 20)),
            },
            EffectStyle {
                corner_radius: 100,
                shadow: Some(ShadowStyle {
                    offset_x: 5,
                    offset_y: 3,
                    blur_radius: 4,
                    spread: 2,
                    color: LinearRgba16::new(0, 0, 0, u16::MAX),
                }),
                ..Default::default()
            },
        );

        assert_eq!(node.effects.corner_radius, 10);
        assert_eq!(
            node.visual_bounds(Rect::new(0, 0, 100, 100)),
            Some(Rect::new(19, 17, 52, 32))
        );
    }
}
