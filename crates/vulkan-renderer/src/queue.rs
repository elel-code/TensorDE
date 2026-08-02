use std::collections::BTreeSet;

use vulkanalia::vk;

use crate::video::{VideoDecodeOperations, VideoDecodeRequirements};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueFamilyInfo {
    pub index: u32,
    pub queue_count: u32,
    pub flags: vk::QueueFlags,
    pub video_decode_operations: VideoDecodeOperations,
}

impl QueueFamilyInfo {
    pub const fn supports_graphics(self) -> bool {
        self.queue_count > 0 && self.flags.contains(vk::QueueFlags::GRAPHICS)
    }

    pub const fn supports_compute(self) -> bool {
        self.queue_count > 0 && self.flags.contains(vk::QueueFlags::COMPUTE)
    }

    pub const fn supports_transfer(self) -> bool {
        self.queue_count > 0 && self.flags.contains(vk::QueueFlags::TRANSFER)
    }

    pub const fn supports_video_decode(self, required: VideoDecodeOperations) -> bool {
        self.queue_count > 0
            && self.flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR)
            && self.video_decode_operations.contains(required)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuePlan {
    pub graphics: u32,
    pub compute: u32,
    pub transfer: u32,
    pub video_decode: Option<u32>,
}

impl QueuePlan {
    pub fn select(families: &[QueueFamilyInfo]) -> Option<Self> {
        let graphics = families
            .iter()
            .copied()
            .find(|family| family.supports_graphics())?;
        let compute = families
            .iter()
            .copied()
            .find(|family| family.supports_compute() && !family.supports_graphics())
            .or_else(|| {
                families
                    .iter()
                    .copied()
                    .find(|family| family.supports_compute())
            })
            .unwrap_or(graphics);
        let transfer = families
            .iter()
            .copied()
            .find(|family| {
                family.supports_transfer()
                    && !family.supports_graphics()
                    && !family.supports_compute()
            })
            .or_else(|| {
                families
                    .iter()
                    .copied()
                    .find(|family| family.supports_transfer() && family.index != graphics.index)
            })
            .unwrap_or(graphics);
        Some(Self {
            graphics: graphics.index,
            compute: compute.index,
            transfer: transfer.index,
            video_decode: None,
        })
    }

    pub(crate) fn require_video_decode(
        mut self,
        families: &[QueueFamilyInfo],
        requirements: VideoDecodeRequirements,
    ) -> Option<Self> {
        let required = VideoDecodeOperations::from_codecs(requirements.codecs());
        self.video_decode = families
            .iter()
            .copied()
            .find(|family| family.supports_video_decode(required))
            .map(|family| family.index);
        self.video_decode.map(|_| self)
    }

    pub fn unique_families(self) -> Vec<u32> {
        [
            Some(self.graphics),
            Some(self.compute),
            Some(self.transfer),
            self.video_decode,
        ]
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_plan_prefers_dedicated_compute_and_transfer() {
        let plan = QueuePlan::select(&[
            QueueFamilyInfo {
                index: 0,
                queue_count: 1,
                flags: vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE,
                video_decode_operations: VideoDecodeOperations::empty(),
            },
            QueueFamilyInfo {
                index: 1,
                queue_count: 1,
                flags: vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER,
                video_decode_operations: VideoDecodeOperations::empty(),
            },
            QueueFamilyInfo {
                index: 2,
                queue_count: 1,
                flags: vk::QueueFlags::TRANSFER,
                video_decode_operations: VideoDecodeOperations::empty(),
            },
        ])
        .unwrap();
        assert_eq!(plan.graphics, 0);
        assert_eq!(plan.compute, 1);
        assert_eq!(plan.transfer, 2);
        assert_eq!(plan.unique_families(), vec![0, 1, 2]);
    }

    #[test]
    fn video_plan_requires_one_family_supporting_the_complete_codec_set() {
        let families = [
            QueueFamilyInfo {
                index: 0,
                queue_count: 1,
                flags: vk::QueueFlags::GRAPHICS,
                video_decode_operations: VideoDecodeOperations::empty(),
            },
            QueueFamilyInfo {
                index: 1,
                queue_count: 1,
                flags: vk::QueueFlags::VIDEO_DECODE_KHR,
                video_decode_operations: VideoDecodeOperations::H264,
            },
            QueueFamilyInfo {
                index: 2,
                queue_count: 1,
                flags: vk::QueueFlags::VIDEO_DECODE_KHR,
                video_decode_operations: VideoDecodeOperations::H264
                    .union(VideoDecodeOperations::H265),
            },
        ];
        let requirements = VideoDecodeRequirements::new(
            crate::VideoDecodeCodecs::H264_HIGH_8 | crate::VideoDecodeCodecs::H265_MAIN_10,
        )
        .unwrap();
        let plan = QueuePlan::select(&families)
            .unwrap()
            .require_video_decode(&families, requirements)
            .unwrap();
        assert_eq!(plan.video_decode, Some(2));
        assert_eq!(plan.unique_families(), vec![0, 2]);
    }
}
