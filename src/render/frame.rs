use std::collections::BTreeMap;

use tensor_util::Rect;
use thiserror::Error;

use crate::scene::{DamageSet, SceneSnapshot};

use super::format::OutputFormat;

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HeapAllocation {
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FrameSubmission {
    pub(crate) target: NativeOutputTarget,
    pub(crate) serial: u64,
    pub(crate) timeline_value: u64,
    pub(crate) scene: SceneSnapshot,
    pub(crate) damage: DamageSet,
    pub(crate) descriptors: HeapAllocation,
}

#[derive(Debug)]
pub(crate) struct FrameScheduler {
    descriptors: DescriptorHeap,
    outputs: BTreeMap<RenderOutputId, OutputFrameState>,
    next_timeline_value: u64,
    device_lost: bool,
    descriptor_stride: u64,
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
        })
    }

    pub(crate) fn register_output(&mut self, target: NativeOutputTarget) -> Result<(), FrameError> {
        if target.viewport.width == 0 || target.viewport.height == 0 {
            return Err(FrameError::InvalidViewport(target.viewport));
        }
        if target.format.format.modifier == smithay::backend::allocator::Modifier::Invalid {
            return Err(FrameError::ImplicitOutputModifier(target.output));
        }
        if target.format.plane_count == 0 {
            return Err(FrameError::InvalidOutputPlaneCount(target.output));
        }
        self.outputs
            .entry(target.output)
            .and_modify(|state| {
                if state.target != target {
                    state.target = target;
                    state.previous_scene = None;
                }
            })
            .or_insert_with(|| OutputFrameState::new(target));
        Ok(())
    }

    pub(crate) fn unregister_output(&mut self, output: RenderOutputId) {
        self.outputs.remove(&output);
    }

    pub(crate) fn output_count(&self) -> usize {
        self.outputs.len()
    }

    pub(crate) fn submit(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        if self.device_lost {
            return Err(FrameError::DeviceLost);
        }
        let state = self
            .outputs
            .get_mut(&output)
            .ok_or(FrameError::UnknownOutput(output))?;
        if state.in_flight && completed_timeline < state.last_submitted_timeline {
            return Err(FrameError::OutputBusy {
                output,
                waiting_for: state.last_submitted_timeline,
            });
        }
        self.descriptors.reclaim(completed_timeline);

        let timeline_value = self.next_timeline_value;
        self.next_timeline_value = self
            .next_timeline_value
            .checked_add(1)
            .ok_or(FrameError::TimelineExhausted)?;
        let descriptor_bytes = self.descriptor_stride.saturating_add(
            u64::try_from(scene.nodes().len())
                .unwrap_or(u64::MAX)
                .saturating_mul(self.descriptor_stride),
        );
        let descriptors = self
            .descriptors
            .allocate(descriptor_bytes, timeline_value)?;
        let damage = scene.damage_since(state.previous_scene.as_ref());
        let serial = state.next_serial;
        state.next_serial = state
            .next_serial
            .checked_add(1)
            .ok_or(FrameError::SerialExhausted)?;
        state.previous_scene = Some(scene.clone());
        state.last_submitted_timeline = timeline_value;
        state.in_flight = true;

        Ok(FrameSubmission {
            target: state.target,
            serial,
            timeline_value,
            scene,
            damage,
            descriptors,
        })
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
    next_serial: u64,
    last_submitted_timeline: u64,
    in_flight: bool,
}

impl OutputFrameState {
    fn new(target: NativeOutputTarget) -> Self {
        Self {
            target,
            previous_scene: None,
            next_serial: 1,
            last_submitted_timeline: 0,
            in_flight: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct DescriptorHeap {
    capacity: u64,
    alignment: u64,
    first_usable_offset: u64,
    cursor: u64,
    active: Vec<ActiveAllocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveAllocation {
    allocation: HeapAllocation,
    retire_timeline: u64,
}

impl DescriptorHeap {
    fn new(capacity: u64, alignment: u64, reserved_range: u64) -> Result<Self, FrameError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(FrameError::InvalidDescriptorAlignment { alignment });
        }
        let first_usable_offset =
            align_up(reserved_range, alignment).ok_or(FrameError::DescriptorSizeOverflow)?;
        if capacity <= first_usable_offset {
            return Err(FrameError::DescriptorHeapTooSmall {
                capacity,
                reserved: first_usable_offset,
            });
        }
        Ok(Self {
            capacity,
            alignment,
            first_usable_offset,
            cursor: first_usable_offset,
            active: Vec::new(),
        })
    }

    fn allocate(&mut self, size: u64, retire_timeline: u64) -> Result<HeapAllocation, FrameError> {
        let size = align_up(size, self.alignment).ok_or(FrameError::DescriptorSizeOverflow)?;
        if size > self.capacity.saturating_sub(self.first_usable_offset) {
            return Err(FrameError::DescriptorRequestTooLarge {
                requested: size,
                capacity: self.capacity,
            });
        }

        let start =
            align_up(self.cursor, self.alignment).ok_or(FrameError::DescriptorSizeOverflow)?;
        let offset = if self.fits(start, size) {
            start
        } else if self.fits(self.first_usable_offset, size) {
            self.first_usable_offset
        } else {
            return Err(FrameError::DescriptorHeapExhausted {
                requested: size,
                capacity: self.capacity,
            });
        };
        let allocation = HeapAllocation { offset, size };
        self.cursor = offset.saturating_add(size);
        self.active.push(ActiveAllocation {
            allocation,
            retire_timeline,
        });
        Ok(allocation)
    }

    fn fits(&self, offset: u64, size: u64) -> bool {
        let Some(end) = offset.checked_add(size) else {
            return false;
        };
        end <= self.capacity
            && self.active.iter().all(|active| {
                let active_end = active
                    .allocation
                    .offset
                    .saturating_add(active.allocation.size);
                end <= active.allocation.offset || offset >= active_end
            })
    }

    fn reclaim(&mut self, completed_timeline: u64) {
        self.active
            .retain(|active| active.retire_timeline > completed_timeline);
        if self.active.is_empty() && self.cursor >= self.capacity {
            self.cursor = self.first_usable_offset;
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
    use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};

    use super::*;
    use crate::{
        ecs::{ViewId, WorkspaceId},
        layout::LayoutPlacement,
        scene::{EffectStyle, SceneNode},
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
                    code: Fourcc::Xrgb8888,
                    modifier: Modifier::from(9),
                },
                plane_count: 1,
            },
        }
    }

    fn scene(view_id: u64) -> SceneSnapshot {
        SceneSnapshot::new(
            WorkspaceId::new(0),
            VIEWPORT,
            vec![SceneNode::new(
                ViewId::new(view_id),
                view_id,
                LayoutPlacement {
                    geometry: Rect::new(0, 0, 640, 480),
                    visible: Some(Rect::new(0, 0, 640, 480)),
                },
                EffectStyle::default(),
            )],
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
                    code: Fourcc::Xrgb8888,
                    modifier: Modifier::Invalid,
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
}
