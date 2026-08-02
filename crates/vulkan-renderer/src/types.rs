//! Backend-neutral value types used at the public renderer boundary.
//!
//! Vulkan mappings stay crate-private. Products select exact rendering
//! semantics without importing or re-exporting the binding crate.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

use vulkanalia::vk;

mod resource;
mod synchronization;
mod version;

pub use resource::{
    BufferUsages, ComponentMapping, ComponentSwizzle, ImageDimension, ImageTiling,
    ImageViewDimension, Origin3D, SampleCount, TextureAspects, TextureFormatFeatures,
    TextureSubresourceLayers, TextureSubresourceRange,
};
pub use synchronization::PipelineStages;
pub use version::{ApiVersion, DeviceType};

#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct SampleCounts(u8);

impl SampleCounts {
    pub const ONE: Self = Self(1 << 0);
    pub const TWO: Self = Self(1 << 1);
    pub const FOUR: Self = Self(1 << 2);
    pub const EIGHT: Self = Self(1 << 3);
    pub const SIXTEEN: Self = Self(1 << 4);
    pub const THIRTY_TWO: Self = Self(1 << 5);
    pub const SIXTY_FOUR: Self = Self(1 << 6);

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn from_vk(flags: vk::SampleCountFlags) -> Self {
        let mut counts = Self(0);
        if flags.contains(vk::SampleCountFlags::_1) {
            counts.0 |= Self::ONE.0;
        }
        if flags.contains(vk::SampleCountFlags::_2) {
            counts.0 |= Self::TWO.0;
        }
        if flags.contains(vk::SampleCountFlags::_4) {
            counts.0 |= Self::FOUR.0;
        }
        if flags.contains(vk::SampleCountFlags::_8) {
            counts.0 |= Self::EIGHT.0;
        }
        if flags.contains(vk::SampleCountFlags::_16) {
            counts.0 |= Self::SIXTEEN.0;
        }
        if flags.contains(vk::SampleCountFlags::_32) {
            counts.0 |= Self::THIRTY_TWO.0;
        }
        if flags.contains(vk::SampleCountFlags::_64) {
            counts.0 |= Self::SIXTY_FOUR.0;
        }
        counts
    }
}

