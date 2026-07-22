use smithay::backend::allocator::Format as DrmFormat;
#[cfg(any(feature = "tty", test))]
use smithay::backend::allocator::{Fourcc, Modifier};
#[cfg(any(feature = "tty", test))]
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VulkanFormatCapability {
    pub(crate) format: DrmFormat,
    pub(crate) plane_count: u32,
    pub(crate) renderable: bool,
    pub(crate) importable: bool,
    pub(crate) exportable: bool,
}

impl VulkanFormatCapability {
    pub(crate) const fn supports_client_import(self) -> bool {
        self.renderable && self.importable
    }

    pub(crate) const fn supports_output_export(self) -> bool {
        self.renderable && self.exportable
    }
}

#[cfg(any(feature = "tty", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GbmFormatCapability {
    pub(crate) format: DrmFormat,
    pub(crate) scanout: bool,
    pub(crate) plane_count: Option<u32>,
}

#[cfg(any(feature = "tty", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OutputFormat {
    pub(crate) format: DrmFormat,
    pub(crate) plane_count: u32,
}

#[cfg(any(feature = "tty", test))]
pub(crate) fn negotiate_output_formats(
    vulkan: &[VulkanFormatCapability],
    kms_scanout: &[DrmFormat],
    gbm: &[GbmFormatCapability],
) -> Result<Vec<OutputFormat>, FormatNegotiationError> {
    let mut candidates = vulkan
        .iter()
        .copied()
        .filter(|candidate| candidate.supports_output_export())
        .filter(|candidate| candidate.plane_count > 0)
        .filter(|candidate| candidate.format.modifier != Modifier::Invalid)
        .filter(|candidate| output_format_rank(candidate.format.code).is_some())
        .filter(|candidate| kms_scanout.contains(&candidate.format))
        .filter(|candidate| {
            gbm.iter().any(|gbm| {
                gbm.format == candidate.format
                    && gbm.scanout
                    && gbm.plane_count == Some(candidate.plane_count)
            })
        })
        .map(|candidate| OutputFormat {
            format: candidate.format,
            plane_count: candidate.plane_count,
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|candidate| preference_key(candidate.format));
    candidates.dedup_by_key(|candidate| format_key(candidate.format));
    if candidates.is_empty() {
        return Err(FormatNegotiationError::NoCompatibleOutputFormat);
    }
    Ok(candidates)
}

#[cfg(any(feature = "tty", test))]
fn preference_key(format: DrmFormat) -> (u8, u8, u64) {
    let modifier = u64::from(format.modifier);
    let modifier_rank = if format.modifier == Modifier::Linear {
        1
    } else if format.modifier == Modifier::Invalid {
        2
    } else {
        0
    };
    (
        output_format_rank(format.code).unwrap_or(u8::MAX),
        modifier_rank,
        modifier,
    )
}

#[cfg(any(feature = "tty", test))]
fn format_key(format: DrmFormat) -> (u32, u64) {
    (format.code as u32, u64::from(format.modifier))
}

#[cfg(any(feature = "tty", test))]
const fn output_format_rank(format: Fourcc) -> Option<u8> {
    match format {
        Fourcc::Xrgb8888 => Some(0),
        Fourcc::Argb8888 => Some(1),
        Fourcc::Xbgr8888 => Some(2),
        Fourcc::Abgr8888 => Some(3),
        Fourcc::Xrgb2101010 => Some(4),
        Fourcc::Argb2101010 => Some(5),
        Fourcc::Xbgr2101010 => Some(6),
        Fourcc::Abgr2101010 => Some(7),
        _ => None,
    }
}

#[cfg(any(feature = "tty", test))]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum FormatNegotiationError {
    #[error("no renderable, exportable, GBM-allocatable DRM modifier can reach the KMS plane")]
    NoCompatibleOutputFormat,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(code: Fourcc, modifier: u64) -> DrmFormat {
        DrmFormat {
            code,
            modifier: Modifier::from(modifier),
        }
    }

    fn vulkan(
        format: DrmFormat,
        renderable: bool,
        importable: bool,
        exportable: bool,
    ) -> VulkanFormatCapability {
        VulkanFormatCapability {
            format,
            plane_count: 1,
            renderable,
            importable,
            exportable,
        }
    }

    fn gbm(format: DrmFormat) -> GbmFormatCapability {
        GbmFormatCapability {
            format,
            scanout: true,
            plane_count: Some(1),
        }
    }

    #[test]
    fn unsupported_fourcc_is_not_an_output_candidate() {
        let nv12 = format(Fourcc::Nv12, 9);

        assert_eq!(
            negotiate_output_formats(&[vulkan(nv12, true, true, true)], &[nv12], &[gbm(nv12)]),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn modifier_must_match_across_all_three_boundaries() {
        let vulkan_format = format(Fourcc::Xrgb8888, 9);
        let kms_format = format(Fourcc::Xrgb8888, 10);

        assert_eq!(
            negotiate_output_formats(
                &[vulkan(vulkan_format, true, true, true)],
                &[kms_format],
                &[gbm(kms_format)],
            ),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn implicit_modifier_is_not_a_native_output_path() {
        let candidate = DrmFormat {
            code: Fourcc::Xrgb8888,
            modifier: Modifier::Invalid,
        };

        assert_eq!(
            negotiate_output_formats(
                &[vulkan(candidate, true, true, true)],
                &[candidate],
                &[gbm(candidate)],
            ),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn renderable_but_non_exportable_is_rejected_for_outputs() {
        let candidate = format(Fourcc::Xrgb8888, 9);

        assert_eq!(
            negotiate_output_formats(
                &[vulkan(candidate, true, true, false)],
                &[candidate],
                &[gbm(candidate)],
            ),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn exportable_but_non_scanout_is_rejected() {
        let candidate = format(Fourcc::Xrgb8888, 9);
        let mut gbm = gbm(candidate);
        gbm.scanout = false;

        assert_eq!(
            negotiate_output_formats(&[vulkan(candidate, true, true, true)], &[candidate], &[gbm],),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }

    #[test]
    fn client_import_and_output_export_are_distinct_roles() {
        let candidate = format(Fourcc::Xrgb8888, 9);
        let import_only = vulkan(candidate, true, true, false);
        let export_only = vulkan(candidate, true, false, true);

        assert!(import_only.supports_client_import());
        assert!(!import_only.supports_output_export());
        assert!(!export_only.supports_client_import());
        assert!(export_only.supports_output_export());
    }

    #[test]
    fn preference_order_is_deterministic_and_deduplicated() {
        let tiled = format(Fourcc::Xrgb8888, 9);
        let linear = DrmFormat {
            code: Fourcc::Xrgb8888,
            modifier: Modifier::Linear,
        };
        let alpha = format(Fourcc::Argb8888, 8);
        let vulkan = [
            vulkan(alpha, true, true, true),
            vulkan(linear, true, true, true),
            vulkan(tiled, true, true, true),
            vulkan(tiled, true, true, true),
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

    #[test]
    fn gbm_plane_topology_must_match_vulkan() {
        let candidate = format(Fourcc::Xrgb8888, 9);
        let mut gbm = gbm(candidate);
        gbm.plane_count = Some(2);

        assert_eq!(
            negotiate_output_formats(&[vulkan(candidate, true, true, true)], &[candidate], &[gbm],),
            Err(FormatNegotiationError::NoCompatibleOutputFormat)
        );
    }
}
