use std::ops::{BitOr, BitOrAssign};

use vulkanalia::vk;

#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct BufferUsages(u32);

impl BufferUsages {
    pub const COPY_SOURCE: Self = Self(1 << 0);
    pub const COPY_DESTINATION: Self = Self(1 << 1);
    pub const UNIFORM: Self = Self(1 << 2);
    pub const STORAGE: Self = Self(1 << 3);
    pub const INDEX: Self = Self(1 << 4);
    pub const VERTEX: Self = Self(1 << 5);
    pub const INDIRECT: Self = Self(1 << 6);
    pub const SHADER_DEVICE_ADDRESS: Self = Self(1 << 7);
    pub const VIDEO_DECODE_SOURCE: Self = Self(1 << 8);
    pub const VIDEO_DECODE_DESTINATION: Self = Self(1 << 9);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub(crate) fn to_vk(self) -> vk::BufferUsageFlags {
        let mut flags = vk::BufferUsageFlags::empty();
        for (usage, raw) in [
            (Self::COPY_SOURCE, vk::BufferUsageFlags::TRANSFER_SRC),
            (Self::COPY_DESTINATION, vk::BufferUsageFlags::TRANSFER_DST),
            (Self::UNIFORM, vk::BufferUsageFlags::UNIFORM_BUFFER),
            (Self::STORAGE, vk::BufferUsageFlags::STORAGE_BUFFER),
            (Self::INDEX, vk::BufferUsageFlags::INDEX_BUFFER),
            (Self::VERTEX, vk::BufferUsageFlags::VERTEX_BUFFER),
            (Self::INDIRECT, vk::BufferUsageFlags::INDIRECT_BUFFER),
            (
                Self::SHADER_DEVICE_ADDRESS,
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ),
            (
                Self::VIDEO_DECODE_SOURCE,
                vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR,
            ),
            (
                Self::VIDEO_DECODE_DESTINATION,
                vk::BufferUsageFlags::VIDEO_DECODE_DST_KHR,
            ),
        ] {
            if self.contains(usage) {
                flags = flags.union(raw);
            }
        }
        flags
    }
}

impl std::fmt::Debug for BufferUsages {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("BufferUsages")
            .field(&self.0)
            .finish()
    }
}

impl BitOr for BufferUsages {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for BufferUsages {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageDimension {
    D1,
    D2,
    D3,
}

impl ImageDimension {
    pub(crate) const fn to_vk(self) -> vk::ImageType {
        match self {
            Self::D1 => vk::ImageType::_1D,
            Self::D2 => vk::ImageType::_2D,
            Self::D3 => vk::ImageType::_3D,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageTiling {
    Optimal,
    Linear,
}

impl ImageTiling {
    pub(crate) const fn to_vk(self) -> vk::ImageTiling {
        match self {
            Self::Optimal => vk::ImageTiling::OPTIMAL,
            Self::Linear => vk::ImageTiling::LINEAR,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SampleCount {
    One,
    Two,
    Four,
    Eight,
    Sixteen,
    ThirtyTwo,
    SixtyFour,
}

impl SampleCount {
    pub(crate) const fn to_vk(self) -> vk::SampleCountFlags {
        match self {
            Self::One => vk::SampleCountFlags::_1,
            Self::Two => vk::SampleCountFlags::_2,
            Self::Four => vk::SampleCountFlags::_4,
            Self::Eight => vk::SampleCountFlags::_8,
            Self::Sixteen => vk::SampleCountFlags::_16,
            Self::ThirtyTwo => vk::SampleCountFlags::_32,
            Self::SixtyFour => vk::SampleCountFlags::_64,
        }
    }

    pub(crate) const fn as_supported_set(self) -> crate::SampleCounts {
        match self {
            Self::One => crate::SampleCounts::ONE,
            Self::Two => crate::SampleCounts::TWO,
            Self::Four => crate::SampleCounts::FOUR,
            Self::Eight => crate::SampleCounts::EIGHT,
            Self::Sixteen => crate::SampleCounts::SIXTEEN,
            Self::ThirtyTwo => crate::SampleCounts::THIRTY_TWO,
            Self::SixtyFour => crate::SampleCounts::SIXTY_FOUR,
        }
    }
}
