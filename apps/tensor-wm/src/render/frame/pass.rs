use tensor_util::{Rect, Size};
use vulkan_renderer::{Extent2D, RetainedColorTargetRequest, TextureFormat, TextureUsages};

use crate::{
    ecs::ViewId,
    scene::{DamageSet, SceneSnapshot},
};

use super::NativeOutputTarget;

pub(crate) const BACKDROP_INTERMEDIATE_LANE_COUNT: usize = 2;
const BACKDROP_FILTER_TAPS_PER_PIXEL: u64 = 9;

/// How the native output attachment begins the direct composition pass.
///
/// A full repaint discards the old slot contents with an attachment clear.
/// Partial repainting preserves the exact contents tracked for that output
/// slot, then clears and redraws only the accumulated render damage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputLoad {
    Clear,
    Preserve,
}

/// One background-sampling dependency that cannot execute inside the direct
/// output pass without an attachment feedback loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackdropPass {
    pub(crate) view_id: ViewId,
    /// Output pixels covered by the effect.
    pub(crate) region: Rect,
    /// Previously composed pixels needed by the filter, including its radius.
    pub(crate) sample_region: Rect,
    pub(crate) radius: u32,
    pub(crate) composite_regions: BackdropRegionSpan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackdropRegionSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl BackdropRegionSpan {
    fn new(start: usize, len: usize) -> Option<Self> {
        Some(Self {
            start: u32::try_from(start).ok()?,
            len: u32::try_from(len).ok()?,
        })
    }

    fn range(self) -> std::ops::Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

/// Retained region-local storage required by the complete backdrop sequence.
///
/// Every backdrop executes in scene order, so all effects in one frame can
/// reuse the same ping-pong pair. Capacity follows the largest expanded sample
/// region instead of the full output or the sum of all effect regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackdropIntermediatePlan {
    pub(crate) extent: Size,
    pub(crate) lanes: u8,
    pub(crate) filter_dispatches: u32,
}

impl BackdropIntermediatePlan {
    /// Lower Tensor's effect capacity into the shared renderer's semantic-free
    /// retained target request. Both ping-pong lanes use this exact shape.
    pub(crate) fn retained_target_requests(
        self,
        format: TextureFormat,
    ) -> [RetainedColorTargetRequest; BACKDROP_INTERMEDIATE_LANE_COUNT] {
        let request = RetainedColorTargetRequest {
            extent: Extent2D::new(self.extent.width, self.extent.height),
            format,
            additional_usage: TextureUsages::COPY_DESTINATION | TextureUsages::STORAGE,
        };
        [request; BACKDROP_INTERMEDIATE_LANE_COUNT]
    }
}

/// GPU pass topology selected from scene semantics, not from a global toggle.
///
/// Local analytic effects remain in the direct pass. Backdrop sampling ends
/// that pass, filters a retained intermediate, and resumes composition, so it
/// is represented explicitly even before the Vulkan lowering is available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CompositionPath {
    DirectSinglePass,
    BackdropDependentMultiPass(Vec<BackdropPass>),
}

/// Fixed-width frame-path telemetry derived together with the pass plan.
///
/// These are deterministic pixel-work counts, not timing estimates. They make
/// direct-versus-local-multi-pass comparisons possible without formatting the
/// complete backdrop vector or querying Vulkan on the frame path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FramePassMetrics {
    pub(crate) backdrop_passes: u32,
    pub(crate) sample_pixels: u64,
    pub(crate) filter_pixels: u64,
    pub(crate) filter_texture_samples: u64,
    pub(crate) composite_pixel_upper_bound: u64,
    pub(crate) retained_intermediate_pixels: u64,
}

