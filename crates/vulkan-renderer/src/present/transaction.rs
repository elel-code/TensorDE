//! Typed acquire/submit/present transactions for direct and offscreen frames.

use std::fmt;
use std::sync::Arc;

use vulkanalia::vk;

use super::{
    AcquiredSurfaceTexture, PresentStatus, PresentationPathPlan, PresentationTarget,
    SurfaceAcquireStrategy, Swapchain,
};
use crate::backend::{DeviceOwner, Queue};
use crate::{
    Backend, BinarySemaphore, BinarySemaphoreDescriptor, CommandBuffer, CommandEncoder,
    CommandEncoderDescriptor, Error, FrameToken, Result,
};
#[cfg(feature = "ffmpeg-vulkan-decode")]
use crate::{DecodedVideoFrame, SubmissionLease, video::decoded_video_submission_parts};

/// Cold-compiled command ordering for one presentation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationTransactionStep {
    SubmitOffscreen,
    AcquireSurface,
    RecordDirectSurface,
    RecordTerminalSurface,
    SubmitDirectSurface,
    SubmitOffscreenAndTerminal,
    SubmitTerminal,
    Present,
}

/// Stable schedule used both for diagnostics and runtime dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationTransactionSchedule {
    steps: Vec<PresentationTransactionStep>,
}

impl PresentationTransactionSchedule {
    pub fn compile(plan: &PresentationPathPlan) -> Self {
        use PresentationTransactionStep as Step;

        let steps = match (plan.target, plan.acquire) {
            (PresentationTarget::DirectSurface, SurfaceAcquireStrategy::BeforeFrame) => vec![
                Step::AcquireSurface,
                Step::RecordDirectSurface,
                Step::SubmitDirectSurface,
                Step::Present,
            ],
            (PresentationTarget::Offscreen, SurfaceAcquireStrategy::BeforeFrame) => vec![
                Step::AcquireSurface,
                Step::RecordTerminalSurface,
                Step::SubmitOffscreenAndTerminal,
                Step::Present,
            ],
            (PresentationTarget::Offscreen, SurfaceAcquireStrategy::AfterOffscreenSubmit) => vec![
                Step::SubmitOffscreen,
                Step::AcquireSurface,
                Step::RecordTerminalSurface,
                Step::SubmitTerminal,
                Step::Present,
            ],
            (PresentationTarget::DirectSurface, SurfaceAcquireStrategy::AfterOffscreenSubmit) => {
                unreachable!("PresentationPathPlan rejects late-acquire direct rendering")
            }
        };
        Self { steps }
    }

    pub fn steps(&self) -> &[PresentationTransactionStep] {
        &self.steps
    }
}

#[derive(Clone, Debug)]
pub struct PresentationTransactionDescriptor<'a> {
    pub label: Option<String>,
    pub plan: &'a PresentationPathPlan,
    pub swapchain: &'a Swapchain,
    pub acquire_timeout_ns: u64,
}

/// The command range which consumes an external frame dependency.
#[cfg(feature = "ffmpeg-vulkan-decode")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationDependencyScope {
    IndependentCommands,
    SurfaceCommands,
}

/// Typed external dependencies consumed by exactly one presentation submit.
#[cfg(feature = "ffmpeg-vulkan-decode")]
#[derive(Clone, Copy, Debug)]
pub struct PresentationFrameDependencies<'a> {
    decoded_video_frames: &'a [DecodedVideoFrame],
    scope: PresentationDependencyScope,
}

#[cfg(feature = "ffmpeg-vulkan-decode")]
impl<'a> PresentationFrameDependencies<'a> {
    pub const NONE: Self = Self {
        decoded_video_frames: &[],
        scope: PresentationDependencyScope::IndependentCommands,
    };

    pub const fn decoded_video(
        frames: &'a [DecodedVideoFrame],
        scope: PresentationDependencyScope,
    ) -> Self {
        Self {
            decoded_video_frames: frames,
            scope,
        }
    }
}

/// Last reached state of the managed transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationTransactionPhase {
    Ready,
    OffscreenSubmitted,
    SurfaceAcquired,
    SurfaceRecorded,
    SurfaceSubmitted,
    Presented,
    Poisoned,
}

/// Exact submission and WSI result of one completed frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentTransactionOutcome {
    pub image_index: u32,
    pub acquire_status: PresentStatus,
    pub present_status: PresentStatus,
    /// Present only for policy-driven late acquire. The surface submission is
    /// later on the same graphics queue and therefore retires this work too.
    pub offscreen_submission: Option<FrameToken>,
    pub surface_submission: FrameToken,
}

