use std::collections::HashMap;

use tensor_util::Rect;

use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId},
    render::{CursorOverlay, CursorOverlays},
    scene::{
        ContentRevision, EffectStyle, FocusOutline, LinearRgba16, SceneSnapshot, SurfaceAlpha,
        SurfaceLayer, SurfaceSampleTransform,
    },
};

use super::{FrameError, NativeOutputTarget};

/// Value-only draw plan produced after ECS extraction and before Vulkan handle
/// resolution.  Image descriptor indices are stable for the lifetime of one
/// frame and begin after descriptor slot zero, which is reserved for the
/// native output image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FrameDrawPlan {
    images: Vec<SurfaceBufferId>,
    draws: Vec<SurfaceDraw>,
    focus_rings: Vec<FocusRingDraw>,
    scene_draws: Vec<SceneDrawCommand>,
    cursors: CursorOverlays,
    cursor_image_descriptors: Vec<Option<u32>>,
}

/// One compositor scene command in back-to-front order.
///
/// The payloads live in their specialized arrays so descriptor preparation can
/// remain batch-oriented. The command stream is the authoritative ordering
/// boundary: a view's focus ring is emitted before that view's client tree,
/// including popups, and later scene nodes naturally cover earlier nodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneDrawCommand {
    Client(usize),
    FocusRing(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SurfaceDraw {
    pub(crate) view_id: ViewId,
    pub(crate) surface_id: SurfaceId,
    pub(crate) revision: ContentRevision,
    pub(crate) image_descriptor: u32,
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
    pub(crate) effects: EffectStyle,
    pub(crate) alpha: SurfaceAlpha,
    pub(crate) sample_transform: SurfaceSampleTransform,
}

/// A compositor-owned rounded outline around one active view.
///
/// Both rectangles use output-local physical coordinates. Keeping the inner
/// geometry explicitly avoids independent width rounding at fractional scale:
/// the shader can cut the exact client-shaped hole out of the outer rounded
/// rectangle. The draw needs no sampled-image descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FocusRingDraw {
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
    pub(crate) inner: Rect,
    pub(crate) color: LinearRgba16,
    pub(crate) outer_radius: u32,
    pub(crate) inner_radius: u32,
}

impl FrameDrawPlan {
    #[cfg(test)]
    pub(crate) fn build(
        scene: &SceneSnapshot,
        target: NativeOutputTarget,
    ) -> Result<Self, FrameError> {
        Self::build_with_cursors(scene, target, CursorOverlays::default())
    }

