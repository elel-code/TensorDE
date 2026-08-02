use std::ops::{BitOr, BitOrAssign};

use vulkanalia::vk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexFormat {
    Float32,
    Float32x2,
    Float32x4,
    Uint32x4,
}

impl VertexFormat {
    pub(super) const fn to_vk(self) -> vk::Format {
        match self {
            Self::Float32 => vk::Format::R32_SFLOAT,
            Self::Float32x2 => vk::Format::R32G32_SFLOAT,
            Self::Float32x4 => vk::Format::R32G32B32A32_SFLOAT,
            Self::Uint32x4 => vk::Format::R32G32B32A32_UINT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendFactor {
    Zero,
    One,
    SourceAlpha,
    OneMinusSourceAlpha,
    DestinationColor,
    OneMinusSourceColor,
}

impl BlendFactor {
    pub(super) const fn to_vk(self) -> vk::BlendFactor {
        match self {
            Self::Zero => vk::BlendFactor::ZERO,
            Self::One => vk::BlendFactor::ONE,
            Self::SourceAlpha => vk::BlendFactor::SRC_ALPHA,
            Self::OneMinusSourceAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
            Self::DestinationColor => vk::BlendFactor::DST_COLOR,
            Self::OneMinusSourceColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendOperation {
    Add,
    Subtract,
    ReverseSubtract,
    Minimum,
    Maximum,
    Multiply,
    Screen,
    HslColor,
}

impl BlendOperation {
    pub(super) const fn to_vk(self) -> vk::BlendOp {
        match self {
            Self::Add => vk::BlendOp::ADD,
            Self::Subtract => vk::BlendOp::SUBTRACT,
            Self::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
            Self::Minimum => vk::BlendOp::MIN,
            Self::Maximum => vk::BlendOp::MAX,
            Self::Multiply => vk::BlendOp::MULTIPLY_EXT,
            Self::Screen => vk::BlendOp::SCREEN_EXT,
            Self::HslColor => vk::BlendOp::HSL_COLOR_EXT,
        }
    }

    pub(super) const fn is_advanced(self) -> bool {
        matches!(self, Self::Multiply | Self::Screen | Self::HslColor)
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct ColorWrites(u8);

impl ColorWrites {
    pub const RED: Self = Self(1 << 0);
    pub const GREEN: Self = Self(1 << 1);
    pub const BLUE: Self = Self(1 << 2);
    pub const ALPHA: Self = Self(1 << 3);
    pub const RGB: Self = Self(Self::RED.0 | Self::GREEN.0 | Self::BLUE.0);
    pub const ALL: Self = Self(Self::RGB.0 | Self::ALPHA.0);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(super) const fn to_vk(self) -> vk::ColorComponentFlags {
        vk::ColorComponentFlags::from_bits_truncate(self.0 as u32)
    }
}

impl std::fmt::Debug for ColorWrites {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ColorWrites").field(&self.0).finish()
    }
}

impl BitOr for ColorWrites {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ColorWrites {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrimitiveTopology {
    #[default]
    TriangleList,
    TriangleStrip,
}

impl PrimitiveTopology {
    pub(super) const fn to_vk(self) -> vk::PrimitiveTopology {
        match self {
            Self::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
            Self::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PolygonMode {
    #[default]
    Fill,
    Line,
    Point,
}

impl PolygonMode {
    pub(super) const fn to_vk(self) -> vk::PolygonMode {
        match self {
            Self::Fill => vk::PolygonMode::FILL,
            Self::Line => vk::PolygonMode::LINE,
            Self::Point => vk::PolygonMode::POINT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CullMode {
    #[default]
    None,
    Front,
    Back,
    FrontAndBack,
}

impl CullMode {
    pub(super) const fn to_vk(self) -> vk::CullModeFlags {
        match self {
            Self::None => vk::CullModeFlags::NONE,
            Self::Front => vk::CullModeFlags::FRONT,
            Self::Back => vk::CullModeFlags::BACK,
            Self::FrontAndBack => vk::CullModeFlags::FRONT_AND_BACK,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FrontFace {
    Clockwise,
    #[default]
    CounterClockwise,
}

impl FrontFace {
    pub(super) const fn to_vk(self) -> vk::FrontFace {
        match self {
            Self::Clockwise => vk::FrontFace::CLOCKWISE,
            Self::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
        }
    }
}
