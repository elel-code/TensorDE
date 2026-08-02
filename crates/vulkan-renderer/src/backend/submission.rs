use std::fmt;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, KhrSwapchainExtensionDeviceCommands},
};

use super::{DeviceOwner, Queue, SemaphoreWait};
use crate::{
    BinarySemaphore, CommandBuffer, Error, FrameToken, PresentStatus, Result, SubmissionLease,
    Swapchain,
};

impl fmt::Debug for Queue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Queue")
            .field("graphics", &self.owner.queues.graphics)
            .field("timeline", &self.owner.timeline)
            .finish_non_exhaustive()
    }
}

impl Queue {
    pub fn raw(&self) -> vk::Queue {
        self.owner.queues.graphics
    }

    pub fn timeline_semaphore(&self) -> vk::Semaphore {
        self.owner.timeline
    }

    /// Consumes executable command buffers and returns the timeline value that
    /// retires all resources referenced by this submission.
    pub fn submit<I>(&self, command_buffers: I) -> Result<FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
    {
        self.submit_with_waits(command_buffers, &[])
    }

    /// Like [`Queue::submit`], with explicit binary/timeline semaphore waits.
    pub fn submit_with_waits<I>(
        &self,
        command_buffers: I,
        waits: &[SemaphoreWait],
    ) -> Result<FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
    {
        self.submit_retained(command_buffers, waits, std::iter::empty())
    }