    pub(crate) fn build_with_cursors(
        scene: &SceneSnapshot,
        target: NativeOutputTarget,
        cursors: CursorOverlays,
    ) -> Result<Self, FrameError> {
        let mut images = Vec::new();
        let mut image_descriptors = HashMap::new();
        let mut draws = Vec::new();
        let mut focus_rings = Vec::new();
        let mut scene_draws = Vec::new();
        let output_viewport = Rect::new(0, 0, scene.viewport.width, scene.viewport.height);

        for node in scene.draw_order() {
            if scene.visual_bounds(node).is_none() {
                continue;
            }
            if let Some(outline) = node.focus_outline
                && let Some(ring) = focus_ring_draw(
                    outline,
                    node.placement.geometry,
                    node.effects.corner_radius,
                    scene.viewport,
                    target,
                )
            {
                let index = focus_rings.len();
                focus_rings.push(ring);
                // Niri's tile stream places the ring behind its client tree:
                // client content and popups must cover it where they overlap.
                scene_draws.push(SceneDrawCommand::FocusRing(index));
            }
            let view_clip = node
                .placement
                .visible
                .map(|clip| clip.translated(-scene.viewport.x, -scene.viewport.y));
            for content in scene.contents_for(node) {
                if content.alpha == SurfaceAlpha::TRANSPARENT {
                    continue;
                }
                let logical_destination = content.local_geometry.translated(
                    node.placement.geometry.x.saturating_sub(scene.viewport.x),
                    node.placement.geometry.y.saturating_sub(scene.viewport.y),
                );
                let logical_clip = match content.layer {
                    SurfaceLayer::View => {
                        let Some(view_clip) = view_clip else {
                            continue;
                        };
                        logical_destination
                            .intersection(view_clip)
                            .and_then(|clip| clip.intersection(output_viewport))
                    }
                    SurfaceLayer::Popup => logical_destination.intersection(output_viewport),
                };
                let Some(logical_clip) = logical_clip else {
                    continue;
                };
                let destination = target.scale.physical_rect_round(logical_destination);
                let Some(clip) = target
                    .scale
                    .physical_rect_cover(logical_clip)
                    .intersection(target.viewport)
                else {
                    continue;
                };
                let image_descriptor = match image_descriptors.get(&content.buffer_id) {
                    Some(index) => *index,
                    None => {
                        let index = u32::try_from(images.len())
                            .ok()
                            .and_then(|index| index.checked_add(1))
                            .ok_or(FrameError::DescriptorSizeOverflow)?;
                        images.push(content.buffer_id);
                        image_descriptors.insert(content.buffer_id, index);
                        index
                    }
                };
                draws.push(SurfaceDraw {
                    view_id: node.view_id,
                    surface_id: content.surface_id,
                    revision: content.revision,
                    image_descriptor,
                    destination,
                    clip,
                    effects: node.effects,
                    alpha: content.alpha,
                    sample_transform: content.sample_transform,
                });
                scene_draws.push(SceneDrawCommand::Client(draws.len() - 1));
            }
        }

        let cursor_image_descriptors = cursors
            .as_slice()
            .iter()
            .map(|cursor| {
                cursor
                    .texture
                    .map(|texture| {
                        image_descriptor_for(texture.buffer_id, &mut images, &mut image_descriptors)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            images,
            draws,
            focus_rings,
            scene_draws,
            cursors,
            cursor_image_descriptors,
        })
    }

    pub(crate) fn images(&self) -> &[SurfaceBufferId] {
        &self.images
    }

    pub(crate) fn draws(&self) -> &[SurfaceDraw] {
        &self.draws
    }

    pub(crate) fn focus_rings(&self) -> &[FocusRingDraw] {
        &self.focus_rings
    }

    pub(crate) fn scene_draws(&self) -> &[SceneDrawCommand] {
        &self.scene_draws
    }

    pub(crate) fn cursors(&self) -> &[CursorOverlay] {
        self.cursors.as_slice()
    }

    pub(crate) fn cursor_batch(&self) -> &CursorOverlays {
        &self.cursors
    }

    pub(crate) fn cursor_image_descriptors(&self) -> &[Option<u32>] {
        &self.cursor_image_descriptors
    }
}

fn image_descriptor_for(
    buffer: SurfaceBufferId,
    images: &mut Vec<SurfaceBufferId>,
    descriptors: &mut HashMap<SurfaceBufferId, u32>,
) -> Result<u32, FrameError> {
    if let Some(index) = descriptors.get(&buffer) {
        return Ok(*index);
    }
    let index = u32::try_from(images.len())
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or(FrameError::DescriptorSizeOverflow)?;
    images.push(buffer);
    descriptors.insert(buffer, index);
    Ok(index)
}

fn focus_ring_draw(
    outline: FocusOutline,
    geometry: Rect,
    corner_radius: u32,
    scene_viewport: Rect,
    target: NativeOutputTarget,
) -> Option<FocusRingDraw> {
    if !outline.visible() || geometry.width == 0 || geometry.height == 0 {
        return None;
    }
    let logical_inner = geometry.translated(-scene_viewport.x, -scene_viewport.y);
    let inner = target.scale.physical_rect_round(logical_inner);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    // Map the expanded logical rectangle by edges first, then guarantee at
    // least one physical pixel on every side. This is the same physical-grid
    // concern as Niri's focus-ring rounding: independent logical-length
    // rounding can otherwise erase a one-logical-pixel ring below scale 1.
    let minimum_width = target.scale.physical_length_round(outline.width).max(1);
    let outer = target
        .scale
        .physical_rect_round(logical_inner.inflated(outline.width))
        .union(inner.inflated(minimum_width));
    let clip = outer.intersection(target.viewport)?;
    let inner_radius = target
        .scale
        .physical_length_round(corner_radius)
        .min(inner.width.min(inner.height) / 2);
    let outer_radius = inner_radius
        .saturating_add(minimum_width)
        .min(outer.width.min(outer.height) / 2);
    Some(FocusRingDraw {
        destination: outer,
        clip,
        inner,
        color: outline.color,
        outer_radius,
        inner_radius,
    })
}

#[cfg(test)]
mod tests {
    use tensor_util::{OutputScale, Size};

    use crate::{
        ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{
            ContentRevision, SceneNode, SurfaceContent, SurfaceLayer, SurfaceSampleTransform,
            SurfaceTransform,
        },
    };

    use super::*;

    fn target(viewport: Rect, scale: OutputScale) -> NativeOutputTarget {
        let physical_viewport =
            scale.physical_rect_round(Rect::new(0, 0, viewport.width, viewport.height));
        NativeOutputTarget {
            output: super::super::RenderOutputId {
                device_id: 1,
                connector_id: 1,
            },
            viewport: physical_viewport,
            format: crate::render::OutputFormat {
                format: tensor_host::DrmFormat::new(
                    tensor_host::Fourcc::XRGB8888,
                    tensor_host::Modifier::from_raw(9),
                ),
                plane_count: 1,
            },
            scale,
        }
    }

    #[test]
    fn repeated_images_share_one_heap_descriptor_in_draw_order() {
        let viewport = Rect::new(0, 0, 200, 100);
        let placement = LayoutPlacement {
            geometry: Rect::new(10, 10, 80, 60),
            visible: Some(Rect::new(10, 10, 80, 60)),
        };
        let contents = vec![
            SurfaceContent {
                surface_id: SurfaceId::new(1),
                buffer_id: SurfaceBufferId::new(9),
                revision: ContentRevision::new(1),
                layer: SurfaceLayer::View,
                alpha: Default::default(),
                local_geometry: Rect::new(0, 0, 80, 60),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
            SurfaceContent {
                surface_id: SurfaceId::new(2),
                buffer_id: SurfaceBufferId::new(9),
                revision: ContentRevision::new(2),
                layer: SurfaceLayer::View,
                alpha: Default::default(),
                local_geometry: Rect::new(5, 5, 20, 20),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
        ];
        let span = crate::scene::ContentSpan::new(0, contents.len()).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_content(span),
            ],
            contents,
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();
        assert_eq!(plan.images(), [SurfaceBufferId::new(9)]);
        assert_eq!(plan.draws().len(), 2);
        assert!(plan.draws().iter().all(|draw| draw.image_descriptor == 1));
    }

    #[test]
    fn transparent_surface_never_allocates_a_descriptor_or_draw() {
        let viewport = Rect::new(0, 0, 80, 60);
        let placement = LayoutPlacement {
            geometry: viewport,
            visible: Some(viewport),
        };
        let contents = vec![SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(9),
            revision: ContentRevision::new(1),
            layer: SurfaceLayer::View,
            alpha: SurfaceAlpha::TRANSPARENT,
            local_geometry: viewport,
            sample_transform: SurfaceSampleTransform::IDENTITY,
        }];
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_content(crate::scene::ContentSpan::new(0, 1).unwrap()),
            ],
            contents,
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();
        assert!(plan.images().is_empty());
        assert!(plan.draws().is_empty());
        assert!(plan.scene_draws().is_empty());
    }

    #[test]
    fn draw_geometry_is_output_local_and_preserves_buffer_transform() {
        let viewport = Rect::new(200, 100, 160, 90);
        let placement = LayoutPlacement {
            geometry: Rect::new(180, 90, 100, 80),
            visible: Some(Rect::new(200, 100, 80, 70)),
        };
        let contents = vec![SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(2),
            revision: ContentRevision::new(3),
            layer: SurfaceLayer::View,
            alpha: Default::default(),
            local_geometry: Rect::new(10, 5, 80, 60),
            sample_transform: SurfaceSampleTransform::for_surface(
                Size::new(80, 100),
                1,
                SurfaceTransform::Rotate90,
                None,
            ),
        }];
        let span = crate::scene::ContentSpan::new(0, contents.len()).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_content(span),
            ],
            contents,
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();
        assert_eq!(plan.draws().len(), 1);
        let draw = plan.draws()[0];
        assert_eq!(draw.destination, Rect::new(-10, -5, 80, 60));
        assert_eq!(draw.clip, Rect::new(0, 0, 70, 55));
        assert_eq!(
            draw.sample_transform,
            SurfaceSampleTransform::for_surface(
                Size::new(80, 100),
                1,
                SurfaceTransform::Rotate90,
                None,
            )
        );
    }

    #[test]
    fn popup_draws_beyond_the_layout_tile_but_not_beyond_output() {
        let viewport = Rect::new(0, 0, 100, 80);
        let placement = LayoutPlacement {
            geometry: Rect::new(10, 10, 20, 20),
            visible: Some(Rect::new(10, 10, 20, 20)),
        };
        let contents = vec![SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(2),
            revision: ContentRevision::new(1),
            layer: SurfaceLayer::Popup,
            alpha: Default::default(),
            local_geometry: Rect::new(15, 5, 30, 10),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        }];
        let span = crate::scene::ContentSpan::new(0, 1).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_content(span),
            ],
            contents,
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();
        assert_eq!(plan.draws().len(), 1);
        assert_eq!(plan.draws()[0].destination, Rect::new(25, 15, 30, 10));
        assert_eq!(plan.draws()[0].clip, Rect::new(25, 15, 30, 10));
    }

    #[test]
    fn fractional_scale_maps_logical_draws_to_physical_target_edges() {
        let viewport = Rect::new(0, 0, 100, 80);
        let placement = LayoutPlacement {
            geometry: Rect::new(1, 2, 3, 4),
            visible: Some(Rect::new(1, 2, 3, 4)),
        };
        let contents = vec![SurfaceContent {
            surface_id: SurfaceId::new(1),
            buffer_id: SurfaceBufferId::new(2),
            revision: ContentRevision::new(1),
            layer: SurfaceLayer::View,
            alpha: Default::default(),
            local_geometry: Rect::new(0, 0, 3, 4),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        }];
        let span = crate::scene::ContentSpan::new(0, 1).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_content(span),
            ],
            contents,
        );
        let plan = FrameDrawPlan::build(
            &scene,
            target(viewport, OutputScale::from_f64(1.25).unwrap()),
        )
        .unwrap();
        let draw = plan.draws()[0];
        assert_eq!(draw.destination, Rect::new(1, 3, 4, 5));
        assert_eq!(draw.clip, Rect::new(1, 2, 4, 6));
    }

    #[test]
    fn focused_view_emits_one_output_clipped_rounded_ring() {
        let viewport = Rect::new(0, 0, 100, 80);
        let placement = LayoutPlacement {
            geometry: Rect::new(0, 0, 20, 10),
            visible: Some(Rect::new(0, 0, 20, 10)),
        };
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, placement, EffectStyle::default())
                    .with_focus_outline(Some(FocusOutline::DEFAULT)),
            ],
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();

        assert_eq!(
            plan.focus_rings(),
            [FocusRingDraw {
                destination: Rect::new(-4, -4, 28, 18),
                clip: Rect::new(0, 0, 24, 14),
                inner: Rect::new(0, 0, 20, 10),
                color: FocusOutline::DEFAULT.color,
                outer_radius: 4,
                inner_radius: 0,
            }]
        );
    }

    #[test]
    fn focus_ring_uses_shared_fractional_edges_and_the_view_corner_radius() {
        let viewport = Rect::new(0, 0, 100, 80);
        let placement = LayoutPlacement {
            geometry: Rect::new(1, 2, 4, 4),
            visible: Some(Rect::new(1, 2, 4, 4)),
        };
        let outline = FocusOutline {
            width: 1,
            color: FocusOutline::DEFAULT.color,
        };
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(
                    ViewId::new(1),
                    1,
                    placement,
                    EffectStyle {
                        corner_radius: 2,
                        ..EffectStyle::default()
                    },
                )
                .with_focus_outline(Some(outline)),
            ],
        );

        let plan = FrameDrawPlan::build(
            &scene,
            target(viewport, OutputScale::from_f64(1.25).unwrap()),
        )
        .unwrap();

        assert_eq!(
            plan.focus_rings(),
            [FocusRingDraw {
                destination: Rect::new(0, 1, 8, 8),
                clip: Rect::new(0, 1, 8, 8),
                inner: Rect::new(1, 3, 5, 5),
                color: FocusOutline::DEFAULT.color,
                outer_radius: 3,
                inner_radius: 2,
            }]
        );
    }

    #[test]
    fn scene_order_keeps_the_ring_below_its_client_tree_and_later_views() {
        let viewport = Rect::new(0, 0, 120, 80);
        let lower = LayoutPlacement {
            geometry: Rect::new(16, 12, 40, 30),
            visible: Some(Rect::new(16, 12, 40, 30)),
        };
        let upper = LayoutPlacement {
            geometry: Rect::new(20, 16, 40, 30),
            visible: Some(Rect::new(20, 16, 40, 30)),
        };
        let contents = vec![
            SurfaceContent {
                surface_id: SurfaceId::new(1),
                buffer_id: SurfaceBufferId::new(1),
                revision: ContentRevision::new(1),
                layer: SurfaceLayer::View,
                alpha: Default::default(),
                local_geometry: Rect::new(0, 0, 40, 30),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
            // This popup overlaps the lower view's ring. It must be emitted
            // after the ring just like Niri's window popup tree.
            SurfaceContent {
                surface_id: SurfaceId::new(2),
                buffer_id: SurfaceBufferId::new(2),
                revision: ContentRevision::new(1),
                layer: SurfaceLayer::Popup,
                alpha: Default::default(),
                local_geometry: Rect::new(34, -2, 18, 12),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
            SurfaceContent {
                surface_id: SurfaceId::new(3),
                buffer_id: SurfaceBufferId::new(3),
                revision: ContentRevision::new(1),
                layer: SurfaceLayer::View,
                alpha: Default::default(),
                local_geometry: Rect::new(0, 0, 40, 30),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
        ];
        let lower_span = crate::scene::ContentSpan::new(0, 2).unwrap();
        let upper_span = crate::scene::ContentSpan::new(2, 1).unwrap();
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            viewport,
            vec![
                SceneNode::new(ViewId::new(1), 1, lower, EffectStyle::default())
                    .with_focus_outline(Some(FocusOutline::DEFAULT))
                    .with_content(lower_span),
                SceneNode::new(ViewId::new(2), 2, upper, EffectStyle::default())
                    .with_content(upper_span),
            ],
            contents,
        );

        let plan = FrameDrawPlan::build(&scene, target(viewport, OutputScale::ONE)).unwrap();

        assert_eq!(
            plan.scene_draws(),
            [
                SceneDrawCommand::FocusRing(0),
                SceneDrawCommand::Client(0),
                SceneDrawCommand::Client(1),
                SceneDrawCommand::Client(2),
            ]
        );
        assert_eq!(
            plan.draws()
                .iter()
                .map(|draw| draw.surface_id)
                .collect::<Vec<_>>(),
            [SurfaceId::new(1), SurfaceId::new(2), SurfaceId::new(3)]
        );
    }
}
