use std::collections::BTreeMap;

use tensor_util::Rect;
use thiserror::Error;

use crate::scene::{DamageSet, SceneSnapshot};

const DESCRIPTOR_ALIGNMENT: u64 = 32;
const NODE_DESCRIPTOR_BYTES: u64 = 128;
const CLEAR_DESCRIPTOR_BYTES: u64 = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RenderOutputId {
    pub(crate) device_id: u64,
    pub(crate) connector_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HeapAllocation {
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FrameSubmission {
    pub(crate) output: RenderOutputId,
    pub(crate) serial: u64,
    pub(crate) timeline_value: u64,
    pub(crate) viewport: Rect,
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
}

impl FrameScheduler {
    pub(crate) fn new(descriptor_heap_size: u64) -> Result<Self, FrameError> {
        Ok(Self {
            descriptors: DescriptorHeap::new(descriptor_heap_size)?,
            outputs: BTreeMap::new(),
            next_timeline_value: 1,
            device_lost: false,
        })
    }

    pub(crate) fn register_output(
        &mut self,
        output: RenderOutputId,
        viewport: Rect,
    ) -> Result<(), FrameError> {
        if viewport.width == 0 || viewport.height == 0 {
            return Err(FrameError::InvalidViewport(viewport));
        }
        self.outputs
            .entry(output)
            .and_modify(|state| state.viewport = viewport)
            .or_insert_with(|| OutputFrameState::new(viewport));
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
        let descriptor_bytes = CLEAR_DESCRIPTOR_BYTES.saturating_add(
            u64::try_from(scene.nodes().len())
                .unwrap_or(u64::MAX)
                .saturating_mul(NODE_DESCRIPTOR_BYTES),
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
            output,
            serial,
            timeline_value,
            viewport: state.viewport,
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
    viewport: Rect,
    previous_scene: Option<SceneSnapshot>,
    next_serial: u64,
    last_submitted_timeline: u64,
    in_flight: bool,
}

impl OutputFrameState {
    fn new(viewport: Rect) -> Self {
        Self {
            viewport,
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
    cursor: u64,
    active: Vec<ActiveAllocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveAllocation {
    allocation: HeapAllocation,
    retire_timeline: u64,
}

impl DescriptorHeap {
    fn new(capacity: u64) -> Result<Self, FrameError> {
        if capacity < DESCRIPTOR_ALIGNMENT {
            return Err(FrameError::DescriptorHeapTooSmall { capacity });
        }
        Ok(Self {
            capacity,
            cursor: 0,
            active: Vec::new(),
        })
    }

    fn allocate(&mut self, size: u64, retire_timeline: u64) -> Result<HeapAllocation, FrameError> {
        let size =
            align_up(size, DESCRIPTOR_ALIGNMENT).ok_or(FrameError::DescriptorSizeOverflow)?;
        if size > self.capacity {
            return Err(FrameError::DescriptorRequestTooLarge {
                requested: size,
                capacity: self.capacity,
            });
        }

        let start = align_up(self.cursor, DESCRIPTOR_ALIGNMENT)
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let offset = if self.fits(start, size) {
            start
        } else if self.fits(0, size) {
            0
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
            self.cursor = 0;
        }
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    value.checked_add((alignment - remainder) % alignment)
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum FrameError {
    #[error("descriptor heap capacity {capacity} is smaller than the required alignment")]
    DescriptorHeapTooSmall { capacity: u64 },
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
    #[error("renderer timeline value space is exhausted")]
    TimelineExhausted,
    #[error("renderer frame serial space is exhausted")]
    SerialExhausted,
    #[error("Vulkan device was lost; frame submission is stopped")]
    DeviceLost,
}

#[cfg(test)]
mod tests {
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
        let mut scheduler = FrameScheduler::new(4096).unwrap();
        scheduler.register_output(OUTPUT, VIEWPORT).unwrap();
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
        let mut scheduler = FrameScheduler::new(4096).unwrap();
        scheduler.register_output(OUTPUT, VIEWPORT).unwrap();
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
        let mut scheduler = FrameScheduler::new(256).unwrap();
        scheduler.register_output(OUTPUT, VIEWPORT).unwrap();
        scheduler.register_output(SECOND_OUTPUT, VIEWPORT).unwrap();
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
        let mut scheduler = FrameScheduler::new(4096).unwrap();
        assert!(matches!(
            scheduler.register_output(OUTPUT, Rect::new(0, 0, 0, 100)),
            Err(FrameError::InvalidViewport(_))
        ));
        assert!(matches!(
            scheduler.submit(OUTPUT, scene(1), 0),
            Err(FrameError::UnknownOutput(_))
        ));
    }

    #[test]
    fn device_loss_stops_future_frame_submission() {
        let mut scheduler = FrameScheduler::new(4096).unwrap();
        scheduler.register_output(OUTPUT, VIEWPORT).unwrap();
        scheduler.mark_device_lost();
        assert_eq!(
            scheduler.submit(OUTPUT, scene(1), 0),
            Err(FrameError::DeviceLost)
        );
    }
}
