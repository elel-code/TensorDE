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

/// Dimension of one typed texture view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ImageViewDimension {
    D1,
    D2,
    D3,
    Cube,
    D1Array,
    D2Array,
    CubeArray,
}

impl ImageViewDimension {
    pub(crate) const fn to_vk(self) -> vk::ImageViewType {
        match self {
            Self::D1 => vk::ImageViewType::_1D,
            Self::D2 => vk::ImageViewType::_2D,
            Self::D3 => vk::ImageViewType::_3D,
            Self::Cube => vk::ImageViewType::CUBE,
            Self::D1Array => vk::ImageViewType::_1D_ARRAY,
            Self::D2Array => vk::ImageViewType::_2D_ARRAY,
            Self::CubeArray => vk::ImageViewType::CUBE_ARRAY,
        }
    }
}

/// Per-component texture-view swizzle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ComponentSwizzle {
    Identity,
    Zero,
    One,
    Red,
    Green,
    Blue,
    Alpha,
}

impl ComponentSwizzle {
    pub(crate) const fn to_vk(self) -> vk::ComponentSwizzle {
        match self {
            Self::Identity => vk::ComponentSwizzle::IDENTITY,
            Self::Zero => vk::ComponentSwizzle::ZERO,
            Self::One => vk::ComponentSwizzle::ONE,
            Self::Red => vk::ComponentSwizzle::R,
            Self::Green => vk::ComponentSwizzle::G,
            Self::Blue => vk::ComponentSwizzle::B,
            Self::Alpha => vk::ComponentSwizzle::A,
        }
    }
}

/// Channel mapping for a typed texture view or dma-buf import/export.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ComponentMapping {
    pub red: ComponentSwizzle,
    pub green: ComponentSwizzle,
    pub blue: ComponentSwizzle,
    pub alpha: ComponentSwizzle,
}

impl ComponentMapping {
    pub const IDENTITY: Self = Self {
        red: ComponentSwizzle::Identity,
        green: ComponentSwizzle::Identity,
        blue: ComponentSwizzle::Identity,
        alpha: ComponentSwizzle::Identity,
    };

    pub(crate) const fn to_vk(self) -> vk::ComponentMapping {
        vk::ComponentMapping {
            r: self.red.to_vk(),
            g: self.green.to_vk(),
            b: self.blue.to_vk(),
            a: self.alpha.to_vk(),
        }
    }
}

impl Default for ComponentMapping {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Bitset of texture aspects addressed by one view, copy, or barrier range.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct TextureAspects(u8);

impl TextureAspects {
    pub const COLOR: Self = Self(1 << 0);
    pub const DEPTH: Self = Self(1 << 1);
    pub const STENCIL: Self = Self(1 << 2);
    pub const PLANE_0: Self = Self(1 << 3);
    pub const PLANE_1: Self = Self(1 << 4);
    pub const PLANE_2: Self = Self(1 << 5);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) const fn to_vk(self) -> vk::ImageAspectFlags {
        let mut flags = vk::ImageAspectFlags::empty();
        if self.contains(Self::COLOR) {
            flags = flags.union(vk::ImageAspectFlags::COLOR);
        }
        if self.contains(Self::DEPTH) {
            flags = flags.union(vk::ImageAspectFlags::DEPTH);
        }
        if self.contains(Self::STENCIL) {
            flags = flags.union(vk::ImageAspectFlags::STENCIL);
        }
        if self.contains(Self::PLANE_0) {
            flags = flags.union(vk::ImageAspectFlags::PLANE_0);
        }
        if self.contains(Self::PLANE_1) {
            flags = flags.union(vk::ImageAspectFlags::PLANE_1);
        }
        if self.contains(Self::PLANE_2) {
            flags = flags.union(vk::ImageAspectFlags::PLANE_2);
        }
        flags
    }
}

impl std::fmt::Debug for TextureAspects {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("TextureAspects")
            .field(&self.0)
            .finish()
    }
}

