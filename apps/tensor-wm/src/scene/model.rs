use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
    layout::{LayoutPlacement, Rect},
};
use tensor_protocol::SurfacePresentationHint;

use super::{ContentSpan, SurfaceContent};

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

/// A compositor-owned ring around the active view. It is scene data rather
/// than a client decoration, so focus remains visible for both native Wayland
/// and rootless XWayland surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusOutline {
    pub width: u32,
    pub color: LinearRgba16,
}

impl FocusOutline {
    /// Tensor's default active-view treatment: a high-contrast blue ring
    /// outside the client geometry, preserving every client pixel. The
    /// configurable global policy lives in [`super::FocusRingStyle`].
    pub const DEFAULT: Self = Self {
        width: 4,
        color: LinearRgba16::new(0x7f7f, 0xc8c8, u16::MAX, u16::MAX),
    };

    pub const fn visible(self) -> bool {
        self.width > 0 && self.color.alpha > 0
    }

    pub fn bounds(self, geometry: Rect) -> Option<Rect> {
        self.visible().then(|| geometry.inflated(self.width))
    }
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

/// Exact non-overlapping blur rectangles in view-local logical coordinates.
///
/// The immutable shared slice is prepared at the protocol/ECS boundary. Scene
/// snapshots clone only the `Arc`; no protocol-region processing enters frame
/// lowering. Absence on a node means the complete visible view is affected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackdropRegion(Arc<[Rect]>);

impl BackdropRegion {
    pub(crate) fn new(rects: Vec<Rect>) -> Option<Self> {
        (!rects.is_empty()).then(|| Self(rects.into()))
    }

    pub(crate) fn rects(&self) -> &[Rect] {
        &self.0
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneNode {
    pub view_id: ViewId,
    pub stacking_order: u64,
    pub placement: LayoutPlacement,
    pub effects: EffectStyle,
    pub focus_outline: Option<FocusOutline>,
    pub presentation_hint: SurfacePresentationHint,
    backdrop_region: Option<BackdropRegion>,
    content: ContentSpan,
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
            focus_outline: None,
            presentation_hint: SurfacePresentationHint::Vsync,
            backdrop_region: None,
            content: ContentSpan::default(),
        }
    }

    pub(crate) fn with_content(mut self, content: ContentSpan) -> Self {
        self.content = content;
        self
    }

    pub(crate) fn with_focus_outline(mut self, focus_outline: Option<FocusOutline>) -> Self {
        self.focus_outline = focus_outline.filter(|outline| outline.visible());
        self
    }

    pub(crate) fn with_presentation_hint(mut self, hint: SurfacePresentationHint) -> Self {
        self.presentation_hint = hint;
        self
    }

    pub(crate) fn with_backdrop_region(mut self, region: Option<BackdropRegion>) -> Self {
        self.backdrop_region = region;
        self
    }

    pub(crate) fn backdrop_regions(&self, viewport: Rect) -> impl Iterator<Item = Rect> + '_ {
        let visible = self
            .placement
            .visible
            .and_then(|visible| visible.intersection(viewport));
        let complete = self
            .backdrop_region
            .is_none()
            .then_some(visible)
            .flatten()
            .into_iter();
        let explicit = self
            .backdrop_region
            .iter()
            .flat_map(BackdropRegion::rects)
            .filter_map(move |region| {
                region
                    .translated(self.placement.geometry.x, self.placement.geometry.y)
                    .intersection(visible?)
            });
        complete.chain(explicit)
    }

    pub fn visual_bounds(&self, viewport: Rect) -> Option<Rect> {
        if self.effects.opacity == UnitFraction::TRANSPARENT {
            return None;
        }
        let mut bounds = self.placement.geometry;
        if let Some(shadow) = self.effects.shadow.and_then(|shadow| shadow.bounds(bounds)) {
            bounds = bounds.union(shadow);
        }
        if let Some(outline) = self
            .focus_outline
            .and_then(|outline| outline.bounds(self.placement.geometry))
        {
            bounds = bounds.union(outline);
        }
        bounds.intersection(viewport)
    }

