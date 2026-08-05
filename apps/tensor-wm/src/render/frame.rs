use std::collections::BTreeMap;

use tensor_util::OutputScale;
use tensor_util::Rect;
use tensor_util::Size;
use thiserror::Error;
use vulkan_renderer::{DescriptorAllocation, DescriptorHeapAllocator, DescriptorHeapError};

use crate::scene::{DamageSet, SceneSnapshot};

use super::{
    CursorOverlay, CursorOverlays, OutputCaptureRequest, cursor::MAX_CURSOR_OVERLAYS,
    format::OutputFormat,
};

#[cfg(test)]
mod cursor_tests;
mod pass;
mod plan;
#[cfg(test)]
pub(crate) use pass::BackdropRegionSpan;
pub(crate) use pass::{
    BACKDROP_INTERMEDIATE_LANE_COUNT, BackdropPass, CompositionPath, FramePassPlan, OutputLoad,
};
#[cfg(test)]
pub(crate) use plan::ClientImageDescriptor;
use plan::FrameDrawPlan;
pub(crate) use plan::SceneDrawCommand;
pub(in crate::render) use plan::ShadowDraw;

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
pub(crate) struct NativeCursorTarget {
    pub(crate) output: RenderOutputId,
    pub(crate) size: Size,
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
    pub(crate) output_slot: u8,
    pub(crate) serial: u64,
    pub(crate) timeline_value: u64,
    pub(crate) scene: SceneSnapshot,
    /// Damage relative to the latest committed scene. Protocol callbacks and
    /// diagnostics consume this semantic delta.
    pub(crate) damage: DamageSet,
    /// Accumulated damage relative to the contents last rendered into this
    /// exact native output slot. Vulkan partial rendering consumes this delta.
    pub(crate) render_damage: DamageSet,
    pub(crate) pass_plan: FramePassPlan,
    pub(crate) descriptors: HeapAllocation,
    pub(crate) client_image_descriptors: u32,
    pub(crate) draw_plan: FrameDrawPlan,
}

#[derive(Debug)]
pub(crate) struct FrameScheduler {
    descriptors: DescriptorHeapAllocator,
    outputs: BTreeMap<RenderOutputId, OutputFrameState>,
    #[cfg(test)]
    next_timeline_value: u64,
    device_lost: bool,
    descriptor_stride: u64,
    descriptor_alignment: u64,
}

impl FrameScheduler {
    #[cfg(test)]
    pub(crate) fn new(
        descriptor_heap_size: u64,
        descriptor_alignment: u64,
        reserved_range: u64,
        descriptor_size: u64,
    ) -> Result<Self, FrameError> {
        if descriptor_size == 0 {
            return Err(FrameError::InvalidDescriptorSize);
        }
        if descriptor_alignment == 0 || !descriptor_alignment.is_power_of_two() {
            return Err(FrameError::InvalidDescriptorAlignment {
                alignment: descriptor_alignment,
            });
        }
        let descriptor_stride = align_up(descriptor_size, descriptor_alignment)
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let descriptors = DescriptorHeapAllocator::new(
            descriptor_heap_size,
            reserved_range,
            descriptor_alignment,
        )
        .map_err(frame_allocator_error)?;
        Self::with_descriptor_allocator(descriptors, descriptor_stride, descriptor_alignment)
    }