impl BitOr for TextureAspects {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextureAspects {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Explicit mip/layer range for a texture view, render-graph binding, or
/// transfer command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureSubresourceRange {
    pub aspects: TextureAspects,
    pub base_mip_level: u32,
    pub level_count: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

impl TextureSubresourceRange {
    pub const fn new(
        aspects: TextureAspects,
        base_mip_level: u32,
        level_count: u32,
        base_array_layer: u32,
        layer_count: u32,
    ) -> Self {
        Self {
            aspects,
            base_mip_level,
            level_count,
            base_array_layer,
            layer_count,
        }
    }

    pub const fn full_color(mip_levels: u32, array_layers: u32) -> Self {
        Self::new(TextureAspects::COLOR, 0, mip_levels, 0, array_layers)
    }

    pub(crate) const fn to_vk(self) -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange {
            aspect_mask: self.aspects.to_vk(),
            base_mip_level: self.base_mip_level,
            level_count: self.level_count,
            base_array_layer: self.base_array_layer,
            layer_count: self.layer_count,
        }
    }
}

/// Explicit one-mip/layer selection for image transfer commands.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureSubresourceLayers {
    pub aspects: TextureAspects,
    pub mip_level: u32,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

impl TextureSubresourceLayers {
    pub const fn new(
        aspects: TextureAspects,
        mip_level: u32,
        base_array_layer: u32,
        layer_count: u32,
    ) -> Self {
        Self {
            aspects,
            mip_level,
            base_array_layer,
            layer_count,
        }
    }

    pub const fn color(mip_level: u32, base_array_layer: u32, layer_count: u32) -> Self {
        Self::new(
            TextureAspects::COLOR,
            mip_level,
            base_array_layer,
            layer_count,
        )
    }

    pub(crate) const fn to_vk(self) -> vk::ImageSubresourceLayers {
        vk::ImageSubresourceLayers {
            aspect_mask: self.aspects.to_vk(),
            mip_level: self.mip_level,
            base_array_layer: self.base_array_layer,
            layer_count: self.layer_count,
        }
    }
}

/// Signed three-dimensional texel origin used by explicit image transfers.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Origin3D {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Origin3D {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub(crate) const fn to_vk(self) -> vk::Offset3D {
        vk::Offset3D {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

/// Usages supported by a DRM modifier for a typed texture format.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct TextureFormatFeatures(u16);

impl TextureFormatFeatures {
    pub const SAMPLED: Self = Self(1 << 0);
    pub const STORAGE: Self = Self(1 << 1);
    pub const COLOR_ATTACHMENT: Self = Self(1 << 2);
    pub const DEPTH_STENCIL_ATTACHMENT: Self = Self(1 << 3);
    pub const BLIT_SOURCE: Self = Self(1 << 4);
    pub const BLIT_DESTINATION: Self = Self(1 << 5);
    pub const TRANSFER_SOURCE: Self = Self(1 << 6);
    pub const TRANSFER_DESTINATION: Self = Self(1 << 7);
    pub const DISJOINT: Self = Self(1 << 8);

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub(crate) const fn from_vk(flags: vk::FormatFeatureFlags2) -> Self {
        let mut features = Self(0);
        if flags.contains(vk::FormatFeatureFlags2::SAMPLED_IMAGE) {
            features.0 |= Self::SAMPLED.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::STORAGE_IMAGE) {
            features.0 |= Self::STORAGE.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::COLOR_ATTACHMENT) {
            features.0 |= Self::COLOR_ATTACHMENT.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::DEPTH_STENCIL_ATTACHMENT) {
            features.0 |= Self::DEPTH_STENCIL_ATTACHMENT.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::BLIT_SRC) {
            features.0 |= Self::BLIT_SOURCE.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::BLIT_DST) {
            features.0 |= Self::BLIT_DESTINATION.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::TRANSFER_SRC) {
            features.0 |= Self::TRANSFER_SOURCE.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::TRANSFER_DST) {
            features.0 |= Self::TRANSFER_DESTINATION.0;
        }
        if flags.contains(vk::FormatFeatureFlags2::DISJOINT) {
            features.0 |= Self::DISJOINT.0;
        }
        features
    }
}

impl std::fmt::Debug for TextureFormatFeatures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("TextureFormatFeatures")
            .field(&self.0)
            .finish()
    }
}

impl BitOr for TextureFormatFeatures {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextureFormatFeatures {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
