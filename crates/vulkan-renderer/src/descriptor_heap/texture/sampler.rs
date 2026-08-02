use vulkanalia::vk::{self, HasBuilder};

use crate::backend::DeviceOwner;
use crate::{Error, Features, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerFilterMode {
    Nearest,
    Linear,
}

impl SamplerFilterMode {
    const fn as_vk(self) -> vk::Filter {
        match self {
            Self::Nearest => vk::Filter::NEAREST,
            Self::Linear => vk::Filter::LINEAR,
        }
    }

    const fn as_mipmap_vk(self) -> vk::SamplerMipmapMode {
        match self {
            Self::Nearest => vk::SamplerMipmapMode::NEAREST,
            Self::Linear => vk::SamplerMipmapMode::LINEAR,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder,
}

impl SamplerAddressMode {
    const fn as_vk(self) -> vk::SamplerAddressMode {
        match self {
            Self::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            Self::Repeat => vk::SamplerAddressMode::REPEAT,
            Self::MirrorRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
            Self::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerBorderColor {
    TransparentBlack,
    OpaqueBlack,
    OpaqueWhite,
}

impl SamplerBorderColor {
    const fn as_vk(self) -> vk::BorderColor {
        match self {
            Self::TransparentBlack => vk::BorderColor::FLOAT_TRANSPARENT_BLACK,
            Self::OpaqueBlack => vk::BorderColor::FLOAT_OPAQUE_BLACK,
            Self::OpaqueWhite => vk::BorderColor::FLOAT_OPAQUE_WHITE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerCompareFunction {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

impl SamplerCompareFunction {
    const fn as_vk(self) -> vk::CompareOp {
        match self {
            Self::Never => vk::CompareOp::NEVER,
            Self::Less => vk::CompareOp::LESS,
            Self::Equal => vk::CompareOp::EQUAL,
            Self::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
            Self::Greater => vk::CompareOp::GREATER,
            Self::NotEqual => vk::CompareOp::NOT_EQUAL,
            Self::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
            Self::Always => vk::CompareOp::ALWAYS,
        }
    }
}

/// Safe normalized-coordinate sampler for a native descriptor heap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerDescriptor {
    pub mag_filter: SamplerFilterMode,
    pub min_filter: SamplerFilterMode,
    pub mipmap_filter: SamplerFilterMode,
    pub address_mode_u: SamplerAddressMode,
    pub address_mode_v: SamplerAddressMode,
    pub address_mode_w: SamplerAddressMode,
    pub mip_lod_bias: f32,
    pub lod_min_clamp: f32,
    pub lod_max_clamp: f32,
    /// Integer anisotropy clamp. `1` disables anisotropic filtering.
    pub max_anisotropy_x1: u32,
    pub compare: Option<SamplerCompareFunction>,
    pub border_color: SamplerBorderColor,
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self::linear_clamp()
    }
}

impl SamplerDescriptor {
    pub const fn linear_clamp() -> Self {
        Self {
            mag_filter: SamplerFilterMode::Linear,
            min_filter: SamplerFilterMode::Linear,
            mipmap_filter: SamplerFilterMode::Linear,
            address_mode_u: SamplerAddressMode::ClampToEdge,
            address_mode_v: SamplerAddressMode::ClampToEdge,
            address_mode_w: SamplerAddressMode::ClampToEdge,
            mip_lod_bias: 0.0,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            max_anisotropy_x1: 1,
            compare: None,
            border_color: SamplerBorderColor::TransparentBlack,
        }
    }

    pub(super) fn validate_device(self, owner: &DeviceOwner) -> Result<()> {
        if self.max_anisotropy_x1 > 1
            && (!owner
                .enabled_features
                .contains(Features::SAMPLER_ANISOTROPY)
                || self.max_anisotropy_x1 > owner.properties.max_sampler_anisotropy_x1)
        {
            return Err(Error::Validation(format!(
                "sampler requests {}x anisotropy but the Device enabled/supports at most {}x",
                self.max_anisotropy_x1, owner.properties.max_sampler_anisotropy_x1
            )));
        }
        Ok(())
    }

    pub(super) fn to_vk(self) -> Result<vk::SamplerCreateInfo> {
        if self.max_anisotropy_x1 == 0
            || !self.mip_lod_bias.is_finite()
            || !self.lod_min_clamp.is_finite()
            || self.lod_min_clamp < 0.0
            || !self.lod_max_clamp.is_finite()
            || self.lod_max_clamp < self.lod_min_clamp
        {
            return Err(Error::Validation(
                "sampler anisotropy and LOD range must be non-zero, finite, and ordered".into(),
            ));
        }
        let compare_enable = self.compare.is_some();
        let compare_op = self
            .compare
            .map_or(vk::CompareOp::ALWAYS, SamplerCompareFunction::as_vk);
        Ok(vk::SamplerCreateInfo::builder()
            .mag_filter(self.mag_filter.as_vk())
            .min_filter(self.min_filter.as_vk())
            .mipmap_mode(self.mipmap_filter.as_mipmap_vk())
            .address_mode_u(self.address_mode_u.as_vk())
            .address_mode_v(self.address_mode_v.as_vk())
            .address_mode_w(self.address_mode_w.as_vk())
            .mip_lod_bias(self.mip_lod_bias)
            .anisotropy_enable(self.max_anisotropy_x1 > 1)
            .max_anisotropy(self.max_anisotropy_x1 as f32)
            .compare_enable(compare_enable)
            .compare_op(compare_op)
            .min_lod(self.lod_min_clamp)
            .max_lod(self.lod_max_clamp)
            .border_color(self.border_color.as_vk())
            .unnormalized_coordinates(false)
            .build())
    }
}
