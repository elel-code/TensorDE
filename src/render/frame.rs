use std::collections::BTreeMap;

use tensor_util::OutputScale;
use tensor_util::Rect;
use thiserror::Error;

use crate::scene::{DamageSet, SceneSnapshot};

use super::{CursorOverlay, format::OutputFormat};

#[cfg(test)]
mod cursor_tests;
mod heap;
mod plan;
use heap::DescriptorHeap;
use plan::FrameDrawPlan;
pub(crate) use plan::SceneDrawCommand;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RenderOutputId {
    pub(crate) device_id: u64,
    pub(crate) connector_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeOutputTarget {
    pub(crate) output: RenderOutputId,
    pub(crate) viewport: Rect,
    pub(crate) format: OutputFormat,
    pub(crate) scale: OutputScale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HeapAllocation {
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FrameSubmission {
    pub(crate) target: NativeOutputTarget,
    pub(crate) output_slot: u8,
    pub(crate) serial: u64,
    pub(crate) timeline_value: u64,
    pub(crate) scene: SceneSnapshot,
    pub(crate) cursor: Option<CursorOverlay>,
    pub(crate) damage: DamageSet,
    pub(crate) descriptors: HeapAllocation,
    pub(crate) client_image_descriptors: u32,
    pub(crate) draw_plan: FrameDrawPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DescriptorHeapLayout {
    pub(crate) capacity: u64,
    /// Common power-of-two alignment satisfying both resource-heap binding
    /// and sampled-image descriptor addressing.
    pub(crate) alignment: u64,
    pub(crate) reserved_range: u64,
    pub(crate) descriptor_size: u64,
}

#[derive(Debug)]
pub(crate) struct FrameScheduler {
    descriptors: DescriptorHeap,
    outputs: BTreeMap<RenderOutputId, OutputFrameState>,
    next_timeline_value: u64,
    device_lost: bool,
    descriptor_stride: u64,
    descriptor_size: u64,
}

impl FrameScheduler {
    pub(crate) fn new(
        descriptor_heap_size: u64,
        descriptor_alignment: u64,
        reserved_range: u64,
        descriptor_size: u64,
    ) -> Result<Self, FrameError> {
        if descriptor_size == 0 {
            return Err(FrameError::InvalidDescriptorSize);
        }
        Ok(Self {
            descriptors: DescriptorHeap::new(
                descriptor_heap_size,
                descriptor_alignment,
                reserved_range,
            )?,
            outputs: BTreeMap::new(),
            next_timeline_value: 1,
            device_lost: false,
            descriptor_stride: align_up(descriptor_size, descriptor_alignment)
                .ok_or(FrameError::DescriptorSizeOverflow)?,
            descriptor_size,
        })
    }

    pub(crate) fn register_output(&mut self, target: NativeOutputTarget) -> Result<(), FrameError> {
        if target.viewport.width == 0 || target.viewport.height == 0 {
            return Err(FrameError::InvalidViewport(target.viewport));
        }
        if target.format.format.modifier.is_invalid() {
            return Err(FrameError::ImplicitOutputModifier(target.output));
        }
        if target.format.plane_count == 0 {
            return Err(FrameError::InvalidOutputPlaneCount(target.output));
        }
        if let Some(state) = self.outputs.get_mut(&target.output) {
            if state.target != target {
                if let Some(prepared) = state.prepared {
                    return Err(FrameError::OutputBusy {
                        output: target.output,
                        waiting_for: prepared.timeline_value,
                    });
                }
                state.target = target;
                state.previous_scene = None;
                state.previous_cursor = None;
                state.next_slot = 0;
            }
        } else {
            self.outputs
                .insert(target.output, OutputFrameState::new(target));
        }
        Ok(())
    }

    pub(crate) fn unregister_output(&mut self, output: RenderOutputId) {
        if let Some(state) = self.outputs.remove(&output)
            && let Some(prepared) = state.prepared
        {
            self.descriptors.cancel(prepared.descriptors);
        }
    }

    pub(crate) fn output_count(&self) -> usize {
        self.outputs.len()
    }

    pub(crate) fn next_output_slot(&self, output: RenderOutputId) -> Option<u8> {
        self.outputs
            .get(&output)
            .filter(|state| !self.device_lost && state.prepared.is_none() && !state.in_flight)
            .map(|state| state.next_slot)
    }

    #[cfg(test)]
    pub(crate) fn output_waiting_for_gpu(&self, output: RenderOutputId) -> bool {
        !self.device_lost
            && self
                .outputs
                .get(&output)
                .is_some_and(|state| state.in_flight)
    }

    pub(crate) fn advance_output_slot(&mut self, output: RenderOutputId) -> Option<u8> {
        let state = self
            .outputs
            .get_mut(&output)
            .filter(|state| !self.device_lost && state.prepared.is_none() && !state.in_flight)?;
        state.next_slot = (state.next_slot + 1) % OUTPUT_SLOT_COUNT;
        Some(state.next_slot)
    }

    pub(crate) const fn layout(&self) -> DescriptorHeapLayout {
        DescriptorHeapLayout {
            capacity: self.descriptors.capacity,
            alignment: self.descriptors.alignment,
            reserved_range: self.descriptors.first_usable_offset,
            descriptor_size: self.descriptor_size,
        }
    }

    #[cfg(test)]
    pub(crate) fn prepare(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        self.prepare_with_cursor(output, scene, None, completed_timeline)
    }

    /// Prepare a frame with a compositor-owned output overlay. Client scene
    /// state remains in ECS; input-driven overlays enter only here after the
    /// protocol boundary has converted them to physical output coordinates.
    pub(crate) fn prepare_with_cursor(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        cursor: Option<CursorOverlay>,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        if self.device_lost {
            return Err(FrameError::DeviceLost);
        }
        let state = self
            .outputs
            .get_mut(&output)
            .ok_or(FrameError::UnknownOutput(output))?;
        if let Some(prepared) = state.prepared {
            return Err(FrameError::OutputBusy {
                output,
                waiting_for: prepared.timeline_value,
            });
        }
        if state.in_flight && completed_timeline < state.last_submitted_timeline {
            return Err(FrameError::OutputBusy {
                output,
                waiting_for: state.last_submitted_timeline,
            });
        }
        self.descriptors.reclaim(completed_timeline);

        let timeline_value = self.next_timeline_value;
        let next_timeline_value = self
            .next_timeline_value
            .checked_add(1)
            .ok_or(FrameError::TimelineExhausted)?;
        let serial = state.next_serial;
        serial.checked_add(1).ok_or(FrameError::SerialExhausted)?;
        let draw_plan = FrameDrawPlan::build_with_cursor(&scene, state.target, cursor)?;
        let client_image_descriptors = u32::try_from(draw_plan.images().len())
            .map_err(|_| FrameError::DescriptorSizeOverflow)?;
        let descriptor_count = 1u64
            .checked_add(u64::from(client_image_descriptors))
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let descriptor_bytes = self
            .descriptor_stride
            .checked_mul(descriptor_count)
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let descriptors = self
            .descriptors
            .allocate(descriptor_bytes, timeline_value)?;
        let mut damage = scene
            .damage_since(state.previous_scene.as_ref())
            .to_physical(scene.viewport, state.target.viewport, state.target.scale);
        if state.previous_cursor != cursor {
            for overlay in [state.previous_cursor, cursor].into_iter().flatten() {
                damage.add_region(overlay.clip, state.target.viewport);
            }
        }
        let output_slot = state.next_slot;
        self.next_timeline_value = next_timeline_value;
        state.prepared = Some(PreparedFrameState {
            timeline_value,
            serial,
            output_slot,
            descriptors,
        });

        Ok(FrameSubmission {
            target: state.target,
            output_slot,
            serial,
            timeline_value,
            scene,
            cursor,
            damage,
            descriptors,
            client_image_descriptors,
            draw_plan,
        })
    }

    /// Convenience path for callers that have no external submission stage.
    /// The renderer uses `prepare`/`commit` so a Vulkan failure can abort safely.
    #[cfg(test)]
    pub(crate) fn submit(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        let frame = self.prepare(output, scene, completed_timeline)?;
        if let Err(error) = self.commit(&frame) {
            let _ = self.abort(&frame);
            return Err(error);
        }
        Ok(frame)
    }

    pub(crate) fn commit(&mut self, frame: &FrameSubmission) -> Result<(), FrameError> {
        let state = self
            .outputs
            .get_mut(&frame.target.output)
            .ok_or(FrameError::UnknownOutput(frame.target.output))?;
        let prepared = state
            .prepared
            .filter(|prepared| prepared.matches(frame))
            .ok_or(FrameError::StalePreparedFrame {
                output: frame.target.output,
                timeline_value: frame.timeline_value,
            })?;
        state.prepared = None;
        state.next_slot = (prepared.output_slot + 1) % OUTPUT_SLOT_COUNT;
        state.next_serial = prepared
            .serial
            .checked_add(1)
            .ok_or(FrameError::SerialExhausted)?;
        state.previous_scene = Some(frame.scene.clone());
        state.previous_cursor = frame.cursor;
        state.last_submitted_timeline = prepared.timeline_value;
        state.in_flight = true;
        Ok(())
    }

    pub(crate) fn abort(&mut self, frame: &FrameSubmission) -> Result<(), FrameError> {
        let state = self
            .outputs
            .get_mut(&frame.target.output)
            .ok_or(FrameError::UnknownOutput(frame.target.output))?;
        let prepared = state
            .prepared
            .filter(|prepared| prepared.matches(frame))
            .ok_or(FrameError::StalePreparedFrame {
                output: frame.target.output,
                timeline_value: frame.timeline_value,
            })?;
        state.prepared = None;
        self.descriptors.cancel(prepared.descriptors);
        Ok(())
    }

    pub(crate) fn retire_completed(&mut self, timeline_value: u64) {
        for state in self.outputs.values_mut() {
            if state.in_flight && state.last_submitted_timeline <= timeline_value {
                state.in_flight = false;
            }
        }
        self.descriptors.reclaim(timeline_value);
    }

    pub(crate) fn mark_device_lost(&mut self) {
        self.device_lost = true;
    }
}

#[derive(Debug)]
struct OutputFrameState {
    target: NativeOutputTarget,
    previous_scene: Option<SceneSnapshot>,
    previous_cursor: Option<CursorOverlay>,
    next_serial: u64,
    last_submitted_timeline: u64,
    in_flight: bool,
    next_slot: u8,
    prepared: Option<PreparedFrameState>,
}

const OUTPUT_SLOT_COUNT: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreparedFrameState {
    timeline_value: u64,
    serial: u64,
    output_slot: u8,
    descriptors: HeapAllocation,
}

impl PreparedFrameState {
    fn matches(self, frame: &FrameSubmission) -> bool {
        self.timeline_value == frame.timeline_value
            && self.serial == frame.serial
            && self.output_slot == frame.output_slot
            && self.descriptors == frame.descriptors
    }
}

impl OutputFrameState {
    fn new(target: NativeOutputTarget) -> Self {
        Self {
            target,
            previous_scene: None,
            previous_cursor: None,
            next_serial: 1,
            last_submitted_timeline: 0,
            in_flight: false,
            next_slot: 0,
            prepared: None,
        }
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    value.checked_add((alignment - remainder) % alignment)
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum FrameError {
    #[error("descriptor heap capacity {capacity} does not exceed reserved range {reserved}")]
    DescriptorHeapTooSmall { capacity: u64, reserved: u64 },
    #[error("descriptor alignment {alignment} must be a non-zero power of two")]
    InvalidDescriptorAlignment { alignment: u64 },
    #[error("descriptor size must be non-zero")]
    InvalidDescriptorSize,
    #[error("descriptor size overflowed the frame allocator")]
    DescriptorSizeOverflow,
    #[error("descriptor request of {requested} bytes exceeds heap capacity {capacity}")]
    DescriptorRequestTooLarge { requested: u64, capacity: u64 },
    #[error("descriptor heap exhausted: requested {requested} bytes, capacity {capacity}")]
    DescriptorHeapExhausted { requested: u64, capacity: u64 },
    #[error("output {0:?} is not registered with the renderer")]
    UnknownOutput(RenderOutputId),
    #[error("output {output:?} is still using frame timeline {waiting_for}")]
    OutputBusy {
        output: RenderOutputId,
        waiting_for: u64,
    },
    #[error("output {output:?} has no prepared frame at timeline {timeline_value}")]
    StalePreparedFrame {
        output: RenderOutputId,
        timeline_value: u64,
    },
    #[error("output viewport {0:?} has zero width or height")]
    InvalidViewport(Rect),
    #[error("output {0:?} requires an explicit DRM modifier")]
    ImplicitOutputModifier(RenderOutputId),
    #[error("output {0:?} has no dma-buf planes")]
    InvalidOutputPlaneCount(RenderOutputId),
    #[error("renderer timeline value space is exhausted")]
    TimelineExhausted,
    #[error("renderer frame serial space is exhausted")]
    SerialExhausted,
    #[error("Vulkan device was lost; frame submission is stopped")]
    DeviceLost,
}

#[cfg(test)]
mod tests {
    use tensor_host::{DrmFormat, Fourcc, Modifier};

    use super::*;
    use crate::{
        ecs::{SurfaceBufferId, SurfaceId, ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{
            ContentRevision, ContentSpan, EffectStyle, SceneNode, SurfaceContent, SurfaceLayer,
            SurfaceSampleTransform,
        },
    };

    const OUTPUT: RenderOutputId = RenderOutputId {
        device_id: 1,
        connector_id: 2,
    };
    const SECOND_OUTPUT: RenderOutputId = RenderOutputId {
        device_id: 1,
        connector_id: 3,
    };
    const VIEWPORT: Rect = Rect::new(0, 0, 1920, 1080);

    fn target(output: RenderOutputId) -> NativeOutputTarget {
        NativeOutputTarget {
            output,
            viewport: VIEWPORT,
            format: OutputFormat {
                format: DrmFormat {
                    code: Fourcc::XRGB8888,
                    modifier: Modifier::from_raw(9),
                },
                plane_count: 1,
            },
            scale: OutputScale::ONE,
        }
    }

    fn scene(view_id: u64) -> SceneSnapshot {
        scene_in(view_id, VIEWPORT)
    }

    fn scene_in(view_id: u64, viewport: Rect) -> SceneSnapshot {
        let contents = vec![SurfaceContent {
            surface_id: SurfaceId::new(view_id),
            buffer_id: SurfaceBufferId::new(view_id),
            revision: ContentRevision::new(1),
            layer: SurfaceLayer::View,
            alpha: Default::default(),
            local_geometry: Rect::new(0, 0, 640, 480),
            sample_transform: SurfaceSampleTransform::IDENTITY,
        }];
        SceneSnapshot::with_content(
            WorkspaceId::new(0),
            viewport,
            vec![
                SceneNode::new(
                    ViewId::new(view_id),
                    view_id,
                    LayoutPlacement {
                        geometry: Rect::new(0, 0, 640, 480),
                        visible: Some(Rect::new(0, 0, 640, 480)),
                    },
                    EffectStyle::default(),
                )
                .with_content(ContentSpan::new(0, 1).unwrap()),
            ],
            contents,
        )
    }

    #[test]
    fn first_frame_and_scene_change_produce_damage() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert_eq!(first.serial, 1);
        assert_eq!(first.damage.regions(), &[VIEWPORT]);
        scheduler.retire_completed(first.timeline_value);

        let second = scheduler
            .submit(OUTPUT, scene(2), first.timeline_value)
            .unwrap();
        assert!(!second.damage.is_empty());
        assert_eq!(second.serial, 2);
    }

    #[test]
    fn fractional_target_scales_draws_and_damage_to_physical_pixels() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        let scaled_target = NativeOutputTarget {
            scale: OutputScale::from_f64(1.25).unwrap(),
            ..target(OUTPUT)
        };
        scheduler.register_output(scaled_target).unwrap();
        let logical_viewport = Rect::new(0, 0, 1536, 864);
        let frame = scheduler
            .submit(OUTPUT, scene_in(1, logical_viewport), 0)
            .unwrap();

        assert_eq!(frame.damage.regions(), [VIEWPORT]);
        assert_eq!(
            frame.draw_plan.draws()[0].destination,
            Rect::new(0, 0, 800, 600)
        );
        assert_eq!(frame.draw_plan.draws()[0].clip, Rect::new(0, 0, 800, 600));
    }

    #[test]
    fn in_flight_output_cannot_reuse_descriptors() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert!(matches!(
            scheduler.submit(OUTPUT, scene(2), 0),
            Err(FrameError::OutputBusy { .. })
        ));
        scheduler.retire_completed(first.timeline_value);
        assert!(
            scheduler
                .submit(OUTPUT, scene(2), first.timeline_value)
                .is_ok()
        );
    }

    #[test]
    fn descriptor_exhaustion_is_reported_until_timeline_retires() {
        let mut scheduler = FrameScheduler::new(96, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        scheduler.register_output(target(SECOND_OUTPUT)).unwrap();
        let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert!(matches!(
            scheduler.submit(SECOND_OUTPUT, scene(2), 0),
            Err(FrameError::DescriptorHeapExhausted { .. })
        ));
        scheduler.retire_completed(first.timeline_value);
        let second = scheduler
            .submit(SECOND_OUTPUT, scene(2), first.timeline_value)
            .unwrap();
        assert_eq!(second.descriptors.offset, 0);
    }

    #[test]
    fn invalid_and_unknown_outputs_fail_at_boundary() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        assert!(matches!(
            scheduler.register_output(NativeOutputTarget {
                viewport: Rect::new(0, 0, 0, 100),
                ..target(OUTPUT)
            }),
            Err(FrameError::InvalidViewport(_))
        ));
        assert!(matches!(
            scheduler.submit(OUTPUT, scene(1), 0),
            Err(FrameError::UnknownOutput(_))
        ));
    }

    #[test]
    fn device_loss_stops_future_frame_submission() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        scheduler.mark_device_lost();
        assert_eq!(
            scheduler.submit(OUTPUT, scene(1), 0),
            Err(FrameError::DeviceLost)
        );
    }

    #[test]
    fn descriptor_heap_respects_reserved_range_and_alignment() {
        let mut scheduler = FrameScheduler::new(4096, 64, 96, 48).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert_eq!(frame.descriptors.offset, 128);
        assert_eq!(frame.descriptors.size, 128);
    }

    #[test]
    fn invalid_descriptor_heap_layout_fails_before_output_registration() {
        assert!(matches!(
            FrameScheduler::new(4096, 0, 0, 32),
            Err(FrameError::InvalidDescriptorAlignment { .. })
        ));
        assert!(matches!(
            FrameScheduler::new(4096, 64, 4096, 32),
            Err(FrameError::DescriptorHeapTooSmall { .. })
        ));
    }

    #[test]
    fn native_target_requires_explicit_modifier_and_planes() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        let implicit = NativeOutputTarget {
            format: OutputFormat {
                format: DrmFormat {
                    code: Fourcc::XRGB8888,
                    modifier: Modifier::INVALID,
                },
                plane_count: 1,
            },
            ..target(OUTPUT)
        };
        assert!(matches!(
            scheduler.register_output(implicit),
            Err(FrameError::ImplicitOutputModifier(_))
        ));
        let no_planes = NativeOutputTarget {
            format: OutputFormat {
                plane_count: 0,
                ..target(OUTPUT).format
            },
            ..target(OUTPUT)
        };
        assert!(matches!(
            scheduler.register_output(no_planes),
            Err(FrameError::InvalidOutputPlaneCount(_))
        ));
    }

    #[test]
    fn output_slots_cycle_with_the_native_triple_buffer_contract() {
        let mut scheduler = FrameScheduler::new(16 * 1024, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert_eq!(first.output_slot, 0);
        scheduler.retire_completed(first.timeline_value);
        let second = scheduler
            .submit(OUTPUT, scene(2), first.timeline_value)
            .unwrap();
        assert_eq!(second.output_slot, 1);
        scheduler.retire_completed(second.timeline_value);
        let third = scheduler
            .submit(OUTPUT, scene(3), second.timeline_value)
            .unwrap();
        assert_eq!(third.output_slot, 2);
        scheduler.retire_completed(third.timeline_value);
        let fourth = scheduler
            .submit(OUTPUT, scene(4), third.timeline_value)
            .unwrap();
        assert_eq!(fourth.output_slot, 0);
    }

    #[test]
    fn next_slot_is_hidden_while_gpu_work_is_in_flight() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        assert_eq!(scheduler.next_output_slot(OUTPUT), Some(0));

        let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert!(scheduler.output_waiting_for_gpu(OUTPUT));
        assert_eq!(scheduler.next_output_slot(OUTPUT), None);
        scheduler.retire_completed(frame.timeline_value);
        assert!(!scheduler.output_waiting_for_gpu(OUTPUT));
        assert_eq!(scheduler.next_output_slot(OUTPUT), Some(1));
    }

    #[test]
    fn idle_output_can_rotate_around_kms_owned_slots() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();

        assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(1));
        assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(2));
        assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(0));

        let frame = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        assert_eq!(scheduler.advance_output_slot(OUTPUT), None);
        scheduler.retire_completed(frame.timeline_value);
        assert_eq!(scheduler.advance_output_slot(OUTPUT), Some(2));
    }

    #[test]
    fn descriptor_heap_layout_exposes_raw_descriptor_size_and_aligned_start() {
        let scheduler = FrameScheduler::new(4096, 64, 96, 48).unwrap();
        assert_eq!(scheduler.layout().reserved_range, 128);
        assert_eq!(scheduler.layout().descriptor_size, 48);
        assert_eq!(scheduler.layout().capacity, 4096);
    }

    #[test]
    fn target_change_preserves_in_flight_lifetime_and_resets_damage_history() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let first = scheduler.submit(OUTPUT, scene(1), 0).unwrap();
        let resized = NativeOutputTarget {
            viewport: Rect::new(0, 0, 2560, 1440),
            ..target(OUTPUT)
        };

        scheduler.register_output(resized).unwrap();
        assert!(matches!(
            scheduler.submit(OUTPUT, scene(1), 0),
            Err(FrameError::OutputBusy { .. })
        ));
        scheduler.retire_completed(first.timeline_value);
        let second = scheduler
            .submit(OUTPUT, scene(1), first.timeline_value)
            .unwrap();

        assert_eq!(second.serial, 2);
        assert_eq!(second.target, resized);
        assert_eq!(second.damage.regions(), &[VIEWPORT]);
    }

    #[test]
    fn aborted_prepare_releases_heap_and_preserves_output_sequence() {
        let mut scheduler = FrameScheduler::new(128, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let first = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();

        scheduler.abort(&first).unwrap();
        let retry = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();

        assert_eq!(retry.serial, first.serial);
        assert_eq!(retry.output_slot, first.output_slot);
        assert_eq!(retry.descriptors, first.descriptors);
        assert!(retry.timeline_value > first.timeline_value);
        scheduler.commit(&retry).unwrap();
    }

    #[test]
    fn prepared_frame_blocks_target_replacement_until_resolved() {
        let mut scheduler = FrameScheduler::new(4096, 32, 0, 32).unwrap();
        scheduler.register_output(target(OUTPUT)).unwrap();
        let frame = scheduler.prepare(OUTPUT, scene(1), 0).unwrap();
        let resized = NativeOutputTarget {
            viewport: Rect::new(0, 0, 2560, 1440),
            ..target(OUTPUT)
        };

        assert!(matches!(
            scheduler.register_output(resized),
            Err(FrameError::OutputBusy { .. })
        ));
        scheduler.abort(&frame).unwrap();
        assert!(scheduler.register_output(resized).is_ok());
    }
}
