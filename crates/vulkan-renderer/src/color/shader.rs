use super::{
    ColorAlphaMode, ColorRange, ColorTransferFunction, ColorTransformPlan, GamutMap, PixelEncoding,
    ToneMap,
};

/// Declares which transfer steps have already been performed by typed Vulkan
/// image views. This keeps hardware sRGB conversion explicit and prevents a
/// managed shader from decoding or encoding the same transfer twice.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ColorShaderEncoding {
    pub source_view_decodes_transfer: bool,
    pub target_attachment_encodes_transfer: bool,
}

/// Fixed-width color payload appended to a product's geometry push data.
///
/// The first 16 bytes contain packed modes and scalar parameters. The three
/// float4 rows hold a linear-light RGB gamut matrix; their fourth components
/// carry source reference white, target reference white, and source peak
/// luminance. The ABI is deliberately 64 bytes so a compositor can keep its
/// existing 64-byte geometry record and use a 128-byte push range for
/// managed-color draws.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorTransformShaderData {
    pub modes: u32,
    pub source_power: f32,
    pub target_power: f32,
    pub tone_scale: f32,
    pub gamut_row_0: [f32; 4],
    pub gamut_row_1: [f32; 4],
    pub gamut_row_2: [f32; 4],
}

impl ColorTransformPlan {
    pub fn shader_data(self, encoding: ColorShaderEncoding) -> ColorTransformShaderData {
        let source_transfer = if encoding.source_view_decodes_transfer {
            TransferMode::Linear
        } else {
            TransferMode::from(self.source.volume.transfer_function)
        };
        let target_transfer = if encoding.target_attachment_encodes_transfer {
            TransferMode::Linear
        } else {
            TransferMode::from(self.target.volume.transfer_function)
        };
        let mut modes = source_transfer as u32 | (target_transfer as u32) << 4;
        modes |= (alpha_mode(self.source.alpha_mode) as u32) << 8;
        if matches!(
            self.source.encoding,
            PixelEncoding::Rgb {
                range: ColorRange::Limited
            }
        ) {
            modes |= 1 << 10;
        }
        if self.gamut_map != GamutMap::None {
            modes |= 1 << 11;
        }
        if self.tone_map != ToneMap::None {
            modes |= 1 << 12;
        }
        let matrix = self.gamut_matrix();
        ColorTransformShaderData {
            modes,
            source_power: transfer_power(self.source.volume.transfer_function),
            target_power: transfer_power(self.target.volume.transfer_function),
            tone_scale: match self.tone_map {
                ToneMap::None => 1.0,
                ToneMap::Bt2390 {
                    source_peak,
                    target_peak,
                } => target_peak as f32 / source_peak as f32,
            },
            gamut_row_0: [
                matrix[0][0],
                matrix[0][1],
                matrix[0][2],
                self.source.volume.reference_white as f32,
            ],
            gamut_row_1: [
                matrix[1][0],
                matrix[1][1],
                matrix[1][2],
                self.target.volume.reference_white as f32,
            ],
            gamut_row_2: [
                matrix[2][0],
                matrix[2][1],
                matrix[2][2],
                self.source.volume.max_luminance as f32,
            ],
        }
    }
}

#[repr(u32)]
enum TransferMode {
    Linear = 0,
    Srgb = 1,
    Bt1886 = 2,
    Gamma = 3,
    Pq = 4,
    Hlg = 5,
}

impl From<ColorTransferFunction> for TransferMode {
    fn from(value: ColorTransferFunction) -> Self {
        match value {
            ColorTransferFunction::Linear => Self::Linear,
            ColorTransferFunction::Srgb => Self::Srgb,
            ColorTransferFunction::Bt1886 => Self::Bt1886,
            ColorTransferFunction::Gamma22
            | ColorTransferFunction::Gamma28
            | ColorTransferFunction::Power { .. } => Self::Gamma,
            ColorTransferFunction::St2084Pq => Self::Pq,
            ColorTransferFunction::Hlg => Self::Hlg,
        }
    }
}

const fn alpha_mode(value: ColorAlphaMode) -> u8 {
    match value {
        ColorAlphaMode::PremultipliedElectrical => 0,
        ColorAlphaMode::PremultipliedOptical => 1,
        ColorAlphaMode::Straight => 2,
    }
}

const fn transfer_power(value: ColorTransferFunction) -> f32 {
    match value {
        ColorTransferFunction::Gamma22 => 2.2,
        ColorTransferFunction::Gamma28 => 2.8,
        ColorTransferFunction::Power { exponent_x10k } => exponent_x10k as f32 / 10_000.0,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use crate::{
        ColorPrimaries, ColorVolume, SourceColorDescriptor, TargetColorDescriptor, TextureFormat,
    };

    use super::*;

    #[test]
    fn shader_payload_is_exactly_one_sixty_four_byte_lane() {
        assert_eq!(mem::size_of::<ColorTransformShaderData>(), 64);
        assert_eq!(mem::align_of::<ColorTransformShaderData>(), 4);
    }

    #[test]
    fn hardware_srgb_ownership_removes_duplicate_transfer_steps() {
        let plan = ColorTransformPlan::build(
            SourceColorDescriptor::default(),
            TargetColorDescriptor {
                volume: ColorVolume {
                    primaries: ColorPrimaries::BT2020,
                    ..ColorVolume::SDR_SRGB
                },
                format: TextureFormat::Bgra8Srgb,
                hdr_metadata_supported: false,
            },
        )
        .unwrap();
        let data = plan.shader_data(ColorShaderEncoding {
            source_view_decodes_transfer: true,
            target_attachment_encodes_transfer: true,
        });

        assert_eq!(data.modes & 0xff, 0);
        assert_ne!(data.modes & (1 << 11), 0);
    }

    #[test]
    fn srgb_to_bt2020_matrix_matches_the_standard_conversion() {
        let plan = ColorTransformPlan::build(
            SourceColorDescriptor::default(),
            TargetColorDescriptor {
                volume: ColorVolume {
                    primaries: ColorPrimaries::BT2020,
                    ..ColorVolume::SDR_SRGB
                },
                format: TextureFormat::Rgba16Float,
                hdr_metadata_supported: false,
            },
        )
        .unwrap();
        let matrix = plan.gamut_matrix();

        let expected = [
            [0.627_404, 0.329_283, 0.043_313],
            [0.069_097, 0.919_54, 0.011_362],
            [0.016_391, 0.088_013, 0.895_595],
        ];
        for (actual, expected) in matrix
            .into_iter()
            .flatten()
            .zip(expected.into_iter().flatten())
        {
            assert!(
                (actual - expected).abs() < 0.000_01,
                "{actual} != {expected}"
            );
        }
    }
}
