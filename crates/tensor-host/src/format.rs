//! DRM fourcc + modifier as pure integers (no libdrm / Smithay).

use core::fmt;
use thiserror::Error;

/// FourCC pixel format code (`DRM_FORMAT_*`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Fourcc(pub u32);

impl Fourcc {
    pub const XRGB8888: Self = Self(u32::from_le_bytes(*b"XR24"));
    pub const ARGB8888: Self = Self(u32::from_le_bytes(*b"AR24"));
    pub const XBGR8888: Self = Self(u32::from_le_bytes(*b"XB24"));
    pub const ABGR8888: Self = Self(u32::from_le_bytes(*b"AB24"));
    pub const XRGB2101010: Self = Self(u32::from_le_bytes(*b"XR30"));
    pub const ARGB2101010: Self = Self(u32::from_le_bytes(*b"AR30"));
    pub const XBGR2101010: Self = Self(u32::from_le_bytes(*b"XB30"));
    pub const ABGR2101010: Self = Self(u32::from_le_bytes(*b"AB30"));
    pub const NV12: Self = Self(u32::from_le_bytes(*b"NV12"));

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl fmt::Display for Fourcc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0.to_le_bytes();
        for b in bytes {
            if b.is_ascii_graphic() {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

/// DRM format modifier (`DRM_FORMAT_MOD_*`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Modifier(pub u64);

impl Modifier {
    pub const LINEAR: Self = Self(0);
    /// Invalid / unspecified modifier (matches common DRM sentinel).
    pub const INVALID: Self = Self(u64::MAX);

    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }

    #[inline]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn is_linear(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_invalid(self) -> bool {
        self.0 == u64::MAX
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

/// Combined fourcc + modifier for format negotiation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DrmFormat {
    pub code: Fourcc,
    pub modifier: Modifier,
}

impl DrmFormat {
    #[inline]
    pub const fn new(code: Fourcc, modifier: Modifier) -> Self {
        Self { code, modifier }
    }

    #[inline]
    pub const fn linear(code: Fourcc) -> Self {
        Self::new(code, Modifier::LINEAR)
    }
}

/// Vulkan / GBM / KMS capability row used by output negotiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatCapability {
    pub format: DrmFormat,
    pub plane_count: u32,
    pub renderable: bool,
    pub importable: bool,
    pub exportable: bool,
}

impl FormatCapability {
    #[inline]
    pub const fn supports_client_import(self) -> bool {
        self.renderable && self.importable
    }

    #[inline]
    pub const fn supports_output_export(self) -> bool {
        self.renderable && self.exportable
    }
}

/// GBM-side capability for a format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GbmCapability {
    pub format: DrmFormat,
    pub scanout: bool,
    pub plane_count: Option<u32>,
}

/// Negotiated scanout format for one output path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputFormat {
    pub format: DrmFormat,
    pub plane_count: u32,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum FormatError {
    #[error("no renderable, exportable, GBM-allocatable DRM modifier can reach the KMS plane")]
    NoCompatibleOutputFormat,
}

/// Prefer XRGB/ARGB 8-bit, then 10-bit; tiled modifiers before linear.
pub fn negotiate_output_formats(
    vulkan: &[FormatCapability],
    kms_scanout: &[DrmFormat],
    gbm: &[GbmCapability],
) -> Result<Vec<OutputFormat>, FormatError> {
    let mut candidates = vulkan
        .iter()
        .copied()
        .filter(|c| c.supports_output_export())
        .filter(|c| c.plane_count > 0)
        .filter(|c| !c.format.modifier.is_invalid())
        .filter(|c| output_format_rank(c.format.code).is_some())
        .filter(|c| kms_scanout.contains(&c.format))
        .filter(|c| {
            gbm.iter()
                .any(|g| g.format == c.format && g.scanout && g.plane_count == Some(c.plane_count))
        })
        .map(|c| OutputFormat {
            format: c.format,
            plane_count: c.plane_count,
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|c| preference_key(c.format));
    candidates.dedup_by_key(|c| (c.format.code.raw(), c.format.modifier.raw()));
    if candidates.is_empty() {
        return Err(FormatError::NoCompatibleOutputFormat);
    }
    Ok(candidates)
}

fn preference_key(format: DrmFormat) -> (u8, u8, u64) {
    let modifier_rank = if format.modifier.is_linear() {
        1
    } else if format.modifier.is_invalid() {
        2
    } else {
        0
    };
    (
        output_format_rank(format.code).unwrap_or(u8::MAX),
        modifier_rank,
        format.modifier.raw(),
    )
}

const fn output_format_rank(format: Fourcc) -> Option<u8> {
    match format {
        Fourcc::XRGB8888 => Some(0),
        Fourcc::ARGB8888 => Some(1),
        Fourcc::XBGR8888 => Some(2),
        Fourcc::ABGR8888 => Some(3),
        Fourcc::XRGB2101010 => Some(4),
        Fourcc::ARGB2101010 => Some(5),
        Fourcc::XBGR2101010 => Some(6),
        Fourcc::ABGR2101010 => Some(7),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(code: Fourcc, modifier: u64) -> DrmFormat {
        DrmFormat::new(code, Modifier::from_raw(modifier))
    }

    fn vulkan(format: DrmFormat, exportable: bool) -> FormatCapability {
        FormatCapability {
            format,
            plane_count: 1,
            renderable: true,
            importable: true,
            exportable,
        }
    }

    fn gbm(format: DrmFormat) -> GbmCapability {
        GbmCapability {
            format,
            scanout: true,
            plane_count: Some(1),
        }
    }

    #[test]
    fn xrgb_fourcc_matches_drm_ascii() {
        assert_eq!(Fourcc::XRGB8888.0, 0x3432_5258);
    }

    #[test]
    fn unsupported_fourcc_is_not_an_output_candidate() {
        let nv12 = fmt(Fourcc::NV12, 9);
        assert_eq!(
            negotiate_output_formats(&[vulkan(nv12, true)], &[nv12], &[gbm(nv12)]),
            Err(FormatError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn modifier_must_match_across_boundaries() {
        let a = fmt(Fourcc::XRGB8888, 9);
        let b = fmt(Fourcc::XRGB8888, 10);
        assert_eq!(
            negotiate_output_formats(&[vulkan(a, true)], &[b], &[gbm(b)]),
            Err(FormatError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn implicit_modifier_is_rejected() {
        let candidate = DrmFormat::new(Fourcc::XRGB8888, Modifier::INVALID);
        assert_eq!(
            negotiate_output_formats(&[vulkan(candidate, true)], &[candidate], &[gbm(candidate)]),
            Err(FormatError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn preference_order_is_deterministic() {
        let tiled = fmt(Fourcc::XRGB8888, 9);
        let linear = DrmFormat::linear(Fourcc::XRGB8888);
        let alpha = fmt(Fourcc::ARGB8888, 8);
        let vulkan = [
            vulkan(alpha, true),
            vulkan(linear, true),
            vulkan(tiled, true),
            vulkan(tiled, true),
        ];
        let scanout = [linear, alpha, tiled];
        let gbm = [gbm(alpha), gbm(tiled), gbm(linear)];
        let negotiated = negotiate_output_formats(&vulkan, &scanout, &gbm).unwrap();
        assert_eq!(
            negotiated,
            vec![
                OutputFormat {
                    format: tiled,
                    plane_count: 1,
                },
                OutputFormat {
                    format: linear,
                    plane_count: 1,
                },
                OutputFormat {
                    format: alpha,
                    plane_count: 1,
                },
            ]
        );
    }
}