#[derive(Debug)]
struct FrameSlotSync {
    acquire: BinarySemaphore,
    last_submission: Option<FrameToken>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SurfaceImageState {
    Undefined,
    Presented,
}

impl SurfaceImageState {
    const fn layout(self) -> vk::ImageLayout {
        match self {
            Self::Undefined => vk::ImageLayout::UNDEFINED,
            Self::Presented => vk::ImageLayout::PRESENT_SRC_KHR,
        }
    }
}

/// Retained WSI synchronization and strict ordering for one swapchain.
///
/// Acquire semaphores are indexed by in-flight frame slot and cannot be reused
/// before that slot's surface submission retires. Render-finished semaphores
/// are indexed by swapchain image: reacquiring that image proves the previous
/// presentation wait has consumed its semaphore signal.
pub struct PresentationTransaction {
    owner: Arc<DeviceOwner>,
    queue: Queue,
    swapchain: vk::SwapchainKHR,
    plan: PresentationPathPlan,
    schedule: PresentationTransactionSchedule,
    acquire_timeout_ns: u64,
    frame_slots: Vec<FrameSlotSync>,
    present_signals: Vec<BinarySemaphore>,
    image_states: Vec<SurfaceImageState>,
    label: Option<String>,
    phase: PresentationTransactionPhase,
}

impl fmt::Debug for PresentationTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationTransaction")
            .field("swapchain", &self.swapchain)
            .field("plan", &self.plan)
            .field("schedule", &self.schedule)
            .field("frame_slots", &self.frame_slots.len())
            .field("surface_images", &self.image_states.len())
            .field("label", &self.label)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl Backend {
    pub fn create_presentation_transaction(
        &self,
        descriptor: &PresentationTransactionDescriptor<'_>,
    ) -> Result<PresentationTransaction> {
        validate_descriptor(self, descriptor)?;
        let mut frame_slots = Vec::with_capacity(descriptor.plan.frame_slots as usize);
        for slot in 0..descriptor.plan.frame_slots {
            frame_slots.push(FrameSlotSync {
                acquire: self.create_binary_semaphore(&BinarySemaphoreDescriptor {
                    label: transaction_label(descriptor.label.as_deref(), "acquire", slot),
                })?,
                last_submission: None,
            });
        }
        let mut present_signals = Vec::with_capacity(descriptor.swapchain.image_count());
        for image in 0..descriptor.swapchain.image_count() {
            present_signals.push(self.create_binary_semaphore(&BinarySemaphoreDescriptor {
                label: transaction_label(descriptor.label.as_deref(), "present", image as u32),
            })?);
        }
        Ok(PresentationTransaction {
            owner: self.shared_owner(),
            queue: self.queue(),
            swapchain: descriptor.swapchain.raw(),
            plan: descriptor.plan.clone(),
            schedule: PresentationTransactionSchedule::compile(descriptor.plan),
            acquire_timeout_ns: descriptor.acquire_timeout_ns,
            image_states: vec![SurfaceImageState::Undefined; descriptor.swapchain.image_count()],
            frame_slots,
            present_signals,
            label: descriptor.label.clone(),
            phase: PresentationTransactionPhase::Ready,
        })
    }
}

impl PresentationTransaction {
    pub const fn plan(&self) -> &PresentationPathPlan {
        &self.plan
    }

    pub const fn schedule(&self) -> &PresentationTransactionSchedule {
        &self.schedule
    }

    pub const fn phase(&self) -> PresentationTransactionPhase {
        self.phase
    }