    /// Submits work while retaining host/resource ownership until the returned
    /// timeline value completes.
    ///
    /// Leases enter the retirement queue only after `vkQueueSubmit2` succeeds.
    /// A validation or submission failure therefore releases them immediately.
    pub fn submit_retained<I, L>(
        &self,
        command_buffers: I,
        waits: &[SemaphoreWait],
        leases: L,
    ) -> Result<FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
        L: IntoIterator<Item = SubmissionLease>,
    {
        let mut leases = leases.into_iter().collect::<Vec<_>>();
        let mut command_buffers = command_buffers.into_iter().collect::<Vec<_>>();
        if command_buffers
            .iter()
            .any(|command_buffer| !command_buffer.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "command buffer was created by a different Device".into(),
            ));
        }
        let handles = command_buffers
            .iter()
            .map(CommandBuffer::raw)
            .collect::<Vec<_>>();
        let frame = submit_new_to_graphics_queue(&self.owner, &handles, waits, &[])?;
        for command_buffer in &mut command_buffers {
            leases.extend(command_buffer.take_for_submission());
        }
        self.owner.retire_command_buffers_after(frame, handles);
        self.submission_retirement.retire_after(frame, leases);
        Ok(frame)
    }

    /// Submits work and signals binary semaphores for a following present.
    ///
    /// # Safety
    ///
    /// Wait semaphores must satisfy [`Queue::submit_raw`] and every signal
    /// semaphore must be unsignalled with no pending signal operation. All
    /// synchronization objects must remain live until the submission completes.
    pub unsafe fn submit_with_binary_signals<I>(
        &self,
        command_buffers: I,
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
    ) -> Result<FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
    {
        unsafe {
            self.submit_retained_with_binary_signals(
                command_buffers,
                waits,
                signals,
                std::iter::empty(),
            )
        }
    }

    /// Submits managed command buffers at a caller-reserved timeline value and
    /// signals binary semaphores for an external consumer.
    ///
    /// This is for frame schedulers that must reserve a timeline value before
    /// descriptor allocation, while still delegating command-buffer lifetime
    /// and resource retirement to the shared renderer.
    ///
    /// # Safety
    ///
    /// `frame` must be a fresh value from this backend. Wait semaphores must
    /// satisfy [`Queue::submit_raw`], and every signal semaphore must be
    /// unsignalled with no pending signal operation.
    pub unsafe fn submit_with_binary_signals_at<I>(
        &self,
        frame: FrameToken,
        command_buffers: I,
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
    ) -> Result<()>
    where
        I: IntoIterator<Item = CommandBuffer>,
    {
        if signals
            .iter()
            .any(|semaphore| !semaphore.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "binary signal semaphore was created by a different Device".into(),
            ));
        }
        let mut command_buffers = command_buffers.into_iter().collect::<Vec<_>>();
        if command_buffers
            .iter()
            .any(|command_buffer| !command_buffer.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "command buffer was created by a different Device".into(),
            ));
        }
        let handles = command_buffers
            .iter()
            .map(CommandBuffer::raw)
            .collect::<Vec<_>>();
        let signals = signals
            .iter()
            .map(|semaphore| semaphore.raw())
            .collect::<Vec<_>>();
        {
            let _submit_guard = self
                .owner
                .submit_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            submit_to_graphics_queue_locked(&self.owner, frame, &handles, waits, &signals)?;
        }
        let mut leases = Vec::new();
        for command_buffer in &mut command_buffers {
            leases.extend(command_buffer.take_for_submission());
        }
        self.owner.retire_command_buffers_after(frame, handles);
        self.submission_retirement.retire_after(frame, leases);
        Ok(())
    }

    /// Submits work, signals binary semaphores, and retains leases until the
    /// returned timeline value completes.
    ///
    /// # Safety
    ///
    /// Wait semaphores must satisfy [`Queue::submit_raw`] and every signal
    /// semaphore must be unsignalled with no pending signal operation. All
    /// synchronization objects must remain live until the submission completes.
    pub unsafe fn submit_retained_with_binary_signals<I, L>(
        &self,
        command_buffers: I,
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
        leases: L,
    ) -> Result<FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
        L: IntoIterator<Item = SubmissionLease>,
    {
        let mut leases = leases.into_iter().collect::<Vec<_>>();
        if signals
            .iter()
            .any(|semaphore| !semaphore.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "binary signal semaphore was created by a different Device".into(),
            ));
        }
        let mut command_buffers = command_buffers.into_iter().collect::<Vec<_>>();
        if command_buffers
            .iter()
            .any(|command_buffer| !command_buffer.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "command buffer was created by a different Device".into(),
            ));
        }
        let handles = command_buffers
            .iter()
            .map(CommandBuffer::raw)
            .collect::<Vec<_>>();
        let signals = signals
            .iter()
            .map(|semaphore| semaphore.raw())
            .collect::<Vec<_>>();
        let frame = submit_new_to_graphics_queue(&self.owner, &handles, waits, &signals)?;
        for command_buffer in &mut command_buffers {
            leases.extend(command_buffer.take_for_submission());
        }
        self.owner.retire_command_buffers_after(frame, handles);
        self.submission_retirement.retire_after(frame, leases);
        Ok(frame)
    }

    /// Submits externally owned Vulkan command buffers.
    ///
    /// # Safety
    ///
    /// Every command buffer must be executable, belong to this device and its
    /// graphics command-pool family, and remain alive until `frame` completes.
    pub unsafe fn submit_raw(
        &self,
        frame: FrameToken,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
    ) -> Result<()> {
        submit_to_graphics_queue(&self.owner, frame, command_buffers, waits)
    }

    /// Submits externally recorded command buffers on a caller-reserved frame
    /// token and signals binary semaphores for an external consumer.
    ///
    /// This is the native-stream counterpart of
    /// [`Self::submit_with_binary_signals`].  It is for integrations whose
    /// command recording has not yet been expressed through
    /// [`crate::CommandEncoder`], while keeping timeline submission and binary
    /// semaphore ownership in the shared renderer.
    ///
    /// # Safety
    ///
    /// Every command buffer must satisfy [`Self::submit_raw`]. Every signal
    /// semaphore must be unsignalled with no pending signal operation, and
    /// every synchronization object must remain live until submission
    /// completes.
    pub unsafe fn submit_raw_with_binary_signals(
        &self,
        frame: FrameToken,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
        signals: &[&BinarySemaphore],
    ) -> Result<()> {
        if signals
            .iter()
            .any(|semaphore| !semaphore.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "binary signal semaphore was created by a different Device".into(),
            ));
        }
        let signals = signals
            .iter()
            .map(|semaphore| semaphore.raw())
            .collect::<Vec<_>>();
        let _submit_guard = self
            .owner
            .submit_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        submit_to_graphics_queue_locked(&self.owner, frame, command_buffers, waits, &signals)
    }

    /// Allocates a monotonic timeline value and submits externally owned
    /// Vulkan command buffers in the same host-serialized transaction.
    ///
    /// # Safety
    ///
    /// The requirements of [`Queue::submit_raw`] apply. Prefer
    /// [`Queue::submit`] for automatic lifetime tracking.
    pub unsafe fn submit_raw_command_buffers(
        &self,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
    ) -> Result<FrameToken> {
        submit_new_to_graphics_queue(&self.owner, command_buffers, waits, &[])
    }

    pub fn completed_timeline(&self) -> Result<u64> {
        let completed = unsafe {
            self.owner
                .device
                .get_semaphore_counter_value(self.owner.timeline)
        }
        .map_err(|source| Error::vulkan("vkGetSemaphoreCounterValue", source))?;
        self.owner.retire_timeline(completed);
        self.owner.retire_completed_command_buffers(completed);
        self.submission_retirement.retire_completed(completed);
        Ok(completed)
    }

    pub fn wait_for(&self, frame: FrameToken, timeout_ns: u64) -> Result<()> {
        let semaphores = [self.owner.timeline];
        let values = [frame.value()];
        let wait = vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphores)
            .values(&values);
        unsafe { self.owner.device.wait_semaphores(&wait, timeout_ns) }
            .map_err(|source| Error::vulkan("vkWaitSemaphores", source))?;
        self.owner.retire_timeline(frame.value());
        self.owner.retire_completed_command_buffers(frame.value());
        self.submission_retirement.retire_completed(frame.value());
        Ok(())
    }

    /// Waits until all work submitted to this logical device is idle, then
    /// retires every completed managed command buffer and submission lease.
    ///
    /// This is intended for infrequent lifetime boundaries such as swapchain
    /// replacement or final shutdown, not steady-state frame pacing.
    pub fn wait_idle(&self) -> Result<()> {
        {
            let _submit_guard = self
                .owner
                .submit_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            unsafe { self.owner.device.device_wait_idle() }
                .map_err(|source| Error::vulkan("vkDeviceWaitIdle", source))?;
        }
        self.completed_timeline().map(|_| ())
    }

    /// Number of leases retained by submissions that have not yet been
    /// observed complete through this queue.
    pub fn pending_submission_leases(&self) -> usize {
        self.submission_retirement.pending_count()
    }

    /// Presents one acquired swapchain image on the graphics queue.
    ///
    /// Queue submission and presentation share one host lock, preserving Vulkan
    /// queue external synchronization across concurrent callers.
    ///
    /// # Safety
    ///
    /// `image_index` must currently be acquired and not previously presented.
    /// Every wait semaphore must be a live binary semaphore belonging to this
    /// device and scheduled to signal before presentation consumes it.
    pub(crate) unsafe fn present(
        &self,
        swapchain: &Swapchain,
        image_index: u32,
        wait_semaphores: &[&BinarySemaphore],
    ) -> Result<PresentStatus> {
        if !swapchain.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "swapchain was created by a different Device".into(),
            ));
        }
        if !swapchain.contains_index(image_index) {
            return Err(Error::Validation(
                "present image index is outside the swapchain".into(),
            ));
        }
        if wait_semaphores
            .iter()
            .any(|semaphore| !semaphore.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "present wait semaphore was created by a different Device".into(),
            ));
        }
        let wait_semaphores = wait_semaphores
            .iter()
            .map(|semaphore| semaphore.raw())
            .collect::<Vec<_>>();
        unsafe { self.present_raw(swapchain, image_index, &wait_semaphores) }
    }

    /// Presents with externally owned raw Vulkan semaphores.
    ///
    /// # Safety
    ///
    /// The swapchain image must be acquired, and every semaphore must be a live
    /// binary semaphore scheduled to signal before this wait consumes it.
    pub unsafe fn present_raw(
        &self,
        swapchain: &Swapchain,
        image_index: u32,
        wait_semaphores: &[vk::Semaphore],
    ) -> Result<PresentStatus> {
        if !swapchain.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "swapchain was created by a different Device".into(),
            ));
        }
        if !swapchain.contains_index(image_index) {
            return Err(Error::Validation(
                "present image index is outside the swapchain".into(),
            ));
        }
        let swapchains = [swapchain.raw()];
        let indices = [image_index];
        let present = vk::PresentInfoKHR::builder()
            .wait_semaphores(wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&indices);
        let _submit_guard = self
            .owner
            .submit_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let success = unsafe {
            self.owner
                .device
                .queue_present_khr(self.owner.queues.graphics, &present)
        }
        .map_err(|source| Error::vulkan("vkQueuePresentKHR", source))?;
        Ok(if success == vk::SuccessCode::SUBOPTIMAL_KHR {
            PresentStatus::Suboptimal
        } else {
            PresentStatus::Optimal
        })
    }
}

