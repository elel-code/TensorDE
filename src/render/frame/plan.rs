use std::collections::HashMap;

use tensor_util::Rect;

use crate::{
    ecs::{SurfaceBufferId, SurfaceId, ViewId},
    render::CursorOverlay,
    scene::{ContentRevision, EffectStyle, SceneSnapshot, SurfaceLayer, SurfaceTransform},
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
    cursor: Option<CursorDraw>,
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
    pub(crate) transform: SurfaceTransform,
}

/// A vector cursor draws after every client surface and therefore needs no
/// sampled-image descriptor. Its rectangle is already output-local physical
/// geometry, keeping pointer coordinates out of the Vulkan boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorDraw {
    pub(crate) destination: Rect,
    pub(crate) clip: Rect,
}

impl FrameDrawPlan {
    #[cfg(test)]
    pub(crate) fn build(
        scene: &SceneSnapshot,
        target: NativeOutputTarget,
    ) -> Result<Self, FrameError> {
        Self::build_with_cursor(scene, target, None)
    }

    pub(crate) fn build_with_cursor(
        scene: &SceneSnapshot,
        target: NativeOutputTarget,
        cursor: Option<CursorOverlay>,
    ) -> Result<Self, FrameError> {
        let mut images = Vec::new();
        let mut image_descriptors = HashMap::new();
        let mut draws = Vec::new();
        let output_viewport = Rect::new(0, 0, scene.viewport.width, scene.viewport.height);

        for node in scene.draw_order() {
            if scene.visual_bounds(node).is_none() {
                continue;
            }
            let view_clip = node
                .placement
                .visible
                .map(|clip| clip.translated(-scene.viewport.x, -scene.viewport.y));
            for content in scene.contents_for(node) {
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
                    transform: content.transform,
                });
            }
        }

        Ok(Self {
            images,
            draws,
            cursor: cursor.map(|overlay| CursorDraw {
                destination: overlay.destination,
                clip: overlay.clip,
            }),
        })
    }

    pub(crate) fn images(&self) -> &[SurfaceBufferId] {
        &self.images
    }

    pub(crate) fn draws(&self) -> &[SurfaceDraw] {
        &self.draws
    }

    pub(crate) const fn cursor(&self) -> Option<CursorDraw> {
        self.cursor
    }
}

#[cfg(test)]
mod tests {
    use tensor_util::{OutputScale, Size};

    use crate::{
        ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{ContentRevision, SceneNode, SurfaceContent, SurfaceLayer, SurfaceTransform},
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
                format: smithay::backend::allocator::Format {
                    code: smithay::backend::allocator::Fourcc::Xrgb8888,
                    modifier: smithay::backend::allocator::Modifier::from(9),
                },
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
                buffer_size: Size::new(80, 60),
                local_geometry: Rect::new(0, 0, 80, 60),
                buffer_scale: 1,
                transform: SurfaceTransform::Normal,
            },
            SurfaceContent {
                surface_id: SurfaceId::new(2),
                buffer_id: SurfaceBufferId::new(9),
                revision: ContentRevision::new(2),
                layer: SurfaceLayer::View,
                buffer_size: Size::new(20, 20),
                local_geometry: Rect::new(5, 5, 20, 20),
                buffer_scale: 1,
                transform: SurfaceTransform::Normal,
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
            buffer_size: Size::new(80, 100),
            local_geometry: Rect::new(10, 5, 80, 60),
            buffer_scale: 1,
            transform: SurfaceTransform::Rotate90,
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
        assert_eq!(draw.transform, SurfaceTransform::Rotate90);
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
            buffer_size: Size::new(30, 10),
            local_geometry: Rect::new(15, 5, 30, 10),
            buffer_scale: 1,
            transform: SurfaceTransform::Normal,
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
            buffer_size: Size::new(3, 4),
            local_geometry: Rect::new(0, 0, 3, 4),
            buffer_scale: 1,
            transform: SurfaceTransform::Normal,
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
}