    /// Executes a complete frame transaction and consumes every command
    /// buffer exactly once.
    ///
    /// `record_independent` runs only after the selected frame slot's previous
    /// timeline submission retires. For an offscreen plan it records the
    /// authored/effect graph ending at SceneColor. For a direct plan it must
    /// return no commands because `record_surface` records the sole physical
    /// pass. The surface callback records only commands between the managed transition to
    /// `ATTACHMENT_OPTIMAL` and the transition to `PRESENT_SRC_KHR`.
    ///
    /// If recording, submission, or presentation fails after acquisition, the
    /// transaction becomes poisoned. Recreate it together with the swapchain;
    /// silently continuing could reuse an acquired image or binary semaphore.
    pub fn execute_frame<I, R, F>(
        &mut self,
        swapchain: &Swapchain,
        frame_slot: usize,
        record_independent: R,
        #[cfg(feature = "ffmpeg-vulkan-decode")] dependencies: PresentationFrameDependencies<'_>,
        record_surface: F,
    ) -> Result<PresentTransactionOutcome>
    where
        I: IntoIterator<Item = CommandBuffer>,
        R: FnOnce() -> Result<I>,
        F: FnOnce(&mut CommandEncoder, &AcquiredSurfaceTexture<'_>) -> Result<()>,
    {
        self.validate_frame_start(swapchain, frame_slot)?;
        let mut independent_commands = record_independent()?.into_iter().collect::<Vec<_>>();
        self.validate_independent_commands(&independent_commands)?;
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        self.validate_dependencies(dependencies)?;

        let late_acquire = self.plan.acquire == SurfaceAcquireStrategy::AfterOffscreenSubmit;
        let offscreen_submission = if late_acquire {
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            let (waits, leases) = self.decoded_submission_parts(
                dependencies,
                PresentationDependencyScope::IndependentCommands,
            )?;
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            let frame = self.queue.submit_retained(
                std::mem::take(&mut independent_commands),
                &waits,
                leases,
            )?;
            #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
            let frame = self
                .queue
                .submit(std::mem::take(&mut independent_commands))?;
            self.frame_slots[frame_slot].last_submission = Some(frame);
            self.phase = PresentationTransactionPhase::OffscreenSubmitted;
            Some(frame)
        } else {
            None
        };

        let acquired = unsafe {
            swapchain.acquire_next_image(
                self.acquire_timeout_ns,
                &self.frame_slots[frame_slot].acquire,
            )
        }?;
        self.phase = PresentationTransactionPhase::SurfaceAcquired;
        let image_index = acquired.index();
        let acquire_status = acquired.status();
        let old_layout = self.image_states[image_index as usize].layout();

        let mut surface_encoder = match CommandEncoder::new(
            Arc::clone(&self.owner),
            &CommandEncoderDescriptor {
                label: self
                    .label
                    .as_deref()
                    .map(|label| format!("{label}-surface-frame-{frame_slot}")),
            },
        ) {
            Ok(encoder) => encoder,
            Err(error) => return self.poison(error),
        };
        unsafe {
            surface_encoder.transition_color_image(
                acquired.image(),
                old_layout,
                vk::ImageLayout::ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            );
        }
        if let Err(error) = record_surface(&mut surface_encoder, &acquired) {
            return self.poison(error);
        }
        unsafe {
            surface_encoder.transition_color_image(
                acquired.image(),
                vk::ImageLayout::ATTACHMENT_OPTIMAL,
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
            );
        }
        let surface_commands = match surface_encoder.finish() {
            Ok(commands) => commands,
            Err(error) => return self.poison(error),
        };
        self.phase = PresentationTransactionPhase::SurfaceRecorded;

        let acquire_wait = self.frame_slots[frame_slot]
            .acquire
            .wait(crate::PipelineStages::COLOR_ATTACHMENT_OUTPUT)?;
        let present_signal = &self.present_signals[image_index as usize];
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        let submission_scope = if late_acquire {
            PresentationDependencyScope::SurfaceCommands
        } else {
            dependencies.scope
        };
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        let (decoded_waits, decoded_leases) =
            self.decoded_submission_parts(dependencies, submission_scope)?;
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        let mut submission_waits = Vec::with_capacity(decoded_waits.len().saturating_add(1));
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        {
            submission_waits.push(acquire_wait);
            submission_waits.extend(decoded_waits);
        }
        let surface_submission = if late_acquire {
            unsafe {
                #[cfg(feature = "ffmpeg-vulkan-decode")]
                {
                    self.queue.submit_retained_with_binary_signals(
                        [surface_commands],
                        &submission_waits,
                        &[present_signal],
                        decoded_leases,
                    )
                }
                #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
                self.queue.submit_with_binary_signals(
                    [surface_commands],
                    &[acquire_wait],
                    &[present_signal],
                )
            }
        } else {
            independent_commands.push(surface_commands);
            unsafe {
                #[cfg(feature = "ffmpeg-vulkan-decode")]
                {
                    self.queue.submit_retained_with_binary_signals(
                        independent_commands,
                        &submission_waits,
                        &[present_signal],
                        decoded_leases,
                    )
                }
                #[cfg(not(feature = "ffmpeg-vulkan-decode"))]
                self.queue.submit_with_binary_signals(
                    independent_commands,
                    &[acquire_wait],
                    &[present_signal],
                )
            }
        };
        let surface_submission = match surface_submission {
            Ok(frame) => frame,
            Err(error) => return self.poison(error),
        };
        self.frame_slots[frame_slot].last_submission = Some(surface_submission);
        self.phase = PresentationTransactionPhase::SurfaceSubmitted;

        let present_status = match unsafe { acquired.present(&self.queue, &[present_signal]) } {
            Ok(status) => status,
            Err(error) => return self.poison(error),
        };
        self.image_states[image_index as usize] = SurfaceImageState::Presented;
        self.phase = PresentationTransactionPhase::Presented;
        Ok(PresentTransactionOutcome {
            image_index,
            acquire_status,
            present_status,
            offscreen_submission,
            surface_submission,
        })
    }

    fn validate_frame_start(&mut self, swapchain: &Swapchain, frame_slot: usize) -> Result<()> {
        if self.phase == PresentationTransactionPhase::Poisoned {
            return Err(Error::Validation(
                "presentation transaction is poisoned; recreate it with the swapchain".into(),
            ));
        }
        if !swapchain.belongs_to(&self.owner) || swapchain.raw() != self.swapchain {
            return Err(Error::Validation(
                "presentation transaction belongs to a different swapchain".into(),
            ));
        }
        let slot = self.frame_slots.get(frame_slot).ok_or_else(|| {
            Error::Validation(format!("presentation frame slot {frame_slot} is missing"))
        })?;
        if let Some(last) = slot.last_submission {
            let completed = self.queue.completed_timeline()?;
            if completed < last.value() {
                self.queue.wait_for(last, u64::MAX)?;
            }
        }
        self.phase = PresentationTransactionPhase::Ready;
        Ok(())
    }

    fn validate_independent_commands(&self, commands: &[CommandBuffer]) -> Result<()> {
        if commands
            .iter()
            .any(|commands| !commands.belongs_to(&self.owner))
        {
            return Err(Error::Validation(
                "presentation command buffer was created by a different Device".into(),
            ));
        }
        match self.plan.target {
            PresentationTarget::DirectSurface if !commands.is_empty() => Err(Error::Validation(
                "direct-surface presentation records its only physical pass after acquire".into(),
            )),
            PresentationTarget::Offscreen if commands.is_empty() => Err(Error::Validation(
                "offscreen presentation requires swapchain-independent command buffers".into(),
            )),
            _ => Ok(()),
        }
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    fn validate_dependencies(&self, dependencies: PresentationFrameDependencies<'_>) -> Result<()> {
        if !dependencies.decoded_video_frames.is_empty()
            && self.plan.target == PresentationTarget::DirectSurface
            && dependencies.scope == PresentationDependencyScope::IndependentCommands
        {
            return Err(Error::Validation(
                "direct-surface presentation cannot consume decoded video in independent commands"
                    .into(),
            ));
        }
        Ok(())
    }

    #[cfg(feature = "ffmpeg-vulkan-decode")]
    fn decoded_submission_parts(
        &self,
        dependencies: PresentationFrameDependencies<'_>,
        scope: PresentationDependencyScope,
    ) -> Result<(Vec<crate::SemaphoreWait>, Vec<SubmissionLease>)> {
        if dependencies.scope != scope {
            return Ok((Vec::new(), Vec::new()));
        }
        decoded_video_submission_parts(&self.owner, dependencies.decoded_video_frames)
    }

    fn poison<T>(&mut self, error: Error) -> Result<T> {
        self.phase = PresentationTransactionPhase::Poisoned;
        Err(error)
    }
}

impl Drop for PresentationTransaction {
    fn drop(&mut self) {
        // Timeline completion alone does not prove that the presentation
        // engine consumed the last render-finished binary semaphore. Teardown
        // is deliberately cold and waits before those semaphores are destroyed.
        let _ = self.queue.wait_idle();
    }
}

fn validate_descriptor(
    backend: &Backend,
    descriptor: &PresentationTransactionDescriptor<'_>,
) -> Result<()> {
    if !descriptor.swapchain.belongs_to(&backend.shared_owner()) {
        return Err(Error::Validation(
            "presentation transaction swapchain was created by a different Device".into(),
        ));
    }
    let configuration = descriptor.swapchain.configuration();
    if configuration.extent != descriptor.plan.surface_extent
        || configuration.format != descriptor.plan.surface_format
    {
        return Err(Error::Validation(
            "presentation plan surface format or extent does not match the swapchain".into(),
        ));
    }
    if !configuration
        .usage
        .contains(crate::TextureUsages::COLOR_ATTACHMENT)
    {
        return Err(Error::Validation(
            "terminal presentation requires COLOR_ATTACHMENT swapchain usage".into(),
        ));
    }
    Ok(())
}

fn transaction_label(base: Option<&str>, role: &str, index: u32) -> Option<String> {
    base.map(|base| format!("{base}-{role}-{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FrameTargetPreference, PresentationPathDescriptor, PresentationRequirements,
        TerminalAlphaMode, TerminalCompositeDescriptor, TerminalSampling,
    };

    fn requirements() -> PresentationRequirements {
        PresentationRequirements {
            surface_extent: crate::Extent2D::new(3840, 2160),
            target_extent: crate::Extent2D::new(3840, 2160),
            surface_format: crate::TextureFormat::Bgra8Unorm,
            target_format: crate::TextureFormat::Rgba16Float,
            frame_slots: 2,
            physical_pass_count: 4,
            sampled_after_write: true,
            has_history: true,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: true,
        }
    }

    #[test]
    fn late_acquire_schedule_submits_offscreen_before_acquire() {
        let plan = PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::Offscreen,
                acquire: SurfaceAcquireStrategy::AfterOffscreenSubmit,
                terminal: TerminalCompositeDescriptor {
                    sampling: TerminalSampling::Linear,
                    alpha: TerminalAlphaMode::Opaque,
                },
            },
            requirements(),
        )
        .unwrap();
        assert_eq!(
            PresentationTransactionSchedule::compile(&plan).steps(),
            &[
                PresentationTransactionStep::SubmitOffscreen,
                PresentationTransactionStep::AcquireSurface,
                PresentationTransactionStep::RecordTerminalSurface,
                PresentationTransactionStep::SubmitTerminal,
                PresentationTransactionStep::Present,
            ]
        );
    }

    #[test]
    fn before_frame_schedule_combines_offscreen_and_terminal_submission() {
        let plan = PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::Offscreen,
                acquire: SurfaceAcquireStrategy::BeforeFrame,
                terminal: TerminalCompositeDescriptor::default(),
            },
            requirements(),
        )
        .unwrap();
        assert_eq!(
            PresentationTransactionSchedule::compile(&plan).steps(),
            &[
                PresentationTransactionStep::AcquireSurface,
                PresentationTransactionStep::RecordTerminalSurface,
                PresentationTransactionStep::SubmitOffscreenAndTerminal,
                PresentationTransactionStep::Present,
            ]
        );
    }