pub(super) fn submit_to_graphics_queue(
    owner: &DeviceOwner,
    frame: FrameToken,
    command_buffers: &[vk::CommandBuffer],
    waits: &[SemaphoreWait],
) -> Result<()> {
    let _submit_guard = owner
        .submit_lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    submit_to_graphics_queue_locked(owner, frame, command_buffers, waits, &[])
}

fn submit_new_to_graphics_queue(
    owner: &DeviceOwner,
    command_buffers: &[vk::CommandBuffer],
    waits: &[SemaphoreWait],
    binary_signals: &[vk::Semaphore],
) -> Result<FrameToken> {
    let _submit_guard = owner
        .submit_lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let frame = owner.allocate_frame()?;
    submit_to_graphics_queue_locked(owner, frame, command_buffers, waits, binary_signals)?;
    Ok(frame)
}

/// Records one queue transaction while `DeviceOwner::submit_lock` is held.
/// Timeline allocation for standard submissions is performed under the same
/// lock so host threads cannot submit value N+1 before value N.
fn submit_to_graphics_queue_locked(
    owner: &DeviceOwner,
    frame: FrameToken,
    command_buffers: &[vk::CommandBuffer],
    waits: &[SemaphoreWait],
    binary_signals: &[vk::Semaphore],
) -> Result<()> {
    let wait_infos = waits
        .iter()
        .map(|wait| {
            vk::SemaphoreSubmitInfo::builder()
                .semaphore(wait.semaphore)
                .value(wait.value)
                .stage_mask(wait.stages)
                .build()
        })
        .collect::<Vec<_>>();
    let command_infos = command_buffers
        .iter()
        .copied()
        .map(|command_buffer| {
            vk::CommandBufferSubmitInfo::builder()
                .command_buffer(command_buffer)
                .build()
        })
        .collect::<Vec<_>>();
    let mut signals = binary_signals
        .iter()
        .copied()
        .map(|semaphore| {
            vk::SemaphoreSubmitInfo::builder()
                .semaphore(semaphore)
                .value(0)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .build()
        })
        .collect::<Vec<_>>();
    signals.push(
        vk::SemaphoreSubmitInfo::builder()
            .semaphore(owner.timeline)
            .value(frame.value())
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build(),
    );
    let submissions = [vk::SubmitInfo2::builder()
        .wait_semaphore_infos(&wait_infos)
        .command_buffer_infos(&command_infos)
        .signal_semaphore_infos(&signals)
        .build()];
    unsafe {
        owner
            .device
            .queue_submit2(owner.queues.graphics, &submissions, vk::Fence::null())
    }
    .map_err(|source| Error::vulkan("vkQueueSubmit2", source))
}