impl FramePassMetrics {
    const DIRECT: Self = Self {
        backdrop_passes: 0,
        sample_pixels: 0,
        filter_pixels: 0,
        filter_texture_samples: 0,
        composite_pixel_upper_bound: 0,
        retained_intermediate_pixels: 0,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FramePassPlan {
    output_load: OutputLoad,
    path: CompositionPath,
    composite_regions: Vec<Rect>,
    intermediate: Option<BackdropIntermediatePlan>,
    metrics: FramePassMetrics,
}

impl FramePassPlan {
    pub(crate) fn build(
        scene: &SceneSnapshot,
        target: NativeOutputTarget,
        render_damage: &DamageSet,
    ) -> Self {
        let output_load = if render_damage.is_full(target.viewport) {
            OutputLoad::Clear
        } else {
            OutputLoad::Preserve
        };
        let mut backdrops = Vec::new();
        let mut composite_regions = Vec::new();
        for node in scene.draw_order() {
            let Some(blur) = node.effects.backdrop_blur.filter(|blur| blur.radius > 0) else {
                continue;
            };
            // Scene damage has already propagated background dependencies to
            // each exact effect rectangle. Overlay-only damage is added later;
            // intersect it here without filling protocol-region holes.
            let start = composite_regions.len();
            let mut region = None;
            for logical_region in node.backdrop_regions(scene.viewport) {
                let local = logical_region.translated(-scene.viewport.x, -scene.viewport.y);
                let Some(effect_region) = target
                    .scale
                    .physical_rect_cover(local)
                    .intersection(target.viewport)
                else {
                    continue;
                };
                for active in render_damage
                    .regions()
                    .iter()
                    .filter_map(|damage| damage.intersection(effect_region))
                {
                    region = Some(region.map_or(active, |region: Rect| region.union(active)));
                    composite_regions.push(active);
                }
            }
            let Some(region) = region else {
                continue;
            };
            let composite_span =
                BackdropRegionSpan::new(start, composite_regions.len().saturating_sub(start))
                    .expect("frame backdrop region table fits in u32");
            let radius = target.scale.physical_length_round(blur.radius).max(1);
            let sample_region = region
                .inflated(radius)
                .intersection(target.viewport)
                .expect("the effect region itself intersects the output");
            backdrops.push(BackdropPass {
                view_id: node.view_id,
                region,
                sample_region,
                radius,
                composite_regions: composite_span,
            });
        }
        let (intermediate, metrics) = if backdrops.is_empty() {
            (None, FramePassMetrics::DIRECT)
        } else {
            let (extent, sample_pixels) = backdrops.iter().fold(
                (Size::default(), 0u64),
                |(extent, sample_pixels), backdrop| {
                    (
                        Size::new(
                            extent.width.max(backdrop.sample_region.width),
                            extent.height.max(backdrop.sample_region.height),
                        ),
                        sample_pixels.saturating_add(pixel_area(backdrop.sample_region)),
                    )
                },
            );
            let composite_pixel_upper_bound = composite_regions
                .iter()
                .copied()
                .map(pixel_area)
                .fold(0u64, u64::saturating_add);
            let backdrop_passes = u32::try_from(backdrops.len()).unwrap_or(u32::MAX);
            let filter_pixels = sample_pixels.saturating_mul(2);
            (
                Some(BackdropIntermediatePlan {
                    extent,
                    lanes: BACKDROP_INTERMEDIATE_LANE_COUNT as u8,
                    filter_dispatches: backdrop_passes.saturating_mul(2),
                }),
                FramePassMetrics {
                    backdrop_passes,
                    sample_pixels,
                    filter_pixels,
                    filter_texture_samples: filter_pixels
                        .saturating_mul(BACKDROP_FILTER_TAPS_PER_PIXEL),
                    composite_pixel_upper_bound,
                    retained_intermediate_pixels: pixel_area(Rect::new(
                        0,
                        0,
                        extent.width,
                        extent.height,
                    ))
                    .saturating_mul(BACKDROP_INTERMEDIATE_LANE_COUNT as u64),
                },
            )
        };
        let path = if backdrops.is_empty() {
            CompositionPath::DirectSinglePass
        } else {
            CompositionPath::BackdropDependentMultiPass(backdrops)
        };
        Self {
            output_load,
            path,
            composite_regions,
            intermediate,
            metrics,
        }
    }

    pub(crate) const fn output_load(&self) -> OutputLoad {
        self.output_load
    }

    pub(crate) fn path(&self) -> &CompositionPath {
        &self.path
    }

    pub(crate) const fn composition_label(&self) -> &'static str {
        match &self.path {
            CompositionPath::DirectSinglePass => "direct-single-pass",
            CompositionPath::BackdropDependentMultiPass(_) => "backdrop-multi-pass",
        }
    }

    pub(crate) const fn intermediate(&self) -> Option<BackdropIntermediatePlan> {
        self.intermediate
    }

    pub(crate) const fn intermediate_descriptor_count(&self) -> u8 {
        match self.intermediate {
            Some(plan) => plan.lanes,
            None => 0,
        }
    }

    pub(crate) fn composite_regions(&self, backdrop: BackdropPass) -> &[Rect] {
        &self.composite_regions[backdrop.composite_regions.range()]
    }
    pub(crate) const fn metrics(&self) -> FramePassMetrics {
        self.metrics
    }
}

const fn pixel_area(rect: Rect) -> u64 {
    (rect.width as u64).saturating_mul(rect.height as u64)
}

#[cfg(test)]
mod tests {
    use tensor_host::{DrmFormat, Fourcc, Modifier};
    use tensor_util::OutputScale;