    /// Connects Tensor's output/frame policy to the allocation state owned by
    /// one shared `vulkan-renderer` resource heap. The scheduler never
    /// recreates offsets or a second retirement namespace.
    pub(crate) fn with_descriptor_allocator(
        descriptors: DescriptorHeapAllocator,
        descriptor_stride: u64,
        descriptor_alignment: u64,
    ) -> Result<Self, FrameError> {
        if descriptor_stride == 0 {
            return Err(FrameError::InvalidDescriptorSize);
        }
        if descriptor_alignment == 0 || !descriptor_alignment.is_power_of_two() {
            return Err(FrameError::InvalidDescriptorAlignment {
                alignment: descriptor_alignment,
            });
        }
        if !descriptor_stride.is_multiple_of(descriptor_alignment) {
            return Err(FrameError::InvalidDescriptorAlignment {
                alignment: descriptor_alignment,
            });
        }
        Ok(Self {
            descriptors,
            outputs: BTreeMap::new(),
            #[cfg(test)]
            next_timeline_value: 1,
            device_lost: false,
            descriptor_stride,
            descriptor_alignment,
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
                if let Some(prepared) = state.prepared.as_ref() {
                    return Err(FrameError::OutputBusy {
                        output: target.output,
                        waiting_for: prepared.timeline_value,
                    });
                }
                state.target = target;
                state.previous_scene = None;
                state.previous_cursors.clear();
                state.clear_slot_history();
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
            let _ = self.descriptors.release(prepared.allocation);
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

    /// Resolves the live shared allocation for a prepared frame. The Vulkan
    /// executor borrows this exact allocation for descriptor encoding, while
    /// this scheduler retains lifecycle ownership until commit or abort.
    pub(crate) fn descriptor_allocation(
        &self,
        frame: &FrameSubmission,
    ) -> Result<&DescriptorAllocation, FrameError> {
        self.outputs
            .get(&frame.target.output)
            .and_then(|state| state.prepared.as_ref())
            .filter(|prepared| prepared.matches(frame))
            .map(|prepared| &prepared.allocation)
            .ok_or(FrameError::StalePreparedFrame {
                output: frame.target.output,
                timeline_value: frame.timeline_value,
            })
    }

    #[cfg(test)]
    pub(crate) fn prepare(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        self.prepare_with_cursors(output, scene, CursorOverlays::default(), completed_timeline)
    }

    /// Prepare a frame with a compositor-owned output overlay. Client scene
    /// state remains in ECS; input-driven overlays enter only here after the
    /// protocol boundary has converted them to physical output coordinates.
    #[cfg(test)]
    pub(crate) fn prepare_with_cursors(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        cursors: CursorOverlays,
        completed_timeline: u64,
    ) -> Result<FrameSubmission, FrameError> {
        let timeline_value = self.next_timeline_value;
        let next_timeline_value = timeline_value
            .checked_add(1)
            .ok_or(FrameError::TimelineExhausted)?;
        let frame = self.prepare_with_cursors_for_timeline(
            output,
            scene,
            cursors,
            None,
            completed_timeline,
            timeline_value,
        )?;
        self.next_timeline_value = next_timeline_value;
        Ok(frame)
    }

    /// Prepares one frame for a timeline value reserved by the shared Vulkan
    /// device. Tensor owns scene/output policy, while the renderer remains
    /// the sole owner of the device timeline and queue submission order.
    pub(crate) fn prepare_with_cursors_for_timeline(
        &mut self,
        output: RenderOutputId,
        scene: SceneSnapshot,
        cursors: CursorOverlays,
        capture: Option<OutputCaptureRequest>,
        completed_timeline: u64,
        timeline_value: u64,
    ) -> Result<FrameSubmission, FrameError> {
        if timeline_value == 0 {
            return Err(FrameError::InvalidTimelineValue);
        }
        if self.device_lost {
            return Err(FrameError::DeviceLost);
        }
        let descriptor_capacity = self.descriptors.reserved_range_offset();
        let state = self
            .outputs
            .get_mut(&output)
            .ok_or(FrameError::UnknownOutput(output))?;
        if let Some(prepared) = state.prepared.as_ref() {
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

        let serial = state.next_serial;
        serial.checked_add(1).ok_or(FrameError::SerialExhausted)?;
        let draw_plan = FrameDrawPlan::build_with_cursors(&scene, state.target, cursors)?;
        let client_image_descriptors = u32::try_from(draw_plan.images().len())
            .map_err(|_| FrameError::DescriptorSizeOverflow)?;
        let mut damage = scene
            .damage_since(state.previous_scene.as_ref())
            .to_physical(scene.viewport, state.target.viewport, state.target.scale);
        let current_cursors = draw_plan.cursors();
        add_cursor_damage(
            &mut damage,
            state.previous_cursors.as_slice(),
            current_cursors,
            state.target.viewport,
        );
        let output_slot = state.next_slot;
        let slot = &state.slot_history[usize::from(output_slot)];
        let mut render_damage = scene.damage_since(slot.scene.as_ref()).to_physical(
            scene.viewport,
            state.target.viewport,
            state.target.scale,
        );
        add_cursor_damage(
            &mut render_damage,
            slot.cursors.as_slice(),
            current_cursors,
            state.target.viewport,
        );
        if let Some(capture) = capture.filter(|request| request.tap_before_software_cursors()) {
            add_capture_cursor_exclusion_damage(
                &mut render_damage,
                slot.cursors.as_slice(),
                current_cursors,
                capture.region,
                state.target.viewport,
            );
        }
        let pass_plan = FramePassPlan::build(&scene, state.target, &render_damage);
        let descriptor_count = 1u64
            .checked_add(u64::from(client_image_descriptors))
            .and_then(|count| {
                count.checked_add(u64::from(pass_plan.intermediate_descriptor_count()))
            })
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let descriptor_bytes = self
            .descriptor_stride
            .checked_mul(descriptor_count)
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let allocation = self
            .descriptors
            .allocate(descriptor_bytes, self.descriptor_alignment)
            .map_err(|error| frame_allocation_error(error, descriptor_capacity))?;
        let descriptors = HeapAllocation {
            offset: allocation.offset(),
            size: allocation.size(),
        };
        state.prepared = Some(PreparedFrameState {
            timeline_value,
            serial,
            output_slot,
            descriptors,
            allocation,
        });

        Ok(FrameSubmission {
            target: state.target,
            output_slot,
            serial,
            timeline_value,
            scene,
            damage,
            render_damage,
            pass_plan,
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
        let descriptor_capacity = self.descriptors.reserved_range_offset();
        let state = self
            .outputs
            .get_mut(&frame.target.output)
            .ok_or(FrameError::UnknownOutput(frame.target.output))?;
        if !state
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.matches(frame))
        {
            return Err(FrameError::StalePreparedFrame {
                output: frame.target.output,
                timeline_value: frame.timeline_value,
            });
        }
        let prepared = state
            .prepared
            .take()
            .expect("prepared-frame match retains the allocation");
        self.descriptors
            .retire_at_timeline(prepared.allocation, prepared.timeline_value)
            .map_err(|error| frame_allocation_error(error, descriptor_capacity))?;
        state.next_slot = (prepared.output_slot + 1) % OUTPUT_SLOT_COUNT;
        state.next_serial = prepared
            .serial
            .checked_add(1)
            .ok_or(FrameError::SerialExhausted)?;
        state.previous_scene = Some(frame.scene.clone());
        state.previous_cursors.clear();
        state
            .previous_cursors
            .extend_from_slice(frame.draw_plan.cursors());
        let slot = &mut state.slot_history[usize::from(prepared.output_slot)];
        slot.scene = Some(frame.scene.clone());
        slot.cursors.clear();
        slot.cursors.extend_from_slice(frame.draw_plan.cursors());
        state.last_submitted_timeline = prepared.timeline_value;
        state.in_flight = true;
        Ok(())
    }

    pub(crate) fn abort(&mut self, frame: &FrameSubmission) -> Result<(), FrameError> {
        let descriptor_capacity = self.descriptors.reserved_range_offset();
        let state = self
            .outputs
            .get_mut(&frame.target.output)
            .ok_or(FrameError::UnknownOutput(frame.target.output))?;
        if !state
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.matches(frame))
        {
            return Err(FrameError::StalePreparedFrame {
                output: frame.target.output,
                timeline_value: frame.timeline_value,
            });
        }
        let prepared = state
            .prepared
            .take()
            .expect("prepared-frame match retains the allocation");
        self.descriptors
            .release(prepared.allocation)
            .map_err(|error| frame_allocation_error(error, descriptor_capacity))?;
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

fn add_cursor_damage(
    damage: &mut DamageSet,
    mut previous: &[CursorOverlay],
    mut current: &[CursorOverlay],
    viewport: Rect,
) {
    while let (Some(before), Some(after)) = (previous.first(), current.first()) {
        match before.source.cmp(&after.source) {
            std::cmp::Ordering::Less => {
                damage.add_region(before.clip, viewport);
                previous = &previous[1..];
            }
            std::cmp::Ordering::Greater => {
                damage.add_region(after.clip, viewport);
                current = &current[1..];
            }
            std::cmp::Ordering::Equal => {
                if before != after {
                    damage.add_region(before.clip, viewport);
                    damage.add_region(after.clip, viewport);
                }
                previous = &previous[1..];
                current = &current[1..];
            }
        }
    }
    for overlay in previous.iter().chain(current) {
        damage.add_region(overlay.clip, viewport);
    }
}

fn add_capture_cursor_exclusion_damage(
    damage: &mut DamageSet,
    previous: &[CursorOverlay],
    current: &[CursorOverlay],
    capture_region: Rect,
    viewport: Rect,
) {
    for overlay in previous.iter().chain(current) {
        if let Some(region) = overlay.clip.intersection(capture_region) {
            damage.add_region(region, viewport);
        }
    }
}

#[derive(Debug)]
struct OutputFrameState {
    target: NativeOutputTarget,
    previous_scene: Option<SceneSnapshot>,
    previous_cursors: Vec<CursorOverlay>,
    slot_history: [OutputSlotHistory; OUTPUT_SLOT_COUNT as usize],
    next_serial: u64,
    last_submitted_timeline: u64,
    in_flight: bool,
    next_slot: u8,
    prepared: Option<PreparedFrameState>,
}

const OUTPUT_SLOT_COUNT: u8 = 3;

#[derive(Debug)]
struct PreparedFrameState {
    timeline_value: u64,
    serial: u64,
    output_slot: u8,
    descriptors: HeapAllocation,
    allocation: DescriptorAllocation,
}

impl PreparedFrameState {
    fn matches(&self, frame: &FrameSubmission) -> bool {
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
            previous_cursors: Vec::with_capacity(MAX_CURSOR_OVERLAYS),
            slot_history: std::array::from_fn(|_| OutputSlotHistory::new()),
            next_serial: 1,
            last_submitted_timeline: 0,
            in_flight: false,
            next_slot: 0,
            prepared: None,
        }
    }

    fn clear_slot_history(&mut self) {
        for slot in &mut self.slot_history {
            slot.scene = None;
            slot.cursors.clear();
        }
    }
}

#[derive(Debug)]
struct OutputSlotHistory {
    scene: Option<SceneSnapshot>,
    cursors: Vec<CursorOverlay>,
}

impl OutputSlotHistory {
    fn new() -> Self {
        Self {
            scene: None,
            cursors: Vec::with_capacity(MAX_CURSOR_OVERLAYS),
        }
    }
}

#[cfg(test)]
fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    value.checked_add((alignment - remainder) % alignment)
}

fn frame_allocator_error(error: DescriptorHeapError) -> FrameError {
    match error {
        DescriptorHeapError::InvalidAlignment(alignment) => {
            FrameError::InvalidDescriptorAlignment { alignment }
        }
        DescriptorHeapError::ReservedRangeConsumesHeap {
            heap_size,
            minimum_reserved_range,
            ..
        } => FrameError::DescriptorHeapTooSmall {
            capacity: heap_size,
            reserved: minimum_reserved_range,
        },
        other => FrameError::DescriptorAllocator(other.to_string()),
    }
}

fn frame_allocation_error(error: DescriptorHeapError, capacity: u64) -> FrameError {
    match error {
        DescriptorHeapError::OutOfMemory { requested, .. } => FrameError::DescriptorHeapExhausted {
            requested,
            capacity,
        },
        other => frame_allocator_error(other),
    }
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
    #[error("descriptor heap exhausted: requested {requested} bytes, capacity {capacity}")]
    DescriptorHeapExhausted { requested: u64, capacity: u64 },
    #[error("shared descriptor heap allocator failed: {0}")]
    DescriptorAllocator(String),
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
    #[error("surface {surface:?} has no executable color path: {reason}")]
    UnsupportedSurfaceColor {
        surface: crate::ecs::SurfaceId,
        reason: crate::render::color::SurfaceColorError,
    },
    #[error("native output format {0:?} has no color target descriptor")]
    UnsupportedOutputColorFormat(tensor_host::Fourcc),
    #[error("renderer timeline value space is exhausted")]
    #[cfg(test)]
    TimelineExhausted,
    #[error("renderer frame timeline values must be non-zero")]
    InvalidTimelineValue,
    #[error("renderer frame serial space is exhausted")]
    SerialExhausted,
    #[error("Vulkan device was lost; frame submission is stopped")]
    DeviceLost,
}

#[cfg(test)]
mod tests;
