use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
    time::Duration,
};

use tensor_host::{VblankClock, VblankMetadata};
use tensor_util::Rect;
use tracing::warn;
use wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::clock::monotonic_now;
use crate::{
    backend::BackendOutputId,
    ecs::{SurfaceId, ViewId},
    render::CursorOverlays,
    scene::{SceneSnapshot, SurfaceContent, SurfaceLayer},
};

use super::{
    RuntimeState,
    surface_tree::{
        OutputPresentationFeedback, send_frame_callbacks_surface_tree,
        take_presentation_feedback_surface_tree,
    },
};
use crate::protocol::globals::{
    compositor::SurfaceData, output::Output, presentation::Refresh, surface_timing::SurfaceBarrier,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PresentationKey {
    output: BackendOutputId,
    timeline_value: u64,
}

#[derive(Debug)]
pub(super) struct CapturedPresentation {
    feedback: OutputPresentationFeedback,
    submitted_surfaces: HashSet<SurfaceId>,
    submitted_views: HashSet<ViewId>,
    cursor_surfaces: CursorSurfaces,
    fifo_barriers: Vec<SurfaceBarrier>,
}

/// Keep the overwhelmingly common core-pointer cursor inline. Tablet cursor
/// surfaces allocate only when a tablet is actually visible on this output.
#[derive(Debug, Default)]
struct CursorSurfaces {
    pointer: Option<WlSurface>,
    dnd_icon: Option<WlSurface>,
    tablets: Vec<WlSurface>,
}

impl CursorSurfaces {
    fn insert(&mut self, source: u64, surface: WlSurface) {
        if source == 0 {
            self.pointer = Some(surface);
        } else if source == crate::protocol::dnd_icon::DND_ICON_SOURCE {
            self.dnd_icon = Some(surface);
        } else if !self.tablets.iter().any(|current| current == &surface) {
            self.tablets.push(surface);
        }
    }

    fn iter(&self) -> impl Iterator<Item = &WlSurface> {
        self.pointer
            .iter()
            .chain(&self.dnd_icon)
            .chain(&self.tablets)
    }

    fn contains(&self, surface: &WlSurface) -> bool {
        self.pointer.as_ref() == Some(surface)
            || self.dnd_icon.as_ref() == Some(surface)
            || self.tablets.iter().any(|current| current == surface)
    }

    fn contains_id(&self, surface: &wayland_server::backend::ObjectId) -> bool {
        self.pointer
            .as_ref()
            .is_some_and(|current| current.id() == *surface)
            || self
                .dnd_icon
                .as_ref()
                .is_some_and(|current| current.id() == *surface)
            || self.tablets.iter().any(|current| current.id() == *surface)
    }
}

#[derive(Debug, Default)]
pub(super) struct PendingPresentations {
    frames: BTreeMap<PresentationKey, OutputPresentationFeedback>,
}

impl PendingPresentations {
    fn insert(
        &mut self,
        output: BackendOutputId,
        timeline_value: u64,
        feedback: OutputPresentationFeedback,
    ) -> Option<OutputPresentationFeedback> {
        self.frames.insert(
            PresentationKey {
                output,
                timeline_value,
            },
            feedback,
        )
    }

    fn take(
        &mut self,
        output: BackendOutputId,
        timeline_value: u64,
    ) -> Option<OutputPresentationFeedback> {
        self.frames.remove(&PresentationKey {
            output,
            timeline_value,
        })
    }

    fn discard_output(&mut self, output: BackendOutputId) -> usize {
        let before = self.frames.len();
        self.frames.retain(|key, _| key.output != output);
        before - self.frames.len()
    }

    fn discard_all(&mut self) -> usize {
        let discarded = self.frames.len();
        self.frames.clear();
        discarded
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.frames.len()
    }
}

impl RuntimeState {
    pub(super) fn capture_scene_presentation(
        &self,
        output_id: BackendOutputId,
        output: &Output,
        scene: &SceneSnapshot,
        cursors: &CursorOverlays,
    ) -> CapturedPresentation {
        let mut cursor_surfaces = CursorSurfaces::default();
        for cursor in cursors.as_slice() {
            let surface = if cursor.source == crate::protocol::dnd_icon::DND_ICON_SOURCE {
                self.dnd_icon.surface().cloned()
            } else {
                self.cursor.surface_for_source(cursor.source)
            };
            let Some(surface) = surface else {
                continue;
            };
            cursor_surfaces.insert(cursor.source, surface);
        }
        self.capture_presentation(
            output_id,
            output,
            scene,
            submitted_scene_surfaces(scene),
            cursor_surfaces,
        )
    }

    pub(super) fn queue_presentation(
        &mut self,
        output: BackendOutputId,
        timeline_value: u64,
        frame: CapturedPresentation,
    ) {
        let CapturedPresentation { feedback, .. } = frame;
        if self
            .pending_presentations
            .insert(output, timeline_value, feedback)
            .is_some()
        {
            warn!(
                output_device = output.device_id,
                output_connector = output.connector_id,
                timeline = timeline_value,
                "duplicate presentation identity replaced an in-flight frame"
            );
        }
    }

    pub(super) fn finish_presentation(
        &mut self,
        output_id: BackendOutputId,
        timeline_value: u64,
        metadata: Option<VblankMetadata>,
    ) -> bool {
        let Some(frame) = self.pending_presentations.take(output_id, timeline_value) else {
            return false;
        };
        let Some(output) = self
            .outputs
            .get(&output_id)
            .map(|managed| managed.output.clone())
        else {
            return false;
        };
        let fallback_time = monotonic_now();
        let sample = presentation_sample(
            metadata,
            fallback_time,
            output.current_mode().map(|mode| mode.refresh_millihertz),
        );
        let mut feedback = frame;
        feedback.presented(sample.time, sample.refresh, sample.sequence, sample.flags);
        true
    }

    pub(super) fn discard_output_presentations(&mut self, output: BackendOutputId) -> usize {
        self.pending_presentations.discard_output(output)
    }

    pub(super) fn discard_all_presentations(&mut self) -> usize {
        self.pending_presentations.discard_all()
    }

    fn capture_presentation(
        &self,
        output_id: BackendOutputId,
        output: &Output,
        scene: &SceneSnapshot,
        mut submitted: HashMap<SurfaceId, ViewId>,
        cursor_surfaces: CursorSurfaces,
    ) -> CapturedPresentation {
        let output_regions = self.output_regions();
        let bounds = scene_surface_bounds(scene);
        submitted.retain(|surface_id, _| {
            bounds
                .get(surface_id)
                .and_then(|bounds| primary_output(*bounds, &output_regions))
                == Some(output_id)
        });
        let submitted_surfaces = submitted.keys().copied().collect::<HashSet<_>>();
        let submitted_views = submitted.values().copied().collect::<HashSet<_>>();
        let surface_buffers = &self.surface_buffers;
        let fifo_barriers = self
            .protocol_globals
            .surface_timing
            .capture_fifo_barriers(|surface| {
                surface_buffers
                    .surface_id(surface)
                    .is_some_and(|surface_id| submitted_surfaces.contains(&surface_id))
                    || cursor_surfaces.contains_id(surface)
            });
        let mut feedback = OutputPresentationFeedback::new(output);
        let mut is_submitted = |surface: &WlSurface, _: &SurfaceData| {
            surface_buffers
                .surface_id(&surface.id())
                .is_some_and(|surface_id| submitted_surfaces.contains(&surface_id))
                || cursor_surfaces.contains(surface)
        };
        let mut feedback_flags =
            |_: &WlSurface, _: &SurfaceData| wp_presentation_feedback::Kind::empty();
        for window in self.space.elements() {
            let Some(root) = window.wl_surface() else {
                continue;
            };
            let Some(view_id) = self.view_for_surface(&root) else {
                continue;
            };
            if submitted_views.contains(&view_id) {
                window.take_presentation_feedback(
                    &self.popups,
                    &mut feedback,
                    &mut is_submitted,
                    &mut feedback_flags,
                );
                if self.input_method_popup_root().as_ref() == Some(root.as_ref()) {
                    self.protocol_globals
                        .input_method
                        .for_each_visible_popup(|popup| {
                            take_presentation_feedback_surface_tree(
                                popup.wl_surface(),
                                &mut feedback,
                                &mut is_submitted,
                                &mut feedback_flags,
                            );
                        });
                }
                #[cfg(feature = "xwayland")]
                for popup in self.x11_popup_surfaces_for_root(&root) {
                    take_presentation_feedback_surface_tree(
                        &popup,
                        &mut feedback,
                        &mut is_submitted,
                        &mut feedback_flags,
                    );
                }
            }
        }
        for surface in cursor_surfaces.iter() {
            take_presentation_feedback_surface_tree(
                surface,
                &mut feedback,
                &mut is_submitted,
                &mut feedback_flags,
            );
        }
        CapturedPresentation {
            feedback,
            submitted_surfaces,
            submitted_views,
            cursor_surfaces,
            fifo_barriers,
        }
    }

    fn output_regions(&self) -> Vec<(BackendOutputId, Rect)> {
        self.outputs
            .iter()
            .filter_map(|(id, managed)| {
                let geometry = self.space.output_geometry(&managed.output)?;
                Some((
                    *id,
                    Rect::new(
                        geometry.loc.x,
                        geometry.loc.y,
                        u32::try_from(geometry.size.w).ok()?,
                        u32::try_from(geometry.size.h).ok()?,
                    ),
                ))
            })
            .collect()
    }

    /// Send frame callbacks once atomic KMS has accepted the submitted frame.
    /// Presentation feedback itself remains pending until the matching vblank.
    pub(super) fn send_submitted_frame_callbacks(&self, frame: &CapturedPresentation) {
        let time = monotonic_now();
        let surface_buffers = &self.surface_buffers;
        let submitted_surfaces = &frame.submitted_surfaces;
        let submitted_views = &frame.submitted_views;
        let cursor_surfaces = &frame.cursor_surfaces;
        let mut is_submitted = |surface: &WlSurface, _: &SurfaceData| {
            surface_buffers
                .surface_id(&surface.id())
                .is_some_and(|surface_id| submitted_surfaces.contains(&surface_id))
                || cursor_surfaces.contains(surface)
        };
        for window in self.space.elements() {
            let Some(root) = window.wl_surface() else {
                continue;
            };
            if self
                .view_for_surface(&root)
                .is_some_and(|view_id| submitted_views.contains(&view_id))
            {
                window.send_frame(&self.popups, time, &mut is_submitted);
                if self.input_method_popup_root().as_ref() == Some(root.as_ref()) {
                    self.protocol_globals
                        .input_method
                        .for_each_visible_popup(|popup| {
                            send_frame_callbacks_surface_tree(
                                popup.wl_surface(),
                                time,
                                &mut is_submitted,
                            );
                        });
                }
                #[cfg(feature = "xwayland")]
                for popup in self.x11_popup_surfaces_for_root(&root) {
                    send_frame_callbacks_surface_tree(&popup, time, &mut is_submitted);
                }
            }
        }
        for surface in cursor_surfaces.iter() {
            send_frame_callbacks_surface_tree(surface, time, &mut is_submitted);
        }
    }

    pub(super) fn release_submitted_fifo_barriers(&mut self, frame: &mut CapturedPresentation) {
        let barriers = std::mem::take(&mut frame.fifo_barriers);
        self.release_captured_fifo_barriers(barriers);
    }

    pub(super) fn discard_captured_presentation(&mut self, mut frame: CapturedPresentation) {
        self.release_submitted_fifo_barriers(&mut frame);
    }
}

fn submitted_scene_surfaces(scene: &SceneSnapshot) -> HashMap<SurfaceId, ViewId> {
    let mut submitted = HashMap::new();
    for node in scene.draw_order() {
        if scene.visual_bounds(node).is_none() {
            continue;
        }
        for content in scene.contents_for(node) {
            if submitted_content_bounds(scene, node, content).is_some() {
                submitted.insert(content.surface_id, node.view_id);
            }
        }
    }
    submitted
}

fn scene_surface_bounds(scene: &SceneSnapshot) -> HashMap<SurfaceId, Rect> {
    let mut bounds = HashMap::<SurfaceId, Rect>::new();
    for node in scene.draw_order() {
        if scene.visual_bounds(node).is_none() {
            continue;
        }
        for content in scene.contents_for(node) {
            let Some(content_bounds) = submitted_content_bounds(scene, node, content) else {
                continue;
            };
            bounds
                .entry(content.surface_id)
                .and_modify(|current| *current = current.union(content_bounds))
                .or_insert(content_bounds);
        }
    }
    bounds
}

fn submitted_content_bounds(
    scene: &SceneSnapshot,
    node: &crate::scene::SceneNode,
    content: &SurfaceContent,
) -> Option<Rect> {
    let destination = content
        .local_geometry
        .translated(node.placement.geometry.x, node.placement.geometry.y);
    match content.layer {
        SurfaceLayer::View => node
            .placement
            .visible
            .and_then(|clip| destination.intersection(clip)),
        SurfaceLayer::Popup => destination.intersection(scene.viewport),
    }
}

fn primary_output(bounds: Rect, outputs: &[(BackendOutputId, Rect)]) -> Option<BackendOutputId> {
    outputs
        .iter()
        .filter_map(|(id, output)| {
            let overlap = bounds.intersection(*output)?;
            let area = u64::from(overlap.width) * u64::from(overlap.height);
            Some((*id, area))
        })
        .min_by_key(|(id, area)| (Reverse(*area), *id))
        .map(|(id, _)| id)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PresentationSample {
    time: Duration,
    refresh: Refresh,
    sequence: u64,
    flags: wp_presentation_feedback::Kind,
}

fn presentation_sample(
    metadata: Option<VblankMetadata>,
    fallback_time: Duration,
    refresh_millihertz: Option<i32>,
) -> PresentationSample {
    let mut flags =
        wp_presentation_feedback::Kind::Vsync | wp_presentation_feedback::Kind::HwCompletion;
    let (time, sequence) = match metadata {
        Some(VblankMetadata {
            timestamp,
            sequence,
            clock: VblankClock::Monotonic,
        }) if !timestamp.is_zero() => {
            flags.insert(wp_presentation_feedback::Kind::HwClock);
            (timestamp, u64::from(sequence))
        }
        Some(VblankMetadata { sequence, .. }) => (fallback_time, u64::from(sequence)),
        None => (fallback_time, 0),
    };
    PresentationSample {
        time,
        refresh: refresh_millihertz.map_or(Refresh::Unknown, refresh_from_millihertz),
        sequence,
        flags,
    }
}

fn refresh_from_millihertz(refresh: i32) -> Refresh {
    let Ok(refresh) = u64::try_from(refresh) else {
        return Refresh::Unknown;
    };
    if refresh == 0 {
        return Refresh::Unknown;
    }
    const NANOS_PER_MILLIHERTZ_PERIOD: u64 = 1_000_000_000_000;
    Refresh::Fixed(Duration::from_nanos(
        (NANOS_PER_MILLIHERTZ_PERIOD + refresh / 2) / refresh,
    ))
}

#[cfg(test)]
mod tests {
    use crate::{
        ecs::{SurfaceBufferId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{ContentRevision, ContentSpan, EffectStyle, SceneNode, SurfaceSampleTransform},
    };

    use super::*;

    const OUTPUT: BackendOutputId = BackendOutputId::new(1, 2);
    const SECOND_OUTPUT: BackendOutputId = BackendOutputId::new(1, 3);

    fn feedback() -> OutputPresentationFeedback {
        let mode = tensor_host::PhysicalMode::new(1920, 1080, 60_000);
        let output = Output::new(
            OUTPUT,
            "test".to_owned(),
            (600, 340),
            tensor_host::SubpixelLayout::HorizontalRgb,
            vec![mode],
            mode,
            mode,
            tensor_util::OutputScale::ONE,
        );
        OutputPresentationFeedback::new(&output)
    }

    #[test]
    fn pending_frames_are_keyed_by_output_and_renderer_timeline() {
        let mut pending = PendingPresentations::default();
        assert!(pending.insert(OUTPUT, 7, feedback()).is_none());
        assert!(pending.insert(SECOND_OUTPUT, 7, feedback()).is_none());
        assert_eq!(pending.len(), 2);

        assert!(pending.take(OUTPUT, 8).is_none());
        assert!(pending.take(SECOND_OUTPUT, 8).is_none());
        assert!(pending.take(OUTPUT, 7).is_some());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending.discard_output(SECOND_OUTPUT), 1);
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn discarding_all_frames_covers_session_loss() {
        let mut pending = PendingPresentations::default();
        pending.insert(OUTPUT, 1, feedback());
        pending.insert(SECOND_OUTPUT, 2, feedback());

        assert_eq!(pending.discard_all(), 2);
        assert_eq!(pending.len(), 0);
        assert!(pending.take(OUTPUT, 1).is_none());
        assert!(pending.take(SECOND_OUTPUT, 2).is_none());
    }

    #[test]
    fn primary_output_prefers_visible_area_then_stable_identity() {
        let outputs = [
            (OUTPUT, Rect::new(0, 0, 100, 100)),
            (SECOND_OUTPUT, Rect::new(100, 0, 100, 100)),
        ];

        assert_eq!(
            primary_output(Rect::new(80, 0, 80, 100), &outputs),
            Some(SECOND_OUTPUT)
        );
        assert_eq!(
            primary_output(Rect::new(50, 0, 100, 100), &outputs),
            Some(OUTPUT)
        );
    }

    #[test]
    fn submitted_surface_membership_and_area_share_scene_clipping() {
        let view_id = ViewId::new(4);
        let placement = LayoutPlacement {
            geometry: Rect::new(50, 0, 100, 100),
            visible: Some(Rect::new(100, 0, 50, 100)),
        };
        let contents = vec![
            content(1, SurfaceLayer::View, Rect::new(-50, 0, 200, 100)),
            content(2, SurfaceLayer::Popup, Rect::new(120, 0, 80, 100)),
            content(3, SurfaceLayer::View, Rect::new(200, 0, 20, 20)),
        ];
        let node = SceneNode::new(view_id, 0, placement, EffectStyle::default())
            .with_content(ContentSpan::new(0, contents.len()).unwrap());
        let scene = SceneSnapshot::with_content(
            WorkspaceId::new(1),
            Rect::new(0, 0, 200, 100),
            vec![node],
            contents,
        );

        let submitted = submitted_scene_surfaces(&scene);
        assert_eq!(submitted.get(&SurfaceId::new(1)), Some(&view_id));
        assert_eq!(submitted.get(&SurfaceId::new(2)), Some(&view_id));
        assert!(!submitted.contains_key(&SurfaceId::new(3)));

        let bounds = scene_surface_bounds(&scene);
        assert_eq!(bounds[&SurfaceId::new(1)], Rect::new(100, 0, 50, 100));
        assert_eq!(bounds[&SurfaceId::new(2)], Rect::new(170, 0, 30, 100));
        assert!(!bounds.contains_key(&SurfaceId::new(3)));
    }

    #[test]
    fn fixed_refresh_rounds_a_millihertz_mode_to_nanoseconds() {
        assert_eq!(
            refresh_from_millihertz(60_000),
            Refresh::Fixed(Duration::from_nanos(16_666_667))
        );
        assert_eq!(refresh_from_millihertz(0), Refresh::Unknown);
        assert_eq!(refresh_from_millihertz(-1), Refresh::Unknown);
    }

    #[test]
    fn zero_hardware_timestamp_keeps_sequence_without_claiming_hardware_clock() {
        let sample = presentation_sample(
            Some(VblankMetadata {
                timestamp: Duration::ZERO,
                sequence: 11,
                clock: VblankClock::Monotonic,
            }),
            Duration::from_secs(4),
            Some(60_000),
        );
        assert_eq!(sample.time, Duration::from_secs(4));
        assert_eq!(sample.sequence, 11);
        assert!(
            !sample
                .flags
                .contains(wp_presentation_feedback::Kind::HwClock)
        );
    }

    #[test]
    fn monotonic_kernel_metadata_is_the_only_hardware_clock() {
        let fallback = Duration::from_secs(3);
        let monotonic = presentation_sample(
            Some(VblankMetadata {
                timestamp: Duration::from_secs(2),
                sequence: 9,
                clock: VblankClock::Monotonic,
            }),
            fallback,
            Some(60_000),
        );
        assert_eq!(monotonic.time, Duration::from_secs(2));
        assert_eq!(monotonic.sequence, 9);
        assert!(
            monotonic
                .flags
                .contains(wp_presentation_feedback::Kind::HwClock)
        );

        let realtime = presentation_sample(
            Some(VblankMetadata {
                timestamp: Duration::ZERO,
                sequence: 10,
                clock: VblankClock::Realtime,
            }),
            fallback,
            Some(60_000),
        );
        assert_eq!(realtime.time, fallback);
        assert_eq!(realtime.sequence, 10);
        assert!(
            !realtime
                .flags
                .contains(wp_presentation_feedback::Kind::HwClock)
        );

        let missing = presentation_sample(None, fallback, None);
        assert_eq!(missing.time, fallback);
        assert_eq!(missing.sequence, 0);
        assert_eq!(missing.refresh, Refresh::Unknown);
    }

    fn content(id: u64, layer: SurfaceLayer, local_geometry: Rect) -> SurfaceContent {
        SurfaceContent {
            surface_id: SurfaceId::new(id),
            buffer_id: SurfaceBufferId::new(id),
            revision: ContentRevision::new(1),
            layer,
            alpha: Default::default(),
            color: Default::default(),
            local_geometry,
            sample_transform: SurfaceSampleTransform::IDENTITY,
        }
    }
}