    use crate::{
        ecs::{ViewId, WorkspaceId},
        layout::LayoutPlacement,
        render::{OutputFormat, RenderOutputId},
        scene::{BackdropBlur, BackdropRegion, EffectStyle, SceneNode},
    };

    use super::*;

    const VIEWPORT: Rect = Rect::new(0, 0, 200, 120);

    fn target(scale: OutputScale) -> NativeOutputTarget {
        NativeOutputTarget {
            output: RenderOutputId {
                device_id: 1,
                connector_id: 2,
            },
            viewport: VIEWPORT,
            format: OutputFormat {
                format: DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9)),
                plane_count: 1,
            },
            scale,
        }
    }

    fn node(effects: EffectStyle) -> SceneNode {
        SceneNode::new(
            ViewId::new(7),
            1,
            LayoutPlacement {
                geometry: Rect::new(10, 20, 80, 40),
                visible: Some(Rect::new(10, 20, 80, 40)),
            },
            effects,
        )
    }

    #[test]
    fn local_effects_keep_the_direct_single_pass_path() {
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(EffectStyle {
                corner_radius: 12,
                ..EffectStyle::default()
            })],
        );
        let plan = FramePassPlan::build(&scene, target(OutputScale::ONE), &DamageSet::default());

        assert_eq!(plan.output_load(), OutputLoad::Preserve);
        assert_eq!(plan.path(), &CompositionPath::DirectSinglePass);
        assert_eq!(plan.composition_label(), "direct-single-pass");
        assert_eq!(plan.metrics(), FramePassMetrics::DIRECT);
    }

    #[test]
    fn backdrop_sampling_selects_a_scaled_region_local_multi_pass() {
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 8 }),
                ..EffectStyle::default()
            })],
        );
        let scale = OutputScale::from_f64(1.25).unwrap();
        let physical_target = NativeOutputTarget {
            viewport: Rect::new(0, 0, 250, 150),
            ..target(scale)
        };
        let damage = DamageSet::full(VIEWPORT).to_physical(
            VIEWPORT,
            physical_target.viewport,
            physical_target.scale,
        );
        let plan = FramePassPlan::build(&scene, physical_target, &damage);

        assert_eq!(plan.output_load(), OutputLoad::Clear);
        assert_eq!(
            plan.path(),
            &CompositionPath::BackdropDependentMultiPass(vec![BackdropPass {
                view_id: ViewId::new(7),
                region: Rect::new(12, 25, 101, 50),
                sample_region: Rect::new(2, 15, 121, 70),
                radius: 10,
                composite_regions: BackdropRegionSpan { start: 0, len: 1 },
            }])
        );
        assert_eq!(
            plan.intermediate(),
            Some(BackdropIntermediatePlan {
                extent: Size::new(121, 70),
                lanes: 2,
                filter_dispatches: 2,
            })
        );
        assert_eq!(plan.intermediate_descriptor_count(), 2);
        assert_eq!(plan.composition_label(), "backdrop-multi-pass");
        assert_eq!(
            plan.metrics(),
            FramePassMetrics {
                backdrop_passes: 1,
                sample_pixels: 8_470,
                filter_pixels: 16_940,
                filter_texture_samples: 152_460,
                composite_pixel_upper_bound: 5_050,
                retained_intermediate_pixels: 16_940,
            }
        );
        assert_eq!(
            plan.intermediate()
                .unwrap()
                .retained_target_requests(TextureFormat::Bgra8Srgb),
            [RetainedColorTargetRequest {
                extent: Extent2D::new(121, 70),
                format: TextureFormat::Bgra8Srgb,
                additional_usage: TextureUsages::COPY_DESTINATION | TextureUsages::STORAGE,
            }; BACKDROP_INTERMEDIATE_LANE_COUNT]
        );
    }

    #[test]
    fn multiple_backdrops_sum_work_but_share_the_largest_two_lane_capacity() {
        let first = node(EffectStyle {
            backdrop_blur: Some(BackdropBlur { radius: 8 }),
            ..EffectStyle::default()
        });
        let second = SceneNode::new(
            ViewId::new(8),
            2,
            LayoutPlacement {
                geometry: Rect::new(120, 10, 60, 30),
                visible: Some(Rect::new(120, 10, 60, 30)),
            },
            EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 4 }),
                ..EffectStyle::default()
            },
        );
        let scene = SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![first, second]);
        let plan =
            FramePassPlan::build(&scene, target(OutputScale::ONE), &DamageSet::full(VIEWPORT));

        assert_eq!(
            plan.intermediate(),
            Some(BackdropIntermediatePlan {
                extent: Size::new(96, 56),
                lanes: 2,
                filter_dispatches: 4,
            })
        );
        assert_eq!(
            plan.metrics(),
            FramePassMetrics {
                backdrop_passes: 2,
                sample_pixels: 7_960,
                filter_pixels: 15_920,
                filter_texture_samples: 143_280,
                composite_pixel_upper_bound: 5_000,
                retained_intermediate_pixels: 10_752,
            }
        );
    }

    #[test]
    fn protocol_region_holes_stay_out_of_composite_work() {
        let effect = node(EffectStyle {
            backdrop_blur: Some(BackdropBlur { radius: 8 }),
            ..EffectStyle::default()
        })
        .with_backdrop_region(BackdropRegion::new(vec![
            Rect::new(0, 0, 20, 10),
            Rect::new(60, 20, 10, 10),
        ]));
        let scene = SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![effect]);
        let plan =
            FramePassPlan::build(&scene, target(OutputScale::ONE), &DamageSet::full(VIEWPORT));
        let CompositionPath::BackdropDependentMultiPass(backdrops) = plan.path() else {
            unreachable!();
        };

        assert_eq!(backdrops.len(), 1);
        assert_eq!(backdrops[0].region, Rect::new(10, 20, 70, 30));
        assert_eq!(backdrops[0].sample_region, Rect::new(2, 12, 86, 46));
        assert_eq!(
            plan.composite_regions(backdrops[0]),
            [Rect::new(10, 20, 20, 10), Rect::new(70, 40, 10, 10)]
        );
        assert_eq!(plan.metrics().sample_pixels, 3_956);
        assert_eq!(plan.metrics().composite_pixel_upper_bound, 300);
    }

    #[test]
    fn damage_away_from_backdrop_keeps_the_direct_path() {
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 8 }),
                ..EffectStyle::default()
            })],
        );
        let mut damage = DamageSet::default();
        damage.add_region(Rect::new(150, 100, 10, 10), VIEWPORT);
        let plan = FramePassPlan::build(&scene, target(OutputScale::ONE), &damage);

        assert_eq!(plan.output_load(), OutputLoad::Preserve);
        assert_eq!(plan.path(), &CompositionPath::DirectSinglePass);
        assert_eq!(plan.intermediate_descriptor_count(), 0);
        assert_eq!(plan.metrics(), FramePassMetrics::DIRECT);
    }

    #[test]
    fn partial_damage_selects_only_the_intersecting_backdrop() {
        let first = node(EffectStyle {
            backdrop_blur: Some(BackdropBlur { radius: 8 }),
            ..EffectStyle::default()
        });
        let second = SceneNode::new(
            ViewId::new(8),
            2,
            LayoutPlacement {
                geometry: Rect::new(120, 10, 60, 30),
                visible: Some(Rect::new(120, 10, 60, 30)),
            },
            EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 4 }),
                ..EffectStyle::default()
            },
        );
        let scene = SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![first, second]);
        let mut damage = DamageSet::default();
        damage.add_region(Rect::new(130, 20, 8, 8), VIEWPORT);
        let plan = FramePassPlan::build(&scene, target(OutputScale::ONE), &damage);

        assert_eq!(
            plan.path(),
            &CompositionPath::BackdropDependentMultiPass(vec![BackdropPass {
                view_id: ViewId::new(8),
                region: Rect::new(130, 20, 8, 8),
                sample_region: Rect::new(126, 16, 16, 16),
                radius: 4,
                composite_regions: BackdropRegionSpan { start: 0, len: 1 },
            }])
        );
        assert_eq!(
            plan.metrics(),
            FramePassMetrics {
                backdrop_passes: 1,
                sample_pixels: 256,
                filter_pixels: 512,
                filter_texture_samples: 4_608,
                composite_pixel_upper_bound: 64,
                retained_intermediate_pixels: 512,
            }
        );
    }

    #[test]
    fn fragmented_damage_uses_one_bounded_effect_region() {
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![node(EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 8 }),
                ..EffectStyle::default()
            })],
        );
        let mut damage = DamageSet::default();
        damage.add_region(Rect::new(12, 22, 4, 4), VIEWPORT);
        damage.add_region(Rect::new(80, 50, 5, 5), VIEWPORT);
        let plan = FramePassPlan::build(&scene, target(OutputScale::ONE), &damage);

        assert_eq!(
            plan.path(),
            &CompositionPath::BackdropDependentMultiPass(vec![BackdropPass {
                view_id: ViewId::new(7),
                region: Rect::new(12, 22, 73, 33),
                sample_region: Rect::new(4, 14, 89, 49),
                radius: 8,
                composite_regions: BackdropRegionSpan { start: 0, len: 2 },
            }])
        );
        assert_eq!(plan.metrics().backdrop_passes, 1);
        assert_eq!(plan.metrics().sample_pixels, 4_361);
        assert_eq!(plan.metrics().composite_pixel_upper_bound, 41);
        let CompositionPath::BackdropDependentMultiPass(backdrops) = plan.path() else {
            unreachable!();
        };
        assert_eq!(
            plan.composite_regions(backdrops[0]),
            [Rect::new(12, 22, 4, 4), Rect::new(80, 50, 5, 5)]
        );
    }

    #[test]
    fn damage_local_sample_region_clips_at_the_output_edge() {
        let scene = SceneSnapshot::new(
            WorkspaceId::new(1),
            VIEWPORT,
            vec![SceneNode::new(
                ViewId::new(9),
                1,
                LayoutPlacement {
                    geometry: Rect::new(0, 0, 30, 20),
                    visible: Some(Rect::new(0, 0, 30, 20)),
                },
                EffectStyle {
                    backdrop_blur: Some(BackdropBlur { radius: 8 }),
                    ..EffectStyle::default()
                },
            )],
        );
        let mut damage = DamageSet::default();
        damage.add_region(Rect::new(0, 0, 3, 3), VIEWPORT);
        let plan = FramePassPlan::build(&scene, target(OutputScale::ONE), &damage);

        assert_eq!(
            plan.path(),
            &CompositionPath::BackdropDependentMultiPass(vec![BackdropPass {
                view_id: ViewId::new(9),
                region: Rect::new(0, 0, 3, 3),
                sample_region: Rect::new(0, 0, 11, 11),
                radius: 8,
                composite_regions: BackdropRegionSpan { start: 0, len: 1 },
            }])
        );
    }

    #[test]
    fn propagated_background_damage_keeps_the_dependent_backdrop() {
        let background = |x| {
            SceneNode::new(
                ViewId::new(1),
                1,
                LayoutPlacement {
                    geometry: Rect::new(x, 30, 4, 4),
                    visible: Some(Rect::new(x, 30, 4, 4)),
                },
                EffectStyle::default(),
            )
        };
        let blur = || {
            node(EffectStyle {
                backdrop_blur: Some(BackdropBlur { radius: 8 }),
                ..EffectStyle::default()
            })
        };
        let old = SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![background(3), blur()]);
        let current =
            SceneSnapshot::new(WorkspaceId::new(1), VIEWPORT, vec![background(4), blur()]);
        let damage = current.damage_since(Some(&old));
        let plan = FramePassPlan::build(&current, target(OutputScale::ONE), &damage);

        assert_eq!(plan.composition_label(), "backdrop-multi-pass");
        assert_eq!(plan.metrics().backdrop_passes, 1);
    }
}
