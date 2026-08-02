use std::ops::{BitOr, BitOrAssign};

use vulkanalia::vk;

/// Pipeline stages at which a queue semaphore wait becomes visible.
///
/// This is a renderer value type rather than a Vulkan bitflag. It covers the
/// synchronization2 stages exposed by renderer command and presentation APIs
/// while keeping binding-specific values inside the shared renderer.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct PipelineStages(u32);

impl PipelineStages {
    pub const DRAW_INDIRECT: Self = Self(1 << 0);
    pub const VERTEX_INPUT: Self = Self(1 << 1);
    pub const VERTEX_SHADER: Self = Self(1 << 2);
    pub const TESSELLATION_CONTROL_SHADER: Self = Self(1 << 3);
    pub const TESSELLATION_EVALUATION_SHADER: Self = Self(1 << 4);
    pub const GEOMETRY_SHADER: Self = Self(1 << 5);
    pub const FRAGMENT_SHADER: Self = Self(1 << 6);
    pub const EARLY_FRAGMENT_TESTS: Self = Self(1 << 7);
    pub const LATE_FRAGMENT_TESTS: Self = Self(1 << 8);
    pub const COLOR_ATTACHMENT_OUTPUT: Self = Self(1 << 9);
    pub const COMPUTE_SHADER: Self = Self(1 << 10);
    pub const TRANSFER: Self = Self(1 << 11);
    pub const HOST: Self = Self(1 << 12);
    pub const ALL_GRAPHICS: Self = Self(1 << 13);
    pub const ALL_COMMANDS: Self = Self(1 << 14);
    pub const VIDEO_DECODE: Self = Self(1 << 15);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub(crate) fn to_vk(self) -> vk::PipelineStageFlags2 {
        let mut stages = vk::PipelineStageFlags2::NONE;
        for (stage, raw) in [
            (Self::DRAW_INDIRECT, vk::PipelineStageFlags2::DRAW_INDIRECT),
            (Self::VERTEX_INPUT, vk::PipelineStageFlags2::VERTEX_INPUT),
            (Self::VERTEX_SHADER, vk::PipelineStageFlags2::VERTEX_SHADER),
            (
                Self::TESSELLATION_CONTROL_SHADER,
                vk::PipelineStageFlags2::TESSELLATION_CONTROL_SHADER,
            ),
            (
                Self::TESSELLATION_EVALUATION_SHADER,
                vk::PipelineStageFlags2::TESSELLATION_EVALUATION_SHADER,
            ),
            (
                Self::GEOMETRY_SHADER,
                vk::PipelineStageFlags2::GEOMETRY_SHADER,
            ),
            (
                Self::FRAGMENT_SHADER,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
            ),
            (
                Self::EARLY_FRAGMENT_TESTS,
                vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
            ),
            (
                Self::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            ),
            (
                Self::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            ),
            (
                Self::COMPUTE_SHADER,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
            ),
            (Self::TRANSFER, vk::PipelineStageFlags2::ALL_TRANSFER),
            (Self::HOST, vk::PipelineStageFlags2::HOST),
            (Self::ALL_GRAPHICS, vk::PipelineStageFlags2::ALL_GRAPHICS),
            (Self::ALL_COMMANDS, vk::PipelineStageFlags2::ALL_COMMANDS),
            (
                Self::VIDEO_DECODE,
                vk::PipelineStageFlags2::VIDEO_DECODE_KHR,
            ),
        ] {
            if self.contains(stage) {
                stages = stages.union(raw);
            }
        }
        stages
    }
}

impl std::fmt::Debug for PipelineStages {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("PipelineStages")
            .field(&self.0)
            .finish()
    }
}

impl BitOr for PipelineStages {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PipelineStages {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
