use std::collections::BTreeSet;

use vulkanalia::vk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueFamilyInfo {
    pub index: u32,
    pub queue_count: u32,
    pub flags: vk::QueueFlags,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueuePlan {
    pub graphics: u32,
    pub compute: u32,
    pub transfer: u32,
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
        })
    }

    pub fn unique_families(self) -> Vec<u32> {
        BTreeSet::from([self.graphics, self.compute, self.transfer])
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
            },
            QueueFamilyInfo {
                index: 1,
                queue_count: 1,
                flags: vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER,
            },
            QueueFamilyInfo {
                index: 2,
                queue_count: 1,
                flags: vk::QueueFlags::TRANSFER,
            },
        ])
        .unwrap();
        assert_eq!(plan.graphics, 0);
        assert_eq!(plan.compute, 1);
        assert_eq!(plan.transfer, 2);
        assert_eq!(plan.unique_families(), vec![0, 1, 2]);
    }
}