impl std::fmt::Debug for SampleCounts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_set()
            .entries(
                [
                    (Self::ONE, 1),
                    (Self::TWO, 2),
                    (Self::FOUR, 4),
                    (Self::EIGHT, 8),
                    (Self::SIXTEEN, 16),
                    (Self::THIRTY_TWO, 32),
                    (Self::SIXTY_FOUR, 64),
                ]
                .into_iter()
                .filter_map(|(flag, count)| self.contains(flag).then_some(count)),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Extent2D {
    pub width: u32,
    pub height: u32,
}

impl Extent2D {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub(crate) const fn to_vk(self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Origin2D {
    pub x: i32,
    pub y: i32,
}

impl Origin2D {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Rect2D {
    pub origin: Origin2D,
    pub extent: Extent2D,
}

impl Rect2D {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            origin: Origin2D::new(x, y),
            extent: Extent2D::new(width, height),
        }
    }

    pub(crate) const fn to_vk(self) -> vk::Rect2D {
        vk::Rect2D {
            offset: vk::Offset2D {
                x: self.origin.x,
                y: self.origin.y,
            },
            extent: self.extent.to_vk(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Viewport {
    pub(crate) const fn to_vk(self) -> vk::Viewport {
        vk::Viewport {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            min_depth: self.min_depth,
            max_depth: self.max_depth,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Extent3D {
    pub width: u32,
    pub height: u32,
    pub depth_or_layers: u32,
}

impl Extent3D {
    pub const fn new(width: u32, height: u32, depth_or_layers: u32) -> Self {
        Self {
            width,
            height,
            depth_or_layers,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0 || self.depth_or_layers == 0
    }

    pub(crate) const fn to_vk(self) -> vk::Extent3D {
        vk::Extent3D {
            width: self.width,
            height: self.height,
            depth: self.depth_or_layers,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureFormat {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba8Srgb,
    Bgra8Unorm,
    Bgra8Srgb,
    A2R10G10B10UnormPack32,
    A2B10G10R10UnormPack32,
    R16Float,
    Rg16Float,
    Rgba16Float,
    R16Unorm,
    Rg16Unorm,
    R32Float,
    Rg32Float,
    Rgba32Float,
    Rgba32Uint,
    Bc1RgbaUnorm,
    Bc2RgbaUnorm,
    Bc3RgbaUnorm,
    Bc4RUnorm,
    Bc5RgUnorm,
    Bc7RgbaUnorm,
    G8B8R8TwoPlane420Unorm,
    G8B8R8TwoPlane422Unorm,
    G8B8R8TwoPlane444Unorm,
    G8B8R8ThreePlane420Unorm,
    G8B8R8ThreePlane422Unorm,
    G8B8R8ThreePlane444Unorm,
    G10X6B10X6R10X6TwoPlane420Unorm3Pack16,
    G10X6B10X6R10X6ThreePlane420Unorm3Pack16,
    G16B16R16TwoPlane420Unorm,
}

impl TextureFormat {
    pub(crate) const fn from_vk(format: vk::Format) -> Option<Self> {
        match format {
            vk::Format::R8_UNORM => Some(Self::R8Unorm),
            vk::Format::R8G8_UNORM => Some(Self::Rg8Unorm),
            vk::Format::R8G8B8A8_UNORM => Some(Self::Rgba8Unorm),
            vk::Format::R8G8B8A8_SRGB => Some(Self::Rgba8Srgb),
            vk::Format::B8G8R8A8_UNORM => Some(Self::Bgra8Unorm),
            vk::Format::B8G8R8A8_SRGB => Some(Self::Bgra8Srgb),
            vk::Format::A2R10G10B10_UNORM_PACK32 => Some(Self::A2R10G10B10UnormPack32),
            vk::Format::A2B10G10R10_UNORM_PACK32 => Some(Self::A2B10G10R10UnormPack32),
            vk::Format::R16_SFLOAT => Some(Self::R16Float),
            vk::Format::R16G16_SFLOAT => Some(Self::Rg16Float),
            vk::Format::R16G16B16A16_SFLOAT => Some(Self::Rgba16Float),
            vk::Format::R16_UNORM => Some(Self::R16Unorm),
            vk::Format::R16G16_UNORM => Some(Self::Rg16Unorm),
            vk::Format::R32_SFLOAT => Some(Self::R32Float),
            vk::Format::R32G32_SFLOAT => Some(Self::Rg32Float),
            vk::Format::R32G32B32A32_SFLOAT => Some(Self::Rgba32Float),
            vk::Format::R32G32B32A32_UINT => Some(Self::Rgba32Uint),
            vk::Format::BC1_RGBA_UNORM_BLOCK => Some(Self::Bc1RgbaUnorm),
            vk::Format::BC2_UNORM_BLOCK => Some(Self::Bc2RgbaUnorm),
            vk::Format::BC3_UNORM_BLOCK => Some(Self::Bc3RgbaUnorm),
            vk::Format::BC4_UNORM_BLOCK => Some(Self::Bc4RUnorm),
            vk::Format::BC5_UNORM_BLOCK => Some(Self::Bc5RgUnorm),
            vk::Format::BC7_UNORM_BLOCK => Some(Self::Bc7RgbaUnorm),
            vk::Format::G8_B8R8_2PLANE_420_UNORM => Some(Self::G8B8R8TwoPlane420Unorm),
            vk::Format::G8_B8R8_2PLANE_422_UNORM => Some(Self::G8B8R8TwoPlane422Unorm),
            vk::Format::G8_B8R8_2PLANE_444_UNORM => Some(Self::G8B8R8TwoPlane444Unorm),
            vk::Format::G8_B8_R8_3PLANE_420_UNORM => Some(Self::G8B8R8ThreePlane420Unorm),
            vk::Format::G8_B8_R8_3PLANE_422_UNORM => Some(Self::G8B8R8ThreePlane422Unorm),
            vk::Format::G8_B8_R8_3PLANE_444_UNORM => Some(Self::G8B8R8ThreePlane444Unorm),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
                Some(Self::G10X6B10X6R10X6TwoPlane420Unorm3Pack16)
            }
            vk::Format::G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16 => {
                Some(Self::G10X6B10X6R10X6ThreePlane420Unorm3Pack16)
            }
            vk::Format::G16_B16R16_2PLANE_420_UNORM => Some(Self::G16B16R16TwoPlane420Unorm),
            _ => None,
        }
    }

    pub(crate) const fn to_vk(self) -> vk::Format {
        match self {
            Self::R8Unorm => vk::Format::R8_UNORM,
            Self::Rg8Unorm => vk::Format::R8G8_UNORM,
            Self::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
            Self::Rgba8Srgb => vk::Format::R8G8B8A8_SRGB,
            Self::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
            Self::Bgra8Srgb => vk::Format::B8G8R8A8_SRGB,
            Self::A2R10G10B10UnormPack32 => vk::Format::A2R10G10B10_UNORM_PACK32,
            Self::A2B10G10R10UnormPack32 => vk::Format::A2B10G10R10_UNORM_PACK32,
            Self::R16Float => vk::Format::R16_SFLOAT,
            Self::Rg16Float => vk::Format::R16G16_SFLOAT,
            Self::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
            Self::R16Unorm => vk::Format::R16_UNORM,
            Self::Rg16Unorm => vk::Format::R16G16_UNORM,
            Self::R32Float => vk::Format::R32_SFLOAT,
            Self::Rg32Float => vk::Format::R32G32_SFLOAT,
            Self::Rgba32Float => vk::Format::R32G32B32A32_SFLOAT,
            Self::Rgba32Uint => vk::Format::R32G32B32A32_UINT,
            Self::Bc1RgbaUnorm => vk::Format::BC1_RGBA_UNORM_BLOCK,
            Self::Bc2RgbaUnorm => vk::Format::BC2_UNORM_BLOCK,
            Self::Bc3RgbaUnorm => vk::Format::BC3_UNORM_BLOCK,
            Self::Bc4RUnorm => vk::Format::BC4_UNORM_BLOCK,
            Self::Bc5RgUnorm => vk::Format::BC5_UNORM_BLOCK,
            Self::Bc7RgbaUnorm => vk::Format::BC7_UNORM_BLOCK,
            Self::G8B8R8TwoPlane420Unorm => vk::Format::G8_B8R8_2PLANE_420_UNORM,
            Self::G8B8R8TwoPlane422Unorm => vk::Format::G8_B8R8_2PLANE_422_UNORM,
            Self::G8B8R8TwoPlane444Unorm => vk::Format::G8_B8R8_2PLANE_444_UNORM,
            Self::G8B8R8ThreePlane420Unorm => vk::Format::G8_B8_R8_3PLANE_420_UNORM,
            Self::G8B8R8ThreePlane422Unorm => vk::Format::G8_B8_R8_3PLANE_422_UNORM,
            Self::G8B8R8ThreePlane444Unorm => vk::Format::G8_B8_R8_3PLANE_444_UNORM,
            Self::G10X6B10X6R10X6TwoPlane420Unorm3Pack16 => {
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            }
            Self::G10X6B10X6R10X6ThreePlane420Unorm3Pack16 => {
                vk::Format::G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16
            }
            Self::G16B16R16TwoPlane420Unorm => vk::Format::G16_B16R16_2PLANE_420_UNORM,
        }
    }
}

#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct TextureUsages(u32);

impl TextureUsages {
    pub const COPY_SOURCE: Self = Self(1 << 0);
    pub const COPY_DESTINATION: Self = Self(1 << 1);
    pub const SAMPLED: Self = Self(1 << 2);
    pub const STORAGE: Self = Self(1 << 3);
    pub const COLOR_ATTACHMENT: Self = Self(1 << 4);
    pub const DEPTH_STENCIL_ATTACHMENT: Self = Self(1 << 5);
    pub const INPUT_ATTACHMENT: Self = Self(1 << 6);
    pub const TRANSIENT_ATTACHMENT: Self = Self(1 << 7);
    pub const VIDEO_DECODE_SOURCE: Self = Self(1 << 8);
    pub const VIDEO_DECODE_DESTINATION: Self = Self(1 << 9);
    pub const VIDEO_DECODE_DPB: Self = Self(1 << 10);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn from_vk(flags: vk::ImageUsageFlags) -> Self {
        let mut usages = Self(0);
        if flags.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
            usages.0 |= Self::COPY_SOURCE.0;
        }
        if flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
            usages.0 |= Self::COPY_DESTINATION.0;
        }
        if flags.contains(vk::ImageUsageFlags::SAMPLED) {
            usages.0 |= Self::SAMPLED.0;
        }
        if flags.contains(vk::ImageUsageFlags::STORAGE) {
            usages.0 |= Self::STORAGE.0;
        }
        if flags.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
            usages.0 |= Self::COLOR_ATTACHMENT.0;
        }
        if flags.contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT) {
            usages.0 |= Self::DEPTH_STENCIL_ATTACHMENT.0;
        }
        if flags.contains(vk::ImageUsageFlags::INPUT_ATTACHMENT) {
            usages.0 |= Self::INPUT_ATTACHMENT.0;
        }
        if flags.contains(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT) {
            usages.0 |= Self::TRANSIENT_ATTACHMENT.0;
        }
        if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR) {
            usages.0 |= Self::VIDEO_DECODE_SOURCE.0;
        }
        if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR) {
            usages.0 |= Self::VIDEO_DECODE_DESTINATION.0;
        }
        if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR) {
            usages.0 |= Self::VIDEO_DECODE_DPB.0;
        }
        usages
    }

    pub(crate) const fn to_vk(self) -> vk::ImageUsageFlags {
        let mut flags = vk::ImageUsageFlags::empty();
        if self.contains(Self::COPY_SOURCE) {
            flags = flags.union(vk::ImageUsageFlags::TRANSFER_SRC);
        }
        if self.contains(Self::COPY_DESTINATION) {
            flags = flags.union(vk::ImageUsageFlags::TRANSFER_DST);
        }
        if self.contains(Self::SAMPLED) {
            flags = flags.union(vk::ImageUsageFlags::SAMPLED);
        }
        if self.contains(Self::STORAGE) {
            flags = flags.union(vk::ImageUsageFlags::STORAGE);
        }
        if self.contains(Self::COLOR_ATTACHMENT) {
            flags = flags.union(vk::ImageUsageFlags::COLOR_ATTACHMENT);
        }
        if self.contains(Self::DEPTH_STENCIL_ATTACHMENT) {
            flags = flags.union(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
        }
        if self.contains(Self::INPUT_ATTACHMENT) {
            flags = flags.union(vk::ImageUsageFlags::INPUT_ATTACHMENT);
        }
        if self.contains(Self::TRANSIENT_ATTACHMENT) {
            flags = flags.union(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT);
        }
        if self.contains(Self::VIDEO_DECODE_SOURCE) {
            flags = flags.union(vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR);
        }
        if self.contains(Self::VIDEO_DECODE_DESTINATION) {
            flags = flags.union(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR);
        }
        if self.contains(Self::VIDEO_DECODE_DPB) {
            flags = flags.union(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR);
        }
        flags
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorSpace {
    SrgbNonlinear,
}

impl ColorSpace {
    pub(crate) const fn from_vk(color_space: vk::ColorSpaceKHR) -> Option<Self> {
        match color_space {
            vk::ColorSpaceKHR::SRGB_NONLINEAR => Some(Self::SrgbNonlinear),
            _ => None,
        }
    }

    pub(crate) const fn to_vk(self) -> vk::ColorSpaceKHR {
        match self {
            Self::SrgbNonlinear => vk::ColorSpaceKHR::SRGB_NONLINEAR,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SurfaceFormat {
    pub format: TextureFormat,
    pub color_space: ColorSpace,
}

impl SurfaceFormat {
    pub const fn new(format: TextureFormat, color_space: ColorSpace) -> Self {
        Self {
            format,
            color_space,
        }
    }

    pub(crate) const fn from_vk(raw: vk::SurfaceFormatKHR) -> Option<Self> {
        let Some(format) = TextureFormat::from_vk(raw.format) else {
            return None;
        };
        let Some(color_space) = ColorSpace::from_vk(raw.color_space) else {
            return None;
        };
        Some(Self {
            format,
            color_space,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompositeAlphaMode {
    Opaque,
    PreMultiplied,
    PostMultiplied,
    Inherit,
}

impl CompositeAlphaMode {
    pub(crate) const fn to_vk(self) -> vk::CompositeAlphaFlagsKHR {
        match self {
            Self::Opaque => vk::CompositeAlphaFlagsKHR::OPAQUE,
            Self::PreMultiplied => vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            Self::PostMultiplied => vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            Self::Inherit => vk::CompositeAlphaFlagsKHR::INHERIT,
        }
    }
}

#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct CompositeAlphaModes(u8);

impl CompositeAlphaModes {
    pub const fn contains(self, mode: CompositeAlphaMode) -> bool {
        self.0 & (1 << mode as u8) != 0
    }

    pub(crate) const fn from_vk(flags: vk::CompositeAlphaFlagsKHR) -> Self {
        let mut modes = Self(0);
        if flags.contains(vk::CompositeAlphaFlagsKHR::OPAQUE) {
            modes.0 |= 1 << CompositeAlphaMode::Opaque as u8;
        }
        if flags.contains(vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED) {
            modes.0 |= 1 << CompositeAlphaMode::PreMultiplied as u8;
        }
        if flags.contains(vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED) {
            modes.0 |= 1 << CompositeAlphaMode::PostMultiplied as u8;
        }
        if flags.contains(vk::CompositeAlphaFlagsKHR::INHERIT) {
            modes.0 |= 1 << CompositeAlphaMode::Inherit as u8;
        }
        modes
    }
}

impl std::fmt::Debug for CompositeAlphaModes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_set()
            .entries(
                [
                    CompositeAlphaMode::Opaque,
                    CompositeAlphaMode::PreMultiplied,
                    CompositeAlphaMode::PostMultiplied,
                    CompositeAlphaMode::Inherit,
                ]
                .into_iter()
                .filter(|mode| self.contains(*mode)),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SurfaceTransform {
    Identity,
    Rotate90,
    Rotate180,
    Rotate270,
    HorizontalMirror,
    HorizontalMirrorRotate90,
    HorizontalMirrorRotate180,
    HorizontalMirrorRotate270,
    Inherit,
}

impl SurfaceTransform {
    pub(crate) const fn to_vk(self) -> vk::SurfaceTransformFlagsKHR {
        match self {
            Self::Identity => vk::SurfaceTransformFlagsKHR::IDENTITY,
            Self::Rotate90 => vk::SurfaceTransformFlagsKHR::ROTATE_90,
            Self::Rotate180 => vk::SurfaceTransformFlagsKHR::ROTATE_180,
            Self::Rotate270 => vk::SurfaceTransformFlagsKHR::ROTATE_270,
            Self::HorizontalMirror => vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR,
            Self::HorizontalMirrorRotate90 => {
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90
            }
            Self::HorizontalMirrorRotate180 => {
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180
            }
            Self::HorizontalMirrorRotate270 => {
                vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270
            }
            Self::Inherit => vk::SurfaceTransformFlagsKHR::INHERIT,
        }
    }

    pub(crate) const fn from_vk(transform: vk::SurfaceTransformFlagsKHR) -> Option<Self> {
        match transform {
            vk::SurfaceTransformFlagsKHR::IDENTITY => Some(Self::Identity),
            vk::SurfaceTransformFlagsKHR::ROTATE_90 => Some(Self::Rotate90),
            vk::SurfaceTransformFlagsKHR::ROTATE_180 => Some(Self::Rotate180),
            vk::SurfaceTransformFlagsKHR::ROTATE_270 => Some(Self::Rotate270),
            vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR => Some(Self::HorizontalMirror),
            vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_90 => {
                Some(Self::HorizontalMirrorRotate90)
            }
            vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_180 => {
                Some(Self::HorizontalMirrorRotate180)
            }
            vk::SurfaceTransformFlagsKHR::HORIZONTAL_MIRROR_ROTATE_270 => {
                Some(Self::HorizontalMirrorRotate270)
            }
            vk::SurfaceTransformFlagsKHR::INHERIT => Some(Self::Inherit),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct SurfaceTransforms(u16);

impl SurfaceTransforms {
    pub const fn contains(self, transform: SurfaceTransform) -> bool {
        self.0 & (1 << transform as u8) != 0
    }

    pub(crate) const fn from_vk(flags: vk::SurfaceTransformFlagsKHR) -> Self {
        let mut transforms = Self(0);
        let variants = [
            SurfaceTransform::Identity,
            SurfaceTransform::Rotate90,
            SurfaceTransform::Rotate180,
            SurfaceTransform::Rotate270,
            SurfaceTransform::HorizontalMirror,
            SurfaceTransform::HorizontalMirrorRotate90,
            SurfaceTransform::HorizontalMirrorRotate180,
            SurfaceTransform::HorizontalMirrorRotate270,
            SurfaceTransform::Inherit,
        ];
        let mut index = 0;
        while index < variants.len() {
            let transform = variants[index];
            if flags.contains(transform.to_vk()) {
                transforms.0 |= 1 << transform as u8;
            }
            index += 1;
        }
        transforms
    }
}

impl std::fmt::Debug for SurfaceTransforms {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_set()
            .entries(
                [
                    SurfaceTransform::Identity,
                    SurfaceTransform::Rotate90,
                    SurfaceTransform::Rotate180,
                    SurfaceTransform::Rotate270,
                    SurfaceTransform::HorizontalMirror,
                    SurfaceTransform::HorizontalMirrorRotate90,
                    SurfaceTransform::HorizontalMirrorRotate180,
                    SurfaceTransform::HorizontalMirrorRotate270,
                    SurfaceTransform::Inherit,
                ]
                .into_iter()
                .filter(|transform| self.contains(*transform)),
            )
            .finish()
    }
}

impl std::fmt::Debug for TextureUsages {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("TextureUsages")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl BitOr for TextureUsages {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextureUsages {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for TextureUsages {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for TextureUsages {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureLayout {
    Undefined,
    General,
    ColorAttachment,
    ShaderReadOnly,
    TransferSource,
    TransferDestination,
    RenderingLocalRead,
    Present,
    VideoDecodeDpb,
}

impl TextureLayout {
    pub(crate) const fn to_vk(self) -> vk::ImageLayout {
        match self {
            Self::Undefined => vk::ImageLayout::UNDEFINED,
            Self::General => vk::ImageLayout::GENERAL,
            Self::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            Self::ShaderReadOnly => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            Self::TransferSource => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            Self::TransferDestination => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            Self::RenderingLocalRead => vk::ImageLayout::RENDERING_LOCAL_READ,
            Self::Present => vk::ImageLayout::PRESENT_SRC_KHR,
            Self::VideoDecodeDpb => vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gilder_texture_formats_have_exact_vulkan_mappings() {
        for format in [
            TextureFormat::R8Unorm,
            TextureFormat::Rgba8Unorm,
            TextureFormat::Rgba8Srgb,
            TextureFormat::Bgra8Unorm,
            TextureFormat::A2R10G10B10UnormPack32,
            TextureFormat::A2B10G10R10UnormPack32,
            TextureFormat::Rgba16Float,
            TextureFormat::Bc7RgbaUnorm,
            TextureFormat::G8B8R8TwoPlane420Unorm,
            TextureFormat::G10X6B10X6R10X6TwoPlane420Unorm3Pack16,
        ] {
            assert_ne!(format.to_vk(), vk::Format::UNDEFINED);
        }
    }

    #[test]
    fn texture_usage_mapping_keeps_scene_and_video_roles_distinct() {
        let usages = TextureUsages::COLOR_ATTACHMENT
            | TextureUsages::SAMPLED
            | TextureUsages::INPUT_ATTACHMENT
            | TextureUsages::VIDEO_DECODE_DPB;
        let vk = usages.to_vk();
        assert!(vk.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT));
        assert!(vk.contains(vk::ImageUsageFlags::SAMPLED));
        assert!(vk.contains(vk::ImageUsageFlags::INPUT_ATTACHMENT));
        assert!(vk.contains(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR));
        assert!(!vk.contains(vk::ImageUsageFlags::STORAGE));
    }
}