    pub const fn samples_background(&self) -> bool {
        self.effects.backdrop_blur.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneSurfaceSubmission {
    pub surface_id: SurfaceId,
    pub view_id: ViewId,
    pub bounds: Rect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneSnapshot {
    pub workspace_id: WorkspaceId,
    pub viewport: Rect,
    nodes: Vec<SceneNode>,
    contents: Vec<SurfaceContent>,
    draw_order: Vec<u32>,
    submitted_surfaces: Arc<[SceneSurfaceSubmission]>,
}

impl SceneSnapshot {
    pub fn new(workspace_id: WorkspaceId, viewport: Rect, nodes: Vec<SceneNode>) -> Self {
        Self::with_content(workspace_id, viewport, nodes, Vec::new())
    }

    pub(crate) fn with_content(
        workspace_id: WorkspaceId,
        viewport: Rect,
        mut nodes: Vec<SceneNode>,
        contents: Vec<SurfaceContent>,
    ) -> Self {
        debug_assert!(nodes.iter().all(|node| {
            node.content
                .range()
                .is_some_and(|range| range.end <= contents.len())
        }));
        nodes.sort_unstable_by_key(|node| node.view_id);
        let mut draw_order = (0..nodes.len())
            .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
            .collect::<Vec<_>>();
        draw_order.sort_unstable_by_key(|index| {
            let node = &nodes[*index as usize];
            (node.stacking_order, node.view_id)
        });
        let mut scene = Self {
            workspace_id,
            viewport,
            nodes,
            contents,
            draw_order,
            submitted_surfaces: Arc::default(),
        };
        scene.submitted_surfaces = build_surface_submissions(&scene);
        scene
    }

    pub fn nodes(&self) -> &[SceneNode] {
        &self.nodes
    }

    pub fn draw_order(&self) -> impl ExactSizeIterator<Item = &SceneNode> {
        self.draw_order
            .iter()
            .map(|index| &self.nodes[*index as usize])
    }

    pub fn contents(&self) -> &[SurfaceContent] {
        &self.contents
    }

    pub fn contents_for(&self, node: &SceneNode) -> &[SurfaceContent] {
        let range = node
            .content
            .range()
            .expect("scene content span was validated during extraction");
        &self.contents[range]
    }

    /// Visible surface membership and unioned logical bounds prepared once
    /// when this immutable snapshot is extracted.
    pub fn submitted_surfaces(&self) -> &[SceneSurfaceSubmission] {
        &self.submitted_surfaces
    }

    /// Return the output-visible bounds of a node and all popup content it
    /// owns.  Layout geometry remains the clip for view content, while popup
    /// surfaces extend the visual region independently until the viewport
    /// boundary.
    pub fn visual_bounds(&self, node: &SceneNode) -> Option<Rect> {
        let mut bounds = node.visual_bounds(self.viewport);
        if node.effects.opacity == UnitFraction::TRANSPARENT {
            return None;
        }
        for content in self
            .contents_for(node)
            .iter()
            .filter(|content| content.layer == super::SurfaceLayer::Popup)
        {
            let destination = content
                .local_geometry
                .translated(node.placement.geometry.x, node.placement.geometry.y);
            let Some(popup_bounds) = destination.intersection(self.viewport) else {
                continue;
            };
            bounds = Some(match bounds {
                Some(bounds) => bounds.union(popup_bounds),
                None => popup_bounds,
            });
        }
        bounds
    }

    /// Return each imported image referenced by the scene once, in stable
    /// scene-table order.  Descriptor allocation and Vulkan descriptor writes
    /// consume the same ordering, so a missing image fails deterministically.
    pub fn buffer_ids(&self) -> Vec<SurfaceBufferId> {
        let mut seen = HashSet::new();
        self.contents
            .iter()
            .filter_map(|content| seen.insert(content.buffer_id).then_some(content.buffer_id))
            .collect()
    }

    pub fn damage_since(&self, previous: Option<&Self>) -> super::DamageSet {
        super::damage::between(previous, self)
    }
}

fn build_surface_submissions(scene: &SceneSnapshot) -> Arc<[SceneSurfaceSubmission]> {
    let mut submitted =
        HashMap::<SurfaceId, SceneSurfaceSubmission>::with_capacity(scene.contents().len());
    for node in scene.draw_order() {
        if scene.visual_bounds(node).is_none() {
            continue;
        }
        for content in scene.contents_for(node) {
            let Some(content_bounds) = submitted_content_bounds(scene, node, content) else {
                continue;
            };
            submitted
                .entry(content.surface_id)
                .and_modify(|surface| {
                    surface.view_id = node.view_id;
                    surface.bounds = surface.bounds.union(content_bounds);
                })
                .or_insert(SceneSurfaceSubmission {
                    surface_id: content.surface_id,
                    view_id: node.view_id,
                    bounds: content_bounds,
                });
        }
    }
    let mut submitted = submitted.into_values().collect::<Vec<_>>();
    submitted.sort_unstable_by_key(|surface| surface.surface_id);
    submitted.into()
}

fn submitted_content_bounds(
    scene: &SceneSnapshot,
    node: &SceneNode,
    content: &SurfaceContent,
) -> Option<Rect> {
    let destination = content
        .local_geometry
        .translated(node.placement.geometry.x, node.placement.geometry.y);
    match content.layer {
        super::SurfaceLayer::View => node
            .placement
            .visible
            .and_then(|clip| destination.intersection(clip)),
        super::SurfaceLayer::Popup => destination.intersection(scene.viewport),
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::{ContentRevision, SurfaceLayer, SurfaceSampleTransform};

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

    #[test]
    fn popup_content_extends_visual_bounds_beyond_the_tile() {
        let viewport = Rect::new(0, 0, 100, 100);
        let placement = LayoutPlacement {
            geometry: Rect::new(10, 10, 20, 20),
            visible: Some(Rect::new(10, 10, 20, 20)),
        };
        let content = SurfaceContent {
            surface_id: crate::ecs::SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(1),
            revision: ContentRevision::new(1),
            layer: SurfaceLayer::Popup,
            alpha: Default::default(),
            color: Default::default(),
            local_geometry: Rect::new(20, 5, 10, 10),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        };
        let span = ContentSpan::new(0, 1).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![SceneNode::new(view(1), 0, placement, EffectStyle::default()).with_content(span)],
            vec![content],
        );

        assert_eq!(
            scene.visual_bounds(&scene.nodes()[0]),
            Some(Rect::new(10, 10, 30, 20))
        );
    }
}
