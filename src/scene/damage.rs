#[cfg(any(feature = "tty", test))]
use tensor_util::OutputScale;
use tensor_util::Rect;

use super::model::{SceneNode, SceneSnapshot};

const MAX_DAMAGE_REGIONS: usize = 64;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DamageSet {
    regions: Vec<Rect>,
}

impl DamageSet {
    pub(crate) fn full(viewport: Rect) -> Self {
        Self {
            regions: non_empty(viewport).into_iter().collect(),
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    pub fn regions(&self) -> &[Rect] {
        &self.regions
    }

    /// Add a region already expressed in the damage set's coordinate space.
    /// Renderer-owned overlays use this after scene damage has crossed the
    /// logical-to-physical boundary.
    #[cfg(feature = "tty")]
    pub(crate) fn add_region(&mut self, region: Rect, viewport: Rect) {
        self.add(region, viewport);
        self.sort();
    }

    #[cfg(any(feature = "tty", test))]
    pub(crate) fn to_physical(
        &self,
        logical_viewport: Rect,
        physical_viewport: Rect,
        scale: OutputScale,
    ) -> Self {
        let regions = self
            .regions
            .iter()
            .filter_map(|region| region.intersection(logical_viewport))
            .map(|region| region.translated(-logical_viewport.x, -logical_viewport.y))
            .map(|region| scale.physical_rect_cover(region))
            .filter_map(|region| region.intersection(physical_viewport))
            .collect();
        Self { regions }
    }

    fn add(&mut self, mut region: Rect, viewport: Rect) {
        let Some(clipped) = region.intersection(viewport) else {
            return;
        };
        region = clipped;
        let mut index = 0;
        while index < self.regions.len() {
            if self.regions[index].touches_or_overlaps(region) {
                region = region.union(self.regions.remove(index));
                index = 0;
            } else {
                index += 1;
            }
        }
        self.regions.push(region);
        if self.regions.len() > MAX_DAMAGE_REGIONS {
            self.regions.clear();
            self.regions.push(viewport);
        }
    }

    fn sort(&mut self) {
        self.regions.sort_unstable_by_key(|rect| (rect.y, rect.x));
    }
}

pub(super) fn between(previous: Option<&SceneSnapshot>, current: &SceneSnapshot) -> DamageSet {
    let Some(previous) = previous else {
        return DamageSet::full(current.viewport);
    };
    if previous.workspace_id != current.workspace_id || previous.viewport != current.viewport {
        return DamageSet::full(current.viewport);
    }

    let mut damage = DamageSet::default();
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < previous.nodes().len() || new_index < current.nodes().len() {
        match (
            previous.nodes().get(old_index),
            current.nodes().get(new_index),
        ) {
            (Some(old), Some(new)) if old.view_id == new.view_id => {
                if old != new || previous.contents_for(old) != current.contents_for(new) {
                    add_node_bounds(&mut damage, previous, *old);
                    add_node_bounds(&mut damage, current, *new);
                }
                old_index += 1;
                new_index += 1;
            }
            (Some(old), Some(new)) if old.view_id < new.view_id => {
                add_node_bounds(&mut damage, previous, *old);
                old_index += 1;
            }
            (Some(_), Some(new)) => {
                add_node_bounds(&mut damage, current, *new);
                new_index += 1;
            }
            (Some(old), None) => {
                add_node_bounds(&mut damage, previous, *old);
                old_index += 1;
            }
            (None, Some(new)) => {
                add_node_bounds(&mut damage, current, *new);
                new_index += 1;
            }
            (None, None) => break,
        }
    }

    propagate_background_dependencies(&mut damage, current);
    damage.sort();
    damage
}

fn add_node_bounds(damage: &mut DamageSet, scene: &SceneSnapshot, node: SceneNode) {
    if let Some(bounds) = scene.visual_bounds(&node) {
        damage.add(bounds, scene.viewport);
    }
}

fn propagate_background_dependencies(damage: &mut DamageSet, current: &SceneSnapshot) {
    let mut changed = true;
    while changed {
        changed = false;
        for node in current
            .nodes()
            .iter()
            .filter(|node| node.samples_background())
        {
            let Some(bounds) = current.visual_bounds(node) else {
                continue;
            };
            if damage
                .regions
                .iter()
                .any(|region| region.intersection(bounds).is_some())
                && !damage
                    .regions
                    .iter()
                    .any(|region| region.contains_rect(bounds))
            {
                damage.add(bounds, current.viewport);
                changed = true;
            }
        }
    }
}

fn non_empty(rect: Rect) -> Option<Rect> {
    (rect.width > 0 && rect.height > 0).then_some(rect)
}

#[cfg(test)]
mod tests {
    use crate::{
        ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{
            BackdropBlur, ContentRevision, ContentSpan, EffectStyle, SceneNode, SceneSnapshot,
            SurfaceContent, SurfaceLayer, SurfaceSampleTransform,
        },
    };

    use super::*;

    const VIEWPORT: Rect = Rect::new(0, 0, 200, 100);

    fn node(id: u64, geometry: Rect, effects: EffectStyle) -> SceneNode {
        SceneNode::new(
            ViewId::new(id),
            id,
            LayoutPlacement {
                geometry,
                visible: geometry.intersection(VIEWPORT),
            },
            effects,
        )
    }

    fn snapshot(nodes: Vec<SceneNode>) -> SceneSnapshot {
        SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, nodes)
    }

    #[test]
    fn unchanged_scene_has_no_damage_and_first_frame_is_full() {
        let scene = snapshot(vec![node(
            1,
            Rect::new(10, 10, 40, 40),
            EffectStyle::default(),
        )]);

        assert_eq!(scene.damage_since(None).regions(), [VIEWPORT]);
        assert!(scene.damage_since(Some(&scene)).regions().is_empty());
    }

    #[test]
    fn logical_damage_covers_fractional_physical_edges() {
        let logical_viewport = Rect::new(200, 100, 100, 80);
        let damage = DamageSet {
            regions: vec![Rect::new(201, 101, 3, 3)],
        };
        let physical = damage.to_physical(
            logical_viewport,
            Rect::new(0, 0, 125, 100),
            OutputScale::from_f64(1.25).unwrap(),
        );
        assert_eq!(physical.regions(), [Rect::new(1, 1, 4, 4)]);

        let full = DamageSet::full(logical_viewport).to_physical(
            logical_viewport,
            Rect::new(0, 0, 125, 100),
            OutputScale::from_f64(1.25).unwrap(),
        );
        assert_eq!(full.regions(), [Rect::new(0, 0, 125, 100)]);
    }

    #[test]
    fn movement_damages_old_and_new_visual_bounds() {
        let old = snapshot(vec![node(
            1,
            Rect::new(10, 10, 20, 20),
            EffectStyle::default(),
        )]);
        let new = snapshot(vec![node(
            1,
            Rect::new(60, 10, 20, 20),
            EffectStyle::default(),
        )]);

        assert_eq!(
            new.damage_since(Some(&old)).regions(),
            [Rect::new(10, 10, 20, 20), Rect::new(60, 10, 20, 20)]
        );
    }

    #[test]
    fn adjacent_damage_is_coalesced() {
        let old = snapshot(vec![node(
            1,
            Rect::new(10, 10, 20, 20),
            EffectStyle::default(),
        )]);
        let new = snapshot(vec![node(
            1,
            Rect::new(30, 10, 20, 20),
            EffectStyle::default(),
        )]);

        assert_eq!(
            new.damage_since(Some(&old)).regions(),
            [Rect::new(10, 10, 40, 20)]
        );
    }

    #[test]
    fn fragmented_damage_falls_back_to_the_viewport() {
        let old = snapshot(Vec::new());
        let nodes = (0..=MAX_DAMAGE_REGIONS)
            .map(|index| {
                node(
                    index as u64 + 1,
                    Rect::new(index as i32 * 3, 0, 1, 1),
                    EffectStyle::default(),
                )
            })
            .collect();
        let new = snapshot(nodes);

        assert_eq!(new.damage_since(Some(&old)).regions(), [VIEWPORT]);
    }

    #[test]
    fn backdrop_blur_propagates_intersecting_background_damage() {
        let blur = EffectStyle {
            backdrop_blur: Some(BackdropBlur { radius: 12 }),
            ..Default::default()
        };
        let old = snapshot(vec![
            node(1, Rect::new(10, 10, 20, 20), EffectStyle::default()),
            node(2, Rect::new(0, 0, 100, 80), blur),
        ]);
        let new = snapshot(vec![
            node(1, Rect::new(15, 10, 20, 20), EffectStyle::default()),
            node(2, Rect::new(0, 0, 100, 80), blur),
        ]);

        assert_eq!(
            new.damage_since(Some(&old)).regions(),
            [Rect::new(0, 0, 100, 80)]
        );
    }

    #[test]
    fn reused_buffer_with_a_new_revision_damages_the_surface() {
        let placement = LayoutPlacement {
            geometry: Rect::new(10, 10, 40, 40),
            visible: Some(Rect::new(10, 10, 40, 40)),
        };
        let content = |revision| SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(2),
            revision: ContentRevision::new(revision),
            layer: SurfaceLayer::View,
            local_geometry: Rect::new(0, 0, 40, 40),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        };
        let span = ContentSpan::new(0, 1).unwrap();
        let old = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(1, placement.geometry, EffectStyle::default()).with_content(span)],
            vec![content(1)],
        );
        let new = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(1, placement.geometry, EffectStyle::default()).with_content(span)],
            vec![content(2)],
        );

        assert_eq!(new.damage_since(Some(&old)).regions(), [placement.geometry]);
    }

    #[test]
    fn popup_motion_damages_both_old_and_new_output_regions() {
        let placement = LayoutPlacement {
            geometry: Rect::new(10, 10, 20, 20),
            visible: Some(Rect::new(10, 10, 20, 20)),
        };
        let content = |x, revision| SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(2),
            revision: ContentRevision::new(revision),
            layer: SurfaceLayer::Popup,
            local_geometry: Rect::new(x, 5, 10, 10),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        };
        let span = ContentSpan::new(0, 1).unwrap();
        let old = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(1, placement.geometry, EffectStyle::default()).with_content(span)],
            vec![content(25, 1)],
        );
        let new = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(1, placement.geometry, EffectStyle::default()).with_content(span)],
            vec![content(55, 2)],
        );

        let damage = new.damage_since(Some(&old));
        assert!(
            damage
                .regions()
                .iter()
                .any(|region| region.contains_rect(Rect::new(35, 15, 10, 10)))
        );
        assert!(
            damage
                .regions()
                .iter()
                .any(|region| region.contains_rect(Rect::new(65, 15, 10, 10)))
        );
    }
}