    #[test]
    fn direct_schedule_has_no_offscreen_or_terminal_composite_step() {
        let mut requirements = requirements();
        requirements.target_format = requirements.surface_format;
        requirements.physical_pass_count = 1;
        requirements.sampled_after_write = false;
        requirements.has_history = false;
        requirements.requires_terminal_transform = false;
        let plan = PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::DirectSurface,
                acquire: SurfaceAcquireStrategy::BeforeFrame,
                terminal: TerminalCompositeDescriptor::default(),
            },
            requirements,
        )
        .unwrap();
        assert_eq!(
            PresentationTransactionSchedule::compile(&plan).steps(),
            &[
                PresentationTransactionStep::AcquireSurface,
                PresentationTransactionStep::RecordDirectSurface,
                PresentationTransactionStep::SubmitDirectSurface,
                PresentationTransactionStep::Present,
            ]
        );
    }

    #[test]
    fn explicitly_selected_automatic_policy_resolves_eligible_single_pass_to_direct() {
        let mut requirements = requirements();
        requirements.target_format = requirements.surface_format;
        requirements.physical_pass_count = 1;
        requirements.sampled_after_write = false;
        requirements.has_history = false;
        requirements.requires_terminal_transform = false;
        let plan = PresentationPathPlan::compile(
            PresentationPathDescriptor {
                target: FrameTargetPreference::Automatic,
                acquire: SurfaceAcquireStrategy::BeforeFrame,
                terminal: TerminalCompositeDescriptor::default(),
            },
            requirements,
        )
        .unwrap();
        assert_eq!(plan.target, PresentationTarget::DirectSurface);
        assert_eq!(
            PresentationTransactionSchedule::compile(&plan).steps(),
            &[
                PresentationTransactionStep::AcquireSurface,
                PresentationTransactionStep::RecordDirectSurface,
                PresentationTransactionStep::SubmitDirectSurface,
                PresentationTransactionStep::Present,
            ]
        );
    }

    #[test]
    fn swapchain_images_start_undefined_and_become_present_owned() {
        assert_eq!(
            SurfaceImageState::Undefined.layout(),
            vk::ImageLayout::UNDEFINED
        );
        assert_eq!(
            SurfaceImageState::Presented.layout(),
            vk::ImageLayout::PRESENT_SRC_KHR
        );
    }
}
